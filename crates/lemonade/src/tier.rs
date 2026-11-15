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
/// The recall model, the same on every tier.
pub const RECALL: Component = c(
    ComponentRole::Recall,
    "Qwen3-Embedding-0.6B-GGUF",
    640_000_000,
);

/// The context the recall model is loaded with. It reads one memory sentence or one question
/// at a time; left to itself the engine gives it 32768, a 3.7 GB cache that is never used. The
/// engine raises anything smaller than 8192 to 8192, and takes a saved 4096 for its old default.
pub const RECALL_CONTEXT: u32 = 8192;

/// The reranking model: it orders what the search found by how well each memory answers the
/// question. It works beside the recall model, so it carries that role.
pub const RERANK: Component = c(
    ComponentRole::Recall,
    "bge-reranker-v2-m3-GGUF",
    636_000_000,
);

/// The context the reranking model is loaded with: a question and one memory sentence at a time,
/// far below what the engine would give it by itself. The engine raises anything smaller than
/// 8192 to 8192.
pub const RERANK_CONTEXT: u32 = 8192;

/// Models a tier uses when they are on disk and does without otherwise. They are not part of
/// what makes a tier installed, so a tier installed before one was added keeps working.
pub fn extras(tier: Tier) -> Vec<Component> {
    match tier {
        Tier::Large | Tier::Standard => vec![RERANK],
        Tier::Small | Tier::Tiny | Tier::Unsupported => Vec::new(),
    }
}

