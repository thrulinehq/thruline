/* T-6-6 · A — every empty state teaches (03-USER-LENS.md rule 12; design
 * record §6, "the empty Inbox teaches").
 *
 * The family `HQ_COPY.empty` holds one entry per empty state a person can meet,
 * each in four parts: what this place is, why it is empty, the ONE thing that
 * would fill it, and the label of the control that does it. This file
 * enumerates that family and renders every state with the REAL component — the
 * Inbox, the way-in strip, the workspace's Trace (mounted in jsdom over a
 * served session that holds nothing), the first-use pick and proposal steps,
 * and the Change screen's Brief — then reads the rendered DOM and asserts:
 *
 *   - all three sentences are on screen (what, why, fill);
 *   - the action is on screen, as a control with exactly that label;
 *   - exactly one PRIMARY control exists on that screen.
 *
 * A family entry with no screen fails here, and a screen that drops a part
 * fails here. Every fixture is synthetic; no string came off a real machine.
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
const temporary = await mkdtemp(join(tmpdir(), 'thruline-empty-states-'))
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
      import { createRoot } from 'react-dom/client';
      import { flushSync } from 'react-dom';
      import Workspace from './shell.jsx';
      import { InboxScreen } from './inbox.jsx';
      import { ChangeWayIn } from './change-way-in.jsx';
      import { FirstUseScreen } from './first-use.jsx';
      import { ChangeShell, ChangeScreen } from './change-workspace.jsx';
      import { buildChangeView, STAGES } from './change-view.js';
      import { deriveChangeModel } from './change-model.js';
      import { candidateRows } from './first-use-view.js';
      import { chooseScreen } from '../routes.js';
      export { React, createRoot, flushSync, Workspace, buildChangeView, deriveChangeModel, candidateRows, STAGES, chooseScreen };
      export const html = (C, props) => renderToStaticMarkup(React.createElement(C, props));
      export const C = { InboxScreen, ChangeWayIn, FirstUseScreen, ChangeShell, ChangeScreen };
    `,
  },
})
const file = join(temporary, 'empty.mjs')
await writeFile(file, result.outputFiles[0].text)
const L = await import(pathToFileURL(file).href)

const E = HQ_COPY.empty
const squash = (s) => String(s || '').replace(/\s+/g, ' ').trim()
const dom = (markup) => new JSDOM('<!doctype html><body>' + markup + '</body>').window.document

/* ── synthetic records ─────────────────────────────────────────────────── */

const CHANGE_ID = 'eeeeeeee-0000-4000-8000-00000000e001'
const bareChange = () => ({
  id: CHANGE_ID,
  project_id: 'eeeeeeee-0000-4000-8000-00000000p001',
  opened_at: '2026-01-02T09:00:00Z',
  request: { title: 'Retry failed uploads', purpose: 'Uploads that fail once are lost.', requester: 'sam', source: 'person' },
  accepted_scope: null,
  implementation: null,
  associations: [],
  examination: 'none_yet',
  decision: 'none_yet',
})
const changeModel = () => {
  const change = bareChange()
  const model = L.buildChangeView({ change, work: [] })
  return { ...model, hq: L.deriveChangeModel(change, null, null, null, []) }
}

/* ── the workspace, mounted over a served session that holds nothing ───── */

const SESSION = 'empty-session-1'
const ROUTES = {
  '/api/sessions': { shape: 'thruline.sessions', schema_version: 1, total: 1, sessions: [{ id: SESSION, repo_root: '/work/uploads', event_count: 0 }] },
  '/api/explain': { shape: 'thruline.explain', schema_version: 1, session: SESSION, what_happened: [], claims_vs_proof: [], promises: [], risk_flags: [] },
  '/api/claims': { shape: 'thruline.claims', schema_version: 1, session: SESSION, claims: [] },
}
async function emptyFetch(url) {
  const path = String(url).replace(/^https?:\/\/[^/]+/, '').split('?')[0]
  const doc = Object.prototype.hasOwnProperty.call(ROUTES, path) ? ROUTES[path] : null
  return doc
    ? { ok: true, status: 200, statusText: 'OK', headers: { get: () => null }, text: async () => JSON.stringify(doc), json: async () => doc }
    : { ok: false, status: 404, statusText: 'Not Found', headers: { get: () => null }, text: async () => '{}', json: async () => ({}) }
}
const GLOBALS = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage', 'CSS', 'fetch']
const settle = () => new Promise((r) => setTimeout(r, 10))

