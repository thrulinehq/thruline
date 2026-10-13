/* PATH-UI-2-R2, BROKEN 2 — the whole-story page's "link it" is a door to the
 * control that links, in one act.
 *
 * Found by the round-1 adversary at f4b81633 (launch gates/adversary-CLOUD-
 * PATH-UI-2/TRANSCRIPT-f4b81633.md, r2/door.mjs). RUN-LINK-INSPECTOR Done-when
 * 3 says the page's "link it" sentence is "a link or button to that same
 * control". What shipped was `<a href="#/changes/<id>/verify">`: clicked, it
 * loaded the Change screen on its Brief altitude (which draws no stages, so the
 * address's stage is ignored), where there was no line open, no name input and
 * no button; the person had to press the Inspector's own "Go to … to link it" a
 * second time to reach them.
 *
 * These tests do what the browser does and nothing shorter. The whole-story
 * page is mounted in a real DOM (jsdom); its door is found by its class and
 * followed by its `href` alone, as a click on an anchor follows it: a NEW
 * document at the address the anchor names, on which `main.jsx`'s own routing
 * table (`chooseScreen`) picks the Change screen, which is then mounted as
 * `main.jsx` mounts it. Nothing in the test knows how the door carries the
 * line it names; if it carried it nowhere, this is what would show.
 *
 * The bar is the run sheet's own door (`openDoor(criterionDoor(n))` from
 * `run-sheet.jsx`): the Verify stage, on the line, in one act. The card asks
 * for a little more: the name input, the box a person has to fill before the
 * line's link control comes on, is where focus lands.
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

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-packet-link-door-'))
after(() => rm(temporary, { recursive: true, force: true }))

/* React DOM decides at LOAD whether the browser has a DOM and whether it fires
 * `input` events; a bundle loaded before any `document` exists takes the
 * Internet Explorer path, and a typed name then never reaches `onChange`. So a
 * bare document is put up while the bundle loads, and the tests below swap it
 * for their own, one document at a time. */
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
const file = join(temporary, 'door.mjs')
await writeFile(file, result.outputFiles[0].text)
const L = await import(pathToFileURL(file).href)

const C = L.HQ_COPY.change
const LINE_ONE = 'When the cart is empty, the total shows zero.'
const LINE_TWO = 'The total shows the sum of the prices.'
const ID = 'cccccccc-0000-4000-8000-00000000c0b1'

/* ── the served documents: a passing run that no line is linked to ───────── */

const change = (criteria) => ({
  id: ID,
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Empty cart total' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1, criteria, allowed_scope: ['src/'], check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet',
  decisions: [],
})

const IDENTITY = {
  id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '',
  outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null,
  evidence_name: null, linked_by: null, waiver: null, waivable: false,
}
const unlinked = (n, text) => ({
  id: 'criterion:' + n, kind: 'criterion', criterion_index: n, text,
  outcome: 'needs_evidence', outcome_slug: 'needs_evidence',
  unsatisfied: 'unlinked_run', unsatisfied_slug: 'readiness_unlinked_run',
  evidence_name: null, linked_by: null, waiver: null, waivable: true,
})
const CANDIDATE = {
  kind: 'check_run', evidence_id: 'eb_test', session_id: 's1', name: 'cargo test',
  observed: 'proven', bears: true, execution: 'completed', execution_slug: 'execution_completed',
}
const coverageOf = (lines) => ({
  scope_version: 1,
  link_rows: 0,
  execution_sessions: ['s1'],
  criteria: lines.map((text, i) => ({ index: i + 1, text, state: 'none', items: [], removed: [] })),
  candidates: [CANDIDATE],
  executions: [],
})
const readinessOf = (lines) => ({
  schema_version: 1, change_id: ID, ready: false, with_exceptions: false, scope_version: 1,
  target_version: 'e71c5b0aaaa', headline_slug: 'readiness_needs_evidence_one',
  obligations: [IDENTITY, ...lines.map((t, i) => unlinked(i + 1, t))],
  rests_on: [], missing: lines.map((_, i) => 'criterion:' + (i + 1)), waived: [], not_applicable: [],
})

