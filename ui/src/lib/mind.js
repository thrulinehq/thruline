/* Guarded client-side boundary for the local, shadow-only interpretation API.
 *
 * This module accepts no caller-authored prompt or provider configuration. It
 * validates the complete server response before returning a single display
 * row; malformed or mismatched responses become one bounded failure code. */

export const MIND_MODEL = 'llama3.2:latest'
export const MIND_MODEL_DIGEST = 'a80c4f17acd55265feec403c7aef86be0c25983ab279d83f3bcd3abbcb5b8b72'
export const MIND_LOOPBACK = '127.0.0.1:11434'
export const MIND_CONTRACT_VERSION = 'grounded-mind-v2'
export const MIND_PROMPT_VERSION = 'grounded-at-export-v2'
export const MIND_SCHEMA_VERSION = 'grounded-drafts-v1'

export const MIND_ENDPOINTS = Object.freeze({
  status: '/api/mind/status',
  interpret: '/api/mind/interpret',
})

export const RELATION_LABELS = Object.freeze({
  recorded: 'Recorded',
  exact_match: 'Exact match',
  model_proposed: 'Model-proposed',
})

export const FINDING_CATEGORIES = Object.freeze([
  'happened',
  'missing',
  'lagging',
  'contradictory',
])

export const SAFE_ACTIONS = Object.freeze([
  'inspect_receipt',
  'inspect_file',
  'review_unfinished',
  'verify_result',
  'clarify_decision',
  'continue_recording',
])

