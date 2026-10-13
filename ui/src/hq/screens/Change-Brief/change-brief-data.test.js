/* W1-B, HQ Next wave 1: the Change-Brief's content arrives through one object.
 * The design gallery (no `data`) still draws the canvas word for word; the live
 * path (`data` present) draws only what `data` says and none of the canvas's
 * design annotations. The pixel match against
 * docs/hq-next/reference/Change-Brief.png is checked by scripts/hq/pixdiff.py,
 * not here. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))

const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
import { DEMO } from './demo.js'
import { withFrame } from '../../data/project.js'
export { DEMO }
export function renderFramed(props, state) {
  const L = withFrame(Logic)
  const inst = new L(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}
export function vals(props, state) {
  const inst = new Logic(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return inst.renderVals()
}
export function render(props, state) {
  return renderToStaticMarkup(React.createElement(Template, { v: vals(props, state), link: (h) => h }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'change-brief-data-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state))
const clone = (x) => JSON.parse(JSON.stringify(x))
const STATES = ['s1', 'checking', 'ready', 'moved']
const FRAGS = ['stmt-52-tests', 'stmt-52-fixed']
const OPEN = { why: true, then: true, kind: true, after: true, tech: true }
/** Every view a person can reach with one snapshot: both statements, every
 *  disclosure row open, Technical on, the held links shown. */
const everyView = (props) => FRAGS.map((frag) => text(props, { frag, tech: true, open: OPEN, showHeld: true })).join('\n')
const live = (state) => { const d = clone(M.DEMO.states[state]); return d }

test('C54 brief#3: a moved Change with an active check draws the checking and Stop state', () => {
  const d = live('moved')
  const running = clone(M.DEMO.states.checking.runs.find((r) => r.status === 'running'))
  running.version = d.currentVersion
  d.runs.push(running)
  d.statements[0].proof.recheck = { status: 'running', runId: running.id }
  d.primaryAction = 'stop_check'
  const v = M.vals({ data: d, actions: { stopCheck() {} } })
  assert.equal(v.pChecking, true)
  assert.equal(v.pMoved, false)
  assert.equal(v.fStop, true)
  assert.equal(v.fCheck, false)
  assert.match(v.statusLeft, /check running/)
})

test('C54 brief#12: Open in editor links to the selected statement\'s recorded edit file and hunk line', () => {
  const d = live('s1')
  d.project.id = 'project-test'
  d.change.threadId = '0123456789abcdef'
  d.statements[0].text = 'I fixed src/receipt.py at the recorded edit.'
  d.statements[0].plain = d.statements[0].text
  d.edits.push({ id: 'editor-edit', step: 51, file: 'src/receipt.py', editorPath: '/Users/test/repo/src/receipt.py', sessionId: d.statements[0].sessionId, hunks: [{ new_start: 7, new_lines: 1 }] })
  const v = M.vals({ data: d })
  assert.equal(v.editorHref, 'vscode://file/Users/test/repo/src/receipt.py:7:1')
  /* The absolute path never reaches the markup (home-paths.test.js census): the item is a
   * button, and the OS link is made only when it is pressed. */
  const html = M.render({ data: d }, { more: true })
  assert.match(html, /<button type="button" role="menuitem"[^>]*>Open in editor<\/button>/)
  assert.ok(!html.includes('vscode:') && !html.includes('/Users/test'), 'no editor URL or home folder in the markup')
  assert.equal(typeof v.openInEditor, 'function')
  const made = []
  const prior = globalThis.document
  globalThis.document = { body: { appendChild() {} }, createElement: () => { const a = { click() { made.push(a.href) }, remove() {} }; return a } }
  try { v.openInEditor() } finally { if (prior === undefined) delete globalThis.document; else globalThis.document = prior }
  assert.deepEqual(made, ['vscode://file/Users/test/repo/src/receipt.py:7:1'])
})

test('C54 brief#25: drawn Brief keys have the screen handlers behind them', () => {
  const calls = []
  const d = live('s1')
  const v = M.vals({ data: d, at: { change: 'chg_test', project: 'project-test' }, actions: {
    check: () => calls.push('check'), tellAgent: () => calls.push('tell'),
  } })
  for (const key of ['primary', 'copy', 'next', 'prev', 'expand', 'approve']) {
    assert.equal(typeof v.keys[key], 'function', key)
  }
  v.keys.primary()
  v.keys.copy()
  assert.deepEqual(calls, ['check', 'tell'])
})

