/* W1-C, HQ Next wave 1: Every-Statement's content arrives through one object.
 * The design gallery (no `data`) still draws the canvas; the live path (`data`
 * present) draws only what `data` says and none of the canvas's design
 * annotations. The pixel match against docs/hq-next/reference/Every-Statement.png
 * is checked by scripts/hq/pixdiff.py, not here. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { parse } from '@babel/parser'

const here = dirname(fileURLToPath(import.meta.url))

/* The screen, bundled for node: its canvas logic over its template, rendered
 * to static markup with or without `data` (the live path vs the gallery). */
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
import { DEMO } from './demo.js'
export { DEMO }
export function instance(props, state) {
  const inst = new Logic(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return inst
}
export function render(props, state) {
  const inst = instance(props, state)
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-w1c-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
/** What a person can read or hear: text plus aria-label and title. */
const text = (props, state) => {
  const html = M.render(props, state)
  const attrs = (html.match(/(?:aria-label|title)="[^"]*"/g) || []).map((a) => a.replace(/^[^"]*"|"$/g, '')).join('\n')
  return unescape(html.replace(/<[^>]*>/g, '\u0001') + '\n' + attrs)
}
const clone = (x) => JSON.parse(JSON.stringify(x))

/** Every string literal in template.jsx, with whether it sits under a `v?.isDemo` gate. */
function templateLiterals() {
  const src = readFileSync(join(here, 'template.jsx'), 'utf8')
  const ast = parse(src, { sourceType: 'module', plugins: ['jsx'] })
  const out = []
  const visit = (node, gated) => {
    if (!node || typeof node.type !== 'string') return
    if (node.type === 'ConditionalExpression' && src.slice(node.test.start, node.test.end) === 'v?.isDemo') gated = true
    if (node.type === 'StringLiteral' || node.type === 'JSXText') out.push({ value: node.value, gated })
    for (const k of Object.keys(node)) {
      if (k === 'loc' || k === 'start' || k === 'end') continue
      const c = node[k]
      if (Array.isArray(c)) c.forEach((x) => visit(x, gated))
      else if (c && typeof c === 'object' && typeof c.type === 'string') visit(c, gated)
    }
  }
  visit(ast.program, false)
  return out
}

/* A different Change: every name, version, path, step, time, command and quote
 * of the canvas's demo replaced. If the screen still shows a canvas value with
 * this data, that value was drawn from a literal, not from `data`. */
const SUB = [
  ['checkout/api.py', 'svc/handler.py'], ['checkout/', 'svc/'], ['cart/totals.py', 'lib/sum.py'], ['tests/test_totals.py', 'spec/sum_test.py'],
  ['e71c5b0', 'zzzz111'], ['9f2ad3e', 'yyyy222'], ['4b19a0c', 'xxxx333'], ['3f9a1c2', 'cccc444'],
  ['checkout-web', 'proj-q'], ['Fix double-applied discount on checkout total', 'Make the widget sturdier'],
  ['Claude Code', 'AgentA'], ['Codex', 'AgentB'], ['pytest -q', 'runtests --fast'],
  ['Tests pass.', 'Specs are green.'], ['lint is clean.', 'style check is clean.'], ['Checkout total fixed.', 'Widget mended.'],
  ['Refactored the discount rules', 'Reworked the widget rules'],
  ['The discount is being applied twice. Fixing the second application.', 'Something odd here; mending it.'],
  ['Applying', 'Using'], ['discount', 'widget'], ['Checkout totals', 'Widget sums'], ['checkout', 'wdg'], ['totals', 'sums'],
  ['~/code/', '~/work/'], ['16:0', '09:1'], ['2026-09-26', '2031-01-02'], ['+00:00', '-03:00'],
]
const STEPKEYS = new Set(['step', 'recordedAtStep', 'madeByStep'])
function other(x, key) {
  if (Array.isArray(x)) return x.map((y) => other(y, key))
  if (x && typeof x === 'object') { const o = {}; for (const [k, v] of Object.entries(x)) o[k] = other(v, k); return o }
  if (typeof x === 'number' && STEPKEYS.has(key)) return x + 100
  if (typeof x === 'number' && key === 'recordsBytes') return 2300000000
  if (typeof x === 'number' && key === 'elapsedS') return 7
  if (typeof x === 'string') { let s = x; for (const [a, b] of SUB) s = s.split(a).join(b); return s }
  return x
}
/** Canvas values that must never reach the live path when `data` says otherwise. */
const CANVAS_FACTS = ['e71c5b0', '9f2ad3e', '4b19a0c', '3f9a1c2', 'checkout', 'Fix double', 'Claude Code', 'Codex', 'pytest',
  'totals', 'Tests pass', 'lint is clean', 'discount', '1.4 GB', '16:0', '2026-09', '+00:00', '~/code',
  /\b[Ss]tep (14|16|26|32|36|38|46|52|56|58|60|62)\b/, /[±✕●❝◭⋯] (16|26|36|46|52|56|60|62)\b/]
