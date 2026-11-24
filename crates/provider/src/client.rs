//! One client to the one host the person chose. It is built only from an address that passed
//! [`validate_url`], and every request it makes is to a path under that address: a URL on any
//! other scheme, host or port is refused before anything leaves. Redirects are never followed
//! and no proxy is used, so nothing the provider says can send a request, or the key, elsewhere.
//! The client resolves names itself: only the chosen host's name is looked up, and
//! only the addresses the rules allow ([`may_connect`]) are handed to the connection, so the
//! addresses judged are the ones connected to. Plain HTTP never reaches an address outside the
//! person's own network, and a name that cannot be looked up is not reached at all.

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use nocturne_contract::{CheckOutcome, ModelList};
use nocturne_engine::OnlineFailure;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue};
use serde_json::{Value, json};
use url::{Origin, Url};

use crate::address::{
    Lookup, OnlineKey, UrlProblem, host_of, may_connect, system_lookup, validate_url,
};

/// The most a reply may be, read or not.
const MAX_REPLY: usize = 16 << 20;
const CONNECT: Duration = Duration::from_secs(10);
/// How long the check and the look at the list of models may take.
pub const CHECK_SECONDS: u64 = 20;

/// What one request cost and how it ended.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Exchange {
    /// `None` when no answer came.
    pub status: Option<u16>,
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub seconds: f32,
    pub cost_usd: Option<f64>,
    /// Stopped here before an answer came, so it says nothing of the provider.
    pub stopped: bool,
    /// What it carried, and where that came from.
    pub carried: Box<Carried>,
}

/// What a request carried, as a person reads it, and the things kept and the memories whose
/// words those were.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Carried {
    pub said: Vec<Said>,
    pub items: Vec<String>,
    pub memories: Vec<String>,
    /// For a question in parts: which part this was, and what the provider asked to look up.
    pub part: Option<crate::lookup::Part>,
}

/// A part of what a request carried, as a person reads it: its words as they left, and in place
/// of a photo or a recording, which it was.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "text", rename_all = "camelCase")]
pub enum Said {
    Words(String),
    Photo,
    Recording,
}

/// What a chat request's `body` carried, part by part.
fn said_of(body: &Value) -> Vec<Said> {
    let mut said = Vec::new();
    for message in body["messages"].as_array().into_iter().flatten() {
        match &message["content"] {
            Value::String(words) => said.push(Said::Words(words.clone())),
            Value::Array(parts) => {
                for part in parts {
                    match part["type"].as_str() {
                        Some("text") => said.push(Said::Words(
                            part["text"].as_str().unwrap_or_default().to_string(),
                        )),
                        Some("image_url") => said.push(Said::Photo),
                        Some("input_audio") => said.push(Said::Recording),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    said
}

/// Stops one request when it turns true ([`Provider::chat_until`]).
pub type Stop = tokio::sync::watch::Receiver<bool>;

/// What one look at the list of models found, and the request, when one left.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub list: ModelList,
    pub exchange: Option<Exchange>,
}

/// A reply the provider wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub text: String,
    /// The look-ups the provider asked for instead of answering, as it wrote them.
    pub tool_calls: Vec<Value>,
    /// It was stopped at the most it could write.
    pub cut: bool,
    pub exchange: Exchange,
}

/// A request that did not come back as a reply.
#[derive(Debug, Clone, PartialEq)]
pub struct Failed {
    pub failure: OnlineFailure,
    pub exchange: Exchange,
    /// The request never left: its address was refused, there was no way to the internet
    /// (`NoInternet`: its name could not be looked up, or no route), or it was stopped first.
    pub refused: bool,
    /// The provider's refusal names tools or functions, and not the length of what was sent.
    pub names_tools: bool,
}

/// The person's provider: its address, the model they named and their key.
pub struct Provider {
    base: Url,
    origin: Origin,
    host: String,
    model: String,
    key: OnlineKey,
    http: reqwest::Client,
    /// Turned off: no request leaves from now on, and one under way is dropped.
    stopped: tokio::sync::watch::Sender<bool>,
}

impl std::fmt::Debug for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Provider")
            .field("base", &self.base.as_str())
            .field("model", &self.model)
            .field("key", &self.key)
            .finish()
    }
}

/// TLS with the public roots, as the sealed channel's client.
fn tls() -> rustls::ClientConfig {
    static TLS: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    let cfg = TLS.get_or_init(|| {
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let mut cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("the default protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
        cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
        Arc::new(cfg)
    });
    (**cfg).clone()
}

/// Why the resolver gave the connection no address: nothing was sent.
#[derive(Debug)]
struct AddressRefused;

impl std::fmt::Display for AddressRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the provider's address was refused")
    }
}

