/* Settings' content arrives only through `data` (demo.js is its shape).
 * The guard: no fact of the demo record is written into the markup or the
 * logic, outside the design-only parts the gallery alone draws. */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { DEMO } from './demo.js'
import { contentLeaks } from './content-guard.js'

const here = (f) => readFileSync(new URL(f, import.meta.url), 'utf8')

// vocabulary and settings values the product uses whatever the record says:
// preference values, the product's own store file names (the Technical line
// says which holds what), and a hook name that is also a word ("Notifications")
const VOCABULARY = new Set(['comfortable', 'system', 'plain', 'ledger.db', 'exec.db', 'Notification'])

test('Settings: no demo fact is written into the markup outside design-only parts', () => {
  assert.deepEqual(contentLeaks(here('./template.jsx'), DEMO, { skip: VOCABULARY, ignoreKeys: ['variants'] }), [])
})

test('Settings: no demo fact is written into the logic', () => {
  assert.deepEqual(contentLeaks(here('./logic.js'), DEMO, { skip: VOCABULARY, ignoreKeys: ['variants'], source: 'js' }), [])
})

test('Settings: every canvas note is drawn from DEMO only, and only in the gallery', () => {
  const t = here('./template.jsx')
  const logic = here('./logic.js')
  const notes = Object.values(DEMO.designNotes)
  assert.equal(notes.length, 8)
  // the notes live in demo.js, never in the markup
  for (const n of notes) assert.ok(!t.includes(n), 'note written into the markup: ' + n.slice(0, 40))
  // the markup draws a note slot only when it holds something
  for (const k of Object.keys(DEMO.designNotes).filter((k) => k !== 'excl' && k !== 'checks')) {
    assert.ok(t.includes('{v?.notes?.' + k + ' ? (<>'), 'slot ' + k + ' is conditional')
  }
  // the logic fills the slots from DEMO only when no record was given
  assert.match(logic, /const notes = isDemo \? Object\.assign\(\{\}, DEMO\.designNotes\) : \{\};/)
  // the DEMO STATE panel is still gated
  const i = t.indexOf('aria-label="Demo state"')
  const gate = t.lastIndexOf('{v?.isDemo ? (<>', i)
  assert.ok(gate > 0 && t.indexOf('</>) : null}', gate) > i)
})

test('Settings: the live words never say trial, engine, route or endpoint', () => {
  const live = here('./live.js')
  const words = [...live.matchAll(/'([^'\\]|\\.)*'/g)].map((m) => m[0])
  for (const w of words) assert.doesNotMatch(w, /\btrial\b|\bengine\b|\broute\b|\bendpoint\b|Needs engine/i, w)
})
