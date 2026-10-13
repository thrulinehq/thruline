/* ⌘K on the record: the one Change snapshot (ui/src/hq/data/change.js) and the
 * project's served view → this screen's data contract (demo.js's shape).
 *
 * PURE. No fetch, no clock but `now`, no model, no network. Every fact is a
 * typed field of the snapshot or of `GET /api/hq/projects/<id>`; a step is the
 * snapshot's own step (a row's place in the thread's record), so ⌘K and every
 * Change screen name the same step for the same row.
 *
 * Ask answers the design's five questions (DESIGN-SPEC §3.08, census §3.08
 * row 5): did it finish, what changed after the last check, was anything
 * destructive, what is not proven, what should I tell the agent. Each answer
 * is built from the snapshot's typed fields — for a tracked Change too: the
 * one Change loader (data/use-change.js) folds its criteria, readiness and run
 * sheet into the snapshot. What they cannot ground is "The record doesn't
 * say", with what was looked for, what is missing and what would answer it.
 *
 *  - "Was anything destructive?" reads every command against the one list of
 *    destructive commands ThruLine keeps (live.js DESTRUCTIVE, the list
 *    `thruline explain` flags), and the edits the thread view types as file
 *    deletions (`opts.deletions`). It says the match is on the command's text:
 *    recorded, not judged.
 *  - "What should I tell the agent?" is the check sheet's own message
 *    (`opts.tell`, from tell.js, which runs Repair-Loop's composer): one draft,
 *    on both screens.
 */
import { UNTITLED } from '../../data/change.js'
import { fmtCount, countOf } from '../../data/count.js'
import { filesAsApprove } from '../../data/project.js'
import { destructivePhrase } from './live.js'

const passing = (r) => r && r.outcome === 'succeeded' && !(r.failed > 0)
const uniq = (xs) => Array.from(new Set(xs))
const last = (xs) => (xs.length ? xs[xs.length - 1] : null)
const msOf = (ts) => { const t = ts ? Date.parse(ts) : NaN; return Number.isNaN(t) ? null : t }
const steps = (...xs) => xs.filter((n) => typeof n === 'number')
const cap = (s) => (s ? s.charAt(0).toUpperCase() + s.slice(1) : s)
const textOf = (s) => (s.plain != null ? s.plain : s.text)

/** `b` happened after `a`: by step when both have one, else by the recorded instant. */
function after(b, a) {
  if (!a || !b) return false
  if (typeof a.step === 'number' && typeof b.step === 'number') return b.step > a.step
  const x = msOf(a.ts)
  const y = msOf(b.ts)
  return x != null && y != null && y > x
}

/** A run, named the way a person finds it: by its step, or — a check ThruLine ran, which has no step — by its time. */
function runName(r) {
  if (!r) return 'a test run'
  if (typeof r.step === 'number') return 'the test run at step ' + r.step
  return 'the check ThruLine ran' + (r.time ? ' at ' + r.time : '')
}

/** The project's threads, as Find sees them when no Change is open. Whose move each is, in the
 *  Inbox's own filing (C46 §1.1): a thread a ready Change files as the approve row is the person's
 *  move (`ready`, by thread id, from the project's readiness reading; data/your-move.js).
 *  NOT-APPROVAL-WORDS: a Change that IS ready makes its thread "your move"; no words here say why
 *  approval is off. */
export function threadItems(projectView, ready = null) {
  const threads = projectView && Array.isArray(projectView.threads) ? projectView.threads : []
  return threads.map((t) => ({
    id: 't:' + t.id, kind: 'Thread', threadId: t.id, step: null, text: t.title || UNTITLED,
    whoseMove: filesAsApprove(t, ready && ready[t.id]) ? 'you' : t.whose_move || null,
    files: (t.top_files || []).map((f) => f.path), at: t.last_activity_at || null,
  }))
}

/** The design's five questions, in the design's order. */
export const QUESTION_KEYS = Object.freeze(['finish', 'changed', 'destructive', 'notproven', 'tell'])

