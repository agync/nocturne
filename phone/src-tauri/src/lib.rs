//! Nocturne on the phone: the phone screens in a web view, and the sealed channel to the
//! person's computer in Rust. The web view reaches nothing on the network itself.

pub mod commands;
pub mod state;
pub mod store;
pub mod uploads;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_nocturne_system::init());
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
        ])
        .run(tauri::generate_context!())
        .expect("the phone app runs");
}
