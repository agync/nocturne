//! `config.json` for the private engine. Nocturne sets the keys it cares about and leaves
//! every other key exactly as it found it.

use std::path::Path;

use nocturne_contract::Tier;
use serde_json::{Map, Value, json};

use crate::error::RuntimeError;

/// llama.cpp flags for the tiers made for computers with little memory. The engine's defaults
/// keep up to 8 GiB of old prompts and 32 snapshots of a conversation for reuse; Nocturne's
/// prompts change with every question, so that memory only grows. A few snapshots and a small
/// prompt store keep the reuse that matters (the shared start of each prompt) at a fraction of
/// the cost. The larger tiers keep the engine's defaults, which were not measured with these.
pub const LLAMACPP_ARGS: &str = "--cache-ram 256 --ctx-checkpoints 4";

/// The llama.cpp flags for `tier`, empty where the engine's defaults stand.
pub fn llamacpp_args(tier: Tier) -> &'static str {
    match tier {
        Tier::Tiny | Tier::Small => LLAMACPP_ARGS,
        Tier::Large | Tier::Standard | Tier::Unsupported => "",
    }
}

/// The engine's `allowed_origins`. Empty means "use the defaults": same-origin pages on the
/// machine's own addresses are allowed. Any non-empty list is authoritative and drops that
/// default, so this one names an origin under the reserved `.invalid` top level (RFC 2606),
/// which no page can have. The engine still lets loopback and desktop-app origins through
/// whatever is set (`is_origin_allowed`, layer 1, in lemonade v2026.46.2); the key is what
/// stops those. Never `null` (it admits sandboxed frames) and never `*`.
pub const ALLOWED_ORIGINS: &str = "https://none.invalid";

/// The one provider entry for Online answers. The key is never part of it: it is handed to the
/// running engine (see [`crate::online`]).
#[derive(Debug, Clone, Copy)]
pub struct CloudEntry<'a> {
    pub base_url: &'a str,
    /// Plain HTTP; written only for an `http://` address on this computer or the person's own
    /// network.
    pub allow_insecure_http: bool,
}

/// What Nocturne decides at each launch.
#[derive(Debug, Clone)]
pub struct Settings<'a> {
    pub models_dir: &'a Path,
    /// True except while the person has approved a download.
    pub offline: bool,
    /// Stops the engine fetching backend executables; on except during setup.
    pub no_fetch_executables: bool,
    pub websocket_port: u16,
    pub llamacpp_backend: Option<&'a str>,
    pub whispercpp_backend: Option<&'a str>,
    /// Flags for every llama.cpp server; empty for none (see [`llamacpp_args`]).
    pub llamacpp_args: &'a str,
    /// The provider for Online answers; `None` (off) writes an empty list.
    pub cloud: Option<CloudEntry<'a>>,
    /// Models of each kind the engine may hold at once: one, or two while the NPU reads items
    /// beside the mind (the engine counts NPU and GPU chat models in one pool).
    pub models_per_kind: u32,
}

/// Merges `settings` into `<config_dir>/config.json`, creating it when absent.
pub fn write(config_dir: &Path, settings: &Settings<'_>) -> Result<(), RuntimeError> {
    std::fs::create_dir_all(config_dir)?;
    let path = config_dir.join("config.json");
    let mut root = match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(Value::Object(map)) => map,
            _ => Map::new(),
        },
        Err(_) => Map::new(),
    };
    let models_dir = settings.models_dir.to_string_lossy().into_owned();
    let fixed = [
        ("host", json!("127.0.0.1")),
        ("broadcast", json!(false)),
        ("allowed_origins", json!(ALLOWED_ORIGINS)),
        ("models_dir", json!(models_dir)),
        ("offline", json!(settings.offline)),
        ("no_fetch_executables", json!(settings.no_fetch_executables)),
        ("auto_check_model_updates", json!(false)),
        ("auto_update_models", json!(false)),
        ("inhibit_suspend", json!(false)),
        ("auto_evict", json!(true)),
        ("max_loaded_models", json!(settings.models_per_kind)),
        // A model loaded with no saved context would otherwise be sized to fill the free memory.
        ("ctx_size", json!(8192)),
        ("log_level", json!("info")),
        ("websocket_port", json!(settings.websocket_port)),
    ];
    for (key, value) in fixed {
        root.insert(key.to_string(), value);
    }
    // Always written, so a provider left by an earlier run never outlives a launch.
    let providers = match settings.cloud {
        Some(c) => json!([{
            "name": crate::online::PROVIDER,
            "base_url": c.base_url,
            "allow_insecure_http": c.allow_insecure_http
                && crate::online::plain_http_allowed(c.base_url),
            "wire_format": "openai",
        }]),
        None => json!([]),
    };
    root.insert("cloud_providers".to_string(), providers);
    set_nested(&mut root, "telemetry", "enabled", json!(false));
    set_nested(&mut root, "llamacpp", "args", json!(settings.llamacpp_args));
    if let Some(b) = settings.llamacpp_backend {
        set_nested(&mut root, "llamacpp", "backend", json!(b));
    }
    if let Some(b) = settings.whispercpp_backend {
        set_nested(&mut root, "whispercpp", "backend", json!(b));
    }
    let tmp = config_dir.join("config.json.tmp");
    let text = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|e| RuntimeError::Io(e.to_string()))?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Saved load options the engine applies whenever it loads a model by itself, merged into
