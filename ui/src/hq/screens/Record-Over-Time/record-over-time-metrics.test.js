/* C30 Record over time: the counts that need more than a verdict (minutes from a
 * statement to its proof, proofs moved by a later edit, statements made before
 * any run) and Export CSV, counted from the statements route's rows only.
 * Synthetic data throughout: made-up projects, claims and times. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { recordFromStatements } from './from-projects.js'
import {
  CSV_COLUMNS, beforeAnyRun, csvField, csvFilename, median, minutesText, minutesToProof, movedProofs, recordCsv, safeName, sliceMetrics, sliceRows,
} from './metrics.js'

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
export function make(props, state) {
  const inst = new Logic(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return inst
}
export function html(inst) {
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'rot-metrics-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const page = (props, state) => unescape(M.html(M.make(props, state)))

/* ---- the made-up record ---- */
const NOW = new Date(2026, 9, 7, 12, 0).getTime()
const t = (ago, h, min = 0) => new Date(2026, 9, 7 - ago, h, min).toISOString()
const CC = 'claude_code'
const CX = 'codex'
const mk = (id, agent, verdict, basis, over) => ({
  id, thread_id: 't-' + id, session_id: 's-' + id, agent, kind: verdict === 'recorded' ? 'done' : 'tests_pass', has_verifier: verdict !== 'recorded',
  verdict, verdict_basis: basis, proven_at: null, run_before: true, proof_moved: false, ...over,
})
const PROJECTS = [
  { id: 'pa', name: 'shop', threads: 5, claims: 9, sources: [CC], last_activity: t(0, 11) },
  { id: 'pb', name: 'notes', threads: 2, claims: 3, sources: [CX], last_activity: t(0, 10) },
]
const SHOP = [
  mk('c1', CC, 'proven', 'settled_by_later_run', { ts: t(0, 10), proven_at: t(0, 10, 12), proof_moved: true }), // waited 12, proved again
  mk('c2', CC, 'proven', 'settled_by_later_run', { ts: t(0, 11), proven_at: t(0, 11, 30), run_before: false }), // waited 30, said before any run
  mk('c3', CC, 'proven', 'run_holds', { ts: t(1, 9), proven_at: t(1, 8, 55) }), // backed by a run made before it
  mk('c4', CC, 'not_proven', 'edited_since_run', { ts: t(1, 10), proof_moved: true }), // open
  mk('c5', CC, 'not_proven', 'no_run_before', { ts: t(2, 10), run_before: false }), // nothing to rest on
  mk('c6', CC, 'contradicted', 'later_run_failed', { ts: t(2, 11), proof_moved: true }), // moved, and the re-run failed
  mk('c7', CC, 'contradicted', 'later_run_failed', { ts: t(3, 10), run_before: false }), // said before any run, a later run failed
  mk('c8', CC, 'recorded', 'no_verifier', { ts: t(3, 11), run_before: false }), // a "done" claim: no run can settle it
  mk('e1', CC, 'proven', 'settled_by_later_run', { ts: t(10, 10), proven_at: t(10, 11, 40) }), // waited 100, 10 days ago
]
const NOTES = [
  mk('d1', CX, 'proven', 'settled_by_later_run', { ts: t(0, 10), proven_at: t(0, 10, 3) }), // waited 3
  mk('d2', CX, 'proven', 'settled_by_later_run', { ts: t(5, 10), proven_at: t(5, 11) }), // waited 60
  mk('d3', CX, 'not_proven', 'edited_since_run', { ts: t(40, 10), proof_moved: true }), // 40 days ago
]
const data = (shop = SHOP, notes = NOTES) =>
  recordFromStatements({ projects: PROJECTS, statements: { pa: { claims: shop, sessions: [] }, pb: { claims: notes, sessions: [] } }, storage: { bytes: 1e9 }, now: NOW })
/* the same record as an older server sent it: no proven_at, run_before or proof_moved */
const strip = (cs) => cs.map(({ proven_at, run_before, proof_moved, ...rest }) => rest) // eslint-disable-line no-unused-vars
const D = data()
const S7 = { range: 7, agent: 'all', project: 'all' }

test('minutes to proof is the median over the statements that waited for a later run, and says who is left out', () => {
  const w7 = minutesToProof(sliceRows(D.rows, S7))
  // c1 12, c2 30, d1 3, d2 60 waited; c3 was backed before it was said
  assert.deepEqual(w7, { served: true, proven: 5, waited: 4, already: 1, unmeasured: 0, minutes: 21 })
  // 30 days adds e1 (100): the median moves to the middle of five
  assert.equal(minutesToProof(sliceRows(D.rows, { ...S7, range: 30 })).minutes, 30)
  // one agent, one project
  assert.equal(minutesToProof(sliceRows(D.rows, { ...S7, agent: 'cc' })).minutes, 21)
  assert.equal(minutesToProof(sliceRows(D.rows, { ...S7, agent: 'cx' })).minutes, 31.5)
  assert.equal(minutesToProof(sliceRows(D.rows, { ...S7, project: 'pb' })).minutes, 31.5)
  // 90 days reaches d3 (40 days ago), which is not proven and changes nothing
  assert.equal(minutesToProof(sliceRows(D.rows, { ...S7, range: 90 })).minutes, 30)
})

