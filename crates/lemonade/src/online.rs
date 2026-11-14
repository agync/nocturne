//! The engine route for Online answers: one provider entry in the engine's `config.json`, one
//! static model row in its `user_models.json`, and the key in `lemond`'s spawn environment only.
//! While Online answers is off none of the three exists.
//!
//! The engine posts to the provider without following redirects (`utils/http_client.cpp:798`
//! passes `follow_redirects = false` to `apply_http_security_policy`, `:645`), so a redirect
//! from the provider never carries the key elsewhere; it comes back as a failed request.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use url::{Host, Url};
use zeroize::Zeroizing;

use crate::error::RuntimeError;

pub use nocturne_engine::OnlineFailure;

/// The provider's name in `cloud_providers`. Underscore only, so the key's environment name is
/// settable everywhere.
pub const PROVIDER: &str = "nocturne_online";
/// The environment variable the engine reads the key from on every request.
pub const KEY_ENV: &str = "LEMONADE_NOCTURNE_ONLINE_API_KEY";
/// The static row's key in `user_models.json`.
pub const ROW: &str = "nocturne-online";
/// The model id Nocturne calls: the engine prefixes registered rows with `user.`.
pub const MODEL_ID: &str = "user.nocturne-online";
pub const MAX_TOKENS: u32 = 600;
/// The one question of a connection check.
pub const CHECK_QUESTION: &str = "Reply with the word ready.";
/// After this long without an answer the person may switch to this computer.
pub const WAIT_SECONDS: u64 = 6;
/// After this long without an answer this computer answers instead.
pub const SWITCH_SECONDS: u64 = 20;
pub const SENT_KEPT_DAYS: u32 = 90;
/// Questions a month, unless the person chose otherwise.
pub const DEFAULT_LIMIT: u32 = 200;
/// The most remembered sentences one question sends.
pub const MAX_FACTS: usize = 12;

/// A relaunch for Online answers waits at most this long for model requests in flight to end
/// when turning off, and [`RELAUNCH_IDLE_WAIT_ON`] for a check or turning on.
pub const RELAUNCH_IDLE_WAIT_OFF: Duration = Duration::from_secs(120);
pub const RELAUNCH_IDLE_WAIT_ON: Duration = Duration::from_secs(30);

/// Networks an address may never name, as `address/prefix`: loopback, link-local (169.254.169.254
/// among them), unspecified, private (RFC 1918), shared (100.64.0.0/10), multicast, broadcast
/// and reserved, unique-local and site-local IPv6, NAT64, 6to4, and the IPv4-mapped forms of the
/// private IPv4 ones. IPv4-mapped and IPv4-compatible IPv6 addresses are also checked as the IPv4
/// address they carry.
pub const REFUSED_NETS: &[&str] = &[
    "127.0.0.0/8",
    "169.254.0.0/16",
    "0.0.0.0/8",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "100.64.0.0/10",
    "224.0.0.0/4",
    "240.0.0.0/4",
    "255.255.255.255/32",
    "::1/128",
    "::/128",
    "fe80::/10",
    "fec0::/10",
    "fc00::/7",
    "ff00::/8",
    "64:ff9b::/96",
    "2002::/16",
    "::ffff:127.0.0.0/104",
    "::ffff:169.254.0.0/112",
    "::ffff:0.0.0.0/104",
    "::ffff:10.0.0.0/104",
    "::ffff:172.16.0.0/108",
    "::ffff:192.168.0.0/112",
];

/// Host names an address may never use: exact, or `*.` for any name under it. A trailing dot is
/// ignored, and a name of one label (no dot) is refused as well.
pub const REFUSED_NAMES: &[&str] = &[
    "localhost",
    "*.localhost",
    "*.local",
    "*.home.arpa",
    "*.internal",
    "*.ts.net",
];

const USER_MODELS: &str = "user_models.json";

