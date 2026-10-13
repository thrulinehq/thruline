# Privacy

This page exists **before** any telemetry does, deliberately. The build spec's
own answer to "what is the telemetry retention policy" is *none until a
published privacy page exists*. So the page comes first, and it currently
describes a system that collects nothing — which is the easiest version of this
page to write honestly, and the right time to write it.

## What leaves your machine

Nothing, by default and by construction. The engine makes no network calls.

The only network use in the whole system is a model provider you configure
yourself: a local Ollama endpoint on a loopback address (`127.0.0.1`,
`localhost` or `::1`). Any other address is refused. It is off by default, it is
used only for annotation, and it is **never in the verdict path**. A model may
help notice that a sentence contains a claim; no model decides whether a claim
holds.

## Where your data lives

| path | what |
|---|---|
| `~/.thruline/exec.db` | the execution store: events, checks, verdicts |
| `~/.thruline/ledger.db` | the session ledger |
| `~/.thruline/spool/` | hook and shim lines awaiting `thruline fold` |
| `~/.thruline/signing.key` | your local signing key, generated on first use |
| `.thruline/` in a repository | policy, and receipts if you commit them |

All files you own. Deleting them deletes everything; nothing is kept elsewhere,
because there is no elsewhere.

Snapshotting writes loose git objects into your repository. `thruline doctor`
reports the footprint, and `git gc --prune=now` reclaims it.

## Redaction

Captured command output can contain credentials. Values matching credential
shapes are masked at capture, **before anything is written**, and the masking
runs ahead of any truncation — a secret must not survive because a buffer was
cut before the mask reached it.

Because redaction runs before hashing, every digest in the record is a digest
over redacted bytes. A digest is therefore never a digest of a credential, which
also means it is never an oracle for one.

`.thruline/redact.yml` extends the pattern list. `thruline doctor` reports
whether it loaded.

## What is in a receipt

A receipt carries argv **digests**, exit codes, output digests, git tree hashes,
and the verdicts computed from them. It does **not** carry file contents or
transcript text.

It does carry things you should look at before publishing one:

- the claim sentences the agent wrote, verbatim;
- promise text;
- file paths, inside findings;
- the repository path or remote, in `subject.name`.

Read one before you publish it — and read the receipt, not just its
verification. `thruline verify-receipt <path>` prints one line per check and a
conclusion — digests, ids and check names, and none of the four things above;
`--format md` adds the claim sentences and the reason behind each verdict. The
rest — promise text, the paths inside findings, `subject.name` — is in the
envelope's base64 `payload` field: `jq -r .payload <path> | base64 -d`.

## Telemetry

**Not built. Nothing is collected.**

There is no `telemetry` verb in this binary. The build spec describes an
opt-in one that would contribute aggregate counts only — claim
kind, verdict, harness, model — and never text, paths, or hashes of content.
None of that exists, which means there is no endpoint, no retention period and
no data.

If it is ever built, this page is where it gets described, and it will say so
before the first byte is sent — not after.

## The hosted parts

The GitHub App and the sync server are built in this repository and **not
operated by it**. Neither is deployed and nothing here contacts either.

`thruline sync` refuses rather than being absent: the hosted sync server does not
ship in v1, and the client stays a feature-flagged reference implementation. A
command that says why it will not run is more useful than one that is missing.

## A note on this page's own honesty

A test walks these chapters and fails if they tell a reader to run a command the
binary does not have. There is no exemption list — which is why "telemetry" is
described above without a `thruline` prefix in front of it. A command that does
not exist does not get to appear as one, not even in a sentence explaining that
it does not exist.
