import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readyEvents, waitingEvents, findingEvents, recordingState, readNotificationFacts, NOT_BACKED_FEED } from './notification-events.js'

test('absence alone never announces; a ready unapproved version and an actual speaker finding each have one stable item', () => {
  const change = { id: 'c1', decisions: { standing: null } }
  const list = { changes: [change] }
  assert.deepEqual(readyEvents(list, { c1: { readiness: { ready: false, headline_slug: 'readiness_missing_evidence' } } }), [])
  assert.deepEqual(findingEvents({ finding_items: [] }), [])
  const ready = { readiness: { ready: true, with_exceptions: false, headline_slug: 'readiness_ready', target_version: 'snap-123456789012' } }
  const event = readyEvents(list, { c1: ready })
  assert.equal(event.length, 1)
  assert.equal(event[0].key, 'ready:c1:snap-123456789012')
  assert.deepEqual(readyEvents({ changes: [{ ...change, decisions: { standing: { target_revision: 'snap-123456789012' } } }] }, { c1: ready }), [])
  assert.deepEqual(findingEvents({ finding_items: [{ id: 'spkf_a', recorded_at: '2026-10-09T00:00:00Z' }] }).map((x) => x.key), ['not_backed:spkf_a'])
})

test('agent question uses the recorded move basis and receipt time; no question means no event', () => {
  const project = { id: 'p1' }
  const view = { project, threads: [
    { id: 't1', whose_move: 'you', whose_move_basis: 'agent_asked', waiting_event_id: 'e-question', last_activity_at: '2026-10-09T10:00:00Z' },
    { id: 't2', whose_move: 'you', whose_move_basis: 'claim_without_passing_test' },
    { id: 't3', whose_move: 'you', whose_move_basis: 'agent_asked', last_activity_at: '2026-10-09T10:00:00Z' },
  ] }
  assert.deepEqual(waitingEvents(project, view).map((x) => x.key), ['agent_waiting:p1:e-question'])
  assert.deepEqual(waitingEvents({ id: 'other' }, view), [])
})

test('recorder stops only with a prior recorded source and an explicit stopped state', () => {
  const base = { project: { id: 'p1' }, claude_code_hooks: { installed: true, alive: true }, record: { through_hooks: { deliveries: 2 }, sessions: { codex: 0 } }, codex: {}, holds: { paused: false } }
  assert.deepEqual(recordingState(base), { project: 'p1', active: true, stopped: false, stamp: '' })
  assert.equal(recordingState({ ...base, holds: { paused: true, paused_at: '2026-10-09T10:00:00Z' } }).stopped, true)
  assert.equal(recordingState({ ...base, claude_code_hooks: { installed: true, alive: true }, record: { through_hooks: { deliveries: 0 }, sessions: { codex: 0 } } }).active, false)
  assert.equal(recordingState(null), null)
})

test('read pass asks only for enabled families and never turns a failed GET into an item', async () => {
  const asked = []
  const get = async (url) => { asked.push(url); if (url === NOT_BACKED_FEED) throw Error('unread'); if (url === '/api/hq/projects/p1') return { project: { id: 'p1' }, threads: [] }; throw Error('unexpected') }
  const facts = await readNotificationFacts({ ready: false, not_backed: true, agent_waiting: true, recording_stopped: false }, get, { projectId: 'p1' })
  assert.deepEqual(asked.sort(), ['/api/hq/projects/p1', NOT_BACKED_FEED].sort())
  assert.deepEqual(facts.events, [])
  assert.deepEqual(facts.errors, ['findings'])
  assert.deepEqual(facts.complete, { ready: false, agent_waiting: true })
})

test('ready polling stays inside the open HQ project’s ledger members', async () => {
  const asked = []
  const get = async (url) => {
    asked.push(url)
    if (url === '/api/hq/projects/p1') return { project: { id: 'p1', members: [{ ledger_project_id: 'lp1' }] }, threads: [] }
    if (url === '/api/changes?project=lp1') return { changes: [{ id: 'c1', decisions: { standing: null } }] }
    if (url === '/api/changes/c1/readiness') return { readiness: { ready: false } }
    throw Error('other project was read')
  }
  const facts = await readNotificationFacts({ ready: true, not_backed: false, agent_waiting: false, recording_stopped: false }, get, { projectId: 'p1' })
  assert.deepEqual(asked, ['/api/hq/projects/p1', '/api/changes?project=lp1', '/api/changes/c1/readiness'])
  assert.deepEqual(facts.events, [])
  assert.deepEqual(facts.complete, { ready: true, agent_waiting: false })
})

test('a failed readiness leaves the ready baseline unread until a complete project pass', async () => {
  const get = async (url) => {
    if (url === '/api/hq/projects/p1') return { project: { id: 'p1', members: [{ ledger_project_id: 'lp1' }] }, threads: [] }
    if (url === '/api/changes?project=lp1') return { changes: [{ id: 'c1' }] }
    throw Error('readiness unread')
  }
  const facts = await readNotificationFacts({ ready: true }, get, { projectId: 'p1' })
  assert.deepEqual(facts.complete, { ready: false, agent_waiting: false })
  assert.deepEqual(facts.errors, ['readiness:c1'])
})
