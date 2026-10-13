/* usePolled: one address re-read on a timer while it matters (the Inbox while an agent is recorded).
 * Proved by mounting the hook in jsdom over an injected reader and a clock the test drives: a tick
 * fires only when the test says so, so nothing here waits on wall-clock time or reaches a server. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { JSDOM } from 'jsdom'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>')
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const React = (await import('react')).default
const { createRoot } = await import('react-dom/client')
const { usePolled, pageVisible } = await import('./use-polled.js')

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
/** A page whose visibility the test sets, with the listener seam the hook uses. */
function fakeDoc(state = 'visible') {
  const listeners = new Set()
  return {
    visibilityState: state,
    addEventListener: (type, fn) => { if (type === 'visibilitychange') listeners.add(fn) },
    removeEventListener: (type, fn) => { listeners.delete(fn) },
    set(next) { this.visibilityState = next; for (const fn of [...listeners]) fn() },
    listening: () => listeners.size,
  }
}
async function settle(cond = () => false, turns = 500) {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}
/** A reader whose answers the test hands out one at a time (a promise per call). */
function reader() {
  const calls = []
  const read = (url) => new Promise((resolve, reject) => calls.push({ url, resolve, reject }))
  return { read, calls }
}

function mount(opts) {
  const seen = { state: null, renders: 0 }
  function Probe() {
    seen.state = usePolled('/doc', opts)
    seen.renders += 1
    return null
  }
  const root = createRoot(document.getElementById('root'))
  root.render(React.createElement(Probe))
  return { seen, unmount: () => root.unmount() }
}

test('it reads once, and does not poll when nothing asks it to', async () => {
  const clock = fakeTimers()
  const r = reader()
  const m = mount({ read: r.read, timers: clock, doc: fakeDoc(), every: undefined })
  try {
    await settle(() => r.calls.length === 1)
    r.calls[0].resolve({ n: 1 })
    await settle(() => m.seen.state && m.seen.state.data)
    assert.deepEqual(m.seen.state.data, { n: 1 })
    assert.equal(clock.active().length, 0, 'no timer is scheduled')
    clock.fire()
    await settle()
    assert.equal(r.calls.length, 1)
  } finally { m.unmount() }
})

test('it polls while the document says it matters, and stops when the document says it no longer does', async () => {
  const clock = fakeTimers()
  const r = reader()
  // the answer decides whether it keeps being read: 5 s while `recording`, never otherwise
  const m = mount({ read: r.read, timers: clock, doc: fakeDoc(), every: (d) => (d && d.recording ? 5000 : undefined) })
  try {
    await settle(() => r.calls.length === 1)
    r.calls[0].resolve({ recording: true, n: 1 })
    await settle(() => clock.active().length === 1)
    assert.equal(clock.active()[0].ms, 5000)
    clock.fire()
    await settle(() => r.calls.length === 2)
    r.calls[1].resolve({ recording: true, n: 2 })
    await settle(() => m.seen.state.data && m.seen.state.data.n === 2)
    assert.equal(clock.active().length, 1, 'still recording: still polling')
    clock.fire()
    await settle(() => r.calls.length === 3)
    r.calls[2].resolve({ recording: false, n: 3 })
    await settle(() => clock.active().length === 0)
    assert.equal(m.seen.state.data.n, 3, 'the last read moved the rows')
    assert.equal(clock.active().length, 0, 'nothing recording: the schedule stops')
    clock.fire()
    await settle()
    assert.equal(r.calls.length, 3, 'and no read follows')
  } finally { m.unmount() }
})

test('a tick never starts a read while the last one is still running', async () => {
  const clock = fakeTimers()
  const r = reader()
  const m = mount({ read: r.read, timers: clock, doc: fakeDoc(), every: 5000 })
  try {
    await settle(() => r.calls.length === 1)
    clock.fire(); clock.fire(); clock.fire() // the first read has not answered
    await settle()
    assert.equal(r.calls.length, 1, 'three ticks, one read in flight')
    r.calls[0].resolve({ n: 1 })
    await settle(() => m.seen.state.data)
    clock.fire()
    await settle(() => r.calls.length === 2)
    assert.equal(r.calls.length, 2, 'once it answered, the next tick reads')
    clock.fire()
    await settle()
    assert.equal(r.calls.length, 2, 'and that one is in flight too')
  } finally { m.unmount() }
})

test('a hidden page is not read; coming back reads once, at once', async () => {
  const clock = fakeTimers()
  const doc = fakeDoc('visible')
  const r = reader()
  const m = mount({ read: r.read, timers: clock, doc, every: 5000 })
  try {
    await settle(() => r.calls.length === 1)
    r.calls[0].resolve({ n: 1 })
    await settle(() => m.seen.state.data)
    doc.set('hidden')
    clock.fire(); clock.fire()
    await settle()
    assert.equal(r.calls.length, 1, 'hidden: ticks read nothing')
    doc.set('visible')
    await settle(() => r.calls.length === 2)
    assert.equal(r.calls.length, 2, 'back in front: one read now, not at the next tick')
    r.calls[1].resolve({ n: 2 })
    await settle(() => m.seen.state.data.n === 2)
  } finally { m.unmount() }
  assert.equal(doc.listening(), 0, 'the listener goes with the screen')
  assert.equal(clock.active().length, 0)
})

test('a failed poll keeps the last good data and says so; the next good read clears it', async () => {
  const clock = fakeTimers()
  const r = reader()
  const m = mount({ read: r.read, timers: clock, doc: fakeDoc(), every: 5000 })
  try {
    await settle(() => r.calls.length === 1)
    r.calls[0].resolve({ n: 1 })
    await settle(() => m.seen.state.data)
    assert.equal(m.seen.state.error, null)
    clock.fire()
    await settle(() => r.calls.length === 2)
    r.calls[1].reject(new Error('ThruLine’s local server did not answer (/doc).'))
    await settle(() => m.seen.state.error)
    assert.deepEqual(m.seen.state.data, { n: 1 }, 'the last good read is still there')
    assert.match(m.seen.state.error.message, /did not answer/)
    assert.equal(clock.active().length, 1, 'and it keeps trying')
    clock.fire()
    await settle(() => r.calls.length === 3)
    r.calls[2].resolve({ n: 3 })
    await settle(() => m.seen.state.error === null)
    assert.deepEqual(m.seen.state.data, { n: 3 })
  } finally { m.unmount() }
})

test('a first read that fails has no data and says so', async () => {
  const r = reader()
  const m = mount({ read: r.read, timers: fakeTimers(), doc: fakeDoc(), every: undefined })
  try {
    await settle(() => r.calls.length === 1)
    r.calls[0].reject(new Error('nope'))
    await settle(() => m.seen.state.error)
    assert.equal(m.seen.state.data, null)
    assert.equal(m.seen.state.loading, false)
  } finally { m.unmount() }
})

test('a page with no document is in view', () => {
  assert.equal(pageVisible(null), true)
  assert.equal(pageVisible({ visibilityState: 'visible' }), true)
  assert.equal(pageVisible({ visibilityState: 'hidden' }), false)
})
