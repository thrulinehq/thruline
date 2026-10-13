import assert from 'node:assert/strict'
import test from 'node:test'

import {
  attachExportedCountDiscrepancy,
  boundedCountDiscrepancy,
  exportedCountDiscrepancy,
  prepare,
  presentationState,
  surfaceProjection,
} from './fixture.js'
import {
  CAPTURE_OPERATIONAL_STATES,
  CAPTURE_UX_SCENARIOS,
  captureKeyboardActivation,
  captureProjectView,
} from './capture.js'

const fixture = {
  project: { id: 'synthetic', name: 'Synthetic', source: 'claude_code' },
  generated_at: '2026-07-31T00:00:00Z',
  ledger: { events: 3, head: 'abc', verified: true },
  sessions: [{ id: 's', label: 'Session 1', date: 'Jul 31', msgs: 3 }],
  counts: { supported: 1, unsupported: 1, dropped: 1 },
  accusations: 1,
  nodes: [
    { id: 'ok', state: 'supported', session_id: 's' },
    { id: 'quiet', state: 'unsupported', session_id: 's' },
    { id: 'ghost', state: 'dropped', session_id: 's' },
  ],
  edges: [
    { from: 'ok', to: 'quiet', kind: 'seq' },
    { from: 'quiet', to: 'ghost', kind: 'seq' },
  ],
  receipts: { ok: {}, quiet: {}, ghost: {} },
}

const publicCounts = (overrides = {}) => ({
  contradicted: 0,
  pending: 0,
  recorded: 0,
  refused: 0,
  supported: 0,
  unknown: 0,
  ...overrides,
})

test('exported count comparison reports exact missing, extra, and unequal keys', () => {
  const counts = publicCounts({ recorded: 1, supported: 9 })
  delete counts.pending
  counts.unexpected = 7
  const got = exportedCountDiscrepancy({
    counts,
    nodes: [
      { id: 'recorded', state: 'recorded' },
      { id: 'supported', state: 'supported' },
    ],
  })

  assert.deepEqual(got, {
    missing: [{ key: 'pending', nodes: 0 }],
    extra: [{ key: 'unexpected', exported: 7 }],
    unequal: [{ key: 'supported', exported: 9, nodes: 1 }],
  })
})

test('public count comparison excludes full-shadow nodes before checking totals', () => {
  const got = exportedCountDiscrepancy({
    contract_version: 2,
    counts: publicCounts({ supported: 1 }),
    nodes: [
      {
        id: 'visible',
        sort_key: 'supported',
        dimensions: { lifecycle: 'completed', evidence: 'supported', currency: 'current' },
      },
      {
        id: 'hidden',
        sort_key: 'unsupported',
        dimensions: { lifecycle: 'completed', evidence: 'supported', currency: 'current' },
      },
    ],
  })
  assert.equal(got, null)
})

test('diagnostic count comparison preserves shadow sort keys rather than badge projection', () => {
  const got = exportedCountDiscrepancy({
    contract_version: 2,
    diagnostics: true,
    counts: { ...publicCounts(), dropped: 1, unsupported: 1 },
    nodes: [
      {
        id: 'unsupported',
        sort_key: 'unsupported',
        dimensions: { lifecycle: 'completed', evidence: 'supported', currency: 'current' },
      },
      {
        id: 'dropped',
        sort_key: 'dropped',
        dimensions: { lifecycle: 'proposed', evidence: 'unknown', currency: 'current' },
      },
    ],
  })
  assert.equal(got, null)
})

test('prototype-shaped node keys remain own counts and fail loud on mismatch', () => {
  const got = exportedCountDiscrepancy({
    counts: publicCounts(),
    nodes: [
      { id: 'proto', state: '__proto__' },
      { id: 'constructor', state: 'constructor' },
      { id: 'prototype', state: 'prototype' },
    ],
  })

  assert.deepEqual(got, {
    missing: [
      { key: '__proto__', nodes: 1 },
      { key: 'constructor', nodes: 1 },
      { key: 'prototype', nodes: 1 },
    ],
    extra: [],
    unequal: [],
  })
})

