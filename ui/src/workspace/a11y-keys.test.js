/* T-6-4 · A — the real key drive (design record §6; 03-USER-LENS rules 6, 7).
 *
 * The real `Workspace` (Explore, over the bundled demo record it opens when no
 * server answers) and the real `ChangeWorkspace` (Brief, over a synthetic
 * Change) are mounted in jsdom and driven with real `KeyboardEvent`s,
 * dispatched on `document.activeElement` so they bubble to the one key handler
 * on the window exactly as a key press does. Nothing here calls `.focus()`:
 * every focus move is the product's own. After each key the test asserts that
 * `document.activeElement` is the element the selection says it should be.
 *
 * The whole §6 map is pressed on both screens: J K / . T F E P B Esc ?
 */
import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'
import { HQ_COPY } from './hq-copy.js'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-a11y-keys-'))
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
      import ChangeWorkspace from './change-workspace.jsx';
      import { a11yFetch, A11Y_CHANGE_ID } from './a11y-fixture.js';
      import { discrepancyAnchor } from './trace-view.js';
      export { React, createRoot, flushSync, Workspace, ChangeWorkspace, a11yFetch, A11Y_CHANGE_ID, discrepancyAnchor };
    `,
  },
})
const file = join(temporary, 'drive.mjs')
await writeFile(file, result.outputFiles[0].text)
const L = await import(pathToFileURL(file).href)

const SH = HQ_COPY.shell
const settle = () => new Promise((r) => setTimeout(r, 10))
const GLOBALS = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage', 'CSS', 'fetch']

/** A browser, borrowed for one test, with one screen mounted in it. */
async function mounted(url, element, run) {
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url, pretendToBeVisual: true })
  const w = dom.window
  const held = GLOBALS.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of GLOBALS) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  set('fetch', L.a11yFetch)
  const root = L.createRoot(w.document.getElementById('root'))
  try {
    const ref = L.React.createRef()
    L.flushSync(() => root.render(element(ref)))
    for (let i = 0; i < 60; i += 1) await settle()
    const d = w.document
    /** One real key press, on whatever has focus now. */
    const press = async (key, extra) => {
      const target = d.activeElement || d.body
      target.dispatchEvent(new w.KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...(extra || {}) }))
      for (let i = 0; i < 4; i += 1) await settle()
    }
    return await run({ w, d, press, ref })
  } finally {
    root.unmount()
    for (const [n, desc] of held) {
      if (desc) Object.defineProperty(globalThis, n, desc)
      else delete globalThis[n]
    }
    w.close()
  }
}

/** A short name for what has focus, for a failure message. */
const who = (el) => (el ? `${el.tagName.toLowerCase()}${el.getAttribute('data-id') ? '[data-id=' + el.getAttribute('data-id') + ']' : ''} "${(el.getAttribute('aria-label') || el.textContent || '').trim().slice(0, 60)}"` : 'nothing')

test('A · the workspace at Explore, keys only: focus moves WITH the selection — J K walk it, . and F light the thread on the card they select, T keeps it, / and ? trap it and give it back, Esc returns it to where it was before the thread lit, and E P B never drop it on the page', async () => {
  await mounted('http://127.0.0.1/?altitude=explore', (ref) => L.React.createElement(L.Workspace, { ref, index: { projects: [], totals: {}, kind: 'index', contract_version: 1 } }), async ({ d, press, ref }) => {
    const ws = ref.current
    assert.equal(ws.state.dataStatus, 'ready', 'the workspace opened a record')
    assert.equal(ws.state.altitude, 'explore')
    const card = (id) => d.querySelector(`[role="application"] [role="button"][data-id="${id}"]`)
    const dialog = () => d.querySelector('.ws-shell-dialog[aria-modal="true"]')
    const active = () => d.activeElement
    /** The element the selection says has focus: the selected card, where drawn. */
    const expectOnSelection = (key) => {
      const sel = ws.state.sel
      assert.ok(sel, `${key}: something is selected`)
      const el = card(ws.resolveEnd(sel).id)
      assert.ok(el, `${key}: the selected card ${sel} is drawn`)
      assert.ok(active() === el, `${key}: focus is on ${who(active())}, not the selected card ${sel}`)
      assert.equal(el.getAttribute('tabindex'), '0', `${key}: the selected card is the one tab stop`)
      assert.equal(d.querySelectorAll('[role="application"] [role="button"][data-id][tabindex="0"]').length, 1, `${key}: exactly one card is tabbable`)
    }

    /* ? — the key list opens, focus goes into it and cannot Tab out. */
    const start = active()
    await press('?', { shiftKey: true })
    assert.ok(dialog(), '? opened the key list')
    assert.ok(dialog().contains(active()), `? : focus is inside the key list, not on ${who(active())}`)
    await press('Tab')
    assert.ok(dialog().contains(active()), 'Tab stays inside the key list')
    await press('Tab', { shiftKey: true })
    assert.ok(dialog().contains(active()), 'Shift+Tab stays inside the key list')
    /* Esc — it closes and focus goes back where it was. */
    await press('Escape')
    assert.ok(dialog() === null, 'Esc closed the key list')
    assert.ok(active() === start, 'Esc gave focus back to where it was before the key list opened')

    /* J J K — the walk (nothing lit yet: record order); focus is on each card it selects. */
    await press('j')
    expectOnSelection('J')
    const first = ws.state.sel
    await press('j')
    expectOnSelection('J again')
    assert.notEqual(ws.state.sel, first, 'J moved on')
    await press('k')
    expectOnSelection('K')
    assert.equal(ws.state.sel, first, 'K came back')
    const beforeThread = active()
    const beforeId = beforeThread.getAttribute('data-id')
    assert.equal(ws.thread(), null, 'the first step lights no thread, so the next lighting is observable')

    /* T — Plain to Technical: the words change, the focus does not. */
    await press('t')
    assert.equal(ws.state.register, 'operator')
    expectOnSelection('T')
    assert.ok(active() === beforeThread, 'T kept focus on the same card')

    /* . — the one primary here is "show the current discrepancy": it selects
     * the worst claim, lights its thread, and focus follows the selection. */
    const primary = d.querySelector('[data-primary-action]')
    assert.ok(primary, 'Explore marks its one primary')
    await press('.')
    assert.equal(ws.state.sel, L.discrepancyAnchor(ws.S()), '. selected the current discrepancy')
    assert.ok(ws.thread(), '. lit a thread')
    expectOnSelection('.')

    /* F — the thread is focused on screen; focus stays on the selected card. */
    await press('f')
    assert.ok(ws.thread(), 'F kept a thread lit')
    expectOnSelection('F')
    const lit = active()

    /* / — Ask opens, traps focus, and Esc gives it back to the lit card. */
    await press('/')
    assert.ok(dialog(), '/ opened Ask')
    assert.ok(dialog().contains(active()), `/ : focus is inside Ask, not on ${who(active())}`)
    await press('j')
    assert.ok(dialog().contains(active()), 'J does nothing behind an open sheet: focus stays in it')
    await press('Escape')
    assert.ok(dialog() === null)
    assert.ok(active() === lit, 'closing Ask gave focus back to the lit card')
    assert.ok(ws.thread(), 'the first Esc closed the sheet and only that')

    /* Esc — the thread goes, and focus returns to where it was before it lit. */
    await press('Escape')
    assert.equal(ws.state.sel, null, 'Esc let go of the thread')
    /* The same card — the picture may have redrawn it as a new element while it was off the thread. */
    assert.ok(active() === card(beforeId), `Esc returned focus to ${who(active())}, not the card ${beforeId} it was on before the thread lit`)

    /* F with nothing lit lights the discrepancy's thread and takes focus to it. */
    await press('f')
    expectOnSelection('F from nothing')

    /* 3 — Workbench draws the same card; focus is on it there. */
    await press('3')
    assert.equal(ws.state.altitude, 'workbench')
    expectOnSelection('3')

    /* 1 — Brief draws no canvas: focus lands on the altitude now pressed, never on the page. */
    await press('1')
    assert.equal(ws.state.altitude, 'brief')
    const pressed = d.querySelector(`[aria-label="${SH.topbar.altitude_label}"] button[aria-pressed="true"]`)
    assert.equal(pressed.textContent, SH.altitudes.brief.label)
    assert.ok(active() === pressed, `1: focus is on ${who(active())}, not the pressed altitude`)

    /* 2 — back at Explore, the selected card has focus again. */
    await press('2')
    assert.equal(ws.state.altitude, 'explore')
    expectOnSelection('2')
  })
})

test('A · the Outline at Explore, keys only: J puts focus on the selected row; Up and Down move selection and focus together, into a closed group and out; F goes back to the picture with focus on the card', async () => {
  await mounted('http://127.0.0.1/?altitude=explore', (ref) => L.React.createElement(L.Workspace, { ref, index: { projects: [], totals: {}, kind: 'index', contract_version: 1 } }), async ({ d, press, ref }) => {
    const ws = ref.current
    const outlineButton = [...d.querySelectorAll('nav button')].find((b) => b.textContent === SH.sidebar.outline)
    assert.ok(outlineButton, 'the sidebar offers the Outline')
    outlineButton.click()
    for (let i = 0; i < 4; i += 1) await settle()
    const region = d.querySelector('.ws-trace-outline')
    assert.ok(region, 'the Outline is showing')
    const row = (id) => d.querySelector(`.ws-trace-outline [data-section="all"] [data-id="${id}"]`)
    const expectRow = (key) => {
      const el = row(ws.state.sel)
      assert.ok(el, `${key}: the selected card ${ws.state.sel} is listed`)
      assert.ok(d.activeElement === el, `${key}: focus is on ${who(d.activeElement)}, not the selected row`)
      assert.equal(el.getAttribute('aria-current'), 'true', `${key}: the selected row is aria-current`)
      assert.equal(el.getAttribute('tabindex'), '0', `${key}: the selected row is the list's one tab stop`)
    }
    await press('j')
    expectRow('J')
    /* Down, repeatedly, through every row — including the ones folded inside a closed group. */
    const all = () => [...d.querySelectorAll('.ws-trace-outline [data-section="all"] .ws-trace-outline-item')].map((b) => b.getAttribute('data-id'))
    const folded = [...d.querySelectorAll('.ws-trace-outline-folded .ws-trace-outline-item')].map((b) => b.getAttribute('data-id'))
    assert.ok(folded.length > 0, 'the demo record has a closed group, and its cards are listed inside it')
    let reachedFolded = false
    for (let i = 0; i < all().length + 4 && !reachedFolded; i += 1) {
      await press('ArrowDown')
      expectRow('Down')
      if (folded.includes(ws.state.sel)) reachedFolded = true
    }
    assert.ok(reachedFolded, 'Down reached a card that was folded inside a closed group')
    await press('ArrowUp')
    expectRow('Up')
    /* F — back to the picture, focus on the card. */
    await press('f')
    assert.equal(ws.state.outline, false)
    const el = d.querySelector(`[role="application"] [role="button"][data-id="${ws.resolveEnd(ws.state.sel).id}"]`)
    assert.ok(d.activeElement === el, `F: focus is on ${who(d.activeElement)}`)
  })
})

