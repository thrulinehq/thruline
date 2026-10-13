/* BRIEF-COUNT-AGREES-R2 (launch-critical) — ONE count of what still needs
 * evidence, from the served readiness, on EVERY surface that states it.
 *
 * Round 1 (`brief-count-agrees.test.js`) made the top of the Brief agree with
 * the list under it and with the Inbox row, and the UI-TRUTH-PAIR adversary
 * then walked the whole-story page and Ask on a real `thruline ui` and found
 * that round 1 had moved the top to the served answer and left two surfaces on
 * the coverage: the page's own "What still needs evidence" section
 * (`packet-view.js` `stillNeedsEvidence`) and Ask's answer (`ask-hq.js`
 * `needsEvidence`). The walk's own Change — two accepted lines, line 1 linked
 * and never assessed, line 2 with nothing — read
 *
 *   the page top:    "Accepted: 2 things count as done. 2 things still need evidence."
 *   the page list:   ONE line, under "What still needs evidence"
 *   Ask:             "One of the things that count as done is still waiting for evidence."
 *
 * and, of the same class, four things that were already wrong at base:
 *
 *   (7) ready with exceptions: "1 of 2 … 1 still needs it." beside "Ready for
 *       approval with exceptions", the line it counts being the one set aside;
 *   (8) "Accepted: 3 things count as done. None has evidence yet." above
 *       "1 thing still needs evidence:" when two lines are set aside;
 *   (9) a set-aside line listed under "What still needs evidence", and a
 *       single never-assessed line's page top saying one still needs evidence
 *       above the heading "Everything here has something behind it";
 *  (10) with the identity unconfirmed, "a line met and a line nothing looked at"
 *       reading "2 things still need evidence" — the second "thing" is the
 *       identity, not a line, and a person reads "things that count as done" as
 *       the accepted lines and nothing else.
 *
 * WHAT THIS FILE HOLDS. There is one definition of "how many of the things
 * that count as done still need evidence", and it is the served readiness's:
 * a line the answer says needs evidence, and no other. A set-aside line (waived
 * or does not apply) is never one. A line whose applicability was never
 * assessed always is. The identity (and a missing review) is never counted as
 * one of the lines that count as done: when it is unconfirmed it is said on its
 * own, said on its own. Every surface says that number for that noun
 * phrase — the Brief top in both registers and its list, the Explore top, the
 * Inbox row and its items, the whole-story page top and its readiness block and
 * its "What still needs evidence" section, Ask, the headline, and every export
 * (Markdown, HTML, JSON, full and redacted) and Copy as Markdown.
 *
 * It walks the browser's REAL read paths (`readChange`, `readPacket`,
 * `readChangeDocuments`, `assembleInbox`, `renderExport`, `copiedMarkdown`,
 * Ask's `answer`), with `fetch` answered from the walk's own capture
 * (`walk-capture.fixture.json`) and the served readiness set, line by line, to
 * what the engine serves for each outcome — the way the adversary's sweep did.
 * The expected numbers are worked out from the outcomes the case names, and
 * from nothing the code under test computes.
 *
 * No temporary file is written: the bundle is imported from a data URL.
 */

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'

const here = dirname(fileURLToPath(import.meta.url))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeShell, readChange, readChangeDocuments } from './change-workspace.jsx';
  import { PacketPage, readPacket, copiedMarkdown } from './packet.jsx';
  import { assembleInbox, InboxItem } from './inbox.jsx';
  import { renderExport } from './packet-export.js';
  import { answer as askAnswer } from './ask-hq.js';
  import { HQ_COPY } from './hq-copy.js';
  import { fill } from './hq-narrate.js';
  import { reasonLine } from './inbox-view.js';
  export { askAnswer, readChange, readChangeDocuments, readPacket, assembleInbox, renderExport, copiedMarkdown, HQ_COPY, fill, reasonLine };
  export const renderRoute = props => renderToStaticMarkup(React.createElement(ChangeShell, props));
  export const renderPage = props => renderToStaticMarkup(React.createElement(PacketPage, props));
  export const renderRow = props => renderToStaticMarkup(React.createElement('ul', null, React.createElement(InboxItem, props)));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(" + JSON.stringify(import.meta.url) + ');' },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  return import('data:text/javascript;base64,' + Buffer.from(result.outputFiles[0].text).toString('base64'))
})()

const CAPTURE = JSON.parse(readFileSync(join(here, 'walk-capture.fixture.json'), 'utf8'))
const ID = CAPTURE.change_id
const clone = (v) => JSON.parse(JSON.stringify(v))
const noop = () => {}
const norm = (s) => String(s).replace(/\s+/g, ' ').trim()

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
/* One window for the whole file: a window per parsed string is what a sweep of
 * a few hundred cases cannot afford. */
