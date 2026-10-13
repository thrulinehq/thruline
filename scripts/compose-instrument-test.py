#!/usr/bin/env python3
"""Unconditional fixture tests for scripts/compose-instrument.py, plus an
optional corpus leg that fails loudly rather than skipping.

Run it:
    python3 scripts/compose-instrument-test.py
    python3 scripts/compose-instrument-test.py --corpus "$TASKS_SRC"

WHY FIXTURES AND NOT A CORPUS-PINNED TEST
-----------------------------------------
The Terminal-Bench corpus tarball is 126 MB and `fixtures/bench/*/` is
gitignored, so a test that needs it runs only on a machine where somebody
fetched it — and a test that quietly passes because it skipped is the exact
Rule 12 failure this program exists to stop. The shapes that matter are a few
hundred bytes each, so they are committed under
`scripts/fixtures/compose-instrument/` and asserted every time.

The corpus leg is opt-in. When it is asked for and the corpus is not there, this
exits non-zero and says so; it never reports a pass it did not earn.

WHAT IS ASSERTED
----------------
accept/  exit 0; the file changed; the sentinel is present; the output parses;
         every whole-line comment survived; the file MODE is unchanged; the
         receipt landed at the path asked for and NOTHING new appeared in the
         task directory.
refuse/  exit 2; the file is BYTE-IDENTICAL to the input (`cmp -s`); no receipt.
fault    an injected internal disagreement exits 3, not 2, and leaves the file
         byte-identical — so a bug in the patcher can never be read as "this
         task's shape is unexpected".
"""

from __future__ import annotations

import argparse
import filecmp
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
TOOL = HERE / "compose-instrument.py"
FIX = HERE / "fixtures" / "compose-instrument"
PY = sys.executable

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


def run_tool(compose: Path, receipt: Path | None, extra=()):
    argv = [PY, str(TOOL), str(compose)]
    if receipt is not None:
        argv += ["--receipt", str(receipt)]
    argv += list(extra)
    return subprocess.run(argv, capture_output=True, text=True)


def full_line_comments(text: str) -> list[str]:
    out = []
    for ln in text.splitlines():
        s = ln.strip()
        if s.startswith("#"):
            out.append(s)
    return out


def test_accept(fixture: Path, tmp: Path):
    name = fixture.name
    print("accept/%s" % name)
    work = tmp / "task"
    work.mkdir()
    dst = work / "docker-compose.yaml"
    shutil.copyfile(fixture, dst)
    os.chmod(dst, 0o644)
    before_text = dst.read_text()
    before_mode = dst.stat().st_mode & 0o7777
    before_listing = sorted(p.name for p in work.iterdir())

    receipt = tmp / "receipts" / (name + ".json")
    r = run_tool(dst, receipt)
    check(r.returncode == 0, "%s exits 0" % name, r.stderr.strip()[:400])
    if r.returncode != 0:
        return

    after_text = dst.read_text()
    check(after_text != before_text, "%s file changed" % name)
    check("/thruline/READY" in after_text, "%s carries the sentinel" % name)
    check((dst.stat().st_mode & 0o7777) == before_mode,
          "%s mode preserved (%o)" % (name, before_mode),
          "now %o" % (dst.stat().st_mode & 0o7777))

    lost = [c for c in full_line_comments(before_text) if c not in after_text]
    check(not lost, "%s comments preserved" % name, "lost: %r" % (lost[:1],))

    check(receipt.is_file(), "%s receipt written outside the task dir" % name)
    check(sorted(p.name for p in work.iterdir()) == before_listing,
          "%s left nothing new in the task directory" % name,
          "now: %s" % sorted(p.name for p in work.iterdir()))

    try:
        import yaml
        doc = yaml.safe_load(after_text)
        parsed = isinstance(doc, dict) and "services" in doc
    except Exception as e:
        doc, parsed = None, False
        print("         parse error: %s" % e)
    check(parsed, "%s output parses" % name)

    if parsed and receipt.is_file():
        rec = json.loads(receipt.read_text())
        svc = doc["services"][rec["client_service"]]
        check(rec["loader"] == "ruamel", "%s receipt records the pinned loader" % name)
        check(all(v in svc["volumes"] for v in rec["volumes_added"]),
              "%s capture mounts present on the selected service" % name)
        check(all(e in svc["environment"] for e in rec["environment_added"]),
              "%s PATH/HOME env present on the selected service" % name)
        check(svc["command"][2] == rec["wrapped_command"],
              "%s command is the wrapped one" % name)
        check(svc["command"][2].endswith("{ " + rec["original_command"] + "; }"),
              "%s the task's own command is brace-wrapped, not discarded" % name)
    shutil.rmtree(work)


