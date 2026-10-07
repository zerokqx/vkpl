# VKPL

VK Music в терминале: Rust, Ratatui, асинхронный SDK и воспроизведение через MPV. Интерфейс на русском, управление с клавиатуры, отдельные экраны библиотеки, поиска, плейлистов, очереди и текущего трека.

Интерфейс использует фон и цвет текста текущей темы терминала. Выделение строк строится на инверсии цветов, а акценты — на начертании текста.

## Запуск

Нужны Linux или macOS, Rust 1.88+ и `mpv`. Для буферизации следующих треков нужен `ffmpeg`. Для браузерного входа нужен Chromium или Google Chrome. На NixOS можно использовать `devenv shell`.

```sh
cargo build --release --locked
./target/release/vkpl login browser
./target/release/vkpl
```

Или установите бинарник через `cargo install --path crates/vkpl --locked`. При первом запуске без сессии TUI откроет экран входа. Настройки хранятся в `$XDG_CONFIG_HOME/vkpl` (обычно `~/.config/vkpl`); путь можно изменить через `--config-dir`.

```sh
vkpl doctor
vkpl --browser google-chrome login browser
vkpl --mpv /путь/к/mpv --ffmpeg /путь/к/ffmpeg
```

## Nix

`flake.nix` собирает VKPL для `x86_64-linux`, `aarch64-linux`, `x86_64-darwin` и `aarch64-darwin`.

```sh
nix run .
nix build .#vkpl
nix profile install .#vkpl
```

Атрибуты сборок для CI и удалённых билд-машин:

```sh
nix build .#packages.x86_64-linux.vkpl
nix build .#packages.aarch64-linux.vkpl
nix build .#packages.x86_64-darwin.vkpl
nix build .#packages.aarch64-darwin.vkpl
```

Пакет оборачивает `mpv` и `ffmpeg`, поэтому они доступны приложению автоматически. Для входа через браузер на Linux dev shell также содержит Chromium; на macOS передайте установленный браузер через `--browser`.

Для разработки выполните `direnv allow`: [.envrc](.envrc) запустит `devenv shell`. Внутри доступны `build`, `check`, `lint`, `fmt`, `test`, `run-app`, `run-gui`, `build-nix`, `run-nix` и `flake-check`.

## Wayland overlay

`vkpl-gui` — Iced-клиент, создающий верхний Wayland layer-shell. Он открывается командой `run-gui` и требует композитор с протоколом `wlr-layer-shell` (Hyprland, Sway, River и совместимые).

## GitHub Releases

Тег вида `v0.1.0` запускает [release workflow](.github/workflows/release.yml) и создаёт GitHub Release с нативными архивами:

- `vkpl-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`
- `vkpl-v0.1.0-aarch64-unknown-linux-gnu.tar.gz`
- `vkpl-v0.1.0-x86_64-apple-darwin.tar.gz`
- `vkpl-v0.1.0-aarch64-apple-darwin.tar.gz`

Внутри каждого архива лежит исполняемый файл `vkpl`; рядом публикуется соответствующий `.sha256`. Чтобы собрать архив на своей машине, выполните `VERSION=v0.1.0 package-local` в devenv shell. Для запуска распакованного бинарника требуются `mpv` и `ffmpeg` в `PATH`.

## Способы входа

| Команда | Что происходит |
| --- | --- |
| `vkpl login browser` | Открывается отдельный временный профиль Chromium. Вы входите на настоящем сайте VK через QR, телефон, пароль или другой предлагаемый VK ID способ. |
| `vkpl login existing` | Подключение к уже открытому Chromium через переключатель в `chrome://inspect/#remote-debugging`; запрос подтверждается в браузере. |
| `vkpl login existing --profile ~/.config/google-chrome` | То же для выбранного каталога профиля Chrome. |
| `vkpl login cdp --port 9222` | Импорт из локального браузера, запущенного с обычным `--remote-debugging-port`. |
| `vkpl login cookies /путь/cookies.txt` | Импорт Netscape cookies.txt или JSON-массива cookies. |
| `vkpl login token` | Скрытый ввод готового токена; также поддерживается stdin. |
| `vkpl logout` | Удаление локальной сессии приложения. |

