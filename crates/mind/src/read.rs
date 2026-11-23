//! Reading an item into memories, whoever runs the model: the requests a reading makes, asked
//! again for what was wrong, read again in halves when a reply loops, is cut off or is full,
//! and the second look at lines an earlier reading left out. A runner gives one reply at a
//! time through [`Reads`]; everything said to the model is decided here.

use nocturne_engine::{BoxFuture, Candidate, ExtractContext, MindError};

use crate::prompts;
use crate::reading::{Parsed, covered, halves, merged, parse_candidates, salvage};

const EXTRACT_ATTEMPTS: usize = 2;
/// How many times a reading cut off at its token ceiling is halved and read again in parts.
const MAX_SPLITS: u32 = 2;
/// The ceiling every reading had before a short note got a lower one.
const ROOMY: u32 = 1024;

/// Where a model's raw replies are traced, for evaluation runs: at trace level on a target of
/// its own, so a log kept at info never holds the words of a memory.
pub const REPLY_TARGET: &str = "nocturne::reply";

/// The token ceiling for reading `text`: room for a memory or two per sentence, and no more,
/// so a reply that loops stops early. A short note gets about six tokens a character (a memory
/// takes some 130, and a dense note holds one every 20 characters or so), never under 512.
pub fn extract_tokens(text: &str) -> u32 {
    let chars = text.chars().count();
    let long = (512 + chars * 3 / 2).clamp(1024, 3072);
    (384 + chars * 6).clamp(512, long) as u32
}

/// Traces a raw reply at `step` (see [`REPLY_TARGET`]).
pub fn trace_reply(step: &str, reply: &str) {
    tracing::trace!(target: REPLY_TARGET, step, reply = %reply);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

/// One message of a reading conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub role: Role,
    pub text: String,
}

impl Turn {
    pub fn system(text: String) -> Turn {
        Turn {
            role: Role::System,
            text,
        }
    }

    pub fn user(text: String) -> Turn {
        Turn {
            role: Role::User,
            text,
        }
    }
}

/// A reading reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reply {
    /// What to read memories from: the whole reply, or the part before it began to loop.
    pub text: String,
    /// The whole reply as it came, for the reply trace and the conversation.
    pub full: String,
    /// The model was stopped at the token ceiling.
    pub cut: bool,
    /// It was ended early because it began to loop ([`crate::watch`]).
    pub looped: bool,
}

/// Whoever runs the model for a reading.
pub trait Reads: Send + Sync {
    /// One reply to `turns` reading `item`, of at most `ceiling` tokens, by the extraction
    /// grammar or its JSON schema, watched as it comes and ended once it loops.
    fn reply<'a>(
        &'a self,
        turns: &'a [Turn],
        item: &'a str,
        ceiling: u32,
    ) -> BoxFuture<'a, Result<Reply, MindError>>;
}

/// The memories in `reply` to `text`, the parser's words kept as text.
fn parsed(reply: &str, text: &str) -> Result<Parsed, String> {
    parse_candidates(reply, text).map_err(|e| format!("an unexpected reply: {e}"))
}

/// Reads `text` into memories. See the module notes.
pub async fn extract(
    r: &dyn Reads,
    text: &str,
    cx: &ExtractContext,
) -> Result<Vec<Candidate>, MindError> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    extract_part(r, text, cx, 0, false).await
}

/// One reading of `text`, asked again once for the memories that were not valid. A reply
/// that loops, is cut off at its ceiling, or is as full as a reply may be, is kept when its
/// whole memories keep every fact of `text`; otherwise `text` is read again in halves,
/// `depth` times at most. `roomy` gives a short note the ceiling every reading had before
/// short notes got a lower one.
fn extract_part<'a>(
    r: &'a dyn Reads,
    text: &'a str,
    cx: &'a ExtractContext,
    depth: u32,
    roomy: bool,
) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
    Box::pin(async move {
        let nonce = prompts::nonce();
        let mut turns = vec![
            Turn::system(prompts::extraction_system(&nonce)),
            Turn::user(prompts::extraction_user(text, cx, &nonce)),
        ];
        let ceiling = extract_tokens(text);
        let ceiling = if roomy { ceiling.max(ROOMY) } else { ceiling };
        let mut last_error = String::new();
        // What a reply got right is kept; a retry asks again only for what was wrong.
        let mut kept: Vec<Candidate> = Vec::new();
        for attempt in 0..EXTRACT_ATTEMPTS {
            let reply = r.reply(&turns, text, ceiling).await?;
            trace_reply(
                if depth == 0 {
                    "extract"
                } else {
                    "extract_half"
                },
                &reply.full,
            );
            let parsed = parsed(&reply.text, text);
            let cut = !reply.looped
                && (reply.cut || parsed.as_ref().is_err_and(|e| e.contains("EOF while")));
            let full = match &parsed {
                Ok(p) if p.total >= prompts::MAX_MEMORIES => Some(p.items.clone()),
                _ => None,
            };
            if reply.looped || cut || full.is_some() {
                let whole = merged(kept, full.unwrap_or_else(|| salvage(&reply.text, text)));
                if covered(text, &whole, cx) {
                    tracing::info!(
                        "a reading that looped, was cut off or was full kept every fact; \
                         not reading it in parts"
                    );
                    return Ok(whole);
                }
                // A short note cut off at its lower ceiling, and that cannot be split, is
                // read once more with the room every note had before.
                let roomier = cut && !roomy && extract_tokens(text) < ROOMY;
                return read_in_parts(r, text, cx, depth, roomier, whole).await;
            }
            match parsed {
                Ok(p) if p.invalid.is_empty() => return Ok(merged(kept, p.items)),
                Ok(p) if attempt + 1 == EXTRACT_ATTEMPTS => {
                    tracing::warn!(
                        "dropped {} invalid memories: {:?}",
                        p.invalid.len(),
                        p.invalid
                    );
                    return Ok(merged(kept, p.items));
                }
                Ok(p) => {
                    kept = merged(kept, p.items);
                    last_error = p.invalid.join("; ");
                }
                Err(e) if attempt + 1 == EXTRACT_ATTEMPTS => {
                    if !kept.is_empty() {
                        tracing::warn!("the retry was unreadable; keeping the first answer");
                        return Ok(kept);
                    }
                    return Err(MindError::Failed(format!("unusable extraction: {e}")));
                }
                Err(e) => {
                    // The parser's words can quote the model; the retry names none of them.
                    tracing::warn!("an unreadable extraction: {e}");
                    last_error = "the reply did not match the format".into();
                }
            }
            turns.push(Turn {
                role: Role::Assistant,
                text: reply.full,
            });
            turns.push(Turn::user(format!(
                "That was not valid: {last_error}. Reply again with JSON only, matching the schema, \
                 with just those memories corrected; the others are kept."
            )));
        }
        Err(MindError::Failed(format!(
            "unusable extraction: {last_error}"
        )))
    })
}

