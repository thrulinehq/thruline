import React from 'react'
import { countPhrase } from '../lib/voice.js'
import { C, FONT, FS, HOVER, LS, RADIUS, SIZE, sx } from './tokens.js'
import { GLOSS, ICONS, NO_CLAUSE, severityOf, wordOf } from './logic.js'
import { READING_LINE, WAITING_STEPS, nothingBackYet, stillReading } from './state.js'

/* ThruLine workspace — the shared leaf helpers.
 *
 * Nine components, and every porter uses them instead of hand-rolling the same
 * markup. Three of them exist to make a doctrine rule impossible to forget:
 *
 *   <StateWord>  a state word cannot render without its clause
 *   <Count>      a count cannot render without a noun, and never at zero
 *   <Refusal>    a surface with no data says so, loudly, and names what is missing
 *
 * Three more keep the four facts four — a read in flight, a read that found
 * nothing, and a read that failed are not one screen:
 *
 *   <Waiting>    a read is in flight, and it names the address it is reading
 *   <Skel>       the SHAPE of what is coming, with no digit, word or row in it
 *   <Absence>    the read succeeded and the record holds nothing — NOT red
 *
 * The rest are pure de-duplication: <Icon> over the 15 ICONS paths and the
 * two chrome glyphs, <Significance> over the three finding severities, and
 * <Kicker> over the 19 uppercase section captions.
 *
 * Only <Waiting> holds state, and only a timer over its own two escalations.
 */

/* Frozen style constants, merged with `sx` (tokens.js style convention). */

const ICON_STYLE = Object.freeze({ strokeLinecap: 'round', strokeLinejoin: 'round', flex: '0 0 auto' })

/* `minWidth: 0` and `overflow: hidden` so this head can be CLIPPED at all.
 * Without them it is an unshrinkable `nowrap` box, and in the left dock's tree
 * leaf it painted its whole intrinsic width across the label beside it
 * (`docks.jsx` TREE_GLYPH). Exported for `lib/shell.test.js`, which asserts the
 * style contract in the absence of a DOM harness. */
export const WORD_HEAD = Object.freeze({ display: 'inline-flex', alignItems: 'baseline', gap: '6px', whiteSpace: 'nowrap', minWidth: 0, overflow: 'hidden' })
const WORD_GLYPH = Object.freeze({ fontSize: FS.title, lineHeight: 1 })
const WORD_TEXT = Object.freeze({ fontSize: FS.lg, fontWeight: 600 })
const WORD_GLOSS = Object.freeze({ fontSize: FS.body, fontWeight: 400, color: C.inkDim, whiteSpace: 'normal' })
const WORD_CHIP = Object.freeze({
  fontSize: FS.meta,
  color: C.bgApp,
  borderRadius: RADIUS.pill,
  padding: '1px 5px',
  fontWeight: 600,
  whiteSpace: 'nowrap',
})
/* BRIEF:1562 — upright, never italic, indented one step. Italic belongs to
 * authored/inferred text and to nothing else (spec-doctrine C-12). */
const CLAUSE = Object.freeze({
  fontStyle: 'normal',
  fontSize: FS.body,
  lineHeight: 1.5,
  color: C.inkSoft,
  paddingLeft: '18px',
  whiteSpace: 'normal',
  textWrap: 'pretty',
})
const WORD_STACK_INLINE = Object.freeze({ display: 'inline-flex', flexDirection: 'column', gap: '3px', minWidth: 0 })
const WORD_STACK_ROW = Object.freeze({ display: 'flex', flexDirection: 'column', gap: '3px', minWidth: 0 })

const SIG_STYLE = Object.freeze({ display: 'inline-flex', alignItems: 'baseline', gap: '5px', fontSize: FS.sm, whiteSpace: 'nowrap' })

const KICKER = Object.freeze({
  fontSize: FS.cap,
  fontWeight: 600,
  textTransform: 'uppercase',
  color: C.inkDim,
})

