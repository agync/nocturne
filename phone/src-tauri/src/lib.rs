//! Nocturne on the phone: the phone screens in a web view, and in Rust either the sealed channel
//! to the person's computer or a memory of the phone's own. The web view reaches nothing on the
//! network itself.

pub mod backup;
pub mod commands;
pub mod home;
pub mod log;
pub mod mind;
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
            log::start(&dir.join("logs"));
            app.manage(state::AppState::new(dir.clone()));
            let clock = std::sync::Arc::new(nocturne_engine::SystemClock);
            let home = home::Home::new(
                dir.clone(),
                clock.clone(),
                exclude_from_backup(app.handle()),
            );
            let nightly = backup::PhoneBackup::new(
                dir.clone(),
                Box::new(backup::SafeBackupKeys(app.handle().clone())),
                clock.clone(),
            );
            backup::place_account::attach(app.handle(), &nightly);
            let thinking =
                mind::PhoneMind::new(dir, Box::new(mind::SafeKeys(app.handle().clone())), clock);
            thinking.follow(&home);
            thinking.decode_with(mind::sound::decoder(app.handle()));
            thinking.tell(std::sync::Arc::new(mind::events::PhoneEvents(
                app.handle().clone(),
            )));
            app.manage(home);
            app.manage(thinking);
            app.manage(nightly);
            app.manage(home::r#move::Moving::default());
            sources::start(app.handle());
            home::commands::fade_soon(app.handle());
            home::commands::clear_backup_copies(app.handle());
            home::r#move::clear_old_put_aside(app.handle());
            mind::run(app.handle());
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
            home::commands::home_ask,
            home::commands::home_cancel_ask,
            home::commands::home_keep_end,
            home::commands::home_original,
            home::commands::home_local,
            home::commands::hear_voice_notes,
            home::commands::voice_access,
            home::commands::voice_request,
            home::commands::save_backup_file,
            home::commands::backup_stop,
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
            home::r#move::move_bring_aside,
            home::r#move::move_bring_stop,
            home::r#move::move_aside,
            mind::status::mind_sent,
            mind::status::mind_set_limit,
            mind::status::mind_month,
            mind::status::mind_reading,
            mind::status::mind_status,
            mind::commands::mind_check,
            mind::commands::mind_turn_on,
            mind::commands::mind_turn_off,
            mind::commands::mind_chosen,
            backup::commands::backup_status,
            backup::commands::backup_new_phrase,
            backup::commands::backup_turn_on,
            backup::commands::backup_recovery_key,
            backup::commands::backup_keep_recovery_key,
            backup::commands::backup_drop_recovery_key,
            backup::commands::backup_change_phrase,
            backup::commands::backup_set_wifi_only,
            backup::commands::backup_now,
            backup::commands::backup_turn_off,
            backup::commands::backup_turn_on_again,
            backup::commands::backup_nights,
            backup::commands::backup_here,
            backup::commands::backup_on_wifi,
            backup::commands::backup_found,
            backup::commands::backup_restore,
            backup::commands::backup_make_in_use,
            backup::place_account::backup_connect,
            backup::screens::backup_save_key,
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
