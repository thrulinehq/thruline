/* C30 R-ROUTES "Install" on First run (census §3.11 rows 5, 6, 10, 11): the
 * boxes drive the body, "Install for this project" sends exactly them against
 * the plan on screen (the click is the consent), the answer is said per part,
 * step 3's MCP, speaker and run-capture rows say what the plan read, "Turn on"
 * and the shim line follow the real install state, and no state of First run
 * says "Not available in this version".
 *
 * The screen's logic over its template, rendered to markup with the live data
 * composed exactly as index.jsx composes it (withCapture, then withInstall).
 * Synthetic documents only (capture-fixture.js, install-fixture.js). */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import * as F from './capture-fixture.js'
import * as P from './install-fixture.js'
import * as I from './install.js'
import { installView, installRows, INIT_CMD, UNDO_CMD } from './install-view.js'

const here = dirname(fileURLToPath(import.meta.url))

const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import * as P from '../../data/project.js'
import * as R from '../../routes.js'
import Template from './template.jsx'
import Logic from './logic.js'
import { withCapture } from './capture-view.js'
import { withInstall } from './install.js'
export { P, R, withCapture, withInstall, Logic }
export function make(props) {
  const inst = new (P.withFrame(Logic))(props)
  inst.props = props
  inst.renderVals() // the product opens on step 1
  return inst
}
export function html(inst) {
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: R.linker({ screen: 'start' }) }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-install-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const PROJECTS = {
  schema_version: 1, complete: true,
  projects: [
    { complete: true, id: 'p1', name: 'shop', root: F.ROOT, sources: ['claude_code'], sessions: 0, last_activity: F.at(16, 5), threads: 0 },
    { complete: true, id: 'p2', name: 'notes', root: F.HOME + '/code/notes', sources: ['claude_code'], sessions: 0, last_activity: F.at(9, 0, 1), threads: 0 },
  ],
}

/** First run's live data as index.jsx composes it: a capture read, then the install plan. */
function liveData({ doc = F.none(), install = null } = {}) {
  const base = Object.assign(M.P.firstRunData({ projects: PROJECTS, storage: { path: F.HOME + '/.thruline' }, now: F.NOW }), { loadError: '' })
  const withRead = M.withCapture(base, { capture: { project: 'p1', doc, error: null, reading: false }, ask: null, known: {}, now: F.NOW })
  return install ? M.withInstall(withRead, Object.assign({ project: 'p1', doc: null, error: null, reading: false, busy: false, result: null }, install)) : withRead
}

const squash = (s) => s.replace(/\s+/g, ' ').trim()
function visible(node) {
  if (node.nodeType === 3) return node.textContent
  if (node.nodeType !== 1) return ''
  if (node.style && node.style.display === 'none') return ''
  return [...node.childNodes].map(visible).join(' ')
}
const shown = (el) => { for (let e = el; e && e.style; e = e.parentElement) if (e.style.display === 'none') return false; return !!el }

/** The screen on one step, with its actions recorded. */
function screen(opts = {}, { step = 2, state = {}, install } = {}) {
  const calls = []
  const acts = {
    load: () => {}, open: () => {}, lookAgain: () => calls.push('lookAgain'), place: () => {},
    install: install || ((choices) => { calls.push(['install', choices]); return Promise.resolve({ ok: true, changed: true, parts: { hooks: { outcome: 'wrote' }, speaker: { outcome: 'not_chosen' }, mcp: { outcome: 'wrote' }, run: { outcome: 'not_chosen' } } }) }),
  }
  const inst = M.make({ data: liveData(opts), actions: acts })
  inst.state = Object.assign({}, inst.state, { step }, state)
  const v = inst.renderVals()
  const html = M.html(inst)
  return { inst, v, html, calls, dom: new JSDOM(html).window.document }
}
const step2 = (s) => s.dom.querySelector('section[aria-label="Step 2: What ThruLine will do"]') || s.dom.querySelector('section[aria-label^="Step 2"]')
const step3 = (s) => s.dom.querySelector('section[aria-label="Step 3: Check it’s recording"]')
const installButton = (s) => [...s.dom.querySelectorAll('button.tl-btn')].find((b) => /Install|Installed|Installing/.test(b.textContent))
const rowOf = (s, label) => [...step3(s).querySelectorAll('[role="listitem"]')].map((li) => {
  const [l, d, r] = li.children
  return { label: squash(l.textContent), detail: squash(d.textContent), word: squash(visible(r)) }
}).find((r) => r.label === label)
const flush = () => new Promise((r) => setImmediate(r))

