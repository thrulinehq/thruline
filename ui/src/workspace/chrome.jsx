import React from 'react'

import { A, BTN, C, FONT, FS, HOVER, LS, RADIUS, SHADOW, SIZE, Z, outlineButton, sx, tint } from './tokens.js'
import { wordOf } from './logic.js'
import { READING_LINE } from './state.js'
import { Icon, Kicker, Skel, StateWord } from './ui-kit.jsx'
import { HQ_COPY } from './hq-copy.js'

/* ThruLine workspace — the chrome.
 *
 * Everything that frames the work: the application bar, the destinations rail
 * and its tooltip, the document tabs, the trace toolbar, the filter strip, the
 * contextual sentence, the status strip, and the four overlays (menu, command
 * palette, toast, demo banner).
 *
 * Design range: spec-template §A lines 348-367 (app bar), 373-389 (rail +
 * tooltip), 431-441 (doc tabs), 444-492 (toolbar), 495-506 (filter bar),
 * 509-513 (sentence strip), 835-847 (menu), 850-865 (palette), 868-870 (toast),
 * 873-882 (status strip).
 *
 * PROPS ARE THE CONTRACT. Every component below receives exactly the props
 * named in its JSDoc — no more, no fewer, no context, no store, no new
 * dependency. A porter who needs another value asks the root for it by name and
 * this file's JSDoc is amended in the same commit; a porter who reaches around
 * props has broken the contract.
 *
 * Accessibility is not decoration here: the design carries 45 `role`, 78
 * `aria-*` and 12 `tabindex` attributes (spec-template §A) and every one of them
 * survives the port.
 *
 * STYLE CONVENTION (tokens.js `sx`): every style is a frozen module-level
 * constant merged by `sx()`. Nothing in JSX allocates an object literal — a
 * literal reallocates per render and cannot be shared between a sticky header
 * and the rows it lines up with. Hover is the single `.ws-hv` CSS rule reading
 * the custom properties `hv()` writes, which is how the design's 73
 * `style-hover` attributes survive byte-for-byte without 73 subscriptions.
 */

/* ── Literals this file owns, and why they are not in `tokens.js` ────────────
 *
 * `tokens.js` lifted the hexes of the TEMPLATE (spec-template §E.1, which
 * counts occurrences in lines 332-884). Three colours below live in the design's
 * SCRIPT instead, so the census never saw them; adding them to `tokens.js` is a
 * contract change owned by another porter (CONTRACT §1), so they are declared
 * once, here, with their citation, rather than re-derived at each use.
 */

/** The active row of the find listbox and of the palette (`dc.html:1276` region
 *  — 11 occurrences in the script). P6-6: the §4 strong divider, the same
 *  ground a selected row takes everywhere else. */
const ACTIVE_ROW = C.accent

/** Filter-chip skins. `ok` is a raised chip on a strong hairline; the
 *  unsupported skin (spec-script §F.3) is coral — a filter this build cannot
 *  read is a gap, not a destructive act — on its 10 % tint (P6-6, §4). */
const CHIP_SKIN = Object.freeze({
  okBg: C.surf2,
  okBd: C.line2,
  badBg: tint(C.coral),
  badBd: C.coral,
})

/* ── Verbatim strings (spec-template §F). Not one of these may be paraphrased;
 *    the typographic characters are load-bearing. ── */

/** :358 — the search box's hint. */
const SEARCH_HINT = 'Search claims, files, or events…'
/** :500 — the add-filter placeholder. */
const FILTER_PLACEHOLDER = 'facet:value · word:not-proven · file:test_shipping.py'
/** :853 — the palette placeholder. */
const PALETTE_PLACEHOLDER = 'Navigate · objects · filters · recent actions · command builders'

/** The chip flag. NOT `unsupported`: that word is a verdict enum wearing a chip
 *  label (`lib/voice.data.js:277`, spec-doctrine A8 #1 / F-10), and in the
 *  design's accusation red a reader cannot tell it from a claim verdict. */
const CHIP_FLAG = 'not a filter this build reads'
/** The unsupported chip's tooltip prefix (spec-doctrine F-10). */
const CHIP_FLAG_TITLE = 'Not a filter this build reads: '
/** :498 — the supported chip's tooltip. The grammar is documented in the tooltip. */
const CHIP_OK_TITLE = 'OR within a facet · AND across facets'

/** :487 — the primary toolbar action. NOT `Find the discrepancy`, which asserts
 *  a discrepancy before the reader has read anything and still says it on the
 *  honest session (spec-doctrine F-19). Open operator decision D-3; until it is
 *  ruled the button renders this composed string. */
const PRIMARY_ACTION = 'Line the claim up against the check'

/** :363/:1362 — the two refresh labels. */
const REFRESH_LABEL = Object.freeze({ busy: 'Refreshing…', idle: 'Refresh reading', reading: 'Reading…' })
/* The bar's two value slots while a read is in flight. A bar is known chrome
 * and keeps its geometry; only the two values it cannot yet know become
 * shapes, so nothing below jumps when they arrive. */
const SLOT_SKEL = Object.freeze({ padding: 0, margin: 0 })

/** :819/:863 — doctrine, not flavour: nothing on this screen executes the
 *  agent's work (PORT-CONTEXT §8). Verbatim and permanent. */
const COPY_NOTE = 'Copying does not execute.'

/** The two chrome glyphs that are not in `logic.ICONS` (:357, :366). */
const GLYPH = Object.freeze({
  magnifier: 'M7 3a4 4 0 1 1 0 8 4 4 0 0 1 0-8z M10 10l3.5 3.5',
  gear:
    'M8 5.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5z M8 1.5v2 M8 12.5v2 M1.5 8h2 M12.5 8h2 M3.4 3.4l1.4 1.4 M11.2 11.2l1.4 1.4 M3.4 12.6l1.4-1.4 M11.2 4.8l1.4-1.4',
})

/* ── Shared style fragments ─────────────────────────────────────────────── */

const SPACER = Object.freeze({ flex: 1 })
const ELLIPSIS = Object.freeze({
  flex: 1,
  minWidth: 0,
  overflow: 'hidden',
  whiteSpace: 'nowrap',
  textOverflow: 'ellipsis',
})
/** :394/:436/:498/:665/:774 — the ghost "×" (H6). */
const X_BTN = Object.freeze({
  background: 'transparent',
  border: 0,
  color: C.inkDim,
  cursor: 'pointer',
  fontSize: FS.body,
  padding: '0 2px',
  lineHeight: 1,
})

/**
 * A stable list key that never falls back to the array index — the design has
 * no `key` on any of its 51 lists (spec-template §B.2) and chips, find results
 * and palette rows all reorder under filtering, where an index key reuses the
 * wrong DOM node.
 *
 * @param {string} base the content-derived key
 * @param {Set<string>} seen keys already issued for this list
 * @returns {string} `base`, suffixed only if the same content appears twice
 */
function uniqueKey(base, seen) {
  let k = base
  let n = 2
  while (seen.has(k)) {
    k = `${base}#${n}`
    n += 1
  }
  seen.add(k)
  return k
}

/**
 * Focus discipline for the two overlays (spec-template Decision D6, resolved by
 * CONTRACT §8 template-D6 at the minimum bar): focus returns to whatever opened
 * the overlay when it closes. Escape itself is the root's global key map.
 *
 * @param {boolean} open whether the overlay is on screen
 * @returns {void}
 */
function useReturnFocus(open) {
  const opener = React.useRef(null)
  React.useEffect(() => {
    if (!open) return undefined
    opener.current = typeof document === 'undefined' ? null : document.activeElement
    return () => {
      const el = opener.current
      opener.current = null
      if (el && typeof el.focus === 'function' && typeof document !== 'undefined' && document.contains(el)) {
        el.focus()
      }
    }
  }, [open])
}

/* ══ APPLICATION BAR (spec-template :348-367) ═══════════════════════════════ */

const BAR = Object.freeze({
  height: `${SIZE.appBar}px`,
  flex: `0 0 ${SIZE.appBar}px`,
  display: 'flex',
  alignItems: 'center',
  gap: '8px',
  padding: '0 12px 0 10px',
  background: C.chrome,
  borderBottom: `1px solid ${C.line1}`,
})
const BRAND = Object.freeze({ display: 'flex', alignItems: 'center', gap: '7px', marginRight: '6px' })
const BADGE = Object.freeze({
  width: '18px',
  height: '18px',
  borderRadius: RADIUS.ctl,
  background: C.amber,
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  color: C.bgApp,
  font: `700 ${FS.sm} ${FONT.mono}`,
})
const WORDMARK = Object.freeze({
  fontFamily: FONT.mono,
  fontSize: FS.base,
  fontWeight: 600,
  color: C.inkBright,
})
const DEMO_PILL = Object.freeze({
  fontSize: FS.meta,
  color: C.amber,
  border: `1px solid ${C.amberBd}`,
  borderRadius: RADIUS.pill,
  padding: '1px 5px',
  fontFamily: FONT.mono,
  whiteSpace: 'nowrap',
})
const CHROME_BTN = Object.freeze({
  flex: '0 0 auto',
  whiteSpace: 'nowrap',
  height: `${SIZE.ctlBar}px`,
  padding: '0 9px',
  display: 'inline-flex',
  alignItems: 'center',
  gap: '6px',
  background: C.surf2Hi,
  border: `1px solid ${C.lineField}`,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  fontSize: FS.body,
  cursor: 'pointer',
})
const BTN_CAP = Object.freeze({ color: C.inkDim, fontSize: FS.cap })
const BTN_VALUE = Object.freeze({ fontWeight: 500 })
const BTN_MONO = Object.freeze({ fontFamily: FONT.mono, fontSize: FS.ctl, whiteSpace: 'nowrap' })
const CARET = Object.freeze({ color: C.inkDim })
const SEARCH = Object.freeze({
  flex: '1 1 120px',
  minWidth: 0,
  maxWidth: `${SIZE.searchMax}px`,
  height: `${SIZE.ctlBar}px`,
  display: 'flex',
  alignItems: 'center',
  gap: '8px',
  padding: '0 8px',
  background: C.fieldBg,
  border: `1px solid ${C.lineField}`,
  borderRadius: RADIUS.ctl,
  cursor: 'text',
  color: C.inkDim,
  fontSize: FS.body,
  overflow: 'hidden',
})
const KBD = Object.freeze({
  fontFamily: FONT.mono,
  fontSize: FS.meta,
  color: C.inkMute,
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.pill,
  padding: '0 4px',
  flex: '0 0 auto',
})
const SPINNER = Object.freeze({
  width: '10px',
  height: '10px',
  borderRadius: RADIUS.round,
  border: `1.5px solid ${C.inkMute}`,
  borderTopColor: 'transparent',
  /* Still (P6-6, §4: nothing loops). The ring with its top open sits beside
     the "Refreshing…" label, which is the part that says what is happening. */
})
const SPINNER_ON = Object.freeze({ display: 'inline-block' })
const SPINNER_OFF = Object.freeze({ display: 'none' })
const SESSION_CHIP = Object.freeze({ fontSize: FS.sm, whiteSpace: 'nowrap' })

