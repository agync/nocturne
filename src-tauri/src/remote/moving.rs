//! A phone that holds its own memory moves it here. The phone offers it (`offerMemory`) with
//! what it holds; the person on this computer agrees, or keeps what is here and the pairing is
//! undone. The phone then sends a backup in 0013's format, locked with a passphrase made for this
//! one move, as the chunks of `memoryDone`. It is checked and unpacked into the restore stage,
//! takes this computer's own settings and paired phones, and the phone's own row becomes the
//! phone as it is paired here, all before the store in use is touched. The store changes places
//! at the next start, as a restore does: Nocturne says so and starts again by itself a moment
//! after the phone is told every count matched. Until then phones are asked to wait, so nothing
//! they send lands in a store that is about to be put aside. The phone puts its memory aside only
//! once it asks again after the restart and hears that the memory opened.
//!
//! What a phone hears, besides the answers named in the contract:
//!
//! | Answer | To | Means |
//! |---|---|---|
//! | `TooLarge` | `offerMemory` | not enough room here yet; the person frees some, the phone tries again |
//! | `TooLarge` | `memoryDone` | room ran out while it was written out; nothing changed |
//! | `Busy` | either | wait and ask again: a check or the restart is under way, or a backup runs |
//! | `MemoryArrived` | either | the memory opened here after the restart |
//! | `NotFound` | `memoryDone` | the memory could not be opened after the restart; nothing changed |
//! | `NotReady` | `memoryDone` | nothing is known of the phone's memory here; it offers it again |
//!
//! A `memoryDone` of size 0 sends nothing: it asks where the phone's memory stands, which is how a
//! phone waits through the restart without ever starting a new move.
//! | `Paused` | `memoryDone` | the person here stopped the move; nothing changed |
//! | `Revoked` | either | the person kept what is here, or removed the phone |
//!
//! One move at a time. Nothing here keeps the passphrase: it goes into the unpack and is gone.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use nocturne_contract::{MemoryCounts, PhoneError, RemoteAnswer};
use nocturne_engine::{DeviceRow, Engine, MIN_PASSPHRASE_CHARS};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::{Notify, oneshot};

use super::incoming::UploadDone;
use super::pairing::ConvId;
use super::protocol::MAX_MEMORY_BYTES;
use super::session::{Core, ReplySink};
use crate::restore::{self, Aside};
use crate::state::AppState;

/// Sent with the move as it stands (`MoveView`), or null once there is none.
pub const EVENT_MOVE: &str = "move-changed";

/// How long an offer waits for the person on this computer.
const DECIDE_WAIT: Duration = Duration::from_secs(600);
/// How long an agreed move waits for its phone to begin sending.
const AGREED_WAIT: Duration = Duration::from_secs(1800);
/// How long the answer that the memory arrived has to reach the phone before Nocturne starts
/// again with it in place.
const ANSWER_WAIT: Duration = Duration::from_secs(2);
/// How often a move waiting for its upload looks whether the session carrying it is still there.
const LOOK_EVERY: Duration = Duration::from_secs(2);
/// Arriving progress reaches the window at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(200);
/// The longest passphrase a phone may send.
const MAX_PASSPHRASE: usize = 256;
/// A session carrying a memory is not cut at the usual fifteen minutes while the memory is on its
/// way or being checked: a large memory takes longer than that to go across.
pub const MEMORY_SESSION_SECS: u64 = 6 * 60 * 60;
/// Where a finished move is remembered, so a phone that missed the answer is told again.
const MOVED: &str = "moved.json";
/// Room a move needs besides twice its size: the upload and the unpacked stage both sit here
/// before the upload goes.
const ROOM_MARGIN: u64 = 64 * 1024 * 1024;

/// The move as the window shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "phase",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MoveView {
    /// "{name} has its own memory. Move it here?" `here`: this computer already holds one, and
    /// `others`: other phones then send their calendar and health again. `room`: not enough
    /// free room yet, and the move button waits.
    Offered {
        name: String,
        items: u32,
        bytes: f64,
        here: bool,
        others: bool,
        room: Option<Room>,
    },
    Arriving {
        name: String,
        done: f64,
        size: f64,
    },
    Checking {
        name: String,
    },
    /// "Nocturne restarts to open the memory from {name}." for the moment before it does.
    Restarting {
        name: String,
    },
    /// "The memory from {name} is here." after the start that opened it. `online`: Online
    /// answers is already set up here.
    Arrived {
        name: String,
        online: bool,
    },
    /// The move ended and nothing changed: room ran out, or the memory did not open after the
    /// restart. `room` says how much it needs, for the first.
    Failed {
        name: String,
        why: FailedWhy,
        room: Option<Room>,
    },
}

/// How much room a move needs here and how much there is, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    pub need: f64,
    pub free: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FailedWhy {
    NoRoom,
    NotOpened,
}

/// The person's answer to an offer: move it here (putting aside what is here, if anything),
/// or not now (keeping what is here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MoveChoice {
    Move,
    NotNow,
}

#[derive(Debug)]
enum Phase {
    Offered(Option<oneshot::Sender<MoveChoice>>),
    Accepted,
    Arriving {
        done: u64,
        size: u64,
    },
    Checking,
    Arrived(MemoryCounts),
    /// Ended with nothing changed; shown until the person says OK. It holds no place.
    Failed(FailedWhy),
}

/// Each move's own id. What it stages and what it notes carry it, so letting go of one move
/// never touches what another staged.
type Owner = String;

#[derive(Debug)]
struct Move {
    owner: Owner,
    device: String,
    name: String,
    offer: MemoryCounts,
    bytes: f64,
    here: Option<MemoryCounts>,
    online: bool,
    phase: Phase,
    /// The data folder its stage is in.
    data: PathBuf,
    /// Its phone was removed while the memory was being checked: it stays, unseen, until the
    /// check ends and lets its stage go, so no other move starts under it.
    gone: bool,
    /// The person stopped it here: its phone hears so.
    stopped: bool,
    /// Other paired phones send again if this memory replaces the one here.
    others: bool,
    /// Room short when it was offered.
    room: Option<Room>,
    /// The conversation carrying the memory, while it is on its way or being checked.
    conv: Option<ConvId>,
    shown: Option<Instant>,
    /// Since when an agreed move has had nothing coming.
    idle_since: Instant,
}

impl Move {
    fn new(device: &DeviceRow, data: &Path, offer: MemoryCounts, bytes: f64) -> Move {
        let owner = nocturne_channel::crypto::random::<8>()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Move {
            owner,
            device: device.id.clone(),
            name: device.name.clone(),
            offer,
            bytes,
            here: None,
            online: false,
            phase: Phase::Accepted,
            data: data.to_path_buf(),
            gone: false,
            stopped: false,
            others: false,
            room: None,
            conv: None,
            shown: None,
            idle_since: Instant::now(),
        }
    }

    /// Agreed, with nothing coming: it waits for its phone to send again.
    fn agreed_again(&mut self) {
        self.phase = Phase::Accepted;
        self.conv = None;
        self.idle_since = Instant::now();
    }

    fn view(&self) -> MoveView {
        let name = self.name.clone();
        match &self.phase {
            Phase::Offered(_) => MoveView::Offered {
                name,
                items: self.offer.items,
                bytes: self.bytes,
                here: self.here.is_some(),
                others: self.others,
                room: self.room,
            },
            Phase::Accepted => MoveView::Arriving {
                name,
                done: 0.0,
                size: self.bytes,
            },
            Phase::Arriving { done, size } => MoveView::Arriving {
                name,
                done: *done as f64,
                size: *size as f64,
            },
            Phase::Checking => MoveView::Checking { name },
            Phase::Arrived(_) => MoveView::Restarting { name },
            Phase::Failed(why) => MoveView::Failed {
                name,
                why: *why,
                room: (*why == FailedWhy::NoRoom).then(|| Room {
                    need: room_needed(self.bytes as u64) as f64,
                    free: 0.0,
                }),
            },
        }
    }
}

