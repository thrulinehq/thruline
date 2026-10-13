#!/usr/bin/env python3
"""Run `packaging/action/action.yml` for real, over receipts that are really
verified, and watch every state it can produce.

WHY THIS FILE EXISTS
--------------------
BUILD-SPEC §9's Action was written, reviewed and committed without ever being
executed: no workflow referenced it, so its step bodies had never run once.
That is the shape this program keeps finding — a guard nobody watched fail —
and it had already cost something here: the policy step invoked
`thruline policy check`, which is not a verb (clap rejects `check` as an
unexpected positional), so the §9 policy gate could not pass for anyone who
enabled it. A test that only reads the YAML cannot see that; only running it
can.

WHAT IT IS NOT
--------------
It does not re-implement the Action. Every step body executed here is read out
of `action.yml` at run time — the same shell, in the same order, with the same
inputs GitHub would pass. If a step is edited, this harness runs the edit. If a
step is deleted, `EXPECTED_STEPS` below fails loudly rather than quietly
testing less.

WHAT NEEDS GITHUB AND IS THEREFORE NOT PROVEN HERE
--------------------------------------------------
`gh` is stubbed. The two calls that reach GitHub's API — `POST
repos/{owner}/{repo}/check-runs` and `gh pr comment` — are recorded and their
payloads asserted, but the API's own acceptance of that payload is NOT tested
by this file and cannot be without a repository, a token and a pull request.
Every other observable state is real: the binary runs, the receipts are signed,
the badges are the bytes the binary emitted.

Usage:  python3 packaging/action/selftest.py [--thruline PATH]
Exit 0 only when every scenario observed the state it expected.
"""

from __future__ import annotations

import argparse
import atexit
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    import yaml
except ImportError:  # pragma: no cover - environment problem, reported loudly
    sys.exit("PyYAML is required: python3 -m pip install pyyaml")

HERE = Path(__file__).resolve().parent
ACTION_YML = HERE / "action.yml"

# The steps this harness knows how to drive. A step added to the Action and not
# added here would otherwise be executed but never asserted about, and a step
# removed would silently shrink the test — Rule 12 in both directions.
EXPECTED_STEPS = [
    "Install ThruLine",
    "Check the clone is deep enough to verify against",
    # **The policy runs BEFORE the receipt is rendered** (operator ruling,
    # 2026-09-04): its conclusion is an input to the one conclusion, not a
    # second verdict posted beside it.
    "Evaluate the policy",
    "Verify the receipt",
    "Render the badge",
    "Post the check run",
    "Post the PR comment",
    "Fail the job when policy says to",
]


# ---------------------------------------------------------------------------
# The runner: GitHub's composite-action semantics, only as far as this Action
# actually uses them, and loud about anything it does not understand.
# ---------------------------------------------------------------------------


class Unsupported(RuntimeError):
    """The Action used an expression this harness cannot evaluate.

    Raised rather than guessed at: a harness that silently treats an unknown
    `if:` as false stops testing the step and says nothing.
    """


def load_action() -> dict:
    doc = yaml.safe_load(ACTION_YML.read_text())
    names = [s.get("name") for s in doc["runs"]["steps"]]
    if names != EXPECTED_STEPS:
        raise SystemExit(
            "action.yml's steps changed and this harness was not updated.\n"
            f"  action.yml: {names}\n  selftest.py: {EXPECTED_STEPS}"
        )
    return doc


ACTION = load_action()
DEFAULT_INPUTS = {k: str(v.get("default", "")) for k, v in ACTION["inputs"].items()}


def resolve(expr_text: str, ctx: dict) -> str:
    """Substitute `${{ ... }}` expressions from a context."""

    def one(m: re.Match) -> str:
        expr = m.group(1).strip()
        if expr in ctx:
            return str(ctx[expr])
        raise Unsupported(f"no value for expression {expr!r}")

    return re.sub(r"\$\{\{(.+?)\}\}", one, expr_text)


def truthy_if(cond: str, ctx: dict) -> bool:
    """Evaluate the `if:` forms this Action uses, and refuse the rest."""
    cond = cond.strip()
    if cond.startswith("${{") and cond.endswith("}}"):
        cond = cond[3:-2].strip()
    for clause in cond.split("&&"):
        m = re.fullmatch(r"\s*([\w.\-]+)\s*(==|!=)\s*'([^']*)'\s*", clause)
        if not m:
            raise Unsupported(f"cannot evaluate `if:` clause {clause!r}")
        lhs, op, rhs = m.group(1), m.group(2), m.group(3)
        if lhs not in ctx:
            raise Unsupported(f"no value for {lhs!r} in `if:`")
        got = str(ctx[lhs])
        if (op == "==" and got != rhs) or (op == "!=" and got == rhs):
            return False
    return True


def stub_gh(dir_: Path, log: Path) -> None:
    """A `gh` that records what it was asked to do and does nothing.

    The real calls need a repository, a token and a pull request. Recording the
    argv and the piped payload is the most this machine can honestly observe.
    """
    dir_.mkdir(parents=True, exist_ok=True)
    gh = dir_ / "gh"
    gh.write_text(
        "#!/usr/bin/env bash\n"
        'printf "ARGV %s\\n" "$*" >> "$GH_LOG"\n'
        'for a in "$@"; do\n'
        '  if [ "$a" = "--input" ]; then next=1; continue; fi\n'
        '  if [ "${next:-}" = "1" ]; then cp "$a" "$GH_LOG.payload"; next=0; fi\n'
        "done\n"
        "exit 0\n"
    )
    gh.chmod(0o755)
    log.write_text("")


class Run:
    def __init__(self) -> None:
        self.steps: list[tuple[str, int, str]] = []
        self.outputs: dict[str, dict[str, str]] = {}
        self.summary = ""
        self.gh_argv: list[str] = []
        self.check_run_payload: dict | None = None

    def exit_code(self) -> int:
        return next((code for _, code, _ in self.steps if code != 0), 0)

    def log(self) -> str:
        return "\n".join(f"  [{c}] {n}\n{o.rstrip()}" for n, c, o in self.steps if o or c)

    def step(self, name: str) -> tuple[int, str] | None:
        for n, c, o in self.steps:
            if n == name:
                return (c, o)
        return None


