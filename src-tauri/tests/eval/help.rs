//! The help check: whether a size of computer may write help answers. The questions and the
//! pass criteria are the ones in `docs/design/in-app-help.md` ("Blending on small computers").
//! A size passes only when every check holds in every run; until then it shows the guide's own
//! words. The run on real models goes through `e2e_real.rs` (`NOCTURNE_E2E_HELP=<runs>`); the
//! fixture test runs it with a scripted mind.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::{Value, json};

/// How many times each question is asked.
pub const RUNS: usize = 3;

/// A how-to question and the guide sections that answer it (any one of them is right).
pub struct HelpQuestion {
    pub id: &'static str,
    pub text: &'static str,
    pub sections: &'static [&'static str],
}

pub const HELP_QUESTIONS: &[HelpQuestion] = &[
    HelpQuestion {
        id: "h01",
        text: "How do I get Nocturne on my phone?",
        sections: &[
            "guide:away-from-home#turn-it-on",
            "guide:away-from-home#add-a-phone",
        ],
    },
    HelpQuestion {
        id: "h02",
        text: "Can I use Nocturne on my phone when I'm out?",
        sections: &[
            "guide:away-from-home#what-the-phone-can-do",
            "guide:away-from-home#turn-it-on",
        ],
    },
    HelpQuestion {
        id: "h03",
        text: "I've lost my phone. What do I do?",
        sections: &["guide:away-from-home#remove-a-lost-phone"],
    },
    HelpQuestion {
        id: "h04",
        text: "My phone says it can't connect any more",
        sections: &["guide:away-from-home#when-the-address-changes"],
    },
    HelpQuestion {
        id: "h05",
        text: "How do I keep something without opening the window?",
        sections: &["guide:desktops#quick-keep-shortcut"],
    },
    HelpQuestion {
        id: "h06",
        text: "Where has the little icon by the clock gone?",
        sections: &["guide:desktops#the-tray"],
    },
    HelpQuestion {
        id: "h07",
        text: "Can it tell me when my page is ready?",
        sections: &["guide:desktops#notifications"],
    },
    HelpQuestion {
        id: "h08",
        text: "How do I make it start when I turn the computer on?",
        sections: &["guide:desktops#start-with-this-computer"],
    },
    HelpQuestion {
        id: "h09",
        text: "Can it pick up everything I save in Downloads?",
        sections: &["guide:desktops#a-folder-that-keeps-what-you-drop-in-it"],
    },
    HelpQuestion {
        id: "h10",
        text: "Why can't it hear me?",
        sections: &["guide:desktops#sound"],
    },
    HelpQuestion {
        id: "h11",
        text: "How do I move everything to my new laptop?",
        sections: &["guide:your-data#back-up-and-restore"],
    },
    HelpQuestion {
        id: "h12",
        text: "How do I get rid of a photo I kept?",
        sections: &["guide:your-data#let-go-bring-back-forget"],
    },
    HelpQuestion {
        id: "h13",
        text: "A note I kept has disappeared",
        sections: &["guide:your-data#let-go-bring-back-forget"],
    },
    HelpQuestion {
        id: "h14",
        text: "Does anything I keep go on the internet?",
        sections: &["guide:your-data#how-you-can-tell-nothing-leaves-your-computer"],
    },
    HelpQuestion {
        id: "h15",
        text: "Can it use the AI I already pay for?",
        sections: &["guide:online-answers#turn-it-on"],
    },
    HelpQuestion {
        id: "h16",
        text: "How do I stop it sending my questions online?",
        sections: &["guide:online-answers#turn-it-off-and-forget-the-key"],
    },
    HelpQuestion {
        id: "h17",
        text: "Can it remind me about the dentist?",
        sections: &["guide:index#what-this-version-does-not-do"],
    },
    HelpQuestion {
        id: "h18",
        text: "How do I take Nocturne off my computer?",
        sections: &["guide:install#take-nocturne-off"],
    },
];

