//! Where the key for Online answers is kept: the system keychain (service `Nocturne`, user
//! `online-answers`), or, when there is none, `<data>/online/key`, readable by its owner only
//! in a folder only its owner can open. The key is read into a redacting type at once and is
//! never logged; the file is never part of an export.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nocturne_contract::KeyStorage;
use nocturne_lemonade::online::OnlineKey;
use zeroize::Zeroizing;

pub const SERVICE: &str = "Nocturne";
pub const USER: &str = "online-answers";
pub const FOLDER: &str = "online";
pub const FILE: &str = "key";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// Neither the keychain nor the file could be used.
    #[error("the key could not be kept or read")]
    Unavailable,
}

/// A place a secret can be kept by name.
pub trait Keychain: Send + Sync {
    /// `Ok(None)` when nothing is stored; `Err` when the keychain cannot be used.
    fn get(&self) -> Result<Option<Zeroizing<String>>, KeyError>;
    fn set(&self, key: &str) -> Result<(), KeyError>;
    fn delete(&self) -> Result<(), KeyError>;
}

/// The system keychain: macOS Keychain, Windows Credential Manager or the Secret Service.
pub struct SystemKeychain;

impl SystemKeychain {
    fn entry() -> Result<keyring::Entry, KeyError> {
        keyring::Entry::new(SERVICE, USER).map_err(|_| KeyError::Unavailable)
    }
}

impl Keychain for SystemKeychain {
    fn get(&self) -> Result<Option<Zeroizing<String>>, KeyError> {
        match Self::entry()?.get_password() {
            Ok(k) => Ok(Some(Zeroizing::new(k))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(KeyError::Unavailable),
        }
    }

    fn set(&self, key: &str) -> Result<(), KeyError> {
        Self::entry()?
            .set_password(key)
            .map_err(|_| KeyError::Unavailable)
    }

    fn delete(&self) -> Result<(), KeyError> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(KeyError::Unavailable),
        }
    }
}

/// The key's two homes, keychain first.
pub struct KeyStore {
    keychain: Arc<dyn Keychain>,
    data_dir: PathBuf,
}

impl KeyStore {
    pub fn new(keychain: Arc<dyn Keychain>, data_dir: &Path) -> Self {
        KeyStore {
            keychain,
            data_dir: data_dir.to_path_buf(),
        }
    }

    /// The fallback file.
    pub fn file(&self) -> PathBuf {
        self.data_dir.join(FOLDER).join(FILE)
    }

    /// The stored key and where it was found. `Ok(None)` when there is none.
    pub fn load(&self) -> Result<Option<(OnlineKey, KeyStorage)>, KeyError> {
        let from_keychain = self.keychain.get();
        if let Ok(Some(k)) = &from_keychain {
            return Ok(Some((OnlineKey::new(k.to_string()), KeyStorage::Keychain)));
        }
        match std::fs::read_to_string(self.file()) {
            Ok(text) => {
                let text = Zeroizing::new(text);
                let key = text.trim();
                if key.is_empty() {
                    return Ok(None);
                }
                Ok(Some((OnlineKey::new(key.to_string()), KeyStorage::File)))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => match from_keychain {
                Ok(_) => Ok(None),
                Err(e) => Err(e),
            },
            Err(_) => Err(KeyError::Unavailable),
        }
    }

    /// Keeps the key in the keychain, or in the owner-only file when the keychain cannot be
    /// used. Whichever is not used holds no copy.
    pub fn save(&self, key: &OnlineKey) -> Result<KeyStorage, KeyError> {
        if self.keychain.set(key.expose()).is_ok() {
            let _ = remove(&self.file());
            return Ok(KeyStorage::Keychain);
        }
        write_private(&self.file(), key.expose()).map_err(|_| KeyError::Unavailable)?;
        Ok(KeyStorage::File)
    }

    /// Removes the key from both homes.
    pub fn forget(&self) -> Result<(), KeyError> {
        let chain = self.keychain.delete();
        let file = remove(&self.file());
        match (chain, file) {
            (_, Err(_)) => Err(KeyError::Unavailable),
            // A keychain that would not delete is gone only if it now says it holds nothing.
            (Err(e), Ok(())) => match self.keychain.get() {
                Ok(None) => Ok(()),
                _ => Err(e),
            },
            (Ok(()), Ok(())) => Ok(()),
        }
    }
}

fn remove(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Writes `text` to `path`, mode 0600 in a folder of mode 0700, replacing it whole.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let tmp = dir.join(format!("{FILE}.tmp"));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)
}
