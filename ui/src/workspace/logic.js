/* ThruLine workspace — the logic module.
 *
 * The port of `scratchpad/design/trace-logic.js` (`window.THRULINE_LOGIC`),
 * plus the four things the design was missing and doctrine requires.
 *
 * PURE. No JSX, no React, no `window`, no fetch, no state. This is the file the
 * unit test walks (`./logic.test.js`), and the only place a state word, a
 * glyph, a gloss, a facet name or a rail destination is spelled.
 *
 * Ported nearly verbatim (spec-script §E7 — none of this came from `DCLogic`):
 *   KIND_LABEL SIG SEVKEY WORD WORDKEY FAM WIRE FACETS LAYOUT ICONS RAIL
 *   short() estH() pinStyle() bez() objType()
 *
 * Added here because the design does not have them and doctrine does
 * (spec-doctrine §B2, §B4, F-3, F-15, F-16, F-18):
 *   WORST_FIRST · GLOSS · normalizeWord() · projectForSessionLine()
 *   sessionWordFrom() · worstFirst() · severityOf()
 */

import { C } from './tokens.js'

/* ── Event kinds → the label a reader sees (trace-logic.js:3) ── */
export const KIND_LABEL = Object.freeze({
  session_start: 'Session started',
  user_prompt: 'Prompt',
  tool_result: 'Tool result',
  fs_change: 'File change',
  snapshot: 'Snapshot',
  tool_call: 'Tool call',
  note: 'Note',
  exec: 'Command ran',
  stop: 'Stopped',
})

/* ── Finding significance. NOT a state word (spec-doctrine F-4).
 *    `[label, colour, glyph]`. The glyphs are BRIEF:399's severity triple
 *    `▲ ● ○`, NOT the design's `○ ◆ ·` — the design reused Not proven's own
 *    glyph for a finding, which is the C20 two-words-on-one-item breach. ── */
export const SIG = Object.freeze({
  fail: Object.freeze(['why it is not proven', C.coral, '▲']),
  warn: Object.freeze(['worth knowing', C.yellow, '●']),
  info: Object.freeze(['for the record', C.inkMute, '○']),
})

/* Facet slugs for severity (trace-logic.js:5). */
export const SEVKEY = Object.freeze({ fail: 'why-not-proven', warn: 'worth-knowing', info: 'for-the-record' })

/* ── THE SEVEN STATE WORDS. `[word, colour, glyph]`, keyed by record slug.
 *
 * Two corrections to trace-logic.js:6, both binding:
 *   1. R8-VOCABULARY: the `couldnt` slug's WORD is `Recorded` (design record
 *      §5). It carries no apostrophe, so the old U+0027-not-U+2019 correction
 *      (spec-doctrine F-3, BRIEF:45) no longer has a word to apply to; the
 *      slug and its query token are unchanged, and `normalizeWord()` stays.
 *   2. Kept is `□` U+25A1 and Broken is `⊠` U+22A0 (spec-doctrine F-15,
 *      BRIEF:399). The design's `■`/`✕` are wrong, and `✕` collides with the
 *      four `×` close buttons in this same UI.
 * There is no eighth word and no word for "no word": a wordless state renders
 * NOTHING — no chip, no glyph, no dash, no "n/a" (BRIEF:1219). ── */
/* P6-6 (design record §4): each word's colour is its meaning hue — Proven and
 * Kept green, Not proven and Broken coral (an evidence gap; red is reserved for
 * destructive actions), Recorded yellow, Loose thread blue (the reader's
 * REQUEST is still unmet, and blue is request), Fog neutral (not recorded).
 * Nothing is carried by colour alone: every word keeps its word and a glyph
 * that differs from every other word's. Neutral fails 4.5:1 as body text, so
 * `wordOf().ink` gives Fog's WORD the secondary text colour and keeps neutral
 * for its glyph and outline (contrast.test.js). */
