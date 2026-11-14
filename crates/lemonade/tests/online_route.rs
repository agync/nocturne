//! The engine route for Online answers against the fake engine: what each launch writes and
//! hands the engine, the address rules, the relaunches, and the redacting log pipe.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use nocturne_engine::OnlineFailure;
use nocturne_lemonade::client::{ChatRequest, Message};
use nocturne_lemonade::logpipe::{self, Redactor};
use nocturne_lemonade::online::{
    self, CloudRoute, KEY_ENV, OnlineKey, PROVIDER, ROW, UrlProblem, classify, validate_url,
};
use nocturne_lemonade::{Dirs, Supervisor, SupervisorOptions};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

const KEY: &str = "sk-route-test-0123456789abcdefghij";

fn quick() -> SupervisorOptions {
    SupervisorOptions {
        restart_limit: 3,
        health_interval: Duration::from_millis(200),
        start_timeout: Duration::from_secs(15),
        backoff_base: Duration::from_millis(50),
    }
}

async fn start(dirs: &Dirs, route: Option<CloudRoute>) -> Supervisor {
    Supervisor::start_with(dirs.clone(), quick(), route)
        .await
        .unwrap()
}

fn route() -> CloudRoute {
    CloudRoute::new(
        "https://API.Example.com./v1",
        "provider/model-name".into(),
        OnlineKey::new(KEY.into()),
    )
    .unwrap()
}

fn config(dirs: &Dirs) -> Value {
    serde_json::from_slice(&std::fs::read(dirs.config().join("config.json")).unwrap()).unwrap()
}

