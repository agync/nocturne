//! Which models a machine can carry, and which backend suits its hardware.

use nocturne_contract::{ComponentRole, Tier, TierOption};

use crate::client::{BackendState, SystemInfo};

const GIB: u64 = 1 << 30;
/// Free space to leave over and above a download.
pub const DISK_HEADROOM: u64 = 2 * GIB;

/// One model a tier needs. `bytes` is its download size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Component {
    pub role: ComponentRole,
    pub model: &'static str,
    pub bytes: u64,
}

const fn c(role: ComponentRole, model: &'static str, bytes: u64) -> Component {
    Component { role, model, bytes }
}

const VOICE: Component = c(ComponentRole::Voice, "kokoro-v1", 354_000_000);
const RECALL: Component = c(
    ComponentRole::Recall,
    "Qwen3-Embedding-0.6B-GGUF",
    640_000_000,
);

pub fn components(tier: Tier) -> Vec<Component> {
    match tier {
        Tier::Large => vec![
            c(
                ComponentRole::Mind,
                "Qwen3.6-35B-A3B-MTP-GGUF",
                23_800_000_000,
            ),
            c(
                ComponentRole::Speech,
                "Whisper-Large-v3-Turbo",
                1_620_000_000,
            ),
            VOICE,
            RECALL,
        ],
        Tier::Standard => vec![
            c(ComponentRole::Mind, "Qwen3.5-9B-GGUF", 6_880_000_000),
            c(ComponentRole::Speech, "Whisper-Small", 488_000_000),
            VOICE,
            RECALL,
        ],
        Tier::Small => vec![
            c(ComponentRole::Mind, "Qwen3.5-4B-MTP-GGUF", 3_660_000_000),
            c(ComponentRole::Speech, "Whisper-Base", 148_000_000),
            VOICE,
            RECALL,
        ],
        Tier::Unsupported => Vec::new(),
    }
}

/// The model that plays `role` in `tier`.
pub fn model_for(tier: Tier, role: ComponentRole) -> Option<&'static str> {
    components(tier)
        .into_iter()
        .find(|c| c.role == role)
        .map(|c| c.model)
}

pub fn download_bytes(tier: Tier) -> u64 {
    components(tier).iter().map(|c| c.bytes).sum()
}

/// What `tier` still needs fetched when everything `held` needs is already on disk: models the
/// two share (the voice and the recall model) are not counted again.
pub fn missing_bytes(tier: Tier, held: Tier) -> u64 {
    let have = components(held);
    components(tier)
        .iter()
        .filter(|c| !have.iter().any(|h| h.model == c.model))
        .map(|c| c.bytes)
        .sum()
}

pub fn min_ram_gib(tier: Tier) -> u32 {
    match tier {
        Tier::Large => 60,
        Tier::Standard => 28,
        Tier::Small => 14,
        Tier::Unsupported => 0,
    }
}

const ORDER: [Tier; 3] = [Tier::Large, Tier::Standard, Tier::Small];

fn ram_ok(tier: Tier, ram_bytes: u64) -> bool {
    ram_bytes >= u64::from(min_ram_gib(tier)) * GIB
}

fn disk_ok(tier: Tier, free_disk_bytes: u64) -> bool {
    free_disk_bytes >= download_bytes(tier) + DISK_HEADROOM
}

/// The biggest tier the memory and the free disk both allow.
pub fn detect(ram_bytes: u64, free_disk_bytes: u64) -> Tier {
    ORDER
        .into_iter()
        .find(|t| ram_ok(*t, ram_bytes) && disk_ok(*t, free_disk_bytes))
        .unwrap_or(Tier::Unsupported)
}

pub fn tier_options(ram_bytes: u64, free_disk_bytes: u64) -> Vec<TierOption> {
    ORDER
        .into_iter()
        .map(|tier| TierOption {
            tier,
            download_bytes: download_bytes(tier) as f64,
            min_ram_gib: min_ram_gib(tier),
            fits: ram_ok(tier, ram_bytes) && disk_ok(tier, free_disk_bytes),
        })
        .collect()
}

