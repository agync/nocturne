//! Help with Nocturne from the ask box: the guide that ships inside the app, the names it gives
//! Nocturne's controls, and which sizes of computer may write help answers.
//!
//! The guide is built from `docs/guide` at this commit, so it always describes the version
//! installed. It is never fetched, kept as a memory, exported or backed up, and which help was
//! shown is not counted or logged. See `docs/design/in-app-help.md`.

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

/// Nocturne's screens and controls as the guide names them in bold, each with the copy key of
/// the words it shows. Bold words not listed here stay plain bold: a run-in heading, or another
/// app's control. A test holds the words to the copy deck, so a renamed control fails the build.
pub const CONTROLS: &[(&str, &str)] = &[
    ("Settings", "screen.settings.title"),
    ("Privacy", "screen.privacy.title"),
    ("Advanced details", "screen.advanced.title"),
    ("Away from home", "away.row.label"),
    ("Online answers", "online.row.label"),
    ("Set up…", "away.off.setUp"),
    ("Turn on", "away.off.turnOn"),
    ("Add a phone", "away.phones.add"),
    ("Allow", "away.pair.allow"),
    ("Don’t allow", "away.pair.deny"),
    ("Scan the new code", "phone.address.scan"),
    ("See phones", "away.phones.see"),
    ("Remove", "away.phones.remove"),
    ("Turn off by itself", "away.autoOff.label"),
    ("Sent to your phones", "privacy.phones.label"),
    ("Open Advanced details", "online.service.open"),
    ("Check", "advanced.online.connect.check"),
    ("Questions from your phones", "online.row.phones.label"),
    ("Turn on online answers", "online.consent.turnOn"),
    ("Choose what runs each job", "models.row.label"),
    (
        "Answer on this computer instead",
        "ask.online.waiting.button",
    ),
    ("This computer", "ask.where.computer"),
    ("Try again", "online.problem.action"),
    ("See what was sent", "ask.mark.seeSent"),
    ("Turn off", "online.turnOff"),
    ("Turn off and forget the key…", "online.turnOffForget"),
    ("Forget the key…", "online.forgetKey"),
    ("Open folder", "privacy.here.open"),
    ("Today", "rail.today"),
    ("Past pages", "rail.pages"),
    ("What Nocturne remembers", "rail.memories"),
    ("Recently kept", "rail.recent"),
    ("Export to a folder…", "privacy.export.button"),
    ("Make a backup…", "privacy.backup.make"),
    ("Restore from a backup", "setup.welcome.restore"),
    ("Restore from a backup…", "privacy.backup.restore"),
    ("Replace with the backup", "restore.confirm.replace"),
    ("Keep what’s here", "restore.confirm.keep"),
    ("Bring it back…", "privacy.backup.putAside.bringBack"),
    ("Delete it…", "privacy.backup.putAside.delete"),
    ("Letting go", "settings.letting.label"),
    ("Fading & let go", "rail.fading"),
    ("Bring back", "button.bringBack"),
    ("Delete…", "memory.action.forget"),
    ("Delete this…", "ask.forget"),
    ("Delete everything…", "privacy.delete.button"),
    ("Start with this computer", "settings.login.label"),
    ("Quick keep shortcut", "settings.shortcut.label"),
    ("Set it to run:", "settings.shortcut.command"),
    ("Morning page", "settings.page.label"),
    ("Tell me when it’s ready", "settings.delivery.notify"),
    ("Don’t interrupt me", "settings.delivery.quiet"),
    ("Nocturne inbox folder", "settings.inbox.label"),
    ("Choose inbox folder…", "settings.inbox.choose"),
    ("Open sound settings", "notice.micDenied.action"),
];

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

/// Whether bold words in the guide name one of Nocturne's controls.
pub fn is_control(words: &str) -> bool {
    CONTROLS.iter().any(|(w, _)| same_words(w, words))
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
    Guide::new(&sources, &is_control, NAMES).expect("the bundled guide reads; a test checks it")
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

    /// The guide's vectors for the finding model `model`. They are made once per model, the
    /// first time a question needs them, and again after the person changes that model. A mind
    /// that cannot make them leaves the guide to be found by its words.
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
    use std::collections::HashMap;

    use nocturne_contract::{GuideBlock, GuideInline};

    use super::*;

    fn deck() -> HashMap<String, String> {
        let mut out = HashMap::new();
        for file in [
            include_str!("../../ui/src/copy/en-GB/advanced.json"),
            include_str!("../../ui/src/copy/en-GB/ask.json"),
            include_str!("../../ui/src/copy/en-GB/away.json"),
            include_str!("../../ui/src/copy/en-GB/backup.json"),
            include_str!("../../ui/src/copy/en-GB/capture.json"),
            include_str!("../../ui/src/copy/en-GB/common.json"),
            include_str!("../../ui/src/copy/en-GB/editions.json"),
            include_str!("../../ui/src/copy/en-GB/fading.json"),
            include_str!("../../ui/src/copy/en-GB/memories.json"),
            include_str!("../../ui/src/copy/en-GB/models.json"),
            include_str!("../../ui/src/copy/en-GB/needs-you.json"),
            include_str!("../../ui/src/copy/en-GB/notice.json"),
            include_str!("../../ui/src/copy/en-GB/online.json"),
            include_str!("../../ui/src/copy/en-GB/page.json"),
            include_str!("../../ui/src/copy/en-GB/phone.json"),
            include_str!("../../ui/src/copy/en-GB/privacy.json"),
            include_str!("../../ui/src/copy/en-GB/recent.json"),
            include_str!("../../ui/src/copy/en-GB/sent.json"),
            include_str!("../../ui/src/copy/en-GB/settings.json"),
            include_str!("../../ui/src/copy/en-GB/setup.json"),
            include_str!("../../ui/src/copy/en-GB/shell.json"),
        ] {
            let map: HashMap<String, String> = serde_json::from_str(file).unwrap();
            out.extend(map);
        }
        out
    }

    #[test]
    fn every_control_the_guide_names_is_spelled_as_the_control_says() {
        let deck = deck();
        for (words, key) in CONTROLS {
            let said = deck
                .get(*key)
                .unwrap_or_else(|| panic!("{key} is not in the copy deck"));
            assert!(
                same_words(words, said),
                "{key}: \"{words}\" but the app says \"{said}\""
            );
        }
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
