//! The phone's own bits Nocturne needs: the screen insets as CSS variables (the app draws edge
//! to edge), the phone's name for the default device name, this app's settings page for a
//! camera turned off there, and what the app was opened for from outside (a shortcut on its
//! icon, or another app's share menu), and on an iPhone a folder kept out of its backup. On a
//! computer every command answers a stand-in.

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

/// The Android side, the shared folder, and the iPhone side, which so far only keeps folders out
/// of the backup: every other command answers its stand-in there.
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

    /// Keep a folder of the app's out of the phone's own backup. Only an iPhone needs asking:
    /// Android's backup rules already leave out every folder of the app.
    pub fn exclude_from_backup(&self, path: &Path) -> Result<(), Error> {
        let path = path
            .to_str()
            .ok_or_else(|| Error("not a plain path".into()))?;
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
