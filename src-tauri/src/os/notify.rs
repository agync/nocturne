//! The one quiet notification: the page is ready.
//!
//! It is sent at most once per page date, only in the hours after the page time, only when the
//! person has not opened the page, and never says anything private: the title is fixed and the
//! body is built from the public needs-you count alone, never from the headline or any memory.

use std::sync::Arc;
use std::time::Duration;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use nocturne_contract::{Delivery, EVENT_PAGE, SettingsView};
use tauri::{AppHandle, Listener, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;

use super::copy;
use super::state::OsState;
use crate::error::blocking;
use crate::state::AppState;

/// How long after the page time a missed notification is still worth sending.
const CATCH_UP: SignedDuration = SignedDuration::from_hours(6);
/// Nothing is sent from here on in the evening, whatever the page time.
const LAST_HOUR: i8 = 21;
const CHECK_EVERY: Duration = Duration::from_secs(60);

/// What shows a notification. A fake stands in for it in tests.
pub trait Notifier: Send + Sync {
    fn notify(&self, title: &str, body: &str) -> Result<(), String>;
}

pub struct PluginNotifier<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> Notifier for PluginNotifier<R> {
    fn notify(&self, title: &str, body: &str) -> Result<(), String> {
        self.0
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|e| e.to_string())
    }
}

fn local(tz: &TimeZone, date: Date, h: i8, m: i8) -> Option<Timestamp> {
    date.at(h, m, 0, 0)
        .to_zoned(tz.clone())
        .ok()
        .map(|z| z.timestamp())
}

/// The date to notify for, if a notification is due now. Pure: every input is passed in.
///
/// Due when delivery is "notify", nothing is paused, today's page is saved and not yet opened,
/// nothing was sent for it, and now is from the page time until the earlier of six hours later
/// and 21:00. A page time that does not exist (the clocks went forward) starts when the clock
/// jumps over it.
pub fn decide(
    now: Timestamp,
    tz: &TimeZone,
    settings: &SettingsView,
    saved_today: bool,
    opened: bool,
    paused: bool,
    notified: Option<Date>,
) -> Option<Date> {
    if settings.delivery != Delivery::Notify || paused || !saved_today || opened {
        return None;
    }
    let today = now.to_zoned(tz.clone()).date();
    if notified == Some(today) {
        return None;
    }
    let (h, m) = settings.page_time.split_once(':')?;
    let start = local(tz, today, h.parse().ok()?, m.parse().ok()?)?;
    let evening = local(tz, today, LAST_HOUR, 0)?;
    let end = start.checked_add(CATCH_UP).ok()?.min(evening);
    (now >= start && now < end).then_some(today)
}

/// The whole body: the count, then where the page is.
pub fn body(public_needs: u32) -> String {
    format!(
        "{} {}",
        copy::count_sentence(public_needs),
        copy::NOTICE_TAIL
    )
}

/// Look at everything and send the notification if one is due. Returns whether one was shown.
pub async fn check<R: Runtime>(app: &AppHandle<R>) -> bool {
    let (Some(shell), Some(os)) = (app.try_state::<AppState>(), app.try_state::<OsState>()) else {
        return false;
    };
    let Some(notifier) = os.notifier() else {
        return false;
    };
    let Ok(engine) = shell.engine() else {
        return false;
    };
    let now = shell.now();
    let tz = shell.clock.tz();
    let today = now.to_zoned(tz.clone()).date();
    let paused = shell.status.is_paused(now);
    let file = os.file();
    let notified = file.notified.as_deref().and_then(|d| d.parse().ok());
    let opened = file.seen.as_deref().and_then(|d| d.parse::<Date>().ok()) == Some(today);
    let read = blocking(move || {
        let settings = engine.settings()?;
        let saved = engine.page(Some(today))?.is_some();
        let public = engine.needs_you_public()?.len() as u32;
        Ok((settings, saved, public))
    })
    .await;
    let Ok((settings, saved, public)) = read else {
        return false;
    };
    let Some(date) = decide(now, &tz, &settings, saved, opened, paused, notified) else {
        return false;
    };
    // Claimed before sending, in one step: the minute timer, a page-ready event and a forged
    // event can all arrive together, and only one may send. A notification that fails is not
    // retried every minute either.
    let claimed = os.update(|f| {
        if f.notified.as_deref() == Some(date.to_string().as_str()) {
            return false;
        }
        f.notified = Some(date.to_string());
        true
    });
    if !claimed {
        return false;
    }
    match notifier.notify(copy::NOTICE_TITLE, &body(public)) {
        Ok(()) => {
            tracing::info!("page notification sent");
            true
        }
        Err(e) => {
            tracing::info!("page notification not shown: {e}");
            false
        }
    }
}

/// Check whenever `page-ready` fires. The event is a trigger only: any page in a window can emit
/// it with any payload, so the payload is never read and [`check`] asks the engine whether
/// today's page is really saved. `listen_any` because the app broadcasts its events.
pub fn listen<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    app.listen_any(EVENT_PAGE, move |_| {
        let handle = handle.clone();
        tauri::async_runtime::spawn(async move {
            check(&handle).await;
        });
    });
}

