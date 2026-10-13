/* Every-Statement's data, selected from the ONE Change snapshot.
 *
 * PURE: no fetch, no clock, no window. The input is the shared Change model's
 * snapshot (ui/src/hq/data/change.js; its shape is Change-Brief/demo.js's
 * DEMO). The output is this screen's data contract (DATA.md, demo.js). Nothing
 * here re-derives a fact the snapshot holds: it renames, picks and joins.
 * What the snapshot does not hold stays absent (null / []), never 0 and never
 * invented; the screen draws absence in the design's own words.
 */

const isRun = (p) => !!p && !!p.restsOn && p.restsOn.kind === 'run'

/* The snapshot's change state → this artboard's state word. "ready" is drawn as Proven. */
const CHANGE_STATE = { ready: 'proven', not_proven: 'not_proven', needs_recheck: 'needs_recheck', recorded: 'recorded', in_progress: 'in_progress', proven: 'proven', contradicted: 'contradicted' }

/* A proof as this screen reads it: the same links, with what the cited run did
 * not cover (`checkedAgainst.misses`) lifted to `misses`. */
function proofOf(p) {
  if (!p) return { verdict: 'not_proven', stopsAt: 2, restsOn: null, lookedAt: [], misses: [] }
  const out = {
    verdict: p.verdict || 'not_proven',
    stopsAt: p.stopsAt == null ? null : p.stopsAt,
    restsOn: p.restsOn ? { ...p.restsOn } : null,
    lookedAt: Array.isArray(p.lookedAt) ? p.lookedAt.slice() : [],
    misses: p.restsOn && p.restsOn.checkedAgainst && Array.isArray(p.restsOn.checkedAgainst.misses) ? p.restsOn.checkedAgainst.misses.slice() : (Array.isArray(p.misses) ? p.misses.slice() : []),
  }
  if (p.since && p.since.editId) out.since = { editId: p.since.editId }
  if (Array.isArray(p.lookedAtSessions)) out.lookedAtSessions = p.lookedAtSessions.slice()
  if (p.recheck && p.recheck.runId) out.recheckRunId = p.recheck.runId
  return out
}

/* The verb a completion claim uses, read from its own words (for "The record
 * can't check "fixed" by itself"). The claim's kind decides it when the
 * sentence names none. */
const VERBS = ['fixed', 'resolved', 'refactored', 'implemented', 'added', 'shipped', 'merged', 'deployed', 'completed', 'done']
function verbOf(text, kind) {
  const t = String(text || '').toLowerCase()
  const hit = VERBS.find((w) => new RegExp('\\b' + w + '\\b').test(t))
  if (hit) return hit
  return kind === 'fixed' ? 'fixed' : 'done'
}

/* The snapshot's claim kinds → the re-check this screen offers. */
const CLAIM_KIND = { tests_pass: 'tests_pass', builds: 'lint_clean', lint_ok: 'lint_clean', lint_clean: 'lint_clean' }

