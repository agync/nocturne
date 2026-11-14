//! Online answers in the store: the settings row, what may be sent, the sent log as templates,
//! the month's counts, pruning, export and delete everything.

mod common;

use common::*;
use jiff::Timestamp;
use nocturne_contract::{
    LeftOut, LimitKind, LocalReason, OnlineLimit, OnlinePause, SendKind, SendResult, SentLine,
    SentMemory,
};
use nocturne_engine::{CorrectPatch, DELETE_PHRASE, NewSend, Predicate};

fn exec(dir: &std::path::Path, sql: &str, id: &str) {
    let conn = rusqlite::Connection::open(dir.join("nocturne.db")).unwrap();
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    conn.execute(sql, [id]).unwrap();
}

fn memory(f: &Fixture, sentence: &str) -> String {
    let words: Vec<&str> = sentence.split_whitespace().collect();
    learn(
        &f.engine,
        sentence,
        cand(
            words[0],
            Predicate::Happened,
            words[words.len() - 1],
            sentence,
        ),
    )
    .1
}

/// The line the answer prompt writes for a memory, around its sentence.
fn line_for(n: u32, sentence: &str) -> String {
    format!("[m{n}] {sentence} (learned Friday 13 November)")
}

/// A send of `ids`, each line built and stored as the online mind does.
fn send_of(f: &Fixture, ids: &[String], result: SendResult, counted: bool) -> NewSend {
    let mut template = vec![
        SentLine::Text {
            text: "Facts:".into(),
        },
        SentLine::Text {
            text: "<facts-n>".into(),
        },
    ];
    let mut memory_lines = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        let n = i as u32 + 1;
        let sentence = f.engine.get_memory(id).unwrap().sentence;
        memory_lines.push((id.clone(), NewSend::hash_line(&line_for(n, &sentence))));
        template.push(SentLine::Memory {
            n,
            text: Some(line_for(n, NewSend::SLOT)),
            state: SentMemory::Same,
        });
    }
    template.push(SentLine::Text {
        text: "</facts-n>".into(),
    });
    NewSend {
        at: f.engine.clock_now(),
        kind: SendKind::Question,
        host: "api.example.com".into(),
        model: "stand-in-model".into(),
        question: "When do the bins go out?".into(),
        instructions: "You answer a person's question.".into(),
        template,
        memory_lines,
        bytes_up: 900,
        bytes_down: 300,
        cost_usd: None,
        status: Some(200),
        seconds: Some(1.2),
        result,
        counted,
        reached: counted,
    }
}

fn answered() -> SendResult {
    SendResult::Answered { receipts: 1 }
}

#[test]
fn fresh_store_is_off() {
    let f = fixture();
    let row = f.engine.online_state().unwrap();
    assert!(!row.wanted_on && !row.set_up);
    assert!(row.phones && !row.send_sensitive);
    assert_eq!(row.limit, OnlineLimit::default());
    assert_eq!(row.limit.kind, LimitKind::None);
    assert_eq!(row.limit.questions, 200);
    assert!(row.base_url.is_none() && row.key_storage.is_none());
    assert!(row.pause.is_none() && row.problem.is_none());
    assert!(f.engine.online_sends(10, None).unwrap().is_empty());
}

#[test]
fn the_row_round_trips() {
    let f = fixture();
    let until = "2026-12-01T00:00:00+00:00".to_string();
    f.engine
        .update_online_state(|r| {
            r.wanted_on = true;
            r.set_up = true;
            r.base_url = Some("https://api.example.com/v1".into());
            r.model = Some("m".into());
            r.limit = OnlineLimit {
                kind: LimitKind::Usd,
                questions: 50,
                usd: Some(5.0),
            };
            r.pause = Some(OnlinePause::Limit {
                until: until.clone(),
            });
            r.on_since = Some(f.engine.clock_now());
        })
        .unwrap();
    let row = f.engine.online_state().unwrap();
    assert!(row.wanted_on && row.set_up);
    assert_eq!(row.limit.kind, LimitKind::Usd);
    assert_eq!(row.pause, Some(OnlinePause::Limit { until }));
    assert_eq!(row.on_since, Some(f.engine.clock_now()));
}