/// `<config_dir>/recipe_options.json` under each built-in model's key in one write. Other
/// models' entries and other options are kept.
pub fn write_model_options(
    config_dir: &Path,
    models: &[(&str, Map<String, Value>)],
) -> Result<(), RuntimeError> {
    std::fs::create_dir_all(config_dir)?;
    let path = config_dir.join("recipe_options.json");
    let mut root = match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(Value::Object(map)) => map,
            _ => Map::new(),
        },
        Err(_) => Map::new(),
    };
    for (model, options) in models {
        for (key, value) in options {
            set_nested(&mut root, &format!("builtin.{model}"), key, value.clone());
        }
    }
    let tmp = config_dir.join("recipe_options.json.tmp");
    let text = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|e| RuntimeError::Io(e.to_string()))?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

fn set_nested(root: &mut Map<String, Value>, section: &str, key: &str, value: Value) {
    let entry = root
        .entry(section.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    if let Some(map) = entry.as_object_mut() {
        map.insert(key.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(dir: &Path) -> Value {
        serde_json::from_slice(&std::fs::read(dir.join("config.json")).unwrap()).unwrap()
    }

    fn settings(models: &Path) -> Settings<'_> {
        Settings {
            models_dir: models,
            offline: true,
            no_fetch_executables: true,
            websocket_port: 41234,
            llamacpp_backend: None,
            whispercpp_backend: None,
            llamacpp_args: "",
            cloud: None,
            models_per_kind: 1,
        }
    }

    #[test]
    fn allowed_origins_is_never_empty_or_open() {
        assert!(!ALLOWED_ORIGINS.is_empty());
        assert!(ALLOWED_ORIGINS.ends_with(".invalid"));
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("config.json"),
            r#"{"allowed_origins": "*"}"#,
        )
        .unwrap();
        write(tmp.path(), &settings(&tmp.path().join("models"))).unwrap();
        assert_eq!(read(tmp.path())["allowed_origins"], ALLOWED_ORIGINS);
    }

    #[test]
    fn creates_the_file_with_our_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let models = tmp.path().join("models");
        write(tmp.path(), &settings(&models)).unwrap();
        let v = read(tmp.path());
        assert_eq!(v["host"], "127.0.0.1");
        assert_eq!(v["broadcast"], false);
        assert_eq!(v["allowed_origins"], ALLOWED_ORIGINS);
        assert_eq!(v["offline"], true);
        assert_eq!(v["no_fetch_executables"], true);
        assert_eq!(v["websocket_port"], 41234);
        assert_eq!(v["telemetry"]["enabled"], false);
        assert_eq!(v["auto_update_models"], false);
        assert_eq!(v["models_dir"], models.to_string_lossy().as_ref());
        assert_eq!(v["llamacpp"]["args"], "");
        assert_eq!(v["max_loaded_models"], 1);
        assert_eq!(v["ctx_size"], 8192);
        let two = Settings {
            models_per_kind: 2,
            ..settings(&models)
        };
        write(tmp.path(), &two).unwrap();
        assert_eq!(read(tmp.path())["max_loaded_models"], 2);
    }

    #[test]
    fn merges_and_keeps_unknown_keys() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("config.json"),
            r#"{"offline": false, "future_key": {"a": 1}, "telemetry": {"hide_inputs": true, "enabled": true}, "llamacpp": {"args": "-x"}}"#,
        )
        .unwrap();
        let models = tmp.path().join("m");
        let mut s = settings(&models);
        s.llamacpp_backend = Some("vulkan");
        s.llamacpp_args = "--cache-ram 256";
        write(tmp.path(), &s).unwrap();
        let v = read(tmp.path());
        assert_eq!(v["future_key"]["a"], 1);
        assert_eq!(v["offline"], true);
        assert_eq!(v["telemetry"]["hide_inputs"], true);
        assert_eq!(v["telemetry"]["enabled"], false);
        assert_eq!(v["llamacpp"]["args"], "--cache-ram 256");
        assert_eq!(v["llamacpp"]["backend"], "vulkan");
    }

    #[test]
    fn model_options_merge_into_the_saved_ones() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("recipe_options.json"),
            r#"{"builtin.Other": {"ctx_size": 8192}, "builtin.Recall": {"llamacpp_args": "-x", "ctx_size": 32768}}"#,
        )
        .unwrap();
        let mut options = Map::new();
        options.insert("ctx_size".into(), json!(4096));
        let mut voice = Map::new();
        voice.insert("evict_idle_timeout".into(), json!(60));
        write_model_options(tmp.path(), &[("Recall", options), ("Voice", voice)]).unwrap();
        let v: Value =
            serde_json::from_slice(&std::fs::read(tmp.path().join("recipe_options.json")).unwrap())
                .unwrap();
        assert_eq!(v["builtin.Recall"]["ctx_size"], 4096);
        assert_eq!(v["builtin.Recall"]["llamacpp_args"], "-x");
        assert_eq!(v["builtin.Other"]["ctx_size"], 8192);
        assert_eq!(v["builtin.Voice"]["evict_idle_timeout"], 60);
    }

    #[test]
    fn only_the_small_tiers_cap_the_caches() {
        assert_eq!(llamacpp_args(Tier::Tiny), LLAMACPP_ARGS);
        assert_eq!(llamacpp_args(Tier::Small), LLAMACPP_ARGS);
        assert_eq!(llamacpp_args(Tier::Standard), "");
        assert_eq!(llamacpp_args(Tier::Large), "");
    }

    #[test]
    fn cloud_providers_empty_when_off() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), &settings(&tmp.path().join("m"))).unwrap();
        assert_eq!(read(tmp.path())["cloud_providers"], json!([]));
    }

    #[test]
    fn a_stale_provider_is_removed_at_launch() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("config.json"),
            r#"{"cloud_providers": [
                {"name": "nocturne_online", "base_url": "https://a.example/v1", "api_key": "sk-stale-1"},
                {"name": "other", "base_url": "http://10.0.0.2/v1", "allow_insecure_http": true}
            ], "offline": false}"#,
        )
        .unwrap();
        write(tmp.path(), &settings(&tmp.path().join("m"))).unwrap();
        assert_eq!(read(tmp.path())["cloud_providers"], json!([]));
        let text = std::fs::read_to_string(tmp.path().join("config.json")).unwrap();
        assert!(!text.contains("api_key") && !text.contains("sk-stale-1"));
    }

    #[test]
    fn one_entry_when_on_and_never_a_key() {
        let tmp = tempfile::tempdir().unwrap();
        let models = tmp.path().join("m");
        let mut s = settings(&models);
        s.cloud = Some(CloudEntry {
            base_url: "https://api.example.com/v1",
            allow_insecure_http: false,
        });
        write(tmp.path(), &s).unwrap();
        let v = read(tmp.path());
        assert_eq!(
            v["cloud_providers"],
            json!([{
                "name": "nocturne_online",
                "base_url": "https://api.example.com/v1",
                "allow_insecure_http": false,
                "wire_format": "openai",
            }])
        );
        assert_eq!(v["offline"], true, "offline stays on with a provider");
        let text = std::fs::read_to_string(tmp.path().join("config.json")).unwrap();
        assert!(!text.contains("api_key") && !text.contains("auth_header"));
    }

    #[test]
    fn plain_http_only_for_this_computer_and_the_own_network() {
        let tmp = tempfile::tempdir().unwrap();
        let models = tmp.path().join("m");
        for (base_url, want) in [
            ("http://127.0.0.1:8080/v1", true),
            ("http://192.168.1.10:8000/v1", true),
            ("http://box.tail1234.ts.net/v1", true),
            ("http://api.example.com/v1", false),
            ("http://169.254.169.254/v1", false),
            ("https://192.168.1.10/v1", false),
        ] {
            let mut s = settings(&models);
            s.cloud = Some(CloudEntry {
                base_url,
                allow_insecure_http: true,
            });
            write(tmp.path(), &s).unwrap();
            assert_eq!(
                read(tmp.path())["cloud_providers"][0]["allow_insecure_http"],
                want,
                "{base_url}"
            );
        }
    }

    #[test]
    fn log_level_is_info() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("config.json"), r#"{"log_level": "debug"}"#).unwrap();
        write(tmp.path(), &settings(&tmp.path().join("m"))).unwrap();
        assert_eq!(read(tmp.path())["log_level"], "info");
    }

    #[test]
    fn replaces_a_broken_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("config.json"), "not json").unwrap();
        let models = tmp.path().join("m");
        write(tmp.path(), &settings(&models)).unwrap();
        assert_eq!(read(tmp.path())["host"], "127.0.0.1");
    }
}
