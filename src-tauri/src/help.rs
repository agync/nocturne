//! Help with Nocturne from the ask box: the guide that ships inside the app, the names it gives
//! Nocturne's controls, and which sizes of computer may write help answers.
//!
//! The guide is built from `docs/guide` at this commit, so it always describes the version
//! installed. It is never fetched, kept as a memory, exported or backed up, and which help was
//! shown is not counted or logged. See `docs/design/in-app-help.md`.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use nocturne_contract::{ComponentRole, GuideView, HelpKind, HelpView, Tier};
use nocturne_engine::guide::{Confidence, GuideMatch};
use nocturne_engine::{Engine, Guide, GuideSource, Mind};

use crate::state::AppState;

/// The version the guide came with.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The guide pages help may use, in the order the reader lists them. Each page lists the ways
/// people ask for its sections in its front matter; only those sections are taken, and only
/// from a page marked `status: stable`.
const PAGES: [(&str, &str); 6] = [
    ("index", include_str!("../../docs/guide/index.md")),
    ("desktops", include_str!("../../docs/guide/desktops.md")),
    (
        "away-from-home",
        include_str!("../../docs/guide/away-from-home.md"),
    ),
    (
        "online-answers",
        include_str!("../../docs/guide/online-answers.md"),
    ),
    ("your-data", include_str!("../../docs/guide/your-data.md")),
    ("install", include_str!("../../docs/guide/install.md")),
];

/// The ways people ask for the sections of the guide's index page, which may not carry front
/// matter of its own (OKF keeps index pages free of it).
const INDEX_ASKED: &[(&str, &[&str])] = &[
    (
        "What Nocturne does",
        &[
            "what does nocturne do",
            "what is nocturne",
            "how does nocturne work",
            "what is nocturne for",
        ],
    ),
    (
        "What this version does not do",
        &[
            "can it remind me",
            "send me reminders",
            "print by itself",
            "other languages",
            "speak french",
            "password-locked pdf",
            "keep anyway",
            "signed by apple",
        ],
    ),
];

/// The control chips the interface draws, by id, each with the copy key of its control's words.
/// One list for both: a screen or control the guide names in bold is drawn as the chip whose
/// control shows exactly those words. Bold words that name no chip stay plain bold: a run-in
/// heading, or another app's control.
const CHIPS: &str = include_str!("../../ui/src/components/ControlChip/chips.json");

/// The interface's copy deck, for the words of each chip's control.
const DECK: [&str; 36] = [
    include_str!("../../ui/src/copy/en-GB/advanced.json"),
    include_str!("../../ui/src/copy/en-GB/ask.json"),
    include_str!("../../ui/src/copy/en-GB/away.json"),
    include_str!("../../ui/src/copy/en-GB/backup.json"),
    include_str!("../../ui/src/copy/en-GB/capture.json"),
    include_str!("../../ui/src/copy/en-GB/common.json"),
    include_str!("../../ui/src/copy/en-GB/editions.json"),
    include_str!("../../ui/src/copy/en-GB/fading.json"),
    include_str!("../../ui/src/copy/en-GB/feelings.json"),
    include_str!("../../ui/src/copy/en-GB/guide.json"),
    include_str!("../../ui/src/copy/en-GB/health.json"),
    include_str!("../../ui/src/copy/en-GB/journal.json"),
    include_str!("../../ui/src/copy/en-GB/journal-checkin.json"),
    include_str!("../../ui/src/copy/en-GB/journal-day.json"),
    include_str!("../../ui/src/copy/en-GB/journal-delete.json"),
    include_str!("../../ui/src/copy/en-GB/journal-lookback.json"),
    include_str!("../../ui/src/copy/en-GB/journal-question.json"),
    include_str!("../../ui/src/copy/en-GB/memories.json"),
    include_str!("../../ui/src/copy/en-GB/models.json"),
    include_str!("../../ui/src/copy/en-GB/moving.json"),
    include_str!("../../ui/src/copy/en-GB/needs-you.json"),
    include_str!("../../ui/src/copy/en-GB/notice.json"),
    include_str!("../../ui/src/copy/en-GB/online.json"),
    include_str!("../../ui/src/copy/en-GB/page.json"),
    include_str!("../../ui/src/copy/en-GB/phone.json"),
    include_str!("../../ui/src/copy/en-GB/phone-alone.json"),
    include_str!("../../ui/src/copy/en-GB/phone-app.json"),
    include_str!("../../ui/src/copy/en-GB/phone-health.json"),
    include_str!("../../ui/src/copy/en-GB/phone-native.json"),
    include_str!("../../ui/src/copy/en-GB/phones.json"),
    include_str!("../../ui/src/copy/en-GB/privacy.json"),
    include_str!("../../ui/src/copy/en-GB/recent.json"),
    include_str!("../../ui/src/copy/en-GB/sent.json"),
    include_str!("../../ui/src/copy/en-GB/settings.json"),
    include_str!("../../ui/src/copy/en-GB/setup.json"),
    include_str!("../../ui/src/copy/en-GB/shell.json"),
];