export const WORD = Object.freeze({
  not_proven: Object.freeze(['Not proven', C.coral, '○']),
  proven: Object.freeze(['Proven', C.green, '●']),
  loose: Object.freeze(['Loose thread', C.blue, '⁓']),
  fog: Object.freeze(['Fog', C.neutral, '⋯']),
  couldnt: Object.freeze(["Recorded", C.yellow, '◌']),
  kept: Object.freeze(['Kept', C.green, '□']),
  broken: Object.freeze(['Broken', C.coral, '⊠']),
})

/* The one word whose hue is not a text colour (see above). */
const INK = Object.freeze({ fog: C.inkDim })

/* Query-token slugs (trace-logic.js:7). Used by `word:` filters and saved views. */
export const WORDKEY = Object.freeze({
  not_proven: 'not-proven',
  proven: 'proven',
  loose: 'loose-thread',
  fog: 'fog',
  couldnt: 'couldnt-check',
  kept: 'kept',
  broken: 'broken',
})

/* The five words that require a `What would prove it:` clause (RSV:287-289). */
export const NEGATIVE_WORDS = Object.freeze(['broken', 'fog', 'not_proven', 'couldnt', 'loose'])

/* Worst-first ranking. ABSENT from the design entirely (spec-doctrine §B2);
 * added here from POS:60-61 / RSV:23. Index 0 is worst. */
export const WORST_FIRST = Object.freeze(['broken', 'fog', 'not_proven', 'couldnt', 'loose', 'kept', 'proven'])

/* Per-word gloss, verbatim from POS:63-68 (spec-doctrine F-18). The design
 * branched on `proven` and applied Not proven's definition to the other six. */
export const GLOSS = Object.freeze({
  proven: 'the record supports it',
  not_proven: 'claimed, and no proof found; the claim may be entirely true',
  couldnt: 'nothing in the record bears on it, and the instrument is fine',
  fog: 'the instrument failed or was never pointed here',
  loose: "the reader's request is still unmet and the decision is theirs",
  kept: 'a constraint held',
  broken: 'a constraint violated, confirmed',
})

/* The ruled empty case for the clause (RSV:309-312). Never omit the clause;
 * state that nothing would prove it. */
export const NO_CLAUSE = 'What would prove it: nothing in this record could.'

/* Node family → `[headerBg, headerInk]` (trace-logic.js:8).
 *
 * P6-6 NOTE: `FAM` and `WIRE` are the Trace's node-family palette, and the only
 * pre-§4 hexes left in the workspace's JavaScript. They are NOT re-pointed here
 * because the card forbids Trace changes: §4 replaces the filled node header
 * with "a 2 px top rule plus a kind label", which is P6-3's rebuild of the
 * canvas, not a hex swap. `visual-system.test.js` allow-lists exactly these two
 * objects so a third palette cannot grow beside them. */
export const FAM = Object.freeze({
  red: ['#6e2b2b', '#f3caca'],
  blue: ['#2c4c7c', '#cfdcf5'],
  teal: ['#1f6663', '#c8ecea'],
  violet: ['#4f3a78', '#ddd2f3'],
  green: ['#2f6b3a', '#cfe9d3'],
  ochre: ['#78561d', '#f1dfb8'],
  grey: ['#3b3e45', '#d3d6dc'],
})

/* Wire colours by family, plus the synthesised `order` wire (trace-logic.js:9). */
export const WIRE = Object.freeze({
  red: '#e07b7b',
  blue: '#7fa6e8',
  teal: '#5cc4bd',
  violet: '#b39ddb',
  green: '#6fc28a',
  ochre: '#e0b15c',
  grey: '#9a9ea6',
  order: '#e6e6e6',
})

/* The 16 facets (trace-logic.js:10). This list IS the filter grammar's
 * vocabulary; `parseToken` rejects anything not in it (spec-script §F.2). */
export const FACETS = Object.freeze([
  'word', 'object', 'event', 'claim', 'finding', 'severity', 'runner', 'check',
  'scope', 'binding', 'recorder', 'session', 'date', 'file', 'has', 'risk',
])

/* Only two aliases exist, and only these two (spec-script §F.2 rule 3). */
export const FACET_ALIAS = Object.freeze({ state: 'word', path: 'file' })

