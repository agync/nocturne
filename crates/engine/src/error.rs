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
    /// The worker no longer holds the read of this item: its lease ran out or was taken by
    /// another, the item could not be read, or it is part of a forget.
    #[error("the lease on this item is gone")]
    LeaseLost,
    /// The forget token is unknown, used, expired, or what it covers has changed since.
    #[error("the forget preview is no longer valid")]
    StaleToken,
    /// Something was forgotten or undone while the work ran, so its result was thrown away.
    #[error("something changed while this ran")]
    Busy,
    /// The window for undoing a keep has passed.
    #[error("it is too late to undo this")]
    TooLate,
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