/**
 * The 44px application bar (spec-template :348-367).
 *
 * Left to right: the brand cluster (an 18px amber `t` badge, the `thruline`
 * wordmark, and the demo pill) · the project menu button · the session menu
 * button carrying the session's state word · the search box that opens the
 * palette · a spacer · Refresh · Settings.
 *
 * @param {object} props
 * @param {string} props.projectLabel the open project's name
 * @param {string} props.sessionLabel the open session's label, rendered mono
 * @param {string|null} props.sessionWord a `logic.WORD` key, or null. Rendered
 *   through `<StateWord variant="chip" clauseHidden>` — the ONE sanctioned
 *   clause-free word in the app, because the chip is a button that opens the
 *   session summary and must carry the accessible name
 *   `{word} — open the session summary` (spec-doctrine C-7, F-2).
 * @param {string|null} props.demoLabel `demo-data.DEMO_LABEL.*` when a demo
 *   session is active, else null. Never a "this is real data" pill — real data
 *   renders NO pill (spec-template Decision D2).
 * @param {'idle'|'refreshing'|'done'} props.refreshStatus drives the spinner
 *   (`display` — a still ring since P6-6, which loops nothing) and the label
 * @param {() => void} props.onProjectMenu opens the generic menu, kind `project`
 * @param {() => void} props.onSessionMenu kind `session`
 * @param {() => void} props.onSettingsMenu kind `settings`
 * @param {() => void} props.onOpenPalette the search box IS the palette opener
 * @param {() => void} props.onRefresh must call `/api/refresh` and report what
 *   actually changed, including a failure. The design's `refresh()` reads
 *   nothing and logs "nothing changed" after 1100ms — that stub does not ship
 *   (spec-script §G.4).
 * @param {boolean} [props.reading] a read is in flight. The two value slots
 *   become inert bars rather than guessed labels — `no project open` / `no
 *   session open` while the app is opening one says a door is shut when it is
 *   being opened — no word chip renders (a word cannot be guessed), and Refresh
 *   is disabled because a second read while the first is in flight is not a
 *   thing this app can honour.
 * @returns {JSX.Element}
 */
export function AppBar({
  projectLabel,
  sessionLabel,
  sessionWord,
  demoLabel,
  refreshStatus,
  reading,
  onProjectMenu,
  onSessionMenu,
  onSettingsMenu,
  onOpenPalette,
  onRefresh,
  onConnections,
}) {
  const busy = refreshStatus === 'refreshing'
  const w = wordOf(sessionWord)
  /* C-7: the word alone is persistent chrome with no room for its clause, so
   * doctrine's answer is the clause ONE TAP BEHIND — the chip is part of a
   * control that opens the session, and the control says so out loud.
   *
   * DOC-06: the name used to promise "open the session summary", which this
   * button does not do — `onSessionMenu` opens the session SWITCHER. An
   * accessible name that states an action the control does not perform is the
   * same defect this product exists to catch, so the name now says what the
   * button does, and the clause it owes is one tap behind IN the switcher:
   * every negative word there carries its own clause (shell.jsx, DOC-07).
   *
   * AND THE SWITCHER NOW SWITCHES. That correction was made one layer too high
   * for a while: the name said "switcher" truthfully about the control, while
   * the menu it opened listed `corpus.order`, which on the live path was the
   * one session already open with a checkmark against it. The menu reads the
   * session listing (`/api/sessions`), so the name is true the whole way
   * down. */
  const sessionName = w.word
    ? `session ${sessionLabel} — ${w.word} — open the session switcher`
    : `session ${sessionLabel}`

  /* THE IN-FLIGHT ACCESSIBLE NAME IS THE SHIPPED SENTENCE. While the bars
   * render, the accessible name is the ONLY channel a non-sighted reader has,
   * and it was a lowercase fragment ("project — reading the record") — no
   * capital, no stop, and a second phrasing of a sentence the product already
   * owns. `READING_LINE` is that sentence (`state.js:33`), so both buttons
   * speak it verbatim rather than paraphrasing it (F-6). */
  return (
    <div className="ws-appbar" style={BAR}>
      <div style={BRAND}>
        <span style={BADGE}>t</span>
        <span style={WORDMARK}>thruline</span>
        {demoLabel ? <span style={DEMO_PILL}>{demoLabel}</span> : null}
      </div>

      <button
        type="button"
        className="ws-hv"
        onClick={onProjectMenu}
        aria-haspopup="menu"
        disabled={!!reading}
        aria-label={reading ? `Project — ${READING_LINE}` : undefined}
        style={sx(CHROME_BTN, HOVER.chrome())}
      >
        <span style={BTN_CAP}>project</span>
        {reading ? <Skel n={1} rowH={22} style={sx(SLOT_SKEL, { width: '86px' })} /> : <span title={projectLabel} style={BTN_VALUE}>{projectLabel.includes('/') ? projectLabel.split('/').filter(Boolean).at(-1) : projectLabel}</span>}
        {reading ? null : <span style={CARET}>▾</span>}
      </button>

      <button
        type="button"
        className="ws-hv"
        onClick={onSessionMenu}
        aria-haspopup="menu"
        disabled={!!reading}
        aria-label={reading ? `Session — ${READING_LINE}` : sessionName}
        style={sx(CHROME_BTN, HOVER.chrome())}
      >
        <span style={BTN_CAP}>session</span>
        {reading ? (
          <Skel n={1} rowH={22} style={sx(SLOT_SKEL, { width: '104px' })} />
        ) : (
          <React.Fragment>
            <span title={sessionLabel} style={BTN_MONO}>{/^[a-f0-9-]{30,}$/i.test(sessionLabel) ? sessionLabel.slice(0, 8) : sessionLabel}</span>
            <StateWord slug={sessionWord} clause={null} clauseHidden variant="chip" style={SESSION_CHIP} />
            <span style={CARET}>▾</span>
          </React.Fragment>
        )}
      </button>

      <div role="search" className="ws-hv" onClick={onOpenPalette} style={sx(SEARCH, HOVER.field())}>
        <Icon d={GLYPH.magnifier} size={13} strokeWidth={1.5} />
        <span style={ELLIPSIS}>{SEARCH_HINT}</span>
        <kbd style={KBD}>⌘K</kbd>
      </div>

      <div style={SPACER} />

      <button type="button" className="ws-connect-button" onClick={onConnections} aria-label="Connections · local records available, agents not connected"><span className="ws-provider-mono">EX</span><span>Connections</span><span title="Local records available">●</span><span title="Live agents not connected">◌</span></button>

      <button
        type="button"
        className="ws-hv"
        onClick={onRefresh}
        aria-live="polite"
        disabled={!!reading}
        style={sx(CHROME_BTN, HOVER.chrome())}
      >
        <span style={sx(SPINNER, busy || reading ? SPINNER_ON : SPINNER_OFF)} />
        <span>{reading ? REFRESH_LABEL.reading : busy ? REFRESH_LABEL.busy : REFRESH_LABEL.idle}</span>
      </button>

      <button
        type="button"
        className="ws-hv"
        onClick={onSettingsMenu}
        aria-label="Settings"
        style={sx(CHROME_BTN, HOVER.chrome())}
      >
        <Icon d={GLYPH.gear} size={13} strokeWidth={1.5} />
        Settings
      </button>
    </div>
  )
}

/* ══ DESTINATIONS RAIL (spec-template :373-386) ═════════════════════════════ */

const RAIL_NAV = Object.freeze({
  width: `${SIZE.rail}px`,
  flex: `0 0 ${SIZE.rail}px`,
  background: C.chrome,
  borderRight: `1px solid ${C.line1}`,
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'stretch',
  paddingTop: '4px',
  overflowY: 'auto',
  overflowX: 'hidden',
  scrollbarWidth: 'none',
})
const RAIL_GROUP = Object.freeze({
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'stretch',
  paddingBottom: '4px',
  borderBottom: `1px solid ${C.surf3}`,
  marginBottom: '4px',
})
const RAIL_CAP = Object.freeze({
  height: `${SIZE.railCaption}px`,
  /* N13-F11-AXE-SR (axe target-size, WCAG 2.2 SC 2.5.8): the caption is a
   * button, so it keeps the 24 px control height as its floor. */
  minHeight: SIZE.ctl,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  gap: '2px',
  background: 'transparent',
  border: 0,
  color: C.inkFaint,
  fontSize: '8.5px',
  letterSpacing: LS.rail,
  textTransform: 'uppercase',
  cursor: 'pointer',
  padding: 0,
})
const RAIL_CARET = Object.freeze({ fontSize: '8px' })
const RAIL_ITEM = Object.freeze({
  height: `${SIZE.railItem}px`,
  margin: '1px 5px',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  background: 'transparent',
  border: 0,
  borderLeft: '2px solid transparent',
  borderRadius: RADIUS.ctl,
  color: C.inkMute,
  cursor: 'pointer',
  padding: 0,
  position: 'relative',
})
const RAIL_ITEM_ON = Object.freeze({
  background: C.surf3,
  borderLeft: `2px solid ${C.amber}`,
  color: C.inkBright,
})