const WINDOW = new JSDOM('<!doctype html><body></body>').window
const parse = (html) => new WINDOW.DOMParser().parseFromString(html, 'text/html')
/** Several selectors over one rendering, parsed once. */
const read = (html, selectors) => {
  const doc = parse('<!doctype html><body>' + html + '</body>')
  return Object.fromEntries(Object.entries(selectors).map(([k, sel]) => [k, [...doc.querySelectorAll(sel)].map((n) => norm(n.textContent))]))
}
const route = (model, altitude, register) => M.renderRoute({
  model, stage: 'define', altitude, register, onGo: noop, onCopy: noop, onReload: noop, onAltitude: noop, onRegister: noop,
})

/* ── the fixtures: n accepted lines, each in one of the engine's outcomes ── */

/**
 *  satisfied        a linked passing run, and the engine says it is met
 *  never_assessed   a linked passing run, and the engine says whether it still
 *                   applies was never assessed (R-2026-09-28-25)
 *  stale            a linked passing run, and the engine says the files it
 *                   looked at have changed since
 *  settled_nothing  something ran and it settled nothing
 *  nothing_looked   no run at all
 *  unlinked_run     a passing run exists that nobody linked to this line
 *  waived           set aside by a named person; nobody checked
 *  not_applicable   set aside by a named person; it does not apply here
 */
const OUTCOMES = ['satisfied', 'never_assessed', 'waived', 'not_applicable', 'unlinked_run', 'nothing_looked', 'stale', 'settled_nothing']
const NEEDS = new Set(['never_assessed', 'nothing_looked', 'unlinked_run', 'stale', 'settled_nothing'])
const SET_ASIDE = new Set(['waived', 'not_applicable'])
const WORDS = (i) => `Line ${i + 1}: the total handles case ${i + 1} correctly.`

/* The two things that are not a line, in the words that say them on their own. They are deliberately not the
 * engine's reasons for a line (`readiness.reasons`): those explain why approval is off, and the one answer to that
 * is the blocker (run-link-truth-r5.test.js), which nothing on a page may lead with anything else. */
const IDENTITY_WORDS = 'Nothing is written down yet about what will be looked at for this Change.'
const REVIEW_WORDS = 'This Change needs a review, and none is written down.'

function shape(lines, { identity = true, review = false } = {}) {
  return (reads) => {
    const change = reads[`/api/changes/${ID}`].body.change
    change.accepted_scope.criteria = lines.map((_, i) => WORDS(i))
    const c = COV(reads)
    const model = clone(c.coverage.criteria[0])
    const none = { items: [], state: 'none', state_slug: 'coverage_none' }
    c.coverage.criteria = lines.map((l, i) => ({
      ...clone(model),
      index: i + 1,
      text: WORDS(i),
      ...(l === 'satisfied' || l === 'never_assessed' || l === 'stale'
        ? {}
        : l === 'settled_nothing' ? { items: model.items.map((it) => ({ ...it, bears: false })) } : none),
    }))
    const idOb = c.readiness.obligations[0]
    const lineOb = c.readiness.obligations[1]
    const need = (o, slug, extra = {}) => ({
      ...o, outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: slug.replace('readiness_', ''), unsatisfied_slug: slug, evidence_name: null, ...extra,
    })
    const setAside = (o, kind, rationale) => ({
      ...o, outcome: kind, outcome_slug: kind, evidence_name: null, waiver: { actor: 'sam', authority: 'the accepted profile', rationale },
    })
    const obligations = [
      identity ? idOb : need(idOb, 'readiness_inputs_missing'),
      ...lines.map((l, i) => {
        const o = { ...clone(lineOb), id: 'criterion:' + (i + 1), criterion_index: i + 1, text: WORDS(i) }
        if (l === 'satisfied') return o
        if (NEEDS.has(l)) return need(o, 'readiness_' + l)
        if (l === 'waived') return setAside(o, 'waived', 'Nobody checked this.')
        if (l === 'not_applicable') return setAside(o, 'not_applicable', 'Does not apply.')
        throw new Error('no such outcome: ' + l)
      }),
      ...(review
        ? [{ id: 'review', kind: 'review', criterion_index: null, text: '', outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'review_missing', unsatisfied_slug: 'readiness_review_missing', evidence_name: null, linked_by: null, waiver: null, waivable: true }]
        : []),
    ]
    const missing = obligations.filter((o) => o.outcome === 'needs_evidence')
    const aside = obligations.filter((o) => o.outcome === 'waived' || o.outcome === 'not_applicable')
    c.readiness.obligations = obligations
    c.readiness.ready = missing.length === 0
    c.readiness.headline_slug = missing.length === 0
      ? (aside.length ? 'readiness_ready_with_exceptions' : 'readiness_ready')
      : missing.length === 1 ? 'readiness_needs_evidence_one' : 'readiness_needs_evidence'
    c.readiness.missing = missing.map((o) => o.id)
    c.readiness.waived = aside.map((o) => o.id)
    c.readiness.with_exceptions = missing.length === 0 && aside.length > 0
    c.readiness.rests_on = obligations
      .filter((o) => o.outcome === 'satisfied' && o.criterion_index)
      .map((o) => ({ obligation_id: o.id, text: o.text, evidence_name: o.evidence_name }))
  }
}

