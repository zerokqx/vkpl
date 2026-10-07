//! Typed asynchronous VK Music API. See `Client`, `Session` and `Page`.
//! Web endpoints are private and can change independently of this crate.
mod auth;
mod client;
mod error;
mod models;
mod pager;

pub use auth::{Cookie, Session};
pub use client::{Client, ClientBuilder};
pub use error::{Error, Result};
pub use models::*;
pub use pager::TrackPager;