impl std::error::Error for AddressRefused {}

/// Why the resolver gave the connection no address: the name could not be looked up.
#[derive(Debug)]
struct LookupFailed;

impl std::fmt::Display for LookupFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the provider's name could not be looked up")
    }
}

impl std::error::Error for LookupFailed {}

/// The client's resolver: the one host's name alone, and of its addresses only those
/// [`may_connect`] allows. One lookup serves both the judging and the connection.
struct OneHost {
    name: String,
    port: u16,
    plain_http: bool,
    lookup: Lookup,
}

impl reqwest::dns::Resolve for OneHost {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let asked = name.as_str().trim_end_matches('.').to_ascii_lowercase();
        let (port, plain_http) = (self.port, self.plain_http);
        let found = (asked == self.name).then(|| (self.lookup)(asked, port));
        Box::pin(async move {
            let refused =
                || -> Box<dyn std::error::Error + Send + Sync> { Box::new(AddressRefused) };
            let Some(found) = found else {
                return Err(refused());
            };
            let found = found.await.map_err(|_| {
                let failed: Box<dyn std::error::Error + Send + Sync> = Box::new(LookupFailed);
                failed
            })?;
            let allowed: Vec<SocketAddr> = found
                .into_iter()
                .filter(|ip| may_connect(*ip, port, plain_http))
                .map(|ip| SocketAddr::new(ip, port))
                .collect();
            if allowed.is_empty() {
                return Err(refused());
            }
            let addrs: reqwest::dns::Addrs = Box::new(allowed.into_iter());
            Ok(addrs)
        })
    }
}

/// The client for `base`, resolving with `lookup` through [`OneHost`].
fn client(base: &Url, lookup: Lookup) -> Result<reqwest::Client, UrlProblem> {
    let one_host = OneHost {
        name: base.host_str().unwrap_or_default().to_ascii_lowercase(),
        port: base.port_or_known_default().ok_or(UrlProblem::Invalid)?,
        plain_http: base.scheme() == "http",
        lookup,
    };
    reqwest::Client::builder()
        .tls_backend_preconfigured(tls())
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .https_only(base.scheme() == "https")
        .dns_resolver(one_host)
        .connect_timeout(CONNECT)
        .build()
        .map_err(|_| UrlProblem::Invalid)
}

impl Provider {
    /// The provider at `url` (normalised by [`validate_url`]), asked for `model` with `key`.
    pub fn new(url: &str, model: &str, key: OnlineKey) -> Result<Provider, UrlProblem> {
        let address = validate_url(url)?;
        let model = model.trim();
        if model.is_empty() {
            return Err(UrlProblem::Invalid);
        }
        let mut base = address.url;
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        let http = client(&base, system_lookup())?;
        Ok(Provider {
            origin: base.origin(),
            host: host_of(&base),
            base,
            model: model.to_string(),
            key,
            http,
            stopped: tokio::sync::watch::channel(false).0,
        })
    }

    /// No request leaves from now on, one under way included at its next step.
    pub fn stop(&self) {
        self.stopped.send_replace(true);
    }

    /// Looks names up with `lookup` in place of the system's resolver. For tests: the address
    /// rules still judge every address it gives.
    #[doc(hidden)]
    pub fn with_lookup(mut self, lookup: Lookup) -> Provider {
        if let Ok(http) = client(&self.base, lookup) {
            self.http = http;
        }
        self
    }

