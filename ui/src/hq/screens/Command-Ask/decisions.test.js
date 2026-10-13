/* C29 part 4: ⌘K finds a decision by the person's own words. Each one is a
 * Find row of kind "Decision", with the one model's state sentence, and a link
 * to where the decision is shown: the open Change's Brief, or, with no Change
 * open, the Inbox for a broken one (the only decisions the Inbox draws). A
 * decision shown nowhere else is no link (C30 R-LINKS: never the Inbox in its
 * place). Synthetic fixtures only; nothing is fetched. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { changeSnapshot } from '../../data/change.js'
import { decisionsModel } from '../../data/decisions.js'
import { DECISIONS_LIST } from '../../data/decisions-fixture.js'
import { commandFromChange } from './from-change.js'

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
function make(props, state) { const inst = new Logic(props || {}); inst.props = props || {}; if (state) inst.state = { ...inst.state, ...state }; return inst }
export function render(props, state) { return renderToStaticMarkup(React.createElement(Template, { v: make(props, state).renderVals(), link: (h) => h })) }
export function vals(props, state) { return make(props, state).renderVals() }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-command-decisions-')), 'command.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state)).replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ')

const T = (m) => '2026-10-07T09:0' + m + ':00.000Z'
const VIEW = {
  project: { id: 'p1', name: 'proj', root: '/r' },
  thread: { id: 'th1', title: 'Fix the total', whose_move: 'you', proof: { edited_after_last_pass: [] } },
  prompts: [{ event_id: 'ev-p', session_id: 's1', ts: T(0), text: 'fix the total', work: true }],
  edits: [{ id: 'ev-e1#0', ts: T(1), file: '/r/a.js', lines_added: 2, lines_removed: 1, ok: true, version_id: 'wt-3c3ca28a1b2c3d4e', session_id: 's1' }],
  commands: [{ id: 'ev-c1#0', ts: T(2), command: 'pytest -q', outcome: 'succeeded', exit_code: 0, is_test: true, tally: { passed: 3, failed: 0 }, version_id: 'wt-3c3ca28a1b2c3d4e', session_id: 's1' }],
  claims: [],
  versions: [{ id: 'wt-3c3ca28a1b2c3d4e', committed: true }],
  current_version_id: 'wt-3c3ca28a1b2c3d4e',
  sessions: [{ id: 's1', source: 'claude_code', origin: 'person', started_at: T(0) }],
  files: [],
}
const PROJECT_VIEW = { project: { id: 'p1', name: 'proj', root: '/r' }, threads: [{ id: 'th1', title: 'Fix the total', whose_move: 'you', top_files: [{ path: '/r/a.js' }] }] }
const NOW = new Date('2026-10-07T10:00:00Z')
const open = commandFromChange(changeSnapshot(VIEW, null, NOW), PROJECT_VIEW, { readAt: '10:00' })
const closed = commandFromChange(null, PROJECT_VIEW, { readAt: '10:00' })
const served = () => decisionsModel({ status: 'served', doc: DECISIONS_LIST })

test('⌘K finds a decision by its words, as a Decision row with its sentence and the Brief it is shown on', () => {
  const props = { data: open, decisions: served(), at: { screen: 'brief', change: 'th1', project: 'p1' }, overlay: true }
  const v = M.vals(props, { query: 'home-folder path' })
  const row = v.findRows.find((r) => r.kind === 'Decision')
  assert.ok(row, 'a Decision row is found')
  assert.equal(row.text, 'Never show a home-folder path on any screen')
  assert.match(row.meta, /Broken on wt-3c3ca28: the record shows it in 2 places\./)
  assert.equal(row.href, '#/changes/th1?project=p1')
  const t = text(props, { query: 'home-folder path' })
  assert.match(t, /Decision/)
  assert.match(t, /Never show a home-folder path on any screen/)
})

test('with no Change open, a broken decision goes to the Inbox that shows it; any other is no link', () => {
  const props = { data: closed, decisions: served(), at: { screen: 'inbox', project: 'p1' }, overlay: true }
  const broken = M.vals(props, { query: 'home-folder' }).findRows.find((r) => r.kind === 'Decision')
  assert.equal(broken.href, '#/inbox?project=p1')
  const holds = M.vals(props, { query: 'Stripe' }).findRows.find((r) => r.kind === 'Decision')
  assert.equal(holds.text, 'Payments are named Stripe everywhere')
  assert.equal(holds.href, undefined, 'never the Inbox in the place of where it is shown')
})

test('no decisions route, and the design gallery: no Decision row', () => {
  const v = M.vals({ data: open, decisions: decisionsModel({ status: 'not_served' }), at: { screen: 'brief', change: 'th1', project: 'p1' } }, { query: 'home-folder' })
  assert.ok(!v.findRows.some((r) => r.kind === 'Decision'))
  const g = M.vals({ decisions: served() }, { query: 'home-folder' })
  assert.ok(!g.findRows.some((r) => r.kind === 'Decision'))
})
