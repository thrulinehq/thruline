/* The Read (P6-2; design record §3.2) — the rail's state, PURE.
 *
 * WHAT THE RAIL IS. When a piece of recorded work is opened, the engine reads
 * its record in steps: it reads the recorded steps, finds what the agent said
 * was done, matches that to the checks that ran, looks at the files, applies
 * the verdict rules and writes it up. `GET /api/read` serves those steps as
 * the engine reported them — each with a real count and its measured time —
 * and this module turns that answer into what the rail says. It decides
 * nothing about the record: every row is a served row, every number a served
 * field.
 *
 * THE RULES (design record §3.2, 03-USER-LENS.md rule 10), each held here:
 *  - Rows are EXACTLY the served steps, in the served order. No step is added
 *    while the answer is on its way — the rail says it is waiting, and says
 *    that showing each step as it happens needs engine support — and none is
 *    dropped: an id this build has no name for renders its own id.
 *  - Each time is the served `elapsed_ms`, as measured. Nothing is scaled,
 *    replayed or padded; there is no schedule here at all.
 *  - A cached answer says "Read earlier at …, nothing changed since." and
 *    shows the one step it took — the comparison — and nothing else.
 *  - A step that failed, could not be measured, was withheld or did not run
 *    says so in words. A withheld step and a step that did not run show no
 *    count and no time: no result is shown for work that has no result.
 *  - Skip is in every state.
 *  - Reduced motion: no spinner and no counter; the finished list as soon as
 *    it is known. (Without it: a spinner and a counter of real wall time,
 *    only while the answer is on its way.)
 *
 * The strings are `HQ_COPY.read`, with the step names from `HQ_COPY.phases`
 * — one name per engine phase across the app. Nothing here is a literal a
 * person reads.
 */
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { clock } from './run-route.js'

export const READ_CONTRACT = 'thruline.read.v1'

const R = HQ_COPY.read

/** The name of one step: the Read's own step, else the engine phase's name, else its id. */
export function stepLabel(id) {
  const key = id == null ? '' : String(id)
  return R.labels[key] || HQ_COPY.phases.labels[key] || key
}

/** A served count in words, or '' when the step carries no whole count. Never coerced. */
export function stepCount(id, n) {
  if (!Number.isInteger(n) || n < 0) return ''
  const key = String(id)
  if (R.counts[key]) return n === 1 ? R.counts[key + '_one'] : fill(R.counts[key], { n })
  return String(n)
}

/** A served time in words, exactly as measured, or '' when there is none. */
export function stepTime(ms) {
  if (!Number.isInteger(ms) || ms < 0) return ''
  return ms < 1000 ? fill(R.ms, { n: ms }) : fill(R.seconds, { n: (ms / 1000).toFixed(1) })
}

/** What a served answer must be before any row of it is shown. */
export function isReadAnswer(doc) {
  return !!doc && doc.contract === READ_CONTRACT && Array.isArray(doc.phases) && typeof doc.cached === 'boolean'
}

/** One served step as a row. */
export function stepRow(phase, all) {
  const p = phase && typeof phase === 'object' ? phase : {}
  const id = p.phase == null ? '' : String(p.phase)
  const status = typeof p.status === 'string' ? p.status : ''
  const row = { id, status, label: stepLabel(id), count: '', time: '', note: '', reason: '' }
  if (status === 'done') {
    row.count = stepCount(id, p.count)
    row.time = stepTime(p.elapsed_ms)
  } else if (status === 'unmeasured') {
    row.count = stepCount(id, p.count)
    row.note = R.status.unmeasured
  } else if (status === 'failed') {
    row.note = R.status.failed
    row.reason = typeof p.reason === 'string' ? p.reason : ''
  } else if (status === 'withheld') {
    const by = p.withheld_by == null ? '' : String(p.withheld_by)
    const blocker = (Array.isArray(all) ? all : []).find((x) => x && String(x.phase) === by)
    row.note = fill(blocker && blocker.status === 'unmeasured' ? R.status.withheld_unmeasured : R.status.withheld, { what: stepLabel(by) })
  } else if (status === 'not_run') {
    row.note = R.status.not_run
  } else {
    /* A status this build does not know is not a result: it is shown as none. */
    row.note = R.status.unknown
  }
  return row
}

/**
 * What the rail says.
 *
 * @param {{status: 'loading'|'ready'|'refused', doc?: object, refusal?: string}} read
 *   `ready` carries the served answer; `refused` carries why it could not be had
 * @param {{reducedMotion?: boolean, waitedMs?: number, session?: string, address?: string}} [opts]
 */
export function readRailModel(read, opts = {}) {
  const reduced = !!opts.reducedMotion
  const base = {
    label: R.label,
    title: R.title,
    skip: R.skip,
    skipLabel: R.skip_label,
    rows: [],
    lead: '',
    live: '',
    waiting: '',
    motion: false,
    technicalSummary: R.technical.summary,
    technical: [[R.technical.address, opts.address || '']],
  }
  const status = read && read.status
  if (status === 'refused' || (status === 'ready' && !isReadAnswer(read.doc))) {
    return {
      ...base,
      state: 'refused',
      lead: R.refused,
      technical: [...base.technical, [R.technical.refused, status === 'refused' ? String(read.refusal || '') : HQ_COPY.phases.failed_body]],
    }
  }
  if (status !== 'ready') {
    const waited = Number.isFinite(opts.waitedMs) && opts.waitedMs >= 0 ? opts.waitedMs : 0
    return {
      ...base,
      state: 'loading',
      lead: R.loading,
      live: R.live,
      motion: !reduced,
      waiting: reduced ? '' : fill(R.waited, { n: (waited / 1000).toFixed(1) }),
    }
  }
  const doc = read.doc
  const rows = doc.phases.map((p) => stepRow(p, doc.phases))
  const compared = doc.compared && typeof doc.compared === 'object' ? doc.compared : {}
  const outcome = R.technical['compared_' + compared.outcome] || String(compared.outcome || '')
  const technical = [
    ...base.technical,
    [R.technical.answer, doc.cached ? R.technical.answer_cached : R.technical.answer_fresh],
    [R.technical.read_at, typeof doc.read_at === 'string' ? doc.read_at : ''],
    [R.technical.compared, Number.isInteger(compared.elapsed_ms) ? outcome + ' ' + fill(R.technical.compared_time, { n: compared.elapsed_ms }) : outcome],
    [R.technical.fingerprint, typeof compared.fingerprint === 'string' ? compared.fingerprint : ''],
    ...rows.map((r) => [R.technical.step, r.id + (r.reason ? ' — ' + R.technical.reason + ': ' + r.reason : '')]),
  ]
  if (doc.cached) {
    return { ...base, state: 'cached', lead: fill(R.cached, { when: clock(doc.read_at) || doc.read_at || '' }), rows, technical, note: R.technical.not_covered }
  }
  const whole = doc.complete === true && rows.every((r) => r.status === 'done')
  return { ...base, state: whole ? 'fresh' : 'incomplete', lead: whole ? R.complete : R.incomplete, rows, technical, note: R.technical.not_covered }
}
