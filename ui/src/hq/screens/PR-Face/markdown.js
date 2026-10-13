/* The note for a pull request: its sentences, its Markdown, and the Action
 * setup line copied after it.
 *
 *   composeNote(facts) → note    (the blocks the preview draws)
 *   markdownOf(note)   → string  (the comment's Markdown)
 *   setupOf(receipts)  → Inline[] (the setup line drawn under the comment)
 *   copyOf(note, setup) → string (exactly what Copy as Markdown copies)
 *
 * PURE: no fetch, no window, no clock. The preview (template.jsx through
 * logic.js) and the Markdown are both drawn from ONE `note`, so the words a
 * person reads on screen and the words they paste are the same words.
 *
 * A note is
 *   { marker, title, lines[], table|null, tableNote|null, tableAfter|null,
 *     layers[{open, summary, items[]}], version, footer, firstLines[] }
 * and every text in it is an Inline list: a string (ThruLine's own words),
 * `{code}` (a version id, or a recorded path or command that holds no backtick
 * and no line break), `{rec}` (recorded words: a title, what the agent said, a
 * path or command that cannot be a code span, a sentence of the Change model
 * that may hold one), `{strong: Inline[]}`, `{em}`, `{glyph}` (a state mark)
 * and `{stop, label}` (where a chain stops). `firstLines` is the headline, the
 * settle line and the approval line again, for the edited-in-place panel; it
 * is not written into the Markdown a second time.
 *
 * ONE ROW PER LINE OF WHAT COUNTS AS DONE (C30 R-PR). The table is the tracked
 * Change's accepted lines and where each stands, read from its
 * `deriveChangeModel` reading (state, word, Proven line, freshness, coverage
 * sentence) and never re-derived here; the four lines above it are that
 * model's readiness and the standing approval in `change_decisions`. What the
 * agent said is folded below, each statement with its own chain.
 *
 * MARKDOWN SAFETY. Recorded words are attacker-influenced. They are written
 * through the export's ONE helper, `escapeMarkdown` (workspace/packet-export.js):
 * no live bracket, no live `<`, no bare address. A recorded text becomes a code
 * span only when it holds no backtick and no line break, so it cannot close the
 * span it is in; anything else is escaped text. Inside a table cell `|` is
 * written `\|`, in a code span too. The only HTML is the hidden marker and the
 * `<details>` folds, and recorded words never enter them unescaped.
 */
import { escapeMarkdown } from '../../../workspace/packet-export.js'
import { fmtCount, countOf } from '../../data/count.js'

const CIRCLED = ['', '①', '②', '③', '④', '⑤', '⑥']
const LINK = ['', 'SAID', 'RESTS ON', 'RAN ON', 'SINCE', 'RE-CHECKED', 'SO']
const LINK_ARIA = ['', 'said', 'rests on', 'ran on', 'since', 're-checked', 'so']
const MAX_CODE = 60

/** How many rows of each list the note carries; the rest are counted, never dropped silently. */
export const MAX_ROWS = 12
export const MAX_SAID = 8
export const MAX_RUNS = 8
export const MAX_VERSIONS = 6

/* The state words of a statement (docs/loop/03-USER-LENS.md §3): a
 * contradicted statement is "Not proven", marked ✕, and the chain says which
 * run failed. */
const MARK = { proven: '●', not_proven: '○', contradicted: '✕', recorded: '▪' }
const WORD = { proven: 'Proven', not_proven: 'Not proven', contradicted: 'Not proven', recorded: 'Recorded' }

/* The marks of the five states `deriveChangeModel` gives a line of what
 * counts as done (DESIGN-SPEC §1.4). A line in any other state carries no
 * mark. Its word is always the model's own (`word`). */
export const LINE_MARK = Object.freeze({ proven: '●', not_proven: '○', needs_recheck: '◭', recorded: '▪', in_progress: '◐' })

