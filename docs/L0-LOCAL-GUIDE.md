# Local advisory workflow

ThruLine checks the literal commands you agree to, keeps missing checks visible,
and explains what evidence applies to the current source. It does not certify
arbitrary natural-language goals or correctness beyond that declared scope.

## Install and prerequisites

The local candidate archive contains the `thruline` binary and embedded UI.
The local macOS arm64 candidate passed its installer and actual Python/TypeScript
command journeys, plus an actual Codex CLI 0.146.0 session followed by explicit
accepted-command capture and advisory rollout import. L0 is qualified for that
local scope; native hooks, other architectures and package managers require their own qualification. No published download is implied by
this document. For a local archive, verify its provided SHA-256, extract it, and
install the binary into a directory you own on PATH. `packaging/install.sh` is
also qualified against a local file mirror with `THRULINE_PREFIX`,
`THRULINE_VERSION` and `THRULINE_BASE_URL` explicitly supplied. The local test does
not authenticate a public release or a remote mirror.

Prerequisites: Git and the project's actual test/build tools. Python examples
need Python with pytest; npm examples need Node, npm and their declared project
dependencies already installed. ThruLine does not install those dependencies or
buy a model account. Advisory captured checks require Unix process groups on
this build. Docker is optional for the separate existing re-execution path;
isolated accepted-check execution remains unimplemented and cannot satisfy an
accepted requirement through that command.

## This prepared candidate

Implementation checkpoint: `af1dee24889a1e11f77aae6160585f86f1da053d`.
The local bundle is `target-l0-release/` in this checkout (intentionally ignored
by Git). It contains the executable, actual installer-tested archive, checksum,
qualification manifest and logs. The archive is unpublished and not notarized.

```sh
cd /Users/dev/thruline/target-l0-release
shasum -a 256 -c sha256.sum
./thruline --version
```

Binary SHA-256:
`adbf05100c669b1ffa1639356b66e0c3aa706d7f62bd2a5296458169a93d6c31`.
Use that explicit executable path in place of `thruline` below to avoid an older
binary already on PATH. The saved `docs/L0-EXAMPLE.html` needs no server or account;
open it locally and select Current pass, Checks failed, Stale evidence or Not run.
The sample proves the presentation, not readiness for a real project.

## Agree, check, inspect

From a Python or npm Git project:

```sh
thruline acceptance init
thruline acceptance status
```

Review `.thruline/acceptance.json`: objective, criteria, literal argv, working
directory, runner, protected paths, timeout and artifacts. A generated suggestion
is a draft. Discovery includes conventional test directories and common root test
filenames; add nested colocated tests and project-specific configuration explicitly.
Accept the exact digest printed by status:

```sh
thruline acceptance accept --digest 'sha256:REVIEWED_DIGEST' --by 'YOUR_LOCAL_LABEL'
thruline acceptance run --check 'CHECK_ID' --digest 'sha256:REVIEWED_DIGEST'
thruline ui --acceptance /absolute/path/to/project
```

Use `--repo` and `--exec-db` on acceptance commands when using explicit paths;
use the same `--exec-db` with `ui`. Keep evidence outside the checked repository.
The default execution store is under the ThruLine home, separate from the legacy
transcript ledger. The UI copies complete commands with its configured paths.
Copying a command does not execute it; Refresh only rereads evidence.

`run` executes one attempt. Exit 0 means all declared requirements are satisfied
at the observed local boundary, 1 means a requirement failed, and 2 means some
required evidence is unverified. `status` is advisory inspection: its successful
process exit is not a completion verdict. Read the explicit result state.

Source, lockfile, protected test, command, executable or observed environment
changes can invalidate current use. Counts from an old run remain historical.
Skipping every test or collecting no tests does not prove a test criterion.
Literal filtering narrows the command's scope; it never certifies an entire
suite beyond that declaration. Human-acceptance criteria remain open until a
separately supported human acceptance path is available; a machine run does not
stand in for human review.

Artifact checks currently require the report to be absent before the producing
run. Use an ignored output path so generating a report does not change the input
tree. A preexisting report remains insufficient even if the command exits zero.

