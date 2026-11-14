//! The seeded day, what a perfect reading of it holds, and the scores for each stage of
//! answering: what reading kept (gold words per item), the links the store formed (moves and
//! which-is-right questions), whether the search found the source (recall at 4 and 12), and the
//! answer step alone on frozen, correct facts. Shared by the real run (`e2e_real.rs`) and the
//! fixture test that runs without models (`eval_fixture.rs`).
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jiff::civil::Date;
use nocturne_contract::{Kept, MemoryFilter, Source};
use nocturne_engine::{Candidate, Engine, Fact, Hit, ManualClock, Mind, Predicate};
use serde_json::{Map, Value, json};
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::Context;

/// The day everything is kept on.
pub const DAY: &str = "2026-11-12";

// ---------------------------------------------------------------------------------------------
// The seeded day. Everything is made up; nothing here is a real person's data.

pub struct Note {
    pub key: &'static str,
    pub at: &'static str,
    pub text: &'static str,
}

pub const NOTES: &[Note] = &[
    Note {
        key: "dentist",
        at: "08:05",
        text: "Dentist appointment with Dr Okafor on Wednesday 18 November at 10:00.",
    },
    Note {
        key: "gran",
        at: "08:40",
        text: "Gran lives in Leeds. Her birthday is on 3 December.",
    },
    Note {
        key: "trip",
        at: "09:10",
        text: "Leo's school trip to the science museum is on Monday 23 November. He needs a packed lunch.",
    },
    Note {
        key: "priya",
        at: "09:55",
        text: "I promised Priya I would send her the budget spreadsheet by Friday 20 November.",
    },
    Note {
        key: "bakery",
        at: "10:30",
        text: "Bakery order for Saturday: two sourdough loaves and one seeded rye, collect before noon from Fenn Street Bakery.",
    },
    Note {
        key: "club1",
        at: "11:15",
        text: "Book club meets on the first Tuesday of the month at the library.",
    },
    Note {
        key: "dentist_moved",
        at: "12:00",
        text: "The dentist appointment has moved. Dr Okafor's receptionist called: it is now Thursday 19 November at 14:30, not Wednesday 18 November.",
    },
    Note {
        key: "blood",
        at: "12:40",
        text: "My blood test showed high cholesterol, 5.9. The doctor wants a recheck in March.",
    },
    Note {
        key: "gran_moved",
        at: "13:20",
        text: "Gran has moved from Leeds to York as of 1 November. She now lives in York.",
    },
    Note {
        key: "swim",
        at: "14:00",
        text: "Leo swims on Mondays with Coach Dee after school.",
    },
    Note {
        key: "paint",
        at: "14:45",
        text: "Decision: we will repaint the hallway in sage green, not grey.",
    },
    Note {
        key: "boiler",
        at: "15:30",
        text: "Boiler service is booked for Monday 30 November, between 8 and 12. The engineer is called Marek.",
    },
    Note {
        key: "club2",
        at: "16:10",
        text: "Sam says book club is actually on the second Wednesday of the month at the community hall.",
    },
    Note {
        key: "mot",
        at: "17:20",
        text: "The car MOT is due on 14 December. The garage is Hale Motors.",
    },
    Note {
        key: "cinema",
        at: "18:05",
        text: "I told Sam I would book the cinema tickets this weekend.",
    },
    Note {
        key: "wifi",
        at: "19:30",
        text: "The guest wifi network is called HillviewGuest.",
    },
];

pub const PHOTO_AT: &str = "21:00";
pub const PHOTO_LINES: [&str; 4] = [
    "CITY PARKING PERMIT",
    "PERMIT NUMBER 4471",
    "BAY C7 LEVEL 2",
    "VALID UNTIL 30 JUNE 2027",
];
pub const VOICE_AT: &str = "21:30";
pub const VOICE_TEXT: &str = "Reminder to myself. The plumber, Mister Hughes, is coming on Friday the twentieth of November at nine in the morning to fix the kitchen tap.";
pub const DOC_AT: &str = "21:45";
pub const DOC_LINES: [&str; 4] = [
    "Riverside Allotment Society, autumn newsletter",
    "Our plot is number 17, next to the water tap.",
    "The water is switched off for winter on 1 December.",
    "The annual fee of 30 pounds is due by 15 January.",
];