/* ---------------- the client: exactly the boxes, the token, the plan ---------------- */

test('postInstall sends exactly the ticked boxes, the plan, the run token and nothing else', async () => {
  const seen = []
  const f = async (url, init = {}) => {
    seen.push({ url, init })
    if (url === I.RUN_TOKEN) return { ok: true, status: 200, json: async () => ({ token: 'T'.repeat(64) }) }
    return { ok: true, status: 200, json: async () => P.answer({ hooks: 'wrote' }, P.plan()) }
  }
  const res = await I.postInstall('p 1', { mcp: true, speaker: false, runcap: true }, P.PLAN_ID, f)
  assert.equal(res.ok, true)
  assert.equal(seen.length, 2)
  assert.equal(seen[0].url, '/api/run/token')
  assert.equal(seen[0].init.method, undefined, 'the token is read with a GET')
  assert.equal(seen[1].url, '/api/hq/projects/p%201/install')
  assert.equal(seen[1].init.method, 'POST')
  assert.equal(seen[1].init.headers['X-ThruLine-Run-Token'], 'T'.repeat(64))
  assert.deepEqual(JSON.parse(seen[1].init.body), { hooks: true, mcp: true, speaker: false, run_capture: true, plan: P.PLAN_ID })
  // the other way round: each box is read, none is assumed
  await I.postInstall('p1', { mcp: false, speaker: true, runcap: false }, P.NEXT_PLAN_ID, f)
  assert.deepEqual(JSON.parse(seen[3].init.body), { hooks: true, mcp: false, speaker: true, run_capture: false, plan: P.NEXT_PLAN_ID })
  await I.postInstall('p1', {}, P.PLAN_ID, f)
  assert.deepEqual(JSON.parse(seen[5].init.body), { hooks: true, mcp: false, speaker: false, run_capture: false, plan: P.PLAN_ID })
  // no token, no POST
  const none = []
  const r2 = await I.postInstall('p1', {}, P.PLAN_ID, async (url) => { none.push(url); return { ok: false, status: 503, json: async () => ({ error: 'run_token_unavailable' }) } })
  assert.equal(r2.ok, false)
  assert.deepEqual(none, ['/api/run/token'])
})

test('readInstall reads the plan with a GET and refuses a shape it cannot read, in a sentence', async () => {
  const urls = []
  const ok = await I.readInstall('p1', { fetchImpl: async (url, init) => { urls.push([url, init.method]); return { ok: true, status: 200, text: async () => JSON.stringify(P.plan()) } } })
  assert.equal(ok.plan, P.PLAN_ID)
  assert.deepEqual(urls, [['/api/hq/projects/p1/install', 'GET']])
  for (const [answer, words] of [
    [{ ok: false, status: 404, text: async () => '{}' }, /no longer knows this folder/],
    [{ ok: true, status: 200, text: async () => '{"schema_version":2}' }, /cannot read/],
    [{ ok: true, status: 200, text: async () => JSON.stringify(P.plan({ id: 'p2' })) }, /cannot read/],
  ]) {
    const err = await I.readInstall('p1', { fetchImpl: async () => answer }).then(() => null, (e) => e)
    assert.match(err.message, words)
    assert.doesNotMatch(err.message, /installed/i)
  }
})

test('a refused click is said in words, and a plan that changed gives the fresh plan', () => {
  const r = I.resultFacts({ ok: false, status: 409, doc: { error: 'plan_changed', plan: P.plan({ plan: P.NEXT_PLAN_ID }) } })
  assert.equal(r.ok, false)
  assert.match(r.plain, /changed since this page read it, so nothing was installed/)
  const t = I.resultFacts({ ok: false, status: 403, doc: { error: 'run_token_required' } })
  assert.match(t.plain, /only installs from its own page/)
  const ok = I.resultFacts({ ok: true, status: 200, doc: P.answer({ hooks: 'already', mcp: 'refused' }, P.plan()) })
  assert.equal(ok.parts.mcp.outcome, 'refused')
  assert.match(ok.parts.mcp.refusal.plain, /link to somewhere else/)
})

/* ---------------- step 2: the boxes, the button, the files ---------------- */