const REFUSAL = Object.freeze({
  padding: '24px',
  background: C.bgApp,
  color: C.danger,
  font: `12px/1.6 ${FONT.mono}`,
  display: 'flex',
  flexDirection: 'column',
  gap: '10px',
  alignItems: 'flex-start',
  minWidth: 0,
  overflow: 'auto',
})
const REFUSAL_TITLE = Object.freeze({ fontSize: FS.title, fontWeight: 600, color: C.danger, textWrap: 'pretty' })
const REFUSAL_MISSING = Object.freeze({ color: C.inkBright, fontFamily: FONT.mono, fontSize: FS.body, wordBreak: 'break-word' })
const REFUSAL_NOTE = Object.freeze({ color: C.inkMute, fontFamily: FONT.sans, fontSize: FS.body })
const REFUSAL_PRE = Object.freeze({
  margin: 0,
  padding: '8px 10px',
  background: C.canvas,
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.ctl,
  color: C.inkCode,
  fontFamily: FONT.mono,
  fontSize: FS.sm,
  whiteSpace: 'pre-wrap',
  wordBreak: 'break-word',
  maxWidth: '100%',
  userSelect: 'text',
})

/**
 * A stroked 16×16 glyph. The design draws every icon as a single `<path>` at
 * `stroke-width 1.4`, round caps, no fill (spec-template §A:380).
 *
 * @param {object} props
 * @param {string} [props.name] key into `ICONS` — one of the 15 rail destinations.
 *   Ignored when `d` is given.
 * @param {string} [props.d] a raw path, for the two chrome glyphs that are not
 *   in `ICONS` (the search magnifier at :357 and the settings gear at :366)
 * @param {number} [props.size=16] px, square
 * @param {string} [props.viewBox='0 0 16 16'] the magnifier and gear also use
 *   `0 0 16 16`; the minimap passes a dynamic box and does not use this component
 * @param {number} [props.strokeWidth=1.4] 1.5 for the magnifier
 * @param {string} [props.color='currentColor'] stroke colour
 * @returns {JSX.Element|null} null when neither `name` nor `d` resolves — a
 *   missing icon renders nothing rather than borrowing a neighbour's glyph
 */
export function Icon({ name, d, size = 16, viewBox = '0 0 16 16', strokeWidth = 1.4, color = 'currentColor' }) {
  const path = d || (name ? ICONS[name] : null)
  if (!path) return null
  return (
    <svg
      viewBox={viewBox}
      width={size}
      height={size}
      fill="none"
      stroke={color}
      aria-hidden="true"
      focusable="false"
      style={sx(ICON_STYLE, { strokeWidth: `${strokeWidth}px` })}
    >
      <path d={path} />
    </svg>
  )
}

