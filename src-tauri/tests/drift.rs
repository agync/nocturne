//! The shell and the interface agree on every command: names in both directions, and the
//! argument keys. Reads the interface's own client files; a missing file is a failure, not a
//! skip.

use std::collections::{BTreeMap, BTreeSet};

use nocturne_app_lib::COMMAND_NAMES;

/// Commands the interface does not call, by design.
const SHELL_ONLY: &[&str] = &[
    "get_edition",
    "print_page",
    "open_sound_settings",
    "quick_keep_command",
    // The battery notice's "Tidy anyway"; the interface wires it to the notice action.
    "tidy_anyway",
];

fn read(rel: &str) -> String {
    let path = format!("{}/{rel}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("{rel} must exist for the drift check (the UI is part of this checkout): {e}")
    })
}

/// Split at commas that are not inside brackets.
fn split_top(s: &str) -> Vec<String> {
    let (mut depth, mut cur, mut out) = (0i32, String::new(), Vec::new());
    for c in s.chars() {
        match c {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// Every command the UI calls, with the argument keys it sends (`None` when the body is raw).
fn ui_calls(src: &str) -> BTreeMap<String, Option<BTreeSet<String>>> {
    let mut out = BTreeMap::new();
    for marker in ["this.call", "invoke"] {
        for (at, _) in src.match_indices(marker) {
            let rest = &src[at + marker.len()..];
            // Skip a generic argument list: `invoke<string | null>(`.
            let rest = if rest.starts_with('<') {
                let mut depth = 0;
                let mut end = 0;
                for (i, c) in rest.char_indices() {
                    match c {
                        '<' => depth += 1,
                        '>' => {
                            depth -= 1;
                            if depth == 0 {
                                end = i + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                &rest[end..]
            } else {
                rest
            };
            let Some(rest) = rest.strip_prefix("(\"") else {
                continue;
            };
            let Some(close) = rest.find('"') else {
                continue;
            };
            let name = &rest[..close];
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                continue;
            }
            let after = rest[close + 1..].trim_start();
            let keys = if after.starts_with(')') {
                Some(BTreeSet::new())
            } else if let Some(obj) = after.strip_prefix(',').map(str::trim_start) {
                if let Some(obj) = obj.strip_prefix('{') {
                    let mut depth = 1;
                    let mut end = 0;
                    for (i, c) in obj.char_indices() {
                        match c {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    end = i;
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    Some(
                        split_top(&obj[..end])
                            .iter()
                            .map(|item| {
                                let item = item.trim();
                                item.split(':').next().unwrap().trim().to_string()
                            })
                            .filter(|k| !k.is_empty())
                            .collect(),
                    )
                } else {
                    None // raw bytes
                }
            } else {
                None
            };
            out.insert(name.to_string(), keys);
        }
    }
    out
}

fn camel(s: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in s.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Every command the shell defines, with its argument keys (camelCase, as Tauri expects them).
fn rust_commands() -> BTreeMap<String, BTreeSet<String>> {
    let dir = format!("{}/src/commands", env!("CARGO_MANIFEST_DIR"));
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let src = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        for (at, _) in src.match_indices("#[tauri::command]") {
            let rest = &src[at..];
            let f = rest.find("fn ").unwrap() + 3;
            let name_end = rest[f..].find(['<', '(']).unwrap();
            let name = rest[f..f + name_end].trim().to_string();
            let open = rest[f..].find('(').unwrap() + f + 1;
            let mut depth = 1;
            let mut close = open;
            for (i, c) in rest[open..].char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = open + i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let keys = split_top(&rest[open..close])
                .iter()
                .filter_map(|p| p.split(':').next().map(|n| n.trim().to_string()))
                .filter(|n| !n.is_empty() && n != "app" && n != "request" && n != "webview")
                .map(|n| camel(&n))
                .collect();
            out.insert(name, keys);
        }
    }
    out
}

fn all_ui_calls() -> BTreeMap<String, Option<BTreeSet<String>>> {
    let mut calls = ui_calls(&read("../ui/src/engine/tauri.ts"));
    calls.extend(ui_calls(&read("../ui/src/platform/tauri.ts")));
    calls
}

#[test]
fn every_command_the_ui_calls_is_registered() {
    let calls = all_ui_calls();
    assert!(calls.len() >= 40, "found too few UI commands: {calls:?}");
    let registered: BTreeSet<&str> = COMMAND_NAMES.iter().copied().collect();
    let missing: Vec<_> = calls
        .keys()
        .filter(|n| !registered.contains(n.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "the UI calls unregistered commands: {missing:?}"
    );
}

#[test]
fn every_registered_command_is_called_by_the_ui_or_is_shell_only() {
    let calls = all_ui_calls();
    let orphans: Vec<_> = COMMAND_NAMES
        .iter()
        .filter(|n| !calls.contains_key(**n) && !SHELL_ONLY.contains(n))
        .collect();
    assert!(
        orphans.is_empty(),
        "registered but never called by the UI (list them as shell-only or remove): {orphans:?}"
    );
    for n in SHELL_ONLY {
        assert!(
            COMMAND_NAMES.contains(n),
            "{n} is listed shell-only but not registered"
        );
    }
}

#[test]
fn the_ui_sends_exactly_the_argument_keys_the_commands_take() {
    let defined = rust_commands();
    let registered: BTreeSet<&str> = COMMAND_NAMES.iter().copied().collect();
    for name in &registered {
        assert!(
            defined.contains_key(*name),
            "{name} has no #[tauri::command] function"
        );
    }
    for (name, keys) in all_ui_calls() {
        let Some(keys) = keys else {
            // A raw body: the command reads the request, not named arguments.
            assert!(
                defined[&name].is_empty(),
                "{name}: raw body but named arguments"
            );
            continue;
        };
        assert_eq!(
            keys, defined[&name],
            "{name}: the UI sends {keys:?}, the command takes {:?}",
            defined[&name]
        );
    }
}

#[test]
fn the_parsers_read_what_they_should() {
    let src = r#"
        a = () => this.call("get_status");
        b = (x) => this.call("set_private", { id, private: isPrivate });
        c = async () => await invoke("keep_bytes", bytes, { headers: {} });
        d = async () => (await invoke<string | null>("pick_folder")) ?? null;
    "#;
    let calls = ui_calls(src);
    assert_eq!(calls["get_status"], Some(BTreeSet::new()));
    assert_eq!(
        calls["set_private"],
        Some(["id", "private"].map(String::from).into_iter().collect())
    );
    assert_eq!(calls["keep_bytes"], None);
    assert_eq!(calls["pick_folder"], Some(BTreeSet::new()));
    assert_eq!(camel("include_originals"), "includeOriginals");
}
