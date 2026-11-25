//! Changing the model for a job in the app, against the fake engine: consent before a download,
//! the current model working until the new one is on disk, pause, cancel, failure, a restart,
//! the ledger, the old model, and what Delete everything and an export do with the choice.

mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use common::*;
use nocturne_app_lib::models::{self, HubOptions};
use nocturne_app_lib::runtime::{Services, StartOptions};
use nocturne_contract::{ComponentRole, LedgerKind, Tier};
use nocturne_intelligence::FakeHost;
use nocturne_intelligence::reindex::ReindexOptions;
use nocturne_lemonade::jobs::JobOptions;
use nocturne_lemonade::record::{self, SetupRecord};
use nocturne_lemonade::selection::{self, OldModel, Pending, Selection};
use nocturne_lemonade::{Dirs, SetupOptions, SupervisorOptions, tier};
use serde_json::{Value, json};
use tauri::Manager;
use tauri::async_runtime::block_on;

const GIB: u64 = 1 << 30;

/// The fake engine binary, built once per run (see `tests/lemonade.rs`).
fn fake_engine() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            if let Some(path) = std::env::var_os("NOCTURNE_FAKE_LEMOND") {
                return PathBuf::from(path);
            }
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
            let mut build = std::process::Command::new(cargo);
            build
                .args(["build", "--locked", "-p", "nocturne-lemonade"])
                .args(["--features", "fake-server", "--bin", "fake-lemond"])
                .args([
                    "--profile",
                    if cfg!(debug_assertions) {
                        "dev"
                    } else {
                        "release"
                    },
                ])
                .arg("--message-format=json-render-diagnostics");
            let out = build
                .stderr(std::process::Stdio::inherit())
                .output()
                .expect("cargo runs");
            assert!(out.status.success(), "the fake engine is built");
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .filter(|m| {
                    m["reason"] == "compiler-artifact" && m["target"]["name"] == "fake-lemond"
                })
                .find_map(|m| m["executable"].as_str().map(PathBuf::from))
                .expect("cargo says where the fake engine is")
        })
        .clone()
}

struct Lab {
    h: Harness,
    services: Arc<Services>,
    config: PathBuf,
    cache: PathBuf,
    _bundle: tempfile::TempDir,
}

impl Drop for Lab {
    fn drop(&mut self) {
        self.h.state().run_exit_hooks();
    }
}

/// Standard installed, its models on disk plus `extra`; `mode` for the fake engine; `before`
/// may write a selection before the launch.
fn lab(mode: &str, extra: &[&str], before: impl FnOnce(&Path)) -> Lab {
    lab_with(mode, extra, before, Duration::from_millis(5))
}

/// A lab whose re-read waits `pause` between chunks, to catch it under way.
fn lab_with(mode: &str, extra: &[&str], before: impl FnOnce(&Path), pause: Duration) -> Lab {
    let h = harness();
    let bundle = tempfile::tempdir().unwrap();
    let link = bundle.path().join(if cfg!(windows) {
        "lemond.exe"
    } else {
        "lemond"
    });
    #[cfg(unix)]
    std::os::unix::fs::symlink(fake_engine(), &link).unwrap();
    #[cfg(not(unix))]
    std::fs::copy(fake_engine(), &link).unwrap();
    let root = h.dir.path().join("engine");
    let dirs = Dirs::new(bundle.path(), &root);
    dirs.ensure().unwrap();
    std::fs::write(
        dirs.config().join("config.json"),
        json!({ "fake_mode": mode }).to_string(),
    )
    .unwrap();
    record::write(
        &dirs.config(),
        &SetupRecord {
            version: 1,
            tier: Some(Tier::Standard),
            done: true,
            completed: Some(Tier::Standard),
            backends: Default::default(),
        },
    )
    .unwrap();
    let mut on_disk: Vec<String> = tier::components(Tier::Standard)
        .into_iter()
        .map(|c| c.model.to_string())
        .collect();
    on_disk.extend(extra.iter().map(|s| s.to_string()));
    std::fs::write(
        dirs.cache().join("fake-state.json"),
        json!({
            "downloaded": on_disk,
            "backends": ["llamacpp:vulkan", "whispercpp:vulkan", "kokoro:cpu"],
            "partial": {},
        })
        .to_string(),
    )
    .unwrap();
    before(&dirs.config());
    h.app.manage(HubOptions {
        jobs: JobOptions {
            poll: Duration::from_millis(20),
            retry_delay: Duration::from_millis(50),
            give_up: Duration::from_secs(10),
        },
        reindex: ReindexOptions {
            chunk: 2,
            pause,
            retry: Duration::from_millis(50),
            embed_limit: Duration::from_secs(10),
        },
        ram_bytes: Some(32 * GIB),
    });
    models::install(h.app.handle()).unwrap();
    // These tests keep and read notes by hand. On battery the reader leaves new notes alone, so
    // it cannot read one a test is applying and take back the memories the test made.
    let host = FakeHost::new();
    host.on_battery(100);
    let mut opts = StartOptions::with_engine(host, bundle.path());
    opts.supervisor = SupervisorOptions {
        health_interval: Duration::from_millis(200),
        start_timeout: Duration::from_secs(20),
        backoff_base: Duration::from_millis(20),
        ..SupervisorOptions::default()
    };
    opts.setup = SetupOptions {
        poll: Duration::from_millis(50),
        ram_bytes: Some(32 * GIB),
        ..SetupOptions::default()
    };
    let services = start(&h, opts);
    let lab = Lab {
        h,
        services,
        config: dirs.config(),
        cache: dirs.cache(),
        _bundle: bundle,
    };
    wait_for("the mind to arrive", || lab.h.state().mind.get().is_some());
    lab
}

