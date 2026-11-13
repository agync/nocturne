//! A world to drive: a store, a manual clock, a scripted mind, a mutable host, recorded events.
#![allow(dead_code)]

use std::sync::Arc;

use jiff::civil::Date;
use nocturne_contract::{Kept, Source};
use nocturne_engine::{Candidate, Clock, Engine, FakeMind, ManualClock, Mind, Predicate};
use nocturne_intelligence::{
    FakeHost, MindSlot, Reader, RecordingEvents, Scheduler, Step, Tidy, new_mind_slot,
};

pub struct World {
    pub dir: tempfile::TempDir,
    pub engine: Engine,
    pub clock: Arc<ManualClock>,
    pub mind: Arc<FakeMind>,
    pub slot: MindSlot,
    pub host: Arc<FakeHost>,
    pub events: Arc<RecordingEvents>,
    pub reader: Reader,
    pub tidy: Tidy,
    pub scheduler: Scheduler,
}

/// A world at `now` (London), with the mind in place.
pub fn world_at(now: &str, mind: FakeMind) -> World {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::at(now, "Europe/London"));
    let engine = Engine::open(dir.path(), clock.clone()).unwrap();
    let mind = Arc::new(mind);
    let slot = new_mind_slot();
    *slot.write().unwrap() = Some(mind.clone() as Arc<dyn Mind>);
    let host = FakeHost::new();
    let events = RecordingEvents::new();
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let reader = Reader::new(
        engine.clone(),
        slot.clone(),
        host.clone(),
        dyn_clock.clone(),
        events.clone(),
    );
    let tidy = Tidy::new(
        engine.clone(),
        reader.clone(),
        slot.clone(),
        host.clone(),
        dyn_clock.clone(),
        events.clone(),
    );
    let scheduler = Scheduler::new(
        engine.clone(),
        reader.clone(),
        tidy.clone(),
        host.clone(),
        dyn_clock,
        events.clone(),
    );
    World {
        dir,
        engine,
        clock,
        mind,
        slot,
        host,
        events,
        reader,
        tidy,
        scheduler,
    }
}

impl World {
    /// The mind leaves the slot (the runtime is down).
    pub fn mind_away(&self) {
        *self.slot.write().unwrap() = None;
    }

    /// The scripted mind is back in the slot.
    pub fn mind_back(&self) {
        *self.slot.write().unwrap() = Some(self.mind.clone() as Arc<dyn Mind>);
    }
}

pub fn world(mind: FakeMind) -> World {
    world_at("2026-11-13T09:00:00Z", mind)
}

pub fn date(s: &str) -> Date {
    s.parse().unwrap()
}

pub fn keep(w: &World, text: &str) -> String {
    id_of(w.engine.keep_text(text, Source::Paste).unwrap())
}

pub fn id_of(kept: Kept) -> String {
    match kept {
        Kept::New { item } | Kept::Duplicate { item } => item.id,
    }
}

/// A recording from quick capture; returns the item id and the stored file name the fake mind
/// is scripted by.
pub fn keep_voice(w: &World, bytes: &[u8]) -> (String, String) {
    let id = id_of(
        w.engine
            .keep_bytes(bytes, "m4a", Source::QuickCapture)
            .unwrap(),
    );
    let name = stored_name(w, &id);
    (id, name)
}

pub fn keep_photo(w: &World, bytes: &[u8], source: Source) -> (String, String) {
    let id = id_of(w.engine.keep_bytes(bytes, "jpg", source).unwrap());
    let name = stored_name(w, &id);
    (id, name)
}

pub fn keep_pdf(w: &World, bytes: &[u8]) -> (String, String) {
    let src = tempfile::tempdir().unwrap();
    let path = src.path().join("scan.pdf");
    std::fs::write(&path, bytes).unwrap();
    let id = id_of(w.engine.keep_file(&path, Source::Drop).unwrap());
    let name = stored_name(w, &id);
    (id, name)
}

fn stored_name(w: &World, id: &str) -> String {
    let path = w.engine.get_item(id).unwrap().original_path.unwrap();
    std::path::Path::new(&path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

pub fn gran() -> Candidate {
    Candidate::new(
        "Gran",
        Predicate::LivesIn,
        "Leeds",
        "Gran lives in Leeds.",
        0.8,
    )
}

pub fn status(w: &World, id: &str) -> nocturne_contract::ItemStatus {
    w.engine.get_item(id).unwrap().status
}

/// Tick the reader until it has nothing more to do (at most 50 steps).
pub async fn read_all(w: &World) -> Vec<Step> {
    let mut steps = Vec::new();
    for _ in 0..50 {
        let step = w.reader.tick().await;
        let more = matches!(step, Step::Read { .. });
        steps.push(step);
        if !more {
            break;
        }
    }
    steps
}
