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

use nocturne_contract::{DesktopShortcut, DesktopShortcutState};

use super::quick_capture;
use super::state::OsState;

/// The shortcut a fresh install has (the engine's default).
pub const DEFAULT: &str = "CommandOrControl+Shift+Space";

/// What holds the shortcut on the desktop. A fake stands in for it in tests.
pub trait Registrar: Send + Sync {
    fn register(&self, shortcut: &Shortcut) -> Result<(), String>;
    fn unregister(&self, shortcut: &Shortcut) -> Result<(), String>;

    /// Set when the desktop, not the app, holds the shortcut.
    fn desktop(&self) -> Option<DesktopHold> {
        None
    }

    /// Open the desktop's own settings for the shortcut.
    fn configure(&self) -> Result<(), String> {
        Err("the app holds this shortcut itself".into())
    }

    /// What a refused registration means.
    fn refusal(&self) -> &'static str {
        "that shortcut is already taken"
    }
}

/// What the desktop reports about a shortcut it holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesktopHold {
    /// The keys it bound, as the desktop names them.
    pub keys: Option<String>,
    /// Whether it can open its own settings for the shortcut.
    pub configurable: bool,
    /// Whether it holds the shortcut now.
    pub held: bool,
    /// Whether the last request to hold it failed (the person said no, or it went wrong).
    pub failed: bool,
}

#[derive(Default)]
pub struct ShortcutSlot {
    pub(super) registrar: Option<Arc<dyn Registrar>>,
    pub(super) current: Option<Shortcut>,
    /// How the registered shortcut was spelled.
    pub(super) spelling: Option<String>,
    /// Whether this session lets the app grab keys (set at install).
    pub(super) grab_allowed: bool,
    /// Whether the desktop's shortcut portal is still being reached.
    pub(super) desktop_pending: bool,
    /// A shortcut chosen while the portal was still being reached, to take when it is.
    pub(super) wanted: Option<String>,
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
const PORTAL_DESCRIPTION: &str = "Keep something";

/// How long the portal has to answer at all. At login it may still be starting.
#[cfg(target_os = "linux")]
const CONNECT_LIMIT: std::time::Duration = std::time::Duration::from_secs(20);

/// The shortcut through the desktop's GlobalShortcuts portal. Each registration is a portal
/// session; a press in a session it holds runs `on_press`. The desktop may ask the person first,
/// and the question can stay open for as long as they like, so a registration only asks: the
/// answer is waited for in the background, and [`Registrar::desktop`] says where it stands.
#[cfg(target_os = "linux")]
pub struct PortalRegistrar {
    shared: Arc<PortalShared>,
    configurable: bool,
}

#[cfg(target_os = "linux")]
struct PortalShared {
    conn: zbus::Connection,
    /// The portal's unique bus name now; only its messages count.
    owner: std::sync::Mutex<zbus::names::OwnedUniqueName>,
    held: std::sync::Mutex<Vec<(Shortcut, zbus::OwnedObjectPath)>>,
    /// Shortcuts the desktop was asked for and has not answered about.
    asked: std::sync::Mutex<Vec<Asked>>,
    keys: std::sync::Mutex<Option<String>>,
    failed: std::sync::atomic::AtomicBool,
    /// The shortcuts the app wants held, whatever has become of the questions about them. Taken
    /// first, before `asked` and `held`, by everything that changes what is registered, so a
    /// restart cannot interleave with a change.
    kept: std::sync::Mutex<Vec<Shortcut>>,
    /// The portal's name went away and nothing has taken it since.
    gone: std::sync::atomic::AtomicBool,
}

#[cfg(target_os = "linux")]
impl PortalShared {
    fn held(&self) -> std::sync::MutexGuard<'_, Vec<(Shortcut, zbus::OwnedObjectPath)>> {
        self.held.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn asked(&self) -> std::sync::MutexGuard<'_, Vec<Asked>> {
        self.asked.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn kept(&self) -> std::sync::MutexGuard<'_, Vec<Shortcut>> {
        self.kept.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn owner(&self) -> std::sync::MutexGuard<'_, zbus::names::OwnedUniqueName> {
        self.owner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn is_portal(&self, sender: &str) -> bool {
        self.owner().as_str() == sender
    }

