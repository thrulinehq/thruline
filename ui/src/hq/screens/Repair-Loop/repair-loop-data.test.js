/* W1-D, HQ Next wave 1: Repair-Loop's content arrives through one object.
 * The design gallery (no `data`) still draws the canvas word for word, with its
 * demo switcher and design notes; the live path (`data` present) draws only what
 * `data` says and none of the canvas's design annotations. The pixel match
 * against docs/hq-next/reference/Repair-Loop.png is checked by
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
  const file = join(await mkdtemp(join(tmpdir(), 'repair-loop-data-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const html = (props, state) => unescape(M.render(props, state))
const clone = (x) => JSON.parse(JSON.stringify(x))
const ANNOTATIONS = ['Demo state', 'Design note', 'NEEDS ENGINE']
const VARIANTS = ['running', 'passed', 'failed', 'stopped', 'na']

/* One live record per variant: the variant's facts carried at the top level, as the store would. */
function live(variant) {
  const d = clone(M.DEMO)
  Object.assign(d, d.states[variant])
  delete d.states
  return d
}

/* Every distinctive canvas value, swapped for one the canvas never shows. */
const SWAPS = [
  ['e71c5b0', 'aa11bb2'], ['9f2ad3e', 'cc33dd4'], ['3f9a1c2', 'ee55ff6'],
  ['checkout-web', 'ledger-api'], ['Fix double-applied discount on checkout total', 'Stop the nightly import double-posting'],
  ['Claude Code', 'Codex'], ['Checkout total fixed. Tests pass.', 'Import fixed. Suite green.'], ['Tests pass.', 'Suite green.'],
  ['checkout/api.py', 'ledger/post.py'], ['checkout/', 'ledger/'], ['tests/test_totals.py', 'tests/test_post.py'],
  ['~/checkout-web/.venv', '~/ledger-api/.venv'], ['the checkout total is wrong when a discount code is applied — fix it', 'the import posts twice'],
]
function swapped(variant) {
  let s = JSON.stringify(live(variant))
  for (const [a, b] of SWAPS) s = s.split(a).join(b)
  const d = JSON.parse(s)
  d.statement.step = 152; d.heldRun.step = 146; d.movedEdit.step = 156
  if (d.run.step) d.run.step = 162
  return d
}

test('the design gallery still draws the canvas, with its annotations', () => {
  const out = html()
  for (const a of ['Demo state', 'Design note']) assert.ok(out.includes(a), a)
  for (const c of ['checkout-web', 'Fix double-applied discount on checkout total', 'e71c5b0', '19 steps', '1.4 GB']) assert.ok(out.includes(c), c)
})

test('every demo variant is reachable from DEMO alone', () => {
  for (const v of VARIANTS) assert.ok(M.DEMO.states[v] && M.DEMO.states[v].run, v)
  assert.ok(html(undefined, { demo: 'failed' }).includes('FAILED checkout/tests/test_api.py::test_total_with_discount — assert 90 == 95'))
  assert.ok(html(undefined, { demo: 'passed' }).includes('5 passed in 0.42s'))
})

test('the live path draws no design annotation, in any run state', () => {
  for (const v of VARIANTS) {
    const out = html({ data: live(v) })
    for (const a of ANNOTATIONS) assert.ok(!out.includes(a), `${v}: ${a}`)
  }
})

test('the live path draws only what data says: no canvas value survives a swap', () => {
  for (const v of VARIANTS) {
    const out = html({ data: swapped(v) })
    for (const [a, b] of SWAPS) {
      assert.ok(!out.includes(a), `${v}: canvas value "${a}" is still drawn`)
      assert.ok(out.includes(b), `${v}: "${b}" from data is not drawn`)
    }
    for (const step of ['step 52', 'step 46', 'step 56', 'step 62']) assert.ok(!out.includes(step + ' ') && !out.includes(step + '<') && !out.includes(step + ','), `${v}: ${step} is still drawn`)
  }
})

test('a stand-in title flows from props.data into the screen', () => {
  const d = live('running')
  d.change.title = 'A title the canvas never had'
  assert.ok(html({ data: d }).includes('A title the canvas never had'))
})
