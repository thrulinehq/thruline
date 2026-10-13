/* APPLIES-VS-MOVED (the step-7 seam) — "Still applies — src/** unchanged since
 * …" never stands beside "The code has changed since …" without saying what
 * each is measured against.
 *
 * Each is true in its own frame. The applicability line compares RECORDED
 * versions; the git line reads the live folder. A person reads "unchanged" as
 * "the code has not changed". So while the served Change carries `code_moved`,
 * the applicability line and the agent section name their frame, on the Change
 * screen, the whole-story page and all three exported files — and with no
 * `code_moved` every byte is what it was.
 *
 * Every fixture is synthetic; the served shapes are the ones the Change,
 * coverage and applicability routes serve.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-applies-moved-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeBrief } from './change-workspace.jsx';
  import { PacketPage } from './packet.jsx';
  import { buildPacketView } from './packet-view.js';
  import { buildChangeView } from './change-view.js';
  import { deriveChangeModel, freshnessOf } from './change-model.js';
  import { evidenceEntries, unlinkedCompletedRun } from './coverage-view.js';
  import { tellDraftOf } from './tell-draft.js';
  import { renderExport } from './packet-export.js';
  import { HQ_COPY } from './hq-copy.js';
  export { deriveChangeModel, freshnessOf, buildChangeView, buildPacketView, evidenceEntries, unlinkedCompletedRun, tellDraftOf, renderExport, HQ_COPY };
  export const renderBrief = props => renderToStaticMarkup(React.createElement(ChangeBrief, props));
  export const renderPage = props => renderToStaticMarkup(React.createElement(PacketPage, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'applies-moved.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const LINE = 'When the cart is empty, the total shows zero.'
const RUN_NAME = 'cargo test'
const MOVED = {
  recorded: 'b03f055f9a3100e4c30aa615af0ba88766f5889b',
  now: '62d575b535356031ada53482bb0dfcc56d5dfb06',
  changed_paths: ['src/lib.rs'],
  commits: 1,
  uncommitted_compared: false,
}

const change = (extra = {}) => ({
  id: 'ch1',
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Empty cart total' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1, criteria: [LINE], allowed_scope: ['src/'], check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet',
  decisions: [],
  ...extra,
})

const IDENTITY = {
  id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '',
  outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null,
  evidence_name: null, linked_by: null, waiver: null, waivable: false,
}

/** The walk's Change after a person linked the passing run and it was checked again: proven, still applies, ready. */
const coverage = () => ({
  scope_version: 1,
  link_rows: 1,
  execution_sessions: ['s1'],
  criteria: [{
    index: 1, text: LINE, state: 'evidence', removed: [],
    items: [{
      link_id: 'l1', evidence_id: 'eb_test', session_id: 's1', basis: 'manual', basis_slug: 'link_basis_manual',
      actor: 'walker', asserted_at: '2026-09-22T11:00:00Z', kind: 'check_run', name: RUN_NAME,
      observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies',
      fresh: true, fresh_slug: 'applicability_still_applies', execution: 'completed', execution_slug: 'execution_completed',
    }],
  }],
  candidates: [{ kind: 'check_run', evidence_id: 'eb_test', session_id: 's1', name: RUN_NAME, observed: 'proven', bears: true, execution: 'completed', execution_slug: 'execution_completed' }],
  executions: [],
  readiness: {
    schema_version: 1, change_id: 'ch1', ready: true, with_exceptions: false, scope_version: 1,
    target_version: 'e71c5b0aaaa', headline_slug: 'readiness_ready',
    obligations: [IDENTITY, {
      id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: LINE,
      outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null,
      evidence_name: RUN_NAME, linked_by: 'walker', waiver: null, waivable: true,
    }],
    rests_on: [{ obligation_id: 'criterion:1', text: LINE, evidence_name: RUN_NAME, linked_by: 'walker' }],
    missing: [], waived: [], not_applicable: [],
  },
})

const applicability = () => ({
  items: [{ link_id: 'l1', criterion_index: 1, applies: true, checked_paths: ['src/**'], old_snapshot: 'd41f8a0aaaa', new_snapshot: 'e71c5b0aaaa', reason_paths: [] }],
})

