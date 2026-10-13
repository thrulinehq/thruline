/* N13-GLYPHS-ARIA — every state glyph is aria-hidden; the word is the
 * accessible name (report line 1122; F11's "read every state as its word").
 *
 * Two halves:
 *  1. A static census over every workspace .jsx file (the run lane's
 *     run-sheet.jsx and read-rail.jsx included): a value read from a glyph
 *     field (`x.glyph`, a local `glyph`, a row's `mark`) is rendered only
 *     inside an element that carries aria-hidden="true", and no glyph field is
 *     ever joined into one text string with a word.
 *  2. Rendered cases for the places the census found: the word pill's head,
 *     the timeline mark, the records-table and drawer cells, the result and
 *     local badges. Each is rendered, and its accessible text — the text a
 *     screen reader reads, aria-hidden subtrees left out — carries the word
 *     and none of the glyphs.
 */
import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { parse } from '@babel/parser'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'

const here = dirname(fileURLToPath(import.meta.url))

/** The glyphs report line 1122 and card N13-GLYPHS-ARIA name, and the result marks. */
export const STATE_GLYPHS = ['●', '○', '✕', '◭', '◐', '▪', '⌒', '◌', '◇', '◆', '⋯', '⁓', '□', '⊠', '✓', '×', '↻', '⊘', '▲', '⚠', '▶', '•', '◷', '◈']

const JSX_FILES = readdirSync(here).filter((f) => f.endsWith('.jsx')).sort()

function walk(node, visit, parents = [], stop) {
  if (!node || typeof node.type !== 'string') return
  if (stop && parents.length && stop(node)) return
  visit(node, parents)
  for (const key of Object.keys(node)) {
    if (key === 'loc' || key === 'start' || key === 'end' || key === 'extra') continue
    const v = node[key]
    const next = [node, ...parents]
    if (Array.isArray(v)) for (const c of v) walk(c, visit, next, stop)
    else if (v && typeof v.type === 'string') walk(v, visit, next, stop)
  }
}

/** Whether an expression reads a glyph field: `x.glyph`, `glyph`, `x.mark`.
 *  A nested element is its own case, so the walk stops at one. */
const isElement = (n) => n.type === 'JSXElement' || n.type === 'JSXFragment'
function readsGlyph(expr) {
  let hit = false
  walk(expr, (n) => {
    if (n.type === 'MemberExpression' && !n.computed && n.property && (n.property.name === 'glyph' || n.property.name === 'mark')) hit = true
    if (n.type === 'Identifier' && n.name === 'glyph') hit = true
  }, [], isElement)
  return hit
}

/** Whether an expression's RENDERED value can be a glyph field. The test of a
 *  `c.glyph ? <span …/> : null` is not rendered; its branches are. */
function rendersGlyph(e) {
  if (!e) return false
  if (isElement(e)) return false
  if (e.type === 'ConditionalExpression') return rendersGlyph(e.consequent) || rendersGlyph(e.alternate)
  if (e.type === 'LogicalExpression') return e.operator === '&&' ? rendersGlyph(e.right) : rendersGlyph(e.left) || rendersGlyph(e.right)
  return readsGlyph(e)
}

const hidden = (el) => (el.openingElement.attributes || []).some((a) => a.type === 'JSXAttribute' && a.name && a.name.name === 'aria-hidden'
  && ((a.value && a.value.type === 'StringLiteral' && a.value.value === 'true') || (a.value && a.value.type === 'JSXExpressionContainer' && a.value.expression.type === 'BooleanLiteral' && a.value.expression.value === true)))

function census() {
  const shown = []
  const joined = []
  for (const f of JSX_FILES) {
    const src = readFileSync(join(here, f), 'utf8')
    const ast = parse(src, { sourceType: 'module', plugins: ['jsx'] })
    walk(ast.program, (n, parents) => {
      if (n.type === 'JSXExpressionContainer' && parents[0] && parents[0].type === 'JSXElement' && rendersGlyph(n.expression)) {
        if (!hidden(parents[0])) shown.push(`${f}:${n.loc.start.line} ${src.slice(n.start, n.end)}`)
      }
      if (n.type === 'BinaryExpression' && n.operator === '+' && readsGlyph(n)) {
        /* `a.glyph + ' ' + a.word` — the glyph and the word in one string. */
        joined.push(`${f}:${n.loc.start.line} ${src.slice(n.start, n.end).slice(0, 80)}`)
      }
      if (n.type === 'TemplateLiteral' && n.expressions.some(readsGlyph)) joined.push(`${f}:${n.loc.start.line} ${src.slice(n.start, n.end).slice(0, 80)}`)
    })
  }
  return { shown, joined }
}

test('census: the workspace .jsx files include the run lane\'s sheet and rail', () => {
  for (const f of ['run-sheet.jsx', 'read-rail.jsx', 'ui-kit.jsx', 'canvas.jsx', 'docks.jsx', 'shell.jsx', 'check-catalog.jsx', 'local-verification.jsx', 'checks-panel.jsx']) {
    assert.ok(JSX_FILES.includes(f), f)
  }
})

test('census: every glyph field rendered into the page sits inside an aria-hidden element', () => {
  const { shown } = census()
  assert.deepEqual(shown, [], 'glyphs a screen reader would read aloud by their Unicode names')
})

test('census: no glyph field is joined into one text string with a word', () => {
  const { joined } = census()
  assert.deepEqual(joined, [], 'a glyph concatenated with its word cannot be hidden from a screen reader')
})

/* ── rendered cases ─────────────────────────────────────────────────────── */