/** What the case says, worked out from its outcomes and from nothing else. */
function expectOf(lines, { identity = true, review = false } = {}) {
  const at = (set) => lines.map((l, i) => [l, i]).filter(([l]) => set(l)).map(([, i]) => i)
  const need = at((l) => NEEDS.has(l))
  const aside = at((l) => SET_ASIDE.has(l))
  const met = at((l) => l === 'satisfied')
  const beyond = [...(identity ? [] : [IDENTITY_WORDS]), ...(review ? [REVIEW_WORDS] : [])]
  return {
    total: lines.length,
    need: need.length,
    needTexts: need.map(WORDS),
    aside: aside.length,
    asideTexts: aside.map(WORDS),
    met: met.length,
    unlinked: lines.filter((l) => l === 'unlinked_run').length,
    beyond,
    ready: need.length === 0 && beyond.length === 0,
    withExceptions: need.length === 0 && beyond.length === 0 && aside.length > 0,
  }
}

/* ── what a person reads, on every surface ─────────────────────────────── */

async function surfaces(lines, opts, after) {
  const { screen, packet, docs } = await readMoment((reads) => {
    shape(lines, opts)(reads)
    if (after) after(COV(reads).readiness)
  })
  const hq = screen.model.hq
  const inbox = M.assembleInbox([docs.change], new Map([[docs.change.id, docs]]))
  assert.equal(inbox.items.length, 1, 'the Change is on the Inbox')
  const item = inbox.items[0]
  const rowHtml = M.renderRow({ item })
  const row = read(rowHtml, { elements: '.ws-inbox-reason', items: '.ws-inbox-attention' })
  const out = { hq, item, row: { reason: item.reasonText, elements: row.elements, items: row.items } }
  out.brief = {}
  for (const [altitude, register] of [['brief', 'manager'], ['brief', 'engineer'], ['explore', 'manager'], ['explore', 'engineer']]) {
    out.brief[`${altitude}/${register}`] = read(route(screen.model, altitude, register), {
      top: '.ws-change-conclusion',
      sentence: '.ws-readiness-sentence',
      listed: '.ws-readiness-missing > li .ws-readiness-what',
      listedRows: '.ws-readiness-missing > li',
      block: '.ws-change-readiness',
    })
  }
  out.page = read(M.renderPage({ model: packet.model, notes: packet.notes, onCopy: noop, records: packet.records }), {
    top: '.ws-change-conclusion',
    readinessLine: '.ws-packet-readiness-line',
    readinessRows: '.ws-packet-readiness-rests > li',
    readinessBlock: '.ws-packet-readiness',
    needsHeading: '.ws-packet-needs > h2',
    needsRows: '.ws-packet-needs-list > li',
    needsLines: '.ws-packet-needs-list > li .ws-packet-criterion',
    needsDoors: '.ws-packet-needs-list > li .ws-packet-link-door',
    needsSection: '.ws-packet-needs',
  })
  const p = packet.model.hq.packet
  out.exports = {}
  for (const format of ['md', 'json', 'html']) {
    out.exports[format] = M.renderExport(p, packet.records, { format }).text
    out.exports[format + '/redacted (paths)'] = M.renderExport(p, packet.records, { format, redact: ['paths'] }).text
    out.exports[format + '/redacted (all)'] = M.renderExport(p, packet.records, { format, redact: ['agent_words', 'paths', 'failure_output'] }).text
  }
  out.exports['copy as markdown'] = M.copiedMarkdown(packet.model.hq, packet.records)
  out.ask = M.askAnswer('needs_evidence', hq)
  return out
}

/* ── reading a number out of a sentence ────────────────────────────────── */

const STILL_NEED = /(\d+) (?:things? )?still needs?\b/g

const factsOf = (text) => {
  const f = { need: [], met: null, total: null, aside: null, none: false, unlinked: null }
  for (const m of text.matchAll(STILL_NEED)) f.need.push(Number(m[1]))
  let m = /(\d+) of (\d+) things that count as done have evidence/.exec(text)
  if (m) { f.met = Number(m[1]); f.total = Number(m[2]) }
  m = /The (\d+) thing that counts as done has evidence/.exec(text)
  if (m) { f.met = 1; f.total = 1 }
  m = /Accepted: (\d+) things? counts? as done/.exec(text)
  if (m) f.total = Number(m[1])
  m = /(\d+) (?:was|were) set aside/.exec(text)
  if (m) f.aside = Number(m[1])
  m = /All (\d+) were set aside/.exec(text)
  if (m) f.aside = Number(m[1])
  if (/It was set aside/.test(text)) f.aside = 1
  f.none = /None has evidence yet|It has no evidence yet/.test(text)
  m = /isn't linked to (\d+) of them|isn't linked to it/.exec(text)
  if (m) f.unlinked = m[1] ? Number(m[1]) : 1
  return f
}

