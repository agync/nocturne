//! The global quick keep shortcut.
//!
//! One default on every desktop (`CommandOrControl+Shift+Space`). On a Wayland session an X11
//! grab only fires while an X11 window has focus, so there the shortcut goes through the desktop's
//! GlobalShortcuts portal ([`PortalRegistrar`]), which may ask the person first and may bind
//! other keys. Where the desktop has no such portal the settings say the shortcut is not
//! available and offer a desktop shortcut that runs [`quick_keep_command`] instead.

use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use super::quick_capture;
use super::state::OsState;

/// The shortcut a fresh install has (the engine's default).
pub const DEFAULT: &str = "CommandOrControl+Shift+Space";

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

/// What the desktop lists the shortcut as, in the portal's own dialog and settings.
#[cfg(target_os = "linux")]
const PORTAL_DESCRIPTION: &str = "Quick keep shortcut";

/// How long the portal has to answer at all. At login it may still be starting.
#[cfg(target_os = "linux")]
const CONNECT_LIMIT: std::time::Duration = std::time::Duration::from_secs(20);

/// The shortcut through the desktop's GlobalShortcuts portal. Each registration is a portal
/// session; a press in a session it holds runs `on_press`.
#[cfg(target_os = "linux")]
pub struct PortalRegistrar {
    conn: zbus::Connection,
    held: Arc<std::sync::Mutex<Vec<(Shortcut, zbus::zvariant::OwnedObjectPath)>>>,
}

#[cfg(target_os = "linux")]
impl PortalRegistrar {
    /// Connect to the portal on `bus` and listen for presses. `None` when the desktop has no
    /// GlobalShortcuts portal. Blocks for up to [`CONNECT_LIMIT`].
    pub fn connect(
        bus: &super::portal::Bus,
        on_press: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<PortalRegistrar> {
        let bus = bus.clone();
        // Listening starts before anything is bound, so no press is missed.
        let (conn, mut presses) = run_bounded(CONNECT_LIMIT, async move {
            let conn = bus.connect().await.ok()?;
            if !super::portal::has_global_shortcuts(&conn).await {
                return None;
            }
            let presses = super::portal::activations(&conn).await.ok()?;
            Some((conn, presses))
        })
        .flatten()?;
        let held: Arc<std::sync::Mutex<Vec<(Shortcut, zbus::zvariant::OwnedObjectPath)>>> =
            Arc::default();
        let sessions = held.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(session) = super::portal::next_activation(&mut presses).await {
                let ours = sessions
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .iter()
                    .any(|(_, s)| *s == session);
                if ours {
                    on_press();
                }
            }
        });
        Some(PortalRegistrar { conn, held })
    }
}

#[cfg(target_os = "linux")]
impl Registrar for PortalRegistrar {
    fn register(&self, shortcut: &Shortcut) -> Result<(), String> {
        let conn = self.conn.clone();
        let preferred = portal_trigger(shortcut);
        let bound = run_bounded(super::portal::ANSWER_LIMIT, async move {
            super::portal::bind_quick_keep(&conn, PORTAL_DESCRIPTION, preferred.as_deref()).await
        })
        .ok_or("the desktop did not answer")??;
        if let Some(keys) = &bound.trigger {
            tracing::info!("the desktop bound the shortcut to {keys}");
        }
        self.held
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((*shortcut, bound.session));
        Ok(())
    }

    fn unregister(&self, shortcut: &Shortcut) -> Result<(), String> {
        let gone: Vec<_> = {
            let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
            let (gone, kept) = held.drain(..).partition(|(s, _)| s == shortcut);
            *held = kept;
            gone
        };
        let conn = self.conn.clone();
        run_bounded(std::time::Duration::from_secs(5), async move {
            for (_, session) in gone {
                super::portal::close(&conn, &session).await;
            }
        });
        Ok(())
    }
}

/// Run `work` on a thread of its own with its own runtime, for at most `limit`. The registrar
/// is called from plain threads and from async commands alike, so it cannot borrow a runtime.
/// The connection outlives that runtime because zbus runs on its own executor (its `tokio`
/// feature is off).
#[cfg(target_os = "linux")]
fn run_bounded<T: Send + 'static>(
    limit: std::time::Duration,
    work: impl std::future::Future<Output = T> + Send + 'static,
) -> Option<T> {
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .ok()?
            .block_on(async { tokio::time::timeout(limit, work).await.ok() })
    })
    .join()
    .ok()
    .flatten()
}

