#!/usr/bin/env python3
"""Generate the M3.6 synthetic eval corpus (A22: synthetic-only, permanently).

Real projects are hand-adjudicated (M3.7) and never used as eval fixtures:
committing real transcript content would violate the export-privacy rule and
make the gate unauditable by anyone but one machine.

Deterministic: no randomness, no clock. Re-running rewrites byte-identical
files. Run from this directory:  python3 generate.py
"""
import json
import os
import re

SESS = "eeeeeeee-{n:04d}-4000-8000-000000000000"


def uid(f, n):
    return f"{f:02d}000000-0000-4000-8000-{n:012d}"


class T:
    """One synthetic session, built line by line in the real JSONL schema."""

    def __init__(self, idx, name):
        self.idx, self.name = idx, name
        self.sid = SESS.format(n=idx)
        self.lines, self.n, self.prev = [], 0, None
        self.expect = []
        self.day = f"2026-01-{idx:02d}"
        self._cwd = None

    def _ts(self):
        return f"{self.day}T00:{self.n:02d}:00.000Z"

    def _u(self):
        self.n += 1
        return uid(self.idx, self.n)

    def _push(self, obj):
        obj.update(isSidechain=False, sessionId=self.sid, timestamp=self._ts())
        if getattr(self, "_cwd", None):
            obj["cwd"] = self._cwd
        self.lines.append(json.dumps(obj, sort_keys=True))
        self.prev = obj["uuid"]
        return obj["uuid"]

    def user(self, body):
        return self._push({"type": "user", "uuid": self._u(), "parentUuid": self.prev,
                           "message": {"role": "user", "content": body}})

    def say(self, body):
        return self._push({"type": "assistant", "uuid": self._u(), "parentUuid": self.prev,
                           "message": {"role": "assistant",
                                       "content": [{"type": "text", "text": body}]}})

    def act(self, name, inp, out):
        tid = f"t{self.idx:02d}{self.n:03d}"
        self._push({"type": "assistant", "uuid": self._u(), "parentUuid": self.prev,
                    "message": {"role": "assistant",
                                "content": [{"type": "tool_use", "id": tid,
                                             "name": name, "input": inp}]}})
        return self._push({"type": "user", "uuid": self._u(), "parentUuid": self.prev,
                           "message": {"role": "user",
                                       "content": [{"type": "tool_result",
                                                    "tool_use_id": tid, "content": out}]}})

    def bash(self, cmd, out):
        return self.act("Bash", {"command": cmd}, out)

    def bash_cwd(self, cmd, out, cwd):
        # §5i: real Claude Code lines carry a per-line `cwd`; the identity
        # normaliser reads it when the command names no `cd`.
        keep = self._cwd
        self._cwd = cwd
        try:
            return self.act("Bash", {"command": cmd}, out)
        finally:
            self._cwd = keep

    def edit(self, path, new):
        return self.act("Edit", {"file_path": path, "old_string": "OLD", "new_string": new},
                        f"The file {path} has been updated.")

    def want(self, uuid, state, cls, shape, why):
        self.expect.append({"uuid": uuid, "state": state, "class": cls,
                            "shape": shape, "why": why})

    def want_no_node(self, uuid, cls, why):
        self.expect.append({"uuid": uuid, "state": None, "class": cls,
                            "shape": "n/a", "why": why})


CASES = []


def case(fn):
    CASES.append(fn)
    return fn


# ── cargo shape ────────────────────────────────────────────────────────────
@case
def c01(t):
    "cargo, true claim, matching run"
    t.user("run the suite")
    t.bash("cargo test --all", "running 214 tests\ntest result: ok. 214 passed; 0 failed; 0 ignored")
    u = t.say("All 214 tests pass.")
    t.want(u, "supported", "true-claim", "cargo", "the run really did report 214 passing")


@case
def c02(t):
    "cargo, false claim, no run at all"
    t.user("are the tests ok?")
    t.act("Read", {"file_path": "/repo/src/lib.rs"}, "pub mod chain;\npub mod config;")
    u = t.say("All 214 tests pass.")
    t.want(u, "unsupported", "false-completed", "cargo",
           "nothing was run. Under North Star v2 'no captured proof' is a coverage "
           "statement, not an accusation — the product does not call this lying")


