/* PATH-UI-2-R3 — a door to a line lands on THAT line, with its control and the
 * name box both on the glass, on a Change of any length.
 *
 * Found by the R2 adversary (launch gates/adversary-PATH-UI-2-R2/TRANSCRIPT-
 * dba312fd.md, BROKEN 1). R2's door opened the line and focused the name box.
 * On a Change with twelve accepted lines every door 1..12 landed identically:
 * the name box focused, all twelve line rows above the viewport, and from the
 * fifth line up the named line's "This bears on it" more than a screen below
 * the fold. Door 12 was indistinguishable from door 1.
 *
 * WHAT IT WAS. `landOnLine` scrolled the line's SUMMARY entry into view (the
 * list of accepted lines, above the Inspector) and then called `name.focus()`
 * with the browser's default scrolling. The controls — the one name box and one
 * row per line with its link control — are a separate list about three
 * thousand pixels further down, so the default focus scroll carried the
 * viewport to the name box and left the summary entry, and every control but
 * the first few, out of sight.
 *
 * WHY THIS TEST LOOKS THE WAY IT DOES. jsdom draws nothing, so it has no
 * scroll. What CAN be pinned in node is what the code asks the browser to do:
 * every `scrollIntoView` and every `focus`, in order, with their options. Those
 * calls are replayed against a small model of Chrome's own rules — the block
 * alignments, and that `focus()` without `preventScroll` scrolls a hidden
 * element to the centre — over a layout calibrated on the real measurement
 * (`ui/test-support/door-geometry.mjs`, 1280x800: name box 273 px from the top
 * of the glass at scrollY 3706; rows 161 px apart; the link control 68 px into
 * its row). The model is a stand-in for layout and says so; the real geometry
 * is measured in Chrome by that script, and its numbers are in the commit.
 *
 * WHAT A DOOR MUST LEAVE (the card's Done-when 1):
 *   (a) line k's row and the control that links it are inside the viewport;
 *   (b) the name box is focused AND inside the viewport;
 *   (c) the landing says which line: the row for line k, and only it, is
 *       marked current (`aria-current`), and the focused name box's accessible
 *       description names line k's text. Door 12's description is not door 1's.
 * "The row" is the row that HOLDS the control (`.ws-cov-rows > li`), not the
 * summary entry `#ws-cov-line-k`.
 *
 * Every fixture is synthetic; nothing here reads a real machine.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'
import { CHANGE_ID as ID, linesOf, routes } from '../../test-support/door-fixture.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-door-r3-'))
after(() => rm(temporary, { recursive: true, force: true }))

/* React DOM decides at LOAD whether the browser has a DOM (see packet-link-door.test.js). */
const boot = new JSDOM('<!doctype html><html><body></body></html>', { url: 'http://127.0.0.1/' })
const BOOT_GLOBALS = { window: boot.window, document: boot.window.document, navigator: boot.window.navigator }
for (const [k, v] of Object.entries(BOOT_GLOBALS)) Object.defineProperty(globalThis, k, { value: v, configurable: true, writable: true })
after(() => { for (const k of Object.keys(BOOT_GLOBALS)) delete globalThis[k]; boot.window.close() })

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
      import PacketView from './packet.jsx';
      import ChangeWorkspace from './change-workspace.jsx';
      import { chooseScreen } from '../routes.js';
      import { HQ_COPY } from './hq-copy.js';
      export { React, createRoot, flushSync, PacketView, ChangeWorkspace, chooseScreen, HQ_COPY };
    `,
  },
})
const file = join(temporary, 'door-r3.mjs')
await writeFile(file, result.outputFiles[0].text)
const L = await import(pathToFileURL(file).href)
const C = L.HQ_COPY.change

/* ── a model of the browser's scrolling, over a calibrated layout ────────── */

/** The viewport heights the card names. */
const VH = 800

/**
 * Where things are, in pixels from the top of the document, as Chrome laid the
 * Verify stage out at 1280 px wide (door-geometry.mjs, dba312fd).
 *   - the summary entries (`.ws-cov-line`, the list of accepted lines) start
 *     at 860 and are 84 apart;
 *   - about 2,100 px of Inspector and other blocks sit between that list and
 *     the coverage section, whatever the number of lines;
 *   - inside the section the name box's input is 39 px tall, and the first row
 *     starts 55 px below the input's top; rows are 161 px apart and 139 tall;
 *   - a row's link control is 68 px into it and 42 tall.
 */
const LAYOUT = { linesTop: 860, linePitch: 84, lineHeight: 70, gapToCoverage: 2111, actorLead: 24, inputHeight: 39, firstRowBelowInput: 55, rowPitch: 161, rowHeight: 139, controlAt: 68, controlHeight: 42 }

/** One document's replayed scroll: what was asked of it, and where the viewport ended up. */
function browserModel(d) {
  const state = { scrollY: 0, calls: [] }
  const rows = () => [...d.querySelectorAll('.ws-cov-rows > li')]
  const summaries = () => [...d.querySelectorAll('.ws-cov-line')]
  const inputTop = () => LAYOUT.linesTop + summaries().length * LAYOUT.linePitch + LAYOUT.gapToCoverage
  const rowTop = (i) => inputTop() + LAYOUT.firstRowBelowInput + i * LAYOUT.rowPitch
  /** The rectangle (document pixels) an element occupies, or null where this model has none to give. */
  function rect(el) {
    if (!el) return null
    const s = el.closest ? el.closest('.ws-cov-line') : null
    if (s) return { top: LAYOUT.linesTop + summaries().indexOf(s) * LAYOUT.linePitch, height: LAYOUT.lineHeight }
    const actor = el.closest ? el.closest('.ws-cov-actor') : null
    if (actor && actor.closest('.ws-cov')) {
      return el.tagName === 'INPUT'
        ? { top: inputTop(), height: LAYOUT.inputHeight }
        : { top: inputTop() - LAYOUT.actorLead, height: LAYOUT.inputHeight + LAYOUT.actorLead }
    }
    const row = el.closest ? el.closest('.ws-cov-rows > li') : null
    if (row) {
      const top = rowTop(rows().indexOf(row))
      const control = el.closest('button')
      if (control && control.textContent.trim() === C.coverage.link) return { top: top + LAYOUT.controlAt, height: LAYOUT.controlHeight }
      return el === row ? { top, height: LAYOUT.rowHeight } : { top, height: LAYOUT.rowHeight }
    }
    return null
  }
  const inView = (r) => !!r && r.top >= state.scrollY && r.top + r.height <= state.scrollY + VH
  const hidden = (r) => r.top + r.height <= state.scrollY || r.top >= state.scrollY + VH
  const clamp = (y) => Math.max(0, Math.round(y))
  return {
    state,
    rect,
    inView: (el) => inView(rect(el)),
    /** `Element.scrollIntoView(arg)`: the block alignments Chrome documents; no argument is `block: 'start'`. */
    scrollIntoView(el, arg) {
      const r = rect(el)
      state.calls.push({ kind: 'scroll', el, arg: arg === undefined ? null : arg })
      if (!r) return
      const block = arg && typeof arg === 'object' && arg.block ? arg.block : 'start'
      if (block === 'start') state.scrollY = clamp(r.top)
      else if (block === 'center') state.scrollY = clamp(r.top - (VH - r.height) / 2)
      else if (block === 'end') state.scrollY = clamp(r.top + r.height - VH)
      else if (block === 'nearest') {
        if (r.top < state.scrollY) state.scrollY = clamp(r.top)
        else if (r.top + r.height > state.scrollY + VH) state.scrollY = clamp(r.top + r.height - VH)
      }
    },
    /** `HTMLElement.focus(opts)`: without `preventScroll`, Chrome brings a hidden element to the centre and leaves a visible one where it is. */
    focus(el, opts) {
      const prevented = !!(opts && opts.preventScroll)
      state.calls.push({ kind: 'focus', el, prevented })
      if (prevented) return
      const r = rect(el)
      if (r && hidden(r)) state.scrollY = clamp(r.top - (VH - r.height) / 2)
    },
  }
}

/* ── a browser, borrowed for one document ────────────────────────────────── */

const GLOBALS = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'HTMLInputElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage', 'CSS', 'fetch']
const settle = () => new Promise((r) => setTimeout(r, 10))

const fetchOf = (table, writes) => async (url, init) => {
  const path = String(url).replace(/^https?:\/\/[^/]+/, '').split('?')[0]
  if (init && init.method && init.method !== 'GET') {
    writes.push({ path, method: init.method, body: init.body })
    return { ok: true, status: 200, statusText: 'OK', headers: { get: () => null }, text: async () => '{}', json: async () => ({}) }
  }
  const doc = Object.prototype.hasOwnProperty.call(table, path) ? table[path] : null
  return doc
    ? { ok: true, status: 200, statusText: 'OK', headers: { get: () => null }, text: async () => JSON.stringify(doc), json: async () => doc }
    : { ok: false, status: 404, statusText: 'Not Found', headers: { get: () => null }, text: async () => '{}', json: async () => ({}) }
}

/** One document at `url`, the Change screen mounted in it as `main.jsx` mounts it, with the scrolling model wired to its `scrollIntoView` and `focus`. */
async function document_(url, table, run, { strict = false } = {}) {
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url, pretendToBeVisual: true })
  const w = dom.window
  const writes = []
  const model = browserModel(w.document)
  w.Element.prototype.scrollIntoView = function scrollIntoView(arg) { model.scrollIntoView(this, arg) }
  const realFocus = w.HTMLElement.prototype.focus
  w.HTMLElement.prototype.focus = function focus(opts) { realFocus.call(this, opts); model.focus(this, opts) }
  const held = GLOBALS.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of GLOBALS) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  set('fetch', fetchOf(table, writes))
  const at = L.chooseScreen({ pathname: w.location.pathname, hash: w.location.hash, search: w.location.search })
  assert.equal(at.screen, 'change', `the address ${url} names the Change screen`)
  const root = L.createRoot(w.document.getElementById('root'))
  const element = L.React.createElement(L.ChangeWorkspace, { id: at.id, stage: at.stage })
  try {
    L.flushSync(() => root.render(strict ? L.React.createElement(L.React.StrictMode, null, element) : element))
    for (let i = 0; i < 40; i += 1) await settle()
    return await run({ w, d: w.document, writes, model })
  } finally {
    root.unmount()
    for (const [n, desc] of held) {
      if (desc) Object.defineProperty(globalThis, n, desc)
      else delete globalThis[n]
    }
    w.close()
  }
}

/** The address a whole-story page's door for line k names (`?door=<k>#/changes/<id>/verify`), read off the page as a person's click would. */
async function doorHrefFor(lines, k) {
  const from = 'http://127.0.0.1/#/changes/' + ID + '/packet'
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: from, pretendToBeVisual: true })
  const w = dom.window
  const held = GLOBALS.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of GLOBALS) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  set('fetch', fetchOf(routes(lines, ID), []))
  const root = L.createRoot(w.document.getElementById('root'))
  try {
    L.flushSync(() => root.render(L.React.createElement(L.PacketView, { id: ID })))
    for (let i = 0; i < 40; i += 1) await settle()
    const door = [...w.document.querySelectorAll('a.ws-packet-link-door')].find((a) => a.textContent.includes(lines[k - 1]))
    assert.ok(door, `the whole-story page has a door for line ${k}`)
    return new URL(door.getAttribute('href'), from).href
  } finally {
    root.unmount()
    for (const [n, desc] of held) {
      if (desc) Object.defineProperty(globalThis, n, desc)
      else delete globalThis[n]
    }
    w.close()
  }
}

