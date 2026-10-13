/* C27 (speed, a release blocker): the Inbox never waits for a load.
 *  (a) a folder with records opens its Inbox at once; a load starts and the
 *      Inbox opens without waiting for it;
 *  (b) while GET /api/hq/load says running, every status bar carries ONE
 *      progress line (data/project.js `loadLine`);
 *  (c) the open project's view re-reads every ~3 s while its load runs;
 *  (4) the default address opens the most recently active project WITH
 *      records; with none, First run.
 * Synthetic fixtures only. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const read = (p) => readFileSync(join(here, p), 'utf8')

const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { withFrame, inboxData, firstRunData, loadLine, viewRereadEvery, currentProjectId } from '../data/project.js'
import { createLoadWatch } from '../data/load-watch.js'
import { changeSnapshot } from '../data/change.js'
import { threadFixture } from './What-Changed/thread-fixture.js'
import { everyStatementFromChange } from './Every-Statement/from-change.js'
import IT from './Inbox/template.jsx'
import IL from './Inbox/logic.js'
import OT from './Onboarding/template.jsx'
import OL from './Onboarding/logic.js'
import ET from './Every-Statement/template.jsx'
import EL from './Every-Statement/logic.js'
const S = { Inbox: [IT, IL], Onboarding: [OT, OL], 'Every-Statement': [ET, EL] }
export { inboxData, firstRunData, loadLine, viewRereadEvery, currentProjectId, createLoadWatch, changeSnapshot, threadFixture, everyStatementFromChange }
export function render(name, props) {
  const [Template, Logic] = S[name]
  const inst = new (withFrame(Logic))(props || {})
  inst.props = props || {}
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}
export function vals(name, props) {
  const [, Logic] = S[name]
  const inst = new (withFrame(Logic))(props || {})
  inst.props = props || {}
  return inst.renderVals()
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-load-')), 'screens.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const words = (html) => unescape(html.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ')
const NOW = new Date('2026-10-07T10:30:00.000Z')
const RUNNING = { running: true, project: 'shop', files_total: 569, files_done: 12, events_added: 1204, finished_at: null }

test('(b) the progress line says what the load states, and nothing when no load runs', () => {
  assert.equal(M.loadLine(RUNNING), 'Reading new recordings · 12 of 569 · 1,204 rows added')
  assert.equal(M.loadLine({ running: true }), 'Reading new recordings', 'a part it does not state is left out, never 0')
  assert.equal(M.loadLine({ running: true, files_done: 0, files_total: 3, events_added: 0 }), 'Reading new recordings · 0 of 3 · 0 rows added', 'a served 0 is a 0')
  assert.equal(M.loadLine({ running: false, files_done: 3, files_total: 3 }), null)
  assert.equal(M.loadLine(null), null)
})

test('(b) census: every screen with a status bar draws the one progress line', () => {
  const dirs = readdirSync(here, { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => d.name)
  const drawn = []
  for (const n of dirs) {
    // a screen with no template of its own (Record-Statements) draws through the shared frame, which carries neither
    if (!existsSync(join(here, n, 'template.jsx'))) continue
    const t = read(n + '/template.jsx')
    const bar = /<footer style=\{\{ height: "26px"/.test(t)
    if (bar || n === 'Whole-Story') {
      assert.match(t, /\{v\?\.loadLine \? \(/, n + ': its status bar does not carry the progress line')
      drawn.push(n)
    }
  }
  assert.ok(drawn.length >= 11, drawn.join(','))
  // one derivation: no screen writes its own
  for (const n of dirs) for (const f of readdirSync(join(here, n)).filter((x) => /\.(js|jsx)$/.test(x) && !x.endsWith('.test.js'))) {
    assert.doesNotMatch(read(n + '/' + f), /Reading new recordings/, n + '/' + f + ' says the progress line itself')
  }
})

test('(b) the Inbox’s status bar shows the line while a load runs, and not otherwise', () => {
  const view = { project: { id: 'shop', name: 'shop', root: '/r', threads: 0, whose_move: { you: 0, agent: 0, done: 0 } }, threads: [] }
  const data = M.inboxData({ projects: { projects: [Object.assign({ sessions: 1, last_activity: NOW.toISOString() }, view.project)] }, view, storage: null, now: NOW })
  assert.ok(words(M.render('Inbox', { data, frame: { load: RUNNING } })).includes('Reading new recordings · 12 of 569 · 1,204 rows added'))
  assert.ok(!words(M.render('Inbox', { data, frame: { load: { running: false } } })).includes('Reading new recordings'))
  const es = M.everyStatementFromChange(M.changeSnapshot(M.threadFixture(), null, NOW))
  assert.ok(words(M.render('Every-Statement', { data: es, frame: { load: RUNNING } })).includes('Reading new recordings · 12 of 569'))
})

test('(c) the open project re-reads every ~3 s while its load runs, and stops when it ends', () => {
  assert.equal(M.viewRereadEvery(RUNNING, 'shop'), 3000)
  assert.equal(M.viewRereadEvery(RUNNING, 'other'), undefined)
  assert.equal(M.viewRereadEvery({ running: false, project: 'shop' }, 'shop'), undefined)
  assert.equal(M.viewRereadEvery(null, 'shop'), undefined)
})

test('(b/c) the one watcher reads fast while a load runs, slowly otherwise, and stops with no one watching', async () => {
  const timers = []
  const answers = [RUNNING, { running: false }]
  let reads = 0
  const w = M.createLoadWatch({ read: async () => { reads++; return answers.shift() || { running: false } }, fast: 3000, slow: 15000, setTimer: (fn, ms) => { timers.push({ fn, ms }); return timers.length }, clearTimer: () => {} })
  const seen = []
  const off = w.subscribe((d) => seen.push(d && d.running))
  await new Promise((r) => setImmediate(r))
  assert.deepEqual(seen, [true])
  assert.equal(timers.at(-1).ms, 3000, 'fast while running')
  await timers.at(-1).fn()
  assert.equal(timers.at(-1).ms, 15000, 'slow once it ended')
  off()
  const before = reads
  await timers.at(-1).fn()
  assert.equal(reads, before, 'nothing is read with no one watching')
})

test('(4) the current project is the most recently active one WITH records; none when none has', () => {
  const P = (id, last, sessions) => ({ id, last_activity: last, sessions })
  assert.equal(M.currentProjectId([P('a', '2026-10-07', 0), P('b', '2026-10-01', 3)]), 'b')
  assert.equal(M.currentProjectId([P('a', '2026-10-07', 0)]), undefined)
  assert.equal(M.currentProjectId([P('a', '2026-10-07', 0)], 'a'), 'a', 'an address that names a project keeps it')
  const src = readFileSync(join(here, '../data/use-project.js'), 'utf8')
  assert.match(src, /noRecords && typeof window !== 'undefined'\) window\.location\.replace\(href\('start'\)\)/, 'with nothing recorded the default address goes to First run')
})

test('(a)+(4) First run: a recorded folder opens its Inbox; a load starts and the Inbox opens without waiting', () => {
  const projects = { projects: [
    { id: 'shop', name: 'shop', root: '/Users/someone/shop', complete: true, sessions: 4, last_activity: '2026-10-07T09:00:00Z', sources: ['codex'] },
    { id: 'new', name: 'fresh', root: '/Users/someone/fresh', complete: true, sessions: 0, last_activity: '2026-10-06T09:00:00Z', sources: ['codex'] },
  ] }
  const data = M.firstRunData({ projects, storage: null, now: NOW })
  const calls = []
  const actions = { open: (id) => calls.push(['open', id]), load: (id) => calls.push(['load', id]) }
  const v = M.vals('Onboarding', { data, actions })
  assert.equal(v.v.continueLabel, 'Open its Inbox')
  v.v.primary.act()
  v.v.folders.find((f) => f.id === 'shop').ui.pick()
  assert.deepEqual(calls, [['open', 'shop'], ['open', 'shop']])
  // a folder with no records: picking it chooses it; its action loads
  const inst = M.vals('Onboarding', { data, actions })
  inst.v.folders.find((f) => f.id === 'new').ui.pick()
  assert.equal(calls.length, 2, 'picking a folder without records does not navigate')
  const only = M.firstRunData({ projects: { projects: [projects.projects[1]] }, storage: null, now: NOW })
  const v2 = M.vals('Onboarding', { data: only, actions })
  assert.equal(v2.v.continueLabel, 'Load every session')
  v2.v.primary.act()
  assert.deepEqual(calls.at(-1), ['load', 'new'])
  assert.ok(v2.v.primary.note.includes('The Inbox opens at once'))
  const live = read('Onboarding/logic.js') + read('Onboarding/index.jsx')
  assert.doesNotMatch(live, /Inbox opens when it is done/, 'nothing says the Inbox waits for the load')
  assert.match(read('Onboarding/index.jsx'), /start\(id\)\.then\(\(ok\) => \{ if \(ok\) open\(id\) \}\)/, 'the load opens the Inbox as soon as it has started')
})
