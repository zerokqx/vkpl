use vk_music::{Client, Session};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: search /path/to/session.json [query]")?;
    let query = std::env::args().nth(2).unwrap_or_else(|| "Кино".into());
    let session: Session = serde_json::from_slice(&std::fs::read(path)?)?;
    let client = Client::new(session)?;
    let page = client.search(&query, 0, 20).await?;
    for track in &page.items {
        println!(
            "{} · {} — {} ({} s)",
            track.key(),
            track.artist,
            track.title,
            track.duration
        );
    }
    if let Some(offset) = page.next_offset() {
        println!("Next page offset: {offset}");
    }
    Ok(())
}