/// A reply to `text` looped, was cut off, or held as many memories as a reply may, and left a
/// fact out: each half of `text` is read on its own. When it cannot be split again, it is read
/// once more with more room if `roomier`, or else `whole` (the complete memories the reply
/// held) is kept.
async fn read_in_parts(
    r: &dyn Reads,
    text: &str,
    cx: &ExtractContext,
    depth: u32,
    roomier: bool,
    whole: Vec<Candidate>,
) -> Result<Vec<Candidate>, MindError> {
    let halves = if depth < MAX_SPLITS {
        halves(text)
    } else {
        None
    };
    let Some((first, second)) = halves else {
        if roomier {
            tracing::warn!("a reading was cut off; reading the item again with more room");
            return match extract_part(r, text, cx, depth, true).await {
                Ok(found) => Ok(merged(whole, found)),
                Err(MindError::Failed(_)) if !whole.is_empty() => Ok(whole),
                Err(e) => Err(e),
            };
        }
        tracing::warn!(
            "a reading was cut off or full; kept {} whole memories",
            whole.len()
        );
        if whole.is_empty() {
            return Err(MindError::Failed("the reading was cut off".into()));
        }
        return Ok(whole);
    };
    tracing::warn!("a reading was cut off or full; reading the item in two parts");
    let mut out = whole;
    let mut failed = None;
    for part in [first, second] {
        match extract_part(r, part, cx, depth + 1, false).await {
            Ok(found) => out = merged(out, found),
            Err(MindError::Failed(why)) => failed = Some(why),
            Err(e) => return Err(e),
        }
    }
    match failed {
        Some(why) if out.is_empty() => Err(MindError::Failed(why)),
        _ => Ok(out),
    }
}

/// Another look at `text` for the facts in `lines`: the whole item, with those lines pointed
/// out. A reply that is cut off or full has the lines asked about in halves, once.
pub async fn second_look(
    r: &dyn Reads,
    text: &str,
    cx: &ExtractContext,
    lines: &[String],
) -> Result<Vec<Candidate>, MindError> {
    if lines.is_empty() || text.trim().is_empty() {
        return Ok(Vec::new());
    }
    focused(r, text, cx, lines, 0).await
}

fn focused<'a>(
    r: &'a dyn Reads,
    text: &'a str,
    cx: &'a ExtractContext,
    lines: &'a [String],
    depth: u32,
) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
    Box::pin(async move {
        let nonce = prompts::nonce();
        let turns = [
            Turn::system(prompts::extraction_system(&nonce)),
            Turn::user(prompts::focus_user(text, cx, &nonce, lines)),
        ];
        let reply = r
            .reply(&turns, text, extract_tokens(&lines.join("\n")))
            .await?;
        trace_reply("extract_second_look", &reply.full);
        if reply.looped {
            return Ok(salvage(&reply.text, text));
        }
        let parsed = parsed(&reply.text, text);
        let cut = reply.cut || parsed.as_ref().is_err_and(|e| e.contains("EOF while"));
        let full = parsed
            .as_ref()
            .is_ok_and(|p| p.total >= prompts::MAX_MEMORIES);
        if (cut || full) && depth == 0 && lines.len() > 1 {
            let (a, b) = lines.split_at(lines.len() / 2);
            let mut out = Vec::new();
            for part in [a, b] {
                out = merged(out, focused(r, text, cx, part, 1).await?);
            }
            return Ok(out);
        }
        match parsed {
            Ok(p) => Ok(p.items),
            Err(_) if cut => Ok(salvage(&reply.text, text)),
            Err(e) => Err(MindError::Failed(format!("unusable second look: {e}"))),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_ceiling_grows_with_the_item_and_stays_bounded() {
        assert_eq!(extract_tokens("Sam likes tea"), 512);
        assert_eq!(extract_tokens(&"x".repeat(69)), 798);
        assert_eq!(extract_tokens(&"x".repeat(112)), 1024);
        assert_eq!(extract_tokens(&"x".repeat(1000)), 2012);
        assert_eq!(extract_tokens(&"x".repeat(6000)), 3072);
    }
}
