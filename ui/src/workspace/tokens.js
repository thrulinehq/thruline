/* ThruLine workspace — the token module.
 *
 * THE ONLY PLACE a colour, size, radius, shadow, z-index or grid template may
 * be written as a literal. Every other file in `src/workspace/` imports from
 * here, and `workspace.css` reads the same values as CSS custom properties
 * (`--ws-*`) from the one block `cssVarBlock()` below prints — a test
 * (`visual-system.test.js`) holds that block byte-equal to this module, so a
 * stylesheet rule and an inline `sx()` style cannot disagree about a colour.
 *
 * P6-6 (design record 2026-09-22 §4, accepted as UI direction R-2026-09-22-3)
 * is the authorised change of every hex below: the workspace moved from the
 * Live Trace spec-template's grey palette to the §4 true-black system. The old
 * names in `C` are KEPT so their ~1,000 consumers keep compiling, but each now
 * points at a §4 value; where an old name has no §4 meaning it is mapped to
 * the nearest §4 surface or text token and the comment says so. There is one
 * palette, not two: no pre-P6-6 hex survives in `C`.
 *
 * The base stylesheet lives in `./workspace.css`. Everything else is a style
 * object built with `sx()`.
 */

/* ── THE §4 PALETTE. The only colour literals in the workspace. ──────────
 *
 * Contrast, measured by `contrast.test.js` (WCAG 2.x), on #000000 / #0A0A0B /
 * #111113:  text 17.96 / 16.93 / 16.13 · text2 8.19 / 7.72 / 7.36 ·
 * text3 3.99 / 3.76 / 3.58. `text3` (= `neutral`) FAILS 4.5:1 as body text on
 * every surface, so it is restricted to large text, glyphs, outlines and the
 * dashed "not recorded" border — never body text. Every meaning hue passes
 * 4.5:1 as text on all three surfaces (red is the lowest, 4.82 on #111113). */
export const PALETTE = Object.freeze({
  app: '#000000', //            app background
  panel: '#0A0A0B', //          panels
  raised: '#111113', //         raised cards, sheets
  hover: '#16161A', //          hover
  hairline: '#1F1F23', //       hairlines
  hairlineStrong: '#2A2A30', // strong dividers
  text: '#EDEDEF', //           text primary
  text2: '#A1A1AA', //          text secondary
  text3: '#6B6B75', //          text tertiary — large text / non-text only
  amber: '#F5A524', //          brand, primary action, lit thread
  green: '#3DD68C', //          proven, evidence that applies
  coral: '#FF6369', //          not proven, evidence gap
  yellow: '#F2C94C', //         recorded, stale, caution
  blue: '#52A8FF', //           request, in progress, live
  violet: '#B78CFF', //         agent's words
  cyan: '#4CC9F0', //           commands and execution
  neutral: '#6B6B75', //        not recorded — always drawn dashed
  red: '#E5484D', //            destructive
})
const P = PALETTE

/* The meaning hues, by name, for loops and tests. */
export const HUES = Object.freeze(['amber', 'green', 'coral', 'yellow', 'blue', 'violet', 'cyan', 'neutral', 'red'])

/* A 10 % tint of a meaning hue: the hover ground of an outline button and the
 * ground of a meaning-marked block. Derived, never a new hex. */
export function tint(hue, pct = 10) {
  return `color-mix(in srgb, ${hue} ${pct}%, transparent)`
}

/* The focus ring. Blue passes 3:1 against #000000 (8.40) and #111113 (7.55),
 * and is not the amber of the primary button, so a focused primary still shows
 * a ring distinct from its own face. */
export const FOCUS = Object.freeze({ color: P.blue, width: '2px', offset: '2px' })

/* The one glow the system allows: the lit thread (§4). Defined here for P6-3;
 * nothing in P6-6 paints it. */
export const GLOW = Object.freeze({ thread: `0 0 10px ${tint(P.amber, 45)}` })

