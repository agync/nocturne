//! The engine error type.

use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("store error: {0}")]
    Store(#[from] rusqlite::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("time error: {0}")]
    Time(#[from] jiff::Error),
    #[error("encoding error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid request: {0}")]
    Invalid(String),
    /// The forget token is unknown, used, expired, or what it covers has changed since.
    #[error("the forget preview is no longer valid")]
    StaleToken,
}

impl Error {
    pub(crate) fn invalid(msg: impl fmt::Display) -> Self {
        Error::Invalid(msg.to_string())
    }

    pub(crate) fn not_found(what: impl fmt::Display) -> Self {
        Error::NotFound(what.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
