//! The look-ups a mind may ask for while it answers (docs/design/ask-with-tools.md#the-look-ups):
//! their descriptions, as OpenAI-compatible `tools`, the reading of what a mind asked for, and
//! the lines that go back, numbered on from the facts inside the same fence. The look-ups only
//! read; the device that answers runs them. The words here are the mind's, never the person's.

use jiff::civil::Date;
use nocturne_contract::{ReadingKind, Tier};
use nocturne_engine::{CALENDAR_DAYS, Fact, HEALTH_DAYS, JOURNAL_DAYS};
use serde_json::{Value, json};

use crate::prompts::{fact_lines, fence};

/// One look-up a mind asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookUp {
    Calendar {
        from: Date,
        to: Date,
    },
    Journal {
        from: Date,
        to: Date,
    },
    Search {
        words: String,
    },
    Health {
        kind: ReadingKind,
        from: Date,
        to: Date,
    },
}

/// Where a look-up looks, for the line shown while it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Place {
    Calendar,
    Journal,
    Search,
    Health,
}

impl LookUp {
    pub fn place(&self) -> Place {
        match self {
            LookUp::Calendar { .. } => Place::Calendar,
            LookUp::Journal { .. } => Place::Journal,
            LookUp::Search { .. } => Place::Search,
            LookUp::Health { .. } => Place::Health,
        }
    }

    /// What it says when it finds nothing.
    pub fn nothing(&self) -> &'static str {
        match self {
            LookUp::Search { .. } => NOTHING_WORDS,
            _ => NOTHING_DAYS,
        }
    }
}

/// A look-up that cannot run as asked: the one plain line the mind gets back, and whether it
/// was broken (a look-up that does not exist, or inputs that do not read) rather than only
/// outside its limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wrong {
    pub line: String,
    pub broken: bool,
}

pub const ANSWER_NOW: &str = "Answer now from the lines you have.";
pub const TOO_MANY: &str = "Not looked up: too many at once.";
pub const NO_MORE: &str = "No more lines for this question. Answer from what you have.";
pub const COULD_NOT: &str = "That couldn't be looked up just now.";
pub const NOTHING_DAYS: &str = "Nothing kept for these days.";
pub const NOTHING_WORDS: &str = "Nothing kept with these words.";
/// What a look-up says when every line it found was handed before in this question.
pub const ALREADY: &str = "Those lines are already above.";
pub const NO_SUCH: &str = "There is no such look-up. Use calendar, journal, search or health.";
const CALENDAR_WRONG: &str = "Give days as YYYY-MM-DD, at most 62 days apart.";
const JOURNAL_WRONG: &str = "Give days as YYYY-MM-DD, at most 14 days apart, none after today.";
const HEALTH_WRONG: &str =
    "Give a kind from the list, and days as YYYY-MM-DD, at most 31 days apart, none after today.";
const SEARCH_WRONG: &str = "Give 1 to 8 words.";

/// The sentence the answer instructions gain when look-ups are offered.
pub const SYSTEM: &str = "When the facts do not answer the question, you may look further with the look-ups you are given. \
They only read what the person kept, and each gives back more numbered facts. \
Ask only for what the question needs, and answer as soon as the facts are enough.";

/// "And 6 more. Ask for fewer days to see them."
pub fn more(n: usize) -> String {
    format!("And {n} more. Ask for fewer days to see them.")
}

/// A kind of reading as the look-up names it.
pub fn kind_word(kind: ReadingKind) -> &'static str {
    match kind {
        ReadingKind::Sleep => "sleep",
        ReadingKind::Steps => "steps",
        ReadingKind::RestingHeartRate => "resting heart rate",
        ReadingKind::HeartRate => "heart rate",
        ReadingKind::BloodPressure => "blood pressure",
        ReadingKind::Workout => "workouts",
        ReadingKind::Weight => "weight",
    }
}

