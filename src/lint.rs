//! Offline validation of a question set.
//!
//! Two classes of problem, and the second is the reason this module exists.
//!
//! **Schema errors** are what the server rejects: a missing `criteria` on a
//! choice, an unknown `type`. Catching them locally turns a billed round trip
//! into an instant error.
//!
//! **Semantic defects** are questions the server happily accepts, bills for,
//! and answers uselessly. These cannot be caught by any schema. Every rule
//! below was derived from an observed failure, not from taste:
//!
//! - A `score` with one level returned `score: 0, confidence: 1`. A `choice`
//!   with one option returned that option at probability 1. Both are forced
//!   answers carrying no information, and both cost money.
//! - Bare-label criteria (`{"analogy": "analogy"}`) versus written
//!   descriptions flipped a verdict on identical input: `analogy` at 0.75
//!   became `popular_opinion` at 0.76 once the labels were described. The
//!   criteria text *is* the prompt.
//! - Conditional phrasing ("answer yes if not applicable") returns values
//!   clustered near 0.5, which is indistinguishable from genuine uncertainty.
//!
//! Findings are advisory by default: this lints style, and style rules are
//! heuristics. `--deny warnings` makes them fatal for CI.

use crate::types::{
    estimate_tokens_len, serialized_len, Guidance, Question, Request, MAX_CONTEXT_TOKENS,
};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Certain to be rejected by the API.
    Error,
    /// Accepted and billed, but very likely not what was meant.
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    /// Stable rule identifier, e.g. `degenerate-criteria`.
    pub rule: &'static str,
    /// Dotted path to the offending value, e.g. `questions.scheme.criteria`.
    pub path: String,
    pub message: String,
    /// What to do about it. Omitted when the message is self-evident.
    pub help: Option<String>,
}

impl Finding {
    fn error(rule: &'static str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Finding {
            severity: Severity::Error,
            rule,
            path: path.into(),
            message: message.into(),
            help: None,
        }
    }

    fn warn(rule: &'static str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Finding {
            severity: Severity::Warning,
            rule,
            path: path.into(),
            message: message.into(),
            help: None,
        }
    }

    fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

/// Words that make a question conditional on its own applicability. Measured:
/// these return probabilities bunched around 0.5 regardless of the input.
const ESCAPE_HATCHES: [&str; 5] = [
    "if applicable",
    "if not applicable",
    "if relevant",
    "if any",
    "where applicable",
];

/// Options that let a Choice decline to fit the input. A closed label set
/// forces a wrong pick on out-of-domain input.
const ESCAPE_OPTIONS: [&str; 6] = ["other", "none", "unknown", "n/a", "na", "none_of_the_above"];

/// Lint a full request.
pub fn lint_request(req: &Request) -> Vec<Finding> {
    let mut findings = lint_questions(&req.questions);

    if req.questions.is_empty() {
        findings.push(Finding::error(
            "no-questions",
            "questions",
            "request has no questions",
        ));
    }

    if let Some(text) = req.state.as_str() {
        if text.trim().is_empty() {
            findings.push(Finding::error("empty-state", "state", "state is empty"));
        }
    }

    // The whole payload counts against the context window, not just the state.
    //
    // `estimate_tokens` is a crude chars/4 heuristic, and measurement showed it
    // running high: a payload it estimated at 38,751 tokens was reported by the
    // API as 31,272 actual tokens, a 24% overestimate, and the call succeeded.
    // An error there would have blocked a request that works.
    //
    // So the hard error is reserved for payloads that cannot fit under any
    // plausible tokenization, and anything merely near the limit is a warning.
    // Being wrong in the warning direction costs a stderr line; being wrong in
    // the error direction costs the user a call they were entitled to make.
    // Serialize to a counting sink instead of a String: a state can be tens
    // of kilobytes, and building a full copy of the payload just to measure
    // it was the largest allocation on the ask path.
    let payload_len = serialized_len(req);
    let tokens = estimate_tokens_len(payload_len);
    // Even a pathological worst case does not compress below ~1 token per
    // character, so an estimate above 4x the limit cannot possibly fit.
    if tokens > MAX_CONTEXT_TOKENS * 4 {
        findings.push(
            Finding::error(
                "context-overflow",
                "state",
                format!(
                    "roughly {tokens} estimated tokens cannot fit the {MAX_CONTEXT_TOKENS}-token limit"
                ),
            )
            .with_help("Split the state, or drop questions that do not need the full context."),
        );
    } else if tokens > MAX_CONTEXT_TOKENS {
        findings.push(
            Finding::warn(
                "context-pressure",
                "state",
                format!(
                    "roughly {tokens} estimated tokens may exceed the {MAX_CONTEXT_TOKENS}-token limit"
                ),
            )
            .with_help(
                "The estimate is chars/4 and measured ~24% high on prose, so this may still fit. \
                 Check `usage.input_tokens` with --raw if you need the real figure.",
            ),
        );
    }

    findings.sort_by(|a, b| a.severity.cmp(&b.severity).then(a.path.cmp(&b.path)));
    findings
}

