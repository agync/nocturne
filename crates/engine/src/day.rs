//! Page windows in local time.
//!
//! A page's window is `[previous saved page's window end, this page's time on its date)`, held
//! as instants, so a day is 23 or 25 hours on a DST night, a time zone change loses and repeats
//! nothing, and several missed days become one catch-up window. The page date is the local date
//! of the window end. A page time inside a spring-forward gap resolves to the instant just after
//! the gap (jiff's "compatible" rule).

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

use crate::error::Result;

/// The last saved page before the one being composed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrevPage {
    pub date: Date,
    pub end: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub start: Timestamp,
    pub end: Timestamp,
    /// Days with no page between the previous page and this one.
    pub catch_up_days: u32,
    /// No page has been saved before this one.
    pub first: bool,
}

impl Window {
    pub fn contains(&self, t: Timestamp) -> bool {
        self.start <= t && t < self.end
    }
}

/// The instant of `HH:MM` on `day` in `tz`.
pub fn page_instant(day: Date, hm: (i8, i8), tz: &TimeZone) -> Result<Timestamp> {
    let dt = day.at(hm.0, hm.1, 0, 0);
    Ok(tz.to_ambiguous_zoned_datetime(dt).compatible()?.timestamp())
}

/// The window for the page dated `day`. `earliest` is the oldest item ever kept, used only for
/// the very first page so it covers everything kept so far.
pub fn window(
    day: Date,
    hm: (i8, i8),
    tz: &TimeZone,
    prev: Option<PrevPage>,
    earliest: Option<Timestamp>,
) -> Result<Window> {
    let end = page_instant(day, hm, tz)?;
    match prev {
        Some(p) => {
            let missed = i64::from(day.since(p.date)?.get_days() - 1).max(0) as u32;
            Ok(Window {
                start: p.end,
                end: end.max(p.end),
                catch_up_days: missed,
                first: false,
            })
        }
        None => {
            let calendar_start = page_instant(day.yesterday()?, hm, tz)?;
            let start = earliest
                .map_or(calendar_start, |e| e.min(calendar_start))
                .min(end);
            Ok(Window {
                start,
                end,
                catch_up_days: 0,
                first: true,
            })
        }
    }
}

/// Minutes after midnight, on the morning after a journal day, at which that day ends: 4:00 am,
/// or half an hour before a page time earlier than 4:30 am, so the page always comes after the
/// day it looks back on. Negative when that is before midnight.
pub fn journal_cutoff(page_hm: (i8, i8)) -> i64 {
    let page = i64::from(page_hm.0) * 60 + i64::from(page_hm.1);
    if page < 4 * 60 + 30 {
        page - 30
    } else {
        4 * 60
    }
}

/// The journal day an instant belongs to: a note kept at 12:40 am belongs to the evening before.
/// It is the day whose bounds hold the instant, so it agrees with `journal_day_bounds` even
/// where a cutoff falls in a clock change.
pub fn journal_day_of(t: Timestamp, page_hm: (i8, i8), tz: &TimeZone) -> Result<Date> {
    let local = t.to_zoned_datetime(tz.clone()).datetime();
    let near = local
        .checked_sub(jiff::SignedDuration::from_mins(journal_cutoff(page_hm)))?
        .date();
    for day in [near, near.yesterday()?, near.tomorrow()?] {
        let (start, end) = journal_day_bounds(day, page_hm, tz)?;
        if start <= t && t < end {
            return Ok(day);
        }
    }
    Ok(near)
}

