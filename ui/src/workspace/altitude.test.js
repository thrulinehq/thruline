/* P6-1 — the altitude module: the default, the memory, and the keyboard.
 *
 * Every storage and window here is a fake handed in, so each rule is watched
 * directly rather than inferred from a screen.
 */
import test from 'node:test'
import assert from 'node:assert/strict'
import {
  ALTITUDES,
  ALTITUDE_KEY,
  DEFAULT_ALTITUDE,
  KEYMAP,
  altitudeFromSearch,
  initialAltitude,
  installKeys,
  isAltitude,
  isTyping,
  keyAction,
  pressPrimary,
  readAltitude,
  rememberAltitude,
} from './altitude.js'
import * as ALT from './altitude.js'

function store(initial) {
  const m = new Map(Object.entries(initial || {}))
  const writes = []
  return {
    writes,
    getItem: (k) => (m.has(k) ? m.get(k) : null),
    setItem: (k, v) => { writes.push([k, v]); m.set(k, String(v)) },
  }
}
const blocked = { getItem() { throw new Error('SecurityError') }, setItem() { throw new Error('QuotaExceeded') } }

test('A3-WORKBENCH ruling: the third view, old choices, and keys 1 / 2 / 3', () => {
  assert.deepEqual([...ALTITUDES], ['brief', 'explore', 'workbench'])
  assert.equal(isAltitude('pro'), false)
  const remembered = store({ [ALTITUDE_KEY]: 'pro' })
  assert.equal(readAltitude(remembered), 'workbench')
  assert.deepEqual(remembered.writes, [], 'reading an old choice does not write')
  assert.equal(altitudeFromSearch('?altitude=pro'), 'workbench')
  assert.equal(initialAltitude({ search: '?altitude=pro', storage: store() }), 'workbench')
  assert.equal(rememberAltitude(remembered, 'pro', { chosen: true }), false)
  assert.equal(rememberAltitude(remembered, 'workbench', { chosen: true }), true)
  assert.deepEqual(remembered.writes, [[ALTITUDE_KEY, 'workbench']])
  const at = (key, target = { tagName: 'DIV' }, extra = {}) => keyAction({ key, target, ...extra })
  assert.deepEqual(['1', '2', '3'].map((key) => at(key)), ['brief', 'explore', 'workbench'])
  for (const key of ['b', 'e', 'p']) assert.equal(at(key), null, `${key} is not an altitude key`)
  assert.equal(at('3', { tagName: 'INPUT' }), null)
  assert.equal(at('3', undefined, { shiftKey: true }), 'workbench', 'AZERTY digit')
  assert.equal(at('3', undefined, { ctrlKey: true }), null)
})

test('three altitudes, Brief first and by default', () => {
  assert.deepEqual([...ALTITUDES], ['brief', 'explore', 'workbench'])
  assert.equal(DEFAULT_ALTITUDE, 'brief')
  assert.match(ALTITUDE_KEY, /^thruline\./, 'one namespaced key')
})

test('no storage, a blocked storage, or a value this build does not know all read as Brief', () => {
  assert.equal(readAltitude(null), 'brief')
  assert.equal(readAltitude(undefined), 'brief')
  assert.equal(readAltitude(blocked), 'brief')
  assert.equal(readAltitude(store({ [ALTITUDE_KEY]: 'expert' })), 'brief')
  assert.equal(readAltitude(store({ [ALTITUDE_KEY]: 'explore' })), 'explore')
  assert.equal(readAltitude(store({ [ALTITUDE_KEY]: 'workbench' })), 'workbench')
})

test('only a choice is remembered: a render, a default or an address writes nothing', () => {
  const s = store()
  assert.equal(rememberAltitude(s, 'workbench'), false)
  assert.equal(rememberAltitude(s, 'workbench', {}), false)
  assert.equal(rememberAltitude(s, 'workbench', { chosen: 'yes' }), false)
  assert.deepEqual(s.writes, [])
  assert.equal(rememberAltitude(s, 'explore', { chosen: true }), true)
  assert.deepEqual(s.writes, [[ALTITUDE_KEY, 'explore']])
  assert.equal(rememberAltitude(s, 'expert', { chosen: true }), false, 'an unknown altitude is not stored')
  assert.equal(rememberAltitude(blocked, 'workbench', { chosen: true }), false, 'a blocked store fails quietly')
})

test('nobody who never chose Workbench is remembered as Workbench, even after opening a ?altitude=workbench address', () => {
  const s = store()
  assert.equal(altitudeFromSearch('?altitude=workbench'), 'workbench')
  assert.equal(initialAltitude({ search: '?altitude=workbench', storage: s }), 'workbench', 'the address shows Workbench on that page')
  assert.deepEqual(s.writes, [], 'and writes nothing')
  assert.equal(initialAltitude({ search: '', storage: s }), 'brief', 'so the next page opens at Brief')
  assert.equal(altitudeFromSearch('?altitude=root'), null)
  assert.equal(initialAltitude({ search: '?altitude=root', storage: store({ [ALTITUDE_KEY]: 'explore' }) }), 'explore', 'a bad address falls back to the remembered choice')
})

