//! Joining the spoken sections into one WAV, with the section starts written into the file.
//!
//! The starts are a RIFF `cue ` chunk with a `LIST`/`adtl`/`labl` label per cue naming the
//! section. They live inside the audio file, so they are kept and removed with it (the store
//! removes audio files it does not account for, and a side file would go with the first sweep).
//!
//! Only what the voice produces is accepted: 16-bit PCM or 32-bit float, every part the same
//! format. A streamed file (data size `0xFFFFFFFF`) is read to its end.

use nocturne_contract::{SpokenKind, SpokenSection};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WavError {
    #[error("not a WAV file")]
    NotWav,
    #[error("the WAV file is cut short")]
    Truncated,
    #[error("that WAV format is not supported")]
    Unsupported,
    #[error("the parts are not in the same format")]
    Mismatch,
    #[error("there is nothing to join")]
    NoParts,
    #[error("the audio is too long")]
    TooLong,
}

const STREAMING: u32 = 0xFFFF_FFFF;
/// What a voice could plausibly produce: anything past this is a damaged header, and the
/// silence between sections is sized from it.
const MAX_RATE: u32 = 192_000;
const MAX_CHANNELS: u16 = 8;
/// The longest silence `join` will make between sections.
const MAX_GAP_MS: u32 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Format {
    /// 1 for PCM, 3 for float.
    tag: u16,
    channels: u16,
    rate: u32,
    bits: u16,
}

impl Format {
    fn block_align(self) -> usize {
        usize::from(self.channels) * usize::from(self.bits / 8)
    }
}

struct Parsed<'a> {
    format: Format,
    data: &'a [u8],
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The chunks of a RIFF body as (id, body). A body that runs past the end of the
/// buffer is cut at the end when the size is the streaming marker, and is an error otherwise.
type Chunk<'a> = ([u8; 4], &'a [u8]);

fn chunks(bytes: &[u8]) -> Result<Vec<Chunk<'_>>, WavError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::NotWav);
    }
    let mut out = Vec::new();
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id: [u8; 4] = bytes[at..at + 4].try_into().map_err(|_| WavError::NotWav)?;
        let size = u32_at(bytes, at + 4).ok_or(WavError::Truncated)?;
        let start = at + 8;
        let end = start + size as usize;
        let body = if size == STREAMING {
            &bytes[start..]
        } else if end > bytes.len() {
            return Err(WavError::Truncated);
        } else {
            &bytes[start..end]
        };
        out.push((id, body));
        at = start + body.len() + (body.len() & 1);
    }
    Ok(out)
}

fn parse(bytes: &[u8]) -> Result<Parsed<'_>, WavError> {
    let mut format = None;
    let mut data = None;
    for (id, body) in chunks(bytes)? {
        match &id {
            b"fmt " => {
                if body.len() < 16 {
                    return Err(WavError::Truncated);
                }
                let tag = u16_at(body, 0).ok_or(WavError::Truncated)?;
                let channels = u16_at(body, 2).ok_or(WavError::Truncated)?;
                let rate = u32_at(body, 4).ok_or(WavError::Truncated)?;
                let bits = u16_at(body, 14).ok_or(WavError::Truncated)?;
                let tag = match (tag, bits) {
                    (1 | 0xFFFE, 16) => 1,
                    (3, 32) => 3,
                    _ => return Err(WavError::Unsupported),
                };
                if channels == 0 || channels > MAX_CHANNELS || rate == 0 || rate > MAX_RATE {
                    return Err(WavError::Unsupported);
                }
                format = Some(Format {
                    tag,
                    channels,
                    rate,
                    bits,
                });
            }
            b"data" if data.is_none() => data = Some(body),
            _ => {}
        }
    }
    let format = format.ok_or(WavError::NotWav)?;
    let data = data.ok_or(WavError::NotWav)?;
    let whole = data.len() - data.len() % format.block_align();
    Ok(Parsed {
        format,
        data: &data[..whole],
    })
}

fn frames(p: &Parsed<'_>) -> u64 {
    (p.data.len() / p.format.block_align()) as u64
}

fn ms(frames: u64, rate: u32) -> u32 {
    (frames * 1000 / u64::from(rate)) as u32
}

/// How long the audio plays, in milliseconds.
pub fn duration_ms(wav: &[u8]) -> Result<u32, WavError> {
    let p = parse(wav)?;
    Ok(ms(frames(&p), p.format.rate))
}

fn label(kind: SpokenKind) -> &'static str {
    match kind {
        SpokenKind::Opening => "opening",
        SpokenKind::NeedsYou => "needsYou",
        SpokenKind::Changed => "changed",
        SpokenKind::ComingUp => "comingUp",
        SpokenKind::LetGo => "letGo",
        SpokenKind::Question => "question",
    }
}

