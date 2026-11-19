//! The journal's looking back, from the app's side: which computers write reflections, what the
//! journal shows for a day or a period, and writing one when the person asks.
//!
//! A reflection or recap is written only on a size of computer that passed the journal check
//! with its preset answering model (`docs/design/evaluation.md`, "The journal check"). Everywhere
//! else the journal shows the day at a glance. Looking back is never use: nothing here touches a
//! memory's last-used time, and nothing here goes online.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nocturne_contract::{
    ComponentRole, Glance, GlanceReason, Period, PeriodKind, ReflectionView, Tier, WrittenWhen,
};
use nocturne_engine::{Engine, MindError};
use nocturne_intelligence::{ReflectError, Wrote, reflect};
use tokio::task::AbortHandle;

use crate::error::{CommandError, blocking};
use crate::state::AppState;

/// The sizes of computer whose preset answering model passed the journal check (the evaluation
/// set in `docs/design/evaluation.md`: every sentence cites a receipt, no feeling without a
/// check-in or a quote, no advice, no invented people). A size that is not here shows the day at
/// a glance instead of a written reflection. None has passed yet: the release checklist records
/// each run.
pub const WRITES_REFLECTIONS: &[Tier] = &[];

/// Whether a size of computer writes reflections and recaps: it passed the check with its preset
/// answering model. A model the person chose themselves has not been through the check.
pub fn writes_reflections(tier: Tier, own_answering_model: bool) -> bool {
    !own_answering_model && WRITES_REFLECTIONS.contains(&tier)
}

/// Whether this computer writes reflections now: the override, else the size of computer and
/// whether the person chose their own answering model.
pub fn writes_now(state: &AppState) -> bool {
    if let Some(w) = state.reflecting.writes() {
        return w;
    }
    let s = nocturne_lemonade::selection::read(&crate::models::config_dir(&state.data_dir));
    writes_reflections(s.tier, s.custom.contains_key(&ComponentRole::Mind))
}

struct Live {
    id: u64,
    abort: AbortHandle,
    /// Checked by the write inside the store's transaction, which an abort cannot reach.
    stop: Arc<AtomicBool>,
}

/// The override of the journal check, and the reflections being written on request, which the
/// person can stop.
#[derive(Default)]
pub struct Reflecting {
    writes: Mutex<Option<bool>>,
    live: Mutex<HashMap<Period, Live>>,
    next: AtomicU64,
}

impl Reflecting {
    /// Set whether reflections are written regardless of the size of computer: for tests, the
    /// mock and the journal check itself. `None` goes back to [`WRITES_REFLECTIONS`].
    pub fn set_writes(&self, writes: Option<bool>) {
        *self.writes.lock().unwrap_or_else(|e| e.into_inner()) = writes;
    }

