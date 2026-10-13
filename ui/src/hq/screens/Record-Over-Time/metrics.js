/* Record-Over-Time: the counts that need more than a verdict, and the CSV.
 *
 * Every number here is made ONLY from the fields the statements route serves
 * on a claim (`GET /api/hq/projects/<id>/statements`, docs/hq-next/API.md):
 *
 *   ts            when the statement was made
 *   verdict       proven | not_proven | contradicted | recorded
 *   verdict_basis the branch of the engine's rule that gave it
 *   has_verifier  a test run could settle it
 *   proven_at     when the run that proves it was recorded (proven only)
 *   run_before    a test run came before it in its piece of work
 *   proof_moved   the run it rests on passed, and the code has moved since
 *
 * A count is `served` only when every row it reads carries the field. A field
 * a server did not send is never read as false, zero or "no": the count is not
 * served and the page says it is not counted. A statement the record cannot
 * time is left out of the median and counted as `unmeasured`, never guessed.
 *
 * PURE: no fetch, no clock, no DOM. The rows are
 * `{pid, a: 'cc'|'cx', ago, key, c}`: the project, the agent, whole days ago,
 * the tally key (from-projects.js `tallyKey`), and the served claim `c`.
 */

export const MIN_MS = 60000

/** What the page says where the record cannot give a number. */
export const REASONS = Object.freeze({
  median:
    'Not counted. This record does not say when the run that proves each statement was made, so the minutes between the two cannot be told.',
  moved: 'Not counted. This record does not say whether a later edit moved the proof a statement rests on.',
  before: 'Not counted. This record does not say whether a run came before each statement.',
  reruns:
    'Not counted. Checks you start from a Change are kept with that Change, and this page does not read them across Changes.',
})

const isBool = (x) => typeof x === 'boolean'
const ms = (iso) => {
  const t = typeof iso === 'string' ? Date.parse(iso) : NaN
  return Number.isNaN(t) ? null : t
}

/** The rows of one selection: a range in days, an agent ('all'|'cc'|'cx') and a project ('all'|id). */
export function sliceRows(rows, { range, agent = 'all', project = 'all' }) {
  return (Array.isArray(rows) ? rows : []).filter(
    (r) => r.ago < Number(range) && (project === 'all' || r.pid === project) && (agent === 'all' || r.a === agent),
  )
}

/** The middle value of a list of numbers (the mean of the two middle ones when even); null when empty. */
export function median(xs) {
  if (!xs.length) return null
  const s = xs.slice().sort((a, b) => a - b)
  const m = s.length >> 1
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2
}

/**
 * How long statements waited for their proof.
 *
 * A proven statement waited when the run that proves it was recorded after the
 * statement was made. One proven by a run recorded before it (or at the same
 * instant) was backed when it was said: it waited for nothing, so it is counted
 * apart, never as 0 minutes in the median.
 *
 * @returns {{served: boolean, proven: number, waited: number, already: number,
 *            unmeasured: number, minutes: number|null}}
 */
export function minutesToProof(rows) {
  const proven = rows.filter((r) => r.c.verdict === 'proven')
  const served = proven.every((r) => r.c.proven_at !== undefined)
  if (!served) return { served: false, proven: proven.length, waited: 0, already: 0, unmeasured: 0, minutes: null }
  const waits = []
  let already = 0
  let unmeasured = 0
  for (const r of proven) {
    const said = ms(r.c.ts)
    const ran = ms(r.c.proven_at)
    if (said === null || ran === null) unmeasured += 1
    else if (ran > said) waits.push((ran - said) / MIN_MS)
    else already += 1
  }
  return { served: true, proven: proven.length, waited: waits.length, already, unmeasured, minutes: median(waits) }
}

/**
 * Minutes as the page and the file both write them: under 10, one decimal
 * (trailing ".0" dropped); from 10, whole minutes. No thousands separator.
 */
export function minutesText(m) {
  if (m === null || m === undefined || !Number.isFinite(m)) return ''
  if (m < 10) return String(Math.round(m * 10) / 10)
  return String(Math.round(m))
}

/**
 * Statements whose proof was moved by a later edit, and where each stands now.
 * `open` + `reproven` + `failed` = `total`; `open` is exactly the statements
 * the headline says "need a re-check".
 *
 * @returns {{served: boolean, total: number, open: number, reproven: number, failed: number}}
 */
export function movedProofs(rows) {
  const served = rows.every((r) => isBool(r.c.proof_moved))
  if (!served) return { served: false, total: 0, open: 0, reproven: 0, failed: 0 }
  const moved = rows.filter((r) => r.c.proof_moved === true)
  const by = (v) => moved.filter((r) => r.c.verdict === v).length
  return { served: true, total: moved.length, open: by('not_proven'), reproven: by('proven'), failed: by('contradicted') }
}