    /// Ask the desktop to hold `shortcut`, and wait for its answer in the background.
    fn ask(self: &Arc<Self>, shortcut: Shortcut) {
        use std::sync::atomic::Ordering;
        self.failed.store(false, Ordering::SeqCst);
        let token = super::portal::token();
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        self.asked().push(Asked {
            shortcut,
            token: token.clone(),
            stop,
        });
        let shared = self.clone();
        let preferred = portal_trigger(&shortcut);
        tauri::async_runtime::spawn(async move {
            let conn = &shared.conn;
            // Making the session does not ask the person, so it is waited for in full: the task
            // then knows the session and closes it itself if the shortcut was let go meanwhile.
            let session = match super::portal::create_session(conn, &token).await {
                Ok(session) => session,
                Err(e) => return shared.answered(&token, Err(e)).await,
            };
            if !matches!(
                stopped.try_recv(),
                Err(tokio::sync::oneshot::error::TryRecvError::Empty)
            ) {
                super::portal::close(conn, &session).await;
                return;
            }
            let [_, bind_token] = super::portal::bind_tokens(&token);
            let binding = super::portal::bind(
                conn,
                &session,
                PORTAL_DESCRIPTION,
                preferred.as_deref(),
                &bind_token,
            );
            let answer = tokio::select! {
                answer = binding => answer,
                // Let go while the desktop was asking: the question is withdrawn.
                _ = &mut stopped => {
                    if let Some(me) = conn.unique_name() {
                        super::portal::cancel(conn, &super::portal::request_path(me.as_str(), &bind_token)).await;
                    }
                    super::portal::close(conn, &session).await;
                    return;
                }
            };
            if answer.is_err() {
                super::portal::close(conn, &session).await;
            }
            let bound = answer.map(|trigger| super::portal::Bound { session, trigger });
            shared.answered(&token, bound).await;
        });
    }

    /// The desktop answered the question named `token`.
    async fn answered(&self, token: &str, answer: Result<super::portal::Bound, String>) {
        use std::sync::atomic::Ordering;
        // The question leaves `asked` and the session enters `held` under one lock, so
        // `unregister` finds the shortcut in one or the other.
        let wanted = {
            let mut asked = self.asked();
            let at = asked.iter().position(|a| a.token == token);
            if let Some(at) = at {
                let shortcut = asked.remove(at).shortcut;
                if let Ok(bound) = &answer {
                    match &bound.trigger {
                        Some(keys) => tracing::info!("the desktop bound the shortcut to {keys}"),
                        None => tracing::info!("the desktop holds the shortcut"),
                    }
                    *self.keys.lock().unwrap_or_else(|e| e.into_inner()) = bound.trigger.clone();
                    self.held().push((shortcut, bound.session.clone()));
                }
            }
            at.is_some()
        };
        match answer {
            // Let go while the desktop was deciding.
            Ok(bound) if !wanted => super::portal::close(&self.conn, &bound.session).await,
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("global shortcut not registered: {e}");
                if wanted {
                    self.failed.store(true, Ordering::SeqCst);
                }
            }
        }
    }

    /// The portal went away or was replaced: its sessions and open questions went with it, so
    /// every shortcut the app wants is asked for again. The call starts the portal again if it
    /// is not running.
    fn rebind(self: &Arc<Self>) {
        let kept = self.kept();
        self.held().clear();
        for open in self.asked().drain(..) {
            let _ = open.stop.send(());
        }
        *self.keys.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if !kept.is_empty() {
            tracing::info!("the shortcut portal restarted: asking for the shortcut again");
        }
        for shortcut in kept.iter() {
            self.ask(*shortcut);
        }
    }

