/* HQ Verdict Panel — the deterministic model behind the Verdict tab.
 * Step 2 and Step 4 of docs/HQ-VERDICT-PANEL-PLAN.md.
 *
 * PURE. No React, no fetch, no clock, no model. One function over the adapted
 * Session (`adapter.js`): the session state word, the headline, the four
 * counts, ranked finding cards with Said / Record / Do, the holds-up list, the
 * four questions, the tree-moved flag, and a Markdown export.
 *
 * THE RULES ARE THE PLAN'S, VERBATIM:
 *   claim state     supported → proven · unsupported → not_proven ·
 *                   unverifiable → couldnt, or fog when capture is degraded
 *   promise state   open / dropped → loose
 *   session state   any not_proven → Not proven; else any loose → Loose
 *                   thread; else any couldnt → Recorded; else Fog when
 *                   degraded; else Proven. Nothing reported → headline/none.
 *   kind label      plan §1.3 mapping; a kind is never invented — when none
 *                   applies the card uses the state's fallback family.
 *   ranking         tier (Not proven → Loose thread → Recorded → Fog),
 *                   then blast radius (distinct paths the findings touch),
 *                   then recency (higher seq first), then input order.
 *
 * Every sentence is rendered through hq-narrate `pick` + `fill`, then passed
 * through `ground()` with the refs that exist in the record and the numbers
 * this model supplied. A sentence that fails grounding falls back down the
 * family chain; a sentence with no ref is an inference and says so.
 */

import { HQ_COPY } from './hq-copy.js'
import { fill, compose, composeQuoted, SlotError, pick, ground, verdictKeys } from './hq-narrate.js'
import { viewFromEntry } from './readable.js'
import { verdictWord } from './adapter.js'
import { escapeMarkdown } from './packet-export.js'

const list = (v) => (Array.isArray(v) ? v : [])
const seqOf = (v) => (typeof v === 'number' && Number.isFinite(v) ? v : null)

/** Tier order for ranking; lower ranks first. */
const TIER = Object.freeze({ not_proven: 0, loose: 1, couldnt: 2, fog: 3, proven: 9 })

/** Detectors that make an unsupported claim "Command failed, then reported done". */
const COMMAND_FAILED = Object.freeze(['test_run_failed', 'reexecution_failed'])

/** The verifier that synthesises a claim for unfinished work the agent never reported done. */
const DERIVED_VERIFIER = /^unclaimed_completion(?:@\d+)?$/

/** Test-integrity detectors: each becomes its own "Tests weakened" card. */
const TESTS_WEAKENED = Object.freeze([
  'assertion_removed', 'assertion_weakened', 'skip_marker_added', 'test_deleted', 'narrowed_run', 'tests_ran_before_final_edit',
])

/* ── per-object state ──────────────────────────────────────────────────── */

/* One rule, the adapter's (T8): `workspaceKnown` is the adapter's typed
 * reading of the engine's cause, and this file never reads the cause itself. */
export function claimState(claim, degraded) {
  return verdictWord(claim.verdict, degraded, claim.workspaceKnown !== false)
}

export function promiseState(promise) {
  return promise.status === 'open' || promise.status === 'dropped' ? 'loose' : null
}

/** The plan §1.3 kind for a claim, or null when none applies. */
export function claimKind(claim, state) {
  const findings = list(claim.findings)
  if (state === 'not_proven') {
    if (findings.some((f) => COMMAND_FAILED.includes(f.key))) return 'command_failed_then_done'
    if (findings.some((f) => f.severity === 'fail')) return 'record_says_otherwise'
    return 'said_done_no_proof'
  }
  if (state === 'couldnt') return 'couldnt_check'
  /* T8: Fog because the workspace could not be read, not because recording
   * stopped — its own family, so it never says "not recorded". */
  if (state === 'fog' && claim.workspaceKnown === false) return 'workspace_unreadable'
  return null
}

/* ── refs and numbers ──────────────────────────────────────────────────── */

/** The Plain-register phrase for a detector id; an unknown id is spelled out, never dropped. */
export function detectorPlain(key) {
  if (!key) return ''
  return HQ_COPY.detectors[key] || String(key).replace(/_/g, ' ')
}

function whenOf(seq) {
  return seq == null ? HQ_COPY.chrome.when_unknown : fill(HQ_COPY.chrome.when_seq, { n: seq })
}

