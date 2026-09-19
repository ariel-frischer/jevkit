//! End-to-end tests: a real `jev ask` binary talking to a real local HTTP
//! server that stands in for the Jev decisions API.
//!
//! These are the only tests that exercise the full requested path: config
//! resolution, lint gating, HTTP, wire decoding, and stdout formatting, with
//! no stubbing of any layer except the network peer itself.
//!
//! The mock server is `tiny_http` on an OS-assigned port, one server per test
//! on its own thread, so tests are parallel-safe and offline.

use assert_cmd::Command;
use std::io::Read as _;
use tiny_http::{Header, Response, Server};

/// A request that hits the mock server, for asserting on the wire payload.
struct MockRequest {
    path: String,
    auth: String,
    body: String,
}

/// Spin up a mock Decisions API. Returns the base URL and a receiver for the
/// request the CLI actually sent. The server answers every POST with `body`.
fn spawn_mock(status: u16, body: &'static str) -> (String, std::sync::mpsc::Receiver<MockRequest>) {
    let server = Server::http("127.0.0.1:0").expect("bind mock server");
    let port = server.server_addr().to_ip().expect("addr").port();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || loop {
        let Ok(mut req) = server.recv() else { break };
        let mut body_buf = String::new();
        let _ = req.as_reader().read_to_string(&mut body_buf);
        let mock = MockRequest {
            path: req.url().to_string(),
            auth: req
                .headers()
                .iter()
                .find(|h| {
                    h.field
                        .as_str()
                        .as_str()
                        .eq_ignore_ascii_case("Authorization")
                })
                .map(|h| h.value.as_str().to_string())
                .unwrap_or_default(),
            body: body_buf,
        };
        let resp = Response::from_string(body)
            .with_status_code(status)
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
        let _ = req.respond(resp);
        let _ = tx.send(mock);
    });
    (format!("http://127.0.0.1:{port}/api/alpha/decisions"), rx)
}

const CLEAN_SET: &str = r#"{"risk":{"type":"noul","instructions":"Is this text dangerous?"}}"#;
const CLEAN_KEY: &str = "test-key-e2e";

/// The happy path: clean question set, mock returns one noul answer.
/// Stdout must be exactly one JSON object mapping the question name to the
/// probability -- the summary contract scripts rely on.
#[test]
fn e2e_ask_happy_path_prints_summary_json() {
    let (endpoint, rx) = spawn_mock(
        200,
        r#"{"model":"typesafe/jev-1.13","answers":{"risk":{"type":"noul","noul":0.23}},"usage":{"input_tokens":10,"output_tokens":2,"cost":0.001}}"#,
    );
    let output = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .env("JEV_ENDPOINT", &endpoint)
        .env("JEV_API_KEY", CLEAN_KEY)
        .args(["ask", "--question-set", CLEAN_SET, "some state"])
        .output()
        .expect("ask runs");

    assert!(
        output.status.success(),
        "exit was {:?}",
        output.status.code()
    );
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let mut parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout is valid JSON");
    // One line when piped (the harness captures stdout).
    assert_eq!(stdout.lines().count(), 1, "compact when piped: {stdout}");
    assert_eq!(
        parsed
            .as_object_mut()
            .expect("summary is an object")
            .remove("risk")
            .expect("question name keys the summary"),
        serde_json::json!(0.23),
        "noul probability, not a truthy boolean"
    );

    let sent = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("mock received the request");
    assert_eq!(sent.path, "/api/alpha/decisions", "decisions endpoint");
    assert_eq!(sent.auth, format!("Bearer {CLEAN_KEY}"));
    let body: serde_json::Value = serde_json::from_str(&sent.body).expect("wire JSON");
    assert_eq!(body["state"], "some state");
    assert_eq!(body["questions"]["risk"]["type"], "noul", "wire type tag");
}

/// --raw echoes the full response, usage and cost included.
#[test]
fn e2e_ask_raw_prints_full_response() {
    let (endpoint, rx) = spawn_mock(
        200,
        r#"{"model":"m","answers":{"r":{"type":"noul","noul":0.5}},"usage":{"input_tokens":7,"output_tokens":1,"cost":0.001},"id":"resp-1"}"#,
    );
    let output = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .env("JEV_ENDPOINT", &endpoint)
        .env("JEV_API_KEY", CLEAN_KEY)
        .args(["ask", "--raw", "--question-set", CLEAN_SET, "state"])
        .output()
        .expect("ask runs");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // --pretty is implied by the wire shape? No: --raw pipes compact. The
    // contract is valid JSON containing usage.
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).expect("raw is JSON");
    assert_eq!(parsed["usage"]["input_tokens"], 7);
    assert_eq!(parsed["id"], "resp-1");
    let _ = rx;
}

/// An API-level rejection (HTTP 400 with a Zod-style nested error) surfaces
/// as exit 1 with the extracted message on stderr, and stdout stays empty.
#[test]
fn e2e_ask_api_error_exits_1_stdout_clean() {
    let (endpoint, _rx) = spawn_mock(
        422,
        r#"{"error":{"message":"[{\"path\":[\"questions\"],\"code\":\"invalid_type\"}]"}}"#,
    );
    let output = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .env("JEV_ENDPOINT", &endpoint)
        .env("JEV_API_KEY", CLEAN_KEY)
        .args(["ask", "--question-set", CLEAN_SET, "state"])
        .output()
        .expect("ask runs");
    assert_eq!(output.status.code(), Some(1), "API error is exit 1");
    assert!(output.stdout.is_empty(), "no partial JSON on stdout");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error:"), "reason on stderr: {stderr}");
}

/// A lint-error question set exits 2 before any HTTP call goes out: the mock
/// must never see a request, proving the wasted-billing guard is upstream of
/// the network.
#[test]
fn e2e_ask_lint_error_never_hits_network() {
    let (endpoint, rx) = spawn_mock(200, "{}");
    let bad = r#"{"s":{"type":"noul","instructions":""}}"#;
    let output = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .env("JEV_ENDPOINT", &endpoint)
        .env("JEV_API_KEY", CLEAN_KEY)
        .args(["ask", "--question-set", bad, "state"])
        .output()
        .expect("ask runs");
    assert_eq!(output.status.code(), Some(2), "lint errors exit 2");
    assert!(output.stdout.is_empty());
    assert!(
        rx.recv_timeout(std::time::Duration::from_millis(200))
            .is_err(),
        "no request may reach the API on lint errors"
    );
}

/// --no-lint lets a lint-failing set through: the request leaves, proving
/// the escape hatch works end to end.
#[test]
fn e2e_ask_no_lint_sends_anyway() {
    let (endpoint, rx) = spawn_mock(
        200,
        r#"{"model":"m","answers":{"s":{"type":"noul","noul":0.1}}}"#,
    );
    let bad = r#"{"s":{"type":"noul","instructions":""}}"#;
    let output = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .env("JEV_ENDPOINT", &endpoint)
        .env("JEV_API_KEY", CLEAN_KEY)
        .args(["ask", "--no-lint", "--question-set", bad, "state"])
        .output()
        .expect("ask runs");
    assert!(
        output.status.success(),
        "no-lint proceeds: {:?}",
        output.status.code()
    );
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .expect("request sent under --no-lint");
}
