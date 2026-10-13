import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import {
  FINDING_CATEGORIES,
  MIND_CONTRACT_VERSION,
  MIND_ENDPOINTS,
  MIND_LOOPBACK,
  MIND_MODEL,
  MIND_MODEL_DIGEST,
  MIND_PROMPT_VERSION,
  MIND_SCHEMA_VERSION,
  RELATION_LABELS,
  SAFE_ACTIONS,
  canonicalMindBytes,
  canonicalMindDigest,
  candidateCoverage,
  deriveMindResponse,
  deriveMindStatus,
  fixtureSourceDigest,
  findingGroupId,
  interpretationBody,
  interpretationHttpFailure,
  mindCheckIsCurrent,
  mindCheckMayStart,
  mindAvailabilityPresentation,
} from './mind.js'
import { workMap } from './work-map.js'

const PROJECT = '0123456789abcdef'
const SESSION = 'public-session-01'
const GENERATED = '2026-08-15T04:25:58Z'
const SOURCE = 'a'.repeat(64)
const DRAFT = 'c'.repeat(64)
const PROMPT = 'd'.repeat(64)
const SCHEMA = 'e'.repeat(64)
const REPRESENTATIVE_FIXTURE = fileURLToPath(new URL('../../../crates/core/tests/fixtures/grounded-mind-public.json', import.meta.url))

const coverage = {
  opening_asks: 1,
  work_clusters: 26,
  checkpoints: 17,
  uncommitted_tails: 0,
  artifacts: 18,
  total: 62,
  included: 62,
  omitted: 0,
}

const kinds = [
  ...Array(1).fill('opening_ask'),
  ...Array(26).fill('work_cluster'),
  ...Array(17).fill('checkpoint'),
  ...Array(18).fill('artifact'),
]

const facts = kinds.map((kind, index) => ({
  id: `fact_${index.toString(16).padStart(16, '0')}`,
  kind,
  recorded_order: index,
  text: `${kind.replaceAll('_', ' ')} ${index + 1} at export.`,
}))

const fixture = {
  receipts: {
    'node-one': { claim: { session: SESSION } },
    'node-two': { claim: { session: SESSION } },
    'other-run-node': { claim: { session: 'other-run' } },
  },
}

const expected = {
  project: PROJECT,
  session: SESSION,
  generatedAt: GENERATED,
  sourceDigest: SOURCE,
  coverage: {
    opening_asks: 1,
    work_clusters: 26,
    checkpoints: 17,
    uncommitted_tails: 0,
    artifacts: 18,
  },
  fixture,
}

const abstained = (reason = 'no_grounded_rows_at_export') => ({ status: 'abstained', reason })
const job = (name, result, suffix) => ({
  result_id: `mind_${suffix.repeat(20)}`,
  job: name,
  scope: 'at_export',
  prompt_digest: PROMPT,
  schema_digest: SCHEMA,
  draft_digest: DRAFT,
  rejected_rows: 0,
  result,
})

function response() {
  return {
    cached: false,
    analysis: {
      contract_version: MIND_CONTRACT_VERSION,
      prompt_version: MIND_PROMPT_VERSION,
      schema_version: MIND_SCHEMA_VERSION,
      tier: 'language',
      mode: 'shadow',
      truth_bearing: false,
      provider: 'ollama',
      model: MIND_MODEL,
      model_digest: MIND_MODEL_DIGEST,
      project: PROJECT,
      session: SESSION,
      generated_at: GENERATED,
      source_digest: SOURCE,
      coverage: structuredClone(coverage),
      facts: structuredClone(facts),
      jobs: [
        job('session_narration', abstained(), '1'),
        job('decision_proposal', abstained(), '2'),
      ],
      output_digest: '',
    },
  }
}

async function seal(value) {
  value.analysis.output_digest = await canonicalMindDigest({ ...value.analysis, output_digest: '' })
  return value
}

function reverseObjectKeys(value) {
  if (Array.isArray(value)) return value.map(reverseObjectKeys)
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value).reverse().map(([key, child]) => [key, reverseObjectKeys(child)]),
    )
  }
  return value
}

const receipt = (suffix, node) => ({ handle: `receipt_${suffix.repeat(16)}`, node })
const rowId = (suffix) => `row_${suffix.repeat(20)}`

