//! The Nocturne guide inside the app: its pages, read from the guide's own Markdown, and finding
//! the sections a question is about.
//!
//! The guide is never a memory. It lives here, in memory, beside the store and never in it: it is
//! not kept, read, filed, tidied or linked, and nothing in the store points at it. Only the
//! sections a page lists in its `asked` front matter are taken: those are the ones in scope.
//!
//! A question is matched three ways, with no extra call to the mind. The ways people ask for a
//! section, from its `asked` list, are looked for in the question through the same stemmed word
//! search memories use (a phrase may have up to two other words in it). The question's meaning
//! vector, the one the memory search already made, is compared with each section's. And the
//! question's own words are looked for in each section, the heading and the ways people ask
//! counting double. A question that names Nocturne, one of its screens or controls, or asks how to
//! do something carries a help signal. A section matches with confidence on a phrase or a close
//! match together with a help signal, or on a way of asking of three words or more said word for
//! word; a phrase or a close match without a help signal is an unsure match.

use std::sync::Mutex;

use nocturne_contract::{
    Certainty, GuideBlock, GuideCite, GuideInline, GuidePage, GuideSection, MemoryStatus,
    MemoryView,
};
use rusqlite::Connection;

use crate::mind::Fact;
use crate::util::fts_words;

/// The id an answer cites a guide section by starts with this; it names no memory.
pub const GUIDE_SECTION: &str = "guide:";

/// A close meaning match.
pub const CLOSE_COSINE: f32 = 0.62;
/// A meaning match near enough to offer the guide when the question also reads as help.
pub const NEAR_COSINE: f32 = 0.5;
/// A close word match: this share of the question's own words is in the section.
pub const CLOSE_WORDS: f32 = 0.6;
/// A word match near enough to offer the guide when the question also reads as help.
pub const NEAR_WORDS: f32 = 0.4;
/// Sections a confident answer is written from, at most.
pub const MAX_SECTIONS: usize = 3;
/// Other words a phrase from an `asked` list may have between its own.
const PHRASE_GAP: usize = 2;
/// Words a way of asking needs to be confident said word for word, without a help signal.
const EXACT_WORDS: usize = 3;

/// Words that say nothing about which section a question wants.
const PLAIN: &[&str] = &[
    "the",
    "is",
    "are",
    "was",
    "were",
    "my",
    "of",
    "to",
    "in",
    "on",
    "at",
    "and",
    "or",
    "an",
    "when",
    "what",
    "who",
    "where",
    "which",
    "do",
    "does",
    "did",
    "for",
    "with",
    "it",
    "this",
    "that",
    "me",
    "about",
    "how",
    "can",
    "could",
    "will",
    "would",
    "should",
    "you",
    "your",
    "there",
    "get",
    "use",
    "nocturne",
    "any",
    "anything",
    "something",
    "some",
    "has",
    "have",
    "be",
    "if",
    "so",
    "from",
    "up",
    "out",
    "all",
    "its",
    "into",
    "by",
    "not",
    "no",
    "don",
    "doesn",
    "won",
    "ve",
    "ll",
    "re",
    "go",
    "goes",
    "tell",
    "say",
    "said",
    "put",
];

/// How a question that asks how to do something can start.
const HOW_TO: &[&str] = &[
    "how do i ",
    "how can i ",
    "how to ",
    "how do you ",
    "how does ",
    "can i ",
    "can it ",
    "can nocturne ",
    "can you ",
    "does it ",
    "does nocturne ",
    "does anything ",
    "is there a way ",
    "where do i ",
    "where can i ",
    "where has ",
    "where have ",
    "why can't ",
    "why can’t ",
    "why does ",
    "why doesn't ",
    "why doesn’t ",
    "why won't ",
    "why won’t ",
    "why is it ",
    "what do i do",
    "will it ",
    "will my ",
    "i've lost ",
    "i’ve lost ",
    "i have lost ",
    "i lost ",
    "my phone says ",
    "it says ",
    "did my question ",
    "was my question ",
];

/// Whether `id` names a guide section rather than a memory.
pub fn is_guide_section(id: &str) -> bool {
    id.starts_with(GUIDE_SECTION)
}

/// One page of the guide as the app bundles it.
pub struct GuideSource<'a> {
    pub slug: &'a str,
    pub markdown: &'a str,
    /// The ways people ask, for a page whose front matter cannot hold them (an index page).
    pub asked: Option<&'a [(&'a str, &'a [&'a str])]>,
}

/// One section in scope.
#[derive(Debug, Clone)]
struct Entry {
    id: String,
    page: usize,
    section: usize,
    asked: Vec<String>,
    /// The section as plain text: its heading, then its words.
    text: String,
}

/// How well a question matched one section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    /// A way of asking for the section is in the question.
    pub phrase: bool,
    /// One of three words or more is in it word for word.
    pub exact: bool,
    pub cosine: f32,
    pub words: f32,
}

impl Score {
    fn close(&self) -> bool {
        self.cosine >= CLOSE_COSINE || self.words >= CLOSE_WORDS
    }

