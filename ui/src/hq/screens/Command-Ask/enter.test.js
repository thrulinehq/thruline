/* Census §3.08 row 2 (design note 2): ↵ on an Act, Go to or Find row follows
 * that row's own link — the element drawn for the row is clicked, so ↵ goes
 * where a click goes and the overlay closes as a click closes it — and
 * Approve stays inert. ↵ on a question still asks it. The palette is mounted
 * in jsdom with the live data; every click on a link is caught and its
 * navigation stopped, so nothing leaves the page. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import { changeSnapshot } from '../../data/change.js'
import { linker } from '../../routes.js'
import { commandFromChange } from './from-change.js'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>', { url: 'http://127.0.0.1/' })
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { createRoot } from 'react-dom/client'
import { DcScreen } from '../../dc/runtime.jsx'
import Template from './template.jsx'
import Logic from './logic.js'
export function mount(el, props, link) { const root = createRoot(el); root.render(React.createElement(DcScreen, { Logic, Template, props, link })); return root }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-command-enter-')), 'command.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const tick = () => new Promise((r) => setTimeout(r, 0))
const A = '0a0a0a0a0a0a0a0a'
const T = (m) => '2026-10-07T09:0' + m + ':00.000Z'
const VIEW = {
  project: { id: 'p1', name: 'shop', root: '/r' },
  thread: { id: A, title: 'Fix the total', whose_move: 'you', proof: { edited_after_last_pass: [] } },
  prompts: [{ event_id: 'ev-p', session_id: 's1', ts: T(0), text: 'fix the totals', work: true }],
  edits: [{ id: 'ev-e1#0', ts: T(1), file: '/r/cart/totals.py', lines_added: 2, lines_removed: 1, ok: true, version_id: 'wt-1', session_id: 's1' }],
  commands: [{ id: 'ev-c1#0', ts: T(2), command: 'pytest -q', outcome: 'succeeded', exit_code: 0, is_test: true, tally: { passed: 3, failed: 0 }, version_id: 'wt-1', session_id: 's1' }],
  claims: [{ id: 'ev-s#0', event_id: 'ev-s', ts: T(3), text: 'Tests pass.', kind: 'tests_pass', has_verifier: true, rests_on_command_id: 'ev-c1#0', followed_by_passing_test: false, version_id: 'wt-1', session_id: 's1' }],
  versions: [{ id: 'wt-1', committed: false }],
  current_version_id: 'wt-1',
  sessions: [{ id: 's1', source: 'claude_code', origin: 'person', started_at: T(0) }],
  files: [],
}
const PROJECT_VIEW = { project: { id: 'p1', name: 'shop', root: '/r' }, threads: [{ id: A, title: 'Fix the total', whose_move: 'you', top_files: [] }] }
const DATA = commandFromChange(changeSnapshot(VIEW, null, new Date('2026-10-07T10:00:00Z')), PROJECT_VIEW, { readAt: '10:00', deletions: [] })
const AT = { screen: 'brief', change: A, project: 'p1' }

/** A palette on the page; every link click is recorded and kept from navigating. */
async function palette(extra = {}) {
  document.body.innerHTML = '<div id="root"></div>'
  const closed = []
  const followed = []
  const onClick = (e) => {
    const a = e.target && e.target.closest ? e.target.closest('a[href]') : null
    if (a) { followed.push(a.getAttribute('href')); e.preventDefault() }
  }
  document.addEventListener('click', onClick, true)
  const root = M.mount(document.getElementById('root'), { data: DATA, at: AT, overlay: true, onClose: () => closed.push(true), projectId: 'p1', ...extra }, linker(AT))
  await tick()
  const input = document.getElementById('tl-cmdk-input')
  const key = async (k) => { input.dispatchEvent(new window.KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true })); await tick() }
  const type = async (text) => {
    const set = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set
    set.call(input, text)
    input.dispatchEvent(new window.Event('input', { bubbles: true }))
    await tick()
  }
  const active = () => input.getAttribute('aria-activedescendant')
  const done = () => { document.removeEventListener('click', onClick, true); root.unmount() }
  return { key, type, active, closed, followed, done }
}

