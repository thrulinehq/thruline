/* C30 R-NAV: the home nav goes somewhere, on every home screen.
 *
 *  1. Changes (`g c`) is `#/changes` and Projects (`g p`) is `#/projects`:
 *     the router reads and writes both, and the app has a screen for each.
 *  2. Changes draws every Change and thread of work across projects, with
 *     the Inbox's own template and rows, one block per project under the
 *     sidebar's own project row; the Inbox's filters plus a state filter,
 *     every count the rows it shows; a project still being read, or not
 *     read, says so and is never "nothing needed".
 *  3. Projects draws `/api/hq/projects` with First run's folder rows, each a
 *     link to that project's Inbox; "Add…" goes to First run.
 *  4. Team (`g t`) is a nav item on every home screen, drawn exactly as the
 *     other nav items; Changes and Projects are links on every home screen.
 *  5. Sidebar project rows are links to that project's Inbox.
 *  6. The links R-LINKS left as no link that now have a target point at it;
 *     the ones whose target does not exist yet stay no link.
 *  7. The design gallery draws none of this: its markup is the canvas's.
 *
 * Synthetic fixtures only (made-up projects under /Users/someone). Each
 * piece is bundled on its own, so a module that does not exist fails the
 * tests that need it, not the file. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>', { url: 'http://127.0.0.1/' })
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const here = dirname(fileURLToPath(import.meta.url))
const read = (p) => readFileSync(join(here, p), 'utf8')

/** Bundle one entry and import it; a missing module rejects (and fails only the tests that await it). */
async function bundle(entry) {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: entry, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-nav-')), 'm.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}
const lazy = (entry) => { let p = null; return () => (p = p || bundle(entry)) }

