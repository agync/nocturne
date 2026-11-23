//! Moving this phone's memory to the computer it has just paired with. The phone offers it with
//! what it holds. An empty computer agrees at once; one that already holds a memory asks the
//! person there how this phone joins it. Once the person here says "Move my memory", nothing
//! else touches the store: a backup in 0013's format is made, locked with a passphrase made for
//! this one move, and sent up in chunks with that passphrase inside the sealed session. The
//! computer checks every count and adds the things to its memory, with no restart, and answers
//! `MemoryBrought`. Only then is the store put aside as `store-put-aside-{time}`, read only, the
//! phone becomes a companion of that computer, and the calendar and health are sent again from
//! nothing so the computer has them too. The put-aside memory stays for 30 days unless the
//! person removes it sooner. A computer still checking or adding when the phone comes back is
//! only asked where the memory stands (a `memoryDone` that sends nothing), so the wait never
//! starts a new move there. A computer that answers it took the memory whole, to open after a
//! restart, is not followed: nothing changes here.
//!
//! Anything kept while the memory moves waits in the screens' queue and goes to the computer
//! once the phone is its companion. A move that stops part way changes nothing here.
//!
//! To use the computer's
//! memory, nothing goes: the store is put aside at once, kept until the person removes it (it
//! is the only copy), and the phone becomes a companion. From This phone, what was put aside
//! can be brought in later: a backup is made from it and offered with `bring`, so the computer
//! adds it without asking again.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;

use jiff::{SignedDuration, Timestamp};
use nocturne_channel::client::{CallError, Phone, Reach};
use nocturne_contract::{
    BackupPhase, BackupReport, BroughtCounts, HomeError, KeptCounts, MemoryCounts, PhoneError,
    PhoneMode, RemoteAnswer, RemoteCall,
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
/// How often a move asks the computer, while it checks or adds, where the memory stands.
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
    /// Of that, what the photos and recordings take.
    pub media: f64,
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
    /// The memory already opened there, or its things were added: the move only has to finish.
    pub opened: bool,
    /// What the computer already holds.
    pub here: Option<MemoryCounts>,
    /// The person there chose to bring this phone's things in: the move starts by itself.
    pub bring: bool,
    /// The person there chose the computer's memory: nothing went, the memory here is put
    /// aside, and this phone now works with that computer.
    pub use_here: bool,
}

/// The finished move, for "Your memory is on {computer}.", or with `brought` for "Your things
/// are on {computer}."
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Moved {
    pub items: u32,
    pub brought: Option<BroughtCounts>,
}

/// The memory put aside after a move, and until when it stays: none when nothing of it is on
/// the computer, since then it is the only copy and only the person removes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PutAside {
    folder: String,
    keep_until: Option<String>,
    /// What it holds is on the computer too. A note from before choosing says so.
    #[serde(default = "on_computer")]
    on_computer: bool,
}

fn on_computer() -> bool {
    true
}

/// The memory a move put aside on this phone, for This phone.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveAside {
    /// Its things are on the computer: it can only be removed. Otherwise "Bring its things to
    /// {computer}…" is offered too.
    pub on_computer: bool,
    /// When it goes by itself, if it does.
    pub keep_until: Option<String>,
    /// When it was put aside, for "Remove what was put aside {day}?".
    pub at: Option<String>,
    /// What it holds, for deleting it for good; none when it cannot be read.
    pub counts: Option<KeptCounts>,
    /// It holds pages or reflections written on this phone.
    pub writing: bool,
}

/// How the computer took what was sent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Arrival {
    /// The memory opened there, with these counts.
    Opened(MemoryCounts),
    /// Its things were checked (these counts) and added to the memory there.
    Brought(MemoryCounts, BroughtCounts),
}

impl Arrival {
    fn counts(&self) -> MemoryCounts {
        match self {
            Arrival::Opened(c) | Arrival::Brought(c, _) => *c,
        }
    }

