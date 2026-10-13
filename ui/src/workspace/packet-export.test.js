/* T-5-4 — the whole story, as a file: the pure renderer.
 *
 * Every assertion is about the TEXT that would be saved, in all three formats,
 * built from the one derived view-model (`deriveChangeModel` → `hq.packet`)
 * over served documents this file writes by hand, and from the served
 * `/export` records answer. Card §B items 2, 3, 4, 6, 7 are here; item 1 (the
 * screen) is `export-sheet.test.js` and `docs/loop/export/export-walk.mjs`.
 *
 * Every fixture is synthetic. No string here came off a real machine.
 */

import test from 'node:test'
import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'

import { deriveChangeModel, STATE_WORD_SET } from './change-model.js'
import { HQ_COPY } from './hq-copy.js'
import {
  renderExport, sha256Hex, FORMATS, CLASSES, EXPORT_CSS, EXPORT_CSP, filenameOf,
} from './packet-export.js'

const X = HQ_COPY.change.export
const D = X.doc
const MD = HQ_COPY.change.packet.markdown

/* ── the fixture ──────────────────────────────────────────────────────── */

const SEEDED = '4719' // a number in the agent's prose and in no typed field
const AGENT_WORDS = `The agent said: "Done — all ${SEEDED} tests pass <script>alert(1)</script> <img src=x onerror=alert(2)>"`
const RUN_NAME = 'cargo test --package cart <img src=x onerror=alert(3)>'
const FAILURE = 'assertion failed: <script>alert(4)</script> totals differ'
const DECISION_PATH = '/Users/synthetic/shop/.thruline/decisions/e71c5b0aaaa.json'
const TRANSCRIPT = '/Users/synthetic/.claude/projects/shop/s1.jsonl'

const CHANGE = {
  id: 'ch1',
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Totals add up on the checkout page' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1,
    criteria: ['The totals add up', 'Nothing else on the page breaks'],
    allowed_scope: ['checkout/'],
    check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet',
  decisions: {
    target_revision: 'e71c5b0aaaa',
    standing: null,
    history: [
      { act: 'approved', target_revision: 'e71c5b0aaaa', actor: 'sam', decided_at: '2026-09-22T11:00:00Z', scope_version: 1 },
      { act: 'withdrawn', target_revision: 'e71c5b0aaaa', actor: 'sam', decided_at: '2026-09-22T11:30:00Z', scope_version: 1 },
    ],
  },
}

const COVERAGE = {
  scope_version: 1,
  link_rows: 1,
  execution_sessions: [],
  criteria: [
    {
      index: 1,
      text: 'The totals add up',
      state: 'evidence',
      items: [{
        link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run',
        name: RUN_NAME, observed: 'not_proven', bears: true, applies: true, applies_slug: 'inputs_applies',
        failure_line: FAILURE,
      }],
    },
    { index: 2, text: 'Nothing else on the page breaks', state: 'none', items: [] },
  ],
  candidates: [],
}

const APPLICABILITY = { items: [] }

const WORK = [{
  id: 's1',
  findings: [{
    said: { text: AGENT_WORDS },
    record: { text: 'Not proven: nothing in the record shows this was done.' },
  }],
}]

const RECORDS = {
  schema_version: 1,
  change_id: 'ch1',
  checked_at: '2026-09-23T12:00:00Z',
  worktree: '/Users/synthetic/shop',
  decisions: [
    { decision_id: 'cd_1', act: 'approved', version: 'e71c5b0aaaa', path: DECISION_PATH, standing: 'readable' },
    { decision_id: 'cd_2', act: 'withdrawn', version: 'e71c5b0aaaa', path: null, standing: null },
  ],
  evidence: [{ kind: 'check_run', evidence_id: 'e1', session_id: 's1', criteria: [1], name: RUN_NAME, standing: 'readable', execution_recorded: true }],
  transcripts: [{ session_id: 's1', file: TRANSCRIPT, lines: 12, lines_gone: 0, lines_changed: 0, standing: 'readable' }],
  totals: { records: 3, readable: 3, not_readable: 0 },
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
}