    /// The host as the person knows it, with its port when one was given.
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// The address of `path` under the provider's: refused unless it is on the one host, by the
    /// same scheme and port.
    pub fn url_for(&self, path: &str) -> Result<Url, UrlProblem> {
        let url = self.base.join(path).map_err(|_| UrlProblem::Invalid)?;
        if url.origin() != self.origin
            || !url.path().starts_with(self.base.path())
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(UrlProblem::NotAllowed);
        }
        Ok(url)
    }

    /// One chat request: `body` with the model set, answered within `timeout`.
    pub async fn chat(&self, body: Value, timeout: Duration) -> Result<Reply, Failed> {
        self.chat_until(body, timeout, None).await
    }

    /// [`Provider::chat`], dropped as soon as `stop` turns true: never sent when it already
    /// has, and otherwise ended where it is, as a request that left.
    pub async fn chat_until(
        &self,
        mut body: Value,
        timeout: Duration,
        stop: Option<Stop>,
    ) -> Result<Reply, Failed> {
        body["model"] = json!(self.model);
        body["stream"] = json!(false);
        let started = Instant::now();
        let (status, bytes, mut exchange) = self
            .send("chat/completions", Some(&body), timeout, started, stop)
            .await?;
        let reply: Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => return Err(failed(OnlineFailure::Other(status), exchange)),
        };
        exchange.cost_usd = reply["usage"]["cost"].as_f64();
        let choice = &reply["choices"][0];
        let text = match &choice["message"]["content"] {
            Value::String(s) => s.clone(),
            Value::Array(parts) => parts
                .iter()
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join(""),
            _ => String::new(),
        };
        let tool_calls = choice["message"]["tool_calls"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(Reply {
            text,
            tool_calls,
            cut: choice["finish_reason"] == "length",
            exchange,
        })
    }

    /// The models the provider lists.
    pub async fn models(&self) -> Result<(Vec<String>, Exchange), Failed> {
        let started = Instant::now();
        let (status, bytes, exchange) = self
            .send(
                "models",
                None,
                Duration::from_secs(CHECK_SECONDS),
                started,
                None,
            )
            .await?;
        let Ok(listed) = serde_json::from_slice::<Value>(&bytes) else {
            return Err(failed(OnlineFailure::Other(status), exchange));
        };
        let ids = listed["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["id"].as_str().map(str::to_string))
            .collect();
        Ok((ids, exchange))
    }

    /// One look at the list of models, at the person's request: the names to choose from, none
    /// (an empty list, or an address with no list), or why it failed, as the check says it. The
    /// exchange is there when the request left.
    pub async fn list(&self) -> Look {
        match self.models().await {
            Ok((names, exchange)) if names.is_empty() => Look {
                list: ModelList::None,
                exchange: Some(exchange),
            },
            Ok((names, exchange)) => Look {
                list: ModelList::Listed { names },
                exchange: Some(exchange),
            },
            Err(f) if f.refused => Look {
                list: ModelList::Failed {
                    problem: refused_outcome(f.failure),
                },
                exchange: None,
            },
            // Answered with something that is not a list, or "not found": some services have none.
            Err(f)
                if matches!(
                    f.failure,
                    OnlineFailure::ModelMissing | OnlineFailure::Other(200..300)
                ) =>
            {
                Look {
                    list: ModelList::None,
                    exchange: Some(f.exchange),
                }
            }
            Err(f) => Look {
                list: ModelList::Failed {
                    problem: check_outcome(f.failure),
                },
                exchange: Some(f.exchange),
            },
        }
    }

    /// The connection check: one look at the list of models, then the one question, a few
    /// words back at most, no memories. Each request's exchange is in the order made.
    pub async fn check(&self) -> (CheckOutcome, Vec<Exchange>) {
        self.check_with(None).await
    }

    /// [`Provider::check`], with the list from the person's own look just before in place of
    /// another look, when that look found names or none. A name not on the list stops nothing:
    /// the question decides, since some services answer to names they leave off.
    pub async fn check_with(&self, looked: Option<&ModelList>) -> (CheckOutcome, Vec<Exchange>) {
        let mut made = Vec::new();
        if !matches!(looked, Some(ModelList::Listed { .. } | ModelList::None)) {
            match self.models().await {
                Ok((_, exchange)) => made.push(exchange),
                Err(f) if f.refused => return (refused_outcome(f.failure), made),
                // A provider with no list still answers questions.
                Err(f) if f.failure == OnlineFailure::ModelMissing => made.push(f.exchange),
                Err(f) => {
                    made.push(f.exchange);
                    return (check_outcome(f.failure), made);
                }
            }
        }
        let body = json!({
            "messages": [{"role": "user", "content": nocturne_mind::online::CHECK_QUESTION}],
            "temperature": nocturne_mind::temperature::CHECK,
            "max_tokens": nocturne_mind::online::CHECK_MAX_TOKENS,
        });
        let outcome = match self.chat(body, Duration::from_secs(CHECK_SECONDS)).await {
            Ok(r) => {
                made.push(r.exchange);
                if r.text.trim().is_empty() {
                    CheckOutcome::EmptyReply
                } else {
                    CheckOutcome::Works {
                        host: self.host.clone(),
                        model: self.model.clone(),
                    }
                }
            }
            Err(f) if f.refused => refused_outcome(f.failure),
            Err(f) => {
                made.push(f.exchange);
                check_outcome(f.failure)
            }
        };
        (outcome, made)
    }

    /// Sends one request to `path` on the one host: a POST of `body`, or a GET without one.
    /// Answers the status, the reply's bytes and the exchange so far. Turning the provider off,
    /// or `stop`, ends it.
    async fn send(
        &self,
        path: &str,
        body: Option<&Value>,
        timeout: Duration,
        started: Instant,
        stop: Option<Stop>,
    ) -> Result<(u16, Vec<u8>, Exchange), Failed> {
        let mut stopped = self.stopped.subscribe();
        let mut stop = stop;
        if *stopped.borrow_and_update() || stop.as_mut().is_some_and(|s| *s.borrow_and_update()) {
            return Err(never_left(OnlineFailure::Refused));
        }
        let said = body.map(said_of).unwrap_or_default();
        let body = body.map(|b| serde_json::to_vec(b).unwrap_or_default());
        let bytes_up = body.as_ref().map_or(0, |b| b.len() as u64);
        let asked_to_stop = async {
            match stop.as_mut() {
                Some(s) => drop(s.wait_for(|s| *s).await),
                None => std::future::pending().await,
            }
        };
        let ended = || {
            Err(failed(
                OnlineFailure::Refused,
                Exchange {
                    bytes_up,
                    seconds: started.elapsed().as_secs_f32(),
                    stopped: true,
                    carried: Box::new(Carried {
                        said: said.clone(),
                        ..Carried::default()
                    }),
                    ..Exchange::default()
                },
            ))
        };
        let sent = async {
            let mut done = self.send_now(path, body, timeout, started).await;
            match &mut done {
                Ok((_, _, exchange)) => exchange.carried.said = said.clone(),
                Err(f) if !f.refused => f.exchange.carried.said = said.clone(),
                Err(_) => {}
            }
            done
        };
        tokio::select! {
            done = sent => done,
            _ = stopped.wait_for(|s| *s) => ended(),
            () = asked_to_stop => ended(),
        }
    }

    async fn send_now(
        &self,
        path: &str,
        body: Option<Vec<u8>>,
        timeout: Duration,
        started: Instant,
    ) -> Result<(u16, Vec<u8>, Exchange), Failed> {
        let refused = || never_left(OnlineFailure::Refused);
        let url = self.url_for(path).map_err(|_| refused())?;
        let mut auth = HeaderValue::from_str(&format!("Bearer {}", self.key.expose()))
            .map_err(|_| refused())?;
        auth.set_sensitive(true);
        let mut exchange = Exchange::default();
        let req = match body {
            Some(bytes) => {
                exchange.bytes_up = bytes.len() as u64;
                self.http
                    .post(url)
                    .header(CONTENT_TYPE, "application/json")
                    .body(bytes)
            }
            None => self.http.get(url),
        };
        let sent = req
            .header(AUTHORIZATION, auth)
            .timeout(timeout)
            .send()
            .await;
        let mut resp = match sent {
            Ok(r) => r,
            Err(e) if caused_by::<AddressRefused>(&e) => return Err(refused()),
            Err(e) if caused_by::<LookupFailed>(&e) => {
                return Err(never_left(OnlineFailure::NoInternet));
            }
            Err(e) if transport(&e) == OnlineFailure::NoInternet => {
                return Err(never_left(OnlineFailure::NoInternet));
            }
            Err(e) => {
                exchange.seconds = started.elapsed().as_secs_f32();
                return Err(failed(transport(&e), exchange));
            }
        };
        let status = resp.status().as_u16();
        exchange.status = Some(status);
        let mut bytes = Vec::new();
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    exchange.bytes_down += chunk.len() as u64;
                    if bytes.len() + chunk.len() > MAX_REPLY {
                        exchange.seconds = started.elapsed().as_secs_f32();
                        return Err(failed(OnlineFailure::Refused, exchange));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => {
                    exchange.seconds = started.elapsed().as_secs_f32();
                    return Err(failed(transport(&e), exchange));
                }
            }
        }
        exchange.seconds = started.elapsed().as_secs_f32();
        if !(200..300).contains(&status) {
            let said = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
            let mut f = failed(by_status(status, said.contains("model")), exchange);
            f.names_tools = (said.contains("tool") || said.contains("function"))
                && !["context", "token", "length", "too long"]
                    .iter()
                    .any(|w| said.contains(w));
            return Err(f);
        }
        Ok((status, bytes, exchange))
    }
}

