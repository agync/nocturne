//! Away from home: the person's own paired phones reach Nocturne through a sealed channel.
//!
//! Off by default: until the person turns it on, nothing is downloaded, started, bound or
//! connected. The hub ([`controller::RemoteHub`]) is made on first use and starts nothing by
//! itself; at launch it starts only when the person left it on and auto-off is not due.

use std::error::Error;
use std::sync::{Arc, Mutex};

use jiff::Timestamp;
use nocturne_contract::{
    EVENT_DEVICE, EVENT_PAIRING, EVENT_REMOTE, PairingRequest, PhoneFeature, PhoneHello,
    PrivacyView, RemoteAdvanced, RemoteConnection,
};
use nocturne_engine::{DeviceRow, Engine};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::ask::AskLane;
use crate::state::AppState;

pub mod audit;
pub mod autooff;
pub mod controller;
pub mod crypto;
pub mod dispatch;
pub mod frame;
pub mod incoming;
pub mod journal;
pub mod keepawake;
pub mod keyfile;
pub mod limits;
pub mod listener;
pub mod moving;
pub mod name;
pub mod pairing;
pub mod protocol;
pub mod qr;
pub mod session;
pub mod static_files;
#[cfg(debug_assertions)]
#[doc(hidden)]
pub mod testing;

use controller::{Helpers, RealHelpers, RemoteHub};
use static_files::PhoneFiles;

/// The managed hub.
pub struct Hub(pub Arc<RemoteHub>);

/// The shell around the hub: the app's store, clock, events and asks.
pub struct AppShell<R: Runtime> {
    pub app: AppHandle<R>,
}

impl<R: Runtime> session::Shell for AppShell<R> {
    fn engine(&self) -> Option<Engine> {
        self.app.try_state::<AppState>()?.engine().ok()
    }

    fn wall_now(&self) -> Timestamp {
        self.app
            .try_state::<AppState>()
            .map(|s| s.now())
            .unwrap_or_else(Timestamp::now)
    }

    fn changed(&self) {
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            if let Some(hub) = hub(&app) {
                let view = hub.view().await;
                if let Err(e) = app.emit(EVENT_REMOTE, &view) {
                    tracing::warn!("could not emit Away from home's state: {e}");
                }
            }
        });
    }

    fn pairing_request(&self, request: &PairingRequest) {
        if let Err(e) = self.app.emit(EVENT_PAIRING, request) {
            tracing::warn!("could not emit a pairing request: {e}");
        }
        crate::window::show_main(&self.app);
    }

    fn device_active(&self, device: &DeviceRow) {
        if let Some(hub) = hub(&self.app) {
            let view = hub.device_view(device);
            if let Err(e) = self.app.emit(EVENT_DEVICE, &view) {
                tracing::warn!("could not emit a phone's use: {e}");
            }
        }
        self.changed();
    }

    fn hello(&self, device: &DeviceRow, connection: RemoteConnection) -> PhoneHello {
        PhoneHello {
            computer_name: name::computer_name(),
            device_name: device.name.clone(),
            connection,
            min_phone_build: protocol::MIN_PHONE_BUILD,
            max_phone_build: Some(protocol::PHONE_BUILD),
            version: self.app.package_info().version.to_string(),
            // A phone app offers the calendar and Health only to a computer that takes them, and
            // offers its own memory only to one that takes that.
            features: vec![
                PhoneFeature::Calendar,
                PhoneFeature::Health,
                PhoneFeature::HoldsMemory,
            ],
            memory_here: false,
        }
    }

    fn move_changed(&self, view: Option<&moving::MoveView>) {
        if let Err(e) = self.app.emit(moving::EVENT_MOVE, view) {
            tracing::warn!("could not emit the move: {e}");
        }
    }

    fn stop_phone(&self, device_id: &str) {
        if let Some(state) = self.app.try_state::<AppState>() {
            state
                .asks
                .cancel_lane(&AskLane::Phone(device_id.to_string()));
        }
    }
}

/// The phone app's files from the app's own assets.
#[cfg_attr(debug_assertions, allow(dead_code))]
struct AppFiles<R: Runtime>(AppHandle<R>);

impl<R: Runtime> PhoneFiles for AppFiles<R> {
    fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.0
            .asset_resolver()
            .get(format!("phone/{name}"))
            .map(|a| a.bytes)
    }
}

/// Development builds serve the phone app from its build folder; a release serves the copy
/// embedded with the app.
#[cfg(debug_assertions)]
fn phone_files<R: Runtime>(_app: &AppHandle<R>) -> Arc<dyn PhoneFiles> {
    let built = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/dist/phone");
    Arc::new(static_files::DirFiles(built))
}