/// The reranking model `tier` uses, if any.
pub fn reranker(tier: Tier) -> Option<&'static str> {
    extras(tier)
        .into_iter()
        .find(|c| c.model == RERANK.model)
        .map(|c| c.model)
}

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
        Tier::Tiny => vec![
            c(ComponentRole::Mind, "Qwen3.5-2B-GGUF", 2_010_000_000),
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

/// What setting up `tier` fetches: its components and its extras.
pub fn download_bytes(tier: Tier) -> u64 {
    components(tier)
        .iter()
        .chain(&extras(tier))
        .map(|c| c.bytes)
        .sum()
}

/// What `tier` still needs fetched when everything `held` fetched is already on disk: models the
/// two share (the voice, the recall model and the reranking model) are not counted again.
pub fn missing_bytes(tier: Tier, held: Tier) -> u64 {
    let have: Vec<Component> = components(held).into_iter().chain(extras(held)).collect();
    components(tier)
        .iter()
        .chain(&extras(tier))
        .filter(|c| !have.iter().any(|h| h.model == c.model))
        .map(|c| c.bytes)
        .sum()
}

pub fn min_ram_gib(tier: Tier) -> u32 {
    match tier {
        Tier::Large => 60,
        Tier::Standard => 28,
        Tier::Small => 14,
        // An 8 GB computer reports a little under 8 GiB once the system has taken its share.
        Tier::Tiny => 6,
        Tier::Unsupported => 0,
    }
}

const ORDER: [Tier; 4] = [Tier::Large, Tier::Standard, Tier::Small, Tier::Tiny];

/// The tier with the least to ask of a machine: below it, nothing fits.
pub const SMALLEST: Tier = Tier::Tiny;

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

/// The backend to run `recipe` on: Metal on a Mac, ROCm on a discrete AMD GPU, Vulkan on any
/// other graphics (AMD integrated, NVIDIA, Intel), otherwise the CPU. Falls back to the CPU when
/// the preferred backend is not available on this machine, and to Metal where the CPU is not
/// offered either (the engine runs everything through Metal on a Mac).
///
/// The engine's CUDA backend is left out on purpose: it serves llama.cpp only, so speech and
/// voice would still need another backend, and it fetches a runtime of its own. Vulkan runs on
/// every NVIDIA card the engine finds, on the same download as the other graphics.
pub fn choose_backend(info: &SystemInfo, recipe: &str) -> &'static str {
    let usable = |backend: &str| {
        matches!(
            info.backend_state(recipe, backend),
            BackendState::Installable | BackendState::Installed
        )
    };
    let metal = info.devices.metal.as_ref().is_some_and(|m| m.available);
    let preferred = if metal {
        "metal"
    } else if info
        .devices
        .amd_gpu
        .iter()
        .any(|g| g.available && !g.integrated)
    {
        "rocm"
    } else if info.devices.has_graphics() {
        "vulkan"
    } else {
        "cpu"
    };
    // On a Mac the engine offers nothing but Metal: an unknown state is no reason to fall back
    // to a CPU backend that cannot run there.
    if metal && info.backend_state(recipe, "metal") != BackendState::Unsupported {
        return "metal";
    }
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
        assert_eq!(
            missing_bytes(Tier::Tiny, Tier::Standard),
            2_010_000_000 + 148_000_000
        );
        assert_eq!(missing_bytes(Tier::Small, Tier::Small), 0);
        assert_eq!(
            missing_bytes(Tier::Standard, Tier::Small),
            6_880_000_000 + 488_000_000 + 636_000_000
        );
        assert_eq!(
            missing_bytes(Tier::Large, Tier::Standard),
            23_800_000_000 + 1_620_000_000
        );
        assert_eq!(
            missing_bytes(Tier::Small, Tier::Unsupported),
            download_bytes(Tier::Small)
        );
    }

    #[test]
    fn detects_by_memory() {
        assert_eq!(detect(gib(4), PLENTY), Tier::Unsupported);
        // An 8 GB Mac reports 8 GiB; an 8 GB Linux computer a little under.
        assert_eq!(detect(gib(8), PLENTY), Tier::Tiny);
        assert_eq!(detect(gib(15) / 2, PLENTY), Tier::Tiny);
        assert_eq!(detect(gib(16), PLENTY), Tier::Small);
        assert_eq!(detect(gib(32), PLENTY), Tier::Standard);
        assert_eq!(detect(gib(64), PLENTY), Tier::Large);
        assert_eq!(detect(gib(124), PLENTY), Tier::Large);
    }

    #[test]
    fn the_thresholds_are_inclusive() {
        assert_eq!(detect(gib(14), PLENTY), Tier::Small);
        assert_eq!(detect(gib(14) - 1, PLENTY), Tier::Tiny);
        assert_eq!(detect(gib(6), PLENTY), Tier::Tiny);
        assert_eq!(detect(gib(6) - 1, PLENTY), Tier::Unsupported);
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
            Tier::Tiny
        );
        assert_eq!(
            detect(ram, download_bytes(Tier::Tiny) + DISK_HEADROOM - 1),
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
                (Tier::Small, true),
                (Tier::Tiny, true)
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
    fn only_standard_and_large_rerank_and_it_is_never_required() {
        assert_eq!(reranker(Tier::Large), Some("bge-reranker-v2-m3-GGUF"));
        assert_eq!(reranker(Tier::Standard), Some("bge-reranker-v2-m3-GGUF"));
        for t in [Tier::Small, Tier::Tiny, Tier::Unsupported] {
            assert_eq!(reranker(t), None, "{t:?}");
            assert!(extras(t).is_empty());
        }
        for t in [Tier::Large, Tier::Standard] {
            assert!(
                components(t).iter().all(|c| c.model != RERANK.model),
                "{t:?} is installed without it"
            );
            assert_eq!(model_for(t, ComponentRole::Recall), Some(RECALL.model));
        }
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
        assert!((gb(Tier::Large) - 27.0).abs() < 0.1);
        assert!((gb(Tier::Standard) - 9.0).abs() < 0.1);
        assert!((gb(Tier::Small) - 4.8).abs() < 0.1);
        assert!((gb(Tier::Tiny) - 3.2).abs() < 0.1);
        assert_eq!(
            model_for(Tier::Tiny, ComponentRole::Mind),
            Some("Qwen3.5-2B-GGUF")
        );
        assert_eq!(min_ram_gib(SMALLEST), 6);
    }

    #[test]
    fn every_tier_can_choose_tiny_and_tiny_needs_only_its_mind_after_small() {
        for ram in [gib(8), gib(16), gib(32), gib(64)] {
            let tiny = tier_options(ram, PLENTY)
                .into_iter()
                .find(|o| o.tier == Tier::Tiny)
                .unwrap();
            assert!(tiny.fits, "{ram}");
        }
        assert_eq!(missing_bytes(Tier::Tiny, Tier::Small), 2_010_000_000);
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
    fn a_mac_runs_everything_on_metal() {
        let json = r#"{"devices": {"amd_gpu": [], "metal": {"available": true, "name": "Apple M4"}},
          "recipes": {
            "llamacpp": {"backends": {"cpu": {"state": "unsupported"}, "metal": {"state": "installable"}, "vulkan": {"state": "unsupported"}}},
            "whispercpp": {"backends": {"cpu": {"state": "unsupported"}, "metal": {"state": "installed"}}},
            "kokoro": {"backends": {"cpu": {"state": "unsupported"}, "metal": {"state": "installable"}}}}}"#;
        let info = info(json);
        for recipe in ["llamacpp", "whispercpp", "kokoro"] {
            assert_eq!(choose_backend(&info, recipe), "metal", "{recipe}");
        }
    }

    #[test]
    fn a_mac_stays_on_metal_when_it_needs_an_update_or_says_nothing() {
        let json = r#"{"devices": {"amd_gpu": [], "metal": {"available": true}},
          "recipes": {
            "llamacpp": {"backends": {"cpu": {"state": "unsupported"}, "metal": {"state": "update_required"}}},
            "whispercpp": {"backends": {"cpu": {"state": "unsupported"}}}}}"#;
        let mac = info(json);
        assert_eq!(
            mac.backend_state("llamacpp", "metal"),
            BackendState::Installable
        );
        assert_eq!(choose_backend(&mac, "llamacpp"), "metal");
        assert_eq!(choose_backend(&mac, "whispercpp"), "metal");
        let unsupported = json.replace(r#""update_required""#, r#""unsupported""#);
        assert_eq!(choose_backend(&info(&unsupported), "llamacpp"), "cpu");
    }

    /// What the engine says on a computer whose graphics are not AMD's: Vulkan runs on the CPU
    /// entry, so it is installable whatever the graphics, and CUDA only where NVIDIA is found.
    fn recipes(cuda: &str) -> String {
        format!(
            r#""recipes": {{
              "llamacpp": {{"backends": {{"cuda": {{"state": "{cuda}"}}, "vulkan": {{"state": "installable"}}, "rocm": {{"state": "unsupported"}}, "cpu": {{"state": "installable"}}}}}},
              "whispercpp": {{"backends": {{"vulkan": {{"state": "installable"}}, "rocm": {{"state": "unsupported"}}, "cpu": {{"state": "installable"}}}}}},
              "kokoro": {{"backends": {{"cpu": {{"state": "installable"}}}}}}}}"#
        )
    }

    fn nvidia(gpu: &str) -> SystemInfo {
        info(&format!(
            r#"{{"devices": {{"amd_gpu": [], "nvidia_gpu": [{gpu}]}}, {}}}"#,
            recipes("installable")
        ))
    }

    #[test]
    fn an_nvidia_card_runs_on_vulkan() {
        let info = nvidia(
            r#"{"available": true, "name": "NVIDIA GeForce RTX 4060", "family": "sm_89", "compute_capability": "8.9", "vram_gb": 8.0}"#,
        );
        assert_eq!(
            info.backend_state("llamacpp", "cuda"),
            BackendState::Installable
        );
        assert_eq!(choose_backend(&info, "llamacpp"), "vulkan");
        assert_eq!(choose_backend(&info, "whispercpp"), "vulkan");
        assert_eq!(choose_backend(&info, "kokoro"), "cpu");
    }

    #[test]
    fn an_nvidia_card_the_engine_could_not_find_means_the_cpu() {
        let none =
            nvidia(r#"{"available": false, "error": "No NVIDIA discrete GPU found", "name": ""}"#);
        assert_eq!(choose_backend(&none, "llamacpp"), "cpu");
        assert_eq!(choose_backend(&none, "whispercpp"), "cpu");
    }

    #[test]
    fn nvidia_beside_an_amd_integrated_gpu_runs_on_vulkan() {
        let json = format!(
            r#"{{"devices": {{"amd_gpu": [{{"available": true, "integrated": true}}],
                "nvidia_gpu": [{{"available": true, "vram_gb": 6.0}}]}}, {}}}"#,
            recipes("installable")
        );
        assert_eq!(choose_backend(&info(&json), "llamacpp"), "vulkan");
    }

    #[test]
    fn a_discrete_amd_gpu_keeps_rocm_beside_other_graphics() {
        let json = format!(
            r#"{{"devices": {{"amd_gpu": [{{"available": true, "integrated": false}}],
                "intel_gpu": [{{"available": true}}]}}, {}}}"#,
            recipes("unsupported").replace(
                r#""rocm": {"state": "unsupported"}"#,
                r#""rocm": {"state": "installable"}"#
            )
        );
        assert_eq!(choose_backend(&info(&json), "llamacpp"), "rocm");
    }

    #[test]
    fn intel_graphics_run_on_vulkan() {
        let json = format!(
            r#"{{"devices": {{"amd_gpu": [], "nvidia_gpu": [{{"available": false, "name": ""}}],
                "intel_gpu": [{{"available": true, "name": "Intel graphics"}}]}}, {}}}"#,
            recipes("unsupported")
        );
        let info = info(&json);
        assert_eq!(choose_backend(&info, "llamacpp"), "vulkan");
        assert_eq!(choose_backend(&info, "whispercpp"), "vulkan");
        assert_eq!(choose_backend(&info, "kokoro"), "cpu");
    }

    #[test]
    fn vulkan_the_engine_cannot_run_means_the_cpu_on_any_graphics() {
        for gpu in [
            r#""nvidia_gpu": [{"available": true}]"#,
            r#""intel_gpu": [{"available": true}]"#,
            r#""amd_gpu": [{"available": true, "integrated": true}]"#,
        ] {
            let json = format!(
                r#"{{"devices": {{{gpu}}}, {}}}"#,
                recipes("installable").replace(
                    r#""vulkan": {"state": "installable"}"#,
                    r#""vulkan": {"state": "unsupported"}"#
                )
            );
            assert_eq!(choose_backend(&info(&json), "llamacpp"), "cpu", "{gpu}");
        }
    }

    #[test]
    fn vulkan_already_installed_is_chosen() {
        let json = format!(
            r#"{{"devices": {{"intel_gpu": [{{"available": true}}]}}, {}}}"#,
            recipes("unsupported").replace(
                r#""vulkan": {"state": "installable"}"#,
                r#""vulkan": {"state": "installed"}"#
            )
        );
        assert_eq!(choose_backend(&info(&json), "llamacpp"), "vulkan");
    }

    #[test]
    fn an_unavailable_intel_entry_is_not_graphics() {
        let json = format!(
            r#"{{"devices": {{"intel_gpu": [{{"available": false}}]}}, {}}}"#,
            recipes("unsupported")
        );
        assert_eq!(choose_backend(&info(&json), "llamacpp"), "cpu");
    }

    #[test]
    fn the_engine_with_no_nvidia_entry_at_all_still_reads() {
        let old = info(r#"{"devices": {"amd_gpu": []}}"#);
        assert!(old.devices.nvidia_gpu.is_empty() && old.devices.intel_gpu.is_empty());
        assert!(!old.devices.has_graphics());
    }

    #[test]
    fn no_gpu_means_the_cpu() {
        let json = r#"{"devices": {"amd_gpu": []}, "recipes": {"llamacpp": {"backends": {"cpu": {"state": "installable"}}}}}"#;
        assert_eq!(choose_backend(&info(json), "llamacpp"), "cpu");
        assert_eq!(choose_backend(&SystemInfo::default(), "llamacpp"), "cpu");
    }
}
