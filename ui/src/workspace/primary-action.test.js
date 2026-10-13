/* A3-F4 — the primary action sits where the question is, and its label
 * equals what it does.
 *
 * The defect this file fences: on a Change whose record holds a failure the
 * model marked the one primary as a LIVE door ("Look at what broke",
 * `enabled: true`, `door: <line>`), no component read `enabled` or `door`, and
 * the Inspector drew `Blocked` — a disabled button whose reason and enabler
 * were both the empty string. The old tests asserted the model and the text,
 * never the rendered `disabled`, so it passed both.
 *
 * Predeclared acceptance, from the card:
 *   1. on every state the Change model can reach (proven, needs_recheck,
 *      not_proven, recorded, in_progress, nothing accepted) the Inspector's one
 *      primary is either enabled and does what its label says, or disabled
 *      with a reason and an enabler that are BOTH non-empty;
 *   2. the primary is inside the first screen at 1512×920 and 1024×768 — that
 *      one needs a browser and is `scripts/testing/primary-visible.mjs`; here
 *      the structure it rests on (the action block is the Inspector's sticky
 *      footer) is held.
 *
 * Every fixture is synthetic. No string here came off a real machine.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-primary-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { createRoot } from 'react-dom/client';
  import { flushSync } from 'react-dom';
  import { ChangeShell, Inspector, Blocked, showsDecision, runsFromHere } from './change-workspace.jsx';
  import * as Model from './change-model.js';
  import { buildChangeView } from './change-view.js';
  import { HQ_COPY } from './hq-copy.js';
  export { React, createRoot, flushSync, ChangeShell, Model, buildChangeView, HQ_COPY, showsDecision, runsFromHere };
  export const renderShell = props => renderToStaticMarkup(React.createElement(ChangeShell, props));
  export const renderInspector = props => renderToStaticMarkup(React.createElement(Inspector, props));
  export const renderBlocked = props => renderToStaticMarkup(React.createElement(Blocked, props));
`
const result = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  stdin: { contents: ENTRY, resolveDir: here },
})
const file = join(temporary, 'primary.mjs')
await writeFile(file, result.outputFiles[0].text)
const M = await import(pathToFileURL(file).href)
const { HQ_COPY, Model } = M

/* The failing-check Change of `failure-shown.test.js`, trimmed to what the
 * Inspector reads. */