/**
 * THE SEVEN-WORD RENDERER. The single component allowed to put a state word on
 * screen. Glyph + word + colour + the `What would prove it:` clause.
 *
 * Doctrine this component enforces so no caller has to remember it:
 *  - A wordless state renders NOTHING — no chip, no glyph, no dash, no `n/a`,
 *    no `no word` (BRIEF:1219; spec-doctrine D-9, D-10, D-11).
 *  - Every negative word (`broken fog not_proven couldnt loose`) carries the
 *    clause (RSV:287-289). The design breaks this on eleven surfaces
 *    (spec-doctrine §C, C-1…C-11); the fix is one required prop, not eleven
 *    patches.
 *  - `clause` absent-as-a-record-fact (`null` or `''`) renders the ruled empty
 *    case `NO_CLAUSE`, because the empty case is STATED, never omitted.
 *  - `clause` **undefined** (the prop was not passed at all) THROWS. That
 *    distinguishes "the record has nothing" from "the porter forgot", and Rule
 *    12 says the second must be loud. The boot Boundary in `main.jsx` catches it.
 *  - The clause is upright in `--ink-2`, indented one step, and NEVER italic —
 *    italic is reserved for authored/inferred text (BRIEF:1562, spec-doctrine C-12).
 *  - No item carries two words. A finding has a `severity`, not a word: render
 *    it with `<Significance>`, never here (spec-doctrine F-4).
 *
 * VARIANTS AND WHERE THE CLAUSE LANDS:
 *  - `inline` and `row` render the clause themselves, under the word.
 *  - `chip` renders the solid node-header pill ALONE — a header is one 30px
 *    flex row and `estH` budgets no clause height there. The caller must still
 *    hold the clause (the prop is required and `undefined` still throws) and
 *    must render it on the same card: `canvas.jsx`'s `NodeCard` carries it as
 *    its own line under `lines[]` (spec-doctrine F-2, canvas.jsx:75-77).
 *
 * @param {object} props
 * @param {string|null|undefined} props.slug a `WORD` key: `proven not_proven
 *   couldnt loose fog kept broken`. Anything else renders nothing.
 * @param {string|null} props.clause REQUIRED whenever `slug` is negative. The
 *   record's own `What would prove it: …` sentence, verbatim — never
 *   re-authored, never paraphrased (spec-doctrine F-2). `null`/`''` → `NO_CLAUSE`.
 * @param {'chip'|'inline'|'row'} [props.variant='inline'] `chip` is the solid
 *   pill in a node header (ink `C.bgApp` on the word's colour — or, for a hue
 *   that fails 4.5:1 behind that ink, a dashed outline — radius 2px,
 *   10px/600 — spec-template :542); `inline` is glyph + word in the word's
 *   colour; `row` stacks the clause under the word as its own line.
 * @param {boolean} [props.gloss=false] append ` — ` + `GLOSS[slug]` (POS:63-68).
 *   Used by the inspector's State row only (spec-doctrine F-18).
 * @param {boolean} [props.clauseHidden=false] the ONE sanctioned way to omit
 *   the clause from this element: the caller guarantees the clause is one tap
 *   behind, because the element IS a control that opens the item (UX:17). Used
 *   only by the app-bar session chip (spec-template :355, spec-doctrine C-7),
 *   which must then carry the accessible name `{word} — open the session summary`.
 * @param {object} [props.style] merged last, via `sx`
 * @returns {JSX.Element|null}
 */
export function StateWord({ slug, clause, variant = 'inline', gloss = false, clauseHidden = false, style }) {
  const w = wordOf(slug)
  /* A wordless state renders nothing at all — not a dash, not "no word". */
  if (!w.word) return null

  /* Rule 12: "the porter forgot" must be loud, and is not the same fact as
   * "the record holds nothing to name". */
  if (w.negative && !clauseHidden && clause === undefined) {
    throw new Error(
      `StateWord: "${w.word}" is a negative state word and needs its clause. ` +
        'Pass the record\'s own "What would prove it: …" sentence, or null when the record has none.',
    )
  }

  if (variant === 'chip') {
    /* A hue that fails 4.5:1 behind dark ink (Fog's neutral, 3.99:1) is not a
       chip ground: it is drawn as §4 draws "not recorded" — a dashed outline in
       the hue, the word in its text-safe ink. */
    const skin = w.ink === w.color ? { background: w.color } : { background: 'transparent', color: w.ink, border: `1px dashed ${w.color}` }
    return <span style={sx(WORD_CHIP, skin, style)} data-word={w.slug}>{w.word}</span>
  }

  const head = (
    <span style={sx(WORD_HEAD, { color: w.color }, variant === 'inline' ? style : null)} data-word={w.slug}>
      <span style={WORD_GLYPH} aria-hidden="true">{w.glyph}</span>
      <span style={w.ink === w.color ? WORD_TEXT : sx(WORD_TEXT, { color: w.ink })}>
        {w.word}
        {gloss && GLOSS[w.slug] ? <span style={WORD_GLOSS}>{` — ${GLOSS[w.slug]}`}</span> : null}
      </span>
    </span>
  )

  const clauseEl =
    w.negative && !clauseHidden ? <span style={CLAUSE}>{clause == null || clause === '' ? NO_CLAUSE : clause}</span> : null

  if (!clauseEl) return variant === 'row' ? <div style={sx(WORD_STACK_ROW, style)}>{head}</div> : head

  return variant === 'row' ? (
    <div style={sx(WORD_STACK_ROW, style)}>
      {head}
      {clauseEl}
    </div>
  ) : (
    <span style={WORD_STACK_INLINE}>
      {head}
      {clauseEl}
    </span>
  )
}

