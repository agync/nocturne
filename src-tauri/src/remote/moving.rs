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
//! | `MemoryBrought` | either | the phone's things were added to the memory here |
//! | `UseComputerMemory` | `offerMemory` | the person here chose this computer's memory |
//! | `NotFound` | `memoryDone` | the memory could not be opened after the restart; nothing changed |
//! | `NotReady` | `memoryDone` | nothing is known of the phone's memory here; it offers it again |
//!
//! A `memoryDone` of size 0 sends nothing: it asks where the phone's memory stands, which is how a
//! phone waits through the restart without ever starting a new move.
//! | `Paused` | `memoryDone` | the person here stopped the move; nothing changed |
//! | `Revoked` | either | the person kept what is here, or removed the phone |
//!
//! A computer that already holds a memory never puts it aside for one arriving. It asks how
//! the phone joins it: "Bring the phone's things in" agrees as above, but the checked memory is
//! unpacked beside the store in use and its things are added to it in one step
//! (`Engine::bring_in`), with no restart; the phone hears `MemoryBrought`. "Use this computer's
//! memory" brings nothing: the phone hears `UseComputerMemory` and stays paired. A phone that
//! offers with `bring` (the person chose to bring in what a move put aside on it) is not asked
//! about again. Stopping a bring-in is possible until the adding begins. Both answers are noted
//! (`joined.json`), so a phone that missed one hears it again.
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
/// Where the phones that joined the memory here are remembered, for the same reason.
const JOINED: &str = "joined.json";
/// The phones `JOINED` remembers, newest last.
const JOINED_KEPT: usize = 16;
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
    /// "{name} has its own memory. Move it here?", or with `here` (this computer already holds
    /// one) "{name} has its own memory, and this computer has one too." `since`: the first day
    /// something was kept on the phone. `room`: not enough free room yet, and the move button
    /// waits.
    Offered {
        name: String,
        items: u32,
        bytes: f64,
        here: bool,
        since: Option<String>,
        room: Option<Room>,
    },
    /// "{name} is getting its memory ready…", or with `bring` "{name} is getting its things
    /// ready…": agreed, and nothing has arrived yet while the phone makes the copy that travels.
    Preparing {
        name: String,
        bring: bool,
    },
    Arriving {
        name: String,
        done: f64,
        size: f64,
        bring: bool,
    },
    Checking {
        name: String,
    },
    /// "Adding them to the memory here…": a bring-in arrived whole. It can no longer be stopped.
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
    NotOpened,
}

/// The person's answer to an offer. To a computer that holds nothing: move it here, or not now.
/// To one that holds a memory: bring the phone's things in, use this computer's memory, or not
/// now. Not now keeps what is here and undoes the pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MoveChoice {
    Move,
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
    Checking,
    /// A bring-in's things are being added; nothing stops it now.
    Adding,
    Arrived(MemoryCounts),
    /// Ended with nothing changed; shown until the person says OK. It holds no place.
    Failed(FailedWhy),
    /// A bring-in's things were added; shown until the person opens today's page. It holds no
    /// place.
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
    /// The phone's things are added to the memory here rather than taking its place.
    bring: bool,
    online: bool,
    phase: Phase,
    /// The data folder its stage is in.
    data: PathBuf,
    /// Its phone was removed while the memory was being checked: it stays, unseen, until the
    /// check ends and lets its stage go, so no other move starts under it.
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
            bring: false,
            online: false,
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
                here: self.here.is_some(),
                since: self.since.clone(),
                room: self.room,
            },
            Phase::Accepted | Phase::Arriving { done: 0, .. } => MoveView::Preparing {
                name,
                bring: self.bring,
            },
            Phase::Arriving { done, size } => MoveView::Arriving {
                name,
                done: *done as f64,
                size: *size as f64,
                bring: self.bring,
            },
            Phase::Checking if self.bring => MoveView::Adding { name },
            Phase::Checking => MoveView::Checking { name },
            Phase::Adding => MoveView::Adding { name },
            Phase::Brought(added) => MoveView::Brought {
                name,
                added: *added,
                removed: self.removed,
            },
            Phase::Used => MoveView::Used { name },
            Phase::Ended(why) => MoveView::Ended { name, why: *why },
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
/// check ends); Arrived --> (none), its stage let go. A bring-in's forget or phone_stopped:
/// Accepted/Arriving --> Ended; Checking --> gone, and its conclude --> Ended.
///
/// A computer that holds a memory: Offered --decide(bring)--> Accepted (bring), on as above to
/// Checking --begin_adding--> Adding --conclude(ok)--> Brought; Offered --decide(use here)-->
/// Used. Adding is never stopped; forget marks it gone like Checking, and its conclude still
/// says Brought, with the phone removed. Brought, Used, Failed and Ended hold no place.
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

