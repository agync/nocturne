//! Finding what was kept by its words, with no mind involved.
//!
//! [`Engine::retrieve`] and the source lines under it search what a mind has read. A phone that
//! keeps on its own has no mind, so its notes stay unread; this searches the originals
//! themselves through the stemmed full-text index, which the store fills when an item is
//! written. Only the plainest rules apply: a check-in is never found, nor an item being
//! forgotten, a reading from a phone, an earlier version that a later one replaced, or an
//! appointment its calendar dropped. A private item is found and says so: the caller decides
//! what to show, and on a phone alone the person's own phone is home.

use nocturne_contract::{ItemKind, Receipt, Source, Trust};
use rusqlite::Connection;

use crate::Engine;
use crate::error::Result;
use crate::rows::de;
use crate::util::fts_or;

/// The most things one search returns.
pub const FIND_CAP: usize = 30;

/// Words either side of a match in the excerpt.
const EXCERPT_WORDS: u32 = 24;
const MARK_OPEN: char = '\u{1}';
const MARK_CLOSE: char = '\u{2}';

/// Which originals a search may offer.
const FIND_RULES: &str = "i.kind NOT IN ('checkin', 'reading') \
     AND COALESCE(json_extract(i.metadata, '$.journal'), '') != 'checkin' \
     AND i.forget_epoch = 0 \
     AND NOT EXISTS (SELECT 1 FROM forgotten_claims c WHERE c.item_id = i.id) \
     AND NOT EXISTS (SELECT 1 FROM forget_jobs j WHERE instr(j.items, '\"' || i.id || '\"') > 0) \
     AND NOT EXISTS (SELECT 1 FROM items n WHERE json_extract(n.metadata, '$.replaces') = i.id) \
     AND (i.kind != 'calendar' OR EXISTS \
         (SELECT 1 FROM phone_records r WHERE r.item_id = i.id AND r.removed_at IS NULL))";

/// Every thing the rules allow.
const ANY: &str = "1";
/// Only what waits to be read.
const UNREAD: &str = "i.status IN ('kept', 'reading')";

/// One stretch of an excerpt, and whether a searched word is in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcerptPart {
    pub text: String,
    pub matched: bool,
}

/// A kept thing found by its words.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundItem {
    /// The chip that names the original: what it is, when it was kept, from which phone.
    pub receipt: Receipt,
    /// The words around the match, split so the match can be marked. Parts run together to
    /// make the excerpt; an excerpt that starts or ends part way through the original has
    /// an ellipsis there.
    pub excerpt: Vec<ExcerptPart>,
    pub private: bool,
}

impl From<FoundItem> for nocturne_contract::FoundItem {
    fn from(found: FoundItem) -> Self {
        Self {
            receipt: found.receipt,
            excerpt: found
                .excerpt
                .into_iter()
                .map(|p| nocturne_contract::ExcerptPart {
                    text: p.text,
                    matched: p.matched,
                })
                .collect(),
            private: found.private,
        }
    }
}

impl Engine {
    /// The kept things whose words match `query`, best match first and the newest first among
    /// equals, at most `limit` and never more than [`FIND_CAP`]. A query with no words in it
    /// finds nothing.
    pub fn find_items(&self, query: &str, limit: usize) -> Result<Vec<FoundItem>> {
        self.find_where(query, limit, ANY)
    }

    /// [`Engine::find_items`], only the things no mind has read yet: what a search of what
    /// Nocturne remembers cannot find until then, such as things just brought in from a phone.
    pub fn find_unread(&self, query: &str, limit: usize) -> Result<Vec<FoundItem>> {
        self.find_where(query, limit, UNREAD)
    }

