/* P6-6 — the §4 true-black visual system, held.
 *
 * One token source (tokens.js), exposed to workspace.css as custom properties
 * through one generated block; no second palette anywhere in the workspace;
 * buttons, type, focus, motion and breakpoints as the design record §4/§6 set
 * them. Every test here is a guard with a break-and-restore pair in
 * docs/loop/reports/p6/P6-6-coder.md.
 */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { join } from 'node:path'
import { BP, BTN, C, CSS_VARS, FOCUS, FS, GLOW, MOTION, PALETTE, PLAIN_MIN_PX, cssVarBlock } from './tokens.js'
import { WORD, wordOf } from './logic.js'

const here = fileURLToPath(new URL('.', import.meta.url))
const read = (f) => readFileSync(join(here, f), 'utf8')
const CSS = read('workspace.css')
const stripComments = (s) => s.replace(/\/\*[\s\S]*?\*\//g, '')

/** Every innermost rule of a sheet, with the at-rule preludes it sits inside. */
function rules(css) {
  const src = stripComments(css)
  const out = []
  const stack = []
  let buf = ''
  for (const ch of src) {
    if (ch === '{') {
      stack.push(buf.trim())
      buf = ''
    } else if (ch === '}') {
      const prelude = stack.pop()
      if (buf.trim() || !prelude.startsWith('@')) {
        const decls = {}
        for (const d of buf.split(';')) {
          const i = d.indexOf(':')
          if (i > 0) decls[d.slice(0, i).trim().toLowerCase()] = d.slice(i + 1).trim()
        }
        out.push({ selector: prelude, at: stack.filter((p) => p.startsWith('@')), decls })
      }
      buf = ''
    } else buf += ch
  }
  return out
}

const BEGIN = '/* @tokens:begin'
const END = '/* @tokens:end */'
const outsideBlock = (css) => css.slice(0, css.indexOf(BEGIN)) + css.slice(css.indexOf(END) + END.length)
const COLOUR_LITERAL = /#[0-9a-f]{3,8}\b|\brgba?\(|\bhsla?\(|(?<![-\w])(?:white|black|red|green|blue|yellow|orange|purple|gray|grey)(?![-\w])/gi

/* ── one token source ─────────────────────────────────────────────────── */

test('workspace.css carries the generated token block exactly once, byte-equal to tokens.js', () => {
  const at = CSS.indexOf(BEGIN)
  assert.ok(at >= 0, 'no @tokens block in workspace.css')
  assert.equal(CSS.indexOf(BEGIN, at + 1), -1, 'two @tokens blocks')
  const block = CSS.slice(at, CSS.indexOf(END) + END.length)
  assert.equal(block, cssVarBlock(), 'the @tokens block has drifted from tokens.js — regenerate it (see tokens.js, "CSS custom properties")')
})

test('no colour literal is written anywhere in workspace.css outside the token block', () => {
  const hits = stripComments(outsideBlock(CSS)).match(COLOUR_LITERAL) || []
  assert.deepEqual(hits, [])
})

test('every --ws-* property workspace.css reads is one the token block defines', () => {
  const used = new Set([...stripComments(outsideBlock(CSS)).matchAll(/var\((--ws-[a-z0-9-]+)/g)].map((m) => m[1]))
  const local = new Set(['--ws-btn-hue']) // set per element by `.ws-btn-outline`
  const missing = [...used].filter((v) => !(v in CSS_VARS) && !local.has(v))
  assert.deepEqual(missing, [])
  // The pre-P6-6 fallback-carrying names are gone, not aliased.
  assert.doesNotMatch(CSS, /var\(--ws-(fg|muted|line|border)\b/)
})

test('one palette: every C colour is a §4 PALETTE value, and PALETTE is exactly the design record §4', () => {
  assert.deepEqual({ ...PALETTE }, {
    app: '#000000', panel: '#0A0A0B', raised: '#111113', hover: '#16161A', hairline: '#1F1F23', hairlineStrong: '#2A2A30',
    text: '#EDEDEF', text2: '#A1A1AA', text3: '#6B6B75',
    amber: '#F5A524', green: '#3DD68C', coral: '#FF6369', yellow: '#F2C94C', blue: '#52A8FF', violet: '#B78CFF', cyan: '#4CC9F0', neutral: '#6B6B75', red: '#E5484D',
  })
  const values = new Set(Object.values(PALETTE))
  const strays = Object.entries(C).filter(([, v]) => !values.has(v)).map(([k, v]) => `${k}=${v}`)
  assert.deepEqual(strays, [], 'a C name that is not a §4 value')
})

test('no colour literal in the workspace JavaScript outside tokens.js, except the two Trace palettes P6-3 owns', () => {
  const hits = []
  for (const f of readdirSync(here)) {
    if (!/\.(jsx?|mjs)$/.test(f) || /\.test\./.test(f) || f === 'tokens.js') continue
    let src = read(f).replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:\\])\/\/.*$/gm, '$1')
    if (f === 'logic.js') {
      // FAM and WIRE: the Trace's node-family palette (see the note above them in logic.js).
      src = src.replace(/export const FAM = Object\.freeze\(\{[\s\S]*?\n\}\)/, '').replace(/export const WIRE = Object\.freeze\(\{[\s\S]*?\n\}\)/, '')
    }
    for (const m of src.matchAll(/['"`][^'"`\n]*?(#[0-9a-fA-F]{3,8}\b|rgba?\(\s*\d)[^'"`\n]*['"`]/g)) {
      if (/^['"`]#[\w/:.-]*['"`]$/.test(m[0]) && !/^['"`]#[0-9a-fA-F]{3,8}['"`]$/.test(m[0])) continue // a URL hash, not a colour
      hits.push(`${f}: ${m[0].slice(0, 60)}`)
    }
  }
  assert.deepEqual(hits, [])
})