/// The person's key. Never printed: `Debug` and `Display` show `[key]`. Wiped from memory when
/// dropped.
#[derive(Clone)]
pub struct OnlineKey(Zeroizing<String>);

impl OnlineKey {
    pub fn new(key: String) -> Self {
        OnlineKey(Zeroizing::new(key))
    }

    /// The key itself, for the spawn environment and nowhere else.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OnlineKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[key]")
    }
}

impl fmt::Display for OnlineKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[key]")
    }
}

/// What the engine needs to reach the person's provider. Built only from an address that passed
/// [`validate_url`], so the normalised address is the only one the engine is given.
#[derive(Debug, Clone)]
pub struct CloudRoute {
    base_url: Url,
    pub model: String,
    pub key: OnlineKey,
    stand_in: bool,
}

impl CloudRoute {
    pub fn new(base_url: &str, model: String, key: OnlineKey) -> Result<Self, UrlProblem> {
        Ok(CloudRoute {
            base_url: validate_url(base_url, false)?,
            model,
            key,
            stand_in: false,
        })
    }

    /// A route to the stand-in provider on loopback, over plain HTTP.
    #[cfg(feature = "online-stand-in")]
    pub fn stand_in(base_url: &str, model: String, key: OnlineKey) -> Result<Self, UrlProblem> {
        Ok(CloudRoute {
            base_url: validate_url(base_url, true)?,
            model,
            key,
            stand_in: true,
        })
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Plain HTTP to the stand-in. Never true in a build without the stand-in feature.
    pub fn allow_insecure_http(&self) -> bool {
        cfg!(feature = "online-stand-in") && self.stand_in && self.base_url.scheme() == "http"
    }
}

/// Why an address cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlProblem {
    Invalid,
    NotHttps,
    NoHost,
    /// User name or password, a fragment, a query or whitespace in the address.
    Extra,
    /// This computer, the home network, or another address never sent to.
    NotAllowed,
}

/// Checks an address the person entered and returns it normalised: `https://` with a public host,
/// nothing else in it. The host is judged as the URL parser reads it, so decimal, octal, hex and
/// shortened IPv4 forms are caught. Builds with the stand-in feature also take
/// `http://127.0.0.1:<port>/...` when `stand_in` is set.
pub fn validate_url(raw: &str, stand_in: bool) -> Result<Url, UrlProblem> {
    if raw.chars().any(char::is_whitespace) {
        return Err(UrlProblem::Extra);
    }
    let mut url = Url::parse(raw).map_err(|_| UrlProblem::Invalid)?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(UrlProblem::Extra);
    }
    if stand_in_allowed(&url, stand_in) {
        return Ok(url);
    }
    if url.scheme() != "https" {
        return Err(UrlProblem::NotHttps);
    }
    let renamed = match url.host() {
        None => return Err(UrlProblem::NoHost),
        Some(Host::Ipv4(a)) => {
            if refused_ip(IpAddr::V4(a)) {
                return Err(UrlProblem::NotAllowed);
            }
            None
        }
        Some(Host::Ipv6(a)) => {
            if refused_ip(IpAddr::V6(a)) {
                return Err(UrlProblem::NotAllowed);
            }
            None
        }
        Some(Host::Domain(name)) => {
            let bare = name.trim_end_matches('.').to_ascii_lowercase();
            if refused_name(&bare) {
                return Err(UrlProblem::NotAllowed);
            }
            (bare != name).then_some(bare)
        }
    };
    if let Some(bare) = renamed {
        url.set_host(Some(&bare)).map_err(|_| UrlProblem::Invalid)?;
    }
    Ok(url)
}

#[cfg(feature = "online-stand-in")]
fn stand_in_allowed(url: &Url, stand_in: bool) -> bool {
    stand_in
        && url.scheme() == "http"
        && url.host() == Some(Host::Ipv4(std::net::Ipv4Addr::LOCALHOST))
        && url.port().is_some()
}

