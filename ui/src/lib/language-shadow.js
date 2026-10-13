/* Fail-closed presentation boundary for the optional at-export language layer.
 *
 * This module never grants model output truth authority. It accepts only the
 * exact exported projection, recomputes its aggregate counts, and emits a
 * session-keyed navigation view that deliberately omits raw event ids.
 */

export const LANGUAGE_PROMPT_REVISION = 'm4.3-language-shadow-v2'
export const LANGUAGE_CATEGORIES = Object.freeze([
  'completed_claim',
  'proposal',
  'finding',
  'micro_intent',
  'guidance',
  'procedure',
  'question',
  'narration',
  'uncertain',
])
export const LANGUAGE_SPEECH_ACTS = Object.freeze([
  'none',
  'proposal',
  'decision',
  'refusal',
  'deferral',
])

const TOP_KEYS = ['coverage', 'items', 'run', 'scope', 'truth_bearing']
const RUN_KEYS = [
  'annotation_id',
  'created_at',
  'extractor',
  'model',
  'model_digest',
  'output_sha256',
  'prompt_revision',
  'prompt_sha256',
]
const COVERAGE_KEYS = [
  'accepted_claims',
  'attempts',
  'candidates',
  'categories',
  'classified',
  'events',
  'labeled',
  'speech_acts',
  'uncertain',
]
const ITEM_KEYS = [
  'annotation_id',
  'attempts',
  'category',
  'event_id',
  'output_sha256',
  'prompt_sha256',
  'seq',
  'session_id',
  'span',
  'speech_act',
]
const ITEM_LABEL_KEYS = [...ITEM_KEYS, 'label']
const SHADOW_STATES = new Set(['unsupported', 'dropped'])
const MAX_ITEMS = 100_000
const MAX_COUNT = 1_000_000
const MAX_INDEX = 2_147_483_647
const HASH = /^[0-9a-f]{64}$/
const ANNOTATION_ID = /^a[0-9a-f]{31}$/
const EXTRACTOR = /^language-v1:[0-9a-f]{12}:t[0-9a-f]{8}$/
const INTERNAL_ID = /^[A-Za-z0-9][A-Za-z0-9:_-]{0,159}$/
const MODEL = /^[A-Za-z0-9](?:[A-Za-z0-9._:-]{0,126}[A-Za-z0-9])?$/
const SECRET = /(?:AKIA|ASIA)[A-Z0-9]{16}|(?:sk|gh[opurs])[-_][A-Za-z0-9_-]{16,}|[A-Za-z0-9_=-]{32,}/i

const TRUTH_TERMS = new Set([
  'abandoned', 'complete', 'completed', 'contradicted', 'created', 'deployed',
  'deleted', 'done', 'dropped', 'entailed', 'false', 'fixed', 'finished',
  'fulfilled', 'green', 'implemented', 'launched', 'merged', 'passed', 'passing',
  'proven', 'published', 'ready', 'recorded', 'refactored', 'released', 'removed',
  'refuted', 'resolved', 'shipped', 'succeeded', 'success', 'successful',
  'supported', 'tested', 'true', 'unsupported', 'unverified', 'validated',
  'verified', 'working', 'works',
])
const COMMAND_TERMS = new Set([
  'bash', 'bun', 'cargo', 'curl', 'deno', 'docker', 'dotnet', 'git', 'go',
  'gradle', 'make', 'mvn', 'node', 'npm', 'npx', 'pnpm', 'python', 'python3',
  'rm', 'rustc', 'sh', 'sudo', 'swift', 'wget', 'yarn', 'zsh',
])

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

function strictArray(value, maximum = MAX_ITEMS) {
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

function boundedInteger(value, maximum = MAX_COUNT) {
  return Number.isSafeInteger(value) && value >= 0 && value <= maximum
}

function canonicalUtc(value) {
  if (typeof value !== 'string') return false
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,9}))?Z$/.exec(value)
  if (!match) return false
  const [, yearText, monthText, dayText, hourText, minuteText, secondText] = match
  const year = Number(yearText)
  const month = Number(monthText)
  const day = Number(dayText)
  const hour = Number(hourText)
  const minute = Number(minuteText)
  const second = Number(secondText)
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0)
  const days = [0, 31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
  return month >= 1 && month <= 12 && day >= 1 && day <= days[month]
    && hour <= 23 && minute <= 59 && second <= 60
}

function safeModel(value) {
  return typeof value === 'string'
    && MODEL.test(value)
    && !value.includes('..')
    && !SECRET.test(value)
}

