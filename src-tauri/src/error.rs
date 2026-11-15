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
    /// The person said no in the app's own window.
    Declined,
    TokenExpired,
    /// Too late to undo: the moment for it has passed.
    TooLate,
    /// The passphrase does not open the backup.
    WrongPassphrase,
    /// The file is not a Nocturne backup.
    NotABackup,
    /// The backup was made by a newer Nocturne than this one.
    NewerBackup,
    /// The backup is damaged or incomplete.
    DamagedBackup,
    /// The person stopped the work.
    Cancelled,
    /// There is not enough room on that drive.
    NoRoom,
    /// That folder does not let Nocturne write.
    CantWrite,
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

    /// A code with nothing beside it: a failure whose text could name a file or repeat what the
    /// person typed.
    fn bare(code: ErrorCode) -> Self {
        CommandError { code, detail: None }
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
            E::LeaseLost | E::Busy => Self::new(ErrorCode::Busy, &e),
            E::TooLate => Self::new(ErrorCode::TooLate, &e),
            E::WrongPassphrase => Self::bare(ErrorCode::WrongPassphrase),
            E::NotABackup => Self::bare(ErrorCode::NotABackup),
            E::BackupTooNew => Self::bare(ErrorCode::NewerBackup),
            E::BackupDamaged => Self::bare(ErrorCode::DamagedBackup),
            E::Cancelled => Self::bare(ErrorCode::Cancelled),
            E::Io(io)
                if matches!(
                    io.kind(),
                    std::io::ErrorKind::StorageFull | std::io::ErrorKind::FileTooLarge
                ) =>
            {
                Self::bare(ErrorCode::NoRoom)
            }
            E::Io(io)
                if matches!(
                    io.kind(),
                    std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem
                ) =>
            {
                Self::bare(ErrorCode::CantWrite)
            }
            E::Io(io) if io.kind() == std::io::ErrorKind::NotFound => {
                Self::new(ErrorCode::NotFound, &e)
            }
            E::Store(rusqlite_err) if is_busy(rusqlite_err) => Self::new(ErrorCode::Busy, &e),
            E::Store(_) | E::Io(_) | E::Time(_) | E::Json(_) => {
                // Kinds only: the text of an engine error can carry a path or a sentence.
                tracing::error!(
                    "engine error: {}",
                    match &e {
                        E::Store(_) => "store",
                        E::Io(_) => "file",
                        E::Time(_) => "time",
                        _ => "encoding",
                    }
                );
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
        assert_eq!(code(Error::Busy), ErrorCode::Busy);
        assert_eq!(code(Error::TooLate), ErrorCode::TooLate);
        assert_eq!(code(Error::WrongPassphrase), ErrorCode::WrongPassphrase);
        assert_eq!(code(Error::NotABackup), ErrorCode::NotABackup);
        assert_eq!(code(Error::BackupTooNew), ErrorCode::NewerBackup);
        assert_eq!(code(Error::BackupDamaged), ErrorCode::DamagedBackup);
        assert_eq!(code(Error::Cancelled), ErrorCode::Cancelled);
        assert_eq!(
            code(Error::Io(std::io::ErrorKind::StorageFull.into())),
            ErrorCode::NoRoom
        );
        assert_eq!(
            code(Error::Io(std::io::ErrorKind::FileTooLarge.into())),
            ErrorCode::NoRoom
        );
        assert_eq!(
            code(Error::Io(std::io::ErrorKind::PermissionDenied.into())),
            ErrorCode::CantWrite
        );
        assert_eq!(
            code(Error::Io(std::io::ErrorKind::ReadOnlyFilesystem.into())),
            ErrorCode::CantWrite
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
        // Backup failures carry no text: no file name and no passphrase can leak through.
        for (e, want) in [
            (Error::WrongPassphrase, "wrongPassphrase"),
            (Error::NotABackup, "notABackup"),
            (Error::BackupTooNew, "newerBackup"),
            (Error::BackupDamaged, "damagedBackup"),
            (Error::Cancelled, "cancelled"),
        ] {
            let mapped = CommandError::from(e);
            assert!(mapped.detail.is_none(), "{want}");
            assert_eq!(serde_json::to_value(&mapped).unwrap()["code"], want);
        }
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
