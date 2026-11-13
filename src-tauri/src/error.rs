//! What a failed command looks like to the interface, and the guards that make sure every
//! command ends in a value or a `CommandError`: never a crash, never a promise left pending.

use std::future::Future;

use serde::Serialize;

/// The reason a command failed, in the words the interface switches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    NotFound,
    Invalid,
    Busy,
    Unavailable,
    TokenExpired,
    /// Too late to undo: the moment for it has passed.
    TooLate,
    Internal,
}

/// A failed command, serialised as JSON (`{ "code": "notFound", "detail": null }`). The detail
/// is for developers and is only filled in debug builds, so a release build never leaks a
/// path or a sentence of the person's into an error.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CommandError {
    pub code: ErrorCode,
    pub detail: Option<String>,
}

impl CommandError {
    pub fn new(code: ErrorCode, detail: impl std::fmt::Display) -> Self {
        CommandError {
            code,
            detail: cfg!(debug_assertions).then(|| detail.to_string()),
        }
    }

    pub fn invalid(detail: impl std::fmt::Display) -> Self {
        Self::new(ErrorCode::Invalid, detail)
    }

    pub fn not_found(detail: impl std::fmt::Display) -> Self {
        Self::new(ErrorCode::NotFound, detail)
    }

    pub fn unavailable(detail: impl std::fmt::Display) -> Self {
        Self::new(ErrorCode::Unavailable, detail)
    }

    pub fn internal(detail: impl std::fmt::Display) -> Self {
        Self::new(ErrorCode::Internal, detail)
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.code)?;
        if let Some(d) = &self.detail {
            write!(f, ": {d}")?;
        }
        Ok(())
    }
}

impl From<nocturne_engine::Error> for CommandError {
    fn from(e: nocturne_engine::Error) -> Self {
        use nocturne_engine::Error as E;
        match &e {
            E::NotFound(_) => Self::new(ErrorCode::NotFound, &e),
            E::Invalid(_) => Self::new(ErrorCode::Invalid, &e),
            E::StaleToken => Self::new(ErrorCode::TokenExpired, &e),
            E::LeaseLost => Self::new(ErrorCode::Busy, &e),
            #[cfg(feature = "engine-round2")]
            E::TooLate => Self::new(ErrorCode::TooLate, &e),
            E::Io(io) if io.kind() == std::io::ErrorKind::NotFound => {
                Self::new(ErrorCode::NotFound, &e)
            }
            E::Store(rusqlite_err) if is_busy(rusqlite_err) => Self::new(ErrorCode::Busy, &e),
            E::Store(_) | E::Io(_) | E::Time(_) | E::Json(_) => {
                tracing::error!("engine error: {e}");
                Self::new(ErrorCode::Internal, &e)
            }
        }
    }
}

fn is_busy(e: &dyn std::error::Error) -> bool {
    // The store is locked by someone else (another process, a long write): worth retrying.
    let text = e.to_string().to_lowercase();
    text.contains("database is locked") || text.contains("database table is locked")
}

fn panicked(e: tokio::task::JoinError) -> CommandError {
    if e.is_panic() {
        tracing::error!("a command panicked");
    }
    CommandError::internal(if e.is_panic() {
        "the command panicked"
    } else {
        "the command was cancelled"
    })
}

/// Run engine work (synchronous, possibly slow) off the async threads. An engine error maps
/// to a `CommandError`; a panic inside maps to `internal`.
pub async fn blocking<T, F>(f: F) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, nocturne_engine::Error> + Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(r) => r.map_err(CommandError::from),
        Err(e) => Err(panicked(e)),
    }
}

/// Like [`blocking`] for work that already speaks `CommandError`.
pub async fn blocking_cmd<T, F>(f: F) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CommandError> + Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(r) => r,
        Err(e) => Err(panicked(e)),
    }
}

/// Run a whole command body on its own task, so a panic anywhere in it becomes `internal`
/// instead of a promise that never settles.
pub async fn guard<T, Fut>(fut: Fut) -> Result<T, CommandError>
where
    T: Send + 'static,
    Fut: Future<Output = Result<T, CommandError>> + Send + 'static,
{
    match tokio::spawn(fut).await {
        Ok(r) => r,
        Err(e) => Err(panicked(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocturne_engine::Error;

    #[test]
    fn engine_errors_map_to_codes() {
        let code = |e: Error| CommandError::from(e).code;
        assert_eq!(code(Error::NotFound("x".into())), ErrorCode::NotFound);
        assert_eq!(code(Error::Invalid("x".into())), ErrorCode::Invalid);
        assert_eq!(code(Error::StaleToken), ErrorCode::TokenExpired);
        assert_eq!(code(Error::LeaseLost), ErrorCode::Busy);
        assert_eq!(
            code(Error::Io(std::io::ErrorKind::PermissionDenied.into())),
            ErrorCode::Internal
        );
        assert_eq!(
            code(Error::Io(std::io::ErrorKind::NotFound.into())),
            ErrorCode::NotFound
        );
    }

    #[test]
    fn serialises_as_camel_case_code() {
        let e = CommandError::not_found("memory m1");
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["code"], "notFound");
        let t = CommandError::from(Error::StaleToken);
        assert_eq!(serde_json::to_value(&t).unwrap()["code"], "tokenExpired");
    }

    #[tokio::test]
    async fn a_panic_in_blocking_work_is_internal() {
        let r: Result<(), _> = blocking(|| panic!("boom")).await;
        assert_eq!(r.unwrap_err().code, ErrorCode::Internal);
        let r: Result<(), _> = blocking_cmd(|| panic!("boom")).await;
        assert_eq!(r.unwrap_err().code, ErrorCode::Internal);
    }

    #[tokio::test]
    async fn a_panic_in_a_command_body_is_internal() {
        let r: Result<u8, _> = guard(async { panic!("boom") }).await;
        assert_eq!(r.unwrap_err().code, ErrorCode::Internal);
    }

    #[tokio::test]
    async fn an_engine_error_in_blocking_work_maps() {
        let r: Result<(), _> = blocking(|| Err(Error::Invalid("no".into()))).await;
        assert_eq!(r.unwrap_err().code, ErrorCode::Invalid);
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn release_builds_carry_no_detail() {
        assert!(CommandError::invalid("secret").detail.is_none());
    }
}