/// How a checked memory ended up here: in the stage for the next start, or its things added to
/// the memory in use.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Outcome {
    Arrived(MemoryCounts),
    Brought(MemoryCounts, BroughtCounts),
}

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

    /// A memory is being checked to take the place of the one here, or waits for the next
    /// start: phones are asked to wait. Things being brought in leave the store in use, so
    /// phones carry on.
    pub fn holding(&self) -> bool {
        lock(&self.slot).as_ref().is_some_and(|m| match m.phase {
            Phase::Checking => !m.bring,
            Phase::Arrived(_) => true,
            _ => false,
        })
    }

    /// The phone's memory is already here, or its things were added: its upload is not taken
    /// again.
    pub fn arrived_for(&self, device: &str) -> bool {
        lock(&self.slot).as_ref().is_some_and(|m| {
            m.device == device
                && !m.gone
                && matches!(m.phase, Phase::Arrived(_) | Phase::Brought(_))
        })
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
            bring: m.bring,
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
                    Phase::Arriving { .. } | Phase::Checking | Phase::Adding | Phase::Arrived(_)
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
    /// and the move whose phone now uses this computer's memory. Moving or bringing in waits
    /// while there is not room. A computer that holds a memory never takes one whole, and one
    /// that holds none has nothing to join.
    fn decide(&self, choice: MoveChoice, free: impl Fn(&Path) -> Option<u64>) -> Decided {
        let mut slot = lock(&self.slot);
        let Some(m) = slot.as_mut().filter(|m| !m.gone) else {
            return Decided::not_taken(shown(&slot));
        };
        let here = m.here.is_some();
        let Phase::Offered(tx) = &mut m.phase else {
            return Decided::not_taken(shown(&slot));
        };
        let fits = match choice {
            MoveChoice::Move => !here,
            MoveChoice::Bring | MoveChoice::UseHere => here,
            MoveChoice::NotNow => true,
        };
        if !fits {
            return Decided::not_taken(shown(&slot));
        }
        if matches!(choice, MoveChoice::Move | MoveChoice::Bring)
            && let Some(room) = short(m.bytes as u64, free(&m.data))
        {
            // Room went since the question showed: it says so now, and moving waits.
            m.room = Some(room);
            return Decided::not_taken(shown(&slot));
        }
        let waiting = tx.take();
        let carried = waiting.is_some_and(|tx| tx.send(choice).is_ok());
        let unpair = (!carried && choice == MoveChoice::NotNow).then(|| m.device.clone());
        let mut used = None;
        match choice {
            MoveChoice::Move | MoveChoice::Bring => {
                m.room = None;
                m.bring = choice == MoveChoice::Bring;
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
        let Some(m) = slot
            .as_mut()
            .filter(|m| m.device == device && !m.gone && m.bring)
        else {
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

    /// Move `owner` brings its phone's things in rather than taking the place of the memory
    /// here.
    fn brings(&self, owner: &str) -> bool {
        lock(&self.slot)
            .as_ref()
            .is_some_and(|m| m.owner == owner && m.bring)
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

    /// A checked bring-in's things are about to be added: from here nothing stops it. False
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
        m.bring = true;
        true
    }

    /// How a check ends: arrived, or let go, with what it staged, when its phone was removed
    /// meanwhile. Only what carries this move's owner is let go, so a check that ends late
    /// never touches another phone's move.
    fn conclude(
        &self,
        data: &Path,
        owner: &str,
        removed: bool,
        checked: Result<Outcome, PhoneError>,
    ) -> (Option<MoveView>, RemoteAnswer) {
        let mut slot = lock(&self.slot);
        let ours = slot
            .as_mut()
            .filter(|m| m.owner == owner && matches!(m.phase, Phase::Checking | Phase::Adding));
        match (checked, ours) {
            (Ok(Outcome::Arrived(counts)), Some(m)) if !m.gone && !removed => {
                m.phase = Phase::Arrived(counts);
                m.conv = None;
                (shown(&slot), RemoteAnswer::MemoryArrived { counts })
            }
            (Ok(Outcome::Brought(counts, added)), Some(m)) if !m.gone && !removed => {
                m.phase = Phase::Brought(added);
                m.conv = None;
                (shown(&slot), RemoteAnswer::MemoryBrought { counts, added })
            }
            // Its phone was removed while its things were added: adding finished, since
            // everything had arrived whole, and the person here hears so.
            (Ok(Outcome::Brought(counts, added)), Some(m)) => {
                m.phase = Phase::Brought(added);
                m.conv = None;
                m.gone = false;
                m.removed = true;
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
                // Whatever this check staged goes, found or not; nothing of another move does.
                if checked.is_ok() {
                    unstage(data, owner);
                }
                let stopped = ours.as_ref().is_some_and(|m| m.stopped);
                match ours {
                    // A bring-in that adds nothing says why until the person says OK.
                    Some(m) if m.ended.is_some() => {
                        m.phase = Phase::Ended(m.ended.unwrap_or(EndedWhy::Removed));
                        m.conv = None;
                        m.gone = false;
                    }
                    Some(_) => *slot = None,
                    None => {}
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
            // A bring-in the phone stopped says so, though its pairing goes with it.
            Phase::Ended(_) => return false,
            // A bring-in's things arriving: nothing is added, and the person here hears why.
            Phase::Accepted | Phase::Arriving { .. } if m.bring => {
                m.phase = Phase::Ended(EndedWhy::Removed);
                m.conv = None;
            }
            Phase::Checking if m.bring => {
                m.gone = true;
                m.ended = Some(EndedWhy::Removed);
            }
            Phase::Checking | Phase::Adding => m.gone = true,
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
            Some(Phase::Checking | Phase::Adding) => Some(Err(())),
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

/// An answer to a phone, for the log: never more than its kind, its code or its counts.
fn said(answer: &RemoteAnswer) -> String {
    match answer {
        RemoteAnswer::MemoryArrived { counts } => format!(
            "opened, {} items, {} memories, {} pages, {} originals",
            counts.items, counts.memories, counts.pages, counts.originals
        ),
        RemoteAnswer::MemoryAccepted { bring: false, .. } => "agreed".into(),
        RemoteAnswer::MemoryAccepted { bring: true, .. } => "agreed to bring its things in".into(),
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

/// `offerMemory`: wait for the person on this computer, then welcome the memory, say this
/// computer's memory is used, or undo the pairing. A phone that brings in what a move put aside
/// is not asked about again. A phone that asks again after its memory arrived, or after it
/// joined the memory here, is told so.
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
    let looked = crate::error::blocking(move || Ok((held(&e)?, e.online_state()?.set_up))).await;
    let Ok((here, online)) = looked else {
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
    new.online = online;
    new.room = room;
    if bring {
        // The person chose on the phone to bring in what a move put aside there: nothing is
        // asked here, and the phone hears at once.
        if let Some(room) = room {
            tracing::info!(
                device = id,
                "not enough room to bring the phone's things in: {} bytes free of {} needed",
                room.free,
                room.need
            );
            return Some(err(PhoneError::TooLarge));
        }
        new.bring = true;
        new.room = None;
        new.agreed_again();
        let Some(view) = core.moves.claim(new) else {
            tracing::info!(device = id, "another move holds the place: the phone waits");
            return Some(err(PhoneError::Busy));
        };
        emit(app, view);
        tracing::info!(device = id, "the phone brings its things in");
        return Some(RemoteAnswer::MemoryAccepted { here, bring: true });
    }
    // Short of room for a whole move, the question shows with its room line and the phone hears
    // so at once: it tries again once the person here has freed some. A computer that holds a
    // memory keeps the phone waiting, since using its memory needs no room.
    new.phase = Phase::Offered((room.is_none() || here.is_some()).then_some(tx));
    let owner = new.owner.clone();
    let Some(view) = core.moves.claim(new) else {
        tracing::info!(device = id, "another move holds the place: the phone waits");
        return Some(err(PhoneError::Busy));
    };
    emit(app, view);
    crate::window::show_main(app);
    if let Some(room) = room.filter(|_| here.is_none()) {
        tracing::info!(
            device = id,
            "not enough room for the move yet: {} bytes free of {} needed",
            room.free,
            room.need
        );
        return Some(err(PhoneError::TooLarge));
    }
    tracing::info!(
        device = id,
        here = here.is_some(),
        "the person here is asked about the move"
    );
    let choice = tokio::select! {
        choice = rx => Some(choice),
        () = out.closed() => None,
        () = tokio::time::sleep(DECIDE_WAIT) => None,
    };
    match choice {
        Some(Ok(MoveChoice::Move)) => {
            tracing::info!(
                device = id,
                "the move is agreed: the phone makes its backup"
            );
            Some(RemoteAnswer::MemoryAccepted { here, bring: false })
        }
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

/// A memory being checked is waited for; one staged for the restart has the phone wait; one
/// that opened after it is answered from what arrived, and a phone that joined the memory here
/// hears how. For an offer, a memory that did not open is let go, so the offer is a new move.
async fn already(
    core: &Arc<Core>,
    device: &str,
    data: &Path,
    asked: Asked,
) -> Option<RemoteAnswer> {
    match settled_state(core, device).await {
        Some(answer) => Some(answer),
        None => joined_here(data, device, asked).or_else(|| opened_here(core, data, device, false)),
    }
}

/// Where this phone's memory stands, for a `memoryDone` that sends nothing.
async fn status(core: &Arc<Core>, device: &str, data: &Path) -> RemoteAnswer {
    match settled_state(core, device).await {
        Some(answer) => answer,
        None => joined_here(data, device, Asked::Status)
            .or_else(|| opened_here(core, data, device, true))
            .unwrap_or(err(PhoneError::NotReady)),
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

/// The person's answer on this computer. False when there was nothing to answer, the answer
/// does not fit (moving whole into a computer that holds a memory, or joining one that holds
/// none), or no room yet to move it here.
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

/// "Stop moving" on this computer: the phone's upload ends, and the phone hears the move was
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
    // staged for it.
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
    let bring = core.moves.brings(&owner);
    if !bring {
        // The store in use is about to be put aside for the one arriving.
        state.asks.cancel_all();
    }

    let checked = check(&state, core, device, &owner, &file, sent, bring).await;
    let _ = std::fs::remove_file(&file);

    // The phone may have been removed while its memory was checked: then what was staged goes.
    let (view, answer) = core
        .moves
        .conclude(&data, &owner, core.removed(&device.id), checked);
    core.moves.settled.notify_waiters();
    emit(app, view);
    tracing::info!(device = id, "the memory was checked: {}", said(&answer));
    let arrived = matches!(answer, RemoteAnswer::MemoryArrived { .. });
    if matches!(answer, RemoteAnswer::MemoryBrought { .. }) {
        // What came in waits to be read as new.
        state.wake_pipeline();
    }
    out.send(answer);
    if arrived && let Some(restart) = state.restarter() {
        // As a restore does: Nocturne starts again with the memory in place, once the phone
        // has had its answer. Until then phones are asked to wait. A phone removed meanwhile
        // took its memory with it, and nothing starts again.
        tokio::time::sleep(ANSWER_WAIT).await;
        if core.moves.arrived_as(&owner) {
            tracing::info!(device = id, "Nocturne starts again to open the memory");
            restart();
        }
    }
    None
}

/// Check the memory and stage it for the next start, or add its things to the memory here when
/// they are brought in, or when this computer came to hold a memory of its own since the person
/// agreed (a memory here is never put aside for one arriving). Either waits for any backup or
/// restore that is running, and never runs over a restore that waits for the next start.
async fn check(
    state: &AppState,
    core: &Arc<Core>,
    device: &DeviceRow,
    owner: &str,
    file: &Path,
    sent: Done,
    bring: bool,
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
    let data = state.data_dir.clone();
    let out = if restore::marker_path(&data).exists() {
        Err(PhoneError::Busy)
    } else {
        // What is here now, not when the person agreed: a lot may have been kept since.
        let here = match state.engine() {
            Ok(e) => crate::error::blocking(move || held(&e))
                .await
                .ok()
                .flatten(),
            Err(_) => None,
        };
        if bring || here.is_some() {
            let out = bring_in(state, core, device, owner, file, &sent).await;
            state.end_backup();
            return out;
        }
        // Phones a replace before this one still has to ask stay asked.
        let resend = read_moved(&data).map(|m| m.resend).unwrap_or_default();
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
            .map(Outcome::Arrived)
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
    Ok(Outcome::Brought(counts, added))
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

    fn accepted() -> RemoteAnswer {
        RemoteAnswer::MemoryAccepted {
            here: None,
            bring: false,
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
        assert!(moves.holding());
        stage_as(dir.path(), &owner, "a");
        let (view, answer) =
            moves.conclude(dir.path(), &owner, false, Ok(Outcome::Arrived(counts(3))));
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
        assert!(!moves.decide(MoveChoice::Move, |_| None).taken);
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
        let (view, answer) =
            moves.conclude(dir.path(), &owner, false, Ok(Outcome::Arrived(counts(3))));
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
        let (view, answer) =
            moves.conclude(dir.path(), &owner, true, Ok(Outcome::Arrived(counts(3))));
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
        moves.conclude(dir.path(), &owner, false, Ok(Outcome::Arrived(counts(3))));
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
        moves.conclude(dir.path(), &a, false, Ok(Outcome::Arrived(counts(3))));
        let b = agreed(&moves, dir.path(), "b");
        moves.begin_check(&b, 1).unwrap();
        stage_as(dir.path(), &b, "b");

        // A late end of a's check (a second report of it) touches nothing of b's.
        let (view, answer) = moves.conclude(dir.path(), &a, true, Ok(Outcome::Arrived(counts(3))));
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
        let (view, answer) = moves.conclude(dir.path(), &owner, false, Err(PhoneError::Busy));
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
            !moves.decide(MoveChoice::Move, |_| Some(10)).taken,
            "no room yet"
        );
        moves.refresh_room(|_| Some(u64::MAX));
        assert!(matches!(
            moves.view(),
            Some(MoveView::Offered { room: None, .. })
        ));
        // No offer is waiting: moving agrees for the phone's next try.
        let decided = moves.decide(MoveChoice::Move, |_| Some(u64::MAX));
        assert!(decided.taken && decided.unpair.is_none());
        assert_eq!(moves.agreed("a"), Some((accepted(), false)));
    }

    #[test]
    fn not_now_with_no_offer_waiting_unpairs_the_phone() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let mut m = Move::new(&phone("a"), dir.path(), counts(1), 1000.0);
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
        let (_, answer) =
            moves.conclude(dir.path(), &owner, false, Ok(Outcome::Arrived(counts(3))));
        assert_eq!(answer, err(PhoneError::Paused));
        assert!(!staged(dir.path()));

        // Nothing to stop once it waits for the restart.
        let owner = agreed(&moves, dir.path(), "a");
        moves.begin_check(&owner, 1).unwrap();
        moves.conclude(dir.path(), &owner, false, Ok(Outcome::Arrived(counts(3))));
        assert_eq!(moves.stop(), None);
    }

    fn added() -> BroughtCounts {
        BroughtCounts {
            things: 2,
            entries: 1,
            checkins: 1,
        }
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

    #[test]
    fn a_computer_that_holds_a_memory_never_takes_one_whole() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let mut rx = asked_with_a_memory_here(&moves, dir.path(), "a");
        assert!(matches!(
            moves.view(),
            Some(MoveView::Offered { here: true, .. })
        ));
        assert!(
            !moves.decide(MoveChoice::Move, |_| None).taken,
            "no replace in a move"
        );
        let decided = moves.decide(MoveChoice::Bring, |_| None);
        assert!(decided.taken);
        assert_eq!(rx.try_recv().unwrap(), MoveChoice::Bring);
        assert!(matches!(
            moves.view(),
            Some(MoveView::Preparing { bring: true, .. })
        ));
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
    fn a_computer_that_holds_nothing_has_nothing_to_join() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let (tx, _rx) = oneshot::channel();
        let mut m = Move::new(&phone("a"), dir.path(), counts(1), 10.0);
        m.phase = Phase::Offered(Some(tx));
        moves.claim(m).unwrap();
        assert!(!moves.decide(MoveChoice::Bring, |_| None).taken);
        assert!(!moves.decide(MoveChoice::UseHere, |_| None).taken);
        assert!(moves.decide(MoveChoice::Move, |_| None).taken);
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
        let mut b = Move::new(&phone("b"), dir.path(), counts(1), 1.0);
        b.phase = Phase::Accepted;
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

    /// Agreed to bring its things in, its upload whole and checked.
    fn checked_bring(moves: &Moves, data: &Path, id: &str) -> Owner {
        let mut rx = asked_with_a_memory_here(moves, data, id);
        assert!(moves.decide(MoveChoice::Bring, |_| None).taken);
        assert_eq!(rx.try_recv().unwrap(), MoveChoice::Bring);
        let (owner, _) = moves.carry(id, 1).unwrap();
        assert!(moves.brings(&owner));
        moves.begin_check(&owner, 1).unwrap();
        owner
    }

    #[test]
    fn a_bring_in_goes_through_without_holding_other_phones() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = checked_bring(&moves, dir.path(), "a");
        assert!(matches!(moves.view(), Some(MoveView::Adding { .. })));
        assert!(
            !moves.holding(),
            "the store in use stays, so phones carry on"
        );
        assert!(moves.begin_adding(&owner));
        assert_eq!(moves.stop(), None, "adding is never stopped");
        assert_eq!(
            moves.phone_stopped("a"),
            Stopping::TooLate,
            "not from the phone either"
        );
        let (view, answer) = moves.conclude(
            dir.path(),
            &owner,
            false,
            Ok(Outcome::Brought(counts(3), added())),
        );
        assert_eq!(
            answer,
            RemoteAnswer::MemoryBrought {
                counts: counts(3),
                added: added()
            }
        );
        assert!(matches!(view, Some(MoveView::Brought { .. })));
        assert!(moves.arrived_for("a"), "an upload sent again is not taken");
        assert!(!moves.holding());
        assert!(moves.dismiss());
        assert_eq!(moves.view(), None);
    }

    #[test]
    fn a_bring_in_stopped_before_adding_adds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = checked_bring(&moves, dir.path(), "a");
        assert_eq!(moves.stop().as_deref(), Some("a"));
        assert!(!moves.begin_adding(&owner), "nothing may be added");
        let (view, answer) = moves.conclude(dir.path(), &owner, false, Err(PhoneError::Revoked));
        assert_eq!((view, answer), (None, err(PhoneError::Paused)));
    }

    #[test]
    fn a_phone_removed_while_its_things_are_added_frees_the_place_after() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = checked_bring(&moves, dir.path(), "a");
        assert!(moves.begin_adding(&owner));
        assert!(moves.forget("a"));
        assert_eq!(moves.view(), None);
        let mut b = Move::new(&phone("b"), dir.path(), counts(1), 1.0);
        b.phase = Phase::Accepted;
        assert!(moves.claim(b).is_none(), "nothing starts under the adding");
        let (view, answer) = moves.conclude(
            dir.path(),
            &owner,
            true,
            Ok(Outcome::Brought(counts(3), added())),
        );
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
        let mut b = Move::new(&phone("b"), dir.path(), counts(1), 1.0);
        b.phase = Phase::Accepted;
        assert!(moves.claim(b).is_some(), "the place is free after");
    }

    #[test]
    fn a_bring_in_whose_phone_went_while_checked_says_nothing_came_in() {
        let dir = tempfile::tempdir().unwrap();
        let moves = Moves::default();
        let owner = checked_bring(&moves, dir.path(), "a");
        assert!(moves.forget("a"));
        assert!(!moves.begin_adding(&owner), "nothing may be added");
        let (view, answer) = moves.conclude(dir.path(), &owner, true, Err(PhoneError::Revoked));
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
        let (view, _) = moves.conclude(dir.path(), &owner, true, Err(PhoneError::Revoked));
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
