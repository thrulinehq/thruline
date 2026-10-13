/* HTT-TEXT-RUN-TOGETHER (stage A3, had-to-think sweep 2026-09-26) — adjacent
 * sentences never render with no space between them.
 *
 * Three places were named, all found by reading the screen, none by a test:
 *
 *   1. the criteria list: "Uploads retry three timesNo test ran for this."
 *      (the line and its status sentence are two inline elements with nothing
 *      between them — the Readiness list on Brief and Explore);
 *   2. the story's readiness line: "…shows zero.This was checked, and the
 *      files it looked at have changed since." (the same list, on the reason
 *      `readiness_stale`);
 *   3. the message for the agent: a multi-line failing line runs straight into
 *      "Please fix it, then run …" on its last line.
 *
 * At cf8ca110 the first two are `change-workspace.jsx` `Readiness` (the
 * `ws-readiness-what` and `ws-readiness-why` spans, on the missing list and on
 * the set-aside list), and the third is `tell-draft.js` `paragraph`. The card's
 * file list names 068cd5d4: the criterion row is `Readiness` in
 * `change-workspace.jsx` (not `coverage-view.js`), the reason sentences are
 * still `hq-copy.js` `readiness_stale` and friends, and the join in the draft
 * moved from line 105 to `paragraph` in `tell-draft.js`.
 *
 * The test reads the surfaces a person reads — the Change screen (Brief and
 * Explore, every stage), the whole-story page and the exported Markdown and
 * HTML — over the documents the server served in a real walk
 * (`walk-capture.fixture.json`, masked), and over the same documents edited
 * into the states that walk did not reach (nothing ran, a passing run nobody
 * linked, never assessed, a line set aside). "Visible text" is what
 * innerText gives in Chrome: inline elements concatenate, block boundaries
 * (and buttons and summaries) are newlines.
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
const temporary = await mkdtemp(join(tmpdir(), 'thruline-run-together-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeShell, Readiness, readChange } from './change-workspace.jsx';
  import { PacketPage, readPacket } from './packet.jsx';
  import { deriveChangeModel } from './change-model.js';
  import { tellDraftOf } from './tell-draft.js';
  import { renderExport } from './packet-export.js';
  import { HQ_COPY } from './hq-copy.js';
  export { readChange, readPacket, deriveChangeModel, tellDraftOf, renderExport, HQ_COPY };
  export const renderReadiness = props => renderToStaticMarkup(React.createElement(Readiness, props));
  export const renderRoute = props => renderToStaticMarkup(React.createElement(ChangeShell, props));
  export const renderPage = props => renderToStaticMarkup(React.createElement(PacketPage, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'run-together.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const CAPTURE = JSON.parse(readFileSync(join(here, 'walk-capture.fixture.json'), 'utf8'))
const ID = CAPTURE.change_id
const clone = (v) => JSON.parse(JSON.stringify(v))

/** `readChange` and `readPacket` over one moment of the walk, `fetch` answered from it. */
async function readMoment(moment, edit) {
  const reads = clone(CAPTURE.moments[moment])
  if (edit) edit(reads)
  const real = globalThis.fetch
  globalThis.fetch = async (url) => {
    const r = reads[String(url)]
    const headers = { get: (h) => (!r ? null : h === 'X-ThruLine-Exit' ? r.exit : h === 'X-ThruLine-Projection' ? r.projection : null) }
    if (!r) return { ok: false, status: 404, headers, text: async () => '' }
    return { ok: r.status >= 200 && r.status < 300, status: r.status, headers, text: async () => JSON.stringify(r.body) }
  }
  try {
    return { screen: await M.readChange(ID), packet: await M.readPacket(ID) }
  } finally {
    globalThis.fetch = real
  }
}

const COV = (reads) => reads[`/api/changes/${ID}/coverage`].body
const noop = () => {}

