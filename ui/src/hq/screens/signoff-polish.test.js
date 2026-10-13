/* Sign-off polish (HQ Next), from the founder's real-store shots:
 *
 *  F2  one question, one answer: the sidebar's Inbox badge is ONE derivation
 *      (data/project.js `inboxBadge`, the current project's own count) on
 *      every screen that draws it; null draws no badge, never 0.
 *  F   the sidebar lists the current project first, marked current, then the
 *      others by last activity, newest first.
 *  G   a project's line stays on one line, whole in title=.
 *  E/H the folder named after the account reads "Home folder", path `~`.
 *  C   Repair-Loop's stop strip: quote and step fit their column; First run's
 *      long paths are cut in the middle, whole in title=.
 *  D   Change-Brief: the statements of one message share one line, so the
 *      424px stand-still region holds its last line.
 * Fixtures are synthetic; nothing here is the founder's record. */
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
  const names = ['Inbox', 'Onboarding', 'Change-Brief', 'Repair-Loop', 'Record-Over-Time', 'Team-View', 'Every-Statement']
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { withFrame, inboxData, firstRunData, inboxBadge, sidebarProjects } from '../data/project.js'
import { changeSnapshot } from '../data/change.js'
import { threadFixture } from './What-Changed/thread-fixture.js'
import { selectRepairLoop } from './Repair-Loop/from-change.js'
import { recordFromProjects, recordFromStatements } from './Record-Over-Time/from-projects.js'
import { everyStatementFromChange } from './Every-Statement/from-change.js'
import { teamFromStore } from './Team-View/from-store.js'
${names.map((n, i) => `import T${i} from './${n}/template.jsx'\nimport L${i} from './${n}/logic.js'`).join('\n')}
const S = { ${names.map((n, i) => `'${n}': [T${i}, L${i}]`).join(', ')} }
export { inboxData, firstRunData, inboxBadge, sidebarProjects, changeSnapshot, threadFixture, selectRepairLoop, recordFromProjects, recordFromStatements, everyStatementFromChange, teamFromStore }
export function render(name, props, { live = true, state } = {}) {
  const [Template, Logic] = S[name]
  const L = live ? withFrame(Logic) : Logic
  const inst = new L(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
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
  const file = join(await mkdtemp(join(tmpdir(), 'hq-signoff-')), 'screens.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const words = (html) => unescape(html.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ')
const NOW = new Date('2026-10-07T10:30:00.000Z')

const P = (id, name, root, last, you, extra = {}) => ({ id, name, root, complete: true, sessions: 2, threads: 3, claims: 4, sources: ['claude_code'], last_activity: last, whose_move: you == null ? null : { you, agent: 0, done: 0 }, ...extra })
const PROJECTS = [
  P('old', 'old-notes', '/Users/someone/old-notes', '2026-08-01T10:00:00Z', 7),
  P('home', 'someone', '/Users/someone', '2026-09-01T10:00:00Z', null, { complete: false, threads: null }),
  P('shop', 'shop', '/Users/someone/shop', '2026-10-07T09:00:00Z', 4),
  P('mid', 'mid', '/Users/someone/mid', '2026-09-20T10:00:00Z', 2),
]
const viewOf = (p, you) => ({ project: Object.assign({}, p, { whose_move: { you, agent: 0, done: 0 } }), threads: Array.from({ length: you }, (_, i) => ({ id: 't' + i, title: 'Work ' + i, whose_move: 'you', whose_move_basis: 'claim_without_passing_test', last_activity_at: p.last_activity, current_version_id: 'wt-1', proof: {} })) })

/* ---- F2: one badge ---- */

// C46 §1.1: the badge is the one count (data/project.js `yourMove`), from the project's readiness
// reading; here the readiness is read and nothing is ready (screens/your-move.test.js has the rest)
const READ = (id) => ({ project: id, ready: {}, moves: {}, failed: 0, listFailed: false })

test('F2: the badge is the current project’s own count — `?project=` when named, else the most recently active; null when not counted', () => {
  assert.equal(M.inboxBadge(PROJECTS, undefined, READ('shop')), 4, 'the most recently active project')
  assert.equal(M.inboxBadge(PROJECTS, 'old', READ('old')), 7)
  assert.equal(M.inboxBadge(PROJECTS, 'home', READ('home')), null, 'a quick row is not yet counted: no badge, never 0')
  assert.equal(M.inboxBadge([]), null)
  assert.equal(M.inboxBadge([P('z', 'z', '/r', '2026-10-01T00:00:00Z', 0)], undefined, READ('z')), 0, 'a counted 0 is a 0')
  assert.equal(M.inboxBadge(PROJECTS), null, 'the readiness not read yet: no badge, never one that may be short')
  assert.equal(M.inboxBadge(PROJECTS, 'old', READ('shop')), null, 'another project’s reading is not this one’s')
})

test('F2: Inbox, Record over time and Team draw the same badge for the same store, and it is the Inbox headline’s count', () => {
  for (const cur of ['shop', 'old', undefined]) {
    const id = cur || 'shop'
    const p = PROJECTS.find((x) => x.id === id)
    const inbox = M.render('Inbox', { data: M.inboxData({ projects: { projects: PROJECTS }, view: viewOf(p, p.whose_move.you), ready: {}, storage: null, now: NOW }), frame: {} })
    const rt = M.render('Record-Over-Time', { data: M.recordFromProjects({ projects: PROJECTS, storage: null, now: NOW.getTime(), current: cur, reading: READ(id) }), frame: {} })
    const team = M.render('Team-View', { data: M.teamFromStore({ project: PROJECTS.find((x) => x.id === id), projects: PROJECTS, changes: [], storage: null, licence: null, now: NOW.getTime(), reading: READ(id) }), frame: {} })
    const badge = (html) => (html.match(/>Inbox<\/span> <span aria-label="[^"]*"[^>]*>(\d+)<\/span>/) || [])[1]
    const want = String(p.whose_move.you)
    assert.equal(badge(inbox), want, 'Inbox ' + id)
    assert.equal(badge(rt), want, 'Record over time ' + id)
    assert.equal(badge(team), want, 'Team ' + id)
    assert.ok(words(inbox).includes(p.whose_move.you + ' Changes are your move'), 'the Inbox footer counts the same')
  }
  // not yet counted: no badge on any of them, never "0" or "null"
  const rt = M.render('Record-Over-Time', { data: M.recordFromProjects({ projects: PROJECTS, storage: null, now: NOW.getTime(), current: 'home' }), frame: {} })
  const team = M.render('Team-View', { data: M.teamFromStore({ project: PROJECTS[1], projects: PROJECTS, changes: [], storage: null, licence: null, now: NOW.getTime() }), frame: {} })
  for (const html of [rt, team]) {
    const i = html.indexOf('>Inbox</span>')
    const nav = html.slice(i, html.indexOf('</a>', i))
    assert.ok(i > 0 && !nav.includes('<span'), 'a badge with no count: ' + nav)
  }
})

test('F2 census: every screen that draws the Inbox badge takes it from inboxBadge, and none counts “your move” itself', () => {
  const dirs = readdirSync(here, { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => d.name)
  const BADGE = /\{"Inbox"\}<\/span>\{" "\}(?:\{[^{}]+ != null \? \()?<span aria-label=\{v\?\.([\w?.]+)Aria\}/
  const drawn = []
  for (const n of dirs) {
    // a screen with no template of its own (Record-Statements) draws through the shared frame, which carries neither
    if (!existsSync(join(here, n, 'template.jsx'))) continue
    const t = read(n + '/template.jsx')
    const m = t.match(BADGE)
    if (!m) continue
    drawn.push(n)
    assert.match(t, new RegExp('\\{v\\?\\.' + m[1].replace(/[?.]/g, (c) => '\\' + c) + ' != null \\? \\('), n + ': the badge is drawn even when there is no count')
    const src = readdirSync(join(here, n)).filter((f) => /\.(js|jsx)$/.test(f) && !f.endsWith('.test.js') && f !== 'demo.js' && f !== 'template.jsx').map((f) => read(n + '/' + f)).join('\n')
    const fromOne = /inboxBadge\(/.test(src) || (n === 'Inbox' && /D\.inboxBadge/.test(src) && /inboxBadge\(/.test(readFileSync(join(here, '../data/project.js'), 'utf8')))
    assert.ok(fromOne, n + ': its badge does not come from inboxBadge')
  }
  // C30 R-NAV: Projects draws the Inbox's sidebar; Changes draws it through the Inbox's own template
  assert.deepEqual(drawn.sort(), ['Inbox', 'Projects', 'Record-Over-Time', 'Team-View'])
  for (const n of dirs) {
    for (const f of readdirSync(join(here, n)).filter((x) => /\.(js|jsx)$/.test(x) && !x.endsWith('.test.js'))) {
      assert.doesNotMatch(read(n + '/' + f), /whose_move\s*&&\s*\w+\.whose_move\.you|whose_move\.you\)\s*\|\|\s*0/, n + '/' + f + ' counts “your move” itself')
    }
  }
})

/* ---- F, G, E/H: the sidebar ---- */

test('F: the sidebar lists the current project first, marked current, then the others newest first', () => {
  const side = M.sidebarProjects(PROJECTS, NOW, 'mid')
  assert.deepEqual(side.map((p) => p.id), ['mid', 'shop', 'home', 'old'])
  assert.deepEqual(side.map((p) => p.current), [true, false, false, false])
  const html = M.render('Inbox', { data: M.inboxData({ projects: { projects: PROJECTS }, view: viewOf(PROJECTS[3], 2), storage: null, now: NOW }), frame: {} })
  const list = html.slice(html.indexOf('>Projects</h2>'), html.indexOf('</section>', html.indexOf('>Projects</h2>')))
  const names = [...list.matchAll(/font-weight:500[^>]*>([^<]+)<\/span>/g)].map((m) => m[1])
  assert.deepEqual(names.slice(0, 4), ['mid', 'shop', 'Home folder', 'old-notes'])
  assert.equal((list.match(/aria-current="true"/g) || []).length, 1, 'one project is marked current')
  assert.ok(list.indexOf('aria-current="true"') < list.indexOf('>mid<'), 'the current mark is on the first row')
  assert.match(list, /aria-current="true" style="position:relative;[^"]*background:#16161A/)
  // Record over time lists them the same way
  const rt = M.render('Record-Over-Time', { data: M.recordFromProjects({ projects: PROJECTS, storage: null, now: NOW.getTime(), current: 'mid' }), frame: {} })
  const rlist = rt.slice(rt.indexOf('>Projects</h2>'))
  const rnames = [...rlist.matchAll(/font-weight:500;white-space:nowrap[^>]*>([^<]+)<\/span>/g)].map((m) => m[1])
  assert.deepEqual(rnames.slice(0, 4), ['mid', 'shop', 'Home folder', 'old-notes'])
})

test('G: a not-yet-counted project’s line stays on one line, whole in title=', () => {
  const html = M.render('Inbox', { data: M.inboxData({ projects: { projects: PROJECTS }, view: viewOf(PROJECTS[2], 4), storage: null, now: NOW }), frame: {} })
  const t = unescape(html)
  const i = t.indexOf('>recorded 1 Sep · not yet counted<')
  assert.ok(i > 0)
  const tag = t.slice(t.lastIndexOf('<span', i), i)
  assert.match(tag, /title="recorded 1 Sep · not yet counted"/)
  assert.match(tag, /white-space:nowrap;overflow:hidden;text-overflow:ellipsis/)
})

test('E/H: the folder named after the account reads “Home folder” with ~ as its path, on First run and in the sidebar', () => {
  const fr = M.firstRunData({ projects: { projects: PROJECTS }, storage: null, now: NOW })
  const home = fr.folders.find((f) => f.id === 'home')
  assert.equal(home.name, 'Home folder')
  assert.equal(home.path, '~')
  const html = words(M.render('Onboarding', { data: fr }))
  assert.ok(html.includes('Home folder'))
  assert.ok(!/\bsomeone\b/.test(html), 'the account name is not a folder name')
  assert.equal(M.sidebarProjects(PROJECTS, NOW).find((p) => p.id === 'home').name, 'Home folder')
})

/* ---- C: text that collided ---- */

test('C: First run cuts a long path in its middle, whole in title=, and neither name nor path can run under the chips', () => {
  const long = P('wt', 'f7ecf2', '/Users/someone/Desktop/Bookshelf/project/.claude/worktrees/trusting-ardinghelli-with-a-long-name/f7ecf2', '2026-10-07T08:00:00Z', 1)
  const fr = M.firstRunData({ projects: { projects: [long] }, storage: null, now: NOW })
  const html = unescape(M.render('Onboarding', { data: fr }))
  const full = '~/Desktop/Bookshelf/project/.claude/worktrees/trusting-ardinghelli-with-a-long-name/f7ecf2'
  const i = html.indexOf('title="' + full + '"')
  assert.ok(i > 0, 'the whole path is in title=')
  const tag = html.slice(i, html.indexOf('>', i))
  assert.match(tag, /white-space:nowrap;overflow:hidden;text-overflow:ellipsis/)
  const shown = html.slice(html.indexOf('>', i) + 1, html.indexOf('<', i))
  assert.ok(shown.length <= 52 && shown.includes('…') && shown.endsWith('/f7ecf2'), shown)
})

test('C: Repair-Loop’s stop strip — the quote fits beside its step, and the ① step cannot run into ②', () => {
  const v = M.threadFixture()
  v.claims[0].text = 'The next guarded suite, predeclaration, passed five checks before ownership inspection.'
  const s = M.changeSnapshot(v, null, NOW)
  const d = M.selectRepairLoop(s, null, { now: NOW })
  const vals = M.vals('Repair-Loop', { data: d, frame: {} })
  const room = Math.floor((150 - 20 - vals.statementStep.length * 7.2) / 7)
  assert.ok(vals.stripShort.length <= room, vals.stripShort + ' does not fit ' + room)
  assert.equal(vals.stripTitle, '❝' + d.statement.text + '❞ ' + vals.statementStep)
  const html = M.render('Repair-Loop', { data: d, frame: {} })
  const i = html.indexOf('title="' + vals.statementStep + '"')
  assert.ok(i > 0, 'the ① step carries title=')
  assert.match(html.slice(i, html.indexOf('>', i)), /overflow:hidden;text-overflow:ellipsis/)
  assert.match(html, /grid-template-columns:72px 76px 88px 76px 92px 72px/)
})

/* ---- D: the stand-still region holds its last line ---- */

test('D: Change-Brief puts one message’s statements on one line, so “Nothing counts as done yet” is not cut', () => {
  const v = M.threadFixture()
  const long = (n) => 'Statement ' + n + ' carries on and on about the historical suites and censuses and the rulings it passed.'
  v.claims = [0, 1, 2].map((k) => Object.assign({}, v.claims[0], { id: 'm1#' + k, text: long(k), has_verifier: k === 2 }))
  const s = M.changeSnapshot(v, null, NOW)
  const vals = M.vals('Change-Brief', { data: s, frame: {} })
  const total = vals.saidStmts.reduce((n, h) => n + h.text.length + 2, 0)
  const pane = 1512 - 460 - 48
  assert.ok(total * 7.4 <= pane - 280 + 1, 'the statements need ' + Math.round(total * 7.4) + 'px of ' + (pane - 280))
  assert.ok(vals.saidStmts.every((h) => h.text.endsWith('…')), 'each is cut, whole in the inspector')
  assert.ok(words(M.render('Change-Brief', { data: s, frame: {} })).includes('Nothing counts as done yet, so nothing is ready.'))
  // the gallery draws the canvas as it is
  const demo = M.vals('Change-Brief', {})
  assert.ok(demo.saidStmts.length > 0)
})

/* ---- C26 ---- */

test('C26-1: First run ranks folders newest first, the Home folder last, and never chooses it for the person', () => {
  const fr = M.firstRunData({ projects: { projects: PROJECTS }, storage: null, now: NOW })
  assert.deepEqual(fr.folders.map((f) => f.id), ['shop', 'mid', 'old', 'home'])
  assert.equal(fr.defaultFolder, 'shop')
  const html = unescape(M.render('Onboarding', { data: fr }))
  const checked = [...html.matchAll(/role="radio" aria-checked="true"[\s\S]*?font-weight:500[^>]*>([^<]+)</g)].map((m) => m[1])
  assert.deepEqual(checked, ['shop'], 'the most recently active folder is chosen')
  // even when the home folder is the most recently active
  const homeNewest = PROJECTS.map((p) => (p.id === 'home' ? Object.assign({}, p, { last_activity: '2026-10-07T10:00:00Z' }) : p))
  const fr2 = M.firstRunData({ projects: { projects: homeNewest }, storage: null, now: NOW })
  assert.equal(fr2.folders[fr2.folders.length - 1].name, 'Home folder')
  assert.equal(fr2.defaultFolder, 'shop')
  // the home folder alone: nothing is chosen, and the one action says to choose
  const only = M.firstRunData({ projects: { projects: [PROJECTS[1]] }, storage: null, now: NOW })
  assert.equal(only.defaultFolder, null)
  const h3 = unescape(M.render('Onboarding', { data: only }))
  assert.doesNotMatch(h3, /role="radio" aria-checked="true"/, 'nothing pre-selected')
  assert.ok(words(h3).includes('Choose a folder to load'))
  assert.ok(!words(h3).includes('Load every session'))
})

test('C26-2: Record over time — the re-check card and the headline give one answer to one question', () => {
  const day = (n) => new Date(2026, 9, 7 - n, 10, 0, 0).toISOString()
  const now = new Date(2026, 9, 7, 12, 0, 0).getTime()
  const c = (id, verdict, basis, n = 0) => ({ id, thread_id: 't', ts: day(n), agent: 'codex', session_id: 's', kind: 'tests_pass', has_verifier: true, verdict, verdict_basis: basis })
  const doc = { claims: [c('a', 'proven', 'run_holds'), c('b', 'not_proven', 'edited_since_run'), c('c', 'not_proven', 'edited_since_run', 1), c('d', 'not_proven', 'run_on_older_version', 2)], sessions: [{ id: 's', source: 'codex', started_at: day(3), last_at: day(0) }] }
  const d = M.recordFromStatements({ projects: [{ id: 'p1', name: 'shop', threads: 1, claims: 4, sources: ['codex'], last_activity: day(0) }], statements: { p1: doc }, storage: null, now })
  const t = words(M.render('Record-Over-Time', { data: d, frame: {} }, { state: { range: '30' } }))
  const head = Number((t.match(/(\d+) needs? a re-check/) || [])[1])
  const card = Number((t.match(/Need a re-check now › (\d+) of/) || [])[1])
  assert.ok(head > 0, t.slice(0, 300))
  assert.equal(card, head, 'the card and the headline disagree: ' + t.slice(0, 600))
  assert.ok(!t.includes('Needed a re-check after a later edit'), 'the live card asks the served question')
})

test('C26-3: Every statement — ❝ and ❞ sit with the first and last words, however long the quote', () => {
  const v = M.threadFixture()
  const long = 'The historical processes suite had already passed 10/10 twice with clean censuses; at the current head, the QR-4 rulings suite passed 6/6 with a clean census.'
  v.claims[0].text = long
  v.claims[1].text = 'Short one.'
  const s = M.changeSnapshot(v, null, NOW)
  const data = M.everyStatementFromChange(s)
  for (const st of s.statements) {
    const html = unescape(M.render('Every-Statement', { data, frame: {} }, { state: { sel: st.id } }))
    const said = html.indexOf(' said</span>')
    assert.ok(said > 0, 'the inspector names who said it')
    const i = html.indexOf(st.plain + '<span aria-hidden', said)
    assert.ok(i > 0, 'the quote is drawn: ' + st.plain)
    const button = html.slice(html.lastIndexOf('<button', i), html.indexOf('</button>', i) + 9)
    assert.match(button, /^<button[^>]*><span aria-hidden="true" style="display:inline-block;color:#B78CFF">❝<\/span>/, 'the opening mark is inside the box with the first word')
    assert.match(button, /<span aria-hidden="true" style="display:inline-block;color:#B78CFF">❞<\/span><\/button>$/, 'the closing mark is inside the box with the last word')
  }
  // the gallery keeps the canvas's own markup
  const demo = M.render('Every-Statement', {}, { live: false })
  assert.doesNotMatch(demo, /display:inline-block;color:#B78CFF">❝/)
})