    fn near(&self) -> bool {
        self.cosine >= NEAR_COSINE || self.words >= NEAR_WORDS
    }

    fn rank(&self) -> f32 {
        f32::from(u8::from(self.phrase)) + self.cosine.max(0.0) + self.words
    }
}

/// How sure the match is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    Confident,
    Unsure,
}

/// The sections a question is about, in the guide's own order.
#[derive(Debug, Clone, PartialEq)]
pub struct GuideMatch {
    pub confidence: Confidence,
    pub ids: Vec<String>,
}

/// The guide, parsed, with its word index.
pub struct Guide {
    pages: Vec<GuidePage>,
    entries: Vec<Entry>,
    /// Names of Nocturne's screens and controls, lower case: naming one is a help signal.
    names: Vec<String>,
    index: Mutex<Connection>,
}

impl std::fmt::Debug for Guide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guide")
            .field("pages", &self.pages.len())
            .field("sections", &self.entries.len())
            .finish()
    }
}

impl Guide {
    /// Parse the pages. `is_control` says whether words in bold name one of Nocturne's screens
    /// or controls; `names` are more words that make a question read as help. Pages whose
    /// `status` is not `stable` are left out, as are sections no `asked` list names.
    pub fn new(
        sources: &[GuideSource<'_>],
        is_control: &dyn Fn(&str) -> bool,
        names: &[&str],
    ) -> Result<Guide, String> {
        let mut pages = Vec::new();
        let mut entries = Vec::new();
        for src in sources {
            // A checkout that writes Windows line ends must read the same guide.
            let markdown = src.markdown.replace("\r\n", "\n");
            let (front, body) = front_matter(&markdown);
            let asked: Vec<(String, Vec<String>)> = match src.asked {
                Some(list) => list
                    .iter()
                    .map(|(h, ways)| (h.to_string(), ways.iter().map(|w| w.to_string()).collect()))
                    .collect(),
                None => {
                    if front.status.is_none() {
                        return Err(format!("{}: no front matter", src.slug));
                    }
                    if front.status.as_deref() != Some("stable") {
                        continue;
                    }
                    front.asked
                }
            };
            let mut page = parse_page(src.slug, body, is_control);
            for (heading, _) in &asked {
                if !page.sections.iter().any(|s| &s.heading == heading) {
                    return Err(format!("{}: no section \"{heading}\"", src.slug));
                }
            }
            page.sections
                .retain(|s| asked.iter().any(|(h, _)| h == &s.heading));
            let at = pages.len();
            for (n, section) in page.sections.iter().enumerate() {
                let ways = asked
                    .iter()
                    .find(|(h, _)| h == &section.heading)
                    .map(|(_, w)| w.clone())
                    .unwrap_or_default();
                entries.push(Entry {
                    id: format!("{GUIDE_SECTION}{}#{}", page.slug, section.slug),
                    page: at,
                    section: n,
                    asked: ways,
                    text: format!("{}. {}", section.heading, section_text(section)),
                });
            }
            if !page.sections.is_empty() {
                pages.push(page);
            }
        }
        let index = build_index(&entries).map_err(|e| e.to_string())?;
        Ok(Guide {
            pages,
            entries,
            names: names.iter().map(|n| n.to_lowercase()).collect(),
            index: Mutex::new(index),
        })
    }

    pub fn pages(&self) -> &[GuidePage] {
        &self.pages
    }

    /// The ids of the sections in scope, in the guide's order.
    pub fn ids(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.id.clone()).collect()
    }

