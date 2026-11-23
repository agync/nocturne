//! A phone that holds its own memory moves it here, and its things join the memory here with no
//! restart (`docs/design/moving-memory.md`, the owner's decision 31). A whole memory never takes
//! the place of the one here in a move; that stays only in restoring a backup file.
//!
//! The phone offers it (`offerMemory`) with what it holds. A computer that holds nothing agrees
//! at once, with nothing asked. One that holds a memory asks the person how the phone joins it:
//! "Bring the phone's things in" agrees; "Use this computer's memory" brings nothing, and the
//! phone hears `UseComputerMemory` and stays paired; "Not now" undoes the pairing. A phone that
//! offers with `bring` (the person chose to bring in what a move put aside on it) is not asked
//! about either. Once agreed, the phone sends a backup in 0013's format, locked with a passphrase
//! made for this one move, as the chunks of `memoryDone`. It is checked, unpacked beside the
//! store in use and its things are added to it in one step (`Engine::bring_in`); the phone hears
//! `MemoryBrought`. Stopping is possible until the adding begins. How each phone joined is noted
//! (`joined.json`), so a phone that missed the answer hears it again.
//!
//! What a phone hears, besides the answers named in the contract:
//!
//! | Answer | To | Means |
//! |---|---|---|
//! | `TooLarge` | `offerMemory` | not enough room here yet; the person frees some, the phone tries again |
//! | `TooLarge` | `memoryDone` | room ran out while it was written out; nothing changed |
//! | `Busy` | either | wait and ask again: another phone's move, or a backup or restore, is under way |
//! | `MemoryBrought` | either | the phone's things were added to the memory here |
//! | `UseComputerMemory` | `offerMemory` | the person here chose this computer's memory |
//! | `NotReady` | `memoryDone` | nothing is known of the phone's memory here; it offers it again |
//! | `Paused` | `memoryDone` | the person here stopped the move; nothing changed |
//! | `Revoked` | either | the person kept what is here, or removed the phone |
//!
//! A `memoryDone` of size 0 sends nothing: it asks where the phone's memory stands, and waits
//! while its things are being added.
//!
//! One move at a time. Nothing here keeps the passphrase: it goes into the unpack and is gone.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use nocturne_contract::{BroughtCounts, MemoryCounts, PhoneError, RemoteAnswer};
use nocturne_engine::{DeviceRow, Engine, MIN_PASSPHRASE_CHARS};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::{Notify, oneshot};

use super::incoming::UploadDone;
use super::pairing::ConvId;
use super::protocol::MAX_MEMORY_BYTES;
use super::session::{Core, ReplySink};
use crate::restore;
use crate::state::AppState;

/// Sent with the move as it stands (`MoveView`), or null once there is none.
pub const EVENT_MOVE: &str = "move-changed";

/// How long an offer waits for the person on this computer.
const DECIDE_WAIT: Duration = Duration::from_secs(600);
/// How long an agreed move waits for its phone to begin sending.
const AGREED_WAIT: Duration = Duration::from_secs(1800);
/// How often a move waiting for its upload looks whether the session carrying it is still there.
const LOOK_EVERY: Duration = Duration::from_secs(2);
/// Arriving progress reaches the window at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(200);
/// The longest passphrase a phone may send.
const MAX_PASSPHRASE: usize = 256;
/// A session carrying a memory is not cut at the usual fifteen minutes while the memory is on its
/// way or being checked: a large memory takes longer than that to go across.
pub const MEMORY_SESSION_SECS: u64 = 6 * 60 * 60;
/// Where the phones that joined the memory here are remembered, so a phone that missed the
/// answer is told again.
const JOINED: &str = "joined.json";
/// The phones `JOINED` remembers, newest last.
const JOINED_KEPT: usize = 16;
/// Room a move needs besides three times its size, for the store's own growth as things are
/// added.
const ROOM_MARGIN: u64 = 64 * 1024 * 1024;
/// How many times the phone's estimate of its backup bringing things in needs here at most: the
/// upload, the backup unpacked beside the store in use, and the originals copied from it into the
/// store, all here at once until the adding ends and the first two go.
const ROOM_TIMES: u64 = 3;

/// The move as the window shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "phase",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MoveView {
    /// "{name} has its own memory, and this computer has one too." `since`: the first day
    /// something was kept on the phone. `room`: not enough free room yet to bring its things in,
    /// and Continue waits while that is chosen.
    Offered {
        name: String,
        items: u32,
        bytes: f64,
        since: Option<String>,
        room: Option<Room>,
    },
    /// "{name} is getting its things ready…": agreed, and nothing has arrived yet while the phone
    /// makes the copy that travels.
    Preparing {
        name: String,
    },
    Arriving {
        name: String,
        done: f64,
        size: f64,
    },
    /// "Adding them to the memory here…": arrived whole. It can no longer be stopped once the
    /// adding begins.
    Adding {
        name: String,
    },
    /// "{name}'s things are here." with what was added. `removed`: the phone was removed while
    /// they were added, so it still holds its own memory.
    Brought {
        name: String,
        added: BroughtCounts,
        removed: bool,
    },
    /// A bring-in that ended with nothing added: the phone was removed here while its things
    /// arrived, or the person stopped it on the phone.
    Ended {
        name: String,
        why: EndedWhy,
    },
    /// "{name} now works with this computer.": the person chose this computer's memory.
    Used {
        name: String,
    },
    /// The move ended and nothing changed: room ran out. `room` says how much it needs.
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

/// What a stop from the phone did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopping {
    /// Nothing will be added, and the person here hears so.
    Stopped,
    /// The things are being added already: they will be.
    TooLate,
    /// No bring-in of this phone's was under way.
    Nothing,
}

/// Why a bring-in ended with nothing added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EndedWhy {
    /// "{name} was removed, so nothing came in from it."
    Removed,
    /// "{name} stopped bringing its things in. Nothing changed here."
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FailedWhy {
    NoRoom,
}

/// The person's answer to an offer, which only a computer that holds a memory asks: bring the
/// phone's things in, use this computer's memory, or not now. Not now keeps what is here and
/// undoes the pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MoveChoice {
    NotNow,
    Bring,
    UseHere,
}

