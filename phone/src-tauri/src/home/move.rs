//! Moving this phone's memory to the computer it has just paired with. The phone offers it with
//! what it holds; once the person on the computer agrees and the person here says "Move my
//! memory", nothing else touches the store: a backup in 0013's format is made, locked with a
//! passphrase made for this one move, and sent up in chunks with that passphrase inside the
//! sealed session. When the computer says every count matches, Nocturne there restarts to open
//! the memory; the phone keeps its memory, still held, and asks until the computer says the
//! memory opened. Only then is the store put aside as `store-put-aside-{time}`, read only, and
//! the phone becomes a companion of that computer. The put-aside memory stays for 30 days unless
//! the person removes it sooner. While it waits the phone only asks where its memory stands (a
//! `memoryDone` that sends nothing), so the wait never starts a new move on the computer.
//!
//! Anything kept while the memory moves waits in the screens' queue and goes to the computer
//! once the phone is its companion. A move that stops part way changes nothing here.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;

use jiff::{SignedDuration, Timestamp};
use nocturne_channel::client::{CallError, Phone, Reach};
use nocturne_contract::{
    BackupPhase, BackupReport, HomeError, MemoryCounts, PhoneError, PhoneMode, RemoteAnswer,
    RemoteCall,
};
use nocturne_engine::BackupRun;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::mode::ModeFile;
use super::{Home, calls};
use crate::state::AppState;
use crate::sync::ledger::write_private;

/// How the move goes, for the moving screen: `{phase, done, total}` in bytes.
pub const EVENT_MOVE: &str = "move-progress";

const MOVING: &str = "moving";
const MOVED: &str = "moved.json";
const KEEP_DAYS: i64 = 30;
/// Characters of the passphrase made for one move, from the channel's id alphabet.
const PASSPHRASE_CHARS: usize = 32;
/// The passphrase work the move's backup asks for. Its passphrase is 32 random characters of 64
/// (192 bits) and goes only inside the sealed session, so stretching it guards nothing. The work
/// a person's own passphrase gets, run twice (written, then read back), held the move for
/// minutes on a slower phone before anything went across.
const MOVE_WORK_FACTOR: u8 = 10;
/// How long a paused move waits before it tries the computer again.
const RETRY_EVERY: std::time::Duration = std::time::Duration::from_secs(5);
/// How often a finishing move asks the computer whether the memory opened.
const ASK_EVERY: std::time::Duration = std::time::Duration::from_secs(3);

/// What the phone holds, for "What moves".
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveFacts {
    pub counts: MemoryCounts,
    /// Things kept, apart from the journal, check-ins, the calendar and readings.
    pub things: u32,
    /// Journal entries and good things.
    pub journal: u32,
    /// The first day something was kept.
    pub since: Option<String>,
    /// About what the move sends, in bytes.
    pub bytes: f64,
}

/// Where the move stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MovePhase {
    /// The backup is being made.
    Preparing,
    Sending,
    /// Everything went; the computer is checking it.
    Checking,
    /// The computer cannot be reached; it carries on when it is back.
    Paused,
    /// Every count matched; Nocturne on the computer restarts to open the memory.
    Finishing,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveProgress {
    pub phase: MovePhase,
    pub done: f64,
    pub total: f64,
}

/// Why a move did not finish. Nothing changed on the phone in any of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MoveError {
    /// The computer could not be reached: it carries on when it is back.
    Paused,
    /// Something did not arrive whole.
    NotWhole,
    /// The computer is older than this phone's memory.
    ComputerOutdated,
    /// The person on the computer kept what is there; the pairing is undone.
    Declined,
    /// The person stopped it.
    Stopped,
    /// The phone has no room for the backup.
    Full,
    /// Something else, or a move is already under way.
    Failed,
    /// The computer has not enough free room for it; nothing moved.
    NoRoom,
    /// The computer ran out of room while it wrote it out; nothing changed.
    RanOutOfRoom,
    /// The computer could not open it after its restart; nothing changed.
    NotOpened,
    /// The person on the computer stopped the move; nothing changed.
    StoppedThere,
}

/// The computer's answer to the offer, for the screens.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveOffered {
    /// The memory already opened there: the move only has to finish.
    pub opened: bool,
    /// What the computer already holds.
    pub here: Option<MemoryCounts>,
}

/// The finished move, for "Your memory is on {computer}."
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Moved {
    pub items: u32,
}

/// The memory put aside after a move, and until when it stays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PutAside {
    folder: String,
    keep_until: String,
}

/// A backup made for the move, with what the computer must report back.
pub(crate) struct Prepared {
    pub file: PathBuf,
    pub sha256: String,
    pub passphrase: String,
    pub report: BackupReport,
    pub items: u32,
}

fn passphrase() -> String {
    let alphabet = nocturne_channel::protocol::ID_ALPHABET.as_bytes();
    nocturne_channel::crypto::random::<PASSPHRASE_CHARS>()
        .iter()
        .map(|b| alphabet[*b as usize % alphabet.len()] as char)
        .collect()
}

fn digest(file: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(file)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn mark_read_only(path: &Path, read_only: bool) {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return;
    };
    if meta.is_dir() {
        for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
            mark_read_only(&entry.path(), read_only);
        }
    }
    let mut perms = meta.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = match (meta.is_dir(), read_only) {
            (true, true) => 0o500,
            (true, false) => 0o700,
            (false, true) => 0o400,
            (false, false) => 0o600,
        };
        perms.set_mode(mode);
    }
    #[cfg(not(unix))]
    perms.set_readonly(read_only);
    let _ = std::fs::set_permissions(path, perms);
}

