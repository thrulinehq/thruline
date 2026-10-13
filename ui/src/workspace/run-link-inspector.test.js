/* RUN-LINK-INSPECTOR (path steps 6 and 8, PW-F10) — after a passing run that no
 * line is linked to, the Inspector says what the top of the screen, the
 * criterion row, the Approve blocker and the run sheet already say, and names
 * the same next act: link it. The whole-story page's "link it" is a door on
 * screen and stays the plain sentence in the exported files.
 *
 * The served documents are the shape `change_readiness::compute` and
 * `change_evidence::coverage` serve for the walk's Change (the same fixtures
 * `run-link-truth.test.js` uses). Every string is synthetic.
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
const temporary = await mkdtemp(join(tmpdir(), 'thruline-run-link-inspector-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeBrief, Inspector } from './change-workspace.jsx';
  import { PacketPage } from './packet.jsx';
  import { buildPacketView } from './packet-view.js';
  import { runSheetModel } from './run-route.js';
  import { deriveChangeModel } from './change-model.js';
  import { buildChangeView } from './change-view.js';
  import { evidenceEntries, unlinkedCompletedRun } from './coverage-view.js';
  import { renderExport } from './packet-export.js';
  import { HQ_COPY } from './hq-copy.js';
  export { deriveChangeModel, buildChangeView, evidenceEntries, unlinkedCompletedRun, buildPacketView, runSheetModel, renderExport, HQ_COPY };
  export const renderBrief = props => renderToStaticMarkup(React.createElement(ChangeBrief, props));
  export const renderInspector = props => renderToStaticMarkup(React.createElement(Inspector, props));
  export const renderPage = props => renderToStaticMarkup(React.createElement(PacketPage, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'run-link-inspector.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const C = M.HQ_COPY.change
const UNLINKED = C.coverage.states.unlinked_run
const NOTHING_YET = C.model.inspector.shows_empty
const LINE = 'When the cart is empty, the total shows zero.'
const LINE_TWO = 'The total shows the sum of the prices.'
const RUN_NAME = 'cargo test'
const esc = (s) => s.replaceAll("'", '&#x27;').replaceAll('"', '&quot;')

const CHANGE = {
  id: 'ch1',
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Empty cart total' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1,
    criteria: [LINE],
    allowed_scope: ['src/'],
    check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet',
  decisions: [],
}
const CHANGE_TWO = { ...CHANGE, accepted_scope: { ...CHANGE.accepted_scope, criteria: [LINE, LINE_TWO] } }

const CANDIDATE = {
  kind: 'check_run', evidence_id: 'eb_test', session_id: 's1', name: RUN_NAME,
  observed: 'proven', bears: true, execution: 'completed', execution_slug: 'execution_completed',
}

const IDENTITY = {
  id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '',
  outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null,
  evidence_name: null, linked_by: null, waiver: null, waivable: false,
}
const UNCONFIRMED_IDENTITY = {
  ...IDENTITY, outcome: 'needs_evidence', outcome_slug: 'needs_evidence',
  unsatisfied: 'identity_unconfirmed', unsatisfied_slug: 'readiness_identity_unconfirmed',
}

const unlinkedObligation = (n, text) => ({
  id: 'criterion:' + n, kind: 'criterion', criterion_index: n, text,
  outcome: 'needs_evidence', outcome_slug: 'needs_evidence',
  unsatisfied: 'unlinked_run', unsatisfied_slug: 'readiness_unlinked_run',
  evidence_name: null, linked_by: null, waiver: null, waivable: true,
})

const readinessOfDoc = (obligations, missing) => ({
  schema_version: 1, change_id: 'ch1', ready: false, with_exceptions: false, scope_version: 1,
  target_version: 'e71c5b0aaaa', headline_slug: 'readiness_needs_evidence_one',
  obligations, rests_on: [], missing, waived: [], not_applicable: [],
})

/** Step 6 of the walk: the run passed, and no line is linked to it. */
const unlinkedCoverage = () => ({
  scope_version: 1,
  link_rows: 0,
  execution_sessions: ['s1'],
  criteria: [{ index: 1, text: LINE, state: 'none', items: [], removed: [] }],
  candidates: [CANDIDATE],
  executions: [],
  readiness: readinessOfDoc([IDENTITY, unlinkedObligation(1, LINE)], ['criterion:1']),
})

