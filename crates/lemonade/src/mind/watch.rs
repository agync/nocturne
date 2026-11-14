//! Watching a reading as it arrives. A small mind can fall into a loop, writing the same memory
//! again and again, or copying out its own instructions, until it reaches its token ceiling.
//! The watch spots that as soon as the memory that starts it is whole, so the request can be
//! ended there and the memories before it kept.

use std::ops::Range;

use super::{candidate, grounded, normalise};

/// Finds the memories of a reading reply as each one is written whole: the objects at the top
/// level of the first array in it. Fed the whole reply so far each time, it reads only what is
/// new.
#[derive(Debug, Default)]
pub struct Memories {
    at: usize,
    started: bool,
    ended: bool,
    depth: usize,
    in_string: bool,
    escaped: bool,
    /// Where the memory being written starts.
    open: Option<usize>,
}

impl Memories {
    /// The byte ranges of the memories `reply` completes since the last call. `reply` must
    /// start with what the last call was given.
    pub fn whole(&mut self, reply: &str) -> Vec<Range<usize>> {
        let mut out = Vec::new();
        if self.ended || self.at >= reply.len() {
            return out;
        }
        if !self.started {
            let Some(i) = reply[self.at..].find('[') else {
                self.at = reply.len();
                return out;
            };
            self.started = true;
            self.at += i + 1;
        }
        for (i, ch) in reply[self.at..].char_indices() {
            let at = self.at + i;
            if self.in_string {
                match (self.escaped, ch) {
                    (true, _) => self.escaped = false,
                    (false, '\\') => self.escaped = true,
                    (false, '"') => self.in_string = false,
                    _ => {}
                }
                continue;
            }
            match ch {
                '"' => self.in_string = true,
                '{' => {
                    if self.depth == 0 {
                        self.open = Some(at);
                    }
                    self.depth += 1;
                }
                '}' if self.depth > 0 => {
                    self.depth -= 1;
                    if self.depth == 0
                        && let Some(from) = self.open.take()
                    {
                        out.push(from..at + 1);
                    }
                }
                ']' if self.depth == 0 => {
                    self.ended = true;
                    self.at = at + 1;
                    return out;
                }
                _ => {}
            }
        }
        self.at = reply.len();
        out
    }
}

/// Why a reply was ended early.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loop {
    /// A memory said again what an earlier valid one said, from the same words: the same
    /// subject, predicate and object, which the mind would drop as a duplicate anyway.
    Repeat,
    /// A second memory quoted the instructions or the lines about the item, not the item.
    Echo,
}

/// The fields of a memory that say what it is, in the order they are written.
const SAID: [&str; 5] = ["quote", "sentence", "subject", "predicate", "object"];

/// The fewest words a quote of the instructions has before it counts as copying them: two words
/// such as "has moved" are as likely the item's own.
const ECHO_WORDS: usize = 3;

/// How many memories quoting the instructions a reply may hold before it is ended: the first is
/// left out and the reading goes on, since a small mind sometimes returns to the item after one.
const ECHOES: usize = 2;

/// Watches one reading reply.
#[derive(Debug)]
pub struct Watch {
    /// The item's words, normalised, with a space at each end.
    item: String,
    /// Every message sent, normalised, with a space at each end.
    prompt: String,
    memories: Memories,
    /// What each valid, grounded memory so far says.
    said: Vec<Vec<String>>,
    echoes: usize,
    /// The memory being judged (where it starts), and how far: its quote, then all it says.
    judging: Option<usize>,
    quote_judged: bool,
    said_judged: bool,
    /// Where the memories worth reading end: the end of the last whole memory before the loop.
    kept: usize,
    /// What ended the reply, once something has.
    pub why: Option<Loop>,
}

impl Watch {
    /// A watch on a reply to `prompt` (every message sent) reading `item`.
    pub fn new(item: &str, prompt: &str) -> Watch {
        Watch {
            item: format!(" {} ", normalise(item)),
            prompt: format!(" {} ", normalise(prompt)),
            memories: Memories::default(),
            said: Vec::new(),
            echoes: 0,
            judging: None,
            quote_judged: false,
            said_judged: false,
            kept: 0,
            why: None,
        }
    }

