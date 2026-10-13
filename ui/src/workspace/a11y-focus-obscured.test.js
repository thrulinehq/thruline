/* N13-FOCUS-2411 — WCAG 2.2 SC 2.4.11 Focus Not Obscured (Minimum), static half.
 *
 * Every sticky element in the workspace is listed here with the scroll box it
 * sticks in, and each scroll box keeps `scroll-padding-top` equal to that
 * sticky element's height — from the same SIZE token where the height is a
 * token, and measured from the element where its height is set by its
 * content (`sticky-padding.js`). A browser that scrolls a focused element
 * into view then stops short of the sticky header instead of under it. A new
 * sticky element fails this test until it is listed.
 *
 * Script focus that can land in such a box asks for `scrollIntoView({ block:
 * 'nearest' })` after it focuses (never `smooth`: nothing a key triggers
 * animates). The rendered half is scripts/testing/focus-obscured-walk.mjs.
 */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'

/** One frozen style object's body (the same reader contrast.test.js uses). */
function styleConst(src, name) {
  const m = new RegExp(`const ${name} = Object\\.freeze\\(\\{([\\s\\S]*?)\\}\\)`).exec(src)
  if (!m) throw new Error(`no style object ${name}`)
  return m[1]
}

const read = (f) => readFileSync(new URL('./' + f, import.meta.url), 'utf8')

/** Every sticky element in the source: `file · where`. */
function stickies() {
  const out = []
  for (const f of readdirSync(new URL('.', import.meta.url)).filter((n) => /\.(jsx|css)$/.test(n)).sort()) {
    const src = read(f).replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, ' '))
    src.split('\n').forEach((line, i) => {
      if (/position:\s*'sticky'|position:\s*sticky/.test(line)) out.push(`${f}:${i + 1}`)
    })
  }
  return out
}

test('every sticky element is accounted for, with the scroll box it can cover', () => {
  const found = stickies().map((s) => s.replace(/:\d+$/, ''))
  assert.deepEqual(found.sort(), [
    'canvas.jsx', //        TB_HEAD — the records table's header, in TB_WRAP
    'chrome.jsx', //        BANNER — the demo banner, in the workspace root, which does not scroll (overflow: hidden)
    'chrome.jsx', //        DemoBanner's own `position: 'sticky'` (the same element; lib/inbox.test.js reads it)
    'docks.jsx', //         STICKY_HEAD — the findings and events headers, in PANE_BODY
    'workspace.css', //     .ws-checks-dialog > header, in the receipt dialog
    'workspace.css', //     .ws-verdict-head, in .ws-hq-content
    'workspace.css', //     .ws-inspector-footer (A3-F4) — the Inspector's action block, stuck to the BOTTOM of the page's own scroll; the page pads its bottom by the footer's measured height (html:has(.ws-inspector-footer) { scroll-padding-bottom: var(--inspector-footer-h) }, written by Inspector; primary-action.test.js pins both)
  ].sort(), `a sticky element this test does not know: ${stickies().join(', ')}`)
})

test('a scroll box under a token-height sticky header pads its top by that token', () => {
  const canvas = read('canvas.jsx')
  assert.match(styleConst(canvas, 'TB_HEAD'), /height:\s*SIZE\.stickyHeader/)
  assert.match(styleConst(canvas, 'TB_WRAP'), /scrollPaddingTop:\s*SIZE\.stickyHeader/, 'the records table pads its scroll box by its sticky header')
  const docks = read('docks.jsx')
  assert.match(styleConst(docks, 'STICKY_HEAD'), /height:\s*SIZE\.statusStrip/)
  assert.match(styleConst(docks, 'PANE_BODY'), /scrollPaddingTop:\s*SIZE\.statusStrip/, 'the dock pane pads its scroll box by the findings/events header')
})

test('a sticky header whose height is its content has its scroll box padded by its measured height', () => {
  const pad = read('sticky-padding.js')
  assert.match(pad, /export function padUnderSticky\(/)
  assert.match(pad, /scrollPaddingTop\s*=\s*[^;]*offsetHeight/, 'the padding is the header\'s own height')
  assert.match(pad, /ResizeObserver/, 'and follows it when the header grows')
  assert.match(read('verdict-panel.jsx'), /padUnderSticky\(headRef\.current\)/, 'the verdict head pads .ws-hq-content')
  assert.match(read('verdict-panel.jsx'), /<header ref=\{headRef\} className=\{`ws-verdict-head/)
  assert.match(read('checks-panel.jsx'), /padUnderSticky\(/, 'the receipt dialog\'s header pads the dialog')
})

test('script focus that can land under a sticky header scrolls to the nearest edge, instantly', () => {
  const alt = read('altitude.js')
  assert.match(alt, /control\.scrollIntoView\(\{ block: 'nearest' \}\)/, '. — the primary it focuses')
  const outline = read('trace-outline.jsx')
  assert.match(outline, /el\.scrollIntoView\(\{ block: 'nearest' \}\)/, 'J / K and the arrows in the Outline')
  for (const f of ['altitude.js', 'trace-outline.jsx', 'sticky-padding.js']) assert.doesNotMatch(read(f), /behavior:\s*'smooth'/, `${f}: nothing a key triggers animates`)
})
