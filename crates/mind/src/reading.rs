//! Reading a reply to an item: the memories in it, each checked against the words of the item
//! it must quote, and what to do with a reply that was cut off.

use jiff::civil::Date;
use nocturne_contract::{EntityKind, Span};
use nocturne_engine::coverage::{self, normalise};
use nocturne_engine::{Candidate, ExtractContext, Predicate};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{guard, watch};

/// Models sometimes wrap JSON in a code fence; take what is inside.
pub fn json_text<T: DeserializeOwned>(text: &str) -> Result<T, serde_json::Error> {
    let trimmed = text.trim();
    let inner = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|t| t.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    serde_json::from_str(inner)
}

#[derive(Deserialize)]
struct Extraction {
    memories: Vec<Value>,
}

/// `more` added to `kept`, leaving out what `kept` already says.
pub fn merged(mut kept: Vec<Candidate>, more: Vec<Candidate>) -> Vec<Candidate> {
    let key = |c: &Candidate| {
        (
            normalise(&c.subject),
            c.predicate.as_str().to_string(),
            normalise(&c.object),
        )
    };
    for c in more {
        if !kept.iter().any(|k| key(k) == key(&c)) {
            kept.push(c);
        }
    }
    kept
}

/// Whether `found` keeps all of `text`: every number, name and role in it, by the check the
/// reader gives a reading before a second look, and every sentence of it quoted, or the reading
/// carried on to its last sentence. The first check alone misses a sentence with no number or
/// name in it, and counts a name another memory happens to hold.
pub fn covered(text: &str, found: &[Candidate], cx: &ExtractContext) -> bool {
    let known: Vec<String> = cx.known_entities.iter().map(|(n, _)| n.clone()).collect();
    if found.is_empty() || !coverage::left_out(text, found, &known).is_empty() {
        return false;
    }
    let quote = |c: &Candidate| match &c.span {
        Some(Span::Quote { text }) => normalise(text),
        _ => String::new(),
    };
    let quotes: Vec<String> = found.iter().map(quote).filter(|q| !q.is_empty()).collect();
    let sentences: Vec<String> = coverage::sentences(text)
        .iter()
        .map(|s| normalise(s))
        .filter(|s| !s.is_empty())
        .collect();
    let holds = |s: &str, q: &str| s.contains(q) || q.contains(s);
    let every = sentences.iter().all(|s| quotes.iter().any(|q| holds(s, q)));
    let to_the_end = match (sentences.last(), found.last().map(quote)) {
        (Some(last), Some(q)) => !q.is_empty() && holds(last, &q),
        _ => false,
    };
    every || to_the_end
}

/// `text` in two parts of about the same length, split between lines or else between
/// sentences. `None` for a single sentence.
pub fn halves(text: &str) -> Option<(&str, &str)> {
    let middle = text.len() / 2;
    let mut cuts: Vec<usize> = text.match_indices('\n').map(|(i, _)| i + 1).collect();
    if cuts.iter().all(|&c| c >= text.trim_end().len()) {
        cuts = text
            .match_indices(['.', '!', '?'])
            .map(|(i, _)| i + 1)
            .filter(|&i| text[i..].starts_with(char::is_whitespace))
            .collect();
    }
    let cut = cuts
        .into_iter()
        .filter(|&c| !text[..c].trim().is_empty() && !text[c..].trim().is_empty())
        .min_by_key(|&c| c.abs_diff(middle))?;
    Some((text[..cut].trim(), text[cut..].trim()))
}

/// The whole, valid and grounded memories in a reply that was cut off part way.
pub fn salvage(reply: &str, text: &str) -> Vec<Candidate> {
    let source = normalise(text);
    watch::Memories::default()
        .whole(reply)
        .into_iter()
        .filter_map(|r| serde_json::from_str::<Value>(&reply[r]).ok())
        .filter_map(|raw| candidate(&raw).ok())
        .filter(|c| grounded(c, &source))
        .map(|c| own_dates(dated(c, text), text))
        .collect()
}

/// A memory must be able to point at the words it came from.
pub fn grounded(c: &Candidate, source: &str) -> bool {
    match &c.span {
        Some(Span::Quote { text }) => {
            let quote = normalise(text);
            !quote.is_empty() && source.contains(&quote)
        }
        _ => false,
    }
}

