/* C30 census rows on the Change screens, each on the real record's shape.
 *
 *  1. Altitude (Change-Brief 14, Every-Statement 2, What-Changed 2, Repair-Loop
 *     2, Approve-Sheet 9, Done-Criteria 2): Brief / Explore / Workbench are
 *     links to the open Change's brief / statements / changed addresses, with
 *     its project, and the one in front of the person is marked as the canvas
 *     marks it. The gallery keeps each board's own mark.
 *  2. Repair-Loop 5: Copy in the command row copies the check's own command,
 *     never the words "the check"; with no command recorded it says so and
 *     offers nothing to copy.
 *  3. Repair-Loop 14: the ladder's "Later work seen" and "What changed" are read
 *     from the record, and say so when it holds nothing.
 *  4. Approve-Sheet 5: each "It rests on" row carries its line's kind and link
 *     basis from the Change model, not "behaviour" for every line.
 *  5. Every-Statement 9: the four filters filter the list by the rows' own
 *     fields; every count is the rows shown; nothing matching is said.
 *  6. Change-Brief defect: a thread with no statements is drawn in its frame,
 *     with an honest empty state and its links.
 *  7. Every-Statement 21: "Open this in the full statement view" opens the
 *     Brief at that statement (`?statement=`), which the Brief selects.
 *
 * Every record here is synthetic; every fetch is stubbed; nothing reads a store
 * or reaches a server. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>', { url: 'http://127.0.0.1/' })
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const here = dirname(fileURLToPath(import.meta.url))
const SIX = ['Change-Brief', 'Every-Statement', 'What-Changed', 'Done-Criteria', 'Repair-Loop', 'Approve-Sheet']

const M = await (async () => {
  const imports = SIX.map((n, i) => `import T${i} from './${n}/template.jsx'\nimport L${i} from './${n}/logic.js'`).join('\n')
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { createRoot } from 'react-dom/client'
import { flushSync } from 'react-dom'
import * as R from '../routes.js'
import * as SC from '../screen.jsx'
import { designLink } from '../artboards.jsx'
import { DcScreen } from '../dc/runtime.jsx'
import { withFrame } from '../data/project.js'
import { changeSnapshot } from '../data/change.js'
import { everyStatementFromChange } from './Every-Statement/from-change.js'
import { whatChangedFromChange } from './What-Changed/from-change.js'
import { selectDoneCriteria } from './Done-Criteria/from-change.js'
import { selectApproveSheet } from './Approve-Sheet/from-change.js'
import { selectRepairLoop } from './Repair-Loop/from-change.js'
import { deriveChangeModel } from '../../workspace/change-model.js'
import BriefScreen from './Change-Brief/index.jsx'
${imports}
const S = { ${SIX.map((n, i) => `'${n}': [T${i}, L${i}]`).join(', ')} }
export { R, SC, changeSnapshot, everyStatementFromChange, whatChangedFromChange, selectDoneCriteria, selectApproveSheet, selectRepairLoop, deriveChangeModel }
export function vals(name, props, state) {
  const L = withFrame(S[name][1])
  const inst = new L(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return inst.renderVals()
}
export function render(name, props, link, state) {
  return renderToStaticMarkup(React.createElement(S[name][0], { v: vals(name, props, state), link }))
}
export function renderDesign(name) {
  return renderToStaticMarkup(React.createElement(DcScreen, { Logic: S[name][1], Template: S[name][0], props: {}, link: designLink }))
}
export function mount(name, props, link) {
  const el = document.createElement('div')
  document.body.appendChild(el)
  const root = createRoot(el)
  flushSync(() => root.render(React.createElement(DcScreen, { Logic: withFrame(S[name][1]), Template: S[name][0], props, link })))
  return { el, unmount: () => { root.unmount(); el.remove() } }
}
export function mountBrief(at) {
  const el = document.createElement('div')
  document.body.appendChild(el)
  const root = createRoot(el)
  flushSync(() => root.render(React.createElement(BriefScreen, { at, openCommand: () => {} })))
  return { el, unmount: () => { root.unmount(); el.remove() } }
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-change-pass-')), 'screens.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const R = M.R

/* ---------- a synthetic served ThreadView (docs/hq-next/API.md) ----------
 * One Claude Code session: an edit, a passing test run, "Tests pass." and
 * "Checkout total fixed." in one message, then an edit that moves the proof.
 * `later`: the agent works on afterwards (an edit, then a command).
 * `codex`: a second session, Codex's, says "Lint is clean." with no run.
 * `claims: false`: nothing the agent said claims it is finished. */
