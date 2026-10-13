/* Presentation vocabulary: how a node state is drawn. Not seeded data —
   these are design-system mappings, carried over from the prototype.

   Every user-visible STRING here now comes from ./voice — the single
   presentation authority (§5v), generated from crates/core/src/voice.rs. What
   stays local is genuinely visual: glyph paths, colours, display order. */

import { chipLabel, sentence, word } from './voice.js'

export const GLYPH = {
  // A logbook line, not a tick: `recorded` asserts that something happened,
  // not that it was correct.
  recorded: 'M4 7h16M4 12h16M4 17h10',
  supported: 'M21.801 10A10 10 0 1 1 17 3.335M9 11l3 3L22 4',
  // `unsupported` is a gap in OUR evidence, not a charge against the user, so
  // it gets a dashed-circle "nothing here" mark rather than anything alarming.
  unsupported: 'M12 2a10 10 0 0 1 7.07 2.93M22 12a10 10 0 0 1-2.93 7.07M12 22A10 10 0 0 1 4.93 19.07M2 12a10 10 0 0 1 2.93-7.07',
  pending: 'M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20M12 6v6l4 2',
  contradicted: 'm21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3M12 9v4M12 17h.01',
  dropped: 'M9 10h.01M15 10h.01M12 2a8 8 0 0 0-8 8v12l3-3 2.5 2.5L12 19l2.5 2.5L17 19l3 3V10a8 8 0 0 0-8-8',
  refused: 'M4.929 4.929 19.07 19.071M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20',
  unknown: 'M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20M9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3M12 17h.01',
}

/* §5v: chip and badge labels come from the presentation authority, never from
 * the engine enum. These were `Contradicted` / `Unsupported` / `Refused` —
 * state names on screen, which is the exact leak the banned-lexicon test now
 * fails on. The words are UX-DOCTRINE's composition words, capitalized. */
/** The node states this shell draws. `stale` and `verified` are receipt-level
 *  dimensions, not node states, so they are not chips here. */
export const STATES = [
  'recorded', 'supported', 'pending', 'contradicted',
  'unsupported', 'dropped', 'refused', 'unknown',
]

export const SLABEL = Object.fromEntries(
  STATES.map((s) => [s, chipLabel(word(s))]),
)

/* First-level receipt lines. Previously engine paraphrase ("Supported by
 * evidence", "No captured proof found"); now the doctrine's own sentences.
 * The engine's reasoning is not deleted — it moved one disclosure deeper. */
export const RECEIPT_STATE = Object.fromEntries(
  STATES.map((s) => [s, sentence(s)]),
)

/** Display order for chips, filters and swatches. Attention-first.
 *
 *  Kept in the M3.9 vocabulary. This list had drifted to the pre-M3.9 names
 *  (`unverified` / `unclassified` / `done`), which silently suppressed every
 *  chip for the states that replaced them — the counts were computed and then
 *  filtered out by a list that had never heard of them. */
export const ORDER = [
  'contradicted', 'dropped', 'unsupported', 'unknown', 'pending', 'recorded',
  'supported', 'refused',
]

/** The chip row is an ATTENTION filter, so the bulk states stay out of it —
 *  M0 excluded `done` for exactly this reason and `recorded` inherits it.
 *  "All" already covers them. */
export const BULK_STATES = new Set(['recorded', 'supported'])

/** UX-DOCTRINE §4, "no naked numbers": a zero-count chip is chrome that tells
 *  the user nothing. Every state now renders only when non-zero — the rule is
 *  universal rather than a per-state exception list, which is how `Refused 0`
 *  survived in the first place. */
export const rendersWhenZero = () => false

export const sv = (s) => `var(--state-${s})`
export const svText = (s) => `color-mix(in oklch, var(--state-${s}) 72%, var(--text-1))`
export const tint = (s) => `var(--state-${s}-tint)`
export const bord = (s) => `var(--state-${s}-border)`

export const fmt = (n) => n.toLocaleString('en-US')

/* ── fixture-driven string composition ────────────────────────────────
   The prototype hardcoded these sentences; §9 stores the parts, so the
   UI composes them. Same output, no seeded strings. */

/* C0.2: "UNVERIFIED" is dead vocabulary — it hid damage, sweeps, and
 * never-checked behind one accusatory bucket. The boolean here is the §9
 * compatibility field, so the honest false-branch is the weaker claim. */
export const ledgerLine = (l) =>
  `Event #${fmt(l.event_no)} of ${fmt(l.total)} · chain ${l.verified ? 'verified ✓' : 'not confirmed'}`

/* The first-level verdict line is the doctrine sentence for the state. The
 * engine's own wording and its score are second-level: `verdictDetail` is what
 * the receipt shows one tap behind. UX-DOCTRINE paper cut #4 — the "Verdict"
 * block used to print engine reasoning verbatim at first level, and the score
 * arrived as a naked number besides. */
export const verdictLine = (state) => RECEIPT_STATE[state] || RECEIPT_STATE.unknown

export const verdictDetail = (v) =>
  v.score == null ? v.text : `${v.text} · ${v.score.toFixed(2)}`

export const sessionLine = (s) => `${s.label} · ${s.date} · ${s.msgs} msgs`

export const claimMeta = (session, r) =>
  `${session ? session.label : r.claim.session} · message ${r.claim.message_no} · ${r.claim.ts}`
