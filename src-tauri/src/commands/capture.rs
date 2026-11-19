//! Capture, items and questions. Capture is acknowledged as soon as the engine has written the
//! item; nothing here waits on the mind.

use std::path::PathBuf;

use nocturne_contract::{EVENT_ITEM, ItemFilter, ItemView, Kept, Source};
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{change, engine, read, run};
use crate::ask;
use crate::error::{CommandError, blocking, blocking_cmd};
use crate::state::{AppState, refresh_status_soon};

/// The most a single piece of raw content may weigh on its way in.
pub const MAX_RAW_BYTES: usize = 256 * 1024 * 1024;
/// The largest file kept by path.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// The id of the notice shown when files were offered and none could be kept.
pub const NOT_KEPT_NOTICE: &str = "not-kept";

/// Tell the interface about a new item, wake the pipeline and refresh the status. Used by every
/// way an item can arrive. The recount runs after this returns: it reads the whole store, can
/// queue behind an import's own recount, and a capture is acknowledged without waiting for it.
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
    let state = app.state::<AppState>();
    if !kept.is_empty() {
        state.status.clear_notice(NOT_KEPT_NOTICE);
    }
    if any_new {
        state.wake_pipeline();
    }
    refresh_status_soon(app);
}

/// Remember what the quick capture window kept: it may read and take back those items and no
/// others (it has no business with the rest of what the person has kept).
pub(super) fn note_quick<R: Runtime>(
    app: &AppHandle<R>,
    webview: &tauri::Webview<R>,
    kept: &[Kept],
) {
    if webview.label() != crate::os::quick_capture::LABEL {
        return;
    }
    let state = app.state::<AppState>();
    // Only what it made: a duplicate is an older item the window did not create.
    for k in kept {
        if let Kept::New { item } = k {
            state.note_quick_item(&item.id);
        }
    }
}

/// Whether the window may touch this item: every window but quick capture may; quick capture
/// only items it kept itself.
fn may_touch<R: Runtime>(app: &AppHandle<R>, webview: &tauri::Webview<R>, id: &str) -> bool {
    webview.label() != crate::os::quick_capture::LABEL || app.state::<AppState>().quick_item(id)
}

/// Only a regular file of a sensible size is kept by path: no folders, devices or pipes.
fn check_file(path: &std::path::Path) -> Result<(), CommandError> {
    let meta = std::fs::metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            CommandError::not_found("that file is not there")
        } else {
            CommandError::invalid("that file cannot be read")
        }
    })?;
    if !meta.is_file() {
        return Err(CommandError::invalid("only files can be kept"));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(CommandError::invalid("that file is too large"));
    }
    Ok(())
}

/// Keep each of `items` and say how each went. `resolve` turns an item into the path to keep and
/// runs right before that item's own keep, so a check it makes is as fresh as the keep.
pub async fn keep_each<R, T>(
    app: &AppHandle<R>,
    items: Vec<T>,
    source: Source,
    resolve: impl Fn(&T) -> Result<PathBuf, CommandError> + Send + 'static,
) -> Result<Vec<Result<Kept, CommandError>>, CommandError>
where
    R: Runtime,
    T: Send + 'static,
{
    let e = engine(app)?;
    let results = blocking_cmd(move || {
        Ok(items
            .iter()
            .map(|item| {
                let path = resolve(item)?;
                check_file(&path)?;
                e.keep_file(&path, source).map_err(CommandError::from)
            })
            .collect::<Vec<_>>())
    })
    .await?;
    let new: Vec<Kept> = results.iter().flatten().cloned().collect();
    if !new.is_empty() {
        announce(app, &new).await;
    }
    Ok(results)
}

/// Keep files by path (a drop, a second launch, the tray). Files that cannot be kept are logged
/// and skipped; if none could be kept the first error is returned.
pub async fn keep_paths<R: Runtime>(
    app: &AppHandle<R>,
    paths: Vec<PathBuf>,
    source: Source,
) -> Result<Vec<Kept>, CommandError> {
    let results = keep_each(app, paths, source, |p| Ok(p.clone())).await?;
    let mut kept = Vec::new();
    let mut first_error = None;
    for r in results {
        match r {
            Ok(k) => kept.push(k),
            Err(err) => {
                tracing::warn!("a file could not be kept: {:?}", err.code);
                first_error.get_or_insert(err);
            }
        }
    }
    if kept.is_empty()
        && let Some(err) = first_error
    {
        return Err(err);
    }
    Ok(kept)
}

#[tauri::command]
pub async fn keep_text<R: Runtime>(
    app: AppHandle<R>,
    webview: tauri::Webview<R>,
    text: String,
    source: Source,
) -> Result<Kept, CommandError> {
    run(app, move |app| async move {
        let e = engine(&app)?;
        let kept = blocking(move || e.keep_text(&text, source)).await?;
        note_quick(&app, &webview, std::slice::from_ref(&kept));
        announce(&app, std::slice::from_ref(&kept)).await;
        Ok(kept)
    })
    .await
}

#[tauri::command]
pub async fn keep_files<R: Runtime>(
    app: AppHandle<R>,
    webview: tauri::Webview<R>,
    paths: Vec<String>,
    source: Source,
) -> Result<Vec<Kept>, CommandError> {
    run(app, move |app| async move {
        if paths.is_empty() {
            return Err(CommandError::invalid("there is nothing to keep"));
        }
        let state = app.state::<AppState>();
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        // Only files the person dropped or picked in this session are kept by path.
        if let Some(p) = paths.iter().find(|p| !state.path_allowed(p)) {
            tracing::warn!("a path that was not chosen was offered");
            let _ = p;
            return Err(CommandError::invalid("that file was not chosen"));
        }
        let kept = keep_paths(&app, paths, source).await?;
        note_quick(&app, &webview, &kept);
        Ok(kept)
    })
    .await
}

