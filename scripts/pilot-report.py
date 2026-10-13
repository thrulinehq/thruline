#!/usr/bin/env python3
"""BUILD-SPEC §12 pilot report — reads the EXECUTION RECORD, not the summary.

Invoked by scripts/pilot-run.sh. Prints the three capture-coverage measures
separately, the tree-before-first-action condition, the measured per-task cost,
and the terminal state.

Two things this deliberately does NOT do:

  * It never reads `auth.json`. The runner mounts `/root/.codex/sessions`
    rather than `/root/.codex`, so the file the harness writes the key into is
    not on the host at all — but this only ever opens `rollout-*.jsonl`, so the
    exclusion holds even if that mount is widened later.
  * It never estimates. A cost with no rollout behind it is reported as
    "not measured" with a reason, because a number that looks measured and is
    not is worse than an absent one.
"""
import hashlib
import json
import os
import pathlib
import re
import sys

# ==========================================================================
# THE INSTRUMENT-FAILURES SET, AND ITS CONSUMER (operator Ruling 6, §12/§20.3)
#
# Before this block, Ruling 6 existed in this repository as prose and as a
# receipt field nothing read: `grep -rn service_user scripts/ crates/` found
# only the writer. A rule nothing enforces is a comment.
#
# The rule: no figure may be emitted unless the set has been READ. Not warned
# about — refused, non-zero, naming what is missing. The precedent for taking
# that literally is forty lines below: this same script once PRINTED that a
# missing usage record "HALTS the run" while exiting 0.
#
# WHY A FILE AND NOT A HEURISTIC. The signature of a task the instrument broke
# is: no READY sentinel, no exec.db, empty spool. Measured on the 10-task probe,
# `spring-messaging-vul` has that signature byte for byte — and its cause is
# that host port 8080 is held by an unrelated container. It is not an instrument
# failure and not a ThruLine result. Nothing derivable from the mount can tell
# the three cases apart, so the classification is an artefact a human writes and
# this script reads. Where a task's capture is empty and the set does not
# classify it, this refuses rather than guessing a class — see EXIT_UNCLASSIFIED.
# ==========================================================================

# ==========================================================================
# WHY "THE FILE EXISTS" IS NOT THE PREDICATE (the hole this block closes)
#
# The first version of the rule above was keyed on the set being READABLE. A
# readable set that says `"entries": []` satisfies it, and so does a set derived
# for a different measurement. Both were satisfied on 2026-09-04, and the
# published 64/99 chain-coverage figure still did not come out of here: it came
# out of an ad-hoc aggregator with no reference to this file, whose four entries
# covered 3 of that measurement's 24 exclusions.
#
# So the predicate is not "read". It is READ AND COMPLETE FOR THIS RUN:
#
#   1. every entry names the RUNS it was derived from (`applies_to`), so an
#      entry cannot be borrowed across measurements by accident;
#   2. every task this report excludes must be classified by an entry that
#      applies to THIS run — otherwise EXIT_UNCLASSIFIED;
#   3. the set must carry an ATTESTATION for this run prefix, listing the
#      exclusions the human who wrote it saw. The report recomputes that list
#      and refuses if the two disagree.
#
# (3) is what separates "no instrument failures" from "I never looked".
# "I never looked" has no attestation row and exits 11. "I looked, and there
# were none" is an attestation row whose `excluded` list is empty — a positive
# claim somebody had to write down, dated, and which this script checks against
# the disk. An empty file can express the first and cannot express the second.
# ==========================================================================

FAILSET_SCHEMA = "thruline.instrument-failures/2"

# SIX CLASSES, NOT THREE. The first three were `instrument / environment /
# product`, and the 24 exclusions of the m1 measurement do not fit them: two
# tasks were REFUSED by compose-instrument rather than broken by it (the
# doctrine's `ceiling` — "a task the instrument cannot reach", PILOT-REPORT.md's
# vocabulary note), one task's own pinned stack no longer builds for reasons
# nothing on this machine caused, and two runs simply never finished. Forcing
# those into `environment` would have put a false sentence in the file — that
# something "outside both ThruLine and the task" broke them — in order to keep
# the class list short.
#
# Only `product` is COUNTED. Every other class is excluded and printed under its
# own heading with its mechanism, because a count of exclusions with no reasons
# is a count of excuses (§20.3).
FAILSET_CLASSES = (
    "instrument",     # ThruLine's instrumentation BROKE the task.
    "ceiling",        # ThruLine's instrumentation REFUSED the task. Not damage.
    "environment",    # something outside both ThruLine and the task (a held
                      # port, host DNS, a read timeout on a package index).
    "task-upstream",  # the task's own image/setup no longer builds, on its own
                      # terms, independent of ThruLine and of this host.
    "run-incomplete", # the run never finished; there is no outcome to attribute
                      # to anyone. Named rather than silently dropped.
    "product",        # the capture is empty and that IS a ThruLine result.
                      # The only class that stays in the denominator.
)
COUNTED_CLASSES = ("product",)
STAGES = ("instrumented", "started", "graded")

# `reproduced` — the mechanism was re-derived from the record (or re-run) by
# whoever wrote the entry. `argued` — it is the best available reading and was
# NOT reproduced. The two must not share a representation either: an argued
# mechanism reads exactly like a reproduced one once it is prose.
EVIDENCE_TIERS = ("reproduced", "argued")

# Distinct codes, so a caller can tell the refusals apart from each other and
# from a Python traceback (1) and from the §12 cost halt (5).
EXIT_NO_FAILSET = 6        # the set could not be read, or is malformed
EXIT_UNCLASSIFIED = 7      # an exclusion the set does not classify for this run
EXIT_VOCABULARY = 8        # a coverage label demanded for oracle-run numbers
EXIT_NO_DENOMINATOR = 9    # nothing measurable left to divide by
EXIT_NO_ATTESTATION = 11   # the set was never read FOR THIS RUN
EXIT_ATTESTATION_STALE = 12  # it was, and it disagrees with the disk
EXIT_PREDICATE_DRIFT = 13  # a coverage predicate changed and the pin did not


class Refused(Exception):
    """A refusal to emit. Carries the exit code the caller must see."""

    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def refuse(code, message):
    print("pilot-report: REFUSED — %s" % message, file=sys.stderr)
    return code


def failset_path(explicit=None):
    if explicit:
        return pathlib.Path(explicit)
    env = os.environ.get("THRULINE_INSTRUMENT_FAILURES")
    if env:
        return pathlib.Path(env)
    return pathlib.Path(__file__).resolve().parent / "instrument-failures.json"


