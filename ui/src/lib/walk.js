/* The walk's mathematics (RA6 motion pass) — pure, node-testable.
 *
 * Arc-length parameterisation, per-segment ease-in-out, and the chomp phase
 * that lands the mouth fully open exactly at pellet contact (CHOMP[0], the
 * pinned contact frame). The renderer (mac/sprites.jsx) owns the rAF loop and
 * reduced-motion parking; nothing here touches the DOM. */

import { CHOMP } from './arcade.js'

/** Ease-in-out inside one segment: t in [0,1] → eased [0,1]. */
export function segmentEase(t) {
  return t < 0.5 ? 2 * t * t : 1 - (2 - 2 * t) ** 2 / 2
}

/**
 * The chomp frame at `distance` from the next pellet with `spacing` between
 * pellets: contact (distance 0) is the fully-open frame, mid-leg passes
 * closed, and the phase is total over the leg regardless of its length.
 */
export function chompFrameAt(distance, spacing) {
  if (!(spacing > 0)) return CHOMP[0]
  const phase = Math.max(0, Math.min(0.9999, distance / spacing))
  return CHOMP[Math.floor((1 - phase) * CHOMP.length) % CHOMP.length]
}

/** Cumulative arc lengths for a polyline. Computed once per trail. */
export function arcLengths(points) {
  const out = [0]
  for (let i = 1; i < points.length; i++) {
    out.push(out[i - 1] + Math.hypot(points[i].x - points[i - 1].x, points[i].y - points[i - 1].y))
  }
  return out
}

/** Position at arc-length `d` along the polyline, eased inside its segment. */
export function pointAt(points, cum, d) {
  const total = cum[cum.length - 1]
  const clamped = Math.max(0, Math.min(total, d))
  let lo = 0
  let hi = cum.length - 1
  while (lo + 1 < hi) {
    const mid = (lo + hi) >> 1
    if (cum[mid] <= clamped) lo = mid
    else hi = mid
  }
  const segLen = cum[lo + 1] - cum[lo] || 1
  const t = segmentEase((clamped - cum[lo]) / segLen)
  const a = points[lo]
  const b = points[lo + 1]
  return {
    x: a.x + (b.x - a.x) * t,
    y: a.y + (b.y - a.y) * t,
    seg: lo,
    segEnd: cum[lo + 1],
  }
}