const leaks = (s) => CANVAS_FACTS.filter((f) => (typeof f === 'string' ? s.includes(f) : f.test(s))).map(String)

/* Every row id and grouping the canvas can show, so a test reaches every branch. */
const STATES = []
for (const sel of ['l1', 'l2', 'l3', 'l4', 'stmt-52-tests', 'stmt-14-lint', 'stmt-52-fixed', 'stmt-14-refactored', 'said-32']) {
  for (const extra of [{}, { groupBy: 'state', alsoOpen: true }, { checking: true }, { sheet: 'tests' }, { sheet: 'lint' }, { stopped: true, menu: true }]) {
    STATES.push({ sel, ...extra })
  }
}
const ANNOTATIONS = ['Illustrative: Codex', 'NEEDS ENGINE', 'stays disabled, with its reason']

test('the gallery draws the canvas from DEMO, annotations included', () => {
  const t = text({})
  for (const s of ['checkout-web', 'Fix double-applied discount on checkout total', '= commit 3f9a1c2', 'read 16:06 ↻',
    'Read at 16:06 on 2026-09-26, UTC+00:00', '4 things still need evidence before e71c5b0 can be approved.',
    '3 need a re-check', '1 in progress', '×3', '×1', 'accepted 16:06; last edit 16:04',
    'pytest -q tests/test_totals.py checkout/ on e71c5b0', 'shares step 46 with lines 1, 2, 4',
    'Claude Code · session 1 · step 52 · 16:03', 'pytest -q tests/test_totals.py: 4 passed, 0 failed',
    'version 9f2ad3e · 1 version older than e71c5b0',
    'Looked at: 2 finished runs (step 26 on 4b19a0c, step 46 on 9f2ad3e) and step 60, which has no result yet.',
    'ThruLine is keeping 1.4 GB of records on this machine', ...ANNOTATIONS]) {
    assert.ok(t.includes(s), `gallery is missing ${JSON.stringify(s)}`)
  }
})

test('with data, nothing the canvas made up reaches the screen, in any state', () => {
  const data = other(M.DEMO)
  for (const st of STATES) {
    const t = text({ data }, st)
    assert.deepEqual(leaks(t), [], `state ${JSON.stringify(st)}`)
    for (const a of ANNOTATIONS) assert.ok(!t.includes(a), `annotation ${a} shown on the live path in ${JSON.stringify(st)}`)
  }
  const t = text({ data })
  for (const s of ['proj-q', 'Make the widget sturdier', 'zzzz111', '= commit cccc444', 'read 09:16 ↻', 'shares step 146 with lines 1, 2, 4',
    'AgentA · session 1 · step 152 · 09:13', 'runtests --fast spec/sum_test.py: 4 passed, 0 failed', 'version yyyy222 · 1 version older than zzzz111',
    '2.3 GB']) {
    assert.ok(t.includes(s), `live path is missing ${JSON.stringify(s)}`)
  }
})

test('no literal in the template outside the demo gate carries a canvas fact', () => {
  const bad = templateLiterals().filter((l) => !l.gated && leaks(l.value).length)
  assert.deepEqual(bad.map((l) => l.value), [])
})

test('the counts and run results come from typed fields, not from prose', () => {
  const data = clone(M.DEMO)
  data.runs.find((r) => r.step === 46).passed = 9
  data.criteria.lines[0].proof.verdict = 'proven'
  const t = text({ data }, { sel: 'stmt-52-tests' })
  assert.ok(t.includes('pytest -q tests/test_totals.py: 9 passed, 0 failed'))
  assert.ok(t.includes('3 things still need evidence before e71c5b0 can be approved.'))
  assert.ok(t.includes(' · 1 proven on '))
  assert.ok(t.includes('What counts as done (4) · What the agents said (4) · Also said (1)'))
})

test('a statement the store does not know opens the first row, never a made-up one', () => {
  const data = clone(M.DEMO)
  data.selected = 'gone'
  const t = text({ data })
  assert.ok(t.includes('What counts as done · line 1'))
})