/**
 * Statements made before any run: a statement a run could settle (it has a
 * verifier, and sits in a piece of work) with no test run before it in that
 * piece of work. `laterProven` + `failedAfter` + `stillNothing` = `total`.
 *
 * @returns {{served: boolean, total: number, laterProven: number, failedAfter: number, stillNothing: number}}
 */
export function beforeAnyRun(rows) {
  const served = rows.every((r) => isBool(r.c.has_verifier)) && rows.filter((r) => r.c.has_verifier === true).every((r) => isBool(r.c.run_before))
  if (!served) return { served: false, total: 0, laterProven: 0, failedAfter: 0, stillNothing: 0 }
  const first = rows.filter((r) => r.c.has_verifier === true && r.c.run_before === false && r.c.verdict_basis !== 'not_in_a_thread')
  const by = (v) => first.filter((r) => r.c.verdict === v).length
  return { served: true, total: first.length, laterProven: by('proven'), failedAfter: by('contradicted'), stillNothing: by('not_proven') }
}

/** Every metric of one selection's rows. */
export function sliceMetrics(rows) {
  return { median: minutesToProof(rows), moved: movedProofs(rows), before: beforeAnyRun(rows) }
}

/* ---- the CSV ---- */

export const CSV_COLUMNS = Object.freeze([
  'project',
  'agent',
  'range_days',
  'counted_through',
  'statements',
  'proven',
  'need_a_recheck',
  'nothing_found',
  'a_run_failed',
  'recorded_settles_nothing',
  'proof_moved_by_a_later_edit',
  'reproven_after_a_later_edit',
  'said_before_any_run',
  'median_minutes_to_proof',
])

const AGENT_NAME = { cc: 'Claude Code', cx: 'Codex' }

/** One CSV field, RFC 4180: quoted when it holds a comma, a quote or a line break; quotes doubled. */
export function csvField(x) {
  const s = x === null || x === undefined ? '' : String(x)
  return /[",\r\n]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s
}

/**
 * A project's name as the file writes it: never a folder path. A name that
 * looks like one keeps only its last segment; a bare home folder is "Home folder".
 */
export function safeName(name) {
  const s = String(name === null || name === undefined ? '' : name).trim()
  if (!s) return 'Unnamed project'
  if (/^~\/?$/.test(s)) return 'Home folder'
  if (/[\\/]/.test(s) || s.startsWith('~')) {
    const last = s.split(/[\\/]+/).filter(Boolean).pop()
    return last && last !== '~' ? last : 'Home folder'
  }
  return s
}

const slug = (s) => s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'project'

/** `thruline-record-<project>-<range>.csv`; one agent chosen adds `-claude-code` or `-codex`. */
export function csvFilename({ project, range, agent }) {
  const who = agent === 'cc' ? '-claude-code' : agent === 'cx' ? '-codex' : ''
  return 'thruline-record-' + (project === null ? 'all-projects' : slug(safeName(project))) + '-' + Number(range) + 'd' + who + '.csv'
}

/**
 * The file for one selection: one row per project and agent that made a
 * statement in it, with the counts the page shows. An empty cell is a count
 * that is not counted, never 0.
 *
 * @param {{rows: object[], projects: Array<{id: string, name: string}>, range: number|string,
 *          agent: 'all'|'cc'|'cx', project: 'all'|string, through: string}} p
 * @returns {{filename: string, text: string, count: number}}
 */
export function recordCsv({ rows, projects, range, agent, project, through }) {
  const lines = [CSV_COLUMNS.join(',')]
  let count = 0
  const chosen = projects.filter((p) => project === 'all' || p.id === project)
  for (const p of chosen) {
    for (const a of ['cc', 'cx']) {
      if (agent !== 'all' && agent !== a) continue
      const mine = sliceRows(rows, { range, agent: a, project: p.id })
      if (!mine.length) continue
      const tally = (k) => mine.filter((r) => r.key === k).length
      const m = sliceMetrics(mine)
      const cells = [
        safeName(p.name),
        AGENT_NAME[a],
        Number(range),
        through,
        mine.length,
        tally('proven'),
        tally('needsRecheck'),
        tally('noProofFound'),
        tally('rerunFailed'),
        tally('recordedStopsAtRanOn') + tally('recordedStopsAtSo'),
        m.moved.served ? m.moved.total : '',
        m.moved.served ? m.moved.reproven : '',
        m.before.served ? m.before.total : '',
        m.median.served ? minutesText(m.median.minutes) : '',
      ]
      lines.push(cells.map(csvField).join(','))
      count += 1
    }
  }
  const named = project === 'all' ? null : (chosen[0] ? chosen[0].name : project)
  return { filename: csvFilename({ project: named, range, agent }), text: lines.join('\r\n') + '\r\n', count }
}