/**
 * The rail group's three-letter caption (`dc.html:1255`).
 *
 * @param {string} key a `logic.RAIL` group key
 * @returns {string} `REV` · `EVID` · `TOOLS`
 */
function railShort(key) {
  if (key === 'evidence') return 'EVID'
  if (key === 'tools') return 'TOOLS'
  return 'REV'
}

/**
 * The 48px destinations rail (spec-template :373-386) — three collapsible
 * groups of five, which is the brief's §3.1 sidebar.
 *
 * @param {object} props
 * @param {ReadonlyArray<{title:string,key:string,items:Array<[string,string]>}>} props.rail
 *   `logic.RAIL`, unmodified
 * @param {string} props.active the current destination key; sets `aria-current="page"`
 * @param {Record<string, boolean>} props.closed group key → collapsed
 * @param {(groupKey: string) => void} props.onToggleGroup
 * @param {(destKey: string) => void} props.onGo
 * @param {(destKey: string, label: string, top: number) => void} props.onHover
 *   fires on BOTH mouseenter and focus (:379) — the tooltip is keyboard-reachable
 * @param {() => void} props.onHoverEnd fires on mouseleave and blur
 * @returns {JSX.Element}
 */
export function Rail({ rail, active, closed, onToggleGroup, onGo, onHover, onHoverEnd }) {
  const navRef = React.useRef(null)

  /* The design measured the tooltip's top against `closest('[data-screen-label]')`
   * and subtracted the 44px app bar — an editor attribute that does not survive
   * the port (spec-script §E6). The rail's own box starts at the middle row's
   * top, so measuring against it is the same number with no global reach. */
  const enter = (key, label) => (e) => {
    const nav = navRef.current
    const r = e.currentTarget.getBoundingClientRect()
    const nr = nav ? nav.getBoundingClientRect() : { top: 0 }
    onHover(key, label, r.top - nr.top + 8)
  }

  return (
    <nav ref={navRef} aria-label="Destinations" style={RAIL_NAV}>
      {rail.map((g) => {
        const open = !closed[g.key]
        return (
          <div key={g.key} style={RAIL_GROUP}>
            <button
              type="button"
              className="ws-hv"
              onClick={() => onToggleGroup(g.key)}
              aria-expanded={open}
              style={sx(RAIL_CAP, HOVER.railCaption())}
            >
              {railShort(g.key)}
              <span style={RAIL_CARET}>{open ? '▾' : '▸'}</span>
            </button>
            {open
              ? g.items.map(([key, label]) => {
                  const on = key === active
                  return (
                    <button
                      key={key}
                      type="button"
                      className="ws-hv"
                      onClick={() => onGo(key)}
                      onMouseEnter={enter(key, label)}
                      onMouseLeave={onHoverEnd}
                      onFocus={enter(key, label)}
                      onBlur={onHoverEnd}
                      aria-label={label}
                      aria-current={on ? 'page' : 'false'}
                      style={sx(RAIL_ITEM, on && RAIL_ITEM_ON, HOVER.railItem())}
                    >
                      <Icon name={key} size={16} />
                    </button>
                  )
                })
              : null}
          </div>
        )
      })}
    </nav>
  )
}

const TOOLTIP = Object.freeze({
  position: 'absolute',
  left: '52px',
  zIndex: Z.railTip,
  background: C.surf3,
  border: `1px solid ${C.linePop}`,
  borderRadius: RADIUS.ctl,
  padding: '3px 8px',
  fontSize: FS.ctl,
  color: C.inkBright,
  boxShadow: SHADOW.tooltip,
  pointerEvents: 'none',
  whiteSpace: 'nowrap',
})

/**
 * The rail's hover tooltip (spec-template :387-389). `role="tooltip"`,
 * absolutely positioned at `left:52px, top:{top}px`, z-index 60,
 * `pointer-events: none`.
 *
 * @param {object} props
 * @param {{text:string, top:number}|null} props.tip null hides it
 * @returns {JSX.Element|null}
 */
export function RailTooltip({ tip }) {
  if (!tip) return null
  return (
    <div role="tooltip" style={sx(TOOLTIP, { top: `${tip.top}px` })}>
      {tip.text}
    </div>
  )
}

/* ══ DEMO BANNER ═══════════════════════════════════════════════════════════ */

const BANNER = Object.freeze({
  position: 'sticky',
  top: 0,
  zIndex: Z.dock,
  flex: '0 0 auto',
  display: 'flex',
  alignItems: 'center',
  gap: '12px',
  padding: '6px 12px',
  background: C.amber,
  color: C.bgApp,
  font: `600 ${FS.sm} ${FONT.mono}`,
})
const BANNER_TAIL = Object.freeze({ fontWeight: 400 })

/**
 * The permanent demo banner. Sticky, `top: 0`, and NOT conditionally rendered
 * while demo data is on screen.
 *
 * There is no control that turns it off — `lib/inbox.test.js:322-345` walks
 * every file listed in `DEMO_SURFACES` for exactly that and fails on any of
 * them. If this component ships, its file and every file importing
 * `demo-data.js` join both `DEMO_SURFACES` and `ALL_SURFACES` (CONTRACT §5
 * edit 5).
 *
 * @param {object} props
 * @param {string} props.label `demo-data.DEMO_LABEL.bundled` or `.fixture`
 * @param {number} [props.n] fixture ordinal, for the fixture-mode strip
 *   `DEMO DATA — fixture {n} of {m}, not this machine's sessions` (BRIEF:1599)
 * @param {number} [props.m]
 * @returns {JSX.Element}
 */
export function DemoBanner({ label, n, m }) {
  return (
    /* `position: 'sticky'` and `top: 0` are written at the element as well as
       in BANNER: they ARE the guarantee — the banner cannot scroll away from
       the data it marks — and `lib/inbox.test.js:337-341` reads them here, in
       the source that follows the attribute, exactly as `cast.jsx:78-82` does.
       `sx` merges, later wins, and the two values are identical, so nothing
       renders differently. */
    <div data-demo-banner="workspace" style={sx(BANNER, { position: 'sticky', top: 0 })}>
      <strong>{label}</strong>
      {typeof n === 'number' && typeof m === 'number' ? (
        <span style={BANNER_TAIL}>{`DEMO DATA — fixture ${n} of ${m}, not this machine's sessions`}</span>
      ) : null}
    </div>
  )
}

/* ══ DOCUMENT TABS (spec-template :431-441) ═════════════════════════════════ */

const TABSTRIP = Object.freeze({
  height: `${SIZE.docTabs}px`,
  flex: `0 0 ${SIZE.docTabs}px`,
  display: 'flex',
  alignItems: 'flex-end',
  gap: '2px',
  padding: '0 8px',
  background: C.chrome,
  borderBottom: `1px solid ${C.line1}`,
  overflow: 'hidden',
})
const TAB = Object.freeze({
  height: `${SIZE.docTab}px`,
  display: 'flex',
  alignItems: 'center',
  gap: '7px',
  padding: '0 10px',
  background: C.lineFaint,
  /* R9: LONG-HAND ONLY — see the same note in `docks.jsx`. `border` beside
   * `borderBottom: 0`, overridden by a second object that set both again, made
   * React log a style-shorthand error on every document-tab switch. */
  borderWidth: '1px 1px 0 1px',
  borderStyle: 'solid',
  borderColor: C.surf3,
  borderRadius: RADIUS.tab,
  color: C.inkMute,
  fontSize: FS.body,
  cursor: 'pointer',
  maxWidth: `${SIZE.docTabMax}px`,
})
const TAB_ON = Object.freeze({
  background: C.canvas,
  borderWidth: '1px 1px 0 1px',
  borderStyle: 'solid',
  borderColor: C.line2,
  color: C.inkBright,
})
const TAB_ICON = Object.freeze({ fontSize: FS.meta })
const TAB_LABEL = Object.freeze({ overflow: 'hidden', whiteSpace: 'nowrap', textOverflow: 'ellipsis' })
const TAB_CLOSE = Object.freeze({ fontSize: FS.lg })
/* N13-F11-AXE-SR: the × is a 24 px pointer target (WCAG 2.2 SC 2.5.8), not a
 * control nested in the tab (axe nested-interactive); Delete is its key. */
const TAB_CLOSE_HIT = Object.freeze({ display: 'inline-flex', alignItems: 'center', justifyContent: 'center', width: SIZE.ctl, height: SIZE.ctl })

/**
 * The document tab strip (spec-template :431-441). `role="tablist"`, 32px, tabs
 * bottom-aligned at 27px with `borderRadius: '3px 3px 0 0'`.
 *
 * The `trace` tab is prepended by the root and is NOT closeable; `claim` and
 * `compare` live in state and are (spec-script §A field 4).
 *
 * The design's italic tagline at :440 DOES NOT SHIP. It is profanity in
 * permanent chrome asserting the record contains deceit, which collapses "no
 * proof found" into "a lie found" and fails the first-time test outright
 * (spec-doctrine F-1, decision D-5). The slot renders nothing until an operator
 * supplies a §8.3 lens sentence.
 *
 * @param {object} props
 * @param {Array<{id:string,label:string,icon:string,iconColor:string,font:string,closeable:boolean}>} props.tabs
 * @param {string} props.active
 * @param {(id: string) => void} props.onSelect
 * @param {(id: string) => void} props.onClose
 * @returns {JSX.Element}
 */
export function DocTabs({ tabs, active, onSelect, onClose }) {
  return (
    <div role="tablist" aria-label="Documents" style={TABSTRIP}>
      {tabs.map((t) => {
        const on = t.id === active
        return (
          <div
            key={t.id}
            role="tab"
            tabIndex={0}
            aria-selected={on}
            aria-keyshortcuts={t.closeable ? 'Delete' : undefined}
            className="ws-hv"
            onClick={() => onSelect(t.id)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') onSelect(t.id)
              /* APG tabs: Delete closes a tab that can be closed. */
              if (t.closeable && e.key === 'Delete') {
                e.preventDefault()
                onClose(t.id)
              }
            }}
            style={sx(TAB, on && TAB_ON, HOVER.treeTab())}
          >
            <span style={sx(TAB_ICON, { color: t.iconColor })} aria-hidden="true">{t.icon}</span>
            <span style={sx(TAB_LABEL, { fontFamily: t.font || 'inherit' })}>{t.label}</span>
            {t.closeable ? <span className="ws-shell-sr">{' ' + HQ_COPY.a11y.tab_close_hint}</span> : null}
            {t.closeable ? (
              <span
                aria-hidden="true"
                title={HQ_COPY.a11y.tab_close_title}
                data-close-doc={t.id}
                className="ws-hv"
                onClick={(e) => {
                  e.stopPropagation()
                  onClose(t.id)
                }}
                style={sx(X_BTN, TAB_CLOSE, TAB_CLOSE_HIT, HOVER.ghost())}
              >
                ×
              </span>
            ) : null}
          </div>
        )
      })}
      <div style={SPACER} />
    </div>
  )
}

