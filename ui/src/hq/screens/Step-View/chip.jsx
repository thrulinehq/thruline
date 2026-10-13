/* A record chip ("Run step 12 ›", "Edit step 9 ›", "Said step 4 ›"), drawn for
 * where it is (C30 R-ROUTES "Step view").
 *
 * - In the design gallery (`live` false) it is the canvas's own element,
 *   unchanged — a button or an anchor — so the gallery stays pixel-identical.
 * - On a live address with a step to open (`to`) it is a real link to it.
 * - On a live address without one (a check ThruLine ran has no step page; a
 *   version recorded at no step) it is plain text in the chip's look: never a
 *   button or a link that does nothing.
 */
import React from 'react'

export function Chip({ live, to, tag = 'button', href, className, style, children, ...rest }) {
  if (!live) {
    return tag === 'a'
      ? <a href={href} className={className} style={style} {...rest}>{children}</a>
      : <button type="button" className={className} style={style} {...rest}>{children}</button>
  }
  const { onClick, 'aria-pressed': _pressed, ...keep } = rest
  if (to) return <a href={to} className={className} style={style} {...keep}>{children}</a>
  return <span style={{ ...(style || {}), cursor: 'default' }} {...keep}>{children}</span>
}
