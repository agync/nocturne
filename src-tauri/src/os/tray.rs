//! The tray: a small icon that says what Nocturne is doing, with the few actions worth having
//! without opening the window. Nothing in it comes from what the person kept: counts come from
//! the public needs-you list only.
//!
//! The menu is a pure model ([`menu_model`]); the rest builds it with the desktop's toolkit and
//! swaps it when the status changes.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::Duration;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use nocturne_contract::{EVENT_STATUS, EngineState, EngineStatus};
use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Listener, Manager, Runtime};

use super::copy;
use super::quick_capture;
use super::state::OsState;
use crate::error::blocking;
use crate::state::{AppState, pause_until, refresh_status, resume_now};
use crate::window::{self, ShowReason};

pub const ID: &str = "nocturne";

/// One thing a menu entry does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Keep,
    Ask,
    OpenToday,
    Pause(PauseFor),
    Resume,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseFor {
    Hour,
    Tonight,
    UntilResumed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    /// A line of status; it does nothing when chosen.
    Info(String),
    Action {
        action: Action,
        label: String,
    },
    Submenu {
        label: String,
        items: Vec<MenuEntry>,
    },
    Separator,
}

/// Which of the five drawn states the icon shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Resting,
    Reading,
    Tidying,
    NeedsYou,
    Paused,
}

impl Icon {
    pub fn file_name(self) -> &'static str {
        match self {
            Icon::Resting => "resting",
            Icon::Reading => "reading",
            Icon::Tidying => "tidying",
            Icon::NeedsYou => "needs-you",
            Icon::Paused => "paused",
        }
    }

    pub fn png(self) -> &'static [u8] {
        match self {
            Icon::Resting => include_bytes!("../../icons/tray/resting.png"),
            Icon::Reading => include_bytes!("../../icons/tray/reading.png"),
            Icon::Tidying => include_bytes!("../../icons/tray/tidying.png"),
            Icon::NeedsYou => include_bytes!("../../icons/tray/needs-you.png"),
            Icon::Paused => include_bytes!("../../icons/tray/paused.png"),
        }
    }
}

/// The icon for a status. "Needs you" shows only when something public needs the person, so
/// the icon never says that a private memory exists.
pub fn icon_for(status: &EngineStatus, public_needs: u32) -> Icon {
    match status.state {
        EngineState::Paused => Icon::Paused,
        EngineState::Tidying => Icon::Tidying,
        EngineState::Starting | EngineState::GettingReady | EngineState::Reading => Icon::Reading,
        EngineState::NeedsYou if public_needs > 0 => Icon::NeedsYou,
        EngineState::NeedsYou | EngineState::Resting | EngineState::Stopped => Icon::Resting,
    }
}