`login existing` подходит для нового режима отладки Chrome 144+, где `/json/version` может отсутствовать. `login cdp` предназначен для обычного CDP HTTP endpoint.

Веб-сессии обновляют API-токен автоматически. Для токена без cookies автоматическое обновление недоступно. Не каждый токен VK имеет доступ к музыкальным методам: приложение показывает отказ API, если прав недостаточно. CAPTCHA, подтверждение входа и двухфакторная проверка проходят в браузере. Приложение не хранит пароль VK.

Сессия сохраняется атомарно в `session.json` с правами `0600`, каталог — `0700`. Это обычный локальный файл с секретами, без шифрования. В очереди не сохраняются временные URL аудио. `logout` удаляет локальные учётные данные, но не отзывает сессию VK на сервере.

## Управление

### Lua-бинды

`~/.config/vkpl/keymaps.lua` создаётся при первом запуске (при заданном `XDG_CONFIG_HOME` — в его каталоге `vkpl`; `--config-dir` меняет путь). Готовый образец: [`config/keymaps.lua`](config/keymaps.lua). `directories` определяет системный каталог настроек. Это единственный источник normal-mode и input-mode биндов: удалённая из файла клавиша не имеет действия. Изменения вступают в силу после перезапуска или команды `:source` в TUI.

```lua
vim.g.mapleader = " "
vim.keymap.set("n", "<leader>n", ":next<CR>")
vim.keymap.set("n", "<leader>m", ":mixes<CR>")
vim.keymap.set("n", "p", ":play-pause<CR>")
```

Поддерживаются normal mode (`"n"`) и input mode (`"i"`), отдельные клавиши, цепочки, `<leader>`, `<Space>`, `<C-x>`, `<A-x>`, `<S-x>`, стрелки, `Tab`, `Enter`, `Esc`, `Backspace`, `Delete`, `Home`, `End`, `PageUp`, `PageDown`. Правая часть — последовательность клавиш либо команда `:имя<CR>`. Lua исполняется при загрузке конфига, поэтому используйте только доверенный файл. Ошибка в конфиге выводится с путём и строкой; при `:source` старые бинды сохраняются до успешной загрузки.

| Клавиша | Действие |
| --- | --- |
| `h/j/k/l`, `↑/↓`, `Enter` | Назад или меню / вниз / вверх / открыть выбранное |
| `10j`, `5k`, `gg`, `G`, `25G` | Перемещение с числом, начало/конец загруженного списка, строка 25 |
| `Ctrl+d/u`, `Ctrl+f/b`, `PgDn/PgUp` | Полэкрана / экран; размер зависит от терминала |
| `gH/M/gL`, `zt/zz/zb` | Верх/середина/низ видимого списка; поставить выбранную строку сверху/в центре/снизу |
| `Ctrl+e/y` | Прокрутка списка с сохранением видимости выбранной строки |
| `Tab`, `Ctrl+w w`, `Ctrl+w h/l`, `Ctrl+h/l` | Переключение и выбор панели |
| `gt/gT`, `g1` … `g9`, `Alt+1` … `Alt+9`, `3gt` | Следующий/предыдущий раздел, раздел по номеру |
| `/`, `f`, `n/N` | Поиск в VK; локальный поиск по загруженным строкам; следующие/предыдущие совпадения |
| `ga` / `A`, `o` | Исполнитель выбранного трека; переключить треки/релизы исполнителя |
| `Enter` на треке | Играть; загруженный список становится очередью |
| `a` / `yy`, `3yy`, `dd` / `Delete`, `3dd` | Добавить треки в очередь; удалить строки из очереди |
| `p`, `H` / `L`, `[` / `]`, `←` / `→` | Пауза, предыдущий/следующий трек, перемотка на 5 секунд |
| `+` / `−`, `s`, `r` | Громкость, случайный порядок, повтор |
| `v`, `Y` | Экран текущего трека, текст VK; прокрутка через `j/k`, `gg/G`, страницы |
| `:`, `?`, `Esc`, `q`, `ZZ`, `ZQ` | Командная строка, справка, отмена/назад, выход с сохранением |

