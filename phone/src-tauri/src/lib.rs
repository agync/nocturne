//! Nocturne on the phone: the phone screens in a web view, and the sealed channel to the
//! person's computer in Rust. The web view reaches nothing on the network itself.

pub mod commands;
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
        .plugin(tauri_plugin_nocturne_health::init());
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());
    builder
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            app.manage(state::AppState::new(dir));
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
        ])
        .build(tauri::generate_context!())
        .expect("the phone app runs")
        .run(|app, event| {
            // Back to the front: send what changed while away.
            if let tauri::RunEvent::Resumed = event {
                sources::start(app);
            }
        });
}