test('R-KEYS . and y: the keys do what the proof panel’s own buttons do, whichever statement the inspector holds', () => {
  /* The panel under the proof chain draws "Check it again ." and "Tell the agent y" for the Change; the inspector's footer
   * draws them only for a statement a rule can check. With the agent's "fixed" claim selected (no verifier) the footer
   * has neither, and the panel still does: the keys have to follow the buttons drawn, not the footer. */
  for (const state of ['s1', 'moved']) {
    const calls = []
    const v = M.vals({ data: live(state), at: { change: 'chg_test', project: 'project-test' }, actions: {
      check: () => calls.push('check'), tellAgent: () => calls.push('tell'),
    } }, { frag: 'stmt-52-fixed' })
    assert.equal(v.fCheck, false, state + ': the inspector footer draws no check for a claim no rule can check')
    assert.equal(v.pStop || v.pMoved, true, state + ': the panel draws Check it again and Tell the agent')
    assert.notEqual(v.keys.primary(), false, state + ': . is taken')
    assert.notEqual(v.keys.copy(), false, state + ': y is taken')
    assert.deepEqual(calls, ['check', 'tell'], state)
  }
  /* where the panel draws neither (checking, ready), the keys still leave the press to the page */
  for (const state of ['checking', 'ready']) {
    const calls = []
    const v = M.vals({ data: live(state), at: { change: 'chg_test', project: 'project-test' }, actions: {
      check: () => calls.push('check'), tellAgent: () => calls.push('tell'),
    } }, { frag: 'stmt-52-fixed' })
    assert.equal(v.keys.primary(), false, state + ': . has no Check it again to press')
    assert.equal(v.keys.copy(), false, state + ': y has no Tell the agent to press')
    assert.deepEqual(calls, [], state)
  }
})

test('R-KEYS t: the Brief’s Plain / Technical button keeps its visible name (no aria-label), live and in the gallery', () => {
  /* The reviewed census rows brief#15 / brief#25 find the button by its visible name; an aria-label would override it. */
  const buttonOf = (html) => {
    const at = html.indexOf('title="Plain / Technical · t"')
    assert.ok(at > 0, 'the Brief draws its Plain / Technical button')
    return html.slice(html.lastIndexOf('<button', at), html.indexOf('</button>', at))
  }
  const visibleName = (button) => button.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ').trim()
  const store = (face) => ({ prefs: { prefs: { face } }, setPreference: () => Promise.resolve({ ok: true }) })
  const at = { change: 'chg_test', project: 'project-test' }
  const plain = buttonOf(M.renderFramed({ data: live('s1'), at, ...store('plain') }))
  const tech = buttonOf(M.renderFramed({ data: live('s1'), at, ...store('technical') }))
  const gallery = buttonOf(M.render())
  for (const [label, b] of [['plain', plain], ['technical', tech], ['gallery', gallery]]) {
    assert.ok(!/aria-label=/.test(b), label + ': the button carries no aria-label, so its accessible name is its visible text')
    assert.match(visibleName(b), /Plain/, label + ': the visible name says Plain')
    assert.match(visibleName(b), /Technical/, label + ': the visible name says Technical')
  }
  assert.match(plain, /aria-pressed="false"/)
  assert.match(tech, /aria-pressed="true"/)
})

/* The canvas's design annotations: never on the live path. */
const ANNOTATIONS = ['Demo records', 'Demo state', 'About this demo', 'Needs engine', 'needs engine', 'illustrative', 'S1 · 16:04 Not proven', 'K-ready · 16:09 proven', 'diff route']

test('the design gallery still draws the canvas default, annotations included', () => {
  const t = text()
  for (const s of [
    'checkout-web', 'Fix double-applied discount on checkout total', 'The tests passed on 9f2ad3e. checkout/api.py changed after that, and nothing has run on e71c5b0.',
    'Looked at: 2 finished runs (step 26 on 4b19a0c, step 46 on 9f2ad3e) and step 60, which has no result yet.',
    'Request · 3', 'Work · 4', 'Execution · 3', 'Claims & evidence · 4', 'Claude Code · step 52 · 16:03 · one message, two statements',
    'Not proven · version e71c5b0 · 2 statements · 19 steps · recording', 'ThruLine is keeping 1.4 GB of records on this machine',
    'Demo records · illustrative, not a real agent run', 'Demo state', 'About this demo.',
  ]) assert.ok(t.includes(s), s)
})

