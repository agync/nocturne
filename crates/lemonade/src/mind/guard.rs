//! What a reply must not be, whatever the tier: a leaked control word, a refusal worded as an
//! answer, punctuation, or one of the instructions it was given. And what a reading must not
//! lose: a number or name in the item that no memory keeps, who someone is to the person, or a
//! day the item never named.

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

/// Titles that come before a name and are not one.
const TITLES: [&str; 8] = ["mr", "mrs", "ms", "miss", "mister", "missus", "dr", "st"];

/// Words that say who someone is to the person, or what they do for them.
const ROLES: &[&str] = &[
    "son",
    "daughter",
    "child",
    "mum",
    "mom",
    "mother",
    "dad",
    "father",
    "brother",
    "sister",
    "wife",
    "husband",
    "partner",
    "cousin",
    "aunt",
    "auntie",
    "uncle",
    "gran",
    "granny",
    "grandma",
    "grandad",
    "grandpa",
    "grandmother",
    "grandfather",
    "nephew",
    "niece",
    "friend",
    "neighbour",
    "boss",
    "manager",
    "colleague",
    "landlord",
    "landlady",
    "plumber",
    "electrician",
    "builder",
    "engineer",
    "dentist",
    "doctor",
    "gp",
    "vet",
    "teacher",
    "tutor",
    "coach",
    "accountant",
    "solicitor",
    "lawyer",
    "mechanic",
    "cleaner",
    "childminder",
    "nanny",
    "nurse",
    "physio",
    "optician",
    "therapist",
    "carer",
    "babysitter",
    "gardener",
    "hairdresser",
    "barber",
    "midwife",
    "pharmacist",
];

const WEEKDAYS: [(&str, Weekday); 7] = [
    ("monday", Weekday::Monday),
    ("tuesday", Weekday::Tuesday),
    ("wednesday", Weekday::Wednesday),
    ("thursday", Weekday::Thursday),
    ("friday", Weekday::Friday),
    ("saturday", Weekday::Saturday),
    ("sunday", Weekday::Sunday),
];

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

/// A token with the punctuation around it, and any possessive, taken off.
fn bare(token: &str) -> &str {
    let t = token.trim_matches(|c: char| !c.is_alphanumeric());
    t.strip_suffix("'s")
        .or_else(|| t.strip_suffix("\u{2019}s"))
        .unwrap_or(t)
}

/// The sentences of `text`: each line, split again after a full stop, question or exclamation
/// mark that ends a word other than a title or an initial.
pub fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut current: Vec<&str> = Vec::new();
        for token in line.split_whitespace() {
            current.push(token);
            let ends = token.ends_with(['.', '!', '?']);
            let word = bare(token).to_lowercase();
            let title = TITLES.contains(&word.as_str()) || word.chars().count() == 1;
            if ends && !title {
                out.push(current.join(" "));
                current.clear();
            }
        }
        if !current.is_empty() {
            out.push(current.join(" "));
        }
    }
    out
}

/// The words of a sentence a memory of it must keep: every number or code (a word with a
/// digit), and every name (a capitalised word that does not open the sentence). A word in
/// capitals throughout needs a digit to count, since a heading in capitals is not a name, and
/// titles, days and months do not count on their own.
pub fn key_words(sentence: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, token) in sentence.split_whitespace().enumerate() {
        let word = bare(token);
        if word.is_empty() {
            continue;
        }
        let digit = word.chars().any(|c| c.is_ascii_digit());
        let lower = word.to_lowercase();
        let capitalised = word.chars().next().is_some_and(char::is_uppercase);
        let shouting = word
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(char::is_uppercase);
        let name = i > 0
            && capitalised
            && !shouting
            && lower != "i"
            && !TITLES.contains(&lower.as_str())
            && !MONTHS.contains(&lower.as_str())
            && !WEEKDAYS
                .iter()
                .any(|(d, _)| lower.trim_end_matches('s') == *d);
        if (digit || name) && !out.contains(&lower) {
            out.push(lower);
        }
    }
    out
}

/// Whether `words` (already normalised) hold `key` as whole words.
fn holds(words: &str, key: &str) -> bool {
    let key = normalise(key);
    !key.is_empty() && format!(" {words} ").contains(&format!(" {key} "))
}

/// The sentences of `text` with a number, code or name that nothing in `said` (what the
/// memories quote and say) holds.
pub fn uncovered(text: &str, said: &[String]) -> Vec<String> {
    let words = normalise(&said.join(" | "));
    sentences(text)
        .into_iter()
        .filter(|s| key_words(s).iter().any(|k| !holds(&words, k)))
        .collect()
}

