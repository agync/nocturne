use nocturne_engine::MindError;

/// Everything that can go wrong while running, reaching or using the private engine.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeError {
    #[error("the engine bundle was not found: {0}")]
    BundleMissing(String),
    #[error("file system problem: {0}")]
    Io(String),
    #[error("the engine did not start: {0}")]
    Start(String),
    #[error("no free port after {0} tries")]
    NoPort(u32),
    #[error("the engine is not running")]
    Unavailable,
    #[error("the engine refused the key")]
    Unauthorized,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("the engine failed ({status}): {message}")]
    Server { status: u16, message: String },
    #[error("the engine rejected the request ({status} {code}): {message}")]
    Rejected {
        status: u16,
        code: String,
        message: String,
    },
    #[error("the engine took too long")]
    Timeout,
    #[error("the answer was cut off")]
    StreamCut,
    #[error("the engine sent something unexpected: {0}")]
    BadResponse(String),
    #[error("the engine is offline by policy")]
    Offline,
    #[error("{0} is not on this machine")]
    NotInstalled(String),
}

impl From<std::io::Error> for RuntimeError {
    fn from(e: std::io::Error) -> Self {
        RuntimeError::Io(e.to_string())
    }
}

impl From<RuntimeError> for MindError {
    fn from(e: RuntimeError) -> Self {
        match e {
            RuntimeError::Unavailable
            | RuntimeError::Unauthorized
            | RuntimeError::Timeout
            | RuntimeError::StreamCut
            | RuntimeError::Io(_)
            | RuntimeError::Start(_)
            | RuntimeError::BundleMissing(_)
            | RuntimeError::NoPort(_)
            | RuntimeError::NotFound(_)
            | RuntimeError::NotInstalled(_)
            | RuntimeError::Offline => MindError::Unavailable,
            RuntimeError::Rejected { status: 409, .. } => MindError::Busy,
            other => MindError::Failed(other.to_string()),
        }
    }
}
