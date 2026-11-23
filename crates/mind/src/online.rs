//! The words of one question asked online, whoever carries it: the full instructions and up to
//! [`MAX_FACTS`] facts, fenced with a fresh nonce like a local answer, and the template its
//! receipt keeps. The reply is judged on this device by [`crate::judge`].

use nocturne_contract::{SentLine, SentMemory};
use nocturne_engine::{Fact, NewSend};

use crate::prompts;

/// The most remembered sentences one question sends.
pub const MAX_FACTS: usize = 12;
/// The most an answer written online may be, in tokens.
pub const MAX_TOKENS: u32 = 600;
/// The one question of a connection check.
pub const CHECK_QUESTION: &str = "Reply with the word ready.";
/// The most its answer may be, in tokens.
pub const CHECK_MAX_TOKENS: u32 = 8;

/// A question ready to send, made before anything leaves.
pub struct Question {
    pub system: String,
    pub user: String,
    /// The facts the question numbers `[m1]`..`[mN]`, in order.
    pub facts: Vec<Fact>,
    /// How many of the facts handed in were dropped by the last look at what may be sent.
    pub dropped: usize,
    /// `template[0]` is the system message; the rest is the question's message, line by line,
    /// each memory's sentence replaced by [`NewSend::SLOT`].
    pub template: Vec<SentLine>,
    /// The hash of each memory line exactly as sent, in order.
    pub line_hashes: Vec<String>,
}

/// The question for `question`: the first [`MAX_FACTS`] facts that `sendable` still allows,
/// numbered again from 1 without gaps. `None` when none may go. A section of the Nocturne guide
/// never goes: help is written on this device.
pub fn question(
    question: &str,
    facts: &[Fact],
    sendable: &dyn Fn(&[String]) -> Vec<bool>,
) -> Option<Question> {
    let first: Vec<Fact> = facts
        .iter()
        .filter(|f| !nocturne_engine::is_guide_section(&f.memory.id))
        .take(MAX_FACTS)
        .cloned()
        .collect();
    let ids: Vec<String> = first.iter().map(|f| f.memory.id.clone()).collect();
    let ok = sendable(&ids);
    let facts: Vec<Fact> = first
        .into_iter()
        .zip(ok.iter().chain(std::iter::repeat(&false)))
        .filter_map(|(f, ok)| ok.then_some(f))
        .collect();
    let dropped = ids.len() - facts.len();
    if facts.is_empty() {
        return None;
    }
    let nonce = prompts::nonce();
    let tier = prompts::ONLINE_TIER;
    let system = prompts::answer_system(tier, &nonce);
    let user = prompts::answer_user(tier, question, &facts, &nonce, None);
    let (head, tail) = prompts::answer_frame(question, &nonce, None);
    let mut template = vec![SentLine::Text {
        text: system.clone(),
    }];
    template.extend(head.into_iter().map(|text| SentLine::Text { text }));
    let mut line_hashes = Vec::new();
    for (i, (f, (before, after))) in facts
        .iter()
        .zip(prompts::fact_lines(tier, &facts))
        .enumerate()
    {
        let sent = format!("{before}{}{after}", f.memory.sentence);
        line_hashes.push(NewSend::hash_line(&sent));
        let after = match &f.memory.was {
            Some(was) => after.replacen(
                &format!("before, it was {was}"),
                &format!("before, it was {}", NewSend::WAS_SLOT),
                1,
            ),
            None => after,
        };
        template.push(SentLine::Memory {
            n: i as u32 + 1,
            text: Some(format!("{before}{}{after}", NewSend::SLOT)),
            state: SentMemory::Same,
        });
    }
    template.extend(tail.into_iter().map(|text| SentLine::Text { text }));
    Some(Question {
        system,
        user,
        facts,
        dropped,
        template,
        line_hashes,
    })
}
