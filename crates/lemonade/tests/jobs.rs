//! Model downloads for a changed job, against the fake engine: the shared download window,
//! pausing, cancelling, failing, the ledger, and removing a model the person no longer wants.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nocturne_contract::{ComponentRole, Tier};
use nocturne_lemonade::jobs::{JobOptions, JobProgress, ModelJobs, OnDownload};
use nocturne_lemonade::{Client, Setup, SetupOptions, Supervisor, SupervisorOptions, tier};
use serde_json::Value;
use tokio::sync::watch;

type Ledger = Arc<Mutex<Vec<(String, u64)>>>;

struct Rig {
    sup: Supervisor,
    jobs: ModelJobs,
    ledger: Ledger,
    _tmp: tempfile::TempDir,
}

fn opts() -> JobOptions {
    JobOptions {
        poll: Duration::from_millis(20),
        retry_delay: Duration::from_millis(50),
        give_up: Duration::from_secs(10),
    }
}

async fn rig(mode: &str) -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    common::set_mode(&dirs, mode);
    let sup = Supervisor::start(
        dirs,
        SupervisorOptions {
            health_interval: Duration::from_millis(200),
            backoff_base: Duration::from_millis(100),
            ..SupervisorOptions::default()
        },
    )
    .await
    .unwrap();
    let jobs = ModelJobs::with_options(sup.control(), opts());
    Rig {
        sup,
        jobs,
        ledger: Ledger::default(),
        _tmp: tmp,
    }
}

impl Rig {
    fn sink(&self) -> OnDownload {
        let ledger = self.ledger.clone();
        Arc::new(move |m: &str, b: u64| ledger.lock().unwrap().push((m.to_string(), b)))
    }

    async fn get(&self, model: &str) -> watch::Receiver<JobProgress> {
        let hold = self.sup.control().hold_online().await.unwrap();
        self.jobs.get(model, hold, self.sink()).await.unwrap()
    }

    async fn offline(&self) -> bool {
        let cfg = self.sup.admin().config().await.unwrap();
        cfg["offline"] == true
    }

