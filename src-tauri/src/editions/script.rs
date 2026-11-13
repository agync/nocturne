//! What the spoken page says, in the order a person wants to hear it.
//!
//! Built from the audio edition of the page, which already has every line about a private
//! memory removed. Forgotten lines are skipped, and the interface's emphasis marks are dropped.

use nocturne_contract::{LineKind, PageLine, PageView, SpokenKind};

/// How many lines of "what changed" and "coming up" are read.
const TOP: usize = 6;
/// How many let-go items are named before "and N more".
const NAMED_LET_GO: usize = 2;

/// One part of the script: spoken on its own, so the player can skip between parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub kind: SpokenKind,
    /// Everything the section says.
    pub text: String,
    /// The same words in the pieces the voice is asked for one at a time, a line each, so no
    /// request is longer than a line.
    pub parts: Vec<String>,
}

fn plain(text: &str) -> String {
    text.replace("**", "").trim().to_string()
}

fn live(lines: &[PageLine]) -> impl Iterator<Item = &PageLine> {
    lines.iter().filter(|l| !l.forgotten)
}

/// "X", "X or Y", "X, Y or Z" joined with `last` before the final item.
fn joined(items: &[String], last: &str) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., end] => format!("{} {last} {end}", rest.join(", ")),
    }
}

/// A line as it is read. A line that asks which of two things is right also reads its choices.
fn spoken_line(line: &PageLine) -> String {
    let text = plain(&line.text);
    let choices: Vec<String> = line
        .choices
        .iter()
        .map(|c| plain(&c.text))
        .filter(|c| !c.is_empty())
        .collect();
    if line.kind == LineKind::Which && choices.len() >= 2 {
        let question = format!("Was it {}?", joined(&choices, "or"));
        let question = match choices.as_slice() {
            [a, b] => format!("Was it {a}, or {b}?"),
            _ => question,
        };
        if text.is_empty() {
            question
        } else {
            format!("{text} {question}")
        }
    } else {
        text
    }
}

fn section(kind: SpokenKind, lines: impl IntoIterator<Item = String>) -> Option<Section> {
    let parts: Vec<String> = lines.into_iter().filter(|l| !l.trim().is_empty()).collect();
    let text = parts.join(" ");
    (!text.is_empty()).then_some(Section { kind, text, parts })
}