/* The states the walk did not reach, made from the same served documents. */
const CASES = {
  'after a passing run (linked, ready)': ['after_pass', null],
  'after a failing re-run (stale)': ['after_fail', null],
  'after the passing re-run': ['after_repass', null],
  'nothing has run': ['after_pass', (reads) => {
    const c = COV(reads)
    c.coverage.criteria[0] = { ...c.coverage.criteria[0], items: [], state: 'none', state_slug: 'coverage_none' }
    c.coverage.candidates = []
    c.readiness.ready = false
    c.readiness.headline_slug = 'readiness_needs_evidence_one'
    c.readiness.missing = ['criterion:1']
    c.readiness.rests_on = []
    c.readiness.obligations[1] = { ...c.readiness.obligations[1], outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked', evidence_name: null }
  }],
  'a passing run nobody linked': ['after_pass', (reads) => {
    const c = COV(reads)
    c.coverage.criteria[0] = { ...c.coverage.criteria[0], items: [], state: 'none', state_slug: 'coverage_none' }
    c.readiness.ready = false
    c.readiness.headline_slug = 'readiness_needs_evidence_one'
    c.readiness.missing = ['criterion:1']
    c.readiness.rests_on = []
    c.readiness.obligations[1] = { ...c.readiness.obligations[1], outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'unlinked_run', unsatisfied_slug: 'readiness_unlinked_run', evidence_name: null }
  }],
  'a linked run never assessed': ['after_pass', (reads) => {
    const c = COV(reads)
    c.readiness.ready = false
    c.readiness.headline_slug = 'readiness_needs_evidence_one'
    c.readiness.missing = ['criterion:1']
    c.readiness.rests_on = []
    c.readiness.obligations[1] = { ...c.readiness.obligations[1], outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'never_assessed', unsatisfied_slug: 'readiness_never_assessed', evidence_name: null }
  }],
  'a line set aside by a person': ['after_pass', (reads) => {
    const c = COV(reads)
    c.readiness.with_exceptions = true
    c.readiness.headline_slug = 'readiness_ready_with_exceptions'
    c.readiness.waived = ['criterion:1']
    c.readiness.rests_on = []
    c.readiness.obligations[1] = {
      ...c.readiness.obligations[1], outcome: 'waived', outcome_slug: 'waived', evidence_name: null,
      waiver: { actor: 'sam', authority: 'the accepted profile', rationale: 'Nobody checked this.' },
    }
  }],
}

/* ── what a person reads ──────────────────────────────────────────────── */

const BLOCKS = new Set([
  'ADDRESS', 'ARTICLE', 'ASIDE', 'BLOCKQUOTE', 'BODY', 'BUTTON', 'DD', 'DETAILS', 'DIALOG', 'DIV', 'DL', 'DT', 'FIELDSET', 'FIGCAPTION', 'FIGURE',
  'FOOTER', 'FORM', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'HEADER', 'HR', 'INPUT', 'LABEL', 'LI', 'MAIN', 'NAV', 'OL', 'OPTION', 'P', 'PRE', 'SECTION',
  'SELECT', 'SUMMARY', 'TABLE', 'TBODY', 'TD', 'TEXTAREA', 'TH', 'THEAD', 'TR', 'UL', 'BR',
])

/** The text a browser shows: inline elements concatenate, block boundaries are newlines. */
function visibleText(html) {
  const doc = new JSDOM('<!doctype html><body>' + html + '</body>').window.document
  const out = []
  const walk = (node) => {
    if (node.nodeType === 3) { out.push(node.nodeValue); return }
    if (node.nodeType !== 1) return
    const tag = node.tagName.toUpperCase()
    if (tag === 'SCRIPT' || tag === 'STYLE') return
    const block = BLOCKS.has(tag)
    if (block) out.push('\n')
    for (const child of node.childNodes) walk(child)
    if (block) out.push('\n')
  }
  walk(doc.body)
  return out.join('').replace(/[ \t\r\f]+/g, ' ')
}

/** A lower-case letter, a full stop or a bracket, then a capital, with nothing between. The product's own name is one word with a capital in it. */
const JOIN = /[a-z.)][A-Z]/g
const joins = (text) => {
  const clean = text.replaceAll('ThruLine', 'Thruline')
  return [...clean.matchAll(JOIN)].map((m) => clean.slice(Math.max(0, m.index - 40), m.index + 40).replace(/\n/g, '⏎'))
}

