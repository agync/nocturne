//! Which decision model serves a computer: the hardware rule, tier by tier.

use nocturne_contract::{ComponentRole, NpuCause};
use nocturne_contract::{Fit, Tier};
use nocturne_lemonade::client::SystemInfo;
use nocturne_lemonade::decision_models::{D1_OMNI_2B, D3_LITE_CPU, D3_LITE_FLM};
use nocturne_lemonade::fit::{self, DeciderPick, decider_for, memory_of};
use nocturne_lemonade::npu::{self, Placement, Readiness};
use nocturne_lemonade::selection::Selection;

const GIB: u64 = 1 << 30;

fn host(devices: &str, llamacpp: &str) -> SystemInfo {
    serde_json::from_str(&format!(
        r#"{{"devices": {{{devices}}},
            "recipes": {{"llamacpp": {{"backends": {llamacpp}}}}}}}"#
    ))
    .unwrap()
}

const VULKAN: &str = r#"{"vulkan": {"state": "installed"}, "cpu": {"state": "installable"}}"#;
const CPU_ONLY: &str = r#"{"cpu": {"state": "installed"}}"#;
const METAL: &str = r#"{"metal": {"state": "installed"}}"#;

fn integrated() -> SystemInfo {
    host(
        r#""amd_gpu": [{"available": true, "integrated": true}]"#,
        VULKAN,
    )
}

fn card(vram_gb: u64) -> SystemInfo {
    host(
        &format!(
            r#""nvidia_gpu": [{{"available": true, "integrated": false, "vram_gb": {vram_gb}}}]"#
        ),
        VULKAN,
    )
}

fn pick(info: &SystemInfo, tier: Tier, ram_gib: u64, npu: &Readiness) -> DeciderPick {
    let vram = info.devices.largest_pool();
    decider_for(info, &Selection::preset(tier), ram_gib * GIB, vram, npu)
}

#[test]
fn tiny_with_integrated_graphics_asks_the_mind() {
    // Tiny reads at 5.27 GB of a 6.5 GB budget: 2.2 GB more does not Fit.
    assert_eq!(
        pick(&integrated(), Tier::Tiny, 8, &Readiness::Absent),
        DeciderPick::Mind
    );
}

#[test]
fn tiny_with_a_ready_npu_still_asks_the_mind() {
    // The NPU's recordings model does not Fit on Tiny, so the NPU is not in use, and the
    // 1.1 GB decider on top of the preset's reading set passes 85% of the budget.
    assert_eq!(
        pick(&integrated(), Tier::Tiny, 8, &Readiness::Ready),
        DeciderPick::Mind
    );
}

#[test]
fn small_with_integrated_graphics_asks_the_mind() {
    // Small reads at 8.1 GB of a 10.3 GB budget (Fits up to 8.8 GB): no room for 2.2 GB.
    assert_eq!(
        pick(&integrated(), Tier::Small, 16, &Readiness::Absent),
        DeciderPick::Mind
    );
}

#[test]
fn standard_with_a_ready_npu_uses_d3_on_the_npu() {
    let info = integrated();
    assert_eq!(
        pick(&info, Tier::Standard, 32, &Readiness::Ready),
        DeciderPick::Model(&D3_LITE_FLM)
    );
}

#[test]
fn standard_with_a_big_enough_card_uses_d1() {
    assert_eq!(
        pick(&card(16), Tier::Standard, 32, &Readiness::Absent),
        DeciderPick::Model(&D1_OMNI_2B)
    );
}

#[test]
fn a_card_too_small_for_the_mind_recall_and_d1_asks_the_mind() {
    // Qwen3.5-9B 8.1 + recall 1.7 + d1 2.2 = 12 GB does not sit on an 8 GB card.
    assert_eq!(
        pick(&card(8), Tier::Standard, 32, &Readiness::Absent),
        DeciderPick::Mind
    );
}

#[test]
fn a_card_too_small_falls_back_to_the_npu_when_it_is_ready() {
    assert_eq!(
        pick(&card(8), Tier::Standard, 32, &Readiness::Ready),
        DeciderPick::Model(&D3_LITE_FLM)
    );
}

#[test]
fn the_npu_model_wins_over_a_capable_card() {
    assert_eq!(
        pick(&card(16), Tier::Standard, 32, &Readiness::Ready),
        DeciderPick::Model(&D3_LITE_FLM)
    );
}