pub struct Question {
    pub id: &'static str,
    pub kind: &'static str,
    pub text: &'static str,
    /// The kept things the answer must come from.
    pub sources: &'static [&'static str],
    /// Every group needs one of its words in the answer.
    pub must: &'static [&'static [&'static str]],
    /// The right answer is to say it does not know.
    pub unknown: bool,
    /// When not empty, the first sentence of the answer needs one of these words.
    pub lead: &'static [&'static str],
}

pub const QUESTIONS: &[Question] = &[
    Question {
        id: "q01",
        kind: "fact",
        text: "Who is coming to service the boiler, and when?",
        sources: &["boiler"],
        must: &[&["marek"], &["30 november", "november 30", "30th"]],
        unknown: false,
        lead: &[],
    },
    Question {
        id: "q02",
        kind: "changed fact",
        text: "When is my dentist appointment?",
        sources: &["dentist_moved", "dentist"],
        must: &[&["19 november", "thursday"], &["14:30", "2:30", "14.30"]],
        unknown: false,
        lead: &[],
    },
    Question {
        id: "q03",
        kind: "promise",
        text: "What did I promise Priya?",
        sources: &["priya"],
        must: &[&["budget"], &["friday", "20 november"]],
        unknown: false,
        lead: &[],
    },
    Question {
        id: "q04",
        kind: "photo only",
        text: "What is my parking permit number?",
        sources: &["photo"],
        must: &[&["4471"]],
        unknown: false,
        lead: &[],
    },
    Question {
        id: "q05",
        kind: "voice only",
        text: "When is the plumber coming?",
        sources: &["voice", "voice_raw"],
        must: &[&["friday", "20", "twentieth"], &["9", "nine"]],
        unknown: false,
        lead: &[],
    },
    Question {
        id: "q06",
        kind: "private, asked directly",
        text: "What did my blood test show?",
        sources: &["blood"],
        must: &[&["cholesterol"], &["5.9"]],
        unknown: false,
        lead: &[],
    },
    Question {
        id: "q07",
        kind: "never kept",
        text: "What is Sam's mother's phone number?",
        sources: &[],
        must: &[],
        unknown: true,
        lead: &[],
    },
    Question {
        id: "q08",
        kind: "which is right",
        text: "When does book club meet?",
        sources: &["club1", "club2"],
        must: &[&["tuesday"], &["wednesday"]],
        unknown: false,
        lead: &[],
    },
    Question {
        id: "q09",
        kind: "changed fact",
        text: "Where does Gran live?",
        sources: &["gran_moved", "gran"],
        must: &[&["york"]],
        unknown: false,
        lead: &[],
    },
    Question {
        id: "q10",
        kind: "document only",
        text: "What is our allotment plot number?",
        sources: &["doc"],
        must: &[&["17"]],
        unknown: false,
        lead: &[],
    },
    Question {
        id: "q11",
        kind: "decision",
        text: "What colour are we painting the hallway?",
        sources: &["paint"],
        must: &[&["sage"]],
        unknown: false,
        lead: &[],
    },
    Question {
        id: "q12",
        kind: "second promise",
        text: "What did I tell Sam I would book this weekend?",
        sources: &["cinema"],
        must: &[&["cinema"]],
        unknown: false,
        lead: &[],
    },
];

// ---------------------------------------------------------------------------------------------
// What a perfect reading keeps: words each item's memories must hold between them. Every group
// needs one of its words.

