/* C30 R-LEDGER: the Inbox files a thread under "Waiting on the agent" when the
 * person copied a note for it (or said they pasted it) and no work has been
 * recorded since — from the thread's `told_agent`, the project view's one new
 * field.
 *
 * Only the FILING moves. Every row keeps the engine's move, so the headline,
 * the filter chips and the sidebar badge are still the engine's counts, and a
 * filter's count equals the rows it shows. Nothing was sent, and every
 * sentence a copy or a mark appears in says so. Synthetic fixtures only. */
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
import * as P from '../../data/project.js'
import { LIST, VIEW, NOW, at } from '../../data/project-fixture.js'
import Logic from './logic.js'
import { inboxText } from './text.js'
export { NOW, at, VIEW, LIST }
export function told(act, over = {}) { return { last_act: act, at: at(16, 2), version_id: 'wt-1', act_id: 'cam_' + act, later_work: false, ...over } }
export function viewWith(byId) { return { ...VIEW, threads: VIEW.threads.map((t) => (byId[t.id] ? { ...t, told_agent: byId[t.id] } : t)) } }
// the readiness is read and nothing is ready (C46 §1.1: without a reading there is no badge at all)
export function data(view) { return P.inboxData({ projects: LIST, view, ready: {}, now: NOW }) }
export function vals({ view, filter = 'all' } = {}) {
  const props = { data: view ? data(view) : undefined }
  const inst = new (P.withFrame(Logic))(props)
  inst.props = props
  inst.state = { ...inst.state, filter }
  return inst.renderVals()
}
export function text(view, ui) { return inboxText(data(view), ui) }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-inbox-told-')), 'inbox.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const ids = (rows) => rows.map((r) => r.id)

test('a copied note with no later work files the thread under "Waiting on the agent", at the act’s own time', () => {
  const view = M.viewWith({ np1: M.told('copied') })
  const row = M.data(view).changes.np1
  assert.equal(row.noteCopiedAt, '16:02')
  assert.equal(row.toldAct, 'copied')
  const t = M.text(view)
  assert.deepEqual(ids(t.groups.agent), ['np1'])
  assert.ok(!ids(t.groups.check).includes('np1'), 'it left "Needs a check"')
  assert.ok(ids(t.groups.check).includes('re1'), 'the others stayed')
  const r = t.groups.agent[0].t
  assert.equal(r.rowLine, 'You copied a note for the agent at 16:02 · nothing was sent')
  assert.match(r.pvLead, /^You copied a note for the agent at 16:02\. No later work has been recorded since\. Nothing was sent\.$/)
  assert.match(r.aria, /You copied a note for the agent at 16:02\. Nothing was sent\./)
  assert.equal(r.note, 'Nothing was sent. ThruLine doesn’t run the agent.')
})

test('a pasted mark is told in its own words, and "copy again" still reads the page’s own time', () => {
  const view = M.viewWith({ np1: M.told('marked_pasted', { at: M.at(16, 12) }) })
  const r = M.text(view).groups.agent[0].t
  assert.equal(r.rowLine, 'You said you pasted the note at 16:12 · nothing was sent')
  assert.match(r.pvLead, /^You said you pasted a note for the agent at 16:12\./)
  assert.match(r.pvLead, /Nothing was sent\.$/)
  const again = M.text(view, { copied: 'np1' }).groups.agent[0].t
  assert.match(again.rowLine, /^You copied the note again at /)
  assert.match(again.note, /nothing was sent\.$/)
})

test('work recorded since, a mark taken back, or an act the engine cannot place leave the thread where the engine put it', () => {
  for (const [name, doc] of [
    ['later work', M.told('copied', { later_work: true })],
    ['a mark taken back', M.told('unmarked_pasted')],
    ['unknown later work', (() => { const d = M.told('copied'); delete d.later_work; return d })()],
    ['an unreadable time', M.told('copied', { at: 'not a time' })],
  ]) {
    const view = M.viewWith({ np1: doc })
    assert.equal(M.data(view).changes.np1.noteCopiedAt, undefined, name)
    const t = M.text(view)
    assert.deepEqual(ids(t.groups.agent), [], name)
    assert.ok(ids(t.groups.check).includes('np1'), name)
  }
})

