//! The HTTP front of the preview and the server: what it admits, what it forwards and what it shows.
//! Requests go straight to the router, without a socket.

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, Response, StatusCode};
use common::*;
use http_body_util::BodyExt;
use nocturne_app_lib::online::{
    self,
    testing::{MemoryKeychain, StandInPort},
};
use nocturne_app_lib::preview::{self, Options, Preview};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "t0ken-for-tests";
const PORT: u16 = 55099;
const PUBLIC: &str = "https://preview.example.test:8443";

struct Rig {
    h: Harness,
    router: Router,
}

fn rig_with(ui: Option<std::path::PathBuf>) -> Rig {
    let h = harness();
    let opts = Options {
        token: TOKEN.into(),
        port: PORT,
        public_origin: Some(PUBLIC.into()),
        ui_dir: ui,
    };
    let server: Arc<Preview> = Preview::new(
        h.app.handle().clone(),
        h.webview.clone(),
        h.dir.path(),
        opts,
    );
    Rig {
        router: preview::router(server),
        h,
    }
}

fn rig() -> Rig {
    rig_with(None)
}

fn req(method: Method, uri: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("host", format!("127.0.0.1:{PORT}"))
}

fn authed(method: Method, uri: &str) -> axum::http::request::Builder {
    req(method, uri).header("authorization", format!("Bearer {TOKEN}"))
}

async fn send(rig: &Rig, request: Request<Body>) -> Response<Body> {
    rig.router.clone().oneshot(request).await.unwrap()
}

async fn json_of(response: Response<Body>) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

