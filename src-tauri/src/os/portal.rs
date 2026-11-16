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
use zbus::names::OwnedUniqueName;
use zbus::zvariant::{DynamicType, OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, MatchRule, MessageStream};

pub const DESTINATION: &str = "org.freedesktop.portal.Desktop";
pub const PATH: &str = "/org/freedesktop/portal/desktop";
const REQUEST: &str = "org.freedesktop.portal.Request";
const BACKGROUND: &str = "org.freedesktop.portal.Background";
const GLOBAL_SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";
const SESSION: &str = "org.freedesktop.portal.Session";

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

/// Each change of the portal's owner: `Some` with its new unique name, `None` when it went away
/// (it crashed or was stopped; the next call starts it again). Sessions do not survive either.
pub async fn owner_changes(conn: &Connection) -> zbus::Result<OwnerChanges> {
    let proxy = zbus::fdo::DBusProxy::new(conn).await?;
    let changes = proxy
        .receive_name_owner_changed_with_args(&[(0, DESTINATION)])
        .await?;
    Ok(OwnerChanges(changes))
}

/// The stream [`owner_changes`] returns.
pub struct OwnerChanges(zbus::fdo::NameOwnerChangedStream);

impl Stream for OwnerChanges {
    type Item = Option<OwnedUniqueName>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;
        loop {
            match Pin::new(&mut self.0).poll_next(cx) {
                Poll::Ready(Some(signal)) => {
                    // A change that cannot be read says nothing about the portal.
                    if let Ok(args) = signal.args() {
                        return Poll::Ready(Some(
                            args.new_owner().as_ref().map(|n| n.to_owned().into()),
                        ));
                    }
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// The next item of any stream that can be polled in place.
pub async fn next_of<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    std::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}

/// The unique bus name the portal answers from now.
pub async fn owner(conn: &Connection) -> zbus::Result<OwnedUniqueName> {
    let name = zbus::names::BusName::try_from(DESTINATION)?;
    Ok(zbus::fdo::DBusProxy::new(conn)
        .await?
        .get_name_owner(name)
        .await?)
}

/// Whether `message` came from `sender`. The match rules already ask the bus for the portal's
/// messages only; this checks it again, so nothing else on the bus can answer for the portal.
fn sent_by(message: &zbus::Message, sender: &OwnedUniqueName) -> bool {
    message
        .header()
        .sender()
        .is_some_and(|s| s.as_str() == sender.as_str())
}

/// Withdraw a request that is still open, so the desktop stops asking the person.
pub async fn cancel(conn: &Connection, request: &str) {
    let _ = conn
        .call_method(Some(DESTINATION), request, Some(REQUEST), "Close", &())
        .await;
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
    let portal = owner(conn).await.map_err(Failed::silent)?;
    while let Some(message) = next(&mut answers).await {
        let Ok(message) = message else { continue };
        if message.header().path().map(|p| p.as_str()) != Some(handle.as_str())
            || !sent_by(&message, &portal)
        {
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
    autostart_within(conn, want, commandline, ANSWER_LIMIT).await
}

/// [`autostart`], waiting at most `limit` for the answer. A request still open then is withdrawn,
/// so an answer that comes later cannot change the entry behind the setting's back.
async fn autostart_within(
    conn: &Connection,
    want: bool,
    commandline: &[&str],
    limit: Duration,
) -> Result<(), Failed> {
    let token = token();
    let mut options = Options::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("autostart", Value::from(want));
    options.insert("commandline", Value::from(commandline.to_vec()));
    let body = ("", options);
    let asked = request(conn, BACKGROUND, "RequestBackground", &body);
    let Ok(answer) = tokio::time::timeout(limit, asked).await else {
        if let Some(me) = conn.unique_name() {
            let path = request_path(me.as_str(), &token);
            let _ = tokio::time::timeout(Duration::from_secs(5), cancel(conn, &path)).await;
        }
        return Err(Failed::silent("RequestBackground had no answer"));
    };
    let results = answer?;
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

/// The version of the desktop's GlobalShortcuts portal, or `None` when it has none. Version 2
/// can open the desktop's own settings for a shortcut.
pub async fn global_shortcuts_version(conn: &Connection) -> Option<u32> {
    let reply = conn
        .call_method(
            Some(DESTINATION),
            PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(GLOBAL_SHORTCUTS, "version"),
        )
        .await
        .ok()?;
    let value: OwnedValue = reply.body().deserialize().ok()?;
    u32::try_from(value).ok()
}

/// The session object a `CreateSession` with `token` makes for the connection named `unique`.
#[cfg(test)]
pub fn session_path(unique: &str, token: &str) -> String {
    format!(
        "{PATH}/session/{}/{token}",
        unique.trim_start_matches(':').replace('.', "_")
    )
}

/// Open the desktop's own settings for the shortcuts of `session` (version 2).
pub async fn configure(conn: &Connection, session: &OwnedObjectPath) -> Result<(), String> {
    conn.call_method(
        Some(DESTINATION),
        PATH,
        Some(GLOBAL_SHORTCUTS),
        "ConfigureShortcuts",
        &(session, "", Options::new()),
    )
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
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
/// keys than the ones suggested. The session lasts until [`close`] or until `conn` closes. The
/// app runs the two steps itself, so it can stop between them; tests use this whole.
#[cfg(test)]
pub async fn bind_quick_keep(
    conn: &Connection,
    description: &str,
    preferred: Option<&str>,
    session_token: &str,
) -> Result<Bound, String> {
    let session = create_session(conn, session_token).await?;
    let [_, bind_token] = bind_tokens(session_token);
    let bound = bind(conn, &session, description, preferred, &bind_token).await;
    if bound.is_err() {
        close(conn, &session).await;
    }
    bound.map(|trigger| Bound { session, trigger })
}

/// A new GlobalShortcuts session named after `session_token`. The desktop does not ask the
/// person here, so the answer comes at once.
pub async fn create_session(
    conn: &Connection,
    session_token: &str,
) -> Result<OwnedObjectPath, String> {
    let [create_token, _] = bind_tokens(session_token);
    let mut options = Options::new();
    options.insert("handle_token", Value::from(create_token.as_str()));
    options.insert("session_handle_token", Value::from(session_token));
    let created = request(conn, GLOBAL_SHORTCUTS, "CreateSession", &(options,)).await?;
    Ok(created
        .get("session_handle")
        .and_then(object_path)
        .ok_or("the portal made no session")?)
}

/// Bind the quick keep shortcut in `session`, the request named after `token`, and return the
/// keys the desktop bound. This is where the desktop may ask the person.
pub async fn bind(
    conn: &Connection,
    session: &OwnedObjectPath,
    description: &str,
    preferred: Option<&str>,
    token: &str,
) -> Result<Option<String>, String> {
    let mut shortcut: HashMap<&str, Value> = HashMap::new();
    shortcut.insert("description", Value::from(description));
    if let Some(p) = preferred {
        shortcut.insert("preferred_trigger", Value::from(p));
    }
    let mut options = Options::new();
    options.insert("handle_token", Value::from(token));
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

/// The `handle_token`s of the two requests made with `session_token`: the first is
/// `CreateSession`'s, the second `BindShortcuts`'s, so a caller can withdraw them with
/// [`cancel`].
pub fn bind_tokens(session_token: &str) -> [String; 2] {
    [format!("{session_token}_c"), format!("{session_token}_b")]
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

/// The session of the next press of the quick keep shortcut whose sender `is_portal` accepts
/// (the portal's [`owner`] at the time), or `None` when the stream ends.
pub async fn next_activation(
    stream: &mut MessageStream,
    is_portal: &(dyn Fn(&str) -> bool + Send + Sync),
) -> Option<OwnedObjectPath> {
    while let Some(message) = next(stream).await {
        let Ok(message) = message else { continue };
        if !message
            .header()
            .sender()
            .is_some_and(|s| is_portal(s.as_str()))
        {
            continue;
        }
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
            // A socket path is limited to 107 bytes, which a long TMPDIR can exceed.
            let dir = tempfile::Builder::new()
                .prefix("nbus")
                .tempdir_in("/tmp")
                .unwrap();
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
                .unwrap_or_else(|e| {
                    panic!(
                        "could not start dbus-daemon ({e}): the portal tests run a private bus \
                         and need it on the PATH (the dbus or dbus-daemon package)"
                    )
                });
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
    /// itself returns. When `silent`, it never answers, and records each request withdrawn.
    struct FakeBackground {
        code: u32,
        granted: bool,
        seen: Seen,
        silent: bool,
        withdrawn: Arc<Mutex<Vec<String>>>,
    }

    struct FakeRequest {
        path: String,
        withdrawn: Arc<Mutex<Vec<String>>>,
    }

    #[zbus::interface(name = "org.freedesktop.portal.Request")]
    impl FakeRequest {
        async fn close(&self) {
            self.withdrawn.lock().unwrap().push(self.path.clone());
        }
    }

    #[zbus::interface(name = "org.freedesktop.portal.Background")]
    impl FakeBackground {
        async fn request_background(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] conn: &Connection,
            #[zbus(object_server)] server: &zbus::ObjectServer,
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
            if self.silent {
                let request = FakeRequest {
                    path: path.clone(),
                    withdrawn: self.withdrawn.clone(),
                };
                server.at(path.as_str(), request).await?;
                return Ok(OwnedObjectPath::try_from(path).unwrap());
            }
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
        let (server, seen, _) = fake_portal(bus, code, granted, false).await;
        (server, seen)
    }

    async fn fake_portal(
        bus: &TestBus,
        code: u32,
        granted: bool,
        silent: bool,
    ) -> (Connection, Seen, Arc<Mutex<Vec<String>>>) {
        let seen = Seen::default();
        let withdrawn = Arc::new(Mutex::new(Vec::new()));
        let fake = FakeBackground {
            code,
            granted,
            seen: seen.clone(),
            silent,
            withdrawn: withdrawn.clone(),
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
        (server, seen, withdrawn)
    }

    #[tokio::test]
    async fn a_request_with_no_answer_in_time_is_withdrawn() {
        let bus = TestBus::start();
        let (_server, seen, withdrawn) = fake_portal(&bus, 0, true, true).await;
        let conn = Bus::at(&bus.address).connect().await.unwrap();
        let err = autostart_within(&conn, true, &["nocturne"], Duration::from_millis(300))
            .await
            .unwrap_err();
        assert!(!err.answered, "no answer is not a refusal");
        assert_eq!(seen.lock().unwrap().len(), 1);
        let withdrawn = withdrawn.lock().unwrap().clone();
        assert_eq!(withdrawn.len(), 1, "{withdrawn:?}");
        assert!(
            withdrawn[0].starts_with(&request_path(conn.unique_name().unwrap().as_str(), "")),
            "the request this connection made: {withdrawn:?}"
        );
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
        pub configured: Closed,
        pub created: Closed,
        pub answered: Closed,
        /// The binds withdrawn with `Request.Close`, by request path.
        pub withdrawn: Closed,
        /// The binds that have reached it, by request path, before it holds them.
        pub arrived: Closed,
        /// Hold every bind this long before answering, as a person deciding does.
        pub delay: Duration,
    }

    struct FakeBind {
        path: String,
        withdrawn: Closed,
    }

    #[zbus::interface(name = "org.freedesktop.portal.Request")]
    impl FakeBind {
        async fn close(&self) {
            self.withdrawn.lock().unwrap().push(self.path.clone());
        }
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
            2
        }

        async fn configure_shortcuts(
            &self,
            session: OwnedObjectPath,
            _parent: &str,
            _options: HashMap<String, OwnedValue>,
        ) {
            self.configured.lock().unwrap().push(session.to_string());
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
            let session = session_path(&sender, &text(&options, "session_handle_token").unwrap());
            self.created.lock().unwrap().push(session.clone());
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

        #[allow(clippy::too_many_arguments)]
        async fn bind_shortcuts(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] conn: &Connection,
            #[zbus(object_server)] server: &zbus::ObjectServer,
            session: OwnedObjectPath,
            shortcuts: Vec<(String, HashMap<String, OwnedValue>)>,
            _parent: &str,
            options: HashMap<String, OwnedValue>,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            let sender = header.sender().unwrap().to_string();
            let request = request_path(&sender, &text(&options, "handle_token").unwrap());
            server
                .at(
                    request.as_str(),
                    FakeBind {
                        path: request.clone(),
                        withdrawn: self.withdrawn.clone(),
                    },
                )
                .await?;
            self.arrived.lock().unwrap().push(request.clone());
            if !self.delay.is_zero() {
                // Waits off the bus's executor, so other calls go on meanwhile.
                let (done, waited) = tokio::sync::oneshot::channel();
                let delay = self.delay;
                std::thread::spawn(move || {
                    std::thread::sleep(delay);
                    let _ = done.send(());
                });
                let _ = waited.await;
            }
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
            self.answered.lock().unwrap().push(request.clone());
            Ok(OwnedObjectPath::try_from(request).unwrap())
        }
    }

    pub(crate) struct Fake {
        pub server: Connection,
        pub binds: Binds,
        pub closed: Closed,
        pub configured: Closed,
        pub created: Closed,
        /// The binds it has answered, by request path.
        pub answered: Closed,
        pub withdrawn: Closed,
        pub arrived: Closed,
    }

    impl Fake {
        /// The bus name of the client that owns `session` (`/session/1_42/t` is `:1.42`).
        pub(crate) fn client_of(&self, session: &str) -> String {
            let owner = session.split('/').rev().nth(1).unwrap();
            format!(":{}", owner.replace('_', "."))
        }
    }

    pub(crate) async fn shortcuts_portal(bus: &TestBus, code: u32, delay: Duration) -> Fake {
        let binds = Binds::default();
        let closed = Closed::default();
        let configured = Closed::default();
        let created = Closed::default();
        let answered = Closed::default();
        let withdrawn = Closed::default();
        let arrived = Closed::default();
        let fake = FakeShortcuts {
            code,
            keys: "Ctrl+Shift+Space".into(),
            binds: binds.clone(),
            closed: closed.clone(),
            configured: configured.clone(),
            created: created.clone(),
            answered: answered.clone(),
            withdrawn: withdrawn.clone(),
            arrived: arrived.clone(),
            delay,
        };
        // A second fake takes the name over, as a portal that restarts does.
        let server = zbus::connection::Builder::address(bus.address.as_str())
            .unwrap()
            .allow_name_replacements(true)
            .replace_existing_names(true)
            .name(DESTINATION)
            .unwrap()
            .serve_at(PATH, fake)
            .unwrap()
            .build()
            .await
            .unwrap();
        Fake {
            server,
            binds,
            closed,
            configured,
            created,
            answered,
            withdrawn,
            arrived,
        }
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
        let Fake {
            server,
            binds,
            closed,
            configured,
            ..
        } = shortcuts_portal(&bus, 0, Duration::ZERO).await;
        let conn = Bus::at(&bus.address).connect().await.unwrap();
        assert_eq!(global_shortcuts_version(&conn).await, Some(2));
        let mut presses = activations(&conn).await.unwrap();
        let session_token = token();
        let bound = bind_quick_keep(
            &conn,
            "Keep something",
            Some("CTRL+SHIFT+space"),
            &session_token,
        )
        .await
        .unwrap();
        assert_eq!(
            bound.session.as_str(),
            session_path(conn.unique_name().unwrap(), &session_token)
        );
        assert_eq!(bound.trigger.as_deref(), Some("Ctrl+Shift+Space"));
        assert_eq!(
            *binds.lock().unwrap(),
            vec![(bound.session.to_string(), Some("CTRL+SHIFT+space".into()))]
        );
        let portal = owner(&conn).await.unwrap();
        let is_portal = |sender: &str| sender == portal.as_str();
        press(&server, conn.unique_name().unwrap(), bound.session.as_str()).await;
        assert_eq!(
            next_activation(&mut presses, &is_portal).await,
            Some(bound.session.clone())
        );
        configure(&conn, &bound.session).await.unwrap();
        assert_eq!(*configured.lock().unwrap(), vec![bound.session.to_string()]);
        close(&conn, &bound.session).await;
        assert_eq!(*closed.lock().unwrap(), vec![bound.session.to_string()]);
    }

    #[tokio::test]
    async fn a_press_from_anyone_but_the_portal_is_ignored() {
        let bus = TestBus::start();
        let fake = shortcuts_portal(&bus, 0, Duration::ZERO).await;
        let conn = Bus::at(&bus.address).connect().await.unwrap();
        let impostor = Bus::at(&bus.address).connect().await.unwrap();
        // No sender in the rule, so only `next_activation` itself can tell the two apart.
        let rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface(GLOBAL_SHORTCUTS)
            .and_then(|r| r.member("Activated"))
            .unwrap()
            .build();
        let mut presses = MessageStream::for_match_rule(rule, &conn, None)
            .await
            .unwrap();
        let portal = owner(&conn).await.unwrap();
        let is_portal = |sender: &str| sender == portal.as_str();
        let me = conn.unique_name().unwrap().to_string();
        let forged = format!("{PATH}/session/forged/t");
        let real = format!("{PATH}/session/real/t");
        press(&impostor, &me, &forged).await;
        press(&fake.server, &me, &real).await;
        assert_eq!(
            next_activation(&mut presses, &is_portal)
                .await
                .map(|s| s.to_string()),
            Some(real)
        );
    }

    #[tokio::test]
    async fn a_refused_bind_closes_its_session() {
        let bus = TestBus::start();
        let fake = shortcuts_portal(&bus, 1, Duration::ZERO).await;
        let conn = Bus::at(&bus.address).connect().await.unwrap();
        assert!(
            bind_quick_keep(&conn, "Keep something", None, &token())
                .await
                .is_err()
        );
        assert_eq!(fake.closed.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn without_the_portal_there_are_no_global_shortcuts() {
        let bus = TestBus::start();
        let conn = Bus::at(&bus.address).connect().await.unwrap();
        assert_eq!(global_shortcuts_version(&conn).await, None);
        let (_server, _) = portal(&bus, 0, true).await;
        assert_eq!(
            global_shortcuts_version(&conn).await,
            None,
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