/** What each read answers, by address. Anything else is a 404, which the screens say aloud and go on without. */
function routes(lines) {
  return {
    ['/api/changes/' + ID]: { change: change(lines) },
    ['/api/changes/' + ID + '/proposals']: { proposals: null },
    ['/api/changes/' + ID + '/coverage']: { coverage: coverageOf(lines), readiness: readinessOf(lines) },
    ['/api/changes/' + ID + '/inputs']: {},
    ['/api/changes/' + ID + '/applicability']: { items: [] },
  }
}
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

/* ── a browser, borrowed for one document ────────────────────────────────── */

const GLOBALS = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'HTMLInputElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage', 'CSS', 'fetch']
const settle = () => new Promise((r) => setTimeout(r, 10))

/** One document at `url`, with `element()` mounted in it, as `main.jsx` mounts a screen. */
async function document_(url, table, element, run, { storage = {}, before = null, strict = false } = {}) {
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url, pretendToBeVisual: true })
  const w = dom.window
  const writes = []
  const scrolled = []
  /* jsdom draws nothing, so it has no scroll; recording who was asked to scroll is the honest stand-in. */
  w.Element.prototype.scrollIntoView = function scrollIntoView() { scrolled.push(this.id || this.tagName) }
  for (const [k, v] of Object.entries(storage)) w.localStorage.setItem(k, v)
  if (before) before(w)
  const held = GLOBALS.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of GLOBALS) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  set('fetch', fetchOf(table, writes))
  const root = L.createRoot(w.document.getElementById('root'))
  try {
    L.flushSync(() => root.render(strict ? L.React.createElement(L.React.StrictMode, null, element(w)) : element(w)))
    for (let i = 0; i < 40; i += 1) await settle()
    return await run({ w, d: w.document, writes, scrolled })
  } finally {
    root.unmount()
    for (const [n, desc] of held) {
      if (desc) Object.defineProperty(globalThis, n, desc)
      else delete globalThis[n]
    }
    w.close()
  }
}

const PACKET = 'http://127.0.0.1/#/changes/' + ID + '/packet'

/** The doors on the whole-story page, as a person would find them: the text and the address each names. */
async function packetDoors(url, table) {
  return document_(url, table, () => L.React.createElement(L.PacketView, { id: ID }), async ({ d }) => (
    [...d.querySelectorAll('a.ws-packet-link-door')].map((a) => ({ text: a.textContent.trim(), href: a.getAttribute('href') }))
  ))
}

/**
 * Follow an anchor: a NEW document at the address it names (resolved against
 * the page it was on), routed by the app's own table, the Change screen
 * mounted as `main.jsx` mounts it.
 */
async function follow(from, href, table, run, options) {
  const url = new URL(href, from)
  const at = L.chooseScreen({ pathname: url.pathname, hash: url.hash, search: url.search })
  assert.equal(at.screen, 'change', `the address ${url.href} names the Change screen`)
  assert.equal(at.id, ID)
  return document_(url.href, table, () => L.React.createElement(L.ChangeWorkspace, { id: at.id, stage: at.stage }), run, options)
}

const nameInput = (d) => d.querySelector('.ws-cov .ws-cov-actor input')
const describe = (el) => (el ? `${el.tagName.toLowerCase()}${el.id ? '#' + el.id : ''}${el.className ? '.' + String(el.className).split(' ')[0] : ''}` : 'nothing')

/** What one act must have done, on the document the door led to. */
function assertLanded({ d, w, scrolled }, line) {
  const shell = d.querySelector('.ws-shell')
  assert.ok(shell, 'the Change screen mounted')
  assert.notEqual(shell.getAttribute('data-altitude'), 'brief', 'not the altitude that draws no stages')
  const tab = d.getElementById('ws-change-tab-verify')
  assert.ok(tab, 'the stage strip is drawn')
  assert.equal(tab.getAttribute('aria-selected'), 'true', 'the Verify stage is the open one')
  assert.ok(d.getElementById('ws-cov-line-' + line), `line ${line} is open on the screen`)
  const input = nameInput(d)
  assert.ok(input, 'the name box that gates the link control is drawn')
  /* Compared as booleans, never as two elements: a failed assert.equal of two DOM nodes makes Node print both object graphs, which takes minutes and the process with it. */
  assert.ok(d.activeElement === input, `keyboard focus is on the name box, not on ${describe(d.activeElement)}`)
  assert.ok(scrolled.length > 0, 'the page was asked to bring something into view')
  assert.equal(w.location.search.includes('door'), false, 'the request is spent: a reload does not repeat it')
}

