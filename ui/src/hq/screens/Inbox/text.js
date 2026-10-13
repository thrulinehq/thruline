/* Inbox: every sentence the screen shows, formatted from the data contract
 * (DATA.md). Pure: the same data gives the same words. renderVals() calls it;
 * nothing here fetches, counts anything the data does not carry, or invents a
 * fact — a missing field reads as empty, never as a pass.
 *
 * The Inbox is a list of rows in five groups (whose move it is). Each row has
 * a kind, and each kind has one row markup in template.jsx:
 *   approve  a Change ready for a decision        (Waiting on you)
 *   prompt   something only the person can answer (Waiting on you)
 *   check    the proof stops before the version    (Needs a check)
 *   agent    a note was copied for the agent       (Waiting on the agent)
 *   live     a session recording now               (Recording now)
 * and "Nothing needed" holds approved, dismissed and finished (done) items. */

import { fmtCount } from '../../data/count.js'
import { readingLine } from '../../data/project.js'

/** A sentence from facts: empty when any fact it needs is missing, so a
 *  missing field never reads as "undefined" or as a claim. */
function T(strings, ...vals) {
  if (vals.some((x) => x === undefined || x === null || (typeof x === 'number' && !Number.isFinite(x)))) return ''
  return strings.reduce((acc, str, i) => acc + str + (i < vals.length ? vals[i] : ''), '')
}

/** A count, its thousands separated (data/count.js); a missing value stays missing, so T still drops the sentence. */
const cnt = (x) => (typeof x === 'number' && Number.isFinite(x) ? fmtCount(x) : x)

const NUM_WORD = ['No', 'One', 'Two', 'Three', 'Four', 'Five', 'Six', 'Seven', 'Eight', 'Nine', 'Ten']
const word = (n) => NUM_WORD[n] || fmtCount(n)
const CIRCLED = ['', '①', '②', '③', '④', '⑤', '⑥']
const circled = (n) => CIRCLED[n] || String(n ?? '')
const plural = (n, one, many) => (n === 1 ? one : many)
const has = (x) => x !== undefined && x !== null

/** What a command the agent asks about does, in the two places it is said. */
const EFFECT = {
  'removes-directory': { asks: 'asks to remove a directory', says: 'It removes a directory.' },
  'removes-files': { asks: 'asks to remove files', says: 'It removes files.' },
}

/** A criterion line's state: its glyph and the words a screen reader hears. */
const LINE_STATE = {
  proven: { glyph: '●', words: (ver) => T`proven on ${ver}` },
  recheck: { glyph: '◭', words: () => 'needs a re-check' },
  progress: { glyph: '◐', words: () => 'in progress' },
  notProven: { glyph: '○', words: () => 'not proven' },
}

const NEEDS = { check: 'a check', prompt: 'a prompt answered' }

/** "a", "a and b", "a, b, and c". */
function listJoin(a) {
  if (a.length <= 1) return a.join('')
  if (a.length === 2) return T`${a[0]} and ${a[1]}`
  return T`${a.slice(0, -1).join(', ')}, and ${a[a.length - 1]}`
}

export function fmtBytes(b) {
  if (typeof b !== 'number' || !Number.isFinite(b)) return null
  if (b >= 1e9) return T`${(b / 1e9).toFixed(1)} GB`
  if (b >= 1e6) return T`${(b / 1e6).toFixed(1)} MB`
  if (b >= 1e3) return T`${Math.round(b / 1e3)} KB`
  return T`${b} bytes`
}

/** "6 min", "3 h", "2 d". */
export function fmtWait(min) {
  if (typeof min !== 'number' || !Number.isFinite(min)) return null
  if (min < 60) return `${Math.max(0, Math.round(min))} min`
  if (min < 1440) return `${Math.round(min / 60)} h`
  return `${Math.round(min / 1440)} d`
}

