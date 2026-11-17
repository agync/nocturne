//! The one seam through which the app starts something on the person's desktop: a folder in the
//! file manager, a page in the browser, the sound settings.
//!
//! `run` installs the real one from `RunEvent::Ready`, never from setup: tests run setup, and
//! must not open anything on the machine they run on. A command that launches something asks
//! this module for the seam and says `unavailable` when it is absent. A test that wants to see
//! what would have been opened installs a recording one.

use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime};

/// What the app asks the desktop to open. Errors are the sentence for the person, or a log line.
pub trait Desktop: Send + Sync {
    /// Open a file or folder with the system's own app.
    fn open_path(&self, path: &Path) -> Result<(), String>;
    /// Open an `https` address in the person's browser.
    fn open_url(&self, url: &str) -> Result<(), String>;
    /// Open the system's sound settings, where the microphone is allowed.
    fn open_sound_settings(&self) -> Result<(), String>;
}

/// The seam as Tauri manages it.
pub struct Seam(pub Arc<dyn Desktop>);

/// The installed seam, or `None` where nothing may be launched (tests, a browser preview).
pub fn get<R: Runtime>(app: &AppHandle<R>) -> Option<Arc<dyn Desktop>> {
    app.try_state::<Seam>().map(|s| s.0.clone())
}

/// Install the seam. The first one stays: a later call changes nothing.
pub fn install<R: Runtime>(app: &AppHandle<R>, desktop: Arc<dyn Desktop>) {
    if app.try_state::<Seam>().is_none() {
        app.manage(Seam(desktop));
    }
}

/// The real desktop: the opener plugin for paths and addresses, the sound settings program.
pub struct System<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> Desktop for System<R> {
    fn open_path(&self, path: &Path) -> Result<(), String> {
        use tauri_plugin_opener::OpenerExt;
        self.0
            .opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|e| e.to_string())
    }

    fn open_url(&self, url: &str) -> Result<(), String> {
        use tauri_plugin_opener::OpenerExt;
        self.0
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| e.to_string())
    }

    fn open_sound_settings(&self) -> Result<(), String> {
        crate::os::sound::open()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
