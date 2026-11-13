//! Long texts are split for extraction at paragraph boundaries.

/// The most characters sent to the mind in one extraction.
pub const CHUNK_CHARS: usize = 6_000;

/// Split `text` into chunks of at most `max` characters, breaking between paragraphs where it
/// can, then at line ends, then at spaces, and only as a last resort inside a word. Chunks keep
/// their paragraph order and blank text yields none.
pub fn split_chunks(text: &str, max: usize) -> Vec<String> {
    let max = max.max(1);
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    let flush = |current: &mut String, chunks: &mut Vec<String>| {
        let t = current.trim();
        if !t.is_empty() {
            chunks.push(t.to_string());
        }
        current.clear();
    };
    for para in text.split("\n\n") {
        if para.trim().is_empty() {
            continue;
        }
        for piece in break_down(para, max) {
            let joined = current.chars().count() + piece.chars().count() + 2;
            if !current.is_empty() && joined > max {
                flush(&mut current, &mut chunks);
            }
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(&piece);
        }
    }
    flush(&mut current, &mut chunks);
    chunks
}

/// A paragraph that fits comes back whole; a longer one is cut at line ends, then spaces.
fn break_down(para: &str, max: usize) -> Vec<String> {
    if para.chars().count() <= max {
        return vec![para.to_string()];
    }
    let mut out = Vec::new();
    let mut rest = para;
    while rest.chars().count() > max {
        let limit = rest.char_indices().nth(max).map_or(rest.len(), |(i, _)| i);
        let head = &rest[..limit];
        let cut = head
            .rfind('\n')
            .or_else(|| head.rfind(char::is_whitespace))
            .filter(|i| *i > 0)
            .unwrap_or(limit);
        out.push(rest[..cut].trim_end().to_string());
        rest = rest[cut..].trim_start();
    }
    if !rest.is_empty() {
        out.push(rest.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_one_chunk() {
        assert_eq!(split_chunks("hello\n\nworld", 100), ["hello\n\nworld"]);
        assert!(split_chunks("  \n\n ", 100).is_empty());
    }

    #[test]
    fn paragraphs_stay_whole_and_in_order() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let c = "c".repeat(40);
        let text = format!("{a}\n\n{b}\n\n{c}");
        let chunks = split_chunks(&text, 90);
        assert_eq!(chunks, [format!("{a}\n\n{b}"), c]);
    }

    #[test]
    fn a_long_paragraph_breaks_at_spaces() {
        let text = vec!["word"; 50].join(" ");
        let chunks = split_chunks(&text, 60);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.chars().count() <= 60));
        assert_eq!(chunks.join(" "), text);
    }

    #[test]
    fn an_unbroken_run_is_cut_by_length_and_multibyte_is_safe() {
        let text = "é".repeat(130);
        let chunks = split_chunks(&text, 50);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn six_thousand_is_the_default_limit() {
        let text = format!("{}\n\n{}", "x".repeat(4000), "y".repeat(4000));
        let chunks = split_chunks(&text, CHUNK_CHARS);
        assert_eq!(chunks.len(), 2);
    }
}