const HARNESS = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { createRoot } from 'react-dom/client'
import { flushSync } from 'react-dom'
import { DcScreen } from '../dc/runtime.jsx'
import { designLink } from '../artboards.jsx'
import { withFrame } from '../data/project.js'
import * as R from '../routes.js'
export { R }
export function renderWith(T, L, props, link, state, live = true) {
  const C = live ? withFrame(L) : L
  const inst = new C(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(T, { v: inst.renderVals(), link: link || ((h) => h) }))
}
export function designWith(T, L) {
  return renderToStaticMarkup(React.createElement(DcScreen, { Logic: L, Template: T, props: {}, link: designLink }))
}
export function mountWith(T, L, props, link) {
  const el = document.createElement('div')
  document.body.appendChild(el)
  const root = createRoot(el)
  flushSync(() => root.render(React.createElement(DcScreen, { Logic: withFrame(L), Template: T, props, link })))
  return { el, unmount: () => { root.unmount(); el.remove() } }
}`

const HOME_SCREENS = `
import InboxT from './Inbox/template.jsx'
import InboxL from './Inbox/logic.js'
import RecordT from './Record-Over-Time/template.jsx'
import RecordL from './Record-Over-Time/logic.js'
import TeamT from './Team-View/template.jsx'
import TeamL from './Team-View/logic.js'
import SettingsT from './Settings/template.jsx'
import SettingsL from './Settings/logic.js'
import { inboxData } from '../data/project.js'
import { recordFromStatements } from './Record-Over-Time/from-projects.js'
import { teamFromStore } from './Team-View/from-store.js'
import { settingsFromServer } from './Settings/from-server.js'
export const S = { Inbox: [InboxT, InboxL], 'Record-Over-Time': [RecordT, RecordL], 'Team-View': [TeamT, TeamL], Settings: [SettingsT, SettingsL] }
export { inboxData, recordFromStatements, teamFromStore, settingsFromServer }`

const base = lazy(HARNESS + HOME_SCREENS)
const changes = lazy(HARNESS + HOME_SCREENS + `
import ChangesT from './Changes/template.jsx'
import ChangesL, { inState, STATES, FILTERS } from './Changes/logic.js'
import { changesData, projectsToRead } from './Changes/from-projects.js'
import { readViews, AT_ONCE } from './Changes/index.jsx'
S.Changes = [ChangesT, ChangesL]
export { changesData, projectsToRead, readViews, AT_ONCE, inState, STATES, FILTERS }`)
const projects = lazy(HARNESS + HOME_SCREENS + `
import ProjectsT from './Projects/template.jsx'
import ProjectsL from './Projects/logic.js'
import { projectsData } from './Projects/from-projects.js'
S.Projects = [ProjectsT, ProjectsL]
export { projectsData }`)
const everything = lazy(HARNESS + HOME_SCREENS + `
import ChangesT from './Changes/template.jsx'
import ChangesL from './Changes/logic.js'
import { changesData } from './Changes/from-projects.js'
import ProjectsT from './Projects/template.jsx'
import ProjectsL from './Projects/logic.js'
import { projectsData } from './Projects/from-projects.js'
S.Changes = [ChangesT, ChangesL]
S.Projects = [ProjectsT, ProjectsL]
export { changesData, projectsData }`)

/* ---------------- fixtures (synthetic) ---------------- */

const NOW = new Date(2026, 9, 7, 16, 10)
const at = (h, m, day = 7) => new Date(2026, 9, day, h, m).toISOString()
const HOME = '/Users/someone'
const tests0 = { total: 0, succeeded: 0, failed: 0, unknown: 0 }
const thread = (id, over) => Object.assign({
  id, title: 'Thread ' + id, title_basis: 'instruction', started_at: at(15, 0), last_activity_at: at(15, 30),
  sessions: 1, prompts: 1, conversation: 0, edits: 0, files: 0, top_files: [], commands: tests0, tests: tests0, commits: 0, claims: 1,
  whose_move: 'done', whose_move_basis: 'nothing_open',
  proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: [] }, current_version_id: '3f9a1c2deadbeef',
}, over)
const SHOP = { id: 'p1', name: 'shop', root: HOME + '/code/shop', complete: true, threads: 4, sessions: 9, claims: 7, sources: ['claude_code', 'codex'], last_activity: at(16, 5), whose_move: { you: 2, agent: 1, done: 1 } }
const NOTES = { id: 'p2', name: 'notes', root: HOME + '/code/notes', complete: true, threads: 1234, sessions: 3, claims: 1, sources: ['codex'], last_activity: at(9, 0, 6), whose_move: { you: 1, agent: 0, done: 0 } }
const EMPTY = { id: 'p3', name: 'empty', root: HOME + '/code/empty', complete: true, threads: 0, sessions: 0, sources: [], last_activity: null, whose_move: { you: 0, agent: 0, done: 0 } }
const LIST = { complete: true, projects: [SHOP, NOTES, EMPTY] }
const VIEW_SHOP = {
  project: SHOP,
  threads: [
    thread('live1', { title: 'Shop live work', whose_move: 'agent', whose_move_basis: 'recent_activity', last_activity_at: at(16, 5), started_at: at(16, 1) }),
    thread('re1', { title: 'Fix the checkout total', whose_move: 'you', whose_move_basis: 'claim_without_passing_test',
      proof: { last_test: { outcome: 'succeeded' }, last_passing_test_at: at(15, 0), edited_after_last_pass: [HOME + '/code/shop/checkout/api.py'] } }),
    thread('ask1', { title: 'Shop question', whose_move: 'you', whose_move_basis: 'agent_asked', last_activity_at: at(16, 4) }),
    thread('done1', { title: 'Shop finished work', last_activity_at: at(9, 0, 6) }),
  ],
}
const VIEW_NOTES = {
  project: NOTES,
  threads: [thread('np1', { title: 'Notes not proven', whose_move: 'you', whose_move_basis: 'claim_without_passing_test', current_version_id: 'wt-00bb958a3c8b2ae7' })],
}
const STORAGE = { bytes: 5.4e9, path: HOME + '/.thruline' }
// C46 §1.1: each project's readiness, read, with nothing ready (Changes draws a block once both its view and this have arrived)
const READ = (id) => ({ project: id, ready: {}, moves: {}, failed: 0, listFailed: false })
const READINGS = { p1: READ('p1'), p2: READ('p2') }
const LICENCE_FREE = { status: 'absent', paid_gate: true, entitlements: { approve_version: { allowed: false, status: 'absent' } }, technical: {} }

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const words = (html) => unescape(html.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ').trim()
function anchors(html) {
  const out = []
  const re = /<a\b([^>]*)>([\s\S]*?)<\/a>/g
  let m
  while ((m = re.exec(html))) {
    const h = m[1].match(/\bhref="([^"]*)"/)
    const st = m[1].match(/\bstyle="([^"]*)"/)
    out.push({ href: h ? unescape(h[1]) : null, attrs: m[1], style: st ? st[1] : '', text: words(m[2]) })
  }
  return out
}
/** The markup between `<aside aria-label="HQ"` and its nav's end. */
const navOf = (html) => {
  const a = html.indexOf('<nav aria-label="Sections"')
  return a < 0 ? '' : html.slice(a, html.indexOf('</nav>', a) + 6)
}
const sidebarOf = (html) => {
  const a = html.indexOf('<section aria-label="Projects"')
  return a < 0 ? '' : html.slice(a, html.indexOf('</section>', a) + 10)
}

/** Every home screen, rendered on its live path. */
async function homePages(M, which) {
  const frame = {}
  const P = {}
  const link = (screen) => M.R.linker({ screen, project: 'p1' })
  const props = (screen, data, extra = {}) => ({ data, frame, at: { screen, project: 'p1' }, ...extra })
  const want = (n) => !which || which.includes(n)
  if (want('Inbox')) P.Inbox = M.renderWith(...M.S.Inbox, props('inbox', M.inboxData({ projects: LIST, view: VIEW_SHOP, storage: STORAGE, now: NOW })), link('inbox'))
  if (want('Record-Over-Time')) P['Record-Over-Time'] = M.renderWith(...M.S['Record-Over-Time'], props('record', M.recordFromStatements({ projects: LIST.projects, statements: { p1: { claims: [], sessions: [] }, p2: { claims: [], sessions: [] }, p3: { claims: [], sessions: [] } }, storage: STORAGE, now: NOW.getTime(), current: 'p1' })), link('record'))
  if (want('Team-View')) P['Team-View'] = M.renderWith(...M.S['Team-View'], props('team', M.teamFromStore({ project: SHOP, projects: LIST.projects, changes: [], storage: STORAGE, licence: LICENCE_FREE, now: NOW.getTime() })), link('team'))
  if (want('Settings')) P.Settings = M.renderWith(...M.S.Settings, props('settings', M.settingsFromServer({ storage: STORAGE, lanes: null, licence: LICENCE_FREE, projects: LIST, now: NOW })), link('settings'))
  if (want('Changes')) P.Changes = M.renderWith(...M.S.Changes, props('changes', M.changesData({ projects: LIST, views: { p1: VIEW_SHOP, p2: VIEW_NOTES }, readings: READINGS, storage: STORAGE, now: NOW, current: 'p1' })), link('changes'))
  if (want('Projects')) P.Projects = M.renderWith(...M.S.Projects, props('projects', M.projectsData({ projects: LIST, storage: STORAGE, now: NOW, current: 'p1' })), link('projects'))
  return P
}

/** Every home screen: the four the canvas drew (asserted on first), then Changes and Projects. */
async function homePagesAll() {
  const M0 = await base()
  const first = await homePages(M0, ['Inbox', 'Record-Over-Time', 'Team-View', 'Settings'])
  let M = M0
  let P = first
  try {
    M = await everything()
    P = Object.assign({}, first, await homePages(M, ['Changes', 'Projects']))
  } catch (e) {
    P = Object.assign({}, first, { Changes: '', Projects: '' })
  }
  return Object.assign({}, M, { P })
}

/* ---------------- 1. the addresses ---------------- */

test('1. #/changes and #/projects are screens: the router reads and writes them, and the app draws each', async () => {
  const { R } = await base()
  assert.deepEqual(R.parse('#/changes'), { screen: 'changes', project: undefined })
  assert.deepEqual(R.parse('#/changes?project=p1'), { screen: 'changes', project: 'p1' })
  assert.deepEqual(R.parse('#/projects?project=p1'), { screen: 'projects', project: 'p1' })
  assert.equal(R.href('changes', { project: 'p1' }), '#/changes?project=p1')
  assert.equal(R.href('projects'), '#/projects')
  // a Change still opens its Brief
  assert.equal(R.parse('#/changes/abc').screen, 'brief')
  assert.equal(R.SCREENS.changes, 'Changes')
  assert.equal(R.SCREENS.projects, 'Projects')
  const app = read('../app.jsx')
  assert.match(app, /Changes: lazy\(\(\) => import\('\.\/screens\/Changes\/index\.jsx'\)\)/)
  assert.match(app, /Projects: lazy\(\(\) => import\('\.\/screens\/Projects\/index\.jsx'\)\)/)
})

/* ---------------- 2. Changes ---------------- */

test('2. Changes draws every thread of every project with records, under that project’s own sidebar row, linked to its Inbox', async () => {
  const M = await changes()
  const html = (await homePages(M, ['Changes'])).Changes
  const w = words(html)
  for (const title of ['Fix the checkout total', 'Shop question', 'Shop live work', 'Notes not proven']) assert.ok(w.includes(title), title + ' is not drawn')
  // one block per project with records, as a section named by the project; the project with none is not a block
  const blocks = [...html.matchAll(/<section aria-label="([^"]+)" style="display:contents">/g)].map((m) => m[1])
  assert.deepEqual(blocks, ['shop', 'notes'])
  // each block opens with the sidebar's project row, a link to that project's Inbox
  for (const [name, id] of [['shop', 'p1'], ['notes', 'p2']]) {
    const at = html.indexOf('<section aria-label="' + name + '" style="display:contents">')
    const head = anchors(html.slice(at, at + 2000))[0]
    assert.equal(head.href, '#/inbox?project=' + id, name)
    assert.ok(head.text.startsWith(name), head.text)
  }
  // the counts in the block heading: 1,234 threads read with the one count formatter
  assert.ok(w.includes('Counted by Change: every thread of work in 2 projects, 1,238 in all.'), w.slice(0, 400))
  // the rows are the Inbox's rows: same title, same address, same state words
  const inbox = (await homePages(M, ['Inbox'])).Inbox
  // a check row's own action goes to that thread's Check it again, in its own project
  assert.ok(anchors(html).some((a) => a.href === '#/changes/re1/check?project=p1'), 'shop’s row')
  assert.ok(anchors(html).some((a) => a.href === '#/changes/np1/check?project=p2'), 'notes’ row')
  const rowWords = (h, title) => { const i = h.indexOf(title); return words(h.slice(h.lastIndexOf('<div role="listitem"', i), h.indexOf('</div></div>', i))) }
  assert.equal(rowWords(html, 'Fix the checkout total').replace(/ ↵ open.*$/, ''), rowWords(inbox, 'Fix the checkout total').replace(/ ↵ open.*$/, ''))
  // the page is Changes: the crumb and the nav say so, and the Inbox is not the current item
  assert.match(html, /<span style="color:#EDEDEF;font-weight:500">Changes<\/span>/)
  const cur = anchors(navOf(html)).filter((a) => /aria-current="page"/.test(a.attrs))
  assert.deepEqual(cur.map((a) => a.text.replace(/ g c$/, '')), ['Changes'])
})

test('2. Changes has the Inbox’s filters plus a state filter; every count is the rows it shows', async () => {
  const M = await changes()
  const data = M.changesData({ projects: LIST, views: { p1: VIEW_SHOP, p2: VIEW_NOTES }, readings: READINGS, storage: STORAGE, now: NOW, current: 'p1' })
  const props = { data, frame: {}, at: { screen: 'changes', project: 'p1' } }
  const link = M.R.linker({ screen: 'changes', project: 'p1' })
  const render = (state) => M.renderWith(...M.S.Changes, props, link, state)
  const titles = (h) => ['Fix the checkout total', 'Shop question', 'Shop live work', 'Notes not proven', 'Shop finished work'].filter((t) => {
    // a row is shown when its title is drawn inside a displayed section
    const re = new RegExp('<section aria-label="[^"]+" style="display:flex;[^"]*"[^>]*>(?:(?!</section>).)*' + t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 's')
    return re.test(h)
  })
  const groups = (h) => [...h.matchAll(/<div role="group" aria-label="(Show|State)"[\s\S]*?<\/div>/g)].map((m) => [m[1], [...m[0].matchAll(/<button[^>]*aria-pressed="(true|false)"[^>]*>([^<]*)<span[^>]*>([^<]*)<\/span>/g)].map((b) => [b[2], b[3], b[1]])])
  const all = render({ nothingOpen: false })
  const g = Object.fromEntries(groups(all))
  assert.deepEqual(g.Show.map((x) => x[0]), ['All', 'Your move', 'The agent’s', 'Recording'])
  assert.deepEqual(g.State.map((x) => x[0]), ['Any state', 'Your turn', 'Needs a re-check', 'Not proven', 'Nothing open'])
  const counts = Object.fromEntries([...g.Show, ...g.State].map((x) => [x[0], x[1]]))
  // every count is the rows that choice shows
  const cases = [
    [{ filter: 'you' }, ['Fix the checkout total', 'Shop question', 'Notes not proven'], counts['Your move']],
    [{ filter: 'live' }, ['Shop live work'], counts.Recording],
    [{ filter: 'agent' }, ['Shop live work'], counts['The agent’s']],
    [{ st: 'recheck' }, ['Fix the checkout total'], counts['Needs a re-check']],
    [{ st: 'notProven' }, ['Notes not proven'], counts['Not proven']],
    [{ st: 'asked' }, ['Shop question'], counts['Your turn']],
    [{ st: 'done' }, ['Shop finished work'], counts['Nothing open']],
    [{ filter: 'you', st: 'recheck' }, ['Fix the checkout total'], null],
  ]
  for (const [state, rows, count] of cases) {
    const h = render(state)
    assert.deepEqual(titles(h), rows, JSON.stringify(state))
    if (count != null) assert.equal(count, String(rows.length), JSON.stringify(state) + ' count')
  }
  // a click on a state chooses it (the handlers are live, not drawn only)
  const { el, unmount } = M.mountWith(...M.S.Changes, props, link)
  try {
    const btn = [...el.querySelectorAll('div[aria-label="State"] button')].find((b) => b.textContent.startsWith('Not proven'))
    btn.dispatchEvent(new dom.window.MouseEvent('click', { bubbles: true }))
    await new Promise((r) => setTimeout(r, 20)) // React draws the click on its next turn
    const now = [...el.querySelectorAll('div[aria-label="State"] button')].find((b) => b.textContent.startsWith('Not proven'))
    assert.equal(now.getAttribute('aria-pressed'), 'true')
    assert.ok(el.textContent.includes('Notes not proven'))
    assert.ok(![...el.querySelectorAll('section[aria-label="Needs a check"]')].some((s) => s.style.display !== 'none' && s.textContent.includes('Fix the checkout total')))
  } finally {
    unmount()
  }
})

test('2. a project still being read, or not read, says so; no count is said until every project is read', async () => {
  const M = await changes()
  const data = M.changesData({ projects: LIST, views: { p1: VIEW_SHOP, p2: { error: 'answered 500' } }, readings: { p1: READ('p1') }, storage: STORAGE, now: NOW, current: 'p1' })
  const w = words(M.renderWith(...M.S.Changes, { data, frame: {}, at: { screen: 'changes', project: 'p1' } }, M.R.linker({ screen: 'changes' })))
  assert.ok(w.includes('ThruLine could not read this project’s record just now.'), 'a failed view is said')
  const reading = M.changesData({ projects: LIST, views: { p1: VIEW_SHOP }, readings: { p1: READ('p1') }, storage: STORAGE, now: NOW })
  const rhtml = M.renderWith(...M.S.Changes, { data: reading, frame: {}, at: { screen: 'changes' } }, M.R.linker({ screen: 'changes' }))
  const rw = words(rhtml)
  assert.ok(rw.includes('Reading the record of notes…'), 'the unread project says it is being read')
  assert.equal(words(rhtml.match(/<h1[^>]*>([\s\S]*?)<\/h1>/)[1]), 'Reading the record…', 'the headline counts nothing while reading')
  assert.ok(rw.includes('Counted by Change: every thread of work in 1 of 2 projects read so far.'))
  assert.equal(words(rhtml.match(/<footer[^>]*>\s*<span>([\s\S]*?)<\/span>/)[1]), '', 'the status bar counts nothing while reading')
  // the rows read so far are drawn
  assert.ok(rw.includes('Fix the checkout total'))
  // the views are read four at a time, every one, and a failed read is a failure, not an empty project
  let open = 0
  let most = 0
  const got = {}
  await M.readViews(['a', 'b', 'c', 'd', 'e', 'f', 'g'], (id, doc) => { got[id] = doc }, {
    get: async (url) => { open++; most = Math.max(most, open); await new Promise((r) => setTimeout(r, 5)); open--; if (url.endsWith('/c')) throw new Error('no'); return { url } },
  })
  assert.equal(M.AT_ONCE, 4)
  assert.ok(most <= 4 && most > 1, 'at once: ' + most)
  assert.deepEqual(Object.keys(got).sort(), ['a', 'b', 'c', 'd', 'e', 'f', 'g'])
  assert.deepEqual(got.c, { error: 'no' })
  assert.equal(got.a.url, '/api/hq/projects/a')
  assert.deepEqual(M.projectsToRead(LIST).map((p) => p.id), ['p1', 'p2'], 'only projects with records, newest first')
})

/* ---------------- 3. Projects ---------------- */

test('3. Projects draws every project with First run’s folder rows, each a link to that project’s Inbox; Add… goes to First run', async () => {
  const M = await projects()
  const html = (await homePages(M, ['Projects'])).Projects
  const list = html.slice(html.indexOf('<div role="list" aria-label="Projects"'))
  const rows = anchors(list).filter((a) => /role="listitem"/.test(a.attrs))
  assert.deepEqual(rows.map((r) => r.href), ['#/inbox?project=p1', '#/inbox?project=p2', '#/inbox?project=p3'])
  // First run's folder-row markup: the grid, the ring, the name over the path, the chips
  const first = read('Onboarding/template.jsx')
  for (const r of rows) {
    assert.match(r.style, /^display:grid;grid-template-columns:18px minmax\(0, 1fr\) auto;gap:16px;align-items:center;padding:14px 16px;/)
  }
  assert.ok(first.includes('gridTemplateColumns: "18px minmax(0, 1fr) auto", gap: "16px", alignItems: "center", padding: "14px 16px"'), 'the row style is First run’s')
  // the project the person is in carries First run's chosen look
  assert.match(rows[0].attrs, /aria-current="true"/)
  assert.match(rows[0].style, /border:1px solid #FF7A29/)
  assert.doesNotMatch(rows[1].attrs, /aria-current/)
  const w = words(list)
  assert.ok(w.includes('~/code/shop'), 'the path, without the home folder')
  assert.ok(!html.includes(HOME), 'no home folder anywhere')
  assert.ok(w.includes('1,234 Changes'), 'a count of 1,000 or more is grouped')
  assert.ok(w.includes('Not recorded yet'))
  // Add… stays, and goes to First run
  const add = anchors(sidebarOf(html)).find((a) => a.text === 'Add…')
  assert.equal(add.href, '#/start?project=p1')
  // the page is Projects
  const cur = anchors(navOf(html)).filter((a) => /aria-current="page"/.test(a.attrs))
  assert.deepEqual(cur.map((a) => a.text.replace(/ g p$/, '')), ['Projects'])
  assert.ok(words(html).includes('Projects'))
})

/* ---------------- 4. the nav on every home screen ---------------- */

const NAV_STYLE = 'height:32px;display:flex;align-items:center;gap:10px;padding:0 8px 0 10px;border-radius:6px;color:#A1A1AA;font-size:13.5px'

test('4. Team is a nav item on every home screen, drawn exactly as the other nav items', async () => {
  // the screens the canvas drew first (their failures say what is missing), then the two new ones
  const M = await homePagesAll()
  const P = M.P
  const bad = []
  for (const [name, html] of Object.entries(P)) {
    const team = anchors(html).filter((a) => a.text === 'Team g t')
    if (team.length !== 1) { bad.push(name + ': ' + team.length + ' Team items'); continue }
    const t = team[0]
    if (!t.href || M.R.parse(t.href).screen !== 'team') bad.push(name + ': Team → ' + t.href)
    if (name === 'Team-View') continue // the current item there
    if (t.style !== NAV_STYLE) bad.push(name + ': Team is styled ' + t.style)
    // the same look as the Record over time item on that screen (where one is drawn)
    const rec = anchors(navOf(html)).find((a) => a.text === 'Record over time g r')
    if (rec && !/aria-current/.test(rec.attrs) && rec.style !== t.style) bad.push(name + ': Team differs from Record over time')
  }
  assert.deepEqual(bad, [])
})

test('4. Changes and Projects are links on every home screen that draws the HQ nav', async () => {
  // the screens the canvas drew first (their failures say what is missing), then the two new ones
  const M = await homePagesAll()
  const P = M.P
  const bad = []
  for (const name of ['Inbox', 'Record-Over-Time', 'Team-View', 'Changes', 'Projects']) {
    const nav = navOf(P[name])
    if (/<button/.test(nav)) bad.push(name + ': a nav item is still a button')
    for (const [label, screen] of [['Changes g c', 'changes'], ['Projects g p', 'projects']]) {
      const a = anchors(nav).find((x) => x.text === label)
      if (!a || !a.href) { bad.push(name + ': ' + label + ' is no link'); continue }
      const r = M.R.parse(a.href)
      if (r.screen !== screen || r.project !== 'p1') bad.push(name + ': ' + label + ' → ' + a.href)
    }
  }
  assert.deepEqual(bad, [])
})

/* ---------------- 5. sidebar project rows ---------------- */

test('5. the sidebar’s project rows are links to that project’s Inbox, on every home screen that draws them', async () => {
  // the screens the canvas drew first (their failures say what is missing), then the two new ones
  const M = await homePagesAll()
  const P = M.P
  const bad = []
  for (const name of ['Inbox', 'Record-Over-Time', 'Changes', 'Projects']) {
    const side = sidebarOf(P[name])
    const rows = anchors(side).filter((a) => a.text !== 'Add…')
    if (rows.length !== 3) bad.push(name + ': ' + rows.length + ' rows')
    const hrefs = rows.map((a) => a.href).sort()
    if (JSON.stringify(hrefs) !== JSON.stringify(['#/inbox?project=p1', '#/inbox?project=p2', '#/inbox?project=p3'])) bad.push(name + ': ' + JSON.stringify(hrefs))
    if (/<div aria-current/.test(side)) bad.push(name + ': a row is not a link')
  }
  assert.deepEqual(bad, [])
})

/* ---------------- 6. the links R-LINKS left as no link ---------------- */

test('6. Record over time’s “See these” and Settings’ “Change by Change” stay no link (Changes lists threads, not statements or criteria); the three statement counts open the statements across Changes they count; Team’s Change names open the Change', async () => {
  const M = await base()
  const claim = (id, verdict) => ({ id, thread_id: 't-' + id, ts: at(10, 0), agent: 'claude_code', kind: 'done', verdict, verdict_basis: null })
  const data = M.recordFromStatements({ projects: [SHOP], statements: { p1: { claims: [claim('a', 'recorded'), claim('b', 'proven')], sessions: [{ id: 's', source: 'claude_code', started_at: at(9, 0), last_at: at(11, 0) }] } }, storage: STORAGE, now: NOW.getTime(), current: 'p1' })
  const link = M.R.linker({ screen: 'record', project: 'p1' })
  const html = M.renderWith(...M.S['Record-Over-Time'], { data, frame: {}, at: { screen: 'record', project: 'p1' } }, link, { stop: 6 })
  /* Record over time (0648307f) draws no "See these" at all where its page would list statements across pieces
     of work; before that it drew it as no link. Either way it is never a link to Changes. */
  const see = anchors(html).find((a) => /^See (it|these)/.test(a.text))
  assert.ok(!see || see.href == null, 'Changes does not list these statements, so the stop panel has no link to it')
  const kpis = anchors(html).filter((a) => /class="tl-row"/.test(a.attrs) && /aria-label="(Statements|Proven|Need a re-check)/.test(a.attrs))
  assert.equal(kpis.length, 3, kpis.map((k) => k.attrs).join('\n'))
  /* C54 #7: the page that lists statements across Changes exists, so each count opens it, in the range, agent and project in view */
  const opened = kpis.map((k) => M.R.parse(k.href))
  for (const o of opened) assert.deepEqual({ ...o, subset: undefined }, { screen: 'recordStatements', project: 'p1', range: '30', agent: 'all', scope: 'all', subset: undefined }, 'a count opens its list')
  assert.deepEqual(opened.map((o) => o.subset), ['all', 'proven', 'open'], 'each count opens the statements it counts (this record does not say which proofs a later edit moved, so the third is the ones to re-check now)')

  const settings = M.renderWith(...M.S.Settings, { data: M.settingsFromServer({ storage: STORAGE, lanes: null, licence: LICENCE_FREE, projects: LIST, now: NOW }), frame: {}, at: { screen: 'settings', project: 'p1' } }, M.R.linker({ screen: 'settings', project: 'p1' }), { nav: 'rules' })
  const byChange = anchors(settings).find((a) => a.text.startsWith('What counts as done, Change by Change'))
  assert.ok(byChange, 'drawn')
  assert.equal(byChange.href, null, 'Changes does not show what counts as done, so the link is no link')

  const change = { id: 'chg_1', project_id: 'L1', request: { title: 'Fix the checkout total' }, accepted_scope: { criteria: ['totals add up'] },
    decisions: { history: [{ id: 'd1', act: 'approved', target_revision: '3f9a1c2deadbeef', actor: 'sam', decided_at: at(12, 0), exceptions: [{ obligation_id: 'criterion:1', kind: 'waived', actor: 'sam', authority: 'lead', rationale: 'later' }] }] } }
  const team = M.renderWith(...M.S['Team-View'], { data: M.teamFromStore({ project: Object.assign({}, SHOP, { members: [{ ledger_project_id: 'L1' }] }), projects: LIST.projects, changes: [change], storage: STORAGE, licence: LICENCE_FREE, now: NOW.getTime() }), frame: {}, at: { screen: 'team', project: 'p1' } }, M.R.linker({ screen: 'team', project: 'p1' }))
  const names = anchors(team).filter((a) => a.text === 'Fix the checkout total')
  const hrefs = names.map((a) => a.href).sort()
  assert.deepEqual(hrefs, ['#/changes/chg_1', '#/changes/chg_1/approve'])
})

/* ---------------- 7. the gallery ---------------- */

test('7. the design gallery draws the canvas: no Team item where the canvas has none, nav items without a target, sidebar rows without a target', async () => {
  const M = await base()
  for (const name of ['Inbox', 'Record-Over-Time', 'Settings']) {
    const html = M.designWith(...M.S[name])
    assert.ok(!anchors(html).some((a) => a.text === 'Team g t'), name + ' draws a Team item in the gallery')
  }
  for (const name of ['Inbox', 'Record-Over-Time', 'Team-View']) {
    const nav = navOf(M.designWith(...M.S[name]))
    for (const label of ['Changes g c', 'Projects g p']) {
      const a = anchors(nav).find((x) => x.text === label)
      assert.ok(a, name + ': ' + label + ' is drawn')
      assert.equal(a.href, null, name + ': ' + label + ' has a target in the gallery')
      assert.equal(a.style, NAV_STYLE, name + ': ' + label + ' style')
    }
  }
  const side = sidebarOf(M.designWith(...M.S.Inbox))
  assert.ok(!anchors(side).some((a) => a.href && a.href.includes('project=')), 'gallery sidebar rows have no target')
})

/* ---------------- copy rules ---------------- */

test('copy: the two new screens use plain words — no intent word, no confidence, no trial — and no home folder', async () => {
  const M = await everything()
  const P = await homePages(M, ['Changes', 'Projects'])
  for (const [name, html] of Object.entries(P)) {
    const w = words(html)
    assert.doesNotMatch(w, /\b(lie|lied|lying|cheat|violat\w*|ignor\w*|hallucinat\w*|trial)\b/i, name)
    assert.doesNotMatch(w, /\d+\s?%/, name + ' says a percentage')
    assert.ok(!html.includes(HOME), name + ' draws the home folder')
  }
  const src = ['Changes/logic.js', 'Changes/from-projects.js', 'Changes/index.jsx', 'Projects/logic.js', 'Projects/from-projects.js', 'Projects/index.jsx'].map(read).join('\n')
  assert.doesNotMatch(src, /toLocaleString|Intl\./, 'counts go through count.js')
})
