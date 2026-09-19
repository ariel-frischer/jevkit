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

/// `bad-questions.yaml` trips only Warning-severity findings, which exit 2
/// (warnings-only) under the documented contract; `--strict` makes them 1.
#[test]
fn lint_bad_file_warnings_exit_zero() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "examples/bad-questions.yaml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .code(2);
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
        .code(2)
        // Compact when piped (the assert harness captures stdout), so no
        // space after the colon; the stable contract is the `"rule"` key.
        .stdout(predicates::str::is_match("\"rule\": ?\"").unwrap());
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
    // A one-option choice parses fine but trips the single-option rule. The
    // option needs a criteria mapping (option -> description) to be legal.
    // single-option is a warning, so the exit is warnings-only (2).
    let inline = r#"{"pick":{"type":"choice","instructions":"State whether this is risky","criteria":{"only":"The only choice"}}}"#;
    let output = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "--question-set", inline])
        .output()
        .expect("lint runs");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
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

/// Compact default when stdout is a pipe: JSON is one line, no indentation.
/// The test harness captures stdout, so this exercises exactly the piped
/// path a script gets.
#[test]
fn ask_dry_run_compacts_when_piped() {
    let output = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args([
            "ask",
            "--question-set",
            CLEAN_INLINE_JSON,
            "--dry-run",
            "state",
        ])
        .output()
        .expect("dry-run runs");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(stdout.lines().count(), 1, "one line when piped: {stdout}");
    serde_json::from_str::<serde_json::Value>(&stdout).expect("piped output is valid JSON");
}

/// --pretty overrides the pipe default, same byte-for-byte form as a
/// terminal run.
#[test]
fn ask_dry_run_pretty_forces_indentation() {
    let output = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args([
            "ask",
            "--question-set",
            CLEAN_INLINE_JSON,
            "--dry-run",
            "state",
            "--pretty",
        ])
        .output()
        .expect("dry-run runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\n  "),
        "pretty output must be indented, got:\n{stdout}"
    );
}

/// Ask without lint reports lint errors with exit code 2, so a pipeline can
/// distinguish "the question set is broken" from exit 1 "usage/API failure".
/// Compare with the generic path, which is exit 1.
#[test]
fn ask_lint_errors_exit_2() {
    // Empty instructions is an error-severity rule (`empty-instructions`),
    // which is what trips exit 2. (A score with empty instructions would
    // fail earlier, at parse time, which is exit 1.)
    let inline = r#"{"s":{"type":"noul","instructions":""}}"#;
    let output = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["ask", "--question-set", inline, "state"])
        .output()
        .expect("ask runs");
    assert_eq!(output.status.code(), Some(2), "lint errors exit 2");
    // Nothing on stdout: a downstream jq must not see partial JSON.
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error["), "reason on stderr: {stderr}");
}

/// Generic failures -- a bad file path, a missing questions source -- keep
/// exit 1 so `? 2` can only mean lint.
#[test]
fn ask_generic_failure_exit_1() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["ask", "--questions", "does/not/exist.yaml", "state"])
        .assert()
        .code(1);
}

/// --compact on lint --json gives one-line JSON, same shape, no indentation.
#[test]
fn lint_json_compact_is_one_line() {
    let output = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "--json", "--compact", "examples/bad-questions.yaml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("lint runs");
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(stdout.lines().count(), 1, "one line: {stdout}");
    serde_json::from_str::<serde_json::Value>(&stdout).expect("compact --json still parses");
}

/// --pretty on lint --json keeps the indented form even though the harness
/// captures stdout (which would otherwise imply compact).
#[test]
fn lint_json_pretty_forces_indentation() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "--json", "--pretty", "examples/bad-questions.yaml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .code(2)
        .stdout(predicates::str::is_match("\n    \"rule\"").unwrap());
}

