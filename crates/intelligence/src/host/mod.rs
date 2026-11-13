//! The machine around the app: power, idle time, sleep and resume.
//!
//! Every question has an honest "I do not know" (`None`): the policy above treats an unknown
//! power state as mains, and unknown idle as idle only in the small hours. Real values are read
//! on a blocking thread, with a one second limit on each D-Bus call, and cached for 30 seconds.

use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

mod system;
pub use system::SystemHost;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

#[cfg(any(test, feature = "fake"))]
mod fake;
#[cfg(any(test, feature = "fake"))]
pub use fake::FakeHost;

/// How long a read value is trusted.
pub const CACHE_TTL: Duration = Duration::from_secs(30);

/// Holds the machine awake while it lives. Dropping it lets the machine sleep again.
pub struct SleepGuard {
    _held: Box<dyn Any + Send>,
}

impl SleepGuard {
    /// Hold `token` until the guard is dropped (a file descriptor, a thread handle, a flag).
    pub fn new(token: impl Any + Send) -> Self {
        SleepGuard {
            _held: Box::new(token),
        }
    }
}

impl std::fmt::Debug for SleepGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SleepGuard")
    }
}

/// What the app asks of the machine. Methods may block (D-Bus, Win32): call them through
/// [`probe`] from async code.
pub trait Host: Send + Sync {
    /// Whether the machine runs on mains power. `None` when it cannot be told.
    fn on_ac_power(&self) -> Option<bool>;
    /// Battery charge, 0 to 100. `None` with no battery or when it cannot be told.
    fn battery_percent(&self) -> Option<u8>;
    /// How long the person has been away from keyboard and mouse. `None` when unknown.
    fn idle_for(&self) -> Option<Duration>;
    /// Keep the machine from sleeping until the guard is dropped. `None` when it cannot.
    fn inhibit_sleep(&self) -> Option<SleepGuard>;
    /// How many times the machine has woken from sleep since the host started watching. A
    /// platform without the signal returns 0 always (the scheduler also checks the clock).
    fn resume_count(&self) -> u64 {
        0
    }
}

/// How long to wait for a host answer before treating it as unknown.
const PROBE_LIMIT: Duration = Duration::from_secs(3);

/// Ask the host on a blocking thread, giving up after a few seconds: a stuck bus must never
/// stall the scheduler.
pub async fn probe<T, F>(host: &Arc<dyn Host>, f: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce(&dyn Host) -> Option<T> + Send + 'static,
{
    probe_within(host, PROBE_LIMIT, f).await
}

async fn probe_within<T, F>(host: &Arc<dyn Host>, limit: Duration, f: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce(&dyn Host) -> Option<T> + Send + 'static,
{
    let host = host.clone();
    let job = tokio::task::spawn_blocking(move || f(host.as_ref()));
    match tokio::time::timeout(limit, job).await {
        Ok(Ok(v)) => v,
        _ => None,
    }
}

/// The idle time from a logind `IdleSinceHint` (microseconds since the epoch) at `now_us`.
pub(crate) fn idle_from_hint(idle_hint: bool, since_us: u64, now_us: u64) -> Duration {
    if !idle_hint || since_us == 0 {
        return Duration::ZERO;
    }
    Duration::from_micros(now_us.saturating_sub(since_us))
}

/// The idle time from Windows tick counts, which wrap every 49.7 days.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn idle_from_ticks(now_tick: u32, last_input_tick: u32) -> Duration {
    Duration::from_millis(u64::from(now_tick.wrapping_sub(last_input_tick)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_from_a_logind_hint() {
        assert_eq!(idle_from_hint(false, 5, 100), Duration::ZERO);
        assert_eq!(idle_from_hint(true, 0, 100), Duration::ZERO);
        assert_eq!(idle_from_hint(true, 40, 100), Duration::from_micros(60));
        // A clock that went backwards never gives a negative idle time.
        assert_eq!(idle_from_hint(true, 200, 100), Duration::ZERO);
    }

    #[test]
    fn idle_ticks_wrap() {
        assert_eq!(idle_from_ticks(5_000, 2_000), Duration::from_secs(3));
        // The counter wrapped between the last input and now.
        assert_eq!(
            idle_from_ticks(1_000, u32::MAX - 999),
            Duration::from_secs(2)
        );
    }

    struct Stuck;
    impl Host for Stuck {
        fn on_ac_power(&self) -> Option<bool> {
            std::thread::sleep(Duration::from_millis(600));
            Some(true)
        }
        fn battery_percent(&self) -> Option<u8> {
            None
        }
        fn idle_for(&self) -> Option<Duration> {
            None
        }
        fn inhibit_sleep(&self) -> Option<SleepGuard> {
            None
        }
    }

    #[tokio::test]
    async fn a_stuck_host_answers_unknown() {
        let host: Arc<dyn Host> = Arc::new(Stuck);
        let limit = Duration::from_millis(50);
        assert_eq!(probe_within(&host, limit, |h| h.on_ac_power()).await, None);
    }
}
