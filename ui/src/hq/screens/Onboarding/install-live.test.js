/* First run on a live address (index.jsx, as it ships), mounted in jsdom: the
 * Install click end to end against a stub server (C30 R-ROUTES "Install";
 * census §3.11 rows 5, 6, 14).
 *
 * `fetch` is a stub that answers from synthetic documents and records every
 * request, so nothing here reaches a server, a settings file or a store.
 * What is pinned:
 *  - the chosen folder's install plan is read at once, with a GET;
 *  - "Install for this project" reads the run token, then POSTs exactly the
 *    ticked boxes and the plan it showed — the only POST on the screen;
 *  - after the answer, the plan is the one the route read after writing, and
 *    step 3's capture read is done again, so step 3 says "Installed";
 *  - a second visit to step 2 offers "Installed · go to step 3", which posts
 *    nothing. */
import { test, afterEach } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import * as F from './capture-fixture.js'
import * as P from './install-fixture.js'

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
  const file = join(await mkdtemp(join(tmpdir(), 'hq-install-live-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const PROJECTS = {
  schema_version: 1, complete: true,
  projects: [
    { complete: true, id: 'p1', name: 'shop', root: F.ROOT, sources: ['claude_code'], sessions: 0, last_activity: F.at(16, 5), threads: 0 },
  ],
}

const timers = { setInterval: () => 0, clearInterval: () => {} }
async function settle(cond = () => false, turns = 800) {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}
const ROOTS = []
afterEach(() => { while (ROOTS.length) ROOTS.pop().unmount() })
const squash = (s) => s.replace(/\s+/g, ' ').trim()
const stepButton = (n) => document.querySelector(`nav[aria-label="Steps"] button:nth-of-type(${n})`)
const installButton = () => [...document.querySelectorAll('button.tl-btn')].find((b) => /Install/.test(b.textContent) && b.offsetParent !== undefined && b.closest('section') && b.closest('section').style.display !== 'none')
const step3 = () => document.querySelector('section[aria-label="Step 3: Check it’s recording"]')

/**
 * Mount First run against a stub server. Before the POST it answers `before`
 * (capture, plan); the POST answers `outcomes` with the plan `after`, and
 * from then on the capture and plan reads answer `after`.
 */
async function mount({
  before = { capture: F.none(), plan: P.plan() },
  after = { capture: F.waiting(), plan: P.plan({ plan: P.NEXT_PLAN_ID, states: { hooks: 'installed', mcp: 'installed' } }) },
  outcomes = { hooks: 'wrote', mcp: 'wrote' },
} = {}) {
  const requests = []
  const bodies = []
  let installed = false
  globalThis.fetch = async (url, init = {}) => {
    const method = (init.method || 'GET').toUpperCase()
    requests.push(method + ' ' + url)
    const json = (status, body) => ({ ok: status < 400, status, headers: { get: () => null }, json: async () => body, text: async () => JSON.stringify(body) })
    if (url === '/api/hq/projects/p1/install' && method === 'POST') {
      bodies.push(JSON.parse(init.body))
      installed = true
      return json(200, P.answer(outcomes, after.plan))
    }
    if (method !== 'GET') return json(405, { error: 'method_not_allowed' })
    if (url === '/api/hq/projects') return json(200, PROJECTS)
    if (url === '/api/hq/load') return json(200, { schema_version: 1, running: false, project: null, finished_at: null })
    if (url === '/api/storage') return json(200, { bytes: 1e6, path: F.HOME + '/.thruline' })
    if (url === '/api/run/token') return json(200, { token: 'T'.repeat(64) })
    if (url === '/api/hq/projects/p1/capture') return json(200, installed ? after.capture : before.capture)
    if (url === '/api/hq/projects/p1/install') return json(200, installed ? after.plan : before.plan)
    return json(404, { error: 'not in the stub' })
  }
  document.body.innerHTML = '<div id="root"></div>'
  ROOTS.push(M.mount(document.getElementById('root'), timers))
  await settle(() => requests.includes('GET /api/hq/projects/p1/install') && requests.includes('GET /api/hq/projects/p1/capture'))
  return { requests, bodies }
}

test('Install sends exactly the ticked boxes against the plan, then step 3 reads the capture again and says Installed', async () => {
  const m = await mount()
  stepButton(2).click()
  await settle(() => installButton() && squash(installButton().textContent) === 'Install for this project' && !installButton().disabled)
  const b = installButton()
  assert.equal(b.getAttribute('title'), 'Writes .claude/settings.local.json and .mcp.json in this folder. Nothing else.')
  const capturesBefore = m.requests.filter((r) => r.endsWith('/capture')).length
  b.click()
  await settle(() => m.bodies.length === 1 && step3() && step3().style.display !== 'none' && /Installed\. Start Claude Code in shop\./.test(squash(step3().textContent)))
  assert.deepEqual(m.bodies, [{ hooks: true, mcp: true, speaker: false, run_capture: false, plan: P.PLAN_ID }])
  const posts = m.requests.filter((r) => r.startsWith('POST '))
  assert.deepEqual(posts, ['POST /api/hq/projects/p1/install'], 'the install is the only write')
  const tokenAt = m.requests.indexOf('GET /api/run/token')
  assert.ok(tokenAt >= 0 && tokenAt < m.requests.indexOf('POST /api/hq/projects/p1/install'), 'the token is read first')
  assert.ok(m.requests.filter((r) => r.endsWith('/capture')).length > capturesBefore, 'step 3 reads the capture again after the install')
  const t = squash(step3().textContent)
  assert.match(t, /MCP server[^✓]*✓ ?Installed/)

  // a second visit: nothing left to add, so the button goes on to step 3 and posts nothing
  stepButton(2).click()
  await settle(() => installButton() && squash(installButton().textContent) === 'Installed · go to step 3')
  installButton().click()
  await settle(() => step3().style.display !== 'none', 50)
  assert.deepEqual(m.requests.filter((r) => r.startsWith('POST ')), ['POST /api/hq/projects/p1/install'])
})

test('row 10: Turn on, where the hooks are in, installs run capture alone; step 3 stays, reads the capture after the write, and shows the one line', async () => {
  const hooksIn = { hooks: 'installed' }
  const m = await mount({
    before: { capture: F.waiting(), plan: P.plan({ states: hooksIn }) },
    after: { capture: F.waiting(), plan: P.plan({ plan: P.NEXT_PLAN_ID, states: { hooks: 'installed', run: 'installed' } }) },
    outcomes: { hooks: 'already', run: 'wrote' },
  })
  stepButton(3).click()
  const runRow = () => [...step3().querySelectorAll('[role="listitem"]')].find((li) => squash(li.children[0].textContent) === 'Run capture')
  const turnOn = () => runRow() && [...runRow().querySelectorAll('button')].find((b) => squash(b.textContent) === 'Turn on' && b.style.display !== 'none')
  await settle(() => step3().style.display !== 'none' && turnOn())
  turnOn().click()
  await settle(() => m.bodies.length === 1 && /One line to add/.test(squash(runRow().textContent)))
  assert.deepEqual(m.bodies, [{ hooks: true, mcp: false, speaker: false, run_capture: true, plan: P.PLAN_ID }])
  const post = m.requests.indexOf('POST /api/hq/projects/p1/install')
  await settle(() => m.requests.lastIndexOf('GET /api/hq/projects/p1/capture') > post)
  assert.ok(m.requests.lastIndexOf('GET /api/hq/projects/p1/capture') > post, 'the capture is read again after the install, with no step change to prompt it')
  assert.notEqual(step3().style.display, 'none', 'Turn on stays on step 3')
  const sec = document.querySelector('section[aria-label="Turn on run capture"]')
  assert.notEqual(sec.style.display, 'none')
  assert.match(squash(sec.textContent), /export PATH="\$HOME\/\.thruline\/shim:\$PATH"/)
})
