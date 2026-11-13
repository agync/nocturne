//! Files coming up from a phone: streamed chunk by chunk into an owner-only file under
//! `<remote>/incoming`, never held whole in memory. One upload per phone at a time, at most
//! `MAX_INCOMING_TOTAL` bytes declared across all of them, `MAX_ITEM` each and `MAX_DAY` a day
//! per phone. The first chunk must start with the declared type's magic bytes, and the bytes
//! received must equal the size declared.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use nocturne_contract::{PhoneError, RemoteCall};
use tokio::sync::oneshot;

use super::crypto::{self, Chunk};
use super::frame::ChunkOrder;
use super::pairing::ConvId;
use super::protocol::*;
use super::session::Core;

/// What a keep of bytes declares before any chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct Declared {
    pub mime: String,
    pub size: f64,
    pub recording: bool,
}

pub fn declared(call: &RemoteCall) -> Option<Declared> {
    match call {
        RemoteCall::KeepBytes { mime, size, .. } => Some(Declared {
            mime: mime.clone(),
            size: *size,
            recording: false,
        }),
        RemoteCall::KeepRecording { mime, size, .. } => Some(Declared {
            mime: mime.clone(),
            size: *size,
            recording: true,
        }),
        _ => None,
    }
}

/// A finished upload: the file and its size.
#[derive(Debug)]
pub struct Uploaded {
    pub path: PathBuf,
    pub bytes: u64,
    pub mime: String,
}

pub type UploadDone = oneshot::Receiver<Result<Uploaded, PhoneError>>;

struct Upload {
    conv: ConvId,
    call_id: u32,
    mime: String,
    declared: u64,
    received: u64,
    order: ChunkOrder,
    file: Option<std::fs::File>,
    path: PathBuf,
    done: Option<oneshot::Sender<Result<Uploaded, PhoneError>>>,
}

impl Upload {
    fn fail(mut self, code: PhoneError) {
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
        if let Some(tx) = self.done.take() {
            let _ = tx.send(Err(code));
        }
    }
}

pub struct Incoming {
    dir: PathBuf,
    uploads: Mutex<HashMap<String, Upload>>,
}

impl Incoming {
    pub fn new(dir: PathBuf) -> Incoming {
        Incoming {
            dir,
            uploads: Mutex::new(HashMap::new()),
        }
    }

    fn uploads(&self) -> MutexGuard<'_, HashMap<String, Upload>> {
        self.uploads.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Start an upload for `device`: checks the declaration, the limits and that no other
    /// upload of this phone is still alive.
    pub fn begin(
        &self,
        core: &Core,
        device: &str,
        conv: ConvId,
        call_id: u32,
        decl: Declared,
    ) -> Result<UploadDone, PhoneError> {
        if !(decl.size.is_finite() && decl.size >= 1.0 && decl.size.fract() == 0.0) {
            return Err(PhoneError::Invalid);
        }
        if decl.size > MAX_ITEM as f64 {
            return Err(PhoneError::TooLarge);
        }
        let size = decl.size as u64;
        let allowed = MIME_MAGIC.iter().any(|m| m.mime == decl.mime);
        if !allowed || (decl.recording && !decl.mime.starts_with("audio/")) {
            return Err(PhoneError::Invalid);
        }
        {
            let today = core.shell.wall_now().strftime("%Y-%m-%d").to_string();
            let mut phones = core.phones.lock().unwrap_or_else(|e| e.into_inner());
            let p = phones.entry(device.to_string()).or_default();
            if p.day.0 != today {
                p.day = (today, 0);
            }
            if p.day.1 + size > MAX_DAY {
                return Err(PhoneError::TooLarge);
            }
        }
        let mut uploads = self.uploads();
        if let Some(old) = uploads.get(device) {
            if core.conv_alive(old.conv) {
                return Err(PhoneError::Busy);
            }
            // A retry from a new session replaces the dead session's partial upload.
            if let Some(old) = uploads.remove(device) {
                old.fail(PhoneError::SessionExpired);
            }
        }
        let others: u64 = uploads.values().map(|u| u.declared).sum();
        if others + size > MAX_INCOMING_TOTAL {
            return Err(PhoneError::Busy);
        }
        crate::fsutil::create_private_dir(&self.dir).map_err(|_| PhoneError::Busy)?;
        let path = self
            .dir
            .join(format!("{}.part", hex(&crypto::random::<12>())));
        let file = private_file(&path).map_err(|_| PhoneError::Busy)?;
        let (tx, rx) = oneshot::channel();
        uploads.insert(
            device.to_string(),
            Upload {
                conv,
                call_id,
                mime: decl.mime,
                declared: size,
                received: 0,
                order: ChunkOrder::default(),
                file: Some(file),
                path,
                done: Some(tx),
            },
        );
        drop(uploads);
        let mut phones = core.phones.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(p) = phones.get_mut(device) {
            p.day.1 += size;
        }
        Ok(rx)
    }

    /// One chunk of `device`'s upload, written straight to its file.
    pub fn chunk(&self, device: &str, conv: ConvId, chunk: Chunk) -> Result<(), PhoneError> {
        let mut uploads = self.uploads();
        let Some(u) = uploads.get_mut(device) else {
            return Err(PhoneError::Invalid);
        };
        if u.conv != conv || u.call_id != chunk.call_id {
            return Err(PhoneError::Invalid);
        }
        let out_of_order = u.order.accept(&chunk).is_err();
        let failure = if out_of_order
            || (chunk.seq == 0 && crypto::magic_matches(&u.mime, &chunk.bytes).is_err())
        {
            Some(PhoneError::Invalid)
        } else if u.received + chunk.bytes.len() as u64 > u.declared {
            Some(PhoneError::TooLarge)
        } else if chunk.last && u.received + chunk.bytes.len() as u64 != u.declared {
            Some(PhoneError::Invalid)
        } else {
            let wrote = u
                .file
                .as_mut()
                .map(|f| f.write_all(&chunk.bytes))
                .unwrap_or(Ok(()));
            wrote.err().map(|_| PhoneError::Busy)
        };
        u.received += chunk.bytes.len() as u64;
        if let Some(code) = failure {
            if let Some(u) = uploads.remove(device) {
                u.fail(code);
            }
            return Ok(());
        }
        if chunk.last {
            let mut u = uploads.remove(device).expect("present");
            let synced = u.file.take().map(|f| f.sync_all()).unwrap_or(Ok(()));
            if synced.is_err() {
                u.fail(PhoneError::Busy);
                return Ok(());
            }
            if let Some(tx) = u.done.take() {
                let _ = tx.send(Ok(Uploaded {
                    path: u.path.clone(),
                    bytes: u.received,
                    mime: u.mime.clone(),
                }));
            }
        }
        Ok(())
    }

    /// Drop a phone's upload (removed, or its call gave up).
    pub fn cancel(&self, device: &str) {
        if let Some(u) = self.uploads().remove(device) {
            u.fail(PhoneError::Revoked);
        }
    }

    pub fn in_flight(&self) -> usize {
        self.uploads().len()
    }

    /// Drop every upload and remove whatever is left in the folder (at start and stop).
    pub fn clear_all(&self) {
        let all: Vec<Upload> = self.uploads().drain().map(|(_, u)| u).collect();
        for u in all {
            u.fail(PhoneError::SessionExpired);
        }
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for e in entries.flatten() {
                let _ = std::fs::remove_dir_all(e.path());
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn private_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}