test('an own prototype-shaped exported count is compared rather than inherited', () => {
  const counts = JSON.parse(JSON.stringify(publicCounts()))
  Object.defineProperty(counts, '__proto__', {
    configurable: true,
    enumerable: true,
    value: 0,
    writable: true,
  })
  const got = exportedCountDiscrepancy({
    counts,
    nodes: [{ id: 'proto', state: '__proto__' }],
  })

  assert.deepEqual(got, {
    missing: [],
    extra: [],
    unequal: [{ key: '__proto__', exported: 0, nodes: 1 }],
  })
})

test('fixtures attach only real discrepancies and retain a bounded key/count report', () => {
  const valid = attachExportedCountDiscrepancy({
    counts: publicCounts({ recorded: 1 }),
    nodes: [{ id: 'one', state: 'recorded' }],
  })
  assert.equal(valid._count_discrepancy, undefined)

  const extras = Object.fromEntries(Array.from({ length: 19 }, (_, index) => [`extra_${index}`, index]))
  extras[`a${'🔒'.repeat(80)}`] = 19
  const mismatched = attachExportedCountDiscrepancy({ counts: extras, nodes: [] })
  assert.ok(mismatched._count_discrepancy)
  const bounded = boundedCountDiscrepancy(mismatched._count_discrepancy)
  assert.equal(bounded.missing.length + bounded.extra.length + bounded.unequal.length, 12)
  assert.equal(bounded.omitted, 14)
  const boundedLongKey = bounded.extra.find((row) => row.key.startsWith('a'))?.key
  assert.equal([...boundedLongKey].length, 64)
  assert.ok(boundedLongKey.endsWith('…'))
  assert.deepEqual(
    Object.keys(bounded).sort(),
    ['extra', 'missing', 'omitted', 'unequal'],
  )
  for (const row of [...bounded.missing, ...bounded.extra, ...bounded.unequal]) {
    assert.deepEqual(Object.keys(row).sort(), Object.keys(row).filter((key) => ['key', 'nodes', 'exported'].includes(key)).sort())
  }
})

test('ordinary UI projection cannot surface full-shadow verdicts', () => {
  const got = surfaceProjection(fixture)
  assert.deepEqual(got.nodes.map((n) => n.id), ['ok'])
  assert.deepEqual(got.edges, [])
  assert.deepEqual(Object.keys(got.receipts), ['ok'])
  assert.equal(got.counts.unsupported, undefined)
  assert.equal(got.counts.dropped, undefined)
  assert.equal(got.accusations, 0)
})

test('capture-related graph targets use standard keyboard activation keys', () => {
  assert.equal(captureKeyboardActivation('Enter'), true)
  assert.equal(captureKeyboardActivation(' '), true)
  assert.equal(captureKeyboardActivation('Escape'), false)
})

test('diagnostics projection preserves every shadow row and receipt', () => {
  const diagnostic = { ...fixture, diagnostics: true }
  const got = surfaceProjection(diagnostic)
  assert.deepEqual(got.nodes.map((n) => n.id), ['ok', 'quiet', 'ghost'])
  assert.deepEqual(got.nodes.map((n) => n.sort_key), ['supported', 'unsupported', 'dropped'])
  assert.deepEqual(Object.keys(got.receipts), ['ok', 'quiet', 'ghost'])
})

test('v2 dimensions outrank an inconsistent legacy state and preserve the diagnostic key', () => {
  const got = surfaceProjection({
    ...fixture,
    contract_version: 2,
    diagnostics: true,
    nodes: [{
      id: 'dimensional',
      state: 'contradicted',
      sort_key: 'unsupported',
      session_id: 's',
      dimensions: { lifecycle: 'completed', evidence: 'supported', currency: 'current' },
    }],
    edges: [],
    receipts: { dimensional: {} },
  })

  assert.equal(got.nodes[0].state, 'supported')
  assert.equal(got.nodes[0].sort_key, 'unsupported')
})

test('public projection hides a shadow sort key even when dimensions map to a calm badge', () => {
  const got = surfaceProjection({
    ...fixture,
    contract_version: 2,
    nodes: [{
      id: 'shadow',
      state: 'supported',
      sort_key: 'dropped',
      session_id: 's',
      dimensions: { lifecycle: 'completed', evidence: 'supported', currency: 'current' },
    }],
    edges: [],
    receipts: { shadow: {} },
  })
  assert.deepEqual(got.nodes, [])
})

