import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* P6B-HTT-4 — the packet never rests "Ready" on a criterion it says was never
 * assessed, without saying so in the same line; and a section whose body is
 * its empty case never wears the populated heading.
 *
 * NEVER-ASSESSED-NOT-READY (R-2026-09-28-25, D-050) — the served readiness
 * below was, until this card, a READY answer resting on line 1: exactly the
 * defect the founder ruled out, since the engine can no longer serve it. The
 * fixture now carries what the engine serves for this record (not ready;
 * line 1 needs evidence because whether it still applies was never assessed;
 * line 2 satisfied), and every expectation that encoded "Ready rests on a
 * never-assessed line" is changed to what a person now reads, each one
 * marked `D-050` where it changed. HTT-4's own promise — the words sit on
 * the line itself, and nothing reads plainly where it is never assessed —
 * is asserted unchanged.
 *
 * The fixture is the defect card's (docs/loop/tasks/P6B-HTT-4.md), rebuilt as
 * served documents: one accepted line, one check run linked to it, a ready
 * readiness that rests on it, and NO applicability assessment for it — so the
 * packet's own "What was not recorded" section names it as never assessed.
 * A second line whose evidence WAS assessed is the control: its rests-on line
 * must not grow the words.
 *
 * Every assertion is about what a person reads: the rendered page, and the
 * Markdown, HTML and JSON files built from the same derivation. Readiness,
 * the rests-on selection and every served verdict field are asserted
 * unchanged. Every string below is synthetic.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-packet-truth-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { PacketPage } from './packet.jsx';
  import { buildPacketView } from './packet-view.js';
  import { deriveChangeModel, neverAssessed } from './change-model.js';
  import { renderExport } from './packet-export.js';
  import { HQ_COPY } from './hq-copy.js';
  import { fill } from './hq-narrate.js';
  export { buildPacketView, deriveChangeModel, neverAssessed, renderExport, HQ_COPY, fill };
  export const renderPacket = props => renderToStaticMarkup(React.createElement(PacketPage, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'packet-truth.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const { buildPacketView, deriveChangeModel, neverAssessed, renderExport, HQ_COPY, fill, renderPacket } = M
const P = HQ_COPY.change.packet
const MD = P.markdown

/* The words the packet already uses for "never assessed", lower-cased so the
 * sentence-initial form in "What was not recorded" and a mid-line form match. */
const NEVER = 'whether this still applies was never assessed'

const REVISION = '01m378r5c4b3a2918f7e6d5c4b3a2918f7e6d5c4'
const UNASSESSED = 'A delivery that was still waiting when the service was stopped is still exactly one delivery after it starts again, and it is attempted again.'
const ASSESSED = 'A delivery that succeeds is recorded once.'
const CHECK = 'bash:synthetic-repo:cargo test --test=restart'
/* The engine's reason slug for a line whose applicability was never assessed. */
const NEVER_SLUG = 'readiness_never_assessed'

const change = () => ({
  id: 'cccccccc-0000-4000-8000-00000000c604',
  project_id: 'cccccccc-0000-4000-8000-00000000p604',
  opened_at: '2026-01-02T09:00:00Z',
  request: { title: 'Deliveries survive a restart', purpose: 'Deliveries survive a restart', requester: 'sam', source: 'person' },
  accepted_scope: {
    version: 1, criteria: [UNASSESSED, ASSESSED], exclusions: [], allowed_scope: ['delivery/**'],
    constraints: [], check_profile: ['cargo-test'],
    content_sha256: '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
    acceptance: { actor: 'sam', accepted_at: '2026-01-02T10:30:00Z', rationale: 'that is the change' },
    proposed_at: '2026-01-02T10:29:00Z',
  },
  implementation: {
    version: 1, bound_at: '2026-01-02T10:00:00Z', repository_identity: 'repo-synthetic-0604',
    base_revision: REVISION, snapshot_id: null, workspace_identity: 'workspace-synthetic-0604',
  },
  associations: [], examination: 'none_yet', decision: 'none_yet',
  decisions: { standing: null, history: [], target_revision: REVISION, scope_version: 1 },
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
})

const item = (n, fresh) => ({
  link_id: 'cce_synthetic_060' + n, kind: 'check_run', evidence_id: 'evb_synthetic_060' + n,
  session_id: 'cli:check:2026-01-02T10:05:00Z', basis: 'declared', basis_slug: 'basis_declared',
  name: CHECK, observed: 'proven', bears: true, applies: true,
  applies_slug: 'inputs_applies', asserted_at: '2026-01-02T10:05:00Z', execution: 'completed', execution_slug: 'execution_completed',
  failure_line: null, evidence_class: null, evidence_class_slug: null,
  ...(fresh ? { fresh: true, fresh_slug: 'applicability_still_applies' } : {}),
})

/** Line 1: evidence, never assessed. Line 2: evidence, assessed and still applying.
 * D-050: NOT ready — the engine's answer for this record (see never-assessed.readiness.json). */
const coverage = () => ({
  schema_version: 1, change_id: 'cccccccc-0000-4000-8000-00000000c604', scope_version: 1, link_rows: 2,
  executions: [], run_checks: null,
  criteria: [
    { index: 1, text: UNASSESSED, state: 'evidence', state_slug: 'coverage_evidence', items: [item(1, false)], removed: [] },
    { index: 2, text: ASSESSED, state: 'evidence', state_slug: 'coverage_evidence', items: [item(2, true)], removed: [] },
  ],
  readiness: {
    ready: false, target_version: REVISION, headline_slug: 'readiness_needs_evidence_one', with_exceptions: false,
    obligations: [
      { id: 'criterion:1', criterion_index: 1, text: UNASSESSED, outcome: 'needs_evidence', unsatisfied: 'never_assessed', unsatisfied_slug: NEVER_SLUG, evidence_name: null, waivable: true },
      { id: 'criterion:2', criterion_index: 2, text: ASSESSED, outcome: 'satisfied', evidence_name: CHECK, waivable: true },
    ],
    rests_on: [],
    missing: ['criterion:1'],
  },
})

const surfaces = () => {
  const ch = change()
  const cov = coverage()
  const hq = deriveChangeModel(ch, cov, null, null, [])
  const model = { ...buildPacketView({ change: ch, proposals: null, work: [], coverage: cov, evidence: [], hq }), hq }
  const html = renderPacket({ model, notes: [] })
  return {
    cov,
    hq,
    html,
    md: renderExport(hq.packet, null, { format: 'md' }).text,
    htmlFile: renderExport(hq.packet, null, { format: 'html' }).text,
    json: JSON.parse(renderExport(hq.packet, null, { format: 'json' }).text),
  }
}

const decode = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/&#x2F;/g, '/')
const plain = (s) => decode(s.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ').trim()
const lower = (s) => String(s).toLowerCase()

/** The rendered page's "It rests on" list, one string per line. */
const pageRests = (html) => {
  const block = /<ul class="ws-packet-readiness-rests">([\s\S]*?)<\/ul>/.exec(html)
  return block ? [...block[1].matchAll(/<li[^>]*>([\s\S]*?)<\/li>/g)].map((m) => plain(m[1])) : []
}

/** Each rests-on line that names `text` must carry the never-assessed words on that line. */
function assertRestsSayIt(where, lines, text, expectWords) {
  const hits = lines.filter((l) => l.includes(text))
  assert.equal(hits.length, 1, `${where}: the rests-on list names "${text.slice(0, 30)}…" exactly once`)
  if (expectWords) assert.ok(lower(hits[0]).includes(NEVER), `${where}: rests on a never-assessed line without saying so on that line: ${hits[0]}`)
  else assert.ok(!lower(hits[0]).includes(NEVER), `${where}: an assessed line was marked never assessed: ${hits[0]}`)
}

/* ── the fixture reproduces the defect's premise ───────────────────────── */

test('P6B-HTT-4 fixture: the packet says NOT ready (D-050), and says line 1 was never assessed', () => {
  const s = surfaces()
  assert.equal(s.hq.readiness.ready, false) // D-050: was true — Ready resting on a never-assessed line
  assert.ok(plain(s.html).includes(fill(MD.not_recorded_no_freshness, { what: UNASSESSED })), 'the page names line 1 as never assessed')
  assert.ok(!plain(s.html).includes(fill(MD.not_recorded_no_freshness, { what: ASSESSED })), 'the control line was assessed')
})

test('a packet with a linked passing check does not say nothing was checked', () => {
  const ch = change()
  ch.accepted_scope = { ...ch.accepted_scope, criteria: [ASSESSED] }
  const checkName = 'python -m unittest test_status.py'
  const cov = coverage()
  cov.link_rows = 1
  cov.criteria = [{
    index: 1, text: ASSESSED, state: 'evidence', state_slug: 'coverage_evidence',
    items: [{ ...item(1, true), name: checkName }], removed: [],
  }]
  cov.readiness = {
    ready: true, target_version: REVISION, headline_slug: 'readiness_ready', with_exceptions: false,
    obligations: [{
      id: 'criterion:1', criterion_index: 1, text: ASSESSED, outcome: 'satisfied',
      evidence_name: checkName, linked_by: 'sam', waivable: true,
    }],
    rests_on: [{ obligation_id: 'criterion:1', text: ASSESSED, evidence_name: checkName, linked_by: 'sam' }],
    missing: [],
  }
  const hq = deriveChangeModel(ch, cov, null, null, [])
  const model = buildPacketView({ change: ch, proposals: null, work: [], coverage: cov, evidence: [], hq })
  const html = renderPacket({ model, notes: [] })

  assert.equal(hq.readiness.ready, true, 'the linked passing check makes this version ready')
  assert.deepEqual(hq.packet.runs.map((run) => run.name), [checkName], 'the run is in the packet')
  const runs = /<section[^>]*id="ws-packet-36-runs"[^>]*>([\s\S]*?)<\/section>/.exec(html)
  assert.ok(runs, 'the page has an Every run section')
  assert.ok(plain(runs[1]).includes(checkName), 'the page lists the passing check')
  const examination = /<section[^>]*id="ws-packet-examination"[^>]*>([\s\S]*?)<\/section>/.exec(html)
  assert.ok(examination, 'the page has an examination section')
  assert.doesNotMatch(
    plain(examination[1]),
    /nothing has been checked for this Change|nothing was looked at/i,
    'an empty linked-conversation section must not deny the formal check recorded above',
  )
})

/* ── done-when 1: the rests-on line says it, on every surface ──────────── */

test('P6B-HTT-4 done-when 1: the page never rests on a never-assessed line without saying so on that line', () => {
  const s = surfaces()
  const lines = pageRests(s.html)
  // D-050: was 2 lines rested on; the list under the answer now holds the one line still needing evidence.
  assert.equal(lines.length, 1, 'the page lists the one line that still needs evidence')
  assertRestsSayIt('page', lines, UNASSESSED, true)
  assert.ok(!lines.some((l) => l.includes(ASSESSED)), 'page: the assessed line is not listed as missing') // D-050
})

test('P6B-HTT-4 done-when 1: the Markdown, HTML and JSON files say it on the same line too', () => {
  const s = surfaces()
  /* The bullets straight after the ready sentence, and nothing further down. */
  const all = s.md.split('\n')
  const at = all.findIndex((l) => l.includes(s.hq.packet.readinessLine))
  assert.ok(at >= 0, 'the Markdown file carries the ready sentence')
  const mdLines = []
  for (const l of all.slice(at + 1)) {
    if (/^\s*[-*] /.test(l)) mdLines.push(l)
    else if (l.trim() === '' && !mdLines.length) continue
    else break
  }
  // D-050: was 2 lines rested on under a ready sentence; now the one missing line under the not-ready sentence.
  assert.equal(mdLines.length, 1, 'the Markdown file lists the one line that still needs evidence')
  assertRestsSayIt('markdown', mdLines, UNASSESSED, true)
  assert.ok(!mdLines.some((l) => l.includes(ASSESSED)), 'markdown: the assessed line is not listed as missing') // D-050
  // D-050: the list sits under the readiness sentence, which is no longer "It rests on:".
  const at2 = s.htmlFile.indexOf(s.hq.packet.readinessLine)
  const htmlBlock = at2 >= 0 ? /^[^<]*<\/p>\s*<ul>([\s\S]*?)<\/ul>/.exec(s.htmlFile.slice(at2 + s.hq.packet.readinessLine.length)) : null
  assert.ok(htmlBlock, 'the HTML file carries the list under the readiness sentence')
  const htmlLines = [...htmlBlock[1].matchAll(/<li[^>]*>([\s\S]*?)<\/li>/g)].map((m) => plain(m[1]))
  assertRestsSayIt('html', htmlLines, UNASSESSED, true)
  assert.ok(!htmlLines.some((l) => l.includes(ASSESSED)), 'html: the assessed line is not listed as missing') // D-050
  assertRestsSayIt('json', s.json.readiness.rests_on, UNASSESSED, true)
  assert.ok(!s.json.readiness.rests_on.some((l) => l.includes(ASSESSED)), 'json: the assessed line is not listed as missing') // D-050
})

test('P6B-HTT-4 done-when 1: no criterion is plainly rested-on in one section and never-assessed in another', () => {
  const s = surfaces()
  for (const [where, rests, notRecorded] of [
    ['page', pageRests(s.html), s.hq.packet.notRecorded],
    ['json', s.json.readiness.rests_on, s.json.not_recorded],
  ]) {
    for (const nr of notRecorded) {
      for (const text of [UNASSESSED, ASSESSED]) {
        if (nr !== fill(MD.not_recorded_no_freshness, { what: text })) continue
        for (const line of rests.filter((l) => l.includes(text))) {
          assert.ok(lower(line).includes(NEVER), `${where}: "${line}" is plainly rested on while "${nr}"`)
        }
      }
    }
  }
})

/* ── done-when 2: an empty body wears the empty heading ────────────────── */

/* BRIEF-COUNT-AGREES-R2 changed this fixture (D-050, finding 9): it reached the empty body ("Everything here has something behind it")
 * with line 1 never assessed, which the served answer says still needs evidence — the page top said one still needed it above that
 * heading. The empty body is reached now by an answer with nothing left to need: both lines met, ready. */
test('P6B-HTT-4 done-when 2: "What still needs evidence" never heads a body that says nothing does', () => {
  const ch = change()
  const cov = coverage()
  cov.readiness = {
    ...cov.readiness,
    ready: true, headline_slug: 'readiness_ready', missing: [],
    obligations: cov.readiness.obligations.map((o) => (o.outcome === 'needs_evidence' ? { ...o, outcome: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: CHECK } : o)),
  }
  const hq = deriveChangeModel(ch, cov, null, null, [])
  const model = { ...buildPacketView({ change: ch, proposals: null, work: [], coverage: cov, evidence: [], hq }), hq }
  const s = { html: renderPacket({ model, notes: [] }) }
  const section = /<section class="ws-packet-needs">([\s\S]*?)<\/section>/.exec(s.html)
  assert.ok(section, 'the what-to-read-first section is on the page')
  const heading = plain(/<h2[^>]*>([\s\S]*?)<\/h2>/.exec(section[1])[1])
  assert.ok(plain(section[1]).includes(P.needs.none), 'the fixture reaches the empty body')
  assert.notEqual(heading, P.needs.heading, 'the populated heading sits over its own empty case')
  assert.equal(heading, P.needs.none_heading, 'the empty case wears its own heading, from the copy file')
})

/* ── done-when 4: nothing about the answer moved ──────────────────────── */

test('P6B-HTT-4 done-when 4: readiness, the rests-on selection and the served fields are unchanged', () => {
  const s = surfaces()
  const served = coverage().readiness
  assert.deepEqual(s.cov.readiness, served, 'the served document is not mutated')
  assert.equal(s.hq.readiness.ready, false) // D-050: was true
  assert.equal(s.json.readiness.ready, false) // D-050: was true
  assert.deepEqual(s.hq.readiness.restsOn.map((r) => [r.id, r.text, r.evidenceName]), served.rests_on.map((r) => [r.obligation_id, r.text, r.evidence_name]))
  /* The Change screen's own rests-on line is the served text and name, untouched. */
  assert.deepEqual(s.hq.readiness.restsOn.map((r) => r.line), served.rests_on.map((r) => r.text + ' — ' + r.evidence_name))
  assert.equal(s.hq.packet.readinessLine, s.hq.readiness.sentence)
  // D-050: was one line per thing it rests on; not ready, it is one line per served obligation still needing evidence.
  const missing = served.obligations.filter((o) => o.outcome === 'needs_evidence')
  assert.equal(s.hq.packet.readinessRests.length, missing.length, 'one line per thing still missing, none added, none dropped')
  s.hq.packet.readinessRests.forEach((line, i) => assert.ok(line.startsWith(missing[i].text + ' — '), `line ${i + 1} still leads with the served words`))
})

/* ── P6B-HTT-4-R2: identity, never text ─────────────────────────────────
 *
 * Round 1 tied a rests-on line to its criterion by the obligation's
 * criterion_index and, failing that, by the accepted WORDS. Two accepted
 * lines can carry the same words; the adversary served two such lines, one
 * never assessed and one assessed and still applying, with no
 * criterion_index, and the packet marked the assessed one "never assessed"
 * (gates/adversary-P6B-HTT-4/TRANSCRIPT.md). The tie is now the criterion's
 * identity: criterion_index, or the recorded obligation id
 * (`criterion:<index>`, change_readiness.rs). When neither is served and the
 * words are shared, the packet does not guess.
 *
 * D-050 (NEVER-ASSESSED-NOT-READY): the twins below used to be served as a
 * READY answer resting on both lines. The engine now serves line 1 as still
 * needing evidence (never assessed) and the answer as not ready, so the
 * identity rule is asserted where it now does its work: which accepted line
 * `neverAssessed()` marks, from the engine's served obligation, by identity.
 */

const TWIN = 'Every delivery is recorded exactly once, even after a restart.'

/** Two accepted lines with the SAME words: 1 never assessed, 2 assessed and fresh. Not ready (D-050). */
const twins = ({ index = true, ids = true } = {}) => {
  const ch = change()
  ch.accepted_scope = { ...ch.accepted_scope, criteria: [TWIN, TWIN] }
  const cov = coverage()
  cov.criteria = [
    { index: 1, text: TWIN, state: 'evidence', state_slug: 'coverage_evidence', items: [item(1, false)], removed: [] },
    { index: 2, text: TWIN, state: 'evidence', state_slug: 'coverage_evidence', items: [item(2, true)], removed: [] },
  ]
  const ob = (n) => ({
    id: ids ? 'criterion:' + n : '', ...(index ? { criterion_index: n } : {}),
    text: TWIN, waivable: true,
    ...(n === 1
      ? { outcome: 'needs_evidence', unsatisfied: 'never_assessed', unsatisfied_slug: NEVER_SLUG, evidence_name: null }
      : { outcome: 'satisfied', evidence_name: CHECK }),
  })
  cov.readiness.obligations = [ob(1), ob(2)]
  cov.readiness.rests_on = []
  return { ch, cov }
}

const surfacesOf = ({ ch, cov }) => {
  const hq = deriveChangeModel(ch, cov, null, null, [])
  const model = { ...buildPacketView({ change: ch, proposals: null, work: [], coverage: cov, evidence: [], hq }), hq }
  const html = renderPacket({ model, notes: [] })
  const md = renderExport(hq.packet, null, { format: 'md' }).text
  const htmlFile = renderExport(hq.packet, null, { format: 'html' }).text
  const json = JSON.parse(renderExport(hq.packet, null, { format: 'json' }).text)
  const all = md.split('\n')
  const at = all.findIndex((l) => l.includes(hq.packet.readinessLine))
  const mdLines = []
  for (const l of at >= 0 ? all.slice(at + 1) : []) {
    if (/^\s*[-*] /.test(l)) mdLines.push(l)
    else if (l.trim() === '' && !mdLines.length) continue
    else break
  }
  const at2 = htmlFile.indexOf(hq.packet.readinessLine)
  const block = at2 >= 0 ? /^[^<]*<\/p>\s*<ul>([\s\S]*?)<\/ul>/.exec(htmlFile.slice(at2 + hq.packet.readinessLine.length)) : null // D-050: under the readiness sentence, not "It rests on:"
  const htmlLines = block ? [...block[1].matchAll(/<li[^>]*>([\s\S]*?)<\/li>/g)].map((m) => plain(m[1])) : []
  return {
    cov, hq, html,
    rests: { page: pageRests(html), markdown: mdLines, html: htmlLines, json: json.readiness.rests_on },
  }
}

const marked = (hq) => hq.criteria.map((c) => neverAssessed(c))

for (const [label, shape] of [
  ['with criterion_index', { index: true, ids: true }],
  ['without criterion_index, by the recorded obligation id', { index: false, ids: true }],
]) {
  test(`P6B-HTT-4-R2: twin words, ${label}: only the never-assessed line is marked, and every surface lists it once`, () => {
    const s = surfacesOf(twins(shape))
    assert.equal(s.hq.readiness.ready, false) // D-050: was true
    assert.deepEqual(marked(s.hq), [true, false], 'line 1, by identity, and never line 2')
    for (const [where, lines] of Object.entries(s.rests)) {
      // D-050: was both lines rested on; now the one line still needing evidence.
      assert.equal(lines.length, 1, `${where}: the one line still needing evidence is listed`)
      assert.ok(lower(lines[0]).includes(NEVER), `${where}: line 1 was never assessed and its line must say so: ${lines[0]}`)
    }
  })
}

test('P6B-HTT-4-R2: twin words and no identity served: no accepted line is marked never assessed', () => {
  const s = surfacesOf(twins({ index: false, ids: false }))
  // D-050: was "each rested-on line says it cannot be told"; not ready, nothing is rested on, and the
  // client does not guess which of two identical lines the engine meant.
  assert.deepEqual(marked(s.hq), [false, false])
  assert.ok(!s.hq.packet.notRecorded.includes(fill(MD.not_recorded_no_freshness, { what: TWIN })), 'no guess in "What was not recorded"')
})

test('P6B-HTT-4-R2: words carried by one line only still identify it when no identity is served', () => {
  const ch = change()
  const cov = coverage()
  cov.readiness.obligations = cov.readiness.obligations.map(({ criterion_index: _drop, ...o }) => ({ ...o, id: '' }))
  cov.readiness.rests_on = cov.readiness.rests_on.map((r) => ({ ...r, obligation_id: '' }))
  const s = surfacesOf({ ch, cov })
  assert.deepEqual(marked(s.hq), [true, false]) // D-050: asserted on the marked line, not on a rested-on one
  for (const [where, lines] of Object.entries(s.rests)) {
    assert.equal(lines.length, 1, where)
    assert.ok(lower(lines[0]).includes(NEVER), `${where}: ${lines[0]}`)
  }
})

test('P6B-HTT-4-R2: readiness, the rests-on selection and the served fields are unchanged for twins', () => {
  for (const shape of [{ index: true, ids: true }, { index: false, ids: true }, { index: false, ids: false }]) {
    const input = twins(shape)
    const served = structuredClone(input.cov.readiness)
    const s = surfacesOf(input)
    assert.deepEqual(s.cov.readiness, served, 'the served document is not mutated')
    assert.equal(s.hq.readiness.ready, false) // D-050: was true
    assert.deepEqual(s.hq.readiness.restsOn, [], 'nothing is rested on') // D-050: was both lines
    assert.equal(s.hq.packet.readinessRests.length, 1, 'none added, none dropped') // D-050: was 2
    s.hq.packet.readinessRests.forEach((line) => assert.ok(line.startsWith(TWIN + ' — ')))
  }
})
