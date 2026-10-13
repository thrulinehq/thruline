/* What-Changed's data, selected from the ONE Change snapshot.
 *
 * PURE: no fetch, no clock, no window. The input is the shared Change model's
 * snapshot (ui/src/hq/data/change.js; its shape is Change-Brief/demo.js's
 * DEMO, plus every edit's hunks). The output is this screen's data contract
 * (DATA.md, demo.js). It renames, picks and joins; it does not re-derive a
 * fact the snapshot holds. A fact the snapshot lacks stays absent.
 *
 * Every edit and every line of every hunk is passed through. Nothing is cut:
 * the diff box scrolls.
 */
import { homeless } from '../../data/paths.js'

/* A hunk as this screen draws it. The snapshot may carry the engine's wire
 * shape (`old_start`, `lines[].kind: context|added|removed`, API.md §Edit) or
 * the camel-case one; both are read. A start the record does not state stays
 * null, and the screen says the line numbers were not recorded. */
const OP = { context: 'context', added: 'add', removed: 'del', add: 'add', del: 'del' }
function hunkOf(h) {
  const pick = (a, b) => (h[a] !== undefined ? h[a] : h[b])
  const oldStart = pick('oldStart', 'old_start')
  const newStart = pick('newStart', 'new_start')
  return {
    oldStart: oldStart == null ? null : oldStart,
    newStart: newStart == null ? null : newStart,
    context: pick('context', 'header') || '',
    lines: (Array.isArray(h.lines) ? h.lines : []).map((l) => ({ op: OP[l.op || l.kind] || 'context', text: l.text == null ? '' : homeless(String(l.text)) })), // the home folder as ~ (paths.js)
  }
}

