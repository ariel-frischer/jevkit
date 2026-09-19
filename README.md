<div align="center">

<pre>
   ██ ██████ ██  ██ ██ ▄█▀ ██ ██████ 
   ██ ██▄▄   ██▄▄██ ████   ██   ██   
████▀ ██▄▄▄▄  ▀██▀  ██ ▀█▄ ██   ██   
</pre>

**Typed decisions from the command line.**

[![CI](https://img.shields.io/github/actions/workflow/status/ariel-frischer/jevkit/ci.yml)](https://github.com/ariel-frischer/jevkit/actions)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

</div>


[TypeSafe’s Jev](https://docs.typesafe.ai) returns typed decisions instead of
prose. `jevkit` is a CLI for it: ask Jev questions about a passage, get back
probabilities, labels, and scores. `jev lint` validates a question set offline,
before you spend anything on a call.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/ariel-frischer/jevkit/main/install.sh | sh
```

More ways to install: see [Installation details](#installation-details).

## Usage

```console
$ jev ask -q severity.yaml "The deploy script drops the production database with no confirmation."
{
  "risky": 0.96,
  "severity": "high",
  "urgency": 2.82
}
```

No file, either: a question set passed inline is a one-liner.

```console
$ jev ask --question-set '{"risky":{"type":"noul","instructions":"Is this risky?"}}' "jumping into a volcano"
{
  "risky": 0.97
}
```

**Built for AI agents too.** `jev` ships with an [agent skill](#the-agent-skill)
that teaches a coding agent to drive the CLI; agents can also consume it via
`npx skills add ariel-frischer/jevkit`.

## Features

- 🎯 **Typed questions**: `noul` (probability), `choice` (label), `score` (level), sent in terse YAML or JSON, by file, stdin, or `--question-set` inline
- 🦀 **Offline linting**: 13 rules that catch billed-but-useless questions before a call; documented exit codes for CI
- 📊 **Usage ledger**: `jev ask --log` appends one JSON line per call (request, response, tokens, cost) to `~/.local/state/jev/usage.jsonl`
- 🛠 **Config**: user defaults in `~/.config/jev/config.toml` (`provider`, `model`, `log`), layered flags > env > config > defaults
- 🔐 **Keyring auth**: keys stored in the OS credential store, never argv or shell history; fingerprint display rather than the key itself
- ⚡ **One call, all answers**: additional questions are answered in parallel by the API, so batching costs nothing in latency

## Why this exists

Calling the decisions API is not hard: one POST, three question types. You can
do it with `curl`. Three things a wrapper can do that curl cannot.

**Less to write per call.** Every call otherwise re-emits the endpoint, auth
header, model string, and the typed envelope: 308 raw chars vs 198 in terse
YAML, 36% less. The terse form expands back into the same JSON before sending,
so the wire payload and the API cost are identical. The saving is in what you
write, not in what you are billed for.

**Validation before you are billed.** The API accepts and charges for questions
that cannot inform you, all returning HTTP 200:

| Question | What comes back | Why it is useless |
|---|---|---|
| `score` with one level | `score: 0, confidence: 1` | The answer was forced |
| `choice` with one option | that option at probability `1.0` | The answer was forced |
| Criteria that restate the label | a plausible-looking label | See below |

The last case is the dangerous one, because the result looks fine. On identical
input, with only the descriptions altered, the verdict inverted: bare
`"analogy"` labels scored `analogy` (0.75), while written descriptions scored
`popular_opinion` (0.76). **Criteria text is the prompt, not documentation.** No
schema catches any of this; `jev lint` does, offline, before you spend anything.

**Fast where speed is actually available.** The API is ~273 ms per call; `jev
lint` is ~0.5 ms with no network call at all, fast enough for a pre-commit hook
over a large question set. Nothing makes `ask` faster than the network, and this
tool does not claim otherwise.

## Installation details

### Build from source

```bash
git clone https://github.com/ariel-frischer/jevkit.git
cd jevkit
make install-global      # builds and installs ~/.local/bin/jev
jev init                 # provider, custom endpoint, key in the OS keyring
# or non-interactive: `jev init --no-key --provider openrouter` then `jev auth login`
```

### Installer internals

It downloads the matching prebuilt binary from GitHub Releases (linux
x86_64/aarch64, macOS x86_64/aarch64), verifies the SHA-256 checksum, backs up an
existing install, and warns if `~/.local/bin` is not on your `PATH`. Override the
version with `JEV_VERSION=v0.1.0`.

### Compatibility

linux x86_64/aarch64 and macOS x86_64/aarch64 (glibc-based linux, e.g. Ubuntu
20.04+, Debian 11+, glibc ≥ 2.31; macOS 11+). Windows needs WSL.

### The agent skill

`.skills/jevkit/` is an [agent skill](https://skills.sh/) that teaches a coding
agent to drive the CLI, including the failure modes that are easy to hit.

Or install the bundled agent skill as a one-liner:

```bash
npx skills add ariel-frischer/jevkit
npx skills ls -g | grep jevkit   # confirm registration
```

One registration covers Claude Code, Codex, Gemini CLI, OpenCode, and Zed,
since they all read `~/.agents/skills`.

When contributing, install the skill by symlink so repository edits stay live.

## Commands

### `jev ask`

Send a question set and print one value per question.

```bash
jev ask -q questions.yaml --file passage.txt   # state from a file
echo "some text" | jev ask -q questions.yaml   # state from stdin
jev ask -q questions.yaml "some text"          # state as an argument

jev ask -q questions.yaml --dry-run            # print the payload, send nothing
jev ask -q questions.yaml --raw                # full response with usage and cost
```

```console
`jev lint --question-set` works the same way.

Linting runs automatically. Errors block the call, warnings print to stderr and
proceed. Warnings go to stderr specifically so that piping stdout to `jq` stays
safe.

JSON output is pretty-printed on a terminal and one-line when stdout is a pipe,
so `jev ask ... | jq` gets compact JSON by default. Pass `--pretty` to force the
indented form in scripts, or `--compact` to force one-line everywhere; the two
flags are mutually exclusive.

Exit codes for `ask`, for use in scripts and pipelines:

- `0` the call succeeded, JSON on stdout
- `1` usage, credential, or API failure
- `2` lint errors (pass `--no-lint` to proceed); nothing was sent, nothing was
  billed, and stdout is empty

### `jev lint`

Validate a question set without touching the network.

```bash
jev lint questions.yaml
jev lint --json questions.yaml          # machine-readable findings
jev lint --strict questions.yaml        # warnings fail CI (alias: --deny-warnings)
jev lint --quiet questions.yaml         # rule ids only, no help blocks
```

Exit codes follow a documented contract, for use in scripts and CI:

- `0` the question set is clean
- `1` errors were found (the API would reject these), or warnings with `--strict`
- `2` warnings only, which are heuristics rather than rejections

`--json` prints the findings as machine-readable JSON with rule id, severity,
message, path, and help -- pretty on a terminal, one-line when piped, with
`--pretty`/`--compact` to override either way. Text mode prints findings to
stdout so piping to `jq`
stays safe. `--quiet` drops the help blocks and prints rule ids only, roughly
cutting text output in half; useful when an agent or script consumes the ids.
Shell completions and a man page are behind the hidden `completions`
subcommand, generated at build time alongside the binary.

### `jev auth`

First-time setup: `jev init` interactively picks a provider (or a custom
decisions URL), stores the API key in the OS keyring, and writes the config
defaults. Flags pre-answer it for scripts and agents:
`jev init --no-key --provider typesafe`.

```bash
jev auth login      # prompts on a hidden TTY, stores in the OS keyring
jev auth status     # shows which credential would be used, never the value
jev auth logout
```

Keys resolve in this order: `--api-key`, then the provider's environment
variable, then the OS keyring. `auth login` never accepts a key as an argument,
because argv is readable by other processes and lands in shell history.

### `jev config`

User defaults in `~/.config/jev/config.toml`. Keys: `provider`, `model`,
`endpoint` (override the decisions URL, e.g. a proxy or self-hosted install),
and `log`, and `lint_verbosity` (`full` default, or `quiet` for findings with rule
ids only; a `--quiet` flag on `ask` and `lint` overrides it).

```bash
jev config keys     # what is configurable
jev config set model typesafe/jev-1.13
jev config show
```

Settings layer highest first: flags > `JEV_*` env vars > config.toml >
built-in defaults.

### Usage ledger

`jev ask --log` appends one JSON line per call (request, response, tokens,
cost) to `~/.local/state/jev/usage.jsonl`. Opt-in only; failed calls are
logged too. `config set log <path-or-1>` makes a bare `--log` use it.

## Writing questions

**YAML or JSON, anywhere a question set is accepted**, by file or on stdin. Ready-made examples are in [`examples/`](examples/), including a clean `severity.yaml` and a file that fails every lint rule (`bad-questions.yaml`). The
format is detected automatically, with no flag: JSON is tried first, because
every JSON document is also valid YAML but the JSON parser gives better errors.

```bash
jev ask -q questions.yaml "text"
jev ask -q questions.json "text"
cat questions.json | jev lint
```

Within either format there are two spellings, terse and canonical. The terse
one exists because the API envelope is verbose: on a two-question payload, 59%
of the JSON was structure (`"type"`, `"instructions"`, `"criteria"`, nesting)
and only 41% was content.

```yaml
# Terse: the primitive is the key, its value is the instructions.
risky:
  noul: Does the passage describe an irreversible operation?

severity:
  choice: How severe is the worst outcome the passage describes?
  options:
    low: Cosmetic or easily reversed with no lasting effect
    high: Permanent data loss or an outage affecting users
    none: The passage does not describe an operation with consequences

urgency:
  score: How soon must someone act?
  levels:
    - Can wait for the next planning cycle
    - Needs attention today
    - Requires an immediate response
```

```yaml
# Canonical: exactly what goes on the wire.
risky:
  type: noul
  instructions: Does the passage describe an irreversible operation?
```

The terse form drops the envelope, never the content. Criteria stay full
sentences in both.

### YAML scalar typing

YAML types an unquoted `1.5` as a number and `true` as a boolean, and the API
requires criteria to be text. `jev` coerces those back to strings rather than
making you quote them, since the text is the only thing they could have meant.
A quoted `"1.5"` is untouched, and objects and arrays are left alone.

Most other YAML-to-JSON complaints do not apply: `12:30`, `0755`, and a
20-digit integer all stay strings under YAML 1.2, and a bare `NO` label stays
`"NO"` rather than becoming `false`.

Two things are still fatal, and both report what to do about it:

- An unquoted value containing `": "`, which YAML reads as a nested mapping.
  This is the likeliest mistake in a criteria file, since descriptions are
  prose. Quote the whole value.
- Tab indentation, which YAML forbids.

`instructions` also accepts a structured object, which the API supports and
which lint reads through:

```yaml
q:
  noul:
    question: Does the `message` ask the recipient to disclose a credential?
    focus: A request to send the credential, not to reset it.
```

## Lint rules

| Rule | Severity | Catches |
|---|---|---|
| `missing-criteria` | error | `choice`/`score` without criteria, which the API rejects |
| `non-string-criteria`, `non-string-level` | error | A non-string criteria value, for requests built in code; the CLI coerces these |
| `empty-instructions` | error | A question with nothing to judge |
| `context-overflow` | error | A payload that cannot fit the 32k-token limit under any tokenization |
| `degenerate-criteria` | warning | Criteria that restate the label |
| `single-option` | warning | A `choice` with one option |
| `single-level` | warning | A `score` with one level |
| `numeric-level` | warning | Levels described as bare numerals |
| `conditional-question` | warning | "if applicable" phrasing, which returns ~0.5 regardless |
| `compound-question` | warning | A `noul` weighing two properties at once |
| `no-escape-option` | warning | A `choice` that cannot decline to answer |
| `context-pressure` | warning | A payload that may exceed the token limit |
| `terse-instructions` | warning | Instructions too short to state a condition |

Errors are certain. Warnings are heuristics: they flag questions that are valid
but probably not what you meant.

## Performance

See [Why this exists](#3-fast-where-speed-is-actually-available) for the
numbers. The short version: the network is ~273 ms and everything local is
under 0.1% of that, so only `lint`, which makes no call, is meaningfully fast.

One thing that *does* move the needle: Jev answers every question in a request
in parallel. Measured on the same passage, ten questions were no slower than
one, for 39% more cost:

| Questions | Median latency | Cost |
|---|---|---|
| 1 | 266 ms | $0.0000120 |
| 10 | 230 ms | $0.0000167 |

So batch questions into one request rather than looping. Ten thin calls would
have cost ten round trips and ten times the base overhead.

## Continuous integration

GitHub Actions runs on push and pull request.

| Job | Runs |
|---|---|
| fmt | `cargo fmt --check` |
| clippy | `cargo clippy --all-targets -- -D warnings` |
| test | offline tests only, no API key or network required |
| build | `cargo build --release` |
| msrv | the 1.88 minimum supported Rust version |
| cargo-deny | dependency advisory audit, weekly |

## Notes on the API

- Decisions go to `/api/alpha/decisions`. Jev rejects `/v1/chat/completions`.
- **A `noul` answer is a probability in `[0,1]`, not a boolean.** Treating it as
  truthy makes every answer "yes". There is deliberately no `as_bool()` helper
  in this codebase: pick a threshold explicitly.
- `criteria` is required for `choice` and `score`, optional for `noul`.
- `noul` answers carry no `confidence` field. Only `choice` and `score` do.
  Derive uncertainty from the probability's distance from 0.5.
- The context limit is 32k tokens, counted over the whole payload. Note that
  `jev`'s local estimate is a crude chars/4 heuristic and measures high: a
  payload it estimated at 38,751 tokens was reported by the API as 31,272 and
  succeeded. So a near-limit payload only warns, and `--raw` reports the real
  `usage.input_tokens`.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for
guidelines; CI runs fmt, clippy, offline tests, and the MSRV build.

## Scope

Three commands, deliberately. Saved recipes, batch processing, and calibration
sweeps are plausible next steps but are not here yet.

## License

`jevkit` is licensed under the MIT license. See the
[`LICENSE`](LICENSE) file for more information.
