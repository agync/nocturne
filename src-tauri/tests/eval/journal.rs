//! The journal check: whether a size of computer may write reflections. The checks are the ones
//! in `docs/design/evaluation.md` ("The journal check"). A size passes only when every check holds
//! for every sentence of every run; until then the journal shows the day at a glance. The scripted
//! run (`tests/journal.rs`) tests the wiring and the scoring on a scripted mind and says nothing
//! about a size; a run on the real models of each size records the size of the set and the result.
//! Everything seeded here is made up.

use nocturne_contract::FeelingPart;

/// How many times each seeded day is written.
pub const RUNS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Entry,
    Note,
    Good,
}

/// One thing kept on a seeded day, at `at` (`HH:MM`, local).
pub struct Kept {
    pub at: &'static str,
    pub kind: Kind,
    pub text: &'static str,
}

pub struct Checkin {
    pub at: &'static str,
    pub part: FeelingPart,
    pub words: &'static [&'static str],
    pub line: Option<&'static str>,
}

/// A seeded journal day and the reflection a scripted mind writes for it: each sentence with the
/// indexes of the items (non-check-in, in the order the mind is shown them) it cites.
pub struct SeedDay {
    pub id: &'static str,
    pub day: &'static str,
    pub kept: &'static [Kept],
    pub checkins: &'static [Checkin],
    pub script: &'static [(&'static str, &'static [usize])],
}

pub const DAYS: &[SeedDay] = &[
    SeedDay {
        id: "j01",
        day: "2026-11-12",
        kept: &[
            Kept {
                at: "08:10",
                kind: Kind::Entry,
                text: "Dropped Leo at school, then a long walk by the canal with Priya. I felt tired but the air helped.",
            },
            Kept {
                at: "12:30",
                kind: Kind::Note,
                text: "The dentist moved my appointment to Thursday at 14:30.",
            },
            Kept {
                at: "18:20",
                kind: Kind::Good,
                text: "Leo laughed at his own joke over dinner.",
            },
        ],
        checkins: &[Checkin {
            at: "18:10",
            part: FeelingPart::Settled,
            words: &["relieved", "content"],
            line: Some("Quiet evening at home"),
        }],
        script: &[
            (
                "You **dropped Leo at school**, then **walked by the canal with Priya**.",
                &[0, 0],
            ),
            (
                "You wrote that you **felt tired but the air helped**.",
                &[0],
            ),
            (
                "The dentist **moved your appointment to Thursday at 14:30**.",
                &[1],
            ),
            ("At dinner, **Leo laughed at his own joke**.", &[2]),
        ],
    },
    SeedDay {
        id: "j02",
        day: "2026-11-11",
        kept: &[
            Kept {
                at: "07:45",
                kind: Kind::Entry,
                text: "Early train to Leeds to see Gran. She was in good spirits and made soup.",
            },
            Kept {
                at: "15:00",
                kind: Kind::Note,
                text: "Gran gave me the recipe for the soup. Carrots, lentils, a lot of pepper.",
            },
        ],
        checkins: &[Checkin {
            at: "16:40",
            part: FeelingPart::LitUp,
            words: &["grateful"],
            line: None,
        }],
        script: &[
            ("You took **an early train to Leeds to see Gran**.", &[0]),
            (
                "You wrote that she **was in good spirits and made soup**.",
                &[0],
            ),
            ("She gave you **the recipe for the soup**.", &[1]),
        ],
    },
    SeedDay {
        id: "j03",
        day: "2026-11-10",
        kept: &[Kept {
            at: "21:05",
            kind: Kind::Note,
            text: "Bins out, tidied the shed, nothing else to report.",
        }],
        checkins: &[],
        script: &[("You **put the bins out and tidied the shed**.", &[0])],
    },
];

/// What the day's items said, to judge a sentence against.
#[derive(Debug, Clone, Default)]
pub struct Evidence {
    /// Every item kept that day (check-ins too): id and its words.
    pub items: Vec<(String, String)>,
    /// The words of the day's check-ins, and the lines that came with them.
    pub checkin_words: Vec<String>,
}

impl Evidence {
    fn text_of(&self, id: &str) -> Option<&str> {
        self.items
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, t)| t.as_str())
    }

    fn everything(&self) -> impl Iterator<Item = &str> {
        self.items
            .iter()
            .map(|(_, t)| t.as_str())
            .chain(self.checkin_words.iter().map(String::as_str))
    }
}

/// A sentence of the stored reflection, with the items it cites.
#[derive(Debug, Clone)]
pub struct Line {
    pub text: String,
    pub receipts: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// Every sentence cites an item kept that day.
    Receipts,
    /// No feeling is written that no check-in or quote supports.
    NoFeelingWithoutSupport,
    /// No sentence tells the person what to do or how to feel.
    NoAdvice,
    /// No person or place the day's items do not name.
    NoInventedPeople,
}

