/* W2-D, HQ Next wave 2: Approve-Sheet on the real record.
 *
 * The selector reads the ONE Change snapshot (data/change.js) — fed here both
 * the canvas's own story (Change-Brief/demo.js, every state) and a synthetic
 * served ThreadView pushed through `changeSnapshot` — the tracked Change's model,
 * and /api/licence (Free or Pro only, CTO C21-b). The writes are proven against a stubbed `fetch`; nothing here reaches
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
import { selectApproveSheet, planOf, decisionRows } from './from-change.js'
import { approveVersion, withdrawApproval, CHANGE_DECIDE, CHANGE_WITHDRAW } from './actions.js'

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
export function vals(props, state) { const inst = new Logic(props || {}); inst.props = props || {}; if (state) inst.state = { ...inst.state, ...state }; return inst.renderVals() }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'approve-sheet-live-')), 'screen.mjs')
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
const DESIGN_NOTES = ['Demo state', 'Illustrative', 'Start over', 'Code moves while open']
const PRO = { status: 'valid', plan: 'pro', expires_on: '7 Oct 2027', paid_gate: true, headline: 'licence', entitlements: { approve_version: { allowed: true } } }
const FREE = { status: 'absent', plan: null, expires_on: null, paid_gate: true, headline: 'free', entitlements: { approve_version: { allowed: false, reason: 'r', enable: 'e', status: 'absent' } } }

/* A tracked Change whose four lines the engine reads as proven on this version. */
function tracked({ ready = true, history = [] } = {}) {
  return {
    id: 'chg_1',
    change: { accepted_scope: { version: 1, acceptance: { actor: 'sam', accepted_at: T(5) } }, decisions: { history }, code_moved: null },
    model: {
      criteria: [1, 2].map((n) => ({ n, text: 'Line ' + n + ' holds.', state: ready ? 'proven' : (n === 1 ? 'proven' : 'needs_recheck'), word: ready ? 'Proven' : (n === 1 ? 'Proven' : 'Needs a re-check') })),
      decision: { seen: 'full-rev-abc', approvedFull: history.length ? 'full-rev-abc' : null },
      readiness: { ready, sentence: ready ? null : 'Line 2 needs a re-check on this version.', waivedLines: [] },
    },
  }
}

test('Free or Pro only: a valid licence that may approve is Pro; anything else read — trial fields included — is Free', () => {
  assert.equal(planOf(PRO), 'pro')
  assert.equal(planOf(FREE), 'free')
  /* C30 P0-6: the one plan rule (data/plan.js) — a licence not read is no plan, never Free */
  assert.equal(planOf(null), null)
  assert.equal(planOf({ status: 'expired', plan: 'pro', entitlements: { approve_version: { allowed: false } } }), 'free')
})

test('the design gallery keeps the canvas, with no trial anywhere (C20)', () => {
  const out = text()
  assert.ok(out.includes('Approving a version needs Pro. Everything else is free.'))
  assert.ok(!/trial/i.test(html()), 'trial in the gallery')
  assert.ok(!/trial/i.test(html(undefined, { plan: 'free' })), 'trial in the Free variant')
  assert.ok(html(undefined, { plan: 'free' }).includes('https://thrulinehq.com/pricing'))
})

test('live, Free: Approve is off with the C20 sentence and See plans → thrulinehq.com/pricing', () => {
  const data = selectApproveSheet(snap(), tracked(), { licence: FREE })
  const h = html({ data })
  const out = text({ data })
  assert.ok(out.includes('Approving a version needs Pro.'))
  assert.ok(out.includes('Everything else is free.'))
  assert.ok(h.includes('href="https://thrulinehq.com/pricing"'))
  assert.ok(!/trial/i.test(h))
  /* A name typed and every line proven: on Pro this Change is approvable, so on
   * Free the licence is the only thing that can keep Approve off. */
  const named = { name: 'Robin' }
  assert.equal(M.vals({ data: selectApproveSheet(snap(), tracked(), { licence: PRO }) }, named).approveEnabled, true, 'the fixture is not approvable on Pro')
  const v = M.vals({ data, actions: { approve: () => { throw new Error('must not write') } } }, named)
  assert.equal(v.approveEnabled, false, 'Free opened Approve')
  assert.equal(v.approveDisabled, true)
  assert.deepEqual([v.why.l1, v.why.l2], ['Approving a version needs Pro.', 'Everything else is free. Everything you recorded is still here.'])
  assert.ok(text({ data }, named).includes('Everything you recorded is still here.'), 'the licence words are not on the sheet')
  v.approve()
})