impl Home {
    fn moving_dir(&self) -> PathBuf {
        self.path(MOVING)
    }

    /// What would move, as the store holds it now.
    pub fn move_facts(&self) -> Result<MoveFacts, HomeError> {
        let (_use, open) = self.using()?;
        let e = &open.engine;
        let err = |x: nocturne_engine::Error| calls::code(&x);
        let holdings = e.holdings(None).map_err(err)?.unwrap_or_default();
        let counts = MemoryCounts {
            items: e.stats().map_err(err)?.item_count,
            memories: holdings.memories,
            pages: holdings.pages,
            originals: holdings.originals,
        };
        let since = e.journal_days().map_err(err)?.last().map(|d| d.to_string());
        let kept = e.kept_counts().map_err(err)?;
        Ok(MoveFacts {
            counts,
            things: kept.things,
            journal: kept.journal_entries,
            since,
            bytes: e.backup_estimate().map_err(err)? as f64,
        })
    }

    /// Make the backup that moves. Call it holding [`Home::exclusive`], so nothing is kept into
    /// the store between this and the put-aside.
    pub(crate) fn prepare_move(
        &self,
        progress: calls::Progress<'_>,
        cancel: &AtomicBool,
    ) -> Result<Prepared, MoveError> {
        let open = self.open_store().map_err(|_| MoveError::Failed)?;
        let dir = self.moving_dir();
        calls::clear(&dir);
        std::fs::create_dir_all(&dir).map_err(|_| MoveError::Failed)?;
        let file = dir.join("memory.nocturne");
        let passphrase = passphrase();
        let run = BackupRun {
            extras: &[],
            progress,
            cancel,
            started: &|_| {},
            work_factor: Some(MOVE_WORK_FACTOR),
        };
        let made = open
            .engine
            .backup_to(&file, &passphrase, &run)
            .and_then(|report| Ok((report, open.engine.stats()?.item_count)));
        let (report, items) = match made {
            Ok(made) => made,
            Err(e) => {
                calls::clear(&dir);
                return Err(match e {
                    nocturne_engine::Error::Cancelled => MoveError::Stopped,
                    e if super::is_full(&e) => MoveError::Full,
                    _ => MoveError::Failed,
                });
            }
        };
        let sha256 = digest(&file).map_err(|_| MoveError::Failed)?;
        Ok(Prepared {
            file,
            sha256,
            passphrase,
            report,
            items,
        })
    }

    /// The computer has the memory: check its counts against what was sent, then put the store
    /// aside, read only, and become its companion. Call it holding [`Home::exclusive`].
    pub(crate) fn finish_move(
        &self,
        prepared: &Prepared,
        arrived: MemoryCounts,
    ) -> Result<Moved, MoveError> {
        let sent = &prepared.report;
        let whole = arrived.items == prepared.items
            && arrived.memories == sent.memories
            && arrived.pages == sent.pages
            && arrived.originals == sent.originals;
        if !whole {
            return Err(MoveError::NotWhole);
        }
        let name = self.mode_file().map(|f| f.name).unwrap_or_default();
        self.close();
        let folder = self
            .put_aside_store()
            .map_err(|_| MoveError::Failed)?
            .ok_or(MoveError::Failed)?;
        mark_read_only(&self.path(&folder), true);
        let keep_until = self
            .clock
            .now()
            .checked_add(SignedDuration::from_hours(24 * KEEP_DAYS))
            .map_err(|_| MoveError::Failed)?;
        let note = PutAside {
            folder,
            keep_until: keep_until.to_string(),
        };
        let bytes = serde_json::to_vec(&note).expect("the note serialises");
        if let Err(e) = write_private(&self.path(MOVED), &bytes) {
            tracing::warn!("could not note the memory put aside: {e}");
        }
        self.save_mode(&ModeFile {
            mode: PhoneMode::Companion,
            name,
        })
        .map_err(|_| MoveError::Failed)?;
        calls::clear(&self.moving_dir());
        Ok(Moved {
            items: arrived.items,
        })
    }

    /// A move that did not finish leaves no backup behind.
    pub(crate) fn drop_move(&self) {
        calls::clear(&self.moving_dir());
    }

