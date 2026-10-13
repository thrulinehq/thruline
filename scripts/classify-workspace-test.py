#!/usr/bin/env python3
"""Observed behaviour for `scripts/classify-workspace.py`.

    scripts/classify-workspace-test.py

# Why this exists

`classify-workspace.py` decides which tasks leave a **coverage denominator**.
That is the same authority `scripts/pilot-report.py` has, and the same authority
`scripts/instrument-failures.json` has, and both of those are watched by suites
that `cargo test` drives. This one was not watched by anything.

Track E's own gate reported it, repeatedly and in writing, as the last hole in
its reach: *"it still does not execute `classify-workspace.py`, which has no
suite."* Naming a hole in every gate statement is better than hiding it, and it
is not a substitute for closing it.

# What is checked

Every stratum and every refusal, against **constructed** task directories rather
than the pinned corpus, so the suite needs no 164 MB extraction and cannot go
red because someone re-provisioned. The one thing it deliberately does NOT
re-assert is the calibration figure — that is a measurement over run records
which are not in this repository, and a test that hardcoded 94.1% would be
pinning a number nobody here can re-derive.

Needs only python3. No corpus, no network, no Docker.
"""

import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
TOOL = ROOT / "scripts" / "classify-workspace.py"

PASS = 0
FAIL = 0


def ok(msg):
    global PASS
    print("  ok   %s" % msg)
    PASS += 1


def bad(msg):
    global FAIL
    print("  FAIL %s" % msg)
    FAIL += 1


def head(msg):
    print("\n== %s ==" % msg)


def task(root, name, dockerfile=None, compose=None, extra=None):
    """Construct one task directory."""
    d = root / name
    d.mkdir(parents=True, exist_ok=True)
    if dockerfile is not None:
        (d / "Dockerfile").write_text(dockerfile)
    if compose is not None:
        (d / "docker-compose.yaml").write_text(compose)
    for rel, body in (extra or {}).items():
        p = d / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(body)
    return d


def run(args, cwd=None):
    p = subprocess.run(
        [sys.executable, str(TOOL)] + args,
        cwd=cwd or str(ROOT),
        capture_output=True,
        text=True,
    )
    return p.returncode, p.stdout, p.stderr


# ---------------------------------------------------------------------------
head("the three strata, on constructed tasks")

tmp = pathlib.Path(tempfile.mkdtemp(prefix="classify-workspace-test."))
# REGISTERED AT CREATION, not removed at the end. The `shutil.rmtree` at the
# bottom of this file runs only if nothing above it raises, so an unexpected
# exception leaks the root — the trailing-cleanup shape, in a suite written the
# same day the shape was named. atexit fires on a normal interpreter exit and on
# an unhandled exception; it does NOT fire on SIGKILL, and nothing in-process
# does.
import atexit
atexit.register(lambda: shutil.rmtree(tmp, ignore_errors=True))
corpus = tmp / "original-tasks"
corpus.mkdir(parents=True)

BASE = "FROM ghcr.io/laude-institute/t-bench/ubuntu-24-04:20250624\n"
COMPOSE = (
    "services:\n  client:\n    image: ${T_BENCH_TASK_DOCKER_CLIENT_IMAGE_NAME}\n"
    "    container_name: ${T_BENCH_TASK_DOCKER_CLIENT_CONTAINER_NAME}\n"
)

# ships — content copied into the workspace.
task(corpus, "ships-into-app", BASE + "WORKDIR /app\nCOPY code.py /app\n", COMPOSE)
# ships — a RELATIVE destination is inside the workspace by construction.
task(corpus, "ships-relative", BASE + "WORKDIR /app\nCOPY data.csv .\n", COMPOSE)
# ships-none — nothing copied anywhere.
task(corpus, "ships-nothing", BASE + "WORKDIR /app\nRUN apt-get update\n", COMPOSE)
# undecidable — content exists but lands OUTSIDE the workspace.
task(corpus, "copies-outside", BASE + "WORKDIR /app\nCOPY blob.enc /protected/\n", COMPOSE)
# workspace from the compose file, which overrides the image's WORKDIR.
task(
    corpus,
    "compose-workdir",
    BASE + "WORKDIR /app\nCOPY thing /srv\n",
    COMPOSE + "    working_dir: /srv\n",
)
# no WORKDIR anywhere: the base image's /app is the default.
task(corpus, "no-workdir", BASE + "COPY x /app\n", COMPOSE)

rc, out, err = run([str(corpus), "--json", str(tmp / "strata.json")])
if rc != 0:
    bad("classifying six constructed tasks exited %d: %s" % (rc, err.strip()[:200]))
