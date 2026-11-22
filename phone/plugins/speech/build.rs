// The app's Rust side calls this plugin; the screens never do, so it has no commands of its own.
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
