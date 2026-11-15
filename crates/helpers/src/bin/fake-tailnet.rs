//! A stand-in for `nocturne-tailnet` used by tests. It takes the same flags, refuses Tailscale
//! settings in its environment, reads the key line for a loopback target, speaks the helper's
//! JSON lines and forwards a loopback "public" port to its target, adding the key header.
//!
//! The control file steers it: `refuse` exits 2 before any event; `events` is the script after
//! `starting` (event objects, `{"wait": "<file beside the binary>"}`, `{"sleep_ms": n}` or
//! `{"exit": code}`), by default one `running` event; `ignore_stop` ignores the stop command;
//! `events_runs` limits the script to the first n runs; `logout_fails` reports a failed
//! sign-out; `serve: false` forwards nothing, so it never listens (the network check); `udp_any`, `edge_attempt`, `mode` and the
//! rest are the shared stand-in behaviours.

use std::io::{BufRead, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::time::Duration;

use nocturne_helpers::{HELPER_KEY_HEADER, HelperKind, fake};
use serde_json::{Value, json};

struct Out {
    last: Value,
}

static OUT: Mutex<Out> = Mutex::new(Out { last: Value::Null });

fn emit(event: Value) {
    let mut out = OUT.lock().unwrap_or_else(|e| e.into_inner());
    if event["state"] != "error" {
        out.last = event.clone();
    }
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{event}");
    let _ = stdout.flush();
}

fn repeat() {
    let last = OUT.lock().unwrap_or_else(|e| e.into_inner()).last.clone();
    if !last.is_null() {
        emit(last);
    }
}

fn end(code: i32) -> ! {
    fake::note("exit", json!(code));
    std::process::exit(code)
}

fn refuse(message: &str) -> ! {
    eprintln!("{message}");
    end(2)
}

fn valid_key(k: &str) -> bool {
    (16..=256).contains(&k.len())
        && k.bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'~'))
}

/// Forward a loopback port to the target, once, as the real helper serves its tailnet address.
fn serve(target: &str, key: Option<String>, control: &Value) {
    static STARTED: Mutex<bool> = Mutex::new(false);
    let mut started = STARTED.lock().unwrap_or_else(|e| e.into_inner());
    if *started {
        return;
    }
    *started = true;
    let origin = match target.strip_prefix("unix:") {
        Some(path) => fake::Origin::Unix(path.into()),
        None => fake::Origin::Tcp(target.trim_start_matches("http://").to_string()),
    };
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    if let Ok(addr) = listener.local_addr() {
        fake::write_whole(
            &fake::exe_dir().join(fake::PUBLIC_PORT_FILE),
            &addr.port().to_string(),
        );
    }
    let header = key.map(|k| (HELPER_KEY_HEADER.to_string(), k));
    let record = control
        .get("record_relay")
        .and_then(Value::as_str)
        .map(std::path::PathBuf::from);
    std::thread::spawn(move || fake::relay(listener, origin, header, record));
}

fn main() {
    fake::sleeper_if_asked();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let index = fake::record("tailnet", &args);
    if let Err(e) = fake::check_flags(HelperKind::Tailnet, "help", &args) {
        refuse(&e);
    }
    let control = fake::control();
    if control.get("refuse").and_then(Value::as_bool) == Some(true) {
        refuse("refused by the control file");
    }
    if std::env::vars().any(|(k, _)| {
        let k = k.to_ascii_uppercase();
        k.starts_with("TS_") || k.starts_with("TSNET_")
    }) {
        refuse("refusing to start with Tailscale settings in the environment");
    }
    let (Some(state), Some(hostname), Some(target)) = (
        fake::flag_value(&args, "--state-dir"),
        fake::flag_value(&args, "--hostname"),
        fake::flag_value(&args, "--target"),
    ) else {
        refuse("usage: nocturne-tailnet --state-dir <abs> --hostname <name> --target <target>")
    };
    if !(target.starts_with("unix:/") || target.starts_with("http://127.0.0.1:")) {
        refuse("the target must be unix:<path> or http://127.0.0.1:<port>");
    }
    if fake::hangs(&control) {
        fake::sleep_forever();
    }

    let (lines_tx, lines) = std::sync::mpsc::channel::<Option<String>>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            fake::note("stdin", json!(line));
            if lines_tx.send(Some(line)).is_err() {
                return;
            }
        }
        let _ = lines_tx.send(None);
    });

    let key = if target.starts_with("http://") {
        let first = lines.recv().ok().flatten().unwrap_or_default();
        let key = serde_json::from_str::<Value>(&first)
            .ok()
            .and_then(|v| v.get("key").and_then(Value::as_str).map(String::from))
            .filter(|k| valid_key(k));
        if key.is_none() {
            emit(json!({"state": "error", "reason": "key"}));
            end(1);
        }
        key
    } else {
        None
    };

    emit(json!({"state": "starting"}));
    let _ = std::fs::create_dir_all(&state);
    let _ = std::fs::write(std::path::Path::new(&state).join("fake-node.state"), "{}");
    let _udp = (control.get("udp_any").and_then(Value::as_bool) == Some(true))
        .then(|| std::net::UdpSocket::bind("0.0.0.0:0"));
    fake::behave(&control, index);

    let scripted = control
        .get("events_runs")
        .and_then(Value::as_u64)
        .is_none_or(|n| index as u64 <= n);
    let script = control
        .get("events")
        .filter(|_| scripted)
        .cloned()
        .unwrap_or_else(
            || json!([{"state": "running", "host": format!("{hostname}.tail-fake.ts.net")}]),
        );
    let script_control = control.clone();
    let script_target = target.clone();
    std::thread::spawn(move || {
        for step in script.as_array().cloned().unwrap_or_default() {
            if let Some(file) = step.get("wait").and_then(Value::as_str) {
                let path = fake::exe_dir().join(file);
                while !path.exists() {
                    std::thread::sleep(Duration::from_millis(25));
                }
            } else if let Some(ms) = step.get("sleep_ms").and_then(Value::as_u64) {
                std::thread::sleep(Duration::from_millis(ms));
            } else if let Some(code) = step.get("exit").and_then(Value::as_i64) {
                end(code as i32);
            } else {
                if step["state"] == "running" && script_control.get("serve") != Some(&json!(false))
                {
                    serve(&script_target, key.clone(), &script_control);
                }
                emit(step);
            }
        }
    });

    let on = |name: &str| control.get(name).and_then(Value::as_bool) == Some(true);
    loop {
        let Ok(Some(line)) = lines.recv() else {
            emit(json!({"state": "stopped"}));
            end(0);
        };
        let cmd = serde_json::from_str::<Value>(&line)
            .ok()
            .and_then(|v| v.get("cmd").and_then(Value::as_str).map(String::from));
        match cmd.as_deref() {
            Some("status") => repeat(),
            Some("stop") if !on("ignore_stop") => {
                emit(json!({"state": "stopped"}));
                end(0);
            }
            Some("logout") => {
                if on("logout_fails") {
                    emit(json!({"state": "error", "reason": "logout"}));
                } else {
                    let _ =
                        std::fs::remove_file(std::path::Path::new(&state).join("fake-node.state"));
                }
                emit(json!({"state": "stopped"}));
                end(0);
            }
            _ => {}
        }
    }
}
