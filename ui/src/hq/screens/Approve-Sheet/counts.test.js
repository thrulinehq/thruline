/* C29: every count of 1,000 or more on the approve sheet and the Change behind
 * it reads with thousands separators; a step number and a line label stay bare. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { DEMO as BRIEF } from '../Change-Brief/demo.js'
import { selectApproveSheet } from './from-change.js'

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
  const file = join(await mkdtemp(join(tmpdir(), 'approve-sheet-counts-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state)).replace(/<!-- -->/g, '')

const PRO = { status: 'valid', plan: 'pro' }
function bigSheet() {
  const base = selectApproveSheet(BRIEF.states.ready, null, { licence: PRO })
  const lines = Array.from({ length: 1001 }, (_, i) => ({ n: i + 1, text: 'Line ' + (i + 1) + ' holds.', kind: 'behaviour', link: null, state: 'proven', word: 'Proven' }))
  return {
    ...base,
    change: { ...base.change, sessions: 1234 },
    criteria: { ...base.criteria, version: 'v1', acceptedAt: '12:00', acceptedBy: 'sam', setAside: 1002, lines },
    run: { step: 1666, command: 'pytest -q', scope: [], version: base.version.id, exit: 0, passed: 1500, failed: 0, durationS: null, at: null, cwd: null, capturedBy: null },
    trace: { ...base.trace, lanes: { request: 1, work: 2345, execution: 5646, claims: 1679 }, runs: [{ step: 1667, passed: 1501, failed: 0, exit: 0 }] },
    act: { ...base.act, changeId: 'chg_1', ready: true, seen: 'full-rev' },
  }
}

test('counts of 1,000 or more read with thousands separators', () => {
  const out = text({ data: bigSheet() })
  for (const s of ['1,234 sessions', 'Work · 2,345', 'Execution · 5,646', 'Claims & evidence · 1,679', '1,501 passed, 0 failed',
    '1,500 passed, 0 failed', 'Rests on 1,001 of 1,001 things', '1,001 lines of what counts as done', '1,002 set aside', 'All 1,001 rest on']) {
    assert.ok(out.includes(s), s)
  }
  for (const bare of ['1234', '2345', '5646', '1679', '1501', '1500', '1002']) assert.ok(!out.includes(bare), 'bare count ' + bare)
})

test('a step number and a line label of 1,000 or more stay bare', () => {
  const out = text({ data: bigSheet() })
  assert.ok(out.includes('step 1667'))
  assert.ok(out.includes('Line 1001'))
  assert.ok(!out.includes('step 1,66'))
  assert.ok(!out.includes('Line 1,001'))
})
