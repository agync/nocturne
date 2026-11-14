//! Whether a reading kept what an item says: every number and name in it, and who someone is to
//! the person. The reader uses it to decide when to give an item a second look, and the tidy to
//! pick the items to read again carefully. English patterns only.

/// Titles that come before a name and are not one.
const TITLES: [&str; 8] = ["mr", "mrs", "ms", "miss", "mister", "missus", "dr", "st"];

/// Words a note opens with or signs off with, before a name that is only being addressed.
const GREETINGS: [&str; 10] = [
    "thanks", "thank", "hi", "hello", "hey", "dear", "cheers", "bye", "love", "you",
];

const DAYS_AND_MONTHS: [&str; 19] = [
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

/// Lower case, with every run of other characters than letters and digits made one space.
pub fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(word.chars().flat_map(char::to_lowercase));
    }
    out
}

/// A token with the punctuation around it, and any possessive, taken off.
fn bare(token: &str) -> &str {
    let t = token.trim_matches(|c: char| !c.is_alphanumeric());
    t.strip_suffix("'s")
        .or_else(|| t.strip_suffix("\u{2019}s"))
        .unwrap_or(t)
}

/// Whether `words` (already normalised) hold `key` as whole words.
fn holds(words: &str, key: &str) -> bool {
    let key = normalise(key);
    !key.is_empty() && format!(" {words} ").contains(&format!(" {key} "))
}

/// The sentences of `text`: each line, split again after a full stop, question or exclamation
/// mark that ends a word other than a title or an initial.
pub fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut current: Vec<&str> = Vec::new();
        for token in line.split_whitespace() {
            current.push(token);
            let word = bare(token).to_lowercase();
            let title = TITLES.contains(&word.as_str()) || word.chars().count() == 1;
            if token.ends_with(['.', '!', '?']) && !title {
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

/// A capitalised word that could be a name: not a heading in capitals, a title, a day, a
/// month, a greeting or "I".
fn name_like(token: &str) -> bool {
    let word = bare(token);
    let lower = word.to_lowercase();
    let shouting = word
        .chars()
        .filter(|c| c.is_alphabetic())
        .all(char::is_uppercase);
    word.chars().next().is_some_and(char::is_uppercase)
        && !shouting
        && lower != "i"
        && !TITLES.contains(&lower.as_str())
        && !GREETINGS.contains(&lower.as_str())
        && !DAYS_AND_MONTHS.contains(&lower.trim_end_matches('s'))
}

/// The words of a sentence a memory of it must keep: every number or code (a word with a
/// digit), and every name. A capitalised word that opens the sentence, or follows a greeting
/// ("Thanks, Sam"), counts as a name only when it is one of the `known` names or more name
/// words follow it.
pub fn key_words(sentence: &str, known: &[String]) -> Vec<String> {
    let tokens: Vec<&str> = sentence.split_whitespace().collect();
    let mut out: Vec<String> = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let word = bare(token);
        if word.is_empty() {
            continue;
        }
        let lower = word.to_lowercase();
        let digit = word.chars().any(|c| c.is_ascii_digit());
        let name = name_like(token) && {
            let after_greeting =
                i > 0 && GREETINGS.contains(&bare(tokens[i - 1]).to_lowercase().as_str());
            let addressed = i == 0 || after_greeting;
            !addressed
                || known.iter().any(|k| k.eq_ignore_ascii_case(word))
                || tokens.get(i + 1).is_some_and(|next| name_like(next))
        };
        if (digit || name) && !out.contains(&lower) {
            out.push(lower);
        }
    }
    out
}

/// The sentences of `text` with a number, code or name that nothing in `said` (what the
/// memories quote and say) holds. `known` are names the person's memories already use.
pub fn uncovered(text: &str, said: &[String], known: &[String]) -> Vec<String> {
    let words = normalise(&said.join(" | "));
    sentences(text)
        .into_iter()
        .filter(|s| key_words(s, known).iter().any(|k| !holds(&words, k)))
        .collect()
}

/// Who a sentence names by what they are to the person: (role, name) for "my son Leo", "the
/// plumber, Mister Hughes", "the engineer is called Marek", "Mrs Carver, our neighbour" and
/// "Leo is my son".
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
/// `told` is what each memory tells (subject, sentence and object, never the quote: the quote
/// is not what an answer is given).
pub fn lost_roles(text: &str, told: &[String]) -> Vec<String> {
    let told: Vec<String> = told.iter().map(|m| normalise(m)).collect();
    sentences(text)
        .into_iter()
        .filter(|s| {
            roles(s).iter().any(|(role, name)| {
                let about: Vec<&String> = told.iter().filter(|m| holds(m, name)).collect();
                !about.is_empty() && !about.iter().any(|m| holds(m, role))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> Vec<String> {
        Vec::new()
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
            key_words(
                "The engineer is called Marek, booked for Monday 30 November at 8:00.",
                &none()
            ),
            ["marek", "30", "8:00"]
        );
        assert_eq!(key_words("PERMIT NUMBER 4471", &none()), ["4471"]);
        assert_eq!(
            key_words("Marek is the plumber Mr Hughes's friend.", &none()),
            ["hughes"]
        );
        assert!(key_words("Reminder to myself.", &none()).is_empty());
    }

    #[test]
    fn an_opening_word_or_a_greeting_is_a_name_only_when_known_or_followed_by_one() {
        assert!(key_words("Thanks, Sam, see you soon.", &none()).is_empty());
        assert!(key_words("Hi Priya", &none()).is_empty());
        assert!(key_words("Sam is coming.", &none()).is_empty());
        assert_eq!(key_words("Sam is coming.", &["sam".into()]), ["sam"]);
        assert_eq!(key_words("Thanks, Sam", &["Sam".into()]), ["sam"]);
        assert_eq!(
            key_words("Marek Nowak is coming.", &none()),
            ["marek", "nowak"]
        );
        assert_eq!(key_words("Hello Sam Jones", &none()), ["sam", "jones"]);
    }

    #[test]
    fn a_line_whose_name_no_memory_keeps_is_uncovered() {
        let text = "Boiler service is booked for Monday 30 November, between 8 and 12. The engineer is called Marek.\nThanks, Sam";
        let said =
            ["Boiler service is booked for Monday 30 November, between 8 and 12".to_string()];
        assert_eq!(
            uncovered(text, &said, &none()),
            ["The engineer is called Marek."]
        );
        let all = [said[0].clone(), "Marek is the engineer.".to_string()];
        assert!(uncovered(text, &all, &none()).is_empty());
        let times = ["Parents' evening is at 18:00.".to_string()];
        assert!(uncovered("Parents' evening at 18:00.", &times, &none()).is_empty());
    }

    #[test]
    fn roles_are_paired_with_the_names_they_belong_to() {
        let pair = |r: &str, n: &str| vec![(r.to_string(), n.to_string())];
        assert_eq!(
            roles("Parents' evening for my son Leo is on Thursday."),
            pair("son", "Leo")
        );
        assert_eq!(
            roles("The plumber, Mister Hughes, is coming on Friday."),
            pair("plumber", "Hughes")
        );
        assert_eq!(
            roles("The engineer is called Marek."),
            pair("engineer", "Marek")
        );
        assert_eq!(
            roles("Mrs Carver, our neighbour, called."),
            pair("neighbour", "Carver")
        );
        assert_eq!(roles("Leo is my son."), pair("son", "Leo"));
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
}
