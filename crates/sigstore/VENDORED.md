# Vendored snapshot — do not edit in place

This crate is a **pinned copy** of `~/thruline-sigstore` (Track B). It is not
where B works, and an edit made here is lost at the next re-vendoring unless
the approved local overrides below are retained.

    source repository   ~/thruline-sigstore  (a separate repository; its
                        commits are not objects in this one)
    source commit       196d512
    travelled-tree      sha256:e4806b6e7e628e73c7ed7ba8a15a1f7ed6ad39b895e419b32290b6155967c043
    files travelled     119
    vendored by         the core, 2026-09-06, DECISIONS #78/#88/#130

## Why a snapshot and not a merge

`~/thruline-sigstore` is a **separate repository**, so B's commits are not
objects here and `git merge` cannot reach them — the core measured that when it
tried. B's own plan §2 left the form to the core: *"a path dependency, not a
workspace member, until this crate has a gate recorded in the core's own tree …
Vendoring or publishing remain the exits from that, and both are the core's
call."* The register made that call: vendored, digest-pinned.

## How the digest is computed, so it can be checked by hand

For every travelled file except this one, in `LC_ALL=C` sorted path order,
a line `<sha256>  <path>`; then the sha256 of that manifest. Build output
(`target/`, `__pycache__/`) is not travelled and is not hashed.

```sh
cd crates/sigstore
find . -type f ! -name VENDORED.md ! -path '*/__pycache__/*' | sed 's|^\./||' | LC_ALL=C sort \
  | while read -r f; do printf '%s  %s\n' "$(shasum -a 256 "$f" | cut -d' ' -f1)" "$f"; done \
  | shasum -a 256
```

`crates/cli/tests/sigstore_snapshot_is_pinned.rs` recomputes it on every gate,
so this file cannot drift from the tree beside it. It sits in the core's own
lane rather than in `crates/sigstore/tests/`, because a core-authored file
inside this tree would be inside its own digest and would break the claim that
the tree is byte-identical to B's. A second check there, `every_pinned_file_is_
tracked_by_git`, asserts that everything inside the digest is also in the
commit — the pin's first recording failed exactly that way.

## What travelled, and what did not

B's travel list (`MERGE-PLAN.md`, *"One correction to the ownership
instruction"*): `Cargo.toml`, `src/`, `tests/`, `fixtures/`, `examples/`,
`scripts/`, `README.md`, `docs/DECISION.md`.

`examples/` is on the list for a measured reason rather than by habit:
`scripts/fresh-machine-verify.sh` stages it, and the two cross-machine proofs
run `examples/verify-bundle.rs` and `examples/sign-local.rs`.

`docs/DECISION.md` is on it too, and that is why the entry is **`196d512`** and
not `12c8f64`. Between them B corrected an overstated claim in that file — the
`sha2` "two generations in one graph" argument, disproved by the core's own
lockfile. `src/`, `tests/`, `fixtures/`, `examples/` and `Cargo.toml` are
**byte-identical** at the two commits (verified: `git diff 12c8f64..196d512 --
Cargo.toml src tests` is empty, and the `src` and `tests` tree objects match),
so the code is the same either way and only the rationale differs. A snapshot at
`12c8f64` would have vendored the uncorrected one.

**First exclusion: `tests/core_contract.rs`.** It asserts B's claims about the
*core* against a real core checkout — line numbers, constants, guard names. It
belongs where B can run it against a moving core, and inside the core it would
be a test asserting things about the tree it lives in, which is a different and
much weaker statement. B keeps it.

**Second exclusion: `fixtures/cosign-keys/oracle/__pycache__/`,** two committed
`.pyc` files. B tracks them; this repository cannot. `.gitignore:13` is
`__pycache__/`, so `git add` silently skipped them — the first pin was computed
over a 121-file worktree while the commit held 119, and a fresh clone would
have failed the count. They are also not reproducible: running the fixture
oracle rewrites the bytecode, and the gate that ran it changed both files and
broke the digest. That is the failure that found this, and it is the failure
the pin exists to make. Python regenerates them on demand, so nothing is lost.

The walk skips `__pycache__` rather than merely deleting the files once, because
deletion alone would let the next oracle run recreate them and redden the gate
on a tree nobody had edited.

## The scrypt profile the workspace root carries for this crate

`crates/sigstore/Cargo.toml:77-81` sets `opt-level = 3` for `scrypt` under both
`dev` and `test`. Cargo **ignores profiles in a non-root workspace member** and
says so on every build: *"profiles for the non root package will be ignored"*.
The root `Cargo.toml:75-79` therefore carries the same two stanzas verbatim.

**The reasoning, and the measurement, live at that root stanza** — see the
comment above it: the cost is roughly 96 seconds on the workspace suite, and
the resulting cargo warning is accepted deliberately, because stripping the
manifest would mean the vendored bytes are no longer B's. This entry is a
pointer, not a second copy; the number belongs in one place.

## One thing in B's fixtures the core noticed while vendoring

Not a defect and not altered — a snapshot that edits what it pins is not a
snapshot — but recorded so nobody discovers it later and wonders:

- `fixtures/cosign-keys/cosign-test.key`, a **test** signing key, committed
  deliberately as a parser fixture. It is not a credential for anything, and it
  is exactly the shape `attest::verify`'s `signing_identity` check refuses to
  treat as an identity (F12): a key committed in a repository proves nothing
  about who signed.

## Changing this crate

Re-vendor from a named B commit and update the source commit, digest and file
count above in the same change. Do not edit the files in place except for the
approved local overrides documented below.

## Licence

This crate inherits the workspace licence, FSL-1.1-MIT (see `LICENSE.md` at the
repository root), through `license.workspace = true` in its `Cargo.toml`, and
its package cannot be published (`publish = false`). That is the one local
metadata choice made here. Future re-vendoring must keep it and repin. The
original 196d512 snapshot digest was
`f67f7896e8a582322c27a2e0b854be3aec1a20c57450b6e20d4a7cf75c904735`.

## Approved exact key modes override — C50/C51, 2026-10-09

C50 §3.3 approved setting key-file modes explicitly, and C51 §1.4 approved
repinning that intended local change. Core commit `530043b7` changes only
`src/key.rs` inside the travelled tree: after exclusive creation, `generate`
sets the private and public descriptors to `0600` and `0644` before writing
either file. This makes their modes independent of the caller's umask without
changing the process-global umask. Private creation remains restricted, and
permission errors use the existing paired-file cleanup.

The named Track B commit remains `196d512`; its repository was not changed.
Tests, fixtures and the 119-file travel list are unchanged. Replacing only
`src/key.rs` with its pre-C50 bytes reproduces the prior pin
`c7b1d4efd51c1081550c4ef8ec0a72d34195655b0286c79b3584ad0846127286`.
The travelled-tree digest above includes this approved source patch and the
metadata override. Future re-vendoring must retain both or explicitly resolve
them against the new upstream source.