def run_action(
    workdir: Path,
    inputs: dict[str, str],
    *,
    thruline_bin: Path,
    home: Path,
    event_name: str = "pull_request",
    base_ref: str = "",
    extra_path: Path | None = None,
) -> Run:
    """Execute the Action's steps in `workdir`, exactly as written."""
    unknown = set(inputs) - set(DEFAULT_INPUTS)
    if unknown:
        raise SystemExit(f"unknown Action inputs: {sorted(unknown)}")
    values = dict(DEFAULT_INPUTS, **{k: str(v) for k, v in inputs.items()})

    tmp = _mkdtemp("tl-action-")
    bin_dir = tmp / "bin"
    gh_log = tmp / "gh.log"
    stub_gh(bin_dir, gh_log)
    # `thruline` is on PATH the way a released binary would be.
    (bin_dir / "thruline").symlink_to(thruline_bin)

    out_file = tmp / "outputs"
    summary_file = tmp / "summary.md"
    out_file.write_text("")
    summary_file.write_text("")

    run = Run()
    path = [str(bin_dir)]
    if extra_path:
        path.insert(0, str(extra_path))
    base_env = dict(os.environ)
    base_env.update(
        {
            "PATH": os.pathsep.join(path + [base_env.get("PATH", "")]),
            "GITHUB_OUTPUT": str(out_file),
            "GITHUB_STEP_SUMMARY": str(summary_file),
            # **The head under review is the repository's own HEAD**, because
            # that is what a runner is in: `actions/checkout` checks out
            # `GITHUB_SHA`, so the workspace and the variable agree by
            # construction.
            #
            # This was the literal `cafebabe…`, a commit that exists in no
            # repository on earth. Nothing read `GITHUB_SHA` at the time, so the
            # fixture modelled a state that cannot occur and nobody noticed.
            # The moment the verifier began binding the receipt to the reviewed
            # head, the harness's green scenario became unreachable — not
            # because the product was wrong, but because the fixture was.
            #
            # Taken from the core's `aa69892`, which fixed this file directly.
            # It is the same failure as this session's own [7b]: a fixture that
            # could not reach the behaviour under test, passing for that reason.
            "GITHUB_SHA": head_sha(workdir),
            # The runner sets this on a pull_request run and `thruline policy`
            # defaults to it. Set to match `base_ref` so the harness cannot
            # accidentally pass through an inherited value from the machine
            # running the tests.
            "GITHUB_BASE_REF": base_ref,
            "GITHUB_EVENT_NAME": event_name,
            "GITHUB_REPOSITORY": "thrulinehq/thruline",
            "GH_LOG": str(gh_log),
            "THRULINE_HOME": str(home),
        }
    )
    base_env.pop("CLAUDE_CODE_SESSION_ID", None)

    for step in ACTION["runs"]["steps"]:
        name = step["name"]
        ctx = {
            "github.token": "x-token",
            "github.action_path": str(HERE),
            "github.event_name": event_name,
            # **The harness never simulated a pull request's base**, which is
            # why it could not see that the policy step had no base to read
            # from. Default "" is a non-pull-request event; a scenario that
            # means to be a PR passes one.
            "github.base_ref": base_ref,
            "github.event.pull_request.number": "7",
        }
        ctx.update({f"inputs.{k}": v for k, v in values.items()})
        for sid, outs in run.outputs.items():
            for k, v in outs.items():
                ctx[f"steps.{sid}.outputs.{k}"] = v
        # A step whose id has not run yet still has referenceable outputs in
        # GitHub — they are empty strings, not errors.
        for other in ACTION["runs"]["steps"]:
            oid = other.get("id")
            if oid:
                for key in ("conclusion", "status", "receipt", "path"):
                    ctx.setdefault(f"steps.{oid}.outputs.{key}", "")

        if "if" in step and not truthy_if(str(step["if"]), ctx):
            run.steps.append((name, 0, "(skipped by `if:`)"))
            continue

        env = dict(base_env)
        for k, v in (step.get("env") or {}).items():
            env[k] = resolve(str(v), ctx)

        body = resolve(step["run"], ctx)
        script = tmp / "step.sh"
        script.write_text(body)
        proc = subprocess.run(
            ["bash", "--noprofile", "--norc", "-e", "-o", "pipefail", str(script)],
            cwd=workdir,
            env=env,
            capture_output=True,
            text=True,
        )
        run.steps.append((name, proc.returncode, proc.stdout + proc.stderr))

        sid = step.get("id")
        if sid:
            parsed = {}
            for line in out_file.read_text().splitlines():
                if "=" in line:
                    k, v = line.split("=", 1)
                    parsed[k] = v
            run.outputs[sid] = parsed
            out_file.write_text("")

        if proc.returncode != 0:
            break

    run.summary = summary_file.read_text()
    run.gh_argv = gh_log.read_text().splitlines()
    payload = Path(str(gh_log) + ".payload")
    if payload.exists():
        run.check_run_payload = json.loads(payload.read_text())
    return run


# ---------------------------------------------------------------------------
# Fixtures: real repositories, real sessions, real signed receipts.
# ---------------------------------------------------------------------------


def git(cwd: Path, *args: str) -> None:
    subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True)


def thruline(bin_: Path, cwd: Path, home: Path, *args: str, stdin: str = "") -> subprocess.CompletedProcess:
    env = dict(os.environ, THRULINE_HOME=str(home))
    env.pop("CLAUDE_CODE_SESSION_ID", None)
    return subprocess.run(
        [str(bin_), *args], cwd=cwd, env=env, input=stdin, capture_output=True, text=True
    )


def head_sha(repo: Path) -> str:
    """The repository's own HEAD, or a sha no repository has when it has none.

    An unborn HEAD is a real state for some of the scenarios below (the empty
    repository), and there the honest answer is a sha that resolves nowhere.
    """
    try:
        out = subprocess.run(
            ["git", "rev-parse", "--verify", "HEAD"],
            cwd=repo, capture_output=True, text=True, check=True,
        )
        return out.stdout.strip()
    except Exception:
        return "cafebabecafebabecafebabecafebabecafebabe"


