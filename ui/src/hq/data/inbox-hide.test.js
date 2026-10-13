/* The Inbox sends back the row address the server served. A hide is a record,
 * so stale or missing addresses never become a local-only dismissal. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { hideRows, INBOX_HIDES_ADDRESS, writeInboxHide } from './inbox-hide.js'

const ADDRESS = {
  row_kind: 'prompt',
  thread_id: 'thread-1',
  row_sha256: 'a'.repeat(64),
  hidden: false,
}

test('a hide reading belongs to exactly the served project and keeps the server row address', () => {
  const doc = { project: 'project-1', rows: [ADDRESS, { ...ADDRESS, thread_id: 'thread-2', hidden: true }] }
  assert.equal(hideRows(doc, 'another-project'), null)
  assert.equal(hideRows({ project: 'project-1', rows: null }, 'project-1'), null)
  assert.deepEqual([...hideRows(doc, 'project-1').keys()], ['thread-1', 'thread-2'])
  assert.equal(hideRows(doc, 'project-1').get('thread-2').hidden, true)
})

test('hiding posts only the served content address, with same-origin credentials', async () => {
  const calls = []
  const answer = { row: { ...ADDRESS, hidden: true, act_id: 'ibh_1' } }
  const fetcher = async (...args) => {
    calls.push(args)
    return { ok: true, json: async () => answer }
  }
  assert.equal(INBOX_HIDES_ADDRESS('project with space'), '/api/hq/projects/project%20with%20space/inbox-hides')
  assert.equal(await writeInboxHide('project with space', ADDRESS, 'hidden', fetcher), answer)
  assert.equal(calls.length, 1)
  const [url, options] = calls[0]
  assert.equal(url, INBOX_HIDES_ADDRESS('project with space'))
  assert.equal(options.method, 'POST')
  assert.equal(options.credentials, 'same-origin')
  assert.deepEqual(JSON.parse(options.body), {
    act: 'hidden', row_kind: 'prompt', thread_id: 'thread-1', expected_sha256: ADDRESS.row_sha256,
  })
  assert.equal(options.headers['content-type'], 'application/json')
})

test('missing or malformed row addresses cannot send a hide', async () => {
  let writes = 0
  const fetcher = async () => { writes++; throw new Error('should not send') }
  for (const [project, row] of [
    [null, ADDRESS],
    ['project-1', null],
    ['project-1', { ...ADDRESS, row_kind: 'unknown' }],
    ['project-1', { ...ADDRESS, row_sha256: 'not-a-digest' }],
    ['project-1', { ...ADDRESS, thread_id: '' }],
  ]) {
    await assert.rejects(writeInboxHide(project, row, 'hidden', fetcher), /cannot be hidden until its record is read again/)
  }
  assert.equal(writes, 0)
})

test('server refusals remain visible, including a stale row', async () => {
  const stale = async () => ({ ok: false, status: 409, json: async () => ({ error: 'row_changed', message: 'The record changed. Reload it.' }) })
  await assert.rejects(writeInboxHide('project-1', ADDRESS, 'hidden', stale), /The record changed\. Reload it\./)
  const unavailable = async () => { throw new Error('offline') }
  await assert.rejects(writeInboxHide('project-1', ADDRESS, 'hidden', unavailable), /local server did not answer/)
})
