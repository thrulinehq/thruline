/* W1-E, HQ Next wave 1: the Whole Story's content arrives through one object.
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

test('the gallery draws the demo and its design notes', () => {
  const html = text({})
  assert.ok(html.includes('Fix double-applied discount on checkout total'))
  assert.ok(html.includes('Design notes · not in the file'))
  const htmlFmt = text({}, { fmt: 'html' })
  assert.ok(htmlFmt.includes('NEEDS ENGINE'))
})

test('the live path draws data, and no design annotation', () => {
  const d = clone(M.DEMO)
  d.change.title = 'A stand-in Change'
  d.change.id = 'a-stand-in-change'
  for (const fmt of ['md', 'html', 'json']) {
    const html = text({ data: d }, { fmt })
    assert.ok(html.includes('A stand-in Change'))
    assert.ok(!html.includes('Fix double-applied discount on checkout total'), fmt)
    assert.ok(!html.includes('Design notes'), fmt)
    assert.ok(!html.includes('NEEDS ENGINE') && !html.includes('ILLUSTRATIVE'), fmt)
  }
  assert.ok(text({ data: d }).includes('thruline-packet-a-stand-in-change-e71c5b0.md'))
})

test('counts are counted from the data, not written down', () => {
  const d = clone(M.DEMO)
  d.recordedSteps = 7
  d.redaction.paths = 2
  d.findings.groups[2].settlement.kind = 'settled'
  d.findings.groups[2].settlement.at = '16:11'
  d.readable.records = d.readable.records.slice(0, 3)
  const html = text({ data: d })
  assert.ok(html.includes('1 session · Claude Code · 7 recorded steps'))
  assert.ok(html.includes('5 found at 16:04 · 4 settled · 1 nothing to settle · 0 open'))
  assert.ok(html.includes('Every one of the 3 records this rests on could be read.'))
  assert.ok(text({ data: d }, { paths: true }).includes('File and folder paths: 2 left out'))
})

test('the version and agent are named by the data', () => {
  const d = clone(M.DEMO)
  d.version.id = 'abc1234'
  d.statements[0].agent = 'Codex'
  d.statements[1].agent = 'Codex'
  const html = text({ data: d })
  assert.ok(html.includes('describes abc1234 as of'))
  assert.ok(html.includes('Codex said '))
})

test('no demo content is left as a literal in the markup', () => {
  const lits = ungatedLiterals().join('\n')
  for (const s of ['checkout-web', 'e71c5b0', 'Fix double-applied', 'Tests pass.', 'step 62', '16:06', '9f2ad3e', 'afb960f9', '1.4 GB', '~/code', 'Claude Code said']) {
    assert.ok(!lits.includes(s), 'literal left in template: ' + s)
  }
})
