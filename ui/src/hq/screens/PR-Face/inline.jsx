/* One Inline list of the note (markdown.js), drawn in the artboard's own
 * markup: the design's `<code>`, `<strong>`, `<em>` and aria-hidden mark spans,
 * with the design's styles. `size` is the code font size the design uses in
 * that place (12.5px in the body, 12px in the table and the footer). `wrap`
 * lets a long recorded path (over 20 characters) break inside its chip on a
 * live page, and keeps a short one (a version id) on one line; the design
 * gallery never sets it. */
import React from 'react'

const codeStyle = (size, wrap) => ({
  fontFamily: "'Geist Mono', monospace", fontSize: size, background: 'rgba(255,255,255,.08)', borderRadius: '4px', padding: '1px 5px', color: '#EDEDEF',
  ...(wrap === 'long' ? { overflowWrap: 'anywhere' } : wrap === 'short' ? { whiteSpace: 'nowrap' } : {}),
})

export function Inline({ segs, size = '12.5px', wrap = false }) {
  return (segs || []).map((x, i) => {
    if (typeof x === 'string') return <React.Fragment key={i}>{x}</React.Fragment>
    if (x == null) return null
    if (x.code != null) return <code key={i} style={codeStyle(size, wrap ? (String(x.code).length > 20 ? 'long' : 'short') : null)}>{x.code}</code>
    if (x.rec != null) return <React.Fragment key={i}>{x.rec}</React.Fragment>
    if (x.strong) return <strong key={i} style={{ fontWeight: '600' }}><Inline segs={x.strong} size={size} wrap={wrap} /></strong>
    if (x.em != null) return <em key={i}>{x.em}</em>
    if (x.glyph != null) return <span key={i} aria-hidden="true">{x.glyph}</span>
    if (x.stop != null) return <span key={i} role="img" aria-label={x.label}>{x.stop}</span>
    return null
  })
}

export default Inline
