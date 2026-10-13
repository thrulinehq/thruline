# ThruLine — Performance Baseline

**This file does not tell you whether a number is current.** It cannot: currency
depends on the corpus, the derived state and the code on the machine reading it,
and this file has no access to any of those. It records **what was measured, and
under exactly which conditions**. To find out whether a figure still holds where
you are:

```bash
cargo build --release && ./target/release/thruline perf
```

That command measures the conditions now and compares them, axis by axis,
against the basis each figure was taken under. A figure whose basis has moved
prints **"stale basis — re-measure"** *instead of its numbers* — not beside
them, because a number printed next to a warning is a number that gets quoted
anyway.

The registry behind it is [`perf-basis.toml`](perf-basis.toml); the machinery is
`crates/core/src/perf_basis.rs`, and `crates/core/tests/a192_perf_basis.rs` is
the contract in executable form.

## Why the file is shaped this way (P2b)

Three sessions in a row were sent chasing a cost that had been mis-attributed,
and each one reasoned from a number whose measuring conditions were no longer
written down:

- **CLAUDE.md** asserted a twelve-minute cold export for three days after A158
  had made it untrue.
- **This file** held a 44-project corpus size and an inverted attribution while
  the real corpus reached 90.
- **A190** timed a query in the `sqlite3` CLI, as an aggregate over a subquery
  against a page cache the previous run had warmed, and reported "~15 %" for
  something A191 measured at 76 %.

The E2 ruling that came out of it: *measurements age like proofs.* Every figure
carries a stamped basis; currency is derived at read; a diverged figure is
unquotable.

## The figures

Generated from [`perf-basis.toml`](perf-basis.toml) — do not hand-edit. Regenerate
with `thruline perf --doc-block`.

