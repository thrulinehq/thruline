/* ⌘K as it ships (index.jsx), mounted in jsdom over a tracked Change's
 * `chg_` address: census §3.08 rows 4, 5 and 6 end to end.
 *
 * The one Change loader answers with a fixed snapshot of a tracked Change;
 * `fetch` is a stub that answers the HQ reads and records every request. What
 * is pinned:
 *  - a `chg_` address is answered: the five questions, from its snapshot;
 *  - what is typed is searched across the record, on the Change's own
 *    project, with the open Change named, and what the agent read is a Find
 *    row linking to its Change;
 *  - the thread view's typed deletions reach "Was anything destructive?";
 *  - every request is a GET. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import { changeSnapshot } from '../../data/change.js'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>', { url: 'http://127.0.0.1/' })
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { createRoot } from 'react-dom/client'
import Screen from './index.jsx'
export function mount(el, at) { const root = createRoot(el); root.render(React.createElement(Screen, { at, onClose: () => {} })); return root }`
  const CHANGE_STUB = `
export const THREAD_ID = /^[0-9a-f]{16}$/
export function useChange(at) { return at ? { data: globalThis.__CA.snapshot, loading: false, error: null, work: [] } : { data: null, loading: false, error: new Error('No Change is open.'), work: [] } }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
    plugins: [{
      name: 'live-stubs',
      setup(b) {
        b.onResolve({ filter: /data\/use-change\.js$/ }, () => ({ path: 'use-change', namespace: 'stub' }))
        b.onLoad({ filter: /^use-change$/, namespace: 'stub' }, () => ({ contents: CHANGE_STUB, loader: 'js', resolveDir: here }))
      },
    }],
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-command-live-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const wait = (ms) => new Promise((r) => setTimeout(r, ms))
const A = '0a0a0a0a0a0a0a0a'
const B = '0b0b0b0b0b0b0b0b'
const T = (m) => '2026-10-07T09:0' + m + ':00.000Z'
const VIEW = {
  schema_version: 1,
  project: { id: 'p1', name: 'shop', root: '/r' },
  thread: { id: A, title: 'Fix the total', whose_move: 'you', proof: { edited_after_last_pass: [] } },
  prompts: [{ event_id: 'ev-p', session_id: 's1', ts: T(0), text: 'fix the totals', work: true }],
  edits: [
    { id: 'ev-e1#0', ts: T(1), file: '/r/cart/totals.py', lines_added: 2, lines_removed: 1, ok: true, version_id: 'wt-1', session_id: 's1', op: 'patch', action: 'update' },
    { id: 'ev-e2#1', ts: T(2), file: '/r/cart/old.py', lines_added: 0, lines_removed: 0, ok: true, version_id: 'wt-1', session_id: 's1', op: 'patch', action: 'delete' },
  ],
  commands: [{ id: 'ev-c1#0', ts: T(3), command: 'pytest -q', outcome: 'succeeded', exit_code: 0, is_test: true, tally: { passed: 3, failed: 0 }, version_id: 'wt-1', session_id: 's1' }],
  claims: [{ id: 'ev-s#0', event_id: 'ev-s', ts: T(4), text: 'Tests pass.', kind: 'tests_pass', has_verifier: true, rests_on_command_id: 'ev-c1#0', followed_by_passing_test: false, version_id: 'wt-1', session_id: 's1', verdict: 'proven', verdict_basis: 'passing_run' }],
  versions: [{ id: 'wt-1', committed: false }],
  current_version_id: 'wt-1',
  sessions: [{ id: 's1', source: 'claude_code', origin: 'person', started_at: T(0) }],
  files: [],
  masking: { masked: 0, families: [], complete: true },
}
const PROJECT_VIEW = { project: { id: 'p1', name: 'shop', root: '/r' }, threads: [{ id: A, title: 'Fix the total', whose_move: 'you', top_files: [] }, { id: B, title: 'Write the migration guide', whose_move: 'done', top_files: [] }] }
const SEARCH = {
  schema_version: 1, project_id: 'p1', thread: A, limit: 20, total: 2, by_kind: {}, searched: {}, masking: { masked: 0, families: [], complete: true },
  results: [
    { kind: 'Read', thread_id: A, thread_title: 'Fix the total', open_thread: true, text: 'cart/totals.py', at: T(1), event_id: 'ev-r', count: 1, source: 'claude_code' },
    { kind: 'Version', thread_id: B, thread_title: 'Write the migration guide', open_thread: false, text: '9e8d7c6', at: T(6), event_id: 'ev-v', count: 1, source: 'claude_code', version: { id: '9e8d7c6', short: '9e8d7c6', commit: '9e8d7c6', committed: true, message: 'Add the totals guide', current: false } },
  ],
}

function serve() {
  const calls = []
  globalThis.fetch = async (url, init = {}) => {
    calls.push({ url: String(url), method: init.method || 'GET' })
    const u = String(url)
    const json = (status, body) => ({ ok: status < 400, status, json: async () => body, text: async () => JSON.stringify(body) })
    if (u === '/api/hq/projects') return json(200, { schema_version: 1, projects: [{ id: 'p9', name: 'other', last_activity: T(9) }, { id: 'p1', name: 'shop', last_activity: T(1) }] })
    if (u === '/api/hq/projects/p1') return json(200, PROJECT_VIEW)
    if (u === '/api/hq/projects/p1/threads/' + A) return json(200, VIEW)
    if (u.startsWith('/api/hq/projects/p1/search?')) return json(200, SEARCH)
    return json(404, { error: 'no_such_route' })
  }
  return calls
}

test('a tracked Change’s chg_ address: the five questions, Find across the record, and the deletions the thread view types', async () => {
  globalThis.__CA = { snapshot: changeSnapshot(VIEW, { change: { id: 'chg_1' }, hq: null, proposals: null }, new Date('2026-10-07T10:00:00Z')) }
  const calls = serve()
  document.body.innerHTML = '<div id="root"></div>'
  const root = M.mount(document.getElementById('root'), { screen: 'brief', change: 'chg_1' })
  // the screen re-reads on timers: it is always unmounted, so a failed assertion cannot hold the process open
  try {
    // the reads answer at once, but a busy machine may take a moment to draw them
    for (let i = 0; i < 150 && !document.getElementById('tl-opt-ask-tell'); i += 1) await wait(20)
    const asks = [...document.querySelectorAll('[id^="tl-opt-ask-"]')].map((b) => b.id.slice('tl-opt-ask-'.length))
    assert.deepEqual(asks, ['finish', 'changed', 'destructive', 'notproven', 'tell'])
    // the read is the Change's own project, never the latest one the list names first
    assert.ok(calls.some((c) => c.url === '/api/hq/projects/p1'), calls.map((c) => c.url).join('\n'))
    assert.ok(!calls.some((c) => c.url === '/api/hq/projects/p9'), 'not another project')

    const input = document.getElementById('tl-cmdk-input')
    const set = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set
    set.call(input, 'totals')
    input.dispatchEvent(new window.Event('input', { bubbles: true }))
    for (let i = 0; i < 150 && ![...document.querySelectorAll('a[id^="tl-opt-find-"]')].some((a) => /^Read/.test(a.textContent.trim())); i += 1) await wait(20)
    const search = calls.find((c) => c.url.startsWith('/api/hq/projects/p1/search?'))
    assert.ok(search, calls.map((c) => c.url).join('\n'))
    assert.equal(search.url, '/api/hq/projects/p1/search?q=totals&limit=20&thread=' + A)
    const rows = [...document.querySelectorAll('a[id^="tl-opt-find-"]')]
    const read = rows.find((a) => /cart\/totals\.py/.test(a.textContent) && /^Read/.test(a.textContent.trim()))
    assert.ok(read, rows.map((a) => a.textContent).join('\n'))
    assert.equal(read.getAttribute('href'), '#/changes/' + A + '?project=p1')

    // Was anything destructive? — the deletion the thread view typed
    set.call(input, '')
    input.dispatchEvent(new window.Event('input', { bubbles: true }))
    await wait(20)
    document.getElementById('tl-opt-ask-destructive').click()
    await wait(20)
    const aside = document.querySelector('aside[aria-label="Answer, in the inspector"]')
    assert.ok(aside, 'answered')
    assert.match(aside.textContent, /deleted\s*cart\/old\.py/)
    document.getElementById('tl-opt-ask-finish').click()
    await wait(20)
    assert.match(document.querySelector('aside[aria-label="Answer, in the inspector"]').textContent, /Proven/)

    assert.ok(calls.every((c) => c.method === 'GET'), 'every request is a GET')
  } finally {
    root.unmount()
  }
})
