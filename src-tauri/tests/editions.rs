//! The spoken page through the mock runtime and a mind that returns real WAV audio.

mod common;

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{Harness, harness_with_mind};
use nocturne_app_lib::editions::speech::{prepare_if_due, spoken_page};
use nocturne_app_lib::os::OsState;
use nocturne_contract::PageView;
use nocturne_engine::{
    AnswerMeta, BoxFuture, Candidate, ExtractContext, FakeMind, Mind, MindError, Predicate,
    Transcript, Understanding,
};
use serde_json::{Value, json};
use tauri::Manager;
use tokio::sync::mpsc;

/// A 24 kHz mono 16-bit WAV of `ms` milliseconds.
fn wav(rate: u32, ms: u32) -> Vec<u8> {
    let frames = (rate * ms / 1000) as usize;
    let data = vec![1u8; frames * 2];
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

type Hook = Box<dyn Fn(usize) + Send + Sync>;

/// A voice that records what it was asked to say and answers with audio.
struct Voice {
    inner: FakeMind,
    said: Mutex<Vec<String>>,
    calls: AtomicUsize,
    hook: Mutex<Option<Hook>>,
    delay: Duration,
    /// Sample rate by call number, to make a voice that changes format.
    rate: Box<dyn Fn(usize) -> u32 + Send + Sync>,
}

impl Voice {
    fn new() -> Arc<Voice> {
        Arc::new(Voice {
            inner: FakeMind::new(),
            said: Mutex::default(),
            calls: AtomicUsize::new(0),
            hook: Mutex::new(None),
            delay: Duration::ZERO,
            rate: Box::new(|_| 24_000),
        })
    }

    fn said(&self) -> Vec<String> {
        self.said.lock().unwrap().clone()
    }
}

impl Mind for Voice {
    fn transcribe<'a>(&'a self, a: &'a Path) -> BoxFuture<'a, Result<Transcript, MindError>> {
        self.inner.transcribe(a)
    }

    fn read_image<'a>(&'a self, i: &'a Path) -> BoxFuture<'a, Result<Understanding, MindError>> {
        self.inner.read_image(i)
    }

    fn read_document<'a>(&'a self, d: &'a Path) -> BoxFuture<'a, Result<Understanding, MindError>> {
        self.inner.read_document(d)
    }

    fn extract<'a>(
        &'a self,
        t: &'a str,
        cx: &'a ExtractContext,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
        self.inner.extract(t, cx)
    }

    fn embed<'a>(&'a self, t: &'a [String]) -> BoxFuture<'a, Result<Vec<Vec<f32>>, MindError>> {
        self.inner.embed(t)
    }

    fn answer<'a>(
        &'a self,
        q: &'a str,
        f: &'a [nocturne_contract::MemoryView],
        o: mpsc::Sender<String>,
    ) -> BoxFuture<'a, Result<AnswerMeta, MindError>> {
        self.inner.answer(q, f, o)
    }

    fn write_headline<'a>(
        &'a self,
        p: &'a PageView,
    ) -> BoxFuture<'a, Result<(String, String), MindError>> {
        self.inner.write_headline(p)
    }

    fn speak<'a>(
        &'a self,
        text: &'a str,
        _voice: &'a str,
    ) -> BoxFuture<'a, Result<Vec<u8>, MindError>> {
        Box::pin(async move {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            self.said.lock().unwrap().push(text.to_string());
            if let Some(hook) = self.hook.lock().unwrap().as_ref() {
                hook(n);
            }
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            Ok(wav((self.rate)(n), 100))
        })
    }
}

/// Two memories due on different days, a saved page for today, the clinic one private.
fn page_with_two_memories(h: &Harness) -> (String, String) {
    let due = |subject: &str, object: &str, sentence: &str, day: &str| {
        let mut c = Candidate::new(subject, Predicate::Due, object, sentence, 0.9);
        c.due = Some(day.parse().unwrap());
        c
    };
    let (_, rent) = h.learn_candidate(
        "rent note",
        due("Rent", "rent", "Rent is due on Saturday.", "2026-11-14"),
    );
    let (_, clinic) = h.learn_candidate(
        "clinic note",
        due(
            "Clinic",
            "x",
            "The clinic wants a visit on Sunday.",
            "2026-11-15",
        ),
    );
    h.save_todays_page();
    (rent[0].clone(), clinic[0].clone())
}

fn speak(h: &Harness) -> Value {
    h.ok("speak_page", json!({"date": "2026-11-13"}))
}

