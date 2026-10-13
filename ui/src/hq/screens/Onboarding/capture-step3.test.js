/* C30 R-ROUTES "Capture health" on First run (census §3.11 rows 3, 7, 8, 9,
 * 12, 13, 14, 17): step 3 draws what the capture route serves for the chosen
 * folder, in every state the design draws, and nothing the record does not
 * hold (G3: no "Installed", "Found", "Recording" without the fact behind it).
 *
 * The screen's logic over its template, rendered to markup with the live data
 * composed exactly as index.jsx composes it (capture-view.js `withCapture`).
 * Synthetic documents only (capture-fixture.js). */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import * as F from './capture-fixture.js'

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
export { P, R, withCapture, Logic }
export function make(props) {
  const inst = new (P.withFrame(Logic))(props)
  inst.props = props
  inst.renderVals() // the product opens on step 1
  return inst
}
export function html(inst, link) {
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: link || R.linker({ screen: 'start' }) }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-capture-step3-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

/* Two folders: shop (the default, chosen) and notes, never read. */
const PROJECTS = {
  schema_version: 1, complete: true,
  projects: [
    { complete: true, id: 'p1', name: 'shop', root: F.ROOT, sources: ['claude_code'], sessions: 0, last_activity: F.at(16, 5), threads: 0 },
    { complete: true, id: 'p2', name: 'notes', root: F.HOME + '/code/notes', sources: ['claude_code'], sessions: 0, last_activity: F.at(9, 0, 1), threads: 0 },
  ],
}
const ASK = { project: 'p1', state: 'read', thread: '0123456789abcdef', title: 'Fix the discount total', step: 3, text: 'the checkout total is wrong when a discount code is applied — fix it' }

/** First run's live data with one capture read for shop (or none), as index.jsx composes it. */
function liveData({ doc = null, error = null, reading = false, ask = null, known = {}, projects = PROJECTS } = {}) {
  const base = Object.assign(M.P.firstRunData({ projects, storage: { path: F.HOME + '/.thruline' }, now: F.NOW }), { loadError: '' })
  const capture = doc || error || reading ? { project: 'p1', doc, error, reading } : null
  return M.withCapture(base, { capture, ask, known, now: F.NOW })
}

/** The screen on one step, its values, and its markup. */
function screen(opts = {}, { step = 3, actions = {} } = {}) {
  const calls = []
  const acts = Object.assign({ load: () => {}, open: () => {}, lookAgain: () => calls.push('lookAgain'), place: (p) => calls.push(['place', p.folder, p.step]) }, actions)
  const inst = M.make({ data: liveData(opts), actions: acts })
  inst.state = Object.assign({}, inst.state, { step })
  const v = inst.renderVals()
  const html = M.html(inst)
  return { inst, v, html, calls, dom: new JSDOM(html).window.document }
}

/** What a person sees: text of every element not hidden by display:none. */
function visible(node) {
  if (node.nodeType === 3) return node.textContent
  if (node.nodeType !== 1) return ''
  if (node.style && node.style.display === 'none') return ''
  return [...node.childNodes].map(visible).join(' ')
}
const squash = (s) => s.replace(/\s+/g, ' ').trim()
const step3 = (doc) => doc.querySelector('section[aria-label="Step 3: Check it’s recording"]')
const seen3 = (s) => squash(visible(step3(s.dom)))
const pill = (s) => squash(visible(step3(s.dom).querySelector('[role="status"]')))
const title = (s) => squash(step3(s.dom).querySelector('h2').textContent)
const rows = (s) => [...step3(s.dom).querySelectorAll('[role="listitem"]')].map((li) => {
  const [label, detail, right] = li.children
  return { label: squash(label.textContent), detail: squash(detail.textContent), word: squash(visible(right)) }
})
const rowOf = (s, label) => rows(s).find((r) => r.label === label)
const shown = (el) => { for (let e = el; e && e.style; e = e.parentElement) if (e.style.display === 'none') return false; return !!el }
const anchor = (s, text) => [...step3(s.dom).querySelectorAll('a')].find((a) => squash(a.textContent).replace(/→/g, '').trim() === text)
const button = (s, text) => [...step3(s.dom).querySelectorAll('button')].find((b) => squash(b.textContent) === text)

/* ---------------- each state the design draws, from served facts ---------------- */

