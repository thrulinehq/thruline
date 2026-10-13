import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { INTERPRETATION_EMPTY, candidateFilePresentation, exactAnchorReceipt, workMap } from './work-map.js'

const representativePath = fileURLToPath(new URL('../../../crates/core/tests/fixtures/grounded-mind-public.json', import.meta.url))

const node = (id, sessionId, eventId) => ({
  id,
  label: id,
  sort_key: 'recorded',
  session_id: sessionId,
  evidence_count: 1,
  anchor_event_id: eventId,
  dimensions: { lifecycle: 'completed', evidence: 'observed', currency: 'current' },
})

const SYNTH = {
  contract_version: 2,
  diagnostics: false,
  project: { id: 'p', name: 'p' },
  sessions: [
    { id: 's2', label: 'Session 2', date: 'Aug 2', msgs: 20 },
    { id: 's1', label: 'Session 1', date: 'Aug 1', msgs: 10 },
  ],
  nodes: [node('n1', 's1', 'event-1'), node('n2', 's2', 'event-2')],
  edges: [],
  receipts: {
    n1: { claim: { text: 'one' }, evidence: [{ path: '/repo/src/a.js', summary: 'Read a.js', kind: 'command', scope: { cwd: '/repo' } }] },
    n2: { claim: { text: 'two' }, evidence: [{ path: '/other/a.js', summary: 'Read outside', kind: 'command', scope: { cwd: '/repo' } }] },
  },
  candidates: {
    opening_asks: [{ session_id: 's1', event_id: 'event-1', text: '# Inspect **this**…', continuation: false }],
    clusters: [
      { session_id: 's1', file: '/repo/src/a.js', edits: 2, first_event_id: 'event-1', last_event_id: 'missing', interpretation: 'Deploy immediately' },
      { session_id: 's2', file: '/repo/src/b.js', edits: 1, first_event_id: 'event-2', last_event_id: 'event-2', interpretation: 'A final outcome' },
    ],
    boundaries: [{ session_id: 's1', event_id: 'missing-boundary', message: '**checkpoint**' }],
    tails: [{ session_id: 's2', edits_after_last_boundary: 3, files: ['/repo/src/b.js'], interpretation: 'abandoned work' }],
    artifacts: [
      { file: '/repo/src/a.js', edits: 2, fate: 'exists', sessions: ['s1'] },
      { file: '/repo/src/b.js', edits: 1, fate: 'unresolved', sessions: ['s2'] },
      { file: '/repo/src/c.js', edits: 1, fate: 'future_value', sessions: ['s2'] },
    ],
  },
}

test('representative candidate counts and once-only coverage are exact', () => {
  const source = readFileSync(representativePath, 'utf8')
  const doc = JSON.parse(source)
  const map = workMap(doc)
  assert.equal(doc.project.id, '0123456789abcdef')
  assert.deepEqual(doc.sessions.map((session) => session.id), ['public-session-01'])
  assert.equal(doc.nodes.length, 228)
  assert.equal(doc.edges.length, 227)
  assert.equal(Object.keys(doc.receipts).length, 228)
  assert.equal(new Set(doc.nodes.map((node) => node.id)).size, 228)
  assert.deepEqual(new Set(Object.keys(doc.receipts)), new Set(doc.nodes.map((node) => node.id)))
  assert.ok(doc.edges.every((edge) => edge.kind === 'seq'))
  assert.doesNotMatch(source, /\/Users\/|\\Users\\|command_identity/)
  assert.deepEqual(map.counts, { openingAsks: 1, clusters: 26, boundaries: 17, tails: 0, artifacts: 18 })
  assert.equal(map.sessions.flatMap((session) => session.openingAsks).length, 1)
  assert.equal(map.sessions.flatMap((session) => session.clusters).length, 26)
  assert.equal(map.sessions.flatMap((session) => session.boundaries).length, 17)
  assert.equal(map.sessions.flatMap((session) => session.tails).length, 0)
  assert.equal(Object.values(map.artifacts).flat().length, 18)
  assert.equal(new Set(Object.values(map.artifacts).flat().map((item) => item.key)).size, 18)
})

