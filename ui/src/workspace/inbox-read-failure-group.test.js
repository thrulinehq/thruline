import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'

/* SEAM-INBOX-READ-VERIFY item 7 (verifier FAIL at f9dcb605, findings 1-3).
 *
 * A Change the Inbox could not read in full is its own labelled group: each
 * one is named by its title (or, with no readable title, by the first part of
 * its id), says in plain words that its details could not be read and that
 * nothing about it is settled, and keeps the server's own words one level
 * down. With nothing else needing attention, the one filled action on the
 * screen re-reads (GET only). With other Changes needing attention, the
 * Change's own action stays the filled one and re-reading is secondary.
 * No two adjacent lines both open with a count of Changes.
 *
 * Synthetic GET responses drive the real readInbox path; nothing here came
 * off a real machine. */
const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-inbox-group-'))
after(() => rm(temporary, { recursive: true, force: true }))

const result = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  stdin: {
    contents: `
      import React from 'react';
      import { renderToStaticMarkup } from 'react-dom/server';
      import { createRoot } from 'react-dom/client';
      import { flushSync } from 'react-dom';
      import Inbox, { readInbox, InboxScreen } from './inbox.jsx';
      import { API, CHANGE, CHANGE_PROPOSALS, CHANGE_COVERAGE, CHANGE_INPUTS, CHANGE_APPLICABILITY, CHANGE_FINDINGS } from './data.js';
      import { aStore } from './attention-count.fixture.js';
      import { HQ_COPY } from './hq-copy.js';
      export { React, createRoot, flushSync, Inbox, readInbox, API, CHANGE, CHANGE_PROPOSALS, CHANGE_COVERAGE, CHANGE_INPUTS, CHANGE_APPLICABILITY, CHANGE_FINDINGS, aStore, HQ_COPY };
      export const renderInbox = (state, props) => renderToStaticMarkup(React.createElement(InboxScreen, { state, ...(props || {}) }));
    `,
    resolveDir: here,
  },
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
})
const bundled = join(temporary, 'inbox-group.mjs')
await writeFile(bundled, result.outputFiles[0].text)
const M = await import(pathToFileURL(bundled).href)
const C = M.HQ_COPY.change.inbox
const U = C.unreadable_group || {}

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>')
const plain = (html) => unescape(html.replace(/<[^>]*>/g, ' ')).replace(/\s+/g, ' ').trim()
/* The page as a person reads it, one block per line. */
const lines = (html) => unescape(html
  .replace(/<\/(p|h1|h2|h3|li|summary|a|button|dt|dd)>/g, '\n')
  .replace(/<(p|h1|h2|h3|li|summary|a|button|dt|dd)([\s>])/g, '\n<$1$2')
  .replace(/<[^>]*>/g, ' '))
  .split('\n').map((l) => l.replace(/\s+/g, ' ').trim()).filter(Boolean)
const COUNT_LEAD = /^\d+ (more )?Changes?\b/
const clone = (value) => JSON.parse(JSON.stringify(value))
const primaries = (html) => html.match(/<(a|button)[^>]*class="[^"]*\bws-inbox-primary\b[^"]*"[^>]*>[^<]*</g) || []
const sectionOf = (html, id) => {
  const at = html.indexOf(`data-section="${id}"`)
  if (at < 0) return ''
  const start = html.lastIndexOf('<section', at)
  const end = html.indexOf('</section>', at)
  return html.slice(start, end + '</section>'.length)
}

/* Changes from the attention fixture. `criteria: 1` is a Change that needs
 * nothing; `criteria: 0` is one waiting on the person (nothing accepted yet). */
function storeOf(specs) {
  const all = []
  const docsById = new Map()
  specs.forEach((spec, i) => {
    const seed = M.aStore(6100, { count: 1, force: { onlyInfo: true, criteria: spec.criteria } })
    const row = clone(seed.all[0])
    const docs = clone(seed.docsById.get(row.id))
    const id = `cccccccc-0000-4000-8000-${String(i).padStart(12, '0')}`
    row.id = id
    row.opened_at = `2026-01-${String(i + 1).padStart(2, '0')}T09:00:00Z`
    row.request.title = spec.title
    docs.change.id = id
    docs.change.opened_at = row.opened_at
    docs.change.request.title = spec.title
    docs.change.associations = []
    all.push(row)
    docsById.set(id, docs)
  })
  return { all, docsById }
}

function routesOf(data) {
  const routes = new Map([[M.API.changes, { changes: data.all }], [M.API.storage, { bytes: 1000, path: '/tmp/synthetic-store' }]])
  for (const row of data.all) {
    const docs = data.docsById.get(row.id)
    routes.set(M.CHANGE(row.id), { change: docs.change })
    routes.set(M.CHANGE_PROPOSALS(row.id), { proposals: docs.proposals })
    routes.set(M.CHANGE_COVERAGE(row.id), { coverage: docs.coverage, readiness: docs.coverage.readiness })
    routes.set(M.CHANGE_INPUTS(row.id), docs.inputs || null)
    routes.set(M.CHANGE_APPLICABILITY(row.id), docs.applicability || null)
    routes.set(M.CHANGE_FINDINGS(row.id), docs.findings || null)
  }
  return routes
}

