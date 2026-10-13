/* W2-B: the Change loader resolves an address to a thread and reads only.
 * Every read is stubbed; the shared API server is never touched and nothing
 * is ever POSTed. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: "export * from './use-change.js'", resolveDir: here, loader: 'js' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'use-change-')), 'm.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const tid = (eventId) => createHash('sha256').update(eventId).digest('hex').slice(0, 16)
const THREAD = tid('ev-first')

const view = {
  project: { id: 'p1', name: 'proj', root: '/r' },
  thread: { id: THREAD, title: 'Fix it', current_version_id: 'wt-1111111111111111', proof: { edited_after_last_pass: [] } },
  prompts: [{ event_id: 'ev-first', session_id: 's1', ts: '2026-10-06T12:00:00Z', text: 'fix it', work: true, version_id: 'wt-1111111111111111' }],
  edits: [], commands: [{ id: 'c#0', ts: '2026-10-06T12:01:00Z', command: 'pytest', outcome: 'succeeded', is_test: true, tally: { passed: 2, failed: 0 }, version_id: 'wt-1111111111111111', session_id: 's1' }],
  claims: [{ id: 'k#0', event_id: 'k', ts: '2026-10-06T12:02:00Z', text: 'Tests pass.', kind: 'tests_pass', has_verifier: true, rests_on_command_id: 'c#0', version_id: 'wt-1111111111111111', session_id: 's1' }],
  versions: [{ id: 'wt-1111111111111111', committed: false }], current_version_id: 'wt-1111111111111111',
  sessions: [{ id: 's1', ledger_project_id: 'lp', source: 'codex', origin: 'person', started_at: '2026-10-06T12:00:00Z' }], files: [],
}

/** A GET-only stub over a map of address → document; every address asked is logged. */
function stub(routes) {
  const asked = []
  const get = async (url) => {
    asked.push(url)
    if (!(url in routes)) throw new Error(url + ' answered 404: {"error":"no_such_route"}')
    return routes[url]
  }
  return { get, asked }
}

test('a thread id with its project reads that thread, its project, the Change index and the store size — nothing else', async () => {
  const s = stub({ ['/api/hq/projects/p1/threads/' + THREAD]: view, '/api/hq/projects/p1': { project: { members: [{ root: '/r' }, { root: '/wt/other' }] } }, '/api/changes': { changes: [] }, '/api/storage': { bytes: 2_000_000_000 } })
  const out = await M.loadChange({ change: THREAD, project: 'p1' }, { get: s.get, readTracked: async () => null })
  assert.equal(out.snapshot.change.threadId, THREAD)
  assert.equal(out.snapshot.machine.recordsBytes, 2_000_000_000)
  assert.deepEqual([...s.asked].sort(), ['/api/changes', '/api/hq/projects/p1', '/api/hq/projects/p1/threads/' + THREAD, '/api/storage'])
})

test('a thread id without a project is found by searching the projects', async () => {
  const s = stub({ '/api/hq/projects': { projects: [{ id: 'p0' }, { id: 'p1' }] }, '/api/hq/projects/p0': { threads: [] }, '/api/hq/projects/p1': { threads: [{ id: THREAD }] } })
  assert.deepEqual(await M.resolveChange({ change: THREAD }, s.get), { project: 'p1', thread: THREAD, trackedId: null })
})

test('a tracked Change resolves to its thread through its linked session’s first prompt', async () => {
  const s = stub({
    '/api/changes/chg_1': { change: { id: 'chg_1', associations: [{ session_id: 's1', provenance: 'confirmed' }] } },
    '/api/hq/sessions/s1': { hq_project_id: 'p1', items: [{ kind: 'assistant', event_id: 'x' }, { kind: 'prompt', event_id: 'ev-first' }] },
    '/api/hq/projects/p1': { threads: [{ id: THREAD }] },
  })
  assert.deepEqual(await M.resolveChange({ change: 'chg_1' }, s.get), { project: 'p1', thread: THREAD, trackedId: 'chg_1' })
})

