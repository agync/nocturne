//! The decision check: whether a decider, asked the typed questions Nocturne logs in shadow,
//! answers as well as today's rules and word lists without making the mistakes that lose data.
//! The set, the arms and the scoring are the ones in `docs/design/evaluation.md` ("The decision
//! check"). The scripted run (`tests/decisions.rs`) tests the wiring and the scoring on scripted
//! deciders and says nothing about a size or a model; the real run asks the mind and a decision
//! model. Everything seeded here is made up, and each label is a single person's reading.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use jiff::civil::Date;
use nocturne_contract::{Edition, EntityKind, Source, Span};
use nocturne_engine::decide::{
    Answer, Backend, Decider, Decision, Kind, Pending, threshold, verdict,
};
use nocturne_engine::{Candidate, Engine, ManualClock, Predicate};
use serde_json::{Map, Value, json};

/// A kept appointment, then a new note about the same slot: what the note does to it.
pub struct MovedCase {
    pub id: &'static str,
    pub lang: &'static str,
    pub subject: &'static str,
    pub first: &'static str,
    pub first_object: &'static str,
    pub first_due: &'static str,
    pub new: &'static str,
    pub new_object: &'static str,
    pub new_due: &'static str,
    /// `moved`, `cancelled`, `same` or `unrelated`.
    pub label: &'static str,
}

/// A note kept, and whether it holds a health, money or ID detail.
pub struct SensitiveCase {
    pub id: &'static str,
    pub lang: &'static str,
    pub text: &'static str,
    pub label: bool,
}

/// A planned thing the person wrote down for the day, a later note that names it, and whether
/// the later note says how it went.
pub struct FollowedUpCase {
    pub id: &'static str,
    pub lang: &'static str,
    pub subject: &'static str,
    pub plan: &'static str,
    pub later: &'static str,
    pub label: bool,
}