/** One project's sidebar line. */
export function projectLine(p) {
  if (!p) return ''
  // a quick row: its counts are not yet counted (null), and it never reads as 0 or "nothing yet"
  if (p.complete === false) return p.lastRecordedOn ? T`recorded ${p.lastRecordedOn} · not yet counted` : 'not yet counted'
  if (p.recording) return p.changeCount ? T`recording · ${cnt(p.changeCount)} ${plural(p.changeCount, 'Change', 'Changes')}` : 'recording · nothing yet'
  const head = p.lastRecordedOn ? T`recorded ${p.lastRecordedOn}` : 'not recording'
  return p.dismissedCount ? T`${head} · ${cnt(p.dismissedCount)} dismissed` : head
}

/** "⑤ RE-CHECKED ×3"; the row omits ×1 and an unknown count, the list states it. */
export function stopLabel(stop, { always = false } = {}) {
  if (!stop) return ''
  const times = has(stop.lines) && (always || stop.lines > 1) ? ` ×${cnt(stop.lines)}` : ''
  return `${circled(stop.link)} ${stop.name}${times}`
}

function lineCells(change) {
  const lines = (change.criteria && change.criteria.lines) || []
  return lines.map((l) => {
    const st = LINE_STATE[l.state] || { glyph: '', words: () => l.state || '' }
    return { n: String(l.n), glyph: st.glyph, aria: T`Line ${l.n}, ${l.gate ? 'the gate, ' : ''}${st.words(change.version)}` }
  })
}

function critHead(change) {
  const lines = (change.criteria && change.criteria.lines) || []
  const parts = ['What counts as done']
  if (change.criteria && change.criteria.version) parts.push(change.criteria.version)
  parts.push(`${cnt(lines.length)} ${plural(lines.length, 'line', 'lines')}`)
  return parts.join(' · ')
}

function tallyOf(change) {
  const lines = (change.criteria && change.criteria.lines) || []
  const n = (st) => lines.filter((l) => l.state === st).length
  return { total: lines.length, proven: n('proven'), recheck: n('recheck'), progress: n('progress') }
}

/** The thread, version by version, as one sentence for a screen reader. */
export function threadSentence(thread) {
  if (!thread || !thread.length) return ''
  const ev = (e) => {
    if (e.kind === 'run') return T`a ${e.result === 'passed' ? 'passing' : e.result === 'failed' ? 'failed' : 'unfinished'} run at step ${e.step}`
    if (e.kind === 'claim') return T`the statement at step ${e.step}`
    if (e.kind === 'edit') return T`${e.file} changed at step ${e.step}`
    if (e.kind === 'command') return e.result == null ? T`step ${e.step} has no result yet` : T`a command at step ${e.step}`
    return T`step ${e.step}`
  }
  const parts = thread.map((v) => {
    const said = (v.events || []).map(ev)
    if (v.now && v.ran === false) said.push('no run has happened on it')
    return T`${v.version}${v.now ? ', now' : ''}: ${listJoin(said)}.`
  })
  return `${word(thread.length)} ${plural(thread.length, 'version', 'versions')}. ${parts.join(' ')}`
}

/* ---------------------------------------------------------------- rows */

function approveText(c) {
  const t = tallyOf(c)
  const cells = lineCells(c)
  /* The record states what a ready answer rests on only for a tracked Change (the canvas's lines);
   * where it does not, the row says so by drawing none of it — never "0 of 0", never a chain it
   * was not asked about. `since` is when it entered the state; a row from the record has only when
   * the work was last active. */
  const hasLines = t.total > 0
  const hasChain = has(c.chain && c.chain.links)
  return {
    title: c.title,
    version: c.version,
    versionTitle: T`version ${c.version}`,
    aria: T`${c.title}. Ready for approval, ${c.version}.`,
    approveAria: T`Approve ${c.version}. Opens a sheet; nothing is recorded until you confirm.`,
    since: c.since ? T`since ${c.since}` : T`last active ${c.lastActive}`,
    meta: c.since ? T`${c.project} · since ${c.since}` : T`${c.project} · last active ${c.lastActive}`,
    restsShow: hasLines,
    chainShow: hasChain,
    criteria: hasLines,
    chain: hasChain,
    rests: hasLines ? T`Rests on ${cnt(t.proven)} of ${cnt(t.total)} things that count as done, each with a run on ` : '', // NBSP, as the canvas has it
    holdsCount: T` · ${cnt(t.proven)} of ${cnt(t.total)}`,
    pvSub: T`Change · ${c.project} · session ended ${c.sessionEndedAt}`,
    pvVersion: T`· ${c.version}`,
    pvLead: hasLines ? T`Ready for approval for version ${c.version}. It rests on:` : T`Ready for approval for version ${c.version}.`,
    pvRests: T`${cnt(t.proven)} of ${cnt(t.total)} things that count as done, each with a run on `,
    critHead: critHead(c),
    stripAria: `Proof strip, ${cnt(t.total)} ${plural(t.total, 'line', 'lines')}`,
    l0: cells[0] || {},
    l1: cells[1] || {},
    l2: cells[2] || {},
    tally: T`${cnt(t.proven)} of ${cnt(t.total)} proven on `,
    chainLine: T`${cnt((c.chain && c.chain.links) ?? null)} links hold · ${cnt(t.proven)} of ${cnt(t.total)} lines`,
    approveNote: T`Approving records your decision on ${c.version} only. It does not merge, deploy or tell the agent anything.`,
    openHref: c.href,
    approveHref: c.approveHref,
  }
}

