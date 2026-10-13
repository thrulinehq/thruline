/* A synthetic tracked Change for PR-Face's tests, through the REAL Change
 * model: a served ThreadView pushed through `changeSnapshot` (data/change.js),
 * and served Change, coverage and applicability documents pushed through
 * `deriveChangeModel` (workspace/change-model.js) — never a hand-written
 * model. Synthetic data only; no record is read.
 *
 * NOT-APPROVAL-WORDS: a test fixture. It writes served readiness documents
 * for the real model to read, and puts no words about approval on a surface.
 */
import { changeSnapshot } from '../../data/change.js'
import { deriveChangeModel } from '../../../workspace/change-model.js'

export const NOW = Date.parse('2026-10-07T07:00:00Z')
export const CHANGE_ID = 'cccccccc-0000-4000-8000-0000000000a1'
export const THREAD_ID = '0123456789abcdef'
export const TARGET = 'bbbbbbb2c0ffee11'

/** A served ThreadView (docs/hq-next/API.md). */
export function threadView(over = {}) {
  return {
    project: { id: 'p1', name: 'shop', root: '/Users/someone/code/shop' },
    thread: { id: THREAD_ID, title: 'Fix the total', whose_move: 'you' },
    sessions: [{ id: 's1', source: 'claude_code', started_at: '2026-09-29T18:00:00Z' }],
    prompts: [{ event_id: 'e0', session_id: 's1', ts: '2026-09-29T18:00:00Z', text: 'fix the total', work: true }],
    edits: [
      { id: 'ed1', session_id: 's1', ts: '2026-09-29T18:05:00Z', file: '/Users/someone/code/shop/cart/total.py', version_id: 'aaaaaaa1', lines_added: 2, lines_removed: 1 },
      { id: 'ed2', session_id: 's1', ts: '2026-09-29T18:20:00Z', file: '/Users/someone/code/shop/cart/api.py', version_id: 'bbbbbbb2' },
    ],
    commands: [
      { id: 'c1', session_id: 's1', ts: '2026-09-29T18:10:00Z', command: 'pytest -q tests', is_test: true, outcome: 'succeeded', exit_code: 0, tally: { passed: 7, failed: 0 }, version_id: 'aaaaaaa1' },
      { id: 'c2', session_id: 's1', ts: '2026-09-29T18:25:00Z', command: 'cd /Users/someone/code/shop && pytest -q', is_test: true, outcome: 'unknown', exit_code: null, version_id: 'bbbbbbb2' },
    ],
    claims: [
      { id: 'k1', event_id: 'm1', session_id: 's1', ts: '2026-09-29T18:12:00Z', text: 'Tests pass.', kind: 'tests_pass', has_verifier: true, rests_on_command_id: 'c1', version_id: 'aaaaaaa1' },
      { id: 'k2', event_id: 'm2', session_id: 's1', ts: '2026-09-29T18:30:00Z', text: 'Done, the total is fixed.', kind: 'done', has_verifier: false, version_id: 'bbbbbbb2' },
    ],
    versions: [{ id: 'aaaaaaa1', short: 'aaaaaaa', committed: true }, { id: 'bbbbbbb2', short: 'bbbbbbb', committed: false }],
    current_version_id: 'bbbbbbb2',
    masking: { masked: 3, families: ['email'], complete: true },
    ...over,
  }
}

const LINES = ['The total adds up', 'A discount applies once', 'No discount leaves the total unchanged', 'The API reports the total']

const item = (over) => ({
  link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run', name: 'pytest -q tests',
  observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies', ...over,
})

/** The served Change (`GET /api/changes/<id>` → `change`). */
export function changeDoc({ standing = null, target = TARGET, history } = {}) {
  return {
    id: CHANGE_ID,
    project_id: 'lp1',
    request: { title: 'Fix the total' },
    implementation: { snapshot_id: TARGET, bound_at: '2026-09-29T18:40:00Z', repository_identity: 'shop' },
    accepted_scope: { version: 1, criteria: LINES, allowed_scope: ['cart/'], check_profile: [], acceptance: { actor: 'sam', accepted_at: '2026-09-29T18:35:00Z' } },
    examination: 'none_yet',
    decisions: { target_revision: target, scope_version: 1, standing, history: history || (standing ? [standing] : []) },
    code_moved: null,
  }
}