#[test]
fn sendable_fails_closed() {
    let f = fixture();
    let ok = memory(&f, "The bins go out on Tuesday");
    let ids = vec![ok.clone(), "mem_unknown".into(), String::new()];
    assert_eq!(f.engine.sendable(&ids, true).unwrap(), [true, false, false]);
    assert_eq!(
        f.engine.online_verdicts(&ids, true).unwrap(),
        [None, Some(LeftOut::Private), Some(LeftOut::Private)]
    );
}

#[test]
fn private_in_any_version_not_sendable() {
    let f = fixture();
    let id = memory(&f, "The safe code is 4321");
    let new = f
        .engine
        .correct(
            &id,
            CorrectPatch {
                object: "1234".into(),
                sentence: Some("The safe code is 1234".into()),
                due: None,
                valid_from: None,
                valid_to: None,
            },
        )
        .unwrap();
    assert_eq!(
        f.engine.sendable(std::slice::from_ref(&new), true).unwrap(),
        [true]
    );
    exec(
        f.dir.path(),
        "UPDATE memories SET private = 1 WHERE id = ?1",
        &id,
    );
    assert_eq!(
        f.engine.sendable(&[id, new], true).unwrap(),
        [false, false],
        "an earlier private version keeps the chain private"
    );
}

#[test]
fn sensitive_needs_the_switch() {
    let f = fixture();
    let id = memory(&f, "Sam takes insulin at night");
    exec(
        f.dir.path(),
        "UPDATE memories SET suggest_private = 1 WHERE id = ?1",
        &id,
    );
    let ids = vec![id];
    assert_eq!(
        f.engine.online_verdicts(&ids, false).unwrap(),
        [Some(LeftOut::Sensitive)]
    );
    assert_eq!(f.engine.sendable(&ids, true).unwrap(), [true]);
}

fn correct(f: &Fixture, id: &str, object: &str, sentence: &str) -> String {
    f.engine
        .correct(
            id,
            CorrectPatch {
                object: object.into(),
                sentence: Some(sentence.into()),
                due: None,
                valid_from: None,
                valid_to: None,
            },
        )
        .unwrap()
}

/// Health, money and ID details are found in any version's words or labels, not only in what
/// was suggested private when the current version was kept.
#[test]
fn sensitive_in_any_version_needs_the_switch() {
    let f = fixture();
    let plain = memory(&f, "Ana visited the bakery on Sunday");
    let bank = memory(&f, "Rent goes to GB82 WEST 1234 5698 7654 32");
    let passport = memory(&f, "Ana renewed passport AB1234567");
    let pay = memory(&f, "Ana earns £52,000 as salary");
    let was_card = memory(&f, "Ana uses card 4111 1111 1111 1111");
    let now_plain = correct(&f, &was_card, "a new card", "Ana uses a new card");
    let was_flagged = memory(&f, "Ana sees the nurse weekly");
    exec(
        f.dir.path(),
        "UPDATE memories SET suggest_private = 1 WHERE id = ?1",
        &was_flagged,
    );
    let flagged_now_plain = correct(&f, &was_flagged, "monthly", "Ana sees the nurse monthly");
    let labelled = memory(&f, "Ana sorted the paperwork");
    exec(
        f.dir.path(),
        "UPDATE memories SET tags = '[\"Bank\"]' WHERE id = ?1",
        &labelled,
    );
    let ids = vec![
        plain,
        bank,
        passport,
        pay,
        now_plain,
        flagged_now_plain,
        labelled,
    ];
    let s = Some(LeftOut::Sensitive);
    assert_eq!(
        f.engine.online_verdicts(&ids, false).unwrap(),
        [None, s, s, s, s, s, s]
    );
    assert_eq!(f.engine.sendable(&ids, true).unwrap(), [true; 7]);
}

