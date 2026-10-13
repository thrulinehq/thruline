/* The Inbox's approve row: which tracked Changes are ready for a decision, and which thread of
 * work each one stands for. Every read here is a stub of a served document (docs/hq-next/API.md,
 * GET /api/changes, /api/changes/<id>/readiness, the session and project views); the join from a
 * Change to its thread is the Change screen's own (`resolveChange`), run for real over the stubs.
 * Synthetic fixtures only; nothing here reaches a server. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { JSDOM } from 'jsdom'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>')
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const React = (await import('react')).default
const { createRoot } = await import('react-dom/client')
const { readyOf, readReadyChanges, readyNote, READY_CAP } = await import('./inbox-ready.js')
const { sha256Hex } = await import('./use-change.js')
const { useInbox } = await import('./use-project.js')
const { VIEW, at } = await import('./project-fixture.js')

const clone = (x) => JSON.parse(JSON.stringify(x))
const TARGET = '9f2ad3e5c1b7a0d4e8f6'

/** A readiness document as `GET /api/changes/<id>/readiness` serves it. */
const readiness = (over = {}) => ({
  schema_version: 1,
  readiness: {
    ready: true, with_exceptions: false, scope_version: 1, target_version: TARGET, headline_slug: 'readiness_ready',
    obligations: [
      { id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, outcome: 'satisfied' },
      { id: 'criterion:1', kind: 'criterion', criterion_index: 1, outcome: 'satisfied' },
      { id: 'criterion:2', kind: 'criterion', criterion_index: 2, outcome: 'satisfied' },
    ],
    rests_on: [], missing: [], waived: [], not_applicable: [], ...over,
  },
})
const notReady = () => readiness({ ready: false, headline_slug: 'readiness_needs_evidence', missing: ['criterion:2'] })

/** The project (ledger members included) and the sessions whose prompts name its threads. */
const THREAD = async (event) => (await sha256Hex(event)).slice(0, 16)
async function world({ changes, ready = {}, runs = {}, extra = {}, failing = new Set() }) {
  const view = clone(VIEW)
  view.project.members = [{ ledger_project_id: 'lp1', source: 'claude_code', root: view.project.root, root_basis: 'recorded_cwd' }]
  // the thread each session's first prompt names: np1 and done1 and done2 get one apiece
  const tid = { s_np1: 'np1', s_done1: 'done1', s_done2: 'done2', s_re1: 're1', s_live1: 'live1' }
  const events = {}
  for (const [sid, id] of Object.entries(tid)) {
    const ev = 'ev-' + sid
    const real = await THREAD(ev)
    const t = view.threads.find((x) => x.id === id)
    t.id = real
    events[sid] = { ev, thread: real, name: id }
  }
  const asked = []
  const get = async (url) => {
    asked.push(url)
    if (failing.has(url)) throw new Error(`${url} answered 500`)
    if (url === '/api/changes') return { changes }
    let m
    if ((m = url.match(/^\/api\/changes\/([^/]+)\/readiness$/))) return ready[m[1]] || notReady()
    if ((m = url.match(/^\/api\/changes\/([^/]+)\/run$/))) return runs[m[1]] || { checks: [] }
    if ((m = url.match(/^\/api\/changes\/([^/]+)$/))) {
      const c = changes.find((x) => x.id === m[1])
      return { change: c }
    }
    if ((m = url.match(/^\/api\/hq\/sessions\/(.+)$/))) {
      const e = events[m[1]]
      return e ? { hq_project_id: view.project.id, items: [{ kind: 'prompt', event_id: e.ev }] } : { hq_project_id: 'other', items: [] }
    }
    if (url === `/api/hq/projects/${view.project.id}`) return view
    if (url === `/api/hq/projects/${view.project.id}/inbox-hides`) return { project: view.project.id, rows: [] }
    throw new Error(`${url} answered 404`)
  }
  return { view, get, asked, events, ...extra }
}
const change = (id, sid, over = {}) => ({
  id, project_id: 'lp1', opened_at: at(10, 0), request: { title: id },
  associations: sid ? [{ id: 'a_' + id, session_id: sid, provenance: 'confirmed', asserted_at: over.opened_at || at(10, 0) }] : [],
  decisions: { standing: null, history: [] }, ...over,
})