/// `CommandOrControl+Shift+Space` as a person reads it.
pub fn shortcut_label(spelling: &str) -> String {
    spelling
        .split('+')
        .map(|p| match p.to_ascii_lowercase().as_str() {
            "commandorcontrol" | "commandorctrl" | "cmdorctrl" | "cmdorcontrol" | "control" => {
                "Ctrl".to_string()
            }
            _ => p.to_string(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn clock_text(at: &Zoned) -> String {
    format!("{:02}:{:02}", at.hour(), at.minute())
}

/// "21:30", "tomorrow 01:00" or "Sat 09:00".
fn until_text(until: Timestamp, now: Timestamp, tz: &TimeZone) -> String {
    let at = until.to_zoned(tz.clone());
    let today = now.to_zoned(tz.clone()).date();
    let time = clock_text(&at);
    if at.date() == today {
        time
    } else if today.tomorrow().is_ok_and(|t| t == at.date()) {
        format!("tomorrow {time}")
    } else {
        let day = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
            [at.date().weekday().to_monday_zero_offset() as usize];
        format!("{day} {time}")
    }
}

/// The status lines at the top of the menu.
fn status_lines(
    status: &EngineStatus,
    public_needs: u32,
    now: Timestamp,
    tz: &TimeZone,
) -> Vec<String> {
    match status.state {
        EngineState::Paused => {
            let until = status
                .paused_until
                .as_deref()
                .and_then(|u| u.parse::<Timestamp>().ok());
            vec![match until {
                Some(u) => format!("{} {}", copy::PAUSED_UNTIL, until_text(u, now, tz)),
                None => copy::PAUSED_UNTIL_RESUMED.to_string(),
            }]
        }
        EngineState::Reading => vec![if status.waiting_to_read > 0 {
            format!(
                "{} \u{b7} {} waiting",
                copy::STATE_READING,
                status.waiting_to_read
            )
        } else {
            copy::STATE_READING.to_string()
        }],
        EngineState::Tidying => {
            let mut lines = Vec::new();
            match &status.tidy {
                Some(t) => {
                    lines.push(format!(
                        "{} \u{b7} {} of {}",
                        copy::STATE_TIDYING,
                        t.done,
                        t.total
                    ));
                    let mut second = String::new();
                    if let Some(eta) = t.eta_seconds {
                        second.push_str(&copy::about_minutes(eta));
                        second.push(' ');
                    }
                    second.push_str(copy::TIDY_PAUSING_IS_FINE);
                    lines.push(second);
                }
                None => lines.push(copy::STATE_TIDYING.to_string()),
            }
            lines
        }
        EngineState::NeedsYou if public_needs > 0 => vec![copy::things_need_you(public_needs)],
        EngineState::NeedsYou | EngineState::Resting => vec![copy::STATE_RESTING.to_string()],
        EngineState::Starting => vec![copy::STATE_STARTING.to_string()],
        EngineState::GettingReady => vec![copy::STATE_GETTING_READY.to_string()],
        EngineState::Stopped => vec![copy::STATE_STOPPED.to_string()],
    }
}

/// The menu for a status. `shortcut` is the quick keep shortcut when it is registered.
pub fn menu_model(
    status: &EngineStatus,
    public_needs: u32,
    shortcut: Option<&str>,
    now: Timestamp,
    tz: &TimeZone,
) -> Vec<MenuEntry> {
    let mut menu: Vec<MenuEntry> = status_lines(status, public_needs, now, tz)
        .into_iter()
        .map(MenuEntry::Info)
        .collect();
    menu.push(MenuEntry::Separator);
    let keep = match shortcut {
        Some(s) => format!("{} ({})", copy::MENU_KEEP, shortcut_label(s)),
        None => copy::MENU_KEEP.to_string(),
    };
    menu.push(MenuEntry::Action {
        action: Action::Keep,
        label: keep,
    });
    menu.push(MenuEntry::Action {
        action: Action::Ask,
        label: copy::MENU_ASK.to_string(),
    });
    menu.push(MenuEntry::Action {
        action: Action::OpenToday,
        label: copy::MENU_OPEN_TODAY.to_string(),
    });
    menu.push(MenuEntry::Separator);
    if status.state == EngineState::Paused {
        menu.push(MenuEntry::Action {
            action: Action::Resume,
            label: copy::MENU_RESUME.to_string(),
        });
    } else {
        menu.push(MenuEntry::Submenu {
            label: copy::MENU_PAUSE.to_string(),
            items: [
                (PauseFor::Hour, copy::MENU_PAUSE_HOUR),
                (PauseFor::Tonight, copy::MENU_PAUSE_TONIGHT),
                (PauseFor::UntilResumed, copy::MENU_PAUSE_RESUME),
            ]
            .into_iter()
            .map(|(p, label)| MenuEntry::Action {
                action: Action::Pause(p),
                label: label.to_string(),
            })
            .collect(),
        });
    }
    menu.push(MenuEntry::Separator);
    menu.push(MenuEntry::Action {
        action: Action::Quit,
        label: copy::MENU_QUIT.to_string(),
    });
    menu
}

/// When "Until tonight" ends: the next 01:00 local, when the tidy window opens. A 01:00 that
/// does not exist (the clocks go forward) is the first moment after it.
pub fn until_tonight(now: Timestamp, tz: &TimeZone) -> Timestamp {
    let today: Date = now.to_zoned(tz.clone()).date();
    let at = |d: Date| d.at(1, 0, 0, 0).to_zoned(tz.clone()).map(|z| z.timestamp());
    match at(today) {
        Ok(t) if t > now => t,
        _ => today
            .tomorrow()
            .ok()
            .and_then(|d| at(d).ok())
            .unwrap_or(now),
    }
}

impl Action {
    fn id(self) -> &'static str {
        match self {
            Action::Keep => "keep",
            Action::Ask => "ask",
            Action::OpenToday => "open-today",
            Action::Pause(PauseFor::Hour) => "pause-hour",
            Action::Pause(PauseFor::Tonight) => "pause-tonight",
            Action::Pause(PauseFor::UntilResumed) => "pause-resume",
            Action::Resume => "resume",
            Action::Quit => "quit",
        }
    }

    fn from_id(id: &str) -> Option<Action> {
        Some(match id {
            "keep" => Action::Keep,
            "ask" => Action::Ask,
            "open-today" => Action::OpenToday,
            "pause-hour" => Action::Pause(PauseFor::Hour),
            "pause-tonight" => Action::Pause(PauseFor::Tonight),
            "pause-resume" => Action::Pause(PauseFor::UntilResumed),
            "resume" => Action::Resume,
            "quit" => Action::Quit,
            _ => return None,
        })
    }
}

/// How many times the desktop is asked whether it has somewhere for the tray, and how long
/// between asks: at login the panel may start after this app, so one look is not enough.
const HOST_TRIES: u32 = 15;
const HOST_PAUSE: Duration = Duration::from_secs(2);

/// Ask `check` until it says yes or `tries` asks have been made, calling `pause` between them.
pub fn wait_for_host(check: &mut dyn FnMut() -> bool, pause: &mut dyn FnMut(), tries: u32) -> bool {
    for attempt in 0..tries {
        if check() {
            return true;
        }
        if attempt + 1 < tries {
            pause();
        }
    }
    false
}

/// Start the tray. Whether the desktop has somewhere to show it is asked off the main thread,
/// and asked again for about half a minute (a start at login can come before the panel); the
/// tray itself is built on the main thread, and `AppState::tray_available` says how it went. A
/// hidden start keeps the window hidden until the answer is final.
pub fn install<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    std::thread::spawn(move || {
        let host = wait_for_host(
            &mut host_registered,
            &mut || std::thread::sleep(HOST_PAUSE),
            HOST_TRIES,
        );
        let handle = app.clone();
        if let Err(e) = app.run_on_main_thread(move || finish(&handle, host)) {
            tracing::warn!("the tray could not be started: {e}");
        }
    });
}

/// Linux has a tray only when something is listening for it (the StatusNotifier watcher has a
/// host). Without one, building the icon succeeds and shows nothing.
#[cfg(target_os = "linux")]
fn host_registered() -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(query_host().unwrap_or(false));
    });
    rx.recv_timeout(Duration::from_secs(1)).unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn query_host() -> zbus::Result<bool> {
    let connection = zbus::blocking::Connection::session()?;
    // Not cached: the cache asks for every property at once, and warns when there is no watcher.
    let watcher = zbus::blocking::proxy::Builder::<zbus::blocking::Proxy>::new(&connection)
        .destination("org.kde.StatusNotifierWatcher")?
        .path("/StatusNotifierWatcher")?
        .interface("org.kde.StatusNotifierWatcher")?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()?;
    watcher.get_property("IsStatusNotifierHostRegistered")
}