    fn put_aside_note(&self) -> Option<PutAside> {
        let bytes = std::fs::read(self.path(MOVED)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Remove the memory a move put aside, now.
    pub fn remove_put_aside(&self) -> Result<(), HomeError> {
        let Some(note) = self.put_aside_note() else {
            return Ok(());
        };
        let path = self.path(&note.folder);
        if note.folder.starts_with(super::PUT_ASIDE) && path.exists() {
            mark_read_only(&path, false);
            std::fs::remove_dir_all(&path).map_err(|_| HomeError::Failed)?;
        }
        let _ = std::fs::remove_file(self.path(MOVED));
        Ok(())
    }

    /// Remove the memory a move put aside once its 30 days are over.
    pub fn clear_old_put_aside(&self) {
        let Some(note) = self.put_aside_note() else {
            return;
        };
        let due = note
            .keep_until
            .parse::<Timestamp>()
            .is_ok_and(|until| self.clock.now() >= until);
        if due && let Err(e) = self.remove_put_aside() {
            tracing::warn!("the memory put aside could not be removed: {e:?}");
        }
    }
}

/// The ways out to the computer the move takes.
pub(crate) trait Computer {
    fn offer(
        &self,
        call: RemoteCall,
    ) -> impl Future<Output = Result<RemoteAnswer, CallError>> + Send;
    fn send(
        &self,
        call: RemoteCall,
        file: &Path,
        sent: impl Fn(u64) + Send + Sync,
    ) -> impl Future<Output = Result<RemoteAnswer, CallError>> + Send;
    /// Where the memory stands there, sending nothing.
    fn status(&self) -> impl Future<Output = Result<RemoteAnswer, CallError>> + Send;
}

impl Computer for Phone {
    async fn offer(&self, call: RemoteCall) -> Result<RemoteAnswer, CallError> {
        self.call(call).await
    }

    async fn send(
        &self,
        call: RemoteCall,
        file: &Path,
        sent: impl Fn(u64) + Send + Sync,
    ) -> Result<RemoteAnswer, CallError> {
        self.upload_memory(call, file, sent).await
    }

    async fn status(&self) -> Result<RemoteAnswer, CallError> {
        self.memory_status().await
    }
}

fn failure(e: CallError) -> MoveError {
    match e {
        CallError::Connection {
            state: Reach::Removed,
        }
        | CallError::Phone {
            code: PhoneError::Revoked,
        } => MoveError::Declined,
        CallError::Connection {
            state: Reach::ComputerOutdated,
        }
        | CallError::Phone {
            code: PhoneError::ComputerOutdated,
        } => MoveError::ComputerOutdated,
        CallError::Connection { .. }
        | CallError::Phone {
            code: PhoneError::SessionExpired | PhoneError::Busy,
        } => MoveError::Paused,
        CallError::Phone {
            code: PhoneError::Invalid,
        } => MoveError::NotWhole,
        CallError::Phone { .. } => MoveError::Failed,
    }
}

/// Offer the memory and wait for the person on the computer. Answers what the computer already
/// holds, or that the memory is already there (a move whose answer was missed).
pub(crate) async fn offer(
    computer: &impl Computer,
    facts: &MoveFacts,
) -> Result<Offered, MoveError> {
    let call = RemoteCall::OfferMemory {
        counts: facts.counts,
        bytes: facts.bytes.max(1.0),
        schema: nocturne_engine::store::latest_version(),
    };
    tracing::info!(
        "offering the memory: {} items, about {} bytes",
        facts.counts.items,
        facts.bytes
    );
    let answered = computer.offer(call).await.map_err(|e| {
        tracing::info!("the offer was not taken: {e:?}");
        match e {
            CallError::Phone {
                code: PhoneError::TooLarge,
            } => MoveError::NoRoom,
            e => failure(e),
        }
    })?;
    match answered {
        RemoteAnswer::MemoryAccepted { here } => {
            tracing::info!(here = here.is_some(), "the computer agreed to the move");
            Ok(Offered::Accepted { here })
        }
        RemoteAnswer::MemoryArrived { counts } => {
            tracing::info!("the memory already opened on the computer");
            Ok(Offered::Arrived(counts))
        }
        _ => {
            tracing::info!("the computer answered the offer with something else");
            Err(MoveError::Failed)
        }
    }
}

/// Where the memory stands on the computer.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Stands {
    /// It opened there, with these counts.
    Opened(MemoryCounts),
    /// Not opened yet: being checked, or Nocturne there restarts.
    Waiting,
    /// The computer knows nothing of it: it is offered again.
    Unknown,
}

/// Ask where the memory stands, sending nothing.
pub(crate) async fn stands(computer: &impl Computer) -> Result<Stands, MoveError> {
    match computer.status().await {
        Ok(RemoteAnswer::MemoryArrived { counts }) => Ok(Stands::Opened(counts)),
        Ok(_) => Err(MoveError::Failed),
        Err(CallError::Phone {
            code: PhoneError::NotFound,
        }) => Err(MoveError::NotOpened),
        Err(CallError::Phone {
            code: PhoneError::NotReady,
        }) => Ok(Stands::Unknown),
        Err(CallError::Phone {
            code: PhoneError::Paused,
        }) => Err(MoveError::StoppedThere),
        Err(e) => match failure(e) {
            MoveError::Paused => Ok(Stands::Waiting),
            other => Err(other),
        },
    }
}

/// The computer's answer to the offer.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Offered {
    Accepted { here: Option<MemoryCounts> },
    Arrived(MemoryCounts),
}

/// Send the prepared backup; answers the counts that arrived.
pub(crate) async fn send(
    computer: &impl Computer,
    prepared: &Prepared,
    sent: impl Fn(u64) + Send + Sync,
) -> Result<MemoryCounts, MoveError> {
    let size = std::fs::metadata(&prepared.file)
        .map_err(|_| MoveError::Failed)?
        .len();
    let call = RemoteCall::MemoryDone {
        size: size as f64,
        sha256: prepared.sha256.clone(),
        passphrase: prepared.passphrase.clone(),
    };
    tracing::info!("sending the memory: {size} bytes");
    let answered = computer
        .send(call, &prepared.file, sent)
        .await
        .inspect_err(|e| tracing::info!("the memory did not go across: {e:?}"))
        .map_err(|e| match e {
            CallError::Phone {
                code: PhoneError::TooLarge,
            } => MoveError::RanOutOfRoom,
            CallError::Phone {
                code: PhoneError::Paused,
            } => MoveError::StoppedThere,
            e => failure(e),
        })?;
    match answered {
        RemoteAnswer::MemoryArrived { counts } => {
            tracing::info!(
                "the computer has the memory: {} items, {} memories, {} pages, {} originals",
                counts.items,
                counts.memories,
                counts.pages,
                counts.originals
            );
            Ok(counts)
        }
        _ => {
            tracing::info!("the computer answered the memory with something else");
            Err(MoveError::Failed)
        }
    }
}