    /// What each section is read for meaning from, in the order of [`Guide::ids`].
    pub fn texts(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|e| {
                let page = &self.pages[e.page].title;
                format!("{page}: {}", e.text)
            })
            .collect()
    }

    fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// The page and section of `id`.
    pub fn section(&self, id: &str) -> Option<(&GuidePage, &GuideSection)> {
        let e = self.entry(id)?;
        let page = &self.pages[e.page];
        Some((page, &page.sections[e.section]))
    }

    /// The ways people ask for `id`.
    pub fn asked(&self, id: &str) -> &[String] {
        self.entry(id)
            .map(|e| e.asked.as_slice())
            .unwrap_or_default()
    }

    /// Whether `question` reads as being about Nocturne: it names Nocturne, a screen or a
    /// control, or asks how to do something.
    pub fn help_signal(&self, question: &str) -> bool {
        let q = format!(" {} ", normal(question));
        let asks_how = question
            .split(['.', '?', '!'])
            .map(|s| format!("{} ", normal(s)))
            .any(|start| HOW_TO.iter().any(|h| start.starts_with(h)));
        q.contains(" nocturne ")
            || self.names.iter().any(|n| q.contains(&format!(" {n} ")))
            || asks_how
    }

    /// How `question` matches each section, in the order of [`Guide::ids`]. `vectors` are the
    /// sections' meaning vectors, in that order, made by the model that made `query`.
    pub fn scores(
        &self,
        question: &str,
        query: Option<&[f32]>,
        vectors: Option<&[Vec<f32>]>,
    ) -> Vec<Score> {
        let (phrases, exact) = self.phrase_hits(question);
        let words = self.word_scores(question);
        self.entries
            .iter()
            .enumerate()
            .map(|(i, _)| Score {
                phrase: phrases[i],
                exact: exact[i],
                cosine: match (query, vectors.and_then(|v| v.get(i))) {
                    (Some(q), Some(v)) => cosine(q, v),
                    _ => 0.0,
                },
                words: words[i],
            })
            .collect()
    }

    /// The sections `question` is about, if any. A confident match brings every section it is
    /// confident of (at most [`MAX_SECTIONS`], the best first, then put in the guide's order); an
    /// unsure one brings the nearest section.
    pub fn find(
        &self,
        question: &str,
        query: Option<&[f32]>,
        vectors: Option<&[Vec<f32>]>,
    ) -> Option<GuideMatch> {
        let signal = self.help_signal(question);
        let scores = self.scores(question, query, vectors);
        let sure: Vec<usize> = (0..scores.len())
            .filter(|&i| scores[i].exact || ((scores[i].phrase || scores[i].close()) && signal))
            .collect();
        if !sure.is_empty() {
            return Some(GuideMatch {
                confidence: Confidence::Confident,
                ids: self.ordered(sure, &scores),
            });
        }
        let best = (0..scores.len())
            .filter(|&i| scores[i].phrase || scores[i].close() || (scores[i].near() && signal))
            .max_by(|&a, &b| scores[a].rank().total_cmp(&scores[b].rank()))?;
        Some(GuideMatch {
            confidence: Confidence::Unsure,
            ids: vec![self.entries[best].id.clone()],
        })
    }

    /// The sections for a question the person asked to have answered from the guide ("Show the
    /// guide's answer"): what a confident match would bring, or else the nearest section.
    pub fn find_for_help(
        &self,
        question: &str,
        query: Option<&[f32]>,
        vectors: Option<&[Vec<f32>]>,
    ) -> Option<GuideMatch> {
        if let Some(m) = self.find(question, query, vectors)
            && m.confidence == Confidence::Confident
        {
            return Some(m);
        }
        let scores = self.scores(question, query, vectors);
        let best = (0..scores.len())
            .filter(|&i| scores[i].rank() > 0.0)
            .max_by(|&a, &b| scores[a].rank().total_cmp(&scores[b].rank()))?;
        Some(GuideMatch {
            confidence: Confidence::Confident,
            ids: vec![self.entries[best].id.clone()],
        })
    }

    fn ordered(&self, mut found: Vec<usize>, scores: &[Score]) -> Vec<String> {
        found.sort_by(|&a, &b| scores[b].rank().total_cmp(&scores[a].rank()));
        found.truncate(MAX_SECTIONS);
        found.sort_unstable();
        found
            .into_iter()
            .map(|i| self.entries[i].id.clone())
            .collect()
    }

    /// For each section, whether one of the ways people ask for it is in `question`,
    /// and whether one of three words or more is in it word for word.
    fn phrase_hits(&self, question: &str) -> (Vec<bool>, Vec<bool>) {
        let conn = self.index.lock().unwrap_or_else(|e| e.into_inner());
        let n = self.entries.len();
        let asked = || -> rusqlite::Result<(Vec<bool>, Vec<bool>)> {
            conn.execute("DELETE FROM question", [])?;
            conn.execute("INSERT INTO question(text) VALUES (?1)", [question])?;
            let mut stmt = conn.prepare_cached("SELECT 1 FROM question WHERE question MATCH ?1")?;
            let (mut near, mut exact) = (Vec::with_capacity(n), Vec::with_capacity(n));
            for e in &self.entries {
                let (mut hit, mut whole) = (false, false);
                for way in &e.asked {
                    if let Some(q) = phrase_query(way)
                        && stmt.exists([q])?
                    {
                        hit = true;
                    }
                    if let Some(q) = exact_query(way)
                        && stmt.exists([q])?
                    {
                        whole = true;
                    }
                }
                near.push(hit);
                exact.push(whole);
            }
            Ok((near, exact))
        };
        asked().unwrap_or_else(|e| {
            tracing::warn!("could not look for the ways people ask: {e}");
            (vec![false; n], vec![false; n])
        })
    }

    /// For each section, the share of the question's own words found in it: in its heading or
    /// the ways people ask (a whole word each), or only in its text (half). A question of one
    /// word is counted as two.
    fn word_scores(&self, question: &str) -> Vec<f32> {
        let words: Vec<String> = {
            let mut w = fts_words(question);
            w.retain(|x| !PLAIN.contains(&x.as_str()));
            w.dedup();
            w
        };
        let mut out = vec![0.0f32; self.entries.len()];
        if words.is_empty() {
            return out;
        }
        let conn = self.index.lock().unwrap_or_else(|e| e.into_inner());
        let mut look = || -> rusqlite::Result<()> {
            let mut stmt =
                conn.prepare_cached("SELECT id FROM sections WHERE sections MATCH ?1")?;
            for w in &words {
                let mut ids = |q: String| -> rusqlite::Result<Vec<String>> {
                    stmt.query_map([q], |r| r.get::<_, String>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()
                };
                let named = ids(format!("{{heading asked}} : \"{w}\""))?;
                let said = ids(format!("body : \"{w}\""))?;
                for (i, e) in self.entries.iter().enumerate() {
                    if named.contains(&e.id) {
                        out[i] += 1.0;
                    } else if said.contains(&e.id) {
                        out[i] += 0.5;
                    }
                }
            }
            Ok(())
        };
        if let Err(e) = look() {
            tracing::warn!("could not search the guide: {e}");
            return vec![0.0; self.entries.len()];
        }
        // One word alone is never a close match.
        let n = words.len().max(2) as f32;
        out.iter_mut().for_each(|s| *s /= n);
        out
    }

    /// The receipt of a section: its heading, its first sentence and its steps.
    pub fn cite(&self, id: &str) -> Option<GuideCite> {
        let (page, section) = self.section(id)?;
        Some(GuideCite {
            id: id.to_string(),
            page: page.slug.clone(),
            page_title: page.title.clone(),
            section: section.slug.clone(),
            heading: section.heading.clone(),
            quote: quote(section),
            steps: steps(section),
        })
    }

    /// A section as a fact for the mind: its words, under the page's title and its heading. It is
    /// told apart from a memory by its id, and carries no receipt of something kept.
    pub fn fact(&self, id: &str) -> Option<Fact> {
        let (page, section) = self.section(id)?;
        Some(Fact::from(MemoryView {
            id: id.to_string(),
            sentence: section_text(section),
            subject: page.title.clone(),
            entity_id: None,
            predicate: "guide".into(),
            object: section.heading.clone(),
            certainty: Certainty::Sure,
            status: MemoryStatus::Active,
            valid_from: None,
            valid_to: None,
            due: None,
            learned_at: String::new(),
            last_used: None,
            let_go_at: None,
            keep_forever: false,
            private: false,
            suggest_private: false,
            was: None,
            moved_from: None,
            receipts: Vec::new(),
        }))
    }
}