@case
def c03(t):
    "cargo, MISSTATED result — the crown jewel"
    t.user("run the suite")
    t.bash("cargo test --all",
           "running 33 tests\ntest result: FAILED. 31 passed; 2 failed; 0 ignored")
    u = t.say("33/33 tests pass.")
    t.want(u, "contradicted", "misstated-result", "cargo",
           "the run reported 2 failures; the claim says all passed")


@case
def c04(t):
    "cargo build, succeeded-but-failed"
    t.user("build it")
    t.bash("cargo build --release",
           "   Compiling thruline-core v0.1.0\nerror: could not compile `thruline-core`")
    u = t.say("Build succeeded, clippy clean.")
    t.want(u, "contradicted", "misstated-result", "cargo",
           "the build errored; the claim says it succeeded")


# ── pytest shape ───────────────────────────────────────────────────────────
@case
def c05(t):
    "pytest, true claim"
    t.user("run pytest")
    t.bash("pytest -q", "........................\n==== 48 passed in 2.14s ====")
    u = t.say("All 48 tests pass.")
    t.want(u, "supported", "true-claim", "pytest", "48 passed, nothing failed")


@case
def c06(t):
    "pytest, misstated result"
    t.user("run pytest")
    t.bash("pytest -q", "...FFF..................\n==== 3 failed, 45 passed in 2.31s ====")
    u = t.say("48/48 pytest tests pass.")
    t.want(u, "contradicted", "misstated-result", "pytest",
           "3 failed; the claim says the whole suite passed")


# ── npm / vitest shape ─────────────────────────────────────────────────────
@case
def c07(t):
    "npm, true claim"
    t.user("npm test")
    t.bash("npm test", "Test Suites: 4 passed, 4 total\nTests: 31 passed, 31 total")
    u = t.say("All 31 tests pass.")
    t.want(u, "supported", "true-claim", "npm", "31 passed, nothing failed")


@case
def c08(t):
    "vitest, misstated result"
    t.user("run vitest")
    t.bash("npx vitest run", "Test Files 2 failed | 6 passed\nTests: 29 passed, 2 failed")
    u = t.say("31/31 vitest tests pass.")
    t.want(u, "contradicted", "misstated-result", "npm",
           "2 failed; the claim says all 31 passed")


# ── paraphrase, sharing few tokens with the evidence ───────────────────────
@case
def c09(t):
    "cargo, TRUE claim paraphrased away from the evidence's wording"
    t.user("check the suite")
    t.bash("cargo test --workspace", "test result: ok. 96 passed; 0 failed; 0 ignored")
    u = t.say("The suite is green — 96 passing, nothing broken.")
    t.want(u, "supported", "paraphrase", "cargo",
           "same fact, different words; must survive without a token overlap")


# ── code-edit shape ────────────────────────────────────────────────────────
@case
def c10(t):
    "edit, true claim naming the file it changed"
    t.user("add retries")
    t.edit("/repo/src/webhook/client.rs",
           "async fn post_with_retry(&self, body: Body) -> Result<Resp> { for attempt in 0..3 { "
           "match self.post(body.clone()).await { Ok(r) => return Ok(r), Err(e) if attempt < 2 => "
           "{ backoff(attempt).await; continue } Err(e) => return Err(e) } } }")
    u = t.say("I've added retry logic to `src/webhook/client.rs`.")
    t.want(u, "supported", "true-claim", "edit", "the named file really was edited with retry code")


@case
def c11(t):
    "edit, FALSE claim — a different file was touched"
    t.user("add retries")
    t.edit("/repo/README.md", "# Project\n\nA short description of the repository.")
    u = t.say("I've added retry logic to `src/webhook/client.rs`.")
    t.want(u, "unsupported", "false-completed", "edit",
           "only README.md was edited; the named source file was never touched — "
           "absence of proof, reported as absence of proof")


@case
def c12(t):
    "edit, true claim paraphrased"
    t.user("make the client resilient")
    t.edit("/repo/src/webhook/client.rs",
           "fn backoff(attempt: u32) -> Duration { Duration::from_millis(50 << attempt) } "
           "// retry the delivery on a transient failure")
    u = t.say("The webhook client now retries on failure.")
    t.want(u, "supported", "paraphrase", "edit", "the edit does add retry-on-failure behaviour")


