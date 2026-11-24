//! The look-up path against the stand-in provider with a scripted mind
//! (docs/design/ask-with-tools.md): the design's examples in the number of requests shown, the
//! limits, refusals learned, each part counted and the month's limit looked at before each, and
//! every part named for the Sent online list.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jiff::civil::{Date, Weekday};
use nocturne_contract::{MemoryView, ReadingKind};
use nocturne_engine::{AskToday, Fact, MindError};
use nocturne_mind::lookup::{self as look, LookUp, Place};
use nocturne_provider::address::OnlineKey;
use nocturne_provider::lookup::{self, LookUps, Part, Parts, Tools};
use nocturne_provider::mind::Asking;
use nocturne_provider::stand_in::{Answer, KEY, MODEL, StandIn};
use nocturne_provider::{Job, Provider, ProviderMind};
use serde_json::{Value, json};
use tokio::sync::mpsc;

fn d(s: &str) -> Date {
    s.parse().unwrap()
}

fn today() -> AskToday {
    AskToday {
        day: d("2026-11-24"),
        hour: 21,
        minute: 40,
        zone: "Europe/London".into(),
        week_start: Weekday::Monday,
    }
}

fn fact(id: &str, sentence: &str) -> Fact {
    Fact::from(
        serde_json::from_value::<MemoryView>(json!({
            "id": id, "sentence": sentence, "subject": "x", "entityId": null,
            "predicate": "scheduled", "object": "x", "certainty": "sure", "status": "active",
            "validFrom": null, "validTo": null, "due": null,
            "learnedAt": "2026-11-21T09:00:00+00:00", "lastUsed": null, "letGoAt": null,
            "keepForever": false, "private": false, "was": null,
            "receipts": [{"itemId": format!("item-{id}"), "kind": "calendar", "source": "phone",
                          "label": "Calendar", "keptAt": "2026-11-21T09:00:00Z",
                          "durationMs": null, "span": null, "unconfirmed": false,
                          "fromDevice": "Pixel 9"}],
        }))
        .unwrap(),
    )
}

/// Each request the mind noted: what for, and which part.
type Noted = Arc<Mutex<Vec<(Job, Option<Part>)>>>;

struct Rig {
    stand_in: StandIn,
    mind: ProviderMind,
    parts: Noted,
    looked: Arc<Mutex<Vec<LookUp>>>,
    places: Arc<Mutex<Vec<Vec<Place>>>>,
    learned: Arc<Mutex<Vec<Tools>>>,
}

async fn rig(known: Tools) -> Rig {
    let stand_in = StandIn::start().await;
    let parts: Noted = Arc::default();
    let learned: Arc<Mutex<Vec<Tools>>> = Arc::default();
    let p = parts.clone();
    let l = learned.clone();
    let mind = ProviderMind::new(
        Provider::new(&stand_in.url(), MODEL, OnlineKey::new(KEY.into())).unwrap(),
    )
    .with_record(Arc::new(move |job, ex| {
        p.lock().unwrap().push((job, ex.carried.part.clone()));
    }))
    .with_tools(known, Arc::new(move |t| l.lock().unwrap().push(t)));
    Rig {
        stand_in,
        mind,
        parts,
        looked: Arc::default(),
        places: Arc::default(),
        learned,
    }
}

impl Rig {
    /// Look-ups on a scripted store: the journal of 17 November has two lines, nothing else
    /// has any.
    fn look_ups(&self) -> LookUps {
        let looked = self.looked.clone();
        let places = self.places.clone();
        LookUps {
            kinds: vec![ReadingKind::Sleep],
            run: Arc::new(move |l| {
                looked.lock().unwrap().push(l.clone());
                match l {
                    LookUp::Journal { from, .. } if *from == d("2026-11-17") => Some((
                        vec![
                            fact("j1", "Mrs Patel said Leo's reading has come on a lot."),
                            fact("j2", "Good thing: Leo read a whole chapter on his own."),
                        ],
                        0,
                    )),
                    LookUp::Calendar { .. } => Some((vec![fact("c9", "Bins on Monday.")], 3)),
                    _ => Some((Vec::new(), 0)),
                }
            }),
            looking: Arc::new(move |p| places.lock().unwrap().push(p.to_vec())),
            limit: Duration::from_secs(90),
        }
    }