const clone = (v) => JSON.parse(JSON.stringify(v))
/* EXPORT-MD-ESCAPE (D-050). The Markdown shows a `<` that could begin a tag as
 * `&lt;`. The three hostile strings above have no other character the Markdown
 * treats specially, so this is exactly what the file holds for them; the old
 * expected value here was the raw string, which was the defect. */
const mdSafe = (s) => s.replaceAll('<', '&lt;')
/* EXPORT-MD-ESCAPE-R4 (D-050): every `[` and `]` in the Markdown is escaped, and a redaction marker (`[left out: …]`) is
 * put in place of a value before the text is escaped, so the marker is written `\\[left out: …\\]` (it reads the same). */
const mdMarker = (s) => s.replace(/[[\]]/g, '\\$&')
const hqOf = ({ change = CHANGE, coverage = COVERAGE, work = WORK } = {}) => deriveChangeModel(change, coverage, APPLICABILITY, null, work)
const all = (packet, records, redact) => Object.fromEntries(FORMATS.map((f) => [f, renderExport(packet, records, { format: f, redact })]))

/** Every string and number reachable in a value, as one haystack. */
function typedFields(node, out = []) {
  if (node === null || node === undefined) return out
  if (Array.isArray(node)) { for (const v of node) typedFields(v, out); return out }
  if (typeof node === 'object') { for (const v of Object.values(node)) typedFields(v, out); return out }
  if (typeof node === 'string' || typeof node === 'number') out.push(String(node))
  return out
}

/** The HTML without its constant head (stylesheet, policy, viewport), which
 * carries no record value, and with its five escapes read back — `&#39;` is
 * an apostrophe, not the number 39. */
