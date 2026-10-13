/* T-6-4 · Build 2 and 6 — roles, names, live regions, and a ring wherever focus can land.
 *
 * Rendered to static markup over synthetic data (the Trace fixture; the
 * a11y fixture's Change) and read in jsdom:
 *  - the dock is a named tablist of tabs, each controlling a named tabpanel,
 *    every tab reachable by Tab, in the order they read;
 *  - the Inspector's four blocks are regions, each named by its own heading,
 *    in reading order, and nothing on either screen jumps the Tab order;
 *  - the altitude and wording switches are named groups of `aria-pressed`
 *    buttons; the top bar's note and the thread bar are polite live regions;
 *  - the scrubber says where it is in Plain words;
 *  - every element that takes focus from script (a canvas card, an Outline
 *    row, an off control's reason) has a ring drawn on `:focus`, in the
 *    focus colour, and nothing in the sheet turns a ring off.
 */
import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'
import { HQ_COPY } from './hq-copy.js'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-a11y-roles-'))
after(() => rm(temporary, { recursive: true, force: true }))

const result = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  stdin: {
    resolveDir: here,
    contents: `
      import React from 'react';
      import { renderToStaticMarkup } from 'react-dom/server';
      import { Workspace } from './shell.jsx';
      import { adaptSession } from './adapter.js';
      import { TRACE_DOCS } from './trace-fixture.js';
      import { discrepancyAnchor } from './trace-view.js';
      import { ChangeShell } from './change-workspace.jsx';
      import { buildChangeView } from './change-view.js';
      import { buildCoverageView, evidenceEntries } from './coverage-view.js';
      import { deriveChangeModel } from './change-model.js';
      import { A11Y_CHANGE, A11Y_COVERAGE } from './a11y-fixture.js';
      export const html = (el) => renderToStaticMarkup(el);
      export const renderShell = (props) => renderToStaticMarkup(React.createElement(ChangeShell, props));
      export { Workspace, adaptSession, TRACE_DOCS, discrepancyAnchor, buildChangeView, buildCoverageView, evidenceEntries, deriveChangeModel, A11Y_CHANGE, A11Y_COVERAGE };
    `,
  },
})
const file = join(temporary, 'roles.mjs')
await writeFile(file, result.outputFiles[0].text)
const M = await import(pathToFileURL(file).href)

const SH = HQ_COPY.shell
const AY = HQ_COPY.a11y
const doc = (html) => new JSDOM('<!doctype html><body>' + html + '</body>').window.document

function workspace(over) {
  const s = M.adaptSession(M.TRACE_DOCS)
  const w = new M.Workspace({ index: { projects: [], totals: {}, kind: 'index', contract_version: 1 } })
  w.state = {
    ...w.state,
    altitude: 'explore', session: s.key, corpus: { order: [s.key], sessions: { [s.key]: s } }, dataStatus: 'ready',
    cw: 40000, ch: 40000, view: { x: 0, y: 0, z: 1 },
    panels: { left: false, right: true, bottom: true, minimap: true },
    ...(over || {}),
  }
  return { w, s }
}
const explore = (over) => { const { w } = workspace(over); return doc(M.html(w.render())) }

function changeBrief() {
  const model = M.buildChangeView({ change: M.A11Y_CHANGE, proposals: null, work: [], evidence: M.evidenceEntries(M.A11Y_COVERAGE) })
  model.coverage = M.buildCoverageView(M.A11Y_COVERAGE)
  model.hq = M.deriveChangeModel(M.A11Y_CHANGE, M.A11Y_COVERAGE, null, null, [])
  return doc(M.renderShell({ model, stage: 'define', altitude: 'brief', register: 'manager', onAltitude() {}, onRegister() {}, onReload() {} }))
}

test('6 · the dock is a named tablist: each tab names the panel it controls, the panel is labelled by its tab, and every tab is reachable by Tab in the order it reads', () => {
  const d = explore()
  const dock = d.querySelector(`section[aria-label="${AY.dock_label}"]`)
  assert.ok(dock, 'the dock is a named section')
  const list = dock.querySelector('[role="tablist"]')
  assert.equal(list.getAttribute('aria-label'), AY.dock_tabs_label)
  const tabs = [...list.querySelectorAll('[role="tab"]')]
  assert.ok(tabs.length >= 3, `${tabs.length} tabs`)
  const selected = tabs.filter((t) => t.getAttribute('aria-selected') === 'true')
  assert.equal(selected.length, 1)
  for (const t of tabs) {
    assert.equal(t.tagName, 'BUTTON')
    assert.notEqual(t.getAttribute('tabindex'), '-1', `${t.textContent} is out of the Tab order`)
    assert.match(t.getAttribute('aria-controls') || '', /^ws-dock-pane-/)
  }
  const panel = dock.querySelector('[role="tabpanel"]')
  assert.equal(panel.getAttribute('id'), selected[0].getAttribute('aria-controls'))
  assert.equal(panel.getAttribute('aria-labelledby'), selected[0].getAttribute('id'))
  /* reading order: the DOM order is the order the tabs are drawn */
  assert.deepEqual(tabs.map((t) => t.getAttribute('id')), [...d.querySelectorAll('[role="tab"][id^="ws-dock-tab-"]')].map((t) => t.getAttribute('id')))
})

