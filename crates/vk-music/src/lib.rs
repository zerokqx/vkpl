//! Typed asynchronous VK Music API. See `Client`, `Session` and `Page`.
//! Web endpoints are private and can change independently of this crate.
mod client;
mod pager;

pub use client::{Client, ClientBuilder};
pub use pager::TrackPager;
pub use vk_music_core::*;
