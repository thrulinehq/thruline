/* W2-E, HQ Next wave 2: Record over time on the real record. The page counts from
 * the statements route (from-projects.js); when that read fails it draws the
 * project documents only, says the statements could not be read, and disables
 * its counting controls. The derivations the counts use are tested here on
 * their own; the counts that need more than a verdict, and Export CSV, are in
 * record-over-time-metrics.test.js. */
import { recordFromProjects, recordFromStatements, tallyKey, dailySeries } from './from-projects.js'
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
export { Logic }
export function render(props, state) {
  const inst = new Logic(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'w2e-rot-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state))

const PROJECTS = [
  { id: 'p1', name: 'shop', threads: 12, claims: 40, sources: ['claude_code'], last_activity: '2026-10-06T21:49:04Z', whose_move: { you: 2 } },
  { id: 'p2', name: 'notes', threads: 3, claims: 2, sources: ['codex'], last_activity: '2026-09-30T10:00:00Z', whose_move: { you: 1 } },
]
const NOW = Date.parse('2026-10-07T09:00:00Z')

test('the live data is the project documents’ typed fields, and no count it was not given', () => {
  // C46 §1.1: the badge is the one count, from the current project's readiness reading (read, nothing ready)
  const READ = (id) => ({ project: id, ready: {}, moves: {}, failed: 0, listFailed: false })
  const d = recordFromProjects({ projects: PROJECTS, storage: { bytes: 2e9 }, now: NOW, reading: READ('p1') })
  assert.deepEqual(d.totals, { claims: 42, threads: 15 })
  // the sidebar badge is the current project's own count (F2: one question, one answer — data/project.js inboxBadge),
  // here the most recently active, never a sum across projects
  assert.equal(d.inbox.yourMove, 2)
  assert.equal(recordFromProjects({ projects: PROJECTS, storage: null, now: NOW, current: 'p2', reading: READ('p2') }).inbox.yourMove, 1)
  assert.equal(recordFromProjects({ projects: PROJECTS, storage: null, now: NOW }).inbox.yourMove, null, 'not read yet: no badge')
  assert.deepEqual(d.sources, ['claude_code', 'codex'])
  assert.equal(d.tallies, null)
  assert.equal(d.daily, null)
  assert.equal(d.medianMinutesToProof, null)
})

test('when the statements cannot be read, the live page says so, counts nothing, and disables what would count', () => {
  const html = text({ data: recordFromProjects({ projects: PROJECTS, storage: { bytes: 2e9 }, now: NOW }) })
  assert.ok(html.includes('The statements could not be read just now.'))
  assert.ok(html.includes('The record holds 42 statements across 15 pieces of work.'))
  assert.ok(html.includes('Reload this page'))
  assert.ok(html.includes('shop') && html.includes('notes'))
  assert.equal((html.match(/disabled=""/g) || []).length, 5, 'three ranges, Projects and Export CSV')
  for (const never of ['Not available in this version', 'Open the Inbox', 'Demo records', 'Codex app server not answering', 'checkout-web', 'notes-cli', '41 of 64', 'hook v0.4']) {
    assert.ok(!html.includes(never), 'live page shows ' + never)
  }
})

test('a statement’s place in the tally is read from its proof, never re-derived', () => {
  assert.equal(tallyKey({ proof: { verdict: 'proven' } }), 'proven')
  assert.equal(tallyKey({ proof: { verdict: 'not_proven', stopsAt: 2 } }), 'noProofFound')
  assert.equal(tallyKey({ proof: { verdict: 'not_proven', stopsAt: 5 } }), 'needsRecheck')
  assert.equal(tallyKey({ proof: { verdict: 'contradicted', recheck: { status: 'failed' } } }), 'rerunFailed')
  assert.equal(tallyKey({ proof: { verdict: 'something_new' } }), null)
  assert.equal(tallyKey({ proof: { verdict: 'not_proven', stopsAt: 5, recheck: { status: 'failed' } } }), 'rerunFailed')
  assert.equal(tallyKey({ proof: { verdict: 'recorded', stopsAt: 2 } }), 'recordedStopsAtSo')
  assert.equal(tallyKey({}), null)
})

test('a day an agent has no recorded session is null, never 0', () => {
  const now = new Date(2026, 9, 7, 12, 0, 0).getTime()
  const day = (n) => new Date(2026, 9, 7 - n, 10, 0, 0).toISOString()
  const d = dailySeries(
    [{ ts: day(0), agent: 'cc', proven: true }, { ts: day(0), agent: 'cc', proven: false }],
    [{ agent: 'cc', from: day(1), to: day(0) }],
    now, 7)
  assert.deepEqual(d.cc.statements, [null, null, null, null, null, 0, 2])
  assert.deepEqual(d.cc.proven, [null, null, null, null, null, 0, 1])
  assert.deepEqual(d.cx.statements, [null, null, null, null, null, null, null])
})

/* ---- the per-day series and the tallies are counted from the same rows ----
 * A real-store shot (All projects, last 30 days) counted 5,475 statements in
 * the cards and said "Too few days with statements to draw" under them: each
 * project's series marked a day "not recorded" when that project had no
 * session that day, and the page reads a day of All as not recorded when any
 * project is. Synthetic fixture: four projects, two agents, and on every day
 * at least one project with no session. */
const FNOW = new Date(2026, 9, 7, 12, 0, 0).getTime()
const fday = (n, h = 10) => new Date(2026, 9, 7 - n, h, 0, 0).toISOString()
const claim = (id, n, agent, verdict, basis) => ({ id, thread_id: 't-' + id, ts: fday(n), agent, kind: 'done', verdict, verdict_basis: basis || null })
const span = (n, source, len = 0) => ({ id: 's' + n + source, source, started_at: fday(n, 9), last_at: fday(n - len, 11) })
const FPROJECTS = [
  { id: 'pa', name: 'proj-a', threads: 3, claims: 7, sources: ['claude_code', 'codex'], last_activity: fday(0) },
  { id: 'pb', name: 'proj-b', threads: 2, claims: 5, sources: ['claude_code', 'codex'], last_activity: fday(1) },
  { id: 'pc', name: 'proj-c', threads: 1, claims: 6, sources: ['claude_code'], last_activity: fday(5) },
  { id: 'pd', name: 'proj-d', threads: 0, claims: 0, sources: [], last_activity: null },
]
const FSTATEMENTS = {
  pa: {
    claims: [claim('a1', 0, 'claude_code', 'proven'), claim('a2', 0, 'claude_code', 'not_proven', 'no_match'), claim('a3', 2, 'claude_code', 'recorded'),
      claim('a4', 4, 'claude_code', 'contradicted'), claim('a5', 10, 'claude_code', 'proven'), claim('a6', 40, 'claude_code', 'not_proven', 'edited_since_run'),
      claim('a7', 1, 'codex', 'proven')],
    sessions: [span(0, 'claude_code'), span(2, 'claude_code'), span(4, 'claude_code'), span(10, 'claude_code'), span(40, 'claude_code'), span(1, 'codex')],
  },
  pb: {
    claims: [claim('b1', 1, 'codex', 'recorded'), claim('b2', 3, 'codex', 'proven'), claim('b3', 20, 'codex', 'not_proven', 'no_match'),
      claim('b4', 80, 'codex', 'proven'), claim('b5', 2, 'claude_code', 'proven')],
    sessions: [span(1, 'codex'), span(3, 'codex', 1), span(20, 'codex'), span(80, 'codex'), span(2, 'claude_code')],
  },
  pc: {
    claims: [claim('c1', 5, 'claude_code', 'recorded'), claim('c2', 60, 'claude_code', 'proven'),
      // rows no count may use: a verdict the tally does not place, a time that does not parse, an unknown agent, a future time
      claim('c3', 6, 'claude_code', 'something_new'), Object.assign(claim('c4', 6, 'claude_code', 'proven'), { ts: 'not a time' }),
      claim('c5', 6, 'other_agent', 'proven'), claim('c6', -2, 'claude_code', 'proven')],
    sessions: [span(5, 'claude_code'), span(60, 'claude_code')],
  },
  pd: { claims: [], sessions: [] },
}
const AG = { claude_code: 'cc', codex: 'cx' }
/** The rows, read straight from the fixture: what each count must be made of. */
const fixtureRows = () => Object.entries(FSTATEMENTS).flatMap(([pid, doc]) => doc.claims
  .filter((c) => AG[c.agent] && ['proven', 'contradicted', 'recorded', 'not_proven'].includes(c.verdict) && !Number.isNaN(Date.parse(c.ts)))
  .map((c) => ({ pid, a: AG[c.agent], ago: Math.round((new Date(FNOW).setHours(0, 0, 0, 0) - new Date(c.ts).setHours(0, 0, 0, 0)) / 86400000) }))
  .filter((r) => r.ago >= 0))
const fdata = () => recordFromStatements({ projects: FPROJECTS, statements: FSTATEMENTS, storage: { bytes: 1e9 }, now: FNOW })

test('live Capture shows Try again for an unobserved Codex source and rechecks the selected project', () => {
  const health = { codex: { configured: false, state: null } }
  for (const data of [recordFromProjects({ projects: PROJECTS, storage: null, now: NOW }), fdata()]) {
    let reads = 0
    const props = { data, captureHealth: health, retryCapture: () => { reads += 1 } }
    const inst = new M.Logic(props)
    assert.match(text(props), /<section aria-label="Capture"[\s\S]*Try again/)
    const before = inst.renderVals().codex
    assert.equal(before.down, true)
    assert.equal(before.notTried, true)
    before.retry()
    assert.equal(reads, 1, 'Try again re-reads the project capture health')
    const after = inst.renderVals().codex
    assert.equal(after.tried, true)
    assert.match(after.triedText, /^Tried again at \d\d:\d\d\. Codex capture is still off/)
  }
  assert.doesNotMatch(text({ data: fdata(), captureHealth: { codex: { configured: true, state: 'watching', watchesThisProject: true } } }), /Try again/)
  assert.doesNotMatch(text({ data: fdata(), captureHealth: { codex: { configured: true, state: 'watching', watchesThisProject: null } } }), /Try again/, 'a fact the route could not read is not read as a fault')
})

/* C54 #5 review gaps: the card says what is true of THIS project, and a read that failed is said as one */
const capture = (codex) => ({ data: fdata(), captureHealth: { codex } })
const card = (props, state) => text(props, state).match(/<section aria-label="Capture"[\s\S]*?<\/section>/)[0]

test('a Codex observer that watches another folder is not drawn as healthy for this one', () => {
  const html = card(capture({ configured: true, state: 'watching', paused: false, watchesThisProject: false }))
  assert.match(html, /Codex is watching a different folder/)
  assert.match(html, /Try again/)
  assert.doesNotMatch(html, /app server not answering|capture is off/, 'it is neither of the other two faults')
  const props = capture({ configured: true, state: 'watching', watchesThisProject: false })
  const inst = new M.Logic(props)
  inst.renderVals().codex.retry()
  assert.match(inst.renderVals().codex.triedText, /^Tried again at \d\d:\d\d\. Codex is still watching a different folder/)
  // an observer that is not answering is that fault first, whichever folder it was watching
  assert.match(card(capture({ configured: true, state: 'disconnected', watchesThisProject: false })), /Codex app server not answering/)
})

test('capture that is off is said as off, and an observer that is not answering is said as not answering', () => {
  const off = card(capture({ configured: false, state: null, paused: null, watchesThisProject: null, reason: 'HQ was not started with --observe-codex' }))
  assert.match(off, /Codex capture is off/)
  assert.doesNotMatch(off, /app server not answering/)
  for (const state of ['disconnected', 'stopped']) {
    const down = card(capture({ configured: true, state, watchesThisProject: null }))
    assert.match(down, /Codex app server not answering/)
    assert.doesNotMatch(down, /capture is off/)
  }
})

test('a capture read that failed is said, with Try again, and is not a vanished card', () => {
  const error = 'ThruLine’s local server did not answer. Is thruline ui still running?'
  let reads = 0
  const props = { data: fdata(), captureHealth: null, captureError: error, retryCapture: () => { reads += 1 } }
  const html = card(props)
  assert.match(html, /Capture could not be checked just now/)
  assert.ok(html.includes(error), 'the reason the read gave is said')
  assert.match(html, /Try again/)
  assert.doesNotMatch(html, /app server not answering|capture is off|different folder/, 'a read that failed claims nothing about Codex')
  const inst = new M.Logic(props)
  inst.renderVals().codex.retry()
  assert.equal(reads, 1)
  assert.match(inst.renderVals().codex.triedText, /^Tried again at \d\d:\d\d\. Capture still could not be checked/)
  // nothing read yet and nothing failed: the card is not drawn, because nothing is known to say
  assert.doesNotMatch(text({ data: fdata(), captureHealth: null, captureError: null }), /<section aria-label="Capture"[\s\S]*Try again/)
})

test('Try again says only that it is checking until the new read returns, and "Still …" comes only from that read', () => {
  const cases = [
    ['an observer that is not answering', { data: fdata(), captureHealth: { codex: { configured: true, state: 'disconnected' } }, retryCapture: () => {} }, /Still not answering/],
    ['capture that is off', { data: fdata(), captureHealth: { codex: { configured: false } }, retryCapture: () => {} }, /Codex capture is still off/],
    ['a read that failed', { data: fdata(), captureHealth: null, captureError: 'ThruLine’s local server did not answer.', retryCapture: () => {} }, /Capture still could not be checked/],
  ]
  for (const [what, props, still] of cases) {
    const inst = new M.Logic(props)
    inst.renderVals().codex.retry()
    // the re-read is out: the answer on screen is the one before it, so no result is stated yet
    inst.props = { ...props, captureReading: true, ...(props.captureError ? { captureError: null } : {}) }
    const checking = inst.renderVals().codex
    assert.equal(checking.down, true, what + ': the card stays while it checks')
    assert.equal(checking.tried, true)
    assert.equal(checking.triedText, 'Checking…', what)
    const drawn = card(inst.props, { codexTried: true, codexWas: inst.state.codexWas })
    assert.match(drawn, /Checking…/, what)
    assert.doesNotMatch(drawn, /Still|still|Tried again/, what)
    // the read has returned and the fault stands: now, and only now, it says so
    inst.props = props
    assert.match(inst.renderVals().codex.triedText, /^Tried again at \d\d:\d\d\. /, what)
    assert.match(inst.renderVals().codex.triedText, still, what)
  }
})

test('the screen hands the card the capture read’s facts, its error, and the read to repeat', () => {
  const src = readFileSync(join(here, 'index.jsx'), 'utf8')
  assert.match(src, /useCapture\(currentProjectId\(list, at && at\.project\)\)/, 'the folder in front of the person')
  assert.match(src, /captureHealth: capture\.facts, captureError: capture\.error, captureReading: capture\.reading, retryCapture: capture\.reload/)
})
const tail = (arr, w) => (arr || []).slice(-w).reduce((n, x) => n + (x || 0), 0)

test('the per-day series sums to the windowed tally for every project, every agent and All, at 7, 30 and 90 days', () => {
  const d = fdata()
  for (const w of [7, 30, 90]) {
    let allTally = 0, allSeries = 0
    for (const pid of ['pa', 'pb', 'pc', 'pd']) {
      for (const a of ['cc', 'cx']) {
        const t = ((d.tallies[w][pid] || {})[a] || {}).statements || 0
        const s = tail(((d.daily[pid] || {})[a] || {}).statements, w)
        const rows = fixtureRows().filter((r) => r.pid === pid && r.a === a && r.ago < w).length
        assert.equal(t, rows, `tally ${w}d ${pid} ${a}`)
        assert.equal(s, t, `series ${w}d ${pid} ${a}`)
        allTally += t; allSeries += s
      }
    }
    assert.equal(allSeries, allTally, `All ${w}d`)
    assert.equal(allTally, fixtureRows().filter((r) => r.ago < w).length)
  }
})

test('a day is "not recorded" only when the agent recorded nothing in any project that day', () => {
  const d = fdata()
  // day 1 (index 88): Claude Code recorded nowhere; Codex recorded in pa and pb
  assert.equal(d.daily.pa.cc.statements[88], null)
  assert.equal(d.daily.pa.cx.statements[88], 1)
  // day 0: Claude Code recorded in pa only; pb and pc did no work that day, which is a 0, not a missing day
  assert.equal(d.daily.pa.cc.statements[89], 2)
  assert.equal(d.daily.pb.cc.statements[89], 0)
  assert.equal(d.daily.pc.cc.statements[89], 0)
  // proj-d has no row and no session: absent, never a row of zeros
  assert.equal(d.daily.pd, undefined)
  // proj-c never recorded Codex: absent
  assert.equal(d.daily.pc.cx, undefined)
})

const TOO_FEW = /Too few days with statements to draw\.|One statement, on one day\./
test('"too few days" is said only when the rows in view really fall on fewer than two days', () => {
  const d = fdata()
  for (const range of ['7', '30', '90']) {
    for (const agent of ['all', 'cc', 'cx']) {
      for (const project of ['all', 'pa', 'pb', 'pc']) {
        const days = new Set(fixtureRows().filter((r) => r.ago < +range && (project === 'all' || r.pid === project) && (agent === 'all' || r.a === agent)).map((r) => r.ago))
        const html = text({ data: d }, { range, agent, project })
        const said = TOO_FEW.test(html)
        // with no rows at all the page draws its empty state instead; with one day or more it is exact
        if (days.size === 0) assert.ok(!said || !html.includes('statements per day'), `${range}d ${agent} ${project}: empty`)
        else assert.equal(said, days.size < 2, `${range}d ${agent} ${project}: ${days.size} days, said too few: ${said}`)
      }
    }
  }
  assert.ok(!TOO_FEW.test(text({ data: d }, { range: '30', agent: 'all', project: 'all' })), 'the shot that contradicted itself')
})

test('rows that really fall on one day are said as too few days to draw', () => {
  const one = {
    pa: { claims: [claim('x1', 3, 'claude_code', 'proven'), claim('x2', 3, 'claude_code', 'recorded')], sessions: [span(3, 'claude_code'), span(1, 'claude_code')] },
    pb: { claims: [claim('y1', 3, 'codex', 'proven')], sessions: [span(3, 'codex'), span(0, 'codex')] },
  }
  const d = recordFromStatements({ projects: FPROJECTS.slice(0, 2), statements: one, storage: null, now: FNOW })
  const html = text({ data: d }, { range: '7', agent: 'all', project: 'all' })
  assert.ok(html.includes('Too few days with statements to draw.'))
  assert.equal(tail(d.daily.pa.cc.statements, 7) + tail(d.daily.pb.cx.statements, 7), 3)
})
