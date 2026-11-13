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
//!
//! What the engine cannot be told is that two models must not share a computer with little
//! memory. On the tiny and small tiers the voice runs alone: [`alone`] names it, and the client
//! unloads every other model that is not busy before the voice is used.

use nocturne_contract::{ComponentRole, Tier};
use serde_json::{Map, Value, json};

use crate::fit::Budget;
use crate::selection::{self, Selection};
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

/// Models that run with nothing else loaded on `tier`. On the tiny and small tiers the voice,
/// the largest in memory while it speaks, never shares the computer with the mind.
pub fn alone(tier: Tier) -> Vec<String> {
    match tier {
        Tier::Tiny | Tier::Small => tier::model_for(tier, ComponentRole::Voice)
            .map(|m| vec![m.to_string()])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The options to save for the models of `s`: those of every tier, and for each model the
/// person chose instead, the options its job's model gets on `s`'s tier.
pub fn saved_options_for(s: &Selection) -> Vec<(String, Map<String, Value>)> {
    let mut out: Vec<(String, Map<String, Value>)> = saved_options()
        .into_iter()
        .map(|(m, o)| (m.to_string(), o))
        .collect();
    for (role, model) in &s.custom {
        if out.iter().any(|(m, _)| m == model) {
            continue;
        }
        let mut options = Map::new();
        if let Some(secs) = evict_after(s.tier, *role) {
            options.insert("evict_idle_timeout".into(), json!(secs));
        }
        match role {
            ComponentRole::Recall => {
                options.insert("ctx_size".into(), json!(tier::RECALL_CONTEXT));
            }
            ComponentRole::Mind => {
                options.insert("ctx_size".into(), json!(mind_context_for(s)));
            }
            ComponentRole::Speech | ComponentRole::Voice => {}
        }
        if !options.is_empty() {
            out.push((model.clone(), options));
        }
    }
    out
}

/// Models that run with nothing else loaded for `s`: the tier's rule, and the voice too when
/// the person's own choices would not fit beside it at the busiest moment of reading.
pub fn alone_for(s: &Selection, b: &Budget) -> Vec<String> {
    let mut out = alone(s.tier);
    if s.custom.is_empty() {
        return out;
    }
    let picks = selection::picks(s);
    let mem = |m: &str| crate::fit::memory_of(m, None);
    let read = crate::fit::read_set_with(&picks, mem);
    if !picks.voice.is_empty() && read + mem(&picks.voice) > b.budget && !out.contains(&picks.voice)
    {
        out.push(picks.voice.clone());
    }
    out
}

/// The context the mind of `s` is loaded with: its tier's, whichever model answers.
pub fn mind_context_for(s: &Selection) -> u32 {
    mind_context(s.tier)
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

    #[test]
    fn existing_tier_behaviour_unchanged() {
        let b = crate::fit::budget(16 << 30, 0, 100 << 30);
        let today: Vec<(String, Map<String, Value>)> = saved_options()
            .into_iter()
            .map(|(m, o)| (m.to_string(), o))
            .collect();
        for t in [
            Tier::Large,
            Tier::Standard,
            Tier::Small,
            Tier::Tiny,
            Tier::Unsupported,
        ] {
            let s = Selection::preset(t);
            assert_eq!(saved_options_for(&s), today, "{t:?}");
            assert_eq!(alone_for(&s, &b), alone(t), "{t:?}");
            assert_eq!(mind_context_for(&s), mind_context(t), "{t:?}");
        }
    }

    #[test]
    fn the_voice_runs_alone_on_the_tiny_and_small_tiers() {
        assert_eq!(alone(Tier::Tiny), ["kokoro-v1"]);
        assert_eq!(alone(Tier::Small), ["kokoro-v1"]);
        assert!(alone(Tier::Standard).is_empty());
        assert!(alone(Tier::Unsupported).is_empty());
    }
}
