//! End-to-end CLI integration tests for the `jev` binary.
//!
//! These run the compiled binary through `assert_cmd` against the checked-in
//! `examples/` fixtures. They are fully offline: `lint` makes no network call
//! and needs no API key, per the AGENTS.md hard rule.
//!
//! Exit-code contract asserted here is the **current** one: exit 0 when clean,
//! exit 1 when any Error-severity finding fires (warnings alone still exit 0
//! unless `--deny-warnings` is passed).
//!
//! NOTE: a parallel branch (agent/cli-ergonomics) is introducing 0=clean,
//! 1=errors, 2=warnings-only plus `--strict`. See the `POST_MERGE_ADJUSTMENTS`
//! comment at the bottom for exactly which assertions must change after that
//! merge lands.

use assert_cmd::Command;

/// `jev lint examples/severity.yaml` is the clean fixture: exit 0, and the
/// human-readable summary lands on stdout.
#[test]
fn lint_clean_file_exits_zero() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "examples/severity.yaml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "3 question(s), no problems found",
        ));
}

/// The JSON form of the clean fixture must emit valid, empty JSON on stdout.
#[test]
fn lint_clean_file_json_outputs_empty_array() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "--json", "examples/severity.yaml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .success()
        .stdout(predicates::str::contains("[]"));
}

/// `bad-questions.yaml` trips only Warning-severity findings, so under the
/// current contract it exits 0 (warnings are advisory; `--deny-warnings`
/// makes them fatal).
#[test]
fn lint_bad_file_warnings_exit_zero() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "examples/bad-questions.yaml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .success()
        .code(0);
}

/// `--deny-warnings` makes the same warnings fatal, exit 1.
#[test]
fn lint_bad_file_deny_warnings_exits_one() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "--deny-warnings", "examples/bad-questions.yaml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .failure()
        .code(1);
}

/// Each semantic rule that the bad fixture is documented to trip must appear
/// by its stable rule id in the output.
#[test]
fn lint_bad_file_reports_expected_rule_ids() {
    let output = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "examples/bad-questions.yaml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("lint runs");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for rule in [
        "single-option",
        "degenerate-criteria",
        "single-level",
        "numeric-level",
        "conditional-question",
    ] {
        assert!(
            combined.contains(rule),
            "expected rule id `{rule}` in lint output, got:\n{combined}"
        );
    }
}

/// `--json` on the bad fixture must emit one JSON object per finding, each
/// carrying the stable `rule` field.
#[test]
fn lint_bad_file_json_includes_rule_field() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "--json", "examples/bad-questions.yaml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .success()
        .stdout(predicates::str::is_match("\"rule\": \"").unwrap());
}

/// Stdout/stderr separation contract: all of `lint`'s output (summary,
/// findings, JSON) is primary output on stdout, so piping stdout to `jq`
/// stays safe. Diagnostics that are not part of the payload, such as Rust's
/// own parse errors, go to stderr.
#[test]
fn lint_findings_go_to_stdout_not_stderr() {
    let output = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "examples/bad-questions.yaml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("lint runs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("[single-option]"),
        "findings belong on stdout (pipeable to jq), got stdout:\n{stdout}"
    );
    assert!(
        !stderr.contains("[single-option]"),
        "stderr must stay free of finding lines, got stderr:\n{stderr}"
    );
}

/// On the clean path nothing is written to stderr at all.
#[test]
fn lint_clean_file_stderr_is_empty() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "examples/severity.yaml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .success()
        .stderr(predicates::str::is_empty());
}

/// A file that does not parse is a hard diagnostic on stderr, not a lint
/// finding on stdout, and it exits nonzero.
#[test]
fn lint_unparseable_file_fails_with_diagnostic_on_stderr() {
    let dir = std::env::temp_dir().join(format!("jev-lint-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("broken.yaml");
    std::fs::write(&path, "{ not: valid: yaml: ][").expect("write temp fixture");

    let assert = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint"])
        .arg(&path)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .failure();

    let output = assert.get_output();
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("[error"),
        "parse failures must not be reported as lint findings"
    );

    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
}

/// A missing file argument is an error, not an empty clean run.
#[test]
fn lint_missing_file_fails() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "examples/does-not-exist.yaml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// --question-set: question sets passed inline as CLI strings, no file needed.
// ---------------------------------------------------------------------------

const CLEAN_INLINE_JSON: &str =
    r#"{"risky":{"type":"noul","instructions":"Is this text dangerous?"}}"#;

const CLEAN_INLINE_YAML: &str = "risky:\n  noul: Is this text dangerous?\n";

/// An inline JSON question set lints clean and exits zero, exactly as a file
/// would, without any temporary file.
#[test]
fn ask_inline_question_set_json_lints_clean() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args([
            "ask",
            "--question-set",
            CLEAN_INLINE_JSON,
            "--dry-run",
            "state",
        ])
        .assert()
        .success();
}

/// An inline YAML question set works the same way as inline JSON.
#[test]
fn ask_inline_question_set_yaml_lints_clean() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args([
            "ask",
            "--question-set",
            CLEAN_INLINE_YAML,
            "--dry-run",
            "state",
        ])
        .assert()
        .success();
}

/// An inline question set reaches the API payload: the question name, type,
/// and instructions must appear in the dry-run request.
#[test]
fn ask_inline_question_set_appears_in_dry_run_payload() {
    let output = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args([
            "ask",
            "--question-set",
            CLEAN_INLINE_JSON,
            "--dry-run",
            "some state",
        ])
        .output()
        .expect("dry-run runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"risky\"") && stdout.contains("Is this text dangerous?"),
        "dry-run payload must contain the inline question, got:\n{stdout}"
    );
}

/// Inline input flows through linting: a known failure rule fires on an inline
/// set, same as it would from a file.
#[test]
fn lint_inline_question_set_reports_findings() {
    // One option is an error: the answer cannot inform anyone. Criteria are
    // required for the set to parse at all.
    let inline = r#"{"pick":{"type":"choice","options":["only"],"criteria":"Choose one","instructions":"Pick one"}}"#;
    let assert = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "--question-set", inline])
        .assert()
        .failure();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        stdout.contains("single-option"),
        "inline set must go through lint rules, got:\n{stdout}"
    );
}

/// An inline set lints clean under --json, producing the empty array a clean
/// file produces.
#[test]
fn lint_inline_question_set_json_outputs_empty_array() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "--json", "--question-set", CLEAN_INLINE_JSON])
        .assert()
        .success()
        .stdout(predicates::str::contains("[]"));
}

/// Passing --question-set and both questions unset behaves as before: ask
/// refuses without any question source.
#[test]
fn ask_without_question_source_fails() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["ask", "--dry-run", "state"])
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// POST_MERGE_ADJUSTMENTS for agent/cli-ergonomics (0=clean, 1=errors,
// 2=warnings-only, --strict):
//
// 1. `lint_bad_file_warnings_exit_zero` (current 0) becomes `.code(2)` for a
//    warnings-only payload, and `lint_bad_file_deny_warnings_exits_one`
//    (`--deny-warnings`) likely renames to whatever the merged `--strict`
//    flag is called; re-pin both after reading the merged src change.
// 2. `lint_clean_file_json_outputs_empty_array` and
//    `lint_clean_file_exits_zero`: unchanged (0 stays clean).
// 3. New behavior to cover after merge: a warnings-only fixture must exit 2
//    (or 0 under `--strict` absence semantics per the merged spec). No such
//    fixture exists in examples/ yet; add one then.
// ---------------------------------------------------------------------------
