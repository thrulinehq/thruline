/* viewport.js — the arithmetic behind drawing a very tall graph cheaply and
 * honestly. Three separable jobs, none of which touches the DOM, React or
 * `this`, so all three are unit-testable on their own:
 *
 *   1. WINDOWING  — which boxes are near enough to the view to be worth mounting
 *   2. THE OVERVIEW — how a ~1,700 x ~130,000 px graph is drawn legibly in 170x108
 *   3. FIT        — what zoom shows everything, and what to say when none does
 *
 * WHY WINDOWING IS ONLY EVER A DRAWING DECISION. A node that is not mounted is
 * still in scope, still counted, still reachable by find, by replay, by the
 * overview and by every filter — `shell.buildVals` applies the window at the
 * render boundary and NOWHERE else, and in particular never inside
 * `visibleNodes()`, which six things read and four of them are counts. The
 * safety of that argument rests on one property of `windowBox` below: the
 * window is a STRICT SUPERSET of the viewport, by at least one whole viewport
 * in every direction. A card that is dropped is a card that was off-screen
 * anyway. If that margin were ever zero, or computed from a `view` other than
 * the one the transform string is computed from, a card could vanish from
 * inside the visible area — which is why `windowBox` takes the view as an
 * argument instead of reading it, and why the caller derives both in the same
 * pass.
 *
 * WHY THE OVERVIEW STRETCHES. A real session's graph is roughly 1:76 tall. Fit
 * that aspect uniformly into a 170x108 box and the whole trace is a 1.4 px
 * column; scale it uniformly to the width and every card is 0.09 px tall, which
 * is what "the minimap renders as an empty box" actually was. So the two axes
 * are scaled independently, every mark is drawn at a floor of one whole cell,
 * a cell holding several cards takes the WORST of them, and the surface says
 * out loud what one pixel of it stands for. A stretched, stated overview is
 * honest; a true-aspect one that draws nothing is not.
 */

/** The window's floor, in GRAPH px — used when one viewport is small because
 *  the reader has zoomed in. Roughly four card heights. */
export const WINDOW_MIN_MARGIN = 500

/** How many extra viewports of margin the window carries on each side. One is
 *  the minimum that keeps a drag from outrunning the window between commits. */
export const WINDOW_VIEWPORTS = 1

/** The largest zoom `fitView` will choose on its own. */
export const FIT_MAX_Z = 1.4

/** The smallest zoom `fitView` will choose: below this a card's text is not
 *  text any more, so fitting past it buys a picture, not a reading. */
export const FIT_MIN_Z = 0.3

/** Overview cell edge, in overview px. Smaller is sharper and costs more
 *  rects; 2 keeps a 170x108 overview under ~4,600 cells before run-merging. */
export const MM_CELL = 2

/** No overview mark is ever drawn smaller than this, in overview px. */
export const MM_MIN_MARK = 2

/** The viewport rectangle's floor, in overview px, so "where am I" survives a
 *  graph 76 times taller than it is wide. */
export const MM_MIN_VIEWPORT = 4

/* ── 1. Windowing ────────────────────────────────────────────────────────── */

/**
 * The part of the graph the reader can actually see, in graph coordinates.
 *
 * @param {{x:number,y:number,z:number}} view
 * @param {number} cw canvas width in screen px
 * @param {number} ch canvas height in screen px
 * @returns {{x0:number,y0:number,x1:number,y1:number}}
 */
export function viewportBox(view, cw, ch) {
  const v = view || {}
  const z = v.z > 0 ? v.z : 1
  const x0 = -(v.x || 0) / z
  const y0 = -(v.y || 0) / z
  return { x0, y0, x1: x0 + (cw || 0) / z, y1: y0 + (ch || 0) / z }
}

/**
 * The part of the graph that must be MOUNTED: the viewport plus a margin of at
 * least one viewport, and never less than `WINDOW_MIN_MARGIN` graph px, in
 * every direction. Expressed in graph units so it does not shrink as the reader
 * zooms out.
 *
 * @param {{x:number,y:number,z:number}} view
 * @param {number} cw @param {number} ch
 * @param {{minMargin?:number, viewports?:number}} [opts]
 * @returns {{x0:number,y0:number,x1:number,y1:number}}
 */
