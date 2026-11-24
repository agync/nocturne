//! The phone's own bits Nocturne needs: the screen insets as CSS variables (the app draws edge
//! to edge), the phone's name for the default device name, this app's settings page for a
//! camera turned off there, and what the app was opened for from outside (a shortcut on its
//! icon, or another app's share menu), the phone's own sheets to save a file and to choose one,
//! the room left on the phone, and on an iPhone a folder kept out of its backup. For the core
//! alone: a secret kept in the phone's safe place (Android's keystore, the iPhone's keychain),
//! whether the phone charges and its network is unmetered, and the background windows the system
//! allows while it charges. On a computer every command answers a stand-in, and no secret is
//! kept.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{AppHandle, Manager, Runtime, command};

/// The system bars and cut-outs, and the keyboard while it is up, in CSS pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Insets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
    #[serde(default)]
    pub keyboard: f64,
}

/// A file another app shared, as copied into this app's own space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedFile {
    /// Its name in the shared folder, which `shared_file` reads.
    pub name: String,
    /// Its own name, as the other app gave it.
    pub shown: String,
    pub mime: String,
    pub size: u64,
}

/// What another app shared: its text, its subject line, and its files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shared {
    pub text: Option<String>,
    pub subject: Option<String>,
    pub files: Vec<SharedFile>,
}

#[derive(Debug, Deserialize)]
struct SharedAnswer {
    shared: Option<SharedOnPhone>,
}

#[derive(Debug, Deserialize)]
struct SharedOnPhone {
    text: Option<String>,
    subject: Option<String>,
    dir: String,
    #[serde(default)]
    files: Vec<SharedFile>,
}

#[derive(Debug, Deserialize)]
struct Route {
    route: Option<String>,
}

/// The shortcuts on the app's icon.
const ROUTES: &[&str] = &["keep", "record", "checkin"];

/// A kept file's name, only when it names a file directly inside the shared folder.
fn inside(dir: &Path, name: &str) -> Option<PathBuf> {
    let plain = !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
        && name != "..";
    plain.then(|| dir.join(name))
}

#[derive(Debug, Deserialize)]
struct Named {
    name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Error(String);

/// A file the person chose through the phone's open sheet, copied into the app's own space.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Chosen {
    /// Where the copy is.
    pub path: String,
    /// Its own name, as the place it came from gives it.
    pub name: String,
    pub size: u64,
    /// When it last changed there, in milliseconds since 1970, when that place says.
    #[serde(default)]
    pub modified: Option<i64>,
}

/// What came of one of the phone's sheets.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum Sheet {
    Saved,
    Chosen(Chosen),
    /// The person closed the sheet.
    Cancelled,
    /// The place had no room for the file.
    Full,
    Failed,
}

#[derive(Debug, Deserialize)]
struct Free {
    #[serde(default)]
    bytes: Option<u64>,
}

/// Whether the phone charges, whether the system has a background window open for the app, and
/// whether its network costs nothing by the byte, such as Wi-Fi.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct Power {
    /// `None` when the phone does not say.
    #[serde(default)]
    pub charging: Option<bool>,
    #[serde(default)]
    pub window: bool,
    /// `None` while offline, or when the phone does not say.
    #[serde(default)]
    pub unmetered: Option<bool>,
}

#[derive(Debug, Serialize)]
struct Secret<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct SecretValue {
    #[serde(default)]
    value: Option<String>,
}

#[derive(Debug, Serialize)]
struct BackgroundOn {
    on: bool,
}

#[derive(Debug, Serialize)]
struct SaveFile<'a> {
    path: &'a str,
    name: &'a str,
}

fn plain(path: &Path) -> Result<&str, Error> {
    path.to_str()
        .ok_or_else(|| Error("not a plain path".into()))
}

/// The Android side, the shared folder, and the iPhone side, which keeps folders out of the
/// backup, shows the save and open sheets and says how much room is left: every other command
/// answers its stand-in there.
pub struct System<R: Runtime>(
    Option<PluginHandle<R>>,
    Mutex<Option<PathBuf>>,
    Option<PluginHandle<R>>,
);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Folder<'a> {
    path: &'a str,
}