fn build_index(entries: &[Entry]) -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(
        "CREATE VIRTUAL TABLE sections USING fts5(id UNINDEXED, heading, asked, body, \
         tokenize='porter unicode61'); \
         CREATE VIRTUAL TABLE question USING fts5(text, tokenize='porter unicode61');",
    )?;
    for e in entries {
        let (heading, body) = e.text.split_once(". ").unwrap_or((&e.text, ""));
        conn.execute(
            "INSERT INTO sections(id, heading, asked, body) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![e.id, heading, e.asked.join(". "), body],
        )?;
    }
    Ok(conn)
}

/// A way of asking as a stemmed search of the question: every word of it, near each other.
fn phrase_query(way: &str) -> Option<String> {
    let words = fts_words(way);
    match words.len() {
        0 => None,
        1 => Some(format!("\"{}\"", words[0])),
        n => Some(format!(
            "NEAR({}, {})",
            words
                .iter()
                .map(|w| format!("\"{w}\""))
                .collect::<Vec<_>>()
                .join(" "),
            n - 2 + PHRASE_GAP
        )),
    }
}

/// A way of asking of [`EXACT_WORDS`] words or more as a stemmed phrase, every word in order and
/// next to each other ("can't" is "can" and "t").
fn exact_query(way: &str) -> Option<String> {
    let words: Vec<String> = way
        .to_lowercase()
        .replace('’', "'")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    (words.len() >= EXACT_WORDS).then(|| format!("\"{}\"", words.join(" ")))
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

/// Lower case, curly apostrophes made straight, runs of space made one.
fn normal(text: &str) -> String {
    text.to_lowercase()
        .replace('’', "'")
        .split(|c: char| c.is_whitespace() || matches!(c, '?' | '!' | ',' | '.' | ';' | ':'))
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------------------------
// Reading the guide's Markdown: front matter, then a title, an opening and `##` sections of
// paragraphs, numbered steps, lists and tables.

#[derive(Default)]
struct Front {
    status: Option<String>,
    asked: Vec<(String, Vec<String>)>,
}

fn front_matter(markdown: &str) -> (Front, &str) {
    let mut front = Front::default();
    let Some(rest) = markdown.strip_prefix("---\n") else {
        return (front, markdown);
    };
    let Some(end) = rest.find("\n---\n") else {
        return (front, markdown);
    };
    let (head, body) = (&rest[..end], &rest[end + 5..]);
    let mut in_asked = false;
    for line in head.lines() {
        if let Some(item) = line.strip_prefix("  ").filter(|_| in_asked) {
            if let Some((heading, ways)) = item.split_once(": [") {
                let ways = ways.trim_end().trim_end_matches(']');
                front.asked.push((
                    unquote(heading.trim()).to_string(),
                    ways.split(", ")
                        .map(|w| unquote(w.trim()).to_string())
                        .filter(|w| !w.is_empty())
                        .collect(),
                ));
            }
            continue;
        }
        in_asked = false;
        if let Some(v) = line.strip_prefix("status:") {
            front.status = Some(v.trim().to_string());
        } else if line.trim_end() == "asked:" {
            in_asked = true;
        }
    }
    (front, body)
}

fn unquote(s: &str) -> &str {
    for q in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(q).and_then(|x| x.strip_suffix(q)) {
            return inner;
        }
    }
    s
}

/// The slug of a heading, as the site makes it: lower case, words joined by hyphens.
pub fn slug(heading: &str) -> String {
    heading
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn parse_page(slug_of_page: &str, body: &str, is_control: &dyn Fn(&str) -> bool) -> GuidePage {
    let mut title = String::new();
    let mut preamble: Vec<&str> = Vec::new();
    let mut sections: Vec<(String, Vec<&str>)> = Vec::new();
    let mut fenced = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        if fenced {
            match sections.last_mut() {
                Some((_, lines)) => lines.push(line),
                None => preamble.push(line),
            }
        } else if let Some(t) = line.strip_prefix("# ") {
            title = t.trim().to_string();
        } else if let Some(h) = line.strip_prefix("## ") {
            sections.push((h.trim().to_string(), Vec::new()));
        } else if let Some((_, lines)) = sections.last_mut() {
            lines.push(line);
        } else {
            preamble.push(line);
        }
    }
    let opening = blocks(&preamble, is_control)
        .into_iter()
        .find_map(|b| match b {
            GuideBlock::Paragraph { parts } => Some(parts),
            _ => None,
        })
        .unwrap_or_default();
    GuidePage {
        slug: slug_of_page.to_string(),
        title,
        opening,
        sections: sections
            .into_iter()
            .map(|(heading, lines)| GuideSection {
                slug: slug(&heading),
                blocks: blocks(&lines, is_control),
                heading,
            })
            .collect(),
    }
}

fn step_start(line: &str) -> Option<&str> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    (digits > 0)
        .then(|| line[digits..].strip_prefix(". "))
        .flatten()
}