function promptText(p, change) {
  const effect = EFFECT[p.effect] || null
  const turn = p.agent ? `Your turn in ${p.agent}` : 'Your turn'
  const wait = fmtWait(p.waitingMinutes)
  const asks = effect ? T`· ${effect.asks} · requested ${p.requestedAt}` : p.asked ? T`The agent asked a question · ${p.requestedAt}` : ''
  return {
    aria: [T`${turn}, ${p.waitingMinutes < 60 ? `${p.waitingMinutes} ${plural(p.waitingMinutes, 'minute', 'minutes')}` : wait}.`, T`${change.title}.`, has(p.step) ? T`Step ${p.step} has no result yet.` : p.asked ? 'The agent asked you something.' : '']
      .filter(Boolean)
      .join(' '),
    turn,
    changeTitle: change.title,
    waited: wait,
    meta: T`${change.project} · waiting ${wait}`,
    command: p.command,
    asks,
    stepLine: T`step ${p.step} · no result yet`,
    pvTitle: p.command || change.title,
    pvSub: p.command ? [T`Step ${p.step}`, 'command', change.title, p.agent].filter(Boolean).join(' · ') : [T`Asked ${p.requestedAt}`, change.project, p.agent].filter(Boolean).join(' · '),
    pvTurn: T`${turn} · ${wait}`,
    pvNoResult: has(p.step) ? T`Step ${p.step} has no result yet.` : p.asked ? 'The agent asked you something, and nothing after it is recorded.' : '',
    pvRequested: effect ? T` was requested at ${p.requestedAt} and nothing after it is recorded. ${effect.says}` : '',
    pvSettle: p.agent
      ? T`Answer it in ${p.agent}, where the agent asked. If it runs, its result shows up here, and at ⑤ RE-CHECKED if it covers `
      : 'Answer it where the agent asked. What happens next shows up here.',
    scope: p.scope,
    pvOn: p.scope ? T` on ${change.version}.` : '',
    openHref: p.href || change.href,
  }
}