/// Check when a page is saved and once a minute.
pub fn install<R: Runtime>(app: &AppHandle<R>, os: &OsState) {
    os.set_notifier(Arc::new(PluginNotifier(app.clone())));
    listen(app);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            check(&handle).await;
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocturne_contract::{LettingGo, Theme};

    fn settings(page_time: &str, delivery: Delivery) -> SettingsView {
        SettingsView {
            evening_reminder: None,
            page_time: page_time.into(),
            delivery,
            auto_print: false,
            voice: "bf_emma".into(),
            voice_speed: 1.0,
            shortcut: "CommandOrControl+Shift+Space".into(),
            shortcut_available: true,
            desktop_shortcut: None,
            inbox_folder: None,
            start_at_login: false,
            letting_go: LettingGo::Normal,
            theme: Theme::System,
            language: "en-GB".into(),
            check_updates: false,
            reminders: false,
            lock: false,
            agents: false,
        }
    }

    fn london() -> TimeZone {
        TimeZone::get("Europe/London").unwrap()
    }

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn due(now: &str, page_time: &str, notified: Option<&str>) -> Option<Date> {
        decide(
            at(now),
            &london(),
            &settings(page_time, Delivery::Notify),
            true,
            false,
            false,
            notified.map(|d| d.parse().unwrap()),
        )
    }

    fn date(s: &str) -> Option<Date> {
        Some(s.parse().unwrap())
    }

    #[test]
    fn sends_once_at_page_time() {
        assert_eq!(due("2026-11-13T06:59:00Z", "07:00", None), None);
        assert_eq!(
            due("2026-11-13T07:00:00Z", "07:00", None),
            date("2026-11-13")
        );
        assert_eq!(
            due("2026-11-13T07:01:00Z", "07:00", Some("2026-11-13")),
            None,
            "after sending, none"
        );
        assert_eq!(
            due("2026-11-14T07:00:00Z", "07:00", Some("2026-11-13")),
            date("2026-11-14"),
            "yesterday's does not count for today"
        );
    }

    #[test]
    fn catch_up_page_at_0930_notifies() {
        assert_eq!(
            due("2026-11-13T09:30:00Z", "07:00", None),
            date("2026-11-13")
        );
        assert_eq!(
            due("2026-11-13T12:59:00Z", "07:00", None),
            date("2026-11-13")
        );
    }

    #[test]
    fn at_1400_does_not() {
        assert_eq!(due("2026-11-13T13:00:00Z", "07:00", None), None);
        assert_eq!(due("2026-11-13T14:00:00Z", "07:00", None), None);
    }

    #[test]
    fn page_time_2000_stops_at_2100() {
        assert_eq!(
            due("2026-11-13T20:59:00Z", "20:00", None),
            date("2026-11-13")
        );
        assert_eq!(due("2026-11-13T21:00:00Z", "20:00", None), None);
        assert_eq!(
            due("2026-11-13T22:30:00Z", "22:00", None),
            None,
            "no evening window"
        );
    }

    #[test]
    fn quiet_delivery_paused_or_opened_never_sends() {
        let now = at("2026-11-13T07:30:00Z");
        let tz = london();
        let notify = settings("07:00", Delivery::Notify);
        let quiet = settings("07:00", Delivery::Quiet);
        assert!(decide(now, &tz, &notify, true, false, false, None).is_some());
        assert!(decide(now, &tz, &quiet, true, false, false, None).is_none());
        assert!(decide(now, &tz, &notify, true, false, true, None).is_none());
        assert!(decide(now, &tz, &notify, true, true, false, None).is_none());
        assert!(decide(now, &tz, &notify, false, false, false, None).is_none());
    }

    #[test]
    fn dst_spring_forward_page_time_in_gap() {
        // 01:30 does not exist on 2027-03-28 in London: the clocks jump from 01:00 to 02:00.
        let tz = london();
        let s = settings("01:30", Delivery::Notify);
        let mut sent = 0;
        let mut notified = None;
        let mut now = at("2027-03-28T00:00:00Z");
        let end = at("2027-03-28T08:00:00Z");
        while now < end {
            if let Some(d) = decide(now, &tz, &s, true, false, false, notified) {
                sent += 1;
                notified = Some(d);
            }
            now = now.checked_add(SignedDuration::from_mins(1)).unwrap();
        }
        assert_eq!(sent, 1, "fires once");
    }

    #[test]
    fn body_counts_words() {
        let sentences: Vec<_> = [0, 1, 2, 9, 10].map(copy::count_sentence).into();
        assert_eq!(
            sentences,
            [
                "Nothing needs you today.",
                "One thing needs you today.",
                "Two things need you today.",
                "Nine things need you today.",
                "10 things need you today.",
            ]
        );
        assert_eq!(
            body(2),
            "Two things need you today. It\u{2019}s there when you open Nocturne."
        );
    }

    #[test]
    fn notification_text_has_no_jargon_or_dashes() {
        for n in [0, 1, 2, 9, 10, 40] {
            let text = format!("{} {}", copy::NOTICE_TITLE, body(n));
            assert!(!text.contains('\u{2014}'), "{text}");
            assert!(copy::has_jargon(&text).is_none(), "{text}");
        }
    }
}
