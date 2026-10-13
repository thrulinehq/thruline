/* The whole story, as a file (T-5-4; `B10.1`, `B10.2`, `B10.4`, `B10.6`,
 * `B10.14`–`B10.16`, `B12.2`, `Q18`; design record §3.6, §5, §10).
 *
 * PURE. No React, no DOM, no fetch, no clock, no randomness. One function,
 * `renderExport(packet, records, {format, redact})`, over two inputs and
 * nothing else:
 *
 *  - `packet` is `hq.packet`, the taken-away record `deriveChangeModel` built
 *    (`change-model.js`). Every state word, sentence, count and version in the
 *    file is one of its fields, copied through. This module computes none of
 *    them — the §10 caution: two builders is how the page and the copied text
 *    came to say different things (T-S-5 verifier finding 1).
 *  - `records` is the served answer of `GET /api/changes/<id>/export`
 *    (`crates/core/src/change_export.rs`): whether each record the packet rests
 *    on can be read now. `null` means it was not checked, and the file says
 *    exactly that — never "readable" guessed in its absence.
 *
 * THREE RENDERINGS OF ONE DOCUMENT. Markdown and HTML are both walks of the
 * one block list `documentOf` builds, so the two cannot carry different rows.
 * JSON carries the same fields typed, plus the same rows.
 *
 * WHAT IT ADDS, AND ONLY THIS. (1) A gap, in the place of the record it
 * belongs to (`B10.16`), when a record cannot be read now. (2) The explicit
 * "not recorded" line where a decision's typed field is missing, instead of
 * the blank the model's sentence would carry. (3) For a redacted copy, the
 * label naming each class left out and how many, and a marker in each place
 * (`B10.15`). Every word of all three is `HQ_COPY.change.export`.
 *
 * HTML SAFETY follows `crates/explain/src/html.rs`'s header, not a new rule:
 * self-contained (no script, no stylesheet link, no font, no image, no
 * external reference of any kind) and everything escaped, all five
 * characters, because the agent's words, command lines and what a check
 * printed are attacker-influenced. A Content-Security-Policy of
 * `default-src 'none'` sits on top, so a mistake here still loads nothing and
 * runs nothing.
 *
 * MARKDOWN SAFETY (EXPORT-MD-ESCAPE). The Markdown is the one rendering with no
 * such policy under it: a file a person shares is opened in whatever renders
 * Markdown, and many renderers pass inline HTML through. So recorded text is
 * made safe here too, by ONE helper (`escapeMarkdown`), applied once, at the
 * one place text enters a Markdown line (`markdownOf`) —
 * not per field, because the model's sentences interleave its own words with
 * recorded ones and no field boundary says which is which. Ordinary text reads
 * the same; what changes is listed on the helper. The rule that makes it safe
 * does not depend on which renderer opens the file: no recorded text is ever
 * written as a live code span (EXPORT-MD-ESCAPE-R3), a live bracket, a live `<`
 * or a bare address a renderer would make a link of (EXPORT-MD-ESCAPE-R4).
 * Two other writers (`verdict-model.js`, `shell.jsx`) use the same helper.
 */

import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'

const X = HQ_COPY.change.export
const D = X.doc

/** The file's own name for what it is. Technical face and file name only. */
export const EXPORT_FORMAT = 'thruline-packet'
export const EXPORT_SCHEMA_VERSION = 1
export const FORMATS = ['html', 'md', 'json']
export const CLASSES = ['agent_words', 'paths', 'failure_output']
const EXT = { html: 'html', md: 'md', json: 'json' }
const MIME = {
  html: 'text/html;charset=utf-8',
  md: 'text/markdown;charset=utf-8',
  json: 'application/json;charset=utf-8',
}

const list = (v) => (Array.isArray(v) ? v : [])
const str = (v) => (typeof v === 'string' && v.trim() ? v : null)

/* ── the one entry point ──────────────────────────────────────────────── */

/**
 * @param {object} packet `hq.packet`
 * @param {object|null} records the served `/export` document, or null
 * @param {{format?: string, redact?: string[]}} [options]
 * @returns {{filename: string, mime: string, text: string, redaction: object}}
 */
export function renderExport(packet, records, options = {}) {
  const format = FORMATS.includes(options.format) ? options.format : 'html'
  const classes = CLASSES.filter((c) => list(options.redact).includes(c))
  if (!classes.length) return renderOne(packet, records, format, { redacted: false, removed: [], derived_from_sha256: null })
  /* The derivative is made FROM the full file and never instead of it: the
   * full text is rendered here exactly as it would be on its own, and only its
   * fingerprint travels into the derivative. */
  const full = renderOne(packet, records, format, { redacted: false, removed: [], derived_from_sha256: null })
  const cut = redactInputs(packet, records, classes)
  return renderOne(cut.packet, cut.records, format, {
    redacted: true,
    removed: cut.removed,
    derived_from_sha256: sha256Hex(full.text),
  })
}

function renderOne(packet, records, format, redaction) {
  const p = packet || {}
  const text = format === 'json'
    ? jsonOf(p, records, redaction)
    : format === 'md'
      ? markdownOf(documentOf(p, records, redaction))
      : htmlOf(documentOf(p, records, redaction), p)
  return { filename: filenameOf(p, format, redaction.redacted), mime: MIME[format], text, redaction }
}

