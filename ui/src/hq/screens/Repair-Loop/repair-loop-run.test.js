/* C30 P0-3 and P0-4: Check it again, on the real record.
 *
 * P0-3 — when the server can start the check, the button is on and the click
 * starts it through the existing run routes; while it runs the sheet is read
 * again with its real elapsed time; Stop reaches the cancel route; a refusal
 * is drawn in the server's own words; where the server says it cannot start,
 * its reason is drawn, never a generic "not this version".
 *
 * P0-4 — the copied terminal command is `thruline check --for-change <id> --
 * <runner> <scope…>`; `crates/cli/tests/repair_loop_terminal_command.rs` RUNS
 * the examples pinned in terminal-command.json, which this file pins to the
 * screen's own function.
 *
 * Every fetch is stubbed; nothing here reaches a server or a store. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { changeSnapshot } from '../../data/change.js'
import { selectRepairLoop } from './from-change.js'
import * as Actions from './actions.js'

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
export function render(props) {
  const inst = new Logic(props || {})
  inst.props = props || {}
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}
export function vals(props) { const inst = new Logic(props || {}); inst.props = props || {}; return inst.renderVals() }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'repair-loop-run-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const html = (props) => unescape(M.render(props))
const text = (props) => html(props).replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ')

/* A served ThreadView: a passing run on an older version, an edit at 12:02, then "Tests pass." */
const T = (m, s = 0) => new Date(Date.UTC(2026, 9, 6, 12, m, s)).toISOString()
function thread() {
  const cur = 'wt-2222222222222222'
  return {
    schema_version: 1,
    project: { id: 'p1', name: 'ledger-api', root: '/r/ledger-api' },
    thread: { id: '0123456789abcdef', title: 'Stop the import double-posting', current_version_id: cur, whose_move: 'you', proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: ['/r/ledger-api/src/post.py'] } },
    prompts: [{ event_id: 'ev-p', session_id: 's1', seq: 1, ts: T(0), text: 'the import posts twice', work: true, version_id: 'wt-1111111111111111', raw_file: '/m', raw_line: 1 }],
    edits: [{ id: 'ed0', event_id: 'ed0', session_id: 's1', ts: T(2), file: '/r/ledger-api/src/post.py', op: 'edit', lines_added: 1, lines_removed: 0, ok: true, version_id: cur, raw_file: '/m', raw_line: 3 }],
    commands: [{ id: 'c0#0', event_id: 'c0', part: 0, session_id: 's1', ts: T(1), command: 'pytest -q tests/test_api.py', outcome: 'succeeded', exit_code: 0, is_test: true, is_commit: false, tally: { passed: 4, failed: 0, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 }, version_id: 'wt-1111111111111111', raw_file: '/m', raw_line: 2 }],
    claims: [{ id: 'cl0#0', event_id: 'cl0', session_id: 's1', ts: T(3), text: 'Import fixed. Tests pass.', kind: 'tests_pass', has_verifier: true, rests_on_command_id: 'c0#0', followed_by_passing_test: false, version_id: cur, raw_file: '/m', raw_line: 4 }],
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
const snap = () => changeSnapshot(thread(), null, new Date(Date.UTC(2026, 9, 6, 13, 0)), { storageBytes: 3 * 1024 ** 3 })

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
const live = (sheet, ctx = {}) => selectRepairLoop(snap(), { id: 'chg_1', runSheet: sheet }, { now: NOW, ...ctx })

const GENERIC = /not something this version|not available in this version|this version can.t start/i
const HOLES = /\b(undefined|null|NaN)\b/

/* ---------------- P0-3 ---------------- */

test('P0-3: the server can start it → ▶ Check it again is on, and nothing says this version cannot', () => {
  const data = live(SHEET())
  assert.equal(data.act.canStart, true)
  const h = html({ data })
  const buttons = [...h.matchAll(/<button[^>]*>(?:(?!<\/button>).)*Check it again(?:(?!<\/button>).)*<\/button>/g)].map((m) => m[0])
  assert.ok(buttons.length >= 1, 'a Check it again button is drawn')
  assert.ok(buttons.every((b) => !/aria-disabled="true"/.test(b)), 'every Check it again is on: ' + buttons.join('\n'))
  assert.ok(!GENERIC.test(text({ data })), 'a generic "this version cannot" reached a screen that can start: ' + (text({ data }).match(GENERIC) || [])[0])
  assert.equal((h.match(/class="tl-btn tl-f"/g) || []).length, 1, 'one primary: Check it again; Copy message steps back to the secondary style')
  let n = 0
  M.vals({ data, actions: { checkAgain: () => { n += 1 } } }).againCheck()
  assert.equal(n, 1, 'the click reaches the start')
})

test('P0-3: the first time in a project, the click is the consent, and the sheet says so before it is pressed', () => {
  const data = live(SHEET({ permission: null, consent: { state: null, history: [] } }))
  assert.equal(data.act.canStart, true)
  assert.equal(data.act.needsPermission, true)
  const out = text({ data })
  assert.match(out, /allows checks in ledger-api/)
  assert.match(out, /does not install anything and does not touch the network/)
})

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

test('P0-3: the click POSTs exactly the run request, with this launch’s token, and then reads the sheet', async () => {
  assert.equal(typeof Actions.runControls, 'function', 'actions.js exports runControls')
  const data = live(SHEET())
  const { f, calls } = stub({
    '/api/run/token': { status: 200, doc: { token: 'tok-1' } },
    '/api/changes/chg_1/run': { status: 202, doc: { run: 'hq-check-01', check: 'cargo-test' } },
  })
  let followed = 0
  const said = []
  const controls = Actions.runControls(data.act, { f, said: (w) => said.push(w), follow: () => { followed += 1 } })
  await M.vals({ data, actions: controls }).againCheck()
  const posts = calls.filter((c) => c.method === 'POST')
  assert.deepEqual(posts.map((c) => c.url), ['/api/changes/chg_1/run'], 'exactly one write: the start (permission is already given)')
  assert.equal(posts[0].headers['X-ThruLine-Run-Token'], 'tok-1')
  assert.equal(posts[0].headers['Content-Type'], 'application/json')
  assert.deepEqual(posts[0].body, { check: 'cargo-test' })
  assert.equal(calls[0].url, '/api/run/token', 'the token is read before the write')
  assert.equal(followed, 1, 'after the start, the sheet is followed')
  assert.deepEqual(said, [null], 'a start that went says no refusal')
})

test('P0-3: the first press in a project grants, then starts — the click is the consent', async () => {
  const data = live(SHEET({ permission: null, consent: { state: null, history: [] } }))
  const { f, calls } = stub({ '/api/run/token': { status: 200, doc: { token: 't' } }, '/api/changes/chg_1/run': { status: 202, doc: { run: 'r' } } })
  await Actions.runControls(data.act, { f }).checkAgain()
  assert.deepEqual(calls.filter((c) => c.method === 'POST').map((c) => c.url), ['/api/changes/chg_1/run/permission', '/api/changes/chg_1/run'])
  assert.ok(calls.filter((c) => c.method === 'POST').every((c) => c.headers['X-ThruLine-Run-Token'] === 't'))
})

test('P0-3: running → "Checking now" with the real elapsed time, and Stop is on', () => {
  const data = live(RUNNING())
  assert.equal(data.run.state, 'running')
  assert.equal(data.run.elapsedS, 42, '12:10:00 → 12:10:42')
  const v = M.vals({ data })
  assert.equal(v.run.elapsed, '0:42')
  assert.equal(v.stopDisabled, undefined)
  const out = text({ data })
  assert.match(out, /Checking now/)
  assert.match(out, /0:42/)
  assert.ok(!HOLES.test(out), 'a hole reached the screen: ' + (out.match(/.{40}\b(undefined|null|NaN)\b.{20}/) || [])[0])
  assert.equal(live(RUNNING(), { now: new Date(Date.UTC(2026, 9, 6, 12, 12, 5)) }).run.elapsedS, 125, 'the elapsed time moves with the clock')
})

test('P0-3: while it runs, the sheet is read again until it ends', async () => {
  const { followRun, isRunning } = await import('../../data/run.js')
  const { f, calls } = stub({ '/api/changes/chg_1/run': [{ status: 200, doc: RUNNING() }, { status: 200, doc: RUNNING() }, { status: 200, doc: LAST({ outcome: 'passed', exit_code: 0, tests_passed: 5, tests_failed: 0 }) }] })
  const seen = []
  let waits = 0
  const end = await followRun('chg_1', { f, wait: async () => { waits += 1 }, onSheet: (d) => seen.push(isRunning(d)) })
  assert.deepEqual(calls.map((c) => c.method + ' ' + c.url), ['GET /api/changes/chg_1/run', 'GET /api/changes/chg_1/run', 'GET /api/changes/chg_1/run'])
  assert.deepEqual(seen, [true, true, false])
  assert.equal(waits, 2)
  assert.equal(end.ok, true)
  assert.equal(isRunning(end.doc), false)
})

test('P0-3: Stop this check reaches the cancel route with the run and the person', async () => {
  const data = live(RUNNING())
  const { f, calls } = stub({ '/api/changes/chg_1/runs/hq-check-01/cancel': { status: 200, doc: { cancelled: true, run: 'hq-check-01', cleanup_verified: true } } })
  let followed = 0
  await M.vals({ data, actions: Actions.runControls(data.act, { f, follow: () => { followed += 1 } }) }).stopCheck()
  assert.deepEqual(calls.map((c) => c.method + ' ' + c.url), ['POST /api/changes/chg_1/runs/hq-check-01/cancel'])
  assert.deepEqual(calls[0].body, { actor: 'sam' })
  assert.equal(followed, 1)
})

test('P0-3: still starting → Stop is off, with the server’s reason', () => {
  const data = live(RUNNING('starting'))
  assert.equal(data.run.state, 'running')
  const v = M.vals({ data })
  assert.equal(v.stopDisabled, 'true')
  assert.match(text({ data }), /It is still starting\. Cancel works as soon as it has started\./)
})

test('P0-3: a refusal is drawn in the server’s words', async () => {
  const data = live(SHEET())
  const { f } = stub({ '/api/run/token': { status: 200, doc: { token: 't' } }, '/api/changes/chg_1/run': { status: 403, doc: { error: 'not_in_check_profile' } } })
  const said = []
  await Actions.runControls(data.act, { f, said: (w) => said.push(w) }).checkAgain()
  assert.deepEqual(said, ["That check isn't one this Change lists as counting."])
  data.act.refusal = said[0]
  assert.ok(text({ data }).includes("That check isn't one this Change lists as counting."))
  const cancel = stub({ '/api/changes/chg_1/runs/hq-check-01/cancel': { status: 409, doc: { error: 'already_ended', plain: 'This check had already finished.' } } })
  const words = []
  await Actions.runControls(live(RUNNING()).act, { f: cancel.f, said: (w) => words.push(w) }).stopCheck()
  assert.deepEqual(words, ['This check had already finished.'])
  const running = live(RUNNING())
  running.act.refusal = words[0]
  assert.ok(text({ data: running }).includes('This check had already finished.'))
})

test('P0-3: where the server says it cannot start, its reason is drawn in plain words', () => {
  const cases = [
    [SHEET({ project_folder: null }), 'ThruLine doesn\'t know which folder this Change\'s work is in, so it can\'t run anything yet.', 'Link recorded work to this Change, and its folder is used.'],
    [SHEET({ checks: [] }), 'Nothing this Change lists as counting can be run from here.', 'Accept what counts as done with the tests in it, and this runs them.'],
    [SHEET({ job: { run: 'x', check: 'cargo-test', for_this_change: false, phase: 'running', environment: 'hq', started_at: T(10) } }), 'Another check is running in this project.', 'This one can run as soon as that one finishes.'],
  ]
  for (const [sheet, reason, next] of cases) {
    const data = live(sheet)
    assert.equal(data.act.canStart, false)
    const out = text({ data })
    assert.ok(out.includes(reason), 'reason: ' + reason)
    assert.ok(out.includes(next), 'next: ' + next)
    assert.ok(!GENERIC.test(out), 'generic words: ' + (out.match(GENERIC) || [])[0])
    assert.match(html({ data }), /aria-disabled="true"[^>]*>(?:<span[^>]*>▶<\/span>)?Check it again/)
  }
})

test('P0-3: the run’s end is drawn from the record — passed, failed, stopped — with no hole and no invented step', () => {
  const passed = live(LAST({ outcome: 'passed', exit_code: 0, tests_passed: 5, tests_failed: 0 }))
  assert.equal(passed.run.state, 'passed')
  assert.deepEqual([passed.run.passed, passed.run.failed, passed.run.exit], [5, 0, 0])
  const failed = live(LAST({ outcome: 'failed', exit_code: 101, tests_passed: 3, tests_failed: 2, failure_line: 'test post::twice ... FAILED' }))
  assert.equal(failed.run.state, 'failed')
  const stopped = live(LAST({ state: 'cancelled', outcome: 'cancelled', exit_code: null }))
  assert.equal(stopped.run.state, 'stopped')
  for (const [name, data, words] of [['passed', passed, '5 passed, 0 failed'], ['failed', failed, '2 failed, 3 passed'], ['stopped', stopped, 'You stopped this check']]) {
    const out = text({ data })
    assert.ok(out.includes(words), name + ': ' + words)
    assert.ok(!HOLES.test(out), name + ': a hole reached the screen: ' + (out.match(/.{40}\b(undefined|null|NaN)\b.{20}/) || [])[0])
    assert.ok(!GENERIC.test(out), name + ': generic words')
    assert.ok(!/with\s+in scope|version \S+, \./.test(out), name + ': an empty scope reached a sentence')
  }
  assert.match(text({ data: failed }), /test post::twice \.\.\. FAILED/, 'the failure line the record holds is shown')
})

test('P0-3: a run from before the latest edit is not drawn as this version’s result', () => {
  const old = live(LAST({ outcome: 'passed', exit_code: 0, tests_passed: 5, tests_failed: 0, started_at: T(1, 30), recorded_at: T(1, 50) }))
  assert.notEqual(old.run.state, 'passed')
  const out = text({ data: old })
  assert.ok(!out.includes('5 passed, 0 failed'))
  assert.match(out, /before the latest edit/)
})

test('P0-3: the other ends are said in the record’s words, and the button stays on', () => {
  const nothing = live(LAST({ outcome: 'checked_nothing', why: 'no_test_ran', exit_code: 0 }))
  assert.equal(nothing.act.canStart, true)
  assert.match(text({ data: nothing }), /It finished, but no test ran, so it proves nothing\./)
  const missing = live(LAST({ state: 'blocked', outcome: 'tool_not_found', exit_code: null }))
  assert.equal(missing.act.canStart, true, 'the server would start it again')
  assert.match(text({ data: missing }), /couldn't find cargo/)
})

/* ---------------- P0-4 ---------------- */

const template = () => JSON.parse(readFileSync(join(here, 'terminal-command.json'), 'utf8'))

test('P0-4: the copied command names the Change with --for-change, never a --scope flag the CLI does not have', () => {
  /* the server names no check it can run: the command the statement rested on */
  const data = live(SHEET({ checks: [] }))
  assert.equal(data.check.terminalCommand, 'thruline check --for-change chg_1 -- pytest -q tests/test_api.py')
  assert.ok(!/--scope/.test(data.check.terminalCommand))
  assert.equal(M.vals({ data }).tcCommand, data.check.terminalCommand)
  /* the server names the Change's check: the line runs what the button runs */
  const own = live(SHEET())
  assert.equal(own.check.command, 'cargo test')
  assert.equal(own.check.terminalCommand, 'thruline check --for-change chg_1 -- cargo test')
  /* not tracked: no Change to name, so the run is recorded and attached to nothing */
  assert.equal(selectRepairLoop(snap(), null).check.terminalCommand, 'thruline check -- pytest -q tests/test_api.py')
})

test('P0-4: every example the CLI test runs is exactly what the screen copies', async () => {
  const { terminalCommand } = await import('./terminal.js')
  const TEMPLATE = template()
  assert.ok(Array.isArray(TEMPLATE.examples) && TEMPLATE.examples.length >= 3)
  for (const ex of TEMPLATE.examples) {
    assert.equal(terminalCommand({ changeId: ex.change, runner: ex.runner, scope: ex.scope }), ex.copies, JSON.stringify(ex))
  }
  assert.equal(terminalCommand({ changeId: 'c', runner: null, scope: [] }), null, 'no runner, no command')
})