/** The served coverage: one line Proven, one Needs a re-check, one with nothing, one Not proven. */
export function coverageDoc({ ready = false, states } = {}) {
  const st = states || ['proven', 'stale', 'none', 'not_proven']
  const criteria = LINES.map((text, i) => {
    const s = st[i]
    if (s === 'none') return { index: i + 1, text, state: 'none', items: [] }
    if (s === 'stale') return { index: i + 1, text, state: 'evidence', items: [item({ link_id: 'l' + (i + 1), evidence_id: 'e' + (i + 1), fresh: false, fresh_slug: 'applicability_needs_a_recheck' })] }
    if (s === 'not_proven') return { index: i + 1, text, state: 'evidence', items: [item({ link_id: 'l' + (i + 1), evidence_id: 'e' + (i + 1), observed: 'not_proven' })] }
    return { index: i + 1, text, state: 'evidence', items: [item({ link_id: 'l' + (i + 1), evidence_id: 'e' + (i + 1), fresh: true, fresh_slug: 'applicability_still_applies' })] }
  })
  const obligations = LINES.map((text, i) => ({
    id: 'criterion:' + (i + 1), criterion_index: i + 1, text,
    outcome: ready || st[i] === 'proven' ? 'satisfied' : 'needs_evidence',
    unsatisfied_slug: ready || st[i] === 'proven' ? null : (st[i] === 'stale' ? 'readiness_stale' : st[i] === 'none' ? 'readiness_nothing_looked' : 'readiness_not_proven'),
  }))
  return {
    scope_version: 1,
    link_rows: criteria.filter((c) => c.items.length).length,
    criteria,
    readiness: { ready, target_version: TARGET, headline_slug: ready ? 'readiness_ready' : 'readiness_needs_evidence', obligations, rests_on: [] },
  }
}

const APPLICABILITY = { items: [{ link_id: 'l2', criterion_index: 2, version: TARGET, reason_slug: 'applicability_reason_source_changed', reason_paths: ['cart/api.py'] }] }

/** `{id, change, model}` as `Repair-Loop/live.js` `readTracked` gives it. */
export function tracked(opts = {}) {
  const change = changeDoc(opts)
  const coverage = coverageDoc(opts)
  if (opts.unaccepted) {
    /* tracked, and nothing accepted yet: no scope, no lines, no readiness answer */
    change.accepted_scope = null
    coverage.scope_version = null
    coverage.criteria = []
    coverage.link_rows = 0
    coverage.readiness = { ready: false, target_version: TARGET, headline_slug: 'readiness_nothing_accepted', obligations: [], rests_on: [] }
  }
  const model = deriveChangeModel(change, coverage, APPLICABILITY, null, [])
  return { id: CHANGE_ID, change, coverage, proposals: null, model, runSheet: null }
}

/** The snapshot of that tracked Change, as `useChange` builds it. */
export function snapshot(t, over = {}, now = NOW) {
  return changeSnapshot(threadView(over), t ? { change: t.change, hq: t.model, proposals: null } : null, now)
}

/** A standing approval row (`change_decisions`, as `decisions.standing`). */
export const STANDING = Object.freeze({ id: 'dec1', act: 'approved', target_revision: TARGET, actor: 'sam', decided_at: '2026-09-29T19:00:00Z', scope_version: 1 })

/** `GET /api/changes/<id>/pr`, as the route serves it. */
export function prDoc(over = {}) {
  return {
    schema_version: 1,
    read_at: '2026-10-07T06:59:30Z',
    change_id: CHANGE_ID,
    folder: { recorded: true, present: true, name: 'shop', reason: null },
    git: { branch: 'fix-total', head: 'abcdef0123456789abcdef0123456789abcdef01', detached: false, reason: null },
    receipts: { dir: '.thruline/receipts', count: 0, newest: null, reason: null },
    check_run: { name: 'thruline/receipt' },
    badge: { states: [
      { conclusion: 'success', label: 'verified', colour: '#2ea043' },
      { conclusion: 'neutral', label: 'partial', colour: '#bf8700' },
      { conclusion: 'failure', label: 'not verified', colour: '#cf222e' },
    ] },
    masking: { masked: 0, families: [], complete: true },
    ...over,
  }
}