pub const GOLD: &[(&str, &[&[&str]])] = &[
    (
        "dentist",
        &[
            &["okafor"],
            &["18 november", "november 18"],
            &["10:00", "10 am", "10am", "10.00"],
        ],
    ),
    ("gran", &[&["leeds"], &["3 december", "december 3"]]),
    (
        "trip",
        &[
            &["science museum"],
            &["23 november", "november 23"],
            &["packed lunch"],
        ],
    ),
    (
        "priya",
        &[&["budget"], &["20 november", "november 20", "friday"]],
    ),
    ("bakery", &[&["sourdough"], &["rye"], &["fenn street"]]),
    ("club1", &[&["first tuesday"], &["library"]]),
    (
        "dentist_moved",
        &[&["19 november", "november 19"], &["14:30", "2:30", "14.30"]],
    ),
    ("blood", &[&["cholesterol"], &["5.9"], &["march"]]),
    ("gran_moved", &[&["york"]]),
    ("swim", &[&["monday", "mondays"], &["coach dee"]]),
    ("paint", &[&["sage"]]),
    ("boiler", &[&["30 november", "november 30"], &["marek"]]),
    ("club2", &[&["second wednesday"], &["community hall"]]),
    ("mot", &[&["14 december", "december 14"], &["hale motors"]]),
    ("cinema", &[&["cinema"]]),
    ("wifi", &[&["hillviewguest"]]),
    (
        "photo",
        &[
            &["4471"],
            &["c7"],
            &["30 june 2027", "june 30, 2027", "june 2027"],
        ],
    ),
    (
        "voice",
        &[
            &["plumber"],
            &["hughes"],
            &["20 november", "november 20", "twentieth", "20th"],
            &["kitchen tap", "tap"],
        ],
    ),
    (
        "doc",
        &[
            &["17"],
            &["1 december", "december 1"],
            &["15 january", "january 15"],
        ],
    ),
];

/// Whether `word` is in `text` on its own, not inside a longer word or number: "son" is not in
/// "person".
pub fn has_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

pub fn plain(text: &str) -> String {
    text.replace("**", "")
}

/// Whether an answer to `q` is right: "not sure" (or nothing) for what was never kept, else a
/// finished answer with one word of every group, led by one of `lead` when it says any.
pub fn judge(q: &Question, phase: &str, text: &str) -> bool {
    if q.unknown {
        return matches!(phase, "notSure" | "nothing");
    }
    let lower = plain(text).to_lowercase();
    let first = lower.split_terminator(". ").next().unwrap_or("");
    phase == "done"
        && q.must
            .iter()
            .all(|group| group.iter().any(|w| has_word(&lower, w)))
        && (q.lead.is_empty() || q.lead.iter().any(|w| has_word(first, w)))
}

/// The gold words of item `key` found in `said` (what its memories say, any case).
pub fn coverage(key: &str, said: &[String]) -> Value {
    let Some((_, groups)) = GOLD.iter().find(|(k, _)| *k == key) else {
        return Value::Null;
    };
    let text = said.join(" \n ").to_lowercase();
    let missing: Vec<&str> = groups
        .iter()
        .filter(|g| !g.iter().any(|w| has_word(&text, w)))
        .map(|g| g[0])
        .collect();
    json!({"key": key, "found": groups.len() - missing.len(), "of": groups.len(), "missing": missing})
}

/// Every memory, current or replaced since, once each.
fn every_version(engine: &Engine) -> Vec<nocturne_contract::MemoryView> {
    let current = engine
        .memories(MemoryFilter {
            limit: Some(5000),
            ..MemoryFilter::default()
        })
        .unwrap_or_default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for m in &current {
        for v in std::iter::once(m.clone()).chain(engine.history(&m.id).unwrap_or_default()) {
            if seen.insert(v.id.clone()) {
                out.push(v);
            }
        }
    }
    out
}

/// Everything the memories from `item` say.
fn said_by_item(versions: &[nocturne_contract::MemoryView], item: &str) -> Vec<String> {
    versions
        .iter()
        .filter(|v| v.receipts.iter().any(|r| r.item_id == item))
        .map(|v| format!("{} | {} | {}", v.sentence, v.subject, v.object))
        .collect()
}

/// What reading kept: the gold words found per item, and in all.
pub fn extraction(engine: &Engine, ids: &HashMap<&'static str, String>) -> Value {
    let versions = every_version(engine);
    let mut rows = Vec::new();
    let (mut found, mut of) = (0, 0);
    for (key, _) in GOLD {
        let Some(item) = ids.get(key) else {
            continue;
        };
        let row = coverage(key, &said_by_item(&versions, item));
        found += row["found"].as_u64().unwrap_or(0);
        of += row["of"].as_u64().unwrap_or(0);
        rows.push(row);
    }
    json!({"found": found, "of": of, "value": share(found, of), "items": rows})
}