/// Lint a bare question set, with no state. Used by `jev lint` on a question
/// file that has not been paired with input yet.
pub fn lint_questions(questions: &BTreeMap<String, Question>) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (name, q) in questions {
        let path = format!("questions.{name}");
        lint_instructions(&path, q, &mut findings);
        match q {
            Question::Noul { criteria, .. } => lint_noul(&path, criteria.as_ref(), &mut findings),
            Question::Choice { criteria, .. } => lint_choice(&path, criteria, &mut findings),
            Question::Score { criteria, .. } => lint_score(&path, criteria, &mut findings),
        }
    }
    findings
}

fn lint_instructions(path: &str, q: &Question, findings: &mut Vec<Finding>) {
    let ipath = format!("{path}.instructions");
    let text = guidance_text(q.instructions());

    if text.trim().is_empty() {
        findings.push(Finding::error(
            "empty-instructions",
            &ipath,
            format!("{} question has empty instructions", q.kind()),
        ));
        return;
    }

    let lowered = text.to_lowercase();

    if let Some(hatch) = ESCAPE_HATCHES.iter().find(|h| lowered.contains(*h)) {
        findings.push(
            Finding::warn(
                "conditional-question",
                &ipath,
                format!("conditional phrasing {hatch:?} makes the answer ambiguous"),
            )
            .with_help(
                "Measured: escape hatches return probabilities near 0.5 regardless of input, \
                 which is indistinguishable from genuine uncertainty. Ask positively and gate \
                 the question in code instead.",
            ),
        );
    }

    // A question that weighs two properties gets a blurred answer, and the
    // caller cannot tell which half drove it.
    if lowered.contains(" and ") && matches!(q, Question::Noul { .. }) {
        findings.push(
            Finding::warn(
                "compound-question",
                &ipath,
                "instructions appear to ask about two properties at once",
            )
            .with_help("Split into separate questions and combine the answers in code."),
        );
    }

    if text.split_whitespace().count() < 3 {
        findings.push(
            Finding::warn(
                "terse-instructions",
                &ipath,
                format!("instructions are very short: {text:?}"),
            )
            .with_help(
                "State the exact condition being judged; the model answers the words you wrote.",
            ),
        );
    }
}

fn lint_noul(path: &str, criteria: Option<&Guidance>, findings: &mut Vec<Finding>) {
    let Some(criteria) = criteria else { return };
    let Some(map) = criteria.as_object() else {
        findings.push(
            Finding::error(
                "noul-criteria-shape",
                format!("{path}.criteria"),
                "noul criteria must be an object with `true` and `false` keys",
            )
            .with_help("Omit criteria entirely if you do not need them."),
        );
        return;
    };
    for key in ["true", "false"] {
        if !map.contains_key(key) {
            findings.push(Finding::warn(
                "noul-criteria-incomplete",
                format!("{path}.criteria"),
                format!("noul criteria omit the {key:?} case"),
            ));
        }
    }
}