/**
 * A finding's significance — `why it is not proven` · `worth knowing` · `for the
 * record` (BRIEF:1677). Deliberately a DIFFERENT component from `StateWord`,
 * with different glyphs (`▲ ● ○`) and no clause, because a finding carries no
 * state word and putting one in the word slot is the C20 two-words-on-one-item
 * breach the design commits at dc.html:1111-1112 (spec-doctrine F-4).
 *
 * @param {object} props
 * @param {'fail'|'warn'|'info'} props.severity the record's severity
 * @param {object} [props.style]
 * @returns {JSX.Element|null} null for an unknown severity
 */
export function Significance({ severity, style }) {
  const s = severityOf(severity)
  if (!s.label) return null
  return (
    <span style={sx(SIG_STYLE, { color: s.color }, style)}>
      <span aria-hidden="true">{s.glyph}</span>
      <span>{s.label}</span>
    </span>
  )
}

/**
 * THE COUNT HELPER. Renders nothing at zero.
 *
 * Doctrine: every count carries a noun; zero-count chips do not render; no
 * `NONE`, no `none open`, no dash, no `0 lines` (UX:20, BRIEF:1564, C16;
 * spec-doctrine §D — the design breaches this thirteen times, all of them in
 * chips and table cells).
 *
 * Delegates the singular/plural choice to `countPhrase` in `lib/voice.js`,
 * which is the shipped voice authority and is unit-tested. Do not reimplement it.
 *
 * @param {object} props
 * @param {number|string|null|undefined} props.n the count. A string is accepted
 *   because `countPhrase` accepts a locale-formatted number ("1,094")
 * @param {string} props.singular e.g. `'finding'`, `'thing worth a look'`
 * @param {string} props.plural e.g. `'findings'`, `'things worth a look'`
 * @param {object} [props.style]
 * @param {string} [props.title] tooltip; only set when it says something the
 *   phrase does not
 * @returns {JSX.Element|null} **null when `n` is 0, null or undefined** — the
 *   caller must therefore tolerate an absent child, and a tab or section whose
 *   only content is a count must itself not render its count container
 * @example
 *   <Count n={findings.length} singular="finding" plural="findings" />
 */
export function Count({ n, singular, plural, style, title }) {
  if (n == null) return null
  const digits = String(n).replace(/[^0-9]/g, '')
  if (!digits || Number(digits) === 0) return null
  return (
    <span style={style} title={title}>
      {countPhrase(n, singular, plural)}
    </span>
  )
}

/**
 * The loud refusal. The design has NO error, loading, empty or unknown-shape
 * surface anywhere (spec-template §I.7) and the port must add one: a screen
 * that draws its calm layout over data it could not read is the exact lie this
 * engine exists to catch (`main.jsx:7-13`, Rule 12).
 *
 * Rendered instead of — never beside — the surface it replaces. In particular
 * the Trace canvas MUST render this rather than an empty graph, because an
 * empty graph is indistinguishable from a graph reporting nothing
 * (spec-data §E.2 rule 6).
 *
 * Nothing here executes anything: the remedy is text a reader may select and
 * run themselves (doctrine 8).
 *
 * @param {object} props
 * @param {string} props.title what could not be shown, in the reader's words,
 *   e.g. `'This session has no trace to draw.'`
 * @param {string} props.missing the named absence — a document, an endpoint, a
 *   shape. Never a guess, never a generic "something went wrong".
 *   e.g. `'thruline.explain was not returned by /api/explain'`
 * @param {string} [props.remedy] a COPYABLE command the reader may run
 *   themselves. Nothing on this screen executes it (doctrine 8).
 * @param {string} [props.detail] the raw error text, already bounded by the caller
 * @returns {JSX.Element}
 */
