//! Windows: `GetSystemPowerStatus`, `GetLastInputInfo` and `SetThreadExecutionState`. The only
//! place in this crate that uses `unsafe`; every failure is `None`.
#![allow(unsafe_code)]

use std::sync::mpsc;
use std::time::Duration;

use windows::Win32::System::Power::{
    ES_CONTINUOUS, ES_SYSTEM_REQUIRED, GetSystemPowerStatus, SYSTEM_POWER_STATUS,
    SetThreadExecutionState,
};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

use super::{SleepGuard, idle_from_ticks};

fn status() -> Option<SYSTEM_POWER_STATUS> {
    let mut s = SYSTEM_POWER_STATUS::default();
    // SAFETY: `s` is a valid, writable SYSTEM_POWER_STATUS for the duration of the call.
    unsafe { GetSystemPowerStatus(&mut s).ok()? };
    Some(s)
}

pub fn on_ac_power() -> Option<bool> {
    match status()?.ACLineStatus {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

pub fn battery_percent() -> Option<u8> {
    let s = status()?;
    // 128 is "no system battery"; 255 is unknown.
    if s.BatteryFlag & 128 != 0 || s.BatteryLifePercent > 100 {
        None
    } else {
        Some(s.BatteryLifePercent)
    }
}

pub fn idle_for() -> Option<Duration> {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: `info` is a valid LASTINPUTINFO with `cbSize` set, as the call requires.
    if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        return None;
    }
    // SAFETY: no arguments, no memory is touched.
    let now = unsafe { GetTickCount() };
    Some(idle_from_ticks(now, info.dwTime))
}

/// The execution state belongs to the thread that set it, so a thread of its own holds it and
/// clears it when the guard is dropped.
pub fn inhibit_sleep() -> Option<SleepGuard> {
    let (release, wait) = mpsc::channel::<()>();
    let (ready, started) = mpsc::channel::<bool>();
    std::thread::Builder::new()
        .name("nocturne-keep-awake".into())
        .spawn(move || {
            // SAFETY: a plain flags call on this thread.
            let _ = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
            let _ = ready.send(true);
            let _ = wait.recv();
            // SAFETY: as above; ES_CONTINUOUS alone clears the requirement.
            let _ = unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        })
        .ok()?;
    if started.recv_timeout(Duration::from_secs(1)).ok()? {
        Some(SleepGuard::new(release))
    } else {
        None
    }
}
