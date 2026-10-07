use crate::{Client, Page, Playlist, Result, Track};

enum Source {
    Search(String),
    Library(i64),
    Playlist(Playlist),
}

/// Lazy pagination. A failed request leaves the cursor intact and can be retried.
///
/// ```no_run
/// # async fn example(client: vk_music::Client) -> vk_music::Result<()> {
/// let mut pages = client.search_pages("Кино").page_size(100);
/// while let Some(page) = pages.next_page().await? {
///     for track in page.items { println!("{} — {}", track.artist, track.title); }
/// }
/// # Ok(())
/// # }
/// ```
pub struct TrackPager {
    client: Client,
    source: Source,
    offset: Option<u64>,
    count: u16,
}

impl TrackPager {
    fn new(client: Client, source: Source) -> Self {
        Self {
            client,
            source,
            offset: Some(0),
            count: 100,
        }
    }
    pub fn page_size(mut self, count: u16) -> Self {
        self.count = count.clamp(1, 300);
        self
    }
    pub async fn next_page(&mut self) -> Result<Option<Page<Track>>> {
        let Some(offset) = self.offset else {
            return Ok(None);
        };
        let page = match &self.source {
            Source::Search(query) => self.client.search(query, offset, self.count).await?,
            Source::Library(owner) => self.client.library(*owner, offset, self.count).await?,
            Source::Playlist(playlist) => {
                self.client
                    .playlist_tracks(playlist, offset, self.count)
                    .await?
            }
        };
        self.offset = page.next_offset();
        Ok(Some(page))
    }
}

impl Client {
    pub fn search_pages(&self, query: impl Into<String>) -> TrackPager {
        TrackPager::new(self.clone(), Source::Search(query.into()))
    }
    pub fn library_pages(&self, owner: i64) -> TrackPager {
        TrackPager::new(self.clone(), Source::Library(owner))
    }
    pub fn playlist_pages(&self, playlist: &Playlist) -> TrackPager {
        TrackPager::new(self.clone(), Source::Playlist(playlist.clone()))
    }
}