test('waiting: installed and able to run, nothing through the hooks yet', () => {
  const s = screen({ doc: F.waiting() })
  assert.equal(title(s), 'Installed. Start Claude Code in shop.')
  assert.equal(pill(s), '⋯ No step yet')
  const t = seen3(s)
  assert.match(t, /Start Claude Code in this folder \$ cd ~\/code\/shop \$ claude Copy/)
  assert.deepEqual(rowOf(s, 'Claude Code hooks'), { label: 'Claude Code hooks', detail: 'For every folder, from ~/.claude/settings.json · 8 events', word: '✓ Installed' })
  assert.equal(rowOf(s, 'First step received').word, '⋯ Waiting')
  assert.equal(rowOf(s, 'Git').word, '✓ Found')
  assert.ok(shown(button(s, 'Look again')), 'Look again is drawn')
  assert.ok(shown(anchor(s, 'Open the Inbox anyway')))
  assert.equal(anchor(s, 'Open the Inbox anyway').getAttribute('href'), '#/inbox?project=p1')
  assert.ok(!shown(anchor(s, 'Open the Inbox')), 'the orange Open the Inbox waits for a first step')
  assert.match(t, /The Inbox stays empty until the first step is recorded\./)
})

test('row 7: Copy cd && claude copies the chosen folder’s command, and copying runs nothing', () => {
  const s = screen({ doc: F.waiting() })
  const clips = []
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { clipboard: { writeText: async (t) => { clips.push(t) } } } })
  s.v.v.cp.start.act()
  assert.deepEqual(clips, ['cd ~/code/shop && claude'])
  assert.equal(s.inst.state.copied, 'start')
})

/* CTO 2026-10-12: ThruLine must not show a guessed path. A folder nothing has recorded in is served as its folder name,
 * flagged `path_known: false`; the screen shows that name and says the path is not known until the first recording,
 * and never writes a `cd` to a path it does not have. */
const UNKNOWN = {
  schema_version: 1, complete: true,
  projects: [{ complete: true, id: 'u1', name: '-Users-me-my-app', root: '-Users-me-my-app', path_known: false, sources: ['claude_code'], sessions: 0, last_activity: null, threads: 0,
    members: [{ ledger_project_id: '-Users-me-my-app', source: 'claude_code', root: '-Users-me-my-app', root_basis: 'folder_name' }] }],
}

test('a folder nothing has recorded in shows its folder name and says its path is not known until the first recording', () => {
  const s = screen({ projects: UNKNOWN }, { step: 1 })
  const row = s.v.v.folders[0]
  assert.equal(row.name, '-Users-me-my-app')
  assert.equal(row.path, 'path not known until the first recording')
  assert.match(squash(visible(s.dom.body)), /-Users-me-my-app path not known until the first recording/)
  assert.doesNotMatch(squash(visible(s.dom.body)), /\/Users\/me\/my\/app|~\/my\/app/, 'no decoded path is drawn')
})

test('for a folder with no stated path a copied command starts without a cd', () => {
  const s = screen({ projects: UNKNOWN }, { step: 2 })
  const clips = []
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { clipboard: { writeText: async (t) => { clips.push(t) } } } })
  s.v.v.cp.start.act()
  s.v.v.cp.init.act()
  assert.equal(clips[0], 'claude')
  assert.doesNotMatch(clips[1], /^cd /)
  assert.equal(s.v.v.cdLine, '# in your project folder')
  assert.doesNotMatch(squash(visible(s.dom.body)), /\$ cd /, 'no cd line is drawn')
})

test('recording: the first step arrived through the hooks; the first ask and its thread are drawn; Done links to that thread', () => {
  const s = screen({ doc: F.recording(), ask: ASK })
  assert.equal(title(s), 'shop is recording.')
  assert.equal(pill(s), 'Recording ✓')
  assert.ok(step3(s.dom).querySelector('[role="status"] span[aria-hidden="true"]').style.display !== 'none', 'the pulsing dot is drawn')
  const t = seen3(s)
  assert.match(t, /The first step arrived at 16:01\./)
  assert.match(t, /First steps recorded First step received 16:01 · Claude Code \(hook v0\.1\.0\)/)
  assert.match(t, /You asked · step 3 “the checkout total is wrong when a discount code is applied — fix it”/)
  assert.match(t, /ThruLine proposes a Change from it Fix the discount total It writes nothing down until you say so\./)
  const done = anchor(s, 'Review what counts as done')
  assert.ok(shown(done))
  assert.equal(done.getAttribute('href'), '#/changes/0123456789abcdef/done?project=p1')
  assert.deepEqual(rowOf(s, 'First step received'), { label: 'First step received', detail: 'Claude Code (hook v0.1.0) · 1,204 hook events recorded, the latest at 16:08', word: '✓ Received · 16:01' })
  assert.ok(shown(anchor(s, 'Open the Inbox')))
  assert.equal(anchor(s, 'Open the Inbox').getAttribute('href'), '#/inbox?project=p1')
  assert.ok(!shown(button(s, 'Look again')))
  assert.equal(s.v.v.st3.sub, 'Recording · since 16:01')
})

