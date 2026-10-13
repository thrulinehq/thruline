/* BRIEF-COUNT-AGREES (launch-critical) — the top of the Brief, the list under it
 * and the Inbox row give ONE count of what still needs evidence.
 *
 * Found by the C12 verifier's walk (gates/verify-MERGE-SEAM-C12, Item 8 step 6)
 * on a real `thruline ui`: a Change with two accepted lines, line 1 with a
 * linked passing run that was never assessed, line 2 with nothing. The screen
 * said three different things for one noun phrase —
 *
 *   the top of the Brief:  "1 of 2 things that count as done have evidence. 1 still needs it."
 *   the list under it:     "2 things still need evidence:"
 *   the Inbox row:         "1 still needs it", beside "2 items on this Change need you."
 *
 * The top is built from the coverage (does any evidence bear on the line), the
 * list from the served readiness (what stands between this version and
 * approval), and NEVER-ASSESSED-ON-SCREEN only made them agree when the
 * coverage count was zero. The rule these tests hold: where the served
 * readiness says the version is not ready and names what is missing, the
 * number the top states of what still needs evidence IS that answer's number,
 * and the top never counts a line the answer names as one that has evidence.
 * Where the two already agree, the top is the sentence it always was, byte for
 * byte — nothing is rewritten that was already true.
 *
 * Read through `readChange`, `readPacket` and `readChangeDocuments` — the
 * browser's read paths, with `fetch` answered from the walk's own capture
 * (`walk-capture.fixture.json`) — so the Change screen (Brief and Explore), the
 * whole-story page, the Inbox row and the exported files are all read from one
 * derivation, and each fixture is the capture with the served readiness set to
 * what the engine serves for the case named.
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

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-brief-count-agrees-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeShell, readChange, readChangeDocuments } from './change-workspace.jsx';
  import { PacketPage, readPacket } from './packet.jsx';
  import { assembleInbox, InboxItem } from './inbox.jsx';
  import { renderExport } from './packet-export.js';
  import { HQ_COPY } from './hq-copy.js';
  export { readChange, readChangeDocuments, readPacket, assembleInbox, renderExport, HQ_COPY };
  export const renderRoute = props => renderToStaticMarkup(React.createElement(ChangeShell, props));
  export const renderPage = props => renderToStaticMarkup(React.createElement(PacketPage, props));
  export const renderRow = props => renderToStaticMarkup(React.createElement('ul', null, React.createElement(InboxItem, props)));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'brief-count-agrees.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const CAPTURE = JSON.parse(readFileSync(join(here, 'walk-capture.fixture.json'), 'utf8'))
const ID = CAPTURE.change_id
const clone = (v) => JSON.parse(JSON.stringify(v))
const noop = () => {}

async function readMoment(edit) {
  const reads = clone(CAPTURE.moments.after_pass)
  if (edit) edit(reads)
  const real = globalThis.fetch
  globalThis.fetch = async (url) => {
    const r = reads[String(url)]
    const headers = { get: (h) => (!r ? null : h === 'X-ThruLine-Exit' ? r.exit : h === 'X-ThruLine-Projection' ? r.projection : null) }
    if (!r) return { ok: false, status: 404, headers, text: async () => '' }
    return { ok: r.status >= 200 && r.status < 300, status: r.status, headers, text: async () => JSON.stringify(r.body) }
  }
  try {
    return { screen: await M.readChange(ID), packet: await M.readPacket(ID), docs: await M.readChangeDocuments(ID) }
  } finally {
    globalThis.fetch = real
  }
}

const COV = (reads) => reads[`/api/changes/${ID}/coverage`].body
const dom = (html) => new JSDOM('<!doctype html><body>' + html + '</body>').window.document
const route = (model, altitude) => M.renderRoute({
  model, stage: 'define', altitude, register: 'manager', onGo: noop, onCopy: noop, onReload: noop, onAltitude: noop, onRegister: noop,
})
const each = (html, selector) => [...dom(html).querySelectorAll(selector)].map((n) => n.textContent.replace(/\s+/g, ' ').trim())

/* ── the fixtures: n accepted lines, each in one of the engine's outcomes ── */

const WORDS = ['When the cart is empty, the total shows zero.', 'The total shows the sum of the prices.', 'A price with no tax shows as it is.']

