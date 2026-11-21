//! The phone builds a computer still speaks: every call each build sends, as that build sent
//! it, still parses. `tests/fixtures/phone-v{n}/{cmd}.json` holds one build's calls, a list of
//! examples per call. The app crate dispatches the same files (`src-tauri/tests/phones_builds.rs`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use nocturne_contract::*;
use serde_json::Value;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// One build and its calls, each with its examples.
type Build = (u32, Vec<(String, Vec<Value>)>);

/// Each build's directory, oldest first, with its calls.
fn builds() -> Vec<Build> {
    let mut out = Vec::new();
    for dir in std::fs::read_dir(fixtures()).unwrap() {
        let dir = dir.unwrap().path();
        let Some(n) = dir
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("phone-v"))
        else {
            continue;
        };
        let mut calls = Vec::new();
        for file in std::fs::read_dir(&dir).unwrap() {
            let file = file.unwrap().path();
            let cmd = file.file_stem().unwrap().to_str().unwrap().to_string();
            let examples: Vec<Value> =
                serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
            calls.push((cmd, examples));
        }
        calls.sort_by(|a, b| a.0.cmp(&b.0));
        out.push((n.parse().unwrap(), calls));
    }
    out.sort_by_key(|b| b.0);
    out
}

/// Equal JSON, with numbers compared by value (`16` and `16.0` are the same size).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

#[test]
fn every_call_of_every_build_still_parses_as_sent() {
    let builds = builds();
    let numbers: Vec<u32> = builds.iter().map(|b| b.0).collect();
    assert_eq!(numbers, (1..=numbers.len() as u32).collect::<Vec<_>>());
    for (build, calls) in &builds {
        for (cmd, examples) in calls {
            assert!(!examples.is_empty(), "build {build}: {cmd} has an example");
            for sent in examples {
                assert_eq!(sent["cmd"], cmd.as_str(), "build {build}: {cmd}.json");
                let call: RemoteCall = serde_json::from_value(sent.clone())
                    .unwrap_or_else(|e| panic!("build {build}: {cmd} no longer parses: {e}"));
                assert!(
                    same(&serde_json::to_value(&call).unwrap(), sent),
                    "build {build}: {cmd} reads every field it sent"
                );
            }
        }
    }
}

#[test]
fn a_build_keeps_every_call_of_the_build_before() {
    let builds = builds();
    for pair in builds.windows(2) {
        let older: BTreeSet<&String> = pair[0].1.iter().map(|c| &c.0).collect();
        let newer: BTreeSet<&String> = pair[1].1.iter().map(|c| &c.0).collect();
        assert!(older.is_subset(&newer), "build {} drops a call", pair[1].0);
    }
}

#[test]
fn the_newest_build_has_an_example_of_every_call() {
    let bindings = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/engine/bindings/RemoteCall.ts"),
    )
    .unwrap();
    let every: BTreeSet<String> = bindings
        .split("\"cmd\": \"")
        .skip(1)
        .map(|rest| rest.split('"').next().unwrap().to_string())
        .collect();
    let newest: BTreeSet<String> = builds()
        .last()
        .unwrap()
        .1
        .iter()
        .map(|c| c.0.clone())
        .collect();
    assert_eq!(newest, every);
}

#[test]
fn a_computer_from_before_the_window_speaks_its_oldest_build_only() {
    let old = r#"{"computerName":"Kitchen laptop","deviceName":"Pixel","connection":"simple",
        "version":"0.7.2","minPhoneBuild":1}"#;
    let hello: PhoneHello = serde_json::from_str(old).unwrap();
    assert_eq!(hello.max_phone_build, None);
    assert_eq!(hello.newest_phone_build(), 1);
    let now = PhoneHello {
        max_phone_build: Some(3),
        min_phone_build: 2,
        ..hello
    };
    assert_eq!(now.newest_phone_build(), 3);
    assert_eq!(serde_json::to_value(&now).unwrap()["maxPhoneBuild"], 3);
}

#[test]
fn an_introduction_and_its_answers_are_tagged() {
    let call = RemoteCall::Introduce {
        app: PhoneAppIntro {
            platform: AppPlatform::Ios,
            version: "0.9.0".into(),
            min_build: 1,
            max_build: 2,
        },
    };
    assert_eq!(
        serde_json::to_string(&call).unwrap(),
        r#"{"cmd":"introduce","app":{"platform":"ios","version":"0.9.0","minBuild":1,"maxBuild":2}}"#
    );
    assert_eq!(
        serde_json::to_string(&RemoteAnswer::Introduced { agreed: 2 }).unwrap(),
        r#"{"type":"introduced","agreed":2}"#
    );
    for (code, text) in [
        (PhoneError::TooOld, "tooOld"),
        (PhoneError::ComputerOutdated, "computerOutdated"),
    ] {
        assert_eq!(
            serde_json::to_value(RemoteAnswer::Error { code }).unwrap()["code"],
            text
        );
    }
}
