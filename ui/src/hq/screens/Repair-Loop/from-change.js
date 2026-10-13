/* Repair-Loop on the real record: the screen's data (DEMO's shape, DATA.md)
 * selected from the ONE Change snapshot (`ui/src/hq/data/change.js`, the shape
 * of Change-Brief/demo.js's DEMO) and, where a person tracks the work as a
 * Change, that Change's run sheet read through the existing run-route model.
 *
 * PURE. It picks and renames; it decides nothing the snapshot or the run sheet
 * has not already decided (`runSheetModel` is the run sheet's one reading). A
 * fact the snapshot does not hold is `null` here, never a guess, and the screen
 * says it in the design's language for absence.
 */
import { runSheetModel, outcomeView, clock as sheetClock } from '../../../workspace/run-route.js'
import { terminalCommand } from './terminal.js'
import { runActOf, failuresOf, liveOutputOf } from '../../data/run.js'

/** The most lines of a running check's output the sheet draws: the newest. The server serves up
 *  to a hundred; the rest are said to be there, not drawn (C30 R-RUN). */
export const RUNNING_LINES = 12

const list = (v) => (Array.isArray(v) ? v : [])
const byId = (rows, id) => (id == null ? null : list(rows).find((r) => r && r.id === id) || null)
const num = (v) => (typeof v === 'number' && Number.isFinite(v) ? v : null)
const hhmm = (d) => String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0')
/** A stored moment in milliseconds, or null when it is not one. */
const msOf = (iso) => {
  if (typeof iso !== 'string' || !iso) return null
  const t = Date.parse(iso)
  return Number.isNaN(t) ? null : t
}
/** Whole seconds from one stored moment to another; null when either is missing. */
const secondsBetween = (from, to) => {
  const a = msOf(from)
  const b = msOf(to)
  return a == null || b == null || b < a ? null : Math.floor((b - a) / 1000)
}
const clock = (iso) => sheetClock(iso) || null

/** The statement the screen repairs: the Brief's, else the first whose proof stops. */
export function repairStatement(snapshot) {
  const sts = list(snapshot && snapshot.statements)
  const named = snapshot && snapshot.brief ? byId(sts, snapshot.brief.statementId) : null
  if (named && named.proof && named.proof.restsOn && named.proof.restsOn.kind === 'run') return named
  return sts.find((s) => s && s.proof && s.proof.stopsAt) || named || null
}

function runOf(r) {
  if (!r) return null
  const out = list(r.output).map(String)
  return {
    step: num(r.step),
    at: r.time || null,
    command: r.command || null,
    version: r.version || null,
    exit: num(r.exitCode),
    passed: num(r.passed),
    failed: num(r.failed),
    durationS: num(r.durationMs) == null ? null : r.durationMs / 1000,
    elapsedS: num(r.elapsedS),
    /* the screen echoes the command itself; a recorded echo is not printed twice */
    output: out.length && out[0].startsWith('$ ') ? out.slice(1) : out,
    failures: list(r.failures),
  }
}

/** The finished test runs the record holds, newest last. */
const finished = (snapshot) => list(snapshot && snapshot.runs).filter((r) => r && r.status === 'finished' && r.kind === 'test')
const passedRun = (r) => r && num(r.failed) === 0 && num(r.passed) != null && r.passed > 0
const failedRun = (r) => r && num(r.failed) != null && r.failed > 0

/**
 * @param {object} snapshot  the one Change snapshot (Change-Brief DEMO shape)
 * @param {{id: string, runSheet: object|null}|null} tracked  the tracked Change, if a person tracks this work
 * @param {{now?: Date}} [ctx]
 */