/* ── what a landing left ──────────────────────────────────────────────── */

const nameInput = (d) => d.querySelector('.ws-cov .ws-cov-actor input')
const rowOf = (d, text) => [...d.querySelectorAll('.ws-cov-rows > li')].find((li) => { const p = li.querySelector('.ws-cov-criterion'); return !!p && p.textContent === text }) || null
const linkControl = (row) => (row ? [...row.querySelectorAll('button')].find((b) => b.textContent.trim() === C.coverage.link) || null : null)
const described = (d, el) => (el && el.getAttribute('aria-describedby') ? el.getAttribute('aria-describedby').split(/\s+/).map((id) => { const e = d.getElementById(id); return e ? e.textContent : '' }).join(' ').trim() : '')
const describe = (el) => (el ? `${el.tagName.toLowerCase()}${el.id ? '#' + el.id : ''}${el.className ? '.' + String(el.className).split(' ')[0] : ''}` : 'nothing')

/** The card's (a), (b) and (c) for line `k`, asserted on the document a door led to. Booleans only: a failed `assert.equal` of two DOM nodes prints both object graphs. */
function assertLanded({ d, model }, lines, k) {
  const text = lines[k - 1]
  const row = rowOf(d, text)
  assert.ok(row, `line ${k} has its row in the controls list`)
  const control = linkControl(row)
  assert.ok(control, `line ${k}'s row has its link control`)
  const input = nameInput(d)
  assert.ok(input, 'the name box is drawn')
  const y = model.state.scrollY
  /* (a) */
  assert.ok(model.inView(row), `(a) line ${k}'s row is on the glass: row at ${model.rect(row).top}, viewport ${y}..${y + VH}`)
  assert.ok(model.inView(control), `(a) line ${k}'s link control is on the glass: control at ${model.rect(control).top}, viewport ${y}..${y + VH}`)
  /* (b) */
  assert.ok(d.activeElement === input, `(b) focus is on the name box, not on ${describe(d.activeElement)}`)
  assert.ok(model.inView(input), `(b) the name box is on the glass: input at ${model.rect(input).top}, viewport ${y}..${y + VH}`)
  /* (c) */
  const current = [...d.querySelectorAll('[aria-current]')].filter((e) => e.closest('.ws-cov-rows'))
  assert.ok(current.length === 1 && current[0] === row, `(c) exactly line ${k}'s row is marked current (${current.length} are)`)
  assert.ok(described(d, input).includes(text), `(c) the name box's description names line ${k}: ${JSON.stringify(described(d, input))}`)
}