/// Questions that must stay memory questions, because they share words with the guide.
pub const MEMORY_QUESTIONS: &[(&str, &str)] = &[
    ("m01", "What is Gran's new phone number?"),
    ("m02", "When did Leo get his new phone?"),
    ("m03", "Where did I put the spare key?"),
    (
        "m04",
        "What did the plumber say about turning the water off?",
    ),
    ("m05", "How do I get to the new dentist?"),
];

/// A question that needs both the guide and what the person kept.
pub struct MixedQuestion {
    pub id: &'static str,
    pub text: &'static str,
    pub section: &'static str,
    /// The seeded memory it needs.
    pub kept: &'static str,
}

pub const MIXED_QUESTIONS: &[MixedQuestion] = &[
    MixedQuestion {
        id: "x01",
        text: "How do I get rid of the photo of Gran's letter?",
        section: "guide:your-data#let-go-bring-back-forget",
        kept: "gran_letter",
    },
    MixedQuestion {
        id: "x02",
        text: "The note about the boiler has gone. How do I get it back?",
        section: "guide:your-data#let-go-bring-back-forget",
        kept: "boiler",
    },
    MixedQuestion {
        id: "x03",
        text: "Can I see Leo's trip form on my phone when I'm out?",
        section: "guide:away-from-home#what-the-phone-can-do",
        kept: "trip_form",
    },
    MixedQuestion {
        id: "x04",
        text: "Will my phone see Gran's medication list if I mark it private?",
        section: "guide:away-from-home#what-the-phone-can-do",
        kept: "medication",
    },
    MixedQuestion {
        id: "x05",
        text: "How do I move my notes about the house sale to my new laptop?",
        section: "guide:your-data#back-up-and-restore",
        kept: "house_sale",
    },
    MixedQuestion {
        id: "x06",
        text: "Did my question about half term go online?",
        section: "guide:online-answers#what-is-sent",
        kept: "half_term",
    },
];

/// A memory seeded to share words with the guide: key, note, sentence, subject, object.
pub struct Seed {
    pub key: &'static str,
    pub note: &'static str,
    pub sentence: &'static str,
    pub subject: &'static str,
    pub object: &'static str,
}

pub const SEEDS: &[Seed] = &[
    Seed {
        key: "gran_phone",
        note: "Gran's new phone number is 07700 900123.",
        sentence: "Gran's new phone number is 07700 900123.",
        subject: "Gran",
        object: "07700 900123",
    },
    Seed {
        key: "leo_phone",
        note: "Leo got his new phone on Saturday 7 November.",
        sentence: "Leo got his new phone on Saturday 7 November.",
        subject: "Leo",
        object: "new phone",
    },
    Seed {
        key: "spare_key",
        note: "The spare key is in the blue jar on the kitchen shelf.",
        sentence: "The spare key is in the blue jar on the kitchen shelf.",
        subject: "Spare key",
        object: "blue jar",
    },
    Seed {
        key: "plumber",
        note: "The plumber said to turn the water off at the stopcock under the sink before he comes.",
        sentence: "The plumber said to turn the water off at the stopcock under the sink.",
        subject: "Plumber",
        object: "stopcock",
    },
    Seed {
        key: "dentist_address",
        note: "The new dentist is at 14 Mill Lane, next to the library.",
        sentence: "The new dentist is at 14 Mill Lane, next to the library.",
        subject: "Dentist",
        object: "14 Mill Lane",
    },
    Seed {
        key: "gran_letter",
        note: "Photo of Gran's letter about her hospital visit on 2 December.",
        sentence: "Gran's letter is about her hospital visit on 2 December.",
        subject: "Gran's letter",
        object: "hospital visit",
    },
    Seed {
        key: "boiler",
        note: "Boiler note: the engineer Marek comes on Monday 30 November.",
        sentence: "The boiler engineer Marek comes on Monday 30 November.",
        subject: "Boiler",
        object: "Marek",
    },
    Seed {
        key: "trip_form",
        note: "Leo's trip form for the science museum is due back on Friday 20 November.",
        sentence: "Leo's trip form for the science museum is due back on Friday 20 November.",
        subject: "Leo's trip form",
        object: "Friday 20 November",
    },
    Seed {
        key: "medication",
        note: "Gran's medication list: aspirin in the morning, statin at night.",
        sentence: "Gran's medication list is aspirin in the morning and statin at night.",
        subject: "Gran's medication",
        object: "aspirin and statin",
    },
    Seed {
        key: "house_sale",
        note: "House sale notes: the buyers' survey is on 9 December.",
        sentence: "The buyers' survey for the house sale is on 9 December.",
        subject: "House sale",
        object: "9 December",
    },
    Seed {
        key: "half_term",
        note: "Half term: we decided to visit Aunt May in Whitby.",
        sentence: "For half term we decided to visit Aunt May in Whitby.",
        subject: "Half term",
        object: "Whitby",
    },
];