def load_failset(explicit=None):
    """Read and validate the instrument-failures set, or refuse.

    Every validation here has a failure it exists because of. An entry with no
    mechanism is the one worth naming: `instrument_failures: 3` with three bare
    task names is a count of excuses, and the reporting form §20.3 fixes is
    "each with its task name AND mechanism". A mechanism nobody had to write
    down is a task somebody wanted out of the denominator.
    """
    p = failset_path(explicit)
    if not p.exists():
        raise Refused(
            EXIT_NO_FAILSET,
            "no instrument-failures set at %s.\n"
            "  No coverage figure may be emitted without one: a task the "
            "instrument broke\n"
            "  produces an EMPTY spool, and an empty spool inside the "
            "denominator is a ZERO\n"
            "  that reports the instrument's own damage as the product "
            "under-performing.\n"
            "  Point --instrument-failures or $THRULINE_INSTRUMENT_FAILURES at "
            "the set." % p)
    try:
        raw = p.read_text(encoding="utf-8")
    except OSError as e:
        raise Refused(EXIT_NO_FAILSET,
                      "cannot read the instrument-failures set %s: %s" % (p, e))
    try:
        doc = json.loads(raw)
    except ValueError as e:
        raise Refused(EXIT_NO_FAILSET,
                      "the instrument-failures set %s is not JSON: %s" % (p, e))
    if not isinstance(doc, dict) or doc.get("schema") != FAILSET_SCHEMA:
        raise Refused(
            EXIT_NO_FAILSET,
            "the instrument-failures set %s does not carry schema %r "
            "(found %r)." % (p, FAILSET_SCHEMA, isinstance(doc, dict) and doc.get("schema")))
    entries = doc.get("entries")
    if not isinstance(entries, list):
        raise Refused(EXIT_NO_FAILSET,
                      "the instrument-failures set %s has no `entries` list." % p)
    # ATTESTATIONS ARE REQUIRED, EVEN WHEN EMPTY-LOOKING. The key must be
    # present: a set with no `attestations` key is a set nobody ever pointed at
    # a measurement, and it must not be mistakable for one that covers this run.
    atts = doc.get("attestations")
    if not isinstance(atts, list):
        raise Refused(
            EXIT_NO_FAILSET,
            "the instrument-failures set %s has no `attestations` list.\n"
            "  Each attestation says: for run-id prefix P, these are ALL the "
            "tasks excluded,\n"
            "  and I checked. Without one, an empty `entries` list is "
            "indistinguishable from\n"
            "  a set nobody read against the run being reported." % p)
    attestations = {}
    for i, at in enumerate(atts):
        where = "%s attestation %d" % (p, i)
        if not isinstance(at, dict):
            raise Refused(EXIT_NO_FAILSET, "%s is not an object." % where)
        pref = at.get("run_prefix")
        if not isinstance(pref, str) or not pref:
            raise Refused(EXIT_NO_FAILSET, "%s names no `run_prefix`." % where)
        if pref in attestations:
            raise Refused(
                EXIT_NO_FAILSET,
                "%s repeats run_prefix %r. Two attestations for one run means "
                "the answer depends on read order." % (where, pref))
        excl = at.get("excluded")
        if not isinstance(excl, list) or not all(isinstance(t, str) for t in excl):
            raise Refused(
                EXIT_NO_FAILSET,
                "%s (run_prefix %r) has no `excluded` list of task names. An "
                "attestation with no list is the 'I never looked' case wearing "
                "the 'there were none' costume." % (where, pref))
        if len(set(excl)) != len(excl):
            raise Refused(EXIT_NO_FAILSET,
                          "%s (run_prefix %r) lists a task twice." % (where, pref))
        attestations[pref] = at

    by_task = {}
    for i, e in enumerate(entries):
        where = "%s entry %d" % (p, i)
        if not isinstance(e, dict):
            raise Refused(EXIT_NO_FAILSET, "%s is not an object." % where)
        task = e.get("task")
        if not isinstance(task, str) or not task.strip():
            raise Refused(EXIT_NO_FAILSET, "%s names no task." % where)
        # `applies_to` IS REQUIRED. Without it an entry is a claim about every
        # run that ever names the task, and that is how `lean4-proof` came to be
        # excluded from the m1 denominator on a mechanism — a `pytest` wrapper
        # answering `command -v` — that the m1 shim provably never wrote.
        scopes = e.get("applies_to")
        if not isinstance(scopes, list) or not scopes or \
                not all(isinstance(s, str) and s for s in scopes):
            raise Refused(
                EXIT_NO_FAILSET,
                "%s (task %r) has no `applies_to` list of run-id prefixes. A "
                "classification with no scope silently applies a mechanism "
                "measured in one run to every other run that names the task."
                % (where, task))
        prior = by_task.get(task, [])
        for q in prior:
            if sorted(q["applies_to"]) == sorted(scopes):
                raise Refused(
                    EXIT_NO_FAILSET,
                    "%s repeats task %r for the same scope %s. Two "
                    "classifications for one task in one run means the answer "
                    "depends on read order." % (where, task, scopes))
        cls = e.get("class")
        if cls not in FAILSET_CLASSES:
            raise Refused(
                EXIT_NO_FAILSET,
                "%s (task %r) has class %r; must be one of %s."
                % (where, task, cls, ", ".join(FAILSET_CLASSES)))
        stage = e.get("stage_lost")
        if stage not in STAGES:
            raise Refused(
                EXIT_NO_FAILSET,
                "%s (task %r) has stage_lost %r; must be one of %s — the three "
                "quantities of §1.1, which is what makes them countable "
                "separately." % (where, task, stage, ", ".join(STAGES)))
        mech = e.get("mechanism")
        if not isinstance(mech, str) or len(mech.split()) < 3:
            raise Refused(
                EXIT_NO_FAILSET,
                "%s (task %r) names no mechanism. §20.3 fixes the reporting form "
                "as the task name AND its mechanism; an exclusion whose reason "
                "nobody had to write down is a task removed from a denominator "
                "for convenience." % (where, task))
        tier = e.get("evidence_tier")
        if tier not in EVIDENCE_TIERS:
            raise Refused(
                EXIT_NO_FAILSET,
                "%s (task %r) has evidence_tier %r; must be one of %s. A "
                "mechanism that was argued and one that was reproduced read "
                "identically once they are prose, and the difference is the "
                "whole of whether the exclusion is evidence."
                % (where, task, tier, ", ".join(EVIDENCE_TIERS)))
        by_task.setdefault(task, []).append(e)
    return by_task, p, doc, attestations


def entry_for(by_task, task, run_id):
    """The one entry classifying `task` for `run_id`, or None.

    Scope is matched on the RUN ID, not on the task: `applies_to` holds run-id
    prefixes. Two entries matching one run is a refusal, not a tie-break — a
    tie-break here would pick a mechanism by read order.
    """
    hits = [e for e in by_task.get(task, [])
            if any(run_id.startswith(s) for s in e["applies_to"])]
    if len(hits) > 1:
        raise Refused(
            EXIT_NO_FAILSET,
            "task %r has %d classifications that all apply to run %r (%s). "
            "Which mechanism is true cannot depend on read order."
            % (task, len(hits), run_id,
               " / ".join(h["class"] for h in hits)))
    return hits[0] if hits else None


# --------------------------------------------------------------------------
# Reading one run's record off disk. Every predicate below is a separate fact
# with a separate source, because §1.1 forbids them sharing a denominator.
# --------------------------------------------------------------------------

def _spool_bytes(mount):
    total = 0
    sp = mount / "spool"
    if sp.is_dir():
        for f in sp.rglob("*"):
            if f.is_file():
                try:
                    total += f.stat().st_size
                except OSError:
                    pass
    return total


# git's canonical EMPTY tree, `git hash-object -t tree /dev/null`. A snapshot
# that binds only to this is bound to nothing: it is 40 legal hex characters
# that mean "no files". Counting it as bound reads the m1 measurement as 93 of
# 100 instead of 65 (re-derived 2026-09-04, not recalled); that reading is
# refused here and in docs/PILOT-REPORT.md F1.
EMPTY_TREE = "4b825dc642cb6eb9a060e54bf8d69288fbee4904"

# ==========================================================================
# THE PINNED PREDICATES (the hole this block closes)
#
# The chain-coverage figure moved NINE POINTS on definitions alone — 65 of 100
# under the predicates below, 74 of 100 under a looser reading of the same 100
# records — and nothing committed said which reading was the measurement. A
# figure whose predicate is unstated is not a measurement; it is a number with
# a plausible story attached, and the story is chosen after the fact.
#
# Worse, this one FILE carried three different readings of "bound to a tree":
#
#   * `_chain_measures` refused the empty tree                       -> 65/100
#   * the per-task path counted any non-empty string as bound        -> 93/100
#   * the ad-hoc aggregator that published 64.6% read snapshot EVENTS
#     rather than `checks`                                           -> 74/100
#
# All three printed a line with "bound" in it. So the predicates are pinned
# HERE, as text, once, and BOTH code paths execute them from this table rather
# than restating them. `CHAIN_PREDICATES_SHA256` is checked at run time: edit a
# predicate without updating the digest and every invocation refuses (exit 13)
# instead of quietly emitting a different figure under the same name.
#
# `alternate` is not a second opinion — it is the looser reading, computed and
# printed BESIDE the pinned one so the 9-point gap is a measured quantity in
# the output instead of an argument in a document. `pinned` is the figure; the
# gate is read against `pinned`.
#
# `:empty` binds to EMPTY_TREE. Every predicate is pure SQL on purpose: a
# predicate half in SQL and half in Python cannot be pinned as one string, and
# the Python half is exactly where the empty-tree rule went missing before.
# ==========================================================================

_HEX40 = "length(%(c)s) = 40 and %(c)s not glob '*[^0-9a-f]*'"
_TREE_JSON = "json_extract(payload, '$.tree_hash')"

CHAIN_PREDICATES = {
    "hooks_folded": {
        "component": 1,
        "name": "hooks folded into the chain",
        "pinned": "select count(*) from events where source like '%-hook'",
        "alternate": "select count(*) from events where source like '%-hook'",
        "means": "at least one event whose source ends in `-hook` reached the "
                 "store. NOT a chain recomputation — see the caveat printed "
                 "under the figure.",
        "differs": "no. Both readings agree; the component is pinned anyway so "
                   "that agreeing today is a recorded fact, not an assumption.",
    },
    "agent_commands": {
        "component": 2,
        "name": "agent commands recorded",
        "pinned": "select count(*) from events where source = 'exec-recorder' "
                  "and kind = 'exec'",
        "alternate": "select count(*) from events where "
                     "(source = 'exec-recorder' and kind = 'exec') "
                     "or (source like '%-hook' and kind = 'tool_call')",
        "means": "the PATH shim observed the command and the fold wrote it as "
                 "an `exec`. A hook's `tool_call` says the agent asked for a "
                 "command; only an `exec` says the command was observed "
                 "running. The pinned reading counts the latter.",
        "differs": "yes: 97 of 100 pinned, 100 of 100 alternate, on the m1 "
                   "records (2026-09-04).",
    },
    "snapshot_bound": {
        "component": 3,
        "name": "snapshot bound to a real tree",
        "pinned": "select count(*) from checks where " + (_HEX40 % {"c": "input_tree_hash"})
                  + " and input_tree_hash <> :empty",
        "alternate": "select count(*) from events where kind = 'snapshot' and "
                     + (_HEX40 % {"c": _TREE_JSON}) + " and " + _TREE_JSON
                     + " <> :empty",
        "means": "a CHECK — a test or build invocation whose verdict the "
                 "record has to support — carries the tree its inputs hashed "
                 "to. The alternate asks only whether the snapshotter ever saw "
                 "content at any point in the run, which binds no verdict to "
                 "anything.",
        "differs": "yes: 65 of 100 pinned, 74 of 100 alternate, on the m1 "
                   "records (2026-09-04). The 9 that differ hold a real tree "
                   "in a late `snapshot` event and the EMPTY tree on every "
                   "`checks` row — the content arrived after the checks ran.",
    },
}

