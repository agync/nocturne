//! Build script: the Tauri build step, plus a fallback for a missing interface build.
//!
//! The configuration points `frontendDist` at `../ui/dist`. Tauri embeds that folder only when
//! the `custom-protocol` feature is on (which `cargo tauri build` turns on); in a plain
//! `cargo build` or `cargo test` the app is in development mode and loads `devUrl`, so no
//! folder is needed. If a build with `custom-protocol` finds `ui/dist` absent (a Rust-only
//! checkout), it embeds `frontend-fallback/` instead and warns, so the crate still compiles.
//! A packaged build must run `npm --prefix ui run build` first.
//!
//! On Windows, Tauri links its application manifest into the programs only. The test programs
//! get the same manifest here: without its Common Controls 6 dependency they stop at start-up
//! with STATUS_ENTRYPOINT_NOT_FOUND.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../ui/dist");
    println!("cargo:rerun-if-changed=frontend-fallback");
    println!("cargo:rerun-if-env-changed=TAURI_CONFIG");

    let embeds = std::env::var_os("CARGO_FEATURE_CUSTOM_PROTOCOL").is_some();
    if embeds && !Path::new("../ui/dist/index.html").exists() {
        println!(
            "cargo:warning=ui/dist is missing: embedding the placeholder page. Build the interface first (npm --prefix ui run build) for a real package."
        );
        // Merge with any config the Tauri CLI passed in, then point the embedded assets at the
        // fallback folder. rustc sees this when `generate_context!` expands.
        let mut config: serde_json::Value = std::env::var("TAURI_CONFIG")
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        config["build"]["frontendDist"] = serde_json::json!("frontend-fallback");
        println!("cargo:rustc-env=TAURI_CONFIG={config}");
    }

    tauri_build::build();

    let target = |key: &str| std::env::var(key).unwrap_or_default();
    if target("CARGO_CFG_TARGET_OS") == "windows" && target("CARGO_CFG_TARGET_ENV") == "msvc" {
        let manifest = Path::new(&target("CARGO_MANIFEST_DIR")).join("windows/test-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}
