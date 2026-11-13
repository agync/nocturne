//! Browsers record voice notes as WebM, Ogg or MP4, which the speech model's server does not
//! take. The preview turns them into WAV with `ffmpeg`, when the machine has it, before the app
//! keeps them. Without `ffmpeg`, or when it cannot read the recording, the original is kept
//! as it came.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

const LIMIT: Duration = Duration::from_secs(120);

/// Whether a recording of this type is worth converting: audio the speech model's server does
/// not read as it is.
pub fn needs_conversion(mime: &str) -> bool {
    let essence = mime
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    essence.starts_with("audio/")
        && !matches!(
            essence.as_str(),
            "audio/wav" | "audio/x-wav" | "audio/wave" | "audio/flac" | "audio/x-flac"
        )
}

/// The recording as mono 16 kHz WAV, or `None` when it could not be made.
pub async fn to_wav(work: &Path, bytes: &[u8]) -> Option<Vec<u8>> {
    tokio::fs::create_dir_all(work).await.ok()?;
    let name = super::auth::random_hex(8);
    let (input, output) = (
        work.join(format!("{name}.in")),
        work.join(format!("{name}.wav")),
    );
    tokio::fs::write(&input, bytes).await.ok()?;
    let run = Command::new("ffmpeg")
        .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
        .arg(&input)
        .args(["-ac", "1", "-ar", "16000", "-f", "wav"])
        .arg(&output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .status();
    let done = tokio::time::timeout(LIMIT, run).await;
    let wav = match done {
        Ok(Ok(status)) if status.success() => tokio::fs::read(&output).await.ok(),
        _ => None,
    };
    let _ = tokio::fs::remove_file(&input).await;
    let _ = tokio::fs::remove_file(&output).await;
    wav.filter(|w| w.len() > 44)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_audio_the_speech_server_cannot_read_is_converted() {
        assert!(needs_conversion("audio/webm;codecs=opus"));
        assert!(needs_conversion("audio/mp4"));
        assert!(needs_conversion("Audio/Ogg"));
        assert!(!needs_conversion("audio/wav"));
        assert!(!needs_conversion("audio/flac"));
        assert!(!needs_conversion("image/png"));
    }

    #[tokio::test]
    async fn a_recording_that_cannot_be_read_yields_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(to_wav(dir.path(), b"not audio").await.is_none());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
