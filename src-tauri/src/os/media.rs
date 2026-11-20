//! The microphone in the web view on Linux.
//!
//! WebKitGTK asks the app, not the desktop, whether a page may have the microphone, and a
//! request the app does not answer is refused. So a voice note failed with "Microphone access
//! is turned off for it" even where the microphone was on and the Flatpak carried the sound
//! socket. Tauri puts the question to [`answer`]: the app's own pages may record, and anything
//! else that ends up in a web view may not.

use tauri::webview::{PermissionKind, PermissionResponse};
use tauri::{Runtime, Webview};

/// The address a packaged build is served from on Linux.
const PACKAGED_ORIGIN: &str = "tauri://localhost";

/// The development server named in `tauri.conf.json`, which only a debug build loads from.
const DEV_ORIGIN: &str = "http://localhost:5173";

/// Whether `uri` is one of the app's own pages, matched on the origin so a look-alike host such
/// as `tauri://localhost.example` is not let through.
fn is_app_page(uri: &str) -> bool {
    let same_origin = |origin: &str| {
        uri == origin
            || uri
                .strip_prefix(origin)
                .is_some_and(|rest| matches!(rest.chars().next(), Some('/' | '?' | '#')))
    };
    same_origin(PACKAGED_ORIGIN) || (cfg!(debug_assertions) && same_origin(DEV_ORIGIN))
}

/// Answer a permission request for the whole app. The microphone opens for the app's own pages,
/// since holding Space or the record button is the consent, and is refused for any other page.
/// The camera and every other permission keep WebKit's own handling, which refuses them.
pub fn answer<R: Runtime>(webview: Webview<R>, kind: PermissionKind) -> PermissionResponse {
    if kind != PermissionKind::Microphone {
        return PermissionResponse::Default;
    }
    let uri = webview.url().map(|u| u.to_string()).unwrap_or_default();
    if is_app_page(&uri) {
        PermissionResponse::Allow
    } else {
        PermissionResponse::Deny
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_apps_own_pages_are_recognised() {
        assert!(is_app_page(PACKAGED_ORIGIN));
        assert!(is_app_page("tauri://localhost/index.html"));
        assert!(is_app_page(
            "tauri://localhost/index.html?see-through#/quick-capture"
        ));
        assert!(is_app_page("tauri://localhost#/quick-capture"));
    }

    #[test]
    fn a_look_alike_host_is_not_the_app() {
        for uri in [
            "",
            "tauri://localhost.example/",
            "tauri://localhostfoo",
            "tauri://localhost@evil.example/",
            "tauri://localhost:5173/",
            "tauri://localhost\\evil.example/",
            "https://evil.example.com",
            "asset://localhost/x.png",
        ] {
            assert!(!is_app_page(uri), "{uri}");
        }
    }

    #[test]
    fn the_development_server_is_trusted_only_in_a_debug_build() {
        assert_eq!(
            is_app_page("http://localhost:5173/"),
            cfg!(debug_assertions)
        );
        assert_eq!(
            is_app_page("http://localhost:5173/index.html#/quick-capture"),
            cfg!(debug_assertions)
        );
        assert!(!is_app_page("http://localhost:51730/"));
        assert!(!is_app_page("http://localhost:5173.example/"));
    }
}
