//! `nocturne-setup.json`: what setup chose and how far it got. It sits beside the engine's own
//! `config.json` and is ours alone.

use std::collections::BTreeMap;
use std::path::Path;

use nocturne_contract::Tier;
use serde::{Deserialize, Serialize};

use crate::error::RuntimeError;

const FILE: &str = "nocturne-setup.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupRecord {
    pub version: u32,
    pub tier: Option<Tier>,
    /// True once every component is in place.
    pub done: bool,
    /// The backend chosen for each recipe, for example `llamacpp` to `vulkan`.
    pub backends: BTreeMap<String, String>,
}

pub fn read(config_dir: &Path) -> Option<SetupRecord> {
    let bytes = std::fs::read(config_dir.join(FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn write(config_dir: &Path, record: &SetupRecord) -> Result<(), RuntimeError> {
    std::fs::create_dir_all(config_dir)?;
    let tmp = config_dir.join(format!("{FILE}.tmp"));
    let text = serde_json::to_string_pretty(record).map_err(|e| RuntimeError::Io(e.to_string()))?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, config_dir.join(FILE))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_absence() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read(tmp.path()).is_none());
        let mut record = SetupRecord {
            version: 1,
            tier: Some(Tier::Standard),
            done: true,
            backends: BTreeMap::new(),
        };
        record.backends.insert("llamacpp".into(), "vulkan".into());
        write(tmp.path(), &record).unwrap();
        assert_eq!(read(tmp.path()), Some(record));
    }
}
