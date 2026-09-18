---
name: jevkit
description: >
  Use the `jev` command-line tool to ask TypeSafe's Jev model typed questions
  (noul, choice, score) and to lint question sets offline before spending an API
  call. Use for CLI invocation, question files, `jev lint`, and credentials. For
  designing Jev questions inside a program, in Python or TypeScript or Go, use
  the `jev` skill instead.
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
  tags: jev, typesafe, cli, classification, llm, linting
allowed-tools: Bash Read Write Edit
---

# jevkit

`jev` asks TypeSafe's Jev model typed questions about a passage and returns
probabilities, labels, and scores instead of text. Use it when code needs a
fast classification or judgment rather than generated prose.

A call costs roughly $0.000012 and takes about 270 ms.

## Check it is installed

```bash
jev --version || echo "not installed"
jev auth status          # shows which credential would be used, never the value
```

If it is missing, see [Installing the CLI](#installing-the-cli).

## The workflow that matters

Lint first. It is free, offline, and catches defects the API will either reject
or, worse, silently bill you for.

```bash
jev lint questions.yaml                 # 1. free, no network
jev ask -q questions.yaml --dry-run "…" # 2. confirm the exact payload
jev ask -q questions.yaml --file in.txt # 3. spend the call
jev ask -q questions.yaml --raw "…"     # 4. read probabilities on a miss
```

## Commands

```bash
# ask: state from stdin, a file, or an argument
echo "text" | jev ask -q questions.yaml
jev ask -q questions.yaml --file passage.txt
jev ask -q questions.yaml "some text"

jev ask -q questions.yaml --raw          # full response, with usage and cost
jev ask -q questions.yaml --dry-run      # print the payload, send nothing
jev ask -q questions.yaml --no-lint      # send anyway, defects and all

# lint: never touches the network
jev lint questions.yaml
jev lint --json questions.yaml           # machine-readable findings
jev lint --deny-warnings questions.yaml  # exit 1 on warnings, for CI

# auth: keyring-backed
jev auth login                           # hidden prompt, never argv
jev auth status
jev auth logout
```

Input is YAML **or** JSON, by file or on stdin, detected automatically with no
flag.

## Writing a question set

```yaml
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

Output is one value per question:

```json
{ "risky": 0.90, "severity": "high", "urgency": 1.97 }
```

The canonical `{type, instructions, criteria}` form works too. `instructions`
may be an object with `question`, `focus`, and `inspect` keys when a question
needs supporting detail.

## Choosing a primitive

| Primitive | Use when | Returns | Act on it with |
|---|---|---|---|
| `noul` | A crisp yes or no, where the probability is the signal | a number in `[0,1]` | a threshold |
| `choice` | One of a known unordered set | a label plus probabilities | a branch |
| `score` | A position on a describable scale | a continuous position | a threshold or weight |

## Rules that matter

- **A `noul` answer is a probability, not a boolean.** Treating it as truthy
  makes every answer "yes" and still looks like a working classifier. Choose a
  threshold deliberately.
- **Write criteria as full descriptions, never bare labels.** Measured on
  identical input, `{"analogy": "analogy"}` chose `analogy` (0.75) while a
  written description chose `popular_opinion` (0.76). The verdict inverted.
  Criteria text is the prompt, not documentation.
- **Quote anything YAML might read as a number or boolean.** A bare `1.5` level
  becomes a JSON number and the API rejects it outright.
- **Never write a conditional question.** "…, if applicable" returns values
  near 0.5, indistinguishable from real uncertainty. Ask positively and gate the
  question in code.
- **Give a `choice` an escape option** (`none`, `other`) unless the label set
  provably covers every input.
- **Ask one property per question.** Split anything with "and" in it and
  combine the answers in code.
- **Batch questions into one request.** Measured: ten questions were no slower
  than one (230 ms vs 266 ms) for 39% more cost. Ten separate calls would cost
  ten round trips.
- **A `score` needs two or more levels and a `choice` two or more options.**
  Otherwise the answer is forced, and the call is still billed.

## What lint catches

Errors are things the API rejects. Warnings are questions that are valid but
probably not what you meant.

| Rule | Severity |
|---|---|
| `missing-criteria` | error |
| `non-string-criteria`, `non-string-level` | error |
| `empty-instructions`, `empty-level` | error |
| `context-overflow` | error |
| `degenerate-criteria` (criteria restate the label) | warning |
| `single-option`, `single-level` | warning |
| `conditional-question` | warning |
| `no-escape-option` | warning |
| `compound-question`, `terse-instructions` | warning |
| `numeric-level`, `too-many-levels` | warning |
| `context-pressure` | warning |

## Debugging a wrong answer

1. `jev ask --raw` and read `probabilities`, not just the chosen label. A
   near-uniform distribution means the question did not discriminate.
2. Check the criteria actually describe *when* each option applies.
3. Split any question judging two properties at once.
4. Revise one or two questions at a time, not all of them.
5. Do not trust Jev's self-reported `confidence`. Derive uncertainty from a
   probability's distance from 0.5.

## Installing the CLI

The skill is guidance; `jev` is a separate binary.

```bash
git clone git@gitlab.com:ariel-frischer/jevkit.git
cd jevkit && make install-global      # installs to ~/.local/bin/jev
jev auth login                        # store an OpenRouter key in the keyring
```

Requires a Rust toolchain. Credentials resolve as `--api-key`, then
`OPENROUTER_API_KEY`, then the OS keyring.

## API details worth knowing

- Decisions go to `/api/alpha/decisions`. Jev rejects `/v1/chat/completions`.
- `criteria` is required for `choice` and `score`, optional for `noul`.
- `noul` answers carry no `confidence` field. Only `choice` and `score` do.
- The context limit is 32k tokens over the whole payload.
- Jev answers all questions in a request in parallel.
