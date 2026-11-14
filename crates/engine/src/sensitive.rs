//! Health, money and ID details: what Online answers keeps on this computer unless the person
//! allowed it. A memory counts when any version of it carries such a label, or when its words
//! hold a card or bank number, a long number, an ID-like code, a password or PIN, or an amount
//! of money, or words about health. It fails closed: a shape that only looks like one of these counts too.

/// Labels that mark a health, money or ID detail.
const TAGS: &[&str] = &[
    "health",
    "allergy",
    "allergies",
    "medication",
    "medications",
    "medical",
    "medicine",
    "money",
    "bank",
    "banking",
    "salary",
    "finance",
    "finances",
    "financial",
    "debt",
    "debts",
    "tax",
    "taxes",
    "passport",
    "id",
    "licence",
    "license",
    "insurance",
    "pin",
    "password",
    "passwords",
];

/// Words that only come up beside a secret or an account.
const SECRET_WORDS: &[&str] = &["password", "passwords", "passcode", "iban", "cvv", "cvc"];

const SECRET_PHRASES: &[&str] = &[
    "account number",
    "bank details",
    "card number",
    "national insurance",
    "passport number",
    "security code",
    "social security",
    "sort code",
];

/// Words that tie an amount to pay or to what is owed.
const PAY_OR_DEBT: &[&str] = &[
    "salary",
    "salaries",
    "wage",
    "wages",
    "pay",
    "paid",
    "payslip",
    "earn",
    "earns",
    "earned",
    "earning",
    "earnings",
    "income",
    "bonus",
    "pension",
    "debt",
    "debts",
    "owe",
    "owes",
    "owed",
    "owing",
    "loan",
    "loans",
    "mortgage",
    "overdraft",
    "arrears",
    "borrowed",
    "lent",
    "credit",
    "balance",
    "savings",
];

/// Words that come up when a line is about a person's health. A plain list: a false match
/// only keeps a line here.
const HEALTH_WORDS: &[&str] = &[
    "diagnosis",
    "diagnoses",
    "diagnosed",
    "diagnose",
    "illness",
    "illnesses",
    "depression",
    "depressed",
    "anxiety",
    "cancer",
    "diabetes",
    "diabetic",
    "asthma",
    "adhd",
    "autism",
    "autistic",
    "hiv",
    "pregnancy",
    "pregnant",
    "miscarriage",
    "therapy",
    "therapist",
    "psychiatrist",
    "psychologist",
    "surgery",
    "operation",
    "hospital",
    "prescription",
    "prescribed",
    "medication",
    "medications",
    "medicine",
    "medicines",
    "dose",
    "doses",
    "dosage",
    "symptom",
    "symptoms",
    "gp",
    "rehab",
    "addiction",
    "addicted",
];

const HEALTH_PHRASES: &[&str] = &[
    "doctor appointment",
    "doctor's appointment",
    "doctors appointment",
    "mental health",
];

const CURRENCY_SIGNS: &[char] = &['£', '$', '€', '¥', '₹'];

const CURRENCY_WORDS: &[&str] = &[
    "gbp", "usd", "eur", "pounds", "pound", "dollars", "dollar", "euros", "euro", "quid", "grand",
];

/// Whether a label marks a health, money or ID detail.
pub(crate) fn tag(label: &str) -> bool {
    let label = label.trim().to_lowercase();
    TAGS.contains(&label.as_str())
}

/// Whether some text holds a money or ID detail.
pub(crate) fn text(s: &str) -> bool {
    let lower = s.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let has_digit = s.chars().any(|c| c.is_ascii_digit());
    if words.iter().any(|w| HEALTH_WORDS.contains(w))
        || HEALTH_PHRASES.iter().any(|p| lower.contains(p))
    {
        return true;
    }
    if words.iter().any(|w| SECRET_WORDS.contains(w))
        || SECRET_PHRASES.iter().any(|p| lower.contains(p))
        || (has_digit && words.contains(&"pin"))
    {
        return true;
    }
    if !has_digit {
        return false;
    }
    number_runs(s) || id_like(s) || iban(s) || amount(&lower, &words)
}