else:
    ok("classifies a constructed corpus, exit 0")
    got = json.loads((tmp / "strata.json").read_text())["tasks"]
    expect = {
        "ships-into-app": ("ships", "/app"),
        "ships-relative": ("ships", "/app"),
        "ships-nothing": ("ships-none", "/app"),
        "copies-outside": ("undecidable", "/app"),
        "compose-workdir": ("ships", "/srv"),
        "no-workdir": ("ships", "/app"),
    }
    for name, (stratum, workspace) in expect.items():
        g = got.get(name, {})
        if g.get("stratum") == stratum and g.get("workspace") == workspace:
            ok("%-16s -> %-11s in %s" % (name, stratum, workspace))
        else:
            bad(
                "%s -> stratum %r in %r, expected %r in %r"
                % (name, g.get("stratum"), g.get("workspace"), stratum, workspace)
            )

# THE ONE THAT MATTERS MOST, stated as its own case. A task whose content lands
# outside the workspace must NOT be counted as shipping: that is the whole
# discriminator, and calling it `ships` would put a task with an empty workspace
# into the denominator as a real result.
if got.get("copies-outside", {}).get("stratum") == "ships":
    bad(
        "content copied to /protected was counted as shipping into /app. The "
        "stratum would then include tasks whose workspace is empty, which is "
        "the failure the classifier exists to prevent."
    )
else:
    ok("content landing outside the workspace is NOT counted as shipping")

# ---------------------------------------------------------------------------
head("the predicate pin refuses drift, and emits nothing when it does")

mutant = tmp / "mutant.py"
src = TOOL.read_text()
# Widen one predicate — the same shape as widening a coverage exclusion — and
# leave the digest alone.
mutated = src.replace(
    '"ships": "a COPY/ADD destination, or a compose volume target, that is the "',
    '"ships": "ANY COPY/ADD destination at all, wherever it lands, that is the "',
)
if mutated == src:
    bad("the mutation did not apply — the predicate's text changed, so this control proves nothing")
else:
    mutant.write_text(mutated)
    p = subprocess.run(
        [sys.executable, str(mutant), str(corpus)], capture_output=True, text=True
    )
    if p.returncode == 13:
        ok("a widened predicate with an unchanged digest exits 13")
    else:
        bad("a widened predicate exited %d, not 13" % p.returncode)
    if "ships" in p.stdout and "predicates sha256" in p.stdout:
        bad("it printed a classification anyway — the refusal must emit no verdict")
    else:
        ok("and prints no verdict, so a drifted run cannot be read as a result")

# ---------------------------------------------------------------------------
head("refusals")

rc, out, err = run([str(tmp / "no-such-directory")])
if rc == 5:
    ok("a missing corpus exits 5")
else:
    bad("a missing corpus exited %d, not 5" % rc)

empty = tmp / "empty-corpus"
empty.mkdir()
rc, out, err = run([str(empty)])
if rc == 5:
    ok("a corpus with no task directories exits 5, rather than reporting 0 of 0")
else:
    bad("an empty corpus exited %d, not 5" % rc)

# ---------------------------------------------------------------------------
head("calibration is reported, and is not asserted as a number")

# A pilot dir with one run whose baseline tree is REAL and one whose is EMPTY.
pilot = tmp / "pilot"
EMPTY_TREE = "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
for name, tree in (("ships-into-app", "c95e98ed" + "0" * 32), ("ships-nothing", EMPTY_TREE)):
    d = pilot / ("mount-m1-" + name)
    d.mkdir(parents=True)
    (d / "baseline-tree.txt").write_text(tree + "\n")

rc, out, err = run([str(corpus), "--calibrate", str(pilot), "--run-prefix", "m1-"])
if rc != 0:
    bad("calibration exited %d: %s" % (rc, err.strip()[:200]))
else:
    ok("calibrates against baseline trees, exit 0")
    # `ships` predicted a real baseline and `ships-none` predicted an empty one:
    # both agree, so the DECIDED agreement must be total on this fixture.
    if "agreement on the DECIDED 2: 2 (100.0%)" in out:
        ok("and reports agreement on the tasks it decided")
    else:
        bad("expected total agreement on a fixture built to agree:\n%s" % out)
    if "calibration against 2" in out:
        ok("and names how many baseline trees it swept")
    else:
        bad("the calibration did not name its denominator")

# The FAILURE direction: a fixture built to DISAGREE must not report agreement.
disagree = tmp / "pilot-disagree"
d = disagree / "mount-m1-ships-nothing"
d.mkdir(parents=True)
(d / "baseline-tree.txt").write_text("c95e98ed" + "0" * 32 + "\n")
rc, out, err = run([str(corpus), "--calibrate", str(disagree), "--run-prefix", "m1-"])
if "agreement on the DECIDED 1: 0 (0.0%)" in out:
    ok("a task predicted ships-none with a REAL baseline is reported as disagreement")
else:
    bad("a built-in disagreement was not reported as one:\n%s" % out)

shutil.rmtree(tmp, ignore_errors=True)

print("\n%d passed, %d failed" % (PASS, FAIL))
sys.exit(0 if FAIL == 0 else 1)