impl Lab {
    fn view(&self) -> Value {
        self.h.ok("get_models", json!({}))
    }

    fn job_model(&self, job: &str) -> String {
        self.view()["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["job"] == job)
            .map(|j| j["model"].as_str().unwrap().to_string())
            .unwrap()
    }

    fn change(&self, job: &str, model: &str, allow: bool) -> Result<Value, Value> {
        self.h.call(
            "change_model",
            json!({"job": job, "model": model, "allowDownload": allow}),
        )
    }

    fn selection(&self) -> Selection {
        selection::read(&self.config)
    }

    fn requests(&self, path: &str) -> Vec<Value> {
        block_on(async {
            let mut last = String::new();
            for _ in 0..50 {
                if let Some(client) = self.services.client().await
                    && let Some(endpoint) = client.endpoint()
                {
                    let sent = nocturne_lemonade::tls::client()
                        .get(format!("http://127.0.0.1:{}/fake/requests", endpoint.port))
                        .bearer_auth(&endpoint.key)
                        .send()
                        .await;
                    // A relaunch under way ends the connection, at its start or part way.
                    match sent {
                        Ok(r) => match r.json::<Vec<Value>>().await {
                            Ok(log) => return log,
                            Err(e) => last = e.to_string(),
                        },
                        Err(e) => last = e.to_string(),
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            panic!("the request log could not be read: {last}")
        })
        .into_iter()
        .filter(|r| r["authorised"] == true && r["path"] == path)
        .collect()
    }

    fn downloads_in_ledger(&self) -> Vec<String> {
        self.h
            .engine()
            .ledger()
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == LedgerKind::ModelDownload)
            .map(|e| e.label)
            .collect()
    }

    /// What the switch to a downloaded model waits on, for a wait that timed out: the change as
    /// the person would see it, the choice on disk, the ledger, and the engine's window.
    fn switch_state(&self) -> String {
        let (window, holds, starting) = block_on(async {
            match self.services.engine_control().await {
                Some(c) => (
                    json!(c.download_window_open()),
                    json!(c.download_holds()),
                    json!(c.starting()),
                ),
                None => (json!("no engine"), Value::Null, Value::Null),
            }
        });
        let offline = block_on(async {
            let Some(admin) = self.services.admin().await else {
                return json!("no engine");
            };
            match tokio::time::timeout(Duration::from_secs(3), admin.config()).await {
                Ok(Ok(cfg)) => cfg["offline"].clone(),
                Ok(Err(e)) => json!(format!("config failed: {e}")),
                Err(_) => json!("config timed out"),
            }
        });
        json!({
            "job": self.job_model("recordings"),
            "change": self.view()["change"],
            "selection": self.selection(),
            "ledger": self.downloads_in_ledger(),
            "window": { "open": window, "holds": holds, "engineStarting": starting },
            "engineOffline": offline,
        })
        .to_string()
    }

    fn offline(&self) -> bool {
        block_on(async {
            let admin = self.services.admin().await.expect("an engine");
            admin.config().await.unwrap()["offline"] == true
        })
    }

    fn partial(&self, model: &str) -> u64 {
        let state: Value =
            serde_json::from_slice(&std::fs::read(self.cache.join("fake-state.json")).unwrap())
                .unwrap();
        state["partial"][model].as_u64().unwrap_or(0)
    }

    fn phase(&self) -> Option<String> {
        self.view()["change"]["phase"]["type"]
            .as_str()
            .map(str::to_string)
    }
}

#[test]
fn download_needs_allow() {
    let lab = lab("download-ms=400", &[], |_| {});
    let refused = lab.change("recordings", "Whisper-Base", false).unwrap_err();
    assert_eq!(refused["code"], "invalid");
    assert!(lab.requests("/v1/pull").is_empty(), "nothing was pulled");
    assert!(lab.selection().pending.is_none());
    assert_eq!(lab.job_model("recordings"), "Whisper-Small");
}

#[test]
fn catalogue_only_pull_refused_for_user_ids() {
    let lab = lab("download-ms=400", &[], |_| {});
    for job in ["answering", "recordings", "finding"] {
        let refused = lab.change(job, "user.Qwen3.5-9B-GGUF", true).unwrap_err();
        assert_eq!(refused["code"], "invalid", "{job}");
    }
    let refused = lab
        .h
        .call(
            "preview_model_change",
            json!({"job": "answering", "model": "user.Qwen3.5-9B-GGUF"}),
        )
        .unwrap_err();
    assert_eq!(refused["code"], "invalid");
    // A model that is not for the job is refused too.
    let refused = lab.change("answering", "Whisper-Base", true).unwrap_err();
    assert_eq!(refused["code"], "invalid");
    assert!(lab.requests("/v1/pull").is_empty());
}

#[test]
fn current_keeps_working_until_present_then_switches_with_one_ledger_entry() {
    let lab = lab("download-ms=1500", &[], |_| {});
    let view = lab.change("recordings", "Whisper-Base", true).unwrap();
    assert_eq!(view["change"]["model"], "Whisper-Base");
    assert_eq!(view["change"]["from"], "Whisper-Small");
    // While it downloads, the current model is the one in use.
    assert_eq!(lab.job_model("recordings"), "Whisper-Small");
    assert_eq!(selection::picks(&lab.selection()).speech, "Whisper-Small");
    assert_eq!(lab.selection().pending.unwrap().model, "Whisper-Base");
    wait_for_with(
        "the switch",
        || lab.job_model("recordings") == "Whisper-Base",
        || lab.switch_state(),
    );
    let s = lab.selection();
    assert!(s.pending.is_none());
    assert_eq!(
        s.custom.get(&ComponentRole::Speech).unwrap(),
        "Whisper-Base"
    );
    wait_for("the ledger", || !lab.downloads_in_ledger().is_empty());
    assert_eq!(lab.downloads_in_ledger(), ["Whisper-Base"]);
    // The old model is kept by default and offered for removal.
    let view = lab.view();
    assert_eq!(view["old"]["model"], "Whisper-Small");
    assert_eq!(view["old"]["removed"], false);
    assert!(view["change"].is_null());
    assert_eq!(view["custom"], true);
}

#[test]
fn no_download_change_switches_at_once() {
    let lab = lab("download-ms=400", &["Whisper-Tiny"], |_| {});
    let view = lab.change("recordings", "Whisper-Tiny", false).unwrap();
    assert!(view["change"].is_null());
    assert_eq!(lab.job_model("recordings"), "Whisper-Tiny");
    assert!(lab.requests("/v1/pull").is_empty());
    assert!(lab.downloads_in_ledger().is_empty());
    // Keeping the old model just closes the offer.
    let kept = lab.h.ok("keep_old_model", json!({}));
    assert!(kept["old"].is_null());
    assert!(lab.selection().old.is_none());
    // Reset brings the tier's own back.
    let reset = lab.h.ok("reset_models_to_tier", json!({}));
    assert_eq!(reset["custom"], false);
    assert_eq!(lab.job_model("recordings"), "Whisper-Small");
}

#[test]
fn old_model_kept_by_default_and_remove_asks_the_engine() {
    let lab = lab("download-ms=400", &["Whisper-Tiny"], |_| {});
    lab.change("recordings", "Whisper-Tiny", false).unwrap();
    assert_eq!(lab.selection().old.unwrap().model, "Whisper-Small");
    // A model in use is never removed.
    lab.change("recordings", "Whisper-Small", false).unwrap();
    let mut s = lab.selection();
    s.old = Some(selection::OldModel {
        role: ComponentRole::Speech,
        model: "Whisper-Small".into(),
    });
    selection::write(&lab.config, &s).unwrap();
    assert_eq!(lab.h.code("remove_old_model", json!({})), "invalid");
    assert!(lab.requests("/v1/delete").is_empty());
    lab.change("recordings", "Whisper-Tiny", false).unwrap();
    let view = lab.h.ok("remove_old_model", json!({}));
    assert_eq!(view["old"]["model"], "Whisper-Small");
    assert_eq!(view["old"]["removed"], true);
    assert_eq!(view["old"]["bytes"], 488_000_000.0);
    assert!(lab.selection().old.is_none());
    let deletes = lab.requests("/v1/delete");
    assert_eq!(deletes.len(), 1, "{deletes:?}");
    assert_eq!(deletes[0]["body"]["model_name"], "Whisper-Small");
    let present = block_on(async {
        let client = lab.services.client().await.unwrap();
        client.models().await.unwrap()
    })
    .into_iter()
    .any(|m| m.id == "Whisper-Small" && m.present());
    assert!(!present, "the old model's files are gone");
    // Going back to it now needs a download, and the person's yes.
    assert_eq!(
        lab.change("recordings", "Whisper-Small", false)
            .unwrap_err()["code"],
        "invalid"
    );
}

#[test]
fn pause_resume_cancel() {
    let lab = lab("download-ms=6000", &[], |_| {});
    lab.change("recordings", "Whisper-Base", true).unwrap();
    wait_for("bytes to arrive", || {
        lab.view()["change"]["phase"]["received"]
            .as_f64()
            .is_some_and(|b| b > 0.0)
    });
    assert!(!lab.offline(), "the download window is open");
    let paused = lab.h.ok("pause_model_download", json!({}));
    assert_eq!(paused["change"]["phase"]["paused"], true);
    assert!(lab.selection().pending.unwrap().paused);
    wait_for("the window to close", || lab.offline());
    let resumed = lab.h.ok("resume_model_download", json!({}));
    assert_eq!(resumed["change"]["phase"]["paused"], false);
    assert!(!lab.selection().pending.unwrap().paused);
    assert!(!lab.offline());
    let cancelled = lab.h.ok("cancel_model_change", json!({}));
    assert!(cancelled["change"].is_null());
    assert!(lab.selection().pending.is_none());
    assert_eq!(lab.job_model("recordings"), "Whisper-Small");
    assert!(lab.downloads_in_ledger().is_empty());
    wait_for("the window to close", || lab.offline());
    assert_eq!(
        lab.partial("Whisper-Base"),
        0,
        "the partial download is deleted"
    );
}

#[test]
fn resume_after_restart() {
    let lab = lab("download-ms=600", &[], |config| {
        let mut s = Selection::preset(Tier::Standard);
        s.pending = Some(Pending {
            role: ComponentRole::Speech,
            model: "Whisper-Base".into(),
            from: "Whisper-Small".into(),
            paused: false,
            failed: false,
        });
        selection::write(config, &s).unwrap();
    });
    wait_for("the interrupted change to finish", || {
        lab.job_model("recordings") == "Whisper-Base"
    });
    assert!(lab.selection().pending.is_none());
    assert_eq!(lab.downloads_in_ledger(), ["Whisper-Base"]);
}

#[test]
fn a_paused_change_waits_after_restart() {
    let lab = lab("download-ms=600", &[], |config| {
        let mut s = Selection::preset(Tier::Standard);
        s.pending = Some(Pending {
            role: ComponentRole::Speech,
            model: "Whisper-Base".into(),
            from: "Whisper-Small".into(),
            paused: true,
            failed: false,
        });
        selection::write(config, &s).unwrap();
    });
    std::thread::sleep(Duration::from_millis(500));
    let view = lab.view();
    assert_eq!(view["change"]["resumed"], true);
    assert_eq!(view["change"]["phase"]["paused"], true);
    assert!(
        lab.requests("/v1/pull").is_empty(),
        "nothing downloads by itself"
    );
    lab.h.ok("resume_model_download", json!({}));
    wait_for("the change", || {
        lab.job_model("recordings") == "Whisper-Base"
    });
}

#[test]
fn failure_keeps_partial_and_current() {
    let lab = lab("download-ms=300,disk-full", &[], |_| {});
    lab.change("recordings", "Whisper-Base", true).unwrap();
    wait_for("the failure", || lab.phase().as_deref() == Some("failed"));
    let view = lab.view();
    assert_eq!(view["attention"], true);
    assert!(view["change"]["phase"]["received"].as_f64().unwrap() > 0.0);
    assert_eq!(lab.job_model("recordings"), "Whisper-Small");
    let status = lab.h.ok("get_status", json!({}));
    assert!(
        status["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["code"] == "modelDownloadStopped"),
        "{status}"
    );
    assert!(lab.requests("/v1/delete").is_empty(), "the partial is kept");
    // Cancelling then clears it and the notice.
    lab.h.ok("cancel_model_change", json!({}));
    let status = lab.h.ok("get_status", json!({}));
    assert!(
        !status["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["code"] == "modelDownloadStopped")
    );
}

#[test]
fn quality_warning_when_smaller() {
    let lab = lab("download-ms=400", &[], |_| {});
    let p = lab.h.ok(
        "preview_model_change",
        json!({"job": "answering", "model": "Qwen3.5-2B-GGUF"}),
    );
    assert_eq!(p["smaller"], true);
    assert_eq!(p["tested"], true);
    assert_eq!(p["from"], "Qwen3.5-9B-GGUF");
    assert_eq!(p["downloadBytes"], 2_010_000_000.0);
    assert!(p["memoryAfter"].as_f64().unwrap() < p["memoryNow"].as_f64().unwrap());
    assert!(p["reindexItems"].is_null());
    let p = lab.h.ok(
        "preview_model_change",
        json!({"job": "recordings", "model": "Whisper-Large-v3-Turbo"}),
    );
    assert_eq!(p["smaller"], false);
    assert_eq!(p["newOnly"], true);
    let p = lab.h.ok(
        "preview_model_change",
        json!({"job": "finding", "model": "Qwen3-Embedding-0.6B-GGUF"}),
    );
    assert_eq!(p["reindexItems"], 0);
    assert_eq!(p["downloadBytes"], 0.0);
    // While memories are read again either finding model may be loaded beside the others,
    // never both (the engine keeps one per kind): the larger set counts.
    let p = lab.h.ok(
        "preview_model_change",
        json!({"job": "finding", "model": "nomic-embed-text-v1-GGUF"}),
    );
    assert_eq!(p["memoryAfter"], p["memoryNow"]);
    assert_eq!(p["smaller"], true);
    let p = lab.h.ok(
        "preview_model_change",
        json!({"job": "finding", "model": "Qwen3-Embedding-4B-GGUF"}),
    );
    assert!(p["memoryAfter"].as_f64().unwrap() > p["memoryNow"].as_f64().unwrap());
    assert_eq!(p["tested"], false);
    let thinking = lab
        .h
        .call(
            "preview_model_change",
            json!({"job": "answering", "model": "MiniCPM-V-4.6-Thinking-GGUF"}),
        )
        .unwrap_err();
    assert_eq!(thinking["code"], "invalid");
    let choices = lab.h.ok("list_model_choices", json!({"job": "answering"}));
    let list = choices.as_array().unwrap();
    assert_eq!(list[0]["id"], "Qwen3.5-9B-GGUF");
    assert_eq!(list[0]["current"], true);
    assert!(
        list.iter()
            .all(|c| !c["id"].as_str().unwrap().starts_with("user."))
    );
}

#[test]
fn selection_exported_and_cleared() {
    let lab = lab("download-ms=400", &["Whisper-Tiny"], |_| {});
    lab.change("recordings", "Whisper-Tiny", false).unwrap();
    let out = tempfile::tempdir().unwrap();
    let report = lab
        .h
        .ok("export_to", json!({"dir": out.path().to_str().unwrap()}));
    let exported: Selection = serde_json::from_slice(
        &std::fs::read(Path::new(report["path"].as_str().unwrap()).join("models.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        exported.custom.get(&ComponentRole::Speech).unwrap(),
        "Whisper-Tiny"
    );
    lab.h.ok(
        "delete_everything",
        json!({"confirmPhrase": "delete everything"}),
    );
    assert!(!lab.config.join("selection.json").exists());
    assert_eq!(lab.view()["custom"], false);
    assert_eq!(lab.job_model("recordings"), "Whisper-Small");
}

#[test]
fn the_view_names_each_job_and_the_memory_meter() {
    let lab = lab("download-ms=400", &[], |_| {});
    let view = lab.view();
    assert_eq!(view["preset"], "standard");
    assert_eq!(view["jobs"].as_array().unwrap().len(), 4);
    assert_eq!(view["ramBytes"], (32 * GIB) as f64);
    let budget = view["budgetBytes"].as_f64().unwrap();
    let busiest = view["busiestBytes"].as_f64().unwrap();
    assert!(busiest > 0.0 && busiest <= budget, "{view}");
    let answering = &view["jobs"][0];
    assert_eq!(answering["job"], "answering");
    assert_eq!(answering["name"], "Qwen3.5-9B");
    assert_eq!(answering["recommended"], true);
}

#[test]
fn a_new_finding_model_is_fetched_read_with_and_swapped_in() {
    let lab = lab("download-ms=600", &[], |_| {});
    for i in 0..5 {
        lab.h.learn(
            &format!("note {i}"),
            &format!("Sam likes thing {i}."),
            "Sam",
            &format!("thing {i}"),
        );
    }
    let preview = || {
        lab.h.ok(
            "preview_model_change",
            json!({"job": "finding", "model": "Qwen3-Embedding-4B-GGUF"}),
        )
    };
    wait_for("all five to be counted", || preview()["reindexItems"] == 5);
    let p = preview();
    assert_eq!(p["reindexItems"], 5);
    assert!(p["reindexEtaSeconds"].as_u64().is_some());
    let view = lab
        .change("finding", "Qwen3-Embedding-4B-GGUF", true)
        .unwrap();
    assert_eq!(view["change"]["model"], "Qwen3-Embedding-4B-GGUF");
    // Until the re-read is done, memories are found with the old model.
    assert_eq!(lab.job_model("finding"), "Qwen3-Embedding-0.6B-GGUF");
    wait_for("the re-read to finish", || {
        lab.view()["reindex"]["phase"] == "done"
    });
    let view = lab.view();
    assert_eq!(view["reindex"]["total"], 5);
    assert_eq!(view["reindex"]["done"], 5);
    assert_eq!(view["reindex"]["from"], "Qwen3-Embedding-0.6B-GGUF");
    assert_eq!(lab.job_model("finding"), "Qwen3-Embedding-4B-GGUF");
    assert_eq!(view["old"]["model"], "Qwen3-Embedding-0.6B-GGUF");
    let engine = lab.h.engine();
    assert_eq!(engine.recall_model(), "Qwen3-Embedding-4B-GGUF");
    assert_eq!(
        engine.indexed_model().unwrap().as_deref(),
        Some("Qwen3-Embedding-4B-GGUF")
    );
    assert_eq!(
        lab.selection().custom.get(&ComponentRole::Recall).unwrap(),
        "Qwen3-Embedding-4B-GGUF"
    );
    assert_eq!(lab.downloads_in_ledger(), ["Qwen3-Embedding-4B-GGUF"]);
    // The worker read with the new model.
    wait_for("the re-read to be logged with the new model", || {
        lab.requests("/v1/embeddings")
            .iter()
            .any(|r| r["body"]["model"] == "Qwen3-Embedding-4B-GGUF")
    });
}

#[test]
fn a_re_read_can_be_cancelled_and_the_old_way_stays() {
    let lab = lab("download-ms=300", &["Qwen3-Embedding-4B-GGUF"], |_| {});
    for i in 0..40 {
        lab.h.learn(
            &format!("note {i}"),
            &format!("Sam likes thing {i}."),
            "Sam",
            &format!("thing {i}"),
        );
    }
    lab.change("finding", "Qwen3-Embedding-4B-GGUF", false)
        .unwrap();
    let view = lab.h.ok("cancel_reindex", json!({}));
    assert!(view["reindex"]["phase"] != "running", "{view}");
    wait_for("the worker to stop", || {
        lab.view()["reindex"]["phase"] == "cancelled"
    });
    assert_eq!(lab.job_model("finding"), "Qwen3-Embedding-0.6B-GGUF");
    let engine = lab.h.engine();
    assert_eq!(engine.recall_model(), "Qwen3-Embedding-0.6B-GGUF");
    assert_eq!(
        engine.reindex_state().unwrap().unwrap().phase,
        nocturne_contract::ReindexPhase::Cancelled
    );
    assert!(!lab.selection().custom.contains_key(&ComponentRole::Recall));
}

#[test]
fn two_changes_at_once_are_refused() {
    let lab = lab("download-ms=6000", &["Whisper-Tiny"], |_| {});
    lab.change("recordings", "Whisper-Base", true).unwrap();
    for (job, model, allow) in [
        ("answering", "Qwen3.5-2B-GGUF", true),
        ("recordings", "Whisper-Tiny", false),
        ("finding", "nomic-embed-text-v1-GGUF", true),
    ] {
        let refused = lab.change(job, model, allow).unwrap_err();
        assert_eq!(refused["code"], "busy", "{job}");
    }
    assert_eq!(lab.selection().pending.unwrap().model, "Whisper-Base");
    let pulls: Vec<Value> = lab.requests("/v1/pull");
    assert!(
        pulls
            .iter()
            .all(|p| p["body"]["model_name"] == "Whisper-Base"),
        "{pulls:?}"
    );
    lab.h.ok("cancel_model_change", json!({}));
}

#[test]
fn a_change_during_setup_is_refused() {
    let lab = lab("download-ms=6000", &["Whisper-Tiny"], |_| {});
    lab.h.ok("start_setup", json!({"tier": "small"}));
    wait_for("setup to download", || {
        lab.h.ok("get_setup", json!({}))["phase"] == "downloading"
    });
    for (job, model, allow) in [
        ("recordings", "Whisper-Tiny", false),
        ("recordings", "Whisper-Base", true),
    ] {
        let refused = lab.change(job, model, allow).unwrap_err();
        assert_eq!(refused["code"], "busy", "{model}");
    }
    assert!(lab.selection().pending.is_none());
    assert_eq!(lab.job_model("recordings"), "Whisper-Small");
}

#[test]
fn a_failed_change_waits_after_restart() {
    let lab = lab("download-ms=600", &[], |config| {
        let mut s = Selection::preset(Tier::Standard);
        s.pending = Some(Pending {
            role: ComponentRole::Speech,
            model: "Whisper-Base".into(),
            from: "Whisper-Small".into(),
            paused: false,
            failed: true,
        });
        selection::write(config, &s).unwrap();
    });
    std::thread::sleep(Duration::from_millis(500));
    let view = lab.view();
    assert_eq!(view["change"]["phase"]["type"], "failed");
    assert_eq!(view["attention"], true);
    assert!(
        lab.requests("/v1/pull").is_empty(),
        "nothing downloads by itself"
    );
    // Trying again is the person's yes.
    lab.h.ok("resume_model_download", json!({}));
    wait_for("the change", || {
        lab.job_model("recordings") == "Whisper-Base"
    });
}

fn learn_many(lab: &Lab, n: usize) {
    for i in 0..n {
        lab.h.learn(
            &format!("note {i}"),
            &format!("Sam likes thing {i}."),
            "Sam",
            &format!("thing {i}"),
        );
    }
}

#[test]
fn two_finding_changes_at_once_leave_one_re_read_and_no_stray_vectors() {
    let lab = lab_with(
        "download-ms=400",
        &["Qwen3-Embedding-4B-GGUF", "nomic-embed-text-v1-GGUF"],
        |_| {},
        Duration::from_millis(100),
    );
    learn_many(&lab, 10);
    let hub = models::hub(lab.h.app.handle()).unwrap();
    let app = lab.h.app.handle().clone();
    let (a, b) = block_on(async {
        tokio::join!(
            hub.change(
                &app,
                nocturne_contract::Job::Finding,
                "Qwen3-Embedding-4B-GGUF",
                false
            ),
            hub.change(
                &app,
                nocturne_contract::Job::Finding,
                "nomic-embed-text-v1-GGUF",
                false
            ),
        )
    });
    assert!(a.is_ok() != b.is_ok(), "exactly one change: {a:?} {b:?}");
    let winner = if a.is_ok() {
        "Qwen3-Embedding-4B-GGUF"
    } else {
        "nomic-embed-text-v1-GGUF"
    };
    let engine = lab.h.engine();
    assert_eq!(engine.reindex_state().unwrap().unwrap().model, winner);
    wait_for("the re-read", || lab.view()["reindex"]["phase"] == "done");
    assert_eq!(engine.indexed_model().unwrap().as_deref(), Some(winner));
    assert_eq!(engine.recall_model(), winner);
}

#[test]
fn an_old_model_is_never_removed_while_a_re_read_uses_it() {
    let lab = lab_with(
        "download-ms=400",
        &["Qwen3-Embedding-4B-GGUF"],
        |config| {
            let mut s = Selection::preset(Tier::Standard);
            s.old = Some(OldModel {
                role: ComponentRole::Recall,
                model: "Qwen3-Embedding-4B-GGUF".into(),
            });
            selection::write(config, &s).unwrap();
        },
        Duration::from_millis(300),
    );
    learn_many(&lab, 20);
    lab.change("finding", "Qwen3-Embedding-4B-GGUF", false)
        .unwrap();
    assert_eq!(lab.view()["reindex"]["phase"], "running");
    assert_eq!(lab.h.code("remove_old_model", json!({})), "invalid");
    assert!(lab.requests("/v1/delete").is_empty());
    lab.h.ok("cancel_reindex", json!({}));
}

#[test]
fn a_change_and_removing_its_model_never_both_go_ahead() {
    let lab = lab("download-ms=400", &["Whisper-Tiny"], |_| {});
    lab.change("recordings", "Whisper-Tiny", false).unwrap();
    // The old model is Whisper-Small: remove it and switch back to it at the same moment.
    let hub = models::hub(lab.h.app.handle()).unwrap();
    let app = lab.h.app.handle().clone();
    let (removed, changed) = block_on(async {
        tokio::join!(
            hub.remove_old(&app),
            hub.change(
                &app,
                nocturne_contract::Job::Recordings,
                "Whisper-Small",
                false
            ),
        )
    });
    let present = block_on(async { lab.services.client().await.unwrap().models().await.unwrap() })
        .into_iter()
        .any(|m| m.id == "Whisper-Small" && m.present());
    if changed.is_ok() {
        assert_eq!(lab.job_model("recordings"), "Whisper-Small");
        assert!(present, "the model in use was never deleted");
    } else {
        assert!(removed.is_ok());
        assert!(!present);
        assert_eq!(lab.job_model("recordings"), "Whisper-Tiny");
    }
}

#[test]
fn a_download_that_never_starts_leaves_nothing_pending() {
    let lab = lab("download-ms=400", &[], |_| {});
    // The next launch, the one that opens the download window, cannot start.
    let path = lab.config.join("config.json");
    let mut cfg: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    cfg["fake_mode"] = json!("fail-bind");
    std::fs::write(&path, cfg.to_string()).unwrap();
    let refused = lab.change("recordings", "Whisper-Base", true).unwrap_err();
    assert_ne!(refused["code"], "busy", "{refused}");
    assert!(lab.selection().pending.is_none(), "rolled back");
    assert!(lab.view()["change"].is_null());
}

#[test]
fn setup_waits_while_a_model_downloads() {
    let lab = lab("download-ms=6000", &[], |_| {});
    lab.change("recordings", "Whisper-Base", true).unwrap();
    assert_eq!(lab.h.code("start_setup", json!({"tier": "small"})), "busy");
    lab.h.ok("pause_model_download", json!({}));
    lab.h.ok("cancel_model_change", json!({}));
}

#[test]
fn the_store_names_its_finding_model_from_the_start() {
    let lab = lab("download-ms=400", &[], |_| {});
    let engine = lab.h.engine();
    learn_many(&lab, 1);
    let id = engine.memories(Default::default()).unwrap()[0].id.clone();
    engine.set_recall_model("nomic-embed-text-v1-GGUF");
    engine.store_embeddings(vec![(id, vec![1.0, 0.0])]).unwrap();
    engine.set_recall_model(nocturne_engine::DEFAULT_RECALL_MODEL);
    models::install(lab.h.app.handle()).unwrap();
    assert_eq!(engine.recall_model(), "nomic-embed-text-v1-GGUF");
}
