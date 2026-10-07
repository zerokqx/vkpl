use crate::{
    actions::{self, Action},
    keymaps::{Keymaps, Mapping, Match as KeyMatch},
    login, mpris,
    player::{self, Player},
    storage, ui,
};
use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::process::Command;
use tokio::sync::mpsc;
use vk_music::{Artist, Catalog, Client, Mix, Page, Playlist, Session, Track, TrackId};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Home,
    Library,
    Playlists,
    Search,
    Queue,
    Account,
    Help,
    Playlist,
    Artists,
    Artist,
    ArtistAlbums,
    Mixes,
    Mix,
}
impl View {
    pub fn title(self) -> &'static str {
        match self {
            Self::Home => "Для вас",
            Self::Library => "Моя музыка",
            Self::Playlists => "Плейлисты",
            Self::Search => "Поиск",
            Self::Queue => "Очередь",
            Self::Account => "Аккаунт",
            Self::Help => "Управление",
            Self::Playlist => "Плейлист",
            Self::Artists => "Исполнители",
            Self::Artist => "Исполнитель",
            Self::ArtistAlbums => "Релизы",
            Self::Mixes => "Миксы",
            Self::Mix => "Микс",
        }
    }
}
pub const NAV: [View; 9] = [
    View::Home,
    View::Library,
    View::Playlists,
    View::Search,
    View::Queue,
    View::Account,
    View::Help,
    View::Artists,
    View::Mixes,
];