/// A health line the local model tagged as something else is still found by its words, also in
/// an earlier version, and a plain line with an ordinary tag is not.
#[test]
fn health_words_need_the_switch_whatever_the_tag() {
    let f = fixture();
    let tagged = memory(&f, "Sam was diagnosed with depression");
    exec(
        f.dir.path(),
        "UPDATE memories SET tags = '[\"family\"]' WHERE id = ?1",
        &tagged,
    );
    let earlier = memory(&f, "Ana started therapy in May");
    let later = correct(&f, &earlier, "a hobby", "Ana started pottery in May");
    let plain = memory(&f, "Sam visited the bakery");
    exec(
        f.dir.path(),
        "UPDATE memories SET tags = '[\"family\"]' WHERE id = ?1",
        &plain,
    );
    let ids = vec![tagged, later, plain];
    let s = Some(LeftOut::Sensitive);
    assert_eq!(f.engine.online_verdicts(&ids, false).unwrap(), [s, s, None]);
}

#[test]
fn let_go_and_forgetting_not_sendable() {
    let f = fixture();
    let gone = memory(&f, "Ana visited in March");
    let going = memory(&f, "Ana called on Sunday");
    exec(
        f.dir.path(),
        "UPDATE memories SET let_go_at = '2026-11-01T00:00:00.000Z' WHERE id = ?1",
        &gone,
    );
    exec(
        f.dir.path(),
        "UPDATE memories SET forgetting = 1 WHERE id = ?1",
        &going,
    );
    assert_eq!(
        f.engine.sendable(&[gone, going], true).unwrap(),
        [false, false]
    );
}

#[test]
fn template_rebuilds_lines_while_memories_stay_the_same() {
    let f = fixture();
    let a = memory(&f, "The bins go out on Tuesday");
    let b = memory(&f, "The dentist is on Thursday");
    let id = f
        .engine
        .log_online_send(send_of(&f, &[a, b], answered(), true))
        .unwrap();
    let sends = f.engine.online_sends(10, None).unwrap();
    assert_eq!(sends.len(), 1);
    let s = &sends[0];
    assert_eq!(s.id, id as f64);
    assert_eq!(s.instructions, "You answer a person's question.");
    assert!(s.reached);
    assert_eq!(s.question, "When do the bins go out?");
    assert_eq!(
        s.lines[2],
        SentLine::Memory {
            n: 1,
            text: Some(line_for(1, "The bins go out on Tuesday")),
            state: SentMemory::Same
        }
    );
    assert_eq!(
        s.lines[0],
        SentLine::Text {
            text: "Facts:".into()
        }
    );
}

#[test]
fn log_never_holds_memory_words() {
    let f = fixture();
    let a = memory(&f, "Zebrafinch appointment at the vet");
    f.engine
        .log_online_send(send_of(&f, &[a], answered(), true))
        .unwrap();
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    let stored: String = conn
        .query_row(
            "SELECT template || memory_lines || question FROM online_sends",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!stored.contains("Zebrafinch"), "{stored}");
}

#[test]
fn forgotten_memory_shows_as_deleted_in_log() {
    let f = fixture();
    let a = memory(&f, "The bins go out on Tuesday");
    f.engine
        .log_online_send(send_of(&f, std::slice::from_ref(&a), answered(), true))
        .unwrap();
    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&a), false)
        .unwrap();
    f.engine.forget(&preview.token).unwrap();
    let s = &f.engine.online_sends(10, None).unwrap()[0];
    assert_eq!(
        s.lines[2],
        SentLine::Memory {
            n: 1,
            text: None,
            state: SentMemory::Deleted
        }
    );
    assert!(
        !serde_json::to_string(s)
            .unwrap()
            .contains("go out on Tuesday")
    );
    // Nothing about it is kept: no line, details, id or hash.
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    let stored: String = conn
        .query_row(
            "SELECT template || memory_lines FROM online_sends",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!stored.contains(&a), "{stored}");
    assert!(!stored.contains("learned"), "{stored}");
}