/// The links a perfect store forms on this day: two moves and one which-is-right question.
pub fn links(engine: &Engine, ids: &HashMap<&'static str, String>) -> Value {
    let mems = |key: &str| -> Vec<nocturne_contract::MemoryView> {
        ids.get(key)
            .map(|item| {
                engine
                    .memories(MemoryFilter {
                        item_id: Some(item.clone()),
                        limit: Some(200),
                        ..MemoryFilter::default()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    };
    let mut moves = Vec::new();
    for (from, to) in [("dentist", "dentist_moved"), ("gran", "gran_moved")] {
        let old = ids.get(from).cloned().unwrap_or_default();
        let how = mems(to).iter().find_map(|m| {
            if engine
                .history(&m.id)
                .unwrap_or_default()
                .iter()
                .any(|v| v.id != m.id && v.receipts.iter().any(|r| r.item_id == old))
            {
                Some("history")
            } else if m.was.is_some() {
                Some("was")
            } else {
                None
            }
        });
        moves.push(json!({"from": from, "to": to, "linked": how.is_some(), "how": how}));
    }
    let (a, b) = (mems("club1"), mems("club2"));
    let a_ids: Vec<String> = a.iter().map(|m| m.id.clone()).collect();
    let b_ids: Vec<String> = b.iter().map(|m| m.id.clone()).collect();
    let open = engine
        .open_disputes(&a_ids)
        .unwrap_or_default()
        .iter()
        .any(|(x, y)| b_ids.contains(x) || b_ids.contains(y));
    let merged = a_ids.iter().any(|id| b_ids.contains(id));
    let formed = moves.iter().filter(|m| m["linked"] == true).count() + usize::from(open);
    json!({
        "formed": formed, "of": 3, "moves": moves,
        "disputes": [{"a": "club1", "b": "club2", "open": open, "merged_into_one": merged}],
    })
}

/// The 1-based place of the first hit from one of `wanted` (item ids), if any.
pub fn source_rank(hits: &[Hit], wanted: &HashSet<String>) -> Option<usize> {
    hits.iter()
        .position(|h| {
            h.memory
                .receipts
                .iter()
                .any(|r| wanted.contains(&r.item_id))
        })
        .map(|at| at + 1)
}

pub fn share(a: u64, b: u64) -> Value {
    if b == 0 {
        Value::Null
    } else {
        json!((a as f64 / b as f64 * 1000.0).round() / 1000.0)
    }
}

// ---------------------------------------------------------------------------------------------
// Frozen, correct facts: what a perfect reading of the day keeps, put straight into the store.

fn fact(subject: &str, predicate: Predicate, object: &str, sentence: &str) -> Candidate {
    Candidate::new(subject, predicate, object, sentence, 0.95)
}

fn on(mut c: Candidate, date: &str) -> Candidate {
    c.valid_from = Some(date.parse::<Date>().expect("a date"));
    c
}

fn due(mut c: Candidate, date: &str) -> Candidate {
    c.due = Some(date.parse::<Date>().expect("a date"));
    c
}

fn moved(mut c: Candidate) -> Candidate {
    c.moved = true;
    c
}

/// What a perfect reading of item `key` keeps.
pub fn gold_facts(key: &str) -> Vec<Candidate> {
    use Predicate::*;
    match key {
        "dentist" => vec![on(
            fact(
                "Dentist appointment",
                Scheduled,
                "Wednesday 18 November at 10:00",
                "Dentist appointment with Dr Okafor on Wednesday 18 November at 10:00.",
            ),
            "2026-11-18",
        )],
        "gran" => vec![
            fact("Gran", LivesIn, "Leeds", "Gran lives in Leeds."),
            fact(
                "Gran's birthday",
                Scheduled,
                "3 December",
                "Gran's birthday is on 3 December.",
            ),
        ],
        "trip" => vec![
            on(
                fact(
                    "Leo's school trip",
                    Scheduled,
                    "Monday 23 November",
                    "Leo's school trip to the science museum is on Monday 23 November.",
                ),
                "2026-11-23",
            ),
            fact(
                "Leo's school trip",
                Knows,
                "packed lunch",
                "Leo needs a packed lunch for the school trip.",
            ),
        ],
        "priya" => vec![due(
            fact(
                "Priya",
                Promised,
                "send the budget spreadsheet by Friday 20 November",
                "I promised Priya I would send her the budget spreadsheet by Friday 20 November.",
            ),
            "2026-11-20",
        )],
        "bakery" => vec![fact(
            "Bakery order",
            Scheduled,
            "Saturday",
            "Bakery order for Saturday: two sourdough loaves and one seeded rye, collect before noon from Fenn Street Bakery.",
        )],
        "club1" => vec![fact(
            "Book club",
            Scheduled,
            "first Tuesday of the month at the library",
            "Book club meets on the first Tuesday of the month at the library.",
        )],
        "dentist_moved" => vec![moved(on(
            fact(
                "Dentist appointment",
                Scheduled,
                "Thursday 19 November at 14:30",
                "The dentist appointment with Dr Okafor is now Thursday 19 November at 14:30.",
            ),
            "2026-11-19",
        ))],
        "blood" => vec![
            fact(
                "Blood test",
                Knows,
                "high cholesterol, 5.9",
                "My blood test showed high cholesterol, 5.9.",
            ),
            fact(
                "Cholesterol recheck",
                Scheduled,
                "March",
                "The doctor wants a recheck in March.",
            ),
        ],
        "gran_moved" => vec![moved(on(
            fact(
                "Gran",
                LivesIn,
                "York",
                "Gran now lives in York, since 1 November.",
            ),
            "2026-11-01",
        ))],
        "swim" => vec![fact(
            "Leo",
            Scheduled,
            "swimming on Mondays with Coach Dee",
            "Leo swims on Mondays with Coach Dee after school.",
        )],
        "paint" => vec![fact(
            "Hallway",
            Decided,
            "sage green",
            "We will repaint the hallway in sage green, not grey.",
        )],
        "boiler" => vec![
            on(
                fact(
                    "Boiler service",
                    Scheduled,
                    "Monday 30 November between 8 and 12",
                    "Boiler service is booked for Monday 30 November, between 8 and 12.",
                ),
                "2026-11-30",
            ),
            fact(
                "Marek",
                Knows,
                "boiler engineer",
                "The boiler engineer is called Marek.",
            ),
        ],
        "club2" => vec![fact(
            "Book club",
            Scheduled,
            "second Wednesday of the month at the community hall",
            "Sam says book club is on the second Wednesday of the month at the community hall.",
        )],
        "mot" => vec![due(
            fact(
                "Car MOT",
                Due,
                "14 December",
                "The car MOT is due on 14 December at Hale Motors.",
            ),
            "2026-12-14",
        )],
        "cinema" => vec![fact(
            "Sam",
            Promised,
            "book the cinema tickets this weekend",
            "I told Sam I would book the cinema tickets this weekend.",
        )],
        "wifi" => vec![fact(
            "Guest wifi",
            Knows,
            "HillviewGuest",
            "The guest wifi network is called HillviewGuest.",
        )],
        "photo" => vec![
            fact(
                "Parking permit",
                Knows,
                "number 4471",
                "My parking permit number is 4471, bay C7 level 2.",
            ),
            fact(
                "Parking permit",
                Due,
                "30 June 2027",
                "My parking permit is valid until 30 June 2027.",
            ),
        ],
        "voice" => vec![on(
            fact(
                "Mr Hughes",
                Scheduled,
                "Friday 20 November at 9:00",
                "The plumber, Mr Hughes, is coming on Friday 20 November at 9 in the morning to fix the kitchen tap.",
            ),
            "2026-11-20",
        )],
        "doc" => vec![
            fact(
                "Allotment plot",
                Knows,
                "number 17",
                "Our allotment plot is number 17, next to the water tap.",
            ),
            fact(
                "Allotment water",
                Scheduled,
                "1 December",
                "The allotment water is switched off for winter on 1 December.",
            ),
            due(
                fact(
                    "Allotment fee",
                    Due,
                    "15 January",
                    "The annual allotment fee of 30 pounds is due by 15 January.",
                ),
                "2027-01-15",
            ),
        ],
        _ => Vec::new(),
    }
}

/// The day's items with their text, in the order they were kept: the notes, then what the
/// photo, the voice note and the document say, each kept here as a note.
pub fn day_items() -> Vec<(&'static str, &'static str, String)> {
    let mut out: Vec<(&'static str, &'static str, String)> = NOTES
        .iter()
        .map(|n| (n.key, n.at, n.text.to_string()))
        .collect();
    out.push(("photo", PHOTO_AT, PHOTO_LINES.join("\n")));
    out.push(("voice", VOICE_AT, VOICE_TEXT.to_string()));
    out.push(("doc", DOC_AT, DOC_LINES.join("\n")));
    out
}

/// Keeps the day and applies a perfect reading of it, each item at its own time; the blood test
/// is private. Returns the item id for each key. Nothing is embedded: see [`embed_all`].
pub fn seed_fixture(engine: &Engine, clock: &ManualClock) -> HashMap<&'static str, String> {
    let mut ids = HashMap::new();
    for (key, at, text) in day_items() {
        clock.set_str(&format!("{DAY}T{at}:00Z"));
        let item = match engine
            .keep_text(&text, Source::QuickCapture)
            .expect("the fixture item is kept")
        {
            Kept::New { item } | Kept::Duplicate { item } => item.id,
        };
        if key == "blood" {
            engine.set_item_private(&item, true).expect("private");
        }
        let lease = engine
            .lease_item(&item, Duration::from_secs(600))
            .expect("the fixture item is free to read");
        engine
            .apply_candidates(&item, &lease.lease, gold_facts(key))
            .expect("the perfect reading applies");
        ids.insert(key, item);
    }
    ids
}

/// Embeds every memory that has no vector yet with `mind`, as the app's backfill would.
pub async fn embed_all(engine: &Engine, mind: &dyn Mind) -> usize {
    let mut done = 0;
    loop {
        let todo = engine.memories_without_embedding(32).unwrap_or_default();
        if todo.is_empty() {
            return done;
        }
        let texts: Vec<String> = todo.iter().map(|(_, s)| s.clone()).collect();
        let Ok(vectors) = mind.embed(&texts).await else {
            return done;
        };
        let rows: Vec<(String, Vec<f32>)> =
            todo.into_iter().map(|(id, _)| id).zip(vectors).collect();
        done += rows.len();
        if engine.store_embeddings(rows).is_err() {
            return done;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Raw replies: the mind reports each reply it got, before any check, as a trace event with this
// target (fields `step` and `reply`). Only a run that asks for them records them.

pub const REPLY_TARGET: &str = "nocturne::reply";

#[derive(Clone, Default)]
pub struct ReplyTap(Arc<Mutex<Vec<Value>>>);

impl ReplyTap {
    /// Every reply recorded since the last call.
    pub fn drain(&self) -> Vec<Value> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

#[derive(Default)]
struct Fields(Map<String, Value>);

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), json!(value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_string(), json!(format!("{value:?}")));
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for ReplyTap {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        if event.metadata().target() != REPLY_TARGET {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Value::Object(fields.0));
    }
}

// ---------------------------------------------------------------------------------------------
// The answer step alone.

/// What one question came back with, as the harness judges it.
#[derive(Debug, Clone, Default)]
pub struct Asked {
    pub phase: String,
    /// The streamed text as the interface got it, bold marks included.
    pub text: String,
    pub memory_ids: Vec<String>,
    pub cited_items: Vec<String>,
    pub answered_by: Value,
    pub left_out: Value,
}

/// Where policy, not quality, decided an answer: something was kept back from an online
/// question and it came back "not sure", or this computer answered because of it.
pub fn policy(a: &Asked) -> Option<&'static str> {
    let online = a.answered_by["type"] == "online";
    if online && !a.left_out.is_null() && a.phase == "notSure" {
        Some("keptBackNotSure")
    } else if a.answered_by["type"] == "computer" && a.answered_by["reason"] == "keptHere" {
        Some("answeredHereAfterKeptBack")
    } else {
        None
    }
}

/// What the question about something never kept is given instead: other things about Sam.
const NEAR_UNKNOWN: [&str; 2] = ["cinema", "club2"];

/// The facts a perfect search would give the mind for `q`: the current memories of its source
/// items (for a question about something never kept, of items about the same person), each with
/// the open which-is-right questions among them. No search, no reranking, no lines.
pub fn frozen_facts(
    engine: &Engine,
    ids: &HashMap<&'static str, String>,
    q: &Question,
) -> Vec<Fact> {
    let keys: Vec<&str> = if q.unknown {
        NEAR_UNKNOWN.to_vec()
    } else {
        q.sources.to_vec()
    };
    let mut memories: Vec<nocturne_contract::MemoryView> = Vec::new();
    for key in keys {
        let Some(item) = ids.get(key) else {
            continue;
        };
        for m in engine
            .memories(MemoryFilter {
                item_id: Some(item.clone()),
                limit: Some(50),
                ..MemoryFilter::default()
            })
            .unwrap_or_default()
        {
            if !memories.iter().any(|x| x.id == m.id) {
                memories.push(m);
            }
        }
    }
    let all: Vec<String> = memories.iter().map(|m| m.id.clone()).collect();
    let disputes = engine.open_disputes(&all).unwrap_or_default();
    memories
        .into_iter()
        .map(|memory| {
            let id = memory.id.clone();
            Fact {
                memory,
                disputed_by: disputes
                    .iter()
                    .filter_map(|(a, b)| match (a == &id, b == &id) {
                        (true, _) => Some(b.clone()),
                        (_, true) => Some(a.clone()),
                        _ => None,
                    })
                    .filter(|other| all.contains(other))
                    .collect(),
            }
        })
        .collect()
}

/// The answer step alone: `facts` straight to the mind, judged as the app would show it.
pub async fn answer_alone(mind: &dyn Mind, question: &str, facts: &[Fact]) -> Asked {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(64);
    let reading = async {
        let mut text = String::new();
        while let Some(t) = rx.recv().await {
            text.push_str(&t);
        }
        text
    };
    let (meta, text) = tokio::join!(mind.answer(question, facts, tx), reading);
    let Ok(meta) = meta else {
        return Asked {
            phase: "unavailable".into(),
            ..Asked::default()
        };
    };
    let cited: Vec<&Fact> = meta
        .cited
        .iter()
        .filter_map(|id| facts.iter().find(|f| &f.memory.id == id))
        .collect();
    let phase = if meta.not_sure || cited.is_empty() {
        "notSure"
    } else {
        "done"
    };
    Asked {
        phase: phase.into(),
        text: if phase == "done" { text } else { String::new() },
        memory_ids: cited.iter().map(|f| f.memory.id.clone()).collect(),
        cited_items: cited
            .iter()
            .flat_map(|f| f.memory.receipts.iter().map(|r| r.item_id.clone()))
            .collect(),
        answered_by: Value::Null,
        left_out: Value::Null,
    }
}

/// The twelve questions asked `runs` times over the same store, through `ask`. Each row keeps
/// the raw replies the mind reported for it.
pub fn answer_only(
    runs: usize,
    ids: &HashMap<&'static str, String>,
    tap: Option<&ReplyTap>,
    mut ask: impl FnMut(&Question) -> Asked,
) -> Value {
    let mut rows = Vec::new();
    let mut per_run: Vec<u64> = Vec::new();
    let mut per_question: HashMap<&str, u64> = HashMap::new();
    for run in 0..runs {
        let mut right = 0;
        for q in QUESTIONS {
            if let Some(t) = tap {
                t.drain();
            }
            let a = ask(q);
            let raw = tap.map(ReplyTap::drain).unwrap_or_default();
            let ok = judge(q, &a.phase, &a.text);
            right += u64::from(ok);
            *per_question.entry(q.id).or_default() += u64::from(ok);
            let wanted: HashSet<String> = q
                .sources
                .iter()
                .filter_map(|k| ids.get(k).cloned())
                .collect();
            rows.push(json!({
                "run": run + 1, "id": q.id, "kind": q.kind, "question": q.text,
                "phase": a.phase, "text": plain(&a.text), "raw_text": a.text, "correct": ok,
                "memory_ids": a.memory_ids, "cited_items": a.cited_items.len(),
                "cited_items_supporting": a.cited_items.iter().filter(|i| wanted.contains(*i)).count(),
                "answered_by": a.answered_by, "left_out": a.left_out, "policy": policy(&a),
                "raw_replies": raw,
            }));
        }
        per_run.push(right);
    }
    let mut sorted = per_run.clone();
    sorted.sort_unstable();
    let median = sorted.get(sorted.len() / 2).copied();
    json!({
        "runs": runs, "of": QUESTIONS.len(), "correct_per_run": per_run, "median": median,
        "correct_per_question": QUESTIONS.iter().map(|q| json!({"id": q.id, "correct": per_question.get(q.id).copied().unwrap_or(0), "of": runs})).collect::<Vec<_>>(),
        "rows": rows,
    })
}
