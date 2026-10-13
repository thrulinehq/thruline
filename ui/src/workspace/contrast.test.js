/* P6-6 — contrast, measured on black.
 *
 * The design record (§8) says contrast on black was never verified on the
 * prototype. This file verifies it for the tokens the product ships: a pure
 * WCAG 2.x relative-luminance / contrast-ratio function (no dependency), and a
 * table of every text token against every surface it is painted on.
 *
 * Thresholds (WCAG 2.x): 4.5:1 body text, 3:1 large text (≥ 24px, or ≥ 18.66px
 * bold), 3:1 user-interface components and focus indicators (SC 1.4.11).
 *
 * Print the whole table for pasting:
 *   THRULINE_CONTRAST_TABLE=1 node --test src/workspace/contrast.test.js
 */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { A, BTN, C, CSS_VARS, FOCUS, GLOW, HUES, PALETTE } from './tokens.js'
import { FAM, WORD, wordOf } from './logic.js'
import { EDGE_STYLE } from './trace-view.js'

/* ── the function ─────────────────────────────────────────────────────── */

/** `#RRGGBB` → [r, g, b] in 0..255. */
export function rgb(hex) {
  const m = /^#([0-9a-f]{6})$/i.exec(hex)
  if (!m) throw new Error(`not a #RRGGBB colour: ${hex}`)
  return [0, 2, 4].map((i) => parseInt(m[1].slice(i, i + 2), 16))
}