    fn find_where(&self, query: &str, limit: usize, only: &str) -> Result<Vec<FoundItem>> {
        let limit = limit.min(FIND_CAP);
        let Some(q) = fts_or(query).filter(|_| limit > 0) else {
            return Ok(Vec::new());
        };
        // Only the person's own words: a mind's summary of an item is not what was kept.
        let q = format!("{{title text}} : ({q})");
        self.read(|conn| {
            let mut found = find(conn, &q, limit, only)?;
            if found.len() < limit {
                let have: Vec<String> = found.iter().map(|f| f.receipt.item_id.clone()).collect();
                found.extend(by_place(conn, query, limit - found.len(), &have, only)?);
            }
            Ok(found)
        })
    }
}

fn find(conn: &Connection, q: &str, limit: usize, only: &str) -> Result<Vec<FoundItem>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT i.id, i.kind, i.source, i.title, i.kept_at, i.duration_ms, i.trust, i.private, \
         COALESCE((SELECT rd.name FROM remote_keeps rk JOIN remote_devices rd \
          ON rd.id = rk.device_id AND rd.connection != 'local' WHERE rk.item_id = i.id), json_extract(i.metadata, '$.keptOn')), \
         snippet(items_stem, -1, char({open}), char({close}), '…', {words}) \
         FROM items_stem f JOIN items i ON i.rowid = f.rowid \
         WHERE items_stem MATCH ?1 AND {FIND_RULES} AND {only} \
         ORDER BY f.rank, i.kept_at DESC, i.id LIMIT ?2",
        open = MARK_OPEN as u32,
        close = MARK_CLOSE as u32,
        words = EXCERPT_WORDS,
    ))?;
    let rows = stmt.query_map(rusqlite::params![q, limit as i64], |r| {
        let kind: ItemKind = de(r.get(1)?)?;
        let source: Source = de(r.get(2)?)?;
        let trust: Trust = de(r.get(6)?)?;
        let excerpt: String = r.get(9)?;
        Ok(FoundItem {
            receipt: Receipt {
                item_id: r.get(0)?,
                kind,
                source,
                label: r.get(3)?,
                kept_at: r.get(4)?,
                duration_ms: r.get(5)?,
                span: None,
                unconfirmed: trust == Trust::Untrusted,
                from_device: r.get(8)?,
                calendar: None,
            },
            excerpt: parts(&excerpt),
            private: r.get(7)?,
        })
    })?;
    let mut found: Vec<FoundItem> = rows.collect::<rusqlite::Result<_>>()?;
    for f in &mut found {
        if f.receipt.kind == ItemKind::Calendar {
            f.receipt.calendar = crate::phones::read::calendar_of(conn, &f.receipt.item_id)?;
        }
    }
    Ok(found)
}

