//! The models hub: what each job uses, what it could use instead and how that would fit, and the
//! one change or re-read under way. Downloads are in `changes`, re-reads in `reindex`.

use std::sync::{Arc, Mutex, MutexGuard};

use nocturne_contract::{
    ChangePhase, ChangePreview, ChangeView, ComponentRole, EVENT_MODELS, Fit, Job, ModelChoice,
    ModelsView, OldModelView, ReindexPhase, ReindexView,
};
use nocturne_intelligence::reindex::{ReindexOptions, estimate_seconds};
use nocturne_lemonade::client::{ModelInfo, SystemInfo};
use nocturne_lemonade::fit::{self, Budget, role_of};
use nocturne_lemonade::jobs::{JobOptions, JobProgress, ModelJobs};
use nocturne_lemonade::selection::{self, Selection};
use nocturne_lemonade::{Client, Control};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio_util::sync::CancellationToken;

use super::{config_dir, job_views};
use crate::error::{CommandError, ErrorCode};
use crate::state::AppState;

/// Items read per second when no re-read has been measured yet on this computer.
const ASSUMED_PER_SECOND: f64 = 4.0;

/// Timings, and the memory to assume instead of reading it. The defaults are the product's;
/// tests shrink them.
#[derive(Debug, Clone, Copy, Default)]
pub struct HubOptions {
    pub jobs: JobOptions,
    pub reindex: ReindexOptions,
    pub ram_bytes: Option<u64>,
}

/// A change waiting for its download.
#[derive(Debug, Clone)]
pub(super) struct Change {
    pub role: ComponentRole,
    pub model: String,
    pub from: String,
    pub progress: JobProgress,
    pub resumed: bool,
}

/// A re-read running in this session.
pub(super) struct ReindexRun {
    pub stop: CancellationToken,
    pub done: u32,
    pub total: u32,
    pub eta: Option<u32>,
}

#[derive(Default)]
pub(super) struct HubState {
    pub change: Option<Change>,
    pub reindex: Option<ReindexRun>,
    /// How the last re-read of this session ended, until the next change.
    pub ended: Option<ReindexView>,
    /// The old model the person just removed, shown once as removed.
    pub removed: Option<OldModelView>,
    /// Items per second the last re-read measured.
    pub rate: Option<f64>,
}

pub struct ModelsHub {
    pub(super) opts: HubOptions,
    /// Held by whatever decides about the models on disk (a change, a reset, removing the old
    /// model), so two of them never act on the same state.
    pub(super) deciding: tokio::sync::Mutex<()>,
    pub(super) jobs: tokio::sync::Mutex<Option<Arc<ModelJobs>>>,
    pub(super) state: Mutex<HubState>,
}

/// What the engine says about this computer and its catalogue.
pub(super) struct Host {
    pub catalogue: Vec<ModelInfo>,
    pub info: SystemInfo,
    pub budget: Budget,
}

pub(super) fn busy(what: &str) -> CommandError {
    CommandError::new(ErrorCode::Busy, what)
}

impl ModelsHub {
    pub fn new(opts: HubOptions) -> Self {
        ModelsHub {
            opts,
            deciding: tokio::sync::Mutex::new(()),
            jobs: tokio::sync::Mutex::new(None),
            state: Mutex::new(HubState::default()),
        }
    }

