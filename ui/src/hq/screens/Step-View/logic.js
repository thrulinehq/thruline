/* Step-View: the page at `#/changes/<id>/step/<n>` (C30 R-ROUTES "Step view").
 *
 * It shows one step of the record — what was asked, said, run or edited — with
 * the time, the version and the receipt (the file and line it was read from),
 * and the lines of the session around it. It is the target of every receipt
 * chip and "Run step N ›".
 *
 * `stepViewVals(D)` is pure: it turns the facts from-change.js selects into
 * every word and value the template draws, from the record and nothing else.
 * `renderVals()` is that, as the canvas model wants it. There is no demo
 * record: this page is not a canvas artboard and has no gallery form.
 */
import { DCLogic } from '../../dc/runtime.jsx'
import { storageSentence } from '../../data/project.js'
import { fmtCount } from '../../data/count.js'
import { homeless, middle } from '../../data/paths.js'
import { altitudes } from '../Change-Brief/altitude.js'
import { href } from '../../routes.js'

const T1 = '#EDEDEF', T2 = '#A1A1AA', T3 = '#82828C', OR = '#FF7A29'
const GREEN = 'var(--tl-proven, #3DD68C)', CORAL = '#FF8A8F'

/** Most rows of an edit's lines drawn at once; the rest are said, not drawn. */
export const HUNK_ROWS = 60
/** Longest stretch of a session line drawn at once. */
const LINE_CHARS = 600
const SUMMARY_CHARS = 90

const f = fmtCount
const plural = (n, one, many) => f(n) + ' ' + (n === 1 ? one : many)
const clip = (s, n) => {
  const t = String(s == null ? '' : s).replace(/\s+/g, ' ').trim()
  return t.length > n ? t.slice(0, n - 1) + '…' : t
}

const CLAIM_SAYS = {
  tests_pass: 'It says the tests pass.',
  builds: 'It says the build or the lint is clean.',
  fixed: 'It says something is fixed.',
  done: 'It says the work is done.',
  other: 'It is a statement of another kind.',
}

/** The sentence that says what kind of step this is, with who did it. */
export function whatHappened(s) {
  const who = s.agent
  if (s.kind === 'prompt') return s.prompt && s.prompt.work === false ? 'You wrote' : 'You asked'
  if (s.kind === 'claim') return who + ' said'
  if (s.kind === 'message') return who + ' said'
  if (s.kind === 'edit') {
    const a = s.edit && s.edit.action
    return who + (a === 'create' || a === 'add' ? ' created a file' : a === 'delete' ? ' deleted a file' : ' edited a file')
  }
  const c = s.command || {}
  return who + (c.isCommit ? ' committed' : c.isTest ? ' ran a test' : ' ran a command')
}

/** One line for a step in a list: what it was, in a few words. */
export function summaryOf(s) {
  if (s.kind === 'prompt') return clip(s.prompt && s.prompt.text, SUMMARY_CHARS) || 'You asked'
  if (s.kind === 'claim') return clip(s.claim && s.claim.text, SUMMARY_CHARS) || 'Said'
  if (s.kind === 'message') return clip(s.message && s.message.text, SUMMARY_CHARS) || 'Said'
  if (s.kind === 'edit') return middle(s.edit.file, 40) + (s.edit.linesAdded != null && s.edit.linesRemoved != null ? ' · +' + f(s.edit.linesAdded) + ' −' + f(s.edit.linesRemoved) : '')
  return clip(s.command && s.command.command, SUMMARY_CHARS) || 'A command'
}

/** `+3 −1 lines`, or that the record does not hold them. */
const linesWord = (e) => (e.linesAdded != null && e.linesRemoved != null ? '+' + f(e.linesAdded) + ' −' + f(e.linesRemoved) + ' lines' : 'lines not recorded')

/** What the record states about a test run's count, in its own words. */
function tallyWord(c) {
  if (!c.tally) return null
  const t = c.tally
  const unit = t.unit === 'packages' ? 'packages' : 'tests'
  if (t.passed == null && t.failed == null) return null
  return f(t.passed || 0) + ' ' + unit + ' passed, ' + f(t.failed || 0) + ' failed' + (t.skipped ? ', ' + f(t.skipped) + ' skipped' : '')
}

const OUTCOME = { succeeded: 'Succeeded', failed: 'Failed', unknown: 'No result in the record' }