test('row 8: with no thread in the record there is no Done link at all — never the Inbox in its place', () => {
  const s = screen({ doc: F.recording(), ask: { project: 'p1', state: 'read', thread: null } })
  const t = seen3(s)
  assert.match(t, /Your first request shows here once it is recorded\./)
  assert.doesNotMatch(t, /Review what counts as done/)
  const done = [...step3(s.dom).querySelectorAll('a')].find((a) => a.textContent.includes('Review what counts as done'))
  assert.equal(done.getAttribute('href'), null, 'the anchor carries no address')
  // while the first ask is being read, or when it could not be read, neither card claims anything
  for (const ask of [{ project: 'p1', state: 'reading' }, { project: 'p1', state: 'failed' }, null]) {
    const u = seen3(screen({ doc: F.recording(), ask }))
    assert.doesNotMatch(u, /Your first request shows here|You asked|Review what counts as done/, JSON.stringify(ask))
  }
})

test('not installed: the record says no recorder; Go to step 2', () => {
  const s = screen({ doc: F.none() })
  assert.equal(title(s), 'Nothing is installed in shop yet')
  assert.equal(pill(s), '◌ Not installed')
  assert.match(seen3(s), /Nothing has been installed, so there is nothing to check yet\./)
  assert.equal(rowOf(s, 'Claude Code hooks').word, '◌ Not installed')
  assert.equal(rowOf(s, 'First step received').word, '– After install')
  assert.ok(shown(button(s, 'Go to step 2')))
  button(s, 'Go to step 2')
  s.v.v.f3.act()
  assert.equal(s.inst.state.step, 2)
})

test('the hooks can’t run: every recorder names a missing program; the command shown and copied is `thruline doctor`, which reads every settings level the route reads; Look again re-reads', () => {
  const s = screen({ doc: F.problem() })
  assert.equal(title(s), 'The hooks can’t reach ThruLine.')
  assert.equal(pill(s), '◌ Not recording')
  const t = seen3(s)
  assert.match(t, /They were installed, but they point at a ThruLine program that isn’t there any more, so nothing is being recorded\./)
  assert.match(t, /Find what to fix \$ thruline doctor Copy/)
  assert.doesNotMatch(t, /sensor doctor/, '`thruline sensor doctor` reads one settings file, so it can contradict this screen')
  assert.deepEqual(rowOf(s, 'Claude Code hooks'), { label: 'Claude Code hooks', detail: 'Installed, but they point at a ThruLine program that isn’t there any more', word: '◌ Can’t run' })
  assert.equal(rowOf(s, 'First step received').word, '◌ Nothing yet')
  const clips = []
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { clipboard: { writeText: async (x) => { clips.push(x) } } } })
  s.v.v.cp.doctor.act()
  assert.deepEqual(clips, ['thruline doctor'])
  assert.ok(shown(button(s, 'Look again')))
  s.v.v.f3.act()
  assert.deepEqual(s.calls.filter((c) => c === 'lookAgain'), ['lookAgain'], 'Look again asks for a fresh read, and nothing else')
})

test('one other ThruLine hook can’t run while the recorder can: it never says nothing is being recorded', () => {
  const s = screen({ doc: F.partlyBroken() })
  assert.equal(pill(s), '◌ Can’t run')
  const t = seen3(s)
  assert.match(t, /1 of 9 point at a ThruLine program that isn’t there any more/)
  assert.doesNotMatch(t, /nothing is being recorded|Not recording/)
})

