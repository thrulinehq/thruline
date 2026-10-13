/* C29: every count of 1,000 or more on the Change-Brief reads with thousands
 * separators ("1,679"), while identifiers — a step number — stay bare. Live
 * path only: the design gallery draws DEMO, whose numbers are all under 1,000,
 * so it is unchanged (pinned by change-brief-data.test.js and the pixel diff). */
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
  const file = join(await mkdtemp(join(tmpdir(), 'change-brief-counts-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state))
const clone = (x) => JSON.parse(JSON.stringify(x))
const OPEN = { why: true, then: true, kind: true, after: true, tech: true }

/* A live snapshot with every count raised past 1,000: the tallies of every run,
 * the lines of what counts as done, the Change's step count and sessions; and
 * the statements' step number raised past 1,000 too, which must stay bare. */
const big = (state) => {
  const d = clone(M.DEMO.states[state])
  for (const r of d.runs) if (r.passed != null) { r.passed = 1679; r.failed = r.failed ? 1203 : 0 }
  if (d.criteria) { d.criteria.lineCount = 1200; if (d.criteria.provenOn) d.criteria.provenOn.count = 1200 }
  d.change.stepCount = 5646
  d.sessions = Array.from({ length: 1001 }, (_, i) => ({ ...d.sessions[0], id: 'session-' + (i + 1) }))
  for (const st of d.statements) st.step = 1666
  return d
}

test('a not-proven Change: tallies, sessions and the step count are grouped', () => {
  const t = text({ data: big('s1') }, { tech: true, open: OPEN })
  assert.ok(t.includes('5,646 steps'), 'step count')
  assert.ok(t.includes('1,001 sessions'), 'session count')
  assert.ok(t.includes('1,679 passed'), 'passed tally')
  for (const bare of ['5646', '1001 sessions', '1679', '1203']) assert.ok(!t.includes(bare), 'bare count: ' + bare)
})

test('a ready Change: "N of M" of what counts as done is grouped', () => {
  const t = text({ data: big('ready') }, { tech: true, open: OPEN })
  assert.ok(t.includes('1,200 of 1,200'), 'N of M')
  assert.ok(!t.includes('1200 of 1200'), 'bare N of M')
})

test('a step number of 1,000 or more is an identifier and stays bare', () => {
  const t = text({ data: big('s1') }, { tech: true, open: OPEN })
  assert.ok(t.includes('step 1666'), 'step number bare')
  assert.ok(!t.includes('step 1,666'), 'step number grouped')
})