    /// Whether `reply`, the whole reply so far, has begun to loop. A memory is judged as soon
    /// as the fields that show it are written, before it is whole: its quote, then what it says.
    pub fn looping(&mut self, reply: &str) -> bool {
        if self.why.is_some() {
            return true;
        }
        for range in self.memories.whole(reply) {
            let memory = &reply[range.clone()];
            if self.judge(range.start, memory) {
                return true;
            }
            self.keep(memory);
            self.kept = range.end;
        }
        if let Some(from) = self.memories.open
            && self.judge(from, &reply[from..])
        {
            return true;
        }
        false
    }

    /// Judges the memory written from `from`, `memory` being as much of it as has come.
    fn judge(&mut self, from: usize, memory: &str) -> bool {
        if self.judging != Some(from) {
            self.judging = Some(from);
            self.quote_judged = false;
            self.said_judged = false;
        }
        if !self.quote_judged
            && let Some(quote) = string_after(memory, "quote")
        {
            self.quote_judged = true;
            if self.echoed(&quote) {
                self.echoes += 1;
                if self.echoes >= ECHOES {
                    self.why = Some(Loop::Echo);
                    return true;
                }
            }
        }
        if !self.said_judged
            && let Some(said) = said_by(memory)
        {
            self.said_judged = true;
            if self.said.contains(&said) {
                self.why = Some(Loop::Repeat);
                return true;
            }
        }
        false
    }

    /// Whether `quote` is the instructions' words, not the item's: whole words, enough of them.
    fn echoed(&self, quote: &str) -> bool {
        let quote = normalise(quote);
        let bounded = format!(" {quote} ");
        quote.split(' ').count() >= ECHO_WORDS
            && !self.item.contains(&bounded)
            && self.prompt.contains(&bounded)
    }

    /// Remembers what a whole `memory` says, when it is one the mind keeps: a memory that is not
    /// valid, or quotes words the item does not have, cannot make a later one a repeat.
    fn keep(&mut self, memory: &str) {
        let Ok(raw) = serde_json::from_str::<serde_json::Value>(memory) else {
            return;
        };
        let valid = candidate(&raw).is_ok_and(|c| grounded(&c, self.item.trim()));
        if valid && let Some(said) = said_by(memory) {
            self.said.push(said);
        }
    }

    /// The part of `reply` worth reading once it was ended: up to the end of the last whole
    /// memory before the loop.
    pub fn kept<'a>(&self, reply: &'a str) -> &'a str {
        &reply[..self.kept.min(reply.len())]
    }
}

/// What `memory` says, once all of it has come.
fn said_by(memory: &str) -> Option<Vec<String>> {
    SAID.iter()
        .map(|k| string_after(memory, k).map(|v| normalise(&v)))
        .collect()
}