/// One answer as the check reads it.
#[derive(Debug, Clone, Default)]
pub struct HelpAnswer {
    pub phase: String,
    /// `written`, `words`, `unsure`, or empty for no help.
    pub help: String,
    /// The guide sections cited (written) or offered (words, unsure).
    pub sections: Vec<String>,
    pub memory_ids: Vec<String>,
    /// Each sentence of the reply with what it cites, guide sections by id and memories by id.
    pub said: Vec<(String, Vec<String>)>,
    /// Time to the first useful sentence.
    pub first_sentence: Option<Duration>,
}

/// The sentences of a raw reply with their `[mN]` markers, each mapped to the facts it was given:
/// `facts` are the ids of the facts in the order the mind was given them.
pub fn cited_sentences(reply: &str, facts: &[String]) -> Vec<(String, Vec<String>)> {
    /// The fact a marker at the start of `text` names, and the text after it.
    fn marker<'a>(text: &'a str, facts: &[String]) -> Option<(Option<String>, &'a str)> {
        let after = text.strip_prefix("[m")?;
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        let rest = after[digits.len()..].strip_prefix(']')?;
        let n = digits.parse::<usize>().ok()?;
        Some((n.checked_sub(1).and_then(|i| facts.get(i)).cloned(), rest))
    }
    let mut out = Vec::new();
    let mut now = String::new();
    let mut cites: Vec<String> = Vec::new();
    let mut rest = reply;
    while let Some(c) = rest.chars().next() {
        if let Some((id, after)) = marker(rest, facts) {
            cites.extend(id);
            rest = after;
            continue;
        }
        now.push(c);
        rest = &rest[c.len_utf8()..];
        let ends =
            matches!(c, '.' | '!' | '?') && rest.chars().next().is_none_or(char::is_whitespace);
        if ends || rest.is_empty() {
            // Markers just after the full stop belong to this sentence.
            while let Some((id, after)) = marker(rest.trim_start(), facts) {
                cites.extend(id);
                rest = after;
            }
            let sentence = now.replace("**", "").trim().to_string();
            if sentence.chars().any(char::is_alphanumeric) {
                out.push((sentence, std::mem::take(&mut cites)));
            }
            now.clear();
            cites.clear();
        }
    }
    out
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// What the check needs to know besides the answers.
pub struct Sources<'a> {
    /// The text of each guide section, by id.
    pub guide: &'a HashMap<String, String>,
    /// The sentence of each seeded memory, by memory id.
    pub kept: &'a HashMap<String, String>,
    /// Seed key to memory id.
    pub ids: &'a HashMap<&'static str, String>,
    /// The names of Nocturne's controls and screens, as the guide spells them.
    pub controls: &'a [&'a str],
}

/// Words of a memory that the guide never uses: its names, numbers, dates, places and things.
fn own_words(kept: &str, guide: &HashMap<String, String>) -> Vec<String> {
    let all: std::collections::HashSet<String> = guide.values().flat_map(|t| words(t)).collect();
    words(kept)
        .into_iter()
        .filter(|w| w.len() > 2 && !all.contains(w))
        .collect()
}

