//! The NPU against the fake engine: what its system-info says, where a recording or an item
//! goes, and the silent fallback to the tier's own models.

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nocturne_contract::MemoryView;
use nocturne_contract::{NpuCause, Tier};
use nocturne_engine::{Fact, Mind};
use nocturne_lemonade::npu::{self, Npu, Placement, Readiness, Work};
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

    /// Each load asked for, with its context.
    async fn seen_loads(&self) -> Vec<(String, u64)> {
        self.call("GET", "/fake/requests", json!({}))
            .await
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["path"] == "/v1/load")
            .map(|e| {
                (
                    e["body"]["model_name"].as_str().unwrap_or("").to_string(),
                    e["body"]["ctx_size"].as_u64().unwrap_or(0),
                )
            })
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

/// What each reading request went to, in order, with whether it carried a grammar or a schema.
async fn readings(r: &Rig) -> Vec<(String, bool, bool)> {
    r.call("GET", "/fake/requests", json!({}))
        .await
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["path"] == "/v1/chat/completions")
        .map(|e| {
            let b = &e["body"];
            (
                b["model"].as_str().unwrap_or("").to_string(),
                b.get("grammar").is_some_and(|g| !g.is_null()),
                b["response_format"]["type"] == "json_schema",
            )
        })
        .collect()
}

fn own_mind() -> &'static str {
    nocturne_lemonade::tier::model_for(TIER, nocturne_contract::ComponentRole::Mind).unwrap()
}

fn context() -> nocturne_engine::ExtractContext {
    nocturne_engine::ExtractContext {
        item_kind: nocturne_contract::ItemKind::Text,
        kept_at: "2026-11-10T09:30:00Z".parse().unwrap(),
        today: "2026-11-10".parse().unwrap(),
        known_entities: Vec::new(),
    }
}

fn tea() -> Value {
    let memory = json!({
        "sentence": "Sam likes tea.", "subject": "Sam", "subjectKind": "person",
        "predicate": "likes", "object": "tea", "validFrom": null, "validTo": null,
        "due": null, "confidence": 0.9, "span": {"quote": "Sam likes tea"}, "tags": [],
    });
    json!({"content": json!({"memories": [memory]}).to_string()})
}

async fn placed(mode: &str, placement: Placement) -> Rig {
    let r = rig(mode).await;
    r.npu.set_placement(placement);
    r
}

impl Rig {
    /// The models the engine holds now.
    async fn held(&self) -> Vec<String> {
        self.call("GET", "/v1/health", json!({})).await["all_models_loaded"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["model_name"].as_str().unwrap().to_string())
            .collect()
    }

    async fn allow_per_kind(&self, n: u32) {
        self.sup
            .admin()
            .set(&json!({"max_loaded_models": n}))
            .await
            .unwrap();
    }
}

fn fact() -> Fact {
    let m: MemoryView = serde_json::from_value(json!({
        "id": "m1", "sentence": "Sam likes tea.", "subject": "Sam", "entityId": null,
        "predicate": "likes", "object": "tea", "certainty": "sure", "status": "active",
        "validFrom": null, "validTo": null, "due": null, "learnedAt": "2026-11-10T09:30:00+00:00",
        "lastUsed": null, "letGoAt": null, "keepForever": false, "private": false,
        "suggestPrivate": false, "was": null, "receipts": [],
    }))
    .unwrap();
    Fact::from(m)
}

async fn ask(r: &Rig) -> nocturne_engine::AnswerMeta {
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let facts = [fact()];
    let meta = r
        .mind
        .answer("What does Sam like?", &facts, tx)
        .await
        .unwrap();
    while rx.recv().await.is_some() {}
    meta
}

#[tokio::test]
async fn items_are_read_on_the_npu_by_a_schema_while_the_mind_stays_loaded() {
    let r = placed("npu=ready,downloaded=all", Placement::Split).await;
    r.allow_per_kind(2).await;
    ask(&r).await;
    r.call("POST", "/fake/script", json!({"chat": [tea()]}))
        .await;
    let found = r.mind.extract("Sam likes tea", &context()).await.unwrap();
    assert_eq!(found.len(), 1);
    let chats = readings(&r).await;
    assert_eq!(chats[0].0, own_mind(), "the GPU mind answers");
    assert_eq!(
        chats[1],
        (npu::MIND.model.to_string(), false, true),
        "the NPU runtime ignores a grammar, so the NPU's mind reads by the schema"
    );
    let loads = r.seen_loads().await;
    assert_eq!(loads[1], (npu::MIND.model.to_string(), 8192));
    let held = r.held().await;
    assert!(
        held.iter().any(|m| m == own_mind()) && held.iter().any(|m| m == npu::MIND.model),
        "{held:?}"
    );
    assert_eq!(r.npu.models_per_kind(), 2);
}