/// Runs of digits, alone or in groups split by single spaces, hyphens or dots: a card number
/// that passes the Luhn check, any run whose digits total 9 or more, a sort code (`12-34-56`)
/// or an account number (8 digits alone or in two groups of 4).
fn number_runs(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let glued_before = i > 0 && chars[i - 1].is_alphabetic();
        let mut chunks: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut j = i;
        while j < chars.len() {
            let c = chars[j];
            if c.is_ascii_digit() {
                cur.push(c);
                j += 1;
            } else if matches!(c, ' ' | '-' | '.')
                && !cur.is_empty()
                && chars.get(j + 1).is_some_and(char::is_ascii_digit)
            {
                chunks.push(std::mem::take(&mut cur));
                j += 1;
            } else {
                break;
            }
        }
        chunks.push(cur);
        let glued_after = chars.get(j).is_some_and(|c| c.is_alphabetic());
        let digits: String = chunks.concat();
        if digits.len() >= 9 {
            return true;
        }
        if (13..=19).contains(&digits.len()) && luhn(&digits) {
            return true;
        }
        let lens: Vec<usize> = chunks.iter().map(String::len).collect();
        if lens == [2, 2, 2] {
            return true;
        }
        if (lens == [8] || lens == [4, 4]) && !glued_before && !glued_after {
            return true;
        }
        i = j.max(i + 1);
    }
    false
}

/// The Luhn check card numbers carry.
fn luhn(digits: &str) -> bool {
    let mut sum = 0;
    for (n, c) in digits.chars().rev().enumerate() {
        let Some(mut d) = c.to_digit(10) else {
            return false;
        };
        if n % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
    }
    sum % 10 == 0
}

/// A passport, licence or ID-like code: letters and digits in one word of 6 to 20, at least
/// five of them digits.
fn id_like(s: &str) -> bool {
    s.split(|c: char| !c.is_alphanumeric()).any(|w| {
        let n = w.chars().count();
        let digits = w.chars().filter(char::is_ascii_digit).count();
        let letters = w.chars().filter(|c| c.is_alphabetic()).count();
        (6..=20).contains(&n) && digits >= 5 && letters >= 1
    })
}

/// An IBAN: two letters and two digits, then letters and digits, 15 to 34 in all, written
/// whole or in groups of four.
fn iban(s: &str) -> bool {
    let words: Vec<&str> = s
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .collect();
    for (i, w) in words.iter().enumerate() {
        let c: Vec<char> = w.chars().collect();
        let starts = c.len() >= 4
            && c[0].is_ascii_alphabetic()
            && c[1].is_ascii_alphabetic()
            && c[2].is_ascii_digit()
            && c[3].is_ascii_digit()
            && c.iter().all(char::is_ascii_alphanumeric);
        if !starts {
            continue;
        }
        let mut whole = w.to_string();
        if c.len() <= 8 {
            for g in &words[i + 1..] {
                let n = g.chars().count();
                if n == 0 || n > 8 || !g.chars().all(|c| c.is_ascii_alphanumeric()) {
                    break;
                }
                whole.push_str(g);
                if n < 4 || whole.len() >= 34 {
                    break;
                }
            }
        }
        if (15..=34).contains(&whole.len()) {
            return true;
        }
    }
    false
}

