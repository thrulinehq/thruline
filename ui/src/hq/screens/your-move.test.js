/* "Your move": ONE source, and every screen agrees (C46 §1.1; C30 P0-5).
 *
 * After 997aef3e the Inbox counted a tracked Change that is ready for a decision on a thread the
 * engine calls finished (its approve row), while every other screen's badge read the engine's
 * `whose_move.you` alone and said one fewer. Here one synthetic record is served by a fake local
 * server, the readiness is read through the real join (inbox-ready.js → use-change.js
 * `resolveChange`), and every number a screen says about the person's move is compared: the
 * sidebar badge on the Inbox, Changes, Projects, Record over time and Team; the Inbox's headline,
 * "Your move" chip, footer and the rows that filter draws; Changes' rows, chips and headline; and
 * the words ⌘K's Find rows say. Synthetic fixtures only; nothing here reaches a real server. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFileSync, readdirSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))

const M = await (async () => {
  const names = ['Inbox', 'Changes', 'Projects', 'Record-Over-Time', 'Team-View']
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import * as P from '../data/project.js'
import * as Y from '../data/your-move.js'
import { readReadyChanges, readyNote } from '../data/inbox-ready.js'
import { inboxText } from './Inbox/text.js'
import { changesData } from './Changes/from-projects.js'
import { projectsData } from './Projects/from-projects.js'
import { recordFromProjects } from './Record-Over-Time/from-projects.js'
import { teamFromStore } from './Team-View/from-store.js'
import { commandFromChange } from './Command-Ask/from-change.js'
${names.map((n, i) => `import T${i} from './${n}/template.jsx'\nimport L${i} from './${n}/logic.js'`).join('\n')}
const S = { ${names.map((n, i) => `'${n}': [T${i}, L${i}]`).join(', ')} }
export { P, Y, readReadyChanges, readyNote, inboxText, changesData, projectsData, recordFromProjects, teamFromStore, commandFromChange }
function inst(name, props, state) {
  const [, Logic] = S[name]
  const i = new (P.withFrame(Logic))(props)
  i.props = props
  if (state) i.state = { ...i.state, ...state }
  return i
}
export function vals(name, props, state) { return inst(name, props, state).renderVals() }
export function render(name, props, state) {
  const [Template] = S[name]
  return renderToStaticMarkup(React.createElement(Template, { v: inst(name, props, state).renderVals(), link: (h) => h }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-your-move-')), 'screens.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

/* ---- the synthetic record ---- */

const NOW = new Date('2026-10-08T16:10:00.000Z')
const at = (h, m) => new Date(Date.UTC(2026, 9, 8, h, m)).toISOString()
// a thread id is sha256(its first work prompt's event id)[..16] (docs/hq-next/API.md): resolveChange walks it
const sha16 = (s) => createHash('sha256').update(s).digest('hex').slice(0, 16)
const tid = (name) => sha16('ev-' + name)

const none = { total: 0, succeeded: 0, failed: 0, unknown: 0 }
const th = (name, over) => Object.assign({
  id: tid(name), title: `Thread ${name}`, title_basis: 'work_sentence', started_at: at(15, 0), last_activity_at: at(15, 30),
  sessions: 1, prompts: 1, conversation: 0, edits: 0, files: 0, top_files: [], commands: none, tests: none, commits: 0, claims: 1,
  whose_move: 'done', whose_move_basis: 'nothing_open',
  proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: [] }, current_version_id: '3f9a1c2deadbeef',
}, over)
const you = (name, over) => th(name, Object.assign({ whose_move: 'you', whose_move_basis: 'claim_without_passing_test' }, over))
const agent = (name, over) => th(name, Object.assign({ whose_move: 'agent', whose_move_basis: 'recent_activity', last_activity_at: at(16, 5) }, over))
const asks = (name, over) => you(name, Object.assign({ whose_move_basis: 'agent_asked', last_activity_at: at(16, 4) }, over))
const told = (act, over = {}) => ({ last_act: act, at: at(16, 2), version_id: 'wt-1', act_id: 'cam_' + act, later_work: false, ...over })
const recheck = (name, over) => you(name, Object.assign({
  proof: { last_test: { outcome: 'succeeded' }, last_passing_test_at: at(15, 0), edited_after_last_pass: ['/Users/me/code/shop/a.py'] },
}, over))

/** A project row and its view, with `whose_move` what the engine counts from the threads. */
function project(id, name, ledger, last, threads) {
  const n = (m) => threads.filter((t) => t.whose_move === m).length
  const row = {
    complete: true, id, name, root: '/Users/me/code/' + name, sources: ['claude_code'], members: [{ ledger_project_id: ledger, name }],
    sessions: threads.length, prompts: threads.length, claims: threads.length, threads: threads.length, last_activity: last,
    whose_move: { you: n('you'), agent: n('agent'), done: n('done') },
  }
  return { row, view: { schema_version: 1, project: row, threads } }
}

