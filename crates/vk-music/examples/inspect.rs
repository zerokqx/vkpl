//! Read-only live compatibility check. Never prints credentials or signed URLs.
use vk_music::{Client, Session};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: inspect /path/to/session.json")?;
    let session: Session = serde_json::from_slice(&std::fs::read(path)?)?;
    let client = Client::new(session)?;
    let user = client.me().await?;
    println!("users.get: OK");
    let library = client.library(user.id, 0, 3).await?;
    println!("audio.get: {} / {}", library.items.len(), library.count);
    let search = client.search("Кино", 0, 3).await?;
    println!("audio.search: {} / {}", search.items.len(), search.count);
    let playlists = client.playlists(user.id, 0, 3).await?;
    println!(
        "audio.getPlaylists: {} / {}",
        playlists.items.len(),
        playlists.count
    );
    let catalog = client.catalog().await?;
    println!(
        "catalog.getAudio: {} tracks, {} playlists, {} sections",
        catalog.audios.len(),
        catalog.playlists.len(),
        catalog.catalog.sections.len()
    );
    if !catalog.catalog.default_section.is_empty() {
        let section = client
            .section(&catalog.catalog.default_section, None)
            .await?;
        println!("catalog.getSection: {} tracks", section.audios.len());
    }
    if let Some(playlist) = catalog.playlists.first() {
        let page = client.playlist_tracks(playlist, 0, 3).await?;
        println!(
            "audio.get (playlist): {} / {}",
            page.items.len(),
            page.count
        );
    }
    if let Some(track) = library
        .items
        .iter()
        .chain(search.items.iter())
        .find(|t| !t.url.is_empty() && !t.is_restricted())
    {
        let resolved = client.resolve(track).await?;
        println!(
            "audio.getById: URL present, duration {} s",
            resolved.duration
        );
        let response = reqwest::Client::new()
            .get(&resolved.url)
            .header("Range", "bytes=0-1023")
            .send()
            .await
            .map_err(|error| error.without_url())?;
        println!("media URL: HTTP {}", response.status());
        response
            .error_for_status()
            .map_err(|error| error.without_url())?;
    }
    Ok(())
}
