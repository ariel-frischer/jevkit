# AGENTS.md

Guidance for agents working in this repository.

## What this is

`jevkit` is a CLI for TypeSafe's Jev, a model that returns typed decisions
rather than prose. The binary is `jev`.

Rust, single binary, three commands: `ask`, `lint`, `auth`.

## Core goals, in priority order

1. **Catch waste before it is billed.** The API accepts and charges for
   questions that cannot inform anyone: a one-level `score`, a one-option
   `choice`, criteria that merely restate their label. No schema catches these.
   `jev lint` is the reason this tool exists; `ask` is the smaller half.
2. **Never shorten the semantics.** Terse input syntax is fine for the
   *envelope* and forbidden for the *content*. Criteria text is the prompt.
   Measured: bare labels chose `analogy` (0.75) where written descriptions
   chose `popular_opinion` (0.76) on identical input. The verdict inverted.
3. **Stay honest about speed.** The network is ~273 ms; all local work is under
   0.1% of that. `lint` is fast because it makes no call, not because Rust
   parses quickly. Do not put a validation-speed claim in user-facing text.
4. **Keep the surface small.** Three commands. Every addition needs a reason
   that survives "could the user just write ten lines of code instead".

## Scope

**In scope**

- `jev ask`: send a question set, print answers
- `jev lint`: offline schema and semantic validation
- `jev auth`: keyring-backed credentials
- YAML and JSON input, in both terse and canonical spellings

**Out of scope for now**

- Saved recipes. The intended next step, deliberately not yet built: the
  question set format has to prove itself first.
- Batch processing and concurrency.
- Calibration sweeps over labeled data.
- An MCP server. Explicitly not wanted.
- Anything that owns control flow between passes. Gating pass 2 on pass 1 is a
  program; write it in a program.

## Commands

```bash
make build            # cargo build --release
make test             # cargo test (all offline, no API key)
make check            # fmt + clippy + test, what CI runs
make lint             # cargo clippy -D warnings
make install-global   # install to ~/.local/bin
```

## Layout

```
src/types.rs   # Wire types. Transport only, no authoring logic.
src/client.rs  # HTTP, and translation of the server's Zod errors.
src/input.rs   # YAML/JSON parsing, terse and canonical forms.
src/lint.rs    # The differentiator. Schema + semantic rules.
src/main.rs    # CLI wiring.
examples/      # severity.yaml is clean; bad-questions.yaml fails every rule.
```

## Hard-won API details

- Decisions go to `/api/alpha/decisions`. Jev rejects `/v1/chat/completions`.
- **A `noul` answer is a probability in `[0,1]`, not a boolean.** Treating it as
  truthy makes every answer "yes". Deliberately no `as_bool()` helper exists.
- `criteria` is required for `choice` and `score`, optional for `noul`.
- `noul` answers carry no `confidence` field; only `choice` and `score` do.
- `instructions` and `state` each accept a string, an object, or an array. Lint
  rules must read through all three, which is what `guidance_text` is for.
- Server-side validation is Zod. Errors arrive as a JSON-encoded string nested
  inside `error.message`, so the useful part is two layers deep.
- Context limit is 32k tokens over the whole payload, not just the state.
  `estimate_tokens` is chars/4 and measures **high**: a payload it estimated at
  38,751 tokens was reported by the API as 31,272 actual tokens and the call
  succeeded. An early version errored there and blocked a working request. A
  local estimate that can be wrong in the user's favor must warn, not error.
- Additional questions add no latency: Jev answers them in parallel. Batch into
  one request rather than looping.

## Rules

- **Every lint rule needs an observed failure behind it, not an intuition.**
  Put the evidence in the rule's `help` text. A rule nobody can justify is a
  rule that trains users to pass `--no-lint`.
- Errors are for things the API rejects. Everything else is a warning. Do not
  promote a heuristic to an error.
- **A rule built on a local estimate must warn, never error.** Estimates can be
  wrong in the user's favor, and an error then blocks a call that would have
  worked. This already happened once with `context-overflow`. Before shipping a
  new error rule, confirm with `--no-lint` that the API really does reject the
  payload.
- Warnings go to stderr in `ask`, so piping stdout to `jq` stays safe.
- Tests must not require an API key or a network connection. All of them.
- Never log, print, or echo a key. `auth status` prints a fingerprint: length
  plus the last four characters. `auth login` reads from a hidden TTY and never
  from argv.
- Keep `client.rs` free of question-authoring logic and `lint.rs` free of HTTP.
- When adding an input shorthand, ask whether it can shorten criteria. If it
  can, it is the wrong shorthand.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:970c3bf2 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.

## Agent Context Profiles

The managed Beads block is task-tracking guidance, not permission to override repository, user, or orchestrator instructions.

- **Conservative (default)**: Use `bd` for task tracking. Do not run git commits, git pushes, or Dolt remote sync unless explicitly asked. At handoff, report changed files, validation, and suggested next commands.
- **Minimal**: Keep tool instruction files as pointers to `bd prime`; use the same conservative git policy unless active instructions say otherwise.
- **Team-maintainer**: Only when the repository explicitly opts in, agents may close beads, run quality gates, commit, and push as part of session close. A current "do not commit" or "do not push" instruction still wins.

## Session Completion

This protocol applies when ending a Beads implementation workflow. It is subordinate to explicit user, repository, and orchestrator instructions.

1. **File issues for remaining work** - Create beads for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Handle git/sync by active profile**:
   ```bash
   # Conservative/minimal/default: report status and proposed commands; wait for approval.
   git status

   # Team-maintainer opt-in only, unless current instructions forbid it:
   git pull --rebase
   bd dolt push
   git push
   git status
   ```
5. **Hand off** - Summarize changes, validation, issue status, and any blocked sync/commit/push step

**Critical rules:**
- Explicit user or orchestrator instructions override this Beads block.
- Do not commit or push without clear authority from the active profile or the current user request.
- If a required sync or push is blocked, stop and report the exact command and error.
<!-- END BEADS INTEGRATION -->

<!-- BEGIN BEADS CODEX SETUP: generated by bd setup codex -->
## Beads Issue Tracker

Use Beads (`bd`) for durable task tracking in repositories that include it. Use the `beads` skill at `.agents/skills/beads/SKILL.md` (project install) or `~/.agents/skills/beads/SKILL.md` (global install) for Beads workflow guidance, then use the `bd` CLI for issue operations.

### Quick Reference

```bash
bd ready                # Find available work
bd show <id>            # View issue details
bd update <id> --claim  # Claim work
bd close <id>           # Complete work
bd prime                # Refresh Beads context
```

### Rules

- Use `bd` for all task tracking; do not create markdown TODO lists.
- Run `bd prime` when Beads context is missing or stale. Codex 0.129.0+ can load Beads context automatically through native hooks; use `/hooks` to inspect or toggle them.
- Keep persistent project memory in Beads via `bd remember`; do not create ad hoc memory files.

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.
<!-- END BEADS CODEX SETUP -->