/** WCAG 2.x relative luminance of an sRGB colour. */
export function luminance(hex) {
  const [r, g, b] = rgb(hex).map((v) => {
    const c = v / 255
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
  })
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

/** WCAG 2.x contrast ratio, 1..21. */
export function contrast(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (hi + 0.05) / (lo + 0.05)
}

/** `color-mix(in srgb, fg pct%, bg)` — how a tint paints over a surface. */
export function mix(fg, pct, bg) {
  const f = rgb(fg)
  const g = rgb(bg)
  const out = f.map((v, i) => Math.round((v * pct + g[i] * (100 - pct)) / 100))
  return `#${out.map((v) => v.toString(16).padStart(2, '0')).join('')}`
}

test('the contrast function reproduces the WCAG reference points', () => {
  assert.equal(contrast('#000000', '#FFFFFF').toFixed(2), '21.00')
  assert.equal(contrast('#FFFFFF', '#FFFFFF').toFixed(2), '1.00')
  // #767676 on white is the canonical "just passes 4.5:1" grey.
  assert.ok(contrast('#767676', '#FFFFFF') >= 4.5)
  assert.ok(contrast('#777777', '#FFFFFF') < 4.5)
  assert.equal(contrast('#000000', '#FFFFFF'), contrast('#FFFFFF', '#000000'), 'symmetric')
  assert.equal(mix('#FFFFFF', 10, '#000000'), '#1a1a1a')
})

/* ── the table ────────────────────────────────────────────────────────── */

const P = PALETTE
const SURFACES = Object.freeze({ app: P.app, panel: P.panel, raised: P.raised, hover: P.hover })
const BODY = 4.5
const LARGE = 3

/* Every pair the product paints, with the threshold that applies to it. A row
 * is a claim about a real use: `use` says where. */
const ROWS = []
const row = (fg, fgName, bg, bgName, min, use, exception) => ROWS.push({ fg, fgName, bg, bgName, min, use, exception: exception || null, ratio: contrast(fg, bg) })

for (const [bgName, bg] of Object.entries(SURFACES)) {
  row(P.text, 'text', bg, bgName, BODY, 'body text')
  row(P.text2, 'text2', bg, bgName, BODY, 'secondary body text')
  // text3 is restricted: large text, glyphs, outlines, dashed "not recorded".
  row(P.text3, 'text3', bg, bgName, LARGE, 'large text / non-text only')
  row(FOCUS.color, 'focus', bg, bgName, LARGE, 'focus ring (SC 1.4.11)')
  row(P.neutral, 'neutral', bg, bgName, LARGE, 'dashed "not recorded" outline, Fog glyph')
  row(P.red, 'red', bg, bgName, BODY, 'destructive outline button label')
}
for (const [bgName, bg] of Object.entries({ app: P.app, panel: P.panel, raised: P.raised })) {
  for (const h of HUES) {
    if (h === 'neutral' || h === 'red') continue
    row(P[h], h, bg, bgName, BODY, 'meaning hue as text')
  }
}
// The filled primary: black ink on amber, and on amber's hover step.
row(BTN.primary.color, 'app', BTN.primary.background, 'amber', BODY, 'primary button label')
row(P.app, 'app', mix(P.amber, 85, P.text), 'amber-hover', BODY, 'primary button label, hovered')
// A meaning-coloured outline button: the hue on its own 10 % tint over each surface.
for (const [bgName, bg] of Object.entries({ app: P.app, panel: P.panel, raised: P.raised })) {
  for (const h of HUES) {
    if (h === 'neutral') continue
    // Red on its own tint over #111113 is 4.40: a hovered destructive button inks its label in text instead.
    if (h === 'red') row(BTN.destructiveHoverInk, 'text', mix(P.red, 10, bg), `red-tint/${bgName}`, BODY, 'destructive button hovered (label in text)')
    else row(P[h], h, mix(P[h], 10, bg), `${h}-tint/${bgName}`, BODY, 'outline button hovered, tinted block')
  }
}
// State-word chips: dark ink on the hue, where the chip is filled.
for (const slug of Object.keys(WORD)) {
  const w = wordOf(slug)
  if (w.ink === w.color) row(C.bgApp, 'app', w.color, `chip:${slug}`, BODY, `filled "${w.word}" chip`)
  for (const [bgName, bg] of Object.entries({ app: P.app, panel: P.panel, raised: P.raised })) {
    row(w.ink, `ink:${slug}`, bg, bgName, BODY, `"${w.word}" as text`)
    row(w.color, `hue:${slug}`, bg, bgName, LARGE, `"${w.word}" glyph / outline`)
  }
}


/* ── T-6-4 · C — every pair the three P6 cards paint, read off their source ──
 *
 * P6-6 laid the token table above. P6-1 (top bar, sidebar, segmented groups,
 * slots, the two sheets, Brief's chip) and P6-3 (lane headers, thread bar,
 * legend, card faces incl. hollow and band, the Outline) and the dock tabs
 * paint their own pairs. Each row below is READ OFF THE SOURCE — the CSS rule
 * or the inline style object that paints it — not typed in, so a rule that
 * changes its colour changes the measurement. A pair below 4.5:1 (text) or
 * 3:1 (a UI component, a state marker, a focus ring, a line that carries
 * meaning) fails, unless it is one of the NAMED EXCEPTIONS, each of which has
 * its own restricting test further down.
 */

const P6_FIRST_ROW = ROWS.length

export const EXCEPTIONS = Object.freeze({
  /* HANDOFF.md exception 1: the tertiary hue is never body text (test below). */
  text3: 'text3 / neutral #6B6B75 is restricted to borders, glyphs, the dashed outline and large text',
  /* HANDOFF.md exception 2: a hovered destructive label is inked in text (test below). */
  red_tint: 'red #E5484D on its own 10 % tint over #111113 is 4.40; the hovered destructive label is text',
  /* HANDOFF.md, P6-3: off-thread cards dimmed to 0.22 while a thread is lit are
   * decorative; every one is listed undimmed in the Outline (a11y-outline.test.js)
   * and returns undimmed on Esc or Show everything (test below). */
  dimmed: 'an off-thread card dimmed to 0.22 while a thread is lit',
})

const CSS_SRC = readFileSync(new URL('./workspace.css', import.meta.url), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
const CANVAS_SRC = readFileSync(new URL('./canvas.jsx', import.meta.url), 'utf8')
const DOCKS_SRC = readFileSync(new URL('./docks.jsx', import.meta.url), 'utf8')

/** The declarations of the one top-level rule whose selector list includes `sel`. */
export function cssRule(sel) {
  const hits = []
  for (const m of CSS_SRC.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const sels = m[1].split(',').map((x) => x.trim())
    if (!sels.includes(sel)) continue
    const decls = {}
    for (const d of m[2].split(';')) {
      const i = d.indexOf(':')
      if (i > 0) decls[d.slice(0, i).trim()] = d.slice(i + 1).trim()
    }
    hits.push(decls)
  }
  if (!hits.length) throw new Error(`no rule for ${sel} in workspace.css`)
  return Object.assign({}, ...hits)
}
/** The hex a `var(--ws-*)` (anywhere in a value) resolves to. */
export function cssHex(value, what) {
  const m = /var\((--ws-[a-z0-9-]+)\)/.exec(String(value || ''))
  if (!m || !/^#[0-9a-f]{6}$/i.test(CSS_VARS[m[1]] || '')) throw new Error(`${what}: ${value} is not a --ws-* colour`)
  return CSS_VARS[m[1]]
}
/** One inline style object's source, `const NAME = Object.freeze({ … })`. */
export function styleConst(src, name) {
  const m = new RegExp(`const ${name} = Object\\.freeze\\(\\{([\\s\\S]*?)\\}\\)`).exec(src)
  if (!m) throw new Error(`no style object ${name}`)
  return m[1]
}
/** The hex a style property's value names through `C.*` / `P.*`. */
export function styleHex(body, prop, what) {
  const m = new RegExp(`\\b${prop}:\\s*([^,\\n]+)`).exec(body)
  if (!m) throw new Error(`${what}: no ${prop}`)
  const t = /\bC\.([A-Za-z0-9]+)/.exec(m[1])
  if (!t || !/^#[0-9a-f]{6}$/i.test(C[t[1]] || '')) throw new Error(`${what}: ${prop} ${m[1]} is not a C token`)
  return C[t[1]]
}
/** A ground drawn as `A.noteGround` (black at 85 %) over the canvas. */
const noteGround = (under) => { assert.equal(A.noteGround, `color-mix(in srgb, ${P.app} 85%, transparent)`); return mix(P.app, 85, under) }

const cssText = (sel, bg, bgName, use, min = BODY) => { const d = cssRule(sel); row(cssHex(d.color, sel), `${sel} color`, bg, bgName, min, use) }
const cssBorder = (sel, prop, bg, bgName, use) => { const d = cssRule(sel); row(cssHex(d[prop], sel), `${sel} ${prop}`, bg, bgName, LARGE, use) }

/* P6-1 · the top bar (panel ground) */
const TOP = cssHex(cssRule('.ws-shell-top').background, '.ws-shell-top')
assert.equal(TOP, P.panel)
cssText('.ws-shell-top', TOP, 'top bar', 'top bar text')
cssText('.ws-shell-crumb', TOP, 'top bar', 'breadcrumb')
cssText('.ws-shell-crumb-work', TOP, 'top bar', 'breadcrumb: the work')
cssText('.ws-shell-crumb', cssHex(cssRule('.ws-shell-crumb-btn:hover').background), 'crumb hover', 'breadcrumb, hovered')
cssText('.ws-shell-seg-btn', TOP, 'top bar', 'altitude / wording button')
cssText('.ws-shell-seg-btn', cssHex(cssRule('.ws-shell-seg-btn:hover').background), 'seg hover', 'altitude / wording button, hovered')
cssText('.ws-shell-seg-btn[aria-pressed="true"]', cssHex(cssRule('.ws-shell-seg-btn[aria-pressed="true"]').background), 'seg pressed', 'the pressed altitude / wording')
{
  const pressed = cssRule('.ws-shell-seg-btn[aria-pressed="true"]')
  const marker = cssHex(pressed['box-shadow'], 'pressed marker')
  row(marker, 'pressed marker', TOP, 'top bar', LARGE, 'pressed state against the bar (SC 1.4.11)')
  row(marker, 'pressed marker', cssHex(pressed.background), 'seg pressed', LARGE, 'pressed state against its own ground')
}
cssText('.ws-shell-slot-btn', TOP, 'top bar', 'an off slot (Ask)')
cssBorder('.ws-shell-slot-btn', 'border', TOP, 'top bar', 'the off slot\'s dashed outline')
cssText('.ws-shell-slot-tag', TOP, 'top bar', 'the slot tag')
cssText('.ws-shell-why', TOP, 'top bar', 'why a slot is off')
cssText('.ws-shell-read', TOP, 'top bar', 'read time')
cssText('.ws-shell-refresh', TOP, 'top bar', 'Refresh / Keys')
cssText('.ws-shell-refresh', cssHex(cssRule('.ws-shell-refresh:hover:not([disabled])').background), 'refresh hover', 'Refresh / Keys, hovered')
cssText('.ws-shell-note', TOP, 'top bar', 'the top bar\'s note (live)')
/* P6-1 · the sidebar (panel ground) */
const SIDE = cssHex(cssRule('.ws-shell-side').background, '.ws-shell-side')
cssText('.ws-shell-side-item', SIDE, 'sidebar', 'sidebar item')
cssText('.ws-shell-muted', SIDE, 'sidebar', 'sidebar hint')
cssBorder('.ws-shell-side-current', 'border-left', SIDE, 'sidebar', 'the current item\'s rule')
row(cssHex(cssRule('.ws-shell-side-item[aria-pressed="true"]')['box-shadow']), 'side pressed marker', SIDE, 'sidebar', LARGE, 'Outline showing (SC 1.4.11)')
cssText('.ws-shell-side-view', SIDE, 'sidebar', 'a saved view')
cssText('.ws-shell-side-view', cssHex(cssRule('.ws-shell-side-view:hover').background), 'side hover', 'a saved view, hovered')
/* P6-1 · the two sheets (raised ground) */
const SHEET = cssHex(cssRule('.ws-shell-dialog').background, '.ws-shell-dialog')
cssText('.ws-shell-dialog', SHEET, 'sheet', 'key list / Ask')
cssText('.ws-shell-muted', SHEET, 'sheet', 'key list / Ask, secondary')
/* P6-1 · Brief's one state chip, on the Change screen's black */
const STORY = cssHex(cssRule('.ws-shell').background, '.ws-shell')
for (const st of ['proven', 'not_proven', 'needs_recheck', 'in_progress', 'recorded']) {
  const sel = `.ws-brief-chip[data-state="${st}"]`
  cssText(sel, STORY, 'story', `Brief chip: ${st}`)
  row(cssHex(cssRule(sel)['border-color'], sel), `${sel} border`, STORY, 'story', LARGE, `Brief chip outline: ${st}`)
}

/* P6-3 · lane headers, the thread bar and the legend, on the canvas */
const CANVAS = styleHex(styleConst(CANVAS_SRC, 'CANVAS'), 'backgroundColor', 'CANVAS')
const NOTE = noteGround(CANVAS)
row(styleHex(styleConst(CANVAS_SRC, 'LANE_LABEL'), 'color', 'LANE_LABEL'), 'lane header', NOTE, 'note ground', BODY, 'lane header')
row(styleHex(styleConst(CANVAS_SRC, 'FRAME_COUNT'), 'color', 'FRAME_COUNT'), 'lane count', NOTE, 'note ground', BODY, 'lane header count')
row(styleHex(styleConst(CANVAS_SRC, 'THREAD_BAR'), 'color', 'THREAD_BAR'), 'thread bar', NOTE, 'note ground', BODY, 'thread bar (live)')
row(styleHex(styleConst(CANVAS_SRC, 'THREAD_BAR'), 'border', 'THREAD_BAR'), 'thread bar edge', CANVAS, 'canvas', LARGE, 'thread bar edge')
row(styleHex(styleConst(CANVAS_SRC, 'THREAD_BAR_MISSING'), 'color', 'THREAD_BAR_MISSING'), 'thread bar: missing', NOTE, 'note ground', BODY, 'what the thread lacks')
row(styleHex(styleConst(CANVAS_SRC, 'THREAD_BAR_BTN'), 'color', 'THREAD_BAR_BTN'), 'Show everything', NOTE, 'note ground', BODY, 'Show everything')
row(styleHex(styleConst(CANVAS_SRC, 'LEGEND'), 'color', 'LEGEND'), 'legend', NOTE, 'note ground', BODY, 'legend words')
for (const [kind, st] of Object.entries(EDGE_STYLE)) row(st.color, `line: ${kind}`, CANVAS, 'canvas', LARGE, `a ${kind} line and its legend sample`)
row(C.amber, 'line: thread', CANVAS, 'canvas', LARGE, 'a thread line')
/* P6-3 · card faces */
const CARD = styleHex(styleConst(CANVAS_SRC, 'NODE'), 'background', 'NODE')
for (const [name, key] of [['title', 'NODE_TITLE'], ['clause', 'NODE_CLAUSE'], ['fact label', 'FACT_LABEL'], ['fact value', 'FACT_VALUE'], ['fact absent', 'FACT_ABSENT'], ['agent line', 'AGENT_LINE'], ['foot', 'NODE_FOOT'], ['band', 'FACT_BAND']]) {
  const fg = styleHex(styleConst(CANVAS_SRC, key), 'color', key)
  row(fg, `card ${name}`, CARD, 'card', BODY, `card face: ${name}`)
  /* a hollow card has no ground: the same words sit on the canvas */
  row(fg, `hollow card ${name}`, CANVAS, 'canvas', BODY, `hollow card face: ${name}`)
}
row(styleHex(styleConst(CANVAS_SRC, 'FACT_BAND'), 'border', 'FACT_BAND'), 'band edge', CARD, 'card', LARGE, '"Ran on an earlier version" band edge')
row(C.inkSoft, 'card line', CARD, 'card', BODY, 'card body line (shell.jsx lines[].color)')
{
  const mark = styleConst(CANVAS_SRC, 'THREAD_MARK')
  row(styleHex(mark, 'color', 'THREAD_MARK'), 'thread position', styleHex(mark, 'background', 'THREAD_MARK'), 'its own ground', BODY, '"n of N on the thread" on the card header')
}
for (const [fam, [ground, ink]] of Object.entries(FAM)) row(ink, `header ink:${fam}`, ground, `header:${fam}`, BODY, 'card header: kind, title, place')
{
  /* read off the card view-model in shell.jsx: the edge a hollow card is drawn with */
  const m = /facts\.hollowLine \? C\.([A-Za-z0-9]+) :/.exec(readFileSync(new URL('./shell.jsx', import.meta.url), 'utf8'))
  assert.ok(m, 'shell.jsx names the hollow card\'s edge')
  row(C[m[1]], 'hollow edge', CANVAS, 'canvas', LARGE, 'hollow card: its dashed edge (§4 "not recorded")')
}
row(C.amber, 'lit / selected edge', CANVAS, 'canvas', LARGE, 'lit or selected card edge')
{
  /* dimmed: the card, and the canvas under it, at 0.22 */
  const dim = 22
  row(mix(C.ink, dim, CANVAS), 'dimmed card text', mix(CARD, dim, CANVAS), 'dimmed card', BODY, 'off-thread card while a thread is lit', 'dimmed')
}
/* P6-3 · the Outline */
const OUT = cssHex(cssRule('.ws-trace-outline').background, '.ws-trace-outline')
cssText('.ws-trace-outline', OUT, 'outline', 'Outline text')
cssText('.ws-trace-outline h3', OUT, 'outline', 'Outline section heading')
cssText('.ws-trace-outline-intro', OUT, 'outline', 'Outline intro')
cssText('.ws-trace-outline-missing', OUT, 'outline', 'what the thread lacks')
cssText('.ws-trace-outline-status', OUT, 'outline', 'the thread, spoken (live)')
cssText('.ws-trace-outline-lanes', OUT, 'outline', 'the four lanes')
cssText('.ws-trace-outline-back', OUT, 'outline', 'Back to the picture')
const ITEM = cssHex(cssRule('.ws-trace-outline-item').background, '.ws-trace-outline-item')
cssText('.ws-trace-outline-item', ITEM, 'outline row', 'Outline row')
cssText('.ws-trace-outline-meta', ITEM, 'outline row', 'Outline row: lane, kind, place')
cssText('.ws-trace-outline-links', ITEM, 'outline row', 'Outline row: lines by kind')
cssText('.ws-trace-outline-item', cssHex(cssRule('.ws-trace-outline-item:hover').background), 'row hover', 'Outline row, hovered')
row(cssHex(cssRule('.ws-trace-outline-item[aria-current="true"]')['border-color']), 'current row edge', OUT, 'outline', LARGE, 'the selected row (SC 1.4.11)')
row(cssHex(cssRule('.ws-trace-outline-item[data-lit="yes"]')['border-left']), 'lit row rule', ITEM, 'outline row', LARGE, 'a row on the thread')
/* the dock tabs (docks.jsx) */
{
  const strip = styleHex(styleConst(DOCKS_SRC, 'DOCK_TABLIST'), 'background', 'DOCK_TABLIST')
  row(styleHex(styleConst(DOCKS_SRC, 'TAB'), 'color', 'TAB'), 'dock tab', strip, 'tab strip', BODY, 'dock tab')
  row(styleHex(styleConst(DOCKS_SRC, 'TAB_ON'), 'color', 'TAB_ON'), 'dock tab, chosen', strip, 'tab strip', BODY, 'the chosen dock tab')
  row(styleHex(styleConst(DOCKS_SRC, 'TAB_ON'), 'borderColor', 'TAB_ON'), 'dock tab marker', strip, 'tab strip', LARGE, 'the chosen tab\'s underline (SC 1.4.11)')
  row(styleHex(styleConst(DOCKS_SRC, 'TAB_COUNT'), 'color', 'TAB_COUNT'), 'dock tab count', strip, 'tab strip', BODY, 'a dock tab\'s count')
  row(C.inkBright, 'dock tab, hovered', strip, 'tab strip', BODY, 'dock tab hovered (HOVER.ghost ink)')
}
/* the focus ring against every ground it is drawn on */
for (const [bgName, bg] of Object.entries({ 'top bar': TOP, sidebar: SIDE, sheet: SHEET, story: STORY, canvas: CANVAS, card: CARD, 'outline row': ITEM, 'seg pressed (ring inside)': cssHex(cssRule('.ws-shell-seg-btn[aria-pressed="true"]').background), 'thread glow at its strongest': mix(P.amber, Number(/(\d+)%/.exec(GLOW.thread)[1]), CANVAS) })) {
  row(FOCUS.color, 'focus ring', bg, bgName, LARGE, 'focus ring (SC 1.4.11)')
}

if (process.env.THRULINE_CONTRAST_TABLE) {
  const lines = ['| foreground | background | ratio | needs | use |', '|---|---|---:|---:|---|']
  for (const r of ROWS) lines.push(`| ${r.fgName} \`${r.fg}\` | ${r.bgName} \`${r.bg}\` | ${r.ratio.toFixed(2)} | ${r.min} | ${r.use}${r.exception ? ' · named exception: ' + r.exception : ''} |`)
  console.log(lines.join('\n'))
}

test('every text token passes the threshold for every surface it is painted on', () => {
  const fails = ROWS.filter((r) => r.ratio < r.min && !r.exception).map((r) => `${r.fgName} ${r.fg} on ${r.bgName} ${r.bg}: ${r.ratio.toFixed(2)} < ${r.min} (${r.use})`)
  assert.deepEqual(fails, [])
})

test('the one §4 token that fails body contrast is recorded as restricted, not silently used as body text', () => {
  /* text3 / neutral is #6B6B75: 3.99 on #000000, 3.58 on #111113 — below 4.5.
   * The hue is not changed (the design record fixes it); its use is limited. */
  for (const bg of Object.values(SURFACES)) assert.ok(contrast(P.text3, bg) < BODY, `text3 now passes 4.5 on ${bg}: lift the restriction`)
  assert.equal(P.text3, P.neutral)
  // No stylesheet rule paints TEXT in it…
  const css = readFileSync(new URL('./workspace.css', import.meta.url), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
  const textDecls = [...css.matchAll(/(?:^|[;{\s])(color|-webkit-text-fill-color)\s*:\s*([^;}]*)/g)].map((m) => m[2])
  assert.deepEqual(textDecls.filter((v) => /--ws-(text-3|neutral)\b/.test(v)), [], 'a text colour in text-3 / neutral')
  // …and the state word that carries it (Fog) is inked in text2.
  assert.equal(wordOf('fog').color, P.neutral)
  assert.equal(wordOf('fog').ink, P.text2)
  // Old names that used to be tertiary TEXT now resolve to text2.
  for (const name of ['inkMute', 'inkFaint', 'inkDim']) assert.equal(C[name], P.text2, name)
})

test('a state word written as TEXT is inked with wordOf().ink, never its hue — the hue is for glyphs, outlines and chip grounds', () => {
  /* The head in ui-kit.jsx sets the hue on the wrapper (the glyph) and overrides the word's own span with ink, which the
   * visual-system test pins; every other surface writes a word's text through `color:`/`kindColor:` and must use ink. */
  const hits = []
  for (const f of readdirSync(new URL('.', import.meta.url)).filter((x) => /\.jsx$/.test(x) && x !== 'ui-kit.jsx')) {
    const src = readFileSync(new URL(`./${f}`, import.meta.url), 'utf8')
    for (const m of src.matchAll(/(?<![A-Za-z])(color|kindColor):\s*[^,}\n]*?\b(?:w|cw|wordOf\([^)]*\))\.color\b/g)) hits.push(`${f}: ${m[0]}`)
  }
  assert.deepEqual(hits, [])
  assert.match(readFileSync(new URL('./ui-kit.jsx', import.meta.url), 'utf8'), /w\.ink === w\.color \? WORD_TEXT : sx\(WORD_TEXT, \{ color: w\.ink \}\)/)
})

test('the measured exception: red on its own tint over a raised card is below 4.5, so the hovered destructive label is text', () => {
  const r = contrast(P.red, mix(P.red, 10, P.raised))
  assert.ok(r < BODY, `red on red-tint/raised is now ${r.toFixed(2)}: the exception can go`)
  assert.equal(r.toFixed(2), '4.40')
  assert.equal(BTN.destructiveHoverInk, P.text)
  const css = readFileSync(new URL('./workspace.css', import.meta.url), 'utf8')
  assert.match(css, /\.ws-btn-destructive:hover \{[^}]*color: var\(--ws-text\);/)
})

test('the focus ring passes 3:1 against black and against a raised card', () => {
  assert.ok(contrast(FOCUS.color, P.app) >= 3)
  assert.ok(contrast(FOCUS.color, P.raised) >= 3)
  assert.equal(FOCUS.width, '2px')
  assert.notEqual(FOCUS.color, P.amber, 'a focused primary must show a ring that is not its own face')
})

/* ── T-6-4 · C — the named exceptions stay named, and stay restricted ──── */

test('C · the P6 surfaces are measured: at least 90 more pairs than P6-6 laid, read off their own CSS rules and style objects', () => {
  assert.ok(ROWS.length - P6_FIRST_ROW >= 90, `only ${ROWS.length - P6_FIRST_ROW} P6 pairs`)
  for (const k of ['top bar', 'sidebar', 'sheet', 'note ground', 'card', 'canvas', 'outline', 'outline row', 'tab strip']) {
    assert.ok(ROWS.some((r) => r.bgName === k), `nothing measured on ${k}`)
  }
})

test('C · no pair below its threshold is carried unless it is one of the named exceptions, and every named exception is real', () => {
  const excepted = ROWS.filter((r) => r.exception)
  for (const r of excepted) {
    assert.ok(Object.prototype.hasOwnProperty.call(EXCEPTIONS, r.exception), `unlisted exception ${r.exception}`)
    assert.ok(r.ratio < r.min, `${r.fgName} on ${r.bgName} passes now (${r.ratio.toFixed(2)}): drop its exception`)
  }
  assert.deepEqual(Object.keys(EXCEPTIONS).sort(), ['dimmed', 'red_tint', 'text3'])
})

test('C · the dimmed exception is restricted: only a lit thread dims, to exactly 0.22, and Esc / Show everything undims', () => {
  const shell = readFileSync(new URL('./shell.jsx', import.meta.url), 'utf8')
  assert.match(shell, /const dim = this\.isDim\(n, ctx\) \|\| \(threadOn && !onThread\)/)
  assert.match(shell, /opacity: dim \? 0\.22 : 1,/)
  assert.match(shell, /if \(st\.sel\)|if \(!st\.sel\) return/, 'Esc clears the selection, and with it the dimming')
  /* the Outline draws no row dimmed */
  assert.doesNotMatch(readFileSync(new URL('./trace-outline.jsx', import.meta.url), 'utf8'), /opacity/)
  assert.doesNotMatch(CSS_SRC.slice(CSS_SRC.indexOf('.ws-trace-outline {')), /\.ws-trace-outline[^{]*\{[^}]*opacity/)
})

test('C · the card header draws its words at full ink: no opacity on the kind label or the place, and the thread position on its own black', () => {
  assert.doesNotMatch(styleConst(CANVAS_SRC, 'NODE_TYPE'), /opacity/)
  assert.doesNotMatch(styleConst(CANVAS_SRC, 'NODE_HDR_RIGHT'), /opacity/)
  assert.match(styleConst(CANVAS_SRC, 'THREAD_MARK'), /background:\s*C\.bgApp/)
})

/* ── LEG-DIM · report line 388: "Dimmed rows | opacity: t2 → 1.3–2.2:1 | desaturate fill; text ≥ t2 (7.04+)" ── */

test('LEG-DIM · a dimmed timeline or event row is never drawn with opacity; it desaturates to text2 ink on its own ground, ≥ 7.04:1', () => {
  const shell = readFileSync(new URL('./shell.jsx', import.meta.url), 'utf8')
  const docks = readFileSync(new URL('./docks.jsx', import.meta.url), 'utf8')
  assert.doesNotMatch(shell, /opacity: dim \? 0\.35/, 'a timeline or event row still dims by opacity (1.80:1 for text2 at 0.35)')
  const timeline = CANVAS_SRC.slice(CANVAS_SRC.indexOf('export function Timeline'), CANVAS_SRC.indexOf('/* ── Scoped-records table'))
  assert.ok(timeline.length > 100, 'found the Timeline')
  assert.doesNotMatch(timeline, /opacity/, 'the Timeline draws a row with opacity')
  const events = docks.slice(docks.indexOf('function EventsPane'), docks.indexOf('function', docks.indexOf('function EventsPane') + 20))
  assert.ok(events.length > 100, 'found the EventsPane')
  assert.doesNotMatch(events, /opacity/, 'the events pane draws a row with opacity')
  /* the dimmed ink, read off each renderer's own style */
  for (const [src, name] of [[CANVAS_SRC, 'TL_DIM'], [docks, 'EV_DIM']]) {
    const ink = styleHex(styleConst(src, name), 'color', name)
    assert.equal(ink.toLowerCase(), P.text2.toLowerCase(), `${name} is text2`)
    for (const [ground, gName] of [[P.app, 'app'], [P.panel, 'panel'], [P.raised, 'raised']]) {
      const r = contrast(ink, ground)
      assert.ok(r >= 7.04, `${name} on ${gName} is ${r.toFixed(2)}:1, below the report's 7.04`)
    }
  }
  /* each row builder hands the renderer a flag, not an opacity */
  assert.equal((shell.match(/\bdim: !!dim,/g) || []).length, 2, 'the timeline and event row builders each pass `dim: !!dim`')
})
