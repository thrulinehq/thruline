/* C30 R-NAV (CTO sign-off fix): a read that failed, a read still out, or a
 * screen that throws is said INSIDE the app frame — the top bar and the
 * sidebar stay — on every routed screen. It is never a blank page and never
 * a full page of its own. The Team sheet showed it: "could not read the
 * approvals" was the whole page, with nothing to click.
 *
 * Four things are pinned here, each against the real code:
 *   1. Every routed address, mounted through the real App with every read
 *      failing, keeps the frame: a top bar, the sidebar's six sections (this
 *      screen's current where it has one), and the line in the main area.
 *   2. The Team case itself: projects read, approvals not.
 *   3. A screen that throws while drawing is said in the frame, and moving
 *      to another address clears it.
 *   4. No screen's source draws a `100vh` ground of its own (the old way
 *      every one of them failed). The frame template is the one place.
 *
 * Synthetic documents only. Nothing here reads a store or the network. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>', { url: 'http://127.0.0.1/' })
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const here = dirname(fileURLToPath(import.meta.url))

async function bundle(entry) {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: entry, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-frame-')), 'm.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}

const M = await bundle(`
import React from 'react'
import { createRoot } from 'react-dom/client'
import { flushSync } from 'react-dom'
import App from './app.jsx'
import { FrameContext, FrameState, ScreenBoundary, SECTIONS, COULD_NOT_DRAW } from './frame-state.jsx'
import { KEY_TABLE } from './keys/table.js'
export { SECTIONS, COULD_NOT_DRAW, KEY_TABLE }
export function mount(node) {
  const el = document.createElement('div')
  document.body.appendChild(el)
  const root = createRoot(el)
  flushSync(() => root.render(node))
  return { el, unmount: () => { root.unmount(); el.remove() } }
}
export const mountApp = () => mount(React.createElement(App))
export function mountBoundary(Boom, key, at) {
  const tree = (k, boom) => React.createElement(FrameContext.Provider, { value: { at: at || { screen: 'inbox' }, openCommand: null } },
    React.createElement(ScreenBoundary, { resetKey: k }, boom ? React.createElement(Boom) : React.createElement('p', { id: 'fine' }, 'drawn')))
  const el = document.createElement('div')
  document.body.appendChild(el)
  const root = createRoot(el)
  flushSync(() => root.render(tree(key, true)))
  return { el, again: (k, boom) => flushSync(() => root.render(tree(k, boom))), unmount: () => { root.unmount(); el.remove() } }
}
export { FrameState, FrameContext, React, createRoot, flushSync }
`)

/* ---------------- helpers ---------------- */

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
async function until(fn, what, ms = 4000) {
  const end = Date.now() + ms
  for (;;) {
    const v = fn()
    if (v) return v
    if (Date.now() > end) throw new Error('never happened: ' + what)
    await sleep(15)
  }
}
const json = (body, status = 200) => ({ ok: status < 400, status, text: async () => JSON.stringify(body) })
const SECTION_LABELS = ['Inbox', 'Changes', 'Projects', 'Record over time', 'Team', 'Settings']

/** The frame, as a person would find it: what is drawn, and where the line is. */
function frameOf(root) {
  const header = root.querySelector('header')
  const aside = root.querySelector('aside[aria-label="HQ"]')
  const nav = aside ? aside.querySelector('nav[aria-label="Sections"]') : null
  const main = root.querySelector('main')
  const alert = root.querySelector('[role="alert"]')
  return { header, aside, nav, main, alert }
}

