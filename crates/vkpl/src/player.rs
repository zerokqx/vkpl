use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    process::{Child, Command},
    sync::mpsc,
};

#[derive(Debug)]
pub enum Event {
    Position(f64),
    Duration(f64),
    Paused(bool),
    Volume(f64),
    Loaded,
    End,
    Error(String),
}

pub struct Player {
    tx: mpsc::UnboundedSender<Value>,
    child: Child,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    _dir: tempfile::TempDir,
}

impl Player {
    pub async fn start(executable: &str) -> Result<(Self, mpsc::UnboundedReceiver<Event>)> {
        let dir = tempfile::Builder::new().prefix("vkpl-mpv-").tempdir()?;
        let socket = dir.path().join("ipc.sock");
        let mut child = Command::new(executable)
            .args([
                "--no-config",
                "--idle=yes",
                "--no-video",
                "--no-terminal",
                "--ytdl=no",
                "--volume=60",
                "--keep-open=no",
                "--audio-display=no",
            ])
            .arg(format!("--input-ipc-server={}", socket.display()))
            .arg("--referrer=https://vk.ru/")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("Не удалось запустить mpv. Установите mpv или задайте --mpv /путь/к/mpv")?;
        let stream = tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                if let Ok(stream) = UnixStream::connect(&socket).await {
                    return Ok(stream);
                }
                if child.try_wait()?.is_some() {
                    bail!("mpv завершился при запуске");
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .context("mpv не создал IPC сокет")??;
        let (read, mut write) = stream.into_split();
        let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
        let (events, event_rx) = mpsc::unbounded_channel();
        let errors = events.clone();
        let writer = tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                let mut bytes = command.to_string().into_bytes();
                bytes.push(b'\n');
                if write.write_all(&bytes).await.is_err() {
                    let _ = errors.send(Event::Error("Соединение с mpv потеряно".into()));
                    break;
                }
            }
        });
        let reader = tokio::spawn(async move {
            let mut lines = BufReader::new(read).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                let event = match value["event"].as_str() {
                    Some("property-change") => match value["name"].as_str() {
                        Some("time-pos") => value["data"].as_f64().map(Event::Position),
                        Some("duration") => value["data"].as_f64().map(Event::Duration),
                        Some("pause") => value["data"].as_bool().map(Event::Paused),
                        Some("volume") => value["data"].as_f64().map(Event::Volume), _ => None,
                    },
                    Some("file-loaded") => Some(Event::Loaded),
                    Some("end-file") => match value["reason"].as_str() {
                        Some("eof") => Some(Event::End),
                        Some("error") => Some(Event::Error("mpv не смог воспроизвести трек; возможно, ссылка истекла или запись недоступна".into())),
                        _ => None,
                    },
                    _ => {
                        if value["error"].as_str().is_some_and(|e| e != "success") {
                            Some(Event::Error("mpv отклонил команду".into()))
                        } else { None }
                    },
                };
                if let Some(event) = event {
                    let _ = events.send(event);
                }
            }
        });
        let player = Self {
            tx,
            child,
            tasks: vec![writer, reader],
            _dir: dir,
        };
        for (id, name) in ["time-pos", "duration", "pause", "volume"]
            .iter()
            .enumerate()
        {
            player.command(json!(["observe_property", id, name]));
        }
        Ok((player, event_rx))
    }
    pub fn command(&self, command: Value) {
        let _ = self.tx.send(json!({"command": command}));
    }
    pub fn play(&self, url: &str) {
        self.command(json!(["loadfile", url, "replace"]));
        self.command(json!(["set_property", "pause", false]));
    }
    pub fn set_paused(&self, paused: bool) {
        self.command(json!(["set_property", "pause", paused]));
    }
    pub fn set_position(&self, seconds: f64) {
        self.command(json!(["seek", seconds, "absolute"]));
    }
    pub fn volume(&self, volume: f64) {
        self.command(json!(["set_property", "volume", volume.clamp(0.0, 100.0)]));
    }
    pub fn stop(&self) {
        self.command(json!(["stop"]));
    }
    pub async fn shutdown(&mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
        let _ = self.child.start_kill();
    }
}