/* Hand-authored layout, one session only (trace-logic.js:11-16). Tier 1 of the
 * three-tier layout fallback; see `autoLayout()` in `./adapter.js` for tier 3.
 * Frame tuple is `[x, y, w, h, mode]`; node tuple is `[x, y, w, frameOverride?]`. */
export const LAYOUT = Object.freeze({
  'demo-narrowed': {
    frames: {
      request: [0, 0, 240, 460, ''],
      changes: [264, 0, 972, 340, ''],
      execution: [264, 370, 972, 430, 'grow'],
      claims: [1266, 0, 270, 800, 'grow'],
    },
    grow: 380,
    nodes: {
      ev4: [16, 36, 208],
      ev8: [16, 150, 208],
      ask: [16, 300, 208],
      ev13: [280, 36, 200],
      ev17: [280, 170, 200],
      f_removed: [520, 170, 220],
      f_weak: [760, 170, 220],
      tr: [1000, 36, 220],
      ev12: [1000, 36, 220],
      ev16: [1000, 136, 220],
      ev40: [1000, 236, 220],
      ev20: [280, 406, 210],
      ev32: [520, 406, 210],
      chk: [760, 406, 230],
      f_skip: [1000, 406, 220],
      snap: [280, 620, 200, 'execution'],
      ev14: [280, 620, 200, 'execution'],
      ev18: [280, 720, 200, 'execution'],
      ev28: [280, 820, 200, 'execution'],
      ev36: [280, 920, 200, 'execution'],
      ev41: [280, 1020, 200, 'execution'],
      f_narrow: [520, 620, 220],
      f_cov: [760, 620, 210],
      ev24: [1000, 620, 210],
      ev44: [1282, 36, 238],
      clm: [1282, 240, 238],
      prm: [1282, 430, 238],
      rcpt: [1282, 600, 238],
    },
  },
})

/* 15 rail glyph paths, viewBox 0 0 16 16 (trace-logic.js:17-21). */
export const ICONS = Object.freeze({
  inbox: 'M2 9h3l1 2h4l1-2h3 M2 9V4h12v5 M2 9v3h12V9',
  sessions: 'M3 3h10v2H3z M3 7h10v2H3z M3 11h10v2H3z',
  trace: 'M2.5 3.5h3v3h-3z M10.5 9.5h3v3h-3z M5.5 5h2c1 0 1.5.5 1.5 1.5v2c0 1 .5 1.5 1.5 1.5',
  claims: 'M3 3h10v10H3z M5 6h6 M5 8.5h6 M5 11h4',
  asks: 'M8 2.5a5.5 5.5 0 1 1 0 11 5.5 5.5 0 0 1 0-11z M6.3 6.5c0-1 .8-1.8 1.7-1.8s1.7.7 1.7 1.6c0 1.4-1.7 1.4-1.7 2.7 M8 11v.5',
  agreed: 'M3 8.5l3 3 7-7',
  receipts: 'M4 2h8v12l-2-1-2 1-2-1-2 1z M6 6h4 M6 9h4',
  bindings: 'M6.5 9.5a2.5 2.5 0 0 1 0-3.5l2-2a2.5 2.5 0 0 1 3.5 3.5l-1 1 M9.5 6.5a2.5 2.5 0 0 1 0 3.5l-2 2a2.5 2.5 0 0 1-3.5-3.5l1-1',
  ledger: 'M3 2h10v12H3z M5 5h6 M5 8h6 M5 11h3',
  explorer: 'M7 3a4 4 0 1 1 0 8 4 4 0 0 1 0-8z M10 10l3.5 3.5',
  compare: 'M2 3h5v10H2z M9 3h5v10H9z',
  capture: 'M8 2v3 M8 11v3 M2 8h3 M11 8h3 M8 5a3 3 0 1 1 0 6 3 3 0 0 1 0-6z',
  policy: 'M8 2l5 2v4c0 3-2 5-5 6-3-1-5-3-5-6V4z',
  handoff: 'M2 8h9 M8 5l3 3-3 3 M13 3v10',
  diag: 'M2 10h3l2-6 2 8 2-4h3',
})

