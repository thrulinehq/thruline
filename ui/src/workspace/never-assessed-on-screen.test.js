/* NEVER-ASSESSED-ON-SCREEN (path step 8) — after a person links a run, the
 * screen never says "0 still need it" above "1 thing still needs evidence",
 * and its remedy for a line that was never assessed is an act the screen
 * offers: Check it again on this Change, not a terminal command.
 *
 * The walk's sequence, on the documents the server served in a real walk
 * (`walk-capture.fixture.json`, moment `after_pass`: a passing run, linked by a
 * person, no bind since) with the served readiness set to what the engine
 * serves for it: not ready, the line never assessed (R-2026-09-28-25).
 *
 * Read through `readChange` and `readPacket` — the browser's read paths, with
 * `fetch` answered from the capture — so the Change screen (Brief and
 * Explore), the whole-story page, the Inbox row and the exported files are
 * all read from one derivation.
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
const temporary = await mkdtemp(join(tmpdir(), 'thruline-never-assessed-screen-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeShell, readChange } from './change-workspace.jsx';
  import { PacketPage, readPacket } from './packet.jsx';
  import { deriveChangeModel, readinessOf } from './change-model.js';
  import { inboxItem } from './inbox-view.js';
  import { renderExport } from './packet-export.js';
  import { HQ_COPY } from './hq-copy.js';
  export { readChange, readPacket, deriveChangeModel, readinessOf, inboxItem, renderExport, HQ_COPY };
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
  const file = join(temporary, 'never-assessed-screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const CAPTURE = JSON.parse(readFileSync(join(here, 'walk-capture.fixture.json'), 'utf8'))
const ID = CAPTURE.change_id
const clone = (v) => JSON.parse(JSON.stringify(v))
const noop = () => {}
const LINE = CAPTURE.moments.after_pass[`/api/changes/${ID}`].body.change.accepted_scope.criteria[0]

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

/** The walk's step 8: linked by a person, no bind since — the engine's answer is not ready, the line never assessed. */
const NEVER_ASSESSED = (reads) => {
  const c = COV(reads)
  c.readiness.ready = false
  c.readiness.headline_slug = 'readiness_needs_evidence_one'
  c.readiness.missing = ['criterion:1']
  c.readiness.rests_on = []
  c.readiness.obligations[1] = { ...c.readiness.obligations[1], outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'never_assessed', unsatisfied_slug: 'readiness_never_assessed', evidence_name: null }
}

const text = (html) => new JSDOM('<!doctype html><body>' + html + '</body>').window.document.body.textContent.replace(/\s+/g, ' ').trim()
const route = (model, altitude, stage = 'define') => M.renderRoute({
  model, stage, altitude, register: 'manager', onGo: noop, onCopy: noop, onReload: noop, onAltitude: noop, onRegister: noop,
})
const conclusionsOf = (html) => [...new JSDOM('<!doctype html><body>' + html + '</body>').window.document.querySelectorAll('.ws-change-conclusion')].map((p) => p.textContent.trim())

const C = M.HQ_COPY.change
const R = C.model.readiness
const TERMINAL_REASON = 'Whether this still applies was never assessed. Run `thruline change prepare` and it will be.'
const SCREEN_REASON = 'Whether this still applies was never assessed. Check it again on this Change, and it will be.'
const OLD_CONCLUSION = '1 of 1 things that count as done have evidence. 0 still need it.'
const NEW_CONCLUSION = 'Accepted: 1 thing counts as done. 1 thing still needs evidence.'

/* ── one reading, the stricter ────────────────────────────────────────── */

test('no single view model says "0 still need it" and answers "not ready": the top says what the readiness answer says', async () => {
  const { screen } = await readMoment('after_pass', NEVER_ASSESSED)
  const hq = screen.model.hq
  assert.equal(hq.readiness.ready, false)
  assert.equal(hq.readiness.missingCount, 1)
  assert.equal(hq.headline.text, NEW_CONCLUSION, 'the same clause the readiness answer prints, count from readinessOf')
  assert.doesNotMatch(hq.headline.text, /0 still need|have evidence/)
  /* The readiness answer's own sentence, from the same derivation. */
  assert.equal(hq.readiness.sentence, '1 thing still needs evidence:')
  assert.ok(NEW_CONCLUSION.endsWith(hq.readiness.sentence.replace(/:$/, '.')))
  /* And the counts are the coverage's, untouched. */
  assert.deepEqual(hq.counts.withEvidence, 1)
  assert.deepEqual(hq.counts.withoutEvidence, 0)
})

