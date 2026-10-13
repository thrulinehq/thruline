/* C30 R-KEYS: one key table drives every key, the `?` map and Settings ›
 * Keyboard (On / Only with ⌥ / Off, kept through /api/preferences).
 *
 *  1. The table holds §2.9 plus every key each screen draws (the census rows,
 *     written out here by hand), resolves the c/x mismatch, and nothing else
 *     names a key.
 *  2. Every table key, on every live screen where it is drawn, in each of the
 *     three modes, does what the table says — or nothing, where it is not
 *     wired yet or needs a Change and none is open.
 *  3. No key acts inside a text field; ⌘K and Esc keep their old behaviour.
 *  4. `g` waits SEQUENCE_MS (1000 ms) for its second key, and no longer.
 *  5. On the live Inbox, Changes and Projects, j k ↵ . ⌘↵ y e do what a click
 *     on the same row does; h hides a current Inbox or Changes row.
 *  6. The map and Settings › Keyboard list exactly the table, generated.
 *  7. The map opens on `?`, traps focus, closes on Esc and gives focus back.
 *  8. The preference is read and written through /api/preferences; a failed
 *     read is the default and says so, a failed write is said in the pane.
 *  9. The design gallery gets no dispatcher, no map and no markup change.
 *
 * Synthetic fixtures only (made-up projects under /Users/someone, the Inbox
 * canvas's own demo contract). No server: every fetch is a stub. Each piece is
 * bundled on its own, so a module that does not exist fails only the tests
 * that need it. */
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
dom.window.requestAnimationFrame = (fn) => setTimeout(fn, 0)

const here = dirname(fileURLToPath(import.meta.url))
const read = (p) => readFileSync(join(here, p), 'utf8')
const tick = (ms = 20) => new Promise((r) => setTimeout(r, ms))

async function bundle(entry) {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: entry, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-keys-')), 'm.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}
const lazy = (entry) => { let p = null; return () => (p = p || bundle(entry)) }

const KEYS = lazy(`
export * from './table.js'
export * from './dispatch.js'
export * from './rows.js'
export * from './registry.js'
export * from './list.js'
export * as R from '../routes.js'
`)
const PREFS = lazy(`export * from '../data/preferences.js'`)

const UI = lazy(`
import React, { useState } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { createRoot } from 'react-dom/client'
import { flushSync } from 'react-dom'
import { DcScreen } from '../dc/runtime.jsx'
import { designLink } from '../artboards.jsx'
import { withFrame } from '../data/project.js'
import * as R from '../routes.js'
import { useKeys } from './use-keys.jsx'
import { KeysMap } from './overlay.jsx'
import { withKeys, currentKeys } from './registry.js'
import { KEY_TABLE, entriesFor, LIVE_SCREENS } from './table.js'
import { createPreferences } from '../data/preferences.js'
import InboxT from '../screens/Inbox/template.jsx'
import InboxL from '../screens/Inbox/logic.js'
import { DEMO as INBOX_DEMO } from '../screens/Inbox/demo.js'
import ChangesT from '../screens/Changes/template.jsx'
import ChangesL from '../screens/Changes/logic.js'
import { changesData } from '../screens/Changes/from-projects.js'
import ProjectsT from '../screens/Projects/template.jsx'
import ProjectsL from '../screens/Projects/logic.js'
import { projectsData } from '../screens/Projects/from-projects.js'
import SettingsT from '../screens/Settings/template.jsx'
import SettingsL from '../screens/Settings/logic.js'
import { settingsFromServer } from '../screens/Settings/from-server.js'
export { React, R, KEY_TABLE, entriesFor, LIVE_SCREENS, createPreferences, currentKeys, INBOX_DEMO, changesData, projectsData, settingsFromServer, renderToStaticMarkup, KeysMap }
export const S = { Inbox: [InboxT, InboxL], Changes: [ChangesT, ChangesL], Projects: [ProjectsT, ProjectsL], Settings: [SettingsT, SettingsL] }

/** One live screen with the app's keys over it, as app.jsx mounts them. */
export function mount({ name, props, link, at, store, now }) {
  const [T, L] = S[name]
  const Live = withKeys(withFrame(L))
  const out = { palette: false }
  function Harness() {
    const [palette, setPalette] = useState(false)
    out.palette = palette
    const keys = useKeys({ at, palette, setPalette, now, store })
    // a screen handed prefs: 'live' reads the store as Settings' own index does
    const p = Object.assign({}, props, { keysMode: keys.mode }, props.prefs === 'live' ? { prefs: store.get() } : {})
    return React.createElement(React.Fragment, null,
      React.createElement(DcScreen, { Logic: Live, Template: T, props: p, link }),
      palette ? React.createElement('div', { id: 'palette-open' }) : null,
      keys.map)
  }
  const el = document.createElement('div')
  document.body.appendChild(el)
  const root = createRoot(el)
  flushSync(() => root.render(React.createElement(Harness)))
  return { el, out, unmount: () => { root.unmount(); el.remove() } }
}

/** A screen as the design gallery draws it. */
export function design(name) {
  const [T, L] = S[name]
  return renderToStaticMarkup(React.createElement(DcScreen, { Logic: L, Template: T, props: {}, link: designLink }))
}
export function designMount(name) {
  const [T, L] = S[name]
  const el = document.createElement('div')
  document.body.appendChild(el)
  const root = createRoot(el)
  flushSync(() => root.render(React.createElement(DcScreen, { Logic: L, Template: T, props: {}, link: designLink })))
  return { el, unmount: () => { root.unmount(); el.remove() } }
}
/** Settings on its live path, at a pane, as static markup. */
export function settingsHtml({ data, prefs, nav = 'keyboard', actions = {} }) {
  const [T, L] = S.Settings
  const C = withFrame(L)
  const inst = new C({ data, prefs, actions, frame: {}, at: { screen: 'settings' } })
  inst.state = Object.assign({}, inst.state, { nav })
  return renderToStaticMarkup(React.createElement(T, { v: inst.renderVals(), link: (h) => h }))
}
/** The map, drawn on its own. */
export function mapHtml(props) {
  return renderToStaticMarkup(React.createElement(KeysMap, Object.assign({ onClose: () => {} }, props)))
}
`)

/* ---------------- fixtures (synthetic) ---------------- */

const HOME = '/Users/someone'
const NOW = new Date(2026, 9, 7, 16, 10)
const at = (h, m, day = 7) => new Date(2026, 9, day, h, m).toISOString()
const STORAGE = { bytes: 5.4e9, path: HOME + '/.thruline' }
// C46 §1.1: each project's readiness, read, with nothing ready (Changes draws a block once both its view and this have arrived)
const READ = (id) => ({ project: id, ready: {}, moves: {}, failed: 0, listFailed: false })
const READINGS = { p1: READ('p1'), p2: READ('p2') }
const LICENCE_FREE = { status: 'absent', paid_gate: true, entitlements: { approve_version: { allowed: false, status: 'absent' } }, technical: {} }
const MODES = ['on', 'mod', 'off']

/** The Inbox canvas's own demo contract, with the addresses a live row carries. */
function inboxData(M) {
  const D = JSON.parse(JSON.stringify(M.INBOX_DEMO))
  D.changes.post.href = '#/changes/post?project=p1'
  D.changes.post.approveHref = '#/changes/post/approve?project=p1'
  D.changes.disc.href = '#/changes/disc?project=p1'
  D.changes.disc.checkHref = '#/changes/disc/check?project=p1'
  D.changes.stripe.href = '#/changes/stripe?project=p1'
  D.prompts[0].href = '#/changes/disc?project=p1&step=60'
  D.sessions[0].href = '#/changes/disc?project=p1'
  return D
}

/* the Changes and Projects fixtures: two projects with threads, one with none */
const tests0 = { total: 0, succeeded: 0, failed: 0, unknown: 0 }
const thread = (id, over) => Object.assign({
  id, title: 'Thread ' + id, title_basis: 'instruction', started_at: at(15, 0), last_activity_at: at(15, 30),
  sessions: 1, prompts: 1, conversation: 0, edits: 0, files: 0, top_files: [], commands: tests0, tests: tests0, commits: 0, claims: 1,
  whose_move: 'done', whose_move_basis: 'nothing_open',
  proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: [] }, current_version_id: '3f9a1c2deadbeef',
}, over)
const SHOP = { id: 'p1', name: 'shop', root: HOME + '/code/shop', complete: true, threads: 3, sessions: 9, claims: 7, sources: ['claude_code'], last_activity: at(16, 5), whose_move: { you: 2, agent: 0, done: 1 } }
const NOTES = { id: 'p2', name: 'notes', root: HOME + '/code/notes', complete: true, threads: 1, sessions: 3, claims: 1, sources: ['codex'], last_activity: at(9, 0, 6), whose_move: { you: 1, agent: 0, done: 0 } }
const EMPTY = { id: 'p3', name: 'empty', root: HOME + '/code/empty', complete: true, threads: 0, sessions: 0, sources: [], last_activity: null, whose_move: { you: 0, agent: 0, done: 0 } }
const LIST = { complete: true, projects: [SHOP, NOTES, EMPTY] }
const recheck = (id, title) => thread(id, { title, whose_move: 'you', whose_move_basis: 'claim_without_passing_test',
  proof: { last_test: { outcome: 'succeeded' }, last_passing_test_at: at(15, 0), edited_after_last_pass: [HOME + '/code/shop/checkout/api.py'] } })
