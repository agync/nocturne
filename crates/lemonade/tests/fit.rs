//! The fit budget's acceptance tests (every tier preset fits at its own minimum), and how each
//! catalogue model is judged for a job on a given computer.

use nocturne_contract::{ComponentRole, Fit, FitReason, Job, ModelChoice, Tier};
use nocturne_lemonade::client::{CheckpointStatus, ModelInfo, SystemInfo};
use nocturne_lemonade::fit::{
    Exclusion, budget, choices, fit_for, for_host, memory_of, peak, peak_with, qualifies,
};
use nocturne_lemonade::fit_table::{self, VOICE_ALONE, preset_peak};
use nocturne_lemonade::selection::{Selection, picks};
use nocturne_lemonade::tier::{self, DISK_HEADROOM, components, min_ram_gib};

const GIB: u64 = 1 << 30;

fn gib(b: u64) -> f64 {
    b as f64 / GIB as f64
}

#[test]
fn budget_values() {
    for (ram, want) in [
        (6, 5.0),
        (8, 6.1),
        (14, 9.4),
        (16, 10.5),
        (28, 17.1),
        (32, 19.3),
        (60, 34.7),
    ] {
        let got = gib(budget(ram * GIB, 0, 0).budget);
        assert!(
            (got - want).abs() < 0.05,
            "{ram} GiB: {got:.2}, want {want}"
        );
    }
}

#[test]
fn tier_presets_fit_their_minimums() {
    for tier in [Tier::Tiny, Tier::Small, Tier::Standard, Tier::Large] {
        let ram = u64::from(min_ram_gib(tier)) * GIB;
        let b = budget(ram, 0, 0);
        let peak = preset_peak(tier).expect("every preset has a peak");
        let fit = fit_for(peak.busiest, &b);
        if tier == Tier::Tiny {
            assert!(
                matches!(fit, Fit::Fits | Fit::Tight),
                "{tier:?} at {} GiB: {fit:?}",
                min_ram_gib(tier)
            );
        } else {
            assert_eq!(fit, Fit::Fits, "{tier:?} at {} GiB", min_ram_gib(tier));
        }
        assert_eq!(
            fit_for(VOICE_ALONE, &b),
            Fit::Fits,
            "the voice alone on {tier:?}"
        );
    }
    // The 8 GB computer Tiny is made for has room to spare.
    assert_eq!(
        fit_for(
            preset_peak(Tier::Tiny).unwrap().busiest,
            &budget(8 * GIB, 0, 0)
        ),
        Fit::Fits
    );
}

#[test]
fn fit_thresholds() {
    let b = budget(16 * GIB, 0, 0);
    assert_eq!(fit_for(b.budget / 100 * 85, &b), Fit::Fits);
    assert_eq!(fit_for(b.budget, &b), Fit::Tight);
    assert_eq!(fit_for(b.budget + 1, &b), Fit::WontFit);
}

const GB: u64 = 1_000_000_000;
const PLENTY: u64 = 10_000 * GB;

fn model(id: &str, recipe: &str, size: f64, labels: &str, on_disk: bool) -> ModelInfo {
    ModelInfo {
        id: id.into(),
        recipe: recipe.into(),
        size,
        downloaded: on_disk,
        labels: labels.split(',').map(str::to_string).collect(),
        checkpoint_status: vec![CheckpointStatus {
            required: true,
            present: on_disk,
        }],
        ..ModelInfo::default()
    }
}