#[cfg(not(target_os = "linux"))]
fn host_registered() -> bool {
    true
}

fn finish<R: Runtime>(app: &AppHandle<R>, host: bool) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let built = host && build(app);
    state.set_tray_available(built);
    if built {
        tracing::info!("tray created");
        let handle = app.clone();
        state.on_exit(Box::new(move || {
            handle.remove_tray_by_id(ID);
        }));
        listen(app);
        return;
    }
    tracing::info!(
        "tray unavailable: {}",
        if host {
            "it could not be created"
        } else {
            "this desktop has no notification area"
        }
    );
    if state.start_hidden() {
        state.set_start_hidden(false);
        window::show_when_ready(app, ShowReason::NoTray);
    }
}

/// Build the icon. The toolkit's indicator library panics when it cannot be loaded, so a panic
/// is "unavailable", not a crash.
fn build<R: Runtime>(app: &AppHandle<R>) -> bool {
    let attempt = catch_unwind(AssertUnwindSafe(|| -> tauri::Result<()> {
        let starting = EngineStatus {
            state: EngineState::Starting,
            needs_you: 0,
            waiting_to_read: 0,
            sent_to_internet_bytes: 0.0,
            sent_measured: false,
            paused_until: None,
            tidy: None,
            next_tidy: None,
            notices: Vec::new(),
        };
        let state = app.state::<AppState>();
        let model = menu_model(&starting, 0, None, state.now(), &state.clock.tz());
        let menu = build_menu(app, &model)?;
        TrayIconBuilder::with_id(ID)
            .icon(Image::from_bytes(Icon::Reading.png())?)
            .tooltip(copy::TRAY_TOOLTIP)
            .menu(&menu)
            .show_menu_on_left_click(cfg!(not(windows)))
            .on_menu_event(|app, event| {
                if let Some(action) = Action::from_id(event.id().as_ref()) {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move { run(&app, action).await });
                }
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    window::show_main(tray.app_handle());
                }
            })
            .build(app)?;
        Ok(())
    }));
    match attempt {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            tracing::warn!("the tray could not be created: {e}");
            false
        }
        Err(_) => {
            tracing::warn!("the tray could not be created: its library is missing");
            false
        }
    }
}

