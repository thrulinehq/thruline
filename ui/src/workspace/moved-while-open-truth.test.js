/* MOVED-WHILE-OPEN-TRUTH (launch-critical) — "moved while this screen was open"
 * is said only when the screen saw it move.
 *
 * Found by the C12 verifier's walk (gates/verify-MERGE-SEAM-C12, Item 8, finding
 * P2, older than C12): prepare a Change, accept a new list, then load the Change
 * fresh. The Brief said "This Change moved while this screen was open — the
 * approval names V, and the record now stands at W." The screen was not open
 * when it moved; it was opened after. A false sentence about the reader's own
 * session, on the screen whose every sentence has to be true.
 *
 * The rule these tests hold, on the Change screen MOUNTED in jsdom with the
 * server's answers stubbed (the screen's own reads, the Refresh control, the
 * document a person reads):
 *
 *   (a) a screen that loads a Change whose record has already moved past its
 *       approval never says "while this screen was open" — on Brief, on
 *       Explore, after any number of reloads that see the same record — and
 *       still states the version the approval names and the version the record
 *       now stands at;
 *   (b) a screen that is open when the served version changes between two reads
 *       it made itself MAY say it, and does, for as long as it stays open.
 *
 * And the approval's version binding is not part of this: the approval keeps
 * naming the version it was given for, and Approve is not offered for the
 * approval that stands (T-5-2, T-5-2-NO-REVIVAL — those tests are untouched).
 *
 * Every fixture is synthetic. No string here came off a real machine.
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
const temporary = await mkdtemp(join(tmpdir(), 'thruline-moved-while-open-'))
after(() => rm(temporary, { recursive: true, force: true }))

const DRIVE = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { createRoot } from 'react-dom/client';
  import { flushSync } from 'react-dom';
  import ChangeWorkspace, { ChangeShell, readChange } from './change-workspace.jsx';
  export { React, createRoot, flushSync, ChangeWorkspace, readChange };
  export const renderRoute = props => renderToStaticMarkup(React.createElement(ChangeShell, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: DRIVE, resolveDir: here },
  })
  const file = join(temporary, 'moved-while-open.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

/* ── one Change the record backs, approved on A, and the record at B ───── */

const VERSION_A = 'e71c5b0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
const VERSION_B = '9f3d21ebbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'

const APPROVAL = {
  id: 'cd_1', act: 'approved', target_revision: VERSION_A, scope_version: 1,
  actor: 'sam', decided_at: '2026-09-22T11:00:00Z', withdraws: null, record_path: null,
}

/** The Change as the server serves it with the record standing at `revision`, approved on A. */
const changeAt = (revision, { list = 1 } = {}) => ({
  id: 'ch1',
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Totals add up on the checkout page' },
  implementation: { snapshot_id: revision, bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: list,
    criteria: ['The totals add up'],
    allowed_scope: ['checkout/'],
    check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet',
  decisions: { standing: APPROVAL, history: [APPROVAL], target_revision: revision, scope_version: list },
})
const COVERAGE = {
  scope_version: 1,
  link_rows: 1,
  execution_sessions: ['s1'],
  criteria: [{
    index: 1,
    text: 'The totals add up',
    state: 'evidence',
    items: [{
      link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run',
      name: 'tests pass', observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies',
      fresh: true, fresh_slug: 'applicability_still_applies',
    }],
  }],
}
const READINESS = {
  schema_version: 1, ready: true, scope_version: 1, target_version: 'a1b2c3d4e5f6', headline_slug: 'readiness_ready',
  obligations: [
    { id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: null, disposition: null, waivable: false },
    { id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: 'The totals add up', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: 'tests pass', disposition: null, waivable: true },
  ],
  rests_on: [{ obligation_id: 'criterion:1', text: 'The totals add up', evidence_name: 'tests pass' }],
  missing: [], waived: [], not_applicable: [],
}

/** What the server answers, and what a test moves under the screen. */
function served(change) {
  return {
    '/api/changes/ch1': { change },
    '/api/changes/ch1/proposals': { proposals: null },
    '/api/changes/ch1/coverage': { coverage: COVERAGE, readiness: READINESS },
    '/api/changes/ch1/inputs': {},
    '/api/changes/ch1/applicability': { items: [] },
  }
}

const settle = () => new Promise((r) => setTimeout(r, 5))
const answer = (doc) => ({ ok: true, status: 200, headers: { get: () => null }, text: async () => JSON.stringify(doc) })

/**
 * Mount the Change screen in a fresh jsdom window with the server stubbed, hand
 * the test the routes (to move the record under it), the document, and a way to
 * press Refresh and wait for the read it starts to land.
 */
async function open(routes, run) {
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/#/changes/ch1' })
  const w = dom.window
  const names = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'HTMLInputElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage', 'fetch']
  const held = names.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of names) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  let reads = 0
  set('fetch', async (u) => {
    if (u === '/api/changes/ch1') reads += 1
    return u in routes ? answer(routes[u]) : { ok: false, status: 404, headers: { get: () => null }, text: async () => '{}' }
  })
  const root = M.createRoot(w.document.getElementById('root'))
  try {
    M.flushSync(() => root.render(M.React.createElement(M.ChangeWorkspace, { id: 'ch1', stage: 'define' })))
    for (let i = 0; i < 80 && !w.document.querySelector('.ws-shell-refresh'); i += 1) await settle()
    await settle()
    const d = w.document
    const refresh = async () => {
      const before = reads
      const button = [...d.querySelectorAll('button.ws-shell-refresh')].find((b) => b.textContent.trim() === 'Refresh')
      assert.ok(button, 'the screen offers Refresh')
      button.click()
      for (let i = 0; i < 80 && reads === before; i += 1) await settle()
      await settle()
      await settle()
    }
    const text = () => d.body.textContent.replace(/\s+/g, ' ').trim()
    /* The same read a fresh load makes, drawn at another altitude. */
    const drawn = async (altitude) => {
      const r = await M.readChange('ch1')
      assert.equal(r.status, 'ready')
      const noop = () => {}
      const html = M.renderRoute({ model: r.model, stage: 'define', altitude, register: 'manager', onGo: noop, onCopy: noop, onReload: noop, onAltitude: noop, onRegister: noop })
      return new JSDOM('<!doctype html><body>' + html + '</body>').window.document.body.textContent.replace(/\s+/g, ' ').trim()
    }
    return await run({ d, text, refresh, drawn, reads: () => reads })
  } finally {
    root.unmount()
    for (const [n, dsc] of held) {
      if (dsc) Object.defineProperty(globalThis, n, dsc)
      else delete globalThis[n]
    }
    w.close()
  }
}

