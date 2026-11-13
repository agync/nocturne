//! Small helpers: ids, timestamps, keys, enum encoding.

use jiff::Timestamp;
use jiff::civil::Date;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

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

/// Casefold, trim, strip a leading "the", "a", "an" or "my", collapse spaces.
pub(crate) fn key_of(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut words: Vec<&str> = lower.split_whitespace().collect();
    while words.len() > 1 && matches!(words[0], "the" | "a" | "an" | "my") {
        words.remove(0);
    }
    words.join(" ")
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
fn fts_words(q: &str) -> Vec<String> {
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

/// An FTS5 query where any word may match (ranking sorts out relevance), stop words dropped.
pub(crate) fn fts_or(q: &str) -> Option<String> {
    let mut words = fts_words(q);
    let all = words.clone();
    words.retain(|w| !STOPWORDS.contains(&w.as_str()));
    if words.is_empty() {
        words = all;
    }
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
