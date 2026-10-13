import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import {
  LANGUAGE_PROMPT_REVISION,
  languageShadowProjection,
} from './language-shadow.js'

const H = 'a'.repeat(64)
const H2 = 'b'.repeat(64)

function item(overrides = {}) {
  return {
    annotation_id: `a${'1'.repeat(31)}`,
    event_id: 'event:one',
    session_id: 'session:one',
    seq: 4,
    span: [0, 12],
    category: 'finding',
    speech_act: 'none',
    label: 'Parser observation',
    attempts: 1,
    prompt_sha256: H,
    output_sha256: H2,
    ...overrides,
  }
}

function annotation(items = [item()], overrides = {}) {
  return {
    truth_bearing: false,
    scope: 'at_export',
    run: {
      annotation_id: `a${'2'.repeat(31)}`,
      model: 'fixture:manifest',
      model_digest: H,
      prompt_revision: LANGUAGE_PROMPT_REVISION,
      extractor: 'language-v1:0123456789ab:t3f000000',
      created_at: '2026-08-26T12:34:56Z',
      prompt_sha256: H,
      output_sha256: H2,
    },
    coverage: {
      events: new Set(items.map((entry) => entry.event_id)).size,
      candidates: items.length,
      classified: items.filter((entry) => entry.category !== 'uncertain').length,
      uncertain: items.filter((entry) => entry.category === 'uncertain').length,
      labeled: items.filter((entry) => Object.hasOwn(entry, 'label')).length,
      accepted_claims: items.filter((entry) => entry.category !== 'uncertain').length,
      attempts: items.reduce((sum, entry) => sum + entry.attempts, 0),
      categories: frequencies(items, 'category'),
      speech_acts: frequencies(items, 'speech_act'),
    },
    items,
    ...overrides,
  }
}

function fixture(block = annotation()) {
  return {
    sessions: [{ id: 'session:one' }, { id: 'session:two' }],
    nodes: [{ id: 'node:one', session_id: 'session:one', anchor_event_id: 'event:one', state: 'supported' }],
    receipts: { 'node:one': { claim: {} } },
    language_annotations: block,
  }
}

function frequencies(items, field) {
  const result = {}
  for (const entry of items) result[entry[field]] = (result[entry[field]] || 0) + 1
  return result
}

function clone(value) {
  return JSON.parse(JSON.stringify(value))
}

test('valid projection is session-keyed, deterministic, receipt-addressed, and never emits raw event ids', () => {
  const second = item({
    annotation_id: `a${'3'.repeat(31)}`,
    event_id: 'event:two',
    session_id: 'session:two',
    seq: 1,
    span: [2, 8],
    category: 'uncertain',
    speech_act: 'none',
    attempts: 2,
  })
  delete second.label
  const raw = fixture(annotation([second, item()]))
  raw.nodes.push({ id: 'node:two', session_id: 'session:two', anchor_event_id: 'event:two', state: 'supported' })
  raw.receipts['node:two'] = { claim: {} }

  const projection = languageShadowProjection(raw)
  assert.equal(projection.status, 'ready')
  assert.deepEqual(projection.sessions.map((session) => session.sessionId), ['session:one', 'session:two'])
  assert.deepEqual(projection.sessions.map((session) => session.rows.length), [1, 1])
  assert.equal(projection.sessions[0].rows[0].receipt.nodeId, 'node:one')
  assert.equal(projection.sessions[1].rows[0].receipt.nodeId, 'node:two')
  assert.equal(projection.coverage.items, 2)
  assert.doesNotMatch(JSON.stringify(projection), /event:one|event:two|event_id/)

  const reversed = fixture(annotation([item(), second]))
  reversed.nodes = raw.nodes
  reversed.receipts = raw.receipts
  assert.deepEqual(languageShadowProjection(reversed), projection)
})

