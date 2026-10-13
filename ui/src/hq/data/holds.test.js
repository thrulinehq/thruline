/* C30 R-LEDGER capture holds: the pause and exclusions writes (write addresses
 * 26 and 27) and the read of `holds`.
 *
 * Proven against a stubbed `fetch`; nothing here reaches a server. The body is
 * exactly {act} (a pause) or {act, pattern} (an exclusion) — never a folder, a
 * project, a name or a time: the server reads those. This fixture models an
 * older, recorded-only answer; the client never infers an honoured boundary
 * unless the server says so. Across `ui/src/hq` this is the only file that
 * reaches either address. Synthetic fixtures only. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { HOLDS_PAUSE, HOLDS_EXCLUSIONS, recordHold, holdAnswerOf, holdsOf } from './holds.js'

const here = dirname(fileURLToPath(import.meta.url))
const stub = (status, doc) => {
  const calls = []
  const f = async (url, init) => {
    calls.push({ url, method: init && init.method, body: init && init.body ? JSON.parse(init.body) : undefined })
    return { ok: status >= 200 && status < 300, status, json: async () => doc }
  }
  return { f, calls }
}

const NOT_HONOURED = 'Nothing honours a pause or an exclusion yet. It is recorded, and ThruLine still reads this folder as before.'
const HOLDS = {
  paused: true,
  paused_at: '2026-10-08T09:00:00Z',
  paused_by: 'robin',
  exclusions: [
    { key: 'env', covers: '.env and .env.* files', note: 'environment files', built_in: true, on: true, source: 'default', basis: 'on by default (not recorded)', since: null, by: null },
    { key: 'generated', covers: 'generated files', note: 'files your repo marks as generated', built_in: true, on: false, source: 'default', basis: 'off by default (not recorded)', since: null, by: null },
    { key: 'vendor/legacy', covers: 'vendor/legacy', note: null, built_in: false, on: true, source: 'recorded', basis: null, since: '2026-10-08T09:01:00Z', by: 'robin' },
  ],
  honoured: false,
  exclusions_honoured: false,
  note: NOT_HONOURED,
}

test('the addresses name the project on screen, encoded', () => {
  assert.equal(HOLDS_PAUSE('p 1'), '/api/hq/projects/p%201/capture/pause')
  assert.equal(HOLDS_EXCLUSIONS('p/1'), '/api/hq/projects/p%2F1/capture/exclusions')
})

test('a pause sends exactly the act and an exclusion exactly the act and the pattern as typed', async () => {
  const a = stub(200, { act: 'paused', holds: HOLDS })
  const r = await recordHold({ project: 'p1', kind: 'pause', act: 'paused' }, a.f)
  assert.ok(r.ok)
  assert.deepEqual(a.calls, [{ url: '/api/hq/projects/p1/capture/pause', method: 'POST', body: { act: 'paused' } }])
  assert.deepEqual(Object.keys(a.calls[0].body), ['act'], 'no folder, project, name or time')

  const b = stub(200, { act: 'excluded', holds: HOLDS })
  await recordHold({ project: 'p1', kind: 'exclusion', act: 'excluded', pattern: '  vendor/legacy ' }, b.f)
  assert.deepEqual(b.calls, [{ url: '/api/hq/projects/p1/capture/exclusions', method: 'POST', body: { act: 'excluded', pattern: '  vendor/legacy ' } }])
  assert.deepEqual(Object.keys(b.calls[0].body), ['act', 'pattern'])
})

test('nothing is sent without a project, a known act and, for an exclusion, words', async () => {
  const { f, calls } = stub(200, {})
  for (const bad of [
    { kind: 'pause', act: 'paused' },
    { project: 'p1', kind: 'pause', act: 'stop' },
    { project: 'p1', kind: 'pause', act: 'excluded' },
    { project: 'p1', kind: 'exclusion', act: 'paused', pattern: 'env' },
    { project: 'p1', kind: 'exclusion', act: 'excluded' },
    { project: 'p1', kind: 'exclusion', act: 'excluded', pattern: '   ' },
    { project: 'p1', kind: 'exclusion', act: 'excluded', pattern: 7 },
    { project: 'p1', kind: 'other', act: 'paused' },
  ]) {
    const r = await recordHold(bad, f)
    assert.equal(r.ok, false)
    assert.equal(r.status, 0)
  }
  assert.deepEqual(calls, [])
  const empty = await recordHold({ project: 'p1', kind: 'exclusion', act: 'excluded', pattern: ' ' }, f)
  assert.equal(holdAnswerOf(empty).code, 'pattern_required')
})

test('a kept act is read with where the folder stands, and what it still does not do', () => {
  const kept = holdAnswerOf({ ok: true, status: 200, doc: { act: 'paused', hold: { id: 'chd_1', act: 'paused', at: '2026-10-08T09:00:00Z', by: 'robin' }, holds: HOLDS, codex_observer: 'not_watching' } })
  assert.equal(kept.kind, 'kept')
  assert.equal(kept.act, 'paused')
  assert.equal(kept.codexObserver, 'not_watching')
  assert.equal(kept.holds.paused, true)
  assert.equal(kept.holds.honoured, false, 'recorded is not honoured')
  assert.equal(kept.holds.exclusionsHonoured, false, 'recorded is not an active serve boundary')
  assert.equal(kept.holds.note, NOT_HONOURED)
})

test('every refusal says nothing was saved, in the server’s words where it has them', () => {
  const server = { already_paused: 'This project is already paused, so nothing was saved.', not_excluded: 'That path is not excluded, so nothing was saved.', bad_pattern: 'That cannot be left out: it is empty. Use a built-in name (env, secrets, generated) or a path inside the project. Nothing was saved.' }
  for (const [error, message] of Object.entries(server)) {
    const a = holdAnswerOf({ ok: false, status: error === 'bad_pattern' ? 400 : 409, doc: { error, message } })
    assert.equal(a.kind, 'refused')
    assert.equal(a.code, error)
    assert.equal(a.detail, message)
  }
  const own = ['malformed_request', 'unknown_act', 'same_origin_required', 'no_such_project', 'upgrade_pending', 'too_long']
  for (const error of own) {
    const a = holdAnswerOf({ ok: false, status: 400, doc: { error } })
    assert.equal(a.kind, 'refused', error)
    assert.match(a.detail, /Nothing was saved\./, error)
  }
  assert.match(holdAnswerOf({ ok: false, status: 413, doc: null }).detail, /Nothing was saved\./)
  assert.match(holdAnswerOf({ ok: false, status: 409, doc: { error: 'too_long', field: 'pattern', limit: 256 } }).detail, /256 characters/)
  // A busy store, a server that did not answer and a form this page cannot read
  // never claim the record was left alone.
  assert.equal(holdAnswerOf({ ok: false, status: 503, doc: { error: 'store_busy', message: 'Another ThruLine process was writing.' } }).detail, 'Another ThruLine process was writing.')
  assert.equal(holdAnswerOf({ ok: false, status: 0, doc: { error: 'unreachable' } }).code, 'unreachable')
  assert.equal(holdAnswerOf(null).code, 'unreachable')
  assert.equal(holdAnswerOf({ ok: true, status: 200, doc: { nothing: true } }).code, 'unreadable')
  assert.equal(holdAnswerOf({ ok: false, status: 500, doc: {} }).code, 'other')
})

test('holdsOf reads the capture document’s holds for a screen and never guesses', () => {
  const h = holdsOf({ holds: HOLDS })
  assert.equal(h.paused, true)
  assert.equal(h.pausedAt, '2026-10-08T09:00:00Z')
  assert.equal(h.pausedBy, 'robin')
  assert.deepEqual(h.exclusions.map((e) => e.key), ['env', 'generated', 'vendor/legacy'])
  const env = h.exclusions[0]
  assert.deepEqual({ on: env.on, builtIn: env.builtIn, source: env.source, basis: env.basis }, { on: true, builtIn: true, source: 'default', basis: 'on by default (not recorded)' })
  assert.equal(h.exclusions[1].on, false)
  assert.deepEqual({ on: h.exclusions[2].on, by: h.exclusions[2].by, basis: h.exclusions[2].basis }, { on: true, by: 'robin', basis: null })

  // No holds, an older server, or a record that could not be read: null, not "not paused".
  for (const doc of [null, undefined, {}, { holds: null, holds_reason: 'the pauses and exclusions could not be read' }, { holds: {} }, { holds: { exclusions: 'x' } }]) {
    assert.equal(holdsOf(doc), null)
  }
})

test('honoured is false unless the server said true, and a half-formed row is dropped', () => {
  const quiet = holdsOf({ holds: { paused: false, exclusions: [{ key: 'env', on: true }, null, { on: true }, 'x'] } })
  assert.equal(quiet.honoured, false)
  assert.equal(quiet.note, '')
  assert.equal(quiet.paused, false)
  assert.deepEqual(quiet.exclusions.map((e) => e.key), ['env'])
  assert.equal(quiet.exclusions[0].source, 'default')
  assert.equal(holdsOf({ holds: { exclusions: [], honoured: true } }).honoured, true)
  assert.equal(holdsOf({ holds: { exclusions: [], honoured: 'yes' } }).honoured, false)
})

test('no word that overclaims is in the copy', () => {
  const src = readFileSync(join(here, 'holds.js'), 'utf8')
  for (const banned of [/\blie\b/i, /cheat/i, /violat/i, /ignore/i, /hallucinat/i, /\btrial\b/i, /Not available in this version/i, /Never record/i, /~\//]) {
    assert.doesNotMatch(src, banned)
  }
})

test('this is the only file under ui/src/hq that reaches either address, and it POSTs from one place', () => {
  const root = join(here, '..')
  const hits = []
  const walk = (dir) => {
    for (const n of readdirSync(dir)) {
      const p = join(dir, n)
      if (statSync(p).isDirectory()) walk(p)
      else if (/\.(js|jsx)$/.test(n) && !n.endsWith('.test.js') && /capture\/(pause|exclusions)/.test(readFileSync(p, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, ''))) hits.push(p.slice(root.length + 1))
    }
  }
  walk(root)
  assert.deepEqual(hits, ['data/holds.js'])
  const src = readFileSync(join(here, 'holds.js'), 'utf8')
  assert.equal([...src.matchAll(/method:\s*'POST'/g)].length, 1)
  assert.equal([...src.matchAll(/'\/api\//g)].length, 2, 'two addresses')
})
