//! What the phone remembers about its sends, in a JSON file written whole: what went across (a
//! hash for each entry and reading, to send only what changed), what waits to go (the latest
//! version of each, while the computer cannot be reached) and the Health changes token. The
//! person's choices for each source are a file of their own, so a damaged ledger never undoes
//! them.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use nocturne_contract::{CalendarChange, CalendarKind, PhoneCalendar, ReadingChange, ReadingKind};
use serde::{Deserialize, Serialize};

const FILE: &str = "sync.json";
const CHOICES: &str = "sources.json";

/// The kinds that start on; weight is offered and starts off.
pub const DEFAULT_KINDS: [ReadingKind; 6] = [
    ReadingKind::Sleep,
    ReadingKind::HeartRate,
    ReadingKind::RestingHeartRate,
    ReadingKind::BloodPressure,
    ReadingKind::Steps,
    ReadingKind::Workout,
];

/// "Your calendar": whether it is brought, which calendars, and notes. A calendar not in
/// `calendars` takes its default: on, unless it is a holiday or birthday calendar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CalendarSettings {
    pub bring: bool,
    pub calendars: BTreeMap<String, bool>,
    pub notes: bool,
}

impl CalendarSettings {
    pub fn brings(&self, c: &PhoneCalendar) -> bool {
        self.calendars
            .get(&c.id)
            .copied()
            .unwrap_or(c.kind == CalendarKind::Ordinary)
    }

    /// Every calendar on the phone with its switch as it stands, defaults filled in.
    pub fn filled(&self, all: &[PhoneCalendar]) -> CalendarSettings {
        CalendarSettings {
            bring: self.bring,
            calendars: all.iter().map(|c| (c.id.clone(), self.brings(c))).collect(),
            notes: self.notes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HealthSettings {
    pub bring: bool,
    pub kinds: Vec<ReadingKind>,
    pub older_readings: bool,
}

impl Default for HealthSettings {
    fn default() -> HealthSettings {
        HealthSettings {
            bring: false,
            kinds: DEFAULT_KINDS.to_vec(),
            older_readings: false,
        }
    }
}

/// "Last sent {time} · {count} calendar changes, {count} readings".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastSent {
    pub at: String,
    pub calendar_changes: u32,
    pub readings: u32,
}

/// "Last sent {time} · {count} appointments" or "{count} readings".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSent {
    pub at: String,
    pub count: u32,
}

/// The person's choices for each source.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Choices {
    calendar: CalendarSettings,
    health: HealthSettings,
}

/// An entry as it last went across.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentEntry {
    pub hash: String,
    /// Its day as the phone read it: an entry missing from the window counts as removed only
    /// while this day is still inside it.
    pub day: String,
    pub calendar: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CalendarLedger {
    /// Kept in the choices file.
    #[serde(skip)]
    pub settings: CalendarSettings,
    pub sent: BTreeMap<String, SentEntry>,
    /// What waits to go, by phone id: the latest version of each.
    pub pending: BTreeMap<String, CalendarChange>,
    /// The window the pending changes were read in.
    pub window: Option<(String, String)>,
    /// A hash of the calendars list as it last went across.
    pub calendars_sent: Option<String>,
    /// The computer is not taking the calendar from this phone.
    pub stopped: bool,
    /// The last send had more than the computer takes at once.
    pub too_many: bool,
    pub last_sent: Option<SourceSent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentReading {
    pub hash: String,
    pub day: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HealthLedger {
    /// Kept in the choices file.
    #[serde(skip)]
    pub settings: HealthSettings,
    /// By `kind/phoneId`.
    pub sent: BTreeMap<String, SentReading>,
    pub pending: BTreeMap<String, ReadingChange>,
    /// Health Connect's changes token, and what it was made for.
    pub token: Option<String>,
    pub token_kinds: Vec<ReadingKind>,
    pub token_older: bool,
    pub token_distance: bool,
    /// The first day the last full read covered.
    pub from_day: Option<String>,
    pub stopped: bool,
    pub too_many: bool,
    pub last_sent: Option<SourceSent>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Ledger {
    pub calendar: CalendarLedger,
    pub health: HealthLedger,
    pub last_sent: Option<LastSent>,
}

/// Written private and whole: to a temporary file first, then renamed over the old one.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let unique: String = nocturne_channel::crypto::random::<6>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let tmp = path.with_file_name(format!(".{name}.{unique}.tmp"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = (|| {
        let mut f = options.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return written;
    }
    // The rename itself is kept only once the folder is on disk.
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        std::fs::File::open(dir)?.sync_all()?;
    }
    Ok(())
}

/// The ledger in memory, saved after every change that matters.
pub struct LedgerFile {
    dir: PathBuf,
    ledger: Mutex<Ledger>,
}

fn read<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

impl LedgerFile {
    /// A damaged or missing ledger reads as a fresh one: the computer keeps a reading or entry
    /// once however often it is sent. The choices are read on their own.
    pub fn open(dir: &Path) -> LedgerFile {
        let mut ledger: Ledger = read(&dir.join(FILE));
        let choices: Choices = read(&dir.join(CHOICES));
        ledger.calendar.settings = choices.calendar;
        ledger.health.settings = choices.health;
        LedgerFile {
            dir: dir.to_path_buf(),
            ledger: Mutex::new(ledger),
        }
    }

    pub fn get(&self) -> Ledger {
        self.ledger
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Change the ledger and save it, and the choices when they changed.
    pub fn update<T>(&self, f: impl FnOnce(&mut Ledger) -> T) -> T {
        let mut ledger = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
        let before = (
            ledger.calendar.settings.clone(),
            ledger.health.settings.clone(),
        );
        let out = f(&mut ledger);
        let _ = std::fs::create_dir_all(&self.dir);
        let bytes = serde_json::to_vec(&*ledger).expect("the ledger serialises");
        if let Err(e) = write_private(&self.dir.join(FILE), &bytes) {
            tracing::warn!("could not keep what was sent: {e}");
        }
        if before
            != (
                ledger.calendar.settings.clone(),
                ledger.health.settings.clone(),
            )
            || !self.dir.join(CHOICES).exists()
        {
            let choices = Choices {
                calendar: ledger.calendar.settings.clone(),
                health: ledger.health.settings.clone(),
            };
            let bytes = serde_json::to_vec(&choices).expect("the choices serialise");
            if let Err(e) = write_private(&self.dir.join(CHOICES), &bytes) {
                tracing::warn!("could not keep the choices: {e}");
            }
        }
        out
    }

    /// The phone was disconnected or removed: what was sent belongs to that pairing.
    pub fn forget_sends(&self) {
        self.update(|l| {
            let (calendar, health) = (l.calendar.settings.clone(), l.health.settings.clone());
            *l = Ledger::default();
            l.calendar.settings = calendar;
            l.health.settings = health;
        });
    }
}