#[cfg(not(feature = "online-stand-in"))]
fn stand_in_allowed(_url: &Url, _stand_in: bool) -> bool {
    false
}

fn refused_name(name: &str) -> bool {
    if name.is_empty() || !name.contains('.') {
        return true;
    }
    REFUSED_NAMES
        .iter()
        .any(|rule| match rule.strip_prefix("*.") {
            Some(suffix) => name == suffix || name.ends_with(&format!(".{suffix}")),
            None => name == *rule,
        })
}

/// The IPv4 address an IPv4-mapped (`::ffff:a.b.c.d`) or IPv4-compatible (`::a.b.c.d`) IPv6
/// address carries.
fn carried_v4(a: Ipv6Addr) -> Option<Ipv4Addr> {
    if let IpAddr::V4(v4) = a.to_canonical() {
        return Some(v4);
    }
    let s = a.segments();
    if s[..6].iter().all(|x| *x == 0) && !a.is_unspecified() && !a.is_loopback() {
        let [_, _, _, _, _, _, _, _, _, _, _, _, b0, b1, b2, b3] = a.octets();
        return Some(Ipv4Addr::new(b0, b1, b2, b3));
    }
    None
}

fn refused_ip(ip: IpAddr) -> bool {
    if let IpAddr::V6(a) = ip
        && let Some(v4) = carried_v4(a)
        && refused_ip(IpAddr::V4(v4))
    {
        return true;
    }
    REFUSED_NETS.iter().any(|net| in_net(ip, net))
}

fn in_net(ip: IpAddr, net: &str) -> bool {
    let Some((base, prefix)) = net.split_once('/') else {
        return false;
    };
    let (Ok(base), Ok(prefix)) = (base.parse::<IpAddr>(), prefix.parse::<u32>()) else {
        return false;
    };
    match (ip, base) {
        (IpAddr::V4(a), IpAddr::V4(b)) => {
            let mask = u32::MAX.checked_shl(32 - prefix).unwrap_or(0);
            u32::from(a) & mask == u32::from(b) & mask
        }
        (IpAddr::V6(a), IpAddr::V6(b)) => {
            let mask = u128::MAX.checked_shl(128 - prefix).unwrap_or(0);
            u128::from(a) & mask == u128::from(b) & mask
        }
        _ => false,
    }
}

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

