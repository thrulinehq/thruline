import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* T5 (report F13, its T5 part only) — the message for the agent is rebuilt
 * whenever the Change's verdict moves.
 *
 * Done when (F13, verbatim): "The draft changes whenever the verdict does".
 * Predeclared, testable form (card T5):
 *
 *  1. A Change whose claim "Tests pass." has an earlier passing run and then
 *     a failing Check-it-again run on the current version: the draft offered
 *     on the Change screen after the reload differs from the one before, and
 *     the after-draft carries the failing run's own typed values (version,
 *     `tests_failed`, `tests_passed`, the recorded failing line) and no
 *     sentence that the tests passed.
 *  2. For every verdict transition in a table, the draft is a pure function
 *     of the served fields: same fields, same bytes; a different verdict, a
 *     different draft.
 *  3. No draft is stored anywhere: it is composed at render from the
 *     documents the last read returned.
 *
 * Every fixture here is synthetic. No string below came off a real machine.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-tell-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeScreen, ChangeShell, readChange } from './change-workspace.jsx';
  import { buildChangeView } from './change-view.js';
  import { buildCoverageView, evidenceEntries, unlinkedCompletedRun } from './coverage-view.js';
  import { deriveChangeModel } from './change-model.js';
  import { tellDraftOf, cardsTold, lineKindOf, TELL_KINDS } from './tell-draft.js';
  import { HQ_COPY } from './hq-copy.js';
  import { SlotError } from './hq-narrate.js';
  export { buildChangeView, buildCoverageView, evidenceEntries, unlinkedCompletedRun, deriveChangeModel, tellDraftOf, cardsTold, lineKindOf, TELL_KINDS, HQ_COPY, SlotError, readChange };
  export const renderScreen = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderRoute = props => renderToStaticMarkup(React.createElement(ChangeShell, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'tell.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const { buildChangeView, buildCoverageView, evidenceEntries, unlinkedCompletedRun, deriveChangeModel, tellDraftOf, cardsTold, TELL_KINDS, HQ_COPY, SlotError, renderScreen, renderRoute, readChange } = M
const T = HQ_COPY.change.tell
const D = T.draft

const CLAIM = 'Tests pass.'
const REVISION = 'e71c5b04d2a9f3c1b8e6d7a5c4b3a2918f7e6d5c'
const VERSION = REVISION.slice(0, 7)
const FAILING_LINE = 'FAILED checkout/tests/test_api.py::test_total_with_discount - assert 90 == 95'
const CHECK = 'bash:synthetic-repo:pytest -q tests/test_totals.py checkout/'

/* The run door's command at 18ada3e7, word for word (`hq-copy.js`
 * `change.coverage.run_check_command`), filled with this Change's id. */
const RUN_DOOR = `thruline check --for-change cccccccc-0000-4000-8000-00000000c051 -- <the command you would run>`
const DECIDES = 'A person then decides which line the run supports.'

const CHANGE = {
  id: 'cccccccc-0000-4000-8000-00000000c051',
  project_id: 'cccccccc-0000-4000-8000-00000000p051',
  opened_at: '2026-01-02T09:00:00Z',
  request: { title: 'Fix the checkout total', purpose: 'Fix the checkout total', requester: 'sam', source: 'person' },
  accepted_scope: {
    version: 1, criteria: [CLAIM], exclusions: [], allowed_scope: ['checkout/**'],
    constraints: [], check_profile: ['pytest'],
    content_sha256: '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
    acceptance: { actor: 'sam', accepted_at: '2026-01-02T09:06:00Z', rationale: 'what I want checked' },
    proposed_at: '2026-01-02T09:05:00Z',
  },
  implementation: {
    version: 1, bound_at: '2026-01-02T10:00:00Z', repository_identity: 'repo-synthetic-0051',
    base_revision: REVISION, snapshot_id: null, workspace_identity: 'workspace-synthetic-0051',
  },
  associations: [], examination: 'none_yet', decision: 'none_yet',
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
}

const item = (n, over = {}) => ({
  link_id: 'cce_synthetic_05' + n, kind: 'check_run', evidence_id: 'evb_synthetic_05' + n,
  session_id: 'cli:check:2026-01-02T10:0' + n + ':00Z', basis: 'named', basis_slug: 'basis_named',
  name: CHECK, observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies',
  asserted_at: '2026-01-02T10:0' + n + ':00Z', execution: 'completed', execution_slug: 'execution_completed',
  failure_line: null, evidence_class: null, evidence_class_slug: null,
  ...over,
})

const execution = (n) => ({
  id: 'rex_synthetic_05' + n, run_key: 'run_synthetic_05' + n, binding_id: 'evb_synthetic_05' + n,
  name: CHECK, state: 'completed', state_slug: 'execution_completed', settles: true,
  recorded_at: '2026-01-02T10:0' + n + ':30Z',
})

/** The run sheet's `checks`, as `GET /api/changes/<id>/run` serves them: the LATEST run per check. */
const sheet = (n, outcome, passed, failed, line) => [{
  id: 'pytest', program: 'pytest',
  last: { run: 'run_synthetic_05' + n, outcome, tests_passed: passed, tests_failed: failed, failure_line: line || null },
}]

const coverage = ({ items, executions, run_checks, over = {} }) => ({
  schema_version: 1, change_id: CHANGE.id, scope_version: 1, link_rows: items.length,
  executions,
  criteria: [{ index: 1, text: CLAIM, state: items.length ? 'evidence' : 'none', state_slug: 'coverage_evidence', items, removed: [] }],
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
  run_checks,
  ...over,
})

/* BEFORE: one passing run on the current version. */
const BEFORE = coverage({
  items: [item(1)],
  executions: [execution(1)],
  run_checks: sheet(1, 'passed', 4, 0),
})
/* AFTER the Check-it-again reload: the same passing run, then a failing one
 * on the same version — 1 failed, 4 passed, with the line it printed. */
const AFTER = coverage({
  items: [item(1), item(2, { observed: 'not_proven', failure_line: FAILING_LINE })],
  executions: [execution(1), execution(2)],
  run_checks: sheet(2, 'failed', 4, 1, FAILING_LINE),
})

const hqOf = (cov, change = CHANGE) => deriveChangeModel(change, cov, null, null, [])

/** What `readChange` builds, for the screen. */
function screenModel(cov) {
  const model = buildChangeView({ change: CHANGE, proposals: null, work: [], evidence: evidenceEntries(cov), unlinkedRun: unlinkedCompletedRun(cov) })
  model.coverage = buildCoverageView(cov)
  model.hq = hqOf(cov)
  return model
}

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>')
/** The draft the screen OFFERS: the text inside its one draft block. */
const offered = (html) => {
  const m = /<pre class="ws-tell-draft">([\s\S]*?)<\/pre>/.exec(html)
  return m ? unescape(m[1]) : null
}

/* ── 1. the failing re-run ─────────────────────────────────────────────── */

test('T5 done-when 1: after a failing Check-it-again reload, the draft on the Change screen changes and carries the failing run\'s own typed values', () => {
  const before = renderScreen({ model: screenModel(BEFORE), stage: 'verify', onCopy: () => {} })
  const after = renderScreen({ model: screenModel(AFTER), stage: 'verify', onCopy: () => {} })
  const draftBefore = offered(before)
  const draftAfter = offered(after)
  assert.ok(draftAfter, 'after a failing re-run the Change screen offers a message for the agent')
  assert.notEqual(draftAfter, draftBefore, 'the draft did not change when the verdict did')
  /* The failing run's own typed values. */
  assert.ok(draftAfter.includes(VERSION), 'the version the failing run was on')
  assert.ok(draftAfter.includes('1 failed'), 'tests_failed')
  assert.ok(draftAfter.includes('4 passed'), 'tests_passed')
  assert.ok(draftAfter.includes(FAILING_LINE), 'the recorded failing line')
  /* And no sentence that the tests passed. The only "pass" words allowed are
   * the quoted claim itself and the run's own count, which is the record
   * speaking; with both removed, nothing about passing may remain. */
  const stripped = draftAfter.split(`"${CLAIM}"`).join('').split('1 failed, 4 passed').join('')
  assert.doesNotMatch(stripped, /\bpass(ed|es)?\b/i, 'a sentence says the tests passed after a failing re-run')
  assert.doesNotMatch(draftAfter, /\bproven\b|\bcovered\b/i)
})

test('T5 done-when 1, on the route a fresh install opens (Brief): the same draft, rebuilt the same way', () => {
  const route = (cov) => renderRoute({
    model: screenModel(cov), stage: 'verify', altitude: 'brief', register: 'manager',
    onGo: () => {}, onCopy: () => {}, onReload: () => {}, onAltitude: () => {}, onRegister: () => {},
  })
  const b = offered(route(BEFORE))
  const a = offered(route(AFTER))
  assert.equal(b, null, 'a Change proven on this version offers no message')
  assert.equal(a, tellDraftOf(hqOf(AFTER)).draft, 'Brief and the pure function are one text')
  assert.ok(route(BEFORE).includes(unescapeInverse(T.covered.replace('{version}', VERSION))), 'the covered line says why no message is offered')
})

function unescapeInverse(s) { return s.replace(/&/g, '&amp;').replace(/'/g, '&#x27;').replace(/"/g, '&quot;') }

test('T5: the draft is the Change\'s own — every finding card copies it, and no card keeps a session prompt that the record has moved past', () => {
  const tell = tellDraftOf(hqOf(AFTER))
  const card = { id: 'c1', sessionId: 's1', do: { text: 'Nothing. This one is covered.', prompt: null } }
  const stale = { id: 'c2', sessionId: 's1', do: { text: 'Re-run the check.', prompt: 'Re-run the check that covers this and paste the output.' } }
  for (const c of cardsTold([card, stale], tell)) assert.equal(c.do.prompt, tell.draft)
  /* Covered: no card offers a message. */
  for (const c of cardsTold([stale], tellDraftOf(hqOf(BEFORE)))) assert.equal(c.do.prompt, null)
  /* No accepted line: the cards are the session's, unchanged. */
  assert.deepEqual(cardsTold([stale], null), [stale])
})

/* ── 2. the transition table ───────────────────────────────────────────── */

const STATES = {
  proven: coverage({ items: [item(1)], executions: [execution(1)], run_checks: sheet(1, 'passed', 4, 0) }),
  opposite: AFTER,
  older: coverage({ items: [item(1, { applies: false, applies_slug: 'inputs_other_app' })], executions: [execution(1)], run_checks: sheet(1, 'passed', 4, 0) }),
  nothing: coverage({ items: [], executions: [], run_checks: [] }),
  ran_nothing: coverage({ items: [item(1, { observed: 'couldnt', bears: false })], executions: [execution(1)], run_checks: [] }),
}
/* Fog: the Change names no version, so nothing can be said about "this" one. */
const UNBOUND = { ...CHANGE, implementation: null }
const hqState = (s) => (s === 'fog' ? hqOf(STATES.proven, UNBOUND) : hqOf(STATES[s]))

const TRANSITIONS = [
  ['proven', 'opposite'],
  ['opposite', 'proven'],
  ['older', 'opposite'],
  ['older', 'proven'],
  ['nothing', 'opposite'],
  ['ran_nothing', 'proven'],
  ['proven', 'fog'],
  ['opposite', 'fog'],
  ['older', 'fog'],
  ['nothing', 'fog'],
]

for (const [from, to] of TRANSITIONS) {
  test(`T5 done-when 2: ${from} → ${to} gives a different draft; the same fields give the same bytes`, () => {
    const a1 = tellDraftOf(hqState(from))
    const a2 = tellDraftOf(hqState(from))
    const b = tellDraftOf(hqState(to))
    assert.equal(JSON.stringify(a1), JSON.stringify(a2), 'same fields, different bytes')
    assert.notEqual(a1.kind, b.kind, 'the verdict moved and the kind did not')
    assert.notEqual(a1.draft, b.draft, 'the verdict moved and the draft did not')
  })
}

test('T5: the kinds, worst first, and each draft names the command that records the run and leaves the line to a person', () => {
  assert.deepEqual([...TELL_KINDS], ['opposite', 'older', 'nothing', 'fog'])
  assert.equal(tellDraftOf(hqState('proven')).kind, 'covered')
  assert.equal(tellDraftOf(hqState('opposite')).kind, 'opposite')
  assert.equal(tellDraftOf(hqState('older')).kind, 'older')
  assert.equal(tellDraftOf(hqState('nothing')).kind, 'nothing')
  assert.equal(tellDraftOf(hqState('ran_nothing')).kind, 'nothing', 'a run that settled nothing is never a pass')
  assert.equal(tellDraftOf(hqState('fog')).kind, 'fog')
  for (const s of ['opposite', 'older', 'nothing', 'ran_nothing', 'fog']) {
    const { draft } = tellDraftOf(hqState(s))
    assert.ok(draft.startsWith(D.preamble), `${s}: the untrusted-data preamble opens it`)
    /* D-050 (BUILD-16-R2): this loop asserted the draft ends with "Prepared,
     * not sent", asks the agent to "paste the full output", and carries the
     * masking line. All three were person-facing or asked for prose the
     * screen never counts; they now live on the screen, and the draft ends
     * with the command and who decides. */
    assert.ok(draft.endsWith(RUN_DOOR + ' ' + DECIDES), `${s}: the command and "a person decides" close it`)
    assert.doesNotMatch(draft, /paste/i, `${s}: the draft asks the agent to paste something`)
    assert.ok(!draft.includes('How the record was masked'), `${s}: the masking line is the person's, not the agent's`)
  }
  /* Missing evidence never satisfies a check: nothing run, and a run that
   * settled nothing, never read as a pass. */
  for (const s of ['nothing', 'ran_nothing', 'older']) assert.doesNotMatch(tellDraftOf(hqState(s)).draft.split(`"${CLAIM}"`).join(''), /\bpass(ed|es)?\b/i)
})

test('T5: a count the record does not hold is said as absent, never as zero', () => {
  const uncounted = coverage({
    items: [item(1), item(2, { observed: 'not_proven', failure_line: null })],
    executions: [execution(1), execution(2)],
    run_checks: null,
  })
  const { draft } = tellDraftOf(hqOf(uncounted))
  assert.ok(draft.includes('the record holds no count of its tests'))
  assert.ok(draft.includes(D.failure_none), 'no failing line is said as none, not invented')
  assert.doesNotMatch(draft, /\b0 failed\b|\b0 passed\b/)
})

/* ── BUILD-16-R2: the copied text is the agent's (path step 5, PW-F8) ──── */

/* Every sentence at 18ada3e7 that addresses the person, not the agent. */
const PERSON_FACING = [
  'Prepared, not sent.',
  "ThruLine doesn't run the agent",
  'How the record was masked before this was written:',
  'The record this was built from does not say how it was masked.',
  'copy it and paste it yourself',
]
const OPEN_STATES = ['opposite', 'older', 'nothing', 'ran_nothing', 'fog']

test('BUILD-16-R2 1: every draft quotes the run door\'s own command for this Change and says a person then decides which line the run supports', () => {
  assert.equal(RUN_DOOR, HQ_COPY.change.coverage.run_check_command.replace('{change}', CHANGE.id), 'the run door\'s command moved; this test quotes it')
  for (const s of OPEN_STATES) {
    const { draft } = tellDraftOf(hqState(s))
    assert.ok(draft.includes(RUN_DOOR), `${s}: the command that records a run for this Change`)
    assert.ok(draft.includes(DECIDES), `${s}: "a person then decides which line the run supports"`)
  }
})

test('BUILD-16-R2 2: the copied text holds none of the person-facing sentences and asks the agent to paste nothing', () => {
  for (const s of OPEN_STATES) {
    const { draft } = tellDraftOf(hqState(s))
    for (const words of PERSON_FACING) assert.ok(!draft.includes(words), `${s}: the copied text carries a person-facing sentence: ${words}`)
    assert.doesNotMatch(draft, /\bpaste\b/i, `${s}: the copied text asks for pasted output`)
  }
})

test('BUILD-16-R2 3: the copied text never says the run will count, pass or prove a line', () => {
  for (const s of OPEN_STATES) {
    const { draft } = tellDraftOf(hqState(s))
    const last = draft.split('\n\n').pop()
    assert.ok(last.includes(RUN_DOOR), `${s}: the command closes the draft`)
    /* The paragraph that asks for the run: no word of counting or settling. */
    assert.doesNotMatch(last.replace(RUN_DOOR, ''), /\bcount|\bpass|\bprov|\bsettle|\bdone\b|\bgreen\b/i, `${s}: the run request promises a result`)
    /* And nowhere in the draft is a run promised a result. */
    assert.doesNotMatch(draft, /\bwill (?:count|pass|prove|be (?:counted|proven))|\b(?:counts|is counted) (?:for|towards?)\b|\bmakes? (?:it|the line|this line) (?:pass|count|proven)/i, `${s}: the draft says the run will count`)
  }
})

test('BUILD-16-R2 4: the screen still shows the person-facing notes, beside the copy control and outside the copied text', () => {
  for (const html of [
    renderScreen({ model: screenModel(AFTER), stage: 'verify', onCopy: () => {} }),
    renderRoute({ model: screenModel(AFTER), stage: 'verify', altitude: 'brief', register: 'manager', onGo: () => {}, onCopy: () => {}, onReload: () => {}, onAltitude: () => {}, onRegister: () => {} }),
  ]) {
    const draft = offered(html)
    assert.ok(draft, 'a message is offered')
    const section = /<section class="ws-tell"[\s\S]*?<\/section>/.exec(html)[0]
    const outside = unescape(section.replace(/<pre class="ws-tell-draft">[\s\S]*?<\/pre>/, ''))
    assert.ok(outside.includes("Prepared, not sent. ThruLine doesn't run the agent"), '"Prepared, not sent" is on the screen')
    assert.ok(outside.includes('How the record was masked before this was written: detector v2 · nothing matched a known secret shape'), 'the masking statement is on the screen')
    assert.ok(!draft.includes('How the record was masked'), 'the masking statement is not in the copied text')
    /* Beside the copy control: the notes follow the button inside the same section. */
    assert.ok(section.indexOf('ws-tell-masking') > section.indexOf('<button'), 'the masking note sits after the copy control')
  }
})

test('BUILD-16-R2 5: masking does not change — a secret-shaped fixture in the Change\'s ask and criteria reaches the copied text masked exactly as the record served it', () => {
  /* The UI has no masker of its own: masking is the engine's (`redact.rs`),
   * applied before a string is served, and the draft copies what it is
   * served. This pins that the copied text neither unmasks nor re-masks: the
   * served markers survive byte for byte, and no raw secret body appears. */
  const ASK = 'Stop logging [masked: aws] from the checkout job'
  const LINE = 'The deploy key [masked: github] is read from the environment.'
  const BROKE = 'FAILED checkout/tests/test_env.py::test_key - AssertionError: [masked: github] != None'
  const change = { ...CHANGE, request: { ...CHANGE.request, title: ASK, purpose: ASK }, accepted_scope: { ...CHANGE.accepted_scope, criteria: [LINE] } }
  const cov = coverage({
    items: [item(1), item(2, { observed: 'not_proven', failure_line: BROKE })],
    executions: [execution(1), execution(2)],
    run_checks: sheet(2, 'failed', 4, 1, BROKE),
    over: { criteria: [{ index: 1, text: LINE, state: 'evidence', state_slug: 'coverage_evidence', items: [item(1), item(2, { observed: 'not_proven', failure_line: BROKE })], removed: [] }] },
  })
  const { draft } = tellDraftOf(hqOf(cov, change))
  const paras = draft.split('\n\n')
  /* The ask and the line, as served — the same sentences as at 18ada3e7. */
  assert.equal(paras[1], `This is about "${ASK}", on version ${VERSION}.`)
  assert.ok(paras[2].startsWith(`One thing that counts as done: "${LINE}" On the current version, ${VERSION}, a run shows the opposite: ${CHECK} — 1 failed, 4 passed. What the run printed when it broke: ${BROKE}`), paras[2])
  for (const marker of ['[masked: aws]', '[masked: github]']) assert.ok(draft.includes(marker), `the served marker ${marker} survives`)
  assert.doesNotMatch(draft, /TLCANARY|AKIA|ghp_/, 'a raw secret body reached the copied text')
})

/* ── 3. nothing stored; every word from the copy file; no hole ─────────── */

test('T5 done-when 3: no draft is stored anywhere — the module holds no storage, no cache and no state', () => {
  const source = readFileSync(join(here, 'tell-draft.js'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '')
  assert.doesNotMatch(source, /localStorage|sessionStorage|indexedDB|new Map\(|new WeakMap\(|\blet\b|useState|useRef/, 'the draft module can keep something between reads')
  /* And the screen composes it per render from what it was given. */
  const one = offered(renderScreen({ model: screenModel(AFTER), stage: 'verify' }))
  const two = offered(renderScreen({ model: screenModel(BEFORE), stage: 'verify' }))
  const three = offered(renderScreen({ model: screenModel(AFTER), stage: 'verify' }))
  assert.equal(one, three)
  assert.notEqual(one, two)
})

test('T5: a slot the record cannot fill throws rather than rendering a hole', () => {
  const nameless = coverage({ items: [item(2, { observed: 'not_proven', name: '' })], executions: [execution(2)], run_checks: sheet(2, 'failed', 0, 1) })
  assert.throws(() => tellDraftOf(hqOf(nameless)), SlotError)
})

test('T5: on the screen, a draft that cannot be built says which value was missing and offers no message — it never renders a hole', () => {
  const nameless = coverage({ items: [item(2, { observed: 'not_proven', name: '' })], executions: [execution(2)], run_checks: sheet(2, 'failed', 0, 1) })
  const html = renderScreen({ model: screenModel(nameless), stage: 'verify' })
  assert.equal(offered(html), null)
  assert.ok(html.includes(unescapeInverse(T.unbuilt.replace('{slot}', 'check'))))
})

test('T5: every sentence in a draft is a sentence of HQ_COPY.change.tell, filled', () => {
  /* A quotation that ends its own sentence takes the template's stop
   * (`hq-narrate.js` `fill`/`compose`), so `"{what}".` may read `"Tests pass."`.
   * One regex over the whole paragraph, so the engine backtracks across
   * sentence boundaries rather than this test guessing them. */
  const one = Object.values(D).map((t) => '(?:' + t
    .replace(/"\{[a-z_]+\}"\./g, '\u0000')
    .replace(/[.*+?^$()|[\]\\]/g, '\\$&')
    .replace(/\{[a-z_]+\}/g, '.+?')
    .replace(/\u0000/g, '".+?"\\.?') + ')').join('|')
  const paragraph = new RegExp('^(?:' + one + ')(?: (?:' + one + '))*$', 's')
  for (const s of ['opposite', 'older', 'nothing', 'fog']) {
    for (const para of tellDraftOf(hqState(s)).draft.split('\n\n')) {
      assert.match(para, paragraph, `${s}: a sentence that is not in the copy file: ${para}`)
    }
  }
  /* And the matcher is not vacuous: a sentence of this test's own fails it. */
  assert.doesNotMatch('Your tests passed.', paragraph)
  const literal = readFileSync(join(here, 'tell-draft.js'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '')
  assert.doesNotMatch(literal, /'[A-Z][a-z]+ [a-z]+[^']*[.:]'/, 'a sentence literal in tell-draft.js')
})

test('T5: a stale proof that ran on THIS version is said as that — the message never calls it "another version", and never says nothing ran here', () => {
  const cov = coverage({ items: [item(1, { fresh: false, fresh_slug: 'applicability_needs_a_recheck' })], executions: [execution(1)], run_checks: sheet(1, 'passed', 4, 0) })
  const hq = hqOf(cov)
  assert.equal(hq.criteria[0].state, 'needs_recheck')
  const told = tellDraftOf(hq)
  assert.equal(told.kind, 'older')
  assert.ok(told.draft.includes(`The run that bears on it, ${CHECK}, ran on ${VERSION}, and what it checked has changed since`))
  assert.doesNotMatch(told.draft, /another version|Nothing on \S+ has run/)
})

test('T5: of two failing runs on this version, the message names the one linked last — by the time each carries, not by where the document lists it', () => {
  /* The coverage document lists a criterion's items by link id, not by time.
   * Run 2 is the later one and the only one the run sheet holds counts for;
   * it is served FIRST here. */
  const cov = coverage({
    items: [item(2, { observed: 'not_proven', failure_line: FAILING_LINE }), item(1, { observed: 'not_proven', failure_line: 'FAILED an older line' })],
    executions: [execution(1), execution(2)],
    run_checks: sheet(2, 'failed', 4, 1, FAILING_LINE),
  })
  const { draft } = tellDraftOf(hqOf(cov))
  assert.ok(draft.includes('1 failed, 4 passed'), 'the later run, with its own counts')
  assert.ok(draft.includes(FAILING_LINE))
  assert.ok(!draft.includes('FAILED an older line'))
})

/* ── 4. the documents `thruline ui` actually served ────────────────────── */

/* A3 g2 fix round r1. The verifier's real-product walk failed this card: after
 * a failing Check-it-again run the screen offered "The only run that bears on
 * it … was on another version … Nothing on <version> has run for it", with no
 * counts and no failing line. Every test above passed, because every fixture
 * above keeps the earlier passing run `applies: true` with no `fresh` field.
 * The server never serves that once the code has changed: the earlier pass
 * comes back `applies: false, fresh: false` beside the failing run
 * `applies: true`, and the old code let that stale pass decide the message.
 *
 * So these tests read the documents the server served in a real walk
 * (`walk-capture.fixture.json`, masked; its `about` says how it was made),
 * through `readChange` itself — the browser's read path, with `fetch`
 * answered from the capture — rather than a model shape written by hand. */

const CAPTURE = JSON.parse(readFileSync(join(here, 'walk-capture.fixture.json'), 'utf8'))
const CAPTURED_CLAIM = CAPTURE.moments.after_pass[`/api/changes/${CAPTURE.change_id}`].body.change.accepted_scope.criteria[0]

/** `readChange` over one captured moment. Every GET is answered with what the server served then; a read the capture does not hold is recorded, so a test can say the screen read nothing it was not given. */
async function readCaptured(moment) {
  const reads = CAPTURE.moments[moment]
  const missed = []
  const real = globalThis.fetch
  globalThis.fetch = async (url) => {
    const r = reads[String(url)]
    const headers = { get: (h) => (!r ? null : h === 'X-ThruLine-Exit' ? r.exit : h === 'X-ThruLine-Projection' ? r.projection : null) }
    if (!r) { missed.push(String(url)); return { ok: false, status: 404, headers, text: async () => '' } }
    return { ok: r.status >= 200 && r.status < 300, status: r.status, headers, text: async () => JSON.stringify(r.body) }
  }
  try {
    const read = await readChange(CAPTURE.change_id)
    return { ...read, missed }
  } finally {
    globalThis.fetch = real
  }
}

/** The run sheet's typed record of the latest run, as served at one moment. */
const servedLast = (moment) => CAPTURE.moments[moment][`/api/changes/${CAPTURE.change_id}/run`].body.checks[0].last

test('T5 done-when 1, on the documents the server served (captured walk): after a failing Check-it-again run the message carries that run\'s version, counts and failing line, and never says nothing ran on this version', async () => {
  const pass = await readCaptured('after_pass')
  const fail = await readCaptured('after_fail')
  for (const r of [pass, fail]) {
    assert.equal(r.status, 'ready')
    assert.deepEqual(r.missed, [], 'the Change screen read something the capture does not hold')
  }
  /* What the record served, typed — the values the message must carry. */
  const last = servedLast('after_fail')
  assert.equal(last.outcome, 'failed')
  assert.equal(last.tests_failed, 1)
  assert.equal(last.tests_passed, 1)
  assert.ok(last.failure_line, 'the run recorded the line it failed on')
  /* The shape the hand-written fixtures above never had: the earlier pass is
   * about another version and no longer applies; the failing run is on this one. */
  const items = fail.model.hq.criteria[0].items
  assert.ok(items.some((i) => i.state === 'proven' && i.applies === false && i.fresh === false), 'the earlier pass, stale and about another version')
  assert.ok(items.some((i) => i.state === 'not_proven' && i.applies === true), 'the failing run, on this version')

  const version = fail.model.hq.version.short
  assert.ok(version, 'the Change names its version')
  const before = offered(renderScreen({ model: pass.model, stage: 'verify', onCopy: () => {} }))
  const after = offered(renderScreen({ model: fail.model, stage: 'verify', onCopy: () => {} }))
  assert.ok(after, 'after a failing re-run the Change screen offers a message for the agent')
  assert.notEqual(after, before, 'the draft did not change when the verdict did')
  assert.ok(after.includes(`On the current version, ${version}, a run shows the opposite`), 'the failing run, placed on this version')
  assert.ok(after.includes(`${last.tests_failed} failed, ${last.tests_passed} passed`), 'tests_failed and tests_passed, as served')
  assert.ok(after.includes(last.failure_line), 'the recorded failing line, as served')
  assert.doesNotMatch(after, /another version|Nothing on \S+ has run/, 'the message denies the failing run the record holds')
  const stripped = after.split(`"${CAPTURED_CLAIM}"`).join('').split(`${last.tests_failed} failed, ${last.tests_passed} passed`).join('')
  assert.doesNotMatch(stripped, /\bpass(ed|es)?\b/i, 'a sentence says the tests passed after a failing re-run')

  /* The route a fresh install opens (Brief) offers the same text. */
  const brief = offered(renderRoute({
    model: fail.model, stage: 'verify', altitude: 'brief', register: 'manager',
    onGo: () => {}, onCopy: () => {}, onReload: () => {}, onAltitude: () => {}, onRegister: () => {},
  }))
  assert.equal(brief, after)
})

test('T5 done-when 2, on the captured documents: the passing re-run moves the draft again — no message, because the line is proven on this version as the engine\'s own readiness says — and nothing says no run happened there', async () => {
  const fail = await readCaptured('after_fail')
  const repass = await readCaptured('after_repass')
  assert.deepEqual(repass.missed, [])
  const readiness = CAPTURE.moments.after_repass[`/api/changes/${CAPTURE.change_id}/coverage`].body.readiness
  assert.equal(readiness.ready, true, 'the engine served: ready, for this version')
  assert.equal(servedLast('after_repass').outcome, 'passed')
  const version = repass.model.hq.version.short
  const told = tellDraftOf(repass.model.hq)
  assert.equal(told.kind, 'covered')
  assert.equal(told.draft, null)
  assert.notEqual(told.kind, tellDraftOf(fail.model.hq).kind, 'the verdict moved and the kind did not')
  const html = renderScreen({ model: repass.model, stage: 'verify', onCopy: () => {} })
  assert.equal(offered(html), null)
  assert.ok(html.includes(unescapeInverse(T.covered.replace('{version}', version))), 'the covered line says why no message is offered')
  assert.doesNotMatch(unescape(html), /Nothing on \S+ has run/)
  /* Same documents, same bytes. */
  assert.equal(JSON.stringify(tellDraftOf((await readCaptured('after_fail')).model.hq)), JSON.stringify(tellDraftOf(fail.model.hq)))
})

test('T5 r1, on the captured documents: after the passing re-run nothing on the Change screen still says the line needs a re-check — the word, the Proven line and the freshness line agree', async () => {
  const repass = await readCaptured('after_repass')
  const [line] = repass.model.hq.criteria
  assert.equal(line.state, 'proven')
  assert.ok(!line.freshness || line.freshness.fresh !== false, 'the line still carries the stale proof\'s freshness')
  for (const altitude of ['brief', 'workbench']) {
    const text = unescape(renderRoute({
      model: repass.model, stage: 'verify', altitude, register: 'manager',
      onGo: () => {}, onCopy: () => {}, onReload: () => {}, onAltitude: () => {}, onRegister: () => {},
    }).replace(/<[^>]+>/g, ' '))
    assert.doesNotMatch(text, /needs a re-check/i, `${altitude}: "needs a re-check" beside the re-check that passed`)
  }
  /* And after the failing run it still does: the stale proof leads, as the engine's readiness reason does. */
  const fail = await readCaptured('after_fail')
  assert.equal(fail.model.hq.criteria[0].state, 'needs_recheck')
  assert.equal(fail.model.hq.criteria[0].freshness.fresh, false)
})