/// Each chip with the words its control shows.
static CHIP_WORDS: LazyLock<Vec<(String, String)>> = LazyLock::new(|| {
    let mut deck: HashMap<String, String> = HashMap::new();
    for file in DECK {
        deck.extend(
            serde_json::from_str::<HashMap<String, String>>(file)
                .expect("the copy deck reads; a test checks it"),
        );
    }
    let chips: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(CHIPS).expect("the chip list reads; a test checks it");
    chips
        .into_iter()
        .filter_map(|(id, spec)| {
            let words = deck.get(spec["copy"].as_str()?)?.clone();
            Some((id, words))
        })
        .collect()
});

/// Names that make a question read as being about Nocturne.
const NAMES: &[&str] = &[
    "away from home",
    "online answers",
    "settings",
    "privacy",
    "fading & let go",
    "what nocturne remembers",
    "recently kept",
    "past pages",
    "quick keep",
    "advanced details",
    "the guide",
    "morning page",
    "my page",
    "the app",
];

/// The sizes of computer whose preset answering model passed the help check (the evaluation
/// set in `docs/design/in-app-help.md`, three runs each, every check passed). A size that is not
/// here shows the guide's own words instead of a written help answer. None has passed yet: the
/// release checklist records each run.
pub const WRITES_HELP: &[Tier] = &[];

/// Words in the guide's bold match the copy deck with either apostrophe.
fn same_words(a: &str, b: &str) -> bool {
    a.replace('\'', "’") == b.replace('\'', "’")
}

/// The chip that bold words in the guide name, if they name one of Nocturne's controls.
pub fn chip_of(words: &str) -> Option<String> {
    CHIP_WORDS
        .iter()
        .find(|(_, w)| same_words(w, words))
        .map(|(id, _)| id.clone())
}

/// The words of every control a chip names.
pub fn control_words() -> Vec<String> {
    CHIP_WORDS.iter().map(|(_, w)| w.clone()).collect()
}

static GUIDE: LazyLock<Guide> = LazyLock::new(|| {
    let sources: Vec<GuideSource<'_>> = PAGES
        .iter()
        .map(|(slug, markdown)| GuideSource {
            slug,
            markdown,
            asked: (*slug == "index").then_some(INDEX_ASKED),
        })
        .collect();
    Guide::new(&sources, &chip_of, NAMES).expect("the bundled guide reads; a test checks it")
});

/// The guide as it ships with this version.
pub fn guide() -> &'static Guide {
    &GUIDE
}

/// The guide for the reader.
pub fn view() -> GuideView {
    GuideView {
        version: VERSION.to_string(),
        pages: guide().pages().to_vec(),
    }
}

/// Whether a size of computer writes help answers: it passed the check with its preset answering
/// model. A model the person chose themselves has not been through the check.
pub fn writes_help(tier: Tier, own_answering_model: bool) -> bool {
    !own_answering_model && WRITES_HELP.contains(&tier)
}

/// The meaning vectors of the guide, made with the finding model in use, and the override of the
/// check for tests and the help check run.
#[derive(Default)]
pub struct HelpStore {
    vectors: tokio::sync::Mutex<Option<(String, Vec<Vec<f32>>)>>,
    writes: Mutex<Option<bool>>,
}

impl HelpStore {
    /// Set whether help answers are written regardless of the size of computer: for tests and
    /// for the help check itself. `None` goes back to [`WRITES_HELP`].
    pub fn set_writes(&self, writes: Option<bool>) {
        *self.writes.lock().unwrap_or_else(|e| e.into_inner()) = writes;
    }

