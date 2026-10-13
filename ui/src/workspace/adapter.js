/* ThruLine workspace — the real-data adapter.
 *
 * PURE. No fetch, no network, no `window`, no React. Every read happens in
 * `./data.js`; this file only maps documents to the Session shape and refuses
 * when it cannot.
 *
 * Two stores, never joined (docs/UI-SERVER-CONTRACT.md §2.4, spec-data §B):
 *   §9 fixture contract  — /fixtures/index.json + /fixtures/projects/<id>.json
 *   UI server contract   — /api/explain, /api/claims, /api/state, /api/lanes,
 *                          /api/refresh, /api/acceptance, /api/mind/*
 * `/api/receipt`, `/api/verify`, `/api/asks` and `/api/narrate` DO NOT EXIST
 * (verified at crates/cli/src/uiserve.rs:556-651). Roughly a third of the
 * Session shape — `checks[]`, `receipt.*`, `verify.*`, `asks[]`, `narr.*` — is
 * reachable only from a pinned fixture file today. Every field that has no
 * source is listed in `ABSENT_FIELD_NOTES` with the honest gap string it
 * renders instead, and NONE of them may be filled with a placeholder.
 *
 * §9 `nodes` and this file's `nodes` are DIFFERENT MODELS and conflating them
 * is a correctness bug, not a naming inconvenience (spec-data §F.13): a §9 node
 * is a decision-state node with `dimensions.{lifecycle,evidence,currency}` and
 * no geometry, and the index speaks a different vocabulary ("insufficient
 * evidence", "unfinished", "noted") that is NOT the seven state words.
 *
 * THE WORD MAPPINGS BELOW ARE THE RULING'S, NOT A GUESS. Every branch cites a
 * row of `~/thruline-p3/docs/RULING-state-vocabulary.md` §3, read in full this
 * session, including the 2026-09-03 amendment that added Kept and Broken.
 */

import { countPhrase } from '../lib/voice.js'
import { FACETS, FACET_ALIAS, KIND_LABEL, SEVKEY, WORDKEY, estH, objType, sessionWordFrom } from './logic.js'
import { composeLine, readableLine, viewFromEntry } from './readable.js'
import { foldSteps, placementSeq } from './lod.js'
import { actionLabel } from './plain-language.js'

/**
 * @typedef {object} Session
 * The shape every surface in `src/workspace/` reads. 39 top-level keys
 * (spec-data §A). **Absent means the key is not set** — never `null`-as-a-value,
 * never `''`, never a placeholder.
 * @property {string} key @property {string} label @property {string} project
 * @property {string} [id] @property {string} [idNote]
 * @property {string} [origin] @property {string} [harness] @property {string} [harnessRaw]
 * @property {string} [started] @property {string} [ended] @property {string} [endedNote]
 * @property {number} [eventCount] @property {boolean} eventsComplete @property {string} [eventsNote]
 * @property {string} [readingNote] one honest line about values the transport cut before this reading
 * @property {number} compactions
 * @property {string} [finalTree] @property {string} [commit] @property {string} [chainHead]
 * @property {object} capture @property {object} exits
 * @property {string} [countLine] @property {string|null} sessionWord
 * @property {Array} events @property {object} stop
 * @property {Array} asks @property {Array} claims @property {Array} promises
 * @property {Array} checks @property {Array} testChanges
 * @property {object} [receipt] @property {object} [verify] @property {object} narr
 * @property {Record<string,string>} commentary
 * @property {Array} frames @property {Array} nodes @property {Array} wires
 * @property {string[]} evidencePath @property {object} [unitsDropped]
 */

/**
 * The refusal. Thrown — never returned, never swallowed — when a document's
 * `shape` or `schema_version` is not one this build reads.
 *
 * Rule 12 and the shipped precedent (`mac/shell.jsx:221`, `acceptance.jsx:27`):
 * an unknown shape refuses loudly rather than rendering a guess.
 */
export class UnknownShape extends Error {
  /**
   * @param {string} document which document, in the reader's words, e.g. `'the explain projection'`
   * @param {string} expected what this build reads, e.g. `'thruline.explain v1'`
   * @param {string} got what arrived, e.g. `'thruline.explain v2'`
   */
  constructor(document, expected, got) {
    super(`${document}: this build reads ${expected}; the record says ${got}.`)
    this.name = 'UnknownShape'
    this.document = document
    this.expected = expected
    this.got = got
  }
}

/**
 * Shapes and versions this build reads. Anything else is `UnknownShape`.
 * `asks.json` and `narrate.*.json` carry NO shape and NO schema_version
 * (verified, spec-data §F.10) — gate those on structure instead.
 */
export const READS = Object.freeze({
  explain: { shape: 'thruline.explain', schema_version: 1 },
  claims: { shape: 'thruline.claims', schema_version: 1 },
  verify: { shape: 'thruline.verify-receipt', schema_version: 1 },
  receiptPredicateType: 'https://thrulinehq.com/attestation/execution-check/v0.1',
})

/**
 * The gap strings. A field with no source in the record renders ITS OWN
 * sentence naming the absence — never a dash, never `n/a`, never an invented
 * value (spec-data §D.3). Keys are Session field paths; a `null` value is the
 * ruling "render nothing at all".
 *
 * Transcribed from spec-data §D.3's table.
 */
export const ABSENT_FIELD_NOTES = Object.freeze({
  sentence: null,
  countLine: null,
  'events[].tool': null,
  'claims[].seq': 'seq not in this reading — the span names the event, not its position',
  'promises[].seq': 'seq not in this reading — the span names the event, not its position',
  'claims[].plain': null,
  'claims[].prove': 'What would prove it: not in this reading.',
  'claims[].nextCheck': null,
  'findings[].name': null,
  'findings[].derivedDetail': 'detector detail not in this reading',
  'findings[].ref': null,
  'checks[].command': 'argv not in this reading — the receipt carries argv_hash',
  'checks[].scopeNote': null,
  'checks[].seqHint': 'exec seq not in this reading (join by id not exported)',
  testChanges: null,
  origin: null,
  id: 'session id not exported in this reading',
  ended: 'no ending recorded (receipt carries the epoch sentinel)',
  project: 'project name not in this reading — it is named by the summary index, not by a projection',
  'receipt.trailer': null,
})

/* The epoch sentinel `real-fog` carries for "no ending". Rendering 1970 as a
 * time is the lie; the note is the truth (spec-data §B.1). */
const EPOCH_SENTINEL = '1970-01-01T00:00:00Z'

/* ── The ruled word mappings (RULING-state-vocabulary.md §3) ───────────────── */

/**
 * T8 — were the workspace facts this claim depends on readable?
 *
 * The ONLY reader of the engine's typed `unobservable_cause` in the UI (a
 * fence in `workspace-fog.test.js` fails when another file reads it). The
 * engine sets `workspace_unreadable` when the tree moved after the cited check
 * and git could not say which paths changed (`claims_cmd.rs` `changed_paths`,
 * `binding.rs` `UnobservableCause`). Nothing here reads the binding's prose:
 * an absent or unknown cause is "known", which keeps the RSV §3.1 split.
 *
 * @param {object} claim a served claim (`claims --json` `claims[]`)
 * @returns {boolean} false only when the engine says the workspace was unreadable
 */
export function workspaceFactsKnown(claim) {
  return !(claim && claim.unobservable_cause === 'workspace_unreadable')
}

