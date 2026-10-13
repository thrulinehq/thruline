/* RUN-LINK-TRUTH-R2 — no surface shows a stale "still needed" pair beside the
 * unlinked-run sentence, and every leading state's off-primary reads the
 * ENGINE's served reason for that line (`actionBlocker`, one derivation).
 *
 * Round 1's adversary rendered "What we checked" on the unlinked-run Change and
 * found, two sentences apart, "A test ran for this Change but isn't linked to
 * it — link it." and a disabled "Run the check" saying "A check that supports
 * this line now is still needed." The served documents below are the shape
 * `change_readiness::compute` serves for that Change (the engine test
 * `crates/core/tests/unlinked_run_readiness.rs` builds it through the real
 * derivation). Every string is synthetic.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-run-link-r2-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeBrief, Inspector } from './change-workspace.jsx';
  import { CriterionSummary, CriterionCoverage } from './criterion-links.jsx';
  import { deriveChangeModel, readinessOf, actionBlocker, inspectorAction } from './change-model.js';
  import { buildChangeView } from './change-view.js';
  import { buildCoverageView, evidenceEntries, unlinkedCompletedRun } from './coverage-view.js';
  import { HQ_COPY } from './hq-copy.js';
  export { deriveChangeModel, readinessOf, actionBlocker, inspectorAction, buildChangeView, buildCoverageView, evidenceEntries, unlinkedCompletedRun, HQ_COPY };
  export const renderBrief = props => renderToStaticMarkup(React.createElement(ChangeBrief, props));
  export const renderInspector = props => renderToStaticMarkup(React.createElement(Inspector, props));
  export const renderSummary = props => renderToStaticMarkup(React.createElement(CriterionSummary, props));
  export const renderCoverage = props => renderToStaticMarkup(React.createElement(CriterionCoverage, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'run-link-r2.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const C = M.HQ_COPY.change
const MB = C.model.blocked
const R = C.model.readiness
const UNLINKED = R.reasons.readiness_unlinked_run
const LINE = 'When the cart is empty, the total shows zero.'
const RUN_NAME = 'cargo test'
const esc = (s) => s.replaceAll("'", '&#x27;')
const fillName = (s) => s.replace('{name}', RUN_NAME)

const CHANGE = {
  id: 'ch1',
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Empty cart total' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1,
    criteria: [LINE],
    allowed_scope: ['src/'],
    check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet',
  decisions: [],
}

const CANDIDATE = {
  kind: 'check_run', evidence_id: 'eb_test', session_id: 's1', name: RUN_NAME,
  observed: 'proven', bears: true, execution: 'completed', execution_slug: 'execution_completed',
}

const IDENTITY = {
  id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '',
  outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null,
  evidence_name: null, linked_by: null, waiver: null, waivable: false,
}

const coverage = (slug, candidates) => ({
  scope_version: 1,
  link_rows: 0,
  execution_sessions: ['s1'],
  criteria: [{ index: 1, text: LINE, state: 'none', items: [], removed: [] }],
  candidates,
  executions: [],
  readiness: {
    schema_version: 1, change_id: 'ch1', ready: false, with_exceptions: false, scope_version: 1,
    target_version: 'e71c5b0aaaa', headline_slug: 'readiness_needs_evidence_one',
    obligations: [IDENTITY, {
      id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: LINE,
      outcome: 'needs_evidence', outcome_slug: 'needs_evidence',
      unsatisfied: slug.replace(/^readiness_/, ''), unsatisfied_slug: slug,
      evidence_name: null, linked_by: null, waiver: null, waivable: true,
    }],
    rests_on: [], missing: ['criterion:1'], waived: [], not_applicable: [],
  },
})

const unlinkedCoverage = () => coverage('readiness_unlinked_run', [CANDIDATE])

function model(cov) {
  const change = M.buildChangeView({ change: CHANGE, proposals: null, work: [], evidence: M.evidenceEntries(cov), unlinkedRun: M.unlinkedCompletedRun(cov) })
  change.hq = M.deriveChangeModel(CHANGE, cov, null, null, [])
  return change
}

/* ── the break round 1's adversary found ──────────────────────────────── */

test('"What we checked" on an unlinked run: the off primary reads the engine reason for the line, not "still needed"', () => {
  const m = model(unlinkedCoverage())
  assert.equal(m.hq.state, 'in_progress', 'an unlinked-run line leads as in progress')
  const a = m.hq.inspector.action
  /* D-050 (RUN-LINK-TRUTH-R4): the off primary says why APPROVAL is off, in
   * the one answer (`readiness.blocker`), which for this Change is the
   * unlinked run's own pair naming the run. It used to be the line's own
   * sentence (UNLINKED), which the Verify stage still prints. */
  assert.equal(a.reason, fillName(R.blocked_unlinked.reason), 'the action reason is the one blocker, chosen from the engine\'s reason')
  assert.equal(a.enabler, R.blocked_unlinked.enabler, 'the enabler names the one next act: link it')
  assert.doesNotMatch(a.enabler, /turns on/)

  const summary = M.renderSummary({ hq: m.hq })
  assert.ok(summary.includes(esc(fillName(R.blocked_unlinked.reason))), summary)
  assert.ok(!summary.includes(MB.in_progress.reason), 'no "still needed" beside the unlinked-run sentence')
  assert.ok(!summary.includes(esc(MB.in_progress.enabler)), 'no "Open Check it again" beside it either')

  const cov = M.buildCoverageView(unlinkedCoverage())
  const section = M.renderCoverage({ changeId: 'ch1', view: cov, hq: m.hq })
  assert.ok(section.length > 0)
  assert.ok(section.includes(esc(UNLINKED)), 'the line still reads its own sentence (RUN-LINK-TRUTH-R4 item 4)')
  assert.ok(!section.includes(MB.in_progress.reason), 'the whole section never contradicts the line\'s own sentence')
})