    fn writes(&self) -> Option<bool> {
        *self.writes.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The guide's vectors for the finding model `model`. They are made once per model, when
    /// the mind is ready ([`warm`]) or else the first time a question needs them. A mind that
    /// cannot make them leaves the guide to be found by its words, and they are tried again.
    async fn vectors(&self, mind: &dyn Mind, model: &str) -> Option<Vec<Vec<f32>>> {
        let mut held = self.vectors.lock().await;
        if let Some((m, v)) = held.as_ref()
            && m == model
        {
            return Some(v.clone());
        }
        let texts = guide().texts();
        match mind.embed(&texts).await {
            Ok(v) if v.len() == texts.len() => {
                *held = Some((model.to_string(), v.clone()));
                Some(v)
            }
            Ok(_) | Err(_) => {
                tracing::info!("the guide is found by its words only for now");
                None
            }
        }
    }
}

/// Read the guide for meaning with the mind that is now in the slot and its finding model, so
/// the first question does not wait for it. Called whenever a mind is ready, which is also after
/// the person changes the finding model.
pub fn warm<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let (Some(mind), Ok(engine)) = (state.mind.get(), state.engine()) else {
            return;
        };
        state
            .help
            .vectors(mind.as_ref(), &engine.recall_model())
            .await;
    });
}

/// What the guide brings to one question.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub kind: HelpKind,
    pub ids: Vec<String>,
}

/// Whether this computer writes help answers now: the override, else the size of computer and
/// whether the person chose their own answering model.
pub fn writes_now(state: &AppState) -> bool {
    if let Some(w) = state.help.writes() {
        return w;
    }
    let s = nocturne_lemonade::selection::read(&crate::models::config_dir(&state.data_dir));
    writes_help(s.tier, s.custom.contains_key(&ComponentRole::Mind))
}

/// Match `question` against the guide. `asked` is set when the person pressed "Show the guide's
/// answer": the nearest sections answer, whatever the match.
pub async fn find(
    state: &AppState,
    mind: &dyn Mind,
    engine: &Engine,
    question: &str,
    query: Option<&[f32]>,
    asked: bool,
) -> Option<Found> {
    let vectors = match query {
        Some(_) => state.help.vectors(mind, &engine.recall_model()).await,
        None => None,
    };
    let vectors = vectors.as_deref();
    let found: GuideMatch = if asked {
        guide().find_for_help(question, query, vectors)?
    } else {
        guide().find(question, query, vectors)?
    };
    let kind = match found.confidence {
        Confidence::Unsure => HelpKind::Unsure,
        Confidence::Confident if writes_now(state) => HelpKind::Written,
        Confidence::Confident => HelpKind::Words,
    };
    Some(Found {
        kind,
        ids: found.ids,
    })
}

/// The help part of an answer's terminal delta, before the answer has cited anything.
pub fn help_view(found: &Found, online_on: bool) -> HelpView {
    let g = guide();
    let sections: Vec<_> = found.ids.iter().filter_map(|id| g.cite(id)).collect();
    let opening = found
        .ids
        .first()
        .and_then(|id| g.section(id))
        .map(|(page, _)| page.opening.clone())
        .unwrap_or_default();
    HelpView {
        kind: found.kind,
        sections,
        opening,
        cited: Vec::new(),
        online_on,
    }
}

#[cfg(test)]
mod tests {
    use nocturne_contract::{GuideBlock, GuideInline};

    use super::*;

