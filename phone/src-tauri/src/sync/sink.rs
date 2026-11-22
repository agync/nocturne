//! Where a send lands: across the channel to a paired computer, or into this phone's own store.
//! The reading, comparing and ledger are the same for both, so a phone that moves from one to
//! the other keeps what it already sent.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use nocturne_channel::client::{CallError, Reach};
use nocturne_contract::{
    CalendarBatch, PhoneError, ReadingBatch, RemoteAnswer, RemoteCall, SyncReceipt,
};

use super::Link;

/// What takes a batch and says how much of it it took.
pub trait Sink: Send + Sync {
    fn send_calendar(
        &self,
        batch: CalendarBatch,
    ) -> impl Future<Output = Result<SyncReceipt, CallError>> + Send;
    fn send_readings(
        &self,
        batch: ReadingBatch,
    ) -> impl Future<Output = Result<SyncReceipt, CallError>> + Send;
    fn pause(&self, how_long: Duration) -> impl Future<Output = ()> + Send;
}

/// The paired computer, over the channel.
pub struct ChannelSink<L: Link>(pub L);

impl<L: Link> ChannelSink<L> {
    async fn call(&self, call: RemoteCall) -> Result<SyncReceipt, CallError> {
        match self.0.send(call).await? {
            RemoteAnswer::Synced { receipt } => Ok(receipt),
            RemoteAnswer::Revoked => Err(CallError::Connection {
                state: Reach::Removed,
            }),
            RemoteAnswer::Error { code } => Err(CallError::Phone { code }),
            _ => Err(CallError::Phone {
                code: PhoneError::Invalid,
            }),
        }
    }
}

impl<L: Link> Sink for ChannelSink<L> {
    async fn send_calendar(&self, batch: CalendarBatch) -> Result<SyncReceipt, CallError> {
        self.call(RemoteCall::SendCalendar { batch }).await
    }

    async fn send_readings(&self, batch: ReadingBatch) -> Result<SyncReceipt, CallError> {
        self.call(RemoteCall::SendReadings { batch }).await
    }

    fn pause(&self, how_long: Duration) -> impl Future<Output = ()> + Send {
        self.0.pause(how_long)
    }
}

/// This phone's own store, as the engine takes batches from a device. A call does the whole
/// batch before it returns and may block, so [`EngineSink`] runs it off the async workers.
/// `Busy` says the store is not there to take it right now: the send waits and tries again.
pub trait LocalStore: Send + Sync + 'static {
    fn take_calendar(&self, batch: CalendarBatch) -> Result<SyncReceipt, PhoneError>;
    fn take_readings(&self, batch: ReadingBatch) -> Result<SyncReceipt, PhoneError>;
}

/// This phone's own store, with no computer between.
pub struct EngineSink<S: LocalStore>(Arc<S>);

impl<S: LocalStore> EngineSink<S> {
    pub fn new(store: S) -> EngineSink<S> {
        EngineSink(Arc::new(store))
    }

    async fn blocking(
        &self,
        f: impl FnOnce(&S) -> Result<SyncReceipt, PhoneError> + Send + 'static,
    ) -> Result<SyncReceipt, CallError> {
        let store = self.0.clone();
        tokio::task::spawn_blocking(move || f(&store))
            .await
            .unwrap_or(Err(PhoneError::Invalid))
            .map_err(|code| CallError::Phone { code })
    }
}

impl<S: LocalStore> Sink for EngineSink<S> {
    async fn send_calendar(&self, batch: CalendarBatch) -> Result<SyncReceipt, CallError> {
        self.blocking(move |s| s.take_calendar(batch)).await
    }

    async fn send_readings(&self, batch: ReadingBatch) -> Result<SyncReceipt, CallError> {
        self.blocking(move |s| s.take_readings(batch)).await
    }

    async fn pause(&self, how_long: Duration) {
        tokio::time::sleep(how_long).await;
    }
}

/// A sink seen as the link the sends talk through.
pub struct ViaSink<'a, S: Sink>(pub &'a S);

impl<S: Sink> Link for ViaSink<'_, S> {
    async fn send(&self, call: RemoteCall) -> Result<RemoteAnswer, CallError> {
        let receipt = match call {
            RemoteCall::SendCalendar { batch } => self.0.send_calendar(batch).await?,
            RemoteCall::SendReadings { batch } => self.0.send_readings(batch).await?,
            _ => {
                return Err(CallError::Phone {
                    code: PhoneError::Invalid,
                });
            }
        };
        Ok(RemoteAnswer::Synced { receipt })
    }

    fn pause(&self, how_long: Duration) -> impl Future<Output = ()> + Send {
        self.0.pause(how_long)
    }
}

impl<L: Link> Link for &L {
    fn send(
        &self,
        call: RemoteCall,
    ) -> impl Future<Output = Result<RemoteAnswer, CallError>> + Send {
        (**self).send(call)
    }

    fn pause(&self, how_long: Duration) -> impl Future<Output = ()> + Send {
        (**self).pause(how_long)
    }
}
