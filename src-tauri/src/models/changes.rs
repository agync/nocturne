//! A change of model for one job: at once when the model is on disk, otherwise after a download
//! the person agreed to, inside the shared download window. The current model keeps working
//! until the new one is on disk; then the job switches and the old model is kept until the
//! person removes it. A change of the finding model goes on to a re-read (`reindex`).

use std::sync::Arc;

use nocturne_contract::{
    ComponentRole, Job, LedgerKind, ModelsView, Notice, NoticeCode, SetupPhase,
};
use nocturne_lemonade::fit::{self, role_of};
use nocturne_lemonade::jobs::{JobProgress, ModelJobs, OnDownload};
use nocturne_lemonade::selection::{self, OldModel, Pending};
use tauri::{AppHandle, Manager, Runtime};

use super::controller::{Change, ModelsHub, busy, in_use, offered, usable};
use crate::error::CommandError;
use crate::state::{AppState, refresh_status};

/// The notice shown when a download the person agreed to stopped.
pub const STOPPED_NOTICE: &str = "model-download";

impl ModelsHub {
    /// Changes `job` to `model`. A model on disk is used at once; one that is not is fetched
    /// only with `allow_download`, the person's yes. Only catalogue models that qualify for the
    /// job and fit this computer, one change at a time, and none while setup downloads.
    pub async fn change<R: Runtime>(
        self: &Arc<Self>,
        app: &AppHandle<R>,
        job: Job,
        model: &str,
        allow_download: bool,
    ) -> Result<ModelsView, CommandError> {
        if model.starts_with("user.") {
            return Err(CommandError::invalid("only models from the catalogue"));
        }
        if setup_running(app) {
            return Err(busy("setup is downloading"));
        }
        let host = self
            .host(app)
            .await
            .ok_or_else(|| CommandError::unavailable("the private engine is not running"))?;
        let m = offered(job, model, &host.catalogue)?.clone();
        let dir = Self::config_dir(app);
        let s = selection::read(&dir);
        {
            let st = self.state();
            if s.pending.is_some() || st.change.is_some() || st.reindex.is_some() {
                return Err(busy("a change is under way"));
            }
        }
        let role = role_of(job);
        let from = selection::picks(&s).get(role).to_string();
        if from == model {
            return Ok(self.view(app).await);
        }
        let judged = fit::judge(job, &m, &host.catalogue, &host.info, &s, &host.budget);
        if !usable(judged.fit) {
            return Err(CommandError::invalid(
                "that model does not fit this computer",
            ));
        }
        self.state().ended = None;
        self.state().removed = None;
        if m.present() {
            self.switch(app, role, model, &from).await?;
            return Ok(self.emit(app).await);
        }
        if !allow_download {
            return Err(CommandError::invalid(
                "a download needs the person's agreement",
            ));
        }
        let mut taken = false;
        selection::update(&dir, |s| {
            if s.pending.is_none() {
                s.pending = Some(Pending {
                    role,
                    model: model.to_string(),
                    from: from.clone(),
                    paused: false,
                    failed: false,
                });
                taken = true;
            }
        })
        .map_err(CommandError::internal)?;
        if !taken {
            return Err(busy("a change is under way"));
        }
        if let Err(e) = self.fetch(app, role, model, &from, false).await {
            // Nothing started: no download is waiting for the next launch either.
            let _ = selection::update(&dir, |s| {
                if s.pending.as_ref().is_some_and(|p| p.model == model) {
                    s.pending = None;
                }
            });
            return Err(e);
        }
        Ok(self.emit(app).await)
    }

    /// The download job runner, following the engine running now.
    async fn jobs<R: Runtime>(&self, app: &AppHandle<R>) -> Result<Arc<ModelJobs>, CommandError> {
        let control = Self::control(app)
            .await
            .ok_or_else(|| CommandError::unavailable("the private engine is not running"))?;
        let mut slot = self.jobs.lock().await;
        let jobs = slot
            .get_or_insert_with(|| {
                Arc::new(ModelJobs::with_options(control.clone(), self.opts.jobs))
            })
            .clone();
        drop(slot);
        jobs.follow(control).await.map_err(CommandError::internal)?;
        Ok(jobs)
    }

