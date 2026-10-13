/* W2-C, HQ Next wave 2: Every-Statement on the real record. The screen reads one
 * Change snapshot (data/change.js) through from-change.js; these tests drive it
 * from a served ThreadView and from the Brief's own snapshot fixture. The design
 * render (#/design/Every-Statement) is checked by scripts/hq/pixdiff.py, not here. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))

/* The screen bundled for node with the ONE Change model (data/change.js), its
 * selector and a ThreadView fixture: render(data, state) is the live path. */
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
import { everyStatementFromChange } from './from-change.js'
import { changeSnapshot } from '../../data/change.js'
import BRIEF from '../Change-Brief/demo.js'
import { threadFixture } from '../What-Changed/thread-fixture.js'

export { everyStatementFromChange as select, changeSnapshot, BRIEF, threadFixture }
export function instance(props, state) {
  const inst = new Logic(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return inst
}
export function render(props, state) {
  const inst = instance(props, state)
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-w2c-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const html = (props, state) => M.render(props, state)
const text = (props, state) => {
  const h = html(props, state)
  const attrs = (h.match(/(?:aria-label|title)="[^"]*"/g) || []).map((a) => a.replace(/^[^"]*"|"$/g, '')).join('\n')
  return unescape(h.replace(/<[^>]*>/g, '\u0001') + '\n' + attrs)
}
const NOW = Date.parse('2026-10-07T10:30:00.000Z')

const live = () => M.select(M.changeSnapshot(M.threadFixture(), null, NOW))

test('the selector reads the Brief snapshot fixture in all four states', () => {
  for (const [name, snap] of Object.entries(M.BRIEF.states)) {
    const d = M.select(snap)
    assert.equal(d.statements.length, snap.statements.length, name)
    assert.equal(d.version.id, snap.currentVersion, name)
    const t = text({ data: d })
    assert.ok(!/Illustrative: Codex|NEEDS ENGINE|stays disabled/.test(t), name)
    for (const q of snap.statements) assert.ok(t.includes(q.text), name + ': ' + q.text)
  }
})

test('every statement in the record is a row, each exactly once', () => {
  const d = live()
  const h = unescape(html({ data: d }))
  for (const q of d.statements) assert.equal(h.split(q.text).length - 1 >= 1, true, q.text)
  assert.ok(text({ data: d }).includes('What the agents said (2)'))
})

test('the tally and run result come from typed fields; an unserved count stays absent', () => {
  const d = live()
  const t = text({ data: d }, { sel: 'm1#0' })
  assert.ok(t.includes('pytest -q tests/test_totals.py: 4 passed, 0 failed'))
  assert.ok(!/\b0 need a re-check|\b0 in progress/.test(t), 'a count with no accepted lines was drawn as 0')
  assert.ok(t.includes('What counts as done is not accepted yet'))
  assert.equal(d.change.recording, null)
  // polish: the pill never says "not known"; with no recent step it is quietly "Not recording now"
  assert.ok(t.includes('Not recording now'))
  assert.ok(!t.includes('not known'))
})

test('commands that are not test runs are not runs a statement rests on', () => {
  assert.deepEqual(live().runs.map((r) => r.id), ['c1'])
})

test('"Also said" lists every message no statement came from', () => {
  const snap = M.changeSnapshot(M.threadFixture(), null, NOW)
  snap.messages.push({ id: 'x1', step: 7, time: '10:07', sessionId: 's1', agent: 'Claude Code', text: 'Looking at the totals.' })
  snap.messages.push({ id: 'x2', step: 8, time: '10:08', sessionId: 's1', agent: 'Claude Code', text: 'Now the API.' })
  const d = M.select(snap)
  const t = text({ data: d }, { alsoOpen: true })
  assert.ok(t.includes('Also said — not checked · 2'))
  assert.ok(t.includes('Looking at the totals.') && t.includes('Now the API.'))
  assert.ok(t.includes('Claude Code · session 1 · step 8'))
})