#[test]
fn large_on_a_mac_uses_d1_through_metal() {
    let mac = host(r#""metal": {"available": true}"#, METAL);
    assert_eq!(
        pick(&mac, Tier::Large, 64, &Readiness::Absent),
        DeciderPick::Model(&D1_OMNI_2B)
    );
}

#[test]
fn cpu_only_asks_the_mind() {
    let cpu = host(r#""amd_gpu": []"#, CPU_ONLY);
    assert_eq!(
        pick(&cpu, Tier::Standard, 32, &Readiness::Absent),
        DeciderPick::Mind
    );
    assert_eq!(
        pick(&cpu, Tier::Large, 64, &Readiness::Absent),
        DeciderPick::Mind
    );
}

#[test]
fn an_npu_that_is_not_usable_now_is_not_picked() {
    let info = integrated();
    for r in [
        Readiness::Fetch,
        Readiness::Blocked {
            cause: NpuCause::DriverMissing,
            detail: None,
        },
        Readiness::Absent,
    ] {
        // Standard on shared memory: the NPU model is out, the GPU model fits beside the mind.
        assert_eq!(
            pick(&info, Tier::Standard, 32, &r),
            DeciderPick::Model(&D1_OMNI_2B),
            "{r:?}"
        );
        assert_ne!(
            pick(&info, Tier::Standard, 32, &r),
            DeciderPick::Model(&D3_LITE_FLM)
        );
    }
}

#[test]
fn an_unsupported_tier_asks_the_mind() {
    assert_eq!(
        pick(&card(24), Tier::Unsupported, 64, &Readiness::Ready),
        DeciderPick::Mind
    );
}

#[test]
fn a_mind_the_person_chose_does_not_change_the_rule() {
    let mut s = Selection::preset(Tier::Standard);
    s.custom
        .insert(ComponentRole::Mind, "Qwen3.5-4B-MTP-GGUF".into());
    let info = integrated();
    assert_eq!(
        decider_for(&info, &s, 32 * GIB, 0, &Readiness::Ready),
        DeciderPick::Model(&D3_LITE_FLM)
    );
}

#[test]
fn the_cpu_arm_is_never_picked() {
    let info = integrated();
    for tier in [Tier::Tiny, Tier::Small, Tier::Standard, Tier::Large] {
        for r in [Readiness::Absent, Readiness::Ready] {
            for ram in [8, 16, 32, 64, 128] {
                assert_ne!(pick(&info, tier, ram, &r), DeciderPick::Model(&D3_LITE_CPU));
            }
        }
    }
}

#[test]
fn the_decision_models_memory_counts_in_the_reading_moment() {
    // Small reads at 8.1 GB. On 16 GiB the budget is 10.3 GB, so a model of up to 0.7 GB would
    // Fit and d1 at 2.2 GB does not, though Small alone Fits.
    let info = integrated();
    let s = Selection::preset(Tier::Small);
    let b = fit::budget(16 * GIB, 0, u64::MAX);
    let alone = fit::busiest(&s, &[], &b);
    assert_eq!(fit::fit_for(alone, &b), Fit::Fits);
    assert_ne!(
        fit::fit_for(alone + memory_of(D1_OMNI_2B.id, None), &b),
        Fit::Fits
    );
    assert_eq!(
        decider_for(&info, &s, 16 * GIB, 0, &Readiness::Absent),
        DeciderPick::Mind
    );
    // With more memory the same selection has room, so the answer comes from the budget.
    assert_eq!(
        decider_for(&info, &s, 24 * GIB, 0, &Readiness::Absent),
        DeciderPick::Model(&D1_OMNI_2B)
    );
}

#[test]
fn the_npu_minds_reading_set_is_counted_when_it_is_placed() {
    // Large reads at 29.3 GB. At 64 GiB the NPU mind does not fit beside it (placement stays on
    // the GPU) and the NPU decider adds 1.1 GB to Large alone. At 72 GiB the NPU mind is placed
    // beside Large's, 9.8 GB more, so the same decider no longer Fits; at 80 GiB it does again.
    let info = integrated();
    let s = Selection::preset(Tier::Large);
    let at = |gib: u64| decider_for(&info, &s, gib * GIB, 0, &Readiness::Ready);
    assert_eq!(npu::placement(&s, 64 * GIB, 0), Placement::Gpu);
    assert_eq!(at(64), DeciderPick::Model(&D3_LITE_FLM));
    assert_eq!(npu::placement(&s, 72 * GIB, 0), Placement::Split);
    assert_eq!(at(72), DeciderPick::Mind);
    assert_eq!(at(80), DeciderPick::Model(&D3_LITE_FLM));
}
