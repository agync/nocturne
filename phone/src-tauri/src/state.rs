//! The app's state: the store, the pairing in progress, and the phone once paired.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use nocturne_channel::client::{self, Connection, OnIdentity, Options, Phone, Router};
use nocturne_channel::phone::{PhoneIdentity, PhonePairing};
use tauri::{AppHandle, Emitter, Runtime};

use crate::store::{Meta, Store};
use crate::uploads::Uploads;

pub struct AppState {
    pub store: Arc<Store>,
    pub router: Router,
    pub http: client::HttpClient,
    pub pairing: Mutex<Option<PhonePairing>>,
    pub phone: Mutex<Option<Phone>>,
    pub uploads: Mutex<Uploads>,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// Where frames go: the computer's https address, or loopback in a `dev-loopback` build.
pub fn router() -> Router {
    #[cfg(feature = "dev-loopback")]
    {
        let port = option_env!("NOCTURNE_DEV_LOOPBACK_PORT")
            .and_then(|p| p.parse().ok())
            .unwrap_or(8799);
        client::loopback_router(port)
    }
    #[cfg(not(feature = "dev-loopback"))]
    client::https_router()
}

impl AppState {
    pub fn new(dir: PathBuf) -> AppState {
        AppState {
            store: Arc::new(Store::new(dir)),
            router: router(),
            http: client::http_client(),
            pairing: Mutex::new(None),
            phone: Mutex::new(None),
            uploads: Mutex::new(Uploads::default()),
        }
    }

    /// The phone for the kept identity, made once.
    pub fn phone<R: Runtime>(&self, app: &AppHandle<R>) -> Option<Phone> {
        let mut slot = self.phone.lock().unwrap();
        if slot.is_none() {
            let identity = self.store.identity()?;
            *slot = Some(self.make_phone(app, identity));
        }
        slot.clone()
    }

    /// A phone for a new identity, replacing any other.
    pub fn adopt<R: Runtime>(&self, app: &AppHandle<R>, identity: PhoneIdentity) -> Phone {
        let phone = self.make_phone(app, identity);
        *self.phone.lock().unwrap() = Some(phone.clone());
        phone
    }

    pub fn drop_phone(&self) {
        self.phone.lock().unwrap().take();
    }

    fn make_phone<R: Runtime>(&self, app: &AppHandle<R>, identity: PhoneIdentity) -> Phone {
        let store = self.store.clone();
        let on_identity: OnIdentity = Arc::new(move |i: Option<&PhoneIdentity>| match i {
            Some(i) => {
                if let Err(e) = store.save_identity(i) {
                    tracing::warn!("could not keep the new address: {e}");
                }
            }
            None => store.clear(),
        });
        let phone = Phone::new(
            identity,
            self.router.clone(),
            Options::default(),
            on_identity,
        );
        let mut states = phone.connection();
        let store = self.store.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let state = states.borrow_and_update().clone();
                if let Connection::Connected { hello } = &state
                    && store.identity().is_some()
                {
                    let meta = Meta {
                        computer_name: Some(hello.computer_name.clone()),
                        connection: Some(hello.connection),
                        version: Some(hello.version.clone()),
                        last_connected_at: Some(now_ms()),
                        ..store.meta()
                    };
                    let _ = store.save_meta(&meta);
                }
                let _ = app.emit("connection", &state);
                if states.changed().await.is_err() {
                    break;
                }
            }
        });
        phone
    }
}