/// Who a sentence names by what they are to the person: (role, name) for "my son Leo", "the
/// plumber, Mister Hughes", "the engineer is called Marek", "Mrs Carver, our neighbour" and
/// "Leo is my son". English only.
pub fn roles(sentence: &str) -> Vec<(String, String)> {
    let tokens: Vec<&str> = sentence.split_whitespace().collect();
    let lower: Vec<String> = tokens.iter().map(|t| bare(t).to_lowercase()).collect();
    let is_name = |i: usize| {
        let w = bare(tokens[i]);
        w.chars().next().is_some_and(char::is_uppercase)
            && w.chars().all(|c| c.is_alphabetic() || c == '-')
            && !TITLES.contains(&lower[i].as_str())
            && !ROLES.contains(&lower[i].as_str())
    };
    let mut out = Vec::new();
    for (i, role) in lower.iter().enumerate() {
        if !ROLES.contains(&role.as_str()) {
            continue;
        }
        let mut found = None;
        let mut j = i + 1;
        while j < tokens.len() && j <= i + 3 {
            if is_name(j) {
                found = Some(j);
                break;
            }
            if !["is", "was", "called", "named"].contains(&lower[j].as_str())
                && !TITLES.contains(&lower[j].as_str())
            {
                break;
            }
            j += 1;
        }
        let owner = |k: usize| ["my", "our", "the", "your"].contains(&lower[k].as_str());
        if found.is_none() && i >= 2 && owner(i - 1) {
            if tokens[i - 2].ends_with(',') && is_name(i - 2) {
                found = Some(i - 2);
            } else if i >= 3 && lower[i - 2] == "is" && is_name(i - 3) {
                found = Some(i - 3);
            }
        }
        if let Some(n) = found {
            out.push((role.clone(), bare(tokens[n]).to_string()));
        }
    }
    out
}

/// How many words in `text` say who someone is to the person.
pub fn role_words(text: &str) -> usize {
    text.split_whitespace()
        .filter(|t| ROLES.contains(&bare(t).to_lowercase().as_str()))
        .count()
}

/// The sentences of `text` that name someone by a role that every memory about them lost.
/// `memories` is what each memory says (subject, sentence and object together).
pub fn lost_roles(text: &str, memories: &[String]) -> Vec<String> {
    let said: Vec<String> = memories.iter().map(|m| normalise(m)).collect();
    sentences(text)
        .into_iter()
        .filter(|s| {
            roles(s).iter().any(|(role, name)| {
                let about: Vec<&String> = said.iter().filter(|m| holds(m, name)).collect();
                !about.is_empty() && !about.iter().any(|m| holds(m, role))
            })
        })
        .collect()
}

