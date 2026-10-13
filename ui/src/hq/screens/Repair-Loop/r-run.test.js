/* C30 R-RUN, UI side: every screen that draws "Check it again" can run, stop
 * and finish it, through the one run client (data/run.js → actions.js).
 *
 *  1. From the Brief the click starts the check in place and draws it running.
 *  2. "The click is the consent": the other screens open the check sheet
 *     already running (`…/check?run=1`), and a reload or Back never starts a
 *     second run.
 *  3. Stop reaches the cancel route, from every screen that draws it.
 *  4. All six run outcomes are marked in the honest-outcomes list.
 *  5. The failed-state draft names the failures the record names, and says so
 *     when it names none.
 *  6. No "Not available in this version" on any Check it again.
 *  7. The Brief and the check sheet agree on a run's result.
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
  const file = join(await mkdtemp(join(tmpdir(), 'r-run-' + tag + '-')), 'm.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}

/* Every screen's logic and template, the one Change model and the run client. */
const M = await bundle(`
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import RLTemplate from './template.jsx'
import RLLogic from './logic.js'
import BTemplate from '../Change-Brief/template.jsx'
import BLogic from '../Change-Brief/logic.js'
import ESTemplate from '../Every-Statement/template.jsx'
import ESLogic from '../Every-Statement/logic.js'
import WCTemplate from '../What-Changed/template.jsx'
import WCLogic from '../What-Changed/logic.js'
import CATemplate from '../Command-Ask/template.jsx'
import CALogic from '../Command-Ask/logic.js'
export * as CHANGE from '../../data/change.js'
export * as RUN from '../../data/run.js'
export * as ACTIONS from './actions.js'
export * as ROUTES from '../../routes.js'
export { selectRepairLoop } from './from-change.js'
export { everyStatementFromChange } from '../Every-Statement/from-change.js'
export { whatChangedFromChange, withServedHunks } from '../What-Changed/from-change.js'
export { commandFromChange } from '../Command-Ask/from-change.js'
const SCREENS = { rl: [RLLogic, RLTemplate], brief: [BLogic, BTemplate], es: [ESLogic, ESTemplate], wc: [WCLogic, WCTemplate], ca: [CALogic, CATemplate] }
export function instance(name, props, state) {
  const inst = new SCREENS[name][0](props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return inst
}
export function vals(name, props, state) { return instance(name, props, state).renderVals() }
export function render(name, props, state) {
  return renderToStaticMarkup(React.createElement(SCREENS[name][1], { v: vals(name, props, state), link: (h) => h }))
}`, 'all')

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const html = (name, props, state) => unescape(M.render(name, props, state))
/** What a person can read: the text, and every title and aria-label. */
const text = (name, props, state) => {
  const h = html(name, props, state)
  const attrs = (h.match(/(?:aria-label|title)="[^"]*"/g) || []).map((a) => a.replace(/^[^"]*"|"$/g, '')).join('\n')
  return h.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ') + '\n' + attrs
}

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

/* The served run sheet (`GET /api/changes/<id>/run`, run_route.rs `view`). */
const SHEET = (o = {}) => ({
  schema_version: 2, change: 'chg_1', project_folder: '~/ledger-api',
  permission: { granted_by: 'sam', granted_at: T(5) },
  consent: { state: 'given', history: [], file: '.thruline/run-permission.json' },
  checks: [{ id: 'cargo-test', program: 'cargo', category: 'tests', command: 'cargo test', last: null }],
  not_runnable_here: [], environments: [], remembered_environment: null, job: null, not_started: null, actor: 'sam',
  ...o,
})
const RUNNING = (phase = 'running') => SHEET({ job: { run: 'hq-check-01', check: 'cargo-test', for_this_change: true, phase, environment: 'hq', started_at: T(10) } })
const LAST = (last) => SHEET({ checks: [{ id: 'cargo-test', program: 'cargo', category: 'tests', command: 'cargo test', last: { run: 'hq-check-01', state: 'completed', why: null, reason: null, signal: null, suites: 1, failure_line: null, fetch_skipped: null, survivors: [], settles: true, started_at: T(10), recorded_at: T(11, 30), cleanup_verified: true, ...last } }] })
const NOW = new Date(Date.UTC(2026, 9, 6, 12, 10, 42))

