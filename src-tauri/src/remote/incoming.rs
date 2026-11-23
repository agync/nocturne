//! Files coming up from a phone: streamed chunk by chunk into an owner-only file under
//! `<remote>/incoming`, never held whole in memory. One upload per phone at a time, at most
//! `MAX_INCOMING_TOTAL` bytes declared across all of them, `MAX_ITEM` each and `MAX_DAY` a day
//! per phone. The first chunk must start with the declared type's magic bytes, and the bytes
//! received must equal the size declared. A phone's whole memory moving here (`memoryDone`)
//! comes the same way, up to `MAX_MEMORY_BYTES` and outside the daily and incoming totals, and
//! must start as a backup file does.

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
    pub client_id: String,
}

pub fn declared(call: &RemoteCall) -> Option<Declared> {
    match call {
        RemoteCall::KeepBytes {
            mime,
            size,
            client_id,
            ..
        } => Some(Declared {
            mime: mime.clone(),
            size: *size,
            recording: false,
            client_id: client_id.clone(),
        }),
        RemoteCall::KeepRecording {
            mime,
            size,
            client_id,
            ..
        } => Some(Declared {
            mime: mime.clone(),
            size: *size,
            recording: true,
            client_id: client_id.clone(),
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
    client_id: String,
    mime: String,
    declared: u64,
    received: u64,
    order: ChunkOrder,
    file: Option<std::fs::File>,
    path: PathBuf,
    done: Option<oneshot::Sender<Result<Uploaded, PhoneError>>>,
    /// A phone's memory moving here, not a keep.
    memory: bool,
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
        // The phone records WAV, the one sound the speech model reads.
        if !allowed || (decl.recording && decl.mime != "audio/wav") {
            return Err(PhoneError::Invalid);
        }
        let used = core
            .shell
            .engine()
            .ok_or(PhoneError::NotReady)?
            .remote_day(device, &today(core))
            .map_err(|_| PhoneError::Busy)?;
        if used.saturating_add(size) > MAX_DAY {
            return Err(PhoneError::TooLarge);
        }
        // The day is charged by the keep itself, once admitted (see `charge_day`).
        let mut uploads = self.uploads();
        if let Some(old) = uploads.get(device) {
            // The same keep sent again is a retry of the one still here: a cut the computer has
            // not noticed yet (a phone that lost its signal) must not turn the retry away.
            if core.conv_alive(old.conv) && old.client_id != decl.client_id {
                return Err(PhoneError::Busy);
            }
            // A retry from a new session replaces the dead session's partial upload.
            if let Some(old) = uploads.remove(device) {
                old.fail(PhoneError::SessionExpired);
            }
        }
        let others: u64 = uploads
            .values()
            .filter(|u| !u.memory)
            .map(|u| u.declared)
            .sum();
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
                client_id: decl.client_id,
                mime: decl.mime,
                declared: size,
                received: 0,
                order: ChunkOrder::default(),
                file: Some(file),
                path,
                done: Some(tx),
                memory: false,
            },
        );
        Ok(rx)
    }

    /// Start the upload of a phone's memory, `size` bytes of a backup file. The caller has
    /// checked that the person agreed to it.
    pub fn begin_memory(
        &self,
        device: &str,
        conv: ConvId,
        call_id: u32,
        size: f64,
    ) -> Result<UploadDone, PhoneError> {
        if !(size.is_finite() && size >= 1.0 && size.fract() == 0.0) {
            return Err(PhoneError::Invalid);
        }
        if size > MAX_MEMORY_BYTES as f64 {
            return Err(PhoneError::TooLarge);
        }
        let mut uploads = self.uploads();
        // One move at a time: whatever this phone had coming gives way to it.
        if let Some(old) = uploads.remove(device) {
            old.fail(PhoneError::SessionExpired);
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
                mime: String::new(),
                declared: size as u64,
                received: 0,
                order: ChunkOrder::default(),
                file: Some(file),
                path,
                done: Some(tx),
                memory: true,
            },
        );
        Ok(rx)
    }

    /// How much of a phone's memory is in, and its size, while it is coming.
    pub fn memory_progress(&self, device: &str) -> Option<(u64, u64)> {
        self.uploads()
            .get(device)
            .filter(|u| u.memory)
            .map(|u| (u.received, u.declared))
    }

    /// Count `size` bytes against a phone's day, once the keep is admitted. The count is kept
    /// in the store, so a restart does not start the day over.
    pub fn charge_day(&self, core: &Core, device: &str, size: u64) -> Result<(), PhoneError> {
        let engine = core.shell.engine().ok_or(PhoneError::NotReady)?;
        match engine.charge_remote_day(device, &today(core), size, MAX_DAY) {
            Ok(true) => Ok(()),
            Ok(false) => Err(PhoneError::TooLarge),
            Err(_) => Err(PhoneError::Busy),
        }
    }

    /// Give back what a keep that did not happen was charged.
    pub fn refund_day(&self, core: &Core, device: &str, size: u64) {
        if let Some(engine) = core.shell.engine()
            && let Err(e) = engine.refund_remote_day(device, &today(core), size)
        {
            tracing::warn!("could not give back a phone's day: {e}");
        }
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
        let first_fits = match u.memory {
            true => chunk.bytes.starts_with(BACKUP_MAGIC),
            false => crypto::magic_matches(&u.mime, &chunk.bytes).is_ok(),
        };
        let failure = if out_of_order || (chunk.seq == 0 && !first_fits) {
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
            wrote.err().map(|e| write_failed(&e))
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
            if let Err(e) = synced {
                u.fail(write_failed(&e));
                return Ok(());
            }
            let sent = u.done.take().map(|tx| {
                tx.send(Ok(Uploaded {
                    path: u.path.clone(),
                    bytes: u.received,
                    mime: u.mime.clone(),
                }))
            });
            // Nobody waits for it any more (the call already answered): do not keep the file.
            if !matches!(sent, Some(Ok(()))) {
                let _ = std::fs::remove_file(&u.path);
            }
        }
        Ok(())
    }

    /// Drop a phone's upload (removed, or its call gave up).
    pub fn cancel(&self, device: &str) {
        self.fail(device, PhoneError::Revoked);
    }

    /// End a phone's upload with `code`, which its call answers.
    pub fn fail(&self, device: &str, code: PhoneError) {
        if let Some(u) = self.uploads().remove(device) {
            u.fail(code);
        }
    }

    /// Drop the upload of one call, if it is still coming (the call answered without it).
    /// Each session numbers its calls afresh, so the session is part of the match: a call of a
    /// closed session never drops the upload a retry from a new session began.
    pub fn cancel_call(&self, device: &str, conv: ConvId, call_id: u32) {
        let mut uploads = self.uploads();
        if uploads
            .get(device)
            .is_some_and(|u| u.conv == conv && u.call_id == call_id)
            && let Some(u) = uploads.remove(device)
        {
            drop(uploads);
            u.fail(PhoneError::Invalid);
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

/// A write that did not go: no room here is `TooLarge`, anything else may pass.
fn write_failed(e: &std::io::Error) -> PhoneError {
    use std::io::ErrorKind::{FileTooLarge, QuotaExceeded, StorageFull};
    match e.kind() {
        StorageFull | QuotaExceeded | FileTooLarge => PhoneError::TooLarge,
        _ => PhoneError::Busy,
    }
}

/// How a backup file starts: the header of the encryption it is locked with.
const BACKUP_MAGIC: &[u8] = b"age-encryption.org/v1\n";

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

/// The day a phone's keeps count toward.
fn today(core: &Core) -> String {
    core.shell.wall_now().strftime("%Y-%m-%d").to_string()
}