const VIEW_SHOP = { project: SHOP, threads: [recheck('re1', 'Fix the checkout total'), recheck('re2', 'Fix the tax line'), thread('done1', { title: 'Shop finished work', last_activity_at: at(9, 0, 6) })] }
const VIEW_NOTES = { project: NOTES, threads: [thread('np1', { title: 'Notes not proven', whose_move: 'you', whose_move_basis: 'claim_without_passing_test' })] }

/** A preferences store answering from a fake server. */
function fakeServer(M, { keys = 'on', get = 'ok', post = 'ok', fromFile } = {}) {
  const calls = []
  const doc = (k) => ({ schema_version: 1, preferences: { face: 'plain', density: 'comfortable', motion: 'system', colour_vision: 'standard', keys: k, masked_copy: true, notifications: { opposite: true, moved: true, finished: true }, weekly_summary: false }, defaults: {}, path: HOME + '/.thruline/preferences.json', file: 'read', from_file: fromFile || ['keys'], unknown_keys: [], problems: [] })
  let held = keys
  const fetchImpl = async (url, opts = {}) => {
    calls.push({ url, method: opts.method || 'GET', body: opts.body, headers: opts.headers })
    if ((opts.method || 'GET') === 'GET') {
      if (get === 'down') throw new Error('connection refused')
      if (get === '500') return { ok: false, status: 500, json: async () => ({}) }
      return { ok: true, status: 200, json: async () => doc(held) }
    }
    if (post === 'down') throw new Error('connection refused')
    if (post === 'newer') return { ok: false, status: 409, json: async () => ({ error: 'preferences_file_newer' }) }
    if (post === 'symlink') return { ok: false, status: 409, json: async () => ({ error: 'preferences_file_not_regular' }) }
    held = JSON.parse(opts.body).keys
    return { ok: true, status: 200, json: async () => doc(held) }
  }
  return { store: M.createPreferences({ fetchImpl }), calls }
}

/** One keydown, in a mode's own way of pressing a single key. */
function press(target, key, { alt = false, meta = false, shift = false, code } = {}) {
  const e = new dom.window.KeyboardEvent('keydown', { key, code: code || codeOf(key), altKey: alt, metaKey: meta, shiftKey: shift, bubbles: true, cancelable: true })
  ;(target || document.body).dispatchEvent(e)
  return e
}
function codeOf(key) {
  if (/^[a-z]$/.test(key)) return 'Key' + key.toUpperCase()
  if (/^[0-9]$/.test(key)) return 'Digit' + key
  return { '.': 'Period', '/': 'Slash', '?': 'Slash', '~': 'Backquote', Enter: 'Enter', Escape: 'Escape', ArrowUp: 'ArrowUp', ArrowDown: 'ArrowDown' }[key] || ''
}
/* with ⌥ held a Mac types another character; the dispatcher must read the key's place */
const MAC_ALT = { j: '∆', k: '˚', g: '©', i: 'ˆ', '.': '≥', '/': '÷', '?': '¿', y: '¥', e: '´', '1': '¡', '2': '™', '3': '£', t: '†', d: '∂', w: '∑', c: 'ç', h: '˙', l: '¬', p: 'π', r: '®', s: 'ß', '~': '`' }
/** A fake keydown for the pure dispatcher. */
function ev(key, { alt = false, meta = false, repeat = false } = {}) {
  const shift = key === '?' || key === '~'
  return { key: alt && MAC_ALT[key] ? MAC_ALT[key] : key, code: codeOf(key), altKey: alt, metaKey: meta, ctrlKey: false, shiftKey: shift, repeat, isComposing: false, defaultPrevented: false }
}

/* ---------------- 1. the table ---------------- */

/* Every key each screen draws, from FEATURE-CENSUS-20261007.md's "keys" rows and
 * DESIGN-SPEC §2.9, written out by hand so the table is checked against them. */
const DRAWN = {
  Inbox: ['j', 'k', '↵', '.', 'y', '⌘↵', 'h', 'e', 'g i', '?', '⌘K'],
  'Change-Brief': ['.', 'y', 'w', 'g d', 'g w', '⌘↵', 'j', 'k', 'e', 'd', '~', '1', '2', '3', 't', '⌘K'],
  'Every-Statement': ['g l', '/', '.', 'y', '↵', 'j', 'k', '?', '⌘K'],
  'What-Changed': ['y', '.', '⌘↵', '?', '⌘K'],
  'Done-Criteria': ['g d', '?', '⌘K'],
  'Repair-Loop': ['c', '.', 'y', '⌘↵', 'Esc', '?', '⌘K'],
  'Approve-Sheet': ['Esc', '⌘↵', 'g w', '⌘K'],
  'Command-Ask': ['/', '.', 'y', '⌘↵', 'g d', 'g l', 'g w', 'g i', '~', '?', '⌘K', '↑', '↓', '↵', 'Esc'],
  'Record-Over-Time': ['g c', 'g p', 'g r', 'g s', '?', '⌘K'],
  Settings: ['t', 'g s', '?', '⌘K'],
  'Team-View': ['g t', '?', '⌘K'],
  'Whole-Story': ['g w', 'Esc', '⌘K'],
}
const SPEC_29 = ['⌘K', '?', 't', 'g i', 'g c', 'g p', 'g r', 'g s', 'g d', 'g w', '1', '2', '3', 'j', 'k', '↵', '.', '⌘↵', 'y', 'h', 'e', 'd', '~', 'w']

test('1. the table holds every key each screen draws and every key §2.9 names', async () => {
  const M = await KEYS()
  const missing = []
  for (const [screen, keys] of Object.entries(DRAWN)) {
    const { here, everywhere } = M.entriesFor(screen)
    const there = new Set([...here, ...everywhere].flatMap((e) => e.keys))
    for (const k of keys) if (!there.has(k)) missing.push(screen + ': ' + k)
  }
  const all = new Set(M.KEY_TABLE.flatMap((e) => e.keys))
  for (const k of SPEC_29) if (!all.has(k)) missing.push('§2.9: ' + k)
  assert.deepEqual(missing, [])
  // ids are unique, every entry is complete, every drawn key spells a token
  const ids = M.KEY_TABLE.map((e) => e.id)
  assert.equal(new Set(ids).size, ids.length)
  for (const e of M.KEY_TABLE) {
    assert.ok(e.label && Array.isArray(e.keys) && e.keys.length, e.id)
    assert.ok(['single', 'chord', 'named'].includes(e.kind), e.id)
    assert.ok(e.global || (Array.isArray(e.screens) && Array.isArray(e.wired)), e.id + ' says where it is drawn and wired')
    for (const k of e.keys) assert.ok(M.tokenOf(k).token || M.tokenOf(k).seq, k)
    if (e.wired) for (const s of e.wired) assert.ok(e.screens.includes(s), e.id + ' wired where it is not drawn: ' + s)
    if (!e.global && e.wired.length < e.screens.length) assert.ok(e.waiting, e.id + ' says what its unwired screens wait on')
  }
})

test('1. the c/x mismatch is resolved in the table: no x; c only where Repair-Loop draws it; g p, g r, 1 2 3 and ⌘↵ are in', async () => {
  const M = await KEYS()
  const where = (k) => M.KEY_TABLE.filter((e) => e.keys.includes(k))
  assert.deepEqual(where('x'), [], 'x is drawn on no screen')
  assert.deepEqual(where('c').map((e) => [e.id, e.screens]), [['check-half', ['Repair-Loop']]])
  assert.equal(where('.')[0].label, 'Check it again', '. is Check it again, as §2.9 says')
  for (const k of ['g p', 'g r', '1', '2', '3', '⌘↵']) assert.ok(where(k).length, k)
  assert.equal(M.SEQUENCE_MS, 1000)
})

