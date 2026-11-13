//! The shell and the interface agree on every command: names in both directions, and the
//! argument keys. Reads the interface's own client files; a missing file is a failure, not a
//! skip.

use std::collections::{BTreeMap, BTreeSet};

use nocturne_app_lib::COMMAND_NAMES;

/// Commands the interface does not call, by design.
const SHELL_ONLY: &[&str] = &[];

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
                .filter_map(|p| {
                    p.split(':')
                        .next()
                        .map(|n| n.trim().trim_start_matches('_').to_string())
                })
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

/// The right-hand side of every `export const` in a TypeScript file, as JSON. Understands the
/// literals `ui/src/phone/protocol.ts` uses: numbers (hex too), strings, arrays and objects
/// with bare keys. A value it cannot read (an import) comes back as `None`.
fn ts_constants(src: &str) -> BTreeMap<String, Option<serde_json::Value>> {
    let mut out = BTreeMap::new();
    for (at, _) in src.match_indices("export const ") {
        let rest = &src[at + "export const ".len()..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let rest = &rest[rest.find('=').expect("an initialiser") + 1..];
        let (mut depth, mut in_str, mut end) = (0i32, false, rest.len());
        for (i, c) in rest.char_indices() {
            match c {
                '"' => in_str = !in_str,
                '[' | '{' | '(' if !in_str => depth += 1,
                ']' | '}' | ')' if !in_str => depth -= 1,
                ';' if !in_str && depth == 0 => {
                    end = i;
                    break;
                }
                _ => {}
            }
        }
        out.insert(name, ts_literal(&rest[..end]));
    }
    out
}

fn ts_literal(s: &str) -> Option<serde_json::Value> {
    let chars: Vec<char> = s.chars().collect();
    let mut json = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            let close = chars[i + 1..].iter().position(|&x| x == '"')? + i + 1;
            json.extend(&chars[i..=close]);
            i = close + 1;
        } else if c == '0' && chars.get(i + 1) == Some(&'x') {
            let digits: String = chars[i + 2..]
                .iter()
                .take_while(|x| x.is_ascii_hexdigit())
                .collect();
            json.push_str(&u64::from_str_radix(&digits, 16).ok()?.to_string());
            i += 2 + digits.len();
        } else if c.is_ascii_alphabetic() || c == '_' {
            let ident: String = chars[i..]
                .iter()
                .take_while(|x| x.is_ascii_alphanumeric() || **x == '_')
                .collect();
            i += ident.len();
            let next = chars[i..].iter().find(|x| !x.is_whitespace());
            if next == Some(&':') {
                json.push_str(&format!("\"{ident}\""));
            } else {
                json.push_str(&ident);
            }
        } else if c == ',' {
            let next = chars[i + 1..].iter().find(|x| !x.is_whitespace());
            if !matches!(next, Some(']') | Some('}')) {
                json.push(c);
            }
            i += 1;
        } else {
            json.push(c);
            i += 1;
        }
    }
    serde_json::from_str(&json).ok()
}

#[test]
fn phone_protocol_matches_shell() {
    use nocturne_app_lib::remote::protocol;

    let mut ts = ts_constants(&read("../ui/src/phone/protocol.ts"));
    assert_eq!(
        ts.remove("WORDS"),
        Some(None),
        "the phone's words come from words.json"
    );
    let shell = protocol::table();
    assert_eq!(
        ts.keys().collect::<BTreeSet<_>>(),
        shell.keys().collect::<BTreeSet<_>>(),
        "the same constant names on both sides"
    );
    for (name, value) in &shell {
        assert_eq!(ts[name].as_ref(), Some(value), "{name} differs");
    }

    let words: Vec<String> = serde_json::from_str(&read("../ui/src/phone/words.json")).unwrap();
    assert_eq!(words.len(), 2048);
    assert!(
        words.iter().eq(protocol::WORDS.iter()),
        "the phone's word list is the shell's"
    );
}

#[test]
fn the_constant_reader_reads_what_it_should() {
    let src = r#"
        import list from "./x.json";
        export const A = 26;
        export const B = "up__";
        export const C: readonly (readonly [number, number])[] = [
          [0x0000, 0x001f],
          [0x200e, 0x200f],
        ];
        export const D: readonly T[] = [{ mime: "a/b", magic: [[4, "ff.."]] }];
        export const E: readonly string[] = list;
    "#;
    let c = ts_constants(src);
    assert_eq!(c["A"], Some(serde_json::json!(26)));
    assert_eq!(c["B"], Some(serde_json::json!("up__")));
    assert_eq!(c["C"], Some(serde_json::json!([[0, 31], [8206, 8207]])));
    assert_eq!(
        c["D"],
        Some(serde_json::json!([{"mime": "a/b", "magic": [[4, "ff.."]]}]))
    );
    assert_eq!(c["E"], None);
}