export function Refusal({ title, missing, remedy, detail }) {
  return (
    <div role="alert" style={REFUSAL}>
      <div style={REFUSAL_TITLE}>{title}</div>
      <div>
        <Kicker style={{ marginBottom: '4px' }}>What is missing</Kicker>
        <div style={REFUSAL_MISSING}>{missing}</div>
      </div>
      {remedy ? (
        <div>
          <Kicker style={{ marginBottom: '4px' }}>You can run this yourself</Kicker>
          <pre style={REFUSAL_PRE}>{remedy}</pre>
          <div style={sx(REFUSAL_NOTE, { marginTop: '4px' })}>Nothing on this screen runs it.</div>
        </div>
      ) : null}
      {detail ? (
        <div>
          <Kicker style={{ marginBottom: '4px' }}>What the read returned</Kicker>
          <pre style={REFUSAL_PRE}>{detail}</pre>
        </div>
      ) : null}
    </div>
  )
}

/**
 * An uppercase section caption — the design's 19 `.06em` letter-spaced captions
 * (spec-template §E.2), e.g. `Filters`, `Saved views`, `Next useful check`.
 *
 * @param {object} props
 * @param {React.ReactNode} props.children the caption text
 * @param {string} [props.spacing='.06em'] `.1em` for the rail group caption,
 *   `.08em` for a frame title or the `authored` kicker, `.05em` for `Filters`
 * @param {object} [props.style]
 * @returns {JSX.Element}
 */
export function Kicker({ children, spacing = LS.cap, style }) {
  return <div style={sx(KICKER, { letterSpacing: spacing }, style)}>{children}</div>
}

/* ── The three in-flight / honest-empty surfaces ─────────────────────────── */

const WAITING = Object.freeze({
  display: 'flex',
  flexDirection: 'column',
  gap: '6px',
  alignItems: 'flex-start',
  padding: '18px 24px',
  minWidth: 0,
})
const WAITING_LEAD = Object.freeze({ font: `400 12px/1.6 ${FONT.mono}`, color: C.inkMute })
const WAITING_MORE = Object.freeze({ font: `400 12px/1.6 ${FONT.sans}`, color: C.inkDim, maxWidth: '52ch', textWrap: 'pretty' })

/* The skeleton's three widths, so a block of bars does not read as a table.
 * Three is the CAP, and it is a shape, never a count: a list is coming, not
 * three things (spec-matrix §0). */
const SKEL_WIDTHS = Object.freeze(['62%', '84%', '47%'])
const SKEL_WRAP = Object.freeze({ display: 'flex', flexDirection: 'column', gap: '6px', padding: '8px 10px', minWidth: 0 })
const SKEL_BAR = Object.freeze({ background: C.line2, opacity: 0.5, borderRadius: RADIUS.pill, flex: '0 0 auto' })

const ABSENCE = Object.freeze({
  padding: '24px',
  background: C.canvas,
  color: C.inkDim,
  display: 'flex',
  flexDirection: 'column',
  gap: '10px',
  alignItems: 'flex-start',
  minWidth: 0,
  overflow: 'auto',
})
const ABSENCE_TITLE = Object.freeze({
  font: `600 14px/1.45 ${FONT.sans}`,
  color: C.inkSoft,
  textWrap: 'pretty',
  maxWidth: '54ch',
})
const ABSENCE_BODY = Object.freeze({ font: `400 12px/1.6 ${FONT.sans}`, color: C.inkDim, maxWidth: '54ch', textWrap: 'pretty' })
/* T-6-6: the one fill control. Its colours are `.ws-btn-primary`'s (P6-6); this
 * sets only its size and place, so no new colour pair is introduced here. */
