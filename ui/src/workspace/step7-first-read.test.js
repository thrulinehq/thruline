/* STEP-7-FIRST-READ (path step 7, PW-F12; founder ruling #21 (a): "The
 * read-time git line, cautious direction only, with its receipt shown.")
 *
 * STEP-7-GIT-LINE put the served `code_moved` into the one derived model, and
 * Brief, Explore and the whole story print it. What a person reads FIRST after
 * the agent has committed is the Inbox card, then the Change's top — and the
 * Inbox card printed nothing about it. This file holds three things:
 *
 *   1. The Inbox card for a Change whose served `code_moved` says the code
 *      moved carries ONE cautious clause, from the same derivation
 *      (`hq.codeMoved`), and no card says anything when the field is absent,
 *      `null`, or names equal commits — silence, never "unchanged".
 *   2. It claims less, by render: with the clause taken out, every Inbox card
 *      and the Change's Brief are byte-identical with the field on and off —
 *      readiness, counts, state words, decision standing and approval.
 *   3. The Change's own Refresh re-reads the served Change, so a move that
 *      happened while the Change was open reaches Brief's top, with its
 *      receipt one level down — and pressing it sends nothing but GETs.
 *
 * Every value is synthetic; no string here came off a real machine.
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
const temporary = await mkdtemp(join(tmpdir(), 'thruline-step7-first-read-'))
after(() => rm(temporary, { recursive: true, force: true }))

const out = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  stdin: {
    contents: `
      import React from 'react';
      import { renderToStaticMarkup } from 'react-dom/server';
      import { createRoot } from 'react-dom/client';
      import { flushSync } from 'react-dom';
      import { InboxScreen, assembleInbox } from './inbox.jsx';
      import ChangeWorkspace from './change-workspace.jsx';
      import { HQ_COPY } from './hq-copy.js';
      import { fill } from './hq-narrate.js';
      import { aStore } from './attention-count.fixture.js';
      import { A11Y_CHANGE, A11Y_CHANGE_ID, A11Y_CHANGE_ROUTES } from './a11y-fixture.js';
      export { React, createRoot, flushSync, ChangeWorkspace, assembleInbox, HQ_COPY, fill, aStore, A11Y_CHANGE, A11Y_CHANGE_ID, A11Y_CHANGE_ROUTES };
      export const renderInbox = (state) => renderToStaticMarkup(React.createElement(InboxScreen, { state }));
    `,
    resolveDir: here,
  },
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
})
const file = join(temporary, 'step7.mjs')
await writeFile(file, out.outputFiles[0].text)
const M = await import(pathToFileURL(file).href)
const CM = M.HQ_COPY.change.model.code_moved

const MOVED = {
  recorded: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0',
  now: 'f0e1d2c3b4a5968778695a4b3c2d1e0f9a8b7c6d',
  changed_paths: ['src/lib.rs'],
  commits: 1,
  uncommitted_compared: false,
}
const EQUAL = { ...MOVED, now: MOVED.recorded }
const CLAIMS_MORE = /unchanged|nothing changed|nothing has changed|not changed|still applies|up to date/i
/** On a whole page, where "Still applies" is a state word about linked evidence and not about the code. */
const PAGE_CLAIMS_MORE = /unchanged|nothing changed|nothing has changed|not changed|up to date/i
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&')
const textOf = (html) => unescape(html.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ')

/** The same generated store, with `code_moved` set on every Change as given. */
function storeWith(seed, moved) {
  const { all, docsById } = M.aStore(seed, { count: 6 })
  const docs = new Map()
  for (const [id, d] of docsById) docs.set(id, moved === undefined ? d : { ...d, change: { ...d.change, code_moved: moved } })
  return { all, docs }
}
const inboxOf = (seed, moved) => {
  const { all, docs } = storeWith(seed, moved)
  const state = { ...M.assembleInbox(all, docs), storage: { known: false }, notes: [] }
  return { state, html: M.renderInbox(state), docs }
}
/** Each card, by Change id. */
const cardsOf = (html) => new Map([...html.matchAll(/<li class="ws-inbox-item" data-change="([^"]+)">([\s\S]*?)<\/li>(?=<li class="ws-inbox-item"|<\/ul>)/g)].map((m) => [m[1], m[2]]))
/** The one clause the card may carry, as its own element. */
const CLAUSE_RE = /<p class="ws-inbox-code-moved">([^<]*)<\/p>/g
const SEEDS = [7101, 7102, 7103, 7104, 7105, 7106, 7107, 7108]

test('STEP-7-FIRST-READ · the Inbox card says, in one cautious clause from the same derivation, that the code changed since the version it names', () => {
  let cards = 0
  for (const seed of SEEDS) {
    const { state, html } = inboxOf(seed, MOVED)
    const byId = cardsOf(html)
    for (const item of state.items) {
      const card = byId.get(item.id)
      assert.ok(card != null, `seed ${seed}: no card drawn for ${item.id}`)
      const clauses = [...card.matchAll(CLAUSE_RE)].map((m) => unescape(m[1]))
      const short = item.hq && item.hq.version ? item.hq.version.short : null
      const expected = (short ? M.fill(CM.lead_named, { version: short }) : CM.lead_unnamed) + '.'
      assert.deepEqual(clauses, [expected], `seed ${seed}, ${item.id}: the card does not carry exactly one moved clause: ${textOf(card)}`)
      assert.ok(item.hq.codeMoved && item.hq.codeMoved.sentence.startsWith(expected.slice(0, -1)), 'the clause is not the moved sentence\'s own lead')
      assert.doesNotMatch(clauses[0], CLAIMS_MORE)
      cards += 1
    }
  }
  assert.ok(cards > 0, 'no store drew a card at all')
})

test('STEP-7-FIRST-READ · no field, a null field or equal commits: no card says anything about the code, and nothing reads as "unchanged"', () => {
  for (const seed of SEEDS) {
    const base = inboxOf(seed, undefined).html
    for (const moved of [null, EQUAL]) {
      const { html } = inboxOf(seed, moved)
      assert.equal(html, base, `seed ${seed}: the Inbox differs from the base with ${JSON.stringify(moved)}`)
      assert.equal([...html.matchAll(CLAUSE_RE)].length, 0)
      assert.doesNotMatch(textOf(html), PAGE_CLAIMS_MORE)
    }
  }
})

test('STEP-7-FIRST-READ · unapproved cards keep their exact reading; a standing approval on moved code gets attention', () => {
  let approvedSeen = 0
  for (const seed of SEEDS) {
    const on = inboxOf(seed, MOVED)
    const off = inboxOf(seed, null)
    const approved = new Set([...on.docs].filter(([, d]) => d.change.decisions && d.change.decisions.standing).map(([id]) => id))
    const onItems = new Map(on.state.items.map((i) => [i.id, i]))
    const offItems = new Map(off.state.items.map((i) => [i.id, i]))
    const onCards = cardsOf(on.html)
    const offCards = cardsOf(off.html)
    for (const [id, item] of offItems) {
      if (approved.has(id)) continue
      const moved = onItems.get(id)
      assert.ok(moved, `seed ${seed}: an unapproved card disappeared: ${id}`)
      assert.equal(onCards.get(id).replace(CLAUSE_RE, ''), offCards.get(id), `seed ${seed}: an unapproved card changed beyond the code clause: ${id}`)
      const stable = (i) => JSON.stringify([i.reason, i.reasonText, i.also, i.attention, i.href, i.hq && i.hq.inboxRow, i.hq && i.hq.readiness, i.hq && i.hq.decision, i.hq && i.hq.word])
      assert.equal(stable(moved), stable(item), `seed ${seed}: an unapproved derived field moved: ${id}`)
    }
    for (const id of approved) {
      approvedSeen += 1
      const item = onItems.get(id)
      assert.ok(item, `seed ${seed}: approved code moved but has no Inbox card: ${id}`)
      assert.ok([item.reason.id, ...item.also.map((r) => r.reason.id)].includes('approved_code_moved'), `seed ${seed}: the card does not name the old approval: ${id}`)
      assert.equal(item.hq.decision.approved, true, `seed ${seed}: the standing approval was lost: ${id}`)
      assert.equal(item.hq.codeMoved.recorded, MOVED.recorded, `seed ${seed}: the card used another code comparison: ${id}`)
    }
    /* SEAM-INBOX-APPROVED-VERIFY advisory 4c — restored for approved Changes,
     * moved and not moved: the served move adds the one reason and the clause,
     * and moves nothing else the record says — readiness, the decision and the
     * state word are the same with the field on and off. */
    const modelOf = (state) => new Map(state.changes.map((c) => [c.id, c.hq]))
    const onModels = modelOf(on.state)
    const offModels = modelOf(off.state)
    for (const id of approved) {
      const a = onModels.get(id)
      const b = offModels.get(id)
      assert.ok(a && b, `seed ${seed}: an approved Change was not read: ${id}`)
      assert.deepEqual(a.readiness, b.readiness, `seed ${seed}: readiness moved with the served move: ${id}`)
      assert.deepEqual(a.decision, b.decision, `seed ${seed}: the decision moved with the served move: ${id}`)
      assert.deepEqual(a.word, b.word, `seed ${seed}: the state word moved with the served move: ${id}`)
      assert.equal(a.decision.approved, true, `seed ${seed}: ${id}`)
      /* Not moved (no served move, approval on the version the record stands at): no approved-move reason. */
      const still = offItems.get(id)
      if (!b.decision.movedSince) {
        assert.ok(!still || ![still.reason.id, ...still.also.map((r) => r.reason.id)].includes('approved_code_moved'),
          `seed ${seed}: an approval on unmoved code was called moved: ${id}`)
      }
    }
    assert.equal(on.state.total, off.state.total)
    assert.equal(on.state.unread, off.state.unread)
  }
  assert.ok(approvedSeen > 0, 'the generated stores never exercised a standing approval')
})

/* ── the Change, mounted: open, then Refresh after the code moved ────── */

const settle = () => new Promise((r) => setTimeout(r, 10))
const GLOBALS = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage', 'CSS', 'fetch']

/** A local server's answers for the fixture Change, with `code_moved` as `moved()` says now. */
function server(moved) {
  const calls = []
  const fetch = async (url, init) => {
    const path = String(url).replace(/^https?:\/\/[^/]+/, '').split('?')[0]
    calls.push({ path, method: (init && init.method) || 'GET' })
    let doc = Object.prototype.hasOwnProperty.call(M.A11Y_CHANGE_ROUTES, path) ? M.A11Y_CHANGE_ROUTES[path] : null
    if (doc && path === '/api/changes/' + M.A11Y_CHANGE_ID) {
      const m = moved()
      doc = { change: m === undefined ? M.A11Y_CHANGE : { ...M.A11Y_CHANGE, code_moved: m } }
    }
    return doc
      ? { ok: true, status: 200, statusText: 'OK', headers: { get: () => null }, text: async () => JSON.stringify(doc), json: async () => doc }
      : { ok: false, status: 404, statusText: 'Not Found', headers: { get: () => null }, text: async () => '{}', json: async () => ({}) }
  }
  return { fetch, calls }
}

async function mountedChange(fetch, run) {
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/#/changes/' + M.A11Y_CHANGE_ID, pretendToBeVisual: true })
  const w = dom.window
  const held = GLOBALS.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of GLOBALS) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  set('fetch', fetch)
  const root = M.createRoot(w.document.getElementById('root'))
  try {
    M.flushSync(() => root.render(M.React.createElement(M.ChangeWorkspace, { id: M.A11Y_CHANGE_ID })))
    for (let i = 0; i < 60; i += 1) await settle()
    return await run({ w, d: w.document })
  } finally {
    root.unmount()
    for (const [n, desc] of held) {
      if (desc) Object.defineProperty(globalThis, n, desc)
      else delete globalThis[n]
    }
    w.close()
  }
}

/** Brief's story and Inspector, as HTML — everything the top shows but the read-time stamp. */
const briefOf = (d) => {
  const story = d.querySelector('.ws-brief-story')
  const inspector = d.querySelector('.ws-brief-inspector')
  return { story, html: (story ? story.outerHTML : '') + (inspector ? inspector.outerHTML : '') }
}
const refresh = async (d) => {
  const b = [...d.querySelectorAll('button.ws-shell-refresh')].find((x) => x.textContent.trim() === M.HQ_COPY.shell.topbar.refresh)
  assert.ok(b, 'the Change has no Refresh in its top bar')
  b.click()
  for (let i = 0; i < 60; i += 1) await settle()
}

test('STEP-7-FIRST-READ · the Change\'s own Refresh re-reads the served Change: a move made while it was open reaches Brief\'s top, with its receipt one level down, and Refresh sends only GETs', async () => {
  let moved = undefined
  const s = server(() => moved)
  await mountedChange(s.fetch, async ({ d }) => {
    const before = briefOf(d)
    assert.ok(before.story, 'Brief did not open')
    assert.doesNotMatch(before.story.textContent, /The code has changed/, 'Brief says the code moved before it did')
    const reads = s.calls.filter((c) => c.path === '/api/changes/' + M.A11Y_CHANGE_ID).length
    moved = MOVED
    await refresh(d)
    assert.equal(s.calls.filter((c) => c.path === '/api/changes/' + M.A11Y_CHANGE_ID).length, reads + 1, 'Refresh did not read the served Change again')
    const top = briefOf(d).story
    const lead = M.fill(CM.lead_named, { version: '' }).split(' ').slice(0, 5).join(' ')
    const line = [...top.querySelectorAll('p.ws-change-code-limit')].find((p) => p.textContent.startsWith(lead))
    assert.ok(line, `after Refresh, Brief's top does not say the code moved: ${top.textContent}`)
    assert.ok(line.textContent.includes('src/lib.rs'), line.textContent)
    const receipt = top.querySelector('details.ws-change-code-receipt')
    assert.ok(receipt && receipt.textContent.includes(MOVED.recorded) && receipt.textContent.includes(MOVED.now), 'the receipt is not one level down after Refresh')
    assert.deepEqual([...new Set(s.calls.map((c) => c.method))], ['GET'], 'opening or refreshing the Change sent something other than a GET')
  })
})

test('STEP-7-FIRST-READ · claims less on the Change: with the moved line and its receipt taken out, Brief (story and Inspector) is byte-identical with the field on and off', async () => {
  let moved = null
  const s = server(() => moved)
  await mountedChange(s.fetch, async ({ d }) => {
    const off = briefOf(d).html
    moved = MOVED
    await refresh(d)
    const on = briefOf(d)
    const sentence = [...on.story.querySelectorAll('p.ws-change-code-limit')].map((p) => p.textContent).find((t) => /src\/lib\.rs/.test(t))
    assert.ok(sentence, 'no moved line to take out')
    const esc = (t) => t.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    /* MERGE-SEAM-C12 — the union with APPLIES-VS-MOVED (cloud/path-ui-2, merged after this lineage): while the
     * served Change says the code moved, exactly two more sentences on Brief name their frame — the Inspector's
     * "Still applies" line, and the agent section's covered line, which gains one trailing clause. Both are
     * code_moved-dependent by that card's design, so they are taken out with the moved line and the receipt:
     * the framed Inspector line is put back to the base line the field-off page shows, in the same place, and
     * the trailing clause is removed. Each must be PRESENT (spelled from the copy file, not from a literal), so
     * the stripping cannot hide a frame that went missing; every other byte must still be identical. */
    const AM = M.HQ_COPY.change.model.applies_as_recorded
    const TELL = M.HQ_COPY.change.tell
    const FRESH = /<p class="ws-cov-line-fresh">([^<]*)<\/p>/g
    const baseFresh = [...off.matchAll(FRESH)].map((m) => m[1])
    const onFresh = [...on.html.matchAll(FRESH)].map((m) => m[1])
    assert.equal(onFresh.length, baseFresh.length, 'the moved field changed how many applicability lines Brief shows')
    const framedLine = (form) => new RegExp('^' + form.replace(/[.*+?^${}()|[\]\\]/g, '\\$&').replace('\\{version\\}', '[^<\\s]+') + '$')
    let framedCount = 0
    let unframed = on.html
    onFresh.forEach((line, i) => {
      if (line === baseFresh[i]) return
      assert.ok(framedLine(AM.named).test(line) || framedLine(AM.unnamed).test(line), `an applicability line changed to something other than its frame: ${line}`)
      framedCount += 1
      unframed = unframed.replace('<p class="ws-cov-line-fresh">' + line + '</p>', '<p class="ws-cov-line-fresh">' + baseFresh[i] + '</p>')
    })
    assert.ok(framedCount > 0, 'while the code has moved, no applicability line on Brief names its frame')
    assert.equal(unframed.split(' ' + TELL.covered_moved).length, 2, 'the agent section does not carry the frame clause exactly once')
    unframed = unframed.split(' ' + TELL.covered_moved).join('')
    const stripped = unframed
      .split('<p class="ws-change-muted ws-change-code-limit">' + esc(sentence) + '</p>').join('')
      .replace(/<details class="ws-change-technical ws-change-code-receipt">[\s\S]*?<\/details>/g, '')
    assert.equal(stripped, off, 'the moved line changed something else on Brief')
    /* And back: a later read with equal commits says nothing again — never "unchanged". */
    moved = EQUAL
    await refresh(d)
    assert.equal(briefOf(d).html, off)
    assert.doesNotMatch(d.body.textContent, PAGE_CLAIMS_MORE)
  })
})
