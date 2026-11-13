//! Whether a model fits this computer for a job: the memory budget, the busiest moment of the
//! chosen set, the engine's own size cap and the free disk.

use nocturne_contract::{Job, ModelChoice};

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

/// The limits for a computer with `ram` bytes. The budget is not set yet.
pub fn budget(ram: u64, largest_pool: u64, disk_free: u64) -> Budget {
    Budget {
        ram,
        budget: 0,
        disk_free,
        engine_cap: largest_pool.max(ram / 10 * 8),
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
