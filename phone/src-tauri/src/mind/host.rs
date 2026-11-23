//! What the reader and the scheduler ask of a phone. Whether it charges is told to it from
//! outside ([`PhoneHost::set_charging`]); until then it is not known, which the reader and the
//! scheduler take as on mains. A phone tells no idle time and holds nothing awake.

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
}

impl Host for PhoneHost {
    fn on_ac_power(&self) -> Option<bool> {
        match self.charging.load(Ordering::SeqCst) {
            CHARGING => Some(true),
            ON_BATTERY => Some(false),
            _ => None,
        }
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