/// Holds the store for the move: nothing else uses it until this goes.
struct Hold {
    release: Option<mpsc::Sender<()>>,
}

impl Hold {
    fn take<R: Runtime>(app: &AppHandle<R>) -> Hold {
        let (release, wait) = mpsc::channel::<()>();
        let (held, ready) = mpsc::channel::<()>();
        let app = app.clone();
        std::thread::spawn(move || {
            let home = app.state::<Home>();
            let _only = home.exclusive();
            let _ = held.send(());
            let _ = wait.recv();
        });
        let _ = ready.recv();
        Hold {
            release: Some(release),
        }
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        if let Some(r) = self.release.take() {
            let _ = r.send(());
        }
    }
}

/// The move under way on this phone, so "Stop" reaches it.
#[derive(Default)]
pub struct Moving {
    busy: AtomicBool,
    stop: AtomicBool,
    stopped: tokio::sync::Notify,
}

struct Busy<'a>(&'a Moving);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::SeqCst);
    }
}

fn phone<R: Runtime>(app: &AppHandle<R>) -> Result<Phone, MoveError> {
    app.state::<AppState>()
        .phone(app)
        .ok_or(MoveError::Declined)
}

fn emit<R: Runtime>(app: &AppHandle<R>, phase: MovePhase, done: u64, total: u64) {
    let progress = MoveProgress {
        phase,
        done: done as f64,
        total: total as f64,
    };
    if let Err(e) = app.emit(EVENT_MOVE, progress) {
        tracing::warn!("could not say how the move goes: {e}");
    }
}

/// What would move, for "What moves".
#[tauri::command]
pub async fn move_facts<R: Runtime>(app: AppHandle<R>) -> Result<MoveFacts, HomeError> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Home>().move_facts())
        .await
        .unwrap_or(Err(HomeError::Failed))
}

/// Offer the memory to the computer this phone has just paired with, and wait for the person
/// there. Answers what the computer already holds, if anything.
#[tauri::command]
pub async fn move_offer<R: Runtime>(app: AppHandle<R>) -> Result<MoveOffered, MoveError> {
    let facts = move_facts(app.clone())
        .await
        .map_err(|_| MoveError::Failed)?;
    Ok(match offer(&phone(&app)?, &facts).await? {
        Offered::Accepted { here } => MoveOffered {
            opened: false,
            here,
        },
        // Already opened there, from a move whose phone went away part way: moving finishes it
        // without sending anything again.
        Offered::Arrived(_) => MoveOffered {
            opened: true,
            here: None,
        },
    })
}

/// "Move my memory": make the backup, send it and, once every count matches, put the memory
/// here aside and work with the computer. Progress comes as `move-progress` events.
#[tauri::command]
pub async fn move_memory<R: Runtime>(app: AppHandle<R>) -> Result<Moved, MoveError> {
    let moving = app.state::<Moving>();
    if moving.busy.swap(true, Ordering::SeqCst) {
        return Err(MoveError::Failed);
    }
    let _busy = Busy(&moving);
    moving.stop.store(false, Ordering::SeqCst);
    // Listened for from here: a Stop while the backup is made still ends the move, rather than
    // the move carrying on through a pairing the Stop undid.
    let stopped = moving.stopped.notified();
    tokio::pin!(stopped);
    stopped.as_mut().enable();
    let Ok(computer) = phone(&app) else {
        tracing::info!("no pairing to move the memory through");
        return Err(MoveError::Declined);
    };
    tracing::info!("moving the memory: making its backup");
    let began = std::time::Instant::now();
    let hold = {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || Hold::take(&app))
            .await
            .map_err(|_| MoveError::Failed)?
    };
    tracing::info!(
        "nothing else uses the memory now, after {} ms",
        began.elapsed().as_millis()
    );
    let worker = app.clone();
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        let stop = &worker.state::<Moving>().stop;
        // Written, then read back to prove it: both are getting it ready.
        let progress = |_: BackupPhase, done: u64, total: u64| {
            emit(&worker, MovePhase::Preparing, done, total);
        };
        worker.state::<Home>().prepare_move(&progress, stop)
    })
    .await
    .unwrap_or(Err(MoveError::Failed))
    .inspect_err(|e| tracing::info!("the backup for the move was not made: {e:?}"))?;
    tracing::info!(
        "the backup for the move is made: {} bytes in {} ms",
        prepared.report.bytes,
        began.elapsed().as_millis()
    );
    let report = |phase, done, total| emit(&app, phase, done, total);
    let arrived = tokio::select! {
        biased;
        () = &mut stopped => Err(MoveError::Stopped),
        arrived = carry(&computer, &prepared, &report, RETRY_EVERY) => arrived,
    };
    let home = app.state::<Home>();
    let counts = match arrived {
        Ok(counts) => counts,
        Err(e) => {
            tracing::info!("the move ended with nothing changed here: {e:?}");
            home.drop_move();
            drop(hold);
            return Err(e);
        }
    };
    // The store is still held: the put-aside runs before anything can use it again.
    let worker = app.clone();
    let finished = tauri::async_runtime::spawn_blocking(move || {
        let home = worker.state::<Home>();
        let out = home.finish_move(&prepared, counts);
        if out.is_err() {
            home.drop_move();
        }
        out
    })
    .await
    .unwrap_or(Err(MoveError::Failed));
    drop(hold);
    match &finished {
        Ok(moved) => tracing::info!(
            "the memory is on the computer with {} items: this phone follows it now",
            moved.items
        ),
        Err(e) => tracing::info!("the move did not finish here: {e:?}"),
    }
    if finished.is_ok() {
        // The ledger is kept: what was sent into the store went with it.
        if let Err(e) = app.emit(super::commands::EVENT_MODE, home.mode(true)) {
            tracing::warn!("could not say where the memory lives: {e}");
        }
        crate::sources::start(&app);
    }
    finished
}