/// The backend to run `recipe` on: Vulkan on an integrated GPU (the dependable path there),
/// ROCm on a discrete AMD GPU, otherwise the CPU. Falls back to the CPU when the preferred
/// backend is not available on this machine.
pub fn choose_backend(info: &SystemInfo, recipe: &str) -> &'static str {
    let usable = |backend: &str| {
        matches!(
            info.backend_state(recipe, backend),
            BackendState::Installable | BackendState::Installed
        )
    };
    let gpus: Vec<_> = info
        .devices
        .amd_gpu
        .iter()
        .filter(|g| g.available)
        .collect();
    let preferred = if gpus.iter().any(|g| !g.integrated) {
        "rocm"
    } else if !gpus.is_empty() {
        "vulkan"
    } else {
        "cpu"
    };
    if usable(preferred) { preferred } else { "cpu" }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn gib(n: u64) -> u64 {
        n * GIB
    }

    const PLENTY: u64 = 10_000 * GIB;

    #[test]
    fn a_smaller_tier_needs_only_what_the_bigger_one_lacks() {
        assert_eq!(
            missing_bytes(Tier::Small, Tier::Standard),
            3_660_000_000 + 148_000_000
        );
        assert_eq!(missing_bytes(Tier::Small, Tier::Small), 0);
        assert_eq!(
            missing_bytes(Tier::Standard, Tier::Small),
            6_880_000_000 + 488_000_000
        );
        assert_eq!(
            missing_bytes(Tier::Small, Tier::Unsupported),
            download_bytes(Tier::Small)
        );
    }

    #[test]
    fn detects_by_memory() {
        assert_eq!(detect(gib(8), PLENTY), Tier::Unsupported);
        assert_eq!(detect(gib(16), PLENTY), Tier::Small);
        assert_eq!(detect(gib(32), PLENTY), Tier::Standard);
        assert_eq!(detect(gib(64), PLENTY), Tier::Large);
        assert_eq!(detect(gib(124), PLENTY), Tier::Large);
    }

    #[test]
    fn the_thresholds_are_inclusive() {
        assert_eq!(detect(gib(14), PLENTY), Tier::Small);
        assert_eq!(detect(gib(14) - 1, PLENTY), Tier::Unsupported);
        assert_eq!(detect(gib(28), PLENTY), Tier::Standard);
        assert_eq!(detect(gib(60), PLENTY), Tier::Large);
    }

    #[test]
    fn disk_can_lower_the_tier_or_rule_every_tier_out() {
        let ram = gib(124);
        assert_eq!(
            detect(ram, download_bytes(Tier::Large) + DISK_HEADROOM),
            Tier::Large
        );
        assert_eq!(
            detect(ram, download_bytes(Tier::Large) + DISK_HEADROOM - 1),
            Tier::Standard
        );
        assert_eq!(
            detect(ram, download_bytes(Tier::Small) + DISK_HEADROOM - 1),
            Tier::Unsupported
        );
    }

    #[test]
    fn options_mark_short_disk_and_short_memory() {
        let opts = tier_options(gib(32), download_bytes(Tier::Standard) + DISK_HEADROOM);
        let fits: Vec<(Tier, bool)> = opts.iter().map(|o| (o.tier, o.fits)).collect();
        assert_eq!(
            fits,
            [
                (Tier::Large, false),
                (Tier::Standard, true),
                (Tier::Small, true)
            ]
        );
        let short_disk = tier_options(gib(124), gib(3));
        assert!(short_disk.iter().all(|o| !o.fits));
        assert_eq!(short_disk[0].min_ram_gib, 60);
        assert_eq!(
            short_disk[0].download_bytes,
            download_bytes(Tier::Large) as f64
        );
    }

    #[test]
    fn components_match_the_roadmap_and_sizes() {
        let standard = components(Tier::Standard);
        assert_eq!(standard.len(), 4);
        assert_eq!(
            model_for(Tier::Standard, ComponentRole::Mind),
            Some("Qwen3.5-9B-GGUF")
        );
        assert_eq!(
            model_for(Tier::Small, ComponentRole::Speech),
            Some("Whisper-Base")
        );
        assert_eq!(
            model_for(Tier::Large, ComponentRole::Recall),
            Some("Qwen3-Embedding-0.6B-GGUF")
        );
        assert_eq!(
            model_for(Tier::Large, ComponentRole::Voice),
            Some("kokoro-v1")
        );
        assert!(components(Tier::Unsupported).is_empty());
        let gb = |t| download_bytes(t) as f64 / 1e9;
        assert!((gb(Tier::Large) - 26.4).abs() < 0.1);
        assert!((gb(Tier::Standard) - 8.4).abs() < 0.1);
        assert!((gb(Tier::Small) - 4.8).abs() < 0.1);
    }

    fn info(json: &str) -> SystemInfo {
        serde_json::from_str(json).unwrap()
    }

    const IGPU: &str = r#"{
      "devices": {"amd_gpu": [{"available": true, "integrated": true, "family": "gfx1150"}],
                  "amd_npu": {"available": true}},
      "recipes": {
        "llamacpp": {"backends": {"rocm": {"state": "installable"}, "vulkan": {"state": "installable"}, "cpu": {"state": "installable"}}},
        "whispercpp": {"backends": {"rocm": {"state": "installable"}, "vulkan": {"state": "installed"}, "cpu": {"state": "installable"}}},
        "kokoro": {"backends": {"cpu": {"state": "installable"}}}}}"#;

    #[test]
    fn an_integrated_gpu_runs_on_vulkan() {
        let info = info(IGPU);
        assert_eq!(choose_backend(&info, "llamacpp"), "vulkan");
        assert_eq!(choose_backend(&info, "whispercpp"), "vulkan");
        assert_eq!(choose_backend(&info, "kokoro"), "cpu");
    }

    #[test]
    fn a_discrete_amd_gpu_runs_on_rocm_when_it_can() {
        let json = IGPU.replace(r#""integrated": true"#, r#""integrated": false"#);
        assert_eq!(choose_backend(&info(&json), "llamacpp"), "rocm");
        let unsupported = json.replacen(
            r#""rocm": {"state": "installable"}"#,
            r#""rocm": {"state": "unsupported"}"#,
            1,
        );
        assert_eq!(choose_backend(&info(&unsupported), "llamacpp"), "cpu");
    }

    #[test]
    fn no_gpu_means_the_cpu() {
        let json = r#"{"devices": {"amd_gpu": []}, "recipes": {"llamacpp": {"backends": {"cpu": {"state": "installable"}}}}}"#;
        assert_eq!(choose_backend(&info(json), "llamacpp"), "cpu");
        assert_eq!(choose_backend(&SystemInfo::default(), "llamacpp"), "cpu");
    }
}