fn never_left(failure: OnlineFailure) -> Failed {
    Failed {
        failure,
        exchange: Exchange::default(),
        refused: true,
        names_tools: false,
    }
}

/// What a request that never left means for a check.
fn refused_outcome(f: OnlineFailure) -> CheckOutcome {
    match f {
        OnlineFailure::NoInternet => CheckOutcome::NoInternet,
        _ => CheckOutcome::AddressNotAllowed,
    }
}

fn failed(failure: OnlineFailure, exchange: Exchange) -> Failed {
    Failed {
        failure,
        exchange,
        refused: false,
        names_tools: false,
    }
}

/// What a status other than success means. `says_model` is whether the provider's reply names
/// the model, read for a 400 only and never kept.
pub fn by_status(status: u16, says_model: bool) -> OnlineFailure {
    match status {
        401 | 403 => OnlineFailure::KeyRefused,
        402 => OnlineFailure::NoCredit,
        404 => OnlineFailure::ModelMissing,
        429 => OnlineFailure::Busy,
        400 if says_model => OnlineFailure::ModelMissing,
        500..=599 => OnlineFailure::Other(status),
        _ => OnlineFailure::Refused,
    }
}

/// Whether `e` came of the resolver's `E`: the request never left.
fn caused_by<E: std::error::Error + 'static>(e: &reqwest::Error) -> bool {
    let mut source: Option<&dyn std::error::Error> = Some(e);
    while let Some(s) = source {
        if s.is::<E>() {
            return true;
        }
        source = s.source();
    }
    false
}

