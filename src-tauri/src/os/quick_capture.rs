//! The quick capture window: a small always-on-top box that keeps one thing and goes away.
//!
//! It is made when asked for and destroyed when closed, so every open starts clean. The window
//! may call only the commands in `window_may_call`, and its capability file grants nothing else.

use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

pub const LABEL: &str = "quick-capture";
const ROUTE: &str = "index.html#/quick-capture";
/// The same page when the window can be see-through: the interface then draws only the card.
const ROUTE_SEE_THROUGH: &str = "index.html?see-through#/quick-capture";
const WIDTH: f64 = 560.0;
const HEIGHT: f64 = 240.0;
/// The second-launch flag that opens this window (a desktop shortcut runs it).
pub const FLAG: &str = "--quick-capture";

/// Whether a second launch asked for quick capture.
pub fn wants_quick_capture(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| a == FLAG)
}

/// The top left corner that centres a window of `window` physical pixels on a monitor.
pub fn centred(
    monitor_pos: (i32, i32),
    monitor_size: (u32, u32),
    window: (u32, u32),
) -> (i32, i32) {
    let x = monitor_pos.0 + (i64::from(monitor_size.0) - i64::from(window.0)) as i32 / 2;
    let y = monitor_pos.1 + (i64::from(monitor_size.1) - i64::from(window.1)) as i32 / 2;
    (x, y)
}

/// Open quick capture, or bring it forward if it is already open. The window is made from an
/// async task: building a window from a command thread can deadlock on Windows.
pub fn open<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let shown = make(&app).and_then(|fresh| match fresh {
            Some(window) => {
                place(&app, &window);
                show(&window)
            }
            None => Ok(()),
        });
        if let Err(e) = shown {
            tracing::warn!("quick capture did not open: {e}");
        }
    });
}

/// Make the window hidden, or focus the one that exists (and return `None`).
fn make<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Option<WebviewWindow<R>>> {
    if let Some(w) = app.get_webview_window(LABEL) {
        show(&w)?;
        return Ok(None);
    }
    let see_through = can_see_through();
    let route = if see_through {
        ROUTE_SEE_THROUGH
    } else {
        ROUTE
    };
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(route.into()))
        .title("Nocturne")
        .inner_size(WIDTH, HEIGHT)
        // Linux fits the window to the card as it grows and shrinks.
        .resizable(cfg!(target_os = "linux"))
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        // Only the card shows where the window can be see-through: the page around it is clear
        // and the card draws its own shadow. Elsewhere the window is opaque and fitted.
        .transparent(see_through)
        .shadow(!see_through)
        .visible(false)
        .build()?;
    Ok(Some(window))
}

/// Whether a see-through window is drawn right here. WebKitGTK repaints only the damaged part of
/// a transparent window, so once the text field is focused the rest of the card vanishes. Linux
/// keeps an opaque window, which the page fits to its card instead.
const fn can_see_through() -> bool {
    !cfg!(target_os = "linux")
}

fn show<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    let _ = window.unminimize();
    window.show()?;
    window.set_focus()
}

/// Make and show the window where it is, or focus the one that exists. Placement needs a real
/// monitor list, which the test runtime has not got, so tests use this.
pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    match make(app)? {
        Some(window) => show(&window),
        None => Ok(()),
    }
}

/// Centre the window on the monitor the pointer is on, falling back to the primary one.
fn place<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(m) = monitor else {
        let _ = window.center();
        return;
    };
    let scale = m.scale_factor();
    let size: PhysicalSize<u32> = LogicalSize::new(WIDTH, HEIGHT).to_physical(scale);
    let at = centred(
        (m.position().x, m.position().y),
        (m.size().width, m.size().height),
        (size.width, size.height),
    );
    let _ = window.set_position(PhysicalPosition::new(at.0, at.1));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_flag_is_an_argument_not_the_program_name() {
        assert!(wants_quick_capture(&args(&["nocturne", "--quick-capture"])));
        assert!(wants_quick_capture(&args(&[
            "nocturne",
            "/a/file.txt",
            "--quick-capture"
        ])));
        assert!(!wants_quick_capture(&args(&["--quick-capture"])));
        assert!(!wants_quick_capture(&args(&["nocturne", "--hidden"])));
    }

    #[test]
    fn the_see_through_route_carries_the_flag_the_page_reads() {
        assert!(ROUTE_SEE_THROUGH.contains("?see-through#"));
        assert!(!ROUTE.contains("see-through"));
    }

    #[test]
    fn the_window_is_centred_on_its_monitor() {
        assert_eq!(centred((0, 0), (1920, 1080), (560, 240)), (680, 420));
        assert_eq!(
            centred((1920, 0), (1920, 1080), (560, 240)),
            (2600, 420),
            "a second monitor to the right"
        );
        assert_eq!(centred((-1280, 0), (1280, 720), (560, 240)), (-920, 240));
    }
}
