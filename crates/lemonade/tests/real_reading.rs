//! One reading by a real mind through the real `lemond`, to prove the reading grammar on the
//! pinned engine. Ignored unless `NOCTURNE_LEMOND` names a bundle and `NOCTURNE_E2E_DIR` a
//! scratch folder whose `data/engine` already holds the Tiny models:
//!
//! ```text
//! NOCTURNE_LEMOND=vendor/lemonade/linux-x86_64 NOCTURNE_E2E_DIR=/path/to/scratch \
//!     cargo test -p nocturne-lemonade --test real_reading -- --ignored --nocapture
//! ```
//!
//! The models and the engine's backends are linked in read-only; everything the engine writes
//! goes to a fresh folder under the build directory. Offline: nothing is pulled.
// The models are linked in with Unix symlinks, so the test is built for Unix only.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use nocturne_contract::{EntityKind, ItemKind, Tier};
use nocturne_engine::{ExtractContext, Mind};
use nocturne_lemonade::{LemonadeMind, Supervisor, SupervisorOptions, selection};

const NOTE: &str = "Boiler service is booked for Monday 30 November, between 8 and 12. The engineer is called Marek.\nThe plumber, Mister Hughes, is coming on Friday the twentieth of November at nine in the morning to fix the kitchen tap.";

const KEYS: [&str; 12] = [
    "span",
    "sentence",
    "subject",
    "subjectKind",
    "predicate",
    "object",
    "due",
    "validFrom",
    "validTo",
    "moved",
    "confidence",
    "tags",
];

fn path_from(var: &str) -> PathBuf {
    let given = PathBuf::from(std::env::var(var).unwrap_or_else(|_| panic!("set {var}")));
    if given.is_absolute() || given.exists() {
        return given;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(given)
}

fn link(from: &Path, to: &Path) {
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(from, to).unwrap();
}

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

#[tokio::test]
#[ignore = "needs NOCTURNE_LEMOND and NOCTURNE_E2E_DIR"]
async fn a_tiny_mind_reads_by_the_grammar_in_reading_order() {
    let shared = path_from("NOCTURNE_E2E_DIR").join("data/engine");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/real-reading");
    let _ = std::fs::remove_dir_all(&root);
    link(&shared.join("models"), &root.join("models"));
    link(&shared.join("cache/bin"), &root.join("cache/bin"));
    let dirs = nocturne_lemonade::Dirs::new(path_from("NOCTURNE_LEMOND"), &root);
    let sup = Supervisor::start(dirs, SupervisorOptions::default())
        .await
        .unwrap();

    let log = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(log.clone())
        .with_ansi(false)
        .with_env_filter(tracing_subscriber::EnvFilter::new(
            "info,nocturne::reply=trace",
        ))
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let picks = selection::picks(&selection::Selection::preset(Tier::Tiny));
    let mind = LemonadeMind::new(sup.client(), picks, Tier::Tiny);
    let cx = ExtractContext {
        item_kind: ItemKind::Text,
        kept_at: "2026-11-14T09:30:00Z".parse().unwrap(),
        today: "2026-11-14".parse().unwrap(),
        known_entities: vec![("Sam".into(), EntityKind::Person)],
    };
    let started = std::time::Instant::now();
    let found = mind.extract(NOTE, &cx).await;
    let took = started.elapsed();
    let lines = vec!["The engineer is called Marek.".to_string()];
    let second = mind.extract_lines(NOTE, &cx, &lines).await;
    sup.shutdown().await.unwrap();

    let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    println!("{text}");
    println!(
        "reading took {took:?} and {} requests",
        mind.reading_calls()
    );
    let found = found.expect("the reading succeeds");
    println!("memories: {found:#?}");
    println!("second look: {second:#?}");
    assert!(!found.is_empty(), "the note holds facts");
    assert!(second.is_ok(), "the second look succeeds");
    assert!(
        !text.contains("refused the reading grammar"),
        "the engine takes the grammar"
    );

    let replies: Vec<&str> = text.split("nocturne::reply").skip(1).collect();
    assert!(!replies.is_empty(), "the raw replies are traced");
    for line in replies {
        let at: Vec<usize> = KEYS
            .iter()
            .filter_map(|k| line.find(&format!("\"{k}\"")))
            .collect();
        assert_eq!(at.len(), KEYS.len(), "every key: {line}");
        assert!(at.windows(2).all(|w| w[0] < w[1]), "reading order: {line}");
    }
}
