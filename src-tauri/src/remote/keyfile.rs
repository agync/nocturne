//! The computer's long-term key on disk.

use std::path::Path;

use p256::SecretKey;
use zeroize::Zeroizing;

use super::crypto::new_secret;

pub const COMPUTER_KEY_FILE: &str = "computer.key";

/// The computer's long-term key, `<dir>/computer.key` (PKCS#8, owner-only), made on first use.
pub fn load_or_create_computer_key(dir: &Path) -> std::io::Result<SecretKey> {
    use p256::pkcs8::{DecodePrivateKey, EncodePrivateKey};
    let path = dir.join(COMPUTER_KEY_FILE);
    match std::fs::read(&path) {
        Ok(der) => {
            let der = Zeroizing::new(der);
            return SecretKey::from_pkcs8_der(&der)
                .map_err(|_| std::io::Error::other("the computer key is unreadable"));
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        Err(_) => {}
    }
    crate::fsutil::create_private_dir(dir)?;
    let key = new_secret();
    let der = key
        .to_pkcs8_der()
        .map_err(|_| std::io::Error::other("the computer key could not be encoded"))?;
    crate::fsutil::write_private(&path, der.as_bytes())?;
    Ok(key)
}

/// Replace the computer's key with a new one.
pub fn replace_computer_key(dir: &Path) -> std::io::Result<SecretKey> {
    match std::fs::remove_file(dir.join(COMPUTER_KEY_FILE)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    load_or_create_computer_key(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computer_key_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let remote = dir.path().join("remote");
        assert!(!remote.join(COMPUTER_KEY_FILE).exists());
        let a = load_or_create_computer_key(&remote).unwrap();
        let b = load_or_create_computer_key(&remote).unwrap();
        assert_eq!(a.to_bytes(), b.to_bytes(), "made on first use only");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode =
                |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&remote.join(COMPUTER_KEY_FILE)), 0o600);
            assert_eq!(mode(&remote), 0o700);
        }
        let c = replace_computer_key(&remote).unwrap();
        assert_ne!(a.to_bytes(), c.to_bytes());
    }
}
