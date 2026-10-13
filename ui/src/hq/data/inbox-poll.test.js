/* The Inbox re-reads its documents while something is being recorded, and only then:
 * every 5 seconds, not while the page is hidden, never two reads at once, and a re-read that
 * fails keeps the rows on screen and says so. Proved by mounting `useInbox` in jsdom over a
 * stubbed fetch and a clock the test drives. Nothing here reaches a server. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { JSDOM } from 'jsdom'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>')
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const React = (await import('react')).default
const { createRoot } = await import('react-dom/client')
const { useInbox, REFRESH_FAILED } = await import('./use-project.js')
const { isRecording, inboxRereadEvery, RECORDING_REREAD_MS } = await import('./project.js')
const { VIEW, LIST, at } = await import('./project-fixture.js')

const clone = (x) => JSON.parse(JSON.stringify(x))
/** The project's view as the engine would serve it with no agent working. */
const quiet = () => {
  const v = clone(VIEW)
  v.threads = v.threads.filter((t) => t.whose_move !== 'agent')
  v.project.whose_move = { you: 3, agent: 0, done: 2 }
  return v
}

test('isRecording: a thread that is the agent’s move, or the project’s own count of them', () => {
  assert.equal(isRecording(VIEW), true)
  assert.equal(isRecording(quiet()), false)
  assert.equal(isRecording({ project: { whose_move: { you: 0, agent: 2, done: 0 } }, threads: [] }), true, 'the count alone')
  assert.equal(isRecording({ project: {}, threads: [{ whose_move: 'agent' }] }), true, 'a thread alone')
  for (const nothing of [null, undefined, {}, { project: { whose_move: { agent: 0 } }, threads: [] }]) assert.equal(isRecording(nothing), false)
})

test('inboxRereadEvery: 3 s while a load runs, else 5 s while recording (the view, or this machine), else never', () => {
  const loading = { running: true, project: 'p1' }
  assert.equal(inboxRereadEvery(loading, 'p1', quiet()), 3000, 'a load of this project keeps its pace')
  assert.equal(inboxRereadEvery({ running: false }, 'p1', VIEW), RECORDING_REREAD_MS)
  assert.equal(RECORDING_REREAD_MS, 5000)
  assert.equal(inboxRereadEvery(null, 'p1', quiet()), undefined)
  assert.equal(inboxRereadEvery(null, 'p1', null), undefined, 'nothing read yet: nothing to follow')
  assert.equal(inboxRereadEvery(null, 'p1', quiet(), true), 5000, 'an agent session on this machine was just written to')
})

/* ---- the hook, mounted ---- */
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
const visibleDoc = { visibilityState: 'visible', addEventListener() {}, removeEventListener() {} }
async function settle(cond = () => false, turns = 800) {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}

/** A server that answers from `world`, and counts what was asked. */
function serve(world) {
  const asked = []
  globalThis.fetch = async (url) => {
    asked.push(url)
    const answer = async () => {
      if (url === '/api/hq/projects') return { status: 200, body: { schema_version: 1, complete: true, projects: [world.view.project] } }
      if (url === '/api/hq/projects/p1') return world.projectStatus && world.projectStatus !== 200 ? { status: world.projectStatus, body: { error: 'x' } } : { status: 200, body: world.view }
      if (url === '/api/hq/projects/p1/inbox-hides') return { status: 200, body: { project: 'p1', rows: [] } }
      if (url === '/api/storage') return { status: 200, body: { bytes: 4096 } }
      if (url === '/api/hq/load') return { status: 200, body: { running: false } }
      if (url === '/api/changes') return { status: 200, body: { changes: [] } }
      return { status: 404, body: { error: 'no_such_thing' } }
    }
    if (world.hold && url === '/api/hq/projects/p1') await world.hold // a slow read: not answered until the test lets it
    const { status, body } = await answer()
    return { ok: status === 200, status, text: async () => JSON.stringify(body) }
  }
  return asked
}
const projectReads = (asked) => asked.filter((u) => u === '/api/hq/projects/p1').length

function mount(opts) {
  const seen = { out: null }
  function Probe() {
    seen.out = useInbox('p1', opts)
    return null
  }
  const root = createRoot(document.getElementById('root'))
  root.render(React.createElement(Probe))
  return { seen, unmount: () => root.unmount() }
}