/* The places that carry a line of what counts as done beside a sentence about
 * it, and the message for the agent. Chrome's innerText is what the sweep read;
 * a static render has no stylesheet, so the scan is confined to the containers
 * whose parts are all inline text — a rail item or a stage button, whose two
 * spans are stacked by CSS, would read as a false join here. */
const REGIONS = [
  '.ws-change-readiness', '.ws-cov-rows', '.ws-packet-needs', '.ws-packet-mapping', '.ws-packet-readiness', '.ws-packet-rows', '.ws-tell-draft', '.ws-tell', '.ws-brief-story',
]
const regionsOf = (html) => {
  const doc = new JSDOM('<!doctype html><body>' + html + '</body>').window.document
  return [...doc.querySelectorAll(REGIONS.join(','))].map((el) => visibleText(el.outerHTML))
}

const surfacesOf = ({ screen, packet }) => {
  const out = {}
  for (const altitude of ['brief', 'explore']) {
    for (const stage of ['define', 'prepare', 'build', 'verify', 'review', 'approve']) {
      regionsOf(M.renderRoute({
        model: screen.model, stage, altitude, register: 'manager',
        onGo: noop, onCopy: noop, onReload: noop, onAltitude: noop, onRegister: noop,
      })).forEach((t, i) => { out[`Change screen · ${altitude} · ${stage} · region ${i}`] = t })
    }
  }
  regionsOf(M.renderPage({ model: packet.model, notes: packet.notes, onCopy: noop, records: packet.records })).forEach((t, i) => { out[`the whole-story page · region ${i}`] = t })
  out['exported Markdown'] = M.renderExport(packet.model.hq.packet, packet.records, { format: 'md' }).text
  out['exported HTML'] = visibleText(M.renderExport(packet.model.hq.packet, packet.records, { format: 'html' }).text)
  return out
}

for (const [name, [moment, edit]] of Object.entries(CASES)) {
  test(`no two sentences run together on any surface — ${name}`, async () => {
    const read = await readMoment(moment, edit)
    assert.equal(read.screen.status, 'ready')
    assert.equal(read.packet.status, 'ready')
    const bad = []
    for (const [where, text] of Object.entries(surfacesOf(read))) for (const j of joins(text)) bad.push(`${where}: …${j}…`)
    assert.deepEqual([...new Set(bad)], [])
  })
}

test('the run-together check is a live instrument: it sees the two inline spans the Readiness list used to join', () => {
  const found = joins(visibleText('<li><span>zero.</span><span>No test ran</span></li>'))
  assert.equal(found.length, 1)
  assert.ok(found[0].includes('zero.No test ran'), found[0])
  assert.deepEqual(joins(visibleText('<li><p>zero.</p><p>No test ran</p></li>')), [])
  assert.deepEqual(joins(visibleText('<p>Open ThruLine, then ThruLine again</p>')), [])
})

/* ── the readiness list says the line and its reason apart, on screen ──── */

test('each line of the still-needs list and of the set-aside list is the line, a dash and its reason — the same shape the whole-story page uses', async () => {
  for (const name of ['nothing has run', 'a passing run nobody linked', 'a line set aside by a person', 'after a failing re-run (stale)']) {
    const [moment, edit] = CASES[name]
    const { screen } = await readMoment(moment, edit)
    const brief = visibleText(M.renderRoute({ model: screen.model, stage: 'verify', altitude: 'brief', register: 'manager', onGo: noop, onCopy: noop, onReload: noop, onAltitude: noop, onRegister: noop }))
    const at = brief.split('\n').find((l) => l.startsWith('When the cart is empty, the total shows zero.') && / — (No test ran|A test ran|Nobody checked|Waived|This was checked)/.test(l))
    assert.ok(at, `${name}: no readiness line reads "line — reason"\n${brief.split('\n').filter((l) => l.includes('total shows zero')).join('\n')}`)
  }
})

/* ── the message for the agent: a multi-line failure is quoted, then the ask ── */

const T = M.HQ_COPY.change.tell.draft
const TWO_LINES = 'thread \'tests::an_empty_cart_totals_zero\' panicked at src/lib.rs:12:9:\nassertion `left == right` failed\n  left: 1\n right: 0'

