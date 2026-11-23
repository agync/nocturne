//! Reads a reflection's reply: sentences, the item markers `[i2]` that end them, and the `**bold**`
//! words drawn from each item. A sentence that cites nothing real is never returned.

use nocturne_engine::{Reflected, WrittenSentence};

use crate::guard;

/// Words that end in a full stop without ending a sentence.
const ABBREVIATIONS: [&str; 9] = ["dr", "mr", "mrs", "ms", "st", "vs", "prof", "e.g", "i.e"];

/// What the mind wrote, as sentences each citing items of a day of `items` items, with how many it
/// wrote before any was dropped. `system` is the instructions it was given, so that a reply that
/// reads them back is dropped.
pub fn sentences(reply: &str, items: usize, system: &str) -> Reflected {
    let reply = guard::strip_control(reply);
    let pieces: Vec<String> = split(&reply)
        .into_iter()
        .filter(|raw| !raw.trim().is_empty())
        .collect();
    let written = pieces.len();
    if guard::degenerate(&strip_markers(&reply), system) {
        return Reflected {
            sentences: Vec::new(),
            written,
        };
    }
    let sentences = pieces
        .iter()
        .filter_map(|raw| sentence(raw, items))
        .filter(|s| !guard::not_sure(&s.text, true) && !guard::degenerate(&s.text, system))
        .collect();
    Reflected { sentences, written }
}

fn strip_markers(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match marker_at(&chars, i) {
            Some((len, _)) => i += len,
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    out.replace("**", "")
}

/// The marker starting at `at`: its length in characters and the item numbers it holds, as
/// written. `[i2]`, `[i2][i3]` and `[i2, i3]` all read.
fn marker_at(chars: &[char], at: usize) -> Option<(usize, Vec<usize>)> {
    if chars.get(at) != Some(&'[') {
        return None;
    }
    let close = chars[at..].iter().take(32).position(|c| *c == ']')?;
    let inner: String = chars[at + 1..at + close].iter().collect();
    let mut numbers = Vec::new();
    let mut lettered = false;
    for token in inner.split([',', ';', ' ']).filter(|t| !t.is_empty()) {
        let digits = match token.strip_prefix(['i', 'I']) {
            Some(rest) => {
                lettered = true;
                rest
            }
            None => token,
        };
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) || digits.len() > 6 {
            return None;
        }
        numbers.push(digits.parse().ok()?);
    }
    (lettered && !numbers.is_empty()).then_some((close + 1, numbers))
}

fn is_stop(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '\u{2026}')
}

fn abbreviation(cur: &str) -> bool {
    let word = cur
        .rsplit(|c: char| c.is_whitespace())
        .next()
        .unwrap_or("")
        .trim_end_matches('.')
        .to_lowercase();
    ABBREVIATIONS.contains(&word.as_str())
}

/// The reply cut after each full stop (or `!`, `?`) that is followed by a space or the end, with
/// the markers that come right after the stop kept with the sentence they end.
fn split(reply: &str) -> Vec<String> {
    let chars: Vec<char> = reply.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut bold = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            bold = !bold;
            cur.push_str("**");
            i += 2;
            continue;
        }
        if let Some((len, _)) = marker_at(&chars, i) {
            cur.extend(&chars[i..i + len]);
            i += len;
            continue;
        }
        cur.push(c);
        i += 1;
        let ends = is_stop(c)
            && !bold
            && chars
                .get(i)
                .is_none_or(|n| n.is_whitespace() || *n == '[' || *n == '"')
            && !(c == '.' && abbreviation(&cur));
        if !ends {
            continue;
        }
        let mut j = i;
        loop {
            let mut k = j;
            while chars.get(k).is_some_and(|n| n.is_whitespace()) {
                k += 1;
            }
            match marker_at(&chars, k) {
                Some((len, _)) => {
                    cur.push(' ');
                    cur.extend(&chars[k..k + len]);
                    j = k + len;
                }
                None => break,
            }
        }
        i = j;
        out.push(std::mem::take(&mut cur));
    }
    out.push(cur);
    out
}

struct Span {
    start: usize,
    end: usize,
    item: Option<usize>,
}

/// The markers that follow one stretch of words, with whether the words before them were any.
struct Group {
    counted: bool,
    items: Vec<usize>,
}

struct Reader {
    items: usize,
    text: String,
    spans: Vec<Span>,
    open: Option<usize>,
    group: Option<Group>,
    cited: Vec<usize>,
    words_since: bool,
    gap: bool,
}