/** The real Workspace at Explore, in jsdom, reading the empty session above. */
export async function withEmptyTrace(run) {
  const jsdom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/?altitude=explore', pretendToBeVisual: true })
  const w = jsdom.window
  const held = GLOBALS.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of GLOBALS) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  set('fetch', emptyFetch)
  const root = L.createRoot(w.document.getElementById('root'))
  try {
    const ref = L.React.createRef()
    L.flushSync(() => root.render(L.React.createElement(L.Workspace, { ref, index: { projects: [], totals: {}, kind: 'index', contract_version: 1 } })))
    for (let i = 0; i < 60; i += 1) await settle()
    return await run({ w, d: w.document, ws: ref.current })
  } finally {
    root.unmount()
    for (const [n, desc] of held) {
      if (desc) Object.defineProperty(globalThis, n, desc)
      else delete globalThis[n]
    }
    w.close()
  }
}

/* ── one renderer per empty state ──────────────────────────────────────── *
 * Each returns the rendered document, the element the state lives in, the
 * selector of that screen's primary class, and (for the Trace) a way to press
 * the action. */

const SCREENS = {
  inbox: async () => {
    const d = dom(L.html(L.C.InboxScreen, { state: { status: 'ready', items: [], settled: 0, total: 0, unread: 0, newest: null, storage: { known: false }, notes: [] } }))
    return { d, box: d.querySelector('[data-empty="inbox"]'), primary: '.ws-inbox-primary' }
  },
  change_list: async () => {
    const d = dom(L.html(L.C.ChangeWayIn, { state: { status: 'ready', rows: [], more: 0 } }))
    return { d, box: d.querySelector('[data-empty="change_list"]'), primary: '.ws-wayin-primary' }
  },
  trace: async () => withEmptyTrace(async ({ d, ws }) => {
    assert.equal(ws.state.dataStatus, 'ready', 'the workspace read the served session')
    assert.equal(ws.state.isDemo, false, 'the served (empty) session opened, not the bundled demo')
    const canvas = d.querySelector('[role="application"]')
    assert.ok(canvas, 'the Trace canvas is drawn at Explore')
    const box = canvas.querySelector('[data-empty="absence"]')
    /* `.` presses the first `[data-primary-action]` (altitude.js pressPrimary):
     * on an empty Trace that is the empty state's own action. */
    const first = d.querySelector('[data-primary-action]')
    assert.equal(squash(first && first.textContent), E.trace.action, 'on an empty Trace, . would press something other than its one fill')
    let opened = null
    if (box) {
      const btn = [...box.querySelectorAll('button')].find((b) => squash(b.textContent) === E.trace.action)
      if (btn) {
        btn.dispatchEvent(new d.defaultView.MouseEvent('click', { bubbles: true }))
        for (let i = 0; i < 4; i += 1) await settle()
        opened = ws.state.menu
      }
    }
    /* Snapshot what the assertions need before the window closes. The whole
     * workspace is the screen here, less the way-in strip above it, which is
     * its own empty state (`change_list`) and is tested as one. The toolbar's
     * primary carries no class, so a primary is anything `.` would press
     * (`data-primary-action`) or anything painted as one. */
    const strip = d.querySelector('.ws-wayin')
    if (strip) strip.remove()
    const snap = dom(d.body.innerHTML)
    return { d: snap, box: snap.querySelector('[data-empty="absence"]'), primary: '[data-primary-action], .ws-btn-primary, .ws-verdict-primary, .ws-checks-primary, .ws-change-primary', opened }
  }),
  no_project: async () => {
    const overview = { schema_version: 1, registered: 0, store: { folder: '/tmp/fixture-home/.thruline', bytes: 0 }, candidates: [], codex_recordings: 0, notes: [] }
    const d = dom(L.html(L.C.FirstUseScreen, { state: { status: 'ready', step: 'pick', selected: null, ...L.candidateRows(overview), storage: { known: false } } }))
    return { d, box: d.querySelector('[data-empty="no_project"]'), primary: '.ws-inbox-primary' }
  },
  no_recordings: async () => {
    const d = dom(L.html(L.C.FirstUseScreen, { state: { status: 'ready', step: 'propose', project: 'p1', proposal: { have: false, project: 'p1' } } }))
    return { d, box: d.querySelector('[data-empty="no_recordings"]'), primary: '.ws-inbox-primary' }
  },
  change_unlinked: async () => {
    const d = dom(L.html(L.C.ChangeShell, { model: changeModel(), stage: 'define', altitude: 'brief', register: 'manager' }))
    return { d, box: d.querySelector('[data-empty="change_unlinked"]'), primary: '.ws-change-primary, .ws-verdict-primary' }
  },
}

