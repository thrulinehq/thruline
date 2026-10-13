/* Step-View's facts, selected from the one Change snapshot (data/change.js) and,
 * for the lines around the step, from the session timeline the server serves
 * (`GET /api/hq/sessions/<sid>`, docs/hq-next/API.md).
 *
 * PURE. Nothing here derives a number the snapshot does not hold: the step
 * numbers are `snapshot.steps[].step` (a row's place in the thread's record,
 * the same numbers every other screen and chip uses), the receipt is the
 * served row's own file and line, and the lines around are the session's own
 * items. A step the record does not number — a check ThruLine ran has none —
 * is not found here, and nothing is made up to stand in for it.
 */
import { hhmm } from '../../data/change.js'
import { href } from '../../routes.js'
import { isStep } from './link.js'

/** How many steps the "around this step" list shows on each side. */
export const STEPS_EACH_SIDE = 3
/** How many of the session's own lines are shown on each side of the step's. */
export const LINES_EACH_SIDE = 3

const AGENT_FALLBACK = 'The agent'

/** The step a snapshot holds at number `n`, or null. */
export function stepAt(snap, n) {
  const steps = snap && Array.isArray(snap.steps) ? snap.steps : []
  return isStep(n) ? steps.find((s) => s.step === n) || null : null
}

/** One step, trimmed to what a neighbour list or a link needs. */
function brief(s, at) {
  return {
    step: s.step,
    kind: s.kind,
    time: s.time || null,
    agent: s.agent || null,
    prompt: s.prompt ? { text: s.prompt.text, work: s.prompt.work } : null,
    claim: s.claim ? { text: s.claim.text } : null,
    message: s.message ? { text: s.message.text } : null,
    edit: s.edit ? { file: s.edit.file, linesAdded: s.edit.linesAdded, linesRemoved: s.edit.linesRemoved } : null,
    command: s.command ? { command: s.command.command, isTest: s.command.isTest, isCommit: s.command.isCommit, outcome: s.command.outcome, tally: s.command.tally } : null,
    href: href('step', { change: at.change, step: s.step, project: at.project }),
  }
}

/**
 * The session's own lines around the item that is this step's record.
 *
 * @param {object|null} timeline  `SessionTimeline`
 * @param {string} eventId        the step's event id (`<event>#<n>` already cut)
 * @returns {{found: boolean, items: object[], before: number, after: number}}
 */
export function linesAround(timeline, eventId, each = LINES_EACH_SIDE) {
  const items = timeline && Array.isArray(timeline.items) ? timeline.items : []
  const at = items.findIndex((x) => x && x.event_id === eventId)
  if (at < 0) return { found: false, items: [], before: 0, after: 0 }
  const from = Math.max(0, at - each)
  const to = Math.min(items.length, at + each + 1)
  return {
    found: true,
    before: from,
    after: items.length - to,
    items: items.slice(from, to).map((x, i) => ({
      eventId: x.event_id,
      seq: x.seq ?? null,
      kind: x.kind || 'other',
      time: hhmm(x.ts),
      text: typeof x.text === 'string' ? x.text : null,
      tool: x.tool ? { name: x.tool.name || null, summary: Array.isArray(x.tool.summary) ? x.tool.summary : [], outcome: x.tool.outcome || null, exitCode: x.tool.exit_code ?? null } : null,
      sidechain: Boolean(x.sidechain),
      current: from + i === at,
    })),
  }
}

/**
 * @param {object} snap   the Change snapshot (data/change.js)
 * @param {{step?: number|null, asked?: string, change?: string, project?: string}} at  the address
 * @param {{timeline?: object|null, timelineError?: Error|null}} [session]
 */
export function stepViewFromChange(snap, at, session = {}) {
  const S = snap || {}
  const steps = Array.isArray(S.steps) ? S.steps : []
  const stepCount = S.change && Number.isInteger(S.change.stepCount) ? S.change.stepCount : steps.length
  const where = { change: at && at.change, project: (at && at.project) || (S.project && S.project.id ? String(S.project.id) : undefined) }
  const n = at && isStep(at.step) ? at.step : null
  const row = stepAt(S, n)
  const sessions = Array.isArray(S.sessions) ? S.sessions : []

  const data = {
    live: true,
    project: { id: S.project ? S.project.id || null : null, name: S.project ? S.project.name || '' : '' },
    change: { id: where.change || null, title: S.change ? S.change.title || '' : '', stepCount },
    machine: { recordsBytes: S.machine ? S.machine.recordsBytes ?? null : null },
    view: { readAt: S.view ? S.view.readAt || '' : '' },
    asked: n,
    askedText: at && at.asked ? at.asked : n != null ? String(n) : '',
    where,
    found: Boolean(row),
    step: null,
    previous: null,
    next: null,
    around: [],
    lines: null,
    linesError: null,
    statementHref: null,
  }
  if (!row) return data

  const index = sessions.findIndex((x) => x.id === row.sessionId)
  data.step = {
    ...brief(row, where),
    ts: row.ts,
    id: row.id,
    eventId: row.eventId,
    sessionId: row.sessionId,
    session: index >= 0 && sessions.length > 1 ? { n: index + 1, of: sessions.length } : null,
    agent: row.agent || AGENT_FALLBACK,
    version: row.version,
    receipt: row.receipt,
    prompt: row.prompt,
    edit: row.edit,
    command: row.command,
    claim: row.claim,
    message: row.message,
  }
  const by = (k) => steps.find((s) => s.step === k)
  const prev = by(row.step - 1)
  const next = by(row.step + 1)
  data.previous = prev ? brief(prev, where) : null
  data.next = next ? brief(next, where) : null
  data.around = steps
    .filter((s) => s.step >= row.step - STEPS_EACH_SIDE && s.step <= row.step + STEPS_EACH_SIDE)
    .map((s) => ({ ...brief(s, where), current: s.step === row.step }))
  if (row.claim && row.claim.statementId) data.statementHref = href('brief', { change: where.change, project: where.project, statement: row.claim.statementId })

  if (session.timeline) data.lines = linesAround(session.timeline, row.eventId)
  else if (session.timelineError) data.linesError = true
  return data
}