/// A move that finished, remembered beside the backup state.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Moved {
    owner: Owner,
    device: String,
    /// The phone's name, for "The memory from {name} is here." after the next start.
    #[serde(default)]
    name: String,
    counts: MemoryCounts,
    /// The person has seen that it is here, or that it did not open.
    #[serde(default)]
    seen: bool,
    /// It did not open after the restart; nothing changed (set by the start that tried).
    #[serde(default)]
    failed: bool,
    /// Its phone has heard that it did not open.
    #[serde(default)]
    told: bool,
    /// The other phones of a replaced memory, and which of their sources still have to send
    /// again from scratch.
    #[serde(default)]
    resend: Vec<Resend>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Resend {
    device: String,
    sources: Vec<String>,
}

/// The one move under way, if any. Every change of phase is one method here, under one lock:
///
/// ```text
/// (none) --claim--> Offered --decide(move)--> Accepted <--upload failed-- Arriving
///                      |                         |  ^                       ^
///                      +--decide(not now)--> (none) +--------carry----------+
///                                                Accepted/Arriving --begin_check--> Checking
///                                                Checking --conclude(ok)--> Arrived
///                                                Checking --conclude(failed)--> (none)
/// forget: Offered/Accepted/Arriving --> (none); Checking --> Checking, gone (a tombstone the
/// check ends); Arrived --> (none), its stage let go.
/// ```
#[derive(Default)]
pub struct Moves {
    slot: Mutex<Option<Move>>,
    settled: Notify,
    /// A phone whose agreed move the person stopped before its upload began: it hears so once.
    stopped_for: Mutex<Option<String>>,
    /// The last move the person stopped, so its upload ending just after hears why.
    stopped_owner: Mutex<Option<Owner>>,
    /// One change to the note of a move at a time.
    note: Mutex<()>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn shown(slot: &Option<Move>) -> Option<MoveView> {
    slot.as_ref().filter(|m| !m.gone).map(Move::view)
}

/// An agreed move whose phone sent nothing for `AGREED_WAIT` is let go, so it never holds the
/// place for other phones. Looked at whenever the place is.
fn expire_idle(slot: &mut Option<Move>) {
    let idle = slot.as_ref().is_some_and(|m| match &m.phase {
        Phase::Accepted => m.idle_since.elapsed() >= AGREED_WAIT,
        // A question waiting for room, with no offer to answer: as long as an offer waits.
        Phase::Offered(None) => m.idle_since.elapsed() >= DECIDE_WAIT,
        _ => false,
    });
    if idle {
        *slot = None;
    }
}

impl Moves {
    pub fn view(&self) -> Option<MoveView> {
        let mut slot = lock(&self.slot);
        expire_idle(&mut slot);
        shown(&slot)
    }

    /// The conversation carries a memory: it is not cut for its age, and never gives way to a
    /// newer session of the same phone.
    pub fn holds(&self, conv: ConvId) -> bool {
        lock(&self.slot)
            .as_ref()
            .is_some_and(|m| m.conv == Some(conv))
    }

    /// A memory is being checked or waits for the next start: phones are asked to wait.
    pub fn holding(&self) -> bool {
        lock(&self.slot)
            .as_ref()
            .is_some_and(|m| matches!(m.phase, Phase::Checking | Phase::Arrived(_)))
    }

    /// The phone's memory is already here: its upload is not taken again.
    pub fn arrived_for(&self, device: &str) -> bool {
        lock(&self.slot)
            .as_ref()
            .is_some_and(|m| m.device == device && !m.gone && matches!(m.phase, Phase::Arrived(_)))
    }

    /// Move `owner` is here and waits for the next start.
    fn arrived_as(&self, owner: &str) -> bool {
        lock(&self.slot)
            .as_ref()
            .is_some_and(|m| m.owner == owner && !m.gone && matches!(m.phase, Phase::Arrived(_)))
    }

    /// The person agreed and the phone may send its memory.
    pub fn may_upload(&self, device: &str) -> bool {
        let mut slot = lock(&self.slot);
        expire_idle(&mut slot);
        slot.as_ref().is_some_and(|m| {
            m.device == device
                && !m.gone
                && matches!(m.phase, Phase::Accepted | Phase::Arriving { .. })
        })
    }

    /// What this computer held when the person agreed, for a phone carrying on after a pause.
    /// An upload still on its way gives way first: a phone asking again proves it stopped, so
    /// the second answer says whether one was dropped.
    fn agreed(&self, device: &str) -> Option<(Option<MemoryCounts>, bool)> {
        let mut slot = lock(&self.slot);
        expire_idle(&mut slot);
        let m = slot.as_mut().filter(|m| m.device == device && !m.gone)?;
        let dropped = matches!(m.phase, Phase::Arriving { .. });
        if dropped {
            m.agreed_again();
        }
        matches!(m.phase, Phase::Accepted).then_some((m.here, dropped))
    }

    /// The bytes of the memory so far; answers the view when it is due to be shown.
    pub fn arriving(&self, device: &str, done: u64, size: u64) -> Option<MoveView> {
        let mut slot = lock(&self.slot);
        let m = slot.as_mut().filter(|m| m.device == device && !m.gone)?;
        if !matches!(m.phase, Phase::Accepted | Phase::Arriving { .. }) {
            return None;
        }
        m.phase = Phase::Arriving { done, size };
        let due = done >= size || m.shown.is_none_or(|t| t.elapsed() >= PROGRESS_EVERY);
        due.then(|| {
            m.shown = Some(Instant::now());
            m.view()
        })
    }

    /// Take the place for a new offer. Refused while another phone's move, or this phone's
    /// memory past its offer, holds it.
    fn claim(&self, new: Move) -> Option<Option<MoveView>> {
        let mut slot = lock(&self.slot);
        expire_idle(&mut slot);
        // A new offer is a new move: a stop said of the last one no longer applies.
        {
            let mut stopped = lock(&self.stopped_for);
            if stopped.as_deref() == Some(new.device.as_str()) {
                *stopped = None;
            }
        }
        if slot
            .as_ref()
            .is_some_and(|m| matches!(m.phase, Phase::Failed(_)))
        {
            *slot = None;
        }
        let busy = slot.as_ref().is_some_and(|m| {
            m.device != new.device
                || m.gone
                || matches!(
                    m.phase,
                    Phase::Arriving { .. } | Phase::Checking | Phase::Arrived(_)
                )
        });
        if busy {
            return None;
        }
        *slot = Some(new);
        Some(shown(&slot))
    }

    /// The person's answer to the question on screen. `free` is the room here now. Answers
    /// whether it was taken, and the phone to unpair when the answer was "not now" and no offer
    /// was waiting to carry it (the phone tried, heard there was no room, and waits to try
    /// again). Moving waits while there is not room.
    fn decide(
        &self,
        choice: MoveChoice,
        free: impl Fn(&Path) -> Option<u64>,
    ) -> (bool, Option<String>, Option<MoveView>) {
        let mut slot = lock(&self.slot);
        let Some(m) = slot.as_mut().filter(|m| !m.gone) else {
            return (false, None, shown(&slot));
        };
        let Phase::Offered(tx) = &mut m.phase else {
            return (false, None, shown(&slot));
        };
        if choice == MoveChoice::Move
            && let Some(room) = short(m.bytes as u64, free(&m.data))
        {
            // Room went since the question showed: it says so now, and moving waits.
            m.room = Some(room);
            return (false, None, shown(&slot));
        }
        let waiting = tx.take();
        let carried = waiting.is_some_and(|tx| tx.send(choice).is_ok());
        let unpair = (!carried && choice == MoveChoice::NotNow).then(|| m.device.clone());
        if choice == MoveChoice::Move {
            m.room = None;
            m.agreed_again();
        } else {
            *slot = None;
        }
        (true, unpair, shown(&slot))
    }