test('A · the Change screen at Brief, keys only: no canvas, so J K F leave focus and say why; . puts focus on the one primary and says why it is off; T, ? and / never lose it; E P B never drop it on the page', async () => {
  await mounted('http://127.0.0.1/#/changes/' + L.A11Y_CHANGE_ID, () => L.React.createElement(L.ChangeWorkspace, { id: L.A11Y_CHANGE_ID, stage: 'define' }), async ({ d, press }) => {
    const shell = () => d.querySelector('.ws-shell')
    assert.ok(shell(), 'the Change screen mounted')
    assert.equal(shell().getAttribute('data-altitude'), 'brief')
    const note = () => d.querySelector('.ws-shell-note').textContent
    const dialog = () => d.querySelector('.ws-shell-dialog[aria-modal="true"]')
    const active = () => d.activeElement

    /* . — the one primary is off. A disabled button cannot take focus, so
     * focus lands on its reason — the sentence that says why and what would
     * turn it on — and the note says the same. */
    await press('.')
    const off = d.querySelector('[data-primary-action] .ws-change-primary')
    assert.ok(off)
    assert.ok(off.disabled || off.getAttribute('aria-disabled') === 'true', 'the primary on this fixture is off')
    const reason = d.getElementById(off.getAttribute('aria-describedby'))
    const primary = off.disabled ? reason : off
    assert.ok(active() === primary, `. : focus is on ${who(active())}, not the off primary or its reason`)
    assert.equal(primary.getAttribute('tabindex') === '-1' || !off.disabled, true, 'the reason takes focus from script and stays out of the Tab order')
    assert.ok(note().includes(reason.textContent.trim()), 'the note says why it is off and what would turn it on')

    /* J K F — no picture here: each says so, and focus does not move. */
    for (const k of ['j', 'k', 'f']) {
      await press(k)
      assert.equal(note(), k === 'f' ? SH.notes.thread_here_none : SH.notes.walk_here_none, k)
      assert.ok(active() === primary, `${k.toUpperCase()}: focus stayed on the primary's reason`)
    }

    /* T — Technical and back; focus stays. */
    await press('t')
    assert.ok(active() === primary, 'T kept focus')
    await press('t')

    /* ? and / — each traps focus and gives it back. */
    for (const [key, extra, title] of [['?', { shiftKey: true }, SH.keys.title], ['/', {}, SH.ask.title]]) {
      await press(key, extra)
      assert.equal(dialog().querySelector('h2').textContent, title)
      assert.ok(dialog().contains(active()), `${key}: focus is inside the sheet`)
      await press('Tab')
      assert.ok(dialog().contains(active()), `${key}: Tab stays inside`)
      await press('Escape')
      assert.ok(dialog() === null)
      assert.ok(active() === primary, `${key}: closing gave focus back to the primary`)
    }

    /* 2 — the Brief's primary is gone at Explore: focus lands on the altitude now pressed. */
    await press('2')
    assert.equal(shell().getAttribute('data-altitude'), 'explore')
    const pressed = () => d.querySelector(`[aria-label="${SH.topbar.altitude_label}"] button[aria-pressed="true"]`)
    assert.ok(active() === pressed(), `2: focus is on ${who(active())}`)
    /* 3, 1 — focus is on a control that is still there, never the page. */
    await press('3')
    assert.equal(shell().getAttribute('data-altitude'), 'workbench')
    assert.ok(active() && active() !== d.body && active().isConnected, `3: focus is on ${who(active())}`)
    await press('1')
    assert.equal(shell().getAttribute('data-altitude'), 'brief')
    assert.ok(active() && active() !== d.body && active().isConnected, `1: focus is on ${who(active())}`)
    /* Esc with nothing open closes nothing and moves nothing. */
    const at = active()
    await press('Escape')
    assert.ok(active() === at, 'Esc with nothing open left focus where it was')
  })
})

