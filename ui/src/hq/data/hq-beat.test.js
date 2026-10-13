/* Settings, census settings#3 (HQ stopped): the page notices that HQ itself has gone away.
 *
 * DESIGN-SPEC §3.10 "System-state bar conditions" (stopped) draws "HQ stopped." when HQ stops, and
 * "read HH:MM · not refreshing" in the read stamp. The live page hard-coded `hq.stopped = false`
 * and re-read only on a 60 s timer (lanes), so a stopped server was never shown. HQ is stopped when a
 * read gets NO answer at all (the browser could not reach it) or a gateway answers 502/503/504 (the census's
 * front, with nothing behind it); any other HTTP error is HQ answering badly, which is not stopped.
 * Synthetic fakes only: `read` and the clock are stubs. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { JSDOM } from 'jsdom'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>')
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const React = (await import('react')).default
const { createRoot } = await import('react-dom/client')
const { readJson } = await import('./api.js')
const B = await import('./hq-beat.js')

async function settle(cond = () => false, turns = 500) {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}
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
// a stubbed document: a visibility state and the visibilitychange event, nothing else
function fakeDoc(state = 'visible') {
  const doc = new EventTarget()
  doc.visibilityState = state
  doc.set = (next) => { doc.visibilityState = next; doc.dispatchEvent(new Event('visibilitychange')) }
  return doc
}
const gone = () => Object.assign(new Error('ThruLine’s local server did not answer (/api/state).'), { unreachable: true })

test('a read that got no answer at all is tagged unreachable; an HTTP error and bad JSON are not', async () => {
  const real = globalThis.fetch
  try {
    globalThis.fetch = async () => { throw new TypeError('Failed to fetch') }
    const none = await readJson('/api/state').then(() => null, (e) => e)
    assert.equal(none.unreachable, true, 'the browser reached nobody')
    assert.match(none.message, /did not answer/, 'the sentence a person reads is unchanged')
    globalThis.fetch = async () => ({ ok: false, status: 500, text: async () => 'boom' })
    const bad = await readJson('/api/state').then(() => null, (e) => e)
    assert.notEqual(bad.unreachable, true, 'HQ answered, badly: it is not stopped')
    globalThis.fetch = async () => ({ ok: true, status: 200, text: async () => '<html>' })
    const html = await readJson('/api/state').then(() => null, (e) => e)
    assert.notEqual(html.unreachable, true)
  } finally {
    globalThis.fetch = real
  }
})

test('hqUnreachable is true for an error tagged unreachable and for a gateway status; any other answer is not stopped', () => {
  assert.equal(B.hqUnreachable(gone()), true)
  assert.equal(B.hqUnreachable(new Error('/api/state answered 500')), false)
  for (const status of [400, 401, 403, 404, 500, 501]) assert.equal(B.hqUnreachable(Object.assign(new Error('x'), { status })), false, status + ' is HQ answering')
  for (const status of [502, 503, 504]) assert.equal(B.hqUnreachable(Object.assign(new Error('x'), { status })), true, status + ' is a gateway with nothing behind it')
  assert.equal(B.hqUnreachable(null), false)
  assert.equal(B.hqUnreachable(undefined), false)
})

function mount(answers, doc = fakeDoc()) {
  const reads = []
  const read = (url) => {
    reads.push(url)
    const a = answers.length > 1 ? answers.shift() : answers[0]
    return a instanceof Error ? Promise.reject(a) : Promise.resolve(a)
  }
  const clock = fakeTimers()
  const seen = { stopped: null }
  function Probe() {
    seen.stopped = B.useHqStopped({ read, timers: clock, doc })
    return null
  }
  const root = createRoot(document.createElement('div'))
  root.render(React.createElement(Probe))
  return { reads, clock, seen, root, doc }
}

test('useHqStopped: a quick heartbeat on the cheap in-memory state route, and the page is not stopped until a read gets no answer', async () => {
  const h = mount([{ ok: true }, gone(), gone(), { ok: true }])
  try {
    await settle(() => h.clock.active().length === 1)
    assert.equal(h.seen.stopped, false, 'a page that just opened is not stopped')
    assert.equal(h.reads.length, 0, 'no extra read at load: the page was just served by HQ')
    assert.equal(h.clock.active()[0].ms, B.HQ_BEAT_MS)
    assert.ok(B.HQ_BEAT_MS <= 5000, 'a stop shows within a few seconds, not on the 60 s lanes timer')
    h.clock.fire()
    await settle(() => h.reads.length === 1)
    assert.deepEqual(h.reads, ['/api/state'])
    await settle(() => false, 20)
    assert.equal(h.seen.stopped, false, 'HQ answered')
    h.clock.fire()
    await settle(() => h.seen.stopped === true)
    assert.equal(h.seen.stopped, true, 'no answer: HQ stopped')
    h.clock.fire()
    await settle(() => false, 20)
    assert.equal(h.seen.stopped, true, 'still gone')
    h.clock.fire()
    await settle(() => h.seen.stopped === false)
    assert.equal(h.seen.stopped, false, 'it answers again: the bar clears')
  } finally {
    h.root.unmount()
  }
  assert.equal(h.clock.active().length, 0, 'the heartbeat stops when the screen is left')
})

test('readJson puts the HTTP status on the error it throws, and a gateway status reads as no answer from HQ', async () => {
  const real = globalThis.fetch
  try {
    // a front that proxies /api/ to HQ (the census's Node front) answers 502 once HQ is gone
    globalThis.fetch = async () => ({ ok: false, status: 502, text: async () => 'Error: connect ECONNREFUSED 127.0.0.1:1' })
    const gateway = await readJson('/api/state').then(() => null, (e) => e)
    assert.equal(gateway.status, 502)
    assert.equal(B.hqUnreachable(gateway), true)
    globalThis.fetch = async () => ({ ok: false, status: 500, text: async () => 'boom' })
    const bad = await readJson('/api/state').then(() => null, (e) => e)
    assert.equal(bad.status, 500)
    assert.equal(B.hqUnreachable(bad), false)
    globalThis.fetch = async () => { throw new TypeError('Failed to fetch') }
    const none = await readJson('/api/state').then(() => null, (e) => e)
    assert.equal(B.hqUnreachable(none), true, 'a network failure still reads as stopped')
  } finally {
    globalThis.fetch = real
  }
})

test('useHqStopped: a 502 from a front whose HQ is gone sets stopped, and the next good read clears it', async () => {
  const h = mount([{ ok: true }, Object.assign(new Error('/api/state answered 502: connect ECONNREFUSED'), { status: 502 }), { ok: true }])
  try {
    await settle(() => h.clock.active().length === 1)
    h.clock.fire()
    await settle(() => false, 20)
    assert.equal(h.seen.stopped, false)
    h.clock.fire()
    await settle(() => h.seen.stopped === true)
    assert.equal(h.seen.stopped, true, '502: nothing is behind the gateway')
    h.clock.fire()
    await settle(() => h.seen.stopped === false)
    assert.equal(h.seen.stopped, false)
  } finally {
    h.root.unmount()
  }
})

test('useHqStopped: an HQ that answers with an error is not a stopped HQ', async () => {
  const h = mount([new Error('/api/state answered 500')])
  try {
    await settle(() => h.clock.active().length === 1)
    h.clock.fire()
    await settle(() => h.reads.length === 1)
    await settle(() => false, 30)
    assert.equal(h.seen.stopped, false)
  } finally {
    h.root.unmount()
  }
})

// CTO ruling 2026-10-12 17:20Z: the heartbeat runs only while Settings is mounted AND the page is visible.

test('useHqStopped: no interval and no read while the page is hidden; one read at once on return to visible, then the beat resumes', async () => {
  const h = mount([{ ok: true }], fakeDoc('hidden'))
  try {
    await settle(() => false, 20)
    assert.equal(h.clock.active().length, 0, 'opened hidden: nothing is scheduled')
    assert.equal(h.reads.length, 0)
    h.doc.set('visible')
    await settle(() => h.reads.length === 1)
    assert.deepEqual(h.reads, ['/api/state'], 'one read at once, not on the next tick')
    assert.equal(h.clock.active().length, 1, 'and the beat resumes at the same cadence')
    assert.equal(h.clock.active()[0].ms, B.HQ_BEAT_MS)
    h.clock.fire()
    await settle(() => h.reads.length === 2)
    assert.equal(h.reads.length, 2)
    h.doc.set('hidden')
    assert.equal(h.clock.active().length, 0, 'hidden again: the interval is cleared')
    h.clock.fire()
    await settle(() => false, 20)
    assert.equal(h.reads.length, 2, 'no request while hidden')
    h.doc.set('visible')
    await settle(() => h.reads.length === 3)
    assert.equal(h.reads.length, 3, 'one read on return')
    await settle(() => false, 20)
    assert.equal(h.reads.length, 3, 'only one')
    assert.equal(h.clock.active().length, 1)
  } finally {
    h.root.unmount()
  }
})

test('useHqStopped: the stopped answer is unchanged by visibility: a read on return that gets no answer sets stopped, a good one clears it', async () => {
  const h = mount([gone(), { ok: true }])
  try {
    await settle(() => h.clock.active().length === 1)
    h.doc.set('hidden')
    h.doc.set('visible')
    await settle(() => h.seen.stopped === true)
    assert.equal(h.seen.stopped, true, 'HQ went away while the tab was hidden: the return read says so')
    h.doc.set('hidden')
    h.doc.set('visible')
    await settle(() => h.seen.stopped === false)
    assert.equal(h.seen.stopped, false)
  } finally {
    h.root.unmount()
  }
})

test('useHqStopped: nothing is read after unmount, whether the page was visible or hidden, and the listener is gone', async () => {
  const shown = mount([{ ok: true }])
  await settle(() => shown.clock.active().length === 1)
  shown.root.unmount()
  await settle(() => shown.clock.active().length === 0)
  assert.equal(shown.clock.active().length, 0, 'unmount clears the interval')
  shown.doc.set('hidden')
  shown.doc.set('visible')
  await settle(() => false, 20)
  assert.equal(shown.reads.length, 0, 'a visibilitychange after unmount reads nothing and restarts nothing')
  assert.equal(shown.clock.active().length, 0)

  const hid = mount([{ ok: true }], fakeDoc('hidden'))
  await settle(() => false, 20)
  hid.root.unmount()
  await settle(() => false, 20)
  hid.doc.set('visible')
  await settle(() => false, 20)
  assert.equal(hid.reads.length, 0, 'unmounted while hidden: the return to visible reads nothing')
  assert.equal(hid.clock.active().length, 0)
})

test('useHqStopped: with no document passed it follows the real document.visibilityState', async () => {
  const set = (state) => {
    Object.defineProperty(document, 'visibilityState', { value: state, configurable: true })
    document.dispatchEvent(new window.Event('visibilitychange'))
  }
  const reads = []
  const read = (url) => { reads.push(url); return Promise.resolve({ ok: true }) }
  const clock = fakeTimers()
  function Probe() {
    B.useHqStopped({ read, timers: clock })
    return null
  }
  Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true })
  const root = createRoot(document.createElement('div'))
  try {
    root.render(React.createElement(Probe))
    await settle(() => false, 20)
    assert.equal(clock.active().length, 0, 'the real document says hidden: no interval')
    set('visible')
    await settle(() => reads.length === 1)
    assert.equal(reads.length, 1)
    assert.equal(clock.active().length, 1)
  } finally {
    root.unmount()
    delete document.visibilityState
  }
  assert.equal(clock.active().length, 0)
})
