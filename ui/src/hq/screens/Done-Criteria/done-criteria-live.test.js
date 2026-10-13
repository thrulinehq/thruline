/* W2-D, HQ Next wave 2: Done-Criteria on the real record.
 *
 * The selector reads the ONE Change snapshot (data/change.js) — fed here both
 * the canvas's own story (Change-Brief/demo.js, every state) and a synthetic
 * served ThreadView pushed through `changeSnapshot` — and the tracked Change's
 * proposals, accepted version, coverage and model. The writes are proven against a stubbed `fetch`; nothing here reaches
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
import { selectDoneCriteria } from './from-change.js'
import { acceptCriteria, linkRun, SCOPE_ACCEPT, CHANGE_LINKS } from './actions.js'
import { href } from '../../routes.js'

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
  const file = join(await mkdtemp(join(tmpdir(), 'done-criteria-live-')), 'screen.mjs')
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
const DESIGN_NOTES = ['Demo state', 'Design note', 'shown as designed']
const UI = [undefined, { face: 'technical' }, { tplOpen: true }, { composer: 'line' }, { composer: 'scope' }]

const proposed = () => ({
  id: 'chg_1',
  change: { accepted_scope: null, code_moved: null },
  proposals: { criteria: [{ number: 1, text: 'Each import row posts once.' }, { number: 2, text: 'Re-running an import posts nothing new.' }], notes: [{ text: 'make it right' }], scope: { paths: ['src/'] } },
  coverage: { criteria: [], candidates: [] },
  model: null,
})
const accepted = () => ({
  id: 'chg_1',
  change: { accepted_scope: { version: 1, allowed_scope: ['src/'], acceptance: { actor: 'robin', accepted_at: T(8) } }, code_moved: null },
  proposals: null,
  coverage: { criteria: [{ n: 1, items: [] }], candidates: [{ kind: 'check_run', execution: 'completed', bears: true, observed: 'proven', evidence_id: 'chk_run_9', name: 'pytest' }] },
  model: { criteria: [
    { n: 1, text: 'Each import row posts once.', state: 'in_progress', word: 'In progress', line: 'A test ran for this Change but isn’t linked to this line — link it.', unlinked: true },
    { n: 2, text: 'Re-running an import posts nothing new.', state: 'proven', word: 'Proven', line: 'Proven on wt-2222222.' },
  ] },
})

test('the canvas story, selected from the one snapshot: the record’s request, claim and runs; no lines until a Change is tracked', () => {
  const d = selectDoneCriteria(BRIEF.states.s1, null)
  assert.equal(d.request.text, 'the checkout total is wrong when a discount code is applied — fix it')
  assert.equal(d.claim.text, 'Checkout total fixed.')
  assert.deepEqual([d.lastRun.step, d.lastRun.version], [46, '9f2ad3e'])
  assert.deepEqual([d.earlierRun.step, d.earlierRun.failed], [26, 1])
  assert.equal(d.criteria.lines.length, 0, 'no tracked Change: nothing is proposed, and nothing is invented')
  assert.deepEqual(d.covers.map((c) => c.path), ['cart/', 'checkout/'])
})

test('tracked, proposed: the proposals arrive as proposed lines, nothing accepted for the person', () => {
  const d = selectDoneCriteria(snap(), proposed())
  assert.deepEqual(d.criteria.lines.map((l) => [l.n, l.text, l.state]), [[1, 'Each import row posts once.', 'proposed'], [2, 'Re-running an import posts nothing new.', 'proposed']])
  assert.equal(d.criteria.recorded, false)
  assert.deepEqual(d.notCheckable, { quote: 'make it right', shownBy: [] })
  assert.deepEqual(d.covers.map((c) => c.path), ['src/'])
  const out = text({ data: d })
  assert.ok(out.includes('0 of 2 accepted'))
  assert.ok(out.includes('“make it right” — no command can observe it.'))
})

test('tracked, accepted: each line reads the engine’s own state and sentence', () => {
  const d = selectDoneCriteria(snap(), accepted())
  assert.equal(d.criteria.recorded, true)
  assert.deepEqual(d.act.unlinkedRun, { id: 'chk_run_9', name: 'pytest' })
  const out = text({ data: d })
  assert.ok(out.includes('A test ran for this Change but isn’t linked to this line — link it.'))
  assert.ok(out.includes('Proven on wt-2222222.'))
  assert.ok(out.includes('1 in progress · 1 proven on'), out.match(/.{0,40}in progress.{0,40}/))
})

test('Bar set after the work: the two MOMENTS are compared, not their clock faces — accepted at 00:10 after an edit at 23:50 the day before is after the work', () => {
  /* local-time constructors, so the two faces are 23:50 and 00:10 in every timezone */
  const at = (d, h, m) => new Date(2026, 9, d, h, m, 0).toISOString()
  const bar = (acceptedAt) => {
    const t = thread()
    t.edits[0].ts = at(6, 23, 50)
    const tr = accepted()
    tr.change.accepted_scope.acceptance.accepted_at = acceptedAt
    const data = selectDoneCriteria(changeSnapshot(t, null, new Date(2026, 9, 7, 1, 0), { storageBytes: 3 * 1024 ** 3 }), tr)
    return { data, v: M.vals({ at: { change: 'chg_1', project: 'p1' }, data }) }
  }
  const after = bar(at(7, 0, 10))
  assert.equal(after.data.agent.lastEdit.at, '23:50')
  assert.equal(after.data.criteria.acceptedAt, '00:10')
  assert.equal(after.v.showBarChip, true, 'accepted after midnight, after the work: the chip is drawn')
  assert.equal(after.v.barChip, 'Bar set after the work: accepted 00:10; the agent’s last edit was 23:50.')
  assert.equal(bar(at(6, 23, 40)).v.showBarChip, false, 'accepted before the last edit: no chip')
  assert.equal(bar(at(5, 23, 59)).v.showBarChip, false, 'accepted the day before the edit, later on the clock: no chip')
})

