import test from 'node:test'
import assert from 'node:assert/strict'
import { API, PROJECT, readProject, readProjection } from './data.js'

/* The read layer, pinned.
 *
 * The whole of `src/workspace/data.js` shipped as a stub for a while:
 * `readProjection`, `readProject` and a data hook all returned
 * `{ status: 'idle' }` with a `TODO(port)` above them. Because `'idle'` is
 * neither `'ready'` nor `'refused'`, every caller fell straight through to the
 * bundled demo corpus and the app could never show a real reading — on a
 * machine holding 91 exported projects. Worse, `refresh()` reached its success
 * toast and told the reader "Reading refreshed" having issued no request at
 * all. These tests exist so that cannot come back quietly.
 *
 * They drive `fetch` through a stub rather than a server: the contract being
 * pinned is what this module ASKS FOR and what it does with each answer
 * (docs/UI-SERVER-CONTRACT.md §2.3), which is exactly the part a running
 * server cannot make obvious.
 */

/** Install a fetch stub; returns the calls it recorded. */
function stubFetch(reply) {
  const calls = []
  globalThis.fetch = async (url, init) => {
    calls.push({ url, init })
    return reply(url, init)
  }
  return calls
}

function response({ ok = true, status = 200, statusText = 'OK', body = '{}', headers = {} }) {
  return {
    ok,
    status,
    statusText,
    headers: { get: (k) => (Object.prototype.hasOwnProperty.call(headers, k) ? headers[k] : null) },
    text: async () => body,
  }
}

test('opening a project issues a no-store request for that project fixture', async () => {
  const calls = stubFetch(() => response({ body: JSON.stringify({ project: 'thruline' }) }))
  const read = await readProject('abc123')

  assert.equal(calls.length, 1, 'opening a project must actually read it')
  assert.equal(calls[0].url, PROJECT('abc123'))
  assert.equal(calls[0].init.cache, 'no-store', 'every read is no-store; no exception')
  assert.equal(read.status, 'ready')
  assert.deepEqual(read.doc, { project: 'thruline' })
  assert.equal(read.raw, JSON.stringify({ project: 'thruline' }), 'the raw text is kept for a source digest')
})

test('a diagnostics export is refused by name, never rendered', async () => {
  stubFetch(() => response({ body: JSON.stringify({ diagnostics: true }) }))
  const read = await readProject('diag')
  assert.equal(read.status, 'refused')
  assert.match(read.refusal, /diagnostics export/)
})

test('a missing project fixture is spoken, not swallowed', async () => {
  stubFetch(() => response({ ok: false, status: 404, statusText: 'Not Found', body: '' }))
  const read = await readProject('gone')
  assert.equal(read.status, 'refused')
  assert.match(read.refusal, /404/)
})

test('exit 3 is a policy answer served as 200 and is a SUCCESSFUL read', async () => {
  stubFetch(() =>
    response({
      body: JSON.stringify({ session: 's1' }),
      headers: { 'X-ThruLine-Exit': '3', 'X-ThruLine-Projection': 'explain --json' },
    }),
  )
  const read = await readProjection(API.explain)
  assert.equal(read.status, 'ready', 'exit 3 is an unsupported claim — what the reader came to see')
  assert.equal(read.exit, 3)
  assert.equal(read.projection, 'explain --json')
})

test('a 502 projection_failure names the command and the exit code it refuses on', async () => {
  stubFetch(() =>
    response({
      ok: false,
      status: 502,
      body: JSON.stringify({ error: 'projection_failed', command: 'thruline explain --json', exit_code: 1, stderr: 'no ledger' }),
    }),
  )
  const read = await readProjection(API.explain)
  assert.equal(read.status, 'refused')
  assert.match(read.refusal, /thruline explain --json/)
  assert.match(read.refusal, /exited 1/)
  assert.match(read.refusal, /no ledger/)
})

test('a read is never `idle` for a real answer — the status a caller can mistake for success', async () => {
  stubFetch(() => response({ body: '{}' }))
  for (const url of [API.sessions, API.explain, API.claims, API.state, API.acceptance]) {
    const read = await readProjection(url)
    assert.notEqual(read.status, 'idle', `${url} returned the status that used to reach the success toast`)
  }
})

test('the session listing is a GET, read no-store, and carries no query parameter of its own', async () => {
  // `crates/cli/src/uiserve.rs:661-668` answers 400 `unknown_query_parameter`
  // to ANY parameter on this route, and `:1381` does the same on `/api/explain`
  // for any name that is not `session`. So the app must never append its own
  // page params (`?project=`, `?node=`, `?cast=`) to an API address.
  const calls = stubFetch(() => response({ body: JSON.stringify({ schema_version: 1, shape: 'thruline.sessions', total: 0, sessions: [] }) }))
  const read = await readProjection(API.sessions)
  assert.equal(calls[0].url, '/api/sessions')
  assert.doesNotMatch(calls[0].url, /\?/)
  assert.equal(calls[0].init.method, 'GET')
  assert.equal(calls[0].init.cache, 'no-store')
  assert.equal(read.status, 'ready')
})

test('one session is addressed as exactly `?session=<id>` and nothing else', async () => {
  const calls = stubFetch(() => response({ body: '{}' }))
  await readProjection(API.explain + '?session=' + encodeURIComponent('5e5a0021-0000-4000-8000-000000000021'))
  assert.equal(calls[0].url, '/api/explain?session=5e5a0021-0000-4000-8000-000000000021')
})

test('a refused session listing is a named refusal, so the picker can say why it is empty', async () => {
  stubFetch(() =>
    response({
      ok: false,
      status: 502,
      body: JSON.stringify({ error: 'projection_failed', command: 'thruline sessions --json', exit_code: 1, stderr: 'no store' }),
    }),
  )
  const read = await readProjection(API.sessions)
  assert.equal(read.status, 'refused')
  assert.match(read.refusal, /thruline sessions --json/)
})

test('refresh is the one POST on this surface; every other projection is a GET', async () => {
  const calls = stubFetch(() => response({ body: JSON.stringify({ ok: true, started: true }) }))
  await readProjection(API.refresh)
  await readProjection(API.state)
  assert.equal(calls[0].init.method, 'POST', 'docs/UI-SERVER-CONTRACT.md:183')
  assert.equal(calls[1].init.method, 'GET')
})

test('a refused refresh is refused, so nothing downstream can call it a re-read', async () => {
  stubFetch(() => response({ ok: false, status: 409, body: JSON.stringify({ ok: false, reason: 'already_refreshing' }) }))
  const read = await readProjection(API.refresh)
  assert.equal(read.status, 'refused')
  assert.match(read.refusal, /already_refreshing/)
})

test('a server that does not answer at all becomes a named refusal, never a throw', async () => {
  globalThis.fetch = async () => {
    throw new Error('ECONNREFUSED')
  }
  const read = await readProjection(API.explain)
  assert.equal(read.status, 'refused')
  assert.match(read.refusal, /thruline ui/)
})
