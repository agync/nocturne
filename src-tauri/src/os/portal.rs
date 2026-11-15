//! The desktop portals, for what a Flatpak cannot do from inside its sandbox.
//!
//! A portal method answers at once with a request object; the result comes later as that
//! object's `Response` signal, often after the desktop has asked the person. [`request`] listens
//! before it calls, so an answer that comes back quickly is not missed.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use zbus::export::futures_core::Stream;
use zbus::zvariant::{DynamicType, OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, MatchRule, MessageStream};

pub const DESTINATION: &str = "org.freedesktop.portal.Desktop";
pub const PATH: &str = "/org/freedesktop/portal/desktop";
const REQUEST: &str = "org.freedesktop.portal.Request";
const BACKGROUND: &str = "org.freedesktop.portal.Background";

/// How long the person has to answer the desktop's question.
const ANSWER_LIMIT: Duration = Duration::from_secs(300);

/// Why a portal request did not succeed, and whether the desktop gave an answer. A refusal is an
/// answer; a bus that is down, a portal that is absent and a wait that ran out are not.
#[derive(Debug)]
pub struct Failed {
    pub answered: bool,
    why: String,
}

impl Failed {
    pub(super) fn silent(why: impl ToString) -> Failed {
        Failed {
            answered: false,
            why: why.to_string(),
        }
    }

    fn refused(why: impl ToString) -> Failed {
        Failed {
            answered: true,
            why: why.to_string(),
        }
    }
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.why)
    }
}

impl From<Failed> for String {
    fn from(f: Failed) -> String {
        f.why
    }
}

pub type Options<'a> = HashMap<&'a str, Value<'a>>;
pub type Results = HashMap<String, OwnedValue>;

/// Which session bus the portal is on: the person's, or one a test started.
#[derive(Debug, Clone, Default)]
pub struct Bus {
    address: Option<String>,
}

impl Bus {
    pub fn session() -> Bus {
        Bus::default()
    }

    pub fn at(address: &str) -> Bus {
        Bus {
            address: Some(address.to_string()),
        }
    }

    pub async fn connect(&self) -> zbus::Result<Connection> {
        match &self.address {
            None => Connection::session().await,
            Some(a) => {
                zbus::connection::Builder::address(a.as_str())?
                    .build()
                    .await
            }
        }
    }
}

