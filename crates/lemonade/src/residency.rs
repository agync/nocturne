//! Which models stay in memory, left to the engine's own eviction wherever it can do the job.
//!
//! The engine unloads a model that has been idle for its `evict_idle_timeout` (300 seconds by
//! default) when `auto_evict` is on, which Nocturne always sets. It is a per-model option, saved
//! in the engine's `recipe_options.json`. The voice is used once a day for the spoken page and the
//! speech model only while a recording is read, yet each holds hundreds of megabytes (the voice
//! up to 3 GB), so [`saved_options`] gives them short timeouts, gives the tiny mind a shorter one
//! than the engine's, and saves every model's context so that a load the engine makes by itself
//! never sizes it to fill the free memory. The next request loads a model again with them. On a
//! Mac the engine has no memory-pressure eviction, so these timers are what frees the memory.

use nocturne_contract::{ComponentRole, Tier};
use serde_json::{Map, Value, json};

use crate::tier;

/// Seconds a model in `role` on `tier` may sit idle before the engine unloads it, where the
/// engine's own 300 seconds is too long.
pub fn evict_after(tier: Tier, role: ComponentRole) -> Option<u32> {
    match role {
        ComponentRole::Voice => Some(60),
        ComponentRole::Speech => Some(120),
        ComponentRole::Mind if tier == Tier::Tiny => Some(180),
        ComponentRole::Mind | ComponentRole::Recall => None,
    }
}

/// The context the mind of `tier` is loaded with.
pub fn mind_context(tier: Tier) -> u32 {
    if tier == Tier::Large { 16384 } else { 8192 }
}

/// The options to save for each model of every tier, so the engine applies them whenever it
/// loads one by itself.
pub fn saved_options() -> Vec<(&'static str, Map<String, Value>)> {
    let mut out: Vec<(&'static str, Map<String, Value>)> = Vec::new();
    let tiers = [Tier::Large, Tier::Standard, Tier::Small, Tier::Tiny];
    for (t, c) in tiers
        .into_iter()
        .flat_map(|t| tier::components(t).into_iter().map(move |c| (t, c)))
    {
        if out.iter().any(|(m, _)| *m == c.model) {
            continue;
        }
        let mut options = Map::new();
        if let Some(secs) = evict_after(t, c.role) {
            options.insert("evict_idle_timeout".into(), json!(secs));
        }
        match c.role {
            ComponentRole::Recall => {
                options.insert("ctx_size".into(), json!(tier::RECALL_CONTEXT));
            }
            ComponentRole::Mind => {
                options.insert("ctx_size".into(), json!(mind_context(t)));
            }
            ComponentRole::Speech | ComponentRole::Voice => {}
        }
        if !options.is_empty() {
            out.push((c.model, options));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options_for(model: &str) -> Option<Map<String, Value>> {
        saved_options()
            .into_iter()
            .find(|(m, _)| *m == model)
            .map(|(_, o)| o)
    }

    #[test]
    fn the_voice_speech_and_tiny_mind_go_sooner_and_the_other_minds_keep_the_engine_default() {
        assert_eq!(options_for("kokoro-v1").unwrap()["evict_idle_timeout"], 60);
        for speech in ["Whisper-Base", "Whisper-Small", "Whisper-Large-v3-Turbo"] {
            assert_eq!(options_for(speech).unwrap()["evict_idle_timeout"], 120);
        }
        assert_eq!(
            options_for("Qwen3.5-2B-GGUF").unwrap()["evict_idle_timeout"],
            180
        );
        assert!(
            !options_for("Qwen3.5-9B-GGUF")
                .unwrap()
                .contains_key("evict_idle_timeout")
        );
    }

    #[test]
    fn every_mind_has_its_context_saved() {
        for t in [Tier::Large, Tier::Standard, Tier::Small, Tier::Tiny] {
            let mind = tier::model_for(t, ComponentRole::Mind).unwrap();
            assert_eq!(options_for(mind).unwrap()["ctx_size"], mind_context(t));
        }
        assert_eq!(mind_context(Tier::Large), 16384);
    }

    #[test]
    fn the_recall_model_is_saved_once_with_its_context() {
        let recall: Vec<_> = saved_options()
            .into_iter()
            .filter(|(m, _)| *m == tier::RECALL.model)
            .collect();
        assert_eq!(recall.len(), 1);
        assert_eq!(recall[0].1["ctx_size"], tier::RECALL_CONTEXT);
        assert!(!recall[0].1.contains_key("evict_idle_timeout"));
    }
}
