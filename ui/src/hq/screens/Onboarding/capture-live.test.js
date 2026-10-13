/* First run on a live address (index.jsx, as it ships), mounted in jsdom,
 * reading the capture route for the chosen folder (C30 R-ROUTES "Capture
 * health"; census §3.11 rows 3, 8, 12, 13, 14, 17).
 *
 * `fetch` is a stub that answers from synthetic documents (capture-fixture.js)
 * and records every request; the re-read clock is a fake the test drives. So
 * nothing here reaches a server or a store, and nothing waits on wall-clock.
 * What is pinned:
 *  - the chosen folder's capture is read at once, and no other folder's;
 *  - opening step 3 reads it again; while it says waiting it is re-read on a
 *    2 s clock, and the clock stops once a first step is seen;
 *  - Look again reads it now;
 *  - recording reads the project and its first thread, and Done links there;
 *  - a failed read is said;
 *  - every request is a GET. */
import { test, afterEach } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import * as F from './capture-fixture.js'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>', { url: 'http://127.0.0.1/#/start' })
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { createRoot } from 'react-dom/client'
import Screen from './index.jsx'
export function mount(el, timers) { const root = createRoot(el); root.render(React.createElement(Screen, { at: { screen: 'start' }, timers })); return root }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-capture-live-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const PROJECTS = {
  schema_version: 1, complete: true,
  projects: [
    { complete: true, id: 'p1', name: 'shop', root: F.ROOT, sources: ['claude_code'], sessions: 0, last_activity: F.at(16, 5), threads: 1 },
    { complete: true, id: 'p2', name: 'notes', root: F.HOME + '/code/notes', sources: ['claude_code'], sessions: 0, last_activity: F.at(9, 0, 1), threads: 0 },
  ],
}
const T0 = '0123456789abcdef'
const VIEW = {
  schema_version: 1,
  project: Object.assign({}, PROJECTS.projects[0]),
  threads: [
    { id: 'fedcba9876543210', title: 'A later thread', started_at: F.at(16, 4), last_activity_at: F.at(16, 8), whose_move: 'agent' },
    { id: T0, title: 'Fix the discount total', started_at: F.at(16, 1), last_activity_at: F.at(16, 3), whose_move: 'you' },
  ],
}
const THREAD = {
  project: { id: 'p1', name: 'shop', root: F.ROOT },
  thread: VIEW.threads[1],
  prompts: [{ event_id: 'e1', session_id: 's1', seq: 1, ts: F.at(16, 1), text: 'the checkout total is wrong when a discount code is applied — fix it', work: true, version_id: 'wt-1', raw_file: F.HOME + '/.claude/projects/x/s1.jsonl', raw_line: 1 }],
  edits: [], commands: [], claims: [], versions: [], current_version_id: 'wt-1',
  sessions: [{ id: 's1', ledger_project_id: 'x', source: 'claude_code', origin: 'person', started_at: F.at(16, 1), last_at: F.at(16, 3) }],
  files: [], masking: { masked: 0, families: [], complete: true },
}

function fakeTimers() {
  const live = new Map()
  let id = 0
  return {
    setInterval: (fn, ms) => { live.set(++id, { fn, ms }); return id },
    clearInterval: (t) => { live.delete(t) },
    active: () => [...live.values()],
    fire: () => { for (const t of [...live.values()]) t.fn() },
  }
}
async function settle(cond = () => false, turns = 600) {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}
const ROOTS = []
afterEach(() => { while (ROOTS.length) ROOTS.pop().unmount() })

const squash = (s) => s.replace(/\s+/g, ' ').trim()
const shown = (el) => { for (let e = el; e && e.style; e = e.parentElement) if (e.style.display === 'none') return false; return !!el }
const step3 = () => document.querySelector('section[aria-label="Step 3: Check it’s recording"]')
const h2 = () => squash(step3().querySelector('h2').textContent)
const buttonIn = (el, text) => [...el.querySelectorAll('button')].find((b) => squash(b.textContent) === text)
const radio = (name) => [...document.querySelectorAll('button[role="radio"]')].find((b) => b.textContent.includes(name))

/**
 * Mount First run. `captures` is the scripted list of capture answers for p1
 * (one per read; the last repeats); an Error answers 503.
 */
