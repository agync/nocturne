//! A question asked on a phone alone, answered by its online mind: the remembered lines found by
//! words and the date step's lines for the days the question names, then the provider's answer
//! from them, with a receipt for every line it uses, as on the computer. A question that names a
//! time, or whose words found fewer than 3 lines, takes the look-up path: the provider may ask
//! to look further, read only, on this phone (docs/design/ask-with-tools.md). An answer that
//! cites nothing is not sure, and shows the nearest lines instead. With the mind off, or a store
//! that will not open, the answer is unavailable.

use std::sync::Arc;
use std::time::Duration;

use nocturne_contract::{AnsweredBy, AskDelta, AskPhase, NearMatch, ReadingKind};
use nocturne_engine::{
    AskToday, CALENDAR_LINES, DATE_STEP_LINES, Engine, Fact, HEALTH_LINES, Hit, JOURNAL_LINES,
    LookScope, SEARCH_LINES, UseKind, date_step, is_kept_line, is_source_line,
};
use nocturne_mind::lookup::{LookUp, Place};
use nocturne_provider::lookup::{LookUps, Parts};
use nocturne_provider::mind::Asking;
use tokio::sync::mpsc;

use super::{PhoneMind, lock};
use crate::home::Home;

/// The most remembered lines handed to the mind for one question.
const LINES: usize = 12;
/// Questions stopped before they began that are remembered, the oldest let go first.
const STOPPED_EARLY: usize = 8;
/// The nearest lines shown when the answer is not sure.
const NEAR: usize = 2;
/// Fewer lines found by words than this, and the question takes the look-up path.
const LOOK_BELOW: usize = 3;
/// The longest a question on the look-up path may take, all its parts together.
const QUESTION_LIMIT: Duration = Duration::from_secs(90);

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
            let today = e.ask_today().map_err(|x| {
                tracing::warn!("could not read today: {x}");
            })?;
            let step = date_step(question, today.day, today.week_start);
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
            let words = if hits.is_empty() {
                hits
            } else {
                e.with_source_lines(question, hits.clone(), LINES)
                    .unwrap_or(hits)
            };
            // The date step: what was kept for the days the question names.
            let days = e
                .date_step_lines(&step, today.day, LookScope::ALL, DATE_STEP_LINES)
                .map_err(|x| tracing::warn!("the date step failed: {x}"))?;
            let kinds = e.reading_kinds().unwrap_or_default();
            Ok(Found {
                words,
                days,
                found,
                names_time: step.names_time(),
                today,
                kinds,
                engine: e.clone(),
            })
        });
        let Ok(Found {
            words,
            days,
            found,
            names_time,
            today,
            kinds,
            engine,
        }) = found
        else {
            return out(end(AskPhase::Unavailable));
        };
        // The look-up path when the question names a time or the words found little; the date
        // step's lines come first, then the words', one copy of each.
        let looks = names_time || words.len() < LOOK_BELOW;
        let takes = on.mind.tools().offered();
        let mut hits = days;
        for w in words {
            if !hits.iter().any(|h| h.memory.id == w.memory.id) {
                hits.push(w);
            }
        }
        // Nothing to answer from, and a mind that cannot look: nothing is asked.
        if hits.is_empty() && !(looks && takes) {
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
        let (look_tx, mut look_rx) = mpsc::unbounded_channel::<Vec<Place>>();
        let look_ups = looks.then(|| LookUps {
            kinds,
            run: Arc::new(move |l: &LookUp| look_up(&engine, l)),
            looking: Arc::new(move |p: &[Place]| {
                let _ = look_tx.send(p.to_vec());
            }),
            limit: QUESTION_LIMIT,
        });
        let may_send = || self.paused_until().is_none();
        let counted = || self.count(1);
        let mind = on.mind.clone();
        let mut said = String::new();
        let answered = rt.block_on(async {
            let answering = mind.answer_asked(
                Asking {
                    ask: &ask_id,
                    question,
                    facts: &facts,
                    today: &today,
                    look_ups: look_ups.as_ref(),
                    parts: Parts {
                        may_send: &may_send,
                        counted: &counted,
                    },
                    stop: Some(stop.clone()),
                },
                tx,
            );
            tokio::pin!(answering);
            loop {
                tokio::select! {
                    Some(places) = look_rx.recv() => {
                        out(delta(&ask_id, looking_phase(&places)));
                    }
                    Some(text) = rx.recv() => {
                        said.push_str(&text);
                        let mut d = delta(&ask_id, AskPhase::Streaming);
                        d.text_delta = Some(text);
                        out(d);
                    }
                    answered = &mut answering => {
                        while let Ok(text) = rx.try_recv() {
                            said.push_str(&text);
                            let mut d = delta(&ask_id, AskPhase::Streaming);
                            d.text_delta = Some(text);
                            out(d);
                        }
                        break answered;
                    }
                }
            }
        });
        if let (Some((from, forgets)), Ok((_use, open))) = (before, home.using()) {
            self.after_request(&open.engine, from, forgets);
        }
        let answered = match answered {
            // Answered, part by part counted as the service did the work, though a Stop since
            // keeps it from showing.
            Ok(a) => {
                if *stop.borrow() {
                    return out(end(AskPhase::Unavailable));
                }
                a
            }
            // Stopped: what was not answered is not counted.
            Err(_) if *stop.borrow() => return out(end(AskPhase::Unavailable)),
            Err(x) => {
                tracing::warn!("the online mind did not answer: {x}");
                return out(end(AskPhase::Unavailable));
            }
        };
        // A look found nothing, and nothing was kept that could answer.
        if answered.nothing {
            self.sent.ended_with_nothing(&ask_id);
            return out(end(AskPhase::Nothing));
        }
        let meta = answered.meta;
        let handed = answered.facts;
        let cited: Vec<&Fact> = meta
            .cited
            .iter()
            .filter_map(|id| handed.iter().find(|f| &f.memory.id == id))
            .collect();
        // Every claim carries a receipt, of a line handed in this question: an answer that cites
        // nothing is not sure, and shows the nearest lines, the date step's first.
        if meta.not_sure || cited.is_empty() {
            let mut d = end(AskPhase::NotSure);
            d.near = handed
                .iter()
                .filter_map(|f| {
                    f.memory.receipts.first().map(|r| NearMatch {
                        text: f.memory.sentence.clone(),
                        receipt: r.clone(),
                    })
                })
                .take(NEAR)
                .collect();
            return out(d);
        }
        let cited_ids: Vec<String> = cited.iter().map(|f| f.memory.id.clone()).collect();
        // A line of an original, or a check-in, an entry or a reading kept as it is, is no
        // memory: shown without bold, never counted as used.
        let ids: Vec<String> = cited_ids
            .iter()
            .filter(|id| !is_source_line(id) && !is_kept_line(id))
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
            .flat_map(|f| f.memory.receipts.iter().cloned())
            .collect();
        d.plain = ids.len() != cited_ids.len() || !bold_matches(&said, &meta.spans, &ids);
        d.memory_ids = ids;
        out(d);
    }
}

