# Security policy

## Reporting

**Do not open a public issue for a security report.**

> **Contact: unset.** This is an operator decision and is deliberately not
> invented here. Until it is set, a reporter has no private channel, and that is
> a gap rather than an omission — see *What this policy cannot promise yet*.

Include: what you did, what happened, what you expected, and the commit you
tested. A receipt, a policy file, or a session that reproduces it is worth more
than a description.

## What a vulnerability is here

This project's product is a verdict about someone else's work. So the security
boundary is not the usual one, and the most valuable report is not a crash.

**In scope, most valuable first:**

1. **A receipt that verifies when it should not.** Any input that makes
   `thruline verify-receipt` conclude `verified` — or a check run conclude
   `success` — over a receipt that does not deserve it. This is the whole
   product failing, and it includes the vacuous cases: an empty array that
   satisfies a rule by containing nothing to violate it.
2. **A claim that reaches `supported` without the evidence.** A crafted session,
   claim table, policy, or repository layout that promotes a verdict it should
   not — or that suppresses a finding a detector should have emitted.
3. **A receipt that fails when it should not.** A false accusation is a real
   defect here. This project's own record says the most expensive error it can
   make is accusing honest work.
4. **Capture that can be silently disabled.** A way to remove hooks, bypass the
   `PATH` shim, or degrade capture *without* the session being marked
   `capture_degraded`. Silence about a gap is the failure mode; a loud refusal
   is not.
5. **Secrets surviving redaction** into a stored event, a digest, a receipt, or
   any `--json` output.

   **Where redaction actually runs, because this entry used to overstate it.**
   It said "Redaction runs before hashing" without qualification. That is true
   of exactly one of the three write paths, and stating it of all of them made
   this file claim a property the store does not have:

   - **Capture (BUILD-SPEC §4.6) — redacted before it reaches disk.** A hook
     delivery taken by `thruline hook <harness> --spool` goes through
     `crates/capture/src/redaction.rs` before the spool line is written, and the
     ordering is a type consequence rather than a convention: `Bounded` is
     constructible only from `Redacted`, so unredacted text cannot reach a
     truncation. `thruline fold` carries that redacted payload into `exec.db`.
     What is *not* masked on that line is named rather than glossed: the
     spine fields lifted straight out of the delivery (`session_id`, `cwd`,
     `transcript_path`), and `raw_sha256`, which is a digest of the original
     bytes — a fingerprint, not content.
   - **The sensor pack — stored verbatim, not redacted.** `thruline sensor
     append <Event>`, which is what `thruline init` installs, writes the hook
     payload byte-for-byte into `sensor_events`. `crates/core/src/sensor.rs`
     contains no call to the masker, by design: rule 2 of that module is
     "verbatim or nothing".
   - **`thruline ingest` — the raw transcript line is preserved.** The
     `events` table in `ledger.db` holds what the harness wrote, per the
     raw-line-preservation invariant in `CLAUDE.md`. There is no masker in the
     ingest or parse path.

   So for those last two, redaction runs **on the way out, not on the way in**:
   at derivation and at render/export time, through the call sites enumerated
   in `redact::MASK_CALL_SITES` (derivation, capsules, the digest, `explain`,
   the `thruline ui` JSON API, `--json` output). A secret that reaches
   `~/.thruline` is on disk in the clear, and what redaction protects is every
   artifact derived from it.

   **The trade-off, stated rather than implied.** Redacting on the sensor and
   ingest write paths would cost the raw-line-preservation invariant — the
   thing that lets `thruline rebuild` reproduce `nodes`/`edges`
   byte-identically and lets every UI claim trace back to a file and line. It
   is an architectural decision that has not been taken, not an oversight, and
   it is not fixed by a patch to the masker.

   A leak still matters at both ends. Into an artifact — a digest, a receipt,
   a `--json` payload — it is a leak into something hashed, signed and meant to
   be shared. Into the store it is a leak into a file that stays on this
   machine, which is why the home directory and its databases are created
   owner-only (`crates/core/src/private_store.rs`); installs that predate that
   code have world-readable files and need one `chmod` to repair.
6. **Signature, canonicalization or DSSE handling** that accepts a forged or
   replayed envelope — including a receipt replayed onto a different tree or
   commit.
7. **Anything the engine does over a network.** The engine makes no network
   calls. A path that does is a defect regardless of what it sends.

**Out of scope**, because the threat model already says so in
`docs/book/src/threat-model.md` under *What is not defended*:

- An agent that edits the ledger. Tampering is **detectable, not impossible** —
  the chain is hashed and a receipt over a broken chain fails. A report that the
  ledger is writable is not a finding; a report that tampering is *undetectable*
  is.
- A check that is itself wrong. ThruLine confirms a check ran and what it
  reported, never that it tests the right thing.
- Imported harness logs. A rollout is the harness's own account; imported
  sessions are `capture_degraded` and no claim in one can reach `supported`.
- Missing tampering detectors for a language that has none. Their absence is not
  evidence, and the docs say so.
- A compromised machine. ThruLine records what happened on it; it cannot detect
  a kernel that lies.

## What this policy cannot promise yet

Stated rather than implied, because a security policy that overstates its own
readiness is the first thing a researcher catches:

- **No private reporting channel is configured.** See above.
- **No release channel exists.** Nothing is published, so there is no artifact
  to patch and ship. A fix would land on a branch, and a reporter should expect
  to be told that rather than given a version number.
- **No CI has ever run on this repository.** Every green figure comes from
  developer machines. A fix's verification will say which machine it ran on.
- **No bug bounty**, and none is implied by this file.

## Coordinated disclosure

Please give a reasonable window before publishing, and say up front if you have
a deadline. Since there is no release channel, "fixed" will mean a named commit
on a branch, and that commit will be cited to you.

## Test keys

Keys under tests/ and fixtures/ are throwaway test keys. They protect nothing.

## Licence

ThruLine is licensed under the Functional Source License, Version 1.1, MIT
Future License (FSL-1.1-MIT); see LICENSE.md. Reporting a vulnerability does
not assign copyright and does not require a CLA.