test('1. nothing but the table names a key: the dispatcher, the map and Settings › Keyboard read it', () => {
  const app = read('../app.jsx')
  assert.doesNotMatch(app, /addEventListener\('keydown'/, 'app.jsx has no key listener of its own')
  assert.match(app, /useKeys\(\{ at, palette: commandOpen, setPalette: setCommandOpen \}\)/, '⌘K reaches the palette through the state openCommand sets')
  assert.match(app, /<Screen at=\{at\} openCommand=\{\(\) => setCommandOpen\(true\)\}/)
  const kb = read('../screens/Settings/keyboard.js') + read('../screens/Settings/live.js')
  for (const label of ['Next / previous in a list', 'Hide until it changes', 'What counts as done', 'Find, ask, act']) {
    assert.ok(!kb.includes("'" + label + "'"), 'Settings’ live path writes the label “' + label + '” by hand')
  }
  assert.match(read('../screens/Settings/keyboard.js'), /settingsRows\(mode\)/)
})

/* ---------------- 2. every key, every screen, every mode ---------------- */

const CHANGE_AT = { 'Change-Brief': 'brief', 'Every-Statement': 'statements', 'What-Changed': 'changed', 'Done-Criteria': 'done', 'Repair-Loop': 'check', 'Approve-Sheet': 'approve', 'Whole-Story': 'story', 'PR-Face': 'pr' }
const HOME_AT = { Inbox: 'inbox', Changes: 'changes', Projects: 'projects', 'Record-Over-Time': 'record', 'Team-View': 'team', Settings: 'settings', Onboarding: 'start' }
const routeOf = (screen) => (CHANGE_AT[screen] ? { screen: CHANGE_AT[screen], change: 'c1', project: 'p1' } : { screen: HOME_AT[screen], project: 'p1' })
/* where each navigating key goes from a Change screen (c1 in p1), written out by hand */
const GOES = {
  'g i': '#/inbox?project=p1', 'g c': '#/changes?project=p1', 'g p': '#/projects?project=p1', 'g r': '#/record?project=p1',
  'g s': '#/settings?project=p1', 'g t': '#/team?project=p1', 'g d': '#/changes/c1/done?project=p1', 'g w': '#/changes/c1/story?project=p1',
  'g l': '#/changes/c1/statements?project=p1', 1: '#/changes/c1?project=p1', 2: '#/changes/c1/statements?project=p1', 3: '#/changes/c1/changed?project=p1',
}

/** Press one drawn key through a fresh dispatcher; a `g` sequence is two presses. */
function pressDrawn(M, drawn, mode, ctx, { alt } = {}) {
  const d = M.createDispatcher({ now: () => 0 })
  const t = M.tokenOf(drawn)
  const useAlt = alt !== undefined ? alt : mode === 'mod'
  if (t.seq) {
    const first = d.decide(ev(t.seq[0], { alt: useAlt }), ctx)
    if (first.do !== 'wait') return first
    return d.decide(ev(t.seq[1]), ctx)
  }
  const key = { 'Mod+k': 'k', 'Mod+Enter': 'Enter' }[t.token] || t.token
  return d.decide(ev(key, { alt: t.token.startsWith('Mod+') ? false : useAlt, meta: t.token.startsWith('Mod+') }), ctx)
}

test('2. every table key, on every screen where it is drawn, in each mode, does what the table says — or nothing', async () => {
  const M = await KEYS()
  const SCREENS = [...M.LIVE_SCREENS]
  const bad = []
  let checked = 0
  for (const screen of SCREENS) {
    const r = routeOf(screen)
    const { here, everywhere } = M.entriesFor(screen)
    for (const mode of MODES) {
      for (const entry of [...here, ...everywhere]) {
        if (entry.by === 'palette') continue
        entry.keys.forEach((drawn, i) => {
          // Esc's "close" acts only on what is open (test 3); with nothing open, Esc is the screen's own (its row here)
          if (entry.id === 'close') return
          const act = M.actOf(entry, i)
          const calls = []
          const handlers = { [act]: () => { calls.push(act) } }
          const ctx = { mode, screen, at: r, palette: false, overlay: false, target: document.body, handlers }
          const got = pressDrawn(M, drawn, mode, ctx)
          checked++
          const tag = screen + ' · ' + mode + ' · ' + drawn + ' (' + entry.id + '): '
          // what the table says this key does here
          let want
          if (entry.kind === 'single' && mode === 'off') want = 'none'
          else if (!M.wiredOn(entry, screen)) want = 'none'
          else if (entry.change && !r.change) want = 'none'
          else if (entry.go) want = 'go'
          else if (entry.id === 'help') want = 'map'
          else if (entry.id === 'slash') want = 'find'
          else if (entry.id === 'find') want = 'palette'
          else if (entry.id === 'face') want = 'face'
          else want = 'call'
          if (got.do !== want) { bad.push(tag + 'did ' + got.do + ', the table says ' + want); return }
          if (want === 'go') {
            const goes = entry.id === 'back' ? '#/changes/c1?project=p1' : GOES[drawn]
            if (got.href !== goes) bad.push(tag + 'went to ' + got.href + ', not ' + goes)
          }
          if (want === 'call' && got.act !== act) bad.push(tag + 'called ' + got.act)
          // a single key in "Only with ⌥" does nothing without ⌥; in "On", nothing with ⌥
          if (entry.kind === 'single' && mode !== 'off') {
            const other = pressDrawn(M, drawn, mode, ctx, { alt: mode !== 'mod' })
            if (other.do !== 'none') bad.push(tag + (mode === 'mod' ? 'acted without ⌥' : 'acted with ⌥ held'))
          }
          if (entry.kind === 'single' && mode === 'off') {
            const withAlt = pressDrawn(M, drawn, mode, ctx, { alt: true })
            if (withAlt.do !== 'none') bad.push(tag + 'acted in Off with ⌥ held')
          }
        })
      }
    }
  }
  assert.deepEqual(bad, [])
  assert.ok(checked > 600, 'checked ' + checked)
})

test('2. Every Statement list keys act, unwired keys do nothing, and Change keys do nothing on a home screen', async () => {
  const M = await KEYS()
  const all = { next() {}, prev() {}, open: () => '#/x', primary: () => '#/x', approve: () => '#/x', copy() {}, expand() {}, hide() {}, then() {}, tech() {}, walk() {}, 'check-half'() {}, cancel() {} }
  for (const [screen, keys] of [['Every-Statement', ['j', 'k', '.', 'y', '↵']]]) {
    for (const k of keys) {
      const got = pressDrawn(M, k, 'on', { mode: 'on', screen, at: routeOf(screen), target: document.body, handlers: all })
      assert.equal(got.do, 'call', screen + ' ' + k)
    }
  }
  for (const k of ['⌘↵']) assert.equal(pressDrawn(M, k, 'on', { mode: 'on', screen: 'Approve-Sheet', at: routeOf('Approve-Sheet'), target: document.body, handlers: all }).do, 'none')
  for (const screen of ['Change-Brief', 'Approve-Sheet']) {
    assert.equal(M.createDispatcher().decide(ev('Escape'), { mode: 'off', screen, at: routeOf(screen), target: document.body, handlers: all }).act, 'cancel', screen + ' Esc closes the open sheet')
  }
  for (const [screen, keys] of [['Change-Brief', ['d', 'w', '~']], ['Repair-Loop', ['c']]]) {
    for (const key of keys) {
      const got = pressDrawn(M, key, 'on', { mode: 'on', screen, at: routeOf(screen), target: document.body, handlers: all })
      assert.equal(got.do, 'call', screen + ' ' + key)
    }
  }
  for (const screen of ['Inbox', 'Changes', 'Projects', 'Record-Over-Time', 'Team-View', 'Settings', 'Onboarding']) {
    for (const k of ['g d', 'g w', 'g l', '1', '2', '3']) {
      assert.equal(pressDrawn(M, k, 'on', { mode: 'on', screen, at: routeOf(screen), target: document.body, handlers: all }).do, 'none', screen + ' ' + k)
    }
  }
  // Esc is "back to the Change" where the canvas draws it, from the address alone
  for (const screen of ['Repair-Loop', 'Whole-Story']) {
    const got = M.createDispatcher().decide(ev('Escape'), { mode: 'off', screen, at: routeOf(screen), target: document.body, handlers: {} })
    assert.deepEqual([got.do, got.href], ['go', '#/changes/c1?project=p1'], screen)
  }
  // t switches the stored face, on every live screen, with no handler of the screen's own (C30 R-PREFS)
  assert.equal(pressDrawn(M, 't', 'on', { mode: 'on', screen: 'Settings', at: routeOf('Settings'), target: document.body, handlers: {} }).do, 'face')
})

/* ---------------- 3. text fields, ⌘K, Esc ---------------- */

test('3. no key acts inside a text field; ⌘K and Esc still reach the palette from one', async () => {
  const M = await KEYS()
  const fields = [document.createElement('input'), document.createElement('textarea'), document.createElement('select')]
  const search = document.createElement('input'); search.setAttribute('type', 'search'); fields.push(search)
  const editable = document.createElement('div'); editable.setAttribute('contenteditable', 'true')
  const inside = document.createElement('span'); editable.appendChild(inside); fields.push(inside)
  const all = { next() {}, prev() {}, open: () => '#/x', primary: () => '#/x', approve: () => '#/x', copy() {}, expand() {} }
  for (const target of fields) {
    for (const mode of MODES) {
      for (const e of M.KEY_TABLE) {
        if (e.id === 'find' || e.id === 'close' || e.by) continue
        for (const k of e.keys) {
          // a sequence's first key must not start one either
          const first = M.tokenOf(k).seq ? M.tokenOf(k).seq[0] : null
          const got = first
            ? M.createDispatcher().decide(ev(first, { alt: mode === 'mod' }), { mode, screen: 'Inbox', at: routeOf('Inbox'), target, handlers: all })
            : pressDrawn(M, k, mode, { mode, screen: 'Inbox', at: routeOf('Inbox'), target, handlers: all })
          assert.equal(got.do, 'none', target.tagName + ' ' + mode + ' ' + k)
        }
      }
    }
    const d = M.createDispatcher()
    assert.equal(d.decide(ev('k', { meta: true }), { mode: 'on', screen: 'Inbox', at: routeOf('Inbox'), target, handlers: all }).do, 'palette', '⌘K from a field')
    assert.equal(d.decide(ev('Escape'), { mode: 'on', screen: 'Inbox', at: routeOf('Inbox'), palette: true, target, handlers: all }).do, 'close-palette')
  }
  assert.equal(M.isTextField(Object.assign(document.createElement('input'), { type: 'checkbox' })), false)
  const note = document.createElement('textarea')
  assert.equal(M.createDispatcher().decide(ev('Escape'), { mode: 'off', screen: 'Approve-Sheet', at: routeOf('Approve-Sheet'), target: note, handlers: { cancel() {} } }).act, 'cancel', 'Esc closes a sheet from its note field')
})

test('3. ⌘K toggles Find, ask, act in every mode; while the palette or the map is open nothing else acts', async () => {
  const M = await KEYS()
  const d = M.createDispatcher()
  const ctx = (o) => Object.assign({ mode: 'off', screen: 'Inbox', at: routeOf('Inbox'), target: document.body, handlers: { next() {} } }, o)
  for (const mode of MODES) assert.equal(d.decide(ev('k', { meta: true }), ctx({ mode })).do, 'palette', mode)
  assert.equal(d.decide(Object.assign(ev('k'), { ctrlKey: true }), ctx()).do, 'palette', 'Ctrl+K')
  assert.equal(d.decide(ev('j'), ctx({ mode: 'on', palette: true })).do, 'none')
  assert.equal(d.decide(ev('Escape'), ctx({ palette: true })).do, 'close-palette')
  assert.equal(d.decide(ev('Escape'), ctx({ overlay: true })).do, 'close-map')
  assert.equal(d.decide(ev('j'), ctx({ mode: 'on', overlay: true })).do, 'none')
  assert.equal(d.decide(ev('?'), ctx({ mode: 'on', overlay: true })).do, 'close-map')
  assert.equal(d.decide(Object.assign(ev('j'), { defaultPrevented: true }), ctx({ mode: 'on' })).do, 'none', 'a key the page handled is left alone')
  // ↵ and ⌘↵ belong to a focused link or button; a list row's select button is not one
  const a = document.createElement('a'); a.setAttribute('href', '#/x')
  const row = document.createElement('button'); row.className = 'tl-row'
  const rowLink = document.createElement('a'); rowLink.className = 'tl-row'; rowLink.setAttribute('href', '#/y')
  assert.equal(d.decide(ev('Enter'), ctx({ target: a, handlers: { open: () => '#/z' } })).do, 'none')
  assert.equal(d.decide(ev('Enter'), ctx({ target: rowLink, handlers: { open: () => '#/z' } })).do, 'none')
  assert.equal(d.decide(ev('Enter'), ctx({ target: row, handlers: { open: () => '#/z' } })).do, 'call')
  assert.equal(d.decide(ev('Enter', { meta: true }), ctx({ target: row, handlers: { approve: () => '#/z' } })).do, 'call')
  // a held key repeats only j and k
  assert.equal(d.decide(ev('j', { repeat: true }), ctx({ mode: 'on' })).do, 'call')
  assert.equal(d.decide(ev('?', { repeat: true }), ctx({ mode: 'on' })).do, 'none')
})

/* ---------------- 4. g timing ---------------- */

test('4. g waits 1000 ms for its second key and no longer; Esc or any other key ends the wait', async () => {
  const M = await KEYS()
  let t = 0
  const d = M.createDispatcher({ now: () => t })
  const ctx = (mode = 'on') => ({ mode, screen: 'Inbox', at: routeOf('Inbox'), target: document.body, handlers: {} })
  t = 0; assert.equal(d.decide(ev('g'), ctx()).do, 'wait'); t = 1000
  assert.deepEqual(d.decide(ev('i'), ctx()), { do: 'go', entry: M.KEY_TABLE.find((e) => e.id === 'go-inbox'), href: '#/inbox?project=p1' })
  t = 5000; d.decide(ev('g'), ctx()); t = 6001
  assert.equal(d.decide(ev('i'), ctx()).do, 'none', 'i after 1001 ms is not g i')
  assert.equal(d.pending(), false)
  t = 0; d.decide(ev('g'), ctx()); assert.equal(d.pending(), true); d.decide(ev('Escape'), ctx())
  assert.equal(d.pending(), false, 'Esc ends the wait')
  t = 0; d.decide(ev('g'), ctx()); assert.equal(d.decide(ev('q'), ctx()).do, 'none'); assert.equal(d.decide(ev('i'), ctx()).do, 'none', 'an unknown second key ends it')
  // "Only with ⌥": ⌥g, then the second key with or without ⌥; g alone is typing
  t = 0; assert.equal(d.decide(ev('g', { alt: true }), ctx('mod')).do, 'wait'); assert.equal(d.decide(ev('c'), ctx('mod')).href, '#/changes?project=p1')
  t = 0; d.decide(ev('g', { alt: true }), ctx('mod')); assert.equal(d.decide(ev('p', { alt: true }), ctx('mod')).href, '#/projects?project=p1')
  t = 0; assert.equal(d.decide(ev('g'), ctx('mod')).do, 'none'); assert.equal(d.decide(ev('c'), ctx('mod')).do, 'none')
  t = 0; assert.equal(d.decide(ev('g'), ctx('off')).do, 'none')
})

/* A key held down repeats (event.repeat). Only j and k are meant to repeat; a held
 * ? or g must neither undo what its first keydown did nor end the wait it began
 * (C30 R-KEYS; census key:repeat-help and key:repeat-sequence, found by holding
 * the key through two real keydowns). */
test('4b. a held ? leaves the map its first keydown opened, in every mode that lets ? act', async () => {
  const M = await KEYS()
  const ctx = (o) => Object.assign({ mode: 'on', screen: 'Inbox', at: routeOf('Inbox'), target: document.body, handlers: {} }, o)
  for (const [mode, alt] of [['on', false], ['mod', true]]) {
    const d = M.createDispatcher()
    assert.equal(d.decide(ev('?', { alt }), ctx({ mode })).do, 'map', mode + ': the first ? opens the map')
    assert.equal(d.decide(ev('?', { alt, repeat: true }), ctx({ mode, overlay: true })).do, 'none', mode + ': a repeated ? does not close it')
    assert.equal(d.decide(ev('?', { alt }), ctx({ mode, overlay: true })).do, 'close-map', mode + ': a fresh ? still closes it')
  }
})

test('4c. a held g keeps the wait its first keydown began, and does not extend it', async () => {
  const M = await KEYS()
  let t = 0
  const d = M.createDispatcher({ now: () => t })
  const ctx = (mode = 'on') => ({ mode, screen: 'Inbox', at: routeOf('Inbox'), target: document.body, handlers: {} })
  for (const [mode, alt] of [['on', false], ['mod', true]]) {
    t = 0; assert.equal(d.decide(ev('g', { alt }), ctx(mode)).do, 'wait', mode + ': g waits')
    t = 40; assert.equal(d.decide(ev('g', { alt, repeat: true }), ctx(mode)).do, 'wait', mode + ': a repeated g is still the same wait')
    assert.equal(d.pending(), true)
    t = 80
    assert.deepEqual(d.decide(ev('c', { alt }), ctx(mode)), { do: 'go', entry: M.KEY_TABLE.find((e) => e.id === 'go-changes'), href: '#/changes?project=p1' }, mode + ': c after a held g reaches Changes')
  }
  // the window is counted from the first g: a repeat at 600 ms does not buy another 1000 ms
  t = 0; d.decide(ev('g'), ctx()); t = 600; d.decide(ev('g', { repeat: true }), ctx()); t = 1001
  assert.equal(d.decide(ev('c'), ctx()).do, 'none', 'c at 1001 ms is not g c')
  // and a repeated g with no wait pending starts none (a held g is not a sequence)
  t = 5000; assert.equal(d.decide(ev('g', { repeat: true }), ctx()).do, 'none')
  assert.equal(d.pending(), false)
})

/* ---------------- 5. the live lists ---------------- */

test('5. Inbox: j k move the selection a click makes; ↵ . ⌘↵ y follow the selected row’s own links; e opens Nothing needed; h needs a row address', async () => {
  const U = await UI()
  window.location.hash = '#/inbox?project=p1'
  const { store } = fakeServer(U, { keys: 'on' })
  await store.load()
  const link = U.R.linker({ screen: 'inbox', project: 'p1' })
  const m = U.mount({ name: 'Inbox', props: { data: inboxData(U), frame: {}, at: { screen: 'inbox', project: 'p1' } }, link, at: { screen: 'inbox', project: 'p1' }, store })
  const sel = () => [...m.el.querySelectorAll('button.tl-row[aria-current="true"]')].map((b) => b.getAttribute('aria-label').split('.')[0])
  try {
    assert.deepEqual(sel(), ['Fix double-applied discount on checkout total'], 'starts on the first row to act on')
    // . and y on a check row: its Check it again and Tell the agent, one address
    press(null, '.'); await tick(); assert.equal(window.location.hash, '#/changes/disc/check?project=p1')
    window.location.hash = '#/inbox?project=p1'
    press(null, 'y'); await tick(); assert.equal(window.location.hash, '#/changes/disc/check?project=p1')
    window.location.hash = '#/inbox?project=p1'
    press(null, 'Enter'); await tick(); assert.equal(window.location.hash, '#/changes/disc?project=p1')
    window.location.hash = '#/inbox?project=p1'
    // ⌘↵ on a check row has nothing to approve
    const e0 = press(null, 'Enter', { meta: true }); await tick()
    assert.equal(e0.defaultPrevented, false); assert.equal(window.location.hash, '#/inbox?project=p1')
    // j k walk the drawn order: Ready for approval, your turn, needs a re-check, the agent's, recording
    press(null, 'k'); await tick(); assert.match(sel()[0], /^Your turn/)
    press(null, 'k'); await tick(); assert.deepEqual(sel(), ['Add postcode validation to the address form'])
    press(null, 'k'); await tick(); assert.deepEqual(sel(), ['Add postcode validation to the address form'], 'k at the top stays')
    press(null, 'Enter', { meta: true }); await tick(); assert.equal(window.location.hash, '#/changes/post/approve?project=p1')
    window.location.hash = '#/inbox?project=p1'
    const e1 = press(null, '.'); await tick(); assert.equal(e1.defaultPrevented, false, '. on an approve row has no Check it again')
    for (let i = 0; i < 3; i++) { press(null, 'j'); await tick() }
    assert.deepEqual(sel(), ['Upgrade Stripe SDK to v14'])
    // y on the agent's row is its Copy it again
    press(null, 'y'); await tick()
    assert.ok(m.el.textContent.includes('You copied the note again at 16:10'), 'Copy it again ran')
    press(null, 'j'); await tick(); assert.match(sel()[0], /session/i)
    press(null, 'j'); await tick(); assert.match(sel()[0], /session/i, 'j at the bottom stays')
    // e opens and closes Nothing needed
    const nn = () => [...m.el.querySelectorAll('button[aria-expanded]')].find((b) => b.textContent.includes('Nothing needed')).getAttribute('aria-expanded')
    assert.equal(nn(), 'false'); press(null, 'e'); await tick(); assert.equal(nn(), 'true'); press(null, 'e'); await tick(); assert.equal(nn(), 'false')
    // the canvas fixture has no server row address, so h has no act to send
    const before = m.el.innerHTML
    const h = press(null, 'h'); await tick()
    assert.equal(h.defaultPrevented, false); assert.equal(m.el.innerHTML, before)
    // keys inside a field do nothing
    const input = document.createElement('input'); document.body.appendChild(input); input.focus()
    press(input, 'k'); press(input, 'e'); press(input, 'Enter'); await tick()
    assert.match(sel()[0], /session/i); assert.equal(nn(), 'false'); assert.equal(window.location.hash, '#/inbox?project=p1')
    input.remove()
  } finally {
    m.unmount()
  }
})

test('5. h sends the selected served row in the Inbox and Changes', async () => {
  const U = await UI()
  const { store } = fakeServer(U, { keys: 'on' })
  await store.load()
  const addr = (thread_id) => ({ thread_id, row_kind: 'thread', row_sha256: 'a'.repeat(64), hidden: false })
  const calls = []
  const inbox = inboxData(U)
  inbox.hideRows = { disc: addr('disc') }
  const atInbox = { screen: 'inbox', project: 'p1' }
  const first = U.mount({ name: 'Inbox', props: { data: inbox, frame: {}, at: atInbox, hideRow: (row) => calls.push(['p1', row]) }, link: U.R.linker(atInbox), at: atInbox, store })
  try {
    press(null, 'h'); await tick()
    assert.deepEqual(calls, [['p1', inbox.hideRows.disc]])
  } finally { first.unmount() }

  const hides = { p1: { project: 'p1', rows: [addr('re1'), addr('re2')] } }
  const data = U.changesData({ projects: LIST, views: { p1: VIEW_SHOP, p2: VIEW_NOTES }, readings: READINGS, hides, storage: STORAGE, now: NOW, current: 'p1' })
  const atChanges = { screen: 'changes', project: 'p1' }
  const second = U.mount({ name: 'Changes', props: { data, frame: {}, at: atChanges, hideRow: (project, row) => calls.push([project, row]) }, link: U.R.linker(atChanges), at: atChanges, store })
  try {
    press(null, 'h'); await tick()
    assert.equal(calls.length, 2)
    assert.equal(calls[1][0], 'p1')
    assert.deepEqual(calls[1][1], data.blocks[0].data.hideRows.re1)
  } finally { second.unmount() }
})

test('5. Inbox, in each mode: the hint bar says the mode; ⌥ is needed in “Only with ⌥”; Off leaves ↵ and ⌘↵', async () => {
  const U = await UI()
  for (const [mode, hint] of [['on', 'Single keys on · change'], ['mod', 'Single keys need ⌥ · change'], ['off', 'Single keys off · change']]) {
    window.location.hash = '#/inbox?project=p1'
    const { store } = fakeServer(U, { keys: mode })
    await store.load()
    const m = U.mount({ name: 'Inbox', props: { data: inboxData(U), frame: {}, at: { screen: 'inbox', project: 'p1' } }, link: U.R.linker({ screen: 'inbox', project: 'p1' }), at: { screen: 'inbox', project: 'p1' }, store })
    const sel = () => m.el.querySelector('button.tl-row[aria-current="true"]').getAttribute('aria-label').split('.')[0]
    try {
      assert.ok(m.el.textContent.includes(hint), mode + ': ' + hint)
      press(null, 'j'); await tick()
      assert.equal(sel(), mode === 'on' ? 'Upgrade Stripe SDK to v14' : 'Fix double-applied discount on checkout total', mode + ' j')
      press(null, 'j', { alt: true, code: 'KeyJ' }); await tick()
      // ⌥j types ∆ on a Mac: the key's place is what counts
      const e = new dom.window.KeyboardEvent('keydown', { key: '∆', code: 'KeyJ', altKey: true, bubbles: true, cancelable: true })
      document.body.dispatchEvent(e); await tick()
      if (mode === 'mod') assert.match(sel(), /session/i, 'mod: ⌥j twice moved twice')
      if (mode === 'off') assert.equal(sel(), 'Fix double-applied discount on checkout total', 'off: no single key moves')
      press(null, 'Enter'); await tick()
      assert.notEqual(window.location.hash, '#/inbox?project=p1', mode + ': ↵ opens in every mode')
    } finally {
      m.unmount()
    }
  }
})

test('5. Changes: j k walk every project’s rows, ↵ . y follow the selected row, e opens Nothing needed in that row’s project', async () => {
  const U = await UI()
  window.location.hash = '#/changes?project=p1'
  const { store } = fakeServer(U, { keys: 'on' })
  await store.load()
  const data = U.changesData({ projects: LIST, views: { p1: VIEW_SHOP, p2: VIEW_NOTES }, readings: READINGS, storage: STORAGE, now: NOW, current: 'p1' })
  const where = { screen: 'changes', project: 'p1' }
  const m = U.mount({ name: 'Changes', props: { data, frame: {}, at: where }, link: U.R.linker(where), at: where, store })
  const titles = () => [...m.el.querySelectorAll('button.tl-row[aria-current]')].map((b) => b.getAttribute('aria-label').split('.')[0])
  const sel = () => m.el.querySelector('button.tl-row[aria-current="true"]').getAttribute('aria-label').split('.')[0]
  try {
    assert.ok(m.el.textContent.includes('Single keys on · change'))
    const order = titles()
    assert.equal(order.length, 3)
    assert.equal(order[2], 'Notes not proven', 'notes’ row is drawn after shop’s')
    assert.equal(sel(), order[0])
    press(null, 'j'); await tick(); assert.equal(sel(), order[1])
    press(null, 'j'); await tick(); assert.equal(sel(), 'Notes not proven', 'j crosses into the next project')
    press(null, '.'); await tick(); assert.equal(window.location.hash, '#/changes/np1/check?project=p2')
    window.location.hash = '#/changes?project=p1'
    press(null, 'y'); await tick(); assert.equal(window.location.hash, '#/changes/np1/check?project=p2')
    window.location.hash = '#/changes?project=p1'
    press(null, 'Enter'); await tick(); assert.match(window.location.hash, /^#\/changes\/np1(\?|$)/)
    window.location.hash = '#/changes?project=p1'
    press(null, 'k'); await tick(); assert.equal(sel(), order[1])
    // e: Nothing needed in shop, the selected row's project
    const shop = m.el.querySelector('section[aria-label="shop"]')
    const exp = () => [...shop.querySelectorAll('button[aria-expanded]')].find((b) => b.textContent.includes('Nothing needed')).getAttribute('aria-expanded')
    assert.equal(exp(), 'false'); press(null, 'e'); await tick(); assert.equal(exp(), 'true')
    const h = press(null, 'h'); assert.equal(h.defaultPrevented, false)
  } finally {
    m.unmount()
  }
})

test('5. Projects: j k move focus along the rows; ↵ on a focused row is the link’s own; ↵ with none focused opens the project you are in', async () => {
  const U = await UI()
  window.location.hash = '#/projects?project=p2'
  const { store } = fakeServer(U, { keys: 'on' })
  await store.load()
  const data = U.projectsData({ projects: LIST, storage: STORAGE, now: NOW, current: 'p2' })
  const where = { screen: 'projects', project: 'p2' }
  const m = U.mount({ name: 'Projects', props: { data, frame: {}, at: where }, link: U.R.linker(where), at: where, store })
  const rows = () => [...m.el.querySelectorAll('div[role="list"][aria-label="Projects"] > [role="listitem"]')]
  try {
    assert.equal(rows().length, 3)
    const cur = rows().findIndex((a) => a.getAttribute('aria-current') === 'true')
    assert.equal(rows()[cur].getAttribute('href'), '#/inbox?project=p2')
    document.body.focus()
    press(null, 'Enter'); await tick(); assert.equal(window.location.hash, '#/inbox?project=p2', '↵ opens the project you are in')
    window.location.hash = '#/projects?project=p2'
    press(null, 'j'); await tick()
    const after = Math.min(cur + 1, 2)
    assert.equal(document.activeElement, rows()[after], 'j moves on from the row you are in')
    press(document.activeElement, 'k'); await tick(); assert.equal(document.activeElement, rows()[after - 1])
    for (let i = 0; i < 3; i++) { press(document.activeElement, 'k'); await tick() }
    assert.equal(document.activeElement, rows()[0], 'k stops at the first row')
    const e = press(document.activeElement, 'Enter'); assert.equal(e.defaultPrevented, false, 'the focused link keeps its ↵')
    press(document.activeElement, '.'); press(document.activeElement, 'y'); await tick()
    assert.equal(window.location.hash, '#/projects?project=p2', '. and y mean nothing on Projects')
  } finally {
    m.unmount()
  }
})

test('5. each list screen registers exactly the handlers the table says are wired on it, and the gallery registers none', async () => {
  const U = await UI()
  const M = await KEYS()
  const { store } = fakeServer(U, { keys: 'on' })
  await store.load()
  const cases = [
    ['Inbox', { data: inboxData(U) }, { screen: 'inbox', project: 'p1' }],
    ['Changes', { data: U.changesData({ projects: LIST, views: { p1: VIEW_SHOP, p2: VIEW_NOTES }, readings: READINGS, storage: STORAGE, now: NOW, current: 'p1' }) }, { screen: 'changes', project: 'p1' }],
    ['Projects', { data: U.projectsData({ projects: LIST, storage: STORAGE, now: NOW, current: 'p1' }) }, { screen: 'projects', project: 'p1' }],
  ]
  for (const [name, props, where] of cases) {
    const m = U.mount({ name, props: Object.assign({ frame: {}, at: where }, props), link: U.R.linker(where), at: where, store })
    try {
      assert.deepEqual(Object.keys(U.currentKeys()).sort(), M.handlersWired(name), name)
    } finally {
      m.unmount()
    }
    assert.deepEqual(Object.keys(U.currentKeys()), [], name + ': unmounting releases its keys')
  }
  assert.deepEqual(M.handlersWired('Every-Statement'), ['copy', 'next', 'open', 'prev', 'primary'], 'Every-Statement registers its drawn list and check keys')
  assert.deepEqual(M.handlersWired('What-Changed'), ['approve', 'copy', 'primary'])
  assert.deepEqual(M.handlersWired('Change-Brief'), ['approve', 'cancel', 'copy', 'expand', 'next', 'prev', 'primary', 'tech', 'then', 'walk'])
  assert.deepEqual(M.handlersWired('Repair-Loop'), ['approve', 'check-half', 'copy', 'primary'])
  assert.deepEqual(M.handlersWired('Approve-Sheet'), ['cancel'])
})

/* ---------------- 6. the map and Settings list exactly the table ---------------- */

/** The rows of one keyboard-map table: [keys, label, state] each. */
const rowsOf = (html, label) => {
  const at0 = html.indexOf('role="table" aria-label="' + label + '"')
  if (at0 < 0) return null
  const ends = ['role="group"', '<p ', 'role="table"', '</section>'].map((m) => html.indexOf(m, at0 + 10)).filter((i) => i > 0)
  const seg = html.slice(at0, Math.min(...ends))
  // the state words keep their spaces from breaking (U+00A0); read here as words
  const un = (x) => x.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/\u00a0/g, ' ')
  return [...seg.matchAll(/<div role="row"[^>]*>[\s\S]*?<kbd[^>]*>([^<]*)<\/kbd>[\s\S]*?<span role="cell"[^>]*>([^<]*)<\/span>[\s\S]*?<span role="cell"[^>]*>([^<]*)<\/span>/g)].map((x) => [un(x[1]), un(x[2]), un(x[3])])
}

test('6. Settings › Keyboard lists exactly the table, in its order, in each mode', async () => {
  const U = await UI()
  const M = await KEYS()
  const data = U.settingsFromServer({ storage: STORAGE, licence: LICENCE_FREE, projects: LIST, now: NOW })
  for (const mode of MODES) {
    const prefs = { status: 'read', prefs: { keys: mode }, fromFile: ['keys'], file: 'read', pending: {}, writeError: {} }
    const got = rowsOf(U.settingsHtml({ data, prefs }), 'Keyboard map')
    const want = M.KEY_TABLE.map((e) => [M.keyText(e, mode), e.label, M.stateAnywhere(e, mode).replace(/\u00a0/g, ' ')])
    assert.deepEqual(got, want, mode)
    const by = Object.fromEntries(got.map((r) => [r[1], r]))
    assert.deepEqual(by['Next / previous in a list'], [mode === 'mod' ? '⌥j / ⌥k' : 'j / k', 'Next / previous in a list', mode === 'off' ? 'off' : ''])
    assert.deepEqual(by['Find, ask, act'], ['⌘K', 'Find, ask, act', ''])
    assert.deepEqual(by['Approve'], ['⌘↵', 'Approve', 'on some screens'], '⌘↵ is not a single key')
    assert.deepEqual(by['What counts as done'], [mode === 'mod' ? '⌥g d' : 'g d', 'What counts as done', mode === 'off' ? 'off' : 'on a Change'])
    assert.deepEqual(by['Hide until it changes'][2], mode === 'off' ? 'off' : '')
    assert.deepEqual(by['Plain / Technical'][2], mode === 'off' ? 'off' : '', 't works everywhere now (R-PREFS)')
    assert.deepEqual(by['Back to the Change'], ['Esc', 'Back to the Change', ''])
  }
})

test('6. the map lists the keys of the screen in front, then the keys that work everywhere, from the table', async () => {
  const U = await UI()
  const M = await KEYS()
  for (const screen of U.LIVE_SCREENS) {
    for (const mode of MODES) {
      const r = routeOf(screen)
      const html = U.mapHtml({ screen, at: r, mode })
      const { here, everywhere } = M.entriesFor(screen)
      const want = (es) => es.map((e) => [M.keyText(e, mode), e.label, M.stateOn(e, { mode, screen, change: r.change }).replace(/\u00a0/g, ' ')])
      assert.deepEqual(rowsOf(html, 'On this screen') || [], want(here), screen + ' ' + mode + ' here')
      assert.deepEqual(rowsOf(html, 'Everywhere'), want(everywhere), screen + ' ' + mode + ' everywhere')
    }
  }
  const inbox = U.mapHtml({ screen: 'Inbox', at: routeOf('Inbox'), mode: 'on' })
  const ih = Object.fromEntries(rowsOf(inbox, 'On this screen').map((r) => [r[0], r[2]]))
  assert.deepEqual(ih, { 'j / k': '', '↵': '', '.': '', '⌘↵': '', y: '', h: '', e: '' })
  const ie = Object.fromEntries(rowsOf(inbox, 'Everywhere').map((r) => [r[0], r[2]]))
  assert.equal(ie['g d'], 'on a Change'); assert.equal(ie['g i'], ''); assert.equal(ie.t, ''); assert.equal(ie['?'], '')
  const brief = Object.fromEntries(rowsOf(U.mapHtml({ screen: 'Change-Brief', at: routeOf('Change-Brief'), mode: 'mod' }), 'On this screen').map((r) => [r[0], r[2]]))
  assert.equal(brief['⌥j / ⌥k'], ''); assert.equal(brief['⌥.'], '')
  assert.ok(inbox.includes('Single keys <span style="color:#EDEDEF">on</span>'))
  assert.ok(!U.mapHtml({ screen: 'Settings', at: routeOf('Settings'), mode: 'off' }).includes('On this screen'), 'Settings draws no key of its own, so no empty group')
  assert.ok(inbox.includes('href="#/settings?project=p1&amp;pane=keyboard"'), 'Keyboard settings opens Settings › Keyboard')
  for (const html of [inbox]) assert.doesNotMatch(html.replace(/<[^>]+>/g, ' '), /\b(lie|cheat|violat\w*|ignor\w*|hallucinat\w*|trial)\b|Not available in this version|\d+\s?%/i)
})

/* ---------------- 7. the map: open, focus, close ---------------- */

test('7. ? opens the map; Tab stays inside it; Esc closes it and gives focus back; ⌘K and / open Find, ask, act', async () => {
  const U = await UI()
  window.location.hash = '#/inbox?project=p1'
  const { store } = fakeServer(U, { keys: 'on' })
  await store.load()
  const where = { screen: 'inbox', project: 'p1' }
  const m = U.mount({ name: 'Inbox', props: { data: inboxData(U), frame: {}, at: where }, link: U.R.linker(where), at: where, store })
  try {
    const rowBtn = m.el.querySelector('button.tl-row'); rowBtn.focus()
    press(rowBtn, '?', { shift: true }); await tick()
    const dlg = document.querySelector('section[role="dialog"][aria-labelledby="tl-keys-label"]')
    assert.ok(dlg, 'the map is open')
    assert.equal(dlg.getAttribute('aria-modal'), 'true')
    assert.ok(dlg.contains(document.activeElement), 'focus moved into the map')
    const focusables = [...dlg.querySelectorAll('button, a[href]')]
    assert.equal(focusables.length, 2)
    assert.equal(document.activeElement, focusables[0])
    dlg.dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true })); assert.equal(document.activeElement, focusables[1])
    focusables[1].dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true })); assert.equal(document.activeElement, focusables[0], 'Tab goes round')
    focusables[0].dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true })); assert.equal(document.activeElement, focusables[1], 'Shift+Tab goes round')
    // while the map is open, j does nothing
    press(document.activeElement, 'j'); await tick()
    assert.match(m.el.querySelector('button.tl-row[aria-current="true"]').getAttribute('aria-label'), /^Fix double-applied/)
    press(document.activeElement, 'Escape'); await tick()
    assert.equal(document.querySelector('section[aria-labelledby="tl-keys-label"]'), null, 'Esc closes it')
    assert.equal(document.activeElement, rowBtn, 'focus is back where it was')
    // ⌘K toggles Find, ask, act; Esc closes it; / opens it
    press(null, 'k', { meta: true }); await tick(); assert.equal(m.out.palette, true)
    press(null, 'k', { meta: true }); await tick(); assert.equal(m.out.palette, false)
    press(null, '/'); await tick(); assert.equal(m.out.palette, 'ask')
    press(null, 'Escape'); await tick(); assert.equal(m.out.palette, false)
    // ⌘K over the map closes the map and opens the palette
    press(null, '?', { shift: true }); await tick(); press(document.activeElement, 'k', { meta: true }); await tick()
    assert.equal(document.querySelector('section[aria-labelledby="tl-keys-label"]'), null); assert.equal(m.out.palette, true)
    press(null, 'Escape'); await tick()
    // the scrim and the esc button close it too
    press(null, '?', { shift: true }); await tick()
    document.querySelector('.tl-keys-overlay > div[aria-hidden="true"]').dispatchEvent(new dom.window.MouseEvent('click', { bubbles: true })); await tick()
    assert.equal(document.querySelector('section[aria-labelledby="tl-keys-label"]'), null, 'the scrim closes it')
  } finally {
    m.unmount()
  }
})