/* ══ TOOLBAR (spec-template :444-492) ═══════════════════════════════════════ */

const TOOLBAR = Object.freeze({
  display: 'flex',
  flexWrap: 'wrap',
  /* N13-FOCUS-2411: a toolbar that wraps keeps its full height. Shrunk by its
   * column (1024 wide, Workbench), its second row was drawn under the strip below
   * it, and a Tab stop there was entirely hidden. */
  flexShrink: 0,
  alignItems: 'center',
  gap: '4px 6px',
  padding: '5px 8px',
  background: C.surf1,
  borderBottom: `1px solid ${C.surf3}`,
  minHeight: `${SIZE.toolbarMin}px`,
})
const SEG = Object.freeze({
  display: 'inline-flex',
  border: `1px solid ${C.lineField}`,
  borderRadius: RADIUS.ctl,
  overflow: 'hidden',
})
const SEG_BTN = Object.freeze({
  height: `${SIZE.ctl}px`,
  padding: '0 9px',
  background: C.surf3,
  border: 0,
  color: C.ink,
  fontSize: FS.ctl,
  cursor: 'pointer',
})
const SEG_BTN_ON = Object.freeze({ background: C.accent })
const SEG_DIVIDER = Object.freeze({ borderLeft: `1px solid ${C.lineField}` })
/* Step 5: "7 of 26 objects shown · 19 merged" — the dim-not-hide invariant
 * said in words for the one view that folds. Mono like `MATCH_LINE`. */
const FOCUS_PILL = Object.freeze({
  height: `${SIZE.ctl}px`,
  display: 'inline-flex',
  alignItems: 'center',
  padding: '0 9px',
  border: `1px solid ${C.lineField}`,
  borderRadius: RADIUS.ctl,
  color: C.inkDim,
  fontSize: FS.sm,
  fontFamily: FONT.mono,
  whiteSpace: 'nowrap',
})
const FOCUS_TOOLS = Object.freeze({ marginLeft: 'auto' })
const GROUP = Object.freeze({ display: 'inline-flex', gap: '2px' })
const GROUP_MID = Object.freeze({ display: 'inline-flex', alignItems: 'center', gap: '2px' })
const CTL = Object.freeze({
  height: `${SIZE.ctl}px`,
  padding: '0 8px',
  background: C.surf3,
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  fontSize: FS.ctl,
  cursor: 'pointer',
})
const CTL_TIGHT = Object.freeze({ padding: '0 7px' })
const CTL_START = Object.freeze({ borderRadius: RADIUS.segStart })
const CTL_END = Object.freeze({ borderRadius: RADIUS.segEnd })
const CTL_MID = Object.freeze({ borderRadius: 0 })
const CTL_ON = Object.freeze({ background: C.accent, border: `1px solid ${C.accentBd}` })
const CTL_ON_FLAT = Object.freeze({ background: C.accent })
const ICON_CTL = Object.freeze({ width: `${SIZE.ctl}px`, padding: 0, fontSize: FS.body })
const STEP_CTL = Object.freeze({ width: '26px', padding: 0, fontSize: FS.sm })
const ZOOM_READOUT = Object.freeze({
  height: `${SIZE.ctl}px`,
  minWidth: '40px',
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  background: C.lineFaint,
  border: `1px solid ${C.line2}`,
  color: C.inkMute,
  fontSize: FS.cap,
  fontFamily: FONT.mono,
})
const REPLAY_LABEL = Object.freeze({ fontFamily: FONT.mono, borderRadius: 0 })
/* The toolbar's primary: the view's ONE filled amber button (§4, BTN.primary). */
const PRIMARY = Object.freeze({
  height: `${SIZE.ctl}px`,
  padding: '0 10px',
  ...BTN.primary,
  borderRadius: RADIUS.ctl,
  fontSize: FS.ctl,
  cursor: 'pointer',
})
const FIND_WRAP = Object.freeze({ position: 'relative' })
const FIND_INPUT = Object.freeze({
  height: `${SIZE.ctl}px`,
  width: `${SIZE.findInput}px`,
  boxSizing: 'border-box',
  background: C.fieldBg,
  border: `1px solid ${C.lineField}`,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  padding: '0 7px',
  fontSize: FS.ctl,
})
const FIND_BOX = Object.freeze({
  position: 'absolute',
  left: 0,
  top: '26px',
  width: `${SIZE.findListbox}px`,
  maxHeight: `${SIZE.findListboxMaxH}px`,
  overflow: 'auto',
  zIndex: Z.findListbox,
  background: C.surf2,
  border: `1px solid ${C.linePop}`,
  borderRadius: RADIUS.ctl,
  boxShadow: SHADOW.findListbox,
})
const FIND_HEAD = Object.freeze({
  padding: '4px 8px',
  fontSize: FS.cap,
  color: C.inkDim,
  borderBottom: `1px solid ${C.line1}`,
})
const FIND_OPT = Object.freeze({
  width: '100%',
  display: 'flex',
  alignItems: 'center',
  gap: '8px',
  padding: '5px 8px',
  background: 'transparent',
  border: 0,
  color: C.ink,
  fontSize: FS.ctl,
  cursor: 'pointer',
  textAlign: 'left',
})
const FIND_OPT_ON = Object.freeze({ background: ACTIVE_ROW })
const FIND_KIND = Object.freeze({
  fontSize: FS.meta,
  color: C.inkDim,
  width: '52px',
  flex: '0 0 52px',
  fontFamily: FONT.mono,
})
const FIND_WHERE = Object.freeze({ fontSize: FS.meta, color: C.inkFaint, fontFamily: FONT.mono })

/**
 * The trace toolbar (spec-template :444-492) — `role="toolbar"`, flex-wrap,
 * eight labelled groups plus the primary action.
 *
 * Groups, in order: Representation (Graph/Timeline/Table, segmented) · Find in
 * trace (input + listbox) · Findings (◂ Finding / Finding ▸) · the three
 * toggles (Show findings only, Focus selection, Show linked evidence) · Zoom
 * (Fit, −, readout, +) · Detail (Expand, Collapse, Widen) · Replay (|◂, label,
 * ▸|) · Jump to stop · Show test changes · Compare sessions · THE PRIMARY ·
 * spacer · Panels ▾ · Focus canvas · Reset layout.
 *
 * THE PRIMARY ACTION IS NOT `Find the discrepancy` (spec-doctrine F-19, E-8):
 * that framing asserts a discrepancy exists before the reader has read
 * anything, and on the honest session it still says it. The action is a
 * comparison, not a hunt. Its label is an OPEN OPERATOR DECISION (D-3) and
 * until it is ruled the button renders `PRIMARY_ACTION`. The empty-state toast
 * stays verbatim: `No completion claim in this session — nothing to reconcile.`
 *
 * @param {object} props
 * @param {'graph'|'timeline'|'table'} props.rep
 * @param {string} props.findText @param {boolean} props.findOpen
 * @param {Array<{id:string,kind:string,text:string,where:string,active:boolean}>} props.findResults
 *   ALREADY sliced to the 40 the listbox renders — the design clamps the
 *   keyboard cursor to the FULL list and moves an invisible selection past 40
 *   (spec-script §D.2, a known bug); the port clamps to what it renders.
 * @param {string} props.findCountLine
 * @param {boolean} props.findingsOnly @param {boolean} props.linked
 * @param {boolean} props.focusCanvas
 * @param {string} props.zoomPct @param {string} props.replayLabel
 * @param {string} [props.replayGlyph] drawn before the label, hidden from a screen reader
 * @param {boolean} props.replayPlaying
 * @param {Record<string, Function>} props.on the 27 toolbar callbacks, keyed by
 *   the design's own names so the mapping stays checkable: `onRepGraph
 *   onRepTimeline onRepTable onFind onFindKey onFindFocus onPrevFinding
 *   onNextFinding onFindingsOnly onFocusSel onLinked onFit onZoomOut onZoomIn
 *   onExpand onCollapse onWiden onStepBack onReplay onStepFwd onJumpStop
 *   onTestChanges onCompare onDiscrepancy onPanelsMenu onFocusCanvas
 *   onResetLayout` (spec-script §C.1). `onFind`, `onFindKey` and `onFindFocus`
 *   receive the DOM event, as they do in the design.
 * @returns {JSX.Element}
 */