#[test]
fn changed_memory_shows_as_changed_in_log() {
    let f = fixture();
    let a = memory(&f, "The dentist is on Thursday");
    f.engine
        .log_online_send(send_of(&f, std::slice::from_ref(&a), answered(), true))
        .unwrap();
    f.engine
        .correct(
            &a,
            CorrectPatch {
                object: "Friday".into(),
                sentence: Some("The dentist is on Friday".into()),
                due: None,
                valid_from: None,
                valid_to: None,
            },
        )
        .unwrap();
    let s = &f.engine.online_sends(10, None).unwrap()[0];
    assert_eq!(
        s.lines[2],
        SentLine::Memory {
            n: 1,
            text: None,
            state: SentMemory::Changed
        }
    );
    assert!(!serde_json::to_string(s).unwrap().contains("Thursday"));
}

#[test]
fn month_count_resets_on_the_first_local() {
    // 23:30 on 30 November in London is still November; at midnight it is December.
    let f = fixture_at("2026-11-30T23:30:00Z", "Europe/London");
    let tz = jiff::tz::TimeZone::get("Europe/London").unwrap();
    let a = memory(&f, "The bins go out on Tuesday");
    for _ in 0..3 {
        f.engine
            .log_online_send(send_of(&f, std::slice::from_ref(&a), answered(), true))
            .unwrap();
    }
    assert_eq!(f.engine.questions_this_month(&tz).unwrap(), 3);
    assert_eq!(f.engine.online_resets_on(&tz), "2026-12-01");
    f.clock.set_str("2026-12-01T00:00:01Z");
    assert_eq!(f.engine.questions_this_month(&tz).unwrap(), 0);
    // In a zone east of UTC the month turned an hour earlier.
    let f = fixture_at("2026-11-30T23:30:00Z", "Europe/Berlin");
    let berlin = jiff::tz::TimeZone::get("Europe/Berlin").unwrap();
    let a = memory(&f, "The bins go out on Tuesday");
    f.engine
        .log_online_send(send_of(&f, &[a], answered(), true))
        .unwrap();
    assert_eq!(f.engine.questions_this_month(&berlin).unwrap(), 1);
    assert_eq!(f.engine.online_resets_on(&berlin), "2027-01-01");
}

#[test]
fn checks_not_counted() {
    let f = fixture();
    let tz = jiff::tz::TimeZone::get("Europe/London").unwrap();
    let mut check = send_of(&f, &[], answered(), true);
    check.kind = SendKind::Check;
    f.engine.log_online_send(check).unwrap();
    assert_eq!(f.engine.questions_this_month(&tz).unwrap(), 0);
}

#[test]
fn not_sent_not_counted() {
    let f = fixture();
    let tz = jiff::tz::TimeZone::get("Europe/London").unwrap();
    let a = memory(&f, "The bins go out on Tuesday");
    f.engine
        .log_online_send(send_of(
            &f,
            std::slice::from_ref(&a),
            SendResult::NotSent {
                reason: LocalReason::Limit,
            },
            false,
        ))
        .unwrap();
    f.engine
        .log_online_send(send_of(
            &f,
            &[a],
            SendResult::Failed {
                reason: LocalReason::NoInternet,
            },
            true,
        ))
        .unwrap();
    assert_eq!(
        f.engine.questions_this_month(&tz).unwrap(),
        1,
        "a failed send reached the route; one not sent did not"
    );
    let sends = f.engine.online_sends(10, None).unwrap();
    assert!(sends[0].reached && !sends[1].reached, "newest first");
}

#[test]
fn cost_is_summed_and_reported_only_when_the_last_three_had_it() {
    let f = fixture();
    let tz = jiff::tz::TimeZone::get("Europe/London").unwrap();
    let a = memory(&f, "The bins go out on Tuesday");
    let with = |cost: Option<f64>| {
        let mut s = send_of(&f, std::slice::from_ref(&a), answered(), true);
        s.cost_usd = cost;
        s
    };
    assert_eq!(f.engine.spent_this_month(&tz).unwrap(), None);
    f.engine.log_online_send(with(None)).unwrap();
    f.engine.log_online_send(with(Some(0.01))).unwrap();
    f.engine.log_online_send(with(Some(0.02))).unwrap();
    assert!(!f.engine.online_cost_reported().unwrap());
    f.engine.log_online_send(with(Some(0.03))).unwrap();
    assert!(f.engine.online_cost_reported().unwrap());
    let spent = f.engine.spent_this_month(&tz).unwrap().unwrap();
    assert!((spent - 0.06).abs() < 1e-9);
}

