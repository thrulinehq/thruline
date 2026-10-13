import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* T6 (report T6, line 361: "Three-way copy derived from the chain") — what
 * happened after a line was raised speaks in exactly three ways, from typed
 * fields of the served coverage document, and says the same thing on the
 * Change screen, the page a person takes away and the exported file.
 *
 * Done when (predeclared on the card; no F-row names T6 directly):
 *  1. The sentence is one of three kinds, chosen only from typed fields:
 *     (a) no run on the current version since it was raised → "Still stands:
 *         nothing has run on {version} since this was raised."
 *     (b) a run on the current version shows the opposite → "Still stands:
 *         the run on {version} … shows {failed} failed, {passed} passed."
 *     (c) a run on the current version holds → the full Proven line.
 *  2. "nothing has checked it" / "nothing has been checked a second time" can
 *     never be produced when `coverage.executions` holds a second run of the
 *     same check.
 *  3. The take-away (page and exported file) and the Change screen say the
 *     same kind for the same fields.
 *  4. The "Told" rung is drawn off with its reason visible, never inferred.
 *
 * Every fixture here is synthetic. No string below came off a real machine.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-repairs-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeScreen, readChange } from './change-workspace.jsx';
  import { PacketPage, readPacket } from './packet.jsx';
  import { buildChangeView } from './change-view.js';
  import { buildCoverageView, evidenceEntries, unlinkedCompletedRun } from './coverage-view.js';
  import { deriveChangeModel, repairHistoryOf } from './change-model.js';
  import { buildPacketView } from './packet-view.js';
  import { renderExport } from './packet-export.js';
  import { HQ_COPY } from './hq-copy.js';
  export { buildChangeView, buildCoverageView, evidenceEntries, unlinkedCompletedRun, deriveChangeModel, repairHistoryOf, buildPacketView, renderExport, HQ_COPY, readChange, readPacket };
  export const renderScreen = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderPacket = props => renderToStaticMarkup(React.createElement(PacketPage, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'repairs.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const { buildChangeView, buildCoverageView, evidenceEntries, unlinkedCompletedRun, deriveChangeModel, buildPacketView, renderExport, HQ_COPY, renderScreen, renderPacket, readChange, readPacket } = M
const R = HQ_COPY.change.repairs
const EMPTY = HQ_COPY.change.packet.sections.repairs.empty

const CLAIM = 'Tests pass.'
const REVISION = 'e71c5b04d2a9f3c1b8e6d7a5c4b3a2918f7e6d5c'
const VERSION = REVISION.slice(0, 7)
const CHECK = 'bash:synthetic-repo:pytest -q tests/test_totals.py checkout/'

const CHANGE = {
  id: 'cccccccc-0000-4000-8000-00000000c061',
  project_id: 'cccccccc-0000-4000-8000-00000000p061',
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
    version: 1, bound_at: '2026-01-02T10:00:00Z', repository_identity: 'repo-synthetic-0061',
    base_revision: REVISION, snapshot_id: null, workspace_identity: 'workspace-synthetic-0061',
  },
  associations: [], examination: 'none_yet', decision: 'none_yet',
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
}

const item = (n, over = {}) => ({
  link_id: 'cce_synthetic_06' + n, kind: 'check_run', evidence_id: 'evb_synthetic_06' + n,
  session_id: 'cli:check:2026-01-02T10:0' + n + ':00Z', basis: 'named', basis_slug: 'basis_named',
  name: CHECK, observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies',
  asserted_at: '2026-01-02T10:0' + n + ':00Z', execution: 'completed', execution_slug: 'execution_completed',
  failure_line: null, evidence_class: null, evidence_class_slug: null,
  ...over,
})
const execution = (n, over = {}) => ({
  id: 'rex_synthetic_06' + n, run_key: 'run_synthetic_06' + n, binding_id: 'evb_synthetic_06' + n,
  name: CHECK, state: 'completed', state_slug: 'execution_completed', settles: true,
  recorded_at: '2026-01-02T10:0' + n + ':30Z', ...over,
})
const sheet = (n, outcome, passed, failed) => [{ id: 'pytest', program: 'pytest', last: { run: 'run_synthetic_06' + n, outcome, tests_passed: passed, tests_failed: failed } }]
const coverage = (items, executions, runChecks) => ({
  schema_version: 1, change_id: CHANGE.id, scope_version: 1, link_rows: items.length, executions,
  criteria: [{ index: 1, text: CLAIM, state: items.length ? 'evidence' : 'none', state_slug: 'coverage_evidence', items, removed: [] }],
  run_checks: runChecks,
})

/* The fixtures, one per kind. */
const CASES = {
  /* (a) the only run was about another version. */
  a_other_version: coverage([item(1, { applies: false, applies_slug: 'inputs_other_app' })], [execution(1)], sheet(1, 'passed', 4, 0)),
  /* (a) what ran on this version settled nothing. */
  a_settled_nothing: coverage([item(1, { observed: 'couldnt', bears: false, execution: 'cancelled' })], [execution(1, { state: 'cancelled', settles: false })], null),
  /* (b) a pass, then Check it again fails on the same version: 1 failed, 4 passed. */
  b_rerun_fails: coverage(
    [item(1), item(2, { observed: 'not_proven', failure_line: 'FAILED test_total - assert 90 == 95' })],
    [execution(1), execution(2)],
    sheet(2, 'failed', 4, 1),
  ),
  /* (c) a failure, then Check it again holds — two runs of the SAME check. */
  c_rerun_holds: coverage([item(1, { applies: false, observed: 'not_proven' }), item(2)], [execution(1), execution(2)], sheet(2, 'passed', 5, 0)),
  /* nothing run at all. */
  none: coverage([], [], null),
}

const hqOf = (cov, change = CHANGE) => deriveChangeModel(change, cov, null, null, [])
const screenModel = (cov) => {
  const model = buildChangeView({ change: CHANGE, proposals: null, work: [], evidence: evidenceEntries(cov), unlinkedRun: unlinkedCompletedRun(cov) })
  model.coverage = buildCoverageView(cov)
  model.hq = hqOf(cov)
  return model
}
const packetModel = (cov) => {
  const hq = hqOf(cov)
  return { ...buildPacketView({ change: CHANGE, proposals: null, work: [], coverage: cov, evidence: [], hq }), hq }
}
const plain = (html) => html.replace(/<[^>]+>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ')
const screen = (cov) => plain(renderScreen({ model: screenModel(cov), stage: 'verify' }))
const page = (cov) => plain(renderPacket({ model: packetModel(cov), notes: [] }))
const exported = (cov) => renderExport(hqOf(cov).packet, null, { format: 'md' }).text

/* ── 1. three kinds, from typed fields ─────────────────────────────────── */

test('T6 done-when 1: (a) no run on the current version since it was raised', () => {
  const [row] = hqOf(CASES.a_other_version).repairs
  assert.equal(row.kind, 'a')
  assert.equal(row.sentence, `Still stands: nothing has run on ${VERSION} since this was raised.`)
  const [settled] = hqOf(CASES.a_settled_nothing).repairs
  assert.equal(settled.kind, 'a', 'a run that settled nothing is never a hold')
  assert.equal(settled.sentence, `Still stands: what ran on ${VERSION} settled nothing.`)
})

test('T6 done-when 1: (b) a run on the current version shows the opposite — failures first, counts from tests_failed / tests_passed', () => {
  const [row] = hqOf(CASES.b_rerun_fails).repairs
  assert.equal(row.kind, 'b')
  assert.equal(row.sentence, `Still stands: the run on ${VERSION} shows 1 failed, 4 passed.`)
  assert.equal(row.failed, 1)
  assert.equal(row.passed, 4)
  assert.doesNotMatch(row.sentence, /Failed · \d+ of \d+/)
})

test('T6 done-when 1: (c) a run on the current version holds — the full Proven line, never "cleared" or "resolved"', () => {
  const hq = hqOf(CASES.c_rerun_holds)
  const [row] = hq.repairs
  assert.equal(row.kind, 'c')
  assert.equal(row.sentence, `Proven: ${CHECK} · version ${VERSION} · checkout/**`)
  for (const s of Object.values(R)) assert.doesNotMatch(s, /\bclear(ed|s)?\b|\bresolved?\b/i, `banned word in: ${s}`)
})

test('T6: counts come from typed fields only — none served is said as none, never as zero', () => {
  const uncounted = coverage(
    [item(1), item(2, { observed: 'not_proven' })],
    [execution(1), execution(2)],
    null,
  )
  const [row] = hqOf(uncounted).repairs
  assert.equal(row.kind, 'b')
  assert.equal(row.sentence, `Still stands: the run on ${VERSION} shows the opposite, and the record holds no count of its tests.`)
  assert.equal(row.failed, null)
  /* A count present on only one half is not a count. */
  const half = coverage([item(2, { observed: 'not_proven' })], [execution(2)], [{ last: { run: 'run_synthetic_062', tests_failed: 1, tests_passed: null } }])
  assert.equal(hqOf(half).repairs[0].failed, null)
})

test('T6: a run on another version never yields kind (c)', () => {
  const other = coverage([item(1, { applies: false, applies_slug: 'inputs_other_app' })], [execution(1)], sheet(1, 'passed', 9, 0))
  const [row] = hqOf(other).repairs
  assert.notEqual(row.kind, 'c')
  assert.doesNotMatch(row.sentence, /^Proven/)
})

test('T6: with no version named, no run is placed on "this" version', () => {
  const [row] = hqOf(CASES.b_rerun_fails, { ...CHANGE, implementation: null }).repairs
  assert.equal(row.kind, 'unbound')
  assert.equal(row.sentence, R.unbound)
})

/* ── 2. the property the prototype broke ───────────────────────────────── */

const NEVER = [/nothing has checked it/i, /nothing has been checked a second time/i]

for (const name of ['b_rerun_fails', 'c_rerun_holds']) {
  test(`T6 done-when 2 (${name}): with a second run of the same check, "nothing has been checked a second time" is produced nowhere`, () => {
    const cov = CASES[name]
    const runs = cov.executions.filter((e) => e.name === CHECK)
    assert.ok(runs.length >= 2, 'the fixture holds a second run of one check')
    for (const [surface, text] of [['Change screen', screen(cov)], ['packet page', page(cov)], ['exported file', exported(cov)]]) {
      for (const re of NEVER) assert.doesNotMatch(text, re, `${surface} says ${re} under a second run`)
    }
  })
}

test('T6: with no run at all, the section says exactly that — the one case it is true', () => {
  assert.deepEqual(hqOf(CASES.none).repairs, [])
  assert.ok(page(CASES.none).includes(EMPTY))
  assert.doesNotMatch(screen(CASES.none), new RegExp(R.heading), 'no ladder over nothing')
})

test('T6: the page\'s repair section is filled exactly when a run exists', () => {
  const section = (cov) => buildPacketView({ change: CHANGE, proposals: null, work: [], coverage: cov, evidence: [], hq: hqOf(cov) }).sections.find((s) => s.id === 'repairs')
  for (const name of ['a_other_version', 'a_settled_nothing', 'b_rerun_fails', 'c_rerun_holds']) assert.equal(section(CASES[name]).filled, true, name)
  assert.equal(section(CASES.none).filled, false)
})

/* ── 3. one kind on every surface ──────────────────────────────────────── */

for (const name of ['a_other_version', 'a_settled_nothing', 'b_rerun_fails']) {
  test(`T6 done-when 3 (${name}): the Change screen, the packet page and the exported file say the same sentence for the same fields`, () => {
    const cov = CASES[name]
    const [row] = hqOf(cov).repairs
    assert.ok(screen(cov).includes(row.sentence), 'the Change screen')
    assert.ok(page(cov).includes(row.sentence), 'the packet page')
    assert.ok(exported(cov).includes(row.sentence), 'the exported file')
  })
}

test('T6 done-when 3 (c): the Proven line is on all three surfaces', () => {
  const cov = CASES.c_rerun_holds
  const [row] = hqOf(cov).repairs
  assert.ok(screen(cov).includes(row.sentence))
  assert.ok(page(cov).includes(row.sentence))
  /* In the file it is the statement's own line — one Proven line, not two. */
  assert.ok(exported(cov).includes(row.sentence))
})

/* ── 4. the Told rung ──────────────────────────────────────────────────── */

test('T6 done-when 4: "Told the agent" is drawn off with its reason, and never inferred', () => {
  const html = renderScreen({ model: screenModel(CASES.b_rerun_fails), stage: 'verify', onCopy: () => {} })
  const rung = /<li class="ws-repairs-rung ws-repairs-rung-off" data-off="true">([\s\S]*?)<\/li>/.exec(html)
  assert.ok(rung, 'the Told rung is drawn, off')
  const body = plain(rung[1])
  assert.ok(body.includes(R.told))
  assert.ok(body.includes(R.told_off), 'its reason is visible')
  /* No other rung claims the agent was told. */
  assert.doesNotMatch(plain(html).replace(R.told_off, ''), /\bwas told\b|\bhas been told\b/i)
  assert.ok(plain(html).includes(R.closing))
})

test('T6 (a): "no longer applies" is said only of a run on THIS version that went stale — never of a run on another version beside one that ran here', () => {
  /* A run on this version, stale: what ran here no longer applies. */
  const staleHere = coverage([item(1, { fresh: false, fresh_slug: 'applicability_needs_a_recheck' })], [execution(1)], sheet(1, 'passed', 4, 0))
  assert.equal(hqOf(staleHere).repairs[0].sentence, `Still stands: what ran on ${VERSION} no longer applies, because what it checked has changed since.`)
  /* A stale run on ANOTHER version, and a run here that bears and settled
   * nothing (Recorded): the model leads with "Needs a re-check", and what ran
   * HERE did not go stale, so it is not said to. */
  const elsewhere = coverage(
    [item(1, { applies: false, applies_slug: 'inputs_other_revision', fresh: false, fresh_slug: 'applicability_needs_a_recheck' }), item(2, { observed: 'loose' })],
    [execution(1), execution(2)],
    null,
  )
  const hq = hqOf(elsewhere)
  assert.equal(hq.criteria[0].state, 'needs_recheck')
  const [row] = hq.repairs
  assert.equal(row.kind, 'a')
  assert.equal(row.sentence, `Still stands: what ran on ${VERSION} settled nothing.`)
})

/* ── 5. the documents `thruline ui` actually served ────────────────────── */

/* A3 g2 fix round r1. The verifier's real-product walk failed done-when 1(b):
 * after a passing Check-it-again run, a code change and a failing one, the
 * ladder in Workbench > Verify, the take-away page and the file all said "Still
 * stands: what ran on <version> no longer applies, because what it checked
 * has changed since" — false about the failing run, which does apply. Every
 * case above passed, because every fixture above keeps the earlier run
 * `applies: true` with no `fresh` field, which the server never serves once
 * the code has changed. These tests read the server's own documents
 * (`walk-capture.fixture.json`, masked; its `about` says how it was made)
 * through `readChange` and `readPacket`, with `fetch` answered from the
 * capture, and check the three surfaces the card names. */

const CAPTURE = JSON.parse(readFileSync(join(here, 'walk-capture.fixture.json'), 'utf8'))
const CHANGE_URL = `/api/changes/${CAPTURE.change_id}`

/** The Change screen's read and the take-away page's read of one captured moment. */
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
    return { screen: await readChange(CAPTURE.change_id), packet: await readPacket(CAPTURE.change_id), missed }
  } finally {
    globalThis.fetch = real
  }
}

