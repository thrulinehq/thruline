import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import {
  STAGES,
  acceptedScope,
  approveBlocker,
  buildChangeView,
  conclusionOf,
  dayOf,
  scopeOfWork,
  shortRevision,
  stagesOf,
  targetVersion,
} from './change-view.js'

/* T-2-4 — the deterministic model behind the Change screen.
 *
 * Every test here is one acceptance criterion of `docs/loop/tasks/T-2-4.md`
 * §Acceptance 2, and every one of them is an assertion about what SAM reads,
 * not about how the code is arranged. The fixture Change is synthetic
 * throughout: the ids are the `cccccccc-…` family T-2-1's own tests use and no
 * string below came off a real machine.
 */

const SESSION_A = 'cccccccc-0000-4000-8000-00000000s001'
const SESSION_B = 'cccccccc-0000-4000-8000-00000000s002'

/** One Change record in the shape `/api/changes/<id>` serves (T-2-1 snapshot). */
function aChange(over = {}) {
  return {
    schema_version: 2,
    id: 'cccccccc-0000-4000-8000-00000000c001',
    project_id: 'cccccccc-0000-4000-8000-00000000p001',
    opened_at: '2026-01-02T09:00:00Z',
    request: {
      title: 'Retry failed uploads',
      purpose: 'Uploads that fail once are lost, and the person never finds out.',
      prompt_event_id: null,
      requester: 'sam',
      source: 'person',
    },
    accepted_scope: null,
    scope_history: [],
    implementation: null,
    implementation_history: [],
    associations: [],
    examination: 'none_yet',
    decision: 'none_yet',
    masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
    ...over,
  }
}

const scope = (criteria, checkProfile = []) => ({
  version: 1,
  proposed_at: '2026-01-02T09:05:00Z',
  criteria,
  exclusions: [],
  allowed_scope: [],
  constraints: [],
  check_profile: checkProfile,
  content_sha256: '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
  acceptance: { actor: 'sam', accepted_at: '2026-01-02T09:06:00Z', rationale: 'That is what I asked for.' },
})

const bound = {
  version: 1,
  bound_at: '2026-01-02T10:00:00Z',
  repository_identity: 'repo-synthetic-0001',
  /* A revision that names something. It used to be 39 zeros and a 1, whose
     short form is `0000000` — the string a reader of the real screen paused
     over (T24-3), and a fixture that taught the test suite to expect it. */
  base_revision: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0',
  snapshot_id: null,
  workspace_identity: 'workspace-synthetic-0001',
}

const link = (session, provenance, at) => ({
  id: 'cccccccc-0000-4000-8000-00000000a' + at,
  session_id: session,
  provenance,
  basis: provenance === 'suggested' ? 'timing' : 'user',
  actor: provenance === 'suggested' ? null : 'sam',
  asserted_at: '2026-01-02T10:0' + at + ':00Z',
  reason: null,
})

/* ── the conclusion rule, one test per branch ──────────────────────────── */

test('conclusion: nothing accepted — the sentence asks for what should count', () => {
  const c = conclusionOf({ criteria: [], evidence: [] })
  assert.equal(c.key, 'no_criteria')
  assert.equal(fill(HQ_COPY.change.conclusion[c.key], c.values), 'Nothing counts as done yet: accept what should.')
})

test('conclusion: accepted and nothing has evidence — missing evidence is said to be missing', () => {
  const c = conclusionOf({ criteria: ['a', 'b', 'c'], evidence: [] })
  assert.equal(c.key, 'no_evidence')
  assert.equal(fill(HQ_COPY.change.conclusion[c.key], c.values), 'Accepted: 3 things count as done. None has evidence yet.')
  const one = conclusionOf({ criteria: ['a'], evidence: [] })
  assert.equal(one.key, 'no_evidence_one')
  assert.equal(fill(HQ_COPY.change.conclusion[one.key], one.values), 'Accepted: 1 thing counts as done. It has no evidence yet.')
})

test('conclusion: some have evidence — the ones that do not are counted, not rounded away', () => {
  const c = conclusionOf({
    criteria: ['a', 'b', 'c'],
    evidence: [{ criterion: 1, state: 'proven' }, { criterion: 2, state: 'proven' }],
  })
  assert.equal(c.key, 'some_evidence_one')
  assert.equal(fill(HQ_COPY.change.conclusion[c.key], c.values), '2 of 3 things that count as done have evidence. 1 still needs it.')
  const two = conclusionOf({ criteria: ['a', 'b', 'c', 'd'], evidence: [{ criterion: 1, state: 'proven' }, { criterion: 2, state: 'proven' }] })
  assert.equal(two.key, 'some_evidence')
  assert.equal(fill(HQ_COPY.change.conclusion[two.key], two.values), '2 of 4 things that count as done have evidence. 2 still need it.')
})