/** What was asked of the browser, in words a failure can print. */
const calls = (model) => model.state.calls.map((c) => (c.kind === 'scroll' ? `scrollIntoView(${describe(c.el)}, ${JSON.stringify(c.arg)})` : `focus(${describe(c.el)}, preventScroll=${c.prevented})`)).join(' -> ')

const at = (k) => 'http://127.0.0.1/?door=' + k + '#/changes/' + ID + '/verify'

/* ── the door, on a Change of any length ───────────────────────────────── */

for (const n of [5, 12, 40]) {
  const ks = [...new Set([1, Math.ceil(n / 2), n])]
  for (const k of ks) {
    test(`with ${n} accepted lines, door ${k} leaves line ${k}'s row and control, and the focused name box, on the glass, and says which line`, async () => {
      const lines = linesOf(n)
      const table = routes(lines, ID)
      const href = await doorHrefFor(lines, k)
      assert.equal(new URL(href).search, '?door=' + k, 'the whole-story page names line ' + k + ' in the address')
      await document_(href, table, async (page) => {
        assertLanded(page, lines, k)
        /* What the code asked: nothing after the last scroll may move the view away, and the last scroll is not the far-off summary entry. */
        const asked = page.model.state.calls
        const lastScroll = [...asked].reverse().find((c) => c.kind === 'scroll')
        assert.ok(lastScroll, `the page was asked to scroll: ${calls(page.model)}`)
        assert.ok(!lastScroll.el.closest('.ws-cov-line'), `the last scroll is not to the summary entry, which is nowhere near the control: ${calls(page.model)}`)
        const lastScrollAt = asked.lastIndexOf(lastScroll)
        const focusAfter = asked.slice(lastScrollAt + 1).filter((c) => c.kind === 'focus' && !c.prevented)
        assert.equal(focusAfter.length, 0, `no focus that scrolls comes after the last scroll: ${calls(page.model)}`)
      })
    })
  }
}

