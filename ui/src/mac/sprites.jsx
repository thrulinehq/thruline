import React from 'react'
import { CAST, NIB_A, NIB_B, NIB_MID, PAL } from '../lib/arcade.js'
import { arcLengths, chompFrameAt, pointAt } from '../lib/walk.js'

/* The cast, inside the canvas and nowhere else (RA5 directive 4).
 *
 * The macOS chrome speaks system typography; the arcade survives as the
 * canvas's inhabitants. Every appearance is seated on a record: NIB walks a
 * lane's eaten trail, SNAG stands at a contradicted node, DUSK at a stale one.
 * Nothing summons the benched member — GhostAt refuses it by construction,
 * and the containment test walks these files by name. */

/** A pixel sprite as SVG rects, in canvas coordinates. */
export function SpriteSvg({ rows, x, y, scale = 2, opacity = 1 }) {
  const w = rows[0].length * scale
  const h = rows.length * scale
  const cells = []
  for (let r = 0; r < rows.length; r++) {
    for (let c = 0; c < rows[r].length; c++) {
      const ch = rows[r][c]
      if (ch !== '.' && PAL[ch]) cells.push([c, r, PAL[ch]])
    }
  }
  return (
    <g aria-hidden="true" opacity={opacity} transform={`translate(${x - w / 2}, ${y - h / 2})`}>
      {cells.map(([c, r, fill]) => (
        <rect key={`${c}-${r}`} x={c * scale} y={r * scale} width={scale} height={scale} fill={fill} />
      ))}
    </g>
  )
}

/** A ghost at its node. `who` is 'snag' | 'dusk' — the two with seats. */
export function GhostAt({ who, x, y }) {
  const member = CAST[who]
  if (!member || member.bench) return null
  return <SpriteSvg rows={member.frames[0]} x={x} y={y} scale={2} />
}

/* ── the walk, as craft (RA6 amendment A) ───────────────────────────────
 *
 * The CSS steps() glide was a robot: uniform teleports, a chomp blinking on
 * its own clock, contact whenever it happened to land. This walker is driven
 * by one requestAnimationFrame loop at the display's own cadence:
 *
 *   · constant travel speed over the REAL polyline (arc-length parameterised,
 *     binary-searched — a 5,000-claim lane costs O(log n) per frame);
 *   · easing per path segment (ease-in-out inside each pellet-to-pellet leg,
 *     so NIB gathers itself and settles rather than gliding);
 *   · the chomp is a function of DISTANCE TO THE NEXT PELLET, phased through
 *     the pinned CHOMP cycle so the mouth is fully open exactly at contact —
 *     the same law the RB-era pin holds on the data (`CHOMP[0] === 2`);
 *   · reduced motion parks the final frame at the trail's end and never
 *     starts the loop.
 *
 * Ghosts hold still by design law: SNAG is never animated for charm, DUSK
 * drifts nowhere — GhostAt renders static frames only, and the pin checks. */

const NIB_FRAMES3 = [NIB_A, NIB_MID, NIB_B]
const WALK_SPEED = 90 /* px per second of arc length — the craft constant */

export function NibOnTrail({ points }) {
  const ref = React.useRef(null)
  const cum = React.useMemo(() => arcLengths(points), [points])

  React.useEffect(() => {
    const g = ref.current
    if (!g || points.length < 2) return undefined
    const frames = [...g.children]
    const showFrame = (idx) => {
      frames.forEach((f, i) => f.setAttribute('opacity', i === idx ? '1' : '0'))
    }
    const total = cum[cum.length - 1]
    const place = (d, frameIdx) => {
      const p = pointAt(points, cum, d)
      g.setAttribute('transform', `translate(${p.x - 13}, ${p.y - 13})`)
      showFrame(frameIdx)
      return p
    }
    const mql = window.matchMedia('(prefers-reduced-motion: reduce)')
    if (mql.matches) {
      /* Parked: the final frame at the trail's end, mouth at rest. */
      place(total, 0)
      return undefined
    }
    let raf
    const t0 = performance.now()
    const cycle = total / WALK_SPEED + 1.2
    const step = (now) => {
      const t = ((now - t0) / 1000) % cycle
      const d = Math.min(total, t * WALK_SPEED)
      const p = pointAt(points, cum, d)
      const frameIdx = chompFrameAt(p.segEnd - d, cum[p.seg + 1] - cum[p.seg] || 1)
      g.setAttribute('transform', `translate(${p.x - 13}, ${p.y - 13})`)
      showFrame(frameIdx)
      raf = requestAnimationFrame(step)
    }
    raf = requestAnimationFrame(step)
    return () => cancelAnimationFrame(raf)
  }, [points, cum])

  if (points.length < 2) return null

  const frame = (rows, key) => {
    const cells = []
    for (let r = 0; r < rows.length; r++) {
      for (let c = 0; c < rows[r].length; c++) {
        const ch = rows[r][c]
        if (ch !== '.' && PAL[ch]) cells.push([c, r, PAL[ch]])
      }
    }
    return (
      <g key={key} opacity={key === 0 ? 1 : 0}>
        {cells.map(([c, r, fill]) => (
          <rect key={`${c}-${r}`} x={c * 2} y={r * 2} width={2} height={2} fill={fill} />
        ))}
      </g>
    )
  }
  return (
    <g ref={ref} aria-hidden="true" className="tl-nib">
      {NIB_FRAMES3.map((rows, i) => frame(rows, i))}
    </g>
  )
}
