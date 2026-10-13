import test from 'node:test'
import assert from 'node:assert/strict'
import {
  FIT_MIN_Z,
  MM_MIN_MARK,
  MM_MIN_VIEWPORT,
  WINDOW_MIN_MARGIN,
  boxesIntersect,
  countOnScreen,
  fitView,
  minimapCells,
  minimapLayout,
  minimapMark,
  minimapCaption,
  minimapNote,
  nodeInWindow,
  viewportBox,
  windowBox,
  wireBox,
} from './viewport.js'

/* The pure half of the canvas fixes, pinned. Rule 9: every assertion below is
 * a defect that was measured on a long session (4,279 events, 1,481 cards),
 * so a regression here is a return of that defect, not a style change.
 *
 * The session's real shape, used as the fixture throughout: a graph about
 * 1,700 px wide and about 130,000 px tall, drawn into a 900x500 canvas and a
 * 170x108 overview.
 */

const TALL = { x0: 0, y0: 0, x1: 1700, y1: 130000 }
const CW = 900
const CH = 500

/* ── The windowing predicate ─────────────────────────────────────────────── */

test('the viewport box is the inverse of the transform the canvas draws with', () => {
  // The window and the transform string must be derived from the same `view`
  // in the same pass, or a card can vanish from inside the visible area.
  const vb = viewportBox({ x: -100, y: -200, z: 0.5 }, CW, CH)
  assert.deepEqual(vb, { x0: 200, y0: 400, x1: 200 + 1800, y1: 400 + 1000 })
})

test('the window is a STRICT SUPERSET of the viewport, by at least one viewport each way', () => {
  // This is the whole safety argument for windowing: a card that is dropped
  // is a card that was off-screen anyway.
  const view = { x: 40, y: 30, z: 0.8 }
  const vb = viewportBox(view, CW, CH)
  const win = windowBox(view, CW, CH)
  assert.ok(win.x0 < vb.x0 && win.y0 < vb.y0 && win.x1 > vb.x1 && win.y1 > vb.y1)
  assert.ok(vb.x0 - win.x0 >= vb.x1 - vb.x0)
  assert.ok(vb.y0 - win.y0 >= vb.y1 - vb.y0)
})

test('the margin never falls below its graph-px floor, however far the reader has zoomed in', () => {
  // At z = 2.5 one viewport is only 200 graph px; a window that shrank with
  // the zoom would be outrun by a single drag.
  const win = windowBox({ x: 0, y: 0, z: 2.5 }, CW, CH)
  const vb = viewportBox({ x: 0, y: 0, z: 2.5 }, CW, CH)
  assert.equal(vb.x0 - win.x0, WINDOW_MIN_MARGIN)
  assert.equal(vb.y0 - win.y0, WINDOW_MIN_MARGIN)
})

test('the margin is expressed in graph units, so it grows rather than shrinks as you zoom out', () => {
  const near = windowBox({ x: 0, y: 0, z: 1 }, CW, CH)
  const far = windowBox({ x: 0, y: 0, z: 0.25 }, CW, CH)
  assert.ok(far.x1 - far.x0 > near.x1 - near.x0)
})

test('a card touching the window edge is drawn, not argued about', () => {
  const win = { x0: 0, y0: 0, x1: 100, y1: 100 }
  assert.equal(nodeInWindow(win, { x: 100, y: 100, w: 10, h: 10 }), true)
  assert.equal(nodeInWindow(win, { x: 100.5, y: 0, w: 10, h: 10 }), false)
})

test('a card straddling the viewport from outside the window on both sides is still mounted', () => {
  // A tall frame-sized box that contains the whole window must not be culled
  // by an endpoint test.
  const win = { x0: 1000, y0: 1000, x1: 2000, y1: 2000 }
  assert.equal(nodeInWindow(win, { x: 0, y: 0, w: 5000, h: 5000 }), true)
})

test('a card outside the window is dropped — and that is the only thing windowing does', () => {
  const view = { x: 40, y: 30, z: 0.8 }
  const win = windowBox(view, CW, CH)
  const far = { x: 0, y: 90000, w: 320, h: 110 }
  assert.equal(nodeInWindow(win, far), false)
  // …but it is still on screen once the reader is taken there, which is what
  // `centerOn` does before the next commit windows again.
  const centred = { x: 900 / 2 - 160 * 0.8, y: 500 / 2 - (90000 + 55) * 0.8, z: 0.8 }
  assert.equal(nodeInWindow(windowBox(centred, CW, CH), far), true)
})

