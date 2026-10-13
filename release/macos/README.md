# The macOS release: signed, notarized, stapled (T-DIST-1)

One command, from a clean checkout at tag `v<version>`:

```
release/macos/release.sh <version>
```

It leaves everything in `target/release-dist/<version>/` (git ignores `target/`), and changes nothing else in the tree.

**Status: prepared, never run for real.** The signing identity and the notary profile it needs do not exist yet (`docs/loop/dist/BLOCKED.md` B-1). Until they do, the script stops at the preflight, exits 3, and prints the steps in `founder-steps.txt`.

On a real run every Apple tool is called by its absolute path: `/usr/bin/security`, `/usr/bin/xcrun`, `/usr/bin/codesign`, `/usr/bin/hdiutil`, `/usr/sbin/spctl`. A program of the same name earlier on `PATH` is never used. Only self-test mode calls them by name.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Done: a stapled dmg, the Homebrew archive, `sha256.sum`, and every tool's output in `evidence/`. |
| 1 | Failed: a dirty tree, HEAD not at the tag, too little disk, a build, check, signing, packaging or notarization failure. |
| 3 | **Blocked on the founder**, not failed: there is no Developer ID Application identity, or the `thruline-notary` profile is missing. The script prints the exact steps to unblock it. |

## What it does, step by step

Each step prints one line about what it did.

1. **Preflight.** It refuses before doing anything if:
   - the tree has changes or untracked files;
   - `Cargo.toml`'s workspace version is not `<version>`, there is no tag `v<version>` (the refusal prints the command: `git tag v<version>`), or HEAD is not that tag's commit;
   - less than 8 GiB is free;
   - there is no `Developer ID Application:` identity (`security find-identity -v -p codesigning`). An **Apple Development** identity is refused with the reason: Apple will not notarize it for download;
   - `xcrun notarytool history --keychain-profile thruline-notary` fails.

   It prints the chosen identity's SHA-1 and never its name.
2. **Build.** It runs `cargo build --release --locked -p thruline-cli --no-default-features` for the host architecture. `--no-default-features` is the shipped feature set: the Homebrew formula's test block says so ("The shipped binary is `--no-default-features`"), and `dist-workspace.toml` sets `default-features = false`. The README's default build carries the ORT backend, which RELEASING.md says "ships nowhere" yet. `--universal` builds both Apple targets and joins them with `lipo`, but only when both Rust targets are already installed. The script never installs one.
3. **Release-binary checks** (`check_release_binary` in `lib.sh`; its decision table is `check-release-binary-cases.sh`). Each check below **refuses** the build when it fails, and so does anything the check cannot read (a probe that exits non-zero, prints nothing or prints something that is not JSON, or a status without a boolean `paid_gate`, a string-or-null `technical.licence_id` and an integer `technical.keys_built_in`).
   - `--version` runs.
   - **No debug-only seam's name is in the binary.** The seams are the four names the licence code reads, all `cfg(debug_assertions)` in `crates/cli/src/licence_cmd.rs`: `THRULINE_LICENCE_DEV_PUBLIC_KEY` (trust a test key), `THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS` (trust none of the built-in keys), `THRULINE_PAID_GATE_DEV` (turn the gate off) and `THRULINE_LICENCE_DEV_AFTER_RENAME` (pause `licence install`). The list is `RELEASE_SEAM_VARS`; case z of the decision table fails if the source reads a `THRULINE_*` name the list does not have.
   - **The paid gate is on, and a production key is built in.** In a fresh home, `licence status --json` must say `paid_gate: true` and `keys_built_in` at least 1. A build with the gate off, or with no key (whose gate is forced off), is refused. Until card RELEASE-CHECK-GATE-ON it passed with a warning; that pass encoded the defect (D-050, ruled in D-244).
   - **The as-shipped start state: "Free, no licence"** (v0.2, C20: there is no trial). That fresh home answers `thruline.licence.status` schema 4 with licence `absent`, headline `free` and the sentence "Free. Approving a version needs a Pro licence.", and has no `trial` field. **Approving a version is refused without Pro:** `entitlements.approve_version` is `allowed: false`, status `absent`, its reason starting "Approving a version needs a Pro licence." (the approve route answers `403 licence_required` with this same entitlement). **Everything else works without a licence:** `paid` lists exactly one surface, `approve_version`, sold as `pro` only. Reading the status writes no `trial.json`.
   - **A v0.1.0 trial file opens nothing.** A home holding only a `trial.json` in the shape v0.1.0 wrote, dated today, still answers all of the above, and the file is left byte for byte as it was.
   - **The production key is the one trusted, by id.** A licence naming key id `3ef3540d43dbf105`, signed by a test key, must be answered `tampered` (the binary has a key with that id and the signature does not match it), not `unknown_key` (no key with that id).
   - **No seam acts.** A licence signed by a key made on the spot with `openssl` (Ed25519), in a home that also holds a v0.1.0 `trial.json` dated today, is asked about with no seam set, with each seam set to the value that makes it act, and with all four set: the licence is never accepted, the gate stays on, the key count stays what the fresh home said, and `approve_version` stays denied.
   - With a debug binary given (the self-test passes one if it is built), positive controls first show that where the seams exist they do act: the test licence is accepted, the gate goes off, the keys go to 0, and the key-id licence is answered `tampered`.