function fakeFetch(routes, failing, requests) {
  return async (url, init) => {
    requests.push({ url: String(url), method: String((init && init.method) || 'GET').toUpperCase() })
    const how = failing.get(url)
    if (how === 'refused') return new Response(JSON.stringify({ error: 'projection_failed', command: url, exit_code: 1 }), { status: 502 })
    if (how === 'missing') return new Response('{}')
    if (!routes.has(url)) throw new Error('unexpected synthetic route ' + url)
    return new Response(JSON.stringify(routes.get(url)))
  }
}

async function read(data, failing) {
  const requests = []
  const original = globalThis.fetch
  globalThis.fetch = fakeFetch(routesOf(data), failing, requests)
  try {
    const state = await M.readInbox()
    assert.ok(requests.every(({ method }) => method === 'GET'), 'the Inbox wrote while reading')
    return state
  } finally {
    globalThis.fetch = original
  }
}

test('the unreadable group speaks from the copy file, in plain words', () => {
  assert.equal(U.heading, 'Could not be read')
  assert.equal(U.line, 'ThruLine could not read the details of this Change, so nothing about it is settled.')
  assert.equal(U.untitled, 'Untitled Change {change}')
  assert.equal(U.technical_open, 'What the record said when it was read')
  assert.equal(U.action, 'Read it again')
  assert.equal(U.attention_heading, 'Read in full')
})

test('unreadable only: the Change is named on its own line, the raw words are one level down, and Read it again is the one primary', async () => {
  const data = storeOf([{ criteria: 1, title: 'Add dark mode' }])
  const detail = M.CHANGE(data.all[0].id)
  for (const how of ['refused', 'missing']) {
    const state = await read(data, new Map([[detail, how]]))
    const html = M.renderInbox(state)
    const group = sectionOf(html, 'unreadable')
    assert.ok(group, `${how}: the unreadable Changes are their own section`)
    assert.match(group, /<h2[^>]*>Could not be read<\/h2>/, `${how}: the group is labelled`)
    const item = (group.match(/<li class="ws-inbox-item[^"]*"[^>]*data-change="[^"]+"[\s\S]*?<\/li>/) || [''])[0]
    assert.match(item, /<h3 class="ws-inbox-title">Add dark mode<\/h3>/, `${how}: the Change is named by its title`)
    assert.match(item, new RegExp('<p class="ws-inbox-reason">' + U.line.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '</p>'), `${how}: its line says what happened`)
    /* The server's words: inside the Change's own disclosure, never on top. */
    const raw = how === 'refused' ? detail : fill(C.detail_missing, { title: 'Add dark mode' })
    const disclosure = (item.match(/<details[\s\S]*?<\/details>/) || [''])[0]
    assert.ok(unescape(disclosure).includes(raw), `${how}: the raw words are one level down, on the Change they belong to`)
    assert.ok(unescape(disclosure).includes(U.technical_open), `${how}: the disclosure says what it holds`)
    const notes = (html.match(/<ul class="ws-inbox-notes">[\s\S]*?<\/ul>/) || [''])[0]
    assert.ok(!unescape(notes).includes(raw), `${how}: the raw words are not repeated at the bottom of the page`)
    assert.ok(state.notes.some((n) => n.includes(raw)), `${how}: the reader still says it in its notes`)
    /* One primary, and it re-reads. */
    const p = primaries(html)
    assert.equal(p.length, 1, `${how}: exactly one primary on the screen, got ${p.length}`)
    assert.match(p[0], /^<button type="button"/, `${how}: the primary is a button, not a link that navigates`)
    assert.ok(p[0].endsWith('>' + U.action + '<'), `${how}: the primary says ${U.action}`)
    assert.ok(group.includes(p[0]), `${how}: the primary sits with what it re-reads`)
    assert.doesNotMatch(plain(html), /Nothing needs your attention right now/)
  }
})

test('a Change whose title could not be read is named by the first part of its id', async () => {
  const data = storeOf([{ criteria: 1, title: '' }])
  const id = data.all[0].id
  const state = await read(data, new Map([[M.CHANGE(id), 'refused']]))
  const group = sectionOf(M.renderInbox(state), 'unreadable')
  assert.match(group, new RegExp('<h3 class="ws-inbox-title">Untitled Change ' + id.slice(0, 8) + '</h3>'))
})