test('a statement the record cannot time is counted apart, never guessed, and a missing field is not a number', () => {
  const T = '2026-10-07T10:00:00.000Z'
  const row = (over) => ({ c: { verdict: 'proven', ts: T, proven_at: '2026-10-07T10:10:00.000Z', ...over } })
  const r = minutesToProof([row(), row({ ts: 'not a time' }), row({ proven_at: null }), row({ proven_at: T })])
  assert.deepEqual(r, { served: true, proven: 4, waited: 1, already: 1, unmeasured: 2, minutes: 10 })
  // nothing waited: no median, and no 0
  assert.equal(minutesToProof([row({ proven_at: T })]).minutes, null)
  // nothing proven: no median
  assert.deepEqual(minutesToProof([{ c: { verdict: 'not_proven' } }]), { served: true, proven: 0, waited: 0, already: 0, unmeasured: 0, minutes: null })
  // a server that sends no proven_at: not served, whatever the other rows say
  const old = minutesToProof([row({ proven_at: undefined })])
  assert.equal(old.served, false)
  assert.equal(old.minutes, null)
})

test('how minutes are written, in the page and in the file alike', () => {
  assert.deepEqual([0.4, 4, 4.5, 9.96, 10.5, 31.5, 1440].map(minutesText), ['0.4', '4', '4.5', '10', '11', '32', '1440'])
  assert.equal(minutesText(null), '')
  assert.equal(median([]), null)
  assert.equal(median([5]), 5)
  assert.equal(median([9, 1, 5, 3]), 4)
})

test('a proof moved by a later edit: open + proven again + failed add up, and open is the headline’s "need a re-check"', () => {
  const m = movedProofs(sliceRows(D.rows, S7))
  assert.deepEqual(m, { served: true, total: 3, open: 1, reproven: 1, failed: 1 })
  assert.equal(m.open + m.reproven + m.failed, m.total)
  // for every selection the page offers, "open" is the same number the tally counts as needing a re-check
  for (const range of [7, 30, 90]) {
    for (const agent of ['all', 'cc', 'cx']) {
      for (const project of ['all', 'pa', 'pb']) {
        const rows = sliceRows(D.rows, { range, agent, project })
        const mv = movedProofs(rows)
        assert.equal(mv.open, rows.filter((r) => r.key === 'needsRecheck').length, `${range}d ${agent} ${project}`)
        assert.equal(mv.open + mv.reproven + mv.failed, mv.total)
      }
    }
  }
})

test('claims made before any run: only a statement a run could settle, and the three outcomes add up', () => {
  const b = beforeAnyRun(sliceRows(D.rows, S7))
  // c2 (proven later), c5 (nothing), c7 (a later run failed); c8 is a "done" claim and no run could settle it
  assert.deepEqual(b, { served: true, total: 3, laterProven: 1, failedAfter: 1, stillNothing: 1 })
  assert.equal(b.laterProven + b.failedAfter + b.stillNothing, b.total)
  // a claim in no piece of work is not "before any run": there is no run to be before
  const lone = [{ c: { has_verifier: true, run_before: false, verdict: 'not_proven', verdict_basis: 'not_in_a_thread' } }]
  assert.equal(beforeAnyRun(lone).total, 0)
})

test('what an older server does not send is not counted, in every metric', () => {
  const d = data(strip(SHOP), strip(NOTES))
  const m = sliceMetrics(sliceRows(d.rows, S7))
  assert.equal(m.median.served, false)
  assert.equal(m.moved.served, false)
  assert.equal(m.before.served, false)
  const html = page({ data: d }, { range: '7' })
  assert.ok(html.includes('Not counted: proof times not recorded'))
  assert.ok(html.includes('Earlier re-checks are not counted'))
  assert.ok(html.includes('This record does not say whether a run came before each statement.'))
  // the verdict counts the old server does give are still there
  assert.ok(html.includes('5 of 10 statements are proven'))
  // and the file leaves those cells empty rather than 0
  const { text } = recordCsv({ rows: d.rows, projects: d.projects, range: 7, agent: 'all', project: 'all', through: d.today })
  assert.equal(text.split('\r\n')[1], 'shop,Claude Code,7,2026-10-07,8,3,1,1,2,1,,,,')
})

