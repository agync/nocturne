//! Away from home: the person's own paired phones reach Nocturne through a sealed channel.
//!
//! Off by default: until the person turns it on, nothing is downloaded, started, bound or
//! connected. The hooks below are what the rest of the shell calls; they do nothing yet.

use std::error::Error;

use nocturne_contract::{PrivacyView, RemoteAdvanced};
use std::sync::Mutex;

use tauri::{AppHandle, Manager, Runtime};

pub mod protocol;

/// Called once the app is ready. Starts nothing unless the person turned Away from home on.
pub fn install<R: Runtime>(_app: &AppHandle<R>) -> Result<(), Box<dyn Error>> {
    Ok(())
}

/// Before the store is wiped, once the phrase is right: stop the listener and the helper, and
/// sign out where needed.
pub async fn before_delete_everything<R: Runtime>(app: &AppHandle<R>) {
    note(app, "before_delete_everything");
}

/// After the store is wiped: remove Away from home's own files.
pub fn on_delete_everything<R: Runtime>(app: &AppHandle<R>) {
    note(app, "on_delete_everything");
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
pub fn privacy<R: Runtime>(_app: &AppHandle<R>, _view: &mut PrivacyView) {}

/// Away from home's part of the advanced details, once it has been set up.
pub fn advanced<R: Runtime>(_app: &AppHandle<R>) -> Option<RemoteAdvanced> {
    None
}

/// Whether a phone may reach a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteDecision {
    Allow,
    Never,
}

/// One row per registered command: what a paired phone may do. Only the ten `Allow` rows
/// are reachable from a phone, each through its own typed call, never by name.
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
        ("resolve_which", Never),
        ("confirm_memory", Never),
        ("mark_done", Never),
        ("list_memories", Never),
        ("get_entity", Never),
        ("list_entities", Never),
        ("correct_memory", Never),
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
        ("app_ready", Never),
        ("report_recording", Never),
        ("tidy_anyway", Never),
        ("get_tier_offer", Never),
        ("decline_tier_offer", Never),
        ("get_unfinished_tier", Never),
        ("decline_unfinished_tier", Never),
        ("pick_folder", Never),
        ("pick_files", Never),
        ("open_original", Never),
        ("open_export", Never),
        ("open_logs", Never),
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
    ]
};

/// What a phone may do that no window command does: fetch a cited original, end its session.
pub const PHONE_ONLY_CALLS: &[&str] = &["get_original", "disconnect"];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use nocturne_contract::RemoteCall;

    use super::*;
    use crate::commands::COMMAND_NAMES;

    /// A phone's calls are exactly the ten allowed commands and the two phone verbs: the
    /// `cmd` tag of each call, in snake case, names one of them.
    #[test]
    fn phone_calls_are_the_allowed_commands_and_the_phone_verbs() {
        let s = String::new;
        let calls = [
            RemoteCall::GetStatus,
            RemoteCall::KeepText {
                text: s(),
                client_id: s(),
            },
            RemoteCall::KeepBytes {
                mime: s(),
                size: 0.0,
                client_id: s(),
                caption: None,
            },
            RemoteCall::KeepRecording {
                mime: s(),
                size: 0.0,
                client_id: s(),
            },
            RemoteCall::Unkeep { id: s() },
            RemoteCall::Ask { question: s() },
            RemoteCall::CancelAsk { ask_id: s() },
            RemoteCall::GetPage { date: None },
            RemoteCall::ListPages,
            RemoteCall::GetItem { id: s() },
            RemoteCall::GetOriginal { id: s() },
            RemoteCall::Disconnect,
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
                | RemoteCall::Disconnect => {}
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
        let ten: BTreeSet<&str> = [
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
        ]
        .into_iter()
        .collect();
        assert_eq!(allowed, ten);
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