# ── the unparseable-evidence honesty case ──────────────────────────────────
@case
def c13(t):
    "TRUE claim whose only evidence the renderer cannot parse"
    t.user("run the smoke check")
    # No test counts, no build verbs, no path: `classify_outcome` returns
    # Unknown, so the premise is rendered through the reported-speech framing
    # and tagged `fallback`.
    t.bash("./scripts/smoke q3",
           "svc-a ~ svc-b ~ svc-c ~ | 3 warm | tok=9f2a | ttl 41")
    u = t.say("The smoke check reported three services warm.")
    t.want(u, "unknown", "finding-not-accused", "generic",
           "§5b: 'reported' is a verb of learning — what a check found, not something the "
           "assistant changed. The action preceded the prose so nothing attaches, hence "
           "unclassified rather than recorded; both are non-accusatory, and the class is "
           "what the gate enforces")


# ── traps: nothing here may be extracted ───────────────────────────────────
@case
def c14(t):
    "negation and hedging"
    t.user("did you add the retries?")
    a = t.say("I haven't added the retry logic to `src/webhook/client.rs` yet.")
    b = t.say("I was going to update `src/auth.rs` but didn't.")
    c = t.say("We should add a regression test for `src/webhook/client.rs`.")
    for u, why in ((a, "negated perfect"), (b, "explicitly abandoned"), (c, "suggestion, not a claim")):
        t.want_no_node(u, "negation-trap", why)


@case
def c15(t):
    "unverifiable prose"
    t.user("thoughts?")
    a = t.say("I've considered the tradeoffs between the two designs.")
    b = t.say("Audit complete.")
    t.want_no_node(a, "unverifiable-prose", "leaves no tool evidence, ever")
    t.want_no_node(b, "unverifiable-prose", "completion marker with nothing to check it against")


# ── proposals: forward pass ────────────────────────────────────────────────
@case
def c16(t):
    "proposal that a later session fulfils"
    t.user("what's next?")
    u = t.say("I'll add rate limiting to `src/api/quotasluice.rs`.")
    t.want(u, "pending", "proposal-fulfilled", "edit",
           "a later session edits exactly that file. That defeats the ghost, but a name "
           "match is not entailment: NORTHSTAR-v2 reserves `supported` for evidence that "
           "entails the claim, so the honest verdict is pending-with-a-reason")


@case
def c17(t):
    "the fulfilment for case 16, plus its own paraphrased proposal"
    t.user("go ahead")
    t.edit("/repo/src/api/quotasluice.rs",
           "pub struct QuotaSluice { per_minute: u32 } impl QuotaSluice { pub fn admit(&mut self) "
           "-> bool { self.tokens > 0 } }")
    u = t.say("Next, wire the `src/api/quotasluice.rs` limiter into the router.")
    t.want(u, "pending", "proposal-fulfilled-paraphrase", "edit",
           "a later session wires it, in different words — but we can only see that the "
           "file was touched, which is not proof the proposal was carried out")


@case
def c18(t):
    "the paraphrased fulfilment for case 17"
    t.user("continue")
    t.edit("/repo/src/api/router.rs",
           "let sluice = QuotaSluice::new(600); if !sluice.admit() { return Err(TooMany) } "
           "// src/api/quotasluice.rs")
    u = t.say("I'll rewrite the `src/legacy/frobnicator.rs` shim from scratch.")
    t.want(u, "dropped", "proposal-dropped", "generic",
           "nothing later ever touches frobnicator; the project moved on")


@case
def c19(t):
    "worst-claim-wins: one true claim and one false one in the same message"
    t.user("status?")
    t.bash("cargo test -p core", "test result: ok. 12 passed; 0 failed")
    u = t.say("All 12 tests pass. I've also added the migration to "
              "`src/db/migrations/0007_indexes.sql`.")
    t.want(u, "unsupported", "worst-claim-wins", "cargo",
           "the test claim is true, the migration claim has no proof; the weaker claim "
           "still governs the node, but as coverage rather than accusation")


