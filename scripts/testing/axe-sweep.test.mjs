/* N13-F11-AXE-SR — the axe sweep's own rules, without a browser.
 *
 * Run: node --test scripts/testing/axe-sweep.test.mjs (it is in ui's
 * `npm test`). The sweep itself needs Chrome and is run by hand:
 * `node scripts/testing/axe-sweep.mjs`.
 */
import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import path from 'node:path'
import { GATE, TAGS, VIEWS, tally, verdict } from './axe-sweep.mjs'
import { COULD_NOT_RUN, FAILED, PASSED } from './a11y-harness.mjs'

const UI = path.resolve(import.meta.dirname, '../../ui')

test('axe-core is a pinned devDependency, loaded from node_modules, never from a CDN', async () => {
  const pkg = JSON.parse(await fs.readFile(path.join(UI, 'package.json'), 'utf8'))
  assert.match(pkg.devDependencies['axe-core'] || '', /^\d+\.\d+\.\d+$/, 'an exact version, not a range')
  const src = await fs.readFile(new URL('./axe-sweep.mjs', import.meta.url), 'utf8')
  assert.match(src, /node_modules\/axe-core\/axe\.min\.js/)
  assert.doesNotMatch(src, /https?:\/\/(?!127\.0\.0\.1)/, 'no URL but the local page')
})

test('the sweep runs WCAG 2.0, 2.1 and 2.2 at A and AA, and gates on serious and critical', () => {
  assert.deepEqual([...TAGS], ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22a', 'wcag22aa'])
  assert.deepEqual([...GATE].sort(), ['critical', 'serious'])
})

test('the views are the ones F11 names: Inbox, the six stages, the three altitudes, and the three sheets', () => {
  const names = VIEWS.map((v) => v.name)
  for (const s of ['define', 'prepare', 'build', 'verify', 'review', 'approve']) assert.ok(names.includes(`Change · ${s}`), s)
  for (const n of ['Inbox', 'Trace · Brief', 'Trace · Explore', 'Trace · Workbench', 'Run sheet', 'Export sheet', 'Key list']) assert.ok(names.includes(n), n)
})

test('passed · failed · could not run: nothing ran is never a pass; serious, critical or an unknown impact fails; moderate and minor are listed, not gated', () => {
  const t = tally([
    { id: 'a', impact: 'moderate', nodes: [{ target: ['#x'] }] },
    { id: 'b', impact: 'minor', nodes: [] },
  ])
  assert.deepEqual(t.counts, { critical: 0, serious: 0, moderate: 1, minor: 1, unknown: 0 })
  assert.equal(t.lines.length, 2, 'moderate and minor are printed')
  assert.equal(verdict({ ran: 3, counts: t.counts }), PASSED)
  assert.equal(verdict({ ran: 0, counts: t.counts }), COULD_NOT_RUN)
  assert.equal(verdict({ ran: 1, counts: tally([{ id: 'c', impact: 'serious', nodes: [] }]).counts }), FAILED)
  assert.equal(verdict({ ran: 1, counts: tally([{ id: 'd', impact: 'critical', nodes: [] }]).counts }), FAILED)
  assert.equal(verdict({ ran: 1, counts: tally([{ id: 'e', impact: null, nodes: [] }]).counts }), FAILED, 'an impact axe did not state is not a pass')
})
