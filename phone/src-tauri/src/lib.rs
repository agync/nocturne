//! Nocturne on the phone: the phone screens in a web view, and in Rust either the sealed channel
//! to the person's computer or a memory of the phone's own. The web view reaches nothing on the
//! network itself.

pub mod commands;
pub mod home;
pub mod sources;
pub mod state;
pub mod store;
pub mod sync;
#[cfg(test)]
mod sync_tests;
pub mod uploads;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_nocturne_system::init())
        .plugin(tauri_plugin_nocturne_calendar::init())
        .plugin(tauri_plugin_nocturne_health::init())
        .plugin(tauri_plugin_nocturne_speech::init());
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());
    builder
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            app.manage(state::AppState::new(dir.clone()));
            app.manage(home::Home::new(
                dir,
                std::sync::Arc::new(nocturne_engine::SystemClock),
                exclude_from_backup(app.handle()),
            ));
            app.manage(home::r#move::Moving::default());
            sources::start(app.handle());
            home::commands::fade_soon(app.handle());
            home::commands::clear_backup_copies(app.handle());
            home::r#move::clear_old_put_aside(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::pair_scanned,
            commands::check_address,
            commands::pair_typed,
            commands::pair_connect,
            commands::device,
            commands::connect,
            commands::connection,
            commands::call,
            commands::ask,
            commands::keep_begin,
            commands::keep_part,
            commands::keep_end,
            commands::keep_cancel,
            commands::original,
            commands::relink,
            commands::disconnect,
            sources::sources,
            sources::calendar_status,
            sources::calendar_request,
            sources::calendar_list,
            sources::calendar_settings,
            sources::calendar_save,
            sources::permissions_granted,
            sources::health_availability,
            sources::health_older_available,
            sources::health_request,
            sources::health_settings,
            sources::health_save,
            sources::open_settings,
            sources::sync_now,
            sources::last_sent,
            sources::launch_route,
            sources::phone_platform,
            home::commands::mode,
            home::commands::memory_damaged,
            home::commands::start_alone,
            home::commands::home_call,
            home::commands::home_keep_end,
            home::commands::home_original,
            home::commands::home_local,
            home::commands::hear_voice_notes,
            home::commands::voice_access,
            home::commands::voice_request,
            home::commands::save_backup_file,
            home::commands::choose_backup_file,
            home::commands::let_go_backup_file,
            home::commands::free_space,
            home::r#move::move_facts,
            home::r#move::move_offer,
            home::r#move::move_memory,
            home::r#move::move_stop,
            home::r#move::move_undo,
            home::r#move::move_put_aside,
            home::r#move::move_pending,
        ])
        .build(tauri::generate_context!())
        .expect("the phone app runs")
        .run(|app, event| {
            // Back to the front: send what changed while away, and fade what went unused.
            if let tauri::RunEvent::Resumed = event {
                sources::start(app);
                home::commands::fade_soon(app);
            }
        });
}

/// The store stays out of the phone's own backup. Android's backup rules already leave out every
/// folder of the app; an iPhone marks the folder itself.
fn exclude_from_backup<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) -> home::ExcludeFromBackup {
    #[cfg(target_os = "ios")]
    {
        let app = _app.clone();
        Box::new(move |path: &std::path::Path| {
            use tauri_plugin_nocturne_system::SystemExt;
            if let Err(e) = app.system().exclude_from_backup(path) {
                tracing::warn!("the memory could not be kept out of the phone's backup: {e:?}");
            }
        })
    }
    #[cfg(not(target_os = "ios"))]
    Box::new(|_: &std::path::Path| {})
}
