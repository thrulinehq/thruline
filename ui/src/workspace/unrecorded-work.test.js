import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* UNRECORDED-WORK-NOT-SILENT (launch card, path steps 3–4: PW-F7 part A and
 * PW-F6's founder-free top line).
 *
 * A piece of work linked to a Change can have NO recorded steps in ThruLine —
 * a transcript registered from the first screen, with no hooks installed, is
 * read into the core ledger but never into the store the Change reads the
 * agent's words from. Before this card, such a Change said "The record holds
 * nothing the agent said about this Change." and "Nothing in the 1 piece of
 * work you linked contradicts what was said." Both read as a clean record,
 * and the transcript holds a completion claim. Neither sentence was true.
 *
 * What this file holds the product to:
 *  - one derivation (`unrecordedWorkOf` in change-model.js) decides which
 *    linked pieces of work ThruLine holds no steps for;
 *  - the "what the agent said" section never prints `said_none` while such
 *    work is linked, and says instead that ThruLine holds none of its steps;
 *  - the "contradicts" line does the same;
 *  - the top of the Change (Brief and Explore) carries one sentence saying
 *    what the agent said there was not recorded and has not been checked;
 *  - nothing the Change DECIDES moves: no state word, count, readiness answer
 *    or approval changes because of it — it only claims less;
 *  - it quotes nothing from the agent's log (FOUNDER-ASKS #19).
 *
 * Every fixture is synthetic. No string here came off a real machine.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-unrecorded-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeShell, ChangeScreen } from './change-workspace.jsx';
  import { PacketPage } from './packet.jsx';
  import { buildPacketView } from './packet-view.js';
  import { buildChangeView } from './change-view.js';
  import * as Model from './change-model.js';
  import { HQ_COPY } from './hq-copy.js';
  import { fill } from './hq-narrate.js';
  import { renderExport } from './packet-export.js';
  export { buildPacketView, buildChangeView, Model, HQ_COPY, fill, renderExport };
  export const renderShell = props => renderToStaticMarkup(React.createElement(ChangeShell, props));
  export const renderScreen = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderPage = props => renderToStaticMarkup(React.createElement(PacketPage, props));