impl Reader {
    fn push_char(&mut self, c: char) {
        if c.is_alphanumeric() {
            self.flush();
            self.words_since = true;
        }
        if c.is_whitespace() {
            if !self.text.is_empty() && !self.text.ends_with(' ') {
                self.text.push(' ');
            }
            self.gap = false;
            return;
        }
        let closing = matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | '\u{2026}');
        if self.gap && !closing && !self.text.is_empty() {
            self.text.push(' ');
        }
        self.gap = false;
        self.text.push(c);
    }

    fn marker(&mut self, numbers: &[usize]) {
        let valid: Vec<usize> = numbers
            .iter()
            .copied()
            .filter(|n| (1..=self.items).contains(n))
            .map(|n| n - 1)
            .collect();
        match &mut self.group {
            Some(group) if !self.words_since => {
                for n in valid {
                    if !group.items.contains(&n) {
                        group.items.push(n);
                    }
                }
            }
            _ => {
                self.flush();
                let mut items = Vec::new();
                for n in valid {
                    if !items.contains(&n) {
                        items.push(n);
                    }
                }
                self.group = Some(Group {
                    counted: self.words_since,
                    items,
                });
            }
        }
        self.words_since = false;
        let kept = self.text.trim_end().len();
        self.text.truncate(kept);
        let len = self.text.len();
        for span in &mut self.spans {
            span.start = span.start.min(len);
            span.end = span.end.min(len);
        }
        self.gap = true;
    }

    /// Settles the markers read so far: the bold spans waiting for one go with the items it
    /// cites, in order when there is one item per span, otherwise all with the first.
    fn flush(&mut self) {
        let Some(group) = self.group.take() else {
            return;
        };
        if !group.counted || group.items.is_empty() {
            return;
        }
        for &n in &group.items {
            if !self.cited.contains(&n) {
                self.cited.push(n);
            }
        }
        let waiting: Vec<usize> = (0..self.spans.len())
            .filter(|&s| self.spans[s].item.is_none() && self.open != Some(s))
            .collect();
        for (k, &s) in waiting.iter().enumerate() {
            self.spans[s].item = Some(if group.items.len() == waiting.len() {
                group.items[k]
            } else {
                group.items[0]
            });
        }
    }

    fn bold(&mut self) {
        match self.open.take() {
            Some(s) => {
                let start = self.spans[s].start;
                let kept = self.text.get(start..).unwrap_or("").trim_end().len();
                self.spans[s].end = start + kept;
            }
            None => {
                self.flush();
                self.spans.push(Span {
                    start: self.text.len(),
                    end: self.text.len(),
                    item: None,
                });
                self.open = Some(self.spans.len() - 1);
            }
        }
    }
}