/// The pinned catalogue's models the jobs care about, plus a few that must never be offered.
fn catalogue() -> Vec<ModelInfo> {
    let tiers: Vec<&str> = [Tier::Standard]
        .into_iter()
        .flat_map(components)
        .map(|c| c.model)
        .collect();
    let on = |id: &str| tiers.contains(&id);
    let mut out = vec![
        model(
            "Qwen3.5-0.8B-GGUF",
            "llamacpp",
            0.764,
            "chat,vision,tool-calling",
            false,
        ),
        model(
            "Qwen3.5-2B-GGUF",
            "llamacpp",
            2.01,
            "chat,vision,tool-calling",
            false,
        ),
        model(
            "Qwen3.5-4B-MTP-GGUF",
            "llamacpp",
            3.66,
            "chat,vision,mtp",
            false,
        ),
        model(
            "Qwen3-VL-8B-Instruct-GGUF",
            "llamacpp",
            6.19,
            "chat,vision",
            false,
        ),
        model(
            "Qwen3.5-9B-GGUF",
            "llamacpp",
            6.88,
            "chat,vision,tool-calling",
            false,
        ),
        model(
            "Gemma-4-26B-A4B-it-GGUF",
            "llamacpp",
            18.1,
            "chat,vision",
            false,
        ),
        model(
            "Qwen3.6-35B-A3B-MTP-GGUF",
            "llamacpp",
            23.8,
            "chat,vision,mtp",
            false,
        ),
        model(
            "MiniCPM-V-4.6-Thinking-GGUF",
            "llamacpp",
            1.64,
            "chat,reasoning,vision",
            false,
        ),
        model(
            "Qwen3-4B-Instruct-2507-GGUF",
            "llamacpp",
            2.5,
            "chat,tool-calling",
            false,
        ),
        model("Whisper-Tiny", "whispercpp", 0.075, "transcription", false),
        model("Whisper-Small", "whispercpp", 0.488, "transcription", false),
        model(
            "Whisper-Large-v3",
            "whispercpp",
            3.1,
            "transcription",
            false,
        ),
        model("kokoro-v1", "kokoro", 0.354, "tts", false),
        model(
            "Qwen3-Embedding-0.6B-GGUF",
            "llamacpp",
            0.64,
            "embeddings",
            false,
        ),
        model(
            "Qwen3-Embedding-4B-GGUF",
            "llamacpp",
            4.28,
            "embeddings",
            false,
        ),
        model(
            "Qwen3-Embedding-8B-GGUF",
            "llamacpp",
            8.05,
            "embeddings",
            false,
        ),
        model(
            "nomic-embed-text-v1-GGUF",
            "llamacpp",
            0.0781,
            "embeddings",
            false,
        ),
        model(
            "user.Qwen3.5-9B-GGUF",
            "llamacpp",
            6.88,
            "chat,vision",
            true,
        ),
        model("Phi-4-mini-NPU", "ryzenai-llm", 2.5, "chat,vision", false),
    ];
    for m in &mut out {
        let present = on(&m.id);
        m.downloaded = present;
        m.checkpoint_status[0].present = present;
    }
    out
}

fn info() -> SystemInfo {
    serde_json::from_str(
        r#"{"devices": {"amd_gpu": [{"available": true, "integrated": true}]},
            "recipes": {
              "llamacpp": {"backends": {"vulkan": {"state": "installed"}, "cpu": {"state": "installable"}}},
              "whispercpp": {"backends": {"vulkan": {"state": "installed"}, "cpu": {"state": "installable"}}},
              "kokoro": {"backends": {"cpu": {"state": "installed"}}}}}"#,
    )
    .unwrap()
}

fn find<'a>(list: &'a [ModelChoice], id: &str) -> &'a ModelChoice {
    list.iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("{id} is offered"))
}

fn standard_on(ram_gib: u64) -> (Selection, nocturne_lemonade::fit::Budget) {
    (
        Selection::preset(Tier::Standard),
        budget(ram_gib * GIB, 0, PLENTY),
    )
}

#[test]
fn peak_sets() {
    let small = picks(&Selection::preset(Tier::Small));
    // Reading: the preset's measured figure, above the sum of its models.
    assert_eq!(
        peak(&small, true),
        preset_peak(Tier::Small).unwrap().busiest
    );
    let standard = picks(&Selection::preset(Tier::Standard));
    let mem = |m: &str| memory_of(m, None);
    let read = mem(&standard.mind) + mem(&standard.recall) + mem(&standard.speech);
    let beside = mem(&standard.voice) + mem(&standard.mind) + mem(&standard.recall);
    // Speaking beside the mind and recall is the busiest moment unless the voice runs alone.
    assert_eq!(peak(&standard, false), read.max(beside));
    assert_eq!(peak(&standard, true), read);
    // Nothing chosen still costs the idle floor.
    let empty = picks(&Selection::preset(Tier::Unsupported));
    assert_eq!(peak(&empty, true), 500_000_000);
    // A model the table does not know counts with what the caller says it needs.
    let mut custom = Selection::preset(Tier::Standard);
    custom
        .custom
        .insert(ComponentRole::Mind, "Unknown-GGUF".into());
    let p = picks(&custom);
    let with = peak_with(
        &p,
        true,
        |m| if m == "Unknown-GGUF" { 20 * GB } else { mem(m) },
    );
    assert_eq!(with, 20 * GB + mem(&p.recall) + mem(&p.speech));
}

