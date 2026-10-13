/* W2-B: the one Change model. Synthetic ThreadViews in the served shape
 * (docs/hq-next/API.md) — never the founder's own record, which stays out of
 * git. Each test names the rule of DESIGN-SPEC §4.0 it pins. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { changeSnapshot, withRunSheet, shortVersion, hhmm } from './change.js'
import { DEMO } from '../screens/Change-Brief/demo.js'

const T = (m) => new Date(Date.UTC(2026, 9, 6, 12, m, 0)).toISOString()
const S1 = 'session_aaa'

/** A ThreadView with one session; rows given in time order. */
function tv({ edits = [], commands = [], claims = [], prompts, versions, current = 'wt-1111111111111111', sessions, proof } = {}) {
  return {
    schema_version: 1,
    project: { id: 'p1', name: 'proj', root: '/r/proj' },
    thread: { id: '0123456789abcdef', title: 'Fix the total', current_version_id: current, whose_move: 'you', proof: proof || { last_test: null, last_passing_test_at: null, edited_after_last_pass: [] } },
    prompts: prompts || [{ event_id: 'ev-p', session_id: S1, seq: 1, ts: T(0), text: 'fix the total please', work: true, version_id: current, raw_file: '/m/a', raw_line: 1 }],
    edits: edits.map((e, i) => ({ id: 'ed' + i, event_id: 'ed' + i, session_id: S1, ts: T(e.m), file: '/r/proj/' + e.file, op: 'edit', lines_added: 1, lines_removed: 0, ok: true, version_id: e.v || current, raw_file: '/m/a', raw_line: 10 + i })),
    commands: commands.map((c, i) => ({ id: c.id || 'c' + i + '#0', event_id: 'c' + i, part: 0, session_id: S1, ts: T(c.m), command: c.cmd || 'pytest -q', outcome: c.outcome || 'succeeded', exit_code: c.exit ?? null, is_test: c.test !== false, is_commit: false, tally: c.tally === undefined ? { passed: 4, failed: 0, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 } : c.tally, version_id: c.v || current, raw_file: '/m/a', raw_line: 20 + i })),
    claims: claims.map((c, i) => ({ id: c.id || 'cl' + i + '#0', event_id: c.event || 'cl' + i, session_id: S1, ts: T(c.m), text: c.text || 'Tests pass.', kind: c.kind || 'tests_pass', has_verifier: c.verifier !== false, rests_on_command_id: c.rests ?? null, followed_by_passing_test: false, version_id: c.v || current, raw_file: '/m/a', raw_line: 30 + i })),
    versions: versions || [{ id: current, commit: null, short: null, session_id: S1, from_seq: 1, to_seq: 99, committed: false }],
    current_version_id: current,
    sessions: sessions || [{ id: S1, ledger_project_id: 'lp1', source: 'claude_code', origin: 'person', started_at: T(0), last_at: T(50), file: '/m/a' }],
    files: [],
    masking: { masked: 0, families: [], complete: true },
  }
}
const snap = (v, tracked = null, o = {}) => changeSnapshot(v, tracked, new Date(Date.UTC(2026, 9, 6, 13, 0)), o)
const stmt = (s, i = 0) => s.statements[i]

test('an exclusion marker survives the Change snapshot and quoted-statement display verbatim', () => {
  const mark = 'left out by your exclusion (`src/a.rs`)'
  const v = tv({ edits: [{ m: 1, file: 'src/a.rs' }], claims: [{ m: 2, text: mark }] })
  v.edits[0].file = mark // the server has already replaced this entry
  v.left_out_by_exclusion = 2
  const s = snap(v)
  assert.equal(s.edits[0].file, mark)
  assert.equal(s.statements[0].plain, mark)
  assert.equal(s.leftOutByExclusion, 2)
  assert.equal(plainText(mark), mark)
})

