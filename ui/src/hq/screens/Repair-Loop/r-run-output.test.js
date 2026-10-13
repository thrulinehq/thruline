/* C30 R-RUN, finished: while a check runs, the Brief's live-output box and
 * the check sheet's running form draw what the run sheet serves of what the
 * check has printed so far, followed every second through the one run client
 * (data/run.js), and the elapsed time the sheet serves from the run's own
 * recorded start. Where the sheet serves no output, none is drawn and the
 * screen says so; where it has printed nothing yet, the screen says that.
 * pytest runs get the pass and fail forms with pytest's own counts.
 *
 *  1. liveOutputOf: this Change's running check only, and nothing that was
 *     not served.
 *  2. The Brief: the newest served lines under the command, growing as the
 *     sheet is followed; the served elapsed time.
 *  3. The check sheet: the newest served lines, and when there are more.
 *  4. pytest: the pass and fail forms with the record's counts.
 *
 * Every fetch is stubbed; nothing here reaches a server or a store. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))

async function bundle(ENTRY, tag) {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'r-run-output-' + tag + '-')), 'm.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}

const M = await bundle(`
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import RLTemplate from './template.jsx'
import RLLogic from './logic.js'
import BTemplate from '../Change-Brief/template.jsx'
import BLogic from '../Change-Brief/logic.js'
export * as CHANGE from '../../data/change.js'
export * as RUN from '../../data/run.js'
export { selectRepairLoop } from './from-change.js'
export * as FROM from './from-change.js'
const SCREENS = { rl: [RLLogic, RLTemplate], brief: [BLogic, BTemplate] }
export function vals(name, props, state) {
  const inst = new SCREENS[name][0](props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return inst.renderVals()
}
export function render(name, props, state) {
  return renderToStaticMarkup(React.createElement(SCREENS[name][1], { v: vals(name, props, state), link: (h) => h }))
}`, 'all')

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
/** What a person can read: the text, and every title and aria-label. */
const text = (name, props, state) => {
  const h = unescape(M.render(name, props, state))
  const attrs = (h.match(/(?:aria-label|title)="[^"]*"/g) || []).map((a) => a.replace(/^[^"]*"|"$/g, '')).join('\n')
  return h.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ') + '\n' + attrs
}
const HOLES = /\b(undefined|NaN)\b|step null/

/* A served ThreadView: a passing run on an older version, an edit at 12:02, then "Tests pass." */
const T = (m, s = 0) => new Date(Date.UTC(2026, 9, 6, 12, m, s)).toISOString()
function thread() {
  const cur = 'wt-2222222222222222'
  return {
    schema_version: 1,
    project: { id: 'p1', name: 'ledger-api', root: '/r/ledger-api' },
    thread: { id: '0123456789abcdef', title: 'Stop the import double-posting', current_version_id: cur, whose_move: 'you', proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: ['/r/ledger-api/src/post.py'] } },
    prompts: [{ event_id: 'ev-p', session_id: 's1', seq: 1, ts: T(0), text: 'the import posts twice', work: true, version_id: 'wt-1111111111111111', raw_file: '/m', raw_line: 1 }],
    edits: [{ id: 'ed0', event_id: 'ed0', session_id: 's1', ts: T(2), file: '/r/ledger-api/src/post.py', op: 'edit', lines_added: 1, lines_removed: 0, ok: true, version_id: cur, raw_file: '/m', raw_line: 3, hunks: [] }],
    commands: [{ id: 'c0#0', event_id: 'c0', part: 0, session_id: 's1', ts: T(1), command: 'pytest -q tests/test_api.py', outcome: 'succeeded', exit_code: 0, is_test: true, is_commit: false, tally: { passed: 4, failed: 0, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 }, version_id: 'wt-1111111111111111', raw_file: '/m', raw_line: 2 }],
    claims: [{ id: 'cl0#0', event_id: 'cl0', session_id: 's1', ts: T(3), text: 'Import fixed. Tests pass.', kind: 'tests_pass', has_verifier: true, rests_on_command_id: 'c0#0', followed_by_passing_test: false, version_id: cur, raw_file: '/m', raw_line: 4, verdict: 'not_proven', verdict_basis: 'edited_since_run' }],
    versions: [
      { id: 'wt-1111111111111111', commit: null, short: null, session_id: 's1', from_seq: 1, to_seq: 2, committed: true },
      { id: cur, commit: null, short: null, session_id: 's1', from_seq: 3, to_seq: 9, committed: false },
    ],
    current_version_id: cur,
    sessions: [{ id: 's1', ledger_project_id: 'lp', source: 'claude_code', origin: 'person', started_at: T(0), last_at: T(5), file: '/m' }],
    files: [],
    masking: { masked: 0, families: [], complete: true },
  }
}

