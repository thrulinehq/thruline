/* C30 R-ROUTES "Capture health", the client (data/capture.js): one read of
 * `GET /api/hq/projects/<id>/capture`, the facts a screen draws from it, and
 * the hook that re-reads it while First run waits for a first step.
 *
 * Synthetic documents only (../screens/Onboarding/capture-fixture.js); `fetch`
 * and the clock are stubs, so nothing here reaches a server or a store. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { JSDOM } from 'jsdom'
import * as F from '../screens/Onboarding/capture-fixture.js'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>')
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const React = (await import('react')).default
const { createRoot } = await import('react-dom/client')
const C = await import('./capture.js')

/** A fetch that answers one way and records every request. */
function stubFetch(answer) {
  const calls = []
  const f = async (url, init = {}) => {
    calls.push({ url, method: (init.method || 'GET').toUpperCase() })
    return answer(url, init)
  }
  f.calls = calls
  return f
}
const ok = (body) => ({ ok: true, status: 200, text: async () => JSON.stringify(body), json: async () => body })
const status = (code, body) => ({ ok: false, status: code, text: async () => JSON.stringify(body || { error: 'x' }), json: async () => body || { error: 'x' } })

/* ---------------- readCapture ---------------- */

test('readCapture reads the project’s capture address with a GET, and returns the served document', async () => {
  const f = stubFetch(() => ok(F.waiting('p 1')))
  const doc = await C.readCapture('p 1', { fetchImpl: f })
  assert.equal(doc.project.id, 'p 1')
  assert.deepEqual(f.calls, [{ url: '/api/hq/projects/p%201/capture', method: 'GET' }])
  assert.equal(C.CAPTURE_ADDRESS('p 1'), '/api/hq/projects/p%201/capture')
})

test('a failed read rejects with a sentence a person can read, and never one that says installed or not', async () => {
  const cases = [
    ['404', () => status(404, { error: 'no_such_project' })],
    ['503', () => status(503, { error: 'store_unavailable', detail: 'locked' })],
    ['500', () => status(500, { error: 'read_failed' })],
    ['network', () => { throw new TypeError('Failed to fetch') }],
    ['not JSON', () => ({ ok: true, status: 200, text: async () => '<html>', json: async () => { throw new SyntaxError('x') } })],
    ['another project', () => ok(F.waiting('someone-else'))],
    ['another shape', () => ok(Object.assign(F.waiting('p1'), { schema_version: 2 }))],
  ]
  for (const [name, answer] of cases) {
    const err = await C.readCapture('p1', { fetchImpl: stubFetch(answer) }).then(() => null, (e) => e)
    assert.ok(err instanceof Error, name + ': it did not reject')
    assert.match(err.message, /^[A-Z].+[.?]$/, name + ': ' + err.message)
    assert.doesNotMatch(err.message, /install|undefined|null/i, name + ': ' + err.message)
  }
})

/* ---------------- captureFacts: the one reading of the document ---------------- */

test('captureFacts: each document reads as the phase its facts hold', () => {
  const phase = (d) => C.captureFacts(d).phase
  assert.equal(phase(F.waiting()), 'waiting')
  assert.equal(phase(F.recording()), 'recording')
  assert.equal(phase(F.none()), 'none')
  assert.equal(phase(F.problem()), 'problem')
  assert.equal(phase(F.partlyBroken()), 'problem')
  assert.equal(phase(F.unreadable()), 'unknown')
  assert.equal(phase(F.ledgerUnreadable()), 'unknown')
  assert.equal(phase(F.loadedNotHooked()), 'waiting', 'steps read from the recordings are not steps through the hooks')
  assert.equal(C.captureFacts(null), null)
  assert.equal(C.captureFacts({}), null, 'a document with no hooks part is not read as anything')
})

test('captureFacts keeps every unread fact null, with the route’s reason, and never guesses', () => {
  const u = C.captureFacts(F.unreadable())
  assert.equal(u.hooks.installed, null)
  assert.match(u.reason, /could not be read/)
  const l = C.captureFacts(F.ledgerUnreadable())
  assert.equal(l.hooks.installed, true)
  assert.equal(l.record.readable, false)
  assert.equal(l.record.deliveries, null)
  assert.match(l.reason, /record could not be read/)
  const g = C.captureFacts(F.withGit(F.waiting(), { repository: null, toplevel: null, reason: 'git is not installed on this machine' }))
  assert.deepEqual([g.git.repository, g.git.reason], [null, 'git is not installed on this machine'])
  assert.equal(C.captureFacts(F.withGit(F.waiting(), { repository: false, toplevel: null, reason: null })).git.repository, false)
})

