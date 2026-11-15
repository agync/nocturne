//! Graphics the engine does not report. Its `system-info` lists AMD and NVIDIA graphics and
//! Apple's, and has no entry for Intel's, so a computer with only Intel graphics looks like one
//! with none. This finds those from the machine itself and hands them to `SystemInfo`.

use std::path::Path;
use std::sync::OnceLock;

use crate::client::Gpu;

/// The PCI vendor of Intel, as sysfs writes it.
const INTEL_PCI_VENDOR: &str = "0x8086";
/// The PCI class prefix of a display controller (VGA, 3D and other).
const DISPLAY_CLASS: &str = "0x03";
/// How Windows writes Intel's vendor in a display adapter's hardware id.
const INTEL_WINDOWS_ID: &str = "ven_8086";

fn intel() -> Vec<Gpu> {
    vec![Gpu {
        available: true,
        integrated: true,
        name: "Intel graphics".into(),
        ..Gpu::default()
    }]
}

/// True when a PCI display device under `devices` (a `/sys/bus/pci/devices` folder) is Intel's,
/// has a driver and a render node, and `render_nodes` (`/dev/dri`) holds one this process can
/// see: a card without a driver, or a container without the device, cannot run a model.
pub fn intel_under(devices: &Path, render_nodes: &Path) -> bool {
    let has_node = |dir: &Path| {
        std::fs::read_dir(dir).is_ok_and(|e| {
            e.flatten()
                .any(|f| f.file_name().to_string_lossy().starts_with("renderD"))
        })
    };
    if !has_node(render_nodes) {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(devices) else {
        return false;
    };
    let read = |dir: &Path, name: &str| {
        std::fs::read_to_string(dir.join(name))
            .map(|s| s.trim().to_ascii_lowercase())
            .unwrap_or_default()
    };
    entries.flatten().any(|e| {
        let dir = e.path();
        read(&dir, "vendor") == INTEL_PCI_VENDOR
            && read(&dir, "class").starts_with(DISPLAY_CLASS)
            && dir.join("driver").exists()
            && has_node(&dir.join("drm"))
    })
}

/// True when `listing`, the output of `pnputil` over the display adapters now connected, names an
/// Intel adapter.
pub fn intel_in_listing(listing: &str) -> bool {
    listing.to_ascii_lowercase().contains(INTEL_WINDOWS_ID)
}

#[cfg(windows)]
fn windows_listing() -> Option<String> {
    let out = crate::process::command("pnputil")
        .args(["/enum-devices", "/class", "Display", "/connected"])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(not(windows))]
fn windows_listing() -> Option<String> {
    None
}

/// The Intel graphics on this computer, empty when there are none or this system cannot say.
/// Linux reads the PCI bus and Windows the connected display adapters; a Mac has Apple's, which the
/// engine reports. Looked up once: the graphics do not change while the app runs.
pub fn intel_gpus() -> Vec<Gpu> {
    static FOUND: OnceLock<bool> = OnceLock::new();
    let found = *FOUND.get_or_init(|| {
        if cfg!(target_os = "linux") {
            intel_under(Path::new("/sys/bus/pci/devices"), Path::new("/dev/dri"))
        } else if cfg!(windows) {
            windows_listing().is_some_and(|l| intel_in_listing(&l))
        } else {
            false
        }
    });
    if found { intel() } else { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Bus {
        root: tempfile::TempDir,
    }

    impl Bus {
        fn new() -> Bus {
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(root.path().join("pci")).unwrap();
            std::fs::create_dir_all(root.path().join("dri")).unwrap();
            Bus { root }
        }

        fn pci(&self) -> std::path::PathBuf {
            self.root.path().join("pci")
        }

        fn dri(&self) -> std::path::PathBuf {
            self.root.path().join("dri")
        }

        fn found(&self) -> bool {
            intel_under(&self.pci(), &self.dri())
        }

        /// A PCI device; `driven` gives it a driver and a render node of its own.
        fn device(&self, name: &str, vendor: &str, class: &str, driven: bool) {
            let d = self.pci().join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("vendor"), format!("{vendor}\n")).unwrap();
            std::fs::write(d.join("class"), format!("{class}\n")).unwrap();
            if driven {
                std::fs::create_dir_all(d.join("driver")).unwrap();
                std::fs::create_dir_all(d.join("drm").join("renderD128")).unwrap();
            }
        }

        fn render_node(&self) {
            std::fs::write(self.dri().join("renderD128"), "").unwrap();
        }
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "the PCI device folders it reads are Linux's, and their names hold a colon Windows refuses"
    )]
    fn finds_intel_graphics_that_can_run_a_model() {
        let bus = Bus::new();
        assert!(!intel_under(&bus.pci().join("missing"), &bus.dri()));
        assert!(!bus.found());
        bus.render_node();
        assert!(!bus.found());
        bus.device("0000:00:1f.3", "0x8086", "0x040300", true);
        bus.device("0000:01:00.0", "0x10de", "0x030000", true);
        assert!(
            !bus.found(),
            "neither an audio device nor another maker's card"
        );
        bus.device("0000:00:02.0", "0x8086", "0x030000", true);
        assert!(bus.found());
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "the PCI device folders it reads are Linux's, and their names hold a colon Windows refuses"
    )]
    fn needs_a_driver_a_render_node_and_the_device_in_view() {
        let bus = Bus::new();
        bus.device("0000:00:02.0", "0x8086", "0x030000", false);
        bus.render_node();
        assert!(!bus.found(), "a card with no driver cannot run a model");
        let bus = Bus::new();
        bus.device("0000:00:02.0", "0x8086", "0x030000", true);
        assert!(
            !bus.found(),
            "a container without /dev/dri cannot reach the card"
        );
        bus.render_node();
        assert!(bus.found());
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "the PCI device folders it reads are Linux's, and their names hold a colon Windows refuses"
    )]
    fn counts_a_three_d_controller() {
        let bus = Bus::new();
        bus.render_node();
        bus.device("0000:03:00.0", "0x8086", "0x038000", true);
        assert!(bus.found());
    }

    #[test]
    fn reads_the_windows_listing() {
        let intel = "\r\nHKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}\\0000\r\n    MatchingDeviceId    REG_SZ    pci\\ven_8086&dev_a7a0&subsys_0b221028\r\n";
        let nvidia = "    MatchingDeviceId    REG_SZ    pci\\ven_10de&dev_2520\r\n";
        let both = format!("{nvidia}{intel}");
        assert!(intel_in_listing(intel));
        assert!(intel_in_listing(&both));
        assert!(intel_in_listing(&intel.to_ascii_uppercase()));
        assert!(!intel_in_listing(nvidia));
        assert!(!intel_in_listing(""));
    }
}
