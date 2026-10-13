#!/usr/bin/env python3
"""Classify each Terminal-Bench task by whether it SHIPS CONTENT into the
workspace the agent starts in — before the agent, and before any check, acts.

WHY THIS EXISTS
===============
Component 3 of the chain-coverage gate asks whether a check carries the tree its
inputs hashed to. On the m1 tune-split sweep it read 65/100, and 27 of the 35
misses bound only to git's empty tree. Those 27 were read as a *placement* bug —
`git init` supposedly running in the wrong directory. Measured, that is false:
all 27 resolve to `/app`, which is exactly where `git init` already ran (only 5
of 241 tasks declare `working_dir:` at all, and all 5 say `/app`; none of the 27
declares a `WORKDIR` other than `/app`).

The actual discriminator is whether the task ships a workspace at all:

    of the 65 that bound to a real tree, 61 (94%) COPY/ADD content
    of the 28 that bound only to the empty tree, 4 (14%) do

A task that ships nothing, run by a scripted agent whose entire plan is one call
to the task's own test script, has no content for a check to bind to. Folding
that into an average whose denominator counts it emits a ZERO — and the metric
then reports the *product* under-performing when what actually happened is that
there was nothing to measure. That is the same failure direction Ruling 6 named
for instrument failures, one stratum along, and it gets the same treatment:
**excluded from the denominator, named, and counted separately — never a zero.**

WHAT THIS TOOL CAN AND CANNOT DECIDE, STATED BEFORE ITS OUTPUT
==============================================================
It reads the task's `Dockerfile` and `docker-compose.yaml` statically. It can
decide a `COPY`/`ADD` destination and a volume target. It CANNOT decide content
created by a `RUN` line — `RUN git clone`, `RUN pip download`, a heredoc — which
is a real limit and is reported as its own class rather than folded into either
answer. Three classes, and the third is not a hedge:

    ships          a COPY/ADD or a volume puts content INSIDE the workspace
    ships-none     no COPY/ADD and no volume anywhere in the task
    undecidable    content exists but only RUN lines could have placed it, or
                   it is placed OUTSIDE the workspace

CALIBRATION IS PART OF THE OUTPUT, NOT A CLAIM ABOUT IT
=======================================================
`--calibrate <pilot-dir>` scores this classifier against ground truth: the
`baseline-tree.txt` each m1 run wrote inside its own container, which records
whether the workspace was empty AT THE MOMENT `git init` ran. A classifier whose
agreement with that is not printed beside its verdicts is an opinion.

    scripts/classify-workspace.py <corpus>
    scripts/classify-workspace.py <corpus> --calibrate <pilot-dir> --run-prefix m1-
    scripts/classify-workspace.py <corpus> --json out.json
"""

import argparse
import hashlib
import json
import os
import re
import sys

EXIT_OK = 0
EXIT_USAGE = 2
EXIT_NO_CORPUS = 5
EXIT_PREDICATE_DRIFT = 13  # same number, same meaning, as pilot-report.py

# The empty tree, from `git hash-object -t tree /dev/null`. Not retyped from
# prose: PILOT-REPORT.md carried `4b825dc6c96e…` for three days, which shares
# eight characters with the real value and is otherwise invented.
EMPTY_TREE = "4b825dc642cb6eb9a060e54bf8d69288fbee4904"

# Terminal-Bench's two base images both end `WORKDIR /app`, so a task Dockerfile
# with no WORKDIR of its own still starts there. Verified in the pinned corpus at
# docker/base-images/{ubuntu-24-04,python-3-13}/Dockerfile:7.
DEFAULT_WORKDIR = "/app"

# ==========================================================================
# THE PREDICATES, PINNED. Same discipline as pilot-report.py's CHAIN_PREDICATES
# and for the same reason: this file decides a DENOMINATOR. A stratum whose rule
# can be edited without the digest moving is a stratum that can be widened to
# make a figure look better, which is exactly what happened to `lean4-proof`
# when a probe-scoped exclusion was carried to a whole run prefix.
# ==========================================================================
WORKSPACE_PREDICATES = {
    "workspace": "the last WORKDIR in the task Dockerfile; else the service's "
                 "compose `working_dir:`; else /app from the base image",
    "ships": "a COPY/ADD destination, or a compose volume target, that is the "
             "workspace directory or a path beneath it",
    "ships_none": "the task has no COPY/ADD line and no non-log volume at all",
    "undecidable": "neither of the above: content is placed only by RUN, or "
                   "placed outside the workspace",
}
WORKSPACE_PREDICATES_SHA256 = \
    "dea6e18d09df5a43baa394e6b451bb7e6ede62e8d6c93fa115fcd19c44bba91d"

