/* The whole story's export rail on the live screen (index.jsx, as it ships),
 * mounted in jsdom: census §3.13 rows 8 and 9 end to end.
 *
 * The one Change loader and the tracked-Change read are replaced by fixed
 * synthetic answers; `fetch` is a stub that records every request, so
 * nothing here reaches a server or a store. What is pinned:
 *
 *  - a tracked Change reads `GET /api/changes/<id>/export`, once, and the rail
 *    draws each record's answer; untracked work reads nothing of the kind;
 *  - Download hands the browser a Blob holding exactly the text the preview
 *    shows, under the file's name; Copy as Markdown writes the Markdown file's
 *    text to the clipboard;
 *  - no request on this screen is anything but a GET. */
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
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { createRoot } from 'react-dom/client'
import Screen from './index.jsx'
export function mount(el, at) { const root = createRoot(el); root.render(React.createElement(Screen, { at })); return root }`
  /* The one Change loader, answering with a fixed snapshot; it calls the
   * screen's own readTracked exactly as loadChange does for a tracked Change. */
  const CHANGE_STUB = `
export async function loadChange(at, { readTracked } = {}) {
  const L = globalThis.__WS
  if (L.trackedId && readTracked) await readTracked(L.trackedId)
  return { snapshot: L.snapshot, where: {}, work: null }
}`
  const TRACKED_STUB = `export async function readTracked(id) { globalThis.__WS.readTrackedWith = id; return globalThis.__WS.tracked }`
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
  const file = join(await mkdtemp(join(tmpdir(), 'ws-export-live-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const HOME = '/Users/someone'
const READY = BRIEF.states.ready
const S1 = BRIEF.states.s1
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

const PREFS_DOC = {
  schema_version: 1, file: 'read', from_file: ['masked_copy'],
  preferences: { face: 'plain', density: 'comfortable', motion: 'system', colour_vision: 'standard', keys: 'on', masked_copy: false, notifications: { opposite: true, moved: true, finished: true }, weekly_summary: false },
}

/* Every mounted screen is unmounted after its test, pass or fail, so no
 * watcher's timer keeps this file running. */
const ROOTS = []
afterEach(() => { while (ROOTS.length) ROOTS.pop().unmount() })

async function settle(cond = () => false, turns = 400) {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}
const textOf = (el) => (el ? el.textContent.replace(/\s+/g, ' ').trim() : '')
const button = (label) => [...document.querySelectorAll('button')].find((b) => textOf(b) === label)
const rail = () => document.querySelector('aside[aria-label="Export and contents"]')
const previewText = () => [...document.querySelectorAll('[aria-label="The file, word for word"] > div')].map((d) => (d.textContent === ' ' ? '' : d.textContent)).join('\n') + '\n'

/** Mount the live screen on a snapshot; returns what it asked for, saved and copied. */
async function mount({ snapshot, tracked, trackedId }) {
  const requests = []
  const blobs = []
  const clicks = []
  const clips = []
  globalThis.fetch = async (url, init = {}) => {
    const method = (init.method || 'GET').toUpperCase()
    requests.push(method + ' ' + url)
    /* C30 R-PREFS: this file pins the full copy's name and text, so the person's stored choice is "no masked copy
     * by default" (masked-copy-default.test.js pins what the stored choice does) */
    if (method === 'GET' && url === '/api/preferences') {
      return { ok: true, status: 200, headers: { get: () => null }, json: async () => PREFS_DOC }
    }
    if (method === 'GET' && url === '/api/changes/' + encodeURIComponent(trackedId || '') + '/export') {
      return { ok: true, status: 200, headers: { get: () => null }, json: async () => RECORDS, text: async () => JSON.stringify(RECORDS) }
    }
    return { ok: false, status: 404, headers: { get: () => null }, json: async () => ({ error: 'not in the stub' }), text: async () => '{"error":"not in the stub"}' }
  }
  /* the bundle's `URL` is this process's own, and jsdom's window has its own: both are stood in for */
  for (const U of [window.URL, globalThis.URL]) {
    U.createObjectURL = (blob) => { blobs.push(blob); return 'blob:ws-' + blobs.length }
    U.revokeObjectURL = () => {}
  }
  window.HTMLAnchorElement.prototype.click = function click() { clicks.push({ href: this.getAttribute('href'), download: this.getAttribute('download') }) }
  Object.defineProperty(window.navigator, 'clipboard', { configurable: true, value: { writeText: async (t) => { clips.push(t) } } })
  globalThis.__WS = { snapshot, tracked, trackedId }
  document.body.innerHTML = '<div id="root"></div>'
  const root = M.mount(document.getElementById('root'), { screen: 'Whole-Story', change: snapshot.change.id, project: 'checkout-web' })
  ROOTS.push(root)
  await settle(() => rail() && button('Download .md'))
  return { root, requests, blobs, clicks, clips }
}

test('a tracked Change: Readable now reads GET /api/changes/<id>/export once, and the rail draws each answer', async () => {
  const snapshot = { ...READY, change: { ...READY.change, id: 'chg_9' } }
  const s = await mount({ snapshot, tracked: TRACKED, trackedId: 'chg_9' })
  await settle(() => /could be read/.test(textOf(rail())))
  const exportReads = s.requests.filter((r) => r.endsWith('/export'))
  assert.deepEqual(exportReads, ['GET /api/changes/chg_9/export'])
  const r = textOf(rail())
  assert.ok(r.includes('1 of the 2 records this rests on could be read.'), r)
  assert.ok(r.includes('missing'), r)
  assert.ok(!r.includes(HOME), 'no home folder on screen')
  assert.deepEqual(s.requests.filter((x) => !x.startsWith('GET ')), [], 'nothing but reads')
})

test('Download hands the browser exactly the previewed text, named by the T-5-4 rule; Copy as Markdown copies the Markdown', async () => {
  const snapshot = { ...READY, change: { ...READY.change, id: 'chg_9' } }
  const s = await mount({ snapshot, tracked: TRACKED, trackedId: 'chg_9' })
  await settle(() => /could be read/.test(textOf(rail())))
  const md = previewText()
  button('Download .md').dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await settle(() => s.blobs.length === 1)
  assert.equal(s.blobs.length, 1)
  assert.equal(await s.blobs[0].text(), md, 'the Blob is the preview, word for word')
  assert.match(s.blobs[0].type, /^text\/markdown/)
  assert.equal(s.clicks.length, 1)
  assert.equal(s.clicks[0].download, 'thruline-packet-fix-double-applied-discount-on-checkout-total-e71c5b0.md')
  assert.ok(textOf(rail()).includes('Your browser has the file now'))

  /* HTML, then Copy as Markdown: the clipboard gets the Markdown file, not the HTML */
  const htmlRadio = [...document.querySelectorAll('[role="radio"]')].find((b) => textOf(b).startsWith('HTML'))
  htmlRadio.dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await settle(() => button('Download .html'))
  const html = previewText()
  assert.ok(html.startsWith('<!doctype html>'))
  button('Download .html').dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await settle(() => s.blobs.length === 2)
  assert.equal(await s.blobs[1].text(), html)
  assert.equal(s.clicks[1].download, 'thruline-packet-fix-double-applied-discount-on-checkout-total-e71c5b0.html')
  button('Copy as Markdown').dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await settle(() => s.clips.length === 1)
  assert.deepEqual(s.clips, [md])
  await settle(() => textOf(rail()).includes('Copied: the same text the Markdown file holds.'))
  assert.ok(textOf(rail()).includes('Copied: the same text the Markdown file holds.'))
  assert.deepEqual(s.requests.filter((x) => !x.startsWith('GET ')), [], 'nothing is posted')
})

test('untracked work: no record route is read, the rail says Readable now is for a tracked Change only, and the file is still made', async () => {
  const s = await mount({ snapshot: S1, tracked: null, trackedId: null })
  await settle()
  assert.deepEqual(s.requests.filter((r) => r.includes('/api/changes/')), [], 'no Change route for untracked work')
  const r = textOf(rail())
  assert.ok(r.includes('tracked Change only'), r)
  assert.ok(!/Every one of the|records this rests on could be read/.test(r), r)
  const md = previewText()
  assert.ok(md.startsWith('# ' + S1.change.title + '\n'))
  button('Download .md').dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await settle(() => s.blobs.length === 1)
  assert.equal(await s.blobs[0].text(), md)
  assert.deepEqual(s.requests.filter((x) => !x.startsWith('GET ')), [])
})
