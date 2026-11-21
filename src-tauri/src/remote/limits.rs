//! Rate limits: token buckets per minute, each keyed (a shared bucket, or one per phone), on a
//! clock the tests move by hand.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::protocol::*;

/// A monotonic clock. The app uses the real one; tests use [`ManualMono`].
pub trait Mono: Send + Sync {
    fn now(&self) -> Instant;
}

pub struct RealMono;

impl Mono for RealMono {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// A clock that moves only when told to.
pub struct ManualMono(Mutex<Instant>);

impl ManualMono {
    pub fn new() -> Arc<ManualMono> {
        Arc::new(ManualMono(Mutex::new(Instant::now())))
    }

    pub fn advance(&self, by: Duration) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) += by;
    }
}

impl Mono for ManualMono {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// What a bucket limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    NoKey,
    Static,
    PairingFrames,
    FailedOpen,
    SessionOpens,
    Requests,
    Asks,
    Keeps,
    /// A phone app's calendar and health sends, per phone.
    Syncs,
}

impl Bucket {
    pub fn per_minute(self) -> u32 {
        match self {
            Bucket::NoKey => LIMIT_NO_KEY,
            Bucket::Static => LIMIT_STATIC,
            Bucket::PairingFrames => LIMIT_PAIRING_FRAMES,
            Bucket::FailedOpen => LIMIT_FAILED_SESSION_OPEN,
            Bucket::SessionOpens => LIMIT_SESSION_OPENS_PER_DEVICE,
            Bucket::Requests => LIMIT_REQUESTS,
            Bucket::Asks => LIMIT_ASKS,
            Bucket::Keeps => LIMIT_KEEPS,
            Bucket::Syncs => LIMIT_SYNCS,
        }
    }
}

struct Tokens {
    left: f64,
    at: Instant,
}

/// Every bucket, keyed by what it limits and whose it is (`""` for the shared ones).
pub struct Limits {
    mono: Arc<dyn Mono>,
    buckets: Mutex<HashMap<(Bucket, String), Tokens>>,
}

impl Limits {
    pub fn new(mono: Arc<dyn Mono>) -> Limits {
        Limits {
            mono,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Take one token; false when the bucket is empty.
    pub fn take(&self, bucket: Bucket, whose: &str) -> bool {
        let now = self.mono.now();
        let cap = f64::from(bucket.per_minute());
        let mut all = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if all.len() > 4096 {
            all.retain(|_, t| now.duration_since(t.at) < Duration::from_secs(60));
        }
        let t = all
            .entry((bucket, whose.to_string()))
            .or_insert(Tokens { left: cap, at: now });
        let refill = now.duration_since(t.at).as_secs_f64() * cap / 60.0;
        t.left = (t.left + refill).min(cap);
        t.at = now;
        if t.left >= 1.0 {
            t.left -= 1.0;
            true
        } else {
            false
        }
    }

    /// Whether a bucket still has a token, without taking it.
    pub fn has(&self, bucket: Bucket, whose: &str) -> bool {
        let now = self.mono.now();
        let cap = f64::from(bucket.per_minute());
        let all = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        all.get(&(bucket, whose.to_string()))
            .is_none_or(|t| t.left + now.duration_since(t.at).as_secs_f64() * cap / 60.0 >= 1.0)
    }

    pub fn forget(&self, whose: &str) {
        self.buckets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(_, w), _| w != whose);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_refill_over_a_minute() {
        let mono = ManualMono::new();
        let limits = Limits::new(mono.clone());
        for _ in 0..LIMIT_ASKS {
            assert!(limits.take(Bucket::Asks, "a"));
        }
        assert!(!limits.take(Bucket::Asks, "a"));
        assert!(limits.take(Bucket::Asks, "b"), "per phone");
        mono.advance(Duration::from_secs(6));
        assert!(limits.take(Bucket::Asks, "a"));
        assert!(!limits.take(Bucket::Asks, "a"));
        mono.advance(Duration::from_secs(60));
        assert!(limits.has(Bucket::Asks, "a"));
    }
}