const pl = (n, one, many) => (n === 1 ? one : many)

/** A recorded path or command, shortened to its first line and 60 characters
 *  (the cut is said with "…"), as a code span when it can safely be one. */
export function recorded(text, { max = MAX_CODE } = {}) {
  const raw = String(text == null ? '' : text)
  const first = raw.split(/\r?\n/)[0].trim()
  const cut = first.length > max || first !== raw.trim()
  const shown = first.length > max ? first.slice(0, max - 1) + '…' : (cut ? first + ' …' : first)
  if (shown && !/[`\r\n]/.test(shown)) return { code: shown }
  return { rec: shown }
}

/** A recorded file path: as the snapshot placed it (relative to the project
 *  when inside it); a path outside every root keeps its last two parts, so no
 *  home folder or user name is written into the note. */
export function pathSeg(file) {
  const p = String(file == null ? '' : file)
  if (!p.startsWith('/') && !p.startsWith('~')) return recorded(p)
  const parts = p.split('/').filter(Boolean)
  return recorded(parts.length > 2 ? '…/' + parts.slice(-2).join('/') : p.replace(/^\/(Users|home)\/[^/]+(?=\/|$)/, '~'))
}

/** A recorded command: its leading set-up (`cd <folder> &&`, `NAME=value;` or on a line of its own)
 *  is shown as `…` and any home folder reads `~`; then shortened as
 *  `recorded` does. */
export function commandSeg(command) {
  let c = String(command == null ? '' : command).trim()
  /* leading set-up — `cd <folder> &&` or `NAME=value;` — is where and how it
   * ran, not what it did; it is shown as `…` */
  const SETUP = /^(?:cd[ \t]+\S+|[A-Za-z_][A-Za-z0-9_]*=\S*)[ \t]*(?:&&|;|\r?\n)\s*/
  let cut = false
  for (let m = c.match(SETUP); m && m[0].length < c.length; m = c.match(SETUP)) { c = c.slice(m[0].length); cut = true }
  if (cut) c = '… ' + c
  c = c.replace(/\/(Users|home)\/[^/\s]+(?=\/|\s|$)/g, '~')
  return recorded(c)
}

/** A version id ThruLine wrote (hex, or `wt-` and hex); anything else is escaped text. */
const ver = (v) => (/^[A-Za-z0-9._-]{1,64}$/.test(String(v)) ? { code: String(v) } : { rec: String(v) })

/** A run's result, from its typed tally or, without one, its outcome. */
function tally(r) {
  if (r.passed != null && r.failed != null) return fmtCount(r.passed) + ' passed, ' + fmtCount(r.failed) + ' failed'
  if (r.passed != null) return fmtCount(r.passed) + ' passed'
  if (r.failed != null) return fmtCount(r.failed) + ' failed'
  if (r.outcome === 'succeeded') return 'finished, no tally recorded'
  if (r.outcome === 'failed') return 'failed, no tally recorded'
  return 'no result yet'
}

const isFailing = (r) => r.outcome === 'failed' || (r.failed != null && r.failed > 0)

/** One statement's proof, link by link. */
function chainOf(c, v) {
  const out = []
  const link = (...segs) => { if (out.length) out.push(' → '); out.push(...segs) }
  link('said at step ' + c.step)
  if (c.restsOn) link('step ' + c.restsOn.step + ', ' + tally(c.restsOn) + (c.restsOn.version ? ', on ' : ''), ...(c.restsOn.version ? [ver(c.restsOn.version)] : []))
  else link('no test run before it')
  if (c.since) link(pathSeg(c.since.file), ' changed (step ' + c.since.step + ')')
  if (c.verdict === 'contradicted' && c.failedAfter) link('step ' + c.failedAfter.step + ' failed' + (c.failedAfter.version ? ' on ' : ''), ...(c.failedAfter.version ? [ver(c.failedAfter.version)] : []))
  else if (c.verdict !== 'proven' && c.stopsAt === 5 && v) link('nothing on ', ver(v))
  return out
}

/** Where one line's proof stops, when the model's state places it: ⑤ for a
 *  line that needs a re-check (its evidence was on another version, and no
 *  re-check on this one); ② for a line with nothing in the record behind it.
 *  Every other line is not placed (null) and the note says "—". */
export function lineStop(l) {
  if (!l || l.state === 'proven') return null
  if (l.state === 'needs_recheck') return 5
  if (l.evidence === 0) return 2
  return null
}

/** One line's proof, link by link, in the model's own words: when it was
 *  accepted; what the record holds for it (the first piece of evidence, or the
 *  coverage sentence when there is none); and where it stands (its Proven
 *  line, the freshness sentence of evidence that needs a re-check, or its
 *  state line). */
function lineChain(l, accepted) {
  const out = [accepted]
  if (l.state === 'proven' && l.line) return [...out, ' → ', { rec: l.line }]
  if (!l.evidence) return [...out, ' → ', { rec: l.sentence || 'nothing in the record' }]
  if (l.evidenceName) out.push(' → ', recorded(l.evidenceName), l.evidence > 1 ? ' and ' + fmtCount(l.evidence - 1) + ' more' : '')
  const tail = l.freshness && l.freshness.fresh === false && l.freshness.sentence ? l.freshness.sentence : (l.line || l.word || l.sentence)
  if (tail) out.push(' → ', { rec: tail })
  return out
}

/** "all 4" / "the 1 line" / "3 of 4 lines". */
function waitsOn(n, total) {
  if (n === total) return total === 1 ? 'the 1 line' : 'all ' + fmtCount(total)
  return fmtCount(n) + ' of ' + fmtCount(total) + ' lines'
}

/**
 * The note, from the facts (from-change.js `noteFacts`) of a tracked Change.
 */
export function composeNote(f) {
  const v = f.version
  const vSeg = v ? [ver(v)] : ['no recorded version']
  const lines = Array.isArray(f.lines) ? f.lines : []
  const lv = f.lineVersion || v
  const lvSeg = lv ? [ver(lv)] : ['no recorded version']
  const open = lines.filter((l) => l.state !== 'proven')
  const n = open.length
  const total = lines.length
  const d = f.decision || { approved: false }
  const ready = !!(f.readiness && f.readiness.ready)
  const blocker = f.readiness && f.readiness.blocker ? f.readiness.blocker : null
  const approvedHere = !!d.approved && !d.movedSince
  const moved = !!d.approved && !!d.movedSince

  const safeId = (x) => String(x == null ? 'none' : x).replace(/[^A-Za-z0-9_.-]/g, '')
  const marker = '<!-- thruline:note change=' + safeId(f.changeId) + ' version=' + safeId(v) + ' criteria=' + safeId(f.criteriaVersion || 'none') + ' schema=note/1 -->'

  const title = [{ strong: f.title ? ['ThruLine · ', { rec: f.title }] : ['ThruLine · a Change with no recorded title'] }, ...(v ? [' · version ', ver(v)] : [' · no version recorded'])]

  /* ---- the four lines ---- */
  let headline
  if (approvedHere) headline = [{ glyph: '◆' }, ' Approved on ', ver(d.version), ' by ', { rec: d.actor || 'a person not recorded' }, ' in ThruLine.']
  else if (moved) headline = [{ glyph: '◭' }, ' Needs a re-check — approved on ', ver(d.version), ', and the code changed since.']
  else if (!f.accepted) headline = [{ glyph: '▪' }, ' Recorded — what counts as done is not accepted yet, so nothing is measured on ', ...vSeg, '.']
  else if (ready) headline = [{ glyph: '◇' }, ' Ready for approval for version ', ...vSeg, '.']
  else if (n) {
    const lead = f.leading && LINE_MARK[f.leading.state] && f.leading.word ? f.leading : open[0]
    headline = [{ glyph: LINE_MARK[lead.state] || '○' }, ' ' + (lead.word || 'Not proven') + ' — ' + fmtCount(n) + ' ' + pl(n, 'thing still needs', 'things still need') + ' evidence before ', ...vSeg, ' can be approved.']
  } else if (total) headline = [{ glyph: '◐' }, ' In progress — every line holds on ', ...lvSeg, ', and approval is not open on ', ...vSeg, ' yet.']
  else headline = [{ glyph: '◐' }, ' In progress — what counts as done has no line yet.']
  const out = [[{ strong: headline }]]

  if (!approvedHere && !ready && blocker && blocker.reason) out.push([{ rec: blocker.reason }])

  const settleLabel = { strong: ['To settle it:'] }
  let settle
  if (approvedHere || ready) settle = [settleLabel, ' nothing left on ', ...vSeg, '.' + (f.lastRunAt ? ' Any edit after ' + f.lastRunAt + ' reopens it.' : '')]
  else if (!f.accepted) settle = [settleLabel, ' accept what counts as done in ThruLine; nothing is measured before that.']
  else settle = [settleLabel, ' run the Change’s check on ', ...vSeg, ', or ', { em: 'Check it again' }, ' in ThruLine.']
  out.push(settle)

  const approvalLabel = { strong: ['For approval:'] }
  let approval
  if (approvedHere) approval = [approvalLabel, ' approved in ThruLine by ', { rec: d.actor || 'a person not recorded' }, (d.at ? ' at ' + d.at : '') + '. ThruLine never approves for you.']
  else if (moved) approval = [approvalLabel, ' the approval names ', ver(d.version), ', and the record now stands at ', ...(d.now ? [ver(d.now)] : ['a later version']), '. Not approved for the code now.']
  else if (!f.accepted) approval = [approvalLabel, ' what counts as done is not accepted yet, so there is nothing to approve. Not approved.']
  else if (ready) approval = [approvalLabel, ' not approved yet. ThruLine never approves for you.']
  else if (n) approval = [approvalLabel, ' approval of ', ...vSeg, ' waits on ' + waitsOn(n, total) + '. Not approved.']
  else approval = [approvalLabel, ' not open yet. Not approved.']
  out.push(approval)
  const firstLines = [out[0], settle, approval]

  const stops = new Map()
  for (const l of lines) { const k = lineStop(l); if (k) stops.set(k, (stops.get(k) || 0) + 1) }
  if (stops.size) {
    const seg = ['Where the proof stops: ']
    ;[...stops.entries()].sort((a, b) => b[0] - a[0]).forEach(([k, cnt], i) => {
      if (i) seg.push(' · ')
      seg.push({ glyph: CIRCLED[k] }, ' ' + LINK[k] + ' ×' + fmtCount(cnt))
    })
    out.push(seg)
  }

  /* ---- proof, link by link: one row per line of what counts as done ---- */
  let table = null, tableNote = null, tableAfter = null
  if (lines.length) {
    const shown = lines.slice(0, MAX_ROWS)
    const accepted = 'accepted' + (f.acceptedAt ? ' ' + f.acceptedAt : '')
    table = {
      head: [[], ['What counts as done'], ['On ', ...lvSeg], ['Proof, link by link'], ['Stops at']],
      rows: shown.map((l) => {
        const k = lineStop(l)
        return [
          LINE_MARK[l.state] ? [{ glyph: LINE_MARK[l.state] }] : ['—'],
          [{ rec: l.text }],
          [l.word || 'no state recorded'],
          lineChain(l, accepted),
          k ? [{ stop: CIRCLED[k], label: 'link ' + k + ', ' + LINK_ARIA[k] }] : ['—'],
        ]
      }),
    }
    const hidden = lines.length - shown.length
    if (hidden) tableAfter = [fmtCount(hidden) + ' more ' + pl(hidden, 'line', 'lines') + ' of what counts as done ' + pl(hidden, 'is', 'are') + ' not in this table; ThruLine holds every one.']
  } else {
    tableNote = [f.accepted ? 'What counts as done has no line yet, so there is no proof to follow link by link.' : 'What counts as done is not accepted yet, so there is no line to follow link by link.']
  }

  /* ---- the folded layers ---- */
  const layers = []
  const said = [
    ...f.checkable.map((c) => ({ step: c.step, item: [{ glyph: MARK[c.verdict] || MARK.not_proven }, ' ❝', { rec: c.text }, '❞ (step ' + c.step + ') — ' + (WORD[c.verdict] || WORD.not_proven) + ': ', ...chainOf(c, v)] })),
    ...f.unchecked.map((s) => ({ step: s.step, item: [{ glyph: '▪' }, ' ❝', { rec: s.text }, '❞ (step ' + s.step + ') — Recorded. A run can’t check this by itself.'] })),
  ].sort((a, b) => (a.step ?? 0) - (b.step ?? 0))
  const saidItems = said.slice(-MAX_SAID).map((x) => x.item)
  if (said.length > MAX_SAID) saidItems.push([fmtCount(said.length - MAX_SAID) + ' earlier ' + pl(said.length - MAX_SAID, 'one is', 'ones are') + ' not shown here.'])
  if (!said.length) saidItems.push(['The record holds no statement from the agent.'])
  layers.push({
    open: true,
    summary: ['What the agent said, and what the record shows (' + countOf(f.statementCount, 'statement', 'statements') + ' · ' + fmtCount(f.checkable.length) + ' a run can check)'],
    items: saidItems,
  })

  const tests = f.tests
  const finished = tests.filter((r) => r.status === 'finished')
  const onV = finished.filter((r) => v && r.version === v).length
  const noResult = tests.filter((r) => r.status === 'no_result').length
  const ordered = [...tests].sort((a, b) => (isFailing(b) - isFailing(a)) || ((b.step || 0) - (a.step || 0)))
  const runItems = ordered.slice(0, MAX_RUNS).map((r) => {
    const tail = []
    if (r.version) tail.push(' · on ', ver(r.version))
    tail.push((r.step != null ? ' · step ' + r.step : ' · run in ThruLine') + (r.at ? ' · ' + r.at : ''))
    if (r.status === 'no_result') return [{ glyph: '⋯' }, ' ', commandSeg(r.command), ' · no result yet', ...tail]
    return [commandSeg(r.command), (r.exitCode != null ? ' · exit ' + r.exitCode : '') + ' · ' + tally(r), ...tail]
  })
  if (tests.length > MAX_RUNS) runItems.push([fmtCount(tests.length - MAX_RUNS) + ' more test ' + pl(tests.length - MAX_RUNS, 'run is', 'runs are') + ' not shown here.'])
  if (!tests.length) runItems.push(['No test run is in the record for this Change.'])
  layers.push({
    open: true,
    summary: ['Runs, failures first — ' + fmtCount(finished.length) + ' finished, ', ...(onV ? [fmtCount(onV) + ' on '] : ['none on ']), ...vSeg, ' · ' + fmtCount(noResult) + ' with no result yet'],
    items: runItems,
  })

  const looked = []
  const sc = f.sessions.count
  looked.push(['Looked at: ' + fmtCount(sc) + ' recorded ' + pl(sc, 'session', 'sessions') + (f.sessions.agents.length ? ' (' + f.sessions.agents.join(', ') + ')' : '') + (f.stepCount != null ? ' · ' + fmtCount(f.stepCount) + ' steps' : '') + ' · ' + countOf(f.editCount, 'edit', 'edits') + ' · ' + countOf(f.commandCount, 'command', 'commands') + '.'])
  if (f.versionsSeen.length) {
    const vs = f.versionsSeen.slice(-MAX_VERSIONS)
    const seg = ['Versions recorded: ']
    vs.forEach((x, i) => { if (i) seg.push(', '); seg.push(ver(x)) })
    const more = f.versionsSeen.length - vs.length
    seg.push(more ? ' and ' + fmtCount(more) + ' earlier.' : '.')
    looked.push(seg)
  }
  looked.push([f.firstAt ? 'Not recorded: anything before ' + f.firstAt + '.' : 'Not recorded: when this work began; no recorded row carries a time.'])
  if (f.noResultCount) looked.push([fmtCount(f.noResultCount) + ' ' + pl(f.noResultCount, 'command has', 'commands have') + ' no result in the record.'])
  layers.push({ open: false, summary: ['What was looked at · what was not recorded'], items: looked })

  const handoff = []
  if (n) {
    handoff.push([(n === 1 ? 'This line of what counts as done is' : 'These ' + fmtCount(n) + ' lines of what counts as done are') + ' not proven on ', ...lvSeg, ':'])
    for (const o of open.slice(0, 5)) handoff.push([{ rec: o.text }])
    if (n > 5) handoff.push([fmtCount(n - 5) + ' more ' + pl(n - 5, 'is', 'are') + ' in ThruLine.'])
    handoff.push(['Run the check again on ', ...vSeg, ' and report what it printed. Don’t say it passes until a run on this version shows it.'])
  } else {
    handoff.push(['Nothing to hand off: no line of what counts as done is waiting on a run.'])
  }
  layers.push({ open: false, summary: ['Prepared for the agent — not sent'], items: handoff })

  /* ---- version ↔ commit ---- */
  let version
  const row = f.versionRow
  if (!v) version = ['ThruLine has recorded no version for this Change.']
  else if (row && row.commit && row.unsaved === false) version = [ver(v), ' = commit ', ver(row.commit), ' with no unsaved changes.']
  else if (row && row.unsaved === true) version = [ver(v), ' holds unsaved changes that no commit contains yet.']
  else if (row && row.unsaved === false) version = [ver(v), ' was committed; ThruLine recorded no commit hash for it.']
  else version = [ver(v), ': ThruLine has not recorded whether it is committed.']

  /* ---- footer ---- */
  const m = f.masking
  const masked = m && m.masked != null
    ? 'masked: ' + fmtCount(m.masked) + (m.families.length ? ' (' + m.families.join(', ') + ')' : '') + (m.complete === false ? ' · masking incomplete' : '')
    : 'masking not recorded'
  const footer = [
    'Generated locally by ThruLine · nothing was uploaded · Describes ', ...vSeg,
    ' as read at ' + (f.readAt || 'an unrecorded time') + '; every time in this note is UTC · Every line comes from typed fields in a local, hash-chained record; numbers never come from the agent’s prose · Advisory: this never blocks a merge and is not a guarantee that the code is correct · ' + masked + ' · Check the record it was read from: ',
    { code: 'thruline verify' }, ', on the machine that recorded it.',
  ]

  return { marker, title, lines: out, table, tableNote, tableAfter, layers, version, footer, firstLines }
}

/* ------------------------------------------------------------ the setup line */

/** Where the ThruLine Action looks for a receipt, relative to the repository
 *  (`packaging/action/action.yml`, "Verify the receipt"). */
export const RECEIPTS_DIR = '.thruline/receipts/'
/** The book's own receipt name, used when the folder holds none
 *  (`docs/book/src/ci.md`, "Producing a receipt"). */
export const EXAMPLE_RECEIPT = 'latest.json'

/** A receipt's file name that can be written inside the command as it is. */
const plainName = (n) => typeof n === 'string' && /^[A-Za-z0-9._-]{1,200}$/.test(n)

/** The command the Action runs on one receipt, with the arguments
 *  `packaging/action/action.yml` passes (`--offline`, `--format`,
 *  `--head-sha "$GITHUB_SHA"`). `format` is `check-run` or `badge`. */
export function verifyCommand(receipt, format = 'check-run') {
  const file = plainName(receipt) ? receipt : EXAMPLE_RECEIPT
  return 'thruline verify-receipt ' + RECEIPTS_DIR + file + ' --offline --format ' + format + ' --head-sha "$GITHUB_SHA"'
}

/**
 * The Action setup line, drawn under the comment and copied after it. The
 * ThruLine Action has no published address (docs/book/src/ci.md, "Until it is
 * published"), so the line is its own check, as a CI step.
 * @param {{newest?: ?string, checkName?: ?string}} [r] the folder's newest receipt; the CLI's check-run name
 */
export function setupOf(r = {}) {
  const name = typeof r.checkName === 'string' && /^[A-Za-z0-9/_.-]{1,80}$/.test(r.checkName) ? r.checkName : null
  return [
    'In CI: the ThruLine Action is not published yet, so add its check as a step. ',
    { code: verifyCommand(r.newest) },
    name ? ' prints the ' : ' prints the check run the Action would post.',
    ...(name ? [{ code: name }, ' check run the Action would post.'] : []),
  ]
}

/** Exactly what Copy as Markdown copies: the comment, then the setup line. */
export function copyOf(note, setup) {
  return markdownOf(note) + '\n' + inlineMd(setup) + '\n'
}

/* ---------------------------------------------------------------- Markdown */

const html = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')

/** One Inline list as Markdown. `cell`: inside a table cell. `tag`: inside an HTML tag (a summary). */
export function inlineMd(segs, { cell = false, tag = false } = {}) {
  const pipe = (s) => (cell ? s.replace(/\|/g, '\\|') : s)
  return (segs || []).map((x) => {
    if (typeof x === 'string') return tag ? html(x) : pipe(x)
    if (x.code != null) return tag ? '<code>' + html(x.code) + '</code>' : '`' + pipe(String(x.code)) + '`'
    if (x.rec != null) return tag ? html(x.rec) : pipe(escapeMarkdown(x.rec))
    if (x.strong) return (tag ? '<b>' : '**') + inlineMd(x.strong, { cell, tag }) + (tag ? '</b>' : '**')
    if (x.em != null) return (tag ? '<i>' : '*') + (tag ? html(x.em) : pipe(String(x.em))) + (tag ? '</i>' : '*')
    if (x.glyph != null) return String(x.glyph)
    if (x.stop != null) return String(x.stop)
    return ''
  }).join('')
}

/** The comment as Markdown. */
export function markdownOf(note) {
  const out = []
  out.push(note.marker)
  out.push(inlineMd(note.title))
  out.push('')
  for (const l of note.lines) { out.push(inlineMd(l)); out.push('') }
  if (note.table) {
    out.push('| ' + note.table.head.map((h) => inlineMd(h, { cell: true })).join(' | ') + ' |')
    out.push('| :-: | --- | --- | --- | :-: |')
    for (const r of note.table.rows) out.push('| ' + r.map((cell) => inlineMd(cell, { cell: true })).join(' | ') + ' |')
    out.push('')
  }
  if (note.tableNote) { out.push(inlineMd(note.tableNote)); out.push('') }
  if (note.tableAfter) { out.push(inlineMd(note.tableAfter)); out.push('') }
  for (const layer of note.layers) {
    out.push(layer.open ? '<details open>' : '<details>')
    out.push('<summary>' + inlineMd(layer.summary, { tag: true }) + '</summary>')
    out.push('')
    for (const it of layer.items) out.push('- ' + inlineMd(it))
    out.push('')
    out.push('</details>')
    out.push('')
  }
  out.push(inlineMd(note.version))
  out.push('')
  out.push(inlineMd(note.footer))
  return out.join('\n') + '\n'
}

export default markdownOf
