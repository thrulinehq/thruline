/* W2-E, HQ Next wave 2: Team on the real record. ThruLine v0.2 has no team
 * model, so the live page shows only what the store holds — approvals, lines
 * set aside, the plan and storage — and draws every other section with its
 * reason in plain words. Never a trial, never Paddle. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { teamFromStore, planOf } from './from-store.js'

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
  const file = join(await mkdtemp(join(tmpdir(), 'w2e-team-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state))

const PROJECT = { id: 'p1', name: 'shop', members: [{ ledger_project_id: 'L1' }], whose_move: { you: 3 } }
const OTHER = { id: 'p2', name: 'notes', members: [{ ledger_project_id: 'L2' }], whose_move: { you: 1 } }
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
const ELSEWHERE = { ...CHANGE, id: 'chg_2', project_id: 'L2', request: { title: 'Not this project' } }
const LICENCE_ABSENT = { status: 'absent', plan: null, expires_on: null, paid_gate: true, headline: 'free', entitlements: { approve_version: { allowed: false, reason: 'r', enable: 'e', status: 'absent' } } }
const STORE = { project: PROJECT, projects: [PROJECT, OTHER], changes: [CHANGE, ELSEWHERE], storage: { bytes: 5.4e9 }, licence: LICENCE_ABSENT, now: Date.parse('2026-10-07T09:00:00Z') }

test('only the project’s own Changes reach the page, as approvals and lines set aside', () => {
  const d = teamFromStore(STORE)
  assert.equal(d.decisions.length, 1)
  assert.equal(d.decisions[0].title, 'Fix the cart total')
  assert.equal(d.decisions[0].version, '3f9a1c2')
  assert.equal(d.setAside.length, 1)
  assert.equal(d.setAside[0].line, 'Coupons apply once')
  assert.equal(d.setAside[0].why, 'Only staging can reach it.')
  // the sidebar badge is the current project's own count (F2 — data/project.js inboxBadge), never a sum across projects;
  // C46 §1.1: it is the one count, so it needs the project's readiness reading, and is no badge without one
  assert.equal(d.inbox.yourMove, null)
  const read = { project: PROJECT.id, ready: {}, moves: {}, failed: 0, listFailed: false }
  assert.equal(teamFromStore({ ...STORE, reading: read }).inbox.yourMove, 3)
  assert.equal(d.storage.bytes, 5.4e9)
})

test('the plan is Free or Pro, and a trial is never read', () => {
  assert.deepEqual(planOf(LICENCE_ABSENT), { name: 'Free', licensee: null })
  // C30 P0-6: the one plan rule (data/plan.js); "active" was never a status the server serves
  assert.equal(planOf({ status: 'valid', plan: 'pro', licensee: 'Acme', entitlements: { approve_version: { allowed: true } } }).name, 'Pro')
  assert.equal(planOf({ status: 'active', plan: 'pro', licensee: 'Acme', entitlements: { approve_version: { allowed: true } } }).name, 'Free')
  assert.equal(planOf(null), null)
  const html = text({ data: teamFromStore(STORE) })
  assert.ok(!/trial/i.test(html), 'the live page never says trial')
  assert.ok(html.includes('Approving a version needs Pro. Everything else is free.'))
})

test('the live banner stays one line: its link is never pushed off the right edge', () => {
  // At 1512 px the banner is a single 32 px nowrap row: 12.5 px text takes about 5.1 px a
  // character, and the sentence once grew to 277 characters, which pushed "See plans" off the
  // edge ("See p…"). Keep head + text under 240 characters.
  const html = text({ data: teamFromStore(STORE) })
  const banner = html.match(/role="note"[^>]*>(.*?)<\/div>/s)?.[1] || ''
  const words = banner.replace(/<[^>]+>/g, '').replace(/&#x27;|&quot;|&amp;/g, 'x')
  assert.ok(words.includes('Team arrives in a later version.'), 'the banner is drawn')
  assert.ok(words.length < 240 + 'See plans'.length, `the banner is ${words.length} characters`)
})

test('the live page draws the record and says why the rest is not here', () => {
  const html = text({ data: teamFromStore(STORE) })
  assert.ok(html.includes('Team arrives in a later version.'))
  assert.ok(html.includes('Fix the cart total'))
  assert.ok(html.includes('Approved · Sam · '))
  // C30 R-LEDGER: a second approval is real now — the slot says what the record holds (team-view-seconds.test.js)
  assert.ok(html.includes('No second approval yet'))
  assert.ok(!html.includes('A second approval is not available in this version.'))
  assert.ok(html.includes('Coupons apply once'))
  assert.ok(html.includes('ThruLine is keeping 5.4 GB of records on this machine'))
  for (const why of ['Groups would name', 'Approval rules would say', 'Shared lists of what counts as done would', 'A seat would be a person who approves']) {
    assert.ok(html.includes('Not available in this version. ' + why), why)
  }
  for (const never of ['Roadmap.', 'Paddle', 'Acme', 'payments-owners', 'Priya', 'illustrative', 'Bug fix', 'Assign']) {
    assert.ok(!html.includes(never), 'live page shows ' + never)
  }
})

test('an empty store says so, in words, and counts nothing it was not given', () => {
  const d = teamFromStore({ ...STORE, changes: [], storage: null })
  const html = text({ data: d })
  assert.ok(html.includes('No version has been approved on this machine yet.'))
  assert.ok(html.includes('Nothing has been set aside on this machine.'))
  assert.ok(html.includes('How much ThruLine keeps on this machine could not be read'))
})

test('Team reaches no write address except through actions.js (C30 R-LEDGER: the two second-approval ones)', () => {
  for (const f of ['index.jsx', 'from-store.js', 'logic.js', 'seconds.jsx']) {
    const src = readFileSync(join(here, f), 'utf8')
    assert.ok(!/method:\s*['"](POST|PUT|PATCH|DELETE)/i.test(src), f + ' writes')
  }
})