/// Send the backup, and when the computer cannot be reached, wait and carry on: it is asked
/// again first, so a memory that arrived while the answer was lost is not sent twice.
pub(crate) async fn carry(
    computer: &impl Computer,
    prepared: &Prepared,
    report: &(dyn Fn(MovePhase, u64, u64) + Sync),
    pause: std::time::Duration,
) -> Result<MemoryCounts, MoveError> {
    let size = std::fs::metadata(&prepared.file)
        .map(|m| m.len())
        .unwrap_or(0);
    let facts = MoveFacts {
        counts: MemoryCounts {
            items: prepared.items,
            memories: prepared.report.memories,
            pages: prepared.report.pages,
            originals: prepared.report.originals,
        },
        things: 0,
        journal: 0,
        since: None,
        bytes: size as f64,
    };
    // First where it stands: a move whose phone went away part way (the app stopped while the
    // computer restarted) finishes without sending anything again.
    let first = stands(computer).await;
    tracing::info!("where the memory stands on the computer: {first:?}");
    match first? {
        Stands::Opened(counts) => return Ok(counts),
        Stands::Waiting => {
            if let Some(counts) = finishing(computer, size, report, pause).await? {
                return Ok(counts);
            }
        }
        Stands::Unknown => {}
    }
    // Agreed already when this starts; asked again only after a pause, or once the computer
    // said it knows nothing of it.
    let mut ask = false;
    let last = AtomicU64::new(0);
    let mut paused = false;
    loop {
        let again = if ask {
            offer(computer, &facts).await
        } else {
            Ok(Offered::Accepted { here: None })
        };
        ask = true;
        let out = match again {
            Ok(Offered::Arrived(counts)) => return Ok(counts),
            Ok(Offered::Accepted { .. }) => {
                send(computer, prepared, |done| {
                    let before = last.swap(done, Ordering::Relaxed);
                    if tenths(before, size) != tenths(done, size) {
                        tracing::info!("sent {done} of {size} bytes");
                    }
                    let phase = if done >= size {
                        MovePhase::Checking
                    } else {
                        MovePhase::Sending
                    };
                    report(phase, done, size);
                })
                .await
            }
            Err(e) => Err(e),
        };
        match out {
            Err(MoveError::Paused) => {
                if !paused {
                    tracing::info!("the computer cannot be reached: trying again until it answers");
                }
                paused = true;
                last.store(0, Ordering::Relaxed);
                report(MovePhase::Paused, 0, size);
                tokio::time::sleep(pause).await;
            }
            // Every count matched: wait for the computer to say the memory opened. Should it
            // know nothing of it, it is offered again and sent.
            Ok(_) => {
                if let Some(counts) = finishing(computer, size, report, pause).await? {
                    return Ok(counts);
                }
            }
            Err(e) => return Err(e),
        }
    }
}

/// While Nocturne on the computer restarts: ask where the memory stands, sending nothing, until
/// it opened there (`Some`), it could not open it, or the computer knows nothing of it (`None`).
/// An answer that has to wait (the restart, the connection) just waits.
async fn finishing(
    computer: &impl Computer,
    size: u64,
    report: &(dyn Fn(MovePhase, u64, u64) + Sync),
    pause: std::time::Duration,
) -> Result<Option<MemoryCounts>, MoveError> {
    tracing::info!("waiting for the memory to open on the computer");
    loop {
        report(MovePhase::Finishing, size, size);
        tokio::time::sleep(pause.min(ASK_EVERY)).await;
        match stands(computer).await? {
            Stands::Opened(counts) => return Ok(Some(counts)),
            Stands::Unknown => return Ok(None),
            Stands::Waiting => {}
        }
    }
}

/// Whole tenths of `size` that `done` makes, for a progress line each tenth of the way.
fn tenths(done: u64, size: u64) -> u64 {
    done.saturating_mul(10) / size.max(1)
}

/// "Stop": the move ends and nothing changes here. The pairing is undone, so the computer
/// keeps nothing of it.
#[tauri::command]
pub async fn move_stop<R: Runtime>(app: AppHandle<R>) -> Result<(), MoveError> {
    tracing::info!("the person stopped the move");
    let moving = app.state::<Moving>();
    moving.stop.store(true, Ordering::SeqCst);
    moving.stopped.notify_waiters();
    move_undo(app).await
}

/// "Not now", or the computer kept what it has: the pairing is undone on both, and the phone
/// stays a phone alone.
#[tauri::command]
pub async fn move_undo<R: Runtime>(app: AppHandle<R>) -> Result<(), MoveError> {
    let state = app.state::<AppState>();
    if let Some(phone) = state.phone(&app) {
        phone.disconnect().await;
    }
    state.store.clear();
    state.drop_phone();
    Ok(())
}

/// A move is part way: the phone holds its own memory and a pairing made for moving it (every
/// other way out undoes that pairing), so the screens carry the move on.
#[tauri::command]
pub fn move_pending<R: Runtime>(app: AppHandle<R>) -> bool {
    let paired = app.state::<AppState>().store.identity().is_some();
    paired && app.state::<Home>().mode(true) == Some(PhoneMode::Alone)
}

