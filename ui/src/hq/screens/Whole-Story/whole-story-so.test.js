/* C30 P0-1: the whole story's "What counts as done" draws each line's own
 * verdict at ⑥ So, read from the tracked Change's model (deriveChangeModel's
 * per-line `state` and `proven.version`), or no verdict at all. It used to draw
 * "● Proven · version <page version>" on every accepted line with nothing in
 * the record behind it. Synthetic fixtures only. */
import { wholeStoryFrom } from './from-change.js'
import { DEMO as BRIEF } from '../Change-Brief/demo.js'
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
export function render(props) {
  const inst = new Logic(props || {})
  inst.props = props || {}
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'c30-ws-so-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')

/** The "What counts as done" section only, as markup and as text. */
function doneSection(props) {
  const html = unescape(M.render(props))
  const from = html.indexOf('id="ws-done"')
  const to = html.indexOf('id="ws-said"')
  assert.ok(from > 0 && to > from, 'the page draws the What counts as done section')
  const markup = html.slice(from, to)
  return { markup, text: markup.replace(/<[^>]+>/g, '') }
}

/** One criterion row's text, split on the row markers. */
const rows = (markup) => markup.split('class="tl-row"').slice(1).map((r) => r.replace(/<[^>]+>/g, ''))

const LIT = '0 0 6px rgba(255,154,82,.6)'
/* Proven is drawn from the one token (data/palette.js), its standard green the fallback: C30 R-PREFS */
const GREEN_BORDER = 'rgba(var(--tl-proven-rgb, 61, 214, 140), .6)'

/** A tracked Change whose model is on `short`, with the given criteria. */
const tracked = (short, criteria) => ({
  change: { id: 'chg_syn', accepted_scope: { version: 1, acceptance: { accepted_at: '2026-10-07T10:00:00Z' } } },
  hq: { version: { short }, counts: { proven: criteria.filter((c) => c.state === 'proven').length }, criteria },
  proposals: null,
})
/** The snapshot for that tracked Change: Brief's s1 state, with the criteria accepted. */
const snap = (t) => ({ ...BRIEF.states.s1, criteria: { ...BRIEF.states.s1.criteria, accepted: true, version: 'v1', acceptedAt: '10:00', lineCount: t.hq.criteria.length, provenOn: { version: t.hq.version.short, count: t.hq.counts.proven, runId: null } } })

test('a Change whose lines are not proven draws no "Proven" on any of them, and each line its own state', () => {
  const t = tracked('abc1234', [
    { n: 1, text: 'Line one is checked', state: 'not_proven', word: 'Not proven', proven: null },
    { n: 2, text: 'Line two is checked', state: 'needs_recheck', word: 'Needs a re-check', proven: null },
    { n: 3, text: 'Line three is checked', state: 'in_progress', word: 'In progress', proven: null },
    { n: 4, text: 'Line four is checked', state: 'recorded', word: 'Recorded', proven: null },
  ])
  const { markup, text } = doneSection({ data: wholeStoryFrom(snap(t), t) })
  assert.ok(!/Proven/.test(text), 'no line says Proven: ' + text)
  assert.ok(!markup.includes(GREEN_BORDER), 'no green verdict pill is drawn')
  assert.ok(!markup.includes(LIT), 'no chain is drawn lit when nothing holds')
  const r = rows(markup)
  assert.equal(r.length, 4)
  assert.ok(r[0].includes('○') && r[0].includes('Not proven'), r[0])
  assert.ok(r[1].includes('◭') && r[1].includes('Needs a re-check'), r[1])
  assert.ok(r[2].includes('◐') && r[2].includes('In progress'), r[2])
  assert.ok(r[3].includes('▪') && r[3].includes('Recorded'), r[3])
})

test('a line proven on the exact version says so, with that version and its scope', () => {
  const t = tracked('abc1234', [
    { n: 1, text: 'Totals are right', state: 'proven', word: 'Proven', proven: { version: 'abc1234', scope: ['checkout/'] } },
    { n: 2, text: 'Codes apply once', state: 'not_proven', word: 'Not proven', proven: null },
  ])
  const { markup } = doneSection({ data: wholeStoryFrom(snap(t), t) })
  const r = rows(markup)
  assert.ok(r[0].includes('●') && r[0].includes('Proven · version abc1234'), r[0])
  assert.ok(r[0].includes('checkout/'), 'the scope it was proven over is drawn: ' + r[0])
  assert.ok(!/Proven · version/.test(r[1]) && r[1].includes('Not proven'), r[1])
  assert.ok(!r[1].includes('Proven ·'), r[1])
})

test('with no version, or no state, in the record, the line draws no verdict at all', () => {
  const t = tracked('abc1234', [
    { n: 1, text: 'Proven, version not recorded', state: 'proven', word: 'Proven', proven: null },
    { n: 2, text: 'No state recorded', word: 'Proven' },
  ])
  const { markup } = doneSection({ data: wholeStoryFrom(snap(t), t) })
  const r = rows(markup)
  assert.equal(r.length, 2)
  for (const row of r) assert.ok(!/Proven/.test(row.replace('Proven, version not recorded', '')), row)
  assert.ok(!markup.includes(GREEN_BORDER))
  assert.ok(!markup.includes(LIT))
})

test('the design gallery still draws its four proven lines exactly as the canvas does', () => {
  const { markup } = doneSection({})
  const r = rows(markup)
  assert.equal(r.length, 4)
  for (const row of r) assert.ok(row.includes('●Proven · version e71c5b0'), row)
  assert.equal(markup.split(LIT).length - 1, 4 * 5, 'five lit links on each of the four lines')
})