test('conclusion: a problem in the record outranks the count, and says so plainly', () => {
  const c = conclusionOf({
    criteria: ['a', 'b', 'c'],
    evidence: [{ criterion: 1, state: 'proven' }, { criterion: 2, state: 'not_proven' }],
  })
  assert.equal(c.key, 'problem_one')
  assert.equal(fill(HQ_COPY.change.conclusion[c.key], c.values), 'The record shows a problem with 1 of 3 things.')
  const two = conclusionOf({ criteria: ['a', 'b', 'c'], evidence: [{ criterion: 1, state: 'not_proven' }, { criterion: 2, state: 'not_proven' }] })
  assert.equal(two.key, 'problem')
  assert.equal(fill(HQ_COPY.change.conclusion[two.key], two.values), 'The record shows a problem with 2 of 3 things.')
})

test('conclusion: "recorded" is not a pass and not a failure — the criterion still needs evidence', () => {
  /* `B7.2`, `B7.15`: an examination that could not settle anything must not
   * satisfy a mandatory check and must not be reported as a defect. It counts
   * as covered by an observation, and it raises no problem. */
  const c = conclusionOf({ criteria: ['a', 'b'], evidence: [{ criterion: 1, state: 'couldnt' }] })
  assert.equal(c.key, 'some_evidence_one')
  assert.equal(c.problems, 0)
})

test('conclusion: the sentence is never a state word and never a percentage (negative)', () => {
  const five = Object.values(HQ_COPY.states).map((s) => s.word)
  const cases = [
    { criteria: [], evidence: [] },
    { criteria: ['a'], evidence: [] },
    { criteria: ['a', 'b', 'c'], evidence: [] },
    { criteria: ['a', 'b', 'c'], evidence: [{ criterion: 1, state: 'proven' }] },
    { criteria: ['a', 'b', 'c'], evidence: [{ criterion: 1, state: 'not_proven' }] },
    { criteria: ['a', 'b'], evidence: [{ criterion: 1, state: 'not_proven' }, { criterion: 2, state: 'not_proven' }] },
  ]
  for (const record of cases) {
    const c = conclusionOf(record)
    const text = fill(HQ_COPY.change.conclusion[c.key], c.values)
    for (const word of five) assert.ok(!text.includes(word), `the top sentence renders "${word}": ${text}`)
    assert.doesNotMatch(text, /\d+\s*%/, `the top sentence carries a percentage: ${text}`)
    assert.doesNotMatch(text, /\bgreen\b|\ball clear\b|\bready to ship\b/i, `the top sentence declares an all-clear: ${text}`)
  }
})

test('conclusion: evidence pointing at a criterion that does not exist changes nothing', () => {
  const c = conclusionOf({ criteria: ['a'], evidence: [{ criterion: 9, state: 'proven' }, { criterion: 0, state: 'proven' }] })
  assert.equal(c.key, 'no_evidence_one')
})

/* ── the one named version ─────────────────────────────────────────────── */

test('the target version is a short revision and a date, and it is a sentence, not a chip', () => {
  const v = targetVersion(aChange({ implementation: bound }))
  assert.deepEqual(v, { bound: true, named: true, key: 'bound', values: { revision: 'a1b2c3d', date: '2026-01-02' }, revision: 'a1b2c3d', date: '2026-01-02' })
  assert.equal(fill(HQ_COPY.change.version[v.key], v.values), 'version a1b2c3d · 2026-01-02')
})

test('the snapshot wins over the base revision: the version named is the copy being looked at', () => {
  const v = targetVersion(aChange({ implementation: { ...bound, snapshot_id: 'abcdef1234567890' } }))
  assert.equal(v.revision, 'abcdef1')
})

test('no version bound: the line says so rather than showing an empty slot', () => {
  const v = targetVersion(aChange())
  assert.equal(v.bound, false)
  /* And it says what the absence MEANS, not only that something is absent:
     without it a reader is told a fact with no consequence attached. */
  assert.match(HQ_COPY.change.version[v.key], /^No version of the work is bound to this Change yet,/)
  assert.match(HQ_COPY.change.version[v.key], /about the work in general, not about one version/)
})