function safeLabel(value) {
  if (typeof value !== 'string' || value.trim() !== value || value.length === 0) return false
  if (Array.from(value).length > 64 || /[\p{Cc}\p{Cf}]/u.test(value) || SECRET.test(value)) return false
  const words = value.split(/\s+/u)
  if (words.length < 1 || words.length > 4) return false
  for (const word of words) {
    if (!/^[\p{L}\p{N}](?:[\p{L}\p{N}'’\-]*[\p{L}\p{N}])?$/u.test(word)) return false
  }
  const semanticWords = value.toLocaleLowerCase('en-US').split(/[^\p{L}\p{N}]+/u).filter(Boolean)
  return !semanticWords.some((word) => TRUTH_TERMS.has(word) || COMMAND_TERMS.has(word))
}

function exactCountMap(value, domain, expected) {
  if (value === null || typeof value !== 'object') return false
  const keys = Object.keys(expected).sort()
  if (!exactObject(value, keys) || keys.some((key) => !domain.includes(key))) return false
  return keys.every((key) => boundedInteger(value[key]) && value[key] > 0 && value[key] === expected[key])
}

function countBy(items, field) {
  const counts = Object.create(null)
  for (const item of items) counts[item[field]] = (counts[item[field]] || 0) + 1
  return counts
}

function knownSessions(raw) {
  const sessions = dataOwn(raw, 'sessions')
  if (!strictArray(sessions)) return null
  const ids = []
  const seen = new Set()
  for (const session of sessions) {
    const id = dataOwn(session, 'id')
    if (typeof id !== 'string' || !INTERNAL_ID.test(id) || seen.has(id)) return null
    seen.add(id)
    ids.push(id)
  }
  return { ids, set: seen }
}

function validRun(run) {
  return exactObject(run, RUN_KEYS)
    && ANNOTATION_ID.test(run.annotation_id)
    && safeModel(run.model)
    && HASH.test(run.model_digest)
    && run.prompt_revision === LANGUAGE_PROMPT_REVISION
    && EXTRACTOR.test(run.extractor)
    && canonicalUtc(run.created_at)
    && HASH.test(run.prompt_sha256)
    && HASH.test(run.output_sha256)
}

function validItem(item, sessionSet) {
  const keys = Object.hasOwn(item ?? {}, 'label') ? ITEM_LABEL_KEYS : ITEM_KEYS
  if (!exactObject(item, keys)) return false
  if (!ANNOTATION_ID.test(item.annotation_id)
    || typeof item.event_id !== 'string' || !INTERNAL_ID.test(item.event_id)
    || typeof item.session_id !== 'string' || !sessionSet.has(item.session_id)
    || !boundedInteger(item.seq, 1_000_000_000)
    || !strictArray(item.span, 2) || item.span.length !== 2
    || !boundedInteger(item.span[0], MAX_INDEX) || !boundedInteger(item.span[1], MAX_INDEX)
    || item.span[0] >= item.span[1]
    || !LANGUAGE_CATEGORIES.includes(item.category)
    || !LANGUAGE_SPEECH_ACTS.includes(item.speech_act)
    || !boundedInteger(item.attempts, 2)
    || (item.attempts === 0 && item.category !== 'uncertain')
    || !HASH.test(item.prompt_sha256) || !HASH.test(item.output_sha256)) return false
  const hasLabel = Object.hasOwn(item, 'label')
  if (hasLabel && !safeLabel(item.label)) return false
  return item.category !== 'uncertain'
    ? true
    : item.speech_act === 'none' && !hasLabel
}

function ordinaryNodes(raw) {
  const nodes = dataOwn(raw, 'nodes')
  if (!Array.isArray(nodes)) return []
  const diagnostic = dataOwn(raw, 'diagnostics') === true
  return nodes.filter((node) => {
    if (node === null || typeof node !== 'object') return false
    if (diagnostic) return true
    const sortKey = dataOwn(node, 'sort_key') ?? dataOwn(node, 'state')
    return !SHADOW_STATES.has(sortKey)
  })
}

function receiptFor(raw, nodes, item) {
  const matches = nodes.filter((node) => dataOwn(node, 'session_id') === item.session_id
    && dataOwn(node, 'anchor_event_id') === item.event_id)
  if (matches.length !== 1) return { available: false, label: 'Exact receipt unavailable' }
  const nodeId = dataOwn(matches[0], 'id')
  if (typeof nodeId !== 'string' || !INTERNAL_ID.test(nodeId)) {
    return { available: false, label: 'Exact receipt unavailable' }
  }
  const receipts = dataOwn(raw, 'receipts')
  const receipt = dataOwn(receipts, nodeId)
  if (receipt === null || typeof receipt !== 'object') {
    return { available: false, label: 'Exact receipt unavailable' }
  }
  return { available: true, nodeId, label: 'Open exact receipt' }
}

function invalidProjection() {
  return {
    status: 'invalid',
    message: 'Language annotation export did not match the guarded contract.',
    sessions: [],
  }
}

export function languageShadowProjection(raw) {
  const block = dataOwn(raw, 'language_annotations')
  if (block === undefined) return { status: 'absent', sessions: [] }
  const sessions = knownSessions(raw)
  if (!sessions || !exactObject(block, TOP_KEYS) || block.truth_bearing !== false || block.scope !== 'at_export') {
    return invalidProjection()
  }
  if (!validRun(block.run) || !exactObject(block.coverage, COVERAGE_KEYS) || !strictArray(block.items)) {
    return invalidProjection()
  }
  const items = block.items
  if (!items.every((item) => validItem(item, sessions.set))) return invalidProjection()

  const eventSpans = new Set()
  const annotationEvents = new Map()
  const eventBindings = new Map()
  for (const item of items) {
    const eventSpan = `${item.event_id}\u001f${item.span[0]}\u001f${item.span[1]}`
    if (eventSpans.has(eventSpan)) return invalidProjection()
    eventSpans.add(eventSpan)
    if (annotationEvents.has(item.annotation_id) && annotationEvents.get(item.annotation_id) !== item.event_id) return invalidProjection()
    annotationEvents.set(item.annotation_id, item.event_id)
    const binding = `${item.session_id}\u001f${item.seq}\u001f${item.prompt_sha256}\u001f${item.output_sha256}`
    if (eventBindings.has(item.event_id) && eventBindings.get(item.event_id) !== binding) return invalidProjection()
    eventBindings.set(item.event_id, binding)
  }

  const classified = items.filter((item) => item.category !== 'uncertain').length
  const uncertain = items.length - classified
  const labeled = items.filter((item) => Object.hasOwn(item, 'label')).length
  const attempts = items.reduce((sum, item) => sum + item.attempts, 0)
  const uniqueEvents = new Set(items.map((item) => item.event_id)).size
  const coverage = block.coverage
  if (!boundedInteger(coverage.events) || coverage.events < uniqueEvents
    || coverage.candidates !== items.length
    || coverage.classified !== classified
    || coverage.uncertain !== uncertain
    || coverage.labeled !== labeled
    || coverage.attempts !== attempts
    || !boundedInteger(coverage.accepted_claims) || coverage.accepted_claims > classified
    || !exactCountMap(coverage.categories, LANGUAGE_CATEGORIES, countBy(items, 'category'))
    || !exactCountMap(coverage.speech_acts, LANGUAGE_SPEECH_ACTS, countBy(items, 'speech_act'))) {
    return invalidProjection()
  }

  const nodes = ordinaryNodes(raw)
  const rowsBySession = new Map(sessions.ids.map((id) => [id, []]))
  const ordered = [...items].sort((left, right) => sessions.ids.indexOf(left.session_id) - sessions.ids.indexOf(right.session_id)
    || left.seq - right.seq
    || left.span[0] - right.span[0]
    || left.span[1] - right.span[1]
    || left.annotation_id.localeCompare(right.annotation_id))
  for (const item of ordered) {
    const sessionIndex = sessions.ids.indexOf(item.session_id)
    const rows = rowsBySession.get(item.session_id)
    rows.push({
      key: `language-row-${sessionIndex}-${rows.length}`,
      sequence: item.seq,
      span: [...item.span],
      category: item.category,
      speechAct: item.speech_act,
      label: Object.hasOwn(item, 'label') ? item.label : null,
      attempts: item.attempts,
      receipt: receiptFor(raw, nodes, item),
      provenance: {
        annotationId: item.annotation_id,
        promptDigest: item.prompt_sha256,
        outputDigest: item.output_sha256,
      },
    })
  }

  return {
    status: 'ready',
    authority: { truthBearing: false, scope: 'at_export' },
    run: {
      annotationId: block.run.annotation_id,
      model: block.run.model,
      modelDigest: block.run.model_digest,
      promptRevision: block.run.prompt_revision,
      extractor: block.run.extractor,
      createdAt: block.run.created_at,
      promptDigest: block.run.prompt_sha256,
      outputDigest: block.run.output_sha256,
    },
    coverage: {
      events: coverage.events,
      items: items.length,
      classified,
      uncertain,
      labeled,
      acceptedClaims: coverage.accepted_claims,
      attempts,
      categories: { ...coverage.categories },
      speechActs: { ...coverage.speech_acts },
    },
    sessions: sessions.ids.map((sessionId, index) => ({
      key: `language-session-${index}`,
      sessionId,
      rows: rowsBySession.get(sessionId),
    })),
  }
}
