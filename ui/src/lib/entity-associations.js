/* Fail-closed browser boundary for optional operator-attested navigation.
 *
 * The export carries two exact event subjects so the browser can locate their
 * receipts. Those event ids are consumed here and deliberately omitted from
 * the returned presentation rows. An association is navigation only: it never
 * becomes a graph edge, identity merge, or causal claim.
 */

const ROW_KEYS = [
  'candidate_id',
  'decided_at_unix_ms',
  'decision',
  'operator_attested',
  'project_id',
  'subjects',
]
const SUBJECT_KEYS = ['event_id', 'session_id']
const SUBJECT_SPAN_KEYS = [...SUBJECT_KEYS, 'span']
const SPAN_KEYS = ['end', 'start']
const CANDIDATE_ID = /^entity_assoc_[0-9a-f]{64}$/
const INTERNAL_ID = /^[A-Za-z0-9][A-Za-z0-9:_-]{0,159}$/
const ORDINARY_STATES = new Set(['recorded', 'supported', 'pending', 'contradicted', 'refused', 'unknown'])
const MAX_ROWS = 100_000
const MAX_INDEX = 4_294_967_295
const MAX_TIMESTAMP = 8_640_000_000_000_000
export const ENTITY_ASSOCIATION_PAGE = 4

function dataOwn(object, key) {
  if (object === null || (typeof object !== 'object' && typeof object !== 'function')) return undefined
  const descriptor = Object.getOwnPropertyDescriptor(object, key)
  return descriptor && Object.hasOwn(descriptor, 'value') ? descriptor.value : undefined
}

function exactObject(value, keys) {
  if (value === null || typeof value !== 'object') return false
  const prototype = Object.getPrototypeOf(value)
  if (prototype !== Object.prototype && prototype !== null) return false
  const ownKeys = Reflect.ownKeys(value)
  if (ownKeys.some((key) => typeof key !== 'string')) return false
  const sorted = [...ownKeys].sort()
  const expected = [...keys].sort()
  if (sorted.length !== expected.length || sorted.some((key, index) => key !== expected[index])) return false
  return ownKeys.every((key) => {
    const descriptor = Object.getOwnPropertyDescriptor(value, key)
    return descriptor?.enumerable === true && Object.hasOwn(descriptor, 'value')
  })
}

function strictArray(value, maximum = MAX_ROWS) {
  if (!Array.isArray(value) || Object.getPrototypeOf(value) !== Array.prototype || value.length > maximum) return false
  const keys = Reflect.ownKeys(value)
  if (keys.some((key) => typeof key !== 'string')) return false
  if (keys.some((key) => key !== 'length' && !/^(?:0|[1-9][0-9]*)$/.test(key))) return false
  for (let index = 0; index < value.length; index += 1) {
    const descriptor = Object.getOwnPropertyDescriptor(value, String(index))
    if (!descriptor || descriptor.enumerable !== true || !Object.hasOwn(descriptor, 'value')) return false
  }
  return keys.length === value.length + 1
}

function boundedInteger(value, maximum) {
  return Number.isSafeInteger(value) && value >= 0 && value <= maximum
}

function knownSessions(raw) {
  const sessions = dataOwn(raw, 'sessions')
  if (!strictArray(sessions)) return null
  const ids = new Set()
  for (const session of sessions) {
    const id = dataOwn(session, 'id')
    if (typeof id !== 'string' || !INTERNAL_ID.test(id) || ids.has(id)) return null
    ids.add(id)
  }
  return ids
}

function projectId(raw) {
  const project = dataOwn(raw, 'project')
  const id = dataOwn(project, 'id')
  return typeof id === 'string' && id.length > 0 && id.length <= 256 ? id : null
}

function validSpan(span) {
  return exactObject(span, SPAN_KEYS)
    && boundedInteger(span.start, MAX_INDEX)
    && boundedInteger(span.end, MAX_INDEX)
    && span.start < span.end
}

function validSubject(subject, sessions) {
  const keys = Object.hasOwn(subject ?? {}, 'span') ? SUBJECT_SPAN_KEYS : SUBJECT_KEYS
  return exactObject(subject, keys)
    && typeof subject.event_id === 'string' && INTERNAL_ID.test(subject.event_id)
    && typeof subject.session_id === 'string' && sessions.has(subject.session_id)
    && (!Object.hasOwn(subject, 'span') || validSpan(subject.span))
}

function validRow(row, fixtureProject, sessions) {
  if (!exactObject(row, ROW_KEYS)
    || typeof row.candidate_id !== 'string' || !CANDIDATE_ID.test(row.candidate_id)
    || row.project_id !== fixtureProject
    || row.operator_attested !== true
    || row.decision !== 'associate'
    || !boundedInteger(row.decided_at_unix_ms, MAX_TIMESTAMP)
    || !strictArray(row.subjects, 2) || row.subjects.length !== 2
    || !row.subjects.every((subject) => validSubject(subject, sessions))) return false

  const [first, second] = row.subjects
  return first.event_id !== second.event_id && first.event_id < second.event_id
}

