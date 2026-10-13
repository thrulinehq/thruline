/* The look a person chose, applied to the whole app (C30 R-PREFS): the three
 * attributes on the app root, the CSS that follows them, and the ONE token every
 * Proven colour is drawn from (data/palette.js).
 *
 *  1. density, motion and colour vision become three attributes, defaults first;
 *  2. appearance.css re-points the Proven token to the §1.11 teal, and cuts
 *     motion for "Always reduce" and for "Follow this Mac" on a Mac that asks;
 *  3. nobody writes the Proven green as a hex or an rgba again: every use reads
 *     the token, with the standard green as its fallback, so the gallery draws
 *     the canvas exactly;
 *  4. a Mac that asks for less motion, or "Always reduce", scrolls at once. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'
import { appearanceOf, applyAppearance, ATTRIBUTES, rootOf, motionReduced } from './appearance.js'
import { PROVEN, provenA, STANDARD, TEAL } from './palette.js'
import { DEFAULTS } from './preferences.js'

const here = dirname(fileURLToPath(import.meta.url))
const hq = join(here, '..')
const text = (p) => readFileSync(p, 'utf8')

class El {
  constructor() { this.attrs = new Map(); this.sets = 0 }
  getAttribute(k) { return this.attrs.has(k) ? this.attrs.get(k) : null }
  setAttribute(k, v) { this.sets += 1; this.attrs.set(k, v) }
}
const state = (over = {}) => ({ status: 'read', prefs: Object.assign({}, DEFAULTS, over) })

test('1. the three attributes carry the defaults until the choices are read, and the stored values after', () => {
  const el = new El()
  applyAppearance(el, { status: 'reading', prefs: DEFAULTS })
  assert.deepEqual([el.getAttribute('data-density'), el.getAttribute('data-motion'), el.getAttribute('data-colour-vision')], ['comfortable', 'system', 'standard'])
  applyAppearance(el, state({ density: 'compact', motion: 'reduce', colour_vision: 'proven_teal' }))
  assert.deepEqual([el.getAttribute('data-density'), el.getAttribute('data-motion'), el.getAttribute('data-colour-vision')], ['compact', 'reduce', 'proven_teal'])
  const before = el.sets
  applyAppearance(el, state({ density: 'compact', motion: 'reduce', colour_vision: 'proven_teal' }))
  assert.equal(el.sets, before, 'nothing is written that is already there')
  assert.deepEqual(ATTRIBUTES, { density: 'data-density', motion: 'data-motion', colour_vision: 'data-colour-vision' })
})

test('1. a value the route would not take, or no state at all, is the default; no element is a no-op', () => {
  assert.deepEqual(appearanceOf({ prefs: { density: 'huge', motion: 'sometimes', colour_vision: 'blue' } }), { density: 'comfortable', motion: 'system', colour_vision: 'standard' })
  assert.deepEqual(appearanceOf(null), { density: 'comfortable', motion: 'system', colour_vision: 'standard' })
  applyAppearance(null, state())
  applyAppearance({}, state())
})

test('2. appearance.css: "Proven is teal" re-points the one token, and nothing else changes colour', () => {
  const css = text(join(hq, 'appearance.css'))
  const rule = /#root\[data-colour-vision="proven_teal"\]\s*\{([^}]*)\}/.exec(css)
  assert.ok(rule, 'a rule for proven_teal on the app root')
  assert.match(rule[1], new RegExp('--tl-proven:\\s*' + TEAL.hex + ';', 'i'))
  assert.match(rule[1], new RegExp('--tl-proven-rgb:\\s*' + TEAL.rgb.replace(/ /g, '\\s*') + ';'))
  assert.equal(rule[1].split(';').filter((d) => d.trim()).length, 2, 'exactly the two tokens')
  // DESIGN-SPEC §1.11: swap #3DD68C → #2DD4BF (border rgba(45,212,191,.6), fill rgba(45,212,191,.08))
  assert.equal(STANDARD.hex, '#3DD68C')
  assert.equal(TEAL.hex, '#2DD4BF')
  assert.equal(TEAL.rgb, '45, 212, 191')
  assert.equal(css.match(/--tl-proven/g).length, 2, 'the swap names the token in one place only')
})

test('2. appearance.css: "Always reduce" cuts animation and transition; "Follow this Mac" does the same when the Mac asks', () => {
  const css = text(join(hq, 'appearance.css'))
  const reduce = css.slice(css.indexOf('#root[data-motion="reduce"]'), css.indexOf('@media'))
  assert.match(reduce, /animation:\s*none\s*!important/)
  assert.match(reduce, /transition:\s*none\s*!important/)
  assert.match(reduce, /#root\[data-motion="reduce"\] \*/)
  const media = css.slice(css.indexOf('@media (prefers-reduced-motion: reduce)'))
  assert.match(media, /#root\[data-motion="system"\] \*/)
  assert.match(media, /animation:\s*none\s*!important/)
  assert.match(media, /transition:\s*none\s*!important/)
  assert.doesNotMatch(css.slice(0, css.indexOf('@media')).replace(reduce, ''), /system/, 'plain "system" outside the media query would cut motion for everyone')
})