    fn writes(&self) -> Option<bool> {
        *self.writes.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn live(&self) -> std::sync::MutexGuard<'_, HashMap<Period, Live>> {
        self.live.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Whether a reflection for `period` is being written on request.
    pub fn is_writing(&self, period: &Period) -> bool {
        self.live().contains_key(period)
    }

    /// Track a write; `None` when one for this period is already going.
    fn begin(&self, period: &Period, abort: AbortHandle, stop: Arc<AtomicBool>) -> Option<u64> {
        let mut live = self.live();
        if live.contains_key(period) {
            return None;
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        live.insert(period.clone(), Live { id, abort, stop });
        Some(id)
    }

    fn end(&self, period: &Period, id: u64) {
        let mut live = self.live();
        if live.get(period).is_some_and(|l| l.id == id) {
            live.remove(period);
        }
    }

    /// Stop the write for `period`, if one is going. Nothing is stored for it; what was stored
    /// before stays.
    pub fn stop(&self, period: &Period) -> bool {
        let found = self.live().remove(period);
        let was = found.is_some();
        if let Some(l) = found {
            l.stop.store(true, Ordering::SeqCst);
            l.abort.abort();
        }
        was
    }
}

struct AbortOnDrop<T>(tokio::task::JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// What stands in the way of writing for `period`, or the glance to write beside.
enum Assessed {
    Refused(Box<ReflectionView>),
    Ready(Glance),
}

/// Whether anything in the period is the person's own, so that there is something to write
/// about. A day looks at what the mind would be given; a longer period at what was kept.
fn has_own_words(
    engine: &Engine,
    period: &Period,
    glance: &Glance,
) -> nocturne_engine::Result<bool> {
    if period.kind == PeriodKind::Day {
        return Ok(engine
            .reflection_input(period)?
            .items
            .iter()
            .any(|i| !i.background));
    }
    use nocturne_contract::GlanceKind::{Note, Photo, Voice};
    Ok(!glance.felt.is_empty()
        || !glance.good.is_empty()
        || !glance.entry_lines.is_empty()
        || glance
            .kept
            .iter()
            .any(|k| matches!(k.kind, Note | Voice | Photo)))
}

async fn assess(state: &AppState, period: &Period) -> Result<Assessed, CommandError> {
    let engine = state.engine()?;
    let p = period.clone();
    let (glance, own) = blocking(move || {
        let glance = engine.glance(&p)?;
        let own = has_own_words(&engine, &p, &glance)?;
        Ok((glance, own))
    })
    .await?;
    let writing = state.reflecting.is_writing(period)
        || state
            .services()
            .is_some_and(|s| s.scheduler.writing(period));
    let nothing = glance.kept.is_empty() && glance.felt.is_empty();
    let refused = |reason| ReflectionView::Glance {
        glance: glance.clone(),
        reason,
    };
    Ok(if writing {
        Assessed::Refused(Box::new(ReflectionView::Writing { glance }))
    } else if nothing {
        Assessed::Refused(Box::new(ReflectionView::Nothing {
            period: period.clone(),
        }))
    } else if state.mind.get().is_none() || state.mind.is_loading() {
        Assessed::Refused(Box::new(refused(GlanceReason::NotReady)))
    } else if !writes_now(state) {
        Assessed::Refused(Box::new(refused(GlanceReason::NotOnThisComputer)))
    } else if !own {
        Assessed::Refused(Box::new(refused(GlanceReason::Listed)))
    } else {
        Assessed::Ready(glance)
    })
}

/// What the journal shows for a day or a period: what is written, else why it is not.
pub async fn view(state: &AppState, period: &Period) -> Result<ReflectionView, CommandError> {
    let engine = state.engine()?;
    let p = period.clone();
    let saved = blocking(move || engine.saved_reflection(&p)).await?;
    let writing = state.reflecting.is_writing(period)
        || state
            .services()
            .is_some_and(|s| s.scheduler.writing(period));
    if let (Some(saved), false) = (saved, writing) {
        return Ok(saved);
    }
    Ok(match assess(state, period).await? {
        Assessed::Refused(view) => *view,
        Assessed::Ready(glance) => ReflectionView::NotYet { glance },
    })
}

/// Write the reflection or recap for `period` now, because the person asked. It holds off the
/// reading like an answer does, runs on battery, and replaces what was written. A period that is
/// still going is written as it is so far. Returns what is now stored, or why nothing could be.
pub async fn reflect(state: &AppState, period: &Period) -> Result<ReflectionView, CommandError> {
    let glance = match assess(state, period).await? {
        Assessed::Refused(view) => return Ok(*view),
        Assessed::Ready(glance) => glance,
    };
    let engine = state.engine()?;
    let Some(mind) = state.mind.get() else {
        return Ok(ReflectionView::Glance {
            glance,
            reason: GlanceReason::NotReady,
        });
    };
    let _reader_waits = state.services().map(|s| s.reader.ask_begin());
    let (e, p) = (engine.clone(), period.clone());
    let (go, wait) = tokio::sync::oneshot::channel::<()>();
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let mut task = AbortOnDrop(tokio::spawn(async move {
        let _ = wait.await;
        reflect::write_stoppable(&e, mind.as_ref(), &p, WrittenWhen::OnDemand, stopped).await
    }));
    // Tracked before it starts, so a stop can never come too early to find it.
    let Some(id) = state.reflecting.begin(period, task.0.abort_handle(), stop) else {
        return Ok(ReflectionView::Writing { glance });
    };
    let _ = go.send(());
    let joined = (&mut task.0).await;
    state.reflecting.end(period, id);
    match joined {
        Ok(Ok(Wrote::Day(_) | Wrote::Recap(_))) => {
            let p = period.clone();
            let saved = blocking(move || engine.saved_reflection(&p)).await?;
            Ok(saved.unwrap_or(ReflectionView::NotYet { glance }))
        }
        Ok(Ok(Wrote::Listed)) => Ok(ReflectionView::Glance {
            glance,
            reason: GlanceReason::Listed,
        }),
        Ok(Err(ReflectError::Mind(MindError::Busy | MindError::Unavailable))) => {
            Ok(ReflectionView::Glance {
                glance,
                reason: GlanceReason::NotReady,
            })
        }
        Ok(Err(ReflectError::Engine(nocturne_engine::Error::Cancelled))) => {
            Ok(ReflectionView::NotYet { glance })
        }
        Ok(Err(ReflectError::Engine(e))) => Err(e.into()),
        Ok(Err(ReflectError::Mind(e))) => {
            tracing::warn!("a reflection could not be written: {e}");
            Err(CommandError::internal(e))
        }
        // Stopped by the person: back to where it was.
        Err(e) if e.is_cancelled() => Ok(ReflectionView::NotYet { glance }),
        Err(_) => Err(CommandError::internal("writing the reflection failed")),
    }
}

/// Stop the reflection being written for `period`.
pub fn stop(state: &AppState, period: &Period) {
    state.reflecting.stop(period);
}

/// Delete the writing for `period`, and stop it being written first. Entries are untouched.
pub async fn delete(state: &AppState, period: &Period) -> Result<(), CommandError> {
    stop(state, period);
    let engine = state.engine()?;
    let p = period.clone();
    blocking(move || engine.delete_reflection(&p)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> Period {
        Period {
            kind: PeriodKind::Day,
            start: "2026-11-12".into(),
            end: "2026-11-12".into(),
        }
    }

    #[tokio::test]
    async fn stopping_a_write_sets_the_flag_its_save_checks() {
        let reflecting = Reflecting::default();
        let task = tokio::spawn(std::future::pending::<()>());
        let stop = Arc::new(AtomicBool::new(false));
        let id = reflecting
            .begin(&day(), task.abort_handle(), stop.clone())
            .unwrap();
        assert!(!stop.load(Ordering::SeqCst));
        assert!(reflecting.stop(&day()));
        assert!(stop.load(Ordering::SeqCst));
        assert!(!reflecting.is_writing(&day()));
        reflecting.end(&day(), id);
        assert!(task.await.unwrap_err().is_cancelled());
    }
}
