import React from 'react'
import { C, FONT, FS, GRID, HOVER, LS, RADIUS, SIZE, Z, outlineButton, sx } from './tokens.js'
import { Absence, Count, Kicker, Refusal, Significance, Skel, StateWord, Waiting } from './ui-kit.jsx'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'

/* ThruLine workspace — the three docks and the resize separator.
 *
 * Design range: spec-template §A lines 392-425 (left dock), 687-767 (bottom
 * dock), 771-832 (inspector). Each dock is preceded or followed by a 5px
 * `role="separator"` with `tabindex=0` that resizes by drag AND by arrow key —
 * 16px per press, same clamps as the drag (`CLAMP.left/right/bottom`).
 *
 * Sizes are state, persisted under the one localStorage key
 * `thruline.trace.prefs.v1` along with `rep`, `panels.{bottom,minimap}`,
 * `groupBy`, `columns` and `savedViews`. `panels.left`/`right`, `filters`,
 * `session` and `sel` are deliberately NOT persisted (spec-script §C).
 *
 * WHAT THIS FILE CHANGED, AND WHY (each is a contract §7 correction, not taste):
 *  · Every `role="row"` here now sits inside a real `role="grid"` with
 *    `aria-rowcount`, and every cell carries `role="columnheader" className="ws-head-cell"` or
 *    `role="gridcell"`. The design ships bare `role="row"` with no table at all
 *    (spec-template Decision D5), which a screen reader cannot navigate. The
 *    tablists gained `role="tabpanel"` bodies with `aria-controls`/
 *    `aria-labelledby` for the same reason — a tablist with no panel is broken.
 *  · A dock tab's count goes through `<Count>`, so a zero count renders NO
 *    count element. The tab itself stays: it is navigation (spec-doctrine F-6).
 *  · The dock caption carries the `authored` kicker and the left rule, or does
 *    not render. The design puts authored prose in the dock's own voice, with
 *    no attribution at all — the worst placement in it (spec-doctrine E-1).
 *  · The inspector's authored block moved BELOW `As the CLI narrates it`, so
 *    authored text never sits between two record sentences (F-17b).
 *  · The primary inspector action's `title` reads `Show me the proof`; the
 *    design's bare `title="p"` read as an error (spec-template Decision D7), and
 *    since P6-1 `P` was the §6 key for the third altitude (Workbench, keys
 *    1 / 2 / 3 since A3-WORKBENCH), so the key hint is gone with it.
 *  · A pane whose rows are ABSENT (not empty — absent) renders `<Refusal>`
 *    instead of the pane, naming what was missing (contract rule 5).
 */

/* ── Shared style constants. Frozen, module-level, merged with `sx` — never an
 *    object literal in JSX, so a sticky header and its rows cannot drift. ── */

const ELLIPSIS = Object.freeze({ overflow: 'hidden', whiteSpace: 'nowrap', textOverflow: 'ellipsis' })

const DOCK_HEAD = Object.freeze({
  height: SIZE.paneHeader,
  display: 'flex',
  alignItems: 'center',
  gap: '6px',
  padding: '0 8px 0 10px',
  background: C.lineRow, // the dock header ground (spec-template :394, :774)
  borderBottom: `1px solid ${C.lineStrong}`,
  fontSize: FS.cap,
  fontWeight: 600,
  letterSpacing: LS.cap,
  textTransform: 'uppercase',
  color: C.inkMute,
})

const HIDE_BTN = Object.freeze({
  background: 'transparent',
  border: 0,
  color: C.inkDim,
  cursor: 'pointer',
  fontSize: FS.body,
  padding: '0 2px',
})

const TABLIST = Object.freeze({
  height: SIZE.paneHeader,
  display: 'flex',
  alignItems: 'stretch',
  background: C.surf2,
  borderBottom: `1px solid ${C.lineStrong}`,
  padding: '0 6px',
})

/* R9: LONG-HAND ONLY. `border: 0` beside `borderBottom` and a `TAB_ON` that
 * overrode `borderBottomColor` made React log a style-shorthand error on every
 * tab switch ("Updating border borderBottom … can lead to styling bugs"): the
 * two objects disagreed about which property owned the bottom edge. Widths
 * name the edge, one colour paints it, and the two objects never touch the
 * same property from different levels. */
const TAB = Object.freeze({
  padding: '0 11px',
  background: 'transparent',
  borderWidth: '0 0 2px 0',
  borderStyle: 'solid',
  borderColor: 'transparent',
  color: C.inkMute,
  fontSize: FS.ctl,
  fontWeight: 500,
  cursor: 'pointer',
  display: 'inline-flex',
  alignItems: 'center',
  gap: '6px',
})
const TAB_ON = Object.freeze({ borderColor: C.amber, color: C.inkBright })
const TAB_COUNT = Object.freeze({ font: `${FS.meta} ${FONT.mono}`, color: C.inkDim })

/* N13-FOCUS-2411: the findings and events headers stick here; focus is kept below them. */
const PANE_BODY = Object.freeze({ flex: 1, overflow: 'auto', minHeight: 0, scrollPaddingTop: SIZE.statusStrip })

/* R4: every header cell truncates rather than escaping its box. `GRID.findings`
 * gives one column `minmax(…,1fr)`, and in a narrow dock that column reached
 * zero width while its label kept painting — over the next header and over the
 * first data row. The class lives in `workspace.css` because a child selector
 * cannot be written as an inline style. */
const STICKY_HEAD = Object.freeze({
  display: 'grid',
  gap: '10px',
  padding: '0 12px',
  height: SIZE.statusStrip,
  alignItems: 'center',
  fontSize: FS.cap,
  letterSpacing: LS.cap,
  textTransform: 'uppercase',
  color: C.inkDim,
  borderBottom: `1px solid ${C.surf3}`,
  position: 'sticky',
  top: 0,
  background: C.surf1,
})
/* The checks header is the one that does NOT stick (design :725). */
const PLAIN_HEAD = Object.freeze({
  display: 'grid',
  gap: '10px',
  padding: '0 12px',
  height: SIZE.statusStrip,
  alignItems: 'center',
  fontSize: FS.cap,
  letterSpacing: LS.cap,
  textTransform: 'uppercase',
  color: C.inkDim,
  borderBottom: `1px solid ${C.surf3}`,
})

const ROW = Object.freeze({
  display: 'grid',
  gap: '10px',
  padding: '0 12px',
  alignItems: 'center',
  borderLeft: '2px solid transparent',
  cursor: 'pointer',
  fontSize: FS.body,
})

const NOTE = Object.freeze({ padding: '8px 12px', fontSize: FS.ctl, color: C.inkMute })

const MONO_META = Object.freeze({ font: `${FS.cap} ${FONT.mono}`, color: C.inkMute })

const GRID_ROLE = Object.freeze({ display: 'block' })

/**
 * Enter/Space activate a row, exactly as the design's synthesised `onKey` does
 * (spec-script §B.12 `row()`), and nothing else is swallowed.
 *
 * @param {(id: string) => void} act what a press means
 * @param {string} id the row's own id
 * @returns {(e: React.KeyboardEvent) => void}
 */
function pressKey(act, id) {
  return (e) => {
    if (e.key === 'Enter' || e.key === ' ' || e.key === 'Spacebar') {
      e.preventDefault()
      if (act && id != null) act(id)
    }
  }
}

/**
 * A key/value row of the inspector's Details / Evidence / History tabs
 * (spec-template :786, :799, :803 — one grid, `GRID.inspectorKv`).
 *
 * @param {object} props
 * @param {{k:string, v:string, color?:string, font?:string, fontSize?:string}} props.r
 *   the row built by the root's `R(k, v, o)`; an absent value already carries
 *   its own honest gap string and its own `#8b8f97`, so this component never
 *   substitutes a placeholder of its own
 * @returns {JSX.Element}
 */
function KvRow({ r }) {
  return (
    <div style={KV_ROW}>
      <span style={KV_K}>{r.k}</span>
      <span style={sx(KV_V, { color: r.color || C.ink, fontFamily: r.font || 'inherit', fontSize: r.fontSize || FS.body })}>
        {r.v}
      </span>
    </div>
  )
}

const KV_ROW = Object.freeze({
  display: 'grid',
  gridTemplateColumns: GRID.inspectorKv,
  gap: '8px',
  padding: '5px 0',
  borderBottom: `1px solid ${C.lineRow}`,
  fontSize: FS.body,
})
const KV_K = Object.freeze({ color: C.inkDim })
const KV_V = Object.freeze({ wordBreak: 'break-word', lineHeight: 1.4 })

/**
 * The `authored` block — italic, left-ruled, with the kicker that names it as
 * authored (spec-template :625, :779). It is the ONLY italic text in a dock;
 * the clause above it stays upright, because italic is reserved for authored
 * and inferred text (spec-doctrine C-12).
 *
 * @param {object} props
 * @param {string} props.text the authored sentence, verbatim
 * @param {string} [props.kicker='authored']
 * @param {object} [props.style]
 * @returns {JSX.Element|null} null when there is nothing authored to show
 */
function Authored({ text, kicker = 'authored', style }) {
  if (!text) return null
  return (
    <div style={sx(AUTHORED, style)}>
      <span style={AUTHORED_KICKER}>{kicker}</span>
      {text}
    </div>
  )
}

const AUTHORED = Object.freeze({
  marginTop: '8px',
  fontSize: FS.ctl,
  fontStyle: 'italic',
  color: C.inkMute,
  borderLeft: `2px solid ${C.line2}`,
  paddingLeft: '8px',
  lineHeight: 1.4,
})
const AUTHORED_KICKER = Object.freeze({
  fontStyle: 'normal',
  fontSize: FS.micro,
  letterSpacing: LS.frame,
  textTransform: 'uppercase',
  color: C.inkFaint,
  marginRight: '6px',
})

/* ── The separator ──────────────────────────────────────────────────────── */

