/* C30 R-PREFS: the person's stored choices, applied across the whole app.
 *
 *  1. Face (Plain / Technical) is ONE stored value: every screen that draws the
 *     button draws it from the store, a click on any of them changes it for all,
 *     and a screen with no Technical content of its own shows exactly what it
 *     showed (nothing is invented).
 *  2. `t` switches the face — not while typing, not in "Off", with ⌥ in "Only with ⌥".
 *  3. A choice the server does not keep is said on every screen but Settings
 *     (the notice), and the face stays where it was.
 *  4. Density is the Inbox's and Changes' own control, and Settings reads the same value.
 *  5. Settings: every control reads the stored value, writes through the store,
 *     says beside it what is in flight or was refused, and says what the
 *     notifications and the weekly summary do within the open HQ page.
 *  6. The app root carries density, motion and colour vision, and follows the store.
 *  7. The design gallery is handed none of it and draws the canvas.
 *
 * Synthetic fixtures only. Every fetch is a stub; no server. The pieces are
 * bundled on their own (esbuild), as keys/keys.test.js does. */
import { test, afterEach } from 'node:test'
import assert from 'node:assert/strict'
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
const FORBIDDEN = /\b(lie|lies|lying|cheat|violate|violation|ignore|ignored|hallucinat\w*|trial)\b|\d\s?%|\/Users\//i

async function bundle(entry) {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: entry, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-prefs-')), 'm.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}
const lazy = (entry) => { let p = null; return () => (p = p || bundle(entry)) }

const SCREENS = ['Inbox', 'Change-Brief', 'Every-Statement', 'What-Changed', 'Done-Criteria', 'Repair-Loop', 'Approve-Sheet', 'Command-Ask', 'Record-Over-Time', 'Team-View']
const imports = SCREENS.map((n, i) => `import T${i} from './screens/${n}/template.jsx'\nimport L${i} from './screens/${n}/logic.js'`).join('\n')
const table = SCREENS.map((n, i) => `  '${n}': [T${i}, L${i}],`).join('\n')

const UI = lazy(`
import React, { useState } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { createRoot } from 'react-dom/client'
import { flushSync } from 'react-dom'
import { PortedScreen } from './screen.jsx'
import { DcScreen } from './dc/runtime.jsx'
import { designLink } from './artboards.jsx'
import { withFrame } from './data/project.js'
import { useKeys } from './keys/use-keys.jsx'
import { useAppearance } from './appearance.jsx'
import { PrefsNotice } from './prefs-notice.jsx'
import { createPreferences, usePreferences } from './data/preferences.js'
import { settingsFromServer } from './screens/Settings/from-server.js'
import SettingsT from './screens/Settings/template.jsx'
import SettingsL from './screens/Settings/logic.js'
import ChangesT from './screens/Changes/template.jsx'
import ChangesL from './screens/Changes/logic.js'
import { changesData } from './screens/Changes/from-projects.js'
${imports}
export { React, renderToStaticMarkup, createRoot, flushSync, PortedScreen, DcScreen, designLink, withFrame, useKeys, useAppearance, PrefsNotice, createPreferences, usePreferences, settingsFromServer, SettingsT, SettingsL, ChangesT, ChangesL, changesData, useState }
export const S = {
${table}
}
`)

/* ---------------- a server, and a screen on it ---------------- */

const DEFAULT_PREFS = { face: 'plain', density: 'comfortable', motion: 'system', colour_vision: 'standard', keys: 'on', masked_copy: true, notifications: { ready: false, not_backed: false, agent_waiting: false, recording_stopped: false }, weekly_summary: false }
const docOf = (p, file = 'read') => ({ schema_version: 1, preferences: p, defaults: DEFAULT_PREFS, path: '/Users/someone/.thruline/preferences.json', file, from_file: [], unknown_keys: [], problems: [] })

/** A preferences server in memory: every request recorded; `refuse` makes every write a 409. */
function fakeServer(over = {}) {
  const calls = []
  const held = JSON.parse(JSON.stringify(Object.assign({}, DEFAULT_PREFS, over)))
  const srv = {
    calls, held, refuse: null, file: 'read',
    fetchImpl: async (url, opts = {}) => {
      const method = opts.method || 'GET'
      calls.push({ url, method, body: opts.body })
      if (method === 'GET') return { ok: true, status: 200, json: async () => docOf(held, srv.file) }
      if (srv.refuse) return { ok: false, status: 409, json: async () => ({ error: srv.refuse }) }
      const b = JSON.parse(opts.body)
      for (const [k, v] of Object.entries(b)) { if (k === 'notifications') Object.assign(held.notifications, v); else held[k] = v }
      return { ok: true, status: 200, json: async () => docOf(held, srv.file) }
    },
  }
  srv.posts = () => calls.filter((c) => c.method === 'POST').map((c) => c.body)
  return srv
}
/* every mounted screen is unmounted after its test, pass or fail: a screen's watcher keeps a timer, and a timer
 * left running would keep this file from ever finishing */