#[test]
fn private_line_never_spoken() {
    let voice = Voice::new();
    let h = harness_with_mind(voice.clone());
    let (rent, clinic) = page_with_two_memories(&h);
    h.ok("set_private", json!({"id": clinic, "private": true}));

    let page = speak(&h);
    assert_eq!(page["privateSkipped"], 1);
    let said = voice.said().join(" ");
    assert!(said.contains("Rent is due on Saturday."), "{said}");
    assert!(!said.to_lowercase().contains("clinic"), "{said}");
    let sections = page["sections"].as_array().unwrap();
    assert!(sections.len() >= 2, "{page}");
    assert_eq!(sections[0]["kind"], "opening");
    assert_eq!(sections[0]["startMs"], 0);
    assert!(page["durationMs"].as_u64().unwrap() > 100);
    let spoken_calls = voice.calls.load(Ordering::SeqCst);
    assert_eq!(spoken_calls, sections.len(), "one speak per section");

    let again = speak(&h);
    assert_eq!(again["path"], page["path"], "kept, not made again");
    assert_eq!(
        again["sections"], page["sections"],
        "read back from the file"
    );
    assert_eq!(again["durationMs"], page["durationMs"]);
    assert_eq!(voice.calls.load(Ordering::SeqCst), spoken_calls);

    // Another cited memory becomes private: the kept audio is gone and the next call speaks again.
    h.ok("set_private", json!({"id": rent, "private": true}));
    assert!(
        h.engine()
            .cached_audio("2026-11-13", "bf_emma")
            .unwrap()
            .is_none()
    );
    let third = speak(&h);
    assert_eq!(third["privateSkipped"], 2);
    assert!(voice.calls.load(Ordering::SeqCst) > spoken_calls);
    assert!(
        !voice
            .said()
            .split_off(spoken_calls)
            .join(" ")
            .contains("Rent is due"),
        "the newly private line is not spoken"
    );
}

#[test]
fn two_calls_one_synthesis() {
    let voice = Arc::new(Voice {
        inner: FakeMind::new(),
        said: Mutex::default(),
        calls: AtomicUsize::new(0),
        hook: Mutex::new(None),
        delay: Duration::from_millis(40),
        rate: Box::new(|_| 24_000),
    });
    let h = harness_with_mind(voice.clone());
    page_with_two_memories(&h);
    let app = h.app.handle().clone();
    let day = "2026-11-13".parse().unwrap();
    let (a, b) = tauri::async_runtime::block_on(async {
        tokio::join!(spoken_page(&app, day), spoken_page(&app, day))
    });
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.path, b.path);
    assert_eq!(a.sections, b.sections);
    assert_eq!(
        voice.calls.load(Ordering::SeqCst),
        a.sections.len(),
        "each section was spoken once"
    );
}