/// Whether `quote` names the day `due` falls on: by its weekday or its day of the month, or
/// by a word such as tomorrow that counts from another day. A weekday or month the quote names
/// that is not `due`'s rules it out whatever else the quote says.
pub fn day_supported(due: Date, quote: &str) -> bool {
    let words = normalise(quote);
    let tokens: Vec<&str> = words.split(' ').collect();
    let weekdays: Vec<Weekday> = tokens
        .iter()
        .filter_map(|t| {
            WEEKDAYS
                .iter()
                .find(|(d, _)| t.trim_end_matches('s') == *d)
                .map(|(_, w)| *w)
        })
        .collect();
    if !weekdays.is_empty() && !weekdays.contains(&due.weekday()) {
        return false;
    }
    let months: Vec<usize> = tokens
        .iter()
        .filter_map(|t| MONTHS.iter().position(|m| t == m))
        .collect();
    if !months.is_empty() && !months.contains(&(due.month() as usize - 1)) {
        return false;
    }
    let day = due.day() as usize;
    let numbered = tokens.iter().any(|t| {
        let digits = t.trim_end_matches(|c: char| c.is_ascii_alphabetic());
        digits.parse::<usize>() == Ok(day)
    }) || holds(&words, ORDINALS[day - 1]);
    let relative = RELATIVE.iter().any(|r| tokens.contains(r))
        || tokens.windows(2).any(|w| {
            ["next", "this", "coming"].contains(&w[0]) && ["week", "month", "year"].contains(&w[1])
        })
        || tokens.windows(3).any(|w| {
            w[0] == "in"
                && (w[1].parse::<u32>().is_ok() || ["a", "an", "two", "three"].contains(&w[1]))
                && ["day", "days", "week", "weeks", "month", "months"].contains(&w[2])
        });
    !weekdays.is_empty() || numbered || relative
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
    fn sentences_split_at_line_ends_and_full_stops_but_not_after_titles() {
        assert_eq!(
            sentences("Boiler on Monday 30 November. The engineer is Mr. Marek\nBAY C7"),
            [
                "Boiler on Monday 30 November.",
                "The engineer is Mr. Marek",
                "BAY C7"
            ]
        );
    }

    #[test]
    fn numbers_codes_and_names_are_the_words_to_keep() {
        assert_eq!(
            key_words("The engineer is called Marek, booked for Monday 30 November at 8:00."),
            ["marek", "30", "8:00"]
        );
        assert_eq!(key_words("PERMIT NUMBER 4471"), ["4471"]);
        assert_eq!(
            key_words("Marek is the plumber Mr Hughes's friend."),
            ["hughes"]
        );
        assert!(key_words("Reminder to myself.").is_empty());
    }

    #[test]
    fn a_line_whose_name_no_memory_keeps_is_uncovered() {
        let text = "Boiler service is booked for Monday 30 November, between 8 and 12. The engineer is called Marek.";
        let said =
            ["Boiler service is booked for Monday 30 November, between 8 and 12".to_string()];
        assert_eq!(uncovered(text, &said), ["The engineer is called Marek."]);
        let all = [said[0].clone(), "Marek is the engineer.".to_string()];
        assert!(uncovered(text, &all).is_empty());
        let times = ["Parents' evening is at 18:00.".to_string()];
        assert!(uncovered("Parents' evening at 18:00.", &times).is_empty());
    }

    #[test]
    fn roles_are_paired_with_the_names_they_belong_to() {
        assert_eq!(
            roles("Parents' evening for my son Leo is on Thursday."),
            [("son".into(), "Leo".into())]
        );
        assert_eq!(
            roles("The plumber, Mister Hughes, is coming on Friday."),
            [("plumber".to_string(), "Hughes".to_string())]
        );
        assert_eq!(
            roles("The engineer is called Marek."),
            [("engineer".into(), "Marek".into())]
        );
        assert_eq!(
            roles("Mrs Carver, our neighbour, called."),
            [("neighbour".into(), "Carver".into())]
        );
        assert_eq!(roles("Leo is my son."), [("son".into(), "Leo".into())]);
        assert!(roles("Gran has moved to York.").is_empty());
    }

    #[test]
    fn a_role_lost_from_every_memory_about_someone_is_found() {
        let text = "Reminder to myself. The plumber, Mister Hughes, is coming on Friday.";
        let lost = ["Hughes | Mr Hughes is coming on Friday. | Friday".to_string()];
        assert_eq!(
            lost_roles(text, &lost),
            ["The plumber, Mister Hughes, is coming on Friday."]
        );
        let kept = ["Hughes | The plumber, Mr Hughes, is coming on Friday. | Friday".to_string()];
        assert!(lost_roles(text, &kept).is_empty());
        assert!(
            lost_roles(text, &[]).is_empty(),
            "a name no memory has is not this check's"
        );
        assert_eq!(role_words("the plumber and my son"), 2);
    }

    #[test]
    fn a_day_the_quote_never_names_is_not_supported() {
        let d = |s: &str| s.parse::<Date>().unwrap();
        // 2026-11-12 is a Thursday.
        assert!(!day_supported(
            d("2026-11-12"),
            "book club is on the first Tuesday of the month"
        ));
        assert!(!day_supported(d("2026-11-20"), "Sam likes tea"));
        assert!(!day_supported(d("2026-11-30"), "valid until 30 June"));
        assert!(day_supported(d("2026-11-12"), "Dentist moved to Thursday"));
        assert!(day_supported(
            d("2027-01-15"),
            "The annual fee of 30 pounds is due by 15 January"
        ));
        assert!(day_supported(
            d("2026-11-20"),
            "on Friday the twentieth of November"
        ));
        assert!(day_supported(d("2026-11-21"), "on the 21st"));
        assert!(day_supported(
            d("2026-11-15"),
            "I will book the tickets tomorrow"
        ));
        assert!(day_supported(d("2026-11-21"), "I will call them next week"));
        assert!(day_supported(
            d("2026-11-24"),
            "the results come in 10 days"
        ));
        assert!(!day_supported(d("2026-11-24"), "the results come in"));
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