/** The one Change snapshot of a TRACKED Change (`chg_1`), its run sheet folded in. */
const tracked = (sheet, now = NOW) => M.CHANGE.changeSnapshot(thread(), { change: { id: 'chg_1' }, hq: null, proposals: null, runSheet: sheet }, now)
const untracked = () => M.CHANGE.changeSnapshot(thread(), null, NOW)
const rl = (sheet, now = NOW) => M.selectRepairLoop(tracked(sheet, now), { id: 'chg_1', runSheet: sheet }, { now })

function stub(answers) {
  const calls = []
  const f = async (url, init = {}) => {
    calls.push({ url, method: init.method || 'GET', headers: init.headers || {}, body: init.body ? JSON.parse(init.body) : null })
    const queue = answers[url]
    const a = Array.isArray(queue) ? (queue.length > 1 ? queue.shift() : queue[0]) : queue || { status: 200, doc: {} }
    return { ok: a.status < 400, status: a.status, json: async () => a.doc }
  }
  return { f, calls }
}
const posts = (calls) => calls.filter((c) => c.method === 'POST').map((c) => c.url)

/* What the controls act on. Where data/run.js does not have the one reading,
 * the check sheet's own (from-change.js `act`) stands in, so a screen that
 * ignores the act fails on what it draws, not on a missing name. */
const actOf = (id, sheet) => (M.RUN.runActOf ? M.RUN.runActOf(id, sheet) : M.selectRepairLoop(untracked(), id ? { id, runSheet: sheet } : null, { now: NOW }).act)
const opensOf = (o) => (M.RUN.openCheckActions ? M.RUN.openCheckActions(o) : { check: () => {}, stopCheck: o.act && o.act.runKey ? o.stop : undefined })

/* The Brief's in-place actions (Change-Brief/check.js), bundled on their own so a
 * missing module is one failing test, not a broken file. */
async function briefCheck() {
  try {
    return await bundle(`export * from '../Change-Brief/check.js'`, 'brief')
  } catch (e) {
    assert.fail('Change-Brief/check.js (the Brief’s in-place Check it again) is missing: ' + String(e && e.message).split('\n')[0])
  }
}

/* ---------------- 1. Brief: the click starts the check in place, and it draws running ---------------- */

test('R-RUN 1: from the Brief, ▶ Check it again starts the check in place, and the Brief draws it running', async () => {
  /* the sheet says this Change's check is running: the Brief (the one snapshot) draws it, in place */
  const going = tracked(RUNNING())
  assert.equal(going.primaryAction, 'stop_check', 'the Change snapshot reads the running check from its run sheet')
  const drawn = text('brief', { data: going, actions: { check: () => {}, stopCheck: () => {} } })
  assert.match(drawn, /Checking now/)

  const B = await briefCheck()
  const act = M.RUN.runActOf('chg_1', SHEET())
  assert.equal(act.canStart, true)
  const { f, calls } = stub({ '/api/run/token': { status: 200, doc: { token: 'tok' } }, '/api/changes/chg_1/run': { status: 202, doc: { run: 'hq-check-01' } } })
  const controls = M.ACTIONS.runControls(act, { f })
  let opened = 0
  const actions = B.briefCheckActions({ act, start: controls.checkAgain, stop: controls.stopCheck, open: () => { opened += 1 } })
  const ready = tracked(SHEET())
  assert.equal(M.vals('brief', { data: ready, actions }).chk.off, undefined, 'the control is on')
  await M.vals('brief', { data: ready, actions }).runCheck()
  assert.deepEqual(posts(calls), ['/api/changes/chg_1/run'], 'the press starts the one check, here')
  assert.equal(calls.find((c) => c.method === 'POST').headers['X-ThruLine-Run-Token'], 'tok')
  assert.equal(opened, 0, 'the Brief does not leave for the sheet: it runs in place')

  /* the sheet now says it is running: the Brief draws Checking now, the real elapsed time, and Stop */
  const out = text('brief', { data: going, actions })
  assert.match(out, /Checking now/)
  assert.match(out, /42 s/, '12:10:00 → 12:10:42')
  assert.match(out, /Stop this check/)
  assert.match(out, /It is not shown while the check runs\./, 'no output is drawn as if streamed; it says what is shown')
  assert.ok(!/\b(undefined|NaN)\b|step null/.test(out), 'a hole reached the Brief: ' + (out.match(/.{40}\b(undefined|NaN|step null)\b.{20}/) || [])[0])
  assert.equal(M.CHANGE.withRunSheet(going, RUNNING(), new Date(NOW.getTime() + 83000)).runs.find((r) => r.status === 'running').elapsedS, 125, 'the elapsed time moves with the clock')
})

