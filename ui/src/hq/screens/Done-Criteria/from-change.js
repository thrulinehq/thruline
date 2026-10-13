/* Done-Criteria on the real record: the screen's data (DEMO's shape, DATA.md)
 * selected from the ONE Change snapshot (`ui/src/hq/data/change.js`, the shape
 * of Change-Brief/demo.js's DEMO) and, where a person tracks the work as a
 * Change, that Change's documents read through the existing
 * `readChangeDocuments` / `deriveChangeModel`: the proposed lines, the accepted
 * version, each accepted line's state and sentence. Nothing here re-derives a
 * criterion, a coverage state or a readiness answer.
 *
 * PURE. A fact none of them holds is `null` or empty, never a guess.
 */
import { clock } from '../../../workspace/run-route.js'
import { unlinkedCompletedRun } from '../../../workspace/coverage-view.js'
import { templatesOf } from './templates.js'

const list = (v) => (Array.isArray(v) ? v : [])
const byId = (rows, id) => (id == null ? null : list(rows).find((r) => r && r.id === id) || null)
const num = (v) => (typeof v === 'number' && Number.isFinite(v) ? v : null)
const str = (v) => (v == null ? '' : String(v))

function runOf(r) {
  if (!r) return null
  return {
    startStep: null, step: num(r.step), command: r.command || null, file: list(r.scope)[0] || null,
    version: r.version || null, exit: num(r.exitCode), passed: num(r.passed), failed: num(r.failed),
    capturedBy: r.capturedBy || null,
  }
}

/** The steps of the recorded edits under one covered folder. A served folder
 *  is written the way the engine matches it (`src/**`), a suggested one as
 *  the edits' top folder (`src/`): both mean everything under `src/`. */
function stepsUnder(edits, path) {
  const under = str(path).replace(/\*\*$/, '')
  return list(edits).filter((e) => under && str(e.file).startsWith(under)).map((e) => e.step).filter((n) => n != null)
}

/** Folders the recorded edits touched, in first-touched order, with their steps. */
function foldersOf(edits) {
  const out = []
  for (const e of list(edits)) {
    const f = str(e.file)
    const top = f.includes('/') ? f.slice(0, f.indexOf('/') + 1) : f
    if (!top) continue
    let row = out.find((r) => r.path === top)
    if (!row) { row = { path: top, editSteps: [], included: true }; out.push(row) }
    if (num(e.step) != null) row.editSteps.push(e.step)
  }
  return out
}

/**
 * @param {object} snapshot  the one Change snapshot
 * @param {{id: string, change: object, proposals: object|null, coverage: object|null, model: object|null}|null} tracked
 * @param {{now?: Date, templates?: object}} [ctx]  `templates` is the read of
 *   `GET /api/hq/projects/<id>/done-templates` (templates.js)
 */