const SPLIT_V = Object.freeze({ width: SIZE.splitter, flex: `0 0 ${SIZE.splitter}px`, position: 'relative' })
const SPLIT_H = Object.freeze({ height: SIZE.splitter, flex: `0 0 ${SIZE.splitter}px`, position: 'relative' })
const SPLIT_OFF = Object.freeze({ display: 'none' })
const SPLIT_BAR_V = Object.freeze({ width: '100%', height: '100%', cursor: 'col-resize', background: C.bgApp })
const SPLIT_BAR_H = Object.freeze({ width: '100%', height: '100%', cursor: 'row-resize', background: C.bgApp })
/* N13-DRAG-257: the two step buttons sit across the middle of the separator. */
const SPLIT_STEPS = Object.freeze({
  position: 'absolute',
  left: '50%',
  top: '50%',
  transform: 'translate(-50%, -50%)',
  display: 'flex',
  gap: '2px',
  zIndex: Z.dock,
})
const SPLIT_STEP = Object.freeze({
  width: SIZE.ctl,
  height: SIZE.ctl,
  padding: 0,
  font: 'inherit',
  fontSize: FS.body,
  lineHeight: 1,
  color: C.inkDim,
  background: C.surf1,
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.ctl,
  cursor: 'pointer',
})
const SPLIT_STEP_OFF = Object.freeze({ cursor: 'default' })
/* Which arrow key each step button stands for, and its name, per dock. The
 * first button is the one on the left (or top) of the separator. */
const SPLIT_STEP_KEYS = Object.freeze({
  left: [['ArrowLeft', 'left_narrower', '\u2212'], ['ArrowRight', 'left_wider', '+']],
  right: [['ArrowLeft', 'right_wider', '+'], ['ArrowRight', 'right_narrower', '\u2212']],
  bottom: [['ArrowUp', 'bottom_taller', '+'], ['ArrowDown', 'bottom_shorter', '\u2212']],
})

/**
 * A resize separator (spec-template :424, :688, :772).
 *
 * @param {object} props
 * @param {'left'|'right'|'bottom'} props.which
 * @param {'horizontal'|'vertical'} props.orientation `vertical` for left/right
 * @param {string} props.label e.g. `'Resize left dock (arrow keys)'` — verbatim (§F.4)
 * @param {boolean} props.visible drives `display`; a docked panel in narrow
 *   mode hides its separator
 * @param {number} [props.value] the dock's size now, px — a focusable separator
 *   states its value (N13-F11-AXE-SR, axe aria-required-attr)
 * @param {number} [props.min] @param {number} [props.max] the drag's own clamp
 *   (the bottom dock's is lower in a short window: R6's cap)
 * @param {boolean} [props.capped] the top is R6's short-window cap, not the
 *   drag's own clamp — it decides which reason an off "taller" step gives
 * @param {(e: MouseEvent) => void} props.onDown
 * @param {(e: KeyboardEvent) => void} props.onKey 16px per arrow press; the
 *   two step buttons (N13-DRAG-257, WCAG 2.2 SC 2.5.7) call it with the arrow
 *   key they stand for, so a click and a key press resize by the same step
 *   within the same clamp. A step that cannot move the separator any
 *   further is off and says why and what turns it back on (A3-F10): an
 *   enabled button that changes nothing is a dead control.
 * @returns {JSX.Element|null}
 */
export function Splitter({ which, orientation, label, visible = true, value, min, max, capped = false, onDown, onKey }) {
  const R = HQ_COPY.a11y.resize
  const vertical = which !== 'bottom'
  const num = (n) => typeof n === 'number' && Number.isFinite(n)
  return (
    <div style={sx(vertical ? SPLIT_V : SPLIT_H, !visible && SPLIT_OFF)}>
      <div
        role="separator"
        aria-orientation={orientation}
        aria-label={label}
        aria-valuenow={value}
        aria-valuemin={min}
        aria-valuemax={max}
        tabIndex={0}
        className="ws-hv"
        onMouseDown={onDown}
        onKeyDown={onKey}
        style={sx(vertical ? SPLIT_BAR_V : SPLIT_BAR_H, HOVER.separator())}
      />
      <div style={sx(SPLIT_STEPS, { flexDirection: vertical ? 'column' : 'row' })}>
        {(SPLIT_STEP_KEYS[which] || []).map(([key, name, sign]) => {
          const grows = sign === '+'
          const stuck = num(value) && (grows ? num(max) && value >= max : num(min) && value <= min)
          const why = stuck ? (grows && capped ? R.short_window : R.at_limit) : null
          const whyId = `ws-split-${which}-${name}-why`
          return (
            <React.Fragment key={key}>
              <button
                type="button"
                className="ws-hv"
                aria-label={R[name]}
                title={why ? `${why.reason} ${why.enabler}` : R[name]}
                disabled={stuck}
                aria-describedby={why ? whyId : undefined}
                onClick={() => onKey && onKey({ key, preventDefault() {} })}
                style={sx(SPLIT_STEP, stuck && SPLIT_STEP_OFF)}
              >
                <span aria-hidden="true">{sign}</span>
              </button>
              {why ? <span id={whyId} hidden>{why.reason} {why.enabler}</span> : null}
            </React.Fragment>
          )
        })}
      </div>
    </div>
  )
}

/* ── The left dock ──────────────────────────────────────────────────────── */

/* The design writes `left/top/bottom/z-index` unconditionally and neutralises
 * them with `position: static` when the viewport is wide (`ui.leftPosition`).
 * Here they are applied only when the dock is actually positioned, so a caller
 * passing `relative` cannot shift the dock 48px sideways. */
const LEFT_OVER = Object.freeze({ left: SIZE.rail, top: 0, bottom: 0, zIndex: Z.dock })
const LEFT_ASIDE = Object.freeze({
  minWidth: SIZE.leftMin,
  flex: '0 0 auto',
  background: C.surf1,
  borderRight: `1px solid ${C.line1}`,
  display: 'flex',
  flexDirection: 'column',
  minHeight: 0,
})
const LEFT_FILTER_WRAP = Object.freeze({ padding: '6px 8px', borderBottom: `1px solid ${C.surf3}` })
const LEFT_FILTER = Object.freeze({
  width: '100%',
  boxSizing: 'border-box',
  height: SIZE.ctl,
  background: C.fieldBg,
  border: `1px solid ${C.lineField}`,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  padding: '0 7px',
  fontSize: FS.body,
})
const LEFT_LIST = Object.freeze({ flex: 1, overflow: 'auto', padding: '4px 0' })
const SESSION_LINE = Object.freeze({
  display: 'flex',
  alignItems: 'flex-start',
  flexWrap: 'wrap',
  gap: '6px',
  padding: '4px 10px 6px',
  fontSize: FS.body,
  color: C.inkBright,
})
const SESSION_ID = Object.freeze({ fontFamily: FONT.mono, fontSize: FS.ctl })
const SESSION_COUNT = Object.freeze({ color: C.inkDim, fontSize: FS.sm })
/* THE TWO COLLISIONS THE SCREENSHOTS SHOW, and the discipline this codebase
 * already wrote down for another surface. `workspace.css:96-108`
 * `.ws-head-cell` exists because "a grid column with a `1fr` track can be
 * squeezed to nothing in a narrow dock. When that happened the header label
 * kept painting at its natural width, on top of the header beside it and the
 * first row beneath it." These four objects are the same bug in flexbox.
 *
 * `TREE_TITLE` was `{flex: 1}` alone: `flex: 1` sets the BASIS to 0% but keeps
 * `min-width: auto` — the min-content width of "Commands", about 60px — and
 * with no `overflow: hidden` that floor is hard and the item refuses to shrink.
 * All the shrinking therefore fell on `TREE_COUNT`, which had neither
 * `flexShrink: 0` nor `nowrap`, so `1391 command events` wrapped to three lines
 * inside a 28px `alignItems: center` row and painted above and below the button
 * box, over the rows either side. `TREE_GROUP` set no `overflow`, so nothing
 * clipped it.
 *
 * Exported because `lib/shell.test.js` asserts them: there is no DOM harness in
 * this project, and the next person to write `flex: 1` here should have a test
 * tell them rather than a screenshot. */
export const TREE_GROUP = Object.freeze({
  width: '100%',
  display: 'flex',
  alignItems: 'center',
  gap: '6px',
  padding: '0 10px',
  background: 'transparent',
  border: 0,
  color: C.ink,
  fontSize: FS.body,
  cursor: 'pointer',
  textAlign: 'left',
  overflow: 'hidden',
  minWidth: 0,
})
const TREE_CARET = Object.freeze({ color: C.inkDim, fontSize: FS.micro, width: '8px', flexShrink: 0 })
export const TREE_TITLE = Object.freeze({ flex: 1, fontWeight: 500, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })
export const TREE_COUNT = Object.freeze({ fontSize: FS.sm, color: C.inkDim, fontFamily: FONT.mono, flexShrink: 0, whiteSpace: 'nowrap' })
const TREE_LEAF = Object.freeze({
  width: '100%',
  display: 'flex',
  alignItems: 'center',
  gap: '6px',
  padding: '0 8px 0 24px',
  border: 0,
  borderLeft: '2px solid transparent',
  color: C.ink,
  fontSize: FS.body,
  cursor: 'pointer',
  textAlign: 'left',
})
/* "Recorded" painted over "task complete". `TREE_GLYPH` holds a
 * `<StateWord variant="inline">`, which renders the GLYPH AND THE WHOLE WORD
 * (`ui-kit.jsx:202-210`) inside a `whiteSpace: 'nowrap'` head — an intrinsic
 * width of about 110px in a slot the author sized at 10px. `minWidth: '10px'`
 * REPLACED the flex item's default `min-width: auto`, which is the very floor
 * that normally prevents this, and `TREE_LABEL`'s `flex: 1` basis of 0% meant
 * it could not give back space it had never claimed. All the deficit landed on
 * the glyph slot, which compressed to 10px while a hundred pixels of the state
 * word kept painting across the label beside it.
 *
 * The state word now owns its intrinsic width up to a cap and the label yields,
 * because the label was already ellipsis-capable and the word is not
 * abbreviable: doctrine rule 1 puts the WORD on screen, and a lone glyph is not
 * the word. (That was the rejected alternative: a fourth `StateWord` variant
 * rendering `◌` alone would fit, and would silently remove the state word from
 * a list of claims.) */
