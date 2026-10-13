import assert from 'node:assert/strict'
import { globSync, readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import {
  ENTITY_ASSOCIATION_PAGE,
  entityAssociationProjection,
  entityAssociationWindow,
} from './entity-associations.js'

const candidate = (index) => `entity_assoc_${index.toString(16).padStart(64, '0')}`

function subject(event, session, span) {
  const value = { event_id: event, session_id: session }
  if (span) value.span = span
  return value
}

function row(index, overrides = {}) {
  return {
    candidate_id: candidate(index),
    project_id: 'project:one',
    subjects: [
      subject(`event:${index}:a`, 'session:one'),
      subject(`event:${index}:b`, index % 2 === 0 ? 'session:two' : 'session:one', { start: 1, end: 4 }),
    ],
    operator_attested: true,
    decision: 'associate',
    decided_at_unix_ms: 1_700_000_000_000 + index,
    ...overrides,
  }
}

function fixture(rows) {
  const raw = {
    project: { id: 'project:one', name: 'Fixture' },
    sessions: [{ id: 'session:one' }, { id: 'session:two' }],
    nodes: [],
    receipts: {},
  }
  if (rows !== undefined) raw.entity_associations = rows
  for (const entry of rows || []) {
    for (const [index, entrySubject] of entry.subjects.entries()) {
      const nodeId = `node:${entry.candidate_id.slice(-2)}:${index + 1}`
      raw.nodes.push({
        id: nodeId,
        session_id: entrySubject.session_id,
        anchor_event_id: entrySubject.event_id,
        sort_key: 'recorded',
      })
      raw.receipts[nodeId] = { claim: { text: 'kept outside projection' } }
    }
  }
  return raw
}

function clone(value) {
  return JSON.parse(JSON.stringify(value))
}

test('absent is quiet while a present empty block fails its noncanonical contract closed', () => {
  assert.deepEqual(entityAssociationProjection(fixture()), { status: 'absent', rows: [] })
  assert.equal(entityAssociationProjection(fixture([])).status, 'invalid')

  const inherited = fixture()
  Object.setPrototypeOf(inherited, { entity_associations: [row(1)] })
  assert.deepEqual(entityAssociationProjection(inherited), { status: 'absent', rows: [] })

  const accessor = fixture()
  Object.defineProperty(accessor, 'entity_associations', { get: () => [row(1)], enumerable: true })
  assert.equal(entityAssociationProjection(accessor).status, 'invalid')
})

test('one accepted row becomes a two-receipt navigation bridge without raw event ids', () => {
  const raw = fixture([row(1)])
  const projection = entityAssociationProjection(raw)
  assert.equal(projection.status, 'ready')
  assert.equal(projection.rows.length, 1)
  assert.deepEqual(projection.rows[0].receipts.map((receipt) => ({
    position: receipt.position,
    available: receipt.available,
    nodeId: receipt.nodeId,
  })), [
    { position: 1, available: true, nodeId: 'node:01:1' },
    { position: 2, available: true, nodeId: 'node:01:2' },
  ])
  assert.match(projection.rows[0].decisionTime, /^2023-11-14T22:13:20\.001Z$/)
  assert.match(projection.identity, /^entity-set-1-[0-9a-f]{16}$/)
  assert.doesNotMatch(JSON.stringify(projection), /event:|event_id|project:one|entity_assoc_|proposal|operator|model|path|confidence/)
})

test('five rows retain canonical candidate order across two sessions and window four then all', () => {
  const rows = [1, 2, 3, 4, 5].map((index) => row(index))
  const projection = entityAssociationProjection(fixture(rows))
  assert.equal(projection.status, 'ready')
  assert.deepEqual(projection.rows.map((entry) => entry.key), [
    'entity-association-0',
    'entity-association-1',
    'entity-association-2',
    'entity-association-3',
    'entity-association-4',
  ])
  assert.deepEqual(projection.rows.map((entry) => entry.position), [1, 2, 3, 4, 5])
  const initial = entityAssociationWindow(projection.rows)
  assert.equal(ENTITY_ASSOCIATION_PAGE, 4)
  assert.deepEqual(initial.rows.map((entry) => entry.key), projection.rows.slice(0, 4).map((entry) => entry.key))
  assert.deepEqual({ total: initial.total, remaining: initial.remaining, expanded: initial.expanded }, {
    total: 5, remaining: 1, expanded: false,
  })
  const expanded = entityAssociationWindow(projection.rows, projection.rows.length)
  assert.deepEqual(expanded.rows, projection.rows)
  assert.equal(expanded.expanded, true)
  assert.deepEqual(entityAssociationWindow(projection.rows, ENTITY_ASSOCIATION_PAGE).rows, projection.rows.slice(0, 4))
})

test('whole block and nested values reject accessors, prototypes, sparse arrays, and unknown fields', () => {
  const mutations = [
    (raw) => { raw.entity_associations.extra = true },
    (raw) => { raw.entity_associations[0].unknown = true },
    (raw) => { raw.entity_associations[0].subjects[0].confidence = 0.8 },
    (raw) => { raw.entity_associations[0].subjects[0].path = '/private/source' },
    (raw) => { raw.entity_associations[0].subjects[1].span.extra = 1 },
    (raw) => { Object.setPrototypeOf(raw.entity_associations[0], { inherited: true }) },
    (raw) => { Object.defineProperty(raw.entity_associations[0], 'candidate_id', { get: () => candidate(1), enumerable: true }) },
    (raw) => { Object.defineProperty(raw.entity_associations[0], '__proto__', { value: 'x', enumerable: true }) },
    (raw) => { Object.defineProperty(raw.entity_associations[0], Symbol('private'), { value: true, enumerable: true }) },
    (raw) => { Object.setPrototypeOf(raw.entity_associations, Object.create(Array.prototype)) },
    (raw) => { delete raw.entity_associations[0].subjects[1] },
  ]
  for (const mutate of mutations) {
    const raw = fixture([row(1)])
    mutate(raw)
    assert.equal(entityAssociationProjection(raw).status, 'invalid')
  }
})

test('identity, project, decision, timestamp, subject, span, session, and order failures reject the whole block', () => {
  const mutations = [
    (raw) => { raw.entity_associations[0].candidate_id = `entity_assoc_${'A'.repeat(64)}` },
    (raw) => {
      raw.entity_associations[0].candidate_id = { toString() { throw new Error('must not execute') } }
    },
    (raw) => { raw.entity_associations[0].project_id = 'project:other' },
    (raw) => { raw.entity_associations[0].operator_attested = false },
    (raw) => { raw.entity_associations[0].decision = 'reject' },
    (raw) => { raw.entity_associations[0].decided_at_unix_ms = -1 },
    (raw) => { raw.entity_associations[0].subjects[1].event_id = raw.entity_associations[0].subjects[0].event_id },
    (raw) => { raw.entity_associations[0].subjects.reverse() },
    (raw) => { raw.entity_associations[0].subjects[0].session_id = 'session:missing' },
    (raw) => { raw.entity_associations[0].subjects[1].span.start = 4 },
    (raw) => { raw.entity_associations[0].subjects[1].span.end = 4_294_967_296 },
    (raw) => { raw.sessions.push({ id: 'session:one' }) },
  ]
  for (const mutate of mutations) {
    const raw = fixture([row(1)])
    mutate(raw)
    assert.equal(entityAssociationProjection(raw).status, 'invalid')
  }

  const reversed = fixture([row(2), row(1)])
  assert.equal(entityAssociationProjection(reversed).status, 'invalid')
  const duplicate = fixture([row(1), clone(row(1))])
  assert.equal(entityAssociationProjection(duplicate).status, 'invalid')

  const maximumU32Span = fixture([row(1)])
  maximumU32Span.entity_associations[0].subjects[1].span = { start: 4_294_967_294, end: 4_294_967_295 }
  assert.equal(entityAssociationProjection(maximumU32Span).status, 'ready')
})

test('receipt joins require one ordinary same-session own node and own receipt', () => {
  const entry = row(1)

  const ambiguous = fixture([entry])
  ambiguous.nodes.push({ ...ambiguous.nodes[0], id: 'node:duplicate' })
  ambiguous.receipts['node:duplicate'] = { claim: {} }
  let projection = entityAssociationProjection(ambiguous)
  assert.equal(projection.status, 'ready')
  assert.equal(projection.rows[0].receipts[0].available, false)

  const missing = fixture([entry])
  delete missing.receipts['node:01:1']
  projection = entityAssociationProjection(missing)
  assert.equal(projection.rows[0].receipts[0].available, false)

  const inherited = fixture([entry])
  delete inherited.receipts['node:01:1']
  Object.setPrototypeOf(inherited.receipts, { 'node:01:1': { claim: {} } })
  projection = entityAssociationProjection(inherited)
  assert.equal(projection.rows[0].receipts[0].available, false)

  const shadow = fixture([entry])
  shadow.nodes[0].sort_key = 'unsupported'
  projection = entityAssociationProjection(shadow)
  assert.equal(projection.rows[0].receipts[0].available, false)

  const wrongSession = fixture([entry])
  wrongSession.nodes[0].session_id = 'session:two'
  projection = entityAssociationProjection(wrongSession)
  assert.equal(projection.rows[0].receipts[0].available, false)

  const inheritedNode = fixture([entry])
  inheritedNode.nodes[0] = Object.create({
    id: 'node:01:1',
    session_id: 'session:one',
    anchor_event_id: 'event:1:a',
    sort_key: 'recorded',
  })
  projection = entityAssociationProjection(inheritedNode)
  assert.equal(projection.rows[0].receipts[0].available, false)

  const duplicateIdElsewhere = fixture([entry])
  duplicateIdElsewhere.nodes.push({
    id: 'node:01:1',
    session_id: 'session:two',
    anchor_event_id: 'event:unrelated',
    sort_key: 'recorded',
  })
  projection = entityAssociationProjection(duplicateIdElsewhere)
  assert.equal(projection.rows[0].receipts[0].available, false)
})

test('project bridge source is in-flow, progressive, accessible, privacy-bounded, and ordered before artifacts', () => {
  const component = readFileSync(fileURLToPath(new URL('../mac/work-map.jsx', import.meta.url)), 'utf8')
  const css = readFileSync(fileURLToPath(new URL('../macos.css', import.meta.url)), 'utf8')
  const packageJson = JSON.parse(readFileSync(fileURLToPath(new URL('../../package.json', import.meta.url)), 'utf8'))
  const stageStart = component.indexOf('function EntityAssociationStage')
  const stageEnd = component.indexOf('function RecordedDetail', stageStart)
  const stage = component.slice(stageStart, stageEnd)
  const sessions = component.indexOf('{map.sessions.map')
  const sessionsClose = component.indexOf('        ))}', sessions)
  const renderedStage = component.indexOf('<EntityAssociationStage', sessions)
  const artifacts = component.indexOf('<section className="wm-outputs"', renderedStage)
  const associationCss = css.match(/(?:^|\n)\.wm-associations[\s\S]*?(?=\n\.wm-item \{)/)?.[0] || ''

  assert.ok(stageStart > 0 && stageEnd > stageStart)
  assert.ok(sessions > 0 && sessionsClose > sessions && renderedStage > sessionsClose && artifacts > renderedStage)
  assert.equal((component.match(/<EntityAssociationStage/g) || []).length, 1)
  assert.match(component, /entityAssociationProjection\(fixture\)/)
  assert.match(component, /key=\{`entity-associations:\$\{fixture\.project\?\.id/)
  assert.match(stage, /if \(projection\.status === 'absent'\) return null/)
  assert.match(stage, /The optional association block did not match the guarded export contract/)
  assert.match(stage, /Operator-attested association/)
  assert.match(stage, /Navigation only · not identity, merge, or causality/)
  assert.match(stage, /Operator-attested/)
  assert.match(stage, /Show next \{Math\.min\(ENTITY_ASSOCIATION_PAGE, window\.remaining\)\} associations/)
  assert.match(stage, /Show fewer associations/)
  assert.match(stage, /event\.currentTarget\.focus\(\)/)
  assert.match(stage, /aria-expanded=\{false\}/)
  assert.match(stage, /aria-expanded=\{true\}/)
  assert.match(stage, /aria-controls=\{regionId\}/)
  assert.match(component, /aria-label=\{`Association \$\{associationPosition\} of \$\{associationTotal\} · \$\{position\} · Open exact receipt`\}/)
  assert.match(component, /onClick=\{\(\) => onSelect\(receipt\.nodeId\)\}/)
  assert.match(component, /Exact receipt unavailable/)
  assert.match(component, /<details className="wm-association-provenance">/)
  assert.match(component, /<dd>\{row\.decisionTime\}<\/dd>/)
  assert.doesNotMatch(stage, /candidate_id|event_id|raw prompt|confidence|path|proposal label|model output/i)
  assert.match(associationCss, /\.wm-association-route \{[^}]*display: grid/)
  assert.match(associationCss, /\.wm-association-connector > span \{[^}]*border-top: 1px solid currentColor/)
  assert.doesNotMatch(associationCss, /position:\s*(?:fixed|absolute)|white-space:\s*nowrap/)
  assert.match(css, /@media \(max-width: 600px\)[\s\S]*\.wm-association-route \{ grid-template-columns: 1fr; \}/)
  assert.match(css, /\.wm-association-receipt \{[^}]*white-space: normal/)
  /* R-2026-09-23-14: the `test` script names a glob, not 77 paths. The
   * guard's intent is unchanged — this file must actually be run by
   * `npm test` — so it now expands the script's own pattern and looks for
   * itself in the result, which also fails if the pattern is narrowed. */
  const pattern = packageJson.scripts.test.match(/"([^"]+)"/)[1]
  const selected = globSync(pattern, { cwd: fileURLToPath(new URL('../../', import.meta.url)) })
  assert.ok(
    selected.some((entry) => entry.replaceAll('\\\\', '/') === 'src/lib/entity-associations.test.js'),
    'the test-script glob no longer selects this file',
  )

  const boundary = readFileSync(fileURLToPath(new URL('./entity-associations.js', import.meta.url)), 'utf8')
  assert.match(boundary, /function receiptIndex\(raw\)/)
  assert.match(boundary, /const bySubject = new Map\(\)/)
  assert.doesNotMatch(boundary.slice(boundary.indexOf('function exactReceipt'), boundary.indexOf('function invalidProjection')), /\.filter\(/)
})