test('the snapshot has DEMO’s shape: every top-level field and every proof link', () => {
  const s = snap(tv({ commands: [{ m: 1 }], claims: [{ m: 2, rests: 'c0#0' }] }))
  const demoKeys = Object.keys(DEMO).filter((k) => k !== 'states')
  for (const k of demoKeys) assert.ok(k in s, 'missing ' + k)
  for (const k of Object.keys(DEMO.change)) assert.ok(k in s.change, 'change.' + k)
  for (const k of Object.keys(DEMO.view)) assert.ok(k in s.view, 'view.' + k)
  for (const k of Object.keys(DEMO.criteria)) assert.ok(k in s.criteria, 'criteria.' + k)
  for (const k of ['verdict', 'version', 'scope', 'stopsAt', 'rule', 'restsOn', 'ranOn', 'since', 'recheck', 'lookedAt', 'earlier', 'thenAndNow', 'followUp']) assert.ok(k in stmt(s).proof, 'proof.' + k)
  for (const k of Object.keys(DEMO.runs[0])) assert.ok(k in s.runs[0], 'runs[].' + k)
  for (const k of Object.keys(DEMO.edits[0])) assert.ok(k in snap(tv({ edits: [{ m: 1, file: 'a.py' }] })).edits[0], 'edits[].' + k)
})

test('steps are each row’s place in the thread’s record, in time order', () => {
  const s = snap(tv({ edits: [{ m: 3, file: 'a.py' }], commands: [{ m: 1 }], claims: [{ m: 5, rests: 'c0#0' }] }))
  assert.equal(s.request.step, 1)
  assert.equal(s.runs[0].step, 2)
  assert.equal(s.edits[0].step, 3)
  assert.equal(stmt(s).step, 4)
  assert.equal(s.change.stepCount, 4)
})

test('② rests on the named run; ④ the first edit after it, scope unknown so any edit counts; stops at ⑤', () => {
  const s = snap(tv({ commands: [{ m: 1 }], edits: [{ m: 2, file: 'api.py' }], claims: [{ m: 3, rests: 'c0#0' }] }))
  const p = stmt(s).proof
  assert.equal(p.verdict, 'not_proven')
  assert.equal(p.restsOn.runId, 'c0#0')
  assert.equal(p.restsOn.basis, 'latest_before')
  assert.equal(p.since.editId, 'ed0')
  assert.equal(p.since.dependsOnWholeChange, true, 'the run states no scope, and the snapshot must say so')
  assert.equal(p.stopsAt, 5)
  assert.equal(s.change.state, 'not_proven')
})

test('a passing run on the current version with nothing after it: proven at ⑥', () => {
  const p = stmt(snap(tv({ commands: [{ m: 1 }], claims: [{ m: 2, rests: 'c0#0' }] }))).proof
  assert.equal(p.verdict, 'proven')
  assert.equal(p.since, null)
  assert.equal(p.recheck.status, 'not_needed')
})

test('⑤ a passing test run on the current version after the last edit settles it', () => {
  const p = stmt(snap(tv({ commands: [{ m: 1 }, { m: 4, cmd: 'pytest -q tests/' }], edits: [{ m: 2, file: 'api.py' }], claims: [{ m: 3, rests: 'c0#0' }] }))).proof
  assert.equal(p.verdict, 'proven')
  assert.equal(p.restsOn.runId, 'c1#0')
  assert.equal(p.restsOn.basis, 'later_run')
})

test('a later failing test run contradicts the claim (✕ at ⑥)', () => {
  const p = stmt(snap(tv({ commands: [{ m: 1 }, { m: 4, outcome: 'failed', tally: { passed: 3, failed: 1 } }], claims: [{ m: 2, rests: 'c0#0' }] }))).proof
  assert.equal(p.verdict, 'contradicted')
  assert.equal(p.recheck.status, 'failed')
  assert.equal(p.recheck.runId, 'c1#0')
})

test('a run with no result never counts as a link: stops at ②', () => {
  const p = stmt(snap(tv({ commands: [{ m: 1, outcome: 'unknown', tally: null }], claims: [{ m: 2, rests: 'c0#0' }] }))).proof
  assert.equal(p.verdict, 'not_proven')
  assert.equal(p.stopsAt, 2)
})

test('no test run before it: claimed, and no proof found — stops at ②', () => {
  const p = stmt(snap(tv({ claims: [{ m: 2 }] }))).proof
  assert.equal(p.verdict, 'not_proven')
  assert.equal(p.stopsAt, 2)
  assert.equal(p.restsOn.runId, null)
})

test('a claim with no verifier rests on what counts as done and stops at ② until it is accepted', () => {
  const v = tv({ claims: [{ m: 2, kind: 'done', verifier: false, text: 'Done.' }] })
  const open = stmt(snap(v)).proof
  assert.equal(open.verdict, 'recorded')
  assert.deepEqual(open.restsOn, { kind: 'criteria' })
  assert.equal(open.stopsAt, 2)
  const tracked = { change: { id: 'chg_1', accepted_scope: { version: 1, criteria: ['a', 'b'], acceptance: { accepted_at: T(1) } } }, hq: { title: 'T', criteria: [{}, {}], counts: { proven: 1 }, version: { short: 'abc1234' }, readiness: { ready: false }, decision: {} }, proposals: null }
  assert.equal(stmt(snap(v, tracked)).proof.stopsAt, null)
})