fn rows(dirs: &Dirs) -> Value {
    std::fs::read(dirs.config().join("user_models.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(json!({}))
}

async fn engine_env(sup: &Supervisor) -> Value {
    let endpoint = sup.client().endpoint().unwrap();
    reqwest::Client::new()
        .get(format!("http://127.0.0.1:{}/fake/env", endpoint.port))
        .bearer_auth(&endpoint.key)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

/// Off: no provider, no row, no key, and nothing in the environment that names a proxy.
async fn assert_off(sup: &Supervisor, dirs: &Dirs) {
    assert_eq!(config(dirs)["cloud_providers"], json!([]));
    assert!(rows(dirs).get(ROW).is_none());
    let env = engine_env(sup).await;
    assert!(env.get(KEY_ENV).is_none());
    assert!(!env.to_string().contains(KEY));
}

#[tokio::test]
async fn off_launch_has_no_provider_row_or_key() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    assert_off(&sup, &dirs).await;
    assert_eq!(config(&dirs)["offline"], true);
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn on_launch_writes_one_entry_row_and_env_key() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    std::fs::create_dir_all(dirs.config()).unwrap();
    std::fs::write(
        dirs.config().join("user_models.json"),
        r#"{"mine": {"recipe": "llamacpp", "checkpoint": "a/b"}}"#,
    )
    .unwrap();
    let sup = start(&dirs, Some(route())).await;
    assert_eq!(
        config(&dirs)["cloud_providers"],
        json!([{
            "name": PROVIDER,
            "base_url": "https://api.example.com/v1",
            "allow_insecure_http": false,
            "wire_format": "openai",
        }]),
        "only the normalised address is written"
    );
    let rows = rows(&dirs);
    assert_eq!(
        rows[ROW],
        json!({"recipe": "cloud", "cloud_provider": PROVIDER, "checkpoint": "provider/model-name",
               "labels": ["chat"], "suggested": false})
    );
    assert_eq!(rows["mine"]["recipe"], "llamacpp", "other rows are kept");
    let env = engine_env(&sup).await;
    assert_eq!(env[KEY_ENV], KEY);
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn key_never_in_argv_or_config_or_row() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, Some(route())).await;
    for file in ["config.json", "user_models.json", "recipe_options.json"] {
        let text = std::fs::read_to_string(dirs.config().join(file)).unwrap_or_default();
        assert!(!text.contains(KEY) && !text.contains(&KEY[..20]), "{file}");
    }
    #[cfg(target_os = "linux")]
    {
        let pid = sup.pid().unwrap();
        let argv = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap();
        assert!(!String::from_utf8_lossy(&argv).contains(&KEY[..20]));
        assert!(!String::from_utf8_lossy(&argv).contains("--log-file"));
    }
    let log = std::fs::read_to_string(dirs.logs().join("lemond.log")).unwrap_or_default();
    assert!(!log.contains(KEY));
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn env_has_no_proxy_variables() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, Some(route())).await;
    let env = engine_env(&sup).await;
    let names: Vec<String> = env.as_object().unwrap().keys().cloned().collect();
    assert!(
        names
            .iter()
            .all(|n| !n.to_ascii_lowercase().contains("proxy")),
        "{names:?}"
    );
    let mut lemonade: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|n| n.starts_with("LEMONADE_"))
        .collect();
    lemonade.sort_unstable();
    assert_eq!(
        lemonade,
        [
            "LEMONADE_ADMIN_API_KEY",
            "LEMONADE_API_KEY",
            "LEMONADE_NOCTURNE_ONLINE_API_KEY"
        ]
    );
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn download_window_launch_has_no_provider_row_or_key() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, Some(route())).await;
    let control = sup.control();
    let hold = control.hold_online().await.unwrap();
    assert!(control.download_window_open());
    assert_eq!(config(&dirs)["offline"], false);
    assert_off(&sup, &dirs).await;
    hold.release().await;
    assert!(!control.download_window_open());
    assert_eq!(config(&dirs)["offline"], true);
    assert_eq!(config(&dirs)["cloud_providers"][0]["name"], PROVIDER);
    assert!(
        rows(&dirs).get(ROW).is_some(),
        "the closing launch restores it"
    );
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn offline_stays_true_with_route() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    sup.control().set_cloud(Some(route())).await.unwrap();
    let cfg = sup.admin().config().await.unwrap();
    assert_eq!(cfg["offline"], true);
    assert_eq!(config(&dirs)["no_fetch_executables"], true);
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn turn_off_relaunches_without_route() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let control = sup.control();
    control.set_cloud(Some(route())).await.unwrap();
    assert!(control.has_cloud());
    assert_eq!(engine_env(&sup).await[KEY_ENV], KEY);
    control.set_cloud(None).await.unwrap();
    assert!(!control.has_cloud());
    assert_off(&sup, &dirs).await;
    assert_eq!(common::launch_count(&dirs), 3);
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn relaunch_for_online_not_counted_as_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    for _ in 0..4 {
        sup.control().set_cloud(Some(route())).await.unwrap();
        sup.control().set_cloud(None).await.unwrap();
    }
    assert_eq!(
        sup.restarts(),
        0,
        "more relaunches than the restart limit, none a crash"
    );
    assert!(matches!(
        *sup.state().borrow(),
        nocturne_lemonade::RuntimeState::Ready { .. }
    ));
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn relaunch_waits_for_idle_with_a_cap() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let control = sup.control();
    let client = sup.client();
    let endpoint = client.endpoint().unwrap();
    let script = |n: usize| {
        json!(
            (0..n)
                .map(|_| json!({"deltas": ["a ", "b ", "c [m1]."], "delay_ms": 50}))
                .collect::<Vec<_>>()
        )
    };
    reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/fake/script", endpoint.port))
        .bearer_auth(&endpoint.key)
        .json(&script(2))
        .send()
        .await
        .unwrap();
    let req = ChatRequest::new("some-model", vec![Message::user("q")]);

    // A request in flight is waited for, here until it is read to its end.
    control.set_idle_waits(Duration::from_secs(10), Duration::from_secs(10));
    let stream = client.chat(&req).await.unwrap();
    assert_eq!(client.busy_total(), 1);
    let reader = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        stream.collect::<Vec<_>>().await
    });
    let began = Instant::now();
    control.set_cloud(Some(route())).await.unwrap();
    let waited = began.elapsed();
    assert!(waited >= Duration::from_millis(350), "{waited:?}");
    assert!(waited < Duration::from_secs(8), "{waited:?}");
    let _ = reader.await;

    // One that does not end is waited for only up to the cap.
    let stream = client.chat(&req).await.unwrap();
    control.set_idle_waits(Duration::from_millis(300), Duration::from_millis(300));
    let began = Instant::now();
    control.set_cloud(None).await.unwrap();
    let waited = began.elapsed();
    assert!(waited >= Duration::from_millis(300), "{waited:?}");
    drop(stream);
    assert_off(&sup, &dirs).await;
    sup.shutdown().await.unwrap();
}

