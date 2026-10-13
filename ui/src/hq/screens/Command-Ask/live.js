/* ⌘K on the record: how a served fact is said in the palette, where the
 * gallery says the canvas's demo. logic.js calls these only when
 * `this.props.data` is present, so the design rendering cannot move.
 *
 * Every function is pure and null-safe: a fact the record does not hold is
 * left out of the sentence or said to be missing ("no result yet", "not
 * accepted yet"), never filled in.
 */
import { middle, homeless } from '../../data/paths.js' // a long path keeps its file name; whole in title=
import { plainText } from '../../data/change.js'
import { fmtCount, countOf } from '../../data/count.js'
import { decisionSearchItems } from '../../data/decisions.js'

export const STATE = Object.freeze({
  not_proven: { glyph: '○', word: 'Not proven', gcol: '#A1A1AA' },
  proven: { glyph: '●', word: 'Proven', gcol: 'var(--tl-proven, #3DD68C)' },
  contradicted: { glyph: '✕', word: 'Not proven · the record shows the opposite', gcol: '#A1A1AA' },
  recorded: { glyph: '○', word: 'Recorded', gcol: '#82828C' },
})

const WHOSE = Object.freeze({ you: 'your move', agent: 'the agent is working', done: 'nothing open' })
const join = (xs) => xs.filter((x) => x != null && x !== '').join(' · ')
const stepsText = (ns) => (ns.length === 1 ? 'step ' + ns[0] : 'steps ' + ns.join(' and '))
const older = (n) => fmtCount(n) + (n === 1 ? ' version older' : ' versions older')
const plural = countOf

/** "4 passed, 0 failed" from a tally, else the outcome in words. */
export function runCounts(r) {
  if (r.passed != null || r.failed != null) {
    const p = r.passed || 0
    const f = r.failed || 0
    return f > 0 ? fmtCount(f) + ' failed, ' + fmtCount(p) + ' passed' : fmtCount(p) + ' passed, ' + fmtCount(f) + ' failed'
  }
  return r.outcome === 'succeeded' ? 'passed' : r.outcome === 'failed' ? 'failed' : 'no result yet'
}

/** One Find row: the same fields the gallery row has, plus `screen` and `change` for its address. */
export function liveRecRow(r, ctx) {
  const T1 = '#EDEDEF'
  const VIOLET = '#B78CFF'
  const step = r.step != null ? 'step ' + r.step : ''
  switch (r.kind) {
    case 'Request':
      return { kind: r.kind, qo: '“', text: r.text, qc: '”', qcol: T1, mono: false, meta: join([r.by === 'you' ? 'your request' : r.by, r.at]), step, screen: 'brief' }
    case 'Edit':
      return r.afterLastCheck
        ? { kind: r.kind, text: r.op + ' ' + middle(r.file, 36), title: r.op + ' ' + r.file, mono: true, glyph: '◭ ', gcol: '#F2C94C', meta: join(['changed after the last passing test run', r.at]), step, screen: 'changed' }
        : { kind: r.kind, text: r.op + ' ' + middle(r.file, 36), title: r.op + ' ' + r.file, mono: true, meta: r.versionAfter ? 'then version ' + r.versionAfter : (r.at || ''), step, screen: 'changed' }
    case 'Run': {
      const tail = r.exit ? 'exit ' + r.exit : (r.versionsBehind ? older(r.versionsBehind) : '')
      return { kind: r.kind, text: r.command, mono: true, meta: join([runCounts(r), r.version, tail]), step, screen: 'brief' }
    }
    case 'Said': {
      const st = r.statement && STATE[r.statement.state]
      return st
        ? { kind: r.kind, qo: '❝', text: r.text, qc: '❞', qcol: VIOLET, mono: false, glyph: st.glyph + ' ', gcol: st.gcol, meta: join([st.word, r.statement.version ? 'version ' + r.statement.version : '', r.at]), step, screen: 'statements' }
        : { kind: r.kind, qo: '❝', text: r.text, qc: '❞', qcol: VIOLET, mono: false, meta: join([r.agent, r.at]), step, screen: 'brief' }
    }
    case 'Command': {
      const listed = r.destructive ? 'contains “' + r.destructive + '”' : ''
      return r.result == null
        ? { kind: r.kind, text: r.command, mono: true, glyph: '⋯ ', gcol: '#A1A1AA', meta: join(['no result yet', listed]), step, screen: 'brief' }
        : { kind: r.kind, text: r.command, mono: true, meta: join([r.result, listed]), step, screen: 'brief' }
    }
    case 'File':
      return { kind: r.kind, text: r.path, mono: true, meta: r.changedAt.length ? 'changed at ' + stepsText(r.changedAt.slice(-3)) + (r.changedAt.length > 3 ? ' and earlier' : '') + (r.uncovered ? ' · changed after the last passing test run' : '') : 'run at ' + stepsText(r.runAt), step: r.lastVersion || '', screen: 'changed' }
    case 'Thread':
      return { kind: 'Change', text: r.text, mono: false, meta: join([WHOSE[r.whoseMove] || '', r.files && r.files[0] ? r.files[0].split('/').pop() : '', r.at ? ctx.when(r.at) : '']), step: '', screen: 'brief', change: r.threadId }
    default:
      return { kind: r.kind, text: r.text || '', mono: false, meta: '', step, screen: 'brief' }
  }
}