test('row 6: Install is on, names each file it writes, and sends exactly the ticked boxes; a settled answer goes to step 3', async () => {
  const s = screen({ install: { doc: P.plan() } })
  const b = installButton(s)
  assert.equal(squash(b.textContent), 'Install for this project')
  assert.equal(b.hasAttribute('disabled'), false)
  assert.equal(b.getAttribute('title'), 'Writes .claude/settings.local.json and .mcp.json in this folder. Nothing else.')
  const t = squash(visible(s.dom.body))
  assert.match(t, /Adds Claude Code hooks and MCP server · ~\/code\/shop only/)
  assert.match(t, /Left out: speaker hooks and run capture · add later in Settings/)
  s.v.v.sum.install()
  await flush(); await flush()
  assert.deepEqual(s.calls, [['install', { mcp: true, speaker: false, runcap: false }]])
  assert.equal(s.inst.state.step, 3, 'a settled install goes on to step 3')
})

test('row 5: each box is its own yes — the body follows the boxes, and run capture is said to live in ThruLine’s own folder', async () => {
  const s = screen({ install: { doc: P.plan() } }, { state: { mcp: false, speaker: true, runcap: true } })
  const t = squash(visible(s.dom.body))
  assert.match(t, /Adds Claude Code hooks and speaker hooks · ~\/code\/shop only, and run capture in ThruLine’s own folder/)
  assert.equal(installButton(s).getAttribute('title'), 'Writes .claude/settings.local.json in this folder and the shim folder in ThruLine’s home. Nothing else.')
  s.v.v.sum.install()
  await flush()
  assert.deepEqual(s.calls, [['install', { mcp: false, speaker: true, runcap: true }]])
  // toggling a box changes the next body
  s.v.v.c.mcp.toggle()
  const v = s.inst.renderVals()
  v.v.sum.install()
  await flush()
  assert.deepEqual(s.calls[1], ['install', { mcp: true, speaker: true, runcap: true }])
})

test('Install is off, and says why, while the plan is read or when it could not be read — never "Not available in this version"', () => {
  const reading = screen({ install: { reading: true } })
  assert.equal(installButton(reading).hasAttribute('disabled'), true)
  assert.match(installButton(reading).getAttribute('title'), /Reading what is already installed for this folder/)
  const failed = screen({ install: { error: 'ThruLine could not open its record, so nothing could be checked.' } })
  assert.equal(installButton(failed).hasAttribute('disabled'), true)
  assert.match(squash(visible(failed.dom.body)), /ThruLine could not open its record, so nothing could be checked\./)
  for (const s of [reading, failed]) assert.doesNotMatch(s.html, /Not available in this version/)
  for (const s of [reading, failed]) { s.v.v.sum.install(); assert.deepEqual(s.calls, []) }
})

test('a part the plan refuses is said before the click, and the rest can still be installed', () => {
  const s = screen({ install: { doc: P.plan({ states: { mcp: 'refused' } }) } })
  const t = squash(visible(s.dom.body))
  assert.match(t, /Adds Claude Code hooks · ~\/code\/shop only/)
  assert.match(t, /Can’t add MCP server: A server named thruline is already in \.mcp\.json, and ThruLine did not write it, so it was left alone\./)
  assert.equal(installButton(s).hasAttribute('disabled'), false)
})

test('everything chosen already installed: the button says so and goes to step 3 without writing', async () => {
  const doc = P.plan({ states: { hooks: 'installed', mcp: 'installed' } })
  const s = screen({ doc: F.waiting(), install: { doc } })
  const b = installButton(s)
  assert.equal(squash(b.textContent), 'Installed · go to step 3')
  s.v.v.sum.install()
  await flush()
  assert.deepEqual(s.calls, [], 'nothing is posted')
  assert.equal(s.inst.state.step, 3)
})

test('hooks already on for every folder: said as such, never as "this folder only"', () => {
  const doc = P.plan({ states: { hooks: 'installed', mcp: 'installed' }, where: { hooks: ['user'], mcp: ['user'] } })
  const s = screen({ doc: F.waiting(), install: { doc } })
  const t = squash(visible(s.dom.body))
  assert.match(t, /Installed: Claude Code hooks and MCP server/)
  assert.doesNotMatch(t, /Installed: Claude Code hooks and MCP server · ~\/code\/shop only/)
  assert.match(t, /Claude Code hooks are on for every folder, from your own Claude Code settings/)
})

