# Changelog

All notable changes to jevkit will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

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

[Unreleased]: https://github.com/ariel-frischer/jevkit/compare/v0.2.0...HEAD
