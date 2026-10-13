/* W2-C, HQ Next wave 2: What-Changed on the real record. The screen reads one
 * Change snapshot (data/change.js) through from-change.js; these tests drive it
 * from a served ThreadView and from the Brief's own snapshot fixture. The design
 * render (#/design/What-Changed) is checked by scripts/hq/pixdiff.py, not here. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { KEY_TABLE, wiredOn } from '../../keys/table.js'

const here = dirname(fileURLToPath(import.meta.url))

/* The screen bundled for node with the ONE Change model (data/change.js), its
 * selector and a ThreadView fixture: render(data, state) is the live path. */
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
import { whatChangedFromChange } from './from-change.js'
import { changeSnapshot } from '../../data/change.js'
import BRIEF from '../Change-Brief/demo.js'
import { threadFixture } from './thread-fixture.js'
import { withServedHunks, kindOf } from './from-change.js'
export { withServedHunks, kindOf }
export { whatChangedFromChange as select, changeSnapshot, BRIEF, threadFixture }
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

const live = () => {
  const view = M.threadFixture()
  const snap = M.changeSnapshot(view, null, NOW)
  return M.select(M.withServedHunks(snap, view))
}

test('the selector reads the Brief snapshot fixture in all four states without inventing a count', () => {
  for (const [name, snap] of Object.entries(M.BRIEF.states)) {
    const d = M.select(snap)
    assert.ok(d, name)
    assert.equal(d.version.id, snap.currentVersion, name)
    assert.deepEqual(d.edits.map((e) => e.id), snap.edits.map((e) => e.id), name)
    // the Brief fixture's edits carry no lines: they stay absent, never 0
    assert.ok(d.edits.every((e) => e.hunks === null), name)
    const t = text({ data: d })
    assert.ok(!/Demo records|engine:|illustrative/.test(t), name)
  }
})

test('C54 changed#8: a cancelled check remains visible as stopped from the served run sheet', () => {
  const view = M.threadFixture()
  const snap = M.changeSnapshot(view, null, NOW)
  snap.runSheet = { checks: [{ last: {
    run: 'run-stopped', outcome: 'cancelled', started_at: '2026-10-07T10:15:00.000Z',
    recorded_at: '2026-10-07T10:15:09.000Z',
  } }] }
  const data = M.select(snap)
  assert.equal(data.stopped.elapsedS, 9)
  const act = M.instance({ data }).renderVals().act
  assert.equal(act.stopped, true)
  assert.match(act.stoppedText, /You stopped this check at \d\d:\d\d after 9 s\. What it printed is kept, and it settles nothing\./)
})

test('C54 changed#12: the live unchanged-file note discloses that declared paths are not built', () => {
  const v = M.instance({ data: live() }, { file: 'src/totals.py' }).renderVals()
  assert.match(v.det.note, /Declared paths are engine work, not built yet\./)
})

test('C54 changed#14: drawn What Changed keys reach check, Tell the agent and approval', () => {
  for (const id of ['primary', 'copy', 'approve']) {
    assert.equal(wiredOn(KEY_TABLE.find((e) => e.id === id), 'What-Changed'), true, id)
  }
  const calls = []
  const data = live()
  const actions = { check: () => calls.push('check'), openTell: () => calls.push('tell'), openApprove: () => calls.push('approve') }
  const v = M.instance({ data, actions }).renderVals()
  v.keys.primary()
  v.keys.copy()
  const ready = { ...data, change: { ...data.change, state: 'ready' } }
  M.instance({ data: ready, actions }).renderVals().keys.approve()
  assert.deepEqual(calls, ['check', 'tell', 'approve'])
})

test('every line of every edit reaches the screen, in the whole Change, and a long one is not cut', () => {
  const d = live()
  for (const file of d.files) {
    const h = html({ data: d }, { file: file.path, from: 'base' })
    const view = M.threadFixture().edits.filter((e) => e.file.endsWith(file.path.split('/').pop()))
    for (const e of view) for (const hk of e.hunks) for (const l of hk.lines) {
      if (l.text) assert.ok(unescape(h).includes(l.text), `${file.path}: line ${JSON.stringify(l.text)} is missing`)
    }
  }
  const t = text({ data: d }, { file: 'src/api.py', from: 'base' })
  assert.ok(t.includes('line 300 of the new file'))
  assert.ok(t.includes('+300 −0'))
})

