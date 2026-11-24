//! What the backup's screens need beyond the backup itself: "Print or save…" for a recovery key,
//! through the phone's own save sheet, and "Copy" for a phrase. The key is written to a private
//! folder of the app's cache only while the sheet is open, then removed.

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_nocturne_system::{Sheet, SystemExt};

use crate::sync::ledger::write_private;

/// The cache folder the key passes through.
const OUTBOX: &str = "backup-key";
/// The name the save sheet offers.
const NAME: &str = "nocturne-recovery-key.txt";

/// A copied phrase leaves the clipboard after this long.
const COPY_CLEARS: std::time::Duration = std::time::Duration::from_secs(60);

/// "Copy": the phrase on the phone's clipboard for a password manager, kept out of its paste
/// previews and cleared after a minute. `false` where the phone has no such clipboard, so the
/// screen copies it the web's way.
#[tauri::command]
pub fn backup_copy<R: Runtime>(app: AppHandle<R>, text: String) -> bool {
    app.system()
        .copy_secret(&text, COPY_CLEARS)
        .inspect_err(|e| tracing::warn!("the phrase could not be copied: {e:?}"))
        .unwrap_or(false)
}

/// What came of the save sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum KeySaved {
    Saved,
    Cancelled,
    Failed,
}

fn saved(sheet: Sheet) -> KeySaved {
    match sheet {
        Sheet::Saved | Sheet::Chosen(_) => KeySaved::Saved,
        Sheet::Cancelled => KeySaved::Cancelled,
        Sheet::Full | Sheet::Failed => KeySaved::Failed,
    }
}

/// "Print or save…": the phone's save sheet for the recovery key just shown.
#[tauri::command]
pub async fn backup_save_key<R: Runtime>(app: AppHandle<R>, key: String) -> KeySaved {
    let Ok(dir) = app.path().app_cache_dir().map(|d| d.join(OUTBOX)) else {
        return KeySaved::Failed;
    };
    tauri::async_runtime::spawn_blocking(move || {
        crate::home::calls::clear(&dir);
        if std::fs::create_dir_all(&dir).is_err() {
            return KeySaved::Failed;
        }
        let path = dir.join(NAME);
        let result = match write_private(&path, format!("{key}\n").as_bytes()) {
            Ok(()) => app
                .system()
                .save_file(&path, NAME)
                .map_or(KeySaved::Failed, saved),
            Err(e) => {
                tracing::warn!("the recovery key could not be readied for the sheet: {e}");
                KeySaved::Failed
            }
        };
        crate::home::calls::clear(&dir);
        result
    })
    .await
    .unwrap_or(KeySaved::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sheet_outcome_says_whether_the_key_was_saved() {
        assert_eq!(saved(Sheet::Saved), KeySaved::Saved);
        assert_eq!(saved(Sheet::Cancelled), KeySaved::Cancelled);
        assert_eq!(saved(Sheet::Full), KeySaved::Failed);
        assert_eq!(saved(Sheet::Failed), KeySaved::Failed);
    }
}
