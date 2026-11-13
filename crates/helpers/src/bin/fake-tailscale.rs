//! A stand-in for the `tailscale` command used by tests. It keeps its state in a file beside the
//! daemon's socket and speaks the JSON the real one prints for `up`, `status`, `serve` and `logout`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use nocturne_helpers::HelperKind;
use nocturne_helpers::fake;
use serde_json::{Value, json};

fn state_path(socket: &Path) -> PathBuf {
    socket
        .parent()
        .unwrap_or(Path::new("."))
        .join("fake-ts.json")
}

fn load(socket: &Path) -> Value {
    std::fs::read_to_string(state_path(socket))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({"logged_in": false, "hostname": "", "serve": null}))
}

fn save(socket: &Path, state: &Value) {
    let _ = std::fs::write(state_path(socket), state.to_string());
}

fn wait_for(path: &Path) {
    for _ in 0..2400 {
        if path.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn fail(message: &str, code: i32) -> ! {
    eprintln!("{message}");
    std::process::exit(code)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    fake::record("tailscale", &args);
    let globals: Vec<String> = args
        .iter()
        .take_while(|a| a.starts_with('-'))
        .cloned()
        .collect();
    if let Err(e) = fake::check_flags(HelperKind::Tailscaled, "tailscale", &globals) {
        fail(&e, 2);
    }
    let rest = &args[globals.len()..];
    let socket = PathBuf::from(
        fake::flag_value(&globals, "--socket").unwrap_or_else(|| fail("no socket given", 2)),
    );
    let Some(command) = rest.first().map(String::as_str) else {
        fail("no command given", 2)
    };
    let (fixture, flags) = match (command, rest.get(1).map(String::as_str)) {
        ("serve", Some(sub @ ("status" | "reset" | "set-config"))) => {
            (format!("serve-{sub}"), &rest[2..])
        }
        ("up" | "set" | "status" | "logout" | "serve", _) => (command.to_string(), &rest[1..]),
        _ => fail(&format!("unknown command: {command}"), 2),
    };
    if let Err(e) = fake::check_flags(HelperKind::Tailscaled, &fixture, flags) {
        fail(&e, 2);
    }
    if !socket.exists() {
        fail(
            "failed to connect to local tailscaled; it doesn't appear to be running",
            1,
        );
    }
    let control = fake::control();
    let sockdir = socket.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut state = load(&socket);
    let on = |key: &str| control.get(key).and_then(Value::as_bool) == Some(true);
    match fixture.as_str() {
        "status" => {
            let backend = if state["logged_in"] == true {
                "Running"
            } else {
                "NeedsLogin"
            };
            let name = state["hostname"].as_str().unwrap_or("").to_string();
            let dns = if name.is_empty() {
                String::new()
            } else {
                format!("{name}.tail-fake.ts.net.")
            };
            let peers: Value = control.get("peers").cloned().unwrap_or_else(|| json!({}));
            println!(
                "{}",
                json!({"BackendState": backend, "Self": {"DNSName": dns}, "Peer": peers})
            );
        }
        "up" => {
            if state["logged_in"] != true {
                if on("needs_login") {
                    println!(
                        "{}",
                        json!({"AuthURL": "https://login.example.invalid/a/fake", "BackendState": "NeedsLogin"})
                    );
                    wait_for(&sockdir.join("fake-signin"));
                }
                state["logged_in"] = json!(true);
            }
            state["hostname"] = json!(fake::flag_value(flags, "--hostname").unwrap_or_default());
            save(&socket, &state);
            println!("{}", json!({"BackendState": "Running"}));
        }
        "set" => {}
        "serve" => {
            if on("allow_links") {
                println!(
                    "\nServe is not enabled on your tailnet.\nTo enable, visit:\n\n\thttps://login.example.invalid/f/serve?node=fake\n"
                );
                wait_for(&sockdir.join("fake-links"));
            }
            state["serve"] = json!(flags.last().cloned().unwrap_or_default());
            save(&socket, &state);
            println!("Available within your tailnet:");
        }
        "serve-set-config" => {
            let file = flags.first().cloned().unwrap_or_default();
            let text =
                std::fs::read_to_string(&file).unwrap_or_else(|_| fail("cannot read the file", 1));
            state["serve"] = json!(text);
            save(&socket, &state);
        }
        "serve-status" => {
            let funnel = if on("funnel_on") {
                json!({"fake.ts.net:443": true})
            } else {
                json!({})
            };
            println!(
                "{}",
                json!({"TCP": {"443": {"HTTPS": true}}, "AllowFunnel": funnel})
            );
        }
        "serve-reset" => {
            state["serve"] = Value::Null;
            save(&socket, &state);
        }
        "logout" => {
            state["logged_in"] = json!(false);
            state["serve"] = Value::Null;
            save(&socket, &state);
            let _ = std::fs::remove_file(sockdir.join("tailscaled.state"));
        }
        _ => {}
    }
}