test('door 12 and door 1 of twelve lines are different landings: the marked row and the name box\'s description name their own line', async () => {
  const lines = linesOf(12)
  const table = routes(lines, ID)
  const seen = []
  for (const k of [1, 12]) {
    await document_(await doorHrefFor(lines, k), table, async ({ d }) => {
      const current = [...d.querySelectorAll('[aria-current]')].filter((e) => e.closest('.ws-cov-rows'))
      assert.equal(current.length, 1, `door ${k}: one row is marked current`)
      seen.push({ marked: current[0].querySelector('.ws-cov-criterion').textContent, description: described(d, nameInput(d)) })
    })
  }
  assert.notEqual(seen[0].marked, seen[1].marked, 'the marked row differs')
  assert.notEqual(seen[0].description, seen[1].description, 'the name box\'s description differs')
  assert.equal(seen[1].marked, lines[11])
})

test('a Change of two lines still lands with both link controls in reach and the name box focused (the walk fixture)', async () => {
  const lines = linesOf(2)
  const table = routes(lines, ID)
  for (const k of [1, 2]) {
    await document_(await doorHrefFor(lines, k), table, async ({ d, model }) => {
      const input = nameInput(d)
      assert.ok(d.activeElement === input, 'the name box is focused')
      assert.ok(model.inView(input), 'the name box is on the glass')
      for (const line of lines) assert.ok(model.inView(linkControl(rowOf(d, line))), `the control for "${line}" is on the glass`)
    })
  }
})