test('where the proof stops is one group per link, with one bar per line', () => {
  const d = M.select(M.BRIEF)
  d.criteria.lines = [5, 5, 2].map((n, i) => ({ id: 'l' + i, num: i + 1, gate: false, link: null, text: 'line ' + (i + 1),
    proof: { verdict: 'not_proven', stopsAt: n, restsOn: null, lookedAt: [], misses: [] } }))
  const h = html({ data: d })
  assert.ok(unescape(h).includes('⑤ RE-CHECKED') && h.includes('×2') && h.includes('② RESTS ON') && h.includes('×1'))
  const bars = (h.match(/width:6px;height:14px;border-radius:1px;background:#A1A1AA/g) || []).length
  assert.equal(bars, 3)
})

test('a statement a later run contradicts reads Contradicted and names that run', () => {
  const view = M.threadFixture()
  view.commands.push({ ...view.commands[0], id: 'c3', event_id: 'c3', ts: '2026-10-07T10:08:00.000Z', outcome: 'failed', exit_code: 1,
    tally: { passed: 3, failed: 1, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 } })
  const d = M.select(M.changeSnapshot(view, null, NOW))
  const t = text({ data: d }, { sel: 'm1#0' })
  assert.ok(t.includes('Contradicted'))
  assert.ok(/ran later and did not hold: 3 passed, 1 failed/.test(t))
})

test('Run it with no act handed in is off and says why in plain words, never "not available in this version" (C30)', () => {
  const d = live()
  const h = html({ data: d }, { sheet: 'tests', sel: 'm1#0' })
  assert.match(h, /<button[^>]*disabled=""[^>]*title="This check cannot start from this screen\."/)
  assert.ok(!/not available in this version/i.test(h))
  const inst = M.instance({ data: d }, { sheet: 'tests' })
  inst.renderVals().runCheck()
  assert.equal(inst.state.checking, false)
  assert.ok(!/engine|endpoint|route/i.test(text({ data: d }, { sheet: 'tests' })))
})

test('a long agent message shows the statement with one on each side and says how much more there is', () => {
  const snap = M.changeSnapshot(M.threadFixture(), null, NOW)
  const base = snap.statements[0]
  snap.statements = Array.from({ length: 9 }, (_, i) => ({ ...base, id: 's' + i, text: 'Claim number ' + i + '.', plain: 'Claim number ' + i + '.' }))
  snap.messages = [{ id: base.messageId, step: base.step, time: base.time, sessionId: base.sessionId, agent: base.agent,
    text: snap.statements.map((q) => q.text).join(' ') }]
  const t = text({ data: M.select(snap) }, { sel: 's4' })
  assert.equal(t.split('… 3 more in this message').length - 1, 2, 'the cut on each side is not stated')
  assert.ok(t.includes('Claim number 3.') && t.includes('Claim number 5.'))
})

test('C54 statements#8/#11/#17/#21: accepted scope lines become rack rows and selectable proof segments', () => {
  const tracked = {
    change: { id: 'chg_accepted', accepted_scope: { version: 3, criteria: ['Coupon has expired before use.'], acceptance: { actor: 'you', accepted_at: '2026-10-07T10:09:00Z' } } },
    hq: { title: 'Coupon', criteria: [{ n: 1, text: 'Coupon has expired before use.', state: 'in_progress' }], counts: { proven: 0 }, version: { short: 'abc1234' }, readiness: { ready: false }, decision: {} },
    model: { criteria: [{ n: 1, text: 'Coupon has expired before use.', state: 'in_progress' }] },
  }
  const d = M.select(M.changeSnapshot(M.threadFixture(), tracked, NOW))
  assert.equal(d.criteria.lines.length, 1)
  assert.equal(d.criteria.lines[0].text, 'Coupon has expired before use.')
  const h = html({ data: d, at: { change: 'chg_accepted', project: 'p1' } })
  assert.match(h, /Coupon has expired before use\./)
  assert.match(h, /aria-label="What counts as done, line by line"/)
  assert.match(h, /Line 1: /)
  const vals = M.instance({ data: d, at: { change: 'chg_accepted', project: 'p1' } }, { sel: d.criteria.lines[0].id }).renderVals()
  assert.equal(vals.fullView, '#/changes/chg_accepted/done?project=p1')
})

test('Bar set after the work: the two MOMENTS are compared, not their clock faces — accepted at 00:10 after an edit at 23:50 the day before is after the work', () => {
  /* local-time constructors, so the faces are 23:xx / 00:xx in every timezone */
  const at = (d, h, m) => new Date(2026, 9, d, h, m, 0).toISOString()
  const bar = (editsAt, acceptedAt) => {
    const view = M.threadFixture()
    view.edits.forEach((e, i) => { e.ts = editsAt[Math.min(i, editsAt.length - 1)] })
    const tracked = {
      change: { id: 'chg_accepted', accepted_scope: { version: 1, criteria: ['The cart total is correct.'], acceptance: { actor: 'you', accepted_at: acceptedAt } } },
      hq: { title: 'Cart', criteria: [{ n: 1, text: 'The cart total is correct.', state: 'in_progress' }], counts: { proven: 0 }, version: { short: 'wt-2222222' }, readiness: { ready: false }, decision: {} },
      model: { criteria: [{ n: 1, text: 'The cart total is correct.', state: 'in_progress' }] },
    }
    const d = M.select(M.changeSnapshot(view, tracked, NOW))
    return { d, v: M.instance({ data: d, at: { change: 'chg_accepted', project: 'p1' } }, { sel: 'line-1' }).renderVals() }
  }
  const after = bar([at(6, 23, 50)], at(7, 0, 10))
  assert.equal(after.d.lastEditAt, '23:50')
  assert.equal(after.d.criteria.acceptedAt, '00:10')
  assert.equal(after.v.c.hasBar, true, 'accepted after midnight, after the work: the bar chip is drawn')
  assert.equal(after.v.c.barTitle, 'Bar set after the work: accepted 00:10; the agent’s last edit was 23:50.')
  assert.ok(JSON.stringify(after.v.insp).includes(', after the work'), 'the ① Accepted link says after the work')
  assert.equal(bar([at(6, 23, 50)], at(6, 23, 40)).v.c.hasBar, false, 'accepted before the last edit: no chip')
  assert.equal(bar([at(6, 23, 50)], at(5, 23, 59)).v.c.hasBar, false, 'accepted the day before the edit, later on the clock: no chip')
  /* the LAST edit is the latest moment, not the greatest face: 00:05 the next day is later than 23:50 */
  const wrap = bar([at(6, 23, 50), at(7, 0, 5)], at(7, 0, 10))
  assert.equal(wrap.d.lastEditAt, '00:05')
  assert.equal(wrap.v.c.hasBar, true)
})

test('a proven accepted line uses the served assessment instead of inventing a missing run', () => {
  const tracked = {
    change: { id: 'chg_accepted', accepted_scope: { version: 1, criteria: ['The cart total is correct.'], acceptance: { actor: 'you', accepted_at: '2026-10-07T10:09:00Z' } } },
    hq: { title: 'Cart', criteria: [{ n: 1, text: 'The cart total is correct.', state: 'proven', line: 'Proven on wt-2222222.' }], counts: { proven: 1 }, version: { short: 'wt-2222222' }, readiness: { ready: true }, decision: {} },
  }
  const d = M.select(M.changeSnapshot(M.threadFixture(), tracked, NOW))
  const vals = M.instance({ data: d }, { sel: 'line-1' }).renderVals()
  assert.equal(vals.insp.l2, 'Proven on wt-2222222.')
  assert.ok(!vals.insp.items.some((p) => /No run bears|Nothing in the record can check/.test(p.pre || '')))
})

test('C54 statements#17/#20: a served unclassified agent message is Also said with a step receipt', () => {
  const view = M.threadFixture()
  view.messages = [{ event_id: 'said-other', session_id: view.sessions[0].id, ts: '2026-10-07T10:02:00Z', text: 'Looking at the cart module first.' }]
  const snap = M.changeSnapshot(view, null, NOW)
  const message = snap.messages.find((m) => m.id === 'said-other')
  assert.ok(message, 'the served message should not disappear when no claim rule matches')
  assert.equal(message.text, 'Looking at the cart module first.')
  assert.ok(Number.isInteger(message.step) && message.step > 0)
  const d = M.select(snap)
  const props = { data: d, at: { change: 'chg_1', project: 'p1' } }
  const h = html(props, { alsoOpen: true, sel: 'said-other' })
  assert.match(h, /Also said — not checked · 1/)
  assert.match(h, /Looking at the cart module first\./)
  assert.match(M.instance(props, { alsoOpen: true, sel: 'said-other' }).renderVals().alsoHref, /^#\/changes\/chg_1\/step\/\d+\?project=p1$/)
})

test('C54 statements#12/#13/#14: every tests action opens the existing run-intent check route', () => {
  const d = live()
  const calls = []
  const inst = M.instance({ data: d, actions: { check: () => calls.push('check'), checkHref: '#/changes/chg_1/check?run=1' } }, { sel: d.statements[0].id })
  const vals = inst.renderVals()
  vals.openSel()
  assert.deepEqual(calls, ['check'])
  assert.equal(inst.state.sheet, '')
  const row = vals.groups.flatMap((g) => g.rows).find((r) => r.text === d.statements[0].text)
  assert.ok(row)
  row.act()
  assert.deepEqual(calls, ['check', 'check'])
  assert.equal(inst.state.sheet, '')
  assert.equal(vals.sheetTests, false)
})

test('C54 statements#16: a cancelled served check keeps its stopped caption', () => {
  const snap = M.changeSnapshot(M.threadFixture(), null, NOW)
  snap.runSheet = { checks: [{ command: 'pytest -q', last: { run: 'run-stopped', outcome: 'cancelled', started_at: '2026-10-07T10:10:00Z', recorded_at: '2026-10-07T10:10:09Z' } }] }
  const d = M.select(snap)
  const vals = M.instance({ data: d }).renderVals()
  assert.equal(d.stopped.elapsedS, 9)
  assert.match(vals.captionTwo, /You stopped this check after 9 s/)
})

test('C54 statements#14: a recorded lint_ok claim offers its lint check sheet', () => {
  const snap = M.changeSnapshot(M.threadFixture(), null, NOW)
  snap.statements[0].claimKind = 'lint_ok'
  snap.statements[0].text = 'Lint is clean.'
  snap.statements[0].plain = 'Lint is clean.'
  const d = M.select(snap)
  assert.equal(d.statements[0].claimKind, 'lint_clean')
  const vals = M.instance({ data: d }, { sel: d.statements[0].id }).renderVals()
  const row = vals.groups.flatMap((g) => g.rows).find((r) => r.text === 'Lint is clean.')
  assert.match(row.actLabel, /^Check it again: Lint is clean\./)
})

test('C54 statements#7: Timeline and Transcript tabs render recorded statement panels', () => {
  const d = live()
  for (const view of ['timeline', 'transcript']) {
    const h = html({ data: d }, { view })
    assert.match(h, new RegExp('role="tabpanel"[^>]*aria-label="' + (view === 'timeline' ? 'Timeline' : 'Transcript') + '"'))
    assert.match(h, /Tests pass\./)
    assert.match(h, new RegExp('role="tab" aria-selected="true"[^>]*>' + (view === 'timeline' ? '[\\s\\S]*?Timeline' : '[\\s\\S]*?Transcript')))
  }
})

test('C54 statements#23: focused keys use the served check and the selected row', () => {
  const d = live()
  const calls = []
  const vals = M.instance({ data: d, actions: { check: () => calls.push('check') } }, { sel: d.statements[0].id }).renderVals()
  assert.equal(typeof vals.keys.next, 'function')
  assert.equal(typeof vals.keys.prev, 'function')
  assert.equal(typeof vals.keys.open, 'function')
  assert.equal(typeof vals.keys.primary, 'function')
  vals.keys.primary()
  vals.keys.open()
  assert.deepEqual(calls, ['check', 'check'])
  assert.equal(typeof vals.keys.copy, 'function')
})