impl<R: Runtime> System<R> {
    #[cfg(mobile)]
    fn run<T: serde::de::DeserializeOwned>(&self, method: &str) -> Result<Option<T>, Error> {
        let Some(handle) = self.0.as_ref() else {
            return Ok(None);
        };
        handle
            .run_mobile_plugin(method, ())
            .map(Some)
            .map_err(|e| Error(e.to_string()))
    }

    #[cfg(not(mobile))]
    fn run<T>(&self, _method: &str) -> Result<Option<T>, Error> {
        let _ = &self.0;
        Ok(None)
    }

    /// Whichever side this phone has, for what both sides answer.
    #[cfg(mobile)]
    fn native<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        args: impl Serialize,
    ) -> Result<Option<T>, Error> {
        let Some(handle) = self.0.as_ref().or(self.2.as_ref()) else {
            return Ok(None);
        };
        handle
            .run_mobile_plugin(method, args)
            .map(Some)
            .map_err(|e| Error(e.to_string()))
    }

    #[cfg(not(mobile))]
    fn native<T>(&self, _method: &str, _args: impl Serialize) -> Result<Option<T>, Error> {
        let _ = (&self.0, &self.2);
        Ok(None)
    }

    /// The bytes free on the volume that holds `dir`, as the app may use them. `None` where the
    /// phone does not say, and on a computer.
    pub fn free_space(&self, dir: &Path) -> Result<Option<u64>, Error> {
        let free: Option<Free> = self.native("freeSpace", Folder { path: plain(dir)? })?;
        Ok(free.and_then(|f| f.bytes))
    }

    /// The phone's save sheet for the file at `src`, offered under `name`, so the person chooses
    /// where it goes. Blocks until the sheet closes and the file is written there: call it off
    /// the main thread. `Sheet::Failed` on a computer.
    pub fn save_file(&self, src: &Path, name: &str) -> Result<Sheet, Error> {
        let args = SaveFile {
            path: plain(src)?,
            name,
        };
        Ok(self.native("saveFile", args)?.unwrap_or(Sheet::Failed))
    }

    /// The phone's open sheet; the chosen file is copied into `dir`, replacing the copy there.
    /// Blocks like [`System::save_file`]. `Sheet::Failed` on a computer.
    pub fn choose_file(&self, dir: &Path) -> Result<Sheet, Error> {
        let sheet = self.native("chooseFile", Folder { path: plain(dir)? })?;
        Ok(sheet.unwrap_or(Sheet::Failed))
    }

    /// Keeps `value` as the secret `name` in the phone's safe place, in place of one kept
    /// before. An error where there is no safe place, as on a computer.
    pub fn keep_secret(&self, name: &str, value: &str) -> Result<(), Error> {
        let args = Secret {
            name,
            value: Some(value),
        };
        let kept: Option<serde_json::Value> = self.native("secretKeep", args)?;
        kept.map(drop).ok_or_else(|| Error("no safe place".into()))
    }

    /// The secret `name`, or `None` when none is kept.
    pub fn secret(&self, name: &str) -> Result<Option<String>, Error> {
        let read: Option<SecretValue> = self.native("secretRead", Secret { name, value: None })?;
        Ok(read.and_then(|r| r.value))
    }

    /// Removes the secret `name`, and on Android the key that sealed it.
    pub fn forget_secret(&self, name: &str) -> Result<(), Error> {
        let _: Option<serde_json::Value> =
            self.native("secretForget", Secret { name, value: None })?;
        Ok(())
    }

    /// Whether the phone charges and a background window is open. Unknown on a computer.
    pub fn power(&self) -> Result<Power, Error> {
        Ok(self.native("power", ())?.unwrap_or_default())
    }

    /// Asks the system for background windows while the phone charges, or for none.
    pub fn background(&self, on: bool) -> Result<(), Error> {
        let _: Option<serde_json::Value> = self.native("background", BackgroundOn { on })?;
        Ok(())
    }

    /// Ends the open background window: nothing is left to do in it.
    pub fn background_done(&self) -> Result<(), Error> {
        let _: Option<serde_json::Value> = self.native("backgroundDone", ())?;
        Ok(())
    }

    /// Keep a folder of the app's out of the phone's own backup. Only an iPhone needs asking:
    /// Android's backup rules already leave out every folder of the app.
    pub fn exclude_from_backup(&self, path: &Path) -> Result<(), Error> {
        let path = plain(path)?;
        #[cfg(mobile)]
        if let Some(handle) = self.2.as_ref() {
            return handle
                .run_mobile_plugin::<serde_json::Value>("excludeFromBackup", Folder { path })
                .map(drop)
                .map_err(|e| Error(e.to_string()));
        }
        let _ = (&self.2, Folder { path });
        Ok(())
    }
}

