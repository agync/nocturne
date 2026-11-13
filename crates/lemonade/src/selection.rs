//! `selection.json`: the model chosen for each job. The tier is the preset; `custom` overrides
//! single jobs. It sits beside `nocturne-setup.json` and is ours alone. The tier always follows
//! the installed one: when setup moves to another tier, the person's other choices go with the
//! old tier, except the finding model, whose vectors fill the store.

use std::collections::BTreeMap;
use std::path::Path;

use nocturne_contract::{ComponentRole, Tier};
use serde::{Deserialize, Serialize};

use crate::error::RuntimeError;
use crate::tier;

const FILE: &str = "selection.json";

/// Held for every read-modify-write of the file in this process.
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A change of model waiting for its download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    pub role: ComponentRole,
    pub model: String,
    pub from: String,
    /// The person paused the download; a restart does not carry it on by itself.
    #[serde(default)]
    pub paused: bool,
    /// The download stopped by itself; it waits for the person to try again.
    #[serde(default)]
    pub failed: bool,
}

/// The model a job used before a change, kept until the person removes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OldModel {
    pub role: ComponentRole,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub tier: Tier,
    #[serde(default)]
    pub custom: BTreeMap<ComponentRole, String>,
    #[serde(default)]
    pub pending: Option<Pending>,
    #[serde(default)]
    pub old: Option<OldModel>,
}

impl Selection {
    /// The tier's own models, nothing changed.
    pub fn preset(tier: Tier) -> Self {
        Selection {
            tier,
            custom: BTreeMap::new(),
            pending: None,
            old: None,
        }
    }
}

/// The model in use for each job. Empty where the tier has none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picks {
    pub mind: String,
    pub speech: String,
    pub voice: String,
    pub recall: String,
}

impl Picks {
    pub fn get(&self, role: ComponentRole) -> &str {
        match role {
            ComponentRole::Mind => &self.mind,
            ComponentRole::Speech => &self.speech,
            ComponentRole::Voice => &self.voice,
            ComponentRole::Recall => &self.recall,
        }
    }
}

/// The selection on disk, on the installed tier. No file, or one that cannot be read, is the
/// tier's preset.
pub fn read(config_dir: &Path) -> Selection {
    let tier = crate::record::read(config_dir)
        .and_then(|r| r.working_tier())
        .unwrap_or(Tier::Unsupported);
    let saved: Option<Selection> = std::fs::read(config_dir.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    match saved {
        Some(s) if s.tier == tier => s,
        Some(s) => {
            let mut moved = Selection::preset(tier);
            if let Some(recall) = s.custom.get(&ComponentRole::Recall) {
                moved.custom.insert(ComponentRole::Recall, recall.clone());
            }
            moved.pending = s.pending.filter(|p| p.role == ComponentRole::Recall);
            moved.old = s.old;
            moved
        }
        None => Selection::preset(tier),
    }
}

/// Removes `selection.json`: every job goes back to its tier's own model.
pub fn clear(config_dir: &Path) -> Result<(), RuntimeError> {
    let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    match std::fs::remove_file(config_dir.join(FILE)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Every read-modify-write of `selection.json` in this process goes through here, one at a time,
/// so a download finishing, a setup ending and the person's own change never lose each other's
/// edits. Returns the selection as written.
pub fn update(
    config_dir: &Path,
    f: impl FnOnce(&mut Selection),
) -> Result<Selection, RuntimeError> {
    let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = read(config_dir);
    f(&mut s);
    write(config_dir, &s)?;
    Ok(s)
}

pub fn write(config_dir: &Path, s: &Selection) -> Result<(), RuntimeError> {
    std::fs::create_dir_all(config_dir)?;
    let tmp = config_dir.join(format!("{FILE}.tmp"));
    let text = serde_json::to_string_pretty(s).map_err(|e| RuntimeError::Io(e.to_string()))?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, config_dir.join(FILE))?;
    Ok(())
}

pub fn picks(s: &Selection) -> Picks {
    let pick = |role| {
        s.custom
            .get(&role)
            .cloned()
            .or_else(|| tier::model_for(s.tier, role).map(str::to_string))
            .unwrap_or_default()
    };
    Picks {
        mind: pick(ComponentRole::Mind),
        speech: pick(ComponentRole::Speech),
        voice: pick(ComponentRole::Voice),
        recall: pick(ComponentRole::Recall),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_are_the_tier_preset_with_custom_jobs_on_top() {
        let mut s = Selection::preset(Tier::Standard);
        let preset = picks(&s);
        for role in [
            ComponentRole::Mind,
            ComponentRole::Speech,
            ComponentRole::Voice,
            ComponentRole::Recall,
        ] {
            assert_eq!(
                Some(preset.get(role)),
                tier::model_for(Tier::Standard, role)
            );
        }
        s.custom.insert(ComponentRole::Mind, "Other-GGUF".into());
        assert_eq!(picks(&s).mind, "Other-GGUF");
        assert_eq!(picks(&s).recall, preset.recall);
        assert_eq!(picks(&Selection::preset(Tier::Unsupported)).mind, "");
    }

    #[test]
    fn read_is_the_installed_tier_and_write_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(read(tmp.path()).tier, Tier::Unsupported);
        let mut s = Selection::preset(Tier::Small);
        s.custom
            .insert(ComponentRole::Speech, "Whisper-Base".into());
        write(tmp.path(), &s).unwrap();
        let back: Selection =
            serde_json::from_slice(&std::fs::read(tmp.path().join(FILE)).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