/* The sidebar: 3 groups × 5 destinations (trace-logic.js:22-26), which is the
 * brief's §3.1 sidebar. Item shape `[key, label]`. */
export const RAIL = Object.freeze([
  Object.freeze({
    title: 'Review',
    key: 'review',
    items: Object.freeze([
      ['inbox', 'Inbox'],
      ['sessions', 'Sessions'],
      ['trace', 'Trace'],
      ['claims', 'Claims'],
      ['asks', 'Asks & promises'],
    ]),
  }),
  Object.freeze({
    title: 'Evidence',
    key: 'evidence',
    items: Object.freeze([
      ['agreed', 'Agreed checks'],
      ['receipts', 'Receipts'],
      ['bindings', 'Evidence bindings'],
      ['ledger', 'Audit Ledger'],
      ['explorer', 'Explorer'],
    ]),
  }),
  Object.freeze({
    title: 'Tools',
    key: 'tools',
    items: Object.freeze([
      ['compare', 'Compare sessions'],
      ['capture', 'Capture & setup'],
      ['policy', 'Policy'],
      ['handoff', 'Handoff'],
      ['diag', 'Operator diagnostics'],
    ]),
  }),
])

/**
 * Elide a hash for display, keeping any `algo:` prefix. Truncation happens HERE,
 * at render — never in the data (spec-data §B.2, §D.4: the adapter must not
 * reproduce the demo's `"sha256:…"` elisions over values the record holds).
 *
 * @param {string|null|undefined} h a hash, optionally `sha256:`-prefixed
 * @returns {string} `''` for absent input, else `prefix + first 8 chars + '…'`
 */
export function short(h) {
  if (!h) return ''
  const s = String(h)
  const i = s.indexOf(':')
  const body = i >= 0 ? s.slice(i + 1) : s
  return (i >= 0 ? s.slice(0, i + 1) : '') + body.slice(0, 8) + (body.length > 8 ? '…' : '')
}

/* Character advance in px at the card's 12px title size. A `tool_call`, `exec`
 * or `note` card renders its title in MONO (`shell.jsx`,
 * `titleFont: n.mono ? FONT.mono : 'inherit'`), whose advance is ~7.2px, not
 * the proportional ~6.6px. `estH` used 6.6 for every title and so under-budgeted
 * every mono card by ~9%; once the shortfall exceeded the 14px layout gap, a
 * card painted over the next one. Humanising changes every title length in the
 * session, which is why DESIGN-readable-trace.md §1.6.1 binds this fix to the
 * same change: shipping one without the other turns a latent overlap visible. */
export const ADVANCE_MONO = 7.2
export const ADVANCE_BODY = 6.6

/**
 * How many lines a card's title occupies. The truncation budget and this
 * function must read the SAME advance constant, or cards overlap.
 *
 * @param {{title?:string, mono?:boolean}} n @param {number} cw content width in px
 * @returns {number}
 */
export function titleLines(n, cw) {
  const adv = n && n.mono ? ADVANCE_MONO : ADVANCE_BODY
  return Math.ceil((String((n && n.title) || '').length * adv) / cw)
}

/**
 * The height authority for a node card. Height is NEVER stored on a node
 * (spec-data §A.19); every geometry read calls this.
 *
 * @param {object} n node with `pinsL[]`, `pinsR[]`, `title`, `lines[]`
 * @param {number} w node width in px
 * @returns {number} height in px
 */