export const TREE_GLYPH = Object.freeze({ fontSize: FS.sm, textAlign: 'center', flex: '0 0 auto', maxWidth: '52%', minWidth: 0, overflow: 'hidden' })
export const TREE_LABEL = Object.freeze({ flex: 1, minWidth: 0, overflow: 'hidden', whiteSpace: 'nowrap', textOverflow: 'ellipsis' })
export const TREE_SUB = Object.freeze({
  fontSize: FS.cap,
  color: C.inkDim,
  fontFamily: FONT.mono,
  whiteSpace: 'nowrap',
  flexShrink: 0,
  maxWidth: '38%',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
})
const SAVED_WRAP = Object.freeze({
  borderTop: `1px solid ${C.line1}`,
  maxHeight: SIZE.savedViewsMaxH,
  display: 'flex',
  flexDirection: 'column',
  minHeight: 0,
})
const SAVED_HEAD = Object.freeze({
  height: SIZE.subHeader,
  display: 'flex',
  alignItems: 'center',
  padding: '0 10px',
  background: C.surf2,
})
const SAVED_LIST = Object.freeze({ overflow: 'auto', padding: '3px 0' })
const SAVED_ROW = Object.freeze({
  width: '100%',
  display: 'flex',
  alignItems: 'center',
  gap: '6px',
  padding: '0 10px 0 12px',
  border: 0,
  color: C.ink,
  fontSize: FS.body,
  cursor: 'pointer',
  textAlign: 'left',
})
const SAVED_GLYPH = Object.freeze({ color: C.inkDim, fontSize: FS.meta })
const SAVED_NAME = Object.freeze({ flex: 1, overflow: 'hidden', whiteSpace: 'nowrap', textOverflow: 'ellipsis' })
const SAVED_COUNT = Object.freeze({ fontSize: FS.cap, color: C.inkDim, whiteSpace: 'nowrap' })

/**
 * The left dock — `Session objects` (spec-template :392-423).
 *
 * Header · object filter input · the object tree (10 groups, each collapsible,
 * each leaf a 2px-accented row) · Saved views (max-height 38%).
 *
 * Every leaf row shows the object's glyph in its word colour. The design
 * renders the word with no clause and no path to one (spec-doctrine C-2); here
 * the glyph is a `<StateWord variant="inline" clauseHidden>` ONLY because the
 * row is a control that selects the object and puts the clause in the inspector
 * one click away (UX:17) — and the row's `title` carries the clause so it is
 * reachable without the click.
 *
 * A saved view whose tokens include ANY unsupported token reports
 * `none in this record` rather than silently applying its valid half
 * (spec-script §F.3).
 *
 * @param {object} props
 * @param {number} props.width px @param {string} props.position `absolute` when
 *   narrow, `static` when wide — the design's own two values; the 48px offset
 *   and the z-index apply only in the positioned case @param {string} props.shadow
 * @param {string} props.sessionLabel @param {string|null} props.sessionWord
 * @param {string|null} props.sessionClause the session word's own clause. This
 *   line is not a control, so the clause-free exemption does not reach it and
 *   the clause must arrive here (DOC-08); `null` states the ruled empty case.
 * @param {string} props.treeCountLine a phrase with a noun, e.g. `28 objects`
 * @param {string} props.treeSearch
 * @param {Array<{key:string,title:string,count:string|null,open:boolean,caret?:string,items:Array<{nodeId:string,label:string,sub:string,title:string,clause?:string|null,wordSlug?:string|null,sel:boolean,bg?:string,edge?:string,font?:string,fontSize?:string}>}>} props.groups
 *   `count` is a rendered phrase or null; zero renders no count element
 * @param {Array<{name:string,query:string,count:string,active:boolean,bg?:string}>} props.savedViews
 * @param {number} props.rowH
 * @param {(v: string) => void} props.onTreeSearch
 * @param {(groupKey: string) => void} props.onToggleGroup
 * @param {(nodeId: string) => void} props.onSelect
 * @param {(viewName: string) => void} props.onApplyView
 * @param {() => void} props.onHide
 * @param {string|null} [props.reading] the address being read, when a read is in
 *   flight: the session line and the tree become inert bars, the filter field is
 *   disabled (there is nothing to filter yet) and the waiting line says what is
 *   being read. The count line renders NOTHING — `0 objects` while reading is a
 *   fabricated count.
 * @param {object|null} [props.absence] `<Absence>` props for a record that was
 *   read and holds no objects to list
 * @returns {JSX.Element|null}
 */
export function LeftDock({
  width,
  position,
  shadow,
  sessionLabel,
  sessionWord,
  sessionClause,
  treeCountLine,
  treeSearch,
  groups,
  savedViews,
  rowH,
  reading,
  absence,
  onTreeSearch,
  onToggleGroup,
  onSelect,
  onApplyView,
  onHide,
}) {
  const tree = groups || []
  const views = savedViews || []
  /* A filter over nothing is a control that cannot do what it offers. */
  const filterOff = !!reading || (!!absence && !tree.length)
  return (
    <aside
      aria-label="Session objects"
      style={sx(LEFT_ASIDE, position === 'absolute' && LEFT_OVER, { position, width, boxShadow: shadow })}
    >
      <div style={DOCK_HEAD}>
        <span style={{ flex: 1 }}>Session objects</span>
        <button
          type="button"
          className="ws-hv"
          onClick={onHide}
          aria-label="Hide left dock"
          style={sx(HIDE_BTN, HOVER.ghost())}
        >
          ×
        </button>
      </div>

      <div style={LEFT_FILTER_WRAP}>
        <input
          value={treeSearch}
          onChange={(e) => onTreeSearch(e.target.value)}
          placeholder="Filter objects…"
          aria-label="Filter objects"
          disabled={filterOff}
          style={LEFT_FILTER}
        />
      </div>

      {reading ? (
        <div style={LEFT_LIST} aria-busy="true">
          <Skel n={1} rowH={rowH} />
          <Skel n={3} rowH={rowH} />
          <Waiting endpoint={reading} />
        </div>
      ) : absence && !tree.length ? (
        <div style={LEFT_LIST}>
          <Absence {...absence} />
        </div>
      ) : (
      <div style={LEFT_LIST}>
        <div style={SESSION_LINE}>
          {/* DOC-08: this line is NOT a control, and `clauseHidden` is
              sanctioned only where the element IS a control that opens the item
              (ui-kit.jsx). It used to render the word clause-free anyway. The
              clause now arrives as a prop — `sessionClause`, the record's own
              `What would prove it:` sentence — and `<StateWord variant="row">`
              stacks it under the word. `null` renders the ruled empty case,
              which is an honest statement about the record, not an invention. */}
          <StateWord slug={sessionWord} clause={sessionClause === undefined ? null : sessionClause} variant="row" />
          <span style={SESSION_ID}>{sessionLabel}</span>
          <span style={SESSION_COUNT}>{treeCountLine}</span>
        </div>

        {tree.map((g) => (
          <div key={g.key}>
            <button
              type="button"
              className="ws-hv"
              onClick={() => onToggleGroup(g.key)}
              aria-expanded={!!g.open}
              style={sx(TREE_GROUP, { height: rowH }, HOVER.treeTab())}
            >
              <span style={TREE_CARET}>{g.caret || (g.open ? '▾' : '▸')}</span>
              <span style={TREE_TITLE}>{g.title}</span>
              {g.count ? <span style={TREE_COUNT}>{g.count}</span> : null}
            </button>
            {g.open
              ? (g.items || []).map((it) => (
                  <button
                    key={it.nodeId}
                    type="button"
                    className="ws-hv"
                    onClick={() => onSelect(it.nodeId)}
                    aria-pressed={!!it.sel}
                    title={[it.title, it.clause].filter(Boolean).join(' — ')}
                    style={sx(
                      TREE_LEAF,
                      { height: rowH, background: it.bg || 'transparent', borderLeftColor: it.edge || 'transparent' },
                      HOVER.listItem(),
                    )}
                  >
                    <span style={TREE_GLYPH}>
                      <StateWord slug={it.wordSlug} clause={it.clause === undefined ? null : it.clause} variant="inline" clauseHidden />
                    </span>
                    <span style={sx(TREE_LABEL, { fontFamily: it.font || 'inherit', fontSize: it.fontSize || FS.body })}>
                      {it.label}
                    </span>
                    <span style={TREE_SUB}>{it.sub}</span>
                  </button>
                ))
              : null}
          </div>
        ))}
      </div>
      )}

      <div style={SAVED_WRAP}>
        <div style={SAVED_HEAD}>
          <Kicker>Saved views</Kicker>
        </div>
        <div style={SAVED_LIST}>
          {views.map((v) => (
            <button
              key={v.name}
              type="button"
              className="ws-hv"
              onClick={() => onApplyView(v.name)}
              aria-pressed={!!v.active}
              title={v.query}
              style={sx(SAVED_ROW, { height: rowH, background: v.bg || 'transparent' }, HOVER.listItem())}
            >
              <span style={SAVED_GLYPH} aria-hidden="true">◈</span>
              <span style={SAVED_NAME}>{v.name}</span>
              <span style={SAVED_COUNT}>{v.count}</span>
            </button>
          ))}
        </div>
      </div>
    </aside>
  )
}

/* ── The inspector ──────────────────────────────────────────────────────── */

const INSP_TABS = Object.freeze([
  Object.freeze({ key: 'details', label: 'Details' }),
  Object.freeze({ key: 'evidence', label: 'Evidence' }),
  Object.freeze({ key: 'history', label: 'History' }),
  Object.freeze({ key: 'raw', label: 'Raw' }),
])