// every kind of thread at once (the C45 mix), in the project in front of the person
const SHOP = project('shop', 'shop', 'L1', at(16, 5), [
  agent('live1'), agent('live2'),
  recheck('re1'), you('np1'), you('np2', { told_agent: told('copied') }), you('np3', { told_agent: told('copied', { later_work: true }) }),
  asks('ask1'), asks('ask2', { told_agent: told('copied') }),
  you('rd2'), // the person's move, and its tracked Change is ready: the approve row instead
  th('rd1'), // finished, and its tracked Change is ready: one MORE of the person's move than the engine counted
  agent('rd3'), // ready, but the agent is working on it now: stays the agent's
  th('nr1'), // finished, its tracked Change is not ready
  th('done1'), th('done2'),
])
// a second project, older, with a ready Change on finished work too
const NOTES = project('notes', 'notes', 'L2', at(9, 0), [you('n1'), th('n2'), th('n3')])
const LIST = { schema_version: 1, complete: true, projects: [NOTES.row, SHOP.row] }

const READY_DOC = { readiness: { ready: true, with_exceptions: false, headline_slug: 'readiness_ready', target_version: 'c0ffee1234567890', scope_version: 1, obligations: [{ kind: 'criterion', outcome: 'satisfied', criterion_index: 1 }] } }
const NOT_READY_DOC = { readiness: { ready: false, with_exceptions: false, headline_slug: 'readiness_not_ready', target_version: 'c0ffee1234567890', obligations: [] } }
const change = (name, ledger, ready) => ({
  id: 'chg_' + name, project_id: ledger, opened_at: at(14, 0), ready,
  associations: [{ session_id: 's-' + name, provenance: 'confirmed', asserted_at: at(14, 1) }],
})
const CHANGES = [
  change('rd1', 'L1', true), change('rd2', 'L1', true), change('rd3', 'L1', true), change('ask1', 'L1', true), change('nr1', 'L1', false),
  change('n2', 'L2', true),
]
const HQ_OF = { L1: 'shop', L2: 'notes' }

/** The fake local server: every GET the readiness read and the join make. */
function server({ fail = () => false, hides = {} } = {}) {
  const seen = []
  const get = async (url) => {
    seen.push(url)
    if (fail(url)) throw new Error(url + ' answered 500')
    if (url === '/api/hq/projects') return LIST
    if (url === '/api/hq/projects/shop') return SHOP.view
    if (url === '/api/hq/projects/notes') return NOTES.view
    if (url === '/api/hq/projects/shop/inbox-hides') return { project: 'shop', rows: hides.shop || [] }
    if (url === '/api/hq/projects/notes/inbox-hides') return { project: 'notes', rows: hides.notes || [] }
    if (url === '/api/changes') return { changes: CHANGES.map(({ ready, ...c }) => c) }
    let m = url.match(/^\/api\/changes\/(chg_\w+)\/readiness$/)
    if (m) return CHANGES.find((c) => c.id === m[1]).ready ? READY_DOC : NOT_READY_DOC
    m = url.match(/^\/api\/changes\/(chg_\w+)$/)
    if (m) { const { ready, ...c } = CHANGES.find((x) => x.id === m[1]); return { change: c } }
    m = url.match(/^\/api\/hq\/sessions\/s-(\w+)$/)
    if (m) {
      const c = CHANGES.find((x) => x.id === 'chg_' + m[1])
      return { hq_project_id: HQ_OF[c.project_id], items: [{ kind: 'prompt', event_id: 'ev-' + m[1] }] }
    }
    throw new Error('no such address in the fixture: ' + url)
  }
  return { get, seen }
}

/* ---- what each screen draws ---- */

const badge = (html) => {
  const m = html.match(/>Inbox<\/span> <span aria-label="[^"]*"[^>]*>(\d+)<\/span>/)
  return m ? Number(m[1]) : null
}
const shown = (v) => ['you', 'check', 'agent', 'live'].filter((g) => v.g[g] === 'flex').reduce((n, g) => n + v.groups[g].length, 0)
const chip = (v, key) => Number(v.filters.find((f) => f.key === key).count)
const WORD = ['No', 'One', 'Two', 'Three', 'Four', 'Five', 'Six', 'Seven', 'Eight', 'Nine', 'Ten', 'Eleven', 'Twelve']
const headCount = (headline) => WORD.indexOf(headline.split(' ')[0])

