/* N13-FOCUS-2411 — WCAG 2.2 SC 2.4.11 Focus Not Obscured (Minimum), rendered.
 *
 *   node scripts/testing/focus-obscured-walk.mjs
 *
 * On the shared harness page (`a11y-harness.mjs`) at 1512×920 and 1024×768,
 * presses Tab through every focusable on the Trace at Explore and at Workbench,
 * the Change screen at Explore, the Inbox and the key-list sheet, then
 * Shift+Tab back through them (backwards is where a sticky header hides
 * what the page just scrolled up to). At every stop it takes the focused
 * element's box, clipped to the viewport, and asks `elementFromPoint` at
 * its four inner corners and its centre what is on top there:
 *
 *   - ENTIRELY OBSCURED: at none of the five points is the element (or
 *     something inside it) on top — the 2.4.11 failure;
 *   - centre hidden: the stricter record, reported, not the gate.
 *
 * Exit 0 passed (0 entirely obscured) · 1 failed · 2 could not run.
 */
import { COULD_NOT_RUN, FAILED, PASSED, withPage } from './a11y-harness.mjs'

const PAGES = [
  { name: 'Trace · Explore', path: '?altitude=explore#/a11y' },
  { name: 'Trace · Workbench', path: '?altitude=workbench#/a11y' },
  { name: 'Change · Explore', path: '?altitude=explore#/changes/ch1' },
  { name: 'Change · Brief', path: '?altitude=brief#/changes/ch1' },
  { name: 'Inbox', path: '#/inbox', ready: '#root > *' },
  { name: 'Key list sheet', path: '?altitude=explore#/changes/ch1', open: `[...document.querySelectorAll('.ws-shell-top button')].find((b) => b.textContent.trim() === 'Keys')?.click()` },
]
const SIZES = [[1512, 920], [1024, 768]]
const MAX = 600

/* In the page: where focus is, and whether it can be seen. */
const STOP = `(() => {
  const el = document.activeElement
  if (!el || el === document.body || el === document.documentElement) return null
  const r = el.getBoundingClientRect()
  const x0 = Math.max(0, r.left), y0 = Math.max(0, r.top), x1 = Math.min(innerWidth, r.right), y1 = Math.min(innerHeight, r.bottom)
  const name = (el.getAttribute('aria-label') || el.textContent || el.getAttribute('title') || '').trim().replace(/\\s+/g, ' ').slice(0, 50)
  const at = Array.prototype.indexOf.call(document.querySelectorAll('*'), el)
  const who = el.tagName.toLowerCase() + (el.id ? '#' + el.id : '') + (el.getAttribute('role') ? '[role=' + el.getAttribute('role') + ']' : '') + (el.getAttribute('data-id') ? '[data-id=' + el.getAttribute('data-id') + ']' : '') + ' "' + name + '"'
  if (x1 - x0 < 1 || y1 - y0 < 1) return { who, key: who + '#' + at, points: 0, seen: 0, centre: false, box: [r.left, r.top, r.width, r.height].map(Math.round), top: 'outside the viewport' }
  const inset = (a, b) => Math.min(2, (b - a) / 2)
  const pts = [[x0 + inset(x0, x1), y0 + inset(y0, y1)], [x1 - inset(x0, x1), y0 + inset(y0, y1)], [x0 + inset(x0, x1), y1 - inset(y0, y1)], [x1 - inset(x0, x1), y1 - inset(y0, y1)], [(x0 + x1) / 2, (y0 + y1) / 2]]
  const on = pts.map(([x, y]) => { const h = document.elementFromPoint(x, y); return !!h && (h === el || el.contains(h)) })
  const centreHit = document.elementFromPoint(pts[4][0], pts[4][1])
  const desc = (h) => h ? h.tagName.toLowerCase() + (h.className && typeof h.className === 'string' ? '.' + h.className.split(/\\s+/).slice(0, 2).join('.') : '') + (h.contains(el) ? ' (an ancestor)' : '') + (getComputedStyle(h).position === 'sticky' || (h.closest && [...(function* up(n) { while (n) { yield n; n = n.parentElement } })(h)].some((n) => getComputedStyle(n).position === 'sticky')) ? ' (sticky)' : '') : 'nothing'
  let sc = el.parentElement
  while (sc && !(/(auto|scroll)/.test(getComputedStyle(sc).overflowY) && sc.scrollHeight > sc.clientHeight)) sc = sc.parentElement
  const scr = sc ? sc.getBoundingClientRect() : null
  const clipped = scr ? (r.bottom <= scr.top || r.top >= scr.bottom) : false
  return { who, key: who + '#' + at, points: pts.length, seen: on.filter(Boolean).length, centre: on[4], box: [r.left, r.top, r.width, r.height].map(Math.round), top: desc(centreHit) + (clipped ? ' [outside its scroll box ' + [scr.top, scr.bottom].map(Math.round).join('-') + ']' : '') }
})()`

let failures = 0
let stops = 0
let centreHidden = 0
const notRun = []
const code = await withPage('focus-obscured-walk', async ({ send, evaluate, goto, press, base }) => {
  for (const [w, h] of SIZES) {
    await send('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 1, mobile: false })
    for (const page of PAGES) {
      await goto(base + page.path, page.ready)
      if (page.open) { await evaluate(page.open); await new Promise((r) => setTimeout(r, 300)) }
      else await evaluate(`document.activeElement && document.activeElement.blur && document.activeElement.blur()`)
      const seen = new Set()
      let pageStops = 0
      let pageBad = 0
      for (const dir of ['Tab', 'Shift+Tab']) {
        let first = null
        for (let i = 0; i < MAX; i += 1) {
          await press(dir, 30)
          const s = await evaluate(STOP)
          if (!s) continue
          if (first === null) first = s.key
          else if (s.key === first) break
          stops += 1
          pageStops += 1
          seen.add(s.who)
          if (!s.centre) centreHidden += 1
          if (s.seen === 0) {
            failures += 1
            pageBad += 1
            console.log(`OBSCURED  ${w}x${h} ${page.name} ${dir}: ${s.who} box ${s.box.join(',')} — on top at its centre: ${s.top}`)
          }
        }
      }
      if (!pageStops) {
        notRun.push(`${w}x${h} ${page.name}`)
        console.log(`page  ${w}x${h} ${page.name}: could not run — Tab reached no element on this page`)
        continue
      }
      console.log(`page  ${w}x${h} ${page.name}: ${pageStops} stops, ${seen.size} distinct, ${pageBad} entirely obscured`)
    }
  }
  if (!stops) { console.log('focus-obscured-walk: could not run — Tab reached nothing'); return COULD_NOT_RUN }
  console.log(`focus-obscured-walk: ${stops} stops; ${failures} entirely obscured; ${centreHidden} with the centre hidden (stricter, reported)`)
  if (notRun.length) console.log(`focus-obscured-walk: could not run on ${notRun.length} page(s), not counted as passed: ${notRun.join('; ')}`)
  console.log(failures ? 'focus-obscured-walk: FAILED' : 'focus-obscured-walk: PASSED (on the pages that ran)')
  return failures ? FAILED : PASSED
})
process.exit(code)