export function windowBox(view, cw, ch, opts) {
  const o = opts || {}
  const vb = viewportBox(view, cw, ch)
  const k = o.viewports == null ? WINDOW_VIEWPORTS : o.viewports
  const min = o.minMargin == null ? WINDOW_MIN_MARGIN : o.minMargin
  const mx = Math.max(min, (vb.x1 - vb.x0) * k)
  const my = Math.max(min, (vb.y1 - vb.y0) * k)
  return { x0: vb.x0 - mx, y0: vb.y0 - my, x1: vb.x1 + mx, y1: vb.y1 + my }
}

/**
 * Do two axis-aligned boxes overlap? Touching edges count as overlapping — a
 * card exactly on the boundary is drawn rather than argued about.
 *
 * @param {{x0:number,y0:number,x1:number,y1:number}} a
 * @param {{x0:number,y0:number,x1:number,y1:number}} b
 * @returns {boolean}
 */
export function boxesIntersect(a, b) {
  if (!a || !b) return true
  return a.x0 <= b.x1 && a.x1 >= b.x0 && a.y0 <= b.y1 && a.y1 >= b.y0
}

/**
 * THE WINDOWING PREDICATE. `g` is a `geo()` box — `{x,y,w,h}`.
 *
 * @param {{x0:number,y0:number,x1:number,y1:number}} win
 * @param {{x:number,y:number,w:number,h:number}} g
 * @returns {boolean}
 */
export function nodeInWindow(win, g) {
  if (!g) return false
  return boxesIntersect(win, { x0: g.x, y0: g.y, x1: g.x + (g.w || 0), y1: g.y + (g.h || 0) })
}

/**
 * A wire's own bounding box, control points included. `logic.bez` pushes both
 * control points horizontally by `c = max(40, |dx| * 0.45)`, so the curve can
 * bulge outside the rectangle spanned by its endpoints. A wire is windowed by
 * THIS box and never by whether its endpoints are inside — a wire crossing the
 * viewport from off-screen to off-screen must still be drawn.
 *
 * @param {{x:number,y:number}} pa @param {{x:number,y:number}} pb
 * @returns {{x0:number,y0:number,x1:number,y1:number}}
 */
export function wireBox(pa, pb) {
  const c = Math.max(40, Math.abs(pb.x - pa.x) * 0.45)
  const xs = [pa.x, pb.x, pa.x + c, pb.x - c]
  return {
    x0: Math.min.apply(null, xs),
    x1: Math.max.apply(null, xs),
    y0: Math.min(pa.y, pb.y),
    y1: Math.max(pa.y, pb.y),
  }
}

/* ── 2. The overview ─────────────────────────────────────────────────────── */

/**
 * The mapping between graph coordinates and overview pixels. The two axes are
 * scaled INDEPENDENTLY; see this file's header for why, and `note` for what the
 * surface is obliged to say about it.
 *
 * @param {{x0:number,y0:number,x1:number,y1:number}} box the graph extent, padded
 * @param {number} w overview width in px @param {number} h overview height in px
 * @returns {{w:number,h:number,sx:number,sy:number,gw:number,gh:number,
 *            toX:(n:number)=>number, toY:(n:number)=>number,
 *            fromX:(n:number)=>number, fromY:(n:number)=>number}}
 */
export function minimapLayout(box, w, h) {
  const gw = Math.max(1, (box.x1 - box.x0) || 1)
  const gh = Math.max(1, (box.y1 - box.y0) || 1)
  const sx = w / gw
  const sy = h / gh
  return {
    w,
    h,
    sx,
    sy,
    gw,
    gh,
    toX: (n) => (n - box.x0) * sx,
    toY: (n) => (n - box.y0) * sy,
    fromX: (n) => n / sx + box.x0,
    fromY: (n) => n / sy + box.y0,
  }
}