fn blocks(lines: &[&str], is_control: &dyn Fn(&str) -> bool) -> Vec<GuideBlock> {
    let inline = |s: &str| inlines(s, is_control);
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        let item_start = |l: &str| -> Option<String> {
            step_start(l)
                .or_else(|| l.strip_prefix("- "))
                .map(str::to_string)
        };
        if step_start(line).is_some() || line.starts_with("- ") {
            let numbered = step_start(line).is_some();
            let mut items: Vec<String> = Vec::new();
            while i < lines.len() {
                let l = lines[i];
                let starts = if numbered {
                    step_start(l).map(str::to_string)
                } else {
                    l.strip_prefix("- ").map(str::to_string)
                };
                if let Some(first) = starts {
                    items.push(first);
                } else if l.starts_with(' ') && !l.trim().is_empty() {
                    if let Some(last) = items.last_mut() {
                        last.push(' ');
                        last.push_str(l.trim());
                    }
                } else {
                    break;
                }
                i += 1;
            }
            let items = items.iter().map(|s| inline(s)).collect();
            out.push(if numbered {
                GuideBlock::Steps { items }
            } else {
                GuideBlock::List { items }
            });
            continue;
        }
        if line.starts_with('|') {
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].starts_with('|') {
                let cells: Vec<&str> = lines[i]
                    .trim()
                    .trim_matches('|')
                    .split('|')
                    .map(str::trim)
                    .collect();
                if !cells
                    .iter()
                    .all(|c| c.chars().all(|ch| ch == '-' || ch == ':'))
                {
                    rows.push(cells.iter().map(|c| inline(c)).collect());
                }
                i += 1;
            }
            out.push(GuideBlock::Table { rows });
            continue;
        }
        if let Some(h) = line.strip_prefix("### ") {
            out.push(GuideBlock::Paragraph {
                parts: vec![GuideInline::Strong {
                    text: h.trim().to_string(),
                }],
            });
            i += 1;
            continue;
        }
        let mut text = String::new();
        while i < lines.len()
            && !lines[i].trim().is_empty()
            && item_start(lines[i]).is_none()
            && !lines[i].starts_with('|')
        {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(lines[i].trim());
            i += 1;
        }
        out.push(GuideBlock::Paragraph {
            parts: inline(&text),
        });
    }
    out
}

fn push_text(out: &mut Vec<GuideInline>, s: &str) {
    if s.is_empty() {
        return;
    }
    if let Some(GuideInline::Text { text }) = out.last_mut() {
        text.push_str(s);
    } else {
        out.push(GuideInline::Text {
            text: s.to_string(),
        });
    }
}