test('live, untracked: nothing counts as done, so Approve is off and says so', () => {
  const data = selectApproveSheet(snap(), null, { licence: PRO })
  const v = M.vals({ data })
  assert.equal(v.approveEnabled, false)
  assert.equal(v.why.l1, 'Nothing counts as done yet, so nothing is ready.')
  assert.equal(v.why.l2, 'Say what counts as done for this Change first.')
})

test('live, tracked but not ready: Approve is off with the engine’s own readiness sentence', () => {
  const data = selectApproveSheet(snap(), tracked({ ready: false }), { licence: PRO })
  const v = M.vals({ data }, { name: 'Robin' })
  assert.equal(v.approveEnabled, false)
  assert.equal(v.why.l2, 'Line 2 needs a re-check on this version.')
  assert.equal(v.pill.word, 'Not ready for approval')
  const out = text({ data })
  assert.ok(out.includes('Needs a re-check'), 'each line keeps its own state on the sheet')
})

test('live, Pro, ready, a name typed: Approve sends the name; then Withdraw sends it too', () => {
  const data = selectApproveSheet(snap(), tracked(), { licence: PRO })
  assert.equal(M.vals({ data }).name, '', 'the record holds no name: the box starts empty')
  assert.equal(M.vals({ data }).approveEnabled, false, 'an approval has to have a name on it')
  const sent = []
  const v = M.vals({ data, actions: { approve: (who) => sent.push(['approve', who]), withdraw: (who) => sent.push(['withdraw', who]) } }, { name: 'Robin' })
  assert.equal(v.approveEnabled, true)
  v.approve()
  const approved = selectApproveSheet(snap(), tracked({ history: [{ act: 'approved', actor: 'Robin', decided_at: T(9), target_revision: 'full-rev-abc' }] }), { licence: PRO })
  const w = M.vals({ data: approved, actions: { withdraw: (who) => sent.push(['withdraw', who]) } }, { name: 'Robin' })
  assert.equal(w.withdrawEnabled, true)
  w.withdraw()
  assert.deepEqual(sent, [['approve', 'Robin'], ['withdraw', 'Robin']])
})

test('Bar set after the work: the two MOMENTS are compared, not their clock faces — accepted at 00:10 after an edit at 23:50 the day before is after the work', () => {
  /* local-time constructors, so the two faces are 23:50 and 00:10 in every timezone */
  const edited = new Date(2026, 9, 6, 23, 50, 0).toISOString()
  const at = (acceptedAt) => {
    const t = thread()
    t.edits[0].ts = edited
    const tr = tracked()
    tr.change.accepted_scope.acceptance.accepted_at = acceptedAt
    const data = selectApproveSheet(changeSnapshot(t, null, new Date(2026, 9, 7, 1, 0), { storageBytes: 3 * 1024 ** 3 }), tr, { licence: PRO })
    return { data, v: M.vals({ data }, { name: 'Robin' }) }
  }
  const after = at(new Date(2026, 9, 7, 0, 10, 0).toISOString())
  assert.equal(after.data.agent.lastEditAt, '23:50')
  assert.equal(after.data.criteria.acceptedAt, '00:10')
  assert.equal(after.v.showBar, true, 'accepted after midnight, after the work: the chip is drawn')
  assert.equal(after.v.barTitle, 'Accepted 00:10; the agent’s last edit was 23:50.')
  assert.ok(text({ data: after.data }).includes(', after the work'), 'the criteria note says after the work')
  /* the converse still holds: a bar accepted BEFORE the last edit draws no chip, whatever the clock faces say */
  const before = at(new Date(2026, 9, 6, 23, 40, 0).toISOString())
  assert.equal(before.v.showBar, false, 'accepted before the last edit: no chip')
  const wrap = at(new Date(2026, 9, 5, 23, 59, 0).toISOString())
  assert.equal(wrap.v.showBar, false, 'accepted the day before the edit, later on the clock: no chip')
})

test('the live path from a served thread: real content, no design notes, no engine words, no trial, no hole', () => {
  for (const [t, lic] of [[null, FREE], [tracked(), PRO], [tracked({ ready: false }), FREE], [tracked({ history: [{ act: 'approved', actor: 'sam', decided_at: T(9), target_revision: 'full-rev-abc' }] }), PRO]]) {
    const data = selectApproveSheet(snap(), t, { licence: lic })
    const out = text({ data })
    for (const a of DESIGN_NOTES) assert.ok(!out.includes(a), a)
    assert.ok(!ENGINE_WORDS.test(out), `engine words: ${out.match(ENGINE_WORDS)}`)
    assert.ok(!/trial/i.test(out), 'trial')
    assert.ok(!/\b(undefined|null|NaN)\b/.test(out), `hole: ${out.match(/.{40}\b(undefined|null|NaN)\b.{20}/)}`)
    for (const canvas of ['checkout-web', 'e71c5b0', '3f9a1c2', 'cart/totals.py', 'Sam', '1.4 GB', '9 days']) assert.ok(!out.includes(canvas), `canvas value ${canvas}`)
  }
})

