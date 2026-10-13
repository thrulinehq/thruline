/* The Inbox's approve row, drawn: "Ready for approval", the version, the Approve button at the
 * Approve sheet. A row from the record states only what the record holds: where it has no lines
 * or no chain, none is drawn (never "0 of 0", never "holds end to end" it was not asked about),
 * and "since" is "last active" because the record has only that. The design gallery (no data) is
 * still the canvas, word for word. Synthetic fixtures only. */
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
import * as P from '../../data/project.js'
import { LIST, VIEW, NOW } from '../../data/project-fixture.js'
import { inboxText } from './text.js'
export { NOW, VIEW, LIST }
export function data(ready, view = VIEW) { return P.inboxData({ projects: LIST, view, ready, now: NOW }) }
export function render(props, state) {
  const inst = new Logic(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}
export const text = (d) => inboxText(d, {})`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-inbox-ready-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const html = (props, state) => unescape(M.render(props, state))
/** What a person reads: the markup with its tags taken out. */
const words = (out) => out.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ')
const withLines = { change: 'chg_a', version: '9f2ad3e', criteria: { version: 'v1', lines: [{ n: 1, state: 'proven', gate: false }, { n: 2, state: 'proven', gate: false }] } }
const bare = { change: 'chg_a', version: '9f2ad3e', criteria: null }

test('the approve row is "Waiting on you", first, with the real version and the Approve sheet’s address', () => {
  const D = M.data({ done1: withLines })
  const t = M.text(D)
  assert.deepEqual(t.groups.you.map((r) => [r.id, r.kind]), [['done1', 'approve'], ['ask-ask1', 'prompt']])
  const r = t.groups.you[0].t
  assert.equal(r.version, '9f2ad3e')
  assert.equal(r.approveHref, '#/changes/chg_a/approve?project=p1')
  assert.equal(r.openHref, '#/changes/chg_a?project=p1')
  assert.equal(r.approveAria, 'Approve 9f2ad3e. Opens a sheet; nothing is recorded until you confirm.')
  assert.match(r.since, /^last active /, 'the record holds when the work was last active, not when it became ready')
  assert.match(r.meta, /^shop · last active /)
  assert.match(r.pvSub, /^Change · shop · session ended /)
})

test('on the screen: the Approve button goes to the sheet, and ⌘↵ would too', () => {
  const D = M.data({ done1: withLines })
  const out = html({ data: D })
  assert.match(out, /Ready for approval/)
  assert.match(out, /href="#\/changes\/chg_a\/approve\?project=p1"/)
  assert.match(out, /Approve <span[^>]*>9f2ad3e<\/span>/)
  assert.match(out, /Rests on 2 of 2 things that count as done, each with a run on <span[^>]*>9f2ad3e<\/span>\./)
  assert.match(out, /What counts as done · v1 · 2 lines/)
  assert.doesNotMatch(out, /undefined|NaN|\bnull\b/)
})

test('a row from the record draws no chain it was not given: no "holds end to end", no six links', () => {
  const out = html({ data: M.data({ done1: withLines }) })
  assert.doesNotMatch(out, /holds end to end/)
  assert.doesNotMatch(out, /Each line’s chain/)
  assert.doesNotMatch(out, /All six links hold/)
})

test('the picked approve row’s preview draws the lines it has and no chain', () => {
  const live = words(html({ data: M.data({ done1: withLines }) }, { sel: 'done1' }))
  assert.match(live, /Ready for approval for version 9f2ad3e\. It rests on:/)
  assert.match(live, /2 of 2 things that count as done, each with a run on\s+9f2ad3e\s*\./)
  assert.match(live, /2 of 2 proven on\s+9f2ad3e\s+· no line stops short/)
  assert.doesNotMatch(live, /Each line’s chain/)
  const bareLive = words(html({ data: M.data({ done1: bare }) }, { sel: 'done1' }))
  assert.match(bareLive, /Ready for approval for version 9f2ad3e\./)
  assert.doesNotMatch(bareLive, /It rests on|proven on|Each line’s chain|0 of 0/)
  assert.match(bareLive, /Approve 9f2ad3e/)
})

test('with no lines served the row draws none: never "0 of 0"', () => {
  const D = M.data({ done1: bare })
  const out = html({ data: D })
  assert.match(out, /Ready for approval/)
  assert.doesNotMatch(out, /Rests on|0 of 0|things that count as done|What counts as done|It rests on/)
  const r = M.text(D).groups.you[0].t
  assert.equal(r.pvLead, 'Ready for approval for version 9f2ad3e.')
  assert.equal(r.rests, '')
})

test('a ready Change replaces the thread’s own row: it is not also a not-proven row', () => {
  const D = M.data({ np1: withLines, re1: { ...withLines, change: 'chg_b' } })
  const t = M.text(D)
  assert.deepEqual(t.groups.you.filter((r) => r.kind === 'approve').map((r) => r.id).sort(), ['np1', 're1'])
  assert.deepEqual(t.groups.check, [], 'both not-proven rows became approve rows')
  assert.equal(Object.values(D.changes).filter((c) => c.state === 'notProven' || c.state === 'recheck').length, 0)
})

test('the design gallery is still the canvas: since, the chain, the lines', () => {
  const out = words(html({}))
  assert.match(out, /holds end to end/)
  assert.match(out, /Rests on 3 of 3 things that count as done/)
  assert.match(out, /since 15:48/)
  // its preview, with the lines and the chain, when the canvas’s ready row is the one picked
  const preview = words(html({}, { sel: 'post' }))
  assert.match(preview, /Each line’s chain/)
  assert.match(preview, /It rests on:/)
  assert.match(preview, /What counts as done/)
})

test('no word the product does not say, and no time-zone name, on the approve row', () => {
  const out = words(html({ data: M.data({ done1: withLines }) }))
  assert.doesNotMatch(out, /\b(lie|lied|cheat|violate|ignore|ignored|hallucinat\w*|trial)\b/i)
  assert.doesNotMatch(out, /\b(UTC|GMT|IST|PST|PDT|EST|EDT)\b|[0-9]+ ?%/)
  assert.doesNotMatch(out, /\/Users\//)
})