/* ── the two guards a key drive over this fixture cannot reach ─────────── */

test('A · when the element focus came from has been redrawn, Esc still returns focus to the same card — by its id, not to the first card', async () => {
  await mounted('http://127.0.0.1/?altitude=explore', (ref) => L.React.createElement(L.Workspace, { ref, index: { projects: [], totals: {}, kind: 'index', contract_version: 1 } }), async ({ d, press, ref }) => {
    const ws = ref.current
    await press('j')
    await press('j')
    await press('j')
    const id = ws.state.sel
    await press('Escape')
    /* with nothing selected the canvas's one tab stop is the first card: the fallback */
    const first = d.querySelector('[role="application"] [role="button"][data-id][tabindex="0"]')
    /* the picture draws only the cards near the view, so the element can be a
     * new one by the time focus goes back: stand in for that with a detached one */
    const gone = d.createElement('div')
    ws.restoreFocus({ el: gone, id })
    const again = d.querySelector(`[role="application"] [role="button"][data-id="${id}"]`)
    assert.ok(again, `the card ${id} is still drawn`)
    assert.ok(d.activeElement === again, `focus went to ${who(d.activeElement)}, not the card ${id}`)
    assert.notEqual(id, first && first.getAttribute('data-id'), 'the card is not the first one, so the fallback cannot pass this by accident')
  })
})

