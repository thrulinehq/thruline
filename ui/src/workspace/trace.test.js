/* P6-3 — the Trace, rendered (design record §3.3, §4, §5, §6).
 *
 * The workspace is bundled and rendered the way shell-chrome.test.js renders
 * it: to static markup over a synthetic session (`trace-fixture.js`, adapted
 * by the real adapter), and the shell's key handler in jsdom with real key
 * presses. Every fixture is synthetic.
 *
 * One test per acceptance line of the card:
 *  - Plain never shows seq, clm_, chk_, a full hash or a session id, on any
 *    card of any kind, on the canvas or in the Outline — and Technical does;
 *  - the legend is always drawn at Explore and Workbench: three line kinds, a fourth
 *    only when the record carries one this view cannot classify;
 *  - selecting a card lights its thread (amber, the one glow, a position in
 *    words) and dims the rest; the thread's lines draw once, cause first;
 *  - cards say where things ran, only from typed fields, every gap in words;
 *    the band for an earlier version; hollow for no result; the agent line;
 *  - the Outline twin: same cards, same thread, lane and line-kind words,
 *    keyboard, shared selection, reachable from the sidebar;
 *  - Focus thread, Walk me through it, the scrubber, and J / K / F / Esc.
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
import { parse } from '@babel/parser'
import { census, missingNames, renderWorkspaceHtml } from './control-census.js'
import { LANE_BY_KIND } from './trace-view.js'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-trace-'))
after(() => rm(temporary, { recursive: true, force: true }))

const COMMON = {
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
}
async function load(name, contents) {
  const result = await build({ ...COMMON, stdin: { contents, resolveDir: here } })
  const file = join(temporary, name)
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}

const M = await load('trace.mjs', `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { createRoot } from 'react-dom/client';
  import { flushSync } from 'react-dom';
  import { Workspace } from './shell.jsx';
  import { ShellTopBar } from './shell-chrome.jsx';
  import { TraceOutline } from './trace-outline.jsx';
  import { adaptSession } from './adapter.js';
  import { TRACE_DOCS } from './trace-fixture.js';
  import { outlineRows, threadOf } from './trace-view.js';
  export const html = (el) => renderToStaticMarkup(el);
  export { React, createRoot, flushSync, Workspace, ShellTopBar, TraceOutline, adaptSession, TRACE_DOCS, outlineRows, threadOf };
`)

const T = HQ_COPY.trace
const SH = HQ_COPY.shell
const read = (f) => readFileSync(join(here, f), 'utf8')
const doc = (html) => new JSDOM('<!doctype html><body>' + html + '</body>').window.document
const BANNED = /\bseq\b|clm_|chk_|[0-9a-f]{40}|session id/i

const session = () => M.adaptSession(M.TRACE_DOCS)
const S = session()
const idOf = (t) => S.nodes.find((n) => n.objType === t).id
const CLAIM = idOf('claim')
const [CHK1, CHK2] = S.nodes.filter((n) => n.objType === 'check').map((n) => n.id)

/** A workspace instance over one session, with state set directly. */
function workspace(over, s = session()) {
  const w = new M.Workspace({ index: { projects: [], totals: {}, kind: 'index', contract_version: 1 } })
  w.setState = (u, then) => {
    w.state = { ...w.state, ...(typeof u === 'function' ? u(w.state) : u) }
    if (then) then()
  }
  w.toast = (t) => (w.toasted = t)
  w.log = () => {}
  w.state = {
    ...w.state,
    altitude: 'explore',
    session: s.key,
    corpus: { order: [s.key], sessions: { [s.key]: s } },
    dataStatus: 'ready',
    /* a viewport big enough that the window draws every card */
    cw: 40000,
    ch: 40000,
    view: { x: 0, y: 0, z: 1 },
    focusMode: 'everything',
    ...(over || {}),
  }
  return w
}
const render = (over, s) => doc(M.html(workspace(over, s).render()))
/** Everything the canvas says: its text, and every label and title inside it. */
function canvasWords(d) {
  const c = d.querySelector('[role="application"]')
  assert.ok(c, 'no canvas')
  const attrs = [...c.querySelectorAll('[aria-label], [title]')].map((e) => (e.getAttribute('aria-label') || '') + ' ' + (e.getAttribute('title') || ''))
  return c.textContent + ' ' + attrs.join(' ')
}
const cardOf = (d, id) => d.querySelector(`[role="application"] [role="button"][data-id="${id}"]`)