/** The top sentence, held to what the served readiness says. */
function checkTop(where, text, X) {
  const f = factsOf(text)
  const at = `${where}: "${text}"`
  for (const n of f.need) assert.equal(n, X.need, `${at} — says ${n} still need evidence, and the served readiness names ${X.need} of the accepted lines`)
  if (f.met !== null) assert.equal(f.met, X.met, `${at} — says ${f.met} have evidence, and the served readiness says ${X.met} are met`)
  if (f.total !== null) assert.equal(f.total, X.total, `${at} — says ${f.total} count as done, and ${X.total} are accepted`)
  if (f.aside !== null) assert.equal(f.aside, X.aside, `${at} — says ${f.aside} were set aside, and ${X.aside} were`)
  if (f.unlinked !== null) assert.equal(f.unlinked, X.unlinked, `${at} — names ${f.unlinked} unlinked lines, and the served readiness names ${X.unlinked}`)
  if (X.need > 0) {
    assert.ok(f.need.length > 0 || f.none || f.unlinked !== null, `${at} — ${X.need} line(s) still need evidence and the top says neither how many nor that none has any`)
  }
  if (f.none || f.unlinked !== null) {
    assert.ok(X.met === 0 && X.aside === 0 && X.beyond.length === 0 && X.need === X.total, `${at} — "none has evidence" is only true when every line still needs it`)
  }
  if (X.need === 0) assert.ok(!f.need.some((n) => n > 0) && !f.none, `${at} — nothing waits for evidence`)
  if (X.need === 0 && X.aside > 0) assert.equal(f.aside, X.aside, `${at} — the lines set aside are said, since nothing else is left to say of them`)
  for (const words of [IDENTITY_WORDS, REVIEW_WORDS]) {
    assert.equal(text.includes(words), X.beyond.includes(words), `${at} — the words for ${words === IDENTITY_WORDS ? 'the identity' : 'the review'} are ${X.beyond.includes(words) ? 'missing' : 'said where nothing is missing'}`)
  }
}

/* ── the check, over every surface ─────────────────────────────────────── */

const askLead = (sentence) => {
  let m = /^One of the things that count as done is still waiting for evidence\./.exec(sentence)
  if (m) return 1
  m = /^(\d+) of the things that count as done are still waiting for evidence\./.exec(sentence)
  if (m) return Number(m[1])
  if (/^Every accepted statement has something behind it\./.test(sentence) || /^Nothing that counts as done is waiting for evidence\./.test(sentence)) return 0
  return null
}

/** A file's blocks, in order: `{tag, text}`, the same for Markdown and HTML. */
function blocksOf(format, text) {
  if (format === 'md') {
    return text.split('\n').filter((l) => l.trim()).map((l) => (
      l.startsWith('# ') ? { tag: 'h1', text: l.slice(2) }
        : l.startsWith('## ') ? { tag: 'h2', text: l.slice(3) }
          : l.startsWith('- ') ? { tag: 'li', text: l.slice(2) }
            : { tag: 'p', text: l }
    ))
  }
  return [...parse(text).querySelectorAll('h1, h2, p, li')].map((n) => ({ tag: n.tagName.toLowerCase(), text: norm(n.textContent) }))
}

