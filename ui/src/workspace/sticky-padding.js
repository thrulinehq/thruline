/* N13-FOCUS-2411 (WCAG 2.2 SC 2.4.11 Focus Not Obscured, Minimum).
 *
 * PURE apart from the DOM it is handed. A sticky header whose height is set
 * by its content (the verdict head, the receipt dialog's header) cannot name
 * a SIZE token, so its scroll box is padded by the header's measured height:
 * `scroll-padding-top` on the nearest ancestor that scrolls, kept equal to
 * `offsetHeight` while the header grows or shrinks. A browser that scrolls a
 * focused element into view then stops below the header, not under it.
 * No storage, no network, no animation.
 */

/** The nearest ancestor of `el` that scrolls vertically, or null. */
export function scrollBoxOf(el) {
  if (!el || typeof getComputedStyle !== 'function') return null
  for (let n = el.parentElement; n; n = n.parentElement) {
    const y = getComputedStyle(n).overflowY
    if (y === 'auto' || y === 'scroll') return n
  }
  return null
}

/**
 * Pad the scroll box a sticky header sits in by the header's own height.
 *
 * @param {HTMLElement|null} header
 * @returns {() => void} cleanup: stops following and clears the padding
 */
export function padUnderSticky(header) {
  const box = scrollBoxOf(header)
  if (!box) return () => {}
  const set = () => { box.style.scrollPaddingTop = header.offsetHeight + 'px' }
  set()
  const RO = typeof window !== 'undefined' ? window.ResizeObserver : undefined
  const ro = typeof RO === 'function' ? new RO(set) : null
  if (ro) ro.observe(header)
  return () => {
    if (ro) ro.disconnect()
    box.style.scrollPaddingTop = ''
  }
}
