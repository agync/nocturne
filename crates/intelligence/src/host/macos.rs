//! macOS: `pmset` for power, `ioreg` for idle time and `caffeinate` for sleep inhibition. The
//! system tools are run as child processes, so this needs no `unsafe` and no framework
//! bindings. Each call has a one second limit and every failure is `None`.
//!
//! Waking from sleep has no listener here: the scheduler notices it by the wall clock running
//! ahead of the monotonic one. Rust's `Instant` reads `CLOCK_UPTIME_RAW` on macOS, which stops
//! while the machine sleeps, so the two clocks drift apart by the length of the sleep. That
//! follows the documentation and is not verified by sleeping a real machine; if it were wrong,
//! the five-minute check for a missed page still catches the wake.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::SleepGuard;

const CALL_LIMIT: Duration = Duration::from_secs(1);
const PMSET: &str = "/usr/bin/pmset";
const IOREG: &str = "/usr/sbin/ioreg";
const CAFFEINATE: &str = "/usr/bin/caffeinate";

/// The standard output of a system tool, or `None` when it cannot start, fails or takes longer
/// than [`CALL_LIMIT`] (it is killed then).
fn output(program: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("nocturne-host-read".into())
        .spawn(move || {
            let mut text = String::new();
            let _ = tx.send(stdout.read_to_string(&mut text).ok().map(|_| text));
        })
        .ok()?;
    let text = rx.recv_timeout(CALL_LIMIT).ok().flatten();
    // Output at EOF means the tool is finishing; give it a moment, then end it either way.
    let grace = Instant::now() + Duration::from_millis(100);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if text.is_some() && Instant::now() < grace => {
                std::thread::sleep(Duration::from_millis(5));
            }
            _ => break None,
        }
    };
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    status.filter(|s| s.success()).and(text)
}

fn pmset() -> Option<String> {
    output(PMSET, &["-g", "batt"])
}

pub fn on_ac_power() -> Option<bool> {
    parse_ac(&pmset()?)
}

pub fn battery_percent() -> Option<u8> {
    parse_percent(&pmset()?)
}

pub fn idle_for() -> Option<Duration> {
    parse_idle(&output(IOREG, &["-r", "-c", "IOHIDSystem", "-d", "1"])?)
}

/// `caffeinate -i -w <our pid>` for as long as the guard lives: no idle sleep while it runs, and
/// it ends by itself if this process does.
pub fn inhibit_sleep() -> Option<SleepGuard> {
    Caffeinate::start(std::process::id()).map(SleepGuard::new)
}

/// A running `caffeinate`. Dropping it ends exactly that process and reaps it.
pub(super) struct Caffeinate {
    child: Child,
}

