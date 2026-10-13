/* C29 part 3, C35: Approve refuses while the engine says an accepted decision
 * blocks approving the exact version on the sheet (`approval_gate` active, and
 * the decision's or its result's own `blocks_approval`), and says so in the
 * decision's own words, with its sentence and "See where": broken on this
 * version, no proof found on it, or no check recorded on it yet. A warning, a
 * valid waiver, a stale result, an unverifiable result the policy only warns
 * about, a dormant gate and a result on another version refuse nothing; a
 * decision not checked on this version is a note, and a gate that is not
 * active is said with the server's own reason. The server stays the gate:
 * every answer it gives is still drawn (C30 P0-2). Synthetic fixtures only;
 * nothing is fetched or POSTed. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { changeSnapshot } from '../../data/change.js'
import { decisionsModel } from '../../data/decisions.js'
import { DECISIONS_CASES, DECISIONS_DORMANT, DECISIONS_LIST } from '../../data/decisions-fixture.js'
import { selectApproveSheet } from './from-change.js'

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
function make(props, state) { const inst = new Logic(props || {}); inst.props = props || {}; if (state) inst.state = { ...inst.state, ...state }; return inst }
export function render(props, state) { return renderToStaticMarkup(React.createElement(Template, { v: make(props, state).renderVals(), link: (h) => h })) }
export function vals(props, state) { return make(props, state).renderVals() }
export { make }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-approve-decisions-')), 'approve.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const html = (props, state) => unescape(M.render(props, state))
const text = (props, state) => html(props, state).replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ')

const CUR = 'wt-3c3ca28a1b2c3d4e'
const OLDER = 'wt-0aa1bb2cc3dd4ee5'
const T = (m) => new Date(Date.UTC(2026, 9, 7, 9, m, 0)).toISOString()
function thread(cur) {
  return {
    schema_version: 1,
    project: { id: 'p1', name: 'ledger-api', root: '/r/ledger-api' },
    thread: { id: '0123456789abcdef', title: 'Stop the import double-posting', current_version_id: cur, whose_move: 'you', proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: [] } },
    prompts: [{ event_id: 'ev-p', session_id: 's1', seq: 1, ts: T(0), text: 'the import posts twice', work: true, version_id: cur, raw_file: '/m', raw_line: 1 }],
    edits: [{ id: 'ed0', event_id: 'ed0', session_id: 's1', ts: T(2), file: '/r/ledger-api/src/post.py', op: 'edit', lines_added: 1, lines_removed: 0, ok: true, version_id: cur, raw_file: '/m', raw_line: 3 }],
    commands: [{ id: 'c0#0', event_id: 'c0', part: 0, session_id: 's1', ts: T(3), command: 'pytest -q', outcome: 'succeeded', exit_code: 0, is_test: true, is_commit: false, tally: { passed: 4, failed: 0, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 }, version_id: cur, raw_file: '/m', raw_line: 4 }],
    claims: [{ id: 'cl0#0', event_id: 'cl0', session_id: 's1', ts: T(4), text: 'Import fixed. Tests pass.', kind: 'tests_pass', has_verifier: true, rests_on_command_id: 'c0#0', followed_by_passing_test: true, version_id: cur, raw_file: '/m', raw_line: 5 }],
    versions: [{ id: cur, commit: null, short: null, session_id: 's1', from_seq: 1, to_seq: 9, committed: true }],
    current_version_id: cur,
    sessions: [{ id: 's1', ledger_project_id: 'lp', source: 'claude_code', origin: 'person', started_at: T(0), last_at: T(5), file: '/m' }],
    files: [],
    masking: { masked: 0, families: [], complete: true },
  }
}
const PRO = { status: 'valid', plan: 'pro', entitlements: { approve_version: { allowed: true } } }
const tracked = () => ({
  id: 'chg_1',
  change: { accepted_scope: { version: 1, acceptance: { actor: 'sam', accepted_at: T(5) } }, decisions: { history: [] }, code_moved: null },
  model: {
    criteria: [1, 2].map((n) => ({ n, text: 'Line ' + n + ' holds.', state: 'proven', word: 'Proven' })),
    decision: { seen: 'full-rev-abc', approvedFull: null },
    readiness: { ready: true, sentence: null, waivedLines: [] },
  },
})
const sheet = (cur = CUR) => selectApproveSheet(changeSnapshot(thread(cur), null, new Date(Date.UTC(2026, 9, 7, 10, 0))), tracked(), { licence: PRO })
const clone = (x) => JSON.parse(JSON.stringify(x))
const served = (doc = DECISIONS_LIST) => decisionsModel({ status: 'served', doc })
/** The list with only the decisions named kept, each changed by `edit`. */
const only = (ids, edit = (d) => d) => {
  const doc = clone(DECISIONS_LIST)
  doc.decisions = doc.decisions.filter((d) => ids.includes(d.id)).map(edit)
  return doc
}
const AT = { screen: 'approve', change: '0123456789abcdef', project: 'p1' }

