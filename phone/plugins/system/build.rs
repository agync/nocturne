const COMMANDS: &[&str] = &["insets", "device_name", "open_app_settings"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