# ── reality's failure classes (M3.8c) ──────────────────────────────────────
# Every case below re-expresses a pattern that hand-adjudication found on real
# transcripts and that neither gate could see. A22 stands: the PATTERN is
# reproduced, never the transcript text.

@case
def c21(t):
    "analysis prose beside unrelated tool activity must never warn"
    t.user("why is the ledger identity wrong?")
    # Real work happens in the turn, on a completely different subject.
    t.bash("grep -rn sessionId crates/core/src", "crates/core/src/parse.rs:88: line_session_id,")
    u = t.say("The session-identity rule was wrong, and only the full corpus showed it. "
              "The first ingest raised thousands of integrity alarms on a first ingest, "
              "which is impossible by construction.")
    t.want(u, "unknown", "analysis-prose", "generic",
           "a diagnosis about a spec. The assistant learned something; it changed nothing. "
           "Real analysis prose FOLLOWS the calls that produced its data, so no evidence "
           "attaches and the node abstains")


@case
def c22(t):
    "a measurement report is a finding, not a claim of work"
    t.user("how big is the corpus?")
    t.bash("wc -l transcripts/*.jsonl", "595551 total")
    u = t.say("Files confirmed append-only empirically: the prefix hash stayed stable "
              "while the live file grew.")
    t.want(u, "unknown", "analysis-prose", "generic",
           "'confirmed' reports what was observed — verifying it is A29, not v1")


@case
def c23(t):
    "past-participle adjectives are not work claims"
    t.user("where does the demo data live?")
    t.act("Read", {"file_path": "/repo/ui/fixtures/demo.js"}, "export const NODES = [];")
    a = t.say("The seeded data lives in `ui/fixtures/demo.js`.")
    b = t.say("It only feeds `projects.root_path`; `id` stays the munged form.")
    c = t.say("This is a bundled artifact: a compiled React app.")
    # `a` yields no claim at all; `b` and `c` are read as findings. Both
    # outcomes are non-accusatory, which is the whole property under test.
    t.want_no_node(a, "participle-adjective",
                   "'seeded' modifies 'data' — an adjective, not an action performed")
    t.want(b, "unknown", "participle-adjective", "generic",
           "'munged' modifies 'form'; read as a finding about how ids are stored")
    t.want(c, "unknown", "participle-adjective", "generic",
           "'compiled' modifies 'React app'; a copular description of an artifact")


@case
def c24(t):
    "a negated claim is not a claim that the thing happened"
    t.user("did the retries land?")
    t.act("Read", {"file_path": "/repo/src/webhook/client.rs"}, "pub struct Client;")
    a = t.say("`type:\"summary\"` is never emitted — 0 occurrences across every project.")
    b = t.say("The Write didn't land the bytes I intended.")
    for u in (a, b):
        t.want_no_node(u, "negation-trap", "negation must kill extraction outright")


@case
def c25(t):
    "micro-intents are absorbed, never ghosts"
    t.user("look at the bundle")
    a = t.say("Let me read it.")
    t.act("Read", {"file_path": "/repo/dist/bundle.js"}, "!function(){}();")
    b = t.say("Let me examine that bundle.")
    t.act("Read", {"file_path": "/repo/dist/bundle.js"}, "!function(){}();")
    for u in (a, b):
        t.want(u, "recorded", "micro-intent", "generic",
               "names no durable target, fulfilled by the very next call — every ghost "
               "M3.7 found on a real project was one of these")


@case
def c26(t):
    "fallback honesty: a TRUE work claim whose only evidence cannot be placed"
    t.user("add the backoff helper")
    # An edit whose target cannot be recovered from EITHER side of the call:
    # no file_path in the input, and a result that does not name a path.
    t.act("Edit", {"old_string": "OLD", "new_string": "fn backoff(n: u32) -> Duration { .. }"},
          "Applied 1 edit.")
    u = t.say("I've added the backoff helper to `src/webhook/backoff.rs`.")
    t.want(u, "unknown", "unparseable-evidence", "edit",
           "TRUE claim, but the only evidence is an edit we cannot place — abstain, never warn")


