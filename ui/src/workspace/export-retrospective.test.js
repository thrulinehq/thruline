import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* QUAL-F2-R3 (`Q7`, brief §16 row 7) — the caution that what counts as done
 * was accepted after the work it covers is in the file a person takes away.
 *
 * WHY THIS FILE EXISTS. QUAL-F2-R2 put the sentence on the Change screen and on
 * the packet PAGE, in JSX, and left `hq.packet.sections` — the one array the
 * exported file walks (`packet-export.js` documentOf / jsonOf) — untouched. On a
 * real `thruline ui` the fact was served and shown and the export preview said
 * "preview has sentence \"accepted after the work was done\": false"; so did
 * every format and Copy as Markdown (the R2 adversary, `export-probe.mjs`). In
 * ThruLine's own terms the taken-away file IS the packet: the T-5-4 doctrine is
 * that "the exported file has one source of numbers: hq.packet.sections[]".
 *
 * WHAT IS PINNED. For a Change whose acceptance came after the work, the
 * sentence and its UTC date are in the Markdown, the HTML and the JSON export
 * (the JSON also as typed data, not only prose), in Copy as Markdown, and in the
 * redacted copy (a date is not private); for an accepted-first Change they are
 * in none of them. And there is ONE source: the caution is a section of
 * `hq.packet.sections`; the page's accepted section prints that section rather
 * than composing a sentence of its own, and the file composes it from the copy
 * file's string.
 *
 * The fixture is synthetic. No string below came off a real machine.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-export-retrospective-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { PacketPage, copiedMarkdown } from './packet.jsx';
  import { deriveChangeModel } from './change-model.js';
  import { buildPacketView } from './packet-view.js';
  import { renderExport, FORMATS } from './packet-export.js';
  import { HQ_COPY } from './hq-copy.js';
  export { copiedMarkdown, deriveChangeModel, buildPacketView, renderExport, FORMATS, HQ_COPY };
  export const renderPacket = props => renderToStaticMarkup(React.createElement(PacketPage, props));