test('6 · the Inspector\'s four blocks are regions named by their own headings, in reading order; the whole is named; nothing on either screen jumps the Tab order', () => {
  const d = changeBrief()
  const insp = d.querySelector('.ws-change-inspector')
  assert.equal(insp.getAttribute('role'), 'region')
  assert.equal(insp.getAttribute('aria-label'), AY.inspector_label)
  const blocks = [...insp.querySelectorAll(':scope > .ws-inspector-block')]
  assert.equal(blocks.length, 4)
  const I = []
  for (const b of blocks) {
    assert.equal(b.getAttribute('role'), 'region')
    const h = d.getElementById(b.getAttribute('aria-labelledby'))
    assert.ok(h && h.tagName === 'H3' && b.contains(h), 'named by its own heading')
    I.push(h.textContent)
  }
  assert.equal(new Set(I).size, 4, 'four different names')
  for (const page of [d, explore(), explore({ altitude: 'workbench' })]) {
    const jumps = [...page.querySelectorAll('[tabindex]')].filter((e) => Number(e.getAttribute('tabindex')) > 0)
    assert.deepEqual(jumps.map((e) => e.outerHTML.slice(0, 80)), [], 'a positive tabindex breaks reading order')
  }
})

test('6 · the altitude and wording switches are named groups of aria-pressed buttons; the top bar note and the thread bar are polite live regions; the scrubber speaks Plain words', () => {
  const { w, s } = workspace({ sel: null })
  w.state = { ...w.state, sel: M.discrepancyAnchor(s) }
  const d = doc(M.html(w.render()))
  for (const [label, n] of [[SH.topbar.altitude_label, 3], [SH.topbar.register_label, 2]]) {
    const g = d.querySelector(`[role="group"][aria-label="${label}"]`)
    assert.ok(g, label)
    const b = [...g.querySelectorAll('button')]
    assert.equal(b.length, n)
    assert.ok(b.every((x) => ['true', 'false'].includes(x.getAttribute('aria-pressed'))))
    assert.equal(b.filter((x) => x.getAttribute('aria-pressed') === 'true').length, 1)
  }
  const note = d.querySelector('.ws-shell-note')
  assert.equal(note.getAttribute('role'), 'status')
  assert.equal(note.getAttribute('aria-live'), 'polite')
  const bar = d.querySelector('.ws-trace-thread-bar')
  assert.ok(bar, 'a thread is lit')
  assert.equal(bar.getAttribute('role'), 'status')
  assert.equal(bar.getAttribute('aria-live'), 'polite')
  const range = d.querySelector('[role="toolbar"] input[type="range"]')
  assert.match(range.getAttribute('aria-valuetext'), /steps?/)
  assert.doesNotMatch(range.getAttribute('aria-valuetext'), /\bseq\b/)
  const at = doc(M.html(workspace({ replay: { idx: 6, playing: false } }).w.render())).querySelector('[role="toolbar"] input[type="range"]')
  assert.match(at.getAttribute('aria-valuetext'), /step 7\b.*7 of 12/, 'where the scrubber is, as "step 7 … 7 of 12"')
})

/* ── 2 · a ring wherever focus can land ─────────────────────────────── */

const CSS = readFileSync(join(here, 'workspace.css'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
const ringRules = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter((m) => /outline:\s*var\(--ws-focus-width\) solid var\(--ws-focus\)/.test(m[2])).map((m) => m[1].split(',').map((x) => x.trim())).flat()

test('2 · every element that takes focus from script — a canvas card, an Outline row, an off control\'s reason — has a ring drawn on :focus, not only on :focus-visible', () => {
  for (const sel of ['.ws-root [role="button"][data-id]:focus', '.ws-trace-outline-item:focus', '.ws-change-blocked-why:focus']) {
    assert.ok(ringRules.includes(sel), `${sel} draws no ring`)
  }
  /* and those are exactly the elements the product focuses from script */
  const shell = readFileSync(join(here, 'shell.jsx'), 'utf8')
  assert.match(shell, /cardElement\(id\) \{/)
  assert.match(readFileSync(join(here, 'blocked.jsx'), 'utf8'), /className="ws-change-blocked-why" id=\{id\} tabIndex=\{-1\}/)
})

test('2 · a ring that would be clipped is drawn inside: segmented buttons and tabs', () => {
  for (const sel of ['.ws-shell-seg-btn:focus-visible', '.ws-root [role="tab"]:focus-visible', '.ws-change [role="tab"]:focus-visible']) assert.ok(ringRules.includes(sel), sel)
  const m = /\.ws-shell-seg-btn:focus-visible \{([^}]*)\}/.exec(CSS)
  assert.match(m[1], /outline-offset:\s*calc\(0px - var\(--ws-focus-width\)\)/)
  assert.match(CSS, /\.ws-shell-seg \{[^}]*overflow: hidden/, 'the group still clips, which is why the ring is inside')
})

test('2 · the Change screen\'s shell rings every focusable kind it holds, and nothing turns a ring off', () => {
  for (const sel of ['.ws-shell button:focus-visible', '.ws-shell a:focus-visible', '.ws-shell summary:focus-visible', '.ws-shell [tabindex]:focus-visible', '.ws-shell input:focus-visible']) assert.ok(ringRules.includes(sel), sel)
  assert.doesNotMatch(CSS, /outline:\s*(none|0)\b/)
})

test('6 · a pressed state is never told by a ground alone: the pressed altitude, the Story toggle and the Outline item each carry a marker', () => {
  for (const sel of ['.ws-shell-seg-btn[aria-pressed="true"]', '.ws-shell-story-toggle[aria-pressed="true"]', '.ws-shell-side-item[aria-pressed="true"]']) {
    const m = new RegExp(sel.replace(/[.[\]"=]/g, (c) => '\\' + c) + ' \\{([^}]*)\\}').exec(CSS)
    assert.ok(m, sel)
    assert.match(m[1], /box-shadow:\s*inset [^;]*var\(--ws-text(-2)?\)/, `${sel} has no marker`)
  }
})
