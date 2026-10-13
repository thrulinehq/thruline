/* L1 derivation — the board, from one project fixture.
 *
 * Everything here is a *selection* over the §9 document plus the voice
 * authority's own composers (`word`, `sentence`, `composition`, `countPhrase`).
 * No English is written in this file and none is written in L1.jsx: a sentence
 * composed in a view is a sentence no lexicon gate has read (§5v).
 *
 * Ordering is taken from the export's own canonical order, never from parsing a
 * clock in the browser — two runs must produce the same board. */

import { SHADOW_STATES, normalizeNode, surfaceProjection } from './fixture.js'
import { VERDICT, composition, countPhrase, sentence, word } from './voice.js'

/** Composition word back to its state slug — the index emits words, and glyph
 *  and colour are keyed by slug. Built from the authority, never hand-listed. */
export const SLUG_FOR_WORD = Object.fromEntries(
  Object.entries(VERDICT).map(([slug, row]) => [row.word, slug]),
)

/**
 * A class has graduated when it is no longer a shadow class.
 *
 * This is the single gate LEFTOVERS is built behind. The region is designed
 * into the layout and returns nothing while its class is in shadow — the same
 * discipline as the benched ghost, one altitude up (§5w, doctrine's register
 * table: LEFTOVERS renders "never, until graduation rules the class in").
 */
export const graduated = (cls) => !SHADOW_STATES.has(cls)

/**
 * The uneaten region's contents, or `null` when its class is still in shadow.
 *
 * The gate is a function rather than a comment or a deleted file, so it can be
 * called in a test: `uneaten('dropped', rows)` returns `null` today and will
 * return rows the moment `dropped` leaves the shadow set — and not a day
 * sooner. L1's LEFTOVERS section renders exactly what this returns.
 */
export function uneaten(cls, rows = []) {
  if (!graduated(cls)) return null
  return { label: 'LEFTOVERS', rows }
}

/**
 * Proof currentness for a project as a whole.
 *
 * Only the negative freshness fact is provable (§M4.4): a receipt is `stale`,
 * or its currentness is `unknown`. One stale receipt therefore governs and is
 * never outvoted by current ones — the worse fact is the knowable one.
 */
export function proofKey(capture) {
  if (capture?.counts?.stale > 0) return 'stale'
  if (capture?.counts?.current > 0) return 'current'
  return 'unknown'
}

/** Non-colour cue per state. Colour reinforces; the glyph carries. */
export const GLYPH = {
  verified: '◉',
  supported: '●',
  contradicted: '◆',
  stale: '◌',
  pending: '▪',
  recorded: '·',
  unknown: '○',
  refused: '—',
}

export const stateColor = (state) => {
  switch (state) {
    case 'contradicted':
      return 'var(--danger)'
    case 'stale':
      return 'var(--drift)'
    case 'pending':
      return 'var(--coin)'
    case 'supported':
    case 'verified':
      return 'var(--phos)'
    default:
      return 'var(--ink-3)'
  }
}

/** The order chips are read in — worst first, so the eye lands on the problem. */
const CHIP_ORDER = [
  'contradicted',
  'stale',
  'pending',
  'unknown',
  'supported',
  'verified',
  'recorded',
  'refused',
]

/**
 * The composed chip line for a set of counts.
 *
 * Zero-count parts vanish inside `composition`, once, rather than at each call
 * site — the rule that `Refused 0` broke by living at the call sites (§4).
 */
export function chipsFrom(counts) {
  const out = []
  for (const state of CHIP_ORDER) {
    const n = counts?.[state] || 0
    if (n > 0) out.push({ state, count: n, word: word(state) })
  }
  return out
}

export const chipLine = (chips) => composition(chips.map((c) => [c.count, c.word]))

/**
 * The pips of one run, and of the replay: its claims in the export's recorded
 * order, one entry per node, each carrying its state (RA3, design S3).
 *
 * This is the single selection both the run card's pip row and THE RUN replay
 * canvas draw from, so the two can never disagree about what the run held.
 * Order is the ledger's own — never a clock, never a re-sort. Colour is
 * assigned at the call site from `stateColor`; here there are only facts.
 */
export function runPips(ledger, runId) {
  /* The ledger arrives newest-first; a run replays oldest-first. */
  return ledger
    .filter((r) => r.run === runId)
    .slice()
    .reverse()
    .map((r) => ({ id: r.id, state: r.state, word: r.word }))
}

