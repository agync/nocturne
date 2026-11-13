//! The connection helpers behind Away from home: pinned download, verification and supervision
//! of `cloudflared` (Simple) and `tailscaled` (Most private), each behind one [`Transport`].
//!
//! Nothing here runs until the person turns Away from home on and agrees to the download.
//! Secrets (the helper key) never reach argv, logs or `Debug` output.

use std::fmt;
use std::future::Future;
use std::io::Read;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HelperKind {
    Cloudflared,
    Tailscaled,
}

/// Where the helpers live: `<data>/remote`, owner-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperDirs {
    pub root: PathBuf,
}

/// A helper on disk, with the checksum it was pinned at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub kind: HelperKind,
    pub version: String,
    /// Lowercase hex.
    pub sha256: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InstallProgress {
    pub received: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HelperError {
    #[error("no internet connection")]
    NoInternet,
    #[error("the download failed: {0}")]
    Download(String),
    #[error("the helper is not the pinned one")]
    Mismatch,
    #[error("the helper did not start: {0}")]
    Spawn(String),
    #[error("the helper stopped: {0}")]
    Exited(String),
    #[error("signing in failed: {0}")]
    SignIn(String),
    #[error("the account does not allow this")]
    AccountBlocked,
    #[error("not supported on this computer")]
    Unsupported,
    #[error("stopped")]
    Stopped,
}

/// The helper of this kind already installed under `dirs`, if any.
pub fn installed(kind: HelperKind, dirs: &HelperDirs) -> Option<Installed> {
    let _ = (kind, dirs);
    None
}

/// Hash the helper again and compare it with its pin. Called before every start.
pub fn verify(installed: &Installed) -> Result<(), HelperError> {
    let mut file = std::fs::File::open(&installed.path).map_err(|_| HelperError::Mismatch)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf).map_err(|_| HelperError::Mismatch)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let got: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if got == installed.sha256.to_ascii_lowercase() {
        Ok(())
    } else {
        Err(HelperError::Mismatch)
    }
}

/// Download, check and install the pinned helper of this kind, reporting progress. Only after
/// the person agreed to the download.
pub async fn install(
    kind: HelperKind,
    dirs: &HelperDirs,
    progress: watch::Sender<InstallProgress>,
    cancel: CancellationToken,
) -> Result<Installed, HelperError> {
    let _ = (kind, dirs, progress, cancel);
    Err(HelperError::Unsupported)
}

/// How the helper reaches Nocturne's listener.
#[derive(Clone, PartialEq, Eq)]
pub enum Origin {
    /// An owner-only Unix socket (Linux and macOS): file permissions are the key.
    Unix(PathBuf),
    /// Loopback TCP with a per-launch key (Windows).
    Tcp { port: u16, key: String },
}

impl fmt::Debug for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Origin::Unix(path) => f.debug_tuple("Unix").field(path).finish(),
            Origin::Tcp { port, .. } => f
                .debug_struct("Tcp")
                .field("port", port)
                .field("key", &"<hidden>")
                .finish(),
        }
    }
}

/// How requests prove they came through the helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCarrier {
    UnixSocket,
    HostHeader,
    PathPrefix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub origin: Origin,
    pub machine_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportState {
    Stopped,
    Starting,
    NeedsSignIn {
        url: String,
        allow_links: bool,
    },
    Up {
        address: String,
        route: Option<String>,
    },
    Restarting {
        attempt: u32,
    },
    Failed {
        error: HelperError,
    },
}

/// For the advanced details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperInfo {
    pub name: &'static str,
    pub version: String,
    pub pid: Option<u32>,
    pub uptime: Option<Duration>,
    pub restarts: u32,
    pub rss_bytes: Option<u64>,
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One way for phones to reach the computer.
pub trait Transport: Send + Sync {
    fn kind(&self) -> HelperKind;
    fn key_carrier(&self) -> KeyCarrier;
    fn start(
        &self,
        spec: LaunchSpec,
    ) -> BoxFuture<'_, Result<watch::Receiver<TransportState>, HelperError>>;
    /// Returns once no helper process remains.
    fn stop(&self) -> BoxFuture<'_, Result<(), HelperError>>;
    /// Tailscaled: log out and forget its state. Cloudflared: nothing to do.
    fn sign_out(&self) -> BoxFuture<'_, Result<(), HelperError>>;
    fn info(&self) -> Option<HelperInfo>;
}

