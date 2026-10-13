# Threat model

ThruLine's job is to be right about an agent that is trying, or accidentally
managing, to look successful. The interesting attacks are therefore against the
*evidence*, not against the process.

## The adversary

An agent that has read `.claude/settings.json` and knows ThruLine is installed.
It can run any command the user allows, edit any file in the repository, and
write whatever it likes in its final message. It is not assumed to be malicious;
the same behaviours arise from a model optimising for a green result.

## What is assumed

- The machine running the agent is not compromised. ThruLine records what
  happened on it; it cannot detect a kernel that lies.
- `git` is present and its output is trustworthy.
- The user is not attacking themselves.

## What is defended

**A claim with no check.** No check in the record means no Proven. The absence
of evidence is Couldn't-check, and Couldn't-check never becomes Proven at any
layer — not in a verifier, not in a receipt, not in a policy decision, not in a
check run. Every stage that could round it away has a test that it does not.

**A narrowed check.** The command line is captured, so "all tests pass" backed
by a single-test run is visible as `narrowed_run`. When the command itself is
not in the record, the finding is `narrowing_unchecked` — an explicit statement
that the question could not be asked, never silence.

**A weakened test.** The diff is examined for assertions removed, loosened,
swallowed, or weakened to something that always holds. An assertion loosened all
the way to truthy is treated as a removal, because it is one.

**A test skipped.** A skip marker added in-session is reported, and its severity
follows the run: `fail` when the run left tests unrun, `info` when it did not.
The marker alone is a candidate, not a conclusion.

**A check that ran before the last edit.** What passed is not what is being
claimed for, and `ran_before_final_edit` says so. A check bound to a tree that no
longer exists is not evidence for the tree in front of you.

**A write nothing reported.** A tool that changes a file without reporting a path
— `sed -i`, `patch`, a shell redirect — is caught by comparing trees rather than
by trusting the edit events.

**Code that knows it is being tested.** Environment sniffing, branches keyed on
literals that only the tests use, implementations that return the constant the
test expects. Each is a warning alone and a failure alongside a fail-severity
test finding.

**A flaky pass.** Two runs on the same tree with different outcomes make the
claim Not proven, and a flaky test never counts as passing evidence in either
direction. The comparison is over the exit code **and** the tally: two runs that
both exit 1 and report different counts are not the same outcome.

**Re-execution used as laundering.** Running `thruline reexec` on a session
whose tests ran before the final edit does not improve the verdict. A
re-execution can confirm or contradict; it can never promote. Two independent
guards enforce it, and reverting either alone leaves the other.

**A `reexecuted: true` that nothing backs.** The flag is never asserted: it is
derived from the container image digest `thruline reexec` records on the `Check`
row. A receipt asserting the boolean without the digest is refused, and so is a
digest with the boolean false. The digest is not itself chained — a re-execution
appends no event and leaves the session's chain head unchanged — so it carries
the standing of the derived `checks` table, which is the standing every other
check in the predicate already has.

**Prose that excuses missing evidence.** Honest-failure language ("I couldn't
get the tests running") is *narrative only*. It never alters a verdict, a gate
outcome, or a receipt. An agent that claims success and appends an excuse does
not get its missing evidence excused by the sentence it chose to write. The same
reasoning keeps "the agent was honest" out of the receipt entirely: a receipt is
a gating artifact, and any field a policy engine can read is a field an agent can
learn to write.

**A model in the verdict path.** There is not one. Model-added claims are
advisory and cannot fail a gate; no policy key can reach the extractor or a
score, and the schema has no such key. The boundary is structural, not a
convention.

**A tampered receipt.** Signed DSSE, verified offline over the payload bytes as
received — the statement is serialised deterministically, in struct-declaration
order, not canonicalised. The subject digest binds it to a tree, recomputed
against the repository in front of the verifier rather than read out of the
statement — so a receipt cannot be replayed onto another commit.

**A truncated session.** A prefix of a hash chain is itself a valid hash chain,
so the receipt carries the event count as well as the chain head. A matching head
with a different count is a session with its tail removed, and it fails.

**A supply-chain swap.** Every install path — the Action, the installer script,
the Homebrew formula, the `npx` shim — verifies a checksum *before* the binary is
put anywhere, and every automated path is pinned to an exact version. A version
that changes without a diff is a version nobody reviewed. The `curl | sh`
installer is the one exception, and a narrow one: with no `THRULINE_VERSION` set
it resolves the newest release, prints which version it resolved and how to pin
it, and still refuses a binary whose checksum is missing or does not match. None
of them has ever run, because no release exists; the logic is verified by
reading.

## What is not defended

- **An agent that edits the ledger.** The event table is append-only, enforced
  by database triggers, and hash-chained — so tampering is detectable, not
  impossible. A receipt over a broken chain fails verification.
- **A check that is itself wrong.** ThruLine confirms that a check ran and what
  it reported. Whether the check tests the right thing is a human question.
- **Imported harness logs.** A rollout is the harness's own account, not
  evidence. Imported sessions are marked `capture_degraded` and no claim in one
  can reach Proven.
- **Capture that was never installed.** A repository with no hooks and no shim
  produces no checks, and every claim comes back Couldn't-check. That is the
  correct answer and it is not a verdict about the code. `thruline doctor`
  reports whether the install is intact.
- **A hidden write outside the tree.** A change to a gitignored path, or outside
  the repository, is invisible to a tree hash by construction. Where a
  re-execution is superseded by one, `reexecution_superseded` says so; where it
  is not detected, nothing does.
- **Languages without a tampering detector.** The AST detectors cover Python,
  JavaScript/TypeScript, Rust and Go. A test file in another language yields no
  tampering findings, and their absence is not evidence.

## Where a reader should be sceptical

Two things on this page are argued rather than measured.

**Detection rates are not published.** There is no number for how often the
tampering detectors catch real cheating, because no labelled corpus of real
cheating has been measured here. [The benchmark page](./benchmark.md) says what
was tried and why it produced nothing.

**None of this has run in CI.** The tests behind these defences pass on one
macOS host. No continuous integration has ever executed on this repository, so
"a test asserts it" means a test that passes where it was written.

*What would prove both:* a labelled corpus with a published detection rate, and
a green run on a case-sensitive Linux runner that somebody other than the author
can point at.
