/* RUN-LINK-TRUTH-R3 — no decide control is enabled anywhere while the engine
 * says the Change is not ready.
 *
 * Round 2's adversary (attack 2) rendered "What we checked" on a Change whose
 * only accepted line reads proven while the engine's readiness still needs
 * evidence — an unconfirmed `candidate_identity` obligation, which is tied to
 * no line — and found an enabled "Review and decide" button there, while the
 * Inspector on the same stage showed no decision and Brief's Approve was off.
 * `CriterionSummary` answered "may I offer the decision?" with a second rule
 * that never read `readiness.ready`. There is now one rule, `showsDecision`,
 * and every surface that draws a decide control reads it. Every string below
 * is synthetic.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, readdir, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-run-link-r3-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeBrief, Inspector, showsDecision } from './change-workspace.jsx';
  import { CriterionSummary } from './criterion-links.jsx';
  import { deriveChangeModel } from './change-model.js';
  import { buildChangeView } from './change-view.js';
  import { evidenceEntries, unlinkedCompletedRun } from './coverage-view.js';
  import { HQ_COPY } from './hq-copy.js';
  export { deriveChangeModel, showsDecision, buildChangeView, evidenceEntries, unlinkedCompletedRun, HQ_COPY };
  export const renderBrief = props => renderToStaticMarkup(React.createElement(ChangeBrief, props));
  export const renderInspector = props => renderToStaticMarkup(React.createElement(Inspector, props));
  export const renderSummary = props => renderToStaticMarkup(React.createElement(CriterionSummary, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'run-link-r3.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const C = M.HQ_COPY.change
const DECIDE = C.coverage.decide
const LINE = 'When the cart is empty, the total shows zero.'
const esc = (s) => s.replaceAll("'", '&#x27;')

const change = (decisions = []) => ({
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
  decisions,
})

const ITEM = {
  link_id: 'it1', kind: 'check_run', evidence_id: 'eb_test', session_id: 's1', name: 'cargo test',
  basis: 'manual', actor: 'sam', observed: 'proven', bears: true, applies: true, asserted_at: '2026-09-22T11:00:00Z',
}

/* The line's own item reads proven; `identity` decides whether the ENGINE is
 * ready. `identity: false` is the adversary's document exactly. */
const coverage = (identity) => ({
  scope_version: 1,
  link_rows: 1,
  execution_sessions: ['s1'],
  criteria: [{ index: 1, text: LINE, state: 'proven', items: [ITEM], removed: [] }],
  candidates: [],
  executions: [],
  readiness: {
    schema_version: 1, change_id: 'ch1', ready: identity, with_exceptions: false, scope_version: 1,
    target_version: 'e71c5b0aaaa', headline_slug: identity ? 'readiness_ready' : 'readiness_needs_evidence_one',
    obligations: [
      {
        id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '',
        outcome: identity ? 'satisfied' : 'needs_evidence', outcome_slug: identity ? 'satisfied' : 'needs_evidence',
        unsatisfied: identity ? null : 'identity_unconfirmed', unsatisfied_slug: identity ? null : 'readiness_identity_unconfirmed',
        evidence_name: null, linked_by: null, waiver: null, waivable: false,
      },
      {
        id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: LINE,
        outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null,
        evidence_name: 'cargo test', linked_by: 'sam', waiver: null, waivable: true,
      },
    ],
    rests_on: identity ? [{ obligation: 'criterion:1', criterion_index: 1, text: LINE, evidence_name: 'cargo test', linked_by: 'sam' }] : [],
    missing: identity ? [] : ['candidate_identity'], waived: [], not_applicable: [],
  },
})

function model(identity, decisions) {
  const ch = change(decisions)
  const cov = coverage(identity)
  const view = M.buildChangeView({ change: ch, proposals: null, work: [], evidence: M.evidenceEntries(cov), unlinkedRun: M.unlinkedCompletedRun(cov) })
  view.hq = M.deriveChangeModel(ch, cov, null, null, [])
  return view
}

