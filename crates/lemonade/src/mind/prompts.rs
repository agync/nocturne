//! The words the model is given, and the schemas its answers must fit.

use nocturne_contract::{EntityKind, ItemKind, MemoryView, PageView};
use nocturne_engine::ExtractContext;
use serde_json::{Value, json};

pub const NOT_SURE: &str = "NOT_SURE";

pub const PREDICATES: [&str; 10] = [
    "prefers",
    "works_on",
    "decided",
    "lives_in",
    "knows",
    "happened",
    "due",
    "promised",
    "scheduled",
    "takes",
];

pub fn extraction_schema() -> Value {
    let date = json!({"type": ["string", "null"], "description": "YYYY-MM-DD or null"});
    json!({
        "type": "object",
        "properties": {
            "memories": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "sentence": {"type": "string"},
                        "subject": {"type": "string"},
                        "subjectKind": {"type": ["string", "null"], "enum": ["person", "place", "thing", null]},
                        "predicate": {"type": "string"},
                        "object": {"type": "string"},
                        "validFrom": date,
                        "validTo": date,
                        "due": date,
                        "confidence": {"type": "number"},
                        "span": {
                            "type": ["object", "null"],
                            "properties": {"quote": {"type": "string"}},
                            "required": ["quote"]
                        },
                        "tags": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["sentence", "subject", "predicate", "object", "confidence"]
                }
            }
        },
        "required": ["memories"]
    })
}

pub fn extraction_system() -> String {
    format!(
        "You read one thing a person kept and list the separate facts worth remembering from it.\n\
Reply with JSON only, shaped {{\"memories\": [...]}}. Each memory has:\n\
- sentence: one plain British English sentence that stands alone, in the third person or as the person wrote it.\n\
- subject: who or what it is about; subjectKind: person, place or thing.\n\
- predicate: one of {}; when none fits, write other:<one word>.\n\
- object: the value of the fact.\n\
- validFrom, validTo, due: dates as YYYY-MM-DD when the text gives them, otherwise null. Work out relative dates from the day the item was kept.\n\
- confidence: 0 to 1, how sure the text makes it.\n\
- span: {{\"quote\": the exact words that support it}}.\n\
- tags: a few lower-case words.\n\
Skip small talk and anything that will not matter later. Never invent facts. An empty list is fine.",
        PREDICATES.join(", ")
    )
}

fn kind_word(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Person => "person",
        EntityKind::Place => "place",
        EntityKind::Thing => "thing",
    }
}

pub fn extraction_user(text: &str, cx: &ExtractContext) -> String {
    let kind = match cx.item_kind {
        ItemKind::Text => "a note",
        ItemKind::Image => "text read from an image",
        ItemKind::Audio => "a voice recording transcript",
        ItemKind::Document => "a document",
        ItemKind::Web => "a web page",
    };
    let mut out = format!(
        "This is {kind}, kept on {}. Today is {}.\n",
        cx.kept_at, cx.today
    );
    if !cx.known_entities.is_empty() {
        let known: Vec<String> = cx
            .known_entities
            .iter()
            .map(|(name, k)| format!("{name} ({})", kind_word(*k)))
            .collect();
        out.push_str(&format!(
            "Names already known, reuse them exactly: {}.\n",
            known.join(", ")
        ));
    }
    out.push_str("\n---\n");
    out.push_str(text);
    out
}

pub fn answer_system() -> String {
    format!(
        "You answer a person's question about their own life using only the numbered facts you are given.\n\
Write in British English, briefly and plainly. After each claim, cite the fact it came from as [m1], [m2] and so on.\n\
Never add anything the facts do not say. If the facts do not answer the question, reply with exactly {NOT_SURE} and nothing else."
    )
}

pub fn answer_user(question: &str, facts: &[MemoryView]) -> String {
    let mut out = String::from("Facts:\n");
    for (i, f) in facts.iter().enumerate() {
        let learned = f.learned_at.get(..10).unwrap_or(&f.learned_at);
        out.push_str(&format!(
            "[m{}] {} (learned {learned}, {:?})\n",
            i + 1,
            f.sentence,
            f.status
        ));
    }
    out.push_str(&format!("\nQuestion: {question}"));
    out
}

pub fn image_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": {"type": "string"},
            "text": {"type": "string"},
            "summary": {"type": "string"},
            "language": {"type": "string"}
        },
        "required": ["title", "text", "summary", "language"]
    })
}

pub fn image_system() -> &'static str {
    "You look at an image a person kept. Reply with JSON only: title (a few words), text (every word legible in the image, in reading order, empty if none), summary (one or two plain sentences saying what the image shows), language (ISO 639-1 code of the text, or \"en\")."
}

pub fn headline_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"headline": {"type": "string"}, "dek": {"type": "string"}},
        "required": ["headline", "dek"]
    })
}

pub fn headline_system() -> &'static str {
    "You write the front of a morning page of someone's personal memory. Reply with JSON only: headline (under 9 words, sentence case, no full stop) and dek (one calm sentence under 22 words). British English. Be specific about what is on the page; never invent anything."
}

pub fn headline_user(page: &PageView) -> String {
    let mut out = format!(
        "Page for {}, number {}. Kept: {}, changes: {}, let go: {}.\n",
        page.date, page.page_number, page.kept_count, page.changes_count, page.let_go_count
    );
    for (title, lines) in [
        ("Needs you", &page.needs_you),
        ("Changed", &page.changed),
        ("Let go", &page.let_go),
        ("Coming up", &page.upcoming),
        ("Learned", &page.learned),
    ] {
        if lines.is_empty() {
            continue;
        }
        out.push_str(&format!("\n{title}:\n"));
        for l in lines.iter().take(8) {
            out.push_str(&format!("- {}\n", l.text.replace("**", "")));
        }
    }
    out
}