`

const bundled = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  stdin: { contents: ENTRY, resolveDir: here },
})
const file = join(temporary, 'unrecorded.mjs')
await writeFile(file, bundled.outputFiles[0].text)
const U = await import(pathToFileURL(file).href)
const { buildPacketView, buildChangeView, Model, HQ_COPY, fill, renderExport } = U

const MD = HQ_COPY.change.packet.markdown
const V = HQ_COPY.change.verify
const T = HQ_COPY.change.unrecorded
const SAID_NONE = 'The record holds nothing the agent said about this Change.'

const SESSION_A = 'cccccccc-0000-4000-8000-00000000u001'
const SESSION_B = 'cccccccc-0000-4000-8000-00000000u002'
const SESSION_X = 'cccccccc-0000-4000-8000-00000000u0e1'

const association = (id, session) => ({ id, session_id: session, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-01-02T10:01:00Z', reason: null })

function aChange(sessions) {
  return {
    id: 'cccccccc-0000-4000-8000-00000000c0u1',
    project_id: 'cccccccc-0000-4000-8000-00000000p0u1',
    opened_at: '2026-01-02T09:00:00Z',
    request: { title: 'Add the cart total', purpose: 'The cart shows no total.', requester: 'sam', source: 'person' },
    accepted_scope: {
      version: 1,
      criteria: ['The cart shows a total.', 'The total is the sum of the prices.'],
      exclusions: [], allowed_scope: [], constraints: [], check_profile: [],
      content_sha256: '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
      acceptance: { actor: 'sam', accepted_at: '2026-01-02T09:06:00Z', rationale: 'That is what I asked for.' },
      proposed_at: '2026-01-02T09:05:00Z',
    },
    implementation: {
      version: 1, bound_at: '2026-01-02T10:00:00Z', repository_identity: 'repo-synthetic-0001',
      base_revision: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0', snapshot_id: null, workspace_identity: 'workspace-synthetic-0001',
    },
    associations: sessions.map((s, i) => association('a' + (i + 1), s)),
    examination: 'none_yet',
    decision: 'none_yet',
    masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
  }
}

const item = (observed, extra) => ({
  link_id: 'l1', evidence_id: 'e1', session_id: SESSION_X, basis: 'declared', kind: 'check_run', name: 'cargo test',
  observed, bears: true, applies: true, applies_slug: 'inputs_applies', ...(extra || {}),
})
/* Three coverage shapes: nothing looked at, one line proven, one not proven. */
const COVERAGES = {
  nothing: { scope_version: 1, link_rows: 0, execution_sessions: [], criteria: [
    { index: 1, text: 'The cart shows a total.', state: 'none', items: [] },
    { index: 2, text: 'The total is the sum of the prices.', state: 'none', items: [] },
  ], candidates: [] },
  proven: { scope_version: 1, link_rows: 1, execution_sessions: [SESSION_X], criteria: [
    { index: 1, text: 'The cart shows a total.', state: 'evidence', items: [item('proven', { fresh: true })] },
    { index: 2, text: 'The total is the sum of the prices.', state: 'none', items: [] },
  ], candidates: [] },
  not_proven: { scope_version: 1, link_rows: 1, execution_sessions: [SESSION_X], criteria: [
    { index: 1, text: 'The cart shows a total.', state: 'evidence', items: [item('not_proven', { failure_line: 'FAILED total: expected 3, saw 0' })] },
    { index: 2, text: 'The total is the sum of the prices.', state: 'none', items: [] },
  ], candidates: [] },
}

/* One finding card, as `buildVerdict` produces it, for a piece of work that
 * WAS recorded and holds a claim. */
const CLAIM = {
  id: 'clm_1', state: 'not_proven', kind: 'said_done_no_proof',
  said: { text: 'The AI said this was done at step 4: "All tests pass."' },
  record: { text: 'Not proven: nothing in the record shows this was done.' },
}

/* The loader's own shape (`readChangeDocuments`): `steps` is always present,
 * a number when the reading counted, null when it could not. Since R2 it also
 * carries `ended` and `claims` (`claimCaptureOf`): a recorded piece of work
 * here is one whose end was recorded, so a claim would have been captured,
 * holding `claims` of them (unrecorded-capture.test.js covers every shape
 * through the real loader). */
const unrecorded = (id) => ({ id, findings: [], verdict: null, focus: null, steps: 0, ended: false, claims: 0 })
const unknown = (id) => ({ id, findings: [], verdict: null, focus: null, steps: null, ended: null, claims: null })
const recorded = (id, findings = []) => ({ id, findings, verdict: null, focus: null, steps: 17, ended: true, claims: findings.length })

const said = (hq) => hq.packet.sections.find((s) => s.id === 'said')
const saidText = (hq) => {
  const s = said(hq)
  return s.rows.length ? s.rows.map((r) => [r.text, ...r.lines].join('\n')).join('\n') : s.none
}
const text = (html) => html.replace(/<[^>]+>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ')

/* ── the derivation ───────────────────────────────────────────────────── */

test('unrecordedWorkOf names every linked piece of work ThruLine holds no steps for, and only those', () => {
  const of = (work) => Model.unrecordedWorkOf(work)
  assert.equal(of([]).count, 0)
  assert.equal(of([recorded(SESSION_A)]).count, 0)
  assert.deepEqual(of([unrecorded(SESSION_A)]).ids, [SESSION_A])
  assert.deepEqual(of([unknown(SESSION_A)]).ids, [SESSION_A], 'a count the reading could not make is not a count of steps')
  assert.deepEqual(of([recorded(SESSION_A), unrecorded(SESSION_B)]).ids, [SESSION_B])
  assert.equal(of([unrecorded(SESSION_A), unknown(SESSION_B)]).count, 2)
})

/* ── the said section ─────────────────────────────────────────────────── */

test('one unrecorded piece of work: the said section never prints said_none, and says ThruLine holds none of its steps', () => {
  const hq = Model.deriveChangeModel(aChange([SESSION_A]), COVERAGES.nothing, null, null, [unrecorded(SESSION_A)])
  const body = saidText(hq)
  assert.ok(!body.includes(SAID_NONE), body)
  assert.ok(!body.includes(MD.said_none), body)
  assert.ok(body.includes(MD.said_unrecorded_one), body)
})

test('an unknown step count reads the same as none: never said_none', () => {
  const hq = Model.deriveChangeModel(aChange([SESSION_A]), COVERAGES.nothing, null, null, [unknown(SESSION_A)])
  assert.ok(saidText(hq).includes(MD.said_unrecorded_one))
  assert.ok(!saidText(hq).includes(SAID_NONE))
})

test('two unrecorded pieces of work: the many form, with the count', () => {
  const hq = Model.deriveChangeModel(aChange([SESSION_A, SESSION_B]), COVERAGES.nothing, null, null, [unrecorded(SESSION_A), unknown(SESSION_B)])
  assert.ok(saidText(hq).includes(fill(MD.said_unrecorded_many, { n: 2 })), saidText(hq))
  assert.ok(!saidText(hq).includes(SAID_NONE))
})

test('recorded work holding a claim beside unrecorded work: both are said, and said_none is not', () => {
  const hq = Model.deriveChangeModel(aChange([SESSION_A, SESSION_B]), COVERAGES.nothing, null, null, [recorded(SESSION_A, [CLAIM]), unrecorded(SESSION_B)])
  const body = saidText(hq)
  assert.ok(body.includes(fill(MD.said, { said: CLAIM.said.text })), body)
  assert.ok(body.includes(MD.said_unrecorded_one), body)
  assert.ok(!body.includes(SAID_NONE))
})

test('said_none still renders when every linked piece of work was recorded and none holds a claim', () => {
  const hq = Model.deriveChangeModel(aChange([SESSION_A, SESSION_B]), COVERAGES.nothing, null, null, [recorded(SESSION_A), recorded(SESSION_B)])
  assert.equal(said(hq).rows.length, 0)
  assert.equal(said(hq).none, MD.said_none)
  assert.equal(hq.unrecorded.count, 0)
})

test('nothing linked: the said section is unchanged (said_none, no unrecorded line)', () => {
  const hq = Model.deriveChangeModel(aChange([]), COVERAGES.nothing, null, null, [])
  assert.equal(said(hq).rows.length, 0)
  assert.equal(said(hq).none, MD.said_none)
})

test('the Markdown and every exported format carry the same sentence, from hq.packet.sections', () => {
  const hq = Model.deriveChangeModel(aChange([SESSION_A]), COVERAGES.nothing, null, null, [unrecorded(SESSION_A)])
  const md = Model.packetMarkdown(hq)
  assert.ok(md.includes(MD.said_unrecorded_one), md)
  assert.ok(!md.includes(SAID_NONE))
  for (const format of ['md', 'html', 'json']) {
    const out = renderExport(hq.packet, null, { format })
    const body = typeof out === 'string' ? out : (out.text || out.content || JSON.stringify(out))
    assert.ok(body.includes(MD.said_unrecorded_one.replace(/'/g, format === 'html' ? '&#39;' : "'")), format + ': ' + body.slice(0, 400))
    assert.ok(!body.includes(SAID_NONE), format)
  }
})

/* ── the "contradicts" line ───────────────────────────────────────────── */

test('the Change screen\'s Verify view does not say nothing contradicts what was said, for unrecorded work', () => {
  const change = aChange([SESSION_A])
  const work = [unrecorded(SESSION_A)]
  const model = { ...buildChangeView({ change, work }), hq: Model.deriveChangeModel(change, COVERAGES.nothing, null, null, work) }
  const body = text(U.renderScreen({ model, stage: 'verify' }))
  assert.ok(!body.includes('contradicts what was said'), body)
  assert.ok(body.includes(V.unrecorded_one), body)
})

test('the whole-story page does not say nothing contradicts what was said, for unrecorded work', () => {
  const change = aChange([SESSION_A])
  const work = [unrecorded(SESSION_A)]
  const hq = Model.deriveChangeModel(change, COVERAGES.nothing, null, null, work)
  const model = { ...buildPacketView({ change, work, coverage: COVERAGES.nothing, evidence: [], hq }), hq }
  const body = text(U.renderPage({ model }))
  assert.ok(!body.includes('contradicts what was said'), body)
  assert.ok(!body.includes(SAID_NONE), body)
  assert.ok(body.includes(V.unrecorded_one), body)
  assert.ok(body.includes(MD.said_unrecorded_one), body)
})

test('recorded work keeps the existing contradicts sentence (control)', () => {
  const change = aChange([SESSION_A])
  const work = [recorded(SESSION_A)]
  const model = { ...buildChangeView({ change, work }), hq: Model.deriveChangeModel(change, COVERAGES.nothing, null, null, work) }
  const body = text(U.renderScreen({ model, stage: 'verify' }))
  assert.ok(body.includes(V.nothing_wrong_unchecked_one), body)
  assert.ok(!body.includes(V.unrecorded_one))
})

/* ── the top line (PW-F6, founder-free) ───────────────────────────────── */

test('the top of the Change, at Brief and at Explore, says what the agent said was not recorded and not checked', () => {
  const change = aChange([SESSION_A])
  const work = [unrecorded(SESSION_A)]
  const model = { ...buildChangeView({ change, work }), hq: Model.deriveChangeModel(change, COVERAGES.nothing, null, null, work) }
  assert.equal(T.top_one, 'What the agent said in this work was not recorded, so it has not been checked.')
  for (const altitude of ['brief', 'explore']) {
    const body = text(U.renderShell({ model, stage: 'define', altitude, register: 'manager' }))
    assert.ok(body.includes(T.top_one), altitude + ': ' + body.slice(0, 600))
  }
})

test('no top line when every linked piece of work was recorded', () => {
  const change = aChange([SESSION_A])
  const work = [recorded(SESSION_A)]
  const model = { ...buildChangeView({ change, work }), hq: Model.deriveChangeModel(change, COVERAGES.nothing, null, null, work) }
  for (const altitude of ['brief', 'explore']) {
    const body = text(U.renderShell({ model, stage: 'define', altitude, register: 'manager' }))
    assert.ok(!body.includes(T.top_one), altitude)
  }
})

test('none of the new sentences quotes the agent, accuses, or uses an engine word', () => {
  const lines = [MD.said_unrecorded_one, MD.said_unrecorded_many, V.unrecorded_one, V.unrecorded_many, T.top_one, T.top_many]
  for (const l of lines) {
    assert.equal(typeof l, 'string')
    assert.doesNotMatch(l, /["“”]/, 'a quote mark: ' + l)
    assert.doesNotMatch(l, /\b(session|event|ledger|receipt|lie|lied|false|fake)\b/i, l)
    assert.ok(!l.includes('contradicts what was said'), 'an absence-of-problem phrase: ' + l)
  }
})

/* ── it claims less, and only less ────────────────────────────────────── */

test('no state word, count, readiness answer, approval or primary action moves because work was not recorded', () => {
  const decided = (hq) => ({
    criteria: hq.criteria.map((c) => [c.n, c.state, c.word, c.line]),
    counts: hq.counts,
    headline: hq.headline,
    readiness: hq.readiness,
    decision: hq.decision,
    action: hq.inspector.action,
    inbox: { ...hq.inboxRow },
    runs: hq.packet.runs,
    proof: hq.packet.sections.find((s) => s.id === 'proof'),
  })
  for (const [shape, coverage] of Object.entries(COVERAGES)) {
    const change = aChange([SESSION_A])
    const withSteps = Model.deriveChangeModel(change, coverage, null, null, [recorded(SESSION_A)])
    const without = Model.deriveChangeModel(change, coverage, null, null, [unrecorded(SESSION_A)])
    const neverRead = Model.deriveChangeModel(change, coverage, null, null, [unknown(SESSION_A)])
    assert.deepEqual(decided(without), decided(withSteps), shape + ': 0 steps moved a decided field')
    assert.deepEqual(decided(neverRead), decided(withSteps), shape + ': an unknown count moved a decided field')
  }
})
