//! `config.json` for the private engine. Nocturne sets the keys it cares about and leaves
//! every other key exactly as it found it.

use std::path::Path;

use serde_json::{Map, Value, json};

use crate::error::RuntimeError;

/// Flags for every llama.cpp server the engine starts. Its defaults keep up to 8 GiB of old
/// prompts and 32 snapshots of a conversation in memory for reuse; Nocturne's prompts change
/// with every question, so that memory only grows. A few snapshots and a small prompt store keep
/// the reuse that matters (the shared start of each prompt) at a fraction of the cost.
pub const LLAMACPP_ARGS: &str = "--cache-ram 256 --ctx-checkpoints 4";

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
        ("models_dir", json!(models_dir)),
        ("offline", json!(settings.offline)),
        ("no_fetch_executables", json!(settings.no_fetch_executables)),
        ("auto_check_model_updates", json!(false)),
        ("auto_update_models", json!(false)),
        ("inhibit_suspend", json!(false)),
        ("auto_evict", json!(true)),
        // One model of each kind at a time.
        ("max_loaded_models", json!(1)),
        // A model loaded with no saved context would otherwise be sized to fill the free memory.
        ("ctx_size", json!(8192)),
        ("log_level", json!("info")),
        ("websocket_port", json!(settings.websocket_port)),
    ];
    for (key, value) in fixed {
        root.insert(key.to_string(), value);
    }
    set_nested(&mut root, "telemetry", "enabled", json!(false));
    set_nested(&mut root, "llamacpp", "args", json!(LLAMACPP_ARGS));
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
        }
    }

    #[test]
    fn creates_the_file_with_our_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let models = tmp.path().join("models");
        write(tmp.path(), &settings(&models)).unwrap();
        let v = read(tmp.path());
        assert_eq!(v["host"], "127.0.0.1");
        assert_eq!(v["broadcast"], false);
        assert_eq!(v["offline"], true);
        assert_eq!(v["no_fetch_executables"], true);
        assert_eq!(v["websocket_port"], 41234);
        assert_eq!(v["telemetry"]["enabled"], false);
        assert_eq!(v["auto_update_models"], false);
        assert_eq!(v["models_dir"], models.to_string_lossy().as_ref());
        assert_eq!(v["llamacpp"]["args"], LLAMACPP_ARGS);
        assert_eq!(v["max_loaded_models"], 1);
        assert_eq!(v["ctx_size"], 8192);
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
        write(tmp.path(), &s).unwrap();
        let v = read(tmp.path());
        assert_eq!(v["future_key"]["a"], 1);
        assert_eq!(v["offline"], true);
        assert_eq!(v["telemetry"]["hide_inputs"], true);
        assert_eq!(v["telemetry"]["enabled"], false);
        assert_eq!(v["llamacpp"]["args"], LLAMACPP_ARGS);
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
    fn replaces_a_broken_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("config.json"), "not json").unwrap();
        let models = tmp.path().join("m");
        write(tmp.path(), &settings(&models)).unwrap();
        assert_eq!(read(tmp.path())["host"], "127.0.0.1");
    }
}