const fn moved(
    id: &'static str,
    lang: &'static str,
    subject: &'static str,
    (first, first_object, first_due): (&'static str, &'static str, &'static str),
    (new, new_object, new_due): (&'static str, &'static str, &'static str),
    label: &'static str,
) -> MovedCase {
    MovedCase {
        id,
        lang,
        subject,
        first,
        first_object,
        first_due,
        new,
        new_object,
        new_due,
        label,
    }
}

pub const MOVED: &[MovedCase] = &[
    moved(
        "mv01",
        "en",
        "Dentist appointment",
        (
            "Dentist appointment on Wednesday 18 November at 10:00.",
            "Wednesday 18 November at 10:00",
            "2026-11-18",
        ),
        (
            "The dentist appointment has moved. It is now Thursday 19 November at 14:30.",
            "Thursday 19 November at 14:30",
            "2026-11-19",
        ),
        "moved",
    ),
    moved(
        "mv02",
        "en",
        "Physio",
        (
            "Physio on Friday 20 November at 09:00.",
            "Friday 20 November at 09:00",
            "2026-11-20",
        ),
        (
            "Physio rescheduled to Monday 23 November at 09:00.",
            "Monday 23 November at 09:00",
            "2026-11-23",
        ),
        "moved",
    ),
    moved(
        "mv03",
        "en",
        "Hair appointment",
        (
            "Hair appointment on Saturday 21 November at 11:00.",
            "Saturday 21 November at 11:00",
            "2026-11-21",
        ),
        (
            "Sam cannot do Saturday, so the hair appointment is Sunday 22 November at 11:00 instead.",
            "Sunday 22 November at 11:00",
            "2026-11-22",
        ),
        "moved",
    ),
    moved(
        "mv04",
        "en",
        "Eye test",
        (
            "Eye test on Monday 16 November at 15:00.",
            "Monday 16 November at 15:00",
            "2026-11-16",
        ),
        (
            "The optician rang. The eye test on Monday is off, they will rebook in December.",
            "cancelled, rebook in December",
            "2026-11-16",
        ),
        "cancelled",
    ),
    moved(
        "mv05",
        "en",
        "Yoga class",
        (
            "Yoga class on Tuesday 17 November at 18:30.",
            "Tuesday 17 November at 18:30",
            "2026-11-17",
        ),
        (
            "No yoga on Tuesday, the teacher is ill, back on 24 November.",
            "cancelled, back on 24 November",
            "2026-11-17",
        ),
        "cancelled",
    ),
    moved(
        "mv06",
        "en",
        "Car service",
        (
            "Car service on Thursday 19 November at 08:00.",
            "Thursday 19 November at 08:00",
            "2026-11-19",
        ),
        (
            "The garage confirmed the car service for the 19th at 8am.",
            "the 19th at 8am",
            "2026-12-19",
        ),
        "same",
    ),
    moved(
        "mv07",
        "en",
        "Dentist check-up",
        (
            "Dentist check-up on Friday 27 November at 10:00.",
            "Friday 27 November at 10:00",
            "2026-11-27",
        ),
        (
            "The dentist moved to the High Street. The check-up is still Friday the 27th at 10.",
            "Friday the 27th at 10:00, High Street",
            "2026-12-27",
        ),
        "same",
    ),
    moved(
        "mv08",
        "en",
        "Dentist visit",
        (
            "My dentist visit is Wednesday 25 November at 15:00.",
            "Wednesday 25 November at 15:00",
            "2026-11-25",
        ),
        (
            "Dad has a dentist visit on Thursday 26 November at 09:00.",
            "Thursday 26 November at 09:00",
            "2026-11-26",
        ),
        "unrelated",
    ),
    moved(
        "mv09",
        "en",
        "Dentist trip",
        (
            "Dentist trip with the kids on Friday 4 December at 16:00.",
            "Friday 4 December at 16:00",
            "2026-12-04",
        ),
        (
            "Priya's dentist has moved to Thursday 3 December at 12:00, she told me today.",
            "Thursday 3 December at 12:00",
            "2026-12-03",
        ),
        "unrelated",
    ),
    moved(
        "mv10",
        "de",
        "Zahnarzt",
        (
            "Zahnarzt am Mittwoch, 18. November um 10:00.",
            "Mittwoch, 18. November um 10:00",
            "2026-11-18",
        ),
        (
            "Der Zahnarzt hat den Termin verschoben, jetzt Donnerstag, 19. November um 14:30.",
            "Donnerstag, 19. November um 14:30",
            "2026-11-19",
        ),
        "moved",
    ),
    moved(
        "mv11",
        "es",
        "Peluquería",
        (
            "Peluquería el viernes 20 de noviembre a las 17:00.",
            "viernes 20 de noviembre a las 17:00",
            "2026-11-20",
        ),
        (
            "La peluquería me pasó la cita al sábado 21 de noviembre a las 12:00.",
            "sábado 21 de noviembre a las 12:00",
            "2026-11-21",
        ),
        "moved",
    ),
    moved(
        "mv12",
        "fr",
        "Réunion du club",
        (
            "Réunion du club jeudi 26 novembre à 19:00.",
            "jeudi 26 novembre à 19:00",
            "2026-11-26",
        ),
        (
            "La réunion de jeudi est annulée, la prochaine est le 10 décembre.",
            "annulée, prochaine le 10 décembre",
            "2026-11-26",
        ),
        "cancelled",
    ),
    moved(
        "mv13",
        "nl",
        "Kapper",
        (
            "Kapper donderdag 19 november om 16:00.",
            "donderdag 19 november om 16:00",
            "2026-11-19",
        ),
        (
            "De kapper heeft mijn afspraak verzet naar vrijdag 20 november om 16:00.",
            "vrijdag 20 november om 16:00",
            "2026-11-20",
        ),
        "moved",
    ),
    moved(
        "mv14",
        "nl",
        "Fietsenmaker",
        (
            "Fietsenmaker maandag 23 november om 10:00.",
            "maandag 23 november om 10:00",
            "2026-11-23",
        ),
        (
            "De fietsenmaker heeft bevestigd: maandag de 23e om 10:00.",
            "maandag de 23e om 10:00",
            "2026-12-23",
        ),
        "same",
    ),
    moved(
        "mv15",
        "en",
        "Parents evening",
        (
            "Parents evening on Thursday 3 December at 18:00.",
            "Thursday 3 December at 18:00",
            "2026-12-03",
        ),
        (
            "Reminder from school: parents evening is Thursday the 3rd at 18:00.",
            "Thursday the 3rd at 18:00",
            "2027-01-03",
        ),
        "same",
    ),
    moved(
        "mv16",
        "en",
        "Vet",
        (
            "Vet on Monday 30 November at 14:00.",
            "Monday 30 November at 14:00",
            "2026-11-30",
        ),
        (
            "The vet called and brought us forward to Friday 27 November at 14:00.",
            "Friday 27 November at 14:00",
            "2026-11-27",
        ),
        "moved",
    ),
    moved(
        "mv17",
        "en",
        "Book club",
        (
            "Book club on Wednesday 2 December at 19:30.",
            "Wednesday 2 December at 19:30",
            "2026-12-02",
        ),
        (
            "Mia's book club meets on Wednesday 9 December at 19:30.",
            "Wednesday 9 December at 19:30",
            "2026-12-09",
        ),
        "unrelated",
    ),
];

const fn sensitive(
    id: &'static str,
    lang: &'static str,
    text: &'static str,
    label: bool,
) -> SensitiveCase {
    SensitiveCase {
        id,
        lang,
        text,
        label,
    }
}

pub const SENSITIVE: &[SensitiveCase] = &[
    sensitive(
        "se01",
        "en",
        "I am seeing a counsellor on Friday because I have been feeling low.",
        true,
    ),
    sensitive(
        "se02",
        "nl",
        "Morgen heb ik een afspraak bij de huisarts voor mijn rugpijn.",
        true,
    ),
    sensitive("se03", "en", "Paid the flat deposit of £1,200 today.", true),
    sensitive("se04", "en", "Gran lives at 14 Elm Road, Leeds.", false),
    sensitive(
        "se05",
        "en",
        "The GP prescribed new tablets for my blood pressure.",
        true,
    ),
    sensitive(
        "se06",
        "de",
        "Die Ärztin hat mir ein neues Mittel gegen die Migräne verschrieben.",
        true,
    ),
    sensitive(
        "se07",
        "en",
        "My passport number is 533214987 and it runs out in March.",
        true,
    ),
    sensitive(
        "se08",
        "en",
        "Booked a table for four at the Italian place on Saturday.",
        false,
    ),
    sensitive(
        "se09",
        "en",
        "Leo's school play is on Thursday at 6pm, bring a camera.",
        false,
    ),
    sensitive(
        "se10",
        "es",
        "Mi hermana tiene diabetes y se mide el azúcar cada mañana.",
        true,
    ),
    sensitive(
        "se11",
        "fr",
        "J'ai rendez-vous chez le psychologue mardi, je dors mal en ce moment.",
        true,
    ),
    sensitive(
        "se12",
        "en",
        "The plumber's number is on the fridge, call him about the tap.",
        false,
    ),
    sensitive(
        "se13",
        "en",
        "Rent is 950 euros a month and due on the first.",
        true,
    ),
    sensitive(
        "se14",
        "en",
        "The train to Leeds leaves at 07:45 from platform 3.",
        false,
    ),
    sensitive(
        "se15",
        "en",
        "Bank sort code 20-45-17, account ending 4471.",
        true,
    ),
    sensitive(
        "se16",
        "en",
        "Watched a documentary about hospitals, it was quite good.",
        false,
    ),
    sensitive(
        "se17",
        "nl",
        "Morgen fietsen we met de kinderen naar het bos.",
        false,
    ),
    sensitive(
        "se18",
        "en",
        "Started therapy again, Tuesdays at four.",
        true,
    ),
];

const fn followed(
    id: &'static str,
    lang: &'static str,
    subject: &'static str,
    plan: &'static str,
    later: &'static str,
    label: bool,
) -> FollowedUpCase {
    FollowedUpCase {
        id,
        lang,
        subject,
        plan,
        later,
        label,
    }
}

pub const FOLLOWED_UP: &[FollowedUpCase] = &[
    followed(
        "fu01",
        "en",
        "Parents evening",
        "Parents evening at six, not looking forward to it.",
        "Parents evening went fine, the teacher was kind.",
        true,
    ),
    followed(
        "fu02",
        "en",
        "Job interview",
        "Job interview at ten, I hope I am ready.",
        "The job interview went well, they said they would call on Monday.",
        true,
    ),
    followed(
        "fu03",
        "en",
        "Piano lesson",
        "Piano lesson at five, need to practise the scales first.",
        "Bought a metronome, mostly for the piano lesson, and still cannot find the old book.",
        false,
    ),
    followed(
        "fu04",
        "en",
        "Book club",
        "Book club at seven, I have not finished the book.",
        "A leaflet came for the book club at the library, might join that one too.",
        false,
    ),
    followed(
        "fu05",
        "de",
        "Elternabend",
        "Elternabend um sechs, ich habe etwas Bammel davor.",
        "Der Elternabend war besser als gedacht, die Lehrerin war sehr nett.",
        true,
    ),
    followed(
        "fu06",
        "es",
        "Reunión con el casero",
        "Reunión con el casero a las cinco, quiero hablar de la calefacción.",
        "La reunión con el casero fue bien, arreglará la calefacción esta semana.",
        true,
    ),
    followed(
        "fu07",
        "fr",
        "Marché de Noël",
        "Marché de Noël samedi avec les enfants, je dois acheter des gants.",
        "Le marché de Noël de l'an dernier me manque, je regarde les photos.",
        false,
    ),
    followed(
        "fu08",
        "nl",
        "Schoolreisje",
        "Schoolreisje van Noor vrijdag, ik moet een lunchpakket maken.",
        "Het schoolreisje was leuk, Noor kwam moe maar blij thuis.",
        true,
    ),
    followed(
        "fu09",
        "en",
        "Long run",
        "Long run on Sunday morning, aiming for eighteen kilometres.",
        "The long run was brutal, I stopped at fourteen and walked home.",
        true,
    ),
    followed(
        "fu10",
        "en",
        "Wedding",
        "Wedding of Ana and Tom on Saturday, I am giving a short speech.",
        "Still need to iron the shirt for the wedding and find the card.",
        false,
    ),
    followed(
        "fu11",
        "en",
        "Presentation",
        "Presentation to the board at two, nervous about the plan.",
        "Presentation done. The board asked three questions and liked the plan.",
        true,
    ),
    followed(
        "fu12",
        "en",
        "Dinner with Sam",
        "Dinner with Sam at eight, booked the little Greek place.",
        "Moved dinner with Sam to Friday, she is busy tonight.",
        false,
    ),
    followed(
        "fu13",
        "en",
        "Garage visit",
        "Garage visit at nine to look at the brakes, hope it is cheap.",
        "The garage visit took an hour and the brakes needed new pads.",
        true,
    ),
    followed(
        "fu14",
        "en",
        "Open day",
        "Open day at the new school at ten, bring the form.",
        "Open day dates for the other school are on the website, will check them.",
        false,
    ),
    followed(
        "fu15",
        "en",
        "Swim test",
        "Swim test for Leo on Saturday, he is a bit scared.",
        "Swim test: Leo passed and got his badge, so proud.",
        true,
    ),
    followed(
        "fu16",
        "es",
        "Cumpleaños de Lucía",
        "Cumpleaños de Lucía el sábado, tengo que comprar un regalo.",
        "Mamá preguntó por el cumpleaños de Lucía, no sé qué decir.",
        false,
    ),
];

/// How many seeded cases a kind has.
pub fn case_count(kind: Kind) -> usize {
    match kind {
        Kind::Moved => MOVED.len(),
        Kind::Sensitive => SENSITIVE.len(),
        Kind::FollowedUp => FOLLOWED_UP.len(),
    }
}

/// A seeded case and the decision the engine logged for it, with the rule's real outcome.
#[derive(Debug, Clone)]
pub struct Seeded {
    pub id: &'static str,
    pub kind: Kind,
    pub lang: &'static str,
    /// The true answer: a label of the kind's question (`moved`, `cancelled`, `same`,
    /// `unrelated`) or `yes` and `no`.
    pub label: &'static str,
    pub pending: Pending,
}

impl Seeded {
    /// The label as a yes or no, the way a verdict is read (true: moved, a health detail,
    /// already written about).
    pub fn truth(&self) -> bool {
        label_truth(self.label)
    }
}

fn label_truth(label: &str) -> bool {
    matches!(label, "moved" | "cancelled" | "yes")
}

fn day(s: &str) -> Date {
    s.parse().expect("a date")
}

fn keep(engine: &Engine, text: &str) -> String {
    match engine.keep_text(text, Source::QuickCapture).expect("kept") {
        nocturne_contract::Kept::New { item } | nocturne_contract::Kept::Duplicate { item } => {
            item.id
        }
    }
}

fn apply(engine: &Engine, item: &str, c: Candidate) -> nocturne_engine::ApplyReport {
    let leased = engine
        .lease_item(item, Duration::from_secs(60))
        .expect("leased");
    engine
        .apply_candidates(item, &leased.lease, vec![c])
        .expect("applied")
}

fn appointment(subject: &str, object: &str, due: &str, text: &str) -> Candidate {
    let mut c = Candidate::new(subject, Predicate::Scheduled, object, text, 0.8);
    c.due = Some(day(due));
    c.span = Some(Span::Quote {
        text: text.to_string(),
    });
    c
}

/// Keeps the notes and applies the candidates of every case through the engine, so each rule
/// logs its real outcome, then queues the night's decisions. Returns each case with its row.
/// The clock ends on the morning after the last day seeded.
pub fn seed(engine: &Engine, clock: &ManualClock) -> Vec<Seeded> {
    clock.set_str("2026-11-12T07:00:00Z");
    let page = engine
        .compose_page(day("2026-11-10"), Edition::Screen)
        .expect("a page");
    engine.save_page(&page).expect("saved");

    for (i, c) in MOVED.iter().enumerate() {
        clock.set_str(&format!("2026-11-12T08:{i:02}:00Z"));
        let item = keep(engine, c.first);
        apply(
            engine,
            &item,
            appointment(c.subject, c.first_object, c.first_due, c.first),
        );
    }
    let mut by_item: HashMap<(Kind, String), (&'static str, &'static str, &'static str)> =
        HashMap::new();
    for (i, c) in MOVED.iter().enumerate() {
        clock.set_str(&format!("2026-11-12T12:{i:02}:00Z"));
        let item = keep(engine, c.new);
        apply(
            engine,
            &item,
            appointment(c.subject, c.new_object, c.new_due, c.new),
        );
        by_item.insert((Kind::Moved, item), (c.id, c.lang, c.label));
    }

    for (i, c) in SENSITIVE.iter().enumerate() {
        clock.set_str(&format!("2026-11-12T14:{i:02}:00Z"));
        let item = keep(engine, c.text);
        let mut cand = Candidate::new("Me", Predicate::Happened, c.text, c.text, 0.8);
        cand.span = Some(Span::Quote {
            text: c.text.to_string(),
        });
        let report = apply(engine, &item, cand);
        let memory = report
            .created
            .first()
            .or(report.reinforced.first())
            .expect("a memory")
            .clone();
        let label = if c.label { "yes" } else { "no" };
        by_item.insert((Kind::Sensitive, memory), (c.id, c.lang, label));
    }

    for (i, c) in FOLLOWED_UP.iter().enumerate() {
        let at = 8 * 60 + 3 * i;
        clock.set_str(&format!("2026-11-13T{:02}:{:02}:00Z", at / 60, at % 60));
        let block = engine
            .save_entry_text(
                day("2026-11-13"),
                None,
                u32::try_from(i).unwrap_or(0),
                c.plan,
            )
            .expect("an entry")
            .expect("a block")
            .item
            .id;
        engine.release_entry_holds().expect("released");
        let mut cand = Candidate::new(
            c.subject,
            Predicate::Scheduled,
            "that day",
            &format!("{} is planned.", c.subject),
            0.8,
        );
        cand.subject_kind = Some(EntityKind::Thing);
        cand.due = Some(day("2026-11-13"));
        cand.span = Some(Span::Quote {
            text: c.plan.to_string(),
        });
        apply(engine, &block, cand);
        let later = at + 1;
        clock.set_str(&format!(
            "2026-11-13T{:02}:{:02}:00Z",
            later / 60,
            later % 60
        ));
        keep(engine, c.later);
        let label = if c.label { "yes" } else { "no" };
        by_item.insert((Kind::FollowedUp, block), (c.id, c.lang, label));
    }

    clock.set_str("2026-11-14T04:00:00Z");
    engine
        .queue_decisions(
            day("2026-11-14"),
            "2026-11-11T00:00:00Z".parse().expect("a time"),
        )
        .expect("queued");

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for p in engine.pending_decisions(100_000).expect("pending") {
        let key = match p.kind {
            Kind::Sensitive => p.memory_id.clone(),
            Kind::Moved | Kind::FollowedUp => p.item_id.clone(),
        };
        let Some(key) = key else { continue };
        let Some(&(id, lang, label)) = by_item.get(&(p.kind, key)) else {
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        out.push(Seeded {
            id,
            kind: p.kind,
            lang,
            label,
            pending: p,
        });
    }
    out
}

/// One decision asked of one arm (or read from the rule).
#[derive(Debug, Clone, PartialEq)]
pub struct CaseResult {
    pub id: String,
    /// The kind (`moved`, `sensitive`, `followed_up`) or a guard question.
    pub group: &'static str,
    /// The true answer as yes or no.
    pub label: bool,
    /// The verdict that would lose something if it were wrong.
    pub losing: bool,
    /// None for an error, a timeout, or an answer below the threshold.
    pub verdict: Option<bool>,
    pub error: Option<String>,
    pub ms: u64,
}

/// What the rules said, as an arm. It takes no time and never fails.
pub fn rule_arm(seeded: &[Seeded]) -> Vec<CaseResult> {
    seeded
        .iter()
        .map(|s| CaseResult {
            id: s.id.to_string(),
            group: s.kind.as_str(),
            label: s.truth(),
            losing: s.kind.losing_verdict(),
            verdict: Some(s.pending.rule),
            error: None,
            ms: 0,
        })
        .collect()
}

async fn ask(
    decider: &dyn Decider,
    decision: &Decision,
    timeout: Duration,
) -> (Result<Answer, String>, u64) {
    let start = Instant::now();
    let answer = match tokio::time::timeout(timeout, decider.decide(decision)).await {
        Ok(Ok(answer)) => Ok(answer),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err(format!("timed out after {} ms", timeout.as_millis())),
    };
    (
        answer,
        u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
    )
}

/// Asks `decider` each seeded decision once and turns its answer into a verdict. An error or a
/// timeout is an error with no verdict: in the product the rule's outcome then stands.
pub async fn run_arm(
    decider: &dyn Decider,
    seeded: &[Seeded],
    timeout: Duration,
) -> Vec<CaseResult> {
    let mut out = Vec::new();
    for s in seeded {
        let (answer, ms) = ask(decider, &s.pending.decision, timeout).await;
        let (verdict, error) = match answer {
            Ok(a) => (verdict(s.kind, decider.backend(), &a), None),
            Err(e) => (None, Some(e)),
        };
        out.push(CaseResult {
            id: s.id.to_string(),
            group: s.kind.as_str(),
            label: s.truth(),
            losing: s.kind.losing_verdict(),
            verdict,
            error,
            ms,
        });
    }
    out
}

/// Numbers for one group of one arm.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Score {
    pub n: usize,
    pub correct: usize,
    pub accuracy: f64,
    /// The verdict was the losing one and the label was not.
    pub losing_errors: usize,
    /// Losing errors over the cases whose label is not the losing verdict.
    pub losing_error_rate: f64,
    /// Answered, but below the threshold on both sides.
    pub band: usize,
    pub errors: usize,
    pub p50_ms: u64,
    pub p95_ms: u64,
}

impl Score {
    pub fn json(&self) -> Value {
        json!({
            "n": self.n,
            "correct": self.correct,
            "accuracy": self.accuracy,
            "losing_errors": self.losing_errors,
            "losing_error_rate": self.losing_error_rate,
            "band": self.band,
            "errors": self.errors,
            "p50_ms": self.p50_ms,
            "p95_ms": self.p95_ms,
        })
    }
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (p * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

fn score(results: &[&CaseResult]) -> Score {
    let n = results.len();
    let correct = results
        .iter()
        .filter(|r| r.verdict == Some(r.label))
        .count();
    let losing_errors = results
        .iter()
        .filter(|r| r.verdict == Some(r.losing) && r.label != r.losing)
        .count();
    let safe = results.iter().filter(|r| r.label != r.losing).count();
    let mut times: Vec<u64> = results
        .iter()
        .filter(|r| r.error.is_none())
        .map(|r| r.ms)
        .collect();
    times.sort_unstable();
    let ratio = |a: usize, b: usize| if b == 0 { 0.0 } else { a as f64 / b as f64 };
    Score {
        n,
        correct,
        accuracy: ratio(correct, n),
        losing_errors,
        losing_error_rate: ratio(losing_errors, safe),
        band: results
            .iter()
            .filter(|r| r.error.is_none() && r.verdict.is_none())
            .count(),
        errors: results.iter().filter(|r| r.error.is_some()).count(),
        p50_ms: percentile(&times, 0.50),
        p95_ms: percentile(&times, 0.95),
    }
}

/// One arm's scores, a group at a time (each kind, or each guard question).
#[derive(Debug, Clone)]
pub struct ArmReport {
    pub arm: String,
    pub groups: Vec<(&'static str, Score)>,
}

impl ArmReport {
    pub fn kind(&self, group: &str) -> Score {
        self.groups
            .iter()
            .find(|(g, _)| *g == group)
            .map(|(_, s)| s.clone())
            .unwrap_or_default()
    }

    pub fn json(&self) -> Value {
        let mut kinds = Map::new();
        for (group, s) in &self.groups {
            kinds.insert((*group).to_string(), s.json());
        }
        json!({ "arm": self.arm, "kinds": kinds })
    }
}

/// Scores an arm's results, one group at a time, in the order the groups first appear.
pub fn score_arm(arm: &str, results: &[CaseResult]) -> ArmReport {
    let mut order: Vec<&'static str> = Vec::new();
    for r in results {
        if !order.contains(&r.group) {
            order.push(r.group);
        }
    }
    ArmReport {
        arm: arm.to_string(),
        groups: order
            .into_iter()
            .map(|g| {
                let of: Vec<&CaseResult> = results.iter().filter(|r| r.group == g).collect();
                (g, score(&of))
            })
            .collect(),
    }
}

/// The research's bar for promoting a decider over a baseline on one kind: accuracy at least five
/// points higher and a losing-error rate no higher.
pub fn beats(arm: &Score, baseline: &Score) -> bool {
    arm.accuracy - baseline.accuracy >= 0.05 - 1e-9
        && arm.losing_error_rate <= baseline.losing_error_rate + 1e-9
}

/// Every arm as one JSON value, for `decisions.json`.
pub fn report_json(arms: &[ArmReport]) -> Value {
    json!({ "arms": arms.iter().map(ArmReport::json).collect::<Vec<_>>() })
}

/// A short table of every arm and group.
pub fn markdown(arms: &[ArmReport]) -> String {
    let mut out = String::from(
        "| arm | group | n | accuracy | losing errors | losing rate | band | errors | p50 ms | p95 ms |\n\
         |---|---|---|---|---|---|---|---|---|---|\n",
    );
    for arm in arms {
        for (group, s) in &arm.groups {
            out.push_str(&format!(
                "| {} | {} | {} | {:.0}% | {} | {:.0}% | {} | {} | {} | {} |\n",
                arm.arm,
                group,
                s.n,
                s.accuracy * 100.0,
                s.losing_errors,
                s.losing_error_rate * 100.0,
                s.band,
                s.errors,
                s.p50_ms,
                s.p95_ms
            ));
        }
    }
    out
}

/// A yes or no question about a sentence of a reflection, asked in the evaluation only. The
/// reflection gate is not a shadow kind and nothing here is logged in the product.
pub struct GuardCase {
    pub id: String,
    /// `advice` or `feeling`.
    pub group: &'static str,
    /// True when the answer is yes: the sentence is one the gate should stop.
    pub label: bool,
    pub decision: Decision,
}

const ADVICE_ASK: &str = "Does this sentence tell the person what to do or how to feel?";
const FEELING_ASK: &str = "Does this sentence name a feeling the day's items do not mention?";

/// A hand-written sentence for a seeded day (by index into the journal check's days), and
/// whether it gives advice and whether it names a feeling nothing supports.
const BAD: &[(usize, &str, bool, bool)] = &[
    (0, "You were proud of how you handled the day.", false, true),
    (
        0,
        "Remember to book a new time with the dentist.",
        true,
        false,
    ),
    (
        0,
        "Don't be so hard on yourself about feeling tired.",
        true,
        false,
    ),
    (1, "You should call Gran more often.", true, false),
    (1, "Try to rest more tomorrow.", true, false),
    (
        1,
        "Make sure you do not overdo it with the soup.",
        true,
        false,
    ),
    (
        1,
        "It might help to write the recipe down tonight.",
        true,
        false,
    ),
    (1, "You seemed anxious on the early train.", false, true),
    (1, "You were lonely on the way home.", false, true),
    (1, "You felt grateful that day.", false, false),
    (2, "You were frustrated that nothing happened.", false, true),
    (2, "Why not do something fun tomorrow?", true, false),
];

/// Both questions for each sentence of the journal check's seeded days (all fine) and for the
/// hand-written bad ones.
pub fn guard_cases() -> Vec<GuardCase> {
    use super::journal::DAYS;
    let mut sentences: Vec<(usize, String, bool, bool)> = Vec::new();
    for (d, day) in DAYS.iter().enumerate() {
        for (text, _) in day.script {
            sentences.push((d, text.replace("**", ""), false, false));
        }
    }
    for (d, text, advice, feeling) in BAD {
        sentences.push((*d, (*text).to_string(), *advice, *feeling));
    }
    let mut out = Vec::new();
    for (n, (d, sentence, advice, feeling)) in sentences.iter().enumerate() {
        let mut state = String::from("The day's items:\n");
        for k in DAYS[*d].kept {
            state.push_str(&format!("- {}\n", k.text));
        }
        for c in DAYS[*d].checkins {
            state.push_str(&format!("- Check-in: {}\n", c.words.join(", ")));
        }
        state.push_str(&format!("Sentence: {sentence}"));
        for (group, ask, label) in [
            ("advice", ADVICE_ASK, *advice),
            ("feeling", FEELING_ASK, *feeling),
        ] {
            out.push(GuardCase {
                id: format!("g{:02}-{group}", n + 1),
                group,
                label,
                decision: Decision {
                    state: state.clone(),
                    question: nocturne_engine::decide::Question {
                        instructions: ask.into(),
                        ask: nocturne_engine::decide::Ask::YesNo,
                    },
                },
            });
        }
    }
    out
}

/// Reads a guard answer: yes or no when either side reaches the page question's threshold for
/// this backend.
fn guard_verdict(backend: Backend, answer: &Answer) -> Option<bool> {
    let t = threshold(Kind::FollowedUp, backend);
    if answer.p("yes") >= t {
        Some(true)
    } else if answer.p("no") >= t {
        Some(false)
    } else {
        None
    }
}

/// Asks `decider` each guard question once. A bad sentence answered no is the loss.
pub async fn run_guard_arm(
    decider: &dyn Decider,
    cases: &[GuardCase],
    timeout: Duration,
) -> Vec<CaseResult> {
    let mut out = Vec::new();
    for c in cases {
        let (answer, ms) = ask(decider, &c.decision, timeout).await;
        let (verdict, error) = match answer {
            Ok(a) => (guard_verdict(decider.backend(), &a), None),
            Err(e) => (None, Some(e)),
        };
        out.push(CaseResult {
            id: c.id.clone(),
            group: c.group,
            label: c.label,
            losing: false,
            verdict,
            error,
            ms,
        });
    }
    out
}