function receiptIndex(raw) {
  const bySubject = new Map()
  const idCounts = new Map()
  const value = dataOwn(raw, 'nodes')
  const nodes = strictArray(value) ? value : []
  for (const node of nodes) {
    const nodeId = dataOwn(node, 'id')
    if (typeof nodeId === 'string') idCounts.set(nodeId, (idCounts.get(nodeId) || 0) + 1)
  }
  for (const node of nodes) {
    const sortKey = dataOwn(node, 'sort_key') ?? dataOwn(node, 'state')
    if (!ORDINARY_STATES.has(sortKey)) continue
    const sessionId = dataOwn(node, 'session_id')
    const eventId = dataOwn(node, 'anchor_event_id')
    if (typeof sessionId !== 'string' || typeof eventId !== 'string') continue
    const key = `${sessionId}\u001f${eventId}`
    if (!bySubject.has(key)) bySubject.set(key, [])
    bySubject.get(key).push(node)
  }
  return { bySubject, idCounts }
}

function exactReceipt(raw, index, subject) {
  const matches = index.bySubject.get(`${subject.session_id}\u001f${subject.event_id}`) || []
  if (matches.length !== 1) return { available: false, label: 'Exact receipt unavailable' }
  const nodeId = dataOwn(matches[0], 'id')
  if (typeof nodeId !== 'string' || !INTERNAL_ID.test(nodeId)) {
    return { available: false, label: 'Exact receipt unavailable' }
  }
  if (index.idCounts.get(nodeId) !== 1) return { available: false, label: 'Exact receipt unavailable' }
  const receipt = dataOwn(dataOwn(raw, 'receipts'), nodeId)
  if (receipt === null || typeof receipt !== 'object') {
    return { available: false, label: 'Exact receipt unavailable' }
  }
  return { available: true, nodeId, label: 'Open exact receipt' }
}

function invalidProjection() {
  return {
    status: 'invalid',
    message: 'Entity association export did not match the guarded contract.',
    rows: [],
  }
}

function projectionIdentity(candidateIds) {
  let first = 0x811c9dc5
  let second = 0x9e3779b9
  for (const value of candidateIds) {
    for (let index = 0; index < value.length; index += 1) {
      const code = value.charCodeAt(index)
      first = Math.imul(first ^ code, 0x01000193)
      second = Math.imul(second ^ code, 0x85ebca6b)
    }
    first = Math.imul(first ^ 0xff, 0x01000193)
    second = Math.imul(second ^ 0xff, 0x85ebca6b)
  }
  return `entity-set-${candidateIds.length}-${(first >>> 0).toString(16).padStart(8, '0')}${(second >>> 0).toString(16).padStart(8, '0')}`
}

export function entityAssociationProjection(raw) {
  const descriptor = raw !== null && (typeof raw === 'object' || typeof raw === 'function')
    ? Object.getOwnPropertyDescriptor(raw, 'entity_associations')
    : undefined
  if (!descriptor) return { status: 'absent', rows: [] }
  if (!Object.hasOwn(descriptor, 'value')) return invalidProjection()
  const block = descriptor.value

  const fixtureProject = projectId(raw)
  const sessions = knownSessions(raw)
  if (!fixtureProject || !sessions || !strictArray(block) || block.length === 0) return invalidProjection()

  let previousCandidate = null
  const candidateIds = []
  for (const row of block) {
    if (!validRow(row, fixtureProject, sessions)) return invalidProjection()
    if (previousCandidate !== null && row.candidate_id <= previousCandidate) return invalidProjection()
    previousCandidate = row.candidate_id
    candidateIds.push(row.candidate_id)
  }

  const receiptsBySubject = receiptIndex(raw)
  return {
    status: 'ready',
    identity: projectionIdentity(candidateIds),
    rows: block.map((row, rowIndex) => ({
      key: `entity-association-${rowIndex}`,
      position: rowIndex + 1,
      decisionTime: new Date(row.decided_at_unix_ms).toISOString(),
      receipts: row.subjects.map((subject, subjectIndex) => ({
        position: subjectIndex + 1,
        ...exactReceipt(raw, receiptsBySubject, subject),
      })),
    })),
  }
}

export function entityAssociationWindow(rows, visible = ENTITY_ASSOCIATION_PAGE) {
  if (!Array.isArray(rows)) return { rows: [], total: 0, remaining: 0, expanded: false }
  const count = Number.isSafeInteger(visible) && visible > 0 ? visible : ENTITY_ASSOCIATION_PAGE
  const shown = rows.slice(0, Math.min(rows.length, count))
  return {
    rows: shown,
    total: rows.length,
    remaining: rows.length - shown.length,
    expanded: rows.length > ENTITY_ASSOCIATION_PAGE && shown.length === rows.length,
  }
}