test('readyOf: the plain ready answer only, with the version and the lines it rests on', () => {
  const r = readyOf(readiness())
  assert.equal(r.version, '9f2ad3e')
  assert.deepEqual(r.criteria, { version: 'v1', lines: [{ n: 1, state: 'proven', gate: false }, { n: 2, state: 'proven', gate: false }] })
  // not ready: every other answer is silence, never an approve row
  assert.equal(readyOf(notReady()), null)
  assert.equal(readyOf(readiness({ headline_slug: 'readiness_nothing_accepted' })), null)
  assert.equal(readyOf(readiness({ ready: true, headline_slug: 'readiness_needs_evidence' })), null, 'the slug and the boolean must agree')
  assert.equal(readyOf(readiness({ with_exceptions: true, headline_slug: 'readiness_ready_with_exceptions' })), null, 'ready with exceptions is the Change screen’s to word')
  assert.equal(readyOf(readiness({ target_version: null })), null, 'a ready answer with no version is not one')
  for (const junk of [null, undefined, {}, { readiness: null }, { readiness: { ready: 'yes' } }, 'ready']) assert.equal(readyOf(junk), null)
  // no lines served: the row says nothing about them
  assert.equal(readyOf(readiness({ obligations: [] })).criteria, null)
})

test('a ready Change is placed on the thread of work its linked session names, by the Change screen’s own join', async () => {
  const w = await world({ changes: [change('chg_a', 's_done1'), change('chg_b', 's_np1')], ready: { chg_a: readiness() } })
  const r = await readReadyChanges(w.view, { get: w.get })
  assert.deepEqual(Object.keys(r.ready), [w.events.s_done1.thread])
  assert.equal(r.ready[w.events.s_done1.thread].change, 'chg_a')
  assert.equal(r.ready[w.events.s_done1.thread].version, '9f2ad3e')
  assert.equal(r.failed, 0)
  assert.equal(r.listFailed, false)
  assert.equal(readyNote(r), null)
  assert.equal(r.project, w.view.project.id)
})

test('a confirmed tracked Change is placed even when it is not ready, preserving accepted lines and approval', async () => {
  const w = await world({ changes: [change('chg_a', 's_np1', {
    accepted_scope: { version: 1, acceptance: { accepted_at: at(9, 0) }, criteria: ['The parser returns JSON.'] },
    decisions: { standing: { act: 'approved', target_revision: TARGET, actor: 'you', decided_at: at(10, 0) }, history: [] },
  })] })
  const r = await readReadyChanges(w.view, { get: w.get })
  assert.deepEqual(r.ready, {})
  assert.equal(r.tracked[w.events.s_np1.thread].id, 'chg_a')
  assert.equal(r.tracked[w.events.s_np1.thread].criteria.lines.length, 1)
  assert.equal(r.tracked[w.events.s_np1.thread].approved.actor, 'you')
})

test('an accepted tracked Change carries only the runnable command the server offers', async () => {
  const w = await world({ changes: [change('chg_a', 's_np1', {
    accepted_scope: { version: 1, acceptance: { accepted_at: at(9, 0) }, criteria: ['A checked line.'] },
  })], runs: { chg_a: { checks: [{ id: 'cargo-test', command: 'cargo test -q' }] } } })
  const r = await readReadyChanges(w.view, { get: w.get })
  assert.equal(r.tracked[w.events.s_np1.thread].checkCommand, 'cargo test -q')
})

test('a changed target at the same accepted scope refreshes the tracked command', async () => {
  const ready = { chg_a: readiness() }
  const runs = { chg_a: { checks: [{ id: 'cargo-test', command: 'cargo test -q' }] } }
  const w = await world({ changes: [change('chg_a', 's_np1', {
    accepted_scope: { version: 1, acceptance: { accepted_at: at(9, 0) }, criteria: ['A checked line.'] },
  })], ready, runs })
  const placed = new Map()
  const runCache = new Map()
  const opts = { get: w.get, placed, runCache }
  const thread = w.events.s_np1.thread

  const first = await readReadyChanges(w.view, opts)
  assert.equal(first.tracked[thread].checkCommand, 'cargo test -q')

  ready.chg_a = readiness({ target_version: 'a1b2c3d4e5f60718293a' })
  runs.chg_a = { checks: [{ id: 'npm-test', command: 'npm test' }] }
  const second = await readReadyChanges(w.view, opts)
  assert.equal(second.tracked[thread].checkCommand, 'npm test')
  assert.equal(w.asked.filter((url) => url === '/api/changes/chg_a/run').length, 2)
})