function htmlBody(html) {
  const start = html.indexOf('<body>')
  return html.slice(start)
    .replace(/&#39;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
}

/* ── shape ───────────────────────────────────────────────────────────── */

test('three formats, each with its own name, type and text; the same inputs give the same bytes', () => {
  const p = hqOf().packet
  const a = all(p, RECORDS)
  const b = all(p, RECORDS)
  for (const f of FORMATS) {
    assert.equal(a[f].text, b[f].text, `${f} is not deterministic`)
    assert.match(a[f].filename, new RegExp(`^thruline-packet-totals-add-up-on-the-checkout-page-e71c5b0\\.${f}$`))
  }
  assert.match(a.html.mime, /^text\/html/)
  assert.match(a.md.mime, /^text\/markdown/)
  assert.match(a.json.mime, /^application\/json/)
  const j = JSON.parse(a.json.text)
  assert.equal(j.format, 'thruline-packet')
  assert.equal(j.schema_version, 1)
  assert.equal(j.change_id, 'ch1')
  assert.equal(j.version.short, 'e71c5b0')
  /* Typed values, not only prose rows (B10.1). */
  assert.equal(j.accepted_count, 2)
  assert.equal(j.criteria[0].n, 1)
  assert.equal(j.criteria[0].proof[0].failure.line, FAILURE)
  assert.deepEqual(j.runs[0], { name: RUN_NAME, word: 'Not proven', version: 'e71c5b0' })
  assert.deepEqual(j.decision.history.map((h) => [h.act, h.actor, h.when, h.version, h.scope_version]), [
    ['approved', 'sam', '2026-09-22', 'e71c5b0', 1],
    ['withdrawn', 'sam', '2026-09-22', 'e71c5b0', 1],
  ])
  assert.equal(j.redaction.redacted, false)
  assert.equal(j.records.totals.records, 3)
})

test('a file with no version and no title says so in its name and its heading rather than a blank', () => {
  assert.equal(filenameOf({ title: null, versionShort: null }, 'md', false), 'thruline-packet-change-unversioned.md')
  assert.equal(filenameOf({ title: '../../etc/passwd', versionShort: 'e71c5b0' }, 'html', true), 'thruline-packet-etc-passwd-e71c5b0-redacted.html')
  const p = { ...hqOf().packet, title: null }
  assert.ok(renderExport(p, RECORDS, { format: 'md' }).text.startsWith('# ' + D.no_title + '\n'))
})

test('the Markdown carries every §3.6 section of the model, in order, with the model\'s rows verbatim', () => {
  const hq = hqOf()
  const md = renderExport(hq.packet, RECORDS, { format: 'md' }).text
  let at = 0
  for (const s of hq.packet.sections) {
    const found = md.indexOf('## ' + s.title, at)
    assert.ok(found > at, `${s.title} out of order or missing`)
    at = found
    for (const r of s.rows) assert.ok(md.includes('- ' + mdSafe(r.text)) || s.id === 'decisions', `row missing: ${r.text}`)
  }
  for (const r of hq.packet.sections.find((s) => s.id === 'decisions').rows) assert.ok(md.includes('- ' + mdSafe(r.text)))
})

/* ── §B.2(a) numbers come from typed fields ───────────────────────────── */

test('(B.2a) every digit run in every format is a typed field of the model or of the records answer', () => {
  const hq = hqOf()
  for (const redact of [[], CLASSES]) {
    const out = all(hq.packet, RECORDS, redact)
    const haystack = [
      ...typedFields(hq.packet), ...typedFields(RECORDS),
      String(1), // the file's own format version, a constant of this module
    ].join(' ')
    for (const f of FORMATS) {
      /* The derivative's own provenance record is a typed field of the file. */
      const own = haystack + ' ' + typedFields(out[f].redaction).join(' ')
      /* JSON keys are the schema, not the record: the census reads its values. */
      const body = f === 'html' ? htmlBody(out[f].text) : f === 'json' ? typedFields(JSON.parse(out[f].text)).join('\n') : out[f].text
      const stray = [...body.matchAll(/\d+/g)].map((m) => m[0]).filter((d) => !own.includes(d))
      assert.deepEqual([...new Set(stray)], [], `${f}${redact.length ? ' (redacted)' : ''}: a number came from somewhere other than a typed field`)
    }
  }
})

test('(B.2a) a number that exists only in the agent\'s prose never becomes a number of the file', () => {
  const hq = hqOf()
  const out = all(hq.packet, RECORDS)
  const encodings = {
    md: [mdSafe(AGENT_WORDS)],
    html: [AGENT_WORDS.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;')],
    json: [JSON.stringify(AGENT_WORDS).slice(1, -1)],
  }
  for (const f of FORMATS) {
    assert.ok(out[f].text.includes(SEEDED), `${f}: the quote itself is shown, as the agent's words`)
    let rest = out[f].text
    for (const enc of encodings[f]) rest = rest.split(enc).join('')
    assert.ok(!rest.includes(SEEDED), `${f}: ${SEEDED} escaped the agent's quote`)
  }
  /* And with the agent's words left out, it is nowhere at all. */
  const cut = all(hq.packet, RECORDS, ['agent_words'])
  for (const f of FORMATS) assert.ok(!cut[f].text.includes(SEEDED), `${f} (redacted): ${SEEDED} survived`)
  /* The instrument is live: a count built from the prose would be caught. */
  const smuggled = out.md.text + `\n${SEEDED} tests ran.\n`
  let rest = smuggled.split(mdSafe(AGENT_WORDS)).join('')
  assert.ok(rest.includes(SEEDED))
})

test('(B.2a) no state word in any format that is not a typed field of the model', () => {
  const hq = hqOf()
  const haystack = typedFields(hq.packet).join(' ')
  const out = all(hq.packet, RECORDS)
  for (const f of FORMATS) {
    for (const word of STATE_WORD_SET) {
      if (out[f].text.includes(word)) assert.ok(haystack.includes(word), `${f} says "${word}" and the model does not`)
    }
  }
})

/* ── §B.2(b) a missing typed field reads "not recorded", in its place ──── */

function withHistory(mutate) {
  const change = clone(CHANGE)
  mutate(change.decisions.history[0])
  return hqOf({ change })
}

const DECISION_FIELDS = [
  ['actor', (h) => { delete h.actor }, D.fields.actor],
  ['decided_at', (h) => { delete h.decided_at }, D.fields.when],
  ['scope_version', (h) => { delete h.scope_version }, D.fields.scope],
  ['target_revision', (h) => { delete h.target_revision }, D.fields.version],
]

for (const [field, mutate, label] of DECISION_FIELDS) {
  test(`(B.2b) a decision with no ${field}: the line in its place says "${label}: not recorded." in every format — not blank, not stale`, () => {
    const hq = withHistory(mutate)
    const out = all(hq.packet, RECORDS)
    const sentence = `${label}: not recorded.`
    for (const f of FORMATS) {
      const t = out[f].text
      assert.ok(t.includes(D.decision_incomplete), `${f}: the incomplete decision is not said`)
      assert.ok(t.includes(sentence), `${f}: "${sentence}" is missing`)
      assert.doesNotMatch(t, /\bundefined\b|\bnull\b(?!,|\s*[}\]])/, `${f}: a missing value printed as a word`)
      assert.doesNotMatch(t, /by {2}on|on ,|version \./, `${f}: a blank where the value was`)
    }
    /* In place: in the Markdown, under the first decision's row, before the
     * next row of the list. The boundary used to be the literal sentence
     * "- Approval withdrawn by"; main's T-5-2 (`33425a1b`) replaced that copy
     * with `withdrawn_by_version` ("The approval on <v> was withdrawn by …"),
     * so the literal silently stopped matching and `next` became -1. The
     * boundary is now the next Markdown list item, which is what "in its
     * place" actually means and cannot go stale when copy changes. */
    const md = out.md.text
    const row = md.indexOf('- ' + D.decision_incomplete)
    const next = md.indexOf('\n- ', row)
    const at = md.indexOf(sentence, row)
    assert.ok(next > row, 'there is no second decision row to be in front of')
    assert.ok(row > 0 && at > row && at < next, 'the not-recorded line is not in the place of the field')
    /* And typed as null in the JSON, named in not_recorded. */
    const j = JSON.parse(out.json.text)
    assert.ok(j.not_recorded.includes(sentence))
  })
}

