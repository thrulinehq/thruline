/* W2-D, HQ Next wave 2: Repair-Loop on the real record.
 *
 * The selector reads the ONE Change snapshot (data/change.js) — fed here both
 * the canvas's own story (Change-Brief/demo.js, every state) and a synthetic
 * served ThreadView pushed through `changeSnapshot` — and a tracked Change's run
 * sheet. The writes are proven against a stubbed `fetch`; nothing here reaches
 * a server. The design rendering's pixel match is scripts/hq/pixdiff.py's job. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { DEMO as BRIEF } from '../Change-Brief/demo.js'
import { changeSnapshot } from '../../data/change.js'
import { selectRepairLoop } from './from-change.js'
import { checkAgain, stopCheck, RUN_TOKEN, RUN_START, RUN_PERMISSION, RUN_CANCEL } from './actions.js'
import { KEY_TABLE, wiredOn } from '../../keys/table.js'

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
export function render(props, state) {
  const inst = new Logic(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}
export function vals(props) { const inst = new Logic(props || {}); inst.props = props || {}; return inst.renderVals() }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'repair-loop-live-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const html = (props, state) => unescape(M.render(props, state))
const text = (props, state) => html(props, state).replace(/<[^>]*>/g, ' ')

/* A served ThreadView in the API.md shape: a passing test run, then an edit, then "Tests pass." */
const T = (m) => new Date(Date.UTC(2026, 9, 6, 12, m, 0)).toISOString()
function thread({ second } = {}) {
  const cur = 'wt-2222222222222222'
  const commands = [{ id: 'c0#0', event_id: 'c0', part: 0, session_id: 's1', ts: T(1), command: 'pytest -q tests/test_api.py', outcome: 'succeeded', exit_code: 0, is_test: true, is_commit: false, tally: { passed: 4, failed: 0, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 }, version_id: 'wt-1111111111111111', raw_file: '/m', raw_line: 2 }]
  if (second) commands.push({ id: 'c1#0', event_id: 'c1', part: 0, session_id: 's1', ts: T(4), command: 'pytest -q tests/test_api.py', outcome: second, exit_code: second === 'failed' ? 1 : 0, is_test: true, is_commit: false, tally: second === 'failed' ? { passed: 3, failed: 2, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 } : { passed: 5, failed: 0, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 }, version_id: cur, raw_file: '/m', raw_line: 6 })
  return {
    schema_version: 1,
    project: { id: 'p1', name: 'ledger-api', root: '/r/ledger-api' },
    thread: { id: '0123456789abcdef', title: 'Stop the import double-posting', current_version_id: cur, whose_move: 'you', proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: ['/r/ledger-api/src/post.py'] } },
    prompts: [{ event_id: 'ev-p', session_id: 's1', seq: 1, ts: T(0), text: 'the import posts twice', work: true, version_id: 'wt-1111111111111111', raw_file: '/m', raw_line: 1 }],
    edits: [{ id: 'ed0', event_id: 'ed0', session_id: 's1', ts: T(2), file: '/r/ledger-api/src/post.py', op: 'edit', lines_added: 1, lines_removed: 0, ok: true, version_id: cur, raw_file: '/m', raw_line: 3 }],
    commands,
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
const snap = (o) => changeSnapshot(thread(o), null, new Date(Date.UTC(2026, 9, 6, 13, 0)), { storageBytes: 3 * 1024 ** 3 })
const ENGINE_WORDS = /needs engine|NEEDS ENGINE|\bengine\b|endpoint|\broute\b/i
const DESIGN_NOTES = ['Demo state', 'Design note']

test('the canvas story, selected from the one snapshot, is the canvas’s own Repair-Loop record', () => {
  const d = selectRepairLoop(BRIEF.states.s1, null)
  assert.equal(d.statement.text, 'Tests pass.')
  assert.equal(d.statement.said, 'Checkout total fixed. Tests pass.')
  assert.deepEqual([d.heldRun.step, d.heldRun.version, d.heldRun.passed, d.heldRun.failed, d.heldRun.versionsBehind], [46, '9f2ad3e', 4, 0, 1])
  assert.deepEqual([d.movedEdit.file, d.movedEdit.step], ['checkout/api.py', 56])
  assert.deepEqual([d.version.id, d.version.commit, d.version.clean], ['e71c5b0', '3f9a1c2', true])
  assert.equal(d.check.command, 'pytest -q tests/test_totals.py checkout/')
  assert.equal(d.check.covers, 'checkout/')
  assert.equal(d.run.state, 'unavailable', 'with no tracked Change, nothing can start a check from here')
  assert.equal(d.readAt, '16:04')
  assert.equal(d.concern.raisedAt, '16:04')
})

test('every canvas state maps to its run form: checking → running, ready → passed, moved → a new stop', () => {
  assert.equal(selectRepairLoop(BRIEF.states.checking, null).run.state, 'running')
  assert.equal(selectRepairLoop(BRIEF.states.checking, null).run.elapsedS, 3)
  const ready = selectRepairLoop(BRIEF.states.ready, null)
  assert.deepEqual([ready.run.state, ready.run.step, ready.run.passed, ready.run.failed], ['passed', 62, 5, 0])
  const moved = selectRepairLoop(BRIEF.states.moved, null)
  assert.equal(moved.version.id, 'a1b2c3d')
  assert.deepEqual([moved.heldRun.step, moved.movedEdit.step], [62, 64])
  assert.equal(moved.run.state, 'unavailable')
})

test('C54 check#10: a failed recheck named by the proof is the current run on the sheet', () => {
  const snapshot = structuredClone(BRIEF.states.s1)
  const failed = {
    id: 'run-64', step: 64, time: '16:11', kind: 'test',
    command: 'pytest -q tests/test_totals.py checkout/', version: snapshot.currentVersion,
    status: 'finished', exitCode: 101, passed: 4, failed: 1, durationMs: 420,
    capturedBy: 'thruline', scope: ['checkout/'],
  }
  snapshot.runs.push(failed)
  snapshot.statements[0].proof.recheck = { status: 'failed', runId: failed.id }
  const data = selectRepairLoop(snapshot, null)
  assert.deepEqual([data.run.state, data.run.step, data.run.exit, data.run.failed], ['failed', 64, 101, 1])
  assert.match(M.vals({ data }).draft, /1 failed, 4 passed/)
})

test('C54 check#12: with no named runner, the terminal command cannot be copied', () => {
  const snapshot = structuredClone(BRIEF.states.s1)
  snapshot.recheck.runner = null
  const data = selectRepairLoop(snapshot, null)
  assert.equal(data.check.terminalCommand, null)
  const v = M.vals({ data })
  assert.equal(v.tcCopyDisabled, 'true')
  assert.match(v.tcCommand, /No command recorded/)
  v.copyTc()
  assert.equal(M.vals({ data }).tcCopyLabel, 'Copy command')
  assert.match(html({ data }), /aria-disabled="true"[^>]*>Copy command/)
})

test('C54 check#13: a failed cited run is described as failed in every repair summary', () => {
  const snapshot = structuredClone(BRIEF.states.s1)
  const held = snapshot.runs.find((r) => r.id === 'run-46')
  Object.assign(held, { exitCode: 101, passed: 4, failed: 1 })
  const v = M.vals({ data: selectRepairLoop(snapshot, null) })
  assert.match(v.draft, /run you cite .* failed with 1 failed, 4 passed on version/)
  assert.doesNotMatch([v.draft, v.behindHead, v.concernWas].join(' '), /(?:run you cite|tests) passed on/)
})

test('C54 check#16: the drawn Repair Loop keys reach their matching live controls', () => {
  for (const id of ['check-half', 'primary', 'copy', 'approve']) {
    assert.equal(wiredOn(KEY_TABLE.find((e) => e.id === id), 'Repair-Loop'), true, id)
  }
  const calls = []
  const open = selectRepairLoop(BRIEF.states.s1, null)
  open.act.canStart = true
  const v = M.vals({ data: open, actions: {
    checkAgain: () => calls.push('check'),
    toldAct: (act) => calls.push(act),
  } })
  v.keys.primary()
  v.keys.copy()
  assert.deepEqual(calls, ['check', 'copied'])
  const ready = selectRepairLoop(BRIEF.states.ready, null)
  M.vals({ data: ready, actions: { openApprove: () => calls.push('approve') } }).keys.approve()
  assert.equal(calls.at(-1), 'approve')
})

test('Honest outcomes: a tally row is drawn only when the record holds an outcome of that kind on this version', () => {
  const none = selectRepairLoop(snap(), null)
  assert.equal(none.outcomeExamples.passed, null, 'the only pass ran on an older version: no tally row')
  assert.equal(none.outcomeExamples.failed, null, 'no failed run in the record: no invented "1 failed" row')
  assert.deepEqual(M.vals({ data: none }).outcomes.map((o) => o.out), ['ran, checked nothing', 'tool not found', 'stopped by you', 'interrupted'],
    'no tally row the record does not hold for this version; the kinds with no number stay')
  const failedData = selectRepairLoop(snap({ second: 'failed' }), null)
  assert.deepEqual(M.vals({ data: failedData }).outcomes.map((o) => o.out)[0], '2 failed, 3 passed', 'a failed run on this version is drawn with its own tally')
  const failed = text({ data: failedData })
  assert.ok(!failed.includes('5 passed') && !failed.includes('1 failed, 4'), 'the canvas’s example tallies never reach the live screen')
})

test('the live path from a served thread: real content, no design notes, no engine words, no trial, no hole', () => {
  for (const second of [undefined, 'succeeded', 'failed']) {
    const data = selectRepairLoop(snap({ second }), null)
    const out = text({ data })
    for (const a of DESIGN_NOTES) assert.ok(!out.includes(a), `${second}: ${a}`)
    assert.ok(!ENGINE_WORDS.test(out), `${second}: engine words on a Plain surface: ${out.match(ENGINE_WORDS)}`)
    assert.ok(!/trial/i.test(out), `${second}: trial`)
    assert.ok(!/\b(undefined|null|NaN)\b/.test(out), `${second}: a hole reached the screen: ${out.match(/.{40}\b(undefined|null|NaN)\b.{20}/)}`)
    assert.ok(out.includes('ledger-api') && out.includes('Stop the import double-posting'), 'the record’s own names')
    for (const canvas of ['checkout-web', 'e71c5b0', '9f2ad3e', 'checkout/api.py', '1.4 GB']) assert.ok(!out.includes(canvas), `${second}: canvas value ${canvas}`)
  }
})

test('untracked: Check it again and Stop this check render off, with their reason in plain words', () => {
  const data = selectRepairLoop(snap(), null)
  const h = html({ data })
  assert.ok(/aria-disabled="true"[^>]*>[^<]*<span[^>]*>▶/.test(h) || h.includes('aria-disabled="true"'), 'the blocked Check it again is off')
  /* C30 P0-3: the reason is the real one — never the canvas's generic "not something this version can start" */
  assert.ok(text({ data }).includes('This work is not tracked as a Change yet, so a check has nothing to belong to.'))
  assert.ok(!text({ data }).includes('not something this version'))
  const v = M.vals({ data, actions: { checkAgain: () => { throw new Error('must not write') } } })
  v.againCheck() // off: reaches nothing
  v.stopCheck()
})

test('tracked: the run sheet’s permission and check flow to the one start act', () => {
  const sheet = { project_folder: '/r/ledger-api', checks: [{ id: 'chk_tests', command: 'pytest -q', program: 'pytest', category: 'tests', last: null }], environments: [], permission: null, actor: 'sam' }
  const data = selectRepairLoop(snap(), { id: 'chg_1', runSheet: sheet })
  assert.deepEqual(data.act, { changeId: 'chg_1', canStart: true, needsPermission: true, checkId: 'chk_tests', command: 'pytest -q', runKey: null, running: null, actor: 'sam', sheetRead: true, blocked: null, stopBlocked: null, lastWords: null, refusal: null })
  let called = 0
  const v = M.vals({ data, actions: { checkAgain: () => { called += 1 } } })
  assert.equal(v.againDisabled, undefined)
  v.againCheck()
  assert.equal(called, 1)
})

function stub(answers) {
  const calls = []
  const f = async (url, init = {}) => {
    calls.push({ url, method: init.method || 'GET', headers: init.headers || {}, body: init.body ? JSON.parse(init.body) : null })
    const a = answers[url] || { status: 200, doc: {} }
    return { ok: a.status < 400, status: a.status, json: async () => a.doc }
  }
  return { f, calls }
}

test('Check it again: token, then permission once, then the start — each with this launch’s token', async () => {
  const { f, calls } = stub({ [RUN_TOKEN]: { status: 200, doc: { token: 'tok-1' } } })
  const r = await checkAgain({ changeId: 'chg/1', checkId: 'chk_tests', needsPermission: true }, f)
  assert.equal(r.ok, true)
  assert.deepEqual(calls.map((c) => c.method + ' ' + c.url), [
    'GET /api/run/token', 'POST /api/changes/chg%2F1/run/permission', 'GET /api/run/token', 'POST /api/changes/chg%2F1/run',
  ])
  assert.equal(calls[1].headers['X-ThruLine-Run-Token'], 'tok-1')
  assert.deepEqual(calls[3].body, { check: 'chk_tests' })
  assert.equal(RUN_START('a'), '/api/changes/a/run')
  assert.equal(RUN_PERMISSION('a'), '/api/changes/a/run/permission')
})

test('Check it again: a refused permission starts nothing; no token, nothing posted', async () => {
  const refused = stub({ [RUN_TOKEN]: { status: 200, doc: { token: 't' } }, '/api/changes/c/run/permission': { status: 403, doc: { error: 'same_origin_required' } } })
  const r = await checkAgain({ changeId: 'c', checkId: 'k', needsPermission: true }, refused.f)
  assert.equal(r.ok, false)
  assert.ok(!refused.calls.some((c) => c.url === '/api/changes/c/run'))
  const tokenless = stub({ [RUN_TOKEN]: { status: 403, doc: { error: 'run_token_required' } } })
  const t = await checkAgain({ changeId: 'c', checkId: 'k' }, tokenless.f)
  assert.equal(t.ok, false)
  assert.deepEqual(tokenless.calls.map((c) => c.method), ['GET'])
  const none = stub({})
  assert.equal((await checkAgain({ changeId: null, checkId: 'k' }, none.f)).ok, false)
  assert.equal(none.calls.length, 0)
})

test('Stop this check: write address 3 with the actor; nothing without a run', async () => {
  const { f, calls } = stub({})
  await stopCheck({ changeId: 'chg_1', runKey: 'run 7', actor: 'sam' }, f)
  assert.deepEqual(calls.map((c) => c.method + ' ' + c.url), ['POST /api/changes/chg_1/runs/run%207/cancel'])
  assert.deepEqual(calls[0].body, { actor: 'sam' })
  assert.equal(RUN_CANCEL('a', 'b'), '/api/changes/a/runs/b/cancel')
  const none = stub({})
  await stopCheck({ changeId: 'chg_1', runKey: null }, none.f)
  assert.equal(none.calls.length, 0)
})

test('census: in this folder only actions.js writes, and it reaches exactly three addresses', () => {
  const files = readdirSync(here).filter((n) => /\.(js|jsx)$/.test(n) && !n.endsWith('.test.js'))
  for (const n of files) {
    const src = readFileSync(join(here, n), 'utf8')
    if (n === 'actions.js') continue
    assert.ok(!/method:\s*'POST'|method:\s*"POST"/.test(src), `${n} must not POST`)
  }
  const a = readFileSync(join(here, 'actions.js'), 'utf8')
  const addresses = [...a.matchAll(/'\/api\/changes\/' \+ encodeURIComponent\(id\) \+ '([^']*)'(?: \+ encodeURIComponent\(run\) \+ '([^']*)')?/g)].map((m) => m[1] + (m[2] ? '<run>' + m[2] : ''))
  assert.deepEqual(addresses.sort(), ['/run', '/run/permission', '/runs/<run>/cancel'])
})