const FACT_KINDS = Object.freeze([
  'opening_ask',
  'work_cluster',
  'checkpoint',
  'uncommitted_tail',
  'artifact',
])
const ABSTENTION_REASONS = new Set([
  'record_too_thin_at_export',
  'no_grounded_rows_at_export',
  'invalid_structured_draft',
])
const DIGEST = /^[0-9a-f]{64}$/
const FACT_ID = /^fact_[0-9a-f]{16}$/
const RESULT_ID = /^mind_[0-9a-f]{20}$/
const ROW_ID = /^row_[0-9a-f]{20}$/
const RECEIPT_ID = /^receipt_[0-9a-f]{16}$/
const PROJECT_ID = /^[0-9a-f]{16}$/
const SESSION_ID = /^[A-Za-z0-9_:-]{1,128}$/
const ABSOLUTE_PATH = /(?:^|[\s("'`=:\[])(?:\/(?!\/)[^\s"'`<>|]+|[A-Za-z]:[\\/][^\s"'`<>|]+)/m
const COMMAND_OR_RESULT = /(?:^|\n)\s*(?:bash|zsh|sh|fish|powershell|cmd|command|git|cargo|npm|pnpm|yarn|node|python\d*)\b|^[>$]\s|\b(?:command|stdout|stderr|result|output)\s*:|(?:→|⇒|=>|->)\s*output|\s(?:&&|\|\||\|)\s/im
const LIVE_WORD = /(?:^|[^a-z0-9])(?:now|current|currently|today|live|latest)(?=$|[^a-z0-9])/i
const SECRET_PATTERNS = [
  /-----BEGIN [A-Z ]*PRIVATE KEY-----/,
  /\b(?:sk|ghp|glpat|github_pat)_[A-Za-z0-9_-]{12,}\b/,
  /\bAKIA[A-Z0-9]{16}\b/,
  /\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b/,
  /\b(?:api[_ -]?key|access[_ -]?token|token|secret|password)\s*[:=]\s*\S+/i,
]

const isObject = (value) => value !== null && typeof value === 'object' && !Array.isArray(value)
const hasExactKeys = (value, keys) => {
  if (!isObject(value)) return false
  const actual = Object.keys(value).sort()
  const wanted = [...keys].sort()
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index])
}
const integer = (value, max = Number.MAX_SAFE_INTEGER) => Number.isSafeInteger(value) && value >= 0 && value <= max
const scalarLength = (value) => [...value].length
const uniqueStrings = (values) => new Set(values).size === values.length

function canonicalTimestamp(value) {
  if (typeof value !== 'string' || value.length > 30) return false
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,9}))?Z$/.exec(value)
  if (!match) return false
  const [, yearText, monthText, dayText, hourText, minuteText, secondText] = match
  const year = Number(yearText)
  const month = Number(monthText)
  const day = Number(dayText)
  const hour = Number(hourText)
  const minute = Number(minuteText)
  const second = Number(secondText)
  if (month < 1 || month > 12 || hour > 23 || minute > 59 || second > 60) return false
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0)
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
  return day >= 1 && day <= days[month - 1]
}

function containsSecret(value) {
  return SECRET_PATTERNS.some((pattern) => pattern.test(value))
}

function safeFactText(value) {
  return typeof value === 'string'
    && value.trim().length > 0
    && scalarLength(value) <= 500
    && !ABSOLUTE_PATH.test(value)
    && !COMMAND_OR_RESULT.test(value)
    && !containsSecret(value)
}

function safeModelText(value) {
  return safeFactText(value)
    && !COMMAND_OR_RESULT.test(value)
    && !LIVE_WORD.test(value)
    && !/\bfact_[0-9a-f]+\b/i.test(value)
}

function fail(code) {
  return { ok: false, code }
}

const AVAILABILITY = Object.freeze({
  idle: Object.freeze({
    canAnalyze: false,
    checking: false,
    showRecovery: false,
    message: 'Local model availability has not been checked.',
  }),
  checking: Object.freeze({
    canAnalyze: false,
    checking: true,
    showRecovery: false,
    message: 'Checking for the fixed local model…',
  }),
  available: Object.freeze({
    canAnalyze: true,
    checking: false,
    showRecovery: false,
    message: 'The fixed local model is available for an explicit run.',
  }),
  unavailable: Object.freeze({
    canAnalyze: false,
    checking: false,
    showRecovery: true,
    message: 'The fixed local model is not available with the required digest.',
  }),
  status_failure: Object.freeze({
    canAnalyze: false,
    checking: false,
    showRecovery: true,
    message: 'The local model status could not be checked.',
  }),
  source_digest_failure: Object.freeze({
    canAnalyze: false,
    checking: false,
    showRecovery: false,
    message: 'The exact export digest could not be computed, so local interpretation is unavailable.',
  }),
})

/** Closed presentation states for the optional local-analysis readiness gate. */
export function mindAvailabilityPresentation(status) {
  if (!isObject(status) || typeof status.state !== 'string') return null
  if (status.state === 'failure') {
    const view = AVAILABILITY[`${status.reason}_failure`]
    return view ? { ...view, state: 'failure' } : null
  }
  const view = AVAILABILITY[status.state]
  if (!view) return null
  return {
    ...view,
    state: status.state,
    showRecovery: view.showRecovery || (status.state === 'checking' && status.recovery === true),
  }
}

/** One status request may own a project epoch; a newer epoch may supersede it. */
export function mindCheckMayStart(ready, activeEpoch, currentEpoch) {
  return ready === true && Number.isSafeInteger(currentEpoch) && activeEpoch !== currentEpoch
}

/** A response may commit only while its request still owns the current epoch. */
export function mindCheckIsCurrent(activeEpoch, currentEpoch, requestEpoch) {
  return Number.isSafeInteger(requestEpoch)
    && activeEpoch === requestEpoch
    && currentEpoch === requestEpoch
}

/** The only request body the browser is allowed to author. */
export function interpretationBody(project, session) {
  if (!PROJECT_ID.test(project) || !SESSION_ID.test(session) || session.includes('.')) return null
  return { project, session, model: MIND_MODEL }
}

export function findingGroupId(session, category) {
  if (!SESSION_ID.test(session) || session.includes('.') || !FINDING_CATEGORIES.includes(category)) return null
  return `wm-finding-${session}-${category}`
}

async function sha256Bytes(bytes) {
  const digest = await globalThis.crypto.subtle.digest('SHA-256', bytes)
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('')
}

/** SHA-256 of the exact fixture response bytes from the existing single fetch. */
export async function fixtureSourceDigest(sourceText) {
  return sha256Bytes(new TextEncoder().encode(String(sourceText)))
}

/** Order-independent grounded-mind JSON framing shared with the Rust core. */
export function canonicalMindBytes(value) {
  const output = []
  const textEncoder = new TextEncoder()
  const append = (bytes) => {
    for (const byte of bytes) output.push(byte)
  }
  const ascii = (text) => {
    for (const character of String(text)) {
      const byte = character.charCodeAt(0)
      if (byte > 0x7f) throw new TypeError('canonical ASCII framing')
      output.push(byte)
    }
  }
  const framed = (tag, bytes) => {
    ascii(tag)
    ascii(bytes.length)
    ascii(':')
    append(bytes)
    ascii(';')
  }
  const encode = (item) => {
    if (item === null) {
      framed('N', new Uint8Array())
    } else if (typeof item === 'boolean') {
      framed('B', textEncoder.encode(item ? '1' : '0'))
    } else if (typeof item === 'number') {
      if (!Number.isSafeInteger(item) || item < 0) throw new TypeError('canonical nonnegative safe integer')
      framed('I', textEncoder.encode(String(item)))
    } else if (typeof item === 'string') {
      framed('S', textEncoder.encode(item))
    } else if (Array.isArray(item)) {
      ascii('A')
      ascii(item.length)
      ascii(':[')
      for (const child of item) encode(child)
      ascii(']')
    } else if (isObject(item)) {
      const keys = Object.keys(item)
      if (keys.some((key) => !/^[\x00-\x7f]*$/.test(key))) throw new TypeError('canonical ASCII object key')
      keys.sort()
      ascii('O')
      ascii(keys.length)
      ascii(':{')
      for (const key of keys) {
        framed('K', textEncoder.encode(key))
        encode(item[key])
      }
      ascii('}')
    } else {
      throw new TypeError('unsupported canonical JSON value')
    }
  }
  encode(value)
  return new Uint8Array(output)
}

export async function canonicalMindDigest(value) {
  return sha256Bytes(canonicalMindBytes(value))
}

export function deriveMindStatus(value) {
  const keys = [
    'available', 'model', 'model_digest', 'provider', 'contract_version',
    'prompt_version', 'schema_version', 'tier', 'mode', 'truth_bearing',
  ]
  if (!hasExactKeys(value, keys)) return fail('invalid_status_response')
  if (
    typeof value.available !== 'boolean'
    || value.model !== MIND_MODEL
    || value.provider !== 'ollama'
    || value.contract_version !== MIND_CONTRACT_VERSION
    || value.prompt_version !== MIND_PROMPT_VERSION
    || value.schema_version !== MIND_SCHEMA_VERSION
    || value.tier !== 'language'
    || value.mode !== 'shadow'
    || value.truth_bearing !== false
  ) return fail('invalid_status_response')
  if (value.available && value.model_digest !== MIND_MODEL_DIGEST) return fail('invalid_status_response')
  if (!value.available && value.model_digest !== null) return fail('invalid_status_response')
  return {
    ok: true,
    state: value.available ? 'available' : 'unavailable',
    model: value.model,
    modelDigest: value.model_digest,
  }
}

function validateCoverage(value, expected) {
  const keys = ['opening_asks', 'work_clusters', 'checkpoints', 'uncommitted_tails', 'artifacts', 'total', 'included', 'omitted']
  if (!hasExactKeys(value, keys) || !keys.every((key) => integer(value[key]))) return false
  const total = value.opening_asks + value.work_clusters + value.checkpoints + value.uncommitted_tails + value.artifacts
  if (value.total !== total || value.included > 64 || value.omitted !== value.total - value.included) return false
  if (!expected || !hasExactKeys(expected, ['opening_asks', 'work_clusters', 'checkpoints', 'uncommitted_tails', 'artifacts'])) return false
  return ['opening_asks', 'work_clusters', 'checkpoints', 'uncommitted_tails', 'artifacts']
    .every((key) => expected[key] === value[key])
}

function validateFacts(facts, coverage) {
  if (!Array.isArray(facts) || facts.length !== coverage.included || facts.length > 64) return null
  const ids = new Set()
  const kindCounts = Object.fromEntries(FACT_KINDS.map((kind) => [kind, 0]))
  let lastKind = -1
  for (const [index, fact] of facts.entries()) {
    if (!hasExactKeys(fact, ['id', 'kind', 'recorded_order', 'text'])) return null
    const kindIndex = FACT_KINDS.indexOf(fact.kind)
    if (
      !FACT_ID.test(fact.id)
      || ids.has(fact.id)
      || fact.recorded_order !== index
      || kindIndex < lastKind
      || !safeFactText(fact.text)
    ) return null
    ids.add(fact.id)
    lastKind = kindIndex
    kindCounts[fact.kind] += 1
  }
  if (
    kindCounts.opening_ask > coverage.opening_asks
    || kindCounts.work_cluster > coverage.work_clusters
    || kindCounts.checkpoint > coverage.checkpoints
    || kindCounts.uncommitted_tail > coverage.uncommitted_tails
    || kindCounts.artifact > coverage.artifacts
  ) return null
  return new Map(facts.map((fact) => [fact.id, fact]))
}

function knownReceiptNodes(fixture, session) {
  if (!isObject(fixture?.receipts)) return null
  const nodes = new Set()
  for (const [node, receipt] of Object.entries(fixture.receipts)) {
    if (receipt?.claim?.session === session && node.length <= 256) nodes.add(node)
  }
  return nodes
}

function validateReferences(row, facts, nodes) {
  if (!Array.isArray(row.fact_ids) || row.fact_ids.length < 1 || row.fact_ids.length > 8) return null
  if (!row.fact_ids.every((id) => typeof id === 'string' && facts.has(id)) || !uniqueStrings(row.fact_ids)) return null
  if (!Array.isArray(row.receipts) || row.receipts.length > 64) return null
  const handles = new Set()
  for (const receipt of row.receipts) {
    if (
      !hasExactKeys(receipt, ['handle', 'node'])
      || !RECEIPT_ID.test(receipt.handle)
      || handles.has(receipt.handle)
      || typeof receipt.node !== 'string'
      || !nodes.has(receipt.node)
    ) return null
    handles.add(receipt.handle)
  }
  return {
    facts: row.fact_ids.map((id) => facts.get(id)),
    receipts: row.receipts.map((receipt) => ({ ...receipt })),
  }
}

function validateFinding(row, facts, nodes, rowIds) {
  if (!hasExactKeys(row, ['id', 'category', 'text', 'fact_ids', 'receipts'])) return null
  if (!ROW_ID.test(row.id) || rowIds.has(row.id) || !FINDING_CATEGORIES.includes(row.category) || !safeModelText(row.text)) return null
  const resolved = validateReferences(row, facts, nodes)
  if (!resolved) return null
  rowIds.add(row.id)
  return { ...row, ...resolved }
}

function validateDecision(row, facts, nodes, rowIds) {
  if (!hasExactKeys(row, ['id', 'text', 'fact_ids', 'receipts'])) return null
  if (!ROW_ID.test(row.id) || rowIds.has(row.id) || !safeModelText(row.text)) return null
  const resolved = validateReferences(row, facts, nodes)
  if (!resolved) return null
  rowIds.add(row.id)
  return { ...row, ...resolved }
}

function validateNextStep(row, facts, nodes, rowIds) {
  if (!hasExactKeys(row, ['id', 'action', 'text', 'fact_ids', 'receipts'])) return null
  if (!ROW_ID.test(row.id) || rowIds.has(row.id) || !SAFE_ACTIONS.includes(row.action) || !safeModelText(row.text)) return null
  const resolved = validateReferences(row, facts, nodes)
  if (!resolved) return null
  rowIds.add(row.id)
  return { ...row, ...resolved }
}

function validateJob(job, expectedJob, facts, nodes, rowIds) {
  const keys = ['result_id', 'job', 'scope', 'prompt_digest', 'schema_digest', 'draft_digest', 'rejected_rows', 'result']
  if (!hasExactKeys(job, keys)) return null
  if (
    !RESULT_ID.test(job.result_id)
    || job.job !== expectedJob
    || job.scope !== 'at_export'
    || !DIGEST.test(job.prompt_digest)
    || !DIGEST.test(job.schema_digest)
    || !DIGEST.test(job.draft_digest)
    || !integer(job.rejected_rows, 16)
    || !isObject(job.result)
  ) return null

  if (job.result.status === 'abstained') {
    if (!hasExactKeys(job.result, ['status', 'reason']) || !ABSTENTION_REASONS.has(job.result.reason)) return null
    return { rejectedRows: job.rejected_rows, abstained: true, reason: job.result.reason, rows: [] }
  }
  if (expectedJob === 'session_narration' && job.result.status === 'findings') {
    if (!hasExactKeys(job.result, ['status', 'findings']) || !Array.isArray(job.result.findings) || job.result.findings.length < 1 || job.result.findings.length > 8) return null
    const rows = job.result.findings.map((row) => validateFinding(row, facts, nodes, rowIds))
    if (rows.some((row) => !row)) return null
    return { rejectedRows: job.rejected_rows, abstained: false, reason: '', rows }
  }
  if (expectedJob === 'decision_proposal' && job.result.status === 'proposals') {
    if (!hasExactKeys(job.result, ['status', 'decisions', 'next_steps'])) return null
    if (!Array.isArray(job.result.decisions) || !Array.isArray(job.result.next_steps)) return null
    if (job.result.decisions.length > 8 || job.result.next_steps.length > 8 || job.result.decisions.length + job.result.next_steps.length < 1) return null
    const decisions = job.result.decisions.map((row) => validateDecision(row, facts, nodes, rowIds))
    const nextSteps = job.result.next_steps.map((row) => validateNextStep(row, facts, nodes, rowIds))
    if (decisions.some((row) => !row) || nextSteps.some((row) => !row)) return null
    return { rejectedRows: job.rejected_rows, abstained: false, reason: '', decisions, nextSteps }
  }
  return null
}

/** Validate the entire response, then derive the display-only shadow branch. */
export async function deriveMindResponse(value, expected) {
  if (!hasExactKeys(value, ['cached', 'analysis']) || typeof value.cached !== 'boolean') return fail('invalid_interpretation_response')
  const analysis = value.analysis
  const keys = [
    'contract_version', 'prompt_version', 'schema_version', 'tier', 'mode', 'truth_bearing',
    'provider', 'model', 'model_digest', 'project', 'session', 'generated_at', 'source_digest',
    'coverage', 'facts', 'jobs', 'output_digest',
  ]
  if (!hasExactKeys(analysis, keys) || !expected) return fail('invalid_interpretation_response')
  if (
    analysis.contract_version !== MIND_CONTRACT_VERSION
    || analysis.prompt_version !== MIND_PROMPT_VERSION
    || analysis.schema_version !== MIND_SCHEMA_VERSION
    || analysis.tier !== 'language'
    || analysis.mode !== 'shadow'
    || analysis.truth_bearing !== false
    || analysis.provider !== 'ollama'
    || analysis.model !== MIND_MODEL
    || analysis.model_digest !== MIND_MODEL_DIGEST
    || analysis.project !== expected.project
    || analysis.session !== expected.session
    || analysis.generated_at !== expected.generatedAt
    || analysis.source_digest !== expected.sourceDigest
    || !canonicalTimestamp(analysis.generated_at)
    || !DIGEST.test(analysis.source_digest)
    || !DIGEST.test(analysis.output_digest)
    || !validateCoverage(analysis.coverage, expected.coverage)
  ) return fail('invalid_interpretation_response')

  try {
    const unsigned = { ...analysis, output_digest: '' }
    if (await canonicalMindDigest(unsigned) !== analysis.output_digest) {
      return fail('invalid_interpretation_response')
    }
  } catch (_error) {
    return fail('invalid_interpretation_response')
  }

  const facts = validateFacts(analysis.facts, analysis.coverage)
  const nodes = knownReceiptNodes(expected.fixture, expected.session)
  if (!facts || !nodes || !Array.isArray(analysis.jobs) || analysis.jobs.length !== 2) return fail('invalid_interpretation_response')
  const rowIds = new Set()
  const narration = validateJob(analysis.jobs[0], 'session_narration', facts, nodes, rowIds)
  const proposal = validateJob(analysis.jobs[1], 'decision_proposal', facts, nodes, rowIds)
  if (!narration || !proposal) return fail('invalid_interpretation_response')

  const categories = Object.fromEntries(FINDING_CATEGORIES.map((category) => [category, []]))
  for (const row of narration.rows) categories[row.category].push(row)
  return {
    ok: true,
    cached: value.cached,
    categories,
    decisions: proposal.decisions || [],
    nextSteps: proposal.nextSteps || [],
    narration: { abstained: narration.abstained, reason: narration.reason, rejectedRows: narration.rejectedRows },
    proposal: { abstained: proposal.abstained, reason: proposal.reason, rejectedRows: proposal.rejectedRows },
    facts: analysis.facts.map((fact) => ({ ...fact })),
    coverage: { ...analysis.coverage },
    provenance: {
      generatedAt: analysis.generated_at,
      sourceDigest: analysis.source_digest,
      outputDigest: analysis.output_digest,
      model: analysis.model,
      modelDigest: analysis.model_digest,
      contractVersion: analysis.contract_version,
      promptVersion: analysis.prompt_version,
      schemaVersion: analysis.schema_version,
    },
  }
}

export function interpretationHttpFailure(status) {
  if (status === 409) return { state: 'conflict', message: 'Another uncached local analysis is already in progress. Retry this run when it finishes.' }
  if (status === 503) return { state: 'unavailable', message: 'The fixed local model is absent, unreachable, or does not match the required digest.' }
  if ([400, 413, 422].includes(status)) return { state: 'invalid_request', message: 'The local server refused this project, run, or request shape.' }
  return { state: 'failure', message: 'The local interpretation request failed without a validated result.' }
}

export function candidateCoverage(map, session) {
  const group = map?.sessions?.find((item) => item.session?.id === session)
  if (!group) return null
  const artifacts = Object.values(map.artifacts || {})
    .flat()
    .filter((artifact) => artifact.sessions.includes(session))
  return {
    opening_asks: group.openingAsks.length,
    work_clusters: group.clusters.length,
    checkpoints: group.boundaries.length,
    uncommitted_tails: group.tails.length,
    artifacts: artifacts.length,
  }
}
