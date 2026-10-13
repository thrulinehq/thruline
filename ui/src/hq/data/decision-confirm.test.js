/* C35: "Confirm on this Mac" — the client and the flow (data/decision-confirm.js).
 *
 * Proven against a stubbed `fetch`; nothing here reaches a server. What is held:
 *   - the order of requests: token, prepare, review, native-confirm, then the
 *     job, one request each, and native-confirm only after a review was read
 *     and only with that review's digest;
 *   - nothing the server refuses is ever sent (no presence, uid, nonce, result
 *     or actor field on review or native-confirm; prepare builds from a
 *     whitelist);
 *   - every refusal the server sends is shown with its text;
 *   - a running job is never presented as accepted, and only a job that names
 *     a receipt is a recorded act;
 *   - the module reaches exactly its addresses and no other file does.
 * Synthetic fixtures only. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import {
  DECISION_JOB, DECISION_NATIVE_CONFIRM, DECISION_PREPARE, DECISION_REVIEW, DECISION_TOKEN, TOKEN_HEADER,
  actBody, confirmClient, confirmFlow, jobOutcome, prepareBody, refusalOf, reviewFacts,
} from './decision-confirm.js'

const here = dirname(fileURLToPath(import.meta.url))
const SHA = 'a'.repeat(63) + 'b'
const REVIEW = {
  schema_version: 1,
  project_id: 'lp1',
  decision_id: 'dec-05',
  action: 'accept',
  act_row_id: 'act-9',
  decision: {
    revision: 1,
    revision_basis: 'act_revision',
    kind: 'timezone_policy',
    text: 'Every exported time is UTC, written with a Z',
    params: { format: 'utc_z', surfaces: ['docs'] },
    params_sha256: 'c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00',
  },
  confirmation: { thruline_decision_act: { schema_version: 1, action: 'accept', decision_id: 'dec-05', revision: 1, severity: 'warns' } },
  supersession: null,
  summary: 'Review the accept action and the displayed decision revision before confirming.',
}
const JOB = (over) => ({
  schema_version: 1, job_id: 'job-1', kind: 'native_confirmation', project_id: 'p1', decision_id: 'dec-05', act_row_id: 'act-9',
  state: 'running', receipt_id: null, errors: [], progress: { label: 'Review in the native window', unit: 'person_action', completed: 0, total: 1 },
  ...over,
})
const FIELDS = { actor: 'Robin', because: 'We agreed on it', severity: 'warns', revision: 1, applies_from_version: '3c3ca28a1b2c3d4e5f60718293a4b5c6d7e8f901' }

/** A scripted server: `route(method, url, n)` answers `{status, doc}` or throws. */
function server(route) {
  const calls = []
  const f = async (url, init = {}) => {
    const method = init.method || 'GET'
    calls.push({ method, url, headers: init.headers || {}, body: init.body ? JSON.parse(init.body) : undefined })
    const h = route(method, url, calls.length)
    if (h instanceof Error) throw h
    return { ok: h.status >= 200 && h.status < 300, status: h.status, json: async () => h.doc }
  }
  return { f, calls }
}
const answers = (over = {}) => (method, url) => {
  const key = method + ' ' + url.replace(/^\/api\/hq\//, '')
  if (over[key]) return typeof over[key] === 'function' ? over[key]() : over[key]
  if (key === 'GET decisions/confirmation-token') return { status: 200, doc: { schema_version: 1, token: 'tok-1' } }
  if (key === 'POST projects/p1/decisions/dec-05/prepare') return { status: 200, doc: { schema_version: 1, action: 'accept', act_row_id: 'act-9', state: 'pending_bridge', preparation_is_person_act: false } }
  if (key === 'POST projects/p1/decisions/dec-05/review') return { status: 200, doc: { schema_version: 1, review_sha256: SHA, review: REVIEW, preparation_is_person_act: false } }
  if (key === 'POST projects/p1/decisions/dec-05/native-confirm') return { status: 202, doc: JOB() }
  if (key.startsWith('GET decisions/check?job_id=')) return { status: 200, doc: JOB({ state: 'complete', receipt_id: 'rcpt-1', derived_written: true, progress: { label: 'Done', unit: 'person_action', completed: 1, total: 1 } }) }
  return { status: 404, doc: { schema_version: 1, error: 'no_such_route', detail: null } }
}
const noWait = async () => {}
const flowOn = (route, extra = {}) => {
  const s = server(route)
  const recorded = []
  const flow = confirmFlow({ client: confirmClient(s.f), project: 'p1', decision: 'dec-05', action: 'accept', wait: noWait, onRecorded: (j) => recorded.push(j), ...extra })
  return { ...s, flow, recorded }
}

test('the addresses are the contract’s, named once, with the project and decision encoded', () => {
  assert.equal(DECISION_TOKEN, '/api/hq/decisions/confirmation-token')
  assert.equal(DECISION_PREPARE('p 1', 'd/1'), '/api/hq/projects/p%201/decisions/d%2F1/prepare')
  assert.equal(DECISION_REVIEW('p1', 'dec-05'), '/api/hq/projects/p1/decisions/dec-05/review')
  assert.equal(DECISION_NATIVE_CONFIRM('p1', 'dec-05'), '/api/hq/projects/p1/decisions/dec-05/native-confirm')
  assert.equal(DECISION_JOB('01 J'), '/api/hq/decisions/check?job_id=01%20J')
  assert.equal(TOKEN_HEADER, 'X-ThruLine-Run-Token')
})

test('happy path: token, prepare, review, then native-confirm only after the review was read, then the job', async () => {
  const { flow, calls, recorded } = flowOn(answers())
  const seen = []
  flow.subscribe((s) => seen.push(s.step))
  await flow.prepare(FIELDS)
  // prepare reads the review back at once, and stops there: nothing asks for the window yet
  assert.deepEqual(calls.map((c) => c.method + ' ' + c.url), [
    'GET /api/hq/decisions/confirmation-token',
    'POST /api/hq/projects/p1/decisions/dec-05/prepare',
    'POST /api/hq/projects/p1/decisions/dec-05/review',
  ])
  assert.equal(flow.state.step, 'review')
  assert.equal(flow.state.review.sha, SHA)
  assert.deepEqual(flow.state.review.doc, REVIEW)
  await flow.confirm()
  assert.deepEqual(calls.map((c) => c.method + ' ' + c.url).slice(3), [
    'POST /api/hq/projects/p1/decisions/dec-05/native-confirm',
    'GET /api/hq/decisions/check?job_id=job-1',
  ])
  assert.equal(flow.state.step, 'ended')
  assert.equal(flow.state.outcome.kind, 'recorded')
  assert.equal(flow.state.outcome.receiptId, 'rcpt-1')
  assert.equal(recorded.length, 1)
  assert.ok(seen.includes('prepared') && seen.includes('reviewing') && seen.includes('review') && seen.includes('starting') && seen.includes('following') && seen.at(-1) === 'ended')
})

test('the three POSTs carry the Decisions token in the one header; the two GETs carry none; the token is read once', async () => {
  const { flow, calls } = flowOn(answers())
  await flow.prepare(FIELDS)
  await flow.confirm()
  const posts = calls.filter((c) => c.method === 'POST')
  assert.equal(posts.length, 3)
  for (const p of posts) {
    assert.equal(p.headers[TOKEN_HEADER], 'tok-1', p.url)
    assert.equal(p.headers['Content-Type'], 'application/json')
  }
  for (const g of calls.filter((c) => c.method === 'GET')) assert.equal(g.headers[TOKEN_HEADER], undefined, g.url)
  assert.equal(calls.filter((c) => c.url.endsWith('confirmation-token')).length, 1)
  assert.equal(calls.filter((c) => c.url.endsWith('confirmation-token'))[0].body, undefined)
})

test('the bodies are closed: exactly the contract’s fields, and never presence, uid, nonce, result or actor on review or native-confirm', async () => {
  const { flow, calls } = flowOn(answers())
  await flow.prepare({ ...FIELDS, uid: 501, nonce: 'n', presence: true, result: 'confirmed', act_row_id: 'x', event_id: 'ev', expected_review_sha256: SHA, version_id: 'zzz' })
  await flow.confirm()
  const [prepare, review, nativeConfirm] = calls.filter((c) => c.method === 'POST').map((c) => c.body)
  assert.deepEqual(prepare, { schema_version: 1, action: 'accept', actor: 'Robin', because: 'We agreed on it', revision: 1, severity: 'warns', applies_from_version: FIELDS.applies_from_version })
  assert.deepEqual(review, { schema_version: 1, action: 'accept', act_row_id: 'act-9' })
  assert.deepEqual(nativeConfirm, { schema_version: 1, action: 'accept', act_row_id: 'act-9', expected_review_sha256: SHA })
  const FORBIDDEN = ['presence', 'uid', 'nonce', 'result', 'actor', 'because', 'event_id', 'token']
  for (const body of [review, nativeConfirm]) for (const k of FORBIDDEN) assert.ok(!(k in body), k)
  for (const k of ['presence', 'uid', 'nonce', 'result', 'event_id', 'act_row_id', 'expected_review_sha256']) assert.ok(!(k in prepare), k)
})

test('each action sends only its own fields', () => {
  const retire = prepareBody('retire', { ...FIELDS, version_id: 'v', superseded_by: 'd2' })
  assert.deepEqual(retire, { ok: true, body: { schema_version: 1, action: 'retire', actor: 'Robin', because: 'We agreed on it' } })
  const waive = prepareBody('waive', { ...FIELDS, version_id: '3c3ca28a1b2c3d4e5f60718293a4b5c6d7e8f901' })
  assert.deepEqual(waive, { ok: true, body: { schema_version: 1, action: 'waive', actor: 'Robin', because: 'We agreed on it', version_id: '3c3ca28a1b2c3d4e5f60718293a4b5c6d7e8f901' } })
  const accept = prepareBody('accept', FIELDS)
  assert.deepEqual(Object.keys(accept.body).sort(), ['action', 'actor', 'applies_from_version', 'because', 'revision', 'schema_version', 'severity'])
})

test('nothing is sent without a name, a reason of your own, or the action’s own fields; there is no default', async () => {
  for (const [action, input, code] of [
    ['accept', { ...FIELDS, actor: '  ' }, 'name_required'],
    ['accept', { ...FIELDS, because: '' }, 'because_required'],
    ['accept', { ...FIELDS, because: undefined }, 'because_required'],
    ['accept', { ...FIELDS, severity: '' }, 'severity_required'],
    ['accept', { ...FIELDS, severity: 'blocking' }, 'severity_required'],
    ['accept', { ...FIELDS, revision: 0 }, 'revision_required'],
    ['accept', { ...FIELDS, applies_from_version: ' ' }, 'version_required'],
    ['waive', { ...FIELDS }, 'version_required'],
    ['sign', FIELDS, 'unknown_action'],
  ]) {
    assert.deepEqual(prepareBody(action, input), { ok: false, code }, action + ' ' + code)
  }
  const { flow, calls } = flowOn(answers())
  await flow.prepare({ ...FIELDS, because: '' })
  assert.deepEqual(calls, [], 'not even the token is asked for')
  assert.equal(flow.state.step, 'form')
  assert.match(flow.state.refusal.text, /Say why first\. There is no default reason\. Nothing was prepared\.$/)
})

test('the window is not asked for before a review was read, nor with a digest that is not a digest', async () => {
  const { flow, calls } = flowOn(answers())
  await flow.confirm()
  assert.deepEqual(calls, [], 'no prepared line, no review: nothing is sent')
  assert.deepEqual(actBody('accept', 'act-9', 'ABC'), { ok: false, code: 'digest_unreadable' })
  assert.deepEqual(actBody('accept', 'act-9', 'A'.repeat(64)), { ok: false, code: 'digest_unreadable' }, 'lower case, as the server requires')
  assert.deepEqual(actBody('accept', '', SHA), { ok: false, code: 'act_required' })
  assert.deepEqual(actBody('accept', 'act-9', SHA), { ok: true, body: { schema_version: 1, action: 'accept', act_row_id: 'act-9', expected_review_sha256: SHA } })
  // a review that came back without a digest is not a review the person can confirm
  const bad = flowOn(answers({ 'POST projects/p1/decisions/dec-05/review': { status: 200, doc: { schema_version: 1, review: REVIEW } } }))
  await bad.flow.prepare(FIELDS)
  assert.equal(bad.flow.state.step, 'prepared')
  assert.equal(bad.flow.state.review, null)
  assert.match(bad.flow.state.refusal.text, /cannot read/)
  await bad.flow.confirm()
  assert.equal(bad.calls.filter((c) => c.url.endsWith('native-confirm')).length, 0)
})

test('the digest sent is the digest of the review that was shown, and a changed review is read again before it can be confirmed', async () => {
  const moved = flowOn(answers({ 'POST projects/p1/decisions/dec-05/native-confirm': { status: 409, doc: { schema_version: 1, error: 'review_moved', detail: null } } }))
  await moved.flow.prepare(FIELDS)
  await moved.flow.confirm()
  assert.equal(moved.calls.at(-1).body.expected_review_sha256, SHA)
  assert.equal(moved.flow.state.step, 'prepared', 'the review on screen is no longer the one the server holds')
  assert.equal(moved.flow.state.review, null)
  assert.match(moved.flow.state.refusal.text, /Read the review again before you confirm\. Nothing was confirmed\.$/)
})

test('every refusal the server sends is shown with its text, at the step that made it', async () => {
  const cases = [
    ['prepare', 'POST projects/p1/decisions/dec-05/prepare', 400, 'invalid_prepare_body', null, /could not use what this page sent for this action\. Nothing was prepared\.$/],
    ['prepare', 'POST projects/p1/decisions/dec-05/prepare', 400, 'exact_version_required', null, /could not resolve that version to an exact commit/],
    ['prepare', 'POST projects/p1/decisions/dec-05/prepare', 409, 'preparation_conflict', null, /refused to prepare this/],
    ['prepare', 'POST projects/p1/decisions/dec-05/prepare', 403, 'confirmation_token_required', null, /Reload this page and try again/],
    ['prepare', 'POST projects/p1/decisions/dec-05/prepare', 403, 'same_origin_required', null, /only from its own page/],
    ['prepare', 'POST projects/p1/decisions/dec-05/prepare', 404, 'decision_not_found', null, /no longer in the record/],
    ['prepare', 'POST projects/p1/decisions/dec-05/prepare', 503, 'upgrade_pending', null, /thruline upgrade/],
    ['prepare', 'POST projects/p1/decisions/dec-05/prepare', 413, undefined, null, /too long to send/],
    ['prepare', 'POST projects/p1/decisions/dec-05/prepare', 418, 'a_code_this_file_has_no_words_for', 'The detail the server gave.', /refused this \(it answered 418\): a_code_this_file_has_no_words_for\. The detail the server gave\. Nothing was prepared\.$/],
    ['review', 'POST projects/p1/decisions/dec-05/review', 409, 'review_unavailable', null, /could not produce the review/],
    ['review', 'POST projects/p1/decisions/dec-05/review', 404, 'act_not_found', null, /prepared line is no longer in the record/],
    ['confirm', 'POST projects/p1/decisions/dec-05/native-confirm', 409, 'native_confirmation_busy', null, /Another confirmation window is already open on this Mac\. Finish or close it, then try again\. Nothing was confirmed\.$/],
    ['confirm', 'POST projects/p1/decisions/dec-05/native-confirm', 409, 'native_confirmation_unavailable', 'The native review needs a foreground owned terminal.', /not available right now\. The native review needs a foreground owned terminal\. Nothing was confirmed\.$/],
    ['confirm', 'POST projects/p1/decisions/dec-05/native-confirm', 400, 'review_sha256_required', null, /did not get the review’s digest/],
    ['confirm', 'POST projects/p1/decisions/dec-05/native-confirm', 400, 'invalid_native_body', null, /could not use what this page sent/],
    ['confirm', 'POST projects/p1/decisions/dec-05/native-confirm', 503, 'job_store_unavailable', null, /so no window was opened/],
  ]
  for (const [step, key, status, error, detail, shown] of cases) {
    const { flow } = flowOn(answers({ [key]: { status, doc: error ? { schema_version: 1, error, detail } : null } }))
    await flow.prepare(FIELDS)
    if (step === 'confirm') await flow.confirm()
    const r = flow.state.refusal
    assert.ok(r, step + ' ' + error)
    assert.equal(r.status, status)
    assert.match(r.text, shown, step + ' ' + error)
    // a refusal at prepare leaves the form; at review leaves a prepared line to read again; at confirm keeps the review
    assert.equal(flow.state.step, step === 'prepare' ? 'form' : step === 'review' ? 'prepared' : 'review', step + ' ' + error)
    assert.notEqual(flow.state.outcome && flow.state.outcome.kind, 'recorded')
  }
})

test('a refused token, and a server that did not answer, are shown too', async () => {
  const noToken = flowOn(answers({ 'GET decisions/confirmation-token': { status: 403, doc: { schema_version: 1, error: 'same_origin_required', detail: null } } }))
  await noToken.flow.prepare(FIELDS)
  assert.match(noToken.flow.state.refusal.text, /only from its own page\. Reload this page and try again\. Nothing was prepared\.$/)
  assert.equal(noToken.calls.length, 1, 'no POST without the token')
  const down = flowOn(() => new TypeError('Failed to fetch'))
  await down.flow.prepare(FIELDS)
  assert.equal(down.flow.state.refusal.code, 'unreachable')
  assert.match(down.flow.state.refusal.text, /did not answer, so it may not have received this\.$/)
  // a token the server no longer knows is dropped, and the next request reads it again
  let given = 0
  const stale = flowOn((method, url) => {
    if (url.endsWith('confirmation-token')) return { status: 200, doc: { schema_version: 1, token: 'tok-' + ++given } }
    if (url.endsWith('/prepare')) return given === 1 ? { status: 403, doc: { schema_version: 1, error: 'confirmation_token_required', detail: null } } : answers()(method, url)
    return answers()(method, url)
  })
  await stale.flow.prepare(FIELDS)
  assert.equal(stale.flow.state.step, 'form')
  await stale.flow.prepare(FIELDS)
  assert.equal(stale.flow.state.step, 'review')
  assert.deepEqual(stale.calls.filter((c) => c.method === 'POST').map((c) => c.headers[TOKEN_HEADER]), ['tok-1', 'tok-2', 'tok-2'])
})

test('refusalOf reads a response that is not a contract error without hiding it', () => {
  assert.match(refusalOf({ ok: true, status: 200, doc: {} }, 'review').text, /^ThruLine answered in a form this page cannot read\. Nothing was confirmed\.$/)
  assert.match(refusalOf(undefined, 'prepare').text, /^ThruLine refused this\. Nothing was prepared\.$/)
  assert.equal(refusalOf({ ok: false, status: 0, doc: { error: 'unreachable' } }, 'confirm').code, 'unreachable')
})

test('a running job is never presented as accepted; only a job that names a receipt is a recorded act', () => {
  const running = jobOutcome(JOB(), 'accept')
  assert.equal(running.kind, 'running')
  assert.equal(running.final, false)
  assert.equal(running.receiptId, null)
  assert.match(running.sentence, /^Review in the native window\. Waiting for you in the window on this Mac\. Nothing is recorded until you choose Confirm there and pass the Mac’s own presence check\.$/)
  assert.doesNotMatch(running.sentence, /accepted|retired|set this rule aside|recorded as receipt/i)
  // even a running job that carries a receipt id is not read as done
  assert.equal(jobOutcome(JOB({ receipt_id: 'rcpt-x' }), 'accept').kind, 'running')
  // complete + receipt
  const done = jobOutcome(JOB({ state: 'complete', receipt_id: 'rcpt-1', derived_written: true }), 'accept')
  assert.deepEqual([done.kind, done.final, done.receiptId], ['recorded', true, 'rcpt-1'])
  assert.equal(done.sentence, 'You accepted this rule. It is recorded as receipt rcpt-1. The decisions file in the project was refreshed.')
  assert.equal(jobOutcome(JOB({ state: 'complete', receipt_id: 'rcpt-2' }), 'retire').sentence, 'You retired this rule. It is recorded as receipt rcpt-2.')
  assert.equal(jobOutcome(JOB({ state: 'complete', receipt_id: 'rcpt-3' }), 'waive').sentence, 'You set this rule aside for that version. It is recorded as receipt rcpt-3.')
  // complete without a receipt names nothing recorded
  const none = jobOutcome(JOB({ state: 'complete' }), 'accept')
  assert.equal(none.kind, 'no_receipt')
  assert.match(none.sentence, /names no receipt, so nothing is recorded as confirmed/)
  // failed: the job's own errors, as given
  const failed = jobOutcome(JOB({ state: 'failed', errors: ['the review changed before the window opened'] }), 'accept')
  assert.equal(failed.kind, 'failed')
  assert.equal(failed.sentence, 'Not confirmed. the review changed before the window opened No receipt was recorded.')
  assert.match(jobOutcome(JOB({ state: 'failed' }), 'accept').sentence, /ThruLine gave no reason/)
  // a state this page does not know is not read as anything
  assert.equal(jobOutcome(JOB({ state: 'confirmed' }), 'accept').kind, 'unreadable')
  assert.equal(jobOutcome(null, 'accept').kind, 'unreadable')
})

test('a recorded receipt whose project copy did not refresh is said as it is, with the job’s own error', () => {
  const o = jobOutcome(JOB({ state: 'complete', receipt_id: 'rcpt-9', derived_written: false, errors: ['derived copy: permission denied'] }), 'accept')
  assert.equal(o.kind, 'recorded_file_stale')
  assert.equal(o.receiptId, 'rcpt-9')
  assert.equal(o.sentence, 'You accepted this rule, and the receipt is recorded (rcpt-9). The decisions file in the project did not refresh: derived copy: permission denied The record has the receipt; the copy in the project needs repairing.')
  assert.doesNotMatch(o.sentence, /was refreshed/)
})

test('following the job: a running job stays running until the server says otherwise, and nothing is recorded meanwhile', async () => {
  let polls = 0
  const { flow, calls, recorded } = flowOn(answers({
    'GET decisions/check?job_id=job-1': () => {
      polls += 1
      return polls < 3
        ? { status: 200, doc: JOB({ progress: { label: 'Review in the native window', unit: 'person_action', completed: 0, total: 1 } }) }
        : { status: 200, doc: JOB({ state: 'complete', receipt_id: 'rcpt-1', derived_written: true }) }
    },
  }))
  await flow.prepare(FIELDS)
  const kinds = []
  flow.subscribe((s) => s.outcome && kinds.push(s.outcome.kind))
  const walk = flow.confirm()
  await walk
  assert.equal(calls.filter((c) => c.url.includes('check?job_id=')).length, 3)
  assert.deepEqual([...new Set(kinds)], ['running', 'recorded'])
  assert.equal(recorded.length, 1, 'recorded once, after the receipt, not while it ran')
  assert.equal(kinds.indexOf('recorded'), kinds.length - 1)
})

test('a failed job, and a job the server no longer holds, are shown as they are and record nothing', async () => {
  const failed = flowOn(answers({ 'GET decisions/check?job_id=job-1': { status: 200, doc: JOB({ state: 'failed', errors: ['cancelled in the window'] }) } }))
  await failed.flow.prepare(FIELDS)
  await failed.flow.confirm()
  assert.equal(failed.flow.state.outcome.kind, 'failed')
  assert.match(failed.flow.state.outcome.sentence, /cancelled in the window/)
  assert.deepEqual(failed.recorded, [])
  const lost = flowOn(answers({ 'GET decisions/check?job_id=job-1': { status: 404, doc: { schema_version: 1, error: 'job_not_found', detail: null } } }))
  await lost.flow.prepare(FIELDS)
  await lost.flow.confirm()
  assert.equal(lost.flow.state.outcome.kind, 'lost')
  assert.match(lost.flow.state.outcome.sentence, /Nothing is shown as confirmed/)
  assert.deepEqual(lost.recorded, [])
  const odd = flowOn(answers({ 'GET decisions/check?job_id=job-1': { status: 200, doc: JOB({ job_id: 'someone-elses' }) } }))
  await odd.flow.prepare(FIELDS)
  await odd.flow.confirm()
  assert.equal(odd.flow.state.outcome.kind, 'unreadable', 'another job’s answer is not this job’s')
})

test('a server that stops answering while the window is open is not read as a result, and the page can look again', async () => {
  let up = false
  const { flow, calls, recorded } = flowOn(answers({
    'GET decisions/check?job_id=job-1': () => (up ? { status: 200, doc: JOB({ state: 'complete', receipt_id: 'rcpt-1' }) } : new TypeError('Failed to fetch')),
  }))
  await flow.prepare(FIELDS)
  await flow.confirm()
  assert.equal(calls.filter((c) => c.url.includes('check?job_id=')).length, 3, 'three silent reads, then it stops')
  assert.equal(flow.state.outcome.kind, 'unreachable')
  assert.equal(flow.state.step, 'ended')
  assert.deepEqual(recorded, [])
  up = true
  await flow.follow()
  assert.equal(flow.state.outcome.kind, 'recorded')
  assert.equal(recorded.length, 1)
})

test('stopping the flow stops reading the job', async () => {
  let reads = 0
  const f = server(answers({ 'GET decisions/check?job_id=job-1': () => { reads += 1; return { status: 200, doc: JOB() } } }))
  let flow
  flow = confirmFlow({ client: confirmClient(f.f), project: 'p1', decision: 'dec-05', action: 'accept', wait: async () => { if (reads >= 2) flow.stop() } })
  await flow.prepare(FIELDS)
  await flow.confirm()
  assert.equal(reads, 2)
  assert.equal(flow.state.step, 'following')
})

test('the review is shown in full: its facts, and every field exactly as returned', () => {
  const f = reviewFacts(REVIEW)
  assert.equal(f.summary, REVIEW.summary)
  assert.deepEqual(f.rows, [
    ['Action', 'accept'],
    ['The rule', 'Every exported time is UTC, written with a Z'],
    ['Kind', 'timezone_policy'],
    ['Revision', '1 (act revision)'],
    ['Parameters SHA-256', REVIEW.decision.params_sha256],
  ])
  assert.deepEqual(JSON.parse(f.json), REVIEW)
  assert.deepEqual(reviewFacts(null).rows, [])
})

test('the module’s words use no intent word, no confidence, no time-zone name and no home folder', () => {
  const src = readFileSync(join(here, 'decision-confirm.js'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '')
  assert.doesNotMatch(src, /\b(lie|lied|cheat\w*|violat\w*|ignor\w*|hallucinat\w*|trial)\b/i)
  assert.doesNotMatch(src, /\d\s*%|\b(UTC|GMT|PST|IST)\b|\/Users\/|~\//)
  // the honest sentence: a click here is not the confirmation
  assert.match(jobOutcome(JOB(), 'accept').sentence, /Nothing is recorded until you choose Confirm there and pass the Mac’s own presence check/)
})

test('census: this module reaches exactly its five addresses (three POSTs and two GETs), and no other file under ui/src/hq reaches any of them', async () => {
  // behaviour: a whole walk touches these and only these
  const { flow, calls } = flowOn(answers())
  await flow.prepare(FIELDS)
  await flow.confirm()
  const reached = [...new Set(calls.map((c) => c.method + ' ' + c.url.replace(/^\/api\/hq\//, '').replace('projects/p1/decisions/dec-05/', '<d>/').replace('job_id=job-1', 'job_id=<id>')))].sort()
  assert.deepEqual(reached, [
    'GET decisions/check?job_id=<id>',
    'GET decisions/confirmation-token',
    'POST <d>/native-confirm',
    'POST <d>/prepare',
    'POST <d>/review',
  ])
  assert.equal(calls.filter((c) => c.method === 'POST').length, 3, 'three write addresses')
  // source: the address literals in this file are those five, and the only write method is the one helper's POST
  const src = readFileSync(join(here, 'decision-confirm.js'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '')
  const literals = [...src.matchAll(/[`'](\/api\/[^`']*)[`']/g)].map((m) => m[1])
  assert.deepEqual(literals, [
    '/api/hq/decisions/confirmation-token',
    '/api/hq/projects/${enc(project)}/decisions/${enc(decision)}/prepare',
    '/api/hq/projects/${enc(project)}/decisions/${enc(decision)}/review',
    '/api/hq/projects/${enc(project)}/decisions/${enc(decision)}/native-confirm',
    '/api/hq/decisions/check?job_id=${enc(job)}',
  ])
  assert.equal([...src.matchAll(/'POST'/g)].length, 1, 'every write goes through the one POST helper')
  assert.equal([...src.matchAll(/'(PUT|PATCH|DELETE)'/g)].length, 0)
  // no other file under ui/src/hq reaches them
  const root = join(here, '..')
  const hits = []
  const walk = (dir) => {
    for (const n of readdirSync(dir)) {
      const p = join(dir, n)
      if (statSync(p).isDirectory()) walk(p)
      else if (/\.(js|jsx)$/.test(n) && !/\.test\.js$/.test(n)) {
        const body = readFileSync(p, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '')
        if (/native-confirm|confirmation-token|decisions\/check\?job_id|\/decisions\/\$\{[^}]*\}\/(prepare|review)/.test(body)) hits.push(p.slice(root.length + 1))
      }
    }
  }
  walk(root)
  assert.deepEqual(hits, ['data/decision-confirm.js'])
})
