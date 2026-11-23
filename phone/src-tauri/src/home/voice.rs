//! Voice notes in words, on the phone itself, with no network. After a voice note is kept, and
//! when the app opens, each note still waiting is read by the phone's own speech reader: the
//! item is leased, its words recorded, and the lease given back, so it stays waiting for a mind
//! that may read it better. Only when the reader's check passes for the phone's language, and
//! only with the permission already given: nothing here asks for it. Where the reader cannot,
//! the note stays waiting to be read.
//!
//! The reader takes raw sound (16-bit, mono, 16 kHz, no header). A note kept as such a WAV file
//! is read as it is; any other recording is decoded by the phone first, and waits where the phone
//! cannot decode it. A note longer than [`PIECE`] is read in pieces, each cut where it is
//! quietest near its end, and its words are kept only when every piece could be read.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use nocturne_contract::{ItemFilter, ItemKind, ItemStatus};
use nocturne_engine::Transcript;

use super::{Home, lock};

/// How long reading a note may take beyond three times its length before the lease runs out.
const LEASE: Duration = Duration::from_secs(120);
/// Lease time for each piece, for the reader's checks around each read.
const PER_PIECE: Duration = Duration::from_secs(15);
const RATE: u32 = 16_000;
const BYTES_PER_SECOND: u64 = RATE as u64 * 2;
/// The longest stretch the reader is given at once: an iPhone stops at about a minute.
pub const PIECE: Duration = Duration::from_secs(50);
/// How far back from a piece's end a quiet place to cut it is looked for.
const LOOK_BACK: Duration = Duration::from_secs(5);
/// The stretch whose loudness is weighed when looking for a quiet place: 20 ms.
const FRAME: usize = RATE as usize / 50;
/// The longest language tag the screens may send.
const MAX_TAG: usize = 35;

/// The title of a voice note that has not been heard.
pub const VOICE_NOTE: &str = "Voice note";
/// A voice note's title is cut at a word before this many characters.
const TITLE_CHARS: usize = 60;

/// The first words of what was heard, cut at a word before about 60 characters, as a note's
/// title is.
pub fn spoken_title(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let whole = words.join(" ");
    if whole.chars().count() <= TITLE_CHARS {
        return whole;
    }
    let mut title = String::new();
    for word in words {
        let next = if title.is_empty() {
            word.to_string()
        } else {
            format!("{title} {word}")
        };
        if next.chars().count() > TITLE_CHARS {
            break;
        }
        title = next;
    }
    if title.is_empty() {
        title = whole.chars().take(TITLE_CHARS).collect();
    }
    let title = title.trim_end_matches([',', ';', ':', '-']).to_string();
    format!("{title}\u{2026}")
}

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
    /// Writes a recording that is not plain WAV sound to `dest` as the raw sound `read` takes;
    /// answers how long it plays. `None` where the phone cannot decode it.
    fn decode(&self, _src: &Path, _dest: &Path) -> Option<u32> {
        None
    }
}

#[derive(Default)]
pub struct Voice {
    language: Mutex<Option<String>>,
    running: AtomicBool,
    /// Notes read this run that gave no words to keep: not read again until the app starts.
    nothing: Mutex<HashSet<String>>,
    /// The reader said it cannot read the phone's language now.
    unready: AtomicBool,
}