/* ── Plain / Technical ─────────────────────────────────────────────────── */

test('Plain never shows seq, clm_, chk_, a full hash or a session id — on any card of any kind, lit or not, on the canvas or in the Outline', () => {
  const expanded = { ev3: true }
  for (const over of [{ expanded }, { expanded, sel: CLAIM }, { expanded, sel: CHK2 }, { expanded, replay: { idx: 6, playing: false } }]) {
    const d = render({ register: 'manager', ...over })
    /* every node kind is on the canvas */
    for (const n of S.nodes) if (!n.group) assert.ok(cardOf(d, n.id), `card ${n.id} (${n.objType}/${n.kind}) is not drawn`)
    const words = canvasWords(d)
    const hit = BANNED.exec(words)
    assert.equal(hit, null, `Plain shows "${hit && words.slice(Math.max(0, hit.index - 60), hit.index + 60)}"`)
  }
  /* the fixture holds every event kind the adapter knows and every object
   * type but the ending, which only exists when no hand-back was recorded */
  const kinds = new Set(S.nodes.map((n) => n.kind).filter(Boolean))
  for (const k of Object.keys(LANE_BY_KIND)) assert.ok(kinds.has(k), `the fixture has no ${k}`)
  const types = new Set(S.nodes.map((n) => n.objType))
  for (const t of ['event', 'ask', 'claim', 'finding', 'promise', 'check', 'receipt']) assert.ok(types.has(t), `the fixture has no ${t}`)
  const noStop = M.adaptSession({ ...M.TRACE_DOCS, explain: { ...M.TRACE_DOCS.explain, what_happened: M.TRACE_DOCS.explain.what_happened.filter((e) => e.kind !== 'stop'), why_it_stopped: { kind: 'no_stop_recorded', summary: 'No stop was recorded.' } } })
  const ending = noStop.nodes.find((n) => n.objType === 'ending')
  assert.ok(ending, 'the ending card')
  const e = render({ register: 'manager', expanded }, noStop)
  assert.ok(cardOf(e, ending.id))
  assert.doesNotMatch(canvasWords(e), BANNED)

  const o = render({ register: 'manager', expanded, sel: CLAIM, outline: true })
  const list = o.querySelector('.ws-trace-outline')
  assert.ok(list)
  assert.doesNotMatch(list.textContent + [...list.querySelectorAll('[title], [aria-label]')].map((x) => x.getAttribute('title') + x.getAttribute('aria-label')).join(' '), BANNED)
})

test('Technical shows what Plain hides: seq, ids and the full version hash come back', () => {
  const words = canvasWords(render({ register: 'operator', expanded: { ev3: true }, sel: CLAIM }))
  assert.match(words, /\bseq \d+/)
  assert.match(words, /clm_SYNTH/)
  assert.match(words, /chk_SYNTH/)
  assert.match(words, /tree 1{40}/)
})

test('Plain says "step N" where Technical says "seq N"', () => {
  const plain = cardOf(render({ register: 'manager' }), 'ev9').textContent
  const tech = cardOf(render({ register: 'operator' }), 'ev9').textContent
  assert.match(plain, /step 9/)
  assert.match(tech, /seq 9/)
})

/* ── the legend ────────────────────────────────────────────────────────── */