/// The provider as the person knows it: the address's host, with its port when one was given.
pub fn host_of(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_string(),
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
    fn refused_nets_are_well_formed() {
        for net in REFUSED_NETS {
            let (addr, prefix) = net.split_once('/').unwrap();
            let addr: std::net::IpAddr = addr.parse().unwrap();
            let prefix: u8 = prefix.parse().unwrap();
            assert!(prefix <= if addr.is_ipv4() { 32 } else { 128 }, "{net}");
        }
        assert!(REFUSED_NETS.contains(&"169.254.0.0/16"));
        assert!(REFUSED_NAMES.iter().all(|n| !n.is_empty()));
    }

    #[test]
    fn validate_url_takes_a_public_https_address_and_normalises_it() {
        for (raw, want) in [
            ("https://api.example.com/v1", "https://api.example.com/v1"),
            ("HTTPS://API.Example.COM/v1", "https://api.example.com/v1"),
            ("https://api.example.com./v1", "https://api.example.com/v1"),
            (
                "https://api.example.com:8443/v1",
                "https://api.example.com:8443/v1",
            ),
            ("https://8.8.8.8/v1", "https://8.8.8.8/v1"),
            (
                "https://[2001:4860:4860::8888]/v1",
                "https://[2001:4860:4860::8888]/v1",
            ),
        ] {
            assert_eq!(
                validate_url(raw, false).map(|u| u.to_string()),
                Ok(want.to_string()),
                "{raw}"
            );
        }
    }

    #[test]
    fn validate_url_refuses_what_is_not_plain_https() {
        for (raw, want) in [
            ("not a url", UrlProblem::Extra),
            ("api.example.com/v1", UrlProblem::Invalid),
            ("http://api.example.com/v1", UrlProblem::NotHttps),
            ("ftp://api.example.com/v1", UrlProblem::NotHttps),
            ("https://user:pass@api.example.com/v1", UrlProblem::Extra),
            ("https://user@api.example.com/v1", UrlProblem::Extra),
            ("https://api.example.com/v1?key=x", UrlProblem::Extra),
            ("https://api.example.com/v1#top", UrlProblem::Extra),
            ("https://api.example.com/v1\n", UrlProblem::Extra),
            (" https://api.example.com/v1", UrlProblem::Extra),
        ] {
            assert_eq!(validate_url(raw, false), Err(want), "{raw}");
        }
    }

    #[test]
    fn validate_url_refuses_this_computer_and_the_home_network() {
        for raw in [
            "https://127.0.0.1/v1",
            "https://127.1/v1",
            "https://2130706433/v1",
            "https://0x7f000001/v1",
            "https://0x7f.0.0.1/v1",
            "https://017700000001/v1",
            "https://0177.0.0.1/v1",
            "https://169.254.169.254/latest",
            "https://0.0.0.0/v1",
            "https://10.1.2.3/v1",
            "https://172.20.0.1/v1",
            "https://192.168.1.1/v1",
            "https://3232235777/v1",
            "https://100.100.100.100/v1",
            "https://224.0.0.1/v1",
            "https://255.255.255.255/v1",
            "https://240.0.0.1/v1",
            "https://[::1]/v1",
            "https://[::]/v1",
            "https://[fe80::1]/v1",
            "https://[fec0::1]/v1",
            "https://[fd00::1]/v1",
            "https://[ff02::1]/v1",
            "https://[64:ff9b::a00:1]/v1",
            "https://[2002:c0a8:101::1]/v1",
            "https://[::ffff:127.0.0.1]/v1",
            "https://[::ffff:169.254.169.254]/v1",
            "https://[::ffff:100.64.0.1]/v1",
            "https://[::127.0.0.1]/v1",
            "https://[::10.0.0.1]/v1",
            "https://localhost/v1",
            "https://localhost./v1",
            "https://api.localhost/v1",
            "https://printer.local/v1",
            "https://printer.local./v1",
            "https://nas.home.arpa/v1",
            "https://metadata.google.internal/v1",
            "https://box.tail1234.ts.net/v1",
            "https://intranet/v1",
            "https://intranet./v1",
        ] {
            assert_eq!(
                validate_url(raw, false),
                Err(UrlProblem::NotAllowed),
                "{raw}"
            );
        }
    }

    #[test]
    fn a_route_holds_only_the_normalised_address() {
        let key = OnlineKey::new("k".into());
        let route =
            CloudRoute::new("https://API.example.com./v1", "m".into(), key.clone()).unwrap();
        assert_eq!(route.base_url().as_str(), "https://api.example.com/v1");
        assert!(!route.allow_insecure_http());
        assert!(CloudRoute::new("https://10.0.0.1/v1", "m".into(), key).is_err());
    }

    #[cfg(not(feature = "online-stand-in"))]
    #[test]
    fn without_the_stand_in_feature_http_is_refused() {
        assert_eq!(
            validate_url("http://127.0.0.1:8080/v1", true),
            Err(UrlProblem::NotHttps)
        );
    }

    #[cfg(feature = "online-stand-in")]
    #[test]
    fn the_stand_in_feature_takes_loopback_http_only() {
        assert!(validate_url("http://127.0.0.1:8080/v1", true).is_ok());
        assert!(validate_url("http://127.0.0.1:8080/v1", false).is_err());
        assert!(validate_url("http://10.0.0.1:8080/v1", true).is_err());
        let route = CloudRoute::stand_in(
            "http://127.0.0.1:8080/v1",
            "m".into(),
            OnlineKey::new("k".into()),
        )
        .unwrap();
        assert!(route.allow_insecure_http());
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
