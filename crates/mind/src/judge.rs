//! Judging a reply to a question: its markers taken out, the guards' verdict on it, and the
//! facts its receipts map back to.

use nocturne_engine::{AnswerMeta, Fact, MindError};
use tokio::sync::mpsc;

use crate::cite::CiteFilter;
use crate::guard;

/// What both the local and the online answer do with a judged reply: the text goes out only
/// when the answer stands, and each cited number maps back to the fact it was given as.
pub async fn conclude(
    judged: Judged,
    facts: &[Fact],
    out: &mpsc::Sender<String>,
) -> Result<AnswerMeta, MindError> {
    let mut sent_any = false;
    if !judged.not_sure && !judged.text.is_empty() {
        sent_any = true;
        if out.send(judged.text.clone()).await.is_err() {
            return Err(MindError::Failed("the reader went away".into()));
        }
    }
    let ids = |numbers: &[usize]| -> Vec<String> {
        numbers
            .iter()
            .filter_map(|n| facts.get(n - 1))
            .map(|f| f.memory.id.clone())
            .collect()
    };
    let (cited, spans) = if judged.not_sure {
        (Vec::new(), Vec::new())
    } else {
        (
            ids(&judged.cited),
            judged.spans.as_deref().map(ids).unwrap_or_default(),
        )
    };
    let cited_none = cited.is_empty();
    Ok(AnswerMeta {
        cited,
        not_sure: judged.not_sure || !sent_any || cited_none,
        spans,
        online: None,
    })
}

/// What a reply comes to once its markers are taken out and the guards have looked at it.
pub struct Judged {
    /// What the person would read.
    pub text: String,
    pub not_sure: bool,
    /// The facts cited, from 1, in the order first cited.
    pub cited: Vec<usize>,
    /// The fact each bold span stands for, when they can all be paired.
    pub spans: Option<Vec<usize>>,
}

impl Judged {
    pub fn cited_sentences<'a>(&self, facts: &'a [Fact]) -> Vec<&'a str> {
        self.cited
            .iter()
            .filter_map(|n| facts.get(n - 1))
            .map(|f| f.memory.sentence.as_str())
            .collect()
    }
}

/// The cited facts that disagree with a fact the answer does not give: (cited, other side),
/// both from 1. A side is given when it is cited and the answer says what sets it apart from
/// the other side ([`guard::carries`]); a marker alone does not give it.
pub fn one_sided(judged: &Judged, facts: &[Fact]) -> Vec<(usize, usize)> {
    let given = |n: usize, m: usize| {
        let sentence = |i: usize| facts.get(i - 1).map(|f| f.memory.sentence.as_str());
        judged.cited.contains(&n)
            && sentence(n).is_some_and(|s| {
                guard::carries(
                    &judged.text,
                    s,
                    &sentence(m).into_iter().collect::<Vec<_>>(),
                )
            })
    };
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &n in &judged.cited {
        let Some(fact) = facts.get(n - 1) else {
            continue;
        };
        for id in &fact.disputed_by {
            let Some(m) = facts.iter().position(|f| &f.memory.id == id).map(|i| i + 1) else {
                continue;
            };
            let seen = out.iter().any(|&(a, b)| (a, b) == (m, n));
            if !seen && !(given(n, m) && given(m, n)) {
                out.push((n, m));
            }
        }
    }
    out
}

/// What is wrong with an answer a Tiny mind gave, in the words it is told: it cites nothing, it
/// leaves out a number, date or time of a cited fact, or it cites a fact that disagrees with
/// another and not the other.
pub fn complaints(judged: &Judged, facts: &[Fact]) -> Vec<String> {
    let mut out = Vec::new();
    if judged.cited.is_empty() {
        out.push(
            "Write a whole sentence and end it with the number of the fact it comes from, like [m1]."
                .to_string(),
        );
    }
    let left_out = guard::left_out(&judged.text, &judged.cited_sentences(facts));
    if !left_out.is_empty() {
        out.push(format!(
            "You left out {}. Write {} exactly as the fact does.",
            left_out.join(", "),
            if left_out.len() == 1 { "it" } else { "them" }
        ));
    }
    for (n, m) in one_sided(judged, facts) {
        out.push(format!(
            "[m{n}] does not agree with [m{m}]. Give both, and say it is not settled which is right."
        ));
    }
    out
}

/// Both sides of a disagreement in the facts' own words, for when the mind gave one twice.
pub fn both_sides(n: usize, m: usize, facts: &[Fact]) -> String {
    let sentence = |i: usize| facts[i - 1].memory.sentence.trim().to_string();
    format!(
        "{} [m{n}] {} [m{m}] It is not settled which is right.",
        sentence(n),
        sentence(m)
    )
}

pub fn judge(reply: &str, facts: &[Fact], system: &str) -> Judged {
    let reply = guard::strip_control(reply);
    let mut filter = CiteFilter::new(facts.len());
    let mut text = filter.push(&reply);
    text.push_str(&filter.finish());
    let cited = filter.cited().to_vec();
    // Nothing but punctuation, or the instructions read back, is not an answer.
    let not_sure = guard::not_sure(&reply, !cited.is_empty()) || guard::degenerate(&text, system);
    Judged {
        text,
        not_sure,
        cited,
        spans: filter.bold_facts(),
    }
}