/**
 * A claim's word. RSV §3.1: three verdicts, and `unverifiable` splits on the
 * instrument's condition — Fog when capture is degraded, or when the
 * workspace the check depends on could not be read (T8: `logic.js` Fog is
 * "the instrument failed or was never pointed here"); Recorded when the
 * instrument is fine.
 *
 * Exported so the Verdict model derives its claim state through this same
 * function rather than a second copy of the rule.
 *
 * @param {string} verdict @param {boolean} degraded
 * @param {boolean} [workspaceKnown] `workspaceFactsKnown(claim)`; true when omitted
 * @returns {string|null} a `WORD` key, or null for a verdict this ruling does not map
 */
export function verdictWord(verdict, degraded, workspaceKnown = true) {
  if (verdict === 'supported') return 'proven'
  if (verdict === 'unsupported') return 'not_proven'
  if (verdict === 'unverifiable') return degraded || workspaceKnown === false ? 'fog' : 'couldnt'
  return null
}

/**
 * An ask's word. RSV §3.2, all 11 `AskStatus` variants including the
 * 2026-09-03 amendment (`violated` → Broken, `held` → Kept).
 *
 * @param {string} status @param {boolean} degraded
 * @returns {string|null} null for `answered`, which takes no word by ruling
 */
function askWord(status, degraded) {
  switch (status) {
    case 'proven':
      return 'proven'
    case 'partial':
    case 'scope_shrunk':
    case 'untouched':
    case 'dropped':
    case 'refused':
      return 'loose'
    case 'claimed_only':
      return 'not_proven'
    case 'answered':
      return null
    case 'violated':
      return 'broken'
    case 'held':
      return 'kept'
    case 'unverifiable':
      return degraded ? 'fog' : 'couldnt'
    default:
      return null
  }
}

/**
 * A promise's word. RSV §3.3: `open`/`dropped` are Loose thread, `fulfilled`
 * takes none — the match is by name and is explicitly not proof.
 *
 * @param {string} status @returns {string|null}
 */
function promiseWord(status) {
  if (status === 'open' || status === 'dropped') return 'loose'
  return null
}

/**
 * The stop word. RSV §3.5, all nine states.
 *
 * @param {string} kind @returns {string|null}
 */
function stopWord(kind) {
  switch (kind) {
    case 'user_interrupt':
    case 'error':
    case 'permission_denied':
    case 'context_limit':
      return 'loose'
    case 'unknown':
      return 'couldnt'
    case 'no_stop_recorded':
    case 'not_observable':
      return 'fog'
    default:
      /* `agent_finished` and `hook_blocked` take no word, by ruling. */
      return null
  }
}

/* ── Small pure helpers ────────────────────────────────────────────────────── */

const isObj = (v) => !!v && typeof v === 'object' && !Array.isArray(v)
const arr = (v) => (Array.isArray(v) ? v : [])

/** Set `key` on `out` only when `v` is neither null nor undefined nor `''`. */
function put(out, key, v) {
  if (v === null || v === undefined || v === '') return
  out[key] = v
}

/** `claude-code` → `Claude Code`. Display casing is the adapter's, and only that. */
function displayHarness(raw) {
  return String(raw || '')
    .split(/[-_\s]+/)
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(' ')
}

/** The `said`/`record`/`do` triple of one narration item, or null. */
function narrTriple(doc, title, itemIndex) {
  if (!isObj(doc)) return null
  const section = arr(doc.sections).find((s) => s && s.title === title)
  if (!section) return null
  const item = arr(section.items)[itemIndex]
  if (!item) return null
  const sentences = arr(item.sentences).map((s) => (s && typeof s.text === 'string' ? s.text : ''))
  return sentences.length ? sentences : null
}

/** One narration slot across both registers, or null when neither has it. */
function narrSlot(narrMgr, narrOp, title, itemIndex) {
  const mgr = narrTriple(narrMgr, title, itemIndex)
  const op = narrTriple(narrOp, title, itemIndex)
  if (!mgr && !op) return null
  return { mgr: mgr || [], op: op || [] }
}

const NARR_SECTIONS = Object.freeze({
  session: 'WHAT HAPPENED',
  stop: 'WHY IT STOPPED',
  verdict: 'WHAT IT CLAIMED, AND WHAT THE RECORD HOLDS',
  promise: 'WHAT IT PROMISED AND DID NOT DO',
  ask: 'WHAT YOU ASKED FOR, AND WHAT YOU GOT',
})

/** The third sentence of a narration slot is the record's own clause. */
function clauseOf(slot) {
  if (!slot) return null
  const s = (slot.op && slot.op[2]) || (slot.mgr && slot.mgr[2]) || ''
  return s || null
}

/* Node family by object type and kind — the rule the design left implicit
 * (spec-data §B.18: `fam` is derivable from `objType(n)` + `word`). */
const FAM_BY_KIND = Object.freeze({
  session_start: 'grey',
  user_prompt: 'blue',
  tool_result: 'grey',
  fs_change: 'teal',
  snapshot: 'teal',
  tool_call: 'ochre',
  note: 'grey',
  exec: 'ochre',
  stop: 'violet',
})
const FAM_BY_SEVERITY = Object.freeze({ fail: 'red', warn: 'ochre', info: 'grey' })
const MONO_KINDS = Object.freeze(['tool_call', 'exec', 'note'])

/**
 * THE ADAPTER. One pure function, seven optional documents in, one Session out.
 *
 * Refuses BEFORE adapting (spec-data §D.1): any document whose `shape` or
 * `schema_version` is not in `READS`, and any receipt whose `predicateType` is
 * not `READS.receiptPredicateType`, throws `UnknownShape`. `asks` and `narr`
 * are gated on structure (`Array.isArray(asks.asks)`,
 * `Array.isArray(narr.sections)`) and refuse otherwise.
 *
 * Ordering the adapter PINS, because the sources disagree: `findings` come from
 * CLAIMS (5 rows) not EXPLAIN (4 rows); `verify.rows` keep the VERIFY file's
 * own order (spec-data §B.10, §D.3).
 *
 * Derived, never authored (each has a ruling, spec-data §C closing note):
 *   `claims[].word` RSV §3.1 · `promises[].word` §3.3 · `asks[].word` §3.2 ·
 *   `stop.word` §3.5 · `sessionWord` §3.7 (via `logic.sessionWordFrom`).
 * A finding gets NO word (§3.6) — severity only.
 *
 * @param {object} docs
 * @param {object|null} [docs.explain]  `thruline.explain` — /api/explain, or a fixture
 * @param {object|null} [docs.claims]   `thruline.claims` — /api/claims, or a fixture
 * @param {object|null} [docs.asks]     `thruline-x asks --json` (fixture only today)
 * @param {object|null} [docs.receipt]  the DSSE envelope — `payload` may be the
 *   base64 the envelope carries, or an already-decoded object
 * @param {object|null} [docs.verify]   `thruline.verify-receipt` (fixture only today)
 * @param {object|null} [docs.narrOp]   narrate.operator.json (fixture only today)
 * @param {object|null} [docs.narrMgr]  narrate.manager.json (fixture only today)
 * @param {object|null} [docs.exits]    `{explain, claims, narrate, verifyReceipt}` —
 *   from the `X-ThruLine-Exit` response headers, or a fixture's EXIT-CODES.txt
 * @param {string} docs.key             the session key the caller files this under
 * @param {string} [docs.project]       §9 `projects/<id>.json .project.name`; NOT in
 *   explain or claims
 * @param {string} [docs.origin]        composed by the caller from the
 *   `X-ThruLine-Projection` header or the fixture path; NEVER invented
 * @returns {Session} with absent fields ABSENT
 * @throws {UnknownShape}
 */