impl Caffeinate {
    pub(super) fn start(watch_pid: u32) -> Option<Caffeinate> {
        let mut child = Command::new(CAFFEINATE)
            .args(["-i", "-w", &watch_pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        // A bad argument or a missing tool ends it at once: then nothing is held.
        match child.try_wait() {
            Ok(None) => Some(Caffeinate { child }),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Caffeinate {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Whether `pmset -g batt` says the machine runs on mains power. The first line names the
/// source: `Now drawing from 'AC Power'`. A UPS counts as battery: the mains are gone.
pub(super) fn parse_ac(text: &str) -> Option<bool> {
    let first = text.lines().next()?;
    let source = first.split('\'').nth(1)?;
    match source {
        "AC Power" => Some(true),
        "Battery Power" | "UPS Power" => Some(false),
        _ => None,
    }
}

/// The charge of the internal battery from `pmset -g batt`: `71%; discharging; 3:42 remaining`.
/// No internal battery (a desktop) or one that is not present is `None`.
pub(super) fn parse_percent(text: &str) -> Option<u8> {
    let line = text
        .lines()
        .find(|l| l.contains("InternalBattery") && !l.contains("present: false"))?;
    let at = line.find('%')?;
    let digits: String = line[..at]
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse::<u8>().ok().map(|p| p.min(100))
}

/// The idle time from `ioreg -c IOHIDSystem`: `"HIDIdleTime" = 401144686273083`, nanoseconds
/// since the last keyboard, mouse or trackpad event. With several entries the smallest wins.
pub(super) fn parse_idle(text: &str) -> Option<Duration> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("\"HIDIdleTime\"")?;
            let value = rest.trim_start().strip_prefix('=')?.trim();
            value.parse::<u64>().ok().map(Duration::from_nanos)
        })
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    // `pmset_ac.txt` and `ioreg_hid.txt` are captured from a Mac mini (the ioreg one cut down to
    // the lines that matter). The Mac that was available has no battery, so the laptop and UPS
    // samples follow the documented format of `pmset -g batt` instead.
    const AC_DESKTOP: &str = include_str!("macos_samples/pmset_ac.txt");
    const ON_BATTERY: &str = include_str!("macos_samples/pmset_battery.txt");
    const CHARGING: &str = include_str!("macos_samples/pmset_charging.txt");
    const CHARGED: &str = include_str!("macos_samples/pmset_charged.txt");
    const NO_ESTIMATE: &str = include_str!("macos_samples/pmset_no_estimate.txt");
    const UPS: &str = include_str!("macos_samples/pmset_ups.txt");
    const HID: &str = include_str!("macos_samples/ioreg_hid.txt");

    #[test]
    fn a_desktop_on_mains_has_power_and_no_battery() {
        assert_eq!(parse_ac(AC_DESKTOP), Some(true));
        assert_eq!(parse_percent(AC_DESKTOP), None);
    }

    #[test]
    fn a_laptop_reports_its_source_and_charge() {
        assert_eq!(parse_ac(ON_BATTERY), Some(false));
        assert_eq!(parse_percent(ON_BATTERY), Some(71));
        assert_eq!(parse_ac(CHARGING), Some(true));
        assert_eq!(parse_percent(CHARGING), Some(96));
        assert_eq!(parse_ac(CHARGED), Some(true));
        assert_eq!(parse_percent(CHARGED), Some(100));
        assert_eq!(parse_ac(NO_ESTIMATE), Some(false));
        assert_eq!(parse_percent(NO_ESTIMATE), Some(58));
    }

    #[test]
    fn a_ups_is_battery_and_not_the_internal_battery() {
        assert_eq!(parse_ac(UPS), Some(false));
        assert_eq!(parse_percent(UPS), None);
    }

    #[test]
    fn a_battery_that_is_not_present_has_no_charge() {
        let text = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t0%; AC attached; not charging present: false\n";
        assert_eq!(parse_percent(text), None);
        assert_eq!(parse_ac(text), Some(true));
    }

    #[test]
    fn unexpected_output_is_unknown() {
        for text in ["", "\n", "pmset: command not found\n", "Now drawing from\n"] {
            assert_eq!(parse_ac(text), None, "{text:?}");
            assert_eq!(parse_percent(text), None, "{text:?}");
        }
        assert_eq!(parse_ac("Now drawing from 'Solar'\n"), None);
    }

    #[test]
    fn idle_time_is_read_in_nanoseconds() {
        assert_eq!(parse_idle(HID), Some(Duration::from_nanos(401144686273083)));
        let text = "  \"HIDIdleTime\" = 2500000000\n";
        assert_eq!(parse_idle(text), Some(Duration::from_millis(2500)));
    }

    #[test]
    fn the_smallest_idle_time_wins_and_junk_is_unknown() {
        let two = "\"HIDIdleTime\" = 9000000000\n\"HIDIdleTime\" = 3000000000\n";
        assert_eq!(parse_idle(two), Some(Duration::from_secs(3)));
        assert_eq!(parse_idle(""), None);
        assert_eq!(parse_idle("\"HIDIdleTime\" = soon\n"), None);
        assert_eq!(parse_idle("\"OtherIdleTime\" = 5\n"), None);
    }

    #[cfg(target_os = "macos")]
    fn alive(pid: u32) -> bool {
        Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "pid="])
            .output()
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false)
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_sleep_guard_holds_a_caffeinate_and_releases_it() {
        let held = Caffeinate::start(std::process::id()).expect("caffeinate starts");
        let pid = held.pid();
        assert!(alive(pid), "caffeinate runs while the guard lives");
        drop(held);
        assert!(!alive(pid), "caffeinate is gone once the guard is dropped");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_real_machine_answers_within_the_limit() {
        let started = Instant::now();
        assert!(on_ac_power().is_some());
        assert!(idle_for().is_some());
        let _ = battery_percent();
        assert!(inhibit_sleep().is_some());
        assert!(started.elapsed() < Duration::from_secs(4));
    }
}
