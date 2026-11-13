//! Time as a dependency: tests control the clock and the time zone.

use std::sync::Mutex;

use jiff::Timestamp;
use jiff::tz::TimeZone;

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
    fn tz(&self) -> TimeZone;
}

/// The real clock and the system time zone.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }

    fn tz(&self) -> TimeZone {
        TimeZone::system()
    }
}

/// A clock you move by hand. Useful in tests of this crate and of the crates above it.
#[derive(Debug)]
pub struct ManualClock {
    state: Mutex<(Timestamp, TimeZone)>,
}

impl ManualClock {
    pub fn new(now: Timestamp, tz: TimeZone) -> Self {
        ManualClock {
            state: Mutex::new((now, tz)),
        }
    }

    /// Start at an RFC 3339 instant in a named IANA zone, for example
    /// `ManualClock::at("2026-11-13T07:00:00Z", "Europe/London")`.
    pub fn at(now: &str, tz: &str) -> Self {
        let now: Timestamp = now.parse().expect("a valid RFC 3339 instant");
        let tz = TimeZone::get(tz).expect("a known time zone");
        ManualClock::new(now, tz)
    }

    pub fn set(&self, now: Timestamp) {
        self.lock().0 = now;
    }

    pub fn set_str(&self, now: &str) {
        self.set(now.parse().expect("a valid RFC 3339 instant"));
    }

    pub fn advance_days(&self, days: i64) {
        let mut state = self.lock();
        state.0 = state
            .0
            .checked_add(jiff::SignedDuration::from_hours(24 * days))
            .expect("in range");
    }

    pub fn set_tz(&self, tz: TimeZone) {
        self.lock().1 = tz;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, (Timestamp, TimeZone)> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Timestamp {
        self.lock().0
    }

    fn tz(&self) -> TimeZone {
        self.lock().1.clone()
    }
}
