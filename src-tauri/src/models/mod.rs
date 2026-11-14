//! Choosing the model for each job, in the app: the hub that judges choices, fetches a model
//! with the person's yes and switches to it once it is on disk, and the re-read a new finding
//! model needs. Tiers stay the preset; `selection.json` holds the person's changes.

mod changes;
mod controller;
mod reindex;

use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use nocturne_contract::{ComponentRole, Job, JobView, ModelsView, Tier};
use nocturne_lemonade::fit::{self, display_name};
use nocturne_lemonade::selection::{self, Selection};
use nocturne_lemonade::{fit_table, tier};
use tauri::{AppHandle, Manager, Runtime};

pub use controller::{HubOptions, ModelsHub};

use crate::state::AppState;

/// Called once the app is ready.
pub fn install<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn Error>> {
    // The store's vectors name the finding model until the mind arrives and says which it uses.
    if let Ok(engine) = app.state::<AppState>().engine()
        && let Ok(Some(indexed)) = engine.indexed_model()
    {
        engine.set_recall_model(&indexed);
    }
    if app.try_state::<Arc<ModelsHub>>().is_none() {
        let opts = app
            .try_state::<HubOptions>()
            .map(|o| *o.inner())
            .unwrap_or_default();
        app.manage(Arc::new(ModelsHub::new(opts)));
    }
    Ok(())
}

/// The hub, once installed.
pub fn hub<R: Runtime>(app: &AppHandle<R>) -> Option<Arc<ModelsHub>> {
    app.try_state::<Arc<ModelsHub>>().map(|h| h.inner().clone())
}

/// Before the store is wiped, once the phrase is right: no download or re-read carries on.
pub async fn before_delete_everything<R: Runtime>(app: &AppHandle<R>) {
    if let Some(hub) = hub(app) {
        hub.stop_everything(app).await;
    }
}

/// After the store is wiped: every job goes back to its tier's own model.
pub fn on_delete_everything<R: Runtime>(app: &AppHandle<R>) {
    let dir = config_dir(&app.state::<AppState>().data_dir);
    let custom = !selection::read(&dir).custom.is_empty();
    if let Err(e) = selection::clear(&dir) {
        tracing::warn!("the models chosen for each job could not be cleared: {e}");
    }
    if let Some(hub) = hub(app) {
        hub.forget();
    }
    if custom {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            crate::runtime::lemonade::refill_mind(&app).await;
        });
    }
}

/// The mind is in the slot: a change or re-read that a restart interrupted carries on.
pub fn on_mind_ready<R: Runtime>(app: &AppHandle<R>) {
    if let Some(hub) = hub(app) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            hub.carry_on(&app).await;
        });
    }
}

/// A re-read that swapped its vectors in just before a crash, with `selection.json` still
/// naming the old finding model: the selection is brought in line with the store.
pub fn repair_after_swap<R: Runtime>(app: &AppHandle<R>, config_dir: &Path) {
    let Ok(engine) = app.state::<AppState>().engine() else {
        return;
    };
    let (Ok(Some(row)), Ok(Some(indexed))) = (engine.reindex_state(), engine.indexed_model())
    else {
        return;
    };
    let fixed = selection::update(config_dir, |s| {
        if row.phase != nocturne_contract::ReindexPhase::Done
            || row.model != indexed
            || selection::picks(s).recall == indexed
        {
            return;
        }
        *s = fit::with_choice(s, ComponentRole::Recall, &indexed);
        if s.pending
            .as_ref()
            .is_some_and(|p| p.role == ComponentRole::Recall)
        {
            s.pending = None;
        }
    });
    if let Err(e) = fixed {
        tracing::warn!("the finding model could not be repaired: {e}");
    }
}

/// The memory the fit is judged against.
pub fn ram_bytes<R: Runtime>(app: &AppHandle<R>) -> u64 {
    hub(app).map_or_else(fit::ram_bytes, |h| h.ram())
}

/// A new engine was started in place of one that gave up: model downloads follow it.
pub fn on_engine<R: Runtime>(app: &AppHandle<R>, control: nocturne_lemonade::Control) {
    if let Some(hub) = hub(app) {
        tauri::async_runtime::spawn(async move {
            hub.follow_engine(control).await;
        });
    }
}

/// The job a component role does.
pub fn job_of(role: ComponentRole) -> Job {
    fit::job_of(role)
}

/// The engine's settings folder under the data folder.
pub fn config_dir(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("engine").join("config")
}

/// The roles in the order the jobs are listed.
pub(crate) const ROLES: [ComponentRole; 4] = [
    ComponentRole::Mind,
    ComponentRole::Speech,
    ComponentRole::Voice,
    ComponentRole::Recall,
];

/// The model doing each job of `s`, without the engine: sizes from the tiers, memory from the
/// table.
pub(crate) fn job_views(s: &Selection, size_of: impl Fn(&str) -> Option<u64>) -> Vec<JobView> {
    if s.tier == Tier::Unsupported {
        return Vec::new();
    }
    let picks = selection::picks(s);
    ROLES
        .into_iter()
        .filter(|r| !picks.get(*r).is_empty())
        .map(|role| {
            let model = picks.get(role).to_string();
            let size = size_of(&model)
                .or_else(|| {
                    [Tier::Large, Tier::Standard, Tier::Small, Tier::Tiny]
                        .into_iter()
                        .flat_map(tier::components)
                        .find(|c| c.model == model)
                        .map(|c| c.bytes)
                })
                .unwrap_or(0);
            JobView {
                job: job_of(role),
                name: display_name(&model),
                size_bytes: size as f64,
                memory_bytes: fit::memory_of(&model, None) as f64,
                recommended: tier::model_for(s.tier, role) == Some(model.as_str()),
                tested: fit_table::lookup(&model).is_some_and(|e| e.tested),
                model,
            }
        })
        .collect()
}

/// The view of the selection in `config_dir` without the engine: each job's model.
pub fn preset_view(config_dir: &Path) -> ModelsView {
    let s: Selection = selection::read(config_dir);
    ModelsView {
        preset: s.tier,
        custom: !s.custom.is_empty(),
        ram_bytes: 0.0,
        budget_bytes: 0.0,
        busiest_bytes: 0.0,
        jobs: job_views(&s, |_| None),
        change: None,
        reindex: None,
        old: None,
        attention: false,
    }
}
