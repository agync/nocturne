//! Model downloads for a job the person changed, on the engine's pull and downloads APIs, inside
//! the shared download window. Not yet in use.

use std::sync::Arc;

use tokio::sync::watch;

use crate::client::Client;
use crate::error::RuntimeError;
use crate::supervisor::OnlineHold;

/// A download's progress, in bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobProgress {
    pub received: u64,
    pub total: u64,
    pub paused: bool,
    pub done: bool,
    pub failed: bool,
}

/// Called once when a download finishes: the model and its bytes.
pub type OnDownload = Arc<dyn Fn(&str, u64) + Send + Sync>;

pub struct ModelJobs {
    client: Client,
}

impl ModelJobs {
    pub fn new(client: Client) -> Self {
        ModelJobs { client }
    }

    pub async fn get(
        &self,
        model: &str,
        hold: OnlineHold,
        on_download: OnDownload,
    ) -> Result<watch::Receiver<JobProgress>, RuntimeError> {
        let _ = (&self.client, model, hold, on_download);
        Err(RuntimeError::Unavailable)
    }

    pub async fn pause(&self) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unavailable)
    }

    pub async fn resume(&self) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unavailable)
    }

    /// Cancels the download and removes what was received.
    pub async fn cancel(&self) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unavailable)
    }

    /// Deletes a model from disk and returns the bytes freed.
    pub async fn remove(&self, model: &str) -> Result<u64, RuntimeError> {
        let _ = model;
        Err(RuntimeError::Unavailable)
    }
}