test('the legend is always drawn at Explore and Workbench — three line kinds with their own strokes — and a fourth only when the record carries a kind this view cannot classify', () => {
  for (const altitude of ['explore', 'workbench']) {
    const d = render({ altitude })
    const legend = d.querySelector('.ws-trace-legend')
    assert.ok(legend, altitude)
    assert.equal(legend.closest('details'), null, 'never behind a toggle')
    const items = [...legend.querySelectorAll('.ws-trace-legend-item')]
    assert.deepEqual(items.map((i) => i.getAttribute('data-edge')), ['order', 'reference', 'inferred'])
    assert.deepEqual(items.map((i) => i.textContent), [T.edges.order, T.edges.reference, T.edges.inferred])
    const dashes = items.map((i) => i.querySelector('line').getAttribute('stroke-dasharray') || 'solid')
    assert.equal(new Set(dashes).size, 3, dashes.join(' | '))
    for (const i of items) assert.ok(i.getAttribute('title'), 'each entry says what it means')
  }
  const s = session()
  const a = s.nodes.find((n) => n.objType === 'promise')
  const b = s.nodes.find((n) => n.objType === 'claim')
  a.pinsR = a.pinsR.concat([{ id: 'x', label: 'x', t: 'ref' }])
  b.pinsL = b.pinsL.concat([{ id: 'x', label: 'x', t: 'ref' }])
  s.wires = s.wires.concat([{ id: 'w_unknown', from: [a.id, 'x'], to: [b.id, 'x'], kind: 'xref', c: 'grey', label: 'unknown' }])
  const d = render({}, s)
  const items = [...d.querySelectorAll('.ws-trace-legend-item')].map((i) => i.getAttribute('data-edge'))
  assert.deepEqual(items, ['order', 'reference', 'inferred', 'unclassified'])
  assert.equal(d.querySelector('g[data-edge="unclassified"]') != null, true, 'the line is drawn as itself')
})

test('every line on the picture is drawn in its kind\'s stroke', () => {
  const d = render({})
  const lines = [...d.querySelectorAll('g[data-edge]')]
  assert.ok(lines.length > 5)
  for (const g of lines) {
    const style = g.querySelector('path:not(mask path)').getAttribute('style')
    const kind = g.getAttribute('data-edge')
    if (kind === 'order') assert.match(style, /stroke-dasharray:\s*none/, style)
    if (kind === 'reference') assert.match(style, /stroke-dasharray:\s*7 4/, style)
  }
})

/* ── the lit thread ────────────────────────────────────────────────────── */

