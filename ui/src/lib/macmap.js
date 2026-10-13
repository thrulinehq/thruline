/* State → design-token mapping for the macOS shell (RA5).
 *
 * The single place a state slug becomes a colour slot or an SVG glyph. Display
 * words come from the voice authority and nowhere else; this file adds no
 * English. CSS variable names carry engine slugs — a stylesheet identifier is
 * not a first-level string, and keeping the slug makes the token auditable
 * against macos.css. */

import { word, chipLabel } from './voice.js'

/** The design's icon set, keyed by graduated state. Paths are lifted from
 * docs/design/Throughline.html (done/pending/alert/refused) with seats added
 * for the states the design did not draw. */
export const MAC_GLYPH = {
  verified: 'M21.801 10A10 10 0 1 1 17 3.335M9 11l3 3L22 4',
  supported: 'M21.801 10A10 10 0 1 1 17 3.335M9 11l3 3L22 4',
  pending: 'M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20M12 6v6l4 2',
  contradicted: 'm21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3M12 9v4M12 17h.01',
  recorded: 'M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7ZM14 2v4a2 2 0 0 0 2 2h4',
  unknown: 'M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20M9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3M12 17h.01',
  stale: 'M3 3v5h5M3.05 13A9 9 0 1 0 6 5.3L3 8M12 7v5l4 2',
  refused: 'M4.929 4.929 19.07 19.071M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20',
}

export const sv = (s) => `var(--state-${MAC_GLYPH[s] ? s : 'unknown'})`
export const svText = (s) => `color-mix(in oklch, ${sv(s)} 72%, var(--text-1))`
export const tint = (s) => `color-mix(in oklch, ${sv(s)} 12%, var(--surface-1))`
export const bord = (s) => `color-mix(in oklch, ${sv(s)} 45%, transparent)`

/** Chip data for one state and count: the composition word, capitalized —
 * the only chip form doctrine allows. */
export const chip = (state, n) => ({
  state,
  n,
  label: chipLabel(word(state)),
  word: word(state),
  glyph: MAC_GLYPH[state] || MAC_GLYPH.unknown,
})

/** Worst first, so the eye lands on the problem (board.js order, kept). */
export const CHIP_ORDER = [
  'contradicted',
  'stale',
  'pending',
  'unknown',
  'supported',
  'verified',
  'recorded',
  'refused',
]

/** Chips for a counts object, zero counts dropped (§4). */
export function chipsFromCounts(counts) {
  const out = []
  for (const s of CHIP_ORDER) {
    const n = counts?.[s] || 0
    if (n > 0) out.push(chip(s, n))
  }
  return out
}

/** Index-row chips (already word-keyed by the export) mapped to slugs. */
export function chipsFromRow(row, slugForWord) {
  return (row?.chips || []).map((c) => {
    const state = slugForWord[c.word] || 'unknown'
    return { ...chip(state, c.count), label: c.label || chipLabel(c.word) }
  })
}
