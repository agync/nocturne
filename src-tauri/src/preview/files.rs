//! Originals and spoken pages, shown to the browser by id or name and never by path, and the
//! rewriting that keeps the machine's paths out of what the browser reads.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::Preview;

/// A kept thing's id is letters, digits, dashes and underscores.
pub fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A file name in the spoken pages folder: no separators, nothing hidden, no climbing.
pub fn audio_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// The original of an item that exists, if it lies inside the originals folder.
pub async fn original_of(preview: &Preview, id: String) -> Option<PathBuf> {
    if !id_ok(&id) {
        return None;
    }
    let engine = preview.engine()?;
    let originals = preview.data_dir.join("store").join("originals");
    tokio::task::spawn_blocking(move || {
        let item = engine.get_item(&id).ok()?;
        inside(Path::new(&item.original_path?), &originals)
    })
    .await
    .ok()
    .flatten()
}

/// A spoken page in the audio folder.
pub async fn audio_of(preview: &Preview, name: String) -> Option<PathBuf> {
    if !audio_name_ok(&name) {
        return None;
    }
    let dir = preview.data_dir.join("store").join("audio");
    tokio::task::spawn_blocking(move || inside(&dir.join(name), &dir))
        .await
        .ok()
        .flatten()
}

/// `path` as a real file inside `root`, or nothing.
fn inside(path: &Path, root: &Path) -> Option<PathBuf> {
    let real = std::fs::canonicalize(path).ok()?;
    let root = std::fs::canonicalize(root).ok()?;
    (real.starts_with(root) && real.is_file()).then_some(real)
}

/// The places whose names never reach the browser: the data folder, then the home folder.
pub fn hidden_roots(data_dir: &Path) -> Vec<String> {
    let mut roots = vec![data_dir.to_string_lossy().into_owned()];
    if let Some(home) = std::env::var_os("HOME").filter(|h| h.len() > 1) {
        roots.push(home.to_string_lossy().trim_end_matches('/').to_owned());
    }
    roots.retain(|r| r.len() > 1);
    roots
}

/// Replace what names a place on this machine with a link the server will honour: an item's
/// original with `/originals/<id>`, a spoken page with `/audio/<name>`. Any other text that
/// holds a folder in `hidden` has that folder replaced by a plain word.
pub fn hide_paths(v: &mut Value, hidden: &[String]) {
    match v {
        Value::String(text) => {
            for (i, root) in hidden.iter().enumerate() {
                if text.contains(root.as_str()) {
                    *text = text.replace(root.as_str(), if i == 0 { "(data)" } else { "~" });
                }
            }
        }
        Value::Object(map) => {
            let id = map.get("id").and_then(Value::as_str).map(str::to_owned);
            if let (Some(id), Some(Value::String(path))) = (id, map.get_mut("originalPath")) {
                *path = format!("/originals/{id}");
            }
            let spoken = map.contains_key("sections") && map.contains_key("durationMs");
            if spoken && let Some(Value::String(path)) = map.get_mut("path") {
                let name = Path::new(path.as_str())
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();
                *path = format!("/audio/{name}");
            }
            for child in map.values_mut() {
                hide_paths(child, hidden);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|i| hide_paths(i, hidden)),
        _ => {}
    }
}

/// [`hide_paths`] over the text of an event's payload.
pub fn hide_paths_in_json(text: &str, hidden: &[String]) -> Option<String> {
    let mut v: Value = serde_json::from_str(text).ok()?;
    hide_paths(&mut v, hidden);
    serde_json::to_string(&v).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ids_and_names_that_could_climb_are_refused() {
        assert!(id_ok("abc-123_X"));
        for bad in ["", "../x", "a/b", "a.b", "a b", &"a".repeat(65)] {
            assert!(!id_ok(bad), "{bad}");
        }
        assert!(audio_name_ok("page-2026-11-13.wav"));
        for bad in ["", ".hidden", "../x", "a/b", "a\\b", "a b"] {
            assert!(!audio_name_ok(bad), "{bad}");
        }
    }

    #[test]
    fn paths_become_links_deep_inside_a_value() {
        let mut v = json!({
            "items": [{ "id": "i1", "originalPath": "/data/store/originals/aa.png" },
                      { "id": "i2", "originalPath": null }],
            "spoken": { "path": "/data/store/audio/p.wav", "sections": [], "durationMs": 1 },
            "other": { "path": "/keep/me" }
        });
        hide_paths(&mut v, &[]);
        assert_eq!(v["items"][0]["originalPath"], "/originals/i1");
        assert_eq!(v["items"][1]["originalPath"], Value::Null);
        assert_eq!(v["spoken"]["path"], "/audio/p.wav");
        assert_eq!(v["other"]["path"], "/keep/me");
    }

    #[test]
    fn the_data_and_home_folders_are_named_nowhere_in_what_the_browser_reads() {
        let hidden = vec!["/srv/x/data".to_string(), "/srv/y/home".to_string()];
        let mut v = json!({
            "detail": "cannot open /srv/x/data/store/a.db: denied",
            "settings": { "inbox": "/srv/y/home/Inbox", "list": ["/srv/x/data/logs"] },
            "other": "no path here"
        });
        hide_paths(&mut v, &hidden);
        let text = v.to_string();
        assert!(
            !text.contains("/srv/x") && !text.contains("/srv/y/home"),
            "{text}"
        );
        assert_eq!(v["detail"], "cannot open (data)/store/a.db: denied");
        assert_eq!(v["settings"]["inbox"], "~/Inbox");
        assert_eq!(v["other"], "no path here");
    }

    #[test]
    fn a_path_outside_the_root_is_not_served() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("in.txt"), b"x").unwrap();
        std::fs::write(dir.path().join("out.txt"), b"x").unwrap();
        assert!(inside(&root.join("in.txt"), &root).is_some());
        assert!(inside(&dir.path().join("out.txt"), &root).is_none());
        assert!(inside(&root.join("../out.txt"), &root).is_none());
    }
}