function checkText(c) {
  const stops = c.stops || []
  const claim = c.claim || null
  const quotes = (claim && claim.quotes) || []
  const restsOn = c.restsOn || {}
  const uncovered = c.uncovered || {}
  const thread = c.thread || []
  const steps = thread.flatMap((v) => (v.events || []).map((e) => String(e.step)))
  const check = c.check || {}
  const t = tallyOf(c)
  const cells = lineCells(c)
  const recheck = c.state === 'recheck'
  const need = has(c.needEvidence) ? T`${fmtCount(c.needEvidence)} ${plural(c.needEvidence, 'thing still needs', 'things still need')} evidence` : ''
  const stateWords = recheck ? 'Needs a re-check' : 'Not proven'
  // the live record has no criteria, so the lead says what the record does show
  const lead = has(c.needEvidence)
    ? T`${need} before ${c.version} can be approved.`
    : recheck
      ? T`The last passing test run came before ${c.changedFile} changed.`
      : c.lastTestFailed
        ? 'The last test run failed.'
        : 'No passing test run backs what the agent said.'
  return {
    recheck,
    title: c.title,
    version: c.version,
    versionTitle: T`version ${c.version}`,
    aria: [T`${c.title}.`, T`${stateWords}, version ${c.version}.`, need ? `${need}.` : ''].filter(Boolean).join(' '),
    since: c.since ? T`since ${c.since}` : T`last active ${c.lastActive}`,
    meta: c.since ? T`${c.project} · since ${c.since}` : T`${c.project} · last active ${c.lastActive}`,
    needEvidence: need,
    file: c.changedFile,
    moreFiles: c.changedFiles > 1 ? T`\u00a0and ${fmtCount(c.changedFiles - 1)} more` : '', // NBSP: a flex row trims a plain leading space
    lineTail: c.changedFile ? '' : lead,
    rowStop: stopLabel(stops[0]),
    pvSub: [T`Change · ${c.project}`, T`${has(c.sessions) ? fmtCount(c.sessions) : null} ${plural(c.sessions, 'session', 'sessions')}`, c.agent].filter(Boolean).join(' · '),
    pvState: recheck ? 'Needs a re-check ' : 'Not proven ',
    pvVersion: T`· version ${c.version}`,
    pvLead: lead,
    claim: !!claim,
    saidBy: claim ? T`${claim.agent} said ` : '',
    quote0: quotes[0],
    quote1: quotes[1],
    saidAt: claim ? T`${claim.at} · step ${claim.step}` : '',
    rests: has(restsOn.step),
    restsA: T`${has(restsOn.lines) ? fmtCount(restsOn.lines) : null} ${plural(restsOn.lines, 'rests', 'rest')} on step ${restsOn.step}, which ran on `,
    restsVersion: restsOn.version,
    restsB: T` changed at step ${c.changedAtStep}. No run has included `,
    uncoveredScope: uncovered.scope,
    restsC: T` for line ${uncovered.line}.`,
    criteria: t.total > 0,
    critHead: critHead(c),
    stripAria: `Proof strip, ${cnt(t.total)} ${plural(t.total, 'line', 'lines')}`,
    l0: cells[0] || {},
    l1: cells[1] || {},
    l2: cells[2] || {},
    l3: cells[3] || {},
    tally: T`${fmtCount(t.recheck)} need a re-check · ${fmtCount(t.progress)} in progress · ${fmtCount(t.proven)} proven on `,
    stop0: stopLabel(stops[0], { always: true }),
    stop1: stopLabel(stops[1], { always: true }),
    hasThread: thread.length > 0,
    threadAria: threadSentence(thread),
    v0: thread[0] && thread[0].version,
    v1: thread[1] && thread[1].version,
    v2: thread[2] && `${thread[2].version}${thread[2].now ? ' · now' : ''}`,
    s0: steps[0],
    s1: steps[1],
    s2: steps[2],
    s3: steps[3],
    s4: steps[4],
    litA: claim ? T`Lit from step ${claim.step} back to ${restsOn.step}. Hatched where ` : '',
    litB: T` changed at step ${c.changedAtStep}; stops at ${stops[0] ? circled(stops[0].link) : null}.`,
    checkCommand: check.command,
    checkReach: T` on ${c.version} · reaches all ${has(check.reachesLines) ? fmtCount(check.reachesLines) : null} lines · asks ${check.asks}`,
    approvalWaits: has(c.needEvidence) ? T`Approval of ${c.version} waits on all ${fmtCount(c.needEvidence)}.` : '',
    openHref: c.href,
    checkHref: c.checkHref,
  }
}

/* A row from the record carries `toldAct` (data/project.js, from the thread's `told_agent`): the
 * time is the act's own, the words say which act it was, and every sentence says nothing was sent
 * (C30 R-LEDGER). The gallery's canvas row has no `toldAct` and keeps the canvas's words. */