/* Gate-fix: T10 (t10.4) — the session behind is one instant, when it started;
 * no clock-time range is composed in the gallery. */
test('the session behind is when it started, never a composed clock-time range', () => {
  const out = text()
  assert.ok(out.includes('today 16:01'), 'the start instant is drawn')
  assert.ok(!/\d{2}:\d{2}\s*[–-]\s*\d{2}:\d{2}/.test(out), 'a clock-time range is drawn')
})

/* ---- C26 b: "Track this work as a Change" through the first-use write, with the thread on screen ---- */
import { trackWork, trackThenReread, notConfirmedSentence, TRACK_WORK } from './actions.js'
import { trackOf, TRACK_WHY, TRACK_BUSY, TRACKED_NEXT_WHY } from './track.js'

const WHERE = { project: 'dfbfd6089e4bb388', thread: '106426d526acd84c' }
const WRITTEN = { schema_version: 3, change: 'chg_new', already_tracked: false, thread: WHERE.thread, title: 'Stop the import double-posting', confirmed: 1, confirmed_sessions: ['s1'], not_confirmed: [], suggested: 0, basis: 'user', opened_at: '2026-10-07T10:00:00Z' }

test('track: the exact body names the project and the thread on screen, in exactly one POST and nothing else', async () => {
  const { f, calls } = stub({ [TRACK_WORK]: { status: 200, doc: WRITTEN } })
  const r = await trackWork(WHERE, f)
  assert.deepEqual(r, { ok: true, change: 'chg_new', already: false, note: null })
  assert.deepEqual(calls.map((c) => c.method + ' ' + c.url), ['POST /api/first-use/change'])
  assert.deepEqual(calls[0].body, { project: 'dfbfd6089e4bb388', thread: '106426d526acd84c' })
})

