/* The preferences store (C30 R-PREFS): read once at start, written one key or
 * a subset at a time, and every refusal said instead of pretended away.
 *
 * Every fetch is a stub; nothing reaches a server. What is pinned:
 *
 *  1. one read at start; defaults until it answers, and after it fails;
 *  2. a write changes nothing until the server's answer shows it kept;
 *  3. one key, one notification switch, or a subset: one request each;
 *  4. each refusal (every 409 and 400 the route has, a missing route, no
 *     answer, an answer that does not show the choice) leaves the value as it
 *     was, keeps WHY in plain words, and returns false;
 *  5. a choice ThruLine does not offer never reaches the server. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createPreferences, DEFAULTS, NAMES, allowed, valueOf, fileWords } from './preferences.js'
import { refusedLine, keepingLine, stillLine, valueWords, NOT_READ, READING } from './preference-words.js'

const FORBIDDEN = /\b(lie|lies|lying|cheat|violate|violation|ignore|ignored|hallucinat\w*|trial)\b|\d\s?%|UTC|GMT|\/Users\//i

const doc = (over = {}, extra = {}) => ({
  schema_version: 1,
  preferences: Object.assign({ face: 'plain', density: 'comfortable', motion: 'system', colour_vision: 'standard', keys: 'on', masked_copy: true, notifications: { ready: false, not_backed: false, agent_waiting: false, recording_stopped: false }, weekly_summary: false }, over),
  defaults: {},
  path: '/Users/someone/.thruline/preferences.json',
  file: 'read',
  from_file: Object.keys(over),
  unknown_keys: [],
  problems: [],
  ...extra,
})

/** A server holding one preferences document; every request is recorded. */
function server({ get, post } = {}) {
  const calls = []
  let held = doc().preferences
  const fetchImpl = async (url, opts = {}) => {
    const method = opts.method || 'GET'
    calls.push({ url, method, body: opts.body, headers: opts.headers })
    if (method === 'GET') {
      if (get) return get()
      return { ok: true, status: 200, json: async () => doc(held) }
    }
    if (post) {
      const out = post(opts)
      if (out) return out
    }
    const b = JSON.parse(opts.body)
    const next = Object.assign({}, held, b)
    if (b.notifications) next.notifications = Object.assign({}, held.notifications, b.notifications)
    held = next
    return { ok: true, status: 200, json: async () => doc(held) }
  }
  return { fetchImpl, calls, held: () => held }
}
const refusal = (status, body) => () => ({ ok: false, status, json: async () => body })

test('1. one read at start: defaults until it answers, then the file’s choices; a second load shares the one request', async () => {
  const s = server()
  let release
  const gate = new Promise((r) => { release = r })
  const slow = async (...a) => { await gate; return s.fetchImpl(...a) }
  const store = createPreferences({ fetchImpl: slow })
  assert.equal(store.get().status, 'idle')
  assert.deepEqual(store.get().prefs, DEFAULTS)
  const a = store.load()
  const b = store.load()
  assert.equal(store.get().status, 'reading')
  assert.deepEqual(store.get().prefs, DEFAULTS, 'a default is drawn while the answer is out')
  release()
  await Promise.all([a, b])
  assert.equal(store.get().status, 'read')
  assert.equal(s.calls.filter((c) => c.method === 'GET').length, 1)
  assert.equal(s.calls[0].url, '/api/preferences')
})

test('1. a read that fails (route missing, no answer, nonsense) keeps every key at its default and says it failed', async () => {
  for (const get of [refusal(404, {}), () => { throw new Error('connection refused') }, () => ({ ok: true, status: 200, json: async () => ({ hello: 'world' }) })]) {
    const store = createPreferences({ fetchImpl: server({ get }).fetchImpl })
    await store.load()
    assert.equal(store.get().status, 'failed')
    assert.deepEqual(store.get().prefs, DEFAULTS)
    assert.ok(store.get().readError)
  }
})

test('1. a value the route would not take is the default, key by key; the rest are read', async () => {
  const bad = doc({ face: 'technical', density: 'spacious', notifications: { ready: true, not_backed: 'sometimes', agent_waiting: true } })
  const store = createPreferences({ fetchImpl: server({ get: () => ({ ok: true, status: 200, json: async () => bad }) }).fetchImpl })
  await store.load()
  assert.equal(store.get().prefs.face, 'technical')
  assert.equal(store.get().prefs.density, 'comfortable')
  assert.deepEqual(store.get().prefs.notifications, { ready: true, not_backed: false, agent_waiting: true, recording_stopped: false })
})

