//! How long an audio file is, worked out from its first and last bytes before anything is
//! uploaded, so a very long recording is refused cheaply.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// A guess at the length in seconds. WAV, FLAC and Ogg (Opus or Vorbis) are read from their
/// headers; anything else is sized at 16 kB per second, a generous bitrate, so a file is never
/// judged longer than it is by much and a low-bitrate one is judged shorter.
pub fn estimate_seconds(path: &Path) -> f64 {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    probe(path, size).unwrap_or(size as f64 / 16_000.0)
}

fn probe(path: &Path, size: u64) -> Option<f64> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = vec![0u8; 4096];
    let n = file.read(&mut head).ok()?;
    head.truncate(n);
    if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WAVE") {
        return wav(&head, size);
    }
    if head.starts_with(b"fLaC") {
        return flac(&head);
    }
    if head.starts_with(b"OggS") {
        let tail_len = size.min(65_536);
        file.seek(SeekFrom::Start(size - tail_len)).ok()?;
        let mut tail = vec![0u8; tail_len as usize];
        file.read_exact(&mut tail).ok()?;
        return ogg(&head, &tail);
    }
    None
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn le16(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_le_bytes(
        b.get(at..at + 2)?.try_into().ok()?,
    )))
}

/// The data size in the header is trusted only as far as the file goes: a WAV written as a
/// stream (sizes 0xFFFFFFFF) or cut short runs to the end of the file.
fn wav(head: &[u8], file_len: u64) -> Option<f64> {
    let mut pos = 12;
    let (mut rate, mut channels, mut bits) = (0u32, 0u32, 0u32);
    while pos + 8 <= head.len() {
        let id = &head[pos..pos + 4];
        let size = le32(head, pos + 4)?;
        if id == b"fmt " {
            channels = le16(head, pos + 10)?;
            rate = le32(head, pos + 12)?;
            bits = le16(head, pos + 22)?;
        }
        if id == b"data" {
            let per_second = f64::from(rate) * f64::from(channels) * f64::from(bits) / 8.0;
            let bytes = u64::from(size).min(file_len.saturating_sub(pos as u64 + 8));
            return (per_second > 0.0).then(|| bytes as f64 / per_second);
        }
        pos += 8 + size as usize + (size as usize & 1);
    }
    None
}

/// FLAC's STREAMINFO block holds the sample rate and the total sample count.
fn flac(head: &[u8]) -> Option<f64> {
    let info = head.get(8..8 + 34)?;
    let rate =
        (u32::from(info[10]) << 12) | (u32::from(info[11]) << 4) | (u32::from(info[12]) >> 4);
    let total = (u64::from(info[13] & 0x0f) << 32)
        | u64::from(u32::from_be_bytes(info[14..18].try_into().ok()?));
    (rate > 0 && total > 0).then(|| total as f64 / f64::from(rate))
}

/// The last Ogg page's granule position counts samples; the rate comes from the first page.
fn ogg(head: &[u8], tail: &[u8]) -> Option<f64> {
    let rate = if find(head, b"OpusHead").is_some() {
        48_000.0
    } else {
        let at = find(head, b"\x01vorbis")?;
        f64::from(le32(head, at + 7 + 5)?)
    };
    let last = rfind(tail, b"OggS")?;
    let granule = i64::from_le_bytes(tail.get(last + 6..last + 14)?.try_into().ok()?);
    (rate > 0.0 && granule > 0).then(|| granule as f64 / rate)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(name: &str, bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    pub fn wav_bytes(seconds: u32) -> Vec<u8> {
        let rate = 16_000u32;
        let data = seconds * rate * 2;
        let mut v = b"RIFF".to_vec();
        v.extend((36 + data).to_le_bytes());
        v.extend(b"WAVEfmt ");
        v.extend(16u32.to_le_bytes());
        v.extend([1, 0, 1, 0]);
        v.extend(rate.to_le_bytes());
        v.extend((rate * 2).to_le_bytes());
        v.extend([2, 0, 16, 0]);
        v.extend(b"data");
        v.extend(data.to_le_bytes());
        v
    }

    fn flac_bytes(rate: u32, total: u64) -> Vec<u8> {
        let mut v = b"fLaC".to_vec();
        v.extend([0x00, 0, 0, 34]);
        v.extend([0u8; 10]);
        v.push((rate >> 12) as u8);
        v.push((rate >> 4) as u8);
        v.push(((rate & 0x0f) << 4) as u8);
        v.push((total >> 32) as u8 & 0x0f);
        v.extend((total as u32).to_be_bytes());
        v.extend([0u8; 16]);
        v
    }

    fn ogg_page(granule: i64, payload: &[u8]) -> Vec<u8> {
        let mut v = b"OggS\0\x02".to_vec();
        v.extend(granule.to_le_bytes());
        v.extend([0u8; 12]);
        v.extend(payload);
        v
    }

    #[test]
    fn reads_wav_flac_and_ogg_headers() {
        let mut whole = wav_bytes(90);
        whole.resize(whole.len() + 90 * 16_000 * 2, 0);
        let (_d, p) = write("a.wav", &whole);
        assert!((estimate_seconds(&p) - 90.0).abs() < 1e-6);
        let (_d, p) = write("a.flac", &flac_bytes(44_100, 44_100 * 3600));
        assert!((estimate_seconds(&p) - 3600.0).abs() < 1e-6);
        let mut opus = ogg_page(0, b"OpusHead\x01\x02");
        opus.extend(ogg_page(48_000 * 7300, b"audio"));
        let (_d, p) = write("a.opus", &opus);
        assert!((estimate_seconds(&p) - 7300.0).abs() < 1e-6);
        let mut vorbis = ogg_page(0, b"\x01vorbis\0\0\0\0\x02\x44\xac\0\0");
        vorbis.extend(ogg_page(44_100 * 100, b"audio"));
        let (_d, p) = write("a.ogg", &vorbis);
        assert!((estimate_seconds(&p) - 100.0).abs() < 1e-6);
    }

    /// The bundled voice streams its WAV: 32-bit float at 24 kHz, with both sizes 0xFFFFFFFF
    /// because it does not know the length when it writes the header.
    fn streamed_wav(seconds: u32) -> Vec<u8> {
        let rate = 24_000u32;
        let mut v = b"RIFF".to_vec();
        v.extend(u32::MAX.to_le_bytes());
        v.extend(b"WAVEfmt ");
        v.extend(16u32.to_le_bytes());
        v.extend([3, 0, 1, 0]);
        v.extend(rate.to_le_bytes());
        v.extend((rate * 4).to_le_bytes());
        v.extend([4, 0, 32, 0]);
        v.extend(b"data");
        v.extend(u32::MAX.to_le_bytes());
        v.extend(vec![0u8; (seconds * rate * 4) as usize]);
        v
    }

    #[test]
    fn a_streamed_wav_is_as_long_as_its_file() {
        let (_d, p) = write("voice.wav", &streamed_wav(9));
        assert!((estimate_seconds(&p) - 9.0).abs() < 1e-6);
        // A header that claims more than the file holds is held to the file too.
        let mut short = wav_bytes(3600);
        short.extend(vec![0u8; 16_000 * 2 * 5]);
        let (_d, p) = write("cut.wav", &short);
        assert!((estimate_seconds(&p) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn other_containers_fall_back_to_the_size() {
        let (_d, p) = write("a.mp3", &vec![0u8; 160_000]);
        assert!((estimate_seconds(&p) - 10.0).abs() < 1e-6);
        assert_eq!(estimate_seconds(std::path::Path::new("/no/such/file")), 0.0);
    }
}
