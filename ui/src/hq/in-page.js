/* In-page links (C30 R-LINKS): a link whose href is `#<id>` — the whole
 * story's contents, its "what counts as done" line — moves the page to that
 * place. It never changes the address, so the router never sees it and the
 * screen stays where it is.
 *
 * No React here: `followInPage` is a document click listener the app and the
 * design gallery both install.
 */
import { motionReduced } from './data/root.js'

/** The id an in-page link points at, or null when the link is an address (`#/…`), external, or empty. */
export function inPageId(hrefAttr) {
  if (typeof hrefAttr !== 'string' || hrefAttr.length < 2 || hrefAttr[0] !== '#' || hrefAttr[1] === '/') return null
  try {
    return decodeURIComponent(hrefAttr.slice(1))
  } catch (e) {
    return hrefAttr.slice(1)
  }
}

/**
 * A click on an in-page link: scroll to its place and keep the address.
 * Returns true when it handled the click.
 * @param {MouseEvent} e
 */
export function followInPage(e) {
  if (!e || e.defaultPrevented || e.button > 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return false
  const t = e.target
  const a = t && typeof t.closest === 'function' ? t.closest('a[href]') : null
  if (!a) return false
  const id = inPageId(a.getAttribute('href'))
  if (id == null) return false
  e.preventDefault()
  const doc = a.ownerDocument
  const place = doc && doc.getElementById(id)
  if (place && typeof place.scrollIntoView === 'function') {
    /* "Always reduce" (Settings) and a Mac that asks for less motion both scroll at once (data/appearance.js) */
    place.scrollIntoView({ block: 'start', behavior: motionReduced(doc) ? 'auto' : 'smooth' })
  }
  return true
}