/** The frame is intact: top bar, six sections that go somewhere, and the line inside the main area. */
function assertFrameKept(root, label) {
  const f = frameOf(root)
  assert.ok(f.header, label + ': the top bar is drawn')
  assert.ok(f.header.querySelector('a[aria-label="Inbox, home"]'), label + ': the top bar has its way home')
  assert.ok(f.header.querySelector('nav[aria-label="Where you are"]'), label + ': the top bar says where you are')
  assert.ok(f.aside && f.nav, label + ': the sidebar is drawn')
  const links = [...f.nav.querySelectorAll('a')]
  assert.deepEqual(links.map((a) => a.textContent.replace(/g [a-z]$/, '').trim()), SECTION_LABELS, label + ': the six sections')
  assert.ok(links.every((a) => /^#\//.test(a.getAttribute('href') || '')), label + ': every section goes somewhere')
  assert.ok(f.main && f.alert, label + ': the line is drawn in the main area')
  assert.ok(f.main.contains(f.alert), label + ': the line is INSIDE the main area, beside the sidebar')
  assert.ok(f.alert.textContent.trim().length > 0, label + ': the line says something')
  assert.equal(f.main.parentElement, f.aside.parentElement, label + ': the sidebar and the line share one body')
  return f
}

/* ---------------- 1. every routed address, every read failing ---------------- */

// each address the router reads, with a word the failing screen is known to say
const ADDRESSES = [
  ['#/inbox', 'Inbox', /could not read the record|did not answer/],
  ['#/changes', 'Changes', /could not read the record|did not answer/],
  ['#/projects', 'Projects', /could not read which projects|did not answer/],
  ['#/record', 'Record over time', /did not answer/],
  ['#/team', 'Team', /did not answer/],
  ['#/settings', 'Settings', /did not answer/],
  ['#/start', 'First run', /./],
  ['#/changes/c1', 'Brief', /did not answer/],
  ['#/changes/c1/statements', 'Every statement', /did not answer/],
  ['#/changes/c1/changed', 'What changed', /did not answer/],
  ['#/changes/c1/done', 'What counts as done', /did not answer/],
  ['#/changes/c1/check', 'Check it again', /did not answer/],
  ['#/changes/c1/approve', 'Approve', /did not answer/],
  ['#/changes/c1/story', 'The whole story', /did not answer/],
  ['#/changes/c1/pr', 'On the pull request', /did not answer/],
  ['#/changes/c1/step/3', 'One step', /did not answer/],
]

for (const [hash, label, said] of ADDRESSES) {
  test(`${hash}: with every read failing, the frame stays and the line is in it`, async () => {
    globalThis.fetch = async () => { throw new TypeError('fetch failed') }
    dom.window.location.hash = hash
    const app = M.mountApp()
    try {
      const f = await until(() => frameOf(app.el).alert && frameOf(app.el), `${hash} to say it could not read`)
      assertFrameKept(app.el, hash)
      assert.match(f.alert.textContent, said, hash + ': the line names what happened')
      assert.equal(f.main.getAttribute('aria-label'), label, hash + ': the page is still named')
      const here = [...f.nav.querySelectorAll('a[aria-current="page"]')]
      assert.ok(here.length <= 1, hash + ': at most one section is current')
      for (const bad of [/\blie\b/i, /cheat/i, /violate/i, /ignore/i, /hallucinat/i, /trial/i, /\d+\s?%/, /\/Users\//]) assert.ok(!bad.test(f.alert.textContent), hash + ': ' + bad)
    } finally { app.unmount() }
  })
}

test('a section that is the screen you are on is marked current, the others are not', async () => {
  globalThis.fetch = async () => { throw new TypeError('fetch failed') }
  for (const [hash, name] of [['#/record', 'Record over time'], ['#/team', 'Team'], ['#/settings', 'Settings'], ['#/changes', 'Changes'], ['#/projects', 'Projects']]) {
    dom.window.location.hash = hash
    const app = M.mountApp()
    try {
      await until(() => frameOf(app.el).alert, hash)
      const current = [...app.el.querySelectorAll('nav[aria-label="Sections"] a[aria-current="page"]')]
      assert.equal(current.length, 1, hash)
      assert.match(current[0].textContent, new RegExp('^' + name), hash)
    } finally { app.unmount() }
  }
})

/* ---------------- 2. the Team case ---------------- */

const PROJECTS = { complete: true, projects: [{ id: 'p1', name: 'shop', root: '/work/shop', complete: true, threads: 2, sessions: 3, claims: 1, sources: ['claude_code'], last_activity: '2026-10-07T15:00:00Z', whose_move: { you: 1, agent: 0, done: 1 } }] }

test('Team: projects read, approvals not — the line is in the frame, with the sidebar to go elsewhere', async () => {
  globalThis.fetch = async (url) => {
    const u = String(url)
    if (u === '/api/hq/projects') return json(PROJECTS)
    if (u.startsWith('/api/changes')) return json({ error: 'store_busy' }, 503)
    if (u.startsWith('/api/storage')) return json({ bytes: 1000 })
    if (u.startsWith('/api/licence')) return json({ status: 'absent', plan: null })
    if (u.startsWith('/api/lanes')) return json({ window_minutes: 5, claude_code: { live: 0 }, codex: { live: 0 } })
    return json({}, 404)
  }
  dom.window.location.hash = '#/team'
  const app = M.mountApp()
  try {
    await until(() => frameOf(app.el).alert, 'Team to say it could not read the approvals')
    const f = assertFrameKept(app.el, 'Team')
    assert.match(f.alert.textContent, /could not read the approvals on this machine/)
    // Team is the section you are on, and the others are real links
    assert.equal(f.nav.querySelector('a[aria-current="page"]').textContent.replace(/g [a-z]$/, '').trim(), 'Team')
    assert.equal(f.nav.querySelector('a[href^="#/changes"]').textContent.replace(/g [a-z]$/, '').trim(), 'Changes')
    // and nothing of the old full page is left: no element is the whole page with only the line in it
    const roots = [...app.el.children]
    assert.equal(roots.length, 1)
    assert.equal(roots[0].getAttribute('data-frame'), 'kept', 'the page is the frame')
  } finally { app.unmount() }
})

test('a Change that cannot be read keeps the frame, with the Change screens’ crumb back to Changes', async () => {
  globalThis.fetch = async () => json({ error: 'no_such_change' }, 404)
  dom.window.location.hash = '#/changes/c9/approve'
  const app = M.mountApp()
  try {
    await until(() => frameOf(app.el).alert, 'Approve to say it could not read')
    const f = assertFrameKept(app.el, 'Approve')
    const crumb = f.header.querySelector('nav[aria-label="Where you are"]')
    assert.equal(crumb.querySelector('a').getAttribute('href'), '#/changes', 'the crumb goes back to Changes')
    assert.match(crumb.textContent, /Approve/)
  } finally { app.unmount() }
})

test('the project the person is in is kept by every section link', async () => {
  globalThis.fetch = async () => { throw new TypeError('fetch failed') }
  dom.window.location.hash = '#/team?project=p1'
  const app = M.mountApp()
  try {
    await until(() => frameOf(app.el).alert, 'Team')
    const hrefs = [...app.el.querySelectorAll('nav[aria-label="Sections"] a')].map((a) => a.getAttribute('href'))
    assert.ok(hrefs.every((h) => h.includes('project=p1')), hrefs.join(' '))
    assert.equal(app.el.querySelector('a[aria-label="Inbox, home"]').getAttribute('href'), '#/inbox?project=p1')
  } finally { app.unmount() }
})

/* ---------------- 3. a screen that throws ---------------- */

test('a screen that throws while drawing is said in the frame, not left as a blank page; another address clears it', () => {
  const realError = console.error
  const logged = []
  console.error = (...a) => logged.push(a.map(String).join(' '))
  const Boom = () => { throw new Error('unexpected shape') }
  const b = M.mountBoundary(Boom, 'a')
  try {
    const f = assertFrameKept(b.el, 'boundary')
    assert.equal(f.alert.textContent, M.COULD_NOT_DRAW)
    assert.ok(logged.some((l) => l.includes('could not be drawn')), 'and it is written to the console, not hidden')
    // the same address drawing again does not clear it; another address does
    b.again('a', false)
    assert.ok(b.el.querySelector('[role="alert"]'), 'the same address stays said')
    b.again('b', false)
    assert.ok(b.el.querySelector('#fine'), 'moving to another address clears it')
    assert.ok(!b.el.querySelector('[role="alert"]'))
  } finally { b.unmount(); console.error = realError }
})

/* ---------------- 4. no screen draws a ground of its own ---------------- */

function sources(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    if (statSync(p).isDirectory()) sources(p, out)
    else if (/\.(jsx?|mjs)$/.test(name) && !/\.test\.js$/.test(name) && name !== 'template.jsx') out.push(p)
  }
  return out
}

test('no screen draws its own full-page ground: a 100vh ground lives only in the frame', () => {
  const holders = []
  for (const p of sources(here)) {
    const rel = relative(here, p)
    if (/100vh/.test(readFileSync(p, 'utf8'))) holders.push(rel)
  }
  // frame-state.jsx is the frame; app.jsx's fallback is the blank a lazy chunk shows for an instant, not a read state;
  // artboards.jsx is the design gallery's index, which has no live reads
  assert.deepEqual(holders.sort(), ['app.jsx', 'artboards.jsx', 'frame-state.jsx'])
})

test('every screen’s reading and failure states are the shared frame state', () => {
  const states = [
    'screens/Settings/state.jsx', 'screens/Record-Over-Time/state.jsx', 'screens/Team-View/state.jsx', 'screens/Whole-Story/state.jsx',
    'screens/Change-Brief/index.jsx', 'screens/What-Changed/index.jsx', 'screens/Every-Statement/index.jsx', 'screens/Repair-Loop/live.js',
    'screens/Inbox/index.jsx', 'screens/Step-View/index.jsx', 'screens/Record-Statements/index.jsx',
  ]
  for (const rel of states) {
    const src = readFileSync(join(here, rel), 'utf8')
    assert.match(src, /frame-state\.jsx/, rel + ' says its reading and failures through the frame')
    assert.ok(!/role="alert"|role: 'alert'/.test(src) || /<FrameState|<Quiet/.test(src), rel + ' draws no alert of its own')
  }
})

test('the app gives every screen its place and wraps it in the boundary', () => {
  const src = readFileSync(join(here, 'app.jsx'), 'utf8')
  assert.match(src, /<FrameContext\.Provider value=\{frame\}>/, 'the place is given to the frame state')
  assert.match(src, /<ScreenBoundary resetKey=\{[^}]+\}>\s*<Screen /, 'a screen that throws is caught around the screen')
})

test('the keys the frame shows beside each section are the key table’s, not its own', async () => {
  globalThis.fetch = async () => { throw new TypeError('fetch failed') }
  dom.window.location.hash = '#/team'
  const app = M.mountApp()
  try {
    const f = await until(() => frameOf(app.el).alert && frameOf(app.el), 'Team')
    const shown = [...f.nav.querySelectorAll('a')].map((a) => [a.textContent.trim().replace(/g [a-z]$/, '').trim(), (a.textContent.trim().match(/g [a-z]$/) || [null])[0]])
    const table = M.KEY_TABLE.filter((e) => e.global && e.go)
    const want = { Inbox: null, Changes: 'g c', Projects: 'g p', 'Record over time': 'g r', Team: 'g t', Settings: 'g s' }
    assert.deepEqual(Object.fromEntries(shown), want)
    for (const [label, hint] of Object.entries(want)) {
      if (hint) assert.ok(table.some((e) => e.keys[0] === hint && e.label === label), label + ' ' + hint + ' is an entry of the key table')
    }
    const src = readFileSync(join(here, 'frame-state.jsx'), 'utf8')
    assert.ok(!/['"]g [a-z]['"]/.test(src), 'frame-state.jsx names no key sequence of its own')
  } finally { app.unmount() }
})