test('a timestamp that is not a recorded UTC one yields no date, never an invented one', () => {
  assert.equal(dayOf('2026-01-02T10:00:00Z'), '2026-01-02')
  assert.equal(dayOf('yesterday'), null)
  assert.equal(dayOf(null), null)
  assert.equal(shortRevision('abc'), 'abc')
  assert.equal(shortRevision(null), null)
})

/* ── scoping: a suggestion contributes nothing ─────────────────────────── */

test('scoping: only confirmed work counts; a suggested session contributes no card and no node', () => {
  const change = aChange({
    associations: [link(SESSION_A, 'confirmed', 1), link(SESSION_B, 'suggested', 2)],
  })
  const s = scopeOfWork(change)
  assert.deepEqual(s.confirmed, [SESSION_A])
  assert.deepEqual(s.suggested, [SESSION_B])

  const model = buildChangeView({
    change,
    work: [
      { id: SESSION_A, findings: [{ id: 'f1' }], focus: { summary: { total: 4, shown: 2, merged: 2 }, labels: [{ id: 'n1', text: 'one' }, { id: 'n2', text: 'two' }] } },
      { id: SESSION_B, findings: [{ id: 'f2' }], focus: { summary: { total: 9, shown: 9, merged: 0 }, labels: [{ id: 'n9', text: 'nine' }] } },
    ],
  })
  assert.equal(model.work.length, 1, 'the suggested session was not filtered out')
  assert.deepEqual(model.findings.map((f) => f.id), ['f1'])
  assert.equal(model.work[0].focus.summary.total, 4, 'a node from the suggested session reached the focus set')
})

test('scoping: the standing link wins — a suggestion later confirmed counts, a confirmation later rejected does not', () => {
  const s = scopeOfWork(aChange({
    associations: [
      link(SESSION_A, 'suggested', 1), link(SESSION_A, 'confirmed', 2),
      link(SESSION_B, 'confirmed', 3), link(SESSION_B, 'rejected', 4),
    ],
  }))
  assert.deepEqual(s.confirmed, [SESSION_A])
  assert.deepEqual(s.rejected, [SESSION_B])
  assert.deepEqual(s.suggested, [])
})

/* ── Approve: disabled, with a reason and an enabler, in every state ───── */

test('Approve is off for a reason that is a fact about the record, and each reason names what would enable it', () => {
  const states = [
    [{ criteria: [], versionBound: false, withoutEvidence: 0 }, 'no_criteria'],
    [{ criteria: ['a'], versionBound: false, withoutEvidence: 0 }, 'no_version'],
    [{ criteria: ['a'], versionBound: true, withoutEvidence: 2 }, 'no_evidence'],
    [{ criteria: ['a'], versionBound: true, withoutEvidence: 1 }, 'no_evidence_one'],
    [{ criteria: ['a'], versionBound: true, withoutEvidence: 0 }, 'decide_below'],
  ]
  for (const [record, key] of states) {
    const b = approveBlocker(record)
    assert.equal(b.key, key)
    assert.ok(b.reason && b.reason.trim().length > 10, `${key} has no reason`)
    assert.ok(b.enabler && b.enabler.trim().length > 10, `${key} has no enabler`)
    assert.doesNotMatch(fill(b.reason, b.values), /[{}]/, `${key} leaves a brace on screen`)
  }
  assert.equal(fill(approveBlocker(states[2][0]).reason, { n: 2 }), '2 things that count as done have no evidence yet.')
  assert.equal(approveBlocker(states[3][0]).reason, '1 thing that counts as done has no evidence yet.')
})

/* T-3-6-F1 (`Q1`) — the defect itself, as a test: two passing runs on record,
 * and the disabled Approve control said a required check had not run.
 *
 * The blocker's number and the top sentence's number are now the SAME
 * reading, so the only way to make them disagree again is to give the blocker
 * a second source, which is what this asserts is impossible. */
test('the Approve blocker counts what the top sentence counts, and never a second number', () => {
  const criteria = ['the duplicate-retry test passes', 'the restart test passes']
  const evidence = [
    { criterion: 1, state: 'proven', bears: true },
    { criterion: 2, state: 'proven', bears: true },
  ]
  const c = conclusionOf({ criteria, evidence })
  assert.equal(c.withEvidence, 2)
  const b = approveBlocker({ criteria, versionBound: true, withoutEvidence: c.withoutEvidence + c.problems })
  assert.equal(b.key, 'decide_below', 'both lines have evidence: the reason must not be a count of anything')
  assert.doesNotMatch(b.reason, /required check/)
  assert.doesNotMatch(b.reason, /\d/, 'a reason with no missing thing in it has no number in it')

  const half = conclusionOf({ criteria, evidence: [evidence[0]] })
  const hb = approveBlocker({ criteria, versionBound: true, withoutEvidence: half.withoutEvidence + half.problems })
  assert.equal(hb.key, 'no_evidence_one')
  assert.equal(fill(hb.reason, hb.values), '1 thing that counts as done has no evidence yet.')
})

