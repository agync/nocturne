//! What the reader and the scheduler ask of a phone. Whether it charges, its battery level and
//! whether battery saving is on are told to it from outside ([`PhoneHost::set_power`]). Until it
//! is told, or when the phone cannot say, it is taken as on battery: nothing that waits for the
//! charger runs on a guess. A phone tells no idle time and holds nothing awake.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Duration;

use nocturne_intelligence::{Host, SleepGuard};

const UNKNOWN: u8 = 0;
const CHARGING: u8 = 1;
const ON_BATTERY: u8 = 2;

/// No battery level was told.
const NO_LEVEL: u8 = u8::MAX;

#[derive(Debug)]
pub struct PhoneHost {
    charging: AtomicU8,
    percent: AtomicU8,
    saver: AtomicBool,
}

impl Default for PhoneHost {
    fn default() -> Self {
        PhoneHost {
            charging: AtomicU8::new(UNKNOWN),
            percent: AtomicU8::new(NO_LEVEL),
            saver: AtomicBool::new(false),
        }
    }
}

impl PhoneHost {
    pub fn set_charging(&self, charging: Option<bool>) {
        let v = match charging {
            None => UNKNOWN,
            Some(true) => CHARGING,
            Some(false) => ON_BATTERY,
        };
        self.charging.store(v, Ordering::SeqCst);
    }

    /// Whether it charges, its battery level in per cent and whether battery saving is on, each
    /// `None` when the phone does not say. A level the phone cannot have is taken as unknown.
    pub fn set_power(&self, charging: Option<bool>, percent: Option<u8>, saver: Option<bool>) {
        self.set_charging(charging);
        let level = percent.filter(|p| *p <= 100).unwrap_or(NO_LEVEL);
        self.percent.store(level, Ordering::SeqCst);
        self.saver.store(saver.unwrap_or(false), Ordering::SeqCst);
    }

    /// The battery level, when the phone said.
    pub fn percent(&self) -> Option<u8> {
        Some(self.percent.load(Ordering::SeqCst)).filter(|p| *p != NO_LEVEL)
    }

    /// Whether battery saving is on, as the phone said.
    pub fn saver(&self) -> bool {
        self.saver.load(Ordering::SeqCst)
    }

    /// Whether the phone is known to charge.
    pub fn charging(&self) -> bool {
        self.charging.load(Ordering::SeqCst) == CHARGING
    }
}

impl Host for PhoneHost {
    fn on_ac_power(&self) -> Option<bool> {
        Some(self.charging())
    }

    fn battery_percent(&self) -> Option<u8> {
        self.percent()
    }

    fn idle_for(&self) -> Option<Duration> {
        None
    }

    fn inhibit_sleep(&self) -> Option<SleepGuard> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_phone_known_to_charge_counts_as_plugged_in() {
        let host = PhoneHost::default();
        assert_eq!(host.on_ac_power(), Some(false));
        host.set_charging(Some(true));
        assert_eq!(host.on_ac_power(), Some(true));
        host.set_charging(None);
        assert_eq!(host.on_ac_power(), Some(false));
        host.set_charging(Some(false));
        assert_eq!(host.on_ac_power(), Some(false));
        assert!(!host.charging());
    }

    #[test]
    fn the_battery_level_and_saving_are_as_told() {
        let host = PhoneHost::default();
        assert_eq!(host.battery_percent(), None);
        assert!(!host.saver());
        host.set_power(Some(false), Some(42), Some(true));
        assert_eq!(host.battery_percent(), Some(42));
        assert!(host.saver());
        assert!(!host.charging());
        host.set_power(Some(true), Some(250), None);
        assert_eq!(host.battery_percent(), None);
        assert!(!host.saver());
        assert!(host.charging());
    }
}
