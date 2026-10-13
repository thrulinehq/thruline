/* BRIEF-COUNT-AGREES-R3 (launch-critical) — the sweep of round 2, widened to the
 * states the round-2 adversary found it never served.
 *
 * Round 2 (`brief-count-agrees-r2.test.js`) made every surface say one number for
 * "how many still need evidence", and its committed harness modelled the review
 * obligation as a boolean: a review that needs evidence, or none. The round-2
 * adversary then set a REVIEW aside on a real `thruline ui` (a named person
 * running `thruline change waive --obligation review`) and read
 *
 *   the top:                 "1 of 2 things that count as done have evidence.
 *                             1 was set aside, with nobody checking."
 *   the set-aside list under it: TWO entries — the line, and the review
 *
 * on the Change screen, the whole-story page and every export. The sweep never
 * served a set-aside review, so it never saw it.
 *
 * WHAT THIS HARNESS ADDS, over round 2:
 *
 *   - the review obligation in each of its five states — none, needs evidence,
 *     satisfied, waived, does not apply — crossed with the identity confirmed and
 *     not, over the round-2 adversary's own outcome pairs (n = 1..12 lines, every
 *     ordered pair of six outcomes split across them: 4,020 distinct cases, sharded
 *     so that no shard runs a minute), and a seeded random sweep over nine outcomes;
 *   - the one outcome round 2 did not have: a line whose evidence was proven on
 *     ANOTHER version (`other_version`), which the page shows as "Proven on the
 *     previous version — needs a re-check";
 *   - the tell-the-agent panel, which round 2 did not read at all;
 *   - the review's entry in the set-aside list, which has to name itself.
 *
 * WHAT IT HOLDS (each of these is a rule about what a person reads, worked out
 * from the outcomes a case names and from nothing the code under test computes):
 *
 *   1. "set aside" is counted the way its list is. A bare number ("N were set
 *      aside") equals the number of entries the set-aside list holds — the
 *      lines AND the review — or the lines and the review are named apart, each
 *      with its own count ("N of them were set aside", "The review was set
 *      aside"), and every one of them is accounted for.
 *   2. The review's entry names itself: "The review — Waived by …".
 *   3. The tell-the-agent panel never says every line that counts as done is
 *      proven while the served answer names a line that still needs evidence, and
 *      it asks about exactly those lines.
 *   4. The top never says none has evidence over a line the page shows as proven
 *      on another version without saying it is about this version.
 *
 * and everything round 2 held: one count of what still needs evidence on every
 * surface, in every export. The file walks the browser's REAL read paths with
 * `fetch` answered from the walk's own capture; no temporary file is written.
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
  import { ChangeShell, readChange, readChangeDocuments, tellOf } from './change-workspace.jsx';
  import { PacketPage, readPacket, copiedMarkdown } from './packet.jsx';
  import { assembleInbox, InboxItem } from './inbox.jsx';
  import { renderExport } from './packet-export.js';
  import { answer as askAnswer } from './ask-hq.js';
  import { HQ_COPY } from './hq-copy.js';
  import { fill } from './hq-narrate.js';
  import { reasonLine } from './inbox-view.js';
  export { askAnswer, readChange, readChangeDocuments, readPacket, assembleInbox, renderExport, copiedMarkdown, HQ_COPY, fill, reasonLine, tellOf };
  export const renderRoute = props => renderToStaticMarkup(React.createElement(ChangeShell, props));
  export const renderPage = props => renderToStaticMarkup(React.createElement(PacketPage, props));
  export const renderRow = props => renderToStaticMarkup(React.createElement('ul', null, React.createElement(InboxItem, props)));
`

export const M = await (async () => {
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
export const clone = (v) => JSON.parse(JSON.stringify(v))
const noop = () => {}
const norm = (s) => String(s).replace(/\s+/g, ' ').trim()

export async function readMoment(edit) {
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

export const COV = (reads) => reads[`/api/changes/${ID}/coverage`].body
/* One window for the whole file: a window per parsed string is what a sweep of
 * thousands of cases cannot afford. */