def build_fixture(
    root: Path,
    bin_: Path,
    *,
    message: str = "Added the rate function.",
    sid: str = "77777777-8888-4999-8aaa-bbbbbbbbbbbb",
) -> tuple[Path, Path, Path]:
    """A repository with one captured session and a signed receipt for it.

    `message` is the agent's final sentence, and it decides which state this
    fixture can reach — which is the whole point of the operator's ruling of
    2026-09-04. The default sentence yields a receipt with **no recognised
    claim**, so its conclusion is the integrity axis alone and `success` is
    reachable. `"All tests pass."` with no test check in the ledger yields an
    `unverifiable` verdict and can only reach `neutral`.

    **Scenario 1 used to pass `"All tests pass."` and expect `success`.** That
    expectation was already false at `10dae44` — `verify` had started refusing
    to call an `unverifiable` verdict verified, `crates/cli/tests/m2_action.rs`
    was updated to match, and this harness was not. Nothing noticed, because
    the only thing that runs this file is `.github/workflows/receipt.yml`,
    which has never executed on GitHub.
    """
    repo = root / "repo"
    home = root / "home"
    repo.mkdir(parents=True)
    home.mkdir(parents=True)
    git(repo, "init", "-q", "-b", "main")
    git(repo, "config", "user.email", "a@e.invalid")
    git(repo, "config", "user.name", "A")
    (repo / "app.py").write_text("def rate(w):\n    return 2.5 * w\n")
    git(repo, "add", "-A")
    git(repo, "commit", "-qm", "first")

    for payload in (
        {
            "hook_event_name": "SessionStart",
            "session_id": sid,
            "transcript_path": "/t",
            "cwd": str(repo),
            "source": "startup",
        },
        {
            "hook_event_name": "Stop",
            "session_id": sid,
            "transcript_path": "/t",
            "cwd": str(repo),
            "last_assistant_message": message,
            "stop_hook_active": False,
        },
    ):
        out = thruline(bin_, repo, home, "hook", "claude-code", stdin=json.dumps(payload))
        if out.returncode != 0:
            raise SystemExit(f"hook failed: {out.stderr}")
    for args in (("fold",),):
        out = thruline(bin_, repo, home, *args)
        if out.returncode != 0:
            raise SystemExit(f"{args} failed: {out.stderr}")

    receipts = repo / ".thruline" / "receipts"
    receipts.mkdir(parents=True)
    out = thruline(bin_, repo, home, "receipt", "--out", str(receipts / "latest.json"))
    if out.returncode != 0:
        raise SystemExit(f"receipt failed: {out.stderr}")
    # Committed, because §9's zero-install path is "the developer committed a
    # receipt path" — an uncommitted receipt is not in the clone CI checks out,
    # and a fixture that skipped this step would have tested a case no CI run
    # can reach.
    git(repo, "add", "-A")
    git(repo, "commit", "-qm", "receipt")
    return repo, home, receipts / "latest.json"


def copy_workspace(src: Path, dst: Path) -> None:
    """Copy a fixture repo the way a fresh checkout arrives: no run artefacts."""
    shutil.copytree(src, dst)
    for junk in ("thruline-badge.svg", "check-run.json", "receipt-table.md"):
        (dst / junk).unlink(missing_ok=True)


def tamper(src: Path, dst: Path) -> None:
    """Change the signed payload without the key: the signature must fail."""
    import base64

    env = json.loads(src.read_text())
    payload = json.loads(base64.b64decode(env["payload"]))
    payload["predicate"]["session"]["event_chain_head"] = "0" * 64
    env["payload"] = base64.b64encode(json.dumps(payload).encode()).decode()
    dst.write_text(json.dumps(env))


# ---------------------------------------------------------------------------
# Scenarios.
# ---------------------------------------------------------------------------

# **Every temp root this harness creates, removed on every exit path.**
#
# `tempfile.mkdtemp` never auto-deletes, and both roots here were leaked: one
# per `run_action` and one per run. `crates/cli/tests/m2_action.rs` runs this on
# every gate, so every gate left them behind — measured in a gate's TMPDIR as
# 246 `tl-action-*` and 20 `tl-selftest-*` roots.
#
# `atexit` rather than `try/finally` around each site: it covers the exception
# and `SystemExit` paths too, and this file exits through `SystemExit` in a
# dozen places. `ignore_errors` because a reaper that raises on the way out
# would replace a disk leak with a lost exit code.
_TEMP_ROOTS: list[Path] = []


def _reap_temp_roots() -> None:
    for root in _TEMP_ROOTS:
        shutil.rmtree(root, ignore_errors=True)


atexit.register(_reap_temp_roots)


