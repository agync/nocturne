//! Recordings the provider does not take as they are (m4a, aac, webm) go as WAV, decoded by the
//! phone itself, as its speech reader's are.

use std::path::Path;
use std::sync::Arc;

use nocturne_provider::Decode;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_nocturne_speech::SpeechExt;

/// The sample rate the phone's decoder writes, mono, 16-bit.
const RATE: u32 = 16_000;

/// Raw 16-bit mono sound at [`RATE`] as a WAV file.
pub fn wav_of_pcm(pcm: &[u8]) -> Vec<u8> {
    let len = pcm.len() as u32;
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&len.to_le_bytes());
    wav.extend_from_slice(pcm);
    wav
}

/// The phone's decoder, writing its sound to the app's cache for as long as it is read.
pub fn decoder<R: Runtime>(app: &AppHandle<R>) -> Decode {
    let app = app.clone();
    Arc::new(move |src: &Path| {
        let dir = app.path().app_cache_dir().ok()?.join("mind-sound");
        std::fs::create_dir_all(&dir).ok()?;
        let name = format!(
            "{}.pcm",
            nocturne_channel::crypto::random::<8>()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let pcm = dir.join(name);
        let decoded = app
            .speech()
            .decode(src.to_str()?, pcm.to_str()?)
            .ok()
            .flatten();
        let bytes = decoded.and_then(|_| std::fs::read(&pcm).ok());
        let _ = std::fs::remove_file(&pcm);
        bytes.map(|b| wav_of_pcm(&b))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_sound_reads_back_as_the_phones_own_wav() {
        let pcm: Vec<u8> = (0..64u8).collect();
        let wav = wav_of_pcm(&pcm);
        let dir = tempfile::tempdir().unwrap();
        let (src, dest) = (dir.path().join("a.wav"), dir.path().join("a.pcm"));
        std::fs::write(&src, &wav).unwrap();
        assert_eq!(crate::home::voice::pcm_from_wav(&src, &dest), Some(2));
        assert_eq!(std::fs::read(&dest).unwrap(), pcm);
    }
}
