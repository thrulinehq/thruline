/* The altitude group of a Change screen's top bar (DESIGN-SPEC §2.1b item 4),
 * as links (C30 R-PREFS: "Altitude 1 2 3 navigates: Brief → brief, Explore →
 * statements, Workbench → changed").
 *
 * PURE. Each altitude names the canvas page it shows; the template hands that
 * page to `link(…)`, and the router turns it into the open Change's address
 * with its project (routes.js `resolveLink`: a Change screen with no Change
 * open is no link, never the Inbox). Nothing here keeps an altitude in state:
 * the address is the altitude.
 *
 * The marked altitude carries the canvas's "on" look (background #2A2A30,
 * t1) and `aria-current`; the others the canvas's "off" look (transparent,
 * t2). Which one is marked is the screen's own:
 *   - Change-Brief → Brief, Every-Statement → Explore, What-Changed →
 *     Workbench: the screen IS that altitude (`aria-current="page"`);
 *   - Repair-Loop, Approve-Sheet, Done-Criteria: not an altitude of their
 *     own; the canvas marks Brief on each, and so does the product
 *     (`aria-current="true"`: the current item of the group, not this page).
 */

const T1 = '#EDEDEF', T2 = '#A1A1AA', ON = '#2A2A30', CTRL = '#66666F'

/** [key, label, key the design draws, the canvas page it shows]. */
export const ALTITUDES = Object.freeze([
  ['brief', 'Brief', '1', 'Change-Brief.dc.html'],
  ['explore', 'Explore', '2', 'Every-Statement.dc.html'],
  ['workbench', 'Workbench', '3', 'What-Changed.dc.html'],
])

/**
 * The three segments, marked for `marked` ('brief' | 'explore' |
 * 'workbench'). `page` is true when the screen is that altitude itself.
 *
 * @returns {{list: object[], brief: object, explore: object, workbench: object}}
 */
export function altitudes(marked, { page = true } = {}) {
  const list = ALTITUDES.map(([id, label, key, target], i) => {
    const on = id === marked
    return {
      id, label, key: 'Key ' + key, page: target,
      current: on ? (page ? 'page' : 'true') : undefined,
      pressed: on ? 'true' : 'false',
      bg: on ? ON : 'transparent',
      color: on ? T1 : T2,
      bl: i === 0 ? '0px solid transparent' : '1px solid ' + CTRL,
    }
  })
  return { list, brief: list[0], explore: list[1], workbench: list[2] }
}
