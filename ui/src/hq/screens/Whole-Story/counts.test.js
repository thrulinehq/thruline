/* C29: every count of 1,000 or more on the whole story reads with thousands
 * separators ("1,679 recorded steps"); a step number stays bare ("step 1666"). */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { wholeStoryFrom } from './from-change.js'
import { DEMO as BRIEF } from '../Change-Brief/demo.js'

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
  const file = join(await mkdtemp(join(tmpdir(), 'whole-story-counts-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state)).replace(/<!-- -->/g, '')

function bigStory() {
  const d = wholeStoryFrom(BRIEF.states.s1, null)
  const run0 = d.runs[0] || { time: '12:00', command: 'pytest -q', version: d.version.id, now: true }
  return {
    ...d,
    sessions: 1234,
    steps: 1679,
    statementsCount: 2345,
    testsTotal: 5646,
    runs: [{ ...run0, step: 1666, passed: 1500, failed: 0, outcome: 'succeeded' }],
    criteria: { ...d.criteria, accepted: true, lineCount: 1200, proven: 1100 },
    masking: { masked: 3456 },
  }
}

test('live counts of 1,000 or more read with thousands separators', () => {
  const out = text({ data: bigStory(), integrity: { status: 'ready', doc: {
    schema_version: 1, state: 'INTACT', links_checked: 1679, sessions_checked: 1234,
    checked_at: '2026-10-07T10:30:00Z',
  } } })
  for (const s of ['1,234 sessions', '1,679 recorded steps', '1,679 recorded links in 1,234 confirmed sessions', '2,345 statements in all',
    '5,646 test runs', 'of 5,646 test runs.', '1,500 passed, 0 failed', '1,100 of 1,200', '3,456 masked']) {
    assert.ok(out.includes(s), s)
  }
  for (const bare of ['1234', '1679', '2345', '5646', '1500', '1100', '1200', '3456']) assert.ok(!out.includes(bare), 'bare count ' + bare)
})

test('a step number of 1,000 or more stays bare: it is a position, not a count', () => {
  const out = text({ data: bigStory() })
  assert.ok(out.includes('step 1666'))
  assert.ok(!out.includes('step 1,666'))
})