test('a count the record does not hold is absent, never 0', () => {
  const s = snap(tv({ commands: [{ m: 1, tally: null }], claims: [{ m: 2, rests: 'c0#0' }] }))
  assert.equal(s.runs[0].passed, null)
  assert.equal(s.runs[0].failed, null)
  assert.equal(s.runs[0].durationMs, null)
  assert.equal(s.machine.recordsBytes, null)
  assert.equal(s.criteria.lineCount, null)
  assert.equal(s.criteria.proposedCount, null)
})

test('without a tracked Change, what counts as done is not accepted; with one, it comes from the tracked model', () => {
  const v = tv({ commands: [{ m: 1 }], claims: [{ m: 2, rests: 'c0#0' }] })
  assert.equal(snap(v).criteria.accepted, false)
  const tracked = { change: { id: 'chg_9', accepted_scope: { version: 2, criteria: ['x'], acceptance: { accepted_at: T(1) } } }, hq: { title: 'Tracked title', criteria: [{}, {}, {}], counts: { proven: 3 }, version: { short: 'abc1234' }, readiness: { ready: true }, decision: { approvedFull: null } }, proposals: { criteria: [1, 2] } }
  const s = snap(v, tracked)
  assert.equal(s.criteria.accepted, true)
  assert.equal(s.criteria.version, 'v2')
  assert.equal(s.criteria.lineCount, 3, 'lines come from the derived model, not re-counted')
  assert.equal(s.criteria.provenOn.count, 3)
  assert.equal(s.criteria.proposedCount, 2)
  assert.equal(s.change.state, 'ready')
  assert.equal(s.change.id, 'chg_9')
  assert.equal(s.change.title, 'Tracked title')
  assert.equal(s.primaryAction, 'approve')
})

test('versions read short; the current one is drawn last', () => {
  assert.equal(shortVersion({ id: '3f9a1c2' }), '3f9a1c2')
  assert.equal(shortVersion({ id: 'wt-00bb958a3c8b2ae7' }), 'wt-00bb958')
  const v = tv({ current: 'wt-2222222222222222', versions: [{ id: 'wt-2222222222222222', committed: false }, { id: 'abcdef1', short: 'abcdef1', committed: true }] })
  const s = snap(v)
  assert.deepEqual(s.versions.map((x) => x.id), ['abcdef1', 'wt-2222222'])
  assert.equal(s.currentVersion, 'wt-2222222')
  assert.equal(s.versions[0].commit, 'abcdef1')
  assert.equal(s.versions[1].unsavedChanges, true)
})

test('the code moving after the first read is “needs a re-check”, naming the files changed since', () => {
  const v = tv({ edits: [{ m: 30, file: 'late.py' }], commands: [{ m: 1 }], claims: [{ m: 2, rests: 'c0#0' }] })
  const s = snap(v, null, { readVersion: 'wt-0000000', loadedAt: T(10) })
  assert.equal(s.change.state, 'needs_recheck')
  assert.equal(s.primaryAction, 'check_what_changed')
  assert.deepEqual(s.view.changedSinceLoad, ['late.py'])
})

test('C54 brief#23: a version move names its recorded file even when a late-loaded edit has an earlier timestamp', () => {
  const v = tv({
    current: 'wt-2222222222222222',
    versions: [{ id: 'wt-1111111111111111', committed: false }, { id: 'wt-2222222222222222', committed: false }],
    edits: [{ m: 15, file: 'late.py', v: 'wt-2222222222222222' }],
    commands: [{ m: 1, v: 'wt-1111111111111111' }],
    claims: [{ m: 2, rests: 'c0#0', v: 'wt-1111111111111111' }],
  })
  const s = snap(v, null, { readVersion: 'wt-1111111', loadedAt: T(20) })
  assert.equal(s.change.state, 'needs_recheck')
  assert.deepEqual(s.view.changedSinceLoad, ['late.py'])
})