#[tokio::test]
async fn one_model_of_a_kind_evicts_the_gpu_mind() {
    let r = placed("npu=ready,downloaded=all", Placement::Split).await;
    ask(&r).await;
    r.call("POST", "/fake/script", json!({"chat": [tea()]}))
        .await;
    r.mind.extract("Sam likes tea", &context()).await.unwrap();
    let held = r.held().await;
    assert!(!held.iter().any(|m| m == own_mind()), "{held:?}");
    assert!(held.iter().any(|m| m == npu::MIND.model), "{held:?}");
}

#[tokio::test]
async fn with_the_npus_mind_alone_it_reads_and_answers_and_the_gpu_mind_stays_away() {
    let r = placed("npu=ready,downloaded=all", Placement::NpuOnly).await;
    r.call("POST", "/fake/script", json!({"chat": [tea()]}))
        .await;
    r.mind.extract("Sam likes tea", &context()).await.unwrap();
    let meta = ask(&r).await;
    assert!(!meta.not_sure);
    r.mind.extract("Nothing to see", &context()).await.unwrap();
    assert!(
        readings(&r)
            .await
            .iter()
            .all(|(m, _, _)| m == npu::MIND.model),
        "{:?}",
        readings(&r).await
    );
    assert!(
        r.seen_loads()
            .await
            .iter()
            .all(|(m, _)| m == npu::MIND.model)
    );
    assert_eq!(r.npu.models_per_kind(), 1);
}

#[tokio::test]
async fn an_answer_the_npu_cannot_give_comes_from_the_gpu_mind() {
    let r = placed("npu=ready,downloaded=all", Placement::NpuOnly).await;
    r.call(
        "POST",
        "/fake/script",
        json!({"chat": [{"status": 500, "error": "flm serve exited"}]}),
    )
    .await;
    let meta = ask(&r).await;
    assert!(!meta.not_sure);
    let chats: Vec<String> = readings(&r).await.into_iter().map(|c| c.0).collect();
    assert_eq!(chats, [npu::MIND.model, own_mind()]);
}

#[tokio::test]
async fn with_the_npus_mind_alone_a_failure_swaps_the_gpu_mind_in_and_back_out() {
    let r = placed("npu=ready,downloaded=all", Placement::NpuOnly).await;
    r.mind.extract("Sam likes tea", &context()).await.unwrap();
    assert_eq!(r.held().await, [npu::MIND.model]);
    r.call(
        "POST",
        "/fake/script",
        json!({"chat": [{"status": 500, "error": "flm serve exited"}]}),
    )
    .await;
    assert!(!ask(&r).await.not_sure);
    assert_eq!(
        r.held().await,
        [own_mind()],
        "one model of a kind: the GPU mind took the NPU's place"
    );
    r.mind.extract("Sam likes tea", &context()).await.unwrap();
    assert_eq!(
        r.held().await,
        [npu::MIND.model],
        "the next item brings the NPU's mind back"
    );
    assert_eq!(readings(&r).await.last().unwrap().0, npu::MIND.model);
}

/// The paths asked for, in order, of `paths`.
async fn asked(r: &Rig, paths: &[&str]) -> Vec<String> {
    r.call("GET", "/fake/requests", json!({}))
        .await
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["path"].as_str())
        .filter(|p| paths.contains(p))
        .map(String::from)
        .collect()
}

#[tokio::test]
async fn the_limit_rises_before_both_minds_load_and_falls_after_one_leaves() {
    let r = rig("npu=ready,downloaded=all").await;
    let (admin, client) = (r.sup.admin(), r.sup.client());
    r.npu
        .move_to(
            Some(&admin),
            &client,
            Placement::Gpu,
            Placement::Split,
            own_mind(),
        )
        .await;
    assert_eq!(r.npu.placement(), Placement::Split);
    assert_eq!(admin.config().await.unwrap()["max_loaded_models"], 2);
    assert_eq!(client.models_per_kind(), 2, "and at the next launch");
    ask(&r).await;
    r.call("POST", "/fake/script", json!({"chat": [tea()]}))
        .await;
    r.mind.extract("Sam likes tea", &context()).await.unwrap();
    assert_eq!(r.held().await.len(), 2);

    r.npu
        .move_to(
            Some(&admin),
            &client,
            Placement::Split,
            Placement::NpuOnly,
            own_mind(),
        )
        .await;
    assert_eq!(
        asked(&r, &["/internal/set", "/v1/unload"]).await,
        ["/internal/set", "/v1/unload", "/internal/set"],
        "the GPU mind leaves before the limit falls"
    );
    assert_eq!(r.held().await, [npu::MIND.model]);
    assert_eq!(admin.config().await.unwrap()["max_loaded_models"], 1);
    assert_eq!(client.models_per_kind(), 1);
}

#[tokio::test]
async fn without_the_engine_taking_two_the_gpu_keeps_the_work() {
    let r = rig("npu=ready,downloaded=all").await;
    let client = r.sup.client();
    r.npu
        .move_to(None, &client, Placement::Gpu, Placement::Split, own_mind())
        .await;
    assert_eq!(r.npu.placement(), Placement::Gpu);
    assert_eq!(client.models_per_kind(), 1);
}

