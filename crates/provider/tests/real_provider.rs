//! A run against a real provider, by hand. Ignored unless the three details are given, as a
//! person would type them, in the environment:
//!
//! ```text
//! NOCTURNE_PROVIDER_URL=https://provider.example/v1 NOCTURNE_PROVIDER_KEY=... \
//! NOCTURNE_PROVIDER_MODEL=some-model \
//!   cargo test -p nocturne-provider --test real_provider -- --ignored --nocapture
//! ```
//!
//! It sends the check, one reading of the example note, one answer from two made-up lines and
//! one headline, and prints what came back. Nothing of the person's is sent.

use std::sync::{Arc, Mutex};

use nocturne_contract::{CheckOutcome, EntityKind, ItemKind, MemoryView};
use nocturne_engine::{ExtractContext, Fact, Mind};
use nocturne_mind::prompts;
use nocturne_provider::address::OnlineKey;
use nocturne_provider::{Exchange, Job, Provider, ProviderMind};
use serde_json::json;
use tokio::sync::mpsc;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))
}

fn provider() -> Provider {
    Provider::new(
        &env("NOCTURNE_PROVIDER_URL"),
        &env("NOCTURNE_PROVIDER_MODEL"),
        OnlineKey::new(env("NOCTURNE_PROVIDER_KEY")),
    )
    .expect("an address the rules allow")
}

fn memory(id: &str, sentence: &str) -> MemoryView {
    serde_json::from_value(json!({
        "id": id, "sentence": sentence, "subject": "Bins", "entityId": null,
        "predicate": "scheduled", "object": "Tuesday", "certainty": "sure", "status": "active",
        "validFrom": null, "validTo": null, "due": null, "learnedAt": "2026-11-10T09:30:00+00:00",
        "lastUsed": null, "letGoAt": null, "keepForever": false, "private": false,
        "was": null, "receipts": [],
    }))
    .unwrap()
}

#[tokio::test]
#[ignore = "needs NOCTURNE_PROVIDER_URL, NOCTURNE_PROVIDER_KEY and NOCTURNE_PROVIDER_MODEL"]
async fn a_real_provider_reads_answers_and_writes() {
    let (outcome, _) = provider().check().await;
    println!("check: {outcome:?}");
    assert!(matches!(outcome, CheckOutcome::Works { .. }), "{outcome:?}");

    let made: Arc<Mutex<Vec<(Job, Exchange)>>> = Arc::default();
    let mind = ProviderMind::new(provider()).with_record({
        let made = made.clone();
        Arc::new(move |job, e| made.lock().unwrap().push((job, e.clone())))
    });
    let cx = ExtractContext {
        item_kind: ItemKind::Text,
        kept_at: "2026-03-02T09:00:00Z".parse().unwrap(),
        today: prompts::EXAMPLE_KEPT.parse().unwrap(),
        known_entities: vec![("Ana".into(), EntityKind::Person)],
    };
    let found = mind.extract(prompts::EXAMPLE_NOTE, &cx).await;
    println!("reading: {found:#?}");
    assert!(found.is_ok_and(|f| !f.is_empty()));

    let facts = vec![
        Fact::from(memory("mem_1", "The bins go out on Tuesday.")),
        Fact::from(memory(
            "mem_2",
            "The recycling goes out every other Friday.",
        )),
    ];
    let (tx, mut rx) = mpsc::channel(64);
    let meta = mind
        .answer("When do the bins go out?", &facts, tx)
        .await
        .unwrap();
    let mut shown = String::new();
    while let Ok(part) = rx.try_recv() {
        shown.push_str(&part);
    }
    println!(
        "answer: {shown:?} cited {:?} not sure {}",
        meta.cited, meta.not_sure
    );

    let (headline, dek) = mind
        .write_headline(&serde_json::from_value(page()).unwrap())
        .await
        .unwrap();
    println!("headline: {headline:?} / {dek:?}");
    for (job, e) in made.lock().unwrap().iter() {
        println!(
            "{job:?}: status {:?}, {} bytes up, {} down, {:.1} s",
            e.status, e.bytes_up, e.bytes_down, e.seconds
        );
    }
}

fn page() -> serde_json::Value {
    json!({
        "date": "2026-11-14", "pageNumber": 2, "edition": "screen", "variant": "normal",
        "headline": "", "dek": "", "needsYou": [], "changed": [], "letGo": [], "upcoming": [],
        "learned": [], "question": null, "moreNeedsYou": 0, "moreChanged": 0, "keptCount": 4,
        "changesCount": 0, "letGoCount": 0, "catchUpDays": 0, "privateSkipped": 0, "pending": 0,
        "tidiedFrom": null, "tidiedTo": null, "update": null, "journalLine": null,
        "journalSkipped": null, "calendarToday": [], "moreCalendarToday": 0, "lastNight": null,
    })
}

#[test]
fn the_example_page_reads() {
    let _: nocturne_contract::PageView = serde_json::from_value(page()).unwrap();
}
