---
name: jevkit
description: >
  Ask TypeSafe's Jev model typed questions (noul, choice, score) from the
  command line, and lint question sets offline before spending an API call.
  Use when writing Jev questions, debugging a Jev question that answers wrong,
  or classifying text without generating prose.
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
  tags: jev, typesafe, cli, classification, llm
allowed-tools: Bash Read Write Edit
---

# jevkit

`jev` asks TypeSafe's Jev model typed questions about a passage and returns
probabilities, labels, and scores instead of text. Use it when code needs a
fast classification or judgment rather than generated prose.

## Commands

```bash
jev ask -q questions.yaml --file passage.txt   # state from a file
echo "text" | jev ask -q questions.yaml        # state from stdin
jev ask -q questions.yaml "text"               # state as an argument
jev ask -q questions.yaml --dry-run            # print the payload, send nothing
jev ask -q questions.yaml --raw                # full response with usage and cost

jev lint questions.yaml                        # offline check, no API call
jev lint --json questions.yaml                 # machine-readable findings
jev lint --deny-warnings questions.yaml        # exit 1 on warnings, for CI

jev auth login                                 # hidden prompt, OS keyring
jev auth status                                # which key, never the value
```

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

The canonical `{type, instructions, criteria}` form is accepted too, as is
JSON. `instructions` may be an object with `question`, `focus`, and `inspect`
keys when a question needs supporting detail.

## Choosing a primitive

| Primitive | Use when | Returns | Act on it with |
|---|---|---|---|
| `noul` | A crisp yes or no, where the probability is the signal | a number in `[0,1]` | a threshold |
| `choice` | One of a known unordered set | a label plus probabilities | a branch |
| `score` | A position on a describable scale | a continuous position | a threshold or weight |

## Rules that matter

- **A `noul` answer is a probability, not a boolean.** Treating it as truthy
  makes every answer "yes". Choose a threshold deliberately.
- **Write criteria as full descriptions, never bare labels.** Measured on
  identical input, `{"analogy": "analogy"}` chose `analogy` (0.75) while a
  written description chose `popular_opinion` (0.76). The verdict inverted.
- **Never write a conditional question.** "…, if applicable" returns values
  clustered near 0.5, indistinguishable from real uncertainty. Ask positively
  and gate the question in code.
- **Give a `choice` an escape option** (`none`, `other`) unless the label set
  provably covers every input.
- **Ask one property per question.** Split anything with "and" in it and
  combine the answers in code.
- **Batch questions into one request.** Jev answers them in parallel, so ten
  questions cost about one round trip.
- **A `score` needs at least two levels and a `choice` at least two options.**
  Otherwise the answer is forced, and the call is still billed.

## Workflow

1. Write the question set.
2. `jev lint questions.yaml` and fix what it reports. This costs nothing.
3. `jev ask --dry-run` to confirm the payload.
4. `jev ask` against real input, then read the probabilities on any miss with
   `--raw` and revise one or two questions at a time.

## Development

```bash
make build    # cargo build --release
make test     # offline tests, no API key needed
make check    # fmt + clippy + test
```

Use `chlog add ...`, `chlog sync`, and `chlog check` for changelog updates.