test('the page writes the counts it can make, with how it made them', () => {
  const html = page({ data: D }, { range: '7' })
  // the median card: the number, its unit, who it is made of, and the derivation as its tooltip
  assert.match(html, /<span[^>]*>21<\/span> <span[^>]*>min<\/span>/)
  assert.ok(html.includes('across the 4 proven by a later run'))
  assert.ok(html.includes('Median of the minutes from each statement to the recorded start of the run that proves it'))
  assert.ok(html.includes('1 more was already backed by a run made before it was said, so it waited for nothing and is not in this number.'))
  // the re-check card asks the design's question, and its parts add up to its number
  const k = M.make({ data: D }, { range: '7' }).renderVals().k
  assert.deepEqual([k.re, k.med, k.medUnit], ['3', '21', 'min'])
  assert.ok(html.includes('Needed a re-check after a later edit'))
  assert.ok(html.includes('1 open · 1 re-proven · 1 failed'))
  assert.ok(html.includes('1 re-proven after a later edit'))
  assert.ok(html.includes('Needed a re-check after a later edit: 3 of 10. 1 still need one, 1 were re-proven, 1 failed when re-run.'))
  // claims made before any run
  assert.ok(html.includes('1 proven by a run made afterwards'))
  assert.ok(html.includes('1 still has nothing to rest on · 1 had a run afterwards that failed'))
  // the re-runs card says what it cannot count, and why, and shows no number for it
  assert.ok(html.includes('Checks you start from a Change are kept with that Change, and this page does not read them across Changes.'))
  // each agent’s row carries its own median
  assert.ok(html.includes('21 min') && html.includes('32 min'))
  for (const never of ['Not available in this version', 'in this version', 'Demo records']) assert.ok(!html.includes(never), never)
})

test('30 days moves the median; one agent shows its own', () => {
  assert.match(page({ data: D }, { range: '30' }), /<span[^>]*>30<\/span> <span[^>]*>min<\/span>/)
  assert.ok(page({ data: D }, { range: '30', agent: 'cx' }).includes('across the 2 proven by a later run'))
  // nothing waited in this selection: no number, and the reason, never 0
  const none = data([mk('x', CC, 'proven', 'run_holds', { ts: t(0, 10), proven_at: t(0, 9) })], [])
  const html = page({ data: none }, { range: '7' })
  assert.ok(html.includes('all 1 proven were already backed'))
  assert.ok(!/<span[^>]*>0<\/span> <span[^>]*>min<\/span>/.test(html))
})

/* ---- Export CSV ---- */
const HEADER = CSV_COLUMNS.join(',')

test('the file is exactly the rows of the selection, byte for byte', () => {
  const f = recordCsv({ rows: D.rows, projects: D.projects, range: 7, agent: 'all', project: 'all', through: D.today })
  assert.equal(f.filename, 'thruline-record-all-projects-7d.csv')
  assert.equal(f.count, 2)
  assert.equal(
    f.text,
    HEADER + '\r\n'
      + 'shop,Claude Code,7,2026-10-07,8,3,1,1,2,1,3,1,3,21\r\n'
      + 'notes,Codex,7,2026-10-07,2,2,0,0,0,0,0,0,0,32\r\n',
  )
  assert.equal(HEADER, 'project,agent,range_days,counted_through,statements,proven,need_a_recheck,nothing_found,a_run_failed,recorded_settles_nothing,proof_moved_by_a_later_edit,reproven_after_a_later_edit,said_before_any_run,median_minutes_to_proof')
  // one project, one agent: its name in the file name
  const one = recordCsv({ rows: D.rows, projects: D.projects, range: 30, agent: 'cc', project: 'pa', through: D.today })
  assert.equal(one.filename, 'thruline-record-shop-30d-claude-code.csv')
  assert.equal(one.text, HEADER + '\r\n' + 'shop,Claude Code,30,2026-10-07,9,4,1,1,2,1,3,1,3,30\r\n')
})

test('the counts in a row add up, and a row exists only where a statement was made', () => {
  for (const range of [7, 30, 90]) {
    const f = recordCsv({ rows: D.rows, projects: D.projects, range, agent: 'all', project: 'all', through: D.today })
    const lines = f.text.trim().split('\r\n').slice(1).map((l) => l.split(','))
    assert.equal(lines.length, f.count)
    for (const c of lines) assert.equal(+c[4], +c[5] + +c[6] + +c[7] + +c[8] + +c[9], c.join(','))
    assert.equal(lines.reduce((n, c) => n + +c[4], 0), sliceRows(D.rows, { range, agent: 'all', project: 'all' }).length)
  }
  // Codex made nothing in shop, and Claude Code nothing in notes: no row, not a row of zeros
  const f = recordCsv({ rows: D.rows, projects: D.projects, range: 90, agent: 'all', project: 'all', through: D.today })
  assert.equal(f.text.trim().split('\r\n').length, 1 + 2, 'shop/Claude Code and notes/Codex, and nothing else')
})