    async fn ask(
        &self,
        question: &str,
        facts: &[Fact],
        look_ups: Option<&LookUps>,
        parts: Parts<'_>,
    ) -> Result<lookup::Answered, MindError> {
        let (tx, _rx) = mpsc::channel(8);
        let today = today();
        self.mind
            .answer_asked(
                Asking {
                    ask: "q1",
                    question,
                    facts,
                    today: &today,
                    look_ups,
                    parts,
                    stop: None,
                },
                tx,
            )
            .await
    }

    fn bodies(&self) -> Vec<Value> {
        self.stand_in.chats().into_iter().map(|c| c.body).collect()
    }
}

fn user_of(body: &Value) -> String {
    body["messages"][1]["content"].as_str().unwrap().to_string()
}

fn ids(a: &lookup::Answered) -> Vec<String> {
    a.meta.cited.clone()
}

#[tokio::test]
async fn whats_on_tomorrow_is_answered_in_one_request_from_the_date_step_s_lines() {
    let r = rig(Tools::Unknown).await;
    let lookups = r.look_ups();
    r.stand_in.then(Answer::text(
        "Tomorrow, Wednesday 25 November, you have **the dentist at 10:30** at Smile Dental [m1] \
         and **Leo's swimming at 17:00** at the leisure centre [m2]. **The bins go out** on \
         Wednesday night [m3].",
    ));
    let facts = vec![
        fact(
            "c1",
            "Dentist is on Wednesday 25 November at 10:30, at Smile Dental.",
        ),
        fact(
            "c2",
            "Leo's swimming is on Wednesday 25 November at 17:00, at the leisure centre.",
        ),
        fact("n1", "Bins Weds night from now on."),
    ];
    let a = r
        .ask("What's on tomorrow?", &facts, Some(&lookups), Parts::free())
        .await
        .unwrap();
    assert_eq!(a.parts, 1, "counts 1");
    assert_eq!(ids(&a), ["c1", "c2", "n1"]);
    assert!(!a.meta.not_sure);
    let bodies = r.bodies();
    assert_eq!(bodies.len(), 1);
    assert_eq!(
        bodies[0]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["calendar", "journal", "search", "health"],
        "look-ups are offered"
    );
    let user = user_of(&bodies[0]);
    assert!(user.contains(
        "Today is Tuesday 24 November 2026, 21:40, in the time zone Europe/London. \
         The week starts on Monday.\nQuestion: What's on tomorrow?"
    ));
    assert!(
        bodies[0]["messages"][0]["content"]
            .as_str()
            .unwrap()
            .ends_with(look::SYSTEM)
    );
    assert!(r.looked.lock().unwrap().is_empty());
    // Every part is named for the Sent online list.
    let parts = r.parts.lock().unwrap().clone();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].0, Job::Answer);
    assert_eq!(parts[0].1.as_ref().unwrap().n, 1);
    assert_eq!(parts[0].1.as_ref().unwrap().ask, "q1");
}

