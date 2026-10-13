# Contributing

We are not accepting outside code contributions yet. Issues and discussions are welcome.

The rest of this page is for reading and building the source: how to build it, how
the checks work, and how to report a problem.

## The licence, and what is published

The source is published under the Functional Source License, Version 1.1, MIT
Future License (FSL-1.1-MIT); see [`LICENSE.md`](LICENSE.md). There is no
published crate. The install instructions in
[`docs/book/src/install.md`](docs/book/src/install.md) say what does work.

## Build prerequisites, which are not obvious

`docs/book/src/install.md` is the authority. Two of its rows exist because a
container built from an earlier version of that page failed on them, several
minutes into a build, from a crate you cannot see in `Cargo.toml`:

- **A C++ compiler, GCC 13 or newer.** The prebuilt ONNX Runtime that `ort-sys`
  downloads needs a GCC 13 libstdc++. **It does not build on Debian bookworm**
  (GCC 12.2) — the link fails on `_M_replace_cold` at about seven minutes.
  Trixie (14.2) installs in about three.
- **`pkg-config` and OpenSSL headers**, for the same dependency chain.

`--no-default-features` drops the ONNX backend and this whole chain with it.

## The rule that surprises people: the docs are tested

A documentation change can fail the build, and usually does when it is wrong.
`crates/cli/tests/m6_docs_are_true.rs` walks the book and asserts:

- every `thruline <verb>` the docs type exists;
- every **flag and subcommand** the docs type parses. A `policy init` written as
  a subcommand fails this, because `init` is a flag;
- every command the **product itself** recommends, in its printed strings,
  parses too;
- every finding the verifier reference names is one a verifier emits, **and**
  every finding a verifier emits is documented;
- every documented finding carries a severity and a worked example;
- every policy key documented is one the schema accepts, and the reverse;
- every `SUMMARY.md` link resolves, and every chapter is linked.

So: **do not write a command into the docs without running it.** If you are
documenting something that does not exist, say so without typing it as a command
— the census cannot tell an example from an explanation, and that is deliberate.

## Running the gate

From a build directory **outside** the worktree, with exit codes read unpiped —
never through `| grep`, which reports the grep's status and has hidden real
failures here:

```sh
D=$(mktemp -d)
CARGO_TARGET_DIR="$D" cargo test --workspace --no-fail-fast; echo "tests: $?"
CARGO_TARGET_DIR="$D" cargo clippy --all-targets -- -D warnings; echo "clippy: $?"
CARGO_TARGET_DIR="$D" cargo fmt --check; echo "fmt: $?"
python3 docs/spec/validate.py;                    echo "spec: $?"
python3 docs/spec/emit_example.py --selftest /tmp/r.json; echo "emitter: $?"
```

Reap `$D` once you have recorded what it produced.

## Writing

These rules govern user-facing text in this repository, and they matter most when
you read the product's own wording. The parts that bite most often:

- **The record is the subject of a negative sentence, never the agent.** "The
  record shows no test run after the last edit", not "the agent skipped the
  tests". The accusation is the reader's to make.
- **Every negative state says what would settle it** — a *What would prove it*
  line.
- **Never "Verification failed."** Never "caught in a lie", in any form,
  including quoting it to deny it.
- **A gap that is honestly named is finished work.** Do not delete a "not built"
  or a "what would prove it" to make a page look complete.

## Security

See [SECURITY.md](SECURITY.md). Do not open a public issue for a vulnerability.

## Issues and discussions

Issues and discussions are welcome: bug reports, a command whose output surprised
you, a page of the docs that is wrong. Say what you ran and what you saw.
