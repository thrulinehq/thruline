/* C29: every count of 1,000 or more on ⌘K is written with thousands
 * separators (data/count.js `fmtCount`); a step number stays bare.
 *
 * The live data is what from-change.test.js selects (`commandFromChange` over
 * the one Change snapshot), its tallies and record counts raised past 1,000,
 * rendered through template + logic the way the other screens' live tests do. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { changeSnapshot } from '../../data/change.js'
import { commandFromChange } from './from-change.js'

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
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'command-ask-counts-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state)).replace(/<[^>]*>/g, ' ')

/* from-change.test.js's thread, its passing run's tally raised past 1,000 */
const T = (m) => '2026-10-06T20:0' + m + ':00.000Z'
const VIEW = {
  project: { id: 'p1', name: 'proj', root: '/r' },
  thread: { id: 'th1', title: 'Fix the total', whose_move: 'you', proof: { edited_after_last_pass: ['/r/b.js'] } },
  prompts: [{ event_id: 'ev-p', session_id: 's1', ts: T(0), text: 'fix the total', work: true }],
  edits: [
    { id: 'ev-e1#0', ts: T(1), file: '/r/a.js', lines_added: 2, lines_removed: 1, ok: true, version_id: 'wt-1', session_id: 's1' },
    { id: 'ev-e2#0', ts: T(4), file: '/r/b.js', lines_added: 1, lines_removed: 0, ok: true, version_id: 'wt-1', session_id: 's1' },
  ],
  commands: [
    { id: 'ev-c1#0', ts: T(2), command: 'pytest -q', outcome: 'succeeded', exit_code: null, is_test: true, tally: { passed: 3456, failed: 0 }, version_id: 'wt-1', session_id: 's1' },
    { id: 'ev-c3#0', ts: T(5), command: 'pytest -q tests/x', outcome: 'unknown', is_test: true, tally: null, version_id: 'wt-1', session_id: 's1' },
  ],
  claims: [{ id: 'ev-s#0', event_id: 'ev-s', ts: T(6), text: 'Tests pass.', kind: 'tests_pass', has_verifier: true, rests_on_command_id: 'ev-c3#0', followed_by_passing_test: false, version_id: 'wt-1', session_id: 's1' }],
  versions: [{ id: 'wt-1', committed: false }],
  current_version_id: 'wt-1',
  sessions: [{ id: 's1', source: 'claude_code', origin: 'person', started_at: T(0) }],
  files: [],
}
const PROJECT_VIEW = { project: { id: 'p1', name: 'proj', root: '/r' }, threads: [{ id: 'th1', title: 'Fix the total', whose_move: 'you', top_files: [] }] }
const live = () => {
  const d = commandFromChange(changeSnapshot(VIEW, null, new Date('2026-10-06T20:30:00Z')), PROJECT_VIEW, { readAt: '20:30' })
  /* raised past 1,000: the record's size; the passing run sits at step 1666, a position */
  return {
    ...d,
    record: { ...d.record, steps: 1679, statements: 2048, statementsNeedingCheck: 1001 },
    diff: { edits: 1500, files: 1100, filesChangedAfterLastCheck: 1050 },
    findable: d.findable.map((x) => (x.kind === 'Run' ? { ...x, step: 1666 } : x)),
  }
}

test('the header counts the record with thousands separators', () => {
  const out = text({ data: live() })
  assert.ok(out.includes('1,679 steps · 2,048 statements'), out.match(/.{0,30}steps.{0,30}/))
  assert.ok(!/\b1679 steps|\b2048 statements/.test(out))
})

test('Find counts the recorded steps with separators, and a run’s tally too', () => {
  const out = text({ data: live() }, { query: 'pytest', active: '' })
  assert.ok(out.includes('in the 1,679 recorded steps'), out.match(/.{0,30}recorded steps.{0,30}/))
  assert.ok(out.includes('3,456 passed, 0 failed'), out.match(/.{0,40}passed.{0,40}/))
  assert.ok(!out.includes('3456'))
})

test('a step number of 1,000 or more is a position and stays bare', () => {
  const out = text({ data: live() }, { query: 'pytest', active: '' })
  assert.ok(out.includes('step 1666'), out.match(/.{0,30}step 16.{0,30}/))
  assert.ok(!out.includes('1,666'))
})

test('the palette’s lines count statements, edits and files with separators', () => {
  const out = text({ data: live() })
  assert.ok(out.includes('2,048 statements · 1,001 need a check'), out.match(/.{0,40}need a check.{0,20}/))
  assert.ok(out.includes('1,500 edits to 1,100 files · 1,050 files changed'), out.match(/.{0,40}edits to.{0,40}/))
})