export function adaptSession(docs) {
  const d = docs || {}
  const explain = d.explain || null
  const claimsDoc = d.claims || null
  const asksDoc = d.asks || null
  const receiptDoc = d.receipt || null
  const verifyDoc = d.verify || null
  const narrOp = d.narrOp || null
  const narrMgr = d.narrMgr || null

  /* ── Refuse first (spec-data §D.1). Nothing below runs on an unknown shape. */
  refuseUnknown('the explain projection', explain, READS.explain)
  refuseUnknown('the claims projection', claimsDoc, READS.claims)
  refuseUnknown('the receipt verification', verifyDoc, READS.verify)
  if (asksDoc && !Array.isArray(asksDoc.asks)) {
    throw new UnknownShape('the asks reading', 'an object with an `asks` array', 'an object without one')
  }
  if (narrOp && !Array.isArray(narrOp.sections)) {
    throw new UnknownShape('the operator narration', 'an object with a `sections` array', 'an object without one')
  }
  if (narrMgr && !Array.isArray(narrMgr.sections)) {
    throw new UnknownShape('the manager narration', 'an object with a `sections` array', 'an object without one')
  }
  let payload = null
  if (receiptDoc) {
    /* A DSSE envelope carries `payload` as base64 (that is what `receipt.json`
     * holds on disk and what `thruline receipt` writes). An earlier caller was
     * required to decode it first, and handing the envelope through verbatim —
     * the obvious thing to do — was refused as an unknown shape. Both forms are
     * read here: an already-decoded object, or the base64 the envelope really
     * carries. Decoding is not interpretation; the payload is still checked
     * against the one predicate this build reads, and anything else still
     * refuses loudly rather than rendering a guess. */
    payload = isObj(receiptDoc.payload) ? receiptDoc.payload : null
    if (!payload && typeof receiptDoc.payload === 'string') {
      try {
        const json =
          typeof atob === 'function'
            ? decodeURIComponent(escape(atob(receiptDoc.payload)))
            : Buffer.from(receiptDoc.payload, 'base64').toString('utf8')
        const parsed = JSON.parse(json)
        payload = isObj(parsed) ? parsed : null
      } catch {
        throw new UnknownShape(
          'the receipt envelope',
          'a DSSE `payload` that is base64-encoded JSON, or an already-decoded object',
          'a string this build could not decode as JSON',
        )
      }
    }
    if (!payload) {
      throw new UnknownShape('the receipt envelope', 'a DSSE envelope whose payload is already decoded', 'an envelope with no decoded payload')
    }
    if (payload.predicateType !== READS.receiptPredicateType) {
      throw new UnknownShape('the receipt', READS.receiptPredicateType, String(payload.predicateType))
    }
  }

  const pred = payload && isObj(payload.predicate) ? payload.predicate : {}
  const predSession = isObj(pred.session) ? pred.session : {}
  const predCapture = isObj(pred.capture) ? pred.capture : {}
  const subject = arr(payload && payload.subject)[0] || {}
  const subjectDigest = isObj(subject.digest) ? subject.digest : {}

  /* Capture degradation, from either served source, and only from a literal `true`: a string, a number, null or an
   * absent field is not a degradation (closed world). A DSSE receipt carries it in its predicate when one is handed in.
   * `claims --json` serves the session view's own flag, the one that capped its claims' verdicts (§13), and `/api/claims`
   * is the only one of the two the live shell is handed (STEP0 audit §3.6; docs/INTERFACE-REQUESTS.md row 19). */
  const degraded = predCapture.capture_degraded === true || (!!claimsDoc && claimsDoc.capture_degraded === true)

  const S = /** @type {Session} */ ({})
  S.key = String(d.key)
  S.label = (explain && explain.session) || String(d.key)
  put(S, 'project', d.project)

  // Optional versioned result envelope. No inferred mappings from commands to
  // catalog checks; older servers leave every catalog result unestablished.
  const verification = explain?.verification
  if (verification?.contract === 'thruline.check-results.v1') {
    S.checkResults = arr(verification.results).filter(isObj)
    S.verificationContext = isObj(verification.context) ? verification.context : {}
  }

  /* Identity. All three documents agree on the id; the note only exists when
   * none of them carries it. */
  const id = (explain && explain.session) || (claimsDoc && claimsDoc.session) || predSession.id || null
  if (id) S.id = id
  else S.idNote = ABSENT_FIELD_NOTES.id

  put(S, 'origin', d.origin)
  const harness = predSession.harness || d.harness
  if (harness) {
    S.harness = displayHarness(harness)
    S.harnessRaw = harness
  }
  put(S, 'started', predSession.started)
  if (predSession.ended && predSession.ended !== EPOCH_SENTINEL) S.ended = predSession.ended
  else if (predSession.ended === EPOCH_SENTINEL) S.endedNote = ABSENT_FIELD_NOTES.ended

  /* Events. `tool` has no record source and stays absent (spec-data §B.6). */
  const happened = arr(explain && explain.what_happened)
  const riskBySeq = {}
  arr(explain && explain.risk_flags).forEach((r) => {
    if (r && r.seq != null) riskBySeq[r.seq] = r.risk
  })
  /* `text` is the RECORD VALUE and is never overwritten: every surface keeps it
   * one disclosure deeper, and the copy affordance copies it rather than the
   * derived line (DESIGN-readable-trace.md §1.8). `read` is the humanised
   * struct and `readText` the composed card string — derived here, once, so the
   * graph, the timeline, the table and both docks read one reading and cannot
   * disagree. */
  S.events = happened.map((e) => {
    const row = { seq: e.seq, kind: e.kind, text: e.text }
    put(row, 'tool', e.tool)
    put(row, 'input', e.input)
    put(row, 'risk', riskBySeq[e.seq])
    row.read = readableLine(viewFromEntry(row))
    row.readText = composeLine(row.read)
    return row
  })

  /* §1.6.4: a display cut and a record cut are different facts. The transport
   * cuts every tool input to 50 characters before this reading
   * (`crates/cli/src/explain_cmd.rs:435`), and level 2 cannot show more either
   * — so it is stated ONCE for the session rather than on every card. Rule 12
   * requires it be loud once, not 1,391 times. */
  const shortened = S.events.filter((e) => e.read && e.read.cutUpstream).length
  if (shortened > 0) {
    S.readingNote = `${countPhrase(shortened, 'recorded value was shortened', 'recorded values were shortened')} before this reading · the full value is in the record`
  }

  const recordedCount = predSession.event_count != null ? predSession.event_count : happened.length || null
  if (recordedCount != null) S.eventCount = recordedCount
  S.eventsComplete = recordedCount != null && happened.length === recordedCount
  if (!S.eventsComplete && recordedCount != null) {
    S.eventsNote = `${countPhrase(recordedCount, 'event', 'events')} in the record · ${happened.length} shown in this reading`
  }

  S.compactions = (explain && explain.what_it_forgot && explain.what_it_forgot.compactions) || 0
  if (asksDoc && isObj(asksDoc.units_dropped)) S.unitsDropped = { ...asksDoc.units_dropped }

  put(S, 'finalTree', (claimsDoc && claimsDoc.final_tree_hash) || subjectDigest.gitTree)
  put(S, 'commit', subjectDigest.gitCommit)
  put(S, 'chainHead', predSession.event_chain_head)

  /* Capture. `condition` states the boolean and nothing about a cause, because
   * the record carries no cause (spec-data §D.3). `recorderLine` is BRIEF:1521's
   * instrument sentence and replaces the `capture_fidelity` identifier
   * (spec-doctrine F-9). */
  const capture = {
    sources: arr(predCapture.sources).slice(),
    redactions: predCapture.redactions_applied || 0,
    degraded,
    condition: degraded ? 'degraded' : 'intact',
  }
  const fidelityRow = arr(verifyDoc && verifyDoc.checks).find((c) => c && c.name === 'capture_fidelity')
  if (fidelityRow) {
    capture.recorderLine =
      fidelityRow.outcome === 'pass'
        ? 'The recorder was working for this session.'
        : 'The recorder is not vouched for this session — cause not recorded.'
  }
  S.capture = capture

  S.exits = isObj(d.exits) ? { ...d.exits } : {}

  /* Stop. `seq` and `text` are derived from the stop event; the word is ruled. */
  const why = isObj(explain && explain.why_it_stopped) ? explain.why_it_stopped : {}
  const stopEvent = [...happened].reverse().find((e) => e && e.kind === 'stop') || null
  const stop = {
    kind: why.kind || 'no_stop_recorded',
    summary: why.summary || '',
    seq: stopEvent ? stopEvent.seq : null,
    lastEvents: arr(why.last_events).slice(),
    text: stopEvent ? String(stopEvent.text).replace(/^stopped:\s*/, '') : null,
  }
  put(stop, 'word', stopWord(stop.kind))
  S.stop = stop

  /* Narration. Five slots, two registers, `said`/`record`/`do` order. */
  S.narr = {
    session: narrSlot(narrMgr, narrOp, NARR_SECTIONS.session, 0),
    stop: narrSlot(narrMgr, narrOp, NARR_SECTIONS.stop, 0),
    verdict: narrSlot(narrMgr, narrOp, NARR_SECTIONS.verdict, 0),
    promise: narrSlot(narrMgr, narrOp, NARR_SECTIONS.promise, 0),
    /* items[1] on purpose: items[0] is `template:ask/summary` (spec-data §B.16). */
    ask: narrSlot(narrMgr, narrOp, NARR_SECTIONS.ask, 1),
  }

  /* Asks. */
  S.asks = arr(asksDoc && asksDoc.asks).map((a, i) => {
    const span = isObj(a.span) ? a.span : {}
    const row = {
      id: a.id || null,
      kind: a.kind,
      status: a.status,
      word: askWord(a.status, degraded),
      text: a.text,
      eventId: span.event_id || null,
      offset: Array.isArray(span.offset) ? span.offset.slice() : null,
      extractor: a.extractor,
      node: `ask${i + 1}`,
    }
    if (!row.id) row.idNote = 'ask id not exported in this reading'
    return row
  })

  /* Claims. CLAIMS is richer than EXPLAIN and is the claim identity's source;
   * findings keep the CLAIMS order (spec-data §B.9, §B.10). */
  const claimClause = clauseOf(S.narr.verdict) || ABSENT_FIELD_NOTES['claims[].prove']
  S.claims = arr(claimsDoc && claimsDoc.claims).map((c, i) => {
    const span = isObj(c.span) ? c.span : {}
    const row = {
      id: c.id,
      kind: c.kind,
      text: c.text,
      verdict: c.verdict,
      word: verdictWord(c.verdict, degraded, workspaceFactsKnown(c)),
      /* T8: false only when the engine typed the workspace as unreadable for
       * this claim's check. Carried so every later reader asks this field,
       * never the served claim again. */
      workspaceKnown: workspaceFactsKnown(c),
      verifier: c.verifier,
      scope: c.scope,
      extractor: c.extractor,
      eventId: span.event_id || null,
      offset: Array.isArray(span.offset) ? span.offset.slice() : null,
      seq: null,
      seqNote: ABSENT_FIELD_NOTES['claims[].seq'],
      evidence: arr(c.evidence).slice(),
      binding: c.binding,
      reasons: arr(c.reasons).slice(),
      warnings: arr(c.warnings).slice(),
      findings: arr(c.findings).map((f, findingIndex) => {
        const finding = {
          key: f.detector,
          severity: f.severity,
          detail: typeof f.detail === 'string' ? f.detail : '',
          node: i === 0 && arr(c.findings).findIndex(other => other.detector === f.detector) === findingIndex
            ? `f_${f.detector}` : `f_${f.detector}_${i + 1}_${findingIndex + 1}`,
        }
        put(finding, 'path', f.path)
        put(finding, 'basis', f.basis)
        put(finding, 'resolveWith', f.resolveWith)
        if (!finding.detail) finding.derivedDetail = ABSENT_FIELD_NOTES['findings[].derivedDetail']
        return finding
      }),
      node: i === 0 ? 'clm' : `clm${i + 1}`,
      prove: claimClause,
    }
    return row
  })

  /* Promises. `followUp` and `prove` come out of the operator narration, where
   * the record states them in its own words (spec-data §B.11). */
  const promiseSlot = S.narr.promise
  const promiseFollowUp = promiseSlot && promiseSlot.op && promiseSlot.op[1]
    ? String(promiseSlot.op[1]).replace(/^Loose thread:\s*/, '').replace(/\.$/, '')
    : null
  const promiseClause = clauseOf(promiseSlot)
  const promiseRows = arr((claimsDoc && claimsDoc.promises) || (explain && explain.promises))
  S.promises = promiseRows.map((p, i) => {
    const row = {
      id: p.id,
      kind: p.kind,
      status: p.status,
      word: promiseWord(p.status),
      text: p.text,
      eventId: p.opened_event_id || null,
      seq: null,
      seqNote: ABSENT_FIELD_NOTES['promises[].seq'],
      node: i === 0 ? 'prm' : `prm${i + 1}`,
    }
    put(row, 'followUp', promiseFollowUp)
    put(row, 'prove', promiseClause)
    return row
  })

  /* Checks. Full check objects live ONLY in the DSSE payload; `command` and
   * `scopeNote` have no record source (spec-data §B.12, §F.2, §F.4). */
  const bindingLabel = S.claims[0] && S.claims[0].binding === 'ran_on_final_tree' ? 'ran on the final tree' : null
  S.checks = arr(pred.checks).map((k, i) => {
    const row = {
      id: k.id,
      category: k.category,
      runner: k.runner,
      exit: k.exit_code,
      argvHash: k.argv_hash || null,
      inputTree: k.input_tree_hash,
      bound: !!(S.finalTree && k.input_tree_hash === S.finalTree),
      reexecuted: !!k.reexecuted,
      parsed: isObj(k.parsed) ? { ...k.parsed } : {},
      node: i === 0 ? 'chk' : `chk${i + 1}`,
      commandNote: `argv not in this reading — the receipt carries argv_hash ${k.argv_hash || ''}`.trim(),
      seqHint: ABSENT_FIELD_NOTES['checks[].seqHint'],
    }
    put(row, 'bindingLabel', bindingLabel)
    return row
  })

  /* `testChanges` is a re-presentation of two findings and has no independent
   * source; the panel is omitted rather than invented (spec-data §B.13). */
  S.testChanges = []

  if (payload) {
    const verifier = isObj(pred.verifier) ? pred.verifier : {}
    const promises = isObj(pred.promises) ? pred.promises : {}
    const receipt = {
      trailer: null,
      keyid: (arr(receiptDoc.signatures)[0] || {}).keyid || null,
      payloadType: receiptDoc.payloadType,
      predicateType: payload.predicateType,
      subjectName: subject.name,
      verifierVersion: verifier.thruline_version,
      mode: verifier.mode,
      policyHash: verifier.policy_hash,
      policyInForce: !!(verifyDoc && verifyDoc.policy && verifyDoc.policy.evaluated),
      promisesOpen: arr(promises.open).slice(),
      promisesFulfilled: arr(promises.fulfilled).slice(),
      node: 'rcpt',
    }
    /* Only when the verifier could not bind the head: otherwise the commit is
     * already `S.commit` and repeating it invents a second fact (spec-data §B.14). */
    const headBinding = arr(verifyDoc && verifyDoc.checks).find((c) => c && c.name === 'head_binding')
    if (headBinding && headBinding.outcome !== 'pass') put(receipt, 'commitUnderReview', subjectDigest.gitCommit)
    S.receipt = receipt
  }

  if (verifyDoc) {
    const integrity = isObj(verifyDoc.integrity) ? verifyDoc.integrity : {}
    const policy = isObj(verifyDoc.policy) ? verifyDoc.policy : {}
    const vclaims = isObj(verifyDoc.claims) ? verifyDoc.claims : {}
    S.verify = {
      /* The VERIFY file's own order, preserved (spec-data §B.15). */
      rows: arr(verifyDoc.checks).map((c) => {
        const row = { name: c.name, outcome: c.outcome, detail: typeof c.detail === 'string' ? c.detail : '' }
        put(row, 'reason', c.reason)
        return row
      }),
      conclusion: verifyDoc.conclusion,
      verified: !!verifyDoc.verified,
      status: verifyDoc.status,
      exit: verifyDoc.exit,
      integrity: integrity.status,
      integrityExit: integrity.exit,
      claims: {
        total: vclaims.total || 0,
        supported: vclaims.supported || 0,
        unsupported: vclaims.unsupported || 0,
        unverifiable: vclaims.unverifiable || 0,
      },
      policyEvaluated: !!policy.evaluated,
      policyNote: policy.note,
      integrityNote: integrity.note,
    }
  }

  /* The session line. Its item set is the caller's — here, every claim, ask and
   * promise word plus the stop word — and `sessionWordFrom` does not
   * special-case (spec-data §F.9 is an operator decision, not a code one). */
  const items = []
  S.claims.forEach((c) => c.word && items.push(c.word))
  S.asks.forEach((a) => a.word && items.push(a.word))
  S.promises.forEach((p) => p.word && items.push(p.word))
  if (S.stop.word) items.push(S.stop.word)
  S.sessionWord = S.claims.length === 0 && items.length === 0 ? null : sessionWordFrom(items)

  const countParts = []
  const notProven = S.claims.filter((c) => c.word === 'not_proven').length
  const proven = S.claims.filter((c) => c.word === 'proven').length
  const loose = items.filter((w) => w === 'loose').length
  if (notProven) countParts.push(countPhrase(notProven, 'not proven claim', 'not proven claims'))
  if (proven) countParts.push(countPhrase(proven, 'proven claim', 'proven claims'))
  if (loose) countParts.push(countPhrase(loose, 'loose thread', 'loose threads'))
  if (countParts.length) S.countLine = countParts.join(' · ')

  /* Authored commentary has no record source. An empty map means the `authored`
   * chip simply does not appear (spec-data §D.3). */
  S.commentary = {}

  S.frames = []
  S.nodes = []
  S.wires = []
  S.evidencePath = []
  return autoLayout(S)
}