/** Every number every screen says about the person's move, for one record and the readings of it. */
function everyScreen({ inboxReady, readings, reading }) {
  const out = {}
  const inboxD = M.P.inboxData({ projects: LIST, view: SHOP.view, ready: inboxReady && inboxReady.ready, readyFailed: inboxReady ? inboxReady.failed + (inboxReady.listFailed ? 1 : 0) : 0, now: NOW })
  const t = M.inboxText(inboxD)
  const youV = M.vals('Inbox', { data: inboxD }, { filter: 'you' })
  out.inbox = {
    badge: badge(M.render('Inbox', { data: inboxD, frame: {} })),
    headline: headCount(t.headline),
    chip: chip(youV, 'you'),
    footer: Number(t.footer.split(' ')[0]),
    rows: shown(youV),
  }
  const chD = M.changesData({ projects: LIST, views: { shop: SHOP.view, notes: NOTES.view }, readings, now: NOW, current: 'shop' })
  const chYou = M.vals('Changes', { data: chD }, { filter: 'you' })
  const shopBlock = chYou.blocks.find((b) => b.key === 'shop')
  out.changes = {
    badge: badge(M.render('Changes', { data: chD, frame: {} })),
    shopRows: shopBlock && shopBlock.groups ? ['you', 'check', 'agent', 'live'].reduce((n, g) => n + shopBlock.groups[g].length, 0) : null,
    chip: chip(chYou, 'you'),
    headline: headCount(chYou.t.headline),
  }
  out.projects = { badge: badge(M.render('Projects', { data: M.projectsData({ projects: LIST, storage: null, now: NOW, current: 'shop', reading }), frame: {} })) }
  out.record = { badge: badge(M.render('Record-Over-Time', { data: M.recordFromProjects({ projects: LIST.projects, storage: null, now: NOW.getTime(), current: 'shop', reading }), frame: {} })) }
  out.team = { badge: badge(M.render('Team-View', { data: M.teamFromStore({ project: SHOP.row, projects: LIST.projects, changes: [], storage: null, licence: null, now: NOW.getTime(), reading }), frame: {} })) }
  const cmd = M.commandFromChange(null, SHOP.view, { readAt: '16:10', ready: reading && reading.ready })
  out.command = Object.fromEntries(cmd.findable.map((r) => [r.threadId, r.whoseMove]))
  return out
}

const SHOP_YOU = SHOP.row.whose_move.you + 1 // + rd1: finished work whose tracked Change is ready
const NOTES_YOU = NOTES.row.whose_move.you + 1 // + n2

test('the shared reading reads the readiness through the real join and counts the finished thread a ready Change files as an approve row', async () => {
  const s = server()
  const reading = await M.Y.readYourMove(SHOP.row, { get: s.get })
  assert.equal(reading.project, 'shop')
  assert.deepEqual(Object.keys(reading.ready).sort(), [tid('ask1'), tid('rd1'), tid('rd2'), tid('rd3')].sort())
  assert.equal(M.P.yourMove(SHOP.row, reading), SHOP_YOU, 'the engine’s count plus the one finished thread whose Change is ready')
  // the view is read once, for the join and the threads' moves both
  assert.equal(s.seen.filter((u) => u === '/api/hq/projects/shop').length, 1, s.seen.join('\n'))
  assert.equal(s.seen.filter((u) => u === '/api/hq/projects/shop/inbox-hides').length, 1, s.seen.join('\n'))
})

test('a hidden ready row is absent from the shared badge', async () => {
  const s = server({ hides: { shop: [{ thread_id: tid('rd1'), row_kind: 'thread', row_sha256: 'a'.repeat(64), hidden: true }] } })
  const reading = await M.Y.readYourMove(SHOP.row, { get: s.get })
  assert.equal(reading.hiddenYourMove, 1)
  assert.equal(M.P.yourMove(SHOP.row, reading), SHOP_YOU - 1)
})

test('every badge, headline, chip, footer and list says the same number of the person’s move, on every screen', async () => {
  const s = server()
  const reading = await M.Y.readYourMove(SHOP.row, { get: s.get })
  const notesReading = await M.Y.readYourMove(NOTES.row, { get: s.get })
  const inboxReady = await M.readReadyChanges(SHOP.view, { get: s.get })
  const got = everyScreen({ inboxReady, readings: { shop: reading, notes: notesReading }, reading })

  assert.deepEqual(got.inbox, { badge: SHOP_YOU, headline: SHOP_YOU, chip: SHOP_YOU, footer: SHOP_YOU, rows: SHOP_YOU }, 'Inbox')
  assert.equal(got.changes.badge, SHOP_YOU, 'Changes: the sidebar badge')
  assert.equal(got.changes.shopRows, SHOP_YOU, 'Changes: the shop block’s rows under "Your move"')
  assert.equal(got.changes.chip, SHOP_YOU + NOTES_YOU, 'Changes: the "Your move" chip counts every project’s rows')
  assert.equal(got.changes.headline, SHOP_YOU + NOTES_YOU, 'Changes: the headline counts every project’s rows')
  assert.equal(got.projects.badge, SHOP_YOU, 'Projects')
  assert.equal(got.record.badge, SHOP_YOU, 'Record over time')
  assert.equal(got.team.badge, SHOP_YOU, 'Team')
  // ⌘K's Find rows say whose move each thread is in the Inbox's own words
  assert.equal(got.command[tid('rd1')], 'you', '⌘K: finished work whose Change is ready is the person’s move')
  assert.equal(got.command[tid('nr1')], 'done')
  assert.equal(got.command[tid('rd3')], 'agent', 'the agent is still working on it')
  assert.equal(Object.values(got.command).filter((w) => w === 'you').length, SHOP_YOU, '⌘K counts the same threads as the person’s move')
})

