//! Candidates become memories: reinforcement, trust and corroboration, contradictions, history,
//! corrections, and the entries that wait on the person.

use std::collections::HashSet;

use nocturne_contract::{
    EntityKind, EntityView, MemoryFilter, MemoryStatus, MemoryView, NeedsYouEntry, Span,
    WhichOption,
};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::Engine;
use crate::audio::pages_citing;
use crate::capture::check_lease;
use crate::entity::{Alias, entity_views, find_by_key, note_rewrite, replace_name, resolve_alias};
use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::journal::mentions::{Found, Routed, route, write_mentions};
use crate::rows::{MEM_COLS, MemRow, load_mem, mem_row, mem_view, query_mems, receipts_for};
use crate::types::{ApplyReport, Candidate, CorrectPatch, Predicate};
use crate::util::{claim_hash, enum_str, fmt_ts, fts_and, key_of, new_id, overlaps};

/// A real-world change (moved house, new dose) closes the old row at the new row's start date.
fn closes_old_value(predicate: &str) -> bool {
    matches!(predicate, "lives_in" | "takes" | "prefers")
}

/// Two items kept closer together than this (a folder dropped at once, two notes typed in a
/// row) cannot be put in order, so what they disagree on is asked about.
const ORDER_GAP: jiff::SignedDuration = jiff::SignedDuration::from_mins(5);

/// Which of two items was kept first, when that is clear.
pub(crate) fn kept_order(new: &str, old: &str) -> Option<std::cmp::Ordering> {
    let (Ok(new), Ok(old)) = (
        new.parse::<jiff::Timestamp>(),
        old.parse::<jiff::Timestamp>(),
    ) else {
        return None;
    };
    let gap = new.duration_since(old);
    if gap >= ORDER_GAP {
        Some(std::cmp::Ordering::Greater)
    } else if gap <= -ORDER_GAP {
        Some(std::cmp::Ordering::Less)
    } else {
        None
    }
}

/// When the newest trusted item behind an existing memory was kept. Only trusted items put
/// values in order: an untrusted one agreeing late with an old value does not make it newer.
fn last_kept(tx: &Transaction<'_>, memory: &str) -> Result<Option<String>> {
    Ok(tx.query_row(
        "SELECT MAX(i.kept_at) FROM memory_sources s JOIN items i ON i.id = s.item_id \
         WHERE s.memory_id = ?1 AND i.trust = 'trusted'",
        [memory],
        |r| r.get(0),
    )?)
}

/// Words that say a value replaces an earlier one ("it has moved to Thursday"). Not "now":
/// "I now also have" adds an appointment.
const MOVE_WORDS: [&str; 5] = ["moved", "changed", "instead", "rescheduled", "postponed"];

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

pub(crate) fn words_of(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether the item says this candidate replaces an earlier value: the mind marked it so, or the
/// words it quotes say so. The reader's own sentence saying it moved counts only when the quote
/// itself has a move word, or "now" with the subject or a day or time: a paraphrase alone is not
/// the item's word.
fn says_moved(c: &Candidate) -> bool {
    let date = date_str(c.due.or(c.valid_from));
    let quote = match &c.span {
        Some(Span::Quote { text }) => Some(text.as_str()),
        _ => None,
    };
    let cue = quote.is_some_and(|q| {
        let words = words_of(q);
        has_move_word(&words)
            || (words.iter().any(|w| w == "now") && (names_all(q, &c.subject) || names_a_value(q)))
    });
    let sentence = Span::Quote {
        text: c.sentence.clone(),
    };
    c.moved
        || c.span
            .as_ref()
            .is_some_and(|s| quote_says_moved(s, &c.object, date.as_deref(), &c.subject))
        || (cue && quote_says_moved(&sentence, &c.object, date.as_deref(), &c.subject))
}

/// Whether `words` holds a word that says something moved.
fn has_move_word(words: &[String]) -> bool {
    words.iter().any(|w| MOVE_WORDS.contains(&w.as_str()))
        || words
            .windows(2)
            .any(|p| p[0] == "brought" && p[1] == "forward")
}

/// The sentences of a quote, as words.
fn quote_sentences(text: &str) -> Vec<Vec<String>> {
    text.split(['.', '!', '?', ';', '\n'])
        .map(words_of)
        .filter(|w| !w.is_empty())
        .collect()
}

/// Whether `words` gives the value `object` (its words in a row) or the day and month of `date`.
fn gives_value(words: &[String], object: &str, date: Option<&str>) -> bool {
    let value = words_of(object);
    let day_month: Option<(String, &str)> = date.and_then(|d| {
        let d: jiff::civil::Date = d.parse().ok()?;
        Some((
            d.day().to_string(),
            MONTHS[usize::try_from(d.month()).ok()? - 1],
        ))
    });
    (!value.is_empty() && words.windows(value.len()).any(|w| w == value))
        || day_month
            .as_ref()
            .is_some_and(|(day, month)| words.contains(day) && words.iter().any(|w| w == month))
}

/// A move word counts only in the sentence of the quote that gives the new value (its words, or
/// the day and month of its date), or in the sentence just before it when that one names the
/// whole subject and the value's own sentence speaks of it as "it": "The dentist appointment has
/// moved. It is now Thursday 19 November." But "The dentist moved to the High Street. Hygienist
/// 3 December" does not move the dentist.
fn quote_says_moved(span: &Span, object: &str, date: Option<&str>, subject: &str) -> bool {
    let Span::Quote { text } = span else {
        return false;
    };
    let sentences = quote_sentences(text);
    let naming = naming_words(subject);
    sentences.iter().enumerate().any(|(at, words)| {
        if !gives_value(words, object, date) {
            return false;
        }
        has_move_word(words)
            || (at > 0 && words.iter().any(|w| w == "it") && {
                let before = &sentences[at - 1];
                has_move_word(before)
                    && !naming.is_empty()
                    && naming.iter().all(|w| before.contains(w))
            })
    })
}

/// Whether `text`, in the sentence that gives the new value (`object`, or the day and month of
/// `date`), says the old day no longer holds: "it is now Thursday 19 November at 14:30, not
/// Wednesday 18 November". The old day is the day and month of `old_date`, after "not", "no
/// longer" or "instead of" in that same sentence.
fn cancels_in(text: &str, object: &str, date: Option<&str>, old_date: &str) -> bool {
    let Ok(old) = old_date.parse::<jiff::civil::Date>() else {
        return false;
    };
    let Some(month) = usize::try_from(old.month())
        .ok()
        .and_then(|m| MONTHS.get(m - 1))
    else {
        return false;
    };
    let day = old.day().to_string();
    quote_sentences(text).iter().any(|words| {
        if !gives_value(words, object, date) {
            return false;
        }
        (0..words.len()).any(|i| {
            let after = match (words[i].as_str(), words.get(i + 1).map(String::as_str)) {
                ("not", _) => i + 1,
                ("no", Some("longer")) | ("instead", Some("of")) => i + 2,
                _ => return false,
            };
            let rest = &words[after.min(words.len())..];
            rest.contains(&day) && rest.iter().any(|w| w == month)
        })
    })
}

/// Whether the candidate's quote says `e`'s day, filed under the same subject, no longer holds.
fn quote_cancels(c: &Candidate, e: &MemRow) -> bool {
    let (Some(Span::Quote { text }), Some(old)) =
        (&c.span, e.due.as_deref().or(e.valid_from.as_deref()))
    else {
        return false;
    };
    let date = date_str(c.due.or(c.valid_from));
    key_of(&c.subject) == e.subject_key && cancels_in(text, &c.object, date.as_deref(), old)
}

/// The concrete values a phrase pins down, by kind. Each value lists the forms it may take:
/// "2:30" with no am or pm may be 02:30 or 14:30.
#[derive(Default)]
struct Values {
    weekdays: Vec<Vec<String>>,
    months: Vec<Vec<String>>,
    ordinals: Vec<Vec<String>>,
    times: Vec<Vec<String>>,
    numbers: Vec<Vec<String>>,
}

#[derive(Clone, Copy)]
enum ValueKind {
    Weekday,
    Month,
    Ordinal,
    Time,
    Number,
}

/// One word read as a value. A weak one ("sat", "first", "may") counts only next to a date or
/// time word: "Sat 14 November", "first Tuesday", but not "the sat nav" or "first prize".
struct Read {
    kind: ValueKind,
    forms: Vec<String>,
    weak: bool,
}

const WEEKDAYS: [(&str, &[&str]); 7] = [
    ("monday", &["mon", "monday", "mondays"]),
    ("tuesday", &["tue", "tues", "tuesday", "tuesdays"]),
    ("wednesday", &["wed", "wednesday", "wednesdays"]),
    (
        "thursday",
        &["thu", "thur", "thurs", "thursday", "thursdays"],
    ),
    ("friday", &["fri", "friday", "fridays"]),
    ("saturday", &["sat", "saturday", "saturdays"]),
    ("sunday", &["sun", "sunday", "sundays"]),
];

const MONTH_FORMS: [&[&str]; 12] = [
    &["jan", "january"],
    &["feb", "february"],
    &["mar", "march"],
    &["apr", "april"],
    &["may"],
    &["jun", "june"],
    &["jul", "july"],
    &["aug", "august"],
    &["sep", "sept", "september"],
    &["oct", "october"],
    &["nov", "november"],
    &["dec", "december"],
];

const ORDINALS: [&str; 5] = ["first", "second", "third", "fourth", "fifth"];

/// Words that are also everyday words, so they are values only in a date or time phrase.
const WEAK_WORDS: [&str; 7] = ["sat", "sun", "wed", "first", "second", "may", "march"];

/// Words that say a number is money: "30 pounds".
const MONEY_WORDS: [&str; 9] = [
    "pound", "pounds", "pence", "p", "quid", "euro", "euros", "dollar", "dollars",
];

/// A word of the phrase. `money` when a currency sign comes right before it: "£12.50".
struct Token {
    text: String,
    money: bool,
}

/// Lowercase words, keeping "14:30", "5.9" and "18/11/2026" whole.
fn value_tokens(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.to_lowercase().chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut money = false;
    for (i, &ch) in chars.iter().enumerate() {
        let joins = matches!(ch, ':' | '.' | '/' | '-')
            && cur.ends_with(|c: char| c.is_ascii_digit())
            && chars.get(i + 1).is_some_and(char::is_ascii_digit);
        if ch.is_alphanumeric() || joins {
            if cur.is_empty() {
                money = i > 0 && matches!(chars[i - 1], '£' | '$' | '€');
            }
            cur.push(ch);
        } else if !cur.is_empty() {
            out.push(Token {
                text: std::mem::take(&mut cur),
                money,
            });
        }
    }
    if !cur.is_empty() {
        out.push(Token { text: cur, money });
    }
    out
}

/// The forms of a clock time, in minutes into the day: "14:30" is one, "2:30" with no am or pm
/// is 02:30 or 14:30 (an hour from 1 to 7 is ambiguous), "9" is a time only with am or pm.
fn clock_forms(num: &str, pm: Option<bool>) -> Option<Vec<String>> {
    let (h, m) = match num.split_once([':', '.']) {
        Some((h, m)) if m.len() == 2 => (h.parse::<u16>().ok()?, m.parse::<u16>().ok()?),
        Some(_) => return None,
        None if pm.is_some() => (num.parse::<u16>().ok()?, 0),
        None => return None,
    };
    if h >= 24 || m >= 60 || (pm.is_some() && !(1..=12).contains(&h)) {
        return None;
    }
    let hours = match pm {
        Some(true) if h < 12 => vec![h + 12],
        Some(false) if h == 12 => vec![0],
        None if (1..=7).contains(&h) => vec![h, h + 12],
        _ => vec![h],
    };
    Some(hours.iter().map(|h| (h * 60 + m).to_string()).collect())
}

fn read_token(tokens: &[Token], i: usize) -> Option<Read> {
    let t = tokens[i].text.as_str();
    let next = tokens.get(i + 1).map(|n| n.text.as_str());
    let read = |kind, form: String| {
        Some(Read {
            kind,
            forms: vec![form],
            weak: WEAK_WORDS.contains(&t),
        })
    };
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, ':' | '.' | '/' | '-')))
        .unwrap_or(t.len());
    let (num, suffix) = t.split_at(split);
    if num.is_empty() {
        if let Some((day, _)) = WEEKDAYS.iter().find(|(_, forms)| forms.contains(&t)) {
            return read(ValueKind::Weekday, (*day).to_string());
        }
        if let Some(m) = MONTH_FORMS.iter().position(|forms| forms.contains(&t)) {
            return read(ValueKind::Month, (m + 1).to_string());
        }
        if let Some(n) = ORDINALS.iter().position(|o| *o == t) {
            return read(ValueKind::Ordinal, (n + 1).to_string());
        }
        return match t {
            "noon" | "midday" => read(ValueKind::Time, (12 * 60).to_string()),
            "midnight" => read(ValueKind::Time, "0".to_string()),
            _ => None,
        };
    }
    let separators = num.matches([':', '.', '/', '-']).count();
    let money = tokens[i].money || next.is_some_and(|n| MONEY_WORDS.contains(&n));
    if money || separators > 1 || num.contains(['/', '-']) {
        return None;
    }
    let meridiem = |s: &str| match s {
        "am" => Some(false),
        "pm" => Some(true),
        _ => None,
    };
    let pm = if suffix.is_empty() {
        next.and_then(meridiem)
    } else {
        meridiem(suffix)
    };
    if let Some(forms) = clock_forms(num, pm) {
        return Some(Read {
            kind: ValueKind::Time,
            forms,
            weak: false,
        });
    }
    if num.contains(':') {
        return None;
    }
    let n = num.trim_start_matches('0');
    let n = if n.is_empty() || n.starts_with('.') {
        format!("0{n}")
    } else {
        n.to_string()
    };
    match suffix {
        "st" | "nd" | "rd" | "th" if matches!(n.as_str(), "1" | "2" | "3" | "4" | "5") => {
            read(ValueKind::Ordinal, n)
        }
        "" | "st" | "nd" | "rd" | "th" => read(ValueKind::Number, n),
        _ => None,
    }
}