async function failingHq(failureLine) {
  const { screen } = await readMoment('after_fail', (reads) => {
    const cov = COV(reads)
    for (const item of cov.coverage.criteria[0].items) if (item.observed === 'not_proven') item.failure_line = failureLine
    const run = reads[`/api/changes/${ID}/run`].body.checks[0].last
    run.failure_line = failureLine
  })
  return screen.model.hq
}

test('a two-line failure is quoted whole and the ask starts on a line of its own', async () => {
  const hq = await failingHq(TWO_LINES)
  const tell = M.tellDraftOf(hq)
  assert.equal(tell.kind, 'opposite')
  const at = tell.draft.indexOf('What the run printed when it broke: ')
  assert.ok(at >= 0, 'the failure is quoted')
  const lines = tell.draft.slice(at).split('\n')
  assert.deepEqual(lines.slice(0, 5), [
    'What the run printed when it broke: ' + TWO_LINES.split('\n')[0],
    TWO_LINES.split('\n')[1],
    TWO_LINES.split('\n')[2],
    TWO_LINES.split('\n')[3],
    lines[4],
  ])
  assert.match(lines[4], /^Please fix it, then run .+ again on the current code\.$/, 'the ask is its own line, and nothing of the failure is on it')
  assert.doesNotMatch(lines[3], /Please fix it/, 'the last line of the failure does not run into the ask')
  /* The failure is data, and it is still whole. */
  assert.ok(tell.draft.includes(TWO_LINES))
})

test('a one-line failure keeps its draft exactly: the ask follows on the same paragraph, as it did', async () => {
  const one = 'assertion failed: left == right'
  const tell = M.tellDraftOf(await failingHq(one))
  assert.ok(tell.draft.includes(`What the run printed when it broke: ${one} Please fix it, then run `), tell.draft)
})

test('with no failing line the draft is unchanged', async () => {
  const tell = M.tellDraftOf(await failingHq(null))
  assert.ok(tell.draft.includes(`${T.failure_none} Please fix it, then run `), tell.draft)
})

/* ── self-attack ──────────────────────────────────────────────────────── */

test('a Windows line ending inside the failure counts as a line break, and the failure is still quoted whole', async () => {
  const crlf = 'assertion failed: left == right\r\n  left: 1\r\n right: 0'
  const tell = M.tellDraftOf(await failingHq(crlf))
  assert.ok(tell.draft.includes(crlf))
  assert.match(tell.draft, / right: 0\nPlease fix it, then run .+ again on the current code\.$/m)
})

test('a failure whose only extra line break is at its end is one line: it is trimmed, and the draft is the one-line draft', async () => {
  const one = M.tellDraftOf(await failingHq('assertion failed: left == right'))
  const trailing = M.tellDraftOf(await failingHq('assertion failed: left == right\n'))
  assert.equal(trailing.draft, one.draft)
})

test('the readiness list puts a dash only between a line and a reason that are both there', () => {
  const render = (missing, waivedLines = []) => visibleText(M.renderReadiness({
    readiness: { absent: false, sentence: '1 thing still needs evidence:', heading: 'x', ready: false, missing, waivedLines, setAsideHeading: 'Set aside' },
  }))
  const both = render([{ id: 'a', text: 'Uploads retry three times', reason: 'No test ran for this.' }])
  assert.ok(both.includes('Uploads retry three times — No test ran for this.'), both)
  assert.deepEqual(joins(both), [])
  const noReason = render([{ id: 'a', text: 'Uploads retry three times', reason: null }])
  assert.ok(!noReason.includes('—'), noReason)
  const noText = render([{ id: 'a', text: null, reason: 'No test ran for this.' }])
  assert.ok(!noText.includes('—'), noText)
  const aside = render([], [{ id: 'b', text: 'Uploads retry three times', waiver: 'Waived by sam, under the accepted profile: Nobody checked this. Nobody checked this.' }])
  assert.ok(aside.includes('Uploads retry three times — Waived by sam'), aside)
  assert.deepEqual(joins(aside), [])
})
