#!/usr/bin/env node
/* A3-F14 done-when 3 — the Inbox, and the list of Changes above the session
 * view, are one column with no horizontal scroll at 390 px and at 699 px.
 *
 *   node scripts/testing/inbox-width.mjs
 *
 * What it measures. The REAL `InboxScreen` and `ChangeWayIn`, rendered to
 * static markup over a generated store (`attention-count.fixture.js`, seeded,
 * synthetic: every whose-turn section is populated and the read limit is
 * crossed), under the REAL `workspace.css`, in headless Chrome. For each
 * width the page is re-laid out and `document.documentElement.scrollWidth <=
 * innerWidth` is read back through DevTools.
 *
 * The launch goes through `spawnChromeWithDevTools` (scripts/testing/
 * chrome-launch.mjs): `--headless=new`, the ephemeral DevTools port
 * (`--remote-debugging-port=0`, R-2026-09-24-28), and a fresh `mktemp`
 * profile that is removed afterwards. Nothing is served over a network: the
 * page is a file in its own temporary directory.
 *
 * Exit 0 = both widths fit · 1 = a width scrolls sideways, cuts an element
 * off at the right edge, or lays a list out in more than one column (the
 * numbers are printed) · 2 = the measurement could not run (Chrome absent, bundle
 * failed) — 2 is NOT a pass.
 */

import { createRequire } from 'node:module'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { CHROME_EXECUTABLE, spawnChromeWithDevTools } from './chrome-launch.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const UI = join(HERE, '../../ui')
const WORKSPACE = join(UI, 'src/workspace')
const WIDTHS = [390, 699]
const SEED = 1003

const scratch = await mkdtemp(join(tmpdir(), 'thruline-inbox-width-'))
let chrome = null
let ws = null
async function done(code, message) {
  if (message) (code === 0 ? console.log : console.error)('inbox-width: ' + message)
  try { if (ws) ws.close() } catch { /* already closed */ }
  try { if (chrome) chrome.proc.kill('SIGKILL') } catch { /* already gone */ }
  await rm(scratch, { recursive: true, force: true }).catch(() => {})
  process.exit(code)
}

if (!existsSync(CHROME_EXECUTABLE)) await done(2, 'could not run: Chrome is not installed at the path chrome-launch.mjs names')

/* ── the page ─────────────────────────────────────────────────────────── */