test('live request and run receipts open their exact Step View, including the accepted now-box', () => {
  const at = { change: 'chg_1', project: 'p1' }
  const proposedData = selectDoneCriteria(snap(), proposed())
  const request = href('step', { ...at, step: proposedData.request.step })
  const run = href('step', { ...at, step: proposedData.lastRun.step })
  const p = M.vals({ at, data: proposedData })
  assert.equal(p.requestStepHref, request)
  assert.equal(p.cards[0].receiptHref, run)
  assert.equal(p.cards[0].hasReceipt, true)
  const before = html({ at, data: proposedData })
  assert.equal(before.split(`href="${request}"`).length - 1, 2, 'header and Request rail open the request step')
  assert.ok(before.includes(`href="${run}"`), 'In the record now opens the run step')

  const acceptedData = selectDoneCriteria(snap(), accepted())
  const a = M.vals({ at, data: acceptedData })
  assert.equal(a.cards[0].nowReceiptHref, run)
  assert.equal(a.cards[0].hasNowReceipt, true)
  const after = html({ at, data: acceptedData })
  assert.ok(after.includes(`href="${run}"`), 'the recorded now-box opens the run step')
})

test('a check run owned by ThruLine has no invented step receipt or Link step null', () => {
  const at = { change: 'chg_1', project: 'p1' }
  const data = selectDoneCriteria(snap(), accepted())
  data.lastRun = { ...data.lastRun, step: null, capturedBy: 'thruline' }
  const v = M.vals({ at, data })
  assert.equal(v.nowReceipt, '')
  assert.equal(v.cards[0].hasNowReceipt, false)
  assert.equal(v.cards[0].nowReceiptHref, undefined)
  assert.equal(v.linkNowLabel, 'Link check run')
  assert.equal(v.cards[0].linkAria, 'Link check run to line 1')
  assert.equal(v.cards[0].linkLabel, 'Link check run')
  for (const state of [undefined, { face: 'technical' }]) {
    const out = text({ at, data }, state)
    assert.ok(!/\bstep null\b|Link step null/.test(out), out.match(/.{0,40}step null.{0,40}/))
    assert.ok(out.includes('Link check run'))
  }
  const unrecorded = selectDoneCriteria(snap(), proposed())
  unrecorded.lastRun = { ...unrecorded.lastRun, step: null, capturedBy: 'thruline' }
  const before = M.vals({ at, data: unrecorded })
  assert.equal(before.cards[0].hasReceipt, false)
  assert.ok(!/\bstep null\b/.test(text({ at, data: unrecorded })))

  const fractional = selectDoneCriteria(snap(), proposed())
  fractional.request.step = 0
  fractional.lastRun.step = 2.5
  const invalid = M.vals({ at, data: fractional })
  assert.equal(invalid.requestStepHref, undefined)
  assert.equal(invalid.requestStepLink, '')
  assert.equal(invalid.cards[0].hasReceipt, false)
})

test('the live path: real content, no design notes, no engine words, no trial, no hole — in every panel', () => {
  for (const t of [null, proposed(), accepted()]) for (const st of UI) {
    const data = selectDoneCriteria(snap(), t)
    const out = text({ data }, st)
    const tag = (t ? t.change.accepted_scope ? 'accepted' : 'proposed' : 'untracked') + ' ' + JSON.stringify(st)
    for (const a of DESIGN_NOTES) assert.ok(!out.includes(a), `${tag}: ${a}`)
    assert.ok(!ENGINE_WORDS.test(out), `${tag}: engine words: ${out.match(ENGINE_WORDS)}`)
    assert.ok(!/trial/i.test(out), `${tag}: trial`)
    assert.ok(!/\b(undefined|null|NaN)\b/.test(out), `${tag}: hole: ${out.match(/.{40}\b(undefined|null|NaN)\b.{20}/)}`)
    for (const canvas of ['checkout-web', 'e71c5b0', '9f2ad3e', 'cart/', 'test_single_discount', 'Bug fix: reproduces', '1.4 GB', '16:06']) assert.ok(!out.includes(canvas), `${tag}: canvas value ${canvas}`)
  }
})