/// What a reply came to.
#[derive(Debug)]
pub struct Parsed {
    /// Valid memories whose quote is really in the item.
    pub items: Vec<Candidate>,
    /// A note for each memory that did not validate.
    pub invalid: Vec<String>,
    /// How many memories the reply held, whatever came of them.
    pub total: usize,
}

/// The memories in `reply` to `text`. Memories quoting words that are not there are dropped
/// quietly, and a day the item never names is taken off the memory that gives it.
pub fn parse_candidates(reply: &str, text: &str) -> Result<Parsed, serde_json::Error> {
    let parsed: Extraction = json_text(reply)?;
    let source = normalise(text);
    let mut items = Vec::new();
    let mut invalid = Vec::new();
    for (i, raw) in parsed.memories.iter().enumerate() {
        match candidate(raw) {
            Ok(c) if grounded(&c, &source) => items.push(own_dates(dated(c, text), text)),
            Ok(c) => tracing::warn!("dropped an ungrounded memory about {}", c.subject),
            Err(why) => invalid.push(format!("memory {}: {why}", i + 1)),
        }
    }
    Ok(Parsed {
        items,
        invalid,
        total: parsed.memories.len(),
    })
}

/// `c` with each date its sentence writes as YYYY-MM-DD, where the item does not, put back in
/// the words the item writes that day in ("1 December"): the person's own words are what an
/// answer gives and a search finds. The object is left as it is: the store tells memories
/// apart by it, and one object for a day must not become two.
pub fn own_dates(mut c: Candidate, text: &str) -> Candidate {
    c.sentence = restored(&c.sentence, text);
    c
}

pub fn restored(words: &str, text: &str) -> String {
    let mut out = words.to_string();
    let bytes = words.as_bytes();
    let digit = |i: usize| bytes.get(i).is_some_and(u8::is_ascii_digit);
    for at in (0..bytes.len().saturating_sub(9)).rev() {
        let shaped = (0..10).all(|k| {
            if k == 4 || k == 7 {
                bytes[at + k] == b'-'
            } else {
                digit(at + k)
            }
        });
        if !shaped || (at > 0 && digit(at - 1)) || digit(at + 10) {
            continue;
        }
        let iso = &words[at..at + 10];
        if text.contains(iso) {
            continue;
        }
        if let Ok(date) = iso.parse::<Date>()
            && let Some(own) = guard::written_day(date, text)
        {
            out.replace_range(at..at + 10, &own);
        }
    }
    out
}

/// `c` without a `due` that neither its quote nor the line it stands in (with the heading
/// above that line) names.
pub fn dated(mut c: Candidate, text: &str) -> Candidate {
    if let Some(due) = c.due {
        let quote = match &c.span {
            Some(Span::Quote { text }) => text.as_str(),
            _ => "",
        };
        if !guard::day_supported(due, &around(quote, text)) {
            c.due = None;
        }
    }
    c
}

/// `quote` with the line of `text` it stands in and the nearest line with words above that.
pub fn around(quote: &str, text: &str) -> String {
    let q = normalise(quote);
    let lines: Vec<&str> = text.lines().collect();
    let at = lines.iter().position(|l| {
        let l = normalise(l);
        !l.is_empty() && !q.is_empty() && (l.contains(&q) || q.contains(&l))
    });
    let mut out = quote.to_string();
    if let Some(i) = at {
        out.push_str(" | ");
        out.push_str(lines[i]);
        if let Some(heading) = lines[..i].iter().rev().find(|l| !l.trim().is_empty()) {
            out.push_str(" | ");
            out.push_str(heading);
        }
    }
    out
}

fn text_field(raw: &Value, key: &str) -> Result<String, String> {
    match raw[key].as_str().map(str::trim) {
        Some(s) if !s.is_empty() => Ok(s.to_string()),
        _ => Err(format!("{key} is missing")),
    }
}

fn date_field(raw: &Value, key: &str) -> Result<Option<Date>, String> {
    match raw[key].as_str().map(str::trim) {
        None | Some("") => Ok(None),
        Some(s) => s
            .parse::<Date>()
            .map(Some)
            .map_err(|_| format!("{key} is not a YYYY-MM-DD date")),
    }
}

/// Words that stand for someone or something named elsewhere: a memory about one of them cannot
/// say who or what it is about.
pub const PRONOUNS: [&str; 16] = [
    "he", "she", "it", "they", "him", "her", "them", "his", "hers", "its", "their", "theirs",
    "this", "that", "these", "those",
];