#[test]
fn pruned_after_90_days() {
    let f = fixture();
    let a = memory(&f, "The bins go out on Tuesday");
    f.engine
        .log_online_send(send_of(&f, std::slice::from_ref(&a), answered(), true))
        .unwrap();
    f.clock.set_str("2027-03-01T07:00:00Z");
    f.engine
        .log_online_send(send_of(&f, &[a], answered(), true))
        .unwrap();
    let cutoff: Timestamp = f
        .engine
        .clock_now()
        .checked_sub(jiff::SignedDuration::from_hours(24 * 90))
        .unwrap();
    assert_eq!(f.engine.prune_online_sends(cutoff).unwrap(), 1);
    assert_eq!(f.engine.online_sends(10, None).unwrap().len(), 1);
}

#[test]
fn sends_page_newest_first() {
    let f = fixture();
    let a = memory(&f, "The bins go out on Tuesday");
    let ids: Vec<i64> = (0..5)
        .map(|_| {
            f.engine
                .log_online_send(send_of(&f, std::slice::from_ref(&a), answered(), true))
                .unwrap()
        })
        .collect();
    let first = f.engine.online_sends(2, None).unwrap();
    assert_eq!(
        first.iter().map(|s| s.id as i64).collect::<Vec<_>>(),
        [ids[4], ids[3]]
    );
    let next = f.engine.online_sends(2, Some(ids[3])).unwrap();
    assert_eq!(
        next.iter().map(|s| s.id as i64).collect::<Vec<_>>(),
        [ids[2], ids[1]]
    );
    let (last, kept) = f.engine.last_online_send().unwrap();
    assert_eq!(kept, 5);
    assert_eq!(last.unwrap().status, Some(200));
}

#[test]
fn export_has_readable_log_and_no_key() {
    let f = fixture();
    let a = memory(&f, "The bins go out on Tuesday");
    let gone = memory(&f, "Ana visited in March");
    f.engine
        .log_online_send(send_of(&f, &[a, gone.clone()], answered(), true))
        .unwrap();
    let preview = f.engine.preview_forget(&[gone], false).unwrap();
    f.engine.forget(&preview.token).unwrap();
    let out = tempfile::tempdir().unwrap();
    let report = f.engine.export_to(out.path()).unwrap();
    let sent =
        std::fs::read_to_string(std::path::Path::new(&report.path).join("online/sent.md")).unwrap();
    assert!(sent.contains("api.example.com"));
    assert!(sent.contains("When do the bins go out?"));
    assert!(sent.contains("[m1] The bins go out on Tuesday"));
    assert!(sent.contains("[m2] A memory you have since deleted"));
    assert!(!sent.contains("Ana visited"));
    assert!(!sent.to_lowercase().contains("bearer") && !sent.contains("sk-"));
}

#[test]
fn delete_everything_clears_online_rows() {
    let f = fixture();
    let a = memory(&f, "The bins go out on Tuesday");
    f.engine
        .update_online_state(|r| {
            r.wanted_on = true;
            r.set_up = true;
            r.base_url = Some("https://api.example.com/v1".into());
        })
        .unwrap();
    f.engine
        .log_online_send(send_of(&f, &[a], answered(), true))
        .unwrap();
    f.engine.add_sent_online(900).unwrap();
    f.engine.delete_everything(DELETE_PHRASE).unwrap();
    let row = f.engine.online_state().unwrap();
    assert!(!row.wanted_on && !row.set_up && row.base_url.is_none());
    assert_eq!(row.sent_bytes, 0.0);
    assert!(f.engine.online_sends(10, None).unwrap().is_empty());
}

#[test]
fn sent_bytes_add_up() {
    let f = fixture();
    f.engine.add_sent_online(900).unwrap();
    f.engine.add_sent_online(100).unwrap();
    assert_eq!(f.engine.online_state().unwrap().sent_bytes, 1000.0);
}