test('selecting a card lights its thread — amber, the one glow, a position in words — and dims every other card', () => {
  const t = M.threadOf(S, CLAIM)
  const d = render({ sel: CLAIM, expanded: { ev3: true } })
  const lit = [...d.querySelectorAll('[role="application"] [data-lit="yes"][role="button"]')].map((e) => e.getAttribute('data-id'))
  assert.deepEqual(lit.sort(), t.ids.slice().sort())
  for (const id of t.ids) {
    const card = cardOf(d, id)
    const style = card.getAttribute('style')
    assert.match(style, /color-mix\(in srgb, #F5A524 45%, transparent\)/, `${id} does not glow`)
    assert.match(style, /opacity:\s*1\b/)
    assert.match(card.textContent, new RegExp(`\\d of ${t.ids.length} on the thread`))
  }
  for (const n of S.nodes.filter((x) => !x.group && !t.ids.includes(x.id))) {
    assert.match(cardOf(d, n.id).getAttribute('style'), /opacity:\s*0\.22/, `${n.id} is not dimmed`)
  }
  const bar = d.querySelector('.ws-trace-thread-bar')
  assert.ok(bar.textContent.includes('Thread: 4 steps'))
  assert.ok(bar.textContent.includes('Nothing in the record links a request to this.'))
  assert.ok([...bar.querySelectorAll('button')].some((b) => b.textContent === T.thread.show_all))
  assert.ok([...d.querySelectorAll('.ws-trace-legend-item')].some((i) => i.textContent === T.edges.thread))
})

test('the thread\'s lines are amber and draw once, from the cause end, through a mask that keeps their kind\'s pattern', () => {
  const t = M.threadOf(S, CLAIM)
  const d = render({ sel: CLAIM, expanded: { ev3: true } })
  const lit = [...d.querySelectorAll('g[data-lit="yes"]')]
  assert.equal(lit.length, t.wires.length)
  for (const g of lit) {
    const shown = g.querySelector(':scope > path')
    const mask = g.querySelector('mask path.ws-trace-draw')
    assert.ok(mask, 'no drawing mask')
    assert.equal(mask.getAttribute('d'), shown.getAttribute('d'), 'the mask draws along the same path, from its start')
    assert.equal(mask.getAttribute('pathLength'), '1')
    assert.match(shown.getAttribute('style'), /stroke:\s*#F5A524/)
  }
  /* no card is lit and nothing dims with nothing selected */
  const none = render({})
  assert.equal(none.querySelector('[data-lit="yes"]'), null)
  assert.equal(none.querySelector('.ws-trace-thread-bar'), null)
})

test('motion: the thread draws once in 320ms ease-out from the tokens, rests whole, and with reduced motion does not move at all', () => {
  const css = read('workspace.css')
  assert.match(css, /\.ws-trace-draw \{ stroke-dasharray: 1; stroke-dashoffset: 0; \}/, 'the resting state is the whole line')
  assert.match(css, /@keyframes ws-trace-draw \{ from \{ stroke-dashoffset: 1; \} to \{ stroke-dashoffset: 0; \} \}/)
  const block = css.slice(css.indexOf('@keyframes ws-trace-draw'))
  assert.match(block, /@media \(prefers-reduced-motion: no-preference\) \{\n  \.ws-trace-draw \{ animation: ws-trace-draw var\(--ws-motion-slow\) var\(--ws-ease\) 1; \}/)
})

/* ── where things ran ──────────────────────────────────────────────────── */

test('cards say where a run ran — from typed fields only, every gap named — with the earlier-version band and the hollow card', () => {
  const d = render({ register: 'manager', expanded: { ev3: true } })
  const c1 = cardOf(d, CHK1).textContent
  assert.ok(c1.includes(T.run.earlier), 'the band')
  assert.ok(c1.includes('Exit code1'))
  assert.ok(c1.includes('Counted3 passed · 1 failed · 0 skipped · 4 in total'))
  assert.ok(c1.includes('Version1111111'))
  assert.ok(c1.includes('Not recorded: command, how long, where it ran'))
  const c2 = cardOf(d, CHK2)
  assert.equal(c2.getAttribute('data-hollow'), 'yes')
  assert.match(c2.getAttribute('style'), /border:\s*1px dashed/)
  assert.ok(c2.textContent.includes(T.run.no_result))
  assert.ok(!/exit undefined/.test(c2.textContent), 'no "exit undefined"')
  assert.ok(!c2.textContent.includes(T.run.earlier), 'it ran on the last version')
  assert.ok(cardOf(d, 'ev7').textContent.includes('Commandnpm test'))
  for (const id of [CLAIM, idOf('promise'), 'ev11']) assert.ok(cardOf(d, id).textContent.includes(T.agent_reported), id)
  for (const id of ['ev2', 'ev5', idOf('finding')]) assert.ok(!cardOf(d, id).textContent.includes(T.agent_reported), id)
})

/* ── the Outline twin ──────────────────────────────────────────────────── */

test('the Outline is a working sidebar item at Explore, and lists the same cards and the same thread with lane and line-kind words', () => {
  const d = render({})
  const item = [...d.querySelectorAll('nav.ws-shell-side button')].find((b) => b.textContent === SH.sidebar.outline)
  assert.ok(item, 'no Outline item')
  assert.equal(item.getAttribute('aria-disabled'), null)
  assert.equal(item.getAttribute('aria-pressed'), 'false')

  const o = render({ outline: true, sel: CLAIM, expanded: { ev3: true } })
  assert.equal(o.querySelector('[role="application"]'), null, 'the Outline replaces the picture')
  assert.equal([...o.querySelectorAll('nav.ws-shell-side button')].find((b) => b.textContent === SH.sidebar.outline).getAttribute('aria-pressed'), 'true')
  const [threadList, all] = o.querySelectorAll('.ws-trace-outline ol')
  const t = M.threadOf(S, CLAIM)
  assert.deepEqual([...threadList.querySelectorAll('button')].map((b) => b.getAttribute('data-id')), t.ids)
  /* the same cards the picture draws: ev3's fold is open, so its two steps show and the fold does not */
  assert.deepEqual([...all.querySelectorAll('button')].map((b) => b.getAttribute('data-id')).sort(), S.nodes.filter((n) => n.id !== 'ev3').map((n) => n.id).sort())
  const text = o.querySelector('.ws-trace-outline').textContent
  for (const l of ['Request lane', 'Work lane', 'Execution lane', 'Claims & evidence lane']) assert.ok(text.includes(l), l)
  assert.ok(text.includes(T.edges.reference + ':'))
  assert.equal(o.querySelector(`.ws-trace-outline [data-section="all"] [data-id="${CLAIM}"]`).getAttribute('aria-current'), 'true')
  /* one tab stop per list */
  for (const list of [threadList, all]) assert.equal(list.querySelectorAll('button[tabindex="0"]').length, 1)
})

test('the Outline moves with the keyboard and the selection is the canvas\'s own', async () => {
  const dom = new JSDOM('<!doctype html><body><div id="root"></div></body>')
  const names = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'KeyboardEvent']
  const held = names.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  for (const n of names) Object.defineProperty(globalThis, n, { value: n === 'window' ? dom.window : dom.window[n], configurable: true, writable: true })
  try {
    const picked = []
    const rows = M.outlineRows(S, S.nodes.filter((n) => !n.member), 'plain')
    const root = M.createRoot(dom.window.document.getElementById('root'))
    let sel = rows[0].id
    const draw = () => M.flushSync(() => root.render(M.React.createElement(M.TraceOutline, { rows, thread: null, selected: sel, onSelect: (id) => { picked.push(id); sel = id; draw() }, onBack() {} })))
    draw()
    const first = dom.window.document.querySelector(`[data-section="all"] [data-id="${rows[0].id}"]`)
    first.focus()
    first.dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    assert.deepEqual(picked, [rows[1].id])
    assert.equal(dom.window.document.activeElement.getAttribute('data-id'), rows[1].id, 'focus follows')
    dom.window.document.activeElement.dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: 'End', bubbles: true }))
    assert.equal(picked.at(-1), rows.at(-1).id)
    root.unmount()
  } finally {
    for (const [n, d] of held) {
      if (d) Object.defineProperty(globalThis, n, d)
      else delete globalThis[n]
    }
    dom.window.close()
  }
})