/// The `config.json` entry and the static row are written in one place, the launch, from the
/// same route: no other code outside the unit tests calls either writer.
#[test]
fn one_writer_for_entry_and_row() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut calls = Vec::new();
    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let code = text.split("#[cfg(test)]").next().unwrap_or_default();
            for (n, l) in code.lines().enumerate() {
                let l = l.trim_start();
                if l.starts_with("//") || l.starts_with("pub fn") {
                    continue;
                }
                if l.contains("config::write(") || l.contains("write_row(") {
                    let name = path.file_name().unwrap().to_string_lossy().into_owned();
                    calls.push(format!("{name}:{}", n + 1));
                }
            }
        }
    }
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(
        calls.iter().all(|c| c.starts_with("supervisor.rs:")),
        "{calls:?}"
    );
}

#[test]
fn url_refusals() {
    for raw in [
        "https://api.example.com/v1?key=1",
        "https://127.0.0.1/v1",
        "https://localhost/v1",
        "https://169.254.169.254/latest/meta-data",
        "https://[fe80::1]/v1",
        "https://0.0.0.0/v1",
        "https://[::]/v1",
        "https://10.0.0.1/v1",
        "https://172.16.0.1/v1",
        "https://172.31.255.255/v1",
        "https://192.168.0.10/v1",
        "https://[fc00::1]/v1",
        "https://[fd12:3456::1]/v1",
        "https://[::ffff:10.0.0.1]/v1",
        "https://printer.local/v1",
    ] {
        assert!(validate_url(raw, false).is_err(), "{raw}");
    }
    assert_eq!(
        validate_url("https://api.example.com/v1?key=1", false),
        Err(UrlProblem::Extra)
    );
    assert_eq!(
        validate_url("https://[::ffff:10.0.0.1]/v1", false),
        Err(UrlProblem::NotAllowed)
    );
    assert!(validate_url("https://172.32.0.1/v1", false).is_ok());
}

/// A public name is never resolved here: one that points at this computer or the home network
/// is accepted. The residual risk is written down in ADR 0011.
#[test]
fn private_resolving_name_is_a_residual() {
    for raw in [
        "https://localtest.me/v1",
        "https://127.0.0.1.nip.io/v1",
        "https://10.0.0.1.sslip.io/v1",
    ] {
        assert!(validate_url(raw, false).is_ok(), "{raw}");
    }
}

#[test]
fn release_refuses_http_and_userinfo() {
    for raw in [
        "http://api.example.com/v1",
        "http://127.0.0.1:8080/v1",
        "https://user:secret@api.example.com/v1",
        "https://user@api.example.com/v1",
        "https://api.example.com/v1#x",
    ] {
        assert!(validate_url(raw, false).is_err(), "{raw}");
        assert!(CloudRoute::new(raw, "m".into(), OnlineKey::new("k".into())).is_err());
    }
    let route = route();
    assert!(!route.allow_insecure_http());
    assert!(!online::entry(&route).allow_insecure_http);
}

#[cfg(feature = "online-stand-in")]
#[test]
fn stand_in_http_only_for_loopback_with_feature() {
    let key = || OnlineKey::new("k".into());
    let r = CloudRoute::stand_in("http://127.0.0.1:9/v1", "m".into(), key()).unwrap();
    assert!(r.allow_insecure_http());
    assert!(online::entry(&r).allow_insecure_http);
    for raw in [
        "http://10.0.0.1:9/v1",
        "http://localhost:9/v1",
        "http://127.0.0.1/v1",
        "http://[::1]:9/v1",
    ] {
        assert!(
            CloudRoute::stand_in(raw, "m".into(), key()).is_err(),
            "{raw}"
        );
    }
    let https = CloudRoute::stand_in("https://api.example.com/v1", "m".into(), key()).unwrap();
    assert!(!https.allow_insecure_http());
}

