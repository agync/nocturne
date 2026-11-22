//! Small helpers: ids, timestamps, keys, enum encoding.

use jiff::Timestamp;
use jiff::civil::Date;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::error::{Error, Result};

pub(crate) fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("the operating system provides random bytes");
    hex(&buf)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0xf) as usize] as char);
    }
    out
}

/// `itm_`, `mem_`, `ent_`, `cfl_`, `pln_` plus 16 lowercase hex characters.
pub(crate) fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", random_hex(8))
}

/// A 128-bit random token as 32 hex characters.
pub(crate) fn new_id_hex() -> String {
    random_hex(16)
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Fixed-width UTC text, so stored instants sort as strings.
pub(crate) fn fmt_ts(ts: Timestamp) -> String {
    format!("{ts:.3}")
}

pub(crate) fn parse_ts(s: &str) -> Result<Timestamp> {
    s.parse::<Timestamp>().map_err(Error::from)
}

pub(crate) fn parse_date(s: &str) -> Result<Date> {
    s.parse::<Date>().map_err(Error::from)
}

/// The key two sayings of the same thing share: casefolded, with spaces collapsed, edge
/// punctuation and a leading "the", "a", "an" or "my" removed, and "I", "me" and "myself" as one.
/// The store dedupes with it and forgotten claims are matched with it, so they stay in step.
pub(crate) fn key_of(s: &str) -> String {
    let lower = s.nfc().collect::<String>().to_lowercase();
    let mut words: Vec<&str> = lower
        .split_whitespace()
        .map(|w| w.trim_matches(is_edge_punctuation))
        .filter(|w| !w.is_empty())
        .collect();
    while words.len() > 1 && matches!(words[0], "the" | "a" | "an" | "my") {
        words.remove(0);
    }
    if matches!(words.as_slice(), ["i"] | ["me"] | ["myself"]) {
        return "me".to_string();
    }
    words.join(" ")
}

fn is_edge_punctuation(c: char) -> bool {
    matches!(
        c,
        '.' | ','
            | ';'
            | ':'
            | '!'
            | '?'
            | '"'
            | '\''
            | '('
            | ')'
            | '\u{2018}'
            | '\u{2019}'
            | '\u{201c}'
            | '\u{201d}'
    )
}

/// What a claim is, without its words: a hash of the keys of its subject, predicate and object.
/// Callers pass already-keyed subject and object (`key_of`).
pub(crate) fn claim_hash(subject_key: &str, predicate: &str, object_key: &str) -> String {
    let predicate = predicate.trim().to_lowercase();
    sha256_hex(format!("{subject_key}\u{1}{predicate}\u{1}{object_key}").as_bytes())
}

/// Encode a unit-variant enum the way serde does (camelCase through the contract).
pub(crate) fn enum_str<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}

pub(crate) fn parse_enum<T: DeserializeOwned>(s: &str) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(s.to_string()))
        .map_err(|_| Error::invalid(format!("unknown value {s:?}")))
}

pub(crate) fn opt_enum<T: DeserializeOwned>(s: Option<String>) -> Result<Option<T>> {
    s.map(|s| parse_enum(&s)).transpose()
}

/// Words worth searching for: lowercase alphanumeric runs of two or more characters.
pub(crate) fn fts_words(q: &str) -> Vec<String> {
    q.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 2)
        .map(str::to_string)
        .collect()
}