/// A fresh `handle_token`: unique within this process, as the portal asks.
pub fn token() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!("nocturne{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// The request object a call with `token` gets from the connection named `unique` (`:1.42`).
pub fn request_path(unique: &str, token: &str) -> String {
    format!(
        "{PATH}/request/{}/{token}",
        unique.trim_start_matches(':').replace('.', "_")
    )
}

/// The next message on a stream, or `None` when it ends.
async fn next(stream: &mut MessageStream) -> Option<zbus::Result<zbus::Message>> {
    std::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}

/// Call `method` of portal `interface` with `body`, whose options carry a [`token`] as their
/// `handle_token`, and wait for the answer. A refusal or a cancelled question is an error. The
/// wait has no limit of its own: it is runtime-neutral, so the caller bounds it.
pub async fn request<B>(
    conn: &Connection,
    interface: &str,
    method: &str,
    body: &B,
) -> Result<Results, Failed>
where
    B: serde::Serialize + DynamicType,
{
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(DESTINATION)
        .and_then(|r| r.interface(REQUEST))
        .and_then(|r| r.member("Response"))
        .map_err(Failed::silent)?
        .build();
    let mut answers = MessageStream::for_match_rule(rule, conn, None)
        .await
        .map_err(Failed::silent)?;
    let reply = conn
        .call_method(Some(DESTINATION), PATH, Some(interface), method, body)
        .await
        .map_err(Failed::silent)?;
    let handle: OwnedObjectPath = reply.body().deserialize().map_err(Failed::silent)?;
    while let Some(message) = next(&mut answers).await {
        let Ok(message) = message else { continue };
        if message.header().path().map(|p| p.as_str()) != Some(handle.as_str()) {
            continue;
        }
        let (code, results): (u32, Results) =
            message.body().deserialize().map_err(Failed::silent)?;
        return match code {
            0 => Ok(results),
            1 => Err(Failed::refused(format!("{method} was declined"))),
            _ => Err(Failed::refused(format!("{method} did not finish"))),
        };
    }
    Err(Failed::silent("the portal went away"))
}

/// Ask the Background portal to start the app at login with `commandline`, or to stop. The
/// portal writes (or removes) the desktop's own entry, which runs the app through
/// `flatpak run`; the sandbox cannot reach the folder the desktop reads.
pub async fn autostart(conn: &Connection, want: bool, commandline: &[&str]) -> Result<(), Failed> {
    let token = token();
    let mut options = Options::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("autostart", Value::from(want));
    options.insert("commandline", Value::from(commandline.to_vec()));
    let body = ("", options);
    let asked = request(conn, BACKGROUND, "RequestBackground", &body);
    let results = tokio::time::timeout(ANSWER_LIMIT, asked)
        .await
        .map_err(|_| Failed::silent("RequestBackground had no answer"))??;
    let granted = results
        .get("autostart")
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false);
    if want && !granted {
        return Err(Failed::refused(
            "the desktop did not allow starting with this computer",
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod testbus {
    //! A private bus for tests, with nothing on it but what the test puts there.

    use std::io::{BufRead, BufReader};
    use std::process::{Child, Stdio};

    pub struct TestBus {
        child: Child,
        pub address: String,
        _dir: tempfile::TempDir,
    }

    impl TestBus {
        pub fn start() -> TestBus {
            let dir = tempfile::tempdir().unwrap();
            let config = dir.path().join("bus.conf");
            std::fs::write(
                &config,
                format!(
                    "<busconfig><type>session</type><listen>unix:dir={}</listen>\
                     <auth>EXTERNAL</auth><policy context=\"default\">\
                     <allow send_destination=\"*\" eavesdrop=\"true\"/>\
                     <allow eavesdrop=\"true\"/><allow own=\"*\"/></policy></busconfig>",
                    dir.path().display()
                ),
            )
            .unwrap();
            let mut child = nocturne_lemonade::process::command("dbus-daemon")
                .arg(format!("--config-file={}", config.display()))
                .args(["--nofork", "--nopidfile", "--print-address=1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("the tests need dbus-daemon");
            let mut line = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut line)
                .unwrap();
            TestBus {
                child,
                address: line.trim().to_string(),
                _dir: dir,
            }
        }
    }

    impl Drop for TestBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testbus::TestBus;
    use super::*;
    use std::sync::{Arc, Mutex};
    use zbus::message::Header;

    #[test]
    fn the_request_path_follows_the_bus_name() {
        assert_eq!(
            request_path(":1.42", "nocturne7"),
            "/org/freedesktop/portal/desktop/request/1_42/nocturne7"
        );
        assert_ne!(token(), token());
    }

    type Seen = Arc<Mutex<Vec<(bool, Vec<String>)>>>;

    /// The Background portal: answers each request with `code` and `granted`, before the call
    /// itself returns.
    struct FakeBackground {
        code: u32,
        granted: bool,
        seen: Seen,
    }

    #[zbus::interface(name = "org.freedesktop.portal.Background")]
    impl FakeBackground {
        async fn request_background(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] conn: &Connection,
            _parent: &str,
            options: HashMap<String, OwnedValue>,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            let want = bool::try_from(&options["autostart"]).unwrap();
            let line: Vec<String> =
                Vec::try_from(options["commandline"].try_clone().unwrap()).unwrap();
            self.seen.lock().unwrap().push((want, line));
            let token: &str = (&options["handle_token"]).try_into().unwrap();
            let sender = header.sender().unwrap().to_string();
            let path = request_path(&sender, token);
            let mut results: HashMap<&str, Value> = HashMap::new();
            results.insert("autostart", Value::from(self.granted && want));
            conn.emit_signal(
                Some(sender.as_str()),
                path.as_str(),
                REQUEST,
                "Response",
                &(self.code, results),
            )
            .await?;
            Ok(OwnedObjectPath::try_from(path).unwrap())
        }
    }

    async fn portal(bus: &TestBus, code: u32, granted: bool) -> (Connection, Seen) {
        let seen = Seen::default();
        let fake = FakeBackground {
            code,
            granted,
            seen: seen.clone(),
        };
        let server = zbus::connection::Builder::address(bus.address.as_str())
            .unwrap()
            .name(DESTINATION)
            .unwrap()
            .serve_at(PATH, fake)
            .unwrap()
            .build()
            .await
            .unwrap();
        (server, seen)
    }

    #[tokio::test]
    async fn the_background_portal_is_asked_with_the_command_line() {
        let bus = TestBus::start();
        let (_server, seen) = portal(&bus, 0, true).await;
        let conn = Bus::at(&bus.address).connect().await.unwrap();
        autostart(&conn, true, &["/app/bin/nocturne", "--hidden"])
            .await
            .unwrap();
        autostart(&conn, false, &["/app/bin/nocturne", "--hidden"])
            .await
            .unwrap();
        let line = vec!["/app/bin/nocturne".to_string(), "--hidden".to_string()];
        assert_eq!(
            *seen.lock().unwrap(),
            vec![(true, line.clone()), (false, line)]
        );
    }

    #[tokio::test]
    async fn a_declined_or_withheld_start_is_an_error() {
        let bus = TestBus::start();
        let (_server, _) = portal(&bus, 1, true).await;
        let conn = Bus::at(&bus.address).connect().await.unwrap();
        let refused = autostart(&conn, true, &["nocturne"]).await.unwrap_err();
        assert!(refused.answered, "a decline is an answer");

        let bus = TestBus::start();
        let (_server, _) = portal(&bus, 0, false).await;
        let conn = Bus::at(&bus.address).connect().await.unwrap();
        assert!(
            autostart(&conn, true, &["nocturne"])
                .await
                .is_err_and(|e| e.answered),
            "allowed to run, but not at login"
        );
        assert!(
            autostart(&conn, false, &["nocturne"]).await.is_ok(),
            "turning it off needs no permission"
        );
    }

    #[tokio::test]
    async fn no_portal_on_the_bus_is_an_error() {
        let bus = TestBus::start();
        let conn = Bus::at(&bus.address).connect().await.unwrap();
        let gone = autostart(&conn, true, &["nocturne"]).await.unwrap_err();
        assert!(!gone.answered, "no portal gives no answer");
    }
}