    /// A portal took the name `owner`. When the name had gone away, [`Self::rebind`] already
    /// asked it to start, so the shortcuts are asked for again only if that did not take.
    fn took_over(self: &Arc<Self>, owner: zbus::names::OwnedUniqueName) {
        use std::sync::atomic::Ordering;
        let replaced = {
            let mut current = self.owner();
            let replaced = *current != owner;
            *current = owner;
            replaced
        };
        if self.gone.swap(false, Ordering::SeqCst) {
            let pending =
                !self.kept().is_empty() && self.held().is_empty() && self.asked().is_empty();
            if pending {
                self.rebind();
            }
        } else if replaced {
            self.rebind();
        }
    }
}

/// A question to the desktop that is still open: what for, the session token its requests are
/// named after, and how to stop waiting for it.
#[cfg(target_os = "linux")]
struct Asked {
    shortcut: Shortcut,
    token: String,
    stop: tokio::sync::oneshot::Sender<()>,
}

#[cfg(target_os = "linux")]
impl PortalRegistrar {
    /// Connect to the portal on `bus` and listen for presses, and for the portal restarting.
    /// `None` when the desktop has no GlobalShortcuts portal. Blocks for up to
    /// [`CONNECT_LIMIT`].
    pub fn connect(
        bus: &super::portal::Bus,
        on_press: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<PortalRegistrar> {
        let bus = bus.clone();
        // Listening starts before anything is bound, so no press or restart is missed.
        let (conn, version, mut presses, mut owners, portal) =
            run_bounded(CONNECT_LIMIT, async move {
                let conn = bus.connect().await.ok()?;
                let version = super::portal::global_shortcuts_version(&conn).await?;
                let presses = super::portal::activations(&conn).await.ok()?;
                let owners = super::portal::owner_changes(&conn).await.ok()?;
                let portal = super::portal::owner(&conn).await.ok()?;
                Some((conn, version, presses, owners, portal))
            })
            .flatten()?;
        let shared = Arc::new(PortalShared {
            conn,
            owner: std::sync::Mutex::new(portal),
            held: Default::default(),
            asked: Default::default(),
            keys: Default::default(),
            failed: Default::default(),
            kept: Default::default(),
            gone: Default::default(),
        });
        let listener = shared.clone();
        tauri::async_runtime::spawn(async move {
            let is_portal = |sender: &str| listener.is_portal(sender);
            while let Some(session) = super::portal::next_activation(&mut presses, &is_portal).await
            {
                if listener.held().iter().any(|(_, s)| *s == session) {
                    on_press();
                }
            }
        });
        let watcher = shared.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(owner) = super::portal::next_of(&mut owners).await {
                match owner {
                    Some(owner) => watcher.took_over(owner),
                    None => {
                        if !watcher.gone.swap(true, std::sync::atomic::Ordering::SeqCst) {
                            watcher.rebind();
                        }
                    }
                }
            }
        });
        Some(PortalRegistrar {
            shared,
            configurable: version >= 2,
        })
    }
}

#[cfg(target_os = "linux")]
impl Registrar for PortalRegistrar {
    fn register(&self, shortcut: &Shortcut) -> Result<(), String> {
        let mut kept = self.shared.kept();
        if !kept.contains(shortcut) {
            kept.push(*shortcut);
        }
        self.shared.ask(*shortcut);
        Ok(())
    }

    fn unregister(&self, shortcut: &Shortcut) -> Result<(), String> {
        // Questions still open are stopped; each task withdraws its question and closes its
        // own session, which it knows for certain.
        let (open, gone): (Vec<Asked>, Vec<_>) = {
            let mut kept = self.shared.kept();
            kept.retain(|k| k != shortcut);
            let mut asked = self.shared.asked();
            let (open, rest) = asked.drain(..).partition(|a| a.shortcut == *shortcut);
            *asked = rest;
            let mut held = self.shared.held();
            let (gone, rest) = held.drain(..).partition(|(s, _)| s == shortcut);
            *held = rest;
            (open, gone)
        };
        for a in open {
            let _ = a.stop.send(());
        }
        let conn = self.shared.conn.clone();
        run_bounded(std::time::Duration::from_secs(5), async move {
            for (_, session) in gone {
                super::portal::close(&conn, &session).await;
            }
        });
        Ok(())
    }