/* ── the six stages ────────────────────────────────────────────────────── */

test('the six stages are the brief\'s six, in the brief\'s order', () => {
  assert.deepEqual([...STAGES], ['define', 'prepare', 'build', 'verify', 'review', 'approve'])
  const model = buildChangeView({ change: aChange() })
  assert.deepEqual(model.stages.map((s) => s.id), [...STAGES])
  assert.deepEqual(model.stages.map((s) => s.name), ['Define', 'Prepare', 'Build', 'Verify', 'Review', 'Approve'])
})

test('every stage that holds nothing carries a sentence saying what would put something there', () => {
  const stages = stagesOf(buildChangeView({ change: aChange({ request: { title: '', purpose: '', requester: null } }) }))
  for (const s of stages) {
    assert.equal(s.filled, false, `${s.id} claims to hold something on an empty Change`)
    assert.ok(s.empty.length > 30, `${s.id} has no sentence to render when empty`)
  }
  /* T-6-6: it used to send a person to the Verify stage to ask for a review,
   * and nothing there asks for one. Asking for a review is still not built
   * from this screen (blocked.review_not_built); READING one is, since T-4-1:
   * a recorded review fills this stage (the next test). */
  assert.equal(stages[4].empty, 'No review has been written down for this Change yet, so there is nothing here to read.')
  assert.equal(stages[2].empty, 'No changes captured yet for this version.')
})

test('T-4-1 fix 2: a Change with a recorded review marks the Review stage as holding something; with none it stays empty', () => {
  const served = { id: 'rev-1', review_round: 1, state: 'complete', finding_count: 1, applies_to_current_version: false }
  const withOne = buildChangeView({ change: aChange({ reviews: [served] }) })
  assert.deepEqual(withOne.review, [served], 'the served reviews are carried, not re-derived')
  assert.equal(withOne.stages[4].id, 'review')
  assert.equal(withOne.stages[4].filled, true, 'a recorded review must not read as an empty stage')
  // A review of an EARLIER version still fills the stage: the stage holds a
  // review; which version it applies to is the review section's sentence.
  for (const reviews of [undefined, null, [], 'not-a-list']) {
    const none = buildChangeView({ change: aChange({ reviews }) })
    assert.deepEqual(none.review, [])
    assert.equal(none.stages[4].filled, false, `reviews=${JSON.stringify(reviews)} must leave the stage empty`)
  }
})

/* ── the whole model ───────────────────────────────────────────────────── */

test('the model carries what counts as done, the version and the linked work as four separate readings', () => {
  const change = aChange({
    accepted_scope: scope(['A failed upload is tried again three times.'], ['cargo-test']),
    implementation: bound,
    associations: [link(SESSION_A, 'confirmed', 1)],
  })
  const model = buildChangeView({ change, work: [{ id: SESSION_A, findings: [], focus: null }] })
  assert.equal(model.accepted.criteria.length, 1)
  assert.equal(model.accepted.digest, '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c')
  assert.equal(model.version.bound, true)
  assert.deepEqual(model.scope.confirmed, [SESSION_A])
  assert.equal(model.conclusion.key, 'no_evidence_one')
  /* Four separate readings, never one badge: the model exposes them under four
   * names and composes no combined state anywhere (`B7.4`). */
  assert.equal(model.badge, undefined)
  assert.equal(model.status, undefined)
})

test('acceptedScope of a Change with nothing accepted is empty, never a guess', () => {
  const a = acceptedScope(aChange())
  assert.deepEqual(a, { version: null, criteria: [], allowedScope: [], checkProfile: [], acceptedBy: null, acceptedAt: null, digest: null })
})

/* ── the file's own promises ───────────────────────────────────────────── */