#[tokio::test]
async fn the_mind_looks_once_and_answers_from_both_requests_lines() {
    let r = rig(Tools::Unknown).await;
    let lookups = r.look_ups();
    r.stand_in.then(Answer::look(
        "journal",
        r#"{"from":"2026-11-17","to":"2026-11-17"}"#,
    ));
    r.stand_in.then(Answer::text(
        "Parents' evening was on **Tuesday 17 November at 18:00** [m1]. That day you wrote that \
         **Mrs Patel said Leo's reading has come on a lot** [m2], and your good thing was that \
         **Leo read a whole chapter on his own** [m3].",
    ));
    let facts = vec![fact(
        "c1",
        "Parents' evening is on Tuesday 17 November at 18:00.",
    )];
    let a = r
        .ask(
            "How was Leo's parents' evening?",
            &facts,
            Some(&lookups),
            Parts::free(),
        )
        .await
        .unwrap();
    assert_eq!(a.parts, 2, "counts 2");
    assert_eq!(
        ids(&a),
        ["c1", "j1", "j2"],
        "a citation of a looked-up line stands"
    );
    assert_eq!(
        *r.looked.lock().unwrap(),
        vec![LookUp::Journal {
            from: d("2026-11-17"),
            to: d("2026-11-17")
        }]
    );
    assert_eq!(*r.places.lock().unwrap(), vec![vec![Place::Journal]]);
    assert_eq!(
        r.mind.tools(),
        Tools::Takes {
            on: "2026-11-24".into()
        },
        "a mind that looked takes look-ups"
    );
    assert_eq!(r.learned.lock().unwrap().len(), 1);
    let bodies = r.bodies();
    assert_eq!(bodies.len(), 2);
    let msgs = bodies[1]["messages"].as_array().unwrap();
    assert_eq!(msgs[2]["role"], "assistant");
    assert_eq!(msgs[3]["role"], "tool");
    let back = msgs[3]["content"].as_str().unwrap();
    assert!(
        back.contains("[m2] Mrs Patel said Leo's reading has come on a lot."),
        "{back}"
    );
    assert!(back.contains("[m3] Good thing: Leo read a whole chapter on his own."));
    assert!(
        back.starts_with("<facts-"),
        "inside the same fence as the facts"
    );
    let parts = r.parts.lock().unwrap().clone();
    let second = parts[1].1.clone().unwrap();
    assert_eq!(second.n, 2);
    assert_eq!(second.asked.len(), 1);
    assert_eq!(second.asked[0].look, "journal");
    assert_eq!(second.asked[0].from.as_deref(), Some("2026-11-17"));
    assert_eq!(second.asked[0].lines, 2);
    assert_eq!(
        second.from, 2,
        "the words new to part 2 begin after the system and question"
    );
}

#[tokio::test]
async fn a_question_with_nothing_kept_ends_after_an_empty_look_without_another_request() {
    let r = rig(Tools::Unknown).await;
    let lookups = r.look_ups();
    r.stand_in.then(Answer::look(
        "search",
        r#"{"words":"MOT garage car service"}"#,
    ));
    let a = r
        .ask("When is the car's MOT?", &[], Some(&lookups), Parts::free())
        .await
        .unwrap();
    assert!(a.nothing && a.meta.not_sure);
    assert_eq!(a.parts, 1, "counts 1");
    assert_eq!(r.bodies().len(), 1);
}