    /// The engine was replaced (it gave up and was started again): downloads follow it.
    pub async fn follow_engine(&self, control: nocturne_lemonade::Control) {
        let jobs = self.jobs.lock().await.clone();
        if let Some(jobs) = jobs
            && let Err(e) = jobs.follow(control).await
        {
            tracing::warn!("a model download could not follow the new engine: {e}");
        }
    }

    /// Starts the download for a pending change and follows it.
    pub(super) async fn fetch<R: Runtime>(
        self: &Arc<Self>,
        app: &AppHandle<R>,
        role: ComponentRole,
        model: &str,
        from: &str,
        resumed: bool,
    ) -> Result<(), CommandError> {
        let jobs = self.jobs(app).await?;
        let control = Self::control(app)
            .await
            .ok_or_else(|| CommandError::unavailable("the private engine is not running"))?;
        let hold = control
            .hold_online()
            .await
            .map_err(CommandError::internal)?;
        let ledger = app.state::<AppState>().engine().ok();
        let on_download: OnDownload = Arc::new(move |model: &str, bytes: u64| {
            if let Some(engine) = ledger.clone() {
                let label = model.to_string();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(e) = engine.record_download(
                        LedgerKind::ModelDownload,
                        &label,
                        None,
                        bytes as f64,
                    ) {
                        tracing::warn!("a download could not be written to the ledger: {e}");
                    }
                });
            }
        });
        let mut rx = jobs
            .get(model, hold, on_download)
            .await
            .map_err(CommandError::internal)?;
        self.state().change = Some(Change {
            role,
            model: model.to_string(),
            from: from.to_string(),
            progress: rx.borrow().clone(),
            resumed,
        });
        app.state::<AppState>().status.clear_notice(STOPPED_NOTICE);
        let (hub, app) = (self.clone(), app.clone());
        let (model, from) = (model.to_string(), from.to_string());
        tauri::async_runtime::spawn(async move {
            loop {
                let p: JobProgress = rx.borrow_and_update().clone();
                let still_ours = {
                    let mut st = hub.state();
                    match st.change.as_mut().filter(|c| c.model == model) {
                        Some(c) => {
                            c.progress = p.clone();
                            true
                        }
                        None => false,
                    }
                };
                if !still_ours {
                    return;
                }
                if p.done {
                    if let Err(e) = hub.switch(&app, role, &model, &from).await {
                        tracing::warn!("the downloaded model could not be switched in: {e:?}");
                    }
                    hub.emit(&app).await;
                    return;
                }
                if p.failed {
                    let dir = Self::config_dir(&app);
                    let _ = selection::update(&dir, |s| {
                        if let Some(p) = s.pending.as_mut().filter(|p| p.model == model) {
                            p.failed = true;
                        }
                    });
                    stopped(&app).await;
                    hub.emit(&app).await;
                    return;
                }
                hub.emit(&app).await;
                if rx.changed().await.is_err() {
                    return;
                }
            }
        });
        Ok(())
    }

    /// `model` is on disk: `role` uses it from now on (the finding model once its re-read is
    /// done). The model it replaces is kept.
    pub(super) async fn switch<R: Runtime>(
        self: &Arc<Self>,
        app: &AppHandle<R>,
        role: ComponentRole,
        model: &str,
        from: &str,
    ) -> Result<(), CommandError> {
        let dir = Self::config_dir(app);
        self.state().change = None;
        if role == ComponentRole::Recall {
            selection::update(&dir, |s| s.pending = None).map_err(CommandError::internal)?;
            return self.begin_reindex(app, model, from);
        }
        selection::update(&dir, |s| {
            *s = fit::with_choice(s, role, model);
            s.pending = None;
            s.old = (!from.is_empty() && from != model && !in_use(s, from)).then(|| OldModel {
                role,
                model: from.to_string(),
            });
        })
        .map_err(CommandError::internal)?;
        crate::runtime::lemonade::refill_mind(app).await;
        Ok(())
    }

    pub async fn pause<R: Runtime>(&self, app: &AppHandle<R>) -> Result<ModelsView, CommandError> {
        let jobs = self.jobs.lock().await.clone();
        if let Some(jobs) = jobs {
            jobs.pause().await.map_err(CommandError::internal)?;
        }
        self.mark(app, true)?;
        Ok(self.emit(app).await)
    }

    /// Carries a paused download on, or tries a failed or interrupted one again.
    pub async fn resume<R: Runtime>(
        self: &Arc<Self>,
        app: &AppHandle<R>,
    ) -> Result<ModelsView, CommandError> {
        let s = Self::selection(app);
        let Some(p) = s.pending.clone() else {
            return Ok(self.view(app).await);
        };
        if setup_running(app) {
            return Err(busy("setup is downloading"));
        }
        let jobs = self.jobs(app).await?;
        let running = jobs.model().is_some_and(|m| m == p.model);
        let failed = p.failed
            || self
                .state()
                .change
                .as_ref()
                .is_some_and(|c| c.progress.failed);
        self.mark(app, false)?;
        if running && !failed {
            jobs.resume().await.map_err(CommandError::internal)?;
        } else {
            let resumed = self.state().change.as_ref().is_none_or(|c| c.resumed);
            self.state().change = None;
            self.fetch(app, p.role, &p.model, &p.from, resumed).await?;
        }
        Ok(self.emit(app).await)
    }

    /// Stops the change: the download and what it received go, the current model stays.
    pub async fn cancel_change<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<ModelsView, CommandError> {
        let jobs = self.jobs.lock().await.clone();
        if let Some(jobs) = jobs {
            jobs.cancel().await.map_err(CommandError::internal)?;
        }
        let dir = Self::config_dir(app);
        let mut pending = None;
        let s = selection::update(&dir, |s| pending = s.pending.take())
            .map_err(CommandError::internal)?;
        self.state().change = None;
        // A download interrupted by a restart has no job here: its partial files go too.
        if let Some(p) = pending
            && !in_use(&s, &p.model)
            && let Some(client) = Self::client(app).await
            && !client
                .models()
                .await
                .is_ok_and(|ms| ms.iter().any(|m| m.id == p.model && m.present()))
        {
            let _ = client.delete(&p.model).await;
        }
        app.state::<AppState>().status.clear_notice(STOPPED_NOTICE);
        refresh_status(app).await;
        Ok(self.emit(app).await)
    }

    /// Records the person's pause (or that they want it to carry on), and clears a failure.
    fn mark<R: Runtime>(&self, app: &AppHandle<R>, paused: bool) -> Result<(), CommandError> {
        let dir = Self::config_dir(app);
        selection::update(&dir, |s| {
            if let Some(p) = s.pending.as_mut() {
                p.paused = paused;
                if !paused {
                    p.failed = false;
                }
            }
        })
        .map_err(CommandError::internal)?;
        if let Some(c) = self.state().change.as_mut() {
            c.progress.paused = paused;
        }
        Ok(())
    }

    /// After a launch: a change whose model arrived switches; one that was downloading carries
    /// on; one the person paused, or that had stopped by itself, waits for them; a re-read
    /// under way goes on.
    pub async fn carry_on<R: Runtime>(self: &Arc<Self>, app: &AppHandle<R>) {
        self.resume_reindex(app);
        let s = Self::selection(app);
        let Some(p) = s.pending.clone() else {
            return;
        };
        if self.state().change.is_some() {
            return;
        }
        let Some(client) = Self::client(app).await else {
            return;
        };
        let present = client
            .models()
            .await
            .is_ok_and(|ms| ms.iter().any(|m| m.id == p.model && m.present()));
        if present {
            if let Err(e) = self.switch(app, p.role, &p.model, &p.from).await {
                tracing::warn!("a change that finished before the restart did not switch: {e:?}");
            }
        } else if !p.paused
            && !p.failed
            && !setup_running(app)
            && let Err(e) = self.fetch(app, p.role, &p.model, &p.from, true).await
        {
            tracing::warn!("a change could not carry on after the restart: {e:?}");
        }
        self.emit(app).await;
    }
}

/// Setup is fetching a tier: a model change waits for it.
fn setup_running<R: Runtime>(app: &AppHandle<R>) -> bool {
    matches!(
        app.state::<AppState>().get_setup().phase,
        SetupPhase::Downloading | SetupPhase::Installing | SetupPhase::Paused
    )
}

/// The download stopped by itself: a notice says so; the current model keeps working.
async fn stopped<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    state.status.add_notice(Notice {
        id: STOPPED_NOTICE.to_string(),
        code: NoticeCode::ModelDownloadStopped,
        item_id: None,
        eta_seconds: None,
        bytes: None,
    });
    refresh_status(app).await;
}
