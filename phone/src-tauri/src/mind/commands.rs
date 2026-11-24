//! The screens' way to a phone alone's mind: the check, turning on and off, and what is chosen.
//! Each wraps [`PhoneMind`]; the key goes in once, with turning on, and never comes back out.

use nocturne_contract::{CheckOutcome, ModelList};
use nocturne_provider::address::UrlProblem;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::mind::{Chosen, PhoneMind, ProviderSettings};

/// The form's three details, as typed. Its debug form leaves the key out.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderForm {
    pub url: String,
    pub key: String,
    pub model: String,
}

impl std::fmt::Debug for ProviderForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderForm")
            .field("url", &self.url)
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

impl From<ProviderForm> for ProviderSettings {
    fn from(f: ProviderForm) -> Self {
        ProviderSettings {
            url: f.url,
            key: f.key,
            model: f.model,
        }
    }
}

/// What is chosen, as the screens read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum MindChosen {
    Off,
    On { host: String, model: String },
    NeedsKey { host: String, model: String },
}

impl From<Chosen> for MindChosen {
    fn from(c: Chosen) -> Self {
        match c {
            Chosen::Off => MindChosen::Off,
            Chosen::On { host, model } => MindChosen::On { host, model },
            Chosen::NeedsKey { host, model } => MindChosen::NeedsKey { host, model },
        }
    }
}

/// Why turning on did not happen, as the screens name it.
fn problem(p: UrlProblem) -> &'static str {
    match p {
        UrlProblem::NotHttps => "notHttps",
        UrlProblem::NotAllowed => "addressNotAllowed",
        UrlProblem::Invalid | UrlProblem::NoHost | UrlProblem::Extra => "invalid",
    }
}

/// The connection check for the form: nothing kept, nothing turned on.
#[tauri::command]
pub async fn mind_check(
    mind: State<'_, PhoneMind>,
    form: ProviderForm,
) -> Result<CheckOutcome, String> {
    Ok(mind.check(&form.into()).await)
}

/// One look at the service's list with the form's address and key, at the person's request.
#[tauri::command]
pub async fn mind_list(
    mind: State<'_, PhoneMind>,
    form: ProviderForm,
) -> Result<ModelList, String> {
    Ok(mind.list(&form.into()).await)
}

/// Turns the mind on with the form, in place of any chosen before.
#[tauri::command]
pub fn mind_turn_on(mind: State<'_, PhoneMind>, form: ProviderForm) -> Result<(), String> {
    mind.turn_on(form.into())
        .map_err(|p| problem(p).to_string())
}

/// Turns the mind off and forgets the key.
#[tauri::command]
pub fn mind_turn_off(mind: State<'_, PhoneMind>) {
    mind.turn_off();
}

#[tauri::command]
pub fn mind_chosen(mind: State<'_, PhoneMind>) -> MindChosen {
    mind.chosen().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chosen_reads_as_the_screens_expect() {
        assert_eq!(
            serde_json::to_value(MindChosen::from(Chosen::Off)).unwrap(),
            json!({ "state": "off" })
        );
        assert_eq!(
            serde_json::to_value(MindChosen::from(Chosen::On {
                host: "answers.example".into(),
                model: "reader-1".into(),
            }))
            .unwrap(),
            json!({ "state": "on", "host": "answers.example", "model": "reader-1" })
        );
        assert_eq!(
            serde_json::to_value(MindChosen::from(Chosen::NeedsKey {
                host: "answers.example".into(),
                model: "reader-1".into(),
            }))
            .unwrap(),
            json!({ "state": "needsKey", "host": "answers.example", "model": "reader-1" })
        );
    }

    #[test]
    fn the_form_reads_from_the_screens() {
        let form: ProviderForm = serde_json::from_value(
            json!({ "url": "https://answers.example/v1", "key": "k", "model": "reader-1" }),
        )
        .unwrap();
        let settings = ProviderSettings::from(form);
        assert_eq!(settings.url, "https://answers.example/v1");
        assert_eq!(settings.key, "k");
        assert_eq!(settings.model, "reader-1");
    }

    #[test]
    fn the_key_stays_out_of_the_debug_form() {
        let form = ProviderForm {
            url: "https://answers.example/v1".into(),
            key: "secret-key-123".into(),
            model: "reader-1".into(),
        };
        assert!(!format!("{form:?}").contains("secret-key-123"));
    }

    #[test]
    fn each_address_problem_has_a_name() {
        assert_eq!(problem(UrlProblem::NotHttps), "notHttps");
        assert_eq!(problem(UrlProblem::NotAllowed), "addressNotAllowed");
        for p in [UrlProblem::Invalid, UrlProblem::NoHost, UrlProblem::Extra] {
            assert_eq!(problem(p), "invalid");
        }
    }

    #[test]
    fn turning_on_with_a_plain_http_address_online_is_refused_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let mind = PhoneMind::new(
            dir.path().to_path_buf(),
            Box::new(crate::mind::MemoryKeys::default()),
            std::sync::Arc::new(nocturne_engine::ManualClock::at(
                "2026-11-22T09:00:00Z",
                "Europe/London",
            )),
        );
        let refused = mind
            .turn_on(ProviderSettings {
                url: "http://answers.example/v1".into(),
                key: "k".into(),
                model: "reader-1".into(),
            })
            .map_err(problem);
        assert_eq!(refused, Err("notHttps"));
        assert_eq!(MindChosen::from(mind.chosen()), MindChosen::Off);
    }
}
