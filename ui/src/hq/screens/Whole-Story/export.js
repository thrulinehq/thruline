/* The whole story, as a file (C30 R-ROUTES "Export and Download"; the T-5-4
 * rules in CLAUDE.md).
 *
 * PURE: no fetch, no DOM, no clock. `storyFile(P, options)` makes the file
 * from `P`, the page's own values — what logic.js `liveVals` draws: `page`,
 * `bands`, `crit`, `runs`, `words`, `readable`, plus the technical lines —
 * and from nothing else, so every count and sentence in the file is one the
 * page shows (one source of numbers). Markdown, HTML and JSON are three walks
 * of ONE model, so they cannot hold different rows.
 *
 * A redacted copy is a SECOND file, made from the full one and never instead
 * of it: it opens with the classes it leaves out and how many, holds a marker
 * in place of every removal, and names the full file it was made from by that
 * file's sha256 (`derived_from_sha256`). Each count is the number of markers
 * of that class in it: the removals actually made.
 *
 * Escaping is the workspace export's (`packet-export.js`, EXPORT-MD-ESCAPE and
 * the HTML rules), applied once where text enters a line. The engine's word
 * for the file ("packet") is in its name and its technical part only. */
import { escapeMarkdown, escapeHtml, EXPORT_CSS, EXPORT_CSP, EXPORT_FORMAT, EXPORT_SCHEMA_VERSION, filenameOf, sha256Hex } from '../../../workspace/packet-export.js'
import { NAME_NOT_CHECKED } from '../../data/seconds.js'
import { KEPT_BESIDE, SENTENCE_CHANGED } from '../../data/objections.js'

const list = (x) => (Array.isArray(x) ? x : [])
const has = (x) => typeof x === 'string' && x.trim() !== ''

/** The classes a redacted copy can leave out: the page's switch key, the file's class name, the marker and the label. */
export const CLASSES = Object.freeze([
  { key: 'words', name: 'agent_words', label: 'The agent’s own words', marker: '[left out: the agent’s own words]' },
  { key: 'paths', name: 'paths', label: 'File and folder paths', marker: '[left out: a file or folder path]' },
  { key: 'output', name: 'failure_output', label: 'What a check printed when it broke', marker: '[left out: what a check printed when it broke]' },
])
/** The order a copy's label lists them in, as the rail does. */
const LABEL_ORDER = ['paths', 'words', 'output']

export const FILE_TEXT = Object.freeze({
  lead: 'This copy leaves things out. Each one is marked where it was. The full copy is unchanged.',
  leftOut: (label, n) => label + ': ' + n + ' left out',
  pathsLimit: 'Left out: the paths the record holds as paths, and the project’s own folder wherever it appears. Any other path written inside a sentence or a command stays.',
  derivedFrom: (hex) => 'The full copy it was made from: derived_from_sha256 ' + hex,
  noMerge: 'It does not merge, deploy or tell the agent anything.',
  notRecordedNote: 'Not recorded is not the same as didn’t happen.',
  footer: [
    'Made by ThruLine from what is written down on this Mac. Every number comes from typed fields in the record, never from the agent’s prose.',
    'This summarises the record; it is not a guarantee the code is correct.',
  ],
  technical: 'Technical details',
  unreadable: (row) => 'Could not be read now: ' + rowText(row, false),
})

/** One Readable-now row as a line: "◌ A check run · missing · pytest -q". */
export function rowText(r, glyph = true) {
  return (glyph ? r.glyph + ' ' : '') + r.what + ' · ' + r.word + (has(r.detail) ? ' · ' + r.detail : '')
}

/* ── the model: the page, section by section ─────────────────────────────── */

/**
 * @param {object} P the page's values (logic.js `liveVals`)
 * @returns {{title, cover: string[], sections: object[], footer: string[], technical: string[]}}
 */