const MOUNTED = []
afterEach(() => { while (MOUNTED.length) { try { MOUNTED.pop()() } catch { /* already gone */ } } })
const settle = async (cond = () => false, turns = 60) => {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setTimeout(r, 5))
  await new Promise((r) => setTimeout(r, 5))
}
const live = (U, over) => { const srv = fakeServer(over); return { srv, store: U.createPreferences({ fetchImpl: srv.fetchImpl }) } }

/** One live screen (demo content), mounted as app.jsx mounts it: the store is the person's. */
function mountScreen(U, { name, store, at = { screen: 'inbox', project: 'p1' } }) {
  const [T, L] = U.S[name]
  const Live = U.withFrame(L)
  const el = document.createElement('div')
  document.body.appendChild(el)
  const root = U.createRoot(el)
  U.flushSync(() => root.render(U.React.createElement(U.PortedScreen, { Logic: Live, Template: T, at, store })))
  const unmount = () => { root.unmount(); el.remove() }
  MOUNTED.push(unmount)
  return { el, unmount }
}
const parse = (html) => new JSDOM(html).window.document
const faceButton = (root) => [...root.querySelectorAll('button')].find((b) => b.textContent.replace(/\s+/g, '') === 'Plain/Technical')
/** The face a screen's Plain / Technical button says is in use. */
function faceDrawn(root) {
  const b = faceButton(root)
  assert.ok(b, 'the screen draws a Plain / Technical button')
  const label = b.getAttribute('aria-label') || ''
  if (/^Face: Technical/.test(label)) return 'technical'
  if (/^Face: Plain/.test(label)) return 'plain'
  return b.getAttribute('aria-pressed') === 'true' ? 'technical' : 'plain'
}
const asState = (prefs, extra = {}) => ({ status: 'read', prefs: Object.assign({}, DEFAULT_PREFS, prefs), fromFile: [], file: 'read', pending: {}, writeError: {}, ...extra })

/* ---------------- 1. face ---------------- */

test('1. every screen that draws the Plain / Technical button draws the STORED face; the gallery draws the canvas', async () => {
  const U = await UI()
  for (const name of SCREENS) {
    const [T, L] = U.S[name]
    const Live = U.withFrame(L)
    for (const face of ['plain', 'technical']) {
      const props = { prefs: asState({ face }), setPreference: () => Promise.resolve(true) }
      const html = U.renderToStaticMarkup(U.React.createElement(U.DcScreen, { Logic: Live, Template: T, props, link: U.designLink }))
      assert.equal(faceDrawn(parse(html)), face, name + ' with the stored face ' + face)
    }
    const gallery = U.renderToStaticMarkup(U.React.createElement(U.DcScreen, { Logic: L, Template: T, props: {}, link: U.designLink }))
    assert.equal(faceDrawn(parse(gallery)), 'plain', name + ' in the gallery')
  }
})

/* what a screen draws with its Plain / Technical button taken out */
const withoutFaceButton = (html) => {
  const d = parse(html)
  faceButton(d).remove()
  return d.body.innerHTML
}

test('1. a screen with no Technical content of its own shows exactly what it showed: only the button changes', async () => {
  const U = await UI()
  for (const name of ['Inbox', 'Every-Statement', 'What-Changed', 'Repair-Loop', 'Approve-Sheet', 'Command-Ask', 'Record-Over-Time']) {
    const [T, L] = U.S[name]
    const Live = U.withFrame(L)
    const render = (face) => U.renderToStaticMarkup(U.React.createElement(U.DcScreen, { Logic: Live, Template: T, props: { prefs: asState({ face }), setPreference: () => {} }, link: U.designLink }))
    assert.equal(withoutFaceButton(render('technical')), withoutFaceButton(render('plain')), name + ': nothing is invented for the Technical face')
    assert.notEqual(render('technical'), render('plain'), name + ': the button itself changes')
  }
})

