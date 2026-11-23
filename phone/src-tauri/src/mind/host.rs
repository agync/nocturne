//! What the reader and the scheduler ask of a phone. Whether it charges is told to it from
//! outside ([`PhoneHost::set_charging`]). Until it is told, or when the phone cannot say, it is
//! taken as on battery: nothing that waits for the charger runs on a guess. A phone tells no
//! idle time and holds nothing awake.

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use nocturne_intelligence::{Host, SleepGuard};

const UNKNOWN: u8 = 0;
const CHARGING: u8 = 1;
const ON_BATTERY: u8 = 2;

#[derive(Debug, Default)]
pub struct PhoneHost {
    charging: AtomicU8,
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
        None
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
}
