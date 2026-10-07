use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId {
    pub owner_id: i64,
    pub id: i64,
}

impl fmt::Display for TrackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}_{}", self.owner_id, self.id)
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Track {
    pub id: i64,
    pub owner_id: i64,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub main_artists: Vec<Artist>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub access_key: Option<String>,
    #[serde(default)]
    pub album: Option<Album>,
    #[serde(default)]
    pub is_explicit: bool,
    #[serde(default)]
    pub lyrics_id: Option<i64>,
    #[serde(default)]
    pub content_restricted: Option<u32>,
}

impl Track {
    pub fn is_restricted(&self) -> bool {
        self.content_restricted.is_some_and(|code| code != 0)
    }
    pub fn key(&self) -> TrackId {
        TrackId {
            owner_id: self.owner_id,
            id: self.id,
        }
    }
    pub fn api_id(&self) -> String {
        match &self.access_key {
            Some(key) if !key.is_empty() => format!("{}_{}", self.key(), key),
            _ => self.key().to_string(),
        }
    }
}

impl fmt::Debug for Track {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Track")
            .field("id", &self.key())
            .field("artist", &self.artist)
            .field("title", &self.title)
            .field("duration", &self.duration)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Album {
    pub id: i64,
    #[serde(default)]
    pub title: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Playlist {
    pub id: i64,
    pub owner_id: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub access_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Page<T> {
    pub count: u64,
    pub items: Vec<T>,
    #[serde(skip)]
    next: Option<u64>,
}

impl<T> Page<T> {
    pub fn next_offset(&self) -> Option<u64> {
        self.next
    }
    pub fn complete(items: Vec<T>) -> Self {
        Self {
            count: items.len() as u64,
            items,
            next: None,
        }
    }
    pub(crate) fn with_pagination(mut self, offset: u64, requested: u16) -> Self {
        // VK can omit removed/inaccessible records from items while still consuming their offsets.
        let next = offset.saturating_add(u64::from(requested));
        self.next = (requested > 0 && next < self.count).then_some(next);
        self
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    #[serde(default)]
    pub artists: Vec<Artist>,
    #[serde(default)]
    pub audio_stream_mixes: Vec<StreamMix>,
    #[serde(default)]
    pub catalog: CatalogSections,
    #[serde(default)]
    pub audios: Vec<Track>,
    #[serde(default)]
    pub playlists: Vec<Playlist>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogSections {
    #[serde(default)]
    pub default_section: String,
    #[serde(default)]
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Section {
    #[serde(default)]
    pub blocks: Vec<CatalogBlock>,
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub next_from: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SectionPage {
    pub section: Section,
    #[serde(default)]
    pub audios: Vec<Track>,
    #[serde(default)]
    pub playlists: Vec<Playlist>,
}

/// A VK music artist identity, separate from the free-form track artist label.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub bio: String,
}

impl Artist {
    /// Match results from audio.search, which frequently omit `main_artists`.
    /// The free-form artist label is a fallback and may include collaborations.
    pub fn matches_track(&self, track: &Track) -> bool {
        if !track.main_artists.is_empty() {
            return track.main_artists.iter().any(|a| a.id == self.id);
        }
        let name = self.name.trim().to_lowercase();
        if name.is_empty() {
            return false;
        }
        track
            .artist
            .to_lowercase()
            .split([',', '&', ';', '/'])
            .map(str::trim)
            .any(|part| {
                part == name
                    || part.strip_prefix(&name).is_some_and(|rest| {
                        [" feat", " ft", " x ", " и "]
                            .iter()
                            .any(|marker| rest.starts_with(marker))
                    })
            })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogBlock {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub data_type: String,
    #[serde(default)]
    pub next_from: Option<String>,
    #[serde(default)]
    pub audios_ids: Vec<String>,
    #[serde(default)]
    pub playlists_ids: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BlockPage {
    pub block: CatalogBlock,
    #[serde(default)]
    pub audios: Vec<Track>,
    #[serde(default)]
    pub playlists: Vec<Playlist>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMix {
    pub id: String,
    #[serde(default)]
    pub description: String,
    pub stream_mix: StreamMixIdentity,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMixIdentity {
    pub id: String,
    pub title: String,
}

/// Personalized stream plus algorithmic playlists supplied by the VK catalog.
#[derive(Clone)]
pub enum Mix {
    Stream(StreamMix),
    Playlist(Playlist),
}
impl Mix {
    pub fn title(&self) -> &str {
        match self {
            Self::Stream(m) => &m.stream_mix.title,
            Self::Playlist(p) => &p.title,
        }
    }
    pub fn description(&self) -> &str {
        match self {
            Self::Stream(m) => &m.description,
            Self::Playlist(p) => &p.description,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pagination_uses_requested_window_even_when_vk_omits_items() {
        assert_eq!(
            Page {
                count: 100,
                items: vec![1, 2],
                next: None
            }
            .with_pagination(20, 10)
            .next_offset(),
            Some(30)
        );
        assert_eq!(
            Page {
                count: 22,
                items: vec![1, 2],
                next: None
            }
            .with_pagination(20, 10)
            .next_offset(),
            None
        );
        assert_eq!(
            Page::<Track> {
                count: 100,
                items: vec![],
                next: None
            }
            .with_pagination(20, 10)
            .next_offset(),
            Some(30)
        );
    }
    #[test]
    fn artist_search_matches_vk_tracks_without_main_artist_ids() {
        let artist = Artist {
            id: "42".into(),
            name: "лиззз".into(),
            ..Default::default()
        };
        let mut track = Track {
            artist: "лиззз feat. нэнси холод".into(),
            ..Default::default()
        };
        assert!(artist.matches_track(&track));
        track.artist = "Только Лиззз".into();
        assert!(!artist.matches_track(&track));
        track.main_artists.push(Artist {
            id: "43".into(),
            name: "лиззз".into(),
            ..Default::default()
        });
        assert!(!artist.matches_track(&track));
        track.main_artists[0].id = "42".into();
        assert!(artist.matches_track(&track));
    }
    #[test]
    fn signed_url_is_redacted_from_debug() {
        let track: Track = serde_json::from_str(r#"{"id":1,"owner_id":-42,"artist":"Кино","title":"Тест","url":"https://example.com/?secret=hidden","access_key":"private"}"#).unwrap();
        assert_eq!(track.api_id(), "-42_1_private");
        assert!(!format!("{track:?}").contains("hidden"));
        assert!(!format!("{track:?}").contains("private"));
    }
}