#[cfg(not(feature = "online-stand-in"))]
#[test]
fn stand_in_http_only_for_loopback_with_feature() {
    assert_eq!(
        validate_url("http://127.0.0.1:9/v1", true),
        Err(UrlProblem::NotHttps),
        "without the feature even the stand-in address is refused"
    );
}

#[test]
fn classify_maps_statuses_and_curl_text() {
    use OnlineFailure::*;
    let cases = [
        (None, "", Unreachable),
        (
            Some(401),
            "cloud (nocturne_online) request failed",
            KeyRefused,
        ),
        (Some(403), "", KeyRefused),
        (Some(402), "", NoCredit),
        (Some(404), "", ModelMissing),
        (Some(429), "", Busy),
        (
            Some(500),
            "cloud (nocturne_online) request failed",
            Other(500),
        ),
        (Some(503), "", Other(503)),
        (
            Some(500),
            "Network error: CURL error: Couldn't resolve host name",
            NoInternet,
        ),
        (
            Some(500),
            "Network error: CURL error: Could not resolve host: api.example.com",
            NoInternet,
        ),
        (
            Some(500),
            "Network error: CURL error: Network is unreachable",
            NoInternet,
        ),
        (
            Some(500),
            "Network error: CURL error: Couldn't connect to server",
            Unreachable,
        ),
        (
            Some(500),
            "Network error: CURL error: Timeout was reached",
            Unreachable,
        ),
        (
            Some(500),
            "Network error: CURL error: SSL peer certificate or SSH remote key was not OK",
            Unreachable,
        ),
        (Some(400), "model 'x' is not available", ModelMissing),
        (Some(400), "bad request", Refused),
        (Some(307), "", Refused),
    ];
    for (status, text, want) in cases {
        assert_eq!(classify(status, text), want, "{status:?} {text}");
    }
}

fn line(severity: &str, tag: &str, message: &str) -> String {
    format!("2026-11-13 22:06:22.734 [{severity}] ({tag}) {message}")
}