    fn desktop(&self) -> Option<DesktopHold> {
        Some(DesktopHold {
            keys: self
                .shared
                .keys
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            configurable: self.configurable,
            held: !self.shared.held().is_empty(),
            failed: self.shared.failed.load(std::sync::atomic::Ordering::SeqCst),
        })
    }

    fn configure(&self) -> Result<(), String> {
        if !self.configurable {
            return Err("the desktop cannot open its shortcut settings".into());
        }
        let session = self
            .shared
            .held()
            .last()
            .map(|(_, s)| s.clone())
            .ok_or("no shortcut is held")?;
        let conn = self.shared.conn.clone();
        run_bounded(std::time::Duration::from_secs(5), async move {
            super::portal::configure(&conn, &session).await
        })
        .ok_or("the desktop did not answer")?
    }

    fn refusal(&self) -> &'static str {
        "the desktop did not add the shortcut"
    }
}

/// Run `work` on a thread of its own with its own runtime, for at most `limit`. The registrar
/// is called from plain threads and from async commands alike, so it cannot borrow a runtime.
/// The connection outlives that runtime because zbus runs it on its own runtime (its `tokio`
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
    install_writing(state, spelling, grab_allowed);
}

/// The desktop's shortcut portal has been reached: it holds the shortcut from now on. A
/// shortcut chosen while it was being reached wins over `saved`, which may have been read
/// before that choice was saved.
pub fn adopt(state: &OsState, registrar: Arc<dyn Registrar>, saved: Option<&str>) {
    let _writing = state.shortcut_writer();
    let wanted = {
        let mut slot = state.shortcut();
        slot.registrar = Some(registrar);
        slot.desktop_pending = false;
        slot.wanted.take()
    };
    if let Some(spelling) = wanted.as_deref().or(saved) {
        install_writing(state, spelling, true);
    }
}

/// [`install`], for a caller that holds the writer.
fn install_writing(state: &OsState, spelling: &str, grab_allowed: bool) {
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
        let mut slot = state.shortcut();
        if slot.desktop_pending {
            // Taken once the portal is reached (see `adopt`).
            slot.wanted = Some(spelling.trim().to_string());
            return Ok(());
        }
        let Some(registrar) = slot.registrar.clone() else {
            return Ok(());
        };
        if !slot.grab_allowed {
            return Ok(());
        }
        (registrar, slot.current)
    };
    apply(registrar.as_ref(), current.as_ref(), &new)
        .map_err(|_| CommandError::invalid(registrar.refusal()))?;
    let mut slot = state.shortcut();
    slot.current = Some(new);
    slot.spelling = Some(spelling.trim().to_string());
    Ok(())
}

/// Whether the desktop's shortcut portal is still being reached at start.
pub fn set_desktop_pending(state: &OsState, pending: bool) {
    let mut slot = state.shortcut();
    slot.desktop_pending = pending;
    if !pending {
        slot.wanted = None;
    }
}

/// Where the desktop holds the shortcut, what the settings show about it.
pub fn desktop_view(state: &OsState) -> Option<DesktopShortcut> {
    let slot = state.shortcut();
    if slot.desktop_pending {
        return Some(DesktopShortcut {
            state: DesktopShortcutState::Asking,
            keys: None,
            configurable: false,
        });
    }
    let hold = slot.registrar.as_ref()?.desktop()?;
    let shown = if hold.held {
        DesktopShortcutState::Held
    } else if hold.failed {
        DesktopShortcutState::Declined
    } else {
        DesktopShortcutState::Asking
    };
    Some(DesktopShortcut {
        state: shown,
        keys: if hold.held { hold.keys } else { None },
        configurable: hold.configurable,
    })
}

/// The keys to name for the shortcut while it is held: the desktop's own name for them where the
/// desktop holds it, else the saved spelling.
pub fn held_keys(state: &OsState) -> Option<String> {
    let slot = state.shortcut();
    slot.current?;
    match slot.registrar.as_ref().and_then(|r| r.desktop()) {
        Some(hold) if hold.held => hold.keys.or_else(|| slot.spelling.clone()),
        Some(_) => None,
        None => slot.spelling.clone(),
    }
}

