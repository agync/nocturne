//! Settings, privacy, pause, export, delete everything and the advanced details.

use std::path::PathBuf;

use nocturne_contract::{
    AdvancedView, EVENT_EXPORT, ExportProgress, ExportReport, PrivacyView, SettingsPatch,
    SettingsView,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{change, engine, read, run};
use crate::error::{CommandError, blocking, blocking_cmd};
use crate::runtime::EngineDetails;
use crate::state::{AppState, refresh_status};

#[tauri::command]
pub async fn get_settings<R: Runtime>(app: AppHandle<R>) -> Result<SettingsView, CommandError> {
    let view = read(app.clone(), |e| e.settings()).await?;
    Ok(crate::os::adjust_settings(&app, view))
}

#[tauri::command]
pub async fn update_settings<R: Runtime>(
    app: AppHandle<R>,
    patch: SettingsPatch,
) -> Result<SettingsView, CommandError> {
    run(app, move |app| async move {
        let before = read(app.clone(), |e| e.settings()).await?;
        crate::os::apply_settings(&app, &before, &patch).await?;
        let view = change(app.clone(), move |e| e.update_settings(patch)).await?;
        Ok(crate::os::adjust_settings(&app, view))
    })
    .await
}

/// What the privacy screen shows. The shell supplies what the engine cannot know: bytes sent
/// (and whether they are measured) and the size of the mind on disk.
#[tauri::command]
pub async fn get_privacy<R: Runtime>(app: AppHandle<R>) -> Result<PrivacyView, CommandError> {
    run(app, move |app| async move {
        let state = app.state::<AppState>();
        let e = engine(&app)?;
        let dir = state.data_dir.clone();
        let mut view = blocking(move || e.privacy(&dir)).await?;
        let (sent, measured) = state.status.sent();
        view.sent_to_internet_bytes = sent;
        view.sent_measured = measured;
        view.mind_bytes = state.status.mind_bytes();
        Ok(view)
    })
    .await
}

/// Write a copy of everything into a new folder inside `dir`.
#[tauri::command]
pub async fn export_to<R: Runtime>(
    app: AppHandle<R>,
    dir: String,
) -> Result<ExportReport, CommandError> {
    run(app, move |app| async move {
        let target = PathBuf::from(&dir);
        if !target.is_dir() {
            return Err(CommandError::invalid("choose an existing folder"));
        }
        let e = engine(&app)?;
        let handle = app.clone();
        let report = blocking(move || {
            e.export_to_with(&target, &|done, total| {
                if let Err(err) = handle.emit(EVENT_EXPORT, ExportProgress { done, total }) {
                    tracing::warn!("could not emit export progress: {err}");
                }
            })
        })
        .await?;
        // Only a folder made by an export in this session may be opened by `open_export`.
        app.state::<AppState>()
            .allow_export(std::path::Path::new(&report.path));
        Ok(report)
    })
    .await
}

/// Remove everything, after the exact phrase.
#[tauri::command]
pub async fn delete_everything<R: Runtime>(
    app: AppHandle<R>,
    confirm_phrase: String,
) -> Result<(), CommandError> {
    // Nothing in flight may answer from what is about to go.
    app.state::<AppState>().asks.cancel_all();
    // Nothing may land in the store after the wipe: the watched folder stops first.
    crate::os::before_delete_everything(&app).await;
    if let Err(e) = change(app.clone(), move |e| e.delete_everything(&confirm_phrase)).await {
        crate::os::after_failed_delete(&app).await;
        return Err(e);
    }
    crate::os::on_delete_everything(&app);
    let state = app.state::<AppState>();
    let handle = app.clone();
    blocking_cmd(move || {
        let state = handle.state::<AppState>();
        state.status.forget_persisted();
        let _ = std::fs::remove_dir_all(state.data_dir.join("tmp"));
        Ok(())
    })
    .await?;
    state.forget_session();
    refresh_status(&app).await;
    Ok(())
}

/// Pause reading until an instant, or (`null`) until the person resumes.
#[tauri::command]
pub async fn pause<R: Runtime>(
    app: AppHandle<R>,
    until: Option<String>,
) -> Result<(), CommandError> {
    run(app, move |app| async move {
        crate::state::pause_until(&app, until).await
    })
    .await
}

#[tauri::command]
pub async fn resume<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    run(app, move |app| async move {
        crate::state::resume_now(&app).await
    })
    .await
}

/// The technical details. The private engine's version, port, uptime, restarts and the tier come
/// from the running services; the hardware and the components are still empty.
#[tauri::command]
pub async fn get_advanced<R: Runtime>(app: AppHandle<R>) -> Result<AdvancedView, CommandError> {
    run(app, move |app| async move {
        let state = app.state::<AppState>();
        let e = engine(&app)?;
        let (stats, agents) = blocking(move || Ok((e.stats()?, e.settings()?.agents))).await?;
        let (tier, engine) = match state.services() {
            Some(services) => (services.tier(), services.engine_details().await),
            None => (None, EngineDetails::default()),
        };
        Ok(AdvancedView {
            tier,
            ram_gib: 0,
            gpu: None,
            npu: false,
            components: Vec::new(),
            engine_version: engine.version,
            engine_port: engine.port,
            engine_uptime_seconds: engine.uptime_seconds,
            restarts: engine.restarts,
            store_version: stats.store_version,
            memory_count: stats.memory_count,
            item_count: stats.item_count,
            integrity_ok: stats.integrity_ok,
            last_tidy: None,
            agents,
            log_dir: state.log_dir().to_string_lossy().into_owned(),
        })
    })
    .await
}
