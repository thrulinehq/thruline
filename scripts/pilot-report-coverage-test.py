#!/usr/bin/env python3
"""Observed refusals and a monotonicity proof for the coverage report generator.

Run it:
    python3 scripts/pilot-report-coverage-test.py

WHAT THIS IS FOR (operator Ruling 6, BUILD-SPEC §12 / §20.3)
------------------------------------------------------------
A task the instrument breaks produces an EMPTY spool. Folded into an average
whose denominator still counts it, that empty spool contributes a ZERO, and the
metric then reports *the instrument's own damage* as *the product
under-performing* — the failure direction nobody double-checks, because it makes
the product look worse rather than better.

So the generator must refuse to emit a figure it has not read an
instrument-failures set for, and a task marked as an instrument failure must
never be able to lower coverage.

THE SECOND HOLE, CLOSED 2026-09-04 (the one the published figure walked through)
--------------------------------------------------------------------------------
The refusal above was keyed on the set being READABLE. The published 64/99
chain-coverage figure still did not come out of this tool: it came out of an
ad-hoc aggregator, and the committed set classified 3 of that measurement's 24
exclusions. A refusal keyed on "the file exists" is satisfied by an empty file.

So the suite now also holds, each watched refusing:

  * `test_empty_set_is_not_an_unread_set` — the crux. Two sets with BYTE-
    IDENTICAL (empty) entry lists over byte-identical run records: one carries
    an attestation saying "for prefix t-, nothing was excluded, and I checked",
    the other carries none. The first EMITS; the second exits 11. "No instrument
    failures" and "I never looked" no longer share a representation.
  * `test_attestation_must_be_complete_for_this_run` — staleness in BOTH
    directions (under-counted and over-counted) exits 12.
  * `test_scope_is_per_run_not_per_task` — an entry derived from one measurement
    does not classify another, and two entries that both apply to one run refuse
    rather than tie-break.
  * `test_chain_coverage_line_needs_the_set` — the §20.1 line the gate is
    actually about is computed HERE, moves with the evidence, and does not exist
    on any path that skipped the set.
  * `test_task_named_from_the_mount_when_the_harness_never_ran` — the six m1
    runs that died before Terminal-Bench are named from `task-provenance.txt`
    and then classified, instead of making the tool refuse the whole
    measurement as unnameable.

THE TRAP THIS SUITE IS BUILT AROUND
-----------------------------------
"Coverage did not go down" is satisfied by a stub returning a constant, and by
an implementation that excludes everything. Monotonicity alone is satisfiable by
doing nothing. So every monotonicity assertion here is PAIRED:

  * the same run records WITHOUT the instrument marking must produce a
    STRICTLY LOWER figure (test_monotonicity_with_paired_negative), and
  * genuine, unbroken tasks with real evidence must COUNT — a real denominator
    and a real numerator, never 0 of 0 (test_positive_evidence_counts), and
  * degrading one genuine task's evidence must LOWER the figure
    (test_numerator_tracks_evidence), so a constant fails.

Every refusal below is EXERCISED — the real script is run and its exit code and
stderr are read. This program has shipped nine guards nobody ever watched
refuse; none of them are here.

It needs no Docker, no corpus, no network and no credential: the run records
are a few hundred bytes of JSON each, built in a temp directory.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
TOOL = HERE / "pilot-report.py"
DEFAULT_SET = HERE / "instrument-failures.json"
PY = sys.executable

# Exit codes the generator promises. Asserted, not assumed: a refusal that
# exits 1 is indistinguishable from a traceback.
EXIT_NO_FAILSET = 6
EXIT_UNCLASSIFIED = 7
EXIT_VOCABULARY = 8
EXIT_NO_DENOMINATOR = 9
EXIT_EXCLUDED_TASK = 10
EXIT_NO_ATTESTATION = 11
EXIT_ATTESTATION_STALE = 12

FAILURES: list[str] = []
CHECKS = 0


def check(cond, label, detail=""):
    global CHECKS
    CHECKS += 1
    if cond:
        print("  ok   %s" % label)
    else:
        print("  FAIL %s%s" % (label, ("\n         " + detail) if detail else ""))
        FAILURES.append(label)


def head(t):
    print("\n== %s ==" % t)


# --------------------------------------------------------------------------
# Fixture builders. The shapes are the ones measured on the real 10-task probe:
# a healthy run has mount/READY, mount/exec.db and a non-empty spool; a run the
# instrument broke has none of the three.
# --------------------------------------------------------------------------

def make_run(pilot: Path, task: str, *, prefix="t-", started=True, spool=True,
             exec_db=True, instrumented=True, graded=True, resolved=True,
             agent="claude-code", service_user=None, hooks=True, execs=True,
             tree_hash="c" * 40, results=True, provenance=None,
             tool_calls=False, snapshot_tree=None):
    rid = prefix + task
    mount = pilot / ("mount-" + rid)
    (mount / "spool").mkdir(parents=True, exist_ok=True)
    if started:
        (mount / "READY").write_text("instrumented\n")
    if exec_db:
        # A REAL sqlite file with the two tables the per-task path reads. A
        # stand-in text file passed the coverage path (which only asks whether
        # the store exists) and made the per-task path traceback — a fixture
        # that is only as real as the first code path to touch it.
        #
        # `payload` IS PART OF THAT REALISM AND WAS MISSING. Every m1 exec.db
        # carries it; this fixture did not, so the alternate component-3
        # predicate (which reads `payload`) could not be exercised at all. A
        # fixture narrower than the store hides exactly the predicate under
        # test. `tool_calls` and `snapshot_tree` exist so the two readings can
        # be made to DISAGREE on purpose.
        import sqlite3
        con = sqlite3.connect(str(mount / "exec.db"))
        try:
            con.execute("create table events (source text, kind text, "
                        "payload text not null default '{}')")
            con.execute("create table checks (runner text, exit_code int, "
                        "input_tree_hash text not null, parsed text)")
            if hooks:
                con.execute("insert into events(source, kind) "
                            "values ('stop-hook', 'stop')")
            if execs:
                con.execute("insert into events(source, kind) "
                            "values ('exec-recorder', 'exec')")
            if tool_calls:
                con.execute("insert into events(source, kind) "
                            "values ('claude-code-hook', 'tool_call')")
            if snapshot_tree:
                con.execute("insert into events(source, kind, payload) "
                            "values ('git', 'snapshot', ?)",
                            (json.dumps({"tree_hash": snapshot_tree}),))
            con.execute("insert into checks values ('pytest', 0, ?, 'yes')",
                        (tree_hash,))
            con.commit()
        finally:
            con.close()
    if spool:
        (mount / "spool" / "agent.jsonl").write_text('{"kind":"exec"}\n')
    if provenance:
        (mount / "task-provenance.txt").write_text(
            "copied from /some/where/original-tasks/%s\n" % provenance)

    rec = pilot / ("receipts-" + rid)
    rec.mkdir(parents=True, exist_ok=True)
    if instrumented:
        (rec / (task + ".json")).write_text(json.dumps({
            "schema": "thruline.compose-instrument/1",
            "compose": "/staged/%s/docker-compose.yaml" % task,
            "client_service": "client",
            "command_wrapped": True,
            "sha256_before": "a" * 64,
            "sha256_after": "b" * 64,
            "service_user": service_user,
        }))

    if not results:
        # The six m1 runs that died before Terminal-Bench was invoked: no run
        # directory at all, so nothing but the mount can name the task.
        return rid

    rd = pilot / ("runs-" + rid) / rid
    rd.mkdir(parents=True, exist_ok=True)
    (rd / "results.json").write_text(json.dumps({
        "results": [{
            "task_id": task,
            "is_resolved": bool(resolved) if graded else None,
            "failure_mode": "unset" if graded else "parse_error",
        }]
    }))
    (rd / "run_metadata.json").write_text(json.dumps({
        "run_id": rid, "agent_name": agent, "task_ids": [task],
        # The harness's own average — the number that hid the damage.
        "accuracy": 1.0 if (graded and resolved) else 0.0,
    }))
    return rid


def write_set(path: Path, entries, schema="thruline.instrument-failures/2",
              *, prefix="t-", excluded=None, attest=True, attestations=None,
              omit_attestations=False):
    """Write a failset.

    `excluded` defaults to the tasks the entries themselves exclude, which is
    the honest default for a fixture. It is a PARAMETER because the attestation
    is the thing under test: the suite must be able to write one that is empty,
    stale, or absent, and watch each refuse differently.
    """
    doc = {"schema": schema, "derived_on": "2026-09-04", "entries": entries}
    if not omit_attestations:
        if attestations is not None:
            doc["attestations"] = attestations
        elif attest:
            if excluded is None:
                excluded = sorted({e["task"] for e in entries
                                   if e.get("class") != "product"})
            doc["attestations"] = [{"run_prefix": prefix,
                                    "attested_on": "2026-09-04",
                                    "excluded": excluded}]
        else:
            doc["attestations"] = []
    path.write_text(json.dumps(doc, indent=1))
    return path


def entry(task, cls="instrument", stage="started", mech=None, *,
          applies_to=("t-",), tier="reproduced"):
    return {"task": task, "class": cls, "stage_lost": stage,
            "applies_to": list(applies_to), "evidence_tier": tier,
            "mechanism": mech or ("the appended RUN layer inherits USER agent, so "
                                  "apt-get is denied and the image never builds")}


def run_coverage(pilot: Path, prefix, failset: Path | None, *, extra=(), env_set=None):
    """Run the generator. Returns (rc, stdout, stderr, figure-or-None)."""
    out = pilot / "figure.json"
    if out.exists():
        out.unlink()
    argv = [PY, str(TOOL), "--coverage", "--pilot-dir", str(pilot),
            "--run-prefix", prefix, "--json", str(out)] + list(extra)
    if failset is not None:
        argv += ["--instrument-failures", str(failset)]
    env = {"PATH": "/usr/bin:/bin", "HOME": str(pilot)}
    if env_set:
        env.update(env_set)
    p = subprocess.run(argv, capture_output=True, text=True, env=env)
    fig = json.loads(out.read_text()) if out.exists() else None
    return p.returncode, p.stdout, p.stderr, fig


# --------------------------------------------------------------------------
# 1. THE REFUSALS, each watched refusing.
# --------------------------------------------------------------------------

def test_refusals(tmp: Path):
    head("refusals — each one exercised, not asserted about")

    pilot = tmp / "refuse"
    pilot.mkdir()
    for t in ("alpha", "beta"):
        make_run(pilot, t)

    rc, so, se, fig = run_coverage(pilot, "t-", pilot / "does-not-exist.json")
    check(rc == EXIT_NO_FAILSET, "absent set refuses with %d" % EXIT_NO_FAILSET,
          "rc=%d stderr=%s" % (rc, se.strip()[:200]))
    check("instrument-failures set" in se, "and says what is missing", se[:200])
    check(fig is None and "DENOMINATORS" not in so,
          "and emits NO figure at all", so[:200])

    bad = pilot / "notjson.json"
    bad.write_text("{ this is not json")
    rc, so, se, fig = run_coverage(pilot, "t-", bad)
    check(rc == EXIT_NO_FAILSET and fig is None,
          "unparseable set refuses", "rc=%d" % rc)

    rc, so, se, fig = run_coverage(
        pilot, "t-", write_set(pilot / "wrongschema.json", [], schema="nope/9"))
    check(rc == EXIT_NO_FAILSET and fig is None, "wrong schema refuses", "rc=%d" % rc)

    e = entry("alpha")
    e["mechanism"] = "broken"
    rc, so, se, fig = run_coverage(pilot, "t-", write_set(pilot / "nomech.json", [e]))
    check(rc == EXIT_NO_FAILSET and "mechanism" in se,
          "an exclusion with no named mechanism refuses",
          "rc=%d stderr=%s" % (rc, se.strip()[:160]))

    e = entry("alpha")
    del e["mechanism"]
    rc, _, se, fig = run_coverage(pilot, "t-", write_set(pilot / "nomech2.json", [e]))
    check(rc == EXIT_NO_FAILSET, "an exclusion with no mechanism KEY refuses", "rc=%d" % rc)

    e = entry("alpha", cls="convenient")
    rc, _, se, fig = run_coverage(pilot, "t-", write_set(pilot / "badclass.json", [e]))
    check(rc == EXIT_NO_FAILSET and "class" in se, "an invented class refuses", se[:160])

    e = entry("alpha", stage="mostly")
    rc, _, se, fig = run_coverage(pilot, "t-", write_set(pilot / "badstage.json", [e]))
    check(rc == EXIT_NO_FAILSET and "stage_lost" in se,
          "a stage outside §1.1's three refuses", se[:160])

    rc, _, se, fig = run_coverage(
        pilot, "t-", write_set(pilot / "dup.json", [entry("alpha"), entry("alpha")]))
    check(rc == EXIT_NO_FAILSET and "repeats" in se,
          "one task classified twice refuses", se[:160])

    # The refusal the real corpus needs: `spring-messaging-vul` has the same
    # empty-spool signature as two instrument failures and is neither one.
    broken = tmp / "unclassified"
    broken.mkdir()
    make_run(broken, "healthy")
    make_run(broken, "empty-capture", started=False, spool=False, exec_db=False,
             graded=False, resolved=False)
    rc, so, se, fig = run_coverage(broken, "t-", write_set(broken / "s.json", []))
    check(rc == EXIT_UNCLASSIFIED, "an UNCLASSIFIED empty capture refuses with %d"
          % EXIT_UNCLASSIFIED, "rc=%d stderr=%s" % (rc, se.strip()[:220]))
    check(fig is None and "DENOMINATORS" not in so,
          "and no figure is emitted around it", so[:160])
    check("empty-capture" in se, "and the refusal names the task", se[:220])

    empty = tmp / "norun"
    empty.mkdir()
    rc, _, se, fig = run_coverage(empty, "t-", write_set(empty / "s.json", []))
    check(rc == EXIT_NO_DENOMINATOR and fig is None,
          "no run records at all refuses with %d" % EXIT_NO_DENOMINATOR, "rc=%d" % rc)

    allgone = tmp / "allexcluded"
    allgone.mkdir()
    make_run(allgone, "one")
    make_run(allgone, "two")
    rc, so, se, fig = run_coverage(allgone, "t-", write_set(
        allgone / "s.json", [entry("one"), entry("two")]))
    check(rc == EXIT_NO_DENOMINATOR and fig is None,
          "every task excluded refuses rather than printing 0 of 0", "rc=%d" % rc)
    check("not a figure" in se, "and says why an empty denominator is not one",
          se[:200])

    # Vocabulary (§1.1): an oracle number is instrument reach, never coverage.
    orc = tmp / "oracle"
    orc.mkdir()
    make_run(orc, "alpha", agent="oracle")
    s = write_set(orc / "s.json", [])
    rc, so, se, fig = run_coverage(orc, "t-", s, extra=["--as-coverage"])
    check(rc == EXIT_VOCABULARY and fig is None,
          "`--as-coverage` on oracle runs refuses with %d" % EXIT_VOCABULARY,
          "rc=%d stderr=%s" % (rc, se.strip()[:200]))
    rc, so, se, fig = run_coverage(orc, "t-", s)
    check(rc == 0 and fig["label"] == "instrument-reach",
          "and without the flag the same runs are labelled instrument-reach")
    check("NOT COVERAGE" in so and "INSTRUMENT REACH" in so,
          "and the printed report says so in words")


# --------------------------------------------------------------------------
# 2. THE MONOTONICITY TEST — with its paired negative and its paired positive.
# --------------------------------------------------------------------------

def _three_good_and_one_broken(pilot: Path):
    for t in ("alpha", "beta", "gamma"):
        make_run(pilot, t)
    # The exact shape measured on `shell-deobfuscation` and
    # `privilege-escalation`: no sentinel, no exec.db, empty spool. The compose
    # WAS patched (a receipt exists), which is why `instrumented` and `started`
    # are separate facts.
    make_run(pilot, "broken-by-instrument", started=False, spool=False,
             exec_db=False, graded=False, resolved=False)


def test_monotonicity_with_paired_negative(tmp: Path):
    head("monotonicity — a task marked instrument-failure can never lower coverage")

    pilot = tmp / "mono"
    pilot.mkdir()
    _three_good_and_one_broken(pilot)

    marked = write_set(pilot / "marked.json", [
        entry("broken-by-instrument", cls="instrument", stage="started")])
    # The SAME record, classified as a genuine ThruLine zero instead. This is
    # the paired negative: without it, "coverage did not go down" is satisfied
    # by a stub, by an always-exclude, and by an empty denominator.
    unmarked = write_set(pilot / "unmarked.json", [
        entry("broken-by-instrument", cls="product", stage="started",
              mech="the capture is empty and that IS a ThruLine result")],
        excluded=[])

    rc_a, so_a, se_a, a = run_coverage(pilot, "t-", marked)
    rc_b, so_b, se_b, b = run_coverage(pilot, "t-", unmarked)
    check(rc_a == 0 and rc_b == 0, "both arms emit a figure",
          "rc_a=%d rc_b=%d %s%s" % (rc_a, rc_b, se_a[:120], se_b[:120]))
    if not (a and b):
        check(False, "both arms produced a figure to compare")
        return

    check(a["denominator"] == 3, "MARKED: denominator EXCLUDES the broken task (3)",
          "got %d" % a["denominator"])
    check(b["denominator"] == 4, "UNMARKED: denominator INCLUDES it (4)",
          "got %d" % b["denominator"])

    for stage in ("instrumented", "started", "graded"):
        ra = a[stage] / a["denominator"]
        rb = b[stage] / b["denominator"]
        check(ra >= rb, "marking never lowers `%s` (%.3f >= %.3f)" % (stage, ra, rb))

    # THE PAIRED NEGATIVE, stated strictly: without the marking the SAME input
    # produces a STRICTLY LOWER figure on the two stages the break destroyed.
    for stage in ("started", "graded"):
        ra = a[stage] / a["denominator"]
        rb = b[stage] / b["denominator"]
        check(ra > rb,
              "and WITHOUT the marking `%s` is strictly lower (%.3f > %.3f) — so "
              "this test cannot pass for an unrelated reason" % (stage, ra, rb))

    check(b["started"] == 3 and b["denominator"] == 4,
          "the unmarked arm is exactly the defect: 3 of 4 = 0.750, a zero "
          "contributed by the instrument")
    check(a["started"] == 3 and a["denominator"] == 3,
          "the marked arm is 3 of 3 = 1.000 over what was actually measurable")

    names = [f["task"] for f in a["exclusions"]["instrument"]]
    check(names == ["broken-by-instrument"],
          "the excluded task is REPORTED, not merely dropped", str(names))
    check(a["exclusions"]["instrument"][0]["mechanism"].split(),
          "and it carries its mechanism into the machine-readable figure")
    check("broken-by-instrument" in so_a and "mechanism:" in so_a,
          "and into the printed report, beside the figure and never inside it")
    check(b["exclusions"]["instrument"] == [],
          "the unmarked arm reports no instrument failures — the two arms differ "
          "ONLY in the marking")


def test_positive_evidence_counts(tmp: Path):
    head("paired positive — genuine tasks with real evidence DO count")

    pilot = tmp / "positive"
    pilot.mkdir()
    _three_good_and_one_broken(pilot)
    marked = write_set(pilot / "s.json", [entry("broken-by-instrument")])
    rc, so, se, fig = run_coverage(pilot, "t-", marked)
    check(rc == 0, "emits", se[:200])
    if not fig:
        return
    # Against "always exclude everything" and against a constant.
    check(fig["denominator"] == 3, "the denominator is 3, not 0 — excluding the "
          "broken task did not empty the measurement", "got %d" % fig["denominator"])
    check(sorted(fig["counted"]) == ["alpha", "beta", "gamma"],
          "and the three genuine tasks are named as counted", str(fig["counted"]))
    for stage in ("instrumented", "started", "graded"):
        check(fig[stage] == 3, "`%s` counts all 3 genuine tasks" % stage,
              "got %d" % fig[stage])


def test_numerator_tracks_evidence(tmp: Path):
    head("paired positive — the numerator moves when the EVIDENCE moves")

    # A constant, or an implementation that just returns the denominator, passes
    # everything above. This is what it cannot pass: one genuine task whose
    # tests never produced a parsed result — a real ThruLine zero, with a
    # non-empty capture, so nothing may exclude it.
    pilot = tmp / "tracks"
    pilot.mkdir()
    make_run(pilot, "alpha")
    make_run(pilot, "beta")
    make_run(pilot, "gamma", graded=False, resolved=False)
    rc, so, se, fig = run_coverage(pilot, "t-", write_set(pilot / "s.json", []))
    check(rc == 0, "emits with no exclusions at all", se[:200])
    if not fig:
        return
    check(fig["denominator"] == 3 and fig["started"] == 3,
          "denominator 3, started 3")
    check(fig["graded"] == 2,
          "graded is 2 of 3 — an ungraded task with a real capture stays in and "
          "LOWERS the figure", "got %d" % fig["graded"])
    check(all(v == [] for v in fig["exclusions"].values()),
          "and nothing was quietly excluded to make it look better")


def test_three_denominators_are_separate(tmp: Path):
    head("§1.1 — instrumented / started / graded never share a denominator")

    pilot = tmp / "three"
    pilot.mkdir()
    make_run(pilot, "all-three")
    # instrumented, never started (the compose was patched; the container died).
    make_run(pilot, "not-started", started=False, graded=False, resolved=False)
    # started, never graded (lean4-proof: the grader exits 127).
    make_run(pilot, "not-graded", graded=False, resolved=False)
    s = write_set(pilot / "s.json", [
        entry("not-started", cls="product", stage="started",
              mech="classified as a genuine product zero so it stays countable")])
    rc, so, se, fig = run_coverage(pilot, "t-", s)
    check(rc == 0, "emits", se[:200])
    if not fig:
        return
    check((fig["instrumented"], fig["started"], fig["graded"]) == (3, 2, 1),
          "three different numerators over one denominator of 3: "
          "instrumented=3 started=2 graded=1",
          "got %s" % str((fig["instrumented"], fig["started"], fig["graded"])))
    check("1. instrumented" in so and "2. started" in so and "3. graded" in so,
          "and the report prints them as three lines, never as one percentage")
    # The §1.1 block must carry NO percentage. The §20.1 chain block below it
    # does print one per component and one for the conjunction — that is three
    # separate predicates and their AND, not a mean of unlike things.
    stage_block = so.split("DENOMINATORS", 1)[1].split("CHAIN ", 1)[0]
    check("%" not in stage_block,
          "no blended percentage anywhere in the instrumented/started/graded block",
          stage_block[:300])



# --------------------------------------------------------------------------
# 2b. THE CRUX: "no instrument failures" and "I never looked" are different
#     facts and may not share a representation.
# --------------------------------------------------------------------------

def test_empty_set_is_not_an_unread_set(tmp: Path):
    head("empty vs unread — the two must not share a representation")

    # A run set with NOTHING wrong in it. This is the only place the two cases
    # can be told apart at all: where there are exclusions, an unread set
    # already fails the completeness check. Where there are none, a set that
    # says nothing and a set nobody opened look identical — unless the set is
    # made to SAY that it looked.
    clean = tmp / "clean"
    clean.mkdir()
    for t in ("alpha", "beta", "gamma"):
        make_run(clean, t)

    looked = write_set(clean / "looked.json", [], excluded=[])
    rc, so, se, fig = run_coverage(clean, "t-", looked)
    check(rc == 0, "'I looked, and there were none' EMITS", se[:300])
    check(fig is not None and fig["denominator"] == 3,
          "over a real denominator of 3")
    check(bool(fig) and fig["failset"].get("attestation") == {
              "run_prefix": "t-", "excluded": [], "read": True},
          "and the figure carries the attestation it was emitted under",
          str(fig and fig["failset"].get("attestation")))
    check("READ and matched against the run records" in so,
          "and the printed report says the set was read", so[:600])

    # THE PAIRED NEGATIVE. Byte-for-byte the same entries — an empty list — on
    # byte-for-byte the same run records. The ONLY difference is that nobody
    # attested to this run. It must refuse, and with its own code.
    never = write_set(clean / "never.json", [], attest=False)
    rc2, so2, se2, fig2 = run_coverage(clean, "t-", never)
    check(rc2 == EXIT_NO_ATTESTATION,
          "'I never looked' REFUSES with %d — same entries, same records"
          % EXIT_NO_ATTESTATION, "rc=%d stderr=%s" % (rc2, se2.strip()[:300]))
    check(fig2 is None and "DENOMINATORS" not in so2,
          "and emits no figure at all", so2[:200])
    check("never looked" in se2,
          "and names the distinction rather than blaming the file", se2[:400])
    check(json.loads(looked.read_text())["entries"]
          == json.loads(never.read_text())["entries"] == [],
          "the two sets have IDENTICAL entries — so the difference cannot be "
          "the entry list")

    # And the older, weaker predicate: a set that merely EXISTS.
    nokey = clean / "nokey.json"
    nokey.write_text(json.dumps({"schema": "thruline.instrument-failures/2",
                                 "derived_on": "2026-09-04", "entries": []}))
    rc3, so3, se3, fig3 = run_coverage(clean, "t-", nokey)
    check(rc3 == EXIT_NO_FAILSET and fig3 is None,
          "a set with no `attestations` KEY refuses with %d — 'the file exists' "
          "is not the predicate" % EXIT_NO_FAILSET,
          "rc=%d %s" % (rc3, se3.strip()[:200]))


def test_attestation_must_be_complete_for_this_run(tmp: Path):
    head("completeness — the attestation is checked against the disk, both ways")

    pilot = tmp / "attest"
    pilot.mkdir()
    for t in ("alpha", "beta"):
        make_run(pilot, t)
    make_run(pilot, "broken", started=False, spool=False, exec_db=False,
             graded=False, resolved=False)

    # Classified, but the attestation claims nothing was excluded. This is the
    # exact shape the m1 measurement had: a set that classifies SOME of what a
    # run excludes and is silent about the rest.
    understated = write_set(pilot / "under.json", [entry("broken")], excluded=[])
    rc, so, se, fig = run_coverage(pilot, "t-", understated)
    check(rc == EXIT_ATTESTATION_STALE and fig is None,
          "an attestation that under-counts the exclusions refuses with %d"
          % EXIT_ATTESTATION_STALE, "rc=%d %s" % (rc, se.strip()[:300]))
    check("broken" in se, "and names the task it failed to attest to", se[:300])

    # The other direction: attesting to an exclusion that did not happen.
    overstated = write_set(pilot / "over.json", [entry("broken")],
                           excluded=["broken", "a-task-that-was-never-run"])
    rc, so, se, fig = run_coverage(pilot, "t-", overstated)
    check(rc == EXIT_ATTESTATION_STALE and fig is None,
          "and one that over-counts refuses too — staleness has two directions",
          "rc=%d %s" % (rc, se.strip()[:300]))
    check("a-task-that-was-never-run" in se, "naming the phantom", se[:300])

    exact = write_set(pilot / "exact.json", [entry("broken")])
    rc, so, se, fig = run_coverage(pilot, "t-", exact)
    check(rc == 0 and bool(fig) and fig["denominator"] == 2,
          "and the matching one emits, over 2 counted tasks", se[:300])


def test_scope_is_per_run_not_per_task(tmp: Path):
    head("`applies_to` — a mechanism measured in one run does not classify another")

    pilot = tmp / "scope"
    pilot.mkdir()
    make_run(pilot, "alpha")
    make_run(pilot, "wanderer", started=False, spool=False, exec_db=False,
             graded=False, resolved=False)

    e = entry("wanderer")
    del e["applies_to"]
    rc, _, se, fig = run_coverage(pilot, "t-", write_set(pilot / "noscope.json", [e]))
    check(rc == EXIT_NO_FAILSET and "applies_to" in se,
          "an entry with no `applies_to` refuses", "rc=%d %s" % (rc, se[:200]))

    # Scoped to a DIFFERENT measurement: it must not classify this one.
    other = write_set(pilot / "other.json",
                      [entry("wanderer", applies_to=("probe-p1-",))],
                      excluded=["wanderer"])
    rc, so, se, fig = run_coverage(pilot, "t-", other)
    check(rc == EXIT_UNCLASSIFIED and fig is None,
          "an entry scoped to another run leaves this one UNCLASSIFIED (%d)"
          % EXIT_UNCLASSIFIED, "rc=%d %s" % (rc, se.strip()[:250]))
    check("wanderer" in se, "and the refusal names it", se[:250])

    both = write_set(pilot / "both.json",
                     [entry("wanderer", applies_to=("probe-p1-", "t-"))])
    rc, so, se, fig = run_coverage(pilot, "t-", both)
    check(rc == 0 and bool(fig) and fig["denominator"] == 1,
          "and widening the scope to cover this run classifies it", se[:250])

    ambig = write_set(pilot / "ambig.json", [
        entry("wanderer", cls="instrument", applies_to=("t-",)),
        entry("wanderer", cls="environment", applies_to=("t-w",))])
    rc, so, se, fig = run_coverage(pilot, "t-", ambig)
    check(rc == EXIT_NO_FAILSET and "read order" in se,
          "two entries that both apply to one run refuse rather than tie-break",
          "rc=%d %s" % (rc, se.strip()[:250]))

    e = entry("wanderer")
    del e["evidence_tier"]
    rc, _, se, fig = run_coverage(pilot, "t-", write_set(pilot / "notier.json", [e]))
    check(rc == EXIT_NO_FAILSET and "evidence_tier" in se,
          "an entry that does not say whether its mechanism was reproduced or "
          "argued refuses", "rc=%d %s" % (rc, se[:200]))


def test_chain_coverage_line_needs_the_set(tmp: Path):
    head("§20.1 chain coverage — the headline line, and it moves with evidence")

    pilot = tmp / "chain"
    pilot.mkdir()
    make_run(pilot, "full")                      # hooks, execs, real tree
    make_run(pilot, "nohooks", hooks=False)      # folded, but no hook events
    make_run(pilot, "noexec", execs=False)       # no agent commands recorded
    make_run(pilot, "emptytree",
             tree_hash="4b825dc642cb6eb9a060e54bf8d69288fbee4904")

    rc, so, se, fig = run_coverage(pilot, "t-", write_set(pilot / "s.json", []))
    check(rc == 0, "emits", se[:300])
    if not fig:
        return
    cc = fig["chain_coverage"]
    check(cc["denominator"] == 4, "denominator 4", str(cc))
    check(cc["hooks_folded"] == 3, "hooks folded 3 of 4 — one run has no hook "
          "events and it LOWERS the figure", str(cc))
    check(cc["agent_commands_recorded"] == 3, "agent commands 3 of 4", str(cc))
    check(cc["snapshots_bound"] == 3,
          "snapshots bound 3 of 4 — git's EMPTY TREE is not a binding", str(cc))
    check(cc["all_three"] == 1,
          "and all three together is 1 of 4: the conjunction, not a mean",
          str(cc))
    check("CHAIN COVERAGE" in so and "snapshot bound to a real tree" in so,
          "and the printed report carries the line", so[:200])

    # THE WIRING, STATED AS A TEST: the chain-coverage line cannot be reached
    # without the set. Same records, no set.
    rc2, so2, se2, fig2 = run_coverage(pilot, "t-", pilot / "absent.json")
    check(rc2 == EXIT_NO_FAILSET and "CHAIN" not in so2 and fig2 is None,
          "and with no instrument-failures set there is NO chain-coverage line",
          "rc=%d out=%r" % (rc2, so2[:120]))


def test_task_named_from_the_mount_when_the_harness_never_ran(tmp: Path):
    head("naming — a run that died before Terminal-Bench can still be classified")

    # Six of the m1 measurement's 123 runs have no `runs-` directory at all,
    # and the report refused the WHOLE measurement because it could not name
    # them (exit 7, "6 run(s) name no task"). The runner writes
    # mount-<rid>/task-provenance.txt at staging time; that is the record.
    pilot = tmp / "naming"
    pilot.mkdir()
    make_run(pilot, "alpha")
    make_run(pilot, "died-early", started=False, spool=False, exec_db=False,
             instrumented=False, results=False, provenance="died-early")

    rc, so, se, fig = run_coverage(pilot, "t-", write_set(pilot / "s.json", []))
    check(rc == EXIT_UNCLASSIFIED and "died-early" in se,
          "the task is NAMED from the mount and then refused as unclassified — "
          "not refused as unnameable", "rc=%d %s" % (rc, se.strip()[:250]))

    ok = write_set(pilot / "ok.json", [entry("died-early", stage="instrumented")])
    rc, so, se, fig = run_coverage(pilot, "t-", ok)
    check(rc == 0 and bool(fig) and fig["denominator"] == 1,
          "and once classified the measurement proceeds", se[:250])

    # Without the provenance file there is nothing to name it with, and the
    # old refusal is still the right one.
    bare = tmp / "bare"
    bare.mkdir()
    make_run(bare, "alpha")
    make_run(bare, "anonymous", started=False, spool=False, exec_db=False,
             instrumented=False, results=False)
    rc, so, se, fig = run_coverage(bare, "t-", write_set(bare / "s.json", []))
    check(rc == EXIT_UNCLASSIFIED and "name no task" in se,
          "and a run with NO naming record anywhere still refuses",
          "rc=%d %s" % (rc, se.strip()[:250]))


def test_service_user_has_a_consumer(tmp: Path):
    head("the compose receipt's `service_user` finally has a reader")

    pilot = tmp / "svcuser"
    pilot.mkdir()
    make_run(pilot, "alpha")
    make_run(pilot, "beta")
    make_run(pilot, "privilege-ish", service_user="alice")
    # A service-user exclusion is an exclusion, so it must be attested to like
    # any other. That is requirement 2: EVERY exclusion accounted for, not just
    # the ones the failset happens to name.
    rc, so, se, fig = run_coverage(
        pilot, "t-", write_set(pilot / "s.json", [], excluded=["privilege-ish"]))
    check(rc == 0, "emits", se[:200])
    if not fig:
        return
    check(fig["denominator"] == 2,
          "a run under an overridden service user leaves the denominator",
          "got %d" % fig["denominator"])
    check([x["task"] for x in fig["service_user_exclusions"]] == ["privilege-ish"],
          "and is reported by name with the user it ran as",
          str(fig["service_user_exclusions"]))
    check("service-user override" in so, "and appears in the printed report")


# --------------------------------------------------------------------------
# 3. PER-TASK MODE — the same rule on the path scripts/pilot-run.sh takes.
# --------------------------------------------------------------------------

def run_per_task(pilot: Path, rid, task, agent="oracle", failset=None):
    mount = pilot / ("mount-" + rid)
    out = pilot / ("runs-" + rid)
    env = {"PATH": "/usr/bin:/bin", "HOME": str(pilot),
           "MOUNT": str(mount), "OUT": str(out), "RUN_ID": rid,
           "TASK_ID": task, "AGENT": agent, "PAYS": "0"}
    if failset is not None:
        env["THRULINE_INSTRUMENT_FAILURES"] = str(failset)
    p = subprocess.run([PY, str(TOOL)], capture_output=True, text=True, env=env)
    return p.returncode, p.stdout, p.stderr


def test_the_label_is_decided_by_the_denominator_not_by_the_disk(tmp: Path):
    """FAIL-OPEN, demonstrated and closed: `reach_only` iterated every run.

    `reach_only` was `all(... for r in runs)` — every run DISCOVERED — while
    the figure it labels is computed over `counted`. So one EXCLUDED run
    carrying a non-oracle `agent_name` was enough to make an all-oracle
    denominator answer to the word `coverage`: rc 8 REFUSED became rc 0 with a
    `COVERAGE REPORT` printed and `"label": "coverage"` in the JSON, over two
    oracle runs and nothing else. §1.1 is a refusal, not a disclaimer, and this
    walked straight through it.

    Both halves are asserted, because only the pair proves anything: the
    denominator is byte-identical across the two runs below, and ONLY the
    excluded run's agent name changes.
    """
    head("the label answers to the denominator, not to what is on disk")

    pilot = tmp / "labelset"
    pilot.mkdir()
    for t in ("alpha", "beta"):
        make_run(pilot, t, agent="oracle")
    make_run(pilot, "gamma", agent="oracle")
    fs = write_set(pilot / "set.json", [entry("gamma")], excluded=["gamma"])

    rc, so, se, fig = run_coverage(pilot, "t-", fs, extra=["--as-coverage"])
    check(rc == EXIT_VOCABULARY and fig is None,
          "all-oracle denominator refuses `--as-coverage` with %d"
          % EXIT_VOCABULARY, "rc=%d %s" % (rc, se.strip()[:200]))

    # The ONLY change: the EXCLUDED run's agent name. Nothing in the
    # denominator moves — alpha and beta are still the two counted oracle runs.
    meta = pilot / "runs-t-gamma" / "t-gamma" / "run_metadata.json"
    m = json.loads(meta.read_text())
    m["agent_name"] = "claude-code"
    meta.write_text(json.dumps(m))

    rc2, so2, se2, fig2 = run_coverage(pilot, "t-", fs, extra=["--as-coverage"])
    check(rc2 == EXIT_VOCABULARY,
          "and STILL refuses when only the EXCLUDED run's agent name changes",
          "rc=%d stdout=%s" % (rc2, so2.strip()[:200]))
    check(fig2 is None and "COVERAGE REPORT" not in so2,
          "and emits no coverage figure on that flip",
          (so2.strip()[:200] or "(no stdout)"))
    check(fig2 is None or fig2.get("label") != "coverage",
          "and never writes `label: coverage` for an all-oracle denominator",
          json.dumps(fig2)[:200] if fig2 else "(no figure)")

    # Positive control: a real agent IN THE DENOMINATOR does license the word.
    # Without this the assertions above are satisfied by refusing everything.
    p2 = tmp / "labelset-real"
    p2.mkdir()
    make_run(p2, "alpha", agent="claude-code")
    make_run(p2, "beta", agent="claude-code")
    make_run(p2, "gamma", agent="oracle")
    fs2 = write_set(p2 / "set.json", [entry("gamma")], excluded=["gamma"])
    rc3, so3, se3, fig3 = run_coverage(p2, "t-", fs2, extra=["--as-coverage"])
    check(rc3 == 0 and fig3 is not None and fig3.get("label") == "coverage",
          "and a real agent in the denominator IS labelled coverage",
          "rc=%d %s" % (rc3, se3.strip()[:200]))


def test_an_unreadable_execution_store_is_an_empty_capture(tmp: Path):
    """FAIL-OPEN, demonstrated and closed: the predicate was file existence.

    The empty-capture predicate was `not r["exec_db"]` — `Path.exists()`. An
    `exec.db` that is 0-byte or corrupt therefore returned all-zeros from
    `_chain_measures` and was COUNTED: rc 0, no warning, three zeros in the
    numerators of a figure whose denominator still held the run. That is
    precisely the §20 bias — the instrument's own damage read as the product
    under-performing — occurring inside the guard written to stop it.

    `_chain_measures` had a `folded` flag for this and the flag was READ AT NO
    SITE, while the comment above it asserted a routing mechanism that did not
    exist. It is the predicate now.
    """
    head("a store that cannot be read is not a store full of zeros")

    for kind, mangle in (
            ("zero-byte", lambda db: db.write_bytes(b"")),
            ("corrupt", lambda db: db.write_bytes(b"SQLite format 3\x00" + b"\xff" * 512)),
    ):
        pilot = tmp / ("capture-" + kind)
        pilot.mkdir()
        for t in ("alpha", "beta"):
            make_run(pilot, t, agent="claude-code")
        make_run(pilot, "broken", agent="claude-code")
        mangle(pilot / "mount-t-broken" / "exec.db")

        # Unclassified: it must REFUSE rather than average the zeros in.
        fs = write_set(pilot / "set.json", [], excluded=[])
        rc, so, se, fig = run_coverage(pilot, "t-", fs)
        check(rc == EXIT_UNCLASSIFIED and fig is None,
              "a %s exec.db refuses as an unclassified empty capture (%d)"
              % (kind, EXIT_UNCLASSIFIED),
              "rc=%d %s" % (rc, se.strip()[:240]))
        check("broken" in se and "captured nothing" in se,
              "and names the run that captured nothing", se.strip()[:240])
        check("exec.db=True" in se,
              "and says the FILE WAS THERE — which is why existence could never "
              "have been the predicate", se.strip()[:240])

        # Classified: it is excluded with its mechanism, never counted.
        fs2 = write_set(pilot / "set2.json", [entry("broken")],
                        excluded=["broken"])
        rc2, so2, se2, fig2 = run_coverage(pilot, "t-", fs2)
        check(rc2 == 0 and fig2 is not None,
              "and once classified the report emits", "rc=%d %s" % (rc2, se2[:200]))
        check(fig2 and fig2["denominator"] == 2,
              "with the broken run OUT of the denominator",
              json.dumps(fig2.get("denominator") if fig2 else None))
        check(fig2 and fig2["chain_coverage"]["hooks_folded"] == 2,
              "and its zeros nowhere in the numerator",
              json.dumps(fig2.get("chain_coverage") if fig2 else None))


def test_per_task_mode(tmp: Path):
    head("per-task mode — the same refusal on pilot-run.sh's own path")

    pilot = tmp / "pertask"
    pilot.mkdir()
    rid = make_run(pilot, "alpha", agent="oracle")
    rc, so, se = run_per_task(pilot, rid, "alpha",
                              failset=pilot / "absent-set.json")
    check(rc == EXIT_NO_FAILSET, "an absent set refuses here too", "rc=%d" % rc)
    check("CAPTURE" not in so and "hook events" not in so,
          "and no per-task measure is printed", so[:200])

    s = write_set(pilot / "s.json", [])
    rc, so, se = run_per_task(pilot, rid, "alpha", failset=s)
    check(rc == 0, "a classified-clean task still reports", se[:200])
    check("INSTRUMENT REACH" in so, "and an oracle run is labelled instrument reach")
    check("CAPTURE COVERAGE" not in so,
          "and the word `coverage` is never applied to an oracle number", so[:400])

    rid2 = make_run(pilot, "shell-deobfuscation", started=False, spool=False,
                    exec_db=False, graded=False, resolved=False, agent="oracle")
    s2 = write_set(pilot / "s2.json", [entry("shell-deobfuscation")])
    rc, so, se = run_per_task(pilot, rid2, "shell-deobfuscation", failset=s2)
    check(rc == EXIT_EXCLUDED_TASK,
          "a known instrument failure refuses with %d" % EXIT_EXCLUDED_TASK,
          "rc=%d" % rc)
    check("NOT MEASURED" in so and "mechanism" in so,
          "and prints its mechanism instead of a number", so[:300])
    check("hook events" not in so and "shim exec events" not in so,
          "and prints NO zeros for it — that zero is the instrument's damage",
          so[:300])


def test_committed_default_set_is_valid(tmp: Path):
    head("the committed default set loads and validates")

    check(DEFAULT_SET.exists(), "scripts/instrument-failures.json exists")
    doc = json.loads(DEFAULT_SET.read_text())

    # 1. It loads and validates on the default path — every entry-level rule
    #    above runs before the attestation lookup, so reaching that lookup is
    #    proof the whole file parsed and passed.
    pilot = tmp / "default"
    pilot.mkdir()
    make_run(pilot, "alpha")
    rc, so, se, fig = run_coverage(pilot, "t-", None)
    check(rc == EXIT_NO_ATTESTATION and fig is None,
          "the generator loads it with no path given, and then refuses a run "
          "prefix it never attested to (%d)" % EXIT_NO_ATTESTATION,
          "rc=%d %s" % (rc, se.strip()[:300]))
    check("m1-" in se and "probe-p1-" in se,
          "and lists the measurements it DOES cover", se[:400])

    # 2. And it is exercised END TO END against a reconstruction of the probe's
    #    own run shapes: 10 runs, the 4 attested exclusions, 6 counted.
    probe = tmp / "probe"
    probe.mkdir()
    att = [a for a in doc["attestations"] if a["run_prefix"] == "probe-p1-"][0]
    healthy = ["bank-trans-filter", "broken-python", "cprofiling-python",
               "ode-solver-rk4", "port-compressor", "solve-sudoku"]
    for t in healthy:
        make_run(probe, t, prefix="probe-p1-", agent="oracle")
    # `lean4-proof` has a COMPLETE capture in the probe and is excluded by its
    # classification alone — the case a mount-shape heuristic cannot see.
    make_run(probe, "lean4-proof", prefix="probe-p1-", agent="oracle",
             graded=False, resolved=False)
    for t in ("privilege-escalation", "shell-deobfuscation",
              "spring-messaging-vul"):
        make_run(probe, t, prefix="probe-p1-", agent="oracle", started=False,
                 spool=False, exec_db=False, graded=False, resolved=False)
    rc, so, se, fig = run_coverage(probe, "probe-p1-", None)
    check(rc == 0, "and it emits against the probe's own 10 run shapes", se[:400])
    if fig:
        check(fig["denominator"] == 6,
              "6 counted, 4 excluded — exactly the committed attestation",
              "got %d" % fig["denominator"])
        check(sorted(t for lst in fig["exclusions"].values()
                     for t in [x["task"] for x in lst]) == sorted(att["excluded"]),
              "and the exclusions it computes ARE the ones attested to",
              str(fig["exclusions"]))
        check(fig["label"] == "instrument-reach",
              "and an all-oracle probe is labelled instrument reach, never coverage")

    # 3. THE DENOMINATOR docs/PILOT-REPORT.md's correction rests on, pinned
    #    here rather than transcribed there. The published chain-coverage
    #    figure was 64/99; the corrected one is 65/100, and the whole of the
    #    difference is whether `lean4-proof` is excluded from `m1-`. The
    #    committed set answers that, and this is the check that keeps the
    #    report's arithmetic and the file from drifting apart.
    m1 = [a for a in doc["attestations"] if a["run_prefix"] == "m1-"]
    check(len(m1) == 1, "the committed set attests to the m1 measurement")
    if m1:
        check(len(m1[0]["excluded"]) == 23,
              "and names exactly 23 m1 exclusions — 123 discovered - 23 = a "
              "denominator of 100, which is the corrected figure's",
              "got %d: %s" % (len(m1[0]["excluded"]), sorted(m1[0]["excluded"])))
        check("lean4-proof" not in m1[0]["excluded"],
              "and does NOT exclude `lean4-proof` from m1 — the one task "
              "between 64/99 and 65/100")
    l4 = [e for e in doc["entries"] if e["task"] == "lean4-proof"]
    check(l4 and all(e["applies_to"] == ["probe-p1-"] for e in l4),
          "and scopes the `lean4-proof` entry to the PROBE only, so widening it "
          "to the whole `m1-` prefix is not something this file says",
          str([e["applies_to"] for e in l4]))


# --------------------------------------------------------------------------
# 6. THE PREDICATES ARE PINNED, AND THE PIN IS WATCHED REFUSING.
#
# The m1 chain-coverage figure reads 65 of 100 or 74 of 100 depending on nothing
# but which predicate you mean by "recorded" and "bound" — a 9-point swing with
# no evidence behind it. Both are re-derived from the same 100 exec.db files.
# So the predicates are a pinned artefact, and an edit to one must be a
# deliberate re-measurement, not a diff nobody notices.
# --------------------------------------------------------------------------

# The digest of the predicate table, written HERE as a literal as well as in the
# tool. Moving a predicate silently now takes an edit in three places.
PINNED_PREDICATES_SHA256 = \
    "cebbb743a3638aafcec2e147b2f0a428808437f0fea29e2838106cbadafdcd15"
EXIT_PREDICATE_DRIFT = 13
EMPTY_TREE = "4b825dc642cb6eb9a060e54bf8d69288fbee4904"


def test_predicates_are_pinned(tmp: Path):
    head("the chain-coverage predicates are pinned, and drift REFUSES")
    pilot = tmp / "pinned"
    pilot.mkdir()
    make_run(pilot, "good")
    fs = write_set(pilot / "set.json", [])
    rc, so, se, fig = run_coverage(pilot, "t-", fs)
    check(rc == 0 and fig is not None, "a pinned run emits", se)

    cc = (fig or {}).get("chain_coverage", {})
    check(cc.get("predicates_sha256") == PINNED_PREDICATES_SHA256,
          "the emitted figure carries the pinned digest, and it is the one this "
          "suite names",
          "figure says %r, suite says %r"
          % (cc.get("predicates_sha256"), PINNED_PREDICATES_SHA256))
    check(cc.get("reading") == "pinned"
          and set(cc.get("predicates") or {}) ==
          {"hooks_folded", "agent_commands", "snapshot_bound"},
          "and names its reading and all three predicates, as SQL, in the figure")
    check("<> :empty" in (cc.get("predicates") or {}).get("snapshot_bound", ""),
          "and the empty-tree exclusion is IN the pinned predicate text, not in "
          "a footnote beside it")

    # DRIFT, WATCHED. A copy of the tool with one predicate widened — the exact
    # edit that turns 65 into 74 — and the pin left alone.
    drifted = pilot / "drifted-report.py"
    src = TOOL.read_text(encoding="utf-8")
    needle = ('"pinned": "select count(*) from events where '
              "source = 'exec-recorder' \"\n"
              '                  "and kind = \'exec\'",')
    check(needle in src, "the component-2 pinned predicate is where the drift "
                         "test expects it")
    drifted.write_text(src.replace(
        needle,
        '"pinned": "select count(*) from events where '
        "source = 'exec-recorder' \"\n"
        '                  "or kind = \'tool_call\'",'), encoding="utf-8")
    out = pilot / "drift.json"
    p = subprocess.run(
        [PY, str(drifted), "--coverage", "--pilot-dir", str(pilot),
         "--run-prefix", "t-", "--instrument-failures", str(fs),
         "--json", str(out)],
        capture_output=True, text=True, env={"PATH": "/usr/bin:/bin",
                                             "HOME": str(pilot)})
    check(p.returncode == EXIT_PREDICATE_DRIFT,
          "widening a predicate without moving the pin exits %d"
          % EXIT_PREDICATE_DRIFT,
          "rc=%d\n%s" % (p.returncode, p.stderr[:400]))
    check(not out.exists(),
          "and emits NO figure — the refusal is not a warning printed beside a "
          "number")
    check("predicates have changed" in p.stderr,
          "and says so, naming both digests", p.stderr[:200])


def test_the_two_readings_are_measured_side_by_side(tmp: Path):
    head("both readings are emitted, and they are allowed to disagree")
    pilot = tmp / "readings"
    pilot.mkdir()
    # A run shaped like the NINE m1 tasks that separate 65 from 74: the agent's
    # command arrives only as a hook `tool_call`, and the only real tree hash is
    # in a late `snapshot` event while every `checks` row holds the empty tree.
    make_run(pilot, "late", execs=False, tool_calls=True,
             tree_hash=EMPTY_TREE, snapshot_tree="d" * 40)
    fs = write_set(pilot / "set.json", [])
    rc, so, se, fig = run_coverage(pilot, "t-", fs)
    check(rc == 0 and fig is not None, "it emits", se)

    cc = (fig or {}).get("chain_coverage", {})
    alt = (fig or {}).get("chain_coverage_alternate", {})
    check(cc.get("all_three") == 0,
          "the PINNED reading counts it as a miss: no `exec`, and every check "
          "on the empty tree",
          "pinned all_three=%r" % cc.get("all_three"))
    check(alt.get("all_three") == 1,
          "the ALTERNATE reading counts the same run as a hit",
          "alternate all_three=%r" % alt.get("all_three"))
    check(cc.get("denominator") == alt.get("denominator") == 1,
          "over the SAME denominator — the gap is the predicate and nothing else")
    check("ALL THREE: " in so and "PREDICATES, PINNED" in so,
          "and the printed report shows both, with the pinned SQL beside them",
          so[-600:])
    check(alt.get("reading") == "alternate" and "predicates" in alt,
          "and the alternate carries its own predicate text, so neither reading "
          "can be quoted without saying which it is")


def test_the_empty_tree_is_refused_by_BOTH_readings(tmp: Path):
    head("the empty tree binds nothing, under either reading")
    pilot = tmp / "emptytree"
    pilot.mkdir()
    # Everything present and healthy EXCEPT that every tree hash on the record
    # is git's empty tree. This is the 93-of-100 reading, refused.
    make_run(pilot, "hollow", tree_hash=EMPTY_TREE, snapshot_tree=EMPTY_TREE)
    fs = write_set(pilot / "set.json", [])
    rc, so, se, fig = run_coverage(pilot, "t-", fs)
    check(rc == 0, "it emits", se)
    cc = (fig or {}).get("chain_coverage", {})
    alt = (fig or {}).get("chain_coverage_alternate", {})
    check(cc.get("snapshots_bound") == 0 and alt.get("snapshots_bound") == 0,
          "40 legal hex characters meaning `no files` are BOUND under neither "
          "reading",
          "pinned=%r alternate=%r"
          % (cc.get("snapshots_bound"), alt.get("snapshots_bound")))
    check(cc.get("hooks_folded") == 1,
          "and the run is otherwise healthy, so this is the tree hash being "
          "refused and not the run")
    # And a real tree flips it, so the assertion above is not satisfied by a
    # component that never counts anything.
    pilot2 = tmp / "realtree"
    pilot2.mkdir()
    make_run(pilot2, "hollow", tree_hash="e" * 40, snapshot_tree="e" * 40)
    fs2 = write_set(pilot2 / "set.json", [])
    rc2, _, se2, fig2 = run_coverage(pilot2, "t-", fs2)
    check(rc2 == 0 and (fig2 or {}).get("chain_coverage", {}).get(
              "snapshots_bound") == 1,
          "a REAL tree on the same shape binds — the empty-tree refusal is not "
          "a component that never counts", se2)


def main():
    # `--tool` exists so the suite can be pointed at the PRE-CHANGE script and
    # watched failing. A guard that has only ever been seen passing is a guard
    # nobody has seen refuse; this program has shipped nine of those.
    global TOOL
    argv = sys.argv[1:]
    if len(argv) == 2 and argv[0] == "--tool":
        TOOL = Path(argv[1]).resolve()
    elif argv:
        print("usage: pilot-report-coverage-test.py [--tool PATH]", file=sys.stderr)
        return 2
    print("pilot-report coverage guards — %s" % TOOL)
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        test_refusals(tmp)
        test_monotonicity_with_paired_negative(tmp)
        test_positive_evidence_counts(tmp)
        test_numerator_tracks_evidence(tmp)
        test_three_denominators_are_separate(tmp)
        test_empty_set_is_not_an_unread_set(tmp)
        test_attestation_must_be_complete_for_this_run(tmp)
        test_scope_is_per_run_not_per_task(tmp)
        test_chain_coverage_line_needs_the_set(tmp)
        test_task_named_from_the_mount_when_the_harness_never_ran(tmp)
        test_service_user_has_a_consumer(tmp)
        test_the_label_is_decided_by_the_denominator_not_by_the_disk(tmp)
        test_an_unreadable_execution_store_is_an_empty_capture(tmp)
        test_per_task_mode(tmp)
        test_committed_default_set_is_valid(tmp)
        test_predicates_are_pinned(tmp)
        test_the_two_readings_are_measured_side_by_side(tmp)
        test_the_empty_tree_is_refused_by_BOTH_readings(tmp)

    print("\n%d checks, %d failures" % (CHECKS, len(FAILURES)))
    for f in FAILURES:
        print("  FAILED: %s" % f)
    return 1 if FAILURES else 0


if __name__ == "__main__":
    sys.exit(main())
