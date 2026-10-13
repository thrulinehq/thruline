/* C30 R-LEDGER: Repair-Loop's "Copy message" and "Mark as pasted · undo" are the
 * record's, and the ladder's first rung — "Told the agent" — reads the last act.
 *
 * Proven on the screen's own logic with a stubbed write; nothing here reaches a
 * server. Copy message keeps the words the person left in the draft; the mark
 * and its undo name the row the screen saw; and nothing is ever said to have
 * been sent. The gallery keeps the canvas's local toggles. Synthetic fixtures only. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { DEMO as BRIEF } from '../Change-Brief/demo.js'
import { selectRepairLoop } from './from-change.js'
import { toldOf } from '../../data/told.js'

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
export function make(props, state) { const inst = new Logic(props || {}); inst.props = props || {}; if (state) inst.state = { ...inst.state, ...state }; return inst }
export function vals(props, state) { return make(props, state).renderVals() }
export function render(props, state) { const inst = make(props, state); return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h })) }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'repair-loop-told-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')

const NOW = new Date(2026, 9, 8, 17, 0)
const at = (h, m) => new Date(2026, 9, 8, h, m).toISOString()
const served = (act, over = {}) => ({ last_act: act, at: at(16, 2), version_id: 'wt-1', act_id: 'cam_' + act, later_work: false, ...over })

/** The live data the screen is handed: the canvas's record, with a thread's last act laid on it. */
function live(doc, { told: write } = {}) {
  const base = BRIEF.states.s1
  const snapshot = { ...base, change: { ...base.change, told: toldOf(doc, NOW) } }
  const data = selectRepairLoop(snapshot, null, { now: NOW })
  data.act = { ...data.act, told: write }
  return data
}
const spy = () => { const calls = []; return { calls, toldAct: (...a) => { calls.push(a) } } }

test('no act: the first rung is not reached, and the two controls offer to copy and to mark', () => {
  const v = M.vals({ data: live(undefined), actions: spy() })
  assert.equal(v.ladder[0].label, 'Told the agent')
  assert.equal(v.ladder[0].value, 'Not yet · prepared, not sent')
  assert.equal(v.ladder[0].bg, 'transparent')
  assert.equal(v.copyLabel, 'Copy message')
  assert.equal(v.pastedLabel, 'Mark as pasted')
  assert.equal(v.pastedPressed, 'false')
  assert.equal(v.toldLine, 'Prepared, not sent. ThruLine doesn’t run the agent.')
})

test('a copy on the record: the rung reads it at its own time, and nothing was sent', () => {
  const v = M.vals({ data: live(served('copied')), actions: spy() })
  assert.equal(v.ladder[0].value, 'Copied — not sent · 16:02')
  assert.equal(v.ladder[0].bg, '#EDEDEF', 'the rung is reached')
  assert.equal(v.copyLabel, 'Copied — not sent')
  assert.equal(v.pastedLabel, 'Mark as pasted')
  assert.match(v.toldLine, /^Copied — not sent · 16:02\. Nothing was sent; ThruLine doesn’t run the agent\.$/)
})

test('a pasted mark on the record: "You said you pasted it", with its undo', () => {
  const v = M.vals({ data: live(served('marked_pasted', { at: at(16, 12) })), actions: spy() })
  assert.equal(v.ladder[0].value, 'You said you pasted it · 16:12')
  assert.equal(v.pastedLabel, 'Marked as pasted · undo')
  assert.equal(v.pastedPressed, 'true')
  assert.equal(v.pastedBg, '#1F1F23')
  assert.match(v.toldLine, /^You said you pasted it · 16:12\. Nothing was sent by ThruLine\.$/)
})

test('a mark taken back is said as taken back, never as a copy that is waiting', () => {
  const v = M.vals({ data: live(served('unmarked_pasted', { at: at(16, 14) })), actions: spy() })
  assert.match(v.ladder[0].value, /^You took back “pasted” · 16:14 · not sent$/)
  assert.equal(v.pastedLabel, 'Mark as pasted')
  assert.match(v.toldLine, /^You took back “pasted” · 16:14\. Nothing was sent/)
})

test('Copy message keeps the words the person left in the draft, edited or not', () => {
  const a = spy()
  const inst = M.make({ data: live(undefined), actions: a })
  const pristine = inst.renderVals()
  pristine.copyMsg()
  assert.equal(a.calls.length, 1)
  assert.equal(a.calls[0][0], 'copied')
  assert.equal(a.calls[0][1].message, pristine.draft, 'the draft as drawn')
  assert.ok(a.calls[0][1].message.trim().length > 0)
  inst.state = { ...inst.state, edits: { s1: 'Please run the totals test and show me the output.' } }
  const edited = inst.renderVals()
  assert.equal(edited.draft, 'Please run the totals test and show me the output.')
  edited.copyMsg()
  assert.deepEqual(a.calls[1], ['copied', { message: 'Please run the totals test and show me the output.' }])
})

test('Mark as pasted names the copy the screen saw; its undo names the mark', () => {
  const a = spy()
  M.vals({ data: live(served('copied')), actions: a }).togglePasted()
  M.vals({ data: live(served('marked_pasted')), actions: a }).togglePasted()
  M.vals({ data: live(undefined), actions: a }).togglePasted()
  M.vals({ data: live(served('unmarked_pasted')), actions: a }).togglePasted()
  assert.deepEqual(a.calls, [
    ['marked_pasted', { refersTo: 'cam_copied' }],
    ['unmarked_pasted', { refersTo: 'cam_marked_pasted' }],
    ['marked_pasted', { refersTo: undefined }],
    ['marked_pasted', { refersTo: undefined }],
  ])
})

test('a write in flight, and a refused one, are said on the line the act is shown on', () => {
  const busy = M.vals({ data: live(served('copied'), { told: { busy: true, said: null } }), actions: spy() })
  assert.equal(busy.toldLine, 'Keeping that in the record…')
  const said = 'Copied to your clipboard, but ThruLine could not keep a record of it. There is no copied message on this piece of work. Nothing was sent.'
  const refused = M.vals({ data: live(undefined, { told: { busy: false, said } }), actions: spy() })
  assert.equal(refused.toldLine, said)
  assert.equal(refused.ladder[0].value, 'Not yet · prepared, not sent', 'a refused write is not on the record, so the rung does not move')
})

test('the screen says it in the page: the rung, the labels and the line', () => {
  const html = unescape(M.render({ data: live(served('marked_pasted', { at: at(16, 12) })), actions: spy() }))
  const text = html.replace(/<[^>]*>/g, ' ')
  assert.match(text, /Told the agent/)
  assert.match(text, /You said you pasted it · 16:12/)
  assert.match(text, /Marked as pasted · undo/)
  assert.match(text, /Nothing was sent by ThruLine\./)
})

test('the gallery keeps the canvas: two local toggles, and no write', () => {
  const a = spy()
  const fresh = M.vals({ actions: a })
  assert.equal(fresh.copyLabel, 'Copy message')
  assert.equal(fresh.pastedLabel, 'Mark as pasted')
  fresh.togglePasted()
  fresh.copyMsg()
  assert.deepEqual(a.calls, [], 'the gallery never writes')
  const copied = M.vals({ actions: a }, { copied: true })
  assert.equal(copied.copyLabel, 'Copied — not sent')
  assert.match(copied.ladder[0].value, /^Copied — not sent · /)
  const pasted = M.vals({ actions: a }, { pasted: true })
  assert.equal(pasted.pastedLabel, 'Marked as pasted · undo')
  assert.match(pasted.toldLine, /Not sent by ThruLine\.$/)
})
