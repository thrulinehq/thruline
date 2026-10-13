/* N13-DRAG-257 — WCAG 2.2 SC 2.5.7 Dragging Movements: every drag has a
 * single-pointer (click) alternative.
 *
 * 1. A census over every workspace .jsx file: each `onMouseDown` /
 *    `onPointerDown` is either a drag listed in `DRAG_ALTERNATIVES` (with the
 *    single-pointer alternative it names) or a press listed in
 *    `NOT_A_DRAG` (a backdrop that closes on a press, a control that stops a
 *    drag from starting under it). An unlisted one fails.
 * 2. Each named alternative is operated by a click in the real workspace,
 *    mounted in jsdom over the bundled demo record, and moves what the drag
 *    moves: the overview press moves the view, the dock step buttons resize
 *    the dock by the arrow key's step within the same clamp, and the
 *    inspector's move buttons move the selected card by the arrow key's nudge.
 *
 * Note (Rule 7, surfaced): the report's line 1142 says "a click or key
 * alternative"; SC 2.5.7 requires a SINGLE-POINTER one. This test holds the
 * SC's wording; the arrow keys stay.
 */
import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { parse } from '@babel/parser'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'
import * as CENSUS from './control-census.js'
import { HQ_COPY } from './hq-copy.js'
import { CLAMP } from './tokens.js'

const here = dirname(fileURLToPath(import.meta.url))
const JSX = readdirSync(here).filter((f) => f.endsWith('.jsx')).sort()

function walk(node, visit) {
  if (!node || typeof node.type !== 'string') return
  visit(node)
  for (const k of Object.keys(node)) {
    if (k === 'loc' || k === 'start' || k === 'end' || k === 'extra') continue
    const v = node[k]
    if (Array.isArray(v)) for (const c of v) walk(c, visit)
    else if (v && typeof v.type === 'string') walk(v, visit)
  }
}

/** Every press that could start a drag: `file · handler source`, whitespace collapsed. */
function presses() {
  const out = []
  for (const f of JSX) {
    const src = readFileSync(join(here, f), 'utf8')
    walk(parse(src, { sourceType: 'module', plugins: ['jsx'] }).program, (n) => {
      if (n.type === 'JSXAttribute' && n.name && (n.name.name === 'onMouseDown' || n.name.name === 'onPointerDown') && n.value && n.value.type === 'JSXExpressionContainer') {
        out.push(`${f} · ${src.slice(n.value.expression.start, n.value.expression.end).replace(/\s+/g, ' ')}`)
      }
    })
  }
  return out
}

test('census: every drag start in the workspace is listed with its single-pointer alternative, and every other press is named as not a drag', () => {
  const { DRAG_ALTERNATIVES, NOT_A_DRAG } = CENSUS
  assert.ok(Array.isArray(DRAG_ALTERNATIVES), 'control-census.js exports DRAG_ALTERNATIVES')
  assert.ok(Array.isArray(NOT_A_DRAG), 'control-census.js exports NOT_A_DRAG')
  const known = new Set([...DRAG_ALTERNATIVES.map((d) => d.press), ...NOT_A_DRAG.map((d) => d.press)])
  const unlisted = presses().filter((p) => !known.has(p))
  assert.deepEqual(unlisted, [], 'a press that may start a drag and names no single-pointer alternative')
  for (const d of DRAG_ALTERNATIVES) {
    assert.ok(d.drag && d.alternative && d.test, `${d.press}: names the drag, its single-pointer alternative and the test that operates it`)
  }
  for (const d of NOT_A_DRAG) assert.ok(d.why, `${d.press}: says why it is not a drag`)
  /* stale rows are errors too: a row whose press is gone describes nothing */
  const found = new Set(presses())
  for (const p of known) assert.ok(found.has(p), `listed but not in the source: ${p}`)
  assert.deepEqual(DRAG_ALTERNATIVES.map((d) => d.drag).sort(), ['move a card', 'pan the canvas', 'pan with the overview', 'resize a dock'])
})

/* ── operate each alternative with a click ─────────────────────────────── */

const temporary = await mkdtemp(join(tmpdir(), 'thruline-a11y-drag-'))
after(() => rm(temporary, { recursive: true, force: true }))
const result = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  stdin: {
    resolveDir: here,
    contents: `
      import React from 'react';
      import { createRoot } from 'react-dom/client';
      import { flushSync } from 'react-dom';
      import Workspace from './shell.jsx';
      import { a11yFetch } from './a11y-fixture.js';
      export { React, createRoot, flushSync, Workspace, a11yFetch };
    `,
  },
})
const file = join(temporary, 'drag.mjs')
await writeFile(file, result.outputFiles[0].text)
const L = await import(pathToFileURL(file).href)

const settle = () => new Promise((r) => setTimeout(r, 10))
const GLOBALS = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage', 'CSS', 'fetch']