fn kind_of(label_text: &str) -> Option<SpokenKind> {
    [
        SpokenKind::Opening,
        SpokenKind::NeedsYou,
        SpokenKind::Changed,
        SpokenKind::ComingUp,
        SpokenKind::LetGo,
        SpokenKind::Question,
    ]
    .into_iter()
    .find(|k| label(*k) == label_text)
}

fn chunk(out: &mut Vec<u8>, id: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
}

/// Builds the joined WAV one piece at a time, so the voice's answer to a request can be dropped
/// as soon as its samples are in, and only the samples are kept in between.
#[derive(Default)]
pub struct Joiner {
    format: Option<Format>,
    data: Vec<u8>,
    starts: Vec<(SpokenKind, u64)>,
    at_frame: u64,
}

impl Joiner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `wav` after `gap_ms` of silence (none before the first piece). `starts_section`
    /// marks where it begins a section of `kind`; a piece that does not continues the section
    /// before it.
    pub fn push(
        &mut self,
        kind: SpokenKind,
        starts_section: bool,
        wav: &[u8],
        gap_ms: u32,
    ) -> Result<(), WavError> {
        let p = parse(wav)?;
        if gap_ms > MAX_GAP_MS {
            return Err(WavError::TooLong);
        }
        let format = *self.format.get_or_insert(p.format);
        if p.format != format {
            return Err(WavError::Mismatch);
        }
        if !self.starts.is_empty() {
            let gap_frames = u64::from(format.rate) * u64::from(gap_ms) / 1000;
            let silent = self.data.len() + gap_frames as usize * format.block_align();
            self.data.resize(silent, 0);
            self.at_frame += gap_frames;
        }
        if starts_section || self.starts.is_empty() {
            self.starts.push((kind, self.at_frame));
        }
        self.data.extend_from_slice(p.data);
        self.at_frame += frames(&p);
        Ok(())
    }

    /// The file and where each section starts.
    pub fn finish(self) -> Result<(Vec<u8>, Vec<SpokenSection>), WavError> {
        let first = self.format.ok_or(WavError::NoParts)?;
        let (data, starts) = (self.data, self.starts);
        let block = first.block_align();
        if data.len() > (u32::MAX / 2) as usize {
            return Err(WavError::TooLong);
        }

        let mut fmt = Vec::with_capacity(16);
        fmt.extend_from_slice(&first.tag.to_le_bytes());
        fmt.extend_from_slice(&first.channels.to_le_bytes());
        fmt.extend_from_slice(&first.rate.to_le_bytes());
        fmt.extend_from_slice(&(first.rate * block as u32).to_le_bytes());
        fmt.extend_from_slice(&(block as u16).to_le_bytes());
        fmt.extend_from_slice(&first.bits.to_le_bytes());

        let mut cue = (starts.len() as u32).to_le_bytes().to_vec();
        let mut adtl = b"adtl".to_vec();
        for (i, (kind, frame)) in starts.iter().enumerate() {
            let id = (i + 1) as u32;
            cue.extend_from_slice(&id.to_le_bytes());
            cue.extend_from_slice(&(*frame as u32).to_le_bytes());
            cue.extend_from_slice(b"data");
            cue.extend_from_slice(&0u32.to_le_bytes());
            cue.extend_from_slice(&0u32.to_le_bytes());
            cue.extend_from_slice(&(*frame as u32).to_le_bytes());
            let mut text = id.to_le_bytes().to_vec();
            text.extend_from_slice(label(*kind).as_bytes());
            text.push(0);
            chunk(&mut adtl, b"labl", &text);
        }

        let mut body = b"WAVE".to_vec();
        chunk(&mut body, b"fmt ", &fmt);
        chunk(&mut body, b"data", &data);
        chunk(&mut body, b"cue ", &cue);
        chunk(&mut body, b"LIST", &adtl);
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&(body.len() as u32).to_le_bytes());
        wav.extend_from_slice(&body);

        let sections = starts
            .iter()
            .map(|(kind, frame)| SpokenSection {
                kind: *kind,
                start_ms: ms(*frame, first.rate),
            })
            .collect();
        Ok((wav, sections))
    }
}

/// Join the parts in order into one WAV with `gap_ms` of silence between them. Returns the file
/// and where each section starts.
pub fn join(
    parts: &[(SpokenKind, Vec<u8>)],
    gap_ms: u32,
) -> Result<(Vec<u8>, Vec<SpokenSection>), WavError> {
    let mut joiner = Joiner::new();
    for (kind, bytes) in parts {
        joiner.push(*kind, true, bytes, gap_ms)?;
    }
    joiner.finish()
}