    /// The room line as it stands: while the question waits for room, how much there is now.
    fn refresh_room(&self, free: impl Fn(&Path) -> Option<u64>) {
        let mut slot = lock(&self.slot);
        if let Some(m) = slot
            .as_mut()
            .filter(|m| matches!(m.phase, Phase::Offered(_)) && m.room.is_some())
        {
            m.room = short(m.bytes as u64, free(&m.data));
        }
    }

    /// "Stop moving" on this computer, until the restart. Answers the phone whose upload, if
    /// one is coming, ends now with `Paused`.
    pub fn stop(&self) -> Option<String> {
        let mut slot = lock(&self.slot);
        let m = slot.as_mut().filter(|m| !m.gone)?;
        let device = m.device.clone();
        if matches!(
            m.phase,
            Phase::Accepted | Phase::Arriving { .. } | Phase::Checking
        ) {
            *lock(&self.stopped_owner) = Some(m.owner.clone());
        }
        match m.phase {
            Phase::Accepted | Phase::Arriving { .. } => {
                if m.conv.is_none() {
                    *lock(&self.stopped_for) = Some(device.clone());
                }
                *slot = None;
            }
            // The check lets go of what it staged once it ends, and its phone hears why.
            Phase::Checking => {
                m.gone = true;
                m.stopped = true;
            }
            _ => return None,
        }
        drop(slot);
        self.settled.notify_waiters();
        Some(device)
    }

    /// A phone whose move the person stopped has not heard yet.
    pub fn stop_waits(&self, device: &str) -> bool {
        lock(&self.stopped_for).as_deref() == Some(device)
    }

    /// The person stopped move `owner`.
    fn stopped(&self, owner: &str) -> bool {
        lock(&self.stopped_owner).as_deref() == Some(owner)
    }

    /// The person stopped this phone's move before its upload began: true once.
    fn was_stopped(&self, device: &str) -> bool {
        let mut stopped = lock(&self.stopped_for);
        if stopped.as_deref() == Some(device) {
            *stopped = None;
            return true;
        }
        false
    }

    /// "OK" on a move that ended with nothing changed.
    fn dismiss(&self) -> bool {
        let mut slot = lock(&self.slot);
        if slot
            .as_ref()
            .is_some_and(|m| matches!(m.phase, Phase::Failed(_)))
        {
            *slot = None;
            return true;
        }
        false
    }

    /// An offer nobody answered, or whose phone went, is let go.
    fn expire(&self, owner: &str) -> Option<MoveView> {
        let mut slot = lock(&self.slot);
        if slot
            .as_ref()
            .is_some_and(|m| m.owner == owner && matches!(m.phase, Phase::Offered(_)))
        {
            *slot = None;
        }
        shown(&slot)
    }

    /// The memory's upload began on `conv`; answers the move it belongs to.
    fn carry(&self, device: &str, conv: ConvId) -> Option<(Owner, Option<MoveView>)> {
        let mut slot = lock(&self.slot);
        let m = slot.as_mut().filter(|m| {
            m.device == device
                && !m.gone
                && matches!(m.phase, Phase::Accepted | Phase::Arriving { .. })
        })?;
        m.conv = Some(conv);
        let owner = m.owner.clone();
        Some((owner, shown(&slot)))
    }

    /// The upload stopped: the phone may send it again.
    fn upload_failed(&self, owner: &str, conv: ConvId) -> Option<MoveView> {
        let mut slot = lock(&self.slot);
        if let Some(m) = slot.as_mut().filter(|m| {
            m.owner == owner
                && m.conv == Some(conv)
                && matches!(m.phase, Phase::Accepted | Phase::Arriving { .. })
        }) {
            m.agreed_again();
        }
        shown(&slot)
    }

    /// The upload ran out of room here: said on both sides, and not tried again by itself.
    fn upload_no_room(&self, owner: &str, conv: ConvId) -> Option<MoveView> {
        let mut slot = lock(&self.slot);
        if let Some(m) = slot.as_mut().filter(|m| {
            m.owner == owner
                && m.conv == Some(conv)
                && matches!(m.phase, Phase::Accepted | Phase::Arriving { .. })
        }) {
            m.phase = Phase::Failed(FailedWhy::NoRoom);
            m.conv = None;
        }
        shown(&slot)
    }

    /// The upload is whole: it is checked now. None when its phone went meanwhile, and then
    /// nothing may be staged for it.
    fn begin_check(&self, owner: &str, conv: ConvId) -> Option<Option<MoveView>> {
        let mut slot = lock(&self.slot);
        let m = slot.as_mut().filter(|m| {
            m.owner == owner
                && m.conv == Some(conv)
                && !m.gone
                && matches!(m.phase, Phase::Accepted | Phase::Arriving { .. })
        })?;
        m.phase = Phase::Checking;
        Some(shown(&slot))
    }

    /// How a check ends: arrived, or let go, with what it staged, when its phone was removed
    /// meanwhile. Only what carries this move's owner is let go, so a check that ends late
    /// never touches another phone's move.
    fn conclude(
        &self,
        data: &Path,
        owner: &str,
        removed: bool,
        checked: Result<MemoryCounts, PhoneError>,
    ) -> (Option<MoveView>, RemoteAnswer) {
        let mut slot = lock(&self.slot);
        let ours = slot
            .as_mut()
            .filter(|m| m.owner == owner && matches!(m.phase, Phase::Checking));
        match (checked, ours) {
            (Ok(counts), Some(m)) if !m.gone && !removed => {
                m.phase = Phase::Arrived(counts);
                m.conv = None;
                (shown(&slot), RemoteAnswer::MemoryArrived { counts })
            }
            // This computer could not finish it now (no room, its disk, a long backup): nothing
            // changed, and the phone carries on without the person being asked again.
            (Err(PhoneError::Busy), Some(m)) if !m.gone && !removed => {
                m.agreed_again();
                (shown(&slot), err(PhoneError::Busy))
            }
            // Room ran out: said on both sides, and nothing is tried again by itself.
            (Err(PhoneError::TooLarge), Some(m)) if !m.gone && !removed => {
                m.phase = Phase::Failed(FailedWhy::NoRoom);
                m.conv = None;
                (shown(&slot), err(PhoneError::TooLarge))
            }
            (checked, ours) => {
                // Whatever this check staged goes, found or not; nothing of another move does.
                if checked.is_ok() {
                    unstage(data, owner);
                }
                let stopped = ours.as_ref().is_some_and(|m| m.stopped);
                if ours.is_some() {
                    *slot = None;
                }
                let answer = match checked {
                    Ok(_) if stopped => err(PhoneError::Paused),
                    Ok(_) => err(PhoneError::Revoked),
                    Err(_) if stopped => err(PhoneError::Paused),
                    // Nothing changed: the phone may offer it again.
                    Err(code) => err(code),
                };
                (shown(&slot), answer)
            }
        }
    }

    /// A phone that was removed takes its move with it. True when there was one to drop.
    pub fn forget(&self, device: &str) -> bool {
        let mut slot = lock(&self.slot);
        let Some(m) = slot.as_mut().filter(|m| m.device == device && !m.gone) else {
            return false;
        };
        match m.phase {
            Phase::Checking => m.gone = true,
            Phase::Arrived(_) => {
                unstage(&m.data, &m.owner);
                *slot = None;
            }
            _ => *slot = None,
        }
        drop(slot);
        self.settled.notify_waiters();
        true
    }

