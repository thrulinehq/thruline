/* The request a door on one screen makes of another (PATH-UI-2-R2).
 *
 * PURE. Strings in, strings out: no window, no storage, no React. The one
 * place a request is written (`doorHref`) and the one place it is read
 * (`doorFromSearch`), so the two cannot drift.
 *
 * WHY THE ADDRESS CARRIES IT. The whole-story page (`#/changes/<id>/packet`)
 * and the Change screen (`#/changes/<id>/<stage>`) are different roots, and
 * `routes.js` `watchAddress` boots a new document whenever the address names a
 * different screen. Whatever a person meant by pressing "Go to … to link it" on
 * the page — open THIS line — is gone with the old document unless something
 * carries it across. The address is the one thing that does, and it needs no
 * storage: a blocked or full store, a second tab, a link opened in a new window
 * and a copied link all keep it. The Change screen's own hash grammar
 * (`#/changes/<id>/<stage>`) is deliberately left as it is; the request rides in
 * the query, beside `?altitude=` and never instead of it.
 *
 * WHAT IT IS NOT. Not a write and not a decision. It asks a screen to open a
 * line and put the cursor where the line's control begins; the control itself
 * is the one on the Verify stage, which still asks for a name before it acts.
 * The request is spent the moment it has been used (`searchWithoutDoor`), so a
 * reload does not repeat it and a shared address made later does not carry it.
 */

/** The one query parameter a door writes and the Change screen reads. */
export const DOOR_PARAM = 'door'

/** A line number as the address may carry it: one positive whole number of at most four digits, no sign, no zero padding. */
const LINE = /^[1-9][0-9]{0,3}$/
const MOST = 9999

const paramsOf = (search) => new URLSearchParams(typeof search === 'string' ? search : '')

/**
 * The address of a door: the Verify stage of one Change, asking for one line.
 *
 * Whatever else the current address carries (`search`) stays on it, and an
 * earlier request is replaced, never doubled. A line that is not a whole number
 * in range is not written at all: the address falls back to the plain Verify
 * stage, which is what the door was before it named a line, and says nothing it
 * cannot back.
 *
 * @param {string} id the Change
 * @param {number} n the accepted line, counted from 1
 * @param {string} [search] the current `window.location.search`
 * @returns {string} a relative reference: `?door=<n>#/changes/<id>/verify`
 */
export function doorHref(id, n, search = '') {
  const stage = '#/changes/' + encodeURIComponent(String(id)) + '/verify'
  if (!Number.isInteger(n) || n < 1 || n > MOST) return stage
  const params = paramsOf(search)
  params.delete(DOOR_PARAM)
  params.append(DOOR_PARAM, String(n))
  return '?' + params.toString() + stage
}

/**
 * The request an address carries, or null.
 *
 * Exactly one `door` value, and it must be a line number as above. Two of them
 * are neither: a reading that had to pick one would be a guess. Nothing here
 * says whether the Change has that line; the screen that lands finds out.
 *
 * @param {string} search `window.location.search`, with or without its `?`
 * @returns {{n: number}|null}
 */
export function doorFromSearch(search) {
  if (typeof search !== 'string') return null
  const all = paramsOf(search).getAll(DOOR_PARAM)
  if (all.length !== 1 || !LINE.test(all[0])) return null
  return { n: Number(all[0]) }
}

/**
 * The same query with the request taken off it, and nothing else touched.
 *
 * @param {string} search
 * @returns {string} `''` when nothing else is left, else `?` and what is
 */
export function searchWithoutDoor(search) {
  const params = paramsOf(search)
  params.delete(DOOR_PARAM)
  const rest = params.toString()
  return rest ? '?' + rest : ''
}
