/* W1-E, HQ Next wave 1: Record-Over-Time's content arrives through one object.
 * The design gallery (no `data`) draws the canvas's demo; the live path
 * (`data` present) draws only what `data` says and none of the canvas's
 * design annotations. Pixel likeness is pixdiff's job, not this file's. */
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

/* The screen bundled for node: its logic over its template, rendered to static
 * markup with `data` (the live path) or without it (the design gallery). */
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
  const file = join(await mkdtemp(join(tmpdir(), 'w1e-data-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state))
const clone = (x) => JSON.parse(JSON.stringify(x))

/** Every string literal in template.jsx that is not under a `v?.isDemo` gate. */
function ungatedLiterals() {
  const src = readFileSync(join(here, 'template.jsx'), 'utf8')
  const ast = parse(src, { sourceType: 'module', plugins: ['jsx'] })
  const out = []
  const visit = (node, gated) => {
    if (!node || typeof node.type !== 'string') return
    const test = node.test || node.left
    if ((node.type === 'ConditionalExpression' || node.type === 'LogicalExpression') && test && /^v\?\.isDemo\b/.test(src.slice(test.start, test.end))) gated = true
    if (!gated && (node.type === 'StringLiteral' || node.type === 'JSXText')) out.push(node.value)
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

test('the gallery draws the demo and the demo-records annotation', () => {
  const html = text({})
  assert.ok(html.includes('Demo records.'))
  assert.ok(html.includes('41 of 64 statements are proven'))
  assert.ok(html.includes('checkout-web'))
})

test('the live path draws data, and no design annotation', () => {
  const d = clone(M.DEMO)
  d.projects[0].name = 'stand-in-project'
  const html = text({ data: d })
  assert.ok(html.includes('stand-in-project'))
  assert.ok(!html.includes('checkout-web'))
  assert.ok(!html.includes('Demo records.'))
  assert.ok(!html.includes('Illustrative'))
})

test('the headline counts are sums of the tallies in the data', () => {
  const d = clone(M.DEMO)
  d.tallies[30].checkout.cx.statements += 10
  d.tallies[30].checkout.cx.noProofFound += 10
  const html = text({ data: d })
  assert.ok(html.includes('41 of 74 statements are proven'), 'proven of statements')
  assert.ok(html.includes('16 have no proof in the record'))
})

test('a day an agent was not recorded is never drawn as a zero', () => {
  const d = clone(M.DEMO)
  const html = text({ data: d })
  assert.ok(html.includes('Today has no Codex record.'))
  d.daily.checkout.cx.statements[89] = 0
  d.daily.checkout.cx.proven[89] = 0
  assert.ok(!text({ data: d }).includes('Today has no Codex record.'))
})

test('the empty states are made from the project facts', () => {
  const d = clone(M.DEMO)
  d.projects[1].name = 'sample'
  const html = text({ data: d }, { range: '7', project: 'demo' })
  assert.ok(html.includes('No statements in sample in the last 7 days'))
  assert.ok(html.includes('Its one statement was recorded on 5 Sep, 21 days ago.'))
  const none = text({ data: d }, { project: 'notes' })
  assert.ok(none.includes('Nothing recorded yet in notes-cli'))
  assert.ok(none.includes('an empty record is not a zero'))
})

test('no demo content is left as a literal in the markup', () => {
  const lits = ungatedLiterals().join('\n')
  for (const s of ['checkout-web', 'notes-cli', 'recorded 5 Sep', 'hook v0.4', '1.4 GB', '16:10', 'Recording · 2']) {
    assert.ok(!lits.includes(s), 'literal left in template: ' + s)
  }
})