/* ── the door, on a Change with one line ───────────────────────────────── */

test('the whole-story page carries a door for the line a passing run nobody linked would settle', async () => {
  const doors = await packetDoors(PACKET, routes([LINE_ONE]))
  assert.ok(doors.length >= 1, 'the page names the line and offers a way to it')
  for (const door of doors) assert.equal(door.text, L.HQ_COPY.run.outcomes.passed.unlinked_door.replace('{line}', LINE_ONE), door.text)
})

test('ONE act on that door: the Change screen opens on the Verify stage, on the line, with the name box in focus', async () => {
  const doors = await packetDoors(PACKET, routes([LINE_ONE]))
  assert.ok(doors.length >= 1)
  await follow(PACKET, doors[0].href, routes([LINE_ONE]), async (page) => { assertLanded(page, 1) })
})

test('every door on the page does the same, in one act', async () => {
  const doors = await packetDoors(PACKET, routes([LINE_ONE]))
  assert.ok(doors.length >= 2, 'one after "still needs evidence" and one under the mapping row')
  for (const door of doors) {
    await follow(PACKET, door.href, routes([LINE_ONE]), async (page) => { assertLanded(page, 1) })
  }
})

test('from there a person types a name and the line\'s link control is on: nothing else stood in the way', async () => {
  const doors = await packetDoors(PACKET, routes([LINE_ONE]))
  await follow(PACKET, doors[0].href, routes([LINE_ONE]), async ({ d, w }) => {
    const input = nameInput(d)
    assert.ok(d.activeElement === input, `focus is on ${describe(d.activeElement)}`)
    const row = () => d.querySelector('.ws-cov-row')
    const linkBefore = [...row().querySelectorAll('button')].find((b) => b.textContent.trim() === C.coverage.link)
    assert.ok(linkBefore, 'the line has its link control')
    assert.ok(linkBefore.disabled || linkBefore.getAttribute('aria-disabled') === 'true', 'off until a name is on it')
    const setter = Object.getOwnPropertyDescriptor(w.HTMLInputElement.prototype, 'value').set
    setter.call(input, 'walker')
    input.dispatchEvent(new w.Event('input', { bubbles: true }))
    for (let i = 0; i < 6; i += 1) await settle()
    const linkAfter = [...row().querySelectorAll('button')].find((b) => b.textContent.trim() === C.coverage.link)
    assert.ok(linkAfter && !linkAfter.disabled && linkAfter.getAttribute('aria-disabled') !== 'true', 'on once a name is typed')
  })
})

/* ── the door names the line it opens ──────────────────────────────────── */

test('with two lines nobody linked, each door opens its own line', async () => {
  const table = routes([LINE_ONE, LINE_TWO])
  const doors = await packetDoors(PACKET, table)
  const second = doors.find((d) => d.text.includes(LINE_TWO))
  const first = doors.find((d) => d.text.includes(LINE_ONE))
  assert.ok(first && second, `a door for each line: ${JSON.stringify(doors.map((d) => d.text))}`)
  /* PATH-UI-2-R3 (D-050): this used to assert that the summary entry `ws-cov-line-<n>` was the element brought into view. That entry is in the list of accepted lines, about three thousand pixels above the controls on a Change of any length; a door that scrolled to it and then focused the name box (R2) left the named line's control below the fold from the fifth line up. What "each door opens its own line" means is that the row the person is left looking at is that line's: the row that holds its control, marked current. `packet-link-door-r3.test.js` pins where the view ends up. */
  const markedRow = (d) => { const r = d.querySelector('.ws-cov-rows > li[aria-current]'); return r ? r.querySelector('.ws-cov-criterion').textContent : null }
  await follow(PACKET, first.href, table, async (page) => { assertLanded(page, 1); assert.equal(markedRow(page.d), LINE_ONE, 'line 1\'s row is the one marked') })
  await follow(PACKET, second.href, table, async (page) => { assertLanded(page, 2); assert.equal(markedRow(page.d), LINE_TWO, 'line 2\'s row is the one marked') })
})

/* ── the altitude the person was at ────────────────────────────────────── */

