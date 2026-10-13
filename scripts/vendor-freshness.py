#!/usr/bin/env python3
"""Do ThruLine's transcribed Codex facts still match the Codex that is installed?

    scripts/vendor-freshness.py [--verbose]

Every Codex constant in `crates/capture/src/codex.rs`, plus the two residual
assumptions in `scripts/pilot-run.sh`, was hand-copied out of vendor prose or a
`--help` page. A dated comment was the whole of the mitigation, and a date is
not a check: when Codex renames an event or moves its install path, the
constants keep asserting the old world and they assert it *silently*.

This derives each fact from the artefacts actually on this machine -- the
`codex` binary's own bytes, its `--help` grammar, the real install path, the
real sessions tree -- and disagreement is a failure, not a warning.

  NO NETWORK, NO MODEL.  `codex --help` and `codex <sub> --help` are the only
  invocations; `codex doctor` is deliberately NOT used because it probes the
  network. A fact that would need a live model to observe belongs in UNDERIVED,
  never in a check that quietly skips.

EXIT CODES -- only 0 is a pass, and nothing else can be mistaken for one:

  0  VERIFIED   every derivable fact was derived here and agreed
  1  STALE      a transcribed constant disagrees with the installed Codex
  2  BROKEN     the check itself could not run (ThruLine's own constants moved,
                an unclassified vendor constant appeared, the UNDERIVED list
                drifted). Never silently degraded to a skip.
  3  UNCHECKED  at least one fact could not be derived on this machine (codex
                absent, no readable binary, no rollouts yet). Each one is named.
                This is NOT a pass -- see the skip-path note below.

THE SKIP PATH IS THE HAZARD (guard-family instance 9). A control that produces
the expected result through an unrelated mechanism is worse than no control, and
"codex is absent so everything passed" is exactly that shape. So: absence exits
3, never 0; it prints the id and name of every fact it did not check; and the
VERIFIED banner states how many facts were *actually* derived, cross-checked
against the registry -- a derivation that silently no-ops changes the count and
fails rather than reporting green.
"""

# Python 3.9 is still what a bare `/usr/bin/python3` is on macOS, and this
# check must run wherever a developer stands. Deferred annotations keep
# `str | None` legal there.
from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CODEX_RS = ROOT / "crates" / "capture" / "src" / "codex.rs"
PILOT_SH = ROOT / "scripts" / "pilot-run.sh"

VERBOSE = "--verbose" in sys.argv[1:]


# ---------------------------------------------------------------------------
# The registry. Every vendor fact in the tree is in exactly one bucket.
# ---------------------------------------------------------------------------

# Vendor facts this check cannot derive from an installed artefact, each with
# the reason it cannot. Asserted below against the registry, because the
# cheapest way to make a failing derivation green is to demote it to UNDERIVED
# -- and that must cost an edit to this literal, visible in the diff.
UNDERIVED = {
    "U1": (
        "CAPPED_TIMEOUT_MAX_SECS (3 s)",
        "the number is formatted at runtime. The binary carries the message "
        "'clamping SessionEnd hook timeout to ' and NOT the value that follows "
        "it, so the cap can only be observed by timing a real hook that "
        "overruns it -- a measurement, not a constant read.",
    ),
    "U2": (
        "Codex's own 1 s capped / 600 s uncapped hook-timeout defaults",
        "same reason as U1: no static string carries either number. They are "
        "not ThruLine constants; they only justify CAPTURE_TIMEOUT_SECS.",
    ),
    "U3": (
        "that SubagentStop actually fires",
        "no rig can drive a subagent without a live model, and a paid run is "
        "not a check. 8 of the 9 subscribed events are proven by "
        "scripts/hook-event-rig.sh at zero cost; this is the ninth.",
    ),
    "U4": (
        "a vendor event appended AFTER the last one we know",
        "the binary's field table runs the event names together with the next "
        "struct's name, so an appended CamelCase event is ambiguous against it. "
        "Mitigated, not closed: F1 requires one occurrence of the table whose "
        "trailing byte is NOT an uppercase letter, which an appended event "
        "would consume -- and that requirement is itself asserted.",
    ),
}

# Constants in codex.rs that are ThruLine's own choice, not a vendor fact.
# Listed so that "every pub const is classified" is a real assertion.
OURS = {
    "CAPTURE_TIMEOUT_SECS": "our ask on an uncapped event, not a vendor value",
}