/** The three surfaces, as text. */
const surfaces = ({ screen: s, packet: p }) => [
  ['Change screen (Verify)', plain(renderScreen({ model: s.model, stage: 'verify', onCopy: () => {} }))],
  ['take-away page', plain(renderPacket({ model: p.model, notes: [], records: p.records }))],
  ['exported file', renderExport(p.model.hq.packet, p.records, { format: 'md' }).text],
]

test('T6 done-when 1(b), on the documents the server served (captured walk): after a failing Check-it-again run the ladder, the take-away page and the file say "shows 1 failed, 1 passed", and never "no longer applies"', async () => {
  const read = await readCaptured('after_fail')
  assert.deepEqual(read.missed, [], 'a read the capture does not hold')
  assert.equal(read.screen.status, 'ready')
  assert.equal(read.packet.status, 'ready')
  /* The shape the hand-written fixtures above never had. */
  const items = read.screen.model.hq.criteria[0].items
  assert.ok(items.some((i) => i.state === 'proven' && i.applies === false && i.fresh === false), 'the earlier pass, stale and about another version')
  assert.ok(items.some((i) => i.state === 'not_proven' && i.applies === true), 'the failing run, on this version')
  const last = CAPTURE.moments.after_fail[`${CHANGE_URL}/run`].body.checks[0].last
  assert.equal(last.outcome, 'failed')
  const version = read.screen.model.hq.version.short
  const [row] = read.screen.model.hq.repairs
  assert.equal(row.kind, 'b')
  assert.equal(row.failed, last.tests_failed)
  assert.equal(row.passed, last.tests_passed)
  /* The predeclared form, failures first, with the served counts. */
  assert.equal(row.sentence, `Still stands: the run on ${version} shows 1 failed, 1 passed.`)
  assert.equal(JSON.stringify(read.packet.model.hq.repairs), JSON.stringify(read.screen.model.hq.repairs), 'the page read the same rows')
  for (const [surface, text] of surfaces(read)) {
    assert.ok(text.includes(row.sentence), `${surface} does not say ${row.sentence}`)
    assert.doesNotMatch(text, /no longer applies, because what it checked has changed since/, `${surface} says the failing run no longer applies`)
    for (const re of NEVER) assert.doesNotMatch(text, re, `${surface} says ${re} under a second run`)
  }
})