async function atWorkbench(run) {
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/?altitude=workbench', pretendToBeVisual: true })
  const w = dom.window
  /* A window tall enough that the short-window cap on the bottom dock
   * (shell.jsx `sizes()`, below 800px) is not what stops the clicks. */
  Object.defineProperty(w, 'innerHeight', { value: 1000, configurable: true })
  Object.defineProperty(w, 'innerWidth', { value: 1512, configurable: true })
  const held = GLOBALS.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of GLOBALS) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  set('fetch', L.a11yFetch)
  const root = L.createRoot(w.document.getElementById('root'))
  try {
    const ref = L.React.createRef()
    L.flushSync(() => root.render(L.React.createElement(L.Workspace, { ref, index: { projects: [], totals: {}, kind: 'index', contract_version: 1 } })))
    for (let i = 0; i < 60; i += 1) await settle()
    return await run({ w, d: w.document, ws: ref.current })
  } finally {
    root.unmount()
    for (const [n, desc] of held) { if (desc) Object.defineProperty(globalThis, n, desc); else delete globalThis[n] }
    w.close()
  }
}
const click = async (el) => { el.click(); for (let i = 0; i < 4; i += 1) await settle() }

test('pan: one press on the overview moves the view there — no drag', async () => {
  await atWorkbench(async ({ ws }) => {
    assert.equal(ws.state.dataStatus, 'ready')
    assert.ok(ws.vals && ws.vals.mm, 'the overview is drawn at Workbench')
    const before = { ...ws.state.view }
    ws.graphOn().onMinimapDown({ currentTarget: { getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 120 }) }, clientX: 190, clientY: 110, stopPropagation() {} })
    ws.handleUp()
    for (let i = 0; i < 4; i += 1) await settle()
    assert.notDeepEqual(ws.state.view, before, 'a single press on the overview moved the view')
    assert.equal(ws.drag, null, 'nothing is left dragging after the press')
  })
})

test('resize a dock: each separator has two buttons that step it by the arrow key\'s step, within the same clamp', async () => {
  const R = HQ_COPY.a11y.resize
  assert.ok(R, 'HQ_COPY.a11y.resize holds the step buttons\' names')
  await atWorkbench(async ({ d, ws }) => {
    for (const [which, grow, shrink] of [['left', R.left_wider, R.left_narrower], ['right', R.right_wider, R.right_narrower], ['bottom', R.bottom_taller, R.bottom_shorter]]) {
      const btn = (label) => [...d.querySelectorAll('button')].find((b) => b.getAttribute('aria-label') === label)
      assert.ok(btn(grow) && btn(shrink), `${which}: both step buttons are drawn`)
      const start = ws.sizes()[which]
      await click(btn(shrink))
      const lo = ws.sizes()[which]
      assert.equal(lo, Math.max(CLAMP[which][0], start - CLAMP.keyStep), `${which}: one click is one arrow-key step smaller`)
      await click(btn(grow))
      await click(btn(grow))
      assert.equal(ws.sizes()[which], Math.min(CLAMP[which][1], lo + 2 * CLAMP.keyStep), `${which}: two clicks are two steps larger`)
      for (let i = 0; i < 30; i += 1) await click(btn(grow))
      assert.equal(ws.sizes()[which], CLAMP[which][1], `${which}: the clicks stop at the drag's own clamp`)
    }
  })
})

test('move a card: the inspector moves the selected card by the arrow key\'s nudge, one click per step', async () => {
  const M = HQ_COPY.a11y.move_card
  assert.ok(M, 'HQ_COPY.a11y.move_card holds the move buttons\' names')
  await atWorkbench(async ({ d, ws }) => {
    const card = d.querySelector('[role="button"][data-id]')
    assert.ok(card, 'a card is drawn')
    const id = card.getAttribute('data-id')
    ws.select(id)
    ws.setPanel('right', true)
    ws.setState({ evidenceTab: 'trace' })
    for (let i = 0; i < 6; i += 1) await settle()
    const n = ws.nodeById(id)
    const g0 = ws.geo(n)
    const btn = (label) => [...d.querySelectorAll('[aria-label="Inspector"] button')].find((b) => b.getAttribute('aria-label') === label)
    assert.ok(d.querySelector('[aria-label="Inspector"]'), `the inspector is drawn (panels.right ${ws.state.panels.right}, rep ${ws.state.rep}, doc ${ws.state.doc}, sel ${ws.state.sel})`)
    for (const k of ['left', 'right', 'up', 'down']) assert.ok(btn(M[k]), `the inspector draws "${M[k]}"`)
    await click(btn(M.right))
    await click(btn(M.down))
    await click(btn(M.down))
    const g1 = ws.geo(ws.nodeById(id))
    assert.equal(g1.x, g0.x + CLAMP.nodeStep)
    assert.equal(g1.y, g0.y + 2 * CLAMP.nodeStep)
    await click(btn(M.left))
    await click(btn(M.up))
    const g2 = ws.geo(ws.nodeById(id))
    assert.equal(g2.x, g0.x)
    assert.equal(g2.y, g0.y + CLAMP.nodeStep)
  })
})
