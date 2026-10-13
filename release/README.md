# Releasing ThruLine from this machine (T-DIST-1, T-DIST-2)

Three scripts, run in this order by the founder. None of them publishes anything unless you pass `--publish` and type the version.

| Step | Command | What it does |
|---|---|---|
| 1 | `release/macos/release.sh <version>` | Builds, signs, notarizes and staples the macOS release into `target/release-dist/<version>/`. See `release/macos/README.md`. |
| 2 | `release/github-release.sh <version>` | **Dry run.** Checks the files against `sha256.sum` and prints the exact `git push`, `gh release create` and `gh release upload` commands, with each file and its SHA-256. It never calls `gh`. |
| 3 | `release/github-release.sh <version> --publish` | The same, then asks you to type the version, and only then runs the two `gh` commands. |
| 4 | `release/homebrew/render-formula.sh --macos-only <version> target/release-dist/<version>` | Fills Track C's formula template (`packaging/homebrew/Formula/thruline.rb`, never edited) into `target/release-dist/<version>/homebrew/thruline.rb`, checks it with `ruby -c`, and prints the tap commands. It runs none of them. |

The owner and repository come from `Cargo.toml`'s `repository` key, read when the script runs. The printed commands read them the same way (`REPO=$(sed … Cargo.toml)`), so the scripts' output never carries the owner's name.

`render-formula.sh` refuses to leave a placeholder. The template names four archives, two macOS and two Linux. `release.sh` makes only the macOS ones, so without `--macos-only` the script refuses and names the missing Linux targets. `--macos-only` leaves the Linux block out of the rendered formula, so Homebrew on Linux gets nothing from it.

The self-test for steps 2 to 4 is `release/release-paths-selftest.sh`. It needs no build and no network. It runs against a throwaway checkout with a made-up owner and a stand-in `gh`; the real `gh` is not on its `PATH`.

## Two release paths, and which one is canonical is not decided (D-6)

This folder is one release path: **built on this Mac, signed with a Developer ID, notarized by Apple and stapled.** A person who downloads it sees no warning that the developer cannot be verified.

Track C's `.github/workflows/release.yml` is a second path. cargo-dist builds on a tag push in GitHub's CI, and `sign-and-sbom.yml` adds Sigstore signatures and an SBOM. Its macOS archives are **not signed with an Apple Developer ID and not notarized**, so macOS treats a downloaded binary from it as coming from an unidentified developer. Both paths name their archives `thruline-cli-<target>.tar.gz` and write `sha256.sum`.

**Pushing the tag starts Track C's path.** `release.yml` runs on any pushed tag that looks like a version, and `git push origin v<version>` is the line step 2 prints first, because `gh release create --verify-tag` needs the tag on GitHub. So with both paths in the tree, publishing the signed release also starts a cargo-dist release of the same tag, and the two compete for one release and the same file names. Rule D-6 before the first tag is pushed.

Neither path is removed. Which one is canonical is founder decision **D-6** in `docs/loop/dist/HANDOFF.md`.
