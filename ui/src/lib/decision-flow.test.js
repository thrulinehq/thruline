import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { EVENT_PAGE_SIZE, filterDecisionFlow, decisionFlow, windowDecisionFlow } from './decision-flow.js'
import { surfaceProjection } from './fixture.js'
import { chipLabel, word } from './voice.js'

const representativePath = fileURLToPath(new URL('../../../crates/core/tests/fixtures/grounded-mind-public.json', import.meta.url))

const node = (id, sessionId, state, extra = {}) => ({
  id,
  label: `short ${id}`,
  sort_key: state,
  session_id: sessionId,
  evidence_count: 0,
  anchor_event_id: `event-${id}`,
  accusation: state === 'contradicted',
  dimensions: {
    lifecycle: state === 'pending' ? 'proposed' : 'completed',
    evidence: state === 'contradicted' ? 'contradicted' : state === 'pending' ? 'unknown' : 'observed',
    currency: 'current',
  },
  ...extra,
})

const DOC = {
  contract_version: 2,
  diagnostics: false,
  sessions: [
    { id: 's2', label: 'Session 2', date: 'Aug 2', msgs: 20 },
    { id: 's1', label: 'Session 1', date: 'Aug 1', msgs: 10 },
  ],
  nodes: [
    node('a', 's1', 'recorded'),
    node('b', 's2', 'pending'),
    node('c', 's1', 'contradicted', { evidence_count: 2 }),
    node('d', 'orphan', 'recorded'),
    node('shadow', 's2', 'dropped', {
      dimensions: { lifecycle: 'abandoned', evidence: 'unknown', currency: 'superseded' },
    }),
  ],
  edges: [
    { from: 'a', to: 'c', kind: 'seq' },
    { from: 'b', to: 'a', kind: 'resume' },
    { from: 'c', to: 'd', kind: 'seq' },
    { from: 'b', to: 'shadow', kind: 'seq' },
  ],
  receipts: {
    a: { claim: { text: 'Full receipt text for alpha', message_no: 1, ts: '2026-08-01T10:00:00Z' }, evidence: [{ summary: 'Read exact alpha file', path: '/work/src/alpha.js', kind: 'command' }] },
    b: { claim: { text: 'Full receipt text for beta', message_no: 2, ts: '2026-08-02T10:00:00Z' }, evidence: [] },
    c: { claim: { text: 'Full receipt text for gamma', message_no: 3, ts: '2026-08-01T10:01:00Z' }, evidence: [{}, {}] },
    d: { claim: { text: 'Full receipt text for delta', message_no: 4, ts: '2026-08-03T10:00:00Z' }, evidence: [] },
  },
  candidates: {
    opening_asks: [{ session_id: 's1', event_id: 'ask-1', text: 'Please inspect this.' }],
  },
}

test('session groups and nodes retain canonical export order without dropping an unlisted run', () => {
  const flow = decisionFlow(DOC)
  assert.deepEqual(flow.groups.map((group) => group.session.id), ['s2', 's1', 'orphan'])
  assert.deepEqual(flow.groups[0].nodes.map((item) => item.id), ['b'])
  assert.deepEqual(flow.groups[1].nodes.map((item) => item.id), ['a', 'c'])
  assert.deepEqual(flow.groups[2].nodes.map((item) => item.id), ['d'])
  assert.deepEqual(flow.groups[1].openingAsks, DOC.candidates.opening_asks)
})

test('every ordinary projected node id appears exactly once', () => {
  const projected = surfaceProjection(DOC)
  const flow = decisionFlow(DOC)
  const ids = flow.groups.flatMap((group) => group.nodes.map((item) => item.id))
  assert.deepEqual([...ids].sort(), projected.nodes.map((item) => item.id).sort())
  assert.equal(new Set(ids).size, ids.length)
  assert.ok(!ids.includes('shadow'))
})

test('no edge or record is invented and exported edge kinds survive', () => {
  const projected = surfaceProjection(DOC)
  const flow = decisionFlow(DOC)
  assert.deepEqual(flow.edges, projected.edges)
  const exported = new Set(projected.edges.map((edge) => `${edge.from}->${edge.to}:${edge.kind}`))
  for (const item of flow.groups.flatMap((group) => group.nodes)) {
    assert.equal(item.record.id, item.id)
    for (const edge of [...item.lineage.incoming, ...item.lineage.outgoing]) {
      assert.ok(exported.has(`${edge.from}->${edge.to}:${edge.kind}`), `invented edge at ${item.id}`)
    }
  }
  assert.deepEqual(flow.groups[1].nodes[0].lineage.kinds.sort(), ['resume', 'seq'])
})

test('full receipt text is used without rewriting the exported node', () => {
  const item = decisionFlow(DOC).groups[1].nodes[0]
  assert.equal(item.text, 'Full receipt text for alpha')
  assert.equal(item.label, 'short a')
  assert.equal(item.record.label, 'short a')
  assert.equal(item.messageNo, 1)
  assert.equal(item.anchorEventId, 'event-a')
})