#[test]
fn fits_tight_wont_thresholds() {
    let (s, b) = standard_on(28);
    let list = choices(Job::Answering, &catalogue(), &info(), &s, &b);
    assert_eq!(find(&list, "Qwen3.5-9B-GGUF").fit, Fit::Fits);
    assert!(find(&list, "Qwen3.5-9B-GGUF").current);
    assert!(find(&list, "Qwen3.5-9B-GGUF").recommended);
    assert_eq!(find(&list, "Qwen3.5-2B-GGUF").fit, Fit::Fits);
    assert!(find(&list, "Qwen3.5-2B-GGUF").smaller);
    // 19.5 + 1.7 + 0.8 = 22 GB of a 18.4 GB budget.
    let gemma = find(&list, "Gemma-4-26B-A4B-it-GGUF");
    assert_eq!(gemma.fit, Fit::WontFit);
    assert!(gemma.reason.is_some());
    // A finding model that lands between 0.85 of the budget and the budget is tight.
    let (s, b) = standard_on(24);
    let list = choices(Job::Finding, &catalogue(), &info(), &s, &b);
    let mid = find(&list, "Qwen3-Embedding-4B-GGUF");
    assert_eq!(mid.fit, Fit::Tight, "{mid:?}");
    assert_eq!(mid.reason, None);
    // The current model comes first, then by what can be used.
    assert!(list[0].current);
    let usable = |c: &ModelChoice| matches!(c.fit, Fit::Fits | Fit::Tight);
    let first_grey = list.iter().position(|c| !usable(c)).unwrap_or(list.len());
    assert!(list[first_grey..].iter().all(|c| !usable(c)));
}

#[test]
fn reasoning_and_text_only_excluded_for_answering() {
    let all = catalogue();
    let thinking = all.iter().find(|m| m.id.contains("Thinking")).unwrap();
    assert_eq!(
        qualifies(Job::Answering, thinking),
        Err(Exclusion::Reasoning)
    );
    let text_only = all
        .iter()
        .find(|m| m.id == "Qwen3-4B-Instruct-2507-GGUF")
        .unwrap();
    assert_eq!(qualifies(Job::Answering, text_only), Err(Exclusion::Label));
    let npu = all.iter().find(|m| m.recipe == "ryzenai-llm").unwrap();
    assert_eq!(qualifies(Job::Answering, npu), Err(Exclusion::Recipe));
    let mut short = model("Small-ctx-GGUF", "llamacpp", 1.0, "chat,vision", true);
    short.max_context_window = Some(4096);
    assert_eq!(
        qualifies(Job::Answering, &short),
        Err(Exclusion::ShortContext)
    );
    let (s, b) = standard_on(64);
    let ids: Vec<String> = choices(Job::Answering, &all, &info(), &s, &b)
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert!(
        !ids.iter()
            .any(|i| i.contains("Thinking") || i.contains("2507"))
    );
    // Each job sees only its own kind of model.
    let speech: Vec<String> = choices(Job::Recordings, &all, &info(), &s, &b)
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert!(
        speech.iter().all(|i| i.starts_with("Whisper")),
        "{speech:?}"
    );
    let voice = choices(Job::Aloud, &all, &info(), &s, &b);
    assert_eq!(voice.len(), 1);
    assert_eq!(voice[0].id, "kokoro-v1");
}

#[test]
fn user_models_never_offered() {
    let all = catalogue();
    let user = all.iter().find(|m| m.id.starts_with("user.")).unwrap();
    assert_eq!(qualifies(Job::Answering, user), Err(Exclusion::UserModel));
    let mut cloud = model("nocturne-online", "cloud", 0.0, "chat,vision", false);
    cloud.cloud_provider = Some("nocturne_online".into());
    assert_eq!(qualifies(Job::Answering, &cloud), Err(Exclusion::Cloud));
    let (s, b) = standard_on(64);
    for job in [Job::Answering, Job::Recordings, Job::Aloud, Job::Finding] {
        let list = choices(job, &all, &info(), &s, &b);
        assert!(list.iter().all(|c| !c.id.starts_with("user.")), "{job:?}");
    }
}

