//! A decision model behind the engine's `/v1/systemone`, as a [`Decider`].
//!
//! It never causes a download: the engine fetches a model it does not have on first use, so the
//! model is checked present through the model list before the first question.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use nocturne_engine::decide::{Answer, Ask, Backend, Decider, Decision};
use nocturne_engine::{BoxFuture, MindError};

use crate::client::{
    Client, Criteria, SystemOneAnswer, SystemOneKind, SystemOneQuestion, SystemOneRequest,
};

/// The one question's id in each call.
const QID: &str = "q";

pub struct SystemOneDecider {
    client: Client,
    model: String,
    backend: Backend,
    /// The engine launch (plus one) in which the model was last found present.
    checked: AtomicU64,
}

impl SystemOneDecider {
    /// `backend` is where the model runs: [`Backend::LlamaCpp`] or [`Backend::Flm`].
    pub fn new(client: Client, model: &str, backend: Backend) -> SystemOneDecider {
        SystemOneDecider {
            client,
            model: model.to_string(),
            backend,
            checked: AtomicU64::new(0),
        }
    }

    async fn present(&self) -> Result<(), MindError> {
        let launch = self.client.generation() + 1;
        if self.checked.load(Ordering::Relaxed) == launch {
            return Ok(());
        }
        let listed = self.client.models().await?;
        if !listed.iter().any(|m| m.id == self.model && m.present()) {
            return Err(MindError::Unavailable);
        }
        self.checked.store(launch, Ordering::Relaxed);
        Ok(())
    }

    async fn ask(&self, decision: &Decision) -> Result<Answer, MindError> {
        self.present().await?;
        let question = question_for(decision);
        let req = SystemOneRequest {
            model: self.model.clone(),
            state: decision.state.clone(),
            questions: BTreeMap::from([(QID.to_string(), question)]),
        };
        let reply = self.client.systemone(&req).await?;
        let answer = reply
            .answers
            .get(QID)
            .ok_or_else(|| MindError::Failed("the decision model sent no answer".into()))?;
        read_answer(decision, answer, &self.model)
    }
}

impl Decider for SystemOneDecider {
    fn decide<'a>(&'a self, decision: &'a Decision) -> BoxFuture<'a, Result<Answer, MindError>> {
        Box::pin(self.ask(decision))
    }

    fn backend(&self) -> Backend {
        self.backend
    }
}

fn question_for(decision: &Decision) -> SystemOneQuestion {
    let q = &decision.question;
    match &q.ask {
        Ask::YesNo => SystemOneQuestion {
            kind: SystemOneKind::Noul,
            instructions: q.instructions.clone(),
            criteria: None,
        },
        Ask::Choice(options) => SystemOneQuestion {
            kind: SystemOneKind::Choice,
            instructions: q.instructions.clone(),
            criteria: Some(Criteria::Options(
                options
                    .iter()
                    .map(|(option, description)| (option.clone(), Some(description.clone())))
                    .collect(),
            )),
        },
        Ask::Score(levels) => SystemOneQuestion {
            kind: SystemOneKind::Score,
            instructions: q.instructions.clone(),
            criteria: Some(Criteria::Levels(levels.clone())),
        },
    }
}

fn read_answer(
    decision: &Decision,
    answer: &SystemOneAnswer,
    model: &str,
) -> Result<Answer, MindError> {
    if let Some(error) = &answer.error {
        return Err(MindError::Failed(format!(
            "the decision model could not answer: {error}"
        )));
    }
    let missing = |what: &str| MindError::Failed(format!("the decision model sent no {what}"));
    let (label, probabilities) = match &decision.question.ask {
        Ask::YesNo => {
            let p = answer.noul.ok_or_else(|| missing("probability"))?;
            let label = if p >= 0.5 { "yes" } else { "no" };
            (
                label.to_string(),
                BTreeMap::from([("yes".to_string(), p), ("no".to_string(), 1.0 - p)]),
            )
        }
        Ask::Choice(options) => {
            let given = answer
                .probabilities
                .as_ref()
                .ok_or_else(|| missing("probabilities"))?;
            let probabilities: BTreeMap<String, f64> = options
                .iter()
                .filter_map(|(o, _)| given.get(o).map(|p| (o.clone(), *p)))
                .collect();
            let label = match &answer.choice {
                Some(c) if options.iter().any(|(o, _)| o == c) => c.clone(),
                Some(_) => {
                    return Err(MindError::Failed(
                        "the decision model chose an unknown option".into(),
                    ));
                }
                None => most_likely(&probabilities).ok_or_else(|| missing("choice"))?,
            };
            (label, probabilities)
        }
        Ask::Score(levels) => {
            let given = answer
                .probabilities
                .as_ref()
                .ok_or_else(|| missing("probabilities"))?;
            let probabilities: BTreeMap<String, f64> = levels
                .iter()
                .enumerate()
                .filter_map(|(i, level)| given.get(&i.to_string()).map(|p| (level.clone(), *p)))
                .collect();
            let label = most_likely(&probabilities).ok_or_else(|| missing("probabilities"))?;
            (label, probabilities)
        }
    };
    Ok(Answer {
        label,
        probabilities,
        by: model.to_string(),
    })
}

/// The label with the highest probability; the first one when they are level.
fn most_likely(probabilities: &BTreeMap<String, f64>) -> Option<String> {
    probabilities
        .iter()
        .fold(None::<(&String, f64)>, |best, (label, p)| match best {
            Some((_, top)) if top >= *p => best,
            _ => Some((label, *p)),
        })
        .map(|(label, _)| label.clone())
}