test('R-RUN 1: untracked work on the Brief opens the check sheet (Track this work as a Change); a server that will not start draws its reason', async () => {
  const B = await briefCheck()
  let opened = 0
  let started = 0
  const none = B.briefCheckActions({ act: M.RUN.runActOf(null, null), start: () => { started += 1 }, stop: () => {}, open: () => { opened += 1 } })
  M.vals('brief', { data: untracked(), actions: none }).runCheck()
  assert.deepEqual([opened, started], [1, 0])
  const busy = SHEET({ job: { run: 'x', check: 'cargo-test', for_this_change: false, phase: 'running', environment: 'hq', started_at: T(10) } })
  const off = B.briefCheckActions({ act: M.RUN.runActOf('chg_1', busy), start: () => { started += 1 }, stop: () => {}, open: () => { opened += 1 } })
  const v = M.vals('brief', { data: tracked(busy), actions: off })
  assert.equal(v.chk.off, 'true')
  assert.equal(v.chk.why, 'Another check is running in this project. This one can run as soon as that one finishes.')
  v.runCheck()
  assert.deepEqual([opened, started], [1, 0], 'an off control does nothing')
})

/* ---------------- 2. the click is the consent; a reload never starts a second run ---------------- */

function fakeWindow(hash) {
  const win = {
    location: { hash },
    entries: [hash],
    history: {
      state: null,
      replaceState(state, _title, url) { win.location.hash = String(url).slice(String(url).indexOf('#')); win.entries[win.entries.length - 1] = win.location.hash },
    },
  }
  return win
}

test('R-RUN 2: the other screens open the sheet with the intent; it is read once and taken out of the address, so a reload or Back never runs again', () => {
  assert.equal(typeof M.RUN.takeRunIntent, 'function', 'data/run.js exports takeRunIntent')
  const sheetHref = M.ROUTES.href('check', { change: 'chg_1', project: 'p1' })
  const target = M.RUN.checkRunHref(sheetHref)
  assert.equal(target, '#/changes/chg_1/check?project=p1&run=1')
  assert.deepEqual(M.ROUTES.parse(target), { screen: 'check', change: 'chg_1', project: 'p1' }, 'the router still reads the sheet')

  /* arrive: the intent is taken, and the address no longer carries it */
  const win = fakeWindow(target)
  let starts = 0
  const arrive = (w) => { if (M.RUN.takeRunIntent(w)) starts += 1 }
  arrive(win)
  assert.equal(starts, 1)
  assert.equal(win.location.hash, '#/changes/chg_1/check?project=p1')
  assert.deepEqual(win.entries, ['#/changes/chg_1/check?project=p1'], 'no history entry keeps the intent: Back and Forward land on the plain sheet')
  /* reload: the same window, the same address — nothing starts */
  arrive(win)
  arrive(fakeWindow(win.location.hash))
  assert.equal(starts, 1, 'a reload started a second run')
  /* an address that cannot be rewritten is refused: a run a reload could repeat never starts */
  const stuck = fakeWindow(target)
  stuck.history.replaceState = () => { throw new Error('denied') }
  assert.equal(M.RUN.takeRunIntent(stuck), false)
  const ignored = fakeWindow(target)
  ignored.history.replaceState = () => {}
  assert.equal(M.RUN.takeRunIntent(ignored), false, 'an address that still carries the intent starts nothing')
})

