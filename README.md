<div align="center">

**jevkit**

Fast Rust CLI for TypeSafe Jev: typed decisions, offline linting
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

</div>

## Install

**Go install**:

```bash
go install github.com/demo/jevkit/cmd/jevkit@latest
```

**From source**:

```bash
git clone https://gitlab.com/demo/jevkit.git
cd jevkit
make build    # Binary at bin/jevkit
make bin      # Alias for build
```

## Usage

```bash
jevkit --help
```

### AI Agent Skill

This project ships a [SKILL.md](.skills/default/SKILL.md) following the [Agent Skills open standard](https://agentskills.io). Install it so your coding agent knows all commands and options.

**Quick install with [`skills`](https://skills.sh) CLI** (by Vercel Labs):

```bash
npx skills add demo/jevkit
```

<details>
<summary><strong>Manual install</strong></summary>

**Claude Code** — Skills live in `~/.claude/skills/` (global) or `.claude/skills/` (project-local).

```bash
# Global — available in all projects
mkdir -p ~/.claude/skills/jevkit
curl -fsSL https://raw.githubusercontent.com/demo/jevkit/main/.skills/default/SKILL.md \
  -o ~/.claude/skills/jevkit/SKILL.md

# Project-local — checked into this repo only
mkdir -p .claude/skills/jevkit
curl -fsSL https://raw.githubusercontent.com/demo/jevkit/main/.skills/default/SKILL.md \
  -o .claude/skills/jevkit/SKILL.md
```

**Codex CLI** — reads skills from `~/.codex/skills/` (global) or `.codex/skills/` (project-local).

```bash
# Global
mkdir -p ~/.codex/skills/jevkit
curl -fsSL https://raw.githubusercontent.com/demo/jevkit/main/.skills/default/SKILL.md \
  -o ~/.codex/skills/jevkit/SKILL.md

# Project-local
mkdir -p .codex/skills/jevkit
curl -fsSL https://raw.githubusercontent.com/demo/jevkit/main/.skills/default/SKILL.md \
  -o .codex/skills/jevkit/SKILL.md
```

Or pass directly: `codex --instructions .skills/default/SKILL.md`

</details>

## Development

```bash
make build          # Build binary
make bin            # Alias for build
make install-global # Alias for go-install
make test           # Run tests
make lint           # Run linters
make format         # Format code
```

## Shell Completion

```bash
# Bash
source <(jevkit completion bash)

# Zsh
source <(jevkit completion zsh)

# Fish
jevkit completion fish > ~/.config/fish/completions/jevkit.fish
```

## License
[MIT](LICENSE)