function checkExport(where, format, text, X) {
  const at = (s) => `${where}: ${s}`
  if (format === 'json') {
    const j = JSON.parse(text)
    checkTop(at('headline'), j.headline, X)
    const sentence = j.readiness.sentence || ''
    for (const n of factsOf(sentence).need) assert.equal(n, X.need, at(`readiness.sentence "${sentence}" says ${n}`))
    if (!X.ready) {
      assert.equal(j.readiness.rests_on.length, X.need, at(`readiness.rests_on holds ${j.readiness.rests_on.length} lines; ${X.need} still need evidence`))
      for (const t of X.asideTexts) assert.ok(!j.readiness.rests_on.some((r) => r.startsWith(t + ' — ')), at(`a set-aside line is listed under what still needs evidence: ${t}`))
      assert.deepEqual(j.readiness.also ?? [], X.need > 0 ? X.beyond : [], at('readiness.also is what is missing that is not a line'))
      assert.equal((j.readiness.set_aside ?? []).length, X.aside, at('readiness.set_aside holds the lines that were set aside'))
    }
    return
  }
  const blocks = blocksOf(format, text)
  const h1 = blocks.findIndex((b) => b.tag === 'h1')
  const top = blocks.slice(h1 + 1).find((b) => b.tag === 'p')
  checkTop(at('the headline paragraph'), top.text, X)
  for (const b of blocks) for (const n of factsOf(b.text).need) assert.equal(n, X.need, at(`"${b.text}" says ${n}`))
  if (!X.ready) {
    /* The readiness sentence, the list under it, and then — each on its own — what is missing that is not a line and the
     * lines that were set aside. With no line waiting, the sentence IS what is missing that is not a line. */
    let after
    if (X.need > 0) {
      const s = blocks.findIndex((b) => b.tag === 'p' && /^\d+ things? still needs? evidence:$/.test(b.text))
      assert.ok(s >= 0, at('the readiness sentence is in the file'))
      const listed = []
      for (let i = s + 1; i < blocks.length && blocks[i].tag === 'li'; i += 1) listed.push(blocks[i].text)
      assert.equal(listed.length, X.need, at(`${listed.length} lines are listed under the readiness sentence; ${X.need} still need evidence`))
      for (const t of X.asideTexts) assert.ok(!listed.some((b) => b.startsWith(t + ' — ')), at(`a set-aside line is listed under what still needs evidence: ${t}`))
      after = s + 1 + listed.length
      for (const words of X.beyond) {
        assert.equal(blocks[after] && blocks[after].text, words, at(`what is missing that is not a line is not said straight after the list: ${words}`))
        after += 1
      }
    } else if (X.beyond.length) {
      const s = blocks.findIndex((b) => b.tag === 'p' && b.text === X.beyond.join(' '))
      assert.ok(s >= 0, at('what is missing that is not a line is not the readiness sentence'))
      after = s + 1
    }
    if (after !== undefined && X.aside > 0) {
      assert.equal(blocks[after] && blocks[after].text, M.HQ_COPY.change.model.readiness.set_aside_heading, at('the lines set aside have no heading of their own after the list'))
      const set = []
      for (let i = after + 1; i < blocks.length && blocks[i].tag === 'li'; i += 1) set.push(blocks[i].text)
      assert.deepEqual(set.map((b) => b.split(' — ')[0]), X.asideTexts, at('the lines set aside are not the ones a person set aside'))
    }
    const all = blocks.map((b) => b.text).join('\n')
    for (const words of X.beyond) assert.ok(all.includes(words), at(`the file does not say what is missing that is not a line: ${words}`))
  }
}