test('R-RUN 2: arriving with the intent starts only when the server would, and follows a check already going instead of starting another', () => {
  const step = (sheet, extra = {}) => M.RUN.arrivalStep({ snapshot: {}, trackedLoading: false, act: M.RUN.runActOf('chg_1', sheet), sheet, ...extra })
  assert.equal(step(SHEET()), 'start')
  assert.equal(step(RUNNING()), 'follow', 'a running check is followed, never started twice')
  assert.equal(step(RUNNING('starting')), 'follow')
  assert.equal(step(SHEET({ checks: [] })), 'drop', 'the ready form says why')
  assert.equal(step(SHEET(), { trackedLoading: true }), 'wait')
  assert.equal(M.RUN.arrivalStep({ snapshot: {}, trackedLoading: false, act: M.RUN.runActOf(null, null), sheet: null }), 'drop', 'untracked: the sheet offers Track this work as a Change')
})

test('R-RUN 2: Every statement, What changed and ⌘K send the press to the sheet already running; blocked stays put with the reason', () => {
  const at = { change: 'chg_1', project: 'p1' }
  const go = []
  const a = M.RUN.openCheckActions({ act: M.RUN.runActOf('chg_1', SHEET()), at, go: (h) => go.push(h) })
  a.check()
  assert.deepEqual(go, ['#/changes/chg_1/check?project=p1&run=1'])
  assert.equal(a.checkHref, '#/changes/chg_1/check?project=p1&run=1')
  const u = M.RUN.openCheckActions({ act: M.RUN.runActOf(null, null), at, go: (h) => go.push(h) })
  u.check()
  assert.equal(go[1], '#/changes/chg_1/check?project=p1', 'untracked: the sheet without the intent (it offers Track this work)')
  const b = M.RUN.openCheckActions({ act: M.RUN.runActOf('chg_1', SHEET({ project_folder: null })), at, go: (h) => go.push(h) })
  b.check()
  assert.equal(go.length, 2, 'a blocked press goes nowhere')
  assert.equal(b.checkBlocked.reason, 'ThruLine doesn\'t know which folder this Change\'s work is in, so it can\'t run anything yet.')

  /* the screens' own controls reach it */
  const es = M.everyStatementFromChange(tracked(SHEET()))
  let pressed = 0
  M.vals('es', { data: es, actions: { ...a, check: () => { pressed += 1 } } }, { sheet: 'tests' }).runCheck()
  const wc = M.whatChangedFromChange(M.withServedHunks(tracked(SHEET()), thread()))
  M.vals('wc', { data: wc, actions: { ...a, check: () => { pressed += 1 } } }).act.check()
  assert.equal(pressed, 2, 'Every statement’s Run it and What changed’s Check it again press the act')

  /* ⌘K: Act · Check it again links to the sheet already running */
  const ca = M.commandFromChange(tracked(SHEET()), { project: { id: 'p1', name: 'ledger-api', root: '/r/ledger-api' }, threads: [] }, { readAt: '12:10' })
  const h = html('ca', { data: ca, actions: a, at: { screen: 'brief', ...at } })
  assert.match(h, /id="tl-opt-act-check"[^>]*href="#\/changes\/chg_1\/check\?project=p1&run=1"/)
})

/* ---------------- 3. Stop cancels, from every screen that draws it ---------------- */

test('R-RUN 3: Stop this check reaches the cancel route — from the Brief, Every statement, What changed and the sheet', async () => {
  const act = actOf('chg_1', RUNNING())
  assert.equal(act.runKey, 'hq-check-01')
  const CANCEL = '/api/changes/chg_1/runs/hq-check-01/cancel'
  const press = async (where, fire) => {
    const { f, calls } = stub({ [CANCEL]: { status: 200, doc: { cancelled: true, run: 'hq-check-01', cleanup_verified: true } } })
    await fire(M.ACTIONS.runControls(act, { f }).stopCheck)
    assert.deepEqual(calls.map((c) => c.method + ' ' + c.url), ['POST ' + CANCEL], where + ': Stop reaches the cancel route')
    assert.deepEqual(calls[0].body, { actor: 'sam' })
  }
  const going = tracked(RUNNING())
  await press('the check sheet', (stop) => M.vals('rl', { data: rl(RUNNING()), actions: { stopCheck: stop } }).stopCheck())
  const es = M.everyStatementFromChange(going)
  await press('Every statement', (stop) => M.vals('es', { data: es, actions: opensOf({ act, stop, at: { change: 'chg_1' } }) }).stopCheck())
  assert.equal(es.running, true, 'Every statement reads the running check from the record')
  const wc = M.whatChangedFromChange(M.withServedHunks(going, thread()))
  await press('What changed', (stop) => M.vals('wc', { data: wc, actions: opensOf({ act, stop, at: { change: 'chg_1' } }) }).act.stop())
  const wv = M.vals('wc', { data: wc, actions: opensOf({ act, stop: () => {}, at: { change: 'chg_1' } }) })
  assert.equal(wv.act.checking, true, 'What changed reads the running check from the record')
  const B = await briefCheck()
  await press('the Brief', (stop) => M.vals('brief', { data: going, actions: B.briefCheckActions({ act, start: () => {}, stop, open: () => {} }) }).stopCheck())
})