def check_every_temp_root_is_registered() -> None:
    """No `mkdtemp` in this file escapes the registry — audited, not asserted.

    Track Z asked for "one line confirming every path selftest.py creates goes
    through `_mkdtemp`". The honest answer is that **it does not**: the probe in
    `check_reaper_cannot_hide_a_leak` calls `mkdtemp` directly, because it has
    to survive the reaper in order to test it. The invariant is not "everything
    goes through the constructor", it is **"every root reaches `_TEMP_ROOTS`"**,
    and the probe reaches it by appending itself afterwards.

    A line in a message confirming that goes stale the first time someone adds a
    call. Track E's own sweep missed 758 roots from code written the same day by
    the person who had just fixed five other sites — *only auditing the call
    sites finds the class*. So this audits them, on every run.

    It parses this file with `ast` rather than grepping for `mkdtemp`: a text
    search matches the word in this docstring, which is the comment-matching
    hazard `REPORT.md` records five times. An AST call node cannot be a comment.
    """
    import ast

    src = Path(__file__).read_text()
    tree = ast.parse(src)
    allowed = {"_mkdtemp", "check_reaper_cannot_hide_a_leak"}
    offenders: list[str] = []
    registers: set[str] = set()

    for fn in ast.walk(tree):
        if not isinstance(fn, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        for node in ast.walk(fn):
            if isinstance(node, ast.Call):
                f = node.func
                name = (
                    f.attr if isinstance(f, ast.Attribute)
                    else f.id if isinstance(f, ast.Name)
                    else ""
                )
                if name == "mkdtemp" and fn.name not in allowed:
                    offenders.append(f"{fn.name}() at line {node.lineno}")
                # `_TEMP_ROOTS.append(...)` — the registry's only entry point
                # besides `_mkdtemp`.
                if (
                    name == "append"
                    and isinstance(f, ast.Attribute)
                    and isinstance(f.value, ast.Name)
                    and f.value.id == "_TEMP_ROOTS"
                ):
                    registers.add(fn.name)

    if offenders:
        FAILURES.append(
            "temp-root registry — SCOPE: `mkdtemp` calls in THIS file only, not "
            "subprocesses and not the tempfile crate. Unregistered call site(s): "
            f"{offenders}. A root made outside `_mkdtemp` must append itself to "
            "_TEMP_ROOTS, or the reaper never learns it exists and the guard "
            "never sees it survive"
        )
    if "check_reaper_cannot_hide_a_leak" not in registers:
        FAILURES.append(
            "temp-root registry: the probe no longer registers itself, so the one "
            "root deliberately made outside `_mkdtemp` is now outside the registry too"
        )


def check_reaper_cannot_hide_a_leak() -> None:
    """The reaper may swallow its error; the guard may not.

    `_reap_temp_roots` passes `ignore_errors=True` so a failure on the way out
    cannot replace a disk leak with a lost exit code. That trade is deliberate
    and it has a consequence: **the reaper is silent when it fails.**

    The core hit exactly this in the Rust `tempfile` crate — A208's protected
    store writes `seal.part` and `payload.part` at `0o400`, `remove_dir_all`
    gets `EACCES`, the `TempDir` destructor ignores it, and every run touching
    that store leaked its temp directory permanently. 5,775 entries, ~6 GiB.
    Nothing said so, because the only thing that could have said so was the
    call that ignored the error.

    So the guard does not check the removal call. It checks **whether the root
    is gone**, which is the property, and this proves the distinction holds by
    building a root the reaper genuinely cannot remove.

    The obvious guard — assert `rmtree` succeeded — is the one this failure
    defeats, which is why it is written down here rather than assumed.

    **And the obvious FIX is the other one: a `rmtree` at the end of the
    function.** Track E measured that shape in `class_two_corpora.rs:820`, where
    `tl-measures-*` had a cleanup at :844 — placed after an `assert_eq!`. It
    therefore ran on every passing run and on no failing one, so exactly one
    root sat on disk while the sites with no cleanup at all had 1,583. A
    trailing call leaks precisely when a run fails, which is when its temp
    directory is most worth having and least likely to be missed.
    
    `_reap_temp_roots` is registered with `atexit`, so it covers the panic, the
    exception and the `SystemExit` paths by construction, and there is nothing
    to place after an assertion. Said here because the next reader will reach
    for the trailing call, and it looks like the smaller change.
    """
    # **The one path in this file made outside `_mkdtemp`, and deliberately:**
    # it has to survive the reaper to test the reaper. So it is registered as a
    # backstop *after* the probe, below — the registry is the thing that catches
    # what a cleanup misses, and a probe exempt from it would be the one root
    # nothing watches.
    probe = Path(tempfile.mkdtemp(prefix="tl-reaper-probe-"))
    inner = probe / "mirror"
    inner.mkdir()
    sealed = inner / "seal.part"
    sealed.write_text("x")
    try:
        os.chmod(sealed, 0o400)
        os.chmod(inner, 0o500)  # A208's shape: no write bit on the directory
        raised = False
        try:
            shutil.rmtree(probe, ignore_errors=True)  # exactly what the reaper does
        except OSError:
            raised = True

        if not probe.exists():
            # The platform removed it anyway; the trade has no consequence here
            # and there is nothing for the guard to catch. Reported, because a
            # probe that cannot reach its condition must say so rather than
            # read as a pass.
            print("  note: this filesystem removed a 0o400-under-0o500 tree — "
                  "the reaper-silence probe is INCONCLUSIVE here, not passing")
            INCONCLUSIVE.append(
                "reaper-silence probe: this filesystem removed a 0o400-under-0o500 "
                "tree, so the reaper's silent-failure trade was never exercised"
            )
            return

        # The two halves of the trade, both now observed on this machine:
        if raised:
            FAILURES.append(
                "reaper-probe: rmtree(ignore_errors=True) raised. The reaper is "
                "supposed to be silent; if it can raise, it can lose an exit code"
            )
        # ...and being silent, it left the root behind. The guard's predicate is
        # `r.exists()`, evaluated here on the same value it would see, so this
        # ties the probe to the check rather than restating the line above.
        seen_by_guard = [r for r in (probe,) if r.exists()]
        if seen_by_guard and not raised:
            # **Say the condition was reached.** A silent pass here is
            # indistinguishable from a probe that skipped, and "it printed
            # nothing" is the shape this whole harness exists to refuse.
            print("  ok   reaper is silent AND ineffective on a 0o400 tree; "
                  "the guard's existence predicate catches it")
        if not seen_by_guard:
            FAILURES.append(
                "reaper-probe: the reaper failed silently and the guard's own "
                "predicate did not see the surviving root — the check has stopped "
                "checking the property the reaper cannot report"
            )
    finally:
        # Each step independently, because a raising `chmod` would otherwise
        # skip the `rmtree` below it — a cleanup that leaks exactly when
        # something went wrong, which is the shape Track E found at
        # `class_two_corpora.rs:844` and the one this function is about. It
        # would have been an ugly place to reproduce it.
        for step in (
            lambda: os.chmod(inner, 0o700),
            lambda: os.chmod(sealed, 0o600),
            lambda: shutil.rmtree(probe, ignore_errors=True),
        ):
            try:
                step()
            except OSError:
                pass
        # Registered whatever happened. If the cleanup above worked this is a
        # no-op; if it did not, the reaper retries and the guard reports the
        # survivor by name. The probe does not get to be the one root outside
        # the registry.
        _TEMP_ROOTS.append(probe)
    if probe.exists():
        FAILURES.append(f"reaper-probe: could not clean up its own probe at {probe}")


def _mkdtemp(prefix: str) -> Path:
    """`mkdtemp`, registered for removal. The only way this file makes one."""
    root = Path(tempfile.mkdtemp(prefix=prefix))
    _TEMP_ROOTS.append(root)
    return root


FAILURES: list[str] = []
# A probe that RAN but reached no verdict. Separate from FAILURES, which is for
# things that are wrong, and from the OBSERVED states table, which is about
# check-run conclusions. Printed at the end regardless, because a probe that
# returned early is exactly the shape Track E found passing green in its own
# suite the same night: it printed its skip into a harness with no way to hear
# it. Mid-run prose is not a report — the reader looks at the end.
INCONCLUSIVE: list[str] = []
OBSERVED: list[tuple[str, str, str, str, str]] = []


def supports_base(bin_: Path) -> bool:
    """Does this binary's `policy` take `--base`?

    Since core `c37760d` the policy for a pull request is read from the base
    branch, and this harness simulates a real pull request — so against an
    older binary the `--base` the Action now passes is an unknown argument, the
    policy step prints nothing, and the scenarios fail with six assertions that
    all say the same thing and none of which say *version skew*.

    Probed rather than assumed, and the scenarios that need it are SKIPPED
    LOUDLY rather than silently dropped or left to fail obscurely.
    """
    out = subprocess.run(
        [str(bin_), "policy", "--help"], capture_output=True, text=True
    )
    return "--base" in out.stdout


def expect(scenario: str, what: str, got, want) -> None:
    ok = got == want
    print(f"  {'ok  ' if ok else 'FAIL'} {what}: {got!r}" + ("" if ok else f" (wanted {want!r})"))
    if not ok:
        FAILURES.append(f"{scenario}: {what} was {got!r}, wanted {want!r}")


def badge_label(repo: Path) -> str:
    svg = repo / "thruline-badge.svg"
    if not svg.exists():
        return "(no badge)"
    m = re.findall(r">([a-z ]+)<", svg.read_text())
    return m[-1] if m else "(unreadable)"


def badge_colour(repo: Path) -> str:
    svg = repo / "thruline-badge.svg"
    if not svg.exists():
        return "-"
    # The only six-digit colour in the badge is the state colour: the frame
    # uses #555 and #fff.
    cols = re.findall(r'fill="(#[0-9a-f]{6})"', svg.read_text())
    return cols[0] if cols else "-"


def record(scenario: str, run: Run, repo: Path) -> None:
    OBSERVED.append(
        (
            scenario,
            run.outputs.get("verify", {}).get("conclusion", "-") or "-",
            run.outputs.get("verify", {}).get("status", "-") or "-",
            f"{badge_label(repo)} {badge_colour(repo)}",
            str(run.exit_code()),
        )
    )


def run_head_binding_scenario(bin_: Path, drepo: Path, dhome: Path) -> None:
    """Ruling 14 — a genuine receipt from ANOTHER history is not evidence here.

    The core's `aa69892` binds a receipt to the reviewed head: its commit must
    be an ancestor of `$GITHUB_SHA`, with only the receipt's own path allowed to
    differ. Ruling 14 puts the Action's half here — pass `--head-sha`, and
    require the full clone the ancestry needs — and this asserts the binding
    fires *through the Action*, which is the only part the Action owns.

    Before the binding existed this exact shape verified `success`: every object
    a hostile branch can reach is in the object store, so "is the tree in the
    store" was satisfied by a receipt about an entirely different history.

    **Probed by the check's own name, not by a version.** A binary without the
    binding emits no `head_binding` check, and this SKIPS LOUDLY rather than
    asserting a protection that binary does not claim to have.
    """
    print("\n[7d] a genuine receipt from an unrelated history")
    other = drepo.parent / "unrelated"
    orepo, ohome, oreceipt = build_fixture(
        other, bin_, sid="99999999-8888-4999-8aaa-cccccccccccc"
    )
    foreign = drepo / ".thruline" / "receipts" / "foreign.json"
    foreign.parent.mkdir(parents=True, exist_ok=True)
    foreign.write_bytes(Path(oreceipt).read_bytes())

    probe = subprocess.run(
        [str(bin_), "verify-receipt", str(foreign), "--offline", "--format", "json",
         "--head-sha", head_sha(drepo)],
        cwd=drepo, capture_output=True, text=True,
        env=dict(os.environ, THRULINE_HOME=str(dhome)),
    )
    names = []
    try:
        names = [c.get("name") for c in json.loads(probe.stdout).get("checks", [])]
    except Exception:
        pass
    if "head_binding" not in names:
        banner = (
            "SKIPPED [7d]: this `thruline` emits no `head_binding` check, so it does not "
            "bind a receipt to the reviewed head. NOT CHECKED here: that a genuine "
            "receipt from an unrelated history is refused — the shape that verified "
            "`success` before core aa69892. Build a binary at 29f83b6 or later."
        )
        print("\n" + "!" * 78 + "\n" + banner + "\n" + "!" * 78)
        OBSERVED.append(("foreign-history", "SKIPPED", "core too old", "-", "-"))
        foreign.unlink()
        return

    run = run_action(
        drepo,
        {"receipt": ".thruline/receipts/foreign.json",
         "key": str(Path(ohome) / "signing.pub")},
        thruline_bin=bin_,
        home=dhome,
    )
    print(run.log())
    conclusion = run.outputs.get("verify", {}).get("conclusion")
    expect("foreign-history", "conclusion is not success", conclusion != "success", True)
    badge = drepo / "thruline-badge.svg"
    expect(
        "foreign-history",
        "badge does not say verified",
        ">verified<" not in badge.read_text() if badge.exists() else True,
        True,
    )
    OBSERVED.append(("foreign-history", conclusion or "-", "-", "-", str(run.exit_code())))
    foreign.unlink()


def run_policy_scenarios(
    bin_: Path, drepo: Path, dhome: Path, recs: Path, *, base_rule: bool
) -> None:
    """[7b] the policy gate firing, and [7c] its refusal when there is no base.

    `base_rule` is whether this binary has core `c37760d`'s base-branch rule.
    [7b] does not need it — the Action passes no `--base`, so an older core
    reads the policy from the checkout and reaches the same verdict here. [7c]
    asserts the refusal itself and is SKIPPED LOUDLY without it, rather than
    failing with assertions that all say the same thing and none of which say
    *version skew*.
    """
    # **The fixture is built BEFORE the branch**, because both paths need it.
    # It was after, so the old-binary path ran the gate against a policy file
    # that did not exist yet and the binary printed nothing — a harness that
    # fails for its own setup order, reported as the product failing.
    thruline(bin_, drepo, dhome, "policy", "--init")
    strict = drepo / ".thruline" / "policy.yml"
    strict.write_text(
        strict.read_text().replace('paths: ["src/**", "lib/**"]', 'paths: ["**"]')
    )
    # **The receipt is REBUILT under the new policy.** Without this the
    # `failure` below would come from the `policy_hash` check — the receipt
    # naming a policy the repository no longer has — and this scenario
    # would observe a red it did not earn, proving nothing about ruling 3.
    out = thruline(bin_, drepo, dhome, "receipt", "--out", str(recs / "latest.json"))
    if out.returncode != 0:
        raise SystemExit(f"demo receipt failed: {out.stderr}")
    # **A pull request has a base branch, and since core `c37760d` the
    # policy is read from it.** This scenario used to run with no base at
    # all and pass, because the harness never set `GITHUB_EVENT_NAME` and
    # the binary therefore never applied the pull-request rule. The moment
    # the harness started simulating a real pull request, this went red —
    # correctly: `thruline policy` refuses a `pull_request` run with no base
    # rather than judging the change by the policy it may have just written.
    #
    # So the fixture grows the branch a pull request actually has. The
    # strict policy is committed to `main`, the session's work stays on
    # `feature`, and the base is where the gate reads from. That is also the
    # only shape in which the F8 rule can be observed at all.
    git(drepo, "add", "-A")
    git(drepo, "commit", "-qm", "strict policy on the base branch")
    # `origin/main` at this commit and the work on a feature branch: the
    # shape `actions/checkout` leaves on a pull request, where the base
    # exists only as a remote-tracking ref. `update-ref` rather than
    # `branch -f`, because HEAD is already on the default branch and git
    # refuses to force-update the branch it is on — measured, not guessed.
    git(drepo, "update-ref", "refs/remotes/origin/main", "HEAD")
    git(drepo, "checkout", "-qb", "feature")

    if not base_rule:
        # The Action REFUSES against this binary, on purpose. Assert the refusal
        # rather than skipping silently: it is the security property, and it is
        # the one thing about this path that IS checkable here.
        print("\n[7b/7c] this core has no base rule: the Action must refuse the gate")
        run = run_action(
            drepo,
            {"key": str(dhome / "signing.pub"), "policy": ".thruline/policy.yml"},
            thruline_bin=bin_,
            home=dhome,
            base_ref="main",
        )
        print(run.log())
        expect("no-base-rule", "job exit", run.exit_code(), 1)
        expect(
            "no-base-rule",
            "the refusal names why a head-read policy is not a gate",
            "weaken the policy that judges it" in run.log(),
            True,
        )
        # And on a `push`, where there is no base and no self-edit to make, the
        # SAME binary must still run the gate. The refusal is scoped to the
        # event that carries the attack; scoping it wider would fail work that
        # was never exposed.
        print("\n[7b-push] the same old binary on a push: the gate still runs")
        run = run_action(
            drepo,
            {"key": str(dhome / "signing.pub"), "policy": ".thruline/policy.yml"},
            thruline_bin=bin_,
            home=dhome,
            event_name="push",
        )
        print(run.log())
        expect("cheating-session", "policy conclusion", run.outputs["policy"]["conclusion"], "failure")
        expect("cheating-session", "receipt conclusion", run.outputs["verify"]["conclusion"], "failure")
        expect("cheating-session", "job exit", run.exit_code(), 1)
        record("cheating-session", run, drepo)
        banner = (
            "SKIPPED [7c] only: this `thruline` predates the base-branch rule, so on a "
            "pull request the Action refuses the gate outright (asserted above) and on a "
            "push it still runs it (also asserted above). NOT CHECKED here: that the gate "
            "fires from the BASE policy rather than the head, and that a pull_request run "
            "with no base ref refuses with its reason. Build a binary at c37760d or later."
        )
        print("\n" + "!" * 78 + "\n" + banner + "\n" + "!" * 78)
        OBSERVED.append(("no-base-ref", "SKIPPED", "core too old", "-", "-"))
        return

    print("\n[7b] the same session, now judged by .thruline/policy.yml")
    run = run_action(
        drepo,
        {"key": str(dhome / "signing.pub"), "policy": ".thruline/policy.yml"},
        thruline_bin=bin_,
        home=dhome,
        base_ref="main",
    )
    print(run.log())
    # **The finding this scenario used to enshrine, now inverted.** Before
    # the operator's ruling of 2026-09-04 this block asserted
    # `receipt conclusion: success` and `badge: verified` over a session
    # whose `tests_pass` claim is UNSUPPORTED — the receipt was genuine, so
    # the receipt-level axis was green, and only the opt-in policy step
    # went red. Two verdicts on one pull request, and the louder one, on
    # the most public surface the product has, was the wrong one.
    #
    # Ruling points 3 and 5: with a policy present the conclusion comes
    # from the policy, and `failure` comes only from there.
    expect(
        "cheating-session",
        "policy conclusion",
        run.outputs.get("policy", {}).get("conclusion"),
        "failure",
    )
    expect(
        "cheating-session",
        "receipt conclusion",
        run.outputs.get("verify", {}).get("conclusion"),
        "failure",
    )
    expect("cheating-session", "badge", badge_label(drepo), "not verified")
    expect("cheating-session", "badge colour", badge_colour(drepo), "#cf222e")
    expect(
        "cheating-session",
        "posted conclusion",
        (run.check_run_payload or {}).get("conclusion"),
        "failure",
    )
    expect("cheating-session", "job exit", run.exit_code(), 1)
    expect(
        "cheating-session",
        "table names the unsupported claim",
        "**unsupported**" in run.summary,
        True,
    )
    record("cheating-session", run, drepo)

    # ------------------------------------------------------------------
    # [7c] a pull request with NO base ref: the gate refuses, and says why.
    # ------------------------------------------------------------------
    #
    # Since core `c37760d` this is a hard refusal rather than a fall back to
    # the head — the whole point of the F8 ruling. What is asserted here is
    # the Action's half: it must fail CLOSED (a gate that cannot run has not
    # passed) **and** carry the binary's reason into the annotation, because
    # the reason is on stderr and names the fix.
    print("\n[7c] a pull_request run with no base ref: refused, with the reason")
    run = run_action(
        drepo,
        {"key": str(dhome / "signing.pub"), "policy": ".thruline/policy.yml"},
        thruline_bin=bin_,
        home=dhome,
        base_ref="",
    )
    print(run.log())
    expect("no-base-ref", "job exit", run.exit_code(), 1)
    expect(
        "no-base-ref",
        "no conclusion was invented",
        run.outputs.get("policy", {}).get("conclusion"),
        None,
    )
    log = run.log()
    expect(
        "no-base-ref",
        "the annotation carries the binary's own reason",
        "no base ref is available" in log,
        True,
    )
    expect(
        "no-base-ref",
        "and it names the rule rather than only the symptom",
        "read from the base branch" in log,
        True,
    )
    # **Record the row on the path where the scenario actually RAN.**
    # Until this line, `no-base-ref` appended to OBSERVED only in its SKIPPED
    # branch at the capability probe — so an old binary put it in the table and
    # a new binary, where all four assertions above genuinely execute and pass,
    # left it out entirely. The table lost exactly the run that was evidence.
    # Its sibling `run_head_binding_scenario` had this right on both paths; this
    # one had it on one. Caught by noticing the table shrink from ten rows to
    # nine when the newer binary closed the skip.
    #
    # Not `record()`: that reads `badge_label(drepo)`, and this scenario writes
    # no badge — it would report the PREVIOUS scenario's badge on this repo as
    # if it belonged to this row. `-` is the honest value for a badge never
    # written, and the conclusion is `-` because the assertion above is that
    # none was invented.
    OBSERVED.append(("no-base-ref", "-", "-", "-", str(run.exit_code())))


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--thruline", default=shutil.which("thruline"))
    args = ap.parse_args()
    if not args.thruline:
        return int(bool(print("no `thruline` on PATH; pass --thruline PATH")))
    # **Run the checks that depend on nothing, before the ones that depend on
    # everything.** These two need no binary, no fixture and no network. Until
    # this moved they were called at the END of `main`, behind `build_fixture`,
    # which needs all three — so a stale `thruline` on PATH raised `SystemExit`
    # at the first hook call and took both guards with it. Observed, not
    # supposed: run against a binary with no `hook` subcommand, the suite
    # aborted and the source audit never executed. A guard hidden by a
    # dependency it does not have is the same defect one layer up from the one
    # it was written to catch.
    check_every_temp_root_is_registered()
    check_reaper_cannot_hide_a_leak()
    if FAILURES:
        # Reported HERE rather than left for the end block, which the abort path
        # never reaches. Moving a check earlier without moving its report would
        # trade a hidden check for a swallowed one.
        print("\nFAILED (before any fixture was built):")
        for f in FAILURES:
            print(f"  - {f}")
        return 1

    bin_ = Path(args.thruline).resolve()
    root = _mkdtemp("tl-selftest-")
    print(f"thruline: {bin_}\nscratch:  {root}\n")

    repo, home, receipt = build_fixture(root / "base", bin_)

    # --- 1. verified ------------------------------------------------------
    print("[1] a receipt verified on the machine that issued it")
    # The green state, and it is reachable only because this receipt carries no
    # claim the record leaves unproved. That is the operator's ruling of
    # 2026-09-04 in one line: `success` is a statement about the claims too,
    # not only about the signature.
    run = run_action(repo, {"key": str(home / "signing.pub")}, thruline_bin=bin_, home=home)
    print(run.log())
    expect("verified", "conclusion", run.outputs.get("verify", {}).get("conclusion"), "success")
    expect("verified", "status", run.outputs.get("verify", {}).get("status"), "verified")
    expect("verified", "badge", badge_label(repo), "verified")
    expect("verified", "badge colour", badge_colour(repo), "#2ea043")
    expect("verified", "job exit", run.exit_code(), 0)
    expect(
        "verified",
        "check run posted",
        any("check-runs" in a for a in run.gh_argv),
        True,
    )
    expect(
        "verified",
        "posted conclusion",
        (run.check_run_payload or {}).get("conclusion"),
        "success",
    )
    expect("verified", "PR comment posted", any("pr comment" in a for a in run.gh_argv), True)
    record("verified", run, repo)

    # --- 1b. an `unverifiable` verdict is never green ---------------------
    print("\n[1b] the same repository, with the agent claiming `All tests pass.`")
    # No test check was captured, so the receipt's own verdict reads
    # `unverifiable`. §6: never collapse that into `supported`. This is the
    # state scenario 1 used to assert as GREEN.
    unver_root = root / "unverifiable"
    unver_repo, unver_home, _ = build_fixture(
        unver_root,
        bin_,
        message="All tests pass.",
        sid="66666666-7777-4888-8999-aaaaaaaaaaaa",
    )
    run = run_action(
        unver_repo, {"key": str(unver_home / "signing.pub")}, thruline_bin=bin_, home=unver_home
    )
    print(run.log())
    expect("unverifiable", "conclusion", run.outputs.get("verify", {}).get("conclusion"), "neutral")
    expect("unverifiable", "badge", badge_label(unver_repo), "partial")
    expect("unverifiable", "badge colour", badge_colour(unver_repo), "#bf8700")
    expect("unverifiable", "job exit", run.exit_code(), 0)
    expect(
        "unverifiable",
        "posted conclusion",
        (run.check_run_payload or {}).get("conclusion"),
        "neutral",
    )
    record("unverifiable", run, unver_repo)

    # --- 2. partial: the ordinary CI case ---------------------------------
    print("\n[2] the same receipt on a runner with no ledger and no key")
    clone = root / "clone"
    subprocess.run(["git", "clone", "-q", str(repo), str(clone)], check=True)
    empty_home = root / "empty-home"
    empty_home.mkdir()
    run = run_action(clone, {}, thruline_bin=bin_, home=empty_home)
    print(run.log())
    expect("partial", "conclusion", run.outputs.get("verify", {}).get("conclusion"), "neutral")
    expect("partial", "status", run.outputs.get("verify", {}).get("status"), "partial")
    expect("partial", "badge", badge_label(clone), "partial")
    expect("partial", "badge colour", badge_colour(clone), "#bf8700")
    expect("partial", "job exit (fail-on: failure)", run.exit_code(), 0)
    record("partial", run, clone)

    print("\n[2b] the same run with `fail-on: neutral`")
    run = run_action(clone, {"fail-on": "neutral"}, thruline_bin=bin_, home=empty_home)
    expect("partial-strict", "job exit", run.exit_code(), 1)
    expect(
        "partial-strict",
        "failing step",
        [n for n, c, _ in run.steps if c][0],
        "Fail the job when policy says to",
    )

    # --- 3. tampered ------------------------------------------------------
    print("\n[3] a receipt whose payload was edited after signing")
    bad_repo = root / "tampered"
    copy_workspace(repo, bad_repo)
    tamper(receipt, bad_repo / ".thruline" / "receipts" / "latest.json")
    run = run_action(
        bad_repo, {"key": str(home / "signing.pub")}, thruline_bin=bin_, home=home
    )
    print(run.log())
    expect("tampered", "conclusion", run.outputs.get("verify", {}).get("conclusion"), "failure")
    expect("tampered", "status", run.outputs.get("verify", {}).get("status"), "failed")
    expect("tampered", "badge", badge_label(bad_repo), "not verified")
    expect("tampered", "badge colour", badge_colour(bad_repo), "#cf222e")
    expect("tampered", "job exit", run.exit_code(), 1)
    record("tampered", run, bad_repo)

    # --- 4. no receipt at all --------------------------------------------
    print("\n[4] a repository with no receipt")
    none_repo = root / "no-receipt"
    copy_workspace(repo, none_repo)
    shutil.rmtree(none_repo / ".thruline")
    run = run_action(none_repo, {}, thruline_bin=bin_, home=home)
    print(run.log())
    expect("no-receipt", "job exit", run.exit_code(), 1)
    expect("no-receipt", "failing step", [n for n, c, _ in run.steps if c][0], "Verify the receipt")
    expect("no-receipt", "badge", badge_label(none_repo), "(no badge)")
    expect(
        "no-receipt",
        "says what to do",
        "No ThruLine receipt found" in (run.step("Verify the receipt") or (0, ""))[1],
        True,
    )
    record("no-receipt", run, none_repo)

    # --- 5. shallow clone -------------------------------------------------
    print("\n[5] a shallow checkout, which is what actions/checkout gives by default")
    shallow = root / "shallow"
    subprocess.run(
        ["git", "clone", "-q", "--depth", "1", f"file://{repo}", str(shallow)], check=True
    )
    run = run_action(shallow, {}, thruline_bin=bin_, home=empty_home)
    step = run.step("Check the clone is deep enough to verify against")
    expect("shallow", "job exit", run.exit_code(), 1)
    expect("shallow", "names the fix", "fetch-depth: 0" in (step or (0, ""))[1], True)
    expect("shallow", "badge", badge_label(shallow), "(no badge)")
    print("\n[5b] the same clone with `require-full-clone: false`")
    run = run_action(shallow, {"require-full-clone": "false"}, thruline_bin=bin_, home=empty_home)
    step = run.step("Check the clone is deep enough to verify against")
    expect("shallow-allowed", "warns", "::warning" in (step or (0, ""))[1], True)
    expect(
        "shallow-allowed",
        "conclusion",
        run.outputs.get("verify", {}).get("conclusion"),
        "neutral",
    )
    record("shallow-allowed", run, shallow)

    # --- 6. the badge guard, watched failing ------------------------------
    print("\n[6] a binary whose badge disagrees with its own conclusion")
    liar = root / "liar"
    liar.mkdir()
    (liar / "thruline").write_text(
        "#!/usr/bin/env bash\n"
        "# A `thruline` that concludes `neutral` and draws a green badge — the\n"
        "# disagreement the badge step exists to refuse.\n"
        'for a in "$@"; do if [ "$a" = "badge" ]; then\n'
        '  echo "<svg><text>thruline</text><text>verified</text></svg>"; exit 2; fi; done\n'
        f'exec {bin_} "$@"\n'
    )
    (liar / "thruline").chmod(0o755)
    guard_repo = root / "guard"
    copy_workspace(clone, guard_repo)
    run = run_action(guard_repo, {}, thruline_bin=bin_, home=empty_home, extra_path=liar)
    step = run.step("Render the badge")
    expect("badge-guard", "job exit", run.exit_code(), 1)
    expect("badge-guard", "failing step", [n for n, c, _ in run.steps if c][0], "Render the badge")
    expect("badge-guard", "refuses", "disagrees with the conclusion" in (step or (0, ""))[1], True)

    # --- 7. the §9 policy gate over a cheating session ---------------------
    print("\n[7] the demo's cheating session: `tests_pass` is unsupported")
    demo_root = root / "demo"
    demo_root.mkdir()
    demo_home = demo_root / "home"
    demo_home.mkdir()
    out = thruline(bin_, demo_root, demo_home, "demo", "--dir", str(demo_root / "d"))
    if out.returncode != 0:
        print("  SKIPPED: `thruline demo` could not run (no test runner?):")
        print("  " + (out.stderr or out.stdout).strip().replace("\n", "\n  "))
        FAILURES.append("cheating-session: SKIPPED, so the gate was not watched catching one")
    else:
        drepo = demo_root / "d" / "repo"
        dhome = demo_root / "d" / "home"
        recs = drepo / ".thruline" / "receipts"
        recs.mkdir(parents=True, exist_ok=True)

        # --- 8 FIRST, with no policy file in the repository ---------------
        # Ruling points 2 and 4: the check run may never conclude `success`
        # while a claim is `unsupported`; with no policy to judge it, that is
        # `neutral` **with the verdict table rendered** — never green, and
        # never red, because deciding that an unproved claim may not merge is
        # the repository's call and this repository has not made one.
        #
        # It runs before the policy file exists on purpose. §8 binds the
        # policy digest into the receipt, so a receipt issued under one policy
        # and verified under another fails the `policy_hash` check — which
        # would make this scenario red for a reason that has nothing to do
        # with the claim it is about.
        out = thruline(bin_, drepo, dhome, "receipt", "--out", str(recs / "latest.json"))
        if out.returncode != 0:
            raise SystemExit(f"demo receipt failed: {out.stderr}")
        print("\n[8] the same cheating session with NO policy file")
        run = run_action(
            drepo, {"key": str(dhome / "signing.pub")}, thruline_bin=bin_, home=dhome
        )
        print(run.log())
        payload = run.check_run_payload or {}
        output = payload.get("output") or {}
        expect(
            "unsupported-no-policy",
            "conclusion",
            run.outputs.get("verify", {}).get("conclusion"),
            "neutral",
        )
        expect("unsupported-no-policy", "badge", badge_label(drepo), "partial")
        expect("unsupported-no-policy", "badge colour", badge_colour(drepo), "#bf8700")
        expect("unsupported-no-policy", "posted conclusion", payload.get("conclusion"), "neutral")
        expect("unsupported-no-policy", "job exit", run.exit_code(), 0)
        expect(
            "unsupported-no-policy",
            "the verdict table is rendered",
            "| claim | verdict | why |" in output.get("text", ""),
            True,
        )
        expect(
            "unsupported-no-policy",
            "the table marks the claim unsupported",
            "**unsupported**" in output.get("text", ""),
            True,
        )
        expect(
            "unsupported-no-policy",
            "the title does not say the receipt is verified",
            "NOT supported" in output.get("title", ""),
            True,
        )
        record("unsupported-no-policy", run, drepo)

        # --- 7 continues: now with the policy in force --------------------
        # The policy the spec prints requires `tests_pass` only for changes
        # under src/** or lib/**. This repository is flat, so that rule never
        # fires — see the note in README.md. The policy used here covers it.
        run_head_binding_scenario(bin_, drepo, dhome)

        # **Both policy scenarios now need the base rule**, because the Action
        # refuses to run its gate without it: a gate the change under review can
        # edit is not a weaker gate, it is not a gate. That refusal is itself
        # asserted below, so the skip covers the scenarios and not the rule.
        run_policy_scenarios(bin_, drepo, dhome, recs, base_rule=supports_base(bin_))



    # --- what was observed -------------------------------------------------
    print("\nOBSERVED STATES")
    print(f"  {'scenario':<20} {'conclusion':<11} {'status':<9} {'badge':<22} job")
    for row in OBSERVED:
        print(f"  {row[0]:<20} {row[1]:<11} {row[2]:<9} {row[3]:<22} {row[4]}")

    # **The harness proves it left nothing behind**, before it decides its own
    # exit. Checked here and not only in `m2_action.rs`, because this file is
    # what a person runs by hand and the leak it guards against was invisible
    # until someone counted a gate's TMPDIR.
    #
    # SCOPE, stated because a guard whose reach is unstated gets trusted past
    # it: THIS harness's two prefixes, created by THIS process. It says nothing
    # about the Rust `tempfile` crate's `.tmp*` roots, which are a separate and
    # larger accumulation and not this file's to answer for.
    # Reap FIRST, then check. The first version of this guard checked before
    # `atexit` had run — it verified a cleanup that had not happened yet and
    # failed every clean run. `atexit` still covers the exception and
    # `SystemExit` paths, which `main` returning normally does not reach; what
    # this proves is that the normal path leaves nothing.
    _reap_temp_roots()
    leaked = [r for r in _TEMP_ROOTS if r.exists()]
    if leaked:
        FAILURES.append(
            f"{len(leaked)} temp root(s) survived — SCOPE: tl-action-*/tl-selftest-* "
            f"from THIS process only, not the tempfile crate's .tmp* roots. mkdtemp "
            f"never auto-deletes and m2_action runs this on every gate, so a leak here "
            f"accrues once per gate forever: {[str(r) for r in leaked[:3]]}"
        )

    if INCONCLUSIVE:
        print("\nINCONCLUSIVE — ran, reached no verdict, and is NOT evidence:")
        for i in INCONCLUSIVE:
            print(f"  - {i}")

    if FAILURES:
        print("\nFAILED:")
        for f in FAILURES:
            print(f"  - {f}")
        return 1
    skipped = [row[0] for row in OBSERVED if row[1] == "SKIPPED"]
    if skipped:
        # The old sentence was unconditional and would have said "every state
        # above was observed" over rows that say SKIPPED. A summary line that
        # overstates its own table is the failure this product exists against,
        # printed by the harness that checks for it.
        n = len(OBSERVED) - len(skipped)
        print(
            f"\n{n} state{'' if n == 1 else 's'} above "
            f"{'was' if n == 1 else 'were'} observed. "
            f"{len(skipped)} {'was' if len(skipped) == 1 else 'were'} SKIPPED and "
            f"{'is' if len(skipped) == 1 else 'are'} NOT evidence: {', '.join(skipped)}."
        )
    else:
        # The states table is fully observed even when a probe was inconclusive —
        # a probe is not a state. But this is the LAST line, and an unqualified
        # all-clear under an INCONCLUSIVE block reads as a global one. The same
        # correction was already made once above for SKIPPED rows.
        tail = (
            f" {len(INCONCLUSIVE)} probe{'' if len(INCONCLUSIVE) == 1 else 's'} "
            f"above reached no verdict and {'is' if len(INCONCLUSIVE) == 1 else 'are'} "
            "NOT evidence."
            if INCONCLUSIVE
            else ""
        )
        print(f"\nEvery state above was observed, not asserted from a document.{tail}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