def test_refuse(fixture: Path, tmp: Path, expect_rc=2):
    name = fixture.name
    print("refuse/%s" % name)
    work = tmp / "task"
    work.mkdir()
    dst = work / "docker-compose.yaml"
    shutil.copyfile(fixture, dst)
    before_listing = sorted(p.name for p in work.iterdir())
    receipt = tmp / "receipts" / (name + ".json")

    r = run_tool(dst, receipt)
    check(r.returncode == expect_rc, "%s exits %d" % (name, expect_rc),
          "got %d: %s" % (r.returncode, r.stderr.strip()[:300]))
    check(filecmp.cmp(str(fixture), str(dst), shallow=False),
          "%s input byte-identical after refusal" % name)
    check(not receipt.exists(), "%s no receipt on refusal" % name)
    check(sorted(p.name for p in work.iterdir()) == before_listing,
          "%s left nothing new in the task directory" % name)
    check("REFUSED" in r.stderr or "INTERNAL" in r.stderr,
          "%s said why" % name, r.stderr.strip()[:200])
    shutil.rmtree(work)


def test_missing_receipt_flag(tmp: Path):
    print("contract/--receipt is required")
    work = tmp / "task"
    work.mkdir()
    dst = work / "docker-compose.yaml"
    shutil.copyfile(FIX / "accept" / "no-agent-logs-mount.yaml", dst)
    r = run_tool(dst, None)
    check(r.returncode == 2, "no --receipt exits 2", r.stderr.strip()[:200])
    check(filecmp.cmp(str(FIX / "accept" / "no-agent-logs-mount.yaml"), str(dst),
                      shallow=False), "no --receipt leaves the file untouched")
    check(sorted(p.name for p in work.iterdir()) == ["docker-compose.yaml"],
          "no --receipt writes nothing beside the compose file")
    shutil.rmtree(work)


def test_internal_is_exit_3(tmp: Path):
    """Fault injection: make the patcher's own verifier disagree with its edit.

    The draft raised the same exception for this as for an unhandled corpus
    shape, so a bug in the tool reached the caller wearing a refusal's clothes.
    """
    print("contract/internal disagreement is exit 3, not 2")
    work = tmp / "task"
    work.mkdir()
    dst = work / "docker-compose.yaml"
    src = FIX / "accept" / "no-agent-logs-mount.yaml"
    shutil.copyfile(src, dst)
    receipt = tmp / "receipts" / "fault.json"
    code = (
        "import importlib.util,sys\n"
        "spec=importlib.util.spec_from_file_location('ci', %r)\n"
        "m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)\n"
        "real=m.diff\n"
        "m.diff=lambda a,b: real(a,b)+[('add',('services','client','bogus'),None,1)]\n"
        "sys.argv=['ci', %r, '--receipt', %r]\n"
        "sys.exit(m.main())\n" % (str(TOOL), str(dst), str(receipt))
    )
    r = subprocess.run([PY, "-c", code], capture_output=True, text=True)
    check(r.returncode == 3, "injected verifier disagreement exits 3",
          "got %d: %s" % (r.returncode, (r.stderr or r.stdout).strip()[:300]))
    check("INTERNAL ERROR" in r.stderr, "and says it is a bug in the patcher",
          r.stderr.strip()[:200])
    check(filecmp.cmp(str(src), str(dst), shallow=False),
          "internal error leaves the file byte-identical")
    check(not receipt.exists(), "internal error writes no receipt")
    shutil.rmtree(work)