fn lint_choice(path: &str, criteria: &BTreeMap<String, Guidance>, findings: &mut Vec<Finding>) {
    let cpath = format!("{path}.criteria");

    if criteria.is_empty() {
        findings.push(Finding::error(
            "missing-criteria",
            &cpath,
            "choice requires criteria; the API rejects the request without them",
        ));
        return;
    }

    if criteria.len() == 1 {
        let only = criteria.keys().next().unwrap();
        findings.push(
            Finding::warn(
                "single-option",
                &cpath,
                format!("only one option ({only:?}), so the answer is forced"),
            )
            .with_help(
                "Observed: a one-option choice returns that option at probability 1.0 with \
                 confidence 1.0. The call is billed and tells you nothing.",
            ),
        );
    }

    lint_bare_labels(
        &cpath,
        criteria.iter().map(|(k, v)| (k.as_str(), v)),
        findings,
    );

    // YAML reads an unquoted `1.5` or `true` as a number or boolean, and the
    // API rejects a non-string description: 400 "questions.q.criteria.low:
    // Invalid input".
    for (label, desc) in criteria {
        if desc.is_number() || desc.is_boolean() {
            findings.push(
                Finding::error(
                    "non-string-criteria",
                    format!("{cpath}.{label}"),
                    format!("description for {label:?} is {desc}, not text; the API rejects it"),
                )
                .with_help("Quote the value, and better still describe when this option applies."),
            );
        }
    }

    let has_escape = criteria
        .keys()
        .any(|k| ESCAPE_OPTIONS.contains(&k.to_lowercase().trim()));
    if !has_escape && criteria.len() > 1 {
        findings.push(
            Finding::warn(
                "no-escape-option",
                &cpath,
                "no `other` or `none` option, so out-of-domain input is forced into a label",
            )
            .with_help("Add an explicit `none` option describing when nothing fits."),
        );
    }
}

fn lint_score(path: &str, criteria: &[Guidance], findings: &mut Vec<Finding>) {
    let cpath = format!("{path}.criteria");

    if criteria.is_empty() {
        findings.push(Finding::error(
            "missing-criteria",
            &cpath,
            "score requires criteria; the API rejects the request without them",
        ));
        return;
    }

    if criteria.len() < 2 {
        findings.push(
            Finding::warn(
                "single-level",
                &cpath,
                "a score needs at least two levels; one level forces the answer",
            )
            .with_help(
                "Observed: a one-level score returns `score: 0` at confidence 1.0. \
                 The call is billed and tells you nothing.",
            ),
        );
    }

    if criteria.len() > 10 {
        findings.push(Finding::warn(
            "too-many-levels",
            &cpath,
            format!(
                "{} levels; the API documents a maximum of 10",
                criteria.len()
            ),
        ));
    }

    for (i, level) in criteria.iter().enumerate() {
        let text = guidance_text(level);
        if text.trim().is_empty() {
            findings.push(Finding::error(
                "empty-level",
                format!("{cpath}[{i}]"),
                "level description is empty",
            ));
        } else if level.is_number() || level.is_boolean() {
            // A bare `1.5` in YAML deserializes to a JSON number, and the API
            // rejects a non-string level outright: 400 "questions.q.criteria.1:
            // Invalid input". This is a hard error, unlike a quoted "1.5",
            // which the API accepts and which is merely poor practice.
            findings.push(
                Finding::error(
                    "non-string-level",
                    format!("{cpath}[{i}]"),
                    format!("level {i} is {text}, not text; the API rejects a non-string level"),
                )
                .with_help(
                    "YAML reads a bare 1.5 or true as a number or boolean. Quote it, and better \
                     still describe what the level means.",
                ),
            );
        } else if text.trim().parse::<f64>().is_ok() {
            findings.push(
                Finding::warn(
                    "numeric-level",
                    format!("{cpath}[{i}]"),
                    format!("level {i} is the bare number {text:?}"),
                )
                .with_help(
                    "Describe what the level means; a numeral gives the model nothing to match.",
                ),
            );
        }
    }
}

/// Flag criteria whose description merely restates the label.
///
/// This is the highest-value rule here. Measured on identical input: bare
/// labels chose `analogy` (0.75); written descriptions chose
/// `popular_opinion` (0.76). The verdict inverted.
fn lint_bare_labels<'a>(
    cpath: &str,
    entries: impl Iterator<Item = (&'a str, &'a Guidance)>,
    findings: &mut Vec<Finding>,
) {
    let mut bare = Vec::new();
    for (label, desc) in entries {
        let text = guidance_text(desc);
        let normalized = text.trim().to_lowercase().replace(['_', '-'], " ");
        let label_norm = label.trim().to_lowercase().replace(['_', '-'], " ");
        if normalized.is_empty() || normalized == label_norm {
            bare.push(label.to_string());
        }
    }
    if !bare.is_empty() {
        findings.push(
            Finding::warn(
                "degenerate-criteria",
                cpath,
                format!(
                    "{} option(s) restate the label instead of describing it: {}",
                    bare.len(),
                    bare.join(", ")
                ),
            )
            .with_help(
                "Criteria text is the prompt. Measured on identical input, bare labels chose \
                 `analogy` (0.75) where written descriptions chose `popular_opinion` (0.76): \
                 the verdict inverted. Describe when each option applies.",
            ),
        );
    }
}