/**
 * The capture, set to a Change with the lines given, each `{ outcome }`:
 *   satisfied      — a linked passing run, and the engine says it is met
 *   never_assessed — a linked passing run, and the engine says whether it
 *                    still applies was never assessed (R-2026-09-28-25)
 *   nothing_looked — no run at all
 *   waived         — set aside by a named person, nobody checked
 * `identity: false` leaves the candidate identity unconfirmed (an obligation
 * that belongs to no accepted line).
 */
function shape(lines, { identity = true } = {}) {
  return (reads) => {
    const change = reads[`/api/changes/${ID}`].body.change
    change.accepted_scope.criteria = lines.map((_, i) => WORDS[i])
    const c = COV(reads)
    const model = clone(c.coverage.criteria[0])
    const none = { items: [], state: 'none', state_slug: 'coverage_none' }
    c.coverage.criteria = lines.map((l, i) => ({
      ...clone(model),
      index: i + 1,
      text: WORDS[i],
      ...(l.outcome === 'satisfied' || l.outcome === 'never_assessed' ? {} : none),
    }))
    const idOb = c.readiness.obligations[0]
    const lineOb = c.readiness.obligations[1]
    const obligations = [
      identity ? idOb : { ...idOb, outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'inputs_missing', unsatisfied_slug: 'readiness_inputs_missing' },
      ...lines.map((l, i) => {
        const o = { ...clone(lineOb), id: 'criterion:' + (i + 1), criterion_index: i + 1, text: WORDS[i] }
        if (l.outcome === 'satisfied') return o
        if (l.outcome === 'never_assessed') return { ...o, outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'never_assessed', unsatisfied_slug: 'readiness_never_assessed', evidence_name: null }
        if (l.outcome === 'nothing_looked') return { ...o, outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked', evidence_name: null }
        return { ...o, outcome: 'waived', outcome_slug: 'waived', evidence_name: null, waiver: { actor: 'sam', authority: 'the accepted profile', rationale: 'Nobody checked this.' } }
      }),
    ]
    const missing = obligations.filter((o) => o.outcome === 'needs_evidence')
    c.readiness.obligations = obligations
    c.readiness.ready = false
    c.readiness.headline_slug = missing.length === 1 ? 'readiness_needs_evidence_one' : 'readiness_needs_evidence'
    c.readiness.missing = missing.map((o) => o.id)
    c.readiness.waived = obligations.filter((o) => o.outcome === 'waived').map((o) => o.id)
    c.readiness.rests_on = obligations.filter((o) => o.outcome === 'satisfied' && o.criterion_index).map((o) => ({ obligation_id: o.id, text: o.text, evidence_name: o.evidence_name }))
  }
}

/* ── what a person reads, on every surface that states the count ────────── */

const stillNeed = (sentence) => {
  const m = /(\d+) (?:things? )?still needs?\b/.exec(sentence)
  return m ? Number(m[1]) : null
}
const itemsOn = (sentence) => {
  const m = /(\d+) items? on this Change need/.exec(sentence)
  return m ? Number(m[1]) : null
}

async function read(edit) {
  const { screen, packet, docs } = await readMoment(edit)
  const hq = screen.model.hq
  const inbox = M.assembleInbox([docs.change], new Map([[docs.change.id, docs]]))
  assert.equal(inbox.items.length, 1, 'the Change is on the Inbox')
  const item = inbox.items[0]
  const row = M.renderRow({ item })
  const brief = route(screen.model, 'brief')
  const page = M.renderPage({ model: packet.model, notes: packet.notes, onCopy: noop, records: packet.records })
  const tops = {
    brief: each(brief, '.ws-change-conclusion'),
    explore: each(route(screen.model, 'explore'), '.ws-change-conclusion'),
    page: each(page, '.ws-change-conclusion'),
    row: [item.reasonText],
  }
  return {
    hq, packet, item, tops,
    list: each(brief, '.ws-readiness-sentence'),
    listOnExplore: each(route(screen.model, 'explore'), '.ws-readiness-sentence'),
    rowItems: each(row, '.ws-inbox-attention'),
    rowReason: each(row, '.ws-inbox-reason'),
  }
}

/* ── the cases ───────────────────────────────────────────────────────────── */

const NEEDS = (total, n) => `Accepted: ${total} things count as done. ${n === 1 ? '1 thing still needs' : n + ' things still need'} evidence.`
const LIST = (n) => (n === 1 ? '1 thing still needs evidence:' : n + ' things still need evidence:')

/* Where the readiness answer and the coverage disagree about the count or about
 * WHICH lines: the top is the answer's own clause. */
const REWRITTEN = [
  {
    name: 'the walk: line 1 never assessed (it has a linked passing run), line 2 nothing looked',
    edit: shape([{ outcome: 'never_assessed' }, { outcome: 'nothing_looked' }]),
    never: true,
    top: NEEDS(2, 2), list: LIST(2), items: 2, n: 2,
  },
  {
    name: 'waived plus never assessed: line 1 set aside, line 2 has a run but was never assessed',
    edit: shape([{ outcome: 'waived' }, { outcome: 'never_assessed' }]),
    /* The counts happen to be equal (1 and 1), and they are about opposite lines:
     * the coverage counts the set-aside line as the one that "still needs it" and
     * the never-assessed one as the one that "has evidence". */
    never: true,
    top: NEEDS(2, 1), list: LIST(1), items: 1, n: 1,
  },
  {
    name: 'two set aside and one never assessed: the coverage counts two lines that need nothing',
    edit: shape([{ outcome: 'waived' }, { outcome: 'waived' }, { outcome: 'never_assessed' }]),
    never: true,
    top: NEEDS(3, 1), list: LIST(1), items: 1, n: 1,
  },
  {
    name: 'a line met, a line nothing looked at, and the identity unconfirmed: a missing thing that belongs to no line',
    edit: shape([{ outcome: 'satisfied' }, { outcome: 'nothing_looked' }], { identity: false }),
    /* D-050 (BRIEF-COUNT-AGREES-R2, finding 10). This read "Accepted: 2 things count as done. 2 things still need
     * evidence." over "2 things still need evidence:", and the second "thing" was the identity, which is not one of
     * the things that count as done. The identity is said on its own now: one LINE still needs evidence, and the
     * Inbox counts two items (the line, and the identity). The engine still names two missing obligations
     * (`readiness.missing`); `missingCount` is the count the sentences state, of the lines. */
    top: NEEDS(2, 1) + ' Nothing is written down yet about what will be looked at for this Change.', list: LIST(1), items: 2, n: 1,
  },
  {
    name: 'one line met, one never assessed, one nothing looked',
    edit: shape([{ outcome: 'satisfied' }, { outcome: 'never_assessed' }, { outcome: 'nothing_looked' }]),
    never: true,
    top: NEEDS(3, 2), list: LIST(2), items: 2, n: 2,
  },
]

/* Where they already agree, the top is what it always was. */
const UNTOUCHED = [
  {
    name: 'a line met, a line nothing looked at',
    edit: shape([{ outcome: 'satisfied' }, { outcome: 'nothing_looked' }]),
    top: '1 of 2 things that count as done have evidence. 1 still needs it.', list: LIST(1), items: 1, n: 1,
  },
  {
    name: 'two lines met, one nothing looked at',
    edit: shape([{ outcome: 'satisfied' }, { outcome: 'satisfied' }, { outcome: 'nothing_looked' }]),
    top: '2 of 3 things that count as done have evidence. 1 still needs it.', list: LIST(1), items: 1, n: 1,
  },
  {
    name: 'one line met, two nothing looked at',
    edit: shape([{ outcome: 'satisfied' }, { outcome: 'nothing_looked' }, { outcome: 'nothing_looked' }]),
    top: '1 of 3 things that count as done have evidence. 2 still need it.', list: LIST(2), items: 2, n: 2,
  },
  {
    name: 'the one-line case NEVER-ASSESSED-ON-SCREEN pins: its one line never assessed',
    edit: shape([{ outcome: 'never_assessed' }]),
    never: true,
    top: NEEDS(1, 1).replace('things count', 'thing counts'), list: LIST(1), items: 1, n: 1,
  },
  {
    name: 'both lines never assessed',
    edit: shape([{ outcome: 'never_assessed' }, { outcome: 'never_assessed' }]),
    never: true,
    top: NEEDS(2, 2), list: LIST(2), items: 2, n: 2,
  },
]

for (const kind of [{ label: 'agree', cases: REWRITTEN }, { label: 'unmoved', cases: UNTOUCHED }]) {
  for (const c of kind.cases) {
    test(`${kind.label}: ${c.name} — the top, the list and the Inbox row state the same count, ${c.n}`, async () => {
      const r = await read(c.edit)
      /* The served answer is what the case says: not ready, and this many missing. */
      assert.equal(r.hq.readiness.ready, false)
      assert.equal(r.hq.readiness.missingCount, c.n, 'the served readiness names this many lines')
      /* THE CLAIM: the numbers, read back out of the sentences a person reads, are one number. */
      const numbers = [stillNeed(r.tops.brief[0]), stillNeed(r.list[0]), stillNeed(r.tops.row[0]), itemsOn(r.rowItems[0])]
      assert.deepEqual(
        numbers,
        [c.n, c.n, c.n, c.items],
        `the top says "${r.tops.brief[0]}", the list says "${r.list[0]}", the Inbox row says "${r.tops.row[0]}" and "${r.rowItems[0]}"`,
      )
      /* And the words: every surface that prints the top prints the one sentence. */
      assert.deepEqual(r.tops.brief, [c.top], 'Brief')
      assert.deepEqual(r.tops.explore, [c.top], 'Explore')
      assert.deepEqual(r.tops.page, [c.top], 'the whole-story page')
      assert.deepEqual(r.tops.row, [c.top], 'the Inbox row')
      assert.equal(r.hq.headline.text, c.top)
      /* The list under it says its own count. */
      assert.deepEqual(r.list, [c.list], 'the list under the top')
      assert.deepEqual(r.listOnExplore, [c.list])
      assert.equal(itemsOn(r.rowItems[0]), c.items, 'the Inbox row says how many items need you')
      assert.equal(r.item.attention.items, c.items)
    })
  }
}

test('a line the readiness names is never counted, by the top, as one that has evidence, and never read as done', async () => {
  for (const c of [...REWRITTEN, ...UNTOUCHED].filter((x) => x.never)) {
    const r = await read(c.edit)
    for (const [surface, top] of Object.entries(r.tops)) {
      assert.doesNotMatch(top[0], /have evidence|has evidence|0 still need/, `${c.name}: ${surface} says a line that was never assessed has evidence`)
    }
  }
})

test('the exported files say the same top, and nothing else about the count', async () => {
  const c = REWRITTEN[0]
  const { packet } = await readMoment(c.edit)
  for (const format of ['md', 'json']) {
    const file = M.renderExport(packet.model.hq.packet, packet.records, { format }).text
    assert.ok(file.includes(c.top), `${format} carries the top: ${c.top}`)
    assert.ok(!file.includes('1 of 2 things that count as done have evidence. 1 still needs it.'), `${format} no longer says one still needs it`)
  }
})

test('the counts on the conclusion are the coverage\'s, untouched: only the sentence follows the served answer', async () => {
  const { screen } = await readMoment(REWRITTEN[0].edit)
  const hq = screen.model.hq
  assert.equal(hq.counts.withEvidence, 1, 'the coverage counts line 1 as having a run')
  assert.equal(hq.counts.withoutEvidence, 1, 'and line 2 as having none')
  assert.equal(hq.readiness.missingCount, 2, 'the answer says both need evidence')
})

test('a ready answer with a line set aside says so, and no answer, a problem: the top is what it was', async () => {
  /* Ready with a line set aside keeps the top it had (its own pin in never-assessed-on-screen.test.js). */
  const ready = await readMoment((reads) => {
    shape([{ outcome: 'satisfied' }, { outcome: 'waived' }])(reads)
    const c = COV(reads)
    c.readiness.ready = true
    c.readiness.with_exceptions = true
    c.readiness.headline_slug = 'readiness_ready_with_exceptions'
    c.readiness.missing = []
  })
  assert.equal(ready.screen.model.hq.readiness.ready, true)
  /* D-050 (BRIEF-COUNT-AGREES-R2, finding 7). This read "1 of 2 … 1 still needs it." beside "Ready for approval with
   * exceptions": the line it counted as one that still needs evidence was the one set aside, which nobody is to check. */
  assert.equal(ready.screen.model.hq.headline.text, '1 of 2 things that count as done have evidence. 1 was set aside, with nobody checking.')
  /* No readiness served: absence is not an answer. */
  const none = await readMoment((reads) => { shape([{ outcome: 'satisfied' }, { outcome: 'nothing_looked' }])(reads); delete COV(reads).readiness })
  assert.equal(none.screen.model.hq.readiness.absent, true)
  assert.equal(none.screen.model.hq.headline.text, '1 of 2 things that count as done have evidence. 1 still needs it.')
})