    /// Where this phone's memory stands, for a phone asking again: being checked, or staged
    /// for the restart (`Ok`); neither is an answer yet.
    fn state_of(&self, device: &str) -> Option<Result<MemoryCounts, ()>> {
        let slot = lock(&self.slot);
        match slot
            .as_ref()
            .filter(|m| m.device == device)
            .map(|m| &m.phase)
        {
            Some(Phase::Arrived(counts)) => Some(Ok(*counts)),
            Some(Phase::Checking) => Some(Err(())),
            _ => None,
        }
    }
}

fn emit<R: Runtime>(app: &AppHandle<R>, view: Option<MoveView>) {
    if let Err(e) = app.emit(EVENT_MOVE, view) {
        tracing::warn!("could not emit the move: {e}");
    }
}

fn err(code: PhoneError) -> RemoteAnswer {
    RemoteAnswer::Error { code }
}

/// The room a move of `bytes` needs here: the upload and the unpacked stage, and a margin.
fn room_needed(bytes: u64) -> u64 {
    bytes.saturating_mul(2).saturating_add(ROOM_MARGIN)
}

/// The room line for a move of `bytes`, when `free` is short of it. Room that cannot be read is
/// never short: the write itself says when it runs out.
fn short(bytes: u64, free: Option<u64>) -> Option<Room> {
    let need = room_needed(bytes);
    free.filter(|f| *f < need).map(|f| Room {
        need: need as f64,
        free: f as f64,
    })
}

#[cfg(debug_assertions)]
static PRETEND_FREE: Mutex<Vec<(PathBuf, u64)>> = Mutex::new(Vec::new());

/// Tests: the room this data folder has, whatever the disk says.
#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn pretend_free(data: &Path, bytes: Option<u64>) {
    let mut all = lock(&PRETEND_FREE);
    all.retain(|(d, _)| d != data);
    if let Some(bytes) = bytes {
        all.push((data.to_path_buf(), bytes));
    }
}

/// Free bytes on the disk that holds the data folder.
fn free_here(data: &Path) -> Option<u64> {
    #[cfg(debug_assertions)]
    if let Some((_, b)) = lock(&PRETEND_FREE).iter().find(|(d, _)| d == data) {
        return Some(*b);
    }
    let probe = dunce::canonicalize(data).ok()?;
    let disks = sysinfo::Disks::new_with_refreshed_list().ok()?;
    disks
        .iter()
        .filter(|d| probe.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

/// What a store holds of the person's: none when nothing was kept, read or made.
fn held(e: &Engine) -> nocturne_engine::Result<Option<MemoryCounts>> {
    let holdings = e.holdings(None)?.unwrap_or_default();
    let items = e.stats()?.item_count;
    let counts = MemoryCounts {
        items,
        memories: holdings.memories,
        pages: holdings.pages,
        originals: holdings.originals,
    };
    Ok((counts.items + counts.memories + counts.pages > 0).then_some(counts))
}

/// The file in a stage that names the move it belongs to.
const OWNER: &str = "move-owner";

/// Nothing of move `owner` waits for the next start any more. What another move staged or
/// noted stays.
fn unstage(data: &Path, owner: &str) {
    let stage = restore::stage_dir(data);
    if std::fs::read_to_string(stage.join(OWNER)).is_ok_and(|o| o == owner) {
        let _ = std::fs::remove_file(restore::marker_path(data));
        // A quick rename here, so the move's lock is never held while gigabytes are shredded.
        let gone = restore::restore_dir(data).join(format!("{}{owner}", restore::LET_GO));
        match std::fs::rename(&stage, &gone) {
            Ok(()) => {
                std::thread::spawn(move || crate::fsutil::shred_tree(&gone));
            }
            Err(_) => restore::clear_staging(data),
        }
    }
    if read_moved(data).is_some_and(|m| m.owner == owner) {
        let _ = std::fs::remove_file(moved_path(data));
    }
}

/// Where the note of a move is once its memory is in place. Until then it sits in the stage
/// (`restore::ARRIVAL_NOTE`), so a stage that never goes in place takes its note with it.
fn moved_path(data: &Path) -> PathBuf {
    data.join("backup").join(MOVED)
}

fn staged_note(data: &Path) -> PathBuf {
    restore::stage_dir(data).join(restore::ARRIVAL_NOTE)
}

fn read_note(path: &Path) -> Option<Moved> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn read_moved(data: &Path) -> Option<Moved> {
    read_note(&moved_path(data))
}

fn write_note(path: &Path, moved: &Moved) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        crate::fsutil::create_private_dir(dir)?;
    }
    let bytes = serde_json::to_vec(moved).map_err(std::io::Error::other)?;
    crate::fsutil::write_private(path, &bytes)
}

/// A memory that moved here and is now in use, until the person has seen it: the window shows
/// "The memory from {name} is here." after the start that put it in place.
pub fn arrived_here(data: &Path, online: bool) -> Option<MoveView> {
    let moved = read_moved(data).filter(|m| !m.seen)?;
    if restore::marker_path(data).exists() {
        return None;
    }
    Some(match moved.failed {
        true => MoveView::Failed {
            name: moved.name,
            why: FailedWhy::NotOpened,
            room: None,
        },
        false => MoveView::Arrived {
            name: moved.name,
            online,
        },
    })
}

/// "Open today's page": the person saw that the memory is here. False when none waits to be
/// seen.
pub fn seen(moves: &Moves, data: &Path) -> bool {
    let _one = lock(&moves.note);
    // In place already, or waiting for the next start.
    for path in [moved_path(data), staged_note(data)] {
        if let Some(mut moved) = read_note(&path).filter(|m| !m.seen) {
            moved.seen = true;
            return write_note(&path, &moved).is_ok();
        }
    }
    false
}

fn whole(n: f64) -> bool {
    n.is_finite() && n >= 1.0 && n.fract() == 0.0
}

fn is_digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `offerMemory`: wait for the person on this computer, then welcome the memory or undo the
/// pairing. A phone that asks again after its memory arrived is told so.
pub async fn offer<R: Runtime>(
    app: &AppHandle<R>,
    core: &Arc<Core>,
    out: &ReplySink,
    device: &DeviceRow,
    counts: MemoryCounts,
    bytes: f64,
    schema: u32,
) -> Option<RemoteAnswer> {
    if !whole(bytes) || schema == 0 {
        return Some(err(PhoneError::Invalid));
    }
    if bytes > MAX_MEMORY_BYTES as f64 {
        return Some(err(PhoneError::TooLarge));
    }
    if schema > nocturne_engine::store::latest_version() {
        return Some(err(PhoneError::ComputerOutdated));
    }
    let Some(state) = app.try_state::<AppState>() else {
        return Some(err(PhoneError::NotReady));
    };
    let data = state.data_dir.clone();
    if let Some(answer) = already(core, &device.id, &data).await {
        return Some(answer);
    }
    let Ok(e) = state.engine() else {
        return Some(err(PhoneError::NotReady));
    };
    let paired = device.id.clone();
    let looked = crate::error::blocking(move || {
        let others = e.devices(false)?.iter().any(|d| d.id != paired);
        Ok((held(&e)?, e.online_state()?.set_up, others))
    })
    .await;
    let Ok((here, online, others)) = looked else {
        return Some(err(PhoneError::Busy));
    };
    // Agreed to already: a phone carrying on after a pause is not asked about twice.
    if let Some((here, dropped)) = core.moves.agreed(&device.id) {
        if dropped {
            core.incoming.cancel(&device.id);
        }
        return Some(RemoteAnswer::MemoryAccepted { here });
    }
    // A restore that waits for the next start is not replaced by a move.
    if restore::marker_path(&data).exists() {
        return Some(err(PhoneError::Busy));
    }
    let room = {
        let data = data.clone();
        tokio::task::spawn_blocking(move || free_here(&data))
            .await
            .ok()
            .flatten()
    };
    let room = short(bytes as u64, room);
    let (tx, rx) = oneshot::channel();
    let mut new = Move::new(device, &data, counts, bytes);
    new.here = here;
    new.online = online;
    new.others = here.is_some() && others;
    new.room = room;
    // Short of room, the question shows with its room line and the phone hears so at once: it
    // tries again once the person here has freed some.
    new.phase = Phase::Offered(room.is_none().then_some(tx));
    let owner = new.owner.clone();
    let Some(view) = core.moves.claim(new) else {
        return Some(err(PhoneError::Busy));
    };
    emit(app, view);
    crate::window::show_main(app);
    if room.is_some() {
        return Some(err(PhoneError::TooLarge));
    }
    let choice = tokio::select! {
        choice = rx => Some(choice),
        () = out.closed() => None,
        () = tokio::time::sleep(DECIDE_WAIT) => None,
    };
    match choice {
        Some(Ok(MoveChoice::Move)) => Some(RemoteAnswer::MemoryAccepted { here }),
        Some(Ok(MoveChoice::NotNow)) => {
            // Keep what is here: the phone stays as it was, alone and unpaired. The pairing goes
            // first, then the phone is told, then its sessions end.
            revoke(app, &device.id).await;
            out.error(PhoneError::Revoked);
            core.drop_device(&device.id);
            core.shell.changed();
            None
        }
        // A newer offer from the same phone took this one's place.
        Some(Err(_)) => Some(err(PhoneError::Busy)),
        // Nobody answered, or the phone went.
        None => {
            emit(app, core.moves.expire(&owner));
            Some(err(PhoneError::TooLate))
        }
    }
}