const KINDS: [ReadingKind; 7] = [
    ReadingKind::Sleep,
    ReadingKind::Steps,
    ReadingKind::RestingHeartRate,
    ReadingKind::HeartRate,
    ReadingKind::BloodPressure,
    ReadingKind::Workout,
    ReadingKind::Weight,
];

fn days(what: &str) -> Value {
    json!({"type": "string", "description": format!("{what}, as YYYY-MM-DD")})
}

/// The look-ups offered, as OpenAI-compatible `tools`; `health` only when some `kinds` have
/// readings, and only for those.
pub fn tools(kinds: &[ReadingKind]) -> Value {
    let mut out = vec![
        json!({"type": "function", "function": {
            "name": "calendar",
            "description": "Appointments from the person's calendar, and anything they kept with a day, \
        from one day to another, both included, at most 62 days apart.",
            "parameters": {"type": "object", "properties": {
                "from": days("The first day"), "to": days("The last day")},
                "required": ["from", "to"], "additionalProperties": false},
        }}),
        json!({"type": "function", "function": {
            "name": "journal",
            "description": "What the person kept and wrote on those days: journal entries, good things, \
        check-ins and what was read from what they kept. At most 14 days apart, none after today.",
            "parameters": {"type": "object", "properties": {
                "from": days("The first day"), "to": days("The last day")},
                "required": ["from", "to"], "additionalProperties": false},
        }}),
        json!({"type": "function", "function": {
            "name": "search",
            "description": "Find what the person kept with other words than the question's: 1 to 8 words.",
            "parameters": {"type": "object", "properties": {
                "words": {"type": "string", "description": "The words to find"}},
                "required": ["words"], "additionalProperties": false},
        }}),
    ];
    let offered: Vec<&str> = KINDS
        .iter()
        .filter(|k| kinds.contains(k))
        .map(|k| kind_word(*k))
        .collect();
    if !offered.is_empty() {
        out.push(json!({"type": "function", "function": {
            "name": "health",
            "description": "The person's health readings of one kind, one line per night, day or reading, \
at most 31 days apart, none after today.",
            "parameters": {"type": "object", "properties": {
                "kind": {"type": "string", "enum": offered},
                "from": days("The first day"), "to": days("The last day")},
                "required": ["kind", "from", "to"], "additionalProperties": false},
        }}));
    }
    Value::Array(out)
}

fn wrong(line: &str) -> Wrong {
    Wrong {
        line: line.to_string(),
        broken: false,
    }
}

fn broken(line: &str) -> Wrong {
    Wrong {
        line: line.to_string(),
        broken: true,
    }
}

/// Reads what a mind asked for: the look-up `name` with `arguments`, a JSON object as text, on
/// `today`.
pub fn parse(name: &str, arguments: &str, today: Date) -> Result<LookUp, Wrong> {
    let args: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);
    let day = |key: &str| -> Option<Date> { args[key].as_str()?.trim().parse().ok() };
    let range = |limit: i64, no_future: bool, line: &str| -> Result<(Date, Date), Wrong> {
        let (Some(from), Some(to)) = (day("from"), day("to")) else {
            return Err(broken(line));
        };
        let apart = from
            .until(to)
            .map(|s| i64::from(s.get_days()))
            .unwrap_or(-1);
        if apart < 0 || apart > limit || (no_future && to > today) {
            return Err(wrong(line));
        }
        Ok((from, to))
    };
    match name {
        "calendar" => {
            let (from, to) = range(CALENDAR_DAYS, false, CALENDAR_WRONG)?;
            Ok(LookUp::Calendar { from, to })
        }
        "journal" => {
            let (from, to) = range(JOURNAL_DAYS, true, JOURNAL_WRONG)?;
            Ok(LookUp::Journal { from, to })
        }
        "health" => {
            let kind = args["kind"]
                .as_str()
                .and_then(|w| KINDS.iter().find(|k| kind_word(**k) == w.trim()))
                .copied()
                .ok_or_else(|| broken(HEALTH_WRONG))?;
            let (from, to) = range(HEALTH_DAYS, true, HEALTH_WRONG)?;
            Ok(LookUp::Health { kind, from, to })
        }
        "search" => {
            let words = args["words"].as_str().ok_or_else(|| broken(SEARCH_WRONG))?;
            let count = words.split_whitespace().count();
            if !(1..=8).contains(&count) {
                return Err(wrong(SEARCH_WRONG));
            }
            Ok(LookUp::Search {
                words: words.split_whitespace().collect::<Vec<_>>().join(" "),
            })
        }
        _ => Err(broken(NO_SUCH)),
    }
}