/// The transport for an installed helper.
pub fn transport_for(
    kind: HelperKind,
    dirs: &HelperDirs,
    installed: &Installed,
) -> Box<dyn Transport> {
    let _ = (dirs, installed);
    Box::new(NotYet { kind })
}

/// A transport that starts nothing.
struct NotYet {
    kind: HelperKind,
}

impl Transport for NotYet {
    fn kind(&self) -> HelperKind {
        self.kind
    }

    fn key_carrier(&self) -> KeyCarrier {
        match (cfg!(windows), self.kind) {
            (false, _) => KeyCarrier::UnixSocket,
            (true, HelperKind::Cloudflared) => KeyCarrier::HostHeader,
            (true, HelperKind::Tailscaled) => KeyCarrier::PathPrefix,
        }
    }

    fn start(
        &self,
        _spec: LaunchSpec,
    ) -> BoxFuture<'_, Result<watch::Receiver<TransportState>, HelperError>> {
        Box::pin(async { Err(HelperError::Unsupported) })
    }

    fn stop(&self) -> BoxFuture<'_, Result<(), HelperError>> {
        Box::pin(async { Ok(()) })
    }

    fn sign_out(&self) -> BoxFuture<'_, Result<(), HelperError>> {
        Box::pin(async { Ok(()) })
    }

    fn info(&self) -> Option<HelperInfo> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn helper(dir: &std::path::Path, bytes: &[u8], sha256: &str) -> Installed {
        let path = dir.join("helper");
        std::fs::write(&path, bytes).unwrap();
        Installed {
            kind: HelperKind::Cloudflared,
            version: "1".into(),
            sha256: sha256.into(),
            path,
        }
    }

    #[test]
    fn verify_compares_with_the_pin() {
        let dir = std::env::temp_dir().join(format!("nocturne-helpers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // SHA-256 of "abc".
        let pin = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(verify(&helper(&dir, b"abc", pin)), Ok(()));
        assert_eq!(
            verify(&helper(&dir, b"abd", pin)),
            Err(HelperError::Mismatch)
        );
        let mut gone = helper(&dir, b"abc", pin);
        gone.path = dir.join("missing");
        assert_eq!(verify(&gone), Err(HelperError::Mismatch));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_helper_key_never_shows_in_debug() {
        let spec = LaunchSpec {
            origin: Origin::Tcp {
                port: 4100,
                key: "s3cret-helper-key".into(),
            },
            machine_name: "nocturne-ab12".into(),
        };
        let shown = format!("{spec:?}");
        assert!(!shown.contains("s3cret"), "{shown}");
        assert!(shown.contains("4100"));
    }

    #[tokio::test]
    async fn nothing_starts_yet() {
        let dirs = HelperDirs {
            root: PathBuf::from("unused"),
        };
        assert_eq!(installed(HelperKind::Tailscaled, &dirs), None);
        let (tx, _) = watch::channel(InstallProgress::default());
        assert_eq!(
            install(HelperKind::Cloudflared, &dirs, tx, CancellationToken::new()).await,
            Err(HelperError::Unsupported)
        );
        let pinned = Installed {
            kind: HelperKind::Cloudflared,
            version: "1".into(),
            sha256: String::new(),
            path: PathBuf::from("unused"),
        };
        let t = transport_for(HelperKind::Cloudflared, &dirs, &pinned);
        assert_eq!(t.kind(), HelperKind::Cloudflared);
        let spec = LaunchSpec {
            origin: Origin::Unix(PathBuf::from("unused.sock")),
            machine_name: "nocturne-ab12".into(),
        };
        assert_eq!(t.start(spec).await.err(), Some(HelperError::Unsupported));
        assert_eq!(t.stop().await, Ok(()));
        assert!(t.info().is_none());
    }
}