# Compose volume targets that are harness plumbing, never task content.
LOG_TARGET_RE = re.compile(r"\$\{T_BENCH_CONTAINER_(LOGS|AGENT_LOGS)_PATH\}")

COPY_RE = re.compile(r"^\s*(COPY|ADD)\s+(.*)$", re.IGNORECASE)
WORKDIR_RE = re.compile(r"^\s*WORKDIR\s+(\S+)", re.IGNORECASE)


def predicates_digest():
    canon = json.dumps(WORKSPACE_PREDICATES, sort_keys=True,
                       separators=(",", ":"))
    return hashlib.sha256(canon.encode()).hexdigest()


def check_pin():
    got = predicates_digest()
    if got != WORKSPACE_PREDICATES_SHA256:
        sys.stderr.write(
            "PREDICATE DRIFT — refusing to classify anything.\n\n"
            "  pinned:   %s\n  computed: %s\n\n"
            "A predicate in WORKSPACE_PREDICATES changed and the pin did not.\n"
            "This tool decides which tasks leave a coverage denominator, so it\n"
            "emits no verdict at all rather than a different one under the same\n"
            "name. Update WORKSPACE_PREDICATES_SHA256 deliberately, and say why\n"
            "in the commit.\n" % (WORKSPACE_PREDICATES_SHA256, got))
        sys.exit(EXIT_PREDICATE_DRIFT)


def dockerfile_facts(path):
    """(last WORKDIR or None, [COPY/ADD destinations])."""
    workdir, dests = None, []
    if not os.path.exists(path):
        return None, []
    with open(path, errors="replace") as fh:
        for line in fh:
            m = WORKDIR_RE.match(line)
            if m:
                workdir = m.group(1)
                continue
            m = COPY_RE.match(line)
            if m:
                args = m.group(2)
                # Drop flags (--from=, --chown=) and any JSON-array form's
                # punctuation; the destination is the last remaining token.
                toks = [t for t in args.replace("[", " ").replace("]", " ")
                        .replace('"', " ").replace(",", " ").split()
                        if not t.startswith("--")]
                if len(toks) >= 2:
                    dests.append(toks[-1])
    return workdir, dests


def compose_facts(path):
    """(working_dir or None, [volume targets that are not harness logs]).

    Parsed with a line scanner rather than a YAML loader on purpose: this tool
    must run against a corpus without ruamel installed, and both keys it reads
    are flat scalars in every one of the 241 files.
    """
    working_dir, targets = None, []
    if not os.path.exists(path):
        return None, []
    with open(path, errors="replace") as fh:
        for line in fh:
            s = line.strip()
            if s.startswith("working_dir:"):
                working_dir = s.split(":", 1)[1].strip().strip("'\"")
            elif s.startswith("- ") and ":" in s:
                spec = s[2:].strip().strip("'\"")
                parts = spec.split(":")
                if len(parts) >= 2 and not LOG_TARGET_RE.search(spec):
                    targets.append(parts[1])
    return working_dir, targets


def inside(path, workspace):
    """Is `path` the workspace, or beneath it?"""
    if not path.startswith("/"):
        # A relative COPY destination is relative to the current WORKDIR, so it
        # is inside by construction.
        return True
    return path == workspace or path.startswith(workspace.rstrip("/") + "/")


def classify(task_dir):
    df = os.path.join(task_dir, "Dockerfile")
    dc = os.path.join(task_dir, "docker-compose.yaml")
    wd_docker, dests = dockerfile_facts(df)
    wd_compose, targets = compose_facts(dc)

    # Compose's `working_dir:` overrides the image's WORKDIR; the image's
    # WORKDIR overrides the base's. Docker applies this itself at container
    # start, which is why the setup chain asks `pwd` rather than recomputing it.
    workspace = wd_compose or wd_docker or DEFAULT_WORKDIR

    placed_inside = [d for d in dests + targets if inside(d, workspace)]
    if placed_inside:
        return workspace, "ships", placed_inside
    if not dests and not targets:
        return workspace, "ships-none", []
    return workspace, "undecidable", dests + targets