Одиночные цифры теперь начинают числовой префикс Vim. Для быстрого перехода к разделу используйте `g` + цифру или `Alt` + цифру. `z` начинает `zt/zz/zb`; экран трека перенесён на `v`, текст песни — на `Y`, предыдущий/следующий трек — на `H` / `L` (также `[` / `]`). Незавершённая команда видна рядом с `NORMAL`; `Esc` её сбрасывает.

В строке ввода работают стрелки, `Home/End`, `Ctrl+a/e`, `Ctrl+w` (удалить слово), `Ctrl+u` (удалить до начала), `Esc` / `Ctrl+c` (отмена). Текст редактируется с учётом UTF-8. Справка тоже прокручивается.

Команды: `:search Кино`, `:artists Кино`, `:home`, `:library`, `:playlists`, `:queue`, `:account`, `:mixes`, `:help`, `:play`, `:pause`, `:stop`, `:next`, `:prev`, `:reload`, `:25`, `:q`, `:wq`. Все варианты выхода сохраняют локальную очередь.

## Исполнители и миксы

- **Исполнители** (`g8`): `/` ищет музыкальных исполнителей в отдельной вкладке. `Enter` открывает треки исполнителя из каталога и результатов поиска VK, с подгрузкой следующих страниц; `o` — релизы и участие в релизах. Выберите релиз и нажмите `Enter`, чтобы открыть его треки. `h` возвращает к родительскому экрану. Для перехода от трека к его исполнителю используйте `ga` или `A`; при нескольких основных исполнителях откроется первый.
- **Миксы** (`g9`): персональный VK Микс и плейлисты из полки VK «Собрано алгоритмами». `Enter` открывает микс, повторный `Enter` на треке запускает его. VK Микс заранее получает следующие порции музыки и продолжает пополнять очередь независимо от открытой вкладки. Если VK не вернул новых доступных треков, `]` повторяет запрос.
- Состав миксов зависит от аккаунта. Восстанавливается сохранённая очередь; непрерывный поток микса после перезапуска нужно выбрать заново.

## Предзагрузка очереди

VKPL готовит три следующих доступных трека: обновляет их медиассылки и через `ffmpeg` загружает HLS в отдельные временные файлы без перекодирования. При переключении MPV открывает готовый локальный файл. Число готовых треков видно как «Буфер» в строке плеера. Временные файлы удаляются при смене очереди и выходе. Подготовка начинается после загрузки текущего трека, чтобы не задерживать его старт; если следующий файл ещё не готов, используется обновлённая ссылка или обычная загрузка.

При отсутствии FFmpeg приложение продолжает работать и заранее обновляет ссылки, но сами аудиоданные не буферизует. `vkpl doctor` показывает доступность FFmpeg. Путь можно задать через `--ffmpeg`. Предзагрузка занимает сеть и временное место на диске примерно для трёх треков.

## playerctl / MPRIS

При запуске TUI регистрируется `org.mpris.MediaPlayer2.vkpl` в пользовательской D-Bus сессии. Плеер доступен для `playerctl` и системных медиаклавиш:

```sh
playerctl -p vkpl play
playerctl -p vkpl pause
playerctl -p vkpl play-pause
playerctl -p vkpl stop
playerctl -p vkpl next
playerctl -p vkpl previous
playerctl -p vkpl position 10+
playerctl -p vkpl volume 0.5
playerctl -p vkpl loop Playlist
playerctl -p vkpl shuffle On
playerctl -p vkpl metadata
playerctl -p vkpl --follow status
```

`stop` останавливает воспроизведение и отменяет подготовку трека; последующий `play` начинает этот трек сначала. `pause` сохраняет позицию. MPRIS передаёт название, исполнителя, альбом, длительность, позицию, громкость и режимы очереди; изменения публикуются через D-Bus. Без D-Bus или при занятом имени TUI продолжает работать и показывает сообщение. Одновременно регистрируется один экземпляр `vkpl`.

Списки подгружают следующую страницу при приближении к концу. Записи с ограничением VK помечены `×` и не попадают в создаваемую очередь. Очередь, громкость, режим повтора и случайного порядка восстанавливаются после выхода; звук сам не запускается. Ошибка загрузки или воспроизведения отображается в строке состояния. Сетевые запросы не блокируют навигацию; устаревшие результаты поиска отбрасываются.