/// Undo the pairing of a phone whose memory stays where it is.
async fn unpair<R: Runtime>(app: &AppHandle<R>, core: &Core, device: &str) {
    revoke(app, device).await;
    core.drop_device(device);
    core.shell.changed();
}

async fn revoke<R: Runtime>(app: &AppHandle<R>, device: &str) {
    if let Some(state) = app.try_state::<AppState>()
        && let Ok(e) = state.engine()
    {
        let id = device.to_string();
        if crate::error::blocking(move || e.revoke_device(&id))
            .await
            .is_err()
        {
            tracing::warn!("could not undo a pairing the person declined");
        }
    }
}

/// A memory being checked is waited for; one staged for the restart has the phone wait; one
/// that opened after it is answered from what arrived. For an offer, a memory that did not open
/// is let go, so the offer is a new move.
async fn already(core: &Arc<Core>, device: &str, data: &Path) -> Option<RemoteAnswer> {
    match settled_state(core, device).await {
        Some(answer) => Some(answer),
        None => opened_here(core, data, device, false),
    }
}

/// Where this phone's memory stands, for a `memoryDone` that sends nothing.
async fn status(core: &Arc<Core>, device: &str, data: &Path) -> RemoteAnswer {
    match settled_state(core, device).await {
        Some(answer) => answer,
        None => opened_here(core, data, device, true).unwrap_or(err(PhoneError::NotReady)),
    }
}

/// `Busy` while staged for the restart, after any check under way; none when nothing is under
/// way.
async fn settled_state(core: &Arc<Core>, device: &str) -> Option<RemoteAnswer> {
    loop {
        let settled = core.moves.settled.notified();
        match core.moves.state_of(device) {
            Some(Ok(_)) => return Some(err(PhoneError::Busy)),
            Some(Err(())) => settled.await,
            None => return None,
        }
    }
}

/// What the start after a move found: the memory opened (`MemoryArrived`, as often as asked),
/// or it did not and nothing changed. A phone asking where it stands hears `NotFound` until it
/// offers its memory again; that offer lets the note go, so the move starts anew.
fn opened_here(core: &Core, data: &Path, device: &str, asking: bool) -> Option<RemoteAnswer> {
    let _one = lock(&core.moves.note);
    let mut moved = read_moved(data).filter(|m| m.device == device)?;
    if !moved.failed {
        return Some(RemoteAnswer::MemoryArrived {
            counts: moved.counts,
        });
    }
    if moved.told {
        return None;
    }
    if asking {
        return Some(err(PhoneError::NotFound));
    }
    moved.told = true;
    if let Err(e) = write_note(&moved_path(data), &moved) {
        tracing::warn!("could not note that the phone tried again: {e}");
    }
    None
}

/// The first calendar or health send from a phone whose sends went aside with a replaced
/// memory: true once per source, and the phone then sends that source again from scratch.
pub fn resend_due(core: &Core, data: &Path, device: &str, source: &str) -> bool {
    let _one = lock(&core.moves.note);
    let Some(mut moved) = read_moved(data).filter(|m| !m.failed) else {
        return false;
    };
    let Some(r) = moved.resend.iter_mut().find(|r| r.device == device) else {
        return false;
    };
    let before = r.sources.len();
    r.sources.retain(|s| s != source);
    if r.sources.len() == before {
        return false;
    }
    moved.resend.retain(|r| !r.sources.is_empty());
    if let Err(e) = write_note(&moved_path(data), &moved) {
        tracing::warn!("could not note a phone asked to send again: {e}");
    }
    true
}

/// The person's answer on this computer. False when there was nothing to answer, or no room
/// yet to move it here.
pub async fn decide<R: Runtime>(app: &AppHandle<R>, core: &Core, choice: MoveChoice) -> bool {
    let (taken, unpair_now, view) = core.moves.decide(choice, free_here);
    emit(app, view);
    if let Some(device) = unpair_now {
        unpair(app, core, &device).await;
    }
    taken
}

/// "Stop moving" on this computer: the phone's upload ends, and the phone hears the move was
/// stopped. False when there is nothing to stop.
pub fn stop<R: Runtime>(app: &AppHandle<R>, core: &Core) -> bool {
    let Some(device) = core.moves.stop() else {
        return false;
    };
    core.incoming.fail(&device, PhoneError::Paused);
    emit(app, core.moves.view());
    true
}

/// What the phone sent with `memoryDone`.
pub struct Done {
    pub conv: ConvId,
    pub call: u32,
    pub size: f64,
    pub sha256: String,
    pub passphrase: String,
}

/// `memoryDone`: take the upload, check it, stage it and settle it, and answer what arrived.
pub async fn done<R: Runtime>(
    app: &AppHandle<R>,
    core: &Arc<Core>,
    out: &ReplySink,
    device: &DeviceRow,
    upload: Option<UploadDone>,
    sent: Done,
) -> Option<RemoteAnswer> {
    let Some(state) = app.try_state::<AppState>() else {
        return Some(err(PhoneError::NotReady));
    };
    let data = state.data_dir.clone();
    let (conv, call) = (sent.conv, sent.call);
    if core.moves.was_stopped(&device.id) {
        core.incoming.cancel_call(&device.id, conv, call);
        return Some(err(PhoneError::Paused));
    }
    let Some(upload) = upload else {
        return Some(status(core, &device.id, &data).await);
    };
    if !whole(sent.size)
        || !is_digest(&sent.sha256)
        || sent.passphrase.chars().count() < MIN_PASSPHRASE_CHARS
        || sent.passphrase.len() > MAX_PASSPHRASE
    {
        core.incoming.cancel_call(&device.id, conv, call);
        return Some(err(PhoneError::Invalid));
    }
    let Some((owner, view)) = core.moves.carry(&device.id, conv) else {
        core.incoming.cancel_call(&device.id, conv, call);
        return Some(err(PhoneError::NotReady));
    };
    emit(app, view);
    let file = match wait_upload(core, &device.id, conv, call, upload).await {
        Ok(file) => file,
        Err(PhoneError::TooLarge) => {
            emit(app, core.moves.upload_no_room(&owner, conv));
            return Some(err(PhoneError::TooLarge));
        }
        Err(code) => {
            emit(app, core.moves.upload_failed(&owner, conv));
            let code = match core.moves.stopped(&owner) {
                true => PhoneError::Paused,
                false => code,
            };
            return Some(err(code));
        }
    };
    // Its phone may have gone, or the person stopped it, while the memory came: then nothing is
    // staged for it.
    let Some(view) = core.moves.begin_check(&owner, conv) else {
        let _ = std::fs::remove_file(&file);
        return Some(err(match core.moves.stopped(&owner) {
            true => PhoneError::Paused,
            false => PhoneError::Revoked,
        }));
    };
    emit(app, view);
    state.asks.cancel_all();

    let checked = check(&state, device, &owner, &file, sent).await;
    let _ = std::fs::remove_file(&file);

    // The phone may have been removed while its memory was checked: then what was staged goes.
    let (view, answer) = core
        .moves
        .conclude(&data, &owner, core.removed(&device.id), checked);
    core.moves.settled.notify_waiters();
    emit(app, view);
    let arrived = matches!(answer, RemoteAnswer::MemoryArrived { .. });
    out.send(answer);
    if arrived && let Some(restart) = state.restarter() {
        // As a restore does: Nocturne starts again with the memory in place, once the phone
        // has had its answer. Until then phones are asked to wait. A phone removed meanwhile
        // took its memory with it, and nothing starts again.
        tokio::time::sleep(ANSWER_WAIT).await;
        if core.moves.arrived_as(&owner) {
            restart();
        }
    }
    None
}