test('captureFacts: the hooks’ facts — where a recorder is, how many events, what is missing', () => {
  const w = C.captureFacts(F.waiting())
  assert.equal(w.hooks.events, 8)
  assert.equal(w.hooks.version, '0.1.0')
  assert.deepEqual(w.hooks.where.map((x) => x.scope), ['user'])
  assert.equal(w.firstStepSeen, false)
  const p = C.captureFacts(F.problem())
  assert.deepEqual([p.hooks.missing, p.hooks.commands, p.hooks.recorders, p.hooks.recordersMissing], [1, 1, 1, 1])
  assert.deepEqual(p.hooks.where.map((x) => x.scope), ['project'])
  const pb = C.captureFacts(F.partlyBroken())
  assert.deepEqual([pb.hooks.missing, pb.hooks.commands, pb.hooks.recorders, pb.hooks.recordersMissing], [1, 9, 8, 0])
  const r = C.captureFacts(F.recording())
  assert.equal(r.firstStepSeen, true)
  assert.deepEqual([r.record.deliveries, r.record.firstAt, r.record.latestAt], [1204, F.at(16, 1), F.at(16, 8)])
})

test('captureOf: what a screen holds for one read — reading, failed, or the facts', () => {
  assert.equal(C.captureOf({ doc: null, error: null, reading: false }), null)
  assert.deepEqual(C.captureOf({ doc: null, error: null, reading: true }), { state: 'reading' })
  assert.deepEqual(C.captureOf({ doc: null, error: 'It did not answer.', reading: false }), { state: 'failed', error: 'It did not answer.' })
  const v = C.captureOf({ doc: F.recording(), error: null, reading: false })
  assert.equal(v.state, 'read')
  assert.equal(v.installed, true)
  assert.equal(v.health, 'recording')
  assert.equal(C.captureOf({ doc: F.unreadable(), error: null, reading: false }).installed, false, 'not known is never installed')
})

/* ---------------- useCapture: read, re-read while waiting, stop ---------------- */

function fakeTimers() {
  const live = new Map()
  let id = 0
  return {
    setInterval: (fn, ms) => { live.set(++id, { fn, ms }); return id },
    clearInterval: (t) => { live.delete(t) },
    active: () => [...live.values()],
    fire: () => { for (const t of [...live.values()]) t.fn() },
  }
}
async function settle(cond = () => false, turns = 500) {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}

/** Mount the hook on a scripted reader; `answers` is consumed one per read. */
function mountHook({ id = 'p1', poll = true, every, answers }) {
  const reads = []
  const pending = []
  const read = (pid) => {
    reads.push(pid)
    const a = answers.length > 1 ? answers.shift() : answers[0]
    if (a === 'hold') return new Promise((resolve) => pending.push(resolve))
    return a instanceof Error ? Promise.reject(a) : Promise.resolve(a)
  }
  const clock = fakeTimers()
  const seen = { current: {}, history: [] }
  let setProps = null
  function Probe() {
    const [p, set] = React.useState({ id, poll })
    setProps = set
    seen.current = C.useCapture(p.id, { poll: p.poll, every, timers: clock, read })
    seen.history.push(seen.current)
    return null
  }
  const el = document.createElement('div')
  const root = createRoot(el)
  root.render(React.createElement(Probe))
  return { reads, clock, seen, root, pending, set: (p) => setProps((q) => Object.assign({}, q, p)) }
}

test('useCapture reads once at once, and re-reads only while polled and the record says it is waiting', async () => {
  const h = mountHook({ poll: true, answers: [F.waiting(), F.waiting(), F.recording()] })
  try {
    await settle(() => h.seen.current.facts && h.clock.active().length === 1)
    assert.deepEqual(h.reads, ['p1'])
    assert.equal(h.seen.current.facts.phase, 'waiting')
    assert.equal(h.clock.active().length, 1, 'waiting: a re-read is scheduled')
    h.clock.fire()
    await settle(() => h.reads.length === 2 && !h.seen.current.reading)
    assert.equal(h.seen.current.facts.phase, 'waiting')
    assert.equal(h.clock.active().length, 1)
    h.clock.fire()
    await settle(() => h.seen.current.facts && h.seen.current.facts.phase === 'recording' && h.clock.active().length === 0)
    assert.equal(h.reads.length, 3)
    assert.equal(h.clock.active().length, 0, 'a first step is seen: polling stops')
  } finally {
    h.root.unmount()
  }
})