const RIGHT_OVER = Object.freeze({ right: 0, top: 0, bottom: 0, zIndex: Z.dock })
const RIGHT_ASIDE = Object.freeze({
  minWidth: SIZE.rightMin,
  flex: '0 0 auto',
  background: C.surf1,
  borderLeft: `1px solid ${C.line1}`,
  display: 'flex',
  flexDirection: 'column',
  minHeight: 0,
})
const INSP_TYPE = Object.freeze({
  fontWeight: 400,
  textTransform: 'none',
  letterSpacing: 0,
  color: C.inkDim,
  fontFamily: FONT.mono,
  fontSize: FS.cap,
  overflow: 'hidden',
  whiteSpace: 'nowrap',
  textOverflow: 'ellipsis',
  flex: 1,
})
const INSP_SUMMARY = Object.freeze({ padding: '10px 12px 8px', borderBottom: `1px solid ${C.surf3}` })
const INSP_WORD_ROW = Object.freeze({ display: 'flex', alignItems: 'center', gap: '7px', marginBottom: '6px' })
const INSP_ID = Object.freeze({
  font: `${FS.cap} ${FONT.mono}`,
  color: C.inkDim,
  overflow: 'hidden',
  whiteSpace: 'nowrap',
  textOverflow: 'ellipsis',
  maxWidth: SIZE.inspectorIdMax,
})
const INSP_MOVE = Object.freeze({ display: 'flex', alignItems: 'center', gap: '4px', marginTop: '6px' })
const INSP_MOVE_LABEL = Object.freeze({ fontSize: FS.sm, color: C.inkDim, marginRight: '4px' })
const INSP_MOVE_BTN = Object.freeze({
  width: SIZE.ctl,
  height: SIZE.ctl,
  padding: 0,
  font: 'inherit',
  color: C.ink,
  background: 'transparent',
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.ctl,
  cursor: 'pointer',
})
const INSP_TITLE = Object.freeze({
  fontSize: FS.title,
  fontWeight: 500,
  color: C.inkBright,
  lineHeight: 1.35,
  textWrap: 'pretty',
})
const INSP_SENTENCE = Object.freeze({ fontSize: FS.base, lineHeight: 1.5, marginTop: '6px', textWrap: 'pretty' })
const INTEGRITY_ROW = Object.freeze({
  display: 'flex',
  alignItems: 'baseline',
  gap: '6px',
  marginTop: '6px',
  padding: '3px 0 0',
  borderTop: `1px solid ${C.lineRow}`,
  fontSize: FS.ctl,
  color: C.inkSoft,
})
const INTEGRITY_LABEL = Object.freeze({
  fontSize: FS.micro,
  letterSpacing: LS.frame,
  textTransform: 'uppercase',
  color: C.inkFaint,
})
const INSP_PANE = Object.freeze({ flex: 1, overflow: 'auto', minHeight: 0, padding: '6px 12px 10px' })
const CAP_MARGIN = Object.freeze({ display: 'block', margin: '10px 0 4px' })
/* A3-NOTES-NEVER-OPEN: the folded notes under the Inspector's findings. */
const NOTES_BOX = Object.freeze({ marginTop: 10 })
const NOTES_SUMMARY = Object.freeze({ cursor: 'pointer', fontSize: FS.sm, color: C.inkMute })
const NOTES_ABOUT = Object.freeze({ fontSize: FS.sm, color: C.inkMute, margin: '4px 0 6px' })
const FINDING_BTN = Object.freeze({
  width: '100%',
  display: 'flex',
  alignItems: 'center',
  gap: '8px',
  padding: '5px 7px',
  background: C.surfInset,
  border: `1px solid ${C.line1}`,
  borderRadius: RADIUS.ctl,
  marginBottom: '3px',
  color: C.ink,
  fontSize: FS.body,
  cursor: 'pointer',
  textAlign: 'left',
})
const FINDING_NAME = Object.freeze({ flex: 1, fontWeight: 500 })
const NEXT_ROW = Object.freeze({ display: 'flex', alignItems: 'center', gap: '6px' })
const NEXT_CODE = Object.freeze({
  flex: 1,
  font: `${FS.ctl} ${FONT.mono}`,
  color: C.inkBright,
  background: C.bgApp,
  border: `1px solid ${C.line1}`,
  borderRadius: RADIUS.ctl,
  padding: '4px 7px',
  overflow: 'hidden',
  whiteSpace: 'nowrap',
  textOverflow: 'ellipsis',
})
const BTN = Object.freeze({
  height: SIZE.ctl,
  padding: '0 8px',
  background: C.surf3,
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  fontSize: FS.sm,
  cursor: 'pointer',
})
const ACTION_BTN = Object.freeze({
  height: SIZE.ctl,
  padding: '0 8px',
  background: C.surf3,
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  fontSize: FS.ctl,
  cursor: 'pointer',
})
/* The inspector's lead action. It shares the view with the toolbar's one filled
 * amber button, so it is the amber OUTLINE (§4: one filled amber per view). */
const ACTION_PRIMARY = Object.freeze(outlineButton(C.amber))
const NARR_LINE = Object.freeze({ fontSize: FS.body, lineHeight: 1.5, color: C.ink })
const NARR_ABSENT = Object.freeze({ marginTop: '6px', fontSize: FS.ctl, lineHeight: 1.45, color: C.inkDim })
const CLOSING = Object.freeze({ marginTop: '8px', fontSize: FS.ctl, color: C.inkDim, lineHeight: 1.45 })
const RAW_PRE = Object.freeze({
  margin: 0,
  font: `${FS.sm}/1.5 ${FONT.mono}`,
  color: C.inkCode,
  background: C.bgApp,
  border: `1px solid ${C.line1}`,
  borderRadius: RADIUS.ctl,
  padding: '8px',
  whiteSpace: 'pre-wrap',
  wordBreak: 'break-word',
})
const ACTION_BAR = Object.freeze({
  borderTop: `1px solid ${C.line1}`,
  padding: '6px 8px',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '4px',
  background: C.chrome,
})
const CMD_WRAP = Object.freeze({ borderTop: `1px solid ${C.line1}`, background: C.chrome })
const CMD_TOGGLE = Object.freeze({
  width: '100%',
  height: SIZE.subHeader,
  display: 'flex',
  alignItems: 'center',
  gap: '8px',
  padding: '0 10px',
  background: 'transparent',
  border: 0,
  color: C.ink,
  fontSize: FS.ctl,
  cursor: 'pointer',
  textAlign: 'left',
})
const CMD_CARET = Object.freeze({ color: C.inkDim, fontSize: FS.micro })
const CMD_CAVEAT = Object.freeze({ fontSize: FS.cap, color: C.amber })
const CMD_LIST = Object.freeze({ padding: '2px 10px 8px', maxHeight: SIZE.commandListMaxH, overflow: 'auto' })
const CMD_ITEM = Object.freeze({ padding: '5px 0', borderTop: `1px solid ${C.lineRow}` })
const CMD_ITEM_HEAD = Object.freeze({ display: 'flex', alignItems: 'center', gap: '6px' })
const CMD_LABEL = Object.freeze({ flex: 1, fontSize: FS.ctl, color: C.ink })
const CMD_MISSING = Object.freeze({ fontSize: FS.meta, color: C.amber })
const CMD_COPY = Object.freeze({
  height: SIZE.ctlSm,
  padding: '0 8px',
  background: C.surf3,
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  fontSize: FS.sm,
  cursor: 'pointer',
})
const CMD_CODE = Object.freeze({
  display: 'block',
  marginTop: '3px',
  font: `${FS.sm}/1.45 ${FONT.mono}`,
  color: C.inkSoft,
  background: C.bgApp,
  border: `1px solid ${C.line1}`,
  borderRadius: RADIUS.ctl,
  padding: '4px 7px',
  whiteSpace: 'pre-wrap',
  wordBreak: 'break-all',
})

/**
 * The right inspector (spec-template :771-831) — the app's proof surface.
 *
 * Header · summary (word + clause + title + sentences + the authored block) ·
 * four tabs (Details / Evidence / History / Raw) · the action bar · the command
 * drawer.
 *
 * The inspector view-model is built by the root's `inspector(selId)`, a
 * seven-way branch on `logic.objType(n)` plus `n.finding`:
 * claim · check · finding · promise · ask · receipt · ending · event.
 *
 * DOCTRINE THIS COMPONENT CARRIES:
 *  · The State row renders `word — GLOSS[word]` keyed PER WORD from POS:63-68,
 *    not a `proven ? … : …` branch. The design's branch applies Not proven's
 *    definition to all six other words, so a `Recorded` claim reads
 *    "claimed, and no proof found in the record" — the definition of a
 *    different word (spec-doctrine E-5, F-18). The row's label is `State`, not
 *    `Canonical state` (`canonical` is pipeline vocabulary).
 *  · The advisory tail is exactly right and must survive verbatim:
 *    `no proof found in the record; the claim may be entirely true`
 *    (NS4:164-166).
 *  · The authored block sits BELOW `As the CLI narrates it`, keeps its
 *    `authored` kicker and left rule, and is italic — while the clause above it
 *    is upright (spec-doctrine C-12, E-2, F-17).
 *  · An integrity state (`Receipt intact` / `Partly checked here`) is NOT a
 *    state word and does not enter the word slot. It renders in its own
 *    integrity row (spec-doctrine F-8, A8 #9).
 *  · History: `An assessment made at one reading is never rewritten by a later
 *    one; a later assessment is added as its own line.` — verbatim (§F.1).
 *  · The command drawer's caveat `Copying does not execute.` is verbatim and
 *    permanent, and every command is COPIED, never run (doctrine 8).
 *
 * @param {object} props
 * @param {number} props.width @param {string} props.position @param {string} props.shadow
 * @param {object} props.insp `{typeLabel,title,titleFont,id,wordSlug,clause,
 *   sentences[],commentary,narr[],details[],evidence[],history[],raw,
 *   hasFindings,findings[],findingsCount,nextCheck,closing,integrity}`
 * @param {'details'|'evidence'|'history'|'raw'} props.tab
 * @param {string} props.registerLabel
 * @param {boolean} props.cmdOpen
 * @param {Array<{label:string,cmd:string,missing?:string}>} props.commands
 *   `missing` explains why a command is unavailable, e.g. `refused on degraded
 *   capture` — an honest gap string, rendered in `C.amber`
 * @param {string} props.copyNextLabel
 * @param {(tab: string) => void} props.onTab
 * @param {(findingKey: string) => void} props.onFinding
 * @param {Record<string, () => void>} props.on `onProof onSurrounding onCopyRef
 *   onCopyNext onOpenReceipt onCopyMd onToggleCmd onHide` plus
 *   `onCopyCommand(cmd)`
 * @returns {JSX.Element|null}
 */