/* ── the doors inside the app that go to a line's control ──────────────── */

test('the Inspector\'s "link it" door lands as the whole-story page\'s does: row, control and focused name box on the glass, the line marked', async () => {
  const lines = linesOf(12)
  const table = routes(lines, ID)
  await document_('http://127.0.0.1/#/changes/' + ID, table, async (page) => {
    const { d, w } = page
    assert.equal(d.querySelector('.ws-shell').getAttribute('data-altitude'), 'brief')
    const door = d.querySelector('.ws-what-broke[data-door^="ws-cov-line-"]')
    assert.ok(door, 'Brief draws the Inspector\'s door to the line that needs a link')
    const k = Number(door.getAttribute('data-door').replace('ws-cov-line-', ''))
    door.dispatchEvent(new w.MouseEvent('click', { bubbles: true }))
    for (let i = 0; i < 40; i += 1) await settle()
    assert.equal(d.querySelector('.ws-shell').getAttribute('data-altitude'), 'explore', 'the door opens the altitude that draws the controls')
    assertLanded(page, lines, k)
  })
})

test('a door that names a line the Change does not have moves nothing and takes no focus', async () => {
  const lines = linesOf(5)
  await document_(at(9), routes(lines, ID), async ({ d, model }) => {
    assert.ok(nameInput(d) === null || d.activeElement !== nameInput(d), 'no focus was taken')
    assert.equal([...d.querySelectorAll('[aria-current]')].filter((e) => e.closest('.ws-cov-rows')).length, 0, 'no row is marked')
    assert.equal(model.state.calls.filter((c) => c.kind === 'focus' && c.el === nameInput(d)).length, 0)
  })
})

test('with no door in the address nothing is marked, nothing is focused and the rows keep the order the record has', async () => {
  const lines = linesOf(5)
  await document_('http://127.0.0.1/?altitude=explore#/changes/' + ID + '/verify', routes(lines, ID), async ({ d }) => {
    assert.ok(nameInput(d), 'the controls are drawn')
    assert.ok(d.activeElement !== nameInput(d), 'no focus was taken')
    assert.equal([...d.querySelectorAll('[aria-current]')].filter((e) => e.closest('.ws-cov-rows')).length, 0)
    assert.deepEqual([...d.querySelectorAll('.ws-cov-rows > li .ws-cov-criterion')].map((p) => p.textContent), lines)
    assert.equal(nameInput(d).getAttribute('aria-describedby'), null, 'the name box describes no line')
  })
})

/* ── self-attack: the ways a landing can be misused, repeated or left behind ── */

const markedRows = (d) => [...d.querySelectorAll('.ws-cov-rows > li[aria-current]')]
const typeName = async ({ d, w }, name) => {
  const input = nameInput(d)
  Object.getOwnPropertyDescriptor(w.HTMLInputElement.prototype, 'value').set.call(input, name)
  input.dispatchEvent(new w.Event('input', { bubbles: true }))
  for (let i = 0; i < 6; i += 1) await settle()
}
const press = async ({ d, w }, el) => {
  el.dispatchEvent(new w.MouseEvent('click', { bubbles: true }))
  for (let i = 0; i < 12; i += 1) await settle()
  void d
}
const doorTo = (k, altitude = 'explore') => 'http://127.0.0.1/?altitude=' + altitude + '&door=' + k + '#/changes/' + ID + '/verify'