const temporary = await mkdtemp(join(tmpdir(), 'thruline-a11y-glyphs-'))
after(() => rm(temporary, { recursive: true, force: true }))
const result = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  stdin: {
    resolveDir: here,
    contents: `
      import React from 'react';
      import { renderToStaticMarkup } from 'react-dom/server';
      import { StateWord } from './ui-kit.jsx';
      import { Timeline, RecordsTable, DestinationsDrawer } from './canvas.jsx';
      import { ResultBadge } from './check-catalog.jsx';
      import { LocalBadge } from './local-verification.jsx';
      import { WORD, wordOf } from './logic.js';
      const h = React.createElement;
      export const html = (el) => renderToStaticMarkup(el);
      export { h, StateWord, Timeline, RecordsTable, DestinationsDrawer, ResultBadge, LocalBadge, WORD, wordOf };
    `,
  },
})
const file = join(temporary, 'glyphs.mjs')
await writeFile(file, result.outputFiles[0].text)
const M = await import(pathToFileURL(file).href)

/** What a screen reader reads: the text, aria-hidden subtrees left out, and
 *  an element's aria-label in place of its content. */
function spoken(el) {
  let out = ''
  for (const c of el.childNodes) {
    if (c.nodeType === 3) out += c.textContent
    else if (c.nodeType === 1 && c.getAttribute('aria-hidden') !== 'true') out += c.hasAttribute('aria-label') ? ' ' + c.getAttribute('aria-label') + ' ' : spoken(c)
  }
  return out
}
const dom = (markup) => new JSDOM(`<!doctype html><body>${markup}</body>`).window.document.body
const noGlyph = (text, what) => {
  for (const g of STATE_GLYPHS) assert.ok(!text.includes(g), `${what}: "${text}" reads the glyph ${g} aloud`)
}

test('rendered: the word pill says the word and not its glyph, for every state word', () => {
  for (const slug of Object.keys(M.WORD)) {
    const b = dom(M.html(M.h(M.StateWord, { slug, clause: 'What would prove it: a test run.', variant: 'inline' })))
    const said = spoken(b)
    const word = b.textContent.trim()
    if (!word) continue
    noGlyph(said, slug)
  }
})

test('rendered: the timeline mark is hidden; the row still reads its words', () => {
  const rows = [
    { id: 'a', seq: 'seq 1', mark: '○', markColor: '#888', kind: 'Message', kindColor: '#888', text: 'Tests pass', font: 'inherit', fontSize: '13px', ref: 'claim placed here', sel: false, opacity: 1, bg: 'transparent', edge: 'transparent' },
    { id: 'b', seq: 'seq 2', mark: '⚠', markColor: '#888', kind: 'Command', kindColor: '#888', text: 'rm -rf build', font: 'inherit', fontSize: '13px', ref: 'destructive command', sel: false, opacity: 1, bg: 'transparent', edge: 'transparent' },
  ]
  const b = dom(M.html(M.h(M.Timeline, { caption: 'Timeline', rows, note: null, rowH: 28 })))
  const items = [...b.querySelectorAll('[role="listitem"]')]
  assert.equal(items.length, 2)
  for (const it of items) noGlyph(spoken(it), 'timeline row')
  assert.ok(spoken(items[0]).includes('claim placed here'))
})

test('rendered: a records-table state cell and a drawer cell read the word, not the glyph', () => {
  const w = M.wordOf(Object.keys(M.WORD)[0])
  assert.ok(w.word && w.glyph, 'a state word with a glyph')
  const cell = { text: w.word, glyph: w.glyph, color: '#888', font: 'inherit', fontSize: '13px' }
  const t = dom(M.html(M.h(M.RecordsTable, { cols: '1fr', head: [{ key: 's', label: 'State' }], groups: [{ key: 'g', title: 'g', rows: [{ id: 'r', cells: [cell], sel: false, bg: 'transparent', edge: 'transparent' }] }], foot: '', rowH: 28 })))
  const gc = t.querySelector('[role="gridcell"]')
  assert.ok(gc, 'the table drew its cell')
  noGlyph(spoken(gc), 'records table cell')
  assert.ok(spoken(gc).includes(cell.text), 'the word is read')
  assert.ok(gc.textContent.includes(cell.glyph), 'the glyph is still drawn for the eye')
  const d = dom(M.html(M.h(M.DestinationsDrawer, { destination: 'records', title: 'Records', lead: '', width: 480, sections: [{ title: 's', cols: '1fr', rows: [{ cells: [{ ...cell, deco: 'none' }] }] }], onClose() {} })))
  const drawerText = spoken(d)
  assert.ok(drawerText.includes(cell.text), 'the drawer reads the word')
  noGlyph(drawerText, 'drawer cell')
  assert.ok(d.textContent.includes(cell.glyph), 'the glyph is still drawn for the eye')
})

test('rendered: a result badge and a local badge read their word alone', () => {
  for (const status of ['pass', 'fail', 'stale', 'blocked', 'running', 'waived']) {
    const b = dom(M.html(M.h(M.ResultBadge, { status })))
    noGlyph(spoken(b), 'ResultBadge ' + status)
    assert.ok(spoken(b).trim().length > 0, 'ResultBadge ' + status + ' still says a word')
  }
  for (const state of ['unknown', 'pass', 'fail']) {
    const b = dom(M.html(M.h(M.LocalBadge, { state })))
    noGlyph(spoken(b), 'LocalBadge ' + state)
    assert.ok(spoken(b).trim().length > 0)
  }
})