/// Ask the desktop again for a shortcut it did not add. Nothing to do where it is held, or where
/// the app holds its own.
pub fn retry(state: &OsState, spelling: &str) {
    let writing = state.shortcut_writer();
    let (registrar, current) = {
        let slot = state.shortcut();
        (slot.registrar.clone(), slot.current)
    };
    let Some(registrar) = registrar else {
        return;
    };
    if !registrar.desktop().is_some_and(|d| d.failed) {
        return;
    }
    match current {
        Some(shortcut) => {
            let _ = registrar.register(&shortcut);
        }
        None => {
            // `install` takes the writer itself.
            drop(writing);
            install(state, spelling, true);
        }
    }
}

/// Open the desktop's own settings for the shortcut it holds.
pub fn configure(state: &OsState) -> Result<(), crate::error::CommandError> {
    let registrar = state.shortcut().registrar.clone();
    registrar
        .ok_or_else(|| crate::error::CommandError::unavailable("no shortcut is held"))?
        .configure()
        .map_err(|e| {
            tracing::warn!("the desktop's shortcut settings did not open: {e}");
            crate::error::CommandError::unavailable("the desktop's shortcut settings did not open")
        })
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
    fn the_desktop_names_the_shortcut_as_the_box_it_opens() {
        let copy: serde_json::Value =
            serde_json::from_str(include_str!("../../../ui/src/copy/en-GB/shell.json")).unwrap();
        assert_eq!(copy["screen.capture.title"], PORTAL_DESCRIPTION);
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

    /// Wait up to five seconds for `done`.
    #[cfg(target_os = "linux")]
    fn eventually(what: &str, done: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !done() {
            assert!(std::time::Instant::now() < deadline, "never: {what}");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[cfg(target_os = "linux")]
    fn portal_state(
        code: u32,
        delay: std::time::Duration,
        on_press: Arc<dyn Fn() + Send + Sync>,
    ) -> (
        crate::os::portal::testbus::TestBus,
        crate::os::portal::tests::Fake,
        Arc<OsState>,
        tempfile::TempDir,
    ) {
        use crate::os::portal::{Bus, testbus::TestBus, tests as fake};
        let bus = TestBus::start();
        let fake = tauri::async_runtime::block_on(fake::shortcuts_portal(&bus, code, delay));
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(OsState::new(dir.path()));
        let portal = PortalRegistrar::connect(&Bus::at(&bus.address), on_press).unwrap();
        state.set_registrar(Arc::new(portal));
        (bus, fake, state, dir)
    }

    #[cfg(target_os = "linux")]
    fn shown(state: &OsState) -> Option<DesktopShortcutState> {
        desktop_view(state).map(|d| d.state)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_portal_holds_the_shortcut_and_a_press_opens_quick_keep() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let pressed = Arc::new(AtomicUsize::new(0));
        let counter = pressed.clone();
        let (bus, fake, state, _dir) = portal_state(
            0,
            std::time::Duration::ZERO,
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        );
        install(&state, DEFAULT, true);
        eventually("bound", || {
            shown(&state) == Some(DesktopShortcutState::Held)
        });
        change(&state, "Ctrl+Alt+K").unwrap();
        eventually("bound again", || fake.binds.lock().unwrap().len() == 2);
        let sessions: Vec<_> = fake.binds.lock().unwrap().clone();
        assert_eq!(
            sessions
                .iter()
                .map(|(_, k)| k.as_deref())
                .collect::<Vec<_>>(),
            [Some("CTRL+SHIFT+space"), Some("CTRL+ALT+k")]
        );
        eventually("the first session let go", || {
            *fake.closed.lock().unwrap() == vec![sessions[0].0.clone()]
        });
        eventually("the second held", || {
            shown(&state) == Some(DesktopShortcutState::Held)
        });

        let me = fake.client_of(&sessions[1].0);
        tauri::async_runtime::block_on(async {
            // Someone else on the bus cannot press it for the portal.
            let impostor = crate::os::portal::Bus::at(&bus.address)
                .connect()
                .await
                .unwrap();
            crate::os::portal::tests::press(&impostor, &me, &sessions[1].0).await;
            crate::os::portal::tests::press(&fake.server, &me, &sessions[0].0).await;
            crate::os::portal::tests::press(&fake.server, &me, &sessions[1].0).await;
        });
        eventually("a press", || pressed.load(Ordering::SeqCst) > 0);
        // Presses are heard in order: a second real one counted means every earlier one has
        // been heard too, so the count shows which of them opened quick keep.
        tauri::async_runtime::block_on(crate::os::portal::tests::press(
            &fake.server,
            &me,
            &sessions[1].0,
        ));
        eventually("a second press", || pressed.load(Ordering::SeqCst) > 1);
        assert_eq!(
            pressed.load(Ordering::SeqCst),
            2,
            "only the session still held opens quick keep, and only for the portal"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_settings_show_the_keys_the_desktop_bound() {
        let (_bus, fake, state, _dir) = portal_state(0, std::time::Duration::ZERO, Arc::new(|| {}));
        set_desktop_pending(&state, true);
        assert_eq!(
            shown(&state),
            Some(DesktopShortcutState::Asking),
            "still reaching the portal"
        );
        set_desktop_pending(&state, false);
        install(&state, DEFAULT, true);
        eventually("held", || shown(&state) == Some(DesktopShortcutState::Held));
        assert_eq!(
            desktop_view(&state),
            Some(DesktopShortcut {
                state: DesktopShortcutState::Held,
                keys: Some("Ctrl+Shift+Space".into()),
                configurable: true,
            })
        );
        assert_eq!(held_keys(&state).as_deref(), Some("Ctrl+Shift+Space"));
        configure(&state).unwrap();
        assert_eq!(fake.configured.lock().unwrap().len(), 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_shortcut_the_person_declined_can_be_asked_for_again() {
        let (_bus, fake, state, _dir) = portal_state(1, std::time::Duration::ZERO, Arc::new(|| {}));
        install(&state, DEFAULT, true);
        eventually("declined", || {
            shown(&state) == Some(DesktopShortcutState::Declined)
        });
        assert_eq!(
            desktop_view(&state).and_then(|d| d.keys),
            None,
            "no keys when nothing is held"
        );
        assert_eq!(held_keys(&state), None);
        retry(&state, DEFAULT);
        eventually("asked again", || fake.binds.lock().unwrap().len() == 2);
        eventually("declined again", || {
            shown(&state) == Some(DesktopShortcutState::Declined)
        });
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_question_left_open_is_waited_for() {
        let (_bus, fake, state, _dir) =
            portal_state(0, std::time::Duration::from_secs(1), Arc::new(|| {}));
        install(&state, DEFAULT, true);
        assert_eq!(
            shown(&state),
            Some(DesktopShortcutState::Asking),
            "settings can be read while the desktop asks"
        );
        assert_eq!(held_keys(&state), None);
        eventually("held once answered", || {
            shown(&state) == Some(DesktopShortcutState::Held)
        });
        assert!(fake.closed.lock().unwrap().is_empty(), "nothing let go");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_shortcut_let_go_while_the_desktop_asks_withdraws_the_question() {
        let (_bus, fake, state, _dir) =
            portal_state(0, std::time::Duration::from_millis(500), Arc::new(|| {}));
        install(&state, DEFAULT, true);
        eventually("the desktop asking", || {
            fake.arrived.lock().unwrap().len() == 1
        });
        release(&state);
        let session = fake.created.lock().unwrap()[0].clone();
        eventually("the session let go", || {
            *fake.closed.lock().unwrap() == vec![session.clone()]
        });
        eventually("the desktop's late answer", || {
            fake.answered.lock().unwrap().len() == 1
        });
        assert_eq!(held_keys(&state), None);
        assert_eq!(
            desktop_view(&state).map(|d| d.state),
            Some(DesktopShortcutState::Asking),
            "the late answer is not taken"
        );
        assert_eq!(fake.closed.lock().unwrap().len(), 1);
        assert_eq!(
            *fake.withdrawn.lock().unwrap(),
            *fake.answered.lock().unwrap(),
            "the question the desktop was asking is the one withdrawn"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_shortcut_let_go_at_once_leaves_no_session_open() {
        let (_bus, fake, state, _dir) = portal_state(0, std::time::Duration::ZERO, Arc::new(|| {}));
        install(&state, DEFAULT, true);
        release(&state);
        eventually("every session made is closed", || {
            let created = fake.created.lock().unwrap().clone();
            let closed = fake.closed.lock().unwrap().clone();
            created.len() == 1 && closed == created
        });
        assert_eq!(shown(&state), Some(DesktopShortcutState::Asking));
        assert_eq!(held_keys(&state), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_restarted_portal_is_asked_again() {
        use crate::os::portal::tests as fake;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let pressed = Arc::new(AtomicUsize::new(0));
        let counter = pressed.clone();
        let (bus, first, state, _dir) = portal_state(
            0,
            std::time::Duration::ZERO,
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        );
        install(&state, DEFAULT, true);
        eventually("held", || shown(&state) == Some(DesktopShortcutState::Held));
        let me = first.client_of(&first.created.lock().unwrap()[0]);
        // A new portal takes the name over; the old one's sessions are gone.
        let second = tauri::async_runtime::block_on(fake::shortcuts_portal(
            &bus,
            0,
            std::time::Duration::ZERO,
        ));
        eventually("asked again", || second.binds.lock().unwrap().len() == 1);
        eventually("held again", || {
            shown(&state) == Some(DesktopShortcutState::Held)
        });
        let session = second.created.lock().unwrap()[0].clone();
        tauri::async_runtime::block_on(async {
            // The old portal's name is no longer the portal's.
            fake::press(&first.server, &me, &session).await;
            fake::press(&second.server, &me, &session).await;
        });
        eventually("a press", || pressed.load(Ordering::SeqCst) > 0);
        tauri::async_runtime::block_on(fake::press(&second.server, &me, &session));
        eventually("a second press", || pressed.load(Ordering::SeqCst) > 1);
        assert_eq!(pressed.load(Ordering::SeqCst), 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_portal_that_crashes_and_comes_back_is_asked_once() {
        use crate::os::portal::tests as fake;
        let (bus, first, state, _dir) = portal_state(0, std::time::Duration::ZERO, Arc::new(|| {}));
        install(&state, DEFAULT, true);
        eventually("held", || shown(&state) == Some(DesktopShortcutState::Held));
        // The portal stops: the name goes, and asking again finds nothing to start.
        tauri::async_runtime::block_on(first.server.graceful_shutdown());
        eventually("the loss is seen", || {
            shown(&state) != Some(DesktopShortcutState::Held)
        });
        let second = tauri::async_runtime::block_on(fake::shortcuts_portal(
            &bus,
            0,
            std::time::Duration::ZERO,
        ));
        eventually("held again", || {
            shown(&state) == Some(DesktopShortcutState::Held)
        });
        // Anything a second ask would make has had time to arrive.
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert_eq!(second.created.lock().unwrap().len(), 1);
        assert_eq!(second.binds.lock().unwrap().len(), 1);
        assert!(second.closed.lock().unwrap().is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_shortcut_chosen_while_the_portal_is_reached_is_the_one_bound() {
        use crate::os::portal::{Bus, testbus::TestBus, tests as fake};
        let bus = TestBus::start();
        let portal = tauri::async_runtime::block_on(fake::shortcuts_portal(
            &bus,
            0,
            std::time::Duration::ZERO,
        ));
        let dir = tempfile::tempdir().unwrap();
        let state = OsState::new(dir.path());
        install(&state, DEFAULT, false);
        set_desktop_pending(&state, true);
        change(&state, "Ctrl+Alt+K").unwrap();
        let registrar = PortalRegistrar::connect(&Bus::at(&bus.address), Arc::new(|| {})).unwrap();
        // The saved settings were read before the change was saved.
        adopt(&state, Arc::new(registrar), Some(DEFAULT));
        eventually("bound", || portal.binds.lock().unwrap().len() == 1);
        assert_eq!(
            portal.binds.lock().unwrap()[0].1.as_deref(),
            Some("CTRL+ALT+k")
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
