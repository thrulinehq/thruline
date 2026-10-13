/* C30 R-LINKS: the root cause of dead links, pinned.
 *
 *  1. `linker` never falls back to the Inbox. Every `link("…")` target in
 *     every template, and every canvas page a logic file names, resolves to
 *     the screen it names; a link to a Change screen with no Change open, or
 *     to a page the app does not have, is no link at all.
 *  2. Every "Find, ask, act" link and every search field opens the palette
 *     (`openCommand`) over the screen, and does not follow its href.
 *  3. No rendered screen, live or in the design gallery, carries a raw
 *     `.dc.html` href; every live href is an address, a place on the same
 *     page, or an external page.
 *  4. `#ws-…` (and any hash that is not `#/…`) is a place on the page: the
 *     router never reads it, and a click scrolls without changing the address.
 *  5. The project crumb is a link on every Change screen, to that project's
 *     Inbox.
 *  6. Settings addresses name a pane: "see what" → Storage, the hint bar's
 *     "change" → Keyboard, and every plan link → Plan & licence, which
 *     Settings draws.
 *
 * Every screen is rendered here from synthetic fixtures (the What-Changed
 * thread fixture and made-up projects); nothing reads a store. `routes.js`
 * and `in-page.js` are read as namespaces so a missing export fails one test,
 * not the file. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, existsSync } from 'node:fs'
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

const ALL = ['Inbox', 'Onboarding', 'Change-Brief', 'Every-Statement', 'What-Changed', 'Done-Criteria', 'Repair-Loop', 'Approve-Sheet', 'Command-Ask', 'Whole-Story', 'PR-Face', 'Record-Over-Time', 'Team-View', 'Settings']
const CHANGE = ['Change-Brief', 'Every-Statement', 'What-Changed', 'Done-Criteria', 'Repair-Loop', 'Approve-Sheet', 'Command-Ask', 'Whole-Story']
/* the screens that draw a "Find, ask, act" link or a search field (PR-Face and
 * First run draw neither; the palette is Command-Ask itself) */
const FINDS = ['Inbox', 'Change-Brief', 'Every-Statement', 'What-Changed', 'Done-Criteria', 'Repair-Loop', 'Approve-Sheet', 'Whole-Story', 'Record-Over-Time', 'Team-View', 'Settings']
/* each screen's place in the app, as the router gives it */
const SCREEN_KEY = { Inbox: 'inbox', Onboarding: 'start', 'Change-Brief': 'brief', 'Every-Statement': 'statements', 'What-Changed': 'changed', 'Done-Criteria': 'done', 'Repair-Loop': 'check', 'Approve-Sheet': 'approve', 'Command-Ask': 'brief', 'Whole-Story': 'story', 'PR-Face': 'pr', 'Record-Over-Time': 'record', 'Team-View': 'team', Settings: 'settings' }