/* ── the family itself ─────────────────────────────────────────────────── */

test('A · the empty family is complete: every entry has a screen, every screen has an entry, and each has all four parts', () => {
  assert.deepEqual(Object.keys(E).sort(), Object.keys(SCREENS).sort(), 'HQ_COPY.empty and the screens this test renders are not the same set')
  assert.ok(Object.keys(E).length >= 6, 'the six empty states the card names are all here')
  for (const [state, entry] of Object.entries(E)) {
    for (const part of ['what', 'why', 'fill', 'action']) {
      assert.equal(typeof entry[part], 'string', `empty.${state}.${part} is missing`)
      assert.ok(entry[part].trim().length > (part === 'action' ? 3 : 20), `empty.${state}.${part} says nothing: ${entry[part]}`)
    }
  }
})

test('A · no empty state says "setup required", accuses, or uses a word Sam has never heard', () => {
  const PLAIN_BANNED = [/\bevents?\b/i, /\breceipts?\b/i, /\bcandidates?\b/i, /\bapplicability\b/i, /\bdispositions?\b/i, /\bpacket\b/i, /\bledger\b/i, /\bsession id\b/i, /\btree hash\b/i, /\bpredicates?\b/i, /setup required/i]
  const banned = HQ_COPY.banned.map((w) => new RegExp('\\b' + w + '\\b', 'i'))
  for (const [state, entry] of Object.entries(E)) {
    for (const [part, text] of Object.entries(entry)) {
      for (const re of [...PLAIN_BANNED, ...banned]) assert.doesNotMatch(text, re, `empty.${state}.${part}: ${text}`)
    }
  }
})

for (const state of Object.keys(SCREENS)) {
  test(`A · the ${state} empty state renders what it is, why it is empty, the one thing that fills it, and exactly one primary action`, async () => {
    const entry = E[state]
    assert.ok(entry, `no HQ_COPY.empty.${state}`)
    const { d, box, primary, opened } = await SCREENS[state]()
    assert.ok(box, `${state}: the empty state is not on screen at all`)
    const said = squash(box.textContent)
    assert.ok(said.includes(squash(entry.what)), `${state}: WHAT is missing — "${entry.what}" not in: ${said}`)
    assert.ok(said.includes(squash(entry.why)), `${state}: WHY is missing — "${entry.why}" not in: ${said}`)
    assert.ok(said.includes(squash(entry.fill)), `${state}: FILL is missing — "${entry.fill}" not in: ${said}`)
    /* The action is a control with exactly that label. */
    const controls = [...box.querySelectorAll('a[href], button')].filter((el) => squash(el.textContent) === entry.action)
    assert.equal(controls.length, 1, `${state}: the action "${entry.action}" is not one control in the empty state`)
    assert.ok(!controls[0].hasAttribute('disabled'), `${state}: the one thing that fills it is switched off`)
    /* Exactly one primary on the whole screen. */
    const primaries = d.querySelectorAll(primary)
    assert.equal(primaries.length, 1, `${state}: ${primaries.length} primary controls on screen`)
    if (entry.command) {
      assert.ok(said.includes(entry.command), `${state}: the command the fill names is not written out: ${entry.command}`)
    }
    if (state === 'trace') assert.equal(opened, 'session', 'the Trace\'s action did not open the session list')
  })
}

/* The two actions that are links go where they say, and nowhere that loops. */
test('A · the Inbox\'s and the way-in\'s action open the pick screen, which is the first-use route', async () => {
  for (const state of ['inbox', 'change_list']) {
    const { box } = await SCREENS[state]()
    const a = [...box.querySelectorAll('a[href]')].find((el) => squash(el.textContent) === E[state].action)
    assert.equal(a.getAttribute('href'), '#/start', `${state}: the action does not open the pick screen`)
  }
  assert.deepEqual(L.chooseScreen({ pathname: '/', search: '', hash: '#/start' }), { screen: 'first-use' })
})