const WINDOW = new JSDOM('<!doctype html><body></body>').window
const parse = (html) => new WINDOW.DOMParser().parseFromString(html, 'text/html')
/** Several selectors over one rendering, parsed once. */
const read = (html, selectors) => {
  const doc = parse('<!doctype html><body>' + html + '</body>')
  return Object.fromEntries(Object.entries(selectors).map(([k, sel]) => [k, [...doc.querySelectorAll(sel)].map((n) => norm(n.textContent))]))
}
const route = (model, altitude, register, stage = 'define') => M.renderRoute({
  model, stage, altitude, register, onGo: noop, onCopy: noop, onReload: noop, onAltitude: noop, onRegister: noop,
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
 *  other_version    the only run that bears on it was about ANOTHER version
 *  waived           set aside by a named person; nobody checked
 *  not_applicable   set aside by a named person; it does not apply here
 *
 * and the review obligation, which the accepted profile may or may not ask for:
 *
 *  none · needs (no review is written down) · satisfied · waived · not_applicable
 */
export const OUTCOMES = ['satisfied', 'never_assessed', 'waived', 'not_applicable', 'unlinked_run', 'nothing_looked', 'stale', 'settled_nothing', 'other_version']
/** The six outcomes the round-2 adversary's systematic sweep used. */
export const SIX = ['satisfied', 'never_assessed', 'waived', 'not_applicable', 'unlinked_run', 'nothing_looked']
export const REVIEWS = ['none', 'needs', 'satisfied', 'waived', 'not_applicable']
const NEEDS = new Set(['never_assessed', 'nothing_looked', 'unlinked_run', 'stale', 'settled_nothing', 'other_version'])
const SET_ASIDE = new Set(['waived', 'not_applicable'])
export const WORDS = (i) => `Line ${i + 1}: the total handles case ${i + 1} correctly.`

/* The two things that are not a line, in the words that say them on their own. */
export const IDENTITY_WORDS = 'Nothing is written down yet about what will be looked at for this Change.'
export const REVIEW_WORDS = 'This Change needs a review, and none is written down.'
/** The name the review's own entry wears in the set-aside list (BRIEF-COUNT-AGREES-R3). */
export const REVIEW_ENTRY = 'The review'

const reviewObligation = (review) => {
  const base = { id: 'review', kind: 'review', criterion_index: null, text: '', evidence_name: null, linked_by: null, waivable: true }
  if (review === 'needs') {
    return { ...base, outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'review_missing', unsatisfied_slug: 'readiness_review_missing', waiver: null }
  }
  if (review === 'satisfied') return { ...base, outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, waiver: null }
  return {
    ...base,
    outcome: review,
    outcome_slug: review,
    unsatisfied: null,
    unsatisfied_slug: null,
    waiver: { actor: 'sam', authority: 'the release policy', rationale: 'The release policy asks for no review here' },
  }
}

export function shape(lines, { identity = true, review = 'none' } = {}) {
  return (reads) => {
    const change = reads[`/api/changes/${ID}`].body.change
    change.accepted_scope.criteria = lines.map((_, i) => WORDS(i))
    const c = COV(reads)
    const model = clone(c.coverage.criteria[0])
    const none = { items: [], state: 'none', state_slug: 'coverage_none' }
    const other = {
      state: 'other_version',
      state_slug: 'coverage_other_version',
      items: model.items.map((it) => ({ ...it, applies: false, applies_slug: 'inputs_other_revision' })),
    }
    c.coverage.criteria = lines.map((l, i) => ({
      ...clone(model),
      index: i + 1,
      text: WORDS(i),
      ...(l === 'satisfied' || l === 'never_assessed' || l === 'stale'
        ? {}
        : l === 'settled_nothing' ? { items: model.items.map((it) => ({ ...it, bears: false })) }
          : l === 'other_version' ? clone(other) : none),
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
      ...(review !== 'none' ? [reviewObligation(review)] : []),
    ]
    const missing = obligations.filter((o) => o.outcome === 'needs_evidence')
    const aside = obligations.filter((o) => o.outcome === 'waived' || o.outcome === 'not_applicable')
    c.readiness.obligations = obligations
    c.readiness.ready = missing.length === 0
    c.readiness.headline_slug = missing.length === 0
      ? (aside.length ? 'readiness_ready_with_exceptions' : 'readiness_ready')
      : missing.length === 1 ? 'readiness_needs_evidence_one' : 'readiness_needs_evidence'
    c.readiness.missing = missing.map((o) => o.id)
    c.readiness.waived = obligations.filter((o) => o.outcome === 'waived').map((o) => o.id)
    c.readiness.not_applicable = obligations.filter((o) => o.outcome === 'not_applicable').map((o) => o.id)
    c.readiness.with_exceptions = missing.length === 0 && aside.length > 0
    c.readiness.rests_on = obligations
      .filter((o) => o.outcome === 'satisfied' && o.criterion_index)
      .map((o) => ({ obligation_id: o.id, text: o.text, evidence_name: o.evidence_name }))
  }
}

/** What the case says, worked out from its outcomes and from nothing else. */
export function expectOf(lines, { identity = true, review = 'none' } = {}) {
  const at = (set) => lines.map((l, i) => [l, i]).filter(([l]) => set(l)).map(([, i]) => i)
  const need = at((l) => NEEDS.has(l))
  const aside = at((l) => SET_ASIDE.has(l))
  const met = at((l) => l === 'satisfied')
  const beyond = [...(identity ? [] : [IDENTITY_WORDS]), ...(review === 'needs' ? [REVIEW_WORDS] : [])]
  const reviewAside = review === 'waived' || review === 'not_applicable'
  /* What the set-aside list holds: the lines a person set aside, and the review. */
  const entryTexts = [...aside.map(WORDS), ...(reviewAside ? [REVIEW_ENTRY] : [])]
  return {
    total: lines.length,
    need: need.length,
    needTexts: need.map(WORDS),
    aside: aside.length,
    asideTexts: aside.map(WORDS),
    reviewAside,
    entries: entryTexts.length,
    entryTexts,
    met: met.length,
    metTexts: met.map(WORDS),
    unlinked: lines.filter((l) => l === 'unlinked_run').length,
    previous: lines.filter((l) => l === 'other_version').length,
    beyond,
    ready: need.length === 0 && beyond.length === 0,
    withExceptions: need.length === 0 && beyond.length === 0 && entryTexts.length > 0,
  }
}

/* ── what a person reads, on every surface ─────────────────────────────── */

export async function surfaces(lines, opts, after) {
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
      setAside: '.ws-readiness-waived > li',
      setAsideHeading: '.ws-readiness-set-aside-heading',
      tell: '.ws-tell',
    })
  }
  /* The Verify stage draws the same panel again. */
  out.verify = read(route(screen.model, 'explore', 'manager', 'verify'), { tell: '.ws-tell' })
  out.tell = M.tellOf(hq)
  out.page = read(M.renderPage({ model: packet.model, notes: packet.notes, onCopy: noop, records: packet.records }), {
    top: '.ws-change-conclusion',
    readinessLine: '.ws-packet-readiness-line',
    readinessRows: '.ws-packet-readiness-rests > li',
    readinessBlock: '.ws-packet-readiness',
    setAsideHeading: '.ws-packet-readiness-set-aside-heading',
    setAsideRows: '.ws-packet-readiness-set-aside > li',
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

/**
 * What a sentence says about the lines that were set aside, in the three ways it
 * says it:
 *
 *   bare    "N was|were set aside", "All N were set aside", "It was set aside" —
 *           a number with no noun of its own, which can only be a count of what
 *           the set-aside LIST holds;
 *   scoped  "N of them were set aside", "All N of them were set aside", "That one
 *           was set aside" — a count of the lines that count as done, and of
 *           nothing else;
 *   review  "The review was set aside" — the review, named apart.
 */
export function asideFactsOf(text) {
  const f = { bare: [], scoped: [], review: false }
  for (const m of text.matchAll(/(?:^|[^\w])(\d+) (?:was|were) set aside/g)) f.bare.push(Number(m[1]))
  if (/(?:^|[.] |: )It was set aside/.test(text)) f.bare.push(1)
  for (const m of text.matchAll(/(\d+) of them (?:was|were) set aside/g)) f.scoped.push(Number(m[1]))
  if (/That one was set aside/.test(text)) f.scoped.push(1)
  f.review = /The review was set aside/.test(text)
  return f
}

const factsOf = (text) => {
  const f = { need: [], met: null, total: null, none: false, unlinked: null, aside: asideFactsOf(text) }
  for (const m of text.matchAll(STILL_NEED)) f.need.push(Number(m[1]))
  let m = /(\d+) of (\d+) things that count as done have evidence/.exec(text)
  if (m) { f.met = Number(m[1]); f.total = Number(m[2]) }
  m = /The (\d+) thing that counts as done has evidence/.exec(text)
  if (m) { f.met = 1; f.total = 1 }
  m = /Accepted: (\d+) things? counts? as done/.exec(text)
  if (m) f.total = Number(m[1])
  f.none = /None has evidence|It has no evidence/.test(text)
  f.noneOfThisVersion = /None has evidence for this version yet|It has no evidence for this version yet/.test(text)
  m = /isn't linked to (\d+) of them|isn't linked to it/.exec(text)
  if (m) f.unlinked = m[1] ? Number(m[1]) : 1
  return f
}

/**
 * RULE 1. What a sentence says about what was set aside, held to what was.
 * `required`: the sentence is one that has to say it (a top with nothing left to
 * wait for; Ask's answer to "what still needs evidence" when nothing does).
 */
export function checkSetAside(at, text, X, required) {
  const a = asideFactsOf(text)
  for (const n of a.bare) {
    assert.equal(n, X.entries, `${at} — says ${n} were set aside, and the set-aside list holds ${X.entries} (${X.aside} lines, ${X.reviewAside ? 'and the review' : 'no review'})`)
  }
  for (const n of a.scoped) {
    assert.equal(n, X.aside, `${at} — says ${n} of the things that count as done were set aside, and ${X.aside} were`)
  }
  if (a.review) assert.ok(X.reviewAside, `${at} — says the review was set aside, and it was not`)
  if (required && X.entries > 0) {
    const bareTotal = a.bare.includes(X.entries)
    const named = (X.aside === 0 || a.scoped.includes(X.aside)) && (!X.reviewAside || a.review)
    assert.ok(bareTotal || named, `${at} — ${X.entries} were set aside (${X.aside} lines${X.reviewAside ? ', and the review' : ''}) and the sentence does not account for them`)
  }
  if (X.entries === 0) assert.ok(!a.bare.length && !a.scoped.length && !a.review, `${at} — says something was set aside, and nothing was`)
}

/** The top sentence, held to what the served readiness says. */
export function checkTop(where, text, X) {
  const f = factsOf(text)
  const at = `${where}: "${text}"`
  for (const n of f.need) assert.equal(n, X.need, `${at} — says ${n} still need evidence, and the served readiness names ${X.need} of the accepted lines`)
  if (f.met !== null) assert.equal(f.met, X.met, `${at} — says ${f.met} have evidence, and the served readiness says ${X.met} are met`)
  if (f.total !== null) assert.equal(f.total, X.total, `${at} — says ${f.total} count as done, and ${X.total} are accepted`)
  if (f.unlinked !== null) assert.equal(f.unlinked, X.unlinked, `${at} — names ${f.unlinked} unlinked lines, and the served readiness names ${X.unlinked}`)
  if (X.need > 0) {
    assert.ok(f.need.length > 0 || f.none || f.unlinked !== null, `${at} — ${X.need} line(s) still need evidence and the top says neither how many nor that none has any`)
  }
  if (f.none || f.unlinked !== null) {
    assert.ok(X.met === 0 && X.aside === 0 && X.beyond.length === 0 && X.need === X.total, `${at} — "none has evidence" is only true when every line still needs it`)
  }
  /* RULE 4. A line whose only run was about another version is not one that "has no evidence" without saying which version. */
  if (X.previous > 0 && f.none) assert.ok(f.noneOfThisVersion, `${at} — says none has evidence over ${X.previous} line(s) proven on another version, and does not say it is about this version`)
  if (X.need === 0) assert.ok(!f.need.some((n) => n > 0) && !f.none, `${at} — nothing waits for evidence`)
  /* RULE 1, for a top with nothing left to wait for: what was set aside is said. */
  checkSetAside(at, text, X, X.need === 0)
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

const entryOf = (line) => line.split(' — ')[0]

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
      assert.deepEqual((j.readiness.set_aside ?? []).map(entryOf), X.entryTexts, at('readiness.set_aside holds the entries that were set aside: the lines, then the review'))
    } else {
      for (const t of X.entryTexts) assert.ok(j.readiness.rests_on.some((r) => entryOf(r) === t), at(`a ready answer's file does not carry what was set aside: ${t}`))
    }
    return
  }
  const blocks = blocksOf(format, text)
  const h1 = blocks.findIndex((b) => b.tag === 'h1')
  const top = blocks.slice(h1 + 1).find((b) => b.tag === 'p')
  checkTop(at('the headline paragraph'), top.text, X)
  for (const b of blocks) for (const n of factsOf(b.text).need) assert.equal(n, X.need, at(`"${b.text}" says ${n}`))
  if (!X.ready) {
    /* The readiness sentence, the list under it, and then — each on its own — what is missing that is not a line and
     * the entries that were set aside. With no line waiting, the sentence IS what is missing that is not a line. */
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
    if (after !== undefined && X.entries > 0) {
      assert.equal(blocks[after] && blocks[after].text, M.HQ_COPY.change.model.readiness.set_aside_heading, at('the entries set aside have no heading of their own after the list'))
      const set = []
      for (let i = after + 1; i < blocks.length && blocks[i].tag === 'li'; i += 1) set.push(blocks[i].text)
      assert.deepEqual(set.map(entryOf), X.entryTexts, at('the entries set aside are not the ones a person set aside: the lines, then the review'))
    }
    const all = blocks.map((b) => b.text).join('\n')
    for (const words of X.beyond) assert.ok(all.includes(words), at(`the file does not say what is missing that is not a line: ${words}`))
  } else {
    /* A ready answer's file lists what it rests on, and after it what was set aside. */
    const items = blocks.filter((b) => b.tag === 'li').map((b) => entryOf(b.text))
    for (const t of X.entryTexts) assert.ok(items.includes(t), at(`a ready answer's file does not carry what was set aside: ${t}`))
  }
}

/** RULE 3. The tell-the-agent panel, held to the lines the served answer says still need evidence. */
export function checkTell(at, X, panel, tell) {
  const said = panel.join(' ')
  if (X.need > 0) {
    assert.doesNotMatch(said, /Nothing to ask the agent for/, `${at} — the panel says nothing is to be asked while ${X.need} line(s) still need evidence`)
    assert.ok(tell.draft, `${at} — no message for the agent is offered while ${X.need} line(s) still need evidence`)
    assert.equal(tell.covered, null, `${at} — the panel says the lines are covered`)
    for (const t of X.needTexts) assert.ok(tell.draft.includes(`"${t}"`), `${at} — the message for the agent does not ask about a line that still needs evidence: ${t}`)
    for (const t of [...X.asideTexts, ...X.metTexts]) assert.ok(!tell.draft.includes(t), `${at} — the message for the agent asks about a line that needs nothing: ${t}`)
    assert.deepEqual(tell.lines.filter((l) => l.kind !== 'proven' && l.kind !== 'set_aside').map((l) => l.n), X.needTexts.map((t) => Number(/^Line (\d+):/.exec(t)[1])), `${at} — the lines the message covers`)
  } else {
    assert.equal(tell.kind, 'covered', `${at} — nothing waits for evidence, and the panel offers a message`)
    assert.match(said, /Nothing to ask the agent for/, at)
    if (X.aside > 0) assert.match(said, /set aside by a person/, `${at} — lines were set aside, and the sentence says every line is proven`)
  }
}

export function checkCase(label, lines, opts, S) {
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
    /* RULE 1 and 2: the set-aside list holds the entries the top counts, and the review's names itself. */
    assert.deepEqual(b.setAside.map(entryOf), X.entryTexts, `${w}: the set-aside list holds ${JSON.stringify(b.setAside.map(entryOf))}; ${JSON.stringify(X.entryTexts)} were set aside`)
    assert.equal(b.setAsideHeading.length, X.entries > 0 ? 1 : 0, `${w}: the heading over the set-aside list`)
    /* RULE 3: the panel, where the screen draws it (Brief, and the Verify stage below). */
    if (key.startsWith('brief/')) checkTell(`${w}, the message for the agent`, X, b.tell, S.tell)
  }
  checkTell(at('the Verify stage, the message for the agent'), X, S.verify.tell, S.tell)

  /* THE WHOLE-STORY PAGE: its readiness block and its own list. */
  if (!X.ready && X.need > 0) {
    assert.deepEqual(factsOf(S.page.readinessLine.join(' ')).need, [X.need], at(`the page's readiness line: "${S.page.readinessLine.join(' ')}"`))
    assert.equal(S.page.readinessRows.length, X.need, at(`the page's readiness block lists ${S.page.readinessRows.length} lines; ${X.need} still need evidence`))
    for (const t of X.asideTexts) assert.ok(!S.page.readinessRows.some((r) => r.startsWith(t + ' — ')), at('a set-aside line is listed under what still needs evidence in the page readiness block'))
  }
  for (const words of X.beyond) assert.ok(S.page.readinessBlock.join(' ').includes(words), at(`the page does not say, said on its own: ${words}`))
  assert.deepEqual(S.page.needsLines, X.needTexts, at(`the page's "What still needs evidence" lists ${JSON.stringify(S.page.needsLines)}, the lines the answer names are ${JSON.stringify(X.needTexts)}`))
  assert.equal(S.page.needsDoors.length, X.unlinked, at('the page offers the door to link a run at each line whose run nobody linked, and at no other'))
  if (!X.ready) {
    assert.deepEqual(S.page.setAsideRows.map(entryOf), X.entryTexts, at('the page lists what was set aside: the lines, then the review'))
  } else {
    /* A ready answer's page lists what it rests on, and after it what was set aside. */
    for (const t of X.entryTexts) assert.ok(S.page.readinessRows.some((r) => entryOf(r) === t), at(`the page does not carry what was set aside: ${t}`))
  }
  if (!X.ready && X.entries > 0) {
    const block = S.page.readinessBlock.join(' ')
    assert.ok(block.includes(M.HQ_COPY.change.model.readiness.set_aside_heading), at('the page does not say what was set aside is set aside'))
    for (const t of X.entryTexts) assert.ok(block.includes(t), at(`the page does not say something was set aside: ${t}`))
  }
  if (X.need > 0) {
    assert.deepEqual(S.page.needsHeading, [M.HQ_COPY.change.packet.needs.heading], at('the page heading over a list of what needs evidence'))
  } else {
    assert.notDeepEqual(S.page.needsHeading, [M.HQ_COPY.change.packet.needs.heading], at('a heading that asks what still needs evidence, over nothing'))
    if (X.entries > 0) assert.doesNotMatch(S.page.needsSection.join(' '), /has something behind it/, at('"has something behind it" is said of something nobody checked'))
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
    checkSetAside(at('Ask'), S.ask.sentences[0] || '', X, true)
  }
  for (const words of X.beyond) assert.ok(S.ask.sentences.some((s) => s.includes(words)), at(`Ask does not say, said on its own: ${words}`))
  if (X.need === 0 && X.entries > 0) assert.doesNotMatch(S.ask.sentences[0], /has something behind it/, at('Ask says something nobody checked has something behind it'))

  /* THE INBOX: its row says the top, and counts what needs you. */
  const items = /(\d+) items? on this Change needs?/.exec(S.row.items[0] || '')
  assert.ok(items, at(`the Inbox row states how many items need you: "${S.row.items[0]}"`))
  assert.equal(Number(items[1]), X.need + X.beyond.length + (X.ready ? 1 : 0), at('the Inbox row counts the lines that still need evidence, each thing that is not a line once, and a decision waiting for its approver'))
  assert.equal(S.item.attention.items, Number(items[1]), at('the item and the words agree'))
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

const labelOf = (lines, o) => `[${lines.join(', ')}]${o.identity === false ? ' + identity unconfirmed' : ''}${o.review && o.review !== 'none' ? ` + review ${o.review}` : ''}`

/**
 * The round-2 adversary's systematic shape: n = 1..12 lines, half of one outcome
 * and half of another, over every ordered pair of six outcomes, with the identity
 * confirmed and not and the review in each of its five states. (For n = 1 the
 * pair is one line and the second outcome is moot, so the same case is added once.)
 */
export function systematic() {
  const out = []
  const seen = new Set()
  for (let n = 1; n <= 12; n += 1) {
    for (const a of SIX) {
      for (const b of SIX) {
        const lines = [...Array(Math.ceil(n / 2)).fill(a), ...Array(Math.floor(n / 2)).fill(b)]
        const key = lines.join(',')
        if (seen.has(key)) continue
        seen.add(key)
        for (const identity of [true, false]) {
          for (const review of REVIEWS) out.push({ lines, opts: { identity, review }, label: labelOf(lines, { identity, review }) })
        }
      }
    }
  }
  return out
}

/** A seeded mix over all nine outcomes, n = 1..12, the review in each of its states. */
export function random(count, seed0) {
  let seed = seed0
  const next = () => { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 2 ** 32 }
  const out = []
  for (let k = 0; k < count; k += 1) {
    const n = 1 + Math.floor(next() * 12)
    const lines = Array.from({ length: n }, () => OUTCOMES[Math.floor(next() * OUTCOMES.length)])
    const opts = { identity: next() > 0.3, review: REVIEWS[Math.floor(next() * REVIEWS.length)] }
    out.push({ lines, opts, label: labelOf(lines, opts) })
  }
  return out
}

/** One shard of a sweep: every `count`th case from `index`, so each shard has the same mix of sizes. */
export function runShard(name, all, index, count) {
  test(`${name}, shard ${index + 1} of ${count}: the count of what was set aside, the review's entry, the message for the agent and the top say what the served answer says`, async () => {
    const mine = all.filter((_, i) => i % count === index)
    assert.ok(mine.length > 0, 'the shard is empty')
    const failures = []
    for (const c of mine) {
      try {
        checkCase(c.label, c.lines, c.opts, await surfaces(c.lines, c.opts))
      } catch (e) {
        failures.push(e.message.split('\n')[0])
      }
    }
    assert.deepEqual(failures.slice(0, 12), [], `${failures.length} of ${mine.length} cases disagree with themselves; the first ones:`)
  })
}
