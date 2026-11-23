//! The engine route for Online answers: one provider entry in the engine's `config.json`, one
//! static model row in its `user_models.json`, and the key handed to the running engine over
//! loopback (`POST /v1/cloud/auth`, admin key), held in its memory only. The key is never in
//! `lemond`'s environment, so a crash record of the process environment and the model servers
//! it starts never hold it. While Online answers is off none of the three exists.
//!
//! Handing over the key makes the engine read the provider's list of models once, and drops the
//! static row from its model list (`CloudRows::refresh`). So the row's model is loaded first: a
//! loaded model is served without the list, and reads the key on every request
//! (`model_loader.cpp:33-48`, `cloud_server.cpp:382-386, 407-431` in lemonade v2026.47.1).
//!
//! The engine posts to the provider without following redirects (`utils/http_client.cpp:798`
//! passes `follow_redirects = false` to `apply_http_security_policy`, `:645`), so a redirect
//! from the provider never carries the key elsewhere; it comes back as a failed request.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use url::Url;

use crate::error::RuntimeError;

pub use nocturne_engine::OnlineFailure;
pub use nocturne_provider::address::{
    Address, Lookup, NEVER_NAMES, NEVER_NETS, OWN_NAMES, OWN_NETS, OnlineKey, OwnPort, Reach,
    UrlProblem, host_of, plain_http_allowed, resolved_reach, system_lookup, validate_url,
};

/// The provider's name in `cloud_providers`. Underscore only, so the key's environment name is
/// settable everywhere.
pub const PROVIDER: &str = "nocturne_online";
/// The environment variable the engine would read the key from. Nocturne never sets it: the
/// key goes to the running engine instead (see [`give`]).
pub const KEY_ENV: &str = "LEMONADE_NOCTURNE_ONLINE_API_KEY";
/// The static row's key in `user_models.json`.
pub const ROW: &str = "nocturne-online";
/// The model id Nocturne calls: the engine prefixes registered rows with `user.`.
pub const MODEL_ID: &str = "user.nocturne-online";
pub use nocturne_mind::online::CHECK_QUESTION;
pub use nocturne_mind::online::MAX_TOKENS;
/// After this long without an answer the person may switch to this computer.
pub const WAIT_SECONDS: u64 = 6;
/// After this long without an answer this computer answers instead.
pub const SWITCH_SECONDS: u64 = 20;
pub const SENT_KEPT_DAYS: u32 = 90;
pub use nocturne_mind::online::MAX_FACTS;

/// A relaunch for Online answers waits at most this long for model requests in flight to end
/// when turning off, and [`RELAUNCH_IDLE_WAIT_ON`] for a check or turning on.
pub const RELAUNCH_IDLE_WAIT_OFF: Duration = Duration::from_secs(120);
pub const RELAUNCH_IDLE_WAIT_ON: Duration = Duration::from_secs(30);

const USER_MODELS: &str = "user_models.json";

/// What the engine needs to reach the person's provider. Built only from an address that passed
/// [`validate_url`], so the normalised address is the only one the engine is given.
#[derive(Debug, Clone)]
pub struct CloudRoute {
    base_url: Url,
    pub model: String,
    pub key: OnlineKey,
}

impl CloudRoute {
    pub fn new(base_url: &str, model: String, key: OnlineKey) -> Result<Self, UrlProblem> {
        Ok(CloudRoute {
            base_url: validate_url(base_url)?.url,
            model,
            key,
        })
    }

    /// A route to the stand-in provider. The address is judged as any other.
    #[cfg(feature = "online-stand-in")]
    pub fn stand_in(base_url: &str, model: String, key: OnlineKey) -> Result<Self, UrlProblem> {
        tracing::warn!("{STAND_IN_MARKER}: answers go to a stand-in provider");
        CloudRoute::new(base_url, model, key)
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Plain HTTP, which [`validate_url`] takes only for this computer or the person's own
    /// network.
    pub fn allow_insecure_http(&self) -> bool {
        self.base_url.scheme() == "http"
    }

    /// The port, when the address names this computer: the engine never listens on it.
    pub fn loopback_port(&self) -> Option<u16> {
        nocturne_provider::address::loopback_port(&self.base_url)
    }
}

/// Present only in builds with the `online-stand-in` feature, so the package check can refuse them.
#[cfg(feature = "online-stand-in")]
pub const STAND_IN_MARKER: &str = "nocturne-online-stand-in-marker";

/// The provider entry `config.json` carries for `route`.
pub fn entry(route: &CloudRoute) -> crate::config::CloudEntry<'_> {
    crate::config::CloudEntry {
        base_url: route.base_url().as_str(),
        allow_insecure_http: route.allow_insecure_http(),
    }
}