test('only a thread that is the person’s move to check is filed as waiting: not a question, not a recording one', () => {
  const view = M.viewWith({ ask1: M.told('copied'), live1: M.told('copied'), done1: M.told('copied') })
  const d = M.data(view)
  assert.equal(d.changes.ask1.noteCopiedAt, undefined, 'a question only the person can answer stays theirs')
  assert.equal(d.changes.live1.noteCopiedAt, undefined, 'a thread being recorded now stays under Recording now')
  const t = M.text(view)
  assert.deepEqual(ids(t.groups.agent), [])
  assert.ok(ids(t.groups.live).length > 0)
})

test('every count is still the engine’s: the headline, the chips and the sidebar badge do not move', () => {
  const none = M.text(M.viewWith({}))
  const some = M.text(M.viewWith({ np1: M.told('copied'), re1: M.told('marked_pasted', { at: M.at(16, 12) }) }))
  assert.equal(some.headline, none.headline)
  assert.equal(some.n.yourMove, none.n.yourMove)
  assert.equal(some.n.agentMove, none.n.agentMove)
  assert.equal(some.footer, none.footer)
  assert.equal(some.navYou, none.navYou, 'the badge is the one count (data/project.js yourMove), and nothing here is ready')
  assert.equal(some.navYou, '3')
  assert.deepEqual([some.n.agent, some.n.check], ['2', '0'], 'the rows moved between groups')
  assert.deepEqual([none.n.agent, none.n.check], ['0', '2'])
})

test('every filter’s count equals the rows it shows, with waiting rows on the record', () => {
  const view = M.viewWith({ np1: M.told('copied') })
  const shown = (v) => {
    let n = 0
    for (const g of ['you', 'check', 'agent', 'live']) if (v.g[g] === 'flex') n += v.groups[g].length
    return n
  }
  const chip = (v, key) => v.filters.find((f) => f.key === key).count
  for (const filter of ['you', 'agent', 'live']) {
    const v = M.vals({ view, filter })
    assert.equal(String(shown(v)), chip(v, filter), 'filter ' + filter)
  }
  const you = M.vals({ view, filter: 'you' })
  assert.equal(you.g.agent, 'flex', '"Your move" shows the waiting row, which is still the engine’s your-move')
  assert.deepEqual(ids(you.groups.agent), ['np1'])
  assert.equal(you.t.n.agent, '1', 'its header counts the rows it shows')
  const agent = M.vals({ view, filter: 'agent' })
  assert.equal(agent.g.agent, 'none', 'the engine does not say it is the agent’s move, so "The agent’s" does not show it')
  const live = M.vals({ view, filter: 'live' })
  assert.equal(live.g.agent, 'none')
  const all = M.vals({ view, filter: 'all' })
  assert.equal(all.g.agent, 'flex')
  assert.equal(all.t.n.agent, '1')
})

test('the gallery’s "Waiting on the agent" is still what the canvas drew, under All and "The agent’s"', () => {
  const all = M.vals({ filter: 'all' })
  const agent = M.vals({ filter: 'agent' })
  const you = M.vals({ filter: 'you' })
  assert.equal(all.g.agent, 'flex')
  assert.equal(agent.g.agent, 'flex')
  assert.equal(you.g.agent, 'none')
  assert.ok(all.groups.agent.length >= 1)
  assert.equal(agent.groups.agent.length, all.groups.agent.length)
})

test('no thread is filed as waiting until the project view serves told_agent', () => {
  const t = M.text(M.viewWith({}))
  assert.deepEqual(ids(t.groups.agent), [])
  assert.equal(t.n.agent, '0')
})

test('"Copy it again" on a row from the record opens the sheet where the copy is real, and claims no copy', () => {
  const view = M.viewWith({ np1: M.told('copied') })
  const row = M.vals({ view }).groups.agent[0]
  assert.equal(row.recorded, true)
  assert.equal(row.tellHref, M.data(view).changes.np1.checkHref)
  const had = globalThis.window
  globalThis.window = { location: { hash: '' } }
  try {
    row.copyAct()
    assert.equal(globalThis.window.location.hash, row.tellHref, 'it goes where Copy message copies the draft and keeps the act')
    // the gallery's canvas row keeps its toggle and goes nowhere
    const gallery = M.vals({}).groups.agent[0]
    assert.equal(gallery.recorded, false)
    globalThis.window.location.hash = ''
    gallery.copyAct()
    assert.equal(globalThis.window.location.hash, '')
  } finally {
    if (had === undefined) delete globalThis.window
    else globalThis.window = had
  }
})
