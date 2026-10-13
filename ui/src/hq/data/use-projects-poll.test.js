/* The projects list is re-read while it holds quick rows and stops at
 * `complete: true` — proved by mounting the hook in jsdom over a stubbed
 * fetch. Nothing here reaches a server. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { JSDOM } from 'jsdom'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>')
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const React = (await import('react')).default
const { createRoot } = await import('react-dom/client')
const { useProjects } = await import('./use-project.js')

/* A clock the test drives: an interval fires only when the test says so, so
 * the assertion counts reads by ticks, never by wall-clock (no sleeps under load). */
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
/** Let React and the stubbed fetch settle: yield until `cond()` holds (a bounded number of turns, not a time). */
async function settle(cond = () => false, turns = 500) {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}

test('useProjects re-reads /api/hq/projects until complete, then stops', async () => {
  const answers = [
    { schema_version: 1, complete: false, projects_complete: 0, projects: [{ id: 'a', complete: false, threads: null }] },
    { schema_version: 1, complete: false, projects_complete: 0, projects: [{ id: 'a', complete: false, threads: null }] },
    { schema_version: 1, complete: true, projects_complete: 1, projects: [{ id: 'a', complete: true, threads: 3 }] },
  ]
  const calls = []
  globalThis.fetch = async (url) => {
    calls.push(url)
    const body = answers[Math.min(calls.length - 1, answers.length - 1)]
    return { ok: true, status: 200, text: async () => JSON.stringify(body) }
  }
  const clock = fakeTimers()
  let seen = null
  function Probe() {
    const list = useProjects({ every: 25, timers: clock })
    seen = list.data
    return null
  }
  const root = createRoot(document.getElementById('root'))
  root.render(React.createElement(Probe))
  try {
    // read 1, at mount; quick rows → the re-read schedule starts
    await settle(() => calls.length === 1 && seen && clock.active().length === 1)
    assert.equal(calls.length, 1)
    assert.equal(clock.active().length, 1, 'quick rows: a re-read is scheduled')
    assert.equal(clock.active()[0].ms, 25)
    // tick → read 2 (still quick)
    clock.fire()
    await settle(() => calls.length === 2 && seen && clock.active().length === 1)
    assert.equal(calls.length, 2)
    // tick → read 3: complete → the schedule stops
    clock.fire()
    await settle(() => seen && seen.complete && clock.active().length === 0)
    assert.equal(seen && seen.complete, true)
    assert.equal(clock.active().length, 0, 'no re-read is scheduled once complete')
    const atComplete = calls.length
    clock.fire() // nothing is scheduled, so nothing fires
    await settle()
    assert.equal(calls.length, atComplete, 'no read after complete')
    assert.equal(atComplete, 3)
    assert.ok(calls.every((u) => u === '/api/hq/projects'))
  } finally {
    root.unmount()
  }
})
