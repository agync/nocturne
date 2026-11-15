//! Browsers record voice notes as WebM, Ogg or MP4, which the speech model's server does not
//! take. The preview turns them into WAV with `ffmpeg`, when the machine has it, before the app
//! keeps them. Without `ffmpeg`, or when it cannot read the recording, the original is kept
//! as it came.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

const LIMIT: Duration = Duration::from_secs(120);
/// What a recording may run to: an hour of audio and half a gigabyte of WAV.
const MAX_SECONDS: &str = "3600";
const MAX_OUTPUT_BYTES: &str = "536870912";
/// Containers browsers record in, and WAV. Nothing else is opened, whatever a file claims.
const FORMATS: &str = "matroska,webm,ogg,mov,mp4,wav";

/// The `ffmpeg` on this machine's search path, as an absolute path, found once at start so that
/// a later change to the path cannot swap it.
pub fn find_ffmpeg() -> Option<PathBuf> {
    find_in(&std::env::var_os("PATH")?, "ffmpeg")
}

fn find_in(search: &std::ffi::OsStr, name: &str) -> Option<PathBuf> {
    std::env::split_paths(search)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_absolute() && candidate.is_file())
}

/// A work folder that goes away with it, however the request ends.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

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
pub async fn to_wav(ffmpeg: &Path, work: &Path, bytes: &[u8]) -> Option<Vec<u8>> {
    let dir = work.join(super::auth::random_hex(8));
    tokio::fs::create_dir_all(&dir).await.ok()?;
    let _scratch = Scratch(dir.clone());
    let (input, output) = (dir.join("in"), dir.join("out.wav"));
    tokio::fs::write(&input, bytes).await.ok()?;
    let run = nocturne_lemonade::process::tokio_command(ffmpeg)
        .args(["-nostdin", "-loglevel", "error", "-y"])
        .args(["-protocol_whitelist", "file", "-format_whitelist", FORMATS])
        .args(["-t", MAX_SECONDS, "-i"])
        .arg(&input)
        .args([
            "-ac",
            "1",
            "-ar",
            "16000",
            "-fs",
            MAX_OUTPUT_BYTES,
            "-f",
            "wav",
        ])
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
        let Some(ffmpeg) = find_ffmpeg() else { return };
        let dir = tempfile::tempdir().unwrap();
        assert!(to_wav(&ffmpeg, dir.path(), b"not audio").await.is_none());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn a_dropped_request_leaves_nothing_behind() {
        let Some(ffmpeg) = find_ffmpeg() else { return };
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().to_path_buf();
        let task = tokio::spawn(async move { to_wav(&ffmpeg, &work, &[0u8; 4096]).await });
        tokio::time::sleep(Duration::from_millis(5)).await;
        task.abort();
        let _ = task.await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn the_program_is_found_by_absolute_path_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ffmpeg"), b"").unwrap();
        let search = std::env::join_paths([Path::new("relative"), dir.path()]).unwrap();
        assert_eq!(find_in(&search, "ffmpeg"), Some(dir.path().join("ffmpeg")));
        assert_eq!(find_in(&search, "absent"), None);
    }
}