test('T6 done-when 1(c), on the captured documents: after the passing re-run the three surfaces carry the full Proven line for this version, as the engine\'s readiness does, and never "no longer applies"', async () => {
  const read = await readCaptured('after_repass')
  assert.deepEqual(read.missed, [])
  const readiness = CAPTURE.moments.after_repass[`${CHANGE_URL}/coverage`].body.readiness
  assert.equal(readiness.ready, true, 'the engine served: ready, for this version')
  const hq = read.screen.model.hq
  const version = hq.version.short
  const passHere = hq.criteria[0].items.find((i) => i.state === 'proven' && i.applies === true)
  assert.ok(passHere, 'the passing re-run, on this version')
  assert.equal(hq.criteria[0].state, 'proven', 'the line\'s own word, beside the Proven line')
  const [row] = hq.repairs
  assert.equal(row.kind, 'c')
  assert.equal(row.sentence, `Proven: ${passHere.name} · version ${version} · src/**`)
  for (const [surface, text] of surfaces(read)) {
    assert.ok(text.includes(row.sentence), `${surface} does not carry the Proven line`)
    assert.doesNotMatch(text, /no longer applies, because what it checked has changed since/, `${surface} says the passing re-run no longer applies`)
    for (const re of NEVER) assert.doesNotMatch(text, re)
  }
})