const hqOf = (moved, { cov = coverage(), app = applicability() } = {}) => M.deriveChangeModel(change(moved ? { code_moved: moved } : {}), cov, app, null, [])
const briefHtml = (moved, opts) => {
  const c = change(moved ? { code_moved: moved } : {})
  const cov = (opts && opts.cov) || coverage()
  const model = M.buildChangeView({ change: c, proposals: null, work: [], evidence: M.evidenceEntries(cov), unlinkedRun: M.unlinkedCompletedRun(cov) })
  model.hq = hqOf(moved, opts)
  return M.renderBrief({ model, onCopy: () => {}, onReload: () => {}, notes: [], register: 'manager', onExplore: () => {}, onDoor: () => {} })
}
const pageHtml = (moved) => {
  const c = change(moved ? { code_moved: moved } : {})
  const cov = coverage()
  return M.renderPage({ model: M.buildPacketView({ change: c, coverage: cov, work: [], hq: hqOf(moved) }), onCopy: () => {}, records: null })
}
const exportsOf = (moved) => Object.fromEntries(['html', 'md', 'json'].map((format) => [format, M.renderExport(hqOf(moved).packet, null, { format }).text]))
const text = (html) => html.replace(/<[^>]+>/g, ' ').replace(/&#x27;/g, "'").replace(/&#39;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ').trim()
const sha = (s) => createHash('sha256').update(s).digest('hex')

const BASE_SENTENCE = 'Still applies — src/** unchanged since d41f8a0.'
const BASE_COVERED = 'Nothing to ask the agent for: every line that counts as done is proven on e71c5b0.'
const CLAIMS_UNCHANGED = /unchanged|still applies —|has not changed|hasn't changed|nothing (it depends on )?has changed/i

/* ── with no code_moved, every byte is what it was ────────────────────── */

test('with no code_moved the applicability line and the agent section are exactly what they were', () => {
  const hq = hqOf(null)
  assert.equal(hq.criteria[0].freshness.sentence, BASE_SENTENCE)
  assert.equal(hq.inspector.shows.freshness.sentence, BASE_SENTENCE)
  assert.equal(M.tellDraftOf(hq).covered, BASE_COVERED)
  assert.equal(hq.criteria[0].freshness.carriedTechnical, 'carried forward: src/** unchanged d41f8a0→e71c5b0')
  const brief = text(briefHtml(null))
  assert.ok(brief.includes(BASE_SENTENCE) && brief.includes(BASE_COVERED))
})

/* Pinned at cf8ca110 (the base of this lineage): the three files, for the Change with no code_moved. */
const EXPORT_AT_BASE = {
  html: 'b81c913776532e529072c9a2c19789b5893e9f5c4848ce842a207ac6fff0d0da',
  md: '4c50389969e687f786cf9199a6d10641579eb37e339b595add45f12b612b74ab',
  json: 'd9e38aec99f26475a663199165200843adb6b6ed18fabd715ab02ba5907ebdab',
}
/* MERGE-SEAM-C12 (D-050) — the union with QUAL-F2-R3 (cloud/qual-f2-r2 lineage, merged after this one): every JSON export
 * now carries one more key, `"accepted_after_work": null` when the record does not show an acceptance after the work. The
 * base pin above stays exactly as it was, and is applied to the JSON with that one line taken out; the JSON as it is now has
 * its own pin. html and md are unchanged and keep their base pins. */
const QUAL_F2_KEY = /\n *"accepted_after_work": null,/
const JSON_WITH_QUAL_F2_KEY = '6960e35978f862a8dec8d63c82eef67741024275350c048716e18cdf7b1345ac'
test('with no code_moved the three exported files are byte-identical to base', () => {
  const files = exportsOf(null)
  for (const format of ['html', 'md', 'json']) {
    assert.ok(files[format].includes('unchanged since d41f8a0'), `${format} keeps its sentence`)
    if (format === 'json') {
      assert.equal(files.json.match(new RegExp(QUAL_F2_KEY.source, 'g')).length, 1, 'json carries the one QUAL-F2-R3 key exactly once')
      assert.equal(sha(files.json), JSON_WITH_QUAL_F2_KEY, 'json differs from its pin')
      assert.equal(sha(files.json.replace(QUAL_F2_KEY, '')), EXPORT_AT_BASE.json, 'json differs from base by more than the one QUAL-F2-R3 key')
    } else {
      assert.equal(sha(files[format]), EXPORT_AT_BASE[format], `${format} differs from base`)
    }
  }
})

/* ── while code_moved is present, the frame is named ──────────────────── */

const FRAME = /as recorded/
const CHANGED = /code has changed since/

test('while code_moved is present the applicability line names its frame and does not say "unchanged"', () => {
  const hq = hqOf(MOVED)
  for (const s of [hq.criteria[0].freshness.sentence, hq.inspector.shows.freshness.sentence]) {
    assert.match(s, FRAME)
    assert.match(s, CHANGED)
    assert.ok(s.includes('e71c5b0'), 'it names the version it is about: ' + s)
    assert.doesNotMatch(s, CLAIMS_UNCHANGED, s)
  }
  assert.doesNotMatch(hq.criteria[0].freshness.carriedTechnical, /unchanged(?! d41f8a0→e71c5b0)/)
  assert.match(hq.criteria[0].freshness.carriedTechnical, CHANGED)
})

test('while code_moved is present the agent section never says "nothing to ask" without the same frame', () => {
  const tell = M.tellDraftOf(hqOf(MOVED))
  assert.equal(tell.kind, 'covered')
  assert.match(tell.covered, /^Nothing to ask the agent for/)
  assert.match(tell.covered, /version as recorded/)
  assert.match(tell.covered, CHANGED)
})

test('no sentence on the Change screen says or implies the code is unchanged while code_moved is present', () => {
  const brief = text(briefHtml(MOVED))
  assert.ok(brief.includes('The code has changed since version e71c5b0 was recorded'), 'the git line is there')
  assert.doesNotMatch(brief, CLAIMS_UNCHANGED, (brief.match(new RegExp('.{40}' + CLAIMS_UNCHANGED.source + '.{40}', 'i')) || [''])[0])
})

test('no sentence on the whole-story page says or implies the code is unchanged while code_moved is present', () => {
  const page = text(pageHtml(MOVED))
  assert.ok(page.includes('The code has changed since version e71c5b0 was recorded'))
  assert.doesNotMatch(page, CLAIMS_UNCHANGED, (page.match(new RegExp('.{40}' + CLAIMS_UNCHANGED.source + '.{40}', 'i')) || [''])[0])
})

test('no exported file says or implies the code is unchanged while code_moved is present', () => {
  const files = exportsOf(MOVED)
  for (const format of ['html', 'md', 'json']) {
    const body = format === 'html' ? text(files[format]) : files[format]
    assert.match(body, CHANGED, `${format} says the code moved`)
    assert.doesNotMatch(body, CLAIMS_UNCHANGED, `${format}: ${(body.match(new RegExp('.{40}' + CLAIMS_UNCHANGED.source + '.{40}', 'i')) || [''])[0]}`)
  }
})

/* ── claims the same: nothing else moves ──────────────────────────────── */

test('readiness, counts, state words, standing and approval are byte-identical with and without the field', () => {
  const a = hqOf(null)
  const b = hqOf(MOVED)
  const pick = (hq) => ({
    readiness: hq.readiness, counts: hq.counts, state: hq.state, word: hq.word, decision: hq.decision, headline: hq.headline,
    criteria: hq.criteria.map((c) => ({ n: c.n, state: c.state, word: c.word, line: c.line, sentence: c.sentence, proven: c.proven })),
    inboxRow: { ...hq.inboxRow }, standing: hq.decision && hq.decision.standing, coverage: hq.coverage,
  })
  assert.deepEqual(pick(b), pick(a))
})

test('a moved code the record cannot read as two commits says nothing: the sentences are the base sentences', () => {
  for (const served of [{ ...MOVED, now: MOVED.recorded }, { ...MOVED, recorded: null }, {}, null]) {
    const hq = hqOf(served)
    assert.equal(hq.criteria[0].freshness.sentence, BASE_SENTENCE, JSON.stringify(served))
    assert.equal(M.tellDraftOf(hq).covered, BASE_COVERED)
  }
})

test('a line that needs a re-check keeps its own sentence beside a moved code (it never claimed "unchanged")', () => {
  const cov = coverage()
  cov.criteria[0].items[0] = { ...cov.criteria[0].items[0], fresh: false, fresh_slug: 'applicability_needs_a_recheck' }
  const moved = hqOf(MOVED, { cov })
  const plain = hqOf(null, { cov })
  assert.equal(moved.criteria[0].freshness.sentence, plain.criteria[0].freshness.sentence)
  assert.equal(moved.criteria[0].freshness.sentence, 'Proven on the previous version — needs a re-check.')
})

test('a line the record cannot name a path or version for is framed too, in its own words', () => {
  const hq = hqOf(MOVED, { app: { items: [{ link_id: 'l1', criterion_index: 1, applies: true }] } })
  const s = hq.criteria[0].freshness.sentence
  assert.match(s, FRAME)
  assert.match(s, CHANGED)
  assert.doesNotMatch(s, CLAIMS_UNCHANGED, s)
  assert.doesNotMatch(s, /[{}]|undefined|null/)
})

test('the agent section keeps its ask when a line is not proven, moved code or not', () => {
  const cov = coverage()
  cov.criteria[0] = { index: 1, text: LINE, state: 'none', items: [], removed: [] }
  cov.candidates = []
  const a = M.tellDraftOf(hqOf(null, { cov }))
  const b = M.tellDraftOf(hqOf(MOVED, { cov }))
  assert.equal(b.draft, a.draft)
  assert.equal(b.kind, a.kind)
})

/* ── self-attack ──────────────────────────────────────────────────────── */

test('two lines, one that still applies and one that needs a re-check, beside a moved code: only the first is framed', () => {
  const cov = coverage()
  cov.criteria.push({
    index: 2, text: 'The total shows the sum of the prices.', state: 'evidence', removed: [],
    items: [{ ...cov.criteria[0].items[0], link_id: 'l2', evidence_id: 'eb_two', fresh: false, fresh_slug: 'applicability_needs_a_recheck' }],
  })
  const c = change({ code_moved: MOVED })
  c.accepted_scope = { ...c.accepted_scope, criteria: [LINE, 'The total shows the sum of the prices.'] }
  const app = { items: [...applicability().items, { link_id: 'l2', criterion_index: 2, applies: true, checked_paths: ['src/**'], old_snapshot: 'd41f8a0aaaa', new_snapshot: 'e71c5b0aaaa', reason_paths: ['src/lib.rs'], reason_slug: 'applicability_reason_source_changed' }] }
  const hq = M.deriveChangeModel(c, cov, app, null, [])
  assert.match(hq.criteria[0].freshness.sentence, /as recorded/)
  assert.equal(hq.criteria[1].freshness.sentence, 'Proven on the previous version — needs a re-check.')
})

test('a line set aside by a person beside a moved code: the agent section keeps both frames', () => {
  const cov = coverage()
  cov.criteria.push({ index: 2, text: 'The total shows the sum of the prices.', state: 'none', items: [], removed: [] })
  cov.readiness.obligations.push({
    id: 'criterion:2', kind: 'criterion', criterion_index: 2, text: 'The total shows the sum of the prices.',
    outcome: 'waived', outcome_slug: 'waived', unsatisfied: null, unsatisfied_slug: null, evidence_name: null, linked_by: null, waivable: true,
    waiver: { actor: 'sam', authority: 'the accepted profile', rationale: 'Nobody checked this.' },
  })
  cov.readiness.with_exceptions = true
  cov.readiness.headline_slug = 'readiness_ready_with_exceptions'
  const c = change({ code_moved: MOVED })
  c.accepted_scope = { ...c.accepted_scope, criteria: [LINE, 'The total shows the sum of the prices.'] }
  const tell = M.tellDraftOf(M.deriveChangeModel(c, cov, applicability(), null, []))
  assert.equal(tell.kind, 'covered')
  assert.match(tell.covered, /set aside by a person/)
  assert.match(tell.covered, /version as recorded: the code has changed since\.$/)
})

test('many moved files and uncommitted edits compared: the framed line is the same one sentence, and nothing says unchanged', () => {
  const big = { ...MOVED, changed_paths: ['a.rs', 'b.rs', 'c.rs', 'd.rs', 'e.rs'], commits: 3, uncommitted_compared: true }
  const hq = hqOf(big)
  assert.match(hq.criteria[0].freshness.sentence, /as recorded/)
  const brief = text(briefHtml(big))
  assert.doesNotMatch(brief, CLAIMS_UNCHANGED)
})