/* ── the tools: Focus thread, Walk me through it, the scrubber ─────────── */

test('Explore\'s Focus thread and Walk me through it are working buttons, and the scrubber is one range over record order', () => {
  const d = render({})
  const bar = d.querySelector('[role="toolbar"]')
  const btn = (label) => [...bar.querySelectorAll('button')].find((b) => b.textContent === label)
  for (const label of [SH.slots.focus_thread, SH.slots.walk]) {
    assert.ok(btn(label), label)
    assert.equal(btn(label).getAttribute('aria-disabled'), null)
  }
  const range = bar.querySelector('input[type="range"]')
  assert.equal(range.getAttribute('aria-label'), T.scrub.label)
  assert.equal(range.getAttribute('max'), String(S.events.length))
  assert.equal(range.getAttribute('aria-valuetext'), 'The whole record: 12 steps')
  const at = render({ replay: { idx: 6, playing: false } }).querySelector('[role="toolbar"] input[type="range"]')
  assert.equal(at.getAttribute('aria-valuetext'), 'At step 7: 7 of 12')
  const tech = render({ replay: { idx: 6, playing: false }, register: 'operator' }).querySelector('[role="toolbar"] input[type="range"]')
  assert.equal(tech.getAttribute('aria-valuetext'), 'At seq 7: 7 of 12')
  /* Explore's toolbar has no replay buttons: the scrubber replaced them */
  assert.equal([...bar.querySelectorAll('button')].some((b) => /Replay|Step forward|Step back/.test(b.getAttribute('aria-label') || b.textContent)), false)
})