/** One evidence chip, from the evidence the selector named. */
export function liveEv(e) {
  const step = e.step != null ? 'step ' + e.step + ' ›' : ''
  switch (e.kind) {
    case 'Said':
      return { kind: e.kind, qo: '❝', text: e.text, qc: '❞', mono: false, result: '', badge: '', badgeKind: '', note: join([e.agent, e.at, 'the agent’s own words']), step, detail: e.detail }
    case 'Run':
      return { kind: e.kind, text: e.text, mono: true, result: runCounts(e), badge: join([e.version, e.versionsBehind ? older(e.versionsBehind) : '']), badgeKind: e.versionsBehind ? 'older' : 'current', note: '', step, detail: e.detail }
    case 'Edit':
      return { kind: e.kind, text: e.text, mono: true, result: '', badgeGlyph: e.afterRun ? '◭' : '', badge: e.afterRun ? 'after the run' : '', badgeKind: e.afterRun ? 'after' : '', note: join([e.at, e.versionAfter ? 'then version ' + e.versionAfter : '']), step, detail: e.detail }
    case 'Command':
      return { kind: e.kind, text: e.text, mono: true, result: e.result == null ? '' : e.result, badgeGlyph: e.result == null ? '⋯' : '', badge: e.result == null ? 'no result yet' : '', badgeKind: 'noresult', note: join([e.destructive ? 'contains “' + e.destructive + '”' : '', 'recorded, not judged']), step, detail: e.detail }
    default:
      return { kind: e.kind, text: e.text || '', mono: false, result: '', badge: '', badgeKind: '', note: '', step, detail: e.detail || '' }
  }
}

