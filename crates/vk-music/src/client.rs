use crate::{auth::now, *};
use reqwest::{cookie::Jar, header, redirect::Policy};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36";

/// Clones share the connection pool, refresh lock and request throttle.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}
struct Inner {
    http: reqwest::Client,
    session: Mutex<Session>,
    throttle: Mutex<tokio::time::Instant>,
    version: String,
    app_id: String,
    interval: Duration,
    api_base: String,
    auth_url: String,
}

pub struct ClientBuilder {
    session: Session,
    version: String,
    app_id: String,
    user_agent: String,
    timeout: Duration,
    interval: Duration,
}

impl ClientBuilder {
    pub fn api_version(mut self, value: impl Into<String>) -> Self {
        self.version = value.into();
        self
    }
    pub fn app_id(mut self, value: impl Into<String>) -> Self {
        self.app_id = value.into();
        self
    }
    pub fn user_agent(mut self, value: impl Into<String>) -> Self {
        self.user_agent = value.into();
        self
    }
    pub fn timeout(mut self, value: Duration) -> Self {
        self.timeout = value;
        self
    }
    pub fn request_interval(mut self, value: Duration) -> Self {
        self.interval = value;
        self
    }
    pub fn build(self) -> Result<Client> {
        let jar = Arc::new(Jar::default());
        for cookie in &self.session.cookies {
            cookie.validate()?;
            if !cookie.alive() {
                continue;
            }
            let origin: url::Url = format!("https://{}/", cookie.domain.trim_start_matches('.'))
                .parse()
                .map_err(|_| Error::InvalidInput("домен cookie".into()))?;
            let raw = format!(
                "{}={}; Domain={}; Path={}; {}",
                cookie.name,
                cookie.value,
                cookie.domain,
                cookie.path,
                if cookie.secure { "Secure" } else { "" }
            );
            jar.add_cookie_str(&raw, &origin);
        }
        let http = reqwest::Client::builder()
            .user_agent(self.user_agent)
            .timeout(self.timeout)
            .connect_timeout(Duration::from_secs(10))
            .redirect(Policy::none())
            .cookie_provider(jar)
            .build()?;
        Ok(Client {
            inner: Arc::new(Inner {
                http,
                session: Mutex::new(self.session),
                throttle: Mutex::new(tokio::time::Instant::now()),
                version: self.version,
                app_id: self.app_id,
                interval: self.interval,
                api_base: "https://web.api.vk.ru/method/".into(),
                auth_url: "https://login.vk.ru/?act=web_token".into(),
            }),
        })
    }
}

impl Client {
    pub fn builder(session: Session) -> ClientBuilder {
        ClientBuilder {
            session,
            version: "5.289".into(),
            app_id: "6287487".into(),
            user_agent: USER_AGENT.into(),
            timeout: Duration::from_secs(25),
            interval: Duration::from_millis(400),
        }
    }
    pub fn new(session: Session) -> Result<Self> {
        Self::builder(session).build()
    }
    pub async fn session(&self) -> Session {
        self.inner.session.lock().await.clone()
    }

