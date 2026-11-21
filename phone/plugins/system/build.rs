const COMMANDS: &[&str] = &[
    "insets",
    "device_name",
    "open_app_settings",
    "take_launch",
    "take_shared",
    "shared_file",
    "shared_done",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