test('2. a write changes nothing until the server has kept it, then the answer is what is shown', async () => {
  const s = server()
  let release
  const gate = new Promise((r) => { release = r })
  const store = createPreferences({ fetchImpl: async (u, o = {}) => { if (o.method === 'POST') await gate; return s.fetchImpl(u, o) } })
  await store.load()
  const done = store.set('face', 'technical')
  assert.equal(store.get().prefs.face, 'plain', 'not shown as kept while the request is out')
  assert.deepEqual(store.get().pending, { face: 'technical' })
  release()
  assert.equal(await done, true)
  assert.equal(store.get().prefs.face, 'technical')
  assert.deepEqual(store.get().pending, {})
  assert.deepEqual(store.get().writeError, {})
})

test('3. one key, one notification switch, a subset: each is ONE request, and the body names only what changes', async () => {
  const s = server()
  const store = createPreferences({ fetchImpl: s.fetchImpl })
  await store.load()
  await store.set('density', 'compact')
  await store.set('masked_copy', false)
  await store.set('notifications.not_backed', true)
  await store.setMany({ face: 'technical', colour_vision: 'proven_teal', notifications: { agent_waiting: true, ready: true } })
  const posts = s.calls.filter((c) => c.method === 'POST')
  assert.deepEqual(posts.map((c) => c.body), [
    '{"density":"compact"}',
    '{"masked_copy":false}',
    '{"notifications":{"not_backed":true}}',
    '{"face":"technical","colour_vision":"proven_teal","notifications":{"agent_waiting":true,"ready":true}}',
  ])
  for (const p of posts) {
    assert.equal(p.url, '/api/preferences')
    assert.equal(p.headers['Content-Type'], 'application/json')
  }
  const p = store.get().prefs
  assert.deepEqual([p.density, p.masked_copy, p.face, p.colour_vision, p.notifications], ['compact', false, 'technical', 'proven_teal', { ready: true, not_backed: true, agent_waiting: true, recording_stopped: false }])
})

test('3. the stored value reads back through valueOf, for a flat key and a notification switch', () => {
  assert.equal(valueOf(DEFAULTS, 'face'), 'plain')
  assert.equal(valueOf(DEFAULTS, 'notifications.not_backed'), false)
  assert.equal(valueOf(null, 'face'), undefined)
})

const REFUSALS = [
  ['a newer file (409 preferences_file_newer)', refusal(409, { error: 'preferences_file_newer' }), /newer ThruLine wrote your preferences file/],
  ['a file that is not an ordinary one (409 preferences_file_not_regular)', refusal(409, { error: 'preferences_file_not_regular' }), /not an ordinary file/],
  ['a value it does not take (400 invalid_value)', refusal(400, { error: 'invalid_value', key: 'face', allowed: ['plain', 'technical'] }), /does not know that choice/],
  ['a key it does not know (400 unknown_preference)', refusal(400, { error: 'unknown_preference', key: 'theme' }), /does not know that choice/],
  ['a request from another page (403 same_origin_required)', refusal(403, { error: 'same_origin_required' }), /only from its own page/],
  ['a body too large (413 request_too_large)', refusal(413, { error: 'request_too_large' }), /too large/],
  ['a body that did not arrive whole (400 malformed_request)', refusal(400, { error: 'malformed_request' }), /did not reach ThruLine whole/],
  ['a write that failed (500 write_failed)', refusal(500, { error: 'write_failed', detail: 'disk full' }), /could not be written/],
  ['a route an older server does not have (404)', refusal(404, {}), /answered 404/],
  ['no answer at all', () => { throw new Error('connection refused') }, /did not answer/],
  ['an answer that does not show the choice kept', () => ({ ok: true, status: 200, json: async () => doc() }), /did not show the choice kept/],
]

for (const [what, post, why] of REFUSALS) {
  test('4. ' + what + ' leaves the value as it was, says why in plain words, and is not hidden', async () => {
    const s = server({ post })
    const store = createPreferences({ fetchImpl: s.fetchImpl })
    await store.load()
    const kept = await store.set('face', 'technical')
    assert.equal(kept, false)
    assert.equal(store.get().prefs.face, 'plain', 'a refused choice is not drawn as kept')
    assert.deepEqual(store.get().pending, {})
    const err = store.get().writeError.face
    assert.deepEqual(err.value, 'technical')
    assert.match(err.why, why)
    assert.doesNotMatch(err.why, FORBIDDEN)
    // the sentence every screen says, from the one list of words
    const line = refusedLine('face', err, store.get().prefs.face)
    assert.match(line, /^ThruLine could not keep “Technical”: /)
    assert.ok(line.endsWith('Face is still Plain.'), line)
    assert.doesNotMatch(line, FORBIDDEN)
  })
}