/// --compact and --pretty are mutually exclusive on both commands.
#[test]
fn compact_and_pretty_conflict() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args([
            "lint",
            "--json",
            "--compact",
            "--pretty",
            "examples/bad-questions.yaml",
        ])
        .assert()
        .failure();
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

/// `--quiet` drops the multi-line help blocks but keeps rule ids, the part
/// agents and scripts match on. This is the token-cost lever from
/// jevkit-26o: full output on the bad fixture carries `  help:` blocks,
/// quiet output carries none.
#[test]
fn lint_quiet_drops_help_but_keeps_rule_ids() {
    // Point XDG_CONFIG_HOME at an empty dir: the config file could otherwise
    // carry `lint_verbosity = quiet` and invert this test's meaning.
    let cfg_dir = std::env::temp_dir().join(format!("jev-quiet-test-{}", std::process::id()));
    std::fs::create_dir_all(&cfg_dir).expect("create temp config dir");
    let full = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "examples/bad-questions.yaml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("XDG_CONFIG_HOME", &cfg_dir)
        .output()
        .expect("lint runs");
    let full_out = String::from_utf8_lossy(&full.stdout);
    assert!(
        full_out.contains("  help:"),
        "expected help blocks in full lint output, got:\n{full_out}"
    );

    let quiet = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "--quiet", "examples/bad-questions.yaml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("XDG_CONFIG_HOME", &cfg_dir)
        .assert()
        .code(2)
        .get_output()
        .clone();
    let quiet_out = String::from_utf8_lossy(&quiet.stdout);
    assert!(
        !quiet_out.contains("  help:"),
        "quiet output must not contain help blocks, got:\n{quiet_out}"
    );
    assert!(
        quiet_out.contains("degenerate-criteria"),
        "quiet output must keep rule ids, got:\n{quiet_out}"
    );
    // Terse is strictly shorter than full for the same findings.
    assert!(
        quiet_out.len() < full_out.len() / 2,
        "quiet output should be much shorter: quiet {} vs full {} bytes",
        quiet_out.len(),
        full_out.len()
    );
}

/// `--quiet` on `jev ask` keeps the lint failure path terse but still names
/// the rule. An empty state triggers the `empty-state` Error finding; the
/// deeper error findings that carry help blocks (`non-string-criteria`) are
/// unreachable through parsed CLI input, because input.rs coerces numerics
/// to strings, so the observable effect here is the rule id surviving.
#[test]
fn ask_quiet_dry_run_trims_lint_output() {
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args([
            "ask",
            "--dry-run",
            "--quiet",
            "--question-set",
            CLEAN_INLINE_JSON,
            "",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("error[empty-state]"))
        .stderr(predicates::str::contains("fix them or pass --no-lint"));
}

/// The config path: `lint_verbosity = "quiet"` behaves like `--quiet`, and
/// `full` restores help blocks. Set and unset through the CLI itself so the
/// test leaves nothing behind.
#[test]
fn lint_verbosity_config_quiet_drops_help() {
    let cd = env!("CARGO_MANIFEST_DIR");
    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["config", "set", "lint_verbosity", "quiet"])
        .current_dir(cd)
        .assert()
        .success();
    let quiet = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "examples/bad-questions.yaml"])
        .current_dir(cd)
        .assert()
        .code(2)
        .get_output()
        .clone();
    assert!(
        !String::from_utf8_lossy(&quiet.stdout).contains("  help:"),
        "config quiet must drop help blocks"
    );

    Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["config", "set", "lint_verbosity", "full"])
        .current_dir(cd)
        .assert()
        .success();
    let full = Command::cargo_bin("jev")
        .expect("jev binary builds")
        .args(["lint", "examples/bad-questions.yaml"])
        .current_dir(cd)
        .assert()
        .code(2)
        .get_output()
        .clone();
    assert!(
        String::from_utf8_lossy(&full.stdout).contains("  help:"),
        "config full must restore help blocks"
    );
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