test('↵ on Check it again follows the row’s own link, and the palette closes as a click closes it', async () => {
  const p = await palette()
  assert.equal(p.active(), 'tl-opt-act-check')
  await p.key('Enter')
  assert.deepEqual(p.followed, ['#/changes/' + A + '/check?project=p1'])
  assert.equal(p.closed.length, 1)
  p.done()
})

test('↵ follows the link the row draws, even when the screen hands it another (the running sheet)', async () => {
  const runs = '#/changes/' + A + '/check?project=p1&run=1'
  const p = await palette({ actions: { checkHref: runs, runs: true } })
  await p.key('Enter')
  assert.deepEqual(p.followed, [runs])
  p.done()
})

test('↵ on Tell the agent, Say what counts as done and every Go to row goes where the row links', async () => {
  const p = await palette()
  const want = {
    'tl-opt-act-tell': '#/changes/' + A + '/check?project=p1',
    'tl-opt-act-define': '#/changes/' + A + '/done?project=p1',
    'tl-opt-go-statements': '#/changes/' + A + '/statements?project=p1',
    'tl-opt-go-changed': '#/changes/' + A + '/changed?project=p1',
    'tl-opt-go-story': '#/changes/' + A + '/story?project=p1',
    'tl-opt-go-inbox': '#/inbox?project=p1',
  }
  const seen = {}
  for (let i = 0; i < 9; i += 1) {
    await p.key('ArrowDown')
    const id = p.active()
    if (!(id in want) || id in seen) continue
    const before = p.followed.length
    await p.key('Enter')
    seen[id] = p.followed.slice(before)
  }
  for (const [id, href] of Object.entries(want)) assert.deepEqual(seen[id], [href], id)
  p.done()
})

test('↵ on Approve does nothing: it is drawn aria-disabled, and stays inert', async () => {
  const p = await palette()
  await p.key('ArrowDown')
  await p.key('ArrowDown')
  assert.equal(p.active(), 'tl-opt-act-approve')
  await p.key('Enter')
  assert.deepEqual(p.followed, [])
  assert.equal(p.closed.length, 0)
  assert.ok(document.getElementById('tl-opt-act-approve'), 'still open')
  p.done()
})

test('↵ on a Find row opens it on the Change; ↵ on a question asks it and follows nothing', async () => {
  const p = await palette()
  await p.type('totals')
  assert.match(p.active(), /^tl-opt-find-/)
  await p.key('Enter')
  assert.equal(p.followed.length, 1)
  assert.match(p.followed[0], new RegExp('^#/changes/' + A + '(/changed)?\\?project=p1$'))
  p.done()

  const q = await palette()
  await q.type('what changed after the last check?')
  assert.equal(q.active(), 'tl-opt-ask-changed')
  await q.key('Enter')
  assert.deepEqual(q.followed, [])
  assert.ok(document.querySelector('aside[aria-label="Answer, in the inspector"]'), 'the answer is open')
  q.done()
})

/* One Esc is one act. The palette closes itself on an Esc in its field; if that same keydown then
 * goes on to the app's window handler, the palette is already gone by the time the handler looks,
 * so on Check it again and The whole story ("Esc: Back to the Change") the Esc also left the screen.
 * Census key:close on Repair-Loop, every mode. */
test('an Esc that closes the palette is the palette’s alone: the app’s own Esc (Back to the Change) never sees it', async () => {
  const p = await palette()
  const reached = []
  const seen = (e) => reached.push(e.key)
  window.addEventListener('keydown', seen)
  try {
    await p.key('Escape')
  } finally {
    window.removeEventListener('keydown', seen)
  }
  assert.equal(p.closed.length, 1, 'the palette closes')
  assert.deepEqual(reached, [], 'and the keydown stops with it')
  p.done()
})

test('what is typed is told to the screen, so the record is searched for it; Esc’s clear tells it too', async () => {
  const told = []
  const p = await palette({ onQuery: (q) => told.push(q) })
  await p.type('migration')
  await p.key('Escape')
  assert.deepEqual(told, ['migration', ''])
  p.done()
})