export function storyModel(P) {
  const p = P.page
  const chip = (p.chipGlyph ? p.chipGlyph + ' ' : '') + (p.chipWord || '') + (p.approvedOn || '')
  const approval = p.approvedBy ? chip + ' ' + p.approvedBy + (p.approvedAt || '') + (p.approvedAgainst || '') : chip.replace(/ · $/, '') + (has(p.approvedAgainst) ? ' · ' + p.approvedAgainst : '')
  const cover = [approval, p.restsOn, FILE_TEXT.noMerge, p.barSet, ['The whole story', p.project, p.root, p.sessions].join(' · '), p.describes].filter(has)

  /* each record that could not be read is said again in the place it belongs to */
  const R = P.readable
  const unreadableIn = (section) => list(R.rows).filter((r) => !r.readable && r.section === section).map(FILE_TEXT.unreadable)

  const said = []
  const s1 = p.s1
  if (s1) {
    said.push(s1.said + '“' + s1.quote + '” · ' + s1.where)
    if (s1.show0 !== false) said.push(s1.at0 + ' This read ○' + s1.read0 + s1.why0)
    if (s1.show1 !== false) said.push(s1.at1 + ' ● ' + s1.why1)
    if (s1.showProven !== false) {
      said.push('● ' + s1.proven + s1.provenMeta)
      if (has(s1.run)) said.push('Run ' + s1.run + ' · ' + s1.runVersion + ' · ' + String(s1.runStep).replace(/ ›$/, ''))
      if (has(s1.note)) said.push(s1.note)
    }
  }
  if (p.s2) {
    said.push(p.s2.said + '“' + p.s2.quote + '” · ' + p.s2.where)
    said.push('▪ Recorded · ' + p.s2.why + 'what counts as done' + p.s2.tail)
  }
  if (p.s3) said.push('Also said, not checked: “' + p.s3.quote + '” · ' + p.s3.where)

  const findings = []
  for (const g of list(p.groups)) {
    for (const f of list(g.findings)) findings.push(f.n + '. ' + String(f.text).trim() + (has(f.where) ? ' · ' + f.where : ''))
    if (g.settlement) findings.push('→ ' + [g.settlement.head, g.settlement.text, g.settlement.ref].filter(has).map((x) => x.trim()).join(' '))
  }

  const notes = list(P.notes).map((n) => [n.who, n.when, n.body].filter(has).join(' · '))
  const seconds = list(P.seconds).map((s) => [s.who, s.when, 'name on the standing approval'].filter(has).join(' · '))
  const objections = list(P.objected).flatMap((statement) => list(statement.objections).map((o) =>
    '“' + statement.plain + '”' + (statement.step == null ? '' : ' · step ' + statement.step)
      + (has(statement.verdictWord) ? ' · ' + statement.verdictWord + ' (unchanged)' : '')
      + ' · ' + o.lead + ' ' + o.reason + (has(o.meta) ? ' · ' + o.meta : '')
      + (o.matches ? '' : ' · ' + SENTENCE_CHANGED)))

  const sections = [
    { id: 'how', heading: p.howItWent, rows: list(P.bands).flatMap((b) => list(b.items).map((i) => i.aria)) },
    {
      id: 'done', heading: 'What counts as done, and what proves it', meta: p.doneMeta, unreadable: unreadableIn('done'),
      rows: list(P.crit).map((c) => c.n + '. ' + c.text + ' · ' + (c.so ? c.so.glyph + ' ' + c.so.word + c.so.version + (has(c.so.scope) ? ' · ' + c.so.scope : '') : c.said)),
      notes: [String(p.doneNote || '').trim()],
    },
    { id: 'said', heading: 'What the agent said, and what the record shows', meta: p.saidMeta, unreadable: unreadableIn('said'), rows: said },
    {
      id: 'runs', heading: 'Runs', meta: 'each on the version it ran on · failures first',
      rows: list(P.runs).map((r) => [r.time, r.step, r.full || r.cmd, r.result, r.ver].join(' · ')),
      notes: ['Each ran in ' + p.runsRoot + p.runsNote],
    },
    { id: 'findings', heading: 'Findings, and how each was settled', meta: p.findingsMeta, rows: findings, notes: [p.groupsNone] },
    {
      id: 'decisions', heading: 'Decision history', meta: 'version-bound', unreadable: unreadableIn('decisions'),
      rows: list(p.dec).map((d) => (d.at + ' ' + String(d.text).trim() + (has(d.state) ? ' · ' + (d.glyph || '') + d.state : '') + (has(d.file) ? ' · ' + d.file : '')).trim()),
    },
    ...(notes.length ? [{ id: 'notes', heading: 'Notes', rows: notes }] : []),
    ...(seconds.length ? [{ id: 'seconds', heading: 'Second approvals', rows: seconds, notes: [NAME_NOT_CHECKED] }] : []),
    ...(objections.length ? [{ id: 'objections', heading: 'What you said was wrong', rows: objections, notes: [KEPT_BESIDE] }] : []),
    { id: 'not-recorded', heading: 'What was not recorded', meta: p.gapsMeta, rows: list(p.gaps).map((g) => g.glyph + ' ' + g.head + g.text), notes: [FILE_TEXT.notRecordedNote] },
    { id: 'integrity', heading: 'Record integrity', meta: p.integrityMeta, rows: [p.integrityHead, p.integritySteps, p.chain] },
    { id: 'readable', heading: 'Readable now', meta: p.readableAt, lede: p.readableSum, rows: list(R.rows).map((r) => rowText(r)) },
    { id: 'words', heading: 'Words', meta: 'every state word used here', rows: list(P.words).map((w) => w.glyph + ' ' + w.word + ': ' + w.def) },
  ].map((s) => ({ id: s.id, heading: s.heading, meta: has(s.meta) ? s.meta : null, lede: has(s.lede) ? s.lede : null, unreadable: list(s.unreadable), rows: list(s.rows).filter(has), notes: list(s.notes).filter(has) }))

  return { title: p.title, cover, sections, footer: [...FILE_TEXT.footer, p.footTimes].filter(has), technical: list(P.technical).filter(has) }
}