const T = (m) => `2026-10-07T10:${String(m).padStart(2, '0')}:00.000Z`
const V1 = 'wt-1111111111111111'
const V2 = 'wt-2222222222222222'
const TID = '0123456789abcdef'
function thread({ later = false, codex = false, claims = true } = {}) {
  const edit = (id, m, file, version, line) => ({ id, event_id: id, session_id: 's1', ts: T(m), file, op: 'edit', action: null, moved_to: null, hunks: [], lines_added: 1, lines_removed: 0, ok: true, source: 'structured_patch', via: 'tool', version_id: version, raw_file: '/x.jsonl', raw_line: line })
  const command = (id, m, text, test, version, line) => ({ id, event_id: id, part: 0, session_id: 's1', ts: T(m), command: text, readable: true, outcome: 'succeeded', exit_code: 0, output_excerpt: [], output_lines_total: 0, output_scope: 'command', is_test: test, is_commit: false,
    tally: test ? { passed: 4, failed: 0, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 } : null, tally_line: null, via: 'bash', version_id: version, raw_file: '/x.jsonl', raw_line: line })
  const edits = [edit('e1', 1, '/repo/src/totals.py', V1, 2), edit('e2', 5, '/repo/src/api.py', V2, 9)]
  const commands = [command('c1', 2, 'pytest -q tests/test_totals.py', true, V1, 3)]
  if (later) { edits.push(edit('e3', 7, '/repo/src/util.py', V2, 11)); commands.push(command('c3', 8, 'ls src', false, V2, 12)) }
  const said = [
    { id: 'm1#0', event_id: 'm1', session_id: 's1', ts: T(4), text: 'Tests pass.', kind: 'tests_pass', has_verifier: true, marker: 'completion', rests_on_command_id: 'c1', followed_by_passing_test: false, version_id: V1, raw_file: '/x.jsonl', raw_line: 5 },
    { id: 'm1#1', event_id: 'm1', session_id: 's1', ts: T(4), text: 'Checkout total fixed.', kind: 'fixed', has_verifier: false, marker: 'completion', rests_on_command_id: 'c1', followed_by_passing_test: false, version_id: V1, raw_file: '/x.jsonl', raw_line: 5 },
  ]
  if (codex) said.push({ id: 'k1#0', event_id: 'k1', session_id: 's2', ts: T(3), text: 'Lint is clean.', kind: 'lint_clean', has_verifier: true, marker: 'completion', rests_on_command_id: null, followed_by_passing_test: false, version_id: V1, raw_file: '/y.jsonl', raw_line: 2 })
  const sessions = [{ id: 's1', ledger_project_id: 'lp', source: 'claude_code', origin: 'person', started_at: T(0), last_at: T(8), file: '/x.jsonl' }]
  if (codex) sessions.push({ id: 's2', ledger_project_id: 'lp', source: 'codex', origin: 'person', started_at: T(3), last_at: T(3), file: '/y.jsonl' })
  return {
    schema_version: 1,
    project: { id: 'p1', name: 'shop', root: '/repo' },
    thread: { id: TID, title: 'Fix the checkout total', current_version_id: V2, whose_move: 'you', proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: ['/repo/src/api.py'] } },
    prompts: [{ event_id: 'p1', session_id: 's1', seq: 1, ts: T(0), text: 'Fix the checkout total', work: true, version_id: V1, raw_file: '/x.jsonl', raw_line: 1 }],
    edits,
    commands,
    claims: claims ? said : [],
    versions: [
      { id: V1, commit: null, short: null, session_id: 's1', from_seq: 1, to_seq: 4, committed: true, message: 'first', boundary_event_id: 'b1', committed_at: T(4) },
      { id: V2, commit: null, short: null, session_id: 's1', from_seq: 5, to_seq: 12, committed: false, message: null, boundary_event_id: null, committed_at: null },
    ],
    current_version_id: V2,
    sessions,
    files: [],
    masking: { masked: 0, families: [], complete: true },
  }
}
const NOW = new Date('2026-10-07T10:12:00.000Z')
const snap = (o) => M.changeSnapshot(thread(o), null, NOW, { storageBytes: 2.1e9 })
const LICENCE_FREE = { status: 'absent', paid_gate: true, entitlements: { approve_version: { allowed: false, status: 'absent' } }, technical: {} }

