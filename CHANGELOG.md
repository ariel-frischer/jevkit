# Changelog

All notable changes to jevkit will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Fixed

- A configured log path no longer enables the usage ledger implicitly; logging needs --log or JEV_LOG_FILE, and JEV_LOG_FILE=1 selects the default XDG path
- Usage ledger fingerprints --api-key=KEY values like the space-separated form

## [0.4.1] - 2026-09-26

### Changed

- Published to crates.io as `jevkit-cli` (`cargo install jevkit-cli`); the `jevkit` crate name belongs to an unrelated project. The binary is still `jev`

## [0.4.0] - 2026-09-26

### Added

- `jev lint` accepts several files (`jev lint *.yaml`); findings are prefixed with the file name, `--json` findings gain a `file` field, and an unreadable file is reported without stopping the rest
- `jev ask --noul TEXT`: inline yes/no questions, repeatable, named `q1`, `q2`, ...

### Fixed

- `jev ask --questions` and `--question-set` together now error instead of silently ignoring the inline set

## [0.2.0] - 2026-09-18

### Added

- `jev ask`: send a YAML or JSON question set to Jev and print one value per question
- `jev lint`: offline schema and semantic validation, with no network call
- `jev auth`: OS keyring credentials, prompted on a hidden TTY and never taken from argv
- Terse YAML input that drops the API envelope without shortening criteria text
- Translation of the server's nested Zod validation errors into readable paths
- GitHub Actions workflow files for fmt, clippy, offline tests, build, an MSRV 1.88 job, and a weekly cargo-deny audit
- Lint ergonomics: `jev lint --json`, documented exit codes (0 clean, 1 errors, 2 warnings-only), `--strict`, and shell completions
- Documentation for the CI workflows and the lint ergonomics contract
- ANSI logo banner on bare jev --version; script-parsed version output stays plain
- jev init: guided first-run setup that checks the binary, walks through provider and key, and prints copy-pasteable examples
- MSRV raised to 1.88 to match the current keyring, zbus, and clap ecosystem

### Fixed

- Cargo version now matches the v0.1.0 release tag

[Unreleased]: https://github.com/ariel-frischer/jevkit/compare/v0.4.1...HEAD
[0.4.1]: https://github.com/ariel-frischer/jevkit/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/ariel-frischer/jevkit/compare/v0.2.0...v0.4.0
