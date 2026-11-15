//! The routes.

use std::convert::Infallible;
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Extension, Path, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware;
use axum::response::sse::{Event as Sse, KeepAlive};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures_util::Stream;
use serde_json::{Value, json};
use tauri::Manager;
use tokio::sync::broadcast::error::RecvError;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

use super::{Preview, TicketKind, auth, files, invoke, transcode};

/// What an interface needs from the page it loads: nothing from elsewhere, and no frames.
const UI_CSP: &str = "default-src 'self'; img-src 'self' blob: data:; media-src 'self' blob:; \
    style-src 'self' 'unsafe-inline'; font-src 'self'; connect-src 'self'; frame-ancestors 'none'";

/// What any request may carry except a raw upload, which has its own, larger limit.
const JSON_LIMIT: usize = 1 << 20;

/// Everything the browser talks to.
pub fn router(preview: Arc<Preview>) -> Router {
    Router::new()
        .route(
            "/api/invoke/{command}",
            post(invoke_command).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/ticket", post(ticket))
        .route("/healthz", get(health))
        .route("/api/events", get(events))
        .route("/originals/{id}", get(original))
        .route("/audio/{name}", get(audio))
        .fallback(fallback)
        .layer(DefaultBodyLimit::max(JSON_LIMIT))
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
    body: Body,
) -> Response {
    if !crate::COMMAND_NAMES.contains(&command.as_str()) {
        return failure(json!({ "code": "notFound", "detail": "no such command" }));
    }
    if invoke::UNAVAILABLE.contains(&command.as_str()) {
        return failure(invoke::unavailable());
    }
    let window = preview.window.clone();
    let raw = invoke::RAW.contains(&command.as_str());
    let _slot = if raw {
        match preview.raw_slots.try_acquire() {
            Ok(permit) => Some(permit),
            Err(_) => return failure(json!({ "code": "busy", "detail": "try again shortly" })),
        }
    } else {
        None
    };
    let limit = if raw {
        crate::commands::MAX_RAW_BYTES + (1 << 20)
    } else {
        JSON_LIMIT
    };
    let Ok(body) = axum::body::to_bytes(body, limit).await else {
        return failure(json!({ "code": "invalid", "detail": "that is too large to send" }));
    };
    let result = if raw {
        let mut forwarded = tauri::http::HeaderMap::new();
        for name in ["x-nocturne-mime", "x-nocturne-source"] {
            if let Some(value) = headers.get(name) {
                forwarded.insert(tauri::http::HeaderName::from_static(name), value.clone());
            }
        }
        let mut bytes = Vec::from(body);
        let wanted = forwarded
            .get("x-nocturne-mime")
            .and_then(|v| v.to_str().ok())
            .is_some_and(transcode::needs_conversion);
        if wanted
            && let Some(ffmpeg) = &preview.ffmpeg
            && let Some(wav) =
                transcode::to_wav(ffmpeg, &preview.data_dir.join(super::WORK_DIR), &bytes).await
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
            files::hide_paths(&mut value, &preview.hidden);
            json_response(StatusCode::OK, value)
        }
        Ok(Err(mut error)) => {
            files::hide_paths(&mut error, &preview.hidden);
            failure(error)
        }
        Err(_) => failure(json!({ "code": "internal", "detail": "the command did not finish" })),
    }
}

/// Whether the store is open, for a container's health check. It carries no data, so it needs no
/// token, but it still needs an accepted host.
async fn health(State(preview): State<Arc<Preview>>) -> Response {
    let open = preview.app.state::<crate::AppState>().has_engine();
    let status = if open {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    json_response(status, json!({ "ok": open }))
}

async fn ticket(
    State(preview): State<Arc<Preview>>,
    admitted: Option<Extension<auth::Admitted>>,
    body: Bytes,
) -> Response {
    let Some(Extension(admitted)) = admitted else {
        return auth::refuse(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let kind = match serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|v| v["kind"].as_str().map(str::to_owned))
        .as_deref()
    {
        None | Some("events") => TicketKind::Events,
        Some("media") => TicketKind::Media,
        Some(_) => return failure(json!({ "code": "invalid", "detail": "events or media" })),
    };
    let ticket = preview.issue(kind, &admitted);
    json_response(StatusCode::OK, json!({ "ticket": ticket }))
}

/// Every event the app emits, as server-sent events named as the app names them. A new stream
/// starts with the current status, so a browser that reconnects is up to date. A stream ends
/// within a second or so of the token it was opened under being replaced.
async fn events(
    State(preview): State<Arc<Preview>>,
    admitted: Option<Extension<auth::Admitted>>,
) -> Response {
    let Some(Extension(auth::Admitted(opened_under))) = admitted else {
        return auth::refuse(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let rx = preview.events.subscribe();
    let first = {
        let preview = preview.clone();
        tokio::task::spawn_blocking(move || preview.first_status())
            .await
            .ok()
            .flatten()
    };
    let token = preview.opts.token.clone();
    let still_current = move || {
        token
            .current()
            .is_some_and(|t| auth::same(&t, &opened_under))
    };
    let stream: std::pin::Pin<Box<dyn Stream<Item = Result<Sse, Infallible>> + Send>> = Box::pin(
        futures_util::stream::unfold((rx, first), move |(mut rx, first)| {
            let still_current = still_current.clone();
            async move {
                if let Some(status) = first {
                    let event = Sse::default()
                        .event("status-changed")
                        .data(status.to_string());
                    return Some((Ok(event), (rx, None)));
                }
                let mut check = tokio::time::interval(super::STREAM_TOKEN_CHECK);
                loop {
                    tokio::select! {
                        _ = check.tick() => {
                            if !still_current() {
                                return None;
                            }
                        }
                        received = rx.recv() => match received {
                            Ok((name, data)) => {
                                if !still_current() {
                                    return None;
                                }
                                let event = Sse::default().event(name).data(data);
                                return Some((Ok(event), (rx, None)));
                            }
                            Err(RecvError::Lagged(_)) => continue,
                            Err(RecvError::Closed) => return None,
                        },
                    }
                }
            }
        }),
    );
    (
        [(header::CACHE_CONTROL, "no-store")],
        axum::response::Sse::new(stream).keep_alive(KeepAlive::default()),
    )
        .into_response()
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
