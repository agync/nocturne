//! The address rules of an online provider the person chose (ADR 0011): which addresses may be
//! one, over which scheme, and where they point. A provider on the person's own network may be
//! reached over plain HTTP; anywhere else takes HTTPS. Nocturne's own listeners, the cloud
//! metadata services and the networks that are never a provider are refused.

use std::fmt;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::pin::Pin;
use std::sync::Mutex;

use url::{Host, Url};
use zeroize::Zeroizing;

/// Networks that are never a provider, as `address/prefix`: unspecified, link-local (cloud
/// metadata at 169.254.169.254 among them), multicast, broadcast and reserved, the
/// documentation ranges, site-local IPv6, NAT64 and 6to4. IPv4-mapped and IPv4-compatible IPv6
/// addresses are judged as the IPv4 address they carry.
pub const NEVER_NETS: &[&str] = &[
    "0.0.0.0/8",
    "169.254.0.0/16",
    "192.0.2.0/24",
    "198.51.100.0/24",
    "203.0.113.0/24",
    "224.0.0.0/4",
    "240.0.0.0/4",
    "255.255.255.255/32",
    "::/128",
    "fe80::/10",
    "fec0::/10",
    "ff00::/8",
    "2001:db8::/32",
    "3fff::/20",
    "64:ff9b::/96",
    "2002::/16",
];

/// Networks of this computer and the person's own: loopback, private (RFC 1918), shared
/// (100.64.0.0/10, where tailnets live) and unique-local IPv6. A provider here may be reached
/// over plain HTTP.
pub const OWN_NETS: &[&str] = &[
    "127.0.0.0/8",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "100.64.0.0/10",
    "::1/128",
    "fc00::/7",
];

/// Host names that are never a provider: the cloud metadata services.
pub const NEVER_NAMES: &[&str] = &[
    "metadata",
    "metadata.google.internal",
    "instance-data",
    "instance-data.ec2.internal",
];

/// Host names on this computer or the person's own network: exact, or `*.` for any name under
/// it. A name of one label (no dot) is one too. A trailing dot is ignored.
pub const OWN_NAMES: &[&str] = &[
    "localhost",
    "*.localhost",
    "*.local",
    "*.home.arpa",
    "*.internal",
    "*.ts.net",
];

/// Loopback ports Nocturne itself listens on, each held by an [`OwnPort`].
static OWN_PORTS: Mutex<Vec<u16>> = Mutex::new(Vec::new());

/// A loopback port Nocturne listens on (the engine, the preview, a listener of Away from
/// home), held while it does: an address naming this computer at that port is refused, and no
/// other engine is given it.
#[must_use]
#[derive(Debug)]
pub struct OwnPort(u16);

impl OwnPort {
    pub fn hold(port: u16) -> Self {
        OWN_PORTS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(port);
        OwnPort(port)
    }

    /// Holds `port` unless something of this app already holds it, in one step, so two engines
    /// starting at once are never given the same port.
    pub fn claim(port: u16) -> Option<Self> {
        let mut ports = OWN_PORTS.lock().unwrap_or_else(|e| e.into_inner());
        if ports.contains(&port) {
            return None;
        }
        ports.push(port);
        Some(OwnPort(port))
    }

    pub fn port(&self) -> u16 {
        self.0
    }
}

impl Drop for OwnPort {
    fn drop(&mut self) {
        let mut ports = OWN_PORTS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(at) = ports.iter().position(|p| *p == self.0) {
            ports.swap_remove(at);
        }
    }
}

fn own_port(port: u16) -> bool {
    OWN_PORTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&port)
}

/// The person's key. Never printed: `Debug` and `Display` show `[key]`. Wiped from memory when
/// dropped.
#[derive(Clone)]
pub struct OnlineKey(Zeroizing<String>);

impl OnlineKey {
    pub fn new(key: String) -> Self {
        OnlineKey(Zeroizing::new(key))
    }

    /// The key itself, for the request that carries it and the log redactor.
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

/// Why an address cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlProblem {
    Invalid,
    /// Plain HTTP to an address outside the person's own network, or another scheme.
    NotHttps,
    NoHost,
    /// User name or password, a fragment, a query or whitespace in the address.
    Extra,
    /// An address that is never a provider, or one of Nocturne's own ports on this computer.
    NotAllowed,
}