test('the door works from an address that already carries an altitude, and keeps it', async () => {
  const from = 'http://127.0.0.1/?altitude=explore#/changes/' + ID + '/packet'
  const doors = await packetDoors(from, routes([LINE_ONE]))
  assert.ok(doors.length >= 1)
  await follow(from, doors[0].href, routes([LINE_ONE]), async (page) => {
    assertLanded(page, 1)
    assert.equal(page.d.querySelector('.ws-shell').getAttribute('data-altitude'), 'explore')
    assert.match(page.w.location.search, /altitude=explore/, 'what else was on the address stays on it')
  })
})

test('a person who chose the old Pro view is still on Workbench after the door', async () => {
  const doors = await packetDoors(PACKET, routes([LINE_ONE]))
  await follow(PACKET, doors[0].href, routes([LINE_ONE]), async (page) => {
    assertLanded(page, 1)
    assert.equal(page.d.querySelector('.ws-shell').getAttribute('data-altitude'), 'workbench')
  }, { storage: { 'thruline.shell.altitude.v1': 'pro' } })
})

/* ── nothing else moved ───────────────────────────────────────────────── */

test('opening the Change without a door is what it was: Brief, and no focus taken', async () => {
  await document_('http://127.0.0.1/#/changes/' + ID, routes([LINE_ONE]), () => L.React.createElement(L.ChangeWorkspace, { id: ID, stage: 'define' }), async ({ d }) => {
    assert.equal(d.querySelector('.ws-shell').getAttribute('data-altitude'), 'brief')
    assert.ok(d.activeElement !== nameInput(d), 'no focus was taken')
    assert.ok(nameInput(d) === null, 'Brief draws no line controls')
  })
})

test('a stage address with no door still opens the Change as it did: no focus is taken, no line is opened for a person who did not ask', async () => {
  await document_('http://127.0.0.1/#/changes/' + ID + '/verify', routes([LINE_ONE]), () => L.React.createElement(L.ChangeWorkspace, { id: ID, stage: 'verify' }), async ({ d }) => {
    assert.equal(d.querySelector('.ws-shell').getAttribute('data-altitude'), 'brief')
    assert.notEqual(d.activeElement && d.activeElement.tagName, 'INPUT')
  })
})

test('a door for a line the Change does not have opens the Verify stage and steals no focus', async () => {
  const doors = await packetDoors(PACKET, routes([LINE_ONE]))
  const wrong = new URL(doors[0].href, PACKET)
  wrong.searchParams.set('door', '9')
  await follow(PACKET, wrong.search + wrong.hash, routes([LINE_ONE]), async ({ d }) => {
    assert.equal(d.getElementById('ws-change-tab-verify').getAttribute('aria-selected'), 'true')
    assert.ok(d.getElementById('ws-cov-line-9') === null)
    assert.ok(d.activeElement !== nameInput(d), 'focus goes nowhere it was not asked to go')
  })
})

test('the whole-story page writes nothing when its door is followed', async () => {
  let writes
  await document_(PACKET, routes([LINE_ONE]), () => L.React.createElement(L.PacketView, { id: ID }), async (page) => {
    page.d.querySelector('a.ws-packet-link-door').addEventListener('click', (e) => e.preventDefault())
    page.d.querySelector('a.ws-packet-link-door').click()
    writes = page.writes
  })
  assert.deepEqual(writes, [])
})

/* ── self-attack: the ways a door can be misused, repeated or refused ─────── */

const landedAddress = async (from, table) => {
  const doors = await packetDoors(from, table)
  let after
  await follow(from, doors[0].href, table, async (page) => { assertLanded(page, 1); after = page.w.location.href })
  return after
}

