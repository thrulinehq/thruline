# Install

## What your machine needs first

| | why |
|---|---|
| **Rust 1.97.1** | pinned in `rust-toolchain.toml`; `rustup` will fetch it for you |
| **a C++ compiler, GCC 13 or newer** | the tree-sitter grammars are C and C++, and see the floor below |
| **`pkg-config` and OpenSSL headers** | a build-time dependency of the optional ONNX annotation runtime |
| **git** | tree hashes are git tree hashes; everything binds to them |
| **network, during the build** | the same ONNX crate's build script fetches its runtime |
| **pytest** on `PATH` | only for `thruline demo`, which runs a real test and refuses to run without it; `pytest --version` should print a version, and `pipx install pytest` or `python3 -m pip install pytest` installs it |

On Debian or Ubuntu:

```sh
apt install build-essential pkg-config libssl-dev git
```

On macOS, `xcode-select --install`, and Homebrew's `pkg-config` and `openssl`.

### The toolchain floor, which is not a preference

**It does not build on Debian bookworm.** With every prerequisite above
installed, the build reaches the final link and stops:

```text
undefined reference to `std::__cxx11::basic_string<…>::_M_replace_cold(…)'
```

`_M_replace_cold` is a libstdc++ symbol GCC 13 introduced. Bookworm — the
default base of `rust:1.97.1-slim`, and one of the most common Linux CI images
there is — ships GCC 12.2, and the prebuilt ONNX Runtime that `ort-sys`
downloads was compiled against the newer one.

Measured 2026-09-05, same source, same commands:

| base | GCC | result |
|---|---|---|
| `rust:1.97.1-slim-bookworm` | 12.2.0 | **link fails at 7 min 18 s** |
| `rust:1.97.1-slim-trixie` | 14.2.0 | installs in **3 min 20 s** |

So: Debian trixie or newer, Ubuntu 24.04 or newer, or Fedora 38 or newer. On an
older base, `--no-default-features` drops the ONNX annotation backend and with
it this whole dependency — the ledger, verifiers, receipts and gate are
unaffected, and `annotate` and `eval` refuse with a message naming the cause.

### The two errors you will get if you skip the table

**Neither of the other two prerequisites is optional and neither failure is
legible.** Without a C++ compiler the build stops on
`failed to find tool "c++"`. Without `pkg-config` it stops on
`Could not find directory of OpenSSL installation` — several minutes in, from a
crate you did not name and cannot see in this repository's `Cargo.toml`. The
chain is `thruline-core` → `ort` → `ort-sys` (build script) → `ureq` →
`native-tls` → `openssl-sys`.

Those rows are at the top of this page because a fresh container built from an
earlier version of this page hit each of them in turn and got no further. The
container that tests this walk is built from this table, so a prerequisite that
is missing here fails the acceptance run rather than quietly working on the
maintainer's laptop.

The engine still makes no network calls at **run** time. This is a build-time
dependency of a crate that is compiled in whether or not the annotation model is
used, and it is stated here rather than discovered.

## Getting the binary

```sh
git clone https://github.com/thrulinehq/thruline.git
cd thruline
cargo install --path crates/cli --locked
```

That command puts the binary at `~/.cargo/bin/thruline` (Cargo's install
folder, `$CARGO_HOME/bin` if you have moved it). **If you have run `thruline
init`, that is the file your hooks run:** `init` writes the full path of the
binary that ran it into every hook it registers. Installing again with the same
command replaces that file, so the new build records your real sessions from the
next hook that fires. To try a build without replacing the one your hooks run,
see [Trying a new build beside the one you
use](#trying-a-new-build-beside-the-one-you-use).

The source is public under the Functional Source License, Version 1.1, MIT
Future License (FSL-1.1-MIT); the terms are in `LICENSE.md` at the root of that
repository. Each version converts to the MIT license on the second anniversary
of the date it is first made available.

*What would prove it:* a pushed tag, a signed artifact, and a checksum a script
can compare against before it puts a binary anywhere.

`cargo install --git`, a Homebrew tap, an installer script and an `npx` shim are
all built in `packaging/` and none of them is live: no tag has been pushed and
no artifact exists to check a hash against. When a release does exist, every one
of those paths verifies a checksum *before* it puts a binary anywhere, and none
of them resolves a floating "latest" — see [the threat model](./threat-model.md).

Expect the first build to take several minutes. It is a workspace with
tree-sitter grammars in it.

### Trying a new build beside the one you use

Give the new build a folder of its own with `--root`:

```sh
cargo install --path crates/cli --locked --root ~/thruline-try
~/thruline-try/bin/thruline --version
```

That leaves `~/.cargo/bin/thruline`, and every hook that runs it, as it was. Run
the test build by its full path, `~/thruline-try/bin/thruline`: a plain
`thruline` still finds the one your hooks run. It is a second program, not a
second record — it reads the same session records and keeps its own in the same
`~/.thruline` folder. If it needs those records in a newer format it says so and
stops; `thruline upgrade` run from it would change your real records, so leave
that step to the build you decide to keep. `cargo uninstall --root
~/thruline-try thruline-cli` removes it.

## Confirm it

```sh
thruline --version
thruline doctor
```

`doctor` reports whether the install is intact and whether the record on this
machine verifies. It exits 1 when something it checked is not intact, so it is
worth putting in a script.

## The licence

Everything is free except approving a version, which needs a Pro licence.
Opening projects, recording, reading, checking, verifying, exporting,
withdrawing an approval and stopping a run need no licence. Everything you
recorded stays yours.

```sh
thruline licence status
```

reports whether this machine has a licence that works, in one sentence. To
install a downloaded licence file, run `thruline licence install <file>`.

## Then

[The quickstart](./quickstart.md) takes you from here to a claim with a verdict
on it.