/// Bold, code and links in a line; a link keeps its words only.
fn inlines(s: &str, is_control: &dyn Fn(&str) -> bool) -> Vec<GuideInline> {
    let mut out = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        let next = ["**", "`", "["]
            .iter()
            .filter_map(|m| rest.find(m).map(|at| (at, *m)))
            .min_by_key(|(at, _)| *at);
        let Some((at, mark)) = next else {
            push_text(&mut out, rest);
            break;
        };
        push_text(&mut out, &rest[..at]);
        let after = &rest[at + mark.len()..];
        match mark {
            "**" => match after.find("**") {
                Some(end) => {
                    let text = after[..end].to_string();
                    out.push(if is_control(&text) {
                        GuideInline::Control { text }
                    } else {
                        GuideInline::Strong { text }
                    });
                    rest = &after[end + 2..];
                }
                None => {
                    push_text(&mut out, mark);
                    rest = after;
                }
            },
            "`" => match after.find('`') {
                Some(end) => {
                    out.push(GuideInline::Code {
                        text: after[..end].to_string(),
                    });
                    rest = &after[end + 1..];
                }
                None => {
                    push_text(&mut out, mark);
                    rest = after;
                }
            },
            _ => match after
                .find("](")
                .and_then(|mid| after[mid..].find(')').map(|close| (mid, mid + close)))
            {
                Some((mid, close)) => {
                    for part in inlines(&after[..mid], is_control) {
                        match part {
                            GuideInline::Text { text } => push_text(&mut out, &text),
                            other => out.push(other),
                        }
                    }
                    rest = &after[close + 1..];
                }
                None => {
                    push_text(&mut out, mark);
                    rest = after;
                }
            },
        }
    }
    out
}

/// The words of inline runs, as read.
pub fn plain(parts: &[GuideInline]) -> String {
    parts
        .iter()
        .map(|p| match p {
            GuideInline::Text { text }
            | GuideInline::Strong { text }
            | GuideInline::Control { text }
            | GuideInline::Code { text } => text.as_str(),
        })
        .collect()
}

/// A section's words as one line: paragraphs, numbered steps and list items in order.
fn section_text(section: &GuideSection) -> String {
    let mut out: Vec<String> = Vec::new();
    for block in &section.blocks {
        match block {
            GuideBlock::Paragraph { parts } => out.push(plain(parts)),
            GuideBlock::Steps { items } => out.extend(
                items
                    .iter()
                    .enumerate()
                    .map(|(n, item)| format!("{}. {}", n + 1, plain(item))),
            ),
            GuideBlock::List { items } => out.extend(items.iter().map(|i| plain(i))),
            GuideBlock::Table { rows } => out.extend(
                rows.iter()
                    .map(|r| r.iter().map(|c| plain(c)).collect::<Vec<_>>().join(": ")),
            ),
        }
    }
    out.join(" ")
}

/// Where the first sentence of `text` ends, after its full stop.
fn sentence_end(text: &str) -> Option<usize> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (k, &(at, c)) in chars.iter().enumerate() {
        if matches!(c, '.' | '?' | '!') {
            let next = chars.get(k + 1).map(|(_, n)| *n);
            let prev = k.checked_sub(1).map(|j| chars[j].1);
            let in_number = prev.is_some_and(|p| p.is_ascii_digit())
                && next.is_some_and(|n| n.is_ascii_digit());
            if !in_number && next.is_none_or(char::is_whitespace) {
                return Some(at + c.len_utf8());
            }
        }
    }
    None
}

/// The first sentence of inline runs, its controls kept.
pub fn first_sentence(parts: &[GuideInline]) -> Vec<GuideInline> {
    let mut out = Vec::new();
    for p in parts {
        match p {
            GuideInline::Text { text } | GuideInline::Strong { text } => {
                if let Some(end) = sentence_end(text) {
                    let cut = text[..end].to_string();
                    out.push(match p {
                        GuideInline::Strong { .. } => GuideInline::Strong { text: cut },
                        _ => GuideInline::Text { text: cut },
                    });
                    return out;
                }
                out.push(p.clone());
            }
            _ => out.push(p.clone()),
        }
    }
    out
}

fn quote(section: &GuideSection) -> String {
    match section.blocks.first() {
        Some(GuideBlock::Steps { items } | GuideBlock::List { items }) => {
            items.first().map(|i| plain(i)).unwrap_or_default()
        }
        Some(GuideBlock::Paragraph { parts }) => plain(&first_sentence(parts)),
        Some(GuideBlock::Table { .. }) | None => section.heading.clone(),
    }
}

