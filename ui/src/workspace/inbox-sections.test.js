import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* A3-F14 — the Inbox by whose turn it is (report line 1181, Gerrit's
 * attention set): Waiting on you · Needs a check · Waiting on the agent ·
 * Recording now · Nothing needed, in that fixed order, each headed with its
 * own count. "Waiting on the agent" and "Recording now" have no field in the
 * record behind them yet, so they ship disabled with their reason and what
 * would turn them on — never holding an item, never stating a number.
 *
 * Every store is generated (`attention-count.fixture.js`); nothing here came
 * off a real machine.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-sections-'))
after(() => rm(temporary, { recursive: true, force: true }))

const out = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  stdin: {
    contents: `
      import React from 'react';
      import { renderToStaticMarkup } from 'react-dom/server';
      import { InboxScreen, assembleInbox } from './inbox.jsx';
      import { WHOSE_TURN, NEEDS_ENGINE_SECTIONS, SECTION_OF_REASON, REASONS, sectionOf, inboxSections } from './inbox-view.js';
      import { HQ_COPY } from './hq-copy.js';
      import { aStore } from './attention-count.fixture.js';
      export { assembleInbox, WHOSE_TURN, NEEDS_ENGINE_SECTIONS, SECTION_OF_REASON, REASONS, sectionOf, inboxSections, HQ_COPY, aStore };
      export const renderInbox = (state) => renderToStaticMarkup(React.createElement(InboxScreen, { state }));
    `,
    resolveDir: here,
  },
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
})
const file = join(temporary, 'sections.mjs')
await writeFile(file, out.outputFiles[0].text)
const M = await import(pathToFileURL(file).href)
const S = M.HQ_COPY.change.inbox.sections

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&')
const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
const numberFrom = (text, one, many) => (text === one ? 1 : ((m) => (m ? Number(m[1]) : null))(new RegExp('^' + escapeRe(many).replace(escapeRe('{n}'), '(\\d+)') + '$').exec(text)))

/** Each rendered section, in document order: its id, whether it is disabled, its count, its items, its words. */
function sectionsOf(html) {
  return [...html.matchAll(/<section class="ws-inbox-section[^"]*" data-section="([a-z]+)"( aria-disabled="true")?[^>]*>([\s\S]*?)<\/section>/g)].map((m) => {
    const body = m[3]
    const count = (/<(?:p|span) class="ws-inbox-section-count">([^<]*)<\/(?:p|span)>/.exec(body) || [])[1]
    return {
      id: m[1],
      disabled: Boolean(m[2]),
      count: count == null ? null : numberFrom(unescape(count), S.count_one, S.count_many),
      items: [...body.matchAll(/<li class="ws-inbox-item[^"]*" data-change="([^"]+)"/g)].map((x) => x[1]),
      text: unescape(body.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' '),
    }
  })
}

const SEEDS = Array.from({ length: 200 }, (_, k) => 5000 + k)

test('F14 · the whose-turn sections are the report’s five, in its order, and every reason belongs to exactly one', () => {
  assert.deepEqual([...M.WHOSE_TURN], ['you', 'check', 'agent', 'recording', 'nothing'])
  assert.deepEqual([...M.NEEDS_ENGINE_SECTIONS], ['agent', 'recording'])
  for (const reason of M.REASONS) {
    assert.ok(['you', 'check'].includes(M.SECTION_OF_REASON[reason]), `${reason} has no section a person can act from`)
  }
  assert.equal(M.SECTION_OF_REASON.awaiting_approval, 'you')
  assert.equal(M.SECTION_OF_REASON.awaiting_definition, 'you')
  /* Missing evidence never satisfies a check: a line with nothing behind it is "Needs a check", never "Nothing needed". */
  assert.equal(M.SECTION_OF_REASON.missing_evidence, 'check')
  assert.throws(() => M.sectionOf({ reason: { id: 'a_reason_nobody_named' } }), /no section/)
})