test('7. in Off, ? does nothing (it is a single key); ↵ and Esc still work', async () => {
  const U = await UI()
  window.location.hash = '#/inbox?project=p1'
  const { store } = fakeServer(U, { keys: 'off' })
  await store.load()
  const where = { screen: 'inbox', project: 'p1' }
  const m = U.mount({ name: 'Inbox', props: { data: inboxData(U), frame: {}, at: where }, link: U.R.linker(where), at: where, store })
  try {
    press(null, '?', { shift: true }); await tick()
    assert.equal(document.querySelector('section[aria-labelledby="tl-keys-label"]'), null)
    press(null, 'g'); press(null, 'r'); await tick()
    assert.equal(window.location.hash, '#/inbox?project=p1')
  } finally {
    m.unmount()
  }
})

test('7. g then a second key goes there on the live app, timed by the clock it is given', async () => {
  const U = await UI()
  window.location.hash = '#/inbox?project=p1'
  const { store } = fakeServer(U, { keys: 'on' })
  await store.load()
  let t = 0
  const where = { screen: 'inbox', project: 'p1' }
  const m = U.mount({ name: 'Inbox', props: { data: inboxData(U), frame: {}, at: where }, link: U.R.linker(where), at: where, store, now: () => t })
  try {
    press(null, 'g'); t = 999; press(null, 'r'); await tick()
    assert.equal(window.location.hash, '#/record?project=p1')
    window.location.hash = '#/inbox?project=p1'
    t = 10000; press(null, 'g'); t = 11001; press(null, 'r'); await tick()
    assert.equal(window.location.hash, '#/inbox?project=p1', 'too late')
  } finally {
    m.unmount()
  }
})