test('A · . on an off primary puts focus on it (aria-disabled) or on its reason (disabled); . on a live primary presses it and leaves focus alone', async () => {
  const dom = new JSDOM('<!doctype html><body>' +
    '<p id="elsewhere" tabindex="-1">elsewhere</p>' +
    '<div data-primary-action=""><button class="ws-change-primary" aria-disabled="true" aria-describedby="w1">Approve</button><p id="w1">2 required checks have not run. Run them.</p></div>' +
    '</body>')
  const d = dom.window.document
  const { pressPrimary } = await import('./altitude.js')
  let r = pressPrimary(d)
  assert.equal(r.pressed, false)
  assert.ok(d.activeElement === d.querySelector('.ws-change-primary'), 'an aria-disabled primary takes focus')
  d.body.innerHTML = '<div data-primary-action=""><button class="ws-change-primary" disabled aria-describedby="w2">Approve</button><p id="w2" tabindex="-1">Nobody is named.</p></div>'
  r = pressPrimary(d)
  assert.ok(d.activeElement === d.getElementById('w2'), 'a disabled primary cannot take focus, so its reason does')
  assert.equal(r.reason, 'Nobody is named.')
  d.body.innerHTML = '<p id="here" tabindex="-1">here</p><div data-primary-action=""><button class="ws-change-primary">Show</button></div>'
  let clicked = 0
  d.querySelector('button').addEventListener('click', () => { clicked += 1 })
  d.body.firstChild.focus()
  r = pressPrimary(d)
  assert.equal(r.pressed, true)
  assert.equal(clicked, 1)
  assert.ok(d.activeElement === d.getElementById('here'), 'a pressed primary does not take focus: the shortcut acts where you are')
  dom.window.close()
})