test('only the project’s own Changes are read; the 20 most recently active, newest first', async () => {
  const changes = []
  for (let i = 0; i < 25; i++) changes.push(change('chg_' + String(i).padStart(2, '0'), 's_done1', { opened_at: at(8, i) }))
  changes.push(change('chg_other', 's_done1', { project_id: 'lp-elsewhere', opened_at: at(23, 0) }))
  const w = await world({ changes })
  await readReadyChanges(w.view, { get: w.get })
  const read = w.asked.filter((u) => u.endsWith('/readiness')).map((u) => u.split('/')[3])
  assert.equal(READY_CAP, 20)
  assert.equal(read.length, 20, 'twenty, not twenty-five')
  assert.ok(!read.includes('chg_other'), 'another project’s Change is never read')
  assert.ok(read.includes('chg_24') && read.includes('chg_05') && !read.includes('chg_04'), 'the newest twenty')
})

test('a readiness read that fails gives that Change no approve row, is counted, and spoils nothing else', async () => {
  const w = await world({
    changes: [change('chg_a', 's_done1'), change('chg_b', 's_done2', { opened_at: at(9, 0) })],
    ready: { chg_a: readiness(), chg_b: readiness() },
    failing: new Set(['/api/changes/chg_a/readiness']),
  })
  const r = await readReadyChanges(w.view, { get: w.get })
  assert.deepEqual(Object.keys(r.ready), [w.events.s_done2.thread], 'chg_b is still ready; chg_a is not claimed to be')
  assert.equal(r.failed, 1)
  assert.equal(readyNote(r), 'could not check 1 Change for approval')
  assert.equal(readyNote({ ...r, failed: 3 }), 'could not check 3 Changes for approval')
})

test('a Change list that cannot be read, or a join that fails, files nothing as ready and says so', async () => {
  const w = await world({ changes: [change('chg_a', 's_done1')], ready: { chg_a: readiness() }, failing: new Set(['/api/changes']) })
  const r = await readReadyChanges(w.view, { get: w.get })
  assert.deepEqual(r.ready, {})
  assert.equal(r.listFailed, true)
  assert.equal(readyNote(r), 'could not check Changes for approval')
  const w2 = await world({ changes: [change('chg_a', 's_done1')], ready: { chg_a: readiness() }, failing: new Set(['/api/changes/chg_a']) })
  const r2 = await readReadyChanges(w2.view, { get: w2.get })
  assert.deepEqual(r2.ready, {}, 'ready, but which thread it stands for could not be read: no row')
  assert.equal(r2.failed, 1)
})

test('a Change already approved for this version is not waiting; one approved for an older version is', async () => {
  const standing = (v) => ({ decisions: { standing: { target_revision: v, act: 'approved' }, history: [] } })
  const w = await world({
    changes: [change('chg_a', 's_done1', standing(TARGET)), change('chg_b', 's_done2', standing('0000000old'))],
    ready: { chg_a: readiness(), chg_b: readiness() },
  })
  const r = await readReadyChanges(w.view, { get: w.get })
  assert.deepEqual(Object.keys(r.ready), [w.events.s_done2.thread])
})

test('an approval is current when it names the Change’s own target version (not the thread’s version id, which is another id space)', async () => {
  const standing = (v) => ({ decisions: { standing: { target_revision: v, act: 'approved', actor: 'you', decided_at: at(10, 0) }, history: [] } })
  const w = await world({
    changes: [change('chg_a', 's_done1', standing(TARGET)), change('chg_b', 's_done2', standing('0000000old'))],
    ready: { chg_a: readiness(), chg_b: readiness() },
  })
  const r = await readReadyChanges(w.view, { get: w.get })
  assert.equal(r.tracked[w.events.s_done1.thread].approved.version, TARGET)
  assert.equal(r.tracked[w.events.s_done1.thread].approved.current, true, 'approved for the version the Change is on')
  assert.equal(r.tracked[w.events.s_done2.thread].approved.current, false, 'approved for an older version: not current')
})

test('a Change linked to no session, or only a rejected one, is not placed and is not a failure', async () => {
  const rejected = change('chg_b', 's_done2')
  rejected.associations[0].provenance = 'rejected'
  const w = await world({ changes: [change('chg_a', null), rejected], ready: { chg_a: readiness(), chg_b: readiness() } })
  const r = await readReadyChanges(w.view, { get: w.get })
  assert.deepEqual(r.ready, {})
  assert.equal(r.failed, 0)
  assert.ok(!w.asked.some((u) => u.startsWith('/api/hq/sessions/')), 'no session was walked for them')
})