/* ── redaction: values held as paths, and the agent's own words ──────────── */

const escapeRegExp = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
const WORD = '[\\p{L}\\p{N}_]'
const SEG = '[\\p{L}\\p{N}_~@+%\\-]'
/* What may follow a held folder and still be part of the same path: more
 * segments, a slash, and a dot only inside a name (never a sentence's full stop). */
const MORE = `(?:${SEG}|\\.(?=${SEG})|/)*`

function patternOf(value, key) {
  const start = new RegExp('^' + WORD, 'u').test(value) ? `(?<!${WORD})` : ''
  const body = escapeRegExp(value)
  if (key === 'words') {
    const end = new RegExp(WORD + '$', 'u').test(value) ? `(?!${WORD})` : ''
    return `“${body}”|${start}${body}${end}`
  }
  /* a path: whole, together with anything inside it ("checkout/" takes "checkout/api.py") */
  if (value.endsWith('/')) return `${start}${body}${MORE}`
  return `${start}${body}(?:/${MORE})?(?!${WORD})`
}

/* A stand-in for a marker while the passes run, so no later value can match inside one. */
const SENTINEL = (i) => '\uE000' + String.fromCharCode(0xE010 + i) + '\uE001'

/**
 * The model with each ticked class's values replaced by its marker, and how
 * many replacements each class made. The agent's words go first, so a path
 * inside a quote is not counted twice; within a class the longest value goes
 * first, so a value that holds another is removed whole. Headings and the
 * technical part are the file's own words and are never touched.
 *
 * @param {object} model
 * @param {string[]} keys the ticked class keys ('paths' | 'words' | 'output')
 * @param {{paths?: string[], words?: string[], output?: string[]}} removable
 */