#[derive(Debug)]
enum Phase {
    Offered(Option<oneshot::Sender<MoveChoice>>),
    Accepted,
    Arriving {
        done: u64,
        size: u64,
    },
    /// Arrived whole, and being checked and unpacked.
    Checking,
    /// Its things are being added; nothing stops it now.
    Adding,
    /// Ended with nothing changed; shown until the person says OK. It holds no place.
    Failed(FailedWhy),
    /// Its things were added; shown until the person opens today's page. It holds no place.
    Brought(BroughtCounts),
    /// The person chose this computer's memory; shown until they say OK. It holds no place.
    Used,
    /// A bring-in ended with nothing added; shown until the person says OK. It holds no place.
    Ended(EndedWhy),
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
    /// The first day something was kept on the phone, as it said.
    since: Option<String>,
    /// Things kept, as the phone counts them on its own "What moves".
    things: Option<u32>,
    phase: Phase,
    /// The data folder it unpacks in.
    data: PathBuf,
    /// Its phone was removed, or it was stopped, while the memory was being checked or its
    /// things added: it stays, unseen, until that ends, so no other move starts under it.
    gone: bool,
    /// The person stopped it here: its phone hears so.
    stopped: bool,
    /// A bring-in that ends with nothing added, and why, once its check lets go.
    ended: Option<EndedWhy>,
    /// Its phone was removed while its things were added.
    removed: bool,
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
            since: None,
            things: None,
            phase: Phase::Accepted,
            data: data.to_path_buf(),
            gone: false,
            stopped: false,
            ended: None,
            removed: false,
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
                items: self.things.unwrap_or(self.offer.items),
                bytes: self.bytes,
                since: self.since.clone(),
                room: self.room,
            },
            Phase::Accepted | Phase::Arriving { done: 0, .. } => MoveView::Preparing { name },
            Phase::Arriving { done, size } => MoveView::Arriving {
                name,
                done: *done as f64,
                size: *size as f64,
            },
            Phase::Checking | Phase::Adding => MoveView::Adding { name },
            Phase::Brought(added) => MoveView::Brought {
                name,
                added: *added,
                removed: self.removed,
            },
            Phase::Used => MoveView::Used { name },
            Phase::Ended(why) => MoveView::Ended { name, why: *why },
            Phase::Failed(why) => MoveView::Failed {
                name,
                why: *why,
                room: Some(Room {
                    need: room_needed(self.bytes as u64) as f64,
                    free: 0.0,
                }),
            },
        }
    }
}

/// The one move under way, if any. Every change of phase is one method here, under one lock:
///
/// ```text
/// (none) --claim(agreed)--------------------> Accepted <--upload failed-- Arriving
/// (none) --claim--> Offered --decide(bring)--> Accepted --------carry----------^
///                      |--decide(use here)--> Used
///                      +--decide(not now)--> (none)
///                                Accepted/Arriving --begin_check--> Checking
///                                Checking --begin_adding--> Adding --conclude(ok)--> Brought
///                                Checking/Adding --conclude(failed)--> (none), or Failed
/// forget or phone_stopped: Offered --> (none); Accepted/Arriving --> Ended; Checking --> gone
/// (a tombstone the check ends), and its conclude --> Ended. Adding is never stopped; forget
/// marks it gone, and its conclude still says Brought, with the phone removed. Brought, Used,
/// Failed and Ended hold no place.
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

/// What a checked memory held, and what of it was added here.
type Outcome = (MemoryCounts, BroughtCounts);

/// What an answer to the question did.
#[derive(Debug)]
struct Decided {
    taken: bool,
    unpair: Option<String>,
    /// The phone, and its move, that now uses this computer's memory.
    used: Option<(String, Owner)>,
    view: Option<MoveView>,
}

impl Decided {
    fn not_taken(view: Option<MoveView>) -> Decided {
        Decided {
            taken: false,
            unpair: None,
            used: None,
            view,
        }
    }
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