test('(B.2b) the run version: a run whose record does not say which version it ran on says that, in every format', () => {
  const coverage = clone(COVERAGE)
  delete coverage.criteria[0].items[0].applies
  const out = all(hqOf({ coverage }).packet, RECORDS)
  for (const f of FORMATS) assert.ok(out[f].text.includes(MD.run_version_none), `${f}: the missing run version is not said`)
  assert.equal(JSON.parse(out.json.text).runs[0].version, null)
})

/* The cases the export CANNOT tell from a real value, because `change-model.js`
 * (or `coverage-view.js` beneath it) turns the missing field into a default
 * before the export sees it. Each is named here and filed as an interface
 * need; the assertion pins today's behaviour so a fix upstream turns it red
 * and the entry can be closed. */

test('(B.2b) NAMED CASE — INTERFACE-NEEDS 3: a criterion with no number reaches the export as 0', () => {
  const coverage = clone(COVERAGE)
  delete coverage.criteria[1].index
  const j = JSON.parse(renderExport(hqOf({ coverage }).packet, RECORDS, { format: 'json' }).text)
  assert.equal(j.criteria[1].n, 0, 'coverage-view.js defaults a missing index to 0; the export cannot tell it from a real value')
})

test('(B.2b) NAMED CASE — INTERFACE-NEEDS 4: an accepted list with no criteria field reaches the export as a count of 0', () => {
  const change = clone(CHANGE)
  delete change.accepted_scope.criteria
  const p = hqOf({ change }).packet
  assert.equal(p.acceptedCount, 0, 'a missing list and an empty list are the same number by the time the export sees them')
})

test('(B.2b) NAMED CASE — INTERFACE-NEEDS 5: the standing decision\'s header line with no actor carries the model\'s blank', () => {
  const change = clone(CHANGE)
  change.decisions.history = [change.decisions.history[0]]
  change.decisions.standing = { ...change.decisions.history[0] }
  delete change.decisions.standing.actor
  const p = hqOf({ change }).packet
  assert.match(p.decisionBy, /by {2}on/, 'the model fills a blank; hq.packet carries no typed actor for this line')
})

/* ── §B.3 a record that cannot be read is named in its place ──────────── */

