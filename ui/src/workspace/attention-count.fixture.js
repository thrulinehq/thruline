/* NOT-APPROVAL-WORDS: a test fixture; it renders nothing (RUN-LINK-TRUTH-R5 census). */
/* A3-F2 — the generated stores the attention property test runs over.
 *
 * TEST-ONLY. Nothing in the product imports this file, so it never reaches
 * the bundle. Every record here is synthetic: ids are in the `cccccccc-`
 * family, titles are made up, and nothing came off a real machine.
 *
 * DETERMINISTIC. One seeded generator (mulberry32); the seed is part of every
 * failure message, so a failing store can be rebuilt exactly. No clock, no
 * locale, no model.
 *
 * What a store holds is what the Change routes serve: the `/api/changes`
 * list, and per Change the documents `readChangeDocuments` returns — the
 * Change, its proposals, its coverage (criteria, runs, readiness), its
 * applicability, and the confirmed and suggested work, adapted. The work's
 * verdict cards are built by the REAL `buildVerdict` from generated sessions,
 * so a finding of any severity reaches the Change layer exactly as it would
 * from the served claims.
 */

import { buildVerdict } from './verdict-model.js'

/** A small, fast, seedable PRNG. */
export function rng(seed) {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

const pad = (n, w) => String(n).padStart(w, '0')
const hex = (r, len) => Array.from({ length: len }, () => '0123456789abcdef'[Math.floor(r() * 16)]).join('')

/** Severities a detector finding can carry, and the detectors that make a card of their own. */
export const SEVERITIES = Object.freeze(['fail', 'warn', 'info'])
const WEAKENED = Object.freeze(['assertion_removed', 'skip_marker_added', 'tests_ran_before_final_edit'])
const OTHER = Object.freeze(['coverage_not_measured', 'test_added', 'reexecution_confirms'])

/**
 * One generated session, as the adapter hands it to `buildVerdict`.
 *
 * @param {Function} r the PRNG
 * @param {object} [force] `{ onlyInfo: true }` builds a session whose every
 *   finding is `info` severity on a claim the record supports
 */
export function aSession(r, id, force = {}) {
  const pick = (xs) => xs[Math.floor(r() * xs.length)]
  const claims = []
  const nClaims = force.onlyInfo ? 1 + Math.floor(r() * 3) : Math.floor(r() * 4)
  for (let k = 0; k < nClaims; k += 1) {
    const findings = []
    const nFindings = force.onlyInfo ? 1 + Math.floor(r() * 3) : Math.floor(r() * 4)
    for (let j = 0; j < nFindings; j += 1) {
      const severity = force.onlyInfo ? 'info' : pick(SEVERITIES)
      const key = r() < 0.5 ? pick(WEAKENED) : pick(OTHER)
      findings.push({ key, severity, name: key, node: `fnd-${id}-${k}-${j}`, detail: 'a synthetic finding', path: `src/file${j}.js` })
    }
    const verdict = force.onlyInfo ? 'supported' : pick(['supported', 'unsupported', 'unverifiable', null])
    claims.push({ id: `claim-${id}-${k}`, node: `clm-${id}-${k}`, text: 'A synthetic claim', kind: 'tests_pass', verdict, findings, evidence: [] })
  }
  const promises = force.onlyInfo ? [] : Array.from({ length: Math.floor(r() * 2) }, (_, k) => ({ id: `prm-${id}-${k}`, node: `prm-${id}-${k}`, text: 'A synthetic promise', status: pick(['open', 'dropped', 'fulfilled']) }))
  return {
    id,
    key: id,
    capture: { degraded: force.onlyInfo ? false : r() < 0.15 },
    harness: 'the agent',
    claims,
    promises,
    checks: [],
    events: [],
    nodes: [],
  }
}

/** One piece of adapted work, the shape `readChangeDocuments` builds. */
export function aWork(session) {
  const verdict = buildVerdict(session)
  /* `session` is kept beside the adapted work so a test can rebuild the same
   * work with some findings taken away; the product never reads it. */
  return { id: session.id, findings: verdict ? verdict.findings : [], verdict, focus: null, steps: 1, session }
}

const CRITERION_STATES = Object.freeze(['none', 'proven', 'not_proven', 'settled_nothing', 'stale'])
const RUN_STATES = Object.freeze(['running', 'completed', 'cancelled', 'interrupted', 'blocked'])

/**
 * One generated Change and every document the reader returns for it.
 *
 * @returns {{row: object, docs: object}}
 */
export function aChange(r, index, openedAt, force = {}) {
  const pick = (xs) => xs[Math.floor(r() * xs.length)]
  const id = `cccccccc-0000-4000-8000-${pad(index, 12)}`
  const nCriteria = force.criteria != null ? force.criteria : Math.floor(r() * 5)
  const criteria = Array.from({ length: nCriteria }, (_, k) => `Synthetic line ${k + 1}`)
  const bound = force.onlyInfo ? true : r() < 0.7
  const snapshot = hex(r, 40)
  const nSessions = force.onlyInfo ? 1 : Math.floor(r() * 3)
  const sessions = Array.from({ length: nSessions }, (_, k) => aSession(r, `cccccccc-1111-4000-8000-${pad(index, 8)}${pad(k, 4)}`, force))
  const provenance = sessions.map(() => (force.onlyInfo ? 'confirmed' : pick(['confirmed', 'confirmed', 'suggested', 'rejected'])))
  const change = {
    id,
    project_id: 'cccccccc-0000-4000-8000-00000000p001',
    opened_at: openedAt,
    request: { title: `Synthetic Change ${index}`, purpose: 'Generated for the attention property test.', requester: 'sam', source: 'person' },
    accepted_scope: nCriteria
      ? { version: 1, criteria, allowed_scope: ['src/'], check_profile: [], acceptance: { actor: 'sam', accepted_at: openedAt } }
      : null,
    implementation: bound ? { snapshot_id: snapshot, bound_at: openedAt, repository_identity: 'synthetic-repo' } : null,
    associations: sessions.map((s, k) => ({ session_id: s.id, provenance: provenance[k] })),
    examination: 'none_yet',
    decision: force.onlyInfo ? 'none_yet' : pick(['none_yet', 'none_yet', 'awaiting_approval']),
    decisions: !force.onlyInfo && bound && r() < 0.2
      ? { target_revision: snapshot, standing: { act: 'approved', target_revision: snapshot, actor: 'sam', decided_at: openedAt, scope_version: 1, exceptions: [] }, history: [] }
      : null,
  }
  const rows = criteria.map((text, k) => {
    const state = force.onlyInfo ? 'proven' : pick(CRITERION_STATES)
    if (state === 'none') return { index: k + 1, text, state: 'none', items: [] }
    const item = {
      link_id: `l-${index}-${k}`, evidence_id: `e-${index}-${k}`, session_id: sessions.length ? sessions[0].id : 's', basis: 'declared',
      kind: 'check_run', name: 'tests pass',
      observed: state === 'not_proven' ? 'not_proven' : 'proven',
      bears: state !== 'settled_nothing',
      applies: true, applies_slug: 'inputs_applies',
      fresh: state !== 'stale', fresh_slug: state === 'stale' ? 'applicability_needs_a_recheck' : 'applicability_still_applies',
    }
    return { index: k + 1, text, state: 'evidence', items: [item] }
  })
  const nRuns = force.onlyInfo ? 0 : Math.floor(r() * 3)
  const executions = Array.from({ length: nRuns }, (_, k) => {
    const state = pick(RUN_STATES)
    return { run_key: `run-${index}-${k}`, name: 'tests pass', state, state_slug: `execution_${state}`, settles: state === 'completed' }
  })
  const allProven = rows.length > 0 && rows.every((row) => row.state === 'evidence' && row.items[0].observed === 'proven' && row.items[0].bears && row.items[0].fresh)
  const readiness = force.onlyInfo
    ? null
    : r() < 0.3
      ? null
      : {
        ready: allProven && bound && r() < 0.8,
        target_version: snapshot,
        headline_slug: allProven ? 'readiness_ready' : 'readiness_needs_evidence',
        obligations: [],
        rests_on: rows.map((row, k) => ({ obligation_id: `o-${k}`, text: row.text, evidence_name: 'tests pass' })),
        with_exceptions: false,
      }
  const stale = rows.filter((row) => row.state === 'evidence' && row.items[0].fresh === false)
  const applicability = stale.length
    ? { items: stale.map((row) => ({ link_id: row.items[0].link_id, criterion_index: row.index, version: snapshot.slice(0, 7), reason_slug: 'applicability_reason_source_changed', reason_paths: ['src/a.js'] })) }
    : null
  const coverage = { scope_version: nCriteria ? 1 : null, link_rows: rows.filter((row) => row.items.length).length, criteria: rows, executions, readiness }
  return {
    row: { id, opened_at: openedAt, request: change.request, accepted_scope: change.accepted_scope },
    docs: {
      status: 'ready',
      change,
      proposals: null,
      coverage,
      applicability,
      /* Only the confirmed and the suggested work is read; a rejected session is not. */
      work: sessions.filter((_, k) => provenance[k] !== 'rejected').map(aWork),
      notes: [],
    },
  }
}

/**
 * One generated store: 0–40 Changes, so the Inbox's 25-Change read limit is
 * crossed in a good share of them. Opening times are distinct, so "newest
 * first" is a total order and the read set is well defined.
 *
 * @returns {{seed: number, all: object[], docsById: Map<string, object>}}
 */
export function aStore(seed, opts = {}) {
  const r = rng(seed)
  const n = opts.count != null ? opts.count : Math.floor(r() * 41)
  const order = Array.from({ length: n }, (_, k) => k)
  for (let k = n - 1; k > 0; k -= 1) {
    const j = Math.floor(r() * (k + 1))
    const t = order[k]; order[k] = order[j]; order[j] = t
  }
  const all = []
  const docsById = new Map()
  for (let k = 0; k < n; k += 1) {
    const minute = order[k]
    const openedAt = `2026-09-${pad(1 + Math.floor(minute / 60), 2)}T10:${pad(minute % 60, 2)}:00Z`
    const { row, docs } = aChange(r, k, openedAt, opts.force || {})
    all.push(row)
    docsById.set(row.id, docs)
  }
  return { seed, all, docsById }
}