/** `thruline-packet-<change-slug>-<version7>[-redacted].<ext>` */
export function filenameOf(p, format, redacted) {
  const slug = String((p && p.title) || '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 48)
    .replace(/-+$/g, '') || 'change'
  const version = (p && str(p.versionShort)) ? String(p.versionShort).replace(/[^A-Za-z0-9_-]/g, '').slice(0, 7) : 'unversioned'
  return `${EXPORT_FORMAT}-${slug}-${version}${redacted ? '-redacted' : ''}.${EXT[format] || 'html'}`
}

/* ── where each gap goes (B10.16: in that place) ──────────────────────── */

function whyOf(standing) {
  return D.why[standing] || D.why.unknown
}

/**
 * Every record that could not be read, placed: on the row it belongs to when
 * the record says which row that is, and otherwise at the top of its section,
 * labelled. Never only in a footer, never dropped.
 *
 * @returns {{rows: Map<string, string[]>, sections: Map<string, string[]>}}
 *   keys `<section>:<row index>` and `<section>`
 */
export function placeGaps(p, records) {
  const rows = new Map()
  const sections = new Map()
  const toRow = (section, i, text) => {
    const k = section + ':' + i
    rows.set(k, [...(rows.get(k) || []), text])
  }
  const toSection = (section, text) => sections.set(section, [...(sections.get(section) || []), text])
  if (!records) return { rows, sections }

  /* Decisions: the served rows and the model's history are the same list in
   * the same order (`change_decision::read`, oldest first). A row is placed on
   * its history line only when the act agrees and the served full version
   * begins with the short one the line names; anything else goes to the top
   * of the section rather than onto a guess. Nothing is cut here. */
  const history = list(p.decisionHistory)
  list(records.decisions).forEach((d, i) => {
    if (!d || !d.standing || d.standing === 'readable') return
    const text = fill(D.gap.decision, { why: whyOf(d.standing) })
    const h = history[i]
    if (h && h.act === d.act && str(h.version) && String(d.version || '').startsWith(h.version)) toRow('decisions', i, text)
    else toSection('decisions', text)
  })

  /* Evidence a criterion link names: on each line that links it, and — for a
   * run — on its run line when exactly one run carries its name. */
  const criteria = list(p.criteria)
  const runs = list(p.runs)
  for (const e of list(records.evidence)) {
    if (!e || e.standing === 'readable') continue
    const name = str(e.name)
    const text = name ? fill(D.gap.evidence_named, { name, why: whyOf(e.standing) }) : fill(D.gap.evidence, { why: whyOf(e.standing) })
    let placed = false
    for (const n of list(e.criteria)) {
      const i = criteria.findIndex((c) => c && c.n === n)
      if (i >= 0) { toRow('proof', i, text); placed = true }
    }
    if (!placed) toSection('proof', text)
    if (e.kind === 'check_run') {
      const hits = name ? runs.map((r, i) => (r && r.name === name ? i : -1)).filter((i) => i >= 0) : []
      if (hits.length === 1) toRow('runs', hits[0], text)
      else toSection('runs', text)
    }
  }

  /* The recorded conversation: what the agent said, and what was found in
   * it, both rest on it. The packet's rows carry no reference to which
   * session they came from (INTERFACE-NEEDS 2), so the gap sits at the top of
   * both sections, labelled. */
  for (const t of list(records.transcripts)) {
    if (!t || t.standing === 'readable') continue
    const text = t.standing === 'content_changed'
      ? fill(D.gap.transcript_changed, { rows: t.lines_changed, total: t.lines, shown: t.lines_gone })
      : fill(D.gap.transcript, { why: whyOf(t.standing) })
    toSection('said', text)
    toSection('findings', text)
  }
  return { rows, sections }
}

/* ── a decision whose typed fields are not all there ───────────────────── */

/**
 * The model's line for a decision is a sentence with its fields filled in; a
 * missing field leaves a blank (or the word "undefined") in it. Where any of
 * the typed fields is missing, the file prints each field on its own line,
 * and the missing ones as "not recorded" — never the blank.
 */
export function decisionRow(h, modelText) {
  const F = D.fields
  const approved = h && h.act === 'approved'
  const missing = !h || !str(h.act) || !str(h.actor) || !str(h.when) || !str(h.version) || (approved && typeof h.scopeVersion !== 'number')
  if (!missing) return { text: modelText, lines: [], notRecorded: [] }
  const notRecorded = []
  const field = (what, value) => {
    if (value === null || value === undefined || value === '') {
      const line = fill(D.not_recorded, { what })
      notRecorded.push(line)
      return line
    }
    return fill(D.field, { what, value: String(value) })
  }
  const act = h && str(h.act) ? (D.acts[h.act] || D.acts.unknown) : null
  const lines = [
    field(F.act, act),
    field(F.actor, h && str(h.actor)),
    field(F.when, h && str(h.when)),
    field(F.version, h && str(h.version)),
    ...(approved || !h ? [field(F.scope, h && typeof h.scopeVersion === 'number' ? h.scopeVersion : null)] : []),
  ]
  return { text: D.decision_incomplete, lines, notRecorded }
}

/* ── the document, as blocks ──────────────────────────────────────────── */

/**
 * The one block list both Markdown and HTML walk. Every string in it is a
 * field of `packet` or `records`, or a copy string.
 */
export function documentOf(p, records, redaction) {
  const out = []
  const push = (b) => out.push(b)

  /* A redacted copy OPENS with what it leaves out (`B10.15`). */
  if (redaction && redaction.redacted) {
    push({ t: 'label', text: D.leaves_out, items: list(redaction.removed).map((r) => fill(D.left_out, { what: X.classes[r.class], n: r.count })) })
    if (list(redaction.removed).some((r) => r.class === 'paths')) push({ t: 'p', text: D.paths_limit, cls: 'muted' })
  }

  push({ t: 'h1', text: str(p.title) || D.no_title })
  for (const line of [p.headline, p.version, p.coversLine, p.readinessBlocker, p.readinessLine]) if (str(line)) push({ t: 'p', text: line })
  if (list(p.readinessRests).length) push({ t: 'ul', items: list(p.readinessRests).map((r) => ({ text: r, lines: [], gaps: [] })) })
  /* BRIEF-COUNT-AGREES-R2 — after the list of what still needs evidence, each on
   * its own: what is missing that is not one of the lines, and the lines that
   * were set aside under their own heading, never inside that list. */
  for (const line of list(p.readinessAlso)) if (str(line)) push({ t: 'p', text: line })
  if (str(p.readinessSetAsideHeading) && list(p.readinessSetAside).length) {
    push({ t: 'p', text: p.readinessSetAsideHeading })
    push({ t: 'ul', items: list(p.readinessSetAside).map((r) => ({ text: r, lines: [], gaps: [] })) })
  }
  /* STEP-7-GIT-LINE — what the Change screen says about the code, from the
   * same lines: the move read from git first when there was one, then the
   * record's limit. After the ready sentence's own list, never between them. */
  for (const line of list(p.codeLines)) if (str(line)) push({ t: 'p', text: line })
  for (const line of [p.decision, p.decisionListMoved, p.decisionBy]) if (str(line)) push({ t: 'p', text: line })
  /* READY-BESIDE-APPROVED repair (T-5-3, `B10.9`): what the standing approval
   * was given under, under it, one line each — who, whose authority, why. */
  if (list(p.decisionExceptions).length) {
    if (str(p.decisionExceptionsHeading)) push({ t: 'p', text: p.decisionExceptionsHeading })
    push({ t: 'ul', items: list(p.decisionExceptions).map((text) => ({ text, lines: [], gaps: [] })) })
  }

  const gaps = placeGaps(p, records)
  const history = list(p.decisionHistory)
  for (const section of list(p.sections)) {
    push({ t: 'h2', text: section.title })
    const top = gaps.sections.get(section.id) || []
    if (top.length) push({ t: 'gap', text: D.section_gap, items: top })
    const rows = list(section.rows)
    if (!rows.length) { if (str(section.none)) push({ t: 'p', text: section.none }); continue }
    push({
      t: 'ul',
      items: shapedRows(section, history).map((row, i) => ({ ...row, gaps: gaps.rows.get(section.id + ':' + i) || [] })),
    })
  }

  /* Whether the records behind all of this could be read, said once more in
   * one place — in ADDITION to each gap above, never instead of it. */
  push({ t: 'h2', text: D.records_heading })
  const summary = recordsSummary(records)
  if (summary.checked) push({ t: 'p', text: summary.checked })
  push(summary.gap ? { t: 'gap', text: summary.text, items: [] } : { t: 'p', text: summary.text })

  push({ t: 'hr' })
  push({ t: 'p', text: str(p.footer) || D.made })

  /* The technical face: the only place in the file the engine's words live. */
  push({ t: 'h2', text: D.technical_heading, cls: 'technical' })
  const tech = [
    fill(D.field, { what: D.tech_format, value: `${EXPORT_FORMAT} ${EXPORT_SCHEMA_VERSION}` }),
    fill(D.field, { what: D.tech_change, value: str(p.id) || D.tech_version_none }),
    fill(D.field, { what: D.tech_version, value: str(p.versionShort) || D.tech_version_none }),
    fill(D.field, { what: D.tech_masking, value: (records && records.masking && str(records.masking.statement)) || D.tech_masking_none }),
  ]
  if (records) {
    for (const d of list(records.decisions)) if (d && d.path) tech.push(fill(D.field, { what: D.tech_records, value: `${d.path} (${d.standing || 'none'})` }))
    for (const e of list(records.evidence)) if (e) tech.push(fill(D.field, { what: D.tech_records, value: `${e.kind} ${e.evidence_id} (${e.standing})` }))
    for (const t of list(records.transcripts)) if (t) tech.push(fill(D.field, { what: D.tech_records, value: `${t.file} (${t.standing})` }))
  }
  /* STEP-7-GIT-LINE — the move's receipt: the two commits, and that edits
   * not yet committed were not compared. */
  if (p.codeMoved) {
    tech.push(fill(D.field, { what: str(p.codeMoved.heading), value: str(p.codeMoved.receipt) }))
    tech.push(str(p.codeMoved.uncommitted))
  }
  /* UNRECORDED-WORK-NO-QUOTE — where the agent said it, in its own log: one
   * level down from the said section's sentence about it. */
  for (const receipt of list(p.saidUnrecorded && p.saidUnrecorded.receipts)) {
    if (str(receipt)) tech.push(fill(D.field, { what: D.tech_agent_said, value: receipt }))
  }
  if (redaction && redaction.redacted) tech.push(fill(D.field, { what: D.tech_derived_from, value: redaction.derived_from_sha256 }))
  push({ t: 'ul', items: tech.map((text) => ({ text, lines: [], gaps: [] })), cls: 'technical' })
  return out
}

/**
 * The one sentence about the records as a whole, from the served totals. Said
 * as a gap whenever anything could not be read OR nothing was checked.
 */
export function recordsSummary(records) {
  if (!records) return { checked: null, text: fill(D.records_unchecked, { reason: D.records_unchecked_reason }), gap: true }
  const checked = str(records.checked_at) ? fill(D.records_checked, { when: records.checked_at }) : null
  const totals = records.totals || {}
  if (typeof totals.records !== 'number' || totals.records === 0) return { checked, text: D.records_none, gap: false }
  if (totals.not_readable) return { checked, text: fill(D.records_some, { problems: totals.not_readable, total: totals.records }), gap: true }
  return { checked, text: fill(D.records_all, { total: totals.records }), gap: false }
}

/** A section's rows as the file prints them: the model's, except a decision missing a typed field. */
function shapedRows(section, history) {
  return list(section.rows).map((row, i) => {
    const shaped = section.id === 'decisions' ? decisionRow(history[i], row.text) : { text: row.text, lines: [] }
    const out = { text: shaped.text, lines: [...list(row.lines), ...shaped.lines] }
    if (list(row.receipts).length) out.receipts = list(row.receipts)
    return out
  })
}

/* ── Markdown ─────────────────────────────────────────────────────────── */

/* The characters a Markdown parser, or the tools around one, may end a line at. */
const LINE_BREAKS = /[\r\n\v\f\u001c-\u001e\u0085\u2028\u2029]+/

/* A text with line breaks in it, on one line: each run of breaks, and the
 * spaces around it, becomes one space. Written as a split and a trim rather
 * than one regular expression over `[ \t]*` + breaks + `[ \t]*`, which is
 * quadratic on a long run of spaces — and a check's printed output can hold one. */
function oneLine(s) {
  const parts = s.split(LINE_BREAKS)
  if (parts.length === 1) return s
  const last = parts.length - 1
  return parts.map((p, i) => (i === 0 ? p.trimEnd() : i === last ? p.trimStart() : p.trim())).filter(Boolean).join(' ')
}

/* What begins a block when it is the first thing on a line: a heading, a
 * quote, a list item, a thematic break, a setext underline, a code fence, and
 * a table's delimiter row (`-|-`, `|---|`, `:-:`: only `|`, `:`, `-` and
 * spaces, with a `-` — the line under a header row that turns the line above
 * it into a table, in `marked` even when that line has no `|` in it). A
 * reference or footnote definition and a task-list box (`[x]`, which a
 * renderer draws as a ticked checkbox — a "done" the record never wrote) begin
 * with a `[`, and every `[` is escaped (EXPORT-MD-ESCAPE-R4), so neither has a
 * guard of its own any more. */
const LINE_START = [
  /^#{1,6}(?=[ \t]|$)/,
  /^>/,
  /^[-+*](?=[ \t]|$)/,
  /^(?:[-*_][ \t]*){3,}$/,
  /^[-=]+[ \t]*$/,
  /^(?:`{3,}|~{3,})/,
  /^[|: \t-]*-[|: \t-]*$/,
]

/**
 * Recorded text, made safe to put on a line of the Markdown file: it is shown
 * as the words it is, and cannot be read as markup by a renderer that passes
 * HTML through, that follows links, or that fetches images.
 *
 * THE RULE THAT MAKES IT SAFE WITHOUT KNOWING THE RENDERER: no recorded text is
 * ever written as a live construct of any kind (code spans in
 * EXPORT-MD-ESCAPE-R3, everything else in R4). Rounds 1 to 3 decided which
 * constructs were harmless enough to leave alone — a code span that looked
 * plain, a link with an https target — and each time a renderer read one of
 * them differently from the model of it. `marked` reads a link label's code
 * span only at the top level, and masks a backslash and the punctuation after
 * it before it masks a span (`[[a `]x](javascript:alert(1))`` and
 * `_a `<img src=x onerror=…>_\`` were a live link and a live tag in it, and the
 * second ran a script in a browser). And it un-escapes `\[` and `\]` inside a
 * link label and lexes the label again, so `[![a\](https://…/p.png)](https://…/x)`
 * — every bracket "handled", and an https target the guard let through — was a
 * link holding an IMAGE that a browser fetches when the file is opened. Deciding
 * which constructs are plain enough is renderer-quirk hunting, and it cannot be
 * won by a model of a renderer's inline parser, so this rule does not depend on
 * one.
 *
 * WHAT CHANGES, and nothing else does (`packet-export-md-escape.test.js` holds
 * every line of this list):
 *
 *  - every `[` and every `]` is written with a backslash in front (`\[`, `\]`),
 *    whatever follows it. Recorded `[text](url)` is never a live link,
 *    `![alt](url)` is never an image, `[label]: url` is never a reference
 *    definition and `[x]` is never a task-list box, because no bracket is live
 *    for a renderer to read, or to un-escape. A backslash the person recorded in
 *    front of a bracket is theirs and stays (`\[` is left as it is); a backslash
 *    and the punctuation after it are one unit, as a renderer reads them.
 *  - every `<` is written `&lt;` (after a backslash of the person's as well: `\<`
 *    is written `\&lt;`). A `<` begins a tag, a comment, a processing
 *    instruction, a declaration or an angle autolink — `<https://…>`, and
 *    `<1@b.co>`, a `mailto:` link that a digit or any of `_ . - # $ % & ' * + =
 *    { | } ~ ^` before the `@` makes as a letter does. A guard that listed what
 *    could follow the `<` missed the ones it had not thought of. `a < b` and `<3`
 *    are shown as they were recorded.
 *  - `//` is written `/\/`, the second slash escaped. `markdown-it` (linkify on)
 *    makes a link of a bare `//host`, which a browser resolves to `file://host`
 *    in a local file. And an address's `://` makes every renderer that
 *    autolinks it read on to the end of the word, taking an escape's backslash
 *    into the link's text and its address (`https://h.co/a\[b` is a link to
 *    `https://h.co/a%5C%5Bb`), so no `://` and no `//` is left in the file at
 *    all. `https://h.co/a` is shown as `https://h.co/a` and is not a link. A
 *    backslash-escaped character is a token of its own, which linkify does not
 *    read across; `packet-export-md-escape.test.js` shows that in the real
 *    renderers.
 *  - the `@` of a mail address — one that follows a character a mail address's
 *    name can end in, in any of the three renderers — is written `\@`, and the
 *    `.` of `www.` is written `www\.`: a bare mail address is a `mailto:` link,
 *    and a `www.` address is a link that takes an escape's backslash into its
 *    text, in `marked`.
 *  - a line break inside a recorded text is a space, so the text stays on the
 *    line it was placed on and cannot start a heading, a list item, a quote, a
 *    rule or a code fence of its own; leading whitespace goes for the same
 *    reason. And where the text itself begins with one of those markers, or is
 *    nothing but a table's delimiter row (`-|-`, `|---|`, `:-:`), the first
 *    character is escaped.
 *  - `]:` is written `\]\:`. A reference definition is `[label]: destination`
 *    and its label may run over several lines and fields (the round-1 defect:
 *    `[a` in an accepted line, `x]: javascript:…` in what a check printed). No
 *    `[` is live now, so none can open a label, and no `]:` is left as well. The
 *    colon still reads as a colon: `error[E0308]: mismatched types` is shown as
 *    it was recorded.
 *  - every backtick is written `\``. Nothing in the file is a code span, so no
 *    renderer has a span to find, and a backtick one field leaves unpaired
 *    cannot pair with a backtick in another.
 *
 * CODE SPANS ARE WRITTEN AS TEXT. What a person recorded between backticks
 * (read as CommonMark reads it: a run of N backticks and the next run of exactly
 * N) is written as text with every character that could be read as markup, or
 * that a renderer would consume, escaped, so that a reader sees exactly the
 * characters that were recorded, backticks included — in the CommonMark
 * reference implementation, `marked` and `markdown-it` alike
 * (`packet-export-md-escape.test.js` renders all three). Inside such a span
 * `\`, `` ` ``, `(`, `)`, `*`, `_`, `!` and `~` are each written with a
 * backslash in front, an `&` that could begin an entity is `\&`, every `<` is
 * `&lt;`, and the brackets, the `//`, the `@`, the `www.` and the `]:` are
 * written as they are outside a span. What it costs: the code FONT. The words a
 * person put in a span are shown as ordinary text, and the file's raw text is
 * more backslashes than it was.
 *
 * WHAT IT DOES NOT CLAIM. It is written for CommonMark and GitHub-flavoured
 * Markdown. It does not neutralise emphasis, tables, or the attribute or
 * template syntaxes some other flavours add (`{…}` after a link, `{{ … }}`),
 * outside a span. None of those is HTML, a link or an image in the two flavours
 * named; a reader who opens the file in a flavour that adds them is reading
 * plain words there, not this export's own markup. A link a person recorded is
 * shown as the text they typed: none is live, whatever its address. A renderer
 * may still make a link of a bare `example.com` or `www.` address it finds by
 * itself; that link's words are its address.
 *
 * `&` outside a span is NOT escaped: a Markdown renderer treats an entity as
 * text, and no link is left for an entity to change the address of. Leaving `&`
 * alone keeps `R&D` and `&amp;` exactly as they were recorded. A backslash is
 * only ever inserted in front of a character this function has just matched (a
 * bracket, the second slash of `//`, an `@`, the `.` of `www.`, a backtick, the
 * colon of `]:`, or the `.` or `)` of a list number), at the very start of the
 * text, or in front of a character of a span written as text — in which every
 * backslash the person recorded is doubled first — and `<` is never escaped
 * with one.
 *
 * Idempotent, so a text that passed through it twice is not corrupted; it is
 * still applied exactly once. Linear in the length of the text.
 *
 * @param {*} value
 * @returns {string}
 */
export function escapeMarkdown(value) {
  let s = oneLine(String(value)).replace(/^\s+/, '')
  /* The line-start guards run on the text as recorded, BEFORE backticks are
   * read: they add a backslash in front of a character, which the reader below
   * takes as an escape like any other. */
  if (LINE_START.some((re) => re.test(s))) s = '\\' + s
  else s = s.replace(/^(\d{1,9})([.)])(?=[ \t]|$)/, '$1\\$2')
  return escapeBackticks(s)
}

/* A backslash and the ASCII punctuation it escapes, except `<`: a person's `\<`
 * is not kept (every `<` is written `&lt;`, so the tests can say there is none
 * left). */
const KEPT_ESCAPE = '\\\\[!-/:-;=-@[-`{-~]'

/* What may end a mail address's name, in a bare address (`marked`: letters,
 * digits and `._+-`; `markdown-it`: those and `! # $ ' * / = ? ~`): a superset.
 * Not a backtick, which is not one there and which this function itself writes
 * in front of an `@` that opens the text after a code span (so a second pass
 * would escape it, and the helper would not be idempotent). */
const MAIL_NAME = "[\\w.!#$%&'*+/=?^{|}~-]"

/* What is written escaped outside a code span AND inside one, and how: a
 * bracket, a `<`, the second slash of `//`, the colon of `]:`, the `.` of `www.`
 * and the `@` of a mail address. */
const ESCAPED = `[[\\]]|<|//|(?<=\\]):|(?<=www)\\.|(?<=${MAIL_NAME})@`

const PLAIN = new RegExp(`(${KEPT_ESCAPE})|${ESCAPED}`, 'gi')

/* What is written one way by every one of them. */
const written = (m) => (m === '<' ? '&lt;' : m === '//' ? '/\\/' : '\\' + m)

/* What the escapes below do to text that is NOT between backticks. */
function plainText(s) {
  return s.replace(PLAIN, (m, kept) => kept ?? written(m))
}

/* What a backslash escapes in CommonMark: any ASCII punctuation character. */
const ESCAPABLE = /[!-/:-@[-`{-~]/

/* What, between backticks, is written escaped so that the reader sees the
 * character itself: everything ESCAPED names; the ones a renderer reads as
 * markup or consumes (`\`, a backtick, parentheses, `*`, `_`, `!`, `~`); and an
 * `&` that could begin an entity. */
const AS_TEXT = new RegExp(`${ESCAPED}|&(?=#|[A-Za-z][A-Za-z0-9]*;)|[\\\\\`()*_!~]`, 'gi')

/* A span's content, written as text. */
function asText(content) {
  return content.replace(AS_TEXT, written)
}

/**
 * One text with its backticks read the way CommonMark reads them, and none of
 * them left live. Left to right: a backslash and the punctuation after it are
 * one unit (so an escaped backtick starts nothing); a run of N backticks starts
 * a code span if a later run of exactly N exists, and that span is written as
 * text (`asText`) between its backticks, escaped; every other run is shown as
 * the backticks it is, each escaped. Everything outside a span goes through
 * `plainText`.
 *
 * @param {string} s one line
 */
function escapeBackticks(s) {
  if (!s.includes('`')) return plainText(s)
  const runs = new Map() // run length → start of each run of that length, in order
  for (const m of s.matchAll(/`+/g)) {
    const at = runs.get(m[0].length)
    if (at) at.push(m.index)
    else runs.set(m[0].length, [m.index])
  }
  const passed = new Map() // run length → how many of those runs are behind us
  let out = ''
  let plain = ''
  let i = 0
  while (i < s.length) {
    const ch = s[i]
    if (ch === '\\') {
      if (i + 1 < s.length && ESCAPABLE.test(s[i + 1])) { plain += s.slice(i, i + 2); i += 2 } else { plain += ch; i += 1 }
      continue
    }
    if (ch !== '`') { plain += ch; i += 1; continue }
    let j = i
    while (s[j] === '`') j += 1
    const n = j - i
    /* The next run of exactly n backticks after this one, if any. */
    const at = runs.get(n) || []
    let k = passed.get(n) || 0
    while (k < at.length && at[k] < j) k += 1
    passed.set(n, k)
    if (k === at.length) { plain += '\\`'.repeat(n); i = j; continue }
    const close = at[k]
    passed.set(n, k + 1)
    out += plainText(plain) + '\\`'.repeat(n) + asText(s.slice(j, close)) + '\\`'.repeat(n)
    plain = ''
    i = close + n
  }
  return out + plainText(plain)
}

export function markdownOf(blocks) {
  const out = []
  const unreadable = D.unreadable_label
  const t = escapeMarkdown
  for (const b of blocks) {
    if (b.t === 'label') {
      out.push('> ' + t(b.text))
      for (const item of b.items) out.push('> - ' + t(item))
      out.push('')
    } else if (b.t === 'h1') { out.push('# ' + t(b.text)); out.push('') }
    else if (b.t === 'h2') { out.push(''); out.push('## ' + t(b.text)) }
    else if (b.t === 'p') out.push(t(b.text))
    else if (b.t === 'gap') {
      out.push('**' + unreadable + ':** ' + t(b.text))
      for (const item of b.items) out.push('- **' + unreadable + ':** ' + t(item))
    } else if (b.t === 'ul') {
      for (const item of b.items) {
        out.push('- ' + t(item.text))
        for (const sub of item.lines) out.push('  ' + t(sub))
        for (const g of item.gaps) out.push('  **' + unreadable + ':** ' + t(g))
      }
    } else if (b.t === 'hr') { out.push(''); out.push('---') }
  }
  return out.join('\n') + '\n'
}

/* ── HTML ─────────────────────────────────────────────────────────────── */

/** `crates/explain/src/html.rs::escape`, the same five. */
export function escapeHtml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

/* No colour carries meaning here: the one accent is a border beside a gap,
 * and the gap is labelled in words. No green anywhere. */
export const EXPORT_CSS = [
  'body{font-family:-apple-system,system-ui,sans-serif;color:black;background:white;margin:auto;max-width:46em;padding:2em 1.25em;line-height:1.5}',
  'h1{font-size:1.5em;margin:0 0 .75em}',
  'h2{font-size:1.1em;margin:1.75em 0 .5em;border-bottom:1px solid silver;padding-bottom:.2em}',
  'ul{padding-left:1.25em}',
  'li{margin:.35em 0}',
  '.sub{margin:.15em 0 0;color:dimgray}',
  '.gap{border-left:3px solid dimgray;padding:.2em .6em;margin:.4em 0;background:whitesmoke}',
  '.label{border:1px solid dimgray;padding:.5em .8em;margin:0 0 1.5em}',
  '.muted{color:dimgray}',
  '.technical{color:dimgray;font-size:.9em}',
  'hr{border:0;border-top:1px solid silver;margin:2em 0 1em}',
].join('\n')

export const EXPORT_CSP = "default-src 'none'; style-src 'unsafe-inline'"

export function htmlOf(blocks, p) {
  const e = escapeHtml
  const unreadable = D.unreadable_label
  const h = []
  h.push('<!doctype html>')
  h.push('<html lang="en"><head><meta charset="utf-8">')
  h.push(`<meta http-equiv="Content-Security-Policy" content="${EXPORT_CSP}">`)
  h.push('<meta name="viewport" content="width=device-width,initial-scale=1">')
  h.push('<title>' + e(str(p.title) || D.no_title) + '</title>')
  h.push('<style>\n' + EXPORT_CSS + '\n</style>')
  h.push('</head><body><main>')
  const gapLine = (text) => '<p class="gap"><strong>' + e(unreadable) + ':</strong> ' + e(text) + '</p>'
  for (const b of blocks) {
    if (b.t === 'label') {
      h.push('<section class="label"><p><strong>' + e(b.text) + '</strong></p><ul>' + b.items.map((i) => '<li>' + e(i) + '</li>').join('') + '</ul></section>')
    } else if (b.t === 'h1') h.push('<h1>' + e(b.text) + '</h1>')
    else if (b.t === 'h2') h.push('<h2' + (b.cls ? ' class="' + b.cls + '"' : '') + '>' + e(b.text) + '</h2>')
    else if (b.t === 'p') h.push('<p' + (b.cls ? ' class="' + b.cls + '"' : '') + '>' + e(b.text) + '</p>')
    else if (b.t === 'gap') {
      h.push(gapLine(b.text))
      if (b.items.length) h.push('<ul>' + b.items.map((i) => '<li>' + gapLine(i) + '</li>').join('') + '</ul>')
    } else if (b.t === 'ul') {
      h.push('<ul' + (b.cls ? ' class="' + b.cls + '"' : '') + '>' + b.items.map((item) => '<li>' + e(item.text)
        + item.lines.map((l) => '<p class="sub">' + e(l) + '</p>').join('')
        + item.gaps.map(gapLine).join('')
        + '</li>').join('') + '</ul>')
    } else if (b.t === 'hr') h.push('<hr>')
  }
  h.push('</main></body></html>')
  return h.join('\n') + '\n'
}

/* ── JSON ─────────────────────────────────────────────────────────────── */

/**
 * The machine-readable form (`B10.1`): typed fields of `hq.packet`, the same
 * rows the page and the Markdown print, where each gap was placed, and the
 * served records answer. A missing typed value is `null`, and the fields a
 * decision is missing are also named in `not_recorded` as the sentence the
 * other two formats print.
 */
export function jsonOf(p, records, redaction) {
  const gaps = placeGaps(p, records)
  const history = list(p.decisionHistory)
  const fieldGaps = []
  history.forEach((h, i) => {
    for (const text of decisionRow(h, h && h.line).notRecorded) fieldGaps.push({ section: 'decisions', row: i, text })
  })
  const doc = {
    format: EXPORT_FORMAT,
    schema_version: EXPORT_SCHEMA_VERSION,
    change_id: p.id ?? null,
    title: p.title ?? null,
    version: { short: p.versionShort ?? null, sentence: p.version ?? null },
    headline: p.headline ?? null,
    scope: list(p.scope),
    covers: p.coversLine ?? null,
    /* STEP-7-GIT-LINE — the lines about the code, and the move's receipt as
     * typed fields; `moved` is null when nothing was said (never "unchanged"). */
    code: {
      lines: list(p.codeLines),
      moved: p.codeMoved
        ? {
          recorded: p.codeMoved.recorded ?? null,
          now: p.codeMoved.now ?? null,
          changed_paths: list(p.codeMoved.paths),
          commits: typeof p.codeMoved.commits === 'number' ? p.codeMoved.commits : null,
          uncommitted_compared: p.codeMoved.uncommittedCompared === true,
          sentence: p.codeMoved.sentence ?? null,
        }
        : null,
    },
    accepted_count: typeof p.acceptedCount === 'number' ? p.acceptedCount : null,
    /* QUAL-F2-R3 (`Q7`) — when the list of what counts as done was accepted
     * AFTER the work it covers: the served UTC date, which list it was, and the
     * one sentence — the same fields and the same sentence `hq.packet` carries,
     * copied through and never derived here. `null` when the record does not
     * show it (never "no" or "false"): the same rows are also in `sections`. */
    accepted_after_work: p.acceptedAfterWork
      ? {
        accepted_on: p.acceptedAfterWork.acceptedOn ?? null,
        list: typeof p.acceptedAfterWork.scopeVersion === 'number' ? p.acceptedAfterWork.scopeVersion : null,
        sentence: p.acceptedAfterWork.sentence ?? null,
      }
      : null,
    readiness: {
      /* The engine's answer, as served; null only when none could be read. A
       * ready answer the file does not offer — the version stands approved —
       * says so in `shown` and `hidden_because`, present only then. */
      ready: p.readiness && !p.readiness.absent && typeof p.readiness.ready === 'boolean' ? p.readiness.ready : null,
      ...(p.readiness && p.readiness.hiddenBecause ? { shown: false, hidden_because: p.readiness.hiddenBecause } : {}),
      /* RUN-LINK-TRUTH-R5 — why this version can't be approved: the one blocker, or null. */
      blocker: p.readinessBlockerPair ?? null,
      sentence: p.readinessLine ?? null,
      rests_on: list(p.readinessRests),
      /* BRIEF-COUNT-AGREES-R2 — said after the list, each on its own. Present
       * only when there is something to say, so a file that had neither is the
       * file it was. */
      ...(list(p.readinessAlso).length ? { also: list(p.readinessAlso) } : {}),
      ...(list(p.readinessSetAside).length ? { set_aside: list(p.readinessSetAside) } : {}),
    },
    decision: {
      standing: p.decision ?? null,
      list_moved: p.decisionListMoved ?? null,
      by: p.decisionBy ?? null,
      none: p.decisionNone ?? null,
      /* What the standing approval was given under: present only when it was
       * given under something, so a file without exceptions is the file it was. */
      ...(list(p.decisionExceptionRows).length
        ? {
          exceptions_heading: p.decisionExceptionsHeading ?? null,
          exceptions: list(p.decisionExceptionRows).map((e) => ({
            line: e.line ?? null,
            obligation_id: e.obligationId ?? null,
            kind: e.kind ?? null,
            actor: e.actor ?? null,
            authority: e.authority ?? null,
            rationale: e.rationale ?? null,
          })),
        }
        : {}),
      history: history.map((h, i) => ({
        act: h.act ?? null,
        version: h.version ?? null,
        actor: h.actor ?? null,
        when: h.when ?? null,
        scope_version: typeof h.scopeVersion === 'number' ? h.scopeVersion : null,
        /* The line the file prints for this act: the model's, or — when a
         * typed field is missing — the not-recorded sentence, never a blank. */
        line: decisionRow(h, h.line ?? null).text ?? null,
        record: records ? (list(records.decisions)[i] ?? null) : null,
      })),
    },
    criteria: list(p.criteria).map((c) => ({
      n: typeof c.n === 'number' ? c.n : null,
      text: c.text ?? null,
      word: c.word ?? null,
      line: c.line ?? null,
      freshness: c.freshness ? { fresh: c.freshness.fresh ?? null, sentence: c.freshness.sentence ?? null, paths: list(c.freshness.paths) } : null,
      proof: list(c.proof).map((e) => ({
        name: e.name ?? null,
        word: e.word ?? null,
        applies: e.applies ?? null,
        failure: e.failure ? { line: e.failure.line ?? null, class: e.failure.classWord ?? null, class_text: e.failure.classText ?? null } : null,
      })),
    })),
    said: list(p.said).map((s) => ({ said: s.said ?? null, record: s.record ?? null, word: s.word ?? null })),
    runs: list(p.runs).map((r) => ({ name: r.name ?? null, word: r.word ?? null, version: r.version ?? null })),
    findings: list(p.findings).map((f) => ({ record: f.record ?? null, resolution: f.resolution ?? null })),
    not_recorded: [...list(p.notRecorded), ...fieldGaps.map((g) => g.text)],
    sections: list(p.sections).map((s) => ({
      id: s.id,
      title: s.title,
      none: s.none ?? null,
      rows: shapedRows(s, history),
      gaps: gaps.sections.get(s.id) || [],
      row_gaps: list(s.rows).map((_, i) => gaps.rows.get(s.id + ':' + i) || []),
    })),
    records: records || null,
    records_checked: Boolean(records),
    records_summary: recordsSummary(records).text,
    footer: p.footer ?? null,
    redaction: {
      redacted: Boolean(redaction && redaction.redacted),
      removed: list(redaction && redaction.removed),
      derived_from_sha256: (redaction && redaction.derived_from_sha256) || null,
    },
  }
  return JSON.stringify(doc, null, 2) + '\n'
}

/* ── redaction: a labelled derivative (B10.15, Q18) ───────────────────── */

/** The values each class removes, read from TYPED fields and nowhere else. */
export function removalsOf(p, records, classes) {
  const out = {}
  const uniq = (xs) => [...new Set(xs.filter((x) => typeof x === 'string' && x.length > 0))]
  if (classes.includes('agent_words')) out.agent_words = uniq(list(p.said).map((s) => s && s.said))
  if (classes.includes('failure_output')) {
    out.failure_output = uniq(list(p.criteria).flatMap((c) => list(c && c.proof).map((e) => e && e.failure && e.failure.line)))
  }
  if (classes.includes('paths')) {
    out.paths = uniq([
      ...list(p.scope),
      ...list(p.criteria).flatMap((c) => list(c && c.freshness && c.freshness.paths)),
      ...(records ? [records.worktree] : []),
      ...(records ? list(records.decisions).map((d) => d && d.path) : []),
      ...(records ? list(records.transcripts).map((t) => t && t.file) : []),
      ...list(p.saidUnrecorded && p.saidUnrecorded.files),
    ])
  }
  return out
}

/* Section titles and "none" sentences are copy, not record: nothing in them
 * is a removed value, and a marker there would be noise. */
const COPY_KEYS = new Set(['title', 'none'])

const escapeRegExp = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

function replaceDeep(node, pairs, key) {
  if (typeof node === 'string') {
    if (COPY_KEYS.has(key)) return node
    let s = node
    for (const [value, marker, anyCase] of pairs) {
      /* A path is matched in any case: a run's name carries its folder
       * lower-cased, and a path the record holds is the same path. */
      s = anyCase ? s.replace(new RegExp(escapeRegExp(value), 'gi'), marker) : s.split(value).join(marker)
    }
    return s
  }
  if (Array.isArray(node)) return node.map((v) => replaceDeep(v, pairs, null))
  if (node && typeof node === 'object') {
    const out = {}
    for (const [k, v] of Object.entries(node)) out[k] = replaceDeep(v, pairs, k)
    return out
  }
  return node
}

export function redactInputs(packet, records, classes) {
  const found = removalsOf(packet || {}, records, classes)
  const pairs = []
  const removed = []
  for (const c of CLASSES) {
    if (!found[c]) continue
    removed.push({ class: c, count: found[c].length })
    for (const v of found[c]) pairs.push([v, D.markers[c], c === 'paths'])
  }
  /* Longest first, so a value that contains another is removed whole. */
  pairs.sort((a, b) => b[0].length - a[0].length)
  return {
    packet: replaceDeep(packet || {}, pairs, null),
    records: records ? replaceDeep(records, pairs, null) : null,
    removed,
  }
}

/* ── sha256, so the fingerprint is computed the same way everywhere ───── */

const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
])

/** SHA-256 of the UTF-8 bytes of `text`, lowercase hex. */
export function sha256Hex(text) {
  const bytes = new TextEncoder().encode(String(text))
  const bitLen = bytes.length * 8
  const padded = new Uint8Array(((bytes.length + 9 + 63) >> 6) << 6)
  padded.set(bytes)
  padded[bytes.length] = 0x80
  const view = new DataView(padded.buffer)
  view.setUint32(padded.length - 8, Math.floor(bitLen / 0x100000000))
  view.setUint32(padded.length - 4, bitLen >>> 0)
  const H = new Uint32Array([0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19])
  const W = new Uint32Array(64)
  const rotr = (x, n) => (x >>> n) | (x << (32 - n))
  for (let off = 0; off < padded.length; off += 64) {
    for (let i = 0; i < 16; i++) W[i] = view.getUint32(off + i * 4)
    for (let i = 16; i < 64; i++) {
      const s0 = rotr(W[i - 15], 7) ^ rotr(W[i - 15], 18) ^ (W[i - 15] >>> 3)
      const s1 = rotr(W[i - 2], 17) ^ rotr(W[i - 2], 19) ^ (W[i - 2] >>> 10)
      W[i] = (W[i - 16] + s0 + W[i - 7] + s1) >>> 0
    }
    let [a, b, c, d, e, f, g, h] = H
    for (let i = 0; i < 64; i++) {
      const S1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)
      const ch = (e & f) ^ (~e & g)
      const t1 = (h + S1 + ch + K[i] + W[i]) >>> 0
      const S0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)
      const maj = (a & b) ^ (a & c) ^ (b & c)
      const t2 = (S0 + maj) >>> 0
      h = g; g = f; f = e; e = (d + t1) >>> 0; d = c; c = b; b = a; a = (t1 + t2) >>> 0
    }
    H[0] = (H[0] + a) >>> 0; H[1] = (H[1] + b) >>> 0; H[2] = (H[2] + c) >>> 0; H[3] = (H[3] + d) >>> 0
    H[4] = (H[4] + e) >>> 0; H[5] = (H[5] + f) >>> 0; H[6] = (H[6] + g) >>> 0; H[7] = (H[7] + h) >>> 0
  }
  return [...H].map((x) => x.toString(16).padStart(8, '0')).join('')
}

export default renderExport