export function Toolbar({
  rep,
  /* R10: the graph-only groups render only over the Trace document. Over the
   * Compare or Claim document they acted on nothing the reader could see — the
   * zoom readout moved from 96% to 115% with no graph on screen — which is an
   * enabled control lying about what it does.
   * R6: `compact` folds the secondary groups into one `More ▾` menu below
   * ~1400px, where the toolbar otherwise wrapped to three rows and left the
   * canvas 204px of a 720px window. Nothing is dropped; it moves. */
  traceDoc = true,
  compact = false,
  findText,
  findOpen,
  findResults,
  findCountLine,
  findingsOnly,
  linked,
  /* Step 5 (HQ-VERDICT-PANEL-PLAN): Focus / Everything, and the pill that
   * accounts for what Focus merged. Both render over the Trace document only. */
  focusMode = 'everything',
  focusLine = '',
  focusCanvas,
  zoomPct,
  replayLabel,
  replayGlyph,
  replayPlaying,
  on,
  /* P6-1: Explore's slim form, its label, the shell's two slots and one extra control. */
  slim = false,
  slimLabel,
  slots = null,
  /* T-6-6: over a record that holds nothing, the empty Trace's own action
   * ("Choose another session") is the screen's one primary, so this button
   * steps back to an ordinary control and gives up `data-primary-action`.
   * It stays, with the same label and handler, so nothing is taken away. */
  primaryQuiet = false,
  /* TRACE-MISSING-SESSION-TRUE (D-332): over a session that could not be read
   * there is nothing to line up, so the primary is not drawn at all. */
  primaryHidden = false,
  extra = null,
  /* P6-3: the scrubber — one range over record order. Explore draws it in
   * place of the replay buttons; Workbench draws it beside them. */
  scrubber = null,
}) {
  const seen = new Set()
  const inFocus = focusMode === 'focus'
  const representation = (
    <React.Fragment>
      {traceDoc ? (
      <div role="group" aria-label="Representation" style={SEG}>
        <button
          type="button"
          className="ws-hv"
          onClick={on.onRepGraph}
          aria-pressed={rep === 'graph'}
          style={sx(SEG_BTN, rep === 'graph' && SEG_BTN_ON, HOVER.raise())}
        >
          Graph
        </button>
        <button
          type="button"
          className="ws-hv"
          onClick={on.onRepTimeline}
          aria-pressed={rep === 'timeline'}
          style={sx(SEG_BTN, SEG_DIVIDER, rep === 'timeline' && SEG_BTN_ON, HOVER.raise())}
        >
          Timeline
        </button>
        <button
          type="button"
          className="ws-hv"
          onClick={on.onRepTable}
          aria-pressed={rep === 'table'}
          style={sx(SEG_BTN, SEG_DIVIDER, rep === 'table' && SEG_BTN_ON, HOVER.raise())}
        >
          Table
        </button>
      </div>
      ) : null}

    </React.Fragment>
  )
  const findBox = (
      <div style={FIND_WRAP}>
        <input
          value={findText}
          onChange={on.onFind}
          onKeyDown={on.onFindKey}
          onFocus={on.onFindFocus}
          placeholder="Find in trace…"
          aria-label="Find in trace"
          style={FIND_INPUT}
        />
        {findOpen ? (
          <div role="listbox" style={FIND_BOX}>
            <div style={FIND_HEAD}>{findCountLine}</div>
            {findResults.map((r) => (
              <button
                key={uniqueKey(r.id, seen)}
                type="button"
                role="option"
                aria-selected={!!r.active}
                className="ws-hv"
                onClick={r.onClick}
                style={sx(FIND_OPT, r.active && FIND_OPT_ON, HOVER.chrome())}
              >
                <span style={FIND_KIND}>{r.kind}</span>
                <span style={ELLIPSIS}>{r.text}</span>
                <span style={FIND_WHERE}>{r.where}</span>
              </button>
            ))}
          </div>
        ) : null}
      </div>
  )
  const zoom = (
      <div role="group" aria-label="Zoom" style={GROUP_MID}>
        <button
          type="button"
          className="ws-hv"
          onClick={on.onFit}
          title="Fit graph"
          style={sx(CTL, CTL_START, HOVER.raise())}
        >
          Fit
        </button>
        <button
          type="button"
          className="ws-hv"
          onClick={on.onZoomOut}
          aria-label="Zoom out"
          style={sx(CTL, ICON_CTL, CTL_MID, HOVER.raise())}
        >
          −
        </button>
        <span style={ZOOM_READOUT}>{zoomPct}</span>
        <button
          type="button"
          className="ws-hv"
          onClick={on.onZoomIn}
          aria-label="Zoom in"
          style={sx(CTL, ICON_CTL, CTL_END, HOVER.raise())}
        >
          +
        </button>
      </div>
  )
  /* The one filled primary. `data-primary-action` is what the shell's `.` key
   * presses (`altitude.js` `pressPrimary`). */
  const primary = primaryHidden ? null : (
    <button type="button" className="ws-hv" data-primary-action={primaryQuiet ? undefined : ''} onClick={on.onDiscrepancy} style={primaryQuiet ? sx(CTL, HOVER.raise()) : sx(PRIMARY, HOVER.primary())}>
      {PRIMARY_ACTION}
    </button>
  )
  /* P6-1 — Explore's ONE slim toolbar (design record §3.1): Graph · Timeline ·
   * Table · Find · Focus thread · Walk me through it · zoom, and the one
   * primary. Every button here is the SAME element the full toolbar renders,
   * with the same handler; the two slots are the shell's, and P6-3 made them
   * the thread (Focus thread) and the guided walk, with the scrubber after.
   * Nothing is removed from the full toolbar, which Workbench still draws. */
  if (slim) {
    return (
      <div role="toolbar" aria-label={slimLabel} style={TOOLBAR}>
        {representation}
        {findBox}
        {slots}
        {traceDoc ? scrubber : null}
        {zoom}
        {primary}
        <div style={SPACER} />
        {extra}
      </div>
    )
  }
  return (
    <div role="toolbar" aria-label="Trace tools" style={TOOLBAR}>
      {traceDoc ? (
      <div role="group" aria-label="Focus" style={SEG}>
        <button
          type="button"
          className="ws-hv"
          onClick={() => on.onFocusMode && on.onFocusMode('focus')}
          aria-pressed={inFocus}
          title="Only what is not proven, and its chain; the rest merged into counted group cards"
          style={sx(SEG_BTN, inFocus && SEG_BTN_ON, HOVER.raise())}
        >
          Focus · what's not proven
        </button>
        <button
          type="button"
          className="ws-hv"
          onClick={() => on.onFocusMode && on.onFocusMode('everything')}
          aria-pressed={!inFocus}
          title="The full graph"
          style={sx(SEG_BTN, SEG_DIVIDER, !inFocus && SEG_BTN_ON, HOVER.raise())}
        >
          Everything
        </button>
      </div>
      ) : null}
      {traceDoc && inFocus && focusLine ? <span style={FOCUS_PILL} aria-live="polite">{focusLine}</span> : null}

      {representation}
      {findBox}
      {traceDoc ? (
      <React.Fragment>
      <div role="group" aria-label="Findings" style={GROUP}>
        <button
          type="button"
          className="ws-hv"
          onClick={on.onPrevFinding}
          title="Previous finding ( [ )"
          style={sx(CTL, CTL_TIGHT, CTL_START, HOVER.raise())}
        >
          ◂ Finding
        </button>
        <button
          type="button"
          className="ws-hv"
          onClick={on.onNextFinding}
          title="Next finding ( ] )"
          style={sx(CTL, CTL_TIGHT, CTL_END, HOVER.raise())}
        >
          Finding ▸
        </button>
      </div>

      <button
        type="button"
        className="ws-hv"
        onClick={on.onFindingsOnly}
        aria-pressed={!!findingsOnly}
        style={sx(CTL, findingsOnly && CTL_ON, HOVER.raise())}
      >
        Show findings only
      </button>
      <button type="button" className="ws-hv" onClick={on.onFocusSel} style={sx(CTL, HOVER.raise())}>
        Focus selection
      </button>
      <button
        type="button"
        className="ws-hv"
        onClick={on.onLinked}
        aria-pressed={!!linked}
        style={sx(CTL, linked && CTL_ON, HOVER.raise())}
      >
        Show linked evidence
      </button>

      {zoom}
      <div role="group" aria-label="Detail" style={GROUP}>
        <button
          type="button"
          className="ws-hv"
          onClick={on.onExpand}
          style={sx(CTL, CTL_TIGHT, CTL_START, HOVER.raise())}
        >
          Expand
        </button>
        <button
          type="button"
          className="ws-hv"
          onClick={on.onCollapse}
          style={sx(CTL, CTL_TIGHT, CTL_MID, HOVER.raise())}
        >
          Collapse
        </button>
        <button
          type="button"
          className="ws-hv"
          onClick={on.onWiden}
          title="Widen the selected node (layout only)"
          style={sx(CTL, CTL_TIGHT, CTL_END, HOVER.raise())}
        >
          Widen
        </button>
      </div>
      </React.Fragment>
      ) : null}

      {traceDoc && !compact ? (
      <div role="group" aria-label="Replay" style={GROUP_MID}>
        <button
          type="button"
          className="ws-hv"
          onClick={on.onStepBack}
          aria-label="Step back"
          style={sx(CTL, STEP_CTL, CTL_START, HOVER.raise())}
        >
          |◂
        </button>
        <button
          type="button"
          className="ws-hv"
          onClick={on.onReplay}
          aria-pressed={!!replayPlaying}
          style={sx(CTL, REPLAY_LABEL, replayPlaying && CTL_ON_FLAT, HOVER.raise())}
        >
          {/* N13-GLYPHS-ARIA: ▶ and ❚❚ are for the eye; the words are the name. */}
          {replayGlyph ? <span aria-hidden="true">{replayGlyph} </span> : null}
          {replayLabel}
        </button>
        <button
          type="button"
          className="ws-hv"
          onClick={on.onStepFwd}
          aria-label="Step forward"
          style={sx(CTL, STEP_CTL, CTL_END, HOVER.raise())}
        >
          ▸|
        </button>
      </div>
      ) : null}
      {traceDoc ? scrubber : null}

      {compact ? (
        <button
          type="button"
          className="ws-hv"
          onClick={on.onMoreMenu}
          aria-haspopup="menu"
          style={sx(CTL, HOVER.raise())}
        >
          More ▾
        </button>
      ) : (
        <React.Fragment>
          {/* TRACE-MISSING-SESSION-TRUE: not offered over a session that could not be read. */}
          {on.onJumpStop ? <button type="button" className="ws-hv" onClick={on.onJumpStop} style={sx(CTL, HOVER.raise())}>
            Jump to stop
          </button> : null}
          <button type="button" className="ws-hv" onClick={on.onTestChanges} style={sx(CTL, HOVER.raise())}>
            Show test changes
          </button>
          <button type="button" className="ws-hv" onClick={on.onCompare} style={sx(CTL, HOVER.raise())}>
            Compare sessions
          </button>
        </React.Fragment>
      )}
      {primary}

      <div style={SPACER} />

      {traceDoc && inFocus ? (
        <button
          type="button"
          className="ws-hv"
          onClick={on.onFocusTools}
          aria-haspopup="menu"
          title="Filters, Group by, Columns and Save view — the lens controls, behind one button while in Focus"
          style={sx(CTL, FOCUS_TOOLS, HOVER.raise())}
        >
          Filters · Group by · Columns ▾
        </button>
      ) : null}
      <button
        type="button"
        className="ws-hv"
        onClick={on.onPanelsMenu}
        aria-haspopup="menu"
        style={sx(CTL, HOVER.raise())}
      >
        Panels ▾
      </button>
      <button
        type="button"
        className="ws-hv"
        onClick={on.onFocusCanvas}
        aria-pressed={!!focusCanvas}
        style={sx(CTL, focusCanvas && CTL_ON_FLAT, HOVER.raise())}
      >
        Focus canvas
      </button>
      {compact ? null : (
        <button type="button" className="ws-hv" onClick={on.onResetLayout} style={sx(CTL, HOVER.raise())}>
          Reset layout
        </button>
      )}
    </div>
  )
}