## Change requirements and carry work forward

Edit the draft only after deciding the revised scope. Increase its revision,
inspect the new digest and explicitly supersede the previous acceptance hash:

```sh
thruline acceptance accept --digest 'sha256:NEW_DIGEST' --supersedes 'PREVIOUS_ACCEPTANCE_HASH'
thruline acceptance handoff --json > /outside/project/handoff.json
thruline acceptance resume --packet /outside/project/handoff.json --packet-digest 'sha256:PRINTED_PACKET_DIGEST' --by 'RECEIVING_LOCAL_LABEL'
```

A changed or removed draft does not erase previously accepted work. A resumed
packet must match the workspace, branch/commit, accepted requirements and
current evidence. A packet export alone never means recovery was confirmed.

## Share and diagnose

```sh
thruline acceptance receipt --json
thruline verify-receipt /outside/project/receipt.json --key /outside/project/signing.pub --repo /absolute/project --json
thruline --version
thruline acceptance status --json
```

The receipt command prints its detached file and public-key paths. Share the
receipt and public key only after reviewing the declared command metadata. Keep
the private key, output store and raw transcript local. Receipts omit raw test
streams and free-form objective/criterion text. A valid signature alone does not
establish independent trust. Read `RECEIPT-v0.2.md` for the two predicate kinds.

For a support report, provide the binary SHA-256, OS/architecture, runner versions,
command's exit code and the v0.2 receipt or its verification result after review.
These constitute the minimal diagnostic export. Ordinary `acceptance status`
contains local descriptions and paths; inspect it locally and do not share it
blindly. An unreadable contract or store is an explicit error, not an empty result.

## Upgrade and uninstall

Preserve the execution database, its adjacent `.acceptance` directory (keys and
captured output), and detached receipts before changing versions. Replace only
the executable you installed. Read-only commands refuse unsupported newer schema
versions. An explicit accepted-write operation can migrate the small execution
store; it does not migrate the large transcript ledger. Legacy transcript
migration remains the separate `thruline upgrade` workflow.

To uninstall this local route, stop the foreground UI and remove only the
`thruline` binary you installed. Evidence is retained. If you explicitly installed
capture hooks, run `thruline hook-install --undo` with the same harness, scope and
settings path before removing the binary. Do not remove an entire settings file,
user home, shared package cache or Docker data. Delete retained evidence only if
you intentionally no longer want that history.

## Reproduce qualification

The qualification script needs Python `jsonschema`, Git, pytest, and an existing
Vitest dependency directory. These are qualification prerequisites, not hidden
runtime dependencies of ThruLine. It copies cached JavaScript dependencies and
installs only into a temporary home. No model or account is called by this script.

```sh
python3 scripts/qualify-l0-local.py \
  --binary /Users/dev/thruline/target-l0-release/thruline \
  --vitest-modules /absolute/path/to/existing/node_modules
```

A missing dependency fails the run. The script distinguishes generated-command
fixtures from real-agent qualification. Read `L0-QUALIFICATION.json` for actual
versions, failures repaired, remaining ignored tests, resource measurements and
unqualified combinations. A Docker test needs its explicit image variable; an
unset opt-in variable is a failed or unperformed check, never a pass.

For tests against the exact packaged executable instead of Cargo's debug binary:

```sh
THRULINE_QUALIFIED_BINARY=/Users/dev/thruline/target-l0-release/thruline \
THRULINE_QUALIFIED_BINARY_SHA256=adbf05100c669b1ffa1639356b66e0c3aa706d7f62bd2a5296458169a93d6c31 \
CARGO_TARGET_DIR=/private/tmp/tl-148 \
cargo test -p thruline-cli --no-default-features --offline \
  --test l0_acceptance --test m2_action
```

The optional selector refuses a missing or wrong hash. Docker suites also require
`THRULINE_TEST_PYTEST_IMAGE=thruline-acceptance-pytest:local` (or another explicitly
prepared image); the image must already contain pytest. The manifest identifies
the exact image used for this candidate.