const ABSENCE_ACTION = Object.freeze({ marginTop: '8px', padding: '6px 12px', borderRadius: '6px', font: `600 13px/1.4 ${FONT.sans}` })
const ABSENCE_CENTRE = Object.freeze({
  position: 'absolute',
  inset: 0,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  pointerEvents: 'none',
})
/* T-6-6: never taller than the canvas it sits on. An emptiness that says what
 * the place is, why it is empty and what fills it is longer than the one-line
 * refusal this box was sized for, and a centred box taller than its canvas was
 * cut off at both ends (seen in Chrome at 1440×900). It scrolls instead. */
const ABSENCE_CENTRE_INNER = Object.freeze({ pointerEvents: 'auto', maxWidth: '620px', maxHeight: '100%', overflowY: 'auto' })

/**
 * A READ IS IN FLIGHT. Never a refusal, never an export remedy, never a
 * percentage, an ETA or an elapsed count (`refresh-presentation.test.js:75`
 * forbids the last three in progress copy and this inherits the ban).
 *
 * It escalates twice, and both escalations add a FACT rather than a reassurance:
 * at 2.5s it names the address being read, at 10s it says the page has not given
 * up and will speak if the read fails. Nothing here claims to know how long the
 * read will take, because nothing here measures it.
 *
 * One `role="status" aria-live="polite"` region — the same shape
 * `refresh.jsx:104-122` uses — so a reader who cannot see the bars is told the
 * truth the bars cannot tell them.
 *
 * @param {object} props
 * @param {string} props.endpoint the address actually being read, e.g.
 *   `/api/explain`. REQUIRED: a waiting line that cannot say what it is waiting
 *   for is the affordance this app was missing, not a smaller version of it.
 * @param {object} [props.style] merged last
 * @returns {JSX.Element}
 */
export function Waiting({ endpoint, style }) {
  const [step, setStep] = React.useState(0)
  React.useEffect(() => {
    const a = setTimeout(() => setStep((s) => (s < 1 ? 1 : s)), WAITING_STEPS.named)
    const b = setTimeout(() => setStep((s) => (s < 2 ? 2 : s)), WAITING_STEPS.patient)
    return () => {
      clearTimeout(a)
      clearTimeout(b)
    }
  }, [endpoint])
  return (
    <div role="status" aria-live="polite" aria-atomic="true" style={sx(WAITING, style)}>
      <span style={WAITING_LEAD}>{READING_LINE}</span>
      {step >= 1 ? <span style={WAITING_MORE}>{stillReading(endpoint)}</span> : null}
      {step >= 2 ? <span style={WAITING_MORE}>{nothingBackYet(endpoint)}</span> : null}
    </div>
  )
}

/**
 * THE SHAPE OF WHAT IS COMING — and nothing else.
 *
 * No digit, no state word, no glyph, no hash, no identifier, no row count that
 * could be read as data. At most three inert bars, at the pane's own row height,
 * on the pane's own width. `aria-hidden`, because it says nothing true to a
 * screen reader; the `<Waiting>` beside it does.
 *
 * The pulse is declared in `workspace.css` under
 * `@media (prefers-reduced-motion: no-preference)`, and the sheet's existing
 * reduce block kills every animation under `.ws-root` a second time. A reader
 * who has asked for no motion gets three still bars.
 *
 * @param {object} props
 * @param {number} [props.n=3] bars, clamped to 1..3
 * @param {number} [props.rowH=12] the pane's own row height in px
 * @param {object} [props.style] merged last onto the wrapper
 * @returns {JSX.Element}
 */
