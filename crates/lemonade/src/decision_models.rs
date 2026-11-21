//! The models that can answer a decision (a typed question about a short text, answered with
//! probabilities), and what each needs. Which one serves a computer is [`crate::fit::decider_for`].
//!
//! Nothing here is offered to a person or downloaded by setup: the evaluation registers and
//! loads these by hand.

use nocturne_engine::decide::Backend;

/// A decision model: how the engine registers it and what it costs to hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecisionModel {
    pub id: &'static str,
    pub recipe: &'static str,
    /// The `repo:file` the engine fetches. None for a model the engine lists itself.
    pub checkpoint: Option<&'static str>,
    pub labels: &'static [&'static str],
    /// Download size in bytes.
    pub bytes: u64,
    pub backend: Backend,
    pub licence: &'static str,
}

/// `d3-lite` on the NPU through FastFlowLM. The engine lists it from `flm list`, so it needs no
/// checkpoint. About 0.72 GiB to download.
pub const D3_LITE_FLM: DecisionModel = DecisionModel {
    id: "d3-lite-FLM",
    recipe: "flm",
    checkpoint: None,
    labels: &["classification", "systemone"],
    bytes: 773_000_000,
    backend: Backend::Flm,
    licence: "Apache-2.0",
};

/// Liquid `d1-Omni-2B` on llama.cpp (GGUF type `lfm3-d1`), on whatever graphics backend
/// `tier::choose_backend` gives. Its licence is the LFM Open License v1.0, which is not an OSI
/// licence, so it is evaluated and never bundled.
pub const D1_OMNI_2B: DecisionModel = DecisionModel {
    id: "user.d1-Omni-2B",
    recipe: "llamacpp",
    checkpoint: Some("LiquidAI/d1-Omni-2B-GGUF:d1-Omni-2B-Q8_0.gguf"),
    labels: &["classification", "systemone"],
    bytes: 1_846_241_184,
    backend: Backend::LlamaCpp,
    licence: "LFM Open License v1.0",
};

/// `d3-lite` on the CPU. An evaluation arm only: the hardware rule never picks it.
pub const D3_LITE_CPU: DecisionModel = DecisionModel {
    id: "user.vllm-d3-lite",
    recipe: "llamacpp",
    checkpoint: Some("ggml-org/vllm-d3-lite-GGUF:vllm-d3-lite-Q8_0.gguf"),
    labels: &["classification", "systemone"],
    bytes: 1_082_017_408,
    backend: Backend::LlamaCpp,
    licence: "Apache-2.0",
};

pub const ALL: &[&DecisionModel] = &[&D3_LITE_FLM, &D1_OMNI_2B, &D3_LITE_CPU];