/// Any amount of money: a currency sign before a number, or a number before a currency word.
/// A bare `40k` counts only beside a word about pay or debt.
fn amount(lower: &str, words: &[&str]) -> bool {
    let chars: Vec<char> = lower.chars().collect();
    let signed = chars.iter().enumerate().any(|(i, c)| {
        CURRENCY_SIGNS.contains(c)
            && chars[i + 1..]
                .iter()
                .find(|c| **c != ' ')
                .is_some_and(char::is_ascii_digit)
    });
    let worded = words.windows(2).any(|w| {
        w[0].chars().next().is_some_and(|c| c.is_ascii_digit()) && CURRENCY_WORDS.contains(&w[1])
    });
    let thousands = words.iter().any(|w| PAY_OR_DEBT.contains(w))
        && words.iter().any(|w| {
            let digits = w.trim_end_matches('k');
            digits.len() < w.len()
                && !digits.is_empty()
                && digits.chars().all(|c| c.is_ascii_digit())
        });
    signed || worded || thousands
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(f: fn(&str) -> bool, yes: &[&str], no: &[&str]) {
        for s in yes {
            assert!(f(s), "should count: {s}");
        }
        for s in no {
            assert!(!f(s), "should not count: {s}");
        }
    }

    #[test]
    fn labels() {
        for t in [
            "Health",
            "allergy",
            "medication",
            "money",
            "bank",
            "salary",
            "finance",
            "debt",
            "tax",
            "passport",
            "ID",
            "licence",
            "license",
            "insurance",
            "PIN",
            "password",
        ] {
            assert!(tag(t), "{t}");
        }
        for t in ["dentist", "family", "work", "idea", "taxi", "banker"] {
            assert!(!tag(t), "{t}");
        }
    }

    #[test]
    fn cards_pass_luhn() {
        all(
            text,
            &[
                "The card is 4111 1111 1111 1111.",
                "Card 5500-0000-0000-0004 for bills",
                "pay with 378282246310005",
                "The code is 4111 1111 1111 1112 maybe",
            ],
            &["Dates 2026-11-14 and 2027-01-02", "Rooms 12 and 34"],
        );
    }

    #[test]
    fn long_runs() {
        all(
            text,
            &[
                "Reference 123456789",
                "Call 07700900123 later",
                "NHS number 943 476 5919",
                "SSN 123-45-6789",
                "Card 4111 1111 1111 1112",
                "Card 4111.1111.1111.1111",
                "Call 07700 900 123",
                "Phone 07700 900 12",
            ],
            &[
                "Flat 12b",
                "Order 1234567 came",
                "Version 1.2.3 is out",
                "Dates 14.11.2026 and 3 4 5",
            ],
        );
    }

    #[test]
    fn sort_codes_and_accounts() {
        all(
            text,
            &[
                "Bank 20-00-00 then",
                "Account 31926819 at the bank",
                "Account 3192 6819",
            ],
            &["Score 3-2 at half time", "The year 2026 and 20-00"],
        );
    }

    #[test]
    fn id_like_codes() {
        all(
            text,
            &[
                "Passport AB1234567 renews in May",
                "Licence MORGA753116SM9IJ",
                "Number QQ123456C",
            ],
            &[
                "Take the M25 home",
                "Vitamin B12 daily",
                "Postcode SW1A 1AA",
                "Flight BA249 at noon",
            ],
        );
    }

    #[test]
    fn ibans() {
        all(
            iban,
            &[
                "Rent goes to GB82 WEST 1234 5698 7654 32 each month",
                "Pay GB82WEST 12345698 765432 please",
                "IBAN-free text DE89370400440532013000.",
            ],
            &[
                "Seat AB12 CD34",
                "GB82 is a code",
                "AB1 2345 6789 0123 4567",
            ],
        );
        assert!(text("Rent goes to GB82 WEST 1234 5698 7654 32 each month"));
    }

    #[test]
    fn pay_and_debt_amounts() {
        all(
            text,
            &[
                "Her salary is £45,000",
                "Owes Sam $200",
                "The loan is 3000 pounds",
                "Earns 40k a year",
                "Mortgage of € 1200 a month",
                "Rent is £1,200",
                "Overdrawn by £300",
                "Bought the house for £450,000",
                "Lunch was £12",
                "It cost 20 euros",
            ],
            &[
                "Ran 40k steps",
                "The salary review is in May",
                "Paid the 3 tickets",
                "Debt free since 2020",
            ],
        );
    }

    #[test]
    fn health_words() {
        all(
            text,
            &[
                "Sam was diagnosed with depression",
                "Booked a GP appointment",
                "Has a Doctor appointment on Friday",
                "Her mental health is better",
                "Asthma flares in spring",
                "Starts therapy next week",
                "Pregnant, due in May",
                "Dose is twice a day",
                "Sees the Psychiatrist",
                "In rehab since June",
            ],
            &[
                "The dentist is on Thursday.",
                "Gpu drivers updated",
                "Operational review on Monday",
                "Anxiously awaiting the parcel at the post office",
            ],
        );
    }

    #[test]
    fn secrets_and_pins() {
        all(
            text,
            &[
                "The wifi password is hunter",
                "Her PIN is 4821",
                "The sort code is on the letter",
                "Passport number in the drawer",
            ],
            &["Pin the poster up", "The dentist is on Thursday."],
        );
    }
}