/** The event seq a claim or promise span resolves to, when the record joins it. */
function spanSeq(S, item) {
  if (seqOf(item.seq) !== null) return item.seq
  if (!item.eventId) return null
  const ev = list(S.events).find((e) => e && e.eventId && e.eventId === item.eventId)
  return ev ? seqOf(ev.seq) : null
}

/** Check rows a claim cites, in evidence order. */
function citedChecks(S, claim) {
  const checks = list(S.checks)
  return list(claim.evidence).map((id) => checks.find((k) => k.id === id)).filter(Boolean)
}

/* ── failed command exits and failed tool calls (review-model's rule, restated so this file is pure) ──
 * A failed `exec` row is a failed command exit; a failed `tool_result` row is a failed tool call. The record does not
 * link the two (docs/INTERFACE-REQUESTS.md rows 20-22), so each row is counted as itself under its own kind: never
 * added up as commands, never paired by position, time or order, and never said to be one run or two. */

/** The failed rows named by record kind, in the copy's words: "1 failed command exit and 2 failed tool calls". Empty when there are none. */
function failedList(rows) {
  const F = HQ_COPY.chrome.failed
  const exits = rows.filter((r) => r.kind === 'exec').length
  const calls = rows.filter((r) => r.kind === 'tool_result').length
  const parts = []
  if (exits) parts.push(exits === 1 ? F.exits_one : fill(F.exits_many, { n: exits }))
  if (calls && exits) parts.push(calls === 1 ? F.and_tool_calls_one : fill(F.and_tool_calls_many, { n: calls }))
  if (calls && !exits) parts.push(calls === 1 ? F.tool_calls_one : fill(F.tool_calls_many, { n: calls }))
  return parts.join(' ')
}

function failedRows(S) {
  const rows = list(S.events).map((event) => ({ event, view: viewFromEntry(event) }))
  const failed = rows.filter(({ event, view }) =>
    (event.kind === 'tool_result' && view.ok === false) ||
    (event.kind === 'exec' && view.exitCode != null && view.exitCode !== 0))
  const laterPassed = (row) => rows.some(({ event, view }) => {
    if (seqOf(event.seq) === null || seqOf(row.event.seq) === null || event.seq <= row.event.seq) return false
    if (row.event.kind === 'exec') return event.kind === 'exec' && view.exitCode === 0
    return event.kind === 'tool_result' && view.ok === true && view.tool === row.view.tool
  })
  return failed.map((row) => ({ kind: row.event.kind, seq: seqOf(row.event.seq), node: seqOf(row.event.seq) === null ? null : `ev${row.event.seq}`, recovered: laterPassed(row) }))
}

/* ── one narrated sentence ─────────────────────────────────────────────── */

/**
 * Render `part` of the first family in `keys` whose sentence passes grounding.
 * The last key is `verdict/*`, which always exists, so this never returns
 * nothing; if even that fails grounding, its text is returned with the
 * violations attached so the failure is visible rather than hidden.
 *
 * No fallback is silent: every family passed over (a chain key with no family,
 * a part with no template, a slot `compose` could not fill in the sentence or
 * in its Do prompt, or a sentence grounding refused) is listed in `fallbacks`,
 * in chain order, with its violations. The refs are the chips this function
 * appends, never brackets read back out of the filled text, so a bracket
 * inside a slot value is prose and moves neither the family nor the refs.
 */
