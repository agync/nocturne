//! [`OnlineMind`]: the answer to one question written by the person's provider through the engine
//! route. It is not a [`nocturne_engine::Mind`] and holds no mind on this computer: it has one
//! `answer` method, called only by Ask, so no other job can reach the route.

use std::sync::Arc;

use nocturne_engine::{AnswerMeta, Fact, MindError};
use tokio::sync::mpsc;

use crate::client::Client;

/// Whether each memory id may still be sent, asked again just before the body is built.
pub type Sendable = Arc<dyn Fn(&[String]) -> Vec<bool> + Send + Sync>;

pub struct OnlineMind {
    client: Client,
    sendable: Sendable,
}

impl OnlineMind {
    pub fn new(client: Client, sendable: Sendable) -> Self {
        OnlineMind { client, sendable }
    }

    /// Not written online yet.
    pub async fn answer(
        &self,
        question: &str,
        facts: &[Fact],
        out: mpsc::Sender<String>,
    ) -> Result<AnswerMeta, MindError> {
        let _ = (&self.client, &self.sendable, question, facts, out);
        Err(MindError::Unavailable)
    }
}