test('unreadable beside a Change that needs attention: separate labelled groups, the Change’s own action stays the one primary', async () => {
  const data = storeOf([{ criteria: 0, title: 'Fix the login' }, { criteria: 1, title: 'Add dark mode' }])
  const newest = data.all[1].id
  const state = await read(data, new Map([[M.CHANGE(newest), 'refused']]))
  assert.equal(state.items.length, 1, 'the fixture holds one Change that needs attention')
  assert.equal(state.unreadable, 1)
  const html = M.renderInbox(state)
  const group = sectionOf(html, 'unreadable')
  assert.match(group, /<h3 class="ws-inbox-title">Add dark mode<\/h3>/)
  const p = primaries(html)
  assert.equal(p.length, 1, `exactly one primary on the screen, got ${p.length}`)
  assert.ok(p[0].startsWith('<a '), 'the primary is the Change’s own next action')
  assert.ok(!group.includes('ws-inbox-primary'), 're-reading does not compete with it')
  const again = (group.match(/<button[^>]*>[^<]*</) || [''])[0]
  assert.ok(again.endsWith('>' + U.action + '<'), 're-reading is still offered')
  assert.match(again, /class="[^"]*\bws-btn-secondary\b/, 're-reading is drawn secondary')
  /* The Changes read in full are labelled as their own group too. */
  const ls = lines(html)
  const at = ls.indexOf(U.attention_heading)
  assert.ok(at >= 0, 'the Changes read in full have their own label')
  assert.equal(ls[at + 1], C.one, 'the attention count sits under that label')
})

test('no two adjacent lines both open with a count of Changes', async () => {
  const mixed = storeOf([{ criteria: 0, title: 'Fix the login' }, { criteria: 1, title: 'Add dark mode' }])
  const many = storeOf([{ criteria: 0, title: 'A' }, { criteria: 0, title: 'B' }, { criteria: 1, title: 'C' }, { criteria: 1, title: 'D' }])
  const cases = [
    ['mixed', await read(mixed, new Map([[M.CHANGE(mixed.all[1].id), 'refused']]))],
    ['many', await read(many, new Map([[M.CHANGE(many.all[2].id), 'refused'], [M.CHANGE_PROPOSALS(many.all[3].id), 'refused']]))],
    ['only', await read(storeOf([{ criteria: 1, title: 'Only' }]), new Map([[M.CHANGE('cccccccc-0000-4000-8000-000000000000'), 'missing']]))],
  ]
  for (const [name, state] of cases) {
    const ls = lines(M.renderInbox(state))
    for (let i = 1; i < ls.length; i += 1) {
      assert.ok(!(COUNT_LEAD.test(ls[i - 1]) && COUNT_LEAD.test(ls[i])), `${name}: "${ls[i - 1]}" is directly followed by "${ls[i]}"`)
    }
  }
})

test('the fully read, settled Inbox is unchanged: no unreadable group, no re-read control', async () => {
  const data = storeOf([{ criteria: 1, title: 'Add dark mode' }])
  const state = await read(data, new Map())
  const html = M.renderInbox(state)
  assert.equal(sectionOf(html, 'unreadable'), '')
  assert.ok(!html.includes(U.action))
  assert.ok(!lines(html).includes(U.attention_heading), 'no extra label when nothing is unreadable')
  assert.match(plain(html), /Nothing needs your attention right now/)
  assert.equal(primaries(html).length, 1)
})

/* ── Read it again, clicked, in the real mounted Inbox ───────────────── */

const GLOBALS = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'fetch']
const settle = () => new Promise((r) => setTimeout(r, 10))
async function until(fn, label) {
  for (let i = 0; i < 200; i += 1) { if (fn()) return; await settle() }
  assert.fail('timed out waiting for ' + label)
}

test('Read it again re-reads with GET only, and a read that now succeeds settles the Change', async () => {
  const data = storeOf([{ criteria: 1, title: 'Add dark mode' }])
  const detail = M.CHANGE(data.all[0].id)
  const failing = new Map([[detail, 'refused']])
  const requests = []
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/', pretendToBeVisual: true })
  const w = dom.window
  const held = GLOBALS.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of GLOBALS) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  set('fetch', fakeFetch(routesOf(data), failing, requests))
  const root = M.createRoot(w.document.getElementById('root'))
  try {
    M.flushSync(() => root.render(M.React.createElement(M.Inbox)))
    const d = w.document
    await until(() => d.querySelector('[data-section="unreadable"]'), 'the unreadable group')
    const button = d.querySelector('[data-section="unreadable"] button.ws-inbox-primary')
    assert.ok(button, 'the one primary is in the group')
    assert.equal(button.textContent, U.action)
    const before = requests.length
    failing.clear()
    button.dispatchEvent(new w.MouseEvent('click', { bubbles: true }))
    await until(() => !d.querySelector('[data-section="unreadable"]') && /Nothing needs your attention right now/.test(d.body.textContent), 'the re-read')
    const again = requests.slice(before)
    assert.ok(again.some(({ url }) => url === detail), 'the click read the Change again')
    assert.ok(again.every(({ method }) => method === 'GET'), 'the click wrote nothing')
  } finally {
    root.unmount()
    for (const [n, desc] of held) { if (desc) Object.defineProperty(globalThis, n, desc); else delete globalThis[n] }
  }
})

function fill(template, values) {
  return String(template).replace(/\{(\w+)\}/g, (_, k) => (k in values ? String(values[k]) : `{${k}}`))
}
