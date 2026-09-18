# jevkit

A CLI for [TypeSafe's Jev](https://docs.typesafe.ai), a model that returns
typed decisions instead of prose. Ask it questions about a passage and get back
probabilities, labels, and scores.

The binary is called `jev`.

```console
$ echo "The deploy script drops the production database with no confirmation." \
    | jev ask -q severity.yaml
{
  "risky": 0.96,
  "severity": "high",
  "urgency": 2.82
}
```

## Why this exists

Calling the decisions API is not hard: one POST, three question types. You can
do it with `curl`. So the bar for a tool is whether it removes work that
actually costs something. Three things do.

### 1. Less to write per call

Otherwise every call means re-emitting the endpoint, the auth header, the model
string, and the full typed envelope. For an agent that is output tokens spent
on structure rather than on the question:

```text
raw JSON envelope   308 chars
terse YAML          198 chars   36% less
```

To be clear about what this does *not* do: the terse form expands back into the
same JSON before sending, so the wire payload and the API cost are identical.
The saving is in what you write, not in what you are billed for.

Getting the envelope wrong has its own cost, and the CLI makes each of these
structurally impossible: posting to `/v1/chat/completions` (Jev rejects it),
omitting `criteria` on a `choice` (HTTP 400), and reading a `noul` answer as a
boolean, which makes every answer "yes" and still looks like a working
classifier.

### 2. Validation before you are billed

This is the part no other wrapper does, and the reason the tool is worth
installing. **The API accepts and charges for questions that cannot inform
you.** All three of these return HTTP 200:

| Question | What comes back | Why it is useless |
|---|---|---|
| `score` with one level | `score: 0, confidence: 1` | The answer was forced |
| `choice` with one option | that option at probability `1.0` | The answer was forced |
| Criteria that restate the label | a plausible-looking label | See below |

The last case is the dangerous one, because the result looks fine. On identical
input, with the option list unchanged and only the descriptions altered:

```text
criteria: {"analogy": "analogy", ...}                  -> analogy         (0.75)
criteria: {"analogy": "The conclusion transfers from
           a relevantly similar case", ...}            -> popular_opinion (0.76)
```

The verdict inverted. **Criteria text is not documentation, it is the prompt.**

No schema catches any of this. `jev lint` does, offline, before you spend
anything.

### 3. Fast where speed is actually available

Honest accounting, measured against the live endpoint:

| Stage | Time |
|---|---|
| API call (TTFB) | ~273 ms |
| `jev lint`, including process spawn | **0.54 ms** |
| YAML parse | ~0.16 ms |

Rust does not make `ask` faster, and nothing else would either: the network is
99.9% of it, and call latency varied 173-868 ms across repeated runs, which
dwarfs anything local. A Jev CLI sold on request-path speed is selling noise.

What Rust buys is a dependency-free static binary and a `lint` that makes no
network call at all, fast enough to run over a large question set in a
pre-commit hook. That is the one place the speed is real.

## Install

### The CLI

```bash
git clone git@gitlab.com:ariel-frischer/jevkit.git
cd jevkit
make install-global      # builds and installs ~/.local/bin/jev
jev auth login           # store a key in the OS keyring
```

`cargo install --path .` works too. Requires a Rust toolchain.

### The agent skill

`.skills/jevkit/` is an [agent skill](https://skills.sh/) that teaches a coding
agent to drive the CLI, including the failure modes that are easy to hit.

While this repository is private, install it by symlink so repository edits stay
live:

```bash
ln -s "$PWD/.skills/jevkit" ~/.agents/skills/jevkit
ln -s ../../.agents/skills/jevkit ~/.claude/skills/jevkit   # if you use Claude Code
npx skills ls -g | grep jevkit                              # confirm registration
```

One symlink covers Claude Code, Codex, Gemini CLI, OpenCode, and Zed, since they
all read `~/.agents/skills`.

Were the repository public on GitHub, the usual one-liner would apply:

```bash
npx skills add ariel-frischer/jevkit
```

`npx skills add` resolves GitHub repositories, so it cannot reach a private
GitLab project. Use the symlink above.

## Commands

### `jev ask`

Send a question set and print one value per question.

```bash
jev ask -q questions.yaml --file passage.txt   # state from a file
echo "some text" | jev ask -q questions.yaml   # state from stdin
jev ask -q questions.yaml "some text"          # state as an argument

jev ask -q questions.yaml --dry-run            # print the payload, send nothing
jev ask -q questions.yaml --raw                # full response with usage and cost

jev ask --question-set '{"risky":{"type":"noul","instructions":"Risky?"}}' "some text"
```

`--question-set` passes a question set inline as a YAML or JSON string, no file
needed. `jev lint --question-set` works the same way.

Linting runs automatically. Errors block the call, warnings print to stderr and
proceed. Warnings go to stderr specifically so that piping stdout to `jq` stays
safe.

### `jev lint`

Validate a question set without touching the network.

```bash
jev lint questions.yaml
jev lint --json questions.yaml          # machine-readable findings
jev lint --strict questions.yaml        # warnings fail CI (alias: --deny-warnings)
```

Exit codes follow a documented contract, for use in scripts and CI:

- `0` the question set is clean
- `1` errors were found (the API would reject these), or warnings with `--strict`
- `2` warnings only, which are heuristics rather than rejections

`--json` prints the findings as machine-readable JSON with rule id, severity,
message, path, and help. Text mode prints findings to stdout so piping to `jq`
stays safe. Shell completions and a man page are behind the hidden `completions`
subcommand, generated at build time alongside the binary.

### `jev ask` inline question sets

```bash
jev ask --question-set '{"risky":{"type":"noul","instructions":"Risky?"}}' "some text"
```

`--question-set` passes a question set inline as a YAML or JSON string, no file
needed. `jev lint --question-set` works the same way.

### `jev auth`

```bash
jev auth login      # prompts on a hidden TTY, stores in the OS keyring
jev auth status     # shows which credential would be used, never the value
jev auth logout
```

Keys resolve in this order: `--api-key`, then the provider's environment
variable, then the OS keyring. `auth login` never accepts a key as an argument,
because argv is readable by other processes and lands in shell history.

### `jev config`

User defaults in `~/.config/jev/config.toml`. Keys: `provider`, `model`, `log`.

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

**YAML or JSON, anywhere a question set is accepted**, by file or on stdin. The
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

GitHub Actions workflow files exist in the repository. They are code only, so
they are committed even though no public repository has been created yet; they
start running the moment one is.

| Job | Runs |
|---|---|
| fmt | `cargo fmt --check` |
| clippy | `cargo clippy --all-targets -- -D warnings` |
| test | offline tests only, no API key or network required |
| build | `cargo build --release` |
| msrv | the 1.82 minimum supported Rust version |
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

## Scope

Three commands, deliberately. Saved recipes, batch processing, and calibration
sweeps are plausible next steps but are not here yet.

## License

MIT
