//! Looking back: writing a day's reflection with the mind, and composing a period's recap.
//!
//! A day's reflection is the one place the mind writes for the journal. It writes "The day" from
//! the items kept that day, each sentence citing what it came from; the engine drops a sentence
//! that cites nothing the day had. Everything else in a reflection, and every recap, is composed
//! from what was kept. Nothing here counts as use of a memory.

use std::fmt;

use nocturne_contract::{DayReflection, Period, PeriodKind, Recap, WrittenWhen};
use nocturne_engine::{Engine, Error as EngineError, Mind, MindError, WrittenSentence};

/// "The day" holds at most this many sentences.
pub const MAX_SENTENCES: usize = 6;

/// What `write` stored.
#[derive(Debug, Clone, PartialEq)]
pub enum Wrote {
    Day(DayReflection),
    Recap(Recap),
    /// Nothing was written: too little of the period was the person's own, or no sentence
    /// carried a receipt. The period is listed at a glance instead.
    Listed,
}

#[derive(Debug)]
pub enum ReflectError {
    Engine(EngineError),
    Mind(MindError),
}

impl fmt::Display for ReflectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReflectError::Engine(e) => write!(f, "{e}"),
            ReflectError::Mind(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ReflectError {}

impl From<EngineError> for ReflectError {
    fn from(e: EngineError) -> Self {
        ReflectError::Engine(e)
    }
}

impl From<MindError> for ReflectError {
    fn from(e: MindError) -> Self {
        ReflectError::Mind(e)
    }
}

async fn on_engine<T, F>(engine: &Engine, f: F) -> Result<T, EngineError>
where
    T: Send + 'static,
    F: FnOnce(&Engine) -> Result<T, EngineError> + Send + 'static,
{
    let engine = engine.clone();
    tokio::task::spawn_blocking(move || f(&engine))
        .await
        .unwrap_or_else(|_| Err(EngineError::Invalid("a worker thread failed".into())))
}

/// Write the reflection or recap for `period` and store it, replacing the one stored. A day
/// is written by `mind`; a week, month or year is composed and needs none.
pub async fn write(
    engine: &Engine,
    mind: &dyn Mind,
    period: &Period,
    written: WrittenWhen,
) -> Result<Wrote, ReflectError> {
    if period.kind != PeriodKind::Day {
        let p = period.clone();
        let recap = on_engine(engine, move |e| {
            let recap = e.compose_recap(&p, written)?;
            if recap.days > 0 {
                e.save_recap(&recap)?;
            }
            Ok(recap)
        })
        .await?;
        return Ok(if recap.days == 0 {
            Wrote::Listed
        } else {
            Wrote::Recap(recap)
        });
    }
    let p = period.clone();
    let input = on_engine(engine, move |e| e.reflection_input(&p)).await?;
    if input.items.iter().all(|i| i.background) {
        return Ok(Wrote::Listed);
    }
    let kept: Vec<WrittenSentence> = mind
        .reflect(&input)
        .await?
        .into_iter()
        .filter(|s| {
            !s.text.trim().is_empty()
                && !s.cites.is_empty()
                && s.cites.iter().all(|c| *c < input.items.len())
        })
        .take(MAX_SENTENCES)
        .collect();
    if kept.is_empty() {
        return Ok(Wrote::Listed);
    }
    let p = period.clone();
    let reflection = on_engine(engine, move |e| {
        let reflection = e.save_day_reflection(&p, &input, &kept, written)?;
        if reflection.the_day.is_empty() {
            e.delete_reflection(&p)?;
        }
        Ok(reflection)
    })
    .await?;
    Ok(if reflection.the_day.is_empty() {
        Wrote::Listed
    } else {
        Wrote::Day(reflection)
    })
}