    pub(super) fn state(&self) -> MutexGuard<'_, HubState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(super) fn config_dir<R: Runtime>(app: &AppHandle<R>) -> std::path::PathBuf {
        config_dir(&app.state::<AppState>().data_dir)
    }

    pub(super) fn selection<R: Runtime>(app: &AppHandle<R>) -> Selection {
        selection::read(&Self::config_dir(app))
    }

    pub(super) async fn client<R: Runtime>(app: &AppHandle<R>) -> Option<Client> {
        app.state::<AppState>().services()?.client().await
    }

    pub(super) async fn control<R: Runtime>(app: &AppHandle<R>) -> Option<Control> {
        app.state::<AppState>().services()?.engine_control().await
    }

    /// The memory the fit is judged against: the machine's, unless the options say otherwise.
    pub fn ram(&self) -> u64 {
        self.opts.ram_bytes.unwrap_or_else(fit::ram_bytes)
    }

    /// The catalogue and the limits, from the engine when it is running.
    pub(super) async fn host<R: Runtime>(&self, app: &AppHandle<R>) -> Option<Host> {
        let client = Self::client(app).await?;
        let catalogue = client.models().await.ok()?;
        let info = client.system_info().await.unwrap_or_default();
        let disk = info.free_bytes().unwrap_or(0);
        let budget = fit::for_host(&info, self.ram(), disk);
        Some(Host {
            catalogue,
            info,
            budget,
        })
    }

    /// The whole picture: each job's model, the memory meter, and any change, re-read or old
    /// model to show.
    pub async fn view<R: Runtime>(&self, app: &AppHandle<R>) -> ModelsView {
        let s = Self::selection(app);
        let host = self.host(app).await;
        let catalogue = host.as_ref().map(|h| h.catalogue.as_slice()).unwrap_or(&[]);
        let budget = host.as_ref().map_or_else(
            || fit::for_host(&SystemInfo::default(), self.ram(), 0),
            |h| h.budget,
        );
        let size_of = |id: &str| {
            catalogue
                .iter()
                .find(|m| m.id == id)
                .map(|m| fit::gb(m.size))
        };
        let jobs = job_views(&s, size_of);
        let reindex = self.reindex_view(app);
        let st = self.state();
        let change = st
            .change
            .as_ref()
            .map(|c| ChangeView {
                job: fit::job_of(c.role),
                model: c.model.clone(),
                from: c.from.clone(),
                phase: if c.progress.failed {
                    ChangePhase::Failed {
                        received: c.progress.received as f64,
                        total: c.progress.total as f64,
                    }
                } else if c.progress.done {
                    ChangePhase::Ready
                } else {
                    ChangePhase::Downloading {
                        received: c.progress.received as f64,
                        total: c.progress.total as f64,
                        paused: c.progress.paused,
                    }
                },
                resumed: c.resumed,
            })
            .or_else(|| {
                // A change from before the last launch that nothing here is fetching: one the
                // person paused, one that stopped by itself, or one about to carry on.
                s.pending.as_ref().map(|p| {
                    let total = size_of(&p.model).unwrap_or(0) as f64;
                    ChangeView {
                        job: fit::job_of(p.role),
                        model: p.model.clone(),
                        from: p.from.clone(),
                        phase: if p.failed {
                            ChangePhase::Failed {
                                received: 0.0,
                                total,
                            }
                        } else {
                            ChangePhase::Downloading {
                                received: 0.0,
                                total,
                                paused: p.paused,
                            }
                        },
                        resumed: true,
                    }
                })
            });
        let old = st.removed.clone().or_else(|| {
            s.old.as_ref().map(|o| OldModelView {
                job: fit::job_of(o.role),
                model: o.model.clone(),
                bytes: size_of(&o.model).unwrap_or(0) as f64,
                removed: false,
            })
        });
        let attention = matches!(
            change.as_ref().map(|c| &c.phase),
            Some(ChangePhase::Failed { .. })
        );
        ModelsView {
            preset: s.tier,
            custom: !s.custom.is_empty(),
            ram_bytes: budget.ram as f64,
            budget_bytes: budget.budget as f64,
            busiest_bytes: fit::busiest(&s, catalogue, &budget) as f64,
            jobs,
            change,
            reindex: reindex.or_else(|| st.ended.clone()),
            old,
            attention,
        }
    }

    fn reindex_view<R: Runtime>(&self, app: &AppHandle<R>) -> Option<ReindexView> {
        let engine = app.state::<AppState>().engine().ok()?;
        let row = engine
            .reindex_state()
            .ok()
            .flatten()
            .filter(|r| r.phase == ReindexPhase::Running)?;
        let (done, total) = engine.reindex_progress().unwrap_or((0, row.total));
        let st = self.state();
        let rate = st.rate.unwrap_or(ASSUMED_PER_SECOND);
        let eta = st
            .reindex
            .as_ref()
            .and_then(|r| r.eta)
            .or_else(|| estimate_seconds(total.saturating_sub(done), rate));
        Some(ReindexView {
            model: row.model,
            from: row.from_model,
            done,
            total,
            eta_seconds: eta,
            phase: ReindexPhase::Running,
        })
    }

    pub(super) async fn emit<R: Runtime>(&self, app: &AppHandle<R>) -> ModelsView {
        let view = self.view(app).await;
        if let Err(e) = app.emit(EVENT_MODELS, &view) {
            tracing::warn!("could not emit the models view: {e}");
        }
        view
    }

    /// Every model the catalogue offers for `job`, judged against this computer.
    pub async fn choices<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        job: Job,
    ) -> Result<Vec<ModelChoice>, CommandError> {
        let host = self
            .host(app)
            .await
            .ok_or_else(|| CommandError::unavailable("the private engine is not running"))?;
        let s = Self::selection(app);
        Ok(fit::choices(
            job,
            &host.catalogue,
            &host.info,
            &s,
            &host.budget,
        ))
    }

    /// What changing `job` to `model` would mean: the download, the space, the memory, and for
    /// the finding model the re-read.
    pub async fn preview<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        job: Job,
        model: &str,
    ) -> Result<ChangePreview, CommandError> {
        let host = self
            .host(app)
            .await
            .ok_or_else(|| CommandError::unavailable("the private engine is not running"))?;
        let m = offered(job, model, &host.catalogue)?;
        let s = Self::selection(app);
        let role = role_of(job);
        let judged = fit::judge(job, m, &host.catalogue, &host.info, &s, &host.budget);
        let download = if m.present() { 0 } else { fit::gb(m.size) };
        let chosen = fit::with_choice(&s, role, model);
        let now = fit::busiest(&s, &host.catalogue, &host.budget);
        let mut after = fit::busiest(&chosen, &host.catalogue, &host.budget);
        if job == Job::Finding {
            // While memories are read again, asks still find them with the old model and the
            // re-read uses the new one. The engine keeps one finding model loaded at a time
            // (`max_loaded_models` 1 evicts the other before a load), so the larger of the two
            // sets is the most this needs.
            after = after.max(now);
        }
        let (reindex_items, reindex_eta_seconds) = if job == Job::Finding {
            let total = app
                .state::<AppState>()
                .engine()
                .ok()
                .and_then(|e| e.reindex_progress().ok())
                .map_or(0, |(_, total)| total);
            let rate = self.state().rate.unwrap_or(ASSUMED_PER_SECOND);
            (Some(total), estimate_seconds(total, rate))
        } else {
            (None, None)
        };
        Ok(ChangePreview {
            job,
            model: model.to_string(),
            from: selection::picks(&s).get(role).to_string(),
            download_bytes: download as f64,
            free_now: host.budget.disk_free as f64,
            free_after: host.budget.disk_free.saturating_sub(download) as f64,
            memory_now: now as f64,
            memory_after: after as f64,
            fit: judged.fit,
            smaller: judged.smaller,
            tested: judged.tested,
            reindex_items,
            reindex_eta_seconds,
            new_only: job == Job::Recordings,
        })
    }

    /// The person keeps the old model: it stays on disk and the offer to remove it goes.
    pub async fn keep_old<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<ModelsView, CommandError> {
        let dir = Self::config_dir(app);
        selection::update(&dir, |s| s.old = None).map_err(CommandError::internal)?;
        self.state().removed = None;
        Ok(self.emit(app).await)
    }

    /// The person removes the old model: it is deleted, unless a job uses it again meanwhile.
    pub async fn remove_old<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<ModelsView, CommandError> {
        let _deciding = self.deciding.lock().await;
        let dir = Self::config_dir(app);
        let s = selection::read(&dir);
        let Some(old) = s.old.clone() else {
            return Ok(self.view(app).await);
        };
        let pending = s.pending.as_ref().map(|p| p.model.as_str());
        let rereading = app
            .state::<AppState>()
            .engine()
            .ok()
            .and_then(|e| e.reindex_state().ok().flatten())
            .filter(|r| r.phase == ReindexPhase::Running)
            .is_some_and(|r| r.model == old.model || r.from_model == old.model);
        if in_use(&s, &old.model) || pending == Some(old.model.as_str()) || rereading {
            return Err(CommandError::invalid("that model is in use"));
        }
        let client = Self::client(app)
            .await
            .ok_or_else(|| CommandError::unavailable("the private engine is not running"))?;
        let bytes = nocturne_lemonade::jobs::remove(&client, &old.model)
            .await
            .map_err(CommandError::internal)?;
        selection::update(&dir, |s| {
            if s.old.as_ref().is_some_and(|o| o.model == old.model) {
                s.old = None;
            }
        })
        .map_err(CommandError::internal)?;
        self.state().removed = Some(OldModelView {
            job: fit::job_of(old.role),
            model: old.model,
            bytes: bytes as f64,
            removed: true,
        });
        Ok(self.emit(app).await)
    }

    /// Every job back to the tier's own model, when each is on disk. The finding model goes
    /// back through a re-read.
    pub async fn reset<R: Runtime>(
        self: &Arc<Self>,
        app: &AppHandle<R>,
    ) -> Result<ModelsView, CommandError> {
        let _deciding = self.deciding.lock().await;
        let dir = Self::config_dir(app);
        let s = selection::read(&dir);
        if s.pending.is_some() || self.state().change.is_some() || self.state().reindex.is_some() {
            return Err(busy("a change is under way"));
        }
        let host = self
            .host(app)
            .await
            .ok_or_else(|| CommandError::unavailable("the private engine is not running"))?;
        let present = |id: &str| host.catalogue.iter().any(|m| m.id == id && m.present());
        let own = |r| nocturne_lemonade::tier::model_for(s.tier, r).unwrap_or_default();
        if s.custom.keys().any(|r| !present(own(*r))) {
            return Err(CommandError::invalid(
                "the tier's own model is no longer on this computer",
            ));
        }
        let recall = s.custom.get(&ComponentRole::Recall).cloned();
        selection::update(&dir, |now| {
            let mut reset = Selection::preset(now.tier);
            if let Some(r) = &recall {
                reset.custom.insert(ComponentRole::Recall, r.clone());
            }
            reset.old = now.old.clone();
            *now = reset;
        })
        .map_err(CommandError::internal)?;
        crate::runtime::lemonade::refill_mind(app).await;
        if recall.is_some() {
            self.decide(app, Job::Finding, own(ComponentRole::Recall), false)
                .await?;
        }
        Ok(self.emit(app).await)
    }

    /// Before the store is wiped: the download is cancelled and the re-read stopped.
    pub async fn stop_everything<R: Runtime>(&self, app: &AppHandle<R>) {
        let jobs = self.jobs.lock().await.clone();
        if let Some(jobs) = jobs {
            let _ = jobs.cancel().await;
        }
        let stop = self.state().reindex.take().map(|r| r.stop);
        if let Some(stop) = stop {
            stop.cancel();
        }
        if let Ok(engine) = app.state::<AppState>().engine() {
            let _ = engine.cancel_reindex();
        }
        self.forget();
    }

    /// A model download is under way (not paused, not stopped).
    pub fn downloading(&self) -> bool {
        self.state()
            .change
            .as_ref()
            .is_some_and(|c| !c.progress.done && !c.progress.failed && !c.progress.paused)
    }

    /// Forgets what this session showed.
    pub fn forget(&self) {
        *self.state() = HubState::default();
    }
}

/// `model` from the catalogue, if it may do `job`. Ids outside the catalogue (`user.*`) never.
pub(super) fn offered<'a>(
    job: Job,
    model: &str,
    catalogue: &'a [ModelInfo],
) -> Result<&'a ModelInfo, CommandError> {
    if model.starts_with("user.") {
        return Err(CommandError::invalid("only models from the catalogue"));
    }
    let m = catalogue
        .iter()
        .find(|m| m.id == model)
        .ok_or_else(|| CommandError::not_found("no such model in the catalogue"))?;
    fit::qualifies(job, m).map_err(|e| CommandError::invalid(format!("{e:?}")))?;
    Ok(m)
}

/// Whether a job of `s` uses `model`.
pub(super) fn in_use(s: &Selection, model: &str) -> bool {
    let picks = selection::picks(s);
    super::ROLES.into_iter().any(|r| picks.get(r) == model)
}

/// Whether `fit` allows a change.
pub(super) fn usable(fit: Fit) -> bool {
    matches!(fit, Fit::Fits | Fit::Tight)
}
