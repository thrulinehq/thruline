/* The whole story, from the ONE Change snapshot (W2-E).
 *
 *   wholeStoryFrom(snapshot, tracked, options) → the page's live data
 *
 * `snapshot` is `data/change.js`'s `changeSnapshot(threadView, tracked, now)`,
 * whose shape is `screens/Change-Brief/demo.js`'s DEMO. `tracked` is
 * `{change, hq, proposals}` — the tracked Change's served document and its
 * `deriveChangeModel` result — or null. Nothing here re-derives a fact the
 * snapshot or the tracked model holds: verdicts, criteria, readiness and
 * decisions are read, never recomputed. What this file does is choose and
 * group them for this page: the story bands, the statement the page leads
 * with, the findings, the decisions and what the record does not hold.
 *
 * PURE: no fetch, no window, no clock but `options.now`. Every count is a
 * typed field of the snapshot or the length of a list it holds; a count the
 * record does not hold is null, never 0.
 */

import { homePath } from '../../data/paths.js'
import { objectedStatements } from '../../data/objections.js'

const list = (x) => (Array.isArray(x) ? x : [])
const str = (x) => (typeof x === 'string' && x.trim() ? x : null)
const msOf = (ts) => {
  const t = ts ? Date.parse(ts) : NaN
  return Number.isNaN(t) ? null : t
}
const pad = (n) => String(n).padStart(2, '0')
export const hhmm = (ts) => {
  const t = msOf(ts)
  if (t === null) return null
  const d = new Date(t)
  return pad(d.getHours()) + ':' + pad(d.getMinutes())
}
const ymd = (ts) => {
  const t = msOf(ts)
  if (t === null) return null
  const d = new Date(t)
  return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate())
}
/** A time in a file that leaves this page: UTC, with Z, never an offset. */
export const utc = (ts) => {
  const t = msOf(ts)
  return t === null ? null : new Date(t).toISOString().replace(/\.\d{3}Z$/, 'Z')
}

/** A path the record holds, as a person reads it: inside the project, relative
 *  to it (".thruline/decisions/e71c5b0.json"); elsewhere, the home folder as ~. */
export function inProject(path, root) {
  if (typeof path !== 'string' || !path) return null
  const p = homePath(path)
  const r = typeof root === 'string' && root ? root.replace(/\/+$/, '') : null
  return r && p.startsWith(r + '/') ? p.slice(r.length + 1) : p
}

/** At most this many version bands are drawn; earlier ones are summed into one band. */
export const MAX_VERSION_BANDS = 2
/** At most this many test runs are listed; the page says how many it left out. */
export const MAX_RUNS = 12

const isTest = (r) => r && r.kind === 'test'
const resultOf = (r) => (r.passed == null && r.failed == null
  ? { passed: null, failed: null, outcome: r.status === 'no_result' ? null : r.outcome || null }
  : { passed: r.passed, failed: r.failed, outcome: r.outcome || null })

/**
 * @param {object} snap  a Change snapshot (Change-Brief DEMO shape)
 * @param {?object} tracked  `{change, hq, proposals}` or null
 * @param {{now?: number, storageBytes?: ?number}} [options]
 */