function agentText(c, copied, readAt) {
  const pasted = c.toldAct === 'marked_pasted'
  const recorded = Boolean(c.toldAct)
  const did = pasted ? 'said you pasted' : 'copied'
  return {
    title: c.title,
    version: c.version,
    versionTitle: T`version ${c.version}`,
    aria: T`${c.title}. Not proven, version ${c.version}. You ${did} a note for the agent at ${c.noteCopiedAt}.` + (recorded ? ' Nothing was sent.' : ''),
    since: T`since ${c.since}`,
    meta: T`${c.project} · since ${c.since}`,
    rowStop: stopLabel((c.stops || [])[0]),
    pvSub: T`Change · ${c.project} · since ${c.since}`,
    pvVersion: T`· version ${c.version}`,
    pvLead: T`You ${did} a note for the agent at ${c.noteCopiedAt}. No later work has been recorded since.` + (recorded ? ' Nothing was sent.' : ''),
    rowLine: copied
      ? T`You copied the note again at ${readAt}`
      : (pasted ? T`You said you pasted the note at ${c.noteCopiedAt}` : T`You copied a note for the agent at ${c.noteCopiedAt}`) + (recorded ? ' · nothing was sent' : ''),
    ladder: copied ? T`· ${c.noteCopiedAt}, again ${readAt} · you are here` : T`· ${c.noteCopiedAt} · you are here`,
    note: copied ? T`Copied again at ${readAt}. Paste it into ${c.agent || 'the agent'}; nothing was sent.`
      : recorded ? 'Nothing was sent. ThruLine doesn’t run the agent.'
      : 'Prepared, not sent. ThruLine doesn’t run the agent.',
    noteColor: copied ? '#EDEDEF' : '#A1A1AA',
    openHref: c.href,
    /* a row from the record: "Copy it again" cannot copy here (the Inbox holds no message text), so it
     * opens the sheet where Copy message copies the draft and keeps the act, rather than claim a copy
     * that did not happen */
    recorded,
    tellHref: recorded ? c.checkHref : undefined,
  }
}

function liveText(s, change) {
  const who = s.agent ? `${s.agent} session` : 'Session'
  return {
    aria: [T`${who}${s.project ? ` in ${s.project}` : ''}, recording.`, T`Last step recorded at ${s.lastStepAt}.`].filter(Boolean).join(' '),
    title: T`${who} · ${s.startedAt} to now`,
    lastStep: T`Last step recorded at ${s.lastStepAt}`,
    meta: s.hookVersion ? T`${s.project} · hook ${s.hookVersion}` : s.project,
    summary: [has(s.steps) ? `${cnt(s.steps)} steps` : '', has(s.waitingAtStep) ? `waiting at step ${s.waitingAtStep}` : '', T`part of ${change.title}`].filter(Boolean).join(' · '),
    pvTitle: s.project ? `${who} in ${s.project}` : who,
    pvSub: T`Session · part of ${change.title}`,
    started: s.hookVersion ? T`${s.startedAt} · hook ${s.hookVersion}` : s.startedAt,
    steps: has(s.steps) ? cnt(s.steps) : '',
    lastStepAt: s.lastStepAt,
    waitingAt: T`step ${s.waitingAtStep}, which has no result yet`,
    before: T`Before ${s.hookInstalledAt}`,
    beforeNote: T`not recorded; the hook was installed at ${s.hookInstalledAt}`,
    openHref: s.href || change.href,
  }
}

function nothingText(i) {
  if (!i) return {}
  if (i.kind === 'dismissed') {
    if (i.title) return { kind: 'dismissed', project: T`${i.project} ›`, title: T` ${i.title}`, by: `hidden by ${i.by || 'you'} until the record changes${i.on ? ` · ${i.on}` : ''}`, row: i.row }
    const n = i.notProvenStatements || 0
    return { kind: 'dismissed', project: T`${i.project} ›`, title: ` ${cnt(n)} ${plural(n, 'statement', 'statements')} not proven` }
  }
  if (i.kind === 'done') {
    // a finished thread: nothing open, never a verdict — no Approved diamond, no green
    return { kind: 'done', glyph: '–', word: 'Nothing open', version: i.version, project: T`${i.project} ›`, title: T` ${i.title}`, by: T`last active ${i.on}`, href: i.href }
  }
  return { kind: 'approved', glyph: '◆', word: 'Approved', version: i.version, project: T`${i.project} ›`, title: T` ${i.title}`, by: T`by ${i.by}, ${i.on}` }
}