test('representative opening ask has no literal Markdown-link delimiter while its raw source remains byte-exact', () => {
  const doc = JSON.parse(readFileSync(representativePath, 'utf8'))
  const raw = doc.candidates.opening_asks[0].text
  const before = Buffer.from(raw)
  const ask = workMap(doc).sessions.flatMap((session) => session.openingAsks)[0]
  assert.doesNotMatch(ask.text.text, /\]\(/)
  assert.doesNotMatch(ask.text.fullText, /\]\(/)
  assert.doesNotMatch(ask.text.fullText, /!?\[[^\]]*\]\(/)
  assert.equal(ask.excerpt, true)
  assert.equal(doc.candidates.opening_asks[0].text, raw)
  assert.equal(Buffer.compare(Buffer.from(doc.candidates.opening_asks[0].text), before), 0)
  assert.match(raw, /\]\([^\n]*…$/)
})

test('sessions and every candidate category retain export order', () => {
  const map = workMap(SYNTH)
  assert.deepEqual(map.sessions.map((group) => group.session.id), ['s2', 's1'])
  assert.deepEqual(map.sessions[0].clusters.map((item) => item.file), ['/repo/src/b.js'])
  assert.deepEqual(map.sessions[1].clusters.map((item) => item.file), ['/repo/src/a.js'])
  assert.equal(map.sessions[1].openingAsks[0].text.fullText, 'Inspect this…')
  assert.equal(map.sessions[1].openingAsks[0].excerpt, true)
})

test('receipt links require exact, unique node-anchor equality and an existing receipt', () => {
  const map = workMap(SYNTH)
  const cluster = map.sessions.find((group) => group.session.id === 's1').clusters[0]
  assert.equal(cluster.first.eventId, 'event-1')
  assert.equal(cluster.first.nodeId, 'n1')
  assert.equal(cluster.last.eventId, 'missing')
  assert.equal(cluster.last.nodeId, null)

  const duplicate = new Map([['event', [{ id: 'a' }, { id: 'b' }]]])
  assert.deepEqual(exactAnchorReceipt('event', duplicate, { a: {}, b: {} }), { eventId: 'event', nodeId: null, matchCount: 2 })
})

test('artifact branches use exact full-path equality only', () => {
  const map = workMap(SYNTH)
  const exact = map.artifacts.exists[0]
  assert.deepEqual(exact.exactPath.clusterMatches.map((match) => match.key), ['cluster:0'])
  assert.deepEqual(exact.exactPath.evidenceMatches, [{ nodeId: 'n1', evidenceIndex: 0 }])
  const near = map.artifacts.unresolved[0]
  assert.deepEqual(near.exactPath.evidenceMatches, [], 'basename-only equality must not link')

  const emptyPaths = structuredClone(SYNTH)
  emptyPaths.candidates.clusters.push({ session_id: 's1', file: null, edits: 1, first_event_id: '', last_event_id: '' })
  emptyPaths.candidates.artifacts.push({ file: null, edits: 1, fate: 'unresolved', sessions: ['s1'] })
  const empty = workMap(emptyPaths).artifacts.unresolved.at(-1)
  assert.deepEqual(empty.exactPath, { clusterMatches: [], evidenceMatches: [] }, 'absence is not path equality')
})

