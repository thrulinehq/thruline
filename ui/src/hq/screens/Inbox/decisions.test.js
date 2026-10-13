/* C29: the Inbox's "Decision broken" group (docs/decisions/API.md, "UI and
 * gate use"): accepted decisions the record shows broken come first, each in
 * the person's own words, with the version, the number of places and the
 * first place; selecting one shows where, and the terminal commands that
 * settle it. The design gallery draws none of it. Synthetic fixtures only. */
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
import * as P from '../../data/project.js'
import { decisionsModel } from '../../data/decisions.js'
import { DECISIONS_DORMANT, DECISIONS_LIST } from '../../data/decisions-fixture.js'
import { LIST, VIEW, NOW } from '../../data/project-fixture.js'
import Template from './template.jsx'
import Logic from './logic.js'
export { decisionsModel, DECISIONS_LIST, DECISIONS_DORMANT }
export function render({ live = true, decisions, state } = {}) {
  const data = live ? P.inboxData({ projects: LIST, view: VIEW, storage: { bytes: 5_400_000_000 }, now: NOW }) : undefined
  const props = { data, decisions }
  const inst = new (P.withFrame(Logic))(props)
  inst.props = props
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-inbox-decisions-')), 'inbox.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const text = (html) => html.replace(/<[^>]+>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ')
const served = () => M.decisionsModel({ status: 'served', doc: M.DECISIONS_LIST })

test('the broken decision is the first group, in the person’s own words, with its version, places and first place', () => {
  const html = M.render({ decisions: served() })
  const t = text(html)
  const at = t.indexOf('Decision broken')
  assert.ok(at >= 0, 'the group is drawn')
  assert.ok(at < t.indexOf('Waiting on you'), 'before every other group')
  assert.match(t, /Never show a home-folder path on any screen/)
  assert.match(t, /3c3ca28/)
  assert.match(t, /Blocks approval · 2 places/)
  assert.match(t, /ui\/src\/hq\/screens\/Approve-Sheet\/logic\.js:212 · a home-folder path appears in user-facing copy and 1 place more/)
  // only accepted, broken, unwaived decisions: not the proposal, not the one with no proof
  assert.doesNotMatch(t, /Write the failing test before you change the code/)
  assert.doesNotMatch(t, /Every exported time is UTC/)
})

test('selected first, it shows where the record shows it and the terminal commands that settle it', () => {
  const t = text(M.render({ decisions: served() }))
  assert.match(t, /Where the record shows it/)
  assert.match(t, /ui\/src\/hq\/screens\/Settings\/logic\.js:88/)
  assert.match(t, /thruline decide prepare waive dec-01 --version-id wt-3c3ca28a1b2c3d4e/)
  assert.match(t, /thruline decide prepare retire dec-01/)
  assert.match(t, /HQ copies the command; it records nothing and runs nothing/)
})

test('a server without the route, and the design gallery, draw no decision', () => {
  const none = text(M.render({ decisions: M.decisionsModel({ status: 'not_served' }) }))
  assert.doesNotMatch(none, /Decision broken/)
  const gallery = text(M.render({ live: false, decisions: served() }))
  assert.doesNotMatch(gallery, /Decision broken/)
})

test('the group is drawn under All only: no filter counts decisions (C30 P0-5)', () => {
  const group = '<section aria-label="Decision broken"'
  assert.ok(M.render({ decisions: served(), state: { filter: 'all' } }).includes(group))
  for (const filter of ['you', 'agent', 'live']) assert.ok(!M.render({ decisions: served(), state: { filter } }).includes(group), filter)
})

test('a dormant gate does not draw a broken decision as blocking; an active gate does', () => {
  const dormant = M.decisionsModel({ status: 'served', doc: M.DECISIONS_DORMANT })
  const t = text(M.render({ decisions: dormant }))
  assert.match(t, /Set to block approval · 2 places/)
  assert.match(t, /Your rule, accepted by you · set to block approval/)
  assert.doesNotMatch(t, /Blocks approval/)
  assert.match(text(M.render({ decisions: served() })), /Blocks approval · 2 places/)
})
