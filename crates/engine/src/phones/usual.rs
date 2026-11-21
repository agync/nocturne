//! "Your usual": the middle half of the person's own readings over the last 60 days.

use jiff::civil::Date;
use nocturne_contract::Band;

/// Days looked back over, today included.
pub const USUAL_DAYS: i64 = 60;
/// Days with a reading needed before there is a usual.
pub const USUAL_MIN_DAYS: usize = 14;

/// The value at `q` (0 to 1) of sorted values, by linear interpolation between the two
/// nearest ranks.
fn quantile(sorted: &[f64], q: f64) -> f64 {
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

/// The band between the lowest and highest quarter of the values from the 60 days ending
/// `today`, once at least 14 of those days have one. Deterministic for the same input.
pub fn band(values: &[(Date, f64)], today: Date) -> Option<Band> {
    let first = today
        .checked_sub(jiff::Span::new().days(USUAL_DAYS - 1))
        .ok()?;
    let mut inside: Vec<(Date, f64)> = values
        .iter()
        .copied()
        .filter(|(d, v)| *d >= first && *d <= today && v.is_finite())
        .collect();
    let mut days: Vec<Date> = inside.iter().map(|(d, _)| *d).collect();
    days.sort();
    days.dedup();
    if days.len() < USUAL_MIN_DAYS {
        return None;
    }
    inside.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    let sorted: Vec<f64> = inside.into_iter().map(|(_, v)| v).collect();
    Some(Band {
        low: quantile(&sorted, 0.25),
        high: quantile(&sorted, 0.75),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn days(n: usize, today: Date) -> Vec<(Date, f64)> {
        (0..n)
            .map(|i| {
                let d = today.checked_sub(jiff::Span::new().days(i as i64)).unwrap();
                (d, (i + 1) as f64)
            })
            .collect()
    }

    #[test]
    fn thirteen_days_give_no_usual() {
        let today: Date = "2026-11-21".parse().unwrap();
        assert_eq!(band(&days(13, today), today), None);
        assert!(band(&days(14, today), today).is_some());
    }

    #[test]
    fn quartile_edges_interpolate() {
        let today: Date = "2026-11-21".parse().unwrap();
        // Values 1 to 17: the quarter points fall on ranks 4 and 12, exactly 5 and 13.
        let b = band(&days(17, today), today).unwrap();
        assert_eq!((b.low, b.high), (5.0, 13.0));
        // Values 1 to 14: ranks 3.25 and 9.75, between neighbours.
        let b = band(&days(14, today), today).unwrap();
        assert_eq!((b.low, b.high), (4.25, 10.75));
    }

    #[test]
    fn only_the_last_sixty_days_count() {
        let today: Date = "2026-11-21".parse().unwrap();
        let mut v = days(14, today);
        let old = today.checked_sub(jiff::Span::new().days(60)).unwrap();
        v.push((old, 1000.0));
        let b = band(&v, today).unwrap();
        assert_eq!(b.high, 10.75, "a day 61 days back is left out");
    }

    #[test]
    fn same_input_same_band() {
        let today: Date = "2026-11-21".parse().unwrap();
        let mut v = days(20, today);
        let a = band(&v, today);
        v.reverse();
        assert_eq!(a, band(&v, today));
    }
}