test('RFC 4180: fields with a comma, a quote or a line break are quoted; no folder path is written', () => {
  assert.equal(csvField('a,b'), '"a,b"')
  assert.equal(csvField('say "hi"'), '"say ""hi"""')
  assert.equal(csvField('two\nlines'), '"two\nlines"')
  assert.equal(csvField(null), '')
  assert.equal(csvField(5), '5')
  assert.equal(safeName('/Users/someone/work/app'), 'app')
  assert.equal(safeName('~'), 'Home folder')
  assert.equal(safeName('~/proj'), 'proj')
  assert.equal(safeName('C:\\Users\\someone\\app'), 'app')
  assert.equal(safeName('  '), 'Unnamed project')
  assert.equal(csvFilename({ project: 'My App, v2', range: 30, agent: 'cx' }), 'thruline-record-my-app-v2-30d-codex.csv')
  const odd = [{ id: 'pa', name: '/Users/someone/work/"big, app"' }]
  const f = recordCsv({ rows: D.rows, projects: odd, range: 7, agent: 'all', project: 'all', through: D.today })
  assert.ok(!f.text.includes('/Users'), 'no home-folder path')
  assert.ok(f.text.split('\r\n')[1].startsWith('"""big, app""",Claude Code,7'))
})

test('Export CSV saves the file, and the toast names it and counts its rows', () => {
  const saved = []
  const inst = M.make({ data: D, save: (f) => saved.push(f) }, { range: '7' })
  const v = inst.renderVals()
  assert.equal(v.hdr.expOff, false)
  assert.equal(v.toast.show, false)
  v.hdr.exportCsv()
  assert.equal(saved.length, 1)
  assert.equal(saved[0].filename, 'thruline-record-all-projects-7d.csv')
  assert.equal(saved[0].mime, 'text/csv;charset=utf-8')
  assert.equal(saved[0].text, recordCsv({ rows: D.rows, projects: D.projects, range: 7, agent: 'all', project: 'all', through: D.today }).text)
  const html = unescape(M.html(inst))
  assert.ok(html.includes('Saved thruline-record-all-projects-7d.csv'))
  assert.ok(html.includes('2 rows, one per project and agent'))
  // the file name is written whole, not cut at the card's edge
  assert.equal(inst.renderVals().toast.wrap, true)
  assert.ok(html.includes('white-space:normal;overflow-wrap:anywhere'))
  assert.ok(!html.includes('Not available in this version'))
  // choosing a project changes the file; the control never says it is unavailable
  inst.setState({ project: 'pb', toast: false })
  inst.renderVals().hdr.exportCsv()
  assert.equal(saved[1].filename, 'thruline-record-notes-7d.csv')
  assert.ok(unescape(M.html(inst)).includes('1 row, one per project and agent'))
})

test('with nothing to count, Export CSV is off, says why, and saves nothing', () => {
  const saved = []
  const inst = M.make({ data: D, save: (f) => saved.push(f) }, { range: '7', agent: 'cc', project: 'pb' })
  const v = inst.renderVals()
  assert.equal(v.hdr.expOff, true)
  assert.equal(v.hdr.expWhy, 'Nothing to save: no statements in this selection.')
  v.hdr.exportCsv()
  assert.deepEqual(saved, [])
  const html = unescape(M.html(inst))
  const i = html.indexOf('Export CSV</button>')
  const tag = html.slice(html.lastIndexOf('<button', i), i)
  assert.match(tag, /disabled=""/)
  assert.match(tag, /title="Nothing to save: no statements in this selection\."/)
})

test('where the proof stops: a link with no page behind it is not drawn; the Inbox one is', () => {
  // c8 is a "done" claim: it stops at ⑥ SO, whose action would list statements across pieces of work
  const six = M.make({ data: D }, { range: '7', stop: 6 }).renderVals().st
  assert.equal(six.actShow, false)
  assert.equal(six.actHint, 'Each of these is in its own piece of work. This page has no list of them across pieces of work.')
  // nothing found stops at ② RESTS ON, whose action opens them in the Inbox
  const two = M.make({ data: D }, { range: '7', stop: 2 }).renderVals().st
  assert.equal(two.actShow, true)
  assert.equal(two.actHref, 'Inbox.dc.html')
  // the gallery keeps the canvas’s own link
  assert.equal(M.make({}, { stop: 6 }).renderVals().st.actHref, 'Every-Statement.dc.html')
})