let html
try {
  const { build } = createRequire(join(UI, 'package.json'))('esbuild')
  const out = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    stdin: {
      resolveDir: WORKSPACE,
      contents: `
        import React from 'react';
        import { renderToStaticMarkup } from 'react-dom/server';
        import { InboxScreen, assembleInbox } from './inbox.jsx';
        import { ChangeWayIn, wayInRows } from './change-way-in.jsx';
        import { aStore } from './attention-count.fixture.js';
        export function page(seed) {
          const store = aStore(seed, { count: 34 });
          const inbox = { ...assembleInbox(store.all, store.docsById), storage: { known: true, size: '7.2 GB', path: '/a/very/long/folder/name/that/must/wrap/rather/than/push/the/page/sideways/records', bytes: 7200000000, parts: [] }, notes: [] };
          const strip = renderToStaticMarkup(React.createElement(ChangeWayIn, { state: { status: 'ready', ...wayInRows(inbox) } }));
          const screen = renderToStaticMarkup(React.createElement(InboxScreen, { state: inbox }));
          const sections = (screen.match(/data-section="([a-z]+)"/g) || []).map((s) => s.slice(14, -1));
          return { markup: strip + screen, sections, items: inbox.items.length, unread: inbox.unread };
        }
      `,
    },
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  })
  const bundled = join(scratch, 'page.mjs')
  await writeFile(bundled, out.outputFiles[0].text)
  const { page } = await import(pathToFileURL(bundled).href)
  const p = page(SEED)
  for (const id of ['you', 'check', 'agent', 'recording', 'nothing']) {
    if (!p.sections.includes(id)) await done(2, `could not run: the generated store (seed ${SEED}) does not populate the "${id}" section`)
  }
  if (!p.unread) await done(2, `could not run: the generated store (seed ${SEED}) does not cross the read limit`)
  const css = await readFile(join(WORKSPACE, 'workspace.css'), 'utf8')
  html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><style>${css}</style></head><body style="margin:0">${p.markup}</body></html>`
  console.log(`inbox-width: seed ${SEED}, ${p.items} Changes in the Inbox, ${p.unread} not read, sections ${p.sections.join(' · ')}`)
} catch (e) {
  await done(2, 'could not run: ' + (e && e.message ? e.message : String(e)))
}
const file = join(scratch, 'inbox.html')
await writeFile(file, html)

/* ── the browser ──────────────────────────────────────────────────────── */

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
let send
try {
  chrome = await spawnChromeWithDevTools({ userDataDir: join(scratch, 'profile'), extraArgs: ['--disable-extensions', '--window-size=1024,900'] })
  let target = null
  for (let i = 0; i < 60 && !target; i += 1) {
    try { target = (await (await fetch(`http://127.0.0.1:${chrome.port}/json/list`)).json()).find((t) => t.type === 'page') } catch { /* not yet */ }
    if (!target) await sleep(150)
  }
  if (!target) throw new Error('no Chrome page appeared')
  ws = new WebSocket(target.webSocketDebuggerUrl)
  await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = () => no(new Error('the DevTools socket did not open')) })
  let next = 0
  const pending = new Map()
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data)
    if (msg.id && pending.has(msg.id)) { const p = pending.get(msg.id); pending.delete(msg.id); msg.error ? p.no(new Error(msg.error.message)) : p.ok(msg.result) }
  }
  send = (method, params) => new Promise((ok, no) => {
    const id = ++next
    const timer = setTimeout(() => { pending.delete(id); no(new Error(`${method} got no answer in 10 s`)) }, 10000)
    pending.set(id, { ok: (v) => { clearTimeout(timer); ok(v) }, no: (e) => { clearTimeout(timer); no(e) } })
    ws.send(JSON.stringify({ id, method, params: params || {} }))
  })
  await send('Page.enable')
  await send('Page.navigate', { url: pathToFileURL(file).href })
  await sleep(500)
} catch (e) {
  await done(2, 'could not run: ' + (e && e.message ? e.message : String(e)))
}

const failures = []
for (const width of WIDTHS) {
  await send('Emulation.setDeviceMetricsOverride', { width, height: 900, deviceScaleFactor: 1, mobile: width < 700 })
  await sleep(200)
  const r = await send('Runtime.evaluate', {
    returnByValue: true,
    expression: `(() => {
      const lists = [...document.querySelectorAll('.ws-inbox-list')].map((ul) => getComputedStyle(ul).gridTemplateColumns.split(' ').length);
      const cut = [...document.querySelectorAll('.ws-inbox *, .ws-wayin *')].filter((el) => el.getBoundingClientRect().right > window.innerWidth + 1).length;
      return { scrollWidth: document.documentElement.scrollWidth, innerWidth: window.innerWidth, columns: Math.max(0, ...lists), lists: lists.length, cut };
    })()`,
  })
  const m = r.result.value
  /* Three readings, all required: the page does not scroll sideways, no
   * element of either surface ends past the right edge (an ancestor that
   * clips overflow would hide that from scrollWidth), and every list is one
   * column. */
  const ok = m.scrollWidth <= m.innerWidth && m.cut === 0 && m.columns <= 1
  console.log(`inbox-width: ${width} px — scrollWidth ${m.scrollWidth}, innerWidth ${m.innerWidth}, elements past the right edge ${m.cut}, widest list ${m.columns} column(s) over ${m.lists} list(s): ${ok ? 'fits' : 'DOES NOT FIT'}`)
  if (!ok) failures.push(width)
}
await done(failures.length ? 1 : 0, failures.length ? `not one column without horizontal scroll at ${failures.join(', ')} px` : `one column, no horizontal scroll at ${WIDTHS.join(' and ')} px`)