async fn invoke(rig: &Rig, command: &str, args: Value) -> (StatusCode, Value) {
    let request = authed(Method::POST, &format!("/api/invoke/{command}"))
        .header("content-type", "application/json")
        .body(Body::from(args.to_string()))
        .unwrap();
    let response = send(rig, request).await;
    (response.status(), json_of(response).await)
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_with_data_is_served_without_the_token() {
    let r = rig();
    for (method, uri) in [
        (Method::POST, "/api/invoke/get_status"),
        (Method::POST, "/api/ticket"),
        (Method::GET, "/api/events"),
        (Method::GET, "/originals/abc"),
        (Method::GET, "/audio/abc.wav"),
        (Method::GET, "/api/nothing"),
    ] {
        let response = send(&r, req(method, uri).body(Body::empty()).unwrap()).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }
    let wrong = req(Method::POST, "/api/invoke/get_status")
        .header("authorization", "Bearer nope")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&r, wrong).await.status(), StatusCode::UNAUTHORIZED);
    let not_bearer = req(Method::POST, "/api/invoke/get_status")
        .header("authorization", TOKEN)
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&r, not_bearer).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_origin_or_host_is_refused_even_with_the_token() {
    let r = rig();
    let evil = authed(Method::POST, "/api/invoke/get_status")
        .header("origin", "https://evil.example.test")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&r, evil).await.status(), StatusCode::FORBIDDEN);
    let null = authed(Method::POST, "/api/invoke/get_status")
        .header("origin", "null")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&r, null).await.status(), StatusCode::FORBIDDEN);
    let host = Request::builder()
        .method(Method::POST)
        .uri("/api/invoke/get_status")
        .header("host", "rebind.example.test")
        .header("authorization", format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&r, host).await.status(), StatusCode::FORBIDDEN);

    let loopback = authed(Method::POST, "/api/invoke/get_status")
        .header("origin", format!("http://127.0.0.1:{PORT}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&r, loopback).await.status(), StatusCode::OK);
    let public = Request::builder()
        .method(Method::POST)
        .uri("/api/invoke/get_status")
        .header("host", "preview.example.test:8443")
        .header("origin", PUBLIC)
        .header("authorization", format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&r, public).await.status(), StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_note_kept_over_http_is_listed() {
    let r = rig();
    let (status, kept) = invoke(
        &r,
        "keep_text",
        json!({ "text": "Dentist on Thursday at 14:30", "source": "quickCapture" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{kept}");
    let id = id_of(&kept);
    let (status, items) = invoke(&r, "list_items", json!({ "filter": {} })).await;
    assert_eq!(status, StatusCode::OK, "{items}");
    assert!(items.as_array().unwrap().iter().any(|i| i["id"] == id));
    // The same app as the one the harness drives directly.
    assert_eq!(r.h.engine().items(Default::default()).unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn failures_carry_a_code_and_the_desktop_commands_are_unavailable() {
    let r = rig();
    let (status, body) = invoke(&r, "get_item", json!({ "id": "missing" })).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "notFound");
    let (status, body) = invoke(&r, "keep_text", json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid");
    let (status, body) = invoke(&r, "no_such_command", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "notFound");
    for command in [
        "pick_folder",
        "pick_files",
        "open_original",
        "open_logs",
        "open_data_folder",
        "open_sound_settings",
        "print_page",
        "export_to",
        "pick_backup",
        "pick_backup_save",
        "make_backup",
        "probe_backup",
        "estimate_backup",
        "check_backup",
        "restore_backup",
        "restore_put_aside",
        "delete_put_aside",
        "get_backups",
        "dismiss_backup_notices",
        "get_restored",
        "finish_restore",
        "cancel_backup",
        "discard_restore",
    ] {
        let (status, body) = invoke(&r, command, json!({})).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{command}");
        assert_eq!(body["code"], "unavailable", "{command}");
    }
    let (status, body) = invoke(
        &r,
        "update_settings",
        json!({ "patch": { "inboxFolder": "/some/folder" } }),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "unavailable");
}

/// The preview never drives Away from home, even the read that works in the window.
#[tokio::test(flavor = "multi_thread")]
async fn preview_refuses_remote_commands() {
    let r = rig();
    assert!(r.h.call("get_remote", json!({})).is_ok());
    for command in [
        "get_remote",
        "start_remote",
        "stop_remote",
        "pause_remote_download",
        "begin_pairing",
        "decide_pairing",
        "cancel_pairing",
        "rename_device",
        "remove_device",
        "list_remote_events",
        "update_remote_settings",
        "open_remote_sign_in",
        "new_computer_key",
        "get_move",
        "answer_move",
        "finish_move",
        "stop_move",
    ] {
        let (status, body) = invoke(&r, command, json!({})).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{command}");
        assert_eq!(body["code"], "unavailable", "{command}");
    }
}

/// Online answers and choosing models go through the preview as through the window: each
/// reaches the app, and what it answers is what the window would be told.
#[tokio::test(flavor = "multi_thread")]
async fn preview_takes_online_and_model_commands() {
    let r = rig();
    online::manage_hub(
        r.h.app.handle(),
        Arc::new(StandInPort::new(None)),
        Arc::new(MemoryKeychain::default()),
    )
    .unwrap();
    nocturne_app_lib::models::install(r.h.app.handle()).unwrap();
    for (command, args) in [
        ("get_online", json!({})),
        ("list_online_sends", json!({ "limit": 10, "before": null })),
    ] {
        let (status, body) = invoke(&r, command, args.clone()).await;
        assert_eq!(status, StatusCode::OK, "{command}: {body}");
        assert_eq!(body, r.h.ok(command, args), "{command}");
    }
    let (status, body) = invoke(&r, "get_models", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["jobs"].is_array(), "{body}");
    // The key travels in the request body only; an address that is never a provider is
    // refused before anything is sent.
    let (status, body) = invoke(
        &r,
        "check_online",
        json!({ "form": { "baseUrl": "http://169.254.169.254/v1", "key": "sk-preview", "model": "m" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["type"], "addressNotAllowed");
    let refused = json!({ "code": "unavailable", "detail": "this is not available in a browser" });
    for command in [
        "get_online",
        "check_online",
        "cancel_online_check",
        "turn_on_online",
        "turn_off_online",
        "abandon_online_setup",
        "see_online_change",
        "update_online_settings",
        "list_online_sends",
        "get_models",
        "list_model_choices",
        "preview_model_change",
        "change_model",
        "pause_model_download",
        "resume_model_download",
        "cancel_model_change",
        "cancel_reindex",
        "keep_old_model",
        "remove_old_model",
        "reset_models_to_tier",
    ] {
        let (_, body) = invoke(&r, command, json!({})).await;
        assert_ne!(body, refused, "{command}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_recording_is_kept_from_the_raw_body() {
    let r = rig();
    let raw = |mime: Option<&str>, bytes: Vec<u8>| {
        let mut b = authed(Method::POST, "/api/invoke/keep_recording")
            .header("content-type", "application/octet-stream");
        if let Some(m) = mime {
            b = b.header("x-nocturne-mime", m);
        }
        b.body(Body::from(bytes)).unwrap()
    };
    let response = send(&r, raw(Some("audio/wav"), short_wav())).await;
    assert_eq!(response.status(), StatusCode::OK);
    let kept = json_of(response).await;
    assert_eq!(kept["item"]["kind"], "audio", "{kept}");
    // The app records WAV itself; anything else is refused plainly rather than kept unreadable.
    let response = send(&r, raw(Some("audio/webm;codecs=opus"), vec![1u8; 8])).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = send(&r, raw(None, short_wav())).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// A tenth of a second of silence as 16 kHz mono 16-bit WAV.
fn short_wav() -> Vec<u8> {
    let samples = 1600u32;
    let data_len = samples * 2;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&16_000u32.to_le_bytes());
    b.extend_from_slice(&32_000u32.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    b.resize(44 + data_len as usize, 0);
    b
}

async fn read_event(body: &mut Body, name: &str) -> String {
    let mut seen = String::new();
    let wait = async {
        while let Some(frame) = body.frame().await {
            let frame = frame.unwrap();
            if let Some(data) = frame.data_ref() {
                seen.push_str(&String::from_utf8_lossy(data));
                if let Some(at) = seen.find(&format!("event: {name}"))
                    && seen[at..].contains("\n\n")
                {
                    return;
                }
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .unwrap_or_else(|_| panic!("no {name} event; heard {seen:?}"));
    seen
}

#[tokio::test(flavor = "multi_thread")]
async fn the_event_stream_delivers_what_the_app_emits() {
    let r = rig();
    let (_, issued) = {
        let response = send(
            &r,
            authed(Method::POST, "/api/ticket")
                .body(Body::from(json!({ "kind": "events" }).to_string()))
                .unwrap(),
        )
        .await;
        (response.status(), json_of(response).await)
    };
    let ticket = issued["ticket"].as_str().unwrap().to_string();
    let uri = format!("/api/events?ticket={ticket}");
    let response = send(&r, req(Method::GET, &uri).body(Body::empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "text/event-stream",
        "an event stream"
    );
    let mut body = response.into_body();
    // A new stream starts with the status.
    read_event(&mut body, "status-changed").await;
    // A ticket works once.
    let again = send(&r, req(Method::GET, &uri).body(Body::empty()).unwrap()).await;
    assert_eq!(again.status(), StatusCode::UNAUTHORIZED);

    let (status, kept) = invoke(
        &r,
        "keep_text",
        json!({ "text": "Gran moved to York", "source": "quickCapture" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let heard = read_event(&mut body, "item-changed").await;
    assert!(heard.contains(&id_of(&kept)), "{heard}");
}

#[tokio::test(flavor = "multi_thread")]
async fn originals_are_shown_by_id_and_never_by_path() {
    let r = rig();
    let png = [0x89u8, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3, 4];
    let response = send(
        &r,
        authed(Method::POST, "/api/invoke/keep_bytes")
            .header("x-nocturne-mime", "image/png")
            .header("x-nocturne-source", "paste")
            .body(Body::from(png.to_vec()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let id = id_of(&json_of(response).await);

    // What the browser reads names a link, not a place on this machine.
    let (_, item) = invoke(&r, "get_item", json!({ "id": id })).await;
    assert_eq!(item["originalPath"], format!("/originals/{id}"));
    let (_, items) = invoke(&r, "list_items", json!({ "filter": {} })).await;
    assert!(!items.to_string().contains("/store/"));
    assert!(
        !items
            .to_string()
            .contains(&*r.h.dir.path().to_string_lossy())
    );

    let shown = send(
        &r,
        authed(Method::GET, &format!("/originals/{id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(shown.status(), StatusCode::OK);
    assert_eq!(shown.headers()["content-type"], "image/png");
    assert_eq!(shown.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        shown
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .as_ref(),
        png
    );

    // A media ticket opens originals and nothing else.
    let media = send(
        &r,
        authed(Method::POST, "/api/ticket")
            .body(Body::from(json!({ "kind": "media" }).to_string()))
            .unwrap(),
    )
    .await;
    let ticket = json_of(media).await["ticket"].as_str().unwrap().to_string();
    for _ in 0..2 {
        let by_ticket = send(
            &r,
            req(Method::GET, &format!("/originals/{id}?t={ticket}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(by_ticket.status(), StatusCode::OK);
    }
    let elsewhere = send(
        &r,
        req(Method::POST, &format!("/api/invoke/get_status?t={ticket}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(elsewhere.status(), StatusCode::UNAUTHORIZED);

    // Unknown ids, and anything shaped like a path, are not found.
    let stray = r.h.dir.path().join("stray.txt");
    std::fs::write(&stray, b"secret").unwrap();
    for uri in [
        "/originals/nothing".to_string(),
        "/originals/..%2F..%2Fstray.txt".to_string(),
        "/originals/%2e%2e".to_string(),
        "/originals/..".to_string(),
        "/originals/a/b".to_string(),
        format!("/originals/{}", stray.display()),
        format!("/originals/{id}/../{id}"),
        "/audio/..%2Fstray.txt".to_string(),
        "/audio/.hidden".to_string(),
        "/audio/missing.wav".to_string(),
    ] {
        let response = send(&r, authed(Method::GET, &uri).body(Body::empty()).unwrap()).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_interface_is_served_without_the_token_but_not_to_a_wrong_host() {
    let ui = tempfile::tempdir().unwrap();
    std::fs::write(
        ui.path().join("index.html"),
        "<!doctype html><title>x</title>",
    )
    .unwrap();
    let r = rig_with(Some(ui.path().to_path_buf()));
    let page = send(&r, req(Method::GET, "/").body(Body::empty()).unwrap()).await;
    assert_eq!(page.status(), StatusCode::OK);
    assert!(
        page.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("default-src 'self'")
    );
    let wrong = Request::builder()
        .uri("/")
        .header("host", "rebind.example.test")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&r, wrong).await.status(), StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "multi_thread")]
async fn only_a_raw_upload_may_be_large() {
    let r = rig();
    let big = vec![b' '; (1 << 20) + 1];
    let response = send(
        &r,
        authed(Method::POST, "/api/invoke/get_status")
            .body(Body::from(big.clone()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = send(
        &r,
        authed(Method::POST, "/api/ticket")
            .body(Body::from(big))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let response = send(
        &r,
        authed(Method::POST, "/api/invoke/keep_bytes")
            .header("x-nocturne-mime", "image/png")
            .body(Body::from(vec![0u8; (1 << 20) + 1]))
            .unwrap(),
    )
    .await;
    let refused = json_of(response).await;
    assert!(
        !refused.to_string().contains("too large to send"),
        "{refused}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_data_folder_is_named_nowhere_in_an_answer() {
    let r = rig();
    let dir = r.h.dir.path().to_string_lossy().into_owned();
    for command in ["get_status", "get_settings", "get_item"] {
        let (_, body) = invoke(&r, command, json!({ "id": "missing" })).await;
        assert!(!body.to_string().contains(&dir), "{command}: {body}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_health_check_needs_no_token_but_an_accepted_host() {
    let r = rig();
    let ok = send(
        &r,
        req(Method::GET, "/healthz").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::OK);
    assert_eq!(json_of(ok).await, json!({ "ok": true }));
    let wrong = Request::builder()
        .uri("/healthz")
        .header("host", "rebind.example.test")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&r, wrong).await.status(), StatusCode::FORBIDDEN);
}

/// A rig whose token is kept in a file, as the server keeps it.
struct FileRig {
    rig: Rig,
    path: std::path::PathBuf,
    _keep: tempfile::TempDir,
}

fn file_rig() -> (FileRig, String) {
    let h = harness();
    let keep = tempfile::tempdir().unwrap();
    let path = keep.path().join("server-token");
    let (token, _) = preview::ensure_token_file(&path).unwrap();
    let opts = Options {
        token: preview::Token::file(&path),
        port: PORT,
        public_origin: None,
        ui_dir: None,
    };
    let server = Preview::new(
        h.app.handle().clone(),
        h.webview.clone(),
        h.dir.path(),
        opts,
    );
    let rig = Rig {
        router: preview::router(server),
        h,
    };
    (
        FileRig {
            rig,
            path,
            _keep: keep,
        },
        token,
    )
}

fn with_bearer(method: Method, uri: &str, token: &str) -> axum::http::request::Builder {
    req(method, uri).header("authorization", format!("Bearer {token}"))
}

async fn ticket_for(rig: &Rig, token: &str, kind: &str) -> String {
    let response = send(
        rig,
        with_bearer(Method::POST, "/api/ticket", token)
            .body(Body::from(json!({ "kind": kind }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    json_of(response).await["ticket"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_token_kept_in_a_file_admits_and_a_new_one_replaces_it() {
    let (f, first) = file_rig();
    let status_with = |token: String| {
        let router = f.rig.router.clone();
        async move {
            let request = with_bearer(Method::POST, "/api/invoke/get_status", &token)
                .body(Body::empty())
                .unwrap();
            router.oneshot(request).await.unwrap().status()
        }
    };
    assert_eq!(status_with(first.clone()).await, StatusCode::OK);
    let second = preview::rotate_token_file(&f.path).unwrap();
    assert_eq!(status_with(second).await, StatusCode::OK);
    assert_eq!(status_with(first).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_token_ends_the_tickets_made_under_the_old_one() {
    let (f, old) = file_rig();
    let events = ticket_for(&f.rig, &old, "events").await;
    let media = ticket_for(&f.rig, &old, "media").await;
    let probe = format!("/audio/none.wav?t={media}");
    let before = send(
        &f.rig,
        req(Method::GET, &probe).body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(
        before.status(),
        StatusCode::NOT_FOUND,
        "admitted, then not found"
    );
    preview::rotate_token_file(&f.path).unwrap();
    let uri = format!("/api/events?ticket={events}");
    let stream = send(&f.rig, req(Method::GET, &uri).body(Body::empty()).unwrap()).await;
    assert_eq!(stream.status(), StatusCode::UNAUTHORIZED);
    let after = send(
        &f.rig,
        req(Method::GET, &probe).body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_asked_for_just_before_a_new_token_is_bound_to_the_old_one() {
    let (f, old) = file_rig();
    // The body is read after the gate has admitted the request, so the token changes between
    // the check and the issue.
    let path = f.path.clone();
    let body = Body::from_stream(futures_util::stream::once(async move {
        preview::rotate_token_file(&path).unwrap();
        Ok::<_, std::convert::Infallible>(json!({ "kind": "events" }).to_string())
    }));
    let response = send(
        &f.rig,
        with_bearer(Method::POST, "/api/ticket", &old)
            .body(body)
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let ticket = json_of(response).await["ticket"]
        .as_str()
        .unwrap()
        .to_string();
    let uri = format!("/api/events?ticket={ticket}");
    let stream = send(&f.rig, req(Method::GET, &uri).body(Body::empty()).unwrap()).await;
    assert_eq!(stream.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_token_closes_an_open_event_stream() {
    let (f, old) = file_rig();
    let events = ticket_for(&f.rig, &old, "events").await;
    let uri = format!("/api/events?ticket={events}");
    let response = send(&f.rig, req(Method::GET, &uri).body(Body::empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    read_event(&mut body, "status-changed").await;
    preview::rotate_token_file(&f.path).unwrap();
    let ended = async {
        while let Some(frame) = body.frame().await {
            frame.unwrap();
        }
    };
    tokio::time::timeout(Duration::from_secs(10), ended)
        .await
        .expect("the stream ends after the token is replaced");
}