/// The files dropped on the quick capture window since it last asked. The window cannot hear
/// the drop itself, so it asks; what it gets may then be kept with `keep_files`.
#[tauri::command]
pub async fn take_quick_drop<R: Runtime>(app: AppHandle<R>) -> Result<Vec<String>, CommandError> {
    run(app, |app| async move {
        Ok(app
            .state::<AppState>()
            .take_quick_drop()
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect())
    })
    .await
}

/// The file extension for a mime type (parameters such as `;codecs=opus` ignored).
pub(crate) fn extension_for(mime: &str) -> Option<&'static str> {
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

/// Keep the raw body of a request: the mime type rides in `x-nocturne-mime`, and (when the
/// caller does not fix it) the source in `x-nocturne-source`.
async fn keep_raw<R: Runtime>(
    app: AppHandle<R>,
    webview: tauri::Webview<R>,
    request: Request<'_>,
    fixed_source: Option<Source>,
) -> Result<Kept, CommandError> {
    let bytes = match request.body() {
        InvokeBody::Raw(b) if b.len() > MAX_RAW_BYTES => {
            return Err(CommandError::invalid("this is too large to keep"));
        }
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
        // The speech model reads WAV and rejects what web views record natively (WebM, MP4). The
        // interface sends WAV; anything else is refused here, plainly, rather than kept to fail
        // later as an item that could not be read.
        if fixed_source == Some(Source::Recording) && ext != "wav" {
            return Err(CommandError::invalid(
                "a recording must be a WAV file; this kind of recording cannot be read",
            ));
        }
        if bytes.is_empty() {
            return Err(CommandError::invalid("there is nothing to keep"));
        }
        let e = engine(&app)?;
        let kept = blocking(move || e.keep_bytes(&bytes, ext, source)).await?;
        note_quick(&app, &webview, std::slice::from_ref(&kept));
        announce(&app, std::slice::from_ref(&kept)).await;
        Ok(kept)
    })
    .await
}

/// A recording, as the raw request body with its mime type in the `x-nocturne-mime` header.
#[tauri::command]
pub async fn keep_recording<R: Runtime>(
    app: AppHandle<R>,
    webview: tauri::Webview<R>,
    request: Request<'_>,
) -> Result<Kept, CommandError> {
    keep_raw(app, webview, request, Some(Source::Recording)).await
}

/// Any file's bytes (a pasted or captured image, a dropped file's content), with headers
/// `x-nocturne-mime` and `x-nocturne-source`.
#[tauri::command]
pub async fn keep_bytes<R: Runtime>(
    app: AppHandle<R>,
    webview: tauri::Webview<R>,
    request: Request<'_>,
) -> Result<Kept, CommandError> {
    keep_raw(app, webview, request, None).await
}

#[tauri::command]
pub async fn list_items<R: Runtime>(
    app: AppHandle<R>,
    filter: ItemFilter,
) -> Result<Vec<ItemView>, CommandError> {
    read(app, move |e| e.items(filter)).await
}

#[tauri::command]
pub async fn get_item<R: Runtime>(
    app: AppHandle<R>,
    webview: tauri::Webview<R>,
    id: String,
) -> Result<ItemView, CommandError> {
    if !may_touch(&app, &webview, &id) {
        return Err(CommandError::not_found("that item"));
    }
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

/// Take back an item kept in the last few minutes, with everything read from it. Too late
/// answers `tooLate`. Answers in flight are stopped: they may have read what is now gone.
#[tauri::command]
pub async fn unkeep<R: Runtime>(
    app: AppHandle<R>,
    webview: tauri::Webview<R>,
    id: String,
) -> Result<(), CommandError> {
    if !may_touch(&app, &webview, &id) {
        return Err(CommandError::not_found("that item"));
    }
    app.state::<AppState>().asks.cancel_all();
    change(app, move |e| e.unkeep(&id).map(|_| ())).await
}

/// Read an item next: it moves to the front of the queue and the pipeline is woken.
#[tauri::command]
pub async fn read_now<R: Runtime>(app: AppHandle<R>, id: String) -> Result<ItemView, CommandError> {
    let reading = id.clone();
    let item = change(app.clone(), move |e| {
        e.prioritize(&id)?;
        e.get_item(&id)
    })
    .await?;
    let state = app.state::<AppState>();
    match state.services() {
        Some(services) => services.reader.read_now(Some(&reading)).await,
        None => state.wake_pipeline(),
    }
    Ok(item)
}

/// Start answering a question. Returns the ask id; the answer streams as `ask-delta` events.
/// `place` is the person's choice for this one question; without it, `auto`. `help` asks for the
/// guide's answer ("Show the guide's answer").
#[tauri::command]
pub async fn ask<R: Runtime>(
    app: AppHandle<R>,
    question: String,
    place: Option<nocturne_contract::AskPlace>,
    help: Option<bool>,
) -> Result<String, CommandError> {
    run(app, move |app| async move {
        let question = question.trim().to_string();
        if question.is_empty() {
            return Err(CommandError::invalid("there is no question"));
        }
        Ok(ask::start_placed(
            &app,
            question,
            place.unwrap_or_default(),
            help.unwrap_or(false),
        ))
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
