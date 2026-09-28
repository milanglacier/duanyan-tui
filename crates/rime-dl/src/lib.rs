//! Runtime-loaded bindings to librime.
//!
//! The library is opened with `dlopen` (see [`Library`]) so the binary does
//! not link against librime and the path can be chosen at runtime.

pub mod ffi;
pub mod keysym;

mod api;
mod loader;

use std::path::PathBuf;

pub use api::{
    Candidate, Composition, Config, Context, Menu, Notification, NotificationHandler, Rime,
    SchemaListItem, Session, Status, Traits,
};
pub use keysym::RimeKey;
pub use loader::{Library, MIN_VERSION, default_library_name};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to load librime from {path}: {reason}")]
    Load { path: PathBuf, reason: String },
    #[error("librime {found} is too old; {required} or newer is required")]
    Version { found: String, required: String },
    #[error("librime does not provide `{0}`")]
    Missing(&'static str),
    #[error("string contains a NUL byte: {0:?}")]
    InvalidString(String),
    #[error("librime failed to create a session")]
    Session,
}