# pub consts covered by a derived fact below, keyed to the fact that covers them.
COVERED_BY = {
    "SUBSCRIBED_EVENTS": "F1",
    "UNSUBSCRIBED_EVENTS": "F1",
    "CAPPED_EVENTS": "F3",
    "TRUST_BYPASS_FLAG": "F4",
    "HOOKS_FILE": "F5",
    "TRUST_REVIEW_COMMAND": "F6",
    "CAPPED_TIMEOUT_MAX_SECS": "U1",
}


# ---------------------------------------------------------------------------
# Results
# ---------------------------------------------------------------------------

OK, STALE, UNCHECKED = "ok", "STALE", "unchecked"
rows: list[tuple[str, str, str, str]] = []  # (id, status, title, detail)


def record(fid: str, status: str, title: str, detail: str) -> None:
    rows.append((fid, status, title, detail))


class Broken(Exception):
    """The check cannot run. Never degraded to a skip."""


# ---------------------------------------------------------------------------
# Side A: what the tree transcribes. Read out of the files the compiler and the
# shell read, so a constant and its check cannot be edited apart.
# ---------------------------------------------------------------------------


def rust_source() -> str:
    if not CODEX_RS.is_file():
        raise Broken(f"{CODEX_RS} is missing")
    return CODEX_RS.read_text()


def const_str_list(src: str, name: str) -> list[str]:
    m = re.search(
        r"pub const " + name + r"\s*:\s*&\[&str\]\s*=\s*&\[(.*?)\]\s*;", src, re.S
    )
    if not m:
        raise Broken(f"pub const {name}: &[&str] not found in codex.rs in the expected shape")
    return re.findall(r'"([^"]*)"', m.group(1))


def const_str(src: str, name: str) -> str:
    m = re.search(r'pub const ' + name + r'\s*:\s*&str\s*=\s*"([^"]*)"\s*;', src)
    if not m:
        raise Broken(f"pub const {name}: &str not found in codex.rs in the expected shape")
    return m.group(1)


def declared_pub_consts(src: str) -> list[str]:
    return re.findall(r"^pub const ([A-Z0-9_]+)\s*:", src, re.M)


# ---------------------------------------------------------------------------
# Side B: what the installed Codex says. Derived, never transcribed.
# ---------------------------------------------------------------------------


def find_codex() -> str | None:
    return shutil.which("codex")


def codex_home() -> Path:
    return Path(os.environ.get("CODEX_HOME") or (Path.home() / ".codex"))


def resolve_binary(launcher: str) -> Path | None:
    """The Mach-O/ELF that carries the string tables, not the shim in front of it."""
    candidates = [Path(os.path.realpath(launcher))]
    candidates += sorted(codex_home().glob("packages/standalone/releases/*/bin/codex"))
    for c in candidates:
        try:
            with open(c, "rb") as fh:
                magic = fh.read(4)
        except OSError:
            continue
        # Mach-O (thin + fat, both endians) and ELF.
        if magic in (
            b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf",
            b"\xfe\xed\xfa\xce", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca",
            b"\x7fELF",
        ):
            return c
    return None


def run_help(argv: list[str]) -> str:
    p = subprocess.run(
        argv, capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL
    )
    return p.stdout + p.stderr


def vendor_hook_events(blob, known: list[str]) -> tuple[list[str], bool, bytes]:
    """Derive Codex's hook-event set from its own serde field table.

    `HookEventsToml` is the struct whose field names ARE the hook events, and
    serde leaves those names in the binary as one run-together ASCII table. The
    table is consumed greedily with the names the tree claims; the longest
    consumption wins.

    Returns (events, saw_lowercase_terminated_table, leftover). The flag is U4's
    mitigation: an appended vendor event would be CamelCase and would sit where
    that lowercase byte is.
    """
    order = sorted(known, key=len, reverse=True)
    encoded = [(n, n.encode()) for n in order]

    def consume(pos: int) -> tuple[list[str], int]:
        seen, p = [], pos
        while True:
            for name, raw in encoded:
                if blob[p : p + len(raw)] == raw:
                    seen.append(name)
                    p += len(raw)
                    break
            else:
                return seen, p

    best: list[str] = []
    best_tail = b""
    clean_tail = False
    starts = [m.end() for m in re.finditer(rb"HookEventsToml", blob)]
    # Also start from every occurrence of the first field name, so the check
    # does not depend on the struct name surviving a vendor refactor.
    if starts:
        first_field, _ = consume(starts[0])
        if first_field:
            starts += [m.start() for m in re.finditer(first_field[0].encode(), blob)]
    for s in starts:
        seen, end = consume(s)
        if len(seen) > len(best):
            best, best_tail, clean_tail = seen, blob[end : end + 24], False
        if len(seen) == len(best) and len(seen) > 0:
            nxt = blob[end : end + 1]
            if not (b"A" <= nxt <= b"Z"):
                clean_tail = True
    return best, clean_tail, best_tail