/**
 * Throw when a document does not declare the shape and version this build reads.
 *
 * @param {string} label the document, in the reader's words
 * @param {object|null} doc @param {{shape:string, schema_version:number}} reads
 */
function refuseUnknown(label, doc, reads) {
  if (!doc) return
  if (doc.shape !== reads.shape) {
    throw new UnknownShape(label, `${reads.shape} v${reads.schema_version}`, String(doc.shape))
  }
  if (doc.schema_version !== reads.schema_version) {
    throw new UnknownShape(label, `${reads.shape} v${reads.schema_version}`, `${reads.shape} v${doc.schema_version}`)
  }
}

/* ── Tier-3 layout ─────────────────────────────────────────────────────────── */

const PAD = 20
const HEADER = 36
const GAP = 14
const GUTTER = 40
const COL_W = 300
const NODE_MIN = 200
const NODE_MAX = 260

/* The layout metrics, exported because a second reader needs them and a second
 * COPY of them would drift. `shell.jsx`'s `flow()` re-stacks a column when the
 * reader expands a group, and it must stack it by the same rule this file used
 * or the collapsed and expanded readings would disagree about where a card is. */
export const LAYOUT_METRICS = Object.freeze({ PAD, HEADER, GAP, GUTTER, COL_W, NODE_MIN, NODE_MAX })

