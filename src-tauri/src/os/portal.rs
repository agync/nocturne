//! The desktop portals, for what a Flatpak cannot do from inside its sandbox, and what no app
//! can do itself on Wayland (shortcuts that work from any app).
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
const GLOBAL_SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";
const SESSION: &str = "org.freedesktop.portal.Session";

/// How long the person has to answer the desktop's question.
pub const ANSWER_LIMIT: Duration = Duration::from_secs(300);

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

/// The id of the one shortcut the app binds through the portal.
pub const QUICK_KEEP: &str = "quick-keep";

/// Whether the desktop offers shortcuts that work from any app through the portal.
pub async fn has_global_shortcuts(conn: &Connection) -> bool {
    conn.call_method(
        Some(DESTINATION),
        PATH,
        Some("org.freedesktop.DBus.Properties"),
        "Get",
        &(GLOBAL_SHORTCUTS, "version"),
    )
    .await
    .is_ok()
}

/// An object path the portal sent as a path or, as some versions do, as a string.
fn object_path(value: &OwnedValue) -> Option<OwnedObjectPath> {
    match &**value {
        Value::ObjectPath(p) => Some(p.clone().into()),
        Value::Str(s) => OwnedObjectPath::try_from(s.as_str()).ok(),
        _ => None,
    }
}

/// A portal session holding the quick keep shortcut, and the keys the desktop bound it to.
#[derive(Debug, Clone)]
pub struct Bound {
    pub session: OwnedObjectPath,
    pub trigger: Option<String>,
}

/// Bind the quick keep shortcut in a new session, suggesting `preferred` (in the portal's
/// spelling, `CTRL+SHIFT+space`). The desktop may ask the person first, and may bind other
/// keys than the ones suggested. The session lasts until [`close`] or until `conn` closes.
pub async fn bind_quick_keep(
    conn: &Connection,
    description: &str,
    preferred: Option<&str>,
) -> Result<Bound, String> {
    let request_token = token();
    let session_token = token();
    let mut options = Options::new();
    options.insert("handle_token", Value::from(request_token.as_str()));
    options.insert("session_handle_token", Value::from(session_token.as_str()));
    let created = request(conn, GLOBAL_SHORTCUTS, "CreateSession", &(options,)).await?;
    let session = created
        .get("session_handle")
        .and_then(object_path)
        .ok_or("the portal made no session")?;
    let bound = bind(conn, &session, description, preferred).await;
    if bound.is_err() {
        close(conn, &session).await;
    }
    bound.map(|trigger| Bound { session, trigger })
}

async fn bind(
    conn: &Connection,
    session: &OwnedObjectPath,
    description: &str,
    preferred: Option<&str>,
) -> Result<Option<String>, String> {
    let mut shortcut: HashMap<&str, Value> = HashMap::new();
    shortcut.insert("description", Value::from(description));
    if let Some(p) = preferred {
        shortcut.insert("preferred_trigger", Value::from(p));
    }
    let token = token();
    let mut options = Options::new();
    options.insert("handle_token", Value::from(token.as_str()));
    let body = (session, vec![(QUICK_KEEP, shortcut)], "", options);
    let results = request(conn, GLOBAL_SHORTCUTS, "BindShortcuts", &body).await?;
    let shortcuts: Vec<(String, Results)> = results
        .get("shortcuts")
        .and_then(|v| v.try_clone().ok())
        .and_then(|v| Vec::try_from(v).ok())
        .unwrap_or_default();
    let (_, ours) = shortcuts
        .into_iter()
        .find(|(id, _)| id == QUICK_KEEP)
        .ok_or("the desktop did not bind the shortcut")?;
    Ok(ours
        .get("trigger_description")
        .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
        .filter(|t| !t.is_empty()))
}

/// End a session, which lets its shortcut go.
pub async fn close(conn: &Connection, session: &OwnedObjectPath) {
    let _ = conn
        .call_method(
            Some(DESTINATION),
            session.as_str(),
            Some(SESSION),
            "Close",
            &(),
        )
        .await;
}

/// Every press of a shortcut this connection bound. Read it with [`next_activation`].
pub async fn activations(conn: &Connection) -> Result<MessageStream, String> {
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(DESTINATION)
        .and_then(|r| r.interface(GLOBAL_SHORTCUTS))
        .and_then(|r| r.member("Activated"))
        .map_err(|e| e.to_string())?
        .build();
    MessageStream::for_match_rule(rule, conn, None)
        .await
        .map_err(|e| e.to_string())
}