/// Feeds `lines` through the pipe as the engine's output and returns the log written.
async fn piped(lines: &[String], key: Option<&str>) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("logs").join("lemond.log");
    let redact = Arc::new(Redactor::new());
    let key = key.map(|k| OnlineKey::new(k.into()));
    redact.set_key(key.as_ref());
    let (mut engine, stdout) = tokio::io::duplex(4096);
    let task = logpipe::spawn(stdout, file.clone(), redact);
    let end = line("Info", "Server", "end-of-feed");
    for l in lines.iter().chain([&end]) {
        engine.write_all(l.as_bytes()).await.unwrap();
        engine.write_all(b"\n").await.unwrap();
    }
    drop(engine);
    task.await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        if text.contains("end-of-feed") {
            return text;
        }
        assert!(Instant::now() < deadline, "the log was written: {text}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The error bodies the stand-in sends, as the engine logs them: the non-streaming dump cut at
/// about 200 bytes, so a partial key, and the streaming one.
fn engine_error_lines(key: &str, marker: &str) -> Vec<String> {
    let body = |status: u16| {
        json!({"details": {"response": {"error": {"code": status,
            "message": format!("{marker} authorization was Bearer {key}"),
            "type": "stand_in_error"}}, "status_code": status},
            "message": "cloud (nocturne_online) request failed", "type": "backend_error"})
        .to_string()
    };
    let mut out = Vec::new();
    for status in [401, 500] {
        let full = body(status);
        let cut: String = full.chars().take(200).collect();
        out.push(line(
            "Error",
            "Server",
            &format!("Backend returned error response: {cut}"),
        ));
        out.push(line(
            "Error",
            "Cloud",
            &format!(
                "Provider returned status {status}, body: {}",
                json!({"error": {"message": format!("{marker} authorization was Bearer {key}")}})
            ),
        ));
    }
    out.push(line(
        "Warning",
        "Cloud",
        &format!("GET https://api.example.com/v1/models returned HTTP 401. Body: {marker} {key}"),
    ));
    out.push(line("Info", "Server", "kept-after the bodies"));
    out
}

#[tokio::test]
async fn log_pipe_withholds_bodies() {
    let marker = "LOG-MARKER-7f3a";
    let text = piped(&engine_error_lines(KEY, marker), Some(KEY)).await;
    assert!(!text.contains(marker), "{text}");
    assert!(!text.contains(KEY));
    assert!(!text.contains(&KEY[..20]), "a partial key is withheld too");
    assert!(!text.contains(&KEY[KEY.len() - 8..]));
    assert!(text.contains("Backend returned error response: [withheld]"));
    assert!(text.contains("Provider returned status 500, body: [withheld]"));
    assert!(text.contains("kept-after the bodies"));
    // A key in an ordinary line is replaced, ends and all.
    let text = piped(
        &[line(
            "Info",
            "Server",
            &format!("odd {} and {}", &KEY[..8], &KEY[KEY.len() - 8..]),
        )],
        Some(KEY),
    )
    .await;
    assert!(
        !text.contains(&KEY[..8]) && !text.contains(&KEY[KEY.len() - 8..]),
        "{text}"
    );
}

#[tokio::test]
async fn log_pipe_withholds_multi_line_bodies() {
    let marker = "MULTI-MARKER-91c2";
    let lines = vec![
        line("Info", "Server", "before"),
        line(
            "Error",
            "Cloud",
            &format!("Provider returned status 401, body: {{\"error\": \"{marker} first line"),
        ),
        format!("{marker} second line"),
        format!("authorization was Bearer {}", &KEY[..20]),
        "2026-11-13 not a prefix".to_string(),
        format!("{marker} last line\"}}"),
        line("Info", "Server", "kept-after the body"),
        "plain line after a prefixed one is kept".to_string(),
    ];
    let text = piped(&lines, Some(KEY)).await;
    assert!(!text.contains(marker), "{text}");
    assert!(!text.contains(&KEY[..20]));
    assert!(text.contains("before"));
    assert!(text.contains("body: [withheld]"));
    assert!(
        text.contains("kept-after the body"),
        "the next prefixed line is kept"
    );
    assert!(text.contains("plain line after a prefixed one is kept"));
}

#[tokio::test]
async fn log_pipe_never_blocks_engine() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("lemond.log");
    let (mut engine, stdout) = tokio::io::duplex(1024);
    let task = logpipe::spawn(stdout, file.clone(), Arc::new(Redactor::new()));
    let began = Instant::now();
    let filler = line("Info", "Server", &"x".repeat(900));
    for _ in 0..20_000 {
        engine.write_all(filler.as_bytes()).await.unwrap();
        engine.write_all(b"\n").await.unwrap();
    }
    drop(engine);
    task.await.unwrap();
    assert!(
        began.elapsed() < Duration::from_secs(30),
        "the engine's output was read at its own pace: {:?}",
        began.elapsed()
    );
}

#[tokio::test]
async fn log_file_owner_only_and_rotated() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("lemond.log");
    let (mut engine, stdout) = tokio::io::duplex(4096);
    let task = logpipe::spawn_rotating_at(stdout, file.clone(), Arc::new(Redactor::new()), 2048);
    for i in 0..200 {
        let l = line("Info", "Server", &format!("line {i} {}", "y".repeat(60)));
        engine.write_all(l.as_bytes()).await.unwrap();
        engine.write_all(b"\n").await.unwrap();
        if i % 10 == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    drop(engine);
    task.await.unwrap();
    let rotated = |n: usize| tmp.path().join(format!("lemond.log.{n}"));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !rotated(2).exists() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(file.exists() && rotated(1).exists() && rotated(2).exists());
    assert!(!rotated(3).exists(), "three files in all");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for p in [file, rotated(1), rotated(2)] {
            let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{}", p.display());
        }
    }
}