test('the model is pure: no fetch, no window, no clock, no randomness', () => {
  const source = readFileSync(fileURLToPath(new URL('./change-view.js', import.meta.url)), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
  for (const forbidden of ['fetch(', 'window', 'document', 'Date.now', 'new Date', 'Math.random', 'localStorage', 'toLocale']) {
    assert.ok(!source.includes(forbidden), `change-view.js reaches for ${forbidden}`)
  }
})

test('the same record always decides the same screen', () => {
  const change = aChange({
    accepted_scope: scope(['one', 'two']),
    implementation: bound,
    associations: [link(SESSION_B, 'confirmed', 2), link(SESSION_A, 'confirmed', 1)],
  })
  const a = buildChangeView({ change, work: [] })
  const b = buildChangeView({ change, work: [] })
  assert.deepEqual(a, b)
  assert.deepEqual(a.scope.confirmed, [SESSION_A, SESSION_B], 'the order of the served links leaked into the screen')
})

/* ── fix round 1: a revision of nothing but zeros (T24-3) ──────────────── */

test('a revision of nothing but zeros is bound but names nothing, and never renders as a short hash', () => {
  /* Measured on a real screen: `version 0000000 · 2026-01-02`. It reads like a
     placeholder because it is one — git's own name for no revision at all —
     and a person pausing over whether the tool is showing them real data is
     the trust this product cannot spend. */
  const zeros = targetVersion(aChange({ implementation: { ...bound, base_revision: '0'.repeat(40) } }))
  assert.equal(zeros.key, 'placeholder')
  assert.equal(zeros.bound, true, 'a version IS written down; saying none is bound would be a second untruth')
  assert.equal(zeros.named, false)
  assert.equal(zeros.revision, null, 'seven zeros were still handed to the view')
  assert.ok(!fill(HQ_COPY.change.version[zeros.key], zeros.values).includes('0000000'))

  /* And the ordinary case is untouched, digit for digit. */
  const real = targetVersion(aChange({ implementation: { ...bound, base_revision: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0' } }))
  assert.equal(real.key, 'bound')
  assert.equal(real.named, true)
  assert.equal(real.revision, 'a1b2c3d')
  assert.equal(fill(HQ_COPY.change.version.bound, real.values), 'version a1b2c3d · 2026-01-02')

  /* A snapshot of zeros is the same fact reached through the other field. */
  assert.equal(targetVersion(aChange({ implementation: { ...bound, snapshot_id: '0000' } })).key, 'placeholder')
  /* A revision whose SHORT form is zeros names nothing a person could look
     up, whatever the rest of it holds: `version 0000000` was the string on
     the screen. */
  assert.equal(targetVersion(aChange({ implementation: { ...bound, base_revision: '0'.repeat(39) + '1' } })).key, 'placeholder')
  /* And a revision that merely CONTAINS zeros is a revision. */
  assert.equal(targetVersion(aChange({ implementation: { ...bound, base_revision: '0a00000000000000000000000000000000000000' } })).key, 'bound')
})

test('Approve stays disabled on a placeholder version for the reason it is actually disabled', () => {
  const model = buildChangeView({
    change: aChange({ accepted_scope: scope(['one']), implementation: { ...bound, base_revision: '0'.repeat(40) } }),
    work: [],
  })
  /* It is bound, so `no_version` would be a lie. One line is accepted and
     nothing has evidence for it, and that — not a count of checks the record
     was never asked about — is why Approve is off (T-3-6-F1). Both halves are
     present either way. */
  assert.equal(model.blocker.key, 'no_evidence_one')
  assert.ok(model.blocker.reason.length > 10 && model.blocker.enabler.length > 10)
})

/* T-3-6-F1 / R-2026-09-23 — the Approve reason follows the same case as the
 * lines. A Change one link away from evidence is not a Change with none. */
test('Approve says which act is missing: linking the run that ran, or running one', () => {
  const criteria = ['a', 'b']
  const unlinked = approveBlocker({ criteria, versionBound: true, withoutEvidence: 1, unlinkedRun: { id: 'b002', name: 'cargo test --test=restart' } })
  assert.equal(unlinked.key, 'unlinked_one')
  assert.equal(fill(unlinked.reason, unlinked.values), "1 line has a run that isn't linked to it.")
  assert.match(unlinked.enabler, /Link that run/)

  const none = approveBlocker({ criteria, versionBound: true, withoutEvidence: 1, unlinkedRun: null })
  assert.equal(none.key, 'no_evidence_one')
  assert.doesNotMatch(none.reason, /isn't linked/)

  const many = approveBlocker({ criteria, versionBound: true, withoutEvidence: 2, unlinkedRun: { id: 'b002', name: 'x' } })
  assert.equal(fill(many.reason, many.values), "2 lines have a run that isn't linked to them.")
})
