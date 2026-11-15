//! The NPU: whether the engine can use it on this computer, and where recordings are read.
//!
//! The engine serves an AMD XDNA 2 NPU through FastFlowLM (`flm`, backend `npu`). Recordings are
//! read there with [`SPEECH`] when the engine reports the NPU usable, the person has not chosen
//! a speech model and the busiest moment with it still fits the memory budget (decision 0006).
//! Every other job stays where it was. A failed NPU load or request is read again by the tier's
//! own speech model; the engine's reason is kept for the advanced details.

use std::path::Path;
use std::sync::Mutex;

use nocturne_contract::{ComponentRole, Fit, NpuCause, NpuView};

use crate::client::{BackendState, SystemInfo};
use crate::fit;
use crate::residency;
use crate::selection::{self, Selection};
use crate::tier::Component;

/// The engine's recipe and backend for the NPU.
pub const RECIPE: &str = "flm";
pub const BACKEND: &str = "npu";

/// The model that reads recordings on the NPU, with its download size.
pub const SPEECH: Component = Component {
    role: ComponentRole::Speech,
    model: "whisper-v3-turbo-FLM",
    bytes: 620_000_000,
};

/// The engine's NPU runtime download (FastFlowLM's portable build at the pinned engine).
pub const RUNTIME_BYTES: u64 = 41_200_000;

/// The PCI vendor and device of an XDNA 2 NPU, as the engine identifies one.
const XDNA2_PCI: (&str, &str) = ("0x1022", "0x17f0");

/// What the engine's `system-info` says about the NPU before anything is loaded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Readiness {
    /// No NPU on this computer.
    #[default]
    Absent,
    /// An NPU the engine cannot use, and why.
    Blocked {
        cause: NpuCause,
        detail: Option<String>,
    },
    /// Usable once setup has fetched the runtime.
    Fetch,
    /// Usable now.
    Ready,
}

impl Readiness {
    /// Setup may fetch the runtime and the model.
    pub fn wanted(&self) -> bool {
        matches!(self, Readiness::Fetch | Readiness::Ready)
    }
}

fn words(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// Reads the engine's view of the NPU. `pci_xdna2` says this computer has an XDNA 2 NPU on its
/// bus whether or not a driver claimed it: on Linux the engine finds an NPU only through its
/// driver, so an NPU without one would otherwise look like no NPU at all.
pub fn readiness(info: &SystemInfo, pci_xdna2: bool) -> Readiness {
    let npu = info.devices.amd_npu.clone().unwrap_or_default();
    if !npu.available {
        let detail = words(&npu.error);
        // The engine's words on Windows for an NPU with no driver.
        let no_driver = npu.error.contains("driver not installed");
        return if no_driver || pci_xdna2 {
            Readiness::Blocked {
                cause: NpuCause::DriverMissing,
                detail,
            }
        } else {
            Readiness::Absent
        };
    }
    match info.backend_state(RECIPE, BACKEND) {
        BackendState::Installed => Readiness::Ready,
        BackendState::Installable => Readiness::Fetch,
        BackendState::Unsupported | BackendState::Unknown => Readiness::Blocked {
            cause: NpuCause::NotSupported,
            detail: words(info.backend_message(RECIPE, BACKEND)),
        },
    }
}

/// The causes in an engine error about the NPU. The engine joins the sentences `flm validate`
/// gives (lemonade v2026.46.2, `run_flm_validate`), so every one found is returned; an error
/// with none of them is [`NpuCause::Failed`].
pub fn causes_of(message: &str) -> Vec<NpuCause> {
    const KNOWN: &[(&str, NpuCause)] = &[
        ("No AMD NPU device found", NpuCause::DriverMissing),
        ("NPU driver version is too old", NpuCause::DriverOld),
        ("Kernel version is incompatible", NpuCause::KernelOld),
        ("NPU firmware is incompatible", NpuCause::FirmwareOld),
        ("Memlock limits are too low", NpuCause::MemoryLock),
    ];
    let found: Vec<NpuCause> = KNOWN
        .iter()
        .filter(|(text, _)| message.contains(text))
        .map(|(_, cause)| *cause)
        .collect();
    if found.is_empty() {
        vec![NpuCause::Failed]
    } else {
        found
    }
}

/// The engine's own words about a failure, without the model name it starts with or the web
/// address it ends with.
pub fn engine_words(message: &str) -> String {
    let rest = match message.find("': ") {
        Some(at) if message.starts_with("Failed to load model '") => &message[at + 3..],
        _ => message,
    };
    let rest = rest.split("\nVisit ").next().unwrap_or(rest);
    rest.trim().to_string()
}

/// True when the busiest moment of `s` still fits the memory budget of a computer with `ram`
/// bytes, comfortably, with the NPU model reading recordings in place of `s`'s own.
pub fn fits(s: &Selection, ram: u64) -> bool {
    let mut picks = selection::picks(s);
    if picks.speech.is_empty() {
        return false;
    }
    picks.speech = SPEECH.model.to_string();
    let budget = fit::budget(ram, 0, u64::MAX);
    let alone_voice = !residency::alone_for(s, &budget).is_empty();
    fit::fit_for(fit::peak(&picks, alone_voice), &budget) == Fit::Fits
}

/// True when a PCI device under `devices` (a `/sys/bus/pci/devices` folder) is an XDNA 2 NPU.
pub fn pci_xdna2_under(devices: &Path) -> bool {
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
        read(&dir, "vendor") == XDNA2_PCI.0 && read(&dir, "device") == XDNA2_PCI.1
    })
}