function narrate(keys, part, register, values, ctx) {
  const fallbacks = []
  let last = null
  for (const key of keys) {
    if (!Object.prototype.hasOwnProperty.call(HQ_COPY.families, key)) { fallbacks.push({ key, violations: [{ kind: 'no_family' }] }); continue }
    const family = HQ_COPY.families[key][register]
    const template = family[part]
    if (typeof template !== 'string') { fallbacks.push({ key, violations: [{ kind: 'no_template', part }] }); continue }
    let text
    try {
      text = compose(template, values)
    } catch (e) {
      if (!(e instanceof SlotError)) throw e
      fallbacks.push({ key, violations: [{ kind: e.kind, slot: e.slot }] })
      continue
    }
    /* A Do line and its copy-prompt are one instruction: a prompt that cannot
     * fill takes the Do line down the chain with it, rather than leaving a hole
     * ("A test in  changed…") or a silent null. */
    let prompt = null
    if (part === 'do' && typeof family.prompt === 'string') {
      try {
        prompt = compose(family.prompt, values)
      } catch (e) {
        if (!(e instanceof SlotError)) throw e
        fallbacks.push({ key, violations: [{ kind: e.kind, slot: e.slot, part: 'prompt' }] })
        continue
      }
    }
    /* Only the Record part carries receipt chips: it states what the record
     * shows. A Do line is an instruction and is always an inference. */
    const chipRefs = part === 'record' ? ctx.refs : []
    const g = ground(text, { refs: ctx.refs, numbers: ctx.numbers, register, chips: chipRefs })
    const out = { text, role: g.role, refs: chipRefs.slice(), prompt, key, violations: g.violations }
    if (g.ok) return { ...out, fallbacks }
    const passed = { key, violations: g.violations }
    fallbacks.push(passed)
    last = { out, passed }
  }
  if (last) return { ...last.out, fallbacks: fallbacks.filter((f) => f !== last.passed) }
  return { text: '', role: 'inference', refs: [], prompt: null, key: null, violations: [], fallbacks }
}

/* ── one line that may quote someone's words ───────────────────────────── */

/**
 * Render the first candidate a quoting line can show. A Said line walks its family chain's `said` templates, then
 * `chrome.said_unquoted`; a clicked body walks its type's `body`, then its `unquoted` body. The template's own words
 * compose and are grounded like any sentence, with `ctx.refs` as their chips. A quotation shows only when
 * `composeQuoted` finds it is exactly the text of the record its ref resolves to. A quotation is never grounded and
 * licenses nothing: the numbers are the values of the slots the template's own words filled, plus `ctx.numbers`. As
 * in `narrate`, no candidate is passed over silently: each is listed in `fallbacks` with its violations.
 *
 * @param {Array<{key: string, template?: string, part?: string, absent?: object}>} candidates
 * @param {{slot: string, text: string, ref: ?string}|null} quote
 * @param {{refs: string[], numbers: Array, recordText: (ref: string) => (string|undefined)}} ctx
 */
function narrateQuoted(candidates, register, values, quote, ctx) {
  const fallbacks = []
  let last = null
  for (const { key, template, part, absent } of candidates) {
    if (absent) { fallbacks.push({ key, violations: [absent] }); continue }
    if (typeof template !== 'string') { fallbacks.push({ key, violations: [{ kind: 'no_template', part }] }); continue }
    let line
    try {
      line = composeQuoted(template, values, quote, ctx.recordText)
    } catch (e) {
      if (!(e instanceof SlotError)) throw e
      fallbacks.push({ key, violations: [{ kind: e.kind, slot: e.slot }] })
      continue
    }
    if (line.violation) { fallbacks.push({ key, violations: [line.violation] }); continue }
    const numbers = [...line.slots.map((s) => values[s]), ...ctx.numbers]
    const g = ground(line.authored, { refs: ctx.refs, numbers, register, chips: ctx.refs })
    const out = { key, text: line.text, parts: line.parts, quote: line.quote, role: g.role, refs: ctx.refs.slice(), violations: g.violations }
    if (g.ok) return { ...out, fallbacks }
    const passed = { key, violations: g.violations }
    fallbacks.push(passed)
    last = { out, passed }
  }
  if (last) return { ...last.out, fallbacks: fallbacks.filter((f) => f !== last.passed) }
  return { key: null, text: '', parts: [], quote: null, role: 'inference', refs: [], violations: [], fallbacks }
}

/**
 * The text of the claim or promise each ref resolves to: the first claim with that node, else the first promise with
 * it, the one its chip opens. Built once per verdict, so a Said line's lookup does not scan every claim for every card.
 */
function saidRecordIndex(S) {
  const index = new Map()
  for (const item of [...list(S.claims), ...list(S.promises)]) {
    if (item && item.node != null && !index.has(item.node)) index.set(item.node, typeof item.text === 'string' ? item.text : '')
  }
  return index
}

/**
 * A card's Said line: the agent's words, quoted under the chip of the claim or promise they come from. The words are
 * the item's own text, and they show only when they are the text of the record the item's node resolves to. With no
 * record behind the node, the line carries no chip.
 */