/* ── The pre-P6-6 names, re-pointed. Each old name keeps its role; the value
 *    is the §4 token for that role. "nearest" marks a name with no §4 meaning
 *    of its own, mapped to the nearest §4 surface or text token. ── */
export const C = Object.freeze({
  ink: P.text, //                default body / control text
  inkDim: P.text2, //            secondary labels, captions, "×" glyphs
  surf3: P.raised, //            nearest · toolbar button face; rail hover
  line2: P.hairlineStrong, //    button borders; scrollbar thumb
  surf3Hi: P.hover, //           H1 hover
  inkBright: P.text, //          emphasised text
  inkMute: P.text2, //           nearest · tertiary notes — text3 fails body contrast
  line1: P.hairline, //          panel dividers
  lineField: P.hairlineStrong, //input / chrome-button borders
  inkFaint: P.text2, //          nearest · footnotes — text3 fails body contrast
  surf2: P.raised, //            popover + node + tablist ground
  surf2Hi: P.hover, //           nearest · chrome button face; H4 hover
  surfHoverMenu: P.hover, //     H2 hover
  lineRow: P.hairline, //        row hairline inside documents
  bgApp: P.app, //               app ground, <code>/<pre> ground
  surf1: P.panel, //             docks, toolbar, sticky header ground
  fieldBg: P.app, //             nearest · inputs sit darker than their panel
  accent: P.hairlineStrong, //   nearest · a pressed toggle / ::selection; NOT the primary face (that is BTN.primary)
  chrome: P.panel, //            app bar, tab strip, filter bar, status strip
  amber: P.amber, //             brand mark, demo pill, caveats
  lineStrong: P.hairlineStrong, //dock/panel header underlines
  inkSoft: P.text2, //           pin labels, table detail, group caption
  linePop: P.hairlineStrong, //  popover / toast borders
  focus: FOCUS.color, //         :focus-visible, minimap viewport, menu check
  canvas: P.app, //              canvas ground + wire-label halo
  accentBd: P.text3, //          nearest · a pressed toggle's border (non-text, 3.99:1 on black)
  link: P.blue, //               <a>, request-coloured
  amberBd: P.amber, //           demo-pill outline
  lineFaint: P.hairline, //      zoom readout ground, receipt row hairline
  accentHi: P.hover, //          nearest · H10, the old primary hover (primaries now use HOVER.primary)
  linkHover: P.text, //          a:hover
  lineFieldHi: P.text3, //       H11 search-field hover border (non-text)
  danger: P.coral, //            "why it is not proven", a read that failed
  wireRecorded: P.text, //       legend "recorded order"
  wireInferred: P.violet, //     legend "inferred (by text or order)" — the agent's words
  surfInset: P.panel, //         inspector finding button ground
  inkCode: P.text, //            raw <pre> text
  white: P.text, //              node group-toggle text
  green: P.green, //             Proven / Kept
  greyWord: P.neutral, //        Fog — not recorded (glyph/outline only; its word is inked text2)
  coral: P.coral,
  yellow: P.yellow,
  blue: P.blue,
  violet: P.violet,
  cyan: P.cyan,
  neutral: P.neutral,
  red: P.red,
  text3: P.text3,
})

/* Alpha values. Black-alpha shadows and scrims stay rgba (they are a depth,
 * not a colour); every tinted alpha is derived from a §4 token. */
export const A = Object.freeze({
  nodeToggle: 'rgba(0,0,0,.3)',
  nodeHeaderUnderline: 'rgba(0,0,0,.35)',
  drawerShadow: 'rgba(0,0,0,.45)',
  tooltipShadow: 'rgba(0,0,0,.5)',
  findShadow: 'rgba(0,0,0,.55)',
  menuShadow: 'rgba(0,0,0,.6)',
  paletteShadow: 'rgba(0,0,0,.65)',
  grid: 'rgba(255,255,255,.035)',
  frameFill: 'rgba(255,255,255,.05)',
  frameHeader: 'rgba(255,255,255,.08)',
  frameHeaderUnderline: 'rgba(255,255,255,.12)',
  minimapFrameStroke: 'rgba(255,255,255,.14)',
  nodeToggleBorder: 'rgba(255,255,255,.15)',
  frameBorder: 'rgba(255,255,255,.17)',
  noteGround: `color-mix(in srgb, ${P.app} 85%, transparent)`,
  minimapGround: `color-mix(in srgb, ${P.app} 90%, transparent)`,
  minimapViewport: tint(FOCUS.color),
  paletteBackdrop: 'rgba(0,0,0,.55)',
})