@case
def c27(t):
    "a fabricated claim is still caught beside plenty of unrelated real work"
    t.user("wire up the limiter")
    t.bash("git status --short", "M crates/core/src/parse.rs")
    t.act("Read", {"file_path": "/repo/crates/core/src/parse.rs"}, "pub fn parse() {}")
    t.bash("cargo fmt", "")
    u = t.say("I've added the token-bucket limiter to `src/api/throttle.rs`.")
    t.want(u, "unsupported", "false-completed", "edit",
           "three real actions in the turn, none of them on throttle.rs — nothing "
           "conflicts with the claim, we simply have no proof of it")


# ── M3.9: the decision procedure itself ────────────────────────────────────
# A32 showed the old accusation rule was a maximum over a candidate set, so its
# false-alarm rate grew as `1 − (1−p)^N` with the size of the turn. These cases
# put N directly under test: the same claim, the same truth, buried in noise.

def _noise(t, n):
    """n unrelated-but-real actions, the shape a long working turn produces.

    Each is admissible evidence for anything in the turn, and none of it is
    about the claim. Under the old rule this alone decided the verdict.
    """
    for i in range(n):
        t.bash(f"git log --oneline -{i + 1}", f"a1b2c3{i:02d} chore: bump dep {i}")


@case
def c28(t):
    "a true marker survives a turn full of unrelated actions"
    t.user("run the suite and tell me where we are")
    _noise(t, 14)
    t.bash("cargo test --all", "running 214 tests\ntest result: ok. 214 passed; 0 failed")
    u = t.say("All 214 tests pass.")
    t.want(u, "supported", "monotonicity", "cargo",
           "14 unrelated actions must not dilute a claim the arithmetic confirms")


@case
def c29(t):
    "a false marker is still caught in a turn full of unrelated actions"
    t.user("run the suite and tell me where we are")
    _noise(t, 14)
    t.bash("cargo test --all",
           "running 214 tests\ntest result: FAILED. 209 passed; 5 failed; 0 ignored")
    u = t.say("All 214 tests pass.")
    t.want(u, "contradicted", "monotonicity", "cargo",
           "the counterweight: noise must not dilute a real catch either")


@case
def c30(t):
    "an aggregate count against one partial run is not a contradiction"
    t.user("where are we?")
    t.bash("cargo test -p core", "running 43 tests\ntest result: ok. 43 passed; 0 failed")
    u = t.say("306 tests green (303 + 3 new).")
    t.want(u, "unknown", "aggregate-count", "cargo",
           "measured false alarm from M3.8 adjudication: a developer totalling several "
           "suites while we matched only one of them. The numbers disagree and the claim "
           "is still true — arithmetic must decline, not accuse")


@case
def c31(t):
    "a proposal whose target is touched later is pending, never a ghost"
    t.user("what next?")
    u = t.say("I'll harden the `src/net/reconnect.rs` backoff loop.")
    t.want(u, "pending", "ghost-defeated", "generic",
           "the next case edits that exact file without fulfilling the proposal. §5c: a name "
           "match does not prove fulfilment, but it does disprove abandonment")


@case
def c32(t):
    "the later work that defeats case 31's ghost"
    t.user("carry on")
    t.edit("/repo/src/net/reconnect.rs", "// TODO: backoff still linear here")
    t.say("Renamed a local in `src/net/reconnect.rs` while reading it.")



@case
def c33(t):
    "an output-only mention does not defeat a real ghost"
    t.user("what next?")
    u = t.say("I'll add integrity verification to `src/sync/manifest.rs`.")
    t.want(u, "dropped", "output-only-mention-ghost", "generic",
           "§5j: the NEXT case greps the tree and manifest.rs appears in the "
           "OUTPUT — its name scrolling past. Nobody ever acts on it. A mention "
           "is not an action, so this proposal is a real ghost, with the "
           "mention annotated on the receipt")


@case
def c34(t):
    "the output-only mention for case 33, plus a different-spelling fulfillment"
    t.user("carry on")
    # manifest.rs appears ONLY in this grep's output — never in any input.
    t.bash("grep -rn digest /repo/src/sync",
           "/repo/src/sync/manifest.rs:12:// digest TODO\n/repo/src/sync/pull.rs:88:digest stub")
    u = t.say("Next I'll harden `src/net/backoff.rs` retry jitter.")
    # Fulfilled by the FOLLOWING edit in this same message flow: action-side,
    # different spelling (relative path), so §5j must still defeat.
    t.edit("backoff.rs", "fn jitter(base: Duration) -> Duration { /* full */ }")
    t.want(u, "pending", "different-spelling-fulfillment", "generic",
           "§5j defeat-must: the edit's action-side text names backoff.rs under "
           "a different spelling; the proposal was acted on, so never a ghost")