export function Inspector({
  width,
  position,
  shadow,
  insp,
  tab,
  registerLabel,
  cmdOpen,
  commands,
  copyNextLabel,
  reading,
  onTab,
  onFinding,
  on,
}) {
  const h = on || {}
  const frame = sx(RIGHT_ASIDE, position === 'absolute' && RIGHT_OVER, { position, width, boxShadow: shadow })

  if (reading || !insp) {
    /* THREE FACTS, and they were one. A read in flight, and a working app with
       nothing selected, are not failures and are no longer painted as one.
       (The unknown-shape case still refuses: it reaches this component as a
       `<Refusal>` on the centre surface, and an inspector over a record that
       could not be read has no selection to report either.)
       D-1: nothing is selected on first paint, always. Rendering a red
       `role="alert"` for the normal opening condition of the app was the
       right-hand third of the "empty screen" the operator saw. The words are
       kept verbatim; only the housing changed, from a fault to an absence. */
    return (
      <aside aria-label="Inspector" style={frame} aria-busy={reading ? 'true' : undefined}>
        <div style={DOCK_HEAD}>
          <span>Inspector</span>
          <span style={INSP_TYPE} />
          <button type="button" className="ws-hv" onClick={h.onHide} aria-label="Hide inspector" style={sx(HIDE_BTN, HOVER.ghost())}>
            ×
          </button>
        </div>
        <div style={INSP_PANE}>
          {reading ? (
            <React.Fragment>
              <Skel n={3} rowH={26} />
              <Waiting endpoint={reading} />
            </React.Fragment>
          ) : (
            /* The change clause names the three representation controls by
               the labels they actually carry: `Graph`, `Timeline`, `Table`
               (`chrome.jsx:945,955,963`). It read "the trace, the timeline or
               the records table" — "Trace" is the DESTINATION that contains
               all three, and no control anywhere says "records table", so a
               reader was sent looking for two labels not on the screen. */
            <Absence
              title="This inspector has nothing to show."
              absent="no object in this session is selected, so there is no record to read"
              change="What would change it: select an object in Graph, Timeline or Table."
            />
          )}
        </div>
      </aside>
    )
  }

  const findings = insp.findings || []
  const notes = insp.notes || []
  /* A3-NOTES-NEVER-OPEN: the count is the typed `{findings, notes}` the
   * builder produced; `fail` + `warn` are findings, `info` are notes. */
  const typed = insp.findingsCount && typeof insp.findingsCount === 'object' ? insp.findingsCount : { findings: findings.length, notes: notes.length }
  const FC = HQ_COPY.trace.finding_counts
  const kv = tab === 'evidence' ? insp.evidence : tab === 'history' ? insp.history : insp.details

  return (
    <aside aria-label="Inspector" style={frame}>
      <div style={sx(DOCK_HEAD, { gap: '8px' })}>
        <span>Inspector</span>
        <span style={INSP_TYPE}>{insp.typeLabel}</span>
        <button type="button" className="ws-hv" onClick={h.onHide} aria-label="Hide inspector" style={sx(HIDE_BTN, HOVER.ghost())}>
          ×
        </button>
      </div>

      <div style={INSP_SUMMARY}>
        <div style={INSP_WORD_ROW}>
          {/* The word and its clause, together, always — the clause is a
              required prop of this component, so it cannot be forgotten here. */}
          <StateWord slug={insp.wordSlug} clause={insp.clause === undefined ? null : insp.clause} variant="row" gloss />
          <span style={{ flex: 1 }} />
          <span style={INSP_ID} title={insp.id}>
            {insp.id}
          </span>
        </div>
        <div style={sx(INSP_TITLE, { fontFamily: insp.titleFont || 'inherit' })}>{insp.title}</div>
        {/* N13-DRAG-257 (WCAG 2.2 SC 2.5.7): the click that moves the
            selected card, one arrow-key nudge per press. Drawn only while the
            selected object is a card on the picture. */}
        {h.onMove ? (
          <div role="group" aria-label={HQ_COPY.a11y.move_card.label} style={INSP_MOVE}>
            <span style={INSP_MOVE_LABEL}>{HQ_COPY.a11y.move_card.label}</span>
            {[['left', -1, 0, '\u2190'], ['right', 1, 0, '\u2192'], ['up', 0, -1, '\u2191'], ['down', 0, 1, '\u2193']].map(([k, dx, dy, arrow]) => (
              <button key={k} type="button" className="ws-hv" aria-label={HQ_COPY.a11y.move_card[k]} title={HQ_COPY.a11y.move_card[k]} onClick={() => h.onMove(dx, dy)} style={sx(INSP_MOVE_BTN, HOVER.ghost())}>
                <span aria-hidden="true">{arrow}</span>
              </button>
            ))}
          </div>
        ) : null}
        {insp.integrity ? (
          <div style={INTEGRITY_ROW}>
            <span style={INTEGRITY_LABEL}>Integrity</span>
            <span>{insp.integrity}</span>
          </div>
        ) : null}
        {(insp.sentences || []).map((s, i) => (
          <div key={`${i}:${s.text}`} style={sx(INSP_SENTENCE, { color: s.color || C.ink })}>
            {s.text}
          </div>
        ))}
        {/* The absence, stated ONCE, with the command that would end it.
            Silently omitting a section from every real session is itself
            hiding skipped work. The phrasing pattern is `data.js:228-241`'s
            (`Agreed checks need \`thruline ui --acceptance\`.`) and the
            affordance is the one already beside `Next useful check`. Nothing
            authored and nothing model-written may fill this space: if authored
            text is ever placed here it carries `<Authored>`, which is
            deliberately BELOW the narration and stays there. */}
        {insp.narrAbsent ? (
          <div style={NARR_ABSENT}>
            <div>{insp.narrAbsent}</div>
            {insp.narrCommand ? (
              <div style={sx(NEXT_ROW, { marginTop: '5px' })}>
                <code style={NEXT_CODE}>{insp.narrCommand}</code>
                <button
                  type="button"
                  className="ws-hv"
                  onClick={() => h.onCopyCommand && h.onCopyCommand(insp.narrCommand)}
                  style={sx(BTN, HOVER.raise())}
                >
                  Copy
                </button>
              </div>
            ) : null}
          </div>
        ) : null}
        {/* F09: the authored commentary is part of the SUMMARY, above the
            tablist, exactly as the design places it (:775-780). Rendered inside
            the Details tab it vanished the moment the reader looked at
            Evidence, History or Raw — the authored voice is about the item, not
            about one of its four views. */}
        <Authored text={insp.commentary} />
      </div>

      <div role="tablist" aria-label={HQ_COPY.a11y.inspector_tabs_label} style={TABLIST}>
        {INSP_TABS.map((t) => (
          <button
            key={t.key}
            type="button"
            role="tab"
            id={`ws-insp-tab-${t.key}`}
            aria-selected={tab === t.key}
            aria-controls={`ws-insp-pane-${t.key}`}
            className="ws-hv"
            onClick={() => onTab(t.key)}
            style={sx(TAB, tab === t.key && TAB_ON, HOVER.ghost())}
          >
            {t.label}
          </button>
        ))}
      </div>

      <div role="tabpanel" id={`ws-insp-pane-${tab}`} aria-labelledby={`ws-insp-tab-${tab}`} style={INSP_PANE}>
        {tab === 'raw' ? (
          <pre style={RAW_PRE}>{insp.raw}</pre>
        ) : (
          <React.Fragment>
            {(kv || []).map((r, i) => (
              <KvRow key={`${i}:${r.k}`} r={r} />
            ))}

            {tab === 'details' ? (
              <React.Fragment>
                {insp.hasFindings && typed.findings > 0 ? (
                  <React.Fragment>
                    {/* F13 / A3-NOTES-NEVER-OPEN: one count phrase from the
                      * copy file ("3 findings") carries the noun once, and it
                      * counts `fail` + `warn` only — the claim document's copy
                      * of this kicker reads identically. */}
                    <Kicker style={CAP_MARGIN}>{typed.findings === 1 ? FC.findings_one : fill(FC.findings_many, { n: typed.findings })}</Kicker>
                    {findings.map((f) => (
                      <button
                        key={f.key}
                        type="button"
                        className="ws-hv"
                        onClick={() => onFinding(f.key)}
                        style={sx(FINDING_BTN, HOVER.treeTab())}
                      >
                        <span style={FINDING_NAME}>{f.name}</span>
                        <Significance severity={f.severity} />
                      </button>
                    ))}
                  </React.Fragment>
                ) : null}

                {/* A3-NOTES-NEVER-OPEN: `info` findings are notes — folded
                  * away below the findings, counted in their own noun, and
                  * never called open. */}
                {insp.hasFindings && typed.notes > 0 ? (
                  <details style={NOTES_BOX}>
                    <summary style={NOTES_SUMMARY}>{typed.notes === 1 ? FC.notes_one : fill(FC.notes_many, { n: typed.notes })}</summary>
                    <div style={NOTES_ABOUT}>{FC.notes_about}</div>
                    {notes.map((f) => (
                      <button
                        key={f.key}
                        type="button"
                        className="ws-hv"
                        onClick={() => onFinding(f.key)}
                        style={sx(FINDING_BTN, HOVER.treeTab())}
                      >
                        <span style={FINDING_NAME}>{f.name}</span>
                        <Significance severity={f.severity} />
                      </button>
                    ))}
                  </details>
                ) : null}

                {insp.nextCheck ? (
                  <React.Fragment>
                    <Kicker style={CAP_MARGIN}>Next useful check</Kicker>
                    <div style={NEXT_ROW}>
                      <code style={NEXT_CODE}>{insp.nextCheck}</code>
                      <button type="button" className="ws-hv" onClick={h.onCopyNext} style={sx(BTN, HOVER.raise())}>
                        Copy
                      </button>
                    </div>
                  </React.Fragment>
                ) : null}

                {/* A HEADING OVER NOTHING IS NOT A SECTION. `insp.narr` is
                    empty for 4,242 of a 4,243-object session — the inspector
                    fills it for four object types and an event is not one of
                    them — and `S.narr` is null in every slot for every real
                    session besides, because `/api/narrate` does not exist.
                    The screenshot's empty void is the NORMAL case, not an edge
                    case. The zero-count rule is already in force one component
                    away (`:1180-1184`: "a tab whose count is zero renders the
                    tab with no count element"); a section whose body is empty
                    is the same rule. The absence itself is stated once per
                    inspector, in the summary, rather than a whole section
                    quietly disappearing from every real session (Rule 12). */}
                {(insp.narr || []).length ? (
                  <React.Fragment>
                    <Kicker style={CAP_MARGIN}>As the CLI narrates it · {registerLabel}</Kicker>
                    {insp.narr.map((s, i) => (
                      <div key={`${i}:${s.text}`} style={NARR_LINE}>
                        {s.text}
                      </div>
                    ))}
                  </React.Fragment>
                ) : null}
                {/* BELOW the narration, never between two record sentences. */}
              </React.Fragment>
            ) : null}

            {tab === 'evidence' ? <div style={CLOSING}>{insp.closing}</div> : null}

            {tab === 'history' ? (
              <div style={CLOSING}>
                An assessment made at one reading is never rewritten by a later one; a later assessment is added as its own
                line.
              </div>
            ) : null}
          </React.Fragment>
        )}
      </div>

      <div style={ACTION_BAR}>
        <button
          type="button"
          className="ws-hv"
          onClick={h.onProof}
          title="Show me the proof"
          style={sx(ACTION_BTN, ACTION_PRIMARY, HOVER.outline(C.amber))}
        >
          Show me the proof
        </button>
        <button type="button" className="ws-hv" onClick={h.onSurrounding} style={sx(ACTION_BTN, HOVER.raise())}>
          Show surrounding events
        </button>
        <button type="button" className="ws-hv" onClick={h.onCopyRef} style={sx(ACTION_BTN, HOVER.raise())}>
          Copy reference
        </button>
        <button type="button" className="ws-hv" onClick={h.onCopyNext} style={sx(ACTION_BTN, HOVER.raise())}>
          {copyNextLabel}
        </button>
        <button type="button" className="ws-hv" onClick={h.onOpenReceipt} style={sx(ACTION_BTN, HOVER.raise())}>
          Open receipt
        </button>
        <button type="button" className="ws-hv" onClick={h.onCopyMd} style={sx(ACTION_BTN, HOVER.raise())}>
          Copy Markdown
        </button>
      </div>

      <div style={CMD_WRAP}>
        <button
          type="button"
          className="ws-hv"
          onClick={h.onToggleCmd}
          aria-expanded={!!cmdOpen}
          style={sx(CMD_TOGGLE, HOVER.dataRow())}
        >
          <span style={CMD_CARET}>{cmdOpen ? '▾' : '▸'}</span>
          <span style={{ fontWeight: 500 }}>Command drawer</span>
          <span style={CMD_CAVEAT}>Copying does not execute.</span>
          <span style={{ flex: 1 }} />
          <span style={MONO_META}>
            <Count n={(commands || []).length} singular="command" plural="commands" />
          </span>
        </button>
        {cmdOpen ? (
          <div style={CMD_LIST}>
            {(commands || []).map((c) => (
              <div key={c.label} style={CMD_ITEM}>
                <div style={CMD_ITEM_HEAD}>
                  <span style={CMD_LABEL}>{c.label}</span>
                  {c.missing ? <span style={CMD_MISSING}>{c.missing}</span> : null}
                  <button
                    type="button"
                    className="ws-hv"
                    onClick={() => h.onCopyCommand && h.onCopyCommand(c.cmd)}
                    style={sx(CMD_COPY, HOVER.raise())}
                  >
                    Copy
                  </button>
                </div>
                <code style={CMD_CODE}>{c.cmd}</code>
              </div>
            ))}
          </div>
        ) : null}
      </div>
    </aside>
  )
}