test('ready, Pro, a name typed, and no decision blocks: Approve is on (the control the refusal turns off)', () => {
  const v = M.vals({ data: sheet(), decisions: served(only(['dec-02', 'dec-05'])), at: AT }, { name: 'Robin' })
  assert.equal(v.approveEnabled, true)
})

test('a blocking decision broken, or with no proof, on exactly this version: Approve is refused in the decision’s own words', () => {
  const props = { data: sheet(), decisions: served(), at: AT, actions: { approve: () => { throw new Error('must not write') } } }
  const v = M.vals(props, { name: 'Robin' })
  assert.equal(v.approveEnabled, false)
  assert.equal(v.approveDisabled, true)
  assert.equal(v.why.l1, '2 decisions you set block approving this version.')
  assert.match(v.why.l2, /^“Never show a home-folder path on any screen”: Broken on wt-3c3ca28: the record shows it in 2 places\./)
  v.approve() // refused before anything is sent
  const t = text(props, { name: 'Robin' })
  assert.match(t, /Never show a home-folder path on any screen/)
  assert.match(t, /Broken on wt-3c3ca28: the record shows it in 2 places\./)
  assert.match(t, /Write the failing test before you change the code/)
  assert.match(t, /No proof found on wt-3c3ca28: the record has no failing run before the first source edit\./)
  assert.match(t, /See where/)
  assert.ok(html(props, { name: 'Robin' }).includes('href="#/changes/0123456789abcdef?project=p1"'), 'See where goes to the Brief’s decision rows')
  // a warning that holds does not refuse, and is not listed as a refusal
  assert.doesNotMatch(t, /Payments are named Stripe everywhere/)
  for (const w of [/\blie\b/i, /cheat/i, /violat/i, /\bignor/i, /hallucinat/i, /trial/i]) assert.doesNotMatch(t, w)
})

test('one broken blocking decision alone refuses; the sentence is singular', () => {
  const v = M.vals({ data: sheet(), decisions: served(only(['dec-01'])), at: AT }, { name: 'Robin' })
  assert.equal(v.approveEnabled, false)
  assert.equal(v.why.l1, 'A decision you set blocks approving this version.')
})

test('not refused for a warning: the same broken result on a decision that only warns, which the engine says does not block', () => {
  const warns = (d) => Object.assign(d, { severity: 'warns', blocks_approval: false, current_verdict: Object.assign(d.current_verdict, { blocks_approval: false }) })
  const props = { data: sheet(), decisions: served(only(['dec-01'], warns)), at: AT }
  assert.equal(M.vals(props, { name: 'Robin' }).approveEnabled, true)
  assert.doesNotMatch(text(props, { name: 'Robin' }), /block approving/)
})

test('the state word alone does not refuse: a broken, blocking result whose blocks_approval the engine says is false', () => {
  const waived = (d) => Object.assign(d, { blocks_approval: false, current_verdict: Object.assign(d.current_verdict, { blocks_approval: false, waived: { id: 'w1', actor: 'you', because: 'shipping a fix next', at: '2026-10-07T10:00:00Z', version_id: CUR, expires_version: null, expires_at: null } }) })
  const props = { data: sheet(), decisions: served(only(['dec-01'], waived)), at: AT }
  assert.equal(M.vals(props, { name: 'Robin' }).approveEnabled, true)
  assert.doesNotMatch(text(props, { name: 'Robin' }), /Decisions that block this approval|block approving/)
})

