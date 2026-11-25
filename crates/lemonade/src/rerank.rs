//! Reordering what the search found by how well each memory answers the question, with the
//! engine's reranking model (`POST /v1/rerank`). Standard and Large only, and only once the
//! model is fully on disk: the engine fetches a missing model on first use, so nothing is sent
//! before [`Reranker::present`] says it is there.

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Method;
use serde::Deserialize;
use serde_json::json;

use crate::client::Client;
use crate::error::RuntimeError;

/// A first call loads the model, which takes a few seconds on the tiers that have it.
const TIMEOUT: Duration = Duration::from_secs(20);

fn http() -> &'static reqwest::Client {
    static HTTP: OnceLock<reqwest::Client> = OnceLock::new();
    HTTP.get_or_init(|| {
        crate::tls::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(TIMEOUT)
            .no_proxy()
            .build()
            .expect("http client")
    })
}

#[derive(Debug, Clone)]
pub struct Reranker {
    client: Client,
    model: String,
}

#[derive(Deserialize)]
struct Scored {
    index: usize,
    relevance_score: f32,
}

#[derive(Deserialize)]
struct Scores {
    results: Vec<Scored>,
}

impl Reranker {
    pub fn new(client: Client, model: impl Into<String>) -> Reranker {
        Reranker {
            client,
            model: model.into(),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Whether the model is fully on disk.
    pub async fn present(&self) -> bool {
        self.client
            .models()
            .await
            .is_ok_and(|m| m.iter().any(|m| m.id == self.model && m.present()))
    }

    /// One score for each of `documents`, in their order; higher is more relevant. Only the
    /// private engine on loopback is asked, with its per-launch key.
    pub async fn scores(
        &self,
        query: &str,
        documents: &[String],
    ) -> Result<Vec<f32>, RuntimeError> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }
        let endpoint = self.client.endpoint().ok_or(RuntimeError::Unavailable)?;
        let rb = http()
            .request(
                Method::POST,
                format!("http://127.0.0.1:{}/v1/rerank", endpoint.port),
            )
            .bearer_auth(&endpoint.key)
            .json(&json!({"model": self.model, "query": query, "documents": documents}));
        let reply: Scores = Client::json(rb).await?;
        let mut out = vec![f32::NEG_INFINITY; documents.len()];
        let mut seen = vec![false; documents.len()];
        for s in reply.results {
            match seen.get_mut(s.index) {
                Some(seen) if !*seen && s.relevance_score.is_finite() => {
                    *seen = true;
                    out[s.index] = s.relevance_score;
                }
                _ => {
                    return Err(RuntimeError::BadResponse(
                        "a score for a document that was not sent".into(),
                    ));
                }
            }
        }
        if seen.iter().any(|s| !s) {
            return Err(RuntimeError::BadResponse(format!(
                "asked to score {} documents, some came back without one",
                documents.len()
            )));
        }
        Ok(out)
    }
}
