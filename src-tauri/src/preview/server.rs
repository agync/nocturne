//! The routes.

use std::convert::Infallible;
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware;
use axum::response::sse::{Event as Sse, KeepAlive};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures_util::Stream;
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

use super::{Preview, TicketKind, auth, files, invoke, transcode};

/// What an interface needs from the page it loads: nothing from elsewhere, and no frames.
const UI_CSP: &str = "default-src 'self'; img-src 'self' blob: data:; media-src 'self' blob:; \
    style-src 'self' 'unsafe-inline'; font-src 'self'; connect-src 'self'; frame-ancestors 'none'";

/// Everything the browser talks to.
pub fn router(preview: Arc<Preview>) -> Router {
    Router::new()
        .route("/api/invoke/{command}", post(invoke_command))
        .route("/api/ticket", post(ticket))
        .route("/api/events", get(events))
        .route("/originals/{id}", get(original))
        .route("/audio/{name}", get(audio))
        .fallback(fallback)
        .layer(DefaultBodyLimit::max(
            crate::commands::MAX_RAW_BYTES + (1 << 20),
        ))
        .layer(middleware::from_fn_with_state(preview.clone(), auth::gate))
        .with_state(preview)
}

fn status_for(code: &str) -> StatusCode {
    match code {
        "notFound" => StatusCode::NOT_FOUND,
        "invalid" => StatusCode::BAD_REQUEST,
        "busy" => StatusCode::CONFLICT,
        "unavailable" => StatusCode::SERVICE_UNAVAILABLE,
        "tokenExpired" | "tooLate" => StatusCode::GONE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn json_response(status: StatusCode, body: Value) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(body),
    )
        .into_response()
}

fn failure(error: Value) -> Response {
    let status = status_for(error["code"].as_str().unwrap_or("internal"));
    json_response(status, error)
}

/// `{ "patch": { "inboxFolder": "/some/path" } }` names a folder on this machine; a browser
/// cannot choose one.
fn names_a_folder(command: &str, args: &Value) -> bool {
    command == "update_settings" && args["patch"]["inboxFolder"].is_string()
}

async fn invoke_command(
    State(preview): State<Arc<Preview>>,
    Path(command): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !crate::COMMAND_NAMES.contains(&command.as_str()) {
        return failure(json!({ "code": "notFound", "detail": "no such command" }));
    }
    if invoke::UNAVAILABLE.contains(&command.as_str()) {
        return failure(invoke::unavailable());
    }
    let window = preview.window.clone();
    let result = if invoke::RAW.contains(&command.as_str()) {
        let mut forwarded = tauri::http::HeaderMap::new();
        for name in ["x-nocturne-mime", "x-nocturne-source"] {
            if let Some(value) = headers.get(name) {
                forwarded.insert(tauri::http::HeaderName::from_static(name), value.clone());
            }
        }
        let mut bytes = body.to_vec();
        let wanted = forwarded
            .get("x-nocturne-mime")
            .and_then(|v| v.to_str().ok())
            .is_some_and(transcode::needs_conversion);
        if wanted
            && let Some(wav) =
                transcode::to_wav(&preview.data_dir.join("preview-tmp"), &bytes).await
        {
            bytes = wav;
            forwarded.insert(
                tauri::http::HeaderName::from_static("x-nocturne-mime"),
                tauri::http::HeaderValue::from_static("audio/wav"),
            );
        }
        tokio::task::spawn_blocking(move || invoke::call_raw(&window, &command, bytes, forwarded))
            .await
    } else {
        let args: Value = if body.is_empty() {
            json!({})
        } else {
            match serde_json::from_slice(&body) {
                Ok(v @ Value::Object(_)) => v,
                _ => return failure(json!({ "code": "invalid", "detail": "send a JSON object" })),
            }
        };
        if names_a_folder(&command, &args) {
            return failure(invoke::unavailable());
        }
        tokio::task::spawn_blocking(move || invoke::call(&window, &command, args)).await
    };
    match result {
        Ok(Ok(mut value)) => {
            files::hide_paths(&mut value);
            json_response(StatusCode::OK, value)
        }
        Ok(Err(error)) => failure(error),
        Err(_) => failure(json!({ "code": "internal", "detail": "the command did not finish" })),
    }
}

