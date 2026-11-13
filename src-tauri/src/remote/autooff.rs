//! Turning off by itself: after a day or a week on, after thirty days without a phone using it,
//! or never. Checked at launch and every ten minutes; when due, Away from home stops as the
//! switch does and phones stay paired.

use jiff::{SignedDuration, Timestamp};
use nocturne_contract::AutoOff;

/// Whether Away from home should turn off now. `on_since` is when it last turned on;
/// `last_used` the latest any phone used it.
pub fn due(
    rule: AutoOff,
    on_since: Option<Timestamp>,
    last_used: Option<Timestamp>,
    now: Timestamp,
) -> bool {
    let Some(on_since) = on_since else {
        return false;
    };
    let after = |from: Timestamp, days: i64| {
        from.checked_add(SignedDuration::from_hours(24 * days))
            .is_ok_and(|t| now >= t)
    };
    match rule {
        AutoOff::AfterDay => after(on_since, 1),
        AutoOff::AfterWeek => after(on_since, 7),
        AutoOff::Unused30Days => after(last_used.map_or(on_since, |u| u.max(on_since)), 30),
        AutoOff::Never => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn auto_off_table() {
        let on = Some(t("2026-11-01T08:00:00Z"));
        let used = Some(t("2026-11-20T08:00:00Z"));
        for (rule, last, now, want) in [
            (AutoOff::AfterDay, None, "2026-11-02T07:59:59Z", false),
            (AutoOff::AfterDay, None, "2026-11-02T08:00:00Z", true),
            (AutoOff::AfterWeek, used, "2026-11-08T07:59:59Z", false),
            (AutoOff::AfterWeek, used, "2026-11-08T08:00:00Z", true),
            (AutoOff::Unused30Days, None, "2026-12-01T08:00:00Z", true),
            (AutoOff::Unused30Days, used, "2026-12-01T08:00:00Z", false),
            (AutoOff::Unused30Days, used, "2026-12-20T08:00:00Z", true),
            (
                AutoOff::Unused30Days,
                Some(t("2026-10-01T00:00:00Z")),
                "2026-12-01T07:00:00Z",
                false,
            ),
            (AutoOff::Never, None, "2030-01-01T00:00:00Z", false),
        ] {
            assert_eq!(due(rule, on, last, t(now)), want, "{rule:?} {now}");
        }
        assert!(
            !due(AutoOff::AfterDay, None, None, t("2030-01-01T00:00:00Z")),
            "never on"
        );
    }
}
