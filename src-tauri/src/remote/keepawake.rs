//! Keeping the computer awake for phones: only while on, only when the person asked, and only
//! while on mains power (re-checked on every round of housekeeping).

/// Whether to hold the computer awake. Unknown power counts as battery.
pub fn wanted(on: bool, asked: bool, on_mains: Option<bool>) -> bool {
    on && asked && on_mains == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_on_mains_while_on_and_asked() {
        assert!(wanted(true, true, Some(true)));
        for (on, asked, mains) in [
            (false, true, Some(true)),
            (true, false, Some(true)),
            (true, true, Some(false)),
            (true, true, None),
        ] {
            assert!(!wanted(on, asked, mains));
        }
    }
}