    fn brought(&self) -> Option<BroughtCounts> {
        match self {
            Arrival::Opened(_) => None,
            Arrival::Brought(_, added) => Some(*added),
        }
    }
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

/// Every count the computer says it took matches what was sent.
fn whole(prepared: &Prepared, arrived: MemoryCounts) -> bool {
    let sent = &prepared.report;
    arrived.items == prepared.items
        && arrived.memories == sent.memories
        && arrived.pages == sent.pages
        && arrived.originals == sent.originals
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
            media: e.originals_bytes() as f64,
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

    /// The computer has the memory, or added its things: check its counts against what was
    /// sent, then put the store aside, read only, for 30 days, and become its companion. Call it
    /// holding [`Home::exclusive`].
    pub(crate) fn finish_move(
        &self,
        prepared: &Prepared,
        arrival: Arrival,
    ) -> Result<Moved, MoveError> {
        if !whole(prepared, arrival.counts()) {
            return Err(MoveError::NotWhole);
        }
        let keep_until = Some(self.thirty_days()?);
        self.become_companion(keep_until, true)?;
        calls::clear(&self.moving_dir());
        Ok(Moved {
            items: arrival.counts().items,
            brought: arrival.brought(),
        })
    }

    /// The person on the computer chose its memory: nothing goes. The store is put aside, read
    /// only, kept until the person removes it, and the phone becomes a companion. Call it
    /// holding [`Home::exclusive`].
    pub(crate) fn use_computer(&self) -> Result<(), MoveError> {
        if self.mode_file().map(|f| f.mode) != Some(PhoneMode::Alone) {
            return Err(MoveError::Failed);
        }
        self.become_companion(None, false)?;
        calls::clear(&self.moving_dir());
        Ok(())
    }

    fn thirty_days(&self) -> Result<String, MoveError> {
        self.clock
            .now()
            .checked_add(SignedDuration::from_hours(24 * KEEP_DAYS))
            .map(|t| t.to_string())
            .map_err(|_| MoveError::Failed)
    }

    fn become_companion(
        &self,
        keep_until: Option<String>,
        on_computer: bool,
    ) -> Result<(), MoveError> {
        let name = self.mode_file().map(|f| f.name).unwrap_or_default();
        self.close();
        let folder = self
            .put_aside_store()
            .map_err(|_| MoveError::Failed)?
            .ok_or(MoveError::Failed)?;
        mark_read_only(&self.path(&folder), true);
        self.write_put_aside(&PutAside {
            folder,
            keep_until,
            on_computer,
        });
        self.save_mode(&ModeFile {
            mode: PhoneMode::Companion,
            name,
        })
        .map_err(|_| MoveError::Failed)
    }

    fn write_put_aside(&self, note: &PutAside) {
        let bytes = serde_json::to_vec(note).expect("the note serialises");
        if let Err(e) = write_private(&self.path(MOVED), &bytes) {
            tracing::warn!("could not note the memory put aside: {e}");
        }
    }

    /// The memory a move put aside here, for This phone.
    pub fn move_aside(&self) -> Option<MoveAside> {
        let note = self.put_aside_note()?;
        let dir = self.path(&note.folder);
        dir.exists().then(|| MoveAside {
            on_computer: note.on_computer,
            keep_until: note.keep_until,
            at: self.put_aside_at(&note.folder),
            counts: nocturne_engine::store_kept_counts(&dir).ok(),
            writing: nocturne_engine::store_has_writing(&dir).unwrap_or(false),
        })
    }

    /// "Bring its things to {computer}…": make the backup that goes from what a move put aside,
    /// when nothing of it is on the computer yet. It is opened only for this and read only
    /// again after. Answers the backup and the first day something was kept.
    pub(crate) fn prepare_bring(
        &self,
        progress: calls::Progress<'_>,
        cancel: &AtomicBool,
    ) -> Result<(Prepared, Option<String>), MoveError> {
        let note = self
            .put_aside_note()
            .filter(|n| !n.on_computer && n.folder.starts_with(super::PUT_ASIDE))
            .ok_or(MoveError::Failed)?;
        let folder = self.path(&note.folder);
        if !folder.exists() {
            return Err(MoveError::Failed);
        }
        let dir = self.moving_dir();
        calls::clear(&dir);
        std::fs::create_dir_all(&dir).map_err(|_| MoveError::Failed)?;
        let file = dir.join("memory.nocturne");
        let passphrase = passphrase();
        mark_read_only(&folder, false);
        let made = (|| {
            let engine = nocturne_engine::Engine::open(&folder, self.clock.clone())?;
            let run = BackupRun {
                extras: &[],
                progress,
                cancel,
                started: &|_| {},
                work_factor: Some(MOVE_WORK_FACTOR),
            };
            let report = engine.backup_to(&file, &passphrase, &run)?;
            let items = engine.stats()?.item_count;
            let since = engine.journal_days()?.last().map(|d| d.to_string());
            Ok::<_, nocturne_engine::Error>((report, items, since))
        })();
        mark_read_only(&folder, true);
        let (report, items, since) = match made {
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
        let prepared = Prepared {
            file,
            sha256,
            passphrase,
            report,
            items,
        };
        Ok((prepared, since))
    }

    /// What was put aside is now on the computer too: it stays 30 days, unless the person
    /// removes it sooner.
    pub(crate) fn finish_bring(
        &self,
        prepared: &Prepared,
        arrival: Arrival,
    ) -> Result<Moved, MoveError> {
        let Arrival::Brought(counts, added) = arrival else {
            return Err(MoveError::Failed);
        };
        if !whole(prepared, counts) {
            return Err(MoveError::NotWhole);
        }
        let note = self.put_aside_note().ok_or(MoveError::Failed)?;
        self.write_put_aside(&PutAside {
            keep_until: Some(self.thirty_days()?),
            on_computer: true,
            ..note
        });
        calls::clear(&self.moving_dir());
        Ok(Moved {
            items: counts.items,
            brought: Some(added),
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

    /// Remove the memory a move put aside once its 30 days are over. One that is the only copy
    /// has no end and stays.
    pub fn clear_old_put_aside(&self) {
        let Some(note) = self.put_aside_note() else {
            return;
        };
        let due = note
            .keep_until
            .and_then(|k| k.parse::<Timestamp>().ok())
            .is_some_and(|until| self.clock.now() >= until);
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
/// holds and how the phone joins it, or that the memory is already there (a move whose answer
/// was missed). With `bring`, what a move put aside is brought in: the computer asks nothing.
pub(crate) async fn offer(
    computer: &impl Computer,
    facts: &MoveFacts,
    bring: bool,
) -> Result<Offered, MoveError> {
    let call = RemoteCall::OfferMemory {
        counts: facts.counts,
        bytes: facts.bytes.max(1.0),
        schema: nocturne_engine::store::latest_version(),
        since: facts.since.clone(),
        // The facts of a backup made for a move count nothing apart: the computer counts then.
        things: (facts.things + facts.journal > 0).then_some(facts.things),
        bring,
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
        // An empty computer that would take the memory whole and restart is older than this
        // phone: nothing goes, so there are never two homes.
        RemoteAnswer::MemoryAccepted {
            here: None,
            bring: false,
        } => {
            tracing::info!("the computer would take the memory whole: it needs updating");
            Err(MoveError::ComputerOutdated)
        }
        RemoteAnswer::MemoryAccepted { here, bring } => {
            tracing::info!(
                here = here.is_some(),
                bring,
                "the computer agreed to the move"
            );
            Ok(Offered::Accepted { here, bring })
        }
        RemoteAnswer::MemoryArrived { counts } => {
            tracing::info!("the memory already opened on the computer");
            Ok(Offered::Arrived(counts))
        }
        RemoteAnswer::MemoryBrought { counts, added } => {
            tracing::info!("the things were already added on the computer");
            Ok(Offered::Brought(counts, added))
        }
        RemoteAnswer::UseComputerMemory => {
            tracing::info!("the person on the computer chose its memory");
            Ok(Offered::UseHere)
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
    /// Its things were added there.
    Brought(MemoryCounts, BroughtCounts),
    /// Not settled yet: being checked or added there.
    Waiting,
    /// The computer knows nothing of it: it is offered again.
    Unknown,
}

/// Ask where the memory stands, sending nothing.
pub(crate) async fn stands(computer: &impl Computer) -> Result<Stands, MoveError> {
    match computer.status().await {
        Ok(RemoteAnswer::MemoryArrived { counts }) => Ok(Stands::Opened(counts)),
        Ok(RemoteAnswer::MemoryBrought { counts, added }) => Ok(Stands::Brought(counts, added)),
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
    Accepted {
        here: Option<MemoryCounts>,
        bring: bool,
    },
    Arrived(MemoryCounts),
    Brought(MemoryCounts, BroughtCounts),
    /// Nothing goes: the computer's memory is used.
    UseHere,
}

/// Send the prepared backup; answers how the computer took it.
pub(crate) async fn send(
    computer: &impl Computer,
    prepared: &Prepared,
    sent: impl Fn(u64) + Send + Sync,
) -> Result<Arrival, MoveError> {
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
            Ok(Arrival::Opened(counts))
        }
        RemoteAnswer::MemoryBrought { counts, added } => {
            tracing::info!(
                "the computer added the things: {} things, {} journal entries, {} check-ins",
                added.things,
                added.entries,
                added.checkins
            );
            Ok(Arrival::Brought(counts, added))
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

impl Moving {
    /// The move under way ends where it is.
    fn halt(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.stopped.notify_waiters();
    }
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
/// there. Answers what the computer already holds, if anything, and how this phone joins it.
/// When the person there chose the computer's memory, the memory here is put aside before
/// this answers.
#[tauri::command]
pub async fn move_offer<R: Runtime>(app: AppHandle<R>) -> Result<MoveOffered, MoveError> {
    let facts = move_facts(app.clone())
        .await
        .map_err(|_| MoveError::Failed)?;
    let offered = |opened, here, bring, use_here| MoveOffered {
        opened,
        here,
        bring,
        use_here,
    };
    Ok(match offer(&phone(&app)?, &facts, false).await? {
        Offered::Accepted { here, bring } => offered(false, here, bring, false),
        // Already opened or added there, from a move whose phone went away part way: moving
        // finishes it without sending anything again.
        Offered::Arrived(_) => offered(true, None, false, false),
        Offered::Brought(..) => offered(true, None, true, false),
        Offered::UseHere => {
            use_computer(&app).await?;
            offered(false, None, false, true)
        }
    })
}

/// Nothing goes to the computer: the memory here is put aside, kept until the person removes
/// it, and this phone works with the computer from now on. What was sent of the calendar and
/// health went into the memory put aside, so both go to the computer from nothing.
async fn use_computer<R: Runtime>(app: &AppHandle<R>) -> Result<(), MoveError> {
    let hold = {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || Hold::take(&app))
            .await
            .map_err(|_| MoveError::Failed)?
    };
    let worker = app.clone();
    let done = tauri::async_runtime::spawn_blocking(move || worker.state::<Home>().use_computer())
        .await
        .unwrap_or(Err(MoveError::Failed));
    drop(hold);
    done.inspect_err(|e| tracing::info!("the memory here was not put aside: {e:?}"))?;
    tracing::info!("the memory here is put aside: this phone uses the computer's");
    // What was sent while the person there chose went into the memory put aside.
    app.state::<AppState>().syncer.ledger.forget_sends();
    followed(app);
    Ok(())
}

/// This phone now works with the computer: the screens hear so, and the calendar and health
/// go there.
fn followed<R: Runtime>(app: &AppHandle<R>) {
    let home = app.state::<Home>();
    if let Err(e) = app.emit(super::commands::EVENT_MODE, home.mode(true)) {
        tracing::warn!("could not say where the memory lives: {e}");
    }
    crate::sources::start(app);
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
        arrived = carry(&computer, &prepared, &report, RETRY_EVERY, false) => arrived,
    };
    let home = app.state::<Home>();
    let arrival = match arrived {
        Ok(arrival) => arrival,
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
        let out = home.finish_move(&prepared, arrival);
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
    if let Ok(moved) = &finished {
        // A memory that moved whole took what was sent with it, so the ledger is kept. Things
        // brought in left the calendar and health behind: they go again from nothing.
        if moved.brought.is_some() {
            app.state::<AppState>().syncer.ledger.forget_sends();
        }
        followed(&app);
    }
    finished
}

/// "Bring its things to {computer}…" from This phone: make a backup from what a move put aside,
/// offer it to bring its things in, send it and, once they were added, keep what was put aside
/// for 30 days. Progress comes as `move-progress` events. Nothing changes here if it stops.
#[tauri::command]
pub async fn move_bring_aside<R: Runtime>(app: AppHandle<R>) -> Result<Moved, MoveError> {
    let moving = app.state::<Moving>();
    if moving.busy.swap(true, Ordering::SeqCst) {
        return Err(MoveError::Failed);
    }
    let _busy = Busy(&moving);
    moving.stop.store(false, Ordering::SeqCst);
    let stopped = moving.stopped.notified();
    tokio::pin!(stopped);
    stopped.as_mut().enable();
    let computer = phone(&app)?;
    tracing::info!("bringing what was put aside to the computer: making its backup");
    let worker = app.clone();
    let (prepared, since) = tauri::async_runtime::spawn_blocking(move || {
        let stop = &worker.state::<Moving>().stop;
        let progress = |_: BackupPhase, done: u64, total: u64| {
            emit(&worker, MovePhase::Preparing, done, total);
        };
        worker.state::<Home>().prepare_bring(&progress, stop)
    })
    .await
    .unwrap_or(Err(MoveError::Failed))
    .inspect_err(|e| tracing::info!("the backup of what was put aside was not made: {e:?}"))?;
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
        since,
        bytes: size as f64,
        media: 0.0,
    };
    let report = |phase, done, total| emit(&app, phase, done, total);
    let brought = tokio::select! {
        biased;
        () = &mut stopped => Err(MoveError::Stopped),
        brought = async {
            match offer(&computer, &facts, true).await? {
                Offered::Accepted { bring: true, .. } => {
                    carry(&computer, &prepared, &report, RETRY_EVERY, true).await
                }
                Offered::Brought(counts, added) => Ok(Arrival::Brought(counts, added)),
                _ => Err(MoveError::Failed),
            }
        } => brought,
    };
    let home = app.state::<Home>();
    let finished = brought.and_then(|arrival| home.finish_bring(&prepared, arrival));
    match &finished {
        Ok(moved) => {
            tracing::info!(
                "what was put aside is on the computer with {} items",
                moved.items
            );
            app.state::<AppState>().syncer.ledger.forget_sends();
            crate::sources::start(&app);
        }
        Err(e) => {
            tracing::info!("bringing what was put aside ended with nothing changed: {e:?}");
            home.drop_move();
        }
    }
    finished
}

/// "Stop" while what was put aside is brought in: it ends and nothing changes here. This phone
/// stays with its computer.
#[tauri::command]
pub async fn move_bring_stop<R: Runtime>(app: AppHandle<R>) -> Result<(), MoveError> {
    tracing::info!("the person stopped bringing what was put aside");
    tell_stop(&app).await?;
    app.state::<Moving>().halt();
    Ok(())
}

/// Tell the computer the person stopped bringing the things in, so it stops too and says so.
/// Refused once they are being added there: that finishes, and this phone waits to hear it.
async fn tell_stop<R: Runtime>(app: &AppHandle<R>) -> Result<(), MoveError> {
    let Some(phone) = app.state::<AppState>().phone(app) else {
        return Ok(());
    };
    match phone.call(RemoteCall::StopMemory).await {
        Err(CallError::Phone {
            code: PhoneError::TooLate,
        }) => {
            tracing::info!("too late to stop: the computer is adding the things");
            Err(MoveError::Failed)
        }
        Err(e) => {
            tracing::info!("the computer did not hear the stop: {e:?}");
            Ok(())
        }
        Ok(_) => Ok(()),
    }
}

/// The memory a move put aside here, if any, for This phone.
#[tauri::command]
pub fn move_aside<R: Runtime>(app: AppHandle<R>) -> Option<MoveAside> {
    app.state::<Home>().move_aside()
}

/// Send the backup, and when the computer cannot be reached, wait and carry on: it is asked
/// again first, so a memory that arrived while the answer was lost is not sent twice.
pub(crate) async fn carry(
    computer: &impl Computer,
    prepared: &Prepared,
    report: &(dyn Fn(MovePhase, u64, u64) + Sync),
    pause: std::time::Duration,
    bring: bool,
) -> Result<Arrival, MoveError> {
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
        media: 0.0,
    };
    // First where it stands: a move whose phone went away part way (the app stopped while the
    // computer checked or added) finishes without sending anything again.
    let first = stands(computer).await;
    tracing::info!("where the memory stands on the computer: {first:?}");
    match first? {
        Stands::Opened(counts) => return Ok(Arrival::Opened(counts)),
        // Things added from this very memory; one that changed since goes again, and the
        // computer adds only what is new.
        Stands::Brought(counts, added) if whole(prepared, counts) => {
            return Ok(Arrival::Brought(counts, added));
        }
        Stands::Brought(..) => {}
        Stands::Waiting => {
            if let Some(arrival) = settled(computer, size, report, pause).await? {
                return Ok(arrival);
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
            offer(computer, &facts, bring).await
        } else {
            Ok(Offered::Accepted { here: None, bring })
        };
        ask = true;
        let out = match again {
            Ok(Offered::Arrived(counts)) => return Ok(Arrival::Opened(counts)),
            Ok(Offered::Brought(counts, added)) => return Ok(Arrival::Brought(counts, added)),
            // Nothing is sent to a computer whose memory the person chose there.
            Ok(Offered::UseHere) => Err(MoveError::Failed),
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
            Ok(brought @ Arrival::Brought(..)) => return Ok(brought),
            // Taken whole, to open after a restart: no longer how a move ends, so this phone
            // keeps its memory rather than wait on a computer that may come back elsewhere.
            Ok(Arrival::Opened(_)) => {
                tracing::info!("the computer took the memory whole: nothing changes here");
                return Err(MoveError::Failed);
            }
            Err(e) => return Err(e),
        }
    }
}

/// While the computer checks or adds what it was sent: ask where the memory stands, sending
/// nothing, until its things were added (`Some`) or the computer knows nothing of it (`None`).
/// An answer that has to wait (the check, the connection) just waits.
async fn settled(
    computer: &impl Computer,
    size: u64,
    report: &(dyn Fn(MovePhase, u64, u64) + Sync),
    pause: std::time::Duration,
) -> Result<Option<Arrival>, MoveError> {
    tracing::info!("waiting for the computer to finish with what it was sent");
    loop {
        report(MovePhase::Checking, size, size);
        tokio::time::sleep(pause.min(ASK_EVERY)).await;
        match stands(computer).await? {
            Stands::Opened(counts) => return Ok(Some(Arrival::Opened(counts))),
            Stands::Brought(counts, added) => return Ok(Some(Arrival::Brought(counts, added))),
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
    tell_stop(&app).await?;
    app.state::<Moving>().halt();
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

/// "Remove it from this phone", or keep it put aside: for 30 days, or until the person removes
/// it when nothing of it is on the computer (nothing to do then).
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
    use nocturne_contract::{
        BroughtCounts, MemoryCounts, PhoneError, PhoneMode, RemoteAnswer, RemoteCall,
    };
    use nocturne_engine::ManualClock;

    use super::{Arrival, Computer, MoveError, MovePhase, Offered, carry, offer, send};
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
        /// It adds the things to a memory of its own rather than opening the memory.
        brings: bool,
        calls: Mutex<Vec<RemoteCall>>,
    }

    impl Fake {
        fn new() -> Fake {
            Fake {
                scratch: tempfile::tempdir().unwrap(),
                offered: Ok(RemoteAnswer::MemoryAccepted {
                    here: None,
                    bring: true,
                }),
                offers: Mutex::default(),
                sends: Mutex::default(),
                statuses: Mutex::default(),
                took: Mutex::default(),
                skew: 0,
                brings: false,
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
            if self.brings {
                return Ok(RemoteAnswer::MemoryBrought {
                    counts,
                    added: added(),
                });
            }
            Ok(RemoteAnswer::MemoryArrived { counts })
        }

        async fn status(&self) -> Result<RemoteAnswer, CallError> {
            if let Some(next) = self.statuses.lock().unwrap().pop_front() {
                return next;
            }
            match *self.took.lock().unwrap() {
                Some(counts) if self.brings => Ok(RemoteAnswer::MemoryBrought {
                    counts,
                    added: added(),
                }),
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

    fn added() -> BroughtCounts {
        BroughtCounts {
            things: 2,
            entries: 0,
            checkins: 0,
        }
    }

    #[test]
    fn an_offer_hears_how_the_phone_joins_the_computers_memory() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let facts = home.move_facts().unwrap();
        let counts = facts.counts;
        for (said, heard) in [
            (RemoteAnswer::UseComputerMemory, Offered::UseHere),
            (
                RemoteAnswer::MemoryAccepted {
                    here: Some(counts),
                    bring: true,
                },
                Offered::Accepted {
                    here: Some(counts),
                    bring: true,
                },
            ),
            (
                RemoteAnswer::MemoryBrought {
                    counts,
                    added: added(),
                },
                Offered::Brought(counts, added()),
            ),
        ] {
            let mut computer = Fake::new();
            computer.offered = Ok(said);
            assert_eq!(block(offer(&computer, &facts, true)).unwrap(), heard);
            let RemoteCall::OfferMemory { bring, since, .. } = &computer.calls.lock().unwrap()[0]
            else {
                panic!("an offer")
            };
            assert!(*bring);
            assert_eq!(since.as_deref(), Some("2026-11-22"));
        }
    }

    #[test]
    fn things_brought_in_put_the_memory_here_aside_for_thirty_days() {
        let dir = tempfile::tempdir().unwrap();
        let (home, clock) = alone(dir.path());
        let mut computer = Fake::new();
        computer.brings = true;
        let only = home.exclusive();
        let stop = std::sync::atomic::AtomicBool::new(false);
        let prepared = home.prepare_move(&|_, _, _| {}, &stop).unwrap();
        let arrival = block(carry(
            &computer,
            &prepared,
            &|_, _, _| {},
            std::time::Duration::ZERO,
            false,
        ))
        .unwrap();
        assert!(
            matches!(arrival, Arrival::Brought(..)),
            "no restart to wait for"
        );
        let moved = home.finish_move(&prepared, arrival).unwrap();
        drop(only);
        assert_eq!(moved.brought, Some(added()));
        assert_eq!(home.mode(true), Some(PhoneMode::Companion));
        let aside = home.move_aside().unwrap();
        assert!(aside.on_computer, "only removing is offered");
        assert!(aside.keep_until.is_some());
        clock.advance_days(30);
        home.clear_old_put_aside();
        assert!(home.put_aside_names().is_empty());
    }

    #[test]
    fn using_the_computers_memory_keeps_this_one_aside_until_removed() {
        let dir = tempfile::tempdir().unwrap();
        let (home, clock) = alone(dir.path());
        let only = home.exclusive();
        home.use_computer().unwrap();
        drop(only);
        assert_eq!(home.mode(true), Some(PhoneMode::Companion));
        assert!(!home.store_dir().exists());
        let aside = home.move_aside().unwrap();
        assert!(!aside.on_computer);
        assert_eq!(aside.keep_until, None);
        assert!(
            aside.at.is_some(),
            "when it was put aside, for the question"
        );
        let counts = aside
            .counts
            .expect("what it holds, for deleting it for good");
        assert!(counts.things + counts.journal_entries > 0, "{counts:?}");
        assert!(!aside.writing, "no pages or reflections were written here");
        clock.advance_days(400);
        home.clear_old_put_aside();
        assert_eq!(home.put_aside_names().len(), 1, "it is the only copy");
        assert!(
            home.use_computer().is_err(),
            "a companion has nothing to put aside"
        );
        home.remove_put_aside().unwrap();
        assert!(home.put_aside_names().is_empty());
        assert_eq!(home.move_aside(), None);
    }

    #[test]
    fn what_was_put_aside_is_brought_in_later_and_then_kept_thirty_days() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let only = home.exclusive();
        home.use_computer().unwrap();
        drop(only);
        let stop = std::sync::atomic::AtomicBool::new(false);
        let (prepared, since) = home.prepare_bring(&|_, _, _| {}, &stop).unwrap();
        assert_eq!(since.as_deref(), Some("2026-11-22"));
        assert_eq!(prepared.items, 2);
        let folder = dir.path().join(&home.put_aside_names()[0]);
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(folder.join("nocturne.db"))
                .is_err(),
            "read only again"
        );
        let mut computer = Fake::new();
        computer.brings = true;
        let arrival = block(carry(
            &computer,
            &prepared,
            &|_, _, _| {},
            std::time::Duration::ZERO,
            true,
        ))
        .unwrap();
        let mut skewed = Fake::new();
        skewed.brings = true;
        skewed.skew = 1;
        let wrong = block(send(&skewed, &prepared, |_| {})).unwrap();
        assert_eq!(
            home.finish_bring(&prepared, wrong),
            Err(MoveError::NotWhole)
        );
        assert!(!home.move_aside().unwrap().on_computer, "nothing changed");
        let moved = home.finish_bring(&prepared, arrival).unwrap();
        assert_eq!(moved.brought, Some(added()));
        let aside = home.move_aside().unwrap();
        assert!(aside.on_computer);
        assert!(aside.keep_until.is_some());
        assert!(
            home.prepare_bring(&|_, _, _| {}, &stop).is_err(),
            "its things are on the computer already"
        );
        assert!(!dir.path().join("moving").exists());
    }

    #[test]
    fn things_added_from_an_older_copy_of_this_memory_are_sent_again() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let mut computer = Fake::new();
        computer.brings = true;
        let older = MemoryCounts {
            items: 1,
            ..MemoryCounts::default()
        };
        computer
            .statuses
            .lock()
            .unwrap()
            .push_back(Ok(RemoteAnswer::MemoryBrought {
                counts: older,
                added: added(),
            }));
        let (out, _) = carried(&home, &computer);
        assert!(matches!(out.unwrap(), Arrival::Brought(c, _) if c.items == 2));
        assert_eq!(sends(&computer), 1, "what was kept since goes too");
    }

    #[test]
    fn a_bring_in_asked_again_keeps_bringing() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let mut computer = Fake::new();
        computer.brings = true;
        computer.offered = Ok(RemoteAnswer::MemoryAccepted {
            here: None,
            bring: true,
        });
        computer
            .sends
            .lock()
            .unwrap()
            .push_back(CallError::Connection {
                state: Reach::Asleep,
            });
        let only = home.exclusive();
        let stop = std::sync::atomic::AtomicBool::new(false);
        let prepared = home.prepare_move(&|_, _, _| {}, &stop).unwrap();
        let arrival = block(carry(
            &computer,
            &prepared,
            &|_, _, _| {},
            std::time::Duration::ZERO,
            true,
        ))
        .unwrap();
        drop(only);
        home.drop_move();
        assert!(matches!(arrival, Arrival::Brought(..)));
        let offers: Vec<bool> = computer
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter_map(|c| match c {
                RemoteCall::OfferMemory { bring, .. } => Some(*bring),
                _ => None,
            })
            .collect();
        assert_eq!(offers, [true], "asked again as a bring-in");
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
            block(offer(&computer, &facts, false)).unwrap(),
            Offered::Accepted {
                here: None,
                bring: true
            }
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
            (
                Ok(RemoteAnswer::MemoryAccepted {
                    here: None,
                    bring: false,
                }),
                MoveError::ComputerOutdated,
            ),
        ] {
            let mut computer = Fake::new();
            computer.offered = said;
            assert_eq!(block(offer(&computer, &facts, false)).err(), Some(shown));
        }
        let mut computer = Fake::new();
        let counts = facts.counts;
        computer.offered = Ok(RemoteAnswer::MemoryArrived { counts });
        assert_eq!(
            block(offer(&computer, &facts, false)).unwrap(),
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
    fn carried(home: &Home, computer: &Fake) -> (Result<Arrival, MoveError>, Vec<MovePhase>) {
        let stop = std::sync::atomic::AtomicBool::new(false);
        let prepared = home.prepare_move(&|_, _, _| {}, &stop).unwrap();
        let phases = Mutex::new(Vec::new());
        let out = block(carry(
            computer,
            &prepared,
            &|phase, _, _| phases.lock().unwrap().push(phase),
            std::time::Duration::ZERO,
            false,
        ));
        home.drop_move();
        (out, phases.into_inner().unwrap())
    }

    /// A computer that adds the things, as every computer does now.
    fn bringing() -> Fake {
        let mut computer = Fake::new();
        computer.brings = true;
        computer.offered = Ok(RemoteAnswer::MemoryAccepted {
            here: None,
            bring: true,
        });
        computer
    }

    #[test]
    fn the_memory_stays_until_the_computer_says_its_things_were_added() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let computer = bringing();
        // Still checking what an earlier try sent, then out of reach, then it knows nothing.
        computer.statuses.lock().unwrap().extend([
            Err(phone_code(PhoneError::Busy)),
            Err(CallError::Connection {
                state: Reach::CantConnect,
            }),
            Err(phone_code(PhoneError::NotReady)),
        ]);
        let (out, phases) = carried(&home, &computer);
        assert!(matches!(out.unwrap(), Arrival::Brought(c, _) if c.items == 2));
        assert!(phases.contains(&MovePhase::Checking));
        assert!(
            !phases.contains(&MovePhase::Paused),
            "waiting for the check is not a pause"
        );
        assert_eq!(sends(&computer), 1);
        assert_eq!(
            home.mode(true),
            Some(PhoneMode::Alone),
            "put aside only by the caller"
        );
        assert!(home.store_dir().exists());
    }

    #[test]
    fn a_computer_that_takes_the_memory_whole_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let computer = Fake::new();
        let (out, phases) = carried(&home, &computer);
        assert_eq!(out.err(), Some(MoveError::Failed));
        assert_eq!(sends(&computer), 1);
        assert!(
            computer
                .calls
                .lock()
                .unwrap()
                .iter()
                .all(|c| !matches!(c, RemoteCall::OfferMemory { .. })),
            "no wait for a restart, and nothing offered again"
        );
        assert!(!phases.contains(&MovePhase::Paused));
        assert_eq!(home.mode(true), Some(PhoneMode::Alone));
        assert_eq!(home.move_facts().unwrap().counts.items, 2);
    }

    #[test]
    fn a_move_cut_part_way_is_offered_again_before_it_goes_again() {
        let dir = tempfile::tempdir().unwrap();
        let (home, _) = alone(dir.path());
        let computer = bringing();
        computer
            .sends
            .lock()
            .unwrap()
            .push_back(CallError::Connection {
                state: Reach::Asleep,
            });
        let (out, phases) = carried(&home, &computer);
        assert!(matches!(out.unwrap(), Arrival::Brought(..)));
        assert!(phases.contains(&MovePhase::Paused));
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
        assert_eq!(out.unwrap(), Arrival::Opened(counts));
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
            block(offer(&computer, &facts, false)).err(),
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
