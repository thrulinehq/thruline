/* N13-GLYPHS-ARIA — the rendered half: what Chrome's accessibility tree says.
 *
 *   node scripts/testing/glyph-ax-probe.mjs
 *
 * For each page in the shared harness (`a11y-harness.mjs`: the Trace at
 * Brief, Explore and Workbench over the bundled demo record, the Change screen at
 * Brief and Explore over the synthetic Change), reads the full accessibility
 * tree over CDP (`Accessibility.getFullAXTree`) and checks:
 *
 *   1. every node that carries a state (`[data-state]`, and the state word
 *      `[data-word]` that `StateWord` draws) has an accessible text that
 *      contains its state word and none of the state glyphs;
 *   2. no node Chrome exposes (not ignored) has a name holding a state glyph.
 *
 * Exit 0 passed · 1 failed · 2 could not run (never a pass).
 */
import { COULD_NOT_RUN, FAILED, PAGES, PASSED, withPage } from './a11y-harness.mjs'

export const GLYPHS = ['●', '○', '✕', '◭', '◐', '▪', '⌒', '◌', '◇', '◆', '⋯', '⁓', '□', '⊠', '✓', '×', '↻', '⊘', '▲', '⚠', '▶', '•']
const WORDS = { not_proven: 'Not proven', proven: 'Proven', loose: 'Loose thread', fog: 'Fog', couldnt: 'Recorded', kept: 'Kept', broken: 'Broken' }

const code = await withPage('glyph-ax-probe', async ({ send, evaluate, goto, base }) => {
  await send('DOM.enable')
  await send('Accessibility.enable')
  let failures = 0
  let stateNodes = 0
  let checkedNodes = 0
  for (const page of PAGES) {
    await goto(base + page.path)
    /* 1. every state-carrying element: the text a screen reader reads for it. */
    const states = await evaluate(`(() => {
      const spoken = (el) => { let s = ''; for (const c of el.childNodes) { if (c.nodeType === 3) s += c.textContent; else if (c.nodeType === 1 && c.getAttribute('aria-hidden') !== 'true' && getComputedStyle(c).display !== 'none') s += c.hasAttribute('aria-label') ? ' ' + c.getAttribute('aria-label') + ' ' : spoken(c) } return s };
      return [...document.querySelectorAll('[data-word], [data-state]')].filter((el) => !el.closest('[aria-hidden="true"]')).map((el) => ({ slug: el.getAttribute('data-word') || el.getAttribute('data-state'), word: !!el.getAttribute('data-word'), said: (el.hasAttribute('aria-label') ? el.getAttribute('aria-label') : spoken(el)).replace(/\\s+/g, ' ').trim() }))
    })()`)
    for (const s of states) {
      stateNodes += 1
      const glyph = GLYPHS.find((g) => s.said.includes(g))
      const wordMissing = s.word && WORDS[s.slug] && !s.said.includes(WORDS[s.slug])
      if (glyph || wordMissing || !s.said) {
        failures += 1
        console.log(`FAIL  ${page.name}: state ${s.slug} reads "${s.said}"${glyph ? ' (glyph ' + glyph + ')' : ''}${wordMissing ? ' (no word)' : ''}`)
      }
    }
    /* 2. the tree Chrome builds: no exposed name carries a glyph. */
    const { nodes } = await send('Accessibility.getFullAXTree')
    const byId = new Map(nodes.map((n) => [n.nodeId, n]))
    const CONTROLS = new Set(['button', 'tab', 'link', 'menuitem', 'switch', 'checkbox', 'radio', 'option', 'img', 'image'])
    /* A control named by aria-label / aria-labelledby is read by that name;
     * the text inside it is not read on its own. */
    const labelledControl = (n) => {
      for (let p = byId.get(n.parentId); p; p = byId.get(p.parentId)) {
        const role = p.role && p.role.value
        const fromAttr = p.name && (p.name.sources || []).some((src) => src.type === 'attribute' && src.value && !src.superseded)
        if (CONTROLS.has(role)) return fromAttr
      }
      return false
    }
    for (const n of nodes) {
      if (n.ignored) continue
      const role = n.role && n.role.value
      /* InlineTextBox repeats its StaticText; a ListMarker is the list's own bullet. */
      if (role === 'InlineTextBox' || role === 'ListMarker') continue
      if (role === 'StaticText' && labelledControl(n)) continue
      checkedNodes += 1
      const name = n.name && typeof n.name.value === 'string' ? n.name.value : ''
      const glyph = GLYPHS.find((g) => name.includes(g))
      if (glyph) {
        failures += 1
        console.log(`FAIL  ${page.name}: ${n.role && n.role.value} named "${name.slice(0, 80)}" exposes ${glyph}`)
      }
    }
    console.log(`page  ${page.name}: ${states.length} state nodes`)
  }
  console.log(`glyph-ax-probe: ${stateNodes} state nodes and ${checkedNodes} exposed AX nodes over ${PAGES.length} pages; ${failures} failures`)
  if (!stateNodes) { console.log('glyph-ax-probe: could not run — no state node was drawn on any page'); return COULD_NOT_RUN }
  console.log(failures ? 'glyph-ax-probe: FAILED' : 'glyph-ax-probe: PASSED')
  return failures ? FAILED : PASSED
})
process.exit(code)
