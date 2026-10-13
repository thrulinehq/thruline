/* NOT-APPROVAL-WORDS: the run sheet names the lines whose passing run nobody linked; it says nothing about approval (RUN-LINK-TRUTH-R5 census). */
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'

/* P6-5 (design record §3.7, §3.8) — Check it again, without a terminal.
 *
 * The run sheet's model: what the sheet says, which one control is primary,
 * and which of the six outcomes the last run reached. Pure — no fetch, no
 * React — so every sentence below can be asserted on its own.
 *
 * ONE TRUTH. The outcome is the SERVER's (`crates/cli/src/uiserve/run_route.rs`
 * `outcome_of`), read from typed record fields: the execution state, the
 * binding's exit status and stream captures, and the typed test count. This
 * file maps that closed set of words to sentences and never re-derives it; a
 * number on the sheet is a number the record holds.
 *
 * ONLY A PASS READS AS A PASS. `passes` is true for `passed` and for nothing
 * else, and every other outcome's sentence says what it does not settle.
 *
 * The run state lives here and nowhere in `change-model.js`, which this lane
 * may not edit; what that model would need to fold a run into the headline is
 * listed in `docs/loop/run/INTERFACE-NEEDS.md`.
 */

const R = HQ_COPY.run
/* The ruled execution-reason sentences (`crates/core/src/voice.rs`, the same
 * words). OFFLINE-BUILD-OUTCOME (D-051): a build that could not fetch what it
 * needs is `checked_nothing` with the why `prerequisite_absent`, and says the
 * sentence the product already has for that reason rather than a new one. */
const REASONS = HQ_COPY.change.model.execution.reasons

/** The six outcomes, in the design record's order. */
export const OUTCOMES = Object.freeze(['passed', 'failed', 'checked_nothing', 'cancelled', 'interrupted', 'tool_not_found'])

const list = (v) => (Array.isArray(v) ? v : [])
const str = (v) => (v == null ? '' : String(v))

/** A stored UTC moment as this machine's clock time, `HH:MM`. */
export function clock(iso) {
  if (typeof iso !== 'string' || !iso) return ''
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return ''
  return String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0')
}

/** A stored UTC moment as a date a person reads. */
function day(iso) {
  if (typeof iso !== 'string' || !iso) return ''
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return ''
  return d.toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' })
}

/**
 * One outcome, in words. `last` is the served `checks[].last` object.
 *
 * @param {object|null} last
 * @param {{ program?: string }} [ctx]
 * @returns {{ kind: string, title: string, lines: string[], passes: boolean, failureLine: string|null, when: string } | null}
 */
export function outcomeView(last, ctx = {}) {
  if (!last || !last.outcome || last.outcome === 'running' || last.outcome === 'not_run') return null
  const O = R.outcomes
  const kind = OUTCOMES.includes(last.outcome) ? last.outcome : null
  if (!kind) return null
  const lines = []
  let failureLine = null
  switch (kind) {
    case 'passed': {
      const n = Number(last.tests_passed)
      lines.push(n === 1 ? O.passed.line_one : fill(O.passed.line, { n }))
      lines.push(O.passed.note)
      break
    }
    case 'failed': {
      const failed = last.tests_failed
      const passed = last.tests_passed
      if (Number.isInteger(failed) && Number.isInteger(passed) && failed > 0) {
        lines.push(fill(O.failed.line, { failed, total: failed + passed }))
      } else {
        lines.push(O.failed.line_no_count)
      }
      failureLine = typeof last.failure_line === 'string' && last.failure_line ? last.failure_line : null
      break
    }
    case 'checked_nothing':
      lines.push(O.checked_nothing[last.why] || REASONS['execution_reason_' + last.why] || O.checked_nothing.no_test_ran)
      break
    case 'cancelled':
      lines.push(O.cancelled.line)
      break
    case 'interrupted':
      if (last.reason && O.interrupted[last.reason]) lines.push(O.interrupted[last.reason])
      lines.push(O.interrupted.line)
      break
    case 'tool_not_found':
      lines.push(fill(O.tool_not_found.line, { tool: ctx.program || '' }))
      break
    default:
      break
  }
  // OFFLINE-KILL-GUARD-R4 (P-2026-09-28-4): the record says ThruLine's own
  // fetch step did not run, and why; the sheet says it beside the outcome.
  if (typeof last.fetch_skipped === 'string' && last.fetch_skipped) {
    lines.push(fill(O.fetch_skipped, { why: last.fetch_skipped }))
  }
  return {
    kind,
    title: O[kind].title,
    lines,
    /* THE rule: exactly one outcome reads as a pass. */
    passes: kind === 'passed',
    failureLine,
    when: clock(last.recorded_at || last.started_at),
  }
}