fn values_of(text: &str) -> Values {
    let tokens = value_tokens(text);
    let reads: Vec<Option<Read>> = (0..tokens.len()).map(|i| read_token(&tokens, i)).collect();
    let dated = |j: usize| {
        reads[j].as_ref().is_some_and(|r| !r.weak)
            || matches!(tokens[j].text.as_str(), "month" | "week")
    };
    // The nearest word on each side, past "of" and "the": "the first of the month".
    let near = |i: usize| {
        let filler = |j: usize| matches!(tokens[j].text.as_str(), "of" | "the");
        let before = (0..i).rev().find(|&j| !filler(j));
        let after = (i + 1..tokens.len()).find(|&j| !filler(j));
        before.is_some_and(dated) || after.is_some_and(dated)
    };
    let mut v = Values::default();
    for (i, read) in reads.iter().enumerate() {
        let Some(read) = read else {
            continue;
        };
        if read.weak && !near(i) {
            continue;
        }
        let into = match read.kind {
            ValueKind::Weekday => &mut v.weekdays,
            ValueKind::Month => &mut v.months,
            ValueKind::Ordinal => &mut v.ordinals,
            ValueKind::Time => &mut v.times,
            ValueKind::Number => &mut v.numbers,
        };
        into.push(read.forms.clone());
    }
    v
}

/// Whether two phrases name different values of one kind, each one the other lacks: "first
/// Tuesday" and "second Wednesday", "10:00" and "14:30". Saying less is not disagreeing:
/// "18 November" and "Wednesday 18 November at 10:00" agree.
fn values_apart(a: &Values, b: &Values) -> bool {
    fn lacks(a: &[Vec<String>], b: &[Vec<String>]) -> bool {
        a.iter()
            .any(|x| !b.iter().any(|y| x.iter().any(|f| y.contains(f))))
    }
    fn apart(a: &[Vec<String>], b: &[Vec<String>]) -> bool {
        lacks(a, b) && lacks(b, a)
    }
    apart(&a.weekdays, &b.weekdays)
        || apart(&a.months, &b.months)
        || apart(&a.ordinals, &b.ordinals)
        || apart(&a.times, &b.times)
        || apart(&a.numbers, &b.numbers)
}

/// Whether two phrases name different values of one kind (see `values_apart`).
pub(crate) fn values_disagree(a: &str, b: &str) -> bool {
    values_apart(&values_of(a), &values_of(b))
}

/// Whether a phrase pins down any value: a day, month, ordinal, time or number.
pub(crate) fn names_a_value(text: &str) -> bool {
    let v = values_of(text);
    !(v.weekdays.is_empty()
        && v.months.is_empty()
        && v.ordinals.is_empty()
        && v.times.is_empty()
        && v.numbers.is_empty())
}

/// Whether a candidate names other values than a memory: another day, month, ordinal, time or
/// number in its object. Sentences are not compared: they carry addresses, rooms and phone
/// numbers that are no part of the value.
fn says_otherwise(c: &Values, object: &str) -> bool {
    values_apart(c, &values_of(object))
}

/// Words that name nothing on their own: articles, pronouns, titles, small joining words.
const PLAIN_WORDS: &[&str] = &[
    "the", "a", "an", "my", "our", "your", "his", "her", "their", "its", "of", "and", "to", "for",
    "at", "on", "in", "with", "dr", "mr", "mrs", "ms", "miss", "i", "me", "we", "us", "you", "he",
    "she", "it", "they", "them",
];

/// Words that say whose words a claim is: "Ravi thinks choir practice is on Friday".
const REPORT_WORDS: [&str; 15] = [
    "says",
    "said",
    "thinks",
    "thought",
    "reckons",
    "reckoned",
    "believes",
    "believed",
    "claims",
    "claimed",
    "insists",
    "insisted",
    "tells",
    "told",
    "mentioned",
];

/// Words that may follow the thing a reported claim is about: "says book club is ...".
const CLAUSE_WORDS: [&str; 20] = [
    "is", "are", "was", "were", "has", "have", "had", "will", "would", "meets", "meet", "starts",
    "start", "runs", "run", "moved", "now", "actually", "on", "at",
];

fn naming_words(subject: &str) -> Vec<String> {
    words_of(subject)
        .into_iter()
        .filter(|w| {
            w.chars().count() > 1
                && !PLAIN_WORDS.contains(&w.as_str())
                && !w.chars().all(|c| c.is_ascii_digit())
        })
        .collect()
}

/// Whether `sentence` names every naming word of `subject`.
pub(crate) fn names_all(sentence: &str, subject: &str) -> bool {
    let words = words_of(sentence);
    let naming = naming_words(subject);
    !naming.is_empty() && naming.iter().all(|w| words.contains(w))
}

/// Whether `sentence` passes on what `teller` says about `about`: "Ravi thinks choir practice is
/// on Friday" is about choir practice, but "Sam says Leo's party is on Friday" is not about Leo.
fn reports_on(sentence: &str, teller: &str, about: &str) -> bool {
    let words = words_of(sentence);
    let about = words_of(&key_of(about));
    let Some(last) = words_of(teller).pop() else {
        return false;
    };
    if about.is_empty() {
        return false;
    }
    (0..words.len().saturating_sub(1)).any(|i| {
        if words[i] != last || !REPORT_WORDS.contains(&words[i + 1].as_str()) {
            return false;
        }
        let mut rest = &words[i + 2..];
        while let Some((first, tail)) = rest.split_first()
            && matches!(first.as_str(), "that" | "the" | "a" | "an" | "my" | "our")
        {
            rest = tail;
        }
        rest.starts_with(&about)
            && rest
                .get(about.len())
                .is_none_or(|w| CLAUSE_WORDS.contains(&w.as_str()))
    })
}

/// The words a memory or claim is said in: its subject, its sentence and what its items say in
/// the words quoted from them.
fn said_words(subject: &str, sentence: &str, quotes: &[String]) -> HashSet<String> {
    words_of(subject)
        .into_iter()
        .chain(words_of(sentence))
        .chain(quotes.iter().flat_map(|q| words_of(q)))
        .collect()
}

