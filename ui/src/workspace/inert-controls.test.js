import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, readdir, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'
import { actionableControls, census, inertControls, observeClick, controlName, NOOP_HANDLER_PROP, SYNTHETIC } from './control-census.js'
import { A11Y_CHANGE_ROUTES, A11Y_CHANGE_ID } from './a11y-fixture.js'

/* A3-F10 — dead controls. "A control census reports 0 inert controls."
 *
 * An INERT control is an enabled `button`, `[role=button]`, `input` or
 * `select` that (i) has no React click or keyboard handler and is not a
 * submit button inside a form, or (ii) has a handler that changes nothing a
 * person can observe: no DOM change, no request, no clipboard write, no
 * navigation, no focus move. `a[href]` and `summary` are live by construction.
 *
 * The real route components are mounted in jsdom — the session workspace at
 * all three altitudes, the Change screen at 3 altitudes × 6 stages over two
 * Changes (one of them with finding cards), the whole story on one page, the
 * Inbox and first use — each answering its reads from synthetic documents.
 * Part (i) is read off every screen's React props. Part (ii) CLICKS every
 * enabled button and `[role=button]`, one at a time, and watches for an
 * effect; a screen re-mounts whenever an earlier click took a later control
 * off the glass. Every value here is synthetic.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-inert-'))
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
      import PacketView from './packet.jsx';
      import Inbox from './inbox.jsx';
      import FirstUse from './first-use.jsx';
      /* The app's own root wraps every screen in an error boundary (main.jsx);
       * so does this, and it says what it caught instead of drawing it. */
      export class Boundary extends React.Component {
        constructor(p) { super(p); this.state = { err: null } }
        static getDerivedStateFromError(err) { return { err } }
        componentDidCatch(err) { this.props.crashes.push(String(err && err.message || err)) }
        render() { return this.state.err ? React.createElement('pre', null, 'render failed') : this.props.children }
      }
      export { React, createRoot, flushSync, Workspace, ChangeWorkspace, PacketView, Inbox, FirstUse };
    `,
  },
})
const file = join(temporary, 'inert.mjs')
await writeFile(file, result.outputFiles[0].text)
const L = await import(pathToFileURL(file).href)

/* ── the synthetic record every screen reads ─────────────────────────── */

const FOUND = 'ch2'
const FOUND_SESSION = 'sf1'
const FOUND_CHANGE = {
  id: FOUND, project_id: 'p1', opened_at: '2026-09-21T09:00:00Z',
  request: { title: 'Retry failed uploads' },
  implementation: { snapshot_id: 'b0c1d2e3f40', bound_at: '2026-09-21T10:00:00Z', repository_identity: 'uploads' },
  accepted_scope: {
    version: 1, criteria: ['A failed upload is tried again three times.'],
    allowed_scope: [], check_profile: [], acceptance: { actor: 'sam', accepted_at: '2026-09-21T09:30:00Z' },
  },
  associations: [{ id: 'a1', session_id: FOUND_SESSION, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-09-21T10:01:00Z', reason: null }],
  examination: 'none_yet',
}
/* A Change whose record holds a failure: its one primary is "Look at what broke". */
const BROKE = 'ch3'
const BROKE_CHANGE = { ...FOUND_CHANGE, id: BROKE, request: { title: 'Totals add up' }, associations: [] }
const BROKE_COVERAGE = {
  scope_version: 1, link_rows: 1,
  criteria: [{ index: 1, text: FOUND_CHANGE.accepted_scope.criteria[0], state: 'evidence', items: [{
    link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run', name: 'tests pass',
    observed: 'not_proven', bears: true, applies: true, applies_slug: 'inputs_applies', failure_line: 'FAILED retry: expected 3 attempts, saw 1',
  }] }],
}
const ROUTES = {
  ...A11Y_CHANGE_ROUTES,
  ['/api/changes/' + BROKE]: { change: BROKE_CHANGE },
  ['/api/changes/' + BROKE + '/proposals']: { proposals: null },
  ['/api/changes/' + BROKE + '/coverage']: { coverage: BROKE_COVERAGE },
  ['/api/changes/' + BROKE + '/inputs']: {},
  ['/api/changes/' + BROKE + '/applicability']: { items: [] },
  '/api/changes': { changes: [{ id: A11Y_CHANGE_ID, opened_at: '2026-09-22T09:00:00Z' }, { id: FOUND, opened_at: '2026-09-21T09:00:00Z' }] },
  ['/api/changes/' + FOUND]: { change: FOUND_CHANGE },
  ['/api/changes/' + FOUND + '/proposals']: { proposals: null },
  ['/api/changes/' + FOUND + '/coverage']: { coverage: { scope_version: 1, link_rows: 0, criteria: [{ index: 1, text: FOUND_CHANGE.accepted_scope.criteria[0], state: 'none', items: [] }] } },
  ['/api/changes/' + FOUND + '/inputs']: {},
  ['/api/changes/' + FOUND + '/applicability']: { items: [] },
  ['/api/explain?session=' + FOUND_SESSION]: { ...SYNTHETIC.explain, session: FOUND_SESSION },
  ['/api/claims?session=' + FOUND_SESSION]: {
    ...SYNTHETIC.claims, session: FOUND_SESSION,
    claims: [{ id: 'c1', text: 'All tests pass.', verdict: 'unsupported', findings: [{ detector: 'test_run_failed', severity: 'warn', detail: 'A test failed.' }] }],
  },
  '/api/storage': { bytes: 3174400, path: '/tmp/fixture-store', parts: [{ key: 'records', bytes: 3174400 }] },
  '/api/first-use': {
    schema_version: 1, registered: 0, store: { folder: '/tmp/fixture-home/.thruline', bytes: 897024 },
    candidates: [{
      key: 'cccccccc00000001', label: '/tmp/fixture/uploads-api', source: 'claude_code', recordings: 14, not_read: 0,
      first_recorded: '2026-01-02T09:00:00Z', last_recorded: '2026-01-09T09:00:00Z', registered: false, project: null,
    }],
    codex_recordings: 0, notes: [],
  },
}

/* ── a browser, one screen in it, and every effect a click could have ── */

const GLOBALS = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'FocusEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage', 'CSS', 'fetch', 'URL', 'Blob']
const tick = () => new Promise((r) => setTimeout(r, 2))
const OFF = 'button[disabled], input[disabled], select[disabled], [aria-disabled="true"]'

/** Wait until the screen stops changing: three quiet ticks in a row, at most 200. */
async function quiet(w) {
  let changed = 0
  const mo = new w.MutationObserver((r) => { changed += r.length })
  mo.observe(w.document, { subtree: true, childList: true, attributes: true, characterData: true })
  for (let i = 0, still = 0; i < 200 && still < 3; i += 1) {
    const before = changed
    await tick()
    still = changed === before ? still + 1 : 0
  }
  mo.disconnect()
}

async function mount(url, element) {
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url, pretendToBeVisual: true })
  const w = dom.window
  const effects = []
  const held = GLOBALS.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of GLOBALS) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  set('fetch', async (u, opts) => {
    const raw = String(u).replace(/^https?:\/\/[^/]+/, '')
    const method = (opts && opts.method) || 'GET'
    if (method !== 'GET') { effects.push('request ' + method + ' ' + raw); return new Promise(() => {}) }
    effects.push('request ' + raw)
    const path = raw.split('?')[0]
    const doc = ROUTES[raw] || ROUTES[path] || null
    return doc
      ? { ok: true, status: 200, statusText: 'OK', headers: { get: () => null }, text: async () => JSON.stringify(doc), json: async () => doc }
      : { ok: false, status: 404, statusText: 'Not Found', headers: { get: () => null }, text: async () => '{}', json: async () => ({}) }
  })
  Object.defineProperty(w.navigator, 'clipboard', { configurable: true, value: { writeText: async () => { effects.push('clipboard') } } })
  w.URL.createObjectURL = () => { effects.push('download'); return 'blob:fixture' }
  w.URL.revokeObjectURL = () => {}
  w.open = () => { effects.push('window.open'); return null }
  w.scrollTo = () => { effects.push('scroll') }
  w.HTMLElement.prototype.scrollIntoView = function () { effects.push('scroll') }
  /* React was bundled before this window existed, so on focus it watches an
   * input the old way (`attachEvent`), which jsdom does not have. */
  w.HTMLElement.prototype.attachEvent = function () {}
  w.HTMLElement.prototype.detachEvent = function () {}
  /* jsdom has no modal dialog; the one the run sheet opens is given the two
   * calls it makes, so a click that opens it is seen opening it. */
  if (w.HTMLDialogElement && !w.HTMLDialogElement.prototype.showModal) {
    w.HTMLDialogElement.prototype.showModal = function () { this.setAttribute('open', '') }
    w.HTMLDialogElement.prototype.close = function () { this.removeAttribute('open'); this.dispatchEvent(new w.Event('close')) }
  }
  const crashes = []
  const root = L.createRoot(w.document.getElementById('root'))
  L.flushSync(() => root.render(L.React.createElement(L.Boundary, { crashes }, element())))
  await quiet(w)
  effects.length = 0
  return {
    w, d: w.document, effects, crashes,
    close() {
      root.unmount()
      for (const [n, desc] of held) {
        if (desc) Object.defineProperty(globalThis, n, desc)
        else delete globalThis[n]
      }
      w.close()
    },
  }
}

const settleClick = async () => { await tick(); await quiet(globalThis.window) }

/** Each control's key: its name and which of the controls carrying that name it is. */
function keyed(doc) {
  const seen = {}
  return actionableControls(doc).map((el) => {
    const name = controlName(el)
    seen[name] = (seen[name] || 0) + 1
    return { el, key: name + ' #' + seen[name], name }
  })
}

/**
 * Click every clickable control on one screen and return the inert ones.
 * Controls are clicked in order on one mount; a control an earlier click took
 * off the glass (or turned off) is clicked on a fresh mount instead.
 */
async function clickCensus(url, element) {
  const first = await mount(url, element)
  const plan = keyed(first.d).map((c) => ({ key: c.key, name: c.name, tag: c.el.tagName.toLowerCase(), clickable: c.el.matches('button, [role="button"]') }))
  const staticInert = inertControls(first.d)
  /* "Disabled with a visible reason" is the other half of the rule: a control
   * that is off and says nothing is as dead as one that is on and does nothing. */
  for (const el of first.d.querySelectorAll(OFF)) {
    const why = (el.getAttribute('aria-describedby') || '').split(/\s+/).filter(Boolean).map((id) => (first.d.getElementById(id) || {}).textContent || '').join(' ').trim()
    if (!why) staticInert.push({ tag: el.tagName.toLowerCase(), name: controlName(el), why: 'off, and says no reason' })
  }
  const all = census(first.d).length
  first.close()
  const pending = plan.filter((p) => p.clickable)
  const out = { all, total: plan.length, clicked: 0, staticInert, clickInert: [], errors: [], crashes: [] }
  let mounts = 0
  while (pending.length) {
    mounts += 1
    if (mounts > plan.length + 1) throw new Error('the census could not reach: ' + pending.map((p) => p.key).join(' | '))
    const screen = await mount(url, element)
    let progressed = false
    try {
      for (const p of [...pending]) {
        const hit = keyed(screen.d).find((c) => c.key === p.key)
        if (!hit) continue
        /* Under an open modal only the modal is reachable, as with a pointer. */
        const modal = screen.d.querySelector('dialog[open], [aria-modal="true"]')
        if (modal && !modal.contains(hit.el)) continue
        const errors = []
        const onError = (e) => errors.push(String(e.error || e.message))
        screen.w.addEventListener('error', onError)
        const crashed = screen.crashes.length
        const effects = await observeClick(screen.w, hit.el, { effects: screen.effects, settle: settleClick })
        screen.w.removeEventListener('error', onError)
        if (screen.crashes.length > crashed) out.crashes.push({ name: p.key, error: screen.crashes.slice(crashed).join(' | ') })
        pending.splice(pending.indexOf(p), 1)
        progressed = true
        out.clicked += 1
        if (errors.length) out.errors.push({ name: p.key, errors })
        if (!effects.length) out.clickInert.push({ tag: p.tag, name: p.key })
      }
    } finally { screen.close() }
    if (!progressed) throw new Error('no pending control was on a fresh screen: ' + pending.map((p) => p.key).join(' | '))
  }
  out.mounts = mounts + 1
  return out
}

const INDEX = { projects: [], totals: {}, kind: 'index', contract_version: 1 }

/* KNOWN, NOT FIXED HERE, present at the base this card was cut from: on the
 * session workspace at Workbench, "Show findings only" recurses without end —
 * `forcedMembers` → `findingContext` → `linkedSet` → `allWires` → `pinPos` →
 * `geo` → `flow` → `forcedMembers` (shell.jsx) — and the error boundary
 * replaces the whole screen. It is a crash, not a dead control, so it is
 * reported rather than folded into A3-F10. */
const KNOWN_CRASHES = [
  { where: 'the session workspace at workbench', control: 'Show findings only #1', error: 'Maximum call stack size exceeded' },
]
const SCREENS = []
for (const altitude of ['brief', 'explore', 'workbench']) {
  SCREENS.push([`the session workspace at ${altitude}`, 'http://127.0.0.1/?altitude=' + altitude, () => L.React.createElement(L.Workspace, { index: INDEX })])
}
for (const [label, id] of [['a proven Change', A11Y_CHANGE_ID], ['a Change with finding cards', FOUND]]) {
  for (const altitude of ['brief', 'explore', 'workbench']) {
    for (const stage of ['define', 'prepare', 'build', 'verify', 'review', 'approve']) {
      SCREENS.push([`the Change screen, ${label}, at ${altitude}/${stage}`, `http://127.0.0.1/?altitude=${altitude}#/changes/${id}/${stage}`, () => L.React.createElement(L.ChangeWorkspace, { id, stage })])
    }
  }
}
for (const at of ['brief/define', 'explore/verify']) {
  const [altitude, stage] = at.split('/')
  SCREENS.push([`the Change screen, a Change whose record holds a failure, at ${at}`, `http://127.0.0.1/?altitude=${altitude}#/changes/${BROKE}/${stage}`, () => L.React.createElement(L.ChangeWorkspace, { id: BROKE, stage })])
}
for (const [label, id] of [['a proven Change', A11Y_CHANGE_ID], ['a Change with finding cards', FOUND]]) {
  SCREENS.push([`the whole story on one page, ${label}`, `http://127.0.0.1/#/changes/${id}/packet`, () => L.React.createElement(L.PacketView, { id })])
}
SCREENS.push(['the Inbox', 'http://127.0.0.1/#/inbox', () => L.React.createElement(L.Inbox)])
SCREENS.push(['first use', 'http://127.0.0.1/#/first-use', () => L.React.createElement(L.FirstUse)])