    /// The phone's things were added: its upload is not taken again.
    pub fn arrived_for(&self, device: &str) -> bool {
        lock(&self.slot)
            .as_ref()
            .is_some_and(|m| m.device == device && !m.gone && matches!(m.phase, Phase::Brought(_)))
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
    fn agreed(&self, device: &str) -> Option<(RemoteAnswer, bool)> {
        let mut slot = lock(&self.slot);
        expire_idle(&mut slot);
        let m = slot.as_mut().filter(|m| m.device == device && !m.gone)?;
        let dropped = matches!(m.phase, Phase::Arriving { .. });
        if dropped {
            m.agreed_again();
        }
        let accepted = RemoteAnswer::MemoryAccepted {
            here: m.here,
            bring: true,
        };
        matches!(m.phase, Phase::Accepted).then_some((accepted, dropped))
    }

    /// The bytes of the memory so far; answers the view when it is due to be shown.
    pub fn arriving(&self, device: &str, done: u64, size: u64) -> Option<MoveView> {
        let mut slot = lock(&self.slot);
        let m = slot.as_mut().filter(|m| m.device == device && !m.gone)?;
        if !matches!(m.phase, Phase::Accepted | Phase::Arriving { .. }) {
            return None;
        }
        let before = match m.phase {
            Phase::Arriving { done, .. } => done,
            _ => 0,
        };
        if tenths(before, size) != tenths(done, size) {
            tracing::info!(
                device = %m.device,
                "a phone's memory is arriving: {done} of {size} bytes"
            );
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
        if slot.as_ref().is_some_and(|m| {
            matches!(
                m.phase,
                Phase::Failed(_) | Phase::Brought(_) | Phase::Used | Phase::Ended(_)
            )
        }) {
            *slot = None;
        }
        let busy = slot.as_ref().is_some_and(|m| {
            m.device != new.device
                || m.gone
                || matches!(
                    m.phase,
                    Phase::Arriving { .. } | Phase::Checking | Phase::Adding
                )
        });
        if busy {
            return None;
        }
        *slot = Some(new);
        Some(shown(&slot))
    }

    /// The person's answer to the question on screen. `free` is the room here now. Answers
    /// whether it was taken, the phone to unpair when the answer was "not now" and no offer was
    /// waiting to carry it (the phone tried, heard there was no room, and waits to try again),
    /// and the move whose phone now uses this computer's memory. Bringing things in waits while
    /// there is not room.
    fn decide(&self, choice: MoveChoice, free: impl Fn(&Path) -> Option<u64>) -> Decided {
        let mut slot = lock(&self.slot);
        let Some(m) = slot.as_mut().filter(|m| !m.gone) else {
            return Decided::not_taken(shown(&slot));
        };
        let Phase::Offered(tx) = &mut m.phase else {
            return Decided::not_taken(shown(&slot));
        };
        if choice == MoveChoice::Bring
            && let Some(room) = short(m.bytes as u64, free(&m.data))
        {
            // Room went since the question showed: it says so now, and bringing in waits.
            m.room = Some(room);
            return Decided::not_taken(shown(&slot));
        }
        let waiting = tx.take();
        let carried = waiting.is_some_and(|tx| tx.send(choice).is_ok());
        let unpair = (!carried && choice == MoveChoice::NotNow).then(|| m.device.clone());
        let mut used = None;
        match choice {
            MoveChoice::Bring => {
                m.room = None;
                m.agreed_again();
            }
            MoveChoice::UseHere => {
                m.room = None;
                m.phase = Phase::Used;
                used = Some((m.device.clone(), m.owner.clone()));
            }
            MoveChoice::NotNow => *slot = None,
        }
        Decided {
            taken: true,
            unpair,
            used,
            view: shown(&slot),
        }
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

    /// "Stop" on this computer, until the adding begins. Answers the phone whose upload, if one
    /// is coming, ends now with `Paused`.
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
            // The check lets go of what it unpacked once it ends, and its phone hears why.
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

    /// "OK" on a move that ended with nothing changed or used this computer's memory, or "Open
    /// today's page" once a phone's things were added.
    fn dismiss(&self) -> bool {
        let mut slot = lock(&self.slot);
        if slot.as_ref().is_some_and(|m| {
            matches!(
                m.phase,
                Phase::Failed(_) | Phase::Brought(_) | Phase::Used | Phase::Ended(_)
            )
        }) {
            *slot = None;
            return true;
        }
        false
    }

    /// The person stopped bringing this phone's things in, on the phone: until they are being
    /// added, nothing is added and the person here hears so.
    pub fn phone_stopped(&self, device: &str) -> Stopping {
        let mut slot = lock(&self.slot);
        let Some(m) = slot.as_mut().filter(|m| m.device == device && !m.gone) else {
            return Stopping::Nothing;
        };
        match m.phase {
            Phase::Accepted | Phase::Arriving { .. } => {
                m.phase = Phase::Ended(EndedWhy::Stopped);
                m.conv = None;
            }
            // The check lets go of what it unpacked once it ends, and adds nothing.
            Phase::Checking => {
                m.gone = true;
                m.ended = Some(EndedWhy::Stopped);
            }
            Phase::Adding => return Stopping::TooLate,
            _ => return Stopping::Nothing,
        }
        drop(slot);
        self.settled.notify_waiters();
        Stopping::Stopped
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
    /// nothing may be unpacked for it.
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

    /// A checked memory's things are about to be added: from here nothing stops it. False
    /// when its phone went or the person stopped it meanwhile, and then nothing may be added.
    fn begin_adding(&self, owner: &str) -> bool {
        let mut slot = lock(&self.slot);
        let Some(m) = slot
            .as_mut()
            .filter(|m| m.owner == owner && !m.gone && matches!(m.phase, Phase::Checking))
        else {
            return false;
        };
        m.phase = Phase::Adding;
        true
    }

    /// How a check ends: its things added, or nothing changed. Only this move's owner is looked
    /// at, so a check that ends late never touches another phone's move.
    fn conclude(
        &self,
        owner: &str,
        removed: bool,
        checked: Result<Outcome, PhoneError>,
    ) -> (Option<MoveView>, RemoteAnswer) {
        let mut slot = lock(&self.slot);
        let ours = slot
            .as_mut()
            .filter(|m| m.owner == owner && matches!(m.phase, Phase::Checking | Phase::Adding));
        match (checked, ours) {
            // Added. When its phone was removed meanwhile, adding finished all the same, since
            // everything had arrived whole, and the person here hears so.
            (Ok((counts, added)), Some(m)) => {
                m.removed = m.gone || removed;
                m.phase = Phase::Brought(added);
                m.conv = None;
                m.gone = false;
                (shown(&slot), RemoteAnswer::MemoryBrought { counts, added })
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
                let stopped = ours.as_ref().is_some_and(|m| m.stopped);
                match ours {
                    // Nothing added says why until the person says OK.
                    Some(m) if m.ended.is_some() => {
                        m.phase = Phase::Ended(m.ended.unwrap_or(EndedWhy::Removed));
                        m.conv = None;
                        m.gone = false;
                    }
                    Some(_) => *slot = None,
                    None => {}
                }
                let answer = match checked {
                    _ if stopped => err(PhoneError::Paused),
                    Ok(_) => err(PhoneError::Revoked),
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
            // A bring-in the phone stopped says so, though its pairing goes with it.
            Phase::Ended(_) => return false,
            // Its things arriving: nothing is added, and the person here hears why.
            Phase::Accepted | Phase::Arriving { .. } => {
                m.phase = Phase::Ended(EndedWhy::Removed);
                m.conv = None;
            }
            Phase::Checking => {
                m.gone = true;
                m.ended = Some(EndedWhy::Removed);
            }
            Phase::Adding => m.gone = true,
            _ => *slot = None,
        }
        drop(slot);
        self.settled.notify_waiters();
        true
    }

    /// This phone's memory is being checked or its things added: not an answer yet.
    fn under_way(&self, device: &str) -> bool {
        lock(&self.slot).as_ref().is_some_and(|m| {
            m.device == device && matches!(m.phase, Phase::Checking | Phase::Adding)
        })
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

/// An answer to a phone, for the log: never more than its kind, its code or its counts.
fn said(answer: &RemoteAnswer) -> String {
    match answer {
        RemoteAnswer::MemoryAccepted { .. } => "agreed to bring its things in".into(),
        RemoteAnswer::MemoryBrought { added, .. } => format!(
            "added {} things, {} journal entries, {} check-ins",
            added.things, added.entries, added.checkins
        ),
        RemoteAnswer::UseComputerMemory => "use this computer's memory".into(),
        RemoteAnswer::Error { code } => format!("{code:?}"),
        _ => "another answer".into(),
    }
}

/// Whole tenths of `size` that `done` makes, for a progress line each tenth of the way.
fn tenths(done: u64, size: u64) -> u64 {
    done.saturating_mul(10) / size.max(1)
}

/// The room bringing in a memory of `bytes` (the phone's estimate of its backup) needs here.
fn room_needed(bytes: u64) -> u64 {
    bytes.saturating_mul(ROOM_TIMES).saturating_add(ROOM_MARGIN)
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

/// This store holds something of the person's: what decides whether a phone that offers its
/// memory is asked how it joins this one.
pub(crate) fn holds_memory(e: &Engine) -> bool {
    held(e).is_ok_and(|h| h.is_some())
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

/// A phone that joined the memory here, remembered so it hears how again if it missed it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Joined {
    device: String,
    owner: Owner,
    how: How,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum How {
    /// Its things were added.
    Brought {
        counts: MemoryCounts,
        added: BroughtCounts,
    },
    /// The person here chose this computer's memory.
    Used,
}

fn joined_path(data: &Path) -> PathBuf {
    data.join("backup").join(JOINED)
}

fn read_joined(data: &Path) -> Vec<Joined> {
    std::fs::read(joined_path(data))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Note how a phone joined, in place of anything noted of it before.
fn note_joined(moves: &Moves, data: &Path, joined: Joined) -> std::io::Result<()> {
    let _one = lock(&moves.note);
    let mut all = read_joined(data);
    all.retain(|j| j.device != joined.device);
    all.push(joined);
    let over = all.len().saturating_sub(JOINED_KEPT);
    all.drain(..over);
    let path = joined_path(data);
    if let Some(dir) = path.parent() {
        crate::fsutil::create_private_dir(dir)?;
    }
    let bytes = serde_json::to_vec(&all).map_err(std::io::Error::other)?;
    crate::fsutil::write_private(&path, &bytes)
}

/// What a phone asked when it is answered from how it joined: where its memory stands, or an
/// offer of a memory holding `counts`.
#[derive(Debug, Clone, Copy)]
enum Asked {
    Status,
    Offer { counts: MemoryCounts, bring: bool },
}

/// How this phone joined the memory here, as its answer: that its things were added, to an ask
/// or to an offer of the same memory (one that changed since is brought in again, and only what
/// is new is added); to an offer that does not bring things in, that it uses this computer's
/// memory.
fn joined_here(data: &Path, device: &str, asked: Asked) -> Option<RemoteAnswer> {
    let how = read_joined(data)
        .into_iter()
        .find(|j| j.device == device)?
        .how;
    match (how, asked) {
        (How::Brought { counts, added }, Asked::Status) => {
            Some(RemoteAnswer::MemoryBrought { counts, added })
        }
        (
            How::Brought { counts, added },
            Asked::Offer {
                counts: offered, ..
            },
        ) if offered == counts => Some(RemoteAnswer::MemoryBrought { counts, added }),
        (How::Used, Asked::Offer { bring: false, .. }) => Some(RemoteAnswer::UseComputerMemory),
        _ => None,
    }
}

fn whole(n: f64) -> bool {
    n.is_finite() && n >= 1.0 && n.fract() == 0.0
}

fn is_digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// What a phone said with `offerMemory`.
pub struct Offer {
    pub counts: MemoryCounts,
    pub bytes: f64,
    pub schema: u32,
    pub since: Option<String>,
    pub things: Option<u32>,
    pub bring: bool,
}

/// The longest first day a phone may send: a `YYYY-MM-DD` date.
const MAX_SINCE: usize = 10;

/// `offerMemory`: agree at once when this computer holds nothing or the phone brings in what a
/// move put aside on it; otherwise wait for the person on this computer, then agree, say this
/// computer's memory is used, or undo the pairing. A phone that asks again after it joined the
/// memory here is told so.
pub async fn offer<R: Runtime>(
    app: &AppHandle<R>,
    core: &Arc<Core>,
    out: &ReplySink,
    device: &DeviceRow,
    offered: Offer,
) -> Option<RemoteAnswer> {
    let Offer {
        counts,
        bytes,
        schema,
        since,
        things,
        bring,
    } = offered;
    let id = device.id.as_str();
    tracing::info!(
        device = id,
        "a phone offers its memory: {} items, about {bytes} bytes, store version {schema}",
        counts.items
    );
    let since = since
        .filter(|d| d.len() <= MAX_SINCE && jiff::civil::Date::strptime("%Y-%m-%d", d).is_ok());
    if !whole(bytes) || schema == 0 {
        tracing::info!(device = id, "the offer was not whole");
        return Some(err(PhoneError::Invalid));
    }
    if bytes > MAX_MEMORY_BYTES as f64 {
        tracing::info!(
            device = id,
            "the offered memory is larger than a move takes"
        );
        return Some(err(PhoneError::TooLarge));
    }
    if schema > nocturne_engine::store::latest_version() {
        tracing::info!(
            device = id,
            "the offered memory is newer than this computer"
        );
        return Some(err(PhoneError::ComputerOutdated));
    }
    let Some(state) = app.try_state::<AppState>() else {
        return Some(err(PhoneError::NotReady));
    };
    let data = state.data_dir.clone();
    if let Some(answer) = already(core, &device.id, &data, Asked::Offer { counts, bring }).await {
        tracing::info!(
            device = id,
            "the offer is answered from a move before: {}",
            said(&answer)
        );
        return Some(answer);
    }
    let Ok(e) = state.engine() else {
        return Some(err(PhoneError::NotReady));
    };
    let Ok(here) = crate::error::blocking(move || held(&e)).await else {
        return Some(err(PhoneError::Busy));
    };
    // Agreed to already: a phone carrying on after a pause is not asked about twice.
    if let Some((agreed, dropped)) = core.moves.agreed(&device.id) {
        if dropped {
            core.incoming.cancel(&device.id);
        }
        tracing::info!(
            device = id,
            dropped,
            "the move was agreed before: the phone carries on"
        );
        return Some(agreed);
    }
    // A restore that waits for the next start is not replaced by a move.
    if restore::marker_path(&data).exists() {
        tracing::info!(
            device = id,
            "a restore waits for the next start: the phone waits"
        );
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
    new.since = since;
    new.things = things.map(|n| n.min(counts.items));
    new.room = room;
    // The person chose on the phone to bring in what a move put aside there, or this computer
    // holds nothing to choose about: nothing is asked here, the phone hears at once, and its
    // things are added with no restart.
    if bring || here.is_none() {
        if let Some(room) = room {
            tracing::info!(
                device = id,
                "not enough room to bring the phone's things in: {} bytes free of {} needed",
                room.free,
                room.need
            );
            return Some(err(PhoneError::TooLarge));
        }
        new.room = None;
        new.agreed_again();
        let Some(view) = core.moves.claim(new) else {
            tracing::info!(device = id, "another move holds the place: the phone waits");
            return Some(err(PhoneError::Busy));
        };
        emit(app, view);
        if !bring {
            crate::window::show_main(app);
        }
        tracing::info!(
            device = id,
            empty = !bring,
            "the phone brings its things in"
        );
        return Some(RemoteAnswer::MemoryAccepted { here, bring: true });
    }
    // This computer holds a memory: the person here chooses how the phone joins it, and the
    // phone waits, short of room or not, since using this memory needs no room.
    new.phase = Phase::Offered(Some(tx));
    let owner = new.owner.clone();
    let Some(view) = core.moves.claim(new) else {
        tracing::info!(device = id, "another move holds the place: the phone waits");
        return Some(err(PhoneError::Busy));
    };
    emit(app, view);
    crate::window::show_main(app);
    tracing::info!(device = id, "the person here is asked how the phone joins");
    let choice = tokio::select! {
        choice = rx => Some(choice),
        () = out.closed() => None,
        () = tokio::time::sleep(DECIDE_WAIT) => None,
    };
    match choice {
        Some(Ok(MoveChoice::Bring)) => {
            tracing::info!(
                device = id,
                "the person here brings the phone's things in: the phone makes its backup"
            );
            Some(RemoteAnswer::MemoryAccepted { here, bring: true })
        }
        Some(Ok(MoveChoice::UseHere)) => {
            tracing::info!(
                device = id,
                "the person here chose this computer's memory: nothing comes in"
            );
            Some(RemoteAnswer::UseComputerMemory)
        }
        Some(Ok(MoveChoice::NotNow)) => {
            tracing::info!(
                device = id,
                "the person kept what is here: the pairing is undone"
            );
            // Keep what is here: the phone stays as it was, alone and unpaired. The pairing goes
            // first, then the phone is told, then its sessions end.
            revoke(app, &device.id).await;
            out.error(PhoneError::Revoked);
            core.drop_device(&device.id);
            core.shell.changed();
            None
        }
        // A newer offer from the same phone took this one's place.
        Some(Err(_)) => {
            tracing::info!(
                device = id,
                "a newer offer from the phone took this one's place"
            );
            Some(err(PhoneError::Busy))
        }
        // Nobody answered, or the phone went.
        None => {
            tracing::info!(device = id, "nobody answered the offer, or the phone went");
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

/// A memory being checked or added is waited for; then a phone that joined the memory here
/// hears how.
async fn already(
    core: &Arc<Core>,
    device: &str,
    data: &Path,
    asked: Asked,
) -> Option<RemoteAnswer> {
    settled(core, device).await;
    joined_here(data, device, asked)
}

/// Where this phone's memory stands, for a `memoryDone` that sends nothing.
async fn status(core: &Arc<Core>, device: &str, data: &Path) -> RemoteAnswer {
    already(core, device, data, Asked::Status)
        .await
        .unwrap_or(err(PhoneError::NotReady))
}

/// Wait while this phone's memory is being checked or its things added.
async fn settled(core: &Arc<Core>, device: &str) {
    loop {
        let settled = core.moves.settled.notified();
        if !core.moves.under_way(device) {
            return;
        }
        settled.await;
    }
}

/// The person's answer on this computer. False when there was nothing to answer, or no room yet
/// to bring the phone's things in.
pub async fn decide<R: Runtime>(app: &AppHandle<R>, core: &Core, choice: MoveChoice) -> bool {
    let decided = core.moves.decide(choice, free_here);
    if let Some((device, owner)) = decided.used
        && let Some(state) = app.try_state::<AppState>()
    {
        // Noted before the phone can ask again, so a phone that missed the answer hears it.
        let joined = Joined {
            device,
            owner,
            how: How::Used,
        };
        if let Err(e) = note_joined(&core.moves, &state.data_dir, joined) {
            tracing::warn!("could not note that a phone uses this computer's memory: {e}");
        }
    }
    emit(app, decided.view);
    if let Some(device) = decided.unpair {
        unpair(app, core, &device).await;
    }
    decided.taken
}

/// "Stop" on this computer: the phone's upload ends, and the phone hears the move was
/// stopped. False when there is nothing to stop.
pub fn stop<R: Runtime>(app: &AppHandle<R>, core: &Core) -> bool {
    let Some(device) = core.moves.stop() else {
        return false;
    };
    tracing::info!(device = %device, "the person here stopped the move");
    core.incoming.fail(&device, PhoneError::Paused);
    emit(app, core.moves.view());
    true
}

/// The person stopped bringing the phone's things in, on the phone (`stopMemory`, or the
/// pairing it undid for it): nothing is added, and the person here hears so.
pub fn phone_stopped<R: Runtime>(app: &AppHandle<R>, core: &Core, device: &str) -> Stopping {
    let stopping = core.moves.phone_stopped(device);
    if stopping == Stopping::Stopped {
        tracing::info!(device, "the phone stopped bringing its things in");
        core.incoming.cancel(device);
        emit(app, core.moves.view());
    }
    stopping
}

/// What the phone sent with `memoryDone`.
pub struct Done {
    pub conv: ConvId,
    pub call: u32,
    pub size: f64,
    pub sha256: String,
    pub passphrase: String,
}

/// `memoryDone`: take the upload, check it, add its things here, and answer what was added.
pub async fn done<R: Runtime>(
    app: &AppHandle<R>,
    core: &Arc<Core>,
    device: &DeviceRow,
    upload: Option<UploadDone>,
    sent: Done,
) -> Option<RemoteAnswer> {
    let Some(state) = app.try_state::<AppState>() else {
        return Some(err(PhoneError::NotReady));
    };
    let data = state.data_dir.clone();
    let (conv, call) = (sent.conv, sent.call);
    let id = device.id.as_str();
    if core.moves.was_stopped(&device.id) {
        tracing::info!(
            device = id,
            "the phone hears that the person here stopped the move"
        );
        core.incoming.cancel_call(&device.id, conv, call);
        return Some(err(PhoneError::Paused));
    }
    let Some(upload) = upload else {
        let answer = status(core, &device.id, &data).await;
        // A finishing phone asks every few seconds: these stay out of the usual log.
        tracing::debug!(
            device = id,
            "the phone asks where its memory stands: {}",
            said(&answer)
        );
        return Some(answer);
    };
    if !whole(sent.size)
        || !is_digest(&sent.sha256)
        || sent.passphrase.chars().count() < MIN_PASSPHRASE_CHARS
        || sent.passphrase.len() > MAX_PASSPHRASE
    {
        tracing::info!(device = id, "the memory's call was not whole: refused");
        core.incoming.cancel_call(&device.id, conv, call);
        return Some(err(PhoneError::Invalid));
    }
    let Some((owner, view)) = core.moves.carry(&device.id, conv) else {
        tracing::info!(device = id, "a memory came with no move agreed: refused");
        core.incoming.cancel_call(&device.id, conv, call);
        return Some(err(PhoneError::NotReady));
    };
    tracing::info!(
        device = id,
        "the phone's memory begins to arrive: {} bytes",
        sent.size
    );
    emit(app, view);
    let file = match wait_upload(core, &device.id, conv, call, upload).await {
        Ok(file) => file,
        Err(PhoneError::TooLarge) => {
            tracing::info!(device = id, "room ran out while the memory arrived");
            emit(app, core.moves.upload_no_room(&owner, conv));
            return Some(err(PhoneError::TooLarge));
        }
        Err(code) => {
            emit(app, core.moves.upload_failed(&owner, conv));
            let code = match core.moves.stopped(&owner) {
                true => PhoneError::Paused,
                false => code,
            };
            tracing::info!(device = id, "the memory stopped arriving: {code:?}");
            return Some(err(code));
        }
    };
    tracing::info!(device = id, "the memory arrived whole: it is checked now");
    // Its phone may have gone, or the person stopped it, while the memory came: then nothing is
    // unpacked for it.
    let Some(view) = core.moves.begin_check(&owner, conv) else {
        tracing::info!(
            device = id,
            "the phone went, or the move was stopped, as it arrived"
        );
        let _ = std::fs::remove_file(&file);
        return Some(err(match core.moves.stopped(&owner) {
            true => PhoneError::Paused,
            false => PhoneError::Revoked,
        }));
    };
    emit(app, view);

    let checked = check(&state, core, device, &owner, &file, &sent).await;
    let _ = std::fs::remove_file(&file);

    let (view, answer) = core
        .moves
        .conclude(&owner, core.removed(&device.id), checked);
    core.moves.settled.notify_waiters();
    emit(app, view);
    tracing::info!(device = id, "the memory was checked: {}", said(&answer));
    if matches!(answer, RemoteAnswer::MemoryBrought { .. }) {
        // What came in waits to be read as new.
        state.wake_pipeline();
    }
    Some(answer)
}

/// Check the memory and add its things to the memory here, once any backup or restore that is
/// running is done; never over a restore that waits for the next start, which would put aside
/// the store they were added to.
async fn check(
    state: &AppState,
    core: &Arc<Core>,
    device: &DeviceRow,
    owner: &str,
    file: &Path,
    sent: &Done,
) -> Result<Outcome, PhoneError> {
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
    let out = if restore::marker_path(&state.data_dir).exists() {
        Err(PhoneError::Busy)
    } else {
        bring_in(state, core, device, owner, file, sent).await
    };
    state.end_backup();
    out
}

/// Check the memory against what the phone said, unpack it beside the store in use and add its
/// things there in one step, unless its phone went or the person stopped it first. What was
/// unpacked goes either way.
async fn bring_in(
    state: &AppState,
    core: &Arc<Core>,
    device: &DeviceRow,
    owner: &str,
    file: &Path,
    sent: &Done,
) -> Result<Outcome, PhoneError> {
    let data = state.data_dir.clone();
    // Named to be let go: a stage a crash leaves behind is shredded at the next start.
    let stage = restore::restore_dir(&data).join(format!("{}bring-{owner}", restore::LET_GO));
    let unpacked = {
        let (file, stage, size, sha256, passphrase) = (
            file.to_path_buf(),
            stage.clone(),
            sent.size,
            sent.sha256.clone(),
            sent.passphrase.clone(),
        );
        let restore = restore::restore_dir(&data);
        tokio::task::spawn_blocking(move || {
            unpack(&file, &restore, &stage, size, &sha256, &passphrase)
        })
        .await
        .unwrap_or(Err(PhoneError::Busy))
    };
    let let_go = |stage: PathBuf| {
        std::thread::spawn(move || crate::fsutil::shred_tree(&stage));
    };
    let counts = match unpacked {
        Ok(counts) => counts,
        Err(code) => {
            let_go(stage);
            return Err(code);
        }
    };
    if !core.moves.begin_adding(owner) {
        tracing::info!(device = %device.id, "the phone went, or the person stopped it, before its things were added");
        let_go(stage);
        return Err(PhoneError::Revoked);
    }
    tracing::info!(device = %device.id, "the phone's things are being added");
    let added = match state.engine() {
        Ok(e) => {
            let (from, paired) = (stage.join("store"), device.id.clone());
            crate::error::blocking(move || Ok(e.bring_in(&from, &paired)))
                .await
                .unwrap_or_else(|_| Err(nocturne_engine::Error::Busy))
        }
        Err(_) => Err(nocturne_engine::Error::Busy),
    };
    let_go(stage);
    let added = added.map_err(|e| match e {
        // The pairing went as the adding began: nothing was added.
        nocturne_engine::Error::NotFound(_) if core.removed(&device.id) => PhoneError::Revoked,
        nocturne_engine::Error::NotFound(_) => PhoneError::Invalid,
        e => transient(&e),
    })?;
    let added = BroughtCounts {
        things: added.things,
        entries: added.entries,
        checkins: added.checkins,
    };
    let joined = Joined {
        device: device.id.clone(),
        owner: owner.to_string(),
        how: How::Brought { counts, added },
    };
    if let Err(e) = note_joined(&core.moves, &data, joined) {
        tracing::warn!("could not note that a phone's things were added: {e}");
    }
    Ok((counts, added))
}

/// Check the file against what the phone said and unpack it into `stage`, a new folder in
/// `restore`. Answers what it holds.
fn unpack(
    file: &Path,
    restore: &Path,
    stage: &Path,
    size: f64,
    sha256: &str,
    passphrase: &str,
) -> Result<MemoryCounts, PhoneError> {
    let damaged = |_| PhoneError::Invalid;
    let len = std::fs::metadata(file).map_err(damaged)?.len();
    if len as f64 != size || digest(file).map_err(damaged)? != sha256 {
        return Err(PhoneError::Invalid);
    }
    crate::fsutil::create_private_dir(restore).map_err(|_| PhoneError::Busy)?;
    crate::fsutil::shred_tree(stage);
    let quiet = |_, _, _| {};
    let summary =
        nocturne_engine::unpack_backup(file, passphrase, stage, &quiet, &AtomicBool::new(false))
            .map_err(|e| match e {
                nocturne_engine::Error::BackupTooNew => PhoneError::ComputerOutdated,
                nocturne_engine::Error::Io(_) => transient(&e),
                _ => PhoneError::Invalid,
            })?;
    Ok(MemoryCounts {
        items: summary.items,
        memories: summary.memories,
        pages: summary.pages,
        originals: summary.originals,
    })
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

/// The move as the window shows it, if one is under way or waits to be seen.
pub fn view(core: &Core) -> Option<MoveView> {
    core.moves.refresh_room(free_here);
    core.moves.view()
}

/// "Open today's page" or "OK": the person has seen how the move ended. False when nothing
/// waits to be seen.
pub fn finish(core: &Core) -> bool {
    core.moves.dismiss()
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

    fn accepted() -> RemoteAnswer {
        RemoteAnswer::MemoryAccepted {
            here: None,
            bring: true,
        }
    }

    fn added() -> BroughtCounts {
        BroughtCounts {
            things: 2,
            entries: 1,
            checkins: 1,
        }
    }

    fn brought() -> Result<Outcome, PhoneError> {
        Ok((counts(3), added()))
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

    /// Agreed at once, as to a computer that holds nothing, and on its way: answers the move's
    /// owner.
    fn agreed(moves: &Moves, data: &Path, id: &str) -> Owner {
        let m = Move::new(&phone(id), data, counts(1), 10.0);
        assert!(moves.claim(m).is_some(), "{id} may offer");
        moves.carry(id, 1).expect("it may send").0
    }

    /// A phone offering to a computer that holds a memory, with the question on screen.
    fn asked_with_a_memory_here(
        moves: &Moves,
        data: &Path,
        id: &str,
    ) -> oneshot::Receiver<MoveChoice> {
        let (tx, rx) = oneshot::channel();
        let mut m = Move::new(&phone(id), data, counts(1), 10.0);
        m.here = Some(counts(5));
        m.phase = Phase::Offered(Some(tx));
        assert!(moves.claim(m).is_some());
        rx
    }

    /// Agreed to bring its things in, its upload whole and checked.
    fn checked_bring(moves: &Moves, data: &Path, id: &str) -> Owner {
        let mut rx = asked_with_a_memory_here(moves, data, id);
        assert!(moves.decide(MoveChoice::Bring, |_| None).taken);
        assert_eq!(rx.try_recv().unwrap(), MoveChoice::Bring);
        let (owner, _) = moves.carry(id, 1).unwrap();
        moves.begin_check(&owner, 1).unwrap();
        owner
    }

    #[test]
    fn a_move_goes_through_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        assert!(matches!(moves.view(), Some(MoveView::Preparing { .. })));
        moves.arriving("a", 0, 10);
        assert!(
            matches!(moves.view(), Some(MoveView::Preparing { .. })),
            "still being got ready until the first part is in"
        );
        moves.arriving("a", 4, 10);
        assert!(matches!(
            moves.view(),
            Some(MoveView::Arriving {
                done: 4.0,
                size: 10.0,
                ..
            })
        ));
        assert!(moves.begin_check(&owner, 1).is_some());
        assert!(matches!(moves.view(), Some(MoveView::Adding { .. })));
        assert!(moves.begin_adding(&owner));
        let (view, answer) = moves.conclude(&owner, false, brought());
        assert!(matches!(
            view,
            Some(MoveView::Brought { removed: false, .. })
        ));
        assert_eq!(
            answer,
            RemoteAnswer::MemoryBrought {
                counts: counts(3),
                added: added()
            }
        );
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
        asked_with_a_memory_here(&moves, dir.path(), "a");
        assert!(moves.forget("a"));
        assert_eq!(moves.view(), None);
        assert!(!moves.decide(MoveChoice::Bring, |_| None).taken);
    }

    #[test]
    fn removed_during_the_upload_nothing_is_checked() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        moves.arriving("a", 5, 10);
        assert!(moves.forget("a"));
        assert!(matches!(
            moves.view(),
            Some(MoveView::Ended {
                why: EndedWhy::Removed,
                ..
            })
        ));
        assert!(
            moves.begin_check(&owner, 1).is_none(),
            "the check never starts"
        );
    }

    #[test]
    fn a_failed_check_frees_the_place() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        moves.begin_check(&owner, 1).unwrap();
        let (view, answer) = moves.conclude(&owner, false, Err(PhoneError::Invalid));
        assert_eq!((view, answer), (None, err(PhoneError::Invalid)));
        assert!(lock(&moves.slot).is_none());
    }

    #[test]
    fn two_phones_never_undo_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let a = agreed(&moves, dir.path(), "a");
        moves.begin_check(&a, 1).unwrap();

        // Phone b cannot start under a's check.
        let b_move = Move::new(&phone("b"), dir.path(), counts(1), 10.0);
        assert!(moves.claim(b_move).is_none(), "b is asked to wait");

        // a's check ends with nothing added; then b moves.
        moves.conclude(&a, false, Err(PhoneError::Invalid));
        let b = agreed(&moves, dir.path(), "b");
        moves.begin_check(&b, 1).unwrap();

        // A late end of a's check (a second report of it) touches nothing of b's.
        let (view, answer) = moves.conclude(&a, true, brought());
        assert_eq!(answer, err(PhoneError::Revoked));
        assert!(matches!(view, Some(MoveView::Adding { .. })));
        assert!(moves.begin_adding(&b));
    }

    #[test]
    fn a_late_failure_from_an_old_session_leaves_the_new_upload_alone() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        moves.arriving("a", 5, 10);
        // The phone asks again on a new session once the first is gone, and sends again.
        assert_eq!(moves.agreed("a"), Some((accepted(), true)));
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
        let (view, answer) = moves.conclude(&owner, false, Err(PhoneError::Busy));
        assert_eq!(answer, err(PhoneError::Busy));
        assert!(matches!(view, Some(MoveView::Preparing { .. })));
        assert_eq!(
            moves.agreed("a"),
            Some((accepted(), false)),
            "not asked again"
        );
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
        let b = Move::new(&phone("b"), dir.path(), counts(1), 10.0);
        assert!(moves.claim(b).is_some());
    }

    #[test]
    fn bringing_in_needs_room_for_the_upload_the_unpacked_copy_and_the_originals() {
        let gb = 1024 * 1024 * 1024;
        assert_eq!(room_needed(gb), 3 * gb + ROOM_MARGIN);
        assert!(short(gb, Some(2 * gb + ROOM_MARGIN)).is_some());
        assert!(short(gb, Some(3 * gb + ROOM_MARGIN)).is_none());
        assert!(
            short(gb, None).is_none(),
            "room that cannot be read is never short"
        );
    }

    #[test]
    fn short_of_room_the_question_waits_and_bringing_in_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let mut m = Move::new(&phone("a"), dir.path(), counts(1), 1000.0);
        m.here = Some(counts(5));
        m.room = short(1000, Some(10));
        m.phase = Phase::Offered(None);
        moves.claim(m).unwrap();
        assert!(matches!(
            moves.view(),
            Some(MoveView::Offered { room: Some(_), .. })
        ));
        assert!(
            !moves.decide(MoveChoice::Bring, |_| Some(10)).taken,
            "no room yet"
        );
        moves.refresh_room(|_| Some(u64::MAX));
        assert!(matches!(
            moves.view(),
            Some(MoveView::Offered { room: None, .. })
        ));
        // No offer is waiting: bringing in agrees for the phone's next try.
        let decided = moves.decide(MoveChoice::Bring, |_| Some(u64::MAX));
        assert!(decided.taken && decided.unpair.is_none());
        assert_eq!(
            moves.agreed("a").map(|a| a.0),
            Some(RemoteAnswer::MemoryAccepted {
                here: Some(counts(5)),
                bring: true
            })
        );
    }

    #[test]
    fn not_now_with_no_offer_waiting_unpairs_the_phone() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let mut m = Move::new(&phone("a"), dir.path(), counts(1), 1000.0);
        m.here = Some(counts(5));
        m.phase = Phase::Offered(None);
        moves.claim(m).unwrap();
        let decided = moves.decide(MoveChoice::NotNow, |_| None);
        assert!(decided.taken);
        assert_eq!(decided.unpair.as_deref(), Some("a"));
        assert_eq!(decided.view, None);
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

        // Being checked: nothing is added, and the phone hears it was stopped.
        let owner = agreed(&moves, dir.path(), "a");
        moves.begin_check(&owner, 1).unwrap();
        assert_eq!(moves.stop().as_deref(), Some("a"));
        assert!(
            moves
                .claim(Move::new(&phone("b"), dir.path(), counts(1), 1.0))
                .is_none()
        );
        assert!(!moves.begin_adding(&owner), "nothing may be added");
        let (view, answer) = moves.conclude(&owner, false, Err(PhoneError::Revoked));
        assert_eq!((view, answer), (None, err(PhoneError::Paused)));

        // Nothing to stop once its things are being added.
        let owner = agreed(&moves, dir.path(), "a");
        moves.begin_check(&owner, 1).unwrap();
        assert!(moves.begin_adding(&owner));
        assert_eq!(moves.stop(), None, "adding is never stopped");
        assert_eq!(
            moves.phone_stopped("a"),
            Stopping::TooLate,
            "not from the phone either"
        );
    }

    #[test]
    fn a_computer_that_holds_a_memory_asks_how_the_phone_joins() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let mut rx = asked_with_a_memory_here(&moves, dir.path(), "a");
        assert!(matches!(moves.view(), Some(MoveView::Offered { .. })));
        let decided = moves.decide(MoveChoice::Bring, |_| None);
        assert!(decided.taken);
        assert_eq!(rx.try_recv().unwrap(), MoveChoice::Bring);
        assert!(matches!(moves.view(), Some(MoveView::Preparing { .. })));
        assert_eq!(
            moves.agreed("a").map(|a| a.0),
            Some(RemoteAnswer::MemoryAccepted {
                here: Some(counts(5)),
                bring: true
            }),
            "a phone carrying on hears it brings its things in"
        );
    }

    #[test]
    fn using_this_computers_memory_is_shown_and_holds_no_place() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let mut rx = asked_with_a_memory_here(&moves, dir.path(), "a");
        let decided = moves.decide(MoveChoice::UseHere, |_| Some(0));
        assert!(decided.taken, "using this memory needs no room");
        assert_eq!(rx.try_recv().unwrap(), MoveChoice::UseHere);
        assert_eq!(decided.used.map(|u| u.0).as_deref(), Some("a"));
        assert!(matches!(decided.view, Some(MoveView::Used { .. })));
        assert!(!moves.may_upload("a"), "nothing comes in");
        let b = Move::new(&phone("b"), dir.path(), counts(1), 1.0);
        assert!(moves.claim(b).is_some(), "another phone may move");
        assert!(!moves.dismiss());
    }

    #[test]
    fn bringing_things_in_waits_for_room_but_using_this_memory_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        asked_with_a_memory_here(&moves, dir.path(), "a");
        assert!(!moves.decide(MoveChoice::Bring, |_| Some(10)).taken);
        assert!(matches!(
            moves.view(),
            Some(MoveView::Offered { room: Some(_), .. })
        ));
        assert!(moves.decide(MoveChoice::UseHere, |_| Some(10)).taken);
    }

    #[test]
    fn a_bring_in_goes_through_and_is_seen() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = checked_bring(&moves, dir.path(), "a");
        assert!(matches!(moves.view(), Some(MoveView::Adding { .. })));
        assert!(moves.begin_adding(&owner));
        let (view, answer) = moves.conclude(&owner, false, brought());
        assert!(matches!(answer, RemoteAnswer::MemoryBrought { .. }));
        assert!(matches!(view, Some(MoveView::Brought { .. })));
        assert!(moves.arrived_for("a"), "an upload sent again is not taken");
        assert!(moves.dismiss());
        assert_eq!(moves.view(), None);
    }

    #[test]
    fn a_phone_removed_while_its_things_are_added_frees_the_place_after() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = checked_bring(&moves, dir.path(), "a");
        assert!(moves.begin_adding(&owner));
        assert!(moves.forget("a"));
        assert_eq!(moves.view(), None);
        let b = Move::new(&phone("b"), dir.path(), counts(1), 1.0);
        assert!(moves.claim(b).is_none(), "nothing starts under the adding");
        let (view, answer) = moves.conclude(&owner, true, brought());
        assert_eq!(
            view,
            Some(MoveView::Brought {
                name: "a".into(),
                added: added(),
                removed: true
            }),
            "adding finished, and the person here hears the phone still holds its memory"
        );
        assert!(matches!(answer, RemoteAnswer::MemoryBrought { .. }));
        let b = Move::new(&phone("b"), dir.path(), counts(1), 1.0);
        assert!(moves.claim(b).is_some(), "the place is free after");
    }

    #[test]
    fn a_bring_in_whose_phone_went_while_checked_says_nothing_came_in() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = checked_bring(&moves, dir.path(), "a");
        assert!(moves.forget("a"));
        assert!(!moves.begin_adding(&owner), "nothing may be added");
        let (view, answer) = moves.conclude(&owner, true, Err(PhoneError::Revoked));
        assert_eq!(answer, err(PhoneError::Revoked));
        assert_eq!(
            view,
            Some(MoveView::Ended {
                name: "a".into(),
                why: EndedWhy::Removed
            })
        );
        assert!(moves.dismiss());
        assert_eq!(moves.view(), None);
    }

    #[test]
    fn a_bring_in_the_phone_stopped_while_checked_adds_nothing_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = checked_bring(&moves, dir.path(), "a");
        assert_eq!(moves.phone_stopped("a"), Stopping::Stopped);
        assert!(!moves.begin_adding(&owner), "nothing may be added");
        // The pairing it undid for it takes nothing more away.
        assert!(!moves.forget("a"));
        let (view, _) = moves.conclude(&owner, true, Err(PhoneError::Revoked));
        assert_eq!(
            view,
            Some(MoveView::Ended {
                name: "a".into(),
                why: EndedWhy::Stopped
            })
        );
        assert_eq!(moves.phone_stopped("a"), Stopping::Nothing, "once");
    }

    #[test]
    fn a_phone_that_joined_hears_how_again() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let used = Joined {
            device: "a".into(),
            owner: "o1".into(),
            how: How::Used,
        };
        note_joined(&moves, dir.path(), used).unwrap();
        let offer = |bring| Asked::Offer {
            counts: counts(3),
            bring,
        };
        assert_eq!(
            joined_here(dir.path(), "a", offer(false)),
            Some(RemoteAnswer::UseComputerMemory)
        );
        assert_eq!(
            joined_here(dir.path(), "a", offer(true)),
            None,
            "bringing in what was put aside is a new move"
        );
        assert_eq!(joined_here(dir.path(), "a", Asked::Status), None);
        let brought = Joined {
            device: "a".into(),
            owner: "o2".into(),
            how: How::Brought {
                counts: counts(3),
                added: added(),
            },
        };
        note_joined(&moves, dir.path(), brought).unwrap();
        let heard = Some(RemoteAnswer::MemoryBrought {
            counts: counts(3),
            added: added(),
        });
        assert_eq!(joined_here(dir.path(), "a", offer(true)), heard);
        assert_eq!(joined_here(dir.path(), "a", offer(false)), heard);
        assert_eq!(joined_here(dir.path(), "a", Asked::Status), heard);
        let changed = Asked::Offer {
            counts: counts(4),
            bring: false,
        };
        assert_eq!(
            joined_here(dir.path(), "a", changed),
            None,
            "a memory that changed since is brought in again"
        );
        assert_eq!(read_joined(dir.path()).len(), 1, "one note a phone");
        for n in 0..JOINED_KEPT + 4 {
            let other = Joined {
                device: format!("p{n}"),
                owner: format!("o{n}"),
                how: How::Used,
            };
            note_joined(&moves, dir.path(), other).unwrap();
        }
        assert_eq!(read_joined(dir.path()).len(), JOINED_KEPT);
    }

    #[test]
    fn room_running_out_is_said_and_holds_no_place() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = agreed(&moves, dir.path(), "a");
        moves.begin_check(&owner, 1).unwrap();
        let (view, answer) = moves.conclude(&owner, false, Err(PhoneError::TooLarge));
        assert_eq!(answer, err(PhoneError::TooLarge));
        assert!(matches!(
            view,
            Some(MoveView::Failed {
                why: FailedWhy::NoRoom,
                room: Some(_),
                ..
            })
        ));
        assert!(
            moves
                .claim(Move::new(&phone("b"), dir.path(), counts(1), 1.0))
                .is_some()
        );
        assert!(!moves.dismiss(), "the new offer took its place");
    }
}
