/* Approve-Sheet on the real record: the screen's data (DEMO's shape, DATA.md)
 * selected from the ONE Change snapshot (`ui/src/hq/data/change.js`, the shape
 * of Change-Brief/demo.js's DEMO), the tracked Change's documents read through
 * the existing `readChangeDocuments` / `deriveChangeModel` (criteria, readiness,
 * decisions: never re-derived here), and `/api/licence` (Free or Pro only, by
 * the one plan rule, `data/plan.js`).
 *
 * PURE. A fact none of them holds is `null` or absent, never a guess.
 */
import { clock } from '../../../workspace/run-route.js'
import { planOf, licencePlan } from '../../data/plan.js'

const list = (v) => (Array.isArray(v) ? v : [])
const byId = (rows, id) => (id == null ? null : list(rows).find((r) => r && r.id === id) || null)
const num = (v) => (typeof v === 'number' && Number.isFinite(v) ? v : null)
const str = (v) => (v == null ? '' : String(v))

/** Free or Pro, and nothing else (CTO C21-b), by the ONE plan rule (data/plan.js, C30 P0-6). */
export { planOf }

/** The sheet's decision rows from served `change_decisions` rows (`decisions.history`): the record's, or an act's answer. */
export function decisionRows(rows) {
  return list(rows).map((row) => ({
    kind: str(row && row.act) === 'approved' ? 'approved' : 'withdrawn',
    time: clock(row && row.decided_at) || null,
    name: str(row && row.actor),
    note: str(row && row.note),
    targetRevision: str(row && row.target_revision) || null,
  }))
}

/* What one line of what counts as done rests on, by the basis of the evidence the Change model
 * links to it (coverage `items[].basis`, change-model.js): a link a person made, the checks the
 * person accepted, or a run that names what the line is about. "You said this does not bear on
 * this line" is not something it rests on. A line with no such evidence rests on nothing yet. */
const LINK_OF = { manual: 'person', declared: 'accepted_check', named: 'named' }
const LINK_ORDER = ['person', 'accepted_check', 'named']
export function linkOf(criterion) {
  const items = list(criterion && criterion.items).filter((i) => i && LINK_OF[i.basis])
  const bearing = items.filter((i) => i.bears === true)
  const kinds = new Set((bearing.length ? bearing : items).map((i) => LINK_OF[i.basis]))
  return LINK_ORDER.find((k) => kinds.has(k)) || null
}

/** The proof state of one statement, as the sheet's rows draw it. */
function proofOf(s) {
  const v = s && s.proof ? s.proof.verdict : null
  if (v === 'proven') return 'proven'
  if (v === 'recorded' || (s && s.verifier === false)) return 'recorded'
  if (v === 'needs_recheck') return 'needs_recheck'
  return 'not_proven'
}

/**
 * @param {object} snapshot  the one Change snapshot
 * @param {{id: string, change: object, model: object}|null} tracked  the tracked Change: its served document and `deriveChangeModel`'s reading
 * @param {{licence?: object|null}} [ctx]
 */