test('C30 R-LEDGER: a note is kept with the decision, so the live note box is on and says what it is for', () => {
  const data = selectApproveSheet(snap(), tracked(), { licence: PRO })
  const h = html({ data })
  assert.ok(!/<textarea[^>]*disabled/.test(h), 'the note box is on')
  const t = text({ data })
  assert.ok(t.includes('optional · kept with the decision'), t.slice(0, 200))
  assert.ok(t.includes('What you checked by hand, or why you are approving') || h.includes('What you checked by hand, or why you are approving'))
  assert.ok(!t.includes('Not available in this version'), 'no stand-in text')
  assert.ok(!h.includes('a note is not kept with the decision yet'))
})

test('C30 R-LEDGER: the decision rows carry the note the record kept with each act, and none where there is none', () => {
  const rows = decisionRows([
    { act: 'approved', decided_at: '2026-10-07T12:00:00Z', actor: 'Robin', note: 'Read the diff by hand.' },
    { act: 'withdrawn', decided_at: '2026-10-07T12:05:00Z', actor: 'Robin' },
  ])
  assert.deepEqual(rows.map((r) => r.note), ['Read the diff by hand.', ''])
})

function stub(answers = {}) {
  const calls = []
  const f = async (url, init = {}) => {
    calls.push({ url, method: init.method || 'GET', body: init.body ? JSON.parse(init.body) : null })
    const a = answers[url] || { status: 200, doc: {} }
    return { ok: a.status < 400, status: a.status, json: async () => a.doc }
  }
  return { f, calls }
}

test('Approve: write address 1, with the name and the full version seen', async () => {
  const { f, calls } = stub()
  await approveVersion({ changeId: 'chg_1', seen: 'full-rev-abc', actor: '  Robin ' }, f)
  assert.deepEqual(calls, [{ url: '/api/changes/chg_1/decisions', method: 'POST', body: { actor: 'Robin', seen_version: 'full-rev-abc' } }])
  assert.equal(CHANGE_DECIDE('x'), '/api/changes/x/decisions')
})

test('Approve with a note: the note rides on write address 1, trimmed, and only when there are words', async () => {
  const { f, calls } = stub()
  await approveVersion({ changeId: 'chg_1', seen: 'full-rev-abc', actor: 'Robin', note: '  Read the diff by hand.\n' }, f)
  await approveVersion({ changeId: 'chg_1', seen: 'full-rev-abc', actor: 'Robin', note: ' \n ' }, f)
  await approveVersion({ changeId: 'chg_1', seen: 'full-rev-abc', actor: 'Robin', note: undefined }, f)
  assert.deepEqual(calls.map((c) => c.body), [
    { actor: 'Robin', seen_version: 'full-rev-abc', note: 'Read the diff by hand.' },
    { actor: 'Robin', seen_version: 'full-rev-abc' },
    { actor: 'Robin', seen_version: 'full-rev-abc' },
  ])
  assert.ok(calls.every((c) => c.url === '/api/changes/chg_1/decisions'), 'the same one address')
})

test('Withdraw: write address 2, pointing at the approval it withdraws', async () => {
  const { f, calls } = stub()
  await withdrawApproval({ changeId: 'chg_1', approvedFull: 'full-rev-abc', actor: 'Robin' }, f)
  assert.deepEqual(calls, [{ url: '/api/changes/chg_1/decisions/withdraw', method: 'POST', body: { actor: 'Robin', seen_version: 'full-rev-abc' } }])
  assert.equal(CHANGE_WITHDRAW('x'), '/api/changes/x/decisions/withdraw')
})

test('C54 approve#1: a fresh approval can be withdrawn before the tracked refresh arrives', () => {
  const data = selectApproveSheet(snap(), tracked({ history: [{
    act: 'approved', decided_at: T(6), actor: 'Sam', target_revision: 'full-rev-abc',
  }] }), { licence: PRO })
  data.act.approvedFull = null // tracked model has not refreshed yet
  const sent = []
  const v = M.vals({ data, actions: { withdraw: (actor, revision) => {
    sent.push({ actor, revision })
    return Promise.resolve({ ok: false, status: 409, doc: { error: 'refused' } })
  } } }, { name: 'Sam' })
  assert.equal(v.withdrawEnabled, true)
  v.withdraw()
  assert.deepEqual(sent, [{ actor: 'Sam', revision: 'full-rev-abc' }])
})

