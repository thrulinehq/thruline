/* C29: every count of 1,000 or more on Team is written with thousands
 * separators (data/count.js `fmtCount`).
 *
 * The live data is what team-view-live.test.js selects from the store
 * (`teamFromStore`), with its approvals, lines set aside and the inbox badge
 * raised past 1,000. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { teamFromStore } from './from-store.js'

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
  const file = join(await mkdtemp(join(tmpdir(), 'team-view-counts-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state)).replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ')

const PROJECT = { id: 'p1', name: 'shop', members: [{ ledger_project_id: 'L1' }], whose_move: { you: 3 } }
const CHANGE = {
  id: 'chg_1', project_id: 'L1', request: { title: 'Fix the cart total' },
  accepted_scope: { criteria: ['Totals are right', 'Coupons apply once'] },
  decisions: {
    standing: { id: 'd1', target_revision: '3f9a1c2deadbeef', actor: 'Sam', decided_at: '2026-10-06T10:00:00Z' },
    target_revision: '3f9a1c2deadbeef',
    history: [{
      id: 'd1', act: 'approved', target_revision: '3f9a1c2deadbeef', scope_version: 1, actor: 'Sam', decided_at: '2026-10-06T10:00:00Z',
      record_path: '.thruline/decisions/3f9a1c2.json',
      exceptions: [{ obligation_id: 'criterion:2', kind: 'waived', actor: 'Sam', authority: 'owners', rationale: 'Only staging can reach it.' }],
    }],
  },
}
const STORE = { project: PROJECT, projects: [PROJECT], changes: [CHANGE], storage: { bytes: 5.4e9 }, licence: null, now: Date.parse('2026-10-07T09:00:00Z') }
const live = () => {
  const d = teamFromStore(STORE)
  /* raised past 1,000: as many approvals and lines set aside, and the inbox badge */
  const times = (xs, n) => Array.from({ length: n }, () => xs[0])
  return { ...d, decisions: times(d.decisions, 1200), setAside: times(d.setAside, 1500), inbox: { yourMove: 1234 } }
}

test('the approvals and the lines set aside are counted with thousands separators', () => {
  const out = text({ data: live() })
  assert.ok(out.includes('1,200 versions are approved here'), out.match(/.{0,30}approved here.{0,30}/))
  assert.ok(out.includes('1,500 lines set aside'), out.match(/.{0,30}set aside.{0,30}/))
  assert.ok(out.includes('1,200 versions approved · 1,500 lines set aside'))
  assert.ok(!/\b1200\b|\b1500\b/.test(out), out.match(/.{0,30}\b(1200|1500)\b.{0,30}/))
})

test('the section counts and the inbox badge read with separators', () => {
  const out = text({ data: live() })
  assert.ok(out.includes('1,234'), 'the inbox badge')
  assert.ok(!/\b1234\b/.test(out))
})