/**
 * The rows one whose-move filter shows (C30 P0-5, C46 §1.1), from `inboxText`'s groups: the Inbox
 * and Changes both draw through this, so a filter's count is the rows it shows on both, and
 * "Your move" counts exactly the rows `yourMove` counts. "Waiting on the agent" is filed by the
 * record (a note was copied) while every row keeps the engine's move, so "Your move" and
 * "The agent's" each show the waiting rows of their own move only. "The agent's" shows the
 * recording rows of the agent's own threads; "Recording" every recording row.
 */
export function rowsUnder(t, D, f) {
  const changes = (D && D.changes) || {}
  const moveOf = (id) => (changes[id] ? changes[id].move : undefined)
  const sessionMove = (id) => {
    const s = ((D && D.sessions) || []).find((x) => x.id === id)
    return s ? moveOf(s.change) : undefined
  }
  const mine = f === 'all' || f === 'you'
  return {
    you: mine ? t.groups.you : [],
    check: mine ? t.groups.check : [],
    agent: f === 'all' ? t.groups.agent : f === 'you' || f === 'agent' ? t.groups.agent.filter((r) => moveOf(r.id) === f) : [],
    live: f === 'all' || f === 'live' ? t.groups.live : f === 'agent' ? t.groups.live.filter((r) => sessionMove(r.id) === 'agent') : [],
  }
}

/* ---------------------------------------------------------------- screen */