test('track: after it is written the record is re-read once; nothing is opened', async () => {
  const { f } = stub({ [TRACK_WORK]: { status: 200, doc: WRITTEN } })
  const did = []
  await trackThenReread(WHERE, { reread: () => did.push('reread'), open: (id) => did.push('open ' + id) }, f)
  assert.deepEqual(did, ['reread'])
})

test('track: already tracked writes nothing new and opens the Change that tracks this work', async () => {
  const { f, calls } = stub({ [TRACK_WORK]: { status: 200, doc: { schema_version: 3, change: 'chg_old', already_tracked: true, thread: WHERE.thread } } })
  const did = []
  const r = await trackThenReread(WHERE, { reread: () => did.push('reread'), open: (id) => did.push('open ' + id) }, f)
  assert.deepEqual(r, { ok: true, change: 'chg_old', already: true, note: null })
  assert.deepEqual(did, ['open chg_old'])
  assert.equal(calls.length, 1)
})

test('track: sessions in the other recording project are said in plain words, never as codes', async () => {
  const { f } = stub({ [TRACK_WORK]: { status: 200, doc: { ...WRITTEN, not_confirmed: [{ session: 'session_x', ledger_project_id: 'project_y', reason: 'other_recording_project' }] } } })
  const r = await trackWork(WHERE, f)
  assert.equal(r.note, 'One session of this work was recorded under another recording project, and a Change can hold only the sessions of its own project, so it is not linked to this Change.')
  assert.ok(!/session_x|project_y|other_recording_project/.test(r.note))
  assert.match(notConfirmedSentence([{}, {}]), /^2 sessions of this work were .* so they are not linked/)
  assert.equal(notConfirmedSentence([]), null)
  const why = trackOf({ untracked: false, tracked: true, canStart: false, note: r.note }).why
  assert.equal(why, TRACKED_NEXT_WHY + ' ' + r.note)
})