/* ---------------- 8. the preference ---------------- */

test('8. the preference store: read, a failed read is the default and says so, a write is kept only when the server keeps it', async () => {
  const P = await PREFS()
  const U = await UI()
  assert.equal(P.PREFERENCES, '/api/preferences')
  const ok = fakeServer(U, { keys: 'mod' })
  assert.equal(ok.store.get().status, 'idle')
  await ok.store.load()
  assert.deepEqual([ok.store.get().status, ok.store.get().prefs.keys, ok.store.get().fromFile], ['read', 'mod', ['keys']])
  assert.equal(ok.calls[0].method, 'GET'); assert.equal(ok.calls[0].url, '/api/preferences')
  assert.equal(await ok.store.set('keys', 'off'), true)
  const post = ok.calls.find((c) => c.method === 'POST')
  assert.deepEqual([post.url, post.body, post.headers['Content-Type']], ['/api/preferences', '{"keys":"off"}', 'application/json'])
  assert.equal(ok.store.get().prefs.keys, 'off')
  await assert.rejects(ok.store.set('keys', 'sometimes'), /Not a choice/)
  assert.equal(ok.calls.filter((c) => c.method === 'POST').length, 1, 'a bad value is never sent')
  for (const get of ['down', '500']) {
    const bad = fakeServer(U, { get })
    await bad.store.load()
    assert.deepEqual([bad.store.get().status, bad.store.get().prefs.keys], ['failed', 'on'], get)
  }
  for (const [post2, why] of [['newer', 'a newer ThruLine wrote your preferences file, so this one leaves it as it is'], ['symlink', 'your preferences file is not an ordinary file, so ThruLine writes nothing through it'], ['down', 'ThruLine did not answer']]) {
    const s = fakeServer(U, { keys: 'on', post: post2 })
    await s.store.load()
    assert.equal(await s.store.set('keys', 'off'), false)
    assert.deepEqual([s.store.get().prefs.keys, s.store.get().writeError.keys], ['on', { value: 'off', why }], post2)
    assert.deepEqual(s.store.get().pending, {})
  }
})

