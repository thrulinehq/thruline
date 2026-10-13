/* C29: every count of 1,000 or more on Done-Criteria is written with
 * thousands separators (data/count.js `fmtCount`); a step number stays bare.
 *
 * The live data is selected from the one Change snapshot as
 * done-criteria-live.test.js does (the canvas story plus a tracked Change's
 * proposals), with its counts raised past 1,000. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { DEMO as BRIEF } from '../Change-Brief/demo.js'
import { selectDoneCriteria } from './from-change.js'

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
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
  const file = join(await mkdtemp(join(tmpdir(), 'done-criteria-counts-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state)).replace(/<[^>]*>/g, ' ')

const proposed = () => ({
  id: 'chg_1',
  change: { accepted_scope: null, code_moved: null },
  proposals: { criteria: [{ number: 1, text: 'Each import row posts once.' }, { number: 2, text: 'Re-running an import posts nothing new.' }], notes: [], scope: { paths: ['src/'] } },
  coverage: { criteria: [], candidates: [] },
  model: null,
})
const live = () => {
  const d = selectDoneCriteria(BRIEF.states.s1, proposed())
  /* raised past 1,000: the Change's step count and the last run's tally; its step is a position */
  return { ...d, steps: 1679, lastRun: { ...d.lastRun, step: 1666, passed: 1234, failed: 0 } }
}

test('the Change’s step count reads with thousands separators', () => {
  const out = text({ data: live() })
  assert.ok(out.includes('1,679 steps'), out.match(/.{0,30}steps.{0,30}/))
  assert.ok(!/\b1679 steps/.test(out))
})

test('the last run’s tally reads with separators on the technical face', () => {
  const out = text({ data: live() }, { face: 'technical' })
  assert.ok(out.includes('1,234 passed, 0 failed'), out.match(/.{0,40}passed.{0,40}/))
  assert.ok(!out.includes('1234'))
})

test('a step number of 1,000 or more is a position and stays bare', () => {
  const out = text({ data: live() }, { face: 'technical' })
  assert.ok(out.includes('step 1666'), out.match(/.{0,30}step 16.{0,30}/))
  assert.ok(!out.includes('1,666'))
})