export function redactModel(model, keys, removable) {
  const counts = {}
  const passes = CLASSES.filter((c) => keys.includes(c.key)).map((c, i) => {
    const values = [...new Set(list(removable[c.key]).filter(has))].sort((a, b) => b.length - a.length)
    counts[c.key] = 0
    return values.length ? { c, i, re: new RegExp(values.map((v) => patternOf(v, c.key)).join('|'), 'gu') } : { c, i, re: null }
  })
  const cut = (text) => {
    if (!has(text)) return text
    let s = text
    for (const pass of passes) {
      if (!pass.re) continue
      s = s.replace(pass.re, () => { counts[pass.c.key] += 1; return SENTINEL(pass.i) })
    }
    for (const pass of passes) s = s.split(SENTINEL(pass.i)).join(pass.c.marker)
    return s
  }
  const out = {
    title: cut(model.title),
    cover: model.cover.map(cut),
    sections: model.sections.map((s) => ({ ...s, meta: s.meta && cut(s.meta), lede: s.lede && cut(s.lede), unreadable: s.unreadable.map(cut), rows: s.rows.map(cut), notes: s.notes.map(cut) })),
    footer: model.footer.map(cut),
    technical: model.technical,
  }
  return { model: out, counts }
}

/* ── three walks of the model ────────────────────────────────────────────── */

function labelOf(redaction) {
  if (!redaction.redacted) return null
  const items = redaction.removed.map((r) => FILE_TEXT.leftOut(CLASSES.find((c) => c.name === r.class).label, r.count))
  const notes = redaction.removed.some((r) => r.class === 'paths') ? [FILE_TEXT.pathsLimit] : []
  return { lead: FILE_TEXT.lead, items, notes, derived: FILE_TEXT.derivedFrom(redaction.derived_from_sha256) }
}

function techOf(model, redaction) {
  return redaction.redacted ? [...model.technical, 'derived_from_sha256: ' + redaction.derived_from_sha256] : model.technical
}

export function markdownOf(model, redaction) {
  const t = escapeMarkdown
  const out = []
  const label = labelOf(redaction)
  if (label) {
    out.push('> ' + t(label.lead))
    for (const i of label.items) out.push('> - ' + t(i))
    for (const n of label.notes) out.push('> ' + t(n))
    out.push('> ' + t(label.derived))
    out.push('')
  }
  out.push('# ' + t(model.title), '')
  for (const c of model.cover) out.push(t(c), '')
  for (const s of model.sections) {
    out.push('## ' + t(s.heading))
    if (s.meta) out.push(t(s.meta))
    if (s.lede) out.push('', t(s.lede))
    if (s.unreadable.length) { out.push(''); for (const g of s.unreadable) out.push(t(g)) }
    if (s.rows.length) { out.push(''); for (const r of s.rows) out.push('- ' + t(r)) }
    for (const n of s.notes) out.push('', t(n))
    out.push('')
  }
  out.push('---', '')
  for (const f of model.footer) out.push(t(f), '')
  out.push('## ' + FILE_TEXT.technical, '')
  for (const x of techOf(model, redaction)) out.push('- ' + t(x))
  return out.join('\n') + '\n'
}

export function htmlOf(model, redaction) {
  const e = escapeHtml
  const h = ['<!doctype html>', '<html lang="en"><head><meta charset="utf-8">', `<meta http-equiv="Content-Security-Policy" content="${EXPORT_CSP}">`,
    '<meta name="viewport" content="width=device-width,initial-scale=1">', '<title>' + e(model.title) + '</title>', '<style>\n' + EXPORT_CSS + '\n</style>', '</head><body><main>']
  const label = labelOf(redaction)
  if (label) {
    h.push('<section class="label"><p><strong>' + e(label.lead) + '</strong></p><ul>' + label.items.map((i) => '<li>' + e(i) + '</li>').join('') + '</ul>'
      + label.notes.map((n) => '<p class="muted">' + e(n) + '</p>').join('') + '<p class="muted">' + e(label.derived) + '</p></section>')
  }
  h.push('<h1>' + e(model.title) + '</h1>')
  for (const c of model.cover) h.push('<p>' + e(c) + '</p>')
  for (const s of model.sections) {
    h.push('<section id="' + e(s.id) + '">', '<h2>' + e(s.heading) + '</h2>')
    if (s.meta) h.push('<p class="muted">' + e(s.meta) + '</p>')
    if (s.lede) h.push('<p>' + e(s.lede) + '</p>')
    for (const g of s.unreadable) h.push('<p class="gap">' + e(g) + '</p>')
    if (s.rows.length) h.push('<ul>' + s.rows.map((r) => '<li>' + e(r) + '</li>').join('') + '</ul>')
    for (const n of s.notes) h.push('<p>' + e(n) + '</p>')
    h.push('</section>')
  }
  h.push('<hr>', '<footer>' + model.footer.map((f) => '<p class="muted">' + e(f) + '</p>').join('') + '</footer>')
  h.push('<h2 class="technical">' + e(FILE_TEXT.technical) + '</h2>', '<ul class="technical">' + techOf(model, redaction).map((x) => '<li>' + e(x) + '</li>').join('') + '</ul>')
  h.push('</main></body></html>')
  return h.join('\n') + '\n'
}