test('the §6 map, as data: every key is declared, and since P6-3 none waits — J, K and F act with the Trace', () => {
  const keys = KEYMAP.flatMap((r) => r.keys)
  for (const k of ['j', 'k', '/', '.', 't', 'f', '1', '2', '3', 'Escape', '?']) assert.ok(keys.includes(k), `§6 key ${k} is not declared`)
  for (const k of ['b', 'e', 'p']) assert.ok(!keys.includes(k), `${k} still chooses an altitude`)
  assert.equal(new Set(keys).size, keys.length, 'a key is declared twice')
  assert.deepEqual(KEYMAP.filter((r) => !r.wired).flatMap((r) => r.keys).sort(), [])
  assert.ok(Object.isFrozen(KEYMAP) && KEYMAP.every(Object.isFrozen))
})

test('a key maps to its action, in either case', () => {
  const at = (key, extra) => keyAction({ key, target: { tagName: 'DIV' }, ...extra })
  assert.equal(at('1'), 'brief')
  assert.equal(at('2'), 'explore')
  assert.equal(at('3'), 'workbench')
  assert.equal(at('T'), 'register', 'Caps Lock is not a modifier')
  assert.equal(at('t'), 'register')
  assert.equal(at('.'), 'primary')
  assert.equal(at('/'), 'ask')
  assert.equal(at('?', { shiftKey: true }), 'help', 'Shift is how most keyboards type ?')
  assert.equal(at('Escape'), 'close')
  assert.equal(at('j'), 'walk')
  assert.equal(at('f'), 'thread')
  assert.equal(at('x'), null)
})

test('nothing fires while a person is typing, or with a modifier held', () => {
  for (const tagName of ['INPUT', 'TEXTAREA', 'SELECT', 'input']) {
    assert.equal(keyAction({ key: '3', target: { tagName } }), null, tagName)
  }
  assert.equal(keyAction({ key: '3', target: { tagName: 'DIV', isContentEditable: true } }), null, 'contenteditable')
  assert.equal(keyAction({ key: '3', target: { tagName: 'SPAN', closest: (sel) => (/contenteditable/.test(sel) ? {} : null) } }), null, 'inside a contenteditable')
  for (const mod of ['ctrlKey', 'altKey', 'metaKey']) assert.equal(keyAction({ key: '3', target: {}, [mod]: true }), null, mod)
  assert.equal(keyAction({ key: 'T', target: {}, shiftKey: true }), null, 'Shift+T is not T')
  assert.equal(keyAction({ key: '3', target: {}, isComposing: true }), null, 'an IME composition')
  assert.equal(isTyping(null), false)
})

function fakeWindow() {
  const listeners = []
  return {
    listeners,
    addEventListener: (type, fn) => listeners.push([type, fn]),
    removeEventListener: (type, fn) => { const i = listeners.findIndex((l) => l[0] === type && l[1] === fn); if (i >= 0) listeners.splice(i, 1) },
    press(e) { for (const [, fn] of [...listeners]) fn({ target: { tagName: 'BODY' }, preventDefault() { this.prevented = true }, ...e }) },
  }
}

test('the key handler is installed once per window, however many screens ask', () => {
  const win = fakeWindow()
  const seen = []
  const first = installKeys(win, (a) => seen.push('first:' + a))
  const second = installKeys(win, (a) => seen.push('second:' + a))
  assert.equal(win.listeners.length, 1)
  win.press({ key: '2' })
  assert.deepEqual(seen, ['first:explore'])
  second()
  assert.equal(win.listeners.length, 1, "the second caller's cleanup removes nothing it did not install")
  first()
  assert.equal(win.listeners.length, 0)
  const third = installKeys(win, (a) => seen.push('third:' + a))
  win.press({ key: '1' })
  assert.deepEqual(seen, ['first:explore', 'third:brief'], 'the slot is free again after cleanup')
  third()
})

test('a key the map does not own reaches nothing and is not swallowed', () => {
  const win = fakeWindow()
  const seen = []
  const off = installKeys(win, (a) => seen.push(a))
  const e = { key: ']' }
  win.press(e)
  assert.deepEqual(seen, [])
  off()
})

/* A tiny document: one box marked as the screen's primary, and what is in it. */
function docWith(control, reasons) {
  const box = {
    matches: () => false,
    querySelector: (sel) => (control && sel.split(',').some((s) => control.classes.includes(s.trim().replace(/^\./, ''))) ? control : null),
  }
  return {
    querySelector: (sel) => (sel === '[data-primary-action]' ? box : null),
    getElementById: (id) => (reasons && reasons[id] ? { textContent: reasons[id] } : null),
  }
}
const control = (over) => {
  const c = { classes: ['ws-change-primary'], disabled: false, attrs: {}, clicks: 0, ...over }
  c.getAttribute = (n) => (n in c.attrs ? c.attrs[n] : null)
  c.click = () => { c.clicks += 1 }
  return c
}

test('. presses the one primary when it is on', () => {
  const c = control()
  assert.deepEqual(pressPrimary(docWith(c)), { pressed: true, reason: null })
  assert.equal(c.clicks, 1)
})