function saidLine(saidRecords, keys, register, values, item, seq) {
  const ref = item.node == null ? null : item.node
  const recordText = (r) => (saidRecords.has(r) ? saidRecords.get(r) : undefined)
  const candidates = keys.map((key) => (Object.prototype.hasOwnProperty.call(HQ_COPY.families, key)
    ? { key, template: HQ_COPY.families[key][register].said, part: 'said' }
    : { key, absent: { kind: 'no_family' } }))
  candidates.push({ key: 'chrome.said_unquoted', template: HQ_COPY.chrome.said_unquoted[register], part: 'said' })
  const quote = { slot: 'claim_quote', text: typeof item.text === 'string' ? item.text : '', ref }
  const refs = ref != null && typeof recordText(ref) === 'string' ? [ref] : []
  return { ...narrateQuoted(candidates, register, values, quote, { refs, numbers: [seq == null ? '' : seq], recordText }), when: values.when }
}

/** Node types whose clicked body quotes someone's words, and the slot it quotes them in. */
const CLICKED_QUOTE = Object.freeze({ claim: 'claim_quote', promise: 'claim_quote', ask: 'prompt_quote', user_prompt: 'prompt_quote' })

/** The text of the record a quoting clicked node stands for: its claim, promise, ask, or user_prompt event. Undefined when none. */
function clickedRecordText(S, type, nodeId) {
  const found = type === 'claim' ? list(S.claims).find((c) => c && c.node === nodeId)
    : type === 'promise' ? list(S.promises).find((p) => p && p.node === nodeId)
      : type === 'ask' ? list(S.asks).find((a) => a && a.node === nodeId)
        : type === 'user_prompt' ? list(S.events).find((e) => e && e.kind === 'user_prompt' && seqOf(e.seq) !== null && `ev${e.seq}` === nodeId)
          : undefined
  return found ? (typeof found.text === 'string' ? found.text : '') : undefined
}

/* ── cards ─────────────────────────────────────────────────────────────── */

function claimCard(S, claim, index, register, degraded, failed, saidRecords) {
  const state = claimState(claim, degraded)
  const kind = claimKind(claim, state)
  const seq = spanSeq(S, claim)
  const checks = citedChecks(S, claim)
  const findings = list(claim.findings).filter((f) => !TESTS_WEAKENED.includes(f.key))
  const failing = findings.find((f) => COMMAND_FAILED.includes(f.key)) || findings.find((f) => f.severity === 'fail') || findings[0] || null
  const refs = [claim.node, ...findings.map((f) => f.node), ...checks.map((k) => k.node), seq == null ? null : `ev${seq}`].filter(Boolean)
  const unrecovered = failed.filter((f) => !f.recovered)
  const values = {
    claim_quote: String(claim.text || ''),
    claim_kind: String(claim.kind || '').replace(/_/g, ' '),
    when: whenOf(seq),
    reason: list(claim.reasons).join('; ') || (failing && failing.detail) || HQ_COPY.chrome.no_reason,
    detector: failing ? failing.key : '',
    detector_plain: detectorPlain(failing ? failing.key : ''),
    detail: failing ? (failing.detail || failing.derivedDetail || '') : '',
    path: failing && failing.path ? failing.path : '',
    counts: failedList(unrecovered),
  }
  const numbers = [...Object.values(values), unrecovered.length, seq == null ? '' : seq]
  const ctx = { refs, numbers }
  const keys = verdictKeys(kind, state)
  return {
    id: claim.id || claim.node,
    state,
    kind,
    detector: failing ? failing.key : null,
    tier: TIER[state] ?? 9,
    blast: new Set(list(claim.findings).map((f) => f.path).filter(Boolean)).size,
    seq,
    order: index,
    said: saidLine(saidRecords, keys, register, values, claim, seq),
    record: narrate(keys, 'record', register, values, ctx),
    do: narrate(keys, 'do', register, values, ctx),
    proofNode: claim.node,
    refs,
  }
}