/* The served run sheet (`GET /api/changes/<id>/run`, run_route.rs `view`, schema 3). */
const SHEET = (o = {}) => ({
  schema_version: 3, change: 'chg_1', project_folder: '~/ledger-api',
  permission: { granted_by: 'sam', granted_at: T(5) },
  consent: { state: 'given', history: [], file: '.thruline/run-permission.json' },
  checks: [{ id: 'cargo-test', program: 'cargo', category: 'tests', command: 'cargo test', last: null }],
  not_runnable_here: [], environments: [], remembered_environment: null, job: null, not_started: null, actor: 'sam',
  ...o,
})
const MASKING = { masked: 0, families: [], complete: true }
/** A check of this Change running, as a schema-3 sheet serves it; `output` undefined is a sheet that serves none. */
const RUNNING = ({ lines, cut = false, elapsed, phase = 'running', forThis = true } = {}) => SHEET({
  job: {
    run: 'hq-check-01', check: 'cargo-test', for_this_change: forThis, phase, environment: 'hq', started_at: T(10),
    ...(elapsed === undefined ? {} : { recorded_started_at: T(10, 1), elapsed_s: elapsed }),
    ...(lines === undefined ? {} : { output: { lines, cut, masking: MASKING } }),
  },
})
const LAST = (last, check = {}) => SHEET({ checks: [{ id: 'cargo-test', program: 'cargo', category: 'tests', command: 'cargo test', ...check, last: { run: 'hq-check-01', state: 'completed', why: null, reason: null, signal: null, suites: 1, failure_line: null, fetch_skipped: null, survivors: null, settles: true, started_at: T(10), recorded_at: T(11, 30), cleanup_verified: null, elapsed_s: 90, ...last } }] })
const NOW = new Date(Date.UTC(2026, 9, 6, 12, 10, 42))

const tracked = (sheet, now = NOW) => M.CHANGE.changeSnapshot(thread(), { change: { id: 'chg_1' }, hq: null, proposals: null, runSheet: sheet }, now)
const rl = (sheet, now = NOW) => M.selectRepairLoop(tracked(sheet, now), { id: 'chg_1', runSheet: sheet }, { now })
const brief = (sheet) => text('brief', { data: tracked(sheet), actions: { check: () => {}, stopCheck: () => {} } })
const logLines = (sheet) => M.vals('rl', { data: rl(sheet) }).run.lines.map((l) => l.text)

function stub(answers) {
  const calls = []
  const f = async (url, init = {}) => {
    calls.push({ url, method: init.method || 'GET' })
    const queue = answers[url]
    const a = Array.isArray(queue) ? (queue.length > 1 ? queue.shift() : queue[0]) : queue || { status: 200, doc: {} }
    return { ok: a.status < 400, status: a.status, json: async () => a.doc }
  }
  return { f, calls }
}

const NOT_SHOWN = 'ThruLine records what this check prints. It is not shown while the check runs.'

/* ---------------- 1. the one reading ---------------- */