fn menu_items<R: Runtime>(
    app: &AppHandle<R>,
    entries: &[MenuEntry],
) -> tauri::Result<Vec<Box<dyn IsMenuItem<R>>>> {
    let mut out: Vec<Box<dyn IsMenuItem<R>>> = Vec::new();
    for entry in entries {
        match entry {
            MenuEntry::Info(text) => {
                out.push(Box::new(MenuItem::new(app, text, false, None::<&str>)?))
            }
            MenuEntry::Action { action, label } => out.push(Box::new(MenuItem::with_id(
                app,
                action.id(),
                label,
                true,
                None::<&str>,
            )?)),
            MenuEntry::Submenu { label, items } => {
                let inner = menu_items(app, items)?;
                let refs: Vec<&dyn IsMenuItem<R>> = inner.iter().map(|i| i.as_ref()).collect();
                out.push(Box::new(Submenu::with_items(app, label, true, &refs)?));
            }
            MenuEntry::Separator => out.push(Box::new(PredefinedMenuItem::separator(app)?)),
        }
    }
    Ok(out)
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, entries: &[MenuEntry]) -> tauri::Result<Menu<R>> {
    let items = menu_items(app, entries)?;
    let refs: Vec<&dyn IsMenuItem<R>> = items.iter().map(|i| i.as_ref()).collect();
    Menu::with_items(app, &refs)
}

/// What the tray shows now: the icon and the menu, from the real status.
pub type Apply = Arc<dyn Fn(Icon, Vec<MenuEntry>) + Send + Sync>;

/// Follow the status: whenever `status-changed` fires, draw the tray again.
///
/// The event is a trigger only. Any page in a window can emit an event with any payload, so the
/// payload is never read: the status is computed fresh from the app's own state. The listener is
/// `listen_any` because the app broadcasts its events (`emit` has no target), and a listener
/// scoped to one target would hear less, not be safer.
pub fn follow<R: Runtime>(app: &AppHandle<R>, apply: Apply) {
    let handle = app.clone();
    let on_change = apply.clone();
    app.listen_any(EVENT_STATUS, move |_| {
        let handle = handle.clone();
        let apply = on_change.clone();
        tauri::async_runtime::spawn(async move { draw(&handle, &apply).await });
    });
    let handle = app.clone();
    tauri::async_runtime::spawn(async move { draw(&handle, &apply).await });
}