/* ── Buttons (§4). One filled amber button per view; every meaning-coloured
 *    button is an outline in its hue with a 10 % tint on hover; destructive is
 *    a red outline. No gradient, no glass, no glow. ── */
export const BTN = Object.freeze({
  primary: Object.freeze({ background: P.amber, color: P.app, border: `1px solid ${P.amber}`, fontWeight: 600 }),
  /* The primary's hover: a step toward the text colour, never a glow. */
  primaryHover: `color-mix(in srgb, ${P.amber} 85%, ${P.text})`,
  secondary: Object.freeze({ background: 'transparent', color: P.text, border: `1px solid ${P.hairlineStrong}` }),
  destructive: Object.freeze({ background: 'transparent', color: P.red, border: `1px solid ${P.red}` }),
  /* Red on its own 10 % tint over #111113 measures 4.40:1 — below body
   * contrast — so a hovered destructive button inks its label in text. */
  destructiveHoverInk: P.text,
})

/** A meaning-coloured outline button in `hue`. Hover with `HOVER.outline(hue)`. */
export function outlineButton(hue) {
  return { background: 'transparent', color: hue, border: `1px solid ${hue}` }
}

/* Motion (§4): one job — causality and arrival. Two durations, one easing,
 * and nothing loops, idles or celebrates. `prefers-reduced-motion: reduce`
 * collapses every one of them to none (workspace.css). */
export const MOTION = Object.freeze({ quick: '240ms', slow: '320ms', ease: 'ease-out' })

/* Breakpoints (§6). Below `sheet` the inspector becomes a bottom sheet; at
 * `narrow` nothing scrolls sideways and the work name stays in the top bar.
 * CSS media queries cannot read a custom property, so `visual-system.test.js`
 * holds every `@media (max-width: …)` in workspace.css to these values or to a
 * named legacy exception. */
export const BP = Object.freeze({ sheet: 1100, narrow: 800 })

/* Type. Only two families exist in the design (spec-template §A).
 *
 * spec-target Decision 1 is RULED: IBM Plex ships. The 21 unicode-range woff2
 * subsets and their `@font-face` block are in `ui/public/fonts` (see
 * `ATTRIBUTION.md`), `index.html` links `/fonts/plex.css`, the vite allowlist
 * (`vite.config.js:38`) copies the directory into `dist`, and `build.rs` embeds
 * it — self-hosted, loopback only, no CDN and no font URL.
 *
 * Each family keeps the shipped system stack behind it, so a subset that fails
 * to load, or a glyph Plex does not carry, falls back to `macos.css:56-57`
 * rather than to a browser default. These two strings are the whole of the
 * change, and the families reach only the workspace: `os/shell.jsx` re-bases
 * its own subtree at `os.css:23-31` (`.os-root { font: 13px/1.4 var(--sans) }`),
 * and `/acceptance`, `?cast` and the Trace stack are never inside `.ws-root`. */
export const FONT = Object.freeze({
  /* P6-6: the `var()` carries its own fallback stack. The Change screen, the
     packet and the Inbox are not inside `.ws-root`, and on a page where
     `--sans` is not defined a bare `var(--sans)` makes the whole `font`
     declaration invalid — the screen then renders in the browser's serif. */
  sans: "'IBM Plex Sans', var(--sans, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif)",
  mono: "'IBM Plex Mono', var(--mono, ui-monospace, monospace)",
})