/* ── The bottom dock ────────────────────────────────────────────────────── */

const DOCK_SECTION = Object.freeze({
  flex: '0 0 auto',
  minHeight: SIZE.bottomMin,
  background: C.surf1,
  borderTop: `1px solid ${C.line1}`,
  display: 'flex',
  flexDirection: 'column',
  minWidth: 0,
})
const DOCK_TABLIST = Object.freeze({
  height: SIZE.paneHeader,
  display: 'flex',
  alignItems: 'stretch',
  gap: 0,
  background: C.surf2,
  borderBottom: `1px solid ${C.lineStrong}`,
  padding: '0 6px',
})
/* The design's caption is bare italic prose in the dock's own voice; the
 * `authored` kicker and the left rule are the doctrine correction (E-1, F-17a)
 * and are the only additions to it. */
const DOCK_CAPTION = Object.freeze({
  alignSelf: 'center',
  fontSize: FS.cap,
  color: C.inkFaint,
  fontStyle: 'italic',
  paddingRight: '6px',
  borderLeft: `2px solid ${C.line2}`,
  paddingLeft: '8px',
})
const FIND_NAME = Object.freeze({ fontWeight: 500, color: C.inkBright })
const FIND_DETAIL = Object.freeze({ overflow: 'hidden', whiteSpace: 'nowrap', textOverflow: 'ellipsis', color: C.inkSoft })
const FIND_REF = Object.freeze({
  overflow: 'hidden',
  whiteSpace: 'nowrap',
  textOverflow: 'ellipsis',
  font: `${FS.sm} ${FONT.mono}`,
  color: C.inkMute,
})
const PROOF_BTN = Object.freeze({
  height: SIZE.ctlSm,
  padding: '0 8px',
  background: C.surf3,
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  fontSize: FS.sm,
  cursor: 'pointer',
  justifySelf: 'start',
})
const EV_MARK = Object.freeze({ textAlign: 'center' })
/* LEG-DIM: a row past the replay cursor, desaturated — text2, never an opacity. */
const EV_DIM = Object.freeze({ color: C.inkDim })
const EV_SEQ = Object.freeze({ font: `${FS.sm} ${FONT.mono}`, color: C.inkMute })
const EV_KIND = Object.freeze({ fontWeight: 500 })
const EV_REF = Object.freeze({
  overflow: 'hidden',
  whiteSpace: 'nowrap',
  textOverflow: 'ellipsis',
  font: `${FS.cap} ${FONT.mono}`,
})
const CHK_MONO = Object.freeze({ fontFamily: FONT.mono, fontSize: FS.ctl })
const CHK_CMD = Object.freeze({ fontFamily: FONT.mono, fontSize: FS.ctl, color: C.inkBright })
const CHK_SCOPE = Object.freeze({ color: C.inkMute, fontSize: FS.sm })
const CHK_ID = Object.freeze({
  font: `${FS.cap} ${FONT.mono}`,
  color: C.inkMute,
  overflow: 'hidden',
  whiteSpace: 'nowrap',
  textOverflow: 'ellipsis',
})
const PRM_ROW = Object.freeze({
  display: 'grid',
  gridTemplateColumns: GRID.promises,
  gap: '10px',
  padding: '6px 12px',
  alignItems: 'start',
  borderLeft: '2px solid transparent',
  cursor: 'pointer',
  fontSize: FS.body,
  borderBottom: `1px solid ${C.lineRow}`,
})
const PRM_TEXT = Object.freeze({ color: C.inkBright, lineHeight: 1.4 })
const PRM_FOLLOW = Object.freeze({ color: C.inkSoft, lineHeight: 1.4, fontSize: FS.ctl })
const PRM_REF = Object.freeze({ font: `${FS.cap} ${FONT.mono}`, color: C.inkMute, lineHeight: 1.4 })
const OUT_WRAP = Object.freeze({ padding: '6px 12px', fontFamily: FONT.mono, fontSize: FS.ctl, lineHeight: 1.6 })
const OUT_ROW = Object.freeze({ display: 'flex', gap: '12px' })
const OUT_T = Object.freeze({ color: C.inkFaint, flex: '0 0 68px' })
const RCPT_HEAD = Object.freeze({ padding: '6px 12px 2px', fontSize: FS.body, color: C.inkBright })
const RCPT_CAVEAT = Object.freeze({ padding: '0 12px 6px', fontSize: FS.sm, color: C.inkDim })
const RCPT_ROW = Object.freeze({
  display: 'grid',
  gridTemplateColumns: GRID.receipt,
  gap: '10px',
  padding: '3px 12px',
  minHeight: SIZE.subHeader,
  alignItems: 'center',
  fontSize: FS.body,
  borderBottom: `1px solid ${C.lineFaint}`,
})
const RCPT_NAME = Object.freeze({ fontFamily: FONT.mono, fontSize: FS.ctl })
const RCPT_DETAIL = Object.freeze({
  fontSize: FS.ctl,
  overflow: 'hidden',
  whiteSpace: 'nowrap',
  textOverflow: 'ellipsis',
})