fn listen<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    follow(
        app,
        Arc::new(move |icon, model| {
            let handle = handle.clone();
            let _ = handle.clone().run_on_main_thread(move || {
                let Some(tray) = handle.tray_by_id(ID) else {
                    return;
                };
                let result = Image::from_bytes(icon.png())
                    .and_then(|image| tray.set_icon(Some(image)))
                    .and_then(|()| build_menu(&handle, &model))
                    .and_then(|menu| tray.set_menu(Some(menu)));
                if let Err(e) = result {
                    tracing::warn!("the tray could not be updated: {e}");
                }
            });
        }),
    );
}

/// The icon and menu for the status as it is now. The public needs-you count is read here, so a
/// private memory never reaches the menu.
pub async fn current<R: Runtime>(app: &AppHandle<R>) -> (Icon, Vec<MenuEntry>) {
    let status = refresh_status(app).await;
    let state = app.state::<AppState>();
    let public = match state.engine() {
        Ok(engine) => blocking(move || Ok(engine.needs_you_public()?.len() as u32))
            .await
            .unwrap_or(0),
        Err(_) => 0,
    };
    let shortcut = app
        .try_state::<OsState>()
        .and_then(|os| os.shortcut_spelling());
    let model = menu_model(
        &status,
        public,
        shortcut.as_deref(),
        state.now(),
        &state.clock.tz(),
    );
    (icon_for(&status, public), model)
}

async fn draw<R: Runtime>(app: &AppHandle<R>, apply: &Apply) {
    let (icon, model) = current(app).await;
    apply(icon, model);
}

async fn run<R: Runtime>(app: &AppHandle<R>, action: Action) {
    match action {
        Action::Keep => quick_capture::open(app),
        Action::Ask => navigate(app, "/ask"),
        Action::OpenToday => navigate(app, "/today"),
        Action::Pause(how) => {
            let state = app.state::<AppState>();
            let now = state.now();
            let until = match how {
                PauseFor::Hour => now.checked_add(jiff::SignedDuration::from_hours(1)).ok(),
                PauseFor::Tonight => Some(until_tonight(now, &state.clock.tz())),
                PauseFor::UntilResumed => None,
            };
            if let Err(e) = pause_until(app, until.map(|t| t.to_string())).await {
                tracing::warn!("the tray could not pause: {:?}", e.code);
            }
        }
        Action::Resume => {
            if let Err(e) = resume_now(app).await {
                tracing::warn!("the tray could not resume: {:?}", e.code);
            }
        }
        Action::Quit => app.exit(0),
    }
}