/* The Change's primary is the Inspector's; the fill is the exact command. */
test('A · a Change with no linked work names the exact command that links some, with this Change in it', async () => {
  const { box } = await SCREENS.change_unlinked()
  const code = squash(box.querySelector('code') && box.querySelector('code').textContent)
  assert.ok(code.startsWith('thruline check --for-change ' + CHANGE_ID + ' -- '), `the command is not the run-a-check command for this Change: ${code}`)
})

/* ── the Change screen's six stages, on a Change with no linked work ─────
 * The card: "the Change screen with no linked work (`changes.*.empty` rows
 * exist — verify each)". Each stage: a heading (what), its empty sentence
 * (why), and a way to fill it — an enabled primary, or an off primary whose
 * reason names an act, or a command written out. */
test('A · every stage of a Change with no linked work says what it is, why it is empty and how it fills', () => {
  const S = HQ_COPY.change.stages
  const why = { define: S.define.empty, prepare: S.prepare.empty, build: HQ_COPY.change.build.none, verify: S.verify.empty, review: S.review.empty, approve: S.approve.empty }
  for (const stage of L.STAGES) {
    const d = dom(L.html(L.C.ChangeScreen, { model: changeModel(), stage }))
    const panel = d.querySelector('.ws-change-panel')
    const text = squash(panel.textContent)
    assert.ok(panel.querySelector('h2'), `${stage}: no heading says what this stage is`)
    assert.ok(text.includes(squash(why[stage])), `${stage}: its empty sentence is not on screen: ${why[stage]}`)
    const live = [...panel.querySelectorAll('.ws-change-primary')].filter((b) => !b.hasAttribute('disabled'))
    const off = [...panel.querySelectorAll('.ws-change-primary[disabled]')].map((b) => {
      const id = b.getAttribute('aria-describedby')
      return id ? squash(d.getElementById(id).textContent) : ''
    })
    const offWithAct = off.filter((w) => HQ_COPY.enablers.some((p) => w.toLowerCase().includes(p.toLowerCase())))
    const command = panel.querySelector('code')
    assert.ok(live.length || offWithAct.length || command, `${stage}: nothing on this stage says how it would fill`)
    /* A sentence that points at "the command above" has one above it. */
    for (const p of panel.querySelectorAll('p')) {
      if (!/the command above/.test(p.textContent)) continue
      const before = [...panel.querySelectorAll('code')].filter((c) => c.compareDocumentPosition(p) & 4)
      assert.ok(before.length, `${stage}: "${squash(p.textContent)}" points at a command above it, and there is none`)
    }
  }
  /* The one that named a control that does not exist. */
  assert.doesNotMatch(S.review.empty, /Verify stage/, 'Review sends a person to ask for a review where nothing asks for one')
  assert.doesNotMatch(HQ_COPY.change.blocked.nothing_to_accept.enabler, /in the request/, 'nothing in this build edits a request once written')
})

/* ── build item 4: Plain doors say "The whole story"; "packet" is Technical ── */
test('A · no Plain P6 surface says "packet": the door is "The whole story"', async () => {
  const brief = dom(L.html(L.C.ChangeShell, { model: changeModel(), stage: 'define', altitude: 'brief', register: 'manager' }))
  const explore = dom(L.html(L.C.ChangeShell, { model: changeModel(), stage: 'define', altitude: 'explore', register: 'manager' }))
  for (const [where, d] of [['Brief', brief], ['Explore', explore]]) {
    const plain = [...d.querySelectorAll('body *')].filter((el) => !el.closest('details')).map((el) => [...el.childNodes].filter((n) => n.nodeType === 3).map((n) => n.textContent).join(' ')).join(' ')
    const labels = [...d.querySelectorAll('[aria-label],[title]')].map((el) => (el.getAttribute('aria-label') || '') + ' ' + (el.getAttribute('title') || '')).join(' ')
    assert.doesNotMatch(plain + ' ' + labels, /\bpacket\b/i, `${where} says "packet" in Plain`)
  }
  assert.ok(squash(brief.body.textContent).includes(HQ_COPY.change.packet.door), 'the Brief\'s door to the whole story is not drawn')
  await withEmptyTrace(async ({ d }) => {
    const plain = [...d.querySelectorAll('body *')].filter((el) => !el.closest('details')).map((el) => [...el.childNodes].filter((n) => n.nodeType === 3).map((n) => n.textContent).join(' ')).join(' ')
    assert.doesNotMatch(plain, /\bpacket\b/i, 'the workspace at Explore says "packet" in Plain')
  })
})
