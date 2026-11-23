//! [`OnlineMind`]: the answer to one question written by the person's provider through the engine
//! route. It is not a [`nocturne_engine::Mind`] and holds no mind on this computer: it has one
//! `answer` method (and its two halves, [`OnlineMind::prepare`] and [`OnlineMind::send`]),
//! called only by Ask, so no other job can reach the route.
//!
//! The question is written from the full instructions and up to [`MAX_FACTS`](crate::online::MAX_FACTS) facts, fenced with
//! a fresh nonce like a local answer, and judged by the same guards on this computer: a reply
//! whose receipts do not map back to a fact sent is "not sure". One request per question, never
//! retried.

use std::sync::Arc;
use std::time::{Duration, Instant};

use nocturne_contract::{CheckOutcome, SentLine};
use nocturne_engine::{AnswerMeta, Fact, MindError, OnlineFailure, OnlineReceipt};
use nocturne_mind::judge::{conclude, judge};
use nocturne_mind::online::{self, Question};
use tokio::sync::mpsc;

pub use nocturne_provider::client::check_outcome;

use crate::client::{Client, Message, OnlineRequest};
use crate::online::{CHECK_QUESTION, MAX_TOKENS, MODEL_ID, SWITCH_SECONDS, classify};

/// Whether each memory id may still be sent, asked again just before the body is built.
pub type Sendable = Arc<dyn Fn(&[String]) -> Vec<bool> + Send + Sync>;

pub struct OnlineMind {
    client: Client,
    sendable: Sendable,
}

/// A question ready to go: the exact body, the facts it numbers, and the receipt so far (what
/// is sent, as a template). Made before anything leaves, so a question cut short is still
/// logged as sent.
pub struct Prepared {
    request: OnlineRequest,
    system: String,
    facts: Vec<Fact>,
    /// How many of the facts handed in were dropped by the last look at what may be sent.
    pub dropped: usize,
    /// `template[0]` is the system message; the rest is the question's message, line by line.
    pub receipt: OnlineReceipt,
}

impl Prepared {
    /// The facts the body numbers `[m1]`..`[mN]`, in order.
    pub fn facts(&self) -> &[Fact] {
        &self.facts
    }
}

impl OnlineMind {
    pub fn new(client: Client, sendable: Sendable) -> Self {
        OnlineMind { client, sendable }
    }

    /// Prepares and sends one question. See [`OnlineMind::prepare`] and [`OnlineMind::send`].
    pub async fn answer(
        &self,
        question: &str,
        facts: &[Fact],
        out: mpsc::Sender<String>,
    ) -> Result<AnswerMeta, MindError> {
        let Some(prepared) = self.prepare(question, facts) else {
            return Ok(AnswerMeta {
                not_sure: true,
                ..AnswerMeta::default()
            });
        };
        let (meta, _) = self.send(&prepared, out).await;
        meta
    }

    /// The body for `question`: the first [`MAX_FACTS`](crate::online::MAX_FACTS) facts that may still be sent, numbered
    /// again from 1 without gaps. `None` when none may be. A section of the Nocturne guide never
    /// goes: help is written on this computer.
    pub fn prepare(&self, question: &str, facts: &[Fact]) -> Option<Prepared> {
        let Question {
            system,
            user,
            facts,
            dropped,
            template,
            line_hashes,
        } = online::question(question, facts, &*self.sendable)?;
        let request = OnlineRequest {
            model: MODEL_ID.to_string(),
            messages: vec![Message::system(&system), Message::user(&user)],
            temperature: nocturne_mind::temperature::ANSWER,
            max_tokens: MAX_TOKENS,
            timeout: None,
        };
        let receipt = OnlineReceipt {
            bytes_up: serde_json::to_vec(&request)
                .map(|b| b.len() as u64)
                .unwrap_or(0),
            template,
            sent_ids: facts.iter().map(|f| f.memory.id.clone()).collect(),
            line_hashes,
            ..OnlineReceipt::default()
        };
        Some(Prepared {
            request,
            system,
            facts,
            dropped,
            receipt,
        })
    }

    /// Sends a prepared question, once. The receipt comes back whatever happens, with what came
    /// back filled in. A reply is judged here: receipts must map back to the facts sent, and
    /// an answer without one is "not sure".
    pub async fn send(
        &self,
        prepared: &Prepared,
        out: mpsc::Sender<String>,
    ) -> (Result<AnswerMeta, MindError>, OnlineReceipt) {
        let mut receipt = prepared.receipt.clone();
        let started = Instant::now();
        let reply = self.client.chat_online(&prepared.request).await;
        receipt.seconds = started.elapsed().as_secs_f32();
        match reply {
            Ok(r) => {
                receipt.status = Some(r.status);
                receipt.bytes_up = r.bytes_up;
                receipt.bytes_down = r.bytes_down;
                receipt.cost_usd = r.cost_usd;
                let judged = judge(&r.text, &prepared.facts, &prepared.system);
                let meta = conclude(judged, &prepared.facts, &out).await.map(|mut m| {
                    m.online = Some(receipt.clone());
                    m
                });
                (meta, receipt)
            }
            Err(e) => {
                receipt.status = e.status;
                receipt.bytes_down = e.bytes_down;
                let failure = match (e.status, e.message.as_str()) {
                    (None, "timed out") => OnlineFailure::Slow,
                    (status, message) => classify(status, message),
                };
                (Err(MindError::Online(failure)), receipt)
            }
        }
    }
}

/// One connection check: the question "Reply with the word ready.", a few words back at most,
/// no memories. `host` and `model` name the provider in a `Works` outcome. The receipt's
/// `template[0]` is the (empty) system message, as for a question.
pub async fn check(client: &Client, host: &str, model: &str) -> (CheckOutcome, OnlineReceipt) {
    let request = OnlineRequest {
        model: MODEL_ID.to_string(),
        messages: vec![Message::user(CHECK_QUESTION)],
        temperature: nocturne_mind::temperature::CHECK,
        max_tokens: nocturne_mind::online::CHECK_MAX_TOKENS,
        timeout: Some(Duration::from_secs(SWITCH_SECONDS)),
    };
    let mut receipt = OnlineReceipt {
        template: vec![
            SentLine::Text {
                text: String::new(),
            },
            SentLine::Text {
                text: CHECK_QUESTION.to_string(),
            },
        ],
        ..OnlineReceipt::default()
    };
    let started = Instant::now();
    let reply = client.chat_online(&request).await;
    receipt.seconds = started.elapsed().as_secs_f32();
    let outcome = match reply {
        Ok(r) => {
            receipt.status = Some(r.status);
            receipt.bytes_up = r.bytes_up;
            receipt.bytes_down = r.bytes_down;
            receipt.cost_usd = r.cost_usd;
            if r.text.trim().is_empty() {
                CheckOutcome::EmptyReply
            } else {
                CheckOutcome::Works {
                    host: host.to_string(),
                    model: model.to_string(),
                }
            }
        }
        Err(e) => {
            receipt.status = e.status;
            receipt.bytes_up = e.bytes_up;
            receipt.bytes_down = e.bytes_down;
            match (e.status, e.message.as_str()) {
                (None, "timed out") => CheckOutcome::Unreachable,
                (status, message) => check_outcome(classify(status, message)),
            }
        }
    };
    (outcome, receipt)
}
