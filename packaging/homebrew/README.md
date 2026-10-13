# The Homebrew tap

The planned install command is `brew install thrulinehq/thruline/thruline`.

The public release archives live at `thrulinehq/thruline`, which also holds the
source under FSL-1.1-MIT. The `thrulinehq/homebrew-thruline` tap does not
exist yet and is a launch prerequisite.

## Tap pull request

A Homebrew tap is a separate repository named `homebrew-<tap>`. It reads
`Formula/*.rb`. The formula here is a template; the helper fills its version
and checksums from a published binary release. After the operator publishes
that release and creates the public tap repository, open a formula PR:

```sh
python3 packaging/public-release/open_tap_pr.py --tag v0.2.0 --output /tmp/thruline.rb
python3 packaging/public-release/open_tap_pr.py --tag v0.2.0 --create-pr
```

The second command pushes only `Formula/thruline.rb` to a tap branch and opens
a PR. Leave the PR unmerged; the operator reviews and merges it personally.
The tap repository does not exist yet, so neither command has been run.

## The placeholders are load-bearing

`version` and the four `sha256` values are `PLACEHOLDER_*` on purpose. A
formula whose checksums are wrong installs nothing and says so; a formula whose
checksums are *absent* is the one shape that must never reach a tap, because
Homebrew will happily install whatever the URL returns. The ALL-CAPS values
cannot be mistaken for real hashes and cannot be committed by accident without
someone noticing.

The helper fills them from the public release's `SHA256SUMS`, which the
installer also checks against the downloaded archive. To inspect that file:

```sh
gh release download "v$VERSION" --repo thrulinehq/thruline -p SHA256SUMS -O -
```

## Why the Linux bottles are musl

Homebrew on Linux runs against its own glibc, so a `-gnu` build would work
there and fail everywhere a user copies the binary to. One Linux artifact that
runs anywhere beats two that each run somewhere. The measured reason is in
`../install.sh`.

## `brew test` is not decoration

`brew test` runs the `test do` block, and it asserts the property that matters
after an install — the binary runs, **and its refusal path works**: a policy it
cannot parse is refused rather than passed. A formula whose test only checks
that a file landed has tested the download.