function weakenedCard(S, claim, finding, index, register, saidRecords) {
  const state = finding.severity === 'fail' ? 'not_proven' : 'loose'
  const seq = spanSeq(S, claim)
  const checks = citedChecks(S, claim)
  const refs = [finding.node, claim.node, ...checks.map((k) => k.node)].filter(Boolean)
  const values = {
    claim_quote: String(claim.text || ''),
    claim_kind: String(claim.kind || '').replace(/_/g, ' '),
    when: whenOf(seq),
    reason: finding.detail || finding.derivedDetail || HQ_COPY.chrome.no_reason,
    detector: finding.key,
    detector_plain: detectorPlain(finding.key),
    detail: finding.detail || finding.derivedDetail || '',
    path: finding.path || '',
    counts: '',
  }
  const ctx = { refs, numbers: [...Object.values(values), seq == null ? '' : seq] }
  const keys = verdictKeys('tests_weakened', state)
  return {
    id: finding.node,
    state,
    kind: 'tests_weakened',
    detector: finding.key,
    tier: TIER[state],
    blast: finding.path ? 1 : 0,
    seq,
    order: index,
    said: saidLine(saidRecords, keys, register, values, claim, seq),
    record: narrate(keys, 'record', register, values, ctx),
    do: narrate(keys, 'do', register, values, ctx),
    proofNode: finding.node,
    refs,
  }
}

function promiseCard(S, promise, index, register, saidRecords) {
  const state = promiseState(promise)
  const kind = promise.status === 'dropped' ? 'left_behind' : null
  const seq = spanSeq(S, promise)
  const refs = [promise.node, seq == null ? null : `ev${seq}`].filter(Boolean)
  const values = {
    claim_quote: String(promise.text || ''),
    claim_kind: String(promise.kind || '').replace(/_/g, ' '),
    when: whenOf(seq),
    reason: promise.followUp || HQ_COPY.chrome.no_reason,
    detector: '', detector_plain: '', detail: '', path: '', counts: '',
  }
  const ctx = { refs, numbers: [...Object.values(values), seq == null ? '' : seq] }
  const keys = verdictKeys(kind, state)
  return {
    id: promise.id || promise.node,
    state,
    kind,
    detector: null,
    tier: TIER[state],
    blast: 0,
    seq,
    order: index,
    said: saidLine(saidRecords, keys, register, values, promise, seq),
    record: narrate(keys, 'record', register, values, ctx),
    do: narrate(keys, 'do', register, values, ctx),
    proofNode: promise.node,
    refs,
  }
}

function recordingGapCard(S, index, register) {
  const cap = S.capture || {}
  const reason = cap.degradedReason || cap.recorderLine || cap.condition || HQ_COPY.chrome.no_reason
  const refs = S.receipt && S.receipt.node ? [S.receipt.node] : []
  const values = { claim_quote: '', claim_kind: '', when: HQ_COPY.chrome.when_unknown, reason, detector: '', detector_plain: '', detail: '', path: '', counts: '' }
  const ctx = { refs, numbers: [reason] }
  const keys = verdictKeys('recording_gap', 'fog')
  return {
    id: 'recording_gap',
    state: 'fog',
    kind: 'recording_gap',
    detector: null,
    tier: TIER.fog,
    blast: 0,
    seq: null,
    order: index,
    said: null,
    record: narrate(keys, 'record', register, values, ctx),
    do: narrate(keys, 'do', register, values, ctx),
    proofNode: refs[0] || null,
    refs,
  }
}

function rank(a, b) {
  if (a.tier !== b.tier) return a.tier - b.tier
  if (a.blast !== b.blast) return b.blast - a.blast
  const sa = a.seq == null ? -1 : a.seq
  const sb = b.seq == null ? -1 : b.seq
  if (sa !== sb) return sb - sa
  return a.order - b.order
}

/* ── the session ───────────────────────────────────────────────────────── */

function sessionState(states, degraded, reported) {
  if (states.includes('not_proven')) return 'not_proven'
  if (states.includes('loose')) return 'loose'
  if (states.includes('couldnt')) return 'couldnt'
  /* A claim can be Fog without a recording gap (T8: the workspace could not
   * be read), and a session holding one is never called Proven. */
  if (degraded || states.includes('fog')) return 'fog'
  /* Nothing reported and nothing promised: nothing was proven, so the state
   * is not Proven — the record could not check anything. */
  if (reported === 0) return 'couldnt'
  return 'proven'
}