/// The session of the next press of the quick keep shortcut, or `None` when the stream ends.
pub async fn next_activation(stream: &mut MessageStream) -> Option<OwnedObjectPath> {
    while let Some(message) = next(stream).await {
        let Ok(message) = message else { continue };
        type Activated = (OwnedObjectPath, String, u64, Results);
        if let Ok((session, id, _, _)) = message.body().deserialize::<Activated>()
            && id == QUICK_KEEP
        {
            return Some(session);
        }
    }
    None
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
pub(crate) mod tests {
    use super::testbus::TestBus;
    use super::*;
    use std::sync::{Arc, Mutex};
    use zbus::message::Header;
    use zbus::zvariant::ObjectPath;

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

    type Binds = Arc<Mutex<Vec<(String, Option<String>)>>>;
    type Closed = Arc<Mutex<Vec<String>>>;

    /// The GlobalShortcuts portal: binds whatever it is asked to `keys`, or refuses with `code`.
    pub(crate) struct FakeShortcuts {
        pub code: u32,
        pub keys: String,
        pub binds: Binds,
        pub closed: Closed,
    }

    struct FakeSession {
        path: String,
        closed: Closed,
    }

    #[zbus::interface(name = "org.freedesktop.portal.Session")]
    impl FakeSession {
        async fn close(&self) {
            self.closed.lock().unwrap().push(self.path.clone());
        }
    }

    async fn answer(conn: &Connection, sender: &str, path: &str, code: u32, results: Options<'_>) {
        conn.emit_signal(Some(sender), path, REQUEST, "Response", &(code, results))
            .await
            .unwrap();
    }

    fn text(options: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
        options
            .get(key)
            .and_then(|v| String::try_from(v.try_clone().unwrap()).ok())
    }

    #[zbus::interface(name = "org.freedesktop.portal.GlobalShortcuts")]
    impl FakeShortcuts {
        #[zbus(property, name = "version")]
        fn version(&self) -> u32 {
            1
        }

        async fn create_session(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] conn: &Connection,
            #[zbus(object_server)] server: &zbus::ObjectServer,
            options: HashMap<String, OwnedValue>,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            let sender = header.sender().unwrap().to_string();
            let request = request_path(&sender, &text(&options, "handle_token").unwrap());
            let session = request.replace("/request/", "/session/")
                + "_"
                + &text(&options, "session_handle_token").unwrap();
            server
                .at(
                    session.as_str(),
                    FakeSession {
                        path: session.clone(),
                        closed: self.closed.clone(),
                    },
                )
                .await?;
            let mut results = Options::new();
            // Sent as a string, as some portal versions do.
            results.insert("session_handle", Value::from(session.clone()));
            answer(conn, &sender, &request, 0, results).await;
            Ok(OwnedObjectPath::try_from(request).unwrap())
        }

        async fn bind_shortcuts(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] conn: &Connection,
            session: OwnedObjectPath,
            shortcuts: Vec<(String, HashMap<String, OwnedValue>)>,
            _parent: &str,
            options: HashMap<String, OwnedValue>,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            let sender = header.sender().unwrap().to_string();
            let request = request_path(&sender, &text(&options, "handle_token").unwrap());
            let mut bound = Vec::new();
            for (id, props) in &shortcuts {
                self.binds
                    .lock()
                    .unwrap()
                    .push((session.to_string(), text(props, "preferred_trigger")));
                let mut out: HashMap<String, Value> = HashMap::new();
                out.insert(
                    "description".into(),
                    Value::from(text(props, "description").unwrap()),
                );
                out.insert("trigger_description".into(), Value::from(self.keys.clone()));
                bound.push((id.clone(), out));
            }
            let mut results = Options::new();
            results.insert("shortcuts", Value::from(bound));
            answer(conn, &sender, &request, self.code, results).await;
            Ok(OwnedObjectPath::try_from(request).unwrap())
        }
    }

    pub(crate) async fn shortcuts_portal(bus: &TestBus, code: u32) -> (Connection, Binds, Closed) {
        let binds = Binds::default();
        let closed = Closed::default();
        let fake = FakeShortcuts {
            code,
            keys: "Ctrl+Shift+Space".into(),
            binds: binds.clone(),
            closed: closed.clone(),
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
        (server, binds, closed)
    }

    /// Press the quick keep shortcut of `session`, as the desktop does.
    pub(crate) async fn press(server: &Connection, to: &str, session: &str) {
        let options: HashMap<&str, Value> = HashMap::new();
        server
            .emit_signal(
                Some(to),
                PATH,
                GLOBAL_SHORTCUTS,
                "Activated",
                &(
                    ObjectPath::try_from(session).unwrap(),
                    QUICK_KEEP,
                    0u64,
                    options,
                ),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn the_quick_keep_shortcut_is_bound_pressed_and_let_go() {
        let bus = TestBus::start();
        let (server, binds, closed) = shortcuts_portal(&bus, 0).await;
        let conn = Bus::at(&bus.address).connect().await.unwrap();
        assert!(has_global_shortcuts(&conn).await);
        let mut presses = activations(&conn).await.unwrap();
        let bound = bind_quick_keep(&conn, "Quick keep shortcut", Some("CTRL+SHIFT+space"))
            .await
            .unwrap();
        assert_eq!(bound.trigger.as_deref(), Some("Ctrl+Shift+Space"));
        assert_eq!(
            *binds.lock().unwrap(),
            vec![(bound.session.to_string(), Some("CTRL+SHIFT+space".into()))]
        );
        press(&server, conn.unique_name().unwrap(), bound.session.as_str()).await;
        assert_eq!(
            next_activation(&mut presses).await,
            Some(bound.session.clone())
        );
        close(&conn, &bound.session).await;
        assert_eq!(*closed.lock().unwrap(), vec![bound.session.to_string()]);
    }

    #[tokio::test]
    async fn a_refused_bind_closes_its_session() {
        let bus = TestBus::start();
        let (_server, _, closed) = shortcuts_portal(&bus, 1).await;
        let conn = Bus::at(&bus.address).connect().await.unwrap();
        assert!(
            bind_quick_keep(&conn, "Quick keep shortcut", None)
                .await
                .is_err()
        );
        assert_eq!(closed.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn without_the_portal_there_are_no_global_shortcuts() {
        let bus = TestBus::start();
        let conn = Bus::at(&bus.address).connect().await.unwrap();
        assert!(!has_global_shortcuts(&conn).await);
        let (_server, _) = portal(&bus, 0, true).await;
        assert!(
            !has_global_shortcuts(&conn).await,
            "a portal without the interface"
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