/** Each Change screen's live data, from one record (as each index.jsx selects it). */
function liveData(name, s = snap()) {
  switch (name) {
    case 'Change-Brief': return s
    case 'Every-Statement': return M.everyStatementFromChange(s)
    case 'What-Changed': return M.whatChangedFromChange(s)
    case 'Done-Criteria': return M.selectDoneCriteria(s, null)
    case 'Repair-Loop': return M.selectRepairLoop(s, null, { now: NOW })
    case 'Approve-Sheet': return M.selectApproveSheet(s, null, { licence: LICENCE_FREE })
    default: throw new Error(name)
  }
}
const KEY = { 'Change-Brief': 'brief', 'Every-Statement': 'statements', 'What-Changed': 'changed', 'Done-Criteria': 'done', 'Repair-Loop': 'check', 'Approve-Sheet': 'approve' }
/* the address names the Change and not its project: the links must carry the snapshot's (screen.jsx `linkAt`) */
const atOf = (name, extra = {}) => ({ screen: KEY[name], change: TID, ...extra })
const linkOf = (name, s = snap()) => R.linker(M.SC.linkAt(atOf(name), M.SC.projectOf(s)))
const unescape = (x) => x.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const liveHtml = (name, { data, s, at, state } = {}) => {
  const sn = s || snap()
  return unescape(M.render(name, { data: data || liveData(name, sn), frame: {}, at: at || atOf(name) }, linkOf(name, sn), state))
}
const textOf = (html) => html.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ')

/** The altitude group: its anchors (href, aria-current, style), and any button inside it. */
function altitudeOf(html) {
  const i = html.indexOf('aria-label="Altitude"')
  assert.ok(i >= 0, 'the Altitude group is drawn')
  const body = html.slice(i, html.indexOf('</div>', i))
  const anchors = []
  const re = /<a\b([^>]*)>([\s\S]*?)<\/a>/g
  let m
  while ((m = re.exec(body))) {
    const attr = (k) => { const x = m[1].match(new RegExp('\\b' + k + '="([^"]*)"')); return x ? x[1] : null }
    anchors.push({ label: m[2].replace(/<[^>]+>/g, '').trim(), href: attr('href'), current: attr('aria-current'), style: attr('style') || '' })
  }
  return { anchors, buttons: (body.match(/<button\b/g) || []).length }
}

/* ======================= 1. Altitude navigates ======================= */