fn sentence(raw: &str, items: usize) -> Option<WrittenSentence> {
    let chars: Vec<char> = raw.chars().collect();
    let mut r = Reader {
        items,
        text: String::new(),
        spans: Vec::new(),
        open: None,
        group: None,
        cited: Vec::new(),
        words_since: false,
        gap: false,
    };
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '*' && chars.get(i + 1) == Some(&'*') {
            r.bold();
            i += 2;
        } else if let Some((len, numbers)) = marker_at(&chars, i) {
            r.marker(&numbers);
            i += len;
        } else {
            r.push_char(chars[i]);
            i += 1;
        }
    }
    r.flush();
    if r.cited.is_empty() || !r.text.chars().any(char::is_alphanumeric) {
        return None;
    }
    let mut paired: Vec<&Span> = r
        .spans
        .iter()
        .filter(|s| {
            s.item.is_some()
                && r.open.is_none_or(|o| !std::ptr::eq(&r.spans[o], *s))
                && r.text
                    .get(s.start..s.end)
                    .is_some_and(|w| w.chars().any(char::is_alphanumeric))
        })
        .collect();
    paired.sort_by_key(|s| s.start);
    let mut text = String::new();
    let mut at = 0;
    for s in &paired {
        if s.start < at {
            continue;
        }
        text.push_str(r.text.get(at..s.start).unwrap_or(""));
        text.push_str("**");
        text.push_str(r.text.get(s.start..s.end).unwrap_or(""));
        text.push_str("**");
        at = s.end;
    }
    text.push_str(r.text.get(at..).unwrap_or(""));
    let cites = if paired.is_empty() {
        r.cited.clone()
    } else {
        paired.iter().filter_map(|s| s.item).collect()
    };
    Some(WrittenSentence {
        text: text.trim().to_string(),
        cites,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYSTEM: &str = "Write three to six sentences in the second person, one marker after each sentence, never give advice or guess a feeling.";

    fn read(reply: &str, items: usize) -> Vec<(String, Vec<usize>)> {
        sentences(reply, items, SYSTEM)
            .sentences
            .into_iter()
            .map(|s| (s.text, s.cites))
            .collect()
    }

    #[test]
    fn the_count_written_includes_the_sentences_that_were_dropped() {
        let got = sentences(
            "You walked to the station [i1]. You felt tired. You called Sam [i2].",
            2,
            SYSTEM,
        );
        assert_eq!((got.sentences.len(), got.written), (2, 3));
        let got = sentences("You had a lovely day. Truly.", 2, SYSTEM);
        assert_eq!((got.sentences.len(), got.written), (0, 2));
    }

    #[test]
    fn a_sentence_with_no_marker_is_dropped() {
        let got = read(
            "You walked to the station [i1]. You felt tired. You called Sam [i2].",
            2,
        );
        assert_eq!(
            got,
            vec![
                ("You walked to the station.".to_string(), vec![0]),
                ("You called Sam.".to_string(), vec![1]),
            ]
        );
    }

    #[test]
    fn markers_become_zero_based_indexes_and_leave_the_text() {
        let got = read("You walked and then called Sam [i1][i3]. You ate [i2].", 3);
        assert_eq!(
            got[0],
            ("You walked and then called Sam.".into(), vec![0, 2])
        );
        assert_eq!(got[1], ("You ate.".into(), vec![1]));
        let spaced = read("You walked [i1, i2] and ate.", 2);
        assert_eq!(spaced[0], ("You walked and ate.".into(), vec![0, 1]));
    }

    #[test]
    fn markers_after_the_stop_still_end_the_sentence_before_it() {
        let got = read("You walked. [i1] You ate. [i2]", 2);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], ("You walked.".into(), vec![0]));
        assert_eq!(got[1], ("You ate.".into(), vec![1]));
    }

    #[test]
    fn bold_spans_pair_with_the_next_marker() {
        let got = read(
            "You wrote that **the walk was good** [i1], then met Alex at **the old station** [i2].",
            2,
        );
        assert_eq!(
            got,
            vec![(
                "You wrote that **the walk was good**, then met Alex at **the old station**."
                    .to_string(),
                vec![0, 1]
            )]
        );
    }

    #[test]
    fn a_sentence_with_markers_and_no_bold_stays_plain_and_cites_in_order() {
        let got = read("You went out and came back [i2][i1].", 2);
        assert_eq!(got[0], ("You went out and came back.".into(), vec![1, 0]));
    }

    #[test]
    fn two_spans_one_marker_both_take_it() {
        let got = read("You had **tea** and **toast** [i2].", 2);
        assert_eq!(got[0].1, vec![1, 1]);
        assert_eq!(got[0].0, "You had **tea** and **toast**.");
    }

    #[test]
    fn markers_out_of_range_or_repeated_are_ignored() {
        let got = read("You rested [i9]. You read [i0]. You wrote [i1][i1][i7].", 2);
        assert_eq!(got, vec![("You wrote.".to_string(), vec![0])]);
    }

    #[test]
    fn a_marker_with_no_words_before_it_cites_nothing() {
        assert!(read("[i1] You walked.", 2).is_empty());
        assert!(read("[i1]", 2).is_empty());
    }

    #[test]
    fn an_unfinished_or_unpaired_bold_span_is_left_plain() {
        let got = read("You wrote **the walk [i1].", 1);
        assert_eq!(got[0], ("You wrote the walk.".into(), vec![0]));
        let got = read("You wrote **the walk** and ate [i1].", 1);
        assert_eq!(got[0].1, vec![0]);
    }

    #[test]
    fn instructions_read_back_and_refusals_are_dropped() {
        let reply = format!("{SYSTEM} [i1]");
        assert!(read(&reply, 2).is_empty());
        assert!(read("NOT_SURE", 2).is_empty());
        assert!(read("I cannot help with that [i1].", 2).is_empty());
        assert!(read("... [i1]", 2).is_empty());
        assert!(read("", 2).is_empty());
    }

    #[test]
    fn abbreviations_and_decimals_do_not_end_a_sentence() {
        let got = read("You saw Dr. Patel at 6.10 pm [i1].", 1);
        assert_eq!(
            got,
            vec![("You saw Dr. Patel at 6.10 pm.".to_string(), vec![0])]
        );
    }

    #[test]
    fn bold_around_trimmed_space_or_multibyte_text_never_panics() {
        for reply in [
            "**[i1]**",
            "**  [i1]",
            "You **  [i1]. Next **é [i1]",
            "**é [i1]",
            "é** [i1] **é",
            "You rested **é** [i1][i1]**",
        ] {
            let _ = read(reply, 2);
        }
        let got = read("You rested **é [i1]", 2);
        assert_eq!(got, vec![("You rested é".to_string(), vec![0])]);
        assert_eq!(read("You rested **  [i1].", 2)[0].0, "You rested.");
    }

    #[test]
    fn a_model_template_is_stripped_first() {
        let got = read("<think>plan</think>You walked [i1].", 1);
        assert_eq!(got.len(), 1);
    }
}