test('a refused click stays on step 2 and says why', async () => {
  const refused = { ok: false, error: 'plan_changed', plain: I.refusedWords('plan_changed', 409) }
  const s = screen({ install: { doc: P.plan(), result: refused } }, { install: () => Promise.resolve(refused) })
  assert.match(squash(visible(s.dom.body)), /changed since this page read it, so nothing was installed/)
  s.v.v.sum.install()
  await flush(); await flush()
  assert.equal(s.inst.state.step, 2)
})

test('while it writes: "Installing…", off', () => {
  const s = screen({ install: { doc: P.plan(), busy: true } })
  const b = installButton(s)
  assert.equal(squash(b.textContent), 'Installing…')
  assert.equal(b.hasAttribute('disabled'), true)
})

test('the terminal panel names the command that writes the same two files', () => {
  const s = screen({ install: { doc: P.plan() } })
  const panel = s.dom.querySelector('section[aria-label="Same install from a terminal"]')
  const t = squash(visible(panel))
  assert.match(t, /\$ cd ~\/code\/shop \$ thruline init --settings \.claude\/settings\.local\.json --claude-json \.mcp\.json/)
  assert.match(t, /thruline sensor plan --settings \.claude\/settings\.local\.json/)
  assert.equal(s.v.v.cp.init.aria, 'Copy: cd ~/code/shop && ' + INIT_CMD)
})

/* ---------------- step 3: what the plan read ---------------- */

test('step 3: MCP, speaker and run capture say what the plan read in the files', () => {
  const doc = P.plan({ states: { hooks: 'installed', mcp: 'installed', speaker: 'installed', run: 'installed' } })
  const s = screen({ doc: F.waiting(), install: { doc } }, { step: 3, state: { mcp: true, speaker: true, runcap: true } })
  assert.equal(rowOf(s, 'MCP server').word, '✓ Installed')
  assert.match(rowOf(s, 'MCP server').detail, /for ~\/code\/shop only, from \.mcp\.json/)
  assert.equal(rowOf(s, 'Speaker hooks').word, '✓ Installed')
  assert.equal(rowOf(s, 'Run capture').word, '⋯ One line to add')
  assert.equal(s.v.v.st2.sub, 'Installed · hooks + 3 more')
})

test('rows 10 and 11: the shim installed and not on the search path shows the one line, and Copy copies exactly it', () => {
  const doc = P.plan({ states: { hooks: 'installed', run: 'installed' }, wrappers: ['pytest', 'make'] })
  const clips = []
  const before = Object.getOwnPropertyDescriptor(globalThis, 'navigator')
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { clipboard: { writeText: async (x) => { clips.push(x) } } } })
  try {
    const s = screen({ doc: F.waiting(), install: { doc } }, { step: 3, state: { runcap: true } })
    const sec = s.dom.querySelector('section[aria-label="Turn on run capture"]')
    assert.ok(shown(sec), 'the run-capture steps are drawn live once the shim is in')
    const t = squash(visible(sec))
    assert.match(t, /\$ export PATH="\$HOME\/\.thruline\/shim:\$PATH"/)
    assert.match(t, /It wraps pytest and make · in ThruLine’s own folder/)
    assert.match(t, /it never edits your shell profile/)
    s.v.v.cp.shim.act()
    assert.deepEqual(clips, [P.PATH_LINE])
    // on the search path already: nothing left to add, so nothing is drawn
    const on = screen({ doc: F.waiting(), install: { doc: P.plan({ states: { hooks: 'installed', run: 'installed' }, onPath: true }) } }, { step: 3, state: { runcap: true } })
    assert.ok(!shown(on.dom.querySelector('section[aria-label="Turn on run capture"]')))
    assert.equal(rowOf(on, 'Run capture').word, '✓ Installed')
  } finally {
    if (before) Object.defineProperty(globalThis, 'navigator', before)
  }
})