test('v1 demo nodes keep their flat-state presentation', () => {
  const got = surfaceProjection({
    ...fixture,
    diagnostics: true,
    nodes: [{ id: 'legacy', state: 'refused', session_id: 's' }],
    edges: [],
    receipts: { legacy: {} },
  })
  assert.equal(got.nodes[0].state, 'refused')
  assert.equal(got.nodes[0].sort_key, 'refused')
})

test('malformed v2 dimensions fail closed instead of reviving legacy state', () => {
  assert.equal(presentationState({
    state: 'supported',
    dimensions: { lifecycle: 'completed', evidence: 'mystery', currency: 'current' },
  }, 2), 'unknown')
})

test('prepare exposes normalized presentation and diagnostic counts through the view', () => {
  const got = prepare({
    ...fixture,
    contract_version: 2,
    diagnostics: true,
    nodes: [{
      id: 'dimensional',
      state: 'contradicted',
      sort_key: 'unsupported',
      session_id: 's',
      anchor_event_id: 'event-1',
      dimensions: { lifecycle: 'completed', evidence: 'supported', currency: 'current' },
    }],
    edges: [],
    receipts: {
      dimensional: {
        evidence: [],
        claim: {
          claims: [{
            evidence_scopes: [{
              freshness: 'stale',
              freshness_note: 'verified at `s:2`; superseded by `s:4`',
            }],
          }],
        },
      },
    },
  })
  assert.equal(got._view.counts.supported, 1)
  assert.equal(got._view.counts.contradicted, 0)
  assert.equal(got._view.sort_counts.unsupported, 1)
  assert.deepEqual(
    got._view.receipts.dimensional.freshness_notes,
    ['verified at `s:2`; superseded by `s:4`'],
  )
})

test('M5.R2 binds exactly twelve operational states and twenty-two UX scenarios', () => {
  assert.equal(CAPTURE_OPERATIONAL_STATES.length, 12)
  assert.equal(new Set(CAPTURE_OPERATIONAL_STATES).size, 12)
  assert.equal(CAPTURE_UX_SCENARIOS.length, 22)
  assert.deepEqual(CAPTURE_UX_SCENARIOS.map((item) => item.id), Array.from({ length: 22 }, (_, i) => i + 1))
  assert.equal(new Set(CAPTURE_UX_SCENARIOS.map((item) => item.scenario)).size, 22)
  for (const item of CAPTURE_UX_SCENARIOS) {
    assert.ok(item.title.length > 0)
    assert.ok(item.body.length > 0)
    assert.equal(`${item.title} ${item.body}`.includes(item.forbidden), false)
  }
})

test('capture readiness and proof currency remain independent', () => {
  const view = captureProjectView({
    project: { source: 'codex' },
    assured_capture: {
      operational_state: 'assured_ready',
      project_authorized: true,
      explanation: 'Accepted profile and authorization are current.',
      accepted_capture_count: 2,
    },
    capture_receipts: {
      a: { strength: 'assured', currentness: 'current' },
      b: { strength: 'assured', currentness: 'stale' },
    },
  })
  assert.equal(view.state, 'assured_ready')
  assert.equal(view.ready, true)
  assert.equal(view.currency, '1 stale proof receipt')
})

test('accepted capture receipt is joined to a node without altering its truth state', () => {
  const got = prepare({
    ...fixture,
    nodes: [{ id: 'ok', state: 'supported', session_id: 's', anchor_event_id: 'event-ok' }],
    edges: [],
    receipts: { ok: { claim: { claims: [] }, evidence: [] } },
    capture_receipts: {
      'event-ok': {
        strength: 'assured',
        currentness: 'stale',
        execution: { accepted_at_unix_ms: 10 },
        proof_history: [{
          currentness: 'stale',
          changed_dimensions: ['repository'],
          reason: 'repository_changed',
          observed_at_unix_ms: 20,
        }],
      },
    },
  })
  assert.equal(got._view.nodes[0].state, 'supported')
  assert.equal(got._view.receipts.ok.capture.strength, 'Assured at execution')
  assert.equal(got._view.receipts.ok.capture.currency, 'Stale as of accepted observation')
  assert.deepEqual(got._view.receipts.ok.capture.history[0].changed, ['repository'])
})