export function selectRepairLoop(snapshot, tracked, ctx = {}) {
  if (!snapshot) return null
  const now = ctx.now instanceof Date ? ctx.now : new Date()
  const st = repairStatement(snapshot)
  const proof = (st && st.proof) || {}
  const runs = list(snapshot.runs)
  const edits = list(snapshot.edits)
  const versions = list(snapshot.versions)
  const V = snapshot.currentVersion || (snapshot.view && snapshot.view.readVersion) || null
  const cur = byId(versions, V)

  /* ② — the run the statement rested on before any re-check. */
  const lost = list(proof.thenAndNow && proof.thenAndNow.noLongerCounts)[0] || null
  const restsRun = proof.restsOn && proof.restsOn.kind === 'run' ? byId(runs, proof.restsOn.runId) : null
  const held = (lost && byId(runs, lost.runId)) || restsRun
  /* ④ — the edit after it. */
  const movedEdit = byId(edits, proof.since && proof.since.editId) || (lost && byId(edits, lost.editId)) || null

  /* ⑤ — the re-check on the current version: running, or finished. */
  const runningRun = byId(runs, proof.recheck && proof.recheck.status === 'running' ? proof.recheck.runId : null)
    || runs.find((r) => r && r.status === 'running' && r.version === V) || null
  const checkedRun = runningRun ? null
    : byId(runs, proof.recheck && proof.recheck.status !== 'running' ? proof.recheck.runId : null)
      || (proof.verdict === 'proven' && restsRun && restsRun.version === V && restsRun.status === 'finished' ? restsRun : null)
      || byId(runs, proof.followUp && proof.followUp.checkedAgain && proof.followUp.checkedAgain.runId)

  /* The tracked Change's run sheet: permission, the check, a run going now or ended. */
  const served = tracked && tracked.runSheet ? tracked.runSheet : null
  const sheet = served ? runSheetModel(served) : null
  const sheetReady = sheet && sheet.status === 'ready' ? sheet : null
  const job = served && served.job && served.job.for_this_change ? served.job : null
  const last = served && list(served.checks)[0] ? list(served.checks)[0].last || null : null
  /* A run the sheet holds is this version's only when it started after the
   * last edit the record holds: an edit after it means the files it examined
   * are not the files now (the record's own timestamps, compared typed). */
  const lastEditMs = edits.reduce((m, e) => { const t = msOf(e && e.ts); return t != null && (m == null || t > m) ? t : m }, null)
  const current = (r) => r && msOf(r.started_at) != null && (lastEditMs == null || msOf(r.started_at) >= lastEditMs)
  const endedHere = last && last.outcome !== 'running' && last.outcome !== 'not_run' ? last : null
  const endedNow = endedHere && current(endedHere) ? endedHere : null
  /* the newer of the snapshot's re-check and the sheet's own */
  const sheetIsNewer = endedNow && (!checkedRun || (checkedRun.runKey && checkedRun.runKey === endedNow.run)
    || msOf(checkedRun.ts) == null || msOf(endedNow.started_at) > msOf(checkedRun.ts))
  const sheetRun = (state) => ({
    state,
    step: null,
    at: clock(endedNow.recorded_at),
    command: null,
    version: V,
    exit: num(endedNow.exit_code),
    passed: num(endedNow.tests_passed),
    failed: num(endedNow.tests_failed),
    durationS: secondsBetween(endedNow.started_at, endedNow.recorded_at),
    elapsedS: secondsBetween(endedNow.started_at, endedNow.recorded_at),
    output: failuresOf(endedNow).lines,
    /* the failing tests the record names (data/run.js failuresOf, typed fields only) */
    failures: failuresOf(endedNow).lines.map((_, i) => ({ line: i, test: failuresOf(endedNow).tests[i] || null })),
    failuresParsed: true,
    cleanupVerified: endedNow.cleanup_verified === true,
    cancelledRecords: 1,
  })

  /* C30 R-RUN: what this Change's running check has printed so far, and its elapsed time from the
   * run's own recorded start, as the sheet serves them (data/run.js liveOutputOf). They stand in
   * for the snapshot's only on that same run; where the sheet serves no output, none is drawn and
   * `outputServed` says so. */
  const printed = liveOutputOf(served)
  const runningForm = (base, runKey) => {
    const mine = printed && (runKey == null || runKey === printed.run) ? printed : null
    const lines = mine && mine.lines
    return {
      ...base,
      elapsedS: mine && mine.elapsedS != null ? mine.elapsedS : base.elapsedS,
      output: lines ? lines.slice(-RUNNING_LINES) : base.output || [],
      outputServed: Boolean(lines),
      outputCut: Boolean(lines && (mine.cut || lines.length > RUNNING_LINES)),
    }
  }

  let run
  if (runningRun) run = runningForm({ state: 'running', ...runOf(runningRun) }, runningRun.runKey || '')
  else if (sheetReady && (sheetReady.running === 'running' || sheetReady.running === 'starting')) {
    run = runningForm({ state: 'running', version: V, elapsedS: secondsBetween((job && job.started_at) || (last && last.started_at), now.toISOString()), output: [] }, null)
  } else if (sheetIsNewer && endedNow.outcome === 'passed') run = sheetRun('passed')
  else if (sheetIsNewer && endedNow.outcome === 'failed') run = sheetRun('failed')
  else if (sheetIsNewer && endedNow.outcome === 'cancelled') run = sheetRun('stopped')
  else if (checkedRun && checkedRun.version === V && passedRun(checkedRun)) run = { state: 'passed', ...runOf(checkedRun) }
  else if (checkedRun && checkedRun.version === V && failedRun(checkedRun)) run = { state: 'failed', ...runOf(checkedRun) }
  else run = { state: 'unavailable' }

  /* The last end the sheet holds that the run form does not draw, said in the
   * run sheet's own words (run-route.js `outcomeView`): checked nothing, tool
   * not found, interrupted — or any end from before the latest edit. */
  const lastWords = run.state !== 'unavailable' || !endedHere || !sheetReady ? null : (() => {
    const o = outcomeView(endedHere, { program: sheetReady.check ? sheetReady.check.program : '' })
    if (!o) return null
    return {
      kind: o.kind,
      current: endedHere === endedNow,
      sentence: 'The last check here: ' + o.title + '. ' + o.lines.join(' ') + (o.when ? ' It ended at ' + o.when + '.' : '')
        + (endedHere === endedNow ? '' : ' That was before the latest edit, so it says nothing about ' + V + '.'),
    }
  })()

  /* "After it was raised" (the ladder's rungs 2 and 3), from the record: the agent's own steps
   * recorded after the concern was raised — after the statement, or after the edit that moved its
   * proof when that came later — and the files edited after the edit where the proof stops. The
   * statement's own message is not later work, and a check ThruLine ran has no step: it is not the
   * agent's. The record does not say whether later work answers the concern; the screen says so. */
  const stepOf = (x) => (x && Number.isInteger(x.step) ? x.step : null)
  const marks = [stepOf(st), stepOf(movedEdit)].filter((n) => n != null)
  const raisedStep = marks.length ? Math.max(...marks) : null
  const agentSteps = edits.map(stepOf)
    .concat(runs.map(stepOf))
    .concat(list(snapshot.statements).filter((x) => x && !(st && x.messageId && x.messageId === st.messageId)).map(stepOf))
    .filter((n) => n != null)
  const sinceStep = movedEdit ? stepOf(movedEdit) : raisedStep
  const ladder = raisedStep == null ? null : {
    raisedStep,
    later: Array.from(new Set(agentSteps.filter((n) => n > raisedStep))).sort((a, b) => a - b),
    sinceStep,
    changed: sinceStep == null ? [] : edits.filter((e) => stepOf(e) != null && e.step > sinceStep).map((e) => ({ step: e.step, file: e.file })),
  }

  const recheck = snapshot.recheck || null
  const scope = list(recheck && recheck.scope)
  /* What Check it again runs: the tracked Change's own check when the server
   * names one (the button starts exactly that), else the command the record
   * shows the statement rested on. The terminal line names the same command. */
  const sheetCommand = sheetReady && sheetReady.check && sheetReady.check.command ? sheetReady.check.command : null
  const runner = sheetCommand || (recheck && recheck.runner) || null
  const runScope = sheetCommand ? [] : scope
  const command = runner ? [runner, ...runScope].join(' ') : null
  const covers = movedEdit ? scope.find((p) => String(movedEdit.file).startsWith(p)) || null : null

  /* Honest outcomes: a tally is drawn only when the record holds one of that kind. */
  /* …and only on the version in front of the person: a tally from an older version is not this check's */
  const all = finished(snapshot).filter((r) => r.version === V)
  const lastPass = [...all].reverse().find(passedRun) || null
  const lastFail = [...all].reverse().find(failedRun) || null
  const message = st ? list(snapshot.messages).find((m) => m && m.id === st.messageId) : null
  const notProven = list(snapshot.statements).filter((s) => s && s.proof && s.proof.verdict === 'not_proven').length
  const earlier = proof.earlier || null

  return {
    project: { id: snapshot.project && snapshot.project.id ? snapshot.project.id : null, name: snapshot.project ? snapshot.project.name : null, dir: snapshot.project ? snapshot.project.worktree || null : null },
    change: { title: snapshot.change ? snapshot.change.title : null },
    version: { id: V, commit: cur ? cur.commit || null : null, clean: cur ? cur.unsavedChanges === false : false },
    session: (() => {
      const s0 = list(snapshot.sessions)[0]
      /* the record holds when the session started; its day and end are not on the snapshot */
      return { day: null, start: (s0 && s0.startedAt) || (snapshot.change && snapshot.change.recordedFrom) || null, end: null }
    })(),
    agent: { name: (st && st.agent) || (list(snapshot.sessions)[0] || {}).agent || null },
    request: { text: snapshot.request ? snapshot.request.text || null : null },
    statement: st
      ? { text: st.plain || st.text, said: (message && (message.plain || message.text)) || st.plain || st.text, step: num(st.step), at: st.time || null, lastVersionBefore: st.versionBefore || null }
      : null,
    heldRun: held
      ? {
        step: num(held.step), command: held.command || null, version: held.version || null,
        versionsBehind: held.version && V ? versions.findIndex((v) => v.id === V) - versions.findIndex((v) => v.id === held.version) : null,
        exit: num(held.exitCode), passed: num(held.passed), failed: num(held.failed),
      }
      : null,
    movedEdit: movedEdit ? { verb: 'modify', file: movedEdit.file, step: num(movedEdit.step) } : null,
    check: {
      scope,
      covers,
      command,
      terminalCommand: terminalCommand({ changeId: tracked ? tracked.id : null, runner, scope: runScope }),
      environment: sheetReady && sheetReady.technical ? sheetReady.technical.environment || null : null,
      consentGranted: sheetReady ? !sheetReady.permission.needed : null,
    },
    concern: { raisedAt: earlier && earlier.verdict === 'not_proven' ? earlier.at : proof.verdict === 'not_proven' && snapshot.view ? snapshot.view.readAt : null },
    criteria: snapshot.criteria && snapshot.criteria.accepted ? { version: snapshot.criteria.version, lines: num(snapshot.criteria.lineCount) } : null,
    record: { statements: list(snapshot.statements).length, notProven },
    storage: { bytes: snapshot.machine ? num(snapshot.machine.recordsBytes) : null },
    outcomeExamples: {
      passed: lastPass ? { passed: lastPass.passed, failed: lastPass.failed } : null,
      failed: lastFail ? { passed: lastFail.passed, failed: lastFail.failed } : null,
    },
    readAt: snapshot.view ? snapshot.view.readAt || null : null,
    recording: Boolean(snapshot.change && snapshot.change.recording),
    now: hhmm(now),
    steps: snapshot.change ? num(snapshot.change.stepCount) : null,
    run,
    ladder,
    /* the rung "Told the agent": the thread's last copy or pasted mark, from the record
     * (data/told.js; the project view's `told_agent`), or null when the person has done neither */
    told: snapshot.change && snapshot.change.told ? snapshot.change.told : null,
    /* what the controls act on: the tracked Change and its run sheet */
    /* the one reading every screen's controls share (data/run.js runActOf) */
    act: Object.assign(runActOf(tracked ? tracked.id : null, served), { lastWords, refusal: null }),
  }
}

export default selectRepairLoop