/// A shortcut as the portal spells it (`CTRL+SHIFT+space`): modifiers in capitals, then the
/// key's keysym name. `None` for a key with no name here; the desktop then asks for keys itself.
#[cfg(target_os = "linux")]
pub fn portal_trigger(shortcut: &Shortcut) -> Option<String> {
    use tauri_plugin_global_shortcut::Modifiers;
    let mut parts: Vec<String> = [
        (Modifiers::CONTROL, "CTRL"),
        (Modifiers::ALT, "ALT"),
        (Modifiers::SHIFT, "SHIFT"),
        (Modifiers::SUPER, "LOGO"),
    ]
    .into_iter()
    .filter(|(m, _)| shortcut.mods.contains(*m))
    .map(|(_, name)| name.to_string())
    .collect();
    let code = format!("{:?}", shortcut.key);
    let key = if let Some(letter) = code.strip_prefix("Key") {
        letter.to_lowercase()
    } else if let Some(digit) = code.strip_prefix("Digit") {
        digit.to_string()
    } else if code.len() > 1 && code.starts_with('F') && code[1..].parse::<u8>().is_ok() {
        code
    } else {
        match code.as_str() {
            "Space" => "space",
            "Enter" => "Return",
            "Tab" => "Tab",
            "Backspace" => "BackSpace",
            "Delete" => "Delete",
            "Home" => "Home",
            "End" => "End",
            "PageUp" => "Page_Up",
            "PageDown" => "Page_Down",
            "ArrowUp" => "Up",
            "ArrowDown" => "Down",
            "ArrowLeft" => "Left",
            "ArrowRight" => "Right",
            _ => return None,
        }
        .to_string()
    };
    parts.push(key);
    Some(parts.join("+"))
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
    // The slot is not held while registering: the portal may wait for the person, and reading
    // the settings must not wait with it. A change waits for the writer instead.
    let _writing = state.shortcut_writer();
    let registrar = {
        let mut slot = state.shortcut();
        slot.grab_allowed = grab_allowed;
        slot.registrar.clone()
    };
    let Some(registrar) = registrar else {
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
            let mut slot = state.shortcut();
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
    let _writing = state.shortcut_writer();
    let (registrar, current) = {
        let slot = state.shortcut();
        let Some(registrar) = slot.registrar.clone() else {
            return Ok(());
        };
        if !slot.grab_allowed {
            return Ok(());
        }
        (registrar, slot.current)
    };
    apply(registrar.as_ref(), current.as_ref(), &new)
        .map_err(|_| CommandError::invalid("that shortcut is already taken"))?;
    let mut slot = state.shortcut();
    slot.current = Some(new);
    slot.spelling = Some(spelling.trim().to_string());
    Ok(())
}

/// Stop holding the shortcut (the app is exiting).
pub fn release(state: &OsState) {
    let held = {
        let mut slot = state.shortcut();
        slot.spelling = None;
        (slot.registrar.clone(), slot.current.take())
    };
    if let (Some(r), Some(s)) = held {
        let _ = r.unregister(&s);
    }
}

/// The command a desktop shortcut runs to open quick capture: `flatpak run` with the app id
/// inside a Flatpak (the executable's path exists only in the sandbox), the AppImage when there
/// is one (the running executable of an AppImage is a temporary mount), else the executable.
pub fn quick_keep_command(flatpak: Option<&str>, appimage: Option<&str>, exe: &Path) -> String {
    if let Some(id) = flatpak {
        return format!("flatpak run {id} {}", quick_capture::FLAG);
    }
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
    quick_keep_command(
        super::sandbox::flatpak_app_id_now(),
        std::env::var("APPIMAGE").ok().as_deref(),
        &exe,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn parses_ui_and_engine_spellings() {
        // CommandOrControl is Command on a Mac and Control elsewhere.
        let primary = if cfg!(target_os = "macos") {
            "Command"
        } else {
            "Ctrl"
        };
        let a = parse(&format!("{primary}+Shift+Space")).unwrap();
        let b = parse("CommandOrControl+Shift+Space").unwrap();
        let c = parse(&format!("{primary}+Shift+Space").replace("Ctrl", "Control")).unwrap();
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert_eq!(parse("Ctrl+Shift+Space"), parse("Control+Shift+Space"));
        assert!(parse("Super+Alt+K").is_ok());
        assert!(parse("Ctrl+Shift").is_err());
        assert!(parse("").is_err());
        assert!(parse("Ctrl+Space+K").is_err());
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
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

    #[cfg(target_os = "linux")]
    #[test]
    fn the_desktop_is_given_the_settings_label() {
        let copy: serde_json::Value =
            serde_json::from_str(include_str!("../../../ui/src/copy/en-GB/settings.json")).unwrap();
        assert_eq!(copy["settings.shortcut.label"], PORTAL_DESCRIPTION);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_portal_spells_a_shortcut_its_own_way() {
        let spell = |s: &str| portal_trigger(&parse(s).unwrap());
        assert_eq!(
            spell("CommandOrControl+Shift+Space").as_deref(),
            Some("CTRL+SHIFT+space")
        );
        assert_eq!(spell("Ctrl+Alt+K").as_deref(), Some("CTRL+ALT+k"));
        assert_eq!(spell("Super+Shift+7").as_deref(), Some("SHIFT+LOGO+7"));
        assert_eq!(spell("Alt+F12").as_deref(), Some("ALT+F12"));
        assert_eq!(spell("Ctrl+ArrowUp").as_deref(), Some("CTRL+Up"));
        assert_eq!(spell("Ctrl+Enter").as_deref(), Some("CTRL+Return"));
        assert_eq!(spell("Ctrl+PageDown").as_deref(), Some("CTRL+Page_Down"));
        assert_eq!(spell("Ctrl+Backquote"), None, "no name: the desktop asks");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_portal_holds_the_shortcut_and_a_press_opens_quick_keep() {
        use crate::os::portal::{Bus, testbus::TestBus, tests as fake};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::{Duration, Instant};

        let bus = TestBus::start();
        let (server, binds, closed) =
            tauri::async_runtime::block_on(fake::shortcuts_portal(&bus, 0));
        let pressed = Arc::new(AtomicUsize::new(0));
        let counter = pressed.clone();
        let portal = PortalRegistrar::connect(
            &Bus::at(&bus.address),
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .expect("the portal is there");
        let first = parse("CommandOrControl+Shift+Space").unwrap();
        let second = parse("Ctrl+Alt+K").unwrap();
        apply(&portal, None, &first).unwrap();
        apply(&portal, Some(&first), &second).unwrap();
        let sessions: Vec<_> = binds.lock().unwrap().clone();
        assert_eq!(
            sessions
                .iter()
                .map(|(_, k)| k.as_deref())
                .collect::<Vec<_>>(),
            [Some("CTRL+SHIFT+space"), Some("CTRL+ALT+k")]
        );
        assert_eq!(
            *closed.lock().unwrap(),
            vec![sessions[0].0.clone()],
            "the old session is let go after the new one is bound"
        );

        let me = portal.conn.unique_name().unwrap().to_string();
        tauri::async_runtime::block_on(async {
            fake::press(&server, &me, &sessions[0].0).await;
            fake::press(&server, &me, &sessions[1].0).await;
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while pressed.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            pressed.load(Ordering::SeqCst),
            1,
            "only the session still held opens quick keep"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn without_the_portal_there_is_no_portal_registrar() {
        let bus = crate::os::portal::testbus::TestBus::start();
        assert!(
            PortalRegistrar::connect(&crate::os::portal::Bus::at(&bus.address), Arc::new(|| {}))
                .is_none()
        );
    }

    /// Holds each registration until the test lets it go, as a desktop's question does.
    struct Slow {
        started: std::sync::mpsc::SyncSender<()>,
        go: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl Registrar for Slow {
        fn register(&self, _: &Shortcut) -> Result<(), String> {
            let _ = self.started.send(());
            let _ = self.go.lock().unwrap().recv();
            Ok(())
        }

        fn unregister(&self, _: &Shortcut) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn reading_the_shortcut_does_not_wait_for_a_change() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(OsState::new(dir.path()));
        let (started, on_start) = std::sync::mpsc::sync_channel(2);
        let (release, go) = std::sync::mpsc::channel();
        state.set_registrar(Arc::new(Slow {
            started,
            go: Mutex::new(go),
        }));
        release.send(()).unwrap();
        install(&state, "Ctrl+Shift+Space", true);
        on_start.recv().unwrap();
        assert!(state.shortcut_registered());

        let changing = state.clone();
        let change = std::thread::spawn(move || change(&changing, "Ctrl+Alt+K"));
        on_start.recv().unwrap();
        assert!(state.shortcut_registered(), "read while the change waits");
        release.send(()).unwrap();
        change.join().unwrap().unwrap();
        assert_eq!(state.shortcut().spelling.as_deref(), Some("Ctrl+Alt+K"));
    }

    #[test]
    fn quick_keep_command_prefers_appimage() {
        let exe = Path::new("/tmp/.mount_abc/usr/bin/nocturne");
        assert_eq!(
            quick_keep_command(None, Some("/opt/apps/Nocturne.AppImage"), exe),
            "/opt/apps/Nocturne.AppImage --quick-capture"
        );
        assert_eq!(
            quick_keep_command(None, None, Path::new("/usr/bin/nocturne")),
            "/usr/bin/nocturne --quick-capture"
        );
        assert_eq!(
            quick_keep_command(None, Some(""), Path::new("/usr/bin/nocturne")),
            "/usr/bin/nocturne --quick-capture"
        );
        assert_eq!(
            quick_keep_command(None, None, Path::new("/opt/My Apps/nocturne")),
            "\"/opt/My Apps/nocturne\" --quick-capture"
        );
    }

    #[test]
    fn inside_a_flatpak_the_desktop_runs_the_app_through_flatpak() {
        assert_eq!(
            quick_keep_command(
                Some("is.nocturne.Nocturne"),
                None,
                Path::new("/app/bin/nocturne")
            ),
            "flatpak run is.nocturne.Nocturne --quick-capture"
        );
        assert_eq!(
            quick_keep_command(
                Some("is.nocturne.Nocturne"),
                Some("/opt/apps/Nocturne.AppImage"),
                Path::new("/app/bin/nocturne")
            ),
            "flatpak run is.nocturne.Nocturne --quick-capture"
        );
    }
}