/** What to paste into the agent when the tool is not found. Prepared, not sent. */
export function agentPrompt(program, folder) {
  return fill(R.tool.prompt, { path: folder || '', tool: program || '' })
}

/**
 * P6-5B — the permission as the record holds it: who allowed checks here and
 * when, every time it was taken back, and — for the Technical face — the row
 * each line comes from. Every name and time is a served row field; nothing
 * here is remembered by the browser.
 *
 * @param {object|null} consent the served `consent` object
 */
export function consentView(consent) {
  const C = R.consent
  const c = consent && typeof consent === 'object' ? consent : {}
  const history = list(c.history)
  const lines = history.map((row) => fill(row.act === 'revoked' ? C.revoked : C.granted, {
    who: row.actor || '',
    date: day(row.at) || row.at || '',
    when: clock(row.at) || '—',
  }))
  const stateLine = c.state === 'revoked'
    ? C.state_revoked
    : c.state === 'not_in_record'
      ? C.state_not_in_record
      : c.state === 'file_missing' ? C.state_file_missing : null
  return {
    state: c.state || null,
    heading: C.heading,
    lines,
    stateLine,
    /* Only a grant the RECORD holds can be taken back. */
    canTakeBack: c.state === 'given' || c.state === 'file_missing',
    takeBack: C.take_back,
    takeBackNote: C.take_back_note,
    technical: history.map((row) => {
      const r = row.receipt || {}
      return {
        label: fill(row.act === 'revoked' ? R.technical.consent_revoked : R.technical.consent_granted, { n: Number.isInteger(r.seq) ? r.seq : '?' }),
        id: r.id || '',
        link: r.chain_hash || '',
      }
    }),
    file: typeof c.file === 'string' ? c.file : null,
  }
}

/**
 * A refusal slug from the server, in Sam's words. A sentence that names the
 * program (`program_in_project`) names the one the server served in `doc`,
 * and is never said with a bare placeholder when none was served.
 */
export function refusalText(error, doc) {
  const words = R.refusals[error] || R.refusals.other
  if (!words.includes('{tool}')) return words
  const program = doc && typeof doc.program === 'string' && doc.program ? doc.program : null
  return program ? fill(words, { tool: program }) : R.refusals.other
}

/** One environment the server found, as a row a person can pick. */
export function environmentRow(env, program) {
  const label = env.kind === 'hq'
    ? R.tool.env_hq
    : fill(env.kind === 'project' ? R.tool.env_project : R.tool.env_machine, { path: env.folder || '' })
  const has = list(env.has).includes(program)
  return {
    id: env.id,
    label,
    hasTool: has,
    hasLine: fill(has ? R.tool.has : R.tool.has_not, { tool: program || '' }),
  }
}

/**
 * Everything the sheet shows, and the one primary control.
 *
 * @param {object|null} view the served `GET /api/changes/<id>/run`
 * @param {{ version?: {short?: string}|null, pending?: string|null, env?: string|null, refused?: string|null, unlinked?: Array<{n: number, text: string}>|null }} [ctx]
 *   `refused`: the last press's refusal slug, if any (D-049); `unlinked`: the
 *   lines the served readiness names `readiness_unlinked_run` (RUN-LINK-TRUTH)
 */
