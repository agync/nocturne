//! Turns `[m2]` markers in a streamed answer into fact numbers, and keeps them out of the text.

/// Receives text deltas, returns the text safe to show, and remembers which facts were cited.
pub struct CiteFilter {
    facts: usize,
    pending: String,
    cited: Vec<usize>,
}

impl CiteFilter {
    pub fn new(facts: usize) -> Self {
        CiteFilter {
            facts,
            pending: String::new(),
            cited: Vec::new(),
        }
    }

    /// Fact numbers (from 1) in the order first cited.
    pub fn cited(&self) -> &[usize] {
        &self.cited
    }

    pub fn push(&mut self, delta: &str) -> String {
        self.pending.push_str(delta);
        self.drain(false)
    }

    pub fn finish(&mut self) -> String {
        self.drain(true)
    }

    fn drain(&mut self, last: bool) -> String {
        let mut out = String::new();
        let mut rest = std::mem::take(&mut self.pending);
        while let Some(open) = rest.find('[') {
            out.push_str(&rest[..open]);
            let tail = &rest[open..];
            match parse_marker(tail) {
                Marker::Full(len, numbers) => {
                    for n in numbers {
                        if (1..=self.facts).contains(&n) && !self.cited.contains(&n) {
                            self.cited.push(n);
                        }
                    }
                    if out.ends_with(' ') {
                        out.pop();
                    }
                    rest = tail[len..].to_string();
                }
                Marker::Partial if !last => {
                    self.pending = tail.to_string();
                    return self.hold_space(out);
                }
                _ => {
                    out.push('[');
                    rest = tail[1..].to_string();
                }
            }
        }
        out.push_str(&rest);
        if last {
            return out;
        }
        self.hold_space(out)
    }

    /// A trailing space waits in case a marker follows and takes it with it.
    fn hold_space(&mut self, mut out: String) -> String {
        if out.ends_with(' ') {
            out.pop();
            self.pending.insert(0, ' ');
        }
        out
    }
}

enum Marker {
    Full(usize, Vec<usize>),
    Partial,
    No,
}

/// `[m1]`, `[m1, m2]` or `[m1][m2]` at the start of `s`.
fn parse_marker(s: &str) -> Marker {
    let Some(close) = s.find(']') else {
        let body = &s[1..];
        let plausible = s.len() <= 24
            && body
                .chars()
                .all(|c| c == 'm' || c == ',' || c == ' ' || c.is_ascii_digit());
        return if plausible {
            Marker::Partial
        } else {
            Marker::No
        };
    };
    let body = &s[1..close];
    let mut numbers = Vec::new();
    for part in body.split([',', ' ']).filter(|p| !p.is_empty()) {
        match part.strip_prefix('m').and_then(|n| n.parse::<usize>().ok()) {
            Some(n) => numbers.push(n),
            None => return Marker::No,
        }
    }
    if numbers.is_empty() {
        return Marker::No;
    }
    Marker::Full(close + 1, numbers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(chunks: &[&str]) -> (String, Vec<usize>) {
        let mut f = CiteFilter::new(3);
        let mut out = String::new();
        for c in chunks {
            out.push_str(&f.push(c));
        }
        out.push_str(&f.finish());
        (out, f.cited().to_vec())
    }

    #[test]
    fn strips_markers_and_the_space_before_them() {
        assert_eq!(
            run(&["You see Sam on Tuesday [m2]. Then more [m1] text."]),
            ("You see Sam on Tuesday. Then more text.".into(), vec![2, 1])
        );
    }

    #[test]
    fn handles_markers_split_across_deltas() {
        let (text, cited) = run(&["On Tuesday ", "[", "m", "2", "]", ". Bye"]);
        assert_eq!((text.as_str(), cited), ("On Tuesday. Bye", vec![2]));
        let (text, cited) = run(&["a [m", "1][m3] b"]);
        assert_eq!((text.as_str(), cited), ("a b", vec![1, 3]));
    }

    #[test]
    fn keeps_brackets_that_are_not_markers() {
        let (text, cited) = run(&["see [the note] and [x", "yz] ok [m"]);
        assert_eq!(text, "see [the note] and [xyz] ok [m");
        assert!(cited.is_empty());
    }

    #[test]
    fn ignores_out_of_range_and_repeats() {
        let (text, cited) = run(&["a [m9] b [m1, m1] c"]);
        assert_eq!(text, "a b c");
        assert_eq!(cited, vec![1]);
    }
}