/// Where an address points, which decides whether it may be plain HTTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// This computer or the person's own network: HTTPS or plain HTTP.
    Own,
    /// Anywhere else: HTTPS only.
    Public,
}

/// An address that passed [`validate_url`], normalised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub url: Url,
    pub reach: Reach,
}

impl Address {
    /// Whether the engine must be allowed plain HTTP to reach it.
    pub fn insecure_http(&self) -> bool {
        self.url.scheme() == "http"
    }
}

struct Judged {
    url: Url,
    reach: Reach,
    loopback: bool,
}

/// Checks an address the person entered and returns it normalised, with where it points. Never:
/// the networks in [`NEVER_NETS`], the names in [`NEVER_NAMES`], an address with anything but a
/// scheme, host, port and path, and Nocturne's own ports on this computer. This computer and
/// the person's own network ([`OWN_NETS`], [`OWN_NAMES`]) take `https://` or `http://`;
/// anywhere else takes `https://` only. The host is judged as the URL parser reads it, so
/// decimal, octal, hex and shortened IPv4 forms are caught, and an IPv6 address that carries an
/// IPv4 one is judged as that IPv4 address.
pub fn validate_url(raw: &str) -> Result<Address, UrlProblem> {
    let judged = judge(raw)?;
    if judged.loopback && judged.url.port_or_known_default().is_some_and(own_port) {
        return Err(UrlProblem::NotAllowed);
    }
    Ok(Address {
        url: judged.url,
        reach: judged.reach,
    })
}

/// Whether `base_url` is one the engine may reach over plain HTTP: an `http://` address on
/// this computer or the person's own network.
pub fn plain_http_allowed(base_url: &str) -> bool {
    judge(base_url).is_ok_and(|j| j.url.scheme() == "http" && j.reach == Reach::Own)
}

fn judge(raw: &str) -> Result<Judged, UrlProblem> {
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
    if !matches!(url.scheme(), "https" | "http") {
        return Err(UrlProblem::NotHttps);
    }
    let (class, loopback, renamed) = match url.host() {
        None => return Err(UrlProblem::NoHost),
        Some(Host::Ipv4(a)) => (class_of_ip(IpAddr::V4(a)), a.is_loopback(), None),
        Some(Host::Ipv6(a)) => {
            let loopback = a.is_loopback() || carried_v4(a).is_some_and(|v4| v4.is_loopback());
            (class_of_ip(IpAddr::V6(a)), loopback, None)
        }
        Some(Host::Domain(name)) => {
            let bare = name.trim_end_matches('.').to_ascii_lowercase();
            let loopback = name_matches(&bare, &["localhost", "*.localhost"]);
            let renamed = (bare != name).then(|| bare.clone());
            (class_of_name(&bare), loopback, renamed)
        }
    };
    let reach = match class {
        Class::Never => return Err(UrlProblem::NotAllowed),
        Class::Own => Reach::Own,
        Class::Public => Reach::Public,
    };
    if reach == Reach::Public && url.scheme() != "https" {
        return Err(UrlProblem::NotHttps);
    }
    if url.port() == Some(0) {
        return Err(UrlProblem::NotAllowed);
    }
    if let Some(bare) = renamed {
        url.set_host(Some(&bare)).map_err(|_| UrlProblem::Invalid)?;
    }
    Ok(Judged {
        url,
        reach,
        loopback,
    })
}

/// Looks up the addresses a host name has: the system's resolver ([`system_lookup`]), or a
/// stand-in in tests.
pub type Lookup = std::sync::Arc<
    dyn Fn(String, u16) -> Pin<Box<dyn Future<Output = std::io::Result<Vec<IpAddr>>> + Send>>
        + Send
        + Sync,
>;

/// The system's resolver.
pub fn system_lookup() -> Lookup {
    std::sync::Arc::new(|host: String, port: u16| {
        Box::pin(async move {
            Ok(tokio::net::lookup_host((host.as_str(), port))
                .await?
                .map(|a| a.ip())
                .collect())
        })
    })
}