const M = await (async () => {
  const imports = ALL.map((n, i) => `import T${i} from './${n}/template.jsx'\nimport L${i} from './${n}/logic.js'`).join('\n')
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { createRoot } from 'react-dom/client'
import { flushSync } from 'react-dom'
import * as R from '../routes.js'
import * as SC from '../screen.jsx'
import { designLink } from '../artboards.jsx'
import { DcScreen } from '../dc/runtime.jsx'
import { withFrame, inboxData, firstRunData } from '../data/project.js'
import { changeSnapshot } from '../data/change.js'
import { threadFixture } from './What-Changed/thread-fixture.js'
import { everyStatementFromChange } from './Every-Statement/from-change.js'
import { whatChangedFromChange } from './What-Changed/from-change.js'
import { wholeStoryFrom } from './Whole-Story/from-change.js'
import { selectDoneCriteria } from './Done-Criteria/from-change.js'
import { selectApproveSheet } from './Approve-Sheet/from-change.js'
import { selectRepairLoop } from './Repair-Loop/from-change.js'
import { commandFromChange } from './Command-Ask/from-change.js'
import { prFaceFrom } from './PR-Face/from-change.js'
import { settingsFromServer } from './Settings/from-server.js'
import { recordFromStatements } from './Record-Over-Time/from-projects.js'
import { teamFromStore } from './Team-View/from-store.js'
${imports}
const S = { ${ALL.map((n, i) => `'${n}': [T${i}, L${i}]`).join(', ')} }
export { R, SC, designLink, changeSnapshot, threadFixture, everyStatementFromChange, whatChangedFromChange, wholeStoryFrom, selectDoneCriteria, selectApproveSheet, selectRepairLoop, commandFromChange, prFaceFrom, settingsFromServer, recordFromStatements, teamFromStore, inboxData, firstRunData }
const live = (name) => (name === 'PR-Face' ? S[name][1] : withFrame(S[name][1]))
/** a screen as the app draws it: its logic on the live path, its links through \`link\` */
export function render(name, props, link, state) {
  const [Template] = S[name]
  const L = live(name)
  const inst = new L(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link }))
}
/** the gallery: the canvas's own logic and demo data */
export function renderDesign(name) {
  const [Template, Logic] = S[name]
  return renderToStaticMarkup(React.createElement(DcScreen, { Logic, Template, props: {}, link: designLink }))
}
/** mounted in jsdom, so its handlers run */
export function mount(name, props, link) {
  const el = document.createElement('div')
  document.body.appendChild(el)
  const root = createRoot(el)
  flushSync(() => root.render(React.createElement(DcScreen, { Logic: live(name), Template: S[name][0], props, link })))
  return { el, unmount: () => { root.unmount(); el.remove() } }
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-links-')), 'screens.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const R = M.R
const mount = (name, props, link) => M.mount(name, props, link)

/* ---- fixtures (synthetic) ---- */
const NOW = new Date('2026-10-07T10:30:00.000Z')
const PROJECTS = {
  complete: true,
  projects: [
    { id: 'p1', name: 'shop', root: '/Users/someone/shop', complete: true, sessions: 3, threads: 1, claims: 2, sources: ['claude_code'], last_activity: '2026-10-07T10:06:00.000Z', whose_move: { you: 1, agent: 0, done: 0 } },
  ],
}
const VIEW = {
  project: Object.assign({}, PROJECTS.projects[0]),
  threads: [{ id: '0123456789abcdef', title: 'Fix the checkout total', whose_move: 'you', whose_move_basis: 'claim_without_passing_test', current_version_id: 'wt-2222222222222222', last_activity_at: '2026-10-07T10:06:00.000Z', started_at: '2026-10-07T10:00:00.000Z', sessions: 1, proof: { last_passing_test_at: '2026-10-07T10:02:00.000Z', edited_after_last_pass: [] }, top_files: [] }],
}
const STORAGE = { bytes: 5.4e9, path: '/Users/someone/.thruline', parts: [{ path: '/Users/someone/.thruline/ledger.db', bytes: 5e9 }] }
const LICENCE_FREE = { status: 'absent', paid_gate: true, entitlements: { approve_version: { allowed: false, status: 'absent' } }, technical: {} }

const snap = () => M.changeSnapshot(M.threadFixture(), null, NOW, { storageBytes: 5.4e9 })
const PID = () => snap().project.id

/** Every screen's live data, from one record. */
function liveProps(name, s = snap()) {
  const frame = {}
  switch (name) {
    case 'Inbox': return { data: M.inboxData({ projects: PROJECTS, view: VIEW, storage: STORAGE, now: NOW }), frame }
    case 'Onboarding': return { data: M.firstRunData({ projects: PROJECTS, storage: STORAGE, now: NOW }), frame }
    case 'Change-Brief': return { data: s, frame }
    case 'Every-Statement': return { data: M.everyStatementFromChange(s), frame }
    case 'What-Changed': return { data: M.whatChangedFromChange(s), frame }
    case 'Done-Criteria': return { data: M.selectDoneCriteria(s, null), frame }
    case 'Repair-Loop': return { data: M.selectRepairLoop(s, null, { now: NOW }), frame }
    case 'Approve-Sheet': return { data: M.selectApproveSheet(s, null, { licence: LICENCE_FREE }), frame }
    case 'Command-Ask': return { data: M.commandFromChange(s, VIEW, { readAt: '10:30' }), frame }
    case 'Whole-Story': return { data: M.wholeStoryFrom(s, null, { now: NOW.getTime() }), frame }
    case 'PR-Face': return { data: M.prFaceFrom(s, { now: NOW.getTime() }) }
    case 'Record-Over-Time': return { data: M.recordFromStatements({ projects: PROJECTS.projects, statements: { p1: { claims: [], sessions: [] } }, storage: STORAGE, now: NOW.getTime() }), frame }
    case 'Team-View': return { data: M.teamFromStore({ project: PROJECTS.projects[0], projects: PROJECTS.projects, changes: [], storage: STORAGE, licence: LICENCE_FREE, now: NOW.getTime() }), frame }
    case 'Settings': return { data: M.settingsFromServer({ storage: STORAGE, lanes: null, licence: LICENCE_FREE, projects: PROJECTS, now: NOW }), frame }
    default: throw new Error(name)
  }
}

/** Where a screen sits in the app: a Change screen has a Change open (an
 *  address with no project, so the crumb must take the snapshot's). */
function atOf(name) {
  const screen = SCREEN_KEY[name]
  return CHANGE.includes(name) || name === 'PR-Face' ? { screen, change: '0123456789abcdef' } : { screen, project: 'p1' }
}
/** The live linker, as screen.jsx's PortedScreen builds it: a Change screen
 *  hands it the project its snapshot names (`linkAt`). */
function liveLink(name) {
  const at = atOf(name)
  const where = typeof M.SC.linkAt === 'function' ? M.SC.linkAt(at, at.change ? M.SC.projectOf(snap()) : undefined) : at
  return R.linker(where)
}
/* Command-Ask is an overlay in the app (only the palette is drawn); `frame`
 * draws it with the canvas's frame around it, for the crumb and status bar. */
const liveHtml = (name, state, { frame = false } = {}) => {
  const props = liveProps(name)
  return M.render(name, { ...props, at: atOf(name), overlay: name === 'Command-Ask' && !frame ? true : undefined }, liveLink(name), state)
}

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
/** Every <a> in the markup: its href (null when absent) and its words. */
function anchors(html) {
  const out = []
  const re = /<a\b([^>]*)>([\s\S]*?)<\/a>/g
  let m
  while ((m = re.exec(html))) {
    const h = m[1].match(/\bhref="([^"]*)"/)
    out.push({ href: h ? unescape(h[1]) : null, attrs: m[1], text: unescape(m[2].replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ').trim() })
  }
  return out
}

/* What each canvas page is, in the app (the expectation, written out here,
 * not read from routes.js). `null` = a Change screen. */
const EXPECT = {
  Inbox: 'inbox', 'Change-Brief': 'brief', 'Every-Statement': 'statements', 'What-Changed': 'changed', 'Done-Criteria': 'done',
  'Repair-Loop': 'check', 'Approve-Sheet': 'approve', 'Whole-Story': 'story', 'PR-Face': 'pr', 'Record-Over-Time': 'record',
  Settings: 'settings', 'Team-View': 'team', Onboarding: 'start', 'Command-Ask': 'find', 'Plan-Licence': 'plan',
}
const CHANGE_KINDS = new Set(['brief', 'statements', 'changed', 'done', 'check', 'approve', 'story', 'pr'])

/** Every canvas page named anywhere under screens/: `link("…")` in templates, and `'X.dc.html'` in logic. */
function namedTargets() {
  const out = []
  for (const d of readdirSync(here, { withFileTypes: true }).filter((x) => x.isDirectory()).map((x) => x.name)) {
    for (const f of readdirSync(join(here, d)).filter((x) => /\.(jsx?|js)$/.test(x) && !x.endsWith('.test.js'))) {
      const src = read(d + '/' + f)
      const body = src.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '')
      const re = f === 'template.jsx' ? /link\("([^"]+)"\)/g : /['"]([A-Za-z0-9_-]+\.dc\.html(?:#[a-z]*)?)['"]/g
      let m
      while ((m = re.exec(body))) out.push({ where: d + '/' + f, target: m[1] })
    }
  }
  return out
}

/* ---------- 1. no Inbox fallback ---------- */

test('rule 1: every canvas page a template or logic file links to resolves to the screen it names, with a Change open', () => {
  const targets = namedTargets()
  assert.ok(targets.length > 150, 'the census found ' + targets.length + ' links')
  const at = { screen: 'brief', change: 'c1', project: 'p1' }
  const link = R.linker(at)
  const bad = []
  for (const { where, target } of targets) {
    const page = target.split('#')[0].replace(/\.dc\.html$/, '')
    const kind = EXPECT[page]
    const got = link(target)
    if (!kind) { bad.push(where + ': ' + target + ' is a page the app does not have'); continue }
    if (typeof got !== 'string' || !got.startsWith('#/')) { bad.push(where + ': ' + target + ' → ' + got); continue }
    const r = R.parse(got)
    const ok = kind === 'find' ? r.screen === 'brief' && r.change === 'c1' && r.find === true
      : kind === 'plan' ? r.screen === 'settings' && r.pane === 'plan'
        : r.screen === kind && (!CHANGE_KINDS.has(kind) || r.change === 'c1')
    if (!ok) bad.push(where + ': ' + target + ' → ' + got)
  }
  assert.deepEqual(bad, [])
})

test('rule 1: with no Change open, a link to a Change screen is no link, and nothing but the Inbox goes to the Inbox', () => {
  const link = R.linker({ screen: 'record', project: 'p1' })
  const bad = []
  for (const { where, target } of namedTargets()) {
    const page = target.split('#')[0].replace(/\.dc\.html$/, '')
    const got = link(target)
    if (page !== 'Inbox' && typeof got === 'string' && R.parse(got).screen === 'inbox') bad.push(where + ': ' + target + ' → ' + got)
    if (CHANGE_KINDS.has(EXPECT[page]) && got !== undefined) bad.push(where + ': ' + target + ' → ' + got + ' (no Change is open)')
  }
  assert.deepEqual(bad, [])
  assert.equal(link('Nowhere.dc.html'), undefined, 'a page the app does not have is no link')
  assert.equal(link('Settings.dc.html#nowhere'), undefined, 'a Settings pane that does not exist is no link')
  assert.equal(link('Inbox.dc.html'), '#/inbox?project=p1')
})

test('rule 1: a Find row with no Change behind it is no link, never the Inbox', () => {
  const data = M.commandFromChange(null, VIEW, { readAt: '10:30' })
  data.findable = data.findable.concat([{ id: 'n:1', kind: 'Note', text: 'a row that names no Change' }])
  const html = M.render('Command-Ask', { data, frame: {}, overlay: true, at: { screen: 'record', project: 'p1' } }, R.linker({ screen: 'record', project: 'p1' }), { query: 'row that names' })
  const row = anchors(html).find((a) => a.text.includes('a row that names no Change'))
  assert.ok(row, 'the row is drawn')
  assert.equal(row.href, null, 'got ' + row.href)
})

/* ---------- 3. no raw .dc.html, every live href real ---------- */

test('rule 3: no screen, live or in the gallery, renders a raw .dc.html href; every live href is an address, a place on the page, or external', () => {
  const bad = []
  for (const name of ALL) {
    const html = liveHtml(name)
    for (const a of anchors(html)) {
      if (a.href == null) continue
      if (a.href.includes('.dc.html')) { bad.push(name + ' (live): ' + a.href + ' “' + a.text.slice(0, 40) + '”'); continue }
      if (a.href.startsWith('#/')) continue
      if (/^https:\/\//.test(a.href)) continue
      if (/^#[A-Za-z][\w-]*$/.test(a.href) && html.includes('id="' + a.href.slice(1) + '"')) continue
      bad.push(name + ' (live): ' + JSON.stringify(a.href) + ' “' + a.text.slice(0, 40) + '”')
    }
    for (const a of anchors(M.renderDesign(name))) {
      if (a.href && a.href.includes('.dc.html')) bad.push(name + ' (gallery): ' + a.href)
    }
  }
  assert.deepEqual(bad, [])
})

test('rule 3: the plates on the Brief link through the router (What changed ›, What counts as done ›)', () => {
  const html = liveHtml('Change-Brief')
  const plate = anchors(html).filter((a) => /^(What changed|What counts as done) ›$/.test(a.text))
  assert.ok(plate.length >= 1, 'a plate link is drawn')
  for (const a of plate) assert.match(a.href || '', /^#\/changes\/0123456789abcdef\/(changed|done)\?project=/, a.text + ' → ' + a.href)
})

/* ---------- 2. Find, ask, act opens the palette ---------- */

test('rule 2: every Find, ask, act link and every search field opens the palette, and the page stays', () => {
  const bad = []
  for (const name of FINDS) {
    let opened = 0
    const props = { ...liveProps(name), at: atOf(name), openCommand: () => { opened += 1 } }
    const { el, unmount } = mount(name, props, liveLink(name))
    try {
      const finds = [...el.querySelectorAll('a')].filter((a) => /Find, ask, act|^Find a |Opens the palette/.test((a.getAttribute('aria-label') || '') + ' ' + a.textContent.trim()))
      const fields = [...el.querySelectorAll('input[type="search"]')]
      if (!finds.length) bad.push(name + ': no Find, ask, act link found')
      for (const node of [...finds, ...fields]) {
        const before = opened
        const ev = new dom.window.MouseEvent('click', { bubbles: true, cancelable: true })
        node.dispatchEvent(ev)
        if (opened !== before + 1) bad.push(name + ': clicking ' + (node.tagName === 'A' ? '“' + node.textContent.trim().slice(0, 30) + '”' : 'the search field') + ' did not open the palette')
        if (node.tagName === 'A' && !ev.defaultPrevented) bad.push(name + ': the Find link followed its href')
      }
      for (const field of fields) {
        const before = opened
        field.dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: 'a', bubbles: true, cancelable: true }))
        if (opened !== before + 1) bad.push(name + ': typing in the search field did not open the palette')
      }
    } finally {
      unmount()
    }
  }
  assert.deepEqual(bad, [])
})


test('rule 2: every screen that draws a Find link is handed openCommand by its live address', () => {
  for (const name of FINDS) {
    const idx = read(name + '/index.jsx')
    assert.match(idx, /export default function Screen\(\{[^}]*\bopenCommand\b[^}]*\}\)/, name + ': the screen does not take openCommand')
    assert.match(idx, /extra=\{\{[^}]*\bopenCommand\b/, name + ': openCommand is not passed to the ported screen')
  }
  assert.match(read('../app.jsx'), /<Screen at=\{at\} openCommand=\{/, 'the app hands every screen openCommand')
})

test('rule 2: a Find address opens the palette over the screen it names', () => {
  assert.deepEqual(R.parse('#/changes/c1/story?project=p1&find=1'), { screen: 'story', change: 'c1', project: 'p1', find: true })
  assert.equal(R.linker({ screen: 'story', change: 'c1', project: 'p1' })('Command-Ask.dc.html'), '#/changes/c1/story?project=p1&find=1')
  assert.match(read('../app.jsx'), /if \(!at\.find\) return\s+setCommandOpen\(true\)/)
})

/* ---------- 4. in-page links scroll, never route ---------- */

test('rule 4: a hash that is not #/… is a place on the page — the router keeps its route', () => {
  assert.equal(typeof R.nextRoute, 'function', 'routes.js has no nextRoute')
  const prev = R.parse('#/changes/c1/story?project=p1')
  for (const h of ['#ws-done', '#ws-outcome', '#ws-not-recorded', '#top']) assert.equal(R.nextRoute(prev, h), prev, h + ' moved the route')
  assert.deepEqual(R.nextRoute(prev, '#/inbox'), { screen: 'inbox', project: undefined })
  const app = read('../app.jsx')
  assert.match(app, /nextRoute\(prev, window\.location\.hash\)/, 'useRoute does not use nextRoute')
  assert.match(app, /document\.addEventListener\('click', followInPage\)/, 'the app does not scroll in-page links')
  assert.match(read('../artboards.jsx'), /document\.addEventListener\('click', followInPage\)/, 'the gallery does not scroll in-page links')
})

test('rule 4: clicking the whole story’s contents scrolls to the section and leaves the address alone', async () => {
  const P = existsSync(join(here, '../in-page.js')) ? await import('../in-page.js') : {}
  assert.equal(typeof P.followInPage, 'function', 'in-page.js has no followInPage')
  const html = liveHtml('Whole-Story')
  const toc = anchors(html).filter((a) => a.href && a.href.startsWith('#ws-'))
  assert.ok(toc.length >= 9, 'the contents are drawn: ' + toc.length)
  const w = new JSDOM('<!DOCTYPE html><body>' + html + '</body>', { url: 'http://127.0.0.1/#/changes/c1/story' }).window
  let scrolled = []
  w.HTMLElement.prototype.scrollIntoView = function () { scrolled.push(this.id) }
  w.document.addEventListener('click', P.followInPage)
  for (const a of [...w.document.querySelectorAll('a[href^="#ws-"]')]) {
    scrolled = []
    const ev = new w.MouseEvent('click', { bubbles: true, cancelable: true })
    a.dispatchEvent(ev)
    assert.ok(ev.defaultPrevented, a.getAttribute('href') + ' changed the address')
    assert.deepEqual(scrolled, [a.getAttribute('href').slice(1)])
    assert.equal(w.location.hash, '#/changes/c1/story')
  }
  // an address is followed as an address
  const addr = w.document.createElement('a')
  addr.setAttribute('href', '#/inbox')
  w.document.body.appendChild(addr)
  const ev = new w.MouseEvent('click', { bubbles: true, cancelable: true })
  addr.dispatchEvent(ev)
  assert.equal(ev.defaultPrevented, false)
})

/* ---------- 5. the project crumb ---------- */

test('rule 5: on every Change screen the project crumb links to that project’s Inbox', () => {
  const bad = []
  for (const name of CHANGE) {
    const html = liveHtml(name, undefined, { frame: true })
    const nav = html.slice(html.indexOf('<nav aria-label="Where you are"'), html.indexOf('</nav>', html.indexOf('<nav aria-label="Where you are"')))
    const first = anchors(nav + '</nav>')[0]
    if (!first) { bad.push(name + ': the crumb has no link'); continue }
    if (first.text !== snap().project.name) bad.push(name + ': the first crumb link is “' + first.text + '”')
    if (first.href !== '#/inbox?project=' + PID()) bad.push(name + ': the crumb goes to ' + first.href)
  }
  assert.deepEqual(bad, [])
  for (const name of CHANGE.concat(['PR-Face'])) {
    assert.match(read(name + '/index.jsx'), /project=\{/, name + ': the screen does not give its links the Change’s project')
  }
})

/* ---------- 6. Settings addresses name a pane ---------- */

const linkFor = (html, text) => anchors(html).filter((a) => a.text === text)

test('rule 6: the status bar’s “see what” opens Settings › Storage on every screen that draws it', () => {
  const bad = []
  for (const name of ALL.filter((n) => !['Settings', 'Onboarding', 'PR-Face'].includes(n))) {
    const html = liveHtml(name, undefined, { frame: true })
    const see = linkFor(html, 'see what')
    if (!see.length) { bad.push(name + ': no see what link'); continue }
    for (const a of see) {
      const r = a.href ? R.parse(a.href) : {}
      if (r.screen !== 'settings' || r.pane !== 'storage') bad.push(name + ': see what → ' + a.href)
    }
  }
  assert.deepEqual(bad, [])
})

test('rule 6: the hint bar’s “change” opens Settings › Keyboard', () => {
  const change = linkFor(liveHtml('Inbox'), 'change')
  assert.equal(change.length, 1)
  assert.deepEqual(R.parse(change[0].href), { screen: 'settings', project: 'p1', pane: 'keyboard' })
})

test('rule 6 (R-PLAN): the Plan chip, “Plan & licence”, “Plan and licence” and the plan pill open Settings › Plan & licence', () => {
  const bad = []
  const check = (where, list) => {
    if (!list.length) bad.push(where + ': not drawn')
    for (const a of list) {
      const r = a.href ? R.parse(a.href) : {}
      if (r.screen !== 'settings' || r.pane !== 'plan') bad.push(where + ': “' + a.text + '” → ' + a.href)
    }
  }
  // Approve-Sheet's status-bar plan chip says the plan word (Free, from the licence)
  check('Approve-Sheet plan chip', linkFor(liveHtml('Approve-Sheet'), 'Free'))
  const team = liveHtml('Team-View')
  check('Team-View “Plan and licence”', linkFor(team, 'Plan and licence'))
  check('Team-View plan pill', anchors(team).filter((a) => a.attrs.includes('aria-label="Plan:')))
  check('Settings “Plan & licence” nav', linkFor(liveHtml('Settings'), 'Plan: Free ›'))
  assert.deepEqual(bad, [])
  // every Plan-Licence link in every template ("Install a licence", "What Team includes today") goes to the pane
  const link = R.linker({ screen: 'approve', change: 'c1', project: 'p1' })
  assert.deepEqual(R.parse(link('Plan-Licence.dc.html')), { screen: 'settings', project: 'p1', pane: 'plan' })
})

test('rule 6: Settings opens on the pane its address names, and draws Plan & licence from the licence', () => {
  const props = liveProps('Settings')
  const crumbOf = (pane) => {
    const html = M.render('Settings', { ...props, at: { screen: 'settings', pane } }, R.linker({ screen: 'settings' }))
    const shown = [...html.matchAll(/<section aria-labelledby="(h-[a-z]+)" style="display:flex;order:1/g)].map((m) => m[1])
    return { html, shown }
  }
  assert.deepEqual(crumbOf('storage').shown, ['h-privacy'], 'storage')
  assert.deepEqual(crumbOf('keyboard').shown, ['h-keys'], 'keyboard')
  const plan = crumbOf('plan')
  assert.deepEqual(plan.shown, ['h-plan'], 'plan')
  const words = unescape(plan.html.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ')
  assert.ok(words.includes('Plan & licence'))
  assert.ok(words.includes('This machine is on the Free plan.'), 'the licence’s own words')
  assert.ok(words.includes('Everything works except approving a version.'))
  const plans = anchors(plan.html).find((a) => a.text === 'See plans ↗')
  assert.ok(plans && plans.href.startsWith('https://'), 'See plans ↗ is the pricing page')
  assert.doesNotMatch(words, /\btrial\b/i)
  // the address the app writes for each pane
  assert.equal(R.href('settings', { pane: 'storage' }), '#/settings?pane=storage')
  assert.deepEqual(R.parse('#/settings?pane=keyboard'), { screen: 'settings', project: undefined, pane: 'keyboard' })
})
