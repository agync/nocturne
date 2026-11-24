//! The monthly limit: questions, or money when the provider reports cost. The month is the
//! calendar month in the person's time zone, and a limit reached lifts on the 1st.

use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};
use nocturne_contract::{LimitKind, OnlineLimit, OnlinePause};

/// Whether this month's use has reached `limit`. A money limit holds only while the provider
/// reports cost on its answers; when it stops, the question count limits instead, so limiting
/// never silently stops.
pub fn reached(
    limit: &OnlineLimit,
    questions: u32,
    spent: Option<f64>,
    cost_reported: bool,
) -> bool {
    match limit.kind {
        LimitKind::Questions => questions >= limit.questions,
        LimitKind::Usd => match (limit.usd, spent) {
            (Some(cap), Some(spent)) if cost_reported => spent >= cap,
            _ => questions >= limit.questions,
        },
        LimitKind::None => false,
    }
}

/// Midnight at the start of next month, local, as RFC 3339.
pub fn next_month_start(now: Timestamp, tz: &TimeZone) -> String {
    let first = now.to_zoned_datetime(tz.clone()).date().first_of_month();
    first
        .checked_add(1.month())
        .and_then(|d| d.to_zoned_datetime(tz.clone()))
        .map(|z| z.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string())
        .unwrap_or_default()
}

/// Whether a limit pause is over: its month has ended.
pub fn lifted(pause: &OnlinePause, now: Timestamp) -> bool {
    match pause {
        OnlinePause::Limit { until } => until
            .parse::<jiff::ZonedDateTime>()
            .map(|z| now >= z.timestamp())
            .or_else(|_| until.parse::<Timestamp>().map(|t| now >= t))
            .unwrap_or(true),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_turns_with_the_local_month() {
        let tz = TimeZone::get("Europe/London").unwrap();
        let now: Timestamp = "2026-11-30T23:30:00Z".parse().unwrap();
        let until = next_month_start(now, &tz);
        assert_eq!(until, "2026-12-01T00:00:00+00:00");
        let pause = OnlinePause::Limit { until };
        assert!(!lifted(&pause, now));
        assert!(lifted(&pause, "2026-12-01T00:00:00Z".parse().unwrap()));
        assert!(!lifted(&OnlinePause::KeyRefused, now));
    }

    #[test]
    fn a_limit_of_money_falls_back_to_the_count_without_cost() {
        let usd = OnlineLimit {
            kind: LimitKind::Usd,
            questions: 200,
            usd: Some(1.0),
        };
        assert!(reached(&usd, 0, Some(1.0), true));
        assert!(!reached(&usd, 10, Some(0.5), true));
        assert!(
            reached(&usd, 200, Some(0.5), false),
            "no cost reported: the question count limits instead"
        );
        assert!(!reached(&usd, 199, None, false));
    }

    #[test]
    fn there_is_no_limit_unless_one_is_set() {
        let none = OnlineLimit::default();
        assert_eq!(none.kind, LimitKind::None);
        assert!(!reached(&none, 0, None, false));
        assert!(!reached(&none, 1_000_000, Some(500.0), true));
        let count = OnlineLimit {
            kind: LimitKind::Questions,
            questions: 200,
            usd: None,
        };
        assert!(reached(&count, 200, None, false));
        assert!(!reached(&count, 199, None, false));
    }
}
