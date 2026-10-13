/* The four C51 browser events, derived only from served, masked record facts.
 * A missing read never becomes an event. IDs are stable and contain no text.
 * NOT-APPROVAL-WORDS: readyOf files a Change that IS ready for a notification;
 * no notification explains why approval is off. */
import { API, readJson } from './api.js'
import { readyOf } from './inbox-ready.js'
import { href } from '../routes.js'

export const WEEKLY_SUMMARY = '/api/hq/weekly-summary'
export const NOT_BACKED_FEED = '/api/hq/not-backed'
export const NOTIFICATION_POLL_MS = 5000

const rows = (doc, key) => Array.isArray(doc && doc[key]) ? doc[key] : []
const id = (x) => x == null ? '' : String(x)
async function pooled(items, size, read) {
  const out = new Array(items.length)
  let next = 0
  await Promise.all(Array.from({ length: Math.min(size, items.length) }, async () => {
    while (next < items.length) { const i = next++; out[i] = await read(items[i]) }
  }))
  return out
}

export function readyEvents(changes, readinessById) {
  return rows(changes, 'changes').flatMap((change) => {
    if (!change || !change.id) return []
    const ready = readyOf(readinessById[id(change.id)])
    if (!ready || (change.decisions && change.decisions.standing && change.decisions.standing.target_revision === ready.target)) return []
    return [{ key: `ready:${change.id}:${ready.target}`, kind: 'ready', title: 'Your move', body: 'A Change is ready for you to approve.', url: href('approve', { change: change.id }) }]
  })
}

export function waitingEvents(project, view) {
  if (!project || !project.id || !view || !view.project || id(view.project.id) !== id(project.id)) return []
  return rows(view, 'threads').filter((thread) => thread && thread.id && thread.whose_move === 'you' && thread.whose_move_basis === 'agent_asked' && thread.waiting_event_id)
    .map((thread) => ({ key: `agent_waiting:${project.id}:${thread.waiting_event_id}`, kind: 'agent_waiting', title: 'Agent waiting', body: 'An agent is waiting for your answer.', url: href('inbox', { project: project.id }) }))
}

export function findingEvents(summary) {
  return rows(summary, 'finding_items').filter((finding) => finding && finding.id)
    .map((finding) => ({ key: `not_backed:${finding.id}`, kind: 'not_backed', title: 'Not backed', body: 'An agent said the work was done; the record did not back it.', url: href('inbox') }))
}

/* A stopped recorder must be observed after it was active. A lack of new
 * activity is not a stop, nor is a capture read failure. */
export function recordingState(doc) {
  if (!doc || !doc.project || !doc.project.id || !doc.claude_code_hooks || !doc.record) return null
  const hooks = doc.claude_code_hooks
  const codex = doc.codex || {}
  const holds = doc.holds || {}
  const hadHooks = Number(doc.record.through_hooks && doc.record.through_hooks.deliveries) > 0
  const hadCodex = Number(doc.record.sessions && doc.record.sessions.codex) > 0
  const hookActive = hadHooks && hooks.installed === true && hooks.alive === true && holds.paused === false
  const codexActive = hadCodex && codex.watches_this_project === true && codex.state === 'watching' && holds.paused === false
  const stopped = holds.paused === true ||
    (hadHooks && (hooks.installed === false || hooks.alive === false)) ||
    (hadCodex && codex.watches_this_project === true && ['paused', 'stopped', 'disconnected'].includes(codex.state))
  return { project: id(doc.project.id), active: hookActive || codexActive, stopped, stamp: holds.paused_at || (doc.record.latest_step && doc.record.latest_step.at) || '' }
}

/** All enabled event families are read in one pass. One slow/failing address
 * never blocks the others, and no result is inferred for the failed family. */
export async function readNotificationFacts(enabled, get = readJson, { findingAfter = null, projectId = null } = {}) {
  // Project-backed notifications follow only the project currently open in
  // HQ. A global sweep of every project every five seconds is too expensive.
  const wantProjects = !!projectId && (enabled.agent_waiting || enabled.recording_stopped || enabled.ready)
  const wantChanges = enabled.ready
  const wantSummary = enabled.not_backed
  const [viewResult, summaryResult, captureResult] = await Promise.allSettled([
    wantProjects ? get(API.project(projectId)) : null,
    wantSummary ? get(NOT_BACKED_FEED + (findingAfter == null ? '' : `?after=${findingAfter}`)) : null,
    wantProjects && enabled.recording_stopped ? get(`/api/hq/projects/${encodeURIComponent(projectId)}/capture`) : null,
  ])
  const result = { events: [], recordings: [], errors: [], findingSeq: null,
    projectId: projectId == null ? null : id(projectId), complete: { ready: false, agent_waiting: false } }
  const view = viewResult.status === 'fulfilled' ? viewResult.value : null
  const summary = summaryResult.status === 'fulfilled' ? summaryResult.value : null
  const validView = !!(view && view.project && id(view.project.id) === id(projectId))
  if (wantProjects && viewResult.status === 'rejected') result.errors.push('project')
  if (wantSummary && summaryResult.status === 'rejected') result.errors.push('findings')
  if (wantSummary && summary) {
    result.events.push(...findingEvents(summary))
    if (Number.isInteger(summary.next_seq)) result.findingSeq = summary.next_seq
  }
  if (wantChanges && validView) {
    const members = rows(view.project, 'members').map((member) => member && member.ledger_project_id).filter(Boolean)
    const lists = await Promise.allSettled(members.map((member) => get(API.changes(member))))
    const listsComplete = lists.every((list) => list.status === 'fulfilled')
    if (!listsComplete) result.errors.push('changes')
    const changes = { changes: lists.flatMap((list) => list.status === 'fulfilled' ? rows(list.value, 'changes') : []) }
    const entries = await pooled(rows(changes, 'changes').filter((c) => c && c.id), 4, async (change) => {
      try { return [id(change.id), await get(API.readiness(change.id))] }
      catch { result.errors.push(`readiness:${change.id}`); return [id(change.id), null] }
    })
    result.events.push(...readyEvents(changes, Object.fromEntries(entries)))
    result.complete.ready = listsComplete && entries.every((entry) => entry && entry[1] != null)
  }
  if (enabled.agent_waiting && validView) {
    result.events.push(...waitingEvents({ id: projectId }, view))
    result.complete.agent_waiting = true
  }
  if (enabled.recording_stopped && wantProjects) {
    if (captureResult.status === 'fulfilled') {
      const state = recordingState(captureResult.value)
      if (state && state.project === String(projectId)) result.recordings.push(state)
    } else result.errors.push('capture')
  }
  return result
}
