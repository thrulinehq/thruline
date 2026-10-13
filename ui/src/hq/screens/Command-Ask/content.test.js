/* Command-Ask's content arrives only through `data` (demo.js is its shape).
 * The guard: no fact of the demo record is written into the markup or the
 * logic, outside the design-only parts the gallery alone draws. If a demo
 * value reappears as a literal, the live screen would show demo content. */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { DEMO } from './demo.js'
import { contentLeaks } from './content-guard.js'

const here = (f) => readFileSync(new URL(f, import.meta.url), 'utf8')

// vocabulary the product uses whatever the record says (state words), not facts of this record
const VOCABULARY = new Set(['Not proven', 'No result yet', 'not_proven', 'no_result', 'prepared', 'changed', 'Claude Code'])

test('Command-Ask: no demo fact is written into the markup outside design-only parts', () => {
  assert.deepEqual(contentLeaks(here('./template.jsx'), DEMO, { skip: VOCABULARY, ignoreKeys: ['presets'] }), [])
})

test('Command-Ask: no demo fact is written into the logic', () => {
  assert.deepEqual(contentLeaks(here('./logic.js'), DEMO, { skip: VOCABULARY, ignoreKeys: ['presets'], source: 'js' }), [])
})

test('Command-Ask: the design annotations and the backdrop are gated on isDemo', () => {
  const t = here('./template.jsx')
  for (const marker of ['aria-label="Demo state, for design review only"', 'Change · 1 session · Claude Code']) {
    const i = t.indexOf(marker)
    assert.ok(i > 0, marker)
    const gate = t.lastIndexOf('{v?.isDemo ? (<>', i)
    const close = t.indexOf('</>) : null}', gate)
    assert.ok(gate > 0 && close > i, `${marker} is inside an isDemo gate`)
  }
})

test('Command-Ask: the overlay renders only the palette and its scrim', () => {
  const t = here('./template.jsx')
  const start = t.indexOf('if (v?.overlay) {')
  const end = t.indexOf('\n  }', start)
  assert.ok(start > 0 && end > start)
  const block = t.slice(start, end)
  assert.match(block, /\{palette\}/)
  assert.doesNotMatch(block, /<header|<footer|Demo state/)
  assert.match(block, /position: "fixed"/)
  assert.match(block, /onClick=\{v\?\.onClose\}/)
})