    async fn token(&self, rejected: Option<&str>) -> Result<String> {
        let mut session = self.inner.session.lock().await;
        let stale = session.access_token.is_empty()
            || (session.expires_at > 0 && session.expires_at <= now() + 120)
            || rejected.is_some_and(|token| token == session.access_token);
        if !stale {
            return Ok(session.access_token.clone());
        }
        if !session.can_refresh() {
            return Err(Error::Unauthorized);
        }
        let response = self
            .inner
            .http
            .post(&self.inner.auth_url)
            .header(header::ORIGIN, "https://vk.ru")
            .header(header::REFERER, "https://vk.ru/")
            .form(&[("version", "1"), ("app_id", self.inner.app_id.as_str())])
            .send()
            .await?;
        if response.status().is_redirection() {
            return Err(Error::Unauthorized);
        }
        let value: Value = response.error_for_status()?.json().await?;
        let data = &value["data"];
        let token = data["access_token"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or(Error::Unauthorized)?;
        session.access_token = token.into();
        // The web endpoint returns an absolute UNIX timestamp, not a TTL.
        session.expires_at = data["expires"].as_u64().unwrap_or(0);
        session.user_id = data["user_id"].as_i64().or(session.user_id);
        Ok(session.access_token.clone())
    }

    async fn throttle(&self) {
        let mut next = self.inner.throttle.lock().await;
        tokio::time::sleep_until(*next).await;
        *next = tokio::time::Instant::now() + self.inner.interval;
    }

    /// Escape hatch for read methods not yet wrapped by this crate.
    /// Unknown/write methods are sent once; automatic retries are limited to known reads.
    pub async fn request<T: DeserializeOwned>(
        &self,
        method: &str,
        params: &[(&str, String)],
    ) -> Result<T> {
        if method.is_empty()
            || !method
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'_')
        {
            return Err(Error::InvalidInput("имя метода API".into()));
        }
        if params
            .iter()
            .any(|(k, _)| ["access_token", "v", "client_id"].contains(k))
        {
            return Err(Error::InvalidInput(
                "зарезервированный параметр авторизации".into(),
            ));
        }
        let read = matches!(
            method,
            "users.get"
                | "audio.get"
                | "audio.search"
                | "audio.getById"
                | "audio.getPlaylists"
                | "audio.getLyrics"
                | "catalog.getAudio"
                | "catalog.getAudioSearch"
                | "catalog.getSection"
                | "catalog.getBlockItems"
                | "catalog.getAudioArtist"
                | "audio.searchArtists"
        );
        let mut token = self.token(None).await?;
        let mut refreshed = false;
        let mut retries = 0;
        loop {
            self.throttle().await;
            let mut form = params.to_vec();
            form.extend([
                ("access_token", token.clone()),
                ("v", self.inner.version.clone()),
                ("client_id", self.inner.app_id.clone()),
                ("lang", "ru".into()),
            ]);
            let response = self
                .inner
                .http
                .post(format!("{}{method}", self.inner.api_base))
                .header(header::ORIGIN, "https://vk.ru")
                .header(header::REFERER, "https://vk.ru/")
                .form(&form)
                .send()
                .await?;
            let status = response.status();
            if status.is_redirection() || status == reqwest::StatusCode::UNAUTHORIZED {
                return Err(Error::Unauthorized);
            }
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                if read && retries < 2 {
                    retries += 1;
                    tokio::time::sleep(Duration::from_secs(2 * retries)).await;
                    continue;
                }
                return Err(Error::RateLimited);
            }
            let value: Value = response.error_for_status()?.json().await?;
            if let Some(error) = value.get("error") {
                let code = error["error_code"].as_i64().unwrap_or(-1);
                if code == 5 && read && !refreshed {
                    token = self.token(Some(&token)).await?;
                    refreshed = true;
                    continue;
                }
                if code == 6 && read && retries < 2 {
                    retries += 1;
                    tokio::time::sleep(Duration::from_secs(2 * retries)).await;
                    continue;
                }
                return Err(match code {
                    5 => Error::Unauthorized,
                    6 => Error::RateLimited,
                    7 | 15 | 201 | 203 => Error::AccessDenied(code),
                    14 | 17 | 24 => Error::Challenge { code },
                    _ => Error::Api { code },
                });
            }
            return serde_json::from_value(
                value
                    .get("response")
                    .cloned()
                    .ok_or_else(|| Error::Protocol("нет response".into()))?,
            )
            .map_err(|_| Error::Protocol(format!("схема {method} изменилась")));
        }
    }
    pub async fn me(&self) -> Result<User> {
        let users: Vec<User> = self.request("users.get", &[]).await?;
        let user = users.into_iter().next().ok_or(Error::Unauthorized)?;
        self.inner.session.lock().await.user_id = Some(user.id);
        Ok(user)
    }
    pub async fn search(&self, query: &str, offset: u64, count: u16) -> Result<Page<Track>> {
        if query.trim().is_empty() {
            return Err(Error::InvalidInput("пустой поисковый запрос".into()));
        }
        self.request::<Page<Track>>(
            "audio.search",
            &[
                ("q", query.into()),
                ("offset", offset.to_string()),
                ("count", count.clamp(1, 300).to_string()),
            ],
        )
        .await
        .map(|page| page.with_pagination(offset, count.clamp(1, 300)))
    }
    pub async fn library(&self, owner: i64, offset: u64, count: u16) -> Result<Page<Track>> {
        self.request::<Page<Track>>(
            "audio.get",
            &[
                ("owner_id", owner.to_string()),
                ("offset", offset.to_string()),
                ("count", count.clamp(1, 300).to_string()),
            ],
        )
        .await
        .map(|page| page.with_pagination(offset, count.clamp(1, 300)))
    }
    pub async fn playlists(&self, owner: i64, offset: u64, count: u16) -> Result<Page<Playlist>> {
        self.request::<Page<Playlist>>(
            "audio.getPlaylists",
            &[
                ("owner_id", owner.to_string()),
                ("offset", offset.to_string()),
                ("count", count.clamp(1, 100).to_string()),
            ],
        )
        .await
        .map(|page| page.with_pagination(offset, count.clamp(1, 100)))
    }
    pub async fn playlist_tracks(
        &self,
        playlist: &Playlist,
        offset: u64,
        count: u16,
    ) -> Result<Page<Track>> {
        let mut params = vec![
            ("owner_id", playlist.owner_id.to_string()),
            ("album_id", playlist.id.to_string()),
            ("offset", offset.to_string()),
            ("count", count.clamp(1, 300).to_string()),
        ];
        if let Some(key) = &playlist.access_key {
            params.push(("access_key", key.clone()));
        }
        self.request::<Page<Track>>("audio.get", &params)
            .await
            .map(|page| page.with_pagination(offset, count.clamp(1, 300)))
    }
    pub async fn catalog(&self) -> Result<Catalog> {
        self.request("catalog.getAudio", &[("need_blocks", "1".into())])
            .await
    }
    /// Rich search used by vk.ru/music: tracks, playlists and catalog sections.
    pub async fn catalog_search(&self, query: &str) -> Result<Catalog> {
        if query.trim().is_empty() {
            return Err(Error::InvalidInput("пустой поисковый запрос".into()));
        }
        self.request(
            "catalog.getAudioSearch",
            &[("query", query.into()), ("need_blocks", "1".into())],
        )
        .await
    }
    pub async fn section(&self, id: &str, cursor: Option<&str>) -> Result<SectionPage> {
        let mut params = vec![("section_id", id.into())];
        if let Some(cursor) = cursor {
            params.push(("start_from", cursor.into()));
        }
        self.request("catalog.getSection", &params).await
    }
    /// Search actual artist identities, with offset pagination.
    pub async fn search_artists(
        &self,
        query: &str,
        offset: u64,
        count: u16,
    ) -> Result<Page<Artist>> {
        if query.trim().is_empty() {
            return Err(Error::InvalidInput("пустой поисковый запрос".into()));
        }
        let count = count.clamp(1, 100);
        self.request::<Page<Artist>>(
            "audio.searchArtists",
            &[
                ("q", query.into()),
                ("offset", offset.to_string()),
                ("count", count.to_string()),
            ],
        )
        .await
        .map(|page| page.with_pagination(offset, count))
    }
    /// Search all matching audio records attributed to this artist. VK's artist page
    /// only includes a small "Popular" shelf; the search result is filtered locally.
    /// `count` and `next_offset` still describe the raw search window.
    pub async fn artist_tracks(
        &self,
        artist: &Artist,
        offset: u64,
        count: u16,
    ) -> Result<Page<Track>> {
        let mut chars = artist.name.chars();
        let query = match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
            None => return Err(Error::InvalidInput("пустое имя исполнителя".into())),
        };
        let mut page = self.search(&query, offset, count).await?;
        // VK sometimes returns only one result on the first request for an
        // artist with many recordings. Repeat a suspiciously small first page.
        if offset == 0 && count >= 50 && page.count <= 5 {
            let retry = self.search(&query, offset, count).await?;
            if retry.count > page.count {
                page = retry;
            }
        }
        page.items.retain(|track| artist.matches_track(track));
        Ok(page)
    }
    /// Artist profile, popular tracks, releases and related catalog blocks.
    pub async fn artist(&self, id: &str) -> Result<Catalog> {
        if id.trim().is_empty() {
            return Err(Error::InvalidInput("пустой ID исполнителя".into()));
        }
        self.request(
            "catalog.getAudioArtist",
            &[("artist_id", id.into()), ("need_blocks", "1".into())],
        )
        .await
    }
    pub async fn block(&self, id: &str, cursor: Option<&str>, count: u16) -> Result<BlockPage> {
        let mut params = vec![
            ("block_id", id.into()),
            ("count", count.clamp(1, 100).to_string()),
        ];
        if let Some(cursor) = cursor {
            params.push(("start_from", cursor.into()));
        }
        self.request("catalog.getBlockItems", &params).await
    }
    pub async fn mixes(&self) -> Result<Vec<Mix>> {
        let catalog = self.catalog().await?;
        // Select playlist IDs from the algorithmic shelf, rather than every playlist on Home.
        let ids: std::collections::HashSet<_> = catalog
            .catalog
            .sections
            .iter()
            .flat_map(|s| &s.blocks)
            .filter(|b| b.title == "Собрано алгоритмами")
            .flat_map(|b| b.playlists_ids.iter().cloned())
            .collect();
        let mut mixes: Vec<_> = catalog
            .audio_stream_mixes
            .into_iter()
            .map(Mix::Stream)
            .collect();
        mixes.extend(
            catalog
                .playlists
                .into_iter()
                .filter(|p| ids.contains(&format!("{}_{}", p.owner_id, p.id)))
                .map(Mix::Playlist),
        );
        Ok(mixes)
    }
    /// Fetch the next stream batch. The server owns the stream position: no offset or cursor.
    /// Deliberately not retried automatically, as another request can advance the mix.
    pub async fn stream_mix(&self, id: &str) -> Result<Vec<Track>> {
        if id.trim().is_empty() {
            return Err(Error::InvalidInput("пустой ID микса".into()));
        }
        self.request("audio.getStreamMixAudios", &[("mix_id", id.into())])
            .await
    }
    /// Refreshes the signed media URL immediately before playback; never rewrites HLS URLs.
    pub async fn resolve(&self, track: &Track) -> Result<Track> {
        let tracks: Vec<Track> = self
            .request("audio.getById", &[("audios", track.api_id())])
            .await?;
        let track = tracks
            .into_iter()
            .find(|v| v.key() == track.key())
            .ok_or(Error::Unavailable)?;
        let url = url::Url::parse(&track.url).map_err(|_| Error::Unavailable)?;
        if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
            return Err(Error::Unavailable);
        }
        Ok(track)
    }
    pub async fn lyrics(&self, id: i64) -> Result<String> {
        let value: Value = self
            .request("audio.getLyrics", &[("lyrics_id", id.to_string())])
            .await?;
        value["text"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::Protocol("нет текста песни".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    async fn server(replies: Vec<Value>) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for reply in replies {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut chunk = [0; 2048];
                loop {
                    let n = stream.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&bytes[..end]);
                        let length = header
                            .lines()
                            .find_map(|line| {
                                let (key, value) = line.split_once(':')?;
                                key.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8(bytes).unwrap());
                let body = reply.to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
            requests
        });
        (base, task)
    }

    fn local(session: Session, base: String) -> Client {
        Client {
            inner: Arc::new(Inner {
                http: reqwest::Client::builder().no_proxy().build().unwrap(),
                session: Mutex::new(session),
                throttle: Mutex::new(tokio::time::Instant::now()),
                version: "5.282".into(),
                app_id: "6287487".into(),
                interval: Duration::ZERO,
                auth_url: format!("{base}auth"),
                api_base: base,
            }),
        }
    }
    fn web_session() -> Session {
        Session::import(".vk.ru\tTRUE\t/\tTRUE\t0\tremixsid\tfake-cookie").unwrap()
    }

    #[tokio::test]
    async fn concurrent_reads_refresh_only_once() {
        let user = json!({"response":[{"id":42,"first_name":"Test"}]});
        let (base, task) = server(vec![json!({"type":"okay","data":{"access_token":"refreshed-token","expires":now()+3600,"user_id":42}}), user.clone(), user]).await;
        let client = local(web_session(), base);
        let (a, b) = tokio::join!(client.me(), client.me());
        assert_eq!(a.unwrap().id, 42);
        assert_eq!(b.unwrap().id, 42);
        let requests = task.await.unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.starts_with("POST /auth "))
                .count(),
            1
        );
        assert!(requests[1].contains("access_token=refreshed-token"));
        assert!(!requests[1].lines().next().unwrap().contains("token"));
    }

    #[tokio::test]
    async fn rejected_token_refreshes_then_retries_read() {
        let (base, task) = server(vec![
            json!({"error":{"error_code":5}}),
            json!({"data":{"access_token":"new","expires":now()+3600}}),
            json!({"response":{"count":0,"items":[]}}),
        ])
        .await;
        let mut session = web_session();
        session.access_token = "old".into();
        let page = local(session, base)
            .search("Кино & рок", 0, 20)
            .await
            .unwrap();
        assert_eq!(page.count, 0);
        let requests = task.await.unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests[2].contains("access_token=new"));
        assert!(requests[2].contains("%26"));
    }

    #[tokio::test]
    async fn challenges_are_typed_and_not_retried() {
        let (base, task) = server(vec![
            json!({"error":{"error_code":14,"captcha_sid":"hidden"}}),
        ])
        .await;
        let error = local(Session::token("token").unwrap(), base)
            .me()
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Challenge { code: 14 }));
        assert!(!error.to_string().contains("hidden"));
        assert_eq!(task.await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn unknown_mutation_is_not_retried() {
        let (base, task) = server(vec![json!({"error":{"error_code":6}})]).await;
        let result: Result<Value> = local(Session::token("token").unwrap(), base)
            .request("audio.add", &[])
            .await;
        assert!(matches!(result, Err(Error::RateLimited)));
        assert_eq!(task.await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn pager_advances_over_omitted_records_and_stops_without_extra_request() {
        let (base, task) = server(vec![
            json!({"response":{"count":4,"items":[{"id":1,"owner_id":42}]}}),
            json!({"response":{"count":4,"items":[{"id":4,"owner_id":42}]}}),
        ])
        .await;
        let client = local(Session::token("token").unwrap(), base);
        let mut pages = client.library_pages(42).page_size(3);
        assert_eq!(pages.next_page().await.unwrap().unwrap().items[0].id, 1);
        assert_eq!(pages.next_page().await.unwrap().unwrap().items[0].id, 4);
        assert!(pages.next_page().await.unwrap().is_none());
        let requests = task.await.unwrap();
        assert!(requests[0].contains("offset=0"));
        assert!(requests[1].contains("offset=3"));
    }
}
