# CLAUDE.md

See [AGENTS.md](AGENTS.md) for the full guidance. Summary:

`jevkit` is a Rust CLI (binary: `jev`) for TypeSafe's Jev decisions model.
Three commands: `ask`, `lint`, `auth`.

```bash
make build    # cargo build --release
make test     # offline tests, no API key needed
make check    # fmt + clippy + test
```

Two things to internalize before changing anything:

- **A `noul` answer is a probability in [0,1], not a boolean.** Treating it as
  truthy makes every answer "yes".
- **Criteria text is the prompt, not documentation.** Shortening it changes
  answers. Measured: bare labels flipped a verdict.

`lint` is the point of the tool. Every rule needs an observed failure behind it.