test('C54 brief#3: a check started on moved code changes the primary act to Stop until it ends', () => {
  const v = tv({
    current: 'wt-2222222222222222',
    versions: [{ id: 'wt-1111111111111111', committed: false }, { id: 'wt-2222222222222222', committed: false }],
    edits: [{ m: 15, file: 'late.py', v: 'wt-2222222222222222' }],
    commands: [{ m: 1, v: 'wt-1111111111111111' }],
    claims: [{ m: 2, rests: 'c0#0', v: 'wt-1111111111111111' }],
  })
  const moved = snap(v, null, { readVersion: 'wt-1111111', loadedAt: T(10) })
  assert.equal(moved.primaryAction, 'check_what_changed')
  const served = { checks: [{ command: 'pytest -q', last: { run: 'local-1', outcome: 'running', started_at: T(31) } }],
    job: { for_this_change: true, run: 'local-1', phase: 'running', started_at: T(31) } }
  const active = withRunSheet(moved, served, new Date(T(32)))
  assert.equal(active.change.state, 'needs_recheck')
  assert.equal(active.primaryAction, 'stop_check')
  assert.equal(active.runs.at(-1).status, 'running')
})

test('the brief is about the latest checkable statement that is not proven', () => {
  const s = snap(tv({ commands: [{ m: 1 }], edits: [{ m: 3, file: 'a.py' }], claims: [{ m: 2, rests: 'c0#0', id: 'old#0' }, { m: 4, rests: 'c0#0', id: 'new#0' }, { m: 5, verifier: false, kind: 'done', id: 'done#0' }] }))
  assert.equal(s.brief.statementId, 'new#0')
})

