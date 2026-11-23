//! A question asked on a phone alone, answered by its online mind: the remembered lines found by
//! words, then the provider's answer from them, with a receipt for every line it uses, as on the
//! computer. An answer that cites nothing is not sure, and shows the nearest lines instead. With
//! the mind off, or a store that will not open, the answer is unavailable.

use nocturne_contract::{AnsweredBy, AskDelta, AskPhase, NearMatch};
use nocturne_engine::{AnswerMeta, Fact, Hit, UseKind, is_source_line};
use tokio::sync::mpsc;

use super::{PhoneMind, lock};
use crate::home::Home;

/// The most remembered lines handed to the mind for one question.
const LINES: usize = 12;
/// Questions stopped before they began that are remembered, the oldest let go first.
const STOPPED_EARLY: usize = 8;
/// The nearest lines shown when the answer is not sure.
const NEAR: usize = 2;

fn delta(ask_id: &str, phase: AskPhase) -> AskDelta {
    AskDelta {
        ask_id: ask_id.to_string(),
        phase,
        text_delta: None,
        receipts: Vec::new(),
        memory_ids: Vec::new(),
        near: Vec::new(),
        found: None,
        eta_seconds: None,
        plain: false,
        answered_by: None,
        left_out: None,
        send_id: None,
        help: None,
    }
}

/// Whether the bold spans in `text` stand one for one, in order, for the `cited` memories.
fn bold_matches(text: &str, spans: &[String], cited: &[String]) -> bool {
    let markers = text.matches("**").count();
    markers == 0 || (markers.is_multiple_of(2) && markers / 2 == spans.len() && spans == cited)
}

impl PhoneMind {
    /// Answers `question` from the phone's memory, telling `out` each step; the last is
    /// terminal (`done`, `notSure`, `nothing` or `unavailable`). `ask_id` names it for
    /// [`PhoneMind::stop_ask`]; one is made when none is given. Blocks on `rt`: call it off the
    /// async workers.
    pub fn ask(
        &self,
        home: &Home,
        rt: &tokio::runtime::Handle,
        ask_id: Option<&str>,
        question: &str,
        out: &mut dyn FnMut(AskDelta),
    ) {
        let ask_id: String = match ask_id {
            Some(id) => id.to_string(),
            None => nocturne_channel::crypto::random::<8>()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        };
        let (stop_tx, stop) = tokio::sync::watch::channel(false);
        {
            let mut early = lock(&self.stopped_early);
            if let Some(at) = early.iter().position(|id| *id == ask_id) {
                early.remove(at);
                return out(delta(&ask_id, AskPhase::Unavailable));
            }
            lock(&self.asking).insert(ask_id.clone(), stop_tx);
        }
        let _asked = Asked(self, &ask_id);
        let question = question.trim();
        let Some(on) = lock(&self.on).clone() else {
            return out(delta(&ask_id, AskPhase::Unavailable));
        };
        // Paused by the month's limit: Ask finds by words until it starts again.
        if self.paused_until().is_some() {
            return out(delta(&ask_id, AskPhase::Unavailable));
        }
        if question.is_empty() {
            return out(delta(&ask_id, AskPhase::Nothing));
        }
        let by = AnsweredBy::Online {
            host: on.host.clone(),
        };
        let end = |phase: AskPhase| {
            let mut d = delta(&ask_id, phase);
            d.answered_by = Some(by.clone());
            d
        };
        out(delta(&ask_id, AskPhase::Searching));
        // The store is held to find and to note what was used, never while the provider writes:
        // a move or a restore waits for no answer.
        let found = home.using().map_err(drop).and_then(|(_use, open)| {
            let e = &open.engine;
            let hits: Vec<Hit> = match e.retrieve(question, None, LINES) {
                Ok(hits) => hits.into_iter().filter(|h| !h.let_go).collect(),
                Err(x) => {
                    tracing::warn!("finding by words failed: {x}");
                    return Err(());
                }
            };
            // Nothing being forgotten or let go goes; a line of an original is given only
            // from an original with nothing of it forgotten (`with_source_lines`).
            let found = !hits.is_empty();
            let ids: Vec<String> = hits.iter().map(|h| h.memory.id.clone()).collect();
            let may = e.mind_may_read(&ids).map_err(|x| {
                tracing::warn!("could not check what the mind may read: {x}");
            })?;
            let hits: Vec<Hit> = hits
                .into_iter()
                .zip(may)
                .filter_map(|(h, may)| may.then_some(h))
                .collect();
            if hits.is_empty() {
                return Ok((hits, found));
            }
            let hits = e
                .with_source_lines(question, hits.clone(), LINES)
                .unwrap_or(hits);
            Ok((hits, found))
        });
        let Ok((hits, found)) = found else {
            return out(end(AskPhase::Unavailable));
        };
        // All it found is being forgotten or let go: not sure, and nothing is asked.
        if hits.is_empty() {
            return out(end(if found {
                AskPhase::NotSure
            } else {
                AskPhase::Nothing
            }));
        }
        if *stop.borrow() {
            return out(end(AskPhase::Unavailable));
        }
        let facts: Vec<Fact> = hits.iter().map(|h| Fact::from(h.memory.clone())).collect();
        let mut found = delta(&ask_id, AskPhase::Reading);
        found.found = Some(facts.len() as u32);
        out(found);
        out(delta(&ask_id, AskPhase::AskingOnline));

        let before = home
            .using()
            .ok()
            .map(|(_use, open)| (self.sent.last_id(), open.engine.forget_generation()));
        let (tx, mut rx) = mpsc::channel::<String>(32);
        let mind = on.mind.clone();
        let mut said = String::new();
        let meta: Result<AnswerMeta, _> = rt.block_on(async {
            let answering = mind.answer_until(question, &facts, tx, stop.clone());
            tokio::pin!(answering);
            loop {
                tokio::select! {
                    Some(text) = rx.recv() => {
                        said.push_str(&text);
                        let mut d = delta(&ask_id, AskPhase::Streaming);
                        d.text_delta = Some(text);
                        out(d);
                    }
                    meta = &mut answering => {
                        while let Ok(text) = rx.try_recv() {
                            said.push_str(&text);
                            let mut d = delta(&ask_id, AskPhase::Streaming);
                            d.text_delta = Some(text);
                            out(d);
                        }
                        break meta;
                    }
                }
            }
        });
        if let (Some((from, forgets)), Ok((_use, open))) = (before, home.using()) {
            self.after_request(&open.engine, from, forgets);
        }
        let meta = match meta {
            // Answered: counted, as the service did the work, though a Stop since keeps it
            // from showing.
            Ok(meta) => {
                self.count(1);
                if *stop.borrow() {
                    return out(end(AskPhase::Unavailable));
                }
                meta
            }
            // Stopped: not answered, and not counted.
            Err(_) if *stop.borrow() => return out(end(AskPhase::Unavailable)),
            Err(x) => {
                tracing::warn!("the online mind did not answer: {x}");
                return out(end(AskPhase::Unavailable));
            }
        };
        let cited: Vec<&Hit> = meta
            .cited
            .iter()
            .filter_map(|id| hits.iter().find(|h| &h.memory.id == id))
            .collect();
        // Every claim carries a receipt: an answer that cites nothing is not sure.
        if meta.not_sure || cited.is_empty() {
            let mut d = end(AskPhase::NotSure);
            d.near = hits
                .iter()
                .filter_map(|h| {
                    h.memory.receipts.first().map(|r| NearMatch {
                        text: h.memory.sentence.clone(),
                        receipt: r.clone(),
                    })
                })
                .take(NEAR)
                .collect();
            return out(d);
        }
        let cited_ids: Vec<String> = cited.iter().map(|h| h.memory.id.clone()).collect();
        // A line of an original is no memory: shown without bold, never counted as used.
        let ids: Vec<String> = cited_ids
            .iter()
            .filter(|id| !is_source_line(id))
            .cloned()
            .collect();
        if let Ok((_use, open)) = home.using()
            && let Err(x) = open.engine.mark_used(&ids, UseKind::Answer)
        {
            tracing::warn!("could not note the memories used: {x}");
        }
        let mut d = end(AskPhase::Done);
        d.receipts = cited
            .iter()
            .flat_map(|h| h.memory.receipts.iter().cloned())
            .collect();
        d.plain = ids.len() != cited_ids.len() || !bold_matches(&said, &meta.spans, &ids);
        d.memory_ids = ids;
        out(d);
    }
}

