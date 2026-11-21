//! The phone's own bits Nocturne needs: the screen insets as CSS variables (the app draws edge
//! to edge), the phone's name for the default device name, and this app's settings page for a
//! camera turned off there. On a computer every command answers a stand-in.

use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{AppHandle, Manager, Runtime, command};

/// The system bars and cut-outs, in CSS pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Insets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

#[derive(Debug, Deserialize)]
struct Named {
    name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Error(String);

pub struct System<R: Runtime>(Option<PluginHandle<R>>);

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
}

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

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("nocturne-system")
        .invoke_handler(tauri::generate_handler![
            insets,
            device_name,
            open_app_settings
        ])
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            let handle = Some(_api.register_android_plugin("app.nocturne.system", "SystemPlugin")?);
            #[cfg(not(target_os = "android"))]
            let handle: Option<PluginHandle<R>> = None;
            app.manage(System(handle));
            Ok(())
        })
        .build()
}