/* Four fixed columns, derived from object type — never from the data
 * (spec-data §E.2 rule 1). `risk` is appended only when the record flags one. */
const COLUMNS = Object.freeze([
  { id: 'request', title: 'Request' },
  { id: 'changes', title: 'Changes' },
  { id: 'execution', title: 'Execution' },
  { id: 'claims', title: 'Claims & evidence' },
])
/* The demo's own frame title is `Risk flags · recorded`, and `recorded` in a
 * label slot is engine vocabulary (`lib/voice.js` ENUMS, `bannedIn`). The
 * auto-layout's title drops it; the demo record's own value is left as
 * authored, and flagged rather than quietly rewritten. */
const RISK_COLUMN = Object.freeze({ id: 'risk', title: 'Risk flags' })

const COLUMN_BY_KIND = Object.freeze({
  session_start: 'request',
  user_prompt: 'request',
  fs_change: 'changes',
  tool_result: 'changes',
  snapshot: 'changes',
  tool_call: 'execution',
  note: 'execution',
  exec: 'execution',
  stop: 'claims',
})
const COLUMN_BY_OBJECT = Object.freeze({
  ask: 'request',
  check: 'execution',
  finding: 'execution',
  claim: 'claims',
  promise: 'claims',
  receipt: 'claims',
  ending: 'claims',
})

/**
 * TIER 3 LAYOUT — the deterministic auto-layout for any session the adapter
 * built from real documents. This does not exist in the design and the port
 * writes it (spec-data §E.2).
 *
 * The three tiers, in resolution order:
 *   1. `logic.LAYOUT[session.key]` — hand-authored, `demo-narrowed` only.
 *   2. per-record `x/y/w` on `frames[]`/`nodes[]` — the other two demo sessions.
 *   3. THIS — computed from `seq` and `logic.estH`, so the same session lays
 *      out identically on every machine. That is what `rebuild`-style
 *      determinism means for a view.
 *
 * Contract kept, point by point:
 *  · Four fixed columns by object type, plus `risk` iff the record flags one;
 *    anything unmapped lands in `execution`; a frame with zero nodes is not
 *    emitted.
 *  · Within a frame, `seq` ascending; derived objects with no `seq` sort last,
 *    by id — so the order is a pure function of the record.
 *  · Position computed, never stored; `estH` remains the height authority.
 *  · Wires: only the eight record-backed relations, with their pins added in
 *    the same pass. The three text joins (`w_cmdres`, `w_ncmd`, `w_skc`) are
 *    NOT auto-emitted: a text join is not a receipt.
 *  · `evidencePath`: breadth-first from the claim node, else the receipt node,
 *    else `[]`.
 *
 * @param {Session} session a session whose `nodes`/`frames` carry no geometry
 * @returns {Session} the same session with `frames`, `nodes` (x/y/w), `wires`,
 *   pins and `evidencePath` filled
 */
