//! The WebSocket link: binary frames both ways; the listener's pings are answered by the socket.

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::{Connector, connect_async_tls_with_config};

use super::link::{InboxTx, Incoming, LinkEnd, Route, tls};
use crate::protocol::MAX_FRAME;

enum Cmd {
    Frame(Vec<u8>, oneshot::Sender<bool>),
    Close,
}

pub(crate) struct WsLink {
    cmds: mpsc::UnboundedSender<Cmd>,
}

impl WsLink {
    pub(crate) async fn open(route: &Route, inbox: InboxTx) -> Result<WsLink, ()> {
        let connector = if route.secure() {
            Connector::Rustls(tls())
        } else {
            Connector::Plain
        };
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_FRAME))
            .max_frame_size(Some(MAX_FRAME));
        let (stream, _) =
            connect_async_tls_with_config(route.ws_url(), Some(config), true, Some(connector))
                .await
                .map_err(|_| ())?;
        let (mut sink, mut source) = stream.split();
        let (cmds, mut rx) = mpsc::unbounded_channel::<Cmd>();
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    Cmd::Frame(f, done) => {
                        let ok = sink.send(Message::Binary(f.into())).await.is_ok();
                        let _ = done.send(ok);
                        if !ok {
                            break;
                        }
                    }
                    Cmd::Close => {
                        let _ = sink.close().await;
                        break;
                    }
                }
            }
        });
        tokio::spawn(async move {
            let end = loop {
                match source.next().await {
                    Some(Ok(Message::Binary(b))) => {
                        if b.len() > MAX_FRAME {
                            break LinkEnd::Protocol;
                        }
                        if inbox.send(Incoming::Frame(b.to_vec())).is_err() {
                            return;
                        }
                    }
                    Some(Ok(Message::Close(frame))) => {
                        break LinkEnd::Closed {
                            code: frame.map_or(1005, |f| u16::from(f.code)),
                        };
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => break LinkEnd::Closed { code: 1006 },
                }
            };
            let _ = inbox.send(Incoming::End(end));
        });
        Ok(WsLink { cmds })
    }

    pub(crate) async fn send(&self, frame: Vec<u8>) -> Result<(), ()> {
        let (done, sent) = oneshot::channel();
        self.cmds.send(Cmd::Frame(frame, done)).map_err(|_| ())?;
        match sent.await {
            Ok(true) => Ok(()),
            _ => Err(()),
        }
    }

    pub(crate) fn close(&self) {
        let _ = self.cmds.send(Cmd::Close);
    }
}
