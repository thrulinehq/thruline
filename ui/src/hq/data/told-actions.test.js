/* C30 R-LEDGER: the one write for "told the agent" (write address 24).
 *
 * Proven against a stubbed `fetch`; nothing here reaches a server. The body is
 * exactly {act, message?, refers_to?} — never a digest, an anchor, a version, a
 * Change or a name: the server reads those. Every refusal is said in words that
 * add "Nothing was sent."; and across `ui/src/hq` this is the only file that
 * reaches the address. Synthetic fixtures only. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { TOLD_AGENT, recordTold, toldAnswerOf } from './told-actions.js'

const here = dirname(fileURLToPath(import.meta.url))
const stub = (status, doc) => {
  const calls = []
  const f = async (url, init) => {
    calls.push({ url, method: init && init.method, body: init && init.body ? JSON.parse(init.body) : undefined })
    return { ok: status >= 200 && status < 300, status, json: async () => doc }
  }
  return { f, calls }
}

test('the address names the project and the thread on screen, encoded', () => {
  assert.equal(TOLD_AGENT('p 1', 't/1'), '/api/hq/projects/p%201/threads/t%2F1/told-agent')
})

test('a copy sends exactly the act and the words as the person left them', async () => {
  const { f, calls } = stub(200, { act: { id: 'cam_1' }, told_agent: { last_act: 'copied' } })
  const words = '  Re-run the parser test.\n\n'
  const r = await recordTold({ project: 'p1', thread: 't1', act: 'copied', message: words }, f)
  assert.ok(r.ok)
  assert.deepEqual(calls, [{ url: '/api/hq/projects/p1/threads/t1/told-agent', method: 'POST', body: { act: 'copied', message: words } }])
  assert.deepEqual(Object.keys(calls[0].body), ['act', 'message'], 'no digest, anchor, version, Change or name')
})

test('a mark and an undo send only the act and, when the screen saw one, the row they are about', async () => {
  const { f, calls } = stub(200, { act: { id: 'cam_2' }, told_agent: null })
  await recordTold({ project: 'p1', thread: 't1', act: 'marked_pasted' }, f)
  await recordTold({ project: 'p1', thread: 't1', act: 'marked_pasted', refersTo: 'cam_1' }, f)
  await recordTold({ project: 'p1', thread: 't1', act: 'unmarked_pasted', refersTo: 'cam_2' }, f)
  assert.deepEqual(calls.map((c) => c.body), [{ act: 'marked_pasted' }, { act: 'marked_pasted', refers_to: 'cam_1' }, { act: 'unmarked_pasted', refers_to: 'cam_2' }])
})

test('nothing is posted without the work on screen, without words, or for an act the record does not have', async () => {
  const { f, calls } = stub(200, {})
  for (const args of [
    { project: '', thread: 't1', act: 'copied', message: 'x' },
    { project: 'p1', thread: '', act: 'copied', message: 'x' },
    { project: 'p1', thread: 't1', act: 'copied', message: '   ' },
    { project: 'p1', thread: 't1', act: 'copied' },
    { project: 'p1', thread: 't1', act: 'sent', message: 'x' },
    { project: 'p1', thread: 't1', act: 'delivered' },
  ]) {
    const r = await recordTold(args, f)
    assert.equal(r.ok, false)
    assert.equal(r.status, 0)
  }
  assert.deepEqual(calls, [])
})

test('an unreachable server is a refusal in words, not a throw', async () => {
  const f = async () => { throw new Error('down') }
  const r = await recordTold({ project: 'p1', thread: 't1', act: 'marked_pasted' }, f)
  assert.deepEqual(r, { ok: false, status: 0, doc: { error: 'unreachable' } })
  const a = toldAnswerOf(r)
  assert.equal(a.kind, 'refused')
  assert.match(a.detail, /did not answer/)
})

test('a kept act hands back the act and the thread’s told_agent after it', () => {
  const a = toldAnswerOf({ ok: true, status: 200, doc: { act: { id: 'cam_1' }, told_agent: { last_act: 'copied', later_work: false }, nothing_sent: true } })
  assert.equal(a.kind, 'kept')
  assert.equal(a.act.id, 'cam_1')
  assert.equal(a.told.last_act, 'copied')
  assert.equal(toldAnswerOf({ ok: true, status: 200, doc: {} }).kind, 'refused', '200 without an act is not a kept act')
})

test('every refusal the server names is said in words that add "Nothing was sent."', () => {
  const codes = {
    message_required: 400, malformed_request: 400, unknown_act: 400, same_origin_required: 403,
    no_such_project: 404, no_such_thread: 404, too_long: 409, no_copy_to_mark: 409, already_marked: 409,
    no_mark_to_undo: 409, upgrade_pending: 503,
  }
  for (const [error, status] of Object.entries(codes)) {
    const a = toldAnswerOf({ ok: false, status, doc: { error } })
    assert.equal(a.kind, 'refused', error)
    assert.equal(a.code, error)
    assert.match(a.detail, /Nothing was sent\.$/, error)
    assert.ok(!/malformed_request|same_origin|upgrade_pending|no_copy_to_mark|HTTP/.test(a.detail), 'no code reaches the Plain surface: ' + a.detail)
    for (const w of ['lie', 'cheat', 'violate', 'ignore', 'hallucinat', 'trial', 'Not available in this version']) assert.ok(!a.detail.includes(w), w)
  }
  const busy = toldAnswerOf({ ok: false, status: 503, doc: { error: 'store_busy', message: 'Another ThruLine process was writing to the record, so nothing was changed.' } })
  assert.match(busy.detail, /^Another ThruLine process was writing/)
  assert.match(busy.detail, /Nothing was sent\.$/)
  assert.equal(toldAnswerOf({ ok: false, status: 413, doc: null }).code, 'too_long')
  const other = toldAnswerOf({ ok: false, status: 500, doc: { error: 'read_failed' } })
  assert.match(other.detail, /answered 500/)
  assert.match(other.detail, /Nothing was sent\.$/)
  assert.equal(toldAnswerOf(undefined).kind, 'refused')
})

test('census: across ui/src/hq only this file reaches the told-agent address, with exactly one POST', () => {
  const root = join(here, '..')
  const hits = []
  const walk = (dir) => {
    for (const n of readdirSync(dir)) {
      const p = join(dir, n)
      if (statSync(p).isDirectory()) walk(p)
      else if (/\.(js|jsx)$/.test(n) && !n.endsWith('.test.js') && /told-agent/.test(readFileSync(p, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, ''))) hits.push(p.slice(root.length + 1))
    }
  }
  walk(root)
  assert.deepEqual(hits, ['data/told-actions.js'])
  const src = readFileSync(join(here, 'told-actions.js'), 'utf8')
  assert.equal([...src.matchAll(/method:\s*'POST'/g)].length, 1)
  assert.equal([...src.matchAll(/'\/api\//g)].length, 1, 'one address')
})
