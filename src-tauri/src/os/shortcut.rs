//! The global quick keep shortcut.
//!
//! One default on every desktop (`CommandOrControl+Shift+Space`). It is never registered on a
//! Wayland session, where an X11 grab only fires while an X11 window has focus: there the
//! settings say it is not available and offer a desktop shortcut that runs
//! [`quick_keep_command`] instead.

use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use super::quick_capture;
use super::state::OsState;

/// What holds the shortcut on the desktop. A fake stands in for it in tests.
pub trait Registrar: Send + Sync {
    fn register(&self, shortcut: &Shortcut) -> Result<(), String>;
    fn unregister(&self, shortcut: &Shortcut) -> Result<(), String>;
}

#[derive(Default)]
pub struct ShortcutSlot {
    pub(super) registrar: Option<Arc<dyn Registrar>>,
    pub(super) current: Option<Shortcut>,
    /// How the registered shortcut was spelled.
    pub(super) spelling: Option<String>,
    /// Whether this session lets the app grab keys (set at install).
    pub(super) grab_allowed: bool,
}

/// The real thing: the global shortcut plugin, opening quick capture when pressed.
pub struct PluginRegistrar<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> Registrar for PluginRegistrar<R> {
    fn register(&self, shortcut: &Shortcut) -> Result<(), String> {
        self.0
            .global_shortcut()
            .on_shortcut(*shortcut, |app, _, event| {
                if event.state == ShortcutState::Pressed {
                    quick_capture::open(app);
                }
            })
            .map_err(|e| e.to_string())
    }

    fn unregister(&self, shortcut: &Shortcut) -> Result<(), String> {
        self.0
            .global_shortcut()
            .unregister(*shortcut)
            .map_err(|e| e.to_string())
    }
}

/// A shortcut as the interface or the engine spells it (`Ctrl+Shift+Space`,
/// `CommandOrControl+Shift+Space`, `Super+K`).
pub fn parse(spelling: &str) -> Result<Shortcut, String> {
    spelling
        .trim()
        .parse::<Shortcut>()
        .map_err(|e| e.to_string())
}

/// Whether this session lets an app grab keys everywhere. Wayland never does (a grab made
/// through XWayland only fires while an X11 window has focus), so on a Wayland session the
/// answer is no whatever else is set. `env` reads one environment variable.
pub fn session_allows_grab(env: &dyn Fn(&str) -> Option<String>) -> bool {
    if cfg!(not(target_os = "linux")) {
        return true;
    }
    let wayland_display = env("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    let wayland_session =
        env("XDG_SESSION_TYPE").is_some_and(|v| v.eq_ignore_ascii_case("wayland"));
    !(wayland_display || wayland_session)
}

pub fn session_allows_grab_now() -> bool {
    session_allows_grab(&|k| std::env::var(k).ok())
}

/// Move the registration from `old` to `new`: the new one is taken first, so a failure leaves
/// the old one in place.
pub fn apply(
    registrar: &dyn Registrar,
    old: Option<&Shortcut>,
    new: &Shortcut,
) -> Result<(), String> {
    if old == Some(new) {
        return Ok(());
    }
    registrar.register(new)?;
    if let Some(old) = old
        && let Err(e) = registrar.unregister(old)
    {
        let _ = registrar.unregister(new);
        return Err(e);
    }
    Ok(())
}

/// Register the saved shortcut at start. Says in the log why it is not registered when it is
/// not.
pub fn install(state: &OsState, spelling: &str, grab_allowed: bool) {
    let mut slot = state.shortcut();
    slot.grab_allowed = grab_allowed;
    let Some(registrar) = slot.registrar.clone() else {
        return;
    };
    if !grab_allowed {
        tracing::info!("global shortcut not registered: this session does not allow it");
        return;
    }
    let shortcut = match parse(spelling) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("global shortcut not registered: {e}");
            return;
        }
    };
    match apply(registrar.as_ref(), None, &shortcut) {
        Ok(()) => {
            tracing::info!("global shortcut registered");
            slot.current = Some(shortcut);
            slot.spelling = Some(spelling.trim().to_string());
        }
        Err(e) => tracing::warn!("global shortcut not registered: {e}"),
    }
}

/// A change of shortcut, before it is saved. Refuses one that cannot be parsed or taken, and
/// keeps the old one when it does.
pub fn change(state: &OsState, spelling: &str) -> Result<(), crate::error::CommandError> {
    use crate::error::CommandError;
    let new = parse(spelling).map_err(|_| CommandError::invalid("that shortcut is not valid"))?;
    let mut slot = state.shortcut();
    let Some(registrar) = slot.registrar.clone() else {
        return Ok(());
    };
    if !slot.grab_allowed {
        return Ok(());
    }
    apply(registrar.as_ref(), slot.current.as_ref(), &new)
        .map_err(|_| CommandError::invalid("that shortcut is already taken"))?;
    slot.current = Some(new);
    slot.spelling = Some(spelling.trim().to_string());
    Ok(())
}

/// Stop holding the shortcut (the app is exiting).
pub fn release(state: &OsState) {
    let mut slot = state.shortcut();
    slot.spelling = None;
    if let (Some(r), Some(s)) = (slot.registrar.clone(), slot.current.take()) {
        let _ = r.unregister(&s);
    }
}

