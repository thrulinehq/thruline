/* C27 (c): what ThruLine keeps on this machine is one shared reading, so
 * switching screens never waits for the walk of the records folder that
 * measures it (about a second on a large store). Every read is stubbed; no
 * server is touched and nothing is POSTed. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
/* api.js and use-change.js in ONE bundle, so the loader uses the same shared
 * reading the tests drive. */
const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: "export { loadChange } from './use-change.js'; export { readStorage, storageNow, forgetStorage, STORAGE_MAX_AGE_MS } from './api.js'", resolveDir: here, loader: 'js' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'storage-reading-')), 'm.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

/** A storage reader that answers `bytes` in order and counts its calls. */
function reader(...answers) {
  const r = (url) => {
    assert.equal(url, '/api/storage')
    r.calls += 1
    const a = answers[Math.min(r.calls - 1, answers.length - 1)]
    return a instanceof Error ? Promise.reject(a) : Promise.resolve({ bytes: a })
  }
  r.calls = 0
  return r
}

test('two readings inside a minute walk the folder once', async () => {
  M.forgetStorage()
  const get = reader(10, 20)
  assert.deepEqual(await M.readStorage(get, 1_000), { bytes: 10 })
  assert.deepEqual(await M.readStorage(get, 1_000 + M.STORAGE_MAX_AGE_MS - 1), { bytes: 10 })
  assert.equal(get.calls, 1)
  assert.deepEqual(await M.readStorage(get, 1_000 + M.STORAGE_MAX_AGE_MS), { bytes: 20 })
  assert.equal(get.calls, 2)
})

test('an old reading is stated at once while a fresh one is measured', async () => {
  M.forgetStorage()
  let release
  const slow = new Promise((r) => { release = r })
  const get = (url) => { get.calls += 1; return get.calls === 1 ? Promise.resolve({ bytes: 10 }) : slow }
  get.calls = 0
  assert.deepEqual(await M.storageNow(get, 0), { bytes: 10 })
  // a minute later: the walk has started again, and is still running
  assert.deepEqual(await M.storageNow(get, M.STORAGE_MAX_AGE_MS + 5), { bytes: 10 })
  assert.equal(get.calls, 2)
  release({ bytes: 30 })
  await slow
  await new Promise((r) => setImmediate(r))
  assert.deepEqual(await M.storageNow(get, M.STORAGE_MAX_AGE_MS + 6), { bytes: 30 })
  assert.equal(get.calls, 2)
})

test('a failed reading is not kept, and forgetting measures again', async () => {
  M.forgetStorage()
  const get = reader(new Error('down'), 40, 50)
  await assert.rejects(M.readStorage(get, 0))
  await new Promise((r) => setImmediate(r))
  assert.deepEqual(await M.readStorage(get, 1), { bytes: 40 })
  M.forgetStorage()
  assert.deepEqual(await M.readStorage(get, 2), { bytes: 50 })
  assert.equal(get.calls, 3)
})

test("one reader's answer is never another's", async () => {
  M.forgetStorage()
  const a = reader(1)
  const b = reader(2)
  assert.deepEqual(await M.storageNow(a, 0), { bytes: 1 })
  assert.deepEqual(await M.storageNow(b, 0), { bytes: 2 })
  assert.equal(a.calls + b.calls, 2)
})

const tid = (eventId) => createHash('sha256').update(eventId).digest('hex').slice(0, 16)
const THREAD = tid('ev-first')
const view = {
  project: { id: 'p1', name: 'proj', root: '/r' },
  thread: { id: THREAD, title: 'Fix it', current_version_id: 'wt-1111111111111111', proof: { edited_after_last_pass: [] } },
  prompts: [{ event_id: 'ev-first', session_id: 's1', ts: '2026-10-06T12:00:00Z', text: 'fix it', work: true, version_id: 'wt-1111111111111111' }],
  edits: [], commands: [], claims: [],
  versions: [{ id: 'wt-1111111111111111', committed: false }], current_version_id: 'wt-1111111111111111',
  sessions: [{ id: 's1', ledger_project_id: 'lp', source: 'codex', origin: 'person', started_at: '2026-10-06T12:00:00Z' }], files: [],
}

test('switching to a Change never waits for the storage walk once there is a reading', async () => {
  M.forgetStorage()
  let walking = false
  const never = new Promise(() => {})
  const get = (url) => {
    const u = String(url)
    if (u.includes('/threads/')) return Promise.resolve(view)
    if (u === '/api/hq/projects/p1') return Promise.resolve({ project: { members: [{ root: '/r' }] } })
    if (u === '/api/changes') return Promise.resolve({ changes: [] })
    if (u === '/api/storage') {
      if (walking) return never // the second walk never answers
      walking = true
      return Promise.resolve({ bytes: 7_000_000_000 })
    }
    throw new Error(`unexpected read ${u}`)
  }
  const at = { change: THREAD, project: 'p1' }
  const first = await M.loadChange(at, { get, now: Date.parse('2026-10-06T13:00:00Z') })
  assert.equal(first.snapshot.machine.recordsBytes, 7_000_000_000)
  // two minutes later the reading is old: a new walk starts and never ends,
  // and the screen still opens, stating the reading it has
  const later = Date.now() + 2 * M.STORAGE_MAX_AGE_MS
  const realNow = Date.now
  Date.now = () => later
  try {
    const second = await Promise.race([
      M.loadChange(at, { get, now: Date.parse('2026-10-06T13:02:00Z') }),
      new Promise((_, no) => setTimeout(() => no(new Error('the switch waited for the storage walk')), 500)),
    ])
    assert.equal(second.snapshot.machine.recordsBytes, 7_000_000_000)
  } finally {
    Date.now = realNow
  }
})