#[test]
fn unsupported_backend_cant_use() {
    let json = r#"{"devices": {"amd_gpu": []},
        "recipes": {"llamacpp": {"backends": {"cpu": {"state": "unsupported"}}},
                    "whispercpp": {"backends": {"cpu": {"state": "installed"}}}}}"#;
    let no_llama: SystemInfo = serde_json::from_str(json).unwrap();
    let (s, b) = standard_on(64);
    let list = choices(Job::Answering, &catalogue(), &no_llama, &s, &b);
    let c = find(&list, "Qwen3.5-2B-GGUF");
    assert_eq!(c.fit, Fit::CantUse);
    assert_eq!(c.reason, Some(FitReason::NotAvailable));
    let list = choices(Job::Recordings, &catalogue(), &no_llama, &s, &b);
    assert_eq!(find(&list, "Whisper-Tiny").fit, Fit::Fits);
}

fn with_cards(cards: &str) -> SystemInfo {
    serde_json::from_str(&format!(
        r#"{{"devices": {{{cards}}},
            "recipes": {{"llamacpp": {{"backends": {{"vulkan": {{"state": "installable"}}, "cpu": {{"state": "installable"}}}}}}}}}}"#
    ))
    .unwrap()
}

#[test]
fn a_cards_own_memory_lifts_the_engine_cap_and_never_the_memory_budget() {
    let ram = 16 * GIB;
    let plain = for_host(&with_cards(r#""amd_gpu": []"#), ram, PLENTY);
    let small = for_host(
        &with_cards(r#""nvidia_gpu": [{"available": true, "vram_gb": 4.0}]"#),
        ram,
        PLENTY,
    );
    let large = for_host(
        &with_cards(r#""nvidia_gpu": [{"available": true, "vram_gb": 24.0}]"#),
        ram,
        PLENTY,
    );
    // 4 GB of graphics memory is under 0.8 x RAM, so it changes nothing.
    assert_eq!(small, plain);
    assert_eq!(large.engine_cap, 24_000_000_000);
    // The memory a choice may use is still the computer's, whatever the card holds.
    assert_eq!(large.budget, plain.budget);
}

#[test]
fn only_a_card_of_its_own_counts_as_a_pool() {
    let pool = |cards: &str| for_host(&with_cards(cards), 4 * GIB, PLENTY).engine_cap;
    let floor = pool(r#""amd_gpu": []"#);
    // The engine lists one placeholder when it finds no NVIDIA card.
    assert_eq!(
        pool(r#""nvidia_gpu": [{"available": false, "vram_gb": 12.0, "error": "none"}]"#),
        floor
    );
    assert_eq!(
        pool(r#""amd_gpu": [{"available": true, "integrated": true, "vram_gb": 8.0}]"#),
        floor
    );
    assert_eq!(
        pool(r#""intel_gpu": [{"available": true, "integrated": true, "vram_gb": 8.0}]"#),
        floor
    );
    assert_eq!(
        pool(r#""intel_gpu": [{"available": true, "vram_gb": 16.0}]"#),
        16_000_000_000
    );
    assert_eq!(
        pool(
            r#""amd_gpu": [{"available": true, "vram_gb": 8.0}],
               "nvidia_gpu": [{"available": true, "vram_gb": 12.0}]"#
        ),
        12_000_000_000
    );
}

#[test]
fn a_model_too_big_for_a_small_card_still_fits_by_the_computers_memory() {
    // A 4 GB card holds none of the 6.9 GB Standard mind; the engine loads what the card cannot
    // hold from the computer's memory, so the judgement is the memory's alone.
    let (s, b) = standard_on(32);
    let nvidia = with_cards(r#""nvidia_gpu": [{"available": true, "vram_gb": 4.0}]"#);
    let none = with_cards(r#""amd_gpu": []"#);
    let on_card = choices(Job::Answering, &catalogue(), &nvidia, &s, &b);
    let on_cpu = choices(Job::Answering, &catalogue(), &none, &s, &b);
    let key = |l: &[ModelChoice]| {
        l.iter()
            .map(|c| (c.id.clone(), c.fit, c.reason.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(key(&on_card), key(&on_cpu));
    assert_eq!(find(&on_card, "Qwen3.5-9B-GGUF").fit, Fit::Fits);
}

#[test]
fn engine_cap_cant_use() {
    // 0.8 x 16 GiB is under the 23.8 GB download: the engine would not even list it.
    let (s, b) = standard_on(16);
    let list = choices(Job::Answering, &catalogue(), &info(), &s, &b);
    let big = find(&list, "Qwen3.6-35B-A3B-MTP-GGUF");
    assert_eq!(big.fit, Fit::CantUse);
    assert_eq!(big.reason, Some(FitReason::NotAvailable));
    // A large graphics pool lifts the cap, and then memory decides.
    let pooled = budget(16 * GIB, 32 * GIB, PLENTY);
    let list = choices(Job::Answering, &catalogue(), &info(), &s, &pooled);
    assert_eq!(find(&list, "Qwen3.6-35B-A3B-MTP-GGUF").fit, Fit::WontFit);
}

#[test]
fn disk_reason_names_shortfall() {
    let s = Selection::preset(Tier::Standard);
    let free = 7 * GB;
    let b = budget(64 * GIB, 0, free);
    let list = choices(Job::Answering, &catalogue(), &info(), &s, &b);
    let vl = find(&list, "Qwen3-VL-8B-Instruct-GGUF");
    assert_eq!(vl.fit, Fit::CantUse);
    let want = (6_190_000_000 + DISK_HEADROOM - free) as f64;
    assert_eq!(vl.reason, Some(FitReason::Disk { short_bytes: want }));
    // What is on disk needs no space, and a small download still fits.
    assert_eq!(find(&list, "Qwen3.5-9B-GGUF").fit, Fit::Fits);
    assert_eq!(find(&list, "Qwen3.5-2B-GGUF").fit, Fit::Fits);
}

#[test]
fn limiter_named_when_another_job_is_the_cause() {
    // Standard at 24 GiB: an 8 GB finding model with the tier's 9B mind is too much, but it
    // would fit beside a smaller mind.
    let (s, b) = standard_on(24);
    let list = choices(Job::Finding, &catalogue(), &info(), &s, &b);
    let big = find(&list, "Qwen3-Embedding-8B-GGUF");
    assert_eq!(big.fit, Fit::WontFit);
    assert_eq!(
        big.reason,
        Some(FitReason::NeedsOtherJob {
            job: Job::Answering
        })
    );
    // The person's own choice for another job is named as the limiter.
    let mut mine = Selection::preset(Tier::Standard);
    mine.custom
        .insert(ComponentRole::Speech, "Whisper-Large-v3".into());
    let b = budget(26 * GIB, 0, PLENTY);
    let list = choices(Job::Answering, &catalogue(), &info(), &mine, &b);
    let vl = find(&list, "Qwen3-VL-8B-Instruct-GGUF");
    assert_eq!(vl.fit, Fit::Fits, "{vl:?}");
    let s = Selection::preset(Tier::Standard);
    // 19.5 GiB: the preset is tight; the larger speech model the person chose tips it over.
    let tight = budget(39 * GIB / 2, 0, PLENTY);
    let list_preset = choices(Job::Answering, &catalogue(), &info(), &s, &tight);
    let list_mine = choices(Job::Answering, &catalogue(), &info(), &mine, &tight);
    let nine_preset = find(&list_preset, "Qwen3.5-9B-GGUF");
    let nine_mine = find(&list_mine, "Qwen3.5-9B-GGUF");
    assert_ne!(nine_preset.fit, Fit::WontFit);
    assert_eq!(nine_mine.fit, Fit::WontFit, "{nine_mine:?}");
    assert_eq!(
        nine_mine.reason,
        Some(FitReason::Memory {
            limiter: Some(Job::Recordings)
        })
    );
    // Too big whatever the others use: no limiter.
    let tiny = budget(6 * GIB, 32 * GIB, PLENTY);
    let list = choices(Job::Answering, &catalogue(), &info(), &s, &tiny);
    assert_eq!(
        find(&list, "Gemma-4-26B-A4B-it-GGUF").reason,
        Some(FitReason::Memory { limiter: None })
    );
}

#[test]
fn unknown_model_uses_formula_and_untested() {
    let plain = model("Newcomer-VL-3B-GGUF", "llamacpp", 3.0, "chat,vision", false);
    assert_eq!(memory_of(&plain.id, Some(&plain)), 4_200_000_000);
    let mtp = model(
        "Newcomer-MTP-GGUF",
        "llamacpp",
        3.0,
        "chat,vision,mtp",
        false,
    );
    assert_eq!(memory_of(&mtp.id, Some(&mtp)), 4_550_000_000);
    let mut all = catalogue();
    all.push(plain);
    let (s, b) = standard_on(28);
    let list = choices(Job::Answering, &all, &info(), &s, &b);
    let c = find(&list, "Newcomer-VL-3B-GGUF");
    assert!(!c.tested);
    assert_eq!(c.memory_bytes, 4_200_000_000.0);
    assert_eq!(c.name, "Newcomer-VL-3B");
    assert!(find(&list, "Qwen3.5-9B-GGUF").tested);
    assert!(!find(&list, "Qwen3-VL-8B-Instruct-GGUF").tested);
    assert_eq!(find(&list, "Qwen3-VL-8B-Instruct-GGUF").name, "Qwen3-VL-8B");
}

#[test]
fn table_covers_every_tier_model() {
    for t in [Tier::Large, Tier::Standard, Tier::Small, Tier::Tiny] {
        for c in components(t) {
            let e =
                fit_table::lookup(c.model).unwrap_or_else(|| panic!("{} in the table", c.model));
            assert!(e.tested, "{} is tested", c.model);
            assert!(e.ctx_ok);
            assert!(e.memory > 0);
        }
        // An estimated preset peak is the sum of the table's figures for its reading set.
        let p = picks(&Selection::preset(t));
        let sum: u64 = [&p.mind, &p.recall, &p.speech]
            .into_iter()
            .map(|m| memory_of(m, None))
            .sum();
        let peak = preset_peak(t).unwrap();
        if !peak.measured {
            assert_eq!(sum, peak.busiest, "{t:?}");
        }
    }
    let recall = fit_table::lookup(tier::RECALL.model).unwrap();
    assert_eq!(recall.dim, Some(1024));
    // Only the decision models are user models, and only the evaluation loads them.
    assert!(
        fit_table::TABLE
            .iter()
            .filter(|e| e.id.starts_with("user."))
            .all(|e| nocturne_lemonade::decision_models::ALL
                .iter()
                .any(|d| d.id == e.id))
    );
    assert!(
        nocturne_lemonade::decision_models::ALL
            .iter()
            .all(|d| fit_table::lookup(d.id).is_some_and(|e| !e.tested))
    );
    let mut ids: Vec<&str> = fit_table::TABLE.iter().map(|e| e.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), fit_table::TABLE.len());
}

#[test]
fn measured_excess_follows_the_mind() {
    let small = Selection::preset(Tier::Small);
    let mem = |m: &str| memory_of(m, None);
    // The preset reads at its measured figure.
    assert_eq!(
        peak(&picks(&small), true),
        preset_peak(Tier::Small).unwrap().busiest
    );
    // Another speech model keeps the mind's measured excess rather than dropping it.
    let mut lighter = small.clone();
    lighter
        .custom
        .insert(ComponentRole::Speech, "Whisper-Tiny".into());
    let want =
        preset_peak(Tier::Small).unwrap().busiest - mem("Whisper-Base") + mem("Whisper-Tiny");
    assert_eq!(peak(&picks(&lighter), true), want);
    // Another mind carries no excess measured for this one; an unmeasured preset has none.
    let mut other = small.clone();
    other
        .custom
        .insert(ComponentRole::Mind, "Qwen3.5-2B-GGUF".into());
    let p = picks(&other);
    assert_eq!(
        peak(&p, true),
        mem(&p.mind) + mem(&p.recall) + mem(&p.speech)
    );
    assert_eq!(
        nocturne_lemonade::fit::measured_excess("Qwen3.5-9B-GGUF"),
        0
    );
}