/// The words with every word capitalised, as a mind that misremembers a control may write them.
fn capitalised(words: &str) -> String {
    words
        .split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// Judge one sentence against what it cites. Returns `(blended, unfaithful, unsettled)`.
fn judge_sentence(sentence: &str, cites: &[String], src: &Sources<'_>) -> (bool, bool, bool) {
    if cites.is_empty() {
        return (false, false, true);
    }
    let said = words(sentence);
    let mut blended = false;
    let mut unfaithful = false;
    for id in cites {
        if let Some(section) = src.guide.get(id) {
            // Nothing of any memory in a sentence that cites the guide.
            blended |= src
                .kept
                .values()
                .any(|k| own_words(k, src.guide).iter().any(|w| said.contains(w)));
            // Every control it names is spelled as the control is, and is in the section.
            let lower = sentence.to_lowercase();
            for control in src.controls {
                let low = control.to_lowercase();
                if low.len() < 6 || !lower.contains(&low) {
                    continue;
                }
                let exact = sentence.contains(control);
                // Plain words that happen to be a control's ("turn off") name nothing.
                let named = exact || sentence.contains(&capitalised(control));
                if named && (!exact || !section.contains(control)) {
                    eprintln!("not faithful: {control:?} in {sentence:?}");
                    unfaithful = true;
                }
            }
        } else if let Some(kept) = src.kept.get(id) {
            // No step, screen or control of the guide in a sentence that cites a memory.
            blended |= src
                .controls
                .iter()
                .filter(|c| c.len() >= 6)
                .any(|c| sentence.contains(c) && !kept.contains(c));
        }
    }
    (blended, unfaithful, false)
}

/// The steps of a section a run of sentences gives must keep the guide's order: the controls
/// they name appear in the section in the same order.
fn keeps_order(said: &[(String, Vec<String>)], src: &Sources<'_>) -> bool {
    for (id, section) in src.guide {
        let mut last = 0usize;
        for (sentence, cites) in said.iter().filter(|(_, c)| c.contains(id)) {
            let _ = cites;
            let mut named: Vec<usize> = src
                .controls
                .iter()
                .filter(|c| c.len() >= 6 && sentence.contains(*c))
                .filter_map(|c| section.find(c))
                .collect();
            named.sort_unstable();
            if let Some(&first) = named.first() {
                if first < last {
                    return false;
                }
                last = named.last().copied().unwrap_or(first);
            }
        }
    }
    true
}

fn percentile(values: &mut [u64], p: f64) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let at = ((p / 100.0) * (values.len() as f64 - 1.0)).round() as usize;
    values.get(at).copied()
}

