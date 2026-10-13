/* The whole story on the person's stored choices (C30 R-PREFS), as it ships:
 * the live screen (index.jsx) mounted in jsdom, its preferences read from a
 * fake server.
 *
 *  1. Face: Technical draws the file's own technical part (format, ids, sha256,
 *     size) and Plain draws none; the button writes the stored face, so any
 *     other screen's button changes this one; a refused write leaves the face.
 *  2. Masked copy: with `masked_copy` true the redacted copy starts ticked and
 *     the page says it was started by Settings; with false it starts off. Once
 *     the person touches the redacted copy here, the page keeps what they chose.
 *
 * The one Change loader and the tracked read are replaced by fixed synthetic
 * answers; `fetch` is a stub. Nothing reaches a server or a store. Each test
 * bundles the screen afresh, so each has its own preferences store. */
import { test, afterEach } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import { DEMO as BRIEF } from '../Change-Brief/demo.js'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>', { url: 'http://127.0.0.1/' })
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const here = dirname(fileURLToPath(import.meta.url))
const FORBIDDEN = /\b(lie|lies|cheat|violate|ignore|hallucinat\w*|trial)\b|\d\s?%/i

async function fresh() {
  const ENTRY = `
import React from 'react'
import { createRoot } from 'react-dom/client'
import Screen from './index.jsx'
export function mount(el, at) { const root = createRoot(el); root.render(React.createElement(Screen, { at })); return root }`
  const CHANGE_STUB = `
export async function loadChange(at, { readTracked } = {}) {
  const L = globalThis.__WS
  if (L.trackedId && readTracked) await readTracked(L.trackedId)
  return { snapshot: L.snapshot, where: {}, work: null }
}`
  const TRACKED_STUB = `export async function readTracked(id) { return globalThis.__WS.tracked }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
    plugins: [{
      name: 'live-stubs',
      setup(b) {
        b.onResolve({ filter: /data\/use-change\.js$/ }, () => ({ path: 'use-change', namespace: 'stub' }))
        b.onResolve({ filter: /^\.\/tracked\.js$/ }, () => ({ path: 'tracked', namespace: 'stub' }))
        b.onLoad({ filter: /^use-change$/, namespace: 'stub' }, () => ({ contents: CHANGE_STUB, loader: 'js', resolveDir: here }))
        b.onLoad({ filter: /^tracked$/, namespace: 'stub' }, () => ({ contents: TRACKED_STUB, loader: 'js', resolveDir: here }))
      },
    }],
  })
  const file = join(await mkdtemp(join(tmpdir(), 'ws-prefs-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}

const HOME = '/Users/someone'
const READY = BRIEF.states.ready
const TRACKED = {
  change: { id: 'chg_9', accepted_scope: { acceptance: { accepted_at: '2026-09-26T10:36:00Z' } }, decisions: { standing: null, history: [] } },
  hq: { version: { short: 'e71c5b0' }, criteria: [{ n: 1, text: 'Totals are right', state: 'proven', proven: { version: 'e71c5b0', scope: ['checkout/'] } }] },
}
const RECORDS = {
  schema_version: 1, change_id: 'chg_9', checked_at: '2026-09-26T10:45:00Z', worktree: HOME + '/code/checkout-web',
  decisions: [],
  evidence: [{ kind: 'check_run', evidence_id: 'ev_1', session_id: 's1', criteria: [1], name: 'pytest -q', standing: 'missing', execution_recorded: null }],
  transcripts: [{ session_id: 's1', file: HOME + '/.claude/projects/x/s1.jsonl', lines: 9, lines_gone: 0, lines_changed: 0, standing: 'readable' }],
  totals: { records: 2, readable: 1, not_readable: 1 },
  masking: { state: 'ran', detector: 'v3', masked: 0, patterns: [], statement: 'detector v3 · nothing matched a known secret shape' },
}
const DEFAULT_PREFS = { face: 'plain', density: 'comfortable', motion: 'system', colour_vision: 'standard', keys: 'on', masked_copy: true, notifications: { opposite: true, moved: true, finished: true }, weekly_summary: false }

const ROOTS = []
afterEach(() => { while (ROOTS.length) ROOTS.pop().unmount() })
async function settle(cond = () => false, turns = 400) {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}
const textOf = (el) => (el ? el.textContent.replace(/\s+/g, ' ').trim() : '')
const button = (label) => [...document.querySelectorAll('button')].find((b) => textOf(b) === label)
const rail = () => document.querySelector('aside[aria-label="Export and contents"]')
const technical = () => document.querySelector('[role="group"][aria-label="The file’s technical details"]')
const faceButton = () => document.querySelector('button[aria-label^="Face:"]')
const cutSwitch = () => document.querySelector('button[role="switch"][aria-labelledby="ws-cut-label"]')

/** Mount the live screen with the person's stored choices in `stored`; returns what it asked for. */
async function mount({ stored = {}, refuse = null } = {}) {
  const M = await fresh()
  const held = JSON.parse(JSON.stringify({ ...DEFAULT_PREFS, ...stored }))
  const requests = []
  const blobs = []
  const clicks = []
  const doc = () => ({ schema_version: 1, preferences: held, defaults: DEFAULT_PREFS, file: 'read', from_file: Object.keys(stored), unknown_keys: [], problems: [] })
  globalThis.fetch = async (url, init = {}) => {
    const method = (init.method || 'GET').toUpperCase()
    requests.push(method + ' ' + url + (init.body ? ' ' + init.body : ''))
    if (url === '/api/preferences') {
      if (method === 'GET') return { ok: true, status: 200, headers: { get: () => null }, json: async () => doc() }
      if (refuse) return { ok: false, status: 409, headers: { get: () => null }, json: async () => ({ error: refuse }) }
      Object.assign(held, JSON.parse(init.body))
      return { ok: true, status: 200, headers: { get: () => null }, json: async () => doc() }
    }
    if (method === 'GET' && url === '/api/changes/chg_9/export') return { ok: true, status: 200, headers: { get: () => null }, json: async () => RECORDS, text: async () => JSON.stringify(RECORDS) }
    return { ok: false, status: 404, headers: { get: () => null }, json: async () => ({ error: 'not in the stub' }), text: async () => '{"error":"not in the stub"}' }
  }
  for (const U of [window.URL, globalThis.URL]) {
    U.createObjectURL = (blob) => { blobs.push(blob); return 'blob:ws-' + blobs.length }
    U.revokeObjectURL = () => {}
  }
  window.HTMLAnchorElement.prototype.click = function click() { clicks.push({ href: this.getAttribute('href'), download: this.getAttribute('download') }) }
  globalThis.__WS = { snapshot: { ...READY, change: { ...READY.change, id: 'chg_9' } }, tracked: TRACKED, trackedId: 'chg_9' }
  document.body.innerHTML = '<div id="root"></div>'
  const root = M.mount(document.getElementById('root'), { screen: 'Whole-Story', change: 'chg_9', project: 'checkout-web' })
  ROOTS.push(root)
  await settle(() => rail() && button('Download .md') && /could be read/.test(textOf(rail())))
  // the person's choices are read once, at the start, and the page draws what was read
  await settle(() => requests.includes('GET /api/preferences'))
  await settle()
  return { requests, blobs, clicks, held }
}
const previewText = () => [...document.querySelectorAll('[aria-label="The file, word for word"] > div')].map((d) => (d.textContent === ' ' ? '' : d.textContent)).join('\n') + '\n'

/* ---------------- 1. face ---------------- */

test('1. the stored face Technical draws the file’s technical part on the rail, with this exact file’s fingerprint', async () => {
  await mount({ stored: { face: 'technical', masked_copy: false } })
  assert.equal(faceButton().getAttribute('aria-label'), 'Face: Technical (switch to Plain)')
  assert.equal(faceButton().getAttribute('aria-pressed'), 'true')
  const t = technical()
  assert.ok(t, 'the technical part is on the rail')
  const lines = [...t.children].map(textOf)
  assert.ok(lines.includes('Format: thruline-packet 1'), lines.join(' | '))
  assert.ok(lines.includes('Change id: chg_9'))
  assert.ok(lines.some((l) => /^Fingerprint of this exact file \(sha256\): [0-9a-f]{64}$/.test(l)), 'the sha256 of the file')
  assert.ok(lines.some((l) => /^[\d,]+ bytes · text\/markdown/.test(l)), 'the size')
  assert.ok(lines.includes('Record: ev ev_1 · missing') || lines.some((l) => l.startsWith('Record: ')), 'each record by its code')
  for (const l of lines) assert.doesNotMatch(l, FORBIDDEN, l)
})

test('1. the stored face Plain draws no technical part', async () => {
  await mount({ stored: { face: 'plain', masked_copy: false } })
  assert.equal(faceButton().getAttribute('aria-label'), 'Face: Plain (switch to Technical)')
  assert.equal(technical(), null)
})

test('1. the button writes the stored face (so another screen’s button changes this one); Technical then Plain draws and removes the part', async () => {
  const s = await mount({ stored: { masked_copy: false } })
  assert.equal(technical(), null)
  faceButton().dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await settle(() => technical())
  assert.ok(technical())
  assert.equal(s.held.face, 'technical')
  faceButton().dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await settle(() => !technical())
  assert.equal(technical(), null)
  assert.deepEqual(s.requests.filter((r) => r.startsWith('POST')), ['POST /api/preferences {"face":"technical"}', 'POST /api/preferences {"face":"plain"}'])
})

test('1. a face the server will not keep stays where it was', async () => {
  const s = await mount({ stored: { masked_copy: false }, refuse: 'preferences_file_newer' })
  faceButton().dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await settle()
  assert.equal(technical(), null)
  assert.equal(faceButton().getAttribute('aria-label'), 'Face: Plain (switch to Technical)')
  assert.equal(s.requests.filter((r) => r.startsWith('POST')).length, 1)
})

/* ---------------- 2. masked copy ---------------- */

test('2. masked_copy true: the redacted copy starts ticked, says Settings started it, and Download saves the redacted file', async () => {
  const s = await mount({ stored: { masked_copy: true } })
  assert.equal(cutSwitch().getAttribute('aria-checked'), 'true')
  assert.ok(textOf(rail()).includes('On · a second file, leaving out what you tick · started on by Settings'), textOf(rail()).slice(textOf(rail()).indexOf('Redacted copy'), textOf(rail()).indexOf('Redacted copy') + 200))
  const boxes = [...document.querySelectorAll('[role="checkbox"]')]
  assert.ok(boxes.length > 0 && boxes.every((b) => b.getAttribute('aria-checked') === 'true'), 'every kind it can leave out is ticked')
  assert.ok(textOf(rail()).includes('redacted copy') || previewText().includes('left out'))
  button('Download .md').dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await settle(() => s.clicks.length === 1)
  assert.match(s.clicks[0].download, /-redacted\.md$/)
  assert.equal(await s.blobs[0].text(), previewText(), 'what is saved is what the preview shows')
})

test('2. masked_copy false: the redacted copy starts off and Download saves the full file', async () => {
  const s = await mount({ stored: { masked_copy: false } })
  assert.equal(cutSwitch().getAttribute('aria-checked'), 'false')
  assert.ok(!textOf(rail()).includes('started on by Settings'))
  button('Download .md').dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await settle(() => s.clicks.length === 1)
  assert.doesNotMatch(s.clicks[0].download, /redacted/)
})

test('2. once the person touches the redacted copy here, the page keeps what they chose', async () => {
  const s = await mount({ stored: { masked_copy: true } })
  cutSwitch().dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await settle(() => cutSwitch().getAttribute('aria-checked') === 'false')
  assert.equal(cutSwitch().getAttribute('aria-checked'), 'false')
  assert.ok(!textOf(rail()).includes('started on by Settings'))
  button('Download .md').dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await settle(() => s.clicks.length === 1)
  assert.doesNotMatch(s.clicks[0].download, /redacted/)
  // turning it on again by hand is the person's own act, not Settings'
  cutSwitch().dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await settle(() => cutSwitch().getAttribute('aria-checked') === 'true')
  assert.equal(cutSwitch().getAttribute('aria-checked'), 'true')
  assert.ok(!textOf(rail()).includes('started on by Settings'))
  assert.deepEqual(s.requests.filter((r) => r.startsWith('POST')), [], 'the page changes nothing that is stored')
})