test('a hunk the record states no line numbers for says so, and its gutters stay empty', () => {
  const d = live()
  const t = text({ data: d }, { file: 'src/api.py', from: 'base' })
  assert.ok(t.includes('@@ line numbers not recorded @@'))
  assert.ok(t.includes('from the request · no surrounding lines'))
  const p = text({ data: d }, { file: 'src/util.py', from: 'base' })
  assert.ok(p.includes('@@ line numbers not recorded @@') && p.includes('def helper():'))
  const n = text({ data: d }, { file: 'src/totals.py', from: 'base' })
  assert.ok(n.includes('@@ -10,3 +10,4 @@'))
})

test('a file edited after the run is not covered, even on the same version', () => {
  const view = M.threadFixture()
  view.commands[0].version_id = view.edits[1].version_id // the run and the later edit share a version
  const d = M.select(M.withServedHunks(M.changeSnapshot(view, null, NOW), view))
  const t = text({ data: d }, { file: 'src/api.py' })
  const row = t.split('\n').find((l) => l.startsWith('src/api.py. ')) || ''
  assert.match(row, /Not proven, no check covers it/, 'a file changed after the run read as proven')
  const totals = t.split('\n').find((l) => l.startsWith('src/totals.py. ')) || ''
  assert.match(totals, /Proven on /, 'a file unchanged since a run on this version should read as covered')
})

test('commands that are not test runs never count as checks', () => {
  const d = live()
  assert.deepEqual(d.runs.map((r) => r.id), ['c1'])
})

test('Check it again with no act handed in is off, says why in plain words, and does nothing — never "not available in this version" (C30)', () => {
  const d = live()
  const h = html({ data: d })
  assert.match(h, /<button[^>]*disabled=""[^>]*title="This check cannot start from this screen\."/)
  assert.ok(!/not available in this version/i.test(h))
  const inst = M.instance({ data: d })
  inst.renderVals().act.check()
  assert.equal(inst.state.run, 'idle')
  assert.equal(inst._t, undefined)
  assert.ok(!/engine|endpoint|route/i.test(text({ data: d })), 'a Plain surface names the engine')
})

test('Ready for approval comes from the Change, never from this screen', () => {
  const view = M.threadFixture()
  view.commands.push({ ...view.commands[0], id: 'c9', event_id: 'c9', ts: '2026-10-07T10:09:00.000Z', version_id: view.current_version_id })
  const d = M.select(M.withServedHunks(M.changeSnapshot(view, null, NOW), view))
  assert.notEqual(d.change.state, 'ready')
  assert.ok(!text({ data: d }).includes('Ready for approval'))
  d.change.state = 'ready'
  assert.ok(text({ data: d }).includes('Ready for approval'))
})

test('files are grouped by a stated path rule', () => {
  assert.equal(M.kindOf('src/a.py'), 'code')
  assert.equal(M.kindOf('tests/test_a.py'), 'tests')
  assert.equal(M.kindOf('ui/src/x.test.js'), 'tests')
  assert.equal(M.kindOf('package.json'), 'config')
  assert.equal(M.kindOf('ui/dist/assets/a.js'), 'generated')
  assert.equal(M.kindOf('Cargo.lock'), 'generated')
})

test('a Change with no runs and no statements still draws, in the design’s words for absence', () => {
  const view = M.threadFixture()
  view.commands = []
  view.claims = []
  const d = M.select(M.withServedHunks(M.changeSnapshot(view, null, NOW), view))
  const t = text({ data: d })
  assert.ok(t.includes('changed in this Change, and nothing in the record has checked'))
  assert.ok(t.includes('no run in the record backs it'))
})