#[tokio::test]
async fn a_mind_that_always_asks_for_more_stops_at_four_requests_and_eight_look_ups() {
    let r = rig(Tools::Unknown).await;
    let lookups = r.look_ups();
    let five: Vec<(String, String)> = (0..5)
        .map(|i| {
            (
                "calendar".to_string(),
                format!(
                    r#"{{"from":"2026-12-0{}","to":"2026-12-0{}"}}"#,
                    i + 1,
                    i + 1
                ),
            )
        })
        .collect();
    r.stand_in.always(Answer::Tools(five));
    let facts = vec![fact("c1", "Something kept.")];
    let a = r
        .ask("What's on soon?", &facts, Some(&lookups), Parts::free())
        .await
        .unwrap();
    let bodies = r.bodies();
    assert_eq!(bodies.len(), 4, "at most 4 requests");
    assert_eq!(a.parts, 4);
    assert!(bodies[..3].iter().all(|b| b["tools"].is_array()));
    assert!(
        bodies[3].get("tools").is_none(),
        "the fourth offers no look-ups"
    );
    let last = bodies[3]["messages"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["content"], look::ANSWER_NOW);
    assert_eq!(r.looked.lock().unwrap().len(), 8, "at most 8 look-ups");
    let tool_lines: Vec<String> = bodies[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(tool_lines.len(), 5, "every look-up asked for is answered");
    assert_eq!(tool_lines[4], look::TOO_MANY, "at most 4 in one round");
    assert!(tool_lines[0].contains("And 3 more. Ask for fewer days to see them."));
    assert_eq!(
        tool_lines[1],
        format!("{}\n{}", look::ALREADY, look::more(3)),
        "a line already handed is not handed again, and says so"
    );
    // The answer it gives (the standing reply asks again) is not one: not sure.
    assert!(a.meta.not_sure);
}

#[tokio::test]
async fn a_provider_that_refuses_look_ups_answers_by_the_date_step_path_and_is_remembered() {
    let r = rig(Tools::Unknown).await;
    let lookups = r.look_ups();
    r.stand_in.then(Answer::Status(
        400,
        r#"{"error":"tools are not supported"}"#.into(),
    ));
    r.stand_in
        .then(Answer::text("**The dentist** is tomorrow [m1]."));
    let facts = vec![fact("c1", "Dentist is on Wednesday 25 November at 10:30.")];
    let a = r
        .ask(
            "When is the dentist?",
            &facts,
            Some(&lookups),
            Parts::free(),
        )
        .await
        .unwrap();
    assert_eq!(ids(&a), ["c1"]);
    assert_eq!(a.parts, 1, "only the request answered counts");
    let bodies = r.bodies();
    assert_eq!(bodies.len(), 2);
    assert!(bodies[1].get("tools").is_none());
    assert_eq!(
        user_of(&bodies[0]),
        user_of(&bodies[1]),
        "with the same lines"
    );
    assert_eq!(
        r.mind.tools(),
        Tools::Refuses {
            on: "2026-11-24".into()
        }
    );
    assert_eq!(r.learned.lock().unwrap().len(), 1);
    // The next question is not offered look-ups at all.
    r.stand_in.then(Answer::text("**Yes** [m1]."));
    r.ask("Again?", &facts, Some(&lookups), Parts::free())
        .await
        .unwrap();
    assert!(r.bodies()[2].get("tools").is_none());
}

#[tokio::test]
async fn look_ups_asked_for_in_a_shape_that_does_not_read_are_a_refusal() {
    let r = rig(Tools::Unknown).await;
    let lookups = r.look_ups();
    r.stand_in.then(Answer::BrokenTools);
    r.stand_in.then(Answer::text("**The dentist** [m1]."));
    let facts = vec![fact("c1", "Dentist is on Wednesday 25 November.")];
    let a = r
        .ask(
            "When is the dentist?",
            &facts,
            Some(&lookups),
            Parts::free(),
        )
        .await
        .unwrap();
    assert_eq!(ids(&a), ["c1"]);
    assert!(matches!(r.mind.tools(), Tools::Refuses { .. }));
}

#[tokio::test]
async fn two_questions_in_a_row_with_only_broken_look_ups_stop_them() {
    let r = rig(Tools::Unknown).await;
    let lookups = r.look_ups();
    let facts = vec![fact("c1", "Dentist is on Wednesday 25 November.")];
    for n in 0..2 {
        r.stand_in.then(Answer::look("remind", "{}"));
        r.stand_in.then(Answer::text("**The dentist** [m1]."));
        r.ask("When?", &facts, Some(&lookups), Parts::free())
            .await
            .unwrap();
        let known = r.mind.tools();
        if n == 0 {
            assert!(known.offered(), "one question is not enough");
        } else {
            assert!(matches!(known, Tools::Refuses { .. }), "{known:?}");
        }
    }
    let tool = r.bodies()[1]["messages"][3]["content"].clone();
    assert_eq!(tool, look::NO_SUCH);
}

#[tokio::test]
async fn an_answer_citing_nothing_is_not_sure() {
    let r = rig(Tools::Unknown).await;
    r.stand_in
        .then(Answer::text("You have the dentist tomorrow."));
    let facts = vec![fact("c1", "Dentist is on Wednesday 25 November.")];
    let a = r
        .ask("When is the dentist?", &facts, None, Parts::free())
        .await
        .unwrap();
    assert!(a.meta.not_sure);
    assert!(r.bodies()[0].get("tools").is_none(), "the date step path");
}

/// With a number limit of 3, a question in 2 parts counts 2, and the next question that would
/// need 2 more parts stops after its first.
#[tokio::test]
async fn each_part_counts_and_the_limit_is_looked_at_before_each() {
    let r = rig(Tools::Unknown).await;
    let lookups = r.look_ups();
    let count = Arc::new(AtomicU32::new(0));
    let (a, b) = (count.clone(), count.clone());
    let may_send = move || a.load(Ordering::SeqCst) < 3;
    let counted = move || {
        b.fetch_add(1, Ordering::SeqCst);
    };
    let parts = Parts {
        may_send: &may_send,
        counted: &counted,
    };
    let facts = vec![fact("c1", "Parents' evening is on Tuesday 17 November.")];
    for _ in 0..2 {
        r.stand_in.then(Answer::look(
            "journal",
            r#"{"from":"2026-11-17","to":"2026-11-17"}"#,
        ));
        r.stand_in
            .then(Answer::text("It was **Tuesday 17 November** [m1]."));
    }
    r.ask("How was it?", &facts, Some(&lookups), parts)
        .await
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
    let second = r.ask("How was it?", &facts, Some(&lookups), parts).await;
    assert!(matches!(second, Err(MindError::Unavailable)), "{second:?}");
    assert_eq!(
        count.load(Ordering::SeqCst),
        3,
        "stopped after its first part"
    );
    assert_eq!(r.bodies().len(), 3);
}

#[tokio::test]
async fn the_look_up_check_passes_a_mind_that_asks_for_the_calendar() {
    let r = rig(Tools::Unknown).await;
    let provider = r.mind.provider();
    r.stand_in.then(Answer::look(
        "calendar",
        r#"{"from":"2030-01-01","to":"2030-01-01"}"#,
    ));
    assert_eq!(lookup::check(provider).await.0, Some(true));
    r.stand_in.then(Answer::text("Nothing that I know of."));
    assert_eq!(lookup::check(provider).await.0, Some(false));
    r.stand_in.then(Answer::look(
        "calendar",
        r#"{"from":"2030-01-01","to":"2030-01-02"}"#,
    ));
    assert_eq!(
        lookup::check(provider).await.0,
        Some(false),
        "not in the expected form"
    );
    r.stand_in.then(Answer::Status(
        422,
        r#"{"error":"tools are not supported"}"#.into(),
    ));
    assert_eq!(lookup::check(provider).await.0, Some(false));
    r.stand_in.then(Answer::Status(422, "{}".into()));
    assert_eq!(
        lookup::check(provider).await.0,
        None,
        "a refusal that names nothing says nothing of look-ups"
    );
    r.stand_in.then(Answer::Status(503, "{}".into()));
    assert_eq!(lookup::check(provider).await.0, None, "nothing known yet");
    let body = &r.bodies()[0];
    assert_eq!(body["messages"][0]["content"], lookup::CHECK_QUESTION);
    assert_eq!(body["tools"][0]["function"]["name"], "calendar");
}

/// Nothing is learned from a reply that says nothing of look-ups: an empty or cut reply, a
/// refusal that does not name them, one about the length of what was sent, or a refusal naming
/// them whose retry without them was not answered either.
#[tokio::test]
async fn a_refusal_is_learned_only_when_it_names_look_ups_and_the_retry_is_answered() {
    let facts = vec![fact("c1", "Dentist is on Wednesday 25 November.")];
    // An empty reply: answered again without look-ups, nothing learned.
    let r = rig(Tools::Unknown).await;
    let lookups = r.look_ups();
    r.stand_in.then(Answer::text("   "));
    r.stand_in.then(Answer::text("**The dentist** [m1]."));
    let a = r
        .ask("When?", &facts, Some(&lookups), Parts::free())
        .await
        .unwrap();
    assert_eq!(ids(&a), ["c1"]);
    assert_eq!(r.mind.tools(), Tools::Unknown, "an empty reply");
    // A reply cut off at its length before it said anything.
    let r = rig(Tools::Unknown).await;
    r.stand_in.then(Answer::Text(String::new(), "length"));
    r.stand_in.then(Answer::text("**The dentist** [m1]."));
    r.ask("When?", &facts, Some(&lookups), Parts::free())
        .await
        .unwrap();
    assert_eq!(r.mind.tools(), Tools::Unknown, "a cut reply");
    // A 400 that names nothing: today's failure, no retry, nothing learned.
    let r = rig(Tools::Unknown).await;
    r.stand_in
        .then(Answer::Status(400, r#"{"error":"bad request"}"#.into()));
    assert!(
        r.ask("When?", &facts, Some(&lookups), Parts::free())
            .await
            .is_err()
    );
    assert_eq!(r.bodies().len(), 1);
    assert_eq!(r.mind.tools(), Tools::Unknown, "a refusal naming nothing");
    // A 400 about the length of what was sent, though it names the tools too.
    let r = rig(Tools::Unknown).await;
    r.stand_in.then(Answer::Status(
        400,
        r#"{"error":"maximum context length exceeded by messages and tools"}"#.into(),
    ));
    assert!(
        r.ask("When?", &facts, Some(&lookups), Parts::free())
            .await
            .is_err()
    );
    assert_eq!(r.mind.tools(), Tools::Unknown, "a length refusal");
    // A refusal naming look-ups whose retry is not answered either.
    let r = rig(Tools::Unknown).await;
    r.stand_in.then(Answer::Status(
        400,
        r#"{"error":"tools are not supported"}"#.into(),
    ));
    r.stand_in.then(Answer::Status(503, "{}".into()));
    assert!(
        r.ask("When?", &facts, Some(&lookups), Parts::free())
            .await
            .is_err()
    );
    assert_eq!(r.bodies().len(), 2);
    assert_eq!(r.mind.tools(), Tools::Unknown, "a retry not answered");
}

/// A look-up that takes longer than its 2 seconds is "couldn't be looked up just now", and the
/// question goes on, never ending as nothing kept.
#[tokio::test]
async fn a_slow_look_up_is_cut_off_and_the_question_goes_on() {
    let r = rig(Tools::Unknown).await;
    let mut lookups = r.look_ups();
    lookups.run = Arc::new(|_| {
        std::thread::sleep(Duration::from_millis(2_300));
        Some((vec![fact("late", "Too late to be given.")], 0))
    });
    r.stand_in
        .then(Answer::look("search", r#"{"words":"MOT"}"#));
    r.stand_in.then(Answer::text("NOT_SURE"));
    let a = r
        .ask("When is the car's MOT?", &[], Some(&lookups), Parts::free())
        .await
        .unwrap();
    assert!(!a.nothing, "a look-up that did not run is not nothing kept");
    assert_eq!(a.parts, 2);
    let back = r.bodies()[1]["messages"][3]["content"].clone();
    assert_eq!(back, look::COULD_NOT);
}

/// The whole question has its own time: past it, no more parts go.
#[tokio::test]
async fn the_question_stops_at_its_time_limit() {
    let r = rig(Tools::Unknown).await;
    let mut lookups = r.look_ups();
    lookups.limit = Duration::from_millis(1_200);
    lookups.run = Arc::new(|_| {
        std::thread::sleep(Duration::from_millis(1_500));
        Some((Vec::new(), 0))
    });
    r.stand_in
        .always(Answer::look("search", r#"{"words":"MOT"}"#));
    let facts = vec![fact("c1", "Something kept.")];
    let ended = r.ask("When?", &facts, Some(&lookups), Parts::free()).await;
    assert!(
        matches!(
            ended,
            Err(MindError::Online(nocturne_engine::OnlineFailure::Slow))
        ),
        "{ended:?}"
    );
    assert_eq!(r.bodies().len(), 1, "nothing goes past the time limit");
}

/// Stop while Nocturne looks: the next part never goes.
#[tokio::test]
async fn stop_while_looking_sends_nothing_more() {
    let r = rig(Tools::Unknown).await;
    let (stop_tx, stop) = tokio::sync::watch::channel(false);
    let stop_tx = Arc::new(stop_tx);
    let mut lookups = r.look_ups();
    let pressed = stop_tx.clone();
    lookups.run = Arc::new(move |_| {
        pressed.send_replace(true);
        Some((vec![fact("j1", "A line.")], 0))
    });
    r.stand_in.then(Answer::look(
        "journal",
        r#"{"from":"2026-11-17","to":"2026-11-17"}"#,
    ));
    let facts = vec![fact("c1", "Something kept.")];
    let (tx, _rx) = mpsc::channel(8);
    let today = today();
    let ended = r
        .mind
        .answer_asked(
            Asking {
                ask: "q1",
                question: "How was it?",
                facts: &facts,
                today: &today,
                look_ups: Some(&lookups),
                parts: Parts::free(),
                stop: Some(stop),
            },
            tx,
        )
        .await;
    assert!(ended.is_err(), "{ended:?}");
    assert_eq!(r.bodies().len(), 1, "no part after Stop");
}
