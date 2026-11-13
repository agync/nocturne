//! Measured memory, test status, context and vector size for the catalogue models Nocturne knows,
//! so the picker can judge fit before anything is downloaded. Empty until it is filled.

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