test('a wire is windowed by its own box, control points included, never by its endpoints', () => {
  // `bez` pushes both control points out by max(40, |dx| * 0.45), so a wire
  // between two vertically stacked pins bulges 40 px to the right of both.
  const b = wireBox({ x: 300, y: 100 }, { x: 300, y: 400 })
  assert.equal(b.x0, 260)
  assert.equal(b.x1, 340)
  assert.deepEqual([b.y0, b.y1], [100, 400])
  // A wire whose two endpoints are both off-screen but which crosses the view.
  const win = { x0: 0, y0: 200, x1: 1000, y1: 250 }
  assert.equal(boxesIntersect(win, wireBox({ x: 300, y: 100 }, { x: 300, y: 400 })), true)
})

/* ── The overview scale ──────────────────────────────────────────────────── */

test('a real card is a legible mark, not a sub-pixel rect — the whole of defect M2', () => {
  // Measured cause: 1,487 rects at ~0.09 px tall drew an empty box.
  const lay = minimapLayout(TALL, 170, 108)
  assert.ok(110 * lay.sy < 0.2, 'a card really is sub-pixel at true scale')
  const cells = minimapCells([{ x: 0, y: 64000, w: 320, h: 110, rank: 0 }], lay)
  assert.equal(cells.length, 1)
  assert.ok(cells[0].w >= MM_MIN_MARK && cells[0].h >= MM_MIN_MARK)
})

test('every card lights at least one cell, whatever the aspect ratio', () => {
  const shapes = [
    { x0: 0, y0: 0, x1: 1700, y1: 130000 },
    { x0: 0, y0: 0, x1: 90000, y1: 400 },
    { x0: 0, y0: 0, x1: 900, y1: 900 },
  ]
  shapes.forEach((box) => {
    const lay = minimapLayout(box, 170, 108)
    const cells = minimapCells([{ x: box.x0 + 10, y: box.y0 + 10, w: 1, h: 1, rank: 3 }], lay)
    assert.equal(cells.length, 1, 'aspect ' + (box.x1 - box.x0) + ':' + (box.y1 - box.y0))
  })
})

test('worst wins inside a cell — folding and scaling can never quiet a negative', () => {
  const lay = minimapLayout(TALL, 170, 108)
  // Two cards a few hundred graph px apart land in the same cell. Rank 0 is
  // the worst family.
  const cells = minimapCells(
    [
      { x: 0, y: 64000, w: 320, h: 110, rank: 6 },
      { x: 0, y: 64200, w: 320, h: 110, rank: 0 },
    ],
    lay,
  )
  assert.ok(cells.every((c) => c.rank === 0))
})

test('the overview run-merges, so a graph of 1,481 cards is a few hundred rects, not 1,481', () => {
  const lay = minimapLayout(TALL, 170, 108)
  const boxes = []
  for (let i = 0; i < 1481; i++) boxes.push({ x: 120, y: i * 124, w: 320, h: 110, rank: i % 7 })
  const cells = minimapCells(boxes, lay)
  assert.ok(cells.length < 200, 'rects drawn: ' + cells.length)
  assert.ok(cells.length > 0)
})

test('the viewport rectangle is visible on a graph 76 times taller than it is wide', () => {
  const lay = minimapLayout(TALL, 170, 108)
  const vp = minimapMark({ x: 0, y: 64000, w: CW / 0.8, h: CH / 0.8 }, lay)
  assert.ok(vp.h >= MM_MIN_VIEWPORT, 'height ' + vp.h)
  assert.ok(vp.w >= MM_MIN_VIEWPORT)
})

test('a mark is clipped into the overview, never drawn off its own edge', () => {
  const lay = minimapLayout(TALL, 170, 108)
  const vp = minimapMark({ x: 1690, y: 129990, w: 1000, h: 1000 }, lay)
  assert.ok(vp.x >= 0 && vp.y >= 0)
  assert.ok(vp.x + vp.w <= lay.w + 1e-9)
  assert.ok(vp.y + vp.h <= lay.h + 1e-9)
})