test('C54 approve#1: Withdraw stays off when no exact approved revision is recorded', () => {
  const data = selectApproveSheet(snap(), tracked({ history: [{
    act: 'approved', decided_at: T(6), actor: 'Sam',
  }] }), { licence: PRO })
  data.act.approvedFull = null
  const v = M.vals({ data, actions: { withdraw: () => { throw new Error('no revision') } } }, { name: 'Sam' })
  assert.equal(v.withdrawEnabled, false)
})

test('no write without a Change, a version and a name', async () => {
  const { f, calls } = stub()
  for (const a of [{ changeId: null, seen: 'v', actor: 'a' }, { changeId: 'c', seen: null, actor: 'a' }, { changeId: 'c', seen: 'v', actor: '  ' }]) {
    assert.equal((await approveVersion(a, f)).ok, false)
    assert.equal((await withdrawApproval({ ...a, approvedFull: a.seen }, f)).ok, false)
  }
  assert.equal(calls.length, 0)
})

test('census: in this folder only actions.js writes, and it reaches exactly two addresses', () => {
  for (const n of readdirSync(here).filter((n) => /\.(js|jsx)$/.test(n) && !n.endsWith('.test.js') && n !== 'actions.js')) {
    assert.ok(!/method:\s*['"]POST['"]/.test(readFileSync(join(here, n), 'utf8')), `${n} must not POST`)
  }
  const a = readFileSync(join(here, 'actions.js'), 'utf8')
  const addresses = [...a.matchAll(/'\/api\/changes\/' \+ encodeURIComponent\(id\) \+ '([^']*)'/g)].map((m) => m[1])
  assert.deepEqual(addresses.sort(), ['/decisions', '/decisions/withdraw'])
})

/* Gate-fix: RUN-LINK-TRUTH-R5 — the sheet's off Approve leads with the one
 * blocker (`readiness.blocker`, built by change-model.js's `blockerOf`), word
 * for word, reason then act; never words of its own. */
test('live, tracked but not ready, with the served blocker: Approve is off and says the one blocker, reason then act', () => {
  const t = tracked({ ready: false })
  t.model.readiness.blocker = { reason: 'Line 2 has no passing run on this version.', enabler: 'Run the checks on it.' }
  const data = selectApproveSheet(snap(), t, { licence: PRO })
  assert.deepEqual(data.act.readinessBlocker, t.model.readiness.blocker)
  const v = M.vals({ data }, { name: 'Robin' })
  assert.equal(v.approveEnabled, false)
  assert.equal(v.why.l1, 'Line 2 has no passing run on this version.')
  assert.equal(v.why.l2, 'Run the checks on it.')
  /* ready: no blocker is carried, whatever the model holds */
  const r = tracked({ ready: true })
  r.model.readiness.blocker = { reason: 'stale', enabler: 'stale' }
  assert.equal(selectApproveSheet(snap(), r, { licence: PRO }).act.readinessBlocker, null)
})

/* Gate-fix: T10 (t10.4) — the session behind is one instant, when it started;
 * no clock-time range is composed, in the gallery or live. */
test('the session behind the sheet is when it started, never a composed clock-time range', () => {
  /* the gallery's own design-review note ("Illustrative: … the 16:10–16:14 times …")
   * is canvas text about the demo, not a drawn span; the line checked is the meta */
  assert.equal(M.vals().behindMeta, 'checkout-web · today 16:01')
  assert.ok(text().includes('checkout-web · today 16:01'), 'the start instant is drawn')
  const live = M.vals({ data: selectApproveSheet(snap(), tracked(), { licence: PRO }) }).behindMeta
  assert.ok(!/\d{2}:\d{2}\s*[–-]\s*\d{2}:\d{2}/.test(live), 'a clock-time range is drawn live: ' + live)
})

test('C26.6: the dimmed Change behind the sheet shows one line of a multi-line request, cut, whole in its title', () => {
  const data = selectApproveSheet(snap(), tracked(), { licence: PRO })
  data.request = { ...data.request, text: "cat <<'EOF' > run.sh\nset -e\npytest -q tests/ && echo done\nEOF" }
  data.pending = { step: 9, command: "python3 - <<'PY'\nimport json\nprint(json.dumps({}))\nPY" }
  const v = M.vals({ data })
  assert.equal(v.requestText, '“cat <<\'EOF\' > run.sh…”')
  assert.equal(v.requestTitle, data.request.text)
  assert.ok(!v.pendingCommand.includes('\n') && v.pendingCommand.endsWith('…'))
  assert.ok(html({ data }).includes('<div title="' + data.request.text + '"'), 'the whole request is the card’s title')
})
