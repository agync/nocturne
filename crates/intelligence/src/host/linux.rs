//! Linux: UPower and `/sys/class/power_supply` for power, Mutter, the freedesktop ScreenSaver
//! and logind for idle time, logind for sleep inhibition and resume. All through zbus' blocking
//! API with a one second limit per call; every failure is `None`.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;
use zbus::zvariant::{OwnedFd, OwnedValue};

use super::{SleepGuard, idle_from_hint};

const CALL_LIMIT: Duration = Duration::from_secs(1);
const POWER_SUPPLY: &str = "/sys/class/power_supply";

fn system_bus() -> Option<Connection> {
    Builder::system()
        .ok()?
        .method_timeout(CALL_LIMIT)
        .build()
        .ok()
}

fn session_bus() -> Option<Connection> {
    Builder::session()
        .ok()?
        .method_timeout(CALL_LIMIT)
        .build()
        .ok()
}

/// `org.freedesktop.DBus.Properties.Get`, decoded to `T`.
fn property<T: TryFrom<OwnedValue>>(
    conn: &Connection,
    dest: &str,
    path: &str,
    iface: &str,
    name: &str,
) -> Option<T> {
    let reply = conn
        .call_method(
            Some(dest),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(iface, name),
        )
        .ok()?;
    let value: OwnedValue = reply.body().deserialize().ok()?;
    T::try_from(value).ok()
}

const UPOWER: &str = "org.freedesktop.UPower";

pub fn on_ac_power() -> Option<bool> {
    if let Some(conn) = system_bus()
        && let Some(on_battery) = property::<bool>(
            &conn,
            UPOWER,
            "/org/freedesktop/UPower",
            UPOWER,
            "OnBattery",
        )
    {
        return Some(!on_battery);
    }
    sysfs_ac(Path::new(POWER_SUPPLY))
}

pub fn battery_percent() -> Option<u8> {
    if let Some(conn) = system_bus() {
        let display = "/org/freedesktop/UPower/devices/DisplayDevice";
        let device = "org.freedesktop.UPower.Device";
        match property::<bool>(&conn, UPOWER, display, device, "IsPresent") {
            Some(false) => return None,
            Some(true) => {
                if let Some(p) = property::<f64>(&conn, UPOWER, display, device, "Percentage") {
                    return Some(p.round().clamp(0.0, 100.0) as u8);
                }
            }
            None => {}
        }
    }
    sysfs_percent(Path::new(POWER_SUPPLY))
}

pub fn idle_for() -> Option<Duration> {
    if let Some(conn) = session_bus() {
        // GNOME.
        if let Ok(reply) = conn.call_method(
            Some("org.gnome.Mutter.IdleMonitor"),
            "/org/gnome/Mutter/IdleMonitor/Core",
            Some("org.gnome.Mutter.IdleMonitor"),
            "GetIdletime",
            &(),
        ) && let Ok(ms) = reply.body().deserialize::<u64>()
        {
            return Some(Duration::from_millis(ms));
        }
        // KDE and others: the session idle time, in milliseconds.
        if let Ok(reply) = conn.call_method(
            Some("org.freedesktop.ScreenSaver"),
            "/org/freedesktop/ScreenSaver",
            Some("org.freedesktop.ScreenSaver"),
            "GetSessionIdleTime",
            &(),
        ) && let Ok(ms) = reply.body().deserialize::<u32>()
        {
            return Some(Duration::from_millis(u64::from(ms)));
        }
    }
    // logind's idle hint for this session.
    let conn = system_bus()?;
    let dest = "org.freedesktop.login1";
    let path = "/org/freedesktop/login1/session/auto";
    let iface = "org.freedesktop.login1.Session";
    let hint = property::<bool>(&conn, dest, path, iface, "IdleHint")?;
    let since = property::<u64>(&conn, dest, path, iface, "IdleSinceHint").unwrap_or(0);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    Some(idle_from_hint(hint, since, now.as_micros() as u64))
}

/// logind `Inhibit("sleep", ..., "block")`: the returned descriptor holds the lock until closed.
pub fn inhibit_sleep() -> Option<SleepGuard> {
    let conn = system_bus()?;
    let reply = conn
        .call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "Inhibit",
            &("sleep", "Nocturne", "Writing this morning's page", "block"),
        )
        .ok()?;
    let fd: OwnedFd = reply.body().deserialize().ok()?;
    Some(SleepGuard::new(fd))
}

/// Counts `PrepareForSleep(false)` from logind: the machine woke up.
#[derive(Debug, Default)]
pub struct ResumeWatch {
    count: Arc<AtomicU64>,
    started: AtomicBool,
}