/// The text of `key` in `memory` (a memory's JSON, whole or not), once all of it has come.
fn string_after(memory: &str, key: &str) -> Option<String> {
    let at = memory.find(&format!("\"{key}\""))? + key.len() + 2;
    let value = memory[at..].trim_start().strip_prefix(':')?.trim_start();
    serde_json::Deserializer::from_str(value)
        .into_iter::<String>()
        .next()?
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A whole memory as the grammar writes it: the quote first, the object before the dates.
    fn about(quote: &str, sentence: &str, subject: &str) -> String {
        format!(
            r#"{{"span": {{"quote": "{quote}"}}, "sentence": "{sentence}", "subject": "{subject}", "subjectKind": "person", "predicate": "likes", "object": "tea", "due": null, "validFrom": null, "validTo": null, "moved": false, "confidence": 0.9, "tags": []}}"#
        )
    }

    fn memory(quote: &str, sentence: &str) -> String {
        about(quote, sentence, "Sam")
    }

    fn reply(memories: &[String]) -> String {
        format!("{{\"memories\": [{}", memories.join(", "))
    }

    #[test]
    fn memories_are_found_whole_however_the_reply_arrives() {
        let a = memory("Sam likes {tea}", "Sam likes \\\"tea\\\".");
        let b = memory("Sam likes coffee", "Sam likes coffee.");
        let reply = format!("{{\"memories\": [{a}, {b}], \"x\": [{{}}]}}");
        let mut m = Memories::default();
        let mut found = Vec::new();
        for end in 1..=reply.len() {
            if reply.is_char_boundary(end) {
                found.extend(m.whole(&reply[..end]));
            }
        }
        let texts: Vec<&str> = found.iter().map(|r| &reply[r.clone()]).collect();
        assert_eq!(texts, [a.as_str(), b.as_str()]);
    }

    #[test]
    fn the_same_memory_from_the_same_words_again_is_a_loop() {
        let mut w = Watch::new("Sam likes tea. Sam likes coffee.", "Read this.");
        let tea = memory("Sam likes tea", "Sam likes tea.");
        let also = memory("Sam likes tea", "Sam really likes tea.");
        let ana = about("Sam likes tea", "Sam likes tea.", "Ana");
        let so_far = reply(&[tea.clone(), also, ana]);
        assert!(!w.looping(&so_far), "the same words may hold two facts");
        let again = format!("{so_far}, {}", memory("SAM likes tea!", "Sam likes tea"));
        assert!(w.looping(&again));
        assert_eq!(w.why, Some(Loop::Repeat));
        assert_eq!(w.kept(&again), so_far);
    }

    #[test]
    fn a_memory_the_mind_would_not_keep_makes_no_later_one_a_repeat() {
        let tea = memory("Sam likes tea", "Sam likes tea.");
        let bad_date = tea.replace(r#""due": null"#, r#""due": "next Thursday""#);
        let unquoted = memory("Sam adores tea", "Sam likes tea.");
        let mut w = Watch::new("Sam likes tea.", "Read this.");
        let so_far = reply(&[bad_date, tea.clone(), unquoted]);
        assert!(
            !w.looping(&so_far),
            "the invalid one was not kept, so its twin is new"
        );
        let mut w = Watch::new("Sam likes tea.", "Read this.");
        assert!(w.looping(&reply(&[tea.clone(), tea])));
    }

    #[test]
    fn a_repeat_is_seen_before_the_memory_is_whole() {
        let tea = memory("Sam likes tea", "Sam likes tea.");
        let both = reply(&[tea.clone(), tea.clone()]);
        let at_object = both.rfind(r#""tea", "due""#).unwrap() + r#""tea""#.len();
        let mut w = Watch::new("Sam likes tea.", "");
        assert!(
            !w.looping(&both[..at_object - 1]),
            "the object is not whole yet"
        );
        assert!(w.looping(&both[..at_object]));
        assert_eq!(w.kept(&both), reply(&[tea]));
    }

    #[test]
    fn one_quote_of_the_instructions_is_left_out_and_a_second_ends_the_reply() {
        let item = "Gran lives in Leeds. Her birthday is on 3 December.";
        let prompt = format!(
            "This is a note, kept on 2026-11-12. Today is 2026-11-12.\nNames already known, \
             reuse them exactly: Gran (person).\n\n{item}\nWhen the text says something has moved"
        );
        let leeds = memory("Gran lives in Leeds", "Gran lives in Leeds.");
        let birthday = memory(
            "Her birthday is on 3 December",
            "Gran's birthday is 3 December.",
        );
        let names = memory(
            "Names already known, reuse them exactly",
            "Names are known.",
        );
        let today = memory("Today is 2026-11-12", "Today is 12 November.");
        let mut w = Watch::new(item, &prompt);
        let so_far = reply(&[leeds.clone(), names, birthday]);
        assert!(!w.looping(&so_far), "the reading goes on after one echo");
        let ended = format!("{so_far}, {today}");
        assert!(w.looping(&ended));
        assert_eq!(w.why, Some(Loop::Echo));
        assert_eq!(w.kept(&ended), so_far);

        // Two words, or words that are only part of the instructions' words, are no echo.
        let mut w = Watch::new(item, &prompt);
        let short = memory("has moved", "It has moved.");
        let part = memory("ready known, reuse them", "Known.");
        assert!(!w.looping(&reply(&[leeds, short.clone(), part, short])));
    }
}