/**
 * The bottom dock (spec-template :687-766) — six panes behind a `role="tablist"`.
 *
 * Panes: findings · events · checks · promises · output · receipt. Each has a
 * sticky header whose grid template is the SAME constant its rows use
 * (`GRID.findings`, `GRID.events`, `GRID.checks`, `GRID.promises`,
 * `GRID.receipt`) — a header and its rows drifting apart is the failure those
 * constants exist to stop.
 *
 * ZERO-COUNT RULE, and it is where the design breaks hardest (spec-doctrine
 * D-4…D-8, F-6): a tab whose count is zero renders **the tab with no count
 * element**. The tab stays, because it is navigation; the COUNT is what must
 * not render. No `none`, no `none open`, no `0 lines`. `<Count>` returns null at
 * zero, so this is enforced by using it.
 *
 * The events pane WINDOWS above 120 events: ±40 around `eventsFocusSeq`, and
 * the caption says so — `Showing N of M events, windowed ±40 around seq X`
 * (spec-script §B.14). The windowing is the root's; this component renders the
 * rows and the note it is given.
 *
 * The dock caption is NOT an unlabelled authored line. The design renders
 * authored commentary there italic and right-aligned with no `authored` label,
 * no border and no attribution, so it reads as the dock's own summary of the
 * record — the worst placement in the design (spec-doctrine E-1). Either it
 * carries the same `authored` kicker and left rule the inspector uses, or it
 * does not render.
 *
 * The receipt pane's caveat is verbatim: `Receipt integrity is not claim
 * support, and neither is a policy conclusion. Each is stated on its own.`
 *
 * @param {object} props
 * @param {number} props.height @param {string} props.tab
 * @param {Array<{key:string,label:string,count:number,singular:string,plural:string,active:boolean}>} props.tabs
 *   counts arrive as NUMBERS, not strings, so `<Count>` can refuse to render
 *   at zero
 * @param {{kicker:string,text:string}|null} props.caption authored-and-labelled, or null
 * @param {object} props.panes `{findings[],events[],checks[],promises[],output[],receipt[]}`
 *   plus the four notes `findingsNote eventsNote checksNote promisesNote` and
 *   `receiptHead`. A pane's rows being **absent** (`null`/`undefined`) means the
 *   record was not read and renders `<Refusal>`; an empty ARRAY means the record
 *   was read and holds nothing, and renders the pane's own honest note.
 * @param {number} props.rowH
 * @param {(tabKey: string) => void} props.onTab
 * @param {(nodeId: string) => void} props.onSelect
 * @param {(findingKey: string) => void} props.onProof the `Show proof` button
 *   in a findings row
 * @returns {JSX.Element|null}
 */
export function BottomDock({ height, tab, tabs, caption, panes, rowH, reading, onTab, onSelect, onProof }) {
  const p = panes || {}
  return (
    <section aria-label={HQ_COPY.a11y.dock_label} style={sx(DOCK_SECTION, { height })}>
      <div role="tablist" aria-label={HQ_COPY.a11y.dock_tabs_label} style={DOCK_TABLIST}>
        {(tabs || []).map((t) => (
          <button
            key={t.key}
            type="button"
            role="tab"
            id={`ws-dock-tab-${t.key}`}
            aria-selected={t.key === tab}
            aria-controls={`ws-dock-pane-${t.key}`}
            className="ws-hv"
            onClick={() => onTab(t.key)}
            style={sx(TAB, t.key === tab && TAB_ON, HOVER.ghost())}
          >
            <span>{t.label}</span>
            {/* F10/F32: `<Count>` renders NOTHING for a null or zero count, so
                an unknown count and a zero count both painted as a bare label
                and the reader could not tell "none" from "not measured". The
                tab's `note` is the honest word for whichever case it is, and it
                renders exactly when there is no counted phrase to render. */}
            {t.count ? (
              <Count n={t.count} singular={t.singular} plural={t.plural} style={TAB_COUNT} />
            ) : t.note ? (
              <span style={TAB_COUNT}>{t.note}</span>
            ) : null}
          </button>
        ))}
        <span style={{ flex: 1 }} />
        {caption && caption.text ? (
          <span style={DOCK_CAPTION}>
            <span style={AUTHORED_KICKER}>{caption.kicker || 'authored'}</span>
            {caption.text}
          </span>
        ) : null}
      </div>

      {/* A read in flight replaces the pane body, never the tab strip: the tab
          set is known chrome, and `<Count>` already renders nothing at zero, so
          the strip carries no fabricated number while it waits. */}
      {reading ? (
        <div role="tabpanel" id={`ws-dock-pane-${tab}`} aria-labelledby={`ws-dock-tab-${tab}`} aria-busy="true" style={PANE_BODY}>
          <Skel n={3} rowH={rowH} />
          <Waiting endpoint={reading} />
        </div>
      ) : (
      <div role="tabpanel" id={`ws-dock-pane-${tab}`} aria-labelledby={`ws-dock-tab-${tab}`} style={PANE_BODY}>
        {tab === 'findings' ? <FindingsPane rows={p.findings} note={p.findingsNote} rowH={rowH} onSelect={onSelect} onProof={onProof} /> : null}
        {tab === 'events' ? <EventsPane rows={p.events} note={p.eventsNote} rowH={rowH} onSelect={onSelect} /> : null}
        {tab === 'checks' ? <ChecksPane rows={p.checks} note={p.checksNote} rowH={rowH} onSelect={onSelect} /> : null}
        {tab === 'promises' ? <PromisesPane rows={p.promises} note={p.promisesNote} onSelect={onSelect} /> : null}
        {tab === 'output' ? <OutputPane rows={p.output} /> : null}
        {tab === 'receipt' ? <ReceiptPane rows={p.receipt} head={p.receiptHead} /> : null}
      </div>
      )}
    </section>
  )
}

/**
 * The findings pane (spec-template :698-710) — `GRID.findings` for both the
 * sticky header and every row, so the columns cannot drift.
 *
 * A finding carries a SEVERITY, never a state word (spec-doctrine F-4): the
 * `Significance` column is `<Significance>`, a different component with
 * different glyphs, and nothing here ever reaches the state-word slot.
 *
 * @param {object} props
 * @param {Array<{key:string,nodeId:string,name:string,severity:'fail'|'warn'|'info',detail:string,detailStyle?:string,ref:string,sel:boolean,bg?:string,edge?:string}>|null} props.rows
 * @param {string|null} props.note
 * @param {number} props.rowH
 * @param {(nodeId: string) => void} props.onSelect
 * @param {(findingKey: string) => void} props.onProof
 * @returns {JSX.Element}
 */
function FindingsPane({ rows, note, rowH, onSelect, onProof }) {
  if (!rows) {
    return (
      <Refusal
        title="This dock cannot list findings for this session."
        missing="findings were not in the record read for this session"
      />
    )
  }
  return (
    <div role="grid" aria-label="Findings" aria-rowcount={rows.length + 1} style={GRID_ROLE}>
      <div role="row" style={sx(STICKY_HEAD, { gridTemplateColumns: GRID.findings })}>
        <span role="columnheader" className="ws-head-cell">Finding</span>
        <span role="columnheader" className="ws-head-cell">Significance</span>
        <span role="columnheader" className="ws-head-cell">Basis</span>
        {/* The design's header is `Detail (recorded)` (spec-template :699). The
            parenthesis makes `recorded` a label slot of its own, and a record
            enum may not name a column (`lib/voice.js` ENUMS, `bannedIn`;
            CONTRACT §7 correction 10). Same promise, said in words. */}
        <span role="columnheader" className="ws-head-cell">Detail from the record</span>
        <span role="columnheader" className="ws-head-cell">Related path / check</span>
        <span role="columnheader" className="ws-head-cell">Evidence</span>
      </div>
      {rows.map((f) => (
        <div
          key={f.key}
          role="row"
          tabIndex={0}
          aria-selected={!!f.sel}
          className="ws-hv"
          onClick={() => f.nodeId != null && onSelect(f.nodeId)}
          onKeyDown={pressKey(onSelect, f.nodeId)}
          style={sx(
            ROW,
            {
              gridTemplateColumns: GRID.findings,
              minHeight: rowH,
              background: f.bg || 'transparent',
              borderLeftColor: f.edge || 'transparent',
            },
            HOVER.dataRow(),
          )}
        >
          <span role="gridcell" style={FIND_NAME}>
            {f.name}
          </span>
          <span role="gridcell">
            <Significance severity={f.severity} />
          </span>
          <span role="gridcell"><span className="ws-basis-chip">{({ observed: "Observed", gap: "Evidence gap", rule: "Rule", interpretation: "Interpretation" })[f.basis] || "Rule"}</span></span>
          <span role="gridcell" style={sx(FIND_DETAIL, { fontStyle: f.detailStyle || 'normal' })}>
            {f.detail}
          </span>
          <span role="gridcell" style={FIND_REF}>
            {f.ref}
          </span>
          <span role="gridcell">
            <button
              type="button"
              className="ws-hv"
              onClick={(e) => {
                e.stopPropagation()
                onProof(f.key)
              }}
              style={sx(PROOF_BTN, HOVER.raise())}
            >
              Show proof
            </button>
          </span>
        </div>
      ))}
      {note ? <div style={NOTE}>{note}</div> : null}
    </div>
  )
}

/**
 * The events pane (spec-template :711-723) — the recorded events, in recorded
 * order, at `GRID.events`.
 *
 * A row outside the replay cursor is dimmed and is NEVER removed: dimming is
 * not hiding (spec-script §F.5). Since LEG-DIM it desaturates — text, mark,
 * kind and flag in text2 (`EV_DIM`) — and is never drawn at a lower opacity.
 *
 * @param {object} props
 * @param {Array<{seq:string|number,nodeId:string,mark:string,markColor:string,kind:string,kindColor:string,text:string,font?:string,fontSize?:string,ref:string,refColor:string,sel:boolean,dim?:boolean,bg?:string,edge?:string}>|null} props.rows
 * @param {string|null} props.note
 * @param {number} props.rowH
 * @param {(nodeId: string) => void} props.onSelect
 * @returns {JSX.Element}
 */