<!-- BEGIN perf-figures (generated from docs/perf-basis.toml) -->
| measurement | as recorded | basis | comparable against today? |
|---|---|---|---|
| now (single project) | 21 ms / 21 ms vs 100 ms | `B-A196` | yes — `thruline perf` derives currency here |
| now --all | 35 ms / 38 ms vs 100 ms | `B-A196` | yes — `thruline perf` derives currency here |
| packet (single project) | 24 ms / 26 ms vs 150 ms | `B-A196` | yes — `thruline perf` derives currency here |
| packet (repo-scoped) | 34 ms / 44 ms vs 150 ms | `B-A196` | yes — `thruline perf` derives currency here |
| projects (full aggregation, NOT a bare open) | 8545 ms / 9534 ms — Roughly double B-A195's 4,453/4,678 ms on the same event count. Not diagnosed here: the ledger grew 164 MB of A196 index pages and one project/session row, and the volume sat at 97 % full during the run. An attribution row moved and the movement is recorded, not explained away. | `B-A196` | yes — `thruline perf` derives currency here |
| verify (whole ledger) | 205.3 s — One run, no discarded warm-up, under a 900 s ceiling it did not hit. Not a warm number. The first COMPLETED whole-ledger verify this registry has ever held — every prior row is a bound or a 44-project corpus. A196's generation_id indexes are the difference; Q-A189.2's premise has moved accordingly. | `B-A196` | yes — `thruline perf` derives currency here |
| peak RSS (packet, largest project) | 15 MiB against a 1024 MiB full-process budget. One run. | `B-A196` | yes — `thruline perf` derives currency here |
| now (single project, largest) | 23 ms / 24 ms vs 100 ms | `B-A195` | yes — `thruline perf` derives currency here |
| now --all | 36 ms / 37 ms vs 100 ms | `B-A195` | yes — `thruline perf` derives currency here |
| packet (single project, largest) | 26 ms / 27 ms vs 150 ms | `B-A195` | yes — `thruline perf` derives currency here |
| packet (repo-scoped) | 35 ms / 48 ms vs 150 ms | `B-A195` | yes — `thruline perf` derives currency here |
| projects (full aggregation, NOT a bare open) | 4453 ms / 4678 ms | `B-A195` | yes — `thruline perf` derives currency here |
| verify (whole ledger) | Exceeded the suite's 900 s ceiling on one run and was aborted — a bound, not a number. Q-A189.2 remains open. | `B-A195` | yes — `thruline perf` derives currency here |
| peak RSS (packet, largest project) | 15 MiB against a 1024 MiB full-process budget. One run. | `B-A195` | yes — `thruline perf` derives currency here |
| now (single project, largest) | 23 ms / 25 ms vs 100 ms | `B-A192` | yes — `thruline perf` derives currency here |
| now --all | 36 ms / 37 ms vs 100 ms | `B-A192` | yes — `thruline perf` derives currency here |
| packet (single project, largest) | 25 ms / 27 ms vs 150 ms | `B-A192` | yes — `thruline perf` derives currency here |
| packet (repo-scoped) | 34 ms / 38 ms vs 150 ms | `B-A192` | yes — `thruline perf` derives currency here |
| projects (full aggregation, NOT a bare open) | 4652 ms / 5780 ms | `B-A192` | yes — `thruline perf` derives currency here |
| verify (whole ledger) | Exceeded the suite's 900 s ceiling on one run and was aborted — a bound, not a number. An unbounded run of the same command was still going at 43 minutes earlier the same evening. Whether this row belongs in the standing suite at all is Q-A189.2. | `B-A192` | yes — `thruline perf` derives currency here |
| peak RSS (packet, largest project) | 15 MiB against a 1024 MiB full-process budget. One run. | `B-A192` | yes — `thruline perf` derives currency here |
| now --all | 39 ms / 41 ms vs 100 ms | `B-A191` | **no** — basis unrecoverable, re-measure only |
| packet (repo-scoped) | 38 ms / 40 ms vs 150 ms | `B-A191` | **no** — basis unrecoverable, re-measure only |
| export --all, cold | 474.3 s — One run, 346 MB across 91 projects, largest document 66 MB. The number CLAUDE.md quotes. RA1's ~60 s cold bar remains open debt at 7.9x over. | `B-A186` | **no** — basis unrecoverable, re-measure only |
| verify (whole ledger) | 21.9 s — One run. 44 projects, ~1.4 GB. | `B-2026-08-06` | **no** — basis unrecoverable, re-measure only |
| verify (whole ledger) | One run, bounded: exceeded 600 s and was not run to completion 20 times. | `B-A189` | **no** — basis unrecoverable, re-measure only |
| peak RSS (packet, largest project) | 15 MiB against a 1024 MiB full-process budget. Not re-measured since. | `B-2026-08-06` | **no** — basis unrecoverable, re-measure only |
| chain-position head read (A117) | 2.2 µs on the largest project, flat across project size — the shape, not the number, is the evidence it is O(1). Verified equal to the uncached fold before being printed, by the measurement itself. | `B-2026-08-06` | **no** — basis unrecoverable, re-measure only |
| exec.db migration, v1 -> v2 (the gate's block ledger) | 2 ms — 2.2 ms to open and migrate 610 MB / 203,399 events / 44,075 sessions; 1.3 ms to open the same store once already migrated. The migration is a CREATE TABLE, an index and two triggers on a table that did not exist, so it rewrites no rows — the cost is independent of corpus size, which is why the two numbers are one millisecond apart.  **This is NOT an A193 migration and does not cost what one costs.** A193 governs `ledger.db` (7.4 GB here), where a DDL change costs a full pass, is charged to whichever process opens the ledger first, and makes every other command refuse until `thruline upgrade` runs. `exec.db` migrates automatically inside `Ledger::open` and refuses nothing. The two were conflated when this change was authorised, and the difference is recorded here because the authorisation was given against the larger cost. | `B-G2-exec-db` | **no** — basis unrecoverable, re-measure only |
| External report decoder timing, heap and separate process RSS | The versioned measurement artifact docs/integrations/validation/measurements.json contains the before/after values and variance. These are bounded microbenchmark observations; do not reuse as current application latency, field precision or whole-product memory savings. Re-run the named Criterion and DHAT harnesses after relevant code, toolchain or workload changes. | `B-integration-decoder-2026-09-09` | **no** — basis unrecoverable, re-measure only |
| HQ load, no-change reload of an already-loaded plan (probe, 242 files) | 26.76 s for 242 files (9.0 files/s), 0 rows added. Phases: setup (per-file orphan walk) 7.77 s, seal 8.44 s, codex_prepare 3.81 s (one live rollout re-mapped and refused), seal_accept 1.91 s, parse 1.24 s, ledger 0.57 s. | `B-C27-before` | **no** — basis unrecoverable, re-measure only |
| HQ load, no-change reload of an already-loaded plan (probe, 243 files) | 20 ms — 0.02 s and 0.01 s (two runs): 243 files skipped unopened, mirror not walked, nothing sealed or parsed. | `B-C27-after` | **no** — basis unrecoverable, re-measure only |
| HQ load, first load into an empty store (probe, 242 files, 826 MB) | 77.51 s (3.1 files/s). Phases: ledger 24.86 s, codex_prepare 23.61 s, seal_accept 11.13 s, seal 8.32 s, setup 6.61 s, parse 1.18 s. | `B-C27-before` | **no** — basis unrecoverable, re-measure only |
| HQ load, first load into an empty store (probe, 243 files, 827 MB) | 48.08 s with 4 worker threads (first thread visible to HQ after 1.21 s); 64.37 s with workers=0 (first thread after 0.49 s). The writer's ledger phase (~28-30 s) is the floor parallel reading cannot lower. | `B-C27-after` | **no** — basis unrecoverable, re-measure only |
| perf-check (b): POST load + poll until done, no change (186 files) | 66.4 s | `B-C27-before` | **no** — basis unrecoverable, re-measure only |
| perf-check (b): POST load + poll until done, no change (186 files) | 54 ms | `B-C27-after` | **no** — basis unrecoverable, re-measure only |
| brand-new user: register (25 newest) then one full load (186 files, 703 MB) | first threads visible 21.6 s after the first-use read (register 20.7 s); full load 165.2 s. | `B-C27-before` | **no** — basis unrecoverable, re-measure only |
| brand-new user: register (25 newest) then one full load (186 files, 703 MB) | first threads visible 4.8 s after the first-use read (register 4.5 s); full load 35.1 s (161 files read, the 25 registered ones skipped unopened). Register runs ingest(), whose code C27 changed only by restructuring; part of its difference may be machine load or other lanes' edits, and is not claimed by C27. | `B-C27-after` | **no** — basis unrecoverable, re-measure only |
| perf-check (a): server start → first GET /api/hq/projects/<id> answered | 3052 ms | `B-C27-before` | **no** — basis unrecoverable, re-measure only |
| perf-check (a): server start → first GET /api/hq/projects/<id> answered | 531 ms | `B-C27-after` | **no** — basis unrecoverable, re-measure only |
| perf-check (c): a Change screen's reads together, warm, slowest of 5 threads | 6287 ms | `B-C27-before` | **no** — basis unrecoverable, re-measure only |
| perf-check (c): a Change screen's reads together, warm, slowest of 5 threads | 612 ms — FAIL. 52-59 ms for threads whose view is 0.2-0.5 MB; 519 and 612 ms for two threads whose thread view is 7.3 and 8.1 MB of JSON. The cost is the thread route's size (view code), not the load; not changed by C27. | `B-C27-after` | **no** — basis unrecoverable, re-measure only |
| perf-check (b) on the real store: POST load + poll until done, nothing new to read | 3017 ms — 3.54 s and 3.02 s (runs 2 and 3): 577 of 578 recordings found unchanged, 1 read (a fork whose refusal stands), 0 rows added. | `B-C27-real-4fd53282` | **no** — basis unrecoverable, re-measure only |
| perf-check (b) on the real store while agents write: POST load + poll until done | 9913 ms — No time budget (C29 R2): reload while agents write is a separate figure from the no-change reload, recorded with its basis, and its budget is that it never blocks any screen. That budget is pinned by crates/cli/tests/hq_routes.rs a_load_that_is_still_writing_never_blocks_a_screen: while a load is running and has added rows, the project list, the project and a thread each answer 200 within 2 s. The 5000 ms budget this row first carried was removed under C29, and the word FAIL with it. Measured: 19.97 s (3,396 rows added), 9.91 s (406), 8.52 s (58). The same live conditions cost 15.0-17.5 s with 0 rows added at 40b37b0e, before an acceptance folded the project head from session rows; each live candidate then re-read all 601,272 events of the Codex project before its final check refused it. | `B-C27-real-df9a73f2` | **no** — basis unrecoverable, re-measure only |
| perf-check (a) on the real store: server start -> first GET /api/hq/projects/<id> answered | 2269 ms — FAIL: 3.30 s, 2.27 s, 2.08 s at load average 5.3-6.5. The same read path (40b37b0e, 16 read-only connections) gave 1.48 s and 1.79 s at load average 2.8-3.2, and 3.4-4.5 s from a cold page cache. Under 2 s cold needs a persisted derived index, which C26 rules out for v0.2. | `B-C27-real-df9a73f2` | **no** — basis unrecoverable, re-measure only |
| perf-check (c) on the real store: a Change screen's reads together, warm, slowest of 9 threads | 273 ms — 273 ms, 584 ms, 134 ms (slowest of 9 threads, 0.3-20 MB views, per run). Thread and project views are served from byte caches. The 584 ms run is not explained; it came right after a reload that added 406 rows, which is a candidate cause, not a measured one. | `B-C27-real-df9a73f2` | **no** — basis unrecoverable, re-measure only |
| perf-check (c) on the real store: a Change screen's reads together, warm, p50 / p95 over 20 switches x 3 runs (8 threads) | 65 ms / 101 ms vs 300 ms — p50 and p95 are nearest-rank over all 60 switches of the three runs; max 171 ms. Per run (p50 / p95 / max): run 1, load 6.78: 76 / 103 / 171 ms; run 2, load 7.69: 61 / 89 / 99 ms; run 3, load 7.57: 63 / 93 / 101 ms. Budget p95 < 300 ms (C29). The 584 ms of B-C27-real-df9a73f2 did not recur: no reload ran before or during these runs, so its candidate cause (a reload that had just added rows) was absent and is still not measured. | `B-C29-real-switch20-df9a73f2` | **no** — basis unrecoverable, re-measure only |
| perf-check (c) on the real store: a Change screen's reads together, warm, p50 / p95 over 20 switches x 3 runs (8 threads) | 65 ms / 106 ms vs 300 ms — p50 and p95 are nearest-rank over all 60 switches of the three runs; max 114 ms. Per run (p50 / p95 / max), with the 1-minute load average before it: run 1, load 11.96: 72 / 110 / 114 ms; run 2, load 11.94: 62 / 102 / 111 ms; run 3, load 12.10: 65 / 105 / 106 ms. Budget p95 < 300 ms (C29). | `B-C29-real-switch20-b5653371` | **no** — basis unrecoverable, re-measure only |
<!-- END perf-figures -->

**Three `verify` rows, and they contradict each other.** That is the point, and
it is the reason this table replaced a paragraph of prose. **21.9 s** (44
projects), **"over 600 s"** (A189), and **"exceeded a 900 s ceiling and was
aborted"** (A192) are all true readings, of different corpora, taken different
ways. The old file left the first two standing side by side with an apology
attached. Here each is a row naming its basis; only the A192 one is quotable,
and only because its basis can be checked. `Registry::validate` refuses two
figures for one measurement that share a basis, because two readings that cannot
be told apart are an unstamped assertion.

**`verify` has no known upper bound on this corpus**, and that is the honest
headline. An unbounded single run was still going at **43 minutes** during this
session before it was stopped; the recorded figure is the suite's 900 s ceiling
being hit, which is a bound rather than a number. It went 21.9 s → unbounded
while the corpus went 44 → 90 projects. Whether this row belongs in the standing
suite at all is **Q-A189.2**.

## Standing budgets

Budgets are not measurements. They do not age, they are not stamped, and they
are not redrawn around whatever number a run happens to produce:

| | budget |
|---|---|
| warm status / header | p95 ≤ 100 ms |
| receipt (`packet`) | p95 ≤ 150 ms |
| full-process peak RSS | ≤ 1 GiB |

Whether a 100 ms bar set when "all" meant 44 projects should govern 90 is
**Q-A189.1**, an open operator question. Until it is ruled on, the bar stands as
registered.

## The bases

Full stamps in [`perf-basis.toml`](perf-basis.toml). In short:

| basis | what it is | comparable? |
|---|---|---|
| `B-2026-08-06` | the original launch receipt: 44 projects, ~777k events, ~1.4 GB, and — discovered only at A189 — a clone on which `rebuild --all` had already been run, a precondition the table did not state | no |
| `B-A186` | the cold-export run CLAUDE.md quoted: 91 projects (the projects-table count), release, real corpus, cold. No event/session/node counts, no commit, no digest | no |
| `B-A189` | the A189 re-measurement: 90 projects, 2,012,204 events, ~6.5 GB, live ledger, never rebuilt. Deviated from this suite's own clone rule and said so: 6.5 GiB did not fit in 5.4 GiB free | no |
| `B-A191` | the index measurement that cleared both standing reds. The most completely stamped run before the machinery existed — and still not enough: the commit it names is the one *before* the index landed, the head counts are absent, and no digest ties the binary to any tree | no |
| `B-A195` | **the current basis at HEAD.** Same corpus, same harness, same method as `B-A192`; the only axis that moved was the code — A193 and A194 changed engine source, and every B-A192 figure correctly went stale on `code_digest`. That is the machinery earning its keep, not a regression | **yes** |
| `B-A192` | the first recoverable basis this project ever had. 90 projects · 2,012,204 events · 76,179 sessions · 87,132 nodes · 91 project heads · 90 node projections · release · code `sha256:b9ba03eda0b69685` · commit `d6ff63e` · harness sha recorded · load average recorded at both ends · clone made with `.backup` | **yes** |

**Every basis that predates this machinery is unrecoverable.** That is not a
defect in the machinery; it is the machinery reporting the true state of what
came before it. A190 established the principle and this table is its
consequence: *a figure whose basis can't be recovered can't be compared against,
only re-measured.* Four historical bases, four permanently unquotable sets of
figures, and one — `B-A192` — that a reader can actually check.

**The check runs both ways, which is how you know it is real.** On the machine
that took them, a **release** binary reads the current basis's figures as
current; a **debug** binary built from the same source reads **none** of them
that way, because the `profile` axis diverges — the same numbers, correctly
refused, on a build that did not produce them.

**And it has already expired one of its own bases in anger.** B-A192 was current
when it was taken. Two items later, A193 and A194 changed engine source, and
every one of those figures began rendering *"stale basis — re-measure
(code_digest)"* — naming the axis that moved. Nobody had to remember to do that,
which was the point.

### What a basis records

Comparison axes, all **observable** — nothing here is a human declaring what the
conditions were:

| axis | how it is read |
|---|---|
| `projects` | `SELECT COUNT(DISTINCT project_id) FROM events` |
| `events` | `SELECT COUNT(*) FROM events` |
| `sessions` | `SELECT COUNT(*) FROM sessions` |
| `nodes` | `SELECT COUNT(*) FROM nodes` |
| `project_heads` | `SELECT COUNT(*) FROM project_heads` |
| `node_projections` | `SELECT COUNT(*) FROM node_projection_heads` |
| `code_digest` | build-time digest over `crates/*/src`, the manifests, `Cargo.lock` |
| `profile` | `debug` or `release`, from `cfg!(debug_assertions)` |

Two of those choices are decisions rather than obvious readings:

- **"Rebuild state" is counted, not declared.** E2's wording names ledger and
  rebuild state as part of the stamp. A declared string ("never rebuilt") is a
  promise, and this whole item exists because promises about measuring
  conditions did not survive three days. So the stamp records what a rebuild
  actually *changes* — the head and projection counts — and reads them off the
  ledger.
- **The code axis is a digest of the source, not a commit sha.** E2 says "when
  the code beneath it changes". A markdown commit changes no code beneath a
  measurement, and a stamp that expired on every commit would be switched off
  within a day. A sha also does not establish that the binary which produced a
  number was built from it; a digest embedded at build time does.

`ledger_bytes`, `projects_table`, `commit`, `machine` and `harness` are recorded
as **descriptive context and never compared**. `ledger_bytes` is the clearest
case: merely opening the ledger during this item moved it by 1.1 MB without one
corpus row changing, and a `VACUUM` moves it by hundreds of megabytes.

**`projects` means one thing now.** On this corpus `COUNT(*) FROM projects` is
**91** and `COUNT(DISTINCT project_id) FROM events` is **90** — one project row
carries no events. A186 reported "91 projects" and A191 stamped "90 projects"
for the same corpus on the same day, and neither said which it meant.

## Reproducing

```bash
cargo build --release
sqlite3 ~/.thruline/ledger.db ".backup /tmp/ledger-clone.db"
scripts/perf-budget-suite.sh /tmp/ledger-clone.db 20
```

Three things about that block are P2b corrections, each paid for by something
that went wrong while it was being written:

1. **It completes, and that is now guaranteed rather than hoped for.** `verify`
   has no known upper bound on this corpus — one unbounded run was still going
   at 43 minutes — so twenty runs of everything was not a long command, it was
   an unbounded one, and nobody ever ran it as written. `verify` is now a single
   run with **no discarded warm-up** under a wall-clock ceiling
   (`VERIFY_BOUND_S`, default 900). Hitting the ceiling is reported as a bound,
   not as a number; a single un-warmed run is not a warm number and is not filed
   as one.

2. **`.backup`, never `cp`.** The suite always said "point it at a clone" and
   never said how. The ledger runs in **WAL mode**, so `cp ledger.db clone.db`
   copies the database without its write-ahead log and yields a clone SQLite
   rejects as *"database disk image is malformed"* — intermittently, depending
   on whether the WAL happened to be checkpointed at that instant. A `cp` clone
   failed exactly that way during this item. The clone matters because `verify`
   **writes** verification rows; the suite is read-only of the commands it times,
   not of the ledger it is pointed at.

3. **The clone is not the live ledger.** A189 measured the real ledger and
   declared the deviation, because 6.5 GiB did not fit in 5.4 GiB free. It fits
   now, and the rule is met rather than declared away.

The suite times **whole binary invocations** — process start, `Ledger::open`,
and the work — because that is what a person waits for. A measurement that
excluded startup would flatter the product against a budget it does not meet.
20 runs each, warm, one discarded warm-up, p95 by nearest rank: no interpolation,
so no reported value was ever un-measured. `packet --project` targets the
**largest** project by construction, so it is the worst case rather than a
typical one.

It cannot verify that you passed a clone rather than the real thing — that
precondition is yours to meet, and it is part of the measurement. Two
preconditions it now *does* handle:

- **A torn clone is named, not left to surface as a bare SQLite error** two
  lines later, with the `cp`-versus-`.backup` trap spelled out in the message.
- **Interrupting the suite kills the invocation it was timing.** Without that,
  stopping a superseded run leaves a fifteen-minute `verify` alive, and the next
  run is measured on a machine with a 6.4 GB scan going in the background.
  That happened here: an entire run's numbers were contaminated and nothing in
  the output said so. The suite now traps and kills its process group.
- **The load average travels with the stamp**, captured before and after. A
  developer's machine is rarely idle — this one was running another project's
  API server and a VM — and "best-effort quiet machine" asserted without a
  reading is not a precondition, it is a hope.

The run ends by printing a `[[basis]]` block and `[[figure]]` blocks in exactly
the shape the registry takes, so a fresh measurement is **pasted** rather than
transcribed. A191 transcribed its stamp by hand and lost the head counts doing
it.

---

# The record

Everything below is **history**: findings, with the occasion that produced each
one. They are not standing figures and none of them is quotable as current —
the standing figures are the table above, and their currency comes from
`thruline perf`. History is kept rather than swept, because the reasoning that
produced a wrong attribution is worth more than a clean table.

## The precondition this file used to hide (A125)

The first version of this table reported `now --all` at **122 ms p95**. That
number was real but conditional: it was measured on a clone where `rebuild --all`
had already been run, and the table did not say so. On a ledger in the state a
user's actually is — upgraded, never rebuilt — the same command took
**1,440–5,621 ms**, and stayed there across repeated warm runs. A 14× miss was
being reported as a 22 % one.

The cause was a design decision from A117, not a mystery: `derive_attention` asks
for the chain position on every project, and with `project_heads` unpopulated
each of those 44 asks fell back to the O(events) fold. The fallback is correct,
which is exactly why it was easy to miss.

**Fixed at the cause:** the migration that adds the persisted-head columns now
populates them in the same step, so the fast path is available on first open with
no user action. One-time cost on the corpus of the day: **7.4 s**, paid once,
during the upgrade. Afterwards the two states agreed — which is the point, since
the number no longer depended on knowing to run a command nobody documented.

**This finding is why `project_heads` and `node_projections` are comparison axes
today.** The condition that broke the measurement was invisible in the stamp
because the stamp had no field for it.

## `packet` on the largest project: 485 ms → 29 ms (A97-era)

The first run of this suite measured `packet` on the largest project at **485 ms
p95 against a 150 ms budget** — a failure, on a budget that had been recorded as
cleared. Attributing it phase by phase:

| phase | cost |
|---|---:|
| chain-position head | 0.04 ms |
| projection view (2,146 nodes) | 2.35 ms |
| **whole compile** | **497 ms** |

so the cost was neither the head nor the nodes. It was one query in
`missing_evidence_families`, joining `claim_searches → claims → events` filtered
on `e.project_id`. SQLite chose `SEARCH e USING idx_events_src_uuid
(project_id=?)` and walked **every one of the project's 473,242 events**, probing
for a claim on each.

Only **116 rows in the entire ledger** carry a non-empty `missing_families`, so
the selective side was never `events`. Driving from `claim_searches` with an
`EXISTS` reduced the event touch to one primary-key probe per surviving row: the
query alone went 1.0 s → 2 ms, `compile_request` 497 ms → 3.17 ms, `packet` end
to end 485 ms → 29 ms p95. Rows returned identical — **116 of 116 both ways, zero
in either difference**, checked across every project before the rewrite landed.

## Chain-position read (A117)

The L9 sub-item 6 receipt. Its **shape** — flat cost across project size — is the
evidence it is O(1), and the shape is what survives; the microsecond values
belong to `B-2026-08-06` and are re-measure-only.

| project | events | sessions | before | after |
|---|---:|---:|---:|---:|
| shop | 473,242 | 55,893 | 719.3 ms | 2.2 µs |
| shop-rate | 132,930 | 14,633 | 183.5 ms | 1.8 µs |
| notes | 33,880 | 26 | 33.3 ms | 2.2 µs |
| atlas | 17,708 | 24 | 13.9 ms | 2.0 µs |
| orbit | 11,169 | 5 | 7.0 ms | 2.2 µs |

Every "after" number was verified equal to the uncached fold over events before
being printed, by the measurement itself:

```bash
cargo test --release --test head_fold_bench -- --nocapture --ignored
```

## The attribution that was wrong twice (A189, then A191)

`now --all` missed its 100 ms budget by 10.3× on the 90-project corpus. The
attribution recorded against it was inverted, then corrected, and the correction
was **also** wrong. Both are kept.

**The first attribution** read: *"It is not a per-project cost: `now` on a single
project is 30 ms, and 44 ms of every invocation is `Ledger::open` — 42 % of the
failing measurement… well under 2 ms each."* Every part of that was false on
re-measurement:

| | recorded 2026-08-06 | measured 2026-08-15 |
|---|---:|---:|
| `now --all` | 101 / 104 ms (44 projects) | 966 / 1,033 ms (90 projects) |
| `Ledger::open` | 44 ms · 42 % of the failure | ~12 ms · ~1.2 % of it |
| per-project cost | "well under 2 ms each" | ~10.5 ms each |

`Ledger::open` had got **faster**, not slower, on a ledger 4.7× larger — A82's
schema-fingerprint fast path holding.

**Why it went wrong, which matters more than the number.** It rested on a row
labelled `projects (bare open)` at 44 ms. `thruline projects` is **not** a bare
open: it lists every project *with its event count*, a `GROUP BY` over the whole
`events` table. At 777k events that aggregation was cheap enough to pass for an
open. At 2.01 M it measured **13.6 s**. The 44 ms was never `Ledger::open` — it
was an open plus a full scan, and the scan was invisible at the old corpus size.
The row is labelled `projects (full aggregation, NOT a bare open)` in the suite
now, so it cannot be misread the same way again.

**A189's corrected cause was also wrong, and A191 found it out by fixing the real
one.** A189 concluded the dominant cost was per-project *derivation* — the debt
A186 left open. It was not. It was **`nodes` carrying no index on `project_id`**,
so every per-project projection read scanned all 87,132 rows. One
`CREATE INDEX` took `now --all` from ~1,018 ms to ~39 ms and `packet --repo` from
~159 ms to ~38 ms, with no change to derivation whatsoever.

A189 was right that the cost was per-project and right to refuse to optimise
`Ledger::open`. It was wrong about *which* per-project cost, and it reached that
conclusion **by elimination rather than by measurement** — the same failure mode
it had just documented one row above. Three directives were sent chasing it.

Full findings: [`recon/C-A189-PERF-REMEASURE.md`](recon/C-A189-PERF-REMEASURE.md)
and [`recon/C-A191-NODES-INDEX.md`](recon/C-A191-NODES-INDEX.md).

## The machine, and the bar it is not

| | |
|---|---|
| CPU | Apple M4 Pro, 14 cores |
| Memory | 24 GiB |
| OS | Darwin 25.1.0 (macOS 26.1) |

**This is not the oldest hardware.** L9's item is "perf on oldest hardware", and
an M4 Pro is the fastest machine the product has been measured on. Every number
here is an **upper bound on performance**, not a guarantee — the budgets must
still be re-run on the slowest supported machine before that phrasing can be
claimed. The gap is named rather than glossed.

## The HQ load (C27)

The figures are in `docs/perf-basis.toml` under `B-C27-before` and `B-C27-after`.
This section explains how they were taken and what they cover.

- **Per file, by phase:**
  `crates/core/tests/hq_load_speed_probe.rs::load_the_plan_file_by_file`.
  - It runs the code the route runs (`hq::run_load_with`) over a plan into a scratch store.
  - It prints the total, per-phase totals (`IngestReport::phase_ns`), files/s, the ten slowest files, and, after C27, the time until HQ can show the first thread.
  - Run it twice on one store to get a first load and then a no-change reload.
- **Over HTTP:** `scripts/hq/perf-check.mjs`. It checks three budgets against a running `thruline ui` and prints PASS/FAIL:
  - Inbox ready after start: < 2 s.
  - No-change reload: < 5 s.
  - A Change screen's reads, warm: < 300 ms.

  With `--start … --register` it first walks the brand-new-user path on an empty store.
- **In the gate:** `crates/core/tests/hq_load_speed.rs` runs in `cargo test --workspace`.
  - A no-change reload of 320 synthetic recordings must open none of them (every file is `chmod 000`).
  - It must take under a fifth of the first load and under 20 s. In debug it measured 58 ms against a 10.2 s first load.
  - Parallel and sequential loads must write the same rows.
- **Where the old cost was.** Every per-file `ingest` call walked every sealed generation in the mirror. On the founder's store that is 21,006 directories, measured read-only at ~0.4 s warm per walk, plus a marker-file read for each of the other source's 20,122 generations. On top of that, every unchanged file was copied, hashed about six times and parsed.
- **Not covered.** The full 569-file plan was not loaded into a scratch store, because free disk was at the 15 GiB floor. The founder's real store was not reloaded with the new code, because this lane does not write to it. The real-store no-change reload is therefore asserted, not measured.
  - Estimate, from the subset: 569 stat checks, about 35 ms.
  - If anything grew: one orphan walk, ~0.4 s warm, plus a full seal and parse of each grown live transcript.

## Open questions

- **Q-A189.1** — should a 100 ms bar set when "all" meant 44 projects govern 90?
  Operator ruling, not a coder's. The budget is not redrawn in the meantime.
- **Q-A189.2** — does `verify` leave the standing suite, get its own bounded
  harness, or keep a single-run row? P2b bounded it to one run so the suite is
  runnable as documented; that is a stopgap, not the answer.

## Decisions C24 scratch measurement (2026-10-07)

This is a separate, single-decision measurement, not an entry in the generated
performance registry above. The binary was the debug build of
`decisions/engine` at `ec667219` on the Apple M4 Pro named above. The ledger was
a scratch SQLite backup of the founder's record, upgraded to the C24 schema and
holding 2,096,236 events after the recorded C20 prompt was ingested. The target
was the exact `efdc063e` Git tree (`88e24ed88f49244682de3ea6f08ba707daac1712`,
4,007 files), with one accepted A1 decision and 1,912 findings. No live ledger
or website was written.

| operation | one wall-clock run | basis and receipt |
|---|---:|---|
| Exact-tree `--replay` | 0.63 s | `/usr/bin/time -p` around `thruline decisions check --version efdc063e --id 873731a4ff07f9be --replay --json`; exit 3; `~/Developer/thruline-decisions-evidence-20261007/logs/thruline-decisions-c20-perf-replay.time` |
| Cached verdict read and JSON output | 0.06 s | Same command without `--replay`; exit 3; `~/Developer/thruline-decisions-evidence-20261007/logs/thruline-decisions-c20-perf-cached.time` |
| One-time scratch ledger upgrade | 504.5 s | `thruline --db <scratch>/ledger.db upgrade`, 19 migration steps; this is not check latency. |

This single A1 check finished below two seconds. It does not establish the
`<2 s` warm census target. The 28-rule sweep, cached Inbox read, and other
hardware have not been measured.

### C1 full scratch visual runs

The C1 check used a separate scratch copy of the founder's record with
2,102,157 events in a 9,276,719,104-byte SQLite file. The machine was the
Apple M4 Pro above. The checker was a pinned debug binary (SHA-256
`cd5f7838d6d9c21e313784ed12757537010a0a5c8ae6b4badb9b96803406cba8`),
and the renderer was a pinned local Chrome executable (SHA-256
`ccffd5c5fe77ba3212e07c2c69091f8048b73c396b4647ecaa150e1e01a04954`).
Each run built the exact Git version in an isolated release target, exported
the full scratch record through that version's UI server, and evaluated all 14
configured screens using the same PNG bytes and zero masks. The historical
version rendered ten routes; four configured routes were absent, so no pixels
were captured for those screens. These are single wall-clock observations from
output-file creation to completion, not a warm latency distribution.

| exact version and operation | wall clock | outcome and receipt |
|---|---:|---|
| `efdc063e` stored check | 8m35s | 14 Broken; verdict `01M49YTZFYC0N773FG67B6Z26M`; `c1_efdc6.json` |
| `efdc063e` `--replay` | 7m38s | Entire verdict equal to stored result; `c1_efdc6_replay.json` |
| `d580b479` stored check | 9m51s | 11 Holds, 3 Broken; verdict `01M49ZW83ET5H9DBT7BTJ71E4C`; `c1_hq_d580.json` |
| `d580b479` `--replay` | 9m16s | Entire verdict equal to stored result; `c1_hq_d580_replay.json` |
| `7de1b8c5` stored check | 578.59 s | 11 Holds, 3 Broken, 0 Unverifiable; verdict `01M4A8Z4140BZZ7B5A91E9S8MY`; `c1_hq_7de1.json` |
| `7de1b8c5` `--replay` | 510.26 s | Entire verdict equal to stored result; same verdict ID; `c1_hq_7de1_replay.json` |

The six receipt files are preserved under the private evidence directory
`~/Developer/thruline-decisions-evidence-20261007/replay-58xfpa/`. The historical exporter
spends about five minutes of CPU in an unrelated project before the screen
captures begin. Even one C1 check is slower than the `<10 s` full decision
sweep target in C24 §7; the target is **not met** by this implementation.
The cached Inbox `<100 ms` target remains unmeasured.

At exact `7de1b8c52f53f4b55b1303dab8a7bbf00600cb20`, all 14 configured
routes rendered and were compared. Inbox, Onboarding, Change-Brief,
Every-Statement, What-Changed, Done-Criteria, Repair-Loop, Command-Ask,
Settings, and Team-View held at 0% difference; Record-Over-Time held at
0.0283241316%. Approve-Sheet was Broken at 1.6911806993% against the accepted
`Approve-Sheet.png` reference. Whole-Story was Broken at 3.4079034392%, with a
1512×2708 capture against a 1512×2800 reference. PR-Face was Broken at
11.3737075332%, with a 1512×2708 capture against a 1512×2400 reference.
The stored and replay receipts are respectively
`~/Developer/thruline-decisions-evidence-20261007/replay-58xfpa/c1_hq_7de1.json` and
`~/Developer/thruline-decisions-evidence-20261007/replay-58xfpa/c1_hq_7de1_replay.json`.

The private evidence folder's `MANIFEST` binds preserved files by SHA-256.
`SOURCE-MAP.json` maps each original source path to its preserved relative path,
size and hash. Raw JSON receipts retain their original bytes and historical
temporary paths; use the source map to find the preserved diff PNGs. Accepted
reference PNGs are preserved under `references/`, with their original paths
and hashes recorded in the same map.

### C24 replay refresh on HQ 99683489 and 2b7d4961

These are frozen-checker observations, using the same private scratch record,
checker digest, renderer digest, accepted PNG bytes and zero masks described
above. The scratch A1/C1 acceptances predate person-authority receipts and are
historical CLI simulations, as documented in `decisions/DECISIONS.md`. They do
not prove current person-authorized acceptance. Decisions code was being tested
while these explicit visual runs executed, so these are wall-clock observations
with concurrent targeted Cargo work, not isolated performance distributions.

| exact target and operation | wall clock | outcome and receipt |
|---|---:|---|
| A1 `99683489` stored / replay | 0.46 s / 0.37 s | Broken, 902 findings across 89 paths; entire verdict arrays equal; `01M4AJ07J1EB2W2TMRFX47S8KA`; `a1_hq_99683489.json` and `_replay.json` |
| C1 `99683489` stored / replay | 667.64 s / 610.93 s | 11 Holds, 3 Broken, 0 Unverifiable; entire verdict arrays equal; `01M4AKRQ6QVPK92EWTAG5P8KWX`; `c1_hq_99683489.json` and `_replay.json` |
| A1 `2b7d4961` stored / replay | 0.42 s / 0.40 s | Broken, 902 findings; entire verdict arrays equal; `01M4AMDXPV127AWSW2VK6BCD6H`; `a1_hq_2b7d4961.json` and `_replay.json` |

All receipts and `/usr/bin/time -p` logs are preserved under
`~/Developer/thruline-decisions-evidence-20261007/replay-58xfpa/`. A1's per-surface findings on
both heads are code 91, docs 560, tests 251 and UI copy 0. The UI surface Holds;
the combined decision is Broken. The accepted allowlist was retained.

C1's ten zero-difference screens and Record-Over-Time at 0.0283241316% Hold on
exact `9968348975c117f21f5a28751a32775f33721cb6`. Approve-Sheet remains Broken
at 1.6911806993% (23,525 pixels). Whole-Story is Broken at 3.4079034392%
(144,277 pixels), capture 1512×2708 versus reference 1512×2800. PR-Face is
Broken at 11.3737075332% (465,696 pixels), capture 1512×2708 versus reference
1512×2400. These remain below the required twelve Holds. The later HQ
`2b7d496139e6bcd0846fabc157797e3786c919ad` has not had a fresh C1 capture;
no older measurement is presented as proof for that head.

The full 28-rule `<10 s` target remains unmet by the explicit C1 path. The cached
Inbox `<100 ms` measurement and a complete eighteen-rule census remain open.

### C24 continuation: exact HQ 0630bb90 acceptance refresh

The Decisions branch was rebased onto `hq-next/ui` at
`0630bb90bc9ffdfa20475b8ca5a6011cbb9b9d0e`. These measurements use the same
scratch record, frozen `thruline-c1-c24-4` checker and pinned renderer described
above. They remain historical CLI acceptance simulations, without current
person-authority receipts. Live records and accepted references were unchanged.
Targeted Decisions compilation ran concurrently, so these are observed wall
times, not isolated performance distributions.

| exact target and operation | wall clock | outcome and receipt |
|---|---:|---|
| A1 `0630bb90` stored / replay | 0.54 s / 0.31 s | Broken, 902 findings: code 91, docs 560, tests 251, UI copy 0; entire verdict arrays equal; `01M4AP263SS4R74FB4FRC6QB3P` |
| C1 `0630bb90` stored / replay | 674.99 s / 541.89 s | 11 Holds, 3 Broken, 0 Unverifiable; entire verdict arrays equal; `01M4AP1BGG4M4A2N3ME6EBP9JX` |

The evidence files are `a1_hq_0630bb90.json`, `a1_hq_0630bb90_replay.json`,
`c1_hq_0630bb90.json`, and `c1_hq_0630bb90_replay.json`, with their
`/usr/bin/time -p` stderr logs, preserved under
`~/Developer/thruline-decisions-evidence-20261007/replay-58xfpa/`. All fourteen diff PNGs and
accepted reference PNGs were read and their SHA-256 values matched the receipt.

C1 retained the same ten zero-difference screens and Record-Over-Time Hold
(0.0283241316%, 394 pixels). Approve-Sheet is Broken at 1.6911806993%
(23,525 pixels). Whole-Story is Broken at 3.4079034392% (144,277 pixels),
1512×2708 against a 1512×2800 reference. PR-Face is Broken at 11.3737075332%
(465,696 pixels), 1512×2708 against a 1512×2400 reference. The required twelve
Holds and full-sweep time target remain unmet.


A1 was also checked on the Decisions implementation commit
`72583417a67fde3f2ce9f39c04504995f561599d`: **955 findings**, code 91,
docs 560, tests 304, UI copy 0. Its 53 additional matches are in the Decisions
synthetic test fixtures. Stored and replay verdict arrays are equal,
receipt `01M4APVDW8GKQK0Y6T7WB79H6V`, with **0.46 s / 0.27 s** wall times.
Evidence: `a1_decisions_72583417.json`, `_replay.json`, and their stderr logs
in the same scratch directory. The accepted terms, surfaces and allowlist
were unchanged; the broad-scope Holds requirement remains unmet.


### C35 · final engine integration, 2026-10-08

The founder deferred live-record speed measurement until a coordinated
whole-store APFS copy with Claude Code in the morning. The earlier ledger-only
copy has 14 raw/generation mismatches and does not qualify as that basis. No
new whole-store clone or live-store performance run occurred overnight.

The isolated pinned Inbox fixture passed with zero changed pixels and a
18.084 s checker duration. It used an empty synthetic record and a previously
verified renderer identity; UI/archive/capture inputs were fresh. It does not
prove founder-record latency, a cold renderer run, or any full-sweep target.
Natural final replay timings are preserved with their exact frozen basis in
the private C35 final report. The warm census, 28-decision sweep and cached
Inbox targets still require the verified coordinated copy.
