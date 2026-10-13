#!/usr/bin/env python3
"""The memory collector's `ps` sample is bounded for liveness, not for speed.

Driven by `crates/cli/tests/verification_memory_sampling.rs` on every
`cargo test -p thruline-cli`; runnable by hand as
`python3 crates/cli/tests/verification_memory_sampling_test.py`.

Three things are checked, and each is the fact a receipt rests on:

1. A `ps` that answers slowly still produces a measurement, and the sample's
   own duration is in the record (`sample_ms`). Before VERIFICATION-MEMORY-TIMEOUT
   a 0.15 s wall clock turned a slow-but-correct sample into
   `RuntimeError('OS process measurement timed out')`, an incomplete
   measurement, and a check that read "unknown" on a loaded machine.
2. A `ps` that never answers is still refused — that is the only remaining
   road to "unknown" from the sampler — and it is refused at the bound.
3. The document the product actually emits carries `sample_ms` on every
   sample and phase, and names the bound it used.

Exits non-zero, loudly, on the first failure. It does not skip: /bin/ps and
python3 are hard dependencies of this collector already.
"""
import atexit
import importlib.util
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time

HELPER = pathlib.Path(__file__).resolve().parents[1] / "src" / "verification_memory.py"

WORK = tempfile.mkdtemp(prefix="thruline-memory-sampling-")
atexit.register(shutil.rmtree, WORK, True)  # registered AT CREATION, not at the bottom

failures = []


def check(name, condition, detail=""):
    print(("ok   " if condition else "FAIL ") + name + (" — " + str(detail) if detail else ""))
    if not condition:
        failures.append(name)


def load():
    spec = importlib.util.spec_from_file_location("thruline_memory_under_test", HELPER)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


class PsShim:
    """Stands in for the `subprocess` module inside the collector, and only for
    `/bin/ps`. Anything else raises rather than being silently let through."""

    TimeoutExpired = subprocess.TimeoutExpired
    PIPE = subprocess.PIPE
    DEVNULL = subprocess.DEVNULL

    def __init__(self, script):
        self.script = script
        self.last = None

    def Popen(self, argv, **kw):
        if argv[:1] != ["/bin/ps"]:
            raise AssertionError("the shim stands in for /bin/ps only, not " + str(argv))
        self.last = subprocess.Popen(["/bin/sh", "-c", self.script, "sh"] + argv[1:], **kw)
        return self.last


def case_slow_ps_is_still_a_measurement():
    mod = load()
    # exec so the shim's pid becomes ps's own pid, which the collector excludes.
    mod.subprocess = PsShim('sleep 1; exec /bin/ps "$@"')
    began = time.monotonic()
    members, sample_ms = mod.snapshot(os.getpid(), os.getpid(), os.getpgrp())
    elapsed_ms = (time.monotonic() - began) * 1000
    check("a ps that takes ~1 s still returns a process inventory", len(members) > 0, len(members))
    check("this process is in it", any(m["pid"] == os.getpid() for m in members))
    check("the sample's own duration is recorded and is >= 1000 ms", sample_ms >= 1000, sample_ms)
    check("sample_ms is a whole number of ms", isinstance(sample_ms, int), type(sample_ms).__name__)
    check("sample_ms is the sample, not the run", sample_ms <= elapsed_ms + 50, (sample_ms, elapsed_ms))


def case_a_ps_that_never_answers_is_unknown():
    mod = load()
    mod.SAMPLE_LIVENESS_SECONDS = 1.0  # the shipped bound, shortened so the suite is quick
    mod.subprocess = PsShim("sleep 600")
    began = time.monotonic()
    try:
        mod.snapshot(os.getpid(), os.getpid(), os.getpgrp())
        check("a ps that never answers is refused", False, "it returned a measurement")
        return
    except RuntimeError as e:
        waited = time.monotonic() - began
        check("a ps that never answers is refused", True, str(e))
        check("the refusal names the liveness bound", "liveness bound" in str(e), str(e))
        check("it waited for the whole bound first", waited >= 1.0, round(waited, 3))
        check("and no longer than it needed", waited < 5.0, round(waited, 3))
    check(
        "the killed sampler is not left running",
        mod.subprocess.last.poll() is not None,
        mod.subprocess.last.poll(),
    )


def case_the_shipped_bound_is_seconds():
    mod = load()
    check(
        "the shipped bound is seconds, not the old 0.15 s speed bound",
        mod.SAMPLE_LIVENESS_SECONDS >= 5.0,
        mod.SAMPLE_LIVENESS_SECONDS,
    )


def case_the_emitted_document_carries_sample_ms():
    work = pathlib.Path(WORK) / "collect"
    work.mkdir(parents=True, exist_ok=True)
    (work / "workload.py").write_text(
        "import time\ndef run_cycle(size):\n    data=bytearray(size)\n"
        "    for i in range(0,size,4096): data[i]=1\n    time.sleep(.05)\n"
    )
    plan = {
        "kind": "resource",
        "module": "workload.py",
        "workload": "Controlled buffer touches, retained until each cycle completes",
        "size": 1048576,
        "size_unit": "bytes per cycle",
        "warmup_cycles": 1,
        "cycles": 1,
        "repetitions": 1,
        "sampled_peak_budget_bytes": 134217728,
        "retained_growth_budget_bytes": 33554432,
    }
    out = subprocess.run(
        [sys.executable, "-I", "-B", str(HELPER), json.dumps(plan)],
        cwd=work,
        capture_output=True,
        text=True,
        start_new_session=True,  # the collector requires its own process group
        timeout=120,
    )
    doc = json.loads(out.stdout)
    check("the collector completed", doc["issues"] == [], doc["issues"])
    check("the bound it used is in the document", doc.get("sample_liveness_bound_ms") == 5000, doc.get("sample_liveness_bound_ms"))
    rows = [r for r in doc["repetitions"] for r in list(r["samples"]) + list(r["phases"])]
    check("there are samples to inspect", len(rows) > 0, len(rows))
    check(
        "every sample and phase carries its own sample_ms",
        all(isinstance(r.get("sample_ms"), int) and r["sample_ms"] >= 0 for r in rows),
        [r.get("sample_ms") for r in rows][:8],
    )


for case in (
    case_slow_ps_is_still_a_measurement,
    case_a_ps_that_never_answers_is_unknown,
    case_the_shipped_bound_is_seconds,
    case_the_emitted_document_carries_sample_ms,
):
    print("--- " + case.__name__)
    try:
        case()
    except Exception as e:  # a case that cannot run is a failing case, never a skip
        import traceback

        traceback.print_exc()
        check(case.__name__ + " ran at all", False, repr(e))

if failures:
    print("\n%d FAILED: %s" % (len(failures), ", ".join(failures)))
    sys.exit(1)
print("\nall checks passed")
