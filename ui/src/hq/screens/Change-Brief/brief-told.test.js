/* C30 R-LEDGER: the Brief's "Tell the agent" is the record's too. Pressing it
 * copies the message — the page does — and keeps that it was copied; the
 * button and the "Told the agent" line then read the thread's last act, with
 * the time, and say nothing was sent. A refused write is said on that line.
 * The gallery keeps the canvas's local toggle. Synthetic fixtures only. */
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
import { withFrame } from '../../data/project.js'
import { changeSnapshot } from '../../data/change.js'
import { threadFixture } from '../What-Changed/thread-fixture.js'
import Logic from './logic.js'
export { changeSnapshot, threadFixture }
export function vals(props, state) {
  const L = withFrame(Logic)
  const inst = new L(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return inst.renderVals()
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'brief-told-')), 'brief.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const NOW = new Date(2026, 9, 8, 17, 0)
const at = (h, m) => new Date(2026, 9, 8, h, m).toISOString()
const served = (act, over = {}) => ({ last_act: act, at: at(16, 2), version_id: 'wt-1', act_id: 'cam_' + act, later_work: false, ...over })
const snapshot = (told) => M.changeSnapshot(M.threadFixture(), null, NOW, { told })
const screen = (told, write) => {
  const calls = []
  const v = M.vals({ data: snapshot(told), actions: { tellAgent: (m) => { calls.push(m) }, told: write }, frame: {} })
  return { v, calls }
}
/** Every `{t, s}` line of the screen's value, whichever panel holds it. */
function lines(o, out = [], seen = new Set()) {
  if (!o || typeof o !== 'object' || seen.has(o)) return out
  seen.add(o)
  if (typeof o.t === 'string' && typeof o.s === 'string') out.push(o)
  for (const k of Object.keys(o)) if (typeof o[k] === 'object') lines(o[k], out, seen)
  return out
}
const rung = (v) => lines(v).filter((l) => l.t === 'Told the agent')

test('no act: the button offers to tell the agent, and the line says it has not been', () => {
  const { v } = screen(undefined)
  assert.equal(v.tellLabel, 'Tell the agent')
  const r = rung(v)
  assert.ok(r.length > 0, 'the "Told the agent" line is drawn')
  for (const l of r) assert.match(l.s, /^— (not yet|not needed)$/)
})

test('a copy on the record: the button and the line say it, at its own time, and nothing was sent', () => {
  const { v } = screen(served('copied'))
  assert.equal(v.tellLabel, 'Copied — not sent')
  for (const l of rung(v)) assert.equal(l.s, '— copied, not sent · 16:02 · nothing was sent')
})

test('a pasted mark is told as the person’s own word; a mark taken back as taken back', () => {
  for (const l of rung(screen(served('marked_pasted', { at: at(16, 12) })).v)) assert.equal(l.s, '— you said you pasted it · 16:12 · nothing was sent')
  const undone = screen(served('unmarked_pasted', { at: at(16, 14) })).v
  assert.equal(undone.tellLabel, 'Tell the agent', 'a copy that was taken back is not standing')
  for (const l of rung(undone)) assert.equal(l.s, '— you took back “pasted” · 16:14 · nothing was sent')
})

test('pressing it copies the message and asks for the copy to be kept — every press', () => {
  const { v, calls } = screen(undefined)
  v.tell()
  v.tell()
  assert.equal(calls.length, 2)
  assert.match(calls[0], /^About “/, 'the same message the clipboard gets')
  assert.ok(calls[0].trim().length > 0)
})

test('the Brief’s Tell the agent message keeps a served exclusion marker', () => {
  const marker = 'left out by your exclusion (`checkout/api.py`)'
  const view = M.threadFixture()
  for (const claim of view.claims || []) claim.text = marker
  for (const edit of view.edits || []) edit.file = marker
  for (const command of view.commands || []) command.command = marker
  const calls = []
  M.vals({ data: M.changeSnapshot(view, null, NOW), actions: { tellAgent: (message) => calls.push(message) }, frame: {} }).tell()
  assert.equal(calls.length, 1)
  assert.ok(calls[0].includes(marker))
})

test('a write in flight, and a refused one, are said on the line, and the button does not claim a copy', () => {
  const busy = screen(undefined, { busy: true, said: null }).v
  for (const l of rung(busy)) assert.equal(l.s, '— keeping that in the record…')
  const said = 'copied to your clipboard, but ThruLine could not keep a record of it. ThruLine needs an upgrade before it can record anything, so nothing was saved. Nothing was sent.'
  const refused = screen(undefined, { busy: false, said }).v
  assert.equal(refused.tellLabel, 'Tell the agent', 'a refused write is not on the record')
  for (const l of rung(refused)) assert.equal(l.s, '— ' + said)
})

test('the gallery keeps the canvas: one local toggle, and no write', () => {
  const calls = []
  const L = M.vals({ actions: { tellAgent: (m) => calls.push(m) } })
  assert.equal(L.tellLabel, 'Tell the agent')
  L.tell()
  assert.deepEqual(calls, [], 'the gallery never writes')
  assert.equal(M.vals({ actions: {} }, { told: true }).tellLabel, 'Copied — not sent')
})