## Rust SDK

`crates/vk-music` не зависит от TUI и MPV. Можно использовать в другом Rust-приложении:

```toml
[dependencies]
vk-music = { path = "../vkpl/crates/vk-music" }
tokio = { version = "1", features = ["full"] }
```

```rust,no_run
use vk_music::{Client, Session};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let cookies = std::fs::read_to_string("cookies.txt")?;
let client = Client::new(Session::import(&cookies)?)?;
let me = client.me().await?;
let page = client.search("Кино", 0, 50).await?;

if let Some(track) = page.items.first() {
    let playable = client.resolve(track).await?;
    // playable.url — актуальная ссылка MP3 или HLS для плеера.
}
if let Some(offset) = page.next_offset() {
    let next_page = client.search("Кино", offset, 50).await?;
}

let playlists = client.playlists(me.id, 0, 50).await?;
if let Some(playlist) = playlists.items.first() {
    let tracks = client.playlist_tracks(playlist, 0, 100).await?;
}
# Ok(())
# }
```

Доступны `me`, `search`, `library`, `playlists`, `playlist_tracks`, `catalog`, `catalog_search`, `section`, `resolve`, `lyrics`, `search_artists`, `artist`, `artist_tracks`, `block`, `mixes`, `stream_mix`, а также типизированный `request::<T>` для других методов. `Client::builder` позволяет задать API version, app ID, User-Agent, таймаут и интервал запросов. Клоны клиента разделяют соединения, ограничитель запросов и блокировку обновления токена.

```rust,no_run
# async fn explore(client: vk_music::Client) -> vk_music::Result<()> {
let artists = client.search_artists("Кино", 0, 50).await?;
if let Some(artist) = artists.items.first() {
    let catalog = client.artist(&artist.id).await?;
    // catalog.artists — профиль, audios — треки, playlists — релизы и подборки.
    // catalog.catalog.sections[].blocks — связи и курсоры отдельных полок.
    if let Some(block) = catalog.catalog.sections.iter()
        .flat_map(|section| &section.blocks)
        .find(|block| block.data_type == "music_audios")
    {
        if let Some(cursor) = &block.next_from {
            let next = client.block(&block.id, Some(cursor), 100).await?;
        }
    }
}
for mix in client.mixes().await? {
    match mix {
        vk_music::Mix::Stream(mix) => {
            let batch = client.stream_mix(&mix.stream_mix.id).await?;
            // Следующий вызов получает следующую порцию; offset не нужен.
        }
        vk_music::Mix::Playlist(playlist) => {
            let page = client.playlist_tracks(&playlist, 0, 100).await?;
        }
    }
}
# Ok(())
# }
```

Для автоматической пагинации используйте `search_pages`, `library_pages` или `playlist_pages`:

```rust,ignore
let mut pages = client.library_pages(me.id).page_size(100);
while let Some(page) = pages.next_page().await? {
    for track in page.items { /* обработать трек */ }
}
```

Следующее смещение учитывает пропущенные сервером записи. После ошибки запрос страницы можно повторить: курсор продвигается только при успехе.

Ошибки различают истёкшую сессию, отсутствие прав, ограничение запросов, CAPTCHA/проверку, недоступный трек и изменение схемы ответа. Токены передаются в теле POST. Повторы ограничены известными операциями чтения; произвольные методы не повторяются автоматически. Пример: `cargo run -p vk-music --example search -- /путь/session.json Кино`. Живая проверка чтения API и медиаресурса: `cargo run -p vk-music --example inspect -- /путь/session.json`.

## Устройство и проверка

```text
crates/vk-music/    Авторизация, HTTP-клиент, модели, ошибки, тесты протокола
crates/vkpl/        CLI, TUI, браузерный вход, MPV IPC, локальное состояние
docs/api.md        Происхождение и статус проверки протокола
```

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Тесты SDK используют локальный HTTP-сервер и не обращаются к VK. Требуется возможность слушать `127.0.0.1`. Живой доступ зависит от сессии, прав приложения, доступности трека и изменений приватного API. Статус исследования и оставшиеся ограничения записаны в [docs/api.md](docs/api.md).