/// "Remove it from this phone", or keep it put aside for 30 days (nothing to do then).
#[tauri::command]
pub async fn move_put_aside<R: Runtime>(app: AppHandle<R>, remove: bool) -> Result<(), HomeError> {
    if !remove {
        return Ok(());
    }
    tauri::async_runtime::spawn_blocking(move || app.state::<Home>().remove_put_aside())
        .await
        .unwrap_or(Err(HomeError::Failed))
}

/// At each start: a memory put aside by a move goes once its 30 days are over.
pub fn clear_old_put_aside<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || app.state::<Home>().clear_old_put_aside());
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use nocturne_channel::client::{CallError, Reach};
    use nocturne_contract::{MemoryCounts, PhoneError, PhoneMode, RemoteAnswer, RemoteCall};
    use nocturne_engine::ManualClock;

    use super::{Computer, MoveError, MovePhase, Offered, carry, offer, send};
    use crate::home::{Home, dispatch};

    fn alone(dir: &Path) -> (Home, Arc<ManualClock>) {
        let clock = Arc::new(ManualClock::at("2026-11-22T09:00:00Z", "Europe/London"));
        let mut home = Home::new(dir.to_path_buf(), clock.clone(), Box::new(|_: &Path| {}));
        home.set_work_factor(10);
        home.start_alone("Pixel", false).unwrap();
        let open = home.open_store().unwrap();
        for (text, id) in [("Bins out on Tuesday.", "c1"), ("Call the plumber.", "c2")] {
            let answer = dispatch::answer(
                &open,
                RemoteCall::KeepText {
                    text: text.into(),
                    client_id: id.into(),
                    feeling: None,
                    good: None,
                    captured_at: None,
                    web: false,
                },
            );
            assert!(matches!(answer, RemoteAnswer::Kept { .. }), "{answer:?}");
        }
        (home, clock)
    }

    /// A computer that opens what it is sent with the passphrase that came with it, and answers
    /// what it found, or what it was told to.
    struct Fake {
        scratch: tempfile::TempDir,
        offered: Result<RemoteAnswer, CallError>,
        /// Answers to the next offers, in turn, before `offered`.
        offers: Mutex<std::collections::VecDeque<Result<RemoteAnswer, CallError>>>,
        /// Answers to the next sends, in turn, before what it found.
        sends: Mutex<std::collections::VecDeque<CallError>>,
        /// Answers to the next asks of where it stands, before what it last took.
        statuses: Mutex<std::collections::VecDeque<Result<RemoteAnswer, CallError>>>,
        /// What it took last, as it would answer once opened.
        took: Mutex<Option<MemoryCounts>>,
        skew: u32,
        calls: Mutex<Vec<RemoteCall>>,
    }

    impl Fake {
        fn new() -> Fake {
            Fake {
                scratch: tempfile::tempdir().unwrap(),
                offered: Ok(RemoteAnswer::MemoryAccepted { here: None }),
                offers: Mutex::default(),
                sends: Mutex::default(),
                statuses: Mutex::default(),
                took: Mutex::default(),
                skew: 0,
                calls: Mutex::default(),
            }
        }
    }

    impl Computer for Fake {
        async fn offer(&self, call: RemoteCall) -> Result<RemoteAnswer, CallError> {
            self.calls.lock().unwrap().push(call);
            let next = self.offers.lock().unwrap().pop_front();
            next.unwrap_or_else(|| self.offered.clone())
        }

        async fn send(
            &self,
            call: RemoteCall,
            file: &Path,
            sent: impl Fn(u64) + Send + Sync,
        ) -> Result<RemoteAnswer, CallError> {
            let RemoteCall::MemoryDone { passphrase, .. } = &call else {
                panic!("{call:?}")
            };
            if let Some(e) = self.sends.lock().unwrap().pop_front() {
                self.calls.lock().unwrap().push(call);
                return Err(e);
            }
            let summary = nocturne_engine::check_backup(
                file,
                passphrase,
                &self.scratch.path().join("check"),
                &|_, _, _| {},
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
            sent(std::fs::metadata(file).unwrap().len());
            self.calls.lock().unwrap().push(call);
            let counts = MemoryCounts {
                items: summary.items + self.skew,
                memories: summary.memories,
                pages: summary.pages,
                originals: summary.originals,
            };
            *self.took.lock().unwrap() = Some(counts);
            Ok(RemoteAnswer::MemoryArrived { counts })
        }

        async fn status(&self) -> Result<RemoteAnswer, CallError> {
            if let Some(next) = self.statuses.lock().unwrap().pop_front() {
                return next;
            }
            match *self.took.lock().unwrap() {
                Some(counts) => Ok(RemoteAnswer::MemoryArrived { counts }),
                None => Err(CallError::Phone {
                    code: PhoneError::NotReady,
                }),
            }
        }
    }

    fn block<T>(f: impl Future<Output = T>) -> T {
        tauri::async_runtime::block_on(f)
    }

    /// The passphrase work a backup asks of whoever opens it, from its header.
    fn work_factor_of(file: &Path) -> u8 {
        let bytes = std::fs::read(file).unwrap();
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).into_owned();
        let stanza = head
            .lines()
            .find(|l| l.starts_with("-> scrypt "))
            .expect("a passphrase stanza")
            .to_string();
        stanza.rsplit(' ').next().unwrap().parse().unwrap()
    }

    #[test]
    fn the_moves_backup_is_quick_to_open_whatever_a_backup_here_asks() {
        let dir = tempfile::tempdir().unwrap();
        // As the app makes it: no lighter work set, so its own backups ask the full work.
        let clock = Arc::new(ManualClock::at("2026-11-22T09:00:00Z", "Europe/London"));
        let home = Home::new(dir.path().to_path_buf(), clock, Box::new(|_: &Path| {}));
        home.start_alone("Pixel", false).unwrap();
        let only = home.exclusive();
        let stop = std::sync::atomic::AtomicBool::new(false);
        let prepared = home.prepare_move(&|_, _, _| {}, &stop).unwrap();
        drop(only);
        assert_eq!(work_factor_of(&prepared.file), super::MOVE_WORK_FACTOR);
        home.drop_move();
    }

    #[test]
    fn the_facts_say_what_would_move() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let facts = home.move_facts().unwrap();
        assert_eq!(facts.counts.items, 2);
        assert_eq!(facts.since.as_deref(), Some("2026-11-22"));
        assert!(facts.bytes > 0.0);
    }

    #[test]
    fn a_memory_that_arrived_whole_is_put_aside_and_the_phone_follows_its_computer() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let computer = Fake::new();
        let facts = home.move_facts().unwrap();
        assert_eq!(
            block(offer(&computer, &facts)).unwrap(),
            Offered::Accepted { here: None }
        );

        let only = home.exclusive();
        let stop = std::sync::atomic::AtomicBool::new(false);
        let prepared = home.prepare_move(&|_, _, _| {}, &stop).unwrap();
        assert_eq!(prepared.passphrase.len(), 32);
        let arrived = block(send(&computer, &prepared, |_| {})).unwrap();
        let moved = home.finish_move(&prepared, arrived).unwrap();
        drop(only);

        assert_eq!(moved.items, 2);
        assert_eq!(home.mode(true), Some(PhoneMode::Companion));
        assert!(
            !home.store_dir().exists(),
            "the memory is no longer in use here"
        );
        let aside = home.put_aside_names();
        assert_eq!(aside.len(), 1);
        let db = dir.path().join(&aside[0]).join("nocturne.db");
        assert!(
            std::fs::OpenOptions::new().write(true).open(&db).is_err(),
            "put aside read only"
        );
        assert!(home.open_store().is_err(), "a companion opens no memory");
        assert!(!dir.path().join("moving").exists(), "no backup is left");
        let sent = computer.calls.lock().unwrap();
        let RemoteCall::MemoryDone { sha256, .. } = &sent[1] else {
            panic!()
        };
        assert_eq!(sha256.len(), 64);
    }

    #[test]
    fn counts_that_do_not_match_change_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let mut computer = Fake::new();
        computer.skew = 1;
        let only = home.exclusive();
        let stop = std::sync::atomic::AtomicBool::new(false);
        let prepared = home.prepare_move(&|_, _, _| {}, &stop).unwrap();
        let arrived = block(send(&computer, &prepared, |_| {})).unwrap();
        assert_eq!(
            home.finish_move(&prepared, arrived),
            Err(MoveError::NotWhole)
        );
        home.drop_move();
        drop(only);
        assert_eq!(home.mode(true), Some(PhoneMode::Alone));
        assert!(home.store_dir().exists());
        assert!(home.put_aside_names().is_empty());
        assert!(!dir.path().join("moving").exists());
        assert_eq!(home.move_facts().unwrap().counts.items, 2);
    }

    #[test]
    fn a_stopped_backup_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let stop = std::sync::atomic::AtomicBool::new(true);
        let only = home.exclusive();
        assert_eq!(
            home.prepare_move(&|_, _, _| {}, &stop).err(),
            Some(MoveError::Stopped)
        );
        drop(only);
        assert!(!dir.path().join("moving").exists());
    }

    #[test]
    fn what_was_put_aside_goes_when_removed_or_after_thirty_days() {
        let moved = |dir: &Path| {
            let (home, clock) = alone(dir);
            let only = home.exclusive();
            let stop = std::sync::atomic::AtomicBool::new(false);
            let prepared = home.prepare_move(&|_, _, _| {}, &stop).unwrap();
            let arrived = block(send(&Fake::new(), &prepared, |_| {})).unwrap();
            home.finish_move(&prepared, arrived).unwrap();
            drop(only);
            (home, clock)
        };
        let first = tempfile::tempdir().unwrap();
        let (home, _) = moved(first.path());
        home.remove_put_aside().unwrap();
        assert!(home.put_aside_names().is_empty());

        let second = tempfile::tempdir().unwrap();
        let (home, clock) = moved(second.path());
        clock.advance_days(29);
        home.clear_old_put_aside();
        assert_eq!(home.put_aside_names().len(), 1, "kept for 30 days");
        clock.advance_days(1);
        home.clear_old_put_aside();
        assert!(home.put_aside_names().is_empty());
    }

    #[test]
    fn what_the_computer_says_becomes_what_the_screens_show() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let facts = home.move_facts().unwrap();
        for (said, shown) in [
            (
                Err(CallError::Phone {
                    code: PhoneError::Revoked,
                }),
                MoveError::Declined,
            ),
            (
                Err(CallError::Connection {
                    state: Reach::Asleep,
                }),
                MoveError::Paused,
            ),
            (
                Err(CallError::Phone {
                    code: PhoneError::ComputerOutdated,
                }),
                MoveError::ComputerOutdated,
            ),
            (Ok(RemoteAnswer::Done), MoveError::Failed),
        ] {
            let mut computer = Fake::new();
            computer.offered = said;
            assert_eq!(block(offer(&computer, &facts)).err(), Some(shown));
        }
        let mut computer = Fake::new();
        let counts = facts.counts;
        computer.offered = Ok(RemoteAnswer::MemoryArrived { counts });
        assert_eq!(
            block(offer(&computer, &facts)).unwrap(),
            Offered::Arrived(counts)
        );
    }

    fn phone_code(code: PhoneError) -> CallError {
        CallError::Phone { code }
    }

    fn sends(computer: &Fake) -> usize {
        computer
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| matches!(c, RemoteCall::MemoryDone { .. }))
            .count()
    }

    /// Prepare under the hold and carry it with `computer`; answers what carry said and every
    /// phase it reported.
    fn carried(home: &Home, computer: &Fake) -> (Result<MemoryCounts, MoveError>, Vec<MovePhase>) {
        let stop = std::sync::atomic::AtomicBool::new(false);
        let prepared = home.prepare_move(&|_, _, _| {}, &stop).unwrap();
        let phases = Mutex::new(Vec::new());
        let out = block(carry(
            computer,
            &prepared,
            &|phase, _, _| phases.lock().unwrap().push(phase),
            std::time::Duration::ZERO,
        ));
        home.drop_move();
        (out, phases.into_inner().unwrap())
    }

    #[test]
    fn the_memory_stays_until_the_computer_says_it_opened() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let computer = Fake::new();
        // Nothing known before it goes; then the computer restarts: wait, unreachable, opened.
        computer.statuses.lock().unwrap().extend([
            Err(phone_code(PhoneError::NotReady)),
            Err(phone_code(PhoneError::Busy)),
            Err(CallError::Connection {
                state: Reach::CantConnect,
            }),
        ]);
        let (out, phases) = carried(&home, &computer);
        assert_eq!(out.unwrap().items, 2);
        assert!(phases.contains(&MovePhase::Finishing));
        assert!(
            !phases.contains(&MovePhase::Paused),
            "finishing is not a pause"
        );
        assert_eq!(
            computer
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|c| matches!(c, RemoteCall::OfferMemory { .. }))
                .count(),
            0,
            "waiting never offers it again"
        );
        assert_eq!(
            home.mode(true),
            Some(PhoneMode::Alone),
            "put aside only by the caller"
        );
        assert!(home.store_dir().exists());
    }

    #[test]
    fn a_memory_that_did_not_open_leaves_this_one_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let computer = Fake::new();
        computer.statuses.lock().unwrap().extend([
            Err(phone_code(PhoneError::NotReady)),
            Err(phone_code(PhoneError::NotFound)),
        ]);
        let (out, _) = carried(&home, &computer);
        assert_eq!(out.err(), Some(MoveError::NotOpened));
        assert_eq!(home.mode(true), Some(PhoneMode::Alone));
        assert_eq!(home.move_facts().unwrap().counts.items, 2);
    }

    #[test]
    fn a_computer_that_knows_nothing_of_it_after_its_restart_is_offered_it_again() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let computer = Fake::new();
        computer.statuses.lock().unwrap().extend([
            Err(phone_code(PhoneError::NotReady)),
            Err(phone_code(PhoneError::NotReady)),
        ]);
        let (out, _) = carried(&home, &computer);
        assert!(out.is_ok());
        assert_eq!(sends(&computer), 2);
        let offers = computer
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| matches!(c, RemoteCall::OfferMemory { .. }))
            .count();
        assert_eq!(offers, 1, "asked again before it went again");
    }

    #[test]
    fn a_move_that_already_opened_there_finishes_without_sending() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let computer = Fake::new();
        let counts = home.move_facts().unwrap().counts;
        computer
            .statuses
            .lock()
            .unwrap()
            .push_back(Ok(RemoteAnswer::MemoryArrived { counts }));
        let (out, _) = carried(&home, &computer);
        assert_eq!(out.unwrap(), counts);
        assert_eq!(sends(&computer), 0);
    }

    #[test]
    fn room_running_out_or_a_stop_there_is_said_and_never_tried_again() {
        for (code, said) in [
            (PhoneError::TooLarge, MoveError::RanOutOfRoom),
            (PhoneError::Paused, MoveError::StoppedThere),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let (home, _) = alone(dir.path());
            let computer = Fake::new();
            computer.sends.lock().unwrap().push_back(phone_code(code));
            let (out, _) = carried(&home, &computer);
            assert_eq!(out.err(), Some(said));
            assert_eq!(sends(&computer), 1, "nothing sent again by itself");
            assert_eq!(home.mode(true), Some(PhoneMode::Alone));
        }
    }

    #[test]
    fn no_room_on_the_computer_is_said_before_anything_moves() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let facts = home.move_facts().unwrap();
        let mut computer = Fake::new();
        computer.offered = Err(phone_code(PhoneError::TooLarge));
        assert_eq!(
            block(offer(&computer, &facts)).err(),
            Some(MoveError::NoRoom)
        );
    }

    #[test]
    fn a_move_leaves_the_store_here_as_it_was_until_it_opened() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let facts = home.move_facts().unwrap();
        assert_eq!((facts.things, facts.journal), (2, 0));
        let stop = std::sync::atomic::AtomicBool::new(false);
        home.prepare_move(&|_, _, _| {}, &stop).unwrap();
        home.drop_move();
        let open = home.open_store().unwrap();
        let items = open
            .engine
            .items(nocturne_contract::ItemFilter::default())
            .unwrap();
        assert!(
            items.iter().all(|i| i.from_device.is_none()),
            "nothing written here"
        );
    }
}