const CRITERIA = ['Two retry requests for the same delivery do not enqueue duplicate work.', 'A waiting delivery survives a restart.']
const CHANGE = {
  id: 'cccccccc-0000-4000-8000-00000000c001',
  project_id: 'cccccccc-0000-4000-8000-00000000p001',
  opened_at: '2026-01-02T09:00:00Z',
  request: { title: 'Retry a failed delivery', purpose: CRITERIA[0], requester: 'sam', source: 'person' },
  accepted_scope: {
    version: 1, criteria: CRITERIA, exclusions: [], allowed_scope: ['src/**'], constraints: [], check_profile: ['cargo-test'],
    content_sha256: '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
    acceptance: { actor: 'sam', accepted_at: '2026-01-02T09:06:00Z', rationale: 'this is what I want checked' },
    proposed_at: '2026-01-02T09:05:00Z',
  },
  implementation: { version: 1, bound_at: '2026-01-02T10:00:00Z', repository_identity: 'repo-synthetic-0001', base_revision: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0', snapshot_id: null, workspace_identity: 'workspace-synthetic-0001' },
  associations: [], examination: 'none_yet', decision: 'none_yet',
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
}
const ITEM = {
  link_id: 'cce_synthetic_0001', kind: 'check_run', evidence_id: 'evb_synthetic_0001', session_id: 'cli:check:2026-01-02T10:05:00Z',
  basis: 'named', basis_slug: 'basis_named', name: 'cargo test --test=duplicate_retry', observed: 'not_proven',
  bears: true, applies: true, applies_slug: 'inputs_applies', asserted_at: '2026-01-02T10:05:00Z',
  execution: 'completed', execution_slug: 'execution_completed', failure_line: 'left: "job_0002" right: "job_0003"',
  evidence_class: 'seeded', evidence_class_slug: 'evidence_class_seeded',
}
const COVERAGE = {
  schema_version: 1, change_id: CHANGE.id, scope_version: 1, link_rows: 1, executions: [],
  criteria: [
    { index: 1, text: CRITERIA[0], state: 'evidence', state_slug: 'coverage_evidence', items: [ITEM], removed: [] },
    { index: 2, text: CRITERIA[1], state: 'none', state_slug: 'coverage_no_test_ran', items: [], removed: [] },
  ],
}
const failing = () => Model.deriveChangeModel(CHANGE, COVERAGE, null, null, [])
const doc = (html) => new JSDOM('<!doctype html><body>' + html + '</body>').window.document
const squash = (s) => String(s || '').replace(/\s+/g, ' ').trim()

/** The action block's one primary, and what a person reads about it. */
function primaryOf(d) {
  const block = d.getElementById('ws-change-decision')
  assert.ok(block, 'the Inspector has no action block')
  const all = [...block.querySelectorAll('.ws-change-primary')]
  assert.equal(all.length, 1, `the action block has ${all.length} primaries, not one`)
  const b = all[0]
  const ids = String(b.getAttribute('aria-describedby') || '').split(/\s+/).filter(Boolean)
  const why = squash(ids.map((id) => (d.getElementById(id) || {}).textContent || '').join(' '))
  return { b, block, off: b.hasAttribute('disabled') || b.getAttribute('aria-disabled') === 'true', why }
}

test('A3-F4 · the not_proven primary renders ENABLED on the route a fresh install opens, and carries the line it opens', () => {
  const m = failing()
  assert.equal(m.state, 'not_proven')
  const d = doc(M.renderShell({
    model: { ...m.view, coverage: m.coverage, hq: m }, stage: 'verify', altitude: 'brief', register: 'manager',
    onGo() {}, onCopy() {}, onReload() {}, onAltitude() {}, onRegister() {},
  }))
  const { b, off } = primaryOf(d)
  assert.equal(squash(b.textContent), HQ_COPY.change.model.actions.look)
  assert.equal(off, false, `"${squash(b.textContent)}" is drawn disabled: ${b.outerHTML}`)
  assert.equal(b.getAttribute('data-door'), 'ws-cov-line-1', 'the door does not name the line that broke')
})

test('A3-F4 · pressing it does what it says: Brief moves to where the lines are and opens the line that broke', async () => {
  const m = failing()
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/?altitude=brief', pretendToBeVisual: true })
  const w = dom.window
  const names = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage', 'CSS']
  const held = names.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  for (const n of names) if (w[n] !== undefined) Object.defineProperty(globalThis, n, { value: n === 'window' ? w : w[n], configurable: true, writable: true })
  const went = []
  const root = M.createRoot(w.document.getElementById('root'))
  try {
    M.flushSync(() => root.render(M.React.createElement(M.ChangeShell, {
      model: { ...m.view, coverage: m.coverage, hq: m }, stage: 'define', altitude: 'brief', register: 'manager',
      onGo: (s) => went.push(['stage', s]), onCopy() {}, onReload() {}, onAltitude: (a) => went.push(['altitude', a]), onRegister() {},
    })))
    const { b } = primaryOf(w.document)
    b.dispatchEvent(new w.MouseEvent('click', { bubbles: true }))
    await new Promise((r) => setTimeout(r, 20))
    assert.deepEqual(went, [['altitude', 'explore'], ['stage', 'verify']], 'the door did not take Brief to the lines')
  } finally {
    root.unmount()
    for (const [n, desc] of held) {
      if (desc) Object.defineProperty(globalThis, n, desc)
      else delete globalThis[n]
    }
    w.close()
  }
})

test('A3-F4 (1) · on every state the model can reach, the one primary is on and does what it says, or off with a reason AND an enabler, both non-empty', () => {
  const base = failing()
  assert.equal(typeof Model.inspectorAction, 'function', 'the Inspector action is not derived by one named function')
  const blocker = base.view ? base.view.blocker : null
  for (const state of ['proven', 'needs_recheck', 'not_proven', 'recorded', 'in_progress', null]) {
    const action = Model.inspectorAction(state, state ? 1 : null, blocker || { reason: 'r', enabler: 'e' }, base.readiness)
    const hq = { ...base, state, inspector: { ...base.inspector, action } }
    const d = doc(M.renderInspector({ hq, cards: [] }))
    const { b, off, why, block } = primaryOf(d)
    const label = squash(b.textContent)
    if (off) {
      assert.ok(action.reason && action.enabler, `${state}: off with an empty half: ${JSON.stringify(action)}`)
      assert.ok(why.includes(squash(action.reason)), `${state}: "${label}" is off and its reason is not read: ${why}`)
      assert.ok(why.includes(squash(action.enabler)), `${state}: "${label}" is off and its enabler is not read: ${why}`)
    } else if (M.showsDecision(hq)) {
      assert.equal(label, hq.decision.approveLabel, `${state}: the Decision's primary`)
    } else if (M.runsFromHere(hq)) {
      assert.equal(label, action.label, `${state}: the run door's label is not the model's`)
      assert.equal(b.getAttribute('aria-haspopup'), 'dialog', `${state}: "${label}" does not open the run sheet`)
    } else {
      assert.equal(state, 'not_proven', `${state}: an enabled primary that is neither a decision, a run door nor the door to what broke`)
      assert.equal(label, action.label)
      assert.equal(b.getAttribute('data-door'), 'ws-cov-line-1', `${state}: "${label}" opens nothing`)
    }
    /* The action block is the Inspector's footer, and it says where its numbers come from. */
    assert.ok(block.classList.contains('ws-inspector-footer'), `${state}: the action block is not the sticky footer`)
    assert.ok(squash(block.textContent).includes(HQ_COPY.change.model.inspector.numbers), `${state}: the footer does not say where its numbers come from`)
  }
})

test('A3-F4 · Blocked refuses a blank half: a disabled control with no reason or no enabler cannot be drawn', () => {
  assert.throws(() => M.renderBlocked({ id: 'x', label: 'Look at what broke', pair: { reason: '', enabler: '' }, primary: true }))
  assert.throws(() => M.renderBlocked({ id: 'x', label: 'L', pair: { reason: 'A reason.', enabler: ' ' } }))
  assert.throws(() => M.renderBlocked({ id: 'x', label: 'L', pair: { reason: '', enabler: 'Run it.' } }))
  assert.match(M.renderBlocked({ id: 'x', label: 'L', pair: { reason: 'A reason.', enabler: 'Run it.' } }), /disabled/)
})

test('A3-F4 · the footer sticks to the bottom of the screen, and a focused item is never hidden under it (WCAG 2.4.11)', async () => {
  const css = await readFile(join(here, 'workspace.css'), 'utf8')
  const rule = css.match(/\.ws-inspector-footer\s*\{([^}]*)\}/)
  assert.ok(rule, 'no .ws-inspector-footer rule')
  assert.match(rule[1], /position:\s*sticky/)
  assert.match(rule[1], /bottom:\s*0/)
  assert.match(css, /scroll-padding-bottom:\s*var\(--inspector-footer-h/, 'nothing keeps a focused item clear of the footer')
})