impl Voice {
    /// Whether the phone's own reader has had its turn at a voice note still without words:
    /// it found none, could not decode it, or cannot read the phone's language. Until then a
    /// mind leaves the note to it.
    pub fn turn_over(&self, id: &str) -> bool {
        self.unready.load(Ordering::SeqCst) || lock(&self.nothing).contains(id)
    }
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

/// The start of the quietest whole frame of `samples`, in samples.
fn quietest(samples: &[i16]) -> usize {
    samples
        .as_chunks::<FRAME>()
        .0
        .iter()
        .enumerate()
        .min_by_key(|(_, frame)| frame.iter().map(|&s| i64::from(s).pow(2)).sum::<i64>())
        .map_or(0, |(i, _)| i * FRAME)
}

/// The byte ranges of raw sound `len` bytes long to read one at a time: none longer than
/// [`PIECE`], each ending in the middle of the quietest frame of its last [`LOOK_BACK`].
/// `window(at, bytes)` gives the samples from byte `at`.
fn pieces(
    len: u64,
    mut window: impl FnMut(u64, usize) -> std::io::Result<Vec<i16>>,
) -> std::io::Result<Vec<(u64, u64)>> {
    let piece = PIECE.as_secs() * BYTES_PER_SECOND;
    let back = LOOK_BACK.as_secs() * BYTES_PER_SECOND;
    let mut out = Vec::new();
    let mut start = 0;
    while len - start > piece {
        let from = start + piece - back;
        let samples = window(from, back as usize)?;
        let cut = from + ((quietest(&samples) + FRAME / 2) * 2) as u64;
        out.push((start, cut));
        start = cut;
    }
    out.push((start, len));
    Ok(out)
}

fn samples_at(file: &mut std::fs::File, at: u64, bytes: usize) -> std::io::Result<Vec<i16>> {
    let mut buf = vec![0u8; bytes];
    file.seek(SeekFrom::Start(at))?;
    file.read_exact(&mut buf)?;
    Ok(buf
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&b| i16::from_le_bytes(b))
        .collect())
}

/// Bytes `range` of `src`, written to `dest`.
fn copy_range(src: &mut std::fs::File, range: (u64, u64), dest: &Path) -> std::io::Result<()> {
    src.seek(SeekFrom::Start(range.0))?;
    let mut out = std::fs::File::create(dest)?;
    std::io::copy(&mut src.take(range.1 - range.0), &mut out)?;
    out.flush()
}

/// The pieces raw sound at `pcm` is read in.
fn pieces_of(pcm: &Path) -> std::io::Result<Vec<(u64, u64)>> {
    let mut file = std::fs::File::open(pcm)?;
    let len = file.metadata()?.len();
    pieces(len, |at, bytes| samples_at(&mut file, at, bytes))
}

/// Read raw sound in `ranges`; words only when every piece could be read. Before each piece,
/// `stop` may say to leave it waiting.
fn read_in_pieces(
    reader: &dyn Reader,
    pcm: &Path,
    ranges: &[(u64, u64)],
    language: &str,
    stop: &dyn Fn() -> bool,
) -> Heard {
    if let [_] = ranges {
        if stop() {
            return Heard::Waits;
        }
        return reader.read(pcm, language);
    }
    let Ok(mut file) = std::fs::File::open(pcm) else {
        return Heard::Waits;
    };
    let mut words: Vec<String> = Vec::new();
    for (i, range) in ranges.iter().enumerate() {
        if stop() {
            return Heard::Waits;
        }
        let part = pcm.with_extension(format!("{i}.pcm"));
        let heard = match copy_range(&mut file, *range, &part) {
            Ok(()) => reader.read(&part, language),
            Err(_) => Heard::Waits,
        };
        let _ = std::fs::remove_file(&part);
        match heard {
            Heard::Words(text) if !text.trim().is_empty() => words.push(text.trim().to_string()),
            Heard::Words(_) | Heard::Nothing => {}
            Heard::Waits => return Heard::Waits,
        }
    }
    if words.is_empty() {
        Heard::Nothing
    } else {
        Heard::Words(words.join(" "))
    }
}