# EMPTY-TREE HANDLING IS PART OF THE PREDICATE, NOT A FOOTNOTE. Both component-3
# readings exclude it explicitly. A third reading that accepts it exists and is
# refused: it reads 93 of 100, and the 40 hex characters it counts mean "no
# files".
CHAIN_EMPTY_TREE_RULE = "excluded from component 3 in BOTH readings"

# Bump deliberately when a predicate changes, and say why in the commit. The
# digest covers the SQL, the component numbers and the names — not the prose.
CHAIN_PREDICATES_SHA256 = \
    "cebbb743a3638aafcec2e147b2f0a428808437f0fea29e2838106cbadafdcd15"


def chain_predicates_digest():
    """SHA-256 over the load-bearing half of the predicate table."""
    canon = {
        k: {"component": v["component"], "name": v["name"],
            "pinned": v["pinned"], "alternate": v["alternate"]}
        for k, v in CHAIN_PREDICATES.items()
    }
    canon["_empty_tree"] = {"hash": EMPTY_TREE, "rule": CHAIN_EMPTY_TREE_RULE}
    return hashlib.sha256(
        json.dumps(canon, sort_keys=True, separators=(",", ":"))
        .encode("utf-8")).hexdigest()


def assert_predicates_pinned():
    """Refuse rather than emit a figure under a predicate nobody declared."""
    got = chain_predicates_digest()
    if got != CHAIN_PREDICATES_SHA256:
        raise Refused(
            EXIT_PREDICATE_DRIFT,
            "the chain-coverage predicates have changed and the pin has not.\n"
            "  pinned : %s\n"
            "  actual : %s\n"
            "  The m1 figure moved 65 -> 74 of the same 100 records on nothing "
            "but a redefinition\n"
            "  of `bound`. A predicate edit is a re-measurement: change "
            "CHAIN_PREDICATES_SHA256\n"
            "  deliberately, in the same commit, and say in the message which "
            "figure it moves."
            % (CHAIN_PREDICATES_SHA256, got))


def _chain_measures(mount):
    """The three §20.1 chain-coverage predicates, off one run's exec.db.

    THIS is the coverage line the gate is about — hooks folded, agent commands
    recorded, snapshots bound — and it is the line that got published without
    passing through the instrument-failures set at all. Computing it here, in
    the same function that already refuses, is the whole of the wiring.

    `folded` IS READ. It says "the execution store was opened and all three
    queries answered", and `coverage_main`'s empty-capture predicate is keyed on
    it. That was not true before: the comment below claimed the caller routed an
    unreadable store to the failset, and the caller's predicate was
    `not r["exec_db"]` — FILE EXISTENCE ONLY. A 0-byte or unreadable `exec.db`
    therefore returned all-zeros and was COUNTED: rc 0, no warning, three zeros
    contributed to the numerators of a figure whose denominator still held the
    run. That is the §20 bias running the wrong way inside the guard that exists
    to stop it, and a flag nothing reads is a comment, not a mechanism.

    `capture` names WHICH of the three no-number cases happened, so the refusal
    can say it instead of printing `exec.db=False` for all of them.
    """
    def nothing(capture):
        return {"hook_events": 0, "agent_execs": 0, "bound_checks": 0,
                "checks": 0, "folded": False, "capture": capture,
                "alt": {k: 0 for k in CHAIN_PREDICATES}}

    db = mount / "exec.db"
    if not db.exists():
        return nothing("absent")
    try:
        if db.stat().st_size == 0:
            # A 0-byte file is a VALID empty SQLite database. It opens, and
            # every query against it fails for a missing table — so this is
            # caught below too. It is named separately because "the fold
            # created the file and wrote nothing" is a different mechanism
            # from "the file is corrupt", and Ruling 6 is about mechanisms.
            return nothing("zero-byte")
    except OSError:
        return nothing("unstattable")
    import sqlite3
    try:
        con = sqlite3.connect("file:%s?mode=ro" % db, uri=True)
    except sqlite3.Error:
        return nothing("unopenable")
    try:
        # EVERY predicate comes out of CHAIN_PREDICATES. Restating one here is
        # how this file came to hold three readings of "bound" at once.
        pin = {k: con.execute(v["pinned"], {"empty": EMPTY_TREE}).fetchone()[0]
               for k, v in CHAIN_PREDICATES.items()}
        alt = {k: con.execute(v["alternate"], {"empty": EMPTY_TREE}).fetchone()[0]
               for k, v in CHAIN_PREDICATES.items()}
        n = con.execute("select count(*) from checks").fetchone()[0]
    except sqlite3.Error:
        # A store this cannot read is not a zero. Reported unfolded, and
        # `coverage_main` really does send it to the failset — see `empty`.
        return nothing("unreadable")
    finally:
        con.close()
    return {"hook_events": pin["hooks_folded"],
            "agent_execs": pin["agent_commands"],
            "bound_checks": pin["snapshot_bound"],
            "checks": n, "folded": True, "capture": "read", "alt": alt}


def read_run(pilot_dir, rid):
    """One run record: mount-<rid>, receipts-<rid>, runs-<rid>/<rid>."""
    mount = pilot_dir / ("mount-" + rid)
    receipts = pilot_dir / ("receipts-" + rid)
    rundir = pilot_dir / ("runs-" + rid) / rid

    task = agent = None
    task_source = None
    graded = False
    resolved = None
    failure_mode = None
    res = rundir / "results.json"
    if res.exists():
        try:
            d = json.loads(res.read_text(encoding="utf-8"))
        except ValueError:
            d = {}
        for r in (d.get("results") or []):
            if isinstance(r, dict):
                if not task and r.get("task_id"):
                    task, task_source = r["task_id"], "results.json"
                failure_mode = failure_mode or r.get("failure_mode")
                # GRADED means "the tests executed and produced a parsed
                # result" — a bool. `is_resolved: null` with
                # `failure_mode: parse_error` is lean4-proof: it started, and
                # nothing was graded.
                if isinstance(r.get("is_resolved"), bool):
                    graded = True
                    resolved = r["is_resolved"]
    meta = rundir / "run_metadata.json"
    if meta.exists():
        try:
            m = json.loads(meta.read_text(encoding="utf-8"))
        except ValueError:
            m = {}
        agent = m.get("agent_name")
        if not task:
            ids = m.get("task_ids")
            if isinstance(ids, list) and ids:
                task, task_source = ids[0], "run_metadata.json"

    # THE FOURTH NAMING SOURCE, AND WHY IT IS NOT A GUESS. Six of the m1
    # measurement's 123 runs died before Terminal-Bench was invoked, so they have
    # no run directory at all and this function returned `task: None` for them —
    # which made the report refuse the whole measurement (exit 7, "6 run(s) name
    # no task"). The runner writes `mount-<rid>/task-provenance.txt` at staging
    # time, holding `copied from <tasks-src>/<task-id>`; its basename IS the task
    # directory the run was built from. That is a record, not an inference from
    # the run id — and on all 123 m1 runs the two agree, which is how it was
    # checked rather than assumed.
    if not task:
        prov = mount / "task-provenance.txt"
        if prov.exists():
            try:
                first = prov.read_text(encoding="utf-8").splitlines()[:1]
            except OSError:
                first = []
            for line in first:
                line = line.strip()
                if line.startswith("copied from ") and len(line) > len("copied from "):
                    cand = pathlib.PurePosixPath(line[len("copied from "):]).name
                    if cand:
                        task, task_source = cand, "task-provenance.txt"

    # INSTRUMENTED — §1.1: "the patcher wrote a file Compose can parse", which
    # is exactly what a compose-instrument receipt attests, and nothing more.
    instrumented = False
    service_user = None
    receipt_file = None
    if receipts.is_dir():
        for rf in sorted(receipts.glob("*.json")):
            try:
                rj = json.loads(rf.read_text(encoding="utf-8"))
            except (ValueError, OSError):
                continue
            if not isinstance(rj, dict):
                continue
            if rj.get("schema") != "thruline.compose-instrument/1":
                continue
            receipt_file = rf.name
            if not task:
                task, task_source = rf.stem, "compose-instrument receipt"
            if rj.get("command_wrapped") and \
                    rj.get("sha256_before") != rj.get("sha256_after"):
                instrumented = True
            # THE FIELD'S CONSUMER. compose-instrument writes `service_user` and
            # says on stderr that such a run "measures INSTRUMENT REACH; it is
            # not a scored pass for a task whose subject is privilege". Until
            # now nothing read it back. A run under an overridden service user
            # cannot sit in a coverage denominator.
            if rj.get("service_user") is not None:
                service_user = rj.get("service_user")
            break

    chain = _chain_measures(mount)
    return {
        "run_id": rid,
        "task": task,
        "task_source": task_source,
        "agent": agent,
        "instrumented": instrumented,
        # The three §20.1 chain-coverage predicates, per run.
        "hooks_folded": chain["hook_events"] > 0,
        "agent_commands": chain["agent_execs"] > 0,
        "snapshot_bound": chain["bound_checks"] > 0,
        # The SAME three components under CHAIN_PREDICATES' looser reading,
        # carried per-run so the contrast is measured, never asserted.
        "alt_hooks_folded": chain["alt"]["hooks_folded"] > 0,
        "alt_agent_commands": chain["alt"]["agent_commands"] > 0,
        "alt_snapshot_bound": chain["alt"]["snapshot_bound"] > 0,
        "chain": chain,
        # STARTED — the sentinel `echo instrumented > /thruline/READY` that
        # compose-instrument appends immediately before `sleep infinity`.
        "started": (mount / "READY").exists(),
        "graded": graded,
        "resolved": resolved,
        "failure_mode": failure_mode,
        "spool_bytes": _spool_bytes(mount),
        # `exec_db` is FILE EXISTENCE and is retained only for the report line.
        # It must never again be the empty-capture predicate: see `capture`.
        "exec_db": (mount / "exec.db").exists(),
        # Whether the store could actually be READ, and if not, why not.
        "capture": chain["capture"],
        "service_user": service_user,
        "receipt": receipt_file,
    }