test('3. the token and its opacity forms, with the standard green as the fallback', () => {
  assert.equal(PROVEN, 'var(--tl-proven, #3DD68C)')
  assert.equal(provenA('.6'), 'rgba(var(--tl-proven-rgb, 61, 214, 140), .6)')
  assert.equal(provenA(0.08), 'rgba(var(--tl-proven-rgb, 61, 214, 140), .08)')
})

/* sources under hq/ that may still name the green literally, and why */
const MAY_NAME_IT = {
  'data/palette.js': 'the one definition',
  'screens/Settings/logic.js': 'the preview pill draws BOTH variants by name (green when off, teal when on), whatever the app root says',
  'screens/Change-Brief/decisions.jsx': 'the Decisions lane’s slot; it adopts the token with its own change (reported, not edited here)',
}
const walk = (dir) => readdirSync(dir).flatMap((f) => {
  const p = join(dir, f)
  return statSync(p).isDirectory() ? walk(p) : [p]
})
const RAW = /(?<!var\(--tl-proven, )#3DD68C|rgba\(\s*61\s*,\s*214\s*,\s*140\s*,/i

test('3. no screen writes the Proven green as a hex or an rgba: every use reads the token', () => {
  const offenders = []
  const named = []
  for (const p of walk(hq)) {
    if (!/\.(js|jsx)$/.test(p) || /\.test\.js$/.test(p)) continue
    const rel = relative(hq, p)
    if (RAW.test(text(p))) (MAY_NAME_IT[rel] ? named : offenders).push(rel)
  }
  assert.deepEqual(offenders, [], 'these still draw Proven without the token, so "Proven is teal" would skip them')
  assert.deepEqual(named.sort(), Object.keys(MAY_NAME_IT).sort(), 'an exception that no longer names the green must be removed from the list')
})

test('3. every token use carries the standard green as its fallback (the gallery draws the canvas)', () => {
  let uses = 0
  for (const p of walk(hq)) {
    if (!/\.(js|jsx)$/.test(p) || /\.test\.js$/.test(p) || /data\/palette\.js$/.test(p)) continue
    const src = text(p)
    for (const m of src.matchAll(/var\(--tl-proven(-rgb)?,\s*([^)]*)\)/g)) {
      uses += 1
      assert.equal(m[2], m[1] ? STANDARD.rgb : STANDARD.hex, relative(hq, p) + ': ' + m[0])
    }
    assert.doesNotMatch(src, /var\(--tl-proven(-rgb)?\)/, relative(hq, p) + ' uses the token with no fallback')
  }
  assert.ok(uses > 40, 'the swap reaches every place Proven is drawn (' + uses + ')')
})

test('4. a Mac that asks for less motion, or “Always reduce”, scrolls at once', () => {
  const doc = (motion, media) => {
    const root = new El()
    if (motion) root.setAttribute('data-motion', motion)
    return { getElementById: (id) => (id === 'root' ? root : null), documentElement: new El(), defaultView: { matchMedia: () => ({ matches: media }) } }
  }
  assert.equal(motionReduced(doc('reduce', false)), true)
  assert.equal(motionReduced(doc('system', true)), true)
  assert.equal(motionReduced(doc('system', false)), false)
  assert.equal(motionReduced(doc(null, false)), false, 'the gallery has no attribute and follows the Mac')
  assert.equal(motionReduced(doc(null, true)), true)
  assert.equal(motionReduced(null), false)
  assert.ok(rootOf(doc('system', false)))
})
