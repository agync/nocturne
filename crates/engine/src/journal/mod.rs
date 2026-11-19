//! The journal (`docs/design/journaling.md`): entries, check-ins, good things, the people and
//! places found in them, and the reflections and recaps that look back.
//!
//! Journal items are ordinary items on the tape. What makes one a journal item lives in its
//! `metadata`: `journal` (`entry`, `good` or `checkin`), `journal_day`, `block`, `replaces`,
//! `about`, `feeling`, `line`, `urge`, `tried`, `photo` and `alt`.

pub(crate) mod mentions;

/// The metadata key that marks a journal item, and its values.
#[allow(dead_code)]
pub(crate) const ROLE_KEY: &str = "journal";