def clamped_events(blob) -> list[str]:
    found = re.findall(rb"clamping ([A-Za-z]+) hook timeout to", blob)
    return sorted({f.decode() for f in found})


# ---------------------------------------------------------------------------
# The pilot wrapper's two residual assumptions.
# ---------------------------------------------------------------------------


def pilot_wrapper() -> str:
    if not PILOT_SH.is_file():
        raise Broken(f"{PILOT_SH} is missing")
    src = PILOT_SH.read_text()
    m = re.search(r"cat > \"\$MOUNT/bin/codex\" <<'WRAP'\n(.*?)\nWRAP\n", src, re.S)
    if not m:
        raise Broken("the codex trust wrapper heredoc was not found in pilot-run.sh")
    return m.group(1)


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------


def main() -> int:
    src = rust_source()

    subscribed = const_str_list(src, "SUBSCRIBED_EVENTS")
    unsubscribed = const_str_list(src, "UNSUBSCRIBED_EVENTS")
    capped = const_str_list(src, "CAPPED_EVENTS")
    bypass_flag = const_str(src, "TRUST_BYPASS_FLAG")
    hooks_file = const_str(src, "HOOKS_FILE")
    review_cmd = const_str(src, "TRUST_REVIEW_COMMAND")

    # Anti-growth, both directions. A new vendor constant with no entry here is
    # a constant nothing checks, which is the whole defect this file exists for.
    classified = set(COVERED_BY) | set(OURS)
    unclassified = [c for c in declared_pub_consts(src) if c not in classified]
    if unclassified:
        raise Broken(
            "codex.rs declares vendor constants this check does not classify: "
            + ", ".join(unclassified)
            + " -- add each to COVERED_BY (derived), UNDERIVED (with a reason), or OURS"
        )
    demoted = [c for c, f in COVERED_BY.items() if f in UNDERIVED]
    if sorted(demoted) != ["CAPPED_TIMEOUT_MAX_SECS"]:
        raise Broken(
            "the UNDERIVED list has drifted: constants mapped to an UNDERIVED reason are "
            + ", ".join(sorted(demoted))
            + " -- a derivation must not be silenced by demoting it"
        )
    if len(UNDERIVED) != 4:
        raise Broken(
            f"UNDERIVED has {len(UNDERIVED)} entries, not the 4 this check declares. "
            "Growing it is allowed; growing it silently is not."
        )

    launcher = find_codex()
    binary = resolve_binary(launcher) if launcher else None
    version = ""
    if launcher:
        version = run_help([launcher, "--version"]).strip().splitlines()[0] if launcher else ""

    print("thruline vendor-freshness — the transcribed Codex facts, re-derived\n")
    print(f"  codex on PATH  : {launcher or 'ABSENT'}")
    print(f"  string source  : {binary or 'unavailable'}")
    print(f"  version        : {version or 'unknown'}")
    print()

    # ---- F1/F3: facts that need the binary's own bytes ---------------------
    if binary is None:
        why = "codex is not on PATH" if not launcher else f"no readable Mach-O/ELF behind {launcher}"
        for fid, title in (
            ("F1", "SUBSCRIBED_EVENTS + UNSUBSCRIBED_EVENTS == Codex's hook-event set"),
            ("F3", "CAPPED_EVENTS == the events Codex says it clamps"),
            ("F5", f"HOOKS_FILE ({hooks_file!r}) is a name Codex knows"),
            ("F6", f"TRUST_REVIEW_COMMAND ({review_cmd!r}) is a name Codex knows"),
        ):
            record(fid, UNCHECKED, title, why)
    else:
        import mmap

        with open(binary, "rb") as fh:
            blob = mmap.mmap(fh.fileno(), 0, access=mmap.ACCESS_READ)
            claimed = list(subscribed) + list(unsubscribed)
            events, clean_tail, tail = vendor_hook_events(blob, claimed)
            missing = [e for e in claimed if e not in events]
            title = "SUBSCRIBED_EVENTS + UNSUBSCRIBED_EVENTS == Codex's hook-event set"
            if missing:
                record(
                    "F1", STALE, title,
                    f"the tree claims {len(claimed)} events; Codex's field table carries "
                    f"{len(events)}. Not present in the installed Codex: {', '.join(missing)}. "
                    f"Derived order: {', '.join(events)}",
                )
            elif not clean_tail:
                record(
                    "F1", STALE, title,
                    "every claimed event was found, but no occurrence of the table ends on a "
                    f"non-uppercase byte -- Codex may have appended an event. Tail: {tail!r}",
                )
            else:
                record("F1", OK, title, f"{len(events)} events, all accounted for")

            derived_capped = clamped_events(blob)
            title = "CAPPED_EVENTS == the events Codex says it clamps"
            if sorted(capped) != derived_capped:
                record(
                    "F3", STALE, title,
                    f"tree: {sorted(capped)}; Codex clamps: {derived_capped}",
                )
            else:
                record("F3", OK, title, f"{derived_capped}")

            for fid, value, const in (
                ("F5", hooks_file, "HOOKS_FILE"),
                ("F6", review_cmd, "TRUST_REVIEW_COMMAND"),
            ):
                title = f"{const} ({value!r}) is a name Codex knows"
                if blob.find(value.encode()) >= 0:
                    record(fid, OK, title, "found verbatim in the binary")
                else:
                    record(fid, STALE, title, "not present anywhere in the installed Codex")
            blob.close()

    # ---- F4: the trust flag, against the binary's own grammar --------------
    title = f"TRUST_BYPASS_FLAG ({bypass_flag}) is accepted by Codex"
    if not launcher:
        record("F4", UNCHECKED, title, "codex is not on PATH")
    else:
        top = run_help([launcher, "--help"])
        if bypass_flag not in top:
            record("F4", STALE, title, "not listed in `codex --help`")
        else:
            record("F4", OK, title, "listed in `codex --help`")

    # ---- F7/F8: the pilot wrapper, RUN rather than read --------------------
    #
    # An earlier draft of this check asked whether the wrapper's text mentioned
    # $PATH. That is a *mention* standing in for a *use* -- guard-family row 3,
    # committed inside the file written to close guard-family row 6. Both facts
    # are now settled by staging the real heredoc as an executable named `codex`,
    # putting its directory FIRST on PATH, and running it. That one arrangement
    # tests everything at once: it must find the real codex (F7), it must not
    # find itself (a self-match hangs or recurses, and the timeout catches it),
    # and it must not reorder argv (F8).
    wrapper = pilot_wrapper()
    t7 = "pilot-run.sh's wrapper finds the codex this machine actually has"
    t8 = "pilot-run.sh's wrapper does not assume subcommand-first argv"
    if not launcher:
        record("F7", UNCHECKED, t7, "codex is not on PATH, so there is nothing to find")
        record("F8", UNCHECKED, t8, "codex is not on PATH, so the wrapper cannot be run")
    else:
        import tempfile

        with tempfile.TemporaryDirectory() as td:
            bindir = Path(td) / "bin"
            bindir.mkdir()
            staged = bindir / "codex"
            staged.write_text(wrapper.rstrip("\n") + "\n")
            staged.chmod(0o755)
            env = dict(os.environ, PATH=f"{bindir}{os.pathsep}{os.environ.get('PATH', '')}")

            def run_wrapper(args: list[str]):
                return subprocess.run(
                    [str(staged), *args], capture_output=True, text=True,
                    timeout=45, stdin=subprocess.DEVNULL, env=env,
                )

            # F7 -- pass-through of a bare global flag; proves the search worked
            # and that the wrapper skipped its own directory.
            resolved = False
            try:
                got = run_wrapper(["--version"])
            except subprocess.TimeoutExpired:
                record("F7", STALE, t7, "the wrapper never returned — it found ITSELF on PATH")
            else:
                if got.returncode == 127:
                    record(
                        "F7", STALE, t7,
                        f"codex is installed at {launcher} and the wrapper exited 127: "
                        f"{got.stderr.strip()!r}. In a container laid out like this machine "
                        "the pilot never runs.",
                    )
                elif got.returncode != 0 or version.split()[-1] not in got.stdout:
                    record(
                        "F7", STALE, t7,
                        f"the wrapper exited {got.returncode} and printed {got.stdout.strip()!r}; "
                        f"the real codex prints {version!r}",
                    )
                else:
                    resolved = True
                    record("F7", OK, t7, f"resolved to a codex reporting {got.stdout.strip()}")

            # F8 -- an argv whose FIRST word is a global option with a separate
            # value. Subcommand-first reordering splits `-c` from `model=x` and
            # Codex exits 2; a straight pass-through exits 0.
            #
            # Gated on F7 deliberately. A wrapper that cannot find codex exits
            # 127 on EVERY argv, so without this gate F8 would report STALE for
            # a mangled argv it never actually mangled -- a verdict reached
            # through an unrelated mechanism, which is the failure this whole
            # file is written against. Observed once, while testing exactly
            # this, and fixed rather than left as a plausible-looking red.
            if not resolved:
                record("F8", UNCHECKED, t8, "F7 failed first: argv never reached codex")
            else:
                try:
                    got = run_wrapper(["-c", "model=thruline-argv-probe", "exec", "--help"])
                except subprocess.TimeoutExpired:
                    record("F8", STALE, t8, "the wrapper never returned on an option-first argv")
                else:
                    if got.returncode != 0:
                        record(
                            "F8", STALE, t8,
                            f"`codex -c model=… exec --help` through the wrapper exited "
                            f"{got.returncode}: "
                            f"{(got.stderr or got.stdout).strip().splitlines()[:2]}. "
                            "The flag is global (`codex [OPTIONS] <COMMAND>`), so inserting it "
                            "after argv[1] lands it between an option and its value.",
                        )
                    else:
                        record("F8", OK, t8, "an option-first argv survives the wrapper intact")

    # ---- F9: the sessions layout ------------------------------------------
    title = "sessions_root layout is <home>/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl"
    root = codex_home() / "sessions"
    if not root.is_dir():
        record("F9", UNCHECKED, title, f"{root} does not exist on this machine")
    else:
        sample = next(root.glob("[0-9][0-9][0-9][0-9]/[0-9][0-9]/[0-9][0-9]/rollout-*.jsonl"), None)
        if sample is None:
            flat = next(root.glob("*.jsonl"), None)
            if flat is not None:
                record("F9", STALE, title, f"rollouts are flat, not date-nested: {flat}")
            else:
                record("F9", UNCHECKED, title, f"{root} holds no rollouts yet")
        else:
            record("F9", OK, title, str(sample.relative_to(root)))

    # ---- report ------------------------------------------------------------
    stale = [r for r in rows if r[1] == STALE]
    unchecked = [r for r in rows if r[1] == UNCHECKED]
    derived = [r for r in rows if r[1] == OK]

    print(f"DERIVED AND AGREED ({len(derived)})")
    for fid, _, title, detail in derived:
        print(f"  {fid}  ok    {title}")
        if VERBOSE:
            print(f"            {detail}")
    if stale:
        print(f"\nSTALE ({len(stale)}) — a transcribed constant no longer matches Codex")
        for fid, _, title, detail in stale:
            print(f"  {fid}  STALE {title}")
            print(f"            {detail}")
    if unchecked:
        print(f"\nNOT CHECKED HERE ({len(unchecked)}) — this is not a pass")
        for fid, _, title, detail in unchecked:
            print(f"  {fid}  ----  {title}")
            print(f"            {detail}")
    print(f"\nUNDERIVED BY CONSTRUCTION ({len(UNDERIVED)}) — no artefact on any machine settles these")
    for uid in sorted(UNDERIVED):
        what, why = UNDERIVED[uid]
        print(f"  {uid}  {what}")
        print(f"      {why}")

    print()
    if stale:
        print(f"STALE — {len(stale)} transcribed Codex fact(s) disagree with the installed Codex.")
        return 1
    if unchecked:
        print(
            f"UNCHECKED — {len(unchecked)} fact(s) could not be derived here, named above. "
            "Not a pass."
        )
        return 3
    print(f"VERIFIED — {len(derived)} Codex facts re-derived from the installed artefacts and agreed.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Broken as exc:
        print(f"\nBROKEN — the check could not run: {exc}", file=sys.stderr)
        sys.exit(2)
