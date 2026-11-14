//! How each job picks its words, per tier. The presets are what every request has always sent:
//! a temperature, and nothing else, so llama.cpp's defaults hold for the rest. A sweep sets
//! them from [`ENV`], which only tests and evaluation runs set.

use nocturne_contract::Tier;
use serde::Deserialize;

use crate::client::Sampling;

/// JSON such as `{"answer": {"temperature": 0.7, "top_k": 20, "seed": 7}}`. Each job named
/// takes the values given in place of its preset's own; anything unreadable is ignored.
pub const ENV: &str = "NOCTURNE_TEST_SAMPLING";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Presets {
    pub extract: Sampling,
    pub answer: Sampling,
    pub image: Sampling,
    pub headline: Sampling,
}

const TODAY: Presets = Presets {
    extract: Sampling::temperature(0.0),
    answer: Sampling::temperature(0.2),
    image: Sampling::temperature(0.0),
    headline: Sampling::temperature(0.4),
};

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Overrides {
    #[serde(default)]
    extract: Sampling,
    #[serde(default)]
    answer: Sampling,
    #[serde(default)]
    image: Sampling,
    #[serde(default)]
    headline: Sampling,
}

impl Presets {
    pub fn for_tier(tier: Tier) -> Presets {
        match tier {
            Tier::Tiny | Tier::Small | Tier::Standard | Tier::Large | Tier::Unsupported => TODAY,
        }
    }

    /// The tier's presets, with what [`ENV`] sets laid over them.
    pub fn from_env(tier: Tier) -> Presets {
        let presets = Presets::for_tier(tier);
        match std::env::var(ENV) {
            Ok(json) => presets.overlaid(&json).unwrap_or_else(|e| {
                tracing::warn!("{ENV} is not readable and is ignored: {e}");
                presets
            }),
            Err(_) => presets,
        }
    }

    fn overlaid(self, json: &str) -> Result<Presets, serde_json::Error> {
        let o: Overrides = serde_json::from_str(json)?;
        Ok(Presets {
            extract: self.extract.overlaid(&o.extract),
            answer: self.answer.overlaid(&o.answer),
            image: self.image.overlaid(&o.image),
            headline: self.headline.overlaid(&o.headline),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{ChatRequest, Message};

    #[test]
    fn every_tier_sends_what_it_always_sent() {
        for tier in [Tier::Tiny, Tier::Small, Tier::Standard, Tier::Large] {
            let p = Presets::for_tier(tier);
            let body = |s: &Sampling| {
                serde_json::to_value(ChatRequest::new("m", vec![Message::user("q")]).sampled(s))
                    .unwrap()
            };
            for (s, t) in [
                (p.extract, 0.0),
                (p.answer, 0.2),
                (p.image, 0.0),
                (p.headline, 0.4),
            ] {
                let b = body(&s);
                assert_eq!(b["temperature"], serde_json::json!(t as f32), "{tier:?}");
                for key in [
                    "top_k",
                    "top_p",
                    "seed",
                    "presence_penalty",
                    "repeat_penalty",
                ] {
                    assert!(b.get(key).is_none(), "{key} for {tier:?}");
                }
            }
        }
    }

    #[test]
    fn a_sweep_overrides_only_what_it_names() {
        let p = TODAY
            .overlaid(r#"{"answer": {"temperature": 0.7, "top_k": 20, "top_p": 0.8, "seed": 7}}"#)
            .unwrap();
        assert_eq!(p.answer.temperature, Some(0.7));
        assert_eq!(p.answer.top_k, Some(20));
        assert_eq!(p.answer.top_p, Some(0.8));
        assert_eq!(p.answer.seed, Some(7));
        assert_eq!(p.answer.presence_penalty, None);
        assert_eq!(p.extract, TODAY.extract);
        assert_eq!(p.headline, TODAY.headline);
        let body = serde_json::to_value(
            ChatRequest::new("m", vec![Message::user("q")]).sampled(&p.answer),
        )
        .unwrap();
        assert_eq!(body["top_k"], 20);
        assert_eq!(body["seed"], 7);
        assert!(TODAY.overlaid(r#"{"answr": {}}"#).is_err());
        assert!(TODAY.overlaid(r#"{"answer": {"top_kk": 1}}"#).is_err());
    }
}