/* ── the state words keep their word, their shape and a meaning hue ───── */

test('each state word paints its §4 meaning hue, and none is told apart by colour alone', () => {
  assert.equal(WORD.proven[1], PALETTE.green)
  assert.equal(WORD.kept[1], PALETTE.green)
  assert.equal(WORD.not_proven[1], PALETTE.coral)
  assert.equal(WORD.broken[1], PALETTE.coral)
  assert.equal(WORD.couldnt[1], PALETTE.yellow)
  assert.equal(WORD.loose[1], PALETTE.blue)
  assert.equal(WORD.fog[1], PALETTE.neutral)
  const glyphs = Object.values(WORD).map((w) => w[2])
  assert.equal(new Set(glyphs).size, glyphs.length, 'two words share a glyph')
  for (const [slug, w] of Object.entries(WORD)) assert.ok(w[0] && w[2], `${slug} lost its word or its shape`)
  // Fog's chip is the §4 "not recorded" shape: dashed, never a filled neutral ground.
  assert.notEqual(wordOf('fog').ink, wordOf('fog').color)
  const kit = read('ui-kit.jsx')
  assert.match(kit, /border: `1px dashed \$\{w\.color\}`/)
})

/* ── buttons ──────────────────────────────────────────────────────────── */

test('one filled amber primary: every primary paints amber with black ink, and nothing else is filled amber', () => {
  assert.deepEqual({ ...BTN.primary }, { background: PALETTE.amber, color: PALETTE.app, border: `1px solid ${PALETTE.amber}`, fontWeight: 600 })
  const rs = rules(CSS)
  const PRIMARIES = ['.ws-btn-primary', '.ws-change-primary', '.ws-wayin-primary', '.ws-inbox-primary', '.ws-checks-panel button.ws-checks-primary', '.ws-hq-content .ws-verdict-primary']
  for (const sel of PRIMARIES) {
    const r = rs.find((x) => x.selector === sel)
    assert.ok(r, `no rule for ${sel}`)
    assert.equal(r.decls.background, 'var(--ws-amber)', `${sel} background`)
    assert.equal(r.decls.color, 'var(--ws-app)', `${sel} ink`)
  }
  const filled = rs.filter((r) => r.decls.background === 'var(--ws-amber)' && !/:hover/.test(r.selector)).map((r) => r.selector)
  assert.deepEqual(filled.sort(), [...PRIMARIES].sort(), 'a filled amber control that is not a primary')
  // The Inbox shows one item per Change, each with its action: only the first is filled.
  const later = rs.find((r) => r.selector === '.ws-inbox-list > li:not(:first-child) .ws-inbox-primary')
  assert.ok(later, 'every Inbox item\'s action is filled — two filled ambers in one view')
  assert.equal(later.decls.background, 'transparent')
  assert.equal(later.decls.color, 'var(--ws-amber)')
  // Inline-style primaries: the toolbar's is filled; the inspector's and the menu's are outlines.
  assert.match(read('chrome.jsx'), /const PRIMARY = Object\.freeze\(\{[\s\S]*?\.\.\.BTN\.primary,/)
  assert.match(read('chrome.jsx'), /const MENU_SAVE_BTN = Object\.freeze\(\{[\s\S]*?\.\.\.outlineButton\(C\.amber\),/)
  assert.match(read('docks.jsx'), /const ACTION_PRIMARY = Object\.freeze\(outlineButton\(C\.amber\)\)/)
})

test('meaning-coloured buttons are outlines with a 10 % tint on hover; destructive is a red outline', () => {
  const rs = rules(CSS)
  const get = (sel) => rs.find((r) => r.selector === sel)?.decls || {}
  assert.equal(get('.ws-btn-outline').background, 'transparent')
  assert.equal(get('.ws-btn-outline').border, '1px solid var(--ws-btn-hue)')
  assert.equal(get('.ws-btn-outline:hover').background, 'color-mix(in srgb, var(--ws-btn-hue) 10%, transparent)')
  assert.equal(get('.ws-btn-destructive').border, '1px solid var(--ws-red)')
  assert.equal(get('.ws-btn-destructive').background, 'transparent')
  assert.equal(CSS_VARS['--ws-amber-tint'], 'color-mix(in srgb, var(--ws-amber) 10%, transparent)')
})

test('no gradient, no glass and no glow — except the one lit-thread glow token, which only the lit thread paints', () => {
  const body = stripComments(outsideBlock(CSS))
  assert.doesNotMatch(body, /(linear|radial|conic)-gradient\(/, 'a gradient in the stylesheet')
  assert.doesNotMatch(body, /backdrop-filter|filter:\s*blur/, 'glass')
  // A shadow may only be the black depth token or a zero-blur rule; never a hue.
  for (const r of rules(CSS)) {
    const v = r.decls['box-shadow']
    if (!v || v === 'none') continue
    assert.doesNotMatch(v, /--ws-(amber|green|coral|yellow|blue|violet|cyan|red|neutral)/, `${r.selector} glows: ${v}`)
  }
  assert.ok(GLOW.thread && CSS_VARS['--ws-glow-thread'])
  assert.doesNotMatch(body, /--ws-glow-thread/, 'the stylesheet paints no glow; the lit thread paints it inline')
  /* P6-3: GLOW.thread is read in exactly one place outside tokens.js — the
   * lit-thread branch of the graph model in shell.jsx — and nowhere else. */
  const readers = readdirSync(here).filter((f) => /\.(jsx?|mjs)$/.test(f) && !/\.test\./.test(f) && f !== 'tokens.js')
    .filter((f) => /GLOW\.thread/.test(read(f).replace(/\/\*[\s\S]*?\*\//g, '')))
  assert.deepEqual(readers, ['shell.jsx'])
  const shell = read('shell.jsx')
  assert.equal(shell.match(/GLOW\.thread/g).length, 1, 'the glow is painted once, on the lit thread')
  assert.match(shell, /onThread\s*\?\s*`0 0 0 1px \$\{C\.amber\}, \$\{GLOW\.thread\}`/, 'the glow belongs to a card on the thread')
})

/* ── type ─────────────────────────────────────────────────────────────── */

const px = (v) => {
  const s = String(v).trim()
  const m = /^var\((--ws-fs-[a-z-]+)\)/.exec(s)
  if (m) return px(CSS_VARS[m[1]])
  const n = /^([\d.]+)px/.exec(s)
  if (n) return Number(n[1])
  const r = /^([\d.]+)rem/.exec(s)
  if (r) return Number(r[1]) * 16
  return null
}

test('the type scale: Plain floor 14px, headline 22–28px, Plex self-hosted, workspace base raised to 14px', () => {
  assert.equal(PLAIN_MIN_PX, 14)
  for (const k of ['plain', 'plainBody', 'plainHeading', 'plainLead', 'base', 'title']) assert.ok(parseFloat(FS[k]) >= 14, `${k} is ${FS[k]}`)
  for (const k of ['headline', 'headlineNarrow', 'docTitle']) assert.ok(parseFloat(FS[k]) >= 22 && parseFloat(FS[k]) <= 28, `${k} is ${FS[k]}`)
  // Plex first, and a fallback stack INSIDE the var(): a bare var(--sans) on a page with no --sans invalidates the
  // whole `font` declaration and the Change screen renders in serif (seen in headless Chrome during P6-6).
  assert.match(CSS_VARS['--ws-font-sans'], /^'IBM Plex Sans', var\(--sans, [^)]*sans-serif\)$/)
  assert.match(CSS_VARS['--ws-font-mono'], /^'IBM Plex Mono', var\(--mono, [^)]*monospace\)$/)
  const root = rules(CSS).find((r) => r.selector === '.ws-root')
  assert.equal(px(root.decls['font-size']), 14)
  assert.doesNotMatch(CSS + read('../../index.html'), /fonts\.googleapis|fonts\.gstatic|@import\s+url\(https?:/, 'a network font')
})

test('every font size on the Plain screens (Change, packet, Inbox, first use, way in) is at least 14px', () => {
  const plain = CSS.slice(CSS.indexOf('/* ── the Change workspace (T-2-4)'))
  assert.ok(plain.length < CSS.length, 'the Plain section marker moved')
  const small = []
  for (const r of rules(plain)) {
    for (const prop of ['font-size', 'font']) {
      const v = r.decls[prop]
      if (!v || v === 'inherit') continue
      const size = px(v)
      if (size === null) small.push(`${r.selector} { ${prop}: ${v} } (not a size this test can read)`)
      else if (size < PLAIN_MIN_PX) small.push(`${r.selector} { ${prop}: ${v} } = ${size}px`)
    }
  }
  assert.deepEqual(small, [])
  // The two headlines use the scale's headline, and step to its narrow size at BP.narrow.
  for (const sel of ['.ws-change-title', '.ws-inbox-heading']) {
    const r = rules(plain).find((x) => x.selector === sel && !x.at.length)
    assert.equal(r.decls['font-size'], 'var(--ws-fs-headline)', sel)
  }
})

/* ── focus ────────────────────────────────────────────────────────────── */

test('a visible focus ring on every interactive element of all three screen roots, and nothing turns it off', () => {
  const rs = rules(CSS)
  const ring = rs.find((r) => r.selector === '.ws-root *:focus-visible, .ws-change *:focus-visible, .ws-inbox *:focus-visible')
  assert.ok(ring, 'the root focus rule is gone')
  assert.equal(ring.decls.outline, 'var(--ws-focus-width) solid var(--ws-focus)')
  assert.equal(ring.decls['outline-offset'], 'var(--ws-focus-offset)')
  assert.equal(CSS_VARS['--ws-focus'], FOCUS.color)
  for (const r of rs.filter((x) => /focus/.test(x.selector) && x.decls.outline)) {
    assert.match(r.decls.outline, /var\(--ws-focus\)/, `${r.selector} draws its ring in another colour: ${r.decls.outline}`)
  }
  const suppress = rs.filter((r) => /^(none|0)\b/.test(r.decls.outline || '') || /^(none|0)\b/.test(r.decls['outline-style'] || '')).map((r) => r.selector)
  assert.deepEqual(suppress, [], 'a rule turns the focus ring off')
  for (const f of readdirSync(here).filter((x) => /\.jsx$/.test(x))) {
    assert.doesNotMatch(read(f), /outline:\s*['"](none|0)['"]/, `${f} turns the focus ring off inline`)
  }
})

/* ── motion ───────────────────────────────────────────────────────────── */

test('motion: 240ms and 320ms ease-out only, played once, nothing loops, and every animation is opt-in', () => {
  assert.deepEqual({ ...MOTION }, { quick: '240ms', slow: '320ms', ease: 'ease-out' })
  const body = stripComments(outsideBlock(CSS))
  assert.doesNotMatch(body, /\binfinite\b/, 'a looping animation')
  assert.doesNotMatch(body, /iteration-count:\s*(?!1\b)/, 'an animation that plays more than once')
  for (const r of rules(CSS)) {
    for (const prop of ['animation', 'transition', 'animation-duration', 'transition-duration']) {
      const v = r.decls[prop]
      if (!v || /^none( !important)?$/.test(v)) continue
      assert.doesNotMatch(v, /\b\d*\.?\d+m?s\b/, `${r.selector} writes a duration literal: ${v}`)
      assert.match(v, /var\(--ws-motion-(quick|slow)\) var\(--ws-ease\)/, `${r.selector} is off the motion tokens: ${v}`)
      assert.ok(r.at.includes('@media (prefers-reduced-motion: no-preference)'), `${r.selector} animates outside the no-preference block`)
    }
  }
  for (const f of readdirSync(here).filter((x) => /\.(jsx|js)$/.test(x) && !/\.test\./.test(x))) {
    const src = read(f).replace(/\/\*[\s\S]*?\*\//g, '')
    assert.doesNotMatch(src, /infinite/, `${f} loops an animation inline`)
    for (const m of src.matchAll(/(transition|animation):\s*(['"`][^'"`]*['"`])/g)) {
      assert.match(m[2], /MOTION\.(quick|slow)|^['"`]none['"`]$/, `${f}: ${m[0]}`)
    }
  }
})

test('reduced motion: every animation and transition under all three roots collapses to none', () => {
  const r = rules(CSS).find((x) => x.at.includes('@media (prefers-reduced-motion: reduce)') && /\.ws-root \*,/.test(x.selector))
  assert.ok(r, 'the reduce block is gone')
  for (const root of ['.ws-root', '.ws-change', '.ws-inbox']) {
    for (const part of [`${root} *`, `${root} *::before`, `${root} *::after`]) assert.ok(r.selector.split(/\s*,\s*/).includes(part), `${part} is not reduced`)
  }
  assert.equal(r.decls.animation, 'none !important')
  assert.equal(r.decls.transition, 'none !important')
})

/* ── breakpoints ──────────────────────────────────────────────────────── */

test('breakpoints are the §6 tokens, and every other width in the sheet is a named legacy exception', () => {
  assert.deepEqual({ ...BP }, { sheet: 1100, narrow: 800 })
  const LEGACY = [1250, 450] // the app bar's label-shedding width, and the checks panel's own narrow layout
  const widths = [...CSS.matchAll(/@media \(max-width: (\d+)px\)/g)].map((m) => Number(m[1]))
  assert.deepEqual(widths.filter((w) => w !== BP.sheet && w !== BP.narrow && !LEGACY.includes(w)), [])
  const narrow = rules(CSS).filter((r) => r.at.includes(`@media (max-width: ${BP.narrow}px)`))
  const plain = narrow.find((r) => r.selector === '.ws-change, .ws-inbox')
  assert.ok(plain, 'the Plain screens have no BP.narrow rule')
  assert.equal(plain.decls['overflow-wrap'], 'anywhere')
  assert.match(read('shell.jsx'), /const NARROW = BP\.sheet/)
})