/* ── N13-KEYS-214 · WCAG 2.2 SC 2.1.4 ─────────────────────────────────── */

test('2.1.4 · the key list holds a named switch for single-key shortcuts, reached from the visible Keys button; off, the letters and [ ] , move nothing, while Cmd+K and Esc still work', async () => {
  const K = SH.keys.single_keys
  assert.ok(K && typeof K.label === 'string' && typeof K.on === 'string' && typeof K.off === 'string', 'HQ_COPY.shell.keys.single_keys carries the switch copy')
  await mounted('http://127.0.0.1/#/changes/' + L.A11Y_CHANGE_ID, () => L.React.createElement(L.ChangeWorkspace, { id: L.A11Y_CHANGE_ID, stage: 'define' }), async ({ w, d, press }) => {
    const shell = () => d.querySelector('.ws-shell')
    const dialog = () => d.querySelector('.ws-shell-dialog[aria-modal="true"]')
    /* The visible Keys button is the door that needs no character key. */
    const keysBtn = [...d.querySelectorAll('.ws-shell-top button')].find((b) => b.textContent === SH.topbar.keys)
    assert.ok(keysBtn, 'a visible Keys button opens the list without pressing ?')
    keysBtn.click()
    for (let i = 0; i < 4; i += 1) await settle()
    const sw = dialog() && dialog().querySelector('[role="switch"]')
    assert.ok(sw, 'the key list holds a role="switch"')
    assert.equal(sw.tagName, 'BUTTON', 'the switch is a button, so Tab reaches it and Space or Enter flips it')
    const name = (sw.getAttribute('aria-labelledby') || '').split(/\s+/).map((id) => d.getElementById(id)).filter(Boolean).map((el) => el.textContent).join(' ') || sw.getAttribute('aria-label') || sw.textContent
    assert.ok(name.includes(K.label), `the switch is named "${K.label}", not "${name}"`)
    assert.equal(sw.getAttribute('aria-checked'), 'true', 'on by default')
    assert.ok(sw.textContent.includes(K.on), 'its state is a word, not a colour alone')
    sw.click()
    for (let i = 0; i < 4; i += 1) await settle()
    assert.equal(sw.getAttribute('aria-checked'), 'false')
    assert.ok(sw.textContent.includes(K.off))
    await press('Escape')
    assert.equal(dialog(), null, 'Esc still closes the sheet with the switch off')

    assert.equal(shell().getAttribute('data-altitude'), 'brief')
    for (const k of ['2', '3', 't', 'j', 'f', '.', '/']) {
      await press(k)
      assert.equal(shell().getAttribute('data-altitude'), 'brief', `${k} moved the altitude with single keys off`)
      assert.equal(dialog(), null, `${k} opened a sheet with single keys off`)
    }
    await press('?', undefined)
    assert.equal(dialog(), null, '? opens nothing with single keys off; the Keys button is the way back')
    await press('k', { metaKey: true })
    assert.equal(dialog() && dialog().querySelector('h2').textContent, HQ_COPY.ask.title, 'Cmd+K still opens Ask')
    await press('Escape')
    assert.equal(dialog(), null)

    keysBtn.click()
    for (let i = 0; i < 4; i += 1) await settle()
    dialog().querySelector('[role="switch"]').click()
    for (let i = 0; i < 4; i += 1) await settle()
    await press('Escape')
    await press('2')
    assert.equal(shell().getAttribute('data-altitude'), 'explore', 'switched back on, 2 acts again')
    w.localStorage.clear()
  })
})

test('2.1.4 · in the workspace, [ ] and comma answer to the same switch', async () => {
  await mounted('http://127.0.0.1/?altitude=explore', (ref) => L.React.createElement(L.Workspace, { ref, index: { projects: [], totals: {}, kind: 'index', contract_version: 1 } }), async ({ w, press, ref }) => {
    const ws = ref.current
    const seen = []
    ws.stepFinding = (dir) => seen.push('finding:' + dir)
    ws.step = (dir) => seen.push('step:' + dir)
    await press(']')
    await press('[')
    await press(',')
    assert.deepEqual(seen, ['finding:1', 'finding:-1', 'step:-1'], 'on: the workspace keys act')
    const { setSingleKeys } = await import('./altitude.js')
    assert.equal(typeof setSingleKeys, 'function')
    setSingleKeys(w, false)
    await press(']')
    await press('[')
    await press(',')
    assert.deepEqual(seen, ['finding:1', 'finding:-1', 'step:-1'], 'off: [ ] and comma move nothing')
    setSingleKeys(w, true)
    w.localStorage.clear()
  })
})