test('the row pinned first is the row the door named: typing a name turns ITS control on, and linking writes THAT line and no other', async () => {
  const lines = linesOf(12)
  await document_(await doorHrefFor(lines, 12), routes(lines, ID), async (page) => {
    const { d, writes } = page
    assertLanded(page, lines, 12)
    await typeName(page, 'walker')
    const row = rowOf(d, lines[11])
    const control = linkControl(row)
    assert.ok(control && !control.disabled, 'line 12\'s link control is on once a name is typed')
    for (const other of lines.slice(0, 11)) assert.ok(linkControl(rowOf(d, other)) && !linkControl(rowOf(d, other)).disabled, 'and so is every other line\'s: one name box gates them all')
    await press(page, control)
    const candidate = [...row.querySelectorAll('.ws-cov-candidates button')].find((b) => b.textContent.trim() === 'cargo test')
    assert.ok(candidate, 'the run that nobody linked is offered under line 12')
    await press(page, candidate)
    const link = writes.find((x) => x.method === 'POST' && x.path === '/api/changes/' + ID + '/links')
    assert.ok(link, `a link was written: ${JSON.stringify(writes.map((x) => x.method + ' ' + x.path))}`)
    const body = JSON.parse(link.body)
    assert.equal(body.criterion, 12, 'the link names line 12, the line the door named, not the row that used to be first')
    assert.equal(body.act, 'link')
    assert.equal(body.actor, 'walker')
    assert.equal(writes.filter((x) => x.method === 'POST').length, 1, 'one write, and only that one')
  })
})

test('after the link is written and the screen reads the record again, the landing is still there: the same row first, marked, the cursor not thrown away', async () => {
  const lines = linesOf(12)
  await document_(await doorHrefFor(lines, 7), routes(lines, ID), async (page) => {
    const { d } = page
    await typeName(page, 'walker')
    const row = rowOf(d, lines[6])
    await press(page, linkControl(row))
    await press(page, [...row.querySelectorAll('.ws-cov-candidates button')][0])
    for (let i = 0; i < 20; i += 1) await settle()
    const marked = markedRows(d)
    assert.equal(marked.length, 1)
    assert.equal(marked[0].querySelector('.ws-cov-criterion').textContent, lines[6])
    assert.ok(d.querySelector('.ws-cov-rows > li') === marked[0], 'and it is still first')
    assert.equal(nameInput(d).value, 'walker', 'the name is still what was typed')
  })
})

test('a second door moves the mark and does not add one: door 5, then the Inspector\'s door, leaves line 1 marked and only line 1', async () => {
  const lines = linesOf(12)
  await document_(doorTo(5), routes(lines, ID), async (page) => {
    const { d, model } = page
    assert.equal(markedRows(d).length, 1)
    assert.equal(markedRows(d)[0].querySelector('.ws-cov-criterion').textContent, lines[4])
    const door = d.querySelector('.ws-what-broke[data-door^="ws-cov-line-"]')
    assert.ok(door, 'the Inspector offers its door on the Verify stage too')
    const k = Number(door.getAttribute('data-door').replace('ws-cov-line-', ''))
    await press(page, door)
    assert.equal(markedRows(d).length, 1, 'one row is marked')
    assert.equal(markedRows(d)[0].querySelector('.ws-cov-criterion').textContent, lines[k - 1])
    assertLanded({ d, model }, lines, k)
  })
})

test('the same door pressed again brings the view back to the name box and the row, after the person has scrolled away', async () => {
  const lines = linesOf(12)
  await document_(doorTo(1), routes(lines, ID), async (page) => {
    const { d, model } = page
    assertLanded(page, lines, 1)
    model.state.scrollY = 0
    assert.ok(!model.inView(nameInput(d)), 'scrolled away: the name box is off the glass')
    const door = d.querySelector('.ws-what-broke[data-door="ws-cov-line-1"]')
    assert.ok(door, 'the Inspector\'s door to line 1')
    await press(page, door)
    assertLanded(page, lines, 1)
  })
})