test('1. a screen whose template already draws a Technical branch draws it from the stored face (Change-Brief, Done-Criteria, Team-View)', async () => {
  const U = await UI()
  for (const name of ['Change-Brief', 'Done-Criteria', 'Team-View']) {
    const [T, L] = U.S[name]
    const Live = U.withFrame(L)
    const render = (face) => U.renderToStaticMarkup(U.React.createElement(U.DcScreen, { Logic: Live, Template: T, props: { prefs: asState({ face }), setPreference: () => {} }, link: U.designLink }))
    // the screen on its own local face (no preferences handed to it), which is Plain
    const local = U.renderToStaticMarkup(U.React.createElement(U.DcScreen, { Logic: Live, Template: T, props: {}, link: U.designLink }))
    assert.equal(render('plain'), local, name + ': the stored Plain is the screen’s own Plain')
    assert.notEqual(withoutFaceButton(render('technical')), withoutFaceButton(render('plain')), name + ': Technical draws what this screen holds for it')
  }
})

test('1. ONE value for the app: a click on any screen’s button changes what every other screen reads, in one write', async () => {
  const U = await UI()
  const { srv, store } = live(U)
  const a = mountScreen(U, { name: 'Inbox', store })
  const b = mountScreen(U, { name: 'Repair-Loop', store, at: { screen: 'check', change: 'c1', project: 'p1' } })
  await settle(() => store.get().status === 'read')
  assert.deepEqual([faceDrawn(a.el), faceDrawn(b.el)], ['plain', 'plain'])
  faceButton(a.el).click()
  await settle(() => store.get().prefs.face === 'technical')
  assert.deepEqual([faceDrawn(a.el), faceDrawn(b.el)], ['technical', 'technical'], 'the same face on both')
  assert.deepEqual(srv.posts(), ['{"face":"technical"}'], 'one write, naming the face only')
  faceButton(b.el).click()
  await settle(() => store.get().prefs.face === 'plain')
  assert.deepEqual([faceDrawn(a.el), faceDrawn(b.el)], ['plain', 'plain'])
  assert.deepEqual(srv.posts(), ['{"face":"technical"}', '{"face":"plain"}'])
  a.unmount(); b.unmount()
})

test('1. before the choices are read, and when they cannot be, a screen draws Plain and says nothing false', async () => {
  const U = await UI()
  const srv = fakeServer({ face: 'technical' })
  const store = U.createPreferences({ fetchImpl: async (u, o = {}) => { if ((o.method || 'GET') === 'GET') await new Promise((r) => setTimeout(r, 30)); return srv.fetchImpl(u, o) } })
  const a = mountScreen(U, { name: 'Inbox', store })
  assert.equal(faceDrawn(a.el), 'plain', 'the default while the answer is out')
  await settle(() => store.get().status === 'read', 80)
  assert.equal(faceDrawn(a.el), 'technical')
  a.unmount()
  const down = U.createPreferences({ fetchImpl: async () => ({ ok: false, status: 404, json: async () => ({}) }) })
  const b = mountScreen(U, { name: 'Inbox', store: down })
  await settle(() => down.get().status === 'failed')
  assert.equal(faceDrawn(b.el), 'plain')
  b.unmount()
})

/* ---------------- 2. t ---------------- */

/** The app's keys over nothing: the hook, a store, and a place to press keys. */
function mountKeys(U, { store, at = { screen: 'inbox', project: 'p1' } }) {
  const out = { palette: false }
  const el = document.createElement('div')
  document.body.appendChild(el)
  const root = U.createRoot(el)
  function Harness() {
    const [palette, setPalette] = U.useState(false)
    out.palette = palette
    const keys = U.useKeys({ at, palette, setPalette, store })
    return U.React.createElement(U.React.Fragment, null, U.React.createElement('input', { id: 'field' }), keys.map)
  }
  U.flushSync(() => root.render(U.React.createElement(Harness)))
  const unmount = () => { root.unmount(); el.remove() }
  MOUNTED.push(unmount)
  return { el, out, unmount }
}
function press(target, key, { alt = false, meta = false } = {}) {
  const code = /^[a-z]$/.test(key) ? 'Key' + key.toUpperCase() : ''
  const e = new dom.window.KeyboardEvent('keydown', { key, code, altKey: alt, metaKey: meta, bubbles: true, cancelable: true })
  ;(target || document.body).dispatchEvent(e)
  return e
}

test('2. t switches the stored face, there and back, from anywhere on a live screen', async () => {
  const U = await UI()
  const { srv, store } = live(U)
  const k = mountKeys(U, { store })
  await settle(() => store.get().status === 'read')
  const e = press(document.body, 't')
  assert.equal(e.defaultPrevented, true)
  await settle(() => store.get().prefs.face === 'technical')
  press(document.body, 't')
  await settle(() => store.get().prefs.face === 'plain')
  assert.deepEqual(srv.posts(), ['{"face":"technical"}', '{"face":"plain"}'])
  k.unmount()
})

