//! Where this phone's memory lives, kept in `mode.json`: on the phone itself (alone), or on a
//! computer it is paired with (companion). A phone with no file and no pairing has not chosen
//! yet: the screens show Welcome. A phone paired before this file existed is a companion.

use nocturne_channel::protocol::MAX_DEVICE_NAME;
use nocturne_contract::{HomeError, PhoneMode};
use serde::{Deserialize, Serialize};

use super::{FALLBACK_NAME, Home};
use crate::sync::ledger::write_private;

const MODE: &str = "mode.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModeFile {
    pub mode: PhoneMode,
    /// The phone's name for its own row in the store.
    pub name: String,
}

/// A name for this phone: some words, short enough for a device row.
fn valid_name(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()
        && name.chars().count() <= MAX_DEVICE_NAME
        && !name.contains(char::is_control))
    .then(|| name.to_string())
}

impl Home {
    pub(crate) fn mode_file(&self) -> Option<ModeFile> {
        let bytes = std::fs::read(self.path(MODE)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub(crate) fn save_mode(&self, file: &ModeFile) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let bytes = serde_json::to_vec(file).expect("the mode serialises");
        write_private(&self.path(MODE), &bytes)
    }

    /// The phone has not chosen where its memory lives: Welcome shows again.
    pub(crate) fn forget_mode(&self) -> std::io::Result<()> {
        match std::fs::remove_file(self.path(MODE)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// Where the memory lives. `paired`: the phone holds a pairing with a computer.
    pub fn mode(&self, paired: bool) -> Option<PhoneMode> {
        match self.mode_file() {
            Some(f) => Some(f.mode),
            None => paired.then_some(PhoneMode::Companion),
        }
    }

    /// The name this phone's row takes: the one it started with, else the one the store
    /// already has, else a plain stand-in.
    pub(crate) fn name_for_store(&self, in_store: Option<String>) -> String {
        self.mode_file()
            .map(|f| f.name)
            .or(in_store)
            .unwrap_or_else(|| FALLBACK_NAME.to_string())
    }

    /// "Start on this phone": the store is made (or opened, when one is there) and this phone
    /// gets its own row. A store put aside as damaged stays where it is. Refused on a phone
    /// paired with a computer: one home at a time.
    pub fn start_alone(&self, name: &str, paired: bool) -> Result<(), HomeError> {
        let name = valid_name(name).ok_or(HomeError::Invalid)?;
        if paired && self.mode(paired) != Some(PhoneMode::Alone) {
            return Err(HomeError::NotReady);
        }
        let _only = self.exclusive();
        self.save_mode(&ModeFile {
            mode: PhoneMode::Alone,
            name: name.clone(),
        })
        .map_err(|e| {
            tracing::warn!("could not keep where the memory lives: {e}");
            HomeError::Failed
        })?;
        self.clear_damaged();
        let open = self.open_store().map_err(|_| HomeError::Failed)?;
        // Already open: the row takes the new name in place.
        let device = open
            .engine
            .ensure_local_device(&name)
            .map_err(|_| HomeError::Failed)?;
        self.set_device(device);
        Ok(())
    }
}