def coverage_main(argv):
    import argparse
    ap = argparse.ArgumentParser(
        prog="pilot-report.py --coverage",
        description="Coverage report over a set of pilot run records. Refuses "
                    "to emit any figure it has not read an instrument-failures "
                    "set for.")
    ap.add_argument("--coverage", action="store_true")
    ap.add_argument("--pilot-dir", required=True,
                    help="the directory holding mount-*/receipts-*/runs-*")
    ap.add_argument("--run-prefix", required=True,
                    help="run-id prefix selecting the runs to report on. "
                         "Required: a report that sweeps every run-id on the "
                         "disk reports on a set nobody chose.")
    ap.add_argument("--instrument-failures", default=None,
                    help="path to the instrument-failures set (default: "
                         "scripts/instrument-failures.json, or "
                         "$THRULINE_INSTRUMENT_FAILURES)")
    ap.add_argument("--as-coverage", action="store_true",
                    help="assert these numbers are COVERAGE. Refused for "
                         "oracle runs, whose numbers are instrument reach.")
    ap.add_argument("--json", default=None, help="also write the figure here")
    a = ap.parse_args(argv)

    # BEFORE anything is read, let alone divided: a figure emitted under a
    # predicate nobody declared is the defect this whole block exists for.
    assert_predicates_pinned()

    failset, fpath, fdoc, attestations = load_failset(a.instrument_failures)

    pilot_dir = pathlib.Path(a.pilot_dir)
    rids = sorted(p.name[len("mount-"):]
                  for p in pilot_dir.glob("mount-" + a.run_prefix + "*")
                  if p.is_dir())
    if not rids:
        raise Refused(
            EXIT_NO_DENOMINATOR,
            "no run records under %s with run-id prefix %r." % (pilot_dir, a.run_prefix))
    runs = [read_run(pilot_dir, rid) for rid in rids]

    unnamed = [r["run_id"] for r in runs if not r["task"]]
    if unnamed:
        raise Refused(
            EXIT_UNCLASSIFIED,
            "%d run(s) name no task (%s). A run whose task cannot be named "
            "cannot be looked up in the instrument-failures set, and an "
            "unclassifiable run may not sit in a denominator."
            % (len(unnamed), ", ".join(unnamed)))

    # by_class[<class>] = [(run, entry), ...]; every class but `product` is an
    # exclusion, and each is printed under its own heading with its mechanism.
    by_class = {c: [] for c in FAILSET_CLASSES if c not in COUNTED_CLASSES}
    excl_service_user = []
    counted = []
    for r in runs:
        e = entry_for(failset, r["task"], r["run_id"])
        # An empty capture is the signature this whole file exists for. It is
        # ambiguous by construction (instrument / ceiling / environment / task /
        # genuine product zero), so an unclassified one is refused, never
        # bucketed.
        # THE EMPTY-CAPTURE PREDICATE. `not r["exec_db"]` was file existence
        # only, so a 0-byte or unreadable store returned all-zeros and was
        # counted with no warning at all. It is keyed on `folded` now: the
        # store was OPENED and all three queries ANSWERED, or this run captured
        # nothing and must be classified rather than averaged.
        empty = ((not r["started"]) or r["spool_bytes"] == 0
                 or not r["chain"]["folded"])
        if e is None:
            if empty:
                raise Refused(
                    EXIT_UNCLASSIFIED,
                    "task %r (run %s) captured nothing — started=%s exec.db=%s "
                    "spool=%d bytes — and the instrument-failures set %s does "
                    "not classify it for this run.\n"
                    "  Counting it would contribute a ZERO that may belong to "
                    "the instrument, to the\n"
                    "  environment, to the task, or to ThruLine, and nothing on "
                    "disk distinguishes them.\n"
                    "  Classify it (%s) with an `applies_to` covering %r, or it "
                    "is not reportable."
                    % (r["task"], r["run_id"], r["started"],
                       "%s (%s)" % (r["exec_db"], r["capture"]),
                       r["spool_bytes"], fpath, " / ".join(FAILSET_CLASSES),
                       r["run_id"]))
            if r["service_user"] is not None:
                excl_service_user.append(r)
            else:
                counted.append(r)
            continue
        if e["class"] in COUNTED_CLASSES:
            # "product" — an empty capture that IS a ThruLine result.
            if r["service_user"] is not None:
                excl_service_user.append(r)
            else:
                counted.append(r)
        else:
            by_class[e["class"]].append((r, e))

    # ------------------------------------------------------------------
    # THE ATTESTATION CHECK. Everything above can pass on a set that was read
    # and is silent — `entries: []` classifies nothing, and a run with no empty
    # captures then sails through with no exclusions and no reader. This is the
    # step that cannot: the set must carry a row for THIS run prefix naming the
    # exclusions somebody saw, and it must match what was just measured.
    # ------------------------------------------------------------------
    excluded_now = sorted(
        [r["task"] for lst in by_class.values() for r, _ in lst]
        + [r["task"] for r in excl_service_user])
    att = attestations.get(a.run_prefix)
    if att is None:
        raise Refused(
            EXIT_NO_ATTESTATION,
            "the instrument-failures set %s carries no attestation for run "
            "prefix %r.\n"
            "  It has attestations for: %s.\n"
            "  This run excludes %d task(s) from the denominator (%s).\n"
            "  An exclusion set nobody attested to is not the same fact as an "
            "empty one:\n"
            "  \"there were no instrument failures\" is a claim a human makes "
            "and dates;\n"
            "  \"I never looked\" is this. They may not share a "
            "representation, so this refuses."
            % (fpath, a.run_prefix,
               ", ".join(sorted(attestations)) or "(none)",
               len(excluded_now), ", ".join(excluded_now) or "none"))
    attested = sorted(att["excluded"])
    if attested != excluded_now:
        missing = [t for t in excluded_now if t not in attested]
        extra = [t for t in attested if t not in excluded_now]
        raise Refused(
            EXIT_ATTESTATION_STALE,
            "the attestation for run prefix %r in %s disagrees with the run "
            "records.\n"
            "  excluded on disk but not attested: %s\n"
            "  attested but not excluded on disk: %s\n"
            "  The attestation is what makes the set COMPLETE for this run "
            "rather than merely\n"
            "  readable. A stale one is a reader who checked a different "
            "measurement."
            % (a.run_prefix, fpath,
               ", ".join(missing) or "none", ", ".join(extra) or "none"))

    denom = len(counted)
    n_instrumented = sum(1 for r in counted if r["instrumented"])
    n_started = sum(1 for r in counted if r["started"])
    n_graded = sum(1 for r in counted if r["graded"])
    # §20.1 chain coverage, over the SAME denominator, from the same records.
    n_hooks = sum(1 for r in counted if r["hooks_folded"])
    n_cmds = sum(1 for r in counted if r["agent_commands"])
    n_bound = sum(1 for r in counted if r["snapshot_bound"])
    n_all3 = sum(1 for r in counted
                 if r["hooks_folded"] and r["agent_commands"] and r["snapshot_bound"])
    # The looser reading of the SAME records, over the SAME denominator.
    n_alt = {k: sum(1 for r in counted if r["alt_" + k])
             for k in ("hooks_folded", "agent_commands", "snapshot_bound")}
    n_alt["all_three"] = sum(1 for r in counted
                             if r["alt_hooks_folded"] and r["alt_agent_commands"]
                             and r["alt_snapshot_bound"])

    # An empty denominator is answered FIRST. It used to sit after the
    # vocabulary refusal, where `all(...)` over an empty `counted` is
    # vacuously True — so a set with everything excluded refused for the wrong
    # reason and named the wrong fix.
    if denom == 0:
        raise Refused(
            EXIT_NO_DENOMINATOR,
            "every one of the %d discovered run(s) was excluded, so there is no "
            "denominator. A figure over an empty denominator is not a figure, "
            "and 0 of 0 printed as a percentage is the same lie as a zero from "
            "a broken task." % len(runs))

    # `agents` is over EVERY discovered run, because the header line reports
    # what was on disk. It is a DISPLAY quantity and nothing keys on it.
    agents = {}
    for r in runs:
        agents[r["agent"] or "unknown"] = agents.get(r["agent"] or "unknown", 0) + 1
    # ------------------------------------------------------------------
    # THE VOCABULARY REFUSAL IS EVALUATED OVER `counted`, WHICH IS THE SET THE
    # FIGURE IS COMPUTED OVER. It used to iterate `runs` — every run
    # discovered, exclusions included — and that is a FAIL-OPEN, demonstrated:
    # on an identical all-oracle denominator, flipping the `agent_name` of the
    # one EXCLUDED run from `oracle` to `claude-code` turned rc 8 REFUSED into
    # rc 0, emitting a `COVERAGE REPORT` and `"label": "coverage"` over two
    # oracle runs. A run that is not in the denominator cannot license the word
    # used to describe it; one excluded row could buy the whole figure a name
    # its evidence does not support. That defeats a §1.1 refusal outright, and
    # a §1.1 refusal is not a disclaimer.
    # ------------------------------------------------------------------
    counted_agents = sorted({(r["agent"] or "unknown") for r in counted})
    reach_only = all(x in ("oracle", "unknown") for x in counted_agents)
    if a.as_coverage and reach_only:
        raise Refused(
            EXIT_VOCABULARY,
            "the %d run(s) IN THE DENOMINATOR used agent(s) %s. An `oracle` run "
            "opens no agent session and fires no hooks, so it can evidence "
            "neither hooks-folded nor agent-commands-recorded (§20.1). Its "
            "number is INSTRUMENT REACH (§1.1): plumbing only, never coverage, "
            "and never in a receipt.%s"
            % (denom, ", ".join(counted_agents),
               ("\n  (Runs on disk also include %s, and every one of them was "
                "EXCLUDED. An excluded\n   run is not evidence for the word "
                "`coverage`.)"
                % ", ".join(sorted(set(agents) - set(counted_agents))))
               if set(agents) - set(counted_agents) else ""))
    label = "instrument-reach" if reach_only else "coverage"

    w = []
    w.append("%s REPORT — %d run(s) under %s (prefix %r)"
             % (label.upper(), len(runs), pilot_dir, a.run_prefix))
    w.append("  agents            : " + ", ".join("%s x%d" % (k, v)
                                                  for k, v in sorted(agents.items())))
    # Printed separately BECAUSE the two sets can differ and the difference is
    # what decides the label. Reading one line and assuming the other is how a
    # figure over two oracle runs got called `coverage`.
    w.append("  agents IN DENOMINATOR: %s  -> label `%s`"
             % (", ".join(counted_agents), label))
    w.append("  instrument-failures set: %s" % fpath)
    w.append("    schema %s · %d task(s) classified · %d entries · derived %s"
             % (FAILSET_SCHEMA, len(failset),
                sum(len(v) for v in failset.values()),
                fdoc.get("derived_on", "undated")))
    w.append("    attestation for %r: %d exclusion(s), READ and matched against "
             "the run records" % (a.run_prefix, len(attested)))
    if not attested:
        w.append("      (an EMPTY attested exclusion set — a positive claim that "
                 "nothing was excluded,")
        w.append("       which is a different fact from an unread set and exits 11 "
                 "if it is missing.)")
    CLASS_HEADINGS = [
        ("instrument", "instrument failures — ThruLine's instrumentation BROKE "
                       "the task (Ruling 6: never zeros, always a mechanism)"),
        ("ceiling", "ceiling — ThruLine's instrumentation REFUSED the task. Not "
                    "damage, and not a product result"),
        ("environment", "environment — neither the instrument nor ThruLine nor "
                        "the task"),
        ("task-upstream", "task upstream — the task's own image no longer builds, "
                          "on its own terms"),
        ("run-incomplete", "run incomplete — the run never finished; there is no "
                           "outcome to attribute"),
    ]
    for cls, heading in CLASS_HEADINGS:
        rows = by_class.get(cls, [])
        w.append("")
        w.append("EXCLUDED — %s" % heading)
        if not rows:
            w.append("  none in this run set.")
        for r, e in rows:
            w.append("  %-30s stage lost: %-12s [%s]"
                     % (r["task"], e["stage_lost"], e["evidence_tier"]))
            w.append("      mechanism: %s" % e["mechanism"])
    if excl_service_user:
        w.append("")
        w.append("EXCLUDED — service-user override (compose receipt `service_user`)")
        for r in excl_service_user:
            w.append("  %-22s runs as `%s` — compose-instrument states such a run "
                     "measures instrument" % (r["task"], r["service_user"]))
            w.append("      reach and is not a scored pass. Excluded from the "
                     "denominator on that record.")
    w.append("")
    w.append("DENOMINATORS — three quantities, never one (§1.1)")
    w.append("  runs discovered                : %d" % len(runs))
    for cls, _ in CLASS_HEADINGS:
        w.append("  excluded, %-21s: %d" % (cls, len(by_class.get(cls, []))))
    w.append("  excluded, service-user override: %d" % len(excl_service_user))
    w.append("  measurable denominator D       : %d" % denom)
    w.append("  1. instrumented : %2d of %-2d   the patcher wrote a file Compose can parse"
             % (n_instrumented, denom))
    w.append("  2. started      : %2d of %-2d   the container reached `sleep infinity`"
             % (n_started, denom))
    w.append("  3. graded       : %2d of %-2d   the tests executed and produced a parsed result"
             % (n_graded, denom))
    w.append("  These are not averaged. Each is strictly weaker than the one above it,")
    w.append("  and a single blended percentage moves for three unrelated reasons.")
    w.append("")
    w.append("CHAIN %s (§20.1) — the gate's own three components, same denominator"
             % label.upper())
    w.append("  1. hooks folded into the chain : %2d of %-3d (%.1f%%)"
             % (n_hooks, denom, 100.0 * n_hooks / denom))
    w.append("  2. agent commands as `exec`    : %2d of %-3d (%.1f%%)"
             % (n_cmds, denom, 100.0 * n_cmds / denom))
    w.append("  3. snapshot bound to a real tree: %2d of %-3d (%.1f%%)"
             % (n_bound, denom, 100.0 * n_bound / denom))
    w.append("  ALL THREE                      : %2d of %-3d (%.1f%%)"
             % (n_all3, denom, 100.0 * n_all3 / denom))
    w.append("  `bound` excludes git's empty tree %s: 40 legal hex characters" % EMPTY_TREE)
    w.append("  that mean \"no files\". Counting it would raise component 3 and bind nothing.")
    w.append("")
    w.append("  PREDICATES, PINNED (sha256 %s)" % CHAIN_PREDICATES_SHA256[:16])
    for key in ("hooks_folded", "agent_commands", "snapshot_bound"):
        p = CHAIN_PREDICATES[key]
        w.append("   %d. %s" % (p["component"], p["name"]))
        w.append("      hit when > 0: %s" % p["pinned"])
    w.append("  The figure above is the PINNED reading. The same 100 records "
             "under a looser")
    w.append("  reading of the same three components — a hook `tool_call` "
             "counted as a command,")
    w.append("  and any `snapshot` event with a real tree counted as bound — "
             "give:")
    w.append("   1. %2d of %-3d   2. %2d of %-3d   3. %2d of %-3d   ALL THREE: "
             "%2d of %-3d (%.1f%%)"
             % (n_alt["hooks_folded"], denom, n_alt["agent_commands"], denom,
                n_alt["snapshot_bound"], denom, n_alt["all_three"], denom,
                100.0 * n_alt["all_three"] / denom))
    w.append("  Both are printed because the gap between them is a DEFINITION, "
             "not evidence.")
    w.append("  The gate is read against the pinned reading; neither reading "
             "may be swapped in")
    w.append("  after the fact to move a number.")
    w.append("  CAVEAT ON COMPONENT 1, stated because it is weaker than its name.")
    w.append("  `hooks folded` here is `>= 1 event whose source ends in -hook is in")
    w.append("  exec.db`. It is NOT a chain recomputation: this reads the store, it does")
    w.append("  not re-hash it. `thruline doctor` is what verifies the chain, and a run")
    w.append("  set that passes here can still hold a broken prev_hash link.")
    if reach_only:
        w.append("")
        w.append("THIS IS INSTRUMENT REACH, NOT COVERAGE (§1.1)")
        w.append("  Every run above used `oracle` (or an agent the record does not name).")
        w.append("  `oracle` opens no agent session and fires no hooks, so these numbers")
        w.append("  can evidence neither `hooks folded` nor `agent commands recorded`.")
        w.append("  They are plumbing. They are not chain coverage (§20.1) and may not")
        w.append("  enter a receipt.")
    w.append("")
    w.append("COUNTED (%d)" % denom)
    for r in counted:
        w.append("  %-30s hooks=%-5s cmds=%-5s bound=%-5s (checks=%d) spool=%d B"
                 % (r["task"], r["hooks_folded"], r["agent_commands"],
                    r["snapshot_bound"], r["chain"]["checks"], r["spool_bytes"]))
    print("\n".join(w))

    figure = {
        "label": label,
        "discovered": len(runs),
        "denominator": denom,
        "instrumented": n_instrumented,
        "started": n_started,
        "graded": n_graded,
        "chain_coverage": {
            "hooks_folded": n_hooks,
            "agent_commands_recorded": n_cmds,
            "snapshots_bound": n_bound,
            "all_three": n_all3,
            "denominator": denom,
            "empty_tree_excluded": EMPTY_TREE,
            "reading": "pinned",
            "predicates_sha256": CHAIN_PREDICATES_SHA256,
            "predicates": {k: v["pinned"] for k, v in CHAIN_PREDICATES.items()},
        },
        # The looser reading, same denominator, same records. Emitted so that a
        # later reader comparing two figures can see which predicate produced
        # each instead of inferring it.
        "chain_coverage_alternate": {
            "hooks_folded": n_alt["hooks_folded"],
            "agent_commands_recorded": n_alt["agent_commands"],
            "snapshots_bound": n_alt["snapshot_bound"],
            "all_three": n_alt["all_three"],
            "denominator": denom,
            "empty_tree_excluded": EMPTY_TREE,
            "reading": "alternate",
            "predicates": {k: v["alternate"] for k, v in CHAIN_PREDICATES.items()},
        },
        "exclusions": {
            cls: [{"task": r["task"], "stage_lost": e["stage_lost"],
                   "evidence_tier": e["evidence_tier"],
                   "mechanism": e["mechanism"]} for r, e in by_class.get(cls, [])]
            for cls, _ in CLASS_HEADINGS
        },
        "service_user_exclusions": [
            {"task": r["task"], "service_user": r["service_user"]}
            for r in excl_service_user],
        "counted": [r["task"] for r in counted],
        "agents": agents,
        # The set the label was actually decided from.
        "agents_in_denominator": counted_agents,
        "failset": {
            "path": str(fpath),
            "sha256": hashlib.sha256(
                fpath.read_bytes()).hexdigest(),
            "tasks_classified": len(failset),
            "entries": sum(len(v) for v in failset.values()),
            "derived_on": fdoc.get("derived_on"),
            "attestation": {"run_prefix": a.run_prefix, "excluded": attested,
                            "read": True},
        },
    }
    if a.json:
        pathlib.Path(a.json).write_text(json.dumps(figure, indent=1) + "\n",
                                        encoding="utf-8")
    return 0