#[tokio::test]
async fn an_item_the_npu_garbles_is_read_by_the_gpu_mind() {
    let r = placed("npu=ready,downloaded=all", Placement::Split).await;
    let garbled = json!({"content": "not a reading"});
    r.call(
        "POST",
        "/fake/script",
        json!({"chat": [garbled, garbled, tea(), tea()]}),
    )
    .await;
    let found = r.mind.extract("Sam likes tea", &context()).await.unwrap();
    assert_eq!(found.len(), 1);
    let npu_mind = npu::MIND.model.to_string();
    assert_eq!(
        readings(&r).await,
        [
            (npu_mind.clone(), false, true),
            (npu_mind.clone(), false, true),
            (own_mind().to_string(), true, false),
        ],
        "the retry on the NPU, then the GPU mind by its grammar"
    );
    assert_eq!(
        r.npu.model_for(Work::Reading),
        Some(npu::MIND.model),
        "one failure is not enough"
    );
    r.mind.extract("Sam likes tea", &context()).await.unwrap();
    assert_eq!(readings(&r).await.last().unwrap().0, npu_mind);
}

#[tokio::test]
async fn an_empty_reading_is_read_again_and_counts_only_when_the_gpu_finds_more() {
    let r = placed("npu=ready,downloaded=all", Placement::Split).await;
    let empty = json!({"content": json!({"memories": []}).to_string()});
    for _ in 0..npu::STRIKES {
        r.call("POST", "/fake/script", json!({"chat": [empty, empty]}))
            .await;
        assert!(
            r.mind
                .extract("Nothing here", &context())
                .await
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(
        r.npu.model_for(Work::Reading),
        Some(npu::MIND.model),
        "nothing to find is no failure"
    );
    for _ in 0..npu::STRIKES {
        r.call("POST", "/fake/script", json!({"chat": [empty, tea()]}))
            .await;
        assert_eq!(
            r.mind
                .extract("Sam likes tea", &context())
                .await
                .unwrap()
                .len(),
            1
        );
    }
    assert_eq!(
        r.npu.model_for(Work::Reading),
        None,
        "the NPU kept missing what was there"
    );
}

#[tokio::test]
async fn an_npu_that_keeps_failing_leaves_the_work_to_the_gpu_until_restart() {
    let r = placed("npu=broken,downloaded=all", Placement::Split).await;
    for _ in 0..npu::STRIKES {
        assert!(r.npu.model_for(Work::Reading).is_some());
        r.mind.extract("Sam likes tea", &context()).await.unwrap();
    }
    assert_eq!(r.npu.model_for(Work::Reading), None);
    assert_eq!(
        r.npu.models_per_kind(),
        1,
        "one model of each kind from the next launch"
    );
    r.mind.extract("Sam likes tea", &context()).await.unwrap();
    let tried = r
        .seen_loads()
        .await
        .iter()
        .filter(|(m, _)| m == npu::MIND.model)
        .count();
    assert_eq!(
        tried,
        npu::STRIKES as usize,
        "each failed load is one strike"
    );
    assert!(
        readings(&r)
            .await
            .iter()
            .all(|(m, grammar, _)| m == own_mind() && *grammar),
        "every item was read by the GPU mind"
    );
}

#[tokio::test]
async fn a_missing_npu_mind_is_never_fetched_on_use() {
    let r = placed("npu=ready,downloaded=all", Placement::NpuOnly).await;
    r.call("POST", "/v1/delete", json!({"model_name": npu::MIND.model}))
        .await;
    r.mind.extract("Sam likes tea", &context()).await.unwrap();
    assert_eq!(r.npu.model_for(Work::Answering), None);
    assert!(r.seen_loads().await.iter().all(|(m, _)| m == own_mind()));
}

#[tokio::test]
async fn on_the_gpu_alone_items_are_read_by_the_gpu_mind() {
    let r = rig("npu=ready,downloaded=all").await;
    r.mind.extract("Sam likes tea", &context()).await.unwrap();
    assert_eq!(readings(&r).await, [(own_mind().to_string(), true, false)]);
    assert_eq!(r.npu.models_per_kind(), 1);
}

/// A decision asked in shadow changes nothing: when the NPU's mind cannot answer it, that is no
/// strike against it, and the GPU mind is not loaded to answer instead.
#[tokio::test]
async fn a_decision_the_npu_cannot_answer_is_no_strike() {
    use nocturne_engine::decide::Kind;
    let r = placed("npu=broken,downloaded=all", Placement::Split).await;
    let decision = nocturne_engine::decide::Decision {
        state: "Memory: Sam likes tea.".into(),
        question: Kind::Sensitive.question(),
    };
    for _ in 0..=npu::STRIKES {
        assert!(r.mind.decide(&decision).await.is_err());
    }
    assert_eq!(r.npu.model_for(Work::Reading), Some(npu::MIND.model));
    assert!(
        r.seen_loads().await.iter().all(|(m, _)| m != own_mind()),
        "the GPU mind was not loaded for a decision"
    );
}
