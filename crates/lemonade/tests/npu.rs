//! Reading recordings on the NPU against the fake engine: what its system-info says, where a
//! recording goes, and the silent fallback to the tier's own speech model.

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nocturne_contract::{NpuCause, Tier};
use nocturne_engine::Mind;
use nocturne_lemonade::npu::{self, Npu, Readiness};
use nocturne_lemonade::selection::{self, Selection};
use nocturne_lemonade::{Dirs, LemonadeMind, Supervisor, SupervisorOptions};
use serde_json::{Value, json};

struct Rig {
    sup: Supervisor,
    npu: Arc<Npu>,
    mind: LemonadeMind,
    dirs: Dirs,
    _tmp: tempfile::TempDir,
}

const TIER: Tier = Tier::Standard;

async fn rig(mode: &str) -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    common::set_mode(&dirs, mode);
    let sup = Supervisor::start(
        dirs.clone(),
        SupervisorOptions {
            health_interval: Duration::from_millis(200),
            ..SupervisorOptions::default()
        },
    )
    .await
    .unwrap();
    let info = sup.client().system_info().await.unwrap();
    let npu = Arc::new(Npu::default());
    npu.set(npu::readiness(&info, false), true, false);
    let picks = selection::picks(&Selection::preset(TIER));
    let mind = LemonadeMind::new(sup.client(), picks, TIER).with_npu(npu.clone());
    Rig {
        sup,
        npu,
        mind,
        dirs,
        _tmp: tmp,
    }
}

impl Rig {
    async fn call(&self, method: &str, path: &str, body: Value) -> Value {
        let e = self.sup.client().endpoint().unwrap();
        let url = format!("http://127.0.0.1:{}{path}", e.port);
        let http = reqwest::Client::new();
        let rb = if method == "POST" {
            http.post(url).json(&body)
        } else {
            http.get(url)
        };
        rb.bearer_auth(e.key)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    /// The model each transcription asked for, in order.
    async fn heard_by(&self) -> Vec<String> {
        self.call("GET", "/fake/requests", json!({}))
            .await
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["multipart"] == true)
            .map(|e| e["fields"]["model"].as_str().unwrap_or("").to_string())
            .collect()
    }

    fn note(&self) -> PathBuf {
        let p = self.dirs.root.join("note.wav");
        let rate = 1_000u32;
        let data = 3 * rate;
        let mut v = b"RIFF".to_vec();
        v.extend((36 + data).to_le_bytes());
        v.extend(b"WAVEfmt ");
        v.extend(16u32.to_le_bytes());
        v.extend([1, 0, 1, 0]);
        v.extend(rate.to_le_bytes());
        v.extend(rate.to_le_bytes());
        v.extend([1, 0, 8, 0]);
        v.extend(b"data");
        v.extend(data.to_le_bytes());
        v.resize(v.len() + data as usize, 0x80);
        std::fs::write(&p, v).unwrap();
        p
    }
}

fn own_speech() -> &'static str {
    nocturne_lemonade::tier::model_for(TIER, nocturne_contract::ComponentRole::Speech).unwrap()
}

#[tokio::test]
async fn system_info_says_what_the_npu_can_do() {
    let readiness_of = |mode: &'static str| async move {
        let r = rig(mode).await;
        let info = r.sup.client().system_info().await.unwrap();
        npu::readiness(&info, false)
    };
    assert_eq!(readiness_of("").await, Readiness::Absent);
    assert_eq!(readiness_of("npu=ready").await, Readiness::Fetch);
    assert_eq!(
        readiness_of("npu=ready,downloaded=all").await,
        Readiness::Ready
    );
    assert_eq!(
        readiness_of("npu=driver-missing").await,
        Readiness::Blocked {
            cause: NpuCause::DriverMissing,
            detail: Some("NPU hardware found but driver not installed".into())
        }
    );
    assert_eq!(
        readiness_of("npu=unsupported,downloaded=all").await,
        Readiness::Blocked {
            cause: NpuCause::NotSupported,
            detail: Some("Requires AMD XDNA 2 NPU".into())
        }
    );
}

#[tokio::test]
async fn a_usable_npu_reads_recordings() {
    let r = rig("npu=ready,downloaded=all").await;
    let t = r.mind.transcribe(&r.note()).await.unwrap();
    assert_eq!(t.text, "hello world");
    assert_eq!(r.heard_by().await, [npu::SPEECH.model]);
    let v = r.npu.view();
    assert!(v.present && v.used, "{v:?}");
    assert_eq!(v.model.as_deref(), Some(npu::SPEECH.model));
}

#[tokio::test]
async fn no_npu_reads_recordings_as_before() {
    let r = rig("downloaded=all").await;
    r.mind.transcribe(&r.note()).await.unwrap();
    assert_eq!(r.heard_by().await, [own_speech()]);
    assert!(!r.npu.view().present);
}

#[tokio::test]
async fn a_refused_npu_falls_back_without_a_word_and_says_why_in_the_details() {
    let r = rig("npu=memlock,downloaded=all").await;
    let first = r.mind.transcribe(&r.note()).await.unwrap();
    assert_eq!(
        first.text, "hello world",
        "the person gets the recording read"
    );
    let second = r.mind.transcribe(&r.note()).await.unwrap();
    assert_eq!(second.text, "hello world");
    assert_eq!(
        r.heard_by().await,
        [npu::SPEECH.model, own_speech(), own_speech()],
        "the NPU is tried once in this run of the engine"
    );
    let v = r.npu.view();
    assert!(v.present && !v.used);
    assert_eq!(v.causes, [NpuCause::MemoryLock]);
    assert_eq!(
        v.detail.as_deref(),
        Some("FLM NPU validation failed: Memlock limits are too low.")
    );
}

#[tokio::test]
async fn every_reason_in_one_refusal_is_kept() {
    let r = rig("npu=firmware,downloaded=all").await;
    r.mind.transcribe(&r.note()).await.unwrap();
    assert_eq!(
        r.npu.view().causes,
        [NpuCause::KernelOld, NpuCause::FirmwareOld]
    );
}

#[tokio::test]
async fn a_broken_npu_request_falls_back_too() {
    let r = rig("npu=broken,downloaded=all").await;
    assert_eq!(
        r.mind.transcribe(&r.note()).await.unwrap().text,
        "hello world"
    );
    let v = r.npu.view();
    assert_eq!(v.causes, [NpuCause::Failed]);
    assert_eq!(v.detail.as_deref(), Some("flm serve exited"));
}

#[tokio::test]
async fn a_missing_npu_model_is_never_fetched_on_use() {
    let r = rig("npu=ready,downloaded=all").await;
    r.call(
        "POST",
        "/v1/delete",
        json!({"model_name": npu::SPEECH.model}),
    )
    .await;
    r.mind.transcribe(&r.note()).await.unwrap();
    assert_eq!(r.heard_by().await, [own_speech()]);
    assert_eq!(r.npu.view().causes, [NpuCause::NotFetched]);
}