#[derive(Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}
impl Repeat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "выкл",
            Self::All => "всё",
            Self::One => "один",
        }
    }
    fn cycle(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Saved {
    #[serde(default)]
    pub queue: Vec<Track>,
    #[serde(default)]
    pub current: Option<usize>,
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default)]
    pub repeat: Repeat,
    #[serde(default = "default_volume")]
    pub volume: f64,
}
fn default_volume() -> f64 {
    60.0
}
impl Default for Saved {
    fn default() -> Self {
        Self {
            queue: vec![],
            current: None,
            shuffle: false,
            repeat: Repeat::Off,
            volume: default_volume(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Search,
    Cookies,
    Token,
    Artists,
    Find,
    Command,
}

enum Message {
    Artists(u64, std::result::Result<Page<Artist>, String>, bool),
    ArtistCatalog(u64, std::result::Result<Catalog, String>),
    ArtistSearch(u64, std::result::Result<Page<Track>, String>),
    PrefetchUrl(u64, u64, TrackId, Track),
    PrefetchDone(u64, u64, TrackId, Option<PathBuf>),
    ArtistReleases(u64, std::result::Result<vk_music::BlockPage, String>),
    Mixes(u64, std::result::Result<Vec<Mix>, String>),
    Stream(u64, std::result::Result<Vec<Track>, String>),
    Tracks(u64, std::result::Result<Page<Track>, String>, bool),
    Playlists(u64, std::result::Result<Page<Playlist>, String>, bool),
    Resolved(u64, std::result::Result<Track, String>),
    Login(std::result::Result<(Client, String), String>),
    Lyrics(u64, std::result::Result<String, String>),
}

pub struct App {
    pub view: View,
    pub nav: usize,
    pub sidebar: bool,
    pub selected: usize,
    pub offset: usize,
    pub viewport: usize,
    pub text_lines: usize,
    pub text_scroll: usize,
    keymaps: Keymaps,
    pub artists: Vec<Artist>,
    pub artist: Option<Artist>,
    pub artist_query: String,
    pub artist_albums: Vec<Playlist>,
    artist_search_next: Option<u64>,
    releases_block: Option<String>,
    releases_cursor: Option<String>,
    pub mixes: Vec<Mix>,
    pub mix: Option<Mix>,
    playlist_parent: View,
    queue_stream: Option<String>,
    stream_generation: u64,
    stream_task: Option<tokio::task::JoinHandle<()>>,
    stream_waiting: bool,
    pub find_query: String,

    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub playlist: Option<Playlist>,
    pub total: u64,
    pub next_offset: Option<u64>,
    pub loading: bool,
    pub input: Option<Input>,
    pub input_text: String,
    pub input_cursor: usize,
    pub query: String,
    pub status: String,
    pub error: bool,
    pub account: String,
    pub login_busy: bool,
    pub saved: Saved,
    prefetch_dir: Option<tempfile::TempDir>,
    prefetch_generation: u64,
    prefetch_job: u64,
    prefetch_tasks: HashMap<TrackId, (u64, tokio::task::JoinHandle<()>)>,
    prefetch_urls: HashMap<TrackId, (Track, Instant)>,
    prefetch_files: HashMap<TrackId, PathBuf>,
    playing_file: Option<PathBuf>,
    pub playing: Option<Track>,
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
    pub buffering: bool,
    pub zen: bool,
    pub lyrics: Option<String>,
    ended: bool,
    pause_on_load: bool,
    seek_serial: u64,
    pub tick: usize,
    pub connected: bool,
    client: Option<Client>,
    user_id: i64,
    generation: u64,
    play_generation: u64,
    tx: mpsc::UnboundedSender<Message>,
    fetch_task: Option<tokio::task::JoinHandle<()>>,
    resolve_task: Option<tokio::task::JoinHandle<()>>,
    login_task: Option<tokio::task::JoinHandle<()>>,
    browser: String,
    ffmpeg: String,
    config: PathBuf,
}

impl App {
    fn new(
        saved: Saved,
        config: PathBuf,
        browser: String,
        ffmpeg: String,
        tx: mpsc::UnboundedSender<Message>,
    ) -> Self {
        Self {
            view: View::Home,
            nav: 0,
            sidebar: false,
            selected: 0,
            offset: 0,
            viewport: 10,
            text_lines: 0,
            text_scroll: 0,
            keymaps: Keymaps::default(),
            artists: vec![],
            artist: None,
            artist_query: String::new(),
            artist_albums: vec![],
            artist_search_next: None,
            releases_block: None,
            releases_cursor: None,
            mixes: vec![],
            mix: None,
            playlist_parent: View::Playlists,
            queue_stream: None,
            stream_generation: 0,
            stream_task: None,
            stream_waiting: false,
            find_query: String::new(),
            tracks: vec![],
            playlists: vec![],
            playlist: None,
            total: 0,
            next_offset: None,
            loading: false,
            input: None,
            input_text: String::new(),
            input_cursor: 0,
            query: String::new(),
            status: "Добро пожаловать в VKPL".into(),
            error: false,
            account: "Вход не выполнен".into(),
            login_busy: false,
            saved,
            prefetch_dir: tempfile::Builder::new()
                .prefix("vkpl-buffer-")
                .tempdir()
                .ok(),
            prefetch_generation: 0,
            prefetch_job: 0,
            prefetch_tasks: HashMap::new(),
            prefetch_urls: HashMap::new(),
            prefetch_files: HashMap::new(),
            playing_file: None,
            playing: None,
            position: 0.0,
            duration: 0.0,
            paused: true,
            buffering: false,
            zen: false,
            lyrics: None,
            tick: 0,
            connected: false,
            client: None,
            user_id: 0,
            generation: 0,
            play_generation: 0,
            ended: false,
            pause_on_load: false,
            seek_serial: 0,
            tx,
            fetch_task: None,
            resolve_task: None,
            login_task: None,
            browser,
            ffmpeg,
            config,
        }
    }
    pub fn count(&self) -> usize {
        match self.view {
            View::Playlists => self.playlists.len(),
            View::Artists => self.artists.len(),
            View::ArtistAlbums => self.artist_albums.len(),
            View::Mixes => self.mixes.len(),
            View::Queue => self.saved.queue.len(),
            View::Account | View::Help => 0,
            _ => self.tracks.len(),
        }
    }
    pub fn visible_tracks(&self) -> &[Track] {
        if self.view == View::Queue {
            &self.saved.queue
        } else {
            &self.tracks
        }
    }
    pub fn pending_keys(&self) -> String {
        self.keymaps.pending()
    }
    fn notify(&mut self, message: impl Into<String>, error: bool) {
        self.status = message.into();
        self.error = error;
    }
    fn change_view(&mut self, view: View) {
        self.view = view;
        self.sidebar = false;
        self.selected = 0;
        self.offset = 0;
        self.text_scroll = 0;
        self.zen = false;
        self.next_offset = None;
        self.generation += 1;
        if let Some(task) = self.fetch_task.take() {
            task.abort();
        }
        self.loading = false;
        self.tracks.clear();
        self.playlists.clear();
        let nav_view = match view {
            View::Artist | View::ArtistAlbums => View::Artists,
            View::Mix => View::Mixes,
            View::Playlist if self.playlist_parent == View::ArtistAlbums => View::Artists,
            View::Playlist => View::Playlists,
            other => other,
        };
        if let Some(index) = NAV.iter().position(|v| *v == nav_view) {
            self.nav = index;
        }
        if self.view == View::ArtistAlbums && self.artist_albums.len() <= self.viewport {
            self.fetch(true);
        } else {
            self.fetch(false);
        }
    }
    fn fetch(&mut self, append: bool) {
        if self.loading || matches!(self.view, View::Queue | View::Help | View::Account) {
            return;
        }
        if self.view == View::ArtistAlbums && (!append || self.releases_cursor.is_none()) {
            return;
        }
        let Some(client) = self.client.clone() else {
            self.view = View::Account;
            self.nav = 5;
            return;
        };
        if (self.view == View::Search && self.query.trim().is_empty())
            || (self.view == View::Artists && self.artist_query.trim().is_empty())
        {
            return;
        }
        let offset = if append && !matches!(self.view, View::Artist | View::ArtistAlbums) {
            let Some(offset) = self.next_offset else {
                return;
            };
            offset
        } else {
            0
        };
        self.generation += 1;
        let generation = self.generation;
        let view = self.view;
        let query = self.query.clone();
        let artist_query = self.artist_query.clone();
        let artist = self.artist.clone();
        let artist_search_next = self.artist_search_next;
        if append && self.view == View::Artist && artist_search_next.is_none() {
            return;
        }
        let releases_block = self.releases_block.clone();
        let releases_cursor = self.releases_cursor.clone();
        let mix = self.mix.clone();
        let owner = self.user_id;
        let playlist = self.playlist.clone();
        let tx = self.tx.clone();
        self.loading = true;
        self.fetch_task = Some(tokio::spawn(async move {
            if view == View::Artists {
                let result = client
                    .search_artists(&artist_query, offset, 50)
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(Message::Artists(generation, result, append));
                return;
            }
            if view == View::Mixes {
                let result = client.mixes().await.map_err(|e| e.to_string());
                let _ = tx.send(Message::Mixes(generation, result));
                return;
            }
            if view == View::ArtistAlbums {
                if let Some(id) = releases_block {
                    let result = client
                        .block(&id, releases_cursor.as_deref(), 100)
                        .await
                        .map_err(|e| e.to_string());
                    let _ = tx.send(Message::ArtistReleases(generation, result));
                }
                return;
            }
            if view == View::Artist {
                if append {
                    if let (Some(artist), Some(offset)) = (artist, artist_search_next) {
                        let result = client
                            .artist_tracks(&artist, offset, 100)
                            .await
                            .map_err(|e| e.to_string());
                        let _ = tx.send(Message::ArtistSearch(generation, result));
                    }
                } else if let Some(artist) = artist {
                    let result = client.artist(&artist.id).await.map_err(|e| e.to_string());
                    let _ = tx.send(Message::ArtistCatalog(generation, result));
                }
                return;
            }
            if view == View::Playlists {
                let result = client
                    .playlists(owner, offset, 50)
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(Message::Playlists(generation, result, append));
                return;
            }
            let result = match view {
                View::Home => match client.catalog().await {
                    Ok(catalog)
                        if catalog.audios.is_empty()
                            && !catalog.catalog.default_section.is_empty() =>
                    {
                        client
                            .section(&catalog.catalog.default_section, None)
                            .await
                            .map(|page| Page::complete(page.audios))
                    }
                    Ok(catalog) => Ok(Page::complete(catalog.audios)),
                    Err(error) => Err(error),
                },
                View::Library => client.library(owner, offset, 100).await,
                View::Search => client.search(&query, offset, 100).await,
                View::Mix => match mix {
                    Some(Mix::Stream(m)) => client
                        .stream_mix(&m.stream_mix.id)
                        .await
                        .map(Page::complete),
                    Some(Mix::Playlist(p)) => client.playlist_tracks(&p, offset, 100).await,
                    None => Err(vk_music::Error::InvalidInput("микс не выбран".into())),
                },
                View::Playlist => match playlist {
                    Some(playlist) => client.playlist_tracks(&playlist, offset, 100).await,
                    None => Err(vk_music::Error::InvalidInput("плейлист не выбран".into())),
                },
                _ => return,
            }
            .map_err(|e| e.to_string());
            let _ = tx.send(Message::Tracks(generation, result, append));
        }));
    }
    fn clear_prefetch(&mut self) {
        self.prefetch_generation = self.prefetch_generation.wrapping_add(1);
        for (_, (_, task)) in self.prefetch_tasks.drain() {
            task.abort();
        }
        self.prefetch_urls.clear();
        for (_, path) in self.prefetch_files.drain() {
            if self.playing_file.as_ref() != Some(&path) {
                let _ = std::fs::remove_file(path);
            }
        }
        if let Some(dir) = &self.prefetch_dir
            && let Ok(entries) = std::fs::read_dir(dir.path())
        {
            for entry in entries.flatten() {
                if self.playing_file.as_ref() != Some(&entry.path()) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
    fn prefetch(&mut self, start: usize) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(dir) = self.prefetch_dir.as_ref() else {
            return;
        };
        let candidates: Vec<_> = self
            .saved
            .queue
            .iter()
            .skip(start)
            .filter(|t| !t.is_restricted())
            .take(3)
            .cloned()
            .collect();
        let desired: std::collections::HashSet<_> = candidates.iter().map(Track::key).collect();
        for (key, (job, task)) in &self.prefetch_tasks {
            if !desired.contains(key) {
                task.abort();
                let path = dir
                    .path()
                    .join(format!("{}-{job}.mka", self.prefetch_generation));
                let _ = std::fs::remove_file(path);
            }
        }
        self.prefetch_tasks.retain(|key, _| desired.contains(key));
        self.prefetch_urls.retain(|key, _| desired.contains(key));
        self.prefetch_files.retain(|key, path| {
            if desired.contains(key) {
                true
            } else {
                if self.playing_file.as_ref() != Some(path) {
                    let _ = std::fs::remove_file(path);
                }
                false
            }
        });
        for track in candidates {
            let key = track.key();
            if self.prefetch_tasks.contains_key(&key)
                || self.prefetch_files.contains_key(&key)
                || self.prefetch_urls.contains_key(&key)
            {
                continue;
            }
            let client = client.clone();
            let tx = self.tx.clone();
            let ffmpeg = self.ffmpeg.clone();
            let generation = self.prefetch_generation;
            self.prefetch_job = self.prefetch_job.wrapping_add(1);
            let job = self.prefetch_job;
            let path = dir.path().join(format!("{generation}-{job}.mka"));
            let task = tokio::spawn(async move {
                let Ok(resolved) = client.resolve(&track).await else {
                    let _ = tx.send(Message::PrefetchDone(generation, job, key, None));
                    return;
                };
                let _ = tx.send(Message::PrefetchUrl(generation, job, key, resolved.clone()));
                // Matroska allows FFmpeg to remux VK's AES-protected HLS to one local file.
                let mut child = match Command::new(ffmpeg)
                    .args([
                        "-hide_banner",
                        "-loglevel",
                        "error",
                        "-nostdin",
                        "-y",
                        "-headers",
                        "Referer: https://vk.ru/\r\n",
                        "-i",
                        &resolved.url,
                        "-map",
                        "0:a:0",
                        "-vn",
                        "-sn",
                        "-dn",
                        "-c:a",
                        "copy",
                        "-f",
                        "matroska",
                    ])
                    .arg(&path)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .kill_on_drop(true)
                    .spawn()
                {
                    Ok(child) => child,
                    Err(_) => {
                        let _ = tx.send(Message::PrefetchDone(generation, job, key, None));
                        return;
                    }
                };
                let ready = match tokio::time::timeout(Duration::from_secs(90), child.wait()).await
                {
                    Ok(Ok(status)) => status.success(),
                    _ => {
                        let _ = child.start_kill();
                        let _ = child.wait().await;
                        false
                    }
                } && std::fs::metadata(&path).is_ok_and(|file| file.len() > 1024);
                if !ready {
                    let _ = tokio::fs::remove_file(&path).await;
                }
                let _ = tx.send(Message::PrefetchDone(
                    generation,
                    job,
                    key,
                    ready.then_some(path),
                ));
            });
            self.prefetch_tasks.insert(key, (job, task));
        }
    }
    pub fn prefetched_count(&self) -> usize {
        self.prefetch_files.len()
    }
    fn play(&mut self, index: usize, player: &Player) {
        let Some(track) = self.saved.queue.get(index).cloned() else {
            return;
        };
        if track.is_restricted() {
            self.notify(
                "VK ограничил доступ к этой записи. Выберите другой трек.",
                true,
            );
            return;
        }
        let Some(client) = self.client.clone() else {
            self.notify("Сначала войдите в VK", true);
            return;
        };
        if let Some(task) = self.resolve_task.take() {
            task.abort();
        }
        self.play_generation += 1;
        let generation = self.play_generation;
        let tx = self.tx.clone();
        player.stop();
        let key = track.key();
        let reusable_file = self
            .playing_file
            .as_ref()
            .filter(|path| self.playing.as_ref().is_some_and(|t| t.key() == key) && path.exists())
            .cloned();
        if let Some(old) = self.playing_file.take()
            && reusable_file.as_ref() != Some(&old)
        {
            let _ = std::fs::remove_file(old);
        }
        self.saved.current = Some(index);
        self.playing = Some(track.clone());
        self.position = 0.0;
        self.duration = track.duration as f64;
        self.buffering = true;
        self.paused = false;
        self.pause_on_load = false;
        self.lyrics = None;
        self.ended = false;
        self.notify("Подготавливаю трек…", false);
        if index + 3 >= self.saved.queue.len() {
            self.refill_stream();
        }
        if let Some(path) = reusable_file.or_else(|| self.prefetch_files.get(&key).cloned())
            && path.exists()
        {
            self.playing_file = Some(path.clone());
            player.play(&path.to_string_lossy());
            self.prefetch(index.saturating_add(1));
            self.notify("Воспроизвожу подготовленный трек…", false);
            return;
        }
        if let Some((ready, timestamp)) = self.prefetch_urls.get(&key)
            && timestamp.elapsed() < Duration::from_secs(120)
        {
            player.play(&ready.url);
            self.prefetch(index.saturating_add(1));
            self.notify("Воспроизвожу подготовленный трек…", false);
            return;
        }
        self.resolve_task = Some(tokio::spawn(async move {
            let result = client.resolve(&track).await.map_err(|e| e.to_string());
            let _ = tx.send(Message::Resolved(generation, result));
        }));
    }
    fn next(&mut self, player: &Player, automatic: bool) {
        let len = self.saved.queue.len();
        if len == 0 {
            return;
        }
        let current = self.saved.current.unwrap_or(0).min(len - 1);
        let index = if automatic && self.saved.repeat == Repeat::One {
            current
        } else if self.saved.shuffle && len > 1 {
            (current + rand::thread_rng().gen_range(1..len)) % len
        } else if current + 1 < len {
            current + 1
        } else if self.queue_stream.is_some() {
            self.stream_waiting = true;
            self.buffering = true;
            self.refill_stream();
            return;
        } else if self.saved.repeat == Repeat::All {
            0
        } else {
            self.stop(player);
            self.notify("Очередь завершена", false);
            return;
        };
        self.play(index, player);
    }
    fn refill_stream(&mut self) {
        if self.stream_task.is_some() {
            return;
        }
        let (Some(client), Some(id)) = (self.client.clone(), self.queue_stream.clone()) else {
            return;
        };
        let tx = self.tx.clone();
        let generation = self.stream_generation;
        self.stream_task = Some(tokio::spawn(async move {
            let result = client.stream_mix(&id).await.map_err(|e| e.to_string());
            let _ = tx.send(Message::Stream(generation, result));
        }));
    }
    fn cancel_stream(&mut self) {
        if let Some(task) = self.stream_task.take() {
            task.abort();
        }
        self.stream_generation += 1;
        self.queue_stream = None;
        self.stream_waiting = false;
    }
    fn stop(&mut self, player: &Player) {
        if let Some(task) = self.resolve_task.take() {
            task.abort();
        }
        self.play_generation += 1;
        if let Some(task) = self.stream_task.take() {
            task.abort();
        }
        self.stream_generation += 1;
        self.stream_waiting = false;
        self.buffering = false;
        self.paused = true;
        self.ended = true;
        self.position = 0.0;
        player.stop();
    }
    fn start_login(&mut self, input: Option<Input>, value: String) {
        if self.login_busy {
            return;
        }
        let browser = self.browser.clone();
        let tx = self.tx.clone();
        let config = self.config.clone();
        self.login_busy = true;
        self.notify("Ожидаю завершения входа…", false);
        self.login_task = Some(tokio::spawn(async move {
            let result = async {
                let session = match input {
                    Some(Input::Cookies) => login::import(std::path::Path::new(&value)).await?,
                    Some(Input::Token) => Session::token(value)?,
                    _ => login::browser(&browser).await?,
                };
                authenticate(session, config).await
            }
            .await
            .map_err(|e: anyhow::Error| e.to_string());
            let _ = tx.send(Message::Login(result));
        }));
    }
    fn message(&mut self, message: Message, player: &Player) {
        match message {
            Message::PrefetchUrl(g, job, key, track)
                if g == self.prefetch_generation
                    && self
                        .prefetch_tasks
                        .get(&key)
                        .is_some_and(|(current, _)| *current == job) =>
            {
                self.prefetch_urls.insert(key, (track, Instant::now()));
            }
            Message::PrefetchDone(g, job, key, path) => {
                if g == self.prefetch_generation
                    && self
                        .prefetch_tasks
                        .get(&key)
                        .is_some_and(|(current, _)| *current == job)
                {
                    self.prefetch_tasks.remove(&key);
                    if let Some(path) = path {
                        self.prefetch_files.insert(key, path);
                    }
                } else if let Some(path) = path {
                    let _ = std::fs::remove_file(path);
                }
            }
            Message::Artists(g, result, append) if g == self.generation => {
                self.loading = false;
                match result {
                    Ok(page) => {
                        self.next_offset = page.next_offset();
                        self.total = page.count;
                        if !append {
                            self.artists.clear();
                            self.selected = 0;
                            self.offset = 0;
                        }
                        self.artists.extend(page.items);
                        self.notify(
                            format!(
                                "Исполнителей: {} / {} · Enter — открыть исполнителя",
                                self.artists.len(),
                                self.total
                            ),
                            false,
                        );
                    }
                    Err(e) => self.notify(e, true),
                }
            }
            Message::ArtistCatalog(g, result) if g == self.generation => {
                self.loading = false;
                match result {
                    Ok(catalog) => {
                        self.tracks = catalog.audios;
                        self.artist_search_next = Some(0);
                        let releases = catalog
                            .catalog
                            .sections
                            .iter()
                            .flat_map(|s| &s.blocks)
                            .find(|b| b.data_type == "music_playlists" && b.title == "Релизы")
                            .or_else(|| {
                                catalog
                                    .catalog
                                    .sections
                                    .iter()
                                    .flat_map(|s| &s.blocks)
                                    .find(|b| b.data_type == "music_playlists")
                            });
                        self.releases_block = releases.map(|b| b.id.clone());
                        self.releases_cursor = releases
                            .and_then(|b| b.next_from.clone())
                            .filter(|s| !s.is_empty());
                        let release_ids: std::collections::HashSet<_> = releases
                            .into_iter()
                            .flat_map(|b| &b.playlists_ids)
                            .collect();
                        self.artist_albums = catalog
                            .playlists
                            .into_iter()
                            .filter(|p| release_ids.contains(&format!("{}_{}", p.owner_id, p.id)))
                            .collect();
                        if let Some(artist) = catalog.artists.into_iter().next() {
                            self.artist = Some(artist);
                        }
                        self.selected = 0;
                        self.offset = 0;
                        self.notify("Загружаю все треки исполнителя…", false);
                        self.fetch(true);
                    }
                    Err(e) => self.notify(e, true),
                }
            }
            Message::ArtistReleases(g, result) if g == self.generation => {
                self.loading = false;
                match result {
                    Ok(page) => {
                        self.releases_cursor = page
                            .block
                            .next_from
                            .filter(|s| !s.is_empty())
                            .filter(|s| Some(s) != self.releases_cursor.as_ref());
                        let mut ids: std::collections::HashSet<_> = self
                            .artist_albums
                            .iter()
                            .map(|p| (p.owner_id, p.id))
                            .collect();
                        self.artist_albums.extend(
                            page.playlists
                                .into_iter()
                                .filter(|p| ids.insert((p.owner_id, p.id))),
                        );
                    }
                    Err(e) => self.notify(e, true),
                }
            }
            Message::ArtistSearch(g, result) if g == self.generation => {
                self.loading = false;
                match result {
                    Ok(page) => {
                        self.artist_search_next = page.next_offset();
                        let mut ids: std::collections::HashSet<_> =
                            self.tracks.iter().map(Track::key).collect();
                        let before = self.tracks.len();
                        self.tracks
                            .extend(page.items.into_iter().filter(|t| ids.insert(t.key())));
                        self.notify(
                            format!("Треков исполнителя: {} · o — релизы", self.tracks.len()),
                            false,
                        );
                        if self.tracks.len() == before && self.artist_search_next.is_some() {
                            self.fetch(true);
                        }
                    }
                    Err(e) => self.notify(e, true),
                }
            }
            Message::Mixes(g, result) if g == self.generation => {
                self.loading = false;
                match result {
                    Ok(mixes) => {
                        self.mixes = mixes;
                        self.selected = 0;
                        self.offset = 0;
                        self.notify("Enter — открыть микс · Enter по треку — запустить", false);
                    }
                    Err(e) => self.notify(e, true),
                }
            }
            Message::Stream(g, result) if g == self.stream_generation => {
                self.stream_task = None;
                let old_len = self.saved.queue.len();
                match result {
                    Ok(tracks) => {
                        let mut ids: std::collections::HashSet<_> =
                            self.saved.queue.iter().map(Track::key).collect();
                        self.saved.queue.extend(
                            tracks
                                .into_iter()
                                .filter(|t| !t.is_restricted() && ids.insert(t.key())),
                        );
                        if self.view == View::Mix
                            && matches!(&self.mix, Some(Mix::Stream(m)) if Some(&m.stream_mix.id) == self.queue_stream.as_ref())
                        {
                            self.tracks = self.saved.queue.clone();
                        }
                        if self.stream_waiting {
                            self.stream_waiting = false;
                            if old_len < self.saved.queue.len() {
                                let pause = self.pause_on_load;
                                self.play(old_len, player);
                                self.pause_on_load = pause;
                                self.paused = pause;
                            } else {
                                self.stop(player);
                                self.notify(
                                    "VK не вернул новых треков микса · ] — повторить",
                                    false,
                                );
                            }
                        } else if let Some(index) = self.saved.current {
                            self.prefetch(index.saturating_add(1));
                        }
                    }
                    Err(e) => {
                        if self.stream_waiting {
                            self.stop(player);
                        }
                        self.notify(e, true);
                    }
                }
            }

            Message::Tracks(g, result, append) if g == self.generation => {
                self.loading = false;
                match result {
                    Ok(page) => {
                        self.next_offset = page.next_offset();
                        self.total = page.count;
                        if !append {
                            self.tracks.clear();
                            self.selected = 0;
                            self.offset = 0;
                        }
                        self.tracks.extend(page.items);
                        self.notify(
                            format!("Загружено {} из {}", self.tracks.len(), self.total),
                            false,
                        );
                    }
                    Err(error) => self.notify(error, true),
                }
            }
            Message::Playlists(g, result, append) if g == self.generation => {
                self.loading = false;
                match result {
                    Ok(page) => {
                        self.next_offset = page.next_offset();
                        self.total = page.count;
                        if !append {
                            self.playlists.clear();
                            self.selected = 0;
                        }
                        self.playlists.extend(page.items);
                        self.notify(format!("Плейлистов: {}", self.total), false);
                    }
                    Err(error) => self.notify(error, true),
                }
            }
            Message::Resolved(g, result) if g == self.play_generation => match result {
                Ok(track) => {
                    player.play(&track.url);
                    if self.pause_on_load {
                        player.set_paused(true);
                    }
                    self.playing = Some(track);
                }
                Err(error) => {
                    self.buffering = false;
                    self.paused = true;
                    self.ended = true;
                    self.notify(error, true);
                }
            },
            Message::Login(result) => {
                self.login_busy = false;
                match result {
                    Ok((client, name)) => {
                        self.client = Some(client);
                        self.account = name;
                        self.connected = true;
                        // me() has already populated user_id; delivered separately via session in run loop.
                        self.notify("Вход выполнен", false);
                    }
                    Err(error) => {
                        self.notify(error, true);
                        self.view = View::Account;
                        self.nav = 5;
                    }
                }
            }
            Message::Lyrics(g, result) if g == self.play_generation => match result {
                Ok(text) => self.lyrics = Some(text),
                Err(error) => self.notify(error, true),
            },
            _ => {}
        }
    }
    fn media_state(&self) -> mpris::State {
        let mut state = mpris::State {
            status: if self.playing.is_none() || self.ended {
                "Stopped"
            } else if self.paused {
                "Paused"
            } else {
                "Playing"
            }
            .into(),
            volume: self.saved.volume / 100.0,
            shuffle: self.saved.shuffle,
            repeat: match self.saved.repeat {
                Repeat::Off => "None",
                Repeat::One => "Track",
                Repeat::All => "Playlist",
            }
            .into(),
            position: (self.position.max(0.0) * 1_000_000.0) as i64,
            can_play: !self.saved.queue.is_empty() && self.client.is_some(),
            can_pause: self.playing.is_some() && !self.ended,
            can_seek: self.playing.is_some() && !self.ended && !self.buffering,
            can_next: (self.saved.shuffle && self.saved.queue.len() > 1)
                || self.queue_stream.is_some()
                || self
                    .saved
                    .current
                    .is_some_and(|i| i + 1 < self.saved.queue.len())
                || (self.saved.repeat == Repeat::All && !self.saved.queue.is_empty()),
            can_previous: self.saved.current.is_some_and(|i| i > 0),
            seek_serial: self.seek_serial,
            ..Default::default()
        };
        if let Some(t) = &self.playing {
            state.track_id = format!(
                "/org/mpris/MediaPlayer2/track/o{}_{}",
                t.owner_id.to_string().replace('-', "n"),
                t.id.to_string().replace('-', "n")
            );
            state.title = t.title.clone();
            state.artist = t.artist.clone();
            state.album = t
                .album
                .as_ref()
                .map(|a| a.title.clone())
                .unwrap_or_default();
            state.duration = (self.duration.max(0.0) * 1_000_000.0) as i64;
        }
        state
    }
    fn media(&mut self, command: mpris::Command, player: &Player) -> bool {
        use mpris::Command;
        match command {
            Command::Quit => return true,
            Command::PlayPause => {
                if self.playing.is_some() && !self.ended && !self.paused {
                    self.media(Command::Pause, player);
                } else {
                    self.media(Command::Play, player);
                }
            }
            Command::Play => {
                if self.playing.is_some() && !self.ended {
                    self.paused = false;
                    self.pause_on_load = false;
                    player.set_paused(false);
                } else if !self.saved.queue.is_empty() {
                    self.play(self.saved.current.unwrap_or(0), player);
                }
            }
            Command::Pause if self.playing.is_some() && !self.ended => {
                self.paused = true;
                self.pause_on_load = true;
                player.set_paused(true);
            }
            Command::Stop => {
                self.stop(player);
                self.notify("Остановлено", false);
            }
            Command::Next | Command::Previous => {
                let was_paused = self.paused;
                let was_stopped = self.ended;
                let state = self.media_state();
                if matches!(command, Command::Next) && state.can_next {
                    self.next(player, false);
                } else if matches!(command, Command::Previous) && state.can_previous {
                    self.play(self.saved.current.unwrap_or(0).saturating_sub(1), player);
                }
                if was_paused || was_stopped {
                    self.pause_on_load = true;
                    self.paused = true;
                    player.set_paused(true);
                }
                if was_stopped {
                    self.stop(player);
                }
            }
            Command::Seek(offset) if self.media_state().can_seek => {
                let target = (self.position + offset as f64 / 1_000_000.0).max(0.0);
                if target > self.duration {
                    self.next(player, false);
                } else {
                    player.set_position(target);
                    self.position = target;
                    self.seek_serial += 1;
                }
            }
            Command::SetPosition(id, position)
                if self.media_state().can_seek
                    && position >= 0
                    && position as f64 / 1_000_000.0 <= self.duration
                    && self.media_state().track_id == id =>
            {
                self.position = position as f64 / 1_000_000.0;
                player.set_position(self.position);
                self.seek_serial += 1;
            }
            Command::Volume(volume) => {
                self.saved.volume = volume * 100.0;
                player.volume(self.saved.volume);
            }
            Command::Shuffle(value) => self.saved.shuffle = value,
            Command::Loop(value) => {
                self.saved.repeat = match value.as_str() {
                    "Track" => Repeat::One,
                    "Playlist" => Repeat::All,
                    _ => Repeat::Off,
                }
            }
            _ => {}
        }
        false
    }
    pub fn text_view(&self) -> bool {
        self.zen || matches!(self.view, View::Help | View::Account)
    }
    fn navigate(&mut self, target: usize) {
        if self.sidebar && !self.zen {
            self.nav = target.min(NAV.len() - 1);
        } else if self.text_view() {
            self.text_scroll = target.min(self.text_lines.saturating_sub(self.viewport));
        } else {
            self.selected = target.min(self.count().saturating_sub(1));
            if self.selected < self.offset {
                self.offset = self.selected;
            }
            if self.selected >= self.offset + self.viewport {
                self.offset = self.selected + 1 - self.viewport;
            }
            if self.selected + 10 >= self.count() {
                self.fetch(true);
            }
        }
    }
    fn navigation_position(&self) -> usize {
        if self.sidebar && !self.zen {
            self.nav
        } else if self.text_view() {
            self.text_scroll
        } else {
            self.selected
        }
    }
    fn local_find(&mut self, backwards: bool) {
        if self.find_query.is_empty() || self.count() == 0 {
            return;
        }
        let q = self.find_query.to_lowercase();
        let count = self.count();
        for step in 1..=count {
            let i = if backwards {
                (self.selected + count - step) % count
            } else {
                (self.selected + step) % count
            };
            let text = match self.view {
                View::Artists => self.artists[i].name.clone(),
                View::Mixes => self.mixes[i].title().to_owned(),
                View::Playlists => self.playlists[i].title.clone(),
                View::ArtistAlbums => self.artist_albums[i].title.clone(),
                _ => {
                    let t = &self.visible_tracks()[i];
                    format!("{} {}", t.artist, t.title)
                }
            };
            if text.to_lowercase().contains(&q) {
                self.sidebar = false;
                self.navigate(i);
                return;
            }
        }
        self.notify("Совпадений в загруженном списке нет", false);
    }
    fn begin_input(&mut self, input: Input, value: String) {
        self.input_cursor = value.len();
        self.input_text = value;
        self.input = Some(input);
        self.keymaps.clear();
    }
    fn select_artist(&mut self, artist: Artist) {
        self.artist = Some(artist);
        self.artist_albums.clear();
        self.artist_search_next = None;
        self.releases_block = None;
        self.releases_cursor = None;
        self.change_view(View::Artist);
    }
    fn open_artist(&mut self) {
        let track = self.visible_tracks().get(self.selected).cloned();
        if let Some(track) = track {
            if let Some(artist) = track.main_artists.first() {
                self.select_artist(artist.clone());
            } else {
                self.artist_query = track.artist;
                self.change_view(View::Artists);
            }
        }
    }
    fn command(&mut self, command: &str, player: &Player) -> bool {
        self.command_with_count(command, player, 1, false)
    }
    fn command_with_count(
        &mut self,
        command: &str,
        player: &Player,
        n: usize,
        counted: bool,
    ) -> bool {
        let (name, arg) = command
            .trim()
            .split_once(' ')
            .unwrap_or((command.trim(), ""));
        if let Some(action) = actions::from_command(name, n, counted) {
            return self.execute_action(action, player);
        }
        if name.starts_with("input-") && self.input.is_some() {
            return self.input_command(name, player);
        }
        match name {
            "q" | "q!" | "quit" | "wq" | "x" => return true,
            "help" | "h" => self.change_view(View::Help),
            "home" => self.change_view(View::Home),
            "library" => self.change_view(View::Library),
            "queue" => self.change_view(View::Queue),
            "playlists" => self.change_view(View::Playlists),
            "account" => self.change_view(View::Account),
            "mixes" => self.change_view(View::Mixes),
            "artists" => {
                if !arg.is_empty() {
                    self.artist_query = arg.into();
                }
                self.change_view(View::Artists);
            }
            "search" => {
                self.query = arg.into();
                self.change_view(View::Search);
            }
            "section" => {
                if let Ok(index) = arg.parse::<usize>()
                    && let Some(view) = NAV.get(index.saturating_sub(1))
                {
                    self.change_view(*view);
                    self.sidebar = false;
                }
            }
            "input-command" => self.begin_input(Input::Command, String::new()),
            "input-search" => {
                self.sidebar = false;
                if self.view == View::Artists {
                    self.begin_input(Input::Artists, self.artist_query.clone());
                } else {
                    self.begin_input(Input::Search, self.query.clone());
                }
            }
            "input-find" => {
                self.sidebar = false;
                self.begin_input(Input::Find, self.find_query.clone());
            }
            "find-next" => self.local_find(false),
            "find-prev" => self.local_find(true),
            "open" => self.open_selected(player),
            "back" => self.back(false),
            "back-or-cancel" => self.back(true),
            "open-artist" if !self.sidebar && !self.zen => self.open_artist(),
            "artist-toggle"
                if !self.loading && matches!(self.view, View::Artist | View::ArtistAlbums) =>
            {
                self.change_view(if self.view == View::Artist {
                    View::ArtistAlbums
                } else {
                    View::Artist
                });
            }
            "zen" => {
                self.zen = !self.zen;
                self.text_scroll = 0;
            }
            "lyrics" => self.show_lyrics(),
            "login-browser" if self.view == View::Account => self.start_login(None, String::new()),
            "login-cookies" if self.view == View::Account => {
                self.begin_input(Input::Cookies, String::new())
            }
            "login-token" if self.view == View::Account => {
                self.begin_input(Input::Token, String::new())
            }
            "play" => {
                self.media(mpris::Command::Play, player);
            }
            "pause" => {
                self.media(mpris::Command::Pause, player);
            }
            "play-pause" => {
                self.media(mpris::Command::PlayPause, player);
            }
            "stop" => self.stop(player),
            "next" => self.next(player, false),
            "prev" => self.play(self.saved.current.unwrap_or(0).saturating_sub(1), player),
            "reload" | "e" => self.fetch(false),
            "refresh-view" if self.view == View::ArtistAlbums => self.change_view(View::Artist),
            "refresh-view" => self.fetch(false),
            "seek-back" => {
                self.media(mpris::Command::Seek(-5_000_000 * n as i64), player);
            }
            "seek-forward" => {
                self.media(mpris::Command::Seek(5_000_000 * n as i64), player);
            }
            "volume-up" => {
                self.saved.volume = (self.saved.volume + 5.0 * n as f64).min(100.0);
                player.volume(self.saved.volume);
            }
            "volume-down" => {
                self.saved.volume = (self.saved.volume - 5.0 * n as f64).max(0.0);
                player.volume(self.saved.volume);
            }
            "shuffle" => self.saved.shuffle = !self.saved.shuffle,
            "repeat" => self.saved.repeat = self.saved.repeat.cycle(),
            "source" | "reload-keymaps" => match Keymaps::load(&self.config.join("keymaps.lua")) {
                Ok(keymaps) => {
                    self.keymaps = keymaps;
                    self.notify("Бинды Lua загружены", false);
                }
                Err(error) => self.notify(format!("Ошибка биндов: {error:#}"), true),
            },
            number if number.parse::<usize>().is_ok() => {
                self.navigate(number.parse::<usize>().unwrap().saturating_sub(1))
            }
            _ => self.notify("Неизвестная команда · :help", true),
        }
        false
    }
    fn key(&mut self, key: KeyEvent, player: &Player) -> bool {
        self.key_inner(key, player, 0)
    }

    fn key_inner(&mut self, key: KeyEvent, player: &Player, depth: usize) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if depth > 8 {
            self.notify("Слишком длинная цепочка Lua-биндов", true);
            return false;
        }
        let mode = if self.input.is_some() { 'i' } else { 'n' };
        match self.keymaps.feed(mode, key) {
            KeyMatch::Pending => false,
            KeyMatch::Retry(key) => self.key_inner(key, player, depth + 1),
            KeyMatch::Mapped(Mapping::Command(command), n, counted) => {
                self.command_with_count(&command, player, n, counted)
            }
            KeyMatch::Mapped(Mapping::Keys(keys), _, _) => {
                for key in keys {
                    if self.key_inner(key, player, depth + 1) {
                        return true;
                    }
                }
                false
            }
            KeyMatch::Unmapped => {
                if self.input.is_some()
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    && let KeyCode::Char(ch) = key.code
                {
                    let cursor = self.input_cursor.min(self.input_text.len());
                    self.input_text.insert(cursor, ch);
                    self.input_cursor = cursor + ch.len_utf8();
                }
                false
            }
        }
    }

    fn input_command(&mut self, name: &str, player: &Player) -> bool {
        let Some(input) = self.input else {
            return false;
        };
        let cursor = self.input_cursor.min(self.input_text.len());
        let previous = self.input_text[..cursor]
            .char_indices()
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0);
        let next = self.input_text[cursor..]
            .chars()
            .next()
            .map(|ch| cursor + ch.len_utf8())
            .unwrap_or(cursor);
        match name {
            "input-cancel" => {
                self.input = None;
                self.input_text.clear();
                self.input_cursor = 0;
            }
            "input-submit" => {
                let value = std::mem::take(&mut self.input_text);
                self.input = None;
                self.input_cursor = 0;
                match input {
                    Input::Search => {
                        self.query = value;
                        self.change_view(View::Search);
                    }
                    Input::Artists => {
                        self.artist_query = value;
                        self.change_view(View::Artists);
                    }
                    Input::Find => {
                        self.find_query = value;
                        self.local_find(false);
                    }
                    Input::Command => return self.command(&value, player),
                    _ => self.start_login(Some(input), value),
                }
            }
            "input-left" => self.input_cursor = previous,
            "input-right" => self.input_cursor = next,
            "input-home" => self.input_cursor = 0,
            "input-end" => self.input_cursor = self.input_text.len(),
            "input-backspace" => {
                self.input_text.drain(previous..cursor);
                self.input_cursor = previous;
            }
            "input-delete" => {
                self.input_text.drain(cursor..next);
            }
            "input-clear-left" => {
                self.input_text.drain(..cursor);
                self.input_cursor = 0;
            }
            "input-delete-word" => {
                let before = self.input_text[..cursor].trim_end();
                let start = before
                    .rfind(char::is_whitespace)
                    .map(|index| index + before[index..].chars().next().unwrap().len_utf8())
                    .unwrap_or(0);
                self.input_text.drain(start..cursor);
                self.input_cursor = start;
            }
            _ => {}
        }
        false
    }

    fn execute_action(&mut self, action: Action, player: &Player) -> bool {
        match action {
            Action::Scroll(delta) => {
                if self.sidebar || self.text_view() {
                    self.navigate(self.navigation_position().saturating_add_signed(delta));
                } else {
                    self.offset = self
                        .offset
                        .saturating_add_signed(delta)
                        .min(self.count().saturating_sub(self.viewport));
                    self.selected = self.selected.clamp(
                        self.offset,
                        (self.offset + self.viewport)
                            .min(self.count())
                            .saturating_sub(1)
                            .max(self.offset),
                    );
                    if self.selected + 10 >= self.count() {
                        self.fetch(true);
                    }
                }
                false
            }
            Action::Move(delta) => {
                self.navigate(self.navigation_position().saturating_add_signed(delta));
                false
            }
            Action::Page(delta, half) => {
                let size = if half {
                    (self.viewport / 2).max(1)
                } else {
                    self.viewport.max(1)
                };
                self.navigate(
                    self.navigation_position()
                        .saturating_add_signed(delta.saturating_mul(size as isize)),
                );
                false
            }
            Action::First(i) => {
                self.navigate(i);
                false
            }
            Action::Last(i) => {
                let end = if self.sidebar {
                    NAV.len() - 1
                } else if self.text_view() {
                    self.text_lines
                } else {
                    self.count().saturating_sub(1)
                };
                self.navigate(i.unwrap_or(end));
                false
            }
            Action::Screen(c) => {
                let (offset, rows, count) = if self.sidebar {
                    (0, NAV.len(), NAV.len())
                } else {
                    (self.offset, self.viewport, self.count())
                };
                let last = (offset + rows).min(count).saturating_sub(1);
                self.navigate(match c {
                    'H' => offset,
                    'M' => offset + last.saturating_sub(offset) / 2,
                    _ => last,
                });
                false
            }
            Action::Align(c) => {
                self.offset = self.selected.saturating_sub(match c {
                    't' => 0,
                    'z' => self.viewport / 2,
                    _ => self.viewport.saturating_sub(1),
                });
                false
            }
            Action::Focus(sidebar) => {
                self.zen = false;
                self.sidebar = sidebar;
                false
            }
            Action::ToggleFocus => {
                self.zen = false;
                self.sidebar = !self.sidebar;
                false
            }
            Action::Tab(delta) => {
                let index = (self.nav as isize + delta).rem_euclid(NAV.len() as isize) as usize;
                self.change_view(NAV[index]);
                self.sidebar = false;
                false
            }
            Action::Section(i) => {
                if let Some(view) = NAV.get(i) {
                    self.change_view(*view);
                    self.sidebar = false;
                }
                false
            }
            Action::Delete(n) => {
                if !self.sidebar
                    && !self.zen
                    && self.view == View::Queue
                    && self.selected < self.count()
                {
                    let start = self.selected;
                    let end = (start + n).min(self.count());
                    self.saved.queue.drain(start..end);
                    match self.saved.current {
                        Some(i) if (start..end).contains(&i) => {
                            self.stop(player);
                            if let Some(path) = self.playing_file.take() {
                                let _ = std::fs::remove_file(path);
                            }
                            self.playing = None;
                            self.saved.current = None;
                        }
                        Some(i) if i >= end => self.saved.current = Some(i - (end - start)),
                        _ => {}
                    }
                    self.navigate(start);
                    let next = if self.playing.is_some() && !self.ended {
                        self.saved.current.unwrap_or(0).saturating_add(1)
                    } else {
                        self.saved.current.unwrap_or(0)
                    };
                    self.prefetch(next);
                }
                false
            }
            Action::Add(n) => {
                if !self.sidebar
                    && !self.zen
                    && !matches!(
                        self.view,
                        View::Artists
                            | View::Mixes
                            | View::ArtistAlbums
                            | View::Playlists
                            | View::Account
                            | View::Help
                    )
                {
                    let tracks: Vec<_> = self
                        .visible_tracks()
                        .iter()
                        .skip(self.selected)
                        .take(n)
                        .filter(|t| !t.is_restricted())
                        .cloned()
                        .collect();
                    let count = tracks.len();
                    self.saved.queue.extend(tracks);
                    let next = if self.playing.is_some() && !self.ended {
                        self.saved.current.unwrap_or(0).saturating_add(1)
                    } else {
                        self.saved.current.unwrap_or(0)
                    };
                    self.prefetch(next);
                    self.notify(format!("Добавлено в очередь: {count}"), false);
                }
                false
            }
        }
    }
    fn open_selected(&mut self, player: &Player) {
        if self.zen {
            return;
        }
        if self.sidebar {
            self.change_view(NAV[self.nav]);
            self.sidebar = false;
        } else if self.view == View::Artists {
            if let Some(artist) = self.artists.get(self.selected).cloned() {
                self.select_artist(artist);
            }
        } else if self.view == View::Mixes {
            if let Some(mix) = self.mixes.get(self.selected).cloned() {
                self.mix = Some(mix);
                self.change_view(View::Mix);
            }
        } else if matches!(self.view, View::Playlists | View::ArtistAlbums) {
            let list = if self.view == View::ArtistAlbums {
                &self.artist_albums
            } else {
                &self.playlists
            };
            if let Some(playlist) = list.get(self.selected).cloned() {
                self.playlist = Some(playlist);
                self.playlist_parent = self.view;
                self.change_view(View::Playlist);
            }
        } else if self.view == View::Queue {
            self.play(self.selected, player);
        } else if let Some(track) = self.tracks.get(self.selected).cloned() {
            if track.is_restricted() {
                self.notify("VK ограничил доступ к этой записи", true);
                return;
            }
            self.cancel_stream();
            player.stop();
            self.clear_prefetch();
            if self.view == View::Mix
                && let Some(Mix::Stream(mix)) = &self.mix
            {
                self.queue_stream = Some(mix.stream_mix.id.clone());
            }
            self.saved.queue = self
                .tracks
                .iter()
                .filter(|track| !track.is_restricted())
                .cloned()
                .collect();
            if let Some(index) = self
                .saved
                .queue
                .iter()
                .position(|queued| queued.key() == track.key())
            {
                self.play(index, player);
            }
        }
    }
    fn back(&mut self, cancel_login: bool) {
        if self.zen {
            self.zen = false;
        } else if cancel_login && self.login_busy {
            if let Some(task) = self.login_task.take() {
                task.abort();
            }
            self.login_busy = false;
            self.notify("Вход отменён", false);
        } else {
            match self.view {
                View::Playlist => self.change_view(self.playlist_parent),
                View::ArtistAlbums => self.change_view(View::Artist),
                View::Artist => self.change_view(View::Artists),
                View::Mix => self.change_view(View::Mixes),
                _ => self.sidebar = true,
            }
        }
    }
    fn show_lyrics(&mut self) {
        if let (Some(client), Some(id)) = (
            self.client.clone(),
            self.playing.as_ref().and_then(|track| track.lyrics_id),
        ) {
            let tx = self.tx.clone();
            let generation = self.play_generation;
            tokio::spawn(async move {
                let _ = tx.send(Message::Lyrics(
                    generation,
                    client.lyrics(id).await.map_err(|error| error.to_string()),
                ));
            });
            self.zen = true;
            self.text_scroll = 0;
        } else {
            self.notify("У этого трека нет доступного текста VK", false);
        }
    }
}

pub async fn authenticate(session: Session, config: PathBuf) -> Result<(Client, String)> {
    let client = Client::new(session)?;
    let user = client.me().await?;
    let session = client.session().await;
    tokio::task::spawn_blocking(move || storage::save(&config.join("session.json"), &session))
        .await??;
    Ok((client, format!("{} {}", user.first_name, user.last_name)))
}

pub async fn run(config: PathBuf, browser: String, mpv: String, ffmpeg: String) -> Result<()> {
    storage::ensure_keymaps(&config)?;
    let keymaps = Keymaps::load(&config.join("keymaps.lua"))?;
    let mut saved: Saved = if config.join("state.json").exists() {
        storage::read(&config.join("state.json"))?
    } else {
        Saved::default()
    };
    if !saved.volume.is_finite() {
        saved.volume = 60.0;
    }
    saved.volume = saved.volume.clamp(0.0, 100.0);
    if saved.current.is_some_and(|i| i >= saved.queue.len()) {
        saved.current = None;
    }
    let (mut player, mut audio) = Player::start(&mpv).await?;
    player.volume(saved.volume);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut app = App::new(saved, config.clone(), browser, ffmpeg, tx.clone());
    app.keymaps = keymaps;
    if config.join("session.json").exists() {
        let session = storage::read(&config.join("session.json"))?;
        let config = config.clone();
        app.login_busy = true;
        app.login_task = Some(tokio::spawn(async move {
            let _ = tx.send(Message::Login(
                authenticate(session, config)
                    .await
                    .map_err(|e| e.to_string()),
            ));
        }));
    } else {
        app.view = View::Account;
        app.nav = 5;
    }
    let (media_tx, mut media_rx) = mpsc::unbounded_channel();
    let _media_tx = media_tx.clone();
    let media =
        match tokio::time::timeout(Duration::from_secs(3), mpris::Service::start(media_tx)).await {
            Ok(Ok(service)) => Some(service),
            _ => {
                app.notify(
                    "MPRIS недоступен: нет D-Bus сессии или имя vkpl занято",
                    true,
                );
                None
            }
        };
    let mut terminal = ratatui::init();
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            ratatui::restore();
        }
    }
    let restore = Restore;
    let mut input = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(80));
    let result: Result<()> = async {
        loop {
            terminal.draw(|frame| ui::draw(frame, &mut app))?;
            if let Some(media) = &media { media.state.send_replace(app.media_state()); }
            tokio::select! {
                Some(command) = media_rx.recv() => { if app.media(command, &player) { break; } },
                _ = tick.tick() => { app.tick = app.tick.wrapping_add(1); },
                event = input.next() => match event {
                    Some(Ok(Event::Key(key))) if app.key(key, &player) => break,
                    Some(Err(error)) => return Err(error.into()), None => break, _ => {},
                },
                message = rx.recv() => if let Some(message) = message {
                    let login = matches!(&message, Message::Login(Ok(_)));
                    app.message(message, &player);
                    if login {
                        app.user_id = app.client.as_ref().unwrap().session().await.user_id.unwrap_or(0);
                        app.change_view(View::Library);
                        if !app.saved.queue.is_empty() { app.prefetch(app.saved.current.unwrap_or(0)); }
                    }
                },
                event = audio.recv() => if let Some(event) = event { match event {
                    player::Event::Position(p) => app.position = p,
                    player::Event::Duration(d) => app.duration = d,
                    player::Event::Paused(p) => app.paused = p || app.ended || app.pause_on_load,
                    player::Event::Volume(v) => app.saved.volume = v,
                    player::Event::Loaded => if !app.ended {
                        app.buffering = false;
                        if app.pause_on_load { player.set_paused(true); app.paused = true; }
                        if let Some(index) = app.saved.current { app.prefetch(index.saturating_add(1)); }
                        app.notify("Воспроизведение", false);
                    },
                    player::Event::End => if !app.buffering && !app.ended { app.next(&player, true); },
                    player::Event::Error(error) => { app.buffering = false; app.paused = true; app.ended = true; app.notify(error, true); },
                } } else { anyhow::bail!("Соединение с mpv завершилось"); },
            }
        }
        Ok(())
    }.await;
    drop(restore);
    for task in [
        app.stream_task.take(),
        app.fetch_task.take(),
        app.resolve_task.take(),
        app.login_task.take(),
    ]
    .into_iter()
    .flatten()
    {
        task.abort();
    }
    player.shutdown().await;
    app.playing_file = None;
    app.clear_prefetch();
    // Signed media URLs are refreshed before playback, so do not persist them.
    for track in &mut app.saved.queue {
        track.url.clear();
    }
    storage::save(&config.join("state.json"), &app.saved)?;
    if let Some(client) = app.client {
        storage::save(&config.join("session.json"), &client.session().await)?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn interface_handles_small_terminals_and_hides_tokens() {
        let (tx, _) = mpsc::unbounded_channel();
        let mut app = App::new(
            Saved::default(),
            PathBuf::from("/tmp/unused"),
            "chromium".into(),
            "ffmpeg".into(),
            tx,
        );
        app.view = View::Account;
        app.input = Some(Input::Token);
        app.input_text = "super-secret-token".into();
        for (width, height) in [(30, 8), (45, 14), (80, 24), (120, 40)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
            let rendered: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(!rendered.contains("super-secret-token"));
        }
        assert_eq!(app.saved.volume, 60.0);
    }

    #[test]
    fn queue_view_and_library_have_independent_sources() {
        let (tx, _) = mpsc::unbounded_channel();
        let mut app = App::new(
            Saved::default(),
            PathBuf::from("/tmp/unused"),
            "chromium".into(),
            "ffmpeg".into(),
            tx,
        );
        app.saved.queue.push(Track {
            id: 1,
            ..Default::default()
        });
        app.tracks.push(Track {
            id: 2,
            ..Default::default()
        });
        app.view = View::Queue;
        assert_eq!(app.visible_tracks()[0].id, 1);
        app.view = View::Library;
        assert_eq!(app.visible_tracks()[0].id, 2);
    }
}