impl ResumeWatch {
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::SeqCst)
    }

    /// Start the listener once. It ends when the bus goes away or the process exits.
    pub fn start(&self) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let count = self.count.clone();
        let _ = std::thread::Builder::new()
            .name("nocturne-resume-watch".into())
            .spawn(move || {
                let Ok(conn) = Connection::system() else {
                    return;
                };
                let Ok(proxy) = zbus::blocking::Proxy::new(
                    &conn,
                    "org.freedesktop.login1",
                    "/org/freedesktop/login1",
                    "org.freedesktop.login1.Manager",
                ) else {
                    return;
                };
                let Ok(signals) = proxy.receive_signal("PrepareForSleep") else {
                    return;
                };
                for msg in signals {
                    if let Ok(going_to_sleep) = msg.body().deserialize::<bool>()
                        && !going_to_sleep
                    {
                        count.fetch_add(1, Ordering::SeqCst);
                    }
                }
            });
    }
}

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// What `/sys/class/power_supply` under `root` says about mains power.
pub(super) fn sysfs_ac(root: &Path) -> Option<bool> {
    let mut mains = Vec::new();
    let mut batteries = Vec::new();
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let dir = entry.path();
        // Peripherals (a mouse, a headset) report as batteries with scope "Device".
        if read(&dir.join("scope")).as_deref() == Some("Device") {
            continue;
        }
        match read(&dir.join("type")).as_deref() {
            Some("Mains") | Some("USB") | Some("USB_C") | Some("USB_PD") => {
                if let Some(online) = read(&dir.join("online")) {
                    mains.push(online == "1");
                }
            }
            Some("Battery") => batteries.push(read(&dir.join("status")).unwrap_or_default()),
            _ => {}
        }
    }
    if mains.iter().any(|online| *online) {
        return Some(true);
    }
    if !mains.is_empty() {
        return Some(false);
    }
    if batteries.is_empty() {
        // No battery and no adapter: a desktop.
        return Some(true);
    }
    // A battery and no adapter to ask: its own status tells.
    if batteries.iter().any(|s| s == "Discharging") {
        Some(false)
    } else if batteries.iter().any(|s| s == "Charging" || s == "Full") {
        Some(true)
    } else {
        None
    }
}

/// The charge of the first system battery under `root`.
pub(super) fn sysfs_percent(root: &Path) -> Option<u8> {
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let dir = entry.path();
        if read(&dir.join("scope")).as_deref() == Some("Device") {
            continue;
        }
        if read(&dir.join("type")).as_deref() == Some("Battery")
            && let Some(p) = read(&dir.join("capacity")).and_then(|c| c.parse::<u8>().ok())
        {
            return Some(p.min(100));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supply(root: &Path, name: &str, files: &[(&str, &str)]) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for (f, v) in files {
            std::fs::write(dir.join(f), format!("{v}\n")).unwrap();
        }
    }

    #[test]
    fn mains_online_means_ac() {
        let d = tempfile::tempdir().unwrap();
        supply(d.path(), "AC", &[("type", "Mains"), ("online", "1")]);
        supply(
            d.path(),
            "BAT0",
            &[
                ("type", "Battery"),
                ("status", "Charging"),
                ("capacity", "71"),
            ],
        );
        assert_eq!(sysfs_ac(d.path()), Some(true));
        assert_eq!(sysfs_percent(d.path()), Some(71));
    }

    #[test]
    fn mains_offline_means_battery() {
        let d = tempfile::tempdir().unwrap();
        supply(d.path(), "AC", &[("type", "Mains"), ("online", "0")]);
        supply(
            d.path(),
            "BAT0",
            &[
                ("type", "Battery"),
                ("status", "Discharging"),
                ("capacity", "38"),
            ],
        );
        assert_eq!(sysfs_ac(d.path()), Some(false));
    }

    #[test]
    fn no_battery_means_ac_and_a_peripheral_is_not_a_battery() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(sysfs_ac(d.path()), Some(true));
        assert_eq!(sysfs_percent(d.path()), None);
        supply(
            d.path(),
            "hidpp_battery_0",
            &[
                ("type", "Battery"),
                ("scope", "Device"),
                ("status", "Discharging"),
                ("capacity", "12"),
            ],
        );
        assert_eq!(sysfs_ac(d.path()), Some(true));
        assert_eq!(sysfs_percent(d.path()), None);
    }

    #[test]
    fn a_battery_alone_tells_by_its_status() {
        let d = tempfile::tempdir().unwrap();
        supply(
            d.path(),
            "BAT0",
            &[("type", "Battery"), ("status", "Discharging")],
        );
        assert_eq!(sysfs_ac(d.path()), Some(false));
        supply(
            d.path(),
            "BAT0",
            &[("type", "Battery"), ("status", "Unknown")],
        );
        assert_eq!(sysfs_ac(d.path()), None);
    }

    #[test]
    fn a_missing_directory_is_unknown() {
        assert_eq!(sysfs_ac(Path::new("/nonexistent/power_supply")), None);
    }
}
