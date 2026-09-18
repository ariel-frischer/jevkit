//! Loading question sets from YAML or JSON.
//!
//! JSON is accepted because it is what machines emit. YAML is accepted because
//! it is what people write, and because the API's own envelope is verbose:
//! measured on a two-question payload, 59% of the JSON was structural
//! (`"type"`, `"instructions"`, `"criteria"`, nesting) and only 41% was the
//! actual question text.
//!
//! So YAML input also supports a terser spelling that uses the primitive as
//! the key:
//!
//! ```yaml
//! risky:
//!   noul: Does this describe a dangerous irreversible operation?
//! severity:
//!   choice: Rate the severity of the described operation
//!   options:
//!     low: Cosmetic or easily reversed
//!     high: Data loss is possible
//! ```
//!
//! This drops the envelope, not the content. Criteria stay full sentences,
//! which matters: shortening them measurably changes answers (see `lint`).
//! The canonical `{type, instructions, criteria}` form is always accepted too.

use crate::types::{Guidance, Question};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;

/// Parse a question set from YAML or JSON.
///
/// The format is detected by trying JSON first, since every JSON document is
/// also valid YAML but the JSON parser gives better errors.
pub fn parse_questions(input: &str) -> Result<BTreeMap<String, Question>> {
    let value = parse_value(input)?;
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow!("expected a mapping of question name to question"))?;

    let mut out = BTreeMap::new();
    for (name, raw) in obj {
        let q = parse_question(raw).with_context(|| format!("question {name:?}"))?;
        out.insert(name.clone(), q);
    }
    Ok(out)
}

/// Parse any YAML or JSON document into a JSON value.
pub fn parse_value(input: &str) -> Result<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(input) {
        return Ok(v);
    }
    serde_norway::from_str::<Value>(input).context("input is neither valid JSON nor valid YAML")
}

fn parse_question(raw: &Value) -> Result<Question> {
    let obj = raw
        .as_object()
        .ok_or_else(|| anyhow!("expected a mapping"))?;

    // Canonical form: an explicit `type` discriminator.
    if let Some(kind) = obj.get("type").and_then(Value::as_str) {
        return parse_canonical(kind, obj);
    }

    // Terse form: the primitive is the key, and its value is the instructions.
    for kind in ["noul", "choice", "score"] {
        if let Some(instructions) = obj.get(kind) {
            return parse_terse(kind, instructions.clone(), obj);
        }
    }

    bail!("no question type found; expected a `type` field, or one of `noul`, `choice`, `score` as a key")
}

fn parse_canonical(kind: &str, obj: &serde_json::Map<String, Value>) -> Result<Question> {
    let instructions = obj
        .get("instructions")
        .cloned()
        .ok_or_else(|| anyhow!("`instructions` is required"))?;
    let criteria = obj.get("criteria").cloned();
    build(kind, instructions, criteria)
}

fn parse_terse(
    kind: &str,
    instructions: Value,
    obj: &serde_json::Map<String, Value>,
) -> Result<Question> {
    // Criteria may be spelled `options`, `levels`, or `criteria`, whichever
    // reads best for the primitive. Otherwise sibling keys are the criteria,
    // which keeps the common choice case to one level of nesting.
    let criteria = obj
        .get("options")
        .or_else(|| obj.get("levels"))
        .or_else(|| obj.get("criteria"))
        .cloned()
        .or_else(|| {
            let rest: serde_json::Map<String, Value> = obj
                .iter()
                .filter(|(k, _)| k.as_str() != kind)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            (!rest.is_empty()).then(|| Value::Object(rest))
        });

    build(kind, instructions, criteria)
}

fn build(kind: &str, instructions: Guidance, criteria: Option<Value>) -> Result<Question> {
    match kind {
        "noul" => Ok(Question::Noul {
            instructions,
            criteria,
        }),
        "choice" => {
            let criteria = criteria.ok_or_else(|| {
                anyhow!("`choice` requires criteria; the API rejects the request without them")
            })?;
            let map = criteria
                .as_object()
                .ok_or_else(|| {
                    anyhow!("`choice` criteria must be a mapping of option to description")
                })?
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            Ok(Question::Choice {
                instructions,
                criteria: map,
            })
        }
        "score" => {
            let criteria = criteria.ok_or_else(|| {
                anyhow!("`score` requires criteria; the API rejects the request without them")
            })?;
            let levels = criteria
                .as_array()
                .ok_or_else(|| {
                    anyhow!("`score` criteria must be a list of level descriptions, lowest first")
                })?
                .clone();
            Ok(Question::Score {
                instructions,
                criteria: levels,
            })
        }
        other => bail!("unknown question type {other:?}; expected `noul`, `choice`, or `score`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_json() {
        let qs =
            parse_questions(r#"{"risky":{"type":"noul","instructions":"Is this dangerous?"}}"#)
                .unwrap();
        assert_eq!(qs["risky"].kind(), "noul");
    }

    #[test]
    fn parses_terse_yaml() {
        let qs = parse_questions(
            "risky:\n  noul: Does this describe a dangerous irreversible operation?\n",
        )
        .unwrap();
        assert_eq!(qs["risky"].kind(), "noul");
    }

    #[test]
    fn terse_choice_uses_sibling_keys_as_criteria() {
        let qs = parse_questions(
            "severity:\n  choice: Rate severity\n  low: Cosmetic only\n  high: Data loss possible\n",
        )
        .unwrap();
        match &qs["severity"] {
            Question::Choice { criteria, .. } => {
                assert_eq!(criteria.len(), 2);
                assert_eq!(criteria["high"], serde_json::json!("Data loss possible"));
            }
            other => panic!("expected choice, got {other:?}"),
        }
    }

    #[test]
    fn terse_choice_accepts_explicit_options_block() {
        let qs = parse_questions(
            "severity:\n  choice: Rate severity\n  options:\n    low: Cosmetic only\n    high: Data loss possible\n",
        )
        .unwrap();
        match &qs["severity"] {
            Question::Choice { criteria, .. } => assert_eq!(criteria.len(), 2),
            other => panic!("expected choice, got {other:?}"),
        }
    }

    #[test]
    fn terse_score_uses_levels() {
        let qs = parse_questions(
            "urgency:\n  score: Rate urgency\n  levels:\n    - Can wait weeks\n    - Needs attention today\n",
        )
        .unwrap();
        match &qs["urgency"] {
            Question::Score { criteria, .. } => assert_eq!(criteria.len(), 2),
            other => panic!("expected score, got {other:?}"),
        }
    }

    #[test]
    fn structured_instructions_survive() {
        let qs = parse_questions(
            "q:\n  noul:\n    question: Does the message ask for a credential?\n    focus: the body\n",
        )
        .unwrap();
        assert!(qs["q"].instructions().is_object());
    }

    #[test]
    fn choice_without_criteria_is_rejected_locally() {
        let err = parse_questions(r#"{"q":{"type":"choice","instructions":"Pick"}}"#).unwrap_err();
        assert!(err
            .chain()
            .any(|e| e.to_string().contains("requires criteria")));
    }

    #[test]
    fn unknown_type_is_rejected() {
        let err = parse_questions(r#"{"q":{"type":"boolean","instructions":"Pick"}}"#).unwrap_err();
        assert!(err
            .chain()
            .any(|e| e.to_string().contains("unknown question type")));
    }
}