/// Judges a name on the person's own network by what it resolves to now, since a name alone
/// (a search domain, split DNS) may point anywhere. Every address must be on this computer or
/// the person's own network, or the address counts as public and must use `https://`; one that
/// is never a provider, or one of Nocturne's own ports on this computer, is refused. An IP
/// literal or a public name is returned as [`validate_url`] judged it. A name that does not
/// resolve keeps its class: nothing can reach it, so nothing is sent.
pub async fn resolved_reach(url: &Url, lookup: &Lookup) -> Result<Reach, UrlProblem> {
    let judged = judge(url.as_str())?;
    let (Some(Host::Domain(name)), Reach::Own) = (url.host(), judged.reach) else {
        return Ok(judged.reach);
    };
    let port = url.port_or_known_default().unwrap_or(443);
    let Ok(found) = lookup(name.to_string(), port).await else {
        return Ok(Reach::Own);
    };
    let mut reach = Reach::Own;
    for ip in found {
        match class_of_ip(ip) {
            Class::Never => return Err(UrlProblem::NotAllowed),
            Class::Public => reach = Reach::Public,
            Class::Own => {
                let loopback = match ip {
                    IpAddr::V4(a) => a.is_loopback(),
                    IpAddr::V6(a) => {
                        a.is_loopback() || carried_v4(a).is_some_and(|v4| v4.is_loopback())
                    }
                };
                if loopback && own_port(port) {
                    return Err(UrlProblem::NotAllowed);
                }
            }
        }
    }
    if reach == Reach::Public && url.scheme() != "https" {
        return Err(UrlProblem::NotAllowed);
    }
    Ok(reach)
}