4. **Codesign.** It runs `codesign --force --timestamp --options runtime --sign <SHA-1>`, then `codesign --verify --strict --verbose=2`, then checks `codesign -dvvv` for the `runtime` flag, a secure timestamp and the chain Developer ID Application → Developer ID Certification Authority → Apple Root CA. **No entitlements file:** the shipped binary links only system libraries, uses no JIT and loads no plugin. The self-test runs the hardened binary to check that it works.
5. **Package.**
   - `thruline-<version>-macos-<arch>.dmg` holds `thruline`, `LICENSE.md`, `THIRD_PARTY_NOTICES.md` and `INSTALL.txt`, and the dmg is signed too. `INSTALL.txt` gives the one line to paste into Terminal (`sudo mkdir -p /usr/local/bin && sudo cp /Volumes/ThruLine/thruline /usr/local/bin/thruline`), says it asks for the Mac's password and why, and ends on `thruline ui`.
   - `thruline-cli-<target>.tar.gz` holds one directory with the binary inside, the name and layout `packaging/homebrew/Formula/thruline.rb` fetches.
   - Each file gets a `.sha256`, and both go in `sha256.sum`.
6. **Notarize.** It runs `xcrun notarytool submit <dmg> --keychain-profile thruline-notary --wait`. On anything but `Accepted`, it saves `notarytool log` to `evidence/notary-log.json` and exits 1.
7. **Staple and check.** It runs `xcrun stapler staple`, `xcrun stapler validate`, then `spctl --assess --type open --context context:primary-signature --verbose=4` on the dmg. Each output goes to `evidence/`. The checksums are re-taken after stapling.
8. **Your second-account check**, printed as a numbered list.

The dmg's notarization covers the binary inside it. A bare command-line binary cannot carry a stapled ticket, so when it is first run from the tarball, Gatekeeper looks the ticket up online.

## Keychain names (from `docs/loop/dist/SECRETS.md`)

| What | Name |
|---|---|
| Code-signing identity | the first valid identity whose name starts `Developer ID Application:`. To pick another, set `THRULINE_CODESIGN_IDENTITY` to its SHA-1 from `security find-identity -v -p codesigning`. |
| Notary credentials | the `notarytool` keychain profile **`thruline-notary`** |

The script only reads these. It never creates, imports or deletes a keychain item.

## The founder's sequence

It is in `founder-steps.txt`, the one source for it: `release.sh` prints that file when it exits 3, and it carries `docs/loop/dist/BLOCKED.md` B-1 (the certificate by Xcode or on the web, where the Team ID is, the notary profile, `git tag v<version>` and the one command, and the second-account check). The self-test fails if a fenced command in B-1 is missing from it, or if it prints a command B-1 does not have.

## The self-test (what the loop can run)

```
release/macos/release-selftest.sh
```

It runs every step that does not need your credentials, through the same functions (`lib.sh`) the release runs:

- It runs `release.sh` itself in self-test mode against a throwaway checkout. Stand-ins for `security` and `xcrun` answer only the two read-only questions and refuse anything else. This covers every preflight refusal and its exit code: no tag (and that the refusal names `git tag v<version>`), a dirty tree, HEAD not at the tag, low disk, no identity, Apple Development only, an override naming an Apple Development identity, and no profile. Each exit 3 is checked against B-1 in both directions. It also checks that the refusals are not vacuous: with everything present, the preflight passes.
- It runs `release.sh` once as a **real** run, with stand-ins for all five Apple tools first on `PATH`, and checks that none is called: the real `/usr/bin/security` answers (read-only) and the run stops at exit 3, before any build.
- It runs the one release build, and the release-binary checks. As positive controls, a debug binary, if one is already built, **accepts** the same test licence, and its seams turn the gate off and remove the keys.
- It signs **ad-hoc** (`--sign -`) with the hardened runtime and `--timestamp=none`, because asking for a secure timestamp contacts Apple. Then it verifies the signature and runs the hardened binary.
- It makes the dmg and the archive, mounts the dmg and checks its three files and `INSTALL.txt`'s words, and checks `sha256.sum`.
- It runs the notarize step's reject and accept paths against the stand-in `xcrun`.
- It checks that `stapler validate` **fails** on the un-notarized dmg, as it should.
- It checks that the tree's `git status` did not change.

It never uses a real identity. An Apple Development one would put a keychain dialog on screen. It never talks to Apple and never writes to a keychain. Self-test mode (`THRULINE_RELEASE_SELFTEST=1`) is the only mode that honours the two seams (`THRULINE_RELEASE_ROOT`, `THRULINE_RELEASE_FREE_GIB`), and it always stops after the preflight.

## Decision D-5: `.dmg` or `.pkg`

This script makes a **`.dmg`**, which needs only the Developer ID Application certificate. The person pastes one line into Terminal to copy `thruline` into `/usr/local/bin`, as `INSTALL.txt` says, and it asks for the Mac's password.

A **`.pkg`** would install `thruline` straight into `/usr/local/bin`, so it is on `PATH` with no copying. It needs a second certificate, **Developer ID Installer**, made the same way as step 2, and a `productbuild --sign` step. Which to ship is the founder's call, recorded as D-5 in `docs/loop/dist/HANDOFF.md`.
