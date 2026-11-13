//! Retrieval: full-text and vector search fused by rank, with status ordering.

/// All current vectors, normalised, held in memory so a search is one pass over them.
pub(crate) struct VecIndex {
    pub dim: usize,
    pub ids: Vec<String>,
    pub data: Vec<f32>,
}