async fn ticket(State(preview): State<Arc<Preview>>, body: Bytes) -> Response {
    let kind = match serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|v| v["kind"].as_str().map(str::to_owned))
        .as_deref()
    {
        None | Some("events") => TicketKind::Events,
        Some("media") => TicketKind::Media,
        Some(_) => return failure(json!({ "code": "invalid", "detail": "events or media" })),
    };
    json_response(StatusCode::OK, json!({ "ticket": preview.issue(kind) }))
}

/// Every event the app emits, as server-sent events named as the app names them. A new stream
/// starts with the current status, so a browser that reconnects is up to date.
async fn events(State(preview): State<Arc<Preview>>) -> impl IntoResponse {
    let rx = preview.events.subscribe();
    let first = {
        let preview = preview.clone();
        tokio::task::spawn_blocking(move || preview.first_status())
            .await
            .ok()
            .flatten()
    };
    let stream: std::pin::Pin<Box<dyn Stream<Item = Result<Sse, Infallible>> + Send>> = Box::pin(
        futures_util::stream::unfold((rx, first), |(mut rx, first)| async move {
            if let Some(status) = first {
                let event = Sse::default()
                    .event("status-changed")
                    .data(status.to_string());
                return Some((Ok(event), (rx, None)));
            }
            loop {
                match rx.recv().await {
                    Ok((name, data)) => {
                        return Some((Ok(Sse::default().event(name).data(data)), (rx, None)));
                    }
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => return None,
                }
            }
        }),
    );
    (
        [(header::CACHE_CONTROL, "no-store")],
        axum::response::Sse::new(stream).keep_alive(KeepAlive::default()),
    )
}

/// Serve a file the server itself chose, with the type its extension says and nothing a page
/// could run.
async fn serve_file(path: std::path::PathBuf, request: Request) -> Response {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let Ok(mut response) = ServeFile::new(&path).oneshot(request).await;
    let kind = match ext.as_deref() {
        Some("wav") => Some("audio/wav"),
        Some(e) => nocturne_engine::kind_for_ext(e).map(|(_, mime)| mime),
        None => None,
    };
    let headers = response.headers_mut();
    if let Some(mime) = kind {
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.map(Body::new)
}

async fn original(
    State(preview): State<Arc<Preview>>,
    Path(id): Path<String>,
    request: Request,
) -> Response {
    match files::original_of(&preview, id).await {
        Some(path) => serve_file(path, request).await,
        None => failure(json!({ "code": "notFound", "detail": null })),
    }
}

async fn audio(
    State(preview): State<Arc<Preview>>,
    Path(name): Path<String>,
    request: Request,
) -> Response {
    match files::audio_of(&preview, name).await {
        Some(path) => serve_file(path, request).await,
        None => failure(json!({ "code": "notFound", "detail": null })),
    }
}

/// An unknown API path is an unknown command; anything else is the interface.
async fn fallback(State(preview): State<Arc<Preview>>, request: Request) -> Response {
    let path = request.uri().path().to_string();
    if path.starts_with("/api/") || path.starts_with("/originals/") || path.starts_with("/audio/") {
        return failure(json!({ "code": "notFound", "detail": null }));
    }
    let Some(dir) = preview.opts.ui_dir.clone() else {
        return (StatusCode::NOT_FOUND, "the interface has not been built").into_response();
    };
    let service = ServeDir::new(&dir).fallback(ServeFile::new(dir.join("index.html")));
    let Ok(response) = service.oneshot(request).await;
    let mut response = response.map(Body::new);
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(UI_CSP),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    );
    response
}