test('A3-F10 · the control census reports 0 inert controls on every screen, by props and by clicking', async () => {
  const report = []
  let cardsSeen = 0
  for (const [where, url, element] of SCREENS) {
    const r = await clickCensus(url, element)
    if (where.includes('finding cards') && /\/verify$|at brief/.test(where)) {
      const s = await mount(url, element)
      cardsSeen += s.d.querySelectorAll('.ws-verdict-card').length
      s.close()
    }
    /* A screen that failed to render would pass by being empty. */
    assert.ok(r.all > 0, `${where}: the census found no control at all — the screen did not render`)
    report.push({ where, total: r.total, clicked: r.clicked, inert: [...r.staticInert.map((c) => c.why + ': ' + c.name), ...r.clickInert.map((c) => 'no effect: ' + c.name)], errors: r.errors, crashes: r.crashes })
  }
  const inert = report.filter((r) => r.inert.length)
  assert.ok(cardsSeen > 0, 'the fixture with finding cards drew none; the census never reached a card')
  assert.deepEqual(inert.map((r) => ({ where: r.where, inert: r.inert })), [], `inert controls:\n${inert.map((r) => r.where + ': ' + r.inert.join(' | ')).join('\n')}`)
  /* A click that takes the whole screen down is not inert — it is worse, and
   * it is not this card's to fix. It is pinned here so the census can never
   * stop seeing it: when it is fixed this list must be emptied, and a new one
   * fails the test. */
  const crashes = report.flatMap((r) => r.crashes.map((c) => ({ where: r.where, control: c.name, error: c.error })))
  assert.deepEqual(crashes, KNOWN_CRASHES)
  const thrown = report.filter((r) => r.errors.length)
  assert.deepEqual(thrown.map((r) => r.where), [], `a click threw:\n${thrown.map((r) => r.where + ': ' + JSON.stringify(r.errors)).join('\n')}`)
  console.log(`# census: ${report.length} screens, ${report.reduce((n, r) => n + r.total, 0)} controls, ${report.reduce((n, r) => n + r.clicked, 0)} clicked, ${inert.length} screens with an inert control`)
})