/// Whether an address found for a provider's name may be connected to at `port`: never one
/// that is never a provider, nor one of Nocturne's own ports on this device, and over plain
/// HTTP only one on this device or the person's own network. The client's resolver lets only
/// these through, so the addresses judged are the ones connected to.
pub fn may_connect(ip: IpAddr, port: u16, plain_http: bool) -> bool {
    let loopback = match ip {
        IpAddr::V4(a) => a.is_loopback(),
        IpAddr::V6(a) => a.is_loopback() || carried_v4(a).is_some_and(|v4| v4.is_loopback()),
    };
    match class_of_ip(ip) {
        Class::Never => false,
        Class::Own => !(loopback && own_port(port)),
        Class::Public => !plain_http,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Never,
    Own,
    Public,
}

fn class_of_name(name: &str) -> Class {
    if name.is_empty() || name_matches(name, NEVER_NAMES) {
        Class::Never
    } else if !name.contains('.') || name_matches(name, OWN_NAMES) {
        Class::Own
    } else {
        Class::Public
    }
}

fn name_matches(name: &str, rules: &[&str]) -> bool {
    rules.iter().any(|rule| match rule.strip_prefix("*.") {
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

fn class_of_ip(ip: IpAddr) -> Class {
    if let IpAddr::V6(a) = ip
        && let Some(v4) = carried_v4(a)
    {
        return class_of_ip(IpAddr::V4(v4));
    }
    if NEVER_NETS.iter().any(|net| in_net(ip, net)) {
        Class::Never
    } else if OWN_NETS.iter().any(|net| in_net(ip, net)) {
        Class::Own
    } else {
        Class::Public
    }
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

/// The provider as the person knows it: the address's host, with its port when one was given.
pub fn host_of(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_string(),
    }
}

/// The port, when `url` names this device: nothing of Nocturne's may listen on it.
pub fn loopback_port(url: &Url) -> Option<u16> {
    judge(url.as_str())
        .ok()
        .filter(|j| j.loopback)
        .and_then(|j| j.url.port_or_known_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lists_are_well_formed() {
        for net in NEVER_NETS.iter().chain(OWN_NETS) {
            let (addr, prefix) = net.split_once('/').unwrap();
            let addr: std::net::IpAddr = addr.parse().unwrap();
            let prefix: u8 = prefix.parse().unwrap();
            assert!(prefix <= if addr.is_ipv4() { 32 } else { 128 }, "{net}");
        }
        assert!(NEVER_NETS.contains(&"169.254.0.0/16"));
        assert!(NEVER_NAMES.iter().chain(OWN_NAMES).all(|n| !n.is_empty()));
    }

    fn judged(raw: &str) -> Result<(String, Reach, bool), UrlProblem> {
        validate_url(raw).map(|a| {
            let insecure = a.insecure_http();
            (a.url.to_string(), a.reach, insecure)
        })
    }

    #[test]
    fn public_addresses_take_https_and_are_normalised() {
        for (raw, want) in [
            ("https://api.example.com/v1", "https://api.example.com/v1"),
            ("HTTPS://API.Example.COM/v1", "https://api.example.com/v1"),
            ("https://api.example.com./v1", "https://api.example.com/v1"),
            (
                "https://api.example.com:8443/v1",
                "https://api.example.com:8443/v1",
            ),
            ("https://8.8.8.8/v1", "https://8.8.8.8/v1"),
            ("https://172.32.0.1/v1", "https://172.32.0.1/v1"),
            ("https://100.128.0.1/v1", "https://100.128.0.1/v1"),
            (
                "https://[2001:4860:4860::8888]/v1",
                "https://[2001:4860:4860::8888]/v1",
            ),
        ] {
            assert_eq!(
                judged(raw),
                Ok((want.to_string(), Reach::Public, false)),
                "{raw}"
            );
        }
    }

    #[test]
    fn public_addresses_refuse_plain_http() {
        for raw in [
            "http://api.example.com/v1",
            "http://8.8.8.8/v1",
            "http://172.32.0.1/v1",
            "http://[2001:4860:4860::8888]/v1",
            "http://localtest.me/v1",
            "ftp://api.example.com/v1",
            "ftp://127.0.0.1/v1",
        ] {
            assert_eq!(judged(raw), Err(UrlProblem::NotHttps), "{raw}");
        }
    }

    #[test]
    fn this_computer_and_the_own_network_take_http_and_https() {
        for (raw, want) in [
            ("http://127.0.0.1:11434/v1", "http://127.0.0.1:11434/v1"),
            ("http://localhost:11434/v1", "http://localhost:11434/v1"),
            ("http://LOCALHOST.:11434/v1", "http://localhost:11434/v1"),
            ("http://ollama.localhost/v1", "http://ollama.localhost/v1"),
            ("http://[::1]:8000/v1", "http://[::1]:8000/v1"),
            ("http://10.1.2.3/v1", "http://10.1.2.3/v1"),
            ("http://172.20.0.1:8080/v1", "http://172.20.0.1:8080/v1"),
            ("http://192.168.1.10:8000/v1", "http://192.168.1.10:8000/v1"),
            ("http://100.100.100.100/v1", "http://100.100.100.100/v1"),
            ("http://[fd00::1]/v1", "http://[fd00::1]/v1"),
            (
                "http://box.tail1234.ts.net/v1",
                "http://box.tail1234.ts.net/v1",
            ),
            ("http://nas.local/v1", "http://nas.local/v1"),
            ("http://nas.local./v1", "http://nas.local/v1"),
            ("http://nas.home.arpa/v1", "http://nas.home.arpa/v1"),
            ("http://gpu.corp.internal/v1", "http://gpu.corp.internal/v1"),
            ("http://gpubox:8000/v1", "http://gpubox:8000/v1"),
            ("http://gpubox.:8000/v1", "http://gpubox:8000/v1"),
        ] {
            assert_eq!(
                judged(raw),
                Ok((want.to_string(), Reach::Own, true)),
                "{raw}"
            );
            let https = raw.replacen("http://", "https://", 1);
            let want = want.replacen("http://", "https://", 1);
            assert_eq!(judged(&https), Ok((want, Reach::Own, false)), "{https}");
        }
    }

    #[test]
    fn tricks_are_normalised_then_judged_by_the_same_rules() {
        for (raw, want) in [
            ("http://127.1/v1", "http://127.0.0.1/v1"),
            ("http://2130706433/v1", "http://127.0.0.1/v1"),
            ("http://0x7f000001/v1", "http://127.0.0.1/v1"),
            ("http://0x7f.0.0.1/v1", "http://127.0.0.1/v1"),
            ("http://017700000001/v1", "http://127.0.0.1/v1"),
            ("http://0177.0.0.1/v1", "http://127.0.0.1/v1"),
            ("http://3232235777/v1", "http://192.168.1.1/v1"),
            ("http://[::ffff:127.0.0.1]/v1", "http://[::ffff:7f00:1]/v1"),
            ("http://[::ffff:100.64.0.1]/v1", "http://[::ffff:6440:1]/v1"),
            ("http://[::10.0.0.1]/v1", "http://[::a00:1]/v1"),
        ] {
            assert_eq!(
                judged(raw),
                Ok((want.to_string(), Reach::Own, true)),
                "{raw}"
            );
        }
        for raw in [
            "http://134744072/v1",
            "http://[::ffff:8.8.8.8]/v1",
            "http://[::8.8.8.8]/v1",
        ] {
            assert_eq!(judged(raw), Err(UrlProblem::NotHttps), "{raw}");
        }
        for raw in [
            "https://2852039166/latest",
            "https://0xa9fea9fe/latest",
            "https://[::ffff:169.254.169.254]/latest",
            "https://[::169.254.169.254]/latest",
            "https://[::ffff:0.0.0.0]/v1",
            "https://[::ffff:224.0.0.1]/v1",
        ] {
            assert_eq!(judged(raw), Err(UrlProblem::NotAllowed), "{raw}");
        }
    }

    #[test]
    fn what_is_never_a_provider_is_refused_over_either_scheme() {
        for host in [
            "0.0.0.0",
            "0",
            "0.1.2.3",
            "169.254.169.254",
            "169.254.0.1",
            "192.0.2.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "239.255.255.250",
            "240.0.0.1",
            "255.255.255.255",
            "[::]",
            "[fe80::1]",
            "[fec0::1]",
            "[ff02::1]",
            "[2001:db8::1]",
            "[3fff::1]",
            "[64:ff9b::a00:1]",
            "[2002:c0a8:101::1]",
            "metadata",
            "metadata.google.internal",
            "metadata.google.internal.",
            "instance-data",
            "instance-data.ec2.internal",
        ] {
            for scheme in ["http", "https"] {
                let raw = format!("{scheme}://{host}/v1");
                assert_eq!(judged(&raw), Err(UrlProblem::NotAllowed), "{raw}");
            }
        }
        assert_eq!(judged("http://127.0.0.1:0/v1"), Err(UrlProblem::NotAllowed));
    }

    #[test]
    fn extras_are_refused_on_any_address() {
        for (raw, want) in [
            ("not a url", UrlProblem::Extra),
            ("api.example.com/v1", UrlProblem::Invalid),
            ("https://user:pass@api.example.com/v1", UrlProblem::Extra),
            ("https://user@api.example.com/v1", UrlProblem::Extra),
            ("https://api.example.com/v1?key=x", UrlProblem::Extra),
            ("https://api.example.com/v1#top", UrlProblem::Extra),
            ("https://api.example.com/v1\n", UrlProblem::Extra),
            (" https://api.example.com/v1", UrlProblem::Extra),
            ("http://user:pass@127.0.0.1:8000/v1", UrlProblem::Extra),
            ("http://192.168.1.10/v1?x=1", UrlProblem::Extra),
            ("http://nas.local/v1#x", UrlProblem::Extra),
        ] {
            assert_eq!(validate_url(raw), Err(want), "{raw}");
        }
    }

    #[test]
    fn nocturnes_own_ports_on_this_computer_are_refused_while_held() {
        let port = 1;
        let raw = format!("http://127.0.0.1:{port}/v1");
        assert!(validate_url(&raw).is_ok());
        let held = OwnPort::hold(port);
        for raw in [
            format!("http://127.0.0.1:{port}/v1"),
            format!("http://127.0.0.2:{port}/v1"),
            format!("https://localhost:{port}/v1"),
            format!("http://[::1]:{port}/v1"),
            format!("http://[::ffff:127.0.0.1]:{port}/v1"),
        ] {
            assert_eq!(validate_url(&raw), Err(UrlProblem::NotAllowed), "{raw}");
        }
        assert!(validate_url(&format!("http://192.168.1.10:{port}/v1")).is_ok());
        assert!(validate_url(&format!("http://127.0.0.1:{}/v1", port + 1)).is_ok());
        let url = Url::parse(&format!("http://localhost:{}/v1", port + 1)).unwrap();
        assert_eq!(loopback_port(&url), Some(port + 1));
        let away = Url::parse("http://192.168.1.10:8000/v1").unwrap();
        assert_eq!(loopback_port(&away), None);
        drop(held);
        assert!(validate_url(&raw).is_ok());
    }

    fn stub(addrs: &'static [&'static str]) -> Lookup {
        std::sync::Arc::new(move |_: String, _: u16| {
            Box::pin(async move { Ok(addrs.iter().map(|a| a.parse().unwrap()).collect()) })
        })
    }

    fn reach_of(raw: &str, lookup: &Lookup) -> Result<Reach, UrlProblem> {
        let url = validate_url(raw).unwrap().url;
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(resolved_reach(&url, lookup))
    }

    #[test]
    fn an_own_name_is_judged_by_what_it_resolves_to() {
        let private = stub(&["192.168.1.10", "fd00::1"]);
        for raw in [
            "http://gpubox:8000/v1",
            "http://nas.local/v1",
            "https://nas.local/v1",
        ] {
            assert_eq!(reach_of(raw, &private), Ok(Reach::Own), "{raw}");
        }
        let public = stub(&["192.168.1.10", "8.8.8.8"]);
        for raw in [
            "http://gpubox:8000/v1",
            "http://gpu.tail1234.ts.net/v1",
            "http://gpu.corp.internal/v1",
        ] {
            assert_eq!(reach_of(raw, &public), Err(UrlProblem::NotAllowed), "{raw}");
        }
        assert_eq!(reach_of("https://gpubox/v1", &public), Ok(Reach::Public));
        let never = stub(&["169.254.169.254"]);
        for raw in ["http://gpubox/v1", "https://nas.home.arpa/v1"] {
            assert_eq!(reach_of(raw, &never), Err(UrlProblem::NotAllowed), "{raw}");
        }
        let mapped = stub(&["::ffff:169.254.169.254"]);
        assert_eq!(
            reach_of("http://gpubox/v1", &mapped),
            Err(UrlProblem::NotAllowed)
        );
    }

    #[test]
    fn literals_and_public_names_are_not_looked_up() {
        let never_called: Lookup = std::sync::Arc::new(|host: String, _: u16| {
            panic!("looked up {host}");
        });
        assert_eq!(
            reach_of("http://192.168.1.10/v1", &never_called),
            Ok(Reach::Own)
        );
        assert_eq!(
            reach_of("https://api.example.com/v1", &never_called),
            Ok(Reach::Public)
        );
        let failing: Lookup = std::sync::Arc::new(|_: String, _: u16| {
            Box::pin(async { Err(std::io::Error::other("no such name")) })
        });
        assert_eq!(reach_of("http://gpubox/v1", &failing), Ok(Reach::Own));
    }

    #[test]
    fn an_own_name_on_this_computer_at_an_own_port_is_refused() {
        let port = 7;
        let _held = OwnPort::hold(port);
        let loopback = stub(&["127.0.0.1"]);
        assert_eq!(
            reach_of(&format!("http://gpubox:{port}/v1"), &loopback),
            Err(UrlProblem::NotAllowed)
        );
        assert_eq!(
            reach_of(&format!("http://gpubox:{}/v1", port + 1), &loopback),
            Ok(Reach::Own)
        );
    }

    #[test]
    fn only_allowed_addresses_may_be_connected_to() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert!(may_connect(ip("192.168.1.10"), 80, true));
        assert!(may_connect(ip("fd00::1"), 80, true));
        assert!(!may_connect(ip("8.8.8.8"), 80, true));
        assert!(may_connect(ip("8.8.8.8"), 443, false));
        for never in [
            "169.254.169.254",
            "::ffff:169.254.169.254",
            "0.0.0.0",
            "224.0.0.1",
        ] {
            assert!(!may_connect(ip(never), 443, false), "{never}");
        }
        let port = 9;
        let _held = OwnPort::hold(port);
        assert!(!may_connect(ip("127.0.0.1"), port, true));
        assert!(may_connect(ip("127.0.0.1"), port + 1, true));
    }

    #[test]
    fn plain_http_is_allowed_only_on_the_own_network() {
        assert!(plain_http_allowed("http://127.0.0.1:8080/v1"));
        assert!(plain_http_allowed("http://box.tail1234.ts.net/v1"));
        assert!(!plain_http_allowed("https://127.0.0.1:8080/v1"));
        assert!(!plain_http_allowed("http://api.example.com/v1"));
        assert!(!plain_http_allowed("http://169.254.169.254/v1"));
    }
}
