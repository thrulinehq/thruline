import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

// Exercise the real workspace load methods with controlled HTTP responses.
// Bundling handles JSX and CSS without requiring a browser or a live ledger.
const temporary = await mkdtemp(join(tmpdir(), 'thruline-loading-'))
after(() => rm(temporary, { recursive: true, force: true }))
const bundle = await build({
  entryPoints: [fileURLToPath(new URL('./shell.jsx', import.meta.url))],
  bundle: true,
  write: false,
  format: 'esm',
  platform: 'node',
  loader: { '.css': 'empty' },
  define: { 'import.meta.env': '{}' },
  logLevel: 'silent',
})
const compiled = join(temporary, 'workspace.mjs')
await writeFile(compiled, bundle.outputFiles[0].text)
const { Workspace } = await import(pathToFileURL(compiled).href)

const id = 'session-two'
const repo = '/work/selected-repository'
const listing = {
  shape: 'thruline.sessions', schema_version: 1, total: 2,
  sessions: [
    { id: 'session-one', repo_root: '/work/other-repository', event_count: 1 },
    { id, repo_root: repo, event_count: 1 },
  ],
}
const explain = {
  shape: 'thruline.explain', schema_version: 1, session: id,
  what_happened: [{ seq: 1, kind: 'user_prompt', text: 'Check this repository.' }],
  claims_vs_proof: [], promises: [], risk_flags: [],
}
const claims = { shape: 'thruline.claims', schema_version: 1, session: id, claims: [] }

function response(doc, status = 200) {
  return {
    ok: status === 200, status,
    headers: { get: () => null },
    text: async () => JSON.stringify(doc),
  }
}

function deferred() {
  let resolve
  const promise = new Promise((done) => { resolve = done })
  return { promise, resolve }
}

function workspace() {
  const w = Object.create(Workspace.prototype)
  w.live = true
  w.epoch = 0
  w.switchEpoch = 0
  w.deepSession = id
  w.props = { index: { projects: [{ name: 'unrelated-old-export', fixture: 'wrong' }] } }
  w.state = {
    isDemo: false, session: '',
    sessions: { rows: [{ id, repo }] },
    corpus: { order: [], sessions: {} },
  }
  w.logs = []
  w.log = (line) => w.logs.push(line)
  w.toast = () => {}
  w.writeSession = () => {}
  w.switchSession = (key) => { w.switched = key }
  w.setState = (update, then) => {
    w.state = { ...w.state, ...(typeof update === 'function' ? update(w.state) : update) }
    if (then) then()
  }
  w.adopt = (corpus) => { w.adopted = corpus; w.state.corpus = corpus }
  return w
}

const tick = () => new Promise((resolve) => setImmediate(resolve))

test('local Agreed checks opens real HQ verification and restores its route without a retired API read', () => {
  const w = workspace()
  w.props.localVerification = true
  w.state.panels = { right: false, left: true }
  w.state.acceptance = { status: 'idle' }
  w.loadAcceptance = () => assert.fail('Local verification must not read legacy acceptance endpoint')
  w.writeRoute = route => { w.route = route }
  w.railGo('agreed')
  assert.equal(w.state.evidenceTab, 'verify')
  assert.equal(w.state.panels.right, true)
  assert.equal(w.route, 'agreed')
  w.railGo('agreed')
  assert.equal(w.state.panels.right, false)
  w.openDestination('agreed')
  assert.equal(w.state.panels.right, true)
  assert.equal(w.state.drawer, 'agreed')
})

test('initial load requests claims with explain and uses the selected live repository', async (t) => {
  const pendingExplain = deferred()
  const pendingClaims = deferred()
  const calls = []
  t.mock.method(globalThis, 'fetch', async (url, init) => {
    calls.push({ url, init })
    if (url === '/api/sessions') return response(listing)
    if (url.startsWith('/api/explain?')) return pendingExplain.promise
    if (url.startsWith('/api/claims?')) return pendingClaims.promise
    throw new Error('Unexpected read: ' + url)
  })
  const w = workspace()
  const load = w.loadData()
  await tick()
  assert.deepEqual(calls.map((call) => call.url), [
    '/api/sessions', '/api/explain?session=' + id, '/api/claims?session=' + id,
  ])
  assert.ok(calls.every((call) => call.init.cache === 'no-store'))
  pendingExplain.resolve(response(explain))
  pendingClaims.resolve(response(claims))
  await load
  assert.equal(w.adopted.sessions[id].project, repo)
  assert.equal(w.adopted.sessions[id].events.length, 1)
  assert.ok(!calls.some((call) => call.url.startsWith('/fixtures/')))
})

test('a failed claims request still opens the successful explain reading', async (t) => {
  t.mock.method(globalThis, 'fetch', async (url) => {
    if (url === '/api/sessions') return response(listing)
    if (url.startsWith('/api/explain?')) return response(explain)
    return response({ error: 'projection_failed', command: 'claims', exit_code: 1 }, 502)
  })
  const w = workspace()
  await w.loadData()
  assert.equal(w.adopted.sessions[id].events.length, 1)
  assert.ok(w.logs.some((line) => line.includes('claims projection could not be read')))
})

test('switching sessions requests both projections together and keeps repository identity', async (t) => {
  const pendingExplain = deferred()
  const pendingClaims = deferred()
  const calls = []
  t.mock.method(globalThis, 'fetch', async (url) => {
    calls.push(url)
    return url.startsWith('/api/explain?') ? pendingExplain.promise : pendingClaims.promise
  })
  const w = workspace()
  const opening = w.openListedSession(id)
  await tick()
  assert.deepEqual(calls, ['/api/explain?session=' + id, '/api/claims?session=' + id])
  pendingExplain.resolve(response(explain))
  pendingClaims.resolve(response(claims))
  await opening
  assert.equal(w.switched, id)
  assert.equal(w.state.corpus.sessions[id].project, repo)
})

test('an abandoned session load cannot adopt or report a late claims failure', async (t) => {
  const pendingClaims = deferred()
  t.mock.method(globalThis, 'fetch', async (url) => {
    if (url.startsWith('/api/explain?')) return response(explain)
    return pendingClaims.promise
  })
  const w = workspace()
  const opening = w.openListedSession(id)
  await tick()
  w.switchEpoch += 1
  pendingClaims.resolve(response({ error: 'projection_failed', command: 'claims', exit_code: 1 }, 502))
  await opening
  assert.equal(w.switched, undefined)
  assert.deepEqual(w.state.corpus.order, [])
  assert.ok(!w.logs.some((line) => line.includes('claims projection could not be read')))
})