test('no rendered surface of the unlinked-run Change prints the stale state-keyed pair', () => {
  const m = model(unlinkedCoverage())
  const surfaces = {
    brief: M.renderBrief({ model: m, onCopy: () => {}, onReload: () => {}, notes: [], register: 'manager', onExplore: () => {}, onDoor: () => {} }),
    inspector: M.renderInspector({ hq: m.hq, cards: [], onCopy: () => {}, onDecided: () => {}, onRecheck: () => {}, onDoor: () => {} }),
    summary: M.renderSummary({ hq: m.hq }),
  }
  for (const [name, dom] of Object.entries(surfaces)) {
    assert.ok(!dom.includes(MB.in_progress.reason), `${name} prints "still needed" over an unlinked run`)
  }
})

/* ── one derivation: every leading state reads the engine's reason ───── */

test('actionBlocker reads the engine\'s reason for the leading line in every non-door state', () => {
  const readiness = M.readinessOf(unlinkedCoverage(), 'e71c5b0')
  const approve = { reason: 'approve-off', enabler: 'approve-enabler' }
  /* D-050 (RUN-LINK-TRUTH-R4): every state reads the one blocker, not the
   * leading line's own sentence. */
  for (const state of ['in_progress', 'recorded', 'needs_recheck', 'not_proven', 'read']) {
    const pair = M.actionBlocker(state, approve, readiness, 1)
    assert.equal(pair.reason, fillName(R.blocked_unlinked.reason), state)
    assert.equal(pair.enabler, R.blocked_unlinked.enabler, state)
  }
  const other = M.readinessOf(coverage('readiness_stale', []), 'e71c5b0')
  const pair = M.actionBlocker('needs_recheck', approve, other, 1)
  assert.equal(pair.reason, R.blocked_by.readiness_stale.reason, 'any served reason has its own pair, not only the unlinked one')
  assert.equal(pair.enabler, R.blocked_by.readiness_stale.enabler, 'with its own next act')
})

test('the state-keyed pair is the fallback only when the engine serves no reason for that line', () => {
  const approve = { reason: 'approve-off', enabler: 'approve-enabler' }
  const readiness = M.readinessOf(unlinkedCoverage(), 'e71c5b0')
  /* D-050 (RUN-LINK-TRUTH-R4): with readiness served, which line leads does
   * not matter — the one blocker. With no readiness, each falls back. */
  assert.deepEqual(M.actionBlocker('in_progress', approve, readiness, 2), { reason: readiness.blocker.reason, enabler: readiness.blocker.enabler })
  assert.deepEqual(M.actionBlocker('in_progress', approve, null, 1), { reason: MB.in_progress.reason, enabler: MB.in_progress.enabler })
  assert.deepEqual(M.actionBlocker('recorded', approve, readiness, null), { reason: readiness.blocker.reason, enabler: readiness.blocker.enabler })
  assert.deepEqual(M.actionBlocker('mystery', approve, null, null), { reason: MB.read.reason, enabler: MB.read.enabler })
  /* proven keeps the Approve door's own answer */
  assert.deepEqual(M.actionBlocker('proven', approve, null, 1), { reason: 'approve-off', enabler: 'approve-enabler' })
  assert.deepEqual(M.actionBlocker('proven', approve, readiness, null), { reason: readiness.blocker.reason, enabler: readiness.blocker.enabler })
})

test('the "Look at what broke" door is untouched: it stays on with no pair', () => {
  const readiness = M.readinessOf(coverage('readiness_not_proven', []), 'e71c5b0')
  const a = M.inspectorAction('not_proven', 1, { reason: 'x', enabler: 'y' }, readiness)
  assert.equal(a.enabled, true)
  assert.equal(a.door, 1)
  assert.equal(a.reason, '')
})

/* ── round 1's guarantees hold ────────────────────────────────────────── */

test('round 1 holds: nothing links automatically, and Brief\'s disabled Approve carries the readiness blocker', () => {
  const m = model(unlinkedCoverage())
  assert.equal(m.hq.readiness.ready, false)
  assert.notEqual(m.hq.criteria[0].state, 'proven', 'an unlinked run never counts')
  assert.equal(m.hq.criteria[0].items.length, 0, 'nothing is linked to the line automatically')
  const brief = M.renderBrief({ model: m, onCopy: () => {}, onReload: () => {}, notes: [], register: 'manager', onExplore: () => {}, onDoor: () => {} })
  const at = brief.indexOf('ws-brief-approve')
  assert.ok(at >= 0)
  assert.ok(brief.slice(at, at + 2000).includes(esc(m.hq.readiness.blocker.reason)))
})