test('4. a refusal on a notification switch names the switch’s own choice, and the next write of it clears the refusal', async () => {
  let refuse = true
  const s = server({ post: () => (refuse ? { ok: false, status: 409, json: async () => ({ error: 'preferences_file_newer' }) } : null) })
  const store = createPreferences({ fetchImpl: s.fetchImpl })
  await store.load()
  assert.equal(await store.set('notifications.not_backed', true), false)
  assert.equal(store.get().prefs.notifications.not_backed, false)
  assert.ok(store.get().writeError['notifications.not_backed'])
  refuse = false
  assert.equal(await store.set('notifications.not_backed', true), true)
  assert.equal(store.get().prefs.notifications.not_backed, true)
  assert.deepEqual(store.get().writeError, {})
})

test('4. dismiss stops saying a refusal, and says nothing for a choice that was not refused', async () => {
  const store = createPreferences({ fetchImpl: server({ post: refusal(409, { error: 'preferences_file_newer' }) }).fetchImpl })
  await store.load()
  await store.set('density', 'compact')
  assert.ok(store.get().writeError.density)
  store.dismiss('face')
  assert.ok(store.get().writeError.density)
  store.dismiss('density')
  assert.deepEqual(store.get().writeError, {})
})

test('2. a choice already being written is not written twice, and another choice is not held up by it', async () => {
  const s = server()
  let release
  const gate = new Promise((r) => { release = r })
  const store = createPreferences({ fetchImpl: async (u, o = {}) => { if (o.method === 'POST' && JSON.parse(o.body).face) await gate; return s.fetchImpl(u, o) } })
  await store.load()
  const first = store.set('face', 'technical')
  assert.equal(await store.set('face', 'plain'), false, 'the same choice, while it is out')
  assert.equal(await store.set('density', 'compact'), true, 'another choice')
  release()
  assert.equal(await first, true)
  assert.equal(s.calls.filter((c) => c.method === 'POST').length, 2)
})

test('5. a choice ThruLine does not offer never reaches the server', async () => {
  const s = server()
  const store = createPreferences({ fetchImpl: s.fetchImpl })
  await store.load()
  for (const [name, value] of [['theme', 'dark'], ['face', 'loud'], ['density', 1], ['masked_copy', 'yes'], ['notifications.perm', true], ['notifications.not_backed', 'on'], ['notifications', { not_backed: false }]]) {
    await assert.rejects(store.set(name, value), /Not a choice ThruLine offers/, name)
  }
  await assert.rejects(store.setMany({}), /No choice to keep/)
  await assert.rejects(store.setMany({ face: 'technical', theme: 'dark' }), /Not a choice ThruLine offers/)
  assert.equal(s.calls.filter((c) => c.method === 'POST').length, 0)
})

test('5. every name the store takes has values, and the permission-prompt row is not one of them', () => {
  assert.deepEqual([...NAMES].sort(), ['colour_vision', 'density', 'face', 'keys', 'masked_copy', 'motion', 'notifications.agent_waiting', 'notifications.not_backed', 'notifications.ready', 'notifications.recording_stopped', 'weekly_summary'])
  assert.equal(allowed('notifications.perm', true), false)
  assert.equal(allowed('face', 'technical'), true)
  assert.equal(allowed('weekly_summary', false), true)
})

test('the words: what a choice is called, said once for every screen, with no forbidden word and no home folder', () => {
  assert.equal(valueWords('face', 'technical'), 'Technical')
  assert.equal(valueWords('motion', 'reduce'), 'Always reduce')
  assert.equal(valueWords('colour_vision', 'proven_teal'), 'Proven is teal')
  assert.equal(valueWords('masked_copy', false), 'Off')
  assert.equal(keepingLine('density', 'compact'), 'Keeping “Compact” on this machine…')
  assert.equal(stillLine('weekly_summary', false), 'The weekly summary is still Off.')
  for (const f of ['newer_schema', 'not_a_regular_file', 'not_json', 'unrecognised', 'unreadable']) assert.doesNotMatch(fileWords(f), FORBIDDEN, f)
  assert.equal(fileWords('read'), null)
  assert.equal(fileWords('absent'), null)
  for (const t of [NOT_READ, READING]) assert.doesNotMatch(t, FORBIDDEN)
})