test('leaving the Verify stage and coming back forgets the landing: nothing marked, no focus taken, the rows in the order the record has', async () => {
  const lines = linesOf(5)
  await document_(doorTo(4), routes(lines, ID), async (page) => {
    const { d } = page
    assertLanded(page, lines, 4)
    await press(page, d.getElementById('ws-change-tab-define'))
    assert.equal(d.getElementById('ws-change-tab-define').getAttribute('aria-selected'), 'true')
    await press(page, d.getElementById('ws-change-tab-verify'))
    assert.equal(d.getElementById('ws-change-tab-verify').getAttribute('aria-selected'), 'true')
    assert.equal(markedRows(d).length, 0, 'the row a door brought a person to is not still marked when they come back on their own')
    assert.deepEqual([...d.querySelectorAll('.ws-cov-rows > li .ws-cov-criterion')].map((p) => p.textContent), lines)
    assert.ok(d.activeElement !== nameInput(d), 'and the cursor is not put back in the name box')
    assert.equal(nameInput(d).getAttribute('aria-describedby'), null)
  })
})

test('the keyboard follows what is on the glass: from the name box the next control is the marked row\'s link control', async () => {
  const lines = linesOf(12)
  await document_(await doorHrefFor(lines, 9), routes(lines, ID), async (page) => {
    const { d, w } = page
    await typeName(page, 'walker')
    const input = nameInput(d)
    const after = [...d.querySelectorAll('.ws-cov button:not([disabled])')].find((b) => input.compareDocumentPosition(b) & w.Node.DOCUMENT_POSITION_FOLLOWING)
    assert.ok(after, 'a control follows the name box')
    assert.equal(after.textContent.trim(), C.coverage.link)
    assert.ok(after.closest('li') === markedRows(d)[0], 'and it is the marked row\'s')
    assert.equal(markedRows(d)[0].querySelector('.ws-cov-criterion').textContent, lines[8])
  })
})

test('under React.StrictMode, as main.jsx mounts it, twelve lines and door 12 leave one marked row, the cursor in the name box and everything on the glass', async () => {
  const lines = linesOf(12)
  await document_(await doorHrefFor(lines, 12), routes(lines, ID), async (page) => {
    assertLanded(page, lines, 12)
    assert.equal(markedRows(page.d).length, 1)
  }, { strict: true })
})

test('a line with markup, braces or a script in it is landed on and printed as text: no element is made from it and no character is replaced', async () => {
  const lines = linesOf(12)
  for (const k of [2, 3, 8]) {
    await document_(await doorHrefFor(lines, k), routes(lines, ID), async (page) => {
      assertLanded(page, lines, k)
      const { d } = page
      assert.ok(d.querySelector('.ws-cov-rows b, .ws-cov-rows script') === null, `line ${k}: nothing in the rows was parsed as markup`)
      assert.equal(markedRows(d)[0].querySelector('.ws-cov-criterion').textContent, lines[k - 1], 'the line is printed exactly')
      assert.ok(described(d, nameInput(d)).endsWith(lines[k - 1]), 'and described exactly')
    })
  }
})

test('a door for a line the Change does not have, made from inside the app after a good one, marks nothing new and takes no focus', async () => {
  const lines = linesOf(5)
  await document_(doorTo(9), routes(lines, ID), async ({ d }) => {
    assert.equal(markedRows(d).length, 0)
    assert.ok(d.activeElement !== nameInput(d))
  })
})

