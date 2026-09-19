# Changelog

All notable changes to jevkit will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.3.0] - 2026-09-19

### Added

- `jev ask` and `jev lint` pipe-aware JSON output with `--compact` and `--pretty` flags
- Documented `ask` exit-code contract for scriptable callers
- End-to-end tests exercising real HTTP round trips against a mock Decisions server

### Changed

- CLI output contract for machine-readable ask/lint output
## [0.2.0] - 2026-09-18

### Added

- `jev ask`: send a YAML or JSON question set to Jev and print one value per question
- `jev lint`: offline schema and semantic validation, with no network call
- `jev auth`: OS keyring credentials, prompted on a hidden TTY and never taken from argv
- Terse YAML input that drops the API envelope without shortening criteria text
- Translation of the server's nested Zod validation errors into readable paths
- GitHub Actions workflow files for fmt, clippy, offline tests, build, an MSRV 1.88 job, and a weekly cargo-deny audit
- Lint ergonomics: `jev lint --json`, documented exit codes (0 clean, 1 errors, 2 warnings-only), `--strict`, and shell completions
- Tunable lint verbosity: `--quiet` on `ask` and `lint` prints rule ids without help blocks, plus a `lint_verbosity` config key (`full|quiet`) so agents can default to terse
- Documentation for the CI workflows and the lint ergonomics contract
- ANSI logo banner on bare jev --version; script-parsed version output stays plain
- jev init: guided first-run setup that checks the binary, walks through provider and key, and prints copy-pasteable examples
- MSRV raised to 1.88 to match the current keyring, zbus, and clap ecosystem

### Fixed

- Crate version aligned with the release tag

[Unreleased]: https://github.com/ariel-frischer/jevkit/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/ariel-frischer/jevkit/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/ariel-frischer/jevkit/compare/v0.1.0...v0.2.0