export function autoLayout(session) {
  const S = session
  /* THE LEVEL-OF-DETAIL PASS (DESIGN-readable-trace §2.0). It runs here, before
   * a single x/y/w is assigned, so the layout stays a pure function of the
   * record and every downstream surface reads ONE node array. It emits into the
   * `group`/`members`/`rep`/`member` vocabulary `shell.jsx` already honours; it
   * is a producer for an existing contract, not a new subsystem. */
  const nodes = foldSteps(buildNodes(S), S.events).nodes
  const byId = {}
  nodes.forEach((n) => {
    byId[n.id] = n
  })

  const wires = buildWires(S, byId)

  const hasRisk = nodes.some((n) => n.risk)
  const columns = hasRisk ? COLUMNS.concat([RISK_COLUMN]) : COLUMNS.slice()

  const columnOf = (n) => {
    if (n.risk && hasRisk) return 'risk'
    const ot = objType(n)
    if (COLUMN_BY_OBJECT[ot]) return COLUMN_BY_OBJECT[ot]
    if (n.kind && COLUMN_BY_KIND[n.kind]) return COLUMN_BY_KIND[n.kind]
    return 'execution'
  }

  const frames = []
  let frameX = 0
  columns.forEach((col) => {
    const members = nodes.filter((n) => columnOf(n) === col.id)
    /* Doctrine 4: a frame with nothing in it does not render. */
    if (!members.length) return
    members.sort((a, b) => {
      /* A group spans a range and is placed at its first member, so the sort
       * key is `placementSeq`, not `seq` — a group whose own `seq` is unset
       * would otherwise sort to the end of its column, away from the run it
       * stands for. */
      const asv = placementSeq(a)
      const bsv = placementSeq(b)
      const as = asv == null ? Infinity : asv
      const bs = bsv == null ? Infinity : bsv
      if (as !== bs) return as - bs
      return a.id < b.id ? -1 : a.id > b.id ? 1 : 0
    })
    const w = Math.min(NODE_MAX, Math.max(NODE_MIN, COL_W - 2 * PAD))
    let y = HEADER
    /* Members of a collapsed group KEEP a frame — `S.wires` below drops any
     * wire whose endpoint has no frame, and a member with no frame silently
     * drops every wire that ended on it — but they consume no height, because
     * the collapsed reading is the one that paints first. Expanding reflows
     * (`shell.jsx` `flow()`), which is why height is never stored. */
    members.forEach((n) => {
      n.frame = col.id
      n.x = frameX + PAD
      n.w = w
      if (n.member) {
        n.y = y
        return
      }
      n.y = y
      y += estH(n, w) + GAP
    })
    /* `auto` marks a frame this function placed. The three bundled demo
     * sessions carry AUTHORED geometry instead, and `shell.jsx`'s `flow()`
     * must leave those alone — re-stacking a hand-placed layout would move
     * every card in it. */
    frames.push({ id: col.id, title: col.title, x: frameX, y: 0, w: COL_W, h: y + PAD, auto: true })
    frameX += COL_W + GUTTER
  })

  S.frames = frames
  S.nodes = nodes.filter((n) => n.frame)
  S.wires = wires.filter((w) => byId[w.from[0]] && byId[w.to[0]] && byId[w.from[0]].frame && byId[w.to[0]].frame)
  S.evidencePath = evidenceWalk(S.nodes, S.wires)
  return S
}

/**
 * One node per record object, with no geometry. `objType` is set explicitly so
 * a real session is never classified by an id it does not have.
 *
 * @param {Session} S @returns {object[]}
 */
function buildNodes(S) {
  const out = []

  S.events.forEach((e) => {
    const node = {
      id: `ev${e.seq}`,
      objType: 'event',
      kind: e.kind,
      seq: e.seq,
      fam: FAM_BY_KIND[e.kind] || 'grey',
      type: 'EVENT',
      sub: KIND_LABEL[e.kind] || e.kind,
      /* THE CARD LABEL. `String(e.text)` put truncated serialized JSON on the
       * headline a human reads — `Bash {"command":"cat /private/tmp/…` — for
       * 1,427 of this session's 4,243 cards. The label is now the humanised
       * line; the record value stays on `S.events[].text`, one disclosure
       * deeper, byte for byte. */
      title: actionLabel(e),
      lines: ['tool_call', 'exec', 'fs_change'].includes(e.kind) ? [e.readText || e.text].filter(Boolean) : [],
      pinsL: [],
      pinsR: [],
      foot: `seq ${e.seq} · ${e.kind}`,
    }
    if (e.kind === 'fs_change') {
      /* The path off the parsed view rather than off a `modify`-only prefix
       * strip, so a `create` or a `delete` row keeps its file too. */
      const file = viewFromEntry(e).path
      if (file) node.file = file
    }
    // Human action labels use proportional text; exact commands remain below
    // the label and in the original record.
    if (e.risk) {
      node.risk = true
      node.lines = [e.risk]
      node.foot += ' · risk flag'
    }
    out.push(node)
  })

  S.asks.forEach((a) => {
    out.push({
      id: a.node,
      objType: 'ask',
      objId: a.id || undefined,
      fam: 'blue',
      type: 'DERIVED',
      sub: `Ask · ${a.kind}`,
      word: a.word || undefined,
      clause: clauseOf(S.narr.ask),
      title: `“${a.text}”`,
      lines: [],
      pinsL: [],
      pinsR: [],
      foot: a.id || a.idNote || 'ask',
    })
  })

  S.claims.forEach((c) => {
    out.push({
      id: c.node,
      objType: 'claim',
      objId: c.id,
      fam: c.word === 'proven' ? 'green' : 'red',
      type: 'DERIVED',
      sub: `Claim · ${c.kind}`,
      word: c.word || undefined,
      clause: c.prove,
      title: `“${c.text}”`,
      lines: [`scope ${c.scope} · extracted by ${c.extractor}`].concat(c.reasons.length ? [c.reasons.join(' · ')] : []),
      pinsL: [],
      pinsR: [],
      foot: c.id,
    })
    c.findings.forEach((f) => {
      out.push({
        id: f.node,
        objType: 'finding',
        finding: f.key,
        fam: FAM_BY_SEVERITY[f.severity] || 'grey',
        type: 'DERIVED',
        sub: `Finding · ${f.severity}`,
        /* A finding carries no word (RSV §3.6) — severity only. */
        title: f.key,
        lines: [f.detail || f.derivedDetail || ''].filter(Boolean),
        pinsL: [],
        pinsR: [],
        foot: f.path ? `${f.key} · ${f.path}` : f.key,
        file: f.path,
      })
    })
  })

  S.promises.forEach((p) => {
    out.push({
      id: p.node,
      objType: 'promise',
      objId: p.id,
      fam: 'violet',
      type: 'DERIVED',
      sub: `Promise · ${p.kind}`,
      word: p.word || undefined,
      clause: p.prove || clauseOf(S.narr.promise),
      title: `“${p.text}”`,
      lines: p.followUp ? [p.followUp] : [],
      pinsL: [],
      pinsR: [],
      foot: p.id,
    })
  })

  S.checks.forEach((k) => {
    const parsed = k.parsed || {}
    out.push({
      id: k.node,
      objType: 'check',
      objId: k.id,
      fam: 'ochre',
      type: 'RECEIPT',
      sub: `Check · ${k.category}`,
      title: `${k.runner} · exit ${k.exit}`,
      lines: [
        `${parsed.passed || 0} passed · ${parsed.failed || 0} failed · ${parsed.skipped || 0} skipped · ${parsed.tests_total || 0} total`,
        k.bound ? 'ran on the final tree' : 'not bound to the final tree',
        k.reexecuted ? 're-executed' : 'not re-executed',
      ],
      pinsL: [],
      pinsR: [],
      foot: k.id,
    })
  })

  if (S.receipt) {
    out.push({
      id: S.receipt.node,
      objType: 'receipt',
      objId: 'receipt',
      fam: 'green',
      type: 'RECEIPT',
      sub: 'DSSE',
      title: 'Receipt intact',
      lines: [`${S.receipt.mode} · verifier ${S.receipt.verifierVersion}`],
      pinsL: [],
      pinsR: [],
      foot: S.receipt.predicateType,
    })
  }

  /* No stop event in the record, but a ruled word for the ending: the ending is
   * an object in its own right (RSV §3.5). */
  if (S.stop.word && !S.events.some((e) => e.kind === 'stop')) {
    out.push({
      id: 'ending',
      objType: 'ending',
      fam: 'grey',
      type: 'DERIVED',
      sub: 'Ending',
      word: S.stop.word,
      clause: clauseOf(S.narr.stop),
      title: 'No ending recorded',
      lines: S.stop.summary ? [S.stop.summary] : [],
      pinsL: [],
      pinsR: [],
      foot: S.id ? `ses_${S.id}` : 'ending',
    })
  }

  return out
}

