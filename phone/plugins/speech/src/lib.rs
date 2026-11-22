//! The phone's own speech reader: a kept voice note, as raw sound in a file, read into words on the
//! phone, offline, and never from the microphone. The app's Rust side calls it; the screens never
//! do. Only Android has it so far; on a computer and on an iPhone every ask is unavailable.
//!
//! Every ask blocks until the phone has answered, which can take as long as the sound plays, so it
//! belongs on a worker thread, never the Android main thread (the plugin works there). One ask
//! runs at a time; the others wait.
//!
//! The sound is 16-bit, mono, 16 kHz, with no header, in a file of the app's own. Reading a file
//! still asks for the microphone permission, because the phone's reader does.

use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{Manager, Runtime};

/// Whether the app may use the microphone permission the phone's reader asks for. `Prompt` is also
/// what a phone says once it will no longer show its prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Access {
    Granted,
    Denied,
    Prompt,
    PromptWithRationale,
}

#[derive(Debug, Deserialize)]
struct Status {
    microphone: Access,
}

/// What one ask to the phone's reader came to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum Outcome {
    /// A file of this language can be read now.
    Ready,
    Text {
        text: String,
    },
    /// The phone has no sound pack for the language. It is never fetched here.
    LanguageUnavailable,
    NotSupported,
    NoPermission,
    /// The file read, and held no words.
    NothingHeard,
    /// The reader did not stop at the end of the file, so it may be listening to the room.
    ListensToMic,
    Failed,
}

impl Outcome {
    /// The words, when there are any.
    pub fn words(&self) -> Option<&str> {
        match self {
            Outcome::Text { text } => Some(text),
            _ => None,
        }
    }

    /// The note is left waiting: reading it is not possible now, and may be later.
    pub fn waits(&self) -> bool {
        !matches!(
            self,
            Outcome::Ready | Outcome::Text { .. } | Outcome::NothingHeard
        )
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Read<'a> {
    path: &'a str,
    language: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Error {
    /// No speech reader on this platform.
    Unavailable,
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

pub struct Speech<R: Runtime>(Option<PluginHandle<R>>);

impl<R: Runtime> Speech<R> {
    #[cfg(mobile)]
    fn run<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        args: impl Serialize,
    ) -> Result<T, Error> {
        let handle = self.0.as_ref().ok_or(Error::Unavailable)?;
        handle
            .run_mobile_plugin(method, args)
            .map_err(|e| Error::Failed(e.to_string()))
    }

    #[cfg(not(mobile))]
    fn run<T>(&self, _method: &str, _args: impl Serialize) -> Result<T, Error> {
        let _ = &self.0;
        Err(Error::Unavailable)
    }

    pub fn access(&self) -> Access {
        self.run::<Status>("checkPermissions", ())
            .map_or(Access::Denied, |s| s.microphone)
    }

    /// Shows the phone's own prompt, unless it will no longer show it; answers what it says.
    pub fn request(&self) -> Result<Access, Error> {
        self.run::<Status>(
            "requestPermissions",
            serde_json::json!({ "permissions": ["microphone"] }),
        )
        .map(|s| s.microphone)
    }

    /// Whether a file in `language` (a tag such as `en-US`) can be read now. It runs the check that
    /// the phone's reader does not listen to the room, so on a reader that fails it the microphone
    /// may be open for a second.
    pub fn check_support(&self, language: &str) -> Result<Outcome, Error> {
        self.run("checkSupport", Read { path: "", language })
    }

    /// Reads the sound file at `path` in `language`.
    pub fn transcribe(&self, path: &str, language: &str) -> Result<Outcome, Error> {
        self.run("transcribe", Read { path, language })
    }
}

pub trait SpeechExt<R: Runtime> {
    fn speech(&self) -> &Speech<R>;
}

impl<R: Runtime, T: Manager<R>> SpeechExt<R> for T {
    fn speech(&self) -> &Speech<R> {
        self.state::<Speech<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("nocturne-speech")
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            let handle = Some(_api.register_android_plugin("app.nocturne.speech", "SpeechPlugin")?);
            #[cfg(not(target_os = "android"))]
            let handle: Option<PluginHandle<R>> = None;
            app.manage(Speech(handle));
            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(json: &str) -> Outcome {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn words_come_with_the_text_outcome() {
        let got = outcome(r#"{"outcome":"text","text":"the purple giraffe"}"#);
        assert_eq!(got.words(), Some("the purple giraffe"));
        assert!(!got.waits());
    }

    #[test]
    fn every_outcome_of_the_phone_side_reads() {
        for (wire, waits) in [
            ("ready", false),
            ("languageUnavailable", true),
            ("notSupported", true),
            ("noPermission", true),
            ("nothingHeard", false),
            ("listensToMic", true),
            ("failed", true),
        ] {
            let got = outcome(&format!(r#"{{"outcome":"{wire}"}}"#));
            assert_eq!(got.waits(), waits, "{wire}");
            assert_eq!(got.words(), None, "{wire}");
        }
    }

    #[test]
    fn access_reads_as_the_phone_says_it() {
        let s: Status = serde_json::from_str(r#"{"microphone":"prompt-with-rationale"}"#).unwrap();
        assert_eq!(s.microphone, Access::PromptWithRationale);
    }

    #[test]
    fn the_ask_names_the_file_and_the_language() {
        let json = serde_json::to_value(Read {
            path: "voice/note.pcm",
            language: "en-US",
        })
        .unwrap();
        assert_eq!(json["path"], "voice/note.pcm");
        assert_eq!(json["language"], "en-US");
    }
}