#[cfg(not(debug_assertions))]
fn phone_files<R: Runtime>(app: &AppHandle<R>) -> Arc<dyn PhoneFiles> {
    Arc::new(AppFiles(app.clone()))
}

/// Make and manage the hub from these parts (tests pass fakes before first use).
pub fn manage_hub<R: Runtime>(
    app: &AppHandle<R>,
    helpers: Arc<dyn Helpers>,
    host: Arc<dyn nocturne_intelligence::Host>,
    files: Arc<dyn PhoneFiles>,
    mono: Arc<dyn limits::Mono>,
) -> Option<Arc<RemoteHub>> {
    let dir = app.try_state::<AppState>()?.data_dir.join("remote");
    let hub = RemoteHub::new(
        Arc::new(AppShell { app: app.clone() }),
        helpers,
        host,
        files,
        mono,
        dir,
    );
    hub.core
        .set_dispatch(Arc::new(dispatch::AppDispatch { app: app.clone() }));
    app.manage(Hub(hub));
    app.try_state::<Hub>().map(|h| h.0.clone())
}

static MAKING: Mutex<()> = Mutex::new(());

/// The hub, made on first use with the real helpers. Making it starts nothing.
pub fn hub<R: Runtime>(app: &AppHandle<R>) -> Option<Arc<RemoteHub>> {
    if let Some(h) = app.try_state::<Hub>() {
        return Some(h.0.clone());
    }
    let _making = MAKING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(h) = app.try_state::<Hub>() {
        return Some(h.0.clone());
    }
    let dir = app.try_state::<AppState>()?.data_dir.join("remote");
    manage_hub(
        app,
        Arc::new(RealHelpers {
            dirs: nocturne_helpers::HelperDirs { root: dir },
        }),
        Arc::new(nocturne_intelligence::SystemHost::new()),
        phone_files(app),
        Arc::new(limits::RealMono),
    )
}

/// Called once the app is ready. Starts nothing unless the person left Away from home on and
/// auto-off is not due.
pub fn install<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn Error>> {
    let Some(hub) = hub(app) else {
        return Ok(());
    };
    audit::prune(&hub.core);
    let stopper = hub.clone();
    if let Some(state) = app.try_state::<AppState>() {
        state.on_exit(Box::new(move || {
            let h = stopper.clone();
            tauri::async_runtime::block_on(async move { h.shutdown().await });
        }));
    }
    let Some(row) = hub.core.shell.engine().and_then(|e| e.remote_state().ok()) else {
        return Ok(());
    };
    if !row.wanted_on {
        return Ok(());
    }
    tauri::async_runtime::spawn(async move {
        if hub.auto_off_due() {
            let _ = hub.stop(false).await;
            return;
        }
        if let Err(e) = hub.start(row.connection, false).await {
            tracing::warn!("Away from home did not start again: {:?}", e.code);
        }
    });
    Ok(())
}

/// Before the store is wiped, once the phrase is right: stop the listener and the helper, and
/// sign out where needed.
pub async fn before_delete_everything<R: Runtime>(app: &AppHandle<R>) {
    note(app, "before_delete_everything");
    if let Some(h) = app.try_state::<Hub>() {
        h.0.before_delete().await;
    }
}

/// After the store is wiped: remove Away from home's own files.
pub fn on_delete_everything<R: Runtime>(app: &AppHandle<R>) {
    note(app, "on_delete_everything");
    match app.try_state::<Hub>() {
        Some(h) => h.0.after_delete(),
        None => {
            if let Some(state) = app.try_state::<AppState>() {
                let _ = std::fs::remove_dir_all(state.data_dir.join("remote"));
            }
        }
    }
}