const WHILE_OPEN = 'while this screen was open'
const FRESH = 'This Change has moved since it was approved — the approval names e71c5b0, and the record now stands at 9f3d21e.'
const OPEN = 'This Change moved while this screen was open — the approval names e71c5b0, and the record now stands at 9f3d21e.'

/* ── (a) a fresh load never says it ─────────────────────────────────────── */

test('(a) a screen opened AFTER the move never says "while this screen was open", and states both versions', async () => {
  await open(served(changeAt(VERSION_B)), async ({ d, text, drawn }) => {
    assert.ok(d.querySelector('.ws-change-changed-since'), 'the moved sentence is on the screen: the approval names one version, the record stands at another')
    assert.ok(!text().includes(WHILE_OPEN), 'the screen was not open when it moved, and does not say so')
    assert.ok(text().includes(FRESH), 'it says what is true: which version the approval names and which one the record stands at')
    assert.ok(text().includes('Approved on e71c5b0 — code changed since'), 'the approval still names the version it was given for')
    /* The same read, drawn at the other altitude. */
    const explore = await drawn('explore')
    assert.ok(!explore.includes(WHILE_OPEN))
    assert.ok(explore.includes(FRESH))
  })
})

test('(a) prepare, then a new list accepted, then a fresh load: the card\'s own case says the same true thing, and the list sentence beside it', async () => {
  await open(served(changeAt(VERSION_B, { list: 2 })), async ({ text }) => {
    assert.ok(!text().includes(WHILE_OPEN))
    assert.ok(text().includes(FRESH))
    assert.ok(text().includes('This approval was given against list 1 of what counts as done. The accepted list is now list 2'), 'the list moved, said once, in its own sentence')
  })
})

test('(a) reloading a record that has not moved is not a move: still never "while this screen was open", however many times it is read', async () => {
  await open(served(changeAt(VERSION_B)), async ({ text, refresh, reads }) => {
    const first = reads()
    await refresh()
    await refresh()
    assert.ok(reads() >= first + 2, 'the screen really read the record again')
    assert.ok(!text().includes(WHILE_OPEN))
    assert.ok(text().includes(FRESH))
  })
})

/* ── (b) an open screen may say it ──────────────────────────────────────── */

test('(b) a screen that is open when the served version changes between two reads it made itself says so, naming both versions', async () => {
  const routes = served(changeAt(VERSION_A))
  await open(routes, async ({ d, text, refresh }) => {
    /* Approved on A, the record at A: nothing has moved, and nothing says it has. */
    assert.ok(text().includes('Approved on e71c5b0'))
    assert.ok(!d.querySelector('.ws-change-changed-since'), 'no moved sentence before anything moved')
    assert.ok(!text().includes(WHILE_OPEN))
    /* The record moves under the open screen; the screen reads it again. */
    Object.assign(routes, served(changeAt(VERSION_B)))
    await refresh()
    assert.ok(text().includes(OPEN), 'the screen was open, saw the version change, and says exactly that')
    assert.ok(text().includes('Approved on e71c5b0 — code changed since'), 'the approval is still on the version it was given for')
    /* And it stays true for as long as this screen stays open. */
    await refresh()
    assert.ok(text().includes(OPEN))
  })
})

test('(b) a screen opened afterwards, on the same record, does not inherit what the first one saw', async () => {
  const routes = served(changeAt(VERSION_A))
  await open(routes, async ({ text, refresh }) => {
    Object.assign(routes, served(changeAt(VERSION_B)))
    await refresh()
    assert.ok(text().includes(OPEN))
  })
  await open(routes, async ({ text }) => {
    assert.ok(!text().includes(WHILE_OPEN))
    assert.ok(text().includes(FRESH))
  })
})

/* ── the approval's version binding is not moved by any of this ─────────── */

test('the approval keeps naming the version it was given for, and Approve is not offered for it, whichever sentence is said', async () => {
  const routes = served(changeAt(VERSION_A))
  await open(routes, async ({ d, text, refresh }) => {
    Object.assign(routes, served(changeAt(VERSION_B)))
    await refresh()
    assert.ok(text().includes(OPEN))
    assert.ok(!text().includes('Approved on 9f3d21e'), 'the approval never moves onto the version the record now stands at')
    const buttons = [...d.querySelectorAll('button')].map((b) => b.textContent.trim())
    assert.ok(!buttons.includes('Approve this version'), 'an approval stands, so Approve is not offered until it is withdrawn')
  })
  await open(routes, async ({ d, text }) => {
    assert.ok(!text().includes('Approved on 9f3d21e'))
    assert.ok(![...d.querySelectorAll('button')].some((b) => b.textContent.trim() === 'Approve this version'))
  })
})