test('Accept: the person’s decisions, wording and folders reach scope/accept, and nothing else', () => {
  const data = selectDoneCriteria(snap(), proposed())
  const sent = []
  const v = M.vals({ data, actions: { accept: (draft) => sent.push(draft) } }, { lines: ['accepted', 'dropped'], texts: ['Every row posts exactly once.', ''] })
  assert.equal(v.showAcceptBtn, true)
  v.recordV()
  assert.deepEqual(sent, [{ accepted: [1], edits: { 1: 'Every row posts exactly once.' }, folders: ['src/'], replacing: false }])
})

test('Link: only an accepted line with an unlinked passing run offers it, and it names that run', () => {
  const sent = []
  const pre = M.vals({ data: selectDoneCriteria(snap(), proposed()) })
  assert.ok(pre.cards.every((c) => !c.canLink && !c.canLinkNow), 'nothing to link before the version is accepted')
  const v = M.vals({ data: selectDoneCriteria(snap(), accepted()), actions: { link: (n) => sent.push(n) } })
  assert.deepEqual(v.cards.map((c) => c.canLinkNow), [true, false])
  v.cards[0].toggleLink()
  assert.deepEqual(sent, [1])
})

function stub() {
  const calls = []
  const f = async (url, init = {}) => {
    calls.push({ url, method: init.method || 'GET', body: init.body ? JSON.parse(init.body) : null })
    return { ok: true, status: 200, json: async () => ({}) }
  }
  return { f, calls }
}

test('scope/accept: the accepted numbers, the person’s wording, the kept folders, a name', async () => {
  const { f, calls } = stub()
  await acceptCriteria({ changeId: 'chg_1', accepted: [1, 3], edits: { 1: ' Every row posts once. ', 2: 'not accepted', 3: '' }, folders: ['src/'], replacing: true }, f)
  assert.deepEqual(calls, [{ url: '/api/changes/chg_1/scope/accept', method: 'POST', body: { criteria: [1, 3], actor: 'you', edits: { 1: 'Every row posts once.' }, allowed_scope: ['src/'], replacing: true } }])
  assert.equal(SCOPE_ACCEPT('x'), '/api/changes/x/scope/accept')
})

test('links: one check run to one accepted line, link or unlink, with a name', async () => {
  const { f, calls } = stub()
  await linkRun({ changeId: 'chg_1', criterion: 2, evidence: 'chk_run_9' }, f)
  await linkRun({ changeId: 'chg_1', criterion: 2, evidence: 'chk_run_9', unlink: true, actor: 'robin' }, f)
  assert.deepEqual(calls.map((c) => c.body), [
    { criterion: 2, kind: 'check_run', evidence: 'chk_run_9', act: 'link', actor: 'you', reason: '' },
    { criterion: 2, kind: 'check_run', evidence: 'chk_run_9', act: 'unlink', actor: 'robin', reason: '' },
  ])
  assert.ok(calls.every((c) => c.url === '/api/changes/chg_1/links' && c.method === 'POST'))
  assert.equal(CHANGE_LINKS('x'), '/api/changes/x/links')
})

test('no write without a Change, an accepted line or a run', async () => {
  const { f, calls } = stub()
  await acceptCriteria({ changeId: null, accepted: [1] }, f)
  await acceptCriteria({ changeId: 'c', accepted: [] }, f)
  await linkRun({ changeId: 'c', criterion: 1, evidence: null }, f)
  await linkRun({ changeId: null, criterion: 1, evidence: 'e' }, f)
  assert.equal(calls.length, 0)
})

test('census: in this folder only actions.js writes, and it reaches exactly two addresses', () => {
  for (const n of readdirSync(here).filter((n) => /\.(js|jsx)$/.test(n) && !n.endsWith('.test.js') && n !== 'actions.js')) {
    assert.ok(!/method:\s*['"]POST['"]/.test(readFileSync(join(here, n), 'utf8')), `${n} must not POST`)
  }
  const a = readFileSync(join(here, 'actions.js'), 'utf8')
  const addresses = [...a.matchAll(/'\/api\/changes\/' \+ encodeURIComponent\(id\) \+ '([^']*)'/g)].map((m) => m[1])
  assert.deepEqual(addresses.sort(), ['/links', '/scope/accept'])
})
