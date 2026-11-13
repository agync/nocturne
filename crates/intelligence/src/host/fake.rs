use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{Host, SleepGuard};

#[derive(Debug, Clone)]
struct State {
    ac: Option<bool>,
    percent: Option<u8>,
    idle: Option<Duration>,
    can_inhibit: bool,
}

/// A host a test steers by hand: power, charge, idle time, sleep inhibition and resumes.
#[derive(Debug)]
pub struct FakeHost {
    state: Mutex<State>,
    resumes: AtomicU64,
    inhibitors: Arc<AtomicU64>,
}

impl Default for FakeHost {
    /// On mains power, a full battery, away for an hour.
    fn default() -> Self {
        FakeHost {
            state: Mutex::new(State {
                ac: Some(true),
                percent: Some(100),
                idle: Some(Duration::from_secs(3600)),
                can_inhibit: true,
            }),
            resumes: AtomicU64::new(0),
            inhibitors: Arc::new(AtomicU64::new(0)),
        }
    }
}

struct Held(Arc<AtomicU64>);

impl Drop for Held {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl FakeHost {
    pub fn new() -> Arc<FakeHost> {
        Arc::new(FakeHost::default())
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_ac(&self, ac: Option<bool>) {
        self.state().ac = ac;
    }

    pub fn set_percent(&self, percent: Option<u8>) {
        self.state().percent = percent;
    }

    pub fn set_idle(&self, idle: Option<Duration>) {
        self.state().idle = idle;
    }

    /// Unplugged at this charge.
    pub fn on_battery(&self, percent: u8) {
        self.set_ac(Some(false));
        self.set_percent(Some(percent));
    }

    pub fn set_can_inhibit(&self, can: bool) {
        self.state().can_inhibit = can;
    }

    /// The machine woke from sleep.
    pub fn resume(&self) {
        self.resumes.fetch_add(1, Ordering::SeqCst);
    }

    /// How many sleep guards are alive now.
    pub fn inhibitors(&self) -> u64 {
        self.inhibitors.load(Ordering::SeqCst)
    }
}

impl Host for FakeHost {
    fn on_ac_power(&self) -> Option<bool> {
        self.state().ac
    }

    fn battery_percent(&self) -> Option<u8> {
        self.state().percent
    }

    fn idle_for(&self) -> Option<Duration> {
        self.state().idle
    }

    fn inhibit_sleep(&self) -> Option<SleepGuard> {
        if !self.state().can_inhibit {
            return None;
        }
        self.inhibitors.fetch_add(1, Ordering::SeqCst);
        Some(SleepGuard::new(Held(self.inhibitors.clone())))
    }

    fn resume_count(&self) -> u64 {
        self.resumes.load(Ordering::SeqCst)
    }
}