export function runSheetModel(view, ctx = {}) {
  if (!view) return { status: 'loading', loading: R.loading }
  const check = list(view.checks)[0] || null
  const folder = view.project_folder || null
  const envs = list(view.environments)
  const envId = ctx.env || view.remembered_environment || 'hq'
  const env = envs.find((e) => e.id === envId) || null
  const whereLine = folder
    ? (envId === 'hq' || !env ? fill(R.where_hq, { path: folder }) : fill(R.where_env, { path: folder, name: env.folder || '' }))
    : null
  const version = ctx.version && ctx.version.short ? ctx.version.short : null

  const job = view.job || null
  const last = check ? check.last || null : null
  const running = job
    ? (job.for_this_change ? job.phase : 'elsewhere')
    : last && last.outcome === 'running' ? 'running' : null
  const runKey = job && job.for_this_change ? job.run : last && last.outcome === 'running' ? last.run : null
  const outcome = running ? null : outcomeView(last, { program: check ? check.program : '' })
  const notStarted = !running && view.not_started ? view.not_started : null

  /* The one primary control, chosen here and nowhere else. */
  let primary
  if (!folder) primary = { act: null, label: R.run, blocked: R.blocked.no_folder }
  else if (!check) primary = { act: null, label: R.run, blocked: R.blocked.no_check }
  else if (running === 'elsewhere') primary = { act: null, label: R.run, blocked: R.blocked.busy }
  else if (running === 'starting') primary = { act: null, label: R.cancel, blocked: R.blocked.cancel_wait }
  else if (running === 'running') primary = { act: 'cancel', label: R.cancel, blocked: null, note: R.cancel_note }
  else if (!view.permission) primary = { act: 'allow-run', label: R.allow_and_run, blocked: null }
  else if (outcome && outcome.kind === 'tool_not_found' && !ctx.pending) primary = { act: 'ask', label: R.tool.ask_agent, blocked: null }
  else primary = { act: 'run', label: last ? R.run_again : R.run, blocked: null }

  /* The places outside HQ's own search path a person can pick: offered when
   * the program was not found, after a program-in-project refusal, or when
   * Settings opens this same sheet to browse before a run. The served Change
   * must still name both a check and its recorded folder. */
  const offerPlaces = check && folder && (ctx.browsePlaces || (outcome && outcome.kind === 'tool_not_found') || ctx.refused === 'program_in_project')
  const places = offerPlaces
    ? envs.filter((e) => e.kind !== 'hq').map((e) => environmentRow(e, check.program))
    : null
  const toolMissing = outcome && outcome.kind === 'tool_not_found' && check
    ? {
      prompt: agentPrompt(check.program, folder),
      elsewhere: places,
    }
    : null

  /* P6-2 Part 0 — ONE line says who allowed checks here and when. The record's
   * history line carries the actor and the time; when it is there, the older
   * "You allowed checks in this project on …" sentence (P6-5, date only) is not
   * said again one line above it. It remains only for a served view with no
   * history to show. */
  const consent = consentView(view.consent)
  /* RUN-LINK-TRUTH (PW-F9) — a pass the record holds and no line counts yet:
   * the lines are the ENGINE's (`readiness_unlinked_run`, handed in by the
   * screen as `ctx.unlinked`), and the sheet only says so and offers one door
   * to the first of them. It links nothing. */
  const firstUnlinked = outcome && outcome.kind === 'passed' ? list(ctx.unlinked).find((u) => u && typeof u.n === 'number') : null
  return {
    status: 'ready',
    heading: R.heading,
    what: check && check.category === 'tests' ? R.what_tests : R.what_other,
    whereLine,
    canLine: version ? fill(R.can_version, { revision: version }) : R.can,
    cannotLine: R.cannot,
    permission: view.permission
      ? { needed: false, given: consent.lines.length ? null : fill(R.permission_given, { date: day(view.permission.granted_at) || view.permission.granted_at }) }
      : { needed: true, heading: R.permission_heading, text: R.permission, promise: R.promise },
    consent,
    promise: R.promise,
    check,
    running,
    runKey,
    runningLine: running === 'running' && (job || last)
      ? fill(R.running, { when: clock((job && job.started_at) || (last && last.started_at)) || '—' })
      : running === 'starting' ? R.starting : null,
    primary,
    outcome,
    unlinked: firstUnlinked
      ? { n: firstUnlinked.n, sentence: R.outcomes.passed.unlinked, door: fill(R.outcomes.passed.unlinked_door, { line: str(firstUnlinked.text) }) }
      : null,
    notStarted: notStarted ? { title: R.outcomes.did_not_start.title, line: R.outcomes.did_not_start.line, reason: notStarted.reason } : null,
    toolMissing,
    places,
    pendingLine: ctx.pending ? fill(R.tool.pending, { when: clock(ctx.pending) || '—' }) : null,
    environment: envId,
    actor: view.actor || '',
    technical: {
      command: check ? check.command : null,
      folder,
      run: runKey || (last && last.run) || (notStarted && notStarted.run) || null,
      exit: last && last.exit_code !== undefined && last.exit_code !== null ? String(last.exit_code) : null,
      environment: env && env.folder ? env.folder : null,
      reason: notStarted ? notStarted.reason : null,
    },
  }
}

/**
 * A double-click is one job, on this side too: while one press is being
 * handled, every further press is ignored — synchronously, before any await,
 * so two clicks in the same frame cannot both get through. The server holds
 * the same rule under its own lock; this is the half a person sees.
 *
 * @template T
 * @param {(...args: any[]) => Promise<T>} act
 * @returns {(...args: any[]) => Promise<T|undefined>}
 */
export function oneAtATime(act) {
  let busy = false
  return async (...args) => {
    if (busy) return undefined
    busy = true
    try {
      return await act(...args)
    } finally {
      busy = false
    }
  }
}

export default runSheetModel
