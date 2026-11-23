//! Decisions asked of the model that already reads: a fixed list of answers, one reply.

use nocturne_engine::MindError;
use nocturne_engine::decide::{Answer, Decision};
use nocturne_mind::decide::{schema, system, user};
use nocturne_mind::prompts;
use serde::Deserialize;

use super::LemonadeMind;
use crate::client::{ChatRequest, Message};
use crate::npu::Work;

impl LemonadeMind {
    /// The model that reads answers. In shadow a decision must change nothing, so a failure is
    /// no strike against the NPU's mind, never loads the GPU mind in its place, and a model not
    /// on this computer is never fetched for it.
    pub(super) async fn decide_impl(&self, decision: &Decision) -> Result<Answer, MindError> {
        let model = match self.npu.as_deref().and_then(|n| n.model_for(Work::Reading)) {
            Some(model) => self.installed_model(model.to_string()).await?,
            None => self.warm_mind().await?,
        };
        self.decide_with(&model, decision).await
    }

    async fn decide_with(&self, model: &str, decision: &Decision) -> Result<Answer, MindError> {
        let nonce = prompts::nonce();
        let labels = decision.question.labels();
        let mut req = ChatRequest::new(
            model,
            vec![
                Message::system(&system(&nonce)),
                Message::user(&user(decision, &nonce)),
            ],
        )
        .sampled(&self.sampling.extract);
        req.max_tokens = Some(40);
        #[derive(Deserialize)]
        struct Reply {
            answer: String,
        }
        let reply: Reply = match self.client.chat_json(&req, &schema(&labels)).await {
            Ok(r) => r,
            Err(e) => return Err(self.map_chat_error(e).await),
        };
        let given = reply.answer.trim();
        match labels.iter().find(|l| l.as_str() == given) {
            Some(label) => Ok(Answer::certain(label, model)),
            None => Err(MindError::Failed(
                "the model answered outside the listed answers".into(),
            )),
        }
    }
}