/// The static row for `route`: a model served by the provider, never downloaded.
fn row(route: &CloudRoute) -> Value {
    serde_json::json!({
        "recipe": "cloud",
        "cloud_provider": PROVIDER,
        "checkpoint": route.model,
        "labels": ["chat"],
        "suggested": false,
    })
}

/// Adds or removes the static row in `<engine_dir>/user_models.json`, keeping every other row.
/// Called only from the supervisor's launch, beside the `config.json` entry.
pub fn write_row(engine_dir: &Path, route: Option<&CloudRoute>) -> Result<(), RuntimeError> {
    let path = engine_dir.join(USER_MODELS);
    let mut rows = match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(Value::Object(rows)) => rows,
            // A file the engine could not read either: with no route there is nothing to take
            // out of it, and a route replaces it.
            _ if route.is_none() => return Ok(()),
            _ => serde_json::Map::new(),
        },
        Err(_) if route.is_none() => return Ok(()),
        Err(_) => serde_json::Map::new(),
    };
    match route {
        Some(r) => {
            let wanted = row(r);
            if rows.get(ROW) == Some(&wanted) {
                return Ok(());
            }
            rows.insert(ROW.to_string(), wanted);
        }
        None => {
            if rows.remove(ROW).is_none() {
                return Ok(());
            }
        }
    }
    std::fs::create_dir_all(engine_dir)?;
    let tmp = engine_dir.join(format!("{USER_MODELS}.tmp"));
    let text = serde_json::to_string_pretty(&Value::Object(rows))
        .map_err(|e| RuntimeError::Io(e.to_string()))?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Gives the engine that `admin` reaches the route's key, before anything else may use it:
/// loads the static row's model (no request to the provider), hands over the key (the engine
/// reads the provider's list once), then confirms the key is held in memory and the model still
/// loaded. Returns how many chat models the provider listed, or `None` when `still` said the
/// route was changed meanwhile (turned off, a new one, Delete everything): the key is then not
/// handed over, or taken back at once. `still` is asked before the load, between the load and
/// the key, and after. On an error or `None` the caller takes the route back with [`take_back`].
pub(crate) async fn give(
    admin: &crate::Admin,
    route: &CloudRoute,
    still: &(dyn Fn() -> bool + Send + Sync),
) -> Result<Option<u64>, RuntimeError> {
    if !still() {
        return Ok(None);
    }
    admin.load(MODEL_ID).await?;
    if !still() {
        return Ok(None);
    }
    let auth = admin.cloud_auth(PROVIDER, &route.key).await?;
    if !still() {
        return Ok(None);
    }
    if !auth.runtime_key_set || auth.env_var_set {
        return Err(RuntimeError::RouteRefused);
    }
    let health = admin.health().await?;
    if !health
        .all_models_loaded
        .iter()
        .any(|m| m.model_name == MODEL_ID)
    {
        return Err(RuntimeError::RouteRefused);
    }
    Ok(Some(auth.models_discovered))
}

/// Makes the engine forget the key and unload the online model, so nothing can be sent with
/// it. Best effort: an engine that does not answer is about to be stopped anyway.
pub(crate) async fn take_back(admin: &crate::Admin) {
    if let Err(e) = admin.cloud_forget(PROVIDER).await {
        tracing::warn!(
            "the engine did not confirm it forgot the Online answers key: {}",
            kind_of(&e)
        );
    }
    let _ = admin.unload(MODEL_ID).await;
}

/// What went wrong, for the app's log: a kind or a status, never the engine's own words, which
/// may quote what a provider sent.
pub fn kind_of(e: &RuntimeError) -> String {
    match e {
        RuntimeError::Rejected { status, .. } | RuntimeError::Server { status, .. } => {
            format!("status {status}")
        }
        RuntimeError::NotFound(_) => "status 404".into(),
        RuntimeError::Unauthorized => "the engine refused its key".into(),
        RuntimeError::Unavailable => "the engine is not running".into(),
        RuntimeError::Timeout => "the engine took too long".into(),
        RuntimeError::RouteRefused => "the route was refused".into(),
        RuntimeError::BadResponse(_) => "an unexpected answer".into(),
        _ => "another failure".into(),
    }
}