test('whole-block and nested objects reject unknown, prototype, accessor, confidence, and path fields', () => {
  for (const mutate of [
    (raw) => { raw.language_annotations.unknown = true },
    (raw) => { raw.language_annotations.items[0].confidence = 0.9 },
    (raw) => { raw.language_annotations.items[0].path = '/private/model' },
    (raw) => { raw.language_annotations.run.reason = 'provider prose' },
    (raw) => { raw.language_annotations.coverage.constructor = 1 },
  ]) {
    const raw = fixture()
    mutate(raw)
    assert.equal(languageShadowProjection(raw).status, 'invalid')
  }

  const ownProto = fixture()
  Object.defineProperty(ownProto.language_annotations.items[0], '__proto__', { value: 'x', enumerable: true })
  assert.equal(languageShadowProjection(ownProto).status, 'invalid')

  const accessor = fixture()
  Object.defineProperty(accessor.language_annotations.run, 'model', { get: () => 'fixture:manifest', enumerable: true })
  assert.equal(languageShadowProjection(accessor).status, 'invalid')
})

test('identity, provenance, count, label, and session failures fail closed', () => {
  const cases = [
    (raw) => { raw.language_annotations.truth_bearing = true },
    (raw) => { raw.language_annotations.scope = 'live' },
    (raw) => { raw.language_annotations.run.model = '/Users/private/model' },
    (raw) => { raw.language_annotations.run.model_digest = 'A'.repeat(64) },
    (raw) => { raw.language_annotations.run.created_at = '2026-02-30T12:00:00Z' },
    (raw) => { raw.language_annotations.run.prompt_revision = 'old' },
    (raw) => { raw.language_annotations.run.extractor = 'language-v1' },
    (raw) => { raw.language_annotations.coverage.candidates = 2 },
    (raw) => { raw.language_annotations.coverage.categories.finding = 2 },
    (raw) => { raw.language_annotations.items[0].session_id = 'missing' },
    (raw) => { raw.language_annotations.items[0].label = 'cargo run' },
  ]
  for (const mutate of cases) {
    const raw = fixture()
    mutate(raw)
    assert.equal(languageShadowProjection(raw).status, 'invalid')
  }
})

test('every closed domain, item relationship, and safe-label rule is enforced', () => {
  const cases = [
    (raw) => { raw.language_annotations.items[0].category = 'success' },
    (raw) => { raw.language_annotations.items[0].speech_act = 'approval' },
    (raw) => { raw.language_annotations.items[0].label = 'five harmless navigation label words' },
    (raw) => { raw.language_annotations.items[0].label = '**Markdown**' },
    (raw) => { raw.language_annotations.items[0].label = 'Verified parser' },
    (raw) => { raw.language_annotations.items[0].label = 'npm install' },
    (raw) => { raw.language_annotations.items[0].label = 'ghp_1234567890abcdefghijkl' },
    (raw) => {
      raw.language_annotations.items[0].category = 'uncertain'
      raw.language_annotations.items[0].speech_act = 'decision'
      raw.language_annotations.coverage.categories = { uncertain: 1 }
      raw.language_annotations.coverage.speech_acts = { decision: 1 }
      raw.language_annotations.coverage.classified = 0
      raw.language_annotations.coverage.uncertain = 1
    },
    (raw) => {
      raw.language_annotations.items[0].category = 'uncertain'
      raw.language_annotations.items[0].speech_act = 'none'
      raw.language_annotations.coverage.categories = { uncertain: 1 }
      raw.language_annotations.coverage.speech_acts = { none: 1 }
      raw.language_annotations.coverage.classified = 0
      raw.language_annotations.coverage.uncertain = 1
    },
  ]
  for (const mutate of cases) {
    const raw = fixture()
    mutate(raw)
    assert.equal(languageShadowProjection(raw).status, 'invalid')
  }

  const uncertain = item({ category: 'uncertain', speech_act: 'none', attempts: 0 })
  delete uncertain.label
  assert.equal(languageShadowProjection(fixture(annotation([uncertain]))).status, 'ready')
})

test('aggregate maps and every recomputable coverage count must exactly match items', () => {
  const cases = [
    (coverage) => { coverage.events = 0 },
    (coverage) => { coverage.classified = 0 },
    (coverage) => { coverage.uncertain = 1 },
    (coverage) => { coverage.labeled = 0 },
    (coverage) => { coverage.attempts = 0 },
    (coverage) => { coverage.accepted_claims = 2 },
    (coverage) => { coverage.categories = { finding: 1, uncertain: 0 } },
    (coverage) => { coverage.categories = { unknown: 1 } },
    (coverage) => { coverage.speech_acts = { none: 2 } },
    (coverage) => { coverage.candidates = -1 },
    (coverage) => { coverage.candidates = 1.5 },
  ]
  for (const mutate of cases) {
    const raw = fixture()
    mutate(raw.language_annotations.coverage)
    assert.equal(languageShadowProjection(raw).status, 'invalid')
  }
})