test('Workbench keeps every control it had, adds the scrubber beside the replay buttons, and offers the lanes as a layout', () => {
  /* the census's own synthetic session and screen, which the baseline was measured on */
  const BASELINE = JSON.parse(read('workbench-controls.baseline.json'))
  const d = doc(renderWorkspaceHtml(M, 'workbench'))
  assert.deepEqual(missingNames(BASELINE.rows, census(d)), [])
  const names = census(d).map((r) => r.name)
  for (const n of ['Step back', 'Step forward', T.scrub.label, '≡ Lanes', '→ Horizontal', '↓ Vertical']) assert.ok(names.includes(n), n)
  assert.equal(d.querySelector('[aria-label="Trace direction"] button[aria-pressed="true"]').textContent, '≡ Lanes', 'the Trace opens in its lanes')
})

test('the scrubber moves the replay head; its last position is the whole record', () => {
  const w = workspace({})
  w.scrubTo(6)
  assert.equal(w.state.replay.idx, 6)
  assert.equal(w.state.sel, 'ev7')
  w.scrubTo(S.events.length)
  assert.equal(w.state.replay.idx, -1)
})

test('Walk: J steps along the lit thread, K back, never wrapping; with nothing lit it walks record order', () => {
  const w = workspace({ sel: CLAIM })
  w.centerOn = () => {}
  const t = M.threadOf(S, CLAIM)
  const said = w.walk(1)
  assert.equal(w.state.sel, t.ids[1])
  assert.ok(said.startsWith('2 of 4 on the thread: '), said)
  w.walk(1); w.walk(1)
  assert.equal(w.walk(1), T.walk.end)
  assert.equal(w.state.sel, t.ids[3])
  w.walk(-1)
  assert.equal(w.state.sel, t.ids[2])
  const r = workspace({})
  r.centerOn = () => {}
  assert.ok(r.walk(1).startsWith('1 of '))
  assert.equal(r.state.sel, 'ev1')
})

test('F focuses the lit thread; with nothing lit it lights the current discrepancy\'s and fits it; with no thread anywhere it says so', () => {
  const w = workspace({})
  let fitted = null
  w.fit = (cards) => (fitted = cards.map((n) => n.id))
  const said = w.focusThread()
  assert.equal(w.state.sel, CLAIM)
  assert.deepEqual(fitted.sort(), M.threadOf(S, CLAIM).ids.map((id) => (id === 'ev4' ? 'ev3' : id)).sort())
  assert.equal(said, 'The thread is on screen: 4 steps.')
  const s = session()
  s.wires = []
  const bare = workspace({}, s)
  bare.fit = () => assert.fail('nothing to fit')
  assert.equal(bare.focusThread(), T.thread.none_focus)
})

test('Esc, or Show everything, lets go of the thread — unless a palette, menu, drawer or find list is open', () => {
  const w = workspace({ sel: CLAIM })
  w.clearThread()
  assert.equal(w.state.sel, null)
  const m = workspace({ sel: CLAIM, menu: 'panels' })
  m.clearThread()
  assert.equal(m.state.sel, CLAIM)
})

/* ── the keys, pressed ─────────────────────────────────────────────────── */