test('the Inbox polls its project every 5 s while an agent is recorded, and the rows move without a reload', async () => {
  const world = { view: clone(VIEW) }
  const asked = serve(world)
  const clock = fakeTimers()
  const m = mount({ timers: clock, doc: visibleDoc })
  try {
    await settle(() => m.seen.out && m.seen.out.data && clock.active().length === 2)
    assert.equal(clock.active()[0].ms, 5000, 'recording: polled every 5 s')
    assert.deepEqual(m.seen.out.data.sessions.map((s) => s.change), ['live1'])
    const first = projectReads(asked)

    // the agent stopped: the next poll moves the row out of "Recording now"
    world.view = quiet()
    clock.fire()
    await settle(() => m.seen.out.data.sessions.length === 0)
    assert.equal(projectReads(asked), first + 1)
    assert.deepEqual(m.seen.out.data.sessions, [], 'the recording row left without a reload')

    // and with nothing recording it stops polling
    await settle(() => clock.active().length === 0)
    assert.equal(clock.active().length, 0, 'nothing is recording: no schedule')
    clock.fire()
    await settle()
    assert.equal(projectReads(asked), first + 1, 'no read follows')
  } finally { m.unmount() }
})

test('with nothing recorded the Inbox reads once; an agent session elsewhere on this machine starts the polling', async () => {
  const world = { view: quiet() }
  const asked = serve(world)
  const clock = fakeTimers()
  const m = mount({ timers: clock, doc: visibleDoc, machineRecording: false })
  try {
    await settle(() => m.seen.out && m.seen.out.data)
    await settle()
    assert.equal(clock.active().length, 0)
    assert.equal(projectReads(asked), 1)
  } finally { m.unmount() }
  const asked2 = serve({ view: quiet() })
  const clock2 = fakeTimers()
  const m2 = mount({ timers: clock2, doc: visibleDoc, machineRecording: true })
  try {
    await settle(() => m2.seen.out && m2.seen.out.data && clock2.active().length === 1)
    assert.equal(clock2.active()[0].ms, 5000)
    clock2.fire()
    await settle(() => projectReads(asked2) === 2)
    assert.equal(projectReads(asked2), 2)
  } finally { m2.unmount() }
})

test('a slow read is never overlapped by the next tick', async () => {
  let release
  const world = { view: clone(VIEW) }
  const asked = serve(world)
  const clock = fakeTimers()
  const m = mount({ timers: clock, doc: visibleDoc })
  try {
    await settle(() => m.seen.out && m.seen.out.data && clock.active().length === 1)
    const before = projectReads(asked)
    world.hold = new Promise((r) => { release = r })
    clock.fire()
    await settle(() => projectReads(asked) === before + 1)
    clock.fire(); clock.fire(); clock.fire()
    await settle()
    assert.equal(projectReads(asked), before + 1, 'three more ticks, still the one read in flight')
    release()
    world.hold = null
    await settle()
    clock.fire()
    await settle(() => projectReads(asked) === before + 2)
    assert.equal(projectReads(asked), before + 2, 'answered: the next tick reads')
  } finally { m.unmount() }
})

test('a failed poll keeps the rows and the time of the last good read, and the header says so', async () => {
  const world = { view: clone(VIEW) }
  serve(world)
  const clock = fakeTimers()
  const m = mount({ timers: clock, doc: visibleDoc })
  try {
    await settle(() => m.seen.out && m.seen.out.data && clock.active().length === 2)
    const good = m.seen.out.data
    assert.equal(good.note, undefined)
    const rows = Object.keys(good.changes)
    assert.equal(rows.length, 4)

    world.projectStatus = 500
    clock.fire()
    await settle(() => m.seen.out.data && m.seen.out.data.note)
    const stale = m.seen.out.data
    assert.equal(stale.note, REFRESH_FAILED)
    assert.deepEqual(Object.keys(stale.changes), rows, 'the rows stay')
    assert.equal(stale.readAt, good.readAt, 'and so does the time they were read')
    assert.equal(m.seen.out.error, null, 'the screen is not blanked for it')
    assert.equal(clock.active().length, 2, 'both the project and hide rows keep trying')

    world.projectStatus = 200
    clock.fire()
    await settle(() => m.seen.out.data && !m.seen.out.data.note)
    assert.equal(m.seen.out.data.note, undefined, 'the next good read clears it')
  } finally { m.unmount() }
})

test('a first read that fails is still the screen’s error: there is nothing to keep', async () => {
  const world = { view: clone(VIEW), projectStatus: 500 }
  serve(world)
  const m = mount({ timers: fakeTimers(), doc: visibleDoc })
  try {
    await settle(() => m.seen.out && m.seen.out.error)
    assert.ok(m.seen.out.error)
    assert.equal(m.seen.out.data, null)
  } finally { m.unmount() }
})

test('the header carries the note beside the time of the read', async () => {
  const { inboxText } = await import('../screens/Inbox/text.js')
  const { inboxData, } = await import('./project.js')
  const D = inboxData({ projects: LIST, view: VIEW, now: new Date(at(16, 10)) })
  assert.equal(inboxText(D).hdrRead, 'read 16:10 ↻')
  assert.equal(inboxText({ ...D, note: REFRESH_FAILED }).hdrRead, 'read 16:10 ↻ · could not refresh just now')
  assert.doesNotMatch(inboxText({ ...D, note: REFRESH_FAILED }).hdrRead, /lie|cheat|violate|ignore|hallucinat|trial|%/i)
})
