/* RUN-LINK-TRUTH (path steps 6 and 8: PW-F9, PW-F10b, PW-F11 words, PW-F14,
 * PW-F15) — a passing check run from the Change that no line is linked to is
 * named as exactly that on every surface, by the ENGINE's served reason; the
 * disabled Approve names that blocker and only the next act; Brief shows the
 * disabled Approve; and a line that counts on a person's link names them.
 *
 * The served documents below are the shape `change_readiness::compute` and
 * `change_evidence::coverage` serve for the walk's Change (see the engine test
 * `crates/core/tests/unlinked_run_readiness.rs`, which builds the same case
 * through the real derivation). Every string is synthetic.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-run-link-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeBrief } from './change-workspace.jsx';
  import { RunSheetBody } from './run-sheet.jsx';
  import { runSheetModel } from './run-route.js';
  import { deriveChangeModel, readinessOf } from './change-model.js';
  import { buildChangeView } from './change-view.js';
  import { coverageRows, evidenceEntries, unlinkedCompletedRun } from './coverage-view.js';
  import { HQ_COPY } from './hq-copy.js';
  export { deriveChangeModel, readinessOf, buildChangeView, coverageRows, evidenceEntries, unlinkedCompletedRun, HQ_COPY, runSheetModel };
  export const renderBrief = props => renderToStaticMarkup(React.createElement(ChangeBrief, props));
  export const renderSheet = props => renderToStaticMarkup(React.createElement(RunSheetBody, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'run-link.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const C = M.HQ_COPY.change
const UNLINKED = C.coverage.states.unlinked_run
const LINE = 'When the cart is empty, the total shows zero.'
const RUN_NAME = 'cargo test'

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

/** Step 6 of the walk: the run passed, and no line is linked to it. */
const unlinkedCoverage = () => ({
  scope_version: 1,
  link_rows: 0,
  execution_sessions: ['s1'],
  criteria: [{ index: 1, text: LINE, state: 'none', items: [], removed: [] }],
  candidates: [CANDIDATE],
  executions: [],
  readiness: {
    schema_version: 1, change_id: 'ch1', ready: false, with_exceptions: false, scope_version: 1,
    target_version: 'e71c5b0aaaa', headline_slug: 'readiness_needs_evidence_one',
    obligations: [IDENTITY, {
      id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: LINE,
      outcome: 'needs_evidence', outcome_slug: 'needs_evidence',
      unsatisfied: 'unlinked_run', unsatisfied_slug: 'readiness_unlinked_run',
      evidence_name: null, linked_by: null, waiver: null, waivable: true,
    }],
    rests_on: [], missing: ['criterion:1'], waived: [], not_applicable: [],
  },
})

/** After a named person linked it and the Change was checked again. */
const linkedCoverage = () => ({
  scope_version: 1,
  link_rows: 1,
  execution_sessions: ['s1'],
  criteria: [{
    index: 1, text: LINE, state: 'evidence', removed: [],
    items: [{
      link_id: 'l1', evidence_id: 'eb_test', session_id: 's1', basis: 'manual', basis_slug: 'link_basis_manual',
      actor: 'walker', asserted_at: '2026-09-22T11:00:00Z', kind: 'check_run', name: RUN_NAME,
      observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies',
      fresh: true, fresh_slug: 'applicability_still_applies', execution: 'completed', execution_slug: 'execution_completed',
    }],
  }],
  candidates: [CANDIDATE],
  executions: [],
  readiness: {
    schema_version: 1, change_id: 'ch1', ready: true, with_exceptions: false, scope_version: 1,
    target_version: 'e71c5b0aaaa', headline_slug: 'readiness_ready',
    obligations: [IDENTITY, {
      id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: LINE,
      outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null,
      evidence_name: RUN_NAME, linked_by: 'walker', waiver: null, waivable: true,
    }],
    rests_on: [{ obligation_id: 'criterion:1', text: LINE, evidence_name: RUN_NAME, linked_by: 'walker' }],
    missing: [], waived: [], not_applicable: [],
  },
})