function headlineFor(state, counts, register, agent, failed, degraded) {
  const values = { agent, ...counts, failed }
  if (counts.reported === 0 && counts.loose === 0) return fill(HQ_COPY.headline['headline/none'][register], values)
  const one = (state === 'proven' && counts.reported === 1) || (state === 'not_proven' && counts.not_proven === 1) || (state === 'loose' && counts.loose === 1)
  /* T8: Fog with no recording gap is Fog from an unreadable workspace; the
   * gap headline ("part of this session was not recorded") would be false. */
  const key = state === 'fog' && !degraded ? 'headline/fog/workspace'
    : one && HQ_COPY.headline[`headline/${state}/one`] ? `headline/${state}/one` : `headline/${state}`
  const parts = [fill(HQ_COPY.headline[key][register], values)]
  if (failed) parts.push(fill(HQ_COPY.headline['headline/clause/failed'][register], values))
  if (state !== 'loose' && counts.loose > 0) {
    parts.push(fill(HQ_COPY.headline[counts.loose === 1 ? 'headline/clause/loose/one' : 'headline/clause/loose'][register], values))
  }
  if (state === 'not_proven' && counts.couldnt > 0) parts.push(fill(HQ_COPY.headline['headline/clause/couldnt'][register], values))
  return parts.join(' ')
}

/**
 * Node-type key into HQ_COPY.clicked for a graph node, or null. A kind or a type resolves by own property only, so
 * `constructor`, `toString`, `__proto__` or `hasOwnProperty` never reaches a function or `Object.prototype` through the
 * prototype, and `link` is the shared link label, not a node type. Each is an unknown kind, and an unknown kind has no
 * clicked body: the copy has no generic clicked template.
 */
function clickedType(node) {
  const t = node.objType || (node.id && /^ev\d+$/.test(node.id) ? 'event' : null)
  const named = (key) => typeof key === 'string' && key !== 'link' && Object.prototype.hasOwnProperty.call(HQ_COPY.clicked, key)
  if (t === 'event') return named(node.kind) ? node.kind : null
  return named(t) ? t : null
}

/**
 * @param {object} session   the adapted Session
 * @param {{register?: 'manager'|'operator'}} [opts]
 */
