use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    process::Command,
    time::{Instant, sleep, timeout},
};
use vk_music::{Client, Cookie, Session};

pub async fn browser(executable: &str) -> Result<Session> {
    // A fresh, private profile avoids touching or decrypting the user's browser databases.
    let profile = tempfile::Builder::new().prefix("vkpl-login-").tempdir()?;
    let mut child = Command::new(executable)
        .arg(format!("--user-data-dir={}", profile.path().display()))
        .args([
            "--remote-debugging-address=127.0.0.1",
            "--remote-debugging-port=0",
            "--no-first-run",
            "--no-default-browser-check",
            "https://vk.ru/music",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| {
            format!("Не удалось запустить {executable}; используйте --browser /путь/к/chromium")
        })?;
    let result = async {
        let deadline = Instant::now() + Duration::from_secs(300);
        let port = loop {
            if child.try_wait()?.is_some() {
                bail!("Браузер закрылся до завершения входа");
            }
            if Instant::now() >= deadline {
                bail!("Время ожидания браузера истекло");
            }
            if let Ok(text) =
                tokio::fs::read_to_string(profile.path().join("DevToolsActivePort")).await
                && let Some(port) = text.lines().next().and_then(|v| v.parse::<u16>().ok())
            {
                break port;
            }
            sleep(Duration::from_millis(250)).await;
        };
        capture(port, deadline).await
    }
    .await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

/// Import a session from an explicitly selected, local CDP browser.
pub async fn cdp(port: u16) -> Result<Session> {
    capture(port, Instant::now() + Duration::from_secs(300)).await
}

/// Chrome 144+ exposes an opt-in debugging socket without /json/version.
pub async fn existing(profile: &Path) -> Result<Session> {
    let text = tokio::fs::read_to_string(profile.join("DevToolsActivePort"))
        .await
        .context(
            "Включите отладку в chrome://inspect/#remote-debugging и подтвердите подключение",
        )?;
    let endpoint = active_endpoint(&text)?;
    capture_endpoint(&endpoint, Instant::now() + Duration::from_secs(300)).await
}

fn active_endpoint(text: &str) -> Result<String> {
    let mut lines = text.lines();
    let port: u16 = lines.next().context("Нет порта CDP")?.trim().parse()?;
    let path = lines.next().context("Нет адреса CDP")?.trim();
    if port == 0 || !path.starts_with("/devtools/browser/") || path.chars().any(char::is_whitespace)
    {
        bail!("Неверный DevToolsActivePort");
    }
    Ok(format!("ws://127.0.0.1:{port}{path}"))
}

async fn capture(port: u16, deadline: Instant) -> Result<Session> {
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()?;
    let info: Value = http
        .get(format!("http://127.0.0.1:{port}/json/version"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let endpoint = info["webSocketDebuggerUrl"]
        .as_str()
        .context("Нет CDP WebSocket")?;
    capture_endpoint(endpoint, deadline).await
}

async fn capture_endpoint(endpoint: &str, deadline: Instant) -> Result<Session> {
    let url = url::Url::parse(endpoint)?;
    if url.scheme() != "ws" || !matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
    {
        bail!("Разрешён только локальный CDP браузер");
    }
    let (mut ws, _) = timeout(
        Duration::from_secs(90),
        tokio_tungstenite::connect_async(endpoint),
    )
    .await??;
    let mut id = 0;
    while Instant::now() < deadline {
        id += 1;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            json!({"id":id,"method":"Storage.getCookies"})
                .to_string()
                .into(),
        ))
        .await?;
        let value = timeout(Duration::from_secs(10), async {
            while let Some(message) = ws.next().await {
                let message = message?;
                if !message.is_text() {
                    continue;
                }
                let value: Value = serde_json::from_str(message.to_text()?)?;
                if value["id"].as_i64() == Some(id) {
                    return Ok::<_, anyhow::Error>(value);
                }
            }
            bail!("Браузер отключился");
        })
        .await??;
        if value.get("error").is_some() {
            bail!("CDP отказал в чтении cookies");
        }
        let cookies: Vec<Cookie> = serde_json::from_value(value["result"]["cookies"].clone())?;
        if let Ok(session) = Session::cookies(cookies) {
            let client = Client::new(session)?;
            match client.me().await {
                Ok(_) => {
                    let _ = ws.close(None).await;
                    return Ok(client.session().await);
                }
                Err(vk_music::Error::Unauthorized) => {}
                Err(error) => return Err(error.into()),
            }
        }
        sleep(Duration::from_secs(2)).await;
    }
    bail!("Вход не завершён за 5 минут")
}

pub async fn import(path: &Path) -> Result<Session> {
    let text = tokio::fs::read_to_string(path)
        .await
        .context("Не удалось прочитать cookies")?;
    Ok(Session::import(&text)?)
}

pub fn token_stdin() -> Result<Session> {
    use std::io::{self, IsTerminal};
    let interactive = io::stdin().is_terminal();
    if !interactive {
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        return Ok(Session::token(line)?);
    }
    eprintln!("Вставьте токен VK и нажмите Enter (ввод скрыт):");
    crossterm::terminal::enable_raw_mode()?;
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
    let _restore = Restore;
    let mut text = String::new();
    loop {
        if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
            use crossterm::event::{KeyCode, KeyModifiers};
            match key.code {
                KeyCode::Enter => break,
                KeyCode::Esc => bail!("Вход отменён"),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    bail!("Вход отменён")
                }
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Char(ch) => text.push(ch),
                _ => {}
            }
        }
    }
    Ok(Session::token(text)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opt_in_debugging_endpoint_is_local_and_validated() {
        assert_eq!(
            active_endpoint("9222\n/devtools/browser/example\n").unwrap(),
            "ws://127.0.0.1:9222/devtools/browser/example"
        );
        assert!(active_endpoint("0\n/devtools/browser/example").is_err());
        assert!(active_endpoint("9222\nhttps://example.org").is_err());
        assert!(active_endpoint("9222\n/devtools/browser/a b").is_err());
    }
}
