//! Settings, privacy, pause, export, delete everything and the advanced details.

use std::path::PathBuf;

use jiff::Timestamp;
use nocturne_contract::{
    AdvancedView, EVENT_EXPORT, ExportProgress, ExportReport, PrivacyView, SettingsPatch,
    SettingsView,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{change, engine, read, run};
use crate::error::{CommandError, blocking, blocking_cmd};
use crate::state::{AppState, refresh_status};

#[tauri::command]
pub async fn get_settings<R: Runtime>(app: AppHandle<R>) -> Result<SettingsView, CommandError> {
    read(app, |e| e.settings()).await
}

#[tauri::command]
pub async fn update_settings<R: Runtime>(
    app: AppHandle<R>,
    patch: SettingsPatch,
) -> Result<SettingsView, CommandError> {
    change(app, move |e| e.update_settings(patch)).await
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
    change(app.clone(), move |e| e.delete_everything(&confirm_phrase)).await?;
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
        let state = app.state::<AppState>();
        if let Some(u) = &until {
            let at: Timestamp = u.parse().map_err(|_| {
                CommandError::invalid("pause until a time such as 2026-11-13T09:00:00Z")
            })?;
            if at <= state.now() {
                return Err(CommandError::invalid("that time has passed"));
            }
        }
        let handle = app.clone();
        blocking_cmd(move || {
            handle.state::<AppState>().status.pause(until);
            Ok(())
        })
        .await?;
        refresh_status(&app).await;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn resume<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    run(app, move |app| async move {
        let state = app.state::<AppState>();
        let handle = app.clone();
        blocking_cmd(move || {
            handle.state::<AppState>().status.resume();
            Ok(())
        })
        .await?;
        state.wake_pipeline();
        refresh_status(&app).await;
        Ok(())
    })
    .await
}

/// The technical details. What the shell does not know yet (the private engine's version, port,
/// components) is empty until the runtime lane fills it in.
#[tauri::command]
pub async fn get_advanced<R: Runtime>(app: AppHandle<R>) -> Result<AdvancedView, CommandError> {
    run(app, move |app| async move {
        let state = app.state::<AppState>();
        let e = engine(&app)?;
        let (stats, agents) = blocking(move || Ok((e.stats()?, e.settings()?.agents))).await?;
        Ok(AdvancedView {
            tier: None,
            ram_gib: 0,
            gpu: None,
            npu: false,
            components: Vec::new(),
            engine_version: None,
            engine_port: None,
            engine_uptime_seconds: None,
            restarts: 0,
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
