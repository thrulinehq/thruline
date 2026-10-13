/* A count and its list agree (C30 P0-5, again over a mixed project).
 *
 * The Inbox's "Your move" chip once said 3 and drew 4 rows. Every number the
 * screen states — the headline, each filter chip, each group header, the
 * sidebar badge and the footer — must equal the rows it stands for, whatever
 * mix of threads the project holds: the agent's, the person's, questions only
 * the person can answer, notes copied for the agent, finished work, and a
 * tracked Change that is ready for a decision (the approve row).
 * Synthetic fixtures only; nothing here reaches a server. */
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
import { NOW, at } from '../../data/project-fixture.js'
import Logic from './logic.js'
import { inboxText } from './text.js'
export { NOW, at }
export function data(view, ready, hides) { return P.inboxData({ projects: { projects: [view.project] }, view, ready, hides, now: NOW }) }
export function vals(d, filter, hideRow) {
  const props = { data: d, hideRow }
  const inst = new (P.withFrame(Logic))(props)
  inst.props = props
  inst.state = { ...inst.state, filter }
  return inst.renderVals()
}
export const text = (d) => inboxText(d, {})`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-inbox-counts-')), 'inbox.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const none = { total: 0, succeeded: 0, failed: 0, unknown: 0 }
const th = (id, over) => Object.assign({
  id, title: `Thread ${id}`, title_basis: 'work_sentence', started_at: M.at(15, 0), last_activity_at: M.at(15, 30),
  sessions: 1, prompts: 1, conversation: 0, edits: 0, files: 0, top_files: [], commands: none, tests: none, commits: 0, claims: 1,
  whose_move: 'done', whose_move_basis: 'nothing_open',
  proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: [] }, current_version_id: '3f9a1c2deadbeef',
}, over)
const you = (id, over) => th(id, Object.assign({ whose_move: 'you', whose_move_basis: 'claim_without_passing_test' }, over))
const agent = (id, over) => th(id, Object.assign({ whose_move: 'agent', whose_move_basis: 'recent_activity', last_activity_at: M.at(16, 5) }, over))
const asks = (id, over) => you(id, Object.assign({ whose_move_basis: 'agent_asked', last_activity_at: M.at(16, 4) }, over))
const told = (act, over = {}) => ({ last_act: act, at: M.at(16, 2), version_id: 'wt-1', act_id: 'cam_' + act, later_work: false, ...over })
const recheck = (id, over) => you(id, Object.assign({
  proof: { last_test: { outcome: 'succeeded' }, last_passing_test_at: M.at(15, 0), edited_after_last_pass: ['/Users/me/code/shop/a.py'] },
}, over))

/** A project view whose `whose_move` is what the engine would count from its threads. */
function view(threads) {
  const n = (m) => threads.filter((t) => t.whose_move === m).length
  return {
    schema_version: 1,
    project: { id: 'p1', name: 'shop', root: '/Users/me/code/shop', threads: threads.length, last_activity: M.at(16, 5), sessions: threads.length, whose_move: { you: n('you'), agent: n('agent'), done: n('done') } },
    threads,
  }
}
const readyFor = (change, version = 'c0ffee1') => ({
  change, version, criteria: { version: 'v1', lines: [{ n: 1, state: 'proven', gate: false }, { n: 2, state: 'proven', gate: false }] },
})

/** Every kind of thread at once, so no count can lean on a project that has only a few. */
const MIX = [
  agent('live1'), agent('live2'),
  recheck('re1'), you('np1'), you('np2', { told_agent: told('copied') }), you('np3', { told_agent: told('copied', { later_work: true }) }),
  asks('ask1'), asks('ask2', { told_agent: told('copied') }),
  you('rd2'), // the person's move on a thread whose tracked Change is ready
  th('rd1'), // finished work whose tracked Change is ready
  th('rd3', { whose_move: 'agent', whose_move_basis: 'recent_activity' }), // ready, but the agent is working on it now
  th('done1'), th('done2'),
]
const READY = { rd1: readyFor('chg_rd1'), rd2: readyFor('chg_rd2'), rd3: readyFor('chg_rd3'), ask1: readyFor('chg_ask1') }

const WORD = ['No', 'One', 'Two', 'Three', 'Four', 'Five', 'Six', 'Seven', 'Eight', 'Nine', 'Ten']
const shown = (v) => ['you', 'check', 'agent', 'live'].filter((g) => v.g[g] === 'flex').reduce((n, g) => n + v.groups[g].length, 0)
const chip = (v, key) => v.filters.find((f) => f.key === key).count
const drawn = (v) => ['you', 'check', 'agent', 'live'].filter((g) => v.g[g] === 'flex').flatMap((g) => v.groups[g])

for (const [name, withReady] of [['without a ready Change', false], ['with ready Changes', true]]) {
  test(`every number equals the rows it stands for, over a mixed project ${name}`, () => {
    // the readiness is read either way (C46 §1.1: with none read there is no badge, screens/your-move.test.js)
    const D = M.data(view(MIX), withReady ? READY : {})
    const t = M.text(D)
    const youRows = M.vals(D, 'you')
    const agentRows = M.vals(D, 'agent')
    const liveRows = M.vals(D, 'live')
    const allRows = M.vals(D, 'all')

    // each chip is the rows its filter draws
    assert.equal(String(shown(youRows)), chip(youRows, 'you'), 'Your move')
    assert.equal(String(shown(agentRows)), chip(agentRows, 'agent'), 'The agent’s')
    assert.equal(String(shown(liveRows)), chip(liveRows, 'live'), 'Recording')

    // the headline says the same two numbers the chips do
    assert.equal(t.n.yourMove, chip(youRows, 'you'))
    assert.equal(t.n.agentMove, chip(agentRows, 'agent'))
    assert.ok(t.headline.startsWith(`${WORD[Number(t.n.yourMove)]} things are your move.`), t.headline)
    assert.ok(t.headline.endsWith(`${WORD[Number(t.n.agentMove)]} are the agent’s.`), t.headline)
    assert.ok(t.footer.startsWith(`${t.n.yourMove} Changes are your move · ${t.n.agentMove} are the agent’s · ${t.n.live} sessions recording`), t.footer)

    // the sidebar badge is the person’s move: the rows "Your move" draws
    assert.equal(t.navYou, String(shown(youRows)), 'the sidebar badge')
    assert.match(t.navYouAria, new RegExp(`^${t.navYou} Changes are your move$`))

    // All draws both: everything that is somebody’s move, once
    assert.equal(shown(allRows), shown(youRows) + shown(agentRows))
    const ids = drawn(allRows).map((r) => r.id)
    assert.equal(new Set(ids).size, ids.length, 'no row is drawn twice')

    // each group header counts the group it heads
    for (const g of ['you', 'check', 'live']) assert.equal(allRows.t.n[g], String(allRows.groups[g].length), 'group ' + g)
    assert.equal(allRows.t.n.agent, String(allRows.groups.agent.length), 'group agent')

    // every thread is a row or a finished item, never both, never neither
    assert.equal(shown(allRows) + D.nothingNeeded.items.length, MIX.length)
    assert.equal(allRows.t.n.nothing, String(D.nothingNeeded.items.length))
  })
}

test('a ready Change is one approve row: not also a not-proven row, a waiting row or a finished item', () => {
  const D = M.data(view(MIX), READY)
  const all = M.vals(D, 'all')
  const kind = Object.fromEntries(drawn(all).map((r) => [r.id, r.kind]))
  assert.equal(kind.rd1, 'approve', 'finished work whose Change is ready is waiting on the person')
  assert.equal(kind.rd2, 'approve', 'a not-proven thread whose Change is ready is the approve row instead')
  assert.equal(M.text(D).groups.check.some((r) => r.id === 'rd1' || r.id === 'rd2'), false)
  assert.equal(D.nothingNeeded.items.some((i) => /rd1/.test(JSON.stringify(i))), false)
  // the agent is still working on rd3, and a question only the person can answer stays a question
  assert.equal(kind['live-rd3'], 'live')
  assert.equal(kind['ask-ask1'], 'prompt')
  assert.ok(!('rd3' in kind) && !('ask1' in kind), 'neither became an approve row')
})

const rowAddress = (thread_id, row_kind, hidden = true, sha = 'a') => ({
  thread_id, row_kind, row_sha256: sha.repeat(64), hidden,
  actor: 'Maya', recorded_at: M.at(16, 6),
})
const hideDoc = (...rows) => ({ project: 'p1', rows })

test('hidden check, prompt and ready rows are each counted once as dismissed, while the visible badge matches its rows', () => {
  const threads = [you('check'), asks('question'), th('ready'), agent('recording'), th('finished')]
  const hides = hideDoc(
    rowAddress('check', 'thread'), rowAddress('question', 'prompt'), rowAddress('ready', 'thread'),
    rowAddress('recording', 'thread', false), rowAddress('finished', 'thread', false),
  )
  const D = M.data(view(threads), { ready: readyFor('chg_ready') }, hides)
  const all = M.vals(D, 'all')
  const yours = M.vals(D, 'you')
  assert.equal(shown(yours), 0)
  assert.equal(D.inboxBadge, 0, 'the shared badge subtracts the hidden prompt and ready Change')
  assert.equal(M.text(D).n.yourMove, '0')
  assert.equal(M.text(D).n.agentMove, '1')
  assert.equal(D.prompts.length, 0, 'the hidden question leaves no active prompt')
  assert.equal(Object.keys(D.changes).length, 1, 'the hidden question also leaves no backing Change')
  assert.equal(D.nothingNeeded.items.filter((r) => r.kind === 'dismissed').length, 3)
  assert.equal(D.projects[0].dismissedCount, 3)
  assert.equal(M.text(D).nn.dismissed, '3 dismissed')
  assert.equal(shown(all) + D.nothingNeeded.items.length, threads.length, 'one visible or dismissed item per thread')
  assert.equal(D.threadCount, threads.length, 'the underlying record still has every thread')
})

test('a new recorded row digest makes an old hide appear again when the server reports it shown', () => {
  const threads = [asks('question')]
  const hidden = M.data(view(threads), {}, hideDoc(rowAddress('question', 'prompt')))
  const changed = M.data(view(threads), {}, hideDoc(rowAddress('question', 'prompt', false, 'b')))
  assert.equal(M.vals(hidden, 'all').groups.you.length, 0)
  assert.equal(M.vals(changed, 'all').groups.you.length, 1)
  assert.equal(changed.nothingNeeded.items.length, 0)
  assert.equal(changed.inboxBadge, 1)
})

test('h on a selected live row passes its served address; a row without one does nothing', () => {
  const threads = [you('check')]
  const address = rowAddress('check', 'thread', false)
  const D = M.data(view(threads), {}, hideDoc(address))
  const calls = []
  const v = M.vals(D, 'all', (row) => { calls.push(row) })
  assert.equal(v.keys.hide(), undefined)
  assert.deepEqual(calls, [address])
  assert.equal(M.vals(M.data(view(threads), {}), 'all', () => {}).keys.hide(), false)
})