/** A Change with nothing run at all: the old reason, untouched. */
const nothingCoverage = () => {
  const doc = unlinkedCoverage()
  doc.candidates = []
  doc.readiness.obligations[1] = { ...doc.readiness.obligations[1], unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked' }
  return doc
}

function model(coverage) {
  const change = M.buildChangeView({ change: CHANGE, proposals: null, work: [], evidence: M.evidenceEntries(coverage), unlinkedRun: M.unlinkedCompletedRun(coverage) })
  change.hq = M.deriveChangeModel(CHANGE, coverage, null, null, [])
  return change
}

const brief = (coverage) => M.renderBrief({ model: model(coverage), onCopy: () => {}, onReload: () => {}, notes: [], register: 'manager', onExplore: () => {}, onDoor: () => {} })

/* ── one reason, one sentence ─────────────────────────────────────────── */

test('the engine reason readiness_unlinked_run reads the Verify stage\'s own sentence, with no new state word', () => {
  assert.equal(C.model.readiness.reasons.readiness_unlinked_run, UNLINKED)
  const r = M.readinessOf(unlinkedCoverage(), 'e71c5b0')
  assert.equal(r.missing.length, 1)
  assert.equal(r.missing[0].reason, UNLINKED, 'the Approve stage and the Inspector print the Verify stage\'s reason')
  assert.notEqual(r.missing[0].reason, C.model.readiness.reasons.readiness_nothing_looked)
})

test('unlinkedCompletedRun and the engine name the same lines', () => {
  const cases = [unlinkedCoverage(), nothingCoverage(), linkedCoverage()]
  for (const doc of cases) {
    const screen = M.coverageRows(doc).filter((row) => row.state === 'unlinked_run').map((row) => row.n)
    const engine = doc.readiness.obligations
      .filter((o) => o.unsatisfied_slug === 'readiness_unlinked_run')
      .map((o) => o.criterion_index)
    assert.deepEqual(screen, engine)
  }
})

/* ── the disabled Approve ─────────────────────────────────────────────── */

test('the disabled Approve names the unlinked run and the act of linking it in What we checked, and promises nothing', () => {
  const b = M.readinessOf(unlinkedCoverage(), 'e71c5b0').blocker
  assert.ok(b.reason.includes(RUN_NAME), `the reason names the run: ${b.reason}`)
  assert.match(b.reason, /isn't linked/)
  assert.match(b.enabler, /[Ll]ink/)
  assert.ok(b.enabler.includes('What we checked'), `the enabler says where: ${b.enabler}`)
  assert.doesNotMatch(b.enabler, /turns on/, 'the enabler names the next act, never the result')
  assert.doesNotMatch(b.reason + b.enabler, /Run the checks that cover/)
})

test('the generic fallback names the next act and never promises that it turns Approve on', () => {
  const b = M.readinessOf(nothingCoverage(), 'e71c5b0').blocker
  assert.ok(b.reason && b.enabler)
  assert.doesNotMatch(b.enabler, /turns on/)
})

test('no Approve enabler says "Nothing on this screen runs a check." — the run sheet does', () => {
  for (const [key, pair] of Object.entries(C.approve.disabled)) {
    assert.doesNotMatch(String(pair.enabler || ''), /Nothing on this screen runs a check/, key)
  }
})

/* ── every surface reads the served reason ────────────────────────────── */

test('the top sentence, the Inbox reason and the Brief no longer say "No test ran" or "It has no evidence yet." over an unlinked passing run', () => {
  const m = model(unlinkedCoverage())
  assert.doesNotMatch(m.hq.headline.text, /no evidence yet/)
  assert.match(m.hq.headline.text, /isn't linked/)
  assert.equal(m.hq.inboxRow.reason, m.hq.headline.text)
  const dom = brief(unlinkedCoverage())
  assert.ok(dom.includes(UNLINKED.replaceAll("'", '&#x27;')), 'Brief prints the served reason')
  assert.ok(!dom.includes('No test ran for this.'), 'Brief never says no test ran')
  assert.ok(!dom.includes('It has no evidence yet.'))
})

test('a Change with nothing run keeps "No test ran for this." and its top sentence', () => {
  const m = model(nothingCoverage())
  assert.equal(m.hq.headline.text, 'Accepted: 1 thing counts as done. It has no evidence yet.')
  assert.equal(M.readinessOf(nothingCoverage(), 'e71c5b0').missing[0].reason, 'No test ran for this.')
})

test('Brief shows "Approve this version" disabled, as a secondary, with the Approve stage\'s own pair (PW-F15)', () => {
  const dom = brief(unlinkedCoverage())
  const pair = M.readinessOf(unlinkedCoverage(), 'e71c5b0').blocker
  const at = dom.indexOf('ws-brief-approve')
  assert.ok(at >= 0, 'Brief carries an Approve control')
  const block = dom.slice(at, at + 2000)
  assert.match(block, /<button[^>]*disabled=""[^>]*>Approve this version<\/button>/)
  assert.doesNotMatch(block.slice(0, block.indexOf('Approve this version')), /ws-change-primary/, 'it is not the primary')
  assert.ok(block.includes(pair.reason.replaceAll("'", '&#x27;')), 'the reason is the Approve stage\'s')
  assert.ok(block.includes(pair.enabler.replaceAll("'", '&#x27;')), 'the enabler is the Approve stage\'s')
})

/* ── a person's link is named wherever the line counts on it ─────────── */

test('a line that counts on a person\'s link names them — on Brief, the packet and the exported rests-on list', () => {
  const m = model(linkedCoverage())
  const r = m.hq.readiness
  assert.equal(r.ready, true)
  assert.equal(r.restsOn.length, 1)
  assert.ok(r.restsOn[0].line.includes('walker'), r.restsOn[0].line)
  assert.ok(m.hq.packet.readinessRests.some((line) => line.includes('walker')), 'the packet and the export read this list')
  assert.ok(brief(linkedCoverage()).includes('walker'), 'Brief names the person')
  assert.ok(m.hq.criteria[0].line.startsWith('Proven'), m.hq.criteria[0].line)
  assert.ok(m.hq.criteria[0].line.includes('linked by walker'), 'a Proven line on a person\'s link names them: ' + m.hq.criteria[0].line)
  const dom = brief(linkedCoverage())
  for (const proven of dom.match(/Proven: [^<]*/g) || []) assert.ok(proven.includes('linked by walker'), proven)
})

test('"What the check says now" names the person too, when its Proven line counts on their link', () => {
  const m = model(linkedCoverage())
  const held = m.hq.repairs.filter((r) => r.kind === 'c')
  assert.ok(held.length > 0, 'the linked run holds on this version: ' + JSON.stringify(m.hq.repairs))
  for (const r of held) {
    assert.ok(r.sentence.startsWith('Proven'), r.sentence)
    assert.ok(r.sentence.includes('linked by walker'), 'a Proven line on a person\'s link names them: ' + r.sentence)
  }
})

/* ── the run sheet ────────────────────────────────────────────────────── */

const passedSheet = {
  schema_version: 1, change: 'ch1', project_folder: '~/shop',
  permission: { granted_by: 'walker', granted_at: '2026-09-23T10:00:00Z' },
  checks: [{ id: 'cargo-test', program: 'cargo', category: 'tests', command: 'cargo test',
    last: { run: 'r1', state: 'completed', outcome: 'passed', exit_code: 0, tests_passed: 2, tests_failed: 0, suites: 1, recorded_at: '2026-09-23T10:05:00Z', settles: true } }],
  environments: [{ id: 'hq', kind: 'hq', folder: null, has: ['cargo'] }],
  not_runnable_here: [], remembered_environment: null, not_started: null, actor: 'walker', job: null,
}

test('a Passed run no line links says so in the same words and offers one door to that line', () => {
  const m = model(unlinkedCoverage())
  const unlinked = m.hq.readiness.unlinked
  assert.deepEqual(unlinked.map((u) => u.n), [1])
  const sheet = M.runSheetModel(passedSheet, { version: { short: 'e71c5b0' }, unlinked })
  assert.equal(sheet.outcome.kind, 'passed')
  const dom = M.renderSheet({ model: sheet, on: {} })
  assert.ok(dom.includes(UNLINKED.replaceAll("'", '&#x27;')), 'the sheet says the run is not linked')
  const doors = dom.match(/ws-run-unlinked-door/g) || []
  assert.equal(doors.length, 1, 'one door to the line\'s link control')

  const linked = M.runSheetModel(passedSheet, { version: { short: 'e71c5b0' }, unlinked: model(linkedCoverage()).hq.readiness.unlinked })
  assert.ok(!M.renderSheet({ model: linked, on: {} }).includes('ws-run-unlinked'), 'a linked run needs no door')
})