pub trait SystemExt<R: Runtime> {
    fn system(&self) -> &System<R>;
}

impl<R: Runtime, T: Manager<R>> SystemExt<R> for T {
    fn system(&self) -> &System<R> {
        self.state::<System<R>>().inner()
    }
}

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_nocturne_system);

#[command]
async fn insets<R: Runtime>(app: AppHandle<R>) -> Result<Insets, Error> {
    Ok(app.state::<System<R>>().run("insets")?.unwrap_or_default())
}

/// The phone's own name where the platform gives it; `None` when it does not.
#[command]
async fn device_name<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, Error> {
    let named: Option<Named> = app.state::<System<R>>().run("deviceName")?;
    Ok(named.and_then(|n| n.name).filter(|n| !n.trim().is_empty()))
}

#[derive(Debug, Deserialize)]
struct Clock {
    is24: Option<bool>,
}

/// Whether the phone is set to 24-hour time; `None` where the platform does not say.
#[command]
async fn hour_clock<R: Runtime>(app: AppHandle<R>) -> Result<Option<bool>, Error> {
    let clock: Option<Clock> = app.state::<System<R>>().native("hourClock", ())?;
    Ok(clock.and_then(|c| c.is24))
}

#[command]
async fn open_app_settings<R: Runtime>(app: AppHandle<R>) -> Result<(), Error> {
    let _: Option<serde_json::Value> = app.state::<System<R>>().run("openAppSettings")?;
    Ok(())
}

/// The shortcut the app was opened from, once.
#[command]
async fn take_launch<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, Error> {
    let route: Option<Route> = app.state::<System<R>>().run("takeLaunch")?;
    Ok(route
        .and_then(|r| r.route)
        .filter(|r| ROUTES.contains(&r.as_str())))
}

/// What another app shared, once. Its files wait in the app's own space until `shared_done`.
#[command]
async fn take_shared<R: Runtime>(app: AppHandle<R>) -> Result<Option<Shared>, Error> {
    let state = app.state::<System<R>>();
    let answer: Option<SharedAnswer> = state.run("takeShared")?;
    let Some(on_phone) = answer.and_then(|a| a.shared) else {
        return Ok(None);
    };
    *state.1.lock().map_err(|_| Error("busy".into()))? = Some(PathBuf::from(&on_phone.dir));
    Ok(Some(Shared {
        text: on_phone.text,
        subject: on_phone.subject,
        files: on_phone.files,
    }))
}

/// One shared file's bytes, by its name in the shared folder.
#[command]
async fn shared_file<R: Runtime>(app: AppHandle<R>, name: String) -> Result<Response, Error> {
    let dir = app
        .state::<System<R>>()
        .1
        .lock()
        .map_err(|_| Error("busy".into()))?
        .clone()
        .ok_or_else(|| Error("nothing shared".into()))?;
    let path = inside(&dir, &name).ok_or_else(|| Error("not shared".into()))?;
    std::fs::read(path)
        .map(Response::new)
        .map_err(|e| Error(e.to_string()))
}

