import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* PACKET-CHECKED-TRUE (QA P0-6, D-335) — the packet's examination section
 * never denies a recorded check.
 *
 * The defect: a Change with one accepted line and one passing formal check
 * run linked to it (no linked conversation work). Verify says Proven and
 * Ready; the packet's "Every run, and the version it ran on" lists the run as
 * Proven on the named version; and the SAME page's "What the checking showed,
 * and what it could not reach" said "Nothing has been checked for this
 * Change", because that section was decided from the linked conversations
 * alone and never from the formal checks the page itself prints.
 *
 * The rule: the examination section is decided from every recorded source
 * the packet shows. "Nothing has been checked" appears only when there is no
 * recorded check of any kind; when there are formal checks the section names
 * them in the words "Every run" uses. No section of one packet contradicts
 * another. Every string below is synthetic.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-packet-checked-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { PacketPage } from './packet.jsx';
  import { buildPacketView } from './packet-view.js';
  import { deriveChangeModel } from './change-model.js';
  import { HQ_COPY } from './hq-copy.js';
  import { renderExport } from './packet-export.js';
  export { buildPacketView, deriveChangeModel, HQ_COPY, renderExport };
  export const renderPacket = props => renderToStaticMarkup(React.createElement(PacketPage, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'packet-checked-true.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const { buildPacketView, deriveChangeModel, HQ_COPY, renderExport, renderPacket } = M
const P = HQ_COPY.change.packet
const NOTHING = P.sections.examination.empty

const ID = 'cccccccc-0000-4000-8000-00000000c635'
const REVISION = '01m378r5c4b3a2918f7e6d5c4b3a2918f7e6d5c4'
const LINE = 'The totals add up.'
const CHECK = 'pytest -q tests/test_totals.py'

const change = () => ({
  id: ID,
  project_id: 'cccccccc-0000-4000-8000-00000000p635',
  opened_at: '2026-01-02T09:00:00Z',
  request: { title: 'Totals add up', purpose: 'Totals add up', requester: 'sam', source: 'person' },
  accepted_scope: {
    version: 1, criteria: [LINE], exclusions: [], allowed_scope: ['totals/**'],
    constraints: [], check_profile: ['pytest'],
    content_sha256: '3a66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
    acceptance: { actor: 'sam', accepted_at: '2026-01-02T10:30:00Z', rationale: 'that is the change' },
    proposed_at: '2026-01-02T10:29:00Z',
  },
  implementation: {
    version: 1, bound_at: '2026-01-02T10:00:00Z', repository_identity: 'repo-synthetic-0635',
    base_revision: REVISION, snapshot_id: null, workspace_identity: 'workspace-synthetic-0635',
  },
  associations: [], examination: 'none_yet', decision: 'none_yet',
  decisions: { standing: null, history: [], target_revision: REVISION, scope_version: 1 },
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
})

/* `thruline check --for-change`, linked as a test-class run: a formal check
 * run, finished, passing, on the version this Change is set up against. */
const run = () => ({
  link_id: 'cce_synthetic_0635', kind: 'check_run', evidence_id: 'evb_synthetic_0635',
  session_id: 'cli:check:2026-01-02T10:05:00Z', basis: 'declared', basis_slug: 'basis_declared',
  name: CHECK, observed: 'proven', bears: true, applies: true,
  applies_slug: 'inputs_applies', asserted_at: '2026-01-02T10:05:00Z', execution: 'completed', execution_slug: 'execution_completed',
  failure_line: null, evidence_class: 'test', evidence_class_slug: 'evidence_class_test',
  fresh: true, fresh_slug: 'applicability_still_applies',
})

/* An observation row: recorded evidence bound to the line, and not a check
 * run — so "Every run" (check runs only, T-S-5 finding 2) does not list it. */
const OBSERVED = 'EVIDENCE_BOUND'
const observation = () => ({
  link_id: 'cce_synthetic_0636', kind: 'observation', evidence_id: 'evb_synthetic_0636',
  session_id: 'cli:observe:2026-01-02T10:06:00Z', basis: 'declared', basis_slug: 'basis_declared',
  name: OBSERVED, observed: null, bears: true, applies: null,
  asserted_at: '2026-01-02T10:06:00Z',
})

const coverage = ({ withRun = true, observationOnly = false } = {}) => ({
  schema_version: 1, change_id: ID, scope_version: 1, link_rows: withRun || observationOnly ? 1 : 0,
  executions: [], run_checks: null,
  criteria: [
    withRun
      ? { index: 1, text: LINE, state: 'evidence', state_slug: 'coverage_evidence', items: [run()], removed: [] }
      : observationOnly
        ? { index: 1, text: LINE, state: 'evidence', state_slug: 'coverage_evidence', items: [observation()], removed: [] }
        : { index: 1, text: LINE, state: 'none', state_slug: 'coverage_none', items: [], removed: [] },
  ],
  readiness: withRun
    ? {
      ready: true, target_version: REVISION, headline_slug: 'readiness_ready', with_exceptions: false,
      obligations: [{ id: 'criterion:1', criterion_index: 1, text: LINE, outcome: 'satisfied', evidence_name: CHECK, waivable: true }],
      rests_on: [{ obligation_id: 'criterion:1', text: LINE, evidence_name: CHECK }],
      missing: [],
    }
    : {
      ready: false, target_version: REVISION, headline_slug: 'readiness_needs_evidence_one', with_exceptions: false,
      obligations: [{ id: 'criterion:1', criterion_index: 1, text: LINE, outcome: 'needs_evidence', evidence_name: null, waivable: true }],
      rests_on: [],
      missing: ['criterion:1'],
    },
})

/** The page as `PacketView` builds it: no linked conversation work at all. */
const surfaces = (opts) => {
  const ch = change()
  const cov = coverage(opts)
  const hq = deriveChangeModel(ch, cov, null, null, [])
  const model = { ...buildPacketView({ change: ch, proposals: null, work: [], coverage: cov, evidence: [], hq }), hq }
  return { hq, model, html: renderPacket({ model, notes: [] }) }
}

const decode = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/&#x2F;/g, '/').replace(/&lt;/g, '<').replace(/&gt;/g, '>')
const plain = (s) => decode(s.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ').trim()

/** The examination section of the rendered page, as the words a person reads. */
function examination(html) {
  const m = /<section class="ws-packet-section" id="ws-packet-examination">([\s\S]*?)<\/section>/.exec(html)
  assert.ok(m, 'the page renders the examination section')
  return plain(m[1])
}

/** The rows of the packet's own "Every run, and the version it ran on". */
function everyRun(hq) {
  const runs = hq.packet.sections.find((s) => s.id === 'runs')
  assert.ok(runs, 'the packet carries its "Every run" section')
  return runs.rows.map((r) => r.text)
}

/** No section of one packet contradicts another: a run listed means something was checked. */
function assertNoContradiction(s, where) {
  const exam = examination(s.html)
  if (everyRun(s.hq).length) {
    assert.ok(!exam.includes(NOTHING), `${where}: "Every run" lists a run, and the examination section says nothing was checked`)
    assert.doesNotMatch(exam, /nothing has been checked/i, `${where}: the examination section denies a recorded check`)
  }
}

/* ── the premise: what Verify and "Every run" already say ─────────────── */

test('PACKET-CHECKED-TRUE premise: the Change is Proven and Ready, and "Every run" lists the check on the named version', () => {
  const s = surfaces()
  assert.equal(s.hq.readiness.ready, true)
  const rows = everyRun(s.hq)
  assert.equal(rows.length, 1)
  assert.ok(rows[0].includes(CHECK), rows[0])
  assert.ok(rows[0].includes(P.markdown.run_version.replace('{version}', s.hq.version.short)), rows[0])
})

/* ── the defect ───────────────────────────────────────────────────────── */

test('PACKET-CHECKED-TRUE: a recorded passing check and no linked conversation work — the examination section does not say nothing was checked', () => {
  const s = surfaces()
  const section = s.model.sections.find((x) => x.id === 'examination')
  assert.equal(section.filled, true, 'the examination section is filled when a formal check is recorded')
  const exam = examination(s.html)
  assert.ok(!exam.includes(NOTHING), 'the examination section denies the recorded check: ' + exam)
})

test('PACKET-CHECKED-TRUE: the examination section names the check in the same words and version as "Every run"', () => {
  const s = surfaces()
  const exam = examination(s.html)
  for (const row of everyRun(s.hq)) assert.ok(exam.includes(row), `the examination section does not name "${row}": ${exam}`)
})

test('PACKET-CHECKED-TRUE: no section of one packet contradicts another', () => {
  assertNoContradiction(surfaces(), 'one passing check, no linked work')
})

/* ── the control: nothing recorded at all still says so ───────────────── */

test('PACKET-CHECKED-TRUE control: with no recorded check of any kind, the section still says nothing has been checked', () => {
  const s = surfaces({ withRun: false })
  assert.equal(everyRun(s.hq).length, 0)
  assert.equal(s.model.sections.find((x) => x.id === 'examination').filled, false)
  assert.ok(examination(s.html).includes(NOTHING))
  assertNoContradiction(s, 'nothing recorded')
})

/* ── the mapping-only case: recorded evidence that is not a run ───────── */

test('PACKET-CHECKED-TRUE: an observation row in the mapping and no run — the examination section never says nothing was looked at', () => {
  const s = surfaces({ withRun: false, observationOnly: true })
  assert.equal(everyRun(s.hq).length, 0, 'premise: an observation is not a run, so "Every run" lists none')
  const row = s.model.mapping.rows[0]
  assert.equal(row.items.length, 1, 'premise: the mapping section shows the recorded observation')
  assert.equal(s.model.sections.find((x) => x.id === 'examination').filled, true, 'the examination section is filled when the mapping shows recorded evidence')
  const exam = examination(s.html)
  assert.ok(!exam.includes(NOTHING), 'the examination section denies recorded evidence: ' + exam)
  assert.doesNotMatch(exam, /nothing was looked at/i)
  assert.doesNotMatch(exam, /nothing has been checked/i)
  /* It says what was recorded, in the mapping section's own words. */
  assert.ok(exam.includes(LINE), exam)
  assert.ok(exam.includes(row.sentence), exam)
  assert.ok(exam.includes(OBSERVED), exam)
})

test('PACKET-CHECKED-TRUE: no section contradicts another — evidence in the mapping means the examination section is not empty', () => {
  for (const opts of [{}, { withRun: false, observationOnly: true }, { withRun: false }]) {
    const s = surfaces(opts)
    const shown = s.model.mapping.rows.some((r) => r.items.length)
    if (shown || everyRun(s.hq).length) assert.ok(!examination(s.html).includes(NOTHING), JSON.stringify(opts))
  }
})

/* ── verifier repair (try 1 of 2): every sentence agrees with every section ─
 *
 * The verifier drove the real HQ (snapA–F) and found four more places where
 * one packet said two things about what was recorded:
 *  1. the footer said "nothing has been written down against this Change"
 *     beside a linked, listed run;
 *  2. a run on OLDER code read "Proven — The record does not say which
 *     version this ran on", while Proof on the same page says it was checked
 *     on different code;
 *  3. a run that checked nothing read only "<name> — version X", where the
 *     mapping says "Ran, checked nothing." and "settles nothing";
 *  4. a run nobody linked was listed under "The checks written down for this
 *     Change", as if it were a check of it.
 */

const RUN_ON_OLD_CODE = 'pytest -q tests/test_totals.py --old'
const RAN_NOTHING = 'sh -c echo hello'
const UNLINKED = 'pytest -q tests/test_unlinked.py'

const linked = (n, extra) => ({ ...run(), link_id: 'cce_synthetic_064' + n, evidence_id: 'evb_synthetic_064' + n, ...extra })

/** A Change whose coverage holds exactly these linked items and candidates. */
const surfacesWith = ({ items = [], candidates = [], work = [] }) => {
  const ch = change()
  const cov = coverage({ withRun: false })
  cov.link_rows = items.length
  cov.criteria = [{ index: 1, text: LINE, state: items.length ? 'evidence' : 'none', state_slug: items.length ? 'coverage_evidence' : 'coverage_none', items, removed: [] }]
  cov.candidates = candidates
  const hq = deriveChangeModel(ch, cov, null, null, work)
  const model = { ...buildPacketView({ change: ch, proposals: null, work, coverage: cov, evidence: [], hq }), hq }
  return { hq, model, html: renderPacket({ model, notes: [] }) }
}

/** The footer of the rendered page, as words. */
function footer(html) {
  const m = /<footer class="ws-packet-footer">([\s\S]*?)<\/footer>/.exec(html)
  assert.ok(m, 'the page renders its footer')
  return plain(m[1])
}

/** The rendered list under "The checks written down for this Change", one string per run. */
function checksList(html) {
  const m = /<ul class="ws-packet-rows ws-packet-checks">([\s\S]*?)<\/ul>\s*<\/?[a-z]/.exec(html)
  if (!m) return []
  return [...m[1].matchAll(/<li[^>]*>([\s\S]*?)<\/li>/g)].map((x) => plain(x[1]))
}

test('PACKET-CHECKED-TRUE repair 1: the footer never says nothing was written down when a run is linked and listed', () => {
  const s = surfaces()
  assert.ok(everyRun(s.hq).length, 'premise: "Every run" lists the run')
  const f = footer(s.html)
  assert.ok(!f.includes(P.footer.none), 'the footer denies the linked run: ' + f)
  assert.doesNotMatch(f, /nothing has been written down/i)
})

test('PACKET-CHECKED-TRUE repair 1: the footer never says nothing was written down beside an observation or an unlinked run', () => {
  for (const s of [
    surfaces({ withRun: false, observationOnly: true }),
    surfacesWith({ candidates: [{ ...run(), kind: 'check_run', evidence_id: 'evb_synthetic_0650', name: UNLINKED }] }),
  ]) assert.doesNotMatch(footer(s.html), /nothing has been written down/i)
})

test('PACKET-CHECKED-TRUE repair 1 control: with nothing recorded the footer still says so', () => {
  assert.ok(footer(surfaces({ withRun: false }).html).includes(P.footer.none))
})

test('PACKET-CHECKED-TRUE repair 2: a run on other code reads as proven on that code, not as an unknown version, and not for this Change', () => {
  const s = surfacesWith({ items: [linked(1, { name: RUN_ON_OLD_CODE, applies: false, applies_slug: 'inputs_other_revision', fresh: null, fresh_slug: null })] })
  const otherCode = HQ_COPY.change.coverage.applies.inputs_other_revision
  const proof = s.hq.packet.sections.find((x) => x.id === 'proof').rows.flatMap((r) => r.lines).join('\n')
  assert.ok(proof.includes(otherCode), 'premise: Proof says it was checked on different code')
  const row = examination(s.html)
  assert.ok(row.includes(RUN_ON_OLD_CODE), row)
  assert.ok(row.includes(HQ_COPY.states.proven.word), 'the run\'s own result is not dropped: ' + row)
  assert.ok(row.includes(otherCode), 'the examination section says, as Proof does, that it ran on different code: ' + row)
  assert.ok(!row.includes(P.markdown.run_version_none), 'the examination section says "does not say which version" of a run Proof places on other code: ' + row)
})

test('PACKET-CHECKED-TRUE repair 3: a run that checked nothing says so, in the mapping\'s words', () => {
  const s = surfacesWith({ items: [linked(2, { name: RAN_NOTHING, observed: 'couldnt', note_slug: 'item_checked_nothing' })] })
  const exam = examination(s.html)
  assert.ok(exam.includes(RAN_NOTHING), exam)
  assert.ok(exam.includes(HQ_COPY.change.coverage.item.checked_nothing), 'the row does not say it checked nothing: ' + exam)
  assert.ok(exam.includes(HQ_COPY.states.couldnt.subtitle), 'the row does not say it settles nothing: ' + exam)
})

test('PACKET-CHECKED-TRUE repair 4: a run nobody linked is never listed as a check of this Change', () => {
  const s = surfacesWith({ candidates: [{ ...run(), kind: 'check_run', evidence_id: 'evb_synthetic_0651', name: UNLINKED }] })
  assert.ok(everyRun(s.hq).some((r) => r.includes(UNLINKED)), 'premise: "Every run" lists the unlinked run')
  assert.ok(!checksList(s.html).some((r) => r.includes(UNLINKED)), 'the unlinked run is listed under the checks of this Change')
  const exam = examination(s.html)
  assert.ok(!exam.includes(NOTHING), 'the section denies a run "Every run" lists: ' + exam)
  assert.ok(exam.includes(UNLINKED), 'the section leaves out a run the page lists: ' + exam)
  assert.match(exam, /not linked/i, 'the unlinked run is not said to be unlinked: ' + exam)
})

/* ── verifier try 2: "Every run" is the one source, fixed at the source ────
 *
 * Try 2 fixed the examination rows and left "Every run" itself saying "The
 * record does not say which version this ran on." of a run Proof places on
 * different code, and giving an unsettled run no result. Every run is what
 * the exported files carry, so the fix belongs there, and the examination
 * section repeats it verbatim. States A–F are the verifier's: an unlinked
 * run, a run on this version, a run on other code, a failure beside a pass on
 * other code, unsettled runs, and linked work beside the runs.
 */

const OLD = HQ_COPY.change.coverage.applies.inputs_other_revision
const HERE = HQ_COPY.change.coverage.applies.inputs_applies
const NOT_HERE = { applies: false, applies_slug: 'inputs_other_revision', fresh: null, fresh_slug: null }
const UNSETTLED = { observed: 'couldnt', note_slug: 'item_checked_nothing' }
const STATES = {
  A: () => ({ candidates: [{ ...run(), kind: 'check_run', evidence_id: 'evb_synthetic_0660', name: UNLINKED }] }),
  B: () => ({ items: [linked(1)] }),
  C: () => ({ items: [linked(2, { name: RUN_ON_OLD_CODE, ...NOT_HERE })] }),
  D: () => ({ items: [linked(3, { observed: 'not_proven', failure_line: 'assert 1 == 2' }), linked(4, { name: RUN_ON_OLD_CODE, ...NOT_HERE })] }),
  E: () => ({ items: [
    linked(5, { name: RAN_NOTHING, ...UNSETTLED }),
    linked(6, { name: CHECK + ' -k failing', observed: 'not_proven', ...NOT_HERE }),
    linked(7, { name: RUN_ON_OLD_CODE, ...NOT_HERE }),
    linked(8, { name: CHECK + ' -k skipped', ...UNSETTLED }),
  ] }),
  F: () => ({ ...STATES.E(), work: [{ id: 'cccccccc-0000-4000-8000-0000000000f6', findings: [], steps: 0 }] }),
}

/** "Every run" as rows: the typed run beside the line the packet prints for it. */
function everyRunRows(hq) {
  const section = hq.packet.sections.find((x) => x.id === 'runs')
  return section.rows.map((row, i) => ({ run: hq.packet.runs[i], text: row.text, lines: row.lines }))
}

/** The examination section's run rows, as rendered: each row's line and the lines under it. */
function examinationRows(html) {
  const m = /<section class="ws-packet-section" id="ws-packet-examination">([\s\S]*?)<\/section>/.exec(html)
  const rows = []
  for (const list of m[1].matchAll(/<ul class="ws-packet-rows ws-packet-(?:checks|unlinked)">([\s\S]*?)<\/ul>\s*<\/?(?!li|ul|span)[a-z]/g)) {
    for (const li of list[1].matchAll(/<li><span class="ws-packet-row-text">([\s\S]*?)<\/span>(?:<ul class="ws-packet-row-lines">([\s\S]*?)<\/ul>)?<\/li>/g)) {
      rows.push({ text: plain(li[1]), lines: li[2] ? [...li[2].matchAll(/<li[^>]*>([\s\S]*?)<\/li>/g)].map((x) => plain(x[1])) : [] })
    }
  }
  return rows
}

/** A Markdown line as the words it says: the export's backslash escapes taken off. */
const unescapeMd = (s) => s.replace(/\\([\\`*_{}[\]()#+\-.!|<>~])/g, '$1')

for (const [state, fixture] of Object.entries(STATES)) {
  test(`PACKET-CHECKED-TRUE try 3, state ${state}: every examination row equals its "Every run" row`, () => {
    const s = surfacesWith(fixture())
    const rows = everyRunRows(s.hq)
    assert.ok(rows.length, 'premise: "Every run" lists a run')
    assert.deepEqual(examinationRows(s.html), rows.map((r) => ({ text: r.text, lines: r.lines })))
  })

  test(`PACKET-CHECKED-TRUE try 3, state ${state}: no "Every run" row contradicts Proof`, () => {
    const s = surfacesWith(fixture())
    const rows = everyRunRows(s.hq)
    const proof = s.hq.packet.criteria.flatMap((c) => c.proof)
    for (const p of proof) {
      const mine = rows.filter((r) => r.run.name === p.name && (!p.word || r.run.word === p.word))
      assert.ok(mine.length, `Proof names ${p.name} and "Every run" does not`)
      for (const r of mine) {
        const said = [r.text, ...r.lines].join(' ')
        if (p.applies === OLD) {
          assert.ok(said.includes(OLD), `Proof says other code, "Every run" says: ${said}`)
          assert.ok(!said.includes(P.markdown.run_version_none), `"Every run" says the version is unknown of a run Proof places on other code: ${said}`)
        }
        if (p.applies === HERE) assert.ok(said.includes(P.markdown.run_version.replace('{version}', s.hq.version.short)), said)
      }
    }
    /* What an unsettled run showed is said, in the mapping's words. */
    for (const r of rows.filter((x) => x.run.name === RAN_NOTHING || x.run.name === CHECK + ' -k skipped')) {
      const said = [r.text, ...r.lines].join(' ')
      assert.ok(said.includes(HQ_COPY.change.coverage.item.checked_nothing), said)
      assert.ok(said.includes(HQ_COPY.states.couldnt.subtitle), said)
    }
    /* A run nobody linked says so on its own row, in every format. */
    for (const r of rows.filter((x) => x.run.name === UNLINKED)) assert.match([r.text, ...r.lines].join(' '), /not linked/i)
  })

  test(`PACKET-CHECKED-TRUE try 3, state ${state}: the Markdown and JSON files carry the same "Every run" rows`, () => {
    const s = surfacesWith(fixture())
    const rows = everyRunRows(s.hq)
    const json = JSON.parse(renderExport(s.hq.packet, null, { format: 'json' }).text)
    const runs = json.sections.find((x) => x.id === 'runs')
    assert.deepEqual(runs.rows.map((r) => ({ text: r.text, lines: r.lines })), rows.map((r) => ({ text: r.text, lines: r.lines })))
    const md = unescapeMd(renderExport(s.hq.packet, null, { format: 'md' }).text)
    for (const r of rows) for (const words of [r.text, ...r.lines]) assert.ok(md.includes(words), `the Markdown file does not carry: ${words}`)
  })
}