test('8. Settings › Keyboard: reading, a failed read, a write in flight and a refused write are each said; a click writes through the store', async () => {
  const U = await UI()
  const data = U.settingsFromServer({ storage: STORAGE, licence: LICENCE_FREE, projects: LIST, now: NOW })
  const pane = (prefs) => {
    const html = U.settingsHtml({ data, prefs })
    const at0 = html.indexOf('aria-labelledby="h-keys"')
    const seg = html.slice(at0, html.indexOf('aria-label="Keyboard map"', at0))
    const text = seg.match(/Single-key shortcuts<\/span> ?<span[^>]*>([^<]*)<\/span>/)[1]
    const pressed = [...seg.matchAll(/aria-pressed="true"[^>]*>([^<]*)</g)].map((x) => x[1])
    return { text: text.replace(/&#x27;/g, "'"), pressed }
  }
  const base = { prefs: { keys: 'on' }, fromFile: [], file: 'absent', pending: {}, writeError: {} }
  assert.deepEqual(pane(Object.assign({}, base, { status: 'reading' })), { text: 'Reading your choice from this machine…', pressed: [] })
  assert.deepEqual(pane(Object.assign({}, base, { status: 'failed' })), { text: 'ThruLine could not read your saved choice just now, so single keys are on, the default.', pressed: ['On'] })
  assert.deepEqual(pane(Object.assign({}, base, { status: 'read', pending: { keys: 'off' } })), { text: 'Keeping “Off” on this machine…', pressed: ['On'] })
  assert.deepEqual(pane(Object.assign({}, base, { status: 'read', writeError: { keys: { value: 'off', why: 'ThruLine did not answer' } } })), { text: 'ThruLine could not keep “Off”: ThruLine did not answer. Single keys are still on.', pressed: ['On'] })
  assert.deepEqual(pane(Object.assign({}, base, { status: 'read', prefs: { keys: 'off' } })), { text: 'Off: no single key acts. ⌘K, ⌘↵, ↵ and Esc still work.', pressed: ['Off'] })
  assert.deepEqual(pane(Object.assign({}, base, { status: 'read', prefs: { keys: 'mod' } })), { text: 'Each single key now needs ⌥ held, so typing can’t trigger one.', pressed: ['Only with ⌥'] })
  assert.deepEqual(pane(Object.assign({}, base, { status: 'read', file: 'newer_schema' })).text, 'On: one key per action. Turn off or add ⌥ if you use speech input. Your preferences file was written by a newer ThruLine, so this one uses the defaults and changes nothing in it.')
  // the live pane is not disabled, and says nothing about "this version"
  const html = U.settingsHtml({ data, prefs: Object.assign({}, base, { status: 'read' }) })
  const keysPane = html.slice(html.indexOf('aria-labelledby="h-keys"'), html.indexOf('aria-labelledby="h-plan"'))
  assert.doesNotMatch(keysPane, /aria-disabled="true"/)
  assert.doesNotMatch(keysPane, /Not available in this version|in this version|later version/)
  assert.ok(!keysPane.includes(HOME), 'no home folder')
  // a click on Off asks the store; a refused write is said in the pane
  const { store } = fakeServer(U, { keys: 'on', post: 'newer' })
  await store.load()
  const asked = []
  const actions = { setPreference: (k, v) => { asked.push([k, v]); return store.set(k, v).catch(() => false) } }
  const where = { screen: 'settings', pane: 'keyboard' }
  const m = U.mount({ name: 'Settings', props: { data, prefs: 'live', actions, frame: {}, at: where }, link: (h) => h, at: where, store })
  try {
    const off = [...m.el.querySelectorAll('div[aria-label="Single-key shortcuts"] button')].find((b) => b.textContent === 'Off')
    off.dispatchEvent(new dom.window.MouseEvent('click', { bubbles: true })); await tick(40)
    assert.deepEqual(asked, [['keys', 'off']])
    assert.deepEqual(store.get().writeError.keys, { value: 'off', why: 'a newer ThruLine wrote your preferences file, so this one leaves it as it is' })
    assert.ok(m.el.textContent.includes('ThruLine could not keep “Off”: a newer ThruLine wrote your preferences file, so this one leaves it as it is. Single keys are still on.'), 'the refusal is said in the pane')
    assert.equal(m.el.querySelector('div[aria-label="Single-key shortcuts"] button[aria-pressed="true"]').textContent, 'On')
  } finally {
    m.unmount()
  }
  // a kept write presses the new choice, and the dispatcher follows it
  const kept = fakeServer(U, { keys: 'on' })
  await kept.store.load()
  const m2 = U.mount({ name: 'Settings', props: { data, prefs: 'live', actions: { setPreference: (k, v) => kept.store.set(k, v) }, frame: {}, at: where }, link: (h) => h, at: where, store: kept.store })
  try {
    const btn = (t) => [...m2.el.querySelectorAll('div[aria-label="Single-key shortcuts"] button')].find((b) => b.textContent === t)
    btn('Only with ⌥').dispatchEvent(new dom.window.MouseEvent('click', { bubbles: true })); await tick(40)
    assert.equal(btn('Only with ⌥').getAttribute('aria-pressed'), 'true')
    assert.ok(m2.el.textContent.includes('⌥j / ⌥k'), 'the map shows ⌥ in front of each single key')
    window.location.hash = '#/settings?pane=keyboard'
    press(null, 'g'); press(null, 'i'); await tick()
    assert.equal(window.location.hash, '#/settings?pane=keyboard', 'g i without ⌥ is typing now')
  } finally {
    m2.unmount()
  }
})

/* ---------------- 9. the gallery ---------------- */

test('9. the design gallery gets no dispatcher, no map and no markup change', async () => {
  const U = await UI()
  const artboards = read('../artboards.jsx')
  assert.doesNotMatch(artboards, /keys\/|app\.jsx/, 'the gallery imports neither the keys nor the app')
  const inbox = U.design('Inbox')
  assert.ok(inbox.includes('Single keys on · '), 'the hint bar says what the canvas says')
  assert.ok(!inbox.includes('tl-keys-overlay'))
  // the gallery's Settings keyboard pane is the canvas's own list, c and x included
  const g = U.designMount('Inbox')
  try {
    assert.deepEqual(Object.keys(U.currentKeys()), [], 'the gallery registers no keys')
    press(null, 'j'); press(null, '?', { shift: true }); await tick()
    assert.equal(document.querySelector('section[aria-labelledby="tl-keys-label"]'), null)
  } finally {
    g.unmount()
  }
  const settingsLogic = read('../screens/Settings/logic.js')
  assert.match(settingsLogic, /\{ keys: \['c'\], label: 'Check it again', single: true \}/, 'the canvas’s list stays the gallery’s')
  assert.match(settingsLogic, /\{ keys: \['x'\], label: 'Select', single: true \}/)
})

test('C54 brief#25: every drawn Change Brief key has a wired handler', async () => {
  const K = await KEYS()
  const handlers = K.handlersWired('Change-Brief')
  for (const name of ['next', 'prev', 'primary', 'approve', 'copy', 'expand', 'then', 'tech', 'walk']) {
    assert.ok(handlers.includes(name), `${name} is wired on Change Brief`)
  }
})

test('C54 command#7: palette dot and y are wired there and owned by the palette', async () => {
  const K = await KEYS()
  for (const id of ['primary', 'copy']) {
    const row = K.KEY_TABLE.find((entry) => entry.id === id)
    assert.equal(K.wiredOn(row, 'Command-Ask'), true, `${id} is usable in the palette`)
  }
  assert.ok(!K.handlersWired('Command-Ask').includes('primary'))
  assert.ok(!K.handlersWired('Command-Ask').includes('copy'))
})