test('2. t does not fire while typing, with ⌘ held, or in "Off"; in "Only with ⌥" it needs ⌥', async () => {
  const U = await UI()
  const a = live(U)
  const k = mountKeys(U, { store: a.store })
  await settle(() => a.store.get().status === 'read')
  const field = k.el.querySelector('#field')
  press(field, 't')
  press(document.body, 't', { meta: true })
  await settle()
  assert.deepEqual(a.srv.posts(), [], 'not inside a field, not as ⌘T')
  k.unmount()

  const off = live(U, { keys: 'off' })
  const k2 = mountKeys(U, { store: off.store })
  await settle(() => off.store.get().status === 'read')
  press(document.body, 't')
  await settle()
  assert.deepEqual(off.srv.posts(), [], 'single keys are off')
  k2.unmount()

  const mod = live(U, { keys: 'mod' })
  const k3 = mountKeys(U, { store: mod.store })
  await settle(() => mod.store.get().status === 'read')
  press(document.body, 't')
  await settle()
  assert.deepEqual(mod.srv.posts(), [], 'without ⌥, nothing')
  press(document.body, '†', { alt: true })
  press(document.body, 't', { alt: true })
  await settle(() => mod.store.get().prefs.face === 'technical')
  assert.equal(mod.store.get().prefs.face, 'technical')
  k3.unmount()
})

test('2. the key mode is read live: the same mounted keys obey a choice made after they mounted', async () => {
  const U = await UI()
  const { srv, store } = live(U)
  const k = mountKeys(U, { store })
  await settle(() => store.get().status === 'read')
  await store.set('keys', 'off')
  press(document.body, 't')
  await settle()
  assert.deepEqual(srv.posts(), ['{"keys":"off"}'], 'Off: t does nothing')
  await store.set('keys', 'on')
  press(document.body, 't')
  await settle(() => store.get().prefs.face === 'technical')
  assert.equal(store.get().prefs.face, 'technical', 'On again: t acts, with no remount')
  k.unmount()
})

/* ---------------- 3. a refusal is said ---------------- */

test('3. a face the server will not keep stays where it was, and is said on the screen; Settings says it by the control instead', async () => {
  const U = await UI()
  const { srv, store } = live(U)
  srv.refuse = 'preferences_file_newer'
  const el = document.createElement('div')
  document.body.appendChild(el)
  const root = U.createRoot(el)
  MOUNTED.push(() => { root.unmount(); el.remove() })
  const place = { at: { screen: 'inbox', project: 'p1' } }
  function Frame() {
    return U.React.createElement(U.React.Fragment, null, U.React.createElement(U.PortedScreen, { Logic: U.withFrame(U.S.Inbox[1]), Template: U.S.Inbox[0], at: place.at, store }), U.React.createElement(U.PrefsNotice, { at: place.at, store }))
  }
  U.flushSync(() => root.render(U.React.createElement(Frame)))
  await settle(() => store.get().status === 'read')
  assert.equal(el.querySelector('[data-prefs-notice]'), null, 'nothing is said when nothing was refused')
  faceButton(el).click()
  await settle(() => Object.keys(store.get().writeError).length > 0)
  assert.equal(faceDrawn(el), 'plain', 'a refused choice is not drawn as kept')
  const note = el.querySelector('[data-prefs-notice="face"]')
  assert.ok(note, 'the refusal is on the screen')
  assert.equal(note.getAttribute('role'), 'alert')
  const said = note.textContent.replace(/\s+/g, ' ')
  assert.match(said, /^ThruLine could not keep “Technical”: a newer ThruLine wrote your preferences file, so this one leaves it as it is\. Face is still Plain\./)
  assert.doesNotMatch(said, FORBIDDEN)
  // on Settings the row says it, so the notice does not
  place.at = { screen: 'settings' }
  U.flushSync(() => root.render(U.React.createElement(Frame)))
  assert.equal(el.querySelector('[data-prefs-notice]'), null)
  place.at = { screen: 'inbox', project: 'p1' }
  U.flushSync(() => root.render(U.React.createElement(Frame)))
  // dismissed by the person
  ;[...el.querySelectorAll('button')].find((b) => b.textContent === 'Dismiss').click()
  await settle()
  assert.equal(el.querySelector('[data-prefs-notice]'), null)
  // and by the choice being kept next time
  srv.refuse = null
  faceButton(el).click()
  await settle(() => store.get().prefs.face === 'technical')
  assert.equal(faceDrawn(el), 'technical')
  assert.equal(el.querySelector('[data-prefs-notice]'), null)
  root.unmount(); el.remove()
})

/* ---------------- 4. density ---------------- */