/// Appointments with a word in their place that starts with one of the words searched. The
/// place is kept beside the title, not in the indexed text, so it is matched as typed, without
/// word endings, and stop words do not count.
fn by_place(
    conn: &Connection,
    query: &str,
    limit: usize,
    have: &[String],
    only: &str,
) -> Result<Vec<FoundItem>> {
    let words = crate::util::fts_terms(query);
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT i.id, i.title, i.kept_at, i.duration_ms, i.trust, i.private, \
         json_extract(i.metadata, '$.entry.place'), \
         COALESCE((SELECT rd.name FROM remote_keeps rk JOIN remote_devices rd \
          ON rd.id = rk.device_id AND rd.connection != 'local' WHERE rk.item_id = i.id), json_extract(i.metadata, '$.keptOn')) \
         FROM items i WHERE i.kind = 'calendar' \
         AND json_extract(i.metadata, '$.entry.place') IS NOT NULL AND {FIND_RULES} AND {only} \
         ORDER BY i.kept_at DESC, i.id"
    ))?;
    let rows = stmt.query_map([], |r| {
        let trust: Trust = de(r.get(4)?)?;
        Ok((
            FoundItem {
                receipt: Receipt {
                    item_id: r.get(0)?,
                    kind: ItemKind::Calendar,
                    source: Source::Phone,
                    label: r.get(1)?,
                    kept_at: r.get(2)?,
                    duration_ms: r.get(3)?,
                    span: None,
                    unconfirmed: trust == Trust::Untrusted,
                    from_device: r.get(7)?,
                    calendar: None,
                },
                excerpt: Vec::new(),
                private: r.get(5)?,
            },
            r.get::<_, String>(6)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (mut item, place) = row?;
        if have.contains(&item.receipt.item_id) {
            continue;
        }
        let lower = place.to_lowercase();
        let Some((at, len)) = place_match(&lower, &words) else {
            continue;
        };
        // Lowercasing that moved byte offsets would mark the wrong letters: show it unmarked.
        item.excerpt = if lower.len() == place.len() {
            marked_place(&place, at, len)
        } else {
            vec![ExcerptPart {
                text: place,
                matched: false,
            }]
        };
        item.receipt.calendar = crate::phones::read::calendar_of(conn, &item.receipt.item_id)?;
        out.push(item);
        if out.len() == limit {
            break;
        }
    }
    Ok(out)
}

/// The first word of `lower` that starts with one of `words`, as its offset and length.
fn place_match(lower: &str, words: &[String]) -> Option<(usize, usize)> {
    let mut at = None;
    for (i, c) in lower
        .char_indices()
        .chain(std::iter::once((lower.len(), ' ')))
    {
        match (c.is_alphanumeric(), at) {
            (true, None) => at = Some(i),
            (false, Some(start)) => {
                let word = &lower[start..i];
                if words.iter().any(|w| word.starts_with(w.as_str())) {
                    return Some((start, word.len()));
                }
                at = None;
            }
            _ => {}
        }
    }
    None
}

/// A place with the match at `at`, `len` bytes long, marked. `at` and `len` come from the
/// lowercased place; where lowercasing moved a character boundary the whole place is unmarked.
fn marked_place(place: &str, at: usize, len: usize) -> Vec<ExcerptPart> {
    let (Some(before), Some(hit), Some(after)) = (
        place.get(..at),
        place.get(at..at + len),
        place.get(at + len..),
    ) else {
        return vec![ExcerptPart {
            text: place.to_string(),
            matched: false,
        }];
    };
    [(before, false), (hit, true), (after, false)]
        .into_iter()
        .filter(|(t, _)| !t.is_empty())
        .map(|(t, matched)| ExcerptPart {
            text: t.to_string(),
            matched,
        })
        .collect()
}

/// Split an excerpt marked with [`MARK_OPEN`] and [`MARK_CLOSE`] into its parts. Line breaks
/// become spaces. A stray marker in what was kept only moves a mark, never loses a word.
fn parts(marked: &str) -> Vec<ExcerptPart> {
    let mut out: Vec<ExcerptPart> = Vec::new();
    let mut cur = String::new();
    let mut matched = false;
    let mut flush = |cur: &mut String, matched: bool| {
        if !cur.is_empty() {
            out.push(ExcerptPart {
                text: std::mem::take(cur),
                matched,
            });
        }
    };
    for c in marked.chars() {
        match c {
            MARK_OPEN => {
                flush(&mut cur, matched);
                matched = true;
            }
            MARK_CLOSE => {
                flush(&mut cur, matched);
                matched = false;
            }
            '\n' | '\r' | '\t' => cur.push(' '),
            c => cur.push(c),
        }
    }
    flush(&mut cur, matched);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_split_on_marks() {
        let p = parts("…saw the \u{1}plumber\u{2} on\nTuesday…");
        let words: Vec<(&str, bool)> = p.iter().map(|p| (p.text.as_str(), p.matched)).collect();
        assert_eq!(
            words,
            [
                ("…saw the ", false),
                ("plumber", true),
                (" on Tuesday…", false)
            ]
        );
    }

    #[test]
    fn parts_of_nothing_is_nothing() {
        assert!(parts("").is_empty());
    }
}
