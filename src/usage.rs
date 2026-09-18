//! Opt-in usage ledger for paid API calls.
//!
//! One JSON line per `ask` call: the tokens and cost the API reports, the
//! full request and response bodies, and the exact command invocation. This
//! is a durable, append-only history for debugging billing and reproducing a
//! past call.
//!
//! Opt-in by design. A CLI should not write files the user never asked for,
//! and the ledger contains the user's own content (`state`, `questions`), so
//! it must not appear by default.
//!
//! Default location follows XDG: `$XDG_STATE_HOME/jev/usage.jsonl`, which is
//! `~/.local/state/jev/usage.jsonl` for a normal user. State, not cache or
//! config: cache is disposable, this is long-term history.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// The per-call metadata of a log entry, kept as a struct so the logger
/// stays under clippy's argument limit.
pub struct CallMeta<'a> {
    pub provider: &'a str,
    pub endpoint: &'a str,
    pub model: &'a str,
    pub session_id: Option<&'a str>,
    pub elapsed_ms: u128,
}

/// One record in the ledger: what was asked, what came back, what it cost.
#[derive(serde::Serialize)]
struct LedgerRecord<'a, R> {
    ts: String,
    command: String,
    invocation: String,
    provider: String,
    endpoint: String,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    elapsed_ms: u128,
    request: &'a serde_json::Value,
    response: Option<&'a R>,
    status: &'static str,
}

/// Resolve the target log path.
///
/// Priority: explicit `--log` path > `JEV_LOG_FILE` env > XDG state dir.
pub fn resolve_log_path(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    if let Some(p) = std::env::var_os("JEV_LOG_FILE") {
        return Ok(PathBuf::from(p));
    }
    state_dir().map(|p| p.join("usage.jsonl"))
}

/// `$XDG_STATE_HOME/jev`, falling back to `~/.local/state/jev`.
fn state_dir() -> Result<PathBuf> {
    let home = match std::env::var_os("XDG_STATE_HOME") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => {
            let home = std::env::var_os("HOME").context(
                "cannot determine the state directory: neither XDG_STATE_HOME nor HOME is set",
            )?;
            PathBuf::from(home).join(".local/state")
        }
    };
    Ok(home.join("jev"))
}

/// Append one call record, creating the file and parent dirs if missing.
///
/// Ledger failures are reported but surfaced only after the call result has
/// already been handled, so a broken ledger never loses the paid result.
pub fn log_call<R: serde::Serialize>(
    path: &PathBuf,
    meta: &CallMeta<'_>,
    request: &serde_json::Value,
    response: Option<&R>,
    status: &'static str,
) -> Result<()> {
    // The raw argv as the user typed it, not a normalized struct: the exact
    // invocation is usually the fastest way to reproduce a call.
    let invocation = std::env::args().collect::<Vec<String>>().join(" ");
    let record = LedgerRecord {
        ts: iso8601_now(),
        command: "ask".to_string(),
        invocation,
        provider: meta.provider.to_string(),
        endpoint: meta.endpoint.to_string(),
        model: meta.model.to_string(),
        session_id: meta.session_id.map(|s| s.to_string()),
        elapsed_ms: meta.elapsed_ms,
        request,
        response,
        status,
    };
    let line = serde_json::to_string(&record).context("failed to serialize a ledger record")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    use std::io::Write;
    file.write_all(format!("{line}\n").as_bytes())
        .with_context(|| format!("failed to append to {}", path.display()))?;
    Ok(())
}

/// A UTC timestamp like `2026-09-18T12:20:00Z`. Done by hand so no date crate
/// is added for one line per call; Howard Hinnant's civil-from-days algorithm,
/// which needs only integer math.
fn iso8601_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_592 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        tod / 3_600,
        (tod % 3_600) / 60,
        tod % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_resolution_prefers_the_explicit_argument() {
        let given = PathBuf::from("/tmp/jev-usage-test.jsonl");
        let got = resolve_log_path(Some(given.clone())).unwrap();
        assert_eq!(got, given);
    }

    #[test]
    fn ledger_record_round_trips_minimal_case() {
        let path = std::env::temp_dir().join("jev-usage-round-trip.jsonl");
        let _ = std::fs::remove_file(&path);
        let request = serde_json::json!({"q": "test"});
        let response = serde_json::json!({"a": "test"});
        let meta = CallMeta {
            provider: "openrouter",
            endpoint: "https://example.test/api/alpha/decisions",
            model: "test-model",
            session_id: None,
            elapsed_ms: 12,
        };
        let () = log_call(&path, &meta, &request, Some(&response), "ok").unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents.lines().count(), 1);
        assert!(contents.contains("\"provider\":\"openrouter\""));
        assert!(!contents.contains("api_key"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn error_records_carry_a_status() {
        let path = std::env::temp_dir().join("jev-usage-error.jsonl");
        let _ = std::fs::remove_file(&path);
        let request = serde_json::json!({"q": "test"});
        let meta = CallMeta {
            provider: "openrouter",
            endpoint: "https://example.test/api/alpha/decisions",
            model: "test-model",
            session_id: None,
            elapsed_ms: 5,
        };
        let () = log_call(
            &path,
            &meta,
            &request,
            Option::<&serde_json::Value>::None,
            "error",
        )
        .unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("\"status\":\"error\""));
        assert!(contents.contains("\"response\":null"));
        let _ = std::fs::remove_file(&path);
    }
}