/// The words trusted items behind a memory are quoted saying.
fn trusted_quotes(tx: &Transaction<'_>, memory: &str) -> Result<Vec<String>> {
    let mut stmt = tx.prepare(
        "SELECT s.span FROM memory_sources s JOIN items i ON i.id = s.item_id \
         WHERE s.memory_id = ?1 AND i.trust = 'trusted' AND s.span IS NOT NULL",
    )?;
    let spans: Vec<String> = stmt
        .query_map([memory], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(spans
        .iter()
        .filter_map(|json| match serde_json::from_str::<Span>(json) {
            Ok(Span::Quote { text }) => Some(text),
            _ => None,
        })
        .collect())
}

/// Whether the claim (`b_subject`, `b_sentence`) is about the memory `a` filed under another
/// name. Its sentence names all of `a`'s subject, and either it passes on what its own subject
/// says about `a` ("Ravi thinks choir practice is ..."), or every naming word of its subject is
/// among `a`'s words ("Dentist appointment" and a note saying "Dentist appointment with Dr
/// Okafor"). A word `a` never says ("Leo Smith's", "Mum's") may name someone else: no link.
fn about_other_subject(
    a_subject: &str,
    a_words: &HashSet<String>,
    b_subject: &str,
    b_sentence: &str,
) -> bool {
    if !names_all(b_sentence, a_subject) {
        return false;
    }
    if reports_on(b_sentence, b_subject, a_subject) {
        return true;
    }
    let naming = naming_words(b_subject);
    !naming.is_empty() && naming.iter().all(|w| a_words.contains(w))
}

/// Appointments filed under another subject that this candidate disputes: the same one said
/// with a different name ("Dr Okafor" and "Dentist appointment"), or someone's view of it filed
/// under them ("Ravi thinks choir practice is ..."). Only a value that differs links them, only
/// between memories of the same privacy, and only when the candidate points at one other
/// subject. A memory the person re-filed is where they want it and is never linked. A missed
/// link costs a question; a wrong one asks about the wrong thing.
fn appointments_elsewhere(
    tx: &Transaction<'_>,
    c: &Candidate,
    values: &Values,
    subject_key: &str,
    private: bool,
    current: &[MemRow],
) -> Result<Vec<MemRow>> {
    let is_value = |w: &str| {
        WEEKDAYS.iter().any(|(_, forms)| forms.contains(&w))
            || MONTH_FORMS.iter().any(|forms| forms.contains(&w))
    };
    let mut words: Vec<String> = naming_words(&c.subject)
        .into_iter()
        .chain(naming_words(&c.sentence))
        .filter(|w| !is_value(w))
        .collect();
    words.sort();
    words.dedup();
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let shared = words
        .iter()
        .map(|w| format!("\"{w}\""))
        .collect::<Vec<_>>()
        .join(" OR ");
    let others: Vec<(String, String, String, String, String)> = {
        let mut stmt = tx.prepare(
            "SELECT m.id, m.subject, m.subject_key, m.object, m.sentence FROM memories m \
             WHERE m.predicate = 'scheduled' AND m.subject_key != ?1 \
             AND m.superseded_at IS NULL AND m.status IN ('active', 'fading') \
             AND m.moved_from_key IS NULL AND m.private = ?2 \
             AND m.rowid IN (SELECT rowid FROM memories_fts WHERE memories_fts MATCH ?3) \
             ORDER BY m.recorded_at, m.id",
        )?;
        stmt.query_map(params![subject_key, private, shared], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<_>>()?
    };
    let quoted: Vec<String> = match &c.span {
        Some(Span::Quote { text }) => vec![text.clone()],
        _ => Vec::new(),
    };
    let c_words = said_words(&c.subject, &c.sentence, &quoted);
    let mut linked = Vec::new();
    let mut subjects = HashSet::new();
    for (id, subject, key, object, sentence) in others {
        if current.iter().any(|m| m.id == id) || !says_otherwise(values, &object) {
            continue;
        }
        let e_words = said_words(&subject, &sentence, &trusted_quotes(tx, &id)?);
        if about_other_subject(&subject, &e_words, &c.subject, &c.sentence)
            || about_other_subject(&c.subject, &c_words, &subject, &sentence)
        {
            linked.push(id);
            subjects.insert(key);
        }
    }
    if subjects.len() > 1 {
        return Ok(Vec::new());
    }
    let mut rows = Vec::new();
    for id in linked {
        rows.extend(load_mem(tx, &id)?);
    }
    Ok(rows)
}

/// Whether a trusted item behind a memory quotes words that say it moved to its value, or, given
/// `other`'s day, that this day no longer holds ("..., not Wednesday 18 November").
fn quotes_a_move(tx: &Transaction<'_>, m: &MemRow, other: Option<&str>) -> Result<bool> {
    let mut stmt = tx.prepare(
        "SELECT s.span FROM memory_sources s JOIN items i ON i.id = s.item_id \
         WHERE s.memory_id = ?1 AND i.trust = 'trusted' AND s.span IS NOT NULL",
    )?;
    let spans: Vec<String> = stmt
        .query_map([&m.id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let date = m.due.as_deref().or(m.valid_from.as_deref());
    Ok(spans.iter().any(|json| {
        serde_json::from_str::<Span>(json).is_ok_and(|s| {
            quote_says_moved(&s, &m.object, date, &m.subject)
                || match (&s, other) {
                    (Span::Quote { text }, Some(old)) => cancels_in(text, &m.object, date, old),
                    _ => false,
                }
        })
    }))
}

/// What a new appointment does to an existing one in its slot.
enum Meets {
    /// Two dated appointments, nothing saying one moved: two separate events.
    Separate,
    /// The new one is the old one moved: it replaces it.
    Replaces,
    /// They may be the same event and cannot be put in order: ask the person.
    Asks,
}

/// A dated appointment replaces another only when the item says it moved, the item is trusted
/// and was kept clearly later than the trusted items behind the old value, the old date was
/// still ahead when it was kept, the new date is not already past, and the person did not
/// choose the old value themselves. `None` when either is undated: the usual rules apply.
fn appointment_meets(
    tx: &Transaction<'_>,
    c: &Candidate,
    new_date: Option<&String>,
    e: &MemRow,
    item: &ItemInfo,
) -> Result<Option<Meets>> {
    let (Some(new_date), Some(old_date)) = (new_date, e.due.as_ref().or(e.valid_from.as_ref()))
    else {
        return Ok(None);
    };
    if !says_moved(c) && !quote_cancels(c, e) {
        // Read out of order, the newer note may be the one that said it moved: then this may be
        // the same appointment, and only the person can say.
        let same = new_date == old_date || quotes_a_move(tx, e, Some(new_date))?;
        return Ok(Some(if same { Meets::Asks } else { Meets::Separate }));
    }
    let newer = last_kept(tx, &e.id)?.and_then(|old| kept_order(&item.kept_at, &old))
        == Some(std::cmp::Ordering::Greater);
    let ahead =
        old_date.as_str() >= item.kept_on.as_str() && new_date.as_str() >= item.kept_on.as_str();
    Ok(Some(if newer && ahead && !e.confirmed {
        Meets::Replaces
    } else {
        Meets::Asks
    }))
}

/// The current memory this reading made or kept in the same slot as `row`, to take over the
/// history `row` held: only a settled one (active, or confirmed by the person) that a trusted
/// item stands behind, and that is itself a move of every value `row` replaced (its quote says
/// it moved, and the dates allow it). Never an unrelated appointment.
fn successor_in_slot(
    tx: &Transaction<'_>,
    row: &MemRow,
    touched: &HashSet<String>,
    item: &ItemInfo,
) -> Result<Option<String>> {
    let replaced = query_mems(
        tx,
        "m.superseded_by = ?1 AND m.superseded_at IS NOT NULL",
        params![row.id],
    )?;
    let candidates = query_mems(
        tx,
        "m.subject_key = ?1 AND m.predicate = ?2 AND m.superseded_at IS NULL AND m.id != ?3 \
         ORDER BY m.recorded_at DESC, m.id",
        params![row.subject_key, row.predicate, row.id],
    )?;
    for m in candidates {
        let settled = m.status == MemoryStatus::Active || m.confirmed;
        if !(settled && touched.contains(&m.id) && m.trusted_sources > 0) {
            continue;
        }
        let Some(new_date) = m.due.as_ref().or(m.valid_from.as_ref()) else {
            continue;
        };
        let moves_them = replaced.iter().all(|o| {
            o.due
                .as_ref()
                .or(o.valid_from.as_ref())
                .is_some_and(|old| old.as_str() >= item.kept_on.as_str())
        });
        if moves_them && new_date.as_str() >= item.kept_on.as_str() && quotes_a_move(tx, &m, None)?
        {
            return Ok(Some(m.id));
        }
    }
    Ok(None)
}

/// The first word of an object, casefolded: "Metformin 500mg" and "metformin 850mg" are one drug.
fn head(s: &str) -> String {
    key_of(s)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
}

/// Whether two memories with the same subject and predicate compete for one slot.
/// Single-valued: `lives_in`, `scheduled`, `due`, `takes` (per drug head), `prefers` (when tags
/// share a category). Everything else never conflicts.
fn same_slot(predicate: &str, new_object: &str, new_tags: &[String], e: &MemRow) -> bool {
    match predicate {
        "lives_in" | "scheduled" | "due" => true,
        "takes" => head(new_object) == head(&e.object),
        "prefers" => {
            let mine: HashSet<String> = new_tags.iter().map(|t| t.to_lowercase()).collect();
            e.tags.iter().any(|t| mine.contains(&t.to_lowercase()))
        }
        _ => false,
    }
}

/// Independent sources: different origin. Two items with no recorded origin carry no evidence of
/// a shared one, so they count as independent. (Different bytes are guaranteed by dedupe.)
fn independent(a: &Option<String>, b: &Option<String>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x != y,
        _ => true,
    }
}

/// Where a candidate was filed.
pub(crate) struct Filed {
    pub entity_id: String,
    /// The person, place or thing was made by this reading.
    pub created: bool,
}

struct ItemInfo {
    trusted: bool,
    private: bool,
    origin: Option<String>,
    kept_at: String,
    /// The local day it was kept, YYYY-MM-DD, in the time zone the reading happens in: the
    /// store keeps only the UTC instant, so a keep made just before midnight in another zone
    /// may count as the next or previous day.
    kept_on: String,
    /// What the item is in the journal (`entry`, `good`, `checkin` or `answer`), when it is a
    /// journal item.
    journal: Option<String>,
    /// A feelings check-in: it names people and places, and says nothing that becomes a memory.
    checkin: bool,
    /// The photo of a good thing.
    good_photo: bool,
}

impl ItemInfo {
    /// Memories read from this item carry the journal mark.
    fn marks_memories(&self) -> bool {
        self.good_photo || matches!(self.journal.as_deref(), Some("entry" | "good" | "answer"))
    }
}

/// Give every memory linked to this item, with its versions, the journal mark, and drop the
/// cached audio of the pages citing any chain that did not carry it yet. Returns the audio files
/// to shred once the transaction has committed.
pub(crate) fn mark_journal_tx(tx: &Transaction<'_>, item_id: &str) -> Result<Vec<PathBuf>> {
    let linked: Vec<String> = {
        let mut stmt = tx.prepare(
            "SELECT s.memory_id FROM memory_sources s JOIN memories m ON m.id = s.memory_id \
             WHERE s.item_id = ?1 AND m.forgetting = 0 ORDER BY s.memory_id",
        )?;
        stmt.query_map([item_id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut files = Vec::new();
    for id in &linked {
        let versions = chain(tx, id)?;
        let mut changed = 0;
        for v in &versions {
            changed += tx.execute(
                "UPDATE memories SET journal = 1 WHERE id = ?1 AND journal = 0",
                [v],
            )?;
        }
        if changed > 0 {
            let pages = pages_citing(tx, &versions)?;
            files.extend(crate::audio::drop_audio_rows(tx, &pages)?);
        }
    }
    Ok(files)
}

/// What reading needs to know about an item.
fn load_item_info(
    tx: &Transaction<'_>,
    item_id: &str,
    today: &str,
    tz: &jiff::tz::TimeZone,
) -> Result<ItemInfo> {
    let (trust, origin, private, kept_at, journal, kind): (
        String,
        Option<String>,
        bool,
        String,
        Option<String>,
        String,
    ) = tx
        .query_row(
            "SELECT trust, origin, private, kept_at, json_extract(metadata, '$.journal'), kind \
             FROM items WHERE id = ?1",
            [item_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| Error::not_found(format!("item {item_id}")))?;
    let kept_on = kept_at
        .parse::<jiff::Timestamp>()
        .map(|t| t.to_zoned(tz.clone()).date().to_string())
        .unwrap_or_else(|_| today.to_string());
    Ok(ItemInfo {
        trusted: trust == "trusted",
        private,
        origin,
        kept_at,
        kept_on,
        checkin: kind == "checkin" || journal.as_deref() == Some("checkin"),
        journal,
        good_photo: tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM items WHERE json_extract(metadata, '$.journal') = 'good' \
             AND json_extract(metadata, '$.photo') = ?1)",
            [item_id],
            |r| r.get(0),
        )?,
    })
}

/// The thing a subject is filed under: the live one with that name key, whatever its kind
/// (memories match their subject by key alone). With several, the one of the requested kind
/// wins, then by id. A subject nothing carries yet gets a new one when its kind is known; the
/// flag says it was made now.
pub(crate) fn upsert_entity(
    tx: &Transaction<'_>,
    subject: &str,
    subject_key: &str,
    kind: Option<EntityKind>,
) -> Result<Option<(String, bool)>> {
    if let Some(id) = find_by_key(tx, subject_key, kind)? {
        return Ok(Some((id, false)));
    }
    let Some(k) = kind else {
        return Ok(None);
    };
    let id = new_id("ent");
    tx.execute(
        "INSERT INTO entities(id, kind, name, key) VALUES (?1, ?2, ?3, ?4)",
        params![
            id,
            enum_str(&k),
            subject,
            format!("{}:{subject_key}", enum_str(&k))
        ],
    )?;
    Ok(Some((id, true)))
}

/// Rows this memory closed or superseded come back: the change that ended them is gone. Rows it
/// superseded while it was itself superseded go to its replacement instead.
pub(crate) fn restore_replaced(tx: &Transaction<'_>, id: &str) -> Result<()> {
    // A memory that was itself replaced hands what it replaced on to its own replacement: in
    // A, B, C (each moving the last), removing B leaves A as C's history, not current again.
    let replacement: Option<String> = tx
        .query_row(
            "SELECT superseded_by FROM memories WHERE id = ?1 AND superseded_at IS NOT NULL",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if let Some(next) = replacement {
        tx.execute(
            "UPDATE memories SET superseded_by = ?2 \
             WHERE superseded_by = ?1 AND superseded_at IS NOT NULL",
            params![id, next],
        )?;
    }
    tx.execute(
        "UPDATE memories SET valid_to = NULL, superseded_by = NULL \
         WHERE superseded_by = ?1 AND superseded_at IS NULL \
           AND valid_to = (SELECT valid_from FROM memories WHERE id = ?1)",
        [id],
    )?;
    tx.execute(
        "UPDATE memories SET superseded_at = NULL, superseded_by = NULL WHERE superseded_by = ?1",
        [id],
    )?;
    Ok(())
}

fn delete_memory(tx: &Transaction<'_>, id: &str) -> Result<()> {
    restore_replaced(tx, id)?;
    tx.execute("DELETE FROM conflicts WHERE a = ?1 OR b = ?1", [id])?;
    crate::journal::question::drop_memory_questions(tx, id)?;
    tx.execute("DELETE FROM page_line_memories WHERE memory_id = ?1", [id])?;
    tx.execute("DELETE FROM memories WHERE id = ?1", [id])?;
    Ok(())
}

/// A memory that changed (confirmed, corrected, newly in conflict) is no longer snoozed.
fn clear_snooze(tx: &Transaction<'_>, ids: &[&str]) -> Result<()> {
    for id in ids {
        tx.execute(
            "UPDATE memories SET snoozed_until = NULL WHERE id = ?1 AND snoozed_until IS NOT NULL",
            [id],
        )?;
    }
    Ok(())
}

/// Whether the memory is snoozed past `today`.
fn is_snoozed(conn: &rusqlite::Connection, id: &str, today: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT COALESCE(snoozed_until > ?2, 0) FROM memories WHERE id = ?1",
        params![id, today],
        |r| r.get(0),
    )?)
}

/// Conflicts for a memory that has just become active (corroborated or confirmed): it competes
/// with other active memories in the same slot whose validity overlaps. Returns new conflict ids.
fn create_conflicts(tx: &Transaction<'_>, id: &str, now: &str, today: &str) -> Result<Vec<String>> {
    let Some(row) = load_mem(tx, id)? else {
        return Ok(Vec::new());
    };
    if row.status != MemoryStatus::Active {
        return Ok(Vec::new());
    }
    let others = query_mems(
        tx,
        "m.subject_key = ?1 AND m.predicate = ?2 AND m.superseded_at IS NULL AND m.id != ?3 \
         AND m.status IN ('active', 'fading') ORDER BY m.recorded_at, m.id",
        params![row.subject_key, row.predicate, row.id],
    )?;
    let mut made = Vec::new();
    for e in &others {
        if e.object_key == row.object_key || !same_slot(&row.predicate, &row.object, &row.tags, e) {
            continue;
        }
        if !overlaps(
            e.valid_from.as_deref(),
            e.valid_to.as_deref(),
            Some(row.valid_from.as_deref().unwrap_or(today)),
            row.valid_to.as_deref(),
        ) {
            continue;
        }
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM conflicts WHERE resolved_at IS NULL \
             AND ((a = ?1 AND b = ?2) OR (a = ?2 AND b = ?1)))",
            params![e.id, row.id],
            |r| r.get(0),
        )?;
        if !exists {
            let cid = new_id("cfl");
            tx.execute(
                "INSERT INTO conflicts(id, a, b, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![cid, e.id, row.id, now],
            )?;
            clear_snooze(tx, &[&e.id, &row.id])?;
            made.push(cid);
        }
    }
    Ok(made)
}

/// Every version of one memory: what it replaced or ended and what replaced or ended it,
/// followed in both directions. Empty if the id does not exist.
pub(crate) fn chain(conn: &rusqlite::Connection, id: &str) -> Result<Vec<String>> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM memories WHERE id = ?1)",
        [id],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(Vec::new());
    }
    let mut seen: Vec<String> = vec![id.to_string()];
    let mut i = 0;
    while i < seen.len() {
        let cur = seen[i].clone();
        i += 1;
        let mut older = conn.prepare_cached("SELECT id FROM memories WHERE superseded_by = ?1")?;
        let mut neighbours: Vec<String> = older
            .query_map([&cur], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let newer: Option<String> = conn
            .query_row(
                "SELECT superseded_by FROM memories WHERE id = ?1",
                [&cur],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        neighbours.extend(newer);
        for n in neighbours {
            if !seen.contains(&n) {
                seen.push(n);
            }
        }
    }
    Ok(seen)
}

/// Whether a memory, or any version of it, is private. Unknown ids fail closed: they count as
/// private, so a line citing something that no longer exists never reaches audio or print.
pub(crate) fn is_private_chain(conn: &rusqlite::Connection, id: &str) -> Result<bool> {
    let ids = chain(conn, id)?;
    if ids.is_empty() {
        return Ok(true);
    }
    for v in &ids {
        let private: bool =
            conn.query_row("SELECT private FROM memories WHERE id = ?1", [v], |r| {
                r.get(0)
            })?;
        if private {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether a memory, or any version of it, was read from an entry or a good thing. Private is
/// checked first by every caller: a memory with a private source is private whatever else it
/// is. An unknown id is no journal memory (it already counts as private).
pub(crate) fn is_journal_chain(conn: &rusqlite::Connection, id: &str) -> Result<bool> {
    for v in chain(conn, id)? {
        let marked: bool =
            conn.query_row("SELECT journal FROM memories WHERE id = ?1", [&v], |r| {
                r.get(0)
            })?;
        if marked {
            return Ok(true);
        }
    }
    Ok(false)
}

fn span_json(c: &Candidate) -> Option<String> {
    c.span.as_ref().and_then(|s| serde_json::to_string(s).ok())
}

fn date_str(d: Option<jiff::civil::Date>) -> Option<String> {
    d.map(|d| d.to_string())
}

#[allow(clippy::too_many_arguments)]
fn apply(
    tx: &Transaction<'_>,
    now: &str,
    today: &str,
    tz: &jiff::tz::TimeZone,
    item_id: &str,
    lease: &str,
    cands: Vec<Candidate>,
    took: Option<std::time::Duration>,
) -> Result<(ApplyReport, Vec<PathBuf>)> {
    check_lease(tx, item_id, lease)?;
    // A forget since the read began: what the mind found may be what was forgotten.
    let stale: bool = tx.query_row(
        "SELECT forget_epoch <> lease_epoch FROM items WHERE id = ?1",
        [item_id],
        |r| r.get(0),
    )?;
    if stale {
        return Err(Error::LeaseLost);
    }
    let forgotten: HashSet<String> = {
        let mut stmt = tx.prepare("SELECT claim FROM forgotten_claims WHERE item_id = ?1")?;
        stmt.query_map([item_id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    let cands: Vec<Candidate> = cands
        .into_iter()
        .filter(|c| {
            !forgotten.contains(&claim_hash(
                &key_of(&c.subject),
                c.predicate.as_str(),
                &key_of(&c.object),
            ))
        })
        .collect();
    let item = load_item_info(tx, item_id, today, tz)?;
    let routed = route(tx, now, item_id, cands)?;
    if item.checkin {
        mention_only(tx, now, item_id, &routed, true)?;
        finish_item(tx, item_id, "understood", took)?;
        return Ok((ApplyReport::default(), Vec::new()));
    }
    let mut report = ApplyReport::default();

    // Applying is idempotent per item: detach what this item said before, apply again, then
    // drop what nothing supports any more.
    // An item that replaces an earlier version is read as that version read again: what only
    // the earlier words supported goes the way a re-read lets it go, and the earlier item keeps
    // its mentions as history.
    let replaces: Option<String> = tx.query_row(
        "SELECT json_extract(metadata, '$.replaces') FROM items WHERE id = ?1",
        [item_id],
        |r| r.get(0),
    )?;
    let earlier: Vec<String> = std::iter::once(item_id.to_string())
        .chain(replaces.filter(|r| r != item_id))
        .collect();
    let mut prev: Vec<(String, Option<String>, String)> = Vec::new();
    for from in &earlier {
        let mut stmt =
            tx.prepare("SELECT memory_id, span FROM memory_sources WHERE item_id = ?1")?;
        let rows = stmt.query_map([from], |r| Ok((r.get(0)?, r.get(1)?, from.clone())))?;
        for row in rows {
            prev.push(row?);
        }
        tx.execute("DELETE FROM memory_sources WHERE item_id = ?1", [from])?;
    }
    let relinked: HashSet<&str> = prev.iter().map(|p| p.0.as_str()).collect();
    let mut touched: HashSet<String> = HashSet::new();

    let mut found: Vec<Found> = Vec::new();
    for r in &routed {
        if let Some(filed) = apply_one(
            tx,
            now,
            today,
            item_id,
            &item,
            &relinked,
            &mut touched,
            &mut report,
            &r.cand,
        )? {
            found.push(Found::of(r, filed.entity_id, filed.created));
        }
    }
    write_mentions(tx, item_id, now, &found, true)?;

    for (mid, span, from) in &prev {
        if touched.contains(mid) {
            continue;
        }
        let remaining: u32 = tx.query_row(
            "SELECT COUNT(*) FROM memory_sources WHERE memory_id = ?1",
            [mid],
            |r| r.get(0),
        )?;
        if remaining > 0 {
            continue;
        }
        let Some(row) = load_mem(tx, mid)? else {
            continue;
        };
        // Nothing now supports this memory. It is deleted only if nothing else depends on it:
        // a memory the person kept forever, used, confirmed or marked private, or that a saved
        // page cites, is never silently dropped by a re-read.
        let protected = row.keep_forever
            || row.last_used.is_some()
            || row.confirmed
            || row.private
            || !pages_citing(tx, std::slice::from_ref(mid))?.is_empty();
        // A memory that replaced others is their history's link: it goes only when this
        // reading has a new value in its slot to take that link over, and is kept otherwise.
        let replaced_others: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM memories WHERE superseded_by = ?1 \
             AND superseded_at IS NOT NULL)",
            [mid],
            |r| r.get(0),
        )?;
        // Decided before anything moves: a row that stays (protected, confirmed, already
        // history) keeps its links.
        let may_go = row.superseded_at.is_none() && !protected;
        let successor = if may_go && replaced_others {
            successor_in_slot(tx, &row, &touched, &item)?
        } else {
            None
        };
        if may_go && (!replaced_others || successor.is_some()) {
            if let Some(next) = &successor {
                tx.execute(
                    "UPDATE memories SET superseded_by = ?2 \
                     WHERE superseded_by = ?1 AND superseded_at IS NOT NULL",
                    params![mid, next],
                )?;
            }
            delete_memory(tx, mid)?;
            report.removed.push(mid.clone());
        } else {
            // It keeps a tombstone link to this item (every memory has a receipt) and, unless
            // the person confirmed it, goes back to unconfirmed: nothing supports it now.
            tx.execute(
                "INSERT OR IGNORE INTO memory_sources(memory_id, item_id, span) VALUES (?1, ?2, ?3)",
                params![mid, from, span],
            )?;
            if row.superseded_at.is_none() && !row.confirmed {
                tx.execute(
                    "UPDATE memories SET status = 'unconfirmed', let_go_at = NULL WHERE id = ?1",
                    [mid],
                )?;
            }
        }
    }

    // A question raised against a memory that has since gone is gone with it.
    let mut open_conflicts = Vec::new();
    for cid in std::mem::take(&mut report.conflicts) {
        let open: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM conflicts WHERE id = ?1)",
            [&cid],
            |r| r.get(0),
        )?;
        if open {
            open_conflicts.push(cid);
        }
    }
    report.conflicts = open_conflicts;

    let files = settle_links(tx, item_id, &item)?;

    let status = if !item.trusted {
        "unconfirmed"
    } else if !report.conflicts.is_empty() {
        "needsYou"
    } else {
        "understood"
    };
    finish_item(tx, item_id, status, took)?;
    Ok((report, files))
}

/// The item is read: its status, and no lease.
fn finish_item(
    tx: &Transaction<'_>,
    item_id: &str,
    status: &str,
    took: Option<std::time::Duration>,
) -> Result<()> {
    tx.execute(
        "UPDATE items SET status = ?2, lease_until = NULL, lease_token = NULL, \
         status_before = NULL, reread = 0, failures = 0, \
         understood_in_ms = COALESCE(?3, understood_in_ms) WHERE id = ?1",
        params![
            item_id,
            status,
            took.map(|t| u32::try_from(t.as_millis()).unwrap_or(u32::MAX))
        ],
    )?;
    Ok(())
}

/// A check-in's reading: the names it says, as mentions, and nothing else. No memory is made,
/// linked or asked about, so what a check-in says never reaches Ask or goes online.
fn mention_only(
    tx: &Transaction<'_>,
    now: &str,
    item_id: &str,
    routed: &[Routed],
    prune: bool,
) -> Result<()> {
    let mut found: Vec<Found> = Vec::new();
    for r in routed {
        let c = &r.cand;
        let name = match resolve_alias(tx, &key_of(&c.subject))? {
            Some(alias) => alias.name,
            None => c.subject.trim().to_string(),
        };
        if name.is_empty() {
            continue;
        }
        if let Some((entity_id, created)) =
            upsert_entity(tx, &name, &key_of(&name), c.subject_kind)?
        {
            found.push(Found::of(r, entity_id, created));
        }
    }
    write_mentions(tx, item_id, now, &found, prune)
}

/// File what was held back for a name that fit two people, now that the person has said who it
/// is. The item's other memories are not touched. Returns the files a private memory let go of.
pub(crate) fn file_held(
    tx: &Transaction<'_>,
    now: &str,
    today: &str,
    tz: &jiff::tz::TimeZone,
    item_id: &str,
    routed: Vec<Routed>,
) -> Result<Vec<PathBuf>> {
    let item = load_item_info(tx, item_id, today, tz)?;
    if item.checkin {
        mention_only(tx, now, item_id, &routed, false)?;
        return Ok(Vec::new());
    }
    let relinked = HashSet::new();
    let mut touched = HashSet::new();
    let mut report = ApplyReport::default();
    let mut found = Vec::new();
    for r in &routed {
        if let Some(filed) = apply_one(
            tx,
            now,
            today,
            item_id,
            &item,
            &relinked,
            &mut touched,
            &mut report,
            &r.cand,
        )? {
            found.push(Found::of(r, filed.entity_id, filed.created));
        }
    }
    write_mentions(tx, item_id, now, &found, false)?;
    settle_links(tx, item_id, &item)
}

/// What an item says takes its sensitivity: every memory now linked to it, whether it made the
/// memory, agreed with it, or kept a tombstone link, is private when the item is, and carries
/// the journal mark when the item is an entry, an answer, a good thing or a good thing's photo. (Early
/// returns in `apply_one` skip the report lists, so the links are the truth.) Returns the cached
/// audio files a private or newly marked memory let go of.
fn settle_links(tx: &Transaction<'_>, item_id: &str, item: &ItemInfo) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if item.private {
        let linked: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT s.memory_id FROM memory_sources s JOIN memories m ON m.id = s.memory_id \
                 WHERE s.item_id = ?1 AND m.forgetting = 0 ORDER BY s.memory_id",
            )?;
            stmt.query_map([item_id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for id in &linked {
            files.extend(crate::lifecycle::set_private_tx(tx, id, true)?);
        }
    }
    if item.marks_memories() {
        files.extend(mark_journal_tx(tx, item_id)?);
    }
    Ok(files)
}

#[allow(clippy::too_many_arguments)]
fn apply_one(
    tx: &Transaction<'_>,
    now: &str,
    today: &str,
    item_id: &str,
    item: &ItemInfo,
    relinked: &HashSet<&str>,
    touched: &mut HashSet<String>,
    report: &mut ApplyReport,
    c: &Candidate,
) -> Result<Option<Filed>> {
    if c.subject.trim().is_empty() || c.object.trim().is_empty() || c.sentence.trim().is_empty() {
        return Err(Error::invalid(
            "a candidate needs a subject, an object and a sentence",
        ));
    }
    // A name the person changed is the name that counts: a claim read about the old one is
    // filed under the new one, in its words too. What that writes is noted against the change,
    // so taking the change back takes it back too.
    let renamed;
    let mut alias_change: Option<String> = None;
    let c = match resolve_alias(tx, &key_of(&c.subject))? {
        Some(Alias { name, change_id }) => {
            alias_change = change_id;
            renamed = Candidate {
                sentence: replace_name(&c.sentence, c.subject.trim(), &name),
                subject: name,
                ..c.clone()
            };
            &renamed
        }
        None => c,
    };
    let subject_key = key_of(&c.subject);
    let object_key = key_of(&c.object);
    let predicate = c.predicate.as_str().trim().to_lowercase();
    let confidence = f64::from(c.confidence.clamp(0.0, 1.0));
    let (vf, vt, due) = (
        date_str(c.valid_from),
        date_str(c.valid_to),
        date_str(c.due),
    );
    let span = span_json(c);

    // The person already corrected this claim away; do not bring it back. A row the person
    // moved answers to its first subject only for the item that said it.
    let known_wrong: bool = {
        let mut stmt = tx.prepare(
            "SELECT w.id, w.subject_key = ?1 FROM memories w JOIN memories s ON s.id = w.superseded_by \
             WHERE (w.subject_key = ?1 OR w.moved_from_key = ?1) AND w.predicate = ?2 \
               AND w.object_key = ?3 \
               AND w.superseded_at IS NOT NULL AND s.confirmed = 1 AND s.superseded_at IS NULL",
        )?;
        let rows: Vec<(String, bool)> = stmt
            .query_map(params![subject_key, predicate, object_key], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        rows.iter()
            .any(|(id, direct)| *direct || relinked.contains(id.as_str()))
    };
    if known_wrong {
        return Ok(None);
    }

    // Memories filed under this subject. One the person moved elsewhere still answers to the
    // subject it was first filed under, but only for the item that said it: any other item
    // saying the same words about that subject is about that subject.
    let mut current = query_mems(
        tx,
        "(m.subject_key = ?1 OR m.moved_from_key = ?1) AND m.predicate = ?2 \
         AND m.superseded_at IS NULL ORDER BY m.recorded_at, m.id",
        params![subject_key, predicate],
    )?;
    current.retain(|m| m.subject_key == subject_key || relinked.contains(m.id.as_str()));
    let values = values_of(&c.object);
    let same_date = |e: &MemRow| {
        matches!(predicate.as_str(), "due" | "scheduled")
            && due.is_some()
            && e.due == due
            && !says_otherwise(&values, &e.object)
    };
    // A date-only match (same due date, different words) is a convenience for trusted items
    // agreeing with settled memories. It never matches an unconfirmed memory, so it can never
    // promote one: promotion needs the same object from a trusted independent item. Words that
    // name another day, ordinal, time or number are no match whatever the date: filed as a
    // receipt, the item would stand behind a value it contradicts. They are a move or a question.
    let matched = current.iter().find(|e| {
        e.object_key == object_key
            || (item.trusted && e.status != MemoryStatus::Unconfirmed && same_date(e))
    });

    if let Some(m) = matched {
        let filed = m.entity_id.clone().map(|entity_id| Filed {
            entity_id,
            created: false,
        });
        tx.execute(
            "INSERT OR IGNORE INTO memory_sources(memory_id, item_id, span) VALUES (?1, ?2, ?3)",
            params![m.id, item_id, span],
        )?;
        touched.insert(m.id.clone());
        if relinked.contains(m.id.as_str()) {
            // The same item read again: refresh, do not count it as a second voice. An untrusted
            // re-read never rewrites a memory that trusted sources support; it may only refine
            // an unconfirmed memory it alone supports. (`m.sources` was counted after this
            // item's link was removed, so 0 means this item is the only source.)
            let may_refresh =
                item.trusted || (m.status == MemoryStatus::Unconfirmed && m.sources == 0);
            if !m.confirmed && may_refresh {
                tx.execute(
                    "UPDATE memories SET sentence = ?2, confidence = ?3, valid_from = ?4, \
                     valid_to = ?5, due = ?6 WHERE id = ?1",
                    params![m.id, c.sentence.trim(), confidence, vf, vt, due],
                )?;
                if m.sentence != c.sentence.trim() {
                    // The old vector described the old words.
                    tx.execute("DELETE FROM embeddings WHERE memory_id = ?1", [&m.id])?;
                    report.reworded.push(m.id.clone());
                    note_rewrite(tx, alias_change.as_deref(), &m.id)?;
                }
            }
            return Ok(filed);
        }
        if !item.trusted {
            return Ok(filed);
        }
        let others: Vec<Option<String>> = {
            let mut stmt = tx.prepare(
                "SELECT i.origin FROM memory_sources s JOIN items i ON i.id = s.item_id \
                 WHERE s.memory_id = ?1 AND s.item_id != ?2",
            )?;
            let rows = stmt.query_map(params![m.id, item_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let all_independent = others.iter().all(|o| independent(o, &item.origin));
        let promote =
            m.status == MemoryStatus::Unconfirmed && all_independent && !c.predicate.is_other();
        let status = match m.status {
            MemoryStatus::Fading | MemoryStatus::LetGo => MemoryStatus::Active,
            MemoryStatus::Unconfirmed if promote => MemoryStatus::Active,
            other => other,
        };
        tx.execute(
            "UPDATE memories SET last_used = ?2, confidence = MAX(confidence, ?3), status = ?4, \
             let_go_at = CASE WHEN ?4 = 'letGo' THEN let_go_at ELSE NULL END WHERE id = ?1",
            params![m.id, now, confidence, enum_str(&status)],
        )?;
        if promote {
            report.promoted.push(m.id.clone());
            // Becoming active makes it compete with what is already active.
            let made = create_conflicts(tx, &m.id, now, today)?;
            report.conflicts.extend(made);
        }
        report.reinforced.push(m.id.clone());
        return Ok(filed);
    }

    // The same item read again, saying what a later item has since replaced: the old row is
    // linked again as history, not made a second time.
    let history: Option<String> = {
        let mut stmt = tx.prepare(
            "SELECT m.id, m.subject_key = ?1 FROM memories m \
             WHERE (m.subject_key = ?1 OR m.moved_from_key = ?1) \
             AND m.predicate = ?2 AND m.object_key = ?3 AND m.superseded_at IS NOT NULL \
             ORDER BY m.recorded_at DESC, m.id",
        )?;
        let rows: Vec<(String, bool)> = stmt
            .query_map(params![subject_key, predicate, object_key], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        rows.into_iter()
            .find(|(id, direct)| *direct || relinked.contains(id.as_str()))
            .map(|(id, _)| id)
            .filter(|id| relinked.contains(id.as_str()))
    };
    if let Some(old) = history {
        tx.execute(
            "INSERT OR IGNORE INTO memory_sources(memory_id, item_id, span) VALUES (?1, ?2, ?3)",
            params![old, item_id, span],
        )?;
        let entity_id: Option<String> = tx
            .query_row(
                "SELECT entity_id FROM memories WHERE id = ?1",
                [&old],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        touched.insert(old);
        return Ok(entity_id.map(|entity_id| Filed {
            entity_id,
            created: false,
        }));
    }

    // A new memory.
    let status = if !item.trusted || c.predicate.is_other() {
        MemoryStatus::Unconfirmed
    } else {
        MemoryStatus::Active
    };
    let id = new_id("mem");
    let mut to_close: Vec<String> = Vec::new();
    let mut to_replace: Vec<String> = Vec::new();
    let mut in_conflict: Vec<String> = Vec::new();
    // What the move rule said of each kept appointment in the slot, logged in shadow once the
    // new memory is stored: (kept id, kept sentence, moved, how the rule acted).
    let mut moves_seen: Vec<(String, String, bool, &'static str)> = Vec::new();
    if status == MemoryStatus::Active {
        let elsewhere = if predicate == "scheduled" {
            appointments_elsewhere(tx, c, &values, &subject_key, item.private, &current)?
        } else {
            Vec::new()
        };
        let slot = current.iter().map(|e| (e, false));
        for (e, filed_elsewhere) in slot.chain(elsewhere.iter().map(|e| (e, true))) {
            if !matches!(e.status, MemoryStatus::Active | MemoryStatus::Fading)
                || e.object_key == object_key
                || !same_slot(&predicate, &c.object, &c.tags, e)
            {
                continue;
            }
            if filed_elsewhere {
                // Under another name it may be this appointment or another one: never replaced,
                // only asked about.
                in_conflict.push(e.id.clone());
                continue;
            }
            let later_start = match (&e.valid_from, &vf) {
                (None, Some(_)) => true,
                (Some(a), Some(b)) => a < b,
                _ => false,
            };
            let still_open = match (&e.valid_to, &vf) {
                (None, _) => true,
                (Some(t), Some(f)) => t > f,
                _ => false,
            };
            // Two dated appointments: separate events, a move, or a question (see
            // `appointment_meets`). An undated one (a club that meets monthly) said two ways is
            // a contradiction. This loop runs only for a trusted item: an untrusted one makes an
            // unconfirmed memory instead.
            let meets = if predicate == "scheduled" {
                appointment_meets(tx, c, due.as_ref().or(vf.as_ref()), e, item)?
            } else {
                None
            };
            if let Some(meets) = &meets {
                let detail = match meets {
                    Meets::Separate => "separate",
                    Meets::Replaces => "replaces",
                    Meets::Asks => "asks",
                };
                let moved = says_moved(c) || quote_cancels(c, e);
                moves_seen.push((e.id.clone(), e.sentence.clone(), moved, detail));
            }
            if closes_old_value(&predicate) && later_start && still_open {
                to_close.push(e.id.clone());
            } else if let Some(meets) = meets {
                match meets {
                    Meets::Separate => {}
                    Meets::Replaces => to_replace.push(e.id.clone()),
                    Meets::Asks => in_conflict.push(e.id.clone()),
                }
            } else if overlaps(
                e.valid_from.as_deref(),
                e.valid_to.as_deref(),
                // A claim with no start is about now, so it does not collide with what ended.
                Some(vf.as_deref().unwrap_or(today)),
                vt.as_deref(),
            ) {
                in_conflict.push(e.id.clone());
            }
        }
    }
    let safety = c.predicate == Predicate::Takes
        || c.tags.iter().any(|t| {
            matches!(
                t.to_lowercase().as_str(),
                "health" | "allergy" | "medication"
            )
        });
    let (entity_id, created) = match upsert_entity(tx, &c.subject, &subject_key, c.subject_kind)? {
        Some((id, created)) => (Some(id), created),
        None => (None, false),
    };
    tx.execute(
        "INSERT INTO memories(id, subject, subject_key, entity_id, predicate, object, object_key, \
         sentence, valid_from, valid_to, due, recorded_at, confidence, status, keep_forever, \
         suggest_private, tags) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, ?16)",
        params![
            id,
            c.subject.trim(),
            subject_key,
            entity_id,
            predicate,
            c.object.trim(),
            object_key,
            c.sentence.trim(),
            vf,
            vt,
            due,
            now,
            confidence,
            enum_str(&status),
            safety,
            serde_json::to_string(&c.tags)?,
        ],
    )?;
    tx.execute(
        "INSERT INTO memory_sources(memory_id, item_id, span) VALUES (?1, ?2, ?3)",
        params![id, item_id, span],
    )?;
    let note = match &c.span {
        Some(Span::Quote { text }) => text.as_str(),
        _ => c.sentence.as_str(),
    };
    for (other, kept, moved, detail) in &moves_seen {
        crate::decide::log::record_moved(tx, item_id, &id, other, kept, note, *moved, detail, now)?;
    }
    for old in &to_close {
        tx.execute(
            "UPDATE memories SET valid_to = ?2, superseded_by = ?3 WHERE id = ?1",
            params![old, vf, id],
        )?;
    }
    for old in &to_replace {
        tx.execute(
            "UPDATE memories SET superseded_at = ?2, superseded_by = ?3 WHERE id = ?1",
            params![old, now, id],
        )?;
        report.replaced.push(old.clone());
    }
    for old in &in_conflict {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM conflicts WHERE resolved_at IS NULL \
             AND ((a = ?1 AND b = ?2) OR (a = ?2 AND b = ?1)))",
            params![old, id],
            |r| r.get(0),
        )?;
        if !exists {
            let cid = new_id("cfl");
            tx.execute(
                "INSERT INTO conflicts(id, a, b, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![cid, old, id, now],
            )?;
            clear_snooze(tx, &[old, &id])?;
            report.conflicts.push(cid);
        }
    }
    note_rewrite(tx, alias_change.as_deref(), &id)?;
    touched.insert(id.clone());
    if status == MemoryStatus::Unconfirmed {
        report.unconfirmed.push(id.clone());
    }
    report.created.push(id);
    Ok(entity_id.map(|entity_id| Filed { entity_id, created }))
}

/// The entries waiting on the person, as of `today` (local).
pub(crate) fn needs_you_on(
    conn: &rusqlite::Connection,
    today: jiff::civil::Date,
) -> Result<Vec<NeedsYouEntry>> {
    let horizon = today
        .checked_add(jiff::Span::new().days(14))
        .map_err(Error::from)?
        .to_string();
    let today_s = today.to_string();
    let mut out: Vec<NeedsYouEntry> = Vec::new();

    // Due items, soonest first; an undated one waits at the end like an undated promise.
    let dated = query_mems(
        conn,
        "m.predicate = 'due' AND m.superseded_at IS NULL AND m.done = 0 \
         AND m.status IN ('active', 'fading') \
         AND (m.snoozed_until IS NULL OR m.snoozed_until <= ?2) \
         AND (COALESCE(m.due, m.valid_from) IS NULL OR COALESCE(m.due, m.valid_from) <= ?1) \
         ORDER BY COALESCE(m.due, m.valid_from) IS NULL, COALESCE(m.due, m.valid_from), \
         m.recorded_at, m.id",
        [&horizon, &today_s],
    )?;
    for m in dated {
        let due = m
            .due
            .clone()
            .or_else(|| m.valid_from.clone())
            .unwrap_or_default();
        out.push(NeedsYouEntry::Due {
            memory: mem_view(conn, &m)?,
            due,
            done: false,
        });
    }

    let promises = query_mems(
        conn,
        "m.predicate = 'promised' AND m.superseded_at IS NULL AND m.done = 0 \
         AND m.status IN ('active', 'fading') \
         AND (m.snoozed_until IS NULL OR m.snoozed_until <= ?2) \
         AND (COALESCE(m.due, m.valid_from) IS NULL OR COALESCE(m.due, m.valid_from) <= ?1) \
         ORDER BY COALESCE(m.due, m.valid_from) IS NULL, COALESCE(m.due, m.valid_from), \
         m.recorded_at, m.id",
        [&horizon, &today_s],
    )?;
    for m in promises {
        out.push(NeedsYouEntry::Promise {
            memory: mem_view(conn, &m)?,
            done: false,
        });
    }

    let mut stmt = conn.prepare(
        "SELECT id, a, b FROM conflicts WHERE resolved_at IS NULL ORDER BY created_at, id",
    )?;
    let conflicts: Vec<(String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (cid, a, b) in conflicts {
        let (Some(ma), Some(mb)) = (load_mem(conn, &a)?, load_mem(conn, &b)?) else {
            continue;
        };
        if ma.superseded_at.is_some() || mb.superseded_at.is_some() {
            continue;
        }
        if is_snoozed(conn, &a, &today_s)? || is_snoozed(conn, &b, &today_s)? {
            continue;
        }
        let mut options = Vec::new();
        for m in [&ma, &mb] {
            let Some(receipt) = receipts_for(conn, &m.id)?.into_iter().next() else {
                continue;
            };
            options.push(WhichOption {
                memory_id: m.id.clone(),
                text: m.sentence.clone(),
                receipt,
            });
        }
        if options.len() == 2 {
            out.push(NeedsYouEntry::Which {
                conflict_id: cid,
                title: ma.subject.clone(),
                options,
            });
        }
    }

    for question in crate::journal::mentions::open_questions(conn, None)? {
        out.push(NeedsYouEntry::SameName { question });
    }

    let unconfirmed = query_mems(
        conn,
        "m.status = 'unconfirmed' AND m.superseded_at IS NULL \
         AND (m.snoozed_until IS NULL OR m.snoozed_until <= ?1) \
         ORDER BY m.recorded_at DESC, m.id",
        [&today_s],
    )?;
    for m in unconfirmed {
        out.push(NeedsYouEntry::Unconfirmed {
            agreeing_sources: m.sources,
            used: m.last_used.is_some(),
            memory: mem_view(conn, &m)?,
        });
    }
    Ok(out)
}

pub(crate) fn confirm_tx(
    tx: &Transaction<'_>,
    id: &str,
    is_true: bool,
    now: &str,
    today: &str,
) -> Result<()> {
    let m = load_mem(tx, id)?.ok_or_else(|| Error::not_found(format!("memory {id}")))?;
    clear_snooze(tx, &[id])?;
    if is_true && m.superseded_at.is_some() {
        return confirm_replaced(tx, &m, now);
    }
    if is_true {
        tx.execute(
            "UPDATE memories SET confirmed = 1, status = 'active', last_used = ?2, \
             let_go_at = NULL WHERE id = ?1",
            params![m.id, now],
        )?;
        if m.status == MemoryStatus::Unconfirmed {
            // Becoming active, it competes with what is already active.
            create_conflicts(tx, &m.id, now, today)?;
        }
    } else {
        tx.execute(
            "UPDATE memories SET confirmed = 0, status = 'unconfirmed', let_go_at = NULL \
             WHERE id = ?1",
            [&m.id],
        )?;
    }
    Ok(())
}

/// The person says a value that was replaced is right. It comes back as current and confirmed.
/// If what replaced it rests only on unconfirmed words, it simply wins; otherwise the person is
/// asked which of the two is right.
fn confirm_replaced(tx: &Transaction<'_>, m: &MemRow, now: &str) -> Result<()> {
    let mut head = m.superseded_by.clone();
    let mut current: Option<MemRow> = None;
    for _ in 0..64 {
        let Some(id) = head.take() else { break };
        let Some(row) = load_mem(tx, &id)? else { break };
        if row.superseded_at.is_none() {
            current = Some(row);
            break;
        }
        head = row.superseded_by.clone();
    }
    tx.execute(
        "UPDATE memories SET superseded_at = NULL, superseded_by = NULL, confirmed = 1, \
         status = 'active', last_used = ?2, let_go_at = NULL WHERE id = ?1",
        params![m.id, now],
    )?;
    let Some(current) = current else {
        return Ok(());
    };
    if current.status == MemoryStatus::Unconfirmed && !current.confirmed {
        tx.execute(
            "UPDATE memories SET superseded_at = ?2, superseded_by = ?3 WHERE id = ?1",
            params![current.id, now, m.id],
        )?;
        return Ok(());
    }
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM conflicts WHERE resolved_at IS NULL \
         AND ((a = ?1 AND b = ?2) OR (a = ?2 AND b = ?1)))",
        params![current.id, m.id],
        |r| r.get(0),
    )?;
    if !exists {
        tx.execute(
            "INSERT INTO conflicts(id, a, b, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![new_id("cfl"), current.id, m.id, now],
        )?;
        clear_snooze(tx, &[&current.id, &m.id])?;
    }
    Ok(())
}

pub(crate) fn mem_views(conn: &rusqlite::Connection, rows: &[MemRow]) -> Result<Vec<MemoryView>> {
    rows.iter().map(|r| mem_view(conn, r)).collect()
}

impl Engine {
    /// Turn what the mind found in an item into memories. The caller must hold the item's lease.
    /// Idempotent per item: it replaces what the same item said the last time it was applied.
    /// What a re-read no longer supports is deleted only if nothing depends on it (see
    /// `apply`); otherwise it goes back to unconfirmed with a link to the item.
    pub fn apply_candidates(
        &self,
        item: &str,
        lease: &str,
        candidates: Vec<Candidate>,
    ) -> Result<ApplyReport> {
        self.apply_timed(item, lease, candidates, None)
    }

    /// `apply_candidates` for a reading that took `took` so far: the item becomes understood
    /// and timed in one step, so it never shows as understood without its time.
    pub fn apply_candidates_took(
        &self,
        item: &str,
        lease: &str,
        candidates: Vec<Candidate>,
        took: std::time::Duration,
    ) -> Result<ApplyReport> {
        self.apply_timed(item, lease, candidates, Some(took))
    }

    fn apply_timed(
        &self,
        item: &str,
        lease: &str,
        candidates: Vec<Candidate>,
        took: Option<std::time::Duration>,
    ) -> Result<ApplyReport> {
        let now = fmt_ts(self.clock().now());
        let tz = self.clock().tz();
        let today = self.clock().now().to_zoned(tz.clone()).date().to_string();
        let (report, files) =
            self.write(|tx| apply(tx, &now, &today, &tz, item, lease, candidates, took))?;
        crate::audio::shred_all(files);
        if !report.removed.is_empty() || !report.reworded.is_empty() || !report.replaced.is_empty()
        {
            self.invalidate_vectors();
        }
        self.bump_audio();
        Ok(report)
    }

    /// Current memories, newest first. Browsing does not count as use.
    pub fn memories(&self, filter: MemoryFilter) -> Result<Vec<MemoryView>> {
        self.read(|conn| {
            let mut sql =
                format!("SELECT {MEM_COLS} FROM memories m WHERE m.superseded_at IS NULL");
            let mut args: Vec<String> = Vec::new();
            if let Some(e) = &filter.entity_id {
                args.push(e.clone());
                sql.push_str(&format!(" AND m.entity_id = ?{}", args.len()));
            }
            if let Some(i) = &filter.item_id {
                args.push(i.clone());
                sql.push_str(&format!(
                    " AND m.id IN (SELECT memory_id FROM memory_sources WHERE item_id = ?{})",
                    args.len()
                ));
            }
            if let Some(s) = &filter.status {
                args.push(enum_str(s));
                sql.push_str(&format!(" AND m.status = ?{}", args.len()));
            }
            if let Some(q) = filter.query.as_deref().and_then(fts_and) {
                args.push(q);
                sql.push_str(&format!(
                    " AND m.rowid IN (SELECT rowid FROM memories_fts WHERE memories_fts MATCH ?{})",
                    args.len()
                ));
            }
            sql.push_str(" ORDER BY m.recorded_at DESC, m.id");
            if let Some(limit) = filter.limit {
                sql.push_str(&format!(" LIMIT {limit}"));
            }
            let mut stmt = conn.prepare(&sql)?;
            let rows: Vec<MemRow> = stmt
                .query_map(rusqlite::params_from_iter(args.iter()), mem_row)?
                .collect::<rusqlite::Result<_>>()?;
            mem_views(conn, &rows)
        })
    }

    pub fn get_memory(&self, id: &str) -> Result<MemoryView> {
        self.read(|conn| {
            let row =
                load_mem(conn, id)?.ok_or_else(|| Error::not_found(format!("memory {id}")))?;
            mem_view(conn, &row)
        })
    }

    /// The finding model new vectors are made with. The app sets it when the mind is filled.
    pub fn set_recall_model(&self, model: &str) {
        *self
            .inner
            .recall_model
            .write()
            .unwrap_or_else(|e| e.into_inner()) = model.to_string();
    }

    pub fn recall_model(&self) -> String {
        self.inner
            .recall_model
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Store vectors the current finding model made. See [`Engine::store_embeddings_from`].
    pub fn store_embeddings(&self, rows: Vec<(String, Vec<f32>)>) -> Result<()> {
        let model = self.recall_model();
        self.store_embeddings_from(&model, rows).map(|_| ())
    }

    /// Store vectors that `model` made, in chunks so capture never waits long, marked with it.
    /// Unknown ids are ignored. Vectors from a model that is no longer the finding model (it
    /// changed while they were being made) are dropped: those memories stay without a vector of
    /// the current model, and the backfill makes them again. A memory given a vector here (new,
    /// or re-worded) is read again by a re-read under way. Returns how many were kept.
    pub fn store_embeddings_from(
        &self,
        model: &str,
        rows: Vec<(String, Vec<f32>)>,
    ) -> Result<usize> {
        let mut kept = 0;
        for chunk in rows.chunks(200) {
            // Held while writing, so the swap of a re-read cannot change the model in between.
            let current = self
                .inner
                .recall_model
                .read()
                .unwrap_or_else(|e| e.into_inner());
            if *current != model {
                return Ok(kept);
            }
            self.write(|tx| {
                for (id, vec) in chunk {
                    let blob: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
                    tx.execute(
                        "INSERT OR REPLACE INTO embeddings(memory_id, dim, vec, model) \
                         SELECT id, ?2, ?3, ?4 FROM memories WHERE id = ?1",
                        params![id, vec.len() as i64, blob, model],
                    )?;
                    tx.execute("DELETE FROM embeddings_next WHERE memory_id = ?1", [id])?;
                }
                Ok(())
            })?;
            drop(current);
            kept += chunk.len();
        }
        self.invalidate_vectors();
        Ok(kept)
    }

    /// Memories that need a vector from the current finding model, newest first.
    pub fn memories_without_embedding(&self, limit: usize) -> Result<Vec<(String, String)>> {
        let model = self.recall_model();
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT m.id, m.sentence FROM memories m \
                 WHERE m.superseded_at IS NULL \
                   AND NOT EXISTS (SELECT 1 FROM embeddings e \
                                   WHERE e.memory_id = m.id AND e.model = ?2) \
                 ORDER BY m.recorded_at DESC, m.id LIMIT ?1",
            )?;
            let rows =
                stmt.query_map(params![limit as i64, model], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    /// Everything waiting on the person, ranked: due soonest, then promises, then "which is
    /// right?", then unconfirmed. Entries are not-done by construction.
    pub fn needs_you(&self) -> Result<Vec<NeedsYouEntry>> {
        let tz = self.clock().tz();
        let today = self.clock().now().to_zoned(tz).date();
        self.read(|conn| needs_you_on(conn, today))
    }

    /// Hide a memory from `needs_you` and the morning page for `days` days, counted from the
    /// local date today (0 brings it back at once). Confirming, correcting or a new conflict
    /// about it clears the snooze.
    pub fn snooze(&self, memory_id: &str, days: u32) -> Result<()> {
        let until = self
            .clock()
            .now()
            .to_zoned(self.clock().tz())
            .date()
            .checked_add(jiff::Span::new().days(i64::from(days)))
            .map_err(Error::from)?
            .to_string();
        self.write(|tx| {
            load_mem(tx, memory_id)?
                .filter(|m| m.superseded_at.is_none())
                .ok_or_else(|| Error::not_found(format!("memory {memory_id}")))?;
            tx.execute(
                "UPDATE memories SET snoozed_until = ?2 WHERE id = ?1",
                params![memory_id, until],
            )?;
            Ok(())
        })
    }

    /// The person says it is true (or not). True confirms and counts as use; false makes it
    /// unconfirmed again and asks again.
    pub fn confirm(&self, id: &str, is_true: bool) -> Result<()> {
        let now = fmt_ts(self.clock().now());
        let today = self.today();
        self.write(|tx| confirm_tx(tx, id, is_true, &now, &today))?;
        self.bump_audio();
        Ok(())
    }

    /// Choose which of two conflicting memories is right. The other stays as history.
    pub fn resolve_which(&self, conflict: &str, keep: &str) -> Result<()> {
        let now = fmt_ts(self.clock().now());
        self.write(|tx| {
            let row: Option<(String, String, Option<String>)> = tx
                .query_row(
                    "SELECT a, b, resolved_at FROM conflicts WHERE id = ?1",
                    [conflict],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let (a, b, resolved) =
                row.ok_or_else(|| Error::not_found(format!("conflict {conflict}")))?;
            if resolved.is_some() {
                return Err(Error::invalid("that choice has already been made"));
            }
            let other = if keep == a {
                b
            } else if keep == b {
                a
            } else {
                return Err(Error::invalid("the memory is not part of that choice"));
            };
            tx.execute(
                "UPDATE memories SET superseded_at = ?2, superseded_by = ?3 \
                 WHERE id = ?1",
                params![other, now, keep],
            )?;
            tx.execute(
                "UPDATE memories SET confirmed = 1, status = 'active', last_used = ?2, \
                 let_go_at = NULL WHERE id = ?1",
                params![keep, now],
            )?;
            tx.execute(
                "UPDATE conflicts SET resolved_at = ?2, kept = ?3 \
                 WHERE resolved_at IS NULL AND (a = ?1 OR b = ?1)",
                params![other, now, keep],
            )?;
            Ok(())
        })?;
        self.invalidate_vectors();
        self.bump_audio();
        Ok(())
    }

    /// The person corrects a memory. The old row closes into history (its dates untouched: it
    /// was wrong, not ended) and the new row cites the same sources. `new_object` replaces the
    /// old object inside the sentence when it appears there, otherwise it becomes the sentence.
    pub fn correct(&self, id: &str, patch: CorrectPatch) -> Result<String> {
        let new_object = patch.object.trim().to_string();
        if new_object.is_empty() {
            return Err(Error::invalid("a correction needs some text"));
        }
        let now = fmt_ts(self.clock().now());
        let created = self.write(|tx| {
            let old = load_mem(tx, id)?.ok_or_else(|| Error::not_found(format!("memory {id}")))?;
            if old.superseded_at.is_some() {
                return Err(Error::invalid("only a current memory can be corrected"));
            }
            let sentence = match patch.sentence.as_deref().map(str::trim) {
                Some(s) if !s.is_empty() => s.to_string(),
                _ if !old.object.is_empty() && old.sentence.contains(&old.object) => {
                    old.sentence.replacen(&old.object, &new_object, 1)
                }
                _ => new_object.clone(),
            };
            clear_snooze(tx, &[id])?;
            let new_id = new_memory_id();
            tx.execute(
                "INSERT INTO memories(id, subject, subject_key, entity_id, predicate, object, \
                 object_key, sentence, valid_from, valid_to, due, recorded_at, confidence, status, \
                 confirmed, last_used, done, keep_forever, private, suggest_private, tags) \
                 SELECT ?2, subject, subject_key, entity_id, predicate, ?3, ?4, ?5, valid_from, \
                 valid_to, due, ?6, 1.0, 'active', 1, ?6, done, keep_forever, private, \
                 suggest_private, tags FROM memories WHERE id = ?1",
                params![id, new_id, new_object, key_of(&new_object), sentence, now],
            )?;
            for (column, value) in [
                ("due", patch.due),
                ("valid_from", patch.valid_from),
                ("valid_to", patch.valid_to),
            ] {
                if let Some(v) = value {
                    tx.execute(
                        &format!("UPDATE memories SET {column} = ?2 WHERE id = ?1"),
                        params![new_id, date_str(v)],
                    )?;
                }
            }
            tx.execute(
                "INSERT INTO memory_sources(memory_id, item_id, span) \
                 SELECT ?2, item_id, span FROM memory_sources WHERE memory_id = ?1",
                params![id, new_id],
            )?;
            tx.execute(
                "UPDATE memories SET superseded_at = ?2, superseded_by = ?3 WHERE id = ?1",
                params![id, now, new_id],
            )?;
            tx.execute(
                "UPDATE conflicts SET a = ?2 WHERE a = ?1 AND resolved_at IS NULL",
                params![id, new_id],
            )?;
            tx.execute(
                "UPDATE conflicts SET b = ?2 WHERE b = ?1 AND resolved_at IS NULL",
                params![id, new_id],
            )?;
            Ok(new_id)
        })?;
        self.invalidate_vectors();
        self.bump_audio();
        Ok(created)
    }

    /// Every version of a memory, newest first: what it is now, and what it replaced or ended.
    pub fn history(&self, memory_id: &str) -> Result<Vec<MemoryView>> {
        self.read(|conn| {
            let ids = chain(conn, memory_id)?;
            if ids.is_empty() {
                return Err(Error::not_found(format!("memory {memory_id}")));
            }
            let mut rows = Vec::new();
            for id in &ids {
                if let Some(r) = load_mem(conn, id)? {
                    rows.push(r);
                }
            }
            rows.sort_by(|a, b| {
                b.recorded_at
                    .cmp(&a.recorded_at)
                    .then_with(|| a.id.cmp(&b.id))
            });
            mem_views(conn, &rows)
        })
    }

    /// The same entries as `needs_you`, without anything private or from the journal, for places
    /// that must never show it: notifications, the tray, a locked window.
    pub fn needs_you_public(&self) -> Result<Vec<NeedsYouEntry>> {
        let tz = self.clock().tz();
        let today = self.clock().now().to_zoned(tz).date();
        self.read(|conn| {
            let mut out = Vec::new();
            for entry in needs_you_on(conn, today)? {
                let private = match &entry {
                    NeedsYouEntry::Due { memory, .. }
                    | NeedsYouEntry::Promise { memory, .. }
                    | NeedsYouEntry::Unconfirmed { memory, .. } => {
                        is_private_chain(conn, &memory.id)? || is_journal_chain(conn, &memory.id)?
                    }
                    NeedsYouEntry::Which { options, .. } => {
                        let mut any = false;
                        for o in options {
                            any |= is_private_chain(conn, &o.memory_id)?
                                || is_journal_chain(conn, &o.memory_id)?;
                        }
                        any
                    }
                    // A name question quotes the journal, which never reaches these places.
                    NeedsYouEntry::SameName { .. } => true,
                };
                if !private {
                    out.push(entry);
                }
            }
            Ok(out)
        })
    }

    /// Tick off a due or promised memory. Counts as use.
    pub fn mark_done(&self, memory_id: &str) -> Result<()> {
        let now = fmt_ts(self.clock().now());
        self.write(|tx| {
            let n = tx.execute(
                "UPDATE memories SET done = 1, last_used = ?2, \
                 status = CASE WHEN status IN ('fading', 'letGo') THEN 'active' ELSE status END, \
                 let_go_at = CASE WHEN status = 'letGo' THEN NULL ELSE let_go_at END \
                 WHERE id = ?1",
                params![memory_id, now],
            )?;
            if n == 0 {
                return Err(Error::not_found(format!("memory {memory_id}")));
            }
            tx.execute(
                "UPDATE page_lines SET done = 1 WHERE id IN \
                 (SELECT line_id FROM page_line_memories WHERE memory_id = ?1)",
                [memory_id],
            )?;
            Ok(())
        })
    }

    /// People, places and things the person has kept things about.
    pub fn entities(&self, kind: Option<EntityKind>) -> Result<Vec<EntityView>> {
        self.entities_matching(kind, None)
    }

    /// Entities whose name or role contains `query` (case-insensitive; blank means all),
    /// optionally of one kind. Most memories first, then by name.
    pub fn entities_matching(
        &self,
        kind: Option<EntityKind>,
        query: Option<&str>,
    ) -> Result<Vec<EntityView>> {
        let needle = query
            .map(|q| q.trim().to_lowercase())
            .filter(|q| !q.is_empty());
        let mut out: Vec<EntityView> = self.read(|conn| entity_views(conn, kind, None))?;
        if let Some(n) = &needle {
            out.retain(|e| {
                e.name.to_lowercase().contains(n.as_str())
                    || e.former_names
                        .iter()
                        .any(|f| f.to_lowercase().contains(n.as_str()))
                    || e.role
                        .as_ref()
                        .is_some_and(|r| r.to_lowercase().contains(n.as_str()))
            });
        }
        out.sort_by(|a, b| {
            b.memory_count
                .cmp(&a.memory_count)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(out)
    }
}

fn new_memory_id() -> String {
    new_id("mem")
}

#[cfg(test)]
mod tests {
    use super::{reports_on, values_disagree};

    #[test]
    fn a_report_is_about_what_follows_the_word_that_says_so() {
        let ravi = "Ravi thinks choir practice is on Friday evenings at the school hall.";
        assert!(reports_on(ravi, "Ravi", "Choir practice"));
        assert!(reports_on(
            "Sam says that the book club is actually on Wednesdays.",
            "Sam",
            "Book club"
        ));
        assert!(!reports_on(
            "Sam says Leo's party is on Friday.",
            "Sam",
            "Leo"
        ));
        assert!(!reports_on(ravi, "Sam", "Choir practice"));
        assert!(!reports_on(
            "Choir practice is on Thursday evenings.",
            "Choir practice",
            "Choir practice"
        ));
    }

    #[test]
    fn values_that_differ_disagree() {
        assert!(values_disagree(
            "the first Tuesday of the month",
            "second Wednesday of the month"
        ));
        assert!(values_disagree("Wednesday at 10:00", "Wednesday at 14:30"));
        assert!(values_disagree("bay 7", "bay 9"));
        assert!(values_disagree("Fridays", "Thursday evenings"));
        assert!(values_disagree("1st Tuesday", "2nd Tuesday"));
        assert!(values_disagree("Sat 14 November", "Sun 15 November"));
        assert!(values_disagree("18 November", "18 December"));
        assert!(values_disagree("May 3", "June 3"));
        assert!(values_disagree("2:30am", "14:30"));
        assert!(values_disagree(
            "the first of the month",
            "the second of the month"
        ));
    }

    #[test]
    fn everyday_words_dates_and_money_are_no_values() {
        assert!(!values_disagree("the sat nav", "the sun room"));
        assert!(!values_disagree("first prize", "second opinion"));
        assert!(!values_disagree("you may go", "in June"));
        assert!(!values_disagree("£12.50", "12:50"));
        assert!(!values_disagree("30 pounds", "45"));
        assert!(!values_disagree("18/11/2026", "19/11/2026"));
        assert!(!values_disagree("18.11.2026", "19.11.2026"));
        assert!(!values_disagree("18-11-2026", "4"));
    }

    #[test]
    fn saying_less_or_the_same_another_way_agrees() {
        assert!(!values_disagree(
            "Wednesday 18 November at 10:00",
            "Wed 18 Nov, 10am"
        ));
        assert!(!values_disagree("2:30pm", "14:30"));
        assert!(!values_disagree("2:30", "14:30"));
        assert!(!values_disagree("2:30", "2:30am"));
        assert!(!values_disagree("1st Tuesday", "first Tuesday"));
        assert!(!values_disagree("14.30", "14:30"));
        assert!(!values_disagree("12 noon", "midday"));
        assert!(!values_disagree(
            "18 November",
            "Wednesday 18 November at 10:00"
        ));
        assert!(!values_disagree("Friday", "Fri 14th"));
        assert!(!values_disagree("cholesterol 5.9", "5.9"));
        assert!(!values_disagree("the library", "the community hall"));
    }
}
