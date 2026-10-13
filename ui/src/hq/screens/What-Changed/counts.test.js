/* C29: every count of 1,000 or more on What-Changed reads with thousands
 * separators ("1,679"), while identifiers — a step number, a line number in a
 * file — stay bare. Live path only: the design gallery draws DEMO, whose
 * numbers are all under 1,000. */
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
import { whatChangedFromChange, withServedHunks } from './from-change.js'
import { changeSnapshot } from '../../data/change.js'
import { threadFixture } from './thread-fixture.js'
export { whatChangedFromChange as select, withServedHunks, changeSnapshot, threadFixture }
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
  const file = join(await mkdtemp(join(tmpdir(), 'what-changed-counts-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => {
  const h = M.render(props, state)
  const attrs = (h.match(/(?:aria-label|title)="[^"]*"/g) || []).map((a) => a.replace(/^[^"]*"|"$/g, '')).join('\n')
  return unescape(h.replace(/<[^>]*>/g, '\u0001') + '\n' + attrs)
}
const NOW = Date.parse('2026-10-07T10:30:00.000Z')
const STEP = 1660

/* The served ThreadView with its counts raised past 1,000 — the request-read
 * edit adds 1,234 lines and the test run's tally is 1,679 passed — and, after
 * selection, every step number moved past 1,000 too, which must stay bare. */
const big = () => {
  const view = M.threadFixture()
  const e2 = view.edits[1]
  e2.hunks[0].lines = Array.from({ length: 1234 }, (_, i) => ({ kind: 'added', text: `line ${i + 1} of the new file` }))
  e2.hunks[0].new_lines = 1234
  e2.lines_added = 1234
  view.commands[0].tally.passed = 1679
  view.commands[0].output_excerpt = ['1679 passed in 0.1s']
  view.commands[0].tally_line = '1679 passed in 0.1s'
  const d = M.select(M.withServedHunks(M.changeSnapshot(view, null, NOW), view))
  const up = (x) => (x == null ? x : x + STEP)
  for (const xs of [d.runs, d.edits, d.statements || [], d.messages || []]) for (const x of xs) if (x && x.step != null) x.step = up(x.step)
  for (const v of d.versions || []) { v.madeByStep = up(v.madeByStep); v.recordedAtStep = up(v.recordedAtStep) }
  return d
}

test('lines added and a run tally are grouped', () => {
  const t = text({ data: big() }, { file: 'src/api.py', from: 'base' })
  assert.ok(t.includes('+1,234 −0'), 'lines added')
  assert.ok(t.includes('1,679 passed'), 'tally')
  assert.ok(!t.includes('+1234') && !/(^|[^\d,])1679 passed, /.test(t), 'bare count')
  assert.ok(t.includes('line 1234 of the new file'), 'a line of the file is its own text, untouched')
})

test('a step number of 1,000 or more is an identifier and stays bare', () => {
  const d = big()
  const t = text({ data: d }, { file: 'src/api.py', from: 'base' })
  const step = d.runs[0].step
  assert.ok(step >= 1000)
  assert.ok(t.includes('step ' + step), 'step number bare')
  assert.ok(!/step \d,\d{3}/.test(t), 'a step number grouped')
})