/// The command a desktop shortcut runs to open quick capture: the AppImage when there is one
/// (the running executable of an AppImage is a temporary mount), else the executable.
pub fn quick_keep_command(appimage: Option<&str>, exe: &Path) -> String {
    let program = match appimage {
        Some(a) if !a.is_empty() => a.to_string(),
        _ => exe.to_string_lossy().into_owned(),
    };
    let needs_quotes = program
        .chars()
        .any(|c| c.is_whitespace() || "\"'\\$`&|;<>()*?[]{}#~!".contains(c));
    let program = if needs_quotes {
        let escaped = program
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`");
        format!("\"{escaped}\"")
    } else {
        program
    };
    format!("{program} {}", quick_capture::FLAG)
}

pub fn quick_keep_command_now() -> String {
    let exe = std::env::current_exe().unwrap_or_else(|_| "nocturne".into());
    quick_keep_command(std::env::var("APPIMAGE").ok().as_deref(), &exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn parses_ui_and_engine_spellings() {
        let a = parse("Ctrl+Shift+Space").unwrap();
        let b = parse("CommandOrControl+Shift+Space").unwrap();
        let c = parse("Control+Shift+Space").unwrap();
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert!(parse("Super+Alt+K").is_ok());
        assert!(parse("Ctrl+Shift").is_err());
        assert!(parse("").is_err());
        assert!(parse("Ctrl+Space+K").is_err());
    }

    fn env(pairs: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        let map: Vec<_> = pairs.to_vec();
        move |k| {
            map.iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wayland_session_never_grabs() {
        assert!(!session_allows_grab(&env(&[(
            "WAYLAND_DISPLAY",
            "wayland-0"
        )])));
        assert!(!session_allows_grab(&env(&[(
            "XDG_SESSION_TYPE",
            "wayland"
        )])));
        assert!(!session_allows_grab(&env(&[
            ("XDG_SESSION_TYPE", "wayland"),
            ("GDK_BACKEND", "x11"),
        ])));
        assert!(!session_allows_grab(&env(&[
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("GDK_BACKEND", "x11"),
        ])));
        assert!(session_allows_grab(&env(&[("XDG_SESSION_TYPE", "x11")])));
        assert!(session_allows_grab(&env(&[("DISPLAY", ":0")])));
        assert!(session_allows_grab(&env(&[])));
        assert!(session_allows_grab(&env(&[("WAYLAND_DISPLAY", "")])));
    }

    #[derive(Default)]
    pub struct Fake {
        pub held: Mutex<Vec<Shortcut>>,
        pub refuse: Mutex<Vec<Shortcut>>,
    }

    impl Registrar for Fake {
        fn register(&self, s: &Shortcut) -> Result<(), String> {
            if self.refuse.lock().unwrap().contains(s) {
                return Err("taken".into());
            }
            self.held.lock().unwrap().push(*s);
            Ok(())
        }

        fn unregister(&self, s: &Shortcut) -> Result<(), String> {
            self.held.lock().unwrap().retain(|h| h != s);
            Ok(())
        }
    }

    #[test]
    fn a_change_takes_the_new_one_then_drops_the_old() {
        let fake = Fake::default();
        let old = parse("Ctrl+Shift+Space").unwrap();
        let new = parse("Ctrl+Alt+K").unwrap();
        apply(&fake, None, &old).unwrap();
        apply(&fake, Some(&old), &new).unwrap();
        assert_eq!(*fake.held.lock().unwrap(), vec![new]);
        apply(&fake, Some(&new), &new).unwrap();
        assert_eq!(
            fake.held.lock().unwrap().len(),
            1,
            "same shortcut: nothing to do"
        );
    }

    #[test]
    fn a_refused_change_keeps_the_old_one() {
        let fake = Fake::default();
        let old = parse("Ctrl+Shift+Space").unwrap();
        let taken = parse("Ctrl+Alt+K").unwrap();
        apply(&fake, None, &old).unwrap();
        fake.refuse.lock().unwrap().push(taken);
        assert!(apply(&fake, Some(&old), &taken).is_err());
        assert_eq!(*fake.held.lock().unwrap(), vec![old]);
    }

    #[test]
    fn quick_keep_command_prefers_appimage() {
        let exe = Path::new("/tmp/.mount_abc/usr/bin/nocturne");
        assert_eq!(
            quick_keep_command(Some("/opt/apps/Nocturne.AppImage"), exe),
            "/opt/apps/Nocturne.AppImage --quick-capture"
        );
        assert_eq!(
            quick_keep_command(None, Path::new("/usr/bin/nocturne")),
            "/usr/bin/nocturne --quick-capture"
        );
        assert_eq!(
            quick_keep_command(Some(""), Path::new("/usr/bin/nocturne")),
            "/usr/bin/nocturne --quick-capture"
        );
        assert_eq!(
            quick_keep_command(None, Path::new("/opt/My Apps/nocturne")),
            "\"/opt/My Apps/nocturne\" --quick-capture"
        );
    }
}
