//! The model chosen for each job: the tier preset, the person's changes on top, what the engine
//! is told to keep loaded and how, and the mind that follows the choice.

mod common;

use std::collections::BTreeMap;

use nocturne_contract::{ComponentRole, Tier};
use nocturne_engine::Mind;
use nocturne_lemonade::LemonadeMind;
use nocturne_lemonade::fit::{budget, with_choice};
use nocturne_lemonade::record::{self, SetupRecord};
use nocturne_lemonade::residency::{
    alone, alone_for, mind_context, saved_options, saved_options_for,
};
use nocturne_lemonade::selection::{self, OldModel, Pending, Selection, picks};
use nocturne_lemonade::tier::{self, RECALL_CONTEXT, components};

const GIB: u64 = 1 << 30;
const TIERS: [Tier; 4] = [Tier::Large, Tier::Standard, Tier::Small, Tier::Tiny];

fn installed(dir: &std::path::Path, tier: Tier) {
    record::write(
        dir,
        &SetupRecord {
            version: 1,
            tier: Some(tier),
            done: true,
            completed: Some(tier),
            backends: BTreeMap::new(),
        },
    )
    .unwrap();
}

#[test]
fn tier_preset_equals_todays_components() {
    for t in TIERS {
        let p = picks(&Selection::preset(t));
        for c in components(t) {
            assert_eq!(p.get(c.role), c.model, "{t:?} {:?}", c.role);
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    installed(tmp.path(), Tier::Small);
    assert_eq!(selection::read(tmp.path()), Selection::preset(Tier::Small));
}

#[test]
fn custom_overrides_one_role() {
    let s = with_choice(
        &Selection::preset(Tier::Standard),
        ComponentRole::Speech,
        "Whisper-Large-v3",
    );
    let p = picks(&s);
    assert_eq!(p.speech, "Whisper-Large-v3");
    let preset = picks(&Selection::preset(Tier::Standard));
    assert_eq!(p.mind, preset.mind);
    assert_eq!(p.voice, preset.voice);
    assert_eq!(p.recall, preset.recall);
    assert_eq!(s.custom.len(), 1);
}

#[test]
fn reset_clears_custom() {
    let mut s = with_choice(
        &Selection::preset(Tier::Standard),
        ComponentRole::Mind,
        "Qwen3.5-2B-GGUF",
    );
    // Choosing the tier's own model again is no custom choice at all.
    s = with_choice(&s, ComponentRole::Mind, "Qwen3.5-9B-GGUF");
    assert!(s.custom.is_empty());
    let tmp = tempfile::tempdir().unwrap();
    installed(tmp.path(), Tier::Standard);
    let custom = with_choice(&s, ComponentRole::Speech, "Whisper-Tiny");
    selection::write(tmp.path(), &custom).unwrap();
    assert!(!selection::read(tmp.path()).custom.is_empty());
    selection::clear(tmp.path()).unwrap();
    selection::clear(tmp.path()).unwrap();
    assert_eq!(
        selection::read(tmp.path()),
        Selection::preset(Tier::Standard)
    );
}

#[test]
fn saved_options_cover_selected_models() {
    let mut s = Selection::preset(Tier::Tiny);
    s.custom
        .insert(ComponentRole::Mind, "Qwen3-VL-4B-Instruct-GGUF".into());
    s.custom
        .insert(ComponentRole::Recall, "nomic-embed-text-v1-GGUF".into());
    s.custom
        .insert(ComponentRole::Speech, "Whisper-Tiny".into());
    let saved = saved_options_for(&s);
    let of = |m: &str| {
        saved
            .iter()
            .find(|(id, _)| id == m)
            .map(|(_, o)| o.clone())
            .unwrap_or_else(|| panic!("{m} has options"))
    };
    // Every tier's own models keep theirs.
    for (m, o) in saved_options() {
        assert_eq!(of(m), o);
    }
    let mind = of("Qwen3-VL-4B-Instruct-GGUF");
    assert_eq!(mind["ctx_size"], mind_context(Tier::Tiny));
    assert_eq!(mind["evict_idle_timeout"], 180);
    assert_eq!(of("nomic-embed-text-v1-GGUF")["ctx_size"], RECALL_CONTEXT);
    assert_eq!(of("Whisper-Tiny")["evict_idle_timeout"], 120);
    // Each model once.
    let mut ids: Vec<&String> = saved.iter().map(|(m, _)| m).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), saved.len());
}

