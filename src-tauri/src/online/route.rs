//! How the hub reaches the private engine: giving it the route (a relaunch), a client for
//! questions, and whether a download holds the window open. The app's port goes through the
//! supervisor; tests use the one in [`super::testing`].

use std::sync::Arc;

use nocturne_contract::LocalReason;
use nocturne_engine::BoxFuture;
use nocturne_lemonade::Client;
use nocturne_lemonade::mind::online::OnlineMind;
use nocturne_lemonade::online::CloudRoute;
use tauri::{AppHandle, Manager, Runtime};

use crate::state::AppState;

pub trait EnginePort: Send + Sync {
    /// Gives the engine this route (`None`: off) and relaunches it. With no engine running,
    /// the next launch takes it from [`super::route_at_launch`].
    fn set_cloud(&self, route: Option<CloudRoute>) -> BoxFuture<'_, Result<(), String>>;
    /// A client for the running engine.
    fn client(&self) -> BoxFuture<'_, Option<Client>>;
    /// A download the person agreed to holds the window open: the engine runs without the
    /// route meanwhile.
    fn downloading(&self) -> BoxFuture<'_, bool>;
    /// Whether the running engine was given a route, or `None` with no engine running.
    fn has_cloud(&self) -> BoxFuture<'_, Option<bool>>;
    /// Whether the running engine holds its route: it took the key and loaded the online model.
    /// `None` with no engine running.
    fn route_taken(&self) -> BoxFuture<'_, Option<bool>>;
}

/// Where one question goes.
pub enum RouteChoice {
    /// Online answers is off (or off for this lane): nothing about it is said.
    Off,
    /// Written online by the provider at `host`.
    Online {
        mind: Arc<OnlineMind>,
        host: String,
        send_sensitive: bool,
        /// Counted against the month's limit until dropped.
        reservation: super::controller::Reservation,
    },
    /// Answered on this computer, and why.
    Computer(LocalReason),
}

/// The app's engine, through the supervisor once it runs.
pub struct AppPort<R: Runtime> {
    pub app: AppHandle<R>,
}

impl<R: Runtime> AppPort<R> {
    async fn control(&self) -> Option<nocturne_lemonade::Control> {
        let services = self.app.try_state::<AppState>()?.services()?;
        services.engine_control().await
    }
}

impl<R: Runtime> EnginePort for AppPort<R> {
    fn set_cloud(&self, route: Option<CloudRoute>) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            match self.control().await {
                Some(c) => c.set_cloud(route).await.map_err(|e| e.to_string()),
                None => Ok(()),
            }
        })
    }

    fn client(&self) -> BoxFuture<'_, Option<Client>> {
        Box::pin(async move { self.control().await.map(|c| c.client()) })
    }

    fn downloading(&self) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            self.control()
                .await
                .is_some_and(|c| c.download_window_open())
        })
    }

    fn has_cloud(&self) -> BoxFuture<'_, Option<bool>> {
        Box::pin(async move { self.control().await.map(|c| c.has_cloud()) })
    }

    fn route_taken(&self) -> BoxFuture<'_, Option<bool>> {
        Box::pin(async move { self.control().await.map(|c| c.route_taken()) })
    }
}