test('useCapture never polls faster than every 2 s, whatever it is asked', async () => {
  const h = mountHook({ poll: true, every: 250, answers: [F.waiting()] })
  try {
    await settle(() => h.clock.active().length === 1)
    assert.equal(h.clock.active()[0].ms, 2000)
    assert.equal(C.CAPTURE_POLL_FLOOR_MS, 2000)
  } finally {
    h.root.unmount()
  }
})

test('useCapture does not poll when not asked to, nor in any phase but waiting', async () => {
  for (const [poll, doc] of [[false, F.waiting()], [true, F.none()], [true, F.problem()], [true, F.unreadable()], [true, F.recording()]]) {
    const h = mountHook({ poll, answers: [doc] })
    try {
      await settle(() => h.seen.current.facts)
      await settle(() => false, 20)
      assert.equal(h.clock.active().length, 0, JSON.stringify([poll, C.captureFacts(doc).phase]))
    } finally {
      h.root.unmount()
    }
  }
})

test('a tick while a read is still out starts no second read', async () => {
  const h = mountHook({ poll: true, answers: [F.waiting(), 'hold'] })
  try {
    await settle(() => h.clock.active().length === 1)
    h.clock.fire()
    await settle(() => h.reads.length === 2)
    h.clock.fire()
    h.clock.fire()
    await settle(() => false, 30)
    assert.equal(h.reads.length, 2, 'the held read is not doubled')
  } finally {
    h.root.unmount()
  }
})

test('reload (Look again) reads now, polled or not', async () => {
  const h = mountHook({ poll: false, answers: [F.problem(), F.recording()] })
  try {
    await settle(() => h.seen.current.facts)
    assert.equal(h.seen.current.facts.phase, 'problem')
    h.seen.current.reload()
    await settle(() => h.seen.current.facts && h.seen.current.facts.phase === 'recording')
    assert.deepEqual(h.reads, ['p1', 'p1'])
  } finally {
    h.root.unmount()
  }
})

test('reload says it is reading in the render it asks in, never one render later', async () => {
  const h = mountHook({ poll: false, answers: [F.problem(), 'hold'] })
  try {
    await settle(() => h.seen.current.facts)
    const asked = h.seen.history.length
    h.seen.current.reload()
    await settle(() => h.reads.length === 2 && h.seen.current.reading)
    const after = h.seen.history.slice(asked)
    assert.ok(after.length > 0)
    assert.ok(after.every((x) => x.reading === true), 'no render between the ask and the answer is a settled one: ' + JSON.stringify(after.map((x) => x.reading)))
  } finally {
    h.root.unmount()
  }
})

test('a failed read is said, holds no facts, and polls nothing; Look again recovers', async () => {
  const h = mountHook({ poll: true, answers: [new Error('ThruLine’s local server did not answer.'), F.waiting()] })
  try {
    await settle(() => h.seen.current.error)
    assert.equal(h.seen.current.error, 'ThruLine’s local server did not answer.')
    assert.equal(h.seen.current.facts, null)
    assert.equal(h.seen.current.doc, null)
    assert.equal(h.clock.active().length, 0)
    h.seen.current.reload()
    await settle(() => h.seen.current.facts)
    assert.equal(h.seen.current.error, null)
    assert.equal(h.seen.current.facts.phase, 'waiting')
  } finally {
    h.root.unmount()
  }
})

test('a new folder is read on its own; the last folder’s answer is never shown for it', async () => {
  const h = mountHook({ id: 'a', poll: false, answers: [F.waiting('a'), 'hold'] })
  try {
    await settle(() => h.seen.current.facts)
    h.set({ id: 'b' })
    await settle(() => h.reads.length === 2)
    assert.deepEqual(h.reads, ['a', 'b'])
    assert.equal(h.seen.current.project, 'b')
    assert.equal(h.seen.current.facts, null, 'b is not read yet: nothing of a is shown for it')
    assert.ok(!h.seen.history.some((x) => x.project === 'b' && (x.facts || x.doc)), 'not even for the one render before b’s read starts')
    assert.equal(h.seen.current.reading, true)
  } finally {
    h.root.unmount()
  }
})