test('every surface that prints the conclusion says the same thing: Brief, Explore, the whole-story page, the Inbox row, the exported files', async () => {
  const { screen, packet } = await readMoment('after_pass', NEVER_ASSESSED)
  const seen = new Set()
  for (const altitude of ['brief', 'explore']) for (const c of conclusionsOf(route(screen.model, altitude))) seen.add(c)
  for (const c of conclusionsOf(M.renderPage({ model: packet.model, notes: packet.notes, onCopy: noop, records: packet.records }))) seen.add(c)
  assert.deepEqual([...seen], [NEW_CONCLUSION])
  const hq = screen.model.hq
  assert.equal(hq.inboxRow.reason, NEW_CONCLUSION)
  assert.equal(hq.inspector.what.summary, NEW_CONCLUSION)
  for (const format of ['md', 'json']) {
    const file = M.renderExport(packet.model.hq.packet, packet.records, { format }).text
    assert.ok(file.includes(NEW_CONCLUSION), `${format} says it`)
    assert.ok(!file.includes(OLD_CONCLUSION), `${format} no longer says nothing still needs evidence`)
  }
  assert.ok(text(M.renderExport(packet.model.hq.packet, packet.records, { format: 'html' }).text).includes(NEW_CONCLUSION))
})

test('claims the same: when the answer is ready, or none is served, or the count is not zero, every word is what it was', async () => {
  /* Ready: the sentence the walk showed before the readiness moved. */
  const ready = await readMoment('after_pass')
  assert.equal(ready.screen.model.hq.readiness.ready, true)
  assert.equal(ready.screen.model.hq.headline.text, OLD_CONCLUSION)
  assert.deepEqual(conclusionsOf(route(ready.screen.model, 'explore')), [OLD_CONCLUSION])
  /* No readiness served at all: absence is not an answer. */
  const none = await readMoment('after_pass', (reads) => { delete COV(reads).readiness })
  assert.equal(none.screen.model.hq.readiness.absent, true)
  assert.equal(none.screen.model.hq.headline.text, OLD_CONCLUSION)
  /* Not ready, and nothing has any evidence: "no evidence yet", unchanged. */
  const nothing = await readMoment('after_pass', (reads) => {
    const c = COV(reads)
    c.coverage.criteria[0] = { ...c.coverage.criteria[0], items: [], state: 'none', state_slug: 'coverage_none' }
    c.coverage.candidates = []
    c.readiness.ready = false
    c.readiness.headline_slug = 'readiness_needs_evidence_one'
    c.readiness.missing = ['criterion:1']
    c.readiness.rests_on = []
    c.readiness.obligations[1] = { ...c.readiness.obligations[1], outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked', evidence_name: null }
  })
  assert.equal(nothing.screen.model.hq.headline.text, 'Accepted: 1 thing counts as done. It has no evidence yet.')
  /* Not ready, and a failure: the problem sentence, unchanged. */
  const failed = await readMoment('after_fail')
  assert.equal(failed.screen.model.hq.readiness.ready, false)
  assert.equal(failed.screen.model.hq.headline.text, 'The record shows a problem with 1 of 1 things.')
})

/* ── the remedy is an act the screen offers ───────────────────────────── */

test('the reason for a line never assessed names Check it again on this Change, on the screen, the whole-story page and the exported files', async () => {
  const { screen, packet } = await readMoment('after_pass', NEVER_ASSESSED)
  const hq = screen.model.hq
  assert.equal(hq.readiness.missing[0].reason, SCREEN_REASON)
  const wanted = LINE + ' — ' + SCREEN_REASON
  assert.deepEqual(packet.model.hq.packet.readinessRests, [wanted])
  for (const html of [route(screen.model, 'brief'), route(screen.model, 'explore', 'verify'), M.renderPage({ model: packet.model, notes: packet.notes, onCopy: noop, records: packet.records })]) {
    assert.ok(text(html).includes(SCREEN_REASON), 'the reason, in Check-it-again words')
    assert.ok(!text(html).includes('thruline change prepare'), 'no terminal command on a screen with no control that runs it')
  }
  assert.ok(M.renderExport(packet.model.hq.packet, packet.records, { format: 'md' }).text.includes(wanted))
})

test('the disabled Approve names the same act, in the Approve pair, on Brief, the Inspector and the whole-story page', async () => {
  const { screen, packet } = await readMoment('after_pass', NEVER_ASSESSED)
  const hq = screen.model.hq
  const pair = hq.readiness.blocker
  assert.match(pair.reason, /never assessed/)
  assert.match(pair.enabler, /Check it again/)
  assert.doesNotMatch(pair.enabler, /thruline change prepare|terminal/)
  assert.doesNotMatch(pair.enabler, /turns on/)
  assert.equal(hq.inspector.action.enabler, pair.enabler)
  const brief = text(route(screen.model, 'brief'))
  assert.ok(brief.includes(pair.enabler), 'Brief prints the pair')
  assert.ok(text(M.renderPage({ model: packet.model, notes: packet.notes, onCopy: noop, records: packet.records })).includes(pair.enabler))
  assert.ok(hq.readiness.offLines[0].includes(pair.enabler))
})

test('and the screen offers that act: Check it again is on Brief, from the story and from the Inspector', async () => {
  const { screen } = await readMoment('after_pass', NEVER_ASSESSED)
  const doc = new JSDOM('<!doctype html><body>' + route(screen.model, 'brief') + '</body>').window.document
  const doors = [...doc.querySelectorAll('button[aria-haspopup="dialog"]')].map((b) => b.textContent.trim())
  assert.ok(doors.length >= 1, 'a run door is on the screen')
  assert.ok(doors.some((label) => /Check it again|Run the check/.test(label)), doors.join(' | '))
})

test('the Inspector no longer says nothing would settle a line the engine says was never assessed', async () => {
  const { screen } = await readMoment('after_pass', NEVER_ASSESSED)
  const settle = screen.model.hq.inspector.settle.sentence
  assert.doesNotMatch(settle, /Nothing — the record already backs this/)
  assert.match(settle, /never assessed/)
  assert.match(settle, /Check it again/)
})

test('the terminal keeps its own words: the mirrored sentence is untouched, and voice.rs still says it', () => {
  assert.equal(R.reasons.readiness_never_assessed, TERMINAL_REASON)
  const voice = readFileSync(join(here, '..', '..', '..', 'crates', 'core', 'src', 'voice.rs'), 'utf8')
  assert.ok(voice.includes('Whether this still applies was never assessed. Run `thruline change prepare` and it will be.'), 'voice.rs is byte-identical in this sentence')
  assert.ok(!voice.includes('Check it again on this Change, and it will be'), 'voice.rs did not learn the screen\'s words')
})

test('where the screen offers no way to check again for this Change, the terminal words stay', async () => {
  const { screen } = await readMoment('after_pass', NEVER_ASSESSED)
  const reads = clone(CAPTURE.moments.after_pass)
  NEVER_ASSESSED(reads)
  const change = reads[`/api/changes/${ID}`].body.change
  const served = COV(reads)
  const hq = M.deriveChangeModel({ ...change, id: undefined }, { ...served.coverage, readiness: served.readiness }, null, null, [])
  assert.equal(hq.readiness.missing[0].reason, TERMINAL_REASON)
  assert.match(hq.readiness.blocker.enabler, /thruline change prepare/)
  assert.ok(screen.model.hq.readiness.missing[0].reason !== TERMINAL_REASON)
})

/* ── the promise is kept by the code it names ─────────────────────────── */

test('the promise "and it will be" holds for a run started from the screen: the run route runs the one wrapper, which derives the links and re-binds the version, and the re-bind runs the freshness assessment', () => {
  const crates = join(here, '..', '..', '..', 'crates')
  const route = readFileSync(join(crates, 'cli', 'src', 'uiserve', 'run_route.rs'), 'utf8')
  const check = readFileSync(join(crates, 'cli', 'src', 'check.rs'), 'utf8')
  const inputs = readFileSync(join(crates, 'core', 'src', 'examined_inputs.rs'), 'utf8')
  /* The run route is a door into the one runner, for this Change. */
  assert.match(route, /check::run_check_at\(/)
  assert.match(route, /for_change: Some\(change_id\.to_string\(\)\)/)
  /* The runner, after the run, derives the links and then re-binds the Change's intended record. */
  const at = check.indexOf('fn record_check_session(')
  const body = check.slice(at, check.indexOf('\n}\n', at))
  assert.ok(body.includes('derive_links('), 'links are derived before the bind')
  assert.ok(body.includes('bind_intended_at('), 'the version is bound')
  assert.ok(body.indexOf('derive_links(') < body.indexOf('bind_intended_at('), 'in that order')
  /* And the bind is what runs the freshness assessment. */
  const bind = inputs.slice(inputs.indexOf('pub fn bind_intended_at('))
  assert.ok(bind.slice(0, bind.indexOf('\n}\n')).includes('on_intended_bound('), 'bind_intended_at runs the freshness assessment')
})

/* ── self-attack ──────────────────────────────────────────────────────── */

const LINE_TWO = 'The total shows the sum of the prices.'
const twoLines = (reads) => {
  const change = reads[`/api/changes/${ID}`].body.change
  change.accepted_scope.criteria = [change.accepted_scope.criteria[0], LINE_TWO]
  const c = COV(reads)
  c.coverage.criteria = [c.coverage.criteria[0], { ...clone(c.coverage.criteria[0]), index: 2, text: LINE_TWO }]
  c.readiness.obligations = [c.readiness.obligations[0], c.readiness.obligations[1], { ...c.readiness.obligations[1], id: 'criterion:2', criterion_index: 2, text: LINE_TWO }]
}
const neverAssessed = (o) => ({ ...o, outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'never_assessed', unsatisfied_slug: 'readiness_never_assessed', evidence_name: null })

test('two lines, both never assessed: the plural forms read as the readiness answer does', async () => {
  const { screen } = await readMoment('after_pass', (reads) => {
    twoLines(reads)
    const c = COV(reads)
    c.readiness.ready = false
    c.readiness.headline_slug = 'readiness_needs_evidence'
    c.readiness.missing = ['criterion:1', 'criterion:2']
    c.readiness.rests_on = []
    c.readiness.obligations[1] = neverAssessed(c.readiness.obligations[1])
    c.readiness.obligations[2] = neverAssessed(c.readiness.obligations[2])
  })
  const hq = screen.model.hq
  assert.equal(hq.readiness.sentence, '2 things still need evidence:')
  assert.equal(hq.headline.text, 'Accepted: 2 things count as done. 2 things still need evidence.')
  assert.deepEqual(conclusionsOf(route(screen.model, 'explore')), [hq.headline.text])
  assert.deepEqual(hq.readiness.missing.map((m) => m.reason), [SCREEN_REASON, SCREEN_REASON])
})

/* BRIEF-COUNT-AGREES moved this expected value (D-050). It read "1 of 2 things that count as done have evidence. 1 still needs it." — "the count is
 * not zero, so the top is the sentence it was" — above a list that said "2 things still need evidence:". The old value encoded the defect the walk
 * found: one noun phrase, two numbers, on one screen. */
test('one line never assessed and one nothing has run for: the top says what the readiness answer says, 2 things still need evidence, not "1 still needs it"', async () => {
  const { screen } = await readMoment('after_pass', (reads) => {
    twoLines(reads)
    const c = COV(reads)
    c.coverage.criteria[1] = { ...c.coverage.criteria[1], items: [], state: 'none', state_slug: 'coverage_none' }
    c.readiness.ready = false
    c.readiness.headline_slug = 'readiness_needs_evidence'
    c.readiness.missing = ['criterion:1', 'criterion:2']
    c.readiness.rests_on = []
    c.readiness.obligations[1] = neverAssessed(c.readiness.obligations[1])
    c.readiness.obligations[2] = { ...c.readiness.obligations[2], outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked', evidence_name: null }
  })
  assert.equal(screen.model.hq.readiness.sentence, '2 things still need evidence:')
  assert.equal(screen.model.hq.headline.text, 'Accepted: 2 things count as done. 2 things still need evidence.')
})

/* BRIEF-COUNT-AGREES-R2 moved this expected value (D-050, finding 7). It read "1 of 2 things that count as done have evidence. 1 still needs
 * it." — "ready with exceptions keeps the top it had" — beside "Ready for approval with exceptions". The line it counted as one that still
 * needs evidence was the line set aside, which nobody is to check; the top says it was set aside. */
test('a line set aside beside a line that has evidence: ready with exceptions says which was set aside, and that nobody checked it', async () => {
  const { screen } = await readMoment('after_pass', (reads) => {
    twoLines(reads)
    const c = COV(reads)
    c.coverage.criteria[1] = { ...c.coverage.criteria[1], items: [], state: 'none', state_slug: 'coverage_none' }
    c.readiness.with_exceptions = true
    c.readiness.headline_slug = 'readiness_ready_with_exceptions'
    c.readiness.waived = ['criterion:2']
    c.readiness.obligations[2] = { ...c.readiness.obligations[2], outcome: 'waived', outcome_slug: 'waived', evidence_name: null, waiver: { actor: 'sam', authority: 'the accepted profile', rationale: 'Nobody checked this.' } }
  })
  const hq = screen.model.hq
  assert.equal(hq.readiness.ready, true)
  assert.equal(hq.headline.text, '1 of 2 things that count as done have evidence. 1 was set aside, with nobody checking.')
})

/* BRIEF-COUNT-AGREES-R2 moved this expected value too (D-050, finding 10). It read "Accepted: 1 thing counts as done. 1 thing still needs
 * evidence." — the one "thing" was the identity, which is not one of the things that count as done, over a line that HAS evidence. The
 * identity is said on its own, and the line is said to have its evidence. */
test('only the identity is unconfirmed: the top says the line has evidence and says the identity on its own, and the remedy stays the terminal\'s (nothing on the screen writes down what will be looked at)', async () => {
  const { screen } = await readMoment('after_pass', (reads) => {
    const c = COV(reads)
    c.readiness.ready = false
    c.readiness.headline_slug = 'readiness_needs_evidence_one'
    c.readiness.missing = ['candidate_identity']
    c.readiness.rests_on = c.readiness.rests_on
    c.readiness.obligations[0] = { ...c.readiness.obligations[0], outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'inputs_missing', unsatisfied_slug: 'readiness_inputs_missing' }
  })
  const hq = screen.model.hq
  assert.equal(hq.headline.text, 'The 1 thing that counts as done has evidence. Nothing is written down yet about what will be looked at for this Change.')
  assert.equal(hq.readiness.missing[0].reason, R.reasons.readiness_inputs_missing)
  assert.match(hq.readiness.blocker.enabler, /thruline change prepare/)
  assert.match(hq.readiness.blocker.enabler, /terminal/)
})

test('a line that could not be assessed keeps its own screen-appropriate remedy, and the top says what the readiness answer says', async () => {
  const { screen } = await readMoment('after_pass', (reads) => {
    const c = COV(reads)
    c.readiness.ready = false
    c.readiness.headline_slug = 'readiness_needs_evidence_one'
    c.readiness.missing = ['criterion:1']
    c.readiness.rests_on = []
    c.readiness.obligations[1] = { ...c.readiness.obligations[1], outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'could_not_assess', unsatisfied_slug: 'readiness_could_not_assess', evidence_name: null }
  })
  const hq = screen.model.hq
  assert.equal(hq.headline.text, NEW_CONCLUSION)
  assert.equal(hq.readiness.missing[0].reason, R.reasons.readiness_could_not_assess)
  assert.equal(hq.readiness.blocker.enabler, R.blocked_by.readiness_could_not_assess.enabler)
  /* Only a line the engine says was NEVER assessed reads the never-assessed settle sentence. */
  assert.doesNotMatch(hq.inspector.settle.sentence, /never assessed/)
})

test('with no id there is no door to press: the top still says what the readiness answer says, and the remedy is the terminal\'s', async () => {
  const reads = clone(CAPTURE.moments.after_pass)
  NEVER_ASSESSED(reads)
  const change = reads[`/api/changes/${ID}`].body.change
  const served = COV(reads)
  const hq = M.deriveChangeModel({ ...change, id: undefined }, { ...served.coverage, readiness: served.readiness }, null, null, [])
  assert.equal(hq.headline.text, NEW_CONCLUSION)
  assert.equal(hq.readiness.missing[0].reason, TERMINAL_REASON)
  assert.doesNotMatch(hq.inspector.settle.sentence, /Check it again/)
})

test('the same document read twice gives the same words, and the answer moving moves them: Ready brings the old top back', async () => {
  const a = await readMoment('after_pass', NEVER_ASSESSED)
  const b = await readMoment('after_pass', NEVER_ASSESSED)
  assert.equal(a.screen.model.hq.headline.text, b.screen.model.hq.headline.text)
  const ready = await readMoment('after_pass')
  assert.equal(ready.screen.model.hq.headline.text, OLD_CONCLUSION)
  assert.notEqual(ready.screen.model.hq.headline.text, a.screen.model.hq.headline.text)
})
