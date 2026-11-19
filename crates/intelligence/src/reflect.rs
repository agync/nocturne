//! Looking back: writing a day's reflection with the mind, and composing a period's recap.
//!
//! A day's reflection is the one place the mind writes for the journal. It writes "The day" from
//! the items kept that day, each sentence citing what it came from; the engine drops a sentence
//! that cites nothing the day had. Everything else in a reflection, and every recap, is composed
//! from what was kept: a recap needs no mind, so [`write_recap`] runs on any computer, before
//! the engine is ready too. Nothing here counts as use of a memory, and nothing here goes
//! online.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
/// is written by `mind`; a week, month or year is composed and needs none ([`write_recap`]).
pub async fn write(
    engine: &Engine,
    mind: &dyn Mind,
    period: &Period,
    written: WrittenWhen,
) -> Result<Wrote, ReflectError> {
    write_stoppable(
        engine,
        mind,
        period,
        written,
        Arc::new(AtomicBool::new(false)),
    )
    .await
}

/// [`write`], which stores nothing once `stop` is set, checked inside the store's own
/// transaction: a write already past its last await, running on a blocking thread, can still
/// be stopped. A stopped write is `ReflectError::Engine(Error::Cancelled)`.
pub async fn write_stoppable(
    engine: &Engine,
    mind: &dyn Mind,
    period: &Period,
    written: WrittenWhen,
    stop: Arc<AtomicBool>,
) -> Result<Wrote, ReflectError> {
    if period.kind != PeriodKind::Day {
        return write_recap(engine, period, written, stop).await;
    }
    let p = period.clone();
    let input = on_engine(engine, move |e| e.reflection_input(&p)).await?;
    if input.items.iter().all(|i| i.background) {
        return Ok(Wrote::Listed);
    }
    let reflected = mind.reflect(&input).await?;
    if reflected.sentences.is_empty() || reflected.sentences.len() * 2 < reflected.written {
        return Ok(Wrote::Listed);
    }
    let kept: Vec<WrittenSentence> = reflected
        .sentences
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
        let reflection = e.save_day_reflection_unless(&p, &input, &kept, written, &|| {
            stop.load(Ordering::SeqCst)
        })?;
        if reflection.the_day.is_empty() {
            e.discard_reflection(&p)?;
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

/// Put the recap for a week, month or year together from what was kept and store it, replacing
/// the one stored. It needs no mind, and stores nothing once `stop` is set, checked inside the
/// store's own transaction. A period with nothing of its own in it is [`Wrote::Listed`]. A day
/// is refused: it is reflected on, not recapped.
pub async fn write_recap(
    engine: &Engine,
    period: &Period,
    written: WrittenWhen,
    stop: Arc<AtomicBool>,
) -> Result<Wrote, ReflectError> {
    let p = period.clone();
    let recap = on_engine(engine, move |e| {
        let recap = e.compose_recap(&p, written)?;
        if recap.days > 0 {
            e.save_recap_unless(&recap, &|| stop.load(Ordering::SeqCst))?;
        }
        Ok(recap)
    })
    .await?;
    Ok(if recap.days == 0 {
        Wrote::Listed
    } else {
        Wrote::Recap(recap)
    })
}