/**
 * What one pixel of the overview stands for. A scale that is not stated is a
 * picture pretending to be a map.
 *
 * @param {{sx:number,sy:number}} layout
 * @returns {string}
 */
export function minimapNote(layout) {
  const down = Math.round(1 / (layout.sy || 1)).toLocaleString('en-US')
  const across = Math.round(1 / (layout.sx || 1)).toLocaleString('en-US')
  return (
    'Overview — stretched to fill, so it is not to scale: one pixel down is about ' +
    down +
    ' px of the trace, one across about ' +
    across +
    ' px. Each mark takes the worst state it covers, and the bright rectangle is what the canvas is showing.'
  )
}

/**
 * The same admission, short enough to print beside the overview itself. The
 * full sentence stays on the surface's `aria-label`.
 *
 * @param {{sx:number,sy:number}} layout
 * @returns {string}
 */
export function minimapCaption(layout) {
  const down = Math.round(1 / (layout.sy || 1)).toLocaleString('en-US')
  return 'Overview — stretched, not to scale: 1 px down ≈ ' + down + ' px of trace. Each mark is the worst state it covers.'
}

/**
 * Rasterise node boxes into overview cells, WORST WINS, then merge each row's
 * runs of equal rank into single rects.
 *
 * Every box lights at least one whole cell — that floor is the fix for the
 * defect this function exists to close, because a 110 px card in a 130,000 px
 * graph is 0.09 px of overview and a rect that small draws nothing at all. A
 * cell covered by several cards takes the LOWEST rank of them, so folding,
 * scaling and overlap can none of them quiet a negative (doctrine: nothing
 * carrying a negative state word, a finding, the stop or a destructive command
 * may be dropped from a count or drawn away).
 *
 * @param {Array<{x:number,y:number,w:number,h:number,rank:number}>} boxes graph units
 * @param {ReturnType<typeof minimapLayout>} layout
 * @param {{cell?:number}} [opts]
 * @returns {Array<{x:number,y:number,w:number,h:number,rank:number}>} overview px
 */
export function minimapCells(boxes, layout, opts) {
  const cell = (opts && opts.cell) || MM_CELL
  const cols = Math.max(1, Math.ceil(layout.w / cell))
  const rows = Math.max(1, Math.ceil(layout.h / cell))
  const grid = new Int16Array(cols * rows).fill(-1)
  const clampC = (n) => Math.max(0, Math.min(cols - 1, n))
  const clampR = (n) => Math.max(0, Math.min(rows - 1, n))
  ;(boxes || []).forEach((b) => {
    const rank = b.rank == null ? 9999 : b.rank
    const c0 = clampC(Math.floor(layout.toX(b.x) / cell))
    const c1 = clampC(Math.floor(layout.toX(b.x + (b.w || 0)) / cell))
    const r0 = clampR(Math.floor(layout.toY(b.y) / cell))
    const r1 = clampR(Math.floor(layout.toY(b.y + (b.h || 0)) / cell))
    for (let r = r0; r <= r1; r++) {
      for (let c = c0; c <= c1; c++) {
        const i = r * cols + c
        if (grid[i] < 0 || rank < grid[i]) grid[i] = rank
      }
    }
  })
  const out = []
  for (let r = 0; r < rows; r++) {
    let c = 0
    while (c < cols) {
      const rank = grid[r * cols + c]
      if (rank < 0) {
        c++
        continue
      }
      let end = c
      while (end + 1 < cols && grid[r * cols + end + 1] === rank) end++
      out.push({
        x: c * cell,
        y: r * cell,
        w: Math.max(MM_MIN_MARK, (end - c + 1) * cell),
        h: Math.max(MM_MIN_MARK, cell),
        rank,
      })
      c = end + 1
    }
  }
  return out
}

/**
 * One box drawn on the overview at a stated floor — the viewport rectangle and
 * the selected card both go through this, because both are single marks that a
 * true scale would render invisible.
 *
 * @param {{x:number,y:number,w:number,h:number}} g graph units
 * @param {ReturnType<typeof minimapLayout>} layout
 * @param {number} [minPx]
 * @returns {{x:number,y:number,w:number,h:number}} overview px, clipped to the box
 */