export function Skel({ n = 3, rowH = 12, style }) {
  const count = Math.max(1, Math.min(3, Math.round(Number(n) || 1)))
  const h = Math.max(6, Math.min(14, Math.round(Number(rowH) || 12) - 14)) || 8
  const bars = []
  for (let i = 0; i < count; i += 1) bars.push(SKEL_WIDTHS[i % SKEL_WIDTHS.length])
  return (
    <div aria-hidden="true" style={sx(SKEL_WRAP, style)}>
      {bars.map((w, i) => (
        <span key={`skel${i}`} className="ws-skel" style={sx(SKEL_BAR, { width: w, height: `${h}px` })} />
      ))}
    </div>
  )
}

/**
 * THE READ SUCCEEDED AND THE RECORD HOLDS NOTHING.
 *
 * The honest-empty counterpart of `<Refusal>`, and deliberately NOT red: an
 * emptiness is not a fault, and painting it in `C.danger` is the inverse lie —
 * it accuses the record of a failure it did not have. `C.inkDim` on `C.canvas`.
 *
 * ABSENT vs EMPTY is the rule this pair exists to keep: an ABSENT record — no
 * array at all, nothing adapted, a shape refused upstream — renders `<Refusal>`;
 * an EMPTY record — the reading happened, the array has length zero — renders
 * this (`canvas.jsx:22-32`).
 *
 * Nothing here executes anything: the remedy is text a reader may select and run
 * themselves (doctrine 8), under the same note `<Refusal>` uses.
 *
 * @param {object} props
 * @param {string} props.title what is not here, in the reader's words, as a full
 *   sentence with a verb and a stop.
 * @param {string} props.absent the named absent noun. REQUIRED, and `undefined`
 *   THROWS, for the same reason `<StateWord>`'s clause does: a blank rectangle
 *   is the failure this component was added to end, and "the porter forgot" must
 *   be loud (Rule 12). Pass a string — never a guess, never "nothing here".
 * @param {string} [props.change] what would change it, ending where the remedy
 *   begins.
 * @param {string} [props.remedy] a COPYABLE command. Nothing on this screen runs it.
 * @param {string} [props.what] T-6-6: what this place is, in one sentence,
 *   before why it is empty (`HQ_COPY.empty.<state>.what`, rule 12).
 * @param {{label: string, onClick: Function}} [props.action] T-6-6: the ONE
 *   control that would fill it, when this screen has one (the Trace's is the
 *   session list). Drawn primary; it runs nothing, it opens a list.
 * @param {string} [props.remedyLead] T-6-6: when an action sits between the
 *   change sentence and the remedy, the words that lead into the remedy, so
 *   the sentence that ends in "run" is followed by the command it names.
 * @param {object} [props.style]
 * @returns {JSX.Element}
 */
export function Absence({ title, absent, change, remedy, what, action, remedyLead, style }) {
  if (absent === undefined) {
    throw new Error(
      'Absence: an emptiness must name what is absent. ' +
        'Pass the absent noun — the thing the record does not hold — never a blank and never a generic phrase.',
    )
  }
  const notHere = (
    <div>
      <Kicker style={{ marginBottom: '4px' }}>What is not here</Kicker>
      <div style={ABSENCE_BODY}>{absent}</div>
    </div>
  )
  /* T-6-6: when this emptiness teaches (`what` given), its three parts and its
   * one control come first — what, why, what fills it, the control — and the
   * exact list of what the read did not hold comes after them, so a short
   * canvas shows the part a person acts on without scrolling. */
  const teaching = !!what
  return (
    <div style={sx(ABSENCE, style)} data-empty={teaching ? 'absence' : undefined}>
      {teaching ? <div style={ABSENCE_BODY}>{what}</div> : null}
      <div style={ABSENCE_TITLE}>{title}</div>
      {teaching ? null : notHere}
      {change ? (
        <div>
          <div style={ABSENCE_BODY}>{change}</div>
          {action ? (
            <button type="button" className="ws-btn-primary" data-primary-action="" style={ABSENCE_ACTION} onClick={action.onClick}>
              {action.label}
            </button>
          ) : null}
          {teaching ? <div style={{ marginTop: '10px' }}>{notHere}</div> : null}
          {remedy ? (
            <React.Fragment>
              {remedyLead ? <div style={sx(ABSENCE_BODY, { marginTop: '10px' })}>{remedyLead}</div> : null}
              <pre style={sx(REFUSAL_PRE, { marginTop: '6px' })}>{remedy}</pre>
              <div style={sx(REFUSAL_NOTE, { marginTop: '4px' })}>Nothing on this screen runs it.</div>
            </React.Fragment>
          ) : null}
        </div>
      ) : teaching ? notHere : null}
    </div>
  )
}

