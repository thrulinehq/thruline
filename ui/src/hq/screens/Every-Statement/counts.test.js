/* C29: every count of 1,000 or more on Every-Statement reads with thousands
 * separators ("1,679"), while identifiers — a step number — stay bare. Live
 * path only: the design gallery draws DEMO, whose numbers are all under 1,000. */
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
import { everyStatementFromChange } from './from-change.js'
import { changeSnapshot } from '../../data/change.js'
import { threadFixture } from '../What-Changed/thread-fixture.js'

export { everyStatementFromChange as select, changeSnapshot, threadFixture }
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
  const file = join(await mkdtemp(join(tmpdir(), 'every-statement-counts-')), 'screen.mjs')
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

/* The served ThreadView, selected, with its counts raised past 1,000: every
 * run's tally, the sessions, and the messages no statement was read from
 * ("Also said"); and every statement's step raised past 1,000, which must stay bare. */
const big = () => {
  const d = M.select(M.changeSnapshot(M.threadFixture(), null, NOW))
  for (const r of d.runs) if (r.passed != null) { r.passed = 1679; r.failed = 1203 }
  d.sessions = Array.from({ length: 1001 }, (_, i) => ({ ...d.sessions[0], id: i ? 's-extra-' + i : d.sessions[0].id }))
  for (let i = 0; i < 1234; i++) d.messages.push({ id: 'also-' + i, step: 7, time: null, sessionId: d.sessions[0].id, agent: 'Claude Code', text: 'Looking at it.' })
  for (const q of d.statements) q.step = 1666
  return d
}

test('sessions, the "also said" tally and the status bar counts are grouped', () => {
  const t = text({ data: big() })
  assert.ok(t.includes('1,001 sessions'), 'session count')
  assert.ok(t.includes('Also said — not checked · 1,234'), 'also-said count')
  assert.ok(t.includes('Also said (1,234)'), 'rack summary')
  for (const bare of ['1001', '1234']) assert.ok(!t.includes(bare), 'bare count: ' + bare)
})

test('a run tally is grouped in the inspector', () => {
  const d = big()
  const st = d.statements.find((q) => q.claimKind === 'tests_pass')
  const t = text({ data: d }, { sel: st.id })
  assert.ok(t.includes('1,679 passed, 1,203 failed'), 'tally')
  assert.ok(!t.includes('1679') && !t.includes('1203'), 'bare tally')
})

test('a step number of 1,000 or more is an identifier and stays bare', () => {
  const t = text({ data: big() })
  assert.ok(t.includes('1666'), 'step number bare')
  assert.ok(!t.includes('1,666'), 'step number grouped')
})