    /// Every chip names a control the copy deck has, and the deck the app reads is the whole
    /// of `ui/src/copy/en-GB`.
    #[test]
    fn every_chip_names_a_control_of_the_deck() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/src/copy/en-GB");
        let files = std::fs::read_dir(dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .is_ok_and(|e| e.path().extension().is_some_and(|x| x == "json"))
            })
            .count();
        assert_eq!(files, DECK.len(), "a copy file is missing from DECK");
        let chips: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(CHIPS).unwrap();
        assert_eq!(
            CHIP_WORDS.len(),
            chips.len(),
            "a chip names a key the deck lacks"
        );
        assert_eq!(chip_of("Add a phone").as_deref(), Some("addAPhone"));
        assert_eq!(chip_of("Don't allow").as_deref(), Some("dontAllow"));
        assert_eq!(chip_of("Make a backup."), None);
    }

    /// Bold words in a section are a control from the list, a run-in heading or stress (they end
    /// a sentence), or one of the few words of other apps the guide names.
    #[test]
    fn bold_words_in_the_guide_are_known() {
        const OTHERS: &[&str] = &[];
        let mut unknown = Vec::new();
        for page in guide().pages() {
            for section in &page.sections {
                for block in &section.blocks {
                    let runs: Vec<&Vec<GuideInline>> = match block {
                        GuideBlock::Paragraph { parts } => vec![parts],
                        GuideBlock::Steps { items } | GuideBlock::List { items } => {
                            items.iter().collect()
                        }
                        GuideBlock::Table { rows } => rows.iter().flatten().collect(),
                    };
                    for part in runs.into_iter().flatten() {
                        if let GuideInline::Strong { text } = part
                            && !text.ends_with(['.', '!', '?'])
                            && !OTHERS.contains(&text.as_str())
                        {
                            unknown.push(format!("{}#{}: {text}", page.slug, section.slug));
                        }
                    }
                }
            }
        }
        assert!(unknown.is_empty(), "not a known control: {unknown:#?}");
    }

    #[test]
    fn the_bundled_guide_holds_the_sections_in_scope() {
        let g = guide();
        let pages: Vec<&str> = g.pages().iter().map(|p| p.slug.as_str()).collect();
        assert_eq!(
            pages,
            [
                "index",
                "desktops",
                "away-from-home",
                "online-answers",
                "your-data",
                "install"
            ]
        );
        let ids = g.ids();
        for id in [
            "guide:index#what-nocturne-does",
            "guide:away-from-home#add-a-phone",
            "guide:online-answers#turn-it-off-and-forget-the-key",
            "guide:your-data#let-go-bring-back-forget",
            "guide:install#take-nocturne-off",
        ] {
            assert!(ids.contains(&id.to_string()), "{id}");
        }
        for out in [
            "guide:install#choose-a-package",
            "guide:install#run-it-as-a-server",
        ] {
            assert!(!ids.contains(&out.to_string()), "{out} is out of scope");
        }
        assert!(
            !g.pages()
                .iter()
                .any(|p| p.slug == "npu" || p.slug == "choosing-models")
        );
        assert!(g.pages().iter().all(|p| !p.opening.is_empty()));
    }

    #[test]
    fn the_three_known_drifts_are_fixed() {
        let text = |id: &str| guide().fact(id).unwrap().memory.sentence;
        let on = text("guide:away-from-home#turn-it-on");
        assert!(
            on.contains("Set up…") && !on.contains("and turn it on."),
            "{on}"
        );
        let address = text("guide:away-from-home#when-the-address-changes");
        assert!(address.contains("Scan the new code"), "{address}");
        let forget = text("guide:your-data#let-go-bring-back-forget");
        assert!(
            forget.contains("Delete this…") && !forget.contains("Forget removes"),
            "{forget}"
        );
    }

    /// The interface's mock shows the same guide. Its copy is written from the bundle: run with
    /// `UPDATE_GUIDE_FIXTURE=1` after changing the guide.
    #[test]
    fn the_mock_guide_is_the_bundled_guide() {
        let g = guide();
        let asked: serde_json::Map<String, serde_json::Value> = g
            .ids()
            .into_iter()
            .map(|id| {
                let ways = serde_json::json!(g.asked(&id));
                (id, ways)
            })
            .collect();
        let fixture = serde_json::json!({ "pages": g.pages(), "asked": asked });
        let want = format!("{}\n", serde_json::to_string_pretty(&fixture).unwrap());
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../ui/src/engine/mock/seed/guide.json"
        );
        if std::env::var_os("UPDATE_GUIDE_FIXTURE").is_some() {
            std::fs::write(path, &want).unwrap();
        }
        let have = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            have == want,
            "ui/src/engine/mock/seed/guide.json is out of date: run UPDATE_GUIDE_FIXTURE=1 cargo test -p nocturne-app --lib help"
        );
    }

    /// A way of asking of one word would make every question with that word help.
    #[test]
    fn every_way_of_asking_has_two_words_or_more() {
        let g = guide();
        for id in g.ids() {
            for way in g.asked(&id) {
                assert!(way.split_whitespace().count() >= 2, "{id}: \"{way}\"");
            }
        }
    }

    #[test]
    fn only_a_size_that_passed_with_its_own_model_writes_help() {
        for tier in [
            Tier::Tiny,
            Tier::Small,
            Tier::Standard,
            Tier::Large,
            Tier::Unsupported,
        ] {
            assert_eq!(writes_help(tier, false), WRITES_HELP.contains(&tier));
            assert!(!writes_help(tier, true), "a chosen model never writes help");
        }
    }
}
