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
    /// The tier setup is installing, or last installed.
    pub tier: Option<Tier>,
    /// True once every component of `tier` is in place.
    pub done: bool,
    /// The last tier that was fully installed. A change of tier overwrites `tier` and clears
    /// `done` before it downloads anything; this is what keeps working if it never finishes.
    #[serde(default)]
    pub completed: Option<Tier>,
    /// The backend chosen for each recipe, for example `llamacpp` to `vulkan`.
    pub backends: BTreeMap<String, String>,
}

impl SetupRecord {
    /// The tier that is fully installed, even while another is being fetched. Records written
    /// before `completed` existed are read by their `done` flag.
    pub fn working_tier(&self) -> Option<Tier> {
        self.completed.or(if self.done { self.tier } else { None })
    }
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
            completed: Some(Tier::Standard),
            backends: BTreeMap::new(),
        };
        record.backends.insert("llamacpp".into(), "vulkan".into());
        write(tmp.path(), &record).unwrap();
        assert_eq!(read(tmp.path()), Some(record));
    }

    #[test]
    fn the_working_tier_survives_an_unfinished_change() {
        let mut record = SetupRecord {
            version: 1,
            tier: Some(Tier::Small),
            done: false,
            completed: Some(Tier::Standard),
            backends: BTreeMap::new(),
        };
        assert_eq!(record.working_tier(), Some(Tier::Standard));
        record.completed = None;
        assert_eq!(record.working_tier(), None, "nothing finished yet");
        record.done = true;
        assert_eq!(
            record.working_tier(),
            Some(Tier::Small),
            "an older record is read by its done flag"
        );
    }

    #[test]
    fn an_older_record_without_the_field_still_reads() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("nocturne-setup.json"),
            r#"{"version":1,"tier":"standard","done":true,"backends":{}}"#,
        )
        .unwrap();
        assert_eq!(
            read(tmp.path()).unwrap().working_tier(),
            Some(Tier::Standard)
        );
    }
}