export function estH(n, w) {
  const pinsL = n.pinsL || []
  const pinsR = n.pinsR || []
  const lines = n.lines || []
  const rows = Math.max(pinsL.length, pinsR.length)
  const cw = Math.max(60, w - 16)
  const tl = titleLines(n, cw)
  let ll = 0
  lines.forEach((l) => {
    ll += Math.ceil((String(l).length * 6.1) / cw)
  })
  /* F29: the card renders a clause line of its own under `lines[]` whenever it
   * carries a NEGATIVE state word (`canvas.jsx` `NodeCard`, spec-doctrine C-1).
   * This function is the height AUTHORITY — every geometry read goes through
   * it — so a line it does not budget for is a card that overflows its own
   * measured box, and every layout, fit and hit test downstream is wrong by
   * that much. The clause is `FS.sm` at 1.3 line-height with a 3px top margin. */
  const wd = !n.severity && n.wordSlug ? wordOf(n.wordSlug) : null
  const clause = wd && wd.negative ? n.clause || NO_CLAUSE : ''
  const cl = clause ? Math.ceil((String(clause).length * 5.6) / (cw - 8)) : 0
  return 30 + (rows ? rows * 20 : 4) + 8 + tl * 15 + ll * 14 + (lines.length ? 4 : 0) + (cl ? cl * 14 + 3 : 0) + 19
}

/**
 * The 9px pin square's appearance, by pin type.
 *
 * @param {{t:'order'|'ref'|'inf'|'path', c?:string}} p a pin
 * @returns {{bg:string, border:string, radius:string, clip:string}}
 */
export function pinStyle(p) {
  const c = p.t === 'order' ? WIRE.order : WIRE[p.c] || C.inkMute
  if (p.t === 'order') {
    return { bg: c, border: '0px solid transparent', radius: '0', clip: 'polygon(0 0, 55% 0, 100% 50%, 55% 100%, 0 100%)' }
  }
  if (p.t === 'inf') return { bg: 'transparent', border: '1.5px dashed ' + c, radius: '50%', clip: 'none' }
  return { bg: c, border: '1.5px solid ' + c, radius: '50%', clip: 'none' }
}

/**
 * A cubic bezier between two pin points, plus its label anchor.
 *
 * @param {number} x1 @param {number} y1 @param {number} x2 @param {number} y2
 * @returns {{d:string, mx:number, my:number}} SVG path and the label midpoint
 */
export function bez(x1, y1, x2, y2) {
  const c = Math.max(40, Math.abs(x2 - x1) * 0.45)
  return {
    d: 'M' + x1 + ' ' + y1 + ' C' + (x1 + c) + ' ' + y1 + ', ' + (x2 - c) + ' ' + y2 + ', ' + x2 + ' ' + y2,
    mx: (x1 + 3 * (x1 + c) + 3 * (x2 - c) + x2) / 8,
    my: (y1 + y2) / 2,
  }
}

/**
 * Classify a node into one of eight object types. Drives the `object:` facet,
 * the inspector's seven-way branch, and the auto-layout column assignment.
 *
 * NOTE (spec-script §G.2): the shipped version keys off hardcoded demo ids
 * (`clm`, `chk`, `prm`, `rcpt`, `ending`). The port keeps that fallback for the
 * bundled demo records but prefers an explicit `n.objType` when the adapter set
 * one, so a real session is not classified by an id it does not have.
 *
 * @param {object} n a graph node
 * @returns {'finding'|'claim'|'check'|'promise'|'ask'|'receipt'|'ending'|'event'}
 */
export function objType(n) {
  if (n.objType) return n.objType
  if (n.finding) return 'finding'
  if (n.id === 'clm') return 'claim'
  if (n.id === 'chk') return 'check'
  if (n.id === 'prm') return 'promise'
  if (n.id === 'ask' || /^ask/.test(n.id) || n.id === 'asks') return 'ask'
  if (n.id === 'rcpt') return 'receipt'
  if (n.id === 'ending') return 'ending'
  return 'event'
}

/**
 * Normalise a rendered word for comparison. U+2019 → U+0027, per BRIEF:45.
 * REQUIRED at every point a word is compared to a narration prefix, a query
 * token or a saved view — without it a narration line spelling the third word
 * with U+2019 silently fails to match and the item renders WORDLESS, which is a
 * doctrine failure, not a cosmetic one (spec-doctrine §B1).
 *
 * Do NOT sweep U+2019 anywhere else: every other curly quote in this UI is
 * ordinary English typography and is correct.
 *
 * @param {string} s any string
 * @returns {string} the same string with U+2019 replaced by U+0027
 */