if "--coverage" in sys.argv[1:]:
    try:
        sys.exit(coverage_main(sys.argv[1:]))
    except Refused as e:
        sys.exit(refuse(e.code, str(e)))

# ==========================================================================
# PER-TASK MODE — one run, driven by the environment scripts/pilot-run.sh sets.
# ==========================================================================

MOUNT = pathlib.Path(os.environ["MOUNT"])
OUT = pathlib.Path(os.environ["OUT"])
RUN_ID = os.environ["RUN_ID"]
TASK_ID = os.environ["TASK_ID"]
AGENT = os.environ["AGENT"]
MODEL = os.environ.get("MODEL", "")
PAYS = os.environ.get("PAYS", "0") == "1"

# The set is read HERE TOO, before anything is measured. This mode prints no
# denominator, but it does print per-task numbers, and for a task the instrument
# broke those numbers are the instrument's own zeros. Refusing at the top means
# the refusal cannot be reached only on the paths somebody remembered to guard.
EXIT_EXCLUDED_TASK = 10
try:
    FAILSET, FAILSET_PATH, FAILSET_DOC, _FAILSET_ATT = load_failset()
except Refused as _e:
    sys.exit(refuse(_e.code, str(_e)))
# Scoped by RUN ID, like the coverage mode: an entry derived from another
# measurement's run must not silently classify this one.
try:
    FAILSET_ENTRY = entry_for(FAILSET, TASK_ID, RUN_ID)
