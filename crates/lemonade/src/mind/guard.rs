//! What a reply must not be, whatever the tier: a leaked control word, a refusal worded as an
//! answer, punctuation, or one of the instructions it was given.

use super::prompts::NOT_SURE;

/// Words in a row copied out of the instructions that make a reply an echo of them.
const ECHO_WORDS: usize = 6;

/// Openings that say the mind has nothing, however it words it.
const REFUSALS: [&str; 20] = [
    "i do not know",
    "i dont know",
    "i am not sure",
    "im not sure",
    "i am unable",
    "i cannot",
    "i cant",
    "i could not find",
    "i couldnt find",
    "i do not have your",
    "i dont have your",
    "i have no information",
    "i have no record",
    "i have nothing",
    "none of the facts",
    "the facts do not",
    "the facts dont",
    "no fact",
    "sorry",
    "unfortunately i",
];

/// Ways of saying the facts hold nothing, anywhere in a reply: they talk about the facts the
/// mind was given, which a person never sees.
const ABSENCES: [&str; 6] = [
    "no information",
    "not mentioned",
    "provided facts",
    "given facts",
    "in the facts",
    "no mention",
];

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace(['\u{2019}', '\''], "")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Removes what a model's own template leaves behind: reasoning blocks and role tokens.
pub fn strip_control(reply: &str) -> String {
    let mut out = String::with_capacity(reply.len());
    let mut rest = reply;
    while let Some(open) = rest.find("<think>") {
        out.push_str(&rest[..open]);
        rest = match rest[open..].find("</think>") {
            Some(close) => &rest[open + close + "</think>".len()..],
            None => "",
        };
    }
    out.push_str(rest);
    let mut text = out.replace("</think>", "");
    while let Some(open) = text.find("<|") {
        let Some(close) = text[open..].find("|>") else {
            break;
        };
        text.replace_range(open..open + close + 2, "");
    }
    text.trim().to_string()
}

/// Whether the mind said it is not sure: the marker, or the marker as it is easily mistyped
/// (any case, with a dash or no separator, or in capitals with a space), or a refusal as the
/// first thing it says.
pub fn not_sure(reply: &str) -> bool {
    let lower = reply.to_lowercase();
    if reply.contains(NOT_SURE)
        || reply.contains("NOT SURE")
        || ["not_sure", "not-sure", "notsure"]
            .iter()
            .any(|m| lower.contains(m))
    {
        return true;
    }
    let said = words(reply).join(" ");
    REFUSALS.iter().any(|r| said.starts_with(r)) || ABSENCES.iter().any(|a| said.contains(a))
}

/// The numbers, dates and times written in `sentences` (the facts an answer cites) that the
/// `answer` does not carry, as they are written there.
pub fn left_out(answer: &str, sentences: &[&str]) -> Vec<String> {
    let said = answer.to_lowercase();
    let mut out: Vec<String> = Vec::new();
    for token in sentences.iter().flat_map(|s| s.split_whitespace()) {
        let token = token.trim_matches(|c: char| !c.is_alphanumeric());
        if token.chars().any(|c| c.is_ascii_digit())
            && !said.contains(&token.to_lowercase())
            && !out.iter().any(|t| t == token)
        {
            out.push(token.to_string());
        }
    }
    out
}

/// The fact (from 1) an answer that cites none is made of: the one whose sentence holds every
/// word of the answer, when only one does and the answer has at least two words. An answer
/// that is the fact's own words is that fact's answer; anything else is left uncited.
pub fn source_of(answer: &str, sentences: &[&str]) -> Option<usize> {
    let said = words(answer);
    if said.len() < 2 {
        return None;
    }
    let mut found = sentences.iter().enumerate().filter(|(_, s)| {
        let held = words(s);
        said.iter().all(|w| held.contains(w))
    });
    match (found.next(), found.next()) {
        (Some((i, _)), None) => Some(i + 1),
        _ => None,
    }
}

