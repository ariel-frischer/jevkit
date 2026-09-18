# Changelog

All notable changes to jevkit will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `jev init`: interactive first-run setup via dialoguer — provider select (openrouter, typesafe, or a custom endpoint), key prompted on a hidden TTY, flags (`--provider`, `--no-key`) for agents and scripts
- `endpoint` config key to override the decisions URL (proxy or self-hosted); consumed by `jev ask` ahead of the provider preset and recorded in the usage ledger
- `jev ask`: send a YAML or JSON question set to Jev and print one value per question
- `jev lint`: offline schema and semantic validation, with no network call
- `jev auth`: OS keyring credentials, prompted on a hidden TTY and never taken from argv
- Terse YAML input that drops the API envelope without shortening criteria text
- Translation of the server's nested Zod validation errors into readable paths
- GitHub Actions workflow files for fmt, clippy, offline tests, build, an MSRV 1.82 job, and a weekly cargo-deny audit
- Lint ergonomics: `jev lint --json`, documented exit codes (0 clean, 1 errors, 2 warnings-only), `--strict`, and shell completions
- Documentation for the CI workflows and the lint ergonomics contract
- ANSI logo banner on bare jev --version; script-parsed version output stays plain

### Fixed

- Cargo version now matches the v0.1.0 release tag

