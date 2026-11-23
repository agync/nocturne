//! The words a decision is asked in: a fixed list of answers, one reply.

use nocturne_engine::decide::{Ask, Decision};
use serde_json::{Value, json};

use crate::prompts;

pub fn schema(labels: &[String]) -> Value {
    json!({
        "type": "object",
        "properties": {"answer": {"type": "string", "enum": labels}},
        "required": ["answer"],
        "additionalProperties": false,
    })
}

pub fn system(nonce: &str) -> String {
    format!(
        "You answer one question about a short text someone kept. Reply with JSON only: answer, \
         exactly one of the listed answers. Judge only from the text; never guess beyond it.\n{}",
        prompts::untrusted("text", nonce)
    )
}

pub fn user(decision: &Decision, nonce: &str) -> String {
    let q = &decision.question;
    let mut out = format!(
        "{}\n\nQuestion: {}\n\nAnswers:\n",
        prompts::fence("text", nonce, &decision.state),
        q.instructions
    );
    match &q.ask {
        Ask::YesNo => out.push_str("- yes\n- no\n"),
        Ask::Choice(options) => {
            for (option, description) in options {
                out.push_str(&format!("- {option}: {description}\n"));
            }
        }
        Ask::Score(levels) => {
            for level in levels {
                out.push_str(&format!("- {level}\n"));
            }
        }
    }
    out
}