test('R-RUN output 1: liveOutputOf reads this Change’s running check only, and nothing it was not served', () => {
  const read = M.RUN.liveOutputOf
  assert.equal(typeof read, 'function', 'data/run.js exports liveOutputOf')
  assert.deepEqual(read(RUNNING({ lines: ['Compiling shop', 'running 5 tests'], elapsed: 7 })), { run: 'hq-check-01', lines: ['Compiling shop', 'running 5 tests'], cut: false, elapsedS: 7 })
  assert.deepEqual(read(RUNNING({ lines: [], elapsed: 0 })), { run: 'hq-check-01', lines: [], cut: false, elapsedS: 0 }, 'served, nothing printed yet')
  assert.deepEqual(read(RUNNING({ lines: ['a'], cut: true })), { run: 'hq-check-01', lines: ['a'], cut: true, elapsedS: null })
  /* a sheet that serves no output (a server from before R-RUN): no lines, never [] */
  assert.deepEqual(read(RUNNING()), { run: 'hq-check-01', lines: null, cut: false, elapsedS: null })
  /* another Change's run in the same project is never this one's */
  assert.equal(read(RUNNING({ lines: ['theirs'], elapsed: 3, forThis: false })), null)
  /* a check of this Change started from a terminal: its time, never its output */
  assert.deepEqual(read(LAST({ state: 'running', outcome: 'running', elapsed_s: 12 })), { run: 'hq-check-01', lines: null, cut: false, elapsedS: 12 })
  /* nothing running */
  assert.equal(read(SHEET()), null)
  assert.equal(read(LAST({ outcome: 'passed', tests_passed: 5, tests_failed: 0 })), null)
  assert.equal(read(null), null)
  /* nothing malformed is drawn */
  const odd = SHEET({ job: { run: 'r', for_this_change: true, phase: 'running', elapsed_s: -1, output: { lines: ['ok', 7, null, 'fine'], cut: 'yes' } } })
  assert.deepEqual(read(odd), { run: 'r', lines: ['ok', 'fine'], cut: false, elapsedS: null })
  assert.equal(read(SHEET({ job: { run: 'r', for_this_change: true, elapsed_s: 1.5 } })).elapsedS, null)
})

/* ---------------- 2. the Brief ---------------- */

test('R-RUN output 2: the Brief draws the newest served lines under the command, and the served elapsed time', () => {
  const out = brief(RUNNING({ lines: ['Compiling ledger-api v0.1.0', 'running 5 tests'], elapsed: 3 }))
  assert.match(out, /Checking now/)
  assert.ok(out.includes('$ cargo test'), 'the command heads the box')
  assert.ok(out.includes('Compiling ledger-api v0.1.0') && out.includes('running 5 tests'), out)
  assert.ok(!out.includes(NOT_SHOWN), 'output is served, so the "not shown" sentence is gone')
  assert.match(out, /checking · 3 s/, 'the served elapsed time, not the browser clock')
  assert.ok(!/checking · 42 s/.test(out), 'the browser’s own count was drawn over the record’s')
  assert.ok(!HOLES.test(out), 'a hole: ' + (out.match(/.{40}(\bundefined\b|\bNaN\b|step null).{20}/) || [])[0])

  /* the next second's read: more printed; the box keeps the newest two */
  const later = brief(RUNNING({ lines: ['Compiling ledger-api v0.1.0', 'running 5 tests', 'test post::twice ... ok'], elapsed: 4 }))
  assert.ok(later.includes('running 5 tests') && later.includes('test post::twice ... ok'), later)
  assert.ok(!later.includes('Compiling ledger-api v0.1.0'), 'the box shows the newest lines')
  assert.match(later, /checking · 4 s/)
})

test('R-RUN output 2: nothing printed yet is said; a sheet that serves no output keeps the honest sentence', () => {
  const none = brief(RUNNING({ lines: [], elapsed: 0 }))
  assert.ok(none.includes('$ cargo test') && none.includes('Nothing printed yet.'), none)
  assert.ok(!none.includes(NOT_SHOWN))
  const old = brief(RUNNING())
  assert.ok(old.includes(NOT_SHOWN), 'no output served: none is drawn as if streamed')
  assert.match(old, /checking · 42 s/, 'no elapsed time served: the browser’s count from the job, as before')
  /* another Change's run is never drawn as this one's */
  const other = brief(RUNNING({ lines: ['theirs'], elapsed: 3, forThis: false }))
  assert.ok(!other.includes('theirs'))
})

test('R-RUN output 2: the screen follows the sheet every second through the one run client, and the lines grow until it ends', async () => {
  const sheets = [
    RUNNING({ lines: [], elapsed: 0 }),
    RUNNING({ lines: ['running 5 tests'], elapsed: 1 }),
    RUNNING({ lines: ['running 5 tests', 'test post::twice ... ok'], elapsed: 2 }),
    LAST({ outcome: 'passed', exit_code: 0, tests_passed: 5, tests_failed: 0 }),
  ]
  const { f, calls } = stub({ '/api/changes/chg_1/run': sheets.map((doc) => ({ status: 200, doc })) })
  const seen = []
  const end = await M.RUN.followRun('chg_1', { f, wait: async () => {}, onSheet: (doc) => seen.push(M.RUN.liveOutputOf(doc)) })
  assert.equal(calls.length, 4, 'read until it ended')
  assert.deepEqual(seen.map((s) => (s ? s.lines : null)), [[], ['running 5 tests'], ['running 5 tests', 'test post::twice ... ok'], null])
  assert.deepEqual(seen.slice(0, 3).map((s) => s.elapsedS), [0, 1, 2])
  assert.equal(M.RUN.POLL_MS, 1000)
  /* once it has ended, nothing of it is drawn as running */
  const done = brief(end.doc)
  assert.ok(!done.includes('test post::twice ... ok') && !/Checking now/.test(done), done)
})