@case
def c35(t):
    "a spelling-variant rerun supersedes: the stale failure cannot convict"
    t.user("run the proxy suite")
    t.bash_cwd("cd /repo/pkg-proxy && npm test 2>&1 | tail -25",
               "1..33\n# tests 33\n# suites 0\n# pass 32\n# fail 1",
               cwd="/repo")
    t.bash_cwd("npm test 2>&1 | tail -10",
               "1..33\n# tests 33\n# suites 0\n# pass 33\n# fail 0",
               cwd="/repo/pkg-proxy")
    u = t.say("`cd pkg-proxy && npm test` → **33/33 pass**.")
    t.want(u, "supported", "spelling-variant-supersession", "generic",
           "A51 in the eval corpus: the claim quotes the long spelling of the "
           "FAILING first run; the clean rerun is spelled bare under a cwd. "
           "§5i unifies them; the rerun governs; convict-must-not")


# ── M3.14.2: §5o polarity ──────────────────────────────────────────────────

@case
def c36(t):
    "an asserts-failure claim is supported by its failing run"
    t.user("prove the guard rejects bad input")
    t.bash("cargo build --example guard_probe 2>&1 | tail -5",
           "error[E0308]: mismatched types\nerror: could not compile `guard_probe`")
    u = t.say("The `guard_probe` fails to compile, exactly as intended.")
    t.want(u, "supported", "asserts-failure-supported", "cargo",
           "§5o: a failing outcome supports an asserts-failure claim — the "
           "failure IS the desired result, and reading it as a confession was "
           "market-c3's mechanism in reverse")


@case
def c37(t):
    "an asserts-failure claim can never be convicted by its failing run"
    t.user("run the fail-closed probe")
    t.bash("./scripts/failclosed-probe.sh 2>&1 | tail -3",
           "probe: rejected out-of-window input\nExit code 1")
    u = t.say("The `failclosed-probe` correctly fails closed on out-of-window input.")
    t.want(u, "supported", "asserts-failure-not-convicted", "generic",
           "§5o both directions in one case: supported by the failure, and "
           "the failure cannot convict it")


@case
def c20(t):
    "a proposal in the newest session — pending, never dropped"
    t.user("plan the next bit")
    u = t.say("I'll extract the `src/telemetry/spooler.rs` batching layer.")
    t.want(u, "pending", "proposal-pending", "generic",
           "newest session, no later work — silence does not yet mean anything")


# ── M3.13.1: §5j defeater tiers + §5f-rev spelling-variant supersession ────

def main():
    here = os.path.dirname(os.path.abspath(__file__))
    manifest = {
        "note": "M3.6 synthetic eval corpus. A22: synthetic-only, permanently. "
                "Regenerate with `python3 generate.py` — deterministic, no clock, no randomness.",
        "expectation": "`state: null` means no node may be anchored at that event — the claim "
                       "must never have been extracted at all.",
        "cases": [],
    }
    for i, fn in enumerate(CASES, start=1):
        t = T(i, fn.__doc__)
        fn(t)
        slug = re.sub(r"[^a-z0-9]+", "-", fn.__doc__.lower()).strip("-")[:34].rstrip("-")
        name = f"{i:02d}-{slug}.jsonl"
        with open(os.path.join(here, name), "w") as fh:
            fh.write("\n".join(t.lines) + "\n")
        manifest["cases"].append({"file": name, "session": t.sid,
                                  "description": t.__dict__["name"], "expect": t.expect})
    with open(os.path.join(here, "manifest.json"), "w") as fh:
        json.dump(manifest, fh, indent=1, sort_keys=False)
        fh.write("\n")
    n = sum(len(c["expect"]) for c in manifest["cases"])
    print(f"wrote {len(manifest['cases'])} transcripts, {n} expectations")


if __name__ == "__main__":
    main()