const NOTICE = Object.freeze({
  display: 'flex',
  alignItems: 'flex-start',
  gap: '10px',
  padding: '8px 12px',
  background: C.surf2,
  borderBottom: `1px solid ${C.line1}`,
  color: C.ink,
  fontSize: FS.body,
  lineHeight: 1.45,
})
const NOTICE_BODY = Object.freeze({ flex: 1, display: 'flex', flexDirection: 'column', gap: '3px' })
const NOTICE_TITLE = Object.freeze({ fontWeight: 600 })
const NOTICE_LINE = Object.freeze({ color: C.inkSoft })
const NOTICE_CMD = Object.freeze({ margin: 0, font: `${FS.ctl} ${FONT.mono}`, color: C.inkCode })
const NOTICE_CLOSE = Object.freeze({
  height: SIZE.ctl,
  padding: '0 8px',
  background: 'transparent',
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.ctl,
  color: C.inkMute,
  fontSize: FS.ctl,
  cursor: 'pointer',
})

/**
 * One thing this page owes the reader about the ADDRESS it was opened at — a
 * `?session=` naming a session the list does not contain, today, and nothing
 * else.
 *
 * It is deliberately NOT the centre refusal (`state.centreState`): the reading
 * that did open is fine, and painting a failure over a working session would be
 * a second wrong statement about the same fact. It is deliberately not a queue
 * either — the Inbox is the app's ONE attention queue (doctrine 6) — so this
 * carries exactly one notice, is dismissible, and never accumulates.
 *
 * It speaks the shipped `<Absence>` vocabulary in a strip: what happened, what
 * is not here, what would change it, and a command a human runs.
 *
 * @param {object} props
 * @param {{title:string, absent:string, change:string, remedy:string}|null} props.notice
 * @param {() => void} props.onDismiss
 * @returns {JSX.Element|null}
 */
export function AddressNotice({ notice, onDismiss }) {
  if (!notice) return null
  return (
    <div role="status" style={NOTICE}>
      <div style={NOTICE_BODY}>
        <span style={NOTICE_TITLE}>{notice.title}</span>
        <span style={NOTICE_LINE}>{notice.absent}</span>
        {notice.change ? <span style={NOTICE_LINE}>{notice.change}</span> : null}
        {notice.remedy ? <pre style={NOTICE_CMD}>{notice.remedy}</pre> : null}
        {notice.remedy ? <span style={NOTICE_LINE}>Nothing on this screen runs it.</span> : null}
      </div>
      <button type="button" className="ws-hv" onClick={onDismiss} style={sx(NOTICE_CLOSE, HOVER.ghost())}>
        Dismiss
      </button>
    </div>
  )
}

/**
 * Centres one block over a surface that keeps painting behind it — the canvas
 * grid, which is how a reader recognises they are still on Trace while it waits
 * or while it states an emptiness.
 *
 * @param {object} props
 * @param {React.ReactNode} props.children an `<Absence>` or a `<Waiting>`
 * @returns {JSX.Element}
 */
export function Centred({ children }) {
  return (
    <div style={ABSENCE_CENTRE}>
      <div style={ABSENCE_CENTRE_INNER}>{children}</div>
    </div>
  )
}
