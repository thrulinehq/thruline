import test from 'node:test'
import assert from 'node:assert/strict'
import { localVerificationSession, coverageCounts, readLocalVerification } from './local-verification-model.js'
test('catalog availability is independent from runtime outcomes', () => {
  assert.deepEqual(coverageCounts([{ availability: 'implemented', validation_status: 'validated', status: 'not_run' }, { availability: 'specification_only', validation_status: 'not_implemented', status: 'pass' }]), { total: 2, implemented: 1, validated: 1 })
})
test('accepted requirement does not invent execution or a completion claim', () => {
  const s = localVerificationSession({ location: { repo: '/sample' }, contract: { objective: 'Return the discounted total' }, history: [] })
  assert.equal(s.events.length, 1)
  assert.equal(s.events[0].kind, 'user_prompt')
  assert.equal(s.claims.length, 0)
  assert.equal(Object.keys(s.localReceiptByNode).length, 0)
})
test('a fresh project has no invented prompt, claim, or graph evidence', () => {
  const s = localVerificationSession({ location: { repo: '/fresh' }, history: [], pack_results: [] })
  assert.equal(s.events.length, 0)
  assert.equal(s.claims.length, 0)
  assert.equal(s.nodes.length, 0)
  assert.match(s.capture.recorderLine, /No captured session evidence/)
  assert.doesNotMatch(s.capture.recorderLine, /recorder was working/)
  assert.notEqual(s.capture.condition, 'intact')
  assert.deepEqual(s.capture.sources, [])
})
test('trace preserves actual mismatch and links stale observations to original receipts', () => {
  const s = localVerificationSession({ location: { repo: '/sample' }, contract: { objective: 'Return 75' }, pack_results: [{ run_id: 'acceptance-evt_1', status: 'stale', explanation: 'The code changed.' }], history: [{ verification: { explanation: 'Mismatch' }, record: { run_id: 'acceptance-evt_1', check: { id: 'discount', runner: 'thruline-json-cases-v1', argv: ['python3', 'checkout.py'] }, start: { inputs: { tree: 'old' } }, historical_state: 'fail', finish: { pack: { observations: [{ description: 'Discount affects total', reason: 'Wrong value', expected: 75, observed: 100, exit: 0, outcome: 'failed' }] } } } }] })
  assert.ok(s.events.some(e => e.text.includes('Expected: 75\nObserved: 100')))
  assert.equal(s.nodes.find(n => n.title === 'Discount affects total').fam, 'amber')
  assert.ok(Object.values(s.localReceiptByNode).includes('acceptance-evt_1'))
  assert.equal(s.claims.length, 0)
  assert.match(s.capture.recorderLine, /Local check receipts do not establish recorder health/)
  assert.notEqual(s.capture.condition, 'intact')
})
test('concurrent readers share a bounded refresh and receive fresh stale evidence after contention', async t => {
  let calls = 0
  let deadline
  const fresh = { pack_results: [{ id: 'discount', status: 'stale' }] }
  t.mock.method(globalThis, 'fetch', async (_url, options) => {
    if (deadline) assert.equal(options.signal, deadline, 'retries must not reset the time budget')
    deadline = options.signal
    return ++calls === 1
      ? { status: 503, ok: false, json: async () => ({ error: 'verification_read_busy' }) }
      : { status: 200, ok: true, json: async () => fresh }
  })
  const graph = readLocalVerification(), hq = readLocalVerification()
  assert.equal(graph, hq)
  assert.deepEqual(await graph, fresh)
  assert.deepEqual(await hq, fresh)
  assert.equal(calls, 2)
})
test('missing evidence errors are never retried as snapshot contention', async t => {
  let calls = 0
  t.mock.method(globalThis, 'fetch', async () => {
    calls++
    return { status: 503, ok: false, json: async () => ({ error: 'verification_read_incomplete', detail: 'Evidence store unavailable' }) }
  })
  await assert.rejects(readLocalVerification(), /Evidence store unavailable/)
  assert.equal(calls, 1)
})
test('completion accounting is an evidence evaluation, not an invented command', () => {
  const s = localVerificationSession({ location: { repo: '/sample' }, history: [{ verification: { explanation: 'No suitable behavior check' }, record: { run_id: 'audit', check: { id: 'completion', runner: 'thruline-completion-v2', argv: ['thruline-completion-v2'] }, start: { inputs: { tree: 'input' } }, historical_state: 'unknown', finish: { pack: { observations: [{ id: 'audit', description: 'Account for accepted criteria', reason: 'No suitable behavior check', observed: { protocol: 'thruline-completion/2', criteria: [] }, exit: null, outcome: 'unparsed' }] } } } }] })
  assert.equal(s.events.filter(e => e.kind === 'exec').length, 0)
  assert.ok(s.events.some(e => e.text.includes('no process outcome claimed')))
  assert.ok(!s.events.some(e => e.text.includes('Expected: undefined')))
})
test('an advisory test edit does not invent a product failure or hide the captured pass', () => {
  const s = localVerificationSession({ location: { repo: '/sample' }, contract: { criteria: [{ id: 'test', description: 'Required suite executes', check_ids: ['suite'] }] }, pack_results: [{ id: 'suite', status: 'pass', scope: ['tests/test.py'], explanation: 'Accepted suite executed', review_notes: ['Test file changed; review its assertions. This does not establish weakened tests.'], catalog_results: [] }], history: [] })
  assert.equal(s.claims[0].findings.length, 0)
  assert.equal(s.localData.pack_results[0].review_notes.length, 1)
  assert.equal(s.localData.pack_results[0].status, 'pass')
})
