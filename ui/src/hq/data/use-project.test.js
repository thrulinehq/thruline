/* W2-A: the one write First run makes — POST /api/hq/projects/<id>/load —
 * proved against a stubbed fetch. Nothing here reaches a server. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { HQ_LOAD_ADDRESS, HQ_LOAD_STATUS, startLoad } from './use-project.js'

const stub = (status, body, calls) => async (url, init) => {
  calls.push({ url, init })
  return { status, json: async () => body }
}

test('the write address is named, and is the only one', () => {
  assert.equal(HQ_LOAD_ADDRESS('dfbfd6089e4bb388'), '/api/hq/projects/dfbfd6089e4bb388/load')
  assert.equal(HQ_LOAD_ADDRESS('a/b'), '/api/hq/projects/a%2Fb/load')
  assert.equal(HQ_LOAD_STATUS, '/api/hq/load')
})

test('Load every session POSTs once, with no body, to the load address', async () => {
  const calls = []
  const status = { running: true, project: 'p1', files_total: 540, files_done: 0 }
  const got = await startLoad('p1', stub(202, status, calls))
  assert.deepEqual(got, status)
  assert.equal(calls.length, 1)
  assert.equal(calls[0].url, '/api/hq/projects/p1/load')
  assert.equal(calls[0].init.method, 'POST')
  assert.equal(calls[0].init.body, undefined)
  assert.equal(calls[0].init.credentials, 'same-origin')
})

test('a load already running is not an error: its status comes back', async () => {
  const status = { running: true, project: 'p1' }
  assert.deepEqual(await startLoad('p1', stub(409, status, [])), status)
})

test('a refusal says what to do, in plain words, never a code', async () => {
  await assert.rejects(startLoad('p1', stub(507, { error: 'not_enough_disk', need_bytes: 1, free_bytes: 0 }, [])), (e) => /not enough free disk space/.test(e.message) && !/507|not_enough_disk/.test(e.message))
  await assert.rejects(startLoad('p1', stub(503, { error: 'store_unavailable' }, [])), /thruline upgrade/)
  await assert.rejects(startLoad('p1', stub(500, { error: 'read_failed' }, [])), (e) => !/read_failed|500/.test(e.message))
  await assert.rejects(startLoad('p1', async () => { throw new TypeError('fetch failed') }), /did not answer/)
  await assert.rejects(startLoad('', stub(202, {}, [])), /No folder/)
})
