//! Measured memory, test status, context and vector size for the catalogue models Nocturne knows,
//! so the picker can judge fit before anything is downloaded, and the memory each tier preset
//! needs at its busiest. The per-model rows are not filled yet.

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
    /// Tested with Nocturne.
    pub tested: bool,
    /// The model takes the context Nocturne loads it with.
    pub ctx_ok: bool,
    /// Vector size, for a finding model.
    pub dim: Option<u32>,
}

pub const TABLE: &[Entry] = &[];

pub fn lookup(id: &str) -> Option<&'static Entry> {
    TABLE.iter().find(|e| e.id == id)
}