def corpus_leg(corpus: str, tmp: Path):
    root = Path(corpus)
    if not root.is_dir():
        print("\ncorpus leg: FAILED — corpus absent at %s\n"
              "  run fixtures/bench/download.sh, or point --corpus at the\n"
              "  extracted Terminal-Bench tasks directory. This is a failure and\n"
              "  not a skip: a test that passes because it did not run is the\n"
              "  reason this file exists." % root, file=sys.stderr)
        FAILURES.append("corpus absent")
        return
    print("\ncorpus leg: %s" % root.resolve())
    ok = refused = 0
    bad_reason = []
    mutated = []
    work = tmp / "corpus"
    work.mkdir()
    for d in sorted(p for p in root.iterdir() if p.is_dir()):
        cf = d / "docker-compose.yaml"
        if not cf.is_file():
            continue
        dst = work / "docker-compose.yaml"
        shutil.copyfile(cf, dst)
        rec = work / "r.json"
        if rec.exists():
            rec.unlink()
        r = run_tool(dst, rec)
        if r.returncode == 0:
            ok += 1
        else:
            refused += 1
            if r.returncode != 2:
                bad_reason.append("%s exit %d" % (d.name, r.returncode))
            elif "declares no 'command'" not in r.stderr:
                bad_reason.append("%s: %s" % (d.name, r.stderr.strip()[:120]))
            if not filecmp.cmp(str(cf), str(dst), shallow=False):
                mutated.append(d.name)
        dst.unlink()
    total = ok + refused
    print("  instrumented %d/%d, refused %d" % (ok, total, refused))
    check(total > 0, "corpus has compose files")
    check(not mutated, "no corpus refusal mutated its input", str(mutated[:5]))
    check(not bad_reason,
          "every corpus refusal is the documented no-`command:` shape, exit 2",
          "\n         ".join(bad_reason[:5]))
    shutil.rmtree(work)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", default=None,
                    help="also run over a Terminal-Bench tasks directory "
                         "(default: $TASKS_SRC if set)")
    a = ap.parse_args()
    corpus = a.corpus if a.corpus is not None else os.environ.get("TASKS_SRC")

    try:
        import ruamel.yaml  # noqa: F401
        import yaml  # noqa: F401
    except Exception as e:
        print("compose-instrument-test: ruamel.yaml and PyYAML are both required "
              "(%s).\n  Run this with an interpreter that has them — the pilot venv's\n"
              "  python does. Apple's /usr/bin/python3 has neither, and a skip here\n"
              "  would be a pass nobody earned." % e, file=sys.stderr)
        return 4

    if not TOOL.is_file():
        print("compose-instrument-test: %s is missing." % TOOL, file=sys.stderr)
        return 2
    accepts = sorted((FIX / "accept").glob("*.yaml"))
    refuses = sorted((FIX / "refuse").glob("*.yaml"))
    if not accepts or not refuses:
        print("compose-instrument-test: fixtures missing under %s" % FIX, file=sys.stderr)
        return 2

    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        (tmp / "receipts").mkdir()
        for f in accepts:
            test_accept(f, tmp)
        for f in refuses:
            test_refuse(f, tmp)
        test_missing_receipt_flag(tmp)
        test_internal_is_exit_3(tmp)
        if corpus is not None:
            corpus_leg(corpus, tmp)
        else:
            print("\ncorpus leg: not requested (no --corpus, no $TASKS_SRC). The "
                  "fixture assertions above ran unconditionally.")

    print("\n%d checks, %d failures" % (CHECKS, len(FAILURES)))
    if FAILURES:
        for f in FAILURES:
            print("  FAILED: %s" % f)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