test('item bounds, dense arrays, uniqueness, and immutable event bindings fail closed', () => {
  const cases = [
    (raw) => { raw.language_annotations.items[0].attempts = 0 },
    (raw) => { raw.language_annotations.items[0].attempts = 3 },
    (raw) => { raw.language_annotations.items[0].seq = -1 },
    (raw) => { raw.language_annotations.items[0].seq = 1.5 },
    (raw) => { raw.language_annotations.items[0].span = [12, 12] },
    (raw) => { raw.language_annotations.items[0].span = [-1, 12] },
    (raw) => { raw.language_annotations.items[0].prompt_sha256 = 'a'.repeat(63) },
    (raw) => { raw.language_annotations.items[0].annotation_id = 'not-an-annotation' },
    (raw) => { raw.sessions.push({ id: 'session:one' }) },
    (raw) => {
      raw.language_annotations.items.push(clone(raw.language_annotations.items[0]))
      raw.language_annotations.coverage.candidates = 2
      raw.language_annotations.coverage.classified = 2
      raw.language_annotations.coverage.labeled = 2
      raw.language_annotations.coverage.accepted_claims = 2
      raw.language_annotations.coverage.attempts = 2
      raw.language_annotations.coverage.categories.finding = 2
      raw.language_annotations.coverage.speech_acts.none = 2
    },
  ]
  for (const mutate of cases) {
    const raw = fixture()
    mutate(raw)
    assert.equal(languageShadowProjection(raw).status, 'invalid')
  }

  const inconsistent = item({
    annotation_id: `a${'3'.repeat(31)}`,
    span: [13, 20],
    seq: 5,
  })
  assert.equal(languageShadowProjection(fixture(annotation([item(), inconsistent]))).status, 'invalid')

  const reusedAnnotation = item({
    event_id: 'event:two',
    span: [13, 20],
  })
  assert.equal(languageShadowProjection(fixture(annotation([item(), reusedAnnotation]))).status, 'invalid')

  const sparse = fixture()
  sparse.language_annotations.items.length = 2
  assert.equal(languageShadowProjection(sparse).status, 'invalid')
})

test('all explicitly forbidden provider and filesystem fields are rejected at every relevant level', () => {
  for (const field of ['confidence', 'reason', 'raw_prompt', 'rawPrompt', 'path']) {
    for (const target of ['top', 'run', 'item']) {
      const raw = fixture()
      const object = target === 'top'
        ? raw.language_annotations
        : target === 'run'
          ? raw.language_annotations.run
          : raw.language_annotations.items[0]
      object[field] = target === 'item' && field === 'confidence' ? 0.8 : 'forbidden'
      assert.equal(languageShadowProjection(raw).status, 'invalid', `${target}.${field}`)
    }
  }
})

test('receipt join requires one same-session ordinary node that owns a receipt', () => {
  const exact = fixture()
  assert.equal(languageShadowProjection(exact).sessions[0].rows[0].receipt.available, true)

  const ambiguous = fixture()
  ambiguous.nodes.push({ id: 'node:duplicate', session_id: 'session:one', anchor_event_id: 'event:one', state: 'supported' })
  ambiguous.receipts['node:duplicate'] = { claim: {} }
  assert.deepEqual(languageShadowProjection(ambiguous).sessions[0].rows[0].receipt, {
    available: false,
    label: 'Exact receipt unavailable',
  })

  const missing = fixture()
  delete missing.receipts['node:one']
  assert.equal(languageShadowProjection(missing).sessions[0].rows[0].receipt.available, false)

  const otherSession = fixture()
  otherSession.nodes.push({ id: 'node:other', session_id: 'session:two', anchor_event_id: 'event:one', state: 'supported' })
  otherSession.receipts['node:other'] = { claim: {} }
  assert.equal(languageShadowProjection(otherSession).sessions[0].rows[0].receipt.available, true)

  const inherited = fixture()
  inherited.receipts = Object.create({ 'node:one': { claim: {} } })
  assert.equal(languageShadowProjection(inherited).sessions[0].rows[0].receipt.available, false)

  const shadowOnly = fixture()
  shadowOnly.nodes[0].sort_key = 'unsupported'
  assert.equal(languageShadowProjection(shadowOnly).sessions[0].rows[0].receipt.available, false)

  const diagnostic = clone(shadowOnly)
  diagnostic.diagnostics = true
  assert.equal(languageShadowProjection(diagnostic).sessions[0].rows[0].receipt.available, true)
})