const seg = (root, group, label) => [...root.querySelectorAll('[role=group][aria-label="' + group + '"] button')].find((b) => b.textContent.trim() === label)

test('4. the Inbox’s Comfortable / Compact buttons are the stored density: compact draws its rows on one line, and the click is a write', async () => {
  const U = await UI()
  const { srv, store } = live(U, { density: 'compact' })
  const a = mountScreen(U, { name: 'Inbox', store })
  await settle(() => store.get().status === 'read')
  assert.equal(seg(a.el, 'Density', 'Compact').getAttribute('aria-pressed'), 'true')
  const padded = (pad) => [...a.el.querySelectorAll('[style]')].filter((n) => n.style.padding === pad).length
  assert.ok(padded('8px 16px 8px 20px') > 0, 'compact row padding (DESIGN-SPEC §3.01)')
  seg(a.el, 'Density', 'Comfortable').click()
  await settle(() => store.get().prefs.density === 'comfortable')
  assert.equal(seg(a.el, 'Density', 'Comfortable').getAttribute('aria-pressed'), 'true')
  assert.ok(padded('10px 16px 10px 20px') > 0 && padded('8px 16px 8px 20px') === 0, 'comfortable row padding')
  assert.deepEqual(srv.posts(), ['{"density":"comfortable"}'])
  a.unmount()
})

test('4. Changes draws the same stored density as the Inbox, and the same button writes it', async () => {
  const U = await UI()
  const HOME = '/Users/someone'
  const at = (h, m) => new Date(2026, 9, 7, h, m).toISOString()
  const t0 = { total: 0, succeeded: 0, failed: 0, unknown: 0 }
  const thread = (id) => ({ id, title: 'Thread ' + id, title_basis: 'instruction', started_at: at(15, 0), last_activity_at: at(15, 30), sessions: 1, prompts: 1, conversation: 0, edits: 0, files: 0, top_files: [], commands: t0, tests: t0, commits: 0, claims: 1, whose_move: 'done', whose_move_basis: 'nothing_open', proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: [] }, current_version_id: '3f9a1c2deadbeef' })
  const SHOP = { id: 'p1', name: 'shop', root: HOME + '/code/shop', complete: true, threads: 1, sessions: 1, claims: 1, sources: ['claude_code'], last_activity: at(16, 5), whose_move: { you: 0, agent: 0, done: 1 } }
  const data = U.changesData({ projects: { complete: true, projects: [SHOP] }, views: { p1: { project: SHOP, threads: [thread('t1')] } }, storage: { bytes: 5.4e9, path: HOME + '/.thruline' }, now: new Date(2026, 9, 7, 16, 10), current: 'p1' })
  const { srv, store } = live(U, { density: 'compact' })
  const el = document.createElement('div')
  document.body.appendChild(el)
  const root = U.createRoot(el)
  MOUNTED.push(() => { root.unmount(); el.remove() })
  U.flushSync(() => root.render(U.React.createElement(U.PortedScreen, { Logic: U.withFrame(U.ChangesL), Template: U.ChangesT, at: { screen: 'changes', project: 'p1' }, data, store })))
  await settle(() => store.get().status === 'read')
  assert.equal(seg(el, 'Density', 'Compact').getAttribute('aria-pressed'), 'true')
  seg(el, 'Density', 'Comfortable').click()
  await settle(() => store.get().prefs.density === 'comfortable')
  assert.equal(seg(el, 'Density', 'Comfortable').getAttribute('aria-pressed'), 'true')
  assert.deepEqual(srv.posts(), ['{"density":"comfortable"}'])
  root.unmount(); el.remove()
})

/* ---------------- 5. Settings ---------------- */

const HOME = '/Users/someone'
const STORAGE = { bytes: 5.4e9, path: HOME + '/.thruline' }
const LICENCE_FREE = { status: 'absent', paid_gate: true, entitlements: { approve_version: { allowed: false, status: 'absent' } }, technical: {} }

/** Settings on its live path: the vals, and what each control asked of the store. */
async function settingsVals(prefsState, { nav = 'appearance', notificationPermission = null } = {}) {
  const U = await UI()
  const asked = []
  const data = U.settingsFromServer({ storage: STORAGE, licence: LICENCE_FREE, now: new Date(2026, 9, 7, 16, 10) })
  const C = U.withFrame(U.SettingsL)
  const props = { data, prefs: prefsState, notificationPermission, setPreference: (n, v) => { asked.push([n, v]); return Promise.resolve(true) }, actions: {}, frame: {}, at: { screen: 'settings' } }
  const inst = new C(props)
  inst.state = Object.assign({}, inst.state, { nav })
  const render = () => U.renderToStaticMarkup(U.React.createElement(U.SettingsT, { v: inst.renderVals(), link: (h) => h }))
  return { U, v: inst.renderVals(), asked, html: render(), render, inst }
}
const pressed = (segs) => segs.filter((o) => o.pressed === 'true').map((o) => o.label)
const textOf = (html) => parse(html).body.textContent.replace(/\s+/g, ' ')

