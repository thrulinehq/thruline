// RUN-LINK-TRUTH item 6 (PW-F11, words only): the open run sheet names the
// version the Change shows after the run — the one the record now holds —
// not the one it was opened on. The version still moves on each check run;
// that half is founder-gated and not changed here.
import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-run-version-'))
after(() => rm(temporary, { recursive: true, force: true }))
const result = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  stdin: { resolveDir: here, contents: `
    import React from 'react';
    import { createRoot } from 'react-dom/client';
    import { flushSync } from 'react-dom';
    import { RunSheetHost, RunDoor } from './run-sheet.jsx';
    import { HQ_COPY } from './hq-copy.js';
    export { React, createRoot, flushSync, RunSheetHost, RunDoor, HQ_COPY };
  ` },
})
const bundled = join(temporary, 'run-sheet-version.mjs')
await writeFile(bundled, result.outputFiles[0].text)
const M = await import(pathToFileURL(bundled).href)
const R = M.HQ_COPY.run
const GLOBALS = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'MouseEvent', 'localStorage', 'fetch']
const tick = () => new Promise(resolve => setTimeout(resolve, 10))

async function until(pred, why) {
  const deadline = Date.now() + 3000
  while (!pred()) {
    assert.ok(Date.now() < deadline, why)
    await tick()
  }
}

const SHEET = {
  schema_version: 1, change: 'c1', project_folder: '~/shop',
  permission: { granted_by: 'walker', granted_at: '2026-09-23T10:00:00Z' },
  checks: [{ id: 'cargo-test', program: 'cargo', category: 'tests', command: 'cargo test',
    last: { run: 'r1', state: 'completed', outcome: 'passed', exit_code: 0, tests_passed: 2, tests_failed: 0, suites: 1, recorded_at: '2026-09-23T10:05:00Z', settles: true } }],
  environments: [{ id: 'hq', kind: 'hq', folder: null, has: ['cargo'] }],
  not_runnable_here: [], remembered_environment: null, not_started: null, actor: 'walker', job: null,
}

test('the open sheet names the version the Change shows now, not the one it opened on', async () => {
  const dom = new JSDOM('<!doctype html><body><div id="root"></div></body>', { url: 'http://127.0.0.1/', pretendToBeVisual: true })
  const w = dom.window
  const held = GLOBALS.map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)])
  const set = (name, value) => Object.defineProperty(globalThis, name, { value, configurable: true, writable: true })
  for (const name of GLOBALS) if (w[name] !== undefined) set(name, w[name])
  const reply = (status, doc) => ({ ok: status >= 200 && status < 300, status, json: async () => doc })
  set('fetch', async (url, options = {}) => {
    const method = (options.method || 'GET').toUpperCase()
    if (url === '/api/changes/c1/run' && method === 'GET') return reply(200, SHEET)
    throw new Error(`unexpected synthetic request: ${method} ${url}`)
  })
  const root = M.createRoot(w.document.getElementById('root'))
  const text = () => w.document.body.textContent
  const screen = (short) => M.React.createElement(M.RunSheetHost, null,
    M.React.createElement(M.RunDoor, { changeId: 'c1', version: { short } }))
  try {
    M.flushSync(() => root.render(screen('01M3M3N')))
    const door = [...w.document.querySelectorAll('button')].find(b => b.textContent.trim() === R.door)
    assert.ok(door, 'the door renders')
    M.flushSync(() => door.click())
    const before = R.can_version.replace('{revision}', '01M3M3N')
    await until(() => text().includes(before), 'the sheet names the version it opened on')

    // The run ended; the Change re-read the record and now shows the version
    // the run was recorded on.
    M.flushSync(() => root.render(screen('01M3M3Q')))
    const now = R.can_version.replace('{revision}', '01M3M3Q')
    await until(() => text().includes(now), 'the open sheet must name the version the Change shows now')
    assert.ok(!text().includes(before), 'and no longer the one it opened on')
  } finally {
    M.flushSync(() => root.unmount())
    dom.window.close()
    for (const [name, descriptor] of held) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor)
      else delete globalThis[name]
    }
  }
})