test('can’t tell: a settings file could not be read — said with its reason, never "Not installed" or "Installed"', () => {
  const s = screen({ doc: F.unreadable() })
  assert.equal(pill(s), '◌ Can’t tell')
  const t = seen3(s)
  assert.match(t, /could not be read \(user: Permission denied \(os error 13\)\)/)
  assert.match(t, /Find what to fix/)
  for (const r of rows(s).slice(0, 2)) assert.doesNotMatch(r.word, /Installed|Not installed|Waiting|Received/, r.label)
  const l = screen({ doc: F.ledgerUnreadable() })
  assert.equal(pill(l), '◌ Can’t tell')
  assert.equal(rowOf(l, 'First step received').word, '◌ Can’t tell')
  assert.match(rowOf(l, 'First step received').detail, /record could not be read/)
  assert.doesNotMatch(seen3(l), /Find what to fix/)
})

test('reading and a failed read: said as such, and neither becomes installed nor not installed', () => {
  const r = screen({ reading: true })
  assert.equal(title(r), 'Checking whether shop is set up to record')
  assert.equal(pill(r), '⋯ Checking')
  const f = screen({ error: 'ThruLine’s local server did not answer. Is thruline ui still running?' })
  assert.equal(title(f), 'The check of shop didn’t answer')
  assert.match(seen3(f), /ThruLine’s local server did not answer\. Is thruline ui still running\?/)
  assert.ok(shown(button(f, 'Look again')))
  for (const s of [r, f]) {
    const t = seen3(s)
    assert.doesNotMatch(t, /Nothing has been installed|Not installed|✓ Installed|Recording|Received/)
  }
})

test('steps read from the recordings are said apart from steps through the hooks', () => {
  const s = screen({ doc: F.loadedNotHooked() })
  const t = seen3(s)
  assert.match(t, /What is recorded here so far was read from the agents’ own recordings\./)
  assert.doesNotMatch(t, /Nothing recorded in this project yet/)
  assert.match(t, /The Inbox already holds what was read from the recordings\./)
})

/* ---------------- row 3: Git, tri-state ---------------- */

test('row 3: a folder’s Git is the route’s answer once read, and "Git not checked yet" until then', () => {
  const chip = (s, name) => {
    const b = [...s.dom.querySelectorAll('button[role="radio"]')].find((x) => x.textContent.includes(name))
    return squash(b.textContent)
  }
  const before = screen({}, { step: 1 })
  assert.match(chip(before, 'shop'), /Git not checked yet/)
  const read = screen({ doc: F.waiting() }, { step: 1 })
  assert.match(chip(read, 'shop'), /Git repository/)
  assert.match(chip(read, 'notes'), /Git not checked yet/, 'a folder never read is never claimed')
  const no = screen({ doc: F.withGit(F.waiting(), { repository: false, toplevel: null, reason: null }) }, { step: 1 })
  assert.match(chip(no, 'shop'), /No Git repository/)
  const known = screen({ doc: F.waiting(), known: { p2: { repository: false, toplevel: null, reason: null } } }, { step: 1 })
  assert.match(chip(known, 'notes'), /No Git repository/, 'a folder read earlier keeps its answer')
  const silent = screen({ doc: F.withGit(F.waiting(), { repository: null, toplevel: null, reason: 'git is not installed on this machine' }) })
  assert.match(chip(silent, 'shop'), /Git didn’t answer/)
  assert.deepEqual(rowOf(silent, 'Git'), { label: 'Git', detail: 'Git is not installed on this machine', word: '◌ No answer' })
  assert.equal(rowOf(screen({ doc: F.withGit(F.waiting(), { repository: false, toplevel: null, reason: null }) }), 'Git').word, '◌ Not found')
})

/* ---------------- G3 and the copy rules, in every live state ---------------- */

const STATES = {
  unread: {}, reading: { reading: true }, failed: { error: 'ThruLine could not check this folder (it answered 500).' },
  none: { doc: F.none() }, waiting: { doc: F.waiting() }, loaded: { doc: F.loadedNotHooked() },
  recording: { doc: F.recording(), ask: ASK }, recordingNoThread: { doc: F.recording(), ask: { project: 'p1', state: 'read', thread: null } },
  problem: { doc: F.problem() }, partly: { doc: F.partlyBroken() }, unknown: { doc: F.unreadable() }, ledger: { doc: F.ledgerUnreadable() },
}

test('G3: MCP, speaker hooks and run capture are never "Installed" — the route does not read them', () => {
  for (const [name, o] of Object.entries(STATES)) {
    const s = screen(o)
    for (const label of ['MCP server', 'Speaker hooks', 'Run capture']) {
      assert.doesNotMatch(rowOf(s, label).word, /Installed|One line to add/, name + ' · ' + label)
    }
    const step2 = squash(visible(s.dom.querySelector('section[aria-label="Step 2: What ThruLine will do"]') || s.dom.createElement('i')))
    assert.doesNotMatch(step2, /Installed:/, name + ': step 2 never says what was installed')
  }
})