function responseWithRows() {
  const value = response()
  value.analysis.jobs[0].rejected_rows = 2
  value.analysis.jobs[0].result = {
    status: 'findings',
    findings: FINDING_CATEGORIES.map((category, index) => ({
      id: rowId(String(index + 3)),
      category,
      text: `${category} work was described at export.`,
      fact_ids: [facts[index + 1].id],
      receipts: index === 0 ? [receipt('1', 'node-one')] : [],
    })),
  }
  value.analysis.jobs[1].rejected_rows = 1
  value.analysis.jobs[1].result = {
    status: 'proposals',
    decisions: [{
      id: rowId('7'),
      text: 'Review the recorded work at export.',
      fact_ids: [facts[6].id],
      receipts: [receipt('2', 'node-one'), receipt('3', 'node-two')],
    }],
    next_steps: [{
      id: rowId('8'),
      action: 'inspect_file',
      text: 'Inspect the cited file from the exported record.',
      fact_ids: [facts[7].id],
      receipts: [],
    }],
  }
  return value
}

test('client pins one fixed local model, two same-origin endpoints, three request keys, and three edge labels', async () => {
  assert.equal(MIND_PROMPT_VERSION, 'grounded-at-export-v2')
  assert.deepEqual(MIND_ENDPOINTS, { status: '/api/mind/status', interpret: '/api/mind/interpret' })
  assert.deepEqual(RELATION_LABELS, {
    recorded: 'Recorded',
    exact_match: 'Exact match',
    model_proposed: 'Model-proposed',
  })
  assert.equal(new Set(Object.values(RELATION_LABELS)).size, 3)
  const body = interpretationBody(PROJECT, SESSION)
  assert.deepEqual(Object.keys(body), ['project', 'session', 'model'])
  assert.deepEqual(body, { project: PROJECT, session: SESSION, model: MIND_MODEL })
  assert.deepEqual(interpretationBody(PROJECT, 'owner:run_01-accepted'), {
    project: PROJECT,
    session: 'owner:run_01-accepted',
    model: MIND_MODEL,
  })
  assert.equal(interpretationBody('UPPERCASE0000000', SESSION), null)
  assert.equal(interpretationBody(PROJECT, '../private'), null)
  for (const invalid of ['run/child', 'run\\child', 'run.child', 'run child', '']) {
    assert.equal(interpretationBody(PROJECT, invalid), null)
  }
  const runOne = FINDING_CATEGORIES.map((category) => findingGroupId('owner:run_01', category))
  const runTwo = FINDING_CATEGORIES.map((category) => findingGroupId('owner:run_02', category))
  assert.equal(new Set([...runOne, ...runTwo]).size, 8)
  assert.equal(findingGroupId('run/child', 'happened'), null)
  assert.equal(findingGroupId('run-one', 'invented'), null)
  assert.equal(await fixtureSourceDigest('abc'), 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad')

  const source = readFileSync(fileURLToPath(new URL('./mind.js', import.meta.url)), 'utf8')
  assert.doesNotMatch(source, /https?:\/\//)
  assert.doesNotMatch(source, /localStorage|sessionStorage|indexedDB/)
  assert.doesNotMatch(JSON.stringify(body), /prompt|transcript|evidence|url|options|api.?key/i)
})

test('status derivation is exact, bounded, shadow-only, and distinguishes unavailable', () => {
  const available = {
    available: true,
    model: MIND_MODEL,
    model_digest: MIND_MODEL_DIGEST,
    provider: 'ollama',
    contract_version: MIND_CONTRACT_VERSION,
    prompt_version: MIND_PROMPT_VERSION,
    schema_version: MIND_SCHEMA_VERSION,
    tier: 'language',
    mode: 'shadow',
    truth_bearing: false,
  }
  assert.deepEqual(deriveMindStatus(available), {
    ok: true,
    state: 'available',
    model: MIND_MODEL,
    modelDigest: MIND_MODEL_DIGEST,
  })
  const unavailable = { ...available, available: false, model_digest: null }
  assert.equal(deriveMindStatus(unavailable).state, 'unavailable')
  for (const mutation of [
    { ...available, model_digest: '0'.repeat(64) },
    { ...available, truth_bearing: true },
    { ...available, mode: 'live' },
    { ...available, extra: 'field' },
    { ...unavailable, model_digest: MIND_MODEL_DIGEST },
  ]) assert.deepEqual(deriveMindStatus(mutation), { ok: false, code: 'invalid_status_response' })
})

test('availability presentation is closed and only the exact available state permits analysis', () => {
  assert.equal(MIND_LOOPBACK, '127.0.0.1:11434')
  const states = {
    idle: mindAvailabilityPresentation({ state: 'idle' }),
    checking: mindAvailabilityPresentation({ state: 'checking' }),
    available: mindAvailabilityPresentation({ state: 'available' }),
    unavailable: mindAvailabilityPresentation({ state: 'unavailable' }),
    statusFailure: mindAvailabilityPresentation({ state: 'failure', reason: 'status' }),
    digestFailure: mindAvailabilityPresentation({ state: 'failure', reason: 'source_digest' }),
  }
  assert.equal(states.available.canAnalyze, true)
  for (const [name, state] of Object.entries(states)) {
    if (name !== 'available') assert.equal(state.canAnalyze, false, name)
  }
  assert.equal(states.unavailable.showRecovery, true)
  assert.equal(states.statusFailure.showRecovery, true)
  assert.equal(states.digestFailure.showRecovery, false)
  assert.equal(states.checking.checking, true)
  assert.equal(mindAvailabilityPresentation({ state: 'checking', recovery: true }).showRecovery, true)
  for (const invalid of [null, {}, { state: 'ready' }, { state: 'failure' }, { state: 'failure', reason: 'provider' }]) {
    assert.equal(mindAvailabilityPresentation(invalid), null)
  }
})

test('status check gates bound overlap and reject stale project epochs', () => {
  assert.equal(mindCheckMayStart(false, null, 1), false)
  assert.equal(mindCheckMayStart(true, 1, 1), false, 'the active check owns this epoch')
  assert.equal(mindCheckMayStart(true, null, 1), true)
  assert.equal(mindCheckMayStart(true, 1, 2), true, 'a new project may supersede the old request')
  assert.equal(mindCheckIsCurrent(1, 1, 1), true)
  assert.equal(mindCheckIsCurrent(2, 2, 1), false, 'the superseded response cannot commit')
  assert.equal(mindCheckIsCurrent(1, 2, 1), false, 'a changed project epoch cannot commit')
})

test('canonical digest framing matches Rust and ignores object insertion order', async () => {
  const value = {
    z: ['snowman ☃', true, null, 17],
    a: { emoji: '🧭', control: 'line\n\u0000tab\t' },
  }
  const permuted = {
    a: { control: 'line\n\u0000tab\t', emoji: '🧭' },
    z: ['snowman ☃', true, null, 17],
  }
  const expectedHex = '4f323a7b4b313a613b4f323a7b4b373a636f6e74726f6c3b5331303a6c696e650a00746162093b4b353a656d6f6a693b53343af09fa7ad3b7d4b313a7a3b41343a5b5331313a736e6f776d616e20e298833b42313a313b4e303a3b49323a31373b5d7d'
  const expectedDigest = '61972fe69792b7eb324afb4f4ce2bd07968fdcf72211955bb57ce9d7797fbd02'
  assert.equal(Buffer.from(canonicalMindBytes(value)).toString('hex'), expectedHex)
  assert.deepEqual(canonicalMindBytes(permuted), canonicalMindBytes(value))
  assert.equal(await canonicalMindDigest(value), expectedDigest)
  assert.equal(await canonicalMindDigest(permuted), expectedDigest)
  assert.throws(() => canonicalMindBytes(-1), /nonnegative safe integer/)
  assert.throws(() => canonicalMindBytes(1.5), /nonnegative safe integer/)
  assert.throws(() => canonicalMindBytes({ 'é': 1 }), /ASCII object key/)
})

test('production response digest is order-independent, mutation-bound, and contract-versioned', async () => {
  const valid = await seal(responseWithRows())
  const accepted = await deriveMindResponse(valid, expected)
  assert.equal(accepted.ok, true)

  const permuted = reverseObjectKeys(valid)
  const permutedResult = await deriveMindResponse(permuted, expected)
  assert.equal(permutedResult.ok, true)
  assert.equal(permutedResult.provenance.outputDigest, accepted.provenance.outputDigest)

  const mutated = structuredClone(valid)
  mutated.analysis.facts[1].text = 'A different safe work summary at export.'
  assert.deepEqual(await deriveMindResponse(mutated, expected), {
    ok: false,
    code: 'invalid_interpretation_response',
  })

  const oldContract = structuredClone(valid)
  oldContract.analysis.contract_version = 'grounded-mind-v1'
  await seal(oldContract)
  assert.deepEqual(await deriveMindResponse(oldContract, expected), {
    ok: false,
    code: 'invalid_interpretation_response',
  })
})

test('representative response resolves exactly 62 facts without changing deterministic coverage', async () => {
  const result = await deriveMindResponse(await seal(response()), expected)
  assert.equal(result.ok, true)
  assert.equal(result.facts.length, 62)
  assert.deepEqual(result.coverage, coverage)
  const map = workMap(JSON.parse(readFileSync(REPRESENTATIVE_FIXTURE, 'utf8')))
  assert.deepEqual(candidateCoverage(map, SESSION), expected.coverage)
  assert.deepEqual(Object.keys(result.categories), FINDING_CATEGORIES)
  assert.ok(Object.values(result.categories).every((rows) => rows.length === 0))
  assert.equal(result.narration.abstained, true)
  assert.equal(result.proposal.abstained, true)
  assert.equal(result.cached, false)
})

test('coverage is scoped to the requested run and artifact session eligibility', () => {
  const map = {
    sessions: [
      { session: { id: 'run-one' }, openingAsks: [{}], clusters: [{}, {}], boundaries: [{}], tails: [] },
      { session: { id: 'run-two' }, openingAsks: [], clusters: [{}], boundaries: [], tails: [{}] },
    ],
    artifacts: {
      exists: [{ sessions: ['run-one'] }, { sessions: ['run-one', 'run-two'] }],
      unresolved: [{ sessions: ['run-two'] }],
      other: [{ sessions: [] }],
    },
  }
  assert.deepEqual(candidateCoverage(map, 'run-one'), {
    opening_asks: 1, work_clusters: 2, checkpoints: 1, uncommitted_tails: 0, artifacts: 2,
  })
  assert.deepEqual(candidateCoverage(map, 'run-two'), {
    opening_asks: 0, work_clusters: 1, checkpoints: 0, uncommitted_tails: 1, artifacts: 2,
  })
  assert.equal(candidateCoverage(map, 'missing-run'), null)
})

test('four category groups, rejected counts, proposals, actions, facts, and zero or multiple receipts survive validation', async () => {
  const result = await deriveMindResponse(await seal(responseWithRows()), expected)
  assert.equal(result.ok, true)
  assert.deepEqual(Object.fromEntries(Object.entries(result.categories).map(([key, rows]) => [key, rows.length])), {
    happened: 1,
    missing: 1,
    lagging: 1,
    contradictory: 1,
  })
  assert.equal(result.narration.rejectedRows, 2)
  assert.equal(result.proposal.rejectedRows, 1)
  assert.equal(result.categories.happened[0].facts[0].text, facts[1].text)
  assert.deepEqual(result.categories.happened[0].receipts.map((item) => item.node), ['node-one'])
  assert.deepEqual(result.decisions[0].receipts.map((item) => item.node), ['node-one', 'node-two'])
  assert.equal(result.nextSteps[0].action, 'inspect_file')
  assert.equal(result.nextSteps[0].receipts.length, 0)
  assert.deepEqual(SAFE_ACTIONS, [
    'inspect_receipt', 'inspect_file', 'review_unfinished', 'verify_result', 'clarify_decision', 'continue_recording',
  ])
})

test('mismatches, partial jobs, unknown or duplicate links, unsafe rows, and malformed shapes fail without leaking model text', async () => {
  const cases = []
  const add = async (mutate) => {
    const value = responseWithRows()
    mutate(value)
    cases.push(await seal(value))
  }
  await add((value) => { value.analysis.project = '0000000000000000' })
  await add((value) => { value.analysis.session = 'other-run' })
  await add((value) => { value.analysis.source_digest = 'f'.repeat(64) })
  await add((value) => { value.analysis.model = 'another-model' })
  await add((value) => { value.analysis.model_digest = '0'.repeat(64) })
  await add((value) => { value.analysis.truth_bearing = true })
  await add((value) => { value.analysis.mode = 'live' })
  await add((value) => { value.analysis.jobs[0].scope = 'live' })
  await add((value) => { value.analysis.jobs.pop() })
  await add((value) => { value.analysis.facts[1].id = value.analysis.facts[0].id })
  await add((value) => { value.analysis.facts[1].text = '/Users/example/private/file.txt' })
  await add((value) => { value.analysis.facts[1].text = 'bash -c whoami → output' })
  await add((value) => { value.analysis.jobs[0].result.findings[0].fact_ids = ['fact_ffffffffffffffff'] })
  await add((value) => { value.analysis.jobs[0].result.findings[0].fact_ids.push(value.analysis.jobs[0].result.findings[0].fact_ids[0]) })
  await add((value) => { value.analysis.jobs[0].result.findings[0].category = 'invented' })
  await add((value) => { value.analysis.jobs[1].result.next_steps[0].action = 'delete_file' })
  await add((value) => { value.analysis.jobs[0].result.findings[0].text = '/Users/example/private/file.txt' })
  await add((value) => { value.analysis.jobs[0].result.findings[0].text = 'bash -c whoami → output' })
  await add((value) => { value.analysis.jobs[0].result.findings[0].text = 'token=synthetic_canary_value_123456' })
  await add((value) => { value.analysis.jobs[0].result.findings[0].text = 'This is currently complete.' })
  await add((value) => { value.analysis.jobs[0].result.findings[0].receipts[0].node = 'other-run-node' })
  await add((value) => { value.analysis.jobs[0].result.findings[0].extra = 'unknown' })
  await add((value) => { value.analysis.jobs[0].result = { status: 'findings', findings: [] } })
  await add((value) => { value.analysis.generated_at = '2026-02-31T00:00:00Z' })
  await add((value) => { value.extra = 'unknown' })
  const forgedDigest = await seal(responseWithRows())
  forgedDigest.analysis.output_digest = '0'.repeat(64)
  cases.push(forgedDigest)

  for (const value of cases) {
    const result = await deriveMindResponse(value, expected)
    assert.deepEqual(result, { ok: false, code: 'invalid_interpretation_response' })
    assert.doesNotMatch(JSON.stringify(result), /synthetic_canary|whoami|private\/file/)
  }
})

test('HTTP failures stay distinct from empty interpretation', () => {
  assert.equal(interpretationHttpFailure(409).state, 'conflict')
  assert.equal(interpretationHttpFailure(503).state, 'unavailable')
  for (const status of [400, 413, 422]) assert.equal(interpretationHttpFailure(status).state, 'invalid_request')
  assert.equal(interpretationHttpFailure(500).state, 'failure')
})

test('the Work Map consumes only validated shadow rows and keeps every relation and receipt door explicit', () => {
  const component = readFileSync(fileURLToPath(new URL('../mac/work-map.jsx', import.meta.url)), 'utf8')
  const shell = readFileSync(fileURLToPath(new URL('../mac/shell.jsx', import.meta.url)), 'utf8')
  const css = readFileSync(fileURLToPath(new URL('../macos.css', import.meta.url)), 'utf8')

  assert.match(component, /data-relation=\{RELATION_LABELS\[kind\]\}/)
  assert.match(component, /FINDING_CATEGORIES\.map/)
  assert.match(component, /No valid local-model row survived for this category\. This is not proof of absence\./)
  assert.match(component, /Model-proposed · not a decision/)
  assert.match(component, /onSelect\(receipt\.node\)/)
  assert.match(component, /No exact receipt was supplied for this cited row\./)
  assert.match(component, /An exact claim receipt is unavailable/)
  assert.match(component, /Lines group and navigate; they never claim causality\./)
  assert.match(component, /findingGroupId\(session, category\)/)
  assert.match(component, /\{analysis\.provenance\.generatedAt\}/)
  assert.match(component, /\{analysis\.provenance\.sourceDigest\}/)
  assert.equal([...component.matchAll(/Local only · No API key · No usage cost · Shadow — cannot change the record/g)].length, 1)
  assert.match(component, /<details id=\{recordedDetailId\(group\.session\.id\)\} className="wm-stage wm-recorded-detail">/)
  assert.doesNotMatch(component, /className="wm-stage wm-recorded-detail"[^>]*\sopen=/)
  assert.match(component, /<RelationLabel kind="recorded" detail="Deterministic support" \/>/)
  assert.match(component, /<RelationLabel kind="model_proposed" detail="Optional shadow" \/>/)
  const sessionRender = component.indexOf('{map.sessions.map((group, sessionIndex) => (')
  const summaryStage = component.indexOf('<RecordedSummary group={group} map={map} />', sessionRender)
  const languageStage = component.indexOf('<LanguageShadowStage', summaryStage)
  const interpretationStage = component.indexOf('<InterpretationStage', languageStage)
  const detailStage = component.indexOf('<RecordedDetail group={group} onSelect={onSelect} />', interpretationStage)
  assert.ok(sessionRender >= 0 && summaryStage > sessionRender && languageStage > summaryStage && interpretationStage > languageStage && detailStage > interpretationStage)
  assert.doesNotMatch(component.slice(sessionRender, interpretationStage), /group\.(?:clusters|boundaries|tails)\.map/)
  assert.doesNotMatch(component, /dangerouslySetInnerHTML|draft_digest|raw model|raw prompt/i)

  assert.match(shell, /const sourceText = await res\.text\(\)/)
  assert.match(shell, /fixtureSourceDigest\(sourceText\)/)
  assert.match(shell, /JSON\.parse\(sourceText\)/)
  assert.ok(shell.indexOf('setFixture(doc)') < shell.indexOf('const digest = await fixtureSourceDigest(sourceText)'))
  assert.match(shell, /catch \(_digestError\)[\s\S]*setMindStatus\(\{ state: 'failure', reason: 'source_digest' \}\)/)
  assert.match(shell, /body: JSON\.stringify\(request\)/)
  assert.doesNotMatch(shell, /localStorage|sessionStorage|indexedDB/)
  assert.doesNotMatch(shell, /body:\s*JSON\.stringify\(\{[^}]*prompt/)

  assert.match(css, /\.wm-session-graph::before/)
  assert.match(css, /\.wm-finding-branches::before/)
  assert.match(css, /\.wm-finding-group::before/)
  assert.match(css, /\.wm-provenance dd[^}]*overflow-wrap: anywhere/)
  assert.match(css, /@media \(max-width: 900px\)/)
})

test('availability recovery is explicit, local-only, and cannot bypass the guarded status path', () => {
  const component = readFileSync(fileURLToPath(new URL('../mac/work-map.jsx', import.meta.url)), 'utf8')
  const shell = readFileSync(fileURLToPath(new URL('../mac/shell.jsx', import.meta.url)), 'utf8')
  const css = readFileSync(fileURLToPath(new URL('../macos.css', import.meta.url)), 'utf8')

  assert.match(component, /function MindRecovery\(\{ status, statusId, onCheckMind \}\)/)
  assert.match(component, /Optional local shadow interpretation/)
  assert.match(component, /No cloud API, account, or API key is used/)
  assert.match(component, /\{MIND_LOOPBACK\}/)
  assert.match(component, /\{MIND_MODEL\}/)
  assert.match(component, /\{MIND_MODEL_DIGEST\}/)
  assert.match(component, /does not download, pull, start, update, or substitute models/)
  assert.match(component, /does not distinguish an absent service, absent model, or digest mismatch/)
  assert.match(component, /disabled=\{availability\.checking\}/)
  assert.match(component, /onClick=\{\(\) => onCheckMind\(\)\}/)
  assert.match(component, /disabled=\{!analysisReady \|\| analyzing\}/)
  assert.match(component, /aria-describedby=\{statusId\}/)
  assert.match(component, /onCheckMind=\{onCheckMind\}/)

  assert.equal([...shell.matchAll(/fetch\(MIND_ENDPOINTS\.status/g)].length, 1)
  assert.match(shell, /onCheckMind=\{recheckMindStatus\}/)
  assert.doesNotMatch(component, /fetch\(|localStorage|sessionStorage|indexedDB/)
  const recoveryCss = css.slice(css.indexOf('.wm-mind-recovery'), css.indexOf('.wm-analysis {'))
  assert.ok(recoveryCss.length > 0)
  assert.doesNotMatch(recoveryCss, /position:\s*(?:fixed|absolute)|\bbottom:|\bright:|z-index/)
})