test('absent optional block stays absent and never consults an inherited block', () => {
  assert.deepEqual(languageShadowProjection({ sessions: [], nodes: [], receipts: {} }), { status: 'absent', sessions: [] })
  const raw = Object.create({ language_annotations: annotation() })
  raw.sessions = []
  raw.nodes = []
  raw.receipts = {}
  assert.deepEqual(languageShadowProjection(raw), { status: 'absent', sessions: [] })
})

test('a valid zero-row projection remains explicit and session-keyed without inventing labels or receipts', () => {
  const projection = languageShadowProjection(fixture(annotation([])))
  assert.equal(projection.status, 'ready')
  assert.deepEqual(projection.sessions.map((session) => session.rows), [[], []])
  assert.deepEqual(projection.coverage, {
    events: 0,
    items: 0,
    classified: 0,
    uncertain: 0,
    labeled: 0,
    acceptedClaims: 0,
    attempts: 0,
    categories: {},
    speechActs: {},
  })
})

test('Work Map inserts one bounded exported-shadow stage before local interpretation with progressive safe rows', () => {
  const component = readFileSync(fileURLToPath(new URL('../mac/work-map.jsx', import.meta.url)), 'utf8')
  const css = readFileSync(fileURLToPath(new URL('../macos.css', import.meta.url)), 'utf8')
  const summary = component.indexOf('<RecordedSummary group={group} map={map} />')
  const shadow = component.indexOf('<LanguageShadowStage')
  const interpretation = component.indexOf('<InterpretationStage')

  assert.ok(summary >= 0 && summary < shadow && shadow < interpretation)
  assert.match(component, /languageShadowProjection\(fixture\)/)
  assert.match(component, /Exported language shadow · at export · cannot change the record/)
  assert.match(component, /projection\.status === 'absent'\) return null/)
  assert.match(component, /projection\.status === 'invalid'/)
  assert.match(component, /showInvalid=\{sessionIndex === 0\}/)
  assert.match(component, /key=\{`language-shadow:\$\{fixture\.project\?\.id \|\| 'project'\}:\$\{group\.session\.id\}:\$\{languageShadow\.status === 'ready' \? languageShadow\.run\.outputDigest : languageShadow\.status\}`\}/)
  assert.match(component, /No labels or receipt links are shown; the deterministic Work Map remains available\./)
  assert.match(component, /Category · \{LANGUAGE_CATEGORY_LABELS\[row\.category\]\}/)
  assert.match(component, /Speech act · \{LANGUAGE_SPEECH_LABELS\[row\.speechAct\]\}/)
  assert.match(component, /No safe navigation label was exported\./)
  assert.match(component, /row\.receipt\.available[\s\S]*onSelect\(row\.receipt\.nodeId\)/)
  assert.match(component, /Exact receipt unavailable/)
  assert.match(component, /const LANGUAGE_ROW_PAGE = 8/)
  assert.match(component, /slice\(0, visibleRows\)/)
  assert.match(component, /Show next \{Math\.min\(LANGUAGE_ROW_PAGE, remaining\)\} language rows/)
  assert.match(component, /Show fewer language rows/)
  assert.match(component, /Row provenance/)
  assert.match(component, /Exported language shadow provenance/)
  for (const forbidden of ['row.eventId', 'row.event_id', 'confidence', 'raw_prompt', 'provider reason', 'row.path']) {
    assert.doesNotMatch(component, new RegExp(forbidden.replace('.', '\\.')))
  }

  assert.match(css, /\.wm-language-rows[^}]*grid-template-columns:\s*repeat\(2, minmax\(0, 1fr\)\)/)
  assert.match(css, /@media \(max-width: 600px\)[\s\S]*\.wm-language-rows[^}]*grid-template-columns:\s*1fr/)
  const scopedCss = css.slice(css.indexOf('.wm-language-trust'), css.indexOf('.wm-item'))
  assert.doesNotMatch(scopedCss, /position:\s*(?:fixed|absolute)|white-space:\s*nowrap/)
})