/// A download window whose relaunch failed is closed again: no later restart opens it without
/// the person's yes.
#[tokio::test]
async fn a_window_that_failed_to_open_stays_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let opts = SupervisorOptions {
        restart_limit: 50,
        backoff_base: Duration::from_millis(300),
        ..quick()
    };
    let sup = Supervisor::start_with(dirs.clone(), opts, None)
        .await
        .unwrap();
    let control = sup.control();
    common::set_mode(&dirs, "fail-bind");
    assert!(control.hold_online().await.is_err());
    assert!(!control.download_window_open(), "rolled back at once");
    common::set_mode(&dirs, "");
    let mut state = sup.state();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if matches!(
                *state.borrow_and_update(),
                nocturne_lemonade::RuntimeState::Ready { .. }
            ) {
                break;
            }
            state.changed().await.unwrap();
        }
    })
    .await
    .expect("the engine came back");
    assert_eq!(config(&dirs)["offline"], true, "the restart stayed offline");
    assert_eq!(config(&dirs)["no_fetch_executables"], true);
    assert!(!control.download_window_open());
    // A later yes still opens it.
    let hold = control.hold_online().await.unwrap();
    assert_eq!(config(&dirs)["offline"], false);
    hold.release().await;
    sup.shutdown().await.unwrap();
}

/// Closing the download window whose relaunch fails keeps it closed: the restart after it
/// launches the engine offline.
#[tokio::test]
async fn a_window_that_failed_to_close_stays_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let opts = SupervisorOptions {
        restart_limit: 50,
        backoff_base: Duration::from_millis(300),
        ..quick()
    };
    let sup = Supervisor::start_with(dirs.clone(), opts, None)
        .await
        .unwrap();
    let control = sup.control();
    let hold = control.hold_online().await.unwrap();
    assert_eq!(config(&dirs)["offline"], false);
    common::set_mode(&dirs, "fail-bind");
    hold.release().await;
    assert!(
        !control.download_window_open(),
        "a failed close is not undone"
    );
    common::set_mode(&dirs, "");
    let mut state = sup.state();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if matches!(
                *state.borrow_and_update(),
                nocturne_lemonade::RuntimeState::Ready { .. }
            ) {
                break;
            }
            state.changed().await.unwrap();
        }
    })
    .await
    .expect("the engine came back");
    assert_eq!(
        config(&dirs)["offline"],
        true,
        "the restart launched offline"
    );
    assert_eq!(config(&dirs)["no_fetch_executables"], true);
    sup.shutdown().await.unwrap();
}

/// A relaunch with a route that fails takes the route away: the restart after it has no key.
#[tokio::test]
async fn a_route_whose_relaunch_failed_is_not_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let opts = SupervisorOptions {
        restart_limit: 50,
        backoff_base: Duration::from_millis(300),
        ..quick()
    };
    let sup = Supervisor::start_with(dirs.clone(), opts, None)
        .await
        .unwrap();
    let control = sup.control();
    common::set_mode(&dirs, "fail-bind");
    assert!(control.set_cloud(Some(route())).await.is_err());
    assert!(!control.has_cloud(), "dropped at once");
    common::set_mode(&dirs, "");
    let mut state = sup.state();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if matches!(
                *state.borrow_and_update(),
                nocturne_lemonade::RuntimeState::Ready { .. }
            ) {
                break;
            }
            state.changed().await.unwrap();
        }
    })
    .await
    .expect("the engine came back");
    assert_off(&sup, &dirs).await;
    sup.shutdown().await.unwrap();
}

/// A line longer than the pipe holds is cut: the rest of it, key and all, is withheld, and the
/// next line is read as usual.
#[tokio::test]
async fn a_very_long_line_is_cut_and_the_rest_withheld() {
    let long = format!(
        "{} {} tail",
        line("Info", "Server", &"z".repeat(200 * 1024)),
        KEY
    );
    let text = piped(
        &[long, line("Info", "Server", "kept-after the long one")],
        None,
    )
    .await;
    assert!(
        !text.contains(&KEY[..20]),
        "nothing past the cut is written"
    );
    assert!(text.contains("kept-after the long one"));
    assert!(text.lines().all(|l| l.len() <= logpipe::MAX_LINE));
}
