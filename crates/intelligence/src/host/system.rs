use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::{CACHE_TTL, Host, SleepGuard};

#[derive(Debug)]
struct Slot<T> {
    read_at: Option<Instant>,
    value: Option<T>,
}

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Slot {
            read_at: None,
            value: None,
        }
    }
}

impl<T: Copy> Slot<T> {
    fn get(&mut self, read: impl FnOnce() -> Option<T>) -> Option<T> {
        if let Some(at) = self.read_at
            && at.elapsed() < CACHE_TTL
        {
            return self.value;
        }
        self.value = read();
        self.read_at = Some(Instant::now());
        self.value
    }
}

#[derive(Debug, Default)]
struct Cache {
    ac: Slot<bool>,
    percent: Slot<u8>,
    idle: Slot<Duration>,
}

/// The real machine. Every failure is `None`; reads are cached for 30 seconds.
#[derive(Debug, Default)]
pub struct SystemHost {
    cache: Mutex<Cache>,
    #[cfg(target_os = "linux")]
    resume: super::linux::ResumeWatch,
}

impl SystemHost {
    pub fn new() -> Self {
        SystemHost::default()
    }

    /// Start listening for the machine waking from sleep (a background thread on Linux).
    /// Without it `resume_count` stays 0 and the scheduler relies on its clock check.
    pub fn watch_resume(&self) {
        #[cfg(target_os = "linux")]
        self.resume.start();
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, Cache> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Host for SystemHost {
    fn on_ac_power(&self) -> Option<bool> {
        self.cache().ac.get(platform::on_ac_power)
    }

    fn battery_percent(&self) -> Option<u8> {
        self.cache().percent.get(platform::battery_percent)
    }

    fn idle_for(&self) -> Option<Duration> {
        self.cache().idle.get(platform::idle_for)
    }

    fn inhibit_sleep(&self) -> Option<SleepGuard> {
        platform::inhibit_sleep()
    }

    fn resume_count(&self) -> u64 {
        #[cfg(target_os = "linux")]
        return self.resume.count();
        #[cfg(not(target_os = "linux"))]
        0
    }
}

#[cfg(target_os = "linux")]
use super::linux as platform;
#[cfg(windows)]
use super::windows as platform;

/// Platforms with no support report nothing.
#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    use std::time::Duration;

    use super::SleepGuard;

    pub fn on_ac_power() -> Option<bool> {
        None
    }
    pub fn battery_percent() -> Option<u8> {
        None
    }
    pub fn idle_for() -> Option<Duration> {
        None
    }
    pub fn inhibit_sleep() -> Option<SleepGuard> {
        None
    }
}