/** The palette's fixed lines, on the record. */
export function livePal(D) {
  const V = D.version.id
  const on = V ? ' · on ' + V : ''
  const R = D.record
  const L = D.proofStop
  const runs = L.finishedRuns
  /* a check ThruLine ran has no step in the agent's record: it is named by its time */
  const runAt = (r) => (r.step != null ? 'step ' + r.step : 'the check at ' + (r.time || 'its end'))
  const looked = ' ' + (runs.length
    ? plural(runs.length, 'finished test run', 'finished test runs') + ' (' + runs.map((r) => runAt(r) + (r.version ? ' on ' + r.version : '')).join(', ') + ')'
    : 'no finished test run')
    + (L.pendingSteps.length ? ' and ' + stepsText(L.pendingSteps) + (L.pendingSteps.length === 1 ? ', which has no result yet.' : ', which have no result yet.') : '.')
  const st = D.story
  const S = st ? (STATE[st.state] || STATE.not_proven) : STATE.recorded
  const crit = D.criteria
  return {
    version: V || '',
    scope: D.open ? 'this Change' : 'this project',
    goHint: (D.open ? 'in this Change' : 'in ThruLine') + ' · moving between views settles nothing',
    actOn: (V ? 'on ' + V : 'on this Change') + ' · each one shows its key',
    checkSub: (D.check.command || 'no test run recorded yet') + on,
    checkAsks: (D.check.command || 'no test run recorded yet') + on + ' · asks once for this project',
    approveWaits: D.approval.waitsOn != null ? 'waits on ' + plural(D.approval.waitsOn, 'thing', 'things') : 'waits on what counts as done',
    approveWhy: crit.proposed != null
      ? ' · ' + fmtCount(crit.proposed) + ' proposed ' + (crit.proposed === 1 ? 'line' : 'lines') + ' of what counts as done, ' + (crit.accepted ? fmtCount(crit.accepted) + ' accepted' : 'none accepted')
      : ' · what counts as done is not accepted yet',
    defineSub: (crit.proposed != null ? plural(crit.proposed, 'line', 'lines') + ' proposed from your request' : 'not accepted yet') + ' · written only when you accept',
    statementsSub: R.statements != null
      ? plural(R.statements, 'statement', 'statements') + ' · ' + fmtCount(R.statementsNeedingCheck) + (R.statementsNeedingCheck === 1 ? ' needs a check' : ' need a check')
      : 'what the agent said in this Change',
    changedSub: D.diff.edits != null
      ? plural(D.diff.edits, 'edit', 'edits') + ' to ' + plural(D.diff.files, 'file', 'files') + ' · ' + (D.diff.filesChangedAfterLastCheck == null ? 'no passing test run yet' : plural(D.diff.filesChangedAfterLastCheck, 'file changed', 'files changed') + ' after the last passing test run')
      : 'what changed in this Change',
    askedAbout: '· asked ' + (D.askedAt || '') + ' · about ' + (D.open ? 'this Change' : 'this project'),
    looked,
    seeChanged: L.changedSince.file ? 'See what changed in ' + L.changedSince.file : 'See what changed',
    pasteInto: 'Nothing here reaches the agent unless a supported route says it did. You paste it into ' + (D.change.agent || 'your agent') + '.',
    storySaid: st ? join(['said by ' + (st.agent || 'the agent'), st.step != null ? 'step ' + st.step : '', st.at]) : '',
    storyText: st ? st.text : 'No statement is recorded for this Change.',
    storyGlyph: S.glyph,
    storyWord: S.word,
    storyMeta: st && st.version ? '· version ' + st.version : '',
    storyWhy: st && st.why ? st.why : '',
    storyLast: st && st.lastVersionBefore ? 'The last version ThruLine recorded before this was said is ' + st.lastVersionBefore + '. ThruLine doesn’t record the code at the moment it spoke.' : '',
  }
}

/** The header and status bar of the full artboard, on the record (the overlay draws neither). */
export function liveTop(D) {
  const R = D.record
  return {
    project: D.project.name || '',
    title: D.change.title || '',
    version: D.version.id || '',
    versionMeta: D.version.id ? join(['(current)', D.version.commit ? '= commit ' + D.version.commit : '', D.version.unsavedChanges === true ? 'unsaved changes' : D.version.unsavedChanges === false ? 'no unsaved changes' : '']) : '',
    readStamp: 'read ' + (D.readAt || '') + ' ↻',
    status: join([R.steps != null ? plural(R.steps, R.unit === 'threads' ? 'Change' : 'step', R.unit === 'threads' ? 'Changes' : 'steps') : '', R.statements != null ? plural(R.statements, 'statement', 'statements') : '', D.recording ? 'recording' : '']),
    keeping: '',
  }
}