test('the overview states its own scale rather than implying one', () => {
  const lay = minimapLayout(TALL, 170, 108)
  const note = minimapNote(lay)
  assert.match(note, /not to scale/)
  assert.match(note, /1,2\d\d px of the trace/)
  assert.match(note, /worst state/)
  // …and it says so ON the surface, not only to a screen reader.
  const caption = minimapCaption(lay)
  assert.match(caption, /not to scale/)
  assert.match(caption, /worst state/)
  assert.ok(caption.length < note.length)
})

test('the viewport rectangle is capped to the overview, not just floored', () => {
  // At Fit's 30% the viewport is WIDER than the whole graph; an uncapped
  // rectangle drew 260 px of overview into a 170 px box.
  const lay = minimapLayout(TALL, 170, 108)
  const vp = minimapMark({ x: 0, y: 0, w: 2600, h: 1400 }, lay)
  assert.ok(vp.w <= lay.w && vp.h <= lay.h)
  assert.ok(vp.x + vp.w <= lay.w + 1e-9)
})

test('the overview mapping round-trips, so a click lands where it was aimed', () => {
  const lay = minimapLayout(TALL, 170, 108)
  assert.ok(Math.abs(lay.fromY(lay.toY(64000)) - 64000) < 1e-6)
  assert.ok(Math.abs(lay.fromX(lay.toX(820)) - 820) < 1e-6)
})

/* ── The fit maths ───────────────────────────────────────────────────────── */

test('a graph that fits is fitted and centred — Fit still does its job', () => {
  const box = { x0: 0, y0: 0, x1: 1200, y1: 600 }
  const f = fitView(box, CW, CH)
  assert.equal(f.clamped, false)
  assert.ok(f.fitsX && f.fitsY)
  assert.ok(Math.abs(f.shown - 1) < 1e-9)
  // centred: equal slack on both sides
  assert.ok(Math.abs(f.x - (CW - 1200 * f.z) / 2) < 1e-9)
})

test('Fit never zooms past the readable floor, and SAYS it clamped — defect M3', () => {
  const f = fitView(TALL, CW, CH)
  assert.equal(f.z, FIT_MIN_Z)
  assert.equal(f.clamped, true)
  assert.ok(f.idealZ < FIT_MIN_Z)
  assert.ok(f.shown < 0.02, 'shown ' + f.shown)
})

test('an axis that cannot fit is anchored to the start of the trace, not to its middle', () => {
  // The measured failure: "clamps at 30% and still leaves 98.8% off-screen"
  // dropped the reader into the middle of a 130,000 px column with nothing to
  // say which middle. The first recorded events are a place; the centre of a
  // column is not.
  const f = fitView(TALL, CW, CH)
  assert.ok(f.fitsX, 'the width does fit at the floor')
  assert.equal(f.fitsY, false)
  const vb = viewportBox(f, CW, CH)
  assert.ok(vb.y0 <= 0 && vb.y0 > -100, 'top of the graph is on screen: y0 = ' + vb.y0)
})

test('a caller that wants a closer look gets it, and the floor cannot lower it', () => {
  // `proof()` calls fit(nodes, 0.85).
  const box = { x0: 0, y0: 0, x1: 4000, y1: 4000 }
  assert.equal(fitView(box, CW, CH, { minZ: 0.85 }).z, 0.85)
  assert.ok(fitView(box, CW, CH).z >= FIT_MIN_Z)
})

test('Fit reports a count the reader can check, not just a percentage', () => {
  const boxes = []
  for (let i = 0; i < 1481; i++) boxes.push({ x: 120, y: i * 88, w: 320, h: 110 })
  const f = fitView({ x0: 120, y0: 0, x1: 440, y1: 1480 * 88 + 110 }, CW, CH)
  const n = countOnScreen(boxes, f, CW, CH)
  assert.ok(n > 0 && n < boxes.length, 'on screen: ' + n)
})

test('a degenerate box does not produce NaN — an empty reading must not break Fit', () => {
  const f = fitView({ x0: 0, y0: 0, x1: 0, y1: 0 }, CW, CH)
  assert.ok(Number.isFinite(f.x) && Number.isFinite(f.y) && Number.isFinite(f.z))
  const lay = minimapLayout({ x0: 0, y0: 0, x1: 0, y1: 0 }, 170, 108)
  assert.ok(Number.isFinite(lay.sx) && Number.isFinite(lay.sy))
  assert.deepEqual(minimapCells([], lay), [])
})