/* ---------------- 4. all six outcomes are marked ---------------- */

test('R-RUN 4: each of the six run outcomes is marked "this run" on its own row of the honest outcomes', () => {
  const cases = [
    ['passed', LAST({ outcome: 'passed', exit_code: 0, tests_passed: 5, tests_failed: 0 }), 'Proven'],
    ['failed', LAST({ outcome: 'failed', exit_code: 101, tests_passed: 3, tests_failed: 2, failure_line: 'test post::twice ... FAILED' }), 'Not proven'],
    ['checked nothing', LAST({ outcome: 'checked_nothing', why: 'no_test_ran', exit_code: 0 }), 'This is not a pass.'],
    ['tool not found', LAST({ state: 'blocked', outcome: 'tool_not_found', exit_code: null }), 'Nothing ran.'],
    ['stopped by you', LAST({ state: 'cancelled', outcome: 'cancelled', exit_code: null }), 'Settles nothing.'],
    ['interrupted', LAST({ state: 'interrupted', outcome: 'interrupted', reason: 'hq_restarted', exit_code: null }), 'Settles nothing.'],
  ]
  const OUT = { 'stopped by you': 'stopped by you', interrupted: 'interrupted', 'checked nothing': 'ran, checked nothing', 'tool not found': 'tool not found' }
  for (const [name, sheet, word] of cases) {
    const v = M.vals('rl', { data: rl(sheet) })
    const marked = v.outcomes.filter((o) => o.tag === 'this run')
    assert.equal(marked.length, 1, name + ': exactly one row is marked')
    assert.equal(marked[0].w, word, name)
    if (OUT[name]) assert.equal(marked[0].out, OUT[name], name)
    const out = text('rl', { data: rl(sheet) })
    assert.ok(!/\b(undefined|NaN)\b/.test(out), name + ': a hole reached the sheet')
  }
})

/* ---------------- 5. the failed-state draft names the failures ---------------- */

test('R-RUN 5: the failed-state draft names the failing test the record names, and how many it does not', () => {
  const v = M.vals('rl', { data: rl(LAST({ outcome: 'failed', exit_code: 101, tests_passed: 3, tests_failed: 2, failure_line: 'test post::twice ... FAILED' })) })
  assert.match(v.draft, /Failing: post::twice\n/)
  assert.match(v.draft, /  test post::twice \.\.\. FAILED\n/, 'the recorded line itself')
  assert.match(v.draft, /1 more failed; the record names only the first\./)
  const py = M.vals('rl', { data: rl(LAST({ outcome: 'failed', exit_code: 1, tests_passed: 4, tests_failed: 1, failure_line: 'FAILED tests/test_api.py::test_import_twice - assert 2 == 1' })) })
  assert.match(py.draft, /Failing: tests\/test_api\.py::test_import_twice\n/)
  assert.ok(!/more failed/.test(py.draft))
  assert.deepEqual(M.RUN.failuresOf({ failure_line: '--- FAIL: TestPost (0.00s)', tests_failed: 1 }).tests, ['TestPost'])
})

