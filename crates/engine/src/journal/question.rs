//! The page's one question from the journal (`docs/design/journaling-follow-ups.md`, section 3).
//!
//! The rule is a set of facts about stored rows, so it needs no mind: a scheduled thing the
//! person wrote about, on the day the page looks back on, is asked about with their own
//! sentences, whole. An answer is kept on today's journal as a journal item of its own.

use std::cmp::Reverse;

use jiff::Timestamp;
use jiff::civil::Date;
use nocturne_contract::{JournalAsk, QuestionChoice};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::{Value, json};

use super::Ctx;
use super::keep::{NewJournal, insert, is_replaced};
use crate::Engine;
use crate::capture::{bump_priority, text_title};
use crate::coverage;
use crate::error::{Error, Result};
use crate::memory::{chain, is_private_chain, words_of};
use crate::rows::{MemRow, query_mems};
use crate::sensitive;
use crate::util::{enum_str, fmt_ts, parse_date, sha256_hex};

/// How many saved pages before a page are looked at: a journal question is asked only when none
/// of them had one, so at most once in seven days.
const WEEK_PAGES: u32 = 6;
/// A block counts when it was kept this many days or fewer before the day of the thing.
const BLOCK_DAYS: i64 = 30;
const MIN_WORDS: usize = 3;
const MAX_WORDS: usize = 20;

/// The memory the page asks about, the person's sentences and where they came from.
#[derive(Debug)]
pub(crate) struct Chosen {
    pub memory: MemRow,
    pub sentence: String,
    pub ask: JournalAsk,
}

/// The journal question for the page dated `page_day`, if the rule finds one and the week
/// allows it. Never on page one: the caller asks the page-time question there first.
pub(crate) fn pick(conn: &Connection, ctx: &Ctx, page_day: Date) -> Result<Option<Chosen>> {
    if asked_this_week(conn, page_day)? {
        return Ok(None);
    }
    let event = page_day.yesterday()?;
    let event_end = ctx.bounds(event)?.1;
    // Promises and deadlines (`promised`, `due`) are other predicates, followed up in Needs you.
    // The reader puts the day a scheduled thing happens in `due`, and a day it only gave as a
    // start in `valid_from`. A thing with no day at all is dated by the day its block was kept.
    let rows = query_mems(
        conn,
        "m.predicate = 'scheduled' AND m.superseded_at IS NULL AND m.status = 'active' \
         AND m.forgetting = 0 AND m.private = 0 \
         AND (COALESCE(m.due, m.valid_from) = ?1 OR COALESCE(m.due, m.valid_from) IS NULL)",
        [event.to_string()],
    )?;
    let mut found: Vec<(Reverse<String>, u32, String, Chosen)> = Vec::new();
    for m in rows {
        let undated = m.due.is_none() && m.valid_from.is_none();
        if let Some((kept_at, chosen)) = qualify(conn, ctx, &m, event, event_end, undated)? {
            let at = minutes_in(&chosen.sentence)
                .or_else(|| minutes_in(&m.object))
                .or_else(|| minutes_in(&m.sentence))
                .unwrap_or(u32::MAX);
            found.push((Reverse(kept_at), at, m.id.clone(), chosen));
        }
    }
    found.sort_by(|a, b| (&a.0, a.1, &a.2).cmp(&(&b.0, b.1, &b.2)));
    Ok(found.into_iter().next().map(|f| f.3))
}

/// Whether one of the saved pages before `page_day` (the most recent few) asked from the journal.
fn asked_this_week(conn: &Connection, page_day: Date) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM questions WHERE kind = 'journal' AND page_date IN \
         (SELECT date FROM pages WHERE date < ?1 ORDER BY date DESC LIMIT ?2))",
        params![page_day.to_string(), WEEK_PAGES],
        |r| r.get(0),
    )?)
}

/// One source of a memory: an item, and where in it the memory was read from.
struct Source {
    id: String,
    kind: String,
    private: bool,
    left_out: bool,
    kept_at: String,
    text: Option<String>,
    role: Option<String>,
    day: Option<String>,
    quote: Option<String>,
}