async function mount(captures) {
  const requests = []
  const script = captures.slice()
  globalThis.fetch = async (url, init = {}) => {
    const method = (init.method || 'GET').toUpperCase()
    requests.push(method + ' ' + url)
    const json = (status, body) => ({ ok: status < 400, status, headers: { get: () => null }, json: async () => body, text: async () => JSON.stringify(body) })
    if (method !== 'GET') return json(405, { error: 'method_not_allowed' })
    if (url === '/api/hq/projects') return json(200, PROJECTS)
    if (url === '/api/hq/load') return json(200, { schema_version: 1, running: false, project: null, finished_at: null })
    if (url === '/api/storage') return json(200, { bytes: 1e6, path: F.HOME + '/.thruline' })
    if (url === '/api/hq/projects/p1/capture') {
      const a = script.length > 1 ? script.shift() : script[0]
      return a instanceof Error ? json(503, { error: 'store_unavailable' }) : json(200, a)
    }
    if (url === '/api/hq/projects/p2/capture') return json(200, F.none('p2'))
    if (url === '/api/hq/projects/p1') return json(200, VIEW)
    if (url === '/api/hq/projects/p1/threads/' + T0) return json(200, THREAD)
    return json(404, { error: 'not in the stub' })
  }
  const clock = fakeTimers()
  document.body.innerHTML = '<div id="root"></div>'
  const root = M.mount(document.getElementById('root'), clock)
  ROOTS.push(root)
  await settle(() => radio('shop') && requests.includes('GET /api/hq/projects/p1/capture') && /Git repository|No Git|didn’t answer/.test(radio('shop').textContent))
  return { requests, clock, captures: () => requests.filter((r) => r.endsWith('/capture')) }
}

test('the chosen folder is read at once — and only it; its Git chip is the route’s answer', async () => {
  const m = await mount([F.waiting()])
  assert.deepEqual(m.captures(), ['GET /api/hq/projects/p1/capture'])
  assert.match(radio('shop').textContent, /Git repository/)
  assert.match(radio('notes').textContent, /Git not checked yet/)
  // choosing the other folder reads that folder
  radio('notes').click()
  await settle(() => m.captures().includes('GET /api/hq/projects/p2/capture') && /No Git repository|Git repository/.test(radio('notes').textContent))
  assert.match(radio('notes').textContent, /Git repository/, 'notes is a repository too, by its own answer')
  assert.equal(m.captures().length, 2)
})

test('step 3: opening it reads again; waiting re-reads on a 2 s clock; a first step stops the clock; Done links to the first thread', async () => {
  const m = await mount([F.waiting(), F.waiting(), F.waiting(), F.recording()])
  document.querySelector('nav[aria-label="Steps"] button:nth-of-type(3)').click()
  await settle(() => m.captures().length === 2 && /Installed\. Start Claude Code in shop\./.test(h2()) && m.clock.active().length === 1)
  assert.equal(m.captures().length, 2, 'opening step 3 read it again')
  assert.equal(m.clock.active().length, 1)
  assert.equal(m.clock.active()[0].ms, 2000)
  m.clock.fire()
  await settle(() => m.captures().length === 3)
  assert.match(h2(), /Installed\. Start Claude Code in shop\./)
  m.clock.fire()
  await settle(() => /shop is recording\./.test(h2()) && m.clock.active().length === 0 && /Review what counts as done/.test(step3().textContent) && document.querySelector('a[href^="#/changes/"]'))
  assert.equal(m.captures().length, 4)
  assert.equal(m.clock.active().length, 0, 'the first step is seen: no more re-reads')
  assert.ok(m.requests.includes('GET /api/hq/projects/p1'))
  assert.ok(m.requests.includes('GET /api/hq/projects/p1/threads/' + T0), 'the first thread (by when it started) is read')
  const done = document.querySelector('a[href^="#/changes/"]')
  assert.equal(done.getAttribute('href'), '#/changes/' + T0 + '/done?project=p1')
  assert.ok(shown(done))
  assert.match(squash(step3().textContent), /You asked · step 1 “the checkout total is wrong when a discount code is applied — fix it”/)
  const inbox = [...step3().querySelectorAll('a')].find((a) => squash(a.textContent).startsWith('Open the Inbox') && shown(a))
  assert.equal(inbox.getAttribute('href'), '#/inbox?project=p1')
  m.clock.fire()
  await settle(() => false, 30)
  assert.equal(m.captures().length, 4)
})

test('Look again reads now, and draws what the record says next', async () => {
  const m = await mount([F.problem(), F.problem(), F.recording()])
  document.querySelector('nav[aria-label="Steps"] button:nth-of-type(3)').click()
  await settle(() => m.captures().length === 2 && /The hooks can’t reach ThruLine\./.test(h2()))
  assert.equal(m.clock.active().length, 0, 'a problem is not polled: the person runs the command, then looks again')
  buttonIn(step3(), 'Look again').click()
  await settle(() => /shop is recording\./.test(h2()))
  assert.equal(m.captures().length, 3)
})

test('a failed read is said — never as installed or not installed — and Look again tries once more', async () => {
  const m = await mount([new Error('503'), new Error('503'), F.none()])
  document.querySelector('nav[aria-label="Steps"] button:nth-of-type(3)').click()
  await settle(() => /The check of shop didn’t answer/.test(h2()))
  assert.match(radio('shop').textContent, /Git not checked yet/)
  const t = squash(step3().textContent)
  assert.match(t, /ThruLine could not open its record, so nothing could be checked\./)
  assert.ok(shown(buttonIn(step3(), 'Look again')))
  buttonIn(step3(), 'Look again').click()
  await settle(() => /Nothing is installed in shop yet/.test(h2()))
  assert.ok(m.requests.every((r) => r.startsWith('GET ')), m.requests.join('\n'))
})
