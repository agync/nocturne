//! The words the desktop side says: the tray menu and the notification. They are kept here, in
//! one place, so the copy review reads them together. No plural syntax: counts pick between
//! two forms in code.

pub const TRAY_TOOLTIP: &str = "Nocturne";

pub const STATE_STARTING: &str = "Starting";
pub const STATE_GETTING_READY: &str = "Getting ready";
pub const STATE_RESTING: &str = "Resting";
pub const STATE_READING: &str = "Reading";
pub const STATE_TIDYING: &str = "Tidying";
pub const STATE_STOPPED: &str = "Stopped";
pub const STATE_PAUSED: &str = "Paused";
pub const PAUSED_UNTIL: &str = "Paused until";
pub const PAUSED_UNTIL_RESUMED: &str = "Paused until you resume";

pub const TIDY_PAUSING_IS_FINE: &str = "Pausing is fine.";

pub const MENU_KEEP: &str = "Keep something\u{2026}";
pub const MENU_ASK: &str = "Ask\u{2026}";
pub const MENU_OPEN_TODAY: &str = "Open today\u{2019}s page";
pub const MENU_PAUSE: &str = "Pause Nocturne";
pub const MENU_PAUSE_HOUR: &str = "For 1 hour";
pub const MENU_PAUSE_TONIGHT: &str = "Until tonight";
pub const MENU_PAUSE_RESUME: &str = "Until I resume";
pub const MENU_RESUME: &str = "Resume now";
pub const MENU_QUIT: &str = "Quit Nocturne";

pub const NOTICE_TITLE: &str = "Your page is ready";
pub const NOTICE_TAIL: &str = "It\u{2019}s there when you open Nocturne.";

/// "1 thing needs you", "3 things need you".
pub fn things_need_you(n: u32) -> String {
    if n == 1 {
        "1 thing needs you".to_string()
    } else {
        format!("{n} things need you")
    }
}

/// "About 3 minutes.", "About 1 minute."
pub fn about_minutes(seconds: u32) -> String {
    let m = seconds.div_ceil(60).max(1);
    if m == 1 {
        "About 1 minute.".to_string()
    } else {
        format!("About {m} minutes.")
    }
}

/// Every string above, for the tests that scan the words.
#[cfg(test)]
pub fn all() -> Vec<String> {
    let mut v: Vec<String> = [
        TRAY_TOOLTIP,
        STATE_STARTING,
        STATE_GETTING_READY,
        STATE_RESTING,
        STATE_READING,
        STATE_TIDYING,
        STATE_STOPPED,
        STATE_PAUSED,
        PAUSED_UNTIL,
        PAUSED_UNTIL_RESUMED,
        TIDY_PAUSING_IS_FINE,
        MENU_KEEP,
        MENU_ASK,
        MENU_OPEN_TODAY,
        MENU_PAUSE,
        MENU_PAUSE_HOUR,
        MENU_PAUSE_TONIGHT,
        MENU_PAUSE_RESUME,
        MENU_RESUME,
        MENU_QUIT,
        NOTICE_TITLE,
        NOTICE_TAIL,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for n in [0, 1, 2, 9, 10] {
        v.push(things_need_you(n));
    }
    v.push(about_minutes(30));
    v.push(about_minutes(600));
    v
}

/// The technical words the interface keeps out of everything but the advanced details, and the
/// never-say list the interface copy is held to. `true` when `text` uses one of them.
#[cfg(test)]
pub fn has_jargon(text: &str) -> Option<String> {
    const JARGON: &[&str] = &[
        "model",
        "llm",
        "inference",
        "token",
        "npu",
        "gpu",
        "embedding",
        "vector",
        "graph",
        "triple",
        "provenance",
        "quarantine",
        "consolidation",
        "backend",
        "server",
        "lemonade",
        "ai",
        "engine",
    ];
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .find(|w| JARGON.contains(&w.as_str()))
}