fn sources_of(conn: &Connection, memory_id: &str) -> Result<Vec<Source>> {
    let mut stmt = conn.prepare_cached(
        "SELECT i.id, i.kind, i.private, i.left_out, i.kept_at, i.text, \
                json_extract(i.metadata, '$.journal'), json_extract(i.metadata, '$.journal_day'), \
                s.span \
         FROM memory_sources s JOIN items i ON i.id = s.item_id WHERE s.memory_id = ?1 \
         ORDER BY i.kept_at DESC, i.id",
    )?;
    let rows = stmt.query_map([memory_id], |r| {
        let span: Option<String> = r.get(8)?;
        let quote = span
            .and_then(|s| serde_json::from_str::<nocturne_contract::Span>(&s).ok())
            .and_then(|s| match s {
                nocturne_contract::Span::Quote { text } => Some(text),
                _ => None,
            });
        Ok(Source {
            id: r.get(0)?,
            kind: r.get(1)?,
            private: r.get(2)?,
            left_out: r.get(3)?,
            kept_at: r.get(4)?,
            text: r.get(5)?,
            role: r.get(6)?,
            day: r.get(7)?,
            quote,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Whether the health, money and ID check flags the memory, in any version of it.
fn flagged(conn: &Connection, id: &str) -> Result<bool> {
    let versions = chain(conn, id)?;
    if versions.is_empty() {
        return Ok(true);
    }
    let mut read = conn.prepare_cached(
        "SELECT suggest_private, subject, object, sentence, tags FROM memories WHERE id = ?1",
    )?;
    for v in &versions {
        let (suggested, subject, object, sentence, tags): (bool, String, String, String, String) =
            read.query_row([v], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?;
        let labelled = match serde_json::from_str::<Vec<String>>(&tags) {
            Ok(tags) => tags.iter().any(|t| sensitive::tag(t)),
            Err(_) => true,
        };
        if suggested
            || labelled
            || [&subject, &object, &sentence]
                .iter()
                .any(|t| sensitive::text(t))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether the memory, in any version, was asked about on any page.
fn asked_before(conn: &Connection, id: &str) -> Result<bool> {
    for v in chain(conn, id)? {
        let asked: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM questions WHERE memory_id = ?1)",
            [&v],
            |r| r.get(0),
        )?;
        if asked {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The memory with the block it was read from and the sentences to quote, when every line of
/// the rule holds. The block's `kept_at` comes back for the pick. A memory with no day of its own
/// (`undated`) qualifies only through a block kept on the event's day.
fn qualify(
    conn: &Connection,
    ctx: &Ctx,
    m: &MemRow,
    event: Date,
    event_end: Timestamp,
    undated: bool,
) -> Result<Option<(String, Chosen)>> {
    if m.is_safety() || m.suggest_private || flagged(conn, &m.id)? {
        return Ok(None);
    }
    if is_private_chain(conn, &m.id)? || asked_before(conn, &m.id)? {
        return Ok(None);
    }
    let sources = sources_of(conn, &m.id)?;
    if sources.iter().any(|s| s.private) {
        return Ok(None);
    }
    // The newest block that holds every line.
    for s in &sources {
        if s.role.as_deref() != Some("entry") || !matches!(s.kind.as_str(), "text" | "audio") {
            continue;
        }
        if s.left_out || is_replaced(conn, &s.id)? {
            continue;
        }
        let written = match &s.day {
            Some(d) => parse_date(d)?,
            None => ctx.day_of(crate::util::parse_ts(&s.kept_at)?)?,
        };
        let back = i64::from(event.since(written)?.get_days());
        let latest = if undated { 0 } else { BLOCK_DAYS };
        if !(0..=latest).contains(&back) {
            continue;
        }
        let (Some(text), Some(quote)) = (&s.text, &s.quote) else {
            continue;
        };
        let Some(sentence) = touched(text, quote) else {
            continue;
        };
        let words = sentence.split_whitespace().count();
        if !(MIN_WORDS..=MAX_WORDS).contains(&words) || sensitive::text(&sentence) {
            continue;
        }
        if written_about(conn, m, &s.id, &s.kept_at, event_end)? {
            continue;
        }
        let ask = JournalAsk {
            item_id: s.id.clone(),
            voice: s.kind == "audio",
            written_on: written.to_string(),
            event_day: event.to_string(),
            answer_item_id: None,
            line: None,
        };
        return Ok(Some((
            s.kept_at.clone(),
            Chosen {
                memory: m.clone(),
                sentence,
                ask,
            },
        )));
    }
    Ok(None)
}

/// The sentences of `text` that `quote` touches, whole and in order, joined by a space.
/// Words are matched without case or punctuation, so the quote may start or end inside a
/// sentence and still bring the whole sentence.
fn touched(text: &str, quote: &str) -> Option<String> {
    let sentences = coverage::sentences(text);
    let wanted = words_of(quote);
    if wanted.is_empty() {
        return None;
    }
    let mut stream: Vec<(String, usize)> = Vec::new();
    for (i, s) in sentences.iter().enumerate() {
        stream.extend(words_of(s).into_iter().map(|w| (w, i)));
    }
    let last_start = stream.len().checked_sub(wanted.len())?;
    let at = (0..=last_start).find(|&i| {
        stream[i..i + wanted.len()]
            .iter()
            .map(|(w, _)| w)
            .eq(wanted.iter())
    })?;
    let (first, last) = (stream[at].1, stream[at + wanted.len() - 1].1);
    Some(sentences[first..=last].join(" "))
}

/// Whether anything kept after the block, up to the end of the event's day, is about the
/// memory's subject: a mention of the same person, place or thing (found by the reading), or the
/// subject's own words in the text. Both are needed. A mention is only there once an item has
/// been read, and a note kept moments ago, still waiting to be read, has none; the words catch
/// it. The words alone would miss a name the reading placed elsewhere ("she", a nickname the
/// person gave).
fn written_about(
    conn: &Connection,
    m: &MemRow,
    block_id: &str,
    block_kept_at: &str,
    event_end: Timestamp,
) -> Result<bool> {
    let end = fmt_ts(event_end);
    if let Some(entity) = &m.entity_id {
        let mentioned: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM mentions n JOIN items i ON i.id = n.item_id \
             WHERE n.entity_id = ?1 AND n.state != 'removed' AND i.id != ?2 \
             AND i.kept_at > ?3 AND i.kept_at < ?4)",
            params![entity, block_id, block_kept_at, end],
            |r| r.get(0),
        )?;
        if mentioned {
            return Ok(true);
        }
    }
    let key = coverage::normalise(&m.subject);
    if key.is_empty() {
        return Ok(false);
    }
    let needle = format!(" {key} ");
    let mut stmt = conn.prepare_cached(
        "SELECT text FROM items WHERE text IS NOT NULL AND id != ?1 AND kept_at > ?2 \
         AND kept_at < ?3",
    )?;
    let texts = stmt.query_map(params![block_id, block_kept_at, end], |r| {
        r.get::<_, String>(0)
    })?;
    for text in texts {
        if format!(" {} ", coverage::normalise(&text?)).contains(&needle) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The first time of day a text gives, in minutes after midnight: "9am", "7:30pm", "14:30",
/// "at 6". An hour with no am or pm, from 1 to 6, is read as the afternoon or evening.
fn minutes_in(text: &str) -> Option<u32> {
    let lower = text.to_lowercase();
    let tokens: Vec<&str> = lower
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric() && c != ':' && c != '.'))
        .collect();
    for (i, token) in tokens.iter().enumerate() {
        let (number, suffix) = match token.find(|c: char| c.is_alphabetic()) {
            Some(at) => (&token[..at], &token[at..]),
            None => (*token, ""),
        };
        let number = number.trim_end_matches('.');
        if number.is_empty()
            || !number
                .chars()
                .all(|c| c.is_ascii_digit() || c == ':' || c == '.')
        {
            continue;
        }
        let parsed = match number.split_once([':', '.']) {
            Some((h, mi)) => h.parse::<u32>().ok().zip(mi.parse::<u32>().ok()),
            None => number.parse::<u32>().ok().map(|h| (h, 0)),
        };
        let Some((h, mi)) = parsed else {
            continue;
        };
        let meridiem = match suffix {
            "am" | "pm" => suffix,
            "" => tokens
                .get(i + 1)
                .copied()
                .filter(|t| matches!(*t, "am" | "pm"))
                .unwrap_or(""),
            _ => continue,
        };
        let colon = number.contains(':');
        if meridiem.is_empty() && !colon && !tokens[..i].last().is_some_and(|t| *t == "at") {
            continue;
        }
        if h > 24 || mi > 59 {
            continue;
        }
        let hour = match meridiem {
            "pm" if h < 12 => h + 12,
            "am" if h == 12 => 0,
            "" if !colon && (1..=6).contains(&h) => h + 12,
            _ => h,
        };
        return Some(hour * 60 + mi);
    }
    None
}

/// How the question reads: the opening by how long before the day it was written, then the
/// person's sentences in quotes.
pub(crate) fn question_words(ask: &JournalAsk, sentence: &str) -> String {
    let verb = if ask.voice { "said" } else { "wrote" };
    let opening = match (parse_date(&ask.written_on), parse_date(&ask.event_day)) {
        (Ok(written), Ok(event)) => match event.since(written).map(|s| s.get_days()) {
            Ok(0) => "Yesterday".to_string(),
            Ok(1..=6) => format!("On {}", written.strftime("%A")),
            _ => format!("On {}", written.strftime("%-d %B")),
        },
        _ => "Earlier".to_string(),
    };
    format!("{opening} you {verb}: \u{201c}{sentence}\u{201d} How did it go?")
}

/// The answer kept for the question of `page_date`: its item id and the line, once added.
pub(crate) fn answer_of(
    conn: &Connection,
    page_date: &str,
) -> Result<Option<(String, Option<String>)>> {
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT id, text FROM items WHERE json_extract(metadata, '$.journal') = 'answer' \
             AND json_extract(metadata, '$.question') = ?1 ORDER BY kept_at DESC, rowid DESC \
             LIMIT 1",
            [page_date],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.map(|(id, text)| (id, text.filter(|t| !t.trim().is_empty()))))
}

/// A stored question's `journal` column, with the answer once there is one.
pub(crate) fn read_ask(
    conn: &Connection,
    page_date: &str,
    json: Option<&str>,
) -> Result<Option<JournalAsk>> {
    let Some(json) = json else {
        return Ok(None);
    };
    let mut ask: JournalAsk = serde_json::from_str(json)?;
    if let Some((id, line)) = answer_of(conn, page_date)? {
        ask.answer_item_id = Some(id);
        ask.line = line;
    }
    Ok(Some(ask))
}

/// What a question stores of where it came from.
pub(crate) fn ask_json(ask: &JournalAsk) -> String {
    json!({"itemId": ask.item_id, "voice": ask.voice, "writtenOn": ask.written_on,
           "eventDay": ask.event_day})
    .to_string()
}

/// A memory is going. The questions asked about it go with it, except a journal question: its row
/// stays with no memory and no words, because it still counts for the week and for the page.
pub(crate) fn drop_memory_questions(tx: &Transaction<'_>, memory_id: &str) -> Result<()> {
    tx.execute(
        "UPDATE questions SET memory_id = NULL, text = '' \
         WHERE memory_id = ?1 AND kind = 'journal'",
        [memory_id],
    )?;
    tx.execute("DELETE FROM questions WHERE memory_id = ?1", [memory_id])?;
    Ok(())
}

/// Items are being deleted. The sentence a journal question quoted from one of them, kept on the
/// question and on the answer given to it, goes too. The answer and the question stay, without it.
pub(crate) fn forget_sentences_about(tx: &Transaction<'_>, items: &[String]) -> Result<()> {
    for item in items {
        tx.execute(
            "UPDATE items SET metadata = json_set(metadata, '$.sentence', '') \
             WHERE json_extract(metadata, '$.journal') = 'answer' \
             AND json_extract(metadata, '$.about') = ?1",
            [item],
        )?;
        tx.execute(
            "UPDATE questions SET text = '' \
             WHERE kind = 'journal' AND json_extract(journal, '$.itemId') = ?1",
            [item],
        )?;
    }
    Ok(())
}

fn choice_title(choice: QuestionChoice) -> &'static str {
    match choice {
        QuestionChoice::Fine => "Fine",
        QuestionChoice::NotGreat => "Not great",
        _ => "Not yet",
    }
}

/// Keep the answer to a journal question on today's journal. There is nothing to read in a
/// choice alone, so the item is understood at once; a line added later makes it a note to read.
pub(crate) fn keep_answer_tx(
    tx: &Transaction<'_>,
    ctx: &Ctx,
    page_date: &str,
    ask: &JournalAsk,
    sentence: &str,
    choice: QuestionChoice,
) -> Result<String> {
    let stamp = fmt_ts(ctx.now);
    let meta: Value = json!({
        "journal": "answer", "journal_day": ctx.today()?.to_string(),
        "choice": enum_str(&choice), "about": ask.item_id, "sentence": sentence,
        "question": page_date, "read_at": stamp,
    });
    insert(
        tx,
        &stamp,
        &NewJournal {
            kind: nocturne_contract::ItemKind::Text,
            source: nocturne_contract::Source::Journal,
            title: choice_title(choice),
            text: Some(""),
            status: "understood",
            meta,
            hold_until: None,
        },
    )
}

impl Engine {
    /// Add a line to the answer given to the journal question on `day`'s page. Allowed once,
    /// and only while that page is the latest: when the next page is written it is replaced
    /// and the answer stays as it was. The line is read like an entry.
    pub fn add_answer_line(&self, day: Date, line: &str) -> Result<()> {
        let line = line.trim();
        if line.is_empty() {
            return Err(Error::invalid("a line to add cannot be empty"));
        }
        let date = day.to_string();
        let title = text_title(line);
        self.write(|tx| {
            let kind: Option<String> = tx
                .query_row(
                    "SELECT kind FROM questions WHERE page_date = ?1",
                    [&date],
                    |r| r.get(0),
                )
                .optional()?;
            if kind.as_deref() != Some("journal") {
                return Err(Error::not_found(format!(
                    "a question from the journal on {date}"
                )));
            }
            let replaced: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM pages WHERE date > ?1)",
                [&date],
                |r| r.get(0),
            )?;
            if replaced {
                return Err(Error::invalid("that page has been replaced"));
            }
            let Some((id, existing)) = answer_of(tx, &date)? else {
                return Err(Error::invalid("that question has not been answered"));
            };
            if existing.is_some() {
                return Err(Error::invalid("that answer already has a line"));
            }
            tx.execute(
                "UPDATE items SET text = ?2, title = ?3, sha256 = ?4, status = 'kept', \
                 hold_until = NULL, metadata = json_remove(metadata, '$.read_at') WHERE id = ?1",
                params![
                    id,
                    line,
                    title,
                    sha256_hex(format!("{line}\0{id}").as_bytes())
                ],
            )?;
            bump_priority(tx, &id)?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sentences_a_quote_touches_come_whole() {
        let text =
            "Long day. Parents\u{2019} evening at six. Not looking forward to it. Bed early.";
        assert_eq!(
            touched(text, "evening at six").as_deref(),
            Some("Parents\u{2019} evening at six.")
        );
        assert_eq!(
            touched(text, "At six. Not looking").as_deref(),
            Some("Parents\u{2019} evening at six. Not looking forward to it.")
        );
        assert_eq!(touched(text, "never said"), None);
        assert_eq!(touched(text, "   "), None);
    }

    #[test]
    fn times_of_day_are_read_from_the_words() {
        assert_eq!(minutes_in("Dentist at 9am, ugh"), Some(9 * 60));
        assert_eq!(minutes_in("Dinner at 7pm"), Some(19 * 60));
        assert_eq!(minutes_in("Call at 14:30."), Some(14 * 60 + 30));
        assert_eq!(minutes_in("Lunch 12 pm"), Some(12 * 60));
        assert_eq!(minutes_in("Parents evening at 6"), Some(18 * 60));
        assert_eq!(minutes_in("Twelve oranges"), None);
        assert_eq!(minutes_in("Room 6 is free"), None);
    }

    #[test]
    fn an_odd_token_is_skipped_and_the_reading_goes_on() {
        assert_eq!(minutes_in("Ref 1:2:3 then dinner at 7pm"), Some(19 * 60));
        assert_eq!(
            minutes_in("Ref 99:99 then call at 14:30"),
            Some(14 * 60 + 30)
        );
        assert_eq!(minutes_in("Version 1.2.3 and dentist at 9am"), Some(9 * 60));
        assert_eq!(minutes_in("Only 1:2:3 here"), None);
    }

    #[test]
    fn the_opening_follows_how_long_before_it_was_written() {
        let ask = |written: &str, voice: bool| JournalAsk {
            item_id: "i".into(),
            voice,
            written_on: written.into(),
            event_day: "2026-11-13".into(),
            answer_item_id: None,
            line: None,
        };
        assert_eq!(
            question_words(&ask("2026-11-13", false), "Parents\u{2019} evening at six."),
            "Yesterday you wrote: \u{201c}Parents\u{2019} evening at six.\u{201d} How did it go?"
        );
        assert_eq!(
            question_words(&ask("2026-11-13", true), "Dentist at nine."),
            "Yesterday you said: \u{201c}Dentist at nine.\u{201d} How did it go?"
        );
        assert_eq!(
            question_words(&ask("2026-11-07", false), "Dentist at nine."),
            "On Saturday you wrote: \u{201c}Dentist at nine.\u{201d} How did it go?"
        );
        assert_eq!(
            question_words(&ask("2026-11-06", true), "Dentist at nine."),
            "On 6 November you said: \u{201c}Dentist at nine.\u{201d} How did it go?"
        );
        assert_eq!(
            question_words(&ask("2026-10-14", false), "Dentist at nine."),
            "On 14 October you wrote: \u{201c}Dentist at nine.\u{201d} How did it go?"
        );
    }
}