#[test]
fn voice_alone_when_read_set_plus_voice_exceeds_budget() {
    // Standard runs the voice beside the others; a bigger mind on a 28 GiB computer does not
    // leave room for it while reading.
    let b = budget(28 * GIB, 0, 0);
    let preset = Selection::preset(Tier::Standard);
    assert!(alone_for(&preset, &b).is_empty());
    let big = with_choice(&preset, ComponentRole::Mind, "Gemma-4-26B-A4B-it-GGUF");
    assert_eq!(alone_for(&big, &b), ["kokoro-v1"]);
    // A smaller mind leaves room.
    let small = with_choice(&preset, ComponentRole::Mind, "Qwen3.5-4B-MTP-GGUF");
    assert!(alone_for(&small, &b).is_empty());
    // Tiny and Small keep the voice alone whatever is chosen.
    let tiny = with_choice(
        &Selection::preset(Tier::Tiny),
        ComponentRole::Speech,
        "Whisper-Tiny",
    );
    assert_eq!(alone_for(&tiny, &budget(64 * GIB, 0, 0)), alone(Tier::Tiny));
}

#[test]
fn selection_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    installed(tmp.path(), Tier::Standard);
    let mut s = with_choice(
        &Selection::preset(Tier::Standard),
        ComponentRole::Mind,
        "Qwen3.5-4B-MTP-GGUF",
    );
    s.pending = Some(Pending {
        role: ComponentRole::Speech,
        model: "Whisper-Large-v3".into(),
        from: "Whisper-Small".into(),
        paused: true,
    });
    s.old = Some(OldModel {
        role: ComponentRole::Mind,
        model: "Qwen3.5-9B-GGUF".into(),
    });
    selection::write(tmp.path(), &s).unwrap();
    assert_eq!(selection::read(tmp.path()), s);
    // A file that cannot be read is the preset, never a failed launch.
    std::fs::write(tmp.path().join("selection.json"), "{not json").unwrap();
    assert_eq!(
        selection::read(tmp.path()),
        Selection::preset(Tier::Standard)
    );
}

#[test]
fn a_new_tier_keeps_only_the_finding_model() {
    let tmp = tempfile::tempdir().unwrap();
    installed(tmp.path(), Tier::Standard);
    let mut s = with_choice(
        &Selection::preset(Tier::Standard),
        ComponentRole::Mind,
        "Qwen3.5-4B-MTP-GGUF",
    );
    s = with_choice(&s, ComponentRole::Recall, "Qwen3-Embedding-4B-GGUF");
    selection::write(tmp.path(), &s).unwrap();
    installed(tmp.path(), Tier::Tiny);
    let moved = selection::read(tmp.path());
    assert_eq!(moved.tier, Tier::Tiny);
    let p = picks(&moved);
    assert_eq!(
        Some(p.mind.as_str()),
        tier::model_for(Tier::Tiny, ComponentRole::Mind)
    );
    assert_eq!(p.recall, "Qwen3-Embedding-4B-GGUF");
}

#[tokio::test]
async fn mind_follows_selection() {
    let fake = common::Fake::started(Some("downloaded=all")).await;
    let s = with_choice(
        &with_choice(
            &Selection::preset(Tier::Standard),
            ComponentRole::Recall,
            "Qwen3-Embedding-0.6B-GGUF",
        ),
        ComponentRole::Speech,
        "Whisper-Tiny",
    );
    let mind = LemonadeMind::new(fake.client.clone(), picks(&s), Tier::Standard);
    mind.embed(&["hello".into()]).await.unwrap();
    let seen = fake.seen("/v1/embeddings").await;
    assert_eq!(
        seen.last().unwrap()["body"]["model"],
        "Qwen3-Embedding-0.6B-GGUF"
    );
    // Embedding with another model, for a re-read, names that model.
    mind.embed_with("Qwen3-Embedding-0.6B-GGUF", &["again".into()])
        .await
        .unwrap();
    let picked = with_choice(
        &Selection::preset(Tier::Standard),
        ComponentRole::Mind,
        "Qwen3.5-2B-GGUF",
    );
    assert_eq!(picks(&picked).mind, "Qwen3.5-2B-GGUF");
    let small_mind = LemonadeMind::new(fake.client.clone(), picks(&picked), Tier::Standard);
    let cx = nocturne_engine::ExtractContext {
        item_kind: nocturne_contract::ItemKind::Text,
        kept_at: "2026-11-10T09:30:00Z".parse().unwrap(),
        today: "2026-11-10".parse().unwrap(),
        known_entities: Vec::new(),
    };
    let _ = small_mind.extract("Sam likes tea", &cx).await;
    let chats = fake.seen("/v1/chat/completions").await;
    assert_eq!(chats.last().unwrap()["body"]["model"], "Qwen3.5-2B-GGUF");
    let loads = fake.seen("/v1/load").await;
    assert!(
        loads
            .iter()
            .all(|l| l["body"]["model_name"] != "Qwen3.5-9B-GGUF"),
        "the tier's own mind is never loaded: {loads:?}"
    );
}
