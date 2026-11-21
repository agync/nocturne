//! Measured memory, test status, context and vector size for the catalogue models Nocturne knows,
//! so the picker can judge fit before anything is downloaded, and the memory each tier preset
//! needs at its busiest. Memory is the peak while the job runs, at the context Nocturne loads
//! the model with (8192 for answering); figures are estimates (size + 1.2 GB for a chat model,
//! + 0.35 GB more with `mtp`) unless `measured` says otherwise.

use nocturne_contract::Tier;

/// The memory a tier preset needs at its busiest moment: mind, recall and speech together (the
/// voice runs alone on Tiny and Small), in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresetPeak {
    pub tier: Tier,
    pub busiest: u64,
    /// Measured in an end-to-end run; otherwise estimated from the models' sizes.
    pub measured: bool,
}

/// Tiny 5.2 GB and Small 8.1 GB measured (answering, Linux, integrated GPU); Standard 10.6 GB
/// and Large 29.3 GB estimated until measured.
pub const PRESET_PEAKS: &[PresetPeak] = &[
    PresetPeak {
        tier: Tier::Tiny,
        busiest: 5_200_000_000,
        measured: true,
    },
    PresetPeak {
        tier: Tier::Small,
        busiest: 8_100_000_000,
        measured: true,
    },
    PresetPeak {
        tier: Tier::Standard,
        busiest: 10_600_000_000,
        measured: false,
    },
    PresetPeak {
        tier: Tier::Large,
        busiest: 29_300_000_000,
        measured: false,
    },
];

/// The voice speaking alone (a page read aloud), measured.
pub const VOICE_ALONE: u64 = 3_400_000_000;

pub fn preset_peak(tier: Tier) -> Option<&'static PresetPeak> {
    PRESET_PEAKS.iter().find(|p| p.tier == tier)
}

/// What Nocturne knows about one catalogue model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    pub id: &'static str,
    /// Memory in use while loaded, in bytes.
    pub memory: u64,
    /// `memory` was measured in an end-to-end run rather than estimated.
    pub measured: bool,
    /// Tested with Nocturne.
    pub tested: bool,
    /// The model takes the context Nocturne loads it with.
    pub ctx_ok: bool,
    /// Vector size, for a finding model.
    pub dim: Option<u32>,
}

const fn row(id: &'static str, gb_x100: u64, measured: bool, tested: bool) -> Entry {
    Entry {
        id,
        memory: gb_x100 * 10_000_000,
        measured,
        tested,
        ctx_ok: true,
        dim: None,
    }
}

const fn finding(id: &'static str, gb_x100: u64, measured: bool, tested: bool, dim: u32) -> Entry {
    Entry {
        dim: Some(dim),
        ..row(id, gb_x100, measured, tested)
    }
}

/// The catalogue models each job may use at the pinned engine (model-picker research, section 3).
pub const TABLE: &[Entry] = &[
    // Answering questions.
    row("Qwen3.5-0.8B-GGUF", 200, false, false),
    row("MiniCPM-V-4.6-GGUF", 280, false, false),
    row("Qwen3.5-2B-GGUF", 320, false, true),
    row("Ministral-3-3B-Instruct-2512-GGUF", 420, false, false),
    row("Qwen3-VL-4B-Instruct-GGUF", 450, false, false),
    row("Gemma-3-4b-it-GGUF", 450, false, false),
    row("Qwen3.5-4B-MTP-GGUF", 490, true, true),
    row("Gemma-4-E4B-it-GGUF", 720, false, false),
    row("Qwen3-VL-8B-Instruct-GGUF", 740, false, false),
    row("Qwen3.5-9B-GGUF", 810, false, true),
    row("Gemma-4-26B-A4B-it-GGUF", 1950, false, false),
    row("Qwen3.6-27B-MTP-GGUF", 2050, false, false),
    row("Qwen3.6-35B-A3B-MTP-GGUF", 2550, false, true),
    // Reading recordings.
    row("Whisper-Tiny", 30, false, true),
    row("Whisper-Base", 37, true, true),
    row("Whisper-Small", 80, false, true),
    row("Whisper-Medium", 200, false, true),
    row("Whisper-Large-v3-Turbo", 210, false, true),
    row("Whisper-Large-v3", 380, false, true),
    // Reading recordings on the NPU: 0.3 GB in its process and 0.7 GB of NPU buffers, measured.
    row("whisper-v3-turbo-FLM", 100, true, true),
    // Reading items on the NPU: its process while reading, measured.
    row("gemma4-it-e4b-FLM", 980, true, true),
    // Reading aloud: the voice speaking alone, whatever its size.
    row("kokoro-v1", 340, true, true),
    // Answering a decision. Untested; each is the download plus a cache for an 8192-token
    // context: about 0.3 GB for the 1B-class d3-lite, 0.2 GB for the 2B d1-Omni, whose
    // convolution layers keep no cache, and compute buffers on top.
    row("d3-lite-FLM", 110, false, false),
    row("user.d1-Omni-2B", 220, false, false),
    row("user.vllm-d3-lite", 140, false, false),
    // Finding memories.
    finding("nomic-embed-text-v1-GGUF", 40, false, false, 768),
    finding("nomic-embed-text-v2-moe-GGUF", 120, false, false, 768),
    finding("Qwen3-Embedding-0.6B-GGUF", 170, true, true, 1024),
    finding("Qwen3-Embedding-4B-GGUF", 500, false, false, 2560),
    finding("Qwen3-Embedding-8B-GGUF", 900, false, false, 4096),
];

pub fn lookup(id: &str) -> Option<&'static Entry> {
    TABLE.iter().find(|e| e.id == id)
}

/// Memory for a model the table does not know: its download plus 1.2 GB, and 0.35 GB more for
/// a model that drafts with `mtp`.
pub fn estimate(size_bytes: u64, labels: &[String]) -> u64 {
    let mtp = labels.iter().any(|l| l == "mtp");
    size_bytes + 1_200_000_000 + if mtp { 350_000_000 } else { 0 }
}