/* Which group a file is listed under, by its path alone (a stated rule, not a guess at content). */
export function kindOf(path) {
  const p = String(path || '')
  const base = p.split('/').pop()
  if (/(^|\/)(dist|build|target|node_modules|vendor|\.next|coverage)\//.test(p) || /\.(min\.js|map)$/.test(p) || /(^|\/)(package-lock\.json|yarn\.lock|pnpm-lock\.yaml|Cargo\.lock|poetry\.lock)$/.test(p)) return 'generated'
  if (/(^|\/)(tests?|__tests__|spec|specs)\//.test(p) || /([._-](test|spec)s?\.[A-Za-z0-9]+$|^test_[^/]*$|_test\.go$)/.test(base)) return 'tests'
  if (/\.(json|toml|ya?ml|ini|cfg|conf|env)$/.test(base) || /^(Makefile|Dockerfile|\.[A-Za-z].*rc)$/.test(base) || /(^|\/)\.github\//.test(p)) return 'config'
  return 'code'
}

/* A run-scope entry that names one file (not a folder or a pattern). */
const isFilePath = (s) => typeof s === 'string' && !s.endsWith('/') && !/[*?]/.test(s) && /\.[A-Za-z0-9]+$/.test(s.split('/').pop())

export function whatChangedFromChange(snap) {
  if (!snap || typeof snap !== 'object') return null
  const versions = Array.isArray(snap.versions) ? snap.versions : []
  const current = snap.currentVersion || (versions.length ? versions[versions.length - 1].id : null)
  const cur = versions.find((v) => v.id === current) || null
  const edits = (Array.isArray(snap.edits) ? snap.edits : []).map((e) => ({
    id: e.id, step: e.step == null ? null : e.step, sessionId: e.sessionId || null, time: e.time || null,
    file: e.file, op: e.op || null, makesVersion: e.makesVersion || null,
    source: e.source || null,
    linesAdded: e.linesAdded != null ? e.linesAdded : (e.lines_added != null ? e.lines_added : null),
    linesRemoved: e.linesRemoved != null ? e.linesRemoved : (e.lines_removed != null ? e.lines_removed : null),
    hunks: Array.isArray(e.hunks) ? e.hunks.map(hunkOf) : null,
  }))
  /* C30 R-RUN: a check ThruLine ran from HQ has no step (it is not in the
   * agent's record). This page orders everything by step, so such a run is
   * placed by its time, half a step after the last record before it; the
   * half step is how the page knows to name it "the check ThruLine ran"
   * instead of a step number (logic.js `stp`). No step number is invented. */
  const ordered = [].concat(
    Array.isArray(snap.edits) ? snap.edits : [],
    (Array.isArray(snap.runs) ? snap.runs : []).filter((r) => r.step != null),
    Array.isArray(snap.statements) ? snap.statements : [],
  ).filter((x) => x && x.step != null && x.ts)
  const msOf = (ts) => { const t = ts ? Date.parse(ts) : NaN; return Number.isNaN(t) ? null : t }
  const placed = (r) => {
    if (r.step != null) return r.step
    const at = msOf(r.ts)
    const before = ordered.filter((x) => at == null || (msOf(x.ts) != null && msOf(x.ts) <= at)).map((x) => x.step)
    return (before.length ? Math.max(...before) : 0) + 0.5
  }
  /* checks only: a command that is not a test run does not cover a file */
  const runs = (Array.isArray(snap.runs) ? snap.runs : []).filter((r) => r.status !== 'running' && r.kind !== 'command').map((r) => ({
    id: r.id, step: placed(r), sessionId: r.sessionId || null, time: r.time || null, version: r.version || null,
    command: r.command || '', scope: Array.isArray(r.scope) ? r.scope.slice() : [], status: r.status, outcome: r.outcome || null,
    exitCode: r.exitCode == null ? null : r.exitCode, passed: r.passed == null ? null : r.passed, failed: r.failed == null ? null : r.failed,
    durationMs: r.durationMs == null ? null : r.durationMs, capturedBy: r.capturedBy || null,
    declaresPaths: !!r.declaresPaths,
  }))
  /* every file an edit touched, in first-edit order, then every single file a run named */
  const paths = []
  edits.forEach((e) => { if (e.file && paths.indexOf(e.file) < 0) paths.push(e.file) })
  runs.forEach((r) => r.scope.forEach((q) => { if (isFilePath(q) && paths.indexOf(q) < 0) paths.push(q) }))
  const statements = Array.isArray(snap.statements) ? snap.statements : []
  const focusId = snap.brief && snap.brief.statementId ? snap.brief.statementId : (statements.length ? statements[0].id : null)
  const lastEdited = edits.length ? edits[edits.length - 1].file : null
  /* C30 R-RUN: a tracked Change runs its own check (the served run sheet's
   * command, exactly what the press starts) */
  const sheetCheck = snap.runSheet && Array.isArray(snap.runSheet.checks) ? snap.runSheet.checks[0] || null : null
  const recheck = sheetCheck && sheetCheck.command
    ? Object.assign({}, snap.recheck || {}, { runner: sheetCheck.command, scope: [], on: (snap.recheck && snap.recheck.on) || current })
    : snap.recheck || null
  /* a check of this Change going now on the current version */
  const going = (Array.isArray(snap.runs) ? snap.runs : []).find((r) => r && r.status === 'running' && r.version === current) || null
  /* A stopped check is in the served run sheet, not in the transcript runs.
   * Keep its recorded end across a reload, only while it is on these files. */
  const last = sheetCheck && sheetCheck.last
  const start = msOf(last && last.started_at)
  const end = msOf(last && last.recorded_at)
  const lastEdit = (Array.isArray(snap.edits) ? snap.edits : []).reduce((m, e) => {
    const t = msOf(e && e.ts)
    return t != null && (m == null || t > m) ? t : m
  }, null)
  const stopped = !going && last && last.outcome === 'cancelled' && start != null
    && end != null && end >= start && (lastEdit == null || start >= lastEdit)
    ? { at: new Date(end).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false }), elapsedS: Math.floor((end - start) / 1000) }
    : null
  const ch = snap.change || {}

  return {
    project: { id: snap.project && snap.project.id, name: snap.project ? snap.project.name : '', worktree: snap.project ? snap.project.worktree || null : null },
    machine: { recordsBytes: snap.machine && snap.machine.recordsBytes != null ? snap.machine.recordsBytes : null },
    view: { readAt: snap.view ? snap.view.readAt : null },
    change: {
      id: ch.id || null, title: ch.title || '', state: ch.state || null, recording: typeof ch.recording === 'boolean' ? ch.recording : null, followingLive: !!(snap.view && snap.view.followingLive),
      stages: Array.isArray(ch.stages) ? ch.stages.map((x) => ({ id: x.id, status: x.status })) : [],
    },
    version: { id: current, commit: cur ? cur.commit || null : null, unsavedChanges: cur ? cur.unsavedChanges : null },
    versions: versions.map((v) => ({ id: v.id, madeByStep: v.madeByStep == null ? null : v.madeByStep, recordedAtStep: v.recordedAtStep == null ? null : v.recordedAtStep })),
    files: paths.map((p) => ({ path: p, kind: kindOf(p) })),
    selectedFile: lastEdited,
    edits,
    runs,
    statements: statements.map((q) => ({
      id: q.id, messageId: q.messageId || null, text: (q.plain != null ? q.plain : q.text), verbatim: q.text, claimKind: q.claimKind || null, verifier: !!q.verifier,
      step: q.step == null ? null : q.step, time: q.time || null, sessionId: q.sessionId || null, agent: q.agent || '',
      versionBefore: q.versionBefore || null,
      proof: { verdict: q.proof ? q.proof.verdict : 'not_proven', restsOn: q.proof && q.proof.restsOn ? { ...q.proof.restsOn } : null },
    })),
    statementId: focusId,
    recheck: recheck && recheck.runner ? { runner: recheck.runner, scope: Array.isArray(recheck.scope) ? recheck.scope.slice() : [], on: recheck.on || current, permission: recheck.permission || null } : null,
    /* added C30 R-RUN: the check going now, from the record (null when none) */
    running: going ? { elapsedS: going.elapsedS == null ? null : going.elapsedS } : null,
    stopped,
  }
}

/* INTERIM, until the shared snapshot carries them (asked of W2-B through the
 * orchestrator): every edit's served hunks and their source, joined onto the
 * snapshot's edits by id from the ThreadView the snapshot was made from. The
 * engine serves them (API.md §Edit). Returns a new snapshot; the input is not changed. */
export function withServedHunks(snap, view) {
  if (!snap || !view || !Array.isArray(view.edits)) return snap
  const served = new Map(view.edits.map((e) => [e.id, e]))
  return { ...snap, edits: (snap.edits || []).map((e) => (e.hunks || !served.has(e.id) ? e : { ...e, hunks: served.get(e.id).hunks, source: served.get(e.id).source })) }
}

export default whatChangedFromChange
