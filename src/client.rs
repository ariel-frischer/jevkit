//! HTTP transport for the decisions API.
//!
//! Deliberately contains no question-authoring logic: that lives in `lint`.
//! The one piece of real work here is translating the server's validation
//! errors, which arrive as a JSON-encoded string inside `error.message`, into
//! something a human can act on.

use crate::types::{Request, Response};
use anyhow::{bail, Context, Result};
use std::time::Duration;

pub struct Client {
    http: reqwest::blocking::Client,
    endpoint: String,
    api_key: String,
}

impl Client {
    pub fn new(endpoint: impl Into<String>, api_key: impl Into<String>) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(90))
            .user_agent(concat!("jevkit/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("failed to build the HTTP client")?;
        Ok(Client {
            http,
            endpoint: endpoint.into(),
            api_key: api_key.into(),
        })
    }

    pub fn decide(&self, req: &Request) -> Result<Response> {
        let resp = self
            .http
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(req)
            .send()
            .with_context(|| format!("request to {} failed", self.endpoint))?;

        let status = resp.status();
        let body = resp.text().context("failed to read the response body")?;

        if !status.is_success() {
            bail!("{}", describe_error(status.as_u16(), &body));
        }

        serde_json::from_str(&body)
            .with_context(|| format!("failed to decode the response: {}", truncate(&body, 400)))
    }
}

/// Turn an API error body into a readable message.
///
/// The server validates with Zod and returns the issue list as a JSON string
/// nested inside `error.message`, so the useful part is two layers deep. A raw
/// dump is unreadable; the path and reason are what matter.
fn describe_error(status: u16, body: &str) -> String {
    let Ok(outer) = serde_json::from_str::<serde_json::Value>(body) else {
        return format!("API returned status {status}: {}", truncate(body, 400));
    };

    let Some(message) = outer.pointer("/error/message").and_then(|m| m.as_str()) else {
        return format!("API returned status {status}: {}", truncate(body, 400));
    };

    let Ok(issues) = serde_json::from_str::<Vec<serde_json::Value>>(message) else {
        return format!("API returned status {status}: {message}");
    };

    let mut out = format!("API rejected the request (status {status}):");
    for issue in &issues {
        let path = issue
            .get("path")
            .and_then(|p| p.as_array())
            .map(|parts| {
                parts
                    .iter()
                    .map(|p| {
                        p.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| p.to_string())
                    })
                    .collect::<Vec<_>>()
                    .join(".")
            })
            .unwrap_or_default();

        let detail = issue
            .get("message")
            .and_then(|m| m.as_str())
            .map(str::to_string)
            .or_else(|| {
                // Discriminator failures carry no `message`, only the list of
                // types that would have been accepted.
                let options = issue.get("options")?.as_array()?;
                let names: Vec<_> = options.iter().filter_map(|o| o.as_str()).collect();
                Some(format!("expected one of: {}", names.join(", ")))
            })
            .unwrap_or_else(|| "invalid".to_string());

        if path.is_empty() {
            out.push_str(&format!("\n  {detail}"));
        } else {
            out.push_str(&format!("\n  {path}: {detail}"));
        }
    }
    out
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}...", &s[..n])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim from a live 400 during development.
    #[test]
    fn explains_missing_criteria() {
        let body = r#"{"error":{"message":"[\n  {\n    \"expected\": \"record\",\n    \"code\": \"invalid_type\",\n    \"path\": [\n      \"questions\",\n      \"q\",\n      \"criteria\"\n    ],\n    \"message\": \"Invalid input: expected record, received undefined\"\n  }\n]","code":400}}"#;
        let msg = describe_error(400, body);
        assert!(msg.contains("questions.q.criteria"));
        assert!(msg.contains("expected record"));
    }

    /// Discriminator failures have no `message` field, only `options`.
    #[test]
    fn explains_unknown_question_type() {
        let body = r#"{"error":{"message":"[{\"code\":\"invalid_union\",\"note\":\"No matching discriminator\",\"discriminator\":\"type\",\"options\":[\"noul\",\"choice\",\"score\"],\"path\":[\"questions\",\"q\"]}]","code":400}}"#;
        let msg = describe_error(400, body);
        assert!(msg.contains("questions.q"));
        assert!(msg.contains("noul, choice, score"));
    }

    #[test]
    fn falls_back_on_unstructured_errors() {
        let msg = describe_error(500, "upstream exploded");
        assert!(msg.contains("500"));
        assert!(msg.contains("upstream exploded"));
    }

    #[test]
    fn handles_plain_string_error_messages() {
        let body = r#"{"error":{"message":"No auth credentials found","code":401}}"#;
        let msg = describe_error(401, body);
        assert!(msg.contains("No auth credentials found"));
    }
}