/// Raw sound left behind by a pass the app was stopped in.
fn sweep_sound(scratch: &Path) {
    for entry in std::fs::read_dir(scratch).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|x| x == "pcm") {
            let _ = std::fs::remove_file(path);
        }
    }
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
            let listed = match self.using() {
                Ok((_use, open)) => open.engine.items(kept),
                Err(_) => break,
            };
            let waiting: Vec<_> = match listed {
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
                let ready = reader.ready(&language);
                self.voice.unready.store(!ready, Ordering::SeqCst);
                if !ready {
                    break;
                }
                asked = true;
            }
            let scratch = self.keeping_dir();
            if std::fs::create_dir_all(&scratch).is_err() {
                break;
            }
            if seen.is_empty() {
                sweep_sound(&scratch);
            }
            for (id, original) in waiting {
                if self.store_wanted() {
                    return heard;
                }
                seen.insert(id.clone());
                if self.hear_one(reader, &language, &id, &original, &scratch) {
                    heard += 1;
                }
            }
        }
        heard
    }

    /// Read one note; answers whether its words were kept. The store is held only to lease the
    /// note and to keep its words, never while the reader reads: a move or a restore waits for
    /// no piece, and no new piece starts while one waits or runs.
    fn hear_one(
        &self,
        reader: &dyn Reader,
        language: &str,
        id: &str,
        original: &Path,
        scratch: &Path,
    ) -> bool {
        let pcm = scratch.join(format!("{id}.pcm"));
        let decoded = pcm_from_wav(original, &pcm).or_else(|| reader.decode(original, &pcm));
        let Some(duration_ms) = decoded else {
            let _ = std::fs::remove_file(&pcm);
            // The original may have moved with a store being swapped: tried again next time.
            if !self.store_wanted() {
                // Not tried again this run: the phone cannot decode it.
                lock(&self.voice.nothing).insert(id.to_string());
            }
            return false;
        };
        let Ok(ranges) = pieces_of(&pcm) else {
            let _ = std::fs::remove_file(&pcm);
            lock(&self.voice.nothing).insert(id.to_string());
            return false;
        };
        let lease = LEASE
            + Duration::from_millis(u64::from(duration_ms) * 3)
            + PER_PIECE * ranges.len() as u32;
        let leased = self.using().ok().and_then(|(_use, open)| {
            let leased = open.engine.lease_item(id, lease).ok()?;
            Some((leased, self.generation()))
        });
        let Some((leased, store)) = leased else {
            let _ = std::fs::remove_file(&pcm);
            return false;
        };
        let heard = read_in_pieces(reader, &pcm, &ranges, language, &|| self.store_wanted());
        let _ = std::fs::remove_file(&pcm);
        // The store may have changed places while the reader read: another store knows nothing
        // of this lease, and its note is read in a later pass.
        let Ok((_use, open)) = self.using() else {
            return false;
        };
        if self.generation() != store {
            return false;
        }
        let e = &open.engine;
        let mut kept = false;
        match heard {
            Heard::Words(text) if !text.trim().is_empty() => {
                let title = spoken_title(&text);
                let t = Transcript {
                    text,
                    language: Some(language.to_string()),
                    duration_ms,
                    segments: Vec::new(),
                };
                match e.record_phone_transcript(id, &leased.lease, t) {
                    Ok(_) => {
                        kept = true;
                        // A note recorded here is "Voice note" until it is heard; one shared
                        // with a name of its own keeps that name.
                        if let Err(x) = e.retitle(id, VOICE_NOTE, &title) {
                            tracing::warn!("a voice note's title was not changed: {x}");
                        }
                    }
                    Err(x) => {
                        // Not tried again this run: a note that outlasts its lease would be
                        // read on every return to the front.
                        tracing::warn!("a voice note's words were not kept: {x}");
                        lock(&self.voice.nothing).insert(id.to_string());
                    }
                }
            }
            Heard::Words(_) | Heard::Nothing => {
                lock(&self.voice.nothing).insert(id.to_string());
            }
            Heard::Waits => {}
        }
        if let Err(x) = e.release_lease(id, &leased.lease, false) {
            tracing::warn!("a voice note's read was not given back: {x}");
        }
        kept
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, loud: i16) -> Vec<i16> {
        (0..n)
            .map(|i| if i % 2 == 0 { loud } else { -loud })
            .collect()
    }

    #[test]
    fn the_quietest_frame_is_found() {
        let mut samples = tone(FRAME * 10, 8000);
        samples[FRAME * 6..FRAME * 7].fill(3);
        assert_eq!(quietest(&samples), FRAME * 6);
        assert_eq!(quietest(&[]), 0);
    }

    #[test]
    fn a_short_note_is_one_piece() {
        let len = 30 * BYTES_PER_SECOND;
        let got = pieces(len, |_, _| panic!("no cut needed")).unwrap();
        assert_eq!(got, vec![(0, len)]);
        let exact = PIECE.as_secs() * BYTES_PER_SECOND;
        assert_eq!(
            pieces(exact, |_, _| unreachable!()).unwrap(),
            vec![(0, exact)]
        );
    }

    #[test]
    fn a_long_note_is_cut_where_it_is_quiet() {
        let secs = 130;
        let mut sound = tone(secs * RATE as usize, 6000);
        // A pause at 47.5 s, inside the first look-back; none inside the second.
        let pause = 47 * RATE as usize + RATE as usize / 2;
        sound[pause..pause + FRAME].fill(0);
        let len = sound.len() as u64 * 2;
        let got = pieces(len, |at, bytes| {
            let at = at as usize / 2;
            Ok(sound[at..at + bytes / 2].to_vec())
        })
        .unwrap();
        let piece = PIECE.as_secs() * BYTES_PER_SECOND;
        assert_eq!(got.len(), 3, "{got:?}");
        assert_eq!(got[0].0, 0);
        assert_eq!(got[0].1, (pause + FRAME / 2) as u64 * 2, "cut in the pause");
        for (a, b) in &got {
            assert!(b - a <= piece, "{a}..{b}");
            assert_eq!(a % 2, 0);
        }
        for pair in got.windows(2) {
            assert_eq!(pair[0].1, pair[1].0, "no sound lost or read twice");
        }
        assert_eq!(got.last().unwrap().1, len);
    }

    struct Pieces(std::sync::Mutex<Vec<Heard>>, std::sync::Mutex<Vec<u64>>);

    impl Reader for Pieces {
        fn ready(&self, _: &str) -> bool {
            true
        }

        fn read(&self, pcm: &Path, _: &str) -> Heard {
            lock(&self.1).push(std::fs::metadata(pcm).unwrap().len());
            lock(&self.0).remove(0)
        }
    }

    fn long_pcm(dir: &Path, secs: u64) -> PathBuf {
        let pcm = dir.join("n.pcm");
        let bytes: Vec<u8> = tone((secs * u64::from(RATE)) as usize, 4000)
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect();
        std::fs::write(&pcm, bytes).unwrap();
        pcm
    }

    #[test]
    fn the_words_of_every_piece_are_joined() {
        let dir = tempfile::tempdir().unwrap();
        let pcm = long_pcm(dir.path(), 110);
        let reader = Pieces(
            std::sync::Mutex::new(vec![
                Heard::Words(" first part ".into()),
                Heard::Nothing,
                Heard::Words("last part".into()),
            ]),
            std::sync::Mutex::default(),
        );
        let ranges = pieces_of(&pcm).unwrap();
        assert_eq!(
            read_in_pieces(&reader, &pcm, &ranges, "en-GB", &|| false),
            Heard::Words("first part last part".into())
        );
        let sizes = lock(&reader.1).clone();
        assert_eq!(sizes.len(), 3);
        assert_eq!(sizes.iter().sum::<u64>(), 110 * BYTES_PER_SECOND);
        let left: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(left.len(), 1, "only the note itself is left");
    }

    #[test]
    fn one_piece_that_cannot_be_read_leaves_the_note_waiting() {
        let dir = tempfile::tempdir().unwrap();
        let pcm = long_pcm(dir.path(), 70);
        let reader = Pieces(
            std::sync::Mutex::new(vec![Heard::Words("half".into()), Heard::Waits]),
            std::sync::Mutex::default(),
        );
        let ranges = pieces_of(&pcm).unwrap();
        assert_eq!(
            read_in_pieces(&reader, &pcm, &ranges, "en-GB", &|| false),
            Heard::Waits
        );
    }

    #[test]
    fn a_read_stops_between_pieces_when_the_store_is_wanted() {
        let dir = tempfile::tempdir().unwrap();
        let pcm = long_pcm(dir.path(), 70);
        let reader = Pieces(
            std::sync::Mutex::new(vec![Heard::Words("half".into())]),
            std::sync::Mutex::default(),
        );
        let ranges = pieces_of(&pcm).unwrap();
        let asked = std::sync::atomic::AtomicUsize::new(0);
        let stop = || asked.fetch_add(1, Ordering::SeqCst) >= 1;
        assert_eq!(
            read_in_pieces(&reader, &pcm, &ranges, "en-GB", &stop),
            Heard::Waits
        );
        assert_eq!(lock(&reader.1).len(), 1, "the second piece is never read");
    }

    #[test]
    fn sound_left_by_a_stopped_pass_is_swept() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.pcm"), b"x").unwrap();
        std::fs::write(dir.path().join("a.1.pcm"), b"x").unwrap();
        std::fs::write(dir.path().join("note.webm"), b"x").unwrap();
        sweep_sound(dir.path());
        let left: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().collect();
        assert_eq!(left.len(), 1);
    }

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
