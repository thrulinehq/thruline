/* W1-D, HQ Next wave 1: Done-Criteria's content arrives through one object.
 * The design gallery (no `data`) still draws the canvas word for word, with its
 * demo switcher and design notes; the live path (`data` present) draws only what
 * `data` says and none of the canvas's design annotations. The pixel match
 * against docs/hq-next/reference/Done-Criteria.png is checked by
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
  const file = join(await mkdtemp(join(tmpdir(), 'done-criteria-data-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const html = (props, state) => unescape(M.render(props, state))
const clone = (x) => JSON.parse(JSON.stringify(x))
const ANNOTATIONS = ['Demo state', 'Design note', 'shown as designed', 'One dropped', 'v1 recorded']
const VARIANTS = ['fresh', 'reviewing', 'dropped', 'recorded']

/* One live record per demo variant: each line's decision and link, and whether the version is recorded, on `criteria`. */
function live(variant) {
  const d = clone(M.DEMO)
  const v = d.states[variant]
  d.criteria.lines.forEach((l, i) => { l.state = v.lines[i]; l.linked = v.links[i] })
  d.criteria.recorded = v.recorded
  if (v.recorded) d.criteria.acceptedAt = '16:06'
  delete d.states
  delete d.demoTimes
  return d
}
const UI = [undefined, { face: 'technical' }, { tplOpen: true }, { composer: 'line' }, { composer: 'scope' }, { editing: 1, draft: 'x' }]

const SWAPS = [
  ['e71c5b0', 'aa11bb2'], ['3f9a1c2', 'ee55ff6'], ['4b19a0c', 'dd44ee5'], ['9f2ad3e', 'cc33dd4'],
  ['checkout-web', 'ledger-api'], ['Fix double-applied discount on checkout total', 'Stop the nightly import double-posting'],
  ['Claude Code', 'Codex'], ['Checkout total fixed.', 'Import fixed.'],
  ['checkout/api.py', 'ledger/post.py'], ['tests/test_totals.py', 'tests/test_post.py'], ['cart/', 'book/'], ['checkout/', 'ledger/'],
  ['Applying a discount code once reduces the total by exactly that discount.', 'Each import row posts once.'],
  ['Applying the same discount code twice does not reduce the total twice.', 'Re-running an import posts nothing new.'],
  ['Checkout totals with no discount code are unchanged.', 'Imports with no duplicates are unchanged.'],
  ['the checkout total is wrong when a discount code is applied — fix it', 'the import posts twice'],
  ['test_single_discount', 'test_single_post'], ['test_double_discount', 'test_double_post'], ['test_total_plain', 'test_plain_import'],
  ['Bug fix: reproduces, fixed, no regression', 'Import rule A'], ['1.4 GB', '__'],
  ['16:01', '09:01'], ['16:03', '09:03'], ['16:04', '09:04'], ['16:06', '09:06'],
  ['fix it', 'sort it out'], ['fixed', 'sorted'],
]
function swapped(variant) {
  let s = JSON.stringify(live(variant))
  for (const [a, b] of SWAPS) s = s.split(a).join(b)
  const d = JSON.parse(s)
  d.steps = 119; d.storage.bytes = 3 * 1024 * 1024 * 1024
  d.request.step = 106; d.claim.step = 152; d.agent.lastEdit.step = 156; d.movedEdit.step = 156
  d.lastRun.step = 146; d.lastRun.startStep = 142; d.earlierRun.step = 126; d.earlierRun.startStep = 122; d.pendingRun.step = 160
  d.criteria.lines[3].ranAtSteps = [122, 142]; d.criteria.lines[3].source.steps = [122, 142]
  d.covers[0].editSteps = [116, 136]; d.covers[1].editSteps = [156]
  return d
}

test('the design gallery still draws the canvas, with its annotations', () => {
  const out = html()
  for (const a of ['Demo state', 'Design note']) assert.ok(out.includes(a), a)
  assert.ok(html(undefined, { tplOpen: true }).includes('needs engine: shown as designed'))
  for (const c of ['checkout-web', 'e71c5b0', '19 steps', '1.4 GB', '2 of 4 accepted']) assert.ok(out.includes(c), c)
})

test('the live path draws no design annotation, in any variant or panel', () => {
  for (const v of VARIANTS) for (const st of UI) {
    const out = html({ data: live(v) }, st)
    for (const a of ANNOTATIONS) assert.ok(!out.includes(a), `${v} ${JSON.stringify(st)}: ${a}`)
  }
})

test('the live path draws only what data says: no canvas value survives a swap', () => {
  for (const v of VARIANTS) for (const st of UI) {
    const out = html({ data: swapped(v) }, st)
    for (const [a] of SWAPS) assert.ok(!out.includes(a), `${v} ${JSON.stringify(st)}: canvas value "${a}" is still drawn`)
    for (const n of ['6', '16', '19', '22', '26', '36', '42', '46', '52', '56', '60']) {
      assert.ok(!new RegExp('(step|steps|Step) ' + n + '(?![0-9])').test(out), `${v} ${JSON.stringify(st)}: step ${n} is still drawn`)
    }
  }
})

test('a stand-in title flows from props.data into the screen', () => {
  const d = live('reviewing')
  d.change.title = 'A title the canvas never had'
  assert.ok(html({ data: d }).includes('A title the canvas never had'))
})