test('R-RUN 5: when nothing parses, the draft says so', () => {
  const none = M.vals('rl', { data: rl(LAST({ outcome: 'failed', exit_code: 101, tests_passed: null, tests_failed: null, failure_line: null })) })
  assert.match(none.draft, /The record names no failing test for this run, and keeps no failure line\./)
  const odd = M.vals('rl', { data: rl(LAST({ outcome: 'failed', exit_code: 2, tests_passed: 0, tests_failed: 3, failure_line: 'error[E0425]: cannot find value `x`' })) })
  assert.match(odd.draft, /The record names no failing test for this run; the failure line it kept is below\.\n  error\[E0425\]/)
  assert.ok(!/Failing:/.test(odd.draft), 'no test name is invented')
})

/* ---------------- 6. census: no "Not available in this version" on any Check it again ---------------- */

const GENERIC = /not available in this version|not something this version|this version can.t start/i

test('R-RUN 6: no screen that draws Check it again says "Not available in this version", in any run state', () => {
  const sheets = { ready: SHEET(), first: SHEET({ permission: null, consent: { state: null, history: [] } }), running: RUNNING(), starting: RUNNING('starting'), blocked: SHEET({ checks: [] }), busy: SHEET({ job: { run: 'x', check: 'cargo-test', for_this_change: false, phase: 'running', environment: 'hq', started_at: T(10) } }), passed: LAST({ outcome: 'passed', exit_code: 0, tests_passed: 5, tests_failed: 0 }), failed: LAST({ outcome: 'failed', exit_code: 1, tests_passed: 1, tests_failed: 1 }) }
  const seen = []
  const check = (where, out) => {
    seen.push(where)
    assert.ok(!GENERIC.test(out), where + ': ' + (out.match(new RegExp('.{60}(' + GENERIC.source + ').{40}', 'i')) || [])[0])
    /* and the run the sheet folded in leaves no hole where a step or a count would be */
    assert.ok(!/\b(undefined|NaN)\b|step null|steps null/.test(out), where + ': a hole: ' + (out.match(/.{50}(\bundefined\b|\bNaN\b|step null|steps null).{30}/) || [])[0])
  }
  for (const [name, sheet] of Object.entries(sheets).concat([['untracked', null]])) {
    const snap = sheet ? tracked(sheet) : untracked()
    const act = actOf(sheet ? 'chg_1' : null, sheet)
    const opens = opensOf({ act, stop: () => {}, at: { change: 'chg_1', project: 'p1' }, go: () => {} })
    const brief = { check: () => {}, stopCheck: act.runKey ? () => {} : undefined, inPlace: act.canStart, checkBlocked: !act.canStart && act.blocked ? act.blocked : null, needsPermission: act.needsPermission }
    check('Brief/' + name, text('brief', { data: snap, actions: brief }) + text('brief', { data: snap, actions: brief }, { tech: true, open: { why: true, then: true, kind: true, after: true, tech: true } }))
    const es = M.everyStatementFromChange(snap)
    check('Every-Statement/' + name, text('es', { data: es, actions: opens }) + text('es', { data: es, actions: opens }, { sheet: 'tests', sel: es.selected }))
    check('What-Changed/' + name, text('wc', { data: M.whatChangedFromChange(M.withServedHunks(snap, thread())), actions: opens }))
    check('Repair-Loop/' + name, text('rl', { data: M.selectRepairLoop(snap, sheet ? { id: 'chg_1', runSheet: sheet } : null, { now: NOW }) }))
    check('Command-Ask/' + name, text('ca', { data: M.commandFromChange(snap, { project: { id: 'p1', name: 'ledger-api' }, threads: [] }, { readAt: '12:10' }), actions: opens, at: { screen: 'brief', change: 'chg_1', project: 'p1' } }))
  }
  assert.equal(seen.length, 45)
})

/* ---------------- 7. the Brief and the sheet agree ---------------- */