/// The section starts written by [`join`], read back from the file. A file with no cue chunk
/// has no sections.
pub fn sections_of(wav: &[u8]) -> Result<Vec<SpokenSection>, WavError> {
    let rate = parse(wav)?.format.rate;
    let mut cues: Vec<(u32, u32)> = Vec::new();
    let mut labels: Vec<(u32, String)> = Vec::new();
    for (id, body) in chunks(wav)? {
        match &id {
            b"cue " => {
                let n = u32_at(body, 0).ok_or(WavError::Truncated)? as usize;
                if n > body.len() / 24 {
                    return Err(WavError::Truncated);
                }
                for i in 0..n {
                    let at = 4 + i * 24;
                    let cue_id = u32_at(body, at).ok_or(WavError::Truncated)?;
                    let frame = u32_at(body, at + 20).ok_or(WavError::Truncated)?;
                    cues.push((cue_id, frame));
                }
            }
            b"LIST" if body.get(0..4) == Some(b"adtl") => {
                let mut at = 4;
                while at + 8 <= body.len() {
                    let size = u32_at(body, at + 4).ok_or(WavError::Truncated)? as usize;
                    let end = (at + 8 + size).min(body.len());
                    if &body[at..at + 4] == b"labl"
                        && let Some(cue_id) = u32_at(body, at + 8)
                    {
                        let text = body.get(at + 12..end).unwrap_or_default();
                        let text = text.split(|b| *b == 0).next().unwrap_or_default();
                        labels.push((cue_id, String::from_utf8_lossy(text).into_owned()));
                    }
                    at = end + (size & 1);
                }
            }
            _ => {}
        }
    }
    Ok(cues
        .into_iter()
        .filter_map(|(id, frame)| {
            let text = &labels.iter().find(|(l, _)| *l == id)?.1;
            Some(SpokenSection {
                kind: kind_of(text)?,
                start_ms: ms(u64::from(frame), rate),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mono 16-bit WAV of `seconds` of a constant tone at `rate`.
    fn tone(rate: u32, seconds: f64, level: i16) -> Vec<u8> {
        let frames = (f64::from(rate) * seconds) as usize;
        let mut data = Vec::new();
        for _ in 0..frames {
            data.extend_from_slice(&level.to_le_bytes());
        }
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&data);
        out
    }

    #[test]
    fn wav_join_offsets() {
        let parts = vec![
            (SpokenKind::Opening, tone(24_000, 1.0, 100)),
            (SpokenKind::NeedsYou, tone(24_000, 0.5, 200)),
        ];
        let (wav, sections) = join(&parts, 600).unwrap();
        assert_eq!(
            sections,
            vec![
                SpokenSection {
                    kind: SpokenKind::Opening,
                    start_ms: 0
                },
                SpokenSection {
                    kind: SpokenKind::NeedsYou,
                    start_ms: 1600
                },
            ]
        );
        assert_eq!(duration_ms(&wav).unwrap(), 2100);
        let p = parse(&wav).unwrap();
        let sample = |frame: usize| i16::from_le_bytes([p.data[frame * 2], p.data[frame * 2 + 1]]);
        assert_eq!(sample(0), 100);
        assert_eq!(sample(24_000), 0, "silence between the parts");
        assert_eq!(sample(24_000 + 14_400), 200, "the second part");
    }

    #[test]
    fn wav_cue_round_trip() {
        let parts: Vec<_> = [
            SpokenKind::Opening,
            SpokenKind::NeedsYou,
            SpokenKind::Changed,
            SpokenKind::ComingUp,
            SpokenKind::LetGo,
            SpokenKind::Question,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, k)| (k, tone(24_000, 0.25 + i as f64 * 0.125, 50)))
        .collect();
        let (wav, sections) = join(&parts, 600).unwrap();
        assert_eq!(sections.len(), 6);
        assert_eq!(sections_of(&wav).unwrap(), sections);
        let starts: Vec<u32> = sections.iter().map(|s| s.start_ms).collect();
        assert!(starts.windows(2).all(|w| w[0] < w[1]), "{starts:?}");
        assert!(
            sections_of(&tone(24_000, 0.1, 1)).unwrap().is_empty(),
            "no cue chunk"
        );
    }

    #[test]
    fn wav_streaming_size_header() {
        let mut part = tone(24_000, 1.0, 7);
        part[4..8].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        let data_at = part.windows(4).position(|w| w == b"data").unwrap();
        part[data_at + 4..data_at + 8].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        assert_eq!(duration_ms(&part).unwrap(), 1000);
        let (wav, sections) = join(&[(SpokenKind::Opening, part)], 600).unwrap();
        assert_eq!(sections.len(), 1);
        assert_eq!(duration_ms(&wav).unwrap(), 1000);
    }

    /// `tone` with the sample rate field changed.
    fn with_rate(rate: u32) -> Vec<u8> {
        let mut wav = tone(24_000, 0.1, 1);
        wav[24..28].copy_from_slice(&rate.to_le_bytes());
        wav
    }

    #[test]
    fn a_damaged_header_cannot_ask_for_a_huge_silence_or_an_absurd_format() {
        assert_eq!(
            join(&[(SpokenKind::Opening, with_rate(4_000_000_000))], 600),
            Err(WavError::Unsupported)
        );
        assert_eq!(duration_ms(&with_rate(192_001)), Err(WavError::Unsupported));
        assert!(duration_ms(&with_rate(192_000)).is_ok());
        let mut many_channels = tone(24_000, 0.1, 1);
        many_channels[22..24].copy_from_slice(&1000u16.to_le_bytes());
        assert_eq!(duration_ms(&many_channels), Err(WavError::Unsupported));
        let parts = [
            (SpokenKind::Opening, tone(24_000, 0.1, 1)),
            (SpokenKind::NeedsYou, tone(24_000, 0.1, 1)),
        ];
        assert_eq!(join(&parts, 4_000_000_000), Err(WavError::TooLong));
        assert!(join(&parts, MAX_GAP_MS).is_ok());
    }

    /// A good file with its cue and label chunks replaced.
    fn with_chunks(cue: &[u8], list: &[u8]) -> Vec<u8> {
        let (good, _) = join(&[(SpokenKind::Opening, tone(24_000, 0.1, 1))], 0).unwrap();
        let end = good.windows(4).position(|w| w == b"cue ").unwrap();
        let mut out = good[..end].to_vec();
        for (id, body) in [(b"cue ", cue), (b"LIST", list)] {
            out.extend_from_slice(id);
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(body);
        }
        let size = (out.len() - 8) as u32;
        out[4..8].copy_from_slice(&size.to_le_bytes());
        out
    }

    #[test]
    fn malformed_cue_and_label_chunks_are_errors_or_empty_never_a_panic() {
        // A count that promises more cue points than the chunk holds.
        let lying = with_chunks(&1000u32.to_le_bytes(), b"adtl");
        assert_eq!(sections_of(&lying), Err(WavError::Truncated));
        // A cue chunk cut off inside a point.
        let mut cut = 1u32.to_le_bytes().to_vec();
        cut.extend_from_slice(&[1, 0, 0]);
        assert_eq!(
            sections_of(&with_chunks(&cut, b"adtl")),
            Err(WavError::Truncated)
        );
        // A label whose size runs past the end of the list, and one with no text.
        let mut cue = 1u32.to_le_bytes().to_vec();
        cue.extend_from_slice(&[
            1, 0, 0, 0, 0, 0, 0, 0, b'd', b'a', b't', b'a', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        let mut list = b"adtl".to_vec();
        list.extend_from_slice(b"labl");
        list.extend_from_slice(&u32::MAX.to_le_bytes());
        list.extend_from_slice(&[1, 0, 0, 0]);
        assert_eq!(sections_of(&with_chunks(&cue, &list)), Ok(vec![]));
        let mut bare = b"adtl".to_vec();
        bare.extend_from_slice(b"labl\x04\0\0\0\x01\0\0\0");
        assert_eq!(sections_of(&with_chunks(&cue, &bare)), Ok(vec![]));
        // A list too short to hold a chunk header.
        assert_eq!(sections_of(&with_chunks(&cue, b"adtl\x01")), Ok(vec![]));
    }

    #[test]
    fn wav_format_mismatch_errors() {
        let a = (SpokenKind::Opening, tone(24_000, 0.1, 1));
        let b = (SpokenKind::NeedsYou, tone(16_000, 0.1, 1));
        assert_eq!(join(&[a.clone(), b], 600), Err(WavError::Mismatch));
        assert_eq!(join(&[], 600), Err(WavError::NoParts));
        assert_eq!(
            join(&[(SpokenKind::Opening, b"not audio at all".to_vec())], 0),
            Err(WavError::NotWav)
        );
        let mut eight_bit = tone(24_000, 0.1, 1);
        eight_bit[34..36].copy_from_slice(&8u16.to_le_bytes());
        assert_eq!(
            join(&[(SpokenKind::Opening, eight_bit)], 0),
            Err(WavError::Unsupported)
        );
        let mut cut = tone(24_000, 0.1, 1);
        cut.truncate(cut.len() - 10);
        assert_eq!(
            join(&[(SpokenKind::Opening, cut)], 0),
            Err(WavError::Truncated)
        );
    }
}