test('5. every Settings control is drawn as the server last kept it', async () => {
  const a = await settingsVals(asState({ face: 'technical', density: 'compact', motion: 'reduce', colour_vision: 'proven_teal', masked_copy: false, weekly_summary: true, notifications: { ready: false, not_backed: true, agent_waiting: false, recording_stopped: true } }))
  assert.deepEqual(pressed(a.v.segFace), ['Technical'])
  assert.deepEqual(pressed(a.v.segDensity), ['Compact'])
  assert.deepEqual(pressed(a.v.segMotion), ['Always reduce'])
  assert.equal(a.v.sw.cvd.checked, 'true')
  assert.equal(a.v.cvd.c, '#2DD4BF', 'the preview pill is teal when the choice is')
  assert.equal(a.v.sw.redact.checked, 'false')
  assert.equal(a.v.sw.digest.checked, 'true')
  assert.deepEqual(a.v.notifs.map((n) => n.sw.checked), ['false', 'true', 'false', 'true'])
  assert.equal(a.v.densityModified, true)
  assert.equal(a.v.faceTop.aria, 'Face: Technical. Switch to Plain')

  const b = await settingsVals(asState({}))
  assert.deepEqual(pressed(b.v.segFace), ['Plain'])
  assert.deepEqual(pressed(b.v.segDensity), ['Comfortable'])
  assert.deepEqual(pressed(b.v.segMotion), ['Follow this Mac'])
  assert.equal(b.v.sw.cvd.checked, 'false')
  assert.equal(b.v.cvd.c, '#3DD68C')
  assert.equal(b.v.sw.redact.checked, 'true')
  assert.equal(b.v.sw.digest.checked, 'false')
  assert.deepEqual(b.v.notifs.map((n) => n.sw.checked), ['false', 'false', 'false', 'false'])
  assert.equal(b.v.densityModified, false)
})

test('5. every Settings control writes through the store, by the name and value the route takes', async () => {
  const s = await settingsVals(asState({}))
  const { v, asked } = s
  v.segFace[1].pick()
  v.segDensity[1].pick()
  v.segMotion[1].pick()
  v.sw.cvd.toggle()
  v.sw.redact.toggle()
  v.sw.digest.toggle()
  v.notifs[0].sw.toggle()
  v.notifs[1].sw.toggle()
  v.notifs[2].sw.toggle()
  v.notifs[3].sw.toggle()
  v.toggleFace()
  assert.deepEqual(asked, [
    ['face', 'technical'], ['density', 'compact'], ['motion', 'reduce'], ['colour_vision', 'proven_teal'], ['masked_copy', false],
    ['weekly_summary', true], ['notifications.ready', true], ['notifications.not_backed', true], ['notifications.agent_waiting', true], ['notifications.recording_stopped', true], ['face', 'technical'],
  ])
  // the other way
  const t = await settingsVals(asState({ face: 'technical', colour_vision: 'proven_teal', masked_copy: false, weekly_summary: true }))
  t.v.sw.cvd.toggle(); t.v.sw.redact.toggle(); t.v.sw.digest.toggle(); t.v.toggleFace()
  assert.deepEqual(t.asked, [['colour_vision', 'standard'], ['masked_copy', true], ['weekly_summary', false], ['face', 'plain']])
})

test('5. all four event choices are available; browser permission feedback never draws an unkept choice as on', async () => {
  const permission = 'Your browser did not allow notifications.'
  const s = await settingsVals(asState({}), { nav: 'notifications', notificationPermission: permission })
  assert.equal(s.v.notifs.length, 4)
  for (const n of s.v.notifs) {
    assert.notEqual(n.sw.dis, 'true')
    assert.equal(n.sw.checked, 'false')
  }
  assert.deepEqual(s.asked, [])
  assert.ok(textOf(s.html).includes(permission))
  assert.doesNotMatch(textOf(s.html), /A permission prompt is waiting/)
})