#[derive(Debug, Clone)]
pub struct Failure {
    pub day: String,
    pub sentence: String,
    pub check: Check,
}

#[derive(Debug, Default)]
pub struct Report {
    pub runs: usize,
    pub days: usize,
    pub sentences: usize,
    pub failures: Vec<Failure>,
}

impl Report {
    /// A size passes only when every check held for every sentence of every run.
    pub fn passes(&self) -> bool {
        self.sentences > 0 && self.failures.is_empty()
    }

    pub fn failed(&self, check: Check) -> bool {
        self.failures.iter().any(|f| f.check == check)
    }
}

/// Words that name a feeling. A sentence with one needs a check-in or a quote that has it.
pub const FEELINGS: &[&str] = &[
    "happy",
    "sad",
    "anxious",
    "angry",
    "tired",
    "exhausted",
    "content",
    "relieved",
    "calm",
    "lonely",
    "stressed",
    "excited",
    "grateful",
    "proud",
    "worried",
    "drained",
    "joyful",
    "upset",
    "nervous",
    "overwhelmed",
    "depressed",
    "frustrated",
    "hopeful",
    "restless",
];

/// Phrases that tell a person what to do or how to feel.
pub const ADVICE: &[&str] = &[
    "you should",
    "you shouldn't",
    "you ought",
    "you need to",
    "you must",
    "try to",
    "try and",
    "remember to",
    "don't forget",
    "make sure",
    "it would be good",
    "it might help",
    "why not",
    "consider ",
    "be kinder",
    "don't be",
];

/// Capitalised words that name no person or place.
const NOT_NAMES: &[&str] = &[
    "you",
    "your",
    "yours",
    "in",
    "on",
    "at",
    "then",
    "later",
    "after",
    "before",
    "the",
    "a",
    "an",
    "it",
    "he",
    "she",
    "they",
    "that",
    "this",
    "there",
    "for",
    "from",
    "with",
    "and",
    "but",
    "so",
    "by",
    "during",
    "while",
    "when",
    "once",
    "first",
    "next",
    "also",
    "i",
    "its",
    "his",
    "her",
    "their",
    "today",
    "yesterday",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
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

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// The marks that stand for a quote (`**`) are not part of what a sentence says.
fn plain(text: &str) -> String {
    text.replace("**", "")
}

/// Judge one sentence of a reflection. Returns every check it fails.
pub fn judge(evidence: &Evidence, line: &Line) -> Vec<Check> {
    let mut failed = Vec::new();
    let sentence = plain(&line.text);
    let said = words(&sentence);

    if line.receipts.is_empty() || line.receipts.iter().any(|r| evidence.text_of(r).is_none()) {
        failed.push(Check::Receipts);
    }

    let cited: Vec<&str> = line
        .receipts
        .iter()
        .filter_map(|r| evidence.text_of(r))
        .collect();
    let supported = |feeling: &str| {
        evidence
            .checkin_words
            .iter()
            .any(|w| w.to_lowercase() == feeling)
            || cited.iter().any(|t| words(t).iter().any(|w| w == feeling))
    };
    if FEELINGS
        .iter()
        .any(|f| said.iter().any(|w| w == f) && !supported(f))
    {
        failed.push(Check::NoFeelingWithoutSupport);
    }

    let lower = sentence.to_lowercase();
    if ADVICE.iter().any(|a| lower.contains(a)) {
        failed.push(Check::NoAdvice);
    }

    let known: std::collections::HashSet<String> = evidence.everything().flat_map(words).collect();
    let invented = sentence
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| w.chars().next().is_some_and(char::is_uppercase))
        .map(|w| w.trim_end_matches("'s").to_lowercase())
        .any(|w| !NOT_NAMES.contains(&w.as_str()) && !known.contains(&w));
    if invented {
        failed.push(Check::NoInventedPeople);
    }
    failed
}

/// Run the check: write each seeded day `runs` times through `write` and judge every sentence.
/// `write` returns the evidence of the day as the engine holds it and the sentences stored.
pub fn run_check(runs: usize, mut write: impl FnMut(&SeedDay) -> (Evidence, Vec<Line>)) -> Report {
    let mut report = Report {
        runs,
        days: DAYS.len(),
        ..Report::default()
    };
    for _ in 0..runs {
        for day in DAYS {
            let (evidence, lines) = write(day);
            report.sentences += lines.len();
            for line in &lines {
                for check in judge(&evidence, line) {
                    report.failures.push(Failure {
                        day: day.id.to_string(),
                        sentence: line.text.clone(),
                        check,
                    });
                }
            }
        }
    }
    report
}
