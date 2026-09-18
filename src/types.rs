//! Request and response types for the Jev decisions API.
//!
//! The shapes here mirror OpenRouter's published component model
//! (`DecisionsRequest`, `DecisionsChoiceQuestion`, and friends). Two details
//! are load-bearing and easy to get wrong:
//!
//! 1. Jev rejects `/v1/chat/completions`. Decisions go to
//!    `/api/alpha/decisions` with a `{model, state, questions}` body.
//!
//! 2. **A `noul` answer is a probability in `[0,1]`, not a boolean.** Treating
//!    it as truthy makes every answer "yes". There is deliberately no
//!    `as_bool()` helper: callers must pick a threshold explicitly.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The Jev release this client is verified against.
pub const DEFAULT_MODEL: &str = "typesafe/jev-1.13";

/// Jev's advertised context window, in tokens.
pub const MAX_CONTEXT_TOKENS: usize = 32_000;

/// Free-form guidance. The API accepts a plain string, an object, or an array
/// for both `state` and `instructions`, so this is kept as raw JSON rather
/// than narrowed to a string.
pub type Guidance = serde_json::Value;

/// One typed question about the state.
///
/// Serialized with an internal `type` tag, which is the discriminator the
/// server's validator keys on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Question {
    /// Yes/no judgment returning a probability. `criteria` is optional, but
    /// supplying `{true, false}` descriptions measurably sharpens borderline
    /// answers.
    Noul {
        instructions: Guidance,
        #[serde(skip_serializing_if = "Option::is_none")]
        criteria: Option<Guidance>,
    },
    /// Categorical judgment over a known label set. `criteria` maps each label
    /// to a description of when it applies and is **required**.
    ///
    /// The descriptions are not decoration: they are the prompt. Passing bare
    /// labels (`{"analogy": "analogy"}`) measurably changes the answer.
    Choice {
        instructions: Guidance,
        criteria: BTreeMap<String, Guidance>,
    },
    /// Position on an ordered scale. `criteria` lists level descriptions from
    /// low to high and is **required**.
    Score {
        instructions: Guidance,
        criteria: Vec<Guidance>,
    },
}

impl Question {
    /// The wire name of this variant, for diagnostics.
    pub fn kind(&self) -> &'static str {
        match self {
            Question::Noul { .. } => "noul",
            Question::Choice { .. } => "choice",
            Question::Score { .. } => "score",
        }
    }

    pub fn instructions(&self) -> &Guidance {
        match self {
            Question::Noul { instructions, .. }
            | Question::Choice { instructions, .. }
            | Question::Score { instructions, .. } => instructions,
        }
    }
}

/// A decisions request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub model: String,
    pub state: Guidance,
    pub questions: BTreeMap<String, Question>,
    /// Groups related requests for observability. Never sent to the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// One decision.
///
/// Note the absence of a `confidence` field on [`Answer::Noul`]: the API does
/// not return one for that primitive. Derive uncertainty from the
/// probability's distance from 0.5.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Answer {
    Noul {
        noul: f64,
    },
    Choice {
        choice: String,
        #[serde(default)]
        probabilities: BTreeMap<String, f64>,
        #[serde(default)]
        confidence: Option<f64>,
    },
    Score {
        score: f64,
        #[serde(default)]
        legend: BTreeMap<String, serde_json::Value>,
        #[serde(default)]
        probabilities: BTreeMap<String, f64>,
        #[serde(default)]
        confidence: Option<f64>,
    },
}

/// Token counts and cost for one call.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cost: f64,
}

/// A decisions response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub model: String,
    pub answers: BTreeMap<String, Answer>,
    #[serde(default)]
    pub usage: Usage,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub provider: String,
}

/// A rough character-based token estimate, for checking a request against
/// [`MAX_CONTEXT_TOKENS`] before spending a call.
pub fn estimate_tokens(s: &str) -> usize {
    s.len() / 4 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noul_omits_absent_criteria() {
        let q = Question::Noul {
            instructions: serde_json::json!("Is this dangerous?"),
            criteria: None,
        };
        let out = serde_json::to_string(&q).unwrap();
        assert_eq!(
            out,
            r#"{"type":"noul","instructions":"Is this dangerous?"}"#
        );
    }

    #[test]
    fn choice_round_trips_with_criteria() {
        let raw = r#"{"type":"choice","instructions":"Rate","criteria":{"low":"cosmetic"}}"#;
        let q: Question = serde_json::from_str(raw).unwrap();
        assert_eq!(q.kind(), "choice");
        assert_eq!(serde_json::to_string(&q).unwrap(), raw);
    }

    #[test]
    fn instructions_accept_structured_objects() {
        let raw = r#"{"type":"noul","instructions":{"question":"Is it?","focus":"the body"}}"#;
        let q: Question = serde_json::from_str(raw).unwrap();
        assert!(q.instructions().is_object());
    }

    #[test]
    fn noul_answer_is_a_probability_not_a_bool() {
        let raw = r#"{"type":"noul","noul":0.97}"#;
        match serde_json::from_str::<Answer>(raw).unwrap() {
            Answer::Noul { noul } => assert!((noul - 0.97).abs() < f64::EPSILON),
            other => panic!("expected noul, got {other:?}"),
        }
    }

    #[test]
    fn score_answer_parses_legend() {
        let raw = r#"{"type":"score","score":0.0,"legend":{"0":"only one level"},"probabilities":{"0":1.0},"confidence":1.0}"#;
        match serde_json::from_str::<Answer>(raw).unwrap() {
            Answer::Score { legend, .. } => assert_eq!(legend.len(), 1),
            other => panic!("expected score, got {other:?}"),
        }
    }
}