/// The instants a journal day runs between: `[start, end)`. On a DST night a day is 23 or 25
/// hours; a cutoff inside a spring-forward gap resolves to the instant just after the gap, and
/// is never later than half an hour before the page that follows.
pub fn journal_day_bounds(
    day: Date,
    page_hm: (i8, i8),
    tz: &TimeZone,
) -> Result<(Timestamp, Timestamp)> {
    let edge = |d: Date| -> Result<Timestamp> {
        let midnight = d.tomorrow()?.at(0, 0, 0, 0);
        let at = midnight.checked_add(jiff::SignedDuration::from_mins(journal_cutoff(page_hm)))?;
        let cutoff = tz.to_ambiguous_zoned_datetime(at).compatible()?.timestamp();
        let latest = page_instant(d.tomorrow()?, page_hm, tz)?
            .checked_sub(jiff::SignedDuration::from_mins(30))?;
        Ok(cutoff.min(latest))
    };
    Ok((edge(day.yesterday()?)?, edge(day)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tz(name: &str) -> TimeZone {
        TimeZone::get(name).unwrap()
    }

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn prev(date: &str, tz_name: &str) -> PrevPage {
        PrevPage {
            date: d(date),
            end: page_instant(d(date), (7, 0), &tz(tz_name)).unwrap(),
        }
    }

    #[test]
    fn a_capture_at_0130_on_the_night_clocks_go_back_lands_in_the_right_day() {
        // Review focus 2. London, 2026-10-25: 02:00 BST becomes 01:00 GMT, so 01:30 happens twice.
        let london = tz("Europe/London");
        let w = window(
            d("2026-10-25"),
            (7, 0),
            &london,
            Some(prev("2026-10-24", "Europe/London")),
            None,
        )
        .unwrap();
        let first_0130 = ts("2026-10-25T00:30:00Z"); // 01:30 BST
        let second_0130 = ts("2026-10-25T01:30:00Z"); // 01:30 GMT
        assert!(w.contains(first_0130));
        assert!(w.contains(second_0130));
        // The next page starts exactly where this one ends: neither instant is counted twice.
        let next = window(
            d("2026-10-26"),
            (7, 0),
            &london,
            Some(PrevPage {
                date: d("2026-10-25"),
                end: w.end,
            }),
            None,
        )
        .unwrap();
        assert!(!next.contains(first_0130) && !next.contains(second_0130));
        assert_eq!(next.start, w.end);
        // The night is 25 hours long.
        assert_eq!(w.end.duration_since(w.start).as_hours(), 25);
    }

    #[test]
    fn an_early_page_on_the_night_clocks_go_forward_still_comes_after_the_day() {
        // Paris, 2026-03-29: 02:00 CET becomes 03:00 CEST. A 03:00 page is 01:00 UTC; the
        // nominal 02:30 cutoff falls in the gap and would resolve after the page.
        let paris = tz("Europe/Paris");
        let page = page_instant(d("2026-03-29"), (3, 0), &paris).unwrap();
        assert_eq!(page, ts("2026-03-29T01:00:00Z"));
        let (_, end) = journal_day_bounds(d("2026-03-28"), (3, 0), &paris).unwrap();
        assert!(page.duration_since(end) >= jiff::SignedDuration::from_mins(30));
        let (start, _) = journal_day_bounds(d("2026-03-29"), (3, 0), &paris).unwrap();
        assert_eq!(start, end);
        let quarter_past_three = ts("2026-03-29T01:15:00Z");
        assert_eq!(
            journal_day_of(quarter_past_three, (3, 0), &paris).unwrap(),
            d("2026-03-29")
        );
    }

    #[test]
    fn a_window_is_23_hours_when_clocks_go_forward() {
        let london = tz("Europe/London");
        let w = window(
            d("2026-03-29"),
            (7, 0),
            &london,
            Some(prev("2026-03-28", "Europe/London")),
            None,
        )
        .unwrap();
        assert_eq!(w.end.duration_since(w.start).as_hours(), 23);
        let normal = window(
            d("2026-03-30"),
            (7, 0),
            &london,
            Some(prev("2026-03-29", "Europe/London")),
            None,
        )
        .unwrap();
        assert_eq!(normal.end.duration_since(normal.start).as_hours(), 24);
    }

    #[test]
    fn a_page_time_inside_the_spring_forward_gap_resolves_after_the_gap() {
        let london = tz("Europe/London");
        // 01:30 on 2026-03-29 does not exist in London; the instant is 02:30 BST = 01:30Z.
        let t = page_instant(d("2026-03-29"), (1, 30), &london).unwrap();
        assert_eq!(t, ts("2026-03-29T01:30:00Z"));
        assert_eq!(t.to_zoned_datetime(london).date(), d("2026-03-29"));
    }

    #[test]
    fn a_time_zone_change_between_pages_loses_and_repeats_nothing() {
        let london_prev = prev("2026-11-10", "Europe/London");
        let tokyo = tz("Asia/Tokyo");
        let w = window(d("2026-11-11"), (7, 0), &tokyo, Some(london_prev), None).unwrap();
        assert_eq!(w.start, london_prev.end, "starts where the last page ended");
        let after = window(
            d("2026-11-12"),
            (7, 0),
            &tokyo,
            Some(PrevPage {
                date: d("2026-11-11"),
                end: w.end,
            }),
            None,
        )
        .unwrap();
        assert_eq!(after.start, w.end);
        // Flying west, the new page time can fall before the old end: the window is empty, not negative.
        let ny = tz("America/New_York");
        let back = window(
            d("2026-11-11"),
            (7, 0),
            &ny,
            Some(PrevPage {
                date: d("2026-11-10"),
                end: ts("2026-11-11T22:00:00Z"),
            }),
            None,
        )
        .unwrap();
        assert_eq!(back.start, back.end);
    }

    #[test]
    fn three_missed_days_are_one_catch_up_window() {
        let london = tz("Europe/London");
        let w = window(
            d("2026-11-13"),
            (7, 0),
            &london,
            Some(prev("2026-11-09", "Europe/London")),
            None,
        )
        .unwrap();
        assert_eq!(w.catch_up_days, 3);
        assert_eq!(w.end.duration_since(w.start).as_hours(), 4 * 24);
        let yesterday = window(
            d("2026-11-13"),
            (7, 0),
            &london,
            Some(prev("2026-11-12", "Europe/London")),
            None,
        )
        .unwrap();
        assert_eq!(yesterday.catch_up_days, 0);
    }

    #[test]
    fn the_first_page_covers_everything_kept_so_far() {
        let london = tz("Europe/London");
        let w = window(
            d("2026-11-13"),
            (7, 0),
            &london,
            None,
            Some(ts("2026-11-01T10:00:00Z")),
        )
        .unwrap();
        assert!(w.first);
        assert_eq!(w.start, ts("2026-11-01T10:00:00Z"));
        let bare = window(d("2026-11-13"), (7, 0), &london, None, None).unwrap();
        assert_eq!(bare.end.duration_since(bare.start).as_hours(), 24);
    }

    #[test]
    fn a_note_after_midnight_belongs_to_the_evening_before() {
        let london = tz("Europe/London");
        // 00:40 GMT on Friday 20 November is Thursday's journal.
        let day = journal_day_of(ts("2026-11-20T00:40:00Z"), (7, 0), &london).unwrap();
        assert_eq!(day, d("2026-11-19"));
        let day = journal_day_of(ts("2026-11-20T04:00:00Z"), (7, 0), &london).unwrap();
        assert_eq!(day, d("2026-11-20"));
        let (start, end) = journal_day_bounds(d("2026-11-19"), (7, 0), &london).unwrap();
        assert_eq!(start, ts("2026-11-19T04:00:00Z"));
        assert_eq!(end, ts("2026-11-20T04:00:00Z"));
    }

    #[test]
    fn an_early_page_ends_the_day_half_an_hour_before_it() {
        let london = tz("Europe/London");
        assert_eq!(journal_cutoff((2, 0)), 90);
        assert_eq!(journal_cutoff((4, 30)), 240);
        assert_eq!(journal_cutoff((0, 10)), -20);
        // A page at 00:10 ends the day at 23:40 the same evening.
        let (_, end) = journal_day_bounds(d("2026-11-19"), (0, 10), &london).unwrap();
        assert_eq!(end, ts("2026-11-19T23:40:00Z"));
        let day = journal_day_of(ts("2026-11-19T23:50:00Z"), (0, 10), &london).unwrap();
        assert_eq!(day, d("2026-11-20"));
    }

    #[test]
    fn journal_days_meet_end_to_end_across_the_clock_change() {
        let london = tz("Europe/London");
        // The clocks go back at 2:00 am on 25 October, inside the journal day of the 24th.
        let (start, end) = journal_day_bounds(d("2026-10-24"), (7, 0), &london).unwrap();
        let (next_start, _) = journal_day_bounds(d("2026-10-25"), (7, 0), &london).unwrap();
        assert_eq!(end, next_start);
        assert_eq!(end.duration_since(start).as_hours(), 25);
    }

    #[test]
    fn a_journal_day_of_an_instant_always_holds_that_instant() {
        for zone in ["Europe/London", "Europe/Paris"] {
            let zone = tz(zone);
            for page in [(2, 0), (3, 0), (7, 0)] {
                for from in ["2026-03-27T00:00:00Z", "2026-10-23T00:00:00Z"] {
                    let mut t = ts(from);
                    let stop = t + jiff::SignedDuration::from_hours(5 * 24);
                    while t < stop {
                        let day = journal_day_of(t, page, &zone).unwrap();
                        let (start, end) = journal_day_bounds(day, page, &zone).unwrap();
                        assert!(
                            start <= t && t < end,
                            "{t} is not in {day} ({start} to {end}) at page time {page:?}"
                        );
                        t += jiff::SignedDuration::from_mins(10);
                    }
                }
            }
        }
    }
}