export function selectApproveSheet(snapshot, tracked, ctx = {}) {
  if (!snapshot) return null
  const runs = list(snapshot.runs)
  const edits = list(snapshot.edits)
  const versions = list(snapshot.versions)
  const V = snapshot.currentVersion || null
  const cur = byId(versions, V)
  const model = tracked && tracked.model ? tracked.model : null
  const doc = tracked && tracked.change ? tracked.change : null
  const scope = doc && doc.accepted_scope ? doc.accepted_scope : null

  /* What the approval would rest on: the run the accepted lines are proven by,
   * else the run the Brief's statement rests on, on this version. */
  const crit = snapshot.criteria || {}
  const brief = snapshot.brief ? byId(snapshot.statements, snapshot.brief.statementId) : null
  const briefRun = brief && brief.proof && brief.proof.restsOn && brief.proof.restsOn.kind === 'run' ? byId(runs, brief.proof.restsOn.runId) : null
  const restRun = byId(runs, crit.provenOn && crit.provenOn.runId) || (briefRun && briefRun.version === V ? briefRun : null)

  const finished = runs.filter((r) => r && r.status === 'finished')
  const pending = [...runs].reverse().find((r) => r && r.status === 'no_result') || null
  const earlier = versions.filter((v) => v.id !== V).slice(-2).map((v) => v.id)
  const lastEdit = edits.length ? edits[edits.length - 1] : null
  const moved = doc && doc.code_moved && str(doc.code_moved.now)
    ? { version: str(doc.code_moved.now).slice(0, 7), files: list(doc.code_moved.changed_paths).map(String), step: null, at: null }
    : null

  const history = decisionRows(doc && doc.decisions && doc.decisions.history)

  return {
    project: { name: snapshot.project ? snapshot.project.name : null, worktree: snapshot.project ? snapshot.project.worktree || null : null },
    change: { title: snapshot.change ? snapshot.change.title : null, sessions: list(snapshot.sessions).length },
    version: { id: V, fullId: cur ? cur.fullId || null : null, commit: cur ? cur.commit || null : null, clean: cur ? cur.unsavedChanges === false : false },
    session: { day: null, start: (list(snapshot.sessions)[0] || {}).startedAt || null, end: null },
    agent: { name: (list(snapshot.sessions)[0] || {}).agent || null, lastEditAt: lastEdit ? lastEdit.time || null : null, lastEditTs: lastEdit ? lastEdit.ts || null : null },
    readAt: snapshot.view ? snapshot.view.readAt || null : null,
    readyAt: null,
    recording: Boolean(snapshot.change && snapshot.change.recording),
    request: { text: snapshot.request ? snapshot.request.text || null : null, step: snapshot.request ? num(snapshot.request.step) : null },
    statements: list(snapshot.statements).map((s) => ({ text: s.plain || s.text, step: num(s.step), at: s.time || null, proof: proofOf(s) })),
    criteria: {
      version: scope ? 'v' + scope.version : null,
      acceptedAt: scope && scope.acceptance ? clock(scope.acceptance.accepted_at) || null : null,
      acceptedTs: scope && scope.acceptance ? str(scope.acceptance.accepted_at) || null : null,
      acceptedBy: scope && scope.acceptance ? str(scope.acceptance.actor) || null : null,
      setAside: model && model.readiness ? list(model.readiness.waivedLines).length : null,
      /* `kind`: the record holds each accepted line as a line of what counts as done and types it no
       * further (no behaviour or gate); `link`: what the line rests on, from the Change model (`linkOf`) */
      lines: model ? list(model.criteria).map((c) => ({ n: c.n, text: c.text, kind: 'criterion', link: linkOf(c), state: c.state, word: c.word || null })) : [],
    },
    run: restRun
      ? {
        step: num(restRun.step), command: restRun.command || null, scope: list(restRun.scope), version: restRun.version || null,
        exit: num(restRun.exitCode), passed: num(restRun.passed), failed: num(restRun.failed),
        durationS: num(restRun.durationMs) == null ? null : restRun.durationMs / 1000, at: restRun.time || null,
        cwd: snapshot.project ? snapshot.project.worktree || null : null, capturedBy: restRun.capturedBy || null,
      }
      : null,
    trace: {
      lanes: { request: snapshot.request ? 1 : 0, work: edits.length, execution: runs.length, claims: list(snapshot.statements).length },
      versions: earlier,
      edits: edits.slice(-3).map((e) => ({ step: num(e.step), file: e.file })),
      runs: finished.slice(-3).map((r) => ({ step: num(r.step), passed: num(r.passed), failed: num(r.failed), exit: num(r.exitCode) })),
    },
    pending: pending ? { step: num(pending.step), command: pending.command || null } : null,
    reviewer: { name: '' },
    decisions: history,
    /* the plan and whether approving is open, by the one plan rule; plan null = the licence was not read */
    licence: (({ plan, approving }) => ({ plan, approving }))(licencePlan(ctx.licence || null)),
    storage: { bytes: snapshot.machine ? num(snapshot.machine.recordsBytes) : null },
    moved,
    /* what the two acts carry: the tracked Change, the version the person saw, and the engine's readiness */
    act: {
      changeId: tracked ? tracked.id : null,
      seen: model && model.decision ? model.decision.seen || null : null,
      approvedFull: (doc && doc.decisions && doc.decisions.standing && doc.decisions.standing.target_revision)
        || (model && model.decision ? model.decision.approvedFull || null : null),
      ready: Boolean(model && model.readiness && model.readiness.ready),
      readiness: model && model.readiness ? model.readiness.sentence || null : null,
      /* RUN-LINK-TRUTH-R5: why approval is off is the one blocker every other
       * surface says — `readiness.blocker`, built by change-model.js's
       * `blockerOf` — never words of this sheet's own. Null when ready. */
      readinessBlocker: model && model.readiness && !model.readiness.ready && model.readiness.blocker
        && model.readiness.blocker.reason ? { reason: model.readiness.blocker.reason, enabler: model.readiness.blocker.enabler || '' } : null,
    },
  }
}

export default selectApproveSheet