test('track: every refusal is said in words, with no code on the Plain surface, and nothing is re-read', async () => {
  const cases = [
    [{ status: 404, doc: { error: 'no_such_thread', detail: 'no thread with this id in this project' } }, /no longer in the record/],
    [{ status: 400, doc: { error: 'refused', detail: 'x' } }, /could not track this work, so nothing was written/],
    [{ status: 503, doc: { error: 'store_busy', message: 'Another ThruLine process is writing to the record.' } }, /^Another ThruLine process is writing to the record\. Nothing was written\.$/],
  ]
  for (const [answer, words] of cases) {
    const { f } = stub({ [TRACK_WORK]: answer })
    let reread = 0
    const r = await trackThenReread(WHERE, { reread: () => { reread += 1 } }, f)
    assert.equal(r.ok, false)
    assert.match(r.said, words)
    assert.ok(!/no_such_thread|refused:|store_busy|\b40\d\b|\b503\b/.test(r.said), r.said)
    assert.equal(reread, 0)
  }
  const down = async () => { throw new Error('down') }
  assert.match((await trackWork(WHERE, down)).said, /did not answer, so nothing was written/)
  const none = stub({})
  assert.equal((await trackWork({ project: 'p', thread: null }, none.f)).ok, false)
  assert.equal(none.calls.length, 0)
})

