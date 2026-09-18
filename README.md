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

One line, Linux and macOS (no Rust toolchain needed; the installer downloads a
prebuilt binary from GitHub Releases and verifies its SHA-256 checksum):

```sh
curl -fsSL https://raw.githubusercontent.com/ariel-frischer/jevkit/main/install.sh | sh
```

Or build from source:

```bash
git clone https://github.com/ariel-frischer/jevkit.git
cd jevkit && make install-global && jev auth login
```

`cargo install --path .` works too. See [Install](#install-1) for Windows, version
overrides, and the agent skill.

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