/**
 * The eight record-backed relations, and their pins, in one pass. A pin with no
 * wire is dead geometry, so pins are created here and nowhere else.
 *
 * @param {Session} S @param {Record<string, object>} byId
 * @returns {object[]}
 */
function buildWires(S, byId) {
  const wires = []
  const pin = (nodeId, side, id, label, t, c) => {
    const n = byId[nodeId]
    if (!n) return false
    const list = side === 'L' ? n.pinsL : n.pinsR
    if (!list.some((p) => p.id === id)) list.push(c ? { id, label, t, c } : { id, label, t })
    return true
  }
  const join = (id, fromNode, fromPin, toNode, toPin, kind, c, label, fromLabel, toLabel, t) => {
    if (!byId[fromNode] || !byId[toNode]) return
    pin(fromNode, 'R', fromPin, fromLabel, t, c)
    pin(toNode, 'L', toPin, toLabel, t, c)
    wires.push({ id, from: [fromNode, fromPin], to: [toNode, toPin], kind, c, label })
  }

  S.claims.forEach((c) => {
    /* claim ← check: `claims[].evidence[]` names the check id. */
    c.evidence.forEach((evId) => {
      const chk = S.checks.find((k) => k.id === evId)
      if (chk) join(`w_ev_${c.node}_${chk.node}`, chk.node, 'ev', c.node, 'ev', 'ref', 'red', `claims[].evidence → ${evId}`, 'evidence', 'evidence', 'ref')
    })
    /* claim ← finding: membership in `claims[].findings[]`. */
    c.findings.forEach((f) => {
      join(`w_f_${f.node}`, f.node, 'claim', c.node, 'fnd', 'ref', FAM_BY_SEVERITY[f.severity] || 'grey', `finding ${f.key} · ${f.severity}`, 'on claim', 'findings', 'ref')
    })
    /* claim ← span: `claims[].span.event_id`. */
    const ev = c.eventId ? S.events.find((e) => eventIdMatches(S, e, c.eventId)) : null
    if (ev) join(`w_span_${c.node}`, `ev${ev.seq}`, 'span', c.node, 'span', 'inf', 'violet', `span ${c.eventId}`, 'span', 'span', 'ref')
    /* finding ← path: the finding's path is an `fs_change` event's file. */
    c.findings.forEach((f) => {
      if (!f.path) return
      const fsEv = S.events.find((e) => e.kind === 'fs_change' && String(e.text || '').indexOf(f.path) >= 0)
      if (fsEv) join(`w_path_${f.node}`, `ev${fsEv.seq}`, 'path', f.node, 'path', 'ref', 'teal', `path ${f.path}`, 'path', 'path', 'path')
    })
  })

  /* promise ← span: `promises[].opened_event_id`. */
  S.promises.forEach((p) => {
    const ev = p.eventId ? S.events.find((e) => eventIdMatches(S, e, p.eventId)) : null
    if (ev) join(`w_span_${p.node}`, `ev${ev.seq}`, 'span2', p.node, 'span', 'inf', 'violet', `opened at ${p.eventId}`, 'promise', 'opened at', 'ref')
  })

  /* ask ← span: `asks[].span.event_id`. */
  S.asks.forEach((a) => {
    const ev = a.eventId ? S.events.find((e) => eventIdMatches(S, e, a.eventId)) : null
    if (ev) join(`w_span_${a.node}`, `ev${ev.seq}`, 'span', a.node, 'span', 'inf', 'blue', `span ${a.eventId}`, 'span', 'from prompt', 'ref')
  })

  /* check ← tree: `checks[].input_tree_hash` is the final tree. */
  S.checks.forEach((k) => {
    if (!k.bound) return
    const snap = [...S.events].reverse().find((e) => e.kind === 'snapshot')
    if (snap) join(`w_tree_${k.node}`, `ev${snap.seq}`, 'tree', k.node, 'tree', 'ref', 'teal', 'input_tree_hash = the final tree', 'tree', 'tree', 'path')
  })

  /* receipt ← claim / check: membership in the DSSE predicate. */
  if (S.receipt) {
    S.claims.forEach((c) => {
      join(`w_r_${c.node}`, c.node, 'rcpt', S.receipt.node, 'claim', 'ref', 'green', `predicate.verdicts[] · ${c.id}`, 'in receipt', 'verdict', 'ref')
    })
    S.checks.forEach((k) => {
      join(`w_r_${k.node}`, k.node, 'rcpt', S.receipt.node, 'check', 'ref', 'green', `predicate.checks[] · ${k.id}`, 'in receipt', 'check', 'ref')
    })
  }

  return wires
}

/* The record exports no event-id → seq join (spec-data §F.3), so an event id can
 * only be matched when the caller has already attached one. This returns false
 * rather than guessing, which is why the span wires simply do not appear on a
 * projection that cannot support them. */
function eventIdMatches(S, event, eventId) {
  return event && eventId && event.eventId === eventId
}

/**
 * Breadth-first over the emitted wires from the claim node, else the receipt
 * node, else `[]` — and the "follow the evidence" affordance does not render on
 * an empty path (spec-data §E.2 rule 5).
 *
 * @param {object[]} nodes @param {object[]} wires @returns {string[]}
 */
function evidenceWalk(nodes, wires) {
  const start = (nodes.find((n) => objType(n) === 'claim') || nodes.find((n) => objType(n) === 'receipt') || null)
  if (!start) return []
  const adjacency = {}
  wires.forEach((w) => {
    ;(adjacency[w.from[0]] = adjacency[w.from[0]] || []).push(w.to[0])
    ;(adjacency[w.to[0]] = adjacency[w.to[0]] || []).push(w.from[0])
  })
  const seen = { [start.id]: true }
  const order = [start.id]
  const queue = [start.id]
  while (queue.length) {
    const id = queue.shift()
    const next = (adjacency[id] || []).slice().sort()
    next.forEach((m) => {
      if (seen[m]) return
      seen[m] = true
      order.push(m)
      queue.push(m)
    })
  }
  return order
}

/* ── The facet index and the filter grammar ────────────────────────────────── */

/**
 * Facet index for one node — the single most important function for the filter
 * grammar. Produces `Record<facet, string[]>` over the 16 `logic.FACETS`.
 *
 * Every value goes through an `add(k, v)` that DROPS `null` and `''` and
 * stringifies the rest, so a facet is multi-valued by construction. `recorder`,
 * `session` and `date` are present on EVERY node, which makes filtering on them
 * a no-op within one session and selective only in the Explorer's cross-session
 * listing.
 *
 * Transcribed from dc.html L99-L116 / spec-script §F.1's 17-row table,
 * including the `runner: 'pytest'` special case for a mono node whose title
 * matches /pytest/.
 *
 * @param {object} n a graph node
 * @param {Session} session the session it belongs to
 * @returns {Record<string, string[]>}
 */