/// The delete-everything hooks that ran, in order, when the app manages one. Tests manage it
/// to see whether a hook ran.
#[derive(Debug, Default)]
pub struct HookLog(Mutex<Vec<&'static str>>);

impl HookLog {
    pub fn calls(&self) -> Vec<&'static str> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

fn note<R: Runtime>(app: &AppHandle<R>, hook: &'static str) {
    if let Some(log) = app.try_state::<HookLog>() {
        log.0.lock().unwrap_or_else(|e| e.into_inner()).push(hook);
    }
}

/// Away from home's part of the privacy view.
pub fn privacy<R: Runtime>(app: &AppHandle<R>, view: &mut PrivacyView) {
    if let Some(h) = hub(app) {
        h.privacy(view);
    }
}

/// Away from home's part of the advanced details, once it has been set up.
pub fn advanced<R: Runtime>(app: &AppHandle<R>) -> Option<RemoteAdvanced> {
    hub(app)?.advanced()
}

/// Whether a phone may reach a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteDecision {
    Allow,
    Never,
}

/// One row per registered command: what a paired phone may do. Only the `Allow` rows
/// are reachable from a phone, each through its own typed call, never by name; the journal
/// calls are typed the same way (see [`PHONE_ONLY_CALLS`]).
pub const REMOTE_DECISIONS: &[(&str, RemoteDecision)] = {
    use RemoteDecision::{Allow, Never};
    &[
        ("get_status", Allow),
        ("get_setup", Never),
        ("start_setup", Never),
        ("pause_setup", Never),
        ("resume_setup", Never),
        ("retry_engine", Never),
        ("keep_text", Allow),
        ("keep_files", Never),
        ("take_quick_drop", Never),
        ("keep_recording", Allow),
        ("keep_bytes", Allow),
        ("unkeep", Allow),
        ("read_now", Never),
        ("list_items", Never),
        ("get_item", Allow),
        ("retry_item", Never),
        ("ask", Allow),
        ("cancel_ask", Allow),
        ("get_page", Allow),
        ("list_pages", Allow),
        ("answer_question", Never),
        ("add_answer_line", Never),
        ("resolve_which", Never),
        ("confirm_memory", Never),
        ("mark_done", Never),
        ("list_memories", Never),
        ("find_unread", Never),
        ("get_entity", Never),
        ("list_entities", Never),
        ("correct_memory", Never),
        ("rename_entity", Never),
        ("merge_entities", Never),
        ("refile_memory", Never),
        ("undo_change", Never),
        ("set_keep_forever", Never),
        ("set_private", Never),
        ("set_item_private", Never),
        ("set_item_keep_forever", Never),
        ("snooze", Never),
        ("restore", Never),
        ("preview_forget", Never),
        ("forget", Never),
        ("list_needs_you", Never),
        ("list_fading", Never),
        ("get_privacy", Never),
        ("export_to", Never),
        ("delete_everything", Never),
        ("make_backup", Never),
        ("probe_backup", Never),
        ("estimate_backup", Never),
        ("check_backup", Never),
        ("restore_backup", Never),
        ("restore_put_aside", Never),
        ("delete_put_aside", Never),
        ("get_backups", Never),
        ("dismiss_backup_notices", Never),
        ("get_restored", Never),
        ("finish_restore", Never),
        ("cancel_backup", Never),
        ("discard_restore", Never),
        ("get_settings", Never),
        ("update_settings", Never),
        ("pause", Never),
        ("resume", Never),
        ("get_advanced", Never),
        ("speak_page", Never),
        ("get_edition", Never),
        ("print_page", Never),
        ("open_sound_settings", Never),
        ("quick_keep_command", Never),
        ("configure_shortcut", Never),
        ("retry_shortcut", Never),
        ("app_ready", Never),
        ("report_recording", Never),
        ("tidy_anyway", Never),
        ("get_tier_offer", Never),
        ("decline_tier_offer", Never),
        ("get_unfinished_tier", Never),
        ("decline_unfinished_tier", Never),
        ("pick_folder", Never),
        ("pick_files", Never),
        ("pick_backup", Never),
        ("pick_backup_save", Never),
        ("open_original", Never),
        ("open_export", Never),
        ("open_logs", Never),
        ("open_data_folder", Never),
        ("get_remote", Never),
        ("start_remote", Never),
        ("stop_remote", Never),
        ("pause_remote_download", Never),
        ("begin_pairing", Never),
        ("decide_pairing", Never),
        ("cancel_pairing", Never),
        ("rename_device", Never),
        ("remove_device", Never),
        ("list_remote_events", Never),
        ("update_remote_settings", Never),
        ("open_remote_sign_in", Never),
        ("new_computer_key", Never),
        ("get_move", Never),
        ("answer_move", Never),
        ("finish_move", Never),
        ("stop_move", Never),
        ("get_online", Never),
        ("check_online", Never),
        ("cancel_online_check", Never),
        ("turn_on_online", Never),
        ("turn_off_online", Never),
        ("abandon_online_setup", Never),
        ("see_online_change", Allow),
        ("update_online_settings", Never),
        ("list_online_sends", Never),
        ("get_models", Never),
        ("list_model_choices", Never),
        ("preview_model_change", Never),
        ("change_model", Never),
        ("pause_model_download", Never),
        ("resume_model_download", Never),
        ("cancel_model_change", Never),
        ("cancel_reindex", Never),
        ("keep_old_model", Never),
        ("remove_old_model", Never),
        ("reset_models_to_tier", Never),
        ("get_guide", Never),
        ("get_journal_day", Allow),
        ("list_journal_days", Never),
        ("list_periods", Never),
        ("dismiss_journal_intro", Never),
        ("save_entry_text", Never),
        ("place_in_entry", Never),
        ("describe_photo", Never),
        ("read_entry_now", Never),
        ("keep_note", Never),
        ("top_feeling_words", Never),
        ("keep_check_in", Allow),
        ("mark_tried", Never),
        ("keep_good_thing", Never),
        ("set_left_out", Never),
        ("move_to_day", Never),
        ("confirm_mention", Never),
        ("remove_mention", Never),
        ("restore_mention", Never),
        ("refile_mention", Never),
        ("answer_same_name", Allow),
        ("entity_journal_days", Never),
        ("get_reflection", Never),
        ("reflect", Never),
        ("stop_reflecting", Never),
        ("delete_reflection", Never),
        ("get_phone_sources", Never),
        ("set_phone_source", Never),
        ("set_phone_page_options", Never),
        ("set_noticing", Never),
        ("list_came_in", Never),
        ("get_health_series", Never),
        ("list_readings", Never),
        ("preview_phone_delete", Never),
        ("delete_phone_data", Never),
    ]
};

/// What a phone may do that no window command does: fetch a cited original, end its session,
/// and the journal calls, which answer from the computer's own journal module. Answering the
/// page's question from the journal is its own call, not `answer_question`: a phone answers no
/// other question, and its answer is kept once per client id. A phone app also sends its
/// calendar and health readings, which no window does, introduces itself first, and offers its
/// whole memory to move here.
pub const PHONE_ONLY_CALLS: &[&str] = &[
    "get_original",
    "disconnect",
    "get_look_back",
    "look_back",
    "answer_journal_question",
    "send_calendar",
    "send_readings",
    "introduce",
    "offer_memory",
    "memory_done",
    "stop_memory",
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use nocturne_contract::{
        AppPlatform, CalendarBatch, CheckInInput, Feeling, FeelingPart, FeelingPoint, MemoryCounts,
        Period, PeriodKind, PhoneAppIntro, QuestionChoice, ReadingBatch, RemoteCall,
        SameNameAnswer,
    };

    use super::*;
    use crate::commands::COMMAND_NAMES;

    /// A phone's calls are exactly the allowed commands, the two phone verbs and the
    /// journal calls: the `cmd` tag of each call, in snake case, names one of them.
    #[test]
    fn phone_calls_are_the_allowed_commands_and_the_phone_verbs() {
        let s = String::new;
        let period = || Period {
            kind: PeriodKind::Day,
            start: s(),
            end: s(),
        };
        let calls = [
            RemoteCall::GetStatus,
            RemoteCall::KeepText {
                text: s(),
                client_id: s(),
                feeling: None,
                good: None,
                captured_at: None,
                web: false,
            },
            RemoteCall::KeepBytes {
                mime: s(),
                size: 0.0,
                client_id: s(),
                caption: None,
                feeling: None,
                good: None,
                captured_at: None,
            },
            RemoteCall::KeepRecording {
                mime: s(),
                size: 0.0,
                client_id: s(),
                feeling: None,
                good: None,
                captured_at: None,
            },
            RemoteCall::Unkeep { id: s() },
            RemoteCall::Ask {
                question: s(),
                place: None,
            },
            RemoteCall::CancelAsk { ask_id: s() },
            RemoteCall::GetPage { date: None },
            RemoteCall::ListPages,
            RemoteCall::GetItem { id: s() },
            RemoteCall::GetOriginal { id: s() },
            RemoteCall::Disconnect,
            RemoteCall::GetJournalDay { date: None },
            RemoteCall::GetLookBack { period: period() },
            RemoteCall::LookBack { period: period() },
            RemoteCall::KeepCheckIn {
                input: CheckInInput {
                    feeling: Feeling {
                        part: FeelingPart::Settled,
                        point: FeelingPoint {
                            pleasant: 0.0,
                            energy: 0.0,
                        },
                        words: Vec::new(),
                    },
                    line: None,
                    urge: None,
                    replaces: None,
                },
                client_id: s(),
                captured_at: None,
            },
            RemoteCall::AnswerSameName {
                question_id: s(),
                answer: SameNameAnswer::Leave,
            },
            RemoteCall::AnswerJournalQuestion {
                page_date: s(),
                choice: QuestionChoice::Fine,
                line: None,
                client_id: s(),
            },
            RemoteCall::SeeOnlineChange,
            RemoteCall::SendCalendar {
                batch: CalendarBatch {
                    sync_id: s(),
                    seq: 0,
                    last: true,
                    window_from: s(),
                    window_to: s(),
                    calendars: Vec::new(),
                    changes: Vec::new(),
                },
            },
            RemoteCall::SendReadings {
                batch: ReadingBatch {
                    sync_id: s(),
                    seq: 0,
                    last: true,
                    changes: Vec::new(),
                },
            },
            RemoteCall::Introduce {
                app: PhoneAppIntro {
                    platform: AppPlatform::Android,
                    version: s(),
                    min_build: 1,
                    max_build: 1,
                },
            },
            RemoteCall::OfferMemory {
                counts: MemoryCounts::default(),
                bytes: 0.0,
                schema: 1,
                since: None,
                things: None,
                bring: false,
            },
            RemoteCall::MemoryDone {
                size: 0.0,
                sha256: s(),
                passphrase: s(),
            },
            RemoteCall::StopMemory,
        ];
        // No wildcard: a new call fails to compile here until it is listed above.
        for call in &calls {
            match call {
                RemoteCall::GetStatus
                | RemoteCall::KeepText { .. }
                | RemoteCall::KeepBytes { .. }
                | RemoteCall::KeepRecording { .. }
                | RemoteCall::Unkeep { .. }
                | RemoteCall::Ask { .. }
                | RemoteCall::CancelAsk { .. }
                | RemoteCall::GetPage { .. }
                | RemoteCall::ListPages
                | RemoteCall::GetItem { .. }
                | RemoteCall::GetOriginal { .. }
                | RemoteCall::Disconnect
                | RemoteCall::GetJournalDay { .. }
                | RemoteCall::GetLookBack { .. }
                | RemoteCall::LookBack { .. }
                | RemoteCall::KeepCheckIn { .. }
                | RemoteCall::AnswerSameName { .. }
                | RemoteCall::AnswerJournalQuestion { .. }
                | RemoteCall::SeeOnlineChange
                | RemoteCall::SendCalendar { .. }
                | RemoteCall::SendReadings { .. }
                | RemoteCall::Introduce { .. }
                | RemoteCall::OfferMemory { .. }
                | RemoteCall::MemoryDone { .. }
                | RemoteCall::StopMemory => {}
            }
        }
        let snake = |camel: &str| {
            camel.chars().fold(String::new(), |mut out, c| {
                if c.is_ascii_uppercase() {
                    out.push('_');
                }
                out.push(c.to_ascii_lowercase());
                out
            })
        };
        let tags: BTreeSet<String> = calls
            .iter()
            .map(|c| snake(serde_json::to_value(c).unwrap()["cmd"].as_str().unwrap()))
            .collect();
        assert_eq!(tags.len(), calls.len());
        let expected: BTreeSet<String> = REMOTE_DECISIONS
            .iter()
            .filter(|(_, d)| *d == RemoteDecision::Allow)
            .map(|(n, _)| n.to_string())
            .chain(PHONE_ONLY_CALLS.iter().map(|n| n.to_string()))
            .collect();
        assert_eq!(tags, expected);
    }

    #[test]
    fn every_command_has_remote_decision() {
        let names: Vec<&str> = REMOTE_DECISIONS.iter().map(|(n, _)| *n).collect();
        let set: BTreeSet<&str> = names.iter().copied().collect();
        assert_eq!(set.len(), names.len(), "no duplicates");
        assert_eq!(
            set,
            COMMAND_NAMES.iter().copied().collect(),
            "one row per command"
        );
        let allowed: BTreeSet<&str> = REMOTE_DECISIONS
            .iter()
            .filter(|(_, d)| *d == RemoteDecision::Allow)
            .map(|(n, _)| *n)
            .collect();
        let allowed_calls: BTreeSet<&str> = [
            "get_status",
            "keep_text",
            "keep_bytes",
            "keep_recording",
            "unkeep",
            "ask",
            "cancel_ask",
            "get_page",
            "list_pages",
            "get_item",
            "get_journal_day",
            "keep_check_in",
            "answer_same_name",
            "see_online_change",
        ]
        .into_iter()
        .collect();
        assert_eq!(allowed, allowed_calls);
        for never in ["report_recording", "delete_everything", "get_remote"] {
            assert!(
                REMOTE_DECISIONS.contains(&(never, RemoteDecision::Never)),
                "{never}"
            );
        }
        for verb in PHONE_ONLY_CALLS {
            assert!(!names.contains(verb), "{verb} is not a window command");
        }
    }
}