/* Font sizes.
 *
 * §4: Plain is a humanist sans (IBM Plex Sans, self-hosted — no network font)
 * and never below 14px; a headline is 22–28px; Technical is IBM Plex Mono.
 *
 * P6-6 raised the reading sizes (`base` 12.5 → 14, `lg` 13 → 15, `title`
 * 14 → 16, `docTitle` 17 → 22). The five dense-chrome sizes below 14px —
 * `micro meta cap sm ctl body` — are the workspace shell's toolbar, dock,
 * table and canvas-node sizes; the shell is rebuilt by P6-1 and the Trace by
 * P6-3, so they are NOT raised here, and every site that still uses one on a
 * Plain surface is listed in docs/loop/reports/p6/P6-6-coder.md. The `plain*`
 * and `headline*` sizes are the scale the Plain screens (Change, packet,
 * Inbox, first use) use, through the `--ws-fs-*` custom properties. */
export const FS = Object.freeze({
  micro: '9px', //    Technical only
  meta: '10px', //    Technical only
  cap: '10.5px', //   uppercase captions, letter-spacing .06em — Technical only
  sm: '11px', //      Technical only
  ctl: '11.5px', //   every toolbar control (dense chrome, P6-1)
  body: '12px', //    dense chrome body (P6-1)
  base: '14px', //    the workspace base: body + contextual sentence
  lg: '15px',
  title: '16px',
  docTitle: '22px', // claim document headline
  plain: '14px', //   the Plain floor: captions, secondary lines
  plainBody: '15px', // Plain body
  plainHeading: '17px', // a section heading on a Plain screen
  plainLead: '18px', // the one-sentence conclusion under a headline
  headline: '26px', // a Plain screen's headline
  headlineNarrow: '22px', // the same headline at BP.narrow and below
})

/* The Plain floor, as a number, for tests. */
export const PLAIN_MIN_PX = 14

export const LS = Object.freeze({
  cap: '.06em', // 19 sites — uppercase section captions
  frame: '.08em', // frame title, "authored" kicker
  nodeType: '.09em',
  rail: '.1em',
  filters: '.05em',
})

/* Sizing (spec-template §E.3). */
export const SIZE = Object.freeze({
  appBar: 44,
  docTabs: 32,
  docTab: 27,
  toolbarMin: 34,
  filterBar: 30,
  sentenceStrip: 26,
  statusStrip: 24,
  paneHeader: 28,
  subHeader: 26,
  stickyHeader: 26,
  railItem: 34,
  railCaption: 16,
  rail: 48,
  splitter: 5,
  ctl: 24, // the canonical control height (44 sites)
  ctlBar: 26, // app-bar controls
  ctlSm: 22, // filter-bar controls, "Show proof"
  chip: 20,
  nodeToggle: 18,
  paletteInput: 38,
  searchMax: 560,
  findInput: 190,
  findListbox: 360,
  findListboxMaxH: 260,
  filterInputMax: 420,
  docTabMax: 260,
  menuMax: 520,
  paletteW: 640,
  paletteListMaxH: 380,
  minimapW: 170,
  minimapH: 108,
  wireSvgW: 3000,
  wireSvgH: 2400,
  commandListMaxH: 220,
  savedViewsMaxH: '38%',
  inspectorIdMax: 170,
  canvasNoteMax: 420,
  leftMin: 160,
  rightMin: 280,
  bottomMin: 120,
})

/* Drag/keyboard resize clamps (spec-script §B.8 `handleMove`). */
export const CLAMP = Object.freeze({
  left: [160, 420],
  right: [280, 560],
  bottom: [120, 480],
  zoom: [0.25, 2.5],
  keyStep: 16, // splitter arrow-key step
  nodeStep: 10, // node arrow-key nudge; 1 with Shift
})

