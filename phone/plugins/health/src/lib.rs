//! Health Connect on Android and Apple Health on iPhone, read only: the kinds the person allows,
//! read as Nocturne keeps them (a night with its asleep periods, a day of heart rate by the hour,
//! a day's steps, each blood pressure reading, workout and weight). The first read covers a
//! number of days back and hands a changes token; later reads bring only what changed, until the
//! token lapses. On iPhone every kind asked about reads as allowed, because Health never says
//! which it refused. The app's Rust side calls it; the screens never do. On a computer Health is
//! missing.

use nocturne_contract::{Reading, ReadingKind};
use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{Manager, Runtime};

/// Whether Health Connect is on this phone. `NeedsUpdate`: installed, but too old.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    Available,
    NeedsUpdate,
    Missing,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Available {
    status: Availability,
    #[serde(default)]
    older_readings: bool,
}

/// The kinds Health Connect lets Nocturne read, and whether it may read further back than 30
/// days.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Granted {
    pub kinds: Vec<ReadingKind>,
    pub history: bool,
    /// Workouts carry their distance only when it is allowed too.
    #[serde(default)]
    pub distance: bool,
    /// Health never says which kinds it refused (iPhone): a reading missing from a read may be
    /// refused, not deleted, so only Health's own deletions remove anything.
    #[serde(default)]
    pub refusals_hidden: bool,
}

/// Everything in the kinds asked for since `days` ago, and the token for what changes next.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullRead {
    pub readings: Vec<Reading>,
    pub token: String,
    /// The first day read, in the phone's own time.
    pub from_day: String,
}

/// What changed since a token: readings new or changed (a day of heart rate or steps read again
/// whole), and the ids of readings deleted in Health. `expired`: the token lapsed, and only a
/// full read will do.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    pub readings: Vec<Reading>,
    pub removed: Vec<String>,
    pub token: Option<String>,
    pub expired: bool,
    /// The days read again whole, by kind: a reading of that kind sent before for one of those
    /// days and missing now was deleted in Health.
    #[serde(default)]
    pub refreshed: std::collections::HashMap<ReadingKind, Vec<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Ask<'a> {
    kinds: &'a [ReadingKind],
    history: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadArgs<'a> {
    kinds: &'a [ReadingKind],
    days: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangesArgs<'a> {
    kinds: &'a [ReadingKind],
    token: &'a str,
    refresh_days: u32,
}

#[derive(Debug, Deserialize)]
struct Route {
    route: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Error {
    Unavailable,
    NotAllowed,
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

pub struct Health<R: Runtime>(Option<PluginHandle<R>>);

impl<R: Runtime> Health<R> {
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
            } else if text.contains("unavailable") {
                Error::Unavailable
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

    pub fn availability(&self) -> Availability {
        self.run::<Available>("availability", ())
            .map_or(Availability::Missing, |a| a.status)
    }

    /// Whether this Health Connect offers readings older than 30 days.
    pub fn older_available(&self) -> bool {
        self.run::<Available>("availability", ())
            .is_ok_and(|a| a.older_readings)
    }

    pub fn granted(&self) -> Result<Granted, Error> {
        self.run("granted", ())
    }

    /// Health Connect's own screen for these kinds; answers what is allowed after it.
    pub fn request(&self, kinds: &[ReadingKind], history: bool) -> Result<Granted, Error> {
        self.run("request", Ask { kinds, history })
    }

    pub fn read(&self, kinds: &[ReadingKind], days: u32) -> Result<FullRead, Error> {
        self.run("read", ReadArgs { kinds, days })
    }

    /// What changed since `token`, with the last `refresh_days` of heart rate, resting heart
    /// rate and steps read again, because a deletion there says only an id.
    pub fn changes(
        &self,
        kinds: &[ReadingKind],
        token: &str,
        refresh_days: u32,
    ) -> Result<Changes, Error> {
        self.run(
            "changes",
            ChangesArgs {
                kinds,
                token,
                refresh_days,
            },
        )
    }

    /// Health Connect's own settings, where the permissions are.
    pub fn open_settings(&self) -> Result<(), Error> {
        self.run::<serde_json::Value>("openSettings", serde_json::json!({ "target": "settings" }))
            .map(|_| ())
    }

    /// Health Connect in the store, where it is missing or too old.
    pub fn open_store(&self) -> Result<(), Error> {
        self.run::<serde_json::Value>("openSettings", serde_json::json!({ "target": "store" }))
            .map(|_| ())
    }

    /// The page the app was opened on from Health Connect (`rationale`), once.
    pub fn take_route(&self) -> Option<String> {
        self.run::<Route>("takeRoute", ())
            .ok()
            .and_then(|r| r.route)
    }
}

pub trait HealthExt<R: Runtime> {
    fn health(&self) -> &Health<R>;
}

impl<R: Runtime, T: Manager<R>> HealthExt<R> for T {
    fn health(&self) -> &Health<R> {
        self.state::<Health<R>>().inner()
    }
}

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_nocturne_health);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("nocturne-health")
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            let handle = Some(_api.register_android_plugin("app.nocturne.health", "HealthPlugin")?);
            #[cfg(target_os = "ios")]
            let handle = Some(_api.register_ios_plugin(init_plugin_nocturne_health)?);
            #[cfg(not(mobile))]
            let handle: Option<PluginHandle<R>> = None;
            app.manage(Health(handle));
            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocturne_contract::ReadingValue;

    #[test]
    fn kinds_go_to_the_plugin_by_their_wire_names() {
        let args = Ask {
            kinds: &[ReadingKind::HeartRate, ReadingKind::Workout],
            history: true,
        };
        assert_eq!(
            serde_json::to_string(&args).unwrap(),
            r#"{"kinds":["heartRate","workout"],"history":true}"#
        );
    }

    #[test]
    fn readings_read_from_the_plugin() {
        let json = r#"{"readings":[{"phoneId":"2026-11-20","day":"2026-11-20",
            "start":"2026-11-20T00:00:00Z","end":null,"sourceDevice":null,
            "value":{"type":"steps","total":8412}}],"removed":["a-b"],"token":"t2","expired":false,
            "refreshed":{"steps":["2026-11-20"]}}"#;
        let c: Changes = serde_json::from_str(json).unwrap();
        assert_eq!(c.readings[0].value, ReadingValue::Steps { total: 8412 });
        assert_eq!(c.removed, vec!["a-b".to_string()]);
        assert_eq!(
            c.refreshed[&ReadingKind::Steps],
            vec!["2026-11-20".to_string()]
        );
    }
}