except Refused as _e:
    sys.exit(refuse(_e.code, str(_e)))

# Micro-dollars per million tokens. Integer arithmetic throughout: money in
# floats accumulates error in whichever direction the rounding mode happens to
# favour, and a budget guard must never round a spend DOWN.
#
# FIVE rates, not two. A two-rate table cannot express the bill: on a real
# Claude Code transcript on this machine, cache reads were 75% of the spend and
# cache writes another 16%, so a table that knows only input and output sees
# $1.60 of an $18.86 bill. See `Price` in crates/bench/src/pilot.rs.
#
#   (base_input, output, cache_read, cache_write_5m, cache_write_1h)
#
# Kept byte-identical to `PRICES` in crates/bench/src/pilot.rs, and
# crates/bench/tests/pilot.rs::the_two_price_tables_cannot_drift_apart parses
# this literal and fails if they disagree. Two tables that must agree and only
# a comment saying so is how the last four vendor facts went stale.
PRICES = {
    # OpenAI / Codex. Input and output verified against the vendor page; the
    # three cache rates are DELIBERATE UPPER BOUNDS at the full input rate,
    # because an unverified cache rate that is too LOW makes the guard trip
    # late. See GPT_5_6_LUNA in crates/bench/src/pilot.rs.
    "gpt-5.6-luna": (200_000, 1_200_000, 200_000, 200_000, 200_000),
    # Anthropic. All five read off platform.claude.com/docs/en/about-claude/
    # pricing on 2026-09-03: $1 base input, $5 output, $0.10 cache hit,
    # $1.25 5 m cache write, $2 1 h cache write per MTok. The id is the pinned
    # snapshot from the models overview; `claude-haiku-4-5` is its alias and is
    # the same rate card.
    "claude-haiku-4-5-20251001": (1_000_000, 5_000_000, 100_000, 1_250_000, 2_000_000),
    "claude-haiku-4-5": (1_000_000, 5_000_000, 100_000, 1_250_000, 2_000_000),
}

# Anything that goes wrong in the COST section sets this. The script used to
# PRINT that a missing usage record "HALTS the run" and then exit 0 — a halt
# that returns success is not a halt, and a caller reading only the status saw
# a clean pilot. Rule 12.
HALT_REASONS = []

lines = []
def out(s=""):
    lines.append(s)