/// True when this computer has an XDNA 2 NPU on its PCI bus. Linux only; elsewhere the engine
/// says so itself.
pub fn pci_xdna2() -> bool {
    cfg!(target_os = "linux") && pci_xdna2_under(Path::new("/sys/bus/pci/devices"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Failure {
    /// The engine run it happened in; the next run tries the NPU again.
    generation: u64,
    causes: Vec<NpuCause>,
    detail: Option<String>,
}

#[derive(Debug, Default)]
struct State {
    readiness: Readiness,
    fits: bool,
    chosen: bool,
    failure: Option<Failure>,
}

impl State {
    fn eligible(&self) -> bool {
        self.readiness == Readiness::Ready && self.fits && !self.chosen
    }
}

/// Where recordings are read, shared by the mind (which reads them) and the advanced details.
#[derive(Debug, Default)]
pub struct Npu {
    state: Mutex<State>,
}

impl Npu {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// What the engine reported, whether the NPU model fits, and whether the person chose a
    /// speech model of their own.
    pub fn set(&self, readiness: Readiness, fits: bool, chosen: bool) {
        let mut s = self.state();
        s.readiness = readiness;
        s.fits = fits;
        s.chosen = chosen;
    }

    /// The model to read recordings with on the NPU in engine run `generation`, if any.
    pub fn speech_model(&self, generation: u64) -> Option<&'static str> {
        let s = self.state();
        let failed_now = s
            .failure
            .as_ref()
            .is_some_and(|f| f.generation == generation);
        (s.eligible() && !failed_now).then_some(SPEECH.model)
    }

    /// The NPU read a recording.
    pub fn worked(&self) {
        self.state().failure = None;
    }

    /// The NPU failed in engine run `generation` with the engine's `message`, and the tier's own
    /// model read the recording instead.
    pub fn failed(&self, generation: u64, message: &str) {
        let detail = words(&engine_words(message));
        tracing::warn!(
            "the NPU could not read a recording; reading on the GPU: {}",
            detail.as_deref().unwrap_or("no reason given")
        );
        self.state().failure = Some(Failure {
            generation,
            causes: causes_of(message),
            detail,
        });
    }

    /// The NPU model is not on this computer yet.
    pub fn not_fetched(&self, generation: u64) {
        self.state().failure = Some(Failure {
            generation,
            causes: vec![NpuCause::NotFetched],
            detail: None,
        });
    }

    pub fn view(&self) -> NpuView {
        let s = self.state();
        let present = s.readiness != Readiness::Absent;
        let used = s.eligible() && s.failure.is_none();
        let mut causes = Vec::new();
        let mut detail = None;
        match &s.readiness {
            Readiness::Absent | Readiness::Ready => {}
            Readiness::Fetch => causes.push(NpuCause::NotFetched),
            Readiness::Blocked { cause, detail: d } => {
                causes.push(*cause);
                detail = d.clone();
            }
        }
        if present && !matches!(s.readiness, Readiness::Blocked { .. }) {
            if !s.fits {
                causes.push(NpuCause::TooLittleMemory);
            }
            if s.chosen {
                causes.push(NpuCause::Chosen);
            }
            if s.eligible()
                && let Some(f) = &s.failure
            {
                causes.extend(f.causes.iter().copied());
                detail = f.detail.clone();
            }
        }
        NpuView {
            present,
            used,
            model: used.then(|| SPEECH.model.to_string()),
            causes,
            detail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocturne_contract::Tier;

    fn info(json: &str) -> SystemInfo {
        serde_json::from_str(json).unwrap()
    }

    fn with_flm(state: &str, message: &str) -> SystemInfo {
        info(&format!(
            r#"{{"devices": {{"amd_npu": {{"available": true, "family": "XDNA2"}}}},
                "recipes": {{"flm": {{"backends": {{"npu": {{"state": "{state}", "message": "{message}"}}}}}}}}}}"#
        ))
    }

    #[test]
    fn reads_the_engines_view_of_the_npu() {
        assert_eq!(
            readiness(&with_flm("installed", ""), false),
            Readiness::Ready
        );
        assert_eq!(
            readiness(&with_flm("installable", ""), false),
            Readiness::Fetch
        );
        assert_eq!(
            readiness(&with_flm("update_required", ""), false),
            Readiness::Fetch
        );
        assert_eq!(
            readiness(&with_flm("unsupported", "Requires AMD XDNA 2 NPU"), false),
            Readiness::Blocked {
                cause: NpuCause::NotSupported,
                detail: Some("Requires AMD XDNA 2 NPU".into())
            }
        );
        assert!(!readiness(&with_flm("unsupported", ""), false).wanted());
        assert!(readiness(&with_flm("installable", ""), false).wanted());
    }

    #[test]
    fn no_npu_is_absent_and_a_missing_driver_is_said() {
        assert_eq!(readiness(&SystemInfo::default(), false), Readiness::Absent);
        let linux = info(
            r#"{"devices": {"amd_npu": {"available": false, "error": "No NPU device found"}}}"#,
        );
        assert_eq!(readiness(&linux, false), Readiness::Absent);
        assert_eq!(
            readiness(&linux, true),
            Readiness::Blocked {
                cause: NpuCause::DriverMissing,
                detail: Some("No NPU device found".into())
            },
            "the NPU is on the bus but no driver claimed it"
        );
        let windows = info(
            r#"{"devices": {"amd_npu": {"available": false, "error": "NPU hardware found but driver not installed"}}}"#,
        );
        assert!(matches!(
            readiness(&windows, false),
            Readiness::Blocked {
                cause: NpuCause::DriverMissing,
                ..
            }
        ));
        let mac = info(
            r#"{"devices": {"amd_npu": {"available": false, "error": "NPU not supported on macOS"}}}"#,
        );
        assert_eq!(readiness(&mac, false), Readiness::Absent);
    }

    #[test]
    fn every_reason_the_engine_gives_has_a_cause() {
        let msg = |reasons: &str| {
            format!(
                "Failed to load model 'whisper-v3-turbo-FLM': FLM NPU validation failed: {reasons}\nVisit https://lemonade-server.ai/driver_install.html for driver installation instructions."
            )
        };
        let cases = [
            ("Memlock limits are too low.", NpuCause::MemoryLock),
            ("NPU firmware is incompatible.", NpuCause::FirmwareOld),
            ("Kernel version is incompatible.", NpuCause::KernelOld),
            ("NPU driver version is too old.", NpuCause::DriverOld),
            ("No AMD NPU device found.", NpuCause::DriverMissing),
        ];
        for (reason, cause) in cases {
            assert_eq!(causes_of(&msg(reason)), vec![cause], "{reason}");
        }
        assert_eq!(
            causes_of(&msg(
                "NPU firmware is incompatible. Kernel version is incompatible. Memlock limits are too low."
            )),
            vec![
                NpuCause::KernelOld,
                NpuCause::FirmwareOld,
                NpuCause::MemoryLock
            ]
        );
        assert_eq!(
            causes_of(&msg("NPU validation failed.")),
            vec![NpuCause::Failed]
        );
        assert_eq!(causes_of("flm serve exited"), vec![NpuCause::Failed]);
        assert_eq!(
            engine_words(&msg("Memlock limits are too low.")),
            "FLM NPU validation failed: Memlock limits are too low."
        );
        assert_eq!(
            engine_words("backend did not start"),
            "backend did not start"
        );
    }

    #[test]
    fn the_npu_model_fits_from_small_up_and_never_on_eight_gigabytes() {
        const GIB: u64 = 1 << 30;
        let fits_at = |tier, gib: u64| fits(&Selection::preset(tier), gib * GIB);
        assert!(
            !fits_at(Tier::Tiny, 8),
            "an 8 GB computer keeps its own speech model"
        );
        assert!(!fits_at(Tier::Tiny, 6));
        assert!(!fits_at(Tier::Small, 14));
        assert!(fits_at(Tier::Small, 15));
        assert!(fits_at(Tier::Small, 16));
        assert!(fits_at(Tier::Standard, 28));
        assert!(fits_at(Tier::Large, 60));
        assert!(!fits(&Selection::preset(Tier::Unsupported), 128 * GIB));
    }

    #[test]
    fn finds_an_npu_on_the_bus_without_a_driver() {
        let root = tempfile::tempdir().unwrap();
        let dev = |name: &str, vendor: &str, device: &str| {
            let d = root.path().join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("vendor"), format!("{vendor}\n")).unwrap();
            std::fs::write(d.join("device"), format!("{device}\n")).unwrap();
        };
        dev("0000:c6:00.0", "0x1002", "0x150e");
        assert!(!pci_xdna2_under(root.path()));
        dev("0000:c7:00.1", "0x1022", "0x17F0");
        assert!(pci_xdna2_under(root.path()));
        assert!(!pci_xdna2_under(&root.path().join("missing")));
    }

    #[test]
    fn recordings_go_to_the_npu_only_when_everything_allows_it() {
        let npu = Npu::default();
        assert_eq!(npu.speech_model(1), None);
        assert_eq!(npu.view(), NpuView::default());

        npu.set(Readiness::Ready, true, false);
        assert_eq!(npu.speech_model(1), Some(SPEECH.model));
        let v = npu.view();
        assert!(v.present && v.used && v.causes.is_empty());
        assert_eq!(v.model.as_deref(), Some(SPEECH.model));

        npu.set(Readiness::Ready, false, false);
        assert_eq!(npu.speech_model(1), None);
        assert_eq!(npu.view().causes, vec![NpuCause::TooLittleMemory]);

        npu.set(Readiness::Ready, true, true);
        assert_eq!(npu.speech_model(1), None);
        assert_eq!(npu.view().causes, vec![NpuCause::Chosen]);

        npu.set(Readiness::Fetch, true, false);
        assert_eq!(npu.speech_model(1), None);
        assert_eq!(npu.view().causes, vec![NpuCause::NotFetched]);

        npu.set(
            Readiness::Blocked {
                cause: NpuCause::DriverMissing,
                detail: Some("No NPU device found".into()),
            },
            false,
            false,
        );
        let v = npu.view();
        assert!(v.present && !v.used);
        assert_eq!(
            v.causes,
            vec![NpuCause::DriverMissing],
            "the fit is beside the point"
        );
        assert_eq!(v.detail.as_deref(), Some("No NPU device found"));
    }

    #[test]
    fn a_failure_falls_back_for_the_rest_of_the_engines_run() {
        let npu = Npu::default();
        npu.set(Readiness::Ready, true, false);
        npu.failed(
            3,
            "Failed to load model 'whisper-v3-turbo-FLM': FLM NPU validation failed: Memlock limits are too low.\nVisit https://lemonade-server.ai/driver_install.html for driver installation instructions.",
        );
        assert_eq!(npu.speech_model(3), None);
        let v = npu.view();
        assert!(!v.used);
        assert_eq!(v.causes, vec![NpuCause::MemoryLock]);
        assert_eq!(
            v.detail.as_deref(),
            Some("FLM NPU validation failed: Memlock limits are too low.")
        );
        assert_eq!(
            npu.speech_model(4),
            Some(SPEECH.model),
            "a new run tries again"
        );
        npu.worked();
        assert!(npu.view().used);
        npu.not_fetched(4);
        assert_eq!(npu.view().causes, vec![NpuCause::NotFetched]);
    }
}