/**
 * @param {object|null} snap  changeSnapshot(...) for the open Change, or null
 * @param {object|null} projectView  GET /api/hq/projects/<id>
 * @param {{readAt?: string, tell?: object|null, deletions?: string[]|null, ready?: object|null}} [opts]
 *   tell       tell.js `tellOf(snap)`: the check sheet's message, or why there is none
 *   deletions  ids of the snapshot's edits the thread view types as deleting a
 *              file (`action: "delete"`); null when the thread view was not read
 *   ready      by thread id, the tracked Change ready for a decision on it (the
 *              project's readiness reading, data/your-move.js); null when not read
 */
export function commandFromChange(snap, projectView, opts = {}) {
  const project = projectView && projectView.project ? projectView.project : null
  const threads = threadItems(projectView, opts.ready || null)
  if (!snap) {
    return {
      live: true, open: false,
      project: { name: project ? project.name : null, path: project ? project.root : null },
      change: { title: null, agent: null },
      version: { id: null, current: true, commit: null, unsavedChanges: null },
      recording: null, readAt: opts.readAt || null, askedAt: opts.readAt || null,
      record: { steps: threads.length, window: { from: null, to: null }, hookInstalledAt: null, statements: null, statementsNeedingCheck: null, unit: 'threads' },
      storage: null, check: { command: null }, criteria: { proposed: null, accepted: 0 }, approval: { waitsOn: null },
      diff: { edits: null, files: null, filesChangedAfterLastCheck: null },
      proofStop: { finishedRuns: [], pendingSteps: [], changedSince: { file: null } },
      findable: threads, nearProofStop: threads.slice(0, 3).map((t) => t.id),
      questions: [], evidence: {}, answers: {}, story: null,
    }
  }

  const V = snap.currentVersion
  const vIndex = new Map((snap.versions || []).map((x, i) => [x.id, i]))
  const behind = (ver) => (vIndex.has(ver) && vIndex.has(V) ? Math.max(0, vIndex.get(V) - vIndex.get(ver)) : null)
  const cur = (snap.versions || []).find((x) => x.id === V) || null
  const runById = new Map(snap.runs.map((r) => [r.id, r]))
  const editById = new Map(snap.edits.map((e) => [e.id, e]))
  const tests = snap.runs.filter((r) => r.kind === 'test')
  const lastPass = last(tests.filter(passing))
  const finished = tests.filter((r) => r.status === 'finished')
  const afterPass = lastPass ? snap.edits.filter((e) => after(e, lastPass)) : []
  const changedAfter = uniq(afterPass.map((e) => e.file))
  const files = uniq(snap.edits.map((e) => e.file))
  const agent = (snap.sessions[0] && snap.sessions[0].agent) || null
  const agentName = agent || 'the agent'
  const times = [snap.request && snap.request.time].concat(snap.edits.map((e) => e.time), snap.runs.map((r) => r.time), snap.statements.map((s) => s.time)).filter(Boolean)
  const byStep = [snap.request].concat(snap.edits, snap.runs, snap.statements).filter((x) => x && x.step != null && x.time).sort((a, b) => a.step - b.step)
  const window = { from: byStep.length ? byStep[0].time : (times[0] || null), to: byStep.length ? last(byStep).time : null }
  const windowText = window.from && window.to ? ', ' + window.from + ' to ' + window.to : ''
  const checkable = snap.statements.filter((s) => s.verifier)
  const open = checkable.filter((s) => s.proof && s.proof.verdict !== 'proven')

  /* ---- Find: every step of this Change, then its files, then the project's threads ---- */
  const findable = []
  if (snap.request) findable.push({ id: 'p:' + snap.request.id, kind: 'Request', step: snap.request.step, text: snap.request.text, by: 'you', at: snap.request.time })
  for (const e of snap.edits) {
    findable.push({ id: 'e:' + e.id, kind: 'Edit', step: e.step, op: 'edit', file: e.file, versionAfter: e.makesVersion, afterLastCheck: !!(lastPass && after(e, lastPass)), at: e.time })
  }
  for (const r of snap.runs) {
    if (r.kind === 'test') {
      findable.push({ id: 'r:' + r.id, kind: 'Run', step: r.step, command: r.command, passed: r.passed, failed: r.failed, outcome: r.outcome, exit: r.exitCode, version: r.version, versionsBehind: behind(r.version) })
    } else {
      findable.push({ id: 'r:' + r.id, kind: 'Command', step: r.step, command: r.command, result: r.outcome === 'unknown' ? null : r.outcome, destructive: destructivePhrase(r.verbatim || r.command) })
    }
  }
  for (const s of snap.statements) {
    findable.push({ id: 's:' + s.id, kind: 'Said', step: s.step, text: textOf(s), agent: s.agent || agent, at: s.time, statement: s.verifier && s.proof ? { state: s.proof.verdict, version: V } : null })
  }
  for (const f of files) {
    const at = snap.edits.filter((e) => e.file === f)
    findable.push({ id: 'f:' + f, kind: 'File', path: f, changedAt: at.map((e) => e.step).filter((n) => n != null), runAt: [], uncovered: changedAfter.includes(f), lastVersion: last(at).makesVersion || null })
  }
  for (const t of threads) if (t.threadId !== snap.change.threadId) findable.push(t)

  const near = []
  const lastAfter = last(afterPass)
  if (lastAfter) near.push('e:' + lastAfter.id)
  if (changedAfter[0]) near.push('f:' + changedAfter[0])
  if (last(tests)) near.push('r:' + last(tests).id)
  if (!near.length && snap.statements.length) near.push('s:' + last(snap.statements).id)

  /* ---- evidence an answer may cite: the same rows, by id ---- */
  const evidence = {}
  const evRun = (r) => {
    const id = 'r:' + r.id
    evidence[id] = {
      kind: r.kind === 'test' ? 'Run' : 'Command', step: r.step, text: r.command, passed: r.passed, failed: r.failed, outcome: r.outcome,
      result: r.outcome === 'unknown' ? null : r.outcome, version: r.version, versionsBehind: behind(r.version), destructive: destructivePhrase(r.verbatim || r.command),
      detail: cap(runName(r)) + (r.version ? ', on version ' + r.version : '') + '. ' + (r.outcome === 'unknown' ? 'No result is recorded for it.' : 'It ' + r.outcome + (r.exitCode != null ? ' with exit code ' + r.exitCode : '') + '.'),
    }
    return id
  }
  const evEdit = (e, run) => {
    const id = 'e:' + e.id
    const late = !!(run && after(e, run))
    evidence[id] = { kind: 'Edit', step: e.step, text: 'edit ' + e.file, afterRun: late, at: e.time, versionAfter: e.makesVersion,
      detail: e.file + ' changed' + (e.step != null ? ' at step ' + e.step : '') + (e.time ? ', ' + e.time : '') + (late ? ', after ' + runName(run) + '.' : '.') + (e.linesAdded != null ? ' +' + fmtCount(e.linesAdded) + ' −' + fmtCount(e.linesRemoved ?? 0) + ' lines.' : '') }
    return id
  }
  const evSaid = (s) => {
    const id = 's:' + s.id
    const who = s.agent || agentName
    evidence[id] = { kind: 'Said', step: s.step, text: textOf(s), quote: true, agent: s.agent || agent || 'The agent', at: s.time, detail: cap(who) + '’s own words' + (s.time ? ' at ' + s.time : '') + ', as recorded in the transcript.' }
    return id
  }
  const silent = (lookedFor, missing, would) => ({ unanswered: true, lookedFor, missing, would })

  const answers = {}

  /* ---- 1. Did it finish? — the statement the Brief is about, and what it rests on ---- */
  const B = snap.statements.find((s) => s.id === snap.brief.statementId) || null
  if (!B) {
    answers.finish = silent(
      'A statement from ' + agentName + ' that the work is finished, in what it said in this Change' + windowText + '.',
      'Nothing ' + agentName + ' said in this Change claims the work is finished, tested or built.',
      'A message from ' + agentName + ' saying it is done, recorded in this Change.',
    )
  } else {
    const p = B.proof || {}
    const rests = p.restsOn && p.restsOn.runId ? runById.get(p.restsOn.runId) || null : null
    const ev = [evSaid(B)]
    const said = (tail) => ({ pre: (B.agent || agentName) + ' said ', quote: textOf(B), post: (B.time ? ' at ' + B.time : '') + tail, steps: steps(B.step) })
    if (p.verdict === 'proven') {
      if (rests) ev.push(evRun(rests))
      answers.finish = {
        state: 'proven', word: 'Proven', about: { version: V },
        lines: [said('.')].concat(rests ? [{ pre: cap(runName(rests)) + ' passed' + (rests.version ? ' on ' + rests.version : '') + ', and no edit is recorded after it.', steps: steps(rests.step) }] : []),
        evidence: ev, looked: false, action: 'none', reason: 'Approval rests on what counts as done, not on this statement.',
      }
    } else if (p.verdict === 'contradicted') {
      const fail = p.recheck && p.recheck.runId ? runById.get(p.recheck.runId) || null : rests
      if (fail) ev.push(evRun(fail))
      answers.finish = {
        state: 'contradicted', word: 'Not proven', about: { opposite: true },
        lines: [said('.')].concat(fail ? [{ pre: cap(runName(fail)) + ' failed' + (fail.failed > 0 ? ': ' + fmtCount(fail.failed) + ' failed, ' + fmtCount(fail.passed || 0) + ' passed' : '') + (fail.version ? ', on ' + fail.version : '') + '.', steps: steps(fail.step) }] : []),
        evidence: ev, looked: true, action: 'check',
      }
    } else if (p.verdict === 'not_proven') {
      const since = p.since && p.since.editId ? editById.get(p.since.editId) || null : null
      const running = p.recheck && p.recheck.status === 'running'
      const lines = []
      if (rests && passing(rests)) {
        ev.push(evRun(rests))
        lines.push(said(', and ' + runName(rests) + ', the run it rests on, passed' + (rests.version ? ' on ' + rests.version : '') + '.'))
        lines[0].steps = steps(B.step, rests.step)
      } else if (rests) {
        ev.push(evRun(rests))
        lines.push(said('.'), { pre: cap(runName(rests)) + ', the run it rests on, has no result recorded.', steps: steps(rests.step) })
      } else {
        lines.push(said('.'), { pre: 'No test run before it is recorded in this Change.', steps: [] })
      }
      if (since) {
        ev.push(evEdit(since, rests))
        lines.push({ pre: 'It then edited ', code: since.file, post: '. No test run has passed' + (V ? ' on ' + V : '') + ' since.', steps: steps(since.step) })
      }
      if (running) lines.push({ pre: 'A check' + (V ? ' of ' + V : '') + ' is running now.', steps: [] })
      answers.finish = { state: 'not_proven', word: 'Not proven', about: { version: V }, lines, evidence: ev, looked: true, action: 'check' }
    } else {
      const crit = snap.criteria || {}
      answers.finish = {
        state: 'recorded', word: 'Recorded', about: {},
        lines: [said('.'), { pre: crit.accepted ? 'It is checked against what counts as done' + (crit.version ? ' (' + crit.version + ')' : '') + ', not by a test run.' : 'It is checked against what counts as done, which isn’t accepted yet.', steps: [] }],
        evidence: ev, looked: false, action: 'none', reason: 'Approval rests on what counts as done, not on this statement.',
      }
    }
  }

  /* ---- 2. What changed after the last check? — edits after the last passing test run ---- */
  if (!lastPass) {
    answers.changed = silent(
      'A test run that passed in this Change, and the edits after it.',
      'No test run in this Change has passed, so there is no last check to compare with.' + (tests.length ? ' ' + countOf(tests.length, 'test run is', 'test runs are') + ' recorded.' : ''),
      'A test run that passes, recorded in this Change.',
    )
  } else {
    const about = typeof lastPass.step === 'number' ? { run: lastPass.step } : { checkAt: lastPass.time || '' }
    const runId = evRun(lastPass)
    if (changedAfter.length) {
      const shown = changedAfter.slice(0, 3).map((f) => last(afterPass.filter((e) => e.file === f)))
      const moved = lastPass.version && V && lastPass.version !== V
      answers.changed = {
        state: 'changed', word: fmtCount(changedAfter.length) + (changedAfter.length === 1 ? ' file changed after' : ' files changed after'), about,
        lines: shown.map((e) => ({ code: e.file, post: ' was changed' + (e.time ? ' at ' + e.time : '') + ', after ' + runName(lastPass) + ' passed' + (lastPass.version ? ' on ' + lastPass.version : '') + '.', steps: steps(e.step) }))
          .concat(changedAfter.length > 3 ? [{ pre: fmtCount(changedAfter.length - 3) + ' more files changed after it too.', steps: [] }] : [])
          .concat([{ pre: moved ? 'The version moved from ' + lastPass.version + ' to ' + V + ', and no test run has passed since.' : 'No test run has passed since.', steps: [] }]),
        evidence: shown.map((e) => evEdit(e, lastPass)).concat(runId), looked: false, action: 'check', seeChanged: true,
      }
    } else {
      answers.changed = {
        state: 'neutral', word: 'Nothing changed after', about,
        lines: [{ pre: 'No edit is recorded after ' + runName(lastPass) + ', which passed' + (lastPass.version ? ' on ' + lastPass.version : '') + '.', steps: steps(lastPass.step) }],
        evidence: [runId], looked: false, action: 'none', reason: 'Only the edits the agent made are in the record. A change made outside it is not.',
      }
    }
  }

  /* ---- 3. Was anything destructive? — commands on the one list, and file deletions ---- */
  {
    const hits = snap.runs.map((r) => ({ r, phrase: destructivePhrase(r.verbatim || r.command) })).filter((x) => x.phrase)
    const deletionsRead = Array.isArray(opts.deletions)
    const dels = deletionsRead ? opts.deletions.map((id) => editById.get(id)).filter(Boolean) : []
    const looked = 'Looked at ' + countOf(snap.runs.length, 'command', 'commands') + ' and ' + countOf(snap.edits.length, 'edit', 'edits') + ' in this Change' + windowText + '.'
    if (!hits.length && !dels.length) {
      answers.destructive = {
        state: 'neutral', word: 'None on the list', about: {},
        lines: [
          { pre: looked, steps: [] },
          { pre: 'No command contains a phrase on ThruLine’s list of destructive commands (rm -rf, git reset --hard, git push --force, drop table and others)' + (deletionsRead ? ', and no edit deletes a file.' : '. Whether an edit deleted a file was not read.'), steps: [] },
        ],
        evidence: [], looked: false, action: 'none',
        reason: 'ThruLine matches each command’s text against that list, and nothing more. It doesn’t judge what a command did.',
      }
    } else {
      const pending = hits.filter((x) => x.r.status === 'no_result')
      const shown = hits.slice(-3)
      const ended = (r) => (r.status === 'no_result' ? ' It has no result yet.' : r.outcome === 'failed' ? ' It failed.' : ' It succeeded.')
      const lines = shown.map(({ r, phrase }) => ({
        pre: (typeof r.step === 'number' ? 'Step ' + r.step : 'A command') + (r.status === 'no_result' ? ' asks to run ' : ' ran '),
        code: r.command, post: '. Its text contains “' + phrase + '”.' + ended(r), steps: steps(r.step),
      }))
      if (hits.length > 3) lines.push({ pre: fmtCount(hits.length - 3) + ' more commands on the list ran earlier.', steps: [] })
      for (const e of dels.slice(-3)) lines.push({ pre: (typeof e.step === 'number' ? 'Step ' + e.step : 'An edit') + ' deleted ', code: e.file, post: '.', steps: steps(e.step) })
      if (hits.length && hits.every((x) => x.r.status === 'no_result') && !dels.length) lines.push({ pre: 'Nothing on the list has run in the recorded part of this Change' + windowText + '.', steps: [] })
      const n = hits.length + dels.length
      answers.destructive = {
        state: pending.length ? 'no_result' : 'neutral',
        word: pending.length ? 'No result yet' : fmtCount(n) + (n === 1 ? ' destructive step' : ' destructive steps'),
        about: pending.length && typeof last(pending).r.step === 'number' ? { step: last(pending).r.step } : {},
        lines,
        evidence: shown.map(({ r }) => evRun(r)).concat(dels.slice(-3).map((e) => evEdit(e, null))),
        looked: false, action: 'none',
        reason: pending.length
          ? 'ThruLine can’t answer this for you. If ' + agentName + ' is asking permission to run it, only you can answer that, in ' + agentName + '.'
          : 'Listed because the record shows it: a command’s text on ThruLine’s list of destructive commands, or an edit that deletes a file. ThruLine doesn’t know what was lost.',
      }
    }
  }

  /* ---- 4. What is not proven? — every statement on this version ---- */
  {
    const recorded = snap.statements.filter((s) => !s.verifier)
    const crit = snap.criteria || {}
    const why = (s) => {
      const p = s.proof || {}
      const rests = p.restsOn && p.restsOn.runId ? runById.get(p.restsOn.runId) || null : null
      const since = p.since && p.since.editId ? editById.get(p.since.editId) || null : null
      if (p.verdict === 'contradicted') {
        const fail = p.recheck && p.recheck.runId ? runById.get(p.recheck.runId) || null : rests
        return { post: ' ' + cap(fail ? runName(fail) : 'a test run') + ' after it failed.', steps: steps(s.step, fail && fail.step), ev: fail ? [evRun(fail)] : [] }
      }
      if (rests && passing(rests) && since) return { post: ' The run it rests on passed' + (rests.version ? ' on ' + rests.version : '') + ', and ' + since.file + ' changed after it.', steps: steps(rests.step, since.step), ev: [evRun(rests), evEdit(since, rests)] }
      if (rests && !passing(rests)) return { post: ' The run it rests on has no result recorded.', steps: steps(s.step, rests.step), ev: [evRun(rests)] }
      return { post: ' No test run before it is recorded.', steps: steps(s.step), ev: [] }
    }
    if (!snap.statements.length) {
      answers.notproven = {
        state: 'neutral', word: 'Nothing claimed', about: {},
        lines: [{ pre: 'Nothing ' + agentName + ' said in this Change claims the work is finished, tested or built, so nothing waits on proof.', steps: [] }],
        evidence: [], looked: false, action: 'none', reason: 'Approval rests on what counts as done, not on statements.',
      }
    } else {
      const lines = []
      const ev = []
      for (const s of open.slice(-3)) {
        const w = why(s)
        ev.push(evSaid(s), ...w.ev)
        lines.push({ quote: textOf(s), post: w.post, steps: w.steps })
      }
      for (const s of recorded.slice(-Math.max(0, 3 - lines.length))) {
        lines.push({ quote: textOf(s), post: ' is recorded. It is checked against what counts as done' + (crit.accepted ? (crit.version ? ' (' + crit.version + ')' : '') + ', not by a test run.' : ', which isn’t accepted yet.'), steps: steps(s.step) })
      }
      const more = open.length + recorded.length - lines.length
      if (more > 0) lines.push({ pre: fmtCount(more) + ' more ' + (more === 1 ? 'statement is' : 'statements are') + ' listed in Every statement.', steps: [] })
      answers.notproven = open.length
        ? { state: 'not_proven', word: 'Not proven', about: { statements: { notProven: open.length, of: snap.statements.length }, version: V }, lines, evidence: uniq(ev), looked: true, action: 'check' }
        : checkable.length
          ? { state: 'proven', word: 'Proven', about: { statements: { proven: checkable.length, of: snap.statements.length }, version: V },
            lines: [{ pre: 'Every statement that names tests or a build has a passing test run' + (V ? ' on ' + V : '') + ' after the last edit.', steps: steps(...checkable.map((s) => s.step).slice(-3)) }].concat(lines),
            evidence: checkable.slice(-2).map(evSaid), looked: false, action: 'none', reason: 'Approval rests on what counts as done, not on these statements.' }
          : { state: 'recorded', word: 'Recorded', about: {}, lines, evidence: [], looked: false, action: 'none', reason: 'Approval rests on what counts as done, not on these statements.' }
    }
  }

  /* ---- 5. What should I tell the agent? — the check sheet's own message ---- */
  {
    const t = opts.tell || null
    if (t && t.draft) {
      const quoted = 'It quotes what ' + agentName + ' said' + (t.heldStep != null ? ' and the run it rests on' : '') + (t.movedFile ? ', and names ' + t.movedFile + ', changed after that' : '') + '.'
      answers.tell = {
        state: 'prepared', word: 'A message is ready', about: { prepared: true },
        lines: [{ pre: quoted, steps: steps(t.statementStep, t.heldStep, t.movedStep) }]
          .concat(t.command ? [{ pre: 'It asks for ', code: t.command, post: (V ? ' on ' + V : '') + '. Copying it doesn’t send it.', steps: [] }] : [{ pre: 'Copying it doesn’t send it.', steps: [] }]),
        evidence: [], looked: false, action: 'tell', draft: t.draft,
      }
    } else {
      answers.tell = {
        state: 'neutral', word: 'No message to prepare', about: {},
        lines: [], evidence: [], looked: false, action: 'none',
        reason: (t && t.reason) || 'Nothing the agent said in this work claims it is finished, so there is nothing to tell the agent yet.',
      }
    }
  }

  const questions = [
    { key: 'finish', text: 'Did it finish this Change?', reads: 'reads what it said and the run it rests on' },
    { key: 'changed', text: 'What changed after the last check?', reads: lastPass ? 'reads edits and versions after ' + (typeof lastPass.step === 'number' ? 'step ' + lastPass.step : 'the check at ' + (lastPass.time || 'its end')) : 'reads edits after the last passing test run' },
    { key: 'destructive', text: 'Was anything destructive?', reads: 'reads every command and edit' },
    { key: 'notproven', text: 'What is not proven?', reads: snap.statements.length === 0 ? 'reads what it said in this Change' : 'reads ' + (snap.statements.length === 1 ? 'the statement' : snap.statements.length === 2 ? 'both statements' : 'all ' + fmtCount(snap.statements.length) + ' statements') + (V ? ' on ' + V : '') },
    { key: 'tell', text: 'What should I tell the agent?', reads: 'builds a message from where the proof stops' },
  ]

  const story = B ? { text: textOf(B), agent: B.agent || agent, step: B.step, at: B.time, state: B.proof ? B.proof.verdict : 'recorded', version: V, why: null, lastVersionBefore: B.versionBefore } : null
  return {
    live: true, open: true,
    project: { name: snap.project.name, path: snap.project.worktree },
    change: { title: snap.change.title, agent, id: snap.change.id, threadId: snap.change.threadId },
    version: { id: V, current: true, commit: cur ? cur.commit : null, unsavedChanges: cur ? cur.unsavedChanges : null },
    recording: snap.change.whoseMove === 'agent',
    readAt: snap.view.readAt, askedAt: snap.view.readAt,
    record: {
      steps: snap.change.stepCount,
      window,
      hookInstalledAt: null,
      statements: snap.statements.length,
      statementsNeedingCheck: open.length,
    },
    storage: null,
    check: { command: snap.recheck.runner },
    criteria: { proposed: snap.criteria.proposedCount, accepted: snap.criteria.accepted ? (snap.criteria.lineCount || 0) : 0 },
    approval: { waitsOn: null },
    diff: { edits: snap.edits.length, files: files.length, filesChangedAfterLastCheck: lastPass ? changedAfter.length : null },
    proofStop: {
      finishedRuns: finished.slice(-2).map((r) => ({ step: r.step, version: r.version, time: r.time })),
      pendingSteps: tests.filter((r) => r.status === 'no_result' && typeof r.step === 'number').slice(-1).map((r) => r.step),
      changedSince: { file: changedAfter[0] || null },
    },
    findable, nearProofStop: near,
    questions, evidence, answers,
    story,
  }
}