test('the live path shows none of the design annotations, in any state', () => {
  for (const st of STATES) {
    const t = everyView({ data: live(st) })
    for (const a of ANNOTATIONS) assert.ok(!t.includes(a), st + ': ' + a)
  }
})

test('content flows from props.data: the title and project are the data’s', () => {
  const d = live('s1')
  d.change.title = 'Stand-in title from the store'
  d.project.name = 'stand-in-project'
  d.project.worktree = '~/w/stand-in'
  const t = text({ data: d })
  assert.ok(t.includes('Stand-in title from the store'))
  assert.ok(t.includes('stand-in-project'))
  assert.ok(!t.includes('Fix double-applied discount'))
  assert.ok(!t.includes('checkout-web'))
})

/* Every record-borne token the canvas shows, renamed in the data: none of the
 * old spellings may survive anywhere on any reachable view. A literal left in
 * template.jsx or logic.js fails this. */
const TOKENS = [
  ['~/code/checkout-web', '~/w/x'], ['checkout-web', 'proj-x'], ['Fix double-applied discount on checkout total', 'Title X'], ['9f2ad3e', 'aaaaaa1'], ['e71c5b0', 'bbbbbb2'],
  ['4b19a0c', 'cccccc3'], ['a1b2c3d', 'dddddd4'], ['3f9a1c2', 'eeeeee5'], ['checkout/api.py', 'src/x.rs'], ['cart/totals.py', 'src/y.rs'],
  ['tests/test_totals.py', 'tests/z.rs'], ['pytest -q', 'cargo test'], ['Claude Code', 'Codex'], ['16:03', '09:33'], ['16:04', '09:44'],
  ['16:01', '09:11'], ['16:06', '09:66'], ['16:09', '09:99'], ['16:11', '09:21'], ['hook v0.4', 'hook v9.9'], ['afb960f9', 'f00dfeed'],
  ['c324df66', 'beefcafe'], ['discount code', 'coupon'], ['checkout/', 'lib/'], ['2026-09-26', '2031-01-02'],
  ['Checkout total fixed.', 'All done here.'], ['Tests pass.', 'Specs pass.'], ['rm -rf .pytest_cache', 'make clean'],
]
const rename = (d) => {
  let s = JSON.stringify(d)
  for (const [a, b] of TOKENS) s = s.split(a).join(b)
  return JSON.parse(s)
}

test('no record-borne token is a literal: renamed in data, gone from every view', () => {
  const all = STATES.map((st) => [st, everyView({ data: rename(live(st)) })])
  for (const [a, b] of TOKENS) {
    for (const [st, t] of all) assert.ok(!t.includes(a), st + ': literal survives: ' + a)
    assert.ok(all.some(([, t]) => t.includes(b)), 'renamed token never drawn: ' + b)
  }
})

test('numbers come from the typed record fields, not from prose', () => {
  const d = live('s1')
  d.runs.find((r) => r.id === 'run-46').passed = 9
  d.machine.recordsBytes = 2_300_000_000
  d.change.stepCount = 27
  const t = text({ data: d })
  assert.ok(t.includes('9 passed, 0 failed'))
  assert.ok(!t.includes('4 passed, 0 failed'))
  assert.ok(t.includes('ThruLine is keeping 2.3 GB'))
  assert.ok(t.includes('2 statements · 27 steps'))
})

test('the lane graph and lit thread are computed from the nodes and edges, and match the canvas', () => {
  const v = M.vals()
  assert.deepEqual(v.edges.map((e) => e.d), [
    'M 134 210 C 148 210, 146 72, 160 72', 'M 316 136 C 323 136, 321 72, 328 72', 'M 468 42 C 468 30, 512 30, 512 42',
    'M 406 200 L 406 101', 'M 280 200 L 280 165', 'M 840 184 C 800 184, 800 175, 770 175 L 470 175 C 450 175, 440 185, 440 200',
  ])
  assert.deepEqual(v.threadPaths.map((p) => p.d), [
    'M 574 101 L 574 128', 'M 567 128 L 581 128', 'M 567 152 L 581 152', 'M 574 152 L 574 200',
    'M 468 42 C 468 30, 512 30, 512 42', 'M 406 200 L 406 101', 'M 840 184 C 800 184, 800 175, 770 175 L 470 175 C 450 175, 440 185, 440 200',
  ])
  const moved = M.vals(undefined, { demo: 'moved' })
  assert.deepEqual(moved.threadPaths.map((p) => p.d).slice(0, 5), [
    'M 738 101 L 738 128', 'M 731 128 L 745 128', 'M 731 152 L 745 152', 'M 738 152 L 738 200', 'M 636 42 C 636 30, 680 30, 680 42',
  ])
  // a node the data drops leaves the map, and its edges with it
  const d = live('s1')
  d.graph.nodes = d.graph.nodes.filter((n) => n.id !== 'run-26')
  const w = M.vals({ data: d })
  assert.ok(!w.edges.some((e) => e.d.startsWith('M 280 ')), 'the dropped run still has an edge')
  assert.ok(w.edges.some((e) => e.d === 'M 406 200 L 406 101'), 'a kept edge is gone')
  assert.equal(w.laneExec, '2')
})

