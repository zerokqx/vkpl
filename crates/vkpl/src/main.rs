mod actions;
mod app;
mod keymaps;
mod login;
mod mpris;
mod player;
mod storage;
mod ui;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use vk_music::Session;

#[derive(Parser)]
#[command(
    version,
    about = "VK Music в терминале",
    long_about = "VK Music в терминале. Без подкоманды запускается TUI. Для входа: vkpl login browser."
)]
struct Args {
    #[arg(long, global = true)]
    config_dir: Option<PathBuf>,
    #[arg(long, default_value = "chromium", global = true)]
    browser: String,
    #[arg(long, default_value = "mpv", global = true)]
    mpv: String,
    #[arg(long, default_value = "ffmpeg", global = true)]
    ffmpeg: String,
    #[command(subcommand)]
    command: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    /// Войти через браузер, cookies или токен
    Login {
        #[command(subcommand)]
        method: Login,
    },
    /// Удалить локальную сессию (браузерная сессия VK не затрагивается)
    Logout,
    /// Проверить установленный плеер и настройки
    Doctor,
}
#[derive(Subcommand)]
enum Login {
    /// Открыть отдельный Chromium: QR / телефон / пароль / VK ID
    Browser,
    /// Подключиться к уже открытому Chrome 144+ через chrome://inspect
    Existing {
        /// Каталог профиля браузера; по умолчанию ~/.config/chromium
        #[arg(long)]
        profile: Option<PathBuf>,
    },
    /// Прочитать cookies из явно выбранного локального CDP браузера
    Cdp {
        #[arg(long, default_value_t = 9222)]
        port: u16,
    },
    /// Импортировать Netscape cookies.txt или JSON cookie export
    Cookies { file: PathBuf },
    /// Ввести токен скрыто или передать через stdin
    Token,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = args
        .config_dir
        .map(Ok)
        .unwrap_or_else(storage::config_dir)?;
    match args.command {
        Some(Action::Login { method }) => {
            let task = async {
                let session = match method {
                    Login::Browser => {
                        eprintln!(
                            "Войдите в открывшемся браузере через VK ID. Ожидание — 5 минут; Ctrl+C отменяет вход."
                        );
                        login::browser(&args.browser).await?
                    }
                    Login::Cdp { port } => login::cdp(port).await?,
                    Login::Existing { profile } => {
                        let profile = profile.unwrap_or(
                            dirs::config_dir()
                                .context("Нет каталога настроек")?
                                .join("chromium"),
                        );
                        eprintln!("Подтвердите подключение в браузере, если появится запрос.");
                        login::existing(&profile).await?
                    }
                    Login::Cookies { file } => login::import(&file).await?,
                    Login::Token => login::token_stdin()?,
                };
                let (_, name) = app::authenticate(session, config).await?;
                println!("Вход выполнен: {name}. Запустите vkpl.");
                Ok::<_, anyhow::Error>(())
            };
            tokio::select! { result = task => result?, _ = tokio::signal::ctrl_c() => eprintln!("Вход отменён") }
        }
        Some(Action::Logout) => match std::fs::remove_file(config.join("session.json")) {
            Ok(()) => println!("Локальная сессия удалена"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                println!("Сессия отсутствует")
            }
            Err(error) => return Err(error.into()),
        },
        Some(Action::Doctor) => {
            let output = tokio::process::Command::new(&args.mpv)
                .arg("--version")
                .output()
                .await
                .context("mpv не найден")?;
            if !output.status.success() {
                anyhow::bail!("mpv --version завершился с ошибкой");
            }
            println!(
                "{}",
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("mpv найден")
            );
            let ffmpeg = tokio::process::Command::new(&args.ffmpeg)
                .arg("-version")
                .output()
                .await;
            println!(
                "Предзагрузка FFmpeg: {}",
                if ffmpeg.is_ok_and(|out| out.status.success()) {
                    "доступна"
                } else {
                    "недоступна (останется подготовка ссылок)"
                }
            );
            println!("Настройки: {}", config.display());
            if config.join("session.json").exists() {
                let session: Session = storage::read(&config.join("session.json"))?;
                println!(
                    "Сессия: есть, обновление токена: {}",
                    if session.can_refresh() {
                        "да"
                    } else {
                        "нет"
                    }
                );
            } else {
                println!("Сессия: нет. Выполните vkpl login browser");
            }
        }
        None => app::run(config, args.browser, args.mpv, args.ffmpeg).await?,
    }
    Ok(())
}
