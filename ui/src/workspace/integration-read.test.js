import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readIntegration } from './integration-read.js'
const response = (status, body) => ({ status, ok: status === 200, json: async () => body })
test('a competing projection is retried without resubmitting the check', async () => {
  const calls = [], waits = []
  const result = await readIntegration('/api/integrations?session=test', {
    fetcher: async (url, options) => { calls.push({ url, options }); return calls.length < 3 ? response(503, { error: 'a_reading_is_in_progress' }) : response(200, { runs: ['saved-receipt'] }) },
    wait: async ms => { waits.push(ms) },
  })
  assert.deepEqual(result.runs, ['saved-receipt'])
  assert.deepEqual(waits, [250, 500])
  assert.ok(calls.every(call => call.options.method === undefined && call.options.cache === 'no-store'))
})
test('unrelated errors are surfaced once; persistent lock contention stays bounded', async () => {
  let calls = 0
  await assert.rejects(readIntegration('/api/integrations', { fetcher: async () => { calls++; return response(503, { error: 'artifact_changed' }) } }), /artifact_changed/)
  assert.equal(calls, 1)
  calls = 0
  let waited = 0
  await assert.rejects(readIntegration('/api/integrations', { fetcher: async () => { calls++; return response(503, { error: 'a_reading_is_in_progress' }) }, wait: async ms => { waited += ms } }), /saved receipts are retained/)
  assert.ok(calls < 25)
  assert.ok(waited >= 60000 && waited < 90000)
})
test('changing sessions aborts the pending retry before another request', async () => {
  const controller = new AbortController()
  let calls = 0
  await assert.rejects(readIntegration('/api/integrations', {
    signal: controller.signal,
    fetcher: async () => { calls++; return response(503, { error: 'a_reading_is_in_progress' }) },
    wait: async () => controller.abort(new Error('session changed')),
  }), /session changed/)
  assert.equal(calls, 1)
})
