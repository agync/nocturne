//! Names: the computer's own (shown to the person's phones, only inside sealed frames) and the
//! neutral machine name the Tailscale helper registers.

use std::path::Path;

/// The computer's host name, or a plain fallback.
pub fn computer_name() -> String {
    sysinfo::System::host_name()
        .ok()
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "This computer".to_string())
}

const MACHINE_FILE: &str = "machine-name";

/// `nocturne-<4 hex>`, random once per install and kept in `<remote>/machine-name`. It lands in
/// public certificate logs, so it says nothing about the person.
pub fn machine_name(remote_dir: &Path) -> String {
    let path = remote_dir.join(MACHINE_FILE);
    if let Ok(text) = std::fs::read_to_string(&path)
        && is_machine_name(text.trim())
    {
        return text.trim().to_string();
    }
    let bytes = super::crypto::random::<2>();
    let name = format!("nocturne-{:02x}{:02x}", bytes[0], bytes[1]);
    if crate::fsutil::create_private_dir(remote_dir).is_ok() {
        let _ = crate::fsutil::write_private(&path, name.as_bytes());
    }
    name
}

fn is_machine_name(s: &str) -> bool {
    s.strip_prefix("nocturne-")
        .is_some_and(|h| h.len() == 4 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_name_neutral_and_stable() {
        let dir = tempfile::tempdir().unwrap();
        let a = machine_name(&dir.path().join("remote"));
        assert!(is_machine_name(&a), "{a}");
        assert_eq!(machine_name(&dir.path().join("remote")), a);
    }
}
