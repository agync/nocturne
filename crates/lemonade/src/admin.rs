//! The engine's `/internal/*` endpoints. They take a separate key that only the supervisor
//! and the parts that change engine settings hold; the inference [`Client`](crate::Client)
//! never sees it, so a bug or a leak there cannot reach these.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use reqwest::Method;
use serde_json::{Value, json};

use crate::client::{Client, Health, Timeouts};
use crate::error::RuntimeError;
use crate::online::OnlineKey;

/// Longest wait for `POST /v1/cloud/auth`, which waits for the provider's list (15 s at most in
/// the engine).
const CLOUD_AUTH_TIMEOUT: Duration = Duration::from_secs(40);
/// Longest wait for the other calls that give or take back the route.
const ROUTE_CALL_TIMEOUT: Duration = Duration::from_secs(20);

/// What the engine says after it was given a provider's key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CloudAuth {
    /// The key came from the engine's environment.
    pub env_var_set: bool,
    /// The key is held in the engine's memory.
    pub runtime_key_set: bool,
    /// Chat models in the provider's list, 0 when the list could not be read.
    pub models_discovered: u64,
}

/// The pair of keys one launch of the engine is given. They are never equal and never empty.
#[derive(Clone)]
pub(crate) struct Keys {
    pub api: String,
    pub admin: String,
}

impl Keys {
    /// Refuses an empty key (the engine reads an empty admin key as "no check") and two equal ones.
    pub(crate) fn new(api: String, admin: String) -> Result<Keys, RuntimeError> {
        if api.is_empty() || admin.is_empty() {
            return Err(RuntimeError::Start("an engine key was empty".into()));
        }
        if api == admin {
            return Err(RuntimeError::Start(
                "the admin key must differ from the API key".into(),
            ));
        }
        Ok(Keys { api, admin })
    }
}

#[derive(Clone)]
struct AdminEndpoint {
    port: u16,
    key: String,
}

struct Shared {
    http: reqwest::Client,
    endpoint: RwLock<Option<AdminEndpoint>>,
}

/// A handle on `/internal/*` that follows restarts, like the [`Client`] it comes with.
#[derive(Clone)]
pub struct Admin {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Admin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Admin").finish_non_exhaustive()
    }
}

impl Admin {
    /// An admin handle for an engine already known to be at `port`. An empty key is refused.
    pub fn new(port: u16, key: impl Into<String>) -> Result<Admin, RuntimeError> {
        let admin = Admin::detached();
        admin.set_endpoint(Some((port, key.into())))?;
        Ok(admin)
    }

    pub(crate) fn detached() -> Admin {
        let timeouts = Timeouts::default();
        let http = reqwest::Client::builder()
            .connect_timeout(timeouts.connect)
            .timeout(timeouts.request)
            .no_proxy()
            .build()
            .expect("http client");
        Admin {
            shared: Arc::new(Shared {
                http,
                endpoint: RwLock::new(None),
            }),
        }
    }

    pub(crate) fn set_endpoint(&self, endpoint: Option<(u16, String)>) -> Result<(), RuntimeError> {
        let endpoint = match endpoint {
            Some((_, key)) if key.is_empty() => {
                return Err(RuntimeError::Start("the admin key was empty".into()));
            }
            Some((port, key)) => Some(AdminEndpoint { port, key }),
            None => None,
        };
        *self
            .shared
            .endpoint
            .write()
            .unwrap_or_else(|e| e.into_inner()) = endpoint;
        Ok(())
    }

    fn request(&self, method: Method, path: &str) -> Result<reqwest::RequestBuilder, RuntimeError> {
        let endpoint = self
            .shared
            .endpoint
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or(RuntimeError::Unavailable)?;
        let url = format!("http://127.0.0.1:{}{path}", endpoint.port);
        Ok(self
            .shared
            .http
            .request(method, url)
            .bearer_auth(endpoint.key))
    }

    /// `POST /internal/set`: changes `config.json` settings without a restart.
    pub async fn set(&self, settings: &Value) -> Result<(), RuntimeError> {
        let rb = self.request(Method::POST, "/internal/set")?.json(settings);
        let _: Value = Client::json(rb).await?;
        Ok(())
    }

    /// `GET /internal/config`: the settings in force.
    pub async fn config(&self) -> Result<Value, RuntimeError> {
        Client::json(self.request(Method::GET, "/internal/config")?).await
    }

    /// `POST /v1/cloud/auth`: hands the engine `key` for `provider`, held in its memory only. The
    /// engine then reads the provider's list of models once, with the key.
    pub async fn cloud_auth(
        &self,
        provider: &str,
        key: &OnlineKey,
    ) -> Result<CloudAuth, RuntimeError> {
        let body = json!({"provider": provider, "api_key": key.expose()});
        let rb = self
            .request(Method::POST, "/v1/cloud/auth")?
            .timeout(CLOUD_AUTH_TIMEOUT)
            .json(&body);
        let v: Value = Client::json(rb).await?;
        let state = &v["auth_state"];
        Ok(CloudAuth {
            env_var_set: state["env_var_set"].as_bool().unwrap_or(true),
            runtime_key_set: state["runtime_key_set"].as_bool().unwrap_or(false),
            models_discovered: v["models_discovered"].as_u64().unwrap_or(0),
        })
    }

    /// `DELETE /v1/cloud/auth/{provider}`: the engine forgets the key it holds for `provider`.
    pub async fn cloud_forget(&self, provider: &str) -> Result<(), RuntimeError> {
        let rb = self
            .request(Method::DELETE, &format!("/v1/cloud/auth/{provider}"))?
            .timeout(ROUTE_CALL_TIMEOUT);
        let _: Value = Client::json(rb).await?;
        Ok(())
    }

    /// `POST /v1/load` of a model that needs no options, such as one a provider serves.
    pub async fn load(&self, model: &str) -> Result<(), RuntimeError> {
        let rb = self
            .request(Method::POST, "/v1/load")?
            .timeout(ROUTE_CALL_TIMEOUT)
            .json(&json!({"model_name": model}));
        let _: Value = Client::json(rb).await?;
        Ok(())
    }

    /// `POST /v1/unload` of one model.
    pub async fn unload(&self, model: &str) -> Result<(), RuntimeError> {
        let rb = self
            .request(Method::POST, "/v1/unload")?
            .timeout(ROUTE_CALL_TIMEOUT)
            .json(&json!({"model_name": model}));
        let _: Value = Client::json(rb).await?;
        Ok(())
    }

    /// `GET /v1/health`, with the admin key.
    pub async fn health(&self) -> Result<Health, RuntimeError> {
        Client::json(
            self.request(Method::GET, "/v1/health")?
                .timeout(ROUTE_CALL_TIMEOUT),
        )
        .await
    }

    /// `POST /internal/shutdown`: unloads every model and stops the engine.
    pub async fn shutdown(&self) -> Result<(), RuntimeError> {
        let rb = self
            .request(Method::POST, "/internal/shutdown")?
            .timeout(Duration::from_secs(2))
            .json(&json!({}));
        let _: Value = Client::json(rb).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_admin_key_cannot_be_held() {
        assert!(Admin::new(1, "").is_err());
        let admin = Admin::detached();
        assert!(admin.set_endpoint(Some((1, String::new()))).is_err());
    }

    #[test]
    fn keys_are_never_empty_or_equal() {
        assert!(Keys::new("a".into(), String::new()).is_err());
        assert!(Keys::new(String::new(), "b".into()).is_err());
        assert!(Keys::new("a".into(), "a".into()).is_err());
        assert!(Keys::new("a".into(), "b".into()).is_ok());
    }
}