export function normalizeWord(s) {
  return String(s == null ? '' : s).replace(/’/g, "'")
}

/**
 * Resolve a record slug to its display triple.
 *
 * An unknown or absent slug resolves to `{word:'', color:C.inkDim, glyph:''}` —
 * which renders as NOTHING, never as a dash, never as `no word` (BRIEF:1219,
 * spec-doctrine D-9/D-11). It must never invent a word for a state it does not
 * rule.
 *
 * @param {string|null|undefined} slug one of the seven keys of `WORD`
 * @returns {{slug:string|null, word:string, color:string, ink:string, glyph:string, negative:boolean}}
 *   `color` is the word's hue (glyph, outline, chip ground); `ink` is the colour
 *   its WORD is written in, which differs only where the hue fails 4.5:1 (Fog).
 */
export function wordOf(slug) {
  const w = slug == null ? null : WORD[slug]
  if (!w) return { slug: null, word: '', color: C.inkDim, ink: C.inkDim, glyph: '', negative: false }
  return { slug, word: w[0], color: w[1], ink: INK[slug] || w[1], glyph: w[2], negative: NEGATIVE_WORDS.indexOf(slug) >= 0 }
}

/**
 * A finding's significance. A finding carries NO state word (RSV:244-256,
 * spec-doctrine F-4) — it supplies the clause for the verdict it attaches to.
 *
 * @param {'fail'|'warn'|'info'} sev the record's severity
 * @returns {{label:string, color:string, glyph:string, key:string}}
 */
export function severityOf(sev) {
  const s = sev == null ? null : SIG[sev]
  if (!s) return { label: '', color: C.inkMute, glyph: '', key: '' }
  return { label: s[0], color: s[1], glyph: s[2], key: SEVKEY[sev] || '' }
}

/**
 * The session-line projection, applied BEFORE ranking on every summary surface
 * (RSV:25-35, BRIEF:1226): Broken → Not proven, Kept → dropped entirely.
 * Kept and Broken are constraints, never claims; letting either reach a claim
 * surface is the exact leak the amendment was written to stop.
 *
 * @param {string[]} slugs item state slugs
 * @returns {string[]} projected slugs, with `kept` removed
 */
export function projectForSessionLine(slugs) {
  const out = []
  ;(slugs || []).forEach((s) => {
    if (s === 'kept') return
    out.push(s === 'broken' ? 'not_proven' : s)
  })
  return out
}

/**
 * Rank slugs worst-first.
 *
 * Stable: equal ranks keep their input order, and a slug that is not one of the
 * seven sorts last rather than being dropped — an unknown state is a thing to
 * look at, not a thing to hide.
 *
 * @param {string[]} slugs
 * @returns {string[]} a new array ordered by `WORST_FIRST`
 */
export function worstFirst(slugs) {
  const rank = (s) => {
    const i = WORST_FIRST.indexOf(s)
    return i < 0 ? WORST_FIRST.length : i
  }
  return (slugs || [])
    .map((s, i) => ({ s, i }))
    .sort((a, b) => rank(a.s) - rank(b.s) || a.i - b.i)
    .map((e) => e.s)
}

/**
 * The word for a whole session (RSV §3.7): no claims and no items → no word;
 * clean → `proven`; mixed → the worst word among the items, AFTER
 * `projectForSessionLine`.
 *
 * OPEN QUESTION carried from spec-data §F.9, unresolved and NOT resolved here:
 * the bundled `demo-honest` record says `proven` while holding one `proven`
 * claim and one `loose` ask, which worst-first makes `loose`. Until an operator
 * rules whether a `loose` ask is an "item" for the session line, this function
 * takes the item set from its caller and does NOT special-case.
 *
 * @param {string[]} slugs the state slugs of every item summarised
 * @returns {string|null} a `WORD` key, or null when there is no word
 */
export function sessionWordFrom(slugs) {
  const projected = projectForSessionLine(slugs).filter((s) => WORD[s])
  if (!projected.length) return null
  return worstFirst(projected)[0]
}