export const RADIUS = Object.freeze({
  ctl: '3px', // 54 sites
  pill: '2px', // 9 sites
  palette: '4px',
  round: '50%',
  segStart: '3px 0 0 3px',
  segEnd: '0 3px 3px 0',
  tab: '3px 3px 0 0',
  nodeHeader: '2px 2px 0 0',
})

export const SHADOW = Object.freeze({
  tooltip: `0 4px 14px ${A.tooltipShadow}`,
  findListbox: `0 8px 24px ${A.findShadow}`,
  drawer: `8px 0 24px ${A.drawerShadow}`,
  menu: `0 10px 30px ${A.menuShadow}`,
  palette: `0 18px 50px ${A.paletteShadow}`,
  toast: `0 6px 20px ${A.tooltipShadow}`,
})

/* The overlay ladder, lowest to highest (spec-template §A). */
export const Z = Object.freeze({
  dock: 20,
  drawer: 30,
  findListbox: 40,
  railTip: 60,
  menuBackdrop: 65,
  menu: 70,
  paletteBackdrop: 80,
  palette: 90,
  toast: 95,
})

/* Grid templates — these ARE the column contracts (spec-template §E.3). A
 * header and its rows must use the same constant or the columns drift. */
export const GRID = Object.freeze({
  timelineRow: '24px 64px 120px minmax(0,1fr) 150px',
  /* R4: the flexible column carries a FLOOR. With `minmax(0,1fr)` it reached
     zero width in a 1280px window and its header text overflowed onto the two
     columns beside it. Below the floor the grid overflows its pane, which
     scrolls — a scrollbar is legible, overlapping text is not. */
  findings: '150px 104px 88px minmax(160px,1fr) 156px 100px',
  events: '24px 56px 120px minmax(0,1fr) 200px',
  checks: '70px minmax(0,1fr) 160px 200px 200px 60px',
  promises: '110px minmax(0,1fr) minmax(0,1fr) 220px',
  receipt: '20px 170px minmax(0,1fr)',
  compare: '150px minmax(0,1fr) minmax(0,1fr)',
  inspectorKv: '104px minmax(0,1fr)',
  claimDocPage: 'minmax(0,1.1fr) minmax(0,1fr)',
  claimDocEvidence: '132px minmax(0,1fr)',
  claimDocFinding: '140px 150px minmax(0,1fr)',
  inboxRow: '170px 1fr',
})

/**
 * THE STYLE-OBJECT CONVENTION.
 *
 * Every element's style is built by `sx(...)` over frozen constants declared at
 * module top level — never by an object literal written inline in JSX, because
 * a literal allocates on every render and cannot be shared between a sticky
 * header and the rows it must line up with.
 *
 *   const ROW = { display: 'grid', gridTemplateColumns: GRID.findings, ... }
 *   <div className="ws-hv" style={sx(ROW, sel && ROW_SEL, hv(C.surf2))} />
 *
 * Falsy parts are skipped, so `cond && STYLE` is the idiom for a conditional
 * layer. Later parts win. Never mutate a part.
 *
 * @param {...(object|false|null|undefined)} parts style fragments, in order
 * @returns {object} a new React style object
 */
export function sx(...parts) {
  const out = {}
  for (const p of parts) if (p) Object.assign(out, p)
  return out
}

/**
 * THE HOVER HELPER — the replacement for the design's 73 `style-hover`
 * attributes over 12 distinct values (spec-template §D).
 *
 * `workspace.css` carries exactly one rule:
 *   .ws-hv:hover { background: var(--hv-bg, inherit); color: var(--hv-fg, inherit); border-color: var(--hv-bd, inherit) }
 * so every site keeps its exact value with no per-element subscription — which
 * matters because the graph canvas renders hundreds of nodes and 73 `useHover`
 * hooks on that tree is a re-render storm.
 *
 * Usage: `<button className="ws-hv" style={sx(BTN, hv(C.surf3Hi))}>`
 *
 * @param {string} [bg] hover background — a `C`/`PALETTE` token or `tint(hue)`,
 *   never a literal (the pre-P6-6 hexes these used to name are retired)
 * @param {string} [fg] hover colour
 * @param {string} [bd] hover border-colour
 * @returns {object} CSS custom properties to merge into a style object
 */