/// The reason names no words of the model's own: the retry goes back as the person's turn.
pub fn named_subject(subject: &str) -> Result<(), String> {
    if PRONOUNS.contains(&subject.trim().to_lowercase().as_str()) {
        return Err("the subject is a pronoun; name who or what it stands for".into());
    }
    Ok(())
}

pub fn candidate(raw: &Value) -> Result<Candidate, String> {
    let sentence = text_field(raw, "sentence")?;
    let subject = text_field(raw, "subject")?;
    named_subject(&subject)?;
    let object = text_field(raw, "object")?;
    let predicate = predicate(&text_field(raw, "predicate")?);
    let confidence = raw["confidence"]
        .as_f64()
        .ok_or("confidence is missing")?
        .clamp(0.0, 1.0) as f32;
    let subject_kind = match raw["subjectKind"].as_str() {
        Some("person") => Some(EntityKind::Person),
        Some("place") => Some(EntityKind::Place),
        Some("thing") => Some(EntityKind::Thing),
        _ => None,
    };
    let span = raw["span"]["quote"]
        .as_str()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(|q| Span::Quote {
            text: q.to_string(),
        });
    let tags = raw["tags"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| t.as_str())
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();
    Ok(Candidate {
        subject,
        subject_kind,
        predicate,
        object,
        sentence,
        valid_from: date_field(raw, "validFrom")?,
        valid_to: date_field(raw, "validTo")?,
        due: date_field(raw, "due")?,
        confidence,
        span,
        tags,
        moved: raw["moved"].as_bool().unwrap_or(false),
    })
}

