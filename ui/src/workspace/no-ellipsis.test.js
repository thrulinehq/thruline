import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

/* T7 (report F5, the truncation half; report line 310: "Never ellipsize a
 * version, a scope or a state word; wrap to two lines").
 *
 * Done when, point 3 (card T7): no version, scope or state word on the
 * Decision, the rests-on list or a proof line is ellipsized at 1512×920,
 * 1024×768 or 390 px. A CSS census rather than three screenshots: every rule
 * in the stylesheet the Change screen and the take-away load is read, at every
 * width (media queries included), so a width that clips cannot hide.
 *
 * Clipping counts whatever it is called: `text-overflow: ellipsis`, a line
 * clamp, or `white-space: nowrap` with `overflow: hidden` (which cuts the text
 * with no mark at all).
 */

const here = dirname(fileURLToPath(import.meta.url))
const CSS = readFileSync(join(here, 'workspace.css'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')

/** The classes that render a version, a scope or a state word on these surfaces. */
const CARRIES = /\.ws-(decision|readiness|cov-|brief-chip|brief-state|brief-scope|change-version|change-conclusion|change-changed-since|inspector|packet|tell|repairs|verdict-chip|change-run)/

function rules(css) {
  const out = []
  for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) out.push({ selector: m[1].trim(), body: m[2] })
  return out
}

const clips = (body) => /text-overflow\s*:\s*ellipsis/i.test(body)
  || /line-clamp/i.test(body)
  || (/white-space\s*:\s*nowrap/i.test(body) && /overflow(-x)?\s*:\s*(hidden|clip)/i.test(body))

test('T7 done-when 3: no rule that renders a version, scope or state word on the Decision, rests-on or proof lines clips its text', () => {
  const all = rules(CSS)
  assert.ok(all.length > 100, 'the census read the stylesheet')
  const carrying = all.filter((r) => CARRIES.test(r.selector))
  assert.ok(carrying.length > 20, 'the census found the rules it is about')
  const clipped = carrying.filter((r) => clips(r.body)).map((r) => r.selector)
  assert.deepEqual(clipped, [], 'a version, scope or state word can be cut off')
})

test('T7 done-when 3: the census is not vacuous — it catches each way of clipping', () => {
  for (const body of ['text-overflow: ellipsis', '-webkit-line-clamp: 2', 'white-space: nowrap; overflow: hidden']) {
    assert.ok(clips(body), body)
  }
  assert.ok(!clips('overflow-wrap: anywhere'))
})

test('T7 done-when 3: none of the components that draw those lines clips by inline style either', () => {
  for (const file of ['change-workspace.jsx', 'packet.jsx', 'criterion-links.jsx', 'verdict-panel.jsx', 'blocked.jsx']) {
    const source = readFileSync(join(here, file), 'utf8')
    assert.doesNotMatch(source, /textOverflow|WebkitLineClamp/, `${file} clips text inline`)
  }
})