function hunkRows(hunks) {
  const rows = []
  for (const h of Array.isArray(hunks) ? hunks : []) {
    const numberless = h.old_start == null || h.new_start == null
    rows.push({ t: 'head', sign: '', text: (numberless ? '@@ line numbers not recorded @@' : '@@ -' + h.old_start + ',' + h.old_lines + ' +' + h.new_start + ',' + h.new_lines + ' @@') + (h.header ? ' ' + homeless(String(h.header)) : '') })
    for (const l of Array.isArray(h.lines) ? h.lines : []) {
      const k = l.kind === 'added' || l.kind === 'add' ? 'a' : l.kind === 'removed' || l.kind === 'del' ? 'd' : 'c'
      rows.push({ t: k, sign: k === 'a' ? '+' : k === 'd' ? '−' : ' ', text: homeless(l.text == null ? '' : String(l.text)) })
    }
  }
  return rows
}

const LINE_KIND = {
  prompt: 'You asked',
  agent_prompt: 'A sub-agent was asked',
  injected: 'Added by the tool',
  assistant: 'The agent said',
  thinking: 'The agent thought',
  summary: 'Summary',
  tool_result: 'A tool answered',
}

function lineWords(l) {
  if (l.kind === 'tool' && l.tool) {
    const first = l.tool.summary && l.tool.summary.length ? l.tool.summary[0] : ''
    return { label: l.tool.name || 'A tool', text: clip(homeless(first), 240), more: 0, outcome: l.tool.outcome ? (l.tool.outcome === 'succeeded' ? 'succeeded' : l.tool.outcome === 'failed' ? 'failed' : null) : null }
  }
  const raw = homeless(l.text == null ? '' : String(l.text))
  const text = raw.length > LINE_CHARS ? raw.slice(0, LINE_CHARS) : raw
  return { label: LINE_KIND[l.kind] || 'A line', text, more: raw.length - text.length, outcome: null }
}

/**
 * Every word and value the template draws.
 * @param {object} D  `stepViewFromChange(…)`
 */
