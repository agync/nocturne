//! The computer's side of the Away from home interop and end to end runs, played over standard
//! input and output. It is the real app on the mock runtime with a scripted mind, turned on
//! through the real commands, the real supervisor and the stand-in `cloudflared`; only the phone
//! is somebody else. `ui/interop` and `ui/e2e-away` start it and drive it.
//!
//! Not part of a normal test run: it waits on its input, so it is ignored unless asked for.
//! Environment:
//! - `AWAY_FAKE_CLOUDFLARED`: the stand-in helper binary to install under the data folder.
//! - `NOCTURNE_FAKE_HELPERS_LOCK`: a lock that pins that binary (the stand-in's `fake` build reads it).
//! - `AWAY_HOST`: the address the stand-in names (default is a fixed one, so a restart keeps it).
//! - `AWAY_REAL`: `simple` or `mostPrivate` for the owner-approved real-helper trials. No stand-in
//!   is installed: cloudflared is downloaded from its pin, and the tailnet helper, which has no
//!   release address yet, is copied from `AWAY_REAL_TAILNET` into the data folder (the app checks
//!   it against the pin before it starts). Never part of a test run; see `ui/e2e-trials`.
//!
//! Each input line is a JSON object with `cmd` and `id`; each answer is a line starting with
//! `AWAYDRV ` and a JSON object with the same `id` and `ok` or `error`.

mod common;

use std::io::{BufRead, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::*;
use nocturne_app_lib::ask::AskLane;
use nocturne_app_lib::remote::controller::RemoteHub;
use nocturne_contract::{ItemFilter, PairingPhase, RemoteState, Source};
use nocturne_engine::{Candidate, FakeMind, Predicate};
use serde_json::{Value, json};

const HOST: &str = "amber-otter-lamp-quiet.trycloudflare.com";

fn say(v: Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "AWAYDRV {v}");
    let _ = out.flush();
}

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