/** After a named person linked it and the Change was checked again. */
const linkedCoverage = () => ({
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
  candidates: [CANDIDATE],
  executions: [],
  readiness: {
    ...readinessOfDoc([IDENTITY, {
      id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: LINE,
      outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null,
      evidence_name: RUN_NAME, linked_by: 'walker', waiver: null, waivable: true,
    }], []),
    ready: true, headline_slug: 'readiness_ready',
    rests_on: [{ obligation_id: 'criterion:1', text: LINE, evidence_name: RUN_NAME, linked_by: 'walker' }],
  },
})

/** A Change with nothing run at all: the old reason, untouched. */
const nothingCoverage = () => {
  const doc = unlinkedCoverage()
  doc.candidates = []
  doc.readiness.obligations[1] = { ...doc.readiness.obligations[1], unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked' }
  return doc
}

/** Two lines, the second still unlinked; the first line set aside by a named person under the accepted authority. */
const waivedBesideUnlinkedCoverage = () => ({
  scope_version: 1,
  link_rows: 0,
  execution_sessions: ['s1'],
  criteria: [
    { index: 1, text: LINE, state: 'none', items: [], removed: [] },
    { index: 2, text: LINE_TWO, state: 'none', items: [], removed: [] },
  ],
  candidates: [CANDIDATE],
  executions: [],
  readiness: readinessOfDoc([
    IDENTITY,
    {
      id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: LINE,
      outcome: 'waived', outcome_slug: 'waived', unsatisfied: null, unsatisfied_slug: null,
      evidence_name: null, linked_by: null, waivable: true,
      waiver: { actor: 'sam', authority: 'the accepted profile', rationale: 'Nobody checked this.' },
    },
    unlinkedObligation(2, LINE_TWO),
  ], ['criterion:2']),
})

function hqOf(coverage, change = CHANGE) {
  return M.deriveChangeModel(change, coverage, null, null, [])
}
function modelOf(coverage, change = CHANGE) {
  const model = M.buildChangeView({ change, proposals: null, work: [], evidence: M.evidenceEntries(coverage), unlinkedRun: M.unlinkedCompletedRun(coverage) })
  model.hq = hqOf(coverage, change)
  return model
}
const noop = () => {}
const inspectorHtml = (coverage, change = CHANGE) => M.renderInspector({ hq: hqOf(coverage, change), cards: [], onCopy: noop, onDecided: noop, onRecheck: noop, onDoor: noop, licence: null })
const briefHtml = (coverage, change = CHANGE) => M.renderBrief({ model: modelOf(coverage, change), onCopy: noop, onReload: noop, notes: [], register: 'manager', onExplore: noop, onDoor: noop })
const blockOf = (html, id) => {
  const at = html.indexOf(id)
  assert.ok(at >= 0, `the markup has ${id}`)
  const start = html.lastIndexOf('<section', at)
  const end = html.indexOf('</section>', at)
  return html.slice(start, end)
}
const shows = (html) => blockOf(html, 'aria-labelledby="ws-inspector-h-shows"')
const settle = (html) => blockOf(html, 'aria-labelledby="ws-inspector-h-settle"')
const action = (html) => blockOf(html, 'aria-labelledby="ws-inspector-h-action"')
const text = (html) => html.replace(/<[^>]+>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ').trim()

const RUN_DOOR_LABEL = (line) => M.HQ_COPY.run.outcomes.passed.unlinked_door.replace('{line}', line)

const passedSheet = {
  schema_version: 1, change: 'ch1', project_folder: '~/shop',
  permission: { granted_by: 'walker', granted_at: '2026-09-23T10:00:00Z' },
  checks: [{ id: 'cargo-test', program: 'cargo', category: 'tests', command: 'cargo test',
    last: { run: 'r1', state: 'completed', outcome: 'passed', exit_code: 0, tests_passed: 2, tests_failed: 0, suites: 1, recorded_at: '2026-09-23T10:05:00Z', settles: true } }],
  environments: [{ id: 'hq', kind: 'hq', folder: null, has: ['cargo'] }],
  not_runnable_here: [], remembered_environment: null, not_started: null, actor: 'walker', job: null,
}

/* ── the Inspector's three blocks, after a passing run nobody linked ──── */

test('"What the record shows" says the line\'s unlinked-run sentence, not "Nothing in the record bears on this line yet."', () => {
  const inspector = hqOf(unlinkedCoverage()).inspector
  assert.equal(inspector.shows.unlinked && inspector.shows.unlinked.sentence, UNLINKED, 'the model carries the criterion row\'s own sentence')
  const block = text(shows(inspectorHtml(unlinkedCoverage())))
  assert.ok(block.includes(text(esc(UNLINKED))), `the block says it: ${block}`)
  assert.ok(!block.includes(NOTHING_YET), `the block no longer says nothing bears on the line: ${block}`)
})

test('"What would settle it" names linking that run to this line, with a person\'s name on it', () => {
  const sentence = hqOf(unlinkedCoverage()).inspector.settle.sentence
  assert.match(sentence, /link/i)
  assert.match(sentence, /name/i)
  assert.notEqual(sentence, C.model.inspector.settle_in_progress, 'not the sentence for a line nothing bears on')
  assert.ok(text(settle(inspectorHtml(unlinkedCoverage()))).includes(sentence))
})

test('"What to do" is one primary: the run sheet\'s door to the line\'s link control, not "Run the check"', () => {
  const hq = hqOf(unlinkedCoverage())
  const a = hq.inspector.action
  assert.ok(a.link, 'the model carries the door')
  assert.equal(a.link.n, 1)
  assert.equal(a.link.label, RUN_DOOR_LABEL(LINE), 'the door reads as the run sheet\'s door does')
  const sheet = M.runSheetModel(passedSheet, { version: { short: 'e71c5b0' }, unlinked: hq.readiness.unlinked })
  assert.equal(sheet.unlinked.n, a.link.n, 'the same line: the same target the run sheet opens')
  assert.equal(sheet.unlinked.door, a.link.label, 'the same words on the same door')
  /* The pair is still the one answer to "why is approval off" — the Verify stage's summary prints it (RUN-LINK-TRUTH-R4). */
  assert.equal(a.reason, hq.readiness.blocker.reason)
  assert.equal(a.enabler, hq.readiness.blocker.enabler)
  const html = action(inspectorHtml(unlinkedCoverage()))
  assert.equal((html.match(/ws-change-primary/g) || []).length, 1, 'exactly one primary')
  assert.ok(html.includes('data-door="ws-cov-line-1"'), 'it opens line 1\'s control')
  assert.ok(!html.includes('aria-haspopup="dialog"'), 'it is not the run sheet\'s opener')
  assert.ok(!text(html).includes(C.model.actions.run), 'and it does not say Run the check')
})

test('Brief still offers the run as a secondary door once the Inspector\'s primary is the link door', () => {
  const html = briefHtml(unlinkedCoverage())
  assert.ok(html.includes('ws-brief-run'), 'Check it again is reachable from the story')
  assert.equal((blockOf(html, 'ws-inspector-footer').match(/ws-change-primary/g) || []).length, 1)
})

test('every block now names the same next act: top, row, blocker and Inspector all say link', () => {
  const hq = hqOf(unlinkedCoverage())
  assert.match(hq.headline.text, /link it/)
  assert.match(hq.criteria[0].sentence, /link it/)
  assert.match(hq.readiness.blocker.enabler, /[Ll]ink/)
  assert.match(hq.inspector.shows.unlinked.sentence, /link it/)
  assert.match(hq.inspector.settle.sentence, /link/i)
  assert.match(hq.inspector.action.link.label, /link it/)
})

/* ── one derivation: the surfaces move together ───────────────────────── */

const SURFACES = (coverage, change) => {
  const hq = hqOf(coverage, change)
  return {
    headline: /isn't linked/.test(hq.headline.text),
    row: hq.criteria[0].sentence === UNLINKED,
    blocker: /isn't linked/.test(hq.readiness.blocker ? hq.readiness.blocker.reason : ''),
    inspectorShows: Boolean(hq.inspector.shows.unlinked),
    inspectorSettle: /link/i.test(hq.inspector.settle.sentence),
    inspectorDoor: Boolean(hq.inspector.action.link),
    runSheet: hq.readiness.unlinked.length > 0,
  }
}

test('one derivation: a Change that has the unlinked run, one that has none, one that has none run, and one already linked — every surface answers alike', () => {
  const unlinked = SURFACES(unlinkedCoverage())
  assert.deepEqual(Object.values(unlinked), Object.values(unlinked).map(() => true), JSON.stringify(unlinked))
  for (const [name, coverage] of [['nothing run', nothingCoverage()], ['linked', linkedCoverage()]]) {
    const s = SURFACES(coverage)
    assert.deepEqual(Object.values(s), Object.values(s).map(() => false), `${name}: ${JSON.stringify(s)}`)
  }
})

test('the Inspector claims less when the two derivations part: it says "link it" only where the engine AND the row both do', () => {
  /* The engine names line 1 but the record holds no run to link (a candidate list that was cut short): the row says "No test ran". */
  const engineOnly = unlinkedCoverage()
  engineOnly.candidates = []
  assert.ok(!hqOf(engineOnly).inspector.shows.unlinked)
  assert.ok(!hqOf(engineOnly).inspector.action.link)
  assert.ok(text(shows(inspectorHtml(engineOnly))).includes(NOTHING_YET))
  /* A run that bears on nothing the engine names: the row says link it, the engine does not. */
  const rowOnly = unlinkedCoverage()
  rowOnly.readiness.obligations[1] = { ...rowOnly.readiness.obligations[1], unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked' }
  assert.ok(!hqOf(rowOnly).inspector.shows.unlinked)
  assert.ok(!hqOf(rowOnly).inspector.action.link)
})

test('a line a named person set aside is never told "link it" by the Inspector, beside an unlinked run for another line', () => {
  const hq = hqOf(waivedBesideUnlinkedCoverage(), CHANGE_TWO)
  assert.deepEqual(hq.readiness.unlinked.map((u) => u.n), [2])
  /* The Inspector is about the first line still needing something in accepted order — the set-aside line is `none` in the coverage, as it was at base. */
  assert.equal(hq.inspector.what.quote, LINE)
  assert.ok(!hq.inspector.shows.unlinked, 'line 1 is set aside; the engine does not name it unlinked')
  assert.ok(!hq.inspector.action.link, 'never a door to link a set-aside line')
})

test('with an unconfirmed identity first in the engine\'s order, the Inspector keeps every base block: its next act is never "link it" beside a blocker that says confirm (RUN-LINK-TRUTH-R3)', () => {
  const coverage = unlinkedCoverage()
  coverage.readiness = readinessOfDoc([UNCONFIRMED_IDENTITY, unlinkedObligation(1, LINE)], ['candidate_identity', 'criterion:1'])
  const hq = hqOf(coverage)
  assert.doesNotMatch(hq.readiness.blocker.reason, /isn't linked/, 'the blocker is identity, first in the engine\'s order')
  assert.ok(!hq.inspector.shows.unlinked)
  assert.equal(hq.inspector.settle.sentence, C.model.inspector.settle_in_progress)
  assert.ok(!hq.inspector.action.link)
  assert.equal(hq.inspector.action.label, C.model.actions.run)
  assert.ok(text(shows(inspectorHtml(coverage))).includes(NOTHING_YET), 'the Inspector is the base Inspector')
})

/* ── claims the same: a line with no run at all ───────────────────────── */

test('a line with no run at all is byte-identical to base in every Inspector block', () => {
  const hq = hqOf(nothingCoverage())
  assert.equal(hq.inspector.shows.empty, NOTHING_YET)
  assert.equal(hq.inspector.shows.unlinked || null, null)
  assert.equal(hq.inspector.settle.sentence, C.model.inspector.settle_in_progress)
  assert.equal(hq.inspector.action.label, 'Run the check')
  assert.equal(hq.inspector.action.enabled, false)
  assert.equal(hq.inspector.action.door, null)
  assert.ok(!('link' in hq.inspector.action), 'the action has exactly the fields it had at base')
  const html = inspectorHtml(nothingCoverage())
  assert.ok(text(shows(html)).includes('Nothing in the record bears on this line yet.'))
  assert.ok(text(settle(html)).includes('A check that covers this line, run on this version.'))
  assert.ok(text(action(html)).startsWith('What to do Run the check'), text(action(html)))
  assert.ok(action(html).includes('aria-haspopup="dialog"'), 'the primary is the run sheet\'s opener')
  assert.ok(!briefHtml(nothingCoverage()).includes('ws-brief-run'), 'and Brief carries no second run door, as at base')
})

test('a linked line has no stale "link it" on Brief, the Inspector or the whole-story page', () => {
  const cov = linkedCoverage()
  const brief = briefHtml(cov)
  assert.ok(!/link it/.test(text(brief)), 'Brief')
  const page = M.renderPage({ model: pageModel(cov), onCopy: noop, records: null })
  assert.ok(!/link it/.test(text(page)), 'the whole-story page')
})

/* ── the whole-story page: a door on screen, the plain sentence in the files ─ */

function pageModel(coverage, change = CHANGE) {
  const hq = hqOf(coverage, change)
  return M.buildPacketView({ change, coverage, work: [], hq })
}

test('on the whole-story page each unlinked-run sentence carries a door to the Verify stage, where the link control is', () => {
  const page = M.renderPage({ model: pageModel(unlinkedCoverage()), onCopy: noop, records: null })
  const doors = page.match(/<a [^>]*ws-packet-link-door[^>]*>[^<]*<\/a>/g) || []
  assert.equal(doors.length, 2, `one after the "still needs evidence" line and one under the mapping row: ${doors.join(' | ')}`)
  for (const d of doors) {
    /* D-050 (PATH-UI-2-R2, BROKEN 2): the expected value here used to be
     * `href="#/changes/ch1/verify"`, a plain link to the Verify stage. That
     * encoded the defect: followed, it loaded Brief, which draws no stage, so
     * the person was on a screen with no line open and no name box and needed a
     * second click. The address now also names the line to open (`?door=1`);
     * `packet-link-door.test.js` follows it and holds what it must do. */
    assert.ok(d.includes('href="?door=1#/changes/ch1/verify"'), d)
    assert.ok(text(d).includes(RUN_DOOR_LABEL(LINE)), d)
  }
  assert.ok(text(page).includes(text(esc(UNLINKED))), 'the plain sentence is still there')
})

test('a line nothing ran for, and a linked line, get no door on the whole-story page', () => {
  for (const cov of [nothingCoverage(), linkedCoverage()]) {
    assert.ok(!M.renderPage({ model: pageModel(cov), onCopy: noop, records: null }).includes('ws-packet-link-door'))
  }
})

const sha = (s) => createHash('sha256').update(s).digest('hex')
const RECORDS = null
/* Pinned at cf8ca110 (the base of this card): the three exported files for the unlinked Change, byte for byte. */
const EXPORT_AT_BASE = {
  html: '0ece1046718654af5e60bb1dc1fd75c6d09fe40204de0d22ff5abb1e4fb9f840',
  md: '057e25d819b1d682f03c1d3de4d03094b609ec99b119400e0f9045f19d7e2c17',
  json: 'cf2077d4827f137b4b5ce847dc4285bcb0f5de5fc14b6d63b6a1a7e6fe33bc0e',
}

/* MERGE-SEAM-C12 (D-050) — the union with QUAL-F2-R3 (cloud/qual-f2-r2 lineage, merged after this one): every JSON export
 * now carries one more key, `"accepted_after_work": null` when the record does not show an acceptance after the work. The
 * base pin above stays exactly as it was, and is applied to the JSON with that one line taken out; the JSON as it is now has
 * its own pin. html and md are unchanged and keep their base pins. */
const QUAL_F2_KEY = /\n *"accepted_after_work": null,/
const JSON_WITH_QUAL_F2_KEY = 'fb8270e8a6e997ea61350300c936b61dabfbb41cc1acda0a7c152cef6e1bbd10'

/* PACKET-CHECKED-TRUE try 3 (D-050) — "Every run" now says, on the row of a run nobody linked, that it is not linked and
 * is not a check of this Change (`markdown.run_unlinked`); this fixture's one run is exactly that. The base pins above stay
 * exactly as they were and are applied with that one line taken out of each format; the files as they are now have their
 * own pins. Nothing else in any of the three files moved. */
const RUN_UNLINKED = 'Not linked to anything this Change counts as done, so it is not a check of it until somebody links it.'
const WITHOUT_RUN_UNLINKED = {
  html: (s) => s.replace('<p class="sub">' + RUN_UNLINKED + '</p>', ''),
  md: (s) => s.replace('\n  ' + RUN_UNLINKED, ''),
  json: (s) => s.replace(/"lines": \[\n\s+"Not linked to anything this Change counts as done, so it is not a check of it until somebody links it\."\n\s+\]/, '"lines": []'),
}
const EXPORT_NOW = {
  html: 'f6004952f8ce953d33a57afd278c44aa8960fd2a93673b099ca578db3237c3e6',
  md: '5573e6647584557738150fc44be3a0f2c8aace0d0bac44db201757c0d39323ac',
  json: 'f3173d55e9437edcd0ff77876e213de49496002c4d087b3d49c4a6a02e5b657f',
}

test('the exported Markdown, HTML and JSON keep the plain sentence, byte-identical to base', () => {
  const packet = hqOf(unlinkedCoverage()).packet
  for (const format of ['html', 'md', 'json']) {
    const out = M.renderExport(packet, RECORDS, { format }).text
    assert.ok(out.includes(UNLINKED.replace("'", format === 'html' ? '&#39;' : "'")), `${format} keeps the sentence`)
    assert.ok(!out.includes('link-door'), `${format} carries no door`)
    assert.ok(!out.includes('#/changes/'), `${format} carries no address`)
    assert.ok(out.includes(RUN_UNLINKED), `${format} says the unlinked run is not linked`) // D-050
    assert.equal(sha(out), EXPORT_NOW[format], `${format} differs from its pin`) // D-050
    const before = WITHOUT_RUN_UNLINKED[format](out)
    if (format === 'json') {
      assert.equal(before.match(new RegExp(QUAL_F2_KEY.source, 'g')).length, 1, 'json carries the one QUAL-F2-R3 key exactly once')
      assert.equal(sha(before), JSON_WITH_QUAL_F2_KEY, 'json differs from its QUAL-F2-R3 pin by more than the unlinked line')
      assert.equal(sha(before.replace(QUAL_F2_KEY, '')), EXPORT_AT_BASE.json, 'json differs from base by more than the one QUAL-F2-R3 key and the unlinked line')
    } else {
      assert.equal(sha(before), EXPORT_AT_BASE[format], `${format} differs from base by more than the unlinked line`)
    }
  }
})

/* ── self-attack: repeats, boundaries, odd characters ─────────────────── */

const twoUnlinkedCoverage = () => ({
  ...unlinkedCoverage(),
  criteria: [
    { index: 1, text: LINE, state: 'none', items: [], removed: [] },
    { index: 2, text: LINE_TWO, state: 'none', items: [], removed: [] },
  ],
  readiness: readinessOfDoc([IDENTITY, unlinkedObligation(1, LINE), unlinkedObligation(2, LINE_TWO)], ['criterion:1', 'criterion:2']),
})

test('two lines with a passing run nobody linked: the Inspector\'s door and the run sheet\'s door are the same first line', () => {
  const hq = hqOf(twoUnlinkedCoverage(), CHANGE_TWO)
  assert.deepEqual(hq.readiness.unlinked.map((u) => u.n), [1, 2])
  assert.equal(hq.inspector.action.link.n, 1)
  assert.equal(hq.inspector.what.quote, LINE)
  const sheet = M.runSheetModel(passedSheet, { version: { short: 'e71c5b0' }, unlinked: hq.readiness.unlinked })
  assert.equal(sheet.unlinked.n, hq.inspector.action.link.n)
})

test('a line whose words look like markup or placeholders is said as written, never filled or executed', () => {
  const ODD = 'Shows {n} items <b>bold</b> & "quoted" “curly” $& $1 {line}'
  const change = { ...CHANGE, accepted_scope: { ...CHANGE.accepted_scope, criteria: [ODD] } }
  const coverage = unlinkedCoverage()
  coverage.criteria[0].text = ODD
  coverage.readiness.obligations[1] = { ...coverage.readiness.obligations[1], text: ODD }
  const hq = hqOf(coverage, change)
  assert.equal(hq.inspector.action.link.label, `Go to “${ODD}” to link it`)
  const html = inspectorHtml(coverage, change)
  assert.ok(!html.includes('<b>bold</b>'), 'the words are escaped')
  assert.ok(html.includes('&lt;b&gt;bold&lt;/b&gt;'))
})

test('once the run is linked the Inspector moves to the linked state and every "link it" is gone from it', () => {
  const hq = hqOf(linkedCoverage())
  assert.ok(!hq.inspector.shows.unlinked)
  assert.ok(!('link' in hq.inspector.action))
  const html = inspectorHtml(linkedCoverage())
  assert.ok(!/link it/.test(text(html)), text(html))
  assert.ok(text(shows(html)).includes('Proven'), text(shows(html)))
})