/// The shared files are kept or let go: their copies leave the app's space.
#[command]
async fn shared_done<R: Runtime>(app: AppHandle<R>) -> Result<(), Error> {
    let dir = app
        .state::<System<R>>()
        .1
        .lock()
        .map_err(|_| Error("busy".into()))?
        .take();
    if let Some(dir) = dir {
        let _ = std::fs::remove_dir_all(dir);
    }
    Ok(())
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("nocturne-system")
        .invoke_handler(tauri::generate_handler![
            insets,
            device_name,
            hour_clock,
            open_app_settings,
            take_launch,
            take_shared,
            shared_file,
            shared_done
        ])
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            let handle = Some(_api.register_android_plugin("app.nocturne.system", "SystemPlugin")?);
            #[cfg(not(target_os = "android"))]
            let handle: Option<PluginHandle<R>> = None;
            #[cfg(target_os = "ios")]
            let ios = Some(_api.register_ios_plugin(init_plugin_nocturne_system)?);
            #[cfg(not(target_os = "ios"))]
            let ios: Option<PluginHandle<R>> = None;
            app.manage(System(handle, Mutex::new(None), ios));
            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_plain_name_reads_a_shared_file() {
        let dir = Path::new("/cache/shared");
        assert_eq!(
            inside(dir, "0-letter.jpg"),
            Some(PathBuf::from("/cache/shared/0-letter.jpg"))
        );
        for bad in ["", "..", ".hidden", "../key", "a/b", "a\\b"] {
            assert_eq!(inside(dir, bad), None, "{bad}");
        }
    }

    #[test]
    fn every_sheet_outcome_reads_from_the_phone() {
        let read = |json: &str| serde_json::from_str::<Sheet>(json).unwrap();
        assert_eq!(read(r#"{"outcome":"saved"}"#), Sheet::Saved);
        assert_eq!(read(r#"{"outcome":"cancelled"}"#), Sheet::Cancelled);
        assert_eq!(read(r#"{"outcome":"full"}"#), Sheet::Full);
        assert_eq!(read(r#"{"outcome":"failed"}"#), Sheet::Failed);
        let chosen = read(
            r#"{"outcome":"chosen","path":"/c/backup-in/chosen","name":"Nocturne backup.nocturne",
            "size":2048,"modified":1763800000000}"#,
        );
        assert_eq!(
            chosen,
            Sheet::Chosen(Chosen {
                path: "/c/backup-in/chosen".into(),
                name: "Nocturne backup.nocturne".into(),
                size: 2048,
                modified: Some(1_763_800_000_000),
            })
        );
        let undated = read(r#"{"outcome":"chosen","path":"/c/x","name":"","size":1}"#);
        assert!(matches!(
            undated,
            Sheet::Chosen(Chosen { modified: None, .. })
        ));
    }

    #[test]
    fn power_reads_from_the_phone() {
        let read = |json: &str| serde_json::from_str::<Power>(json).unwrap();
        assert_eq!(
            read(r#"{"charging":true,"window":true}"#),
            Power {
                charging: Some(true),
                window: true,
                unmetered: None,
            }
        );
        assert_eq!(
            read(r#"{"charging":null,"window":false}"#),
            Power::default()
        );
        assert_eq!(
            read(r#"{"charging":false,"window":false,"unmetered":true}"#).unmetered,
            Some(true)
        );
        assert_eq!(read("{}"), Power::default());
    }

    #[test]
    fn free_space_may_be_unknown() {
        let known: Free = serde_json::from_str(r#"{"bytes":524288000}"#).unwrap();
        assert_eq!(known.bytes, Some(524_288_000));
        let unknown: Free = serde_json::from_str("{}").unwrap();
        assert_eq!(unknown.bytes, None);
    }

    #[test]
    fn a_share_reads_from_the_phone() {
        let answer: SharedAnswer = serde_json::from_str(
            r#"{"shared":{"text":"https://example.org","subject":null,"dir":"/c/shared",
            "files":[{"name":"0-a.pdf","shown":"a.pdf","mime":"application/pdf","size":12}]}}"#,
        )
        .unwrap();
        let on_phone = answer.shared.unwrap();
        assert_eq!(on_phone.files[0].shown, "a.pdf");
        assert_eq!(on_phone.dir, "/c/shared");
        let none: SharedAnswer = serde_json::from_str(r#"{"shared":null}"#).unwrap();
        assert!(none.shared.is_none());
    }
}
