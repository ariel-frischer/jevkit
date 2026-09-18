---
name: jevkit
description: >
  Fast Rust CLI for TypeSafe Jev: typed decisions, offline linting
license: MIT
compatibility:
  - Claude Code
  - Cursor
  - Codex
  - Gemini CLI
  - VS Code
metadata:
  author: Ariel Frischer
  version: 0.0.1
  tags: go, cli
allowed-tools: Bash Read Write Edit
---

# jevkit

Fast Rust CLI for TypeSafe Jev: typed decisions, offline linting

## Commands

```bash
jevkit --help              # Show available commands
jevkit version             # Show version info
jevkit v                   # Alias for version
jevkit completion bash     # Shell completion: bash|zsh|fish|powershell
jevkit --config ./config.yaml config path
jevkit config init         # Create user config
jevkit config get <key>    # Read a config value
jevkit config set <key> <value>
jevkit config toggle <key> # Toggle a boolean value
jevkit config keys         # List configurable keys
```

## Project Structure

Includes: Makefile, CI pipelines, assets/ directory, .gitignore, LICENSE, SECURITY.md, CHANGELOG.yaml + CHANGELOG.md.
Agent guidance lives in `AGENTS.md`.

## Development

```bash
make build          # Build binary
make bin            # Alias for build
make install-global # Alias for go-install
make test           # Run tests
make lint           # Run linters
make format         # Format code
```
Use `chlog add ...`, `chlog sync`, and `chlog check` for changelog updates.