export function minimapMark(g, layout, minPx) {
  const min = minPx == null ? MM_MIN_VIEWPORT : minPx
  /* Floored so it cannot vanish, capped so it cannot spill: at Fit's 30% the
   * viewport is wider than the whole graph, and an uncapped rectangle drew
   * 260 px of overview into a 170 px box. */
  const w = Math.min(layout.w, Math.max(min, (g.w || 0) * layout.sx))
  const h = Math.min(layout.h, Math.max(min, (g.h || 0) * layout.sy))
  const x = Math.max(0, Math.min(layout.w - w, layout.toX(g.x)))
  const y = Math.max(0, Math.min(layout.h - h, layout.toY(g.y)))
  return { x, y, w, h }
}

/* ── 3. Fit ──────────────────────────────────────────────────────────────── */

/**
 * THE FIT MATHS. Returns the view that shows `box`, plus an honest account of
 * what it could not do.
 *
 * A fit that silently clamps is worse than one that refuses. On a session whose
 * graph is 76 times taller than it is wide, the zoom that would show everything
 * is ~1%, at which a card is a smear; the readable floor stops the zoom there
 * and the caller is told so. And when an axis does not fit, this aligns that
 * axis to the START of the graph rather than centring it — centring a 130,000 px
 * column drops the reader into the middle of a session with no way to tell
 * which middle, which is what "clamps at 30% and leaves 98.8% off-screen"
 * looked like from the reader's chair.
 *
 * @param {{x0:number,y0:number,x1:number,y1:number}} box
 * @param {number} cw @param {number} ch
 * @param {{pad?:number,minZ?:number,maxZ?:number,floorZ?:number}} [opts]
 *   `minZ` raises the floor for a caller that wants a closer look (`proof()`
 *   passes 0.85); `floorZ` is the readable floor itself.
 * @returns {{x:number,y:number,z:number,idealZ:number,clamped:boolean,
 *            fitsX:boolean,fitsY:boolean,shown:number}}
 *   `shown` is the fraction of the box's AREA inside the viewport at `z`.
 */
export function fitView(box, cw, ch, opts) {
  const o = opts || {}
  const pad = o.pad == null ? 40 : o.pad
  const floor = o.floorZ == null ? FIT_MIN_Z : o.floorZ
  const maxZ = o.maxZ == null ? FIT_MAX_Z : o.maxZ
  const bw = Math.max(1, box.x1 - box.x0)
  const bh = Math.max(1, box.y1 - box.y0)
  const idealZ = Math.min(cw / (bw + pad), ch / (bh + pad), maxZ)
  let z = Math.max(floor, idealZ)
  if (o.minZ) z = Math.max(z, o.minZ)
  const fitsX = (bw + pad) * z <= cw + 1e-9
  const fitsY = (bh + pad) * z <= ch + 1e-9
  const half = pad / 2
  const x = fitsX ? (cw - bw * z) / 2 - box.x0 * z : half - box.x0 * z
  const y = fitsY ? (ch - bh * z) / 2 - box.y0 * z : half - box.y0 * z
  const shownW = Math.min(1, cw / (bw * z))
  const shownH = Math.min(1, ch / (bh * z))
  return { x, y, z, idealZ, clamped: z > idealZ + 1e-9, fitsX, fitsY, shown: shownW * shownH }
}

/**
 * How many of `boxes` a given view actually puts on screen. `fitView` cannot
 * say "44 of 1,481 cards" on its own — the count is what makes the refusal
 * checkable rather than merely apologetic, so the caller measures it against
 * the same boxes it laid out.
 *
 * @param {Array<{x:number,y:number,w:number,h:number}>} boxes
 * @param {{x:number,y:number,z:number}} view
 * @param {number} cw @param {number} ch
 * @returns {number}
 */
export function countOnScreen(boxes, view, cw, ch) {
  const vb = viewportBox(view, cw, ch)
  let n = 0
  ;(boxes || []).forEach((g) => {
    if (nodeInWindow(vb, g)) n++
  })
  return n
}