/* ══ FILTER CHIP STRIP (spec-template :495-506) ═════════════════════════════ */

const FILTERBAR = Object.freeze({
  height: `${SIZE.filterBar}px`,
  flex: `0 0 ${SIZE.filterBar}px`,
  display: 'flex',
  alignItems: 'center',
  gap: '6px',
  padding: '0 8px',
  background: C.chrome,
  borderBottom: `1px solid ${C.surf3}`,
  overflow: 'hidden',
})
const FILTERS_CAP = Object.freeze({ fontSize: FS.cap, color: C.inkDim })
const CHIP = Object.freeze({
  height: `${SIZE.chip}px`,
  display: 'inline-flex',
  alignItems: 'center',
  gap: '4px',
  padding: '0 4px 0 7px',
  background: CHIP_SKIN.okBg,
  border: `1px solid ${CHIP_SKIN.okBd}`,
  borderRadius: RADIUS.ctl,
  fontFamily: FONT.mono,
  fontSize: FS.sm,
  color: C.linkHover,
  textDecoration: 'none',
  whiteSpace: 'nowrap',
  flex: '0 0 auto',
})
const CHIP_BAD = Object.freeze({
  background: CHIP_SKIN.badBg,
  border: `1px solid ${CHIP_SKIN.badBd}`,
  color: C.danger,
  textDecoration: 'line-through',
})
/* The flag is amber, not the accusation red: red is reserved for a record
 * finding, and a reader cannot tell a red chip from a red verdict (F-10). */
const CHIP_FLAG_S = Object.freeze({ fontSize: FS.micro, color: C.amber, textDecoration: 'none' })
const FILTER_INPUT = Object.freeze({
  height: `${SIZE.ctlSm}px`,
  flex: '1 1 60px',
  minWidth: 0,
  maxWidth: `${SIZE.filterInputMax}px`,
  boxSizing: 'border-box',
  background: C.fieldBg,
  border: `1px dashed ${C.lineField}`,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  padding: '0 7px',
  fontSize: FS.sm,
  fontFamily: FONT.mono,
})
/* R5: the chip strip is the ONE scrollable region of this bar. Everything to
 * the right of it — the four menus and the match count — is pinned and never
 * shrinks, because the bar is `overflow: hidden` and a pushed-out control is
 * simply gone: at 1280px with two chips, `Save view` and the honest
 * `N of M objects match this lens` denominator were both off the right edge
 * with nothing on screen saying so. */
const FB_SCROLL = Object.freeze({
  flex: '1 1 auto',
  minWidth: 0,
  display: 'flex',
  alignItems: 'center',
  gap: '6px',
  height: '100%',
  overflowX: 'auto',
  overflowY: 'hidden',
})
const FB_BTN = Object.freeze({
  flex: '0 0 auto',
  height: `${SIZE.ctlSm}px`,
  padding: '0 8px',
  background: C.surf2Hi,
  border: `1px solid ${C.lineField}`,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  fontSize: FS.sm,
  cursor: 'pointer',
})
const MATCH_LINE = Object.freeze({
  fontSize: FS.sm,
  color: C.inkDim,
  whiteSpace: 'nowrap',
  fontFamily: FONT.mono,
  minWidth: 0,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  /* R5: the denominator is pinned. It is the sentence that keeps a narrowed
     lens honest, so it may not be the thing that gets pushed off. */
  flex: '0 0 auto',
})

/**
 * The filter chip strip (spec-template :495-506), 30px.
 *
 * An unsupported chip is KEPT and rendered distinctly — a coral tint and a
 * coral border, line-through, an inline flag — and changes nothing about
 * what is shown. It is a visible record of a request the record could not
 * honour, which is Rule 12 behaviour (spec-script §F.3).
 *
 * The flag's text is NOT `unsupported`. That word is a verdict enum wearing a
 * chip label in the accusation red (`lib/voice.data.js:277`; spec-doctrine A8
 * #1, F-10). It reads `not a filter this build reads`, in `C.amber`, and the
 * chip's title becomes `Not a filter this build reads: {why}`.
 *
 * @param {object} props
 * @param {import('./adapter.js').Chip[]} props.chips
 * @param {string} props.filterText
 * @param {string} props.groupByLabel
 * @param {string} props.matchLine e.g. `4 of 28 objects match` — the noun is
 *   never dropped (spec-doctrine §D "counts that are correct")
 * @param {(text: string) => void} props.onFilterText
 * @param {(e: KeyboardEvent) => void} props.onFilterKey Enter splits on
 *   whitespace and adds each token
 * @param {(chipText: string) => void} props.onRemove
 * @param {Record<string, Function>} props.on `onFiltersMenu onGroupMenu
 *   onColumnsMenu onSaveMenu`
 * @returns {JSX.Element}
 */
export function FilterBar({
  /* Step 5: in Focus the four lens buttons sit behind the toolbar's one
   * "Filters · Group by · Columns" button. The chips, the input and the match
   * line stay — a chip the reader added must stay visible and removable — and
   * with no chip the strip is not drawn at all. */
  collapsed = false,
  chips,
  filterText,
  groupByLabel,
  matchLine,
  onFilterText,
  onFilterKey,
  onRemove,
  on,
}) {
  const seen = new Set()
  if (collapsed && !(chips && chips.length)) return null
  return (
    <div style={FILTERBAR}>
      <Kicker spacing={LS.filters} style={FILTERS_CAP}>
        Filters
      </Kicker>

      <div style={FB_SCROLL}>
      {chips.map((c) => (
        <span
          key={uniqueKey(c.text, seen)}
          title={c.ok ? CHIP_OK_TITLE : CHIP_FLAG_TITLE + c.why}
          style={sx(CHIP, !c.ok && CHIP_BAD)}
        >
          <span>{c.text}</span>
          {c.ok ? null : <span style={CHIP_FLAG_S}>{CHIP_FLAG}</span>}
          <button
            type="button"
            className="ws-hv"
            onClick={() => onRemove(c.text)}
            aria-label="Remove filter"
            style={sx(X_BTN, HOVER.ghost())}
          >
            ×
          </button>
        </span>
      ))}

      <input
        value={filterText}
        onChange={(e) => onFilterText(e.target.value)}
        onKeyDown={onFilterKey}
        placeholder={FILTER_PLACEHOLDER}
        aria-label="Add filter"
        style={FILTER_INPUT}
      />
      </div>

      {collapsed ? null : (
      <React.Fragment>
      <button
        type="button"
        className="ws-hv"
        onClick={on.onFiltersMenu}
        aria-haspopup="menu"
        style={sx(FB_BTN, HOVER.chrome())}
      >
        + Facet ▾
      </button>
      <button
        type="button"
        className="ws-hv"
        onClick={on.onGroupMenu}
        aria-haspopup="menu"
        style={sx(FB_BTN, HOVER.chrome())}
      >
        {`Group by · ${groupByLabel} ▾`}
      </button>
      <button
        type="button"
        className="ws-hv"
        onClick={on.onColumnsMenu}
        aria-haspopup="menu"
        style={sx(FB_BTN, HOVER.chrome())}
      >
        Columns ▾
      </button>
      <button
        type="button"
        className="ws-hv"
        onClick={on.onSaveMenu}
        aria-haspopup="menu"
        style={sx(FB_BTN, HOVER.chrome())}
      >
        Save view
      </button>
      </React.Fragment>
      )}

      <span style={MATCH_LINE}>{matchLine}</span>
    </div>
  )
}

/* ══ CONTEXTUAL SENTENCE STRIP (spec-template :509-513) ═════════════════════ */

const SENTENCE_STRIP = Object.freeze({
  height: `${SIZE.sentenceStrip}px`,
  flex: `0 0 ${SIZE.sentenceStrip}px`,
  display: 'flex',
  alignItems: 'center',
  gap: '10px',
  padding: '0 10px',
  background: C.surf1,
  borderBottom: `1px solid ${C.surf3}`,
  overflow: 'hidden',
})
/* A band with nothing in it is not a band. `narr.session[register]` is null for
 * every real session (`/api/narrate` does not exist), so this strip was a
 * permanently empty 26px stripe directly above the canvas on every real
 * reading. Zero sentences and no notice → zero height. */
const SENTENCE_STRIP_EMPTY = Object.freeze({ height: 0, flex: '0 0 0px', padding: 0, border: 0, overflow: 'hidden' })
/* With a notice it grows to hold it, because the notice is prose and a clipped
 * `What would prove it:` clause is a clause that was not read. */
const SENTENCE_STRIP_NOTICE = Object.freeze({
  height: 'auto',
  flex: '0 0 auto',
  alignItems: 'flex-start',
  padding: '7px 10px',
  minHeight: `${SIZE.sentenceStrip}px`,
})
const NOTICE = Object.freeze({ display: 'flex', alignItems: 'flex-start', gap: '10px', flex: 1, minWidth: 0 })
/* The strip's spacer is `flex: 1`, which is right when the left side is a row
 * of short sentences and wrong when it is the promoted finding: two `flex: 1`
 * siblings split the free width in half, so the notice was squeezed to 222px,
 * wrapped to 257px tall, and left a 222px column of nothing beside it — the
 * void the operator photographed, rebuilt one band higher. With a notice the
 * spacer keeps only the gap it exists to hold. */
