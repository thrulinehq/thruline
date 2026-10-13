/* C29: every count of 1,000 or more on Repair-Loop is written with thousands
 * separators (data/count.js `fmtCount`); a step number stays bare.
 *
 * The live data is selected from the one Change snapshot exactly as
 * repair-loop-live.test.js does, with its counts raised past 1,000. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { changeSnapshot } from '../../data/change.js'
import { selectRepairLoop } from './from-change.js'
import { notConfirmedSentence } from './actions.js'

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
  const file = join(await mkdtemp(join(tmpdir(), 'repair-loop-counts-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state)).replace(/<[^>]*>/g, ' ')

const T = (m) => new Date(Date.UTC(2026, 9, 6, 12, m, 0)).toISOString()
/* repair-loop-live.test.js's thread, its tallies raised past 1,000: a passing run, an edit, a failing re-check */
function thread() {
  const cur = 'wt-2222222222222222'
  return {
    schema_version: 1,
    project: { id: 'p1', name: 'ledger-api', root: '/r/ledger-api' },
    thread: { id: '0123456789abcdef', title: 'Stop the import double-posting', current_version_id: cur, whose_move: 'you', proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: ['/r/ledger-api/src/post.py'] } },
    prompts: [{ event_id: 'ev-p', session_id: 's1', seq: 1, ts: T(0), text: 'the import posts twice', work: true, version_id: 'wt-1111111111111111', raw_file: '/m', raw_line: 1 }],
    edits: [{ id: 'ed0', event_id: 'ed0', session_id: 's1', ts: T(2), file: '/r/ledger-api/src/post.py', op: 'edit', lines_added: 1, lines_removed: 0, ok: true, version_id: cur, raw_file: '/m', raw_line: 3 }],
    commands: [
      { id: 'c0#0', event_id: 'c0', part: 0, session_id: 's1', ts: T(1), command: 'pytest -q tests/test_api.py', outcome: 'succeeded', exit_code: 0, is_test: true, is_commit: false, tally: { passed: 4321, failed: 0, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 }, version_id: 'wt-1111111111111111', raw_file: '/m', raw_line: 2 },
      { id: 'c1#0', event_id: 'c1', part: 0, session_id: 's1', ts: T(4), command: 'pytest -q tests/test_api.py', outcome: 'failed', exit_code: 1, is_test: true, is_commit: false, tally: { passed: 3210, failed: 1205, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 }, version_id: cur, raw_file: '/m', raw_line: 6 },
    ],
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
const live = () => {
  const d = selectRepairLoop(changeSnapshot(thread(), null, new Date(Date.UTC(2026, 9, 6, 13, 0))), null, { now: new Date(Date.UTC(2026, 9, 6, 13, 0)) })
  /* raised past 1,000: the record's size, and a step number that is a position, not a count */
  return {
    ...d, steps: 1679, record: { ...d.record, statements: 2048, notProven: 1001 },
    run: { state: 'failed', step: 1666, at: '12:04', command: 'pytest -q tests/test_api.py', version: d.version.id, exit: 1, passed: 3210, failed: 1205, durationS: null, elapsedS: null, output: [], failures: [] },
    outcomeExamples: { passed: { passed: 4321, failed: 0 }, failed: { passed: 3210, failed: 1205 } },
  }
}

test('the run’s tally, the record’s step and statement counts read with thousands separators', () => {
  const d = live()
  assert.equal(d.run.state, 'failed', 'the fixture is the failing re-check')
  const out = text({ data: d })
  assert.ok(out.includes('1,205 failed, 3,210 passed'), out.match(/.{0,40}failed.{0,40}/))
  assert.ok(!/\b1205 failed|\b3210 passed/.test(out))
  assert.ok(out.includes('1,679 steps · 2,048 statements · 1,001 not proven'), out.match(/.{0,60}statements.{0,60}/))
  assert.ok(!/\b1679 steps|\b2048 statements|\b1001 not proven/.test(out))
})

test('the earlier passing run’s tally, as an honest outcome, reads with separators', () => {
  const out = text({ data: live() })
  assert.ok(out.includes('4,321 passed, 0 failed'), out.match(/.{0,40}passed.{0,40}/))
  assert.ok(!out.includes('4321'))
})

test('a step number of 1,000 or more is a position and stays bare', () => {
  const out = text({ data: live() })
  assert.ok(out.includes('step 1666'), out.match(/.{0,30}step 16.{0,30}/))
  assert.ok(!out.includes('step 1,666'))
})

test('the message for the agent states the tally with separators', () => {
  const out = text({ data: live() })
  assert.ok(out.includes('→ 1,205 failed, 3,210 passed (step 1666'), out.match(/.{0,40}→.{0,60}/))
})

test('sessions not linked: the count reads with separators', () => {
  assert.ok(notConfirmedSentence(new Array(1500).fill({})).startsWith('1,500 sessions of this work were'))
})