test('state and text filters preserve order and report exact coverage', () => {
  const flow = decisionFlow(DOC)
  const pending = filterDecisionFlow(flow, { filters: ['pending'] })
  assert.equal(pending.totalCount, 4)
  assert.equal(pending.shownCount, 1)
  assert.equal(pending.hiddenCount, 3)
  assert.deepEqual(pending.groups.flatMap((group) => group.nodes.map((item) => item.id)), ['b'])

  const fullText = filterDecisionFlow(flow, { search: 'receipt text for gamma' })
  assert.equal(fullText.shownCount, 1)
  assert.deepEqual(fullText.groups[0].nodes.map((item) => item.id), ['c'])

  const empty = filterDecisionFlow(flow, { search: 'does not exist' })
  assert.equal(empty.shownCount, 0)
  assert.equal(empty.hiddenCount, 4)
  assert.deepEqual(empty.groups, [])

  const evidence = filterDecisionFlow(flow, { search: '/work/src/alpha.js' })
  assert.deepEqual(evidence.groups.flatMap((group) => group.nodes.map((item) => item.id)), ['a'])
})

test('filtering happens before a bounded window and every match remains reachable', () => {
  const doc = JSON.parse(readFileSync(representativePath, 'utf8'))
  const flow = decisionFlow(doc)
  const filtered = filterDecisionFlow(flow)
  const first = windowDecisionFlow(filtered)
  assert.equal(EVENT_PAGE_SIZE, 40)
  assert.equal(first.matchingCount, 228)
  assert.equal(first.mountedCount, 40)
  assert.equal(first.remainingCount, 188)

  const all = windowDecisionFlow(filtered, { limits: { [first.groups[0].session.id]: Infinity } })
  assert.equal(all.mountedCount, 228)
  assert.deepEqual(all.groups[0].nodes.map((item) => item.id), flow.nodeIds)

  const query = filterDecisionFlow(flow, { search: flow.groups[0].nodes.at(-1).id })
  const queriedWindow = windowDecisionFlow(query)
  assert.equal(queriedWindow.matchingCount, 1)
  assert.equal(queriedWindow.mountedCount, 1)
})

test('a selected node expands only its run far enough to include it', () => {
  const nodes = Array.from({ length: 95 }, (_, index) => node(`n${index}`, 's1', 'recorded'))
  const doc = {
    ...DOC,
    sessions: [{ id: 's1', label: 'Session 1' }],
    nodes,
    edges: [],
    receipts: Object.fromEntries(nodes.map((item) => [item.id, { claim: { text: item.id }, evidence: [] }])),
  }
  const filtered = filterDecisionFlow(decisionFlow(doc))
  const visible = windowDecisionFlow(filtered, { selected: 'n87' })
  assert.equal(visible.mountedCount, 95, 'the selected node lands in the next exact page boundary')
  assert.ok(visible.groups[0].nodes.some((item) => item.id === 'n87'))
})

test('every status label comes through the existing voice authority', () => {
  for (const item of decisionFlow(DOC).groups.flatMap((group) => group.nodes)) {
    assert.equal(item.statusWord, word(item.state))
    assert.equal(item.statusLabel, chipLabel(word(item.state)))
  }
})

test('the tracked synthetic representative projects all 228 nodes and 227 exported edges', () => {
  const doc = JSON.parse(readFileSync(representativePath, 'utf8'))
  const flow = decisionFlow(doc)
  assert.equal(flow.totalCount, 228)
  assert.equal(flow.edges.length, 227)
  assert.equal(flow.groups.length, 1)
  assert.deepEqual(flow.groups[0].nodes.map((item) => item.id), doc.nodes.map((item) => item.id))
  assert.equal(flow.groups[0].nodes.filter((item) => item.receipt).length, 228)
})

test('the candidate work map is primary and the progressive event flow remains a subordinate receipt door', () => {
  const shell = readFileSync(fileURLToPath(new URL('../mac/shell.jsx', import.meta.url)), 'utf8')
  const component = readFileSync(fileURLToPath(new URL('../mac/decision-flow.jsx', import.meta.url)), 'utf8')
  const workMap = readFileSync(fileURLToPath(new URL('../mac/work-map.jsx', import.meta.url)), 'utf8')
  assert.match(shell, /import DecisionFlow from '\.\/decision-flow\.jsx'/)
  assert.match(shell, /import WorkMap from '\.\/work-map\.jsx'/)
  assert.doesNotMatch(shell, /import Graph from '\.\/graph\.jsx'/)
  assert.match(shell, /useState\('work'\)/)
  assert.match(shell, /<WorkMap/)
  assert.match(shell, /setAltitude\('events'\)/)
  assert.match(shell, /<DecisionFlow fixture=\{fixture\}/)
  assert.match(shell, /All events \(\{totalNodes\}\)/)
  assert.match(shell, /Distilled summary/)
  assert.match(workMap, /What the record shows/)
  assert.match(component, /Recorded order · not causality/)
  assert.match(component, /Showing \$\{filtered\.shownCount\} of \$\{filtered\.totalCount\} events/)
  assert.match(component, /Show next/)
  assert.match(component, /Show all/)
  assert.match(component, /onKeyDown=/)
  assert.match(component, /onSelect\(node\.id\)/)
  for (const oldControl of ['setZoom', 'setPan', 'minimap', '<Timeline']) {
    assert.ok(!component.includes(oldControl), `vertical flow retained ${oldControl}`)
  }
})