test('real keys on the top bar: J and K walk, F focuses, and each says what it did; Esc with nothing open lets go of the thread', async () => {
  const dom = new JSDOM('<!doctype html><body><div id="root"></div></body>')
  const w = dom.window
  const names = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'KeyboardEvent']
  const held = names.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  for (const n of names) Object.defineProperty(globalThis, n, { value: n === 'window' ? w : w[n], configurable: true, writable: true })
  const settle = () => new Promise((r) => setTimeout(r, 5))
  try {
    const calls = []
    const root = M.createRoot(w.document.getElementById('root'))
    M.flushSync(() => root.render(M.React.createElement(M.ShellTopBar, {
      crumbs: [], altitude: 'explore', onAltitude() {}, register: 'manager', onRegister() {},
      onWalk: (dir) => { calls.push('walk ' + dir); return 'walked ' + dir },
      onThread: () => { calls.push('thread'); return 'focused' },
      onEscape: () => calls.push('escape'),
    })))
    await settle()
    const press = async (key, extra) => {
      w.document.body.dispatchEvent(new w.KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...extra }))
      await settle()
    }
    const note = () => w.document.querySelector('.ws-shell-note').textContent
    await press('j')
    assert.equal(note(), 'walked 1')
    await press('k')
    assert.equal(note(), 'walked -1')
    await press('f')
    assert.equal(note(), 'focused')
    await press('?', { shiftKey: true })
    await press('Escape')
    assert.deepEqual(calls, ['walk 1', 'walk -1', 'thread'], 'Esc closed the key list, and only that')
    await press('Escape')
    assert.deepEqual(calls.at(-1), 'escape')
    root.unmount()
  } finally {
    for (const [n, d] of held) {
      if (d) Object.defineProperty(globalThis, n, d)
      else delete globalThis[n]
    }
    w.close()
  }
})

/* ── the words ─────────────────────────────────────────────────────────── */

const walkStrings = (o, p = []) => Object.entries(o).flatMap(([k, v]) => (typeof v === 'string' ? [[p.concat(k).join('.'), v]] : walkStrings(v, p.concat(k))))

test('every Trace string outside `technical` is Sam\'s vocabulary: no engine noun, no accusation, no all-clear', () => {
  const strings = walkStrings(T).filter(([k]) => !k.startsWith('technical.'))
  assert.ok(strings.length > 80, `found ${strings.length}`)
  const PLAIN_BANNED = [/\bevents?\b/i, /\breceipts?\b/i, /\bcandidates?\b/i, /\bapplicability\b/i, /\bdispositions?\b/i, /\bpacket\b/i, /\bledger\b/i, /\bsession id\b/i, /\btree\b/i, /\bpredicates?\b/i, /\bseq\b/i, /\bfacets?\b/i, /\bobjects?\b/i, /\bderived\b/i]
  const NEVER = [/\blie[ds]?\b/i, /\blying\b/i, /\bhallucinat/i, /\bcaught\b/i, /\bbullshit\b/i, /\bbug-free\b/i, /\bsafe\b/i, /\ball clear\b/i, /\d\s*%/]
  for (const [path, text] of strings) for (const re of [...PLAIN_BANNED, ...NEVER]) assert.doesNotMatch(text, re, `trace.${path}: ${text}`)
  for (const b of HQ_COPY.banned) for (const [path, text] of strings) assert.doesNotMatch(text, new RegExp(`\\b${b}\\b`, 'i'), `trace.${path}`)
})

test('the Trace draws its words from the copy file: no human sentence is written in trace-view.js or trace-outline.jsx', () => {
  const sentences = (src) => {
    const out = []
    const visit = (node) => {
      if (!node || typeof node.type !== 'string') return
      if (node.type === 'StringLiteral' || node.type === 'JSXText') {
        const v = String(node.value).trim()
        /* three words or more, starting with a letter: a sentence a person reads */
        if (/^[A-Za-z][a-z']*( [a-z'&]+){2,}/.test(v)) out.push(v)
      }
      for (const k of Object.keys(node)) {
        if (k === 'loc' || k === 'start' || k === 'end') continue
        const c = node[k]
        if (Array.isArray(c)) c.forEach(visit)
        else if (c && typeof c.type === 'string') visit(c)
      }
    }
    visit(parse(src, { sourceType: 'module', plugins: ['jsx'] }).program)
    return out
  }
  assert.deepEqual(sentences(read('trace-view.js')), [])
  assert.deepEqual(sentences(read('trace-outline.jsx')), [])
})