test('row 10: Turn on installs run capture alone where the hooks are in — and only ticks the box where they are not', async () => {
  const doc = P.plan({ states: { hooks: 'installed' } })
  const s = screen({ doc: F.waiting(), install: { doc } }, { step: 3, state: { runcap: false } })
  const row = s.v.v.health.find((r) => r.label === 'Run capture')
  assert.equal(row.actLabel, 'Turn on')
  row.act()
  await flush()
  assert.deepEqual(s.calls, [['install', { mcp: false, speaker: false, runcap: true }]])
  assert.equal(s.inst.state.step, 3, 'Turn on stays where it is')

  const none = screen({ doc: F.none(), install: { doc: P.plan() } }, { step: 3, state: { runcap: false } })
  const row2 = none.v.v.health.find((r) => r.label === 'Run capture')
  assert.equal(row2.actLabel, 'Turn on')
  row2.act()
  await flush()
  assert.deepEqual(none.calls, [], 'with no hooks in, Turn on is a choice for step 2’s Install, not a write')
  assert.equal(none.inst.state.runcap, true)
})

test('"Changed your mind?" copies the command that removes exactly what Install wrote', () => {
  const doc = P.plan({ states: { hooks: 'installed', mcp: 'installed', run: 'installed' } })
  const s = screen({ doc: F.waiting(), install: { doc } }, { step: 3 })
  const sec = s.dom.querySelector('section[aria-label="Changed your mind"]')
  const t = squash(visible(sec))
  assert.match(t, /\$ cd ~\/code\/shop && thruline init --undo --settings \.claude\/settings\.local\.json --claude-json \.mcp\.json && thruline shim uninstall/)
  assert.match(t, /removes what ThruLine added to this folder’s Claude Code settings/)
  // hooks for every folder: the command and the words say so
  const user = screen({ doc: F.waiting(), install: { doc: P.plan({ states: { hooks: 'installed' }, where: { hooks: ['user'] } }) } }, { step: 3 })
  const ut = squash(visible(user.dom.querySelector('section[aria-label="Changed your mind"]')))
  assert.match(ut, /\$ thruline init --undo/)
  assert.match(ut, /for every folder, so this removes them everywhere/)
  // not read: the project's own undo, never the one-file `sensor uninstall`
  const unread = screen({ doc: F.none() }, { step: 3 })
  assert.match(squash(visible(unread.dom.querySelector('section[aria-label="Changed your mind"]'))), new RegExp('cd ~/code/shop && ' + UNDO_CMD.replace(/[.]/g, '\\.')))
})

/* ---------------- the census: no state of First run says it is not available ---------------- */

test('no state of live First run says "Not available in this version", or a banned word', () => {
  const BANNED = /\b(lie|lied|lying|cheat|violat\w*|ignor\w*|hallucinat\w*|trial)\b|%|Not available in this version|undefined|NaN|\+00:00|\bIST\b/i
  const plans = [
    null, { reading: true }, { error: 'ThruLine could not open its record, so nothing could be checked.' },
    { doc: P.plan() }, { doc: P.plan(), busy: true },
    { doc: P.plan({ states: { hooks: 'refused', speaker: 'refused', mcp: 'refused', run: 'refused' } }) },
    { doc: P.plan({ states: { hooks: 'installed', mcp: 'installed', speaker: 'installed', run: 'installed' } }) },
    { doc: P.plan(), result: { ok: false, error: 'unreachable', plain: I.refusedWords('unreachable', 0) } },
  ]
  for (const install of plans) {
    for (const doc of [F.none(), F.waiting(), F.recording(), F.problem()]) {
      for (const step of [1, 2, 3]) {
        for (const state of [{}, { mcp: false, speaker: true, runcap: true }]) {
          const s = screen({ doc, install }, { step, state })
          const t = squash(visible(s.dom.body))
          assert.doesNotMatch(t, BANNED, JSON.stringify({ install: install && Object.keys(install), step, state }))
          assert.doesNotMatch(s.html, /Not available in this version/)
          assert.doesNotMatch(s.html, /\/Users\/someone/, 'no home-folder path')
        }
      }
    }
  }
})

test('installView and installRows are pure: the same plan and boxes give the same words', () => {
  const proj = { id: 'p1', path: '~/code/shop', install: { state: 'read', facts: I.installFacts(P.plan()) } }
  const a = installView(proj, { mcp: true }, 'cd ~/code/shop')
  const b = installView(proj, { mcp: true }, 'cd ~/code/shop')
  assert.deepEqual(a, b)
  assert.deepEqual(installRows(proj, { mcp: true }), installRows(proj, { mcp: true }))
  assert.equal(installRows({ id: 'p1' }, {}), null, 'no plan read: no rows from it')
})