def calibrate(pilot_dir, prefix, verdicts):
    """Score the classifier against each run's own in-container baseline tree."""
    rows = []
    for name in sorted(os.listdir(pilot_dir)):
        if not name.startswith("mount-" + prefix):
            continue
        task = name[len("mount-" + prefix):]
        bt = os.path.join(pilot_dir, name, "baseline-tree.txt")
        if not os.path.exists(bt):
            continue
        tree = open(bt).read().strip()
        if not tree:
            continue
        rows.append((task, tree != EMPTY_TREE, verdicts.get(task, (None, "absent", None))[1]))
    return rows


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("corpus", help="a Terminal-Bench original-tasks directory")
    ap.add_argument("--json", help="write the full per-task table here")
    ap.add_argument("--calibrate", help="a pilot dir holding mount-<prefix>* runs")
    ap.add_argument("--run-prefix", default="m1-")
    a = ap.parse_args()

    check_pin()

    if not os.path.isdir(a.corpus):
        sys.stderr.write("no corpus directory at %s\n" % a.corpus)
        return EXIT_NO_CORPUS
    tasks = sorted(d for d in os.listdir(a.corpus)
                   if os.path.isdir(os.path.join(a.corpus, d)))
    if not tasks:
        sys.stderr.write("%s holds no task directories\n" % a.corpus)
        return EXIT_NO_CORPUS

    verdicts = {}
    for t in tasks:
        verdicts[t] = classify(os.path.join(a.corpus, t))

    counts = {}
    for _, cls, _ in verdicts.values():
        counts[cls] = counts.get(cls, 0) + 1

    print("workspace-content strata over %d tasks" % len(tasks))
    print("  predicates sha256: %s" % WORKSPACE_PREDICATES_SHA256)
    for cls in ("ships", "ships-none", "undecidable"):
        print("  %-12s %3d" % (cls, counts.get(cls, 0)))

    ws_counts = {}
    for ws, _, _ in verdicts.values():
        ws_counts[ws] = ws_counts.get(ws, 0) + 1
    print("  workspaces: " + ", ".join(
        "%s=%d" % (k, v) for k, v in sorted(ws_counts.items(), key=lambda x: -x[1])))

    if a.calibrate:
        rows = calibrate(a.calibrate, a.run_prefix, verdicts)
        if not rows:
            print("\ncalibration: no mount-%s* runs with a baseline-tree.txt "
                  "under %s" % (a.run_prefix, a.calibrate))
        else:
            print("\ncalibration against %d in-container baseline trees "
                  "(ground truth = the workspace was non-empty when `git init` "
                  "ran)" % len(rows))
            grid = {}
            for _, real, cls in rows:
                grid[(cls, real)] = grid.get((cls, real), 0) + 1
            print("  %-12s %14s %14s" % ("predicted", "baseline REAL", "baseline EMPTY"))
            for cls in ("ships", "ships-none", "undecidable", "absent"):
                r, e = grid.get((cls, True), 0), grid.get((cls, False), 0)
                if r or e:
                    print("  %-12s %14d %14d" % (cls, r, e))
            agree = grid.get(("ships", True), 0) + grid.get(("ships-none", False), 0)
            decided = sum(v for (c, _), v in grid.items()
                          if c in ("ships", "ships-none"))
            if decided:
                print("  agreement on the DECIDED %d: %d (%.1f%%)"
                      % (decided, agree, 100.0 * agree / decided))
            print("  undecidable left undecided: %d"
                  % sum(v for (c, _), v in grid.items() if c == "undecidable"))

    if a.json:
        with open(a.json, "w") as fh:
            json.dump({
                "schema": "thruline.workspace-strata/1",
                "predicates": WORKSPACE_PREDICATES,
                "predicates_sha256": WORKSPACE_PREDICATES_SHA256,
                "corpus": os.path.abspath(a.corpus),
                "tasks": {t: {"workspace": ws, "stratum": cls, "evidence": ev}
                          for t, (ws, cls, ev) in sorted(verdicts.items())},
            }, fh, indent=2, sort_keys=True)
            fh.write("\n")
        print("\nwrote %s" % a.json)

    return EXIT_OK


if __name__ == "__main__":
    sys.exit(main())