/// The steps of a section for the guide's own words: the first sentence of each numbered step,
/// else of each list item, else of each paragraph.
fn steps(section: &GuideSection) -> Vec<Vec<GuideInline>> {
    let items = |numbered: bool| {
        section.blocks.iter().find_map(|b| match b {
            GuideBlock::Steps { items } if numbered => Some(items),
            GuideBlock::List { items } if !numbered => Some(items),
            _ => None,
        })
    };
    if let Some(list) = items(true).or_else(|| items(false)) {
        return list.iter().map(|i| first_sentence(i)).collect();
    }
    section
        .blocks
        .iter()
        .filter_map(|b| match b {
            GuideBlock::Paragraph { parts } => Some(first_sentence(parts)),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "---\ntype: Guide\ntitle: Away from home\nstatus: stable\nasked:\n  Add a phone: [connect my phone, pair a phone, \"get it on my phone\"]\n  Remove a lost phone: [lost my phone]\n---\n\n# Away from home\n\nAway from home lets your phone ask Nocturne a question. It is off until you turn it on.\n\n## Turn it on\n\n1. Open **Away from home** and choose **Set up…**.\n2. Choose a connection.\n\n## Add a phone\n\n1. Choose **Add a phone**. Nocturne shows a code that works once and lasts five minutes.\n2. On the phone, scan the code with its camera. If you cannot scan it, open the address\n   shown and type the code.\n3. Compare the three words. If they are the same, press **Allow**.\n\n## Remove a lost phone\n\nOpen Away from home, choose the phone and remove it. It stops at once.\n";

    const DRAFT: &str =
        "---\nstatus: draft\nasked:\n  One: [a b]\n---\n\n# Draft\n\nHello.\n\n## One\n\nWords.\n";

    fn controls(s: &str) -> bool {
        matches!(s, "Away from home" | "Set up…" | "Add a phone")
    }

    fn guide() -> Guide {
        Guide::new(
            &[
                GuideSource {
                    slug: "away-from-home",
                    markdown: PAGE,
                    asked: None,
                },
                GuideSource {
                    slug: "draft",
                    markdown: DRAFT,
                    asked: None,
                },
            ],
            &controls,
            &["away from home", "add a phone"],
        )
        .unwrap()
    }

    #[test]
    fn only_stable_pages_and_asked_sections_are_taken() {
        let g = guide();
        assert_eq!(g.pages().len(), 1, "the draft page is left out");
        assert_eq!(
            g.ids(),
            [
                "guide:away-from-home#add-a-phone",
                "guide:away-from-home#remove-a-lost-phone"
            ]
        );
        let page = &g.pages()[0];
        assert_eq!(page.title, "Away from home");
        assert_eq!(
            plain(&page.opening),
            "Away from home lets your phone ask Nocturne a question. It is off until you turn it on."
        );
        assert_eq!(
            g.asked("guide:away-from-home#add-a-phone"),
            ["connect my phone", "pair a phone", "get it on my phone"]
        );
    }

    #[test]
    fn a_page_naming_a_section_it_does_not_have_is_refused() {
        let bad = PAGE.replace("Remove a lost phone: [", "Remove a phone: [");
        let err = Guide::new(
            &[GuideSource {
                slug: "away",
                markdown: &bad,
                asked: None,
            }],
            &controls,
            &[],
        )
        .unwrap_err();
        assert!(err.contains("Remove a phone"), "{err}");
    }

    #[test]
    fn steps_controls_and_continuation_lines_are_read() {
        let g = guide();
        let (_, section) = g.section("guide:away-from-home#add-a-phone").unwrap();
        let GuideBlock::Steps { items } = &section.blocks[0] else {
            panic!("{:?}", section.blocks);
        };
        assert_eq!(items.len(), 3);
        assert_eq!(
            items[0][1],
            GuideInline::Control {
                text: "Add a phone".into()
            }
        );
        assert_eq!(
            plain(&items[1]),
            "On the phone, scan the code with its camera. If you cannot scan it, open the address shown and type the code."
        );
        assert_eq!(
            items[2][1],
            GuideInline::Strong {
                text: "Allow".into()
            },
            "a word that names no control of Nocturne stays bold"
        );
        let cite = g.cite("guide:away-from-home#add-a-phone").unwrap();
        assert_eq!(cite.heading, "Add a phone");
        assert_eq!(cite.page_title, "Away from home");
        assert_eq!(
            cite.quote,
            "Choose Add a phone. Nocturne shows a code that works once and lasts five minutes."
        );
        let steps: Vec<String> = cite.steps.iter().map(|s| plain(s)).collect();
        assert_eq!(
            steps,
            [
                "Choose Add a phone.",
                "On the phone, scan the code with its camera.",
                "Compare the three words."
            ]
        );
        let lost = g.cite("guide:away-from-home#remove-a-lost-phone").unwrap();
        assert_eq!(
            lost.steps.iter().map(|s| plain(s)).collect::<Vec<_>>(),
            ["Open Away from home, choose the phone and remove it."]
        );
    }

    #[test]
    fn inline_marks_links_and_tables() {
        let parts = inlines(
            "See [Your data](your-data.md), the `store` folder and **Make a backup.** too",
            &|_| false,
        );
        assert_eq!(
            parts,
            vec![
                GuideInline::Text {
                    text: "See Your data, the ".into()
                },
                GuideInline::Code {
                    text: "store".into()
                },
                GuideInline::Text {
                    text: " folder and ".into()
                },
                GuideInline::Strong {
                    text: "Make a backup.".into()
                },
                GuideInline::Text {
                    text: " too".into()
                },
            ]
        );
        let b = blocks(&["| A | B |", "|---|---|", "| `x` | y |"], &|_| false);
        let GuideBlock::Table { rows } = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn a_way_of_asking_matches_with_confidence() {
        let g = guide();
        for q in [
            "How do I connect my phone?",
            "Can I pair a phone with it?",
            "I've lost my phone. What do I do?",
            "Gran's phone. How do I connect my phone?",
        ] {
            let m = g.find(q, None, None).unwrap_or_else(|| panic!("{q}"));
            assert_eq!(m.confidence, Confidence::Confident, "{q}");
        }
        let m = g.find("I've lost my phone", None, None).unwrap();
        assert_eq!(m.ids, ["guide:away-from-home#remove-a-lost-phone"]);
        // The words of a phrase may have up to two others between them, and are stemmed; without a
        // help signal that is only unsure.
        let m = g.find("connecting my new phone", None, None).unwrap();
        assert_eq!(m.confidence, Confidence::Unsure);
        // A way of asking of three words or more, word for word, needs no help signal.
        let m = g.find("Pair a phone", None, None).unwrap();
        assert_eq!(m.confidence, Confidence::Confident);
    }

    #[test]
    fn a_memory_question_that_shares_words_is_not_help() {
        let g = guide();
        for q in [
            "What is Gran's new phone number?",
            "When did Leo get his new phone?",
            "When did Leo say he lost his phone?",
            "Can Leo connect the printer?",
            "Can I see Gran's phone number?",
        ] {
            assert!(
                g.find(q, None, None)
                    .is_none_or(|m| m.confidence == Confidence::Unsure),
                "{q}"
            );
        }
    }

    #[test]
    fn meaning_needs_a_help_signal_to_be_confident() {
        let g = guide();
        let n = g.ids().len();
        let mut vectors = vec![vec![0.0, 1.0]; n];
        vectors[0] = vec![1.0, 0.0];
        let close = [0.9f32, 0.1];
        let m = g
            .find("Show me the code thing", Some(&close), Some(&vectors))
            .unwrap();
        assert_eq!(m.confidence, Confidence::Unsure, "no help signal");
        let m = g
            .find(
                "How do I show the code thing?",
                Some(&close),
                Some(&vectors),
            )
            .unwrap();
        assert_eq!(m.confidence, Confidence::Confident);
        assert_eq!(m.ids, ["guide:away-from-home#add-a-phone"]);
        let far = [-1.0f32, 0.0];
        assert!(
            g.find("Where did I put the spare key?", Some(&far), Some(&vectors))
                .is_none()
        );
    }

    #[test]
    fn help_signals() {
        let g = guide();
        assert!(g.help_signal("How do I keep a photo?"));
        assert!(g.help_signal("Where is Away from home?"));
        assert!(g.help_signal("does nocturne print?"));
        assert!(g.help_signal("Why can’t it hear me?"));
        assert!(!g.help_signal("Where did I put the spare key?"));
        assert!(!g.help_signal("What did the plumber say about turning the water off?"));
    }

    #[test]
    fn the_guide_answers_when_asked_to_even_when_unsure() {
        let g = guide();
        let m = g.find_for_help("Phone stopped", None, None).unwrap();
        assert_eq!(m.confidence, Confidence::Confident);
        assert_eq!(m.ids.len(), 1);
        assert!(g.find_for_help("Bananas", None, None).is_none());
    }

    #[test]
    fn a_section_as_a_fact_is_no_memory() {
        let g = guide();
        let f = g.fact("guide:away-from-home#add-a-phone").unwrap();
        assert!(is_guide_section(&f.memory.id));
        assert!(f.memory.receipts.is_empty());
        assert!(f.memory.sentence.starts_with("1. Choose Add a phone."));
        assert_eq!(f.memory.subject, "Away from home");
        assert_eq!(f.memory.object, "Add a phone");
    }

    #[test]
    fn windows_line_ends_read_the_same() {
        let crlf = PAGE.replace('\n', "\r\n");
        let g = Guide::new(
            &[GuideSource {
                slug: "away-from-home",
                markdown: &crlf,
                asked: None,
            }],
            &controls,
            &[],
        )
        .unwrap();
        assert_eq!(g.ids(), guide().ids());
    }

    #[test]
    fn a_page_without_front_matter_is_refused() {
        let err = Guide::new(
            &[GuideSource {
                slug: "bare",
                markdown: "# Bare\n\nHello.\n\n## One\n\nWords.\n",
                asked: None,
            }],
            &controls,
            &[],
        )
        .unwrap_err();
        assert!(err.contains("no front matter"), "{err}");
    }

    #[test]
    fn a_heading_in_a_code_block_is_no_heading() {
        let page = parse_page(
            "p",
            "# Title\n\nOpening.\n\n## One\n\n```\n## not a section\n# not a title\n```\n",
            &|_| false,
        );
        assert_eq!(page.title, "Title");
        assert_eq!(page.sections.len(), 1);
    }

    #[test]
    fn slugs_follow_the_heading() {
        assert_eq!(
            slug("Let go, bring back, forget"),
            "let-go-bring-back-forget"
        );
        assert_eq!(slug("Quick keep shortcut"), "quick-keep-shortcut");
    }
}
