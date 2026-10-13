/* RA5 — the graph layout: recorded order and exported lineage, nothing else. */

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { ancestors, layout } from './layout.js'

const REPRESENTATIVE_FIXTURE = fileURLToPath(new URL('../../../crates/core/tests/fixtures/grounded-mind-public.json', import.meta.url))

const node = (id, sessionId, state, extra = {}) => ({
  id,
  label: `claim ${id}`,
  sort_key: state,
  session_id: sessionId,
  evidence_count: 0,
  anchor_event_id: `E-${id}`,
  accusation: state === 'contradicted',
  dimensions: {
    lifecycle: 'completed',
    evidence: state === 'contradicted' ? 'contradicted' : 'observed',
    currency: 'current',
  },
  ...extra,
})

const DOC = {
  contract_version: 2,
  project: { id: 'p', name: 'synth', source: 'claude_code' },
  diagnostics: false,
  sessions: [
    { id: 's1', label: 'Session 1', date: 'Jul 27', msgs: 84 },
    { id: 's2', label: 'Session 2', date: 'Jul 29', msgs: 183 },
  ],
  nodes: [
    node('a', 's1', 'recorded'),
    node('b', 's1', 'recorded'),
    node('c', 's1', 'contradicted'),
    node('d', 's2', 'recorded'),
    /* shadow — must not reach the canvas */
    node('x', 's2', 'dropped', { dimensions: { lifecycle: 'abandoned', evidence: 'unknown', currency: 'superseded' } }),
  ],
  edges: [
    { from: 'a', to: 'b', kind: 'seq' },
    { from: 'b', to: 'c', kind: 'seq' },
    { from: 'c', to: 'd', kind: 'seq' },
    { from: 'd', to: 'x', kind: 'seq' },
  ],
  receipts: {},
}

test('lanes are the sessions, in export order, each labeled with its own facts', () => {
  const g = layout(DOC)
  assert.equal(g.bands.length, 2)
  assert.equal(g.bands[0].label, 'Session 1 · Jul 27 · 84 msgs')
  assert.equal(g.bands[1].label, 'Session 2 · Jul 29 · 183 msgs')
  assert.ok(g.bands[1].top > g.bands[0].top, 'lanes must not reorder the sessions')
})

test('nodes sit in their own lane, columns walking the recorded order', () => {
  const g = layout(DOC)
  const byId = Object.fromEntries(g.nodes.map((n) => [n.id, n]))
  assert.ok(byId.a.x < byId.b.x, 'recorded order must read left to right')
  const band1 = g.bands[0]
  for (const id of ['a', 'b', 'c']) {
    assert.ok(byId[id].y >= band1.top && byId[id].y < band1.top + band1.h, `${id} escaped its lane`)
  }
})

test('every edge on screen exists in the export, with its kind as provenance', () => {
  const g = layout(DOC)
  const exported = new Set(DOC.edges.map((e) => `${e.from}->${e.to}`))
  for (const e of g.edges) {
    assert.ok(exported.has(`${e.from}->${e.to}`), `invented edge ${e.from}->${e.to}`)
    assert.ok(e.kind, 'an edge lost its provenance')
  }
  // The edge into the shadow node never arrives: surfaceProjection prunes
  // edges touching filtered nodes before this file sees them, and the layout's
  // own skip counter guards only the residue (an edge whose end is missing for
  // any other reason).
  assert.equal(g.edges.length, 3)
  assert.ok(!g.edges.some((e) => e.to === 'x' || e.from === 'x'))
})

test('no shadow class reaches the canvas', () => {
  const g = layout(DOC)
  assert.ok(!g.nodes.some((n) => n.id === 'x'), 'a shadow node reached the canvas')
  assert.ok(!('dropped' in g.counts))
})

test('the trail holds eaten states only, in recorded order, or does not exist', () => {
  const g = layout(DOC)
  const t1 = g.trails.find((t) => t.session === 's1')
  assert.ok(t1, 'session 1 has two eaten claims — it has a trail')
  assert.equal(t1.points.length, 2, 'the contradicted claim must not be on the trail')
  assert.ok(!g.trails.some((t) => t.session === 's2'), 'one eaten claim is not a walk')
})

test('a cross-session edge is marked as crossing, and lineage walks edges only', () => {
  const g = layout(DOC)
  const cross = g.edges.find((e) => e.from === 'c' && e.to === 'd')
  assert.equal(cross.crossSession, true)
  assert.deepEqual([...ancestors(g.edges, 'c')].sort(), ['a', 'b'])
  assert.deepEqual([...ancestors(g.edges, 'a')], [])
})

test('two derivations of one document draw the same picture', () => {
  assert.deepEqual(layout(DOC), layout(DOC))
})

test('the layout holds over the tracked synthetic representative', () => {
  const doc = JSON.parse(readFileSync(REPRESENTATIVE_FIXTURE, 'utf8'))
  const g = layout(doc)
  const exported = new Set((doc.edges || []).map((edge) => `${edge.from}->${edge.to}`))
  for (const edge of g.edges) assert.ok(exported.has(`${edge.from}->${edge.to}`), 'invented edge')
  for (const band of g.bands) assert.ok(band.label.length > 0, 'an unlabeled lane')
  assert.equal(g.nodes.length, 228)
  assert.equal(g.edges.length, 227)
})