export function selectDoneCriteria(snapshot, tracked, ctx = {}) {
  if (!snapshot) return null
  const runs = list(snapshot.runs)
  const edits = list(snapshot.edits)
  const versions = list(snapshot.versions)
  const V = snapshot.currentVersion || null
  const cur = byId(versions, V)
  const tests = runs.filter((r) => r && r.status === 'finished' && r.kind === 'test')
  const last = tests.length ? tests[tests.length - 1] : null
  const before = tests.length > 1 ? tests[tests.length - 2] : null
  const pending = [...runs].reverse().find((r) => r && r.status === 'no_result') || null
  const sts = list(snapshot.statements)
  /* the claim the bar is asked for: the latest "fixed"/"done" claim with no verifier, else the latest without one, else the latest */
  const unverified = sts.filter((s) => s && s.verifier === false)
  const claim = [...unverified].reverse().find((s) => s.claimKind === 'fixed' || s.claimKind === 'done' || s.claimKind === 'task_complete')
    || unverified[unverified.length - 1] || sts[sts.length - 1] || null
  const brief = snapshot.brief ? byId(sts, snapshot.brief.statementId) : null
  const movedEdit = brief && brief.proof && brief.proof.since ? byId(edits, brief.proof.since.editId) : null
  const recheck = snapshot.recheck || null
  const command = recheck && recheck.runner ? [recheck.runner, ...list(recheck.scope)].join(' ') : null

  const doc = tracked && tracked.change ? tracked.change : null
  const scope = doc && doc.accepted_scope ? doc.accepted_scope : null
  const model = tracked && tracked.model ? tracked.model : null
  const proposals = tracked && tracked.proposals ? tracked.proposals : null
  const unlinked = tracked && tracked.coverage ? unlinkedCompletedRun(tracked.coverage) : null

  /* The lines: the accepted version's, each with the engine's state and
   * sentence; else the proposals, all still proposed. */
  let lines = []
  let recorded = false
  if (scope && model) {
    recorded = true
    /* `proposal: null`: a line of the standing version, kept by its words
     * (never by a proposal number) when a person changes the version */
    lines = list(model.criteria).map((c) => ({
      n: c.n, text: c.text, kind: 'behaviour', source: { kind: 'accepted' }, check: command, namedTest: null,
      evidence: c.unlinked ? 'candidate' : 'engine', state: 'accepted', linked: false,
      modelState: c.state, word: c.word || null, line: c.line || null, proposal: null,
    }))
  } else if (proposals) {
    lines = list(proposals.criteria).map((c) => ({
      n: c.number, text: c.text, kind: 'behaviour', source: { kind: 'proposal' }, check: command, namedTest: null,
      evidence: 'none', state: 'proposed', linked: false, proposal: c.number,
    }))
  }

  const covers = scope && list(scope.allowed_scope).length
    ? list(scope.allowed_scope).map((p) => ({ path: p, editSteps: stepsUnder(edits, p), included: true }))
    : proposals && proposals.scope && list(proposals.scope.paths).length
      ? list(proposals.scope.paths).map((p) => ({ path: p, editSteps: stepsUnder(edits, p), included: true }))
      : foldersOf(edits)
  const tpl = templatesOf(ctx.templates || null)

  const note = proposals ? list(proposals.notes)[0] || null : null
  const lastEdit = edits.length ? edits[edits.length - 1] : null
  const s0 = list(snapshot.sessions)[0] || {}

  return {
    project: { name: snapshot.project ? snapshot.project.name : null },
    change: { title: snapshot.change ? snapshot.change.title : null },
    version: { id: V, commit: cur ? cur.commit || null : null, clean: cur ? cur.unsavedChanges === false : false },
    steps: snapshot.change ? num(snapshot.change.stepCount) : null,
    readAt: snapshot.view ? snapshot.view.readAt || null : null,
    recording: Boolean(snapshot.change && snapshot.change.recording),
    recordingSince: snapshot.change ? snapshot.change.recordedFrom || null : null,
    request: snapshot.request
      ? { text: snapshot.request.text || null, by: 'you', at: snapshot.request.time || null, step: num(snapshot.request.step) }
      : { text: null, by: 'you', at: null, step: null },
    agent: { name: s0.agent || (claim && claim.agent) || null, lastEdit: { at: lastEdit ? lastEdit.time || null : null, ts: lastEdit ? lastEdit.ts || null : null, step: lastEdit ? num(lastEdit.step) : null } },
    claim: claim ? { text: claim.plain || claim.text, word: null, at: claim.time || null, step: num(claim.step) } : null,
    notCheckable: note ? { quote: str(note.text), shownBy: [] } : null,
    criteria: {
      version: scope ? scope.version : 1,
      recorded,
      acceptedAt: scope && scope.acceptance ? clock(scope.acceptance.accepted_at) || null : null,
      acceptedTs: scope && scope.acceptance ? str(scope.acceptance.accepted_at) || null : null,
      acceptedBy: scope && scope.acceptance ? (str(scope.acceptance.actor) || 'you') : 'you',
      lines,
    },
    recheck: { command },
    lastRun: runOf(last),
    earlierRun: runOf(before),
    pendingRun: pending ? { step: num(pending.step) } : null,
    movedEdit: movedEdit ? { file: movedEdit.file, step: num(movedEdit.step) } : null,
    uncovered: null,
    covers,
    /* what the standing version marks out of scope (the record's exclusions) */
    outOfScope: scope ? list(scope.exclusions).map(str).filter(Boolean) : [],
    templates: tpl.templates,
    templatesNote: { why: tpl.why, detail: tpl.detail },
    /* the six stages, as the one Change model reads them */
    stages: snapshot.change ? list(snapshot.change.stages).map((x) => ({ id: x.id, status: x.status })) : [],
    storage: { bytes: snapshot.machine ? num(snapshot.machine.recordsBytes) : null },
    /* what the writes carry */
    act: {
      changeId: tracked ? tracked.id : null,
      unlinkedRun: unlinked ? { id: unlinked.id, name: unlinked.name } : null,
      /* the work on screen, as "Track this work as a Change" names it */
      project: snapshot.project && snapshot.project.id ? String(snapshot.project.id) : null,
      thread: snapshot.change && snapshot.change.threadId ? String(snapshot.change.threadId) : null,
    },
  }
}

export default selectDoneCriteria