function blockOf(md, rowStart) {
  const at = md.indexOf(rowStart)
  assert.ok(at >= 0, `row not found: ${rowStart}`)
  const rest = md.slice(at + rowStart.length)
  const end = rest.search(/\n(- |## )/)
  return rest.slice(0, end < 0 ? undefined : end)
}

test('(B.3) an approval copy this Mac would not open is named on that approval\'s line, in every format', () => {
  const records = clone(RECORDS)
  records.decisions[0].standing = 'permission_denied'
  records.totals = { records: 3, readable: 2, not_readable: 1 }
  const hq = hqOf()
  const out = all(hq.packet, records)
  const gap = `${D.unreadable_label}:** ${D.gap.decision.replace('{why}', D.why.permission_denied)}`
  const row = hq.packet.decisionHistory[0].line
  assert.ok(blockOf(out.md.text, '- ' + row).includes(gap), 'Markdown: the gap is not under its row')
  /* HTML: inside the same list item as the row. */
  const html = out.html.text
  const li = html.slice(html.lastIndexOf('<li>', html.indexOf(row.replace(/'/g, '&#39;'))))
  assert.ok(li.slice(0, li.indexOf('</li>')).includes(D.why.permission_denied), 'HTML: the gap is not inside its row')
  /* JSON: on the row. */
  const j = JSON.parse(out.json.text)
  assert.ok(j.sections.find((s) => s.id === 'decisions').row_gaps[0].some((g) => g.includes(D.why.permission_denied)))
  /* And the summary says one could not be read — in addition, not instead. */
  for (const f of FORMATS) assert.ok(out[f].text.includes('1 of the 3 records this rests on could not be read'), `${f}: no summary`)
  for (const f of FORMATS) assert.ok(!out[f].text.includes('Every one of the'), `${f}: a readable all-clear beside a gap`)
})

test('(B.3) a run a link points at that the ledger does not hold is named on the line that links it and under runs', () => {
  const records = clone(RECORDS)
  records.evidence = [{ kind: 'check_run', evidence_id: 'evb-gone', session_id: 's1', criteria: [2], name: null, standing: 'dangling_reference', execution_recorded: null }]
  const hq = hqOf()
  const out = all(hq.packet, records)
  const text = D.gap.evidence.replace('{why}', D.why.dangling_reference)
  const secondLine = hq.packet.sections[0].rows[1].text
  assert.ok(blockOf(out.md.text, '- ' + secondLine).includes(text), 'not on the second criterion\'s line')
  assert.ok(!blockOf(out.md.text, '- ' + hq.packet.sections[0].rows[0].text).includes(text), 'placed on the wrong line')
  /* No run carries its name, so under runs it is at the top of the section, labelled. */
  const runs = out.md.text.slice(out.md.text.indexOf('## ' + MD.runs_heading))
  assert.ok(runs.indexOf(D.section_gap) > 0 && runs.indexOf(D.section_gap) < runs.indexOf('\n- '), 'not at the top of the runs section')
})

test('(B.3) a conversation file that is gone is named at the top of both sections that rest on it', () => {
  const records = clone(RECORDS)
  records.transcripts[0].standing = 'missing'
  const out = all(hqOf().packet, records)
  const text = D.gap.transcript.replace('{why}', D.why.missing)
  const hq = hqOf()
  for (const [heading, id] of [[MD.said_heading, 'said'], [MD.findings_heading, 'findings']]) {
    const section = out.md.text.slice(out.md.text.indexOf('## ' + heading))
    const first = section.indexOf('\n- ' + mdSafe(hq.packet.sections.find((s) => s.id === id).rows[0].text))
    assert.ok(section.indexOf(text) > 0 && section.indexOf(text) < first, `${heading}: the gap is not at the top`)
  }
  for (const f of FORMATS) assert.ok(out[f].text.includes(D.why.missing), `${f}: the gap is not in the file`)
})

test('(B.3) a changed conversation file says how many lines changed and are gone — typed numbers from the records answer', () => {
  const records = clone(RECORDS)
  Object.assign(records.transcripts[0], { standing: 'content_changed', lines_changed: 2, lines_gone: 5 })
  const md = renderExport(hqOf().packet, records, { format: 'md' }).text
  assert.ok(md.includes('2 of its 12 lines changed and 5 are gone'))
})

test('(B.3) records that were not checked are said to be unchecked, never readable', () => {
  const out = all(hqOf().packet, null)
  for (const f of FORMATS) {
    assert.ok(out[f].text.includes(D.records_unchecked.split('{reason}')[0]), `${f}: unchecked is not said`)
    assert.ok(!out[f].text.includes('Every one of the'), `${f}: readable was guessed`)
  }
  assert.equal(JSON.parse(out.json.text).records_checked, false)
})

test('(B.3) an unknown standing word is said as unknown, not dropped and not read as fine', () => {
  const records = clone(RECORDS)
  records.decisions[0].standing = 'eaten_by_a_grue'
  const md = renderExport(hqOf().packet, records, { format: 'md' }).text
  assert.ok(md.includes(D.why.unknown))
})

/* ── §B.4 redaction is a labelled derivative ──────────────────────────── */

test('(B.4) the derivative opens with the label naming each class and how many; every removal is marked; nothing removed survives', () => {
  const hq = hqOf()
  const full = all(hq.packet, RECORDS)
  const cut = all(hq.packet, RECORDS, CLASSES)
  const removed = [AGENT_WORDS, FAILURE, 'checkout/', DECISION_PATH, TRANSCRIPT]
  for (const f of FORMATS) {
    const t = cut[f].text
    for (const r of removed) {
      if (f === 'md') assert.ok(full.md.text.includes(mdSafe(r)), `the full file lacks ${r}, so its absence below proves nothing`)
      assert.ok(!t.includes(r), `${f}: a removed value survived: ${r}`)
      assert.ok(!t.includes(JSON.stringify(r).slice(1, -1)), `${f}: a removed value survived, escaped: ${r}`)
    }
    for (const c of CLASSES) {
      const marker = f === 'html' ? D.markers[c].replace(/'/g, '&#39;') : f === 'json' ? JSON.stringify(D.markers[c]).slice(1, -1) : mdMarker(D.markers[c])
      assert.ok(t.includes(marker), `${f}: no in-place marker for ${c}`)
    }
    assert.match(cut[f].filename, /-redacted\.(md|html|json)$/)
  }
  /* Opens with the label. */
  assert.ok(cut.md.text.startsWith('> ' + D.leaves_out + '\n'), 'the Markdown does not open with the label')
  for (const [c, n] of [['agent_words', 1], ['paths', 4], ['failure_output', 1]]) {
    assert.ok(cut.md.text.includes(`> - ${X.classes[c]}: ${n} left out`), `the label does not count ${c}`)
  }
  const body = cut.html.text.slice(cut.html.text.indexOf('<main>') + 6)
  assert.ok(body.startsWith('\n<section class="label">'), 'the HTML does not open with the label')
  const j = JSON.parse(cut.json.text)
  assert.deepEqual(j.redaction.removed, [{ class: 'agent_words', count: 1 }, { class: 'paths', count: 4 }, { class: 'failure_output', count: 1 }])
  assert.equal(j.redaction.redacted, true)
})

test('(B.4) traceable provenance: derived_from_sha256 is the full file\'s hash, and the full file is byte-identical either way', () => {
  const hq = hqOf()
  const before = all(hq.packet, RECORDS)
  const snapshot = JSON.stringify(hq.packet)
  const cut = all(hq.packet, RECORDS, ['agent_words'])
  const after = all(hq.packet, RECORDS)
  assert.equal(JSON.stringify(hq.packet), snapshot, 'redacting mutated the model')
  for (const f of FORMATS) {
    assert.equal(after[f].text, before[f].text, `${f}: the full file changed`)
    const sha = createHash('sha256').update(before[f].text, 'utf8').digest('hex')
    assert.equal(cut[f].redaction.derived_from_sha256, sha, `${f}: provenance does not point at the full file`)
    assert.ok(cut[f].text.includes(sha), `${f}: the provenance is not in the file`)
  }
  assert.equal(JSON.parse(cut.json.text).redaction.derived_from_sha256, createHash('sha256').update(before.json.text).digest('hex'))
})

test('(B.4) leaving one class out leaves the others in', () => {
  const cut = renderExport(hqOf().packet, RECORDS, { format: 'md', redact: ['paths'] }).text
  assert.ok(cut.includes(mdSafe(FAILURE)) && cut.includes(mdSafe(AGENT_WORDS)))
  assert.ok(!cut.includes('checkout/'))
  assert.ok(cut.includes(D.paths_limit), 'the limit of path removal is not said')
})

test('(B.4) a path the record holds is left out wherever it appears, in any case — a run\'s name carries its folder lower-cased', () => {
  const coverage = clone(COVERAGE)
  coverage.criteria[0].items[0].name = 'bash:/users/synthetic/shop:npm test'
  const cut = all(hqOf({ coverage }).packet, RECORDS, ['paths'])
  for (const f of FORMATS) {
    assert.doesNotMatch(cut[f].text, /\/users\/synthetic\/shop/i, `${f}: the folder survived inside the run's name`)
  }
  /* EXPORT-MD-ESCAPE-R2 (D-050): the old expected value was `bash:` + marker + `:npm test`, which
   * leaves a `]:` in the file — the closer of a reference definition. The marker's own `[` does not
   * make it safe: the marker is put in place of a value BEFORE the text is escaped, so a recorded
   * backslash in front of the value escapes that `[`, and the marker's `]:` then closes a label a
   * recorded `[` opened on an earlier line (a test in packet-export-md-escape.test.js builds it). No
   * `]:` is left in the file now; the colon still reads as a colon. */
  /* EXPORT-MD-ESCAPE-R4 (D-050): and the marker's brackets are escaped: `bash:\\[left out: a file or folder path\\]\\:npm test`. */
  assert.ok(cut.md.text.includes('bash:' + mdMarker(D.markers.paths) + '\\:npm test'), 'the run is still named, with its folder marked')
})

test('sha256 matches node\'s on the standard vectors and on unicode', () => {
  for (const s of ['', 'abc', 'Done — “quoted” 🍰\n', 'a'.repeat(1000)]) {
    assert.equal(sha256Hex(s), createHash('sha256').update(s, 'utf8').digest('hex'), JSON.stringify(s.slice(0, 20)))
  }
})

/* ── §B.6 HTML safety ─────────────────────────────────────────────────── */

const ALLOWED_TAGS = new Set(['html', 'head', 'meta', 'title', 'style', 'body', 'main', 'section', 'h1', 'h2', 'p', 'ul', 'li', 'strong', 'hr'])

test('(B.6) the agent\'s words, a command line and a check\'s output reach the HTML only escaped', () => {
  for (const redact of [[], ['paths']]) {
    const html = renderExport(hqOf().packet, RECORDS, { format: 'html', redact }).text
    assert.ok(!/<script/i.test(html), 'a script element')
    assert.ok(!/<img/i.test(html), 'an image element')
    /* Every element in the file is one this module writes; nothing the
     * record said became markup. */
    const tags = new Set([...html.matchAll(/<([a-z][a-z0-9]*)\b/gi)].map((m) => m[1].toLowerCase()))
    for (const t of tags) assert.ok(ALLOWED_TAGS.has(t), `an element the renderer never writes: <${t}>`)
    assert.doesNotMatch(html, /<[^>]*\son[a-z]+\s*=/i, 'an event handler attribute')
    assert.ok(html.includes('&lt;script&gt;alert(1)&lt;/script&gt;'), 'the agent\'s script is not shown, escaped')
    assert.ok(html.includes('&lt;img src=x onerror=alert(3)&gt;'), 'the command line is not shown, escaped')
    assert.ok(html.includes('&lt;script&gt;alert(4)&lt;/script&gt;'), 'the check\'s output is not shown, escaped')
  }
})

test('(B.6) the HTML is self-contained: no external reference of any kind, a policy that loads nothing, the constant stylesheet', () => {
  const html = renderExport(hqOf().packet, RECORDS, { format: 'html' }).text
  for (const re of [/https?:\/\//i, /<[^>]*\s(src|href|action|srcset|poster|data)\s*=/i, /@import/i, /url\(/i, /<link/i, /<script/i, /<iframe/i, /<object/i, /<embed/i, /\/\/[a-z0-9.-]+\.[a-z]{2,}/i]) {
    assert.doesNotMatch(html, re, `an external reference: ${re}`)
  }
  assert.ok(html.includes(`<meta http-equiv="Content-Security-Policy" content="${EXPORT_CSP}">`))
  assert.ok(html.includes('<style>\n' + EXPORT_CSS + '\n</style>'), 'the stylesheet is not the constant')
  assert.doesNotMatch(EXPORT_CSS, /green|#0f0|#00ff00|rgb\(0,\s*128|lime/i, 'green in the stylesheet')
})

/* ── §B.7 vocabulary ──────────────────────────────────────────────────── */

const NEVER = /\b(lied|lie|liar|hallucinated|caught|bullshit|bug-free|all clear)\b|\d\s*%|\bpercent\b/i

test('(B.7) no never-word in any format, full or redacted', () => {
  for (const redact of [[], CLASSES]) {
    const out = all(hqOf().packet, RECORDS, redact)
    for (const f of FORMATS) assert.doesNotMatch(out[f].text, NEVER, `${f}`)
  }
})

test('(B.7) the engine\'s word for the file is only on its technical part (and in its name)', () => {
  const out = all(hqOf().packet, RECORDS)
  const md = out.md.text
  const tech = md.indexOf('## ' + D.technical_heading)
  assert.ok(tech > 0)
  assert.doesNotMatch(md.slice(0, tech), /\bpacket\b/i)
  assert.match(md.slice(tech), /thruline-packet/)
  const html = htmlBody(out.html.text)
  assert.doesNotMatch(html.slice(0, html.indexOf(D.technical_heading)), /\bpacket\b/i)
})

test('(B.7) every word the file adds is from the copy file: no literal sentence in the renderer', () => {
  const src = readFileSync(new URL('./packet-export.js', import.meta.url), 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '')
  const sentences = (src.match(/(['`])[A-Za-z][a-z]+ [a-z][^'`\n]*\1/g) || []).filter((l) => !/^['`](text|application)\//.test(l))
  assert.deepEqual(sentences, [], 'a literal sentence in packet-export.js')
})

/* ── STEP-7-GIT-LINE — the taken-away file carries the move and its receipt ─ */

test('STEP-7-GIT-LINE · every format carries the moved line from hq.packet, with both commits in its technical part', () => {
  const moved = { recorded: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0', now: 'f0e1d2c3b4a5968778695a4b3c2d1e0f9a8b7c6d', changed_paths: ['src/lib.rs'], commits: 1, uncommitted_compared: false }
  const hq = hqOf({ change: { ...clone(CHANGE), code_moved: moved } })
  assert.ok(hq.codeMoved, 'no moved line derived')
  assert.deepEqual(hq.packet.codeLines, hq.codeLimit.lines, 'the packet does not carry the one derivation')
  const a = all(hq.packet, RECORDS)
  for (const f of ['md', 'html']) {
    const body = f === 'html' ? a.html.text.replace(/<[^>]*>/g, ' ').replace(/&#39;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ') : a.md.text
    assert.ok(body.includes('src/lib.rs'), `${f}: the path is not in the file`)
    assert.ok(body.includes(moved.recorded) && body.includes(moved.now), `${f}: the receipt is not in the file`)
  }
  const j = JSON.parse(a.json.text)
  assert.deepEqual(j.code.lines, hq.codeLimit.lines)
  assert.equal(j.code.moved.recorded, moved.recorded)
  assert.equal(j.code.moved.now, moved.now)
  assert.deepEqual(j.code.moved.changed_paths, ['src/lib.rs'])
  assert.equal(j.code.moved.uncommitted_compared, false)

  /* Equal heads: nothing about a move reaches the file. */
  const still = all(hqOf().packet, RECORDS)
  assert.ok(!still.md.text.includes(moved.now))
  assert.equal(JSON.parse(still.json.text).code.moved, null)
})
