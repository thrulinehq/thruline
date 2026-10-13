import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const temporary = await mkdtemp(join(tmpdir(), 'thruline-analysis-'))
after(() => rm(temporary, { recursive: true, force: true }))
const result = await build({
  stdin: { contents: `
    import React from 'react';
    import { renderToStaticMarkup } from 'react-dom/server';
    import { EvidenceAnalysis } from './review.jsx';
    export { concernIncludesNode, investigationFor } from './review.jsx';
    export { Workspace } from './shell.jsx';
    export const render = props => renderToStaticMarkup(React.createElement(EvidenceAnalysis, props));
  `, resolveDir: dirname(fileURLToPath(import.meta.url)) },
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
})
const compiled = join(temporary, 'analysis.mjs')
await writeFile(compiled, result.outputFiles[0].text)
const { concernIncludesNode, investigationFor, Workspace, render } = await import(pathToFileURL(compiled).href)

test('any explicitly linked failure selects the group; nearby and absent nodes do not', () => {
  const concern = { node: 'ev10', eventRefs: [{ node: 'ev10' }, { node: 'ev99' }] }
  assert.equal(concernIncludesNode(concern, 'ev99'), true)
  assert.equal(concernIncludesNode(concern, 'ev11'), false)
  assert.equal(concernIncludesNode({ node: null }, null), false)
})

test('failure guidance requires a matching recovery rather than any later success', () => {
  const guidance = investigationFor({ kind: 'failure' })
  assert.match(guidance.meaning, /same work/)
  assert.match(guidance.closes, /same command and scope/)
  assert.doesNotMatch(guidance.meaning, /agent lied|proven drift/i)
})

test('derived completion analysis cannot present a conditional offer as a completion assertion', () => {
  const guidance = investigationFor({ kind: 'claim', derivedCompletion: true })
  assert.match(guidance.meaning, /not an explicit completion claim/)
  const html = render({ session: { events: [], claims: [{
    id: 'c', node: 'claim-c', kind: 'task_complete', extractor: 'rules',
    verifier: 'unclaimed_completion@1', verdict: 'unverifiable',
    text: '<script>sendSecrets()</script> If you want, I can check later.',
  }] }, selected: 'claim-c' })
  assert.match(html, /context for the derived review/)
  assert.match(html, /Suggested next step/)
  assert.match(html, /&lt;script&gt;sendSecrets/)
  assert.doesNotMatch(html, /<script>/)
})

test('empty and loading readings do not imply a clean session or a model verdict', () => {
  const html = render({ session: { events: [], claims: [] } })
  assert.match(html, /No review points were raised/)
  assert.match(html, /does not mean no tests ran/)
  assert.doesNotMatch(html, /all clear|everything checks out|AI verified|Ask your agent/i)
  const loading = render({ session: {}, reading: '/api/explain' })
  assert.match(loading, /Reading the evidence/)
  assert.doesNotMatch(loading, /0 review points/)
})

test('an unrelated selected object does not receive a different objects suggestion', () => {
  const html = render({ session: { events: [], nodes: [{ id: 'unrelated', title: 'Another record' }],
    promises: [{ id: 'p', node: 'promise-p', status: 'open', text: 'Check later' }] }, selected: 'unrelated' })
  assert.match(html, /No review point is linked to this object/)
  assert.doesNotMatch(html, /Suggested next step/)
})

test('a later grouped failure displays its own result and sequence', () => {
  const html = render({ session: { events: [
    { seq: 10, kind: 'exec', text: 'first-check → exit 1' },
    { seq: 99, kind: 'exec', text: 'second-check → exit 2' },
  ] }, selected: 'ev99' })
  assert.match(html, /Recorded result · event 99/)
  assert.match(html, /<blockquote>[\s\S]*second-check → exit 2<\/blockquote>/)
  assert.doesNotMatch(html, /<blockquote>[\s\S]*first-check → exit 1<\/blockquote>/)
})

test('initial graph view retains the lane overview even when the evidence path contains one claim', () => {
  const w = Object.create(Workspace.prototype)
  w.state = { cw: 1000, ch: 600 }
  w.visibleNodes = () => [{ id: 'a' }, { id: 'claim' }]
  w.bbox = () => ({ x0: 20, y0: 40, x1: 1500, y1: 90000 })
  w.S = () => ({ evidencePath: ['claim'] })
  w.setState = value => { w.state = { ...w.state, ...value } }
  w.initialView()
  assert.ok(w.state.view.z >= .55 && w.state.view.z <= .85)
  assert.equal(w.state.view.y + 40 * w.state.view.z, 32)
  assert.equal(w.state.view.x + 20 * w.state.view.z, 16)
})

test('a short window yields the bottom dock and restores the reader’s preference', () => {
  const oldWindow = globalThis.window
  try {
    globalThis.window = { innerWidth: 1440, innerHeight: 540 }
    const w = Object.create(Workspace.prototype)
    w.state = { panels: { left: true, right: true, bottom: true }, narrow: false, vw: 1440, vh: 900 }
    w.canvasRef = { current: null }
    w.setState = (value, then) => { w.state = { ...w.state, ...value }; then?.() }
    w.measure()
    assert.equal(w.state.panels.bottom, false)
    assert.equal(w.state.bottomPref, true)
    globalThis.window.innerHeight = 900
    w.measure()
    assert.equal(w.state.panels.bottom, true)
  } finally { globalThis.window = oldWindow }
})
