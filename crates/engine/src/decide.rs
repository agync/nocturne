//! Decisions: a typed question about a short text, answered with a probability for each answer
//! (decision 0016). A [`Decider`] answers; code holds the threshold and keeps control.
//!
//! Today every decision runs in shadow. The rules and word lists still act; what a decider would
//! have said is logged beside them (`decide::log`), so the evaluation can compare the two before any
//! decision kind is allowed to act. A decider that fails or is too slow leaves the rule's outcome,
//! and never stops an item from being read.

pub(crate) mod log;

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::mind::{BoxFuture, Mind, MindError};

/// What kind of answer a question takes.
#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// Yes or no, answered as `yes` and `no`.
    YesNo,
    /// One of a fixed list of options, each with a description.
    Choice(Vec<(String, String)>),
    /// A level on a scale, lowest first.
    Score(Vec<String>),
}

/// One question asked about a state.
#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    pub instructions: String,
    pub ask: Ask,
}

impl Question {
    /// Every answer the question allows, in order.
    pub fn labels(&self) -> Vec<String> {
        match &self.ask {
            Ask::YesNo => vec!["yes".into(), "no".into()],
            Ask::Choice(options) => options.iter().map(|(o, _)| o.clone()).collect(),
            Ask::Score(levels) => levels.clone(),
        }
    }
}

/// A state and the question asked about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub state: String,
    pub question: Question,
}

/// A decider's answer: the most likely label, the probability of each label it gave, and who
/// answered (the model, for the log).
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub label: String,
    pub probabilities: BTreeMap<String, f64>,
    pub by: String,
}

impl Answer {
    /// The probability given to `label`, 0 when none was given.
    pub fn p(&self, label: &str) -> f64 {
        self.probabilities.get(label).copied().unwrap_or(0.0)
    }

    /// An answer that is certain of `label`, as a mind's single reply is read.
    pub fn certain(label: &str, by: &str) -> Answer {
        Answer {
            label: label.to_string(),
            probabilities: BTreeMap::from([(label.to_string(), 1.0)]),
            by: by.to_string(),
        }
    }
}

/// Answers decisions. The first decider is the mind Nocturne already loads ([`MindDecider`]); a
/// decision model behind the engine's `/v1/systemone` answers through the same seam.
pub trait Decider: Send + Sync {
    fn decide<'a>(&'a self, decision: &'a Decision) -> BoxFuture<'a, Result<Answer, MindError>>;
    /// Where its probabilities come from, which picks the thresholds.
    fn backend(&self) -> Backend;
}

/// The loaded mind as a decider.
pub struct MindDecider(pub Arc<dyn Mind>);

impl Decider for MindDecider {
    fn decide<'a>(&'a self, decision: &'a Decision) -> BoxFuture<'a, Result<Answer, MindError>> {
        self.0.decide(decision)
    }

    fn backend(&self) -> Backend {
        Backend::Mind
    }
}

/// Where a decider's probabilities come from. Each has its own calibration, so no threshold is
/// shared between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// The mind's single reply, read as certain.
    Mind,
    /// A decision model on llama.cpp.
    LlamaCpp,
    /// A decision model on FastFlowLM, on the NPU.
    Flm,
}

impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Mind => "mind",
            Backend::LlamaCpp => "llamacpp",
            Backend::Flm => "flm",
        }
    }
}