export function everyStatementFromChange(snap) {
  if (!snap || typeof snap !== 'object') return null
  const versions = Array.isArray(snap.versions) ? snap.versions : []
  const current = snap.currentVersion || (versions.length ? versions[versions.length - 1].id : null)
  const cur = versions.find((v) => v.id === current) || null
  const edits = Array.isArray(snap.edits) ? snap.edits : []
  const runs = Array.isArray(snap.runs) ? snap.runs : []
  const statements = Array.isArray(snap.statements) ? snap.statements : []
  const crit = snap.criteria || {}
  const lines = Array.isArray(crit.lines) ? crit.lines : []
  const times = edits.map((e) => e.time).filter(Boolean)
  const latestEdit = edits.filter((e) => e.time && Number.isFinite(Date.parse(e.ts || ''))).reduce((m, e) => (!m || Date.parse(e.ts) > Date.parse(m.ts) ? e : m), null)
  const running = runs.find((r) => r.status === 'running' && (!current || r.version === current)) || null
  /* C30 R-RUN: a tracked Change runs its own check (the served run sheet's
   * command, which is exactly what the press starts); otherwise the command
   * the record shows the statement rested on */
  const sheetCheck = snap.runSheet && Array.isArray(snap.runSheet.checks) ? snap.runSheet.checks[0] || null : null
  const last = sheetCheck && sheetCheck.last ? sheetCheck.last : null
  const lastEditMs = edits.reduce((max, e) => {
    const at = Date.parse(e.ts || '')
    return Number.isFinite(at) && at > max ? at : max
  }, 0)
  const stoppedAt = last && last.outcome === 'cancelled' ? Date.parse(last.started_at || '') : NaN
  const stopped = Number.isFinite(stoppedAt) && stoppedAt >= lastEditMs ? {
    elapsedS: Number.isInteger(last.elapsed_s) && last.elapsed_s >= 0 ? last.elapsed_s
      : Math.max(0, Math.floor((Date.parse(last.recorded_at || '') - stoppedAt) / 1000))
  } : null
  const recheck = snap.recheck
    ? (sheetCheck && sheetCheck.command ? Object.assign({}, snap.recheck, { runner: sheetCheck.command, scope: [] }) : snap.recheck)
    : null
  const referenced = new Set()
  statements.concat(lines).forEach((q) => {
    const p = q.proof || {}
    if (p.restsOn && p.restsOn.runId) referenced.add(p.restsOn.runId)
    if (p.recheck && p.recheck.runId) referenced.add(p.recheck.runId)
    ;(Array.isArray(p.lookedAt) ? p.lookedAt : []).forEach((id) => referenced.add(id))
  })

  const outStatements = statements.map((q) => {
    const proof = proofOf(q.proof)
    if (proof.restsOn && proof.restsOn.kind === 'criteria') proof.verb = verbOf(q.text, q.claimKind)
    return {
      id: q.id, messageId: q.messageId || null, text: (q.plain != null ? q.plain : q.text), verbatim: q.text,
      claimKind: CLAIM_KIND[q.claimKind] || q.claimKind || 'other', verifier: !!q.verifier,
      step: q.step == null ? null : q.step, time: q.time || null, sessionId: q.sessionId || null, agent: q.agent || '',
      versionBefore: q.versionBefore || null,
      proof,
    }
  })
  const outLines = lines.map((l, i) => ({
    id: l.id || 'line-' + (i + 1), num: l.num || i + 1, gate: !!l.gate, link: l.link || null, text: l.text,
    recordedLine: l.recordedLine || null,
    proof: proofOf(l.proof),
  }))

  return {
    project: { id: snap.project && snap.project.id, name: snap.project ? snap.project.name : '', worktree: snap.project ? snap.project.worktree || null : null },
    machine: { recordsBytes: snap.machine && snap.machine.recordsBytes != null ? snap.machine.recordsBytes : null },
    view: { readAt: snap.view ? snap.view.readAt : null, readDate: snap.view ? snap.view.readDate || null : null, utcOffset: null },
    change: {
      id: snap.change ? snap.change.id : null,
      title: snap.change ? snap.change.title : '',
      state: CHANGE_STATE[snap.change && snap.change.state] || 'not_proven',
      recording: snap.change && typeof snap.change.recording === 'boolean' ? snap.change.recording : null,
    },
    version: { id: current, commit: cur ? cur.commit || null : null },
    sessions: (Array.isArray(snap.sessions) ? snap.sessions : []).map((s) => ({ id: s.id, agent: s.agent || '' })),
    versions: versions.map((v) => ({ id: v.id, madeByStep: v.madeByStep == null ? null : v.madeByStep, recordedAtStep: v.recordedAtStep == null ? null : v.recordedAtStep })),
    /* checks only: a command that is not a test run is not something a statement rests on, unless a proof names it */
    runs: runs.filter((r) => r.status !== 'running' && (r.kind !== 'command' || referenced.has(r.id))).map((r) => ({
      id: r.id, step: r.step == null ? null : r.step, sessionId: r.sessionId || null, version: r.version || null,
      command: r.command || '', scope: Array.isArray(r.scope) ? r.scope.slice() : [],
      status: r.status, outcome: r.outcome || null, exitCode: r.exitCode == null ? null : r.exitCode,
      passed: r.passed == null ? null : r.passed, failed: r.failed == null ? null : r.failed,
      durationMs: r.durationMs == null ? null : r.durationMs, capturedBy: r.capturedBy || null,
      declaresPaths: !!r.declaresPaths,
    })),
    edits: edits.map((e) => ({ id: e.id, step: e.step == null ? null : e.step, sessionId: e.sessionId || null, time: e.time || null, file: e.file, makesVersion: e.makesVersion || null })),
    /* the last edit is the latest MOMENT, not the greatest HH:MM face (23:50 outsorts 00:05 the next day) */
    lastEditAt: latestEdit ? latestEdit.time : (times.length ? times.slice().sort()[times.length - 1] : null),
    lastEditTs: latestEdit ? latestEdit.ts : null,
    criteria: {
      accepted: !!crit.accepted,
      version: crit.version || null,
      acceptedAt: crit.acceptedAt || null,
      acceptedTs: crit.acceptedTs || null,
      acceptedBy: crit.acceptedBy || null,
      lines: outLines,
    },
    messages: (Array.isArray(snap.messages) ? snap.messages : []).map((m) => ({ id: m.id, step: m.step == null ? null : m.step, time: m.time || null, sessionId: m.sessionId || null, agent: m.agent || '', text: (m.plain != null ? m.plain : m.text) || '' })),
    statements: outStatements,
    recheck: recheck ? {
      runner: recheck.runner || '',
      scope: Array.isArray(recheck.scope) ? recheck.scope.slice() : [],
      on: recheck.on || current,
      permission: recheck.permission || null,
      /* which rows one re-check reaches: every accepted line, and every test claim it could settle */
      reaches: {
        lines: outLines.map((l) => l.num),
        statements: outStatements.filter((q) => q.claimKind === 'tests_pass' && q.proof.verdict !== 'proven' && isRun(q.proof)).map((q) => q.id),
      },
      elapsedS: running && running.elapsedS != null ? running.elapsedS : null,
    } : null,
    running: !!running,
    stopped,
    selected: snap.brief && snap.brief.statementId ? snap.brief.statementId : null,
  }
}

export default everyStatementFromChange
