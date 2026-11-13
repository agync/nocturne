//! The re-read a new finding model needs, in the app: begun once the model is on disk, run by
//! the intelligence crate's worker beside everything else, measured for its estimate, and
//! finished by naming the new model in `selection.json` after the store has swapped.

use std::sync::Arc;
use std::time::Instant;

use nocturne_contract::{ComponentRole, ModelsView, ReindexPhase, ReindexView};
use nocturne_intelligence::reindex::{RateMeter, ReindexEnd, Reindexer};
use nocturne_lemonade::fit;
use nocturne_lemonade::fit_table;
use nocturne_lemonade::selection::{self, OldModel};
use tauri::{AppHandle, Manager, Runtime};
use tokio_util::sync::CancellationToken;

use super::controller::{ModelsHub, ReindexRun, in_use};
use crate::error::CommandError;
use crate::state::AppState;

impl ModelsHub {
    /// Begins (or, after a restart, carries on) reading every memory with `model`; the old
    /// vectors stay in use until the swap.
    pub(super) fn begin_reindex<R: Runtime>(
        self: &Arc<Self>,
        app: &AppHandle<R>,
        model: &str,
        from: &str,
    ) -> Result<(), CommandError> {
        let engine = app.state::<AppState>().engine()?;
        let dim = fit_table::lookup(model).and_then(|e| e.dim);
        engine
            .begin_reindex(model, dim)
            .map_err(CommandError::internal)?;
        self.run_reindex(app, model, from);
        Ok(())
    }

    /// A re-read the store says is under way and nothing here runs: it carries on.
    pub(super) fn resume_reindex<R: Runtime>(self: &Arc<Self>, app: &AppHandle<R>) {
        if self.state().reindex.is_some() {
            return;
        }
        let Ok(engine) = app.state::<AppState>().engine() else {
            return;
        };
        if let Ok(Some(row)) = engine.reindex_state()
            && row.phase == ReindexPhase::Running
        {
            self.run_reindex(app, &row.model, &row.from_model);
        }
    }

    fn run_reindex<R: Runtime>(self: &Arc<Self>, app: &AppHandle<R>, model: &str, from: &str) {
        let state = app.state::<AppState>();
        let (Ok(engine), Some(services)) = (state.engine(), state.services()) else {
            return;
        };
        let stop = CancellationToken::new();
        let (done, total) = engine.reindex_progress().unwrap_or((0, 0));
        self.state().reindex = Some(ReindexRun {
            stop: stop.clone(),
            done,
            total,
            eta: None,
        });
        let worker = Reindexer::new(
            engine,
            state.mind.shared(),
            services.reader.clone(),
            self.opts.reindex,
        );
        let (hub, app, model, from) = (
            self.clone(),
            app.clone(),
            model.to_string(),
            from.to_string(),
        );
        tauri::async_runtime::spawn(async move {
            let meter = std::sync::Mutex::new(RateMeter::default());
            let progress = |done: u32, total: u32| {
                let mut m = meter.lock().unwrap_or_else(|e| e.into_inner());
                m.note(Instant::now(), done);
                let rate = m.per_second();
                let eta = m.eta(total.saturating_sub(done));
                let mut st = hub.state();
                if rate.is_some() {
                    st.rate = rate;
                }
                if let Some(r) = st.reindex.as_mut() {
                    r.done = done;
                    r.total = total;
                    r.eta = eta;
                }
                drop(st);
                let (hub, app) = (hub.clone(), app.clone());
                tauri::async_runtime::spawn(async move {
                    hub.emit(&app).await;
                });
            };
            let end = worker.run(&model, stop, progress).await;
            let run = hub.state().reindex.take();
            let phase = match end {
                ReindexEnd::Done => {
                    hub.finished(&app, &model, &from).await;
                    ReindexPhase::Done
                }
                ReindexEnd::Stopped => ReindexPhase::Cancelled,
            };
            let (done, total) = run.map_or((0, 0), |r| (r.done, r.total));
            hub.state().ended = Some(ReindexView {
                model,
                from,
                done: if phase == ReindexPhase::Done {
                    total
                } else {
                    done
                },
                total,
                eta_seconds: None,
                phase,
            });
            hub.emit(&app).await;
        });
    }

    /// The store has swapped and already finds memories with the new model: the selection
    /// names it, the old one is kept, and the mind is rebuilt around it.
    async fn finished<R: Runtime>(&self, app: &AppHandle<R>, model: &str, from: &str) {
        let dir = Self::config_dir(app);
        let saved = selection::update(&dir, |s| {
            *s = fit::with_choice(s, ComponentRole::Recall, model);
            s.old = (!from.is_empty() && from != model && !in_use(s, from)).then(|| OldModel {
                role: ComponentRole::Recall,
                model: from.to_string(),
            });
        });
        if let Err(e) = saved {
            // The next launch repairs it from the store (`repair_after_swap`).
            tracing::warn!("the new finding model could not be saved: {e}");
        }
        crate::runtime::lemonade::refill_mind(app).await;
    }

    /// The person stops the re-read: what it built goes and the old way stays.
    pub async fn cancel_reindex<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<ModelsView, CommandError> {
        let engine = app.state::<AppState>().engine()?;
        engine.cancel_reindex().map_err(CommandError::internal)?;
        let stop = self.state().reindex.as_ref().map(|r| r.stop.clone());
        if let Some(stop) = stop {
            stop.cancel();
        }
        Ok(self.emit(app).await)
    }
}
