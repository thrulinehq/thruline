/* N13-FORCED-COLOURS, part A — the rendered check.
 *
 *   node scripts/testing/forced-colors-probe.mjs [screenshot-dir]
 *
 * Opens the shared harness page (`a11y-harness.mjs`) in headless Chrome and
 * turns forced colours on through CDP (`Emulation.setEmulatedMedia`,
 * `forced-colors: active`). On the Trace at Explore it selects a card with a
 * real click and lights its thread with F, moves focus off the card so the
 * focus ring is not what is measured, and asserts, from computed styles:
 *
 *   - the forced palette is really on (`matchMedia` answers it);
 *   - the selected card's `outline-style` is not `none`;
 *   - a card on the lit thread has a dashed border;
 *   - the pressed altitude toggle has a 2px border.
 *
 * With a directory argument it also writes screenshots of the Trace (card
 * selected, thread lit), the Change screen and the key list — to that
 * directory only, never into the repository.
 *
 * Exit 0 passed · 1 failed · 2 could not run (never a pass).
 */
import { writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { COULD_NOT_RUN, FAILED, PASSED, withPage } from './a11y-harness.mjs'

const shots = process.argv[2] || null
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

const code = await withPage('forced-colors-probe', async ({ send, evaluate, goto, base }) => {
  const shot = async (name) => {
    if (!shots) return
    const { data } = await send('Page.captureScreenshot', { format: 'png' })
    await writeFile(join(shots, name + '.png'), Buffer.from(data, 'base64'))
    console.log(`shot  ${join(shots, name + '.png')}`)
  }
  const key = async (k, code, vk) => {
    await send('Input.dispatchKeyEvent', { type: 'keyDown', key: k, code, windowsVirtualKeyCode: vk, text: k })
    await send('Input.dispatchKeyEvent', { type: 'keyUp', key: k, code, windowsVirtualKeyCode: vk })
    await sleep(300)
  }
  await send('Emulation.setEmulatedMedia', { features: [{ name: 'forced-colors', value: 'active' }] })
  await goto(base + '?altitude=explore#/a11y')
  if (!(await evaluate(`matchMedia('(forced-colors: active)').matches`))) {
    console.log('forced-colors-probe: could not run — the forced palette did not turn on')
    return COULD_NOT_RUN
  }
  /* a real click on a card the view shows */
  const at = await evaluate(`(() => { const cards = [...document.querySelectorAll('[role="button"][data-id]')].map((el) => [el, el.getBoundingClientRect()]).filter(([, r]) => r.width > 40 && r.top > 0 && r.bottom < innerHeight && r.left > 0 && r.right < innerWidth); if (!cards.length) return null; const [, r] = cards[Math.min(1, cards.length - 1)]; return { x: r.left + r.width / 2, y: r.top + Math.min(12, r.height / 2) } })()`)
  if (!at) { console.log('forced-colors-probe: could not run — no card in view'); return COULD_NOT_RUN }
  await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: at.x, y: at.y, button: 'left', clickCount: 1 })
  await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: at.x, y: at.y, button: 'left', clickCount: 1 })
  await sleep(400)
  await key('f', 'KeyF', 70)
  /* the focus ring, while focus is on the selected card: the system highlight */
  const ring = await evaluate(`(() => { const probe = document.createElement('span'); probe.style.color = 'Highlight'; document.body.appendChild(probe); const hl = getComputedStyle(probe).color; probe.remove(); const a = document.activeElement; const s = a && a !== document.body ? getComputedStyle(a) : null; return { hl, on: a ? a.tagName + (a.getAttribute('data-id') ? '[data-id=' + a.getAttribute('data-id') + ']' : '') : null, style: s && s.outlineStyle, color: s && s.outlineColor } })()`)
  await evaluate(`document.activeElement && document.activeElement.blur && document.activeElement.blur()`)
  await sleep(200)
  const m = await evaluate(`(() => {
    const s = (el) => el ? getComputedStyle(el) : null
    const sel = document.querySelector('[role="button"][data-id][aria-pressed="true"]')
    const lit = [...document.querySelectorAll('[role="button"][data-id][data-lit="yes"]')]
    const pressed = document.querySelector('.ws-shell-seg-btn[aria-pressed="true"]')
    return {
      selected: sel ? { id: sel.getAttribute('data-id'), outline: s(sel).outlineStyle, outlineWidth: s(sel).outlineWidth, boxShadow: s(sel).boxShadow } : null,
      lit: lit.length, litBorder: lit[0] ? s(lit[0]).borderTopStyle + ' ' + s(lit[0]).borderTopWidth : null,
      pressed: pressed ? s(pressed).borderTopStyle + ' ' + s(pressed).borderTopWidth : null,
    }
  })()`)
  console.log('trace ' + JSON.stringify(m))
  await shot('forced-trace-selected-lit')
  let failures = 0
  const check = (ok, what) => { if (!ok) failures += 1; console.log(`${ok ? 'PASS' : 'FAIL'}  ${what}`) }
  if (!m.selected) { console.log('forced-colors-probe: could not run — the click selected no card'); return COULD_NOT_RUN }
  check(ring.style !== 'none' && ring.color === ring.hl, `focus ring on ${ring.on}: ${ring.style} ${ring.color} (Highlight is ${ring.hl})`)
  check(m.selected.outline !== 'none', `selected card ${m.selected.id}: outline-style ${m.selected.outline} ${m.selected.outlineWidth}`)
  check(m.lit > 0 && /^dashed 2px$/.test(m.litBorder), `lit thread: ${m.lit} cards, border ${m.litBorder}`)
  check(/^solid 2px$/.test(m.pressed || ''), `pressed altitude toggle: border ${m.pressed}`)

  await goto(base + '?altitude=explore#/changes/ch1')
  await shot('forced-change-explore')
  const change = await evaluate(`(() => { const p = document.querySelector('.ws-shell-seg-btn[aria-pressed="true"]'); return p ? getComputedStyle(p).borderTopStyle + ' ' + getComputedStyle(p).borderTopWidth : null })()`)
  check(/^solid 2px$/.test(change || ''), `Change screen pressed altitude toggle: border ${change}`)
  await evaluate(`[...document.querySelectorAll('.ws-shell-top button')].find((b) => /^Keys$/.test(b.textContent.trim()))?.click()`)
  await sleep(300)
  await shot('forced-key-sheet')
  const sheet = await evaluate(`(() => { const b = document.querySelector('.ws-shell-dialog [role="switch"]'); return b ? getComputedStyle(b).borderTopStyle + ' ' + getComputedStyle(b).borderTopWidth : null })()`)
  check(/^solid [12]px$/.test(sheet || ''), `key-list switch keeps an edge: ${sheet}`)

  console.log(`forced-colors-probe: ${failures} failures`)
  console.log(failures ? 'forced-colors-probe: FAILED' : 'forced-colors-probe: PASSED')
  return failures ? FAILED : PASSED
})
process.exit(code)