function checkCase(label, lines, opts, S) {
  const X = expectOf(lines, opts)
  const at = (s) => `${label} — ${s}`
  const hq = S.hq

  /* The served answer is what the case says. `missingCount` is the count the sentences state: of the lines. */
  assert.equal(hq.readiness.ready, X.ready, at('the served readiness'))
  assert.equal(hq.readiness.missingCount, X.need, at('the served count of what still needs evidence'))

  /* THE TOP: one sentence, on every surface that prints it. */
  const headline = hq.headline.text
  checkTop(at('headline'), headline, X)
  for (const [key, b] of Object.entries(S.brief)) assert.deepEqual(b.top, [headline], at(`the top on ${key} is not the headline`))
  assert.deepEqual(S.page.top, [headline], at('the whole-story page top is not the headline'))
  if (!(X.withExceptions && S.item.reason.id === 'awaiting_approval')) {
    /* A Change waiting only for its approver, ready with exceptions, says what the Approve control says. */
    assert.equal(S.row.reason, headline, at('the Inbox row is not the headline'))
  } else {
    assert.ok(!factsOf(S.row.reason).need.some((n) => n > 0), at(`the Inbox row says "${S.row.reason}" beside a Change that waits for nothing`))
  }

  /* THE LIST under the Brief top, and the same list on Explore. */
  for (const [key, b] of Object.entries(S.brief)) {
    const w = at(`the readiness block on ${key}`)
    if (!X.ready && X.need > 0) {
      assert.equal(b.sentence.length, 1, w)
      assert.deepEqual(factsOf(b.sentence[0]).need, [X.need], `${w}: "${b.sentence[0]}"`)
      assert.deepEqual(b.listed, X.needTexts, `${w}: the lines listed are not the lines the answer says need evidence`)
    } else if (!X.ready) {
      assert.deepEqual(b.listedRows, [], `${w}: nothing waits for evidence, and a list of it is drawn`)
    }
    for (const t of X.asideTexts) if (!X.ready) assert.ok(!b.listed.includes(t), `${w}: a set-aside line is listed under what still needs evidence`)
    for (const words of X.beyond) assert.ok(b.block.join(' ').includes(words), `${w}: does not say, said on its own: ${words}`)
  }

  /* THE WHOLE-STORY PAGE: its readiness block and its own list. */
  if (!X.ready && X.need > 0) {
    assert.deepEqual(factsOf(S.page.readinessLine.join(' ')).need, [X.need], at(`the page's readiness line: "${S.page.readinessLine.join(' ')}"`))
    assert.equal(S.page.readinessRows.length, X.need, at(`the page's readiness block lists ${S.page.readinessRows.length} lines; ${X.need} still need evidence`))
    for (const t of X.asideTexts) assert.ok(!S.page.readinessRows.some((r) => r.startsWith(t + ' — ')), at('a set-aside line is listed under what still needs evidence in the page readiness block'))
  }
  for (const words of X.beyond) assert.ok(S.page.readinessBlock.join(' ').includes(words), at(`the page does not say, said on its own: ${words}`))
  assert.deepEqual(S.page.needsLines, X.needTexts, at(`the page's "What still needs evidence" lists ${JSON.stringify(S.page.needsLines)}, the lines the answer names are ${JSON.stringify(X.needTexts)}`))
  assert.equal(S.page.needsDoors.length, X.unlinked, at('the page offers the door to link a run at each line whose run nobody linked, and at no other'))
  if (!X.ready && X.aside > 0) {
    const block = S.page.readinessBlock.join(' ')
    assert.ok(block.includes(M.HQ_COPY.change.model.readiness.set_aside_heading), at('the page does not say the lines set aside are set aside'))
    for (const t of X.asideTexts) assert.ok(block.includes(t), at(`the page does not say a line was set aside: ${t}`))
  }
  if (X.need > 0) {
    assert.deepEqual(S.page.needsHeading, [M.HQ_COPY.change.packet.needs.heading], at('the page heading over a list of what needs evidence'))
  } else {
    assert.notDeepEqual(S.page.needsHeading, [M.HQ_COPY.change.packet.needs.heading], at('a heading that asks what still needs evidence, over nothing'))
    if (X.aside > 0) assert.doesNotMatch(S.page.needsSection.join(' '), /has something behind it/, at('"has something behind it" is said of lines nobody checked'))
  }

  /* ASK. */
  const lead = askLead(S.ask.sentences[0] || '')
  assert.equal(S.ask.answered, true, at('Ask answers'))
  assert.equal(lead, X.need, at(`Ask says "${S.ask.sentences[0]}": ${lead}; the answer names ${X.need}`))
  if (X.need > 0) {
    assert.deepEqual(S.ask.evidence.map((e) => e.label), X.needTexts, at('the lines Ask points at are not the lines the answer names'))
    assert.equal(S.ask.sentences.length >= 1 + X.need, true, at('Ask says why each one still needs evidence'))
  } else {
    for (const t of X.asideTexts) assert.ok(!S.ask.sentences.some((s) => /still waiting for evidence/.test(s) && s.includes(t)), at('Ask calls a set-aside line waiting'))
  }
  for (const words of X.beyond) assert.ok(S.ask.sentences.some((s) => s.includes(words)), at(`Ask does not say, said on its own: ${words}`))
  if (X.need === 0 && X.aside > 0) assert.doesNotMatch(S.ask.sentences[0], /has something behind it/, at('Ask says lines nobody checked have something behind them'))

  /* THE INBOX: its row says the top, and counts what needs you. */
  const items = /(\d+) items? on this Change needs?/.exec(S.row.items[0] || '')
  assert.ok(items, at(`the Inbox row states how many items need you: "${S.row.items[0]}"`))
  assert.equal(Number(items[1]), X.need + X.beyond.length + (X.ready ? 1 : 0), at('the Inbox row counts the lines that still need evidence, each thing that is not a line once, and a decision waiting for its approver'))
  assert.equal(S.item.attention.items, Number(items[1]), at('the item and the words agree'))
  /* The sentence the Inbox says of this reason wherever it is not the row's own top — "Also on this Change:", or a Change
   * whose first reason is another — is a count of the lines, and what is not a line is said in its own words. */
  const reason = hq.attention.reasons.find((r) => r.id === 'missing_evidence')
  if (X.need + X.beyond.length === 0) {
    assert.equal(reason, undefined, at('the Inbox files this Change under missing evidence, and nothing is missing'))
  } else {
    assert.ok(reason, at('the Inbox does not file this Change under missing evidence, and something is'))
    assert.equal(reason.count, X.need + X.beyond.length, at('the reason adds up to the lines and the things that are not lines'))
    const line = M.reasonLine(reason)
    const said = M.fill(line.template, line.values)
    if (X.need > 0) {
      const m = /^(?:1 thing that counts as done has|(\d+) things that count as done have) no evidence yet\.$/.exec(said)
      assert.ok(m, at(`the Inbox's own sentence for missing evidence: "${said}"`))
      assert.equal(m[1] ? Number(m[1]) : 1, X.need, at(`the Inbox says "${said}"; the answer names ${X.need} lines`))
    } else {
      assert.equal(said, X.beyond.join(' '), at('the Inbox says what is missing that is not a line in its own words'))
    }
  }

  /* EVERY EXPORT. */
  for (const [name, text] of Object.entries(S.exports)) {
    const format = name.startsWith('json') ? 'json' : name.startsWith('html') ? 'html' : 'md'
    checkExport(at(name), format, text, X)
  }
  assert.equal(S.exports['copy as markdown'], S.exports.md, at('Copy as Markdown is the Markdown file'))
}

/* ── the cases ─────────────────────────────────────────────────────────── */