test('the brief headline and the inspector follow the statement the data names', () => {
  const d = live('s1')
  const t = text({ data: d }, { frag: 'stmt-52-fixed' })
  assert.ok(t.includes('The record can’t check “fixed” by itself.'))
  assert.ok(t.includes('2 of 2'))
})

/* W2-B: the Brief on snapshots the one Change model builds from a served
 * ThreadView — every proof shape draws, in plain words, with no design note. */
import { changeSnapshot } from '../../data/change.js'
const TS = (m) => new Date(Date.UTC(2026, 9, 6, 12, m)).toISOString()
const CUR = 'wt-1111111111111111'
function thread({ commands = [], edits = [], claims = [] }) {
  return {
    project: { id: 'p', name: 'proj', root: '/r' },
    thread: { id: 'abcdefabcdefabcd', title: 'Fix the checkout total', current_version_id: CUR, proof: { edited_after_last_pass: [] } },
    prompts: [{ event_id: 'p0', session_id: 's', ts: TS(0), text: 'fix it', work: true, version_id: CUR }],
    edits: edits.map((e, i) => ({ id: 'e' + i, ts: TS(e.m), file: '/r/' + e.file, version_id: CUR, session_id: 's' })),
    commands: commands.map((c, i) => ({ id: 'c' + i, ts: TS(c.m), command: 'pytest -q', outcome: c.outcome || 'succeeded', exit_code: null, is_test: true, tally: c.tally === undefined ? { passed: 3, failed: 0 } : c.tally, version_id: CUR, session_id: 's' })),
    claims: claims.map((c, i) => ({ id: 'k' + i, event_id: 'k' + i, ts: TS(c.m), text: c.text || 'Tests pass.', kind: c.kind || 'tests_pass', has_verifier: c.v !== false, rests_on_command_id: c.rests ?? null, version_id: CUR, session_id: 's' })),
    versions: [{ id: CUR, committed: false }], current_version_id: CUR,
    sessions: [{ id: 's', source: 'claude_code', origin: 'person', started_at: TS(0) }], files: [],
  }
}
const SHAPES = {
  stopsAt5: thread({ commands: [{ m: 1 }], edits: [{ m: 2, file: 'api.py' }], claims: [{ m: 3, rests: 'c0' }] }),
  proven: thread({ commands: [{ m: 1 }], claims: [{ m: 2, rests: 'c0' }] }),
  contradicted: thread({ commands: [{ m: 1 }, { m: 4, outcome: 'failed', tally: { passed: 2, failed: 1 } }], claims: [{ m: 2, rests: 'c0' }] }),
  noResult: thread({ commands: [{ m: 1, outcome: 'unknown', tally: null }], claims: [{ m: 2, rests: 'c0' }] }),
  noRun: thread({ claims: [{ m: 2 }] }),
  recorded: thread({ claims: [{ m: 2, kind: 'done', v: false, text: 'All done.' }] }),
}
const ENGINE_WORDS = /needs engine|\bengine\b|endpoint|\broute\b|illustrative|Demo/