/// Flatten guidance to searchable text. Objects and arrays are accepted by the
/// API, so lint rules have to see through them.
///
/// Borrows the inner text where possible (`Cow`) instead of rebuilding a
/// String; the common case is a plain string and that costs no allocation.
fn guidance_text(g: &Guidance) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    match g {
        serde_json::Value::String(s) => Cow::Borrowed(s),
        serde_json::Value::Array(items) => match items.len() {
            0 => Cow::Borrowed(""),
            1 => guidance_text(&items[0]),
            _ => Cow::Owned(
                items
                    .iter()
                    .map(|v| guidance_text(v).into_owned())
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
        },
        serde_json::Value::Object(map) => match map.len() {
            0 => Cow::Borrowed(""),
            1 => guidance_text(map.values().next().unwrap()),
            _ => Cow::Owned(
                map.values()
                    .map(|v| guidance_text(v).into_owned())
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
        },
        serde_json::Value::Null => Cow::Borrowed(""),
        other => Cow::Owned(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn questions(raw: serde_json::Value) -> BTreeMap<String, Question> {
        serde_json::from_value(raw).expect("valid question set")
    }

    fn rules(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.rule).collect()
    }

    #[test]
    fn clean_choice_passes() {
        let qs = questions(json!({
            "scheme": {
                "type": "choice",
                "instructions": "Which argument scheme does this passage use?",
                "criteria": {
                    "popular_opinion": "The conclusion is supported by claiming many people accept it",
                    "expert_opinion": "The conclusion rests on a named authority in a relevant field",
                    "none": "The passage makes no argument at all"
                }
            }
        }));
        assert!(lint_questions(&qs).is_empty());
    }

    #[test]
    fn flags_bare_labels() {
        let qs = questions(json!({
            "scheme": {
                "type": "choice",
                "instructions": "Which argument scheme applies here?",
                "criteria": {"analogy": "analogy", "none": "none"}
            }
        }));
        assert!(rules(&lint_questions(&qs)).contains(&"degenerate-criteria"));
    }

    /// The API accepts and bills this; only a linter catches it.
    #[test]
    fn flags_single_option_choice() {
        let qs = questions(json!({
            "q": {
                "type": "choice",
                "instructions": "Pick the applicable category",
                "criteria": {"only": "the only option available"}
            }
        }));
        assert!(rules(&lint_questions(&qs)).contains(&"single-option"));
    }

    /// Likewise observed live: returns score 0 at confidence 1.
    #[test]
    fn flags_single_level_score() {
        let qs = questions(json!({
            "q": {
                "type": "score",
                "instructions": "Rate the severity of this incident",
                "criteria": ["only one level"]
            }
        }));
        assert!(rules(&lint_questions(&qs)).contains(&"single-level"));
    }

    #[test]
    fn flags_conditional_phrasing() {
        let qs = questions(json!({
            "q": {
                "type": "noul",
                "instructions": "Does the passage cite a source, if applicable?"
            }
        }));
        assert!(rules(&lint_questions(&qs)).contains(&"conditional-question"));
    }

    #[test]
    fn flags_numeric_score_levels() {
        let qs = questions(json!({
            "q": {
                "type": "score",
                "instructions": "Rate the urgency of this ticket",
                "criteria": ["0", "1", "2"]
            }
        }));
        assert_eq!(
            rules(&lint_questions(&qs))
                .iter()
                .filter(|r| **r == "numeric-level")
                .count(),
            3
        );
    }

    /// Quoted numerals are accepted by the API, verified live, so they warn
    /// rather than block.
    #[test]
    fn quoted_numeric_levels_only_warn() {
        let qs = questions(json!({
            "q": {
                "type": "score",
                "instructions": "Rate the urgency of this ticket",
                "criteria": ["0", "1", "2"]
            }
        }));
        assert!(!lint_questions(&qs)
            .iter()
            .any(|f| f.severity == Severity::Error));
    }

    /// YAML reads a bare `1.5` as a number, and the API rejects it:
    /// 400 "questions.q.criteria.1: Invalid input". Must be an error, not a
    /// warning.
    #[test]
    fn unquoted_numeric_level_is_an_error() {
        let qs = questions(json!({
            "q": {
                "type": "score",
                "instructions": "Rate the confidence level described",
                "criteria": ["no", 1.5, "yes"]
            }
        }));
        let found = lint_questions(&qs);
        assert!(rules(&found).contains(&"non-string-level"));
        assert!(found
            .iter()
            .any(|f| f.rule == "non-string-level" && f.severity == Severity::Error));
    }

    /// Same hole on the choice side: 400 "questions.q.criteria.low: Invalid
    /// input".
    #[test]
    fn unquoted_numeric_choice_description_is_an_error() {
        let qs = questions(json!({
            "q": {
                "type": "choice",
                "instructions": "Which severity applies to this passage?",
                "criteria": {"low": 1.5, "high": "Permanent data loss", "none": "Nothing applies"}
            }
        }));
        let found = lint_questions(&qs);
        assert!(found
            .iter()
            .any(|f| f.rule == "non-string-criteria" && f.severity == Severity::Error));
    }

    /// The crate is `serde_norway` for exactly this reason: a bare `NO` key
    /// must stay the string "NO" and not become `false`.
    #[test]
    fn norway_problem_does_not_corrupt_labels() {
        let qs = crate::input::parse_questions(
            "q:\n  choice: Which country is named?\n  options:\n    NO: Norway is named\n    none: No country is named\n",
        )
        .unwrap();
        match &qs["q"] {
            Question::Choice { criteria, .. } => {
                assert!(
                    criteria.contains_key("NO"),
                    "got keys: {:?}",
                    criteria.keys().collect::<Vec<_>>()
                );
            }
            other => panic!("expected choice, got {other:?}"),
        }
    }

    #[test]
    fn flags_missing_escape_option() {
        let qs = questions(json!({
            "q": {
                "type": "choice",
                "instructions": "Which department should handle this ticket?",
                "criteria": {
                    "billing": "Questions about invoices or charges",
                    "technical": "Questions about product functionality"
                }
            }
        }));
        assert!(rules(&lint_questions(&qs)).contains(&"no-escape-option"));
    }

    #[test]
    fn sees_through_structured_instructions() {
        let qs = questions(json!({
            "q": {
                "type": "noul",
                "instructions": {"question": "Is it urgent, if applicable?", "focus": "the body"}
            }
        }));
        assert!(rules(&lint_questions(&qs)).contains(&"conditional-question"));
    }

    fn request_with_state_len(len: usize) -> Request {
        Request {
            model: "typesafe/jev-1.13".into(),
            state: json!("x".repeat(len)),
            questions: questions(json!({
                "q": {"type": "noul", "instructions": "Is this a long document?"}
            })),
            session_id: None,
        }
    }

    /// Only a payload that cannot fit under any tokenization is an error.
    #[test]
    fn flags_context_overflow() {
        let req = request_with_state_len(MAX_CONTEXT_TOKENS * 4 * 4 + 1000);
        assert!(rules(&lint_request(&req)).contains(&"context-overflow"));
    }

    /// Regression: a payload estimated at 38,751 tokens was reported by the API
    /// as 31,272 actual tokens and the call succeeded. Erroring there blocks a
    /// request the user is entitled to make, so this range must only warn.
    #[test]
    fn near_limit_warns_but_does_not_error() {
        let req = request_with_state_len(155_000);
        let found = lint_request(&req);
        assert!(rules(&found).contains(&"context-pressure"));
        assert!(
            !found.iter().any(|f| f.severity == Severity::Error),
            "an over-high estimate must not block a call that would succeed"
        );
    }

    #[test]
    fn ordinary_payloads_raise_no_context_findings() {
        let req = request_with_state_len(1_000);
        let findings = lint_request(&req);
        let found = rules(&findings);
        assert!(!found.contains(&"context-pressure"));
        assert!(!found.contains(&"context-overflow"));
    }
}
