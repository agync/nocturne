//! Voice notes in words, on the phone itself, with no network. After a voice note is kept, and
//! when the app opens, each note still waiting is read by the phone's own speech reader: the
//! item is leased, its words recorded, and the lease given back, so it stays waiting for a mind
//! that may read it better. Only when the reader's check passes for the phone's language, and
//! only with the permission already given: nothing here asks for it. Where the reader cannot,
//! the note stays waiting to be read.
//!
//! The reader takes raw sound (16-bit, mono, 16 kHz, no header). A note kept as such a WAV file
//! is read; any other recording waits until the reader can take it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use nocturne_contract::{ItemFilter, ItemKind, ItemStatus};
use nocturne_engine::Transcript;

use super::{Home, lock};

/// How long one note may take to read before the lease runs out.
const LEASE: Duration = Duration::from_secs(600);
const RATE: u32 = 16_000;
/// The longest language tag the screens may send.
const MAX_TAG: usize = 35;

/// What the phone's reader made of one note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    Words(String),
    /// It read the sound and found no words.
    Nothing,
    /// It could not read it now.
    Waits,
}

/// The phone's speech reader, behind a seam so tests need no phone.
pub trait Reader {
    /// Whether a file in `language` can be read now, offline.
    fn ready(&self, language: &str) -> bool;
    fn read(&self, pcm: &Path, language: &str) -> Heard;
}

#[derive(Default)]
pub struct Voice {
    language: Mutex<Option<String>>,
    running: AtomicBool,
    /// Notes read this run with no words found: not read again until the app starts again.
    nothing: Mutex<HashSet<String>>,
}

struct Running<'a>(&'a AtomicBool);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// A language tag as the web view gives it, such as `en-GB`.
fn valid_language(tag: &str) -> bool {
    (2..=MAX_TAG).contains(&tag.len())
        && tag.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && tag.as_bytes()[0].is_ascii_alphabetic()
}

/// The raw sound of a 16-bit, mono, 16 kHz PCM WAV file, written to `dest`; answers how long
/// it plays. `None` for anything else.
pub fn pcm_from_wav(src: &Path, dest: &Path) -> Option<u32> {
    let bytes = std::fs::read(src).ok()?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at =
        |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let mut at = 12;
    let mut plain = false;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let len = u32_at(at + 4) as usize;
        let body = at + 8;
        let end = body.checked_add(len)?;
        if id == b"fmt " {
            if len < 16 || end > bytes.len() {
                return None;
            }
            plain = u16_at(body) == 1
                && u16_at(body + 2) == 1
                && u32_at(body + 4) == RATE
                && u16_at(body + 14) == 16;
            if !plain {
                return None;
            }
        } else if id == b"data" {
            if !plain {
                return None;
            }
            let data = &bytes[body..end.min(bytes.len())];
            if data.is_empty() {
                return None;
            }
            std::fs::write(dest, data).ok()?;
            return Some((data.len() as u64 * 1000 / (u64::from(RATE) * 2)) as u32);
        }
        at = end.checked_add(len & 1)?;
    }
    None
}

impl Home {
    /// The language voice notes are read in, from the screens. Answers whether it was taken.
    pub fn set_language(&self, tag: &str) -> bool {
        if !valid_language(tag) {
            return false;
        }
        *lock(&self.voice.language) = Some(tag.to_string());
        true
    }

    /// Read every voice note still waiting that the reader can read now. Answers how many got
    /// their words. One pass at a time; a pass asked for while one runs does nothing.
    pub fn hear_waiting(&self, reader: &dyn Reader) -> u32 {
        let Some(language) = lock(&self.voice.language).clone() else {
            return 0;
        };
        if self.voice.running.swap(true, Ordering::SeqCst) {
            return 0;
        }
        let _running = Running(&self.voice.running);
        let Ok(open) = self.open_store() else {
            return 0;
        };
        let e = &open.engine;
        let mut seen: HashSet<String> = HashSet::new();
        let mut asked = false;
        let mut heard = 0;
        // Notes kept while a pass reads are read in the same pass.
        loop {
            let kept = ItemFilter {
                since: None,
                status: Some(ItemStatus::Kept),
                limit: None,
            };
            let waiting: Vec<_> = match e.items(kept) {
                Ok(items) => items
                    .into_iter()
                    .filter(|v| v.kind == ItemKind::Audio && v.text.is_none())
                    .filter_map(|v| v.original_path.map(|o| (v.id, PathBuf::from(o))))
                    .filter(|(id, _)| !seen.contains(id) && !lock(&self.voice.nothing).contains(id))
                    .collect(),
                Err(_) => break,
            };
            if waiting.is_empty() {
                break;
            }
            if !asked {
                if !reader.ready(&language) {
                    break;
                }
                asked = true;
            }
            let scratch = self.keeping_dir();
            if std::fs::create_dir_all(&scratch).is_err() {
                break;
            }
            for (id, original) in waiting {
                seen.insert(id.clone());
                if self.hear_one(e, reader, &language, &id, &original, &scratch) {
                    heard += 1;
                }
            }
        }
        heard
    }

    /// Read one note; answers whether its words were kept.
    fn hear_one(
        &self,
        e: &nocturne_engine::Engine,
        reader: &dyn Reader,
        language: &str,
        id: &str,
        original: &Path,
        scratch: &Path,
    ) -> bool {
        let pcm = scratch.join(format!("{id}.pcm"));
        let Some(duration_ms) = pcm_from_wav(original, &pcm) else {
            return false;
        };
        let Ok(leased) = e.lease_item(id, LEASE) else {
            let _ = std::fs::remove_file(&pcm);
            return false;
        };
        let mut kept = false;
        match reader.read(&pcm, language) {
            Heard::Words(text) if !text.trim().is_empty() => {
                let t = Transcript {
                    text,
                    language: Some(language.to_string()),
                    duration_ms,
                    segments: Vec::new(),
                };
                match e.record_transcript(id, &leased.lease, t) {
                    Ok(_) => kept = true,
                    Err(x) => tracing::warn!("a voice note's words were not kept: {x}"),
                }
            }
            Heard::Words(_) | Heard::Nothing => {
                lock(&self.voice.nothing).insert(id.to_string());
            }
            Heard::Waits => {}
        }
        let _ = std::fs::remove_file(&pcm);
        if let Err(x) = e.release_lease(id, &leased.lease, false) {
            tracing::warn!("a voice note's read was not given back: {x}");
        }
        kept
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_language_tags_are_taken() {
        for good in ["en", "en-GB", "zh-Hant-TW"] {
            assert!(valid_language(good), "{good}");
        }
        for bad in ["", "e", "1en", "en_GB", "en GB", "../x", &"a".repeat(36)] {
            assert!(!valid_language(bad), "{bad}");
        }
    }
}
