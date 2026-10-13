/* W1-E, HQ Next wave 1: Team-View's content arrives through one object.
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

test('the gallery draws the demo and the roadmap annotation', () => {
  const html = text({})
  assert.ok(html.includes('checkout-web'))
  assert.ok(html.includes('Round tax per line, not per order'))
  assert.ok(html.includes('Roadmap.'))
  assert.ok(html.includes('People and records here are illustrative.'))
})

test('a demo-shaped object with no store facts draws no design annotation', () => {
  const d = clone(M.DEMO)
  d.project.name = 'stand-in-project'
  const html = text({ data: d })
  assert.ok(html.includes('stand-in-project'))
  assert.ok(!html.includes('Roadmap.'))
  assert.ok(!html.includes('illustrative'))
})

test('counts are counted from the data, not written down', () => {
  const d = clone(M.DEMO)
  d.setAside = d.setAside.slice(0, 2)
  d.plan.seats = 7
  d.storage.bytes = 2.3e9
  d.activity.monthDeciders = 1
  const html = text({ data: d })
  assert.ok(html.includes('2 lines set aside'))
  assert.ok(html.includes('3 of 7 seats in use'))
  assert.ok(html.includes('Team · 7 seats'))
  assert.ok(html.includes('keeping 2.3 GB of records'))
  assert.ok(html.includes('This month, 1 person approved or set lines aside here.'))
})

test('the base branch is named by the data', () => {
  const d = clone(M.DEMO)
  d.project.baseBranch = 'trunk'
  const html = text({ data: d })
  assert.ok(html.includes('rules and decisions read from trunk'))
  assert.ok(html.includes('Decision files travel through trunk.'))
  assert.ok(!/read from main|through main/.test(html))
})

test('no demo content is left as a literal in the markup', () => {
  const lits = ungatedLiterals().join('\n')
  for (const s of ['checkout-web', 'Acme Ltd', '2c8e4d1', 'e71c5b0', 'Round tax', 'Fix double-applied', '1.4 GB', '16:12', 'Bug fix', 'Priya', 'Recording · 2']) {
    assert.ok(!lits.includes(s), 'literal left in template: ' + s)
  }
})
