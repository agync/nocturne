//! The commands the interface calls. Names are the interface's (snake_case), argument keys are
//! camelCase, errors are `CommandError`. Every body runs under [`guard`], so a failure of any
//! kind settles the promise.

use std::future::Future;

use jiff::civil::Date;
use nocturne_engine::Engine;
use tauri::{AppHandle, Manager, Runtime};

use crate::error::{CommandError, blocking, guard};
use crate::state::{AppState, refresh_status};

pub(crate) mod capture;
mod editions;
mod files;
pub use capture::{MAX_FILE_BYTES, MAX_RAW_BYTES, NOT_KEPT_NOTICE, keep_each, keep_paths};
mod forget;
mod memories;
mod needs;
mod os;
pub(crate) mod page;
mod remote;
mod settings;
mod system;

/// Every command name registered, in the order of [`invoke_handler`]. The tests call each one.
pub const COMMAND_NAMES: &[&str] = &[
    "get_status",
    "get_setup",
    "start_setup",
    "pause_setup",
    "resume_setup",
    "retry_engine",
    "keep_text",
    "keep_files",
    "take_quick_drop",
    "keep_recording",
    "keep_bytes",
    "unkeep",
    "read_now",
    "list_items",
    "get_item",
    "retry_item",
    "ask",
    "cancel_ask",
    "get_page",
    "list_pages",
    "answer_question",
    "resolve_which",
    "confirm_memory",
    "mark_done",
    "list_memories",
    "get_entity",
    "list_entities",
    "correct_memory",
    "rename_entity",
    "merge_entities",
    "refile_memory",
    "undo_change",
    "set_keep_forever",
    "set_private",
    "set_item_private",
    "set_item_keep_forever",
    "snooze",
    "restore",
    "preview_forget",
    "forget",
    "list_needs_you",
    "list_fading",
    "get_privacy",
    "export_to",
    "delete_everything",
    "get_settings",
    "update_settings",
    "pause",
    "resume",
    "get_advanced",
    "speak_page",
    "get_edition",
    "print_page",
    "open_sound_settings",
    "quick_keep_command",
    "app_ready",
    "report_recording",
    "tidy_anyway",
    "get_tier_offer",
    "decline_tier_offer",
    "get_unfinished_tier",
    "decline_unfinished_tier",
    "pick_folder",
    "pick_files",
    "open_original",
    "open_export",
    "open_logs",
    "get_remote",
    "start_remote",
    "stop_remote",
    "pause_remote_download",
    "begin_pairing",
    "decide_pairing",
    "cancel_pairing",
    "rename_device",
    "remove_device",
    "list_remote_events",
    "update_remote_settings",
    "open_remote_sign_in",
    "new_computer_key",
];

/// The handler for every command above, behind the per-window guard (see
/// [`crate::os::window_may_call`]). `configure` installs it; tests that add a command of their
/// own wrap it.
pub fn invoke_handler<R: Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static
{
    let handler = handler_of::<R>(tauri::generate_handler![
        system::get_status,
        system::get_setup,
        system::start_setup,
        system::pause_setup,
        system::resume_setup,
        system::retry_engine,
        capture::keep_text,
        capture::keep_files,
        capture::take_quick_drop,
        capture::keep_recording,
        capture::keep_bytes,
        capture::unkeep,
        capture::read_now,
        capture::list_items,
        capture::get_item,
        capture::retry_item,
        capture::ask,
        capture::cancel_ask,
        page::get_page,
        page::list_pages,
        page::answer_question,
        page::resolve_which,
        memories::confirm_memory,
        page::mark_done,
        memories::list_memories,
        memories::get_entity,
        memories::list_entities,
        memories::correct_memory,
        memories::rename_entity,
        memories::merge_entities,
        memories::refile_memory,
        memories::undo_change,
        memories::set_keep_forever,
        memories::set_private,
        memories::set_item_private,
        memories::set_item_keep_forever,
        memories::snooze,
        memories::restore,
        forget::preview_forget,
        forget::forget,
        needs::list_needs_you,
        needs::list_fading,
        settings::get_privacy,
        settings::export_to,
        settings::delete_everything,
        settings::get_settings,
        settings::update_settings,
        settings::pause,
        settings::resume,
        settings::get_advanced,
        editions::speak_page,
        editions::get_edition,
        editions::print_page,
        os::open_sound_settings,
        os::quick_keep_command,
        system::app_ready,
        system::report_recording,
        system::tidy_anyway,
        system::get_tier_offer,
        system::decline_tier_offer,
        system::get_unfinished_tier,
        system::decline_unfinished_tier,
        files::pick_folder,
        files::pick_files,
        files::open_original,
        files::open_export,
        files::open_logs,
        remote::get_remote,
        remote::start_remote,
        remote::stop_remote,
        remote::pause_remote_download,
        remote::begin_pairing,
        remote::decide_pairing,
        remote::cancel_pairing,
        remote::rename_device,
        remote::remove_device,
        remote::list_remote_events,
        remote::update_remote_settings,
        remote::open_remote_sign_in,
        remote::new_computer_key,
    ]);
    move |invoke| {
        let from = invoke.message.webview_ref().label();
        if !crate::os::window_may_call(from, invoke.message.command()) {
            invoke
                .resolver
                .reject(CommandError::invalid("not from this window"));
            return true;
        }
        handler(invoke)
    }
}

/// Fixes the runtime of the closure `generate_handler!` makes, which it cannot infer alone.
fn handler_of<R: Runtime>(
    f: impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static,
) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    f
}

/// The engine, or `unavailable`.
fn engine<R: Runtime>(app: &AppHandle<R>) -> Result<Engine, CommandError> {
    app.state::<AppState>().engine()
}

/// Run a read on the engine, off the async threads.
async fn read<R, T, F>(app: AppHandle<R>, f: F) -> Result<T, CommandError>
where
    R: Runtime,
    T: Send + 'static,
    F: FnOnce(Engine) -> Result<T, nocturne_engine::Error> + Send + 'static,
{
    guard(async move {
        let e = engine(&app)?;
        blocking(move || f(e)).await
    })
    .await
}

/// Run a change on the engine, then recompute the status (the counts may have moved).
async fn change<R, T, F>(app: AppHandle<R>, f: F) -> Result<T, CommandError>
where
    R: Runtime,
    T: Send + 'static,
    F: FnOnce(Engine) -> Result<T, nocturne_engine::Error> + Send + 'static,
{
    guard(async move {
        let e = engine(&app)?;
        let out = blocking(move || f(e)).await?;
        refresh_status(&app).await;
        Ok(out)
    })
    .await
}

/// A whole command body under the guard, for commands that do more than one engine call.
async fn run<R, T, F, Fut>(app: AppHandle<R>, f: F) -> Result<T, CommandError>
where
    R: Runtime,
    T: Send + 'static,
    F: FnOnce(AppHandle<R>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, CommandError>> + Send + 'static,
{
    guard(f(app)).await
}

fn parse_date(s: &str) -> Result<Date, CommandError> {
    s.parse::<Date>()
        .map_err(|_| CommandError::invalid("a date is YYYY-MM-DD"))
}