/// Check and stage the memory, after any backup or restore that is running, and never over a
/// restore that waits for the next start.
async fn check(
    state: &AppState,
    device: &DeviceRow,
    owner: &str,
    file: &Path,
    sent: Done,
) -> Result<MemoryCounts, PhoneError> {
    let started = Instant::now();
    loop {
        if state.begin_backup().is_ok() {
            break;
        }
        if started.elapsed() >= DECIDE_WAIT {
            return Err(PhoneError::Busy);
        }
        tokio::time::sleep(LOOK_EVERY).await;
    }
    let data = state.data_dir.clone();
    let out = if restore::marker_path(&data).exists() {
        Err(PhoneError::Busy)
    } else {
        // What is here now, not when the person agreed: a lot may have been kept since. The
        // other phones send again only if this memory replaces one.
        let paired = device.id.clone();
        let (here, resend) = match state.engine() {
            Ok(e) => crate::error::blocking(move || {
                let here = held(&e)?;
                let others = match here {
                    Some(_) => e
                        .devices(false)?
                        .into_iter()
                        .filter(|d| d.id != paired)
                        .map(|d| Resend {
                            device: d.id,
                            sources: vec!["calendar".into(), "health".into()],
                        })
                        .collect(),
                    None => Vec::new(),
                };
                Ok((here, others))
            })
            .await
            .unwrap_or_default(),
            Err(_) => (None, Vec::new()),
        };
        // Phones a replace before this one still has to ask stay asked.
        let mut resend = resend;
        for r in read_moved(&data).map(|m| m.resend).unwrap_or_default() {
            if !resend.iter().any(|n| n.device == r.device) {
                resend.push(r);
            }
        }
        let place = Place {
            data,
            paired: device.id.clone(),
            name: device.name.clone(),
            resend,
            here,
            stamp: state
                .now()
                .to_zoned(state.clock.tz())
                .strftime("%Y-%m-%d %H%M%S")
                .to_string(),
            owner: owner.to_string(),
        };
        let (clock, file) = (state.clock.clone(), file.to_path_buf());
        tokio::task::spawn_blocking(move || settle(&place, &file, &sent, clock))
            .await
            .unwrap_or(Err(PhoneError::Busy))
    };
    state.end_backup();
    out
}

/// The upload, once its last chunk is in; given up once its session is gone.
async fn wait_upload(
    core: &Arc<Core>,
    device: &str,
    conv: ConvId,
    call: u32,
    mut upload: UploadDone,
) -> Result<PathBuf, PhoneError> {
    loop {
        match tokio::time::timeout(LOOK_EVERY, &mut upload).await {
            Ok(Ok(Ok(up))) => return Ok(up.path),
            Ok(Ok(Err(code))) => return Err(code),
            Ok(Err(_)) => return Err(PhoneError::SessionExpired),
            Err(_) if core.conv_alive(conv) => continue,
            Err(_) => {
                core.incoming.cancel_call(device, conv, call);
                return Err(PhoneError::SessionExpired);
            }
        }
    }
}

/// Busy for what may pass (no room for a file, the disk, a busy store), so the phone carries on;
/// Invalid for what never will.
fn transient(e: &nocturne_engine::Error) -> PhoneError {
    use nocturne_engine::Error;
    match e {
        Error::Io(io) if no_room(io) => PhoneError::TooLarge,
        Error::Io(_) | Error::Busy | Error::LeaseLost => PhoneError::Busy,
        _ => PhoneError::Invalid,
    }
}

/// The disk is full, or this file would not fit on it.
fn no_room(io: &std::io::Error) -> bool {
    matches!(
        io.kind(),
        std::io::ErrorKind::StorageFull
            | std::io::ErrorKind::QuotaExceeded
            | std::io::ErrorKind::FileTooLarge
    )
}

/// Where a checked memory is staged, and for whom.
struct Place {
    data: PathBuf,
    paired: String,
    name: String,
    /// The other phones, when this memory replaces the one here.
    resend: Vec<Resend>,
    here: Option<MemoryCounts>,
    stamp: String,
    owner: Owner,
}

