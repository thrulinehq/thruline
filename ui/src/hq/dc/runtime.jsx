/* The canvas's component model, in React — just enough of it to run an
 * artboard's logic exactly as the canvas ran it.
 *
 * An artboard is a `class Component extends DCLogic` with a `state` object, a
 * `setState(partial)` that merges and redraws, and a `renderVals()` that
 * returns every value its markup reads. `DcScreen` hosts one instance and
 * hands `renderVals()` to the ported template (scripts/hq/dc2jsx.mjs). The
 * instance lives as long as the screen does; `props` are refreshed on every
 * render so live data reaches `renderVals()` through `this.props`.
 */
import React, { useEffect, useReducer, useRef } from 'react'

export class DCLogic {
  constructor(props) {
    this.props = props || {}
    this.state = {}
    this.__redraw = null
  }

  setState(partial) {
    const next = typeof partial === 'function' ? partial(this.state, this.props) : partial
    if (!next) return
    this.state = { ...this.state, ...next }
    if (this.__redraw) this.__redraw()
  }
}

export function DcScreen({ Logic, Template, props, link }) {
  const [, redraw] = useReducer((n) => n + 1, 0)
  const ref = useRef(null)
  if (!ref.current) ref.current = new Logic(props)
  const inst = ref.current
  inst.props = props || {}
  inst.__redraw = redraw
  useEffect(() => {
    if (typeof inst.componentDidMount === 'function') inst.componentDidMount()
    return () => {
      inst.__redraw = null
      if (typeof inst.componentWillUnmount === 'function') inst.componentWillUnmount()
    }
  }, [inst])
  return <Template v={inst.renderVals()} link={link} />
}

/** A CSS declaration list computed at render time ("color: #fff; gap: 4px")
 *  as a React style object, in the order written. */
export function cssText(s) {
  const out = {}
  if (typeof s !== 'string') return out
  for (const d of s.split(';')) {
    const k = d.indexOf(':')
    if (k < 0) continue
    const prop = d.slice(0, k).trim()
    if (!prop) continue
    const key = prop.startsWith('--') ? prop : prop.replace(/-([a-z])/g, (_, c) => c.toUpperCase())
    out[key] = d.slice(k + 1).trim()
  }
  return out
}