/// Score `runs` runs of the evaluation set. Each run maps a question id to its answer.
pub fn score(runs: &[HashMap<&'static str, HelpAnswer>], src: &Sources<'_>) -> Value {
    let mut per_run = Vec::new();
    let mut pass = true;
    let mut unsettled: Vec<Value> = Vec::new();
    let mut times: Vec<u64> = Vec::new();
    for (n, run) in runs.iter().enumerate() {
        let get = |id: &str| run.get(id).cloned().unwrap_or_default();
        let finds = HELP_QUESTIONS
            .iter()
            .filter(|q| {
                let a = get(q.id);
                a.help == "written" && a.sections.iter().any(|s| q.sections.contains(&s.as_str()))
            })
            .count();
        let memory: Vec<HelpAnswer> = MEMORY_QUESTIONS.iter().map(|(id, _)| get(id)).collect();
        let confident = memory
            .iter()
            .filter(|a| a.help == "written" || a.help == "words")
            .count();
        let unsure = memory.iter().filter(|a| a.help == "unsure").count();
        let both = MIXED_QUESTIONS
            .iter()
            .filter(|q| {
                let a = get(q.id);
                let kept = src.ids.get(q.kept);
                a.help == "written"
                    && a.sections.iter().any(|s| s == q.section)
                    && kept.is_some_and(|k| a.memory_ids.contains(k))
            })
            .count();
        let (mut blends, mut misses) = (0usize, 0usize);
        for (id, a) in run {
            if a.help != "written" {
                continue;
            }
            for (sentence, cites) in &a.said {
                let (b, u, open) = judge_sentence(sentence, cites, src);
                blends += usize::from(b);
                misses += usize::from(u);
                if open {
                    unsettled.push(json!({"run": n + 1, "id": id, "sentence": sentence}));
                }
            }
            if !keeps_order(&a.said, src) {
                misses += 1;
            }
            if let Some(t) = a.first_sentence {
                times.push(t.as_millis() as u64);
            }
        }
        let checks = json!({
            "findsTheRightSection": {"found": finds, "of": HELP_QUESTIONS.len(), "pass": finds >= 16},
            "noHelpWhereNoneWasAsked": {"confident": confident, "unsure": unsure,
                                        "pass": confident == 0 && unsure <= 1},
            "noBlending": {"sentences": blends, "pass": blends == 0},
            "faithfulSteps": {"misses": misses, "pass": misses == 0},
            "bothHalves": {"found": both, "of": MIXED_QUESTIONS.len(), "pass": both >= 5},
        });
        let all = checks
            .as_object()
            .unwrap()
            .values()
            .all(|c| c["pass"] == true);
        pass &= all;
        per_run.push(json!({"run": n + 1, "checks": checks, "pass": all}));
    }
    let p95 = percentile(&mut times, 95.0);
    let fast = p95.is_none_or(|ms| ms < 3000);
    pass &= fast && !runs.is_empty();
    json!({
        "runs": per_run,
        "speed": {"firstSentenceP95Ms": p95, "pass": fast},
        "unsettled": unsettled,
        "pass": pass,
    })
}

/// The guide's section texts, the seeded memories' sentences and the control names, for
/// [`score`].
pub struct Known {
    pub guide: HashMap<String, String>,
    pub kept: HashMap<String, String>,
    pub controls: Vec<&'static str>,
}

impl Known {
    pub fn new(ids: &HashMap<&'static str, String>) -> Known {
        let guide = nocturne_app_lib::help::guide();
        Known {
            guide: guide
                .ids()
                .into_iter()
                .filter_map(|id| {
                    let text = guide.fact(&id)?.memory.sentence;
                    Some((id, text))
                })
                .collect(),
            kept: SEEDS
                .iter()
                .filter_map(|s| Some((ids.get(s.key)?.clone(), s.sentence.to_string())))
                .collect(),
            controls: nocturne_app_lib::help::CONTROLS
                .iter()
                .map(|(w, _)| *w)
                .collect(),
        }
    }

    pub fn sources<'a>(&'a self, ids: &'a HashMap<&'static str, String>) -> Sources<'a> {
        Sources {
            guide: &self.guide,
            kept: &self.kept,
            ids,
            controls: &self.controls,
        }
    }
}

/// Every question of the evaluation set, help first, then memory, then mixed.
pub fn all_questions() -> Vec<(&'static str, &'static str)> {
    HELP_QUESTIONS
        .iter()
        .map(|q| (q.id, q.text))
        .chain(MEMORY_QUESTIONS.iter().copied())
        .chain(MIXED_QUESTIONS.iter().map(|q| (q.id, q.text)))
        .collect()
}

/// Ask the whole set `runs` times and score it. `ask` answers one question.
pub fn run_check(
    runs: usize,
    ids: &HashMap<&'static str, String>,
    mut ask: impl FnMut(&str) -> HelpAnswer,
) -> Value {
    let mut all = Vec::new();
    for _ in 0..runs {
        let mut run = HashMap::new();
        for (id, text) in all_questions() {
            run.insert(id, ask(text));
        }
        all.push(run);
    }
    let known = Known::new(ids);
    score(&all, &known.sources(ids))
}