/// Check the file against what the phone said, unpack it into the restore stage, settle it and
/// ask the next start to put it in place. Anything short of that leaves no stage behind.
fn settle(
    place: &Place,
    file: &Path,
    sent: &Done,
    clock: Arc<dyn nocturne_engine::Clock>,
) -> Result<MemoryCounts, PhoneError> {
    let Place {
        data,
        paired,
        name,
        here,
        stamp,
        owner,
        resend,
    } = place;
    let (here, data) = (*here, data.as_path());
    let damaged = |_| PhoneError::Invalid;
    let size = std::fs::metadata(file).map_err(damaged)?.len();
    if size as f64 != sent.size || digest(file).map_err(damaged)? != sent.sha256 {
        return Err(PhoneError::Invalid);
    }
    restore::clear_staging(data);
    let staged = (|| {
        crate::fsutil::create_private_dir(&restore::restore_dir(data))
            .map_err(|_| PhoneError::Busy)?;
        let quiet = |_, _, _| {};
        let summary = nocturne_engine::unpack_backup(
            file,
            &sent.passphrase,
            &restore::stage_dir(data),
            &quiet,
            &AtomicBool::new(false),
        )
        .map_err(|e| match e {
            nocturne_engine::Error::BackupTooNew => PhoneError::ComputerOutdated,
            // This computer could not write it out: no room, or its disk said no.
            nocturne_engine::Error::Io(_) => transient(&e),
            _ => PhoneError::Invalid,
        })?;
        // The unpack checked every part against what the phone wrote, and counted what is in it.
        let counts = MemoryCounts {
            items: summary.items,
            memories: summary.memories,
            pages: summary.pages,
            originals: summary.originals,
        };
        let store = restore::stage_dir(data).join("store");
        let e = Engine::open(&store, clock).map_err(|e| transient(&e))?;
        e.settle_arrival(&data.join("store"), paired)
            .map_err(|e| transient(&e))?;
        drop(e);
        crate::fsutil::write_private(&restore::stage_dir(data).join(OWNER), owner.as_bytes())
            .map_err(|_| PhoneError::Busy)?;
        // Noted before the marker, so a start that puts the memory in place always knows whose
        // it was and that its phone was told.
        let moved = Moved {
            owner: owner.clone(),
            device: paired.clone(),
            name: name.clone(),
            counts,
            seen: false,
            failed: false,
            told: false,
            resend: resend.clone(),
        };
        write_note(&staged_note(data), &moved).map_err(|_| PhoneError::Busy)?;
        let aside = here.map(|h| Aside {
            at: stamp.clone(),
            memories: h.memories,
            pages: h.pages,
            originals: h.originals,
            bytes: 0.0,
        });
        restore::schedule_arrival(data, here.is_some(), stamp, aside)
            .map_err(|_| PhoneError::Busy)?;
        Ok(counts)
    })();
    if staged.is_err() {
        restore::clear_staging(data);
        let _ = std::fs::remove_file(restore::marker_path(data));
    }
    staged
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

/// The move as the window shows it: one under way, or what the start after one found that the
/// person has not seen yet.
pub fn view<R: Runtime>(app: &AppHandle<R>, core: &Core) -> Option<MoveView> {
    core.moves.refresh_room(free_here);
    if let Some(view) = core.moves.view() {
        return Some(view);
    }
    let state = app.try_state::<AppState>()?;
    let online = state
        .engine()
        .ok()
        .and_then(|e| e.online_state().ok())
        .is_some_and(|o| o.set_up);
    arrived_here(&state.data_dir, online)
}

/// "Open today's page" or "OK": the person has seen how the move ended. False when nothing
/// waits to be seen.
pub fn finish(core: &Core, data: &Path) -> bool {
    core.moves.dismiss() || seen(&core.moves, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(items: u32) -> MemoryCounts {
        MemoryCounts {
            items,
            memories: 0,
            pages: 0,
            originals: 0,
        }
    }

    fn phone(id: &str) -> DeviceRow {
        DeviceRow {
            id: id.into(),
            name: id.into(),
            public_key: Vec::new(),
            home_origin: String::new(),
            connection: nocturne_contract::RemoteConnection::Simple,
            local: false,
            address: None,
            paired_at: jiff::Timestamp::UNIX_EPOCH,
            last_used_at: None,
            revoked_at: None,
        }
    }

    /// Offered, agreed and on its way: answers the move's owner.
    fn agreed(moves: &Moves, data: &Path, id: &str) -> Owner {
        let (tx, _rx) = oneshot::channel();
        let mut m = Move::new(&phone(id), data, counts(1), 10.0);
        m.phase = Phase::Offered(Some(tx));
        assert!(moves.claim(m).is_some(), "{id} may offer");
        // The receiver is gone, so `decide` cannot send: agree in place as `decide` does.
        if let Some(m) = lock(&moves.slot).as_mut() {
            m.phase = Phase::Accepted;
        }
        moves.carry(id, 1).expect("it may send").0
    }

    /// What `settle` leaves for the next start.
    fn stage_as(data: &Path, owner: &str, device: &str) {
        std::fs::create_dir_all(restore::stage_dir(data).join("store")).unwrap();
        std::fs::write(restore::stage_dir(data).join(OWNER), owner).unwrap();
        let moved = Moved {
            owner: owner.into(),
            device: device.into(),
            name: device.into(),
            counts: counts(3),
            seen: false,
            failed: false,
            told: false,
            resend: vec![Resend {
                device: "b".into(),
                sources: vec!["calendar".into(), "health".into()],
            }],
        };
        write_note(&staged_note(data), &moved).unwrap();
        std::fs::write(restore::marker_path(data), b"{}").unwrap();
    }

    fn staged(data: &Path) -> bool {
        restore::marker_path(data).exists() || restore::stage_dir(data).exists()
    }

    #[test]
    fn a_move_goes_through_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        assert!(matches!(moves.view(), Some(MoveView::Arriving { .. })));
        assert!(moves.begin_check(&owner, 1).is_some());
        assert!(moves.holding());
        stage_as(dir.path(), &owner, "a");
        let (view, answer) = moves.conclude(dir.path(), &owner, false, Ok(counts(3)));
        assert!(matches!(view, Some(MoveView::Restarting { .. })));
        assert_eq!(answer, RemoteAnswer::MemoryArrived { counts: counts(3) });
        assert!(staged(dir.path()));
        assert!(
            read_moved(dir.path()).is_none(),
            "not before it is in place"
        );
        assert!(read_note(&staged_note(dir.path())).is_some());
    }

    #[test]
    fn removed_before_the_offer_there_is_nothing_to_drop() {
        let moves = Moves::default();
        assert!(!moves.forget("a"));
        assert!(moves.carry("a", 1).is_none(), "and nothing may be sent");
    }

    #[test]
    fn removed_while_the_offer_waits_the_question_goes() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let (tx, _rx) = oneshot::channel();
        let mut m = Move::new(&phone("a"), dir.path(), counts(1), 10.0);
        m.phase = Phase::Offered(Some(tx));
        moves.claim(m).unwrap();
        assert!(moves.forget("a"));
        assert_eq!(moves.view(), None);
        assert!(!moves.decide(MoveChoice::Move, |_| None).0);
    }

    #[test]
    fn removed_during_the_upload_nothing_is_checked_or_staged() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        moves.arriving("a", 5, 10);
        assert!(moves.forget("a"));
        assert_eq!(moves.view(), None);
        assert!(
            moves.begin_check(&owner, 1).is_none(),
            "the check never starts"
        );
        assert!(!staged(dir.path()));
    }

    #[test]
    fn removed_during_the_check_what_it_stages_goes_when_it_ends() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        moves.begin_check(&owner, 1).unwrap();
        assert!(moves.forget("a"));
        assert_eq!(moves.view(), None, "nothing shows of it");
        assert!(moves.holding(), "phones still wait");
        // The check stages after the removal, then ends.
        stage_as(dir.path(), &owner, "a");
        let (view, answer) = moves.conclude(dir.path(), &owner, false, Ok(counts(3)));
        assert_eq!((view, answer), (None, err(PhoneError::Revoked)));
        assert!(!staged(dir.path()));
        assert!(read_moved(dir.path()).is_none());
        assert!(lock(&moves.slot).is_none());
    }

    #[test]
    fn removed_once_staged_but_before_the_check_ends_it_goes_too() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        moves.begin_check(&owner, 1).unwrap();
        stage_as(dir.path(), &owner, "a");
        // Revoked on the computer: `removed` says so even without `forget`.
        let (view, answer) = moves.conclude(dir.path(), &owner, true, Ok(counts(3)));
        assert_eq!((view, answer), (None, err(PhoneError::Revoked)));
        assert!(!staged(dir.path()));
    }

    #[test]
    fn removed_after_it_arrived_the_staged_memory_goes() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        moves.begin_check(&owner, 1).unwrap();
        stage_as(dir.path(), &owner, "a");
        moves.conclude(dir.path(), &owner, false, Ok(counts(3)));
        assert!(moves.forget("a"));
        assert!(!staged(dir.path()));
        assert!(read_moved(dir.path()).is_none());
        assert_eq!(moves.view(), None);
    }

    #[test]
    fn a_failed_check_frees_the_place_and_stages_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        moves.begin_check(&owner, 1).unwrap();
        let (view, answer) = moves.conclude(dir.path(), &owner, false, Err(PhoneError::Invalid));
        assert_eq!((view, answer), (None, err(PhoneError::Invalid)));
        assert!(lock(&moves.slot).is_none());
    }

    #[test]
    fn two_phones_never_undo_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let a = agreed(&moves, dir.path(), "a");
        moves.begin_check(&a, 1).unwrap();
        moves.forget("a");

        // Phone b cannot start under a's check, tombstone or not.
        let mut b_move = Move::new(&phone("b"), dir.path(), counts(1), 10.0);
        b_move.phase = Phase::Accepted;
        assert!(moves.claim(b_move).is_none(), "b is asked to wait");

        // a's check ends and lets go; then b moves and stages.
        stage_as(dir.path(), &a, "a");
        moves.conclude(dir.path(), &a, false, Ok(counts(3)));
        let b = agreed(&moves, dir.path(), "b");
        moves.begin_check(&b, 1).unwrap();
        stage_as(dir.path(), &b, "b");

        // A late end of a's check (a second report of it) touches nothing of b's.
        let (view, answer) = moves.conclude(dir.path(), &a, true, Ok(counts(3)));
        assert_eq!(answer, err(PhoneError::Revoked));
        assert!(matches!(view, Some(MoveView::Checking { .. })));
        assert!(staged(dir.path()), "b's stage stays");
        assert!(moves.forget("b"));
        assert!(!moves.forget("a"));
        unstage(dir.path(), &a);
        assert!(staged(dir.path()), "letting go of a never takes b's stage");
        unstage(dir.path(), &b);
        assert!(!staged(dir.path()));
    }

    #[test]
    fn a_late_failure_from_an_old_session_leaves_the_new_upload_alone() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        moves.arriving("a", 5, 10);
        // The phone asks again on a new session once the first is gone, and sends again.
        assert_eq!(moves.agreed("a"), Some((None, true)));
        let (again, _) = moves.carry("a", 2).unwrap();
        assert_eq!(again, owner);
        // The first upload's failure arrives late: it names the old session.
        moves.upload_failed(&owner, 1);
        assert!(moves.holds(2), "the new session keeps its long life");
        assert!(moves.begin_check(&owner, 1).is_none());
        assert!(moves.begin_check(&owner, 2).is_some());
    }

    #[test]
    fn a_transient_failure_keeps_the_move_agreed() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        moves.begin_check(&owner, 1).unwrap();
        let (view, answer) = moves.conclude(dir.path(), &owner, false, Err(PhoneError::Busy));
        assert_eq!(answer, err(PhoneError::Busy));
        assert!(matches!(view, Some(MoveView::Arriving { .. })));
        assert_eq!(moves.agreed("a"), Some((None, false)), "not asked again");
    }

    #[test]
    fn an_agreed_move_with_nothing_sent_is_let_go() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        moves.upload_failed(&owner, 1);
        assert!(moves.view().is_some(), "it waits a while");
        if let Some(m) = lock(&moves.slot).as_mut() {
            m.idle_since = Instant::now() - AGREED_WAIT;
        }
        assert_eq!(moves.view(), None);
        // The place is free for another phone.
        let mut b = Move::new(&phone("b"), dir.path(), counts(1), 10.0);
        b.phase = Phase::Accepted;
        assert!(moves.claim(b).is_some());
    }

    #[test]
    fn the_window_says_it_arrived_after_the_start_that_put_it_in_place() {
        let dir = tempfile::tempdir().unwrap();
        stage_as(dir.path(), "o1", "a");
        assert_eq!(
            arrived_here(dir.path(), false),
            None,
            "not before it is in place"
        );
        // The next start put it in place: the note moved beside the backup state.
        let note = read_note(&staged_note(dir.path())).unwrap();
        write_note(&moved_path(dir.path()), &note).unwrap();
        std::fs::remove_file(restore::marker_path(dir.path())).unwrap();
        assert!(matches!(
            arrived_here(dir.path(), true),
            Some(MoveView::Arrived { online: true, .. })
        ));
        assert!(seen(&Moves::default(), dir.path()));
        assert_eq!(
            arrived_here(dir.path(), true),
            None,
            "once seen, never again"
        );
    }

    fn in_place(data: &Path, failed: bool) {
        stage_as(data, "o1", "a");
        let mut note = read_note(&staged_note(data)).unwrap();
        note.failed = failed;
        write_note(&moved_path(data), &note).unwrap();
        std::fs::remove_file(restore::marker_path(data)).unwrap();
        restore::clear_staging(data);
    }

    #[test]
    fn a_phone_asking_after_the_restart_hears_whether_the_memory_opened() {
        let dir = tempfile::tempdir().unwrap();
        in_place(dir.path(), false);
        let opened = read_moved(dir.path()).unwrap();
        assert_eq!(opened.counts, counts(3));
        assert!(matches!(
            arrived_here(dir.path(), false),
            Some(MoveView::Arrived { .. })
        ));

        let failed = tempfile::tempdir().unwrap();
        in_place(failed.path(), true);
        assert!(matches!(
            arrived_here(failed.path(), false),
            Some(MoveView::Failed {
                why: FailedWhy::NotOpened,
                ..
            })
        ));
    }

    #[test]
    fn short_of_room_the_question_waits_and_moving_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let mut m = Move::new(&phone("a"), dir.path(), counts(1), 1000.0);
        m.room = short(1000, Some(10));
        m.phase = Phase::Offered(None);
        moves.claim(m).unwrap();
        assert!(matches!(
            moves.view(),
            Some(MoveView::Offered { room: Some(_), .. })
        ));
        assert!(
            !moves.decide(MoveChoice::Move, |_| Some(10)).0,
            "no room yet"
        );
        moves.refresh_room(|_| Some(u64::MAX));
        assert!(matches!(
            moves.view(),
            Some(MoveView::Offered { room: None, .. })
        ));
        // No offer is waiting: moving agrees for the phone's next try.
        let (taken, unpair, _) = moves.decide(MoveChoice::Move, |_| Some(u64::MAX));
        assert!(taken && unpair.is_none());
        assert_eq!(moves.agreed("a"), Some((None, false)));
    }

    #[test]
    fn not_now_with_no_offer_waiting_unpairs_the_phone() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let mut m = Move::new(&phone("a"), dir.path(), counts(1), 1000.0);
        m.phase = Phase::Offered(None);
        moves.claim(m).unwrap();
        let (taken, unpair, view) = moves.decide(MoveChoice::NotNow, |_| None);
        assert!(taken);
        assert_eq!(unpair.as_deref(), Some("a"));
        assert_eq!(view, None);
    }

    #[test]
    fn stopping_here_ends_the_move_and_its_phone_hears_so() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        // Agreed, nothing sent yet: the phone hears it once, when it sends.
        agreed(&moves, dir.path(), "a");
        if let Some(m) = lock(&moves.slot).as_mut() {
            m.agreed_again();
        }
        assert_eq!(moves.stop().as_deref(), Some("a"));
        assert_eq!(moves.view(), None);
        assert!(moves.was_stopped("a"));
        assert!(!moves.was_stopped("a"));

        // Being checked: what it stages goes, and the phone hears it was stopped.
        let owner = agreed(&moves, dir.path(), "a");
        moves.begin_check(&owner, 1).unwrap();
        assert_eq!(moves.stop().as_deref(), Some("a"));
        assert!(
            moves
                .claim(Move::new(&phone("b"), dir.path(), counts(1), 1.0))
                .is_none()
        );
        stage_as(dir.path(), &owner, "a");
        let (_, answer) = moves.conclude(dir.path(), &owner, false, Ok(counts(3)));
        assert_eq!(answer, err(PhoneError::Paused));
        assert!(!staged(dir.path()));

        // Nothing to stop once it waits for the restart.
        let owner = agreed(&moves, dir.path(), "a");
        moves.begin_check(&owner, 1).unwrap();
        moves.conclude(dir.path(), &owner, false, Ok(counts(3)));
        assert_eq!(moves.stop(), None);
    }

    #[test]
    fn room_running_out_is_said_and_holds_no_place() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        moves.begin_check(&owner, 1).unwrap();
        let (view, answer) = moves.conclude(dir.path(), &owner, false, Err(PhoneError::TooLarge));
        assert_eq!(answer, err(PhoneError::TooLarge));
        assert!(matches!(
            view,
            Some(MoveView::Failed {
                why: FailedWhy::NoRoom,
                room: Some(_),
                ..
            })
        ));
        assert!(!moves.holding());
        assert!(
            moves
                .claim(Move::new(&phone("b"), dir.path(), counts(1), 1.0))
                .is_some()
        );
        assert!(!moves.dismiss(), "the new offer took its place");
    }
}
