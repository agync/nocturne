//! Calling the app's commands the way the main window does.

use serde_json::{Value, json};
use tauri::WebviewWindow;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime};
use tauri::webview::InvokeRequest;

/// Commands that take their content as the raw body, with the mime type in a header.
pub const RAW: &[&str] = &["keep_recording", "keep_bytes"];

/// Commands that reach for the desktop (dialogs, the file manager, the printer, the shell) or
/// for a folder named by the caller. A browser has none of that. Away from home is never
/// driven from the preview either.
pub const UNAVAILABLE: &[&str] = &[
    "pick_folder",
    "pick_files",
    "open_original",
    "open_export",
    "open_logs",
    "open_data_folder",
    "open_sound_settings",
    "print_page",
    "quick_keep_command",
    "take_quick_drop",
    "export_to",
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

pub fn unavailable() -> Value {
    json!({ "code": "unavailable", "detail": "this is not available in a browser" })
}

fn request(cmd: &str, body: InvokeBody, headers: tauri::http::HeaderMap) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        // The app's own address, which the command permissions are checked against.
        url: if cfg!(windows) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .expect("a url"),
        body,
        headers,
        invoke_key: INVOKE_KEY.to_string(),
    }
}

fn answer(r: Result<tauri::ipc::InvokeResponseBody, Value>) -> Result<Value, Value> {
    match r {
        Ok(body) => body
            .deserialize::<Value>()
            .map_err(|_| json!({ "code": "internal", "detail": "the answer was not JSON" })),
        // A command's own error is `{code, detail}`; anything else (an argument that did not
        // parse) arrives as text.
        Err(Value::Object(o)) if o.contains_key("code") => Err(Value::Object(o)),
        Err(other) => Err(json!({
            "code": "invalid",
            "detail": other.as_str().map_or_else(|| other.to_string(), str::to_string),
        })),
    }
}

/// Call a command with JSON arguments. Blocks until it answers: run it off the async threads.
pub fn call(window: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
    let req = request(cmd, InvokeBody::Json(args), Default::default());
    answer(tauri::test::get_ipc_response(window, req))
}

/// Call a command that takes a raw body.
pub fn call_raw(
    window: &WebviewWindow<MockRuntime>,
    cmd: &str,
    bytes: Vec<u8>,
    headers: tauri::http::HeaderMap,
) -> Result<Value, Value> {
    let req = request(cmd, InvokeBody::Raw(bytes), headers);
    answer(tauri::test::get_ipc_response(window, req))
}