test('5. notifications and the weekly summary state their C51 delivery and export limits', async () => {
  const s = await settingsVals(asState({}), { nav: 'notifications' })
  const t = textOf(s.html)
  assert.ok(t.includes('These four events can notify from this open HQ page only. Each starts Off. Turning one on first asks your browser for permission. Nothing leaves this machine.'), 'the note under the switches')
  assert.ok(t.includes('Open HQ page only · browser permission required'), 'the delivery chip')
  assert.ok(t.includes('On the first HQ open each week, show a dismissible last-7-day card. Save as Markdown downloads it to this machine.'), 'the weekly summary')
  assert.doesNotMatch(t, /nothing sends a notification yet|Nothing writes the summary yet/)
  assert.ok(!s.v.notes.notif.includes('Not available in this version'))
  assert.ok(!s.v.content.digestSub.includes('Not available in this version'))
  assert.ok(s.v.notifs.every((n) => !n.sub.includes('Not available in this version')))
  const g = await settingsVals(asState({ masked_copy: true }), { nav: 'privacy' })
  assert.ok(textOf(g.html).includes('The export sheet starts with its redacted copy on.'))
  const h = await settingsVals(asState({ masked_copy: false }), { nav: 'privacy' })
  assert.ok(textOf(h.html).includes('The export sheet starts with its redacted copy off; switch it on there each time.'))
})

test('5. beside each control: a write in flight is said, a refusal is said with what stands, and nothing is drawn as kept', async () => {
  const refused = (name, value, why = 'a newer ThruLine wrote your preferences file, so this one leaves it as it is') => asState({}, { writeError: { [name]: { value, why } } })
  const d = await settingsVals(refused('density', 'compact'))
  assert.match(d.v.densityText, /^ThruLine could not keep “Compact”: a newer ThruLine wrote your preferences file, so this one leaves it as it is\. Density is still Comfortable\.$/)
  assert.deepEqual(pressed(d.v.segDensity), ['Comfortable'])
  const m = await settingsVals(refused('motion', 'reduce'))
  assert.match(m.v.content.motionSub, /could not keep “Always reduce”.*Motion is still Follow this Mac\./)
  const c = await settingsVals(refused('colour_vision', 'proven_teal'))
  assert.match(c.v.cvd.sentence, /could not keep “Proven is teal”.*Colour is still Proven in green\./)
  const f = await settingsVals(refused('face', 'technical'))
  assert.match(f.v.content.faceTail, /could not keep “Technical”.*Face is still Plain\./)
  const r = await settingsVals(refused('masked_copy', false))
  assert.match(r.v.content.redactSub, /could not keep “Off”.*The masked copy is still On\./)
  const w = await settingsVals(refused('weekly_summary', true))
  assert.match(w.v.content.digestSub, /could not keep “On”.*The weekly summary is still Off\./)
  const n = await settingsVals(refused('notifications.not_backed', true))
  assert.match(n.v.notifs[1].sub, /could not keep “On”.*Not backed notifications is still Off\./)
  assert.ok(!/could not keep/.test(n.v.notifs[0].sub), 'only the switch that was refused')
  assert.equal(n.v.notifs[1].sw.checked, 'false')

  const p = await settingsVals(asState({}, { pending: { density: 'compact', 'notifications.agent_waiting': true } }))
  assert.equal(p.v.densityText, 'Keeping “Compact” on this machine…')
  assert.equal(p.v.notifs[2].sub, 'Keeping “On” on this machine…')
  for (const x of [d, m, c, f, r, w, n, p]) assert.doesNotMatch(JSON.stringify([x.v.content, x.v.notifs, x.v.densityText, x.v.cvd]), FORBIDDEN)
})

test('5. while the choices are being read the controls are locked; a read that failed says the defaults are in use', async () => {
  const reading = await settingsVals({ status: 'reading', prefs: asState({}).prefs, pending: {}, writeError: {} })
  for (const s of [reading.v.segFace, reading.v.segDensity, reading.v.segMotion]) assert.ok(s.every((o) => o.dis === 'true'))
  for (const k of ['redact', 'cvd', 'digest']) assert.equal(reading.v.sw[k].dis, 'true', k)
  assert.ok(reading.v.notifs.every((n) => n.sw.dis === 'true'))
  reading.v.sw.cvd.toggle()
  assert.deepEqual(reading.asked, [])
  assert.match(reading.v.content.lookSub, /Reading your choices from this machine…$/)

  const failed = await settingsVals(asState({}, { status: 'failed' }))
  assert.match(failed.v.content.lookSub, /ThruLine could not read your saved choices just now, so these show the defaults\.$/)
  assert.ok(failed.v.segDensity.every((o) => o.dis !== 'true'), 'a failed read can still be written')

  const newer = await settingsVals(asState({}, { file: 'newer_schema' }))
  assert.match(newer.v.content.lookSub, /Your preferences file was written by a newer ThruLine/)
})

