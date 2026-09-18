# Performance optimizations for `jev`

Research into Rust performance techniques evaluated against this codebase,
date 2026-09-18. Conclusion up front: the Cargo build profile was already
production-tuned, so almost every headline technique from the perf literature
was already in place. The wins that actually applied were algorithmic,
allocation-level changes in `lint.rs` and `types.rs`.

## Measured baseline (before this work)

- `jev lint examples/*.yaml`: ~530 us per invocation end to end (hyperfine, 5+ ms
  shell-stripped). Local work is negligible versus the ~273 ms network call in
  `ask`, consistent with the comment in `main.rs`.
- `jev ask` with a 117 KB state (`--dry-run`, no network): ~770 us; with lint
  enabled ~780 us. Binary 7.6 MB, stripped.

## Techniques evaluated

### Already in place (no action)

These were verified present in `Cargo.toml` `[profile.release]`:

- **Fat LTO** (`lto = true`) and **`codegen-units = 1`**: the standard
  production pair (Microsoft Rust Engineering Practices ch7; rustacean
  "LTO and codegen-units"). Percentile gain over thin LTO is small for a
  binary this small and would cost release build time.
- **`panic = "abort"`**, **`strip = true`**: smaller binary, no unwinding code.
- **`blocking` reqwest with no async runtime in the lint/auth path**: the
  client deliberately avoids a Tokio runtime start-up tax for a one-shot CLI.
- **reqwest with `default-features = false`, rustls**: no system OpenSSL
  dependency and a smaller TLS stack.

Rejected techniques, with reasons:

- **PGO / BOLT post-link optimization**: needs profiling infrastructure
  (`perf`, LLVM tools) that is not available on this box without policy change;
  measured effect on a binary whose hot work is ~30 us of parsing is noise-level.
- **`simd-json` or `sonic-rs` replacing `serde_json`**: serde_json wins the
  small-payload case ([rust-json-parsing-benchmarks] showed serde_json 1.6x
  faster than simd-json on small objects), and our payloads are question-set
  sized, not bulk-data sized. The parse step is under a millisecond for YAML
  and microseconds for JSON.
  Not worth a dependency swap and `preserve_order` behavior difference risk.
- **`opt-level = "z"` or `"s"`**: hurts runtime; runtime is one of the things
  being marketed here (see `Cargo.toml` description), and binary size is not
  currently a pain point.
- **`target-cpu=native`**: invalid for a distributed CLI (rustacean pitfall).

### Implemented

1. **Counting-sink serialization instead of building the payload string**
   (`src/types.rs` `CountingSink` + `serialized_len`, used by `lint_request`).

   `lint_request` estimated tokens over the whole request by calling
   `serde_json::to_string(req)`, which allocates an entire copy of the payload
   (state can be tens or hundreds of KB) just to measure it and throw it away.
   Replaced with a `std::io::Write` sink that discards bytes and returns the
   byte count, plus a byte-length variant of the char-based token estimator.

2. **`Cow<str>` in `guidance_text`** (`src/lint.rs`).

   Flattening guidance to text cloned the inner string on the dominant plain
   string path. Now borrows in that case and only allocates for the rare
   object-array-joined shapes; single-element containers are unrolled without
   a join.

## Measured result (after this work)

Same axes, 2026-09-18, release profile, 5 repeated hyperfine runs on
`ask --dry-run` (117 KB state):

| baseline mean | optimized mean | ratio |
|---|---|---|
| 779.7 us | 726.0 us | 1.074 |
| 775.4 us | 705.4 us | 1.099 |
| 764.7 us | 712.6 us | 1.073 |
| 785.3 us | 713.2 us | 1.101 |
| 759.5 us | 699.7 us | 1.085 |

Consistent 7-10% faster; same order of error bars, but the pattern held in
every run and the two changes remove allocations on the measured path, so the
direction and rough magnitude are snapshotted here rather than claimed as
exact. Output is byte-identical to the baseline (verified by diff on both
`ask --dry-run` and `lint`).

## What did not move the needle

- `guidance_text` `Cow` change: on these test payloads with plain-string
  criteria it saves only a handful of allocations. Kept because it is simpler
  than the old version and helps proportionally more with structured
  instruction objects.
- Any profile-flag change: nothing to do, the profile was already at
  production optimum.

## Open areas not pursued

- Binary size (cargo-bloat + dep trimming) if `ask` becomes the dominant
  user path and the binary gets noticeably bigger. Not a current problem.
- Startup latency from dynamic linking (measured 530 us for the whole `lint`
  invocation including process spawn, so the floor is already tiny).
- PGO/BOLT once this binary is on many machines with perf available and a C
  workload profile to justify it.
- Cached/parallel linting if question sets grow into the thousands of
  questions; not currently a real workload.

## Acceptance-path validation (second round, same day)

Beyond the hyperfine numbers, the shipped binary was diffed against the
pre-change baseline (`/tmp/jev-baseline`, commit e2f765a) across the real
acceptance paths:

- **Token-estimate boundaries**: states of 120 KB (no finding), 135 KB
  (`context-pressure`), and 520 KB (`context-overflow`, exit 1) produce
  identical findings on both binaries. The estimate did not shift.
- **Serialization parity**: `ask --dry-run` full-request output is
  byte-identical on 12 randomized states (mixed size 1 KB-300 KB, unicode and
  escape characters mixed in) and on 7 generated question-sets (terse and
  canonical, all three primitives, nested objects, 50-question set) crossed
  with two state shapes. `lint --json` is byte-identical on the same corpus
  plus a terse-YAML file.
- **Error paths**: malformed YAML and a missing file produce identical
  messages and exit codes.
- **Piping contract**: with lint warnings enabled, stderr content is
  identical; stdout stays pure JSON (0 stderr bytes when piping stdout only).

Perf numbers by workload:

| workload | baseline | optimized | ratio |
|---|---|---|---|
| `ask --dry-run`, 117 KB state (5 runs) | 760-785 us | 700-726 us | 1.07-1.10x |
| `lint`, 50-question set (4 runs) | 618-630 us | 610-624 us | ~1.01x, within noise |

The 50-question set shows no measurable gain, as expected: that path never
serialized a large payload, and the question-set parse dominates. The gain
concentrates exactly where the removed allocation lived, which is the
evidence that the mechanism, not luck, produced it.

`cargo test` passes in both debug and release (32 passed, 1 ignored: the
keyring round-trip that requires a desktop Secret Service).

## References

- <https://doc.rust-lang.org/cargo/reference/profiles.html> for profile knobs.
- <https://microsoft.github.io/RustTraining/engineering-book/ch07-release-profiles-and-binary-size.html>
  for the release-profile checklist and cargo-bloat workflow.
- <https://agricidaniel.github.io/rustacean/patterns/LTO-and-codegen-units>
  for thin vs fat LTO tradeoffs.
- <https://github.com/AnnikaCodes/rust-json-parsing-benchmarks> for the
  serde_json vs simd-json evidence.