/// Bring the window forward on a screen.
fn navigate<R: Runtime>(app: &AppHandle<R>, route: &str) {
    window::show_main(app);
    if let Err(e) = app.emit_to(window::MAIN, super::EVENT_NAVIGATE, route) {
        tracing::warn!("could not send the window to {route}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocturne_contract::TidyProgress;

    fn status(state: EngineState) -> EngineStatus {
        EngineStatus {
            state,
            needs_you: 0,
            waiting_to_read: 0,
            sent_to_internet_bytes: 0.0,
            sent_measured: false,
            paused_until: None,
            tidy: None,
            next_tidy: None,
            notices: vec![],
        }
    }

    fn london() -> TimeZone {
        TimeZone::get("Europe/London").unwrap()
    }

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn infos(menu: &[MenuEntry]) -> Vec<&str> {
        menu.iter()
            .filter_map(|e| match e {
                MenuEntry::Info(s) => Some(s.as_str()),
                _ => None,
            })
            .collect()
    }

    fn has(menu: &[MenuEntry], action: Action) -> bool {
        menu.iter()
            .any(|e| matches!(e, MenuEntry::Action { action: a, .. } if *a == action))
    }

    fn has_pause_submenu(menu: &[MenuEntry]) -> bool {
        menu.iter().any(|e| matches!(e, MenuEntry::Submenu { .. }))
    }

    #[test]
    fn menu_for_each_state() {
        let now = at("2026-11-13T09:00:00Z");
        let tz = london();
        let shortcut = Some("CommandOrControl+Shift+Space");

        let resting = menu_model(&status(EngineState::Resting), 0, shortcut, now, &tz);
        assert_eq!(infos(&resting), ["Resting"]);
        assert!(has_pause_submenu(&resting) && !has(&resting, Action::Resume));
        assert!(has(&resting, Action::Quit) && has(&resting, Action::OpenToday));

        let mut reading = status(EngineState::Reading);
        reading.waiting_to_read = 3;
        let menu = menu_model(&reading, 0, shortcut, now, &tz);
        assert_eq!(infos(&menu), ["Reading \u{b7} 3 waiting"]);

        let mut tidying = status(EngineState::Tidying);
        tidying.tidy = Some(TidyProgress {
            done: 2,
            total: 14,
            eta_seconds: Some(170),
        });
        let menu = menu_model(&tidying, 0, shortcut, now, &tz);
        assert_eq!(
            infos(&menu),
            [
                "Tidying \u{b7} 2 of 14",
                "About 3 minutes. Pausing is fine."
            ]
        );

        let mut needs = status(EngineState::NeedsYou);
        needs.needs_you = 2;
        let menu = menu_model(&needs, 2, shortcut, now, &tz);
        assert_eq!(infos(&menu), ["2 things need you"]);

        let mut paused = status(EngineState::Paused);
        paused.paused_until = Some("2026-11-13T21:30:00Z".into());
        let menu = menu_model(&paused, 0, shortcut, now, &tz);
        assert_eq!(infos(&menu), ["Paused until 21:30"]);
        assert!(has(&menu, Action::Resume));
        assert!(!has_pause_submenu(&menu), "no Pause submenu while paused");

        paused.paused_until = None;
        let menu = menu_model(&paused, 0, None, now, &tz);
        assert_eq!(infos(&menu), ["Paused until you resume"]);
    }

    #[test]
    fn a_pause_that_ends_on_another_day_names_the_day() {
        let now = at("2026-11-13T22:00:00Z");
        let tz = london();
        assert_eq!(
            until_text(at("2026-11-14T01:00:00Z"), now, &tz),
            "tomorrow 01:00"
        );
        assert_eq!(
            until_text(at("2026-11-21T09:00:00Z"), now, &tz),
            "Sat 09:00"
        );
    }

    #[test]
    fn the_keep_entry_shows_the_shortcut_only_when_it_works() {
        let now = at("2026-11-13T09:00:00Z");
        let label = |sc| {
            menu_model(&status(EngineState::Resting), 0, sc, now, &london())
                .into_iter()
                .find_map(|e| match e {
                    MenuEntry::Action {
                        action: Action::Keep,
                        label,
                    } => Some(label),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(
            label(Some("CommandOrControl+Shift+Space")),
            "Keep something\u{2026} (Ctrl+Shift+Space)"
        );
        assert_eq!(label(None), "Keep something\u{2026}");
    }

    #[test]
    fn needs_count_is_public_only() {
        let mut s = status(EngineState::NeedsYou);
        s.needs_you = 3;
        let now = at("2026-11-13T09:00:00Z");
        let menu = menu_model(&s, 1, None, now, &london());
        assert_eq!(infos(&menu), ["1 thing needs you"]);
        assert_eq!(icon_for(&s, 1), Icon::NeedsYou);
        let none_public = menu_model(&s, 0, None, now, &london());
        assert_eq!(
            infos(&none_public),
            ["Resting"],
            "private needs are not hinted at"
        );
        assert_eq!(icon_for(&s, 0), Icon::Resting);
    }

    #[test]
    fn each_state_has_an_icon_that_decodes() {
        for state in [
            EngineState::Starting,
            EngineState::GettingReady,
            EngineState::Resting,
            EngineState::Reading,
            EngineState::Tidying,
            EngineState::Paused,
            EngineState::Stopped,
        ] {
            let icon = icon_for(&status(state), 0);
            assert_eq!(&icon.png()[1..4], b"PNG", "{}", icon.file_name());
        }
    }

    #[test]
    fn until_tonight_is_next_0100() {
        let tz = london();
        assert_eq!(
            until_tonight(at("2026-11-13T23:30:00Z"), &tz),
            at("2026-11-14T01:00:00Z")
        );
        assert_eq!(
            until_tonight(at("2026-11-14T00:30:00Z"), &tz),
            at("2026-11-14T01:00:00Z"),
            "before 01:00 it is today's"
        );
        assert_eq!(
            until_tonight(at("2026-11-14T01:00:00Z"), &tz),
            at("2026-11-15T01:00:00Z"),
            "at 01:00 exactly it is the next one"
        );
        // The clocks go back on 2026-10-25: 01:00 happens twice. From the evening before, the
        // first one ends the pause.
        assert_eq!(
            until_tonight(at("2026-10-24T22:30:00Z"), &tz),
            at("2026-10-25T00:00:00Z")
        );
        // After it, the next day's 01:00 (GMT).
        assert_eq!(
            until_tonight(at("2026-10-25T01:30:00Z"), &tz),
            at("2026-10-26T01:00:00Z")
        );
        // The clocks go forward on 2027-03-28: there is no 01:00, so the pause ends at 02:00 BST.
        assert_eq!(
            until_tonight(at("2027-03-27T23:30:00Z"), &tz),
            at("2027-03-28T01:00:00Z")
        );
    }

    #[test]
    fn the_tray_host_is_asked_again_until_it_appears_or_the_tries_run_out() {
        let mut asked = 0;
        let mut waited = 0;
        let found = wait_for_host(
            &mut || {
                asked += 1;
                asked == 3
            },
            &mut || waited += 1,
            15,
        );
        assert!(found);
        assert_eq!((asked, waited), (3, 2));
        let (mut asked, mut waited) = (0, 0);
        let found = wait_for_host(
            &mut || {
                asked += 1;
                false
            },
            &mut || waited += 1,
            4,
        );
        assert!(!found);
        assert_eq!((asked, waited), (4, 3), "no pause after the last ask");
    }

    #[test]
    fn menu_text_has_no_jargon_or_dashes() {
        let fixture = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../ui/src/copy/never-say.fixture.json"
        ))
        .unwrap();
        let fixture: Vec<serde_json::Value> = serde_json::from_str(&fixture).unwrap();
        let never: Vec<String> = fixture
            .iter()
            .flat_map(|e| e["never"].as_array().unwrap().clone())
            .map(|w| w.as_str().unwrap().to_lowercase())
            .collect();

        let now = at("2026-11-13T09:00:00Z");
        let tz = london();
        let mut texts = copy::all();
        for state in [
            EngineState::Starting,
            EngineState::GettingReady,
            EngineState::Resting,
            EngineState::Reading,
            EngineState::Tidying,
            EngineState::NeedsYou,
            EngineState::Paused,
            EngineState::Stopped,
        ] {
            fn flatten(menu: &[MenuEntry], out: &mut Vec<String>) {
                for e in menu {
                    match e {
                        MenuEntry::Info(s) => out.push(s.clone()),
                        MenuEntry::Action { label, .. } => out.push(label.clone()),
                        MenuEntry::Submenu { label, items } => {
                            out.push(label.clone());
                            flatten(items, out);
                        }
                        MenuEntry::Separator => {}
                    }
                }
            }
            let mut s = status(state);
            s.waiting_to_read = 4;
            s.tidy = Some(TidyProgress {
                done: 1,
                total: 5,
                eta_seconds: Some(60),
            });
            flatten(
                &menu_model(&s, 2, Some("CommandOrControl+Shift+Space"), now, &tz),
                &mut texts,
            );
        }
        for text in texts {
            assert!(!text.contains('\u{2014}'), "{text}");
            assert!(copy::has_jargon(&text).is_none(), "{text}");
            let words: Vec<String> = text
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .map(str::to_string)
                .collect();
            let spaced = format!(" {} ", words.join(" "));
            for word in &never {
                assert!(!spaced.contains(&format!(" {word} ")), "{text} says {word}");
            }
        }
    }
}