`

const COMMON = {
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
}

async function load(name, plugins) {
  const result = await build({ ...COMMON, stdin: { contents: ENTRY, resolveDir: here }, plugins })
  const file = join(temporary, name)
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}

const M = await load('export-retrospective.mjs')

/* A second bundle over a PROBE copy: the real `HQ_COPY` with the packet's
 * after-work sentence replaced. Equality with the real copy cannot tell a
 * string that came from the copy file from a literal that happens to match it;
 * a string that does not exist in the real copy can. */
const COPY_FILE = join(here, 'hq-copy.js')
const PROBE_SENTENCE = 'probe-after-work-in-the-file {date}'
const PROBED = await load('export-retrospective-probe.mjs', [{
  name: 'probe-copy',
  setup(b) {
    b.onLoad({ filter: /hq-copy\.js$/ }, () => {
      const source = readFileSync(COPY_FILE, 'utf8')
      /* Anchored to whitespace: `accepted_after_work` (the Change screen's own
       * key) ends in the same letters and must not be the one replaced. */
      const swapped = source.replace(/(\s)after_work: '[^']*'/, `$1after_work: '${PROBE_SENTENCE}'`)
      assert.notEqual(swapped, source, 'the probe replaced nothing')
      return { contents: swapped, loader: 'js' }
    })
  },
}])

const { copiedMarkdown, deriveChangeModel, buildPacketView, renderExport, FORMATS, HQ_COPY, renderPacket } = M

/* The predeclared words, and the acceptance date the fixture carries. */
const AFTER_WORDS = 'accepted after the work was done'
const ACCEPTED_AT = '2026-09-26T09:20:00Z'
const ACCEPTED_DATE = '2026-09-26'
const WORK_AT = '2026-09-25T17:10:00Z'

const CRITERION = 'Two retry requests for the same delivery do not enqueue duplicate work.'
const SCOPE_PATH = 'src/deliveries/'
const WORKTREE = '/Users/synthetic/retry-delivery'
const FAILURE = 'assertion failed: duplicate delivery enqueued'
const AGENT_WORDS = 'The agent said: every retry case passes.'

function aChange(retrospective, over = {}) {
  return {
    schema_version: 6,
    id: 'cccccccc-0000-4000-8000-00000000c0f3',
    project_id: 'cccccccc-0000-4000-8000-00000000p0f3',
    opened_at: '2026-09-25T09:00:00Z',
    request: { title: 'Retry a failed delivery', purpose: CRITERION, requester: 'sam', source: 'person' },
    accepted_scope: {
      version: 1,
      proposed_at: ACCEPTED_AT,
      criteria: [CRITERION],
      exclusions: [], allowed_scope: [SCOPE_PATH], constraints: [], check_profile: [],
      content_sha256: 'e'.repeat(64),
      acceptance: { actor: 'sam', accepted_at: ACCEPTED_AT, rationale: 'accepting what was built' },
    },
    scope_history: [],
    implementation: { version: 0, bound_at: WORK_AT, repository_identity: 'repo-synthetic', base_revision: '3062e374' + '0'.repeat(32), snapshot_id: null, workspace_identity: null },
    implementation_history: [],
    associations: [],
    examination: 'none_yet',
    decision: 'none_yet',
    reviews: [],
    code_moved: null,
    retrospective,
    masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
    ...over,
  }
}

const RETROSPECTIVE = {
  scope_version: 1,
  accepted_at: ACCEPTED_AT,
  accepted_on: ACCEPTED_DATE,
  work_record_id: 'ei-work',
  work_recorded_at: WORK_AT,
  opened_revision: 'a6b45180' + '0'.repeat(32),
  work_revision: '3062e374' + '0'.repeat(32),
}

const COVERAGE = {
  scope_version: 1,
  link_rows: 1,
  execution_sessions: [],
  criteria: [{
    index: 1,
    text: CRITERION,
    state: 'evidence',
    items: [{
      link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run',
      name: 'cargo test --package deliveries', observed: 'not_proven', bears: true, applies: true, applies_slug: 'inputs_applies',
      failure_line: FAILURE,
    }],
  }],
  candidates: [],
}

const WORK = [{ id: 's1', findings: [{ said: { text: AGENT_WORDS }, record: { text: 'Not proven: nothing in the record shows this was done.' } }] }]

const RECORDS = {
  schema_version: 1,
  change_id: 'cccccccc-0000-4000-8000-00000000c0f3',
  checked_at: '2026-09-27T12:00:00Z',
  worktree: WORKTREE,
  decisions: [],
  evidence: [{ kind: 'check_run', evidence_id: 'e1', session_id: 's1', criteria: [1], name: 'cargo test --package deliveries', standing: 'readable', execution_recorded: true }],
  transcripts: [],
  totals: { records: 1, readable: 1, not_readable: 0 },
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
}

const hqOf = (change, mod = M) => mod.deriveChangeModel(change, COVERAGE, { items: [] }, null, WORK)
const files = (hq, records, redact, mod = M) => Object.fromEntries(mod.FORMATS.map((f) => [f, mod.renderExport(hq.packet, records, { format: f, redact })]))
const count = (haystack, needle) => haystack.split(needle).length - 1
const text = (html) => html.replace(/<[^>]*>/g, ' ').replace(/&#x27;/g, "'").replace(/&#39;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/&#x2F;/g, '/').replace(/\s+/g, ' ').trim()
const ALL_CLASSES = ['agent_words', 'paths', 'failure_output']

/** The text inside the element whose id is `id`, up to its matching close. */
function region(html, id) {
  const open = html.indexOf(`id="${id}"`)
  assert.ok(open > 0, `no element with id ${id} on the page`)
  const start = html.lastIndexOf('<', open)
  const tag = /^<([a-z0-9]+)/i.exec(html.slice(start))[1]
  let depth = 0
  const re = new RegExp(`<(/?)${tag}\\b[^>]*>`, 'gi')
  re.lastIndex = start
  for (let m = re.exec(html); m; m = re.exec(html)) {
    depth += m[1] ? -1 : 1
    if (depth === 0) return text(html.slice(start, m.index + m[0].length))
  }
  return text(html.slice(start))
}

/** The sentence the copy file makes for a date. */
const sentenceFor = (date, copy = HQ_COPY) => copy.change.packet.accepted.after_work.replace('{date}', date)

/** The section of `hq.packet.sections` that carries the caution, found by what it says. */
const cautionSection = (hq) => hq.packet.sections.find((s) => s.rows.some((r) => r.text.includes(AFTER_WORDS)))

/* ── 1. the caution is in every file the person takes away ───────────────── */

test('the Markdown export says the list was accepted after the work was done, with the acceptance date', () => {
  const md = files(hqOf(aChange(RETROSPECTIVE)), null).md.text
  assert.ok(md.includes(sentenceFor(ACCEPTED_DATE)), `the Markdown file does not carry the caution:\n${md}`)
})

test('the HTML export says it, with the acceptance date', () => {
  const html = files(hqOf(aChange(RETROSPECTIVE)), null).html.text
  assert.ok(text(html).includes(sentenceFor(ACCEPTED_DATE)), 'the HTML file does not carry the caution')
})

test('the JSON export says it in prose, and as typed data a reader can find', () => {
  const j = JSON.parse(files(hqOf(aChange(RETROSPECTIVE)), null).json.text)
  const prose = j.sections.flatMap((s) => s.rows.map((r) => r.text)).filter((t) => t.includes(AFTER_WORDS))
  assert.deepEqual(prose, [sentenceFor(ACCEPTED_DATE)], 'the JSON sections do not carry the sentence exactly once')
  assert.deepEqual(
    { accepted_on: j.accepted_after_work && j.accepted_after_work.accepted_on, list: j.accepted_after_work && j.accepted_after_work.list },
    { accepted_on: ACCEPTED_DATE, list: 1 },
    'the JSON carries no typed field for when the list was accepted and which list it was',
  )
  /* The date is a leaf of its own outside the prose, so a reader who does not
   * read sentences can still find it. */
  const outside = JSON.stringify({ ...j, sections: undefined })
  assert.ok(outside.includes(`"${ACCEPTED_DATE}"`), 'the acceptance date is only ever inside a sentence')
})

test('Copy as Markdown carries it, and is the export’s own Markdown text', () => {
  const hq = hqOf(aChange(RETROSPECTIVE))
  const copied = copiedMarkdown(hq, RECORDS)
  assert.ok(copied.includes(sentenceFor(ACCEPTED_DATE)), `Copy as Markdown does not carry the caution:\n${copied}`)
  assert.equal(copied, renderExport(hq.packet, RECORDS, { format: 'md' }).text, 'Copy as Markdown is not the export’s Markdown')
})

test('with the records checked too, every format still carries it', () => {
  const out = files(hqOf(aChange(RETROSPECTIVE)), RECORDS)
  for (const f of FORMATS) assert.ok(out[f].text.includes(AFTER_WORDS) || text(out[f].text).includes(AFTER_WORDS), `${f} lost the caution when the records were read`)
})

test('the sentence is printed once in the Markdown and once in the HTML', () => {
  const out = files(hqOf(aChange(RETROSPECTIVE)), null)
  assert.equal(count(out.md.text, AFTER_WORDS), 1)
  assert.equal(count(text(out.html.text), AFTER_WORDS), 1)
})

/* ── 2. the redacted file: a date is not private ─────────────────────────── */

test('the redacted file, in every format, keeps the caution and its date', () => {
  const out = files(hqOf(aChange(RETROSPECTIVE)), RECORDS, ALL_CLASSES)
  for (const f of FORMATS) {
    assert.equal(out[f].redaction.redacted, true, `${f} was not redacted`)
    const body = f === 'html' ? text(out[f].text) : out[f].text
    assert.ok(body.includes(sentenceFor(ACCEPTED_DATE)), `the redacted ${f} file lost the caution`)
  }
  const j = JSON.parse(out.json.text)
  assert.equal(j.accepted_after_work && j.accepted_after_work.accepted_on, ACCEPTED_DATE, 'the redacted JSON lost the typed date')
})

test('the redacted file removes what it says it removes, and still carries the caution', () => {
  const md = files(hqOf(aChange(RETROSPECTIVE)), RECORDS, ALL_CLASSES).md.text
  for (const gone of [AGENT_WORDS, FAILURE, WORKTREE]) assert.ok(!md.includes(gone), `the redacted file still carries ${gone}`)
  assert.ok(md.includes(AFTER_WORDS))
})

/* ── 3. the control: accepted first, or nothing to say ───────────────────── */

test('accepted before the work: no format, no copy, no redacted file says after', () => {
  for (const retrospective of [null, undefined]) {
    const change = aChange(retrospective)
    if (retrospective === undefined) delete change.retrospective
    const hq = hqOf(change)
    for (const redact of [undefined, ALL_CLASSES]) {
      const out = files(hq, RECORDS, redact)
      for (const f of FORMATS) assert.ok(!text(out[f].text).includes(AFTER_WORDS), `${f} says "after" on an acceptance that came first (redact ${JSON.stringify(redact)})`)
    }
    assert.ok(!copiedMarkdown(hq, RECORDS).includes(AFTER_WORDS), 'Copy as Markdown says "after" on an acceptance that came first')
    assert.equal(cautionSection(hq), undefined, 'the packet carries a caution section on an acceptance that came first')
  }
})

test('an accepted-first Change’s files are the same whether the served fact is null or absent, and carry no caution section', () => {
  const absent = aChange(undefined)
  delete absent.retrospective
  const a = files(hqOf(aChange(null)), RECORDS)
  const b = files(hqOf(absent), RECORDS)
  for (const f of FORMATS) assert.equal(a[f].text, b[f].text, `${f} differs between a null fact and an absent one`)
  assert.equal(JSON.parse(a.json.text).accepted_after_work, null, 'the typed field is not null when there is nothing to say')
  assert.deepEqual(hqOf(aChange(null)).packet.sections.map((s) => s.id), ['proof', 'said', 'runs', 'findings', 'decisions', 'not_recorded'])
})

test('a served fact the page would not say is not in the file either', () => {
  const cases = [
    ['a date that cannot be read', aChange({ ...RETROSPECTIVE, accepted_on: 'sometime on friday' })],
    ['no date', aChange({ ...RETROSPECTIVE, accepted_on: null })],
    ['another version of the list than the one on the page', aChange({ ...RETROSPECTIVE, scope_version: 2 })],
    ['a fact that is not an object', aChange('yes')],
    ['no accepted list at all', aChange(RETROSPECTIVE, { accepted_scope: null })],
    /* Neither the Change screen nor the page says it beside a list with nothing
     * in it (both draw it inside the branch that lists the criteria), and a
     * file that said "this list" above a proof section that says "nothing
     * counts as done yet" would be about a list nobody can read. */
    ['a list with nothing in it', (() => { const c = aChange(RETROSPECTIVE); c.accepted_scope.criteria = []; return c })()],
  ]
  for (const [name, change] of cases) {
    const hq = hqOf(change)
    for (const f of FORMATS) assert.ok(!text(files(hq, null)[f].text).includes(AFTER_WORDS), `${f} says it for ${name}`)
    assert.equal(cautionSection(hq), undefined, `the packet holds the caution for ${name}`)
  }
})

/* The date is the served UTC date, not the front of a stamp's spelling: the
 * engine serves it, and a stamp that carries an offset spells another zone's
 * date. D-215 ruled the recorded-time source and the date; this only reads it. */
test('the date in the file is the served UTC date, not the front of the stamp’s spelling', () => {
  const change = aChange({ ...RETROSPECTIVE, accepted_at: '2026-09-26T23:30:00-05:00', accepted_on: '2026-09-27' })
  const out = files(hqOf(change), null)
  for (const f of FORMATS) {
    const body = f === 'html' ? text(out[f].text) : out[f].text
    assert.ok(body.includes(sentenceFor('2026-09-27')), `${f} does not say the served date`)
    assert.ok(!body.includes(sentenceFor('2026-09-26')), `${f} says the stamp’s spelled date`)
  }
})

/* ── 4. one source ───────────────────────────────────────────────────────── */

test('the caution is a section of hq.packet.sections, before the section it qualifies', () => {
  const hq = hqOf(aChange(RETROSPECTIVE))
  const section = cautionSection(hq)
  assert.ok(section, 'the caution is not among hq.packet.sections: the file cannot carry what the section array does not')
  assert.deepEqual(section.rows.map((r) => r.text), [sentenceFor(ACCEPTED_DATE)])
  assert.equal(hq.packet.sections.indexOf(section), 0, 'the caution about the list does not come before the proof measured against it')
  assert.equal(section.title, HQ_COPY.change.packet.markdown.accepted_after_work_heading, 'the section is not titled from the copy file')
})

test('the Markdown and the HTML get the caution from the section array and nowhere else', () => {
  const hq = hqOf(aChange(RETROSPECTIVE))
  assert.ok(cautionSection(hq), 'precondition: the caution is in the section array')
  /* Everything else on the packet stays; only the section goes. */
  const without = { ...hq.packet, sections: hq.packet.sections.filter((s) => s !== cautionSection(hq)) }
  for (const f of ['md', 'html']) {
    const body = renderExport(without, null, { format: f }).text
    assert.ok(!text(body).includes(AFTER_WORDS), `the ${f} export composes the caution on its own, from something other than the section`)
  }
})

test('the sentence in the file is the copy file’s string, not a literal in the exporter', () => {
  const change = aChange(RETROSPECTIVE)
  const hq = hqOf(change, PROBED)
  const out = files(hq, null, undefined, PROBED)
  const want = PROBE_SENTENCE.replace('{date}', ACCEPTED_DATE)
  assert.ok(out.md.text.includes(want), 'the Markdown does not print the copy file’s string')
  assert.ok(text(out.html.text).includes(want), 'the HTML does not print the copy file’s string')
  assert.ok(out.json.text.includes(want), 'the JSON does not print the copy file’s string')
  assert.ok(!out.md.text.includes(AFTER_WORDS), 'a second, literal copy of the sentence is in the Markdown')
})

test('the page prints the section the file prints: change the section and the page changes, and it is not said twice', () => {
  const change = aChange(RETROSPECTIVE)
  const hq = hqOf(change)
  const real = renderPacket({ model: buildPacketView({ change, work: [], hq }), records: null })
  assert.ok(region(real, 'ws-packet-accepted').includes(sentenceFor(ACCEPTED_DATE)), 'the page’s accepted section does not say it')
  assert.equal(count(text(real), AFTER_WORDS), 1, 'the page says it more than once')

  const PROBE = 'section-text-probe on ' + ACCEPTED_DATE
  const changed = {
    ...hq,
    packet: {
      ...hq.packet,
      sections: hq.packet.sections.map((s) => (s === cautionSection(hq) ? { ...s, rows: [{ text: PROBE, lines: [] }] } : s)),
    },
  }
  const page = renderPacket({ model: buildPacketView({ change, work: [], hq: changed }), records: null })
  const accepted = region(page, 'ws-packet-accepted')
  assert.ok(accepted.includes(PROBE), `the page does not print the section’s row: ${accepted}`)
  assert.ok(!text(page).includes(AFTER_WORDS), 'the page composes a sentence of its own beside the section')
  assert.equal(count(text(page), PROBE), 1, 'the section’s row is printed more than once')
})

test('the page and the Markdown file say the same sentence', () => {
  const change = aChange(RETROSPECTIVE)
  const hq = hqOf(change)
  const page = region(renderPacket({ model: buildPacketView({ change, work: [], hq }), records: null }), 'ws-packet-accepted')
  const row = cautionSection(hq).rows[0].text
  assert.ok(page.includes(row), 'the page does not say what the section says')
  assert.ok(files(hq, null).md.text.includes('- ' + row), 'the file does not say what the section says')
})

test('no state word, no accusation and no claim about the work in what the file says', () => {
  const s = sentenceFor(ACCEPTED_DATE)
  for (const banned of ['Proven', 'Not proven', 'In progress', 'Needs a re-check', 'Loose thread', 'Fog', 'Approved', 'lie', 'lied', 'wrong', 'fail', 'ledger', 'receipt', 'event', 'disposition']) {
    assert.ok(!new RegExp(`\\b${banned}\\b`, 'i').test(s), `"${banned}" is in: ${s}`)
  }
  const heading = HQ_COPY.change.packet.markdown.accepted_after_work_heading
  assert.equal(typeof heading, 'string', 'the section has no heading in the copy file')
  for (const banned of ['Proven', 'Not proven', 'lie', 'wrong', 'fail', 'ledger', 'event']) assert.ok(!new RegExp(`\\b${banned}\\b`, 'i').test(heading), `"${banned}" is in the heading: ${heading}`)
})
