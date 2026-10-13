/* C29: every count of 1,000 or more in the pull-request note — the preview and
 * the Markdown a person pastes — reads with thousands separators; a step number
 * stays bare. Synthetic fixtures only (pr-fixture.js). */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { prFaceFrom, noteFacts } from './from-change.js'
import { composeNote, copyOf } from './markdown.js'
import { NOW, tracked, snapshot, prDoc } from './pr-fixture.js'

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
  const file = join(await mkdtemp(join(tmpdir(), 'pr-face-counts-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state)).replace(/<!-- -->/g, '')

function bigNote() {
  const t = tracked()
  const s = snapshot(t)
  const ctx = { now: NOW, tracked: t, pr: { doc: prDoc() } }
  const f = noteFacts(s, ctx)
  const t0 = f.tests[0] || { id: 'r', command: 'pytest -q', version: f.version, outcome: 'succeeded', exitCode: 0, at: null }
  const open = f.lines.find((l) => l.state !== 'proven')
  const facts = {
    ...f,
    sessions: { count: 1234, agents: f.sessions.agents },
    stepCount: 1679,
    editCount: 2345,
    commandCount: 5646,
    noResultCount: 1001,
    statementCount: 3000,
    lines: Array.from({ length: 1200 }, (_, i) => ({ ...open, n: i + 1, text: 'Line ' + (i + 1) })),
    tests: [{ ...t0, step: 1666, status: 'finished', passed: 1500, failed: 0 }],
    masking: { masked: 4321, families: [], complete: true },
  }
  const note = composeNote(facts)
  const d = prFaceFrom(s, ctx)
  return { ...d, note, copy: copyOf(note, d.setup), facts }
}

test('the Markdown a person pastes writes every count of 1,000 or more with separators', () => {
  const md = bigNote().copy
  for (const s of ['1,234 recorded sessions', '1,679 steps', '2,345 edits', '5,646 commands', '1,001 commands have no result',
    '3,000 statements', '1,500 passed, 0 failed', 'masked: 4,321', '1,200 things still need evidence', 'waits on all 1,200', '1,188 more lines of what counts as done']) {
    assert.ok(md.includes(s), s)
  }
  for (const bare of ['1234', '1679', '2345', '5646', '1001', '3000', '1500', '4321', '1200', '1188']) assert.ok(!md.includes(bare), 'bare count ' + bare)
  assert.ok(md.includes('step 1666'), 'a step number stays bare')
  assert.ok(!md.includes('step 1,666'))
})

test('the preview draws the same separated counts, and the step bare', () => {
  const out = text({ data: bigNote() })
  for (const s of ['3,000 statements', '1,500 passed, 0 failed', 'step 1666', '1,200 things still need evidence']) assert.ok(out.includes(s), s)
  for (const bare of ['3000', '1500', '1200']) assert.ok(!out.includes(bare), 'bare count ' + bare)
})