    async fn wait_offline(&self, want: bool) {
        for _ in 0..200 {
            if self.sup.admin().config().await.is_ok() && self.offline().await == want {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the engine never became offline={want}");
    }

    async fn present(&self, model: &str) -> bool {
        self.sup
            .client()
            .models()
            .await
            .unwrap()
            .iter()
            .any(|m| m.id == model && m.present())
    }

    async fn requests(&self, path: &str) -> Vec<Value> {
        let e = self.sup.client().endpoint().unwrap();
        let all: Value = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{}/fake/requests", e.port))
            .bearer_auth(e.key)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        all.as_array()
            .unwrap()
            .iter()
            .filter(|r| r["path"] == path)
            .cloned()
            .collect()
    }
}

async fn until(
    rx: &mut watch::Receiver<JobProgress>,
    want: impl Fn(&JobProgress) -> bool,
) -> JobProgress {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let p = rx.borrow_and_update().clone();
            if want(&p) {
                return p;
            }
            if rx.changed().await.is_err() {
                return rx.borrow().clone();
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("progress never matched; last {:?}", rx.borrow().clone()))
}

#[tokio::test]
async fn a_download_finishes_once_into_the_ledger_and_closes_the_window() {
    let r = rig("download-ms=300").await;
    assert!(r.offline().await);
    let mut rx = r.get("Whisper-Tiny").await;
    assert!(!r.offline().await, "the download window is open");
    let done = until(&mut rx, |p| p.done).await;
    assert!(done.total > 0);
    assert_eq!(done.received, done.total);
    assert!(r.present("Whisper-Tiny").await);
    assert!(
        r.offline().await,
        "the window is closed before the end is announced"
    );
    let ledger = r.ledger.lock().unwrap().clone();
    assert_eq!(ledger, [("Whisper-Tiny".to_string(), done.total)]);
    assert_eq!(r.jobs.model(), None);
}

#[tokio::test]
async fn catalogue_only_pull_refused_for_user_ids() {
    let r = rig("download-ms=300").await;
    let hold = r.sup.control().hold_online().await.unwrap();
    assert!(
        r.jobs
            .get("user.Qwen3.5-9B-GGUF", hold, r.sink())
            .await
            .is_err()
    );
    r.wait_offline(true).await;
    assert!(
        r.requests("/v1/pull").await.is_empty(),
        "nothing was pulled"
    );
}

#[tokio::test]
async fn pause_resume_cancel() {
    let r = rig("download-ms=4000").await;
    let mut rx = r.get("Whisper-Small").await;
    until(&mut rx, |p| p.received > 0).await;
    r.jobs.pause().await.unwrap();
    assert!(rx.borrow().paused);
    // Paused, nothing may reach the internet for it.
    r.wait_offline(true).await;
    let held = rx.borrow().received;
    r.jobs.resume().await.unwrap();
    r.wait_offline(false).await;
    let p = until(&mut rx, |p| !p.paused && p.received > held).await;
    assert!(p.received >= held, "carried on from what was received");
    // Another model while one runs is refused.
    let hold = r.sup.control().hold_online().await.unwrap();
    assert!(r.jobs.get("Whisper-Base", hold, r.sink()).await.is_err());
    r.jobs.cancel().await.unwrap();
    r.wait_offline(true).await;
    assert!(!r.present("Whisper-Small").await);
    assert_eq!(r.jobs.model(), None);
    // The job is gone from the engine, and so are the bytes it had received.
    let jobs = r.sup.client().downloads().await.unwrap();
    assert!(
        jobs.iter().all(|j| j.id != "model:Whisper-Small"),
        "{jobs:?}"
    );
    let state: Value = serde_json::from_slice(
        &std::fs::read(r.sup.dirs().cache().join("fake-state.json")).unwrap(),
    )
    .unwrap();
    assert!(state["partial"].get("Whisper-Small").is_none(), "{state}");
    assert!(r.ledger.lock().unwrap().is_empty());
}

#[tokio::test]
async fn failure_keeps_partial_and_window_closes() {
    let r = rig("download-ms=300,disk-full").await;
    let mut rx = r.get("Whisper-Small").await;
    let p = until(&mut rx, |p| p.failed).await;
    assert!(p.received > 0, "what was received is kept: {p:?}");
    assert!(!r.present("Whisper-Small").await);
    r.wait_offline(true).await;
    assert!(
        r.requests("/v1/delete").await.is_empty(),
        "nothing is deleted"
    );
    assert!(r.ledger.lock().unwrap().is_empty());
}

#[tokio::test]
async fn setup_and_picker_share_one_download_window() {
    let r = rig("download-ms=3000").await;
    let ledger = Arc::new(Mutex::new(Vec::new()));
    let sink = ledger.clone();
    let setup = Setup::with_options(
        &r.sup,
        r.sup.dirs().models(),
        move |kind, label, bytes| sink.lock().unwrap().push((kind, label, bytes)),
        SetupOptions {
            poll: Duration::from_millis(20),
            retry_delay: Duration::from_millis(50),
            ram_bytes: Some(64 << 30),
            components: Some(vec![tier::Component {
                role: ComponentRole::Speech,
                model: "Whisper-Tiny",
                bytes: 75_000_000,
            }]),
            ..SetupOptions::default()
        },
    );
    let mut rx = r.get("Whisper-Small").await;
    setup.start(Tier::Tiny).await.unwrap();
    let mut view = setup.view();
    let phase = |v: &watch::Receiver<nocturne_contract::SetupView>| v.borrow().phase;
    tokio::time::timeout(Duration::from_secs(20), async {
        while phase(&view) != nocturne_contract::SetupPhase::Downloading {
            view.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    // Setup pausing gives its share back; the model download still holds the window.
    setup.pause().await.unwrap();
    assert!(!r.offline().await, "the window stays open for the model");
    assert!(!rx.borrow().done);
    until(&mut rx, |p| p.done).await;
    r.wait_offline(true).await;
    // And setup opens it again for itself, and closes it when done.
    setup.resume().await.unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        while phase(&view) != nocturne_contract::SetupPhase::Done {
            assert_ne!(phase(&view), nocturne_contract::SetupPhase::Failed);
            view.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    r.wait_offline(true).await;
}

#[tokio::test]
async fn remove_frees_the_models_bytes() {
    let r = rig("downloaded=all").await;
    assert!(r.present("Whisper-Base").await);
    let freed = nocturne_lemonade::jobs::remove(&r.sup.client(), "Whisper-Base")
        .await
        .unwrap();
    assert_eq!(freed, 148_000_000);
    assert!(!r.present("Whisper-Base").await);
    // Removing it again frees nothing; a model the catalogue lacks is an error.
    let again = nocturne_lemonade::jobs::remove(&r.sup.client(), "Whisper-Base").await;
    assert_eq!(again.unwrap(), 0);
    let unknown = nocturne_lemonade::jobs::remove(&r.sup.client(), "No-Such-Model").await;
    assert!(unknown.is_err());
    let gone = Client::new(common::CLOSED_PORT, "k");
    assert!(
        nocturne_lemonade::jobs::remove(&gone, "Whisper-Base")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn downloads_follow_an_engine_started_in_place_of_one_that_gave_up() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    common::set_mode(&dirs, "download-ms=3000");
    let sup_opts = || SupervisorOptions {
        health_interval: Duration::from_millis(200),
        backoff_base: Duration::from_millis(100),
        ..SupervisorOptions::default()
    };
    let first = Supervisor::start(dirs.clone(), sup_opts()).await.unwrap();
    let jobs = ModelJobs::with_options(first.control(), opts());
    let ledger = Ledger::default();
    let sink = ledger.clone();
    let on: OnDownload = Arc::new(move |m: &str, b: u64| sink.lock().unwrap().push((m.into(), b)));
    let hold = first.control().hold_online().await.unwrap();
    let mut rx = jobs.get("Whisper-Small", hold, on).await.unwrap();
    until(&mut rx, |p| p.received > 0).await;
    first.shutdown().await.unwrap();
    let second = Supervisor::start(dirs.clone(), sup_opts()).await.unwrap();
    jobs.follow(second.control()).await.unwrap();
    let done = until(&mut rx, |p| p.done).await;
    assert!(done.total > 0);
    let present = second
        .client()
        .models()
        .await
        .unwrap()
        .iter()
        .any(|m| m.id == "Whisper-Small" && m.present());
    assert!(present, "fetched by the new engine");
    assert_eq!(ledger.lock().unwrap().len(), 1);
    for _ in 0..200 {
        if second
            .admin()
            .config()
            .await
            .is_ok_and(|c| c["offline"] == true)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the new engine's window was left open");
}

#[tokio::test]
async fn a_paused_download_does_not_open_a_new_engines_window() {
    let r = rig("download-ms=3000").await;
    let other = rig("download-ms=300").await;
    let mut rx = r.get("Whisper-Small").await;
    until(&mut rx, |p| p.received > 0).await;
    r.jobs.pause().await.unwrap();
    r.jobs.follow(other.sup.control()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        other.offline().await,
        "paused: nothing may reach the internet"
    );
    r.jobs.resume().await.unwrap();
    other.wait_offline(false).await;
    until(&mut rx, |p| p.done).await;
    other.wait_offline(true).await;
}