/// Whether what would be shown says nothing: no letter or digit left, or nearly all words lifted
/// in runs from the instructions `system`. A line of the rules inside a real answer ("it is not
/// settled which is right") is a small part of it and does not count.
pub fn degenerate(shown: &str, system: &str) -> bool {
    let said = words(shown);
    if said.is_empty() {
        return true;
    }
    if said.len() < ECHO_WORDS {
        return false;
    }
    let given = words(system);
    let mut copied = vec![false; said.len()];
    for (at, w) in said.windows(ECHO_WORDS).enumerate() {
        if given.windows(ECHO_WORDS).any(|g| g == w) {
            copied[at..at + ECHO_WORDS].fill(true);
        }
    }
    copied.iter().filter(|c| **c).count() * 5 > said.len() * 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasoning_blocks_and_role_tokens_are_removed() {
        assert_eq!(
            strip_control("<think>hm</think>On Friday [m1].<|im_end|>"),
            "On Friday [m1]."
        );
        assert_eq!(strip_control("a <think>never closed"), "a");
    }

    #[test]
    fn the_marker_is_found_however_it_is_typed() {
        for reply in [
            "NOT_SURE",
            "Not_Sure.",
            "not-sure",
            "NOT SURE",
            "NOTSURE",
            "It is **Friday** [m1]. NOT_SURE",
        ] {
            assert!(not_sure(reply), "{reply}");
        }
        for reply in ["NOT today: Monday [m1].", "I am sure it is Friday [m1]."] {
            assert!(!not_sure(reply), "{reply}");
        }
    }

    #[test]
    fn a_refusal_that_opens_the_reply_is_not_sure() {
        for reply in [
            "I do not know.",
            "I don't know [m1].",
            "**I'm not sure** about that.",
            "Sorry, the facts say nothing.",
            "None of the facts mention it.",
            "The plumber is not mentioned in the provided facts.",
        ] {
            assert!(not_sure(reply), "{reply}");
        }
        assert!(!not_sure("Marek is coming on Monday 30 November [m1]."));
    }

    #[test]
    fn numbers_dates_and_times_the_answer_dropped_are_named() {
        let fact = [
            "My blood test showed high cholesterol, 5.9.",
            "Dentist on Thursday 19 November at 14:30.",
        ];
        assert_eq!(
            left_out("It showed high **cholesterol**.", &fact),
            ["5.9", "19", "14:30"]
        );
        assert!(left_out("High cholesterol, **5.9**.", &fact[..1]).is_empty());
        assert!(left_out("On Thursday 19 November at **14:30**.", &fact[1..]).is_empty());
        assert!(left_out("anything", &["No digits here."]).is_empty());
    }

    #[test]
    fn an_answer_in_one_facts_own_words_is_that_facts() {
        let facts = [
            "You have seat 31 on the Bristol train.",
            "The Bristol train leaves at 9:10.",
        ];
        assert_eq!(source_of("seat 31 on the Bristol train.", &facts), Some(1));
        assert_eq!(source_of("the Bristol train", &facts), None);
        assert_eq!(source_of("2958", &facts), None);
        assert_eq!(source_of("seat 32 on the train", &facts), None);
    }

    #[test]
    fn punctuation_and_echoed_instructions_are_degenerate() {
        let system =
            "Put the answer words in bold, copied from the fact. Sam is at the old station.";
        assert!(degenerate(",,,,,", system));
        assert!(degenerate(" **  ** ", system));
        assert!(degenerate("Sam is at the old station.", system));
        assert!(degenerate(
            "Put the answer words in bold, copied from the fact.",
            system
        ));
        assert!(!degenerate("Marek comes on Monday 30 November.", system));
        let rules = "Say both and say it is not settled which is right.";
        assert!(!degenerate(
            "Choir is on Thursday at the church. Choir is on Friday at the hall. It is not settled which is right.",
            rules
        ));
        assert!(!degenerate("Sam is at 5.", system));
    }
}