export function hv(bg, fg, bd) {
  const out = {}
  if (bg) out['--hv-bg'] = bg
  if (fg) out['--hv-fg'] = fg
  if (bd) out['--hv-bd'] = bd
  return out
}

/* The twelve hover treatments, named so a porter cites a treatment rather than
 * re-deriving a hex (spec-template §D). */
export const HOVER = Object.freeze({
  raise: () => hv(C.surf3Hi), //            H1 · 32 toolbar/secondary controls
  chrome: () => hv(C.surfHoverMenu), //     H2 · 11 app-bar + menu/palette rows
  dataRow: () => hv(C.surf2), //            H3 · 8 data rows
  treeTab: () => hv(C.surf2Hi), //          H4 · 4 tree rows / doc tabs
  listItem: () => hv(C.surf3), //           H5 · 2 dock list rows
  ghost: () => hv(undefined, C.inkBright), //H6 · 7 icon-only / "×" / tab text
  railItem: () => hv(C.surf3, C.inkBright), //H7 · 1 rail destination
  railCaption: () => hv(undefined, C.inkMute), //H8 · 1 rail group caption
  separator: () => hv(C.accent), //         H9 · 3 resize separators
  primary: () => hv(BTN.primaryHover), //   H10 · the one filled amber button
  outline: (hue) => hv(tint(hue)), //       a meaning-coloured outline button: 10 % tint
  field: () => hv(undefined, undefined, C.lineFieldHi), //H11 · 1 search field
  overlayCtl: () => hv(A.tooltipShadow), // H12 · 1 node-header toggle
})

/* ── CSS custom properties ──────────────────────────────────────────────
 * The same values, for `workspace.css`. `cssVarBlock()` prints the block that
 * sits between the `@tokens` markers at the top of that sheet; the test holds
 * the two byte-equal, so a hex cannot be edited in one and not the other. To
 * regenerate after a token change:
 *   node -e "import('./src/workspace/tokens.js').then(m => console.log(m.cssVarBlock()))"
 */
const kebab = (k) => k.replace(/([a-z])([A-Z0-9])/g, '$1-$2').toLowerCase()

export const CSS_VARS = Object.freeze({
  ...Object.fromEntries(Object.entries(PALETTE).map(([k, v]) => [`--ws-${kebab(k)}`, v])),
  ...Object.fromEntries(HUES.map((h) => [`--ws-${h}-tint`, tint(`var(--ws-${h})`)])),
  '--ws-focus': FOCUS.color,
  '--ws-focus-width': FOCUS.width,
  '--ws-focus-offset': FOCUS.offset,
  '--ws-glow-thread': `0 0 10px ${tint('var(--ws-amber)', 45)}`,
  '--ws-primary-hover': 'color-mix(in srgb, var(--ws-amber) 85%, var(--ws-text))',
  '--ws-shadow': A.menuShadow,
  '--ws-scrim': A.paletteShadow,
  '--ws-font-sans': FONT.sans,
  '--ws-font-mono': FONT.mono,
  ...Object.fromEntries(Object.entries(FS).map(([k, v]) => [`--ws-fs-${kebab(k)}`, v])),
  '--ws-motion-quick': MOTION.quick,
  '--ws-motion-slow': MOTION.slow,
  '--ws-ease': MOTION.ease,
})

/** The `@tokens` block of workspace.css, exactly. */
export function cssVarBlock() {
  const lines = Object.entries(CSS_VARS).map(([k, v]) => `  ${k}: ${v};`)
  return ['/* @tokens:begin — generated from tokens.js by cssVarBlock(); do not edit by hand */', 'body, .ws-root, .ws-change, .ws-inbox {', ...lines, '}', '/* @tokens:end */'].join('\n')
}