test('. does not press an off primary; it hands back the reason and what would enable it', () => {
  const c = control({ disabled: true, attrs: { 'aria-describedby': 'why' } })
  assert.deepEqual(pressPrimary(docWith(c, { why: 'No version is bound. Bind one to approve.' })), { pressed: false, reason: 'No version is bound. Bind one to approve.' })
  assert.equal(c.clicks, 0)
})

test('. never guesses: with no marked box, or no primary in it, it presses nothing', () => {
  assert.deepEqual(pressPrimary({ querySelector: () => null }), { pressed: false, reason: null })
  const plain = control({ classes: ['ws-change-secondary'] })
  assert.deepEqual(pressPrimary(docWith(plain)), { pressed: false, reason: null })
  assert.equal(plain.clicks, 0, 'a secondary control in the box is not the primary')
  assert.deepEqual(pressPrimary(null), { pressed: false, reason: null })
})

/* ── N13-KEYS-214 · WCAG 2.2 SC 2.1.4: single-key shortcuts can be switched off ── */

test('2.1.4: the single-key switch is on by default, remembered under one namespaced key, and reads as on when storage is blocked', () => {
  assert.equal(typeof ALT.SINGLE_KEYS_KEY, 'string', 'altitude.js declares SINGLE_KEYS_KEY')
  assert.match(ALT.SINGLE_KEYS_KEY, /^thruline\.shell\./)
  assert.equal(ALT.readSingleKeys(null), true, 'no storage reads as on')
  assert.equal(ALT.readSingleKeys(store()), true, 'nothing remembered reads as on')
  assert.equal(ALT.readSingleKeys(blocked), true, 'a blocked storage reads as on')
  assert.equal(ALT.readSingleKeys(store({ [ALT.SINGLE_KEYS_KEY]: 'nonsense' })), true, 'a value this build does not know reads as on')
  const s = store()
  assert.equal(ALT.rememberSingleKeys(s, false), true)
  assert.deepEqual(s.writes, [[ALT.SINGLE_KEYS_KEY, 'off']])
  assert.equal(ALT.readSingleKeys(s), false)
  ALT.rememberSingleKeys(s, true)
  assert.equal(ALT.readSingleKeys(s), true)
  assert.equal(ALT.rememberSingleKeys(blocked, false), false, 'a blocked storage writes nothing and does not throw')
})

test('2.1.4: with the switch off every printable single-key row answers nothing; Cmd+K, Ctrl+K and Esc still act', () => {
  const at = (key, singleKeys, extra) => keyAction({ key, target: { tagName: 'DIV' }, ...extra }, { singleKeys })
  const printable = KEYMAP.filter((r) => !r.combo).flatMap((r) => r.keys).filter((k) => k.length === 1)
  assert.deepEqual(printable.slice().sort(), ['.', '/', '1', '2', '3', '?', 'f', 'j', 'k', 't'])
  for (const k of printable) {
    const extra = k === '?' ? { shiftKey: true } : {}
    assert.notEqual(at(k, true, extra), null, `${k} acts with the switch on`)
    assert.equal(at(k, false, extra), null, `${k} answers nothing with the switch off`)
  }
  assert.equal(at('k', false, { metaKey: true }), 'ask', 'Cmd+K is a chord, not a character key')
  assert.equal(at('k', false, { ctrlKey: true }), 'ask', 'Ctrl+K is a chord, not a character key')
  assert.equal(at('Escape', false), 'close', 'Esc is not a printable character')
  assert.equal(at('2', undefined), 'explore', 'no switch handed in means on')
})

test('2.1.4: the installed handler reads the switch on every press — this window first, then storage — and the workspace keys ask it too', () => {
  const win = fakeWindow()
  const s = store()
  win.localStorage = s
  const seen = []
  const off = installKeys(win, (a) => seen.push(a))
  assert.equal(ALT.singleKeysOn(win), true)
  win.press({ key: '2' })
  ALT.setSingleKeys(win, false)
  assert.deepEqual(s.writes, [[ALT.SINGLE_KEYS_KEY, 'off']], 'the choice is remembered')
  assert.equal(ALT.singleKeysOn(win), false)
  win.press({ key: '3' })
  win.press({ key: 'k', metaKey: true })
  win.press({ key: 'Escape' })
  assert.deepEqual(seen, ['explore', 'ask', 'close'])
  ALT.setSingleKeys(win, true)
  win.press({ key: '1' })
  assert.deepEqual(seen, ['explore', 'ask', 'close', 'brief'])
  off()
  /* A window whose storage throws still holds the choice for this page. */
  const bare = fakeWindow()
  Object.defineProperty(bare, 'localStorage', { get() { throw new Error('SecurityError') } })
  assert.equal(ALT.singleKeysOn(bare), true, 'blocked storage reads as on')
  ALT.setSingleKeys(bare, false)
  assert.equal(ALT.singleKeysOn(bare), false, 'the switch still works for this page when nothing can be remembered')
  assert.equal(ALT.singleKeysOn(null), true)
})
