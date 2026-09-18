# CLAUDE.md

## Project: jevkit

Fast Rust CLI for TypeSafe Jev: typed decisions, offline linting

## Commands

```bash
make build          # Build binary
make test           # Run tests
make lint           # Run linters
make format         # Format code
```

## Architecture

```
cmd/jevkit/      # CLI entry point (cobra)
internal/version/    # Version info (ldflags)
assets/              # Demo content (GIFs, screenshots)
```

## Coding Standards

- Functions under 40 lines
- Errors wrapped with context: `fmt.Errorf("doing X: %w", err)`
- Map-based table tests: `map[string]struct{}`
- Accept interfaces, return concrete types