const NOTICE_SPACER = Object.freeze({ flex: '0 0 12px' })
const NOTICE_BODY = Object.freeze({ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: '2px' })
const NOTICE_HEAD = Object.freeze({ fontSize: FS.base, color: C.inkBright, lineHeight: 1.4 })
const NOTICE_BASIS = Object.freeze({ fontSize: FS.ctl, color: C.inkSoft, lineHeight: 1.45 })
const NOTICE_CLAUSE = Object.freeze({ fontSize: FS.ctl, color: C.inkDim, lineHeight: 1.45 })
const NOTICE_BTN = Object.freeze({
  flex: '0 0 auto',
  height: SIZE.ctl,
  padding: '0 10px',
  background: C.surf3,
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.ctl,
  color: C.inkBright,
  fontSize: FS.ctl,
  cursor: 'pointer',
})
const SENTENCES = Object.freeze({
  display: 'flex',
  alignItems: 'center',
  gap: '6px',
  minWidth: 0,
  overflow: 'hidden',
})
const SENTENCE = Object.freeze({
  fontSize: FS.base,
  color: C.inkBright,
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
})
const IDENT_LINE = Object.freeze({
  fontSize: FS.sm,
  color: C.inkDim,
  fontFamily: FONT.mono,
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
})

/**
 * The 26px contextual sentence strip (spec-template :509-513).
 *
 * The app-authored `sentence` field DOES NOT SHIP. This strip renders the
 * record's own narration pair — `narr.session[register][0]` then `[1]` — and
 * nothing else. A sentence not on the §8.3 whitelist does not exist
 * (BRIEF:1440, spec-doctrine F-11). Narration is rendered VERBATIM including
 * its upstream warts; the app composes, it never writes (BRIEF:1495).
 *
 * @param {object} props
 * @param {string[]} props.sentences the narration pair, in record order
 * @param {string} props.identLine mono, `C.inkDim` — the session/scope identity
 * @returns {JSX.Element}
 */
export function SentenceStrip({ sentences, identLine, notice, onNotice }) {
  const seen = new Set()
  const lines = sentences || []
  /* THE PROMOTED FINDING (DESIGN-readable-trace §5.2). 133 destructive
   * commands in one session is the product's reason to exist, and it sat in a
   * saved-view list at the bottom of a dock with its own label truncated. It
   * belongs on the surface the Trace opens with — this band, which §4.3(c)
   * empties. The saved view stays: promotion adds a surface, it removes none.
   * The row is a statement ABOUT THE RECORD and carries no state word, because
   * the ruling that maps states to words does not rule one for "matched a
   * destructive pattern" and inventing one would be exactly the failure
   * doctrine rule 1 names. It carries its clause instead. */
  if (!lines.length && !notice) return <div style={sx(SENTENCE_STRIP, SENTENCE_STRIP_EMPTY)} />
  return (
    <div style={sx(SENTENCE_STRIP, notice && SENTENCE_STRIP_NOTICE)}>
      {notice ? (
        <div style={NOTICE}>
          <div style={NOTICE_BODY}>
            <div style={NOTICE_HEAD}>{notice.head}</div>
            {notice.basis ? <div style={NOTICE_BASIS}>{notice.basis}</div> : null}
            <div style={NOTICE_CLAUSE}>{notice.clause}</div>
          </div>
          {notice.action ? (
            <button type="button" className="ws-hv" onClick={onNotice} style={sx(NOTICE_BTN, HOVER.raise())}>
              {notice.action}
            </button>
          ) : null}
        </div>
      ) : (
        <span style={SENTENCES}>
          {lines.map((s) => (
            <span key={uniqueKey(s, seen)} style={SENTENCE}>
              {s}
            </span>
          ))}
        </span>
      )}
      <span style={notice ? NOTICE_SPACER : SPACER} />
      <span style={IDENT_LINE}>{identLine}</span>
    </div>
  )
}

/* ══ STATUS STRIP (spec-template :873-882) ══════════════════════════════════ */

const STATUS = Object.freeze({
  height: `${SIZE.statusStrip}px`,
  flex: `0 0 ${SIZE.statusStrip}px`,
  display: 'flex',
  alignItems: 'center',
  gap: '14px',
  padding: '0 10px',
  background: C.chrome,
  borderTop: `1px solid ${C.line1}`,
  fontSize: FS.sm,
  color: C.inkMute,
  fontFamily: FONT.mono,
  overflow: 'hidden',
  whiteSpace: 'nowrap',
})
const STATUS_ORIGIN = Object.freeze({
  color: C.amber,
  minWidth: 0,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  flex: '0 1 auto',
})
const STATUS_PIPE = Object.freeze({ color: C.line2 })
const STATUS_SCOPE = Object.freeze({
  minWidth: 0,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  flex: '0 1 auto',
})
const STATUS_FIXED = Object.freeze({ flex: '0 0 auto' })

/**
 * The 24px status strip (spec-template :873-882), `role="contentinfo"`, mono.
 * `origin · | · scope · | · read · spacer · connection`.
 *
 * The freshness slot has a ruled string and the design's `'read at ' + (readAt
 * || '—')` is not it: `Ledger read {time}` for a legacy export or
 * `Sessions read live` for exec projections (BRIEF:1510, spec-doctrine D-13).
 * No em-dash placeholder anywhere on this strip — the root composes the string,
 * this component only sets it.
 *
 * @param {object} props
 * @param {string} props.origin rendered in `C.amber`; composed from the
 *   `X-ThruLine-Projection` header or the fixture path, NEVER invented
 * @param {string} props.scope
 * @param {string} props.read the ruled freshness string
 * @param {string} props.conn e.g. `loopback only`
 * @returns {JSX.Element}
 */
export function StatusStrip({ origin, scope, read, conn }) {
  return (
    <div role="contentinfo" style={STATUS}>
      <span style={STATUS_ORIGIN}>{origin}</span>
      <span style={STATUS_PIPE}>|</span>
      <span style={STATUS_SCOPE}>{scope}</span>
      <span style={STATUS_PIPE}>|</span>
      <span style={STATUS_FIXED}>{read}</span>
      <span style={SPACER} />
      <span style={STATUS_FIXED}>{conn}</span>
    </div>
  )
}

/* ══ THE GENERIC MENU (spec-template :835-847) ══════════════════════════════ */

const MENU = Object.freeze({
  position: 'absolute',
  zIndex: Z.menu,
  maxWidth: `${SIZE.menuMax}px`,
  maxHeight: '70vh',
  overflow: 'auto',
  background: C.surf2,
  border: `1px solid ${C.linePop}`,
  borderRadius: RADIUS.ctl,
  boxShadow: SHADOW.menu,
  padding: '4px',
})
const MENU_TITLE = Object.freeze({
  padding: '4px 8px 5px',
  borderBottom: `1px solid ${C.line1}`,
  marginBottom: '3px',
})
const MENU_TITLE_TEXT = Object.freeze({ fontSize: FS.cap, color: C.inkDim })
const MENU_SAVE_ROW = Object.freeze({ display: 'flex', gap: '6px', padding: '4px 6px' })
const MENU_SAVE_INPUT = Object.freeze({
  flex: 1,
  height: `${SIZE.ctl}px`,
  background: C.fieldBg,
  border: `1px solid ${C.lineField}`,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  padding: '0 7px',
  fontSize: FS.body,
})
/* Inside a menu that opens over the toolbar's filled primary, so it is the
 * amber outline, not a second fill (§4). */
const MENU_SAVE_BTN = Object.freeze({
  height: `${SIZE.ctl}px`,
  padding: '0 9px',
  ...outlineButton(C.amber),
  borderRadius: RADIUS.ctl,
  fontSize: FS.ctl,
  cursor: 'pointer',
})
const MENU_NOTE = Object.freeze({ padding: '0 8px 6px', fontSize: FS.sm, color: C.inkDim })
const MENU_HEADER = Object.freeze({ padding: '6px 8px 3px' })
const MENU_HEADER_TEXT = Object.freeze({ fontSize: FS.meta, color: C.inkFaint })
const MENU_ITEM = Object.freeze({
  width: '100%',
  display: 'flex',
  alignItems: 'center',
  gap: '8px',
  padding: '5px 8px',
  background: 'transparent',
  border: 0,
  borderRadius: RADIUS.pill,
  color: C.ink,
  fontSize: FS.body,
  cursor: 'pointer',
  textAlign: 'left',
})
const MENU_CHECK = Object.freeze({ width: '12px', color: C.focus, fontSize: FS.sm })
const MENU_LABEL = Object.freeze({ flex: 1 })
const MENU_PROJECT_LABEL = Object.freeze({ flex: 1, minWidth: 0, overflowWrap: 'anywhere' })
const MENU_HINT = Object.freeze({
  font: `${FS.cap} ${FONT.mono}`,
  color: C.inkDim,
  whiteSpace: 'nowrap',
})
const MENU_BACKDROP = Object.freeze({ position: 'absolute', inset: 0, zIndex: Z.menuBackdrop })

/**
 * THE generic menu (spec-template :835-847) — ONE positioned popup serves all
 * eight kinds: project · session · settings · panels · facet · group-by ·
 * columns · save-view. Keeping it as one popup is the faithful port and the
 * least machinery (spec-template Decision D4); splitting it into eight is a
 * change, not a refactor.
 *
 * Positioned from the trigger's `getBoundingClientRect()` measured against the
 * ROOT REF — not `closest('[data-screen-label]')`, which is a Claude-Design
 * editor attribute that does not survive the port (spec-script §E6). The root
 * does that measuring; this component only places itself at `x`/`y`.
 *
 * @param {object} props
 * @param {string|null} props.kind null closes it
 * @param {string} props.title `aria-label` and the title strip
 * @param {number} props.x @param {number} props.y px, relative to the root
 * @param {number} props.width min-width; max 520px, max-height 70vh
 * @param {Array<{header?:string,label?:string,checked?:boolean,check?:string,hint?:string,font?:string,onClick?:Function}>} props.items
 *   one list carries BOTH headers and items; each element renders exactly one
 *   branch (spec-template §B.1 note on 45/46)
 * @param {boolean} props.input true only for the save-view kind
 * @param {{value:string,label:string,placeholder:string,note:string,onChange:Function}} [props.search] optional project search
 * @param {string} props.saveName @param {string} props.inputNote
 * @param {(v: string) => void} props.onSaveName
 * @param {() => void} props.onSaveView
 * @param {() => void} props.onClose the full-bleed backdrop at z-index 65
 * @returns {JSX.Element|null}
 */
