/* N13-F11-AXE-SR — the product defects the axe sweep
 * (scripts/testing/axe-sweep.mjs) found at serious or critical, pinned in
 * the unit suite so they cannot come back without a browser noticing:
 *
 *   html-has-lang (serious, every view)   — the document names its language;
 *   aria-required-attr (critical)         — a focusable separator states its
 *                                           value, minimum and maximum;
 *   button-name (critical)                — a group card's open/close button
 *                                           always has a name;
 *   target-size (serious, Workbench)      — a rail group caption is at least
 *                                           24 px tall.
 */
import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'
import { HQ_COPY } from './hq-copy.js'
import { CLAMP } from './tokens.js'

const here = dirname(fileURLToPath(import.meta.url))

test('html-has-lang: the app document names its language', () => {
  const html = readFileSync(new URL('../../index.html', import.meta.url), 'utf8')
  assert.match(html, /<html lang="en">/)
})

test('target-size: a rail group caption is at least 24 px tall', () => {
  const src = readFileSync(new URL('./chrome.jsx', import.meta.url), 'utf8')
  const cap = /const RAIL_CAP = Object\.freeze\(\{([\s\S]*?)\}\)/.exec(src)
  assert.ok(cap, 'RAIL_CAP')
  assert.match(cap[1], /minHeight:\s*SIZE\.ctl/, 'the caption keeps the 24 px control height as its floor')
})

const temporary = await mkdtemp(join(tmpdir(), 'thruline-a11y-f11-'))
after(() => rm(temporary, { recursive: true, force: true }))
const result = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  stdin: {
    resolveDir: here,
    contents: `
      import React from 'react';
      import { createRoot } from 'react-dom/client';
      import { flushSync } from 'react-dom';
      import Workspace from './shell.jsx';
      import { a11yFetch } from './a11y-fixture.js';
      export { React, createRoot, flushSync, Workspace, a11yFetch };
    `,
  },
})
const file = join(temporary, 'f11.mjs')
await writeFile(file, result.outputFiles[0].text)
const L = await import(pathToFileURL(file).href)
const settle = () => new Promise((r) => setTimeout(r, 10))
const GLOBALS = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage', 'CSS', 'fetch']

async function at(altitude, run) {
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/?altitude=' + altitude, pretendToBeVisual: true })
  const w = dom.window
  Object.defineProperty(w, 'innerHeight', { value: 1000, configurable: true })
  const held = GLOBALS.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of GLOBALS) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  set('fetch', L.a11yFetch)
  const root = L.createRoot(w.document.getElementById('root'))
  try {
    const ref = L.React.createRef()
    L.flushSync(() => root.render(L.React.createElement(L.Workspace, { ref, index: { projects: [], totals: {}, kind: 'index', contract_version: 1 } })))
    for (let i = 0; i < 60; i += 1) await settle()
    return await run({ d: w.document, ws: ref.current })
  } finally {
    root.unmount()
    for (const [n, desc] of held) { if (desc) Object.defineProperty(globalThis, n, desc); else delete globalThis[n] }
    w.close()
  }
}

test('aria-required-attr: every focusable separator states its value, minimum and maximum — the dock\'s size and the drag\'s clamp', async () => {
  await at('workbench', async ({ d, ws }) => {
    const seps = [...d.querySelectorAll('[role="separator"][tabindex="0"]')]
    assert.equal(seps.length, 3, 'the three dock separators')
    const sizes = ws.sizes()
    const which = { 'Resize left dock (arrow keys)': 'left', 'Resize bottom dock (arrow keys)': 'bottom', 'Resize inspector (arrow keys)': 'right' }
    for (const s of seps) {
      const k = which[s.getAttribute('aria-label')]
      assert.ok(k, s.getAttribute('aria-label'))
      assert.equal(s.getAttribute('aria-valuenow'), String(sizes[k]), `${k}: aria-valuenow`)
      assert.equal(s.getAttribute('aria-valuemin'), String(CLAMP[k][0]), `${k}: aria-valuemin`)
      assert.equal(s.getAttribute('aria-valuemax'), String(CLAMP[k][1]), `${k}: aria-valuemax`)
    }
  })
})

test('button-name: a group card\'s open/close button always has a name, even when the record gives the group none', async () => {
  for (const altitude of ['explore', 'workbench']) {
    await at(altitude, async ({ d }) => {
      const toggles = [...d.querySelectorAll('[role="button"][data-id] button, button[data-id]')]
      assert.ok(toggles.length > 0, `${altitude}: a group card is drawn`)
      for (const b of toggles) {
        const name = (b.getAttribute('aria-label') || b.textContent || '').trim()
        assert.ok(name.length > 1 && !/^[▸▾]$/.test(name), `${altitude}: the toggle on ${b.getAttribute('data-id')} is named "${name}"`)
      }
      const A = HQ_COPY.a11y
      assert.ok(A.group_open && A.group_close, 'the fallback names live in the copy file')
    })
  }
})