/// Removes a question from those under way as it ends.
struct Asked<'a>(&'a PhoneMind, &'a str);

impl Drop for Asked<'_> {
    fn drop(&mut self) {
        lock(&self.0.asking).remove(self.1);
    }
}

impl PhoneMind {
    /// Stops the question `ask_id`: its request to the service is dropped, and it is not
    /// answered, nor counted unless the service had already answered. A question not begun yet
    /// is stopped as it begins; the last few such are remembered.
    pub fn stop_ask(&self, ask_id: &str) {
        let mut early = lock(&self.stopped_early);
        match lock(&self.asking).get(ask_id) {
            Some(stop) => {
                stop.send_replace(true);
            }
            None => {
                if early.len() >= STOPPED_EARLY {
                    early.remove(0);
                }
                early.push(ask_id.to_string());
            }
        }
    }

    /// A screen's `Ask` call, answered whole: the terminal step, with every word of the answer
    /// as its `text_delta`. With the mind off it is `NotReady`, as before a mind was chosen.
    pub fn answer_call(
        &self,
        home: &Home,
        rt: &tokio::runtime::Handle,
        ask_id: Option<&str>,
        question: &str,
    ) -> nocturne_contract::RemoteAnswer {
        use nocturne_contract::{PhoneError, RemoteAnswer};
        if !self.is_on() {
            return RemoteAnswer::Error {
                code: PhoneError::NotReady,
            };
        }
        let mut said = String::new();
        let mut last = None;
        self.ask(home, rt, ask_id, question, &mut |d| match d.phase {
            AskPhase::Streaming => said.push_str(d.text_delta.as_deref().unwrap_or_default()),
            _ => last = Some(d),
        });
        match last {
            Some(mut delta) => {
                // Half an answer that did not end is not shown.
                let ended = matches!(delta.phase, AskPhase::Done | AskPhase::NotSure);
                if ended && !said.is_empty() {
                    delta.text_delta = Some(said);
                }
                RemoteAnswer::AskDelta { delta }
            }
            None => RemoteAnswer::Error {
                code: PhoneError::NotReady,
            },
        }
    }
}
