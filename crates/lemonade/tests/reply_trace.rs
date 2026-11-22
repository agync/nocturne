//! Raw replies are traced for evaluation runs only. This test has a binary of its own because it
//! installs a subscriber for its thread: tracing keeps what a callsite is interested in for the
//! whole process, and a callsite first reached by another thread, which does not see this
//! subscriber, is cached as uninteresting while this one is the only subscriber alive. Beside
//! other tests that made the capture miss events now and then.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nocturne_contract::{EntityKind, ItemKind, Tier};
use nocturne_engine::{ExtractContext, Fact, Mind};
use nocturne_lemonade::{LemonadeMind, Supervisor, SupervisorOptions, selection};
use serde_json::{Value, json};
use tokio::sync::mpsc;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Captured {
        self.clone()
    }
}

fn context() -> ExtractContext {
    ExtractContext {
        item_kind: ItemKind::Text,
        kept_at: "2026-11-10T09:30:00Z".parse().unwrap(),
        today: "2026-11-10".parse().unwrap(),
        known_entities: vec![("Sam".into(), EntityKind::Person)],
    }
}

fn good_memory() -> Value {
    json!({
        "sentence": "Sam prefers tea in the morning.",
        "subject": "Sam", "subjectKind": "person", "predicate": "prefers", "object": "tea",
        "validFrom": "2026-11-10", "validTo": null, "due": "2026-11-20", "confidence": 0.9,
        "span": {"quote": "Sam likes tea"}, "tags": ["Drinks", " morning "],
    })
}

fn fact() -> Fact {
    serde_json::from_value::<nocturne_contract::MemoryView>(json!({
        "id": "a", "sentence": "Dentist on Monday.", "subject": "Sam", "entityId": null,
        "predicate": "scheduled", "object": "x", "certainty": "sure", "status": "active",
        "validFrom": null, "validTo": null, "due": null, "learnedAt": "2026-11-10T09:30:00+00:00",
        "lastUsed": null, "letGoAt": null, "keepForever": false, "private": false,
        "was": null, "receipts": [],
    }))
    .unwrap()
    .into()
}

/// Raw replies are traced for evaluation runs only: a log kept at info never holds them.
#[tokio::test]
async fn raw_replies_are_traced_only_below_info() {
    for (filter, shown) in [("info", false), ("info,nocturne::reply=trace", true)] {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = common::dirs(tmp.path());
        common::set_mode(&dirs, "downloaded=all");
        let sup = Supervisor::start(
            dirs,
            SupervisorOptions {
                health_interval: Duration::from_millis(200),
                ..SupervisorOptions::default()
            },
        )
        .await
        .unwrap();
        let tier = Tier::Standard;
        let mind = LemonadeMind::new(
            sup.client(),
            selection::picks(&selection::Selection::preset(tier)),
            tier,
        );
        let e = sup.client().endpoint().unwrap();
        reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/fake/script", e.port))
            .bearer_auth(e.key)
            .json(&json!({"chat": [
                {"content": json!({"memories": [good_memory()]}).to_string()},
                {"deltas": ["Monday [m1]."]},
            ]}))
            .send()
            .await
            .unwrap();

        let log = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(log.clone())
            .with_ansi(false)
            .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        mind.extract("Sam likes tea", &context()).await.unwrap();
        let (tx, mut rx) = mpsc::channel(64);
        mind.answer("When do I see Sam?", &[fact()], tx)
            .await
            .unwrap();
        while rx.recv().await.is_some() {}
        let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
        assert_eq!(
            text.contains("Sam prefers tea in the morning"),
            shown,
            "{filter}: {text}"
        );
        assert_eq!(text.contains("step=\"extract\""), shown, "{filter}");
        assert_eq!(text.contains("step=\"answer\""), shown, "{filter}");
        assert!(!text.contains("Sam prefers tea") || shown);
    }
}