/// The script for a page: the opening, what needs the person, what changed (the top few),
/// what is coming up, what was let go (one sentence), and the question.
pub fn audio_script(page: &PageView) -> Vec<Section> {
    let mut out = Vec::new();
    out.extend(section(
        SpokenKind::Opening,
        [plain(&page.headline), plain(&page.dek)],
    ));
    out.extend(section(
        SpokenKind::NeedsYou,
        live(&page.needs_you).map(spoken_line),
    ));
    out.extend(section(
        SpokenKind::Changed,
        live(&page.changed).take(TOP).map(spoken_line),
    ));
    out.extend(section(
        SpokenKind::ComingUp,
        live(&page.upcoming).take(TOP).map(spoken_line),
    ));
    let named: Vec<String> = live(&page.let_go)
        .map(|l| plain(l.subject.as_deref().unwrap_or(&l.text)))
        .filter(|s| !s.is_empty())
        .collect();
    if !named.is_empty() {
        let shown = &named[..named.len().min(NAMED_LET_GO)];
        let more = named.len() - shown.len();
        let mut names = shown.to_vec();
        if more > 0 {
            names.push(format!("{more} more"));
        }
        out.extend(section(
            SpokenKind::LetGo,
            [format!("Nocturne let go of {}.", joined(&names, "and"))],
        ));
    }
    if let Some(q) = page.question.as_ref().filter(|q| q.answered.is_none()) {
        out.extend(section(
            SpokenKind::Question,
            [
                plain(&q.text),
                "You can answer it on today\u{2019}s page.".to_string(),
            ],
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocturne_contract::{
        Choice, Edition, ItemKind, PageVariant, Question, QuestionKind, Receipt, Source,
    };

    pub fn line(kind: LineKind, text: &str) -> PageLine {
        PageLine {
            id: "l".into(),
            kind,
            text: text.into(),
            memory_ids: vec![],
            receipts: vec![],
            choices: vec![],
            conflict_id: None,
            subject: None,
            done: false,
            forgotten: false,
            on: None,
            last_used: None,
        }
    }

    pub fn page() -> PageView {
        PageView {
            date: "2026-11-13".into(),
            page_number: 1,
            edition: Edition::Audio,
            variant: PageVariant::Normal,
            headline: "Good morning.".into(),
            dek: "One thing today.".into(),
            needs_you: vec![],
            changed: vec![],
            let_go: vec![],
            upcoming: vec![],
            learned: vec![],
            question: None,
            more_needs_you: 0,
            more_changed: 0,
            kept_count: 0,
            changes_count: 0,
            let_go_count: 0,
            catch_up_days: 0,
            private_skipped: 0,
            pending: 0,
            tidied_from: None,
            tidied_to: None,
            update: None,
        }
    }

    fn kinds(script: &[Section]) -> Vec<SpokenKind> {
        script.iter().map(|s| s.kind).collect()
    }

    #[test]
    fn script_follows_a5_order_and_skips_forgotten() {
        let mut p = page();
        p.needs_you = vec![line(LineKind::Due, "Dentist at **10:00**."), {
            let mut l = line(LineKind::Due, "Gone.");
            l.forgotten = true;
            l
        }];
        p.changed = vec![line(LineKind::Changed, "Rent moved to Friday.")];
        p.upcoming = vec![line(LineKind::Upcoming, "Soup on Friday.")];
        p.let_go = vec![
            line(LineKind::LetGo, "Old coupon"),
            line(LineKind::LetGo, "Gym trial"),
            line(LineKind::LetGo, "Bus pass"),
            line(LineKind::LetGo, "Parking"),
        ];
        p.question = Some(Question {
            kind: QuestionKind::YesNo,
            text: "Do you still go to the gym?".into(),
            memory_id: None,
            receipt: None,
            last_asked: None,
            answered: None,
        });
        let script = audio_script(&p);
        assert_eq!(
            kinds(&script),
            [
                SpokenKind::Opening,
                SpokenKind::NeedsYou,
                SpokenKind::Changed,
                SpokenKind::ComingUp,
                SpokenKind::LetGo,
                SpokenKind::Question
            ]
        );
        assert_eq!(script[0].text, "Good morning. One thing today.");
        assert_eq!(script[1].text, "Dentist at 10:00.");
        assert_eq!(
            script[4].text,
            "Nocturne let go of Old coupon, Gym trial and 2 more."
        );
        assert_eq!(
            script[5].text,
            "Do you still go to the gym? You can answer it on today\u{2019}s page."
        );
        assert_eq!(script[0].parts, ["Good morning.", "One thing today."]);
        assert!(
            script.iter().all(|s| s.parts.join(" ") == s.text),
            "the pieces say what the section says, in order"
        );
        let all: String = script.iter().map(|s| s.text.as_str()).collect();
        assert!(!all.contains("Gone."));
        assert!(!all.contains("**"));
    }

    #[test]
    fn empty_sections_are_left_out_and_an_answered_question_is_not_read() {
        let mut p = page();
        p.question = Some(Question {
            kind: QuestionKind::YesNo,
            text: "Q?".into(),
            memory_id: None,
            receipt: None,
            last_asked: None,
            answered: Some(nocturne_contract::QuestionChoice::Yes),
        });
        assert_eq!(kinds(&audio_script(&p)), [SpokenKind::Opening]);
        p.let_go = vec![line(LineKind::LetGo, "One thing")];
        let script = audio_script(&p);
        assert_eq!(script[1].text, "Nocturne let go of One thing.");
        p.let_go.push(line(LineKind::LetGo, "Another"));
        assert_eq!(
            audio_script(&p)[1].text,
            "Nocturne let go of One thing and Another."
        );
    }

    #[test]
    fn which_line_speaks_choices() {
        let mut p = page();
        let mut which = line(LineKind::Which, "Two dates for the dentist.");
        let choice = |text: &str| Choice {
            memory_id: "m".into(),
            text: text.into(),
            receipt: Receipt {
                item_id: "i".into(),
                kind: ItemKind::Text,
                source: Source::Paste,
                label: "t".into(),
                kept_at: "2026-11-01T10:00:00Z".into(),
                duration_ms: None,
                span: None,
                unconfirmed: false,
                from_device: None,
            },
        };
        which.choices = vec![choice("Thursday at **10:00**"), choice("Friday at 9:00")];
        p.needs_you = vec![which];
        assert_eq!(
            audio_script(&p)[1].text,
            "Two dates for the dentist. Was it Thursday at 10:00, or Friday at 9:00?"
        );
    }

    #[test]
    fn what_changed_and_coming_up_are_capped() {
        let mut p = page();
        p.changed = (0..9)
            .map(|i| line(LineKind::Changed, &format!("Change {i}.")))
            .collect();
        let script = audio_script(&p);
        assert!(script[1].text.contains("Change 5.") && !script[1].text.contains("Change 6."));
    }

    #[test]
    fn a_heavy_page_stays_about_two_minutes_of_speech() {
        let sentence = "Pick up the parcel from the neighbour before Saturday afternoon.";
        let mut p = page();
        p.needs_you = (0..6).map(|_| line(LineKind::Due, sentence)).collect();
        p.changed = (0..9).map(|_| line(LineKind::Changed, sentence)).collect();
        p.upcoming = (0..9).map(|_| line(LineKind::Upcoming, sentence)).collect();
        p.let_go = (0..9)
            .map(|i| line(LineKind::LetGo, &format!("Item {i}")))
            .collect();
        let words: usize = audio_script(&p)
            .iter()
            .map(|s| s.text.split_whitespace().count())
            .sum();
        assert!(words < 450, "{words} words");
    }
}
