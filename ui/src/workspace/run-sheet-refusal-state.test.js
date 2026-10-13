// D-049: a refused start does not announce a completed run.
// Drive RunSheet itself: refusal state must come from the POST, never props.
import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-run-refusal-'))
after(() => rm(temporary, { recursive: true, force: true }))
const result = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  stdin: { resolveDir: here, contents: `
    import React from 'react';
    import { createRoot } from 'react-dom/client';
    import { flushSync } from 'react-dom';
    import { RunSheet } from './run-sheet.jsx';
    import { HQ_COPY } from './hq-copy.js';
    export { React, createRoot, flushSync, RunSheet, HQ_COPY };
  ` },
})
const bundled = join(temporary, 'run-sheet-refusal.mjs')
await writeFile(bundled, result.outputFiles[0].text)
const M = await import(pathToFileURL(bundled).href)
const R = M.HQ_COPY.run
const ROUTE = '/api/changes/c1/run'
const GLOBALS = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'MouseEvent', 'localStorage', 'fetch']
const tick = () => new Promise(resolve => setTimeout(resolve, 10))

async function until(pred, why) {
  const deadline = Date.now() + 3000
  while (!pred()) {
    assert.ok(Date.now() < deadline, why)
    await tick()
  }
  // Let effects from the committed render finish before checking callbacks.
  await tick()
  await tick()
}

function sheet(phase) {
  return {
    schema_version: 1, change: 'c1', project_folder: '~/shop',
    permission: { granted_by: 'sam', granted_at: '2026-09-23T10:00:00Z' },
    checks: [{ id: 'cargo-test', program: 'cargo', category: 'tests', command: 'cargo test', last: phase === 'done'
      ? { run: 'outside-run', state: 'completed', outcome: 'passed', exit_code: 0, tests_passed: 1, tests_failed: 0, suites: 1, recorded_at: '2026-09-23T10:05:00Z', settles: true }
      : null }],
    environments: [
      { id: 'hq', kind: 'hq', folder: null, has: ['cargo'] },
      { id: 'home-local', kind: 'machine', folder: '~/.local/bin', has: ['cargo'] },
    ],
    not_runnable_here: [], remembered_environment: null, not_started: null, actor: 'sam',
    job: phase === 'running' ? { run: 'outside-run', check: 'cargo-test', for_this_change: true, phase: 'running', started_at: '2026-09-23T10:04:00Z' } : null,
  }
}

async function mounted(check, { browsePlaces = false } = {}) {
  const dom = new JSDOM('<!doctype html><body><div id="root"></div></body>', { url: 'http://127.0.0.1/', pretendToBeVisual: true })
  const w = dom.window
  const held = GLOBALS.map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)])
  const set = (name, value) => Object.defineProperty(globalThis, name, { value, configurable: true, writable: true })
  for (const name of GLOBALS) if (w[name] !== undefined) set(name, w[name])
  let phase = 'idle'
  let ran = 0
  let reads = 0
  const writes = []
  const unexpected = []
  const reply = (status, doc) => ({ ok: status >= 200 && status < 300, status, json: async () => doc })
  set('fetch', async (url, options = {}) => {
    const method = (options.method || 'GET').toUpperCase()
    if (url === '/api/run/token' && method === 'GET') return reply(200, { token: 'synthetic-token' })
    if (url === ROUTE && method === 'GET') { reads += 1; return reply(200, sheet(phase)) }
    if (url === ROUTE && method === 'POST') {
      const body = JSON.parse(options.body)
      writes.push({ body, token: options.headers['X-ThruLine-Run-Token'] })
      if (body.environment === 'home-local') {
        phase = 'running'
        return reply(202, { run: 'outside-run' })
      }
      return reply(403, { error: 'program_in_project', program: 'cargo', found_at: '~/shop/node_modules/.bin/cargo' })
    }
    unexpected.push({ method, url })
    throw new Error(`unexpected synthetic request: ${method} ${url}`)
  })
  const root = M.createRoot(w.document.getElementById('root'))
  const button = label => [...w.document.querySelectorAll('button')].find(b => b.textContent.trim() === label)
  const click = b => { assert.ok(b, 'expected control is rendered'); assert.equal(b.disabled, false); M.flushSync(() => b.click()) }
  try {
    M.flushSync(() => root.render(M.React.createElement(M.RunSheet, { changeId: 'c1', browsePlaces, onClose() {}, onRan() { ran += 1 } })))
    await until(() => button(R.run), 'the actual RunSheet must finish its initial read')
    await check({ w, button, click, writes, ran: () => ran, reads: () => reads, finish: () => { phase = 'done' } })
    assert.deepEqual(unexpected, [], 'the test must not make an undeclared request')
  } finally {
    M.flushSync(() => root.unmount())
    dom.window.close()
    for (const [name, descriptor] of held) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor)
      else delete globalThis[name]
    }
  }
}

test('Settings opens the existing picker before a run; the selected served place starts exactly one check', async () => {
  await mounted(async t => {
    await until(() => t.w.document.querySelector('.ws-run-places'), 'the Check sheet must expose its existing place picker')
    assert.deepEqual(t.writes, [], 'opening the picker asks no permission and starts no run')
    const row = [...t.w.document.querySelectorAll('.ws-run-places li')].find(li => li.textContent.includes('~/.local/bin'))
    assert.ok(row)
    t.click(row.querySelector('button'))
    await until(() => t.w.document.querySelector('.ws-run-running'), 'the chosen place must start the check')
    assert.deepEqual(t.writes, [{ body: { check: 'cargo-test', environment: 'home-local' }, token: 'synthetic-token' }])
  }, { browsePlaces: true })
})

async function refuseOnce(t) {
  t.click(t.button(R.run))
  await until(() => t.reads() >= 2 && t.button(R.tool.look_elsewhere) && !t.button(R.run)?.disabled, '403 must render the actual refusal and the place-picker control')
  assert.ok(t.w.document.body.textContent.includes(R.refusals.program_in_project.replaceAll('{tool}', 'cargo')))
  assert.deepEqual(t.writes, [{ body: { check: 'cargo-test' }, token: 'synthetic-token' }])
}

test('a refused start renders its remedy without announcing that a run ended', async () => {
  await mounted(async t => {
    await refuseOnce(t)
    assert.equal(t.ran(), 0, 'program_in_project started no job, so onRan must not fire')
  })
})

test('choosing an outside place after refusal announces only the job that actually finishes', async () => {
  await mounted(async t => {
    await refuseOnce(t)
    t.click(t.button(R.tool.look_elsewhere))
    await until(() => t.w.document.querySelector('.ws-run-places'), 'Look somewhere else must open its places')
    const row = [...t.w.document.querySelectorAll('.ws-run-places li')].find(li => li.textContent.includes('~/.local/bin'))
    assert.ok(row, 'the served outside place must be available')
    t.click(row.querySelector('button'))
    await until(() => t.w.document.querySelector('.ws-run-running'), 'the successful retry must show an actual running job')
    assert.deepEqual(t.writes[1], { body: { check: 'cargo-test', environment: 'home-local' }, token: 'synthetic-token' })
    assert.equal(t.ran(), 0, 'a running job has not ended')
    t.finish()
    await until(() => t.w.document.querySelector('[data-outcome="passed"]'), 'the polling read must observe the finished job')
    assert.equal(t.ran(), 1, 'only the successful outside job ending is announced')
  })
})