/* ---------------------------------------------------------------------------
 * "Was anything destructive?" — the one list ThruLine already keeps.
 *
 * `thruline explain` flags a command as a "destructive command" when its text
 * contains one of these phrases (crates/explain/src/timeline.rs DESTRUCTIVE,
 * lower-cased text, plain substring). The palette uses that same list and
 * nothing else, and says it is a match on the command's text: recorded, not
 * judged. destructive.test.js reads the Rust list and fails when the two
 * differ, so there is one list.
 * ------------------------------------------------------------------------- */
export const DESTRUCTIVE = Object.freeze([
  'rm -rf',
  'rm -fr',
  'git reset --hard',
  'git clean -fd',
  'git push --force',
  'git push -f',
  'drop table',
  'drop database',
  'truncate table',
  'chmod -r 777',
  'mkfs',
  'dd if=',
  '>/dev/sd',
])

/** The first phrase of the list a command's text contains, or null. */
export function destructivePhrase(command) {
  const h = String(command == null ? '' : command).toLowerCase()
  if (!h) return null
  return DESTRUCTIVE.find((d) => h.includes(d)) || null
}

/* ---------------------------------------------------------------------------
 * Find rows from the record search (data/search.js, GET …/search).
 * ------------------------------------------------------------------------- */
const AGENT_NAME = Object.freeze({ claude_code: 'Claude Code', codex: 'Codex' })
const OUTCOME_WORD = Object.freeze({ succeeded: 'succeeded', failed: 'failed', unknown: 'no result yet' })
const SCREEN_OF_KIND = Object.freeze({ Request: 'brief', Read: 'brief', File: 'changed', Run: 'brief', Command: 'brief', Version: 'brief' })
const VERDICT_SCREEN = new Set(['proven', 'not_proven', 'contradicted'])
const times = (n, one, many) => (n > 1 ? many.replace('N', fmtCount(n)) : one)

/** A short id for a row's DOM id: the same hit always gets the same one. */
function rowKey(parts) {
  let h = 5381
  const s = parts.join('\u0001')
  for (let i = 0; i < s.length; i += 1) h = ((h * 33) ^ s.charCodeAt(i)) >>> 0
  return 'q' + h.toString(36)
}

/**
 * One Find row for one hit the route served: the same fields a live row has,
 * the Change it belongs to (`change`, its thread id) and the Change screen
 * that shows it (`screen`). A hit carries no step: steps are places in one
 * Change's own view, which the route does not number. Its instant is in the
 * meta instead, and so is its Change's title when it is not the open one.
 *
 * @param {object} h  one `results[]` item of the search document
 * @param {{when: (ts: string) => string}} ctx
 */
