/* W1-D, HQ Next wave 1: Approve-Sheet's content arrives through one object.
 * The design gallery (no `data`) still draws the canvas word for word, with its
 * demo switcher and design notes; the live path (`data` present) draws only what
 * `data` says and none of the canvas's design annotations. The pixel match
 * against docs/hq-next/reference/Approve-Sheet.png is checked by
 * scripts/hq/pixdiff.py, not here. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))

const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
import { DEMO } from './demo.js'
export { DEMO }
export function render(props, state) {
  const inst = new Logic(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'approve-sheet-data-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const html = (props, state) => unescape(M.render(props, state))
const clone = (x) => JSON.parse(JSON.stringify(x))
const ANNOTATIONS = ['Demo state', 'Illustrative', 'Needs engine', 'Start over', 'Code moves while open']

/* One live record per demo variant: the variant's facts carried at the top level, as the store would. */
function live(...variants) {
  const d = clone(M.DEMO)
  for (const v of variants) Object.assign(d, d.states[v])
  delete d.states
  delete d.demoTimes
  return d
}
const APPROVED = { history: [{ kind: 'approved', time: '16:10', name: 'Sam', note: 'checked by hand' }] }
const WITHDRAWN = { history: [{ kind: 'approved', time: '16:10', name: 'Sam', note: '' }, { kind: 'withdrawn', time: '16:14', name: 'Sam', note: '' }] }
const CASES = [
  ['ready', [], undefined], ['free', ['free'], undefined], ['refused', ['moves'], { movedRefused: true }],
  ['approved', [], APPROVED], ['approved then moved', ['moves'], APPROVED], ['withdrawn', [], WITHDRAWN], ['free and approved', ['free'], APPROVED],
]

const SWAPS = [
  ['e71c5b0', 'aa11bb2'], ['3f9a1c2', 'ee55ff6'], ['a1b2c3d', 'bb22cc3'], ['4b19a0c', 'dd44ee5'], ['9f2ad3e', 'cc33dd4'],
  ['checkout-web', 'ledger-api'], ['Fix double-applied discount on checkout total', 'Stop the nightly import double-posting'],
  ['Claude Code', 'Codex'], ['Checkout total fixed.', 'Import fixed.'], ['Tests pass.', 'Suite green.'],
  ['checkout/api.py', 'ledger/post.py'], ['cart/totals.py', 'ledger/book.py'], ['tests/test_totals.py', 'tests/test_post.py'], ['checkout/', 'ledger/'],
  ['rm -rf .pytest_cache && pytest -q', 'make clean && make check'], ['~/code/checkout-web', '~/code/ledger-api'],
  ['Applying a discount code once reduces the total by exactly that discount.', 'Each import row posts once.'],
  ['Checkout totals with no discount code are unchanged.', 'Imports with no duplicates are unchanged.'],
  ['the checkout total is wrong when a discount code is applied — fix it', 'the import posts twice'],
  ['1 Oct 2026', '3 Nov 2026'], ['Sam', 'Robin'],
]
function swapped(variants) {
  let s = JSON.stringify(live(...variants))
  for (const [a, b] of SWAPS) s = s.split(a).join(b)
  const d = JSON.parse(s)
  d.run.step = 162; d.statements.forEach((x) => { x.step = 152 }); d.pending.step = 160; d.request.step = 106
  d.trace.edits.forEach((e) => { e.step += 100 }); d.trace.runs.forEach((r) => { r.step += 100 })
  if (d.moved) d.moved.step = 164
  return d
}
const swapState = (st) => (st ? JSON.parse(JSON.stringify(st).split('Sam').join('Robin')) : st)

test('the design gallery still draws the canvas, with its annotations', () => {
  const out = html()
  for (const a of ['Demo state', 'Illustrative']) assert.ok(out.includes(a), a)
  for (const c of ['checkout-web', 'Approve', 'e71c5b0', 'Approving a version needs Pro.', '1.4 GB', 'value="Sam"']) assert.ok(out.includes(c), c)
})

test('every demo variant is reachable from DEMO alone', () => {
  assert.ok(html(undefined, { plan: 'free' }).includes('Approving a version needs Pro.'))
  assert.ok(!/trial/i.test(html(undefined, { plan: 'free' })), 'C20: no trial in the design either')
  assert.ok(html(undefined, { moves: true, movedRefused: true }).includes('step 64 · 16:11 · now a1b2c3d'))
})

test('the live path draws no design annotation, in any case', () => {
  for (const [name, variants, st] of CASES) {
    const out = html({ data: live(...variants) }, st)
    for (const a of ANNOTATIONS) assert.ok(!out.includes(a), `${name}: ${a}`)
  }
})

test('the live path draws only what data says: no canvas value survives a swap', () => {
  for (const [name, variants, st] of CASES) {
    const out = html({ data: swapped(variants) }, swapState(st))
    for (const [a] of SWAPS) assert.ok(!out.includes(a), `${name}: canvas value "${a}" is still drawn`)
    for (const step of ['step 52', 'step 62', 'step 60', 'step 6', 'step 16', 'step 26', 'step 64']) {
      assert.ok(!new RegExp(step + '(?![0-9])').test(out), `${name}: ${step} is still drawn`)
    }
  }
})

test('a stand-in title flows from props.data into the screen', () => {
  const d = live()
  d.change.title = 'A title the canvas never had'
  assert.ok(html({ data: d }).includes('A title the canvas never had'))
})