test('track: an untracked thread is always offered the one primary act, with what it records and does not', () => {
  const on = trackOf({ untracked: true })
  assert.deepEqual(on, { offered: true, busy: false, why: TRACK_WHY, next: 'Then say what counts as done, and Check it again runs here.' })
  assert.match(TRACK_WHY, /approves nothing and sends nothing to the agent/)
  assert.equal(trackOf({ untracked: true, busy: true }).why, TRACK_BUSY)
  assert.equal(trackOf({ untracked: true, said: 'x' }).why, 'x')
  const data = { ...selectRepairLoop(snap(), null), track: on }
  const h = html({ data })
  assert.ok(text({ data }).includes('Track this work as a Change'))
  assert.equal((h.match(/class="tl-btn tl-f"/g) || []).length, 1, 'one primary: Copy message steps back to the secondary style')
  let n = 0
  M.vals({ data, actions: { track: () => { n += 1 } } }).track.act()
  assert.equal(n, 1)
})

test('census (C26): actions.js reaches exactly these addresses, and no proposal read is left', () => {
  const a = readFileSync(join(here, 'actions.js'), 'utf8')
  const posts = [...a.matchAll(/export const (\w+) = [^\n]*'(\/api\/[^']*)'/g)].map((m) => m[2])
  assert.deepEqual(posts.sort(), ['/api/changes/', '/api/changes/', '/api/changes/', '/api/first-use/change', '/api/run/token'].sort())
  assert.ok(!/first-use\/proposal/.test(a + readFileSync(join(here, 'index.jsx'), 'utf8')))
})
test('C26.4: the "Tell the agent" title never wraps; the "about ❝…❞" part is what gives way, whole in its title', () => {
  const long = { ...selectRepairLoop(snap(), null) }
  long.statement = { ...long.statement, text: 'The next guarded suite, predeclaration, passed five checks before ownership inspection for PID 73269 became unknown.' }
  const h = html({ data: long })
  assert.match(h, /<h2 id="tl-tell-title" style="[^"]*flex:0 0 auto;[^"]*white-space:nowrap/)
  assert.match(h, /<span title="about ❝The next guarded suite[^"]*❞ step \d+" style="[^"]*min-width:0;overflow:hidden;text-overflow:ellipsis/)
})