export function jsonOf(model, redaction, facts) {
  const label = labelOf(redaction)
  const doc = {
    format: EXPORT_FORMAT,
    schema_version: EXPORT_SCHEMA_VERSION,
    change_id: facts.changeId ?? null,
    tracked: Boolean(facts.tracked),
    version: { short: facts.version ?? null, commit: facts.commit ?? null },
    title: model.title,
    cover: model.cover,
    sections: model.sections,
    footer: model.footer,
    technical: techOf(model, redaction),
    redaction: {
      redacted: redaction.redacted,
      removed: redaction.removed,
      derived_from_sha256: redaction.derived_from_sha256,
      label: label ? [label.lead, ...label.items, ...label.notes, label.derived] : [],
    },
  }
  if (facts.leftOutByExclusion > 0) doc.left_out_by_exclusion = facts.leftOutByExclusion
  return JSON.stringify(doc, null, 2) + '\n'
}

const MIME = { md: 'text/markdown;charset=utf-8', html: 'text/html;charset=utf-8', json: 'application/json;charset=utf-8' }
const FORMATS = ['md', 'html', 'json']
const NO_REDACTION = Object.freeze({ redacted: false, removed: [], derived_from_sha256: null })

function render(model, fmt, redaction, facts) {
  return fmt === 'json' ? jsonOf(model, redaction, facts) : fmt === 'html' ? htmlOf(model, redaction) : markdownOf(model, redaction)
}

/**
 * The file Download saves and the preview shows.
 *
 * @param {object} P the page's values
 * @param {{fmt?: string, keys?: string[], removable?: object, facts?: object}} options
 *   `facts`: `{title, version, commit, changeId, tracked}` for the name and the JSON's typed fields
 * @returns {{text, filename, mime, fmt, redaction, counts, technical: string[]}}
 */
export function storyFile(P, { fmt = 'md', keys = [], removable = {}, facts = {} } = {}) {
  const f = FORMATS.includes(fmt) ? fmt : 'md'
  const model = storyModel(P)
  const full = render(model, f, NO_REDACTION, facts)
  const { model: cutModel, counts } = redactModel(model, keys, removable)
  const removed = LABEL_ORDER.filter((k) => counts[k] > 0).map((k) => ({ class: CLASSES.find((c) => c.key === k).name, count: counts[k] }))
  const name = (redacted) => filenameOf({ title: facts.title, versionShort: facts.version }, f, redacted)
  if (!removed.length) return { text: full, filename: name(false), mime: MIME[f], fmt: f, redaction: NO_REDACTION, counts, technical: techOf(model, NO_REDACTION) }
  const redaction = { redacted: true, removed, derived_from_sha256: sha256Hex(full) }
  return { text: render(cutModel, f, redaction, facts), filename: name(true), mime: MIME[f], fmt: f, redaction, counts, technical: techOf(cutModel, redaction) }
}

/** How many removals each class would make if it were ticked beside the ones already ticked. */
export function classCounts(P, keys, removable) {
  const model = storyModel(P)
  const out = {}
  for (const c of CLASSES) out[c.key] = redactModel(model, keys.includes(c.key) ? keys : [...keys, c.key], removable).counts[c.key]
  return out
}

export { sha256Hex, EXPORT_FORMAT, EXPORT_SCHEMA_VERSION }
