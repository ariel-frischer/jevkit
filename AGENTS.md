# AGENTS.md

## Project Context

jevkit: Fast Rust CLI for TypeSafe Jev: typed decisions, offline linting

- **Language:** Go
- **Module:** `github.com/demo/jevkit`
- **Layout:** CLI only
- **Env prefix:** `JEVKIT`

## Command Map

```bash
make help           # List Make targets
make install        # Download Go modules
make build          # Build ./bin/jevkit with version ldflags
make bin            # Alias for build
make run            # Run ./cmd/jevkit
make go-install     # Install jevkit to GOPATH/bin
make install-global # Alias for go-install
make test           # Run tests
make test-v         # Run tests verbosely
make test-coverage  # Run tests with race detector and coverage
make lint           # Run linters (golangci-lint, fallback go vet)
make format         # Run go fmt ./...
make clean          # Remove build artifacts
```
CLI smoke checks:

```bash
go run ./cmd/jevkit --help
go run ./cmd/jevkit version
go run ./cmd/jevkit config keys
```

## File Layout

```
cmd/jevkit/      # CLI entry point (cobra)
  main.go             # binary entry point
  root.go             # root command, persistent flags, command wiring
  version.go          # version subcommand
  config.go           # user config subcommands
  ui.go               # terminal output helpers
  help.go             # custom help formatting
internal/
  version/            # version info injected via ldflags
  config/             # YAML config load/save/path helpers
assets/               # demo content (GIFs, screenshots)
.gitlab-ci.yml        # GitLab CI
CHANGELOG.yaml        # changelog source
CHANGELOG.md          # generated changelog output
.chlog.yaml           # changelog config
```

## Config Behavior

- User config lives at `~/.config/jevkit/config.yaml` by default.
- Path priority: root `--config`, then `$JEVKIT_CONFIG`, then the default path.
- Config commands: `init`, `show`, `path`, `edit`, `get`, `set`, `toggle`, `keys`.
- Missing config files load as empty config; CLI flags should still win over config defaults.


## Testing Guidance

- Prefer table tests with `map[string]struct{}` for command/config behavior.
- Run `chlog check` after changelog edits.
- Run `make test` for normal validation; use `make test-coverage` when touching shared packages.
- Smoke-test generated command paths with `go run ./cmd/jevkit ...` before release work.

## Release And Changelog

- Release automation was not generated. Keep version ldflags in the Makefile if you add a release flow later.
- Changelog source is `CHANGELOG.yaml`; regenerate `CHANGELOG.md` with `chlog sync`.
- Use `chlog add <category> "message"` for unreleased entries when possible.


## Multi-Agent Git Rules

- Check `git status --short` before editing and before committing.
- Keep writes scoped to files relevant to the task; do not clean up unrelated work.
- Do not use `git stash`, `git reset --hard`, or broad checkout/revert commands in a shared worktree.
- Stage explicit files only, not `git add .` or `git add -A`.
- Run focused tests for the files you touched, then the relevant Make targets before handoff.

## Coding Standards

- Functions under 40 lines
- Errors wrapped with context: `fmt.Errorf("doing X: %w", err)`
- Map-based table tests: `map[string]struct{}`
- Accept interfaces, return concrete types
