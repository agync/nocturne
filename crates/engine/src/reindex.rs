//! Reading memories again for a new finding model: the vectors being built (`embeddings_next`)
//! and the progress row (`reindex_state`). The old index stays in use until the swap.

use jiff::Timestamp;
use nocturne_contract::ReindexPhase;

/// A re-read under way or just finished, one row.
#[derive(Debug, Clone, PartialEq)]
pub struct ReindexRow {
    pub model: String,
    pub from_model: String,
    pub dim: Option<u32>,
    pub total: u32,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
    pub phase: ReindexPhase,
}