test('R-RUN 7: a check run from the sheet is the Brief’s result too — passed proves, failed contradicts, on both screens', () => {
  const passed = LAST({ outcome: 'passed', exit_code: 0, tests_passed: 5, tests_failed: 0 })
  const snap = tracked(passed)
  assert.equal(snap.change.state, 'proven', 'the Change snapshot reads the sheet’s pass')
  const b = M.vals('brief', { data: snap, actions: { check: () => {} } })
  assert.equal(b.pill.word, 'Proven', 'the Brief’s state pill')
  assert.equal(M.vals('brief', { data: untracked(), actions: { check: () => {} } }).pill.word, 'Not proven', 'without the sheet, the same record is not proven')
  const r = M.vals('rl', { data: rl(passed) })
  assert.equal(r.pill.p, true, 'the sheet says Proven')
  assert.equal(r.run.isPassed, true)
  for (const name of ['brief', 'rl']) {
    const out = name === 'brief' ? text('brief', { data: snap, actions: { check: () => {} } }, { tech: true, open: { why: true, then: true, kind: true, after: true, tech: true } }) : text('rl', { data: rl(passed) })
    assert.ok(!/\b(undefined|NaN)\b|step null/.test(out), name + ': a hole: ' + (out.match(/.{40}\b(undefined|NaN|step null)\b.{20}/) || [])[0])
  }

  const failed = LAST({ outcome: 'failed', exit_code: 101, tests_passed: 3, tests_failed: 2, failure_line: 'test post::twice ... FAILED' })
  const fsnap = tracked(failed)
  assert.equal(fsnap.change.state, 'contradicted')
  assert.equal(M.vals('rl', { data: rl(failed) }).pill.f, true)

  /* a run from before the latest edit is nobody's result on this version */
  const old = LAST({ outcome: 'passed', exit_code: 0, tests_passed: 5, tests_failed: 0, started_at: T(1, 30), recorded_at: T(1, 50) })
  assert.equal(tracked(old).change.state, 'not_proven')
  assert.equal(M.vals('rl', { data: rl(old) }).pill.p, false)

  /* Every statement and What changed read the same snapshot */
  const es = M.everyStatementFromChange(snap)
  assert.equal(es.statements[0].proof.verdict, 'proven')
  const wc = M.whatChangedFromChange(M.withServedHunks(snap, thread()))
  assert.equal(wc.statements[0].proof.verdict, 'proven')
  const wout = text('wc', { data: wc, actions: {} })
  assert.ok(!/step null|\bundefined\b/.test(wout), 'What changed: a hole: ' + (wout.match(/.{60}(step null|\bundefined\b).{30}/) || [])[0])
})

/* ---------------- P0-4 on the Brief: its technical line is the command that exists ---------------- */

test('P0-4 on the Brief: the Technical line is the command Repair-Loop copies, never `--scope`', async () => {
  const B = await briefCheck()
  const actions = B.briefCheckActions({ act: M.RUN.runActOf('chg_1', SHEET()), start: () => {}, stop: () => {}, open: () => {} })
  const out = text('brief', { data: tracked(SHEET()), actions }, { tech: true })
  assert.ok(out.includes('thruline check --for-change chg_1 -- pytest -q tests/test_api.py'), 'the line names the Change and the runner')
  assert.doesNotMatch(out, /--scope/)
  const loose = text('brief', { data: untracked(), actions: B.briefCheckActions({ act: null, start: () => {}, stop: () => {}, open: () => {} }) }, { tech: true })
  assert.ok(loose.includes('thruline check -- pytest -q tests/test_api.py'), 'untracked work: the command records and attaches to nothing')
  assert.doesNotMatch(loose, /--scope|--for-change/)
})

test('R-RUN 3: while a check is still starting, the Brief’s Stop says why it cannot stop yet instead of doing nothing silently', async () => {
  const B = await briefCheck()
  const starting = RUNNING('starting')
  const actions = B.briefCheckActions({ act: M.RUN.runActOf('chg_1', starting), start: () => {}, stop: () => {}, open: () => {} })
  assert.equal(actions.stopCheck, undefined, 'nothing to stop yet')
  const out = text('brief', { data: tracked(starting), actions })
  assert.match(out, /Checking now/)
  assert.ok(out.includes(actions.stopWhy), 'the reason is drawn: ' + actions.stopWhy)
  const running = B.briefCheckActions({ act: M.RUN.runActOf('chg_1', RUNNING()), start: () => {}, stop: () => {}, open: () => {} })
  assert.equal(running.stopWhy, null)
  assert.match(text('brief', { data: tracked(RUNNING()), actions: running }), /Stopping it keeps whatever it printed so far\./)
})
