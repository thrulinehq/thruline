import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* T-3-6-F3 (`Q2`, `B16.8`, `B16.10`) — a Change whose record holds a FAILURE.
 *
 * The case this file is the fence for: the record held a failing `cargo test`,
 * the headline said so, the run was bound to the right line — and the Approve
 * stage was disabled with the sentence a Change with nothing run at all gets
 * ("a required check has not run"), so the one act the panel invited was the
 * one act that would not help. Two more gaps in the same shape: what the check
 * actually said when it broke appeared on no screen, and nothing anywhere said
 * the finding was seeded.
 *
 * Three things are asserted here, all of them through the surfaces a person
 * meets:
 *
 *  1. THE BLOCKER'S PRECEDENCE, stated once and tested three ways —
 *     **failure > unlinked run > no evidence**. A failure outranks both counts
 *     because it is what a person acts on first, and the only one of the three
 *     where running something again is the wrong move.
 *  2. THE ONE PRIMARY on a Change with a failure is *Look at what broke*, and
 *     it is a door to the line that broke, not a dead button asking for a run
 *     that has already happened (03-USER-LENS.md rules 6 and 7).
 *  3. THE FAILING ASSERTION'S OWN WORDS and an EVIDENCE-CLASS LABEL appear
 *     beside the finding, on the Change screen and in the packet. An absent
 *     failure line is stated as absent and never invented; an undeclared class
 *     is stated as undeclared and never read as ordinary.
 *
 * Every fixture here is synthetic. No string below came off a real machine.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-failure-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeScreen, ChangeShell } from './change-workspace.jsx';
  import { EvidenceRow } from './criterion-links.jsx';
  import { buildChangeView, approveBlocker } from './change-view.js';
  import { deriveChangeModel, failureOf } from './change-model.js';
  import { HQ_COPY } from './hq-copy.js';
  export { buildChangeView, approveBlocker, deriveChangeModel, failureOf, HQ_COPY };
  export const renderScreen = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderRoute = props => renderToStaticMarkup(React.createElement(ChangeShell, props));
  export const renderRow = props => renderToStaticMarkup(React.createElement(EvidenceRow, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'failure.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const { approveBlocker, deriveChangeModel, failureOf, renderScreen, renderRoute, renderRow, HQ_COPY } = M
const A = HQ_COPY.change.approve.disabled
const F = HQ_COPY.change.coverage.failure
const K = HQ_COPY.change.coverage.evidence_class

const CRITERIA = [
  'Two retry requests for the same delivery do not enqueue duplicate work.',
  'A delivery that was still waiting when the service was stopped is still exactly one delivery after it starts again, and it is attempted again.',
]

const CHANGE = {
  id: 'cccccccc-0000-4000-8000-00000000c001',
  project_id: 'cccccccc-0000-4000-8000-00000000p001',
  opened_at: '2026-01-02T09:00:00Z',
  request: { title: 'Retry a failed delivery', purpose: CRITERIA[0], requester: 'sam', source: 'person' },
  accepted_scope: {
    version: 1, criteria: CRITERIA, exclusions: [], allowed_scope: ['src/**', 'tests/**'],
    constraints: [], check_profile: ['cargo-test'],
    content_sha256: '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
    acceptance: { actor: 'sam', accepted_at: '2026-01-02T09:06:00Z', rationale: 'this is what I want checked' },
    proposed_at: '2026-01-02T09:05:00Z',
  },
  implementation: {
    version: 1, bound_at: '2026-01-02T10:00:00Z', repository_identity: 'repo-synthetic-0001',
    base_revision: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0', snapshot_id: null,
    workspace_identity: 'workspace-synthetic-0001',
  },
  associations: [], examination: 'none_yet', decision: 'none_yet',
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
}

/** The words the seeded run printed when it broke, as the record would hold them. */
const BROKE = 'assertion `left == right` failed: a second retry request while one is pending must return the same job, not a new one\nleft: "job_0002"\nright: "job_0003"'

const item = (over = {}) => ({
  link_id: 'cce_synthetic_0001', kind: 'check_run', evidence_id: 'evb_synthetic_0001',
  session_id: 'cli:check:2026-01-02T10:05:00Z', basis: 'named', basis_slug: 'basis_named',
  name: 'bash:/synthetic/repo:cargo test --test=duplicate_retry', observed: 'not_proven',
  bears: true, applies: true, applies_slug: 'inputs_applies', asserted_at: '2026-01-02T10:05:00Z',
  execution: 'completed', execution_slug: 'execution_completed',
  failure_line: BROKE, evidence_class: 'seeded', evidence_class_slug: 'evidence_class_seeded',
  ...over,
})

const coverage = (over = {}) => ({
  schema_version: 1, change_id: CHANGE.id, scope_version: 1, link_rows: 1, executions: [],
  criteria: [
    { index: 1, text: CRITERIA[0], state: 'evidence', state_slug: 'coverage_evidence', items: [item()], removed: [] },
    { index: 2, text: CRITERIA[1], state: 'none', state_slug: 'coverage_no_test_ran', items: [], removed: [] },
  ],
  ...over,
})

const model = (cov) => deriveChangeModel(CHANGE, cov || coverage(), null, null, [])
const text = (html) => html.replace(/<[^>]*>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/&#x60;/g, '`').replace(/\s+/g, ' ').trim()

/* ── 1. the blocker: failure > unlinked run > no evidence ──────────────── */

test('the record holding a failure is the reason Approve is off, and it outranks both counts', () => {
  const both = approveBlocker({
    criteria: CRITERIA, versionBound: true, problems: 1, withoutEvidence: 1, unlinkedRun: { id: 'rx_1' },
  })
  assert.equal(both.key, 'problem_one', 'a failure outranks an unlinked run AND a missing one')
  assert.equal(both.reason, A.problem_one.reason)
  assert.equal(both.enabler, A.problem_one.enabler)
  assert.match(both.enabler, /Look at what broke/)
  /* The sentence a Change with nothing run gets must not be this Change's. */
  assert.doesNotMatch(both.reason + ' ' + both.enabler, /has not run|no evidence yet/)
})

test('with no failure, an unlinked run outranks a missing one, and a missing one is said last', () => {
  const unlinked = approveBlocker({
    criteria: CRITERIA, versionBound: true, problems: 0, withoutEvidence: 1, unlinkedRun: { id: 'rx_1' },
  })
  assert.equal(unlinked.key, 'unlinked_one')
  const none = approveBlocker({ criteria: CRITERIA, versionBound: true, problems: 0, withoutEvidence: 1, unlinkedRun: null })
  assert.equal(none.key, 'no_evidence_one')
  /* And the three are three different sentences: a failure told as an absence
   * is the defect this card closed. */
  const failure = approveBlocker({ criteria: CRITERIA, versionBound: true, problems: 1, withoutEvidence: 0, unlinkedRun: null })
  assert.equal(new Set([failure.reason, unlinked.reason, none.reason]).size, 3)
})

test('the Approve stage renders the failure reason, not the never-ran one', () => {
  const html = renderScreen({ model: model().view, stage: 'approve' })
  const body = text(html)
  assert.match(body, /The record shows a problem with 1 thing that counts as done\./)
  assert.match(body, /Look at what broke before running anything\./)
  assert.doesNotMatch(body, /has not run/, 'a recorded failure is never "has not run"')
  assert.doesNotMatch(body, /1 thing that counts as done has no evidence yet/)
})

/* ── 2. the one primary action ─────────────────────────────────────────── */

test('the one primary action on a Change with a failure is a live door to the line that broke', () => {
  const m = model()
  assert.equal(m.state, 'not_proven')
  assert.equal(m.inspector.action.label, HQ_COPY.change.model.actions.look)
  assert.equal(m.inspector.action.enabled, true)
  assert.equal(m.inspector.action.door, 1, 'it opens the criterion the record holds a problem for')
  assert.doesNotMatch(m.inspector.action.label.toLowerCase(), /run/)
})

/* ── 3. the failure's own words, and where the finding came from ───────── */

/* THE TEST THAT WAS MISSING (verifier finding 1). Everything below rendered
 * `stage: 'approve'` or asserted against the model object, and the LIVE Verify
 * tree — the screen "Look at what broke" opens — was never exercised. It
 * carried neither the failure words nor the seeded label, because the items it
 * renders came from `buildCoverageView` and the sentences were attached a level
 * up, on the packet's copy. This renders the real component tree at the real
 * address, the way the screen is wired in `change-workspace.jsx`. */
test('the Verify stage — where "Look at what broke" sends Sam — carries what broke and the seeded label', () => {
  const m = model()
  const body = text(renderScreen({
    model: { ...m.view, coverage: m.coverage, hq: m },
    stage: 'verify',
  }))
  assert.match(body, /What the check said when it broke/, 'the live Verify tree shows what broke')
  assert.match(body, /must return the same job, not a new one/)
  assert.match(body, /job_0002/)
  assert.match(body, /job_0003/)
  assert.match(body, /Where this finding came from/, 'B16.10: wherever the finding is shown')
  /* And the one primary on this screen is the door, not a request for a run. */
  assert.match(body, /Look at what broke/)
  assert.match(body, /Set up on purpose to show what a failure looks like/)
  /* And the door lands on the line that broke, on this same screen. */
  assert.equal(m.inspector.action.door, m.criteria[0].n)
  assert.equal(m.criteria[0].state, 'not_proven')
})

/* T-3-6-F6-BRIEF (Phase 3 exit re-run, `Q2` ruled defect). THE TEST THAT WAS
 * STILL MISSING. The test above renders `ChangeScreen`, the six-stage screen —
 * and after the P6 merge nothing routes there on a fresh install: `ChangeShell`
 * substitutes `ChangeBrief` whenever the remembered altitude is `brief`, which
 * is what a profile that never chose one opens at, REGARDLESS of the URL's
 * stage segment. So F3's block was real, tested, and on a screen nobody
 * landed on. This renders the Change route the way a person actually reaches
 * it and asserts the failing assertion's own words and the seeded sentence are
 * in THAT dom. The stage segment is varied to prove it is ignored, which is
 * the fact that made the test above insufficient on its own. */
const route = (m, stage) => text(renderRoute({
  model: { ...m.view, coverage: m.coverage, hq: m },
  stage,
  altitude: 'brief',
  register: 'manager',
  onGo: () => {}, onCopy: () => {}, onReload: () => {}, onAltitude: () => {}, onRegister: () => {},
}))

for (const stage of ['define', 'verify', 'approve']) {
  test(`the Change route as routed — Brief, stage segment "${stage}" — carries what broke and the seeded label`, () => {
    const body = route(model(), stage)
    /* It really is Brief, and not the six-stage screen under another name. */
    assert.doesNotMatch(body, /Moving between them settles nothing/, 'Brief carries no stage strip')
    assert.match(body, /What the check said when it broke/, 'the assertion words are on the surface a fresh install opens')
    assert.match(body, /must return the same job, not a new one/)
    assert.match(body, /job_0002/)
    assert.match(body, /job_0003/)
    assert.match(body, /Where this finding came from/, 'B16.10: wherever the finding is shown')
    assert.match(body, /Set up on purpose to show what a failure looks like/)
  })
}

test('the Change route as routed, on a Change with no failure, carries no failure block', () => {
  const body = route(model(coverage({
    criteria: [
      { index: 1, text: CRITERIA[0], state: 'evidence', state_slug: 'coverage_evidence', items: [item({ observed: 'proven', failure_line: null, evidence_class: null, evidence_class_slug: null })], removed: [] },
    ],
  })), 'verify')
  assert.doesNotMatch(body, /What the check said when it broke/)
  assert.doesNotMatch(body, /Where this finding came from/)
})

test('the Verify stage of a Change with no failure carries no failure block', () => {
  const m = model(coverage({
    criteria: [
      { index: 1, text: CRITERIA[0], state: 'evidence', state_slug: 'coverage_evidence', items: [item({ observed: 'proven', failure_line: null, evidence_class: null, evidence_class_slug: null })], removed: [] },
    ],
  }))
  const body = text(renderScreen({ model: { ...m.view, coverage: m.coverage, hq: m }, stage: 'verify' }))
  assert.doesNotMatch(body, /What the check said when it broke/)
  assert.doesNotMatch(body, /Where this finding came from/)
})

test('the failing assertion\'s own words are beside the finding, in the DOM a person reads', () => {
  const m = model()
  const failure = m.criteria[0].items[0].failure
  assert.equal(failure.line, BROKE)
  /* And on the screen: the row that shows the finding shows what broke. */
  const body = text(renderRow({ item: m.criteria[0].items[0], actor: '', removed: false }))
  assert.match(body, /What the check said when it broke/)
  assert.match(body, /must return the same job, not a new one/)
  assert.match(body, /job_0002/)
  assert.match(body, /job_0003/)
  /* The class label is on the SAME row, because `B16.10` is about the label
   * being wherever the finding is, not somewhere else on the page. */
  assert.match(body, /Where this finding came from/)
  assert.match(body, /Set up on purpose to show what a failure looks like/)
})

test('a row whose record holds no failure line says so on the screen, and invents nothing', () => {
  const m = model(coverage({
    criteria: [
      { index: 1, text: CRITERIA[0], state: 'evidence', state_slug: 'coverage_evidence', items: [item({ failure_line: null, evidence_class: null, evidence_class_slug: null })], removed: [] },
    ],
  }))
  const body = text(renderRow({ item: m.criteria[0].items[0], actor: '', removed: false }))
  assert.match(body, /The record holds no line from the failure\./)
  assert.match(body, /Nobody wrote down whether this was set up as a demonstration\./)
})

test('a passing row carries no failure block at all', () => {
  const m = model(coverage({
    criteria: [
      { index: 1, text: CRITERIA[0], state: 'evidence', state_slug: 'coverage_evidence', items: [item({ observed: 'proven', failure_line: null })], removed: [] },
    ],
  }))
  const body = text(renderRow({ item: m.criteria[0].items[0], actor: '', removed: false }))
  assert.doesNotMatch(body, /What the check said when it broke/)
  assert.doesNotMatch(body, /Where this finding came from/)
})

test('an evidence class of seeded is labelled wherever the finding is shown — screen and packet', () => {
  const m = model()
  const failure = m.criteria[0].items[0].failure
  assert.equal(failure.classWord, 'seeded')
  assert.equal(failure.classText, K.evidence_class_seeded)
  assert.match(failure.classText, /Set up on purpose/)
  assert.match(failure.classText, /not something found in your own work/)

  const proof = m.packet.sections.find((s) => s.id === 'proof')
  const lines = proof.rows.flatMap((r) => r.lines).join(' · ')
  assert.match(lines, /must return the same job, not a new one/, 'the packet carries what broke')
  assert.match(lines, /Set up on purpose/, 'the packet carries the seeded label (B16.10)')
})

test('a failure the record holds no line for says so, and never invents one', () => {
  const m = model(coverage({
    criteria: [
      { index: 1, text: CRITERIA[0], state: 'evidence', state_slug: 'coverage_evidence', items: [item({ failure_line: null })], removed: [] },
      { index: 2, text: CRITERIA[1], state: 'none', state_slug: 'coverage_no_test_ran', items: [], removed: [] },
    ],
  }))
  const failure = m.criteria[0].items[0].failure
  assert.equal(failure.line, null)
  assert.equal(failure.none, F.none)
  assert.equal(failure.none, 'The record holds no line from the failure.')
  const proof = m.packet.sections.find((s) => s.id === 'proof')
  const lines = proof.rows.flatMap((r) => r.lines).join(' · ')
  assert.match(lines, /The record holds no line from the failure\./)
})

test('a run nobody declared a class for says nobody said — never "ordinary"', () => {
  const failure = failureOf({ failureLine: BROKE, evidenceClass: null, evidenceClassSlug: null })
  assert.equal(failure.classWord, null)
  assert.equal(failure.classText, K.none)
  assert.match(failure.classText, /Nobody wrote down/)
  /* The whole point of `B16.10`: silence must not read as field accuracy. */
  assert.doesNotMatch(failure.classText, /what happened when the work was done here/)
})

test('a class slug the copy file has no sentence for falls back to the absence, never to a slug', () => {
  const failure = failureOf({ evidenceClass: 'invented', evidenceClassSlug: 'evidence_class_invented' })
  assert.equal(failure.classText, K.none)
  assert.doesNotMatch(failure.classText, /evidence_class_/)
})

test('a line the record does NOT hold a problem for carries no failure block in the packet', () => {
  const m = model(coverage({
    criteria: [
      { index: 1, text: CRITERIA[0], state: 'evidence', state_slug: 'coverage_evidence', items: [item({ observed: 'proven', failure_line: null, evidence_class: null, evidence_class_slug: null })], removed: [] },
    ],
  }))
  const proof = m.packet.sections.find((s) => s.id === 'proof')
  const lines = proof.rows.flatMap((r) => r.lines).join(' · ')
  assert.doesNotMatch(lines, /The record holds no line from the failure/, 'a passing run has nothing that broke')
  assert.doesNotMatch(lines, /Where this finding came from/)
})
