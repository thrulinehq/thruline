#!/usr/bin/env python3
"""Derive the shim-reachability stratum of a Terminal-Bench corpus, from the corpus.

WHY THIS FILE EXISTS
--------------------
`scripts/pilot-run.sh` used to carry this sentence:

    Measured, not reasoned: classify_corpus over all 241 tasks gives
    18/241 without this line and 235/241 with it.

`grep -rl classify_corpus .` returned exactly one file: the comment that cited
it. The tool did not exist, the number could not be re-derived, and every
ceiling figure in the program descended from it. This is `vendor-freshness.py`'s
doctrine (A205) turned on one of our own constants: a number that cannot be
re-derived from the artefacts on the machine is not a measurement, it is a
memory.

So: this script IS `classify_corpus`. It reads `$TASKS_SRC` and prints the
counts with the rule that produced them. Remove a task from the corpus and the
total moves. Change the PATH model and the strata move. Nothing here is typed
in from a previous run.

WHAT IS BEING CLASSIFIED
------------------------
The pilot's capture depends on the container's test script invoking a program
through `/thruline/shim/<prog>` rather than through the real binary. The compose
`environment:` block sets

    PATH=/thruline/bin:/thruline/shim:<system dirs>[:/root/.local/bin]

and the question the stratum answers is: **for how many tasks does at least one
program in `capture::shim::WRAPPED` actually resolve to the shim when
`run-tests.sh` runs?**

Only two things can get in front of `/thruline/shim`, because it is second in
the list and the only entry ahead of it (`/thruline/bin`) holds the codex
wrapper alone. Both are PATH *prepends* performed by the test script itself:

  1. `source $HOME/.local/bin/env` — uv's installer drop-in. Its whole body is
     *prepend `$HOME/.local/bin` unless it is already on PATH*. That directory
     is where uv's installer puts `uv` and `uvx`. Without the `/root/.local/bin`
     tail on the compose PATH, this prepend fires and every later bare `uv`
     resolves to the real binary. With the tail present, the case statement
     matches and the prepend does not happen — the shim stays first. That one
     line is the whole subject of the disputed number.

  2. `source <venv>/bin/activate` — prepends a virtualenv's `bin`, which holds
     `python`, `python3` and (once `pip install pytest` has run) `pytest`. This
     shadows the shim for those three names *regardless* of the compose PATH,
     which is why the "with the line" figure is not 241.

RULE (this is the rule, stated so it can be argued with)
--------------------------------------------------------
For each task with a `run-tests.sh`, simulate PATH over the script's command
words. A task is:

  reached   — at least one WRAPPED program is invoked as a bare command word
              and resolves to `/thruline/shim`.
  shadowed  — at least one WRAPPED program is invoked, but every such
              invocation resolves to a directory prepended ahead of the shim.
  none      — no WRAPPED program is invoked as a bare command word at all
              (an absolute path, or a runner this build does not wrap).

reached + shadowed + none == the number of tasks in the corpus, always; the
script asserts it.

LIMITS, STATED RATHER THAN HIDDEN
---------------------------------
**The wrap policy. This is the limit that matters most, and it was missing.**
The rule above assumes `/thruline/shim` holds a wrapper for every name in
WRAPPED. That is `WrapPolicy::All`, which is not what installs. BUILD-SPEC
§4.4.4's amended predicate ships `WrapPolicy::Resolvable`: a wrapper exists only
where a real program of that name resolved at install time INSIDE THAT TASK'S
IMAGE. This script reads shell text and builds nothing, so it cannot evaluate
that predicate — and `Resolvable` writes a subset of `All`, so every `reached`
count here is an **upper bound** on reach under the shipped policy. The script
re-derives the shipped policy from `crates/capture/src/shim.rs` and says so
above the numbers whenever the two differ. §1's published "241/241" came from
this script and was read as instrument reach; it is withdrawn as such.

This is also a static reading of shell text, not an execution. It splits on `;`,
`&&`, `||`, `|` and newlines, strips leading `VAR=value` assignments, and takes
the first word. It does not evaluate `if`/`for`/`case` control flow, command
substitution, or aliases; a program invoked only inside a false branch is
counted as invoked. `--verbose` prints every task's decision and the invocation
that decided it, so a disagreement is a line to point at rather than a feeling.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# The wrapped set. Kept here as a literal AND checked against the Rust source
# when that source is reachable, because a stratum computed from a stale list is
# the same class of lie this script exists to stop.
# ---------------------------------------------------------------------------
WRAPPED_FALLBACK = [
    "pytest", "cargo", "go", "npm", "pnpm", "yarn", "npx", "make", "mvn",
    "gradle", "dotnet", "uv", "uvx", "python", "python3",
]

SHIM_DIR = "/thruline/shim"
CODEX_DIR = "/thruline/bin"
SYSTEM_DIRS = [
    "/usr/local/sbin", "/usr/local/bin", "/usr/sbin", "/usr/bin", "/sbin", "/bin",
]
UV_INSTALL_DIR = "/root/.local/bin"

# What each kind of PATH directory can provide. Only directories that can be
# prepended AHEAD of the shim matter, so this is deliberately small.
UV_DIR_PROVIDES = {"uv", "uvx"}
VENV_PROVIDES = {"python", "python3", "pytest"}


def read_wrapped_from_rust(repo_root: Path) -> list[str] | None:
    """Re-derive WRAPPED from crates/capture/src/shim.rs, if it is there."""
    src = repo_root / "crates" / "capture" / "src" / "shim.rs"
    if not src.is_file():
        return None
    text = src.read_text(encoding="utf-8")
    m = re.search(r"pub const WRAPPED:\s*&\[&str\]\s*=\s*&\[(.*?)\];", text, re.S)
    if not m:
        return None
    return re.findall(r'"([^"]+)"', m.group(1))


# ---------------------------------------------------------------------------
# THE MODEL, AND WHY IT IS NOT THE SHIPPED POLICY
#
# This script asks "does a bare `uv` in run-tests.sh resolve to /thruline/shim?"
# and answers it by assuming a wrapper is THERE for every name in WRAPPED. That
# is `WrapPolicy::All`.
#
# The installer does not use `WrapPolicy::All`. BUILD-SPEC §4.4.4's amended
# predicate made `WrapPolicy::Resolvable` the default: a wrapper is written only
# where a real program of that name resolves ON THE MACHINE BEING INSTRUMENTED,
# at install time. Whether `pytest` has a wrapper inside task image N is a fact
# about image N, and this script reads shell text — it never builds an image, so
# it cannot know.
#
# The two models are not interchangeable and the difference is one-directional:
# `Resolvable` writes a SUBSET of what `All` writes, so a task reached under
# `Resolvable` is reached under `All` and never the reverse. Everything this
# script prints as `reached` is therefore an UPPER BOUND on reach under the
# policy that actually ships, and the bound is printed saying so.
#
# The shipped default is re-derived below rather than restated, so this comment
# cannot go quietly stale the way §4.4's "241/241" did.
# ---------------------------------------------------------------------------
MODELLED_POLICY = "all"


def read_install_policy_from_rust(repo_root: Path) -> str | None:
    """Re-derive the policy `InstallOptions::under` gives every install.

    Returns the snake_case variant name (`resolvable`, `all`), or None when the
    source is absent. Raises when the source is present and unparseable — a
    silently-defaulted answer here is the whole failure mode.
    """
    src = repo_root / "crates" / "capture" / "src" / "shim.rs"
    if not src.is_file():
        return None
    text = src.read_text(encoding="utf-8")
    body = re.search(r"impl InstallOptions \{(.*?)\n\}\n", text, re.S)
    if not body:
        raise SystemExit(
            "classify-corpus: crates/capture/src/shim.rs has no `impl InstallOptions`.\n"
            "  This script's whole claim about its own model rests on reading the\n"
            "  policy every install uses out of that block. Refusing to print a\n"
            "  reach figure whose model it cannot state."
        )
    m = re.search(r"policy:\s*WrapPolicy::(\w+)", body.group(1))
    if not m:
        raise SystemExit(
            "classify-corpus: `InstallOptions::under` states no `policy: WrapPolicy::…`.\n"
            "  The shim's install-time wrap policy could not be re-derived, so the\n"
            "  model behind these numbers cannot be stated. Refusing to print them."
        )
    return re.sub(r"(?<!^)(?=[A-Z])", "_", m.group(1)).lower()


# ---------------------------------------------------------------------------
# Shell-ish tokenisation.
# ---------------------------------------------------------------------------
SPLIT_RE = re.compile(r"(?:\n|;|&&|\|\||\||&)")
ASSIGN_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")


def statements(script: str):
    """Yield (lineno, words) for each simple command in the script."""
    lineno = 0
    for raw_line in script.splitlines():
        lineno += 1
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        for frag in SPLIT_RE.split(line):
            frag = frag.strip()
            if not frag:
                continue
            words = frag.split()
            # Strip leading shell keywords and env assignments.
            while words and (
                ASSIGN_RE.match(words[0])
                or words[0] in ("if", "then", "else", "elif", "fi", "do", "done",
                                "while", "for", "case", "esac", "!", "time",
                                "exec", "eval", "command", "sudo", "env")
            ):
                words = words[1:]
            if words:
                yield lineno, words


SOURCE_WORDS = ("source", ".")


def classify_script(script: str, wrapped: set[str], with_uv_tail: bool):
    """Return (stratum, deciding_note). Simulates PATH prepends in order."""
    path = [CODEX_DIR, SHIM_DIR] + list(SYSTEM_DIRS)
    if with_uv_tail:
        path.append(UV_INSTALL_DIR)

    # Directories prepended at runtime, most recent first, with what they hold.
    prepends: list[tuple[str, set[str]]] = []

    invoked_any = False
    reached_note = None
    shadow_note = None

    for lineno, words in statements(script):
        head = words[0]

        if head in SOURCE_WORDS and len(words) >= 2:
            target = words[1]
            # uv's env drop-in: prepend $HOME/.local/bin unless already present.
            if re.search(r"(\$HOME|~|\$\{HOME\})/\.local/bin/env$", target) or \
               target.endswith("/.local/bin/env"):
                already = UV_INSTALL_DIR in path or any(
                    d == UV_INSTALL_DIR for d, _ in prepends)
                if not already:
                    prepends.insert(0, (UV_INSTALL_DIR, set(UV_DIR_PROVIDES)))
                continue
            # A virtualenv activation prepends <venv>/bin.
            if target.endswith("/bin/activate") or target.endswith("/activate"):
                vdir = target.rsplit("/", 1)[0]
                prepends.insert(0, (vdir, set(VENV_PROVIDES)))
                continue
            continue

        if "/" in head:
            # Absolute or relative invocation: PATH is not consulted, so the
            # shim is bypassed by construction. Not counted as an invocation of
            # a wrapped program, because the wrapper was never a candidate.
            continue

        if head not in wrapped:
            continue

        invoked_any = True
        shadowing = next((d for d, provides in prepends if head in provides), None)
        if shadowing is None:
            if reached_note is None:
                reached_note = "line %d: `%s` -> %s/%s" % (
                    lineno, " ".join(words[:3]), SHIM_DIR, head)
        else:
            if shadow_note is None:
                shadow_note = "line %d: `%s` -> %s/%s (prepended ahead of the shim)" % (
                    lineno, " ".join(words[:3]), shadowing, head)

    if reached_note is not None:
        return "reached", reached_note
    if invoked_any:
        return "shadowed", shadow_note or "every wrapped invocation was shadowed"
    return "none", "no wrapped program invoked as a bare command word"


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Derive the shim-reachability stratum of a Terminal-Bench corpus.")
    ap.add_argument("tasks_src", nargs="?", default=os.environ.get("TASKS_SRC"),
                    help="the corpus directory (default: $TASKS_SRC)")
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    ap.add_argument("--verbose", action="store_true",
                    help="print every task's decision and the line that decided it")
    a = ap.parse_args()

    if not a.tasks_src:
        print("classify-corpus: no corpus given. Pass a path or set $TASKS_SRC.\n"
              "  There is no default: a default would let this print a number\n"
              "  for a corpus nobody named, which is the failure it exists to stop.",
              file=sys.stderr)
        return 2
    root = Path(a.tasks_src)
    if not root.is_dir():
        print("classify-corpus: %s is not a directory." % root, file=sys.stderr)
        return 2

    repo_root = Path(__file__).resolve().parent.parent
    from_rust = read_wrapped_from_rust(repo_root)
    if from_rust is None:
        wrapped_list = list(WRAPPED_FALLBACK)
        wrapped_src = "fallback literal in this script (crates/capture/src/shim.rs not found)"
    else:
        wrapped_list = from_rust
        wrapped_src = "crates/capture/src/shim.rs::WRAPPED"
        if sorted(from_rust) != sorted(WRAPPED_FALLBACK):
            print("classify-corpus: WRAPPED has drifted from this script's fallback.\n"
                  "  rust : %s\n  here : %s\n"
                  "  Using the Rust list. Update the fallback."
                  % (sorted(from_rust), sorted(WRAPPED_FALLBACK)), file=sys.stderr)
    wrapped = set(wrapped_list)

    tasks = sorted(p.name for p in root.iterdir()
                   if p.is_dir() and not p.name.startswith("."))
    if not tasks:
        print("classify-corpus: %s contains no task directories." % root, file=sys.stderr)
        return 2

    rows = []
    missing = []
    for name in tasks:
        rt = root / name / "run-tests.sh"
        if not rt.is_file():
            missing.append(name)
            rows.append({"task": name, "with_tail": "none", "without_tail": "none",
                         "note_with": "no run-tests.sh", "note_without": "no run-tests.sh"})
            continue
        script = rt.read_text(encoding="utf-8", errors="replace")
        s_with, n_with = classify_script(script, wrapped, with_uv_tail=True)
        s_without, n_without = classify_script(script, wrapped, with_uv_tail=False)
        rows.append({"task": name, "with_tail": s_with, "without_tail": s_without,
                     "note_with": n_with, "note_without": n_without})

    total = len(rows)

    def count(key, val):
        return sum(1 for r in rows if r[key] == val)

    install_policy = read_install_policy_from_rust(repo_root)
    bound_only = install_policy is not None and install_policy != MODELLED_POLICY

    summary = {
        "corpus": str(root.resolve()),
        "tasks_total": total,
        "tasks_without_run_tests": len(missing),
        "wrapped_source": wrapped_src,
        "wrapped": wrapped_list,
        # The model, in the machine-readable output too, so a figure lifted out
        # of this JSON carries its own qualification.
        "wrap_policy_modelled": MODELLED_POLICY,
        "wrap_policy_shipped": install_policy,
        "reached_is_upper_bound_only": bound_only,
        "with_local_bin_tail": {
            "reached": count("with_tail", "reached"),
            "shadowed": count("with_tail", "shadowed"),
            "none": count("with_tail", "none"),
        },
        "without_local_bin_tail": {
            "reached": count("without_tail", "reached"),
            "shadowed": count("without_tail", "shadowed"),
            "none": count("without_tail", "none"),
        },
    }
    for key in ("with_local_bin_tail", "without_local_bin_tail"):
        s = summary[key]
        assert s["reached"] + s["shadowed"] + s["none"] == total, \
            "strata do not sum to the corpus size — the classifier is broken"

    if a.json:
        print(json.dumps({"summary": summary, "tasks": rows}, indent=1))
        return 0

    if a.verbose:
        for r in rows:
            print("%-44s with=%-9s without=%-9s | %s"
                  % (r["task"], r["with_tail"], r["without_tail"], r["note_with"]))
        print()

    w, wo = summary["with_local_bin_tail"], summary["without_local_bin_tail"]
    print("corpus            : %s" % root.resolve())
    print("tasks             : %d" % total)
    if missing:
        print("no run-tests.sh   : %d (%s)" % (len(missing), ", ".join(missing[:5])))
    print("wrapped programs  : %d, from %s" % (len(wrapped_list), wrapped_src))
    print("wrap policy       : modelled `%s`; shipped `%s`%s"
          % (MODELLED_POLICY,
             install_policy if install_policy else "UNKNOWN (shim.rs not found)",
             "  <-- THESE DIFFER" if bound_only else ""))
    print()
    if bound_only:
        # Printed above the numbers, not in a footnote, because it changes what
        # they are. BUILD-SPEC §1 published this script's `reached` as instrument
        # reach ("241/241") while the installer had already moved to the
        # `resolvable` predicate three sections later in the same document.
        print("!! `reached` BELOW IS AN UPPER BOUND, NOT A MEASUREMENT OF REACH.")
        print("   This script assumes a wrapper exists for every WRAPPED name")
        print("   (`WrapPolicy::%s`). The installer writes one only where a real"
              % MODELLED_POLICY)
        print("   program of that name RESOLVES INSIDE THE TASK IMAGE"
              " (`WrapPolicy::%s`)," % install_policy)
        print("   which is a fact about each image and cannot be read out of shell")
        print("   text. `resolvable` writes a subset of `%s`, so true reach is <= the"
              % MODELLED_POLICY)
        print("   number below, by an amount this script does not measure.")
        print("   Do not publish `reached` as instrument coverage.")
        print()
    label = "reached (upper bound)" if bound_only else "reached"
    print("shim reachability, WITHOUT the /root/.local/bin PATH tail:")
    print("  %-21s %3d/%d" % (label, wo["reached"], total))
    print("  %-21s %3d/%d" % ("shadowed", wo["shadowed"], total))
    print("  %-21s %3d/%d" % ("none", wo["none"], total))
    print()
    print("shim reachability, WITH the /root/.local/bin PATH tail:")
    print("  %-21s %3d/%d" % (label, w["reached"], total))
    print("  %-21s %3d/%d" % ("shadowed", w["shadowed"], total))
    print("  %-21s %3d/%d" % ("none", w["none"], total))
    print()
    print("delta attributable to that one PATH entry: %+d tasks"
          % (w["reached"] - wo["reached"]))
    print()
    print("RULE: a task is `reached` when at least one program in WRAPPED is")
    print("invoked as a bare command word and no directory prepended by the test")
    print("script itself (uv's ~/.local/bin/env drop-in, or a venv activate)")
    print("provides that program. `shadowed` means every such invocation was")
    print("prepended over; `none` means no wrapped program is invoked by bare")
    print("name at all. Static reading of shell text, not an execution — see the")
    print("LIMITS section of this file's docstring.")
    if bound_only:
        print()
        print("MODEL: every WRAPPED name is assumed to have a wrapper. The shipped")
        print("installer writes one only where the program resolves in the image.")
        print("`reached` is therefore an upper bound; reach itself is UNMEASURED.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
