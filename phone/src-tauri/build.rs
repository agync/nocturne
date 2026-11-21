use std::path::Path;

fn main() {
    // The interface is built into ui/dist/phone-native by `make phone/android`. A checkout without
    // it (the Rust jobs, a fresh clone) still builds and tests this crate: an empty page stands in.
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/dist/phone-native");
    if !dist.join("index.html").exists() {
        std::fs::create_dir_all(&dist).expect("ui/dist/phone-native can be created");
        std::fs::write(dist.join("index.html"), "<!doctype html>\n")
            .expect("a stand-in page can be written");
    }
    tauri_build::build()
}