fn until<T>(what: &str, secs: u64, mut f: impl FnMut() -> Option<T>) -> Result<T, String> {
    let start = Instant::now();
    loop {
        if let Some(v) = f() {
            return Ok(v);
        }
        if start.elapsed() > Duration::from_secs(secs) {
            return Err(format!("timed out: {what}"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A small JPEG header followed by a marker the relay must never see.
fn photo(marker: &str, pad: usize) -> Vec<u8> {
    let mut b = vec![0xff, 0xd8, 0xff, 0xe0, 0, 0x10, b'J', b'F', b'I', b'F'];
    b.extend_from_slice(marker.as_bytes());
    b.extend((0..pad).map(|i| (i % 251) as u8));
    b
}

/// A valid one-page PDF that says what it is, with a correct cross-reference table.
fn tiny_pdf() -> Vec<u8> {
    let text = "BT /F1 18 Tf 72 720 Td (Boiler warranty: valid until March 2030.) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{text}\nendstream", text.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

struct Drive {
    real: Option<String>,
    h: Harness,
    hub: Arc<RemoteHub>,
    mind: Arc<FakeMind>,
    gate: Arc<nocturne_engine::Gate>,
    helper_dir: std::path::PathBuf,
}

impl Drive {
    fn state_name(&self) -> String {
        match self.hub.state() {
            RemoteState::Off => "off",
            RemoteState::Starting => "starting",
            RemoteState::On { .. } => "on",
            RemoteState::NeedsRelink { .. } => "needsRelink",
            RemoteState::Problem { .. } => "problem",
            _ => "other",
        }
        .to_string()
    }

    fn start(&self) -> Result<(), String> {
        let connection = self.real.as_deref().unwrap_or("simple");
        self.h
            .call(
                "start_remote",
                json!({"connection": connection, "allowDownload": self.real.is_some()}),
            )
            .map_err(|e| e.to_string())?;
        let secs = if self.real.is_some() { 300 } else { 20 };
        until("on", secs, || match self.hub.state() {
            RemoteState::On { .. } => Some(()),
            RemoteState::NeedsSignIn { .. } if self.real.is_some() => Some(()),
            _ => None,
        })
    }

    /// The address phones use: the helper's own in a real trial, the stand-in's fixed one otherwise.
    fn host(&self) -> String {
        match self.hub.state() {
            RemoteState::On { address, .. } if self.real.is_some() => address
                .trim_start_matches("https://")
                .trim_end_matches('/')
                .to_string(),
            _ => std::env::var("AWAY_HOST").unwrap_or_else(|_| HOST.to_string()),
        }
    }

    /// A note with one public and one private memory (trial check: private stays home).
    fn seed_private(&self) -> Value {
        let e = self.h.engine();
        let note = "Lunch with Maya is on Friday at noon. The spare key hint is the blue heron.";
        let kept = e
            .keep_text(note, nocturne_contract::Source::QuickCapture)
            .unwrap();
        let item = match kept {
            nocturne_contract::Kept::New { item } | nocturne_contract::Kept::Duplicate { item } => {
                item.id
            }
        };
        let p = e.lease_item(&item, Duration::from_secs(60)).unwrap();
        let public = Candidate::new(
            "Lunch with Maya",
            Predicate::Prefers,
            "Friday at noon",
            "Lunch with Maya is on Friday at noon.",
            0.9,
        );
        let private = Candidate::new(
            "Spare key hint",
            Predicate::Prefers,
            "blue heron",
            "The spare key hint is the blue heron.",
            0.9,
        );
        let report = e
            .apply_candidates(&item, &p.lease, vec![public, private])
            .unwrap();
        e.set_private(&report.created[1], true).unwrap();
        self.h.save_todays_page();
        json!({"item": item, "public": report.created[0], "private": report.created[1]})
    }

    /// A one-page PDF with a memory, so a phone can open a PDF original from an answer.
    fn seed_pdf(&self) -> Value {
        let e = self.h.engine();
        let kept = e
            .keep_bytes(&tiny_pdf(), "pdf", Source::QuickCapture)
            .unwrap();
        let item = match kept {
            nocturne_contract::Kept::New { item } | nocturne_contract::Kept::Duplicate { item } => {
                item.id
            }
        };
        let p = e.lease_item(&item, Duration::from_secs(60)).unwrap();
        let c = Candidate::new(
            "Boiler warranty",
            Predicate::Prefers,
            "March 2030",
            "The boiler warranty runs until March 2030.",
            0.9,
        );
        e.apply_candidates(&item, &p.lease, vec![c]).unwrap();
        json!({"item": item})
    }

    fn public_port(&self) -> Result<u16, String> {
        until("the stand-in's public port", 10, || {
            std::fs::read_to_string(self.helper_dir.join("fake-public-port"))
                .ok()
                .and_then(|t| t.trim().parse().ok())
        })
    }

    fn seed(&self) {
        self.h.learn(
            "dentist",
            "The dentist is on Thursday.",
            "Dentist",
            "Thursday",
        );
        let e = self.h.engine();
        let kept = e
            .keep_bytes(
                &photo("DESK-PHOTO-MARKER-5c1e", 4000),
                "jpg",
                Source::QuickCapture,
            )
            .unwrap();
        let item = match kept {
            nocturne_contract::Kept::New { item } | nocturne_contract::Kept::Duplicate { item } => {
                item.id
            }
        };
        let p = e.lease_item(&item, Duration::from_secs(60)).unwrap();
        let c = Candidate::new(
            "Boiler",
            Predicate::Prefers,
            "spring",
            "The boiler is serviced every spring.",
            0.9,
        );
        e.apply_candidates(&item, &p.lease, vec![c]).unwrap();
        self.h.save_todays_page();
    }

    fn items(&self) -> Value {
        let items = self
            .h
            .engine()
            .items(ItemFilter {
                since: None,
                status: None,
                limit: Some(500),
            })
            .unwrap();
        Value::Array(
            items
                .into_iter()
                .map(|i| {
                    json!({
                        "id": i.id,
                        "kind": format!("{:?}", i.kind),
                        "title": i.title,
                        "text": i.text,
                        "source": format!("{:?}", i.source),
                        "hasOriginal": i.has_original,
                        "originalPath": i.original_path,
                    })
                })
                .collect(),
        )
    }

    fn run(&self, cmd: &str, a: &Value) -> Result<Value, String> {
        match cmd {
            "hello" => Ok(json!({
                "host": self.host(),
                "publicPort": if self.real.is_some() { 0 } else { self.public_port()? },
                "socket": self.hub.dir().join("run").join("listener.sock"),
                "dataDir": self.h.dir.path(),
                "state": self.state_name(),
            })),
            "state" => Ok(json!(self.state_name())),
            "on" => {
                self.start()?;
                if self.real.is_some() {
                    return Ok(json!({"host": self.host()}));
                }
                Ok(json!({"publicPort": self.public_port()?}))
            }
            "remote_state" => Ok(serde_json::to_value(self.hub.state()).unwrap()),
            "wait_on" => until("on", a["secs"].as_u64().unwrap_or(60), || {
                matches!(self.hub.state(), RemoteState::On { .. }).then(|| json!(self.host()))
            }),
            "seed_private" => Ok(self.seed_private()),
            "seed_pdf" => Ok(self.seed_pdf()),
            "sign_out" => self
                .h
                .call("stop_remote", json!({"removeAll": true}))
                .map(|_| json!(self.state_name()))
                .map_err(|e| e.to_string()),
            "off" => self
                .h
                .call("stop_remote", json!({"removeAll": false}))
                .map(|_| json!(self.state_name()))
                .map_err(|e| e.to_string()),
            "offer" => {
                let offer = self
                    .h
                    .call("begin_pairing", json!({}))
                    .map_err(|e| e.to_string())?;
                Ok(json!({
                    "pairingId": offer["pairingId"],
                    "link": offer["link"],
                    "typedCode": offer["typedCode"],
                    "typedAddress": offer["typedAddress"],
                    "qr": offer["qr"],
                }))
            }
            "request" => until("a phone asking", 30, || {
                match block(self.hub.view()).pairing.map(|p| p.phase) {
                    Some(PairingPhase::WaitingForYou { request }) => Some(json!({
                        "pairingId": request.pairing_id,
                        "deviceName": request.device_name,
                        "words": request.check_phrase,
                    })),
                    _ => None,
                }
            }),
            "decide" => self
                .h
                .call(
                    "decide_pairing",
                    json!({"pairingId": a["pairingId"], "allow": a["allow"]}),
                )
                .map(|_| json!(true))
                .map_err(|e| e.to_string()),
            "phase" => Ok(serde_json::to_value(block(self.hub.view()).pairing).unwrap()),
            "devices" => Ok(serde_json::to_value(block(self.hub.view()).devices).unwrap()),
            "remove" => self
                .h
                .call("remove_device", json!({"id": a["device"]}))
                .map(|_| json!(true))
                .map_err(|e| e.to_string()),
            "items" => Ok(self.items()),
            "inflight" => Ok(json!(self.h.state().asks.in_flight_in(&AskLane::Phone(
                a["device"].as_str().unwrap_or_default().to_string()
            )))),
            "gate" => {
                if a["open"].as_bool() == Some(true) {
                    self.gate.open();
                } else {
                    self.gate.close();
                    // Forget answers that already passed, so `entered` means the next one.
                    block(async {
                        while tokio::time::timeout(Duration::from_millis(5), self.gate.entered())
                            .await
                            .is_ok()
                        {}
                    });
                }
                Ok(json!(true))
            }
            "entered" => until("an answer to start", 20, || {
                block(async {
                    tokio::time::timeout(Duration::from_millis(50), self.gate.entered())
                        .await
                        .ok()
                })
            })
            .map(|()| json!(true)),
            "save_store" => {
                let to = std::path::PathBuf::from(a["to"].as_str().ok_or("to")?);
                copy_dir(&self.h.dir.path().join("store"), &to.join("store"))
                    .map_err(|e| e.to_string())?;
                Ok(json!(true))
            }
            "calls" => Ok(json!(self.mind.calls())),
            "view" => Ok(serde_json::to_value(block(self.hub.view())).unwrap()),
            other => Err(format!("unknown command {other}")),
        }
    }
}

/// For a real trial: the tailnet helper, which has no release address yet, goes where a download
/// would put it. Cloudflared is left to the app's own pinned download.
fn install_real(data: &std::path::Path, connection: &str) {
    use nocturne_helpers::{HelperKind, HelpersLock};
    use std::os::unix::fs::PermissionsExt;
    if connection != "mostPrivate" {
        return;
    }
    let from = std::env::var("AWAY_REAL_TAILNET").expect("AWAY_REAL_TAILNET");
    let lock = HelpersLock::current();
    let version = &lock.helper(HelperKind::Tailnet).expect("pinned").version;
    let dir = data.join("remote/helpers/tailnet").join(version);
    std::fs::create_dir_all(&dir).unwrap();
    for p in [
        data.join("remote"),
        data.join("remote/helpers"),
        data.join("remote/helpers/tailnet"),
        dir.clone(),
    ] {
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let to = dir.join("nocturne-tailnet");
    std::fs::copy(from, &to).unwrap();
    std::fs::set_permissions(to, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
#[ignore = "driven over stdin by ui/interop and ui/e2e-away"]
fn drive() {
    let mind = Arc::new(FakeMind::new());
    let gate = mind.gate("answer");
    gate.open();
    let h = harness_with_mind(mind.clone());
    let real = std::env::var("AWAY_REAL").ok();
    let helper_dir = h.dir.path().join("remote/helpers/cloudflared/fake");
    if let Some(connection) = &real {
        install_real(h.dir.path(), connection);
    } else {
        std::fs::create_dir_all(&helper_dir).unwrap();
        use std::os::unix::fs::PermissionsExt;
        let from = std::env::var("AWAY_FAKE_CLOUDFLARED").expect("AWAY_FAKE_CLOUDFLARED");
        std::fs::copy(from, helper_dir.join("cloudflared")).unwrap();
        for p in [
            h.dir.path().join("remote"),
            h.dir.path().join("remote/helpers"),
            h.dir.path().join("remote/helpers/cloudflared"),
            helper_dir.clone(),
        ] {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    let host = std::env::var("AWAY_HOST").unwrap_or_else(|_| HOST.to_string());
    let record = h.dir.path().join("relay.bin");
    if real.is_none() {
        std::fs::write(
            helper_dir.join("fake-control.json"),
            json!({"address": host, "record_relay": record}).to_string(),
        )
        .unwrap();
    }
    let hub = nocturne_app_lib::remote::hub(h.app.handle()).expect("a hub");
    let d = Drive {
        real,
        h,
        hub,
        mind,
        gate,
        helper_dir,
    };
    d.seed();
    say(json!({"id": 0, "ok": {"ready": true, "relay": record}}));
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = v["id"].clone();
        let cmd = v["cmd"].as_str().unwrap_or_default().to_string();
        if cmd == "quit" {
            say(json!({"id": id, "ok": true}));
            break;
        }
        match d.run(&cmd, &v) {
            Ok(ok) => say(json!({"id": id, "ok": ok})),
            Err(error) => say(json!({"id": id, "error": error})),
        }
    }
    block(d.hub.shutdown());
}