test('candidate paths need one exact evidence path and containing cwd before rendering relatively', () => {
  const file = '/Users/example/private/project/src/file.js'
  const exact = { path: file, scope: { cwd: '/Users/example/private/project' } }
  assert.deepEqual(candidateFilePresentation(file, [exact, structuredClone(exact)]), {
    display: 'src/file.js',
    label: 'Project-relative path',
    outside: false,
    relation: 'exact evidence path + cwd',
  }, 'duplicate proof of the same cwd remains one exact relation')

  const ambiguous = candidateFilePresentation(file, [
    exact,
    { path: file, scope: { cwd: '/Users/example/private/project/src' } },
  ])
  assert.equal(ambiguous.label, 'Workspace relation ambiguous')
  assert.equal(ambiguous.relation, 'ambiguous exact containing cwd')
  assert.equal(ambiguous.outside, true)
  assert.doesNotMatch(ambiguous.display, /\/Users\//)

  for (const presentation of [
    candidateFilePresentation(file, []),
    candidateFilePresentation(file, [{ path: '/Users/example/private/project/src/other.js', scope: { cwd: '/Users/example/private/project' } }]),
    candidateFilePresentation(file, [{ path: file, scope: { cwd: '/private/other' } }]),
  ]) {
    assert.equal(presentation.label, 'Workspace relation unproven')
    assert.equal(presentation.outside, true)
    assert.doesNotMatch(presentation.display, /\/Users\//)
  }
})

test('representative candidate paths are synthetic, privacy-safe, and retain exact joins', () => {
  const doc = JSON.parse(readFileSync(representativePath, 'utf8'))
  const map = workMap(doc)
  const clusters = map.sessions.flatMap((session) => session.clusters)
  const tails = map.sessions.flatMap((session) => session.tails)
  const artifacts = Object.values(map.artifacts).flat()
  const presentations = [
    ...clusters.map((item) => item.filePresentation),
    ...tails.flatMap((item) => item.filePresentations),
    ...artifacts.map((item) => item.filePresentation),
  ]
  const rawPaths = [
    ...clusters.map((item) => item.file),
    ...tails.flatMap((item) => item.files),
    ...artifacts.map((item) => item.file),
  ]
  assert.equal(presentations.length, rawPaths.length)
  assert.ok(rawPaths.every((path) => String(path).startsWith('/workspace/thruline-synthetic/')))
  assert.ok(rawPaths.every((path) => !/\/Users\/|\\Users\\/.test(String(path))))
  for (const presentation of presentations) {
    assert.doesNotMatch(presentation.display, /\/Users\//)
    assert.equal(presentation.relation, 'exact evidence path + cwd')
  }
  assert.equal(artifacts.reduce((sum, item) => sum + item.exactPath.clusterMatches.length, 0), 26)
  assert.equal(artifacts.reduce((sum, item) => sum + item.exactPath.evidenceMatches.length, 0), 71)
})

test('unproven candidate interpretations never enter presentation data', () => {
  const map = workMap(SYNTH)
  const serialized = JSON.stringify(map)
  for (const semantic of ['Deploy immediately', 'A final outcome', 'abandoned work']) {
    assert.ok(!serialized.includes(semantic), `unproven interpretation leaked: ${semantic}`)
  }
  for (const cluster of map.sessions.flatMap((group) => group.clusters)) {
    assert.equal(cluster.interpretationLabel, INTERPRETATION_EMPTY)
  }
  assert.equal(map.sessions[0].tails[0].interpretationLabel, INTERPRETATION_EMPTY)
  assert.deepEqual(map.sessions[0].tails[0].anchor, { eventId: '', nodeId: null, matchCount: 0 }, 'a tail without an exported event id stays unlinked')
})

test('unknown artifact fates and missing candidate data fail without omission or invention', () => {
  const map = workMap(SYNTH)
  assert.equal(map.artifacts.other.length, 1)
  assert.equal(map.artifacts.other[0].fate, 'future_value')
  assert.equal(workMap(null).status, 'unavailable')
  assert.equal(workMap({ nodes: [], edges: [], receipts: {} }).status, 'unavailable')
})

test('the primary component keeps opaque event ids inside the receipt detail door', () => {
  const component = readFileSync(fileURLToPath(new URL('../mac/work-map.jsx', import.meta.url)), 'utf8')
  assert.ok(!component.includes('<code>{anchor.eventId}</code>'))
  assert.match(component, /Open exact receipt/)
  assert.match(component, /An exact claim receipt is unavailable/)
  assert.match(component, /The export ends this excerpt here; it does not include additional source bytes\./)
  assert.doesNotMatch(component, /item\.text\.truncated \|\| item\.excerpt/)
  assert.doesNotMatch(component, /\{item\.file\}/)
  assert.doesNotMatch(component, /item\.files\.map/)
  assert.match(component, /FileDisplay presentation=\{item\.filePresentation\}/)
})