/// The lines a look-up sends back: `facts` numbered on from `start` (the number of facts handed
/// before them), in the facts' own form inside the fence, then "And N more" when there were
/// more; `empty` when there are none.
pub fn lines(
    tier: Tier,
    facts: &[Fact],
    start: usize,
    more_of: usize,
    nonce: &str,
    empty: &str,
) -> String {
    if facts.is_empty() {
        return match more_of {
            0 => empty.to_string(),
            n => format!("{empty}\n{}", more(n)),
        };
    }
    let body: Vec<String> = facts
        .iter()
        .zip(fact_lines(tier, facts))
        .enumerate()
        .map(|(i, (f, (_, after)))| format!("[m{}] {}{after}", start + i + 1, f.memory.sentence))
        .collect();
    let mut out = fence("facts", nonce, &body.join("\n"));
    if more_of > 0 {
        out.push('\n');
        out.push_str(&more(more_of));
    }
    out
}

/// The days a look-up covers, clipped to `today` where it may not look ahead.
pub fn span(look: &LookUp) -> Option<(Date, Date)> {
    match look {
        LookUp::Calendar { from, to }
        | LookUp::Journal { from, to }
        | LookUp::Health { from, to, .. } => Some((*from, *to)),
        LookUp::Search { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    const TUE: &str = "2026-11-24";

    #[test]
    fn each_look_up_reads_its_inputs() {
        let today = d(TUE);
        assert_eq!(
            parse(
                "calendar",
                r#"{"from":"2026-11-25","to":"2026-11-25"}"#,
                today
            ),
            Ok(LookUp::Calendar {
                from: d("2026-11-25"),
                to: d("2026-11-25")
            })
        );
        assert_eq!(
            parse(
                "journal",
                r#"{"from":"2026-11-17","to":"2026-11-17"}"#,
                today
            ),
            Ok(LookUp::Journal {
                from: d("2026-11-17"),
                to: d("2026-11-17")
            })
        );
        assert_eq!(
            parse("search", r#"{"words":"MOT  garage car service"}"#, today),
            Ok(LookUp::Search {
                words: "MOT garage car service".into()
            })
        );
        assert_eq!(
            parse(
                "health",
                r#"{"kind":"sleep","from":"2026-11-23","to":"2026-11-24"}"#,
                today
            ),
            Ok(LookUp::Health {
                kind: ReadingKind::Sleep,
                from: d("2026-11-23"),
                to: d("2026-11-24")
            })
        );
    }

    #[test]
    fn a_wrong_input_gets_one_plain_line_saying_what_is_allowed() {
        let today = d(TUE);
        let w = |name: &str, args: &str| parse(name, args, today).unwrap_err();
        assert_eq!(
            w("calendar", r#"{"from":"2026-11-01","to":"2027-01-10"}"#),
            wrong(CALENDAR_WRONG),
            "more than 62 days apart"
        );
        assert_eq!(
            w("calendar", r#"{"from":"2026-11-25","to":"2026-11-24"}"#),
            wrong(CALENDAR_WRONG)
        );
        assert_eq!(
            w("calendar", r#"{"from":"tomorrow","to":"2026-11-24"}"#),
            broken(CALENDAR_WRONG),
            "a date that is not one"
        );
        assert_eq!(
            w("journal", r#"{"from":"2026-11-24","to":"2026-11-25"}"#),
            wrong(JOURNAL_WRONG),
            "none after today"
        );
        assert_eq!(
            w("journal", r#"{"from":"2026-11-01","to":"2026-11-20"}"#),
            wrong(JOURNAL_WRONG)
        );
        assert_eq!(
            w(
                "health",
                r#"{"kind":"mood","from":"2026-11-23","to":"2026-11-24"}"#
            ),
            broken(HEALTH_WRONG)
        );
        assert_eq!(w("search", r#"{"words":""}"#), wrong(SEARCH_WRONG));
        assert_eq!(w("search", "not json"), broken(SEARCH_WRONG));
        assert_eq!(w("delete", "{}"), broken(NO_SUCH));
        assert_eq!(
            CALENDAR_WRONG,
            "Give days as YYYY-MM-DD, at most 62 days apart."
        );
    }

    /// "At most N days apart" takes N apart, both ends included, and not one more.
    #[test]
    fn each_limit_takes_its_own_number_of_days_apart() {
        let today = d(TUE);
        let ok = |name: &str, args: &str| parse(name, args, today).is_ok();
        assert!(ok("calendar", r#"{"from":"2026-11-01","to":"2027-01-02"}"#));
        assert!(!ok(
            "calendar",
            r#"{"from":"2026-11-01","to":"2027-01-03"}"#
        ));
        assert!(ok("journal", r#"{"from":"2026-11-10","to":"2026-11-24"}"#));
        assert!(!ok("journal", r#"{"from":"2026-11-09","to":"2026-11-24"}"#));
        let health =
            |from: &str| format!(r#"{{"kind":"sleep","from":"{from}","to":"2026-11-24"}}"#);
        assert!(ok("health", &health("2026-10-24")));
        assert!(!ok("health", &health("2026-10-23")));
    }

    #[test]
    fn health_is_offered_only_for_kinds_with_readings() {
        let names = |v: &Value| -> Vec<String> {
            v.as_array()
                .unwrap()
                .iter()
                .map(|t| t["function"]["name"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(names(&tools(&[])), ["calendar", "journal", "search"]);
        let with = tools(&[ReadingKind::Weight, ReadingKind::Sleep]);
        assert_eq!(names(&with), ["calendar", "journal", "search", "health"]);
        assert_eq!(
            with[3]["function"]["parameters"]["properties"]["kind"]["enum"],
            json!(["sleep", "weight"])
        );
    }

    #[test]
    fn lines_are_numbered_on_from_the_facts_inside_the_fence() {
        let fact = |id: &str, s: &str| {
            Fact::from(
                serde_json::from_value::<nocturne_contract::MemoryView>(json!({
                    "id": id, "sentence": s, "subject": "x", "entityId": null,
                    "predicate": "scheduled", "object": "x", "certainty": "sure",
                    "status": "active", "validFrom": null, "validTo": null,
                    "due": "2026-11-25", "learnedAt": "2026-11-21T09:00:00+00:00",
                    "lastUsed": null, "letGoAt": null, "keepForever": false, "private": false,
                    "was": null, "receipts": [],
                }))
                .unwrap(),
            )
        };
        let facts = vec![
            fact(
                "a",
                "Dentist is on Wednesday 25 November at 10:30, at Smile Dental.",
            ),
            fact(
                "b",
                "Leo's swimming is on Wednesday 25 November at 17:00, at the leisure centre.",
            ),
        ];
        assert_eq!(
            lines(Tier::Standard, &facts, 3, 6, "n", NOTHING_DAYS),
            "<facts-n>\n\
             [m4] Dentist is on Wednesday 25 November at 10:30, at Smile Dental. (learned Saturday 21 November)\n\
             [m5] Leo's swimming is on Wednesday 25 November at 17:00, at the leisure centre. (learned Saturday 21 November)\n\
             </facts-n>\n\
             And 6 more. Ask for fewer days to see them."
        );
        assert_eq!(
            lines(Tier::Standard, &[], 3, 0, "n", NOTHING_DAYS),
            "Nothing kept for these days."
        );
    }
}