export function buildVerdict(session, opts = {}) {
  const S = session || {}
  const register = opts.register === 'operator' ? 'operator' : 'manager'
  const degraded = !!(S.capture && S.capture.degraded === true)
  const agent = S.harness || S.harnessRaw || HQ_COPY.chrome.agent_fallback
  const claims = list(S.claims)
  const promises = list(S.promises)
  const checks = list(S.checks)
  const failed = failedRows(S)
  const unrecovered = failed.filter((f) => !f.recovered)
  const saidRecords = saidRecordIndex(S)

  const cards = []
  const holdsUp = []
  /* UNRECORDED-WORK-NOT-SILENT-R3 — every captured claim that raised no claim
   * card: the ones the record backs (also in `holdsUp`), and any whose verdict
   * this build has no state word for. Each carries the same Said line a card
   * would, and the state this file already computed (null when none), so the
   * Change's "what the agent said" section can show every captured claim once
   * without deciding anything again. Nothing here is a finding or a count. */
  const unflagged = []
  let order = 0
  claims.forEach((c) => {
    const state = claimState(c, degraded)
    if (state === 'proven') {
      const cited = citedChecks(S, c)
      const seq = spanSeq(S, c)
      const refs = [c.node, ...cited.map((k) => k.node)].filter(Boolean)
      const values = { claim_quote: String(c.text || ''), claim_kind: String(c.kind || '').replace(/_/g, ' '), when: whenOf(seq), reason: list(c.reasons).join('; ') || HQ_COPY.chrome.no_reason, detector: '', detector_plain: '', detail: '', path: '', counts: '' }
      const held = { id: c.id || c.node, quote: String(c.text || ''), when: whenOf(seq), proofNode: c.node, refs, record: narrate(verdictKeys(null, 'proven'), 'record', register, values, { refs, numbers: [...Object.values(values), seq == null ? '' : seq] }) }
      holdsUp.push(held)
      unflagged.push({ id: held.id, state, proofNode: c.node, said: saidLine(saidRecords, verdictKeys(null, 'proven'), register, values, c, seq), record: held.record })
    } else if (state) {
      cards.push(claimCard(S, c, order++, register, degraded, failed, saidRecords))
    } else {
      const seq = spanSeq(S, c)
      const values = { claim_quote: String(c.text || ''), claim_kind: String(c.kind || '').replace(/_/g, ' '), when: whenOf(seq), reason: HQ_COPY.chrome.no_reason, detector: '', detector_plain: '', detail: '', path: '', counts: '' }
      unflagged.push({ id: c.id || c.node, state: null, proofNode: c.node, said: saidLine(saidRecords, verdictKeys(null, null), register, values, c, seq), record: null })
    }
    /* A3-NOTES-NEVER-OPEN: a test-integrity finding the detector recorded as
     * `info` (a skip marker that left nothing unrun) is a note, for the
     * record — it is never a "Tests weakened" card, so it is never counted as
     * a finding and never puts a Change in front of a person. */
    list(c.findings).forEach((f) => { if (TESTS_WEAKENED.includes(f.key) && f.severity !== 'info') cards.push(weakenedCard(S, c, f, order++, register, saidRecords)) })
  })
  promises.forEach((p) => { if (promiseState(p)) cards.push(promiseCard(S, p, order++, register, saidRecords)) })
  if (degraded) cards.push(recordingGapCard(S, order++, register))
  cards.sort(rank)

  const counts = {
    reported: claims.length,
    proven: holdsUp.length,
    not_proven: claims.filter((c) => claimState(c, degraded) === 'not_proven').length,
    couldnt: claims.filter((c) => claimState(c, degraded) === 'couldnt').length,
    fog: claims.filter((c) => claimState(c, degraded) === 'fog').length,
    loose: promises.filter(promiseState).length,
    failed_exits: failed.filter((f) => f.kind === 'exec').length,
    failed_tool_calls: failed.filter((f) => f.kind === 'tool_result').length,
  }
  const state = sessionState(cards.map((c) => c.state), degraded, counts.reported + counts.loose)
  const headline = headlineFor(state, counts, register, agent, failedList(unrecovered), degraded)
  /* The receipts for the headline's counts: every claim it counts as
   * reported, then every promise it counts as loose, in record order. */
  const headlineRefs = [...claims.map((c) => c.node), ...promises.filter(promiseState).map((p) => p.node)].filter(Boolean)

  const trees = new Set(list(S.events).filter((e) => e.kind === 'snapshot').map((e) => viewFromEntry(e).treeHash || e.text))
  const checked = {
    events: list(S.events).length,
    claims: claims.length,
    checks: checks.length,
    trees: trees.size,
    verifiers: new Set(claims.map((c) => c.verifier).filter(Boolean)).size,
    /* Claims the verifier synthesised rather than the extractor matched
     * (`unclaimed_completion@N`): the engine's `find_claims` count excludes
     * them, `reported` includes them. The panel names that difference with
     * these as its receipts. */
    derivedRefs: claims.filter((c) => DERIVED_VERIFIER.test(String(c.verifier || ''))).map((c) => c.node).filter(Boolean),
  }

  const cited = claims.flatMap((c) => citedChecks(S, c))
  const treeMoved = !!S.finalTree && cited.some((k) => k.inputTree !== S.finalTree)
  const steps = cards.filter((c) => c.do && c.do.text).length
  const A = HQ_COPY.questions.answers
  const questions = [
    { key: 'asked_vs_got', label: HQ_COPY.questions.asked_vs_got, answer: fill(A.asked_vs_got, counts), refs: holdsUp.map((h) => h.proofNode) },
    { key: 'failed_commands', label: HQ_COPY.questions.failed_commands, answer: failed.length === 0 ? A.failed_none : unrecovered.length === 0 ? fill(A.failed_later_passed, { failed: failedList(failed) }) : failedList(failed), refs: failed.map((f) => f.node).filter(Boolean) },
    { key: 'evidence_covers_latest', label: HQ_COPY.questions.evidence_covers_latest, answer: !cited.length || !S.finalTree ? A.covers_none : treeMoved ? A.covers_no : A.covers_yes, refs: cited.map((k) => k.node) },
    { key: 'next', label: HQ_COPY.questions.next, answer: steps === 0 ? A.steps_none : steps === 1 ? A.steps_one : fill(A.steps, { n: steps }), refs: cards.map((c) => c.proofNode).filter(Boolean) },
  ]

  const nodes = list(S.nodes)
  /* The body goes through `narrateQuoted`: its own words are grounded with the node's chip, and a claim's, promise's,
   * ask's or user prompt's text is a quotation of that record's text, never a word of the sentence. A body that cannot
   * be shown falls back to the type's `unquoted` body, which names the node and quotes nothing. */
  function clicked(nodeId) {
    const node = nodes.find((n) => n.id === nodeId)
    if (!node) return null
    const type = clickedType(node)
    if (!type) return null
    const tpl = HQ_COPY.clicked[type]
    const seq = seqOf(node.seq)
    const detail = (list(node.lines)[0]) || node.title || node.sub || ''
    const values = { when: whenOf(seq), detail }
    const slot = Object.prototype.hasOwnProperty.call(CLICKED_QUOTE, type) ? CLICKED_QUOTE[type] : null
    const recordText = (ref) => (ref === nodeId ? clickedRecordText(S, type, nodeId) : undefined)
    const quote = slot ? { slot, text: recordText(nodeId), ref: nodeId } : null
    const candidates = [['body', tpl.body], ['unquoted', tpl.unquoted]].filter(([, t]) => typeof t === 'string').map(([part, template]) => ({ key: `clicked.${type}.${part}`, template, part }))
    const body = narrateQuoted(candidates, register, values, quote, { refs: [nodeId], numbers: [seq == null ? '' : seq], recordText })
    /* A user prompt's text is the session timeline's (`explain` `what_happened[].text`): the message's first line, masked
     * and shortened when long (`crates/cli/src/explain_cmd.rs` `truncate`). It is quoted exactly as the timeline holds it,
     * but it is not the whole message, so its quotation is marked `excerpt` and never labelled as the words in full. */
    const shown = type === 'user_prompt' ? { ...body, parts: body.parts.map((p) => (typeof p.quote === 'string' ? { ...p, excerpt: true } : p)) } : body
    return { type, title: fill(tpl.title, values), body: shown, link: HQ_COPY.clicked.link, proofNode: nodeId, refs: [nodeId] }
  }

  /* EXPORT-MD-ESCAPE-R4 (D-246): this file is one a person shares, and the claim, the reason, the path and the
   * quotation in it are text the agent or a transcript wrote. Each is written by `packet-export.js`'s `escapeMarkdown`,
   * the helper the packet export uses, so none can be read as a tag, a link, an image or a heading by any renderer.
   * The file's own words (its headings, its counts, its labels, its footer) and its receipt chips are not passed through
   * it as a whole; a chip's text is (a node id is the engine's, and it costs nothing to be sure). */
  function exportMarkdown() {
    const V = register === 'manager' ? { state, headline, counts, findings: cards, holdsUp } : buildVerdict(S, { register: 'manager' })
    const C = HQ_COPY.chrome
    const t = escapeMarkdown
    const chips = (refs) => (refs.length ? ' ' + refs.map((r) => `[${t(r)}]`).join(' ') : '')
    const L = [`# ${C.export_title} — ${HQ_COPY.states[V.state].word}`, '', t(V.headline), '', [
      fill(C.counts.reported, { n: V.counts.reported }), fill(C.counts.proven, { n: V.counts.proven }),
      fill(C.counts.not_proven, { n: V.counts.not_proven }), fill(C.counts.couldnt, { n: V.counts.couldnt }),
    ].concat(V.counts.fog ? [V.counts.fog === 1 ? C.counts.fog_one : fill(C.counts.fog, { n: V.counts.fog })] : []).join(' · ')]
    if (V.findings.length) {
      L.push('', `## ${C.went_wrong}`)
      V.findings.forEach((card) => {
        L.push('', `### ${HQ_COPY.states[card.state].word}${card.kind ? ` · ${HQ_COPY.kinds[card.kind]}` : ''}`)
        if (card.said) L.push(`- ${C.rows.said}: ${t(card.said.text)}${chips(card.said.refs)}`)
        L.push(`- ${C.rows.record}: ${t(card.record.text)}${chips(card.record.refs)}`)
        L.push(`- ${C.rows.do}: ${t(card.do.text)}`)
      })
    }
    if (V.holdsUp.length) {
      L.push('', `## ${V.holdsUp.length === 1 ? C.holds_up_one : fill(C.holds_up_many, { n: V.holdsUp.length })}`)
      V.holdsUp.forEach((h) => L.push(`- "${t(h.quote)}" — ${t(h.record.text)}${chips(h.refs)}`))
    }
    L.push('', C.footer)
    return L.join('\n') + '\n'
  }

  return { state, headline, headlineRefs, counts, checked, findings: cards, holdsUp, unflagged, clicked, questions, treeMoved, exportMarkdown, register }
}

export default buildVerdict