/* ---------------- 3. the check sheet ---------------- */

test('R-RUN output 3: the check sheet’s running form draws the served lines, newest last, with the served elapsed time', () => {
  const sheet = RUNNING({ lines: ['collected 5 items', '', 'tests/test_api.py ....'], elapsed: 7 })
  const data = rl(sheet)
  assert.equal(data.run.state, 'running')
  assert.equal(data.run.elapsedS, 7, 'the record’s elapsed time, not 42 s from the browser clock')
  const v = M.vals('rl', { data })
  assert.deepEqual(v.run.lines.map((l) => l.text), ['$ cargo test', 'collected 5 items', '', 'tests/test_api.py ....'])
  assert.equal(v.run.lines[v.run.lines.length - 1].cursor, true, 'the cursor is on the newest line')
  assert.equal(v.run.elapsed, '0:07')
  const out = text('rl', { data })
  assert.ok(out.includes('tests/test_api.py ....'))
  assert.ok(!out.includes(NOT_SHOWN))
  assert.ok(!HOLES.test(out), 'a hole reached the sheet')
})

test('R-RUN output 3: more than the sheet draws, or cut by the server, is said; nothing yet is said; none served is said', () => {
  const many = Array.from({ length: 30 }, (_, i) => 'line ' + (i + 1))
  const lines = logLines(RUNNING({ lines: many, elapsed: 9 }))
  assert.equal(lines[0], '$ cargo test')
  assert.equal(lines[1], 'Earlier lines are not shown.')
  assert.deepEqual(lines.slice(2), many.slice(-M.FROM.RUNNING_LINES), 'the newest lines, oldest first')
  assert.deepEqual(logLines(RUNNING({ lines: ['a', 'b'], cut: true, elapsed: 9 })), ['$ cargo test', 'Earlier lines are not shown.', 'a', 'b'])
  assert.deepEqual(logLines(RUNNING({ lines: [], elapsed: 0 })), ['$ cargo test', 'Nothing printed yet.'])
  assert.deepEqual(logLines(RUNNING()), ['$ cargo test', NOT_SHOWN])
  assert.equal(rl(RUNNING()).run.elapsedS, 42, 'no elapsed time served: the browser’s count from the job, as before')
  assert.deepEqual(logLines(RUNNING({ phase: 'starting', lines: [] })), ['$ cargo test', 'Nothing printed yet.'])
})

/* ---------------- 4. pytest: the pass and fail forms ---------------- */

test('R-RUN output 4: a pytest run gets the pass and fail forms with the counts its record holds', () => {
  const PYTEST = { id: 'pytest', program: 'pytest', command: 'pytest -q' }
  const passed = M.vals('rl', { data: rl(LAST({ outcome: 'passed', exit_code: 0, tests_passed: 5, tests_failed: 0 }, PYTEST)) })
  assert.equal(passed.run.isPassed, true)
  assert.equal(passed.run.title, '5 passed, 0 failed')
  assert.equal(passed.outcomes.find((o) => o.tag === 'this run').w, 'Proven')
  const failed = M.vals('rl', { data: rl(LAST({ outcome: 'failed', exit_code: 1, tests_passed: 4, tests_failed: 1, failure_line: 'FAILED tests/test_api.py::test_total - assert 90 == 95' }, PYTEST)) })
  assert.equal(failed.run.isFailed, true)
  assert.equal(failed.run.title, '1 failed, 4 passed')
  assert.match(failed.draft, /Failing: tests\/test_api\.py::test_total\n/)
  /* a run whose record holds no count draws none */
  const uncounted = M.vals('rl', { data: rl(LAST({ outcome: 'failed', exit_code: 1, tests_passed: null, tests_failed: null }, PYTEST)) })
  assert.equal(uncounted.run.title, 'no test count recorded')
})