test('G3: "Installed", "Found", "Received" and "Recording" appear only with the fact behind them', () => {
  for (const [name, o] of Object.entries(STATES)) {
    const s = screen(o)
    const facts = o.doc ? o.doc : null
    const hooks = rowOf(s, 'Claude Code hooks').word
    if (/✓ Installed/.test(hooks)) assert.equal(facts && facts.claude_code_hooks.installed, true, name)
    if (/Found/.test(rowOf(s, 'Git').word)) assert.equal(facts && facts.git.repository, true, name)
    if (/Received/.test(rowOf(s, 'First step received').word)) assert.ok(facts && facts.record.through_hooks.deliveries > 0, name)
    if (/Recording/.test(pill(s))) assert.ok(facts && facts.record.through_hooks.deliveries > 0 && facts.claude_code_hooks.installed === true && facts.claude_code_hooks.alive !== false, name)
  }
})

test('copy: plain words on step 3 in every state — no blame words, no "trial", no "%", no home folder, no "undefined"', () => {
  const BANNED = /\b(lie|lied|lying|cheat|violat\w*|ignor\w*|hallucinat\w*|trial)\b|%|Not available in this version|undefined|NaN|null|\+00:00|\bIST\b/i
  for (const [name, o] of Object.entries(STATES)) {
    const s = screen(o)
    const t = squash(step3(s.dom).textContent) + ' ' + squash(s.dom.querySelector('aside[aria-label="Context for this step"]').textContent)
    assert.doesNotMatch(t, BANNED, name)
    assert.ok(!s.html.includes(F.HOME), name + ': the home folder is drawn')
  }
})

/* ---------------- the place the screen reports, and the rail ---------------- */

test('the screen tells index.jsx which folder and step are in front, so it reads that folder only', () => {
  const s = screen({ doc: F.waiting() }, { step: 1 })
  const notes = s.v.v.folders.find((f) => f.id === 'p2')
  notes.ui.pick()
  assert.deepEqual(s.calls.filter(Array.isArray).at(-1), ['place', 'p2', 1])
  s.v.v.toStep3()
  assert.deepEqual(s.calls.filter(Array.isArray).at(-1), ['place', 'p2', 3])
})

test('the rail: step 2 is done only when the record says the hooks are installed, and says only "hooks"', () => {
  assert.equal(screen({ doc: F.waiting() }).v.v.st2.sub, 'Installed · hooks')
  assert.equal(screen({ doc: F.none() }).v.v.st2.sub, 'Read it, choose, then install')
  assert.equal(screen({ doc: F.unreadable() }).v.v.st2.sub, 'Read it, choose, then install')
  assert.equal(screen({ doc: F.waiting() }).v.v.st3.sub, 'Waiting for the first step')
  assert.equal(screen({ reading: true }).v.v.st3.sub, 'Checking…')
})

test('the step 2 preview never says "nothing is installed" when that was not read', () => {
  for (const [name, o] of Object.entries(STATES)) {
    const s = screen(o, { step: 2 })
    const foot = s.v.v.pv.foot
    if (/nothing is installed/.test(foot)) assert.equal(o.doc && o.doc.claude_code_hooks.installed, false, name)
  }
})

test('row 7: a folder whose path a shell would split is copied quoted, so the pasted command still works', () => {
  const projects = { schema_version: 1, complete: true, projects: [{ complete: true, id: 'p3', name: 'my shop', root: F.HOME + "/code/my shop's", sources: ['claude_code'], sessions: 0, last_activity: F.at(16, 5), threads: 0 }] }
  const base = Object.assign(M.P.firstRunData({ projects, storage: null, now: F.NOW }), { loadError: '' })
  const data = M.withCapture(base, { capture: { project: 'p3', doc: F.waiting('p3'), error: null, reading: false }, ask: null, known: {}, now: F.NOW })
  const inst = M.make({ data, actions: {} })
  inst.state = Object.assign({}, inst.state, { step: 3 })
  const clips = []
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { clipboard: { writeText: async (t) => { clips.push(t) } } } })
  inst.renderVals().v.cp.start.act()
  assert.deepEqual(clips, ["cd ~/'code/my shop'\\''s' && claude"])
})