#[test]
fn stale_stamp_retries_then_busy() {
    // The page changes while the voice works, once: the audio is refused, made again, kept.
    let voice = Voice::new();
    let h = harness_with_mind(voice.clone());
    let (_, clinic) = page_with_two_memories(&h);
    let engine = h.engine();
    let id = clinic.clone();
    let toggles = Arc::new(AtomicUsize::new(0));
    let t = toggles.clone();
    *voice.hook.lock().unwrap() = Some(Box::new(move |n| {
        if n == 0 {
            engine
                .set_private(&id, t.fetch_add(1, Ordering::SeqCst).is_multiple_of(2))
                .unwrap();
        }
    }));
    let page = speak(&h);
    assert!(page["path"].is_string(), "{page}");
    let first_pass = page["sections"].as_array().unwrap().len();
    assert!(
        voice.calls.load(Ordering::SeqCst) > first_pass,
        "made twice"
    );

    // It changes every time: after one retry the answer is busy and nothing is kept.
    let voice = Voice::new();
    let h = harness_with_mind(voice.clone());
    let (_, clinic) = page_with_two_memories(&h);
    let engine = h.engine();
    let toggles = Arc::new(AtomicUsize::new(0));
    *voice.hook.lock().unwrap() = Some(Box::new(move |_| {
        engine
            .set_private(
                &clinic,
                toggles.fetch_add(1, Ordering::SeqCst).is_multiple_of(2),
            )
            .unwrap();
    }));
    assert_eq!(h.code("speak_page", json!({"date": "2026-11-13"})), "busy");
    assert!(
        h.engine()
            .cached_audio("2026-11-13", "bf_emma")
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_voice_that_changes_format_is_unavailable_not_a_crash() {
    let voice = Arc::new(Voice {
        inner: FakeMind::new(),
        said: Mutex::default(),
        calls: AtomicUsize::new(0),
        hook: Mutex::new(None),
        delay: Duration::ZERO,
        rate: Box::new(|n| if n == 0 { 24_000 } else { 16_000 }),
    });
    let h = harness_with_mind(voice);
    page_with_two_memories(&h);
    assert_eq!(
        h.code("speak_page", json!({"date": "2026-11-13"})),
        "unavailable"
    );
}

#[test]
fn a_kept_file_from_an_older_version_is_made_again() {
    let voice = Voice::new();
    let h = harness_with_mind(voice.clone());
    page_with_two_memories(&h);
    let e = h.engine();
    let stamp = e.audio_stamp("2026-11-13");
    e.cache_audio("2026-11-13", "bf_emma", b"plain old text", stamp)
        .unwrap()
        .unwrap();
    let page = speak(&h);
    assert!(!page["sections"].as_array().unwrap().is_empty());
    assert!(voice.calls.load(Ordering::SeqCst) > 0);
}

#[test]
fn the_page_is_made_ahead_only_when_it_has_been_used_and_nothing_is_going_on() {
    let voice = Voice::new();
    let h = harness_with_mind(voice.clone());
    page_with_two_memories(&h);
    h.app.manage(OsState::new(h.dir.path()));
    let app = h.app.handle().clone();
    let tried = || tauri::async_runtime::block_on(prepare_if_due(&app));

    assert!(!tried(), "never listened to: nothing is made ahead");
    h.app
        .state::<OsState>()
        .update(|f| f.audio_used = Some("2026-11-12".into()));
    h.ok("pause", json!({"until": null}));
    assert!(!tried(), "paused");
    assert_eq!(voice.calls.load(Ordering::SeqCst), 0);
    h.ok("resume", json!({}));
    assert!(tried());
    assert!(
        h.engine()
            .cached_audio("2026-11-13", "bf_emma")
            .unwrap()
            .is_some()
    );
    let calls = voice.calls.load(Ordering::SeqCst);
    assert!(!tried(), "already made");
    assert_eq!(voice.calls.load(Ordering::SeqCst), calls);
}

#[test]
fn speaking_the_page_notes_that_the_person_listens() {
    let h = harness_with_mind(Voice::new());
    h.app.manage(OsState::new(h.dir.path()));
    page_with_two_memories(&h);
    speak(&h);
    assert_eq!(
        h.app.state::<OsState>().file().audio_used.as_deref(),
        Some("2026-11-13")
    );
}

#[test]
fn a_page_that_is_not_there_is_not_found() {
    let h = harness_with_mind(Voice::new());
    page_with_two_memories(&h);
    assert_eq!(
        h.code("speak_page", json!({"date": "2026-01-01"})),
        "notFound"
    );
    assert_eq!(h.code("speak_page", json!({"date": "soon"})), "invalid");
    let quiet = harness_with_mind(Voice::new());
    assert_eq!(
        quiet.code("speak_page", json!({"date": "2026-11-13"})),
        "notFound"
    );
}

#[test]
fn spoken_pages_are_kept_as_wav_files() {
    let h = harness_with_mind(Voice::new());
    page_with_two_memories(&h);
    let page = speak(&h);
    assert!(page["path"].as_str().unwrap().ends_with(".wav"), "{page}");
}

#[test]
fn old_spoken_pages_are_pruned_and_recent_ones_stay() {
    let h = harness_with_mind(Voice::new());
    let e = h.engine();
    let keep = |date: &str| {
        let stamp = e.audio_stamp(date);
        e.cache_audio(date, "bf_emma", &wav(24_000, 100), stamp)
            .unwrap()
            .unwrap()
    };
    let old = keep("2026-11-01");
    let edge = keep("2026-11-06");
    let recent = keep("2026-11-12");
    let today = keep("2026-11-13");
    assert_eq!(
        e.prune_audio(7).unwrap(),
        1,
        "only the day before the window goes"
    );
    assert!(!old.exists());
    assert!(e.cached_audio("2026-11-01", "bf_emma").unwrap().is_none());
    for kept in [&edge, &recent, &today] {
        assert!(kept.exists());
    }
    assert_eq!(e.prune_audio(7).unwrap(), 0, "nothing left to prune");
}

#[test]
fn the_background_check_prunes_old_pages() {
    let h = harness_with_mind(Voice::new());
    let e = h.engine();
    let stamp = e.audio_stamp("2026-10-01");
    let old = e
        .cache_audio("2026-10-01", "bf_emma", &wav(24_000, 100), stamp)
        .unwrap()
        .unwrap();
    h.app.manage(OsState::new(h.dir.path()));
    let app = h.app.handle().clone();
    tauri::async_runtime::block_on(prepare_if_due(&app));
    assert!(!old.exists());
}