function cases() {
  const out = []
  const add = (lines, opts = {}) => out.push({ lines, opts, label: `[${lines.join(', ')}]${opts.identity === false ? ' + identity unconfirmed' : ''}${opts.review ? ' + review missing' : ''}` })
  for (const a of OUTCOMES) for (const identity of [true, false]) add([a], { identity })
  for (const a of OUTCOMES) for (const b of OUTCOMES) for (const identity of [true, false]) add([a, b], { identity })
  let flip = 0
  for (const a of OUTCOMES) {
    for (const b of OUTCOMES) {
      /* n = 12: half of one outcome, then half of another. */
      add([...Array(6).fill(a), ...Array(6).fill(b)], { identity: (flip += 1) % 2 === 0, review: flip % 5 === 0 })
    }
  }
  /* n = 3 .. 11, a seeded mix. */
  let seed = 20260930
  const next = () => { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 2 ** 32 }
  for (let n = 3; n <= 11; n += 1) {
    for (let k = 0; k < 4; k += 1) {
      add(Array.from({ length: n }, () => OUTCOMES[Math.floor(next() * OUTCOMES.length)]), { identity: next() > 0.35, review: next() > 0.8 })
    }
  }
  return out
}

test('the count of what still needs evidence is one number, on every surface, over every mix of outcomes', async () => {
  const all = cases()
  assert.ok(all.length >= 240, `the sweep is ${all.length} cases`)
  const failures = []
  for (const c of all) {
    try {
      checkCase(c.label, c.lines, c.opts, await surfaces(c.lines, c.opts))
    } catch (e) {
      failures.push(e.message.split('\n')[0])
    }
  }
  assert.deepEqual(failures.slice(0, 12), [], `${failures.length} of ${all.length} cases disagree with themselves; the first ones:`)
})

test('the harness is a live instrument: a top that counts the wrong lines is caught', () => {
  const X = expectOf(['satisfied', 'nothing_looked'], { identity: true })
  checkTop('ok', '1 of 2 things that count as done have evidence. 1 still needs it.', X)
  assert.throws(() => checkTop('bad', 'Accepted: 2 things count as done. 2 things still need evidence.', X), /2 still need evidence/)
  const set = expectOf(['satisfied', 'waived'], { identity: true })
  assert.throws(() => checkTop('bad', '1 of 2 things that count as done have evidence. 1 still needs it.', set), /the served readiness names 0/)
  const id = expectOf(['satisfied', 'nothing_looked'], { identity: false })
  assert.throws(() => checkTop('bad', 'Accepted: 2 things count as done. 1 thing still needs evidence.', id), /the words for the identity are missing/)
})

/* ── the cases the adversary named, word for word ──────────────────────── */

const say = (S) => ({
  top: S.hq.headline.text,
  page: S.page.top[0],
  pageList: S.page.needsLines,
  ask: S.ask.sentences[0],
  briefList: S.brief['brief/manager'].sentence[0],
})

test('the walk: line 1 linked and never assessed, line 2 with nothing — the page top, its list and Ask say two', async () => {
  const S = await surfaces(['never_assessed', 'nothing_looked'])
  const said = say(S)
  assert.equal(said.top, 'Accepted: 2 things count as done. 2 things still need evidence.')
  assert.equal(said.page, said.top)
  assert.deepEqual(said.pageList, [WORDS(0), WORDS(1)])
  assert.equal(said.ask, '2 of the things that count as done are still waiting for evidence.')
  assert.equal(said.briefList, '2 things still need evidence:')
})

test('(7) ready with exceptions: the top says one has evidence and one was set aside, and nothing still needs it', async () => {
  const S = await surfaces(['satisfied', 'waived'])
  const said = say(S)
  assert.equal(said.top, '1 of 2 things that count as done have evidence. 1 was set aside, with nobody checking.')
  assert.deepEqual(said.pageList, [])
  assert.match(said.ask, /^Nothing that counts as done is waiting for evidence\. 1 was set aside, with nobody checking\.$/)
  const all = await surfaces(['waived', 'not_applicable', 'waived'])
  assert.equal(say(all).top, 'Accepted: 3 things count as done. All 3 were set aside, with nobody checking.')
})

test('(8) two lines set aside and one with nothing: the top does not say none has evidence, and the page lists the one line', async () => {
  const S = await surfaces(['waived', 'waived', 'nothing_looked'])
  const said = say(S)
  assert.equal(said.top, 'Accepted: 3 things count as done. 1 thing still needs evidence.')
  assert.deepEqual(said.pageList, [WORDS(2)])
  assert.equal(said.ask, 'One of the things that count as done is still waiting for evidence.')
  const unlinked = await surfaces(['waived', 'waived', 'unlinked_run'])
  assert.deepEqual(say(unlinked).pageList, [WORDS(2)])
})

test('(9) a set-aside line is never listed under "What still needs evidence", and a lone never-assessed line is', async () => {
  const S = await surfaces(['unlinked_run', 'waived'])
  assert.deepEqual(say(S).pageList, [WORDS(0)], 'the set-aside line is not in the page list')
  const one = await surfaces(['never_assessed'])
  const said = say(one)
  assert.equal(said.top, 'Accepted: 1 thing counts as done. 1 thing still needs evidence.')
  assert.deepEqual(one.page.needsHeading, ['What still needs evidence'])
  assert.deepEqual(said.pageList, [WORDS(0)])
  assert.equal(said.ask, 'One of the things that count as done is still waiting for evidence.')
})