const surfaces = (m) => ({
  brief: M.renderBrief({ model: m, onCopy: () => {}, onReload: () => {}, notes: [], register: 'manager', onExplore: () => {}, onDoor: () => {} }),
  inspector: M.renderInspector({ hq: m.hq, cards: [], onCopy: () => {}, onDecided: () => {}, onRecheck: () => {}, onDoor: () => {} }),
  summary: M.renderSummary({ hq: m.hq, onDecide: () => {} }),
})

/* An ENABLED button carrying the label. A disabled one (`Blocked`) says why. */
const enabledButton = (html, label) => [...html.matchAll(/<button\b([^>]*)>([^<]*)<\/button>/g)]
  .some(([, attrs, text]) => text === label && !/\bdisabled\b/.test(attrs))

/* ── the break round 2's adversary found ──────────────────────────────── */

test('"What we checked" offers no decide control while the engine says the version is not ready', () => {
  const m = model(false)
  assert.equal(m.hq.state, 'proven', 'the only line\'s own item reads proven')
  assert.equal(m.hq.readiness.ready, false, 'the engine is not ready: the identity is unconfirmed')
  assert.equal(M.showsDecision(m.hq), false)
  const { summary } = surfaces(m)
  assert.ok(!enabledButton(summary, DECIDE), 'an enabled "Review and decide" beside a not-ready record\n' + summary)
  assert.ok(!summary.includes('ws-cov-decide'), summary)
})

test('no surface enables a decide or approve control while readiness.ready is false', () => {
  const m = model(false)
  const approve = m.hq.decision.approveLabel
  for (const [name, html] of Object.entries(surfaces(m))) {
    assert.ok(!enabledButton(html, DECIDE), `${name} enables "${DECIDE}"`)
    assert.ok(!enabledButton(html, approve), `${name} enables "${approve}"`)
    assert.ok(!enabledButton(html, C.approve.action), `${name} enables "${C.approve.action}"`)
  }
  /* and Brief still says why, in the engine's words (round 1) */
  const { brief } = surfaces(m)
  const at = brief.indexOf('ws-brief-approve')
  assert.ok(at >= 0)
  assert.ok(brief.slice(at, at + 2000).includes(esc(m.hq.readiness.blocker.reason)))
})

test('when the engine IS ready, the summary and the Inspector offer the decision together', () => {
  const m = model(true)
  assert.equal(m.hq.readiness.ready, true)
  assert.equal(M.showsDecision(m.hq), true)
  const { summary, inspector } = surfaces(m)
  assert.ok(enabledButton(summary, DECIDE), summary)
  assert.ok(inspector.includes('ws-decision'), 'the Inspector draws the Decision the summary points at')
})

test('the rule agrees with every surface over the states the model reaches', () => {
  for (const identity of [false, true]) {
    const m = model(identity)
    const { summary, inspector } = surfaces(m)
    const rule = M.showsDecision(m.hq)
    assert.equal(enabledButton(summary, DECIDE), rule, `summary vs rule, ready=${identity}`)
    assert.equal(inspector.includes('class="ws-decision"'), rule, `inspector vs rule, ready=${identity}`)
  }
})

/* ── one rule, and nothing else reads the raw field ───────────────────── */

test('only the one rule reads decision.canApprove to decide whether to offer a decision', async () => {
  const files = (await readdir(here)).filter((f) => /\.(jsx?|mjs)$/.test(f) && !/\.test\.js$/.test(f))
  const readers = []
  for (const f of files) {
    const src = await readFile(join(here, f), 'utf8')
    if (/\.canApprove\b/.test(src)) readers.push(f)
  }
  assert.deepEqual(readers, ['change-model.js'], 'a second reader of canApprove is a second rule')
  const model = await readFile(join(here, 'change-model.js'), 'utf8')
  assert.equal((model.match(/\.canApprove\b/g) || []).length, 1, 'change-model.js reads canApprove once: in showsDecision')
  const links = await readFile(join(here, 'criterion-links.jsx'), 'utf8')
  assert.match(links, /showsDecision\(hq\)/, 'CriterionSummary reads the rule')
})
