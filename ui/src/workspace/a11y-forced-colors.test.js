/* N13-FORCED-COLOURS, part A — a `@media (forced-colors: active)` layer.
 *
 * Report line 1122: forced colours remove background images and shadows, so
 * everything the workspace tells by a ground or a box-shadow — the selected
 * card, the lit thread, a pressed toggle — disappears in Windows High
 * Contrast unless it is redrawn as an outline or a border in system colours.
 * This reads workspace.css and asserts the one block exists and covers:
 * the focus ring, the selected card, the lit-thread card, a pressed toggle,
 * a selected tab and the border of every control. Part B (hatching as an
 * SVG pattern, dashed Fog) waits on N2-PALETTE and is not asserted here.
 */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const CSS = readFileSync(new URL('./workspace.css', import.meta.url), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')

/** The body of every `@media (forced-colors: active) { … }`, braces balanced. */
function blocks(css) {
  const out = []
  const re = /@media\s*\(\s*forced-colors\s*:\s*active\s*\)\s*\{/g
  let m
  while ((m = re.exec(css))) {
    let depth = 1
    let i = re.lastIndex
    for (; i < css.length && depth; i += 1) depth += css[i] === '{' ? 1 : css[i] === '}' ? -1 : 0
    out.push(css.slice(re.lastIndex, i - 1))
  }
  return out
}
/** Rules inside a block: [{ selectors: string[], body }] */
function rules(block) {
  /* split a selector list on the commas outside parentheses (`:is(a, b)` is one) */
  const split = (list) => {
    const out = ['']
    let depth = 0
    for (const ch of list) {
      if (ch === '(') depth += 1
      if (ch === ')') depth -= 1
      if (ch === ',' && depth === 0) out.push('')
      else out[out.length - 1] += ch
    }
    return out.map((x) => x.trim().replace(/\s+/g, ' ')).filter(Boolean)
  }
  return [...block.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({ selectors: split(m[1]), body: m[2] }))
}
const ruleFor = (rs, pred, what) => {
  const r = rs.find((x) => x.selectors.some(pred))
  assert.ok(r, `the forced-colours layer has no rule for ${what}`)
  return r
}

test('workspace.css has exactly one forced-colours layer', () => {
  assert.equal(blocks(CSS).length, 1)
})

test('the layer covers the focus ring, the selected card, the lit thread, a pressed toggle, a selected tab and every control border — each in a system colour', () => {
  const [block] = blocks(CSS)
  assert.ok(block, 'no @media (forced-colors: active) block')
  const rs = rules(block)
  /* The focus ring: the forced palette sets the ring's token to the system
   * highlight on every root, and every ring rule in the sheet draws in that
   * token — so no ring anywhere keeps a colour the palette would drop. */
  const focus = ruleFor(rs, (s) => s === '.ws-root', 'the focus ring token')
  assert.match(focus.body, /--ws-focus:\s*Highlight/)
  for (const root of ['.ws-root', '.ws-change', '.ws-inbox', '.ws-shell', '.ws-shell-sheet-scrim']) assert.ok(focus.selectors.includes(root), `focus ring token under ${root}`)
  const outside = CSS.replace(block, '')
  const rings = [...outside.matchAll(/([^{}]*:focus(?:-visible)?[^{}]*)\{([^{}]*outline:[^{}]*)\}/g)]
  assert.ok(rings.length >= 5, `found ${rings.length} ring rules`)
  for (const [, sel, body] of rings) {
    if (/outline:\s*(none|0)\b/.test(body)) continue
    assert.match(body, /outline:\s*var\(--ws-focus-width\) solid var\(--ws-focus\)/, `a ring drawn outside the token: ${sel.trim()}`)
  }
  const selected = ruleFor(rs, (s) => s.includes('[role="button"][data-id][aria-pressed="true"]'), 'the selected card')
  assert.match(selected.body, /outline:\s*2px solid Highlight/)
  const lit = ruleFor(rs, (s) => s.includes('[data-lit="yes"]'), 'the lit-thread card')
  assert.match(lit.body, /border:\s*2px (solid|dashed) Highlight/)
  const pressed = ruleFor(rs, (s) => s.includes('[aria-pressed="true"]') && s.includes(':not([data-id])'), 'a pressed toggle')
  assert.match(pressed.body, /Highlight/)
  const tab = ruleFor(rs, (s) => s.includes('[role="tab"][aria-selected="true"]'), 'a selected tab')
  assert.match(tab.body, /Highlight/)
  const controls = ruleFor(rs, (s) => s === '.ws-root button', 'control borders')
  for (const root of ['.ws-root', '.ws-change', '.ws-inbox', '.ws-shell', '.ws-shell-dialog']) {
    for (const tag of ['button', 'input', 'select', 'textarea', 'summary']) assert.ok(controls.selectors.includes(`${root} ${tag}`), `control border: ${root} ${tag}`)
  }
  assert.match(controls.body, /border:\s*1px solid ButtonText/)
})

test('a pressed toggle and the selected card are marked by more than the ground the forced palette drops', () => {
  /* the ground-only marks this layer exists to replace are still what the
   * normal palette draws — so the layer is load-bearing, not decoration */
  assert.match(CSS, /\.ws-shell-seg-btn\[aria-pressed="true"\] \{[^}]*box-shadow/)
  const shell = readFileSync(new URL('./shell.jsx', import.meta.url), 'utf8')
  assert.match(shell, /shadow: onThread\s*\?\s*`0 0 0 1px \$\{C\.amber\}, \$\{GLOW\.thread\}`/, 'the lit card is drawn by a box-shadow in the normal palette')
  const canvas = readFileSync(new URL('./canvas.jsx', import.meta.url), 'utf8')
  assert.match(canvas, /data-lit=\{n\.lit \? 'yes' : undefined\}/, 'the lit card carries data-lit for the layer to find')
  assert.match(canvas, /aria-pressed=\{selected \? 'true' : 'false'\}/, 'the selected card carries aria-pressed for the layer to find')
})