/// Phrases of the engine's own network errors (libcurl's, behind "CURL error:") that mean this
/// computer has no way out.
const NO_INTERNET: &[&str] = &[
    "could not resolve",
    "couldn't resolve",
    "resolve host",
    "network is unreachable",
    "no route to host",
];

/// Phrases that mean the provider's address was found but did not answer.
const NOT_REACHED: &[&str] = &[
    "couldn't connect",
    "could not connect",
    "connection refused",
    "timeout was reached",
    "timed out",
    "ssl",
    "certificate",
    "network error",
    "curl error",
];

/// What a failed online request means, from the engine's status and its own error text (the
/// `error.message` it wrote, never the provider's body). `None` means the engine itself did
/// not answer.
pub fn classify(status: Option<u16>, engine_error: &str) -> OnlineFailure {
    let text = engine_error.to_ascii_lowercase();
    let says = |words: &[&str]| words.iter().any(|w| text.contains(w));
    match status {
        None => OnlineFailure::Unreachable,
        Some(401 | 403) => OnlineFailure::KeyRefused,
        Some(402) => OnlineFailure::NoCredit,
        Some(404) => OnlineFailure::ModelMissing,
        Some(429) => OnlineFailure::Busy,
        Some(_) if says(NO_INTERNET) => OnlineFailure::NoInternet,
        Some(_) if says(NOT_REACHED) => OnlineFailure::Unreachable,
        Some(400) if text.contains("model") => OnlineFailure::ModelMissing,
        Some(s @ 500..=599) => OnlineFailure::Other(s),
        Some(_) => OnlineFailure::Refused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_and_display_hide_the_key() {
        let key = OnlineKey::new("sk-or-v1-0123456789abcdef".into());
        assert_eq!(format!("{key:?}"), "[key]");
        assert_eq!(format!("{key}"), "[key]");
        let route = CloudRoute::new("https://api.example.com/v1", "m".into(), key.clone()).unwrap();
        assert!(!format!("{route:?}").contains("0123456789abcdef"));
        assert_eq!(key.expose(), "sk-or-v1-0123456789abcdef");
    }

    #[test]
    fn a_route_holds_only_the_normalised_address() {
        let key = OnlineKey::new("k".into());
        let route =
            CloudRoute::new("https://API.example.com./v1", "m".into(), key.clone()).unwrap();
        assert_eq!(route.base_url().as_str(), "https://api.example.com/v1");
        assert!(!route.allow_insecure_http());
        assert_eq!(route.loopback_port(), None);
        assert!(CloudRoute::new("http://api.example.com/v1", "m".into(), key.clone()).is_err());
        let own = CloudRoute::new("http://192.168.1.10:8000/v1", "m".into(), key).unwrap();
        assert!(own.allow_insecure_http());
        assert_eq!(own.loopback_port(), None);
    }

    #[test]
    fn the_env_name_follows_the_provider_name() {
        assert_eq!(
            KEY_ENV,
            format!("LEMONADE_{}_API_KEY", PROVIDER.to_uppercase())
        );
        assert_eq!(MODEL_ID, format!("user.{ROW}"));
    }

    #[test]
    fn errors_are_logged_by_kind_without_the_engines_words() {
        let words = "sk-or-v1-echoed-by-the-provider";
        for e in [
            RuntimeError::Rejected {
                status: 409,
                code: "auth_conflict".into(),
                message: words.into(),
            },
            RuntimeError::Server {
                status: 500,
                message: words.into(),
            },
            RuntimeError::NotFound(words.into()),
            RuntimeError::BadResponse(words.into()),
            RuntimeError::Io(words.into()),
            RuntimeError::Start(words.into()),
            RuntimeError::LoadFailed {
                out_of_memory: false,
                message: words.into(),
            },
        ] {
            assert!(!kind_of(&e).contains(words), "{e:?}");
        }
        assert_eq!(
            kind_of(&RuntimeError::Server {
                status: 502,
                message: String::new()
            }),
            "status 502"
        );
    }

    #[test]
    fn the_row_is_removed_and_other_rows_kept() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(USER_MODELS),
            r#"{"nocturne-online": {"recipe": "cloud"}, "mine": {"recipe": "llamacpp"}}"#,
        )
        .unwrap();
        write_row(tmp.path(), None).unwrap();
        let v: Value =
            serde_json::from_slice(&std::fs::read(tmp.path().join(USER_MODELS)).unwrap()).unwrap();
        assert!(v.get(ROW).is_none());
        assert_eq!(v["mine"]["recipe"], "llamacpp");
        write_row(&tmp.path().join("absent"), None).unwrap();
    }
}