export function wholeStoryFrom(snap, tracked, options = {}) {
  const S = snap || {}
  const hq = tracked && tracked.hq
  const change = S.change || {}
  const cur = S.currentVersion || null
  const vrow = list(S.versions).find((v) => v.id === cur) || null
  const edits = list(S.edits)
  const runs = list(S.runs)
  const tests = runs.filter(isTest)
  const statements = list(S.statements)
  const stepTs = [...edits, ...runs].map((x) => x.ts).filter(Boolean).sort()
  const firstTs = stepTs[0] || null
  const lastTs = stepTs[stepTs.length - 1] || null
  const fileAtStep = (step) => { const e = edits.find((x) => x.step === step); return e ? e.file : null }
  const runOf = (id) => runs.find((r) => r.id === id) || null

  /* ---- the bands: the request, then each version's edits, last test run and statements ---- */
  const byVersion = new Map()
  for (const v of list(S.versions)) byVersion.set(v.id, { version: v.id, now: v.id === cur, edits: [], tests: [], statements: [] })
  for (const e of edits) if (byVersion.has(e.makesVersion)) byVersion.get(e.makesVersion).edits.push(e)
  for (const r of tests) if (byVersion.has(r.version)) byVersion.get(r.version).tests.push(r)
  for (const st of statements) if (byVersion.has(st.versionBefore)) byVersion.get(st.versionBefore).statements.push(st)
  const vbands = [...byVersion.values()].filter((b) => b.edits.length || b.tests.length || b.statements.length || b.now)
  const summarise = (b) => {
    const ev = []
    if (b.edits.length) ev.push({ kind: 'edits', count: b.edits.length, from: b.edits[0].step, to: b.edits[b.edits.length - 1].step })
    const lastTest = b.tests[b.tests.length - 1]
    if (lastTest) ev.push({ kind: 'run', step: lastTest.step, ...resultOf(lastTest), restsOn: false })
    if (b.statements.length) ev.push({ kind: 'statements', count: b.statements.length, step: b.statements[b.statements.length - 1].step, first: b.statements[0].step })
    const at = (e) => (e.kind === 'edits' ? e.from : e.kind === 'statements' ? e.first : e.step)
    return ev.sort((x, y) => at(x) - at(y))
  }
  const shown = vbands.slice(-MAX_VERSION_BANDS)
  const hidden = vbands.slice(0, Math.max(0, vbands.length - MAX_VERSION_BANDS))
  const timeline = []
  if (firstTs) timeline.push({ kind: 'unrecorded', events: [{ kind: 'gap', before: hhmm(firstTs) }] })
  if (S.request) timeline.push({ kind: 'session', events: [{ kind: 'prompt', step: S.request.step }] })
  if (hidden.length) {
    const n = hidden.reduce((a, b) => a + b.edits.length, 0)
    timeline.push({ kind: 'earlier', versions: hidden.length, events: [{ kind: 'edits', count: n, from: hidden[0].edits[0] ? hidden[0].edits[0].step : null, to: null }] })
  }
  for (const b of shown) timeline.push({ kind: 'version', version: b.version, current: b.now, events: summarise(b) })

  /* ---- the statement the page leads with: the snapshot's own brief statement ---- */
  const brief = statements.find((s) => S.brief && s.id === S.brief.statementId && s.verifier) || [...statements].reverse().find((s) => s.verifier) || null
  let s1 = null
  if (brief) {
    const pf = brief.proof || {}
    const rested = pf.restsOn && pf.restsOn.runId ? runOf(pf.restsOn.runId) : null
    const sinceEdit = pf.since && pf.since.editId ? edits.find((e) => e.id === pf.since.editId) : null
    const failedRun = pf.recheck && pf.recheck.status === 'failed' && pf.recheck.runId ? runOf(pf.recheck.runId) : null
    s1 = {
      agent: brief.agent,
      quote: (brief.plain != null ? brief.plain : brief.text), // the words inside ❝ ❞ (data/change.js `plain`)
      step: brief.step,
      time: brief.time,
      verdict: pf.verdict || null,
      stopsAt: pf.stopsAt == null ? null : pf.stopsAt,
      version: pf.version || cur,
      rested: rested ? { step: rested.step, version: rested.version, command: rested.command, time: rested.time, ...resultOf(rested), scope: list(rested.scope) } : null,
      since: sinceEdit ? { file: sinceEdit.file, step: sinceEdit.step } : null,
      failedRun: failedRun ? { step: failedRun.step, ...resultOf(failedRun) } : null,
      match: pf.restsOn && pf.restsOn.match ? pf.restsOn.match : null,
      versionBefore: brief.versionBefore,
    }
  }
  const doneClaim = [...statements].reverse().find((s) => !s.verifier && (s.claimKind === 'task_complete' || s.claimKind === 'done' || s.claimKind === 'fixed')) || null
  const s2 = doneClaim ? { agent: doneClaim.agent, quote: (doneClaim.plain != null ? doneClaim.plain : doneClaim.text), step: doneClaim.step, time: doneClaim.time, word: doneClaim.claimKind === 'fixed' ? 'fixed' : 'done' } : null
  const other = [...statements].reverse().find((s) => s !== brief && s !== doneClaim) || null
  const s3 = other ? { quote: (other.plain != null ? other.plain : other.text), step: other.step } : null

  /* ---- criteria: only from the tracked Change ---- */
  const crit = S.criteria || {}
  const criteria = {
    accepted: Boolean(crit.accepted),
    version: crit.version || null,
    acceptedAt: crit.acceptedAt || null,
    lineCount: crit.lineCount == null ? null : crit.lineCount,
    proven: crit.provenOn && crit.provenOn.count != null ? crit.provenOn.count : null,
    /* C30 P0-1 — each line's own state and the version its proof names, as
     * deriveChangeModel holds them; `on` is the version the model measured.
     * Nothing is inferred: a line with no state carries none. */
    on: hq && hq.version ? str(hq.version.short) : null,
    lines: hq ? list(hq.criteria).map((c) => ({
      n: c.n,
      text: c.text,
      word: c.word || null,
      state: str(c.state),
      proven: c.state === 'proven' && c.proven && str(c.proven.version) ? { version: c.proven.version, scope: list(c.proven.scope) } : null,
    })) : [],
  }

  /* ---- findings: the snapshot's typed findings, and the tracked Change's own ---- */
  const findings = []
  for (const f of list(S.findings)) {
    if (f.kind === 'edit_after_run') findings.push({ text: (fileAtStep(f.editStep) || 'A file') + ' changed after the run the statement rests on.', where: 'step ' + f.editStep, settled: Boolean(f.resolved), files: [fileAtStep(f.editStep)].filter(Boolean) })
    else if (f.kind === 'no_check') findings.push({ text: 'No passing test ran after ' + f.file + ' changed.', where: 'edited after the last passing run', settled: Boolean(f.resolved), files: [f.file].filter(Boolean) })
    else if (f.kind === 'run_outcomes') {
      const fs = list(f.failedSteps), ps = list(f.passedSteps)
      if (fs.length && ps.length && Math.max(...ps) > Math.max(...fs)) findings.push({ text: 'A test run failed, then a later one passed.', where: 'step ' + Math.max(...fs) + ' → step ' + Math.max(...ps), settled: false, nothing: true })
      else if (fs.length) findings.push({ text: 'The latest test run that ended failed.', where: 'step ' + Math.max(...fs), settled: false })
    } else if (f.kind === 'no_result') findings.push({ text: 'A test run has no result.', where: 'step ' + f.step, settled: false })
  }
  for (const f of list(hq && hq.packet && hq.packet.findings)) findings.push({ text: f.record, where: null, settled: false, resolution: str(f.resolution) })

  /* ---- decisions: the tracked Change's own record ---- */
  const decisions = []
  const doc = tracked && tracked.change && tracked.change.decisions
  const acc = tracked && tracked.change && tracked.change.accepted_scope && tracked.change.accepted_scope.acceptance
  if (acc && acc.accepted_at) decisions.push({ kind: 'accept', ts: acc.accepted_at, criteriaVersion: criteria.version, statements: criteria.lineCount })
  for (const r of list(doc && doc.history)) {
    decisions.push({ kind: r.act === 'withdrawn' ? 'withdraw' : 'approve', ts: r.decided_at, version: r.target_revision, by: r.actor, criteriaVersion: r.scope_version != null ? 'v' + r.scope_version : null, file: inProject(r.record_path, S.project && S.project.worktree) })
  }
  decisions.sort((a, b) => (msOf(a.ts) ?? 0) - (msOf(b.ts) ?? 0))
  const standing = doc && doc.standing ? doc.standing : null
  const approval = standing ? { version: standing.target_revision, by: standing.actor, ts: standing.decided_at, criteriaVersion: standing.scope_version != null ? 'v' + standing.scope_version : null } : null

  /* ---- what the record does not hold ---- */
  const gaps = []
  if (firstTs) gaps.push({ kind: 'before_capture', at: hhmm(firstTs) })
  const noResult = tests.filter((r) => r.status === 'no_result')
  if (noResult.length) gaps.push({ kind: 'no_result', count: noResult.length, step: noResult[noResult.length - 1].step })
  if (s1) gaps.push({ kind: 'code_at_statement', step: s1.step, lastVersion: s1.versionBefore })
  const noLines = edits.filter((e) => e.linesAdded == null && e.linesRemoved == null).length
  if (noLines) gaps.push({ kind: 'edit_line_counts', count: noLines })
  if (!criteria.accepted) gaps.push({ kind: 'criteria_not_accepted' })

  const lastTests = tests.slice(-MAX_RUNS)
  return {
    live: true,
    tracked: Boolean(tracked && tracked.change),
    project: { id: S.project && S.project.id ? S.project.id : null, name: S.project ? S.project.name : null, root: S.project ? S.project.worktree : null },
    change: { id: change.id || null, threadId: change.threadId || null, title: change.title || null, following: change.whoseMove === 'agent' },
    version: { id: cur, commit: vrow ? vrow.commit : null, unsaved: vrow ? vrow.unsavedChanges : null },
    readAt: S.view ? S.view.readAt : hhmm(new Date(options.now ?? Date.now()).toISOString()),
    readDate: S.view && S.view.readDate ? S.view.readDate : ymd(new Date(options.now ?? Date.now()).toISOString()),
    story: { date: ymd(firstTs), from: hhmm(firstTs), to: hhmm(lastTs), firstTs, lastTs },
    agents: [...new Set(list(S.sessions).map((s) => s.agent).filter(Boolean))],
    sessions: list(S.sessions).length,
    steps: change.stepCount == null ? null : change.stepCount,
    statementsCount: statements.length,
    approval,
    criteria,
    timeline,
    runs: lastTests.map((r) => ({ step: r.step, time: r.time, command: r.command, version: r.version, now: r.version === cur, ...resultOf(r) })),
    testsTotal: tests.length,
    s1, s2, s3,
    findings,
    decisions,
    /* C30 R-LEDGER: the words people kept on the tracked Change, as served (data/notes.js); words, never a result */
    notes: list(S.notes),
    seconds: list(S.seconds),
    /* C30 R-LEDGER: the statements people said were wrong, each with the verdict it already had (data/objections.js); words, never a result */
    objected: objectedStatements(S.statements),
    gaps,
    masking: S.masking || null,
    leftOutByExclusion: S.leftOutByExclusion || null,
    storageBytes: options.storageBytes ?? (S.machine ? S.machine.recordsBytes : null),
  }
}