test('a rejected link does not resolve a tracked Change', async () => {
  const s = stub({ '/api/changes/chg_2': { change: { id: 'chg_2', associations: [{ session_id: 's1', provenance: 'rejected' }] } }, '/api/hq/sessions/s1': { hq_project_id: 'p1', items: [{ kind: 'prompt', event_id: 'ev-first' }] }, '/api/hq/projects/p1': { threads: [{ id: THREAD }] } })
  await assert.rejects(M.resolveChange({ change: 'chg_2' }, s.get), /not linked to any recorded work/)
})

test('the tracked Change whose linked sessions include this thread’s is read through the given reader', async () => {
  const s = stub({ '/api/changes': { changes: [{ id: 'chg_a' }, { id: 'chg_b' }] }, '/api/changes/chg_a': { change: { associations: [{ session_id: 'other' }] } }, '/api/changes/chg_b': { change: { associations: [{ session_id: 's1', provenance: 'confirmed' }] } } })
  const read = []
  const t = await M.findTracked(view, null, s.get, async (id) => { read.push(id); return { change: { id } } })
  assert.deepEqual(read, ['chg_b'])
  assert.equal(t.change.id, 'chg_b')
})

test('an address the record does not hold says so in plain words', async () => {
  const s = stub({})
  await assert.rejects(M.loadChange({ change: THREAD, project: 'p1' }, { get: s.get }), /not in the record on this machine/)
})

test('through the real reader every request is a GET', async () => {
  const methods = []
  const real = globalThis.fetch
  globalThis.fetch = async (url, init) => {
    methods.push((init && init.method) || 'GET')
    const body = String(url).includes('/threads/') ? view : String(url).endsWith('/api/storage') ? { bytes: 1 } : { changes: [] }
    return { ok: true, status: 200, text: async () => JSON.stringify(body) }
  }
  try {
    await M.loadChange({ change: THREAD, project: 'p1' })
  } finally { globalThis.fetch = real }
  assert.ok(methods.length >= 2)
  assert.ok(methods.every((m) => m === 'GET'), methods.join(','))
})

/* W2-D, C26 b — after "Track this work as a Change", the thread resolves to its new Change. */
test('only a CONFIRMED link makes a thread tracked; a suggestion never does', async () => {
  const s = stub({ '/api/changes': { changes: [{ id: 'chg_s', opened_at: '2026-10-07T10:00:00Z', associations: [{ session_id: 's1', provenance: 'suggested' }] }] } })
  const t = await M.findTracked(view, null, s.get, async (id) => ({ change: { id } }))
  assert.equal(t, null)
})

test('the newest Change confirming this work wins, read from the list’s own associations', async () => {
  const s = stub({ '/api/changes': { changes: [
    { id: 'chg_old', opened_at: '2026-10-06T10:00:00Z', associations: [{ session_id: 's1', provenance: 'confirmed' }] },
    { id: 'chg_new', opened_at: '2026-10-07T10:00:00Z', associations: [{ session_id: 's1', provenance: 'confirmed' }] },
  ] } })
  const read = []
  await M.findTracked(view, null, s.get, async (id) => { read.push(id); return { change: { id } } })
  assert.deepEqual(read, ['chg_new'])
  assert.ok(!s.asked.some((u) => u.startsWith('/api/changes/')), 'no per-Change read when the list carries the links')
})

test('the work on screen is served with the ledger project the first-use route takes', async () => {
  assert.deepEqual(M.workOf({ sessions: [{ id: 's1', ledger_project_id: 'lp', origin: 'person' }, { id: 's2', ledger_project_id: 'lp', origin: 'subagent' }] }), [{ id: 's1', ledgerProject: 'lp' }])
  const s = stub({ ['/api/hq/projects/p1/threads/' + THREAD]: view, '/api/changes': { changes: [] } })
  const out = await M.loadChange({ change: THREAD, project: 'p1' }, { get: s.get, readTracked: async () => null })
  assert.deepEqual(out.work, [{ id: 's1', ledgerProject: 'lp' }])
})
