/* W1-C, HQ Next wave 1: What-Changed's content arrives through one object.
 * The design gallery (no `data`) still draws the canvas, including its played-out
 * "Check it again"; the live path (`data` present) draws only what `data` says,
 * never fabricates a run, and shows none of the canvas's design annotations.
 * The pixel match against docs/hq-next/reference/What-Changed.png is checked by
 * scripts/hq/pixdiff.py, not here. */
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

const STATES = []
for (const file of ['checkout/api.py', 'cart/totals.py', 'tests/test_totals.py']) {
  for (const from of ['base', 'v1', 'proof']) {
    for (const run of ['idle', 'checking', 'done']) STATES.push({ file, from, run })
  }
}
STATES.push({ stoppedAfter: 3 })
const ANNOTATIONS = ['Demo records.', 'Illustrative, not captured', 'engine:', 'diff route not built yet', 'illustrative', 'engine work, not built yet']

test('the gallery draws the canvas from DEMO, annotations and the played-out check included', () => {
  const t = [{}, { file: 'cart/totals.py', from: 'base' }, { file: 'tests/test_totals.py', run: 'done' }, { run: 'done' }].map((st) => text({}, st)).join('\n')
  for (const s of ['checkout-web', '(current) · = commit 3f9a1c2 · no unsaved changes', 'Recording · following live',
    '1 file changed since the proof ran, and nothing has checked it.', '@@ -41,7 +41,8 @@', '@@ -59,5 +60,6 @@',
    '@@ -12,4 +12,5 @@', '@@ -24,5 +25,3 @@', 'step 56 · 16:04 ›', '+3 −1', '+1 −2', '16, 36',
    'Compare from 9f2ad3e, the version the proof ran on: the edit at step 36, the run at step 46 that held, 4 passed, 0 failed, and the claim at step 52.',
    'Looked at: 2 finished runs (step 26 on 4b19a0c, step 46 on 9f2ad3e) and step 60, which has no result yet.',
    '1 file changed after step 46, and a check on e71c5b0 now covers it.', 'Step 62 is illustrative', ...ANNOTATIONS]) {
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
  for (const s of ['proj-q', 'Make the widget sturdier', '= commit cccc444', 'svc/handler.py', '+3 −1', 'step 156 · 09:14 ›',
    'Compare from yyyy222, the version the proof ran on: the edit at step 136, the run at step 146 that held, 4 passed, 0 failed, and the claim at step 152.',
    'Looked at: 2 finished runs (step 126 on xxxx333, step 146 on yyyy222) and step 160, which has no result yet.', '2.3 GB']) {
    assert.ok(t.includes(s), `live path is missing ${JSON.stringify(s)}`)
  }
})

test('no literal in the template outside the demo gate carries a canvas fact', () => {
  const bad = templateLiterals().filter((l) => !l.gated && leaks(l.value).length)
  assert.deepEqual(bad.map((l) => l.value), [])
})

test('on the live path, Check it again never plays out a made-up result', () => {
  const inst = M.instance({ data: clone(M.DEMO) })
  inst.renderVals().act.check()
  // wave 2: the live product has no route that runs a check, so the control is disabled and does nothing
  assert.equal(inst.state.run, 'idle')
  assert.equal(inst._t, undefined, 'a timer was scheduled to fabricate a finished run')
  const t = text({ data: clone(M.DEMO) }, { run: 'done' })
  assert.ok(!t.includes('Ready for approval'))
  assert.ok(!t.includes('step 62'))
  assert.ok(t.includes('1 file changed since the proof ran, and nothing has checked it.'))
  // the gallery still plays it out
  const demo = M.instance({})
  demo.renderVals().act.check()
  assert.notEqual(demo._t, undefined)
  clearTimeout(demo._t)
})

test('a finished run on this version in the record is what makes it covered', () => {
  const data = clone(M.DEMO)
  data.runs.push({ id: 'run-70', step: 70, sessionId: 'session-1', time: '16:20', version: 'e71c5b0', command: 'pytest -q', scope: ['tests/'],
    status: 'finished', exitCode: 0, passed: 7, failed: 0, durationMs: null, capturedBy: 'thruline', declaresPaths: false })
  const t = text({ data })
  assert.ok(t.includes('1 file changed after step 46, and a check on e71c5b0 now covers it.'))
  assert.ok(t.includes('covered by step 70 on e71c5b0'))
  assert.ok(t.includes('1 check on e71c5b0: 7 passed, 0 failed'))
  for (const an of ANNOTATIONS) assert.ok(!t.includes(an), `annotation ${an} shown once the record has the run`)
})

test('hunk headers, line numbers and +/− counts come from the hunks themselves', () => {
  const data = clone(M.DEMO)
  const api = data.edits.find((e) => e.file === 'checkout/api.py')
  api.hunks[0].oldStart = 141
  api.hunks[0].newStart = 140
  api.hunks[1].lines.push({ op: 'add', text: '    log(summary)' })
  const t = text({ data })
  assert.ok(t.includes('@@ -141,7 +140,8 @@'))
  assert.ok(t.includes('@@ -59,5 +60,7 @@'))
  assert.ok(t.includes('+4 −1'))
  assert.ok(t.includes('2 hunks'))
})