test('until the readiness is read, no screen draws a badge: never a number that may be one short', () => {
  const got = everyScreen({ inboxReady: null, readings: {}, reading: null })
  for (const k of ['inbox', 'changes', 'projects', 'record', 'team']) assert.equal(got[k].badge, null, k)
  assert.equal(M.P.yourMove(SHOP.row, null), null)
  assert.equal(M.P.yourMove(SHOP.row, { project: 'notes', ready: {}, moves: {}, failed: 0, listFailed: false }), null, 'another project’s reading is not this one’s')
  assert.equal(M.P.yourMove(SHOP.row, { project: 'shop', ready: {}, moves: {}, failed: 0, listFailed: false }), SHOP.row.whose_move.you, 'read, and nothing is ready: the engine’s count')
})

test('a readiness look that failed is no count either, and the Inbox says what it could not check', async () => {
  const s = server({ fail: (u) => u === '/api/changes/chg_rd1/readiness' })
  const reading = await M.Y.readYourMove(SHOP.row, { get: s.get })
  assert.equal(reading.failed, 1)
  assert.equal(M.P.yourMove(SHOP.row, reading), null)
  const inboxReady = await M.readReadyChanges(SHOP.view, { get: s.get })
  const got = everyScreen({ inboxReady, readings: { shop: reading }, reading })
  for (const k of ['inbox', 'changes', 'projects', 'record', 'team']) assert.equal(got[k].badge, null, k)
  assert.equal(M.readyNote(inboxReady), 'could not check 1 Change for approval', 'beside the read time')
  // the rows it could file are still drawn, and the headline counts exactly them
  assert.equal(got.inbox.headline, got.inbox.rows)
  assert.equal(got.inbox.chip, got.inbox.rows)
  const listFailed = await M.Y.readYourMove(SHOP.row, { get: server({ fail: (u) => u === '/api/changes' }).get })
  assert.equal(M.P.yourMove(SHOP.row, listFailed), null, 'the list of Changes could not be read')
})

test('one reading per project is kept and shared: a screen opened after the Inbox has it at once', async () => {
  M.Y.forgetYourMove()
  const s = server()
  const a = M.Y.readYourMove(SHOP.row, { get: s.get, keep: true })
  const b = M.Y.readYourMove(SHOP.row, { get: s.get, keep: true })
  assert.equal(a, b, 'two screens asking at once share one read')
  const reading = await a
  assert.equal(M.Y.keptYourMove('shop'), reading)
  assert.equal(M.Y.keptYourMove('notes'), null)
  // what the Inbox read itself is kept the same way, in the same shape
  const inboxReady = await M.readReadyChanges(SHOP.view, { get: s.get })
  const fromInbox = M.P.readingOf(SHOP.view, inboxReady)
  M.Y.keepYourMove(fromInbox)
  assert.equal(M.P.yourMove(SHOP.row, M.Y.keptYourMove('shop')), SHOP_YOU)
  M.Y.forgetYourMove()
})

test('census: "your move" is counted in one function; no other file adds to the engine’s count or counts it itself', () => {
  const root = join(here, '..')
  const files = []
  const walk = (d) => {
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, e.name)
      if (e.isDirectory()) walk(p)
      else if (/\.(js|jsx)$/.test(e.name) && !e.name.endsWith('.test.js') && e.name !== 'demo.js' && !e.name.endsWith('fixture.js')) files.push(p)
    }
  }
  walk(root)
  const offenders = []
  for (const f of files) {
    const src = readFileSync(f, 'utf8')
    const rel = relative(root, f)
    // `whose_move.you` is read in exactly one place outside the filing rule: the one count
    const reads = (src.match(/whose_move\.you\b/g) || []).length
    if (reads && rel !== join('data', 'project.js')) offenders.push(rel + ' reads whose_move.you')
    if (/readyFromDone/.test(src) && rel !== join('data', 'project.js')) offenders.push(rel + ' adds ready Changes itself')
  }
  assert.deepEqual(offenders, [])
  const project = readFileSync(join(root, 'data', 'project.js'), 'utf8')
  assert.equal((project.match(/whose_move\.you\b/g) || []).length, 1, 'data/project.js reads whose_move.you once, in yourMove')
})
