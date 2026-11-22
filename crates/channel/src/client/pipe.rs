//! An in-memory link for tests: the phone's end of a pipe whose other end a test holds, so a
//! client can be driven against a scripted computer with no socket at all. A route named
//! `pipe://<name>` reaches the listener registered under that name in this process.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use tokio::sync::mpsc;

use super::link::{InboxTx, Incoming, LinkEnd};

type Listeners = Mutex<HashMap<String, mpsc::UnboundedSender<Conn>>>;

fn listeners() -> &'static Listeners {
    static LISTENERS: OnceLock<Listeners> = OnceLock::new();
    LISTENERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The phone's end: frames it sends reach the computer's [`Conn`]; the connection closes when
/// it is closed or dropped.
pub(crate) struct PipeLink {
    up: Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>,
}

impl PipeLink {
    /// Connects to the listener named `name`, handing it the computer's end. None when nothing
    /// listens under that name.
    pub(crate) fn open(name: &str, inbox: InboxTx) -> Option<PipeLink> {
        let (up, from_phone) = mpsc::unbounded_channel();
        let conn = Conn {
            from_phone,
            to_phone: inbox,
        };
        listeners().lock().unwrap().get(name)?.send(conn).ok()?;
        Some(PipeLink {
            up: Mutex::new(Some(up)),
        })
    }

    pub(crate) async fn send(&self, frame: Vec<u8>) -> Result<(), ()> {
        match self.up.lock().unwrap().as_ref() {
            Some(up) => up.send(frame).map_err(|_| ()),
            None => Err(()),
        }
    }

    pub(crate) fn close(&self) {
        self.up.lock().unwrap().take();
    }
}

/// The computer's end of one connection.
pub(crate) struct Conn {
    from_phone: mpsc::UnboundedReceiver<Vec<u8>>,
    to_phone: InboxTx,
}

impl Conn {
    /// The next frame the phone sent, or `None` once the phone closed.
    pub(crate) async fn recv(&mut self) -> Option<Vec<u8>> {
        tokio::time::timeout(std::time::Duration::from_secs(5), self.from_phone.recv())
            .await
            .expect("the phone sent nothing for five seconds")
    }

    pub(crate) fn send(&self, frame: Vec<u8>) {
        let _ = self.to_phone.send(Incoming::Frame(frame));
    }

    /// Ends the connection the way a socket or a poll would.
    pub(crate) fn end(&self, end: LinkEnd) {
        let _ = self.to_phone.send(Incoming::End(end));
    }
}

/// Listens for connections under a fresh name.
pub(crate) struct Listener {
    pub(crate) name: String,
    conns: mpsc::UnboundedReceiver<Conn>,
}

impl Listener {
    pub(crate) fn new() -> Listener {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let name = format!("pipe-{}", NEXT.fetch_add(1, Ordering::SeqCst));
        let (tx, conns) = mpsc::unbounded_channel();
        listeners().lock().unwrap().insert(name.clone(), tx);
        Listener { name, conns }
    }

    /// The next connection the phone opens.
    pub(crate) async fn accept(&mut self) -> Conn {
        tokio::time::timeout(std::time::Duration::from_secs(5), self.conns.recv())
            .await
            .expect("the phone opened nothing for five seconds")
            .expect("the listener is open")
    }

    /// Whether the phone opened another connection that nobody accepted.
    pub(crate) fn pending(&mut self) -> bool {
        self.conns.try_recv().is_ok()
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        listeners().lock().unwrap().remove(&self.name);
    }
}
