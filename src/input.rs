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
    serde_norway::from_str::<Value>(input).map_err(|e| diagnose_yaml_error(input, e))
}

/// Turn a raw YAML parser error into something actionable.
///
/// Two failures dominate in practice and neither is obvious from the parser's
/// own wording. Criteria are prose, and prose contains colons.
fn diagnose_yaml_error(input: &str, err: serde_norway::Error) -> anyhow::Error {
    let base = format!("input is neither valid JSON nor valid YAML: {err}");

    // `a: Use this when: the passage cites a source` parses as a nested
    // mapping and fails. This is the single most likely mistake when writing
    // criteria, because a description naturally contains ": ".
    if err.to_string().contains("mapping values are not allowed") {
        if let Some(line) = err
            .location()
            .and_then(|loc| input.lines().nth(loc.line().saturating_sub(1)))
        {
            return anyhow!(
                "{base}\n\
                 \n  {}\n\n\
                 An unquoted value cannot contain \": \", because YAML reads it as a nested\n\
                 mapping. Quote the whole value:\n\
                 \n  key: \"Use this when: the text says so\"",
                line.trim()
            );
        }
    }

    // YAML forbids tabs for indentation, and the parser reports only
    // "character that cannot start any token", which does not name the cause.
    if input
        .lines()
        .any(|l| l.starts_with('\t') || l.starts_with(" \t"))
    {
        return anyhow!("{base}\n\nYAML does not allow tabs for indentation. Use spaces.");
    }

    anyhow!("{base}")
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

/// Coerce a scalar the YAML parser typed as a number or boolean back into a
/// string.
///
/// YAML resolves an unquoted `1.5` to a number and `true` to a boolean, but the
/// API requires criteria to be text and rejects anything else with
/// `400 ... criteria.1: Invalid input`. An author who writes
///
/// ```yaml
/// levels:
///   - 1.5
/// ```
///
/// can only have meant the text "1.5", so there is nothing to disambiguate and
/// no reason to make them quote it. Note that a *quoted* "1.5" arrives here
/// already a string and is untouched, so this changes no meaning.
///
/// Objects and arrays are left alone: the API accepts structured guidance, and
/// flattening it would destroy information.
fn stringify_scalar(v: Value) -> Value {
    match v {
        Value::Number(n) => Value::String(n.to_string()),
        Value::Bool(b) => Value::String(b.to_string()),
        other => other,
    }
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
                .map(|(k, v)| (k.clone(), stringify_scalar(v.clone())))
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
                .iter()
                .cloned()
                .map(stringify_scalar)
                .collect();
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
    fn unquoted_numeric_score_level_becomes_a_string() {
        // YAML types this `1.5` as a number, and the API rejects a non-string
        // level. The author can only have meant the text, so coerce it rather
        // than making them quote it.
        let qs = parse_questions(
            "q:\n  score: Rate confidence\n  levels:\n    - low\n    - 1.5\n    - high\n",
        )
        .unwrap();
        match &qs["q"] {
            Question::Score { criteria, .. } => {
                assert_eq!(criteria[1], serde_json::json!("1.5"));
                assert!(criteria.iter().all(|c| c.is_string()));
            }
            other => panic!("expected score, got {other:?}"),
        }
    }

    #[test]
    fn unquoted_boolean_choice_description_becomes_a_string() {
        let qs = parse_questions(
            "q:\n  choice: Pick one\n  options:\n    a: true\n    b: A real description\n",
        )
        .unwrap();
        match &qs["q"] {
            Question::Choice { criteria, .. } => {
                assert_eq!(criteria["a"], serde_json::json!("true"));
            }
            other => panic!("expected choice, got {other:?}"),
        }
    }

    #[test]
    fn coercion_leaves_ordinary_strings_alone() {
        let qs = parse_questions(
            "q:\n  choice: Pick one\n  options:\n    a: A real description\n    b: \"1.5\"\n",
        )
        .unwrap();
        match &qs["q"] {
            Question::Choice { criteria, .. } => {
                assert_eq!(criteria["a"], serde_json::json!("A real description"));
                assert_eq!(criteria["b"], serde_json::json!("1.5"));
            }
            other => panic!("expected choice, got {other:?}"),
        }
    }

    /// Structured guidance must survive: the API accepts objects and arrays,
    /// and flattening them would lose information.
    #[test]
    fn coercion_does_not_touch_structured_criteria() {
        let qs = parse_questions(
            "q:\n  choice: Pick one\n  options:\n    a:\n      when: It applies here\n      note: Extra detail\n    b: Other\n",
        )
        .unwrap();
        match &qs["q"] {
            Question::Choice { criteria, .. } => assert!(criteria["a"].is_object()),
            other => panic!("expected choice, got {other:?}"),
        }
    }

    /// Criteria are prose, and prose contains colons. The raw parser message
    /// ("mapping values are not allowed in this context") does not tell an
    /// author what to do, so the fix is spelled out.
    #[test]
    fn explains_an_unquoted_colon() {
        let err = parse_questions(
            "q:\n  choice: Pick\n  options:\n    a: Use this when: the passage cites a source\n",
        )
        .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("cannot contain"), "got: {msg}");
        assert!(msg.contains("Quote the whole value"), "got: {msg}");
    }

    #[test]
    fn explains_tab_indentation() {
        let err = parse_questions("q:\n\tnoul: Is this valid?\n").unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("does not allow tabs"), "got: {msg}");
    }

    #[test]
    fn unknown_type_is_rejected() {
        let err = parse_questions(r#"{"q":{"type":"boolean","instructions":"Pick"}}"#).unwrap_err();
        assert!(err
            .chain()
            .any(|e| e.to_string().contains("unknown question type")));
    }
}
