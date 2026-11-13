//! Capture, items and questions. Capture is acknowledged as soon as the engine has written the
//! item; nothing here waits on the mind.

use std::path::{Path, PathBuf};

use nocturne_contract::{EVENT_ITEM, ItemFilter, ItemView, Kept, Source};
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{change, engine, not_yet, read, run};
use crate::ask;
use crate::error::{CommandError, blocking, blocking_cmd};
use crate::state::{AppState, refresh_status};

/// The most a single piece of raw content may weigh on its way in.
const MAX_RAW_BYTES: usize = 512 * 1024 * 1024;

/// Tell the interface about a new item, wake the pipeline and refresh the status. Used by every
/// way an item can arrive.
pub async fn announce<R: Runtime>(app: &AppHandle<R>, kept: &[Kept]) {
    let mut any_new = false;
    for k in kept {
        if let Kept::New { item } = k {
            any_new = true;
            if let Err(e) = app.emit(EVENT_ITEM, item) {
                tracing::warn!("could not emit an item: {e}");
            }
        }
    }
    if any_new {
        app.state::<AppState>().wake_pipeline();
    }
    refresh_status(app).await;
}

/// Keep files by path (a drop, a second launch, the tray). Files that cannot be kept are logged
/// and skipped; if none could be kept the first error is returned.
pub async fn keep_paths<R: Runtime>(
    app: &AppHandle<R>,
    paths: Vec<PathBuf>,
    source: Source,
) -> Result<Vec<Kept>, CommandError> {
    let e = engine(app)?;
    let results = blocking_cmd(move || {
        Ok(paths
            .iter()
            .map(|p| e.keep_file(p, source))
            .collect::<Vec<_>>())
    })
    .await?;
    let mut kept = Vec::new();
    let mut first_error = None;
    for r in results {
        match r {
            Ok(k) => kept.push(k),
            Err(err) => {
                tracing::warn!("a file could not be kept: {err}");
                first_error.get_or_insert(CommandError::from(err));
            }
        }
    }
    if kept.is_empty()
        && let Some(err) = first_error
    {
        return Err(err);
    }
    announce(app, &kept).await;
    Ok(kept)
}

#[tauri::command]
pub async fn keep_text<R: Runtime>(
    app: AppHandle<R>,
    text: String,
    source: Source,
) -> Result<Kept, CommandError> {
    run(app, move |app| async move {
        let e = engine(&app)?;
        let kept = blocking(move || e.keep_text(&text, source)).await?;
        announce(&app, std::slice::from_ref(&kept)).await;
        Ok(kept)
    })
    .await
}

#[tauri::command]
pub async fn keep_files<R: Runtime>(
    app: AppHandle<R>,
    paths: Vec<String>,
    source: Source,
) -> Result<Vec<Kept>, CommandError> {
    run(app, move |app| async move {
        if paths.is_empty() {
            return Err(CommandError::invalid("there is nothing to keep"));
        }
        keep_paths(&app, paths.into_iter().map(PathBuf::from).collect(), source).await
    })
    .await
}

/// The file extension for a mime type (parameters such as `;codecs=opus` ignored).
fn extension_for(mime: &str) -> Option<&'static str> {
    let essence = mime.split(';').next()?.trim().to_ascii_lowercase();
    Some(match essence.as_str() {
        "audio/webm" => "webm",
        "audio/ogg" | "audio/opus" => "ogg",
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/mp4" | "audio/x-m4a" | "audio/aac" => "m4a",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/flac" | "audio/x-flac" => "flac",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/heic" => "heic",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        "text/markdown" => "md",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        _ => return None,
    })
}

