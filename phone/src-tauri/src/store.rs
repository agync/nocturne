//! What the phone keeps about its pairing, in the app's own data folder: the identity (with the
//! private key) and what the computer last said about itself. Each file is written private and
//! whole: to a temporary file first, then renamed over the old one.

use std::io::Write;
use std::path::{Path, PathBuf};

use nocturne_channel::phone::PhoneIdentity;
use nocturne_contract::RemoteConnection;
use serde::{Deserialize, Serialize};

const IDENTITY: &str = "identity.json";
const DEVICE: &str = "device.json";

/// What the computer said about itself, and when, beside the identity.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub computer_name: Option<String>,
    pub connection: Option<RemoteConnection>,
    pub version: Option<String>,
    pub paired_at: u64,
    pub last_connected_at: Option<u64>,
}

/// The paired device as the screens read it. Never the private key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub device_id: String,
    pub address: String,
    pub home_origin: String,
    pub device_name: String,
    #[serde(flatten)]
    pub meta: Meta,
}

impl DeviceInfo {
    pub fn new(identity: &PhoneIdentity, meta: Meta) -> DeviceInfo {
        DeviceInfo {
            device_id: identity.id(),
            address: identity.address.clone(),
            home_origin: identity.home_origin.clone(),
            device_name: identity.device_name.clone(),
            meta,
        }
    }
}

pub struct Store {
    dir: PathBuf,
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let unique: String = nocturne_channel::crypto::random::<6>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let tmp = path.with_file_name(format!(".{name}.{unique}.tmp"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = (|| {
        let mut f = options.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return written;
    }
    // The rename itself is kept only once the folder is on disk.
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        std::fs::File::open(dir)?.sync_all()?;
    }
    Ok(())
}

impl Store {
    pub fn new(dir: PathBuf) -> Store {
        Store { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub fn identity(&self) -> Option<PhoneIdentity> {
        let bytes = std::fs::read(self.path(IDENTITY)).ok()?;
        PhoneIdentity::from_bytes(&bytes)
    }

    pub fn meta(&self) -> Meta {
        std::fs::read(self.path(DEVICE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn device(&self) -> Option<DeviceInfo> {
        self.identity().map(|i| DeviceInfo::new(&i, self.meta()))
    }

    pub fn save_identity(&self, identity: &PhoneIdentity) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        write_private(&self.path(IDENTITY), &identity.to_bytes())
    }

    pub fn save_meta(&self, meta: &Meta) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let bytes = serde_json::to_vec(meta).expect("meta serialises");
        write_private(&self.path(DEVICE), &bytes)
    }

    /// Forget the pairing: this phone was removed, or disconnected.
    pub fn clear(&self) {
        let _ = std::fs::remove_file(self.path(IDENTITY));
        let _ = std::fs::remove_file(self.path(DEVICE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocturne_channel::crypto;

    fn identity() -> PhoneIdentity {
        PhoneIdentity {
            device_id: crypto::random(),
            secret: crypto::new_secret(),
            cpub: crypto::public_bytes(&crypto::new_secret().public_key()).to_vec(),
            home_origin: "https://amber-otter-lamp-quiet.trycloudflare.com".into(),
            address: "amber-otter-lamp-quiet.trycloudflare.com".into(),
            device_name: "Pixel".into(),
        }
    }

    #[test]
    fn identity_round_trips_privately() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("data"));
        assert!(store.device().is_none());
        let id = identity();
        store.save_identity(&id).unwrap();
        store
            .save_meta(&Meta {
                computer_name: Some("Desk".into()),
                paired_at: 7,
                ..Meta::default()
            })
            .unwrap();
        let back = store.identity().unwrap();
        assert_eq!(back.device_id, id.device_id);
        assert_eq!(back.secret.to_bytes(), id.secret.to_bytes());
        let device = store.device().unwrap();
        assert_eq!(device.device_id, id.id());
        assert_eq!(device.meta.computer_name.as_deref(), Some("Desk"));
        let json = serde_json::to_string(&device).unwrap();
        assert!(!json.contains(&crypto::b64(&id.secret.to_bytes())));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("data").join(IDENTITY))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        store.clear();
        assert!(store.device().is_none());
    }

    #[test]
    fn a_damaged_file_is_no_pairing() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf());
        std::fs::write(dir.path().join(IDENTITY), b"{not json").unwrap();
        assert!(store.identity().is_none());
    }
}