/// The decisions asked in shadow: the rows of the research marked a good fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// Did a new note move or cancel a kept appointment? (`memory.rs`, `says_moved`)
    Moved,
    /// Is a memory a health, money or ID detail? (`sensitive.rs`) Recorded only: it may one day
    /// add marks, never remove them.
    Sensitive,
    /// Has the person already written how a planned thing went? (the page question, rule 5)
    FollowedUp,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Moved, Kind::Sensitive, Kind::FollowedUp];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Moved => "moved",
            Kind::Sensitive => "sensitive",
            Kind::FollowedUp => "followed_up",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// The fixed question for this kind.
    pub fn question(self) -> Question {
        match self {
            Kind::Moved => Question {
                instructions: "What does the new note do to the kept appointment?".into(),
                ask: Ask::Choice(vec![
                    (
                        "moved".into(),
                        "It says this same appointment now happens at a different day or time"
                            .into(),
                    ),
                    (
                        "cancelled".into(),
                        "It says this same appointment will not happen".into(),
                    ),
                    (
                        "same".into(),
                        "It repeats or confirms the kept appointment without changing it".into(),
                    ),
                    (
                        "unrelated".into(),
                        "It is about something else, or about someone else's appointment".into(),
                    ),
                ]),
            },
            Kind::Sensitive => Question {
                instructions: "Is this a detail about someone's health, money, or identity \
                               documents and numbers?"
                    .into(),
                ask: Ask::YesNo,
            },
            Kind::FollowedUp => Question {
                instructions: "Does the later writing say how the planned thing went?".into(),
                ask: Ask::YesNo,
            },
        }
    }

    /// The labels that mean "yes" for this kind: moved, a health detail, already written about.
    fn yes_labels(self) -> &'static [&'static str] {
        match self {
            Kind::Moved => &["moved", "cancelled"],
            Kind::Sensitive | Kind::FollowedUp => &["yes"],
        }
    }

    fn no_labels(self) -> &'static [&'static str] {
        match self {
            Kind::Moved => &["same", "unrelated"],
            Kind::Sensitive | Kind::FollowedUp => &["no"],
        }
    }

    /// The verdict that would lose something if it were wrong: a wrong move closes the old
    /// appointment, a missed health detail could leave the computer through Online answers, and
    /// a wrong "already written" drops the person's one chance to say how it went.
    pub fn losing_verdict(self) -> bool {
        match self {
            Kind::Moved | Kind::FollowedUp => true,
            Kind::Sensitive => false,
        }
    }
}

/// How sure a decider must be before its verdict counts, per question and per backend. Until the
/// decision check calibrates them on labelled data, these are cautious starting points: a move
/// must be very sure (it would close a row), a health detail may be marked at less (marks are
/// only ever added), and the page question sits between.
pub fn threshold(kind: Kind, backend: Backend) -> f64 {
    match (kind, backend) {
        (Kind::Moved, Backend::Mind) => 0.9,
        (Kind::Moved, Backend::LlamaCpp) => 0.93,
        (Kind::Moved, Backend::Flm) => 0.9,
        (Kind::Sensitive, Backend::Mind) => 0.5,
        (Kind::Sensitive, Backend::LlamaCpp) => 0.6,
        (Kind::Sensitive, Backend::Flm) => 0.65,
        (Kind::FollowedUp, Backend::Mind) => 0.8,
        (Kind::FollowedUp, Backend::LlamaCpp) => 0.85,
        (Kind::FollowedUp, Backend::Flm) => 0.85,
    }
}

/// The decider's verdict for `kind` (true: moved, a health detail, already written about), or
/// none when neither side reaches the threshold.
pub fn verdict(kind: Kind, backend: Backend, answer: &Answer) -> Option<bool> {
    let t = threshold(kind, backend);
    let yes: f64 = kind.yes_labels().iter().map(|l| answer.p(l)).sum();
    let no: f64 = kind.no_labels().iter().map(|l| answer.p(l)).sum();
    if yes >= t {
        Some(true)
    } else if no >= t {
        Some(false)
    } else {
        None
    }
}

/// A logged decision still waiting for a decider.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub id: String,
    pub kind: Kind,
    /// The item and memory it is about, where there is one.
    pub item_id: Option<String>,
    pub memory_id: Option<String>,
    pub decision: Decision,
    /// What the rule said (true: moved, a health detail, already written about).
    pub rule: bool,
}

/// What came of asking a decider about a pending decision.
#[derive(Debug, Clone, PartialEq)]
pub struct Settled {
    pub backend: Backend,
    /// The answer, or why there was none (an error, a timeout).
    pub answer: Result<Answer, String>,
    pub ms: u64,
}

/// One row of the decision log, as the evaluation reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Logged {
    pub id: String,
    pub kind: Kind,
    pub item_id: Option<String>,
    pub memory_id: Option<String>,
    pub rule: bool,
    /// How the rule acted, where it says more than yes or no (`replaces`, `asks`, `separate`).
    pub rule_detail: Option<String>,
    pub created_at: String,
    pub asked_at: Option<String>,
    pub decider: Option<String>,
    pub label: Option<String>,
    pub p: Option<f64>,
    pub verdict: Option<bool>,
    pub ms: Option<u64>,
    pub error: Option<String>,
}