export function stepViewVals(D) {
  const total = D.change.stepCount
  const addr = D.where
  const brief = href('brief', { change: addr.change, project: addr.project })
  const statements = href('statements', { change: addr.change, project: addr.project })
  const alt = altitudes('none')
  const common = {
    isDemo: false,
    live: true,
    alt,
    page: {
      project: D.project.name,
      title: D.change.title,
      readAt: D.view.readAt,
      briefHref: brief,
      stepWord: D.found ? 'Step ' + f(D.asked) : 'No such step',
      stepMono: D.found ? 'Step ' + f(D.asked) : D.askedText ? 'Step ' + D.askedText : 'Step',
      ofTotal: D.found ? 'of ' + plural(total, 'step', 'steps') + ' in this Change' : total ? 'this Change has ' + plural(total, 'step', 'steps') : 'this Change has no steps yet',
      storage: storageSentence(D.machine.recordsBytes),
    },
    found: D.found,
  }
  if (!D.found) {
    const none = D.asked == null
    return {
      ...common,
      missing: {
        title: none
          ? (D.askedText ? '“' + D.askedText + '” is not a step number.' : 'This address names no step.')
          : 'There is no step ' + f(D.asked) + ' in this Change.',
        body: total
          ? 'This Change’s record has ' + plural(total, 'step', 'steps') + ', numbered 1 to ' + f(total) + '. A step is a row’s place in the record: something asked, said, run or edited, in the order it was recorded.'
          : 'This Change’s record holds no steps yet. A step is a row’s place in the record: something asked, said, run or edited, in the order it was recorded.',
        briefHref: brief,
        statementsHref: statements,
      },
      step: null,
    }
  }
  const s = D.step
  const record = { kind: s.kind, prompt: null, claim: null, message: null, edit: null, command: null }
  if (s.kind === 'prompt') {
    record.prompt = {
      segments: (s.prompt.segments && s.prompt.segments.length ? s.prompt.segments : [{ kind: 'typed', text: s.prompt.text }]).map((x) => ({ pasted: x.kind === 'pasted', label: x.kind === 'pasted' ? 'Pasted in' : '', text: x.text })),
      conversation: s.prompt.work === false,
    }
  } else if (s.kind === 'claim') {
    record.claim = {
      text: s.claim.text,
      says: CLAIM_SAYS[s.claim.claimKind] || CLAIM_SAYS.other,
      checkable: s.claim.verifier ? 'A test run can check this.' : 'Nothing in the record can check this on its own.',
      statementHref: D.statementHref,
    }
  } else if (s.kind === 'message') {
    record.message = { text: s.message.text, note: 'No claim rule applies to this sentence, so it is listed without a verdict and counts toward nothing.' }
  } else if (s.kind === 'edit') {
    const e = s.edit
    const rows = hunkRows(e.hunks)
    const shown = rows.slice(0, HUNK_ROWS)
    record.edit = {
      file: e.file,
      lines: linesWord(e),
      makes: e.makesVersion ? 'This made version ' + e.makesVersion + '.' : '',
      notApplied: e.ok === false ? 'The tool reported that this edit did not apply.' : '',
      rows: shown.map((r) => ({
        ...r,
        color: r.t === 'a' ? GREEN : r.t === 'd' ? CORAL : r.t === 'head' ? T3 : T2,
        bg: r.t === 'a' ? 'rgba(var(--tl-proven-rgb, 61, 214, 140), .08)' : r.t === 'd' ? 'rgba(255,99,105,.08)' : 'transparent',
      })),
      hasRows: shown.length > 0,
      cut: rows.length > shown.length ? 'Showing the first ' + f(shown.length) + ' of ' + plural(rows.length, 'line', 'lines') + '. The rest are in the recording named below.' : '',
      noLines: shown.length === 0 ? 'The record does not hold the lines of this edit.' : '',
      source: e.source === 'request' && shown.length ? 'from the request · no surrounding lines' : '',
    }
  } else {
    const c = s.command
    const tally = tallyWord(c)
    const out = c.output || []
    record.command = {
      command: c.command,
      kind: c.isCommit ? 'A commit' : c.isTest ? 'A test run' : 'A command',
      outcome: OUTCOME[c.outcome] || OUTCOME.unknown,
      outcomeColor: c.outcome === 'failed' ? CORAL : c.outcome === 'succeeded' ? T1 : T2,
      exit: c.exitCode != null ? 'exit ' + c.exitCode : '',
      tally,
      tallyLine: c.tallyLine || '',
      noTally: c.isTest && !tally && c.outcome !== 'unknown' ? 'The record states no test count for this run.' : '',
      output: out.map((x) => homeless(String(x))),
      hasOutput: out.length > 0,
      outputNote: out.length ? (c.outputTotal != null && c.outputTotal > out.length ? 'The last ' + f(out.length) + ' of ' + plural(c.outputTotal, 'line', 'lines') + ' of output' : plural(out.length, 'line', 'lines') + ' of output') : '',
    }
  }

  const lines = D.lines
    ? {
        found: D.lines.found,
        items: D.lines.items.map((l) => {
          const w = lineWords(l)
          return { ...w, time: l.time || '', current: l.current, bar: l.current ? '2px solid ' + OR : '2px solid transparent', bg: l.current ? '#111113' : 'transparent', color: l.current ? T1 : T2, aria: l.current ? 'location' : undefined, mono: l.kind === 'tool' }
        }),
        before: D.lines.before ? plural(D.lines.before, 'earlier line', 'earlier lines') + ' in this session' : '',
        after: D.lines.after ? plural(D.lines.after, 'later line', 'later lines') + ' in this session' : '',
        absent: D.lines.found ? '' : 'This step’s line was not found among the session’s lines.',
      }
    : null

  const nav = (x) => (x ? { href: x.href, label: 'Step ' + f(x.step), what: summaryOf(x) } : null)
  const around = D.around.map((x) => ({
    href: x.href,
    label: 'Step ' + f(x.step),
    what: summaryOf(x),
    time: x.time || '',
    current: x.current ? 'location' : 'false',
    color: x.current ? T1 : T2,
    bar: x.current ? '2px solid ' + OR : '2px solid transparent',
    glow: x.current ? '-4px 0 8px -4px rgba(255,154,82,.6)' : 'none',
    aria: x.current ? 'This step: ' + summaryOf(x) : 'Step ' + f(x.step) + ': ' + summaryOf(x),
  }))

  return {
    ...common,
    missing: null,
    step: {
      n: f(D.asked),
      who: whatHappened(s),
      time: s.time || 'time not recorded',
      version: s.version || '',
      versionNote: s.version ? 'version ' + s.version : 'no version recorded',
      session: (s.session ? 'session ' + s.session.n + ' of ' + s.session.of + ' · ' : '') + s.agent,
      record,
      receipt: {
        file: s.receipt ? s.receipt.file : '',
        line: s.receipt && s.receipt.line != null ? f(s.receipt.line) : '',
        has: Boolean(s.receipt),
        none: 'The record names no file for this step.',
        note: 'This is the line of the recording that this step was read from. ThruLine only reads it.',
      },
      lines,
      linesError: D.linesError ? 'The lines of this session around the step could not be read now.' : '',
      previous: nav(D.previous),
      next: nav(D.next),
      isFirst: !D.previous,
      isLast: !D.next,
      around,
    },
  }
}

export default class Component extends DCLogic {
  renderVals() {
    return stepViewVals(this.props.data)
  }
}