test('two ready Changes on one thread: the most recently active keeps it', async () => {
  const w = await world({
    changes: [change('chg_old', 's_done1', { opened_at: at(8, 0), associations: [{ id: 'a1', session_id: 's_done1', provenance: 'confirmed', asserted_at: at(8, 1) }] }), change('chg_new', 's_done1', { opened_at: at(11, 0) })],
    ready: { chg_old: readiness(), chg_new: readiness() },
  })
  const r = await readReadyChanges(w.view, { get: w.get })
  assert.equal(r.ready[w.events.s_done1.thread].change, 'chg_new')
})

test('where a Change stands is remembered while its links do not change: a re-read does not walk the sessions again', async () => {
  const w = await world({ changes: [change('chg_a', 's_done1')], ready: { chg_a: readiness() } })
  const placed = new Map()
  await readReadyChanges(w.view, { get: w.get, placed })
  const walks = () => w.asked.filter((u) => u.startsWith('/api/hq/sessions/')).length
  assert.equal(walks(), 1)
  await readReadyChanges(w.view, { get: w.get, placed })
  assert.equal(walks(), 1, 'placed once')
  assert.equal(w.asked.filter((u) => u.endsWith('/readiness')).length, 2, 'but readiness is read again every time')
})

test('a project the engine gave no ledger members has no Changes to read', async () => {
  const w = await world({ changes: [change('chg_a', 's_done1')], ready: { chg_a: readiness() } })
  delete w.view.project.members
  const r = await readReadyChanges(w.view, { get: w.get })
  assert.deepEqual(r.ready, {})
  assert.deepEqual(w.asked, [])
})

/* ---- through the hook the Inbox mounts ---- */
async function settle(cond = () => false, turns = 800) {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}
function mountInbox(w, { failing = new Set() } = {}) {
  globalThis.fetch = async (url) => {
    let body
    try {
      if (url === '/api/hq/projects') body = { schema_version: 1, complete: true, projects: [w.view.project] }
      else if (url === '/api/storage') body = { bytes: 1 }
      else if (url === '/api/hq/load') body = { running: false }
      else {
        if (failing.has(url)) throw new Error('500')
        body = await w.get(url)
      }
    } catch (e) {
      return { ok: false, status: 500, text: async () => '{}' }
    }
    return { ok: true, status: 200, text: async () => JSON.stringify(body) }
  }
  const seen = { out: null }
  function Probe() {
    seen.out = useInbox(w.view.project.id, { timers: { setInterval: () => 0, clearInterval() {} }, doc: null })
    return null
  }
  const root = createRoot(document.getElementById('root'))
  root.render(React.createElement(Probe))
  return { seen, unmount: () => root.unmount() }
}

test('the Inbox files a ready Change as its approve row, at the Approve sheet, with its real version', async () => {
  const w = await world({ changes: [change('chg_a', 's_done1')], ready: { chg_a: readiness() } })
  const m = mountInbox(w)
  try {
    const tid = w.events.s_done1.thread
    await settle(() => m.seen.out && m.seen.out.data && m.seen.out.data.changes[tid] && m.seen.out.data.changes[tid].state === 'ready')
    const d = m.seen.out.data
    const row = d.changes[tid]
    assert.equal(row.state, 'ready')
    assert.equal(row.move, 'you')
    assert.equal(row.version, '9f2ad3e', 'the version the readiness is about, not the thread’s')
    assert.equal(row.approveHref, `#/changes/chg_a/approve?project=${w.view.project.id}`)
    assert.equal(row.href, `#/changes/chg_a?project=${w.view.project.id}`)
    assert.equal(d.nothingNeeded.items.some((i) => i.title === `Thread done1`), false, 'it is not also a finished item')
    assert.equal(d.note, undefined)
    // finished work that is now the person’s move: the badge counts it, as the headline does
    assert.equal(d.inboxBadge, 4)
  } finally { m.unmount() }
})

test('a readiness read that fails leaves the Inbox up, with no approve row and a note beside the read time', async () => {
  const w = await world({ changes: [change('chg_a', 's_done1')], ready: { chg_a: readiness() } })
  const m = mountInbox(w, { failing: new Set(['/api/changes/chg_a/readiness']) })
  try {
    await settle(() => m.seen.out && m.seen.out.data && m.seen.out.data.note)
    const d = m.seen.out.data
    assert.equal(d.note, 'could not check 1 Change for approval')
    assert.equal(m.seen.out.error, null, 'the screen is not blanked for it')
    assert.ok(!Object.values(d.changes).some((c) => c.state === 'ready'), 'never a ready it did not read')
    // C46 §1.1: the badge is the one count every screen shows, and a look that failed is no count —
    // no badge, never one that may be short (the other screens have no note to say why)
    assert.equal(d.inboxBadge, null)
  } finally { m.unmount() }
})
