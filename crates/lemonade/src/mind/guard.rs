//! What a reply must not be, whatever the tier: a leaked control word, a refusal worded as an
//! answer, punctuation, or one of the instructions it was given. And a day a reading gives that
//! the item never named.

use jiff::civil::{Date, Weekday};

use super::normalise;
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
/// first thing it says. Talk of the facts holding nothing counts only in an answer that
/// `cites` none: "his surname is not mentioned" beside a cited fact is still an answer.
pub fn not_sure(reply: &str, cites: bool) -> bool {
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
    REFUSALS.iter().any(|r| said.starts_with(r))
        || (!cites && ABSENCES.iter().any(|a| said.contains(a)))
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

/// Weekdays as they are written in full or short.
const WEEKDAYS: [(&[&str], Weekday); 7] = [
    (&["monday", "mon"], Weekday::Monday),
    (&["tuesday", "tue", "tues"], Weekday::Tuesday),
    (&["wednesday", "wed", "weds"], Weekday::Wednesday),
    (&["thursday", "thu", "thur", "thurs"], Weekday::Thursday),
    (&["friday", "fri"], Weekday::Friday),
    (&["saturday", "sat"], Weekday::Saturday),
    (&["sunday", "sun"], Weekday::Sunday),
];

/// Months as they are written in full or short.
const MONTHS: [&[&str]; 12] = [
    &["january", "jan"],
    &["february", "feb"],
    &["march", "mar"],
    &["april", "apr"],
    &["may"],
    &["june", "jun"],
    &["july", "jul"],
    &["august", "aug"],
    &["september", "sep", "sept"],
    &["october", "oct"],
    &["november", "nov"],
    &["december", "dec"],
];

const ORDINALS: [&str; 31] = [
    "first",
    "second",
    "third",
    "fourth",
    "fifth",
    "sixth",
    "seventh",
    "eighth",
    "ninth",
    "tenth",
    "eleventh",
    "twelfth",
    "thirteenth",
    "fourteenth",
    "fifteenth",
    "sixteenth",
    "seventeenth",
    "eighteenth",
    "nineteenth",
    "twentieth",
    "twenty first",
    "twenty second",
    "twenty third",
    "twenty fourth",
    "twenty fifth",
    "twenty sixth",
    "twenty seventh",
    "twenty eighth",
    "twenty ninth",
    "thirtieth",
    "thirty first",
];

/// Words that place a day by its distance from another, not by its name or number.
const RELATIVE: [&str; 6] = [
    "today",
    "tonight",
    "tomorrow",
    "yesterday",
    "weekend",
    "fortnight",
];

/// Phrases that do the same.
const RELATIVE_PHRASES: [&str; 12] = [
    "this morning",
    "this afternoon",
    "this evening",
    "end of the week",
    "end of this week",
    "end of the month",
    "end of this month",
    "next week",
    "this week",
    "next month",
    "this month",
    "coming week",
];

/// Whether `words` (already normalised) hold `key` as whole words.
fn holds(words: &str, key: &str) -> bool {
    let key = normalise(key);
    !key.is_empty() && format!(" {words} ").contains(&format!(" {key} "))
}

/// Whether `context` (a memory's quote, the line it stands in and the heading above) names the
/// day `due` falls on: by its day of the month, its weekday, or a word such as tomorrow that
/// counts from another day. When no day number matches, a weekday the context names that is
/// not `due`'s rules it out; a month it names rules it out when it names none that is.
pub fn day_supported(due: Date, context: &str) -> bool {
    let words = normalise(context);
    let tokens: Vec<&str> = words.split(' ').collect();
    let day = due.day() as usize;
    let numbered = tokens.iter().any(|t| {
        let digits = t.trim_end_matches(|c: char| c.is_ascii_alphabetic());
        digits.parse::<usize>() == Ok(day)
    }) || holds(&words, ORDINALS[day - 1]);
    let months: Vec<usize> = tokens
        .iter()
        .filter_map(|t| MONTHS.iter().position(|m| m.contains(t)))
        .collect();
    if !months.is_empty() && !months.contains(&(due.month() as usize - 1)) {
        return false;
    }
    if numbered {
        return true;
    }
    let weekdays: Vec<Weekday> = tokens
        .iter()
        .filter_map(|t| {
            let one = t
                .strip_suffix('s')
                .filter(|s| s.ends_with("day"))
                .unwrap_or(t);
            WEEKDAYS
                .iter()
                .find(|(names, _)| names.contains(&one))
                .map(|(_, w)| *w)
        })
        .collect();
    if !weekdays.is_empty() && !weekdays.contains(&due.weekday()) {
        return false;
    }
    let units = ["day", "days", "week", "weeks", "month", "months"];
    let relative = RELATIVE.iter().any(|r| tokens.contains(r))
        || RELATIVE_PHRASES.iter().any(|p| holds(&words, p))
        || tokens.windows(3).any(|w| {
            w[0] == "in"
                && (w[1].parse::<u32>().is_ok() || ["a", "an", "two", "three"].contains(&w[1]))
                && units.contains(&w[2])
        })
        || tokens
            .windows(3)
            .any(|w| w[0] == "couple" && w[1] == "of" && units.contains(&w[2]));
    !weekdays.is_empty() || relative
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
            assert!(not_sure(reply, true), "{reply}");
        }
        for reply in ["NOT today: Monday [m1].", "I am sure it is Friday [m1]."] {
            assert!(!not_sure(reply, true), "{reply}");
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
            assert!(not_sure(reply, false), "{reply}");
        }
        assert!(!not_sure(
            "Marek is coming on Monday 30 November [m1].",
            true
        ));
    }

    #[test]
    fn talk_of_missing_facts_voids_only_an_answer_that_cites_nothing() {
        let partial =
            "Marek services the boiler on Monday 30 November [m1]. His surname is not mentioned.";
        assert!(!not_sure(partial, true));
        assert!(not_sure("His surname is not mentioned.", false));
        assert!(
            not_sure("I do not know [m1].", true),
            "a refusal stays a refusal"
        );
        assert!(
            not_sure("Monday [m1]. NOT_SURE", true),
            "and so does the marker"
        );
    }

    #[test]
    fn a_day_the_item_never_names_is_not_supported() {
        let d = |s: &str| s.parse::<Date>().unwrap();
        // 2026-11-12 is a Thursday; 2026-11-14, today, a Saturday.
        let no = [
            (
                "2026-11-12",
                "book club is on the first Tuesday of the month",
            ),
            ("2026-11-20", "Sam likes tea"),
            ("2026-11-30", "valid until 30 June"),
            ("2026-11-24", "the results come in"),
            ("2026-11-19", "Weds at the hall"),
        ];
        for (due, context) in no {
            assert!(!day_supported(d(due), context), "{due} {context}");
        }
        let yes = [
            ("2026-11-12", "Dentist moved to Thursday"),
            ("2026-11-12", "Dentist moved to Thurs"),
            ("2026-11-20", "pick up the keys on Fri"),
            ("2026-11-17", "bins go out on Tues"),
            ("2026-11-18", "choir on Weds"),
            ("2026-11-16", "Leo swims on Mondays"),
            (
                "2027-01-15",
                "The annual fee of 30 pounds is due by 15 January",
            ),
            ("2027-01-15", "fee due 15 Jan"),
            ("2026-11-20", "on Friday the twentieth of November"),
            ("2026-11-21", "on the 21st"),
            // A matching day number wins over a weekday written wrong.
            ("2026-11-19", "Wednesday 19 November"),
            ("2026-11-15", "I will book the tickets tomorrow"),
            ("2026-11-14", "call the garage this afternoon"),
            ("2026-11-14", "drinks this evening"),
            ("2026-11-20", "send it by the end of the week"),
            ("2026-11-21", "I will call them next week"),
            ("2026-11-24", "the results come in 10 days"),
            ("2026-12-05", "the parts arrive in 3 weeks"),
            ("2026-11-16", "it takes a couple of days"),
            ("2026-11-17", "the plumber comes in a couple of days"),
        ];
        for (due, context) in yes {
            assert!(day_supported(d(due), context), "{due} {context}");
        }
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