/**
 * The board: run cards and the dot ledger, derived from one §9 document.
 *
 * @param {object} raw   the project fixture, exactly as exported
 * @param {number} ledgerWindow  how many claims the ledger opens with
 */
export function board(raw, { ledgerWindow = 50 } = {}) {
  const data = surfaceProjection(raw)
  const contractVersion = Number(data.contract_version || 1)
  const nodes = (data.nodes || []).map((n) => normalizeNode(n, contractVersion))

  const sessions = data.sessions || []
  const orderOf = new Map(sessions.map((s, i) => [s.id, i]))

  /* Runs, newest first. `sessions` arrives in the export's canonical order and
     the export orders it oldest-first, so the reversal is the whole sort. */
  const bySession = new Map(sessions.map((s) => [s.id, []]))
  const orphans = []
  for (const n of nodes) {
    const bucket = bySession.get(n.session_id)
    if (bucket) bucket.push(n)
    else orphans.push(n)
  }

  const runs = sessions
    .map((s) => {
      const own = bySession.get(s.id) || []
      const counts = {}
      for (const n of own) counts[n.state] = (counts[n.state] || 0) + 1
      const claimTotal = own.reduce((sum, n) => {
        const decomposed = data.receipts?.[n.id]?.claim?.claims
        return sum + (Array.isArray(decomposed) && decomposed.length > 0 ? decomposed.length : 1)
      }, 0)
      return {
        id: s.id,
        date: s.date,
        messages: countPhrase(s.msgs, 'message', 'messages'),
        /* A run with no claims drops the clause rather than appending "0
           claims" to the one beside it — the `· 0 conflicts` shape §4 bans.
           The run itself still renders: it happened, and it has messages.

           COUNTS CLAIMS, NOT NODES. This said `own.length`, which is the node
           count, while the label said "claims" — and a node composes many
           claims, so the number under-reported the thing it named. Verdicts
           live at claim level (A115); so does this count. Each node
           contributes its receipt's claim decomposition, or 1 when the export
           carries no decomposition for it, because the export gives every node
           a `claim.text` and `claims` is the finer split of that one claim. */
        claims: claimTotal > 0 ? countPhrase(claimTotal, 'claim', 'claims') : '',
        claimCount: claimTotal,
        verdictLine: composition(chipsFrom(counts).map((c) => [c.count, c.word])),
      }
    })
    .reverse()

  /* The dot ledger, newest first. Rank is (session order, position within the
     session) reversed — the export's own order, so the browser never parses a
     clock and two runs list identically. */
  const ranked = [...nodes]
    .map((n, i) => ({ n, i }))
    .sort((a, b) => {
      const sa = orderOf.has(a.n.session_id) ? orderOf.get(a.n.session_id) : -1
      const sb = orderOf.has(b.n.session_id) ? orderOf.get(b.n.session_id) : -1
      if (sa !== sb) return sb - sa
      return b.i - a.i
    })

  const ledger = ranked.map(({ n }) => {
    const receipt = data.receipts?.[n.id]
    return {
      id: n.id,
      run: n.session_id,
      state: n.state,
      glyph: GLYPH[n.state] || GLYPH.unknown,
      word: word(n.state),
      /* Ours, from the authority. The project's own words sit beside it in
         `subject` and are never rewritten to fit our table. */
      sentence: sentence(n.state),
      subject: receipt?.claim?.text || n.label || '',
      date: sessions[orderOf.get(n.session_id)]?.date || '',
    }
  })

  const counts = {}
  for (const n of nodes) counts[n.state] = (counts[n.state] || 0) + 1

  return {
    name: data.project?.name || '',
    source: data.project?.source,
    chips: chipsFrom(counts),
    runs,
    ledger,
    ledgerWindow,
    /* Nothing is hidden silently: what the window leaves out is stated with its
       noun, and the control that opens it sits beside the statement (§4). */
    ledgerRest: Math.max(0, ledger.length - ledgerWindow),
    restLine:
      ledger.length > ledgerWindow
        ? countPhrase(ledger.length - ledgerWindow, 'more claim', 'more claims')
        : '',
    orphanCount: orphans.length,
    receipts: data.receipts || {},
    ledgerHead: data.ledger,
    sessions,
  }
}