/// An FTS5 query where every word must match, the last as a prefix (search as you type).
pub(crate) fn fts_and(q: &str) -> Option<String> {
    let words = fts_words(q);
    if words.is_empty() {
        return None;
    }
    let last = words.len() - 1;
    Some(
        words
            .iter()
            .enumerate()
            .map(|(i, w)| {
                if i == last {
                    format!("\"{w}\"*")
                } else {
                    format!("\"{w}\"")
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    )
}

const STOPWORDS: &[&str] = &[
    "the", "is", "are", "was", "were", "my", "of", "to", "in", "on", "at", "and", "or", "a", "an",
    "when", "what", "who", "where", "which", "do", "does", "did", "for", "with", "it", "this",
    "that", "me", "i", "about", "how",
];

/// The words of a query that count: stop words dropped, unless nothing else is left.
pub(crate) fn fts_terms(q: &str) -> Vec<String> {
    let mut words = fts_words(q);
    let all = words.clone();
    words.retain(|w| !STOPWORDS.contains(&w.as_str()));
    if words.is_empty() { all } else { words }
}

/// An FTS5 query where any word may match (ranking sorts out relevance), stop words dropped.
pub(crate) fn fts_or(q: &str) -> Option<String> {
    let words = fts_terms(q);
    if words.is_empty() {
        return None;
    }
    Some(
        words
            .iter()
            .map(|w| format!("\"{w}\""))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

/// Half-open validity intervals `[from, to)` over ISO dates; `None` is unbounded.
pub(crate) fn overlaps(
    a_from: Option<&str>,
    a_to: Option<&str>,
    b_from: Option<&str>,
    b_to: Option<&str>,
) -> bool {
    let a_starts_before_b_ends = match (a_from, b_to) {
        (Some(a), Some(b)) => a < b,
        _ => true,
    };
    let b_starts_before_a_ends = match (b_from, a_to) {
        (Some(b), Some(a)) => b < a,
        _ => true,
    };
    a_starts_before_b_ends && b_starts_before_a_ends
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_source_round_trips() {
        use nocturne_contract::Source;
        assert_eq!(enum_str(&Source::Phone), "phone");
        assert_eq!(parse_enum::<Source>("phone").unwrap(), Source::Phone);
    }

    #[test]
    fn remote_ledger_kinds_round_trip() {
        use nocturne_contract::LedgerKind;
        for (kind, text) in [
            (LedgerKind::RemoteHelper, "remoteHelper"),
            (LedgerKind::RemoteOnOff, "remoteOnOff"),
            (LedgerKind::OnlineOnOff, "onlineOnOff"),
            (LedgerKind::OnlineConnected, "onlineConnected"),
            (LedgerKind::ModelDownload, "modelDownload"),
        ] {
            assert_eq!(enum_str(&kind), text);
            assert_eq!(parse_enum::<LedgerKind>(text).unwrap(), kind);
        }
    }

    #[test]
    fn online_and_model_values_round_trip() {
        use nocturne_contract::{LocalReason, NoticeCode, SendKind};
        for (code, text) in [
            (NoticeCode::OnlineKeyRefused, "onlineKeyRefused"),
            (NoticeCode::OnlineNoCredit, "onlineNoCredit"),
            (NoticeCode::OnlineLimit, "onlineLimit"),
            (NoticeCode::ModelDownloadStopped, "modelDownloadStopped"),
        ] {
            assert_eq!(enum_str(&code), text);
            assert_eq!(parse_enum::<NoticeCode>(text).unwrap(), code);
        }
        for (kind, text) in [(SendKind::Question, "question"), (SendKind::Check, "check")] {
            assert_eq!(enum_str(&kind), text);
            assert_eq!(parse_enum::<SendKind>(text).unwrap(), kind);
        }
        for (reason, text) in [
            (LocalReason::Chosen, "chosen"),
            (LocalReason::NoInternet, "noInternet"),
            (LocalReason::CouldntReach, "couldntReach"),
            (LocalReason::Slow, "slow"),
            (LocalReason::Limit, "limit"),
            (LocalReason::NoCredit, "noCredit"),
            (LocalReason::KeyRefused, "keyRefused"),
            (LocalReason::KeptHere, "keptHere"),
            (LocalReason::Problem, "problem"),
            (LocalReason::Downloading, "downloading"),
        ] {
            assert_eq!(enum_str(&reason), text);
            assert_eq!(parse_enum::<LocalReason>(text).unwrap(), reason);
        }
    }

    #[test]
    fn keys_strip_leading_articles() {
        assert_eq!(
            key_of("  The   Dentist appointment "),
            "dentist appointment"
        );
        assert_eq!(key_of("my the Cat"), "cat");
        assert_eq!(key_of("The"), "the");
    }

    #[test]
    fn timestamps_have_fixed_width() {
        let a = fmt_ts("2026-11-13T07:00:00Z".parse().unwrap());
        let b = fmt_ts("2026-11-13T07:00:00.123Z".parse().unwrap());
        assert_eq!(a.len(), b.len());
        assert!(a < b);
        assert_eq!(
            parse_ts(&a).unwrap(),
            "2026-11-13T07:00:00Z".parse().unwrap()
        );
    }

    #[test]
    fn intervals_are_half_open() {
        assert!(overlaps(None, None, Some("2026-01-01"), None));
        assert!(!overlaps(
            None,
            Some("2026-01-01"),
            Some("2026-01-01"),
            None
        ));
        assert!(overlaps(
            Some("2026-01-01"),
            Some("2026-02-01"),
            Some("2026-01-31"),
            None
        ));
    }
}
