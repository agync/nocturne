//! `selection.json`: the model chosen for each job. The tier is the preset; `custom` overrides
//! single jobs. It sits beside `nocturne-setup.json` and is ours alone.

use std::collections::BTreeMap;
use std::path::Path;

use nocturne_contract::{ComponentRole, Tier};
use serde::{Deserialize, Serialize};

use crate::error::RuntimeError;
use crate::tier;

const FILE: &str = "selection.json";

/// A change of model waiting for its download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    pub role: ComponentRole,
    pub model: String,
    pub from: String,
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

/// The selection on disk. For now: the preset of the installed tier.
pub fn read(config_dir: &Path) -> Selection {
    let tier = crate::record::read(config_dir)
        .and_then(|r| r.working_tier())
        .unwrap_or(Tier::Unsupported);
    Selection::preset(tier)
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