export function Menu({
  kind,
  title,
  x,
  y,
  width,
  items,
  input,
  search,
  saveName,
  inputNote,
  onSaveName,
  onSaveView,
  onClose,
}) {
  /* Decision D6, minimum bar: focus goes back to whatever opened this. Escape
   * closes it through the root's global key map, not from here. */
  useReturnFocus(!!kind)
  const searchInput = React.useRef(null)
  React.useEffect(() => {
    if (kind === 'project') searchInput.current?.focus()
  }, [kind])
  if (!kind) return null

  const seen = new Set()
  return (
    <React.Fragment>
      <div
        role="menu"
        aria-label={title}
        style={sx(MENU, { left: `${x}px`, top: `${y}px`, minWidth: `${width}px` })}
      >
        <div style={MENU_TITLE}>
          <Kicker style={MENU_TITLE_TEXT}>{title}</Kicker>
        </div>

        {search ? <>
          <div style={MENU_SAVE_ROW}><input ref={searchInput} type="search" value={search.value} onChange={e => search.onChange(e.target.value)} placeholder={search.placeholder} aria-label={search.label} style={MENU_SAVE_INPUT}/></div>
          <p style={MENU_NOTE}>{search.note}</p>
        </> : null}

        {input ? (
          <React.Fragment>
            <div style={MENU_SAVE_ROW}>
              <input
                value={saveName}
                onChange={(e) => onSaveName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') onSaveView()
                }}
                placeholder="Name this view…"
                aria-label="View name"
                style={MENU_SAVE_INPUT}
              />
              <button type="button" onClick={onSaveView} style={MENU_SAVE_BTN}>
                Save
              </button>
            </div>
            <div style={MENU_NOTE}>{inputNote}</div>
          </React.Fragment>
        ) : null}

        {items.map((it) =>
          it.header ? (
            <div key={uniqueKey(`h:${it.header}`, seen)} style={MENU_HEADER}>
              <Kicker style={MENU_HEADER_TEXT}>{it.header}</Kicker>
            </div>
          ) : it.label ? (
            <button
              key={uniqueKey(`i:${it.label}`, seen)}
              type="button"
              role="menuitem"
              className="ws-hv"
              onClick={it.onClick}
              aria-checked={!!it.checked}
              style={sx(MENU_ITEM, HOVER.chrome())}
            >
              <span style={MENU_CHECK}>{it.check}</span>
              <span style={sx(search ? MENU_PROJECT_LABEL : MENU_LABEL, { fontFamily: it.font || 'inherit' })}>{it.label}</span>
              <span style={MENU_HINT}>{it.hint}</span>
            </button>
          ) : null,
        )}
      </div>
      <div onClick={onClose} style={MENU_BACKDROP} />
    </React.Fragment>
  )
}

/* ══ COMMAND PALETTE (spec-template :850-865) ═══════════════════════════════ */

const PAL_BACKDROP = Object.freeze({
  position: 'absolute',
  inset: 0,
  zIndex: Z.paletteBackdrop,
  background: A.paletteBackdrop,
})
const PAL_DIALOG = Object.freeze({
  position: 'absolute',
  left: '50%',
  top: '60px',
  transform: 'translateX(-50%)',
  width: `${SIZE.paletteW}px`,
  maxWidth: '92%',
  zIndex: Z.palette,
  background: C.surf2,
  border: `1px solid ${C.linePop}`,
  borderRadius: RADIUS.palette,
  boxShadow: SHADOW.palette,
  overflow: 'hidden',
})
const PAL_INPUT = Object.freeze({
  width: '100%',
  boxSizing: 'border-box',
  height: `${SIZE.paletteInput}px`,
  background: C.fieldBg,
  border: 0,
  borderBottom: `1px solid ${C.lineStrong}`,
  color: C.inkBright,
  padding: '0 12px',
  fontSize: FS.lg,
})
const PAL_LIST = Object.freeze({
  maxHeight: `${SIZE.paletteListMaxH}px`,
  overflow: 'auto',
  padding: '4px',
})
const PAL_HEADER = Object.freeze({ padding: '6px 8px 3px' })
const PAL_HEADER_TEXT = Object.freeze({ fontSize: FS.meta, color: C.inkFaint })
const PAL_OPT = Object.freeze({
  width: '100%',
  display: 'flex',
  alignItems: 'center',
  gap: '10px',
  padding: '6px 8px',
  background: 'transparent',
  border: 0,
  borderRadius: RADIUS.pill,
  color: C.ink,
  fontSize: FS.base,
  cursor: 'pointer',
  textAlign: 'left',
})
const PAL_OPT_ON = Object.freeze({ background: ACTIVE_ROW })
const PAL_KIND = Object.freeze({
  font: `${FS.meta} ${FONT.mono}`,
  color: C.inkDim,
  width: '64px',
  flex: '0 0 64px',
})
const PAL_HINT = Object.freeze({
  font: `${FS.cap} ${FONT.mono}`,
  color: C.inkFaint,
  whiteSpace: 'nowrap',
})
const PAL_EMPTY = Object.freeze({ padding: '10px 8px', fontSize: FS.body, color: C.inkDim })
const PAL_FOOT = Object.freeze({
  padding: '5px 10px',
  borderTop: `1px solid ${C.line1}`,
  fontSize: FS.cap,
  color: C.inkFaint,
  display: 'flex',
  gap: '12px',
})

/**
 * The ⌘K command palette (spec-template :850-865), 640px, `role="dialog"`.
 *
 * Sections in fixed order: **go to** (the 15 rail destinations) · **view** ·
 * **action** · **object** · **filter** · **session** · **command** · **recent**.
 *
 * THE INDEX SEPARATION IS LOAD-BEARING (spec-script §D.2, §B.14): the list
 * interleaves headers with items, and the active index counts ITEMS ONLY. The
 * root marks the active item with `active: true`; this component never derives
 * an index, so arrow keys can never land on a header.
 *
 * The footer is verbatim: `↑↓ move` · `↵ run` · `esc close` · spacer ·
 * `Copying does not execute.` That last sentence is doctrine, not flavour
 * (PORT-CONTEXT §8) — nothing on this screen executes the agent's work.
 *
 * @param {object} props
 * @param {boolean} props.open
 * @param {string} props.query
 * @param {Array<{header?:string,label?:string,kind?:string,hint?:string,font?:string,active?:boolean,onClick?:Function,onEnter?:Function}>} props.items
 * @param {string} props.empty the no-results sentence; renders only when set
 * @param {(v: string) => void} props.onInput
 * @param {(e: KeyboardEvent) => void} props.onKey
 * @param {() => void} props.onClose
 * @param {React.RefObject<HTMLInputElement>} props.inputRef focused on open
 * @returns {JSX.Element|null}
 */
export function Palette({ open, query, items, empty, onInput, onKey, onClose, inputRef }) {
  useReturnFocus(open)
  if (!open) return null

  const seen = new Set()
  return (
    <React.Fragment>
      <div onClick={onClose} style={PAL_BACKDROP} />
      <div role="dialog" aria-label="Command palette" style={PAL_DIALOG}>
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => onInput(e.target.value)}
          onKeyDown={onKey}
          placeholder={PALETTE_PLACEHOLDER}
          aria-label="Command palette"
          style={PAL_INPUT}
        />
        <div role="listbox" style={PAL_LIST}>
          {items.map((it) =>
            it.header ? (
              <div key={uniqueKey(`h:${it.header}`, seen)} style={PAL_HEADER}>
                <Kicker style={PAL_HEADER_TEXT}>{it.header}</Kicker>
              </div>
            ) : it.label ? (
              <button
                key={uniqueKey(`i:${it.kind || ''}:${it.label}`, seen)}
                type="button"
                role="option"
                aria-selected={!!it.active}
                className="ws-hv"
                onClick={it.onClick}
                onMouseEnter={it.onEnter}
                style={sx(PAL_OPT, it.active && PAL_OPT_ON, HOVER.chrome())}
              >
                <span style={PAL_KIND}>{it.kind}</span>
                <span style={sx(ELLIPSIS, { fontFamily: it.font || 'inherit' })}>{it.label}</span>
                <span style={PAL_HINT}>{it.hint}</span>
              </button>
            ) : null,
          )}
          {empty ? <div style={PAL_EMPTY}>{empty}</div> : null}
        </div>
        <div style={PAL_FOOT}>
          <span>↑↓ move</span>
          <span>↵ run</span>
          <span>esc close</span>
          <span style={SPACER} />
          <span>{COPY_NOTE}</span>
        </div>
      </div>
    </React.Fragment>
  )
}

/* ══ TOAST (spec-template :868-870) ═════════════════════════════════════════ */

const TOAST = Object.freeze({
  position: 'absolute',
  left: '50%',
  bottom: '14px',
  transform: 'translateX(-50%)',
  zIndex: Z.toast,
  background: C.surf3,
  border: `1px solid ${C.linePop}`,
  borderRadius: RADIUS.ctl,
  padding: '6px 12px',
  fontSize: FS.body,
  color: C.inkBright,
  boxShadow: SHADOW.toast,
  maxWidth: '70%',
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
})

/**
 * The single toast (spec-template :868-870). `role="status"`,
 * `aria-live="polite"`, one line, ellipsised, 2600ms.
 *
 * One toast, no queue, no severity, no control to close it — that is the design
 * and the port keeps it (spec-template §I.15). The timer belongs to the root,
 * which clears it on unmount; the original leaks it (spec-script §E6).
 *
 * @param {object} props
 * @param {string|null} props.text
 * @returns {JSX.Element|null}
 */
export function Toast({ text }) {
  if (!text) return null
  return (
    <div role="status" aria-live="polite" style={TOAST}>
      {text}
    </div>
  )
}
