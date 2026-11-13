//! Whether a model fits this computer for a job: the memory budget, the busiest moment of the
//! chosen set, the engine's own size cap and the free disk.

use nocturne_contract::{Fit, Job, ModelChoice};

use crate::client::{ModelInfo, SystemInfo};
use crate::selection::{Picks, Selection};

/// The limits a choice is judged against, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub ram: u64,
    pub budget: u64,
    pub disk_free: u64,
    /// The largest model the engine accepts: max(largest pool, 0.8 x RAM).
    pub engine_cap: u64,
}

/// Why a catalogue model is not offered for a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exclusion {
    Unavailable,
}

const GIB: u64 = 1 << 30;

/// The limits for a computer with `ram` bytes. The memory budget is
/// `min(0.55 x RAM + 1.7 GiB, RAM - 1.0 GiB)`: every tier preset fits at its own minimum, with
/// room left for the system and the app (plan 13, Decisions, "Fit budget").
pub fn budget(ram: u64, largest_pool: u64, disk_free: u64) -> Budget {
    let budget = (ram / 100 * 55 + GIB / 10 * 17).min(ram.saturating_sub(GIB));
    Budget {
        ram,
        budget,
        disk_free,
        engine_cap: largest_pool.max(ram / 10 * 8),
    }
}

/// How `need` bytes at the busiest moment sit in `b`: Fits at most 0.85 of the budget, Tight at
/// most the budget, Won't fit above it.
pub fn fit_for(need: u64, b: &Budget) -> Fit {
    if need <= b.budget / 100 * 85 {
        Fit::Fits
    } else if need <= b.budget {
        Fit::Tight
    } else {
        Fit::WontFit
    }
}

/// The memory the busiest moment of `picks` needs. Not measured yet.
pub fn peak(picks: &Picks, alone_voice: bool) -> u64 {
    let _ = (picks, alone_voice);
    0
}

/// The models offered for `job`, each with its fit. None yet.
pub fn choices(
    job: Job,
    catalogue: &[ModelInfo],
    info: &SystemInfo,
    current: &Selection,
    b: &Budget,
) -> Vec<ModelChoice> {
    let _ = (job, catalogue, info, current, b);
    Vec::new()
}

/// Whether `m` may be offered for `job` at all. Nothing is offered yet.
pub fn qualifies(job: Job, m: &ModelInfo) -> Result<(), Exclusion> {
    let _ = (job, m);
    Err(Exclusion::Unavailable)
}
