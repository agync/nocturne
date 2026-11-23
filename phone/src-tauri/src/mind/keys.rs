//! Where the mind's key is held. On a phone it is the phone's own safe place, through the system
//! plugin ([`SafeKeys`]): sealed by Android's keystore in a folder no backup takes, or in the
//! iPhone's keychain for this device only. Never in the memory or a backup. [`MemoryKeys`] holds it for this run alone, for tests and for a
//! computer, which has no safe place here.

use std::sync::Mutex;

use nocturne_provider::address::OnlineKey;
use tauri::{AppHandle, Runtime};
use tauri_plugin_nocturne_system::SystemExt;

use super::lock;

/// The secret's name in the safe place.
const NAME: &str = "mind";

/// Where the key is held.
pub trait Keys: Send + Sync {
    /// Holds `key` in place of any held before. False when it could not be kept: the mind
    /// works this run and asks for its key again at the next.
    fn keep(&self, key: OnlineKey) -> bool;
    fn key(&self) -> Option<OnlineKey>;
    fn forget(&self);
}

/// The key in this process's memory only, wiped when forgotten.
#[derive(Default)]
pub struct MemoryKeys(Mutex<Option<OnlineKey>>);

impl Keys for MemoryKeys {
    fn keep(&self, key: OnlineKey) -> bool {
        *lock(&self.0) = Some(key);
        true
    }

    fn key(&self) -> Option<OnlineKey> {
        lock(&self.0).clone()
    }

    fn forget(&self) {
        lock(&self.0).take();
    }
}

/// The key in the phone's safe place.
pub struct SafeKeys<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> Keys for SafeKeys<R> {
    fn keep(&self, key: OnlineKey) -> bool {
        match self.0.system().keep_secret(NAME, key.expose()) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("the key could not be kept in the phone's safe place: {e:?}");
                false
            }
        }
    }

    fn key(&self) -> Option<OnlineKey> {
        match self.0.system().secret(NAME) {
            Ok(key) => key.map(OnlineKey::new),
            Err(e) => {
                tracing::warn!("the phone's safe place could not be read: {e:?}");
                None
            }
        }
    }

    fn forget(&self) {
        if let Err(e) = self.0.system().forget_secret(NAME) {
            tracing::warn!("the key could not be removed from the phone's safe place: {e:?}");
        }
    }
}
