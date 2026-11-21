//! The phone's calendar, read only: its calendars, and each appointment on each day in a window
//! of days around today, as the computer takes them. Repeating entries come as the separate days
//! they fall on. Unanswered and declined invitations and cancelled entries are left out, and the
//! notes come only when asked for. The app's Rust side calls it; the screens never do. On a
//! computer there is no calendar: access reads as refused and every read is unavailable.

use nocturne_contract::{CalendarEntry, PhoneCalendar};
use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{Manager, Runtime};

/// Whether the app may read the calendar, as the phone says. `Prompt` is also what a phone says
/// once it will no longer show its prompt; only the app knows whether it asked before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Access {
    Granted,
    Denied,
    Prompt,
    PromptWithRationale,
}

#[derive(Debug, Deserialize)]
struct Status {
    calendar: Access,
}

#[derive(Debug, Deserialize)]
struct Calendars {
    calendars: Vec<PhoneCalendar>,
}

/// The days read, both included, in the phone's own time, and what is on them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instances {
    pub window_from: String,
    pub window_to: String,
    pub entries: Vec<CalendarEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Read<'a> {
    back: u32,
    ahead: u32,
    calendar_ids: &'a [String],
    with_notes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Error {
    /// No calendar on this platform.
    Unavailable,
    /// The calendar is not allowed.
    NotAllowed,
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

pub struct Calendar<R: Runtime>(Option<PluginHandle<R>>);

impl<R: Runtime> Calendar<R> {
    #[cfg(mobile)]
    fn run<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        args: impl Serialize,
    ) -> Result<T, Error> {
        let handle = self.0.as_ref().ok_or(Error::Unavailable)?;
        handle.run_mobile_plugin(method, args).map_err(|e| {
            let text = e.to_string();
            if text.contains("notAllowed") {
                Error::NotAllowed
            } else {
                Error::Failed(text)
            }
        })
    }

    #[cfg(not(mobile))]
    fn run<T>(&self, _method: &str, _args: impl Serialize) -> Result<T, Error> {
        let _ = &self.0;
        Err(Error::Unavailable)
    }

    pub fn access(&self) -> Access {
        self.run::<Status>("checkPermissions", ())
            .map_or(Access::Denied, |s| s.calendar)
    }

    /// Shows the phone's own prompt, unless it will no longer show it; answers what it says.
    pub fn request(&self) -> Result<Access, Error> {
        self.run::<Status>(
            "requestPermissions",
            serde_json::json!({ "permissions": ["calendar"] }),
        )
        .map(|s| s.calendar)
    }

    pub fn calendars(&self) -> Result<Vec<PhoneCalendar>, Error> {
        self.run::<Calendars>("listCalendars", ())
            .map(|c| c.calendars)
    }

    /// From `back` days before today to `ahead` days after it, both included, in the calendars
    /// named.
    pub fn instances(
        &self,
        back: u32,
        ahead: u32,
        calendar_ids: &[String],
        with_notes: bool,
    ) -> Result<Instances, Error> {
        self.run(
            "listInstances",
            Read {
                back,
                ahead,
                calendar_ids,
                with_notes,
            },
        )
    }
}

pub trait CalendarExt<R: Runtime> {
    fn calendar(&self) -> &Calendar<R>;
}

impl<R: Runtime, T: Manager<R>> CalendarExt<R> for T {
    fn calendar(&self) -> &Calendar<R> {
        self.state::<Calendar<R>>().inner()
    }
}

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_nocturne_calendar);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("nocturne-calendar")
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            let handle =
                Some(_api.register_android_plugin("app.nocturne.calendar", "CalendarPlugin")?);
            #[cfg(target_os = "ios")]
            let handle = Some(_api.register_ios_plugin(init_plugin_nocturne_calendar)?);
            #[cfg(not(mobile))]
            let handle: Option<PluginHandle<R>> = None;
            app.manage(Calendar(handle));
            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_reads_as_the_phone_says_it() {
        let s: Status = serde_json::from_str(r#"{"calendar":"prompt-with-rationale"}"#).unwrap();
        assert_eq!(s.calendar, Access::PromptWithRationale);
    }

    #[test]
    fn instances_read_from_the_plugin() {
        let json = r#"{"windowFrom":"2026-11-19","windowTo":"2026-12-05","entries":[{
            "phoneId":"abc","calendarId":"3","title":"Dentist","start":"2026-11-23T09:00:00Z",
            "end":"2026-11-23T09:30:00Z","timeZone":"Europe/London","allDay":false,"place":null,
            "people":[],"repeats":false,"notes":null}]}"#;
        let i: Instances = serde_json::from_str(json).unwrap();
        assert_eq!(i.entries.len(), 1);
        assert_eq!(i.entries[0].repeat_rule, None);
    }
}