test(`F14 · over ${SEEDS.length} generated stores: sections render in order, their counts sum to the heading, and no item is in two`, () => {
  let rendered = 0
  for (const seed of SEEDS) {
    const store = M.aStore(seed)
    const state = { ...M.assembleInbox(store.all, store.docsById), storage: { known: false }, notes: [] }
    const html = M.renderInbox(state)
    if (!state.items.length) continue
    rendered += 1
    const secs = sectionsOf(html)
    const order = secs.map((s) => s.id)
    assert.deepEqual(order, M.WHOSE_TURN.filter((id) => order.includes(id)), `seed ${seed}: sections out of order: ${order}`)
    for (const id of ['agent', 'recording']) assert.ok(order.includes(id), `seed ${seed}: the ${id} section is not present`)
    const heading = numberFrom(unescape((/<p class="ws-inbox-count">([^<]*)<\/p>/.exec(html) || [])[1] || ''), M.HQ_COPY.change.inbox.one, M.HQ_COPY.change.inbox.many)
    const attention = secs.filter((s) => s.id !== 'nothing')
    const sum = attention.reduce((n, s) => n + (s.count || 0), 0)
    assert.equal(sum, heading, `seed ${seed}: the sections sum to ${sum}; the heading says ${heading}`)
    for (const s of attention.filter((x) => !x.disabled)) assert.equal(s.count, s.items.length, `seed ${seed}: ${s.id} says ${s.count} and lists ${s.items.length}`)
    const listed = secs.flatMap((s) => s.items)
    assert.equal(new Set(listed).size, listed.length, `seed ${seed}: an item appears in two sections`)
    assert.equal(listed.length, state.items.length, `seed ${seed}: every item is in exactly one section`)
    for (const item of state.items) {
      const home = secs.find((s) => s.items.includes(item.id))
      assert.equal(home.id, M.sectionOf(item), `seed ${seed}: ${item.id} is in ${home.id}`)
    }
    const nothing = secs.find((s) => s.id === 'nothing')
    if (state.settled) {
      assert.ok(nothing, `seed ${seed}: ${state.settled} Changes need nothing and the section is absent`)
      assert.equal(nothing.count, state.settled, `seed ${seed}: the Nothing-needed count`)
      assert.match(html, /<details class="ws-inbox-section-nothing"/, 'Nothing needed is collapsed')
    }
  }
  assert.ok(rendered >= 150, `only ${rendered} stores had items`)
})

test('F14 · "Waiting on the agent" and "Recording now" are present, disabled, say why and what would turn them on, and never hold an item or a number', () => {
  const store = M.aStore(5003)
  const html = M.renderInbox({ ...M.assembleInbox(store.all, store.docsById), storage: { known: false }, notes: [] })
  for (const id of M.NEEDS_ENGINE_SECTIONS) {
    const s = sectionsOf(html).find((x) => x.id === id)
    assert.ok(s, `${id} is not rendered`)
    assert.equal(s.disabled, true, `${id} is not marked disabled`)
    assert.deepEqual(s.items, [], `${id} holds an item`)
    assert.equal(s.count, null, `${id} states a count`)
    assert.ok(s.text.includes(S[id].heading) && s.text.includes(S[id].reason) && s.text.includes(S[id].enabler), `${id} does not state its reason and enabler`)
    assert.doesNotMatch(s.text, /\d/, `${id} carries a number`)
  }
  /* The pure model agrees: no input reaches either section. */
  for (const seed of SEEDS.slice(0, 40)) {
    const st = M.assembleInbox(M.aStore(seed).all, M.aStore(seed).docsById)
    for (const s of M.inboxSections(st.items, st.settled).filter((x) => M.NEEDS_ENGINE_SECTIONS.includes(x.id))) {
      assert.equal(s.disabled, true)
      assert.equal(s.items.length, 0)
      assert.equal(s.count, null)
    }
  }
})

test('F14 · the section words are the copy file’s, and none of them blames anyone', () => {
  for (const id of M.WHOSE_TURN) assert.equal(typeof S[id].heading, 'string')
  for (const id of M.NEEDS_ENGINE_SECTIONS) {
    assert.equal(typeof S[id].reason, 'string')
    assert.match(S[id].enabler, /^What would turn it on:/)
  }
  const all = JSON.stringify(S)
  assert.doesNotMatch(all, /\b(fail(ed)?|lied|ignored|forgot|hasn.t|didn.t)\b/i, 'a section names blame')
})