pub fn predicate(raw: &str) -> Predicate {
    let norm = raw.trim().to_lowercase().replace([' ', '-'], "_");
    match norm.strip_prefix("other:") {
        Some(word) => Predicate::Other(word.trim().to_string()),
        None => Predicate::parse(&norm),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts;
    use serde_json::json;

    #[test]
    fn the_worked_example_is_a_reply_the_mind_would_keep_whole() {
        let p = parse_candidates(prompts::EXAMPLE_REPLY, prompts::EXAMPLE_NOTE).unwrap();
        assert!(p.invalid.is_empty(), "{:?}", p.invalid);
        assert_eq!(p.items.len(), 3, "every memory in it is grounded");
        assert_eq!(p.total, 3);
        let dues: Vec<Option<Date>> = p.items.iter().map(|c| c.due).collect();
        assert_eq!(
            dues,
            [
                Some("2026-03-06".parse().unwrap()),
                Some("2026-03-13".parse().unwrap()),
                None
            ],
            "every day it gives is one the note names"
        );
        let system = prompts::extraction_system("n");
        assert!(system.contains(prompts::EXAMPLE_NOTE) && system.contains(prompts::EXAMPLE_REPLY));
    }

    /// Who someone is to the person stays in the memory, so "who is" has something to cite.
    #[test]
    fn who_someone_is_stays_in_the_sentence() {
        let items = parse_candidates(prompts::EXAMPLE_REPLY, prompts::EXAMPLE_NOTE)
            .unwrap()
            .items;
        assert!(
            items
                .iter()
                .any(|c| c.sentence.starts_with("My cousin Ana"))
        );
        let system = prompts::extraction_system("n");
        assert!(system.contains("Keep the words that say who someone is to the person"));
    }

    /// The grammar writes each memory's keys in reading order, the quote first, and the worked
    /// example the prompt shows is written the same way.
    #[test]
    fn memories_are_written_in_reading_order_within_bounds() {
        let grammar = prompts::extraction_grammar();
        let memory = grammar
            .lines()
            .find(|l| l.starts_with("memory ::="))
            .unwrap();
        let at: Vec<usize> = prompts::MEMORY_KEYS
            .iter()
            .map(|k| memory.find(&format!("\"\\\"{k}\\\"\"")).unwrap())
            .collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "{memory}");
        assert!(grammar.contains("char{1,300}"), "{grammar}");
        assert!(grammar.contains("(\",\" ws memory){0,15}"), "{grammar}");
        let example: Value = serde_json::from_str(prompts::EXAMPLE_REPLY).unwrap();
        let reply = prompts::EXAMPLE_REPLY;
        let first = &example["memories"][0];
        let mut last = 0;
        for k in prompts::MEMORY_KEYS {
            assert!(first.get(k).is_some(), "{k}");
            let pos = reply.find(&format!("\"{k}\":")).unwrap();
            assert!(pos > last || k == "span", "{k} out of order");
            last = pos;
        }
    }

    #[test]
    fn a_date_rewritten_by_the_mind_is_put_back_in_the_items_words() {
        // Tiny's reading of the allotment newsletter in a real run.
        let text = "Riverside Allotment Society, autumn newsletter\n\nOur plot is number 17, next \
                    to the water tap.\n\nThe water is switched off for winter on 1 December.\n\n\
                    The annual fee of 30 pounds is due by 15 January.";
        assert_eq!(
            restored("The water is switched off for winter on 2026-12-01.", text),
            "The water is switched off for winter on 1 December."
        );
        assert_eq!(restored("by 2026-01-15", text), "by 15 January");
        assert_eq!(
            restored("Kept on 2026-11-12, due 2026-01-15.", text),
            "Kept on 2026-11-12, due 15 January.",
            "a day the item does not write stays as it is"
        );
        assert_eq!(restored("on 2026-01-150", text), "on 2026-01-150");
    }

    #[test]
    fn only_the_sentence_gets_the_items_words_for_a_day() {
        let text = "The water is switched off for winter on 1 December.";
        let raw = serde_json::json!({
            "span": {"quote": "The water is switched off for winter on 1 December"},
            "sentence": "The water is switched off on 2026-12-01.", "subject": "Water",
            "predicate": "scheduled", "object": "2026-12-01", "due": "2026-12-01",
            "confidence": 0.9,
        });
        let c = own_dates(candidate(&raw).unwrap(), text);
        assert_eq!(c.sentence, "The water is switched off on 1 December.");
        assert_eq!(
            c.object, "2026-12-01",
            "the object the store tells memories apart by"
        );
    }

    #[test]
    fn a_day_the_item_does_not_name_is_dropped_and_the_memory_kept() {
        let reply = |due: &str, quote: &str| {
            json!({"memories": [{"span": {"quote": quote}, "sentence": "Book club meets.",
                "subject": "Book club", "subjectKind": "thing", "predicate": "scheduled",
                "object": "first Tuesday", "due": due, "validFrom": null, "validTo": null,
                "moved": false, "confidence": 0.9, "tags": []}]})
            .to_string()
        };
        let text = "Book club\nIt meets on the first Tuesday.";
        let p = parse_candidates(&reply("2026-11-12", "It meets on the first Tuesday"), text);
        assert_eq!(p.unwrap().items[0].due, None);
        // The heading above the quoted line names the day.
        let text = "Tuesday 17 November\nBook club at the library.";
        let p = parse_candidates(&reply("2026-11-17", "Book club at the library"), text);
        assert_eq!(p.unwrap().items[0].due, Some("2026-11-17".parse().unwrap()));
    }

    #[test]
    fn the_whole_memories_of_a_cut_reply_are_kept() {
        let note = prompts::EXAMPLE_NOTE;
        let cut = &prompts::EXAMPLE_REPLY[..prompts::EXAMPLE_REPLY.len() - 60];
        assert!(parse_candidates(cut, note).is_err());
        let kept = salvage(cut, note);
        assert_eq!(kept.len(), 2, "{kept:?}");
        assert_eq!(kept[1].subject, "Dr Patel");
        assert!(salvage("{\"memories\": [{\"span\": {\"quote\": \"a}b", note).is_empty());
    }

    #[test]
    fn an_item_halves_between_lines_or_else_between_sentences() {
        assert_eq!(
            halves("one\ntwo\nthree\nfour"),
            Some(("one\ntwo", "three\nfour"))
        );
        assert_eq!(
            halves("First fact here. Second fact. Third fact here."),
            Some(("First fact here. Second fact.", "Third fact here."))
        );
        assert_eq!(halves("One sentence only."), None);
        assert_eq!(halves("One line\n"), None);
    }

    #[test]
    fn every_predicate_offered_is_one_the_engine_knows() {
        for (name, _) in prompts::PREDICATES {
            assert!(!predicate(name).is_other(), "{name}");
        }
    }

    #[test]
    fn a_pronoun_is_not_a_subject_but_a_name_that_starts_like_one_is() {
        for bad in ["He", " they", "It", "this"] {
            assert!(named_subject(bad.trim()).is_err(), "{bad}");
        }
        for good in [
            "Tom",
            "Gran's birthday",
            "Theatre trip",
            "Hermione",
            "Itinerary",
            "Her Majesty's Passport Office",
            "His Majesty's Theatre",
        ] {
            assert!(named_subject(good).is_ok(), "{good}");
        }
    }
}