/// The help field of a terminal delta as the check reads it: its kind and its sections.
pub fn help_of(help: &Value) -> (String, Vec<String>) {
    let kind = help["kind"].as_str().unwrap_or_default().to_string();
    let sections = help["sections"]
        .as_array()
        .map(|s| {
            s.iter()
                .filter_map(|c| c["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    (kind, sections)
}

/// A mind that hands everything to another and keeps the ids of the facts each answer was
/// given, in order, so the reply's markers can be read against them.
pub struct Recorder {
    inner: std::sync::Arc<dyn nocturne_engine::Mind>,
    facts: std::sync::Mutex<Vec<Vec<String>>>,
}

impl Recorder {
    pub fn new(inner: std::sync::Arc<dyn nocturne_engine::Mind>) -> Recorder {
        Recorder {
            inner,
            facts: std::sync::Mutex::default(),
        }
    }

    /// How many answers it has been asked for.
    pub fn answers(&self) -> usize {
        self.facts.lock().unwrap().len()
    }

    /// The facts of answer `n`, counting from 0.
    pub fn facts_of(&self, n: usize) -> Vec<String> {
        self.facts
            .lock()
            .unwrap()
            .get(n)
            .cloned()
            .unwrap_or_default()
    }
}

impl nocturne_engine::Mind for Recorder {
    fn transcribe<'a>(
        &'a self,
        audio: &'a std::path::Path,
    ) -> nocturne_engine::BoxFuture<
        'a,
        Result<nocturne_engine::Transcript, nocturne_engine::MindError>,
    > {
        self.inner.transcribe(audio)
    }

    fn read_image<'a>(
        &'a self,
        image: &'a std::path::Path,
    ) -> nocturne_engine::BoxFuture<
        'a,
        Result<nocturne_engine::Understanding, nocturne_engine::MindError>,
    > {
        self.inner.read_image(image)
    }

    fn read_document<'a>(
        &'a self,
        doc: &'a std::path::Path,
    ) -> nocturne_engine::BoxFuture<
        'a,
        Result<nocturne_engine::Understanding, nocturne_engine::MindError>,
    > {
        self.inner.read_document(doc)
    }

    fn extract<'a>(
        &'a self,
        text: &'a str,
        cx: &'a nocturne_engine::ExtractContext,
    ) -> nocturne_engine::BoxFuture<
        'a,
        Result<Vec<nocturne_engine::Candidate>, nocturne_engine::MindError>,
    > {
        self.inner.extract(text, cx)
    }

    fn extract_lines<'a>(
        &'a self,
        text: &'a str,
        cx: &'a nocturne_engine::ExtractContext,
        lines: &'a [String],
    ) -> nocturne_engine::BoxFuture<
        'a,
        Result<Vec<nocturne_engine::Candidate>, nocturne_engine::MindError>,
    > {
        self.inner.extract_lines(text, cx, lines)
    }

    fn embed<'a>(
        &'a self,
        texts: &'a [String],
    ) -> nocturne_engine::BoxFuture<'a, Result<Vec<Vec<f32>>, nocturne_engine::MindError>> {
        self.inner.embed(texts)
    }

    fn embed_with<'a>(
        &'a self,
        model: &'a str,
        texts: &'a [String],
    ) -> nocturne_engine::BoxFuture<'a, Result<Vec<Vec<f32>>, nocturne_engine::MindError>> {
        self.inner.embed_with(model, texts)
    }

    fn embed_query<'a>(
        &'a self,
        question: &'a str,
    ) -> nocturne_engine::BoxFuture<'a, Result<Vec<f32>, nocturne_engine::MindError>> {
        self.inner.embed_query(question)
    }

    fn answer<'a>(
        &'a self,
        question: &'a str,
        facts: &'a [nocturne_engine::Fact],
        out: tokio::sync::mpsc::Sender<String>,
    ) -> nocturne_engine::BoxFuture<
        'a,
        Result<nocturne_engine::AnswerMeta, nocturne_engine::MindError>,
    > {
        self.facts
            .lock()
            .unwrap()
            .push(facts.iter().map(|f| f.memory.id.clone()).collect());
        self.inner.answer(question, facts, out)
    }

    fn write_headline<'a>(
        &'a self,
        page: &'a nocturne_contract::PageView,
    ) -> nocturne_engine::BoxFuture<'a, Result<(String, String), nocturne_engine::MindError>> {
        self.inner.write_headline(page)
    }

    fn speak<'a>(
        &'a self,
        text: &'a str,
        voice: &'a str,
    ) -> nocturne_engine::BoxFuture<'a, Result<Vec<u8>, nocturne_engine::MindError>> {
        self.inner.speak(text, voice)
    }

    fn ready_to_speak(&self) -> nocturne_engine::BoxFuture<'_, ()> {
        self.inner.ready_to_speak()
    }
}