test('a landing asked for inside the app is spent too: after the Inspector\'s door and a visit to another stage, coming back to Verify marks nothing and takes no focus', async () => {
  const lines = linesOf(5)
  await document_('http://127.0.0.1/?altitude=explore#/changes/' + ID + '/verify', routes(lines, ID), async (page) => {
    const { d } = page
    assert.equal(markedRows(d).length, 0, 'nothing is marked to begin with')
    const door = d.querySelector('.ws-what-broke[data-door^="ws-cov-line-"]')
    assert.ok(door)
    await press(page, door)
    assert.equal(markedRows(d).length, 1, 'the door marked a row')
    await press(page, d.getElementById('ws-change-tab-build'))
    await press(page, d.getElementById('ws-change-tab-verify'))
    assert.equal(d.getElementById('ws-change-tab-verify').getAttribute('aria-selected'), 'true')
    assert.equal(markedRows(d).length, 0, 'the request was spent: nothing is marked on the way back')
    assert.ok(d.activeElement !== nameInput(d), 'and nobody takes the cursor')
  })
})

/* ── the run sheet's own door: "Go to <line> to link it", after a passing run ── */

/** What the sheet reads (`GET /api/changes/<id>/run`) once a check has passed: the record's last outcome, no run in flight. */
const PASSED_SHEET = {
  schema_version: 1, change: ID, project_folder: '~/shop',
  permission: { granted_by: 'walker', granted_at: '2026-09-23T10:00:00Z' },
  checks: [{ id: 'cargo-test', program: 'cargo', category: 'tests', command: 'cargo test',
    last: { run: 'r1', state: 'completed', outcome: 'passed', exit_code: 0, tests_passed: 2, tests_failed: 0, suites: 1, recorded_at: '2026-09-23T10:05:00Z', settles: true } }],
  environments: [{ id: 'hq', kind: 'hq', folder: null, has: ['cargo'] }],
  not_runnable_here: [], remembered_environment: null, not_started: null, actor: 'walker', job: null,
}
const withSheet = (lines) => ({ ...routes(lines, ID), ['/api/changes/' + ID + '/run']: PASSED_SHEET })

/** Open the run sheet from the door in `scope`, and press the door inside it that goes to the line. */
async function throughTheSheet(page, scope) {
  const { d } = page
  const open = scope.querySelector('button[aria-haspopup="dialog"]')
  assert.ok(open, 'the run sheet has a door here')
  await press(page, open)
  for (let i = 0; i < 20; i += 1) await settle()
  const door = d.querySelector('.ws-run-unlinked-door')
  assert.ok(door, 'the sheet says a passing run nobody linked and offers its door to the line')
  await press(page, door)
  for (let i = 0; i < 20; i += 1) await settle()
  assert.ok(d.querySelector('.ws-run-unlinked-door') === null, 'the sheet closed behind the door')
}

test('the run sheet\'s door, from the Verify stage of a twelve-line Change, lands as every other door to a line does: row, control and the focused name box on the glass', async () => {
  const lines = linesOf(12)
  await document_(doorTo(1).replace('&door=1', ''), withSheet(lines), async (page) => {
    const { d } = page
    assert.equal(markedRows(d).length, 0)
    await throughTheSheet(page, d.querySelector('.ws-change-run-check'))
    assertLanded(page, lines, 1)
  })
})

test('the run sheet\'s door, opened from Brief, takes the person to the Verify stage and lands there the same way', async () => {
  const lines = linesOf(12)
  await document_('http://127.0.0.1/?altitude=brief#/changes/' + ID, withSheet(lines), async (page) => {
    const { d } = page
    assert.equal(d.querySelector('.ws-shell').getAttribute('data-altitude'), 'brief')
    await throughTheSheet(page, d.querySelector('.ws-brief-run'))
    assert.equal(d.querySelector('.ws-shell').getAttribute('data-altitude'), 'explore', 'the door opened the altitude that draws the controls')
    assertLanded(page, lines, 1)
  })
})

/* ── the doors the card names, for twelve lines: k = 1, 5 and 12 ────────────── */

test('with 12 accepted lines, door 5 (the first the R2 adversary found with its control below the fold) leaves line 5\'s row, control and focused name box on the glass, and says which line', async () => {
  const lines = linesOf(12)
  await document_(await doorHrefFor(lines, 5), routes(lines, ID), async (page) => {
    assertLanded(page, lines, 5)
    const lastScroll = [...page.model.state.calls].reverse().find((c) => c.kind === 'scroll')
    assert.ok(lastScroll && !lastScroll.el.closest('.ws-cov-line'), `the last scroll is not to the summary entry: ${calls(page.model)}`)
  })
})
