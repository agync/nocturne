//! One session's sealed channel: its keys and strict counters, in memory only.
//!
//! Counters start at 1 and each frame must carry exactly the last one plus one. Any refusal
//! closes the channel for good: a gap, a repeat, a bad tag, another kind or another sid. A
//! closed channel seals and opens nothing; the phone opens a new session instead.

use super::crypto::{self, Checked, Dir, Envelope, Key32, Refusal, SessionKeys, Sid};
use super::protocol::{CHUNK_HEADER_LEN, ENVELOPE_HEADER_LEN, GCM_TAG_LEN, HEADER_LEN, MAX_FRAME};

/// The largest chunk payload that still fits in one frame.
pub const MAX_CHUNK_PAYLOAD: usize =
    MAX_FRAME - HEADER_LEN - GCM_TAG_LEN - ENVELOPE_HEADER_LEN - CHUNK_HEADER_LEN;

/// Which end of the channel this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Computer,
    Phone,
}

pub struct Channel {
    sid: Sid,
    send_key: Key32,
    recv_key: Key32,
    send_dir: Dir,
    recv_dir: Dir,
    sent: u64,
    received: u64,
    closed: Option<Refusal>,
}

impl Channel {
    pub fn new(side: Side, sid: Sid, keys: SessionKeys) -> Channel {
        let SessionKeys { up, down } = keys;
        let (send_key, recv_key, send_dir, recv_dir) = match side {
            Side::Computer => (down, up, Dir::Down, Dir::Up),
            Side::Phone => (up, down, Dir::Up, Dir::Down),
        };
        Channel {
            sid,
            send_key,
            recv_key,
            send_dir,
            recv_dir,
            sent: 0,
            received: 0,
            closed: None,
        }
    }

    pub fn sid(&self) -> &Sid {
        &self.sid
    }

    /// Why it closed, once it has.
    pub fn closed(&self) -> Option<Refusal> {
        self.closed
    }

    pub fn close(&mut self, why: Refusal) {
        self.closed.get_or_insert(why);
    }

    /// Frames received so far.
    pub fn received(&self) -> u64 {
        self.received
    }

    /// Seal the next frame. None once closed.
    pub fn seal(&mut self, envelope: &[u8]) -> Option<Vec<u8>> {
        if self.closed.is_some() {
            return None;
        }
        self.sent += 1;
        Some(crypto::data_seal(
            &self.send_key,
            self.send_dir,
            &self.sid,
            self.sent,
            envelope,
        ))
    }

    /// Open the next frame; any refusal closes the channel.
    pub fn open(&mut self, frame: &[u8]) -> Checked<Envelope> {
        if let Some(why) = self.closed {
            return Err(why);
        }
        match crypto::data_open(
            frame,
            &self.recv_key,
            self.recv_dir,
            &self.sid,
            self.received,
        ) {
            Ok((ctr, env)) => {
                self.received = ctr;
                Ok(env)
            }
            Err(why) => {
                self.closed = Some(why);
                Err(why)
            }
        }
    }
}

/// Reassembles one incoming call's chunks, refusing a gap or a chunk after the last.
#[derive(Debug, Default)]
pub struct ChunkOrder {
    next: u32,
    done: bool,
}

impl ChunkOrder {
    pub fn accept(&mut self, chunk: &crypto::Chunk) -> Checked<()> {
        if self.done || chunk.seq != self.next {
            return Err(Refusal::ChunkGap);
        }
        self.next += 1;
        self.done = chunk.last;
        Ok(())
    }

    pub fn done(&self) -> bool {
        self.done
    }
}
