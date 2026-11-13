//! When the morning page is due, in local time. Pure functions of the clock, the zone and the
//! page time, so the cases (a page time before 01:00, a clock change, a month end) are tested
//! without a scheduler.

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use nocturne_engine::day::page_instant;

/// The tidy does not open before this hour of the page's date.
const OPENS: (i8, i8) = (1, 0);
/// A page time at or before the opening hour opens the window this long before it.
const EARLY_LEAD: SignedDuration = SignedDuration::from_hours(6);

/// The next page: its date, its instant, and when the overnight window for it opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coming {
    pub date: Date,
    pub instant: Timestamp,
    pub opens: Timestamp,
}

/// Parse `HH:MM`, falling back to the default 07:00.
pub(crate) fn parse_hm(s: &str) -> (i8, i8) {
    s.split_once(':')
        .and_then(|(h, m)| Some((h.parse::<i8>().ok()?, m.parse::<i8>().ok()?)))
        .filter(|(h, m)| (0..24).contains(h) && (0..60).contains(m))
        .unwrap_or((7, 0))
}

fn local_date(now: Timestamp, tz: &TimeZone) -> Date {
    now.to_zoned(tz.clone()).date()
}

/// The first page time strictly after `now`.
pub fn coming(now: Timestamp, hm: (i8, i8), tz: &TimeZone) -> Option<Coming> {
    let mut date = local_date(now, tz).yesterday().ok()?;
    for _ in 0..4 {
        let instant = page_instant(date, hm, tz).ok()?;
        if instant > now {
            let opens = if hm <= OPENS {
                instant.checked_sub(EARLY_LEAD).ok()?
            } else {
                page_instant(date, OPENS, tz).ok()?
            };
            return Some(Coming {
                date,
                instant,
                opens,
            });
        }
        date = date.tomorrow().ok()?;
    }
    None
}

/// The date of the latest page time at or before `now`: today once today's page time has
/// passed, otherwise yesterday. A page dated on or after it has been written for this morning.
pub fn target_date(now: Timestamp, hm: (i8, i8), tz: &TimeZone) -> Option<Date> {
    let today = local_date(now, tz);
    if page_instant(today, hm, tz).ok()? <= now {
        Some(today)
    } else {
        today.yesterday().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tz() -> TimeZone {
        TimeZone::get("Europe/London").unwrap()
    }

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    #[test]
    fn before_the_page_time_the_page_is_today() {
        let c = coming(ts("2026-11-13T03:00:00Z"), (7, 0), &tz()).unwrap();
        assert_eq!(c.date, d("2026-11-13"));
        assert_eq!(c.instant, ts("2026-11-13T07:00:00Z"));
        assert_eq!(c.opens, ts("2026-11-13T01:00:00Z"));
        assert_eq!(
            target_date(ts("2026-11-13T03:00:00Z"), (7, 0), &tz()),
            Some(d("2026-11-12"))
        );
    }

    #[test]
    fn after_the_page_time_the_page_is_tomorrow() {
        let c = coming(ts("2026-11-13T07:00:00Z"), (7, 0), &tz()).unwrap();
        assert_eq!(
            c.date,
            d("2026-11-14"),
            "the page time itself is already passed"
        );
        assert_eq!(
            target_date(ts("2026-11-13T07:00:00Z"), (7, 0), &tz()),
            Some(d("2026-11-13"))
        );
    }

    #[test]
    fn a_page_time_at_or_before_one_opens_six_hours_early() {
        let c = coming(ts("2026-11-13T20:00:00Z"), (0, 30), &tz()).unwrap();
        assert_eq!(c.date, d("2026-11-14"));
        assert_eq!(c.instant, ts("2026-11-14T00:30:00Z"));
        assert_eq!(c.opens, ts("2026-11-13T18:30:00Z"));
        let c = coming(ts("2026-11-13T20:00:00Z"), (1, 0), &tz()).unwrap();
        assert_eq!(c.opens, ts("2026-11-13T19:00:00Z"));
    }

    #[test]
    fn local_time_decides_not_utc() {
        // 2026-07-01 is BST: 07:00 local is 06:00 UTC and the window opens at 00:00 UTC.
        let c = coming(ts("2026-07-01T00:30:00Z"), (7, 0), &tz()).unwrap();
        assert_eq!(c.date, d("2026-07-01"));
        assert_eq!(c.instant, ts("2026-07-01T06:00:00Z"));
        assert_eq!(c.opens, ts("2026-07-01T00:00:00Z"));
    }

    #[test]
    fn hm_parsing_falls_back() {
        assert_eq!(parse_hm("06:45"), (6, 45));
        assert_eq!(parse_hm("nonsense"), (7, 0));
        assert_eq!(parse_hm("25:00"), (7, 0));
    }
}