test('a reload after the door has landed does not repeat it: the line stays open and no focus is taken from a person who has moved on', async () => {
  const table = routes([LINE_ONE])
  const address = await landedAddress(PACKET, table)
  assert.doesNotMatch(address, /door=/, 'the address the person is left on carries no request')
  assert.match(address, /#\/changes\/[^/]+\/verify$/)
  /* What a reload is: a new document at that address, with what this browser remembers (the altitude the door chose). */
  await document_(address, table, () => L.React.createElement(L.ChangeWorkspace, { id: ID, stage: 'verify' }), async ({ d }) => {
    assert.equal(d.getElementById('ws-change-tab-verify').getAttribute('aria-selected'), 'true')
    assert.ok(d.activeElement !== nameInput(d), 'the cursor is not put back in the name box')
  }, { storage: { 'thruline.shell.altitude.v1': 'explore' } })
})

test('an address that will not be rewritten (a sandboxed document) still lands, and says nothing about it', async () => {
  const table = routes([LINE_ONE])
  const doors = await packetDoors(PACKET, table)
  const url = new URL(doors[0].href, PACKET)
  const dom = await document_(url.href, table, () => L.React.createElement(L.ChangeWorkspace, { id: ID, stage: 'verify' }), async ({ d, scrolled }) => {
    assert.equal(d.getElementById('ws-change-tab-verify').getAttribute('aria-selected'), 'true')
    assert.ok(d.activeElement === nameInput(d), `focus is on ${describe(d.activeElement)}`)
    assert.ok(scrolled.length > 0)
    return 'landed'
  }, { before: (w) => { w.history.replaceState = () => { throw new Error('a sandboxed document refuses address writes') } } })
  assert.equal(dom, 'landed')
})

test('two requests in one address are ignored: the Change opens as if nobody had asked, and no focus is taken', async () => {
  const table = routes([LINE_ONE])
  await document_('http://127.0.0.1/?door=1&door=2#/changes/' + ID + '/verify', table, () => L.React.createElement(L.ChangeWorkspace, { id: ID, stage: 'verify' }), async ({ d }) => {
    assert.equal(d.querySelector('.ws-shell').getAttribute('data-altitude'), 'brief')
    assert.ok(d.activeElement !== nameInput(d))
  })
})

test('under React.StrictMode, as main.jsx mounts it, the door lands once and the cursor ends in the name box', async () => {
  const table = routes([LINE_ONE])
  const doors = await packetDoors(PACKET, table)
  const url = new URL(doors[0].href, PACKET)
  await document_(url.href, table, () => L.React.createElement(L.ChangeWorkspace, { id: ID, stage: 'verify' }), async (page) => {
    assertLanded(page, 1)
  }, { strict: true })
})

test('a door asked of an address that names another stage still ends on the Verify stage', async () => {
  const table = routes([LINE_ONE])
  await document_('http://127.0.0.1/?altitude=explore&door=1#/changes/' + ID + '/build', table, () => L.React.createElement(L.ChangeWorkspace, { id: ID, stage: 'build' }), async (page) => {
    assertLanded(page, 1)
  })
})

test('a door for a Change that cannot be opened says what any such address says, throws nothing, and keeps the request', async () => {
  await document_('http://127.0.0.1/?door=1#/changes/' + ID + '/verify', {}, () => L.React.createElement(L.ChangeWorkspace, { id: ID, stage: 'verify' }), async ({ d, w }) => {
    assert.ok(d.querySelector('.ws-change-title'), 'the notice is drawn')
    assert.ok(d.querySelector('.ws-shell') === null, 'no Change screen, so nothing to land on')
    assert.match(w.location.search, /door=1/, 'nothing spent the request: it was never used')
  })
})

test('a door on a Change with nothing accepted opens the Verify stage, takes no focus and throws nothing', async () => {
  const table = routes([])
  await document_('http://127.0.0.1/?altitude=explore&door=1#/changes/' + ID + '/verify', table, () => L.React.createElement(L.ChangeWorkspace, { id: ID, stage: 'verify' }), async ({ d }) => {
    assert.equal(d.getElementById('ws-change-tab-verify').getAttribute('aria-selected'), 'true')
    assert.ok(d.querySelector('.ws-cov-line') === null)
    assert.ok(d.activeElement !== d.querySelector('input'))
  })
})

test('the name box the door lands on is the one the link control reads: one box, in the section that lists the lines', async () => {
  const table = routes([LINE_ONE, LINE_TWO])
  await document_('http://127.0.0.1/?altitude=explore#/changes/' + ID + '/verify', table, () => L.React.createElement(L.ChangeWorkspace, { id: ID, stage: 'verify' }), async ({ d }) => {
    assert.equal(d.querySelectorAll('.ws-cov .ws-cov-actor input').length, 1)
    assert.equal(d.querySelectorAll('.ws-cov').length, 1)
    assert.ok(d.querySelector('.ws-cov').querySelector('.ws-cov-row'), 'and it sits with the rows it gates')
  })
})
