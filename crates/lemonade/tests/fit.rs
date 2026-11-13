//! The fit budget's acceptance tests: every tier preset fits at its own minimum.

use nocturne_contract::{Fit, Tier};
use nocturne_lemonade::fit::{budget, fit_for};
use nocturne_lemonade::fit_table::{VOICE_ALONE, preset_peak};
use nocturne_lemonade::tier::min_ram_gib;

const GIB: u64 = 1 << 30;

fn gib(b: u64) -> f64 {
    b as f64 / GIB as f64
}

#[test]
fn budget_values() {
    for (ram, want) in [
        (6, 5.0),
        (8, 6.1),
        (14, 9.4),
        (16, 10.5),
        (28, 17.1),
        (32, 19.3),
        (60, 34.7),
    ] {
        let got = gib(budget(ram * GIB, 0, 0).budget);
        assert!(
            (got - want).abs() < 0.05,
            "{ram} GiB: {got:.2}, want {want}"
        );
    }
}

#[test]
fn tier_presets_fit_their_minimums() {
    for tier in [Tier::Tiny, Tier::Small, Tier::Standard, Tier::Large] {
        let ram = u64::from(min_ram_gib(tier)) * GIB;
        let b = budget(ram, 0, 0);
        let peak = preset_peak(tier).expect("every preset has a peak");
        let fit = fit_for(peak.busiest, &b);
        if tier == Tier::Tiny {
            assert!(
                matches!(fit, Fit::Fits | Fit::Tight),
                "{tier:?} at {} GiB: {fit:?}",
                min_ram_gib(tier)
            );
        } else {
            assert_eq!(fit, Fit::Fits, "{tier:?} at {} GiB", min_ram_gib(tier));
        }
        assert_eq!(
            fit_for(VOICE_ALONE, &b),
            Fit::Fits,
            "the voice alone on {tier:?}"
        );
    }
    // The 8 GB computer Tiny is made for has room to spare.
    assert_eq!(
        fit_for(
            preset_peak(Tier::Tiny).unwrap().busiest,
            &budget(8 * GIB, 0, 0)
        ),
        Fit::Fits
    );
}

#[test]
fn fit_thresholds() {
    let b = budget(16 * GIB, 0, 0);
    assert_eq!(fit_for(b.budget / 100 * 85, &b), Fit::Fits);
    assert_eq!(fit_for(b.budget, &b), Fit::Tight);
    assert_eq!(fit_for(b.budget + 1, &b), Fit::WontFit);
}