test('A3-F10 · no `={() => {}}` handler prop in the workspace source: a control is given a real act or none', async () => {
  const files = (await readdir(here)).filter((f) => f.endsWith('.jsx'))
  const hits = []
  for (const f of files) {
    const src = await readFile(join(here, f), 'utf8')
    src.split('\n').forEach((line, n) => { if (NOOP_HANDLER_PROP.test(line)) hits.push(`${f}:${n + 1}: ${line.trim()}`) })
  }
  assert.ok(files.length > 10, 'the scan read the workspace sources')
  assert.deepEqual(hits, [])
})

test('A3-F10 · the census is a live instrument: a button with no act, and one whose act changes nothing, are both inert', async () => {
  const s = await mount('http://127.0.0.1/', () => L.React.createElement('div', null,
    L.React.createElement('button', { type: 'button' }, 'Nothing'),
    L.React.createElement('button', { type: 'button', onClick: () => {} }, 'Empty'),
    L.React.createElement('button', { type: 'button', onClick: () => { navigator.clipboard.writeText('x') } }, 'Copies'),
    L.React.createElement('a', { href: '#/inbox' }, 'Link'),
  ))
  try {
    assert.deepEqual(inertControls(s.d).map((c) => c.name), ['Nothing', 'Empty'])
    const [nothing, empty, copies] = actionableControls(s.d)
    assert.deepEqual(await observeClick(s.w, nothing, { effects: s.effects, settle: settleClick }), [])
    assert.deepEqual(await observeClick(s.w, empty, { effects: s.effects, settle: settleClick }), [])
    assert.deepEqual(await observeClick(s.w, copies, { effects: s.effects, settle: settleClick }), ['clipboard'])
  } finally { s.close() }
})
