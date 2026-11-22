//! The decision check on scripted deciders: the seeding, the wiring and the scoring, not a size
//! and not a model. The real run (`make e2e-decisions`) asks the mind and a decision model.

mod eval;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use eval::decisions::*;
use nocturne_engine::decide::{Answer, Backend, Decider, Decision, Kind};
use nocturne_engine::{BoxFuture, Engine, ManualClock, MindError};

const WAIT: Duration = Duration::from_secs(5);

fn world() -> (tempfile::TempDir, Engine, Arc<ManualClock>) {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::at("2026-11-12T08:00:00Z", "Europe/London"));
    let engine = Engine::open(dir.path(), clock.clone()).unwrap();
    (dir, engine, clock)
}

fn certain(label: &str) -> Answer {
    Answer {
        label: label.into(),
        probabilities: BTreeMap::from([(label.to_string(), 1.0)]),
        by: "scripted".into(),
    }
}

fn key(d: &Decision) -> String {
    format!("{}\n{}", d.question.instructions, d.state)
}

/// Answers with the label of the case whose question and state it is shown.
struct Oracle(Vec<(String, String)>);

impl Decider for Oracle {
    fn decide<'a>(&'a self, d: &'a Decision) -> BoxFuture<'a, Result<Answer, MindError>> {
        Box::pin(async move {
            let (_, label) = self
                .0
                .iter()
                .find(|(k, _)| *k == key(d))
                .expect("a state the oracle knows");
            Ok(certain(label))
        })
    }
    fn backend(&self) -> Backend {
        Backend::Mind
    }
}

/// Always answers the one label, or fails.
struct Fixed(Option<&'static str>);

impl Decider for Fixed {
    fn decide<'a>(&'a self, _: &'a Decision) -> BoxFuture<'a, Result<Answer, MindError>> {
        Box::pin(async move {
            match self.0 {
                Some(label) => Ok(certain(label)),
                None => Err(MindError::Unavailable),
            }
        })
    }
    fn backend(&self) -> Backend {
        Backend::Mind
    }
}

struct Slow;

impl Decider for Slow {
    fn decide<'a>(&'a self, _: &'a Decision) -> BoxFuture<'a, Result<Answer, MindError>> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(certain("yes"))
        })
    }
    fn backend(&self) -> Backend {
        Backend::Mind
    }
}

fn seeded() -> (tempfile::TempDir, Vec<Seeded>) {
    let (dir, engine, clock) = world();
    let rows = seed(&engine, &clock);
    (dir, rows)
}

#[test]
fn every_case_is_logged_with_the_real_rule_outcome() {
    let (_dir, rows) = seeded();
    for kind in Kind::ALL {
        let n = rows.iter().filter(|s| s.kind == kind).count();
        assert_eq!(n, case_count(kind), "{kind:?}: every case has one row");
        assert!(n >= 12, "{kind:?}: {n} cases");
    }
    let mut ids: Vec<_> = rows.iter().map(|s| s.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), rows.len(), "ids are unique");
    for s in &rows {
        assert_eq!(s.pending.kind, s.kind);
        assert!(!s.pending.decision.state.is_empty());
    }
    assert!(
        rows.iter()
            .filter(|s| s.kind == Kind::FollowedUp)
            .all(|s| s.pending.rule),
        "the page rule is logged only when it stopped the question"
    );
}

#[test]
fn the_oracle_scores_perfectly_and_the_losing_answer_loses() {
    let (_dir, rows) = seeded();
    let pending = rows.clone();
    let oracle = Oracle(
        pending
            .iter()
            .map(|s| (key(&s.pending.decision), s.label.to_string()))
            .collect(),
    );
    let results = block_on(run_arm(&oracle, &pending, WAIT));
    let report = score_arm("oracle", &results);
    for kind in Kind::ALL {
        let s = report.kind(kind.as_str());
        assert_eq!(s.n, case_count(kind));
        assert_eq!(s.correct, s.n, "{kind:?}");
        assert!((s.accuracy - 1.0).abs() < 1e-9);
        assert_eq!(s.losing_errors, 0);
        assert_eq!(s.errors, 0);
        assert_eq!(s.band, 0);
    }

    for kind in Kind::ALL {
        let losing = kind.losing_verdict();
        let label = match kind {
            Kind::Moved => "moved",
            Kind::FollowedUp => "yes",
        };
        let of_kind: Vec<_> = pending.iter().filter(|s| s.kind == kind).cloned().collect();
        let results = block_on(run_arm(&Fixed(Some(label)), &of_kind, WAIT));
        let s = score_arm("losing", &results);
        let s = s.kind(kind.as_str());
        let wrong = of_kind.iter().filter(|c| c.truth() != losing).count();
        assert!(wrong > 0, "{kind:?} has cases of the other label");
        assert_eq!(s.losing_errors, wrong, "{kind:?}");
        assert_eq!(s.correct, s.n - wrong);
        assert!((s.losing_error_rate - 1.0).abs() < 1e-9, "{kind:?}");
    }
}

#[test]
fn an_error_or_a_timeout_is_an_error_with_no_verdict_and_the_rule_stands() {
    let (_dir, rows) = seeded();
    let rule_before = score_arm("rules", &rule_arm(&rows));
    let results = block_on(run_arm(&Fixed(None), &rows, WAIT));
    assert_eq!(results.len(), rows.len());
    assert!(
        results
            .iter()
            .all(|r| r.verdict.is_none() && r.error.is_some())
    );
    let report = score_arm("broken", &results);
    for kind in Kind::ALL {
        let s = report.kind(kind.as_str());
        assert_eq!(s.errors, s.n);
        assert_eq!(s.correct, 0);
        assert_eq!(s.losing_errors, 0);
        assert_eq!(s.band, 0, "an error is not the band");
    }
    assert_eq!(
        score_arm("rules", &rule_arm(&rows)).json(),
        rule_before.json(),
        "the rule arm does not depend on a decider"
    );

    let few: Vec<_> = rows.iter().take(2).cloned().collect();
    let results = block_on(run_arm(&Slow, &few, Duration::from_millis(20)));
    assert!(results.iter().all(|r| r.verdict.is_none()));
    assert!(
        results
            .iter()
            .all(|r| r.error.as_deref().is_some_and(|e| e.contains("timed out")))
    );
}