/** Every string the Inbox shows, and every row in its group, from the data contract. */
export function inboxText(D, ui = {}) {
  const changes = D.changes || {}
  const all = Object.values(changes)
  const projects = D.projects || []
  const prompts = D.prompts || []
  const sessions = D.sessions || []
  const nn = D.nothingNeeded || {}
  const items = nn.items || []
  const destructive = nn.destructive || []
  const changeOf = (id) => changes[id] || {}
  const readAt = D.readAt || ''

  const yourMove = all.filter((c) => c.move === 'you').length
  const agentMove = all.filter((c) => c.move === 'agent').length
  const recording = projects.filter((p) => p.recording).length
  // C30 R-LEDGER: a row is "Waiting on the agent" because a note was copied for it (`noteCopiedAt`),
  // whatever the engine says whose move it is: the filing moves, the counts above do not
  const firstAgent = all.find((c) => has(c.noteCopiedAt))
  const copiedId = ui.copied === true ? firstAgent && firstAgent.id : ui.copied || null

  const groups = {
    you: [
      ...all.filter((c) => c.move === 'you' && c.state === 'ready').map((c) => ({ id: c.id, threadId: c.id, kind: 'approve', t: approveText(c) })),
      ...prompts.map((p) => ({ id: p.id, threadId: p.change, kind: 'prompt', t: promptText(p, changeOf(p.change)) })),
    ],
    check: all.filter((c) => c.move === 'you' && (c.state === 'recheck' || c.state === 'notProven') && !has(c.noteCopiedAt)).map((c) => ({ id: c.id, threadId: c.id, kind: 'check', t: checkText(c) })),
    agent: all.filter((c) => has(c.noteCopiedAt)).map((c) => ({ id: c.id, threadId: c.id, kind: 'agent', t: agentText(c, copiedId === c.id, readAt) })),
    live: sessions.map((s) => ({ id: s.id, threadId: s.change, kind: 'live', t: liveText(s, changeOf(s.change)) })),
  }
  const byId = {}
  for (const g of Object.values(groups)) for (const r of g) byId[r.id] = r

  const cb = D.countedBy ? changeOf(D.countedBy.change) : null
  const countNote = cb && cb.id
    ? T`Counted by Change: ${cb.shortName || cb.title} needs ${listJoin((D.countedBy.needs || []).map((k) => NEEDS[k] || k))}.`
    : has(D.threadCount) && D.projectName
      ? T`Counted by Change: every thread of work in ${D.projectName}, ${cnt(D.threadCount)} in all.`
      : 'Counted by Change.'

  const headline = (yourMove ? `${word(yourMove)} ${plural(yourMove, 'thing is', 'things are')} your move.` : 'Nothing is your move.')
    + (agentMove ? ` ${word(agentMove)} ${plural(agentMove, 'is', 'are')} the agent’s.` : '')

  const dismissed = items.filter((i) => i.kind === 'dismissed')
  const cap = D.capture || {}
  // C29 R1: the project's view is still being read — the frame is drawn, and only the list area
  // says so, naming the project; no count, no "nothing is your move", no zero
  const reading = !!D.reading

  const out = {
    groups,
    byId,
    n: {
      you: cnt(groups.you.length),
      check: cnt(groups.check.length),
      agent: cnt(groups.agent.length),
      live: cnt(groups.live.length),
      nothing: cnt(items.length),
      yourMove: cnt(yourMove),
      agentMove: cnt(agentMove),
    },
    hdrProjects: `${cnt(projects.length)} ${plural(projects.length, 'project', 'projects')} · counted by Change`,
    hdrRecording: `Recording · ${cnt(recording)} ${plural(recording, 'project', 'projects')}`,
    // a look that could not read something says so beside the time of the read (D.note, from the data layer)
    hdrRead: T`read ${D.readAt} ↻` + (D.readAt && D.note ? ` · ${D.note}` : ''),
    // live, the badge is the ONE derivation every sidebar shows (data/project.js inboxBadge); null → no badge
    navYou: 'inboxBadge' in D ? (D.inboxBadge == null ? null : cnt(D.inboxBadge)) : cnt(yourMove),
    navYouAria: 'inboxBadge' in D ? D.inboxBadgeAria || null : `${cnt(yourMove)} ${plural(yourMove, 'Change is', 'Changes are')} your move`,
    projects: projects.map((p) => Object.assign(
      { name: p.name, line: projectLine(p), dot: p.recording ? (p.changeCount ? 'live' : 'hollow') : null, href: p.href },
      // live rows: the current project carries the nav's selected treatment, and every line stays on one line, whole in title=
      'current' in p ? { cur: p.current ? 'true' : undefined, bg: p.current ? '#16161A' : undefined, edge: !!p.current, lineTitle: projectLine(p), nameTitle: p.name } : {},
    )),
    capClaude: cap.claudeCode ? T` · hook ${cap.claudeCode.hookVersion}` : '',
    capClaudeShow: !!cap.claudeCode,
    capRun: cap.runCapture ? (cap.runCapture.on ? ' on' : ' off') : '',
    capRunShow: !!cap.runCapture,
    codexDown: !!(cap.codex && cap.codex.answering === false),
    captureShow: !!(cap.claudeCode || cap.runCapture || (cap.codex && cap.codex.answering === false)),
    codexTitle: cap.codex && cap.codex.configured === false ? 'Codex capture is not on' : 'Codex app server not answering',
    codexBody: cap.codex && cap.codex.configured === false ? 'HQ was not started with a Codex observation source.' : 'Nothing from Codex is being recorded until it answers.',
    codexTried: cap.codex && cap.codex.configured === false
      ? T`Tried again at ${ui.retryAt || D.readAt}. Codex capture is still off.`
      : T`Tried again at ${ui.retryAt || D.readAt}. Still not answering, so nothing from Codex is being recorded.`,
    headline,
    countNote,
    nn: {
      dismissed: has(D.nothingNeeded && D.nothingNeeded.dismissedCount) || dismissed.length ? `${cnt(dismissed.length)} dismissed` : '',
      destructive: destructive.length ? `${cnt(destructive.length)} ${plural(destructive.length, 'command that removes files ran', 'commands that remove files ran')}` : '',
      items: items.map(nothingText),
      cmds: destructive.map((c) => ({ command: c.command, where: T`· ${c.change} · ${c.at}` })),
    },
    footer: `${cnt(yourMove)} ${plural(yourMove, 'Change is', 'Changes are')} your move · ${cnt(agentMove)} ${plural(agentMove, 'is', 'are')} the agent’s · ${cnt(sessions.length)} ${plural(sessions.length, 'session', 'sessions')} recording${D.readAt ? ` · read ${D.readAt}` : ''}`,
    storage: T`ThruLine is keeping ${fmtBytes(D.storage && D.storage.bytes)} of records on this machine · `,
  }
  if (!reading) return out
  return Object.assign(out, {
    reading: true,
    headline: readingLine(D.projectName),
    countNote: '',
    footer: '',
    n: { you: '', check: '', agent: '', live: '', nothing: '', yourMove: '', agentMove: '' },
  })
}