/// Write the bytes to a private temporary folder, keep them as a file, remove the folder.
fn keep_raw_bytes(
    state: &AppState,
    bytes: &[u8],
    ext: &str,
    source: Source,
) -> Result<Kept, nocturne_engine::Error> {
    use std::io::Write;
    let engine = state.engine().map_err(|_| {
        nocturne_engine::Error::Io(std::io::Error::other("the store could not be opened"))
    })?;
    let mut id = [0u8; 8];
    getrandom::fill(&mut id)
        .map_err(|_| nocturne_engine::Error::Io(std::io::Error::other("no random bytes")))?;
    let id: String = id.iter().map(|b| format!("{b:02x}")).collect();
    let dir = state.tmp_dir().join(id);
    create_private_dir(&dir)?;
    let stamp = state
        .now()
        .to_zoned(state.clock.tz())
        .strftime("%Y%m%d-%H%M%S");
    let stem = if source == Source::Recording {
        "recording"
    } else {
        "item"
    };
    let path = dir.join(format!("{stem}-{stamp}.{ext}"));
    let result = (|| {
        let mut f = create_private_file(&path)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        engine.keep_file(&path, source)
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

pub(crate) fn create_private_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(path)
}

fn create_private_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

/// Keep the raw body of a request: the mime type rides in `x-nocturne-mime`, and (when the
/// caller does not fix it) the source in `x-nocturne-source`.
async fn keep_raw<R: Runtime>(
    app: AppHandle<R>,
    request: Request<'_>,
    fixed_source: Option<Source>,
) -> Result<Kept, CommandError> {
    let bytes = match request.body() {
        InvokeBody::Raw(b) => b.clone(),
        InvokeBody::Json(_) => {
            return Err(CommandError::invalid("the content is sent as raw bytes"));
        }
    };
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let mime = header("x-nocturne-mime");
    let source_header = header("x-nocturne-source");
    run(app, move |app| async move {
        let source = match (fixed_source, source_header) {
            (Some(s), _) => s,
            (None, Some(s)) => serde_json::from_value::<Source>(serde_json::Value::String(s))
                .map_err(|_| CommandError::invalid("unknown source"))?,
            (None, None) => return Err(CommandError::invalid("the source is missing")),
        };
        let ext = mime
            .as_deref()
            .and_then(extension_for)
            .ok_or_else(|| CommandError::invalid("this kind of content cannot be kept"))?;
        if fixed_source == Some(Source::Recording)
            && !matches!(
                nocturne_engine::kind_for_ext(ext),
                Some((nocturne_contract::ItemKind::Audio, _))
            )
        {
            return Err(CommandError::invalid(
                "this kind of recording cannot be kept",
            ));
        }
        if bytes.is_empty() {
            return Err(CommandError::invalid("there is nothing to keep"));
        }
        if bytes.len() > MAX_RAW_BYTES {
            return Err(CommandError::invalid("this is too large to keep"));
        }
        let handle = app.clone();
        let kept = blocking_cmd(move || {
            let state = handle.state::<AppState>();
            keep_raw_bytes(&state, &bytes, ext, source).map_err(CommandError::from)
        })
        .await?;
        announce(&app, std::slice::from_ref(&kept)).await;
        Ok(kept)
    })
    .await
}

/// A recording, as the raw request body with its mime type in the `x-nocturne-mime` header.
#[tauri::command]
pub async fn keep_recording<R: Runtime>(
    app: AppHandle<R>,
    request: Request<'_>,
) -> Result<Kept, CommandError> {
    keep_raw(app, request, Some(Source::Recording)).await
}

/// Any file's bytes (a pasted or captured image, a dropped file's content), with headers
/// `x-nocturne-mime` and `x-nocturne-source`.
#[tauri::command]
pub async fn keep_bytes<R: Runtime>(
    app: AppHandle<R>,
    request: Request<'_>,
) -> Result<Kept, CommandError> {
    keep_raw(app, request, None).await
}

#[tauri::command]
pub async fn list_items<R: Runtime>(
    app: AppHandle<R>,
    filter: ItemFilter,
) -> Result<Vec<ItemView>, CommandError> {
    read(app, move |e| e.items(filter)).await
}

#[tauri::command]
pub async fn get_item<R: Runtime>(app: AppHandle<R>, id: String) -> Result<ItemView, CommandError> {
    read(app, move |e| e.get_item(&id)).await
}

#[tauri::command]
pub async fn retry_item<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<ItemView, CommandError> {
    let item = change(app.clone(), move |e| {
        e.retry_item(&id)?;
        e.get_item(&id)
    })
    .await?;
    if let Err(e) = app.emit(EVENT_ITEM, &item) {
        tracing::warn!("could not emit an item: {e}");
    }
    app.state::<AppState>().wake_pipeline();
    Ok(item)
}

/// Removing an item kept moments ago needs `Engine::unkeep` (engine round 2).
#[tauri::command]
pub async fn unkeep<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), CommandError> {
    let _ = (app, id);
    Err(not_yet("undoing a keep"))
}

/// Read an item next. The engine's queue order (`Engine::prioritize`, round 2) is not there
/// yet; the item is checked and the pipeline woken, which is the part that already helps.
#[tauri::command]
pub async fn read_now<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), CommandError> {
    read(app.clone(), move |e| e.get_item(&id).map(|_| ())).await?;
    app.state::<AppState>().wake_pipeline();
    Ok(())
}

/// Start answering a question. Returns the ask id; the answer streams as `ask-delta` events.
#[tauri::command]
pub async fn ask<R: Runtime>(app: AppHandle<R>, question: String) -> Result<String, CommandError> {
    run(app, move |app| async move {
        let question = question.trim().to_string();
        if question.is_empty() {
            return Err(CommandError::invalid("there is no question"));
        }
        Ok(ask::start(&app, question))
    })
    .await
}

/// Stop one answer. Stopping one that has finished is fine.
#[tauri::command]
pub async fn cancel_ask<R: Runtime>(app: AppHandle<R>, ask_id: String) -> Result<(), CommandError> {
    run(app, move |app| async move {
        app.state::<AppState>().asks.cancel(&ask_id);
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_types_map_to_extensions_the_engine_knows() {
        for mime in [
            "audio/webm",
            "audio/webm;codecs=opus",
            "audio/ogg",
            "audio/wav",
            "audio/mp4",
            "audio/mpeg",
            "audio/flac",
        ] {
            let ext = extension_for(mime).unwrap_or_else(|| panic!("{mime}"));
            let (kind, _) = nocturne_engine::kind_for_ext(ext).unwrap();
            assert_eq!(kind, nocturne_contract::ItemKind::Audio, "{mime}");
        }
        assert!(extension_for("video/mp4").is_none());
        assert!(extension_for("").is_none());
        assert_eq!(extension_for("image/png"), Some("png"));
        assert_eq!(extension_for("application/pdf"), Some("pdf"));
    }
}