#[test]
fn an_unsure_answer_is_the_band_and_not_an_error() {
    let (_dir, rows) = seeded();
    let of_kind: Vec<_> = rows
        .iter()
        .filter(|s| s.kind == Kind::FollowedUp)
        .cloned()
        .collect();
    struct Unsure;
    impl Decider for Unsure {
        fn decide<'a>(&'a self, _: &'a Decision) -> BoxFuture<'a, Result<Answer, MindError>> {
            Box::pin(async move {
                Ok(Answer {
                    label: "yes".into(),
                    probabilities: BTreeMap::from([("yes".into(), 0.4), ("no".into(), 0.4)]),
                    by: "scripted".into(),
                })
            })
        }
        fn backend(&self) -> Backend {
            Backend::Mind
        }
    }
    let results = block_on(run_arm(&Unsure, &of_kind, WAIT));
    let report = score_arm("unsure", &results);
    let s = report.kind("followed_up");
    assert_eq!(s.band, s.n);
    assert_eq!(s.errors, 0);
    assert_eq!(s.correct, 0);
}

#[test]
fn the_rule_arm_is_reported_and_its_numbers_agree() {
    let (_dir, rows) = seeded();
    let results = rule_arm(&rows);
    assert_eq!(results.len(), rows.len());
    let report = score_arm("rules", &results);
    for kind in Kind::ALL {
        let s = report.kind(kind.as_str());
        let mine: Vec<_> = results
            .iter()
            .filter(|r| r.group == kind.as_str())
            .collect();
        assert_eq!(s.n, mine.len());
        let correct = mine.iter().filter(|r| r.verdict == Some(r.label)).count();
        assert_eq!(s.correct, correct);
        let losing = mine
            .iter()
            .filter(|r| {
                r.verdict == Some(kind.losing_verdict()) && r.label != kind.losing_verdict()
            })
            .count();
        assert_eq!(s.losing_errors, losing);
        assert_eq!(s.errors, 0);
        assert_eq!(s.band, 0);
        assert!(s.correct <= s.n);
    }
    let table = markdown(std::slice::from_ref(&report));
    println!("{table}");
    assert!(table.contains("rules"));
    let json = report.json();
    assert_eq!(json["arm"], "rules");
    assert!(json["kinds"]["moved"]["n"].as_u64().unwrap() >= 12);
}

#[test]
fn beating_a_baseline_needs_five_points_and_no_more_losing_errors() {
    let base = Score {
        n: 20,
        correct: 12,
        accuracy: 0.6,
        losing_errors: 2,
        losing_error_rate: 0.2,
        ..Score::default()
    };
    let better = Score {
        correct: 14,
        accuracy: 0.7,
        losing_errors: 1,
        losing_error_rate: 0.1,
        ..base.clone()
    };
    assert!(beats(&better, &base));
    let barely = Score {
        accuracy: 0.62,
        ..better.clone()
    };
    assert!(!beats(&barely, &base));
    let riskier = Score {
        losing_error_rate: 0.3,
        ..better.clone()
    };
    assert!(!beats(&riskier, &base));
    let level = Score {
        accuracy: 0.65,
        ..better
    };
    assert!(beats(&level, &base), "exactly five points counts");
}

#[test]
fn latency_percentiles_come_from_the_asked_cases() {
    let results: Vec<CaseResult> = (1..=100u64)
        .map(|ms| CaseResult {
            id: "x".into(),
            group: "followed_up",
            label: true,
            losing: false,
            verdict: Some(true),
            error: None,
            ms,
        })
        .collect();
    let s = score_arm("timed", &results).kind("followed_up");
    assert_eq!(s.p50_ms, 50);
    assert_eq!(s.p95_ms, 95);
}

#[test]
fn the_reflection_guard_scores_with_an_oracle() {
    let cases = guard_cases();
    assert!(cases.len() >= 20);
    assert!(
        cases.iter().any(|c| c.label),
        "bad sentences are in the set"
    );
    assert!(
        cases.iter().any(|c| !c.label),
        "good sentences are in the set"
    );
    let oracle = Oracle(
        cases
            .iter()
            .map(|c| {
                (
                    key(&c.decision),
                    if c.label { "yes" } else { "no" }.to_string(),
                )
            })
            .collect(),
    );
    let results = block_on(run_guard_arm(&oracle, &cases, WAIT));
    let report = score_arm("guard oracle", &results);
    for group in ["advice", "feeling"] {
        let s = report.kind(group);
        assert!(s.n > 0, "{group}");
        assert_eq!(s.correct, s.n, "{group}");
        assert_eq!(s.losing_errors, 0);
    }
    let blind = block_on(run_guard_arm(&Fixed(Some("no")), &cases, WAIT));
    let report = score_arm("blind", &blind);
    for group in ["advice", "feeling"] {
        let s = report.kind(group);
        let bad = cases.iter().filter(|c| c.group == group && c.label).count();
        assert!(bad > 0, "{group} has bad sentences");
        assert_eq!(s.losing_errors, bad, "a missed bad sentence is the loss");
    }
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tauri::async_runtime::block_on(f)
}