export function searchHitRow(h, ctx) {
  const T1 = '#EDEDEF'
  const VIOLET = '#B78CFF'
  const agent = AGENT_NAME[h.source] || 'the agent'
  /* as every Change screen shows recorded words: the home folder as ~ (paths.js), and a
     statement's markdown markers dropped (change.js plainText) */
  const other = h.open_thread ? '' : (homeless(h.thread_title) || UNTITLED_WORK)
  const at = h.at ? ctx.when(h.at) : ''
  const shown = homeless(h.kind === 'Said' ? plainText(h.text) : h.text)
  const base = { id: rowKey([h.kind, h.thread_id, h.event_id, h.text]), kind: h.kind, text: shown, title: shown, step: '', change: h.thread_id, screen: SCREEN_OF_KIND[h.kind] || 'brief' }
  switch (h.kind) {
    case 'Request':
      return { ...base, qo: '“', qc: '”', qcol: T1, mono: false, meta: join([h.work === false ? 'you wrote' : 'your request', other, at]) }
    case 'Said': {
      const st = STATE[h.verdict] || STATE.recorded
      return { ...base, qo: '❝', qc: '❞', qcol: VIOLET, mono: false, glyph: st.glyph + ' ', gcol: st.gcol, meta: join([st.word, agent, other, at]), screen: VERDICT_SCREEN.has(h.verdict) ? 'statements' : 'brief' }
    }
    case 'File':
      return { ...base, mono: true, meta: join([times(h.count, 'changed once', 'changed N times'), other, at]) }
    case 'Read':
      return { ...base, mono: true, meta: join(['read by ' + agent + (h.count > 1 ? ', ' + fmtCount(h.count) + ' times' : ''), other, at]) }
    case 'Run': {
      const counts = h.tally ? runCounts({ passed: h.tally.passed, failed: h.tally.failed + (h.tally.errors || 0) }) : (OUTCOME_WORD[h.outcome] || 'no result yet')
      const unknown = h.outcome === 'unknown'
      return { ...base, mono: true, glyph: unknown ? '⋯ ' : '', gcol: '#A1A1AA', meta: join([counts, h.exit_code != null && h.exit_code !== 0 ? 'exit ' + h.exit_code : '', times(h.count, '', 'ran N times'), other, at]) }
    }
    case 'Command': {
      const unknown = h.outcome === 'unknown'
      const phrase = destructivePhrase(h.text)
      return { ...base, mono: true, glyph: unknown ? '⋯ ' : '', gcol: '#A1A1AA', meta: join([OUTCOME_WORD[h.outcome] || 'no result yet', phrase ? 'contains “' + phrase + '”' : '', times(h.count, '', 'ran N times'), other, at]) }
    }
    case 'Version': {
      const v = h.version || {}
      return { ...base, mono: true, meta: join([v.current ? 'current' : '', v.committed ? (v.message ? 'commit “' + homeless(v.message) + '”' : 'commit') : 'not committed', other, at]) }
    }
    default:
      return { ...base, mono: false, meta: join([other, at]) }
  }
}

const UNTITLED_WORK = 'Untitled work'

/**
 * What the record search adds to Find for the text typed now.
 *   rows     Find rows (searchHitRow), the open Change's first
 *   total    everything the route found, of which `rows` are the first
 *   state    'idle' | 'reading' | 'served' | 'failed' — for THIS text
 *   message  the failure, in a person's words
 * An answer for other text than `q` is not this text's: it reads as reading.
 */
export function searchFind(found, q, ctx) {
  const text = String(q == null ? '' : q).trim()
  if (!found || !text) return { rows: [], total: 0, state: 'idle', message: '' }
  const mine = found.q != null && String(found.q).trim() === text
  if (!mine || found.status === 'reading') return { rows: [], total: 0, state: 'reading', message: '' }
  if (found.status === 'failed') return { rows: [], total: 0, state: 'failed', message: found.message || '' }
  if (found.status !== 'served' || !found.doc) return { rows: [], total: 0, state: 'idle', message: '' }
  const rows = found.doc.results.map((h) => searchHitRow(h, ctx))
  return { rows, total: Math.max(found.doc.total, rows.length), state: 'served', message: '' }
}

/* C29: a decision's state as Find draws it, glyph and colour only; the words are the model's sentence. */
const DECISION_GLYPH = Object.freeze({
  broken: ['✕ ', '#FF6369'],
  holds: ['● ', 'var(--tl-proven, #3DD68C)'],
  unverifiable: ['○ ', '#A1A1AA'],
  stale: ['◭ ', '#F2C94C'],
})

/**
 * ⌘K's Find rows for the project's decisions (data/decisions.js
 * `decisionSearchItems`): kind "Decision", the person's own words, the state
 * sentence, and the address where the decision is shown (`hrefFor`), or no
 * link when it is shown nowhere (C30 R-LINKS). Empty unless the list was served.
 */
export function decisionFindRows(model, hrefFor) {
  if (!model || model.status !== 'served') return []
  const byId = {}
  for (const r of model.rows) byId['d:' + r.id] = r
  return decisionSearchItems(model, hrefFor).map((it) => {
    const g = DECISION_GLYPH[byId[it.id] && byId[it.id].state] || ['', '#82828C']
    return { id: it.id, kind: 'Decision', text: it.text, title: it.text, mono: false, glyph: g[0], gcol: g[1], meta: it.sub, step: '', href: it.href || undefined, hay: it.hay }
  })
}