function EventsPane({ rows, note, rowH, onSelect }) {
  if (!rows) {
    return (
      <Refusal
        title="This dock cannot list the events of this session."
        missing="the event record was not in the reading for this session"
      />
    )
  }
  return (
    <div role="grid" aria-label="Recorded events" aria-rowcount={rows.length + 1} style={GRID_ROLE}>
      <div role="row" style={sx(STICKY_HEAD, { gridTemplateColumns: GRID.events })}>
        <span role="columnheader" className="ws-head-cell" />
        <span role="columnheader" className="ws-head-cell">seq</span>
        <span role="columnheader" className="ws-head-cell">kind</span>
        <span role="columnheader" className="ws-head-cell">recorded text</span>
        <span role="columnheader" className="ws-head-cell">tree / flag</span>
      </div>
      {rows.map((e) => (
        <div
          key={String(e.seq)}
          role="row"
          tabIndex={0}
          aria-selected={!!e.sel}
          className="ws-hv"
          onClick={() => e.nodeId != null && onSelect(e.nodeId)}
          onKeyDown={pressKey(onSelect, e.nodeId)}
          style={sx(
            ROW,
            {
              gridTemplateColumns: GRID.events,
              minHeight: rowH,
              background: e.bg || 'transparent',
              borderLeftColor: e.edge || 'transparent',
            },
            HOVER.dataRow(),
            e.dim ? EV_DIM : null,
          )}
        >
          <span role="gridcell" style={sx(EV_MARK, { color: e.dim ? EV_DIM.color : e.markColor || C.focus })}>
            {/* N13-GLYPHS-ARIA: the mark is for the eye; a mark with no word
              * beside it is read as its words instead. */}
            <span aria-hidden="true">{e.mark}</span>
            {e.markLabel ? <span className="ws-shell-sr">{e.markLabel}</span> : null}
          </span>
          <span role="gridcell" style={EV_SEQ}>
            {e.seq}
          </span>
          <span role="gridcell" style={sx(EV_KIND, { color: e.dim ? EV_DIM.color : e.kindColor || C.inkSoft })}>
            {e.kind}
          </span>
          <span role="gridcell" style={sx(ELLIPSIS, { fontFamily: e.font || 'inherit', fontSize: e.fontSize || FS.body })}>
            {e.text}
          </span>
          <span role="gridcell" style={sx(EV_REF, { color: e.dim ? EV_DIM.color : e.refColor || C.inkDim })}>
            {e.ref}
          </span>
        </div>
      ))}
      {note ? <div style={NOTE}>{note}</div> : null}
    </div>
  )
}

/**
 * The checks pane (spec-template :724-737) at `GRID.checks`. Its header is the
 * one that does NOT stick, faithfully to the design.
 *
 * @param {object} props
 * @param {Array<{id:string,nodeId:string,runner:string,command:string,scope:string,counts:string,binding:string,bindColor:string,exit:string,sel:boolean,bg?:string,edge?:string}>|null} props.rows
 * @param {string|null} props.note
 * @param {number} props.rowH
 * @param {(nodeId: string) => void} props.onSelect
 * @returns {JSX.Element}
 */
function ChecksPane({ rows, note, rowH, onSelect }) {
  if (!rows) {
    return (
      <Refusal
        title="This dock cannot list the checks of this session."
        missing="no check record was read for this session"
      />
    )
  }
  return (
    <div role="grid" aria-label="Checks" aria-rowcount={rows.length + 1} style={GRID_ROLE}>
      <div role="row" style={sx(PLAIN_HEAD, { gridTemplateColumns: GRID.checks })}>
        <span role="columnheader" className="ws-head-cell">runner</span>
        <span role="columnheader" className="ws-head-cell">command · scope</span>
        <span role="columnheader" className="ws-head-cell">passed / failed / skipped</span>
        <span role="columnheader" className="ws-head-cell">binding</span>
        <span role="columnheader" className="ws-head-cell">check id</span>
        <span role="columnheader" className="ws-head-cell">exit</span>
      </div>
      {rows.map((c) => (
        <div
          key={c.id}
          role="row"
          tabIndex={0}
          className="ws-hv"
          onClick={() => c.nodeId != null && onSelect(c.nodeId)}
          onKeyDown={pressKey(onSelect, c.nodeId)}
          style={sx(
            ROW,
            {
              gridTemplateColumns: GRID.checks,
              minHeight: rowH,
              background: c.bg || 'transparent',
              borderLeftColor: c.edge || 'transparent',
            },
            HOVER.dataRow(),
          )}
        >
          <span role="gridcell" style={CHK_MONO}>
            {c.runner}
          </span>
          <span role="gridcell" style={ELLIPSIS}>
            <span style={CHK_CMD}>{c.command}</span>
            <span style={CHK_SCOPE}> · {c.scope}</span>
          </span>
          <span role="gridcell" style={CHK_MONO}>
            {c.counts}
          </span>
          <span role="gridcell" style={{ color: c.bindColor || C.ink, fontSize: FS.ctl }}>
            {c.binding}
          </span>
          <span role="gridcell" style={CHK_ID}>
            {c.id}
          </span>
          <span role="gridcell" style={CHK_MONO}>
            {c.exit}
          </span>
        </div>
      ))}
      {note ? <div style={NOTE}>{note}</div> : null}
    </div>
  )
}

/**
 * The asks-and-promises pane (spec-template :738-748) at `GRID.promises`. It is
 * the one pane with no header row.
 *
 * The word in the first cell is a state word and goes through `<StateWord>`
 * with its clause: the design paints `⁓ Loose thread` as bare text with no
 * clause anywhere on the row (spec-doctrine C-5). The record's own
 * `What would prove it:` sentence also appears verbatim in the follow-up cell,
 * because that is where the record puts it.
 *
 * @param {object} props
 * @param {Array<{id:string,nodeId:string,wordSlug:string|null,clause:string|null,text:string,followUp:string,ref:string,sel:boolean,bg?:string,edge?:string}>|null} props.rows
 * @param {string|null} props.note
 * @param {(nodeId: string) => void} props.onSelect
 * @returns {JSX.Element}
 */
function PromisesPane({ rows, note, onSelect }) {
  if (!rows) {
    return (
      <Refusal
        title="This dock cannot list the asks and promises of this session."
        missing="no ask or promise record was read for this session"
      />
    )
  }
  return (
    <div role="grid" aria-label="Asks and promises" aria-rowcount={rows.length} style={GRID_ROLE}>
      {rows.map((r) => (
        <div
          key={r.id}
          role="row"
          tabIndex={0}
          aria-selected={!!r.sel}
          className="ws-hv"
          onClick={() => r.nodeId != null && onSelect(r.nodeId)}
          onKeyDown={pressKey(onSelect, r.nodeId)}
          style={sx(
            PRM_ROW,
            { background: r.bg || 'transparent', borderLeftColor: r.edge || 'transparent' },
            HOVER.dataRow(),
          )}
        >
          <span role="gridcell">
            <StateWord slug={r.wordSlug} clause={r.clause === undefined ? null : r.clause} variant="inline" />
          </span>
          <span role="gridcell" style={PRM_TEXT}>
            {r.text}
          </span>
          <span role="gridcell" style={PRM_FOLLOW}>
            {r.followUp}
          </span>
          <span role="gridcell" style={PRM_REF}>
            {r.ref}
          </span>
        </div>
      ))}
      {note ? <div style={NOTE}>{note}</div> : null}
    </div>
  )
}

/**
 * The output pane — THE LOG (spec-template :749-753). Mono, one line per entry,
 * newest first; the root reverses its 200-line ring buffer before passing it.
 *
 * @param {object} props
 * @param {Array<{id?:string,t:string,text:string,color:string}>|null} props.rows
 * @returns {JSX.Element}
 */
function OutputPane({ rows }) {
  if (!rows) {
    return (
      <Refusal title="This dock has no log to show." missing="the session log was not started in this reading" />
    )
  }
  return (
    <div style={OUT_WRAP}>
      {rows.map((o, i) => (
        <div key={o.id || `${o.t}|${o.text}|${i}`} style={OUT_ROW}>
          <span style={OUT_T}>{o.t}</span>
          <span style={{ color: o.color || C.inkCode }}>{o.text}</span>
        </div>
      ))}
    </div>
  )
}

/**
 * The receipt pane (spec-template :754-764) at `GRID.receipt`.
 *
 * Its caveat is the sentence that keeps the three statements apart — receipt
 * integrity, claim support, and a policy conclusion are each stated on their
 * own, and this pane may never merge them.
 *
 * @param {object} props
 * @param {Array<{name:string,glyph:string,color:string,detail:string,detailColor:string}>|null} props.rows
 * @param {string} props.head the head line the root composed
 * @returns {JSX.Element}
 */
function ReceiptPane({ rows, head }) {
  if (!rows) {
    return (
      <Refusal
        title="This dock cannot show the receipt checks for this session."
        missing="no verifier output was read for this session"
        remedy="thruline verify --json"
      />
    )
  }
  return (
    <div role="grid" aria-label="Receipt checks" aria-rowcount={rows.length} style={GRID_ROLE}>
      <div style={RCPT_HEAD}>{head}</div>
      <div style={RCPT_CAVEAT}>
        Receipt integrity is not claim support, and neither is a policy conclusion. Each is stated on its own.
      </div>
      {rows.map((r) => (
        <div key={r.name} role="row" style={RCPT_ROW}>
          <span role="gridcell" style={{ color: r.color || C.inkMute }}>
            <span aria-hidden="true">{r.glyph}</span>
          </span>
          <span role="gridcell" style={RCPT_NAME}>
            {r.name}
          </span>
          <span role="gridcell" style={sx(RCPT_DETAIL, { color: r.detailColor || C.inkSoft })} title={r.detail}>
            {r.detail}
          </span>
        </div>
      ))}
    </div>
  )
}
