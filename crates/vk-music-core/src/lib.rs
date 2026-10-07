//! Transport-independent domain types and session handling for VK Music.
//!
//! This crate intentionally has no async runtime or HTTP dependency. Applications that
//! need the private VK web API can use `vk-music`, which re-exports these types.
mod auth;
mod error;
mod models;

pub use auth::{Cookie, Session};
pub use error::{Error, Result};
pub use models::*;