test('a decision with no check on this version refuses as its own kind, in its own words', () => {
  const props = { data: sheet(), decisions: served(DECISIONS_CASES), at: AT }
  const v = M.vals(props, { name: 'Robin' })
  assert.equal(v.approveEnabled, false)
  assert.equal(v.why.l1, '3 decisions you set block approving this version.')
  const t = text(props, { name: 'Robin' })
  assert.match(t, /Never show a home-folder path on any screen/)
  assert.match(t, /Broken on wt-3c3ca28: the record shows it in 2 places\./)
  assert.match(t, /The privacy sentence is on every settings screen/)
  assert.match(t, /No check is recorded on wt-3c3ca28 yet\./)
  assert.match(t, /Release notes are written before tagging/)
  assert.match(t, /No check is recorded on wt-3c3ca28 yet\. The latest result is on wt-0aa1bb2\./)
  assert.match(t, /Not approved while these decisions block approving wt-3c3ca28\. Each says why\./)
  // what the engine says does not block is not listed as blocking
  for (const not of ['Warn me about home-folder paths', 'Dates in the changelog are UTC', 'Every migration has a rollback note', 'Counts are written with thousands separators']) assert.doesNotMatch(t, new RegExp(not))
  // and a refused decision is a refusal, not also a "not checked" note
  assert.doesNotMatch(t, /Not checked on wt-3c3ca28: .*privacy sentence/)
})

test('a missing check alone refuses; the sentence is singular and names the missing check', () => {
  const only1 = { ...DECISIONS_CASES, decisions: DECISIONS_CASES.decisions.filter((d) => d.id === 'cs-no-verdict') }
  const v = M.vals({ data: sheet(), decisions: served(only1), at: AT }, { name: 'Robin' })
  assert.equal(v.approveEnabled, false)
  assert.equal(v.why.l1, 'A decision you set blocks approving this version.')
  assert.equal(v.why.l2, '“The privacy sentence is on every settings screen”: No check is recorded on wt-3c3ca28 yet.')
  assert.match(text({ data: sheet(), decisions: served(only1), at: AT }, { name: 'Robin' }), /Not approved while this decision blocks approving wt-3c3ca28\. It says why\./)
})

test('a dormant gate refuses nothing, and the sheet says so with the server’s own reason', () => {
  const props = { data: sheet(), decisions: served(DECISIONS_DORMANT), at: AT }
  const v = M.vals(props, { name: 'Robin' })
  assert.equal(v.approveEnabled, true)
  const t = text(props, { name: 'Robin' })
  assert.match(t, /Decisions you set do not block approving right now\. Decisions approval is dormant in this record\./)
  assert.doesNotMatch(t, /Decisions that block this approval|block approving this version/)
  // with no blocking decision there is nothing to say
  const none = only(['dec-02', 'dec-05'])
  none.approval_gate = DECISIONS_DORMANT.approval_gate
  assert.doesNotMatch(text({ data: sheet(), decisions: served(none), at: AT }, { name: 'Robin' }), /do not block approving right now/)
})

test('not refused for another version: the sheet is on an older version than the broken result', () => {
  const props = { data: sheet(OLDER), decisions: served(only(['dec-01', 'dec-03'])), at: AT }
  const v = M.vals(props, { name: 'Robin' })
  assert.equal(v.approveEnabled, true)
  // and the decisions not checked on this version are a note, not a refusal
  const t = text(props, { name: 'Robin' })
  assert.match(t, /Not checked on wt-0aa1bb2/)
  assert.match(t, /Never show a home-folder path on any screen/)
})

test('a decision with a result on an older version is a note on this one, and refuses nothing', () => {
  const props = { data: sheet(), decisions: served(only(['dec-04', 'dec-02'])), at: AT }
  assert.equal(M.vals(props, { name: 'Robin' }).approveEnabled, true)
  const t = text(props, { name: 'Robin' })
  assert.match(t, /Not checked on wt-3c3ca28/)
  assert.match(t, /The status bar always says what ThruLine keeps on this machine/)
})

test('the server stays the gate: its answer is drawn over the decision refusal', () => {
  const inst = M.make({ data: sheet(), decisions: served(), at: AT }, { name: 'Robin', answer: { act: 'approve', kind: 'refused', detail: 'The record refused this.' } })
  const v = inst.renderVals()
  assert.equal(v.why.l1, 'Not approved.')
  assert.equal(v.why.l2, 'The record refused this.')
})

test('no decisions route, or no decisions read yet: the sheet is as it was', () => {
  for (const decisions of [undefined, decisionsModel({ status: 'not_served' }), decisionsModel(undefined)]) {
    const v = M.vals({ data: sheet(), decisions, at: AT }, { name: 'Robin' })
    assert.equal(v.approveEnabled, true)
    assert.doesNotMatch(text({ data: sheet(), decisions, at: AT }, { name: 'Robin' }), /Decisions are not checked|block approving|Not checked on/)
  }
  // the design gallery draws none of it
  assert.doesNotMatch(text({ decisions: served() }), /Never show a home-folder path/)
})