test('every proof shape from the live model draws in plain words, with no design note', () => {
  for (const [name, v] of Object.entries(SHAPES)) {
    const data = changeSnapshot(v, null, new Date(Date.UTC(2026, 9, 6, 13)))
    const t = everyView({ data })
    assert.ok(t.includes('Fix the checkout total'), name)
    assert.ok(!ENGINE_WORDS.test(t.replace(/Map of the record[^"]*/g, '')), name + ': ' + (t.match(ENGINE_WORDS) || [])[0])
    assert.ok(!/undefined|null|NaN/.test(t.replace(/aria-[a-z]+="[^"]*"/g, '')), name + ': a missing fact leaked as a word')
  }
})

test('verdicts: proven is green only at ⑥; contradicted is ✕ coral; a run with no result never counts', () => {
  const v = (shape) => M.vals({ data: changeSnapshot(SHAPES[shape], null, Date.now()) })
  const p = v('proven')
  /* Proven is drawn from the one token (data/palette.js), its standard green the fallback: C30 R-PREFS */
  const PROVEN = 'var(--tl-proven, #3DD68C)'
  assert.equal(p.chainCols[5].discColor, PROVEN)
  assert.ok(p.chainCols.slice(0, 5).every((c) => c.discColor !== PROVEN), 'green on a link')
  const c = v('contradicted')
  assert.equal(c.chainCols[5].discText, '✕')
  assert.equal(c.chainCols[5].discColor, '#FF6369')
  assert.equal(c.pill.glyph, '✕')
  const n = v('noResult')
  assert.equal(n.chainCols[1].f2, 'No result yet ⋯')
  assert.equal(n.chainCols[1].segBg.includes('#66666F'), true, 'no result is dotted and unlit')
  assert.equal(n.chainNote, 'stops at ②')
  assert.equal(v('noRun').chainCols[1].f1, 'nothing recorded')
  assert.equal(v('stopsAt5').chainNote, 'stops at ⑤')
  assert.equal(v('stopsAt5').chainCols[3].discText, '◭', '◭ only at ④')
})

test('with no check route handed to the screen, every check control is off and says why — never "not available in this version" (C30)', () => {
  const data = changeSnapshot(SHAPES.stopsAt5, null, Date.now())
  const html = text({ data })
  assert.ok(html.includes('aria-disabled="true"'))
  assert.ok(html.includes('This check cannot start from this screen.'))
  assert.ok(!/not available in this version/i.test(html))
  const v = M.vals({ data })
  assert.equal(v.chk.off, 'true')
  assert.equal(v.chk.bg, 'transparent', 'an off control is not drawn orange')
  // with an act handed in, the control is on again
  assert.equal(M.vals({ data, actions: { check: () => {} } }).chk.off, undefined)
  // where the server says it will not start one, its own reason is drawn
  const blocked = { reason: 'Another check is running in this project.', enabler: 'This one can run as soon as that one finishes.' }
  const off = M.vals({ data, actions: { check: () => {}, checkBlocked: blocked } })
  assert.equal(off.chk.off, 'true')
  assert.equal(off.chk.why, blocked.reason + ' ' + blocked.enabler)
})

test('the statements of one message become a loop of buttons, one per statement', () => {
  const v = thread({ commands: [{ m: 1 }], claims: [{ m: 2, rests: 'c0' }, { m: 2, rests: 'c0', text: 'Checkout total fixed.', kind: 'done', v: false }] })
  v.claims[1].event_id = v.claims[0].event_id
  const out = M.vals({ data: changeSnapshot(v, null, Date.now()) })
  assert.equal(out.saidStmts.length, 2)
})

test('a tracked Change that is ready draws the approve panel with no missing fact', () => {
  const tracked = { change: { id: 'chg_r', accepted_scope: { version: 1, criteria: ['a'], acceptance: { accepted_at: TS(1) } } }, hq: { title: 'Tracked', criteria: [{}], counts: { proven: 1 }, version: { short: 'wt-1111111' }, readiness: { ready: true }, decision: {} }, proposals: null }
  const data = changeSnapshot(SHAPES.proven, tracked, Date.now())
  const t = everyView({ data })
  assert.ok(t.includes('Ready for approval for version wt-1111111. It rests on:'))
  assert.ok(t.includes('Approve wt-1111111'))
  assert.ok(!/undefined|null|NaN/.test(t.replace(/aria-[a-z]+="[^"]*"/g, '')))
})

test('the lit thread is orange only across links that hold: a run with no result lights nothing', () => {
  const v = M.vals({ data: changeSnapshot(SHAPES.noResult, null, Date.now()) })
  assert.ok(!v.threadPaths.some((p) => p.stroke === '#FF7A29'), 'a no-result run was lit')
  assert.ok(!v.nodes.some((n) => n.border === '1px solid rgba(255,122,41,.55)'), 'a no-result node was lit')
  const ok = M.vals({ data: changeSnapshot(SHAPES.proven, null, Date.now()) })
  assert.ok(ok.threadPaths.some((p) => p.stroke === '#FF7A29'), 'a holding run is lit')
})
