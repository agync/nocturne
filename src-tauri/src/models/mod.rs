//! Choosing the model for each job, in the app. For now every job uses its tier's preset.

use std::error::Error;
use std::path::Path;

use nocturne_contract::{ComponentRole, Job, JobView, ModelsView, Tier};
use nocturne_lemonade::selection::{self, Selection};
use nocturne_lemonade::tier;
use tauri::{AppHandle, Runtime};

/// Called once the app is ready.
pub fn install<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn Error>> {
    let _ = app;
    Ok(())
}

/// Before the store is wiped, once the phrase is right.
pub async fn before_delete_everything<R: Runtime>(app: &AppHandle<R>) {
    let _ = app;
}

/// After the store is wiped.
pub fn on_delete_everything<R: Runtime>(app: &AppHandle<R>) {
    let _ = app;
}

/// The job a component role does.
pub fn job_of(role: ComponentRole) -> Job {
    match role {
        ComponentRole::Mind => Job::Answering,
        ComponentRole::Speech => Job::Recordings,
        ComponentRole::Voice => Job::Aloud,
        ComponentRole::Recall => Job::Finding,
    }
}

/// The engine's settings folder under the data folder.
pub fn config_dir(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("engine").join("config")
}

/// The view of the selection in `config_dir`: the tier's own model for each job.
pub fn preset_view(config_dir: &Path) -> ModelsView {
    let s: Selection = selection::read(config_dir);
    let jobs = if s.tier == Tier::Unsupported {
        Vec::new()
    } else {
        tier::components(s.tier)
            .into_iter()
            .map(|c| JobView {
                job: job_of(c.role),
                model: c.model.to_string(),
                name: c.model.to_string(),
                size_bytes: c.bytes as f64,
                memory_bytes: 0.0,
                recommended: true,
                tested: true,
            })
            .collect()
    };
    ModelsView {
        preset: s.tier,
        custom: !s.custom.is_empty(),
        ram_bytes: 0.0,
        budget_bytes: 0.0,
        busiest_bytes: 0.0,
        jobs,
        change: None,
        reindex: None,
        old: None,
        attention: false,
    }
}
