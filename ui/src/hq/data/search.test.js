/* ⌘K's record search reader (data/search.js): one GET address, every answer
 * said, never swallowed. A stubbed fetch; nothing reaches a server. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { SEARCH_ADDRESS, SEARCH_LIMIT, readSearch } from './search.js'

const stub = (status, body, calls = []) => async (url, init = {}) => {
  calls.push({ url, method: init.method || 'GET', body: init.body })
  return { ok: status < 400, status, text: async () => (typeof body === 'string' ? body : JSON.stringify(body)) }
}
const DOC = { schema_version: 1, project_id: 'p1', thread: null, limit: 20, total: 1, by_kind: {}, results: [{ kind: 'Read', thread_id: '0123456789abcdef', thread_title: 'Fix it', open_thread: false, text: 'src/a.py', at: null, event_id: 'e1', count: 1, source: 'claude_code' }], searched: {}, masking: { masked: 0, families: [], complete: true } }

test('the one address is a GET, with the typed text, the limit and the open Change encoded', () => {
  assert.equal(SEARCH_ADDRESS('p1', 'was a migration run?'), '/api/hq/projects/p1/search?q=was%20a%20migration%20run%3F&limit=' + SEARCH_LIMIT)
  assert.equal(SEARCH_ADDRESS('a/b', 'x&y', { thread: '0123456789abcdef', limit: 5 }), '/api/hq/projects/a%2Fb/search?q=x%26y&limit=5&thread=0123456789abcdef')
})

test('a served document is passed through for the text it answers', async () => {
  const calls = []
  const r = await readSearch('p1', 'src/a', { thread: 'th1' }, stub(200, DOC, calls))
  assert.deepEqual(r, { status: 'served', q: 'src/a', doc: DOC })
  assert.equal(calls.length, 1)
  assert.equal(calls[0].method, 'GET')
  assert.equal(calls[0].body, undefined)
  assert.match(calls[0].url, /&thread=th1$/)
})

test('nothing typed, or no project, asks nothing', async () => {
  const calls = []
  assert.deepEqual(await readSearch('p1', '   ', {}, stub(200, DOC, calls)), { status: 'idle' })
  assert.deepEqual(await readSearch(null, 'x', {}, stub(200, DOC, calls)), { status: 'idle' })
  assert.equal(calls.length, 0)
})

test('every failure is a sentence a person can read, never a code, never an empty result', async () => {
  const cases = [
    [stub(500, { error: 'read_failed' }), /could not search the rest of the record/],
    [stub(503, { error: 'store_unavailable' }), /thruline upgrade/],
    [stub(404, { error: 'no_such_route' }), /does not search the rest of the record yet/],
    [stub(200, { schema_version: 2, results: [] }), /could not search/],
    [stub(200, 'not json'), /could not search/],
    [stub(200, { ...DOC, results: [{ kind: 'Guess', thread_id: 'x' }] }), /could not search/],
    [async () => { throw new TypeError('fetch failed') }, /did not answer/],
  ]
  for (const [f, said] of cases) {
    const r = await readSearch('p1', 'x', {}, f)
    assert.equal(r.status, 'failed')
    assert.equal(r.q, 'x')
    assert.match(r.message, said)
    assert.doesNotMatch(r.message, /\b(500|503|404|read_failed|no_such_route)\b/)
  }
  const long = await readSearch('p1', 'a'.repeat(201), {}, stub(200, DOC))
  assert.equal(long.status, 'failed')
})
