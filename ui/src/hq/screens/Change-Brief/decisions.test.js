/* C29 part 2: the Change Brief's decision rows for the candidate version.
 *
 * Each row is a rule the person set, in their own words (`span`), with the
 * one model's state sentence (data/decisions.js `decisionRow`) and whether it
 * blocks approval or warns; broken first. A server without the Decisions route
 * gives one quiet line, never a verdict. The design gallery draws none of it.
 * Synthetic fixtures only (data/decisions-fixture.js); nothing is fetched. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { changeSnapshot } from '../../data/change.js'
import { decisionsModel } from '../../data/decisions.js'
import { DECISIONS_EMPTY, DECISIONS_LIST } from '../../data/decisions-fixture.js'

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
  const file = join(await mkdtemp(join(tmpdir(), 'hq-brief-decisions-')), 'brief.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state)).replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ')

/* A served ThreadView (docs/hq-next/API.md) whose current version is the one
 * the decisions fixture was checked on. */
const CUR = 'wt-3c3ca28a1b2c3d4e'
const T = (m) => new Date(Date.UTC(2026, 9, 7, 9, m, 0)).toISOString()
function thread(cur = CUR) {
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
const snap = (cur) => changeSnapshot(thread(cur), null, new Date(Date.UTC(2026, 9, 7, 10, 0)))
const served = (doc = DECISIONS_LIST) => decisionsModel({ status: 'served', doc })

test('each decision row is the person’s own words, its state sentence, and blocks or warns', () => {
  const t = text({ data: snap(), decisions: served() })
  assert.match(t, /Never show a home-folder path on any screen/)
  assert.match(t, /Broken on wt-3c3ca28: the record shows it in 2 places\./)
  assert.match(t, /Payments are named Stripe everywhere/)
  assert.match(t, /Holds on wt-3c3ca28, in 310 places checked\./)
  assert.match(t, /Write the failing test before you change the code/)
  assert.match(t, /No proof found on wt-3c3ca28: the record has no failing run before the first source edit\./)
  assert.match(t, /Every exported time is UTC, written with a Z/)
  assert.match(t, /Proposed\. Not in force until you accept it and confirm it yourself\./)
  assert.match(t, /Blocks approval/)
  assert.match(t, /Warns/)
  // the first place the record shows a broken decision, as the Inbox names it
  assert.match(t, /ui\/src\/hq\/screens\/Approve-Sheet\/logic\.js:212/)
  // a verdict on an older version is said to be not checked on this one
  assert.match(t, /The status bar always says what ThruLine keeps on this machine/)
  assert.match(t, /not checked on wt-3c3ca28/)
  for (const w of [/\blie\b/i, /cheat/i, /violat/i, /\bignor/i, /hallucinat/i, /\d+\s*%/, /trial/i, /confidence/i]) assert.doesNotMatch(t.slice(t.indexOf('Decisions')), w)
})

test('broken first: the broken decision comes before every other row', () => {
  const t = text({ data: snap(), decisions: served() })
  const broken = t.indexOf('Never show a home-folder path on any screen')
  assert.ok(broken >= 0)
  for (const other of ['Write the failing test before you change the code', 'The status bar always says', 'Payments are named Stripe everywhere', 'Every exported time is UTC']) {
    const at = t.indexOf(other)
    assert.ok(at > broken, `${other} is after the broken decision`)
  }
})

test('a server without the Decisions route gives one quiet line and no verdict', () => {
  const t = text({ data: snap(), decisions: decisionsModel({ status: 'not_served' }) })
  assert.match(t, /Decisions are not checked in this version of ThruLine\./)
  assert.doesNotMatch(t, /Holds on wt-3c3ca28, in|Broken on wt-3c3ca28:|Blocks approval/) // a decision's sentence shape; the Brief's own "Holds on wt-… · nothing to settle" is not a decision
  // an answer that is not a list says why, in the reader's words
  const u = text({ data: snap(), decisions: decisionsModel({ status: 'unreadable', message: 'ThruLine could not read this project’s decisions: no project p1.' }) })
  assert.match(u, /ThruLine could not read this project’s decisions: no project p1\./)
})

test('before the first read, with no decisions, and in the design gallery: nothing is drawn', () => {
  for (const decisions of [undefined, decisionsModel(undefined), served(DECISIONS_EMPTY)]) {
    const t = text({ data: snap(), decisions })
    assert.doesNotMatch(t, /Decisions are not checked|Blocks approval|Broken on wt-3c3ca28/)
  }
  const gallery = text({ decisions: served() })
  assert.doesNotMatch(gallery, /Never show a home-folder path|Decisions are not checked/)
})