def ceil_div(a, b):
    return -(-a // b)


# --------------------------------------------------------------------------
# The three coverage measures, counted off the folded execution store.
#
# THESE COME OUT OF `CHAIN_PREDICATES`, and did not before. This path used to
# count a check as bound when `input_tree_hash` was merely TRUTHY — so the
# empty tree passed, and the same word `bound` meant 93 of 100 here and 65 of
# 100 in `_chain_measures`, in this one file, with no note that they differed.
# One predicate table, executed twice, is the fix; two hand-written predicates
# with one name is the defect.
# --------------------------------------------------------------------------
try:
    assert_predicates_pinned()
except Refused as _e:
    sys.exit(refuse(_e.code, str(_e)))
hook_events = shim_execs = bound_checks = 0
checks = []
db = MOUNT / "exec.db"
if db.exists():
    import sqlite3
    c = sqlite3.connect(str(db))
    try:
        hook_events = c.execute(
            CHAIN_PREDICATES["hooks_folded"]["pinned"],
            {"empty": EMPTY_TREE}).fetchone()[0]
        shim_execs = c.execute(
            CHAIN_PREDICATES["agent_commands"]["pinned"],
            {"empty": EMPTY_TREE}).fetchone()[0]
        bound_checks = c.execute(
            CHAIN_PREDICATES["snapshot_bound"]["pinned"],
            {"empty": EMPTY_TREE}).fetchone()[0]
        cols = [r[1] for r in c.execute("pragma table_info(checks)")]
        for row in c.execute("select * from checks"):
            checks.append(dict(zip(cols, row)))
    finally:
        c.close()

# EMPTY_TREE is defined once, at the top of this file, next to the chain
# measures that also refuse it. Recording it as "the tree before the first
# action" is the exact failure this pilot exists to catch: it is 40 hex
# characters, it passes every shape check, and it means nothing. The container
# baseline is taken at container START, which is before the harness has even
# copied the task in — so on a real run it is normally this constant.

baseline = (MOUNT / "baseline-tree.txt")
tree_before = baseline.read_text().strip() if baseline.exists() else ""
tree_shaped = bool(re.fullmatch(r"[0-9a-f]{40}", tree_before))

# The condition the operator actually set is "a tree hash recorded BEFORE the
# agent's first action" — which means before the first action we OBSERVED, not
# before the container existed. That is the input tree of the earliest recorded
# exec, which is what a check would be bound against.
# TWO HONEST CAVEATS, because this query says less than it looks like it says:
#
#  * `checks` HAS NO TIMESTAMP COLUMN (verified: pragma table_info carries no
#    ts/created_at). "Earliest" is really "lowest rowid", which equals earliest
#    only because `fold` inserts in spool order — a contract nothing states and
#    no test holds. With one check in a single-task pilot it cannot bite; at ten
#    tasks it could, and the fix is a column, not a cleverer query.
#  * A `where input_tree_hash is not null` clause was here and was a NO-OP: the
#    column is NOT NULL (verified), so it read as excluding unbound checks while
#    excluding nothing. Removed rather than left to reassure a future reader.
tree_at_first_action = ""
n_checks_total = 0
if db.exists():
    import sqlite3 as _s
    _c = _s.connect(str(db))
    try:
        row = _c.execute(
            "select input_tree_hash from checks order by rowid limit 1").fetchone()
        if row:
            tree_at_first_action = row[0]
        n_checks_total = _c.execute("select count(*) from checks").fetchone()[0]
    finally:
        _c.close()

tree_ok = bool(re.fullmatch(r"[0-9a-f]{40}", tree_at_first_action)) \
    and tree_at_first_action != EMPTY_TREE

out(f"run          : {RUN_ID}")
out(f"task         : {TASK_ID}")
out(f"agent        : {AGENT}" + (f"  model: {MODEL}" if MODEL else ""))
out()

# A task the set classifies as an instrument or environment failure has no
# measurement to print. Printing `hook events: NO (0)` for it is the defect
# Ruling 6 names: the instrument's damage rendered as the product's zero.
if FAILSET_ENTRY and FAILSET_ENTRY["class"] not in COUNTED_CLASSES:
    out("NOT MEASURED — %s is a known %s failure (%s)"
        % (TASK_ID, FAILSET_ENTRY["class"], FAILSET_PATH))
    out("  stage lost: %s" % FAILSET_ENTRY["stage_lost"])
    out("  mechanism : %s" % FAILSET_ENTRY["mechanism"])
    out("  evidence  : %s" % FAILSET_ENTRY["evidence_tier"])
    out("  This run measures the instrument, not the product. No capture number")
    out("  is printed for it — a zero here would be the instrument's own damage")
    out("  wearing the product's name (Ruling 6, §12).")
    print("\n".join(lines))
    sys.exit(refuse(
        EXIT_EXCLUDED_TASK,
        "task %r is classified %r in %s and produces no measurement."
        % (TASK_ID, FAILSET_ENTRY["class"], FAILSET_PATH)))

# §1.1 VOCABULARY. This header read "CAPTURE COVERAGE" for every agent,
# `oracle` included — the exact swap Ruling 2 forbids. There is no denominator
# on one task in any case, so neither form is a coverage figure.
if AGENT == "oracle":
    out("INSTRUMENT REACH — plumbing only (§1.1). NOT coverage, never a receipt.")
else:
    out("CAPTURE MEASURES — one task, no denominator. Not a coverage figure.")
out("  the three measures, separately (1 task in this run)")
out(f"  1. hook events present          : {'YES' if hook_events else 'NO ':<3}  ({hook_events} events)")
out(f"  2. shim exec events present     : {'YES' if shim_execs else 'NO ':<3}  ({shim_execs} execs)")
out(f"  3. check bound to a NON-EMPTY tree: {'YES' if bound_checks else 'NO ':<3}  ({bound_checks} of {len(checks)} checks)")
out(f"  +  tree before first action     : {'YES' if tree_ok else 'NO ':<3}  ({tree_at_first_action or 'absent'})")
if n_checks_total > 1:
    out("     caution: more than one check, and `checks` carries no timestamp —")
    out("     'earliest' above is lowest rowid, i.e. spool order by convention")
    out("     and by nothing that is stated or tested.")
if tree_before == EMPTY_TREE:
    out("     note: the container-start baseline is git's EMPTY tree, because the")
    out("     harness stages the task after the container is up. It is reported")
    out("     here only so it is never mistaken for the measurement above.")
elif tree_shaped:
    out(f"     container-start baseline: {tree_before}")
for d in checks:
    out(f"     check: runner={d.get('runner')} exit={d.get('exit_code')} "
        f"tree={str(d.get('input_tree_hash'))[:12]} parsed={d.get('parsed')}")

# --------------------------------------------------------------------------
# THE COST READER. This was the most dangerous defect in the whole pilot.
#
# It read `d["usage"]` and `d["payload"]["usage"]`. Measured against a REAL
# 8,187-line Codex rollout on this machine
# (~/.codex/sessions/2026/05/22/rollout-2026-05-22T22-46-52-...jsonl):
#
#     lines it could read usage from ...............     0
#     lines carrying payload.info.total_token_usage . 1,693
#
# So a paid Codex run printed `input tokens: 0 / output tokens: 0 /
# cost: $0.000000` under a header reading "measured, never estimated", with a
# hardcoded provenance string that made `UsageSource::may_understate()` return
# false, and exited 0. A budget guard reading this would never trip at any
# spend. That is strictly worse than no guard.
#
# TWO RECORD SHAPES, TWO OPPOSITE REDUCTIONS, and getting them the wrong way
# round is a two-to-three-order-of-magnitude error:
#
#   * Codex `payload.info.total_token_usage` is CUMULATIVE and monotonic —
#     each record restates the session total. Measured on that file: 0
#     monotonicity violations in 1,693 records, and summing rather than taking
#     the running maximum overstates input by 820x and output by 886x.
#     Reduction: max WITHIN a file, sum ACROSS files.
#   * Claude Code `message.usage` is PER-REQUEST. Measured on a real
#     533-message transcript in ~/.claude/projects: `input_tokens` is
#     non-decreasing only because it never exceeds 2. Reduction: SUM.
#
# `_cumulative_total` below is written as "first value + every positive
# increment", which telescopes to the maximum when the counter is monotonic and
# stays correct when it is reset (a compaction, a resumed session) — where a
# bare max would silently understate.
# --------------------------------------------------------------------------


def _iter_json(path):
    for line in path.read_text(errors="replace").splitlines():
        try:
            d = json.loads(line)
        except ValueError:
            continue
        if isinstance(d, dict):
            yield d


def _cumulative_total(values):
    """Total of a cumulative counter that may reset. == max() when monotonic."""
    total = 0
    prev = None
    resets = 0
    for v in values:
        if prev is None:
            total = v
        elif v >= prev:
            total += v - prev
        else:
            resets += 1
            total += v
        prev = v
    return total, resets


def codex_usage(rollouts):
    """Sum of per-file cumulative totals. Returns disjoint billing buckets."""
    tot_in = tot_cached = tot_out = 0
    records = resets = 0
    for r in rollouts:
        seq_in, seq_cached, seq_out = [], [], []
        for d in _iter_json(r):
            info = (d.get("payload") or {}).get("info")
            if not isinstance(info, dict):
                continue
            t = info.get("total_token_usage")
            if not isinstance(t, dict):
                continue
            records += 1
            seq_in.append(int(t.get("input_tokens") or 0))
            seq_cached.append(int(t.get("cached_input_tokens") or 0))
            seq_out.append(int(t.get("output_tokens") or 0))
        for seq, bucket in ((seq_in, "in"), (seq_cached, "cached"), (seq_out, "out")):
            v, n = _cumulative_total(seq)
            resets += n
            if bucket == "in":
                tot_in += v
            elif bucket == "cached":
                tot_cached += v
            else:
                tot_out += v
    # Codex reports `input_tokens` INCLUSIVE of `cached_input_tokens`, so the
    # base-rate bucket is the difference. Adding them would double-count the
    # cached half; ignoring the split (as the old reader did) charges cached
    # tokens at the full rate. Codex bills no cache-write line.
    cached = min(tot_cached, tot_in)
    return {
        "input": tot_in - cached,
        "output": tot_out,
        "cache_read": cached,
        "cache_write_5m": 0,
        "cache_write_1h": 0,
    }, records, resets


def claude_usage(paths):
    """Claude Code: per-request usage, summed. Never a running maximum.

    Reads BOTH shapes the harness can leave behind:
      * the stream-json final `result` message (`--output-format stream-json`),
        which carries the whole run's usage in one record; and
      * `assistant` records in a session transcript, one `message.usage` each.
    The `result` record wins where both are present — it is the run's own
    total — and the transcript sum is the fallback.
    """
    acc = {"input": 0, "output": 0, "cache_read": 0,
           "cache_write_5m": 0, "cache_write_1h": 0}
    records = 0
    unsplit_writes = 0
    vendor_usd = 0.0
    shape = None
    for path in paths:
        results, assistants = [], []
        for d in _iter_json(path):
            kind = d.get("type")
            if kind == "result" and isinstance(d.get("usage"), dict):
                results.append(d)
            elif kind == "assistant":
                m = d.get("message")
                if isinstance(m, dict) and isinstance(m.get("usage"), dict):
                    assistants.append(m["usage"])
        if results:
            shape = shape or "result"
            chosen = [r["usage"] for r in results]
            for r in results:
                try:
                    vendor_usd += float(r.get("total_cost_usd") or 0.0)
                except (TypeError, ValueError):
                    pass
        else:
            chosen = assistants
            if chosen:
                shape = shape or "assistant-transcript"
        for u in chosen:
            records += 1
            acc["input"] += int(u.get("input_tokens") or 0)
            acc["output"] += int(u.get("output_tokens") or 0)
            acc["cache_read"] += int(u.get("cache_read_input_tokens") or 0)
            created = int(u.get("cache_creation_input_tokens") or 0)
            split = u.get("cache_creation")
            if isinstance(split, dict):
                acc["cache_write_5m"] += int(split.get("ephemeral_5m_input_tokens") or 0)
                acc["cache_write_1h"] += int(split.get("ephemeral_1h_input_tokens") or 0)
            elif created:
                # No TTL breakdown. Charge the DEARER rate: a cache write billed
                # at 1.25x when it was really 2x understates, and understating
                # is the one direction a budget guard may not err in.
                unsplit_writes += created
                acc["cache_write_1h"] += created
    return acc, records, shape, unsplit_writes, vendor_usd


def price_cost(buckets, price):
    return (
        ceil_div(buckets["input"] * price[0], 1_000_000)
        + ceil_div(buckets["output"] * price[1], 1_000_000)
        + ceil_div(buckets["cache_read"] * price[2], 1_000_000)
        + ceil_div(buckets["cache_write_5m"] * price[3], 1_000_000)
        + ceil_div(buckets["cache_write_1h"] * price[4], 1_000_000)
    )


out()
out("COST — measured, never estimated")
# WHICH BINARY HELD THE CEILING. The runner resolves `thruline` by trying
# target/release, then target/debug, then $PATH, and used to take the first
# executable file it found without asking whether that file could enforce a
# ceiling at all. Naming it here means a report can be read back later and the
# guard's identity is part of the record rather than an assumption about it.
out("  guard binary: %s" % (os.environ.get("TL_HOST") or "none resolved"))
out("  guard version: %s" % (os.environ.get("TL_HOST_VERSION") or "unknown"))
rollouts = sorted((MOUNT / "codex-sessions").rglob("rollout-*.jsonl"))
# Claude Code's own transcripts live in ~/.claude/projects/** INSIDE the
# container, which nothing currently mounts (see the GAP note at the end of this
# section). What the harness does put on the host is its agent-log directory, so
# both are searched and the absence of either is reported rather than assumed.
# Written as an explicit loop. The comprehension this replaced read
# `a + b + c if OUT.exists() else []`, which binds as `(a+b+c) if ... else []`
# — so a missing OUT discarded the two directories that had been found. A
# conditional that silently empties the evidence list is the exact shape this
# whole change is about.
_claude_roots = [MOUNT / "claude-projects", MOUNT / "agent-logs", OUT]
claude_logs = []
for _root in _claude_roots:
    if _root.exists():
        claude_logs.extend(_root.rglob("*.jsonl"))
claude_logs = sorted(set(claude_logs))
buckets = None
source_name = None
if not PAYS:
    out("  agent 'oracle' makes zero model calls, so there is no rollout to read.")
    out("  cost: $0.000000 — measured by the absence of any model call, not assumed.")
elif AGENT == "claude-code":
    buckets, records, shape, unsplit, vendor_usd = claude_usage(claude_logs)
    out(f"  transcript files : {len(claude_logs)}")
    out(f"  usage records    : {records}  (shape: {shape or 'none found'})")
    if records == 0:
        HALT_REASONS.append(
            "no Claude Code usage record was found, so usage cannot be measured "
            "and any cost figure would be inferred")
        out("  NOT MEASURED: no `result` message and no assistant `message.usage`.")
        out("  Per pilot::check_first_task this HALTS the run — and this script now")
        out("  EXITS NON-ZERO rather than printing the word and returning success.")
        out("  GAP: Claude Code writes its transcripts to ~/.claude/projects/** in")
        out("  the container, and no mount brings that to the host. Until one does,")
        out("  the only usage this can read is whatever the harness's own agent-log")
        out("  directory happens to contain.")
        buckets = None
    else:
        source_name = "reconciled" if shape == "result" else "rollout-file"
        if unsplit:
            out(f"  note: {unsplit} cache-write tokens carried no 5m/1h breakdown and")
            out("  were charged at the DEARER 1-hour rate. Never the cheaper one.")
        if vendor_usd:
            out(f"  vendor-stated    : ${vendor_usd:.6f} (total_cost_usd, corroboration only)")
elif not rollouts:
    HALT_REASONS.append("no session rollout file was produced, so usage cannot be measured")
    out("  NOT MEASURED: no session rollout file was produced.")
    out("  Per pilot::check_first_task this HALTS the run — a missing rollout must")
    out("  never be filled in with an estimate. This script now EXITS NON-ZERO on")
    out("  it, rather than printing the word 'HALTS' and returning success.")
else:
    buckets, records, resets = codex_usage(rollouts)
    out(f"  rollout files : {len(rollouts)}")
    out(f"  usage records : {records}  (payload.info.total_token_usage, CUMULATIVE)")
    if resets:
        out(f"  note: {resets} counter reset(s) seen; totalled by summing positive")
        out("  increments, which a bare max() would have silently understated.")
    if records == 0:
        HALT_REASONS.append(
            "the rollout files carry no `payload.info.total_token_usage` record, "
            "so usage cannot be measured")
        out("  NOT MEASURED: rollouts exist but carry no usage record at all.")
        out("  This is the shape the old reader reported as $0.000000 and exit 0.")
        buckets = None
    else:
        source_name = "rollout-file"

if buckets is not None:
    out(f"  source        : {source_name} (carries reasoning tokens; the JSON stream does not)")
    out(f"  input  (base) : {buckets['input']}")
    out(f"  output        : {buckets['output']}")
    out(f"  cache read    : {buckets['cache_read']}")
    out(f"  cache write5m : {buckets['cache_write_5m']}")
    out(f"  cache write1h : {buckets['cache_write_1h']}")
    if sum(buckets.values()) == 0:
        HALT_REASONS.append(
            "every token bucket is zero on a PAYING run — a broken reader, not a "
            "free task")
        out("  ALL ZERO on a paying run. That is a reader fault, not a free task.")
    price = PRICES.get(MODEL)
    if price is None:
        HALT_REASONS.append(f"`{MODEL}` is not in the pinned price table")
        out(f"  NOT PRICED: '{MODEL}' is not in the pinned price table. Refusing to")
        out("  guess a rate — add it with a dated source before spending again.")
    else:
        micros = price_cost(buckets, price)
        out(f"  cost          : ${micros / 1_000_000:.6f}  ({micros} micro-dollars, rounded UP)")
        # The machine-readable half, for `thruline pilot-budget charge`. Written
        # beside the capture mount so the runner charges the SAME numbers that
        # were printed, rather than re-deriving them from prose.
        (MOUNT / "usage.tsv").write_text(
            "\t".join(str(x) for x in (
                buckets["input"], buckets["output"], buckets["cache_read"],
                buckets["cache_write_5m"], buckets["cache_write_1h"],
                source_name, micros)) + "\n")

# --------------------------------------------------------------------------
# Terminal state — tb's own result. CORROBORATION ONLY.
# --------------------------------------------------------------------------
out()
out("TERMINAL STATE (tb's result — corroboration, never the basis of a bound check)")
res = OUT / RUN_ID / "results.json"
if res.exists():
    d = json.loads(res.read_text())
    for r in d.get("results", []):
        out(f"  {r.get('task_id')}: resolved={r.get('is_resolved')} "
            f"failure_mode={r.get('failure_mode')}")
else:
    out(f"  no results.json at {res}")

# --------------------------------------------------------------------------
# Nothing credential-shaped may leave this script. Belt and braces on top of
# the mount that already excludes auth.json.
# --------------------------------------------------------------------------
text = "\n".join(lines)
CRED = re.compile(r"\b(sk-[A-Za-z0-9_-]{16,}|OPENAI_API_KEY\s*=\s*\S+)")
if CRED.search(text):
    print("pilot-report: REFUSING to print — output matched a credential shape.",
          file=sys.stderr)
    sys.exit(2)
# Positive control: the detector must recognise one when it is there, or the
# check above is a tautology that passes on everything.
assert CRED.search("sk-proj-" + "A" * 24), "credential detector is inert"
print(text)

# --------------------------------------------------------------------------
# AND THEN THE EXIT CODE TELLS THE TRUTH.
#
# This script used to PRINT "Per pilot::check_first_task this HALTS the run"
# and exit 0. A halt that returns success is not a halt: `pilot-run.sh` read
# the status, saw zero, and carried on. The report is printed first — it is
# worth having either way — and the status follows it.
#
# 5, not 1: 1 is what a Python traceback exits with, and a caller must be able
# to tell "the pilot found a halting condition" from "this script crashed".
# --------------------------------------------------------------------------
if HALT_REASONS:
    print("\npilot-report: HALT — %d condition(s):" % len(HALT_REASONS), file=sys.stderr)
    for r in HALT_REASONS:
        print(f"  * {r}", file=sys.stderr)
    print("  Nothing here may be filled in with an estimate.", file=sys.stderr)
    sys.exit(5)