/// What a question found before anything is asked.
struct Found {
    words: Vec<Hit>,
    days: Vec<Hit>,
    found: bool,
    names_time: bool,
    today: AskToday,
    kinds: Vec<ReadingKind>,
    engine: Engine,
}

/// The line while a round of look-ups runs.
fn looking_phase(places: &[Place]) -> AskPhase {
    match places {
        [Place::Calendar] => AskPhase::LookingCalendar,
        [Place::Journal] => AskPhase::LookingJournal,
        [Place::Search] => AskPhase::LookingSearch,
        [Place::Health] => AskPhase::LookingHealth,
        _ => AskPhase::LookingSeveral,
    }
}

/// Runs one look-up on this phone's store, read only, with what Ask itself may give.
fn look_up(engine: &Engine, l: &LookUp) -> Option<(Vec<Fact>, usize)> {
    let all = LookScope::ALL;
    let found = match l {
        LookUp::Calendar { from, to } => engine.look_up_calendar(*from, *to, all, CALENDAR_LINES),
        LookUp::Journal { from, to } => engine.look_up_journal(*from, *to, all, JOURNAL_LINES),
        LookUp::Search { words } => engine.look_up_words(words, all, SEARCH_LINES),
        LookUp::Health { kind, from, to } => {
            engine.look_up_health(*kind, *from, *to, all, HEALTH_LINES)
        }
    };
    match found {
        Ok(found) => Some((
            found
                .hits
                .into_iter()
                .map(|h| Fact::from(h.memory))
                .collect(),
            found.more,
        )),
        Err(x) => {
            tracing::warn!("a look-up failed: {x}");
            None
        }
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