test('5. no sentence Settings adds says a forbidden word, a percentage, a time zone, trial or a home folder', async () => {
  for (const nav of ['appearance', 'notifications', 'privacy', 'keyboard']) {
    const s = await settingsVals(asState({ colour_vision: 'proven_teal', masked_copy: false }), { nav })
    assert.doesNotMatch(textOf(s.html), FORBIDDEN, nav)
  }
})

/* ---------------- 6. the app root ---------------- */

test('6. the app root carries density, motion and colour vision, and follows the store; a refused choice moves nothing', async () => {
  const U = await UI()
  const { srv, store } = live(U)
  const el = document.createElement('div')
  el.id = 'root'
  document.body.innerHTML = ''
  document.body.appendChild(el)
  const root = U.createRoot(el)
  MOUNTED.push(() => { root.unmount(); el.remove() })
  function Harness() { U.useAppearance(store); return null }
  const attrs = () => [el.getAttribute('data-density'), el.getAttribute('data-motion'), el.getAttribute('data-colour-vision')]
  U.flushSync(() => root.render(U.React.createElement(Harness)))
  assert.deepEqual(attrs(), ['comfortable', 'system', 'standard'], 'the defaults while the choices are being read')
  await settle(() => store.get().status === 'read')
  await store.set('density', 'compact')
  await store.set('motion', 'reduce')
  await store.set('colour_vision', 'proven_teal')
  await settle()
  assert.deepEqual(attrs(), ['compact', 'reduce', 'proven_teal'])
  srv.refuse = 'preferences_file_newer'
  await store.set('colour_vision', 'standard')
  await settle()
  assert.deepEqual(attrs(), ['compact', 'reduce', 'proven_teal'], 'a refused choice changes nothing')
  root.unmount()
  document.body.innerHTML = ''
})

test('6. app.jsx mounts the three: the keys, the appearance and the notice', async () => {
  const { readFileSync } = await import('node:fs')
  const app = readFileSync(join(here, 'app.jsx'), 'utf8')
  assert.match(app, /useAppearance\(\)/)
  assert.match(app, /<PrefsNotice at=\{at\} \/>/)
  assert.match(app, /useKeys\(\{ at, palette: commandOpen, setPalette: setCommandOpen \}\)/)
})

/* ---------------- 7. the gallery ---------------- */

test('7. the design gallery is handed none of it: no preferences read, no notice, no attributes, every control as the canvas drew it', async () => {
  const U = await UI()
  const { readFileSync } = await import('node:fs')
  const gallery = readFileSync(join(here, 'artboards.jsx'), 'utf8')
  assert.doesNotMatch(gallery, /preferences|appearance|prefs-notice|usePreferences/)
  const [T, L] = U.S.Inbox
  const html = U.renderToStaticMarkup(U.React.createElement(U.DcScreen, { Logic: L, Template: T, props: {}, link: U.designLink }))
  const d = parse(html)
  assert.equal(faceButton(d).getAttribute('aria-label'), 'Face: Plain. Switch to Technical', 'the canvas’s own label')
  assert.equal(seg(d, 'Density', 'Comfortable').getAttribute('aria-pressed'), 'true')
  assert.equal(d.querySelector('[data-prefs-notice]'), null)
  const settings = U.renderToStaticMarkup(U.React.createElement(U.DcScreen, { Logic: U.SettingsL, Template: U.SettingsT, props: {}, link: U.designLink }))
  const sd = parse(settings)
  assert.ok(sd.body.textContent.includes('In this tab only · the page must be open'), 'the canvas’s own chip')
})

test('7. Proven is drawn from the token, with the standard green as its fallback, on every screen that draws it', async () => {
  const U = await UI()
  let withToken = 0
  for (const name of SCREENS) {
    const [T, L] = U.S[name]
    const html = U.renderToStaticMarkup(U.React.createElement(U.DcScreen, { Logic: L, Template: T, props: {}, link: U.designLink }))
    if (html.includes('var(--tl-proven, #3DD68C)') || html.includes('rgba(var(--tl-proven-rgb, 61, 214, 140)')) withToken += 1
    assert.ok(!/#3DD68C/i.test(html.replace(/var\(--tl-proven, #3DD68C\)/g, '')), name + ': no raw green is left over')
    assert.ok(!/rgba\(61, ?214, ?140/.test(html), name + ': no raw rgba green either')
  }
  // most demo states hold no Proven line; the source-wide guard (data/palette.test.js) covers the rest
  assert.ok(withToken >= 2, 'Proven reads the token where a demo state draws it (' + withToken + ' of ' + SCREENS.length + ')')
})