test('times are this machine’s local HH:MM', () => {
  const d = new Date(T(7))
  assert.equal(hhmm(T(7)), String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0'))
  assert.equal(hhmm(null), null)
})

test('pure: no fetch, no clock but the one given', () => {
  const real = globalThis.fetch
  globalThis.fetch = () => { throw new Error('change.js fetched') }
  try {
    const a = snap(tv({ commands: [{ m: 1 }], claims: [{ m: 2, rests: 'c0#0' }] }))
    const b = snap(tv({ commands: [{ m: 1 }], claims: [{ m: 2, rests: 'c0#0' }] }))
    assert.deepEqual(a, b)
  } finally { globalThis.fetch = real }
})

test('added W2-B: edits carry hunks and source as served; worktree paths are placed under their member root', () => {
  const v = tv({ edits: [{ m: 1, file: 'a.py' }] })
  v.edits.push({ id: 'edw', event_id: 'edw', session_id: S1, ts: T(2), file: '/wt/thruline-cloud-x/crates/core/src/lib.rs', op: 'patch', hunks: [{ old_start: null, lines: [{ kind: 'added', text: 'x' }] }], source: 'apply_patch', version_id: 'wt-1111111111111111' })
  v.edits.push({ id: 'edc', event_id: 'edc', session_id: S1, ts: T(3), file: '/r/proj/.claude/worktrees/agent-1/src/b.py', op: 'edit', hunks: [], source: 'structured_patch', version_id: 'wt-1111111111111111' })
  v.edits.push({ id: 'edo', event_id: 'edo', session_id: S1, ts: T(4), file: '/elsewhere/c.py', op: 'edit', hunks: [], source: 'request', version_id: 'wt-1111111111111111' })
  const s = snap(v, null, { members: ['/r/proj', '/wt/thruline-cloud-x'] })
  const by = (id) => s.edits.find((e) => e.id === id)
  assert.deepEqual([by('ed0').file, by('ed0').worktree], ['a.py', null])
  assert.equal(by('ed0').editorPath, '/r/proj/a.py')
  assert.deepEqual([by('edw').file, by('edw').worktree, by('edw').source], ['crates/core/src/lib.rs', 'thruline-cloud-x', 'apply_patch'])
  assert.equal(by('edw').editorPath, '/wt/thruline-cloud-x/crates/core/src/lib.rs')
  assert.equal(by('edw').hunks[0].lines[0].text, 'x')
  assert.deepEqual([by('edc').file, by('edc').worktree], ['src/b.py', 'agent-1'])
  assert.deepEqual([by('edo').file, by('edo').worktree], ['/elsewhere/c.py', null], 'a path under no root is served as recorded')
  assert.equal(by('edo').editorPath, null, 'another checkout is not an editor target')
})

test('the server serves edit paths relative to the project root: editorPath resolves them against the recorded root, and only against it', () => {
  const v = tv({ edits: [{ m: 1, file: 'a.py' }] })
  const mk = (id, file) => v.edits.push({ id, event_id: id, session_id: S1, ts: T(2), file, op: 'edit', hunks: [], source: 'request', version_id: 'wt-1111111111111111' })
  mk('rel', 'src/deep/b.py')
  mk('esc', '../outside/c.py')
  mk('esc2', 'src/../../outside/c.py')
  mk('abs', '/r/proj/src/d.py')
  mk('absout', '/elsewhere/e.py')
  mk('home', '~/Developer/x/f.py')
  mk('dot', './g.py')
  mk('mark', 'left out by your exclusion (`src/a.rs`)')
  mk('nul', 'src/bad\0.py')
  const by = (s, id) => s.edits.find((e) => e.id === id)
  const s = snap(v)
  assert.equal(by(s, 'rel').editorPath, '/r/proj/src/deep/b.py', 'a relative file is joined to the recorded root')
  assert.equal(by(s, 'rel').file, 'src/deep/b.py', 'the displayed path is unchanged')
  assert.equal(by(s, 'esc').editorPath, null, 'a .. escape out of the root is no editor target')
  assert.equal(by(s, 'esc2').editorPath, null, 'a .. escape in the middle is no editor target')
  assert.equal(by(s, 'abs').editorPath, '/r/proj/src/d.py', 'an absolute path under the root is accepted as before')
  assert.equal(by(s, 'absout').editorPath, null, 'an absolute path under no root stays refused')
  assert.equal(by(s, 'home').editorPath, null, 'a ~ path is not invented into a root-relative one')
  assert.equal(by(s, 'dot').editorPath, null, 'a . segment stays refused')
  assert.equal(by(s, 'mark').editorPath, null, 'an exclusion marker is never a path')
  assert.equal(by(s, 'nul').editorPath, null, 'a control character stays refused')
  // a trailing slash on the root does not double up
  const t = snap(Object.assign({}, v, { project: Object.assign({}, v.project, { root: '/r/proj/' }) }))
  assert.equal(by(t, 'rel').editorPath, '/r/proj/src/deep/b.py')
  // no recorded root, no resolution
  const u = snap(Object.assign({}, v, { project: { id: 'p1', name: 'proj' } }))
  assert.equal(by(u, 'rel').editorPath, null, 'without a recorded root a relative file is not a path')
  // the absolute path never reaches text a person reads
  assert.ok(!JSON.stringify(Object.assign({}, by(s, 'rel'), { editorPath: undefined })).includes('/r/proj'), 'the root appears only in editorPath')
})

test('added W2-B: a test run’s scope is the paths its command names, else null', () => {
  const s = snap(tv({ commands: [{ m: 1, cmd: 'cd ui && node --test src/a.test.js src/b.test.js 2>&1 | tail -3' }, { m: 2, cmd: 'cargo test -p thruline-core' }] }))
  assert.deepEqual(s.runs[0].scope, ['src/a.test.js', 'src/b.test.js'])
  assert.equal(s.runs[1].scope, null)
})

test('added W2-B: statements[].plain strips markdown markers; text stays verbatim', () => {
  const s = snap(tv({ commands: [{ m: 1 }], claims: [{ m: 2, rests: 'c0#0', text: 'Tests pass **52/52**, `cargo test` and __all__ green.' }] }))
  assert.equal(stmt(s).text, 'Tests pass **52/52**, `cargo test` and __all__ green.')
  assert.equal(stmt(s).plain, 'Tests pass 52/52, cargo test and all green.')
})

test('added W2-B: recording is true only when the thread’s last row is recent; otherwise not known', () => {
  const v = tv({})
  assert.equal(snap(v).change.recording, null)
  v.thread.whose_move = 'agent'
  assert.equal(snap(v).change.recording, true)
})

/* ---- polish: the engine is the one verdict rule, the tally counts as a failure,
 *      quotes read as words, and a prompt's wrapper tags never show ---- */
import { plainText, unwrapPasted, promptText } from './change.js'

test('polish: a run that exited 0 while its tally names a failure never proves a claim', () => {
  // "3 passed; 1 failed", exit 0, outcome succeeded: the run the claim rests on
  const v = tv({ commands: [{ m: 1, outcome: 'succeeded', exit: 0, tally: { passed: 3, failed: 1, skipped: 0, errors: 0 } }], claims: [{ m: 2, rests: 'c0#0' }] })
  const p = stmt(snap(v)).proof
  assert.notEqual(p.verdict, 'proven')
  assert.equal(p.verdict, 'contradicted')
  // and as a later run after the claim
  const w = tv({ commands: [{ m: 1 }, { m: 3, outcome: 'succeeded', exit: 0, tally: { passed: 3, failed: 1 } }], claims: [{ m: 2, rests: 'c0#0' }] })
  assert.equal(stmt(snap(w)).proof.verdict, 'contradicted')
})

test('polish: the served verdict is the snapshot’s verdict, claim by claim (engine parity)', () => {
  const v = tv({
    commands: [{ m: 1 }, { m: 5, tally: { passed: 2, failed: 0 } }],
    edits: [{ m: 3, file: 'a.py' }],
    claims: [{ m: 2, rests: 'c0#0' }, { m: 4, rests: 'c0#0' }, { m: 6, rests: 'c1#0' }, { m: 7, kind: 'done', verifier: false }],
  })
  const served = [['not_proven', 'edited_since_run'], ['contradicted', 'later_run_failed'], ['proven', 'run_holds'], ['recorded', 'no_verifier']]
  v.claims.forEach((c, i) => { c.verdict = served[i][0]; c.verdict_basis = served[i][1] })
  const s = snap(v)
  for (const [i, [verdict, basis]] of served.entries()) {
    const st = s.statements.find((x) => x.id === v.claims[i].id)
    assert.equal(st.proof.verdict, verdict, 'claim ' + i)
    assert.equal(st.proof.basis, basis)
  }
  // a served verdict the local rule disagrees with still wins, and the chain agrees with it
  assert.equal(s.statements.find((x) => x.id === v.claims[1].id).proof.stopsAt, null)
  // an older server (no verdict) falls back to the corrected local rule
  const old = tv({ commands: [{ m: 1, tally: { passed: 3, failed: 1 } }], claims: [{ m: 2, rests: 'c0#0' }] })
  assert.equal(stmt(snap(old)).proof.verdict, 'contradicted')
})

test('polish: plain reads markdown links as their words and drops heading and list markers; text stays verbatim', () => {
  assert.equal(plainText('See [the spec](https://x.y/z) and **bold** `code`'), 'See the spec and bold code')
  assert.equal(plainText('## Summary\n- 20 passed\n* 0 failed\n3. done'), 'Summary\n20 passed\n0 failed\ndone')
  assert.equal(plainText('1575 passed, 0 failed.'), '1575 passed, 0 failed.')
  const v = tv({ commands: [{ m: 1 }], claims: [{ m: 2, rests: 'c0#0', text: '**`cd ui && npm test`:** [1575 passed](http://l/x), 0 failed.' }] })
  const st = stmt(snap(v))
  assert.equal(st.text, '**`cd ui && npm test`:** [1575 passed](http://l/x), 0 failed.')
  assert.doesNotMatch(st.plain, /\*\*|`|\]\(/)
  assert.equal(snap(v).messages[0].plain, st.plain, 'a message’s words are its statements’ plain words')
})

test('polish: a prompt’s <pasted_content> wrapper tags never reach the screen; served segments win', () => {
  const raw = '<pasted_content id="7fa1">\nThruLine cloud coder lane — read this first\n</pasted_content> build it'
  assert.equal(unwrapPasted(raw), 'ThruLine cloud coder lane — read this first\nbuild it')
  const v = tv({ prompts: [{ event_id: 'ev-p', session_id: S1, seq: 1, ts: T(0), text: raw, work: true, raw_file: '/m/a', raw_line: 1 }] })
  assert.doesNotMatch(snap(v).request.text, /pasted_content/)
  assert.equal(snap(v).request.verbatim, raw)
  const seg = { text: raw, segments: [{ kind: 'typed', text: 'build it' }, { kind: 'pasted', text: 'ThruLine cloud coder lane' }] }
  assert.equal(promptText(seg), 'build it\n\nThruLine cloud coder lane')
  v.thread.title = '<pasted_content id="1"> Lane brief'
  assert.equal(snap(v).change.title, 'Lane brief')
})

test('C29: the session node counts 1,000 or more sessions with thousands separators', () => {
  const many = Array.from({ length: 1234 }, (_, i) => ({ id: 's' + i, ledger_project_id: 'lp1', source: 'claude_code', origin: 'person', started_at: T(0), last_at: T(50), file: '/m/a' }))
  const node = snap(tv({ sessions: many })).graph.nodes.find((n) => n.kind === 'session')
  assert.match(node.label, / · 1,234 sessions$/)
})