export function attrs(n, session) {
  const S = session
  const a = {}
  const add = (k, v) => {
    if (v == null || v === '') return
    ;(a[k] = a[k] || []).push(String(v))
  }
  const nodeById = (id) => arr(S.nodes).find((x) => x.id === id) || null
  const ot = objType(n)
  add('object', ot)
  if (n.word) add('word', WORDKEY[n.word])

  const evKind = (seq) => {
    const e = arr(S.events).find((x) => x.seq === seq)
    if (e) add('event', String(e.kind).replace(/_/g, '-'))
  }
  if (ot === 'event' && n.seq != null) evKind(n.seq)
  /* A group inherits the facets of what it contains, so a filter can never
   * select a member without also selecting the card that stands for it. The
   * LOD pass states the member kinds on the group (`lod.js`), which turns a
   * 1,360-member O(N²) resolution per render into a read. The id walk stays
   * for the bundled demo records, whose three group nodes carry no
   * `memberKinds`. */
  if (n.group && Array.isArray(n.memberKinds)) {
    n.memberKinds.forEach((k) => add('event', String(k).replace(/_/g, '-')))
  } else if (n.group && n.members) {
    n.members.forEach((m) => {
      const mn = nodeById(m)
      if (mn && mn.seq != null) evKind(mn.seq)
    })
  }

  const claim0 = arr(S.claims)[0]
  if (ot === 'claim' && claim0) {
    add('claim', String(claim0.kind).replace(/_/g, '-'))
    add('scope', claim0.scope)
    add('binding', claim0.binding === 'ran_on_final_tree' ? 'final-tree' : 'not-bound')
    if (arr(claim0.findings).length) add('has', 'findings')
    arr(claim0.findings).forEach((f) => add('finding', String(f.key).replace(/_/g, '-')))
  }
  if (n.finding) {
    add('finding', String(n.finding).replace(/_/g, '-'))
    const f = findingByKey(S, n.finding)
    if (f) add('severity', SEVKEY[f.severity])
  }
  const check0 = arr(S.checks)[0]
  if (ot === 'check' && check0) {
    add('runner', check0.runner)
    add('check', check0.category)
    /* Only when the record says so: `scopeNote` has no record source (spec-data
     * §B.12), and defaulting an absent note to `whole-suite` would file a check
     * under a scope nothing states. */
    if (check0.scopeNote) add('scope', /narrow/.test(check0.scopeNote) ? 'narrowed' : 'whole-suite')
    add('binding', check0.bound ? 'final-tree' : 'not-bound')
  }
  if (n.mono && /pytest/.test(n.title || '')) add('runner', 'pytest')
  if (n.file) add('file', n.file)
  if (ot === 'promise') add('has', 'promise')
  if (n.risk) {
    add('risk', 'destructive')
    add('has', 'risk-flag')
  }
  add('recorder', S.capture && S.capture.degraded ? 'degraded' : 'intact')
  add('session', S.key)
  if (S.started) add('date', String(S.started).slice(0, 10))
  return a
}

/**
 * The first finding in the session carrying this detector key.
 *
 * @param {Session} S @param {string} key @returns {object|null}
 */
function findingByKey(S, key) {
  for (const c of arr(S.claims)) {
    const f = arr(c.findings).find((x) => x.key === key)
    if (f) return f
  }
  return null
}

/**
 * Tally every facet value across a session's nodes. Drives the Facets menu, the
 * palette's `filter` section, and `parseToken`'s validity test.
 *
 * O(nodes × facets) and called several times per render in the original
 * (spec-script §G.1), so the result is memoised on the session object identity —
 * one entry, replaced whenever the session changes. The cache is keyed on the
 * object, not on `session.key`, so a re-adapted session of the same key does not
 * serve a stale index.
 *
 * @param {Session} session
 * @returns {Record<string, Record<string, number>>} facet → value → count
 */
const facetCache = { session: null, opts: {} }

export function facetOptions(session) {
  if (facetCache.session === session) return facetCache.opts
  const opts = {}
  arr(session && session.nodes).forEach((n) => {
    const a = attrs(n, session)
    Object.keys(a).forEach((k) => {
      opts[k] = opts[k] || {}
      a[k].forEach((v) => {
        opts[k][v] = (opts[k][v] || 0) + 1
      })
    })
  })
  facetCache.session = session
  facetCache.opts = opts
  return opts
}

/**
 * @typedef {object} Chip a filter chip
 * @property {string} text the canonical `facet:value`, or the raw token when unparseable
 * @property {string} [facet] @property {string} [value]
 * @property {boolean} ok
 * @property {string} [why] one of exactly three: `'use facet:value'` ·
 *   `'unknown facet'` · `'no such value in the loaded record'`
 */

/**
 * Parse one filter token. The grammar is exactly `facet:value`, split on the
 * FIRST colon, both halves lower-cased, with exactly two aliases (`state:`→
 * `word:`, `path:`→`file:`) and no negation, quoting, wildcard, boolean keyword,
 * comparison operator or free-text term (spec-script §F.2).
 *
 * Validity is checked against the LOADED RECORD, not a schema: a well-formed
 * `word:broken` is unsupported in a session holding no broken object.
 *
 * An unsupported token is KEPT as a visible chip, never silently dropped, and
 * `matches` ignores it entirely — so it changes nothing about what is shown and
 * stands as a visible record of a request the record could not honour. That is
 * Rule 12 behaviour and it survives the port.
 *
 * @param {string} tok e.g. `'word:not-proven'`
 * @param {Record<string, Record<string, number>>} opts from `facetOptions`
 * @returns {Chip}
 */
export function parseToken(tok, opts) {
  const raw = String(tok)
  const i = raw.indexOf(':')
  if (i < 0) return { text: raw, ok: false, why: 'use facet:value' }
  let f = raw.slice(0, i).toLowerCase()
  const v = raw.slice(i + 1).toLowerCase()
  if (FACET_ALIAS[f]) f = FACET_ALIAS[f]
  if (FACETS.indexOf(f) < 0) return { text: raw, ok: false, why: 'unknown facet' }
  const table = opts || {}
  if (!table[f] || !table[f][v]) {
    return { text: `${f}:${v}`, facet: f, value: v, ok: false, why: 'no such value in the loaded record' }
  }
  return { text: `${f}:${v}`, facet: f, value: v, ok: true }
}

/**
 * OR within a facet, AND across facets. A node missing a facet entirely can
 * never satisfy that bucket — there is no "absent matches anything". Zero chips
 * matches everything (spec-script §F.4).
 *
 * ONE predicate, called from both the graph and `viewCount`; the original
 * duplicated it inline and the two can drift.
 *
 * @param {object} n @param {Session} session @param {Chip[]} chips
 * @returns {boolean}
 */
export function matches(n, session, chips) {
  const by = {}
  arr(chips).forEach((c) => {
    if (c && c.ok) (by[c.facet] = by[c.facet] || []).push(c.value)
  })
  const keys = Object.keys(by)
  if (!keys.length) return true
  const a = attrs(n, session)
  for (const k of keys) {
    const vals = a[k] || []
    if (!by[k].some((v) => vals.indexOf(v) >= 0)) return false
  }
  return true
}