const MARKED = { 'Change-Brief': ['Brief', 'page'], 'Every-Statement': ['Explore', 'page'], 'What-Changed': ['Workbench', 'page'], 'Done-Criteria': ['Brief', 'true'], 'Repair-Loop': ['Brief', 'true'], 'Approve-Sheet': ['Brief', 'true'] }
for (const name of SIX) {
  test(`1 · ${name}: Brief / Explore / Workbench are links to this Change's brief, statements and changed views, with its project`, () => {
    const { anchors, buttons } = altitudeOf(liveHtml(name))
    assert.equal(buttons, 0, 'no altitude is a state toggle')
    assert.deepEqual(anchors.map((a) => a.label), ['Brief', 'Explore', 'Workbench'])
    const want = ['brief', 'statements', 'changed']
    anchors.forEach((a, i) => {
      assert.ok(a.href && a.href.startsWith('#/'), a.label + ' is an address, got ' + a.href)
      const r = R.parse(a.href)
      assert.equal(r.screen, want[i], a.label + ' → ' + a.href)
      assert.equal(r.change, TID, a.label + ' keeps the open Change')
      assert.equal(r.project, 'p1', a.label + ' keeps the project')
    })
  })
  test(`1 · ${name}: the altitude in front of the person is the one marked, as the canvas marks it`, () => {
    const { anchors } = altitudeOf(liveHtml(name))
    assert.equal(anchors.length, 3, 'three altitude links')
    const [label, current] = MARKED[name]
    for (const a of anchors) {
      const on = a.label === label
      assert.equal(a.current, on ? current : null, a.label + ' aria-current')
      assert.equal(/background:#2A2A30/.test(a.style), on, a.label + ' carries the on look only when marked: ' + a.style)
      assert.equal(/color:#EDEDEF/.test(a.style), on, a.label + ' text colour')
    }
  })
}

test('1 · the gallery keeps each board as drawn: What changed marks Brief there, and Every statement marks Explore', () => {
  const wc = altitudeOf(unescape(M.renderDesign('What-Changed'))).anchors
  assert.deepEqual(wc.filter((a) => /background:#2A2A30/.test(a.style)).map((a) => a.label), ['Brief'])
  const es = altitudeOf(unescape(M.renderDesign('Every-Statement'))).anchors
  assert.deepEqual(es.filter((a) => /background:#2A2A30/.test(a.style)).map((a) => a.label), ['Explore'])
})

/* ======================= 2. Repair-Loop: Copy copies the command ======================= */

/* The served run sheet (`GET /api/changes/<id>/run`): its check is `cargo test`, and it last passed. */
const SHEET = (last) => ({
  schema_version: 2, change: 'chg_1', project_folder: '/repo',
  permission: { granted_by: 'sam', granted_at: T(9) },
  consent: { state: 'given', history: [], file: '.thruline/run-permission.json' },
  checks: [{ id: 'cargo-test', program: 'cargo', category: 'tests', command: 'cargo test', last: last ? { run: 'hq-check-01', state: 'completed', why: null, reason: null, signal: null, suites: 1, failure_line: null, fetch_skipped: null, survivors: [], settles: true, started_at: T(10), recorded_at: T(11), cleanup_verified: true, ...last } : null }],
  not_runnable_here: [], environments: [], remembered_environment: null, job: null, not_started: null, actor: 'sam',
})
function clipboard() {
  const wrote = []
  Object.defineProperty(globalThis.navigator, 'clipboard', { value: { writeText: async (t) => { wrote.push(t) } }, configurable: true })
  return wrote
}
const passedRepair = () => {
  const sheet = SHEET({ outcome: 'passed', exit_code: 0, tests_passed: 5, tests_failed: 0 })
  const s = M.changeSnapshot(thread(), { change: { id: 'chg_1' }, hq: null, proposals: null, runSheet: sheet }, NOW)
  return M.selectRepairLoop(s, { id: 'chg_1', runSheet: sheet }, { now: NOW })
}

test('2 · Repair-Loop: Copy in the command row copies the check’s own command, exactly', () => {
  const data = passedRepair()
  assert.equal(data.run.state, 'passed')
  assert.equal(data.check.command, 'cargo test')
  const wrote = clipboard()
  const v = M.vals('Repair-Loop', { data, frame: {} })
  assert.equal(v.showDlCopy, true)
  v.copyDl()
  assert.deepEqual(wrote, ['cargo test'])
  assert.match(liveHtml('Repair-Loop', { data }), /<code title="cargo test"[^>]*>cargo test<\/code>/)
})

test('2 · Repair-Loop: with no command recorded, the row says so and nothing is copied — never “the check”', () => {
  const data = passedRepair()
  const bare = { ...data, check: { ...data.check, command: null, terminalCommand: null } }
  const wrote = clipboard()
  const v = M.vals('Repair-Loop', { data: bare, frame: {} })
  assert.ok(!v.showDlCopy, 'no Copy is offered for a command the record does not hold')
  v.copyDl()
  assert.deepEqual(wrote, [], 'nothing reaches the clipboard')
  const h = liveHtml('Repair-Loop', { data: bare })
  const code = h.match(/<code title="([^"]*)"[^>]*>([^<]*)<\/code>/)
  assert.ok(code, 'the command row is drawn')
  assert.equal(code[2], 'Not recorded')
  assert.notEqual(code[1], 'the check')
})

/* ======================= 3. Repair-Loop: ladder rungs 2–3 from the record ======================= */

test('3 · Repair-Loop: “Later work seen” and “What changed” name the agent’s later steps and the files changed after the stop', () => {
  const data = M.selectRepairLoop(snap({ later: true }), null, { now: NOW })
  const v = M.vals('Repair-Loop', { data, frame: {} })
  const [, later, changed] = v.ladder
  assert.equal(later.label, 'Later work seen')
  assert.equal(later.value, '2 later steps · steps 7–8')
  assert.equal(later.bg, '#EDEDEF', 'the rung is reached: the record holds later work')
  assert.match(later.title, /does not say whether they answer it/)
  assert.equal(changed.label, 'What changed')
  assert.equal(changed.value, '1 file changed since step 6: src/util.py')
  assert.equal(changed.bg, '#EDEDEF')
})

test('3 · Repair-Loop: when the record holds nothing after the stop, both rungs say so and stay unreached', () => {
  const data = M.selectRepairLoop(snap({ later: false }), null, { now: NOW })
  const v = M.vals('Repair-Loop', { data, frame: {} })
  const [, later, changed] = v.ladder
  assert.equal(later.value, 'No later step from the agent yet')
  assert.equal(changed.value, 'Nothing since step 6 (src/api.py)')
  assert.equal(later.bg, 'transparent')
  assert.equal(changed.bg, 'transparent')
})

/* ======================= 4. Approve-Sheet: what each line rests on ======================= */

const LINES = ['A discount code applies once.', 'Totals with no code are unchanged.', 'The receipt names the code.', 'The cart page loads.']
const CHANGE_DOC = {
  id: 'chg_1', project_id: 'lp', opened_at: T(0), request: { title: 'Fix the checkout total' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: T(9), repository_identity: 'shop' },
  accepted_scope: { version: 1, criteria: LINES, allowed_scope: ['src/'], check_profile: [], acceptance: { actor: 'sam', accepted_at: T(6) } },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: T(6) }],
  examination: 'none_yet', decisions: [],
}
const item = (basis, n) => ({
  link_id: 'l' + n, evidence_id: 'eb' + n, session_id: 's1', basis, basis_slug: 'link_basis_' + basis, actor: 'sam', asserted_at: T(9),
  kind: 'check_run', name: 'cargo test', observed: 'proven', bears: basis !== 'not-bearing', applies: true, applies_slug: 'inputs_applies',
  fresh: true, fresh_slug: 'applicability_still_applies', execution: 'completed', execution_slug: 'execution_completed',
})
const COVERAGE = {
  scope_version: 1, link_rows: 4, execution_sessions: ['s1'], candidates: [], executions: [],
  criteria: [
    { index: 1, text: LINES[0], state: 'evidence', removed: [], items: [item('manual', 1)] },
    { index: 2, text: LINES[1], state: 'evidence', removed: [], items: [item('declared', 2)] },
    { index: 3, text: LINES[2], state: 'evidence', removed: [], items: [item('named', 3)] },
    { index: 4, text: LINES[3], state: 'none', removed: [], items: [item('not-bearing', 4)] },
  ],
}

test('4 · Approve-Sheet: each “It rests on” row carries its own link basis and kind from the Change model', () => {
  const model = M.deriveChangeModel(CHANGE_DOC, COVERAGE, null, null, [])
  assert.equal(model.criteria.length, 4)
  const data = M.selectApproveSheet(snap(), { id: 'chg_1', change: CHANGE_DOC, model }, { licence: LICENCE_FREE })
  assert.deepEqual(data.criteria.lines.map((l) => l.link), ['person', 'accepted_check', 'named', null])
  for (const l of data.criteria.lines) assert.notEqual(l.kind, 'behaviour', 'the record types no line as a behaviour')
  const v = M.vals('Approve-Sheet', { data, frame: {} })
  assert.deepEqual(v.rests.map((r) => r.basis), [
    'You linked this yourself.',
    'The check you accepted.',
    'What ran names what this line is about.',
    'Nothing is linked to this line yet.',
  ])
  assert.ok(v.rests.every((r) => r.gate === false), 'no Gate badge is drawn on a guess')
  const h = liveHtml('Approve-Sheet', { data })
  assert.ok(h.includes(' · What ran names what this line is about.'))
})

test('4 · Approve-Sheet: a line resting on the check ThruLine ran prints no step it does not have', () => {
  const sheet = SHEET({ outcome: 'passed', exit_code: 0, tests_passed: 5, tests_failed: 0 })
  const s = M.changeSnapshot(thread(), { change: { id: 'chg_1' }, hq: null, proposals: null, runSheet: sheet }, NOW)
  const model = M.deriveChangeModel(CHANGE_DOC, COVERAGE, null, null, [])
  const data = M.selectApproveSheet(s, { id: 'chg_1', change: CHANGE_DOC, model }, { licence: LICENCE_FREE })
  assert.ok(data.run, 'the approval rests on a run')
  assert.equal(data.run.step, null, 'the check ThruLine ran has no step in the agent’s record')
  const v = M.vals('Approve-Sheet', { data, frame: {} })
  assert.ok(v.rests.every((r) => !/null/.test(r.step)), v.rests.map((r) => r.step).join(','))
  assert.ok(!/null/.test(v.runStepCaret), v.runStepCaret)
})

/* ======================= 5. Every-Statement: filters ======================= */

const wait = (ms) => new Promise((r) => setTimeout(r, ms))
/* the rack's groups of statements (the collapsed "Also said" group is never counted, and is not one) */
const rackGroups = (el) => Array.from(el.querySelectorAll('section[aria-label="Every statement"] div[role="group"]')).filter((g) => !/^Also said/.test(g.getAttribute('aria-label') || ''))
const rackRows = (el) => rackGroups(el).flatMap((g) => Array.from(g.querySelectorAll('.tl-rack'))).map((r) => r.textContent.replace(/\s+/g, ' '))
const groupHeads = (el) => rackGroups(el).map((g) => g.getAttribute('aria-label'))
const button = (el, start) => Array.from(el.querySelectorAll('button')).find((b) => b.textContent.replace(/\s+/g, ' ').trim().startsWith(start))
/* a click, and the redraw React schedules for it (a microtask after the event) */
const click = async (b) => { b.dispatchEvent(new window.MouseEvent('click', { bubbles: true })); await wait(0) }
const menuOf = (el, label) => el.querySelector(`[role="menu"][aria-label="${label}"]`)
/** each option as a person reads it: ✓ when chosen, its words, and how many rows it would show */
const options = (menu) => Array.from(menu.querySelectorAll('[role="menuitemradio"]')).map((o) => {
  const [label, hint] = Array.from(o.querySelectorAll(':scope > span:nth-child(2) > span')).map((x) => x.textContent)
  return (o.getAttribute('aria-checked') === 'true' ? '✓ ' : '') + label + ' · ' + hint
})

test('5 · Every-Statement: Agent, State, Kind and Session filter the list by the rows’ own fields, and every count is the rows shown', async () => {
  const s = snap({ codex: true })
  const data = M.everyStatementFromChange(s)
  const { el, unmount } = M.mount('Every-Statement', { data, frame: {}, at: atOf('Every-Statement') }, linkOf('Every-Statement', s))
  try {
    assert.equal(rackRows(el).length, 3, 'every statement is listed before any filter')
    await click(button(el, 'Agent'))
    const menu = menuOf(el, 'Filter by agent')
    assert.ok(menu, 'Agent opens its menu')
    assert.deepEqual(options(menu), ['✓ All · 3 statements', 'Claude Code · 2 statements', 'Codex · 1 statement'])
    await click(Array.from(menu.querySelectorAll('[role="menuitemradio"]')).find((o) => o.textContent.includes('Codex')))
    assert.equal(menuOf(el, 'Filter by agent'), null, 'picking closes the menu')
    const codexRows = rackRows(el)
    assert.equal(codexRows.length, 1)
    assert.ok(codexRows[0].includes('Lint is clean.'))
    assert.ok(groupHeads(el).every((g) => /, 1$/.test(g)), 'the group count is the rows shown: ' + groupHeads(el))
    assert.ok(button(el, 'Agent: Codex'), 'the button names the filter in force')
    assert.match(el.textContent, /Showing 1 of 3 · Agent: Codex/)

    await click(button(el, 'State'))
    const states = menuOf(el, 'Filter by state')
    const recorded = Array.from(states.querySelectorAll('[role="menuitemradio"]')).find((o) => o.textContent.includes('Recorded'))
    assert.ok(recorded.textContent.includes('0 statements'), 'a state counts the rows it would show with the Agent filter in force')
    await click(recorded)
    assert.equal(rackRows(el).length, 0)
    assert.deepEqual(groupHeads(el), ['Nothing matches these filters, 0'], 'nothing matching is said')
    assert.match(el.textContent, /State: Recorded · Agent: Codex · pick All in a filter to see every statement/)
    const none = Array.from(el.querySelectorAll('a')).find((a) => /^Open (this|the Brief)/.test(a.textContent))
    assert.equal(none.textContent, 'Open the Brief ›', 'with nothing selected the link says where it goes')
    assert.deepEqual(R.parse(none.getAttribute('href')), { screen: 'brief', change: TID, project: 'p1' })

    await click(button(el, 'Agent'))
    await click(Array.from(menuOf(el, 'Filter by agent').querySelectorAll('[role="menuitemradio"]')).find((o) => o.textContent.startsWith('All')))
    const rec = rackRows(el)
    assert.equal(rec.length, 1)
    assert.ok(rec[0].includes('Checkout total fixed.'))

    await click(button(el, 'Kind'))
    assert.deepEqual(options(menuOf(el, 'Filter by kind')), ['✓ All · 1 statement', 'What the agents said · 1 statement'])
    await click(button(el, 'State: Recorded'))
    await click(Array.from(menuOf(el, 'Filter by state').querySelectorAll('[role="menuitemradio"]')).find((o) => o.textContent.startsWith('All')))
    await click(button(el, 'Session'))
    const sessions = menuOf(el, 'Filter by session')
    assert.deepEqual(options(sessions), ['✓ All · 3 statements', 'Session 1 · Claude Code · 2 statements', 'Session 2 · Codex · 1 statement'])
    await click(Array.from(sessions.querySelectorAll('[role="menuitemradio"]')).find((o) => o.textContent.includes('Session 1')))
    assert.deepEqual(rackRows(el).map((r) => /Tests pass\.|Checkout total fixed\./.test(r)), [true, true])
  } finally {
    unmount()
  }
})

/* ======================= 6. A thread with no statements ======================= */

test('6 · Change-Brief: a thread with no statements is drawn in its frame, with an honest empty state and its links', () => {
  const s = snap({ claims: false })
  assert.equal(s.statements.length, 0)
  const h = liveHtml('Change-Brief', { s, data: s })
  // the frame: the crumb to the project's Inbox, the altitudes, the whole story, and the status bar
  assert.match(h, /<nav aria-label="Where you are"[\s\S]*?<a href="#\/inbox\?project=p1"/)
  assert.equal(altitudeOf(h).anchors.length, 3)
  assert.ok(h.includes('The whole story'))
  assert.match(textOf(h), /Recorded · version wt-2222222 · 0 statements · 4 steps/)
  // the honest empty state, from the record
  assert.match(textOf(h), /Nothing the agent said in this work claims it is finished, so there is nothing to check yet\./)
  assert.match(textOf(h), /The record holds 2 edits and 1 command\./)
  // its links, each to this Change
  const hrefs = Array.from(h.matchAll(/<a href="([^"]*)"[^>]*>(Read the whole story|Say what counts as done|See what changed ›)<\/a>/g)).map((m) => [m[2], R.parse(m[1]).screen, R.parse(m[1]).change])
  assert.deepEqual(hrefs, [['Read the whole story', 'story', TID], ['Say what counts as done', 'done', TID], ['See what changed ›', 'changed', TID]])
  assert.ok(!/❝|Proof stops here|① SAID/.test(h), 'no statement or chain is made up')
})

test('6 · Change-Brief: on its live address the empty Change is the frame, not a bare sentence', async () => {
  const docs = {
    [`/api/hq/projects/p1/threads/${TID}`]: thread({ claims: false }),
    '/api/hq/projects/p1': { schema_version: 1, project: { id: 'p1', name: 'shop', root: '/repo', members: [] }, threads: [] },
    '/api/storage': { bytes: 2.1e9, path: '/repo', parts: [] },
    '/api/changes': { changes: [] },
  }
  const prev = globalThis.fetch
  globalThis.fetch = async (url) => {
    const doc = docs[String(url)]
    return { ok: !!doc, status: doc ? 200 : 404, text: async () => JSON.stringify(doc || { error: 'not here' }), json: async () => doc || {} }
  }
  const { el, unmount } = M.mountBrief({ screen: 'brief', change: TID, project: 'p1' })
  try {
    for (let i = 0; i < 100 && !el.querySelector('h1'); i += 1) await wait(20)
    assert.ok(el.querySelector('nav[aria-label="Where you are"]'), 'the top bar is drawn')
    assert.ok(el.querySelector('[aria-label="Altitude"]'), 'the altitude group is drawn')
    assert.ok(el.querySelector('footer'), 'the status bar is drawn')
    assert.equal(el.querySelector('h1').textContent, 'Nothing the agent said in this work claims it is finished, so there is nothing to check yet.')
    assert.equal(el.querySelector('[role="alert"]'), null, 'not the bare sentence on its own')
  } finally {
    unmount()
    globalThis.fetch = prev
  }
})

/* ======================= 7. The full statement view ======================= */

test('7 · routes: the Brief’s address can name one statement, and only the Brief’s', () => {
  const h = R.href('brief', { change: TID, project: 'p1', statement: 'm1#1' })
  assert.deepEqual(R.parse(h), { screen: 'brief', change: TID, project: 'p1', statement: 'm1#1' })
  assert.equal(R.href('brief', { change: TID, project: 'p1' }), '#/changes/' + TID + '?project=p1', 'unchanged without one')
  assert.equal(R.parse('#/changes/' + TID + '/statements?statement=m1%231').statement, undefined)
})

test('7 · Every-Statement: “Open this in the full statement view” opens the Brief at the statement selected', async () => {
  const s = snap({ codex: true })
  const data = M.everyStatementFromChange(s)
  const { el, unmount } = M.mount('Every-Statement', { data, frame: {}, at: atOf('Every-Statement') }, linkOf('Every-Statement', s))
  try {
    const row = Array.from(el.querySelectorAll('section[aria-label="Every statement"] .tl-rack button[aria-pressed]')).find((b) => b.textContent.includes('Checkout total fixed.'))
    await click(row)
    const link = Array.from(el.querySelectorAll('a')).find((a) => a.textContent.startsWith('Open this in the full statement view'))
    assert.ok(link, 'the link is drawn')
    const r = R.parse(link.getAttribute('href'))
    assert.deepEqual(r, { screen: 'brief', change: TID, project: 'p1', statement: 'm1#1' })
  } finally {
    unmount()
  }
  // the Brief, at that address, shows that statement — not the brief's own ("Tests pass.")
  const brief = M.vals('Change-Brief', { data: s, frame: {}, at: { screen: 'brief', change: TID, statement: 'm1#1' } })
  assert.deepEqual(brief.saidStmts.filter((x) => x.pressed === 'true').map((x) => x.text), ['Checkout total fixed.'])
  const plain = M.vals('Change-Brief', { data: s, frame: {}, at: { screen: 'brief', change: TID } })
  assert.deepEqual(plain.saidStmts.filter((x) => x.pressed === 'true').map((x) => x.text), ['Tests pass.'])
  const unknown = M.vals('Change-Brief', { data: s, frame: {}, at: { screen: 'brief', change: TID, statement: 'no-such' } })
  assert.deepEqual(unknown.saidStmts.filter((x) => x.pressed === 'true').map((x) => x.text), ['Tests pass.'], 'a statement the record does not hold is not guessed at')
})