/// What a request that got no answer means.
fn transport(e: &reqwest::Error) -> OnlineFailure {
    if e.is_timeout() {
        return OnlineFailure::Slow;
    }
    let mut source: Option<&dyn std::error::Error> = Some(e);
    while let Some(s) = source {
        let words = s.to_string().to_ascii_lowercase();
        if words.contains("network is unreachable") || words.contains("no route to host") {
            return OnlineFailure::NoInternet;
        }
        source = s.source();
    }
    OnlineFailure::Unreachable
}

/// What a failure means for a check.
pub fn check_outcome(f: OnlineFailure) -> CheckOutcome {
    match f {
        OnlineFailure::NoInternet => CheckOutcome::NoInternet,
        OnlineFailure::KeyRefused => CheckOutcome::KeyRefused,
        OnlineFailure::NoCredit => CheckOutcome::NoCredit,
        OnlineFailure::ModelMissing => CheckOutcome::ModelMissing,
        OnlineFailure::Busy => CheckOutcome::Busy,
        OnlineFailure::Other(s) if (500..600).contains(&s) => CheckOutcome::ServerError,
        OnlineFailure::Unreachable
        | OnlineFailure::Slow
        | OnlineFailure::Refused
        | OnlineFailure::Other(_) => CheckOutcome::Unreachable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(url: &str) -> Provider {
        Provider::new(url, "m", OnlineKey::new("k".into())).unwrap()
    }

    #[test]
    fn only_paths_under_the_address_are_reached() {
        let p = provider("https://api.example.com/v1");
        assert_eq!(
            p.url_for("chat/completions").unwrap().as_str(),
            "https://api.example.com/v1/chat/completions"
        );
        assert_eq!(p.host(), "api.example.com");
        for path in [
            "//other.example.com/v1/models",
            "https://other.example.com/v1/models",
            "http://api.example.com/v1/models",
            "https://api.example.com:8443/v1/models",
            "/models",
            "../models",
            "models?x=1",
            "models#x",
        ] {
            assert_eq!(p.url_for(path), Err(UrlProblem::NotAllowed), "{path}");
        }
    }

    #[test]
    fn a_provider_needs_an_allowed_address_and_a_model() {
        let key = || OnlineKey::new("k".into());
        assert_eq!(
            Provider::new("http://api.example.com/v1", "m", key()).unwrap_err(),
            UrlProblem::NotHttps
        );
        assert_eq!(
            Provider::new("https://169.254.169.254/v1", "m", key()).unwrap_err(),
            UrlProblem::NotAllowed
        );
        assert_eq!(
            Provider::new("https://api.example.com/v1", " ", key()).unwrap_err(),
            UrlProblem::Invalid
        );
        let p = Provider::new("http://192.168.1.10:8000/v1/", "m", key()).unwrap();
        assert_eq!(p.host(), "192.168.1.10:8000");
        assert!(!format!("{p:?}").contains("Bearer"));
    }

    #[test]
    fn statuses_say_what_went_wrong() {
        assert_eq!(by_status(401, false), OnlineFailure::KeyRefused);
        assert_eq!(by_status(403, false), OnlineFailure::KeyRefused);
        assert_eq!(by_status(402, false), OnlineFailure::NoCredit);
        assert_eq!(by_status(404, false), OnlineFailure::ModelMissing);
        assert_eq!(by_status(400, true), OnlineFailure::ModelMissing);
        assert_eq!(by_status(400, false), OnlineFailure::Refused);
        assert_eq!(by_status(429, false), OnlineFailure::Busy);
        assert_eq!(by_status(307, false), OnlineFailure::Refused);
        assert_eq!(by_status(503, false), OnlineFailure::Other(503));
    }
}