test('(10) the identity unconfirmed is said on its own and never counted as one of the lines that count as done', async () => {
  const S = await surfaces(['satisfied', 'nothing_looked'], { identity: false })
  const said = say(S)
  assert.equal(said.top, `Accepted: 2 things count as done. 1 thing still needs evidence. ${IDENTITY_WORDS}`)
  assert.equal(said.briefList, '1 thing still needs evidence:')
  assert.deepEqual(said.pageList, [WORDS(1)])
  assert.equal(said.ask, 'One of the things that count as done is still waiting for evidence.')
  assert.equal(S.row.items[0], '2 items on this Change need you.', 'the Inbox counts the line and the identity, one each')
  const met = await surfaces(['satisfied', 'satisfied'], { identity: false })
  assert.equal(say(met).top, `2 of 2 things that count as done have evidence. ${IDENTITY_WORDS}`)
  assert.deepEqual(say(met).pageList, [])
})

test('what agreed at round 1 still agrees: where the coverage and the served readiness name the same lines the top is the coverage sentence, byte for byte', async () => {
  const same = await surfaces(['satisfied', 'nothing_looked'])
  assert.equal(say(same).top, '1 of 2 things that count as done have evidence. 1 still needs it.')
  const plain = await surfaces(['satisfied', 'satisfied'])
  assert.equal(say(plain).top, '2 of 2 things that count as done have evidence. 0 still need it.')
  const none = await surfaces(['nothing_looked', 'nothing_looked'])
  assert.equal(say(none).top, 'Accepted: 2 things count as done. None has evidence yet.')
  const rewritten = await surfaces(['satisfied', 'never_assessed', 'nothing_looked'])
  assert.equal(say(rewritten).top, 'Accepted: 3 things count as done. 2 things still need evidence.')
})

/* ── an answer that is odd is still one count ──────────────────────────── */

test('an obligation naming a line the Change does not have is one more thing named, and never one of the lines that count as done', async () => {
  const S = await surfaces(['satisfied', 'nothing_looked'], {}, (r) => {
    r.obligations.push({ ...clone(r.obligations[2]), id: 'criterion:9', criterion_index: 9, text: 'A line this Change does not have.', outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked', evidence_name: null })
    r.missing.push('criterion:9')
  })
  const said = say(S)
  assert.equal(said.top, 'Accepted: 2 things count as done. 1 thing still needs evidence. Something else this Change needs is not written down yet.')
  assert.equal(said.briefList, '1 thing still needs evidence:')
  assert.deepEqual(said.pageList, [WORDS(1)])
  assert.equal(said.ask, 'One of the things that count as done is still waiting for evidence.')
  assert.equal(S.row.items[0], '2 items on this Change need you.')
})

test('the same obligation served twice is one thing, on every surface', async () => {
  const S = await surfaces(['satisfied', 'nothing_looked'], {}, (r) => {
    r.obligations.push(clone(r.obligations[2]))
    r.missing.push(r.obligations[2].id)
  })
  const said = say(S)
  assert.equal(said.top, '1 of 2 things that count as done have evidence. 1 still needs it.')
  assert.equal(said.briefList, '1 thing still needs evidence:')
  assert.deepEqual(said.pageList, [WORDS(1)])
  assert.equal(said.ask, 'One of the things that count as done is still waiting for evidence.')
  assert.equal(S.row.items[0], '1 item on this Change needs you.')
})

test('with no answer served, nothing here is derived: absence of an answer is not an answer', async () => {
  const { screen, packet } = await readMoment((reads) => { shape(['satisfied', 'nothing_looked'])(reads); delete COV(reads).readiness })
  const hq = screen.model.hq
  assert.equal(hq.readiness.absent, true)
  assert.equal(hq.readiness.tally, null)
  assert.equal(hq.headline.text, '1 of 2 things that count as done have evidence. 1 still needs it.')
  assert.equal(M.askAnswer('needs_evidence', hq).sentences[0], 'One of the things that count as done is still waiting for evidence.')
  assert.equal(packet.model.needs.length, 1)
})

test('an answer that names fewer lines than the Change accepts, and nothing that needs evidence, says nothing to count: the top is the coverage\'s', async () => {
  const { screen } = await readMoment((reads) => {
    shape(['satisfied', 'nothing_looked'])(reads)
    const r = COV(reads).readiness
    r.obligations = r.obligations.filter((o) => o.id !== 'criterion:2')
    r.missing = []
    r.ready = false
  })
  const hq = screen.model.hq
  assert.equal(hq.readiness.tally.total, 1, 'the answer names one of the two lines')
  assert.equal(hq.headline.text, '1 of 2 things that count as done have evidence. 1 still needs it.')
})
