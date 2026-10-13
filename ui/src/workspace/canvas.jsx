import React from 'react'
import { C, A, FONT, FS, LS, SIZE, RADIUS, SHADOW, Z, GRID, MOTION, sx, HOVER } from './tokens.js'
import { NO_CLAUSE, pinStyle, wordOf } from './logic.js'
import { prefersReducedMotion } from './reduced-motion.js'
import { Absence, Centred, Count, Kicker, Refusal, Significance, Skel, StateWord, Waiting } from './ui-kit.jsx'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'

/* ThruLine workspace — the centre column's five representations plus the
 * destinations drawer.
 *
 * Design range: spec-template §A lines 516-684.
 *   519-579  GraphCanvas (frames, wires, node cards, minimap, legend)
 *   582-596  Timeline
 *   599-616  RecordsTable
 *   619-639  ClaimDocument
 *   642-660  ComparePane
 *   663-682  DestinationsDrawer
 *
 * Exactly one of the five representations renders at a time; the drawer
 * overlays whichever is showing, inside the centre-content box, at z-index 30.
 *
 * THE REFUSAL RULE, and it applies to every component in this file: when the
 * record for a surface is absent or of an unknown shape, render
 * `<Refusal>` INSTEAD of the surface — never an empty canvas, never an empty
 * table. An empty graph is indistinguishable from a graph reporting nothing
 * (spec-data §E.2 rule 6), and that is the exact failure this product exists to
 * catch.
 *
 * ABSENT vs EMPTY, precisely, because the difference is the whole rule: an
 * ABSENT record is one whose collection is not an array at all — nothing was
 * adapted, nothing was read, the shape was refused upstream — and that renders
 * `<Refusal>`. An EMPTY record is an array of length zero: the reading happened
 * and found nothing, and that renders the surface plus the record's own note
 * (`emptyNote`, `note`, `foot`), which is what the design already carries.
 *
 * THE CLAUSE RULE: every state word on every surface here goes through
 * `<StateWord>` with its `clause`. The design renders words with no clause on
 * eleven surfaces and the node card is the worst of them, because it is the
 * hero surface (spec-doctrine C-1).
 *
 * STYLE CONVENTION (tokens.js `sx`): every style is a frozen module-level
 * constant merged by `sx(...)`; conditional layers are `cond && LAYER`. Nothing
 * in this file writes a hex, a size, a radius, a shadow, a z-index or a grid
 * template — they all come from `./tokens.js`.
 */

/* ── Graph canvas (:520-577) ─────────────────────────────────────────────── */

const CANVAS = Object.freeze({
  flex: 1,
  position: 'relative',
  overflow: 'hidden',
  backgroundColor: C.canvas,
  backgroundImage: `linear-gradient(${A.grid} 1px, transparent 1px), linear-gradient(90deg, ${A.grid} 1px, transparent 1px)`,
  cursor: 'grab',
})
const XFORM = Object.freeze({ position: 'absolute', left: 0, top: 0, transformOrigin: '0 0', willChange: 'transform' })
/* `display:none`, not `visibility:hidden` and not a z-index: it takes the layer
 * out of hit-testing and out of the accessibility tree together, which is what
 * an overlaid empty state needs (see `overlay` in `GraphCanvas`). */
const HIDDEN = Object.freeze({ display: 'none' })
const FRAME = Object.freeze({
  position: 'absolute',
  background: A.frameFill,
  border: `1px solid ${A.frameBorder}`,
  borderRadius: RADIUS.ctl,
  pointerEvents: 'none',
})
const FRAME_HEAD = Object.freeze({
  height: 24,
  display: 'flex',
  alignItems: 'center',
  gap: '8px',
  padding: '0 9px',
  background: A.frameHeader,
  borderBottom: `1px solid ${A.frameHeaderUnderline}`,
  fontSize: FS.ctl,
  fontWeight: 600,
  letterSpacing: LS.frame,
  textTransform: 'uppercase',
  color: C.ink,
})
const FRAME_COUNT = Object.freeze({
  fontWeight: 400,
  textTransform: 'none',
  letterSpacing: 0,
  color: C.inkDim,
  fontFamily: FONT.mono,
})
const WIRE_SVG = Object.freeze({
  position: 'absolute',
  left: 0,
  top: 0,
  width: SIZE.wireSvgW,
  height: SIZE.wireSvgH,
  overflow: 'visible',
  pointerEvents: 'none',
})
const WIRE_PATH = Object.freeze({ fill: 'none', pointerEvents: 'stroke', cursor: 'pointer', strokeLinecap: 'round' })
const WIRE_LABEL = Object.freeze({
  font: `${FS.meta} ${FONT.mono}`,
  fill: C.ink,
  paintOrder: 'stroke',
  stroke: C.canvas,
  strokeWidth: '4px',
  strokeLinejoin: 'round',
  pointerEvents: 'none',
})
const CANVAS_NOTE = Object.freeze({
  position: 'absolute',
  left: 12,
  top: 10,
  fontSize: FS.ctl,
  color: C.inkMute,
  background: A.noteGround,
  border: `1px solid ${C.line1}`,
  borderRadius: RADIUS.ctl,
  padding: '5px 8px',
  maxWidth: SIZE.canvasNoteMax,
})
const MINIMAP = Object.freeze({
  position: 'absolute',
  right: 10,
  bottom: 10,
  width: SIZE.minimapW,
  height: SIZE.minimapH,
  background: A.minimapGround,
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.ctl,
  cursor: 'crosshair',
})
/* The overview is drawn in ITS OWN pixel space now, not in graph coordinates
 * (`viewport.minimapLayout`), so these stroke widths are overview px. They used
 * to be 6 and 8 GRAPH px, which on a ~130,000 px-tall session was invisible for
 * the same reason every node rect was: the surface was scaled by ~0.0008. */
const MM_FRAME = Object.freeze({ fill: A.frameFill, stroke: A.minimapFrameStroke, strokeWidth: '1px' })
const MM_VIEWPORT = Object.freeze({ fill: A.minimapViewport, stroke: C.focus, strokeWidth: '1.5px' })
const MM_SEL = Object.freeze({ fill: C.amber, stroke: C.amber, strokeWidth: '1px' })
/* The scale, said out loud. An overview that is not to scale and does not admit
 * it is a picture pretending to be a map. */
const MM_CAPTION = Object.freeze({
  position: 'absolute',
  right: 10,
  bottom: 10 + SIZE.minimapH + 4,
  width: SIZE.minimapW,
  fontSize: FS.cap,
  lineHeight: 1.35,
  color: C.inkDim,
  background: A.noteGround,
  border: `1px solid ${C.line1}`,
  borderRadius: RADIUS.ctl,
  padding: '3px 6px',
  pointerEvents: 'none',
  boxSizing: 'border-box',
})
const LEGEND = Object.freeze({
  position: 'absolute',
  left: 10,
  bottom: 10,
  display: 'flex',
  gap: '6px',
  alignItems: 'center',
  fontSize: FS.cap,
  color: C.inkDim,
  background: A.noteGround,
  border: `1px solid ${C.line1}`,
  borderRadius: RADIUS.ctl,
  padding: '3px 8px',
  pointerEvents: 'none',
})
const LEGEND_RULE = Object.freeze({ display: 'inline-block', width: 16, height: 0 })
/* P6-3: the legend is a trust feature (§3.3) — always drawn, and it answers a
 * pointer so each entry's hint can be read. Each sample is the line kind's own
 * stroke, so the legend and the lines cannot disagree. */
const LEGEND_LIVE = Object.freeze({ pointerEvents: 'auto', flexWrap: 'wrap', maxWidth: 'calc(100% - 20px - ' + SIZE.minimapW + 'px)' })
const LEGEND_ITEM = Object.freeze({ display: 'inline-flex', alignItems: 'center', gap: '5px', marginRight: 8 })
const LANE_LABEL = Object.freeze({
  position: 'absolute',
  left: 8,
  fontSize: FS.ctl,
  fontWeight: 600,
  letterSpacing: LS.frame,
  textTransform: 'uppercase',
  color: C.ink,
  background: A.noteGround,
  border: `1px solid ${C.line1}`,
  borderRadius: RADIUS.ctl,
  padding: '2px 7px',
  pointerEvents: 'none',
  whiteSpace: 'nowrap',
})
const THREAD_BAR = Object.freeze({
  position: 'absolute',
  left: '50%',
  top: 10,
  transform: 'translateX(-50%)',
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: '8px',
  maxWidth: '70%',
  fontSize: FS.ctl,
  color: C.ink,
  background: A.noteGround,
  border: `1px solid ${C.amber}`,
  borderRadius: RADIUS.ctl,
  padding: '4px 8px',
})
const THREAD_BAR_MISSING = Object.freeze({ color: C.inkSoft })
const THREAD_BAR_BTN = Object.freeze({
  background: 'transparent',
  color: C.ink,
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.ctl,
  padding: '1px 8px',
  fontSize: FS.ctl,
  cursor: 'pointer',
})

/** One line kind's sample: the same stroke the lines on the picture use. */
function LineSample({ color, dash, cap }) {
  return (
    <svg width="24" height="8" aria-hidden="true" style={{ flex: '0 0 24px' }}>
      <line x1="2" y1="4" x2="22" y2="4" stroke={color} strokeWidth="2" strokeDasharray={dash === 'none' ? undefined : dash} strokeLinecap={cap || 'butt'} />
    </svg>
  )
}

/**
 * The graph viewport (spec-template :519-579). `role="application"`, the grid
 * ground, pan/zoom/drag, and the transform layer holding frames, wires and node
 * cards.
 *
 * The wheel handler is registered as a NON-PASSIVE native listener on
 * `canvasRef`, not as the JSX `onWheel` prop: React attaches wheel passively, so
 * a `preventDefault()` inside a JSX handler is a no-op with a console warning
 * and the page zooms instead of the canvas (spec-template Decision D8,
 * spec-script §C.6).
 *
 * GEOMETRY IS NOT DERIVED HERE. Every `x`/`y`/`w`/`h`, every wire `d` and every
 * transform arrives already computed by the root through
 * `logic.LAYOUT/estH/geo/frames/pinPos/allWires` (spec-script §B.4). This
 * component positions what it is given and nothing else — the one exception is
 * a pin's own 9px square appearance, which is `logic.pinStyle` applied to the
 * pin, and a pin row's `i * 20` offset (spec-script §B.4 #28), both of which are
 * appearance rather than layout.
 *
 * @param {object} props
 * @param {React.RefObject<HTMLDivElement>} props.canvasRef measured by the
 *   root's `measure()`; the ONLY element the root reads geometry from
 * @param {{transform:string, gridSize:string, gridPos:string}} props.view
 * @param {Array<{id:string,title:string,count:string,x:number,y:number,w:number,h:number}>} props.frames
 *   `count` is a rendered phrase (`'3 objects'`), never a naked number. A frame
 *   with zero visible nodes is not in this array at all.
 * @param {Array<object>} props.wires each `{id,d,color,width,dash,opacity,showLabel,lx,ly,label}`
 * @param {Array<object>} props.nodes each carrying `{id,x,y,w,opacity,border,shadow,hdr,hdrText,
 *   type,sub,title,titleFont,lines[],foot,pinsL[],pinsR[],pinsH,aria,sel,
 *   wordSlug,clause,hdrRight,group,members}`
 * @param {string|null} props.emptyNote the floating note over the canvas; when
 *   the record is ABSENT rather than empty, `<Refusal>` renders instead of the
 *   whole surface
 * @param {string|null} [props.reading] the address being read, when a read is in
 *   flight. The grid paints and a `<Waiting>` centres over it — never node
 *   silhouettes, because a graph skeleton draws fake STRUCTURE, which is worse
 *   than a fake count.
 * @param {object|null} [props.absence] `<Absence>` props for a record that was
 *   read and holds no events. Centred over the grid so the reader can see they
 *   are still on Trace. This is the cell that used to draw a blank grid with no
 *   sentence at all.
 * @param {object|null} props.minimap `{viewBox, frames[], nodes[], viewport}`; null hides it
 * @param {string} props.replayNote the legend's fourth line
 * @param {Record<string, Function>} props.on `canvasDown(e) canvasWheel(e) onFit()
 *   onNodeDown(id,e) onNodeKey(id,e) onNodeDouble(id) onNodeToggle(id)
 *   onWireEnter(id) onWireLeave(id) onWireClick(id) onMinimapDown(e)`
 * @returns {JSX.Element}
 */
export function GraphCanvas({ canvasRef, view, frames, wires, nodes, emptyNote, reading, absence, minimap, replayNote, windowNote, legend, legendThread, lanes, threadBar, onShowAll, on }) {
  const handlers = on || {}
  const wheel = handlers.canvasWheel

  /* Non-passive wheel: the JSX `onWheel` prop cannot `preventDefault()`
   * (spec-template Decision D8). */
  React.useEffect(() => {
    const el = canvasRef && canvasRef.current
    if (!el || typeof wheel !== 'function') return undefined
    el.addEventListener('wheel', wheel, { passive: false })
    return () => el.removeEventListener('wheel', wheel, { passive: false })
  }, [canvasRef, wheel])

  if (!Array.isArray(nodes) || !Array.isArray(frames) || !Array.isArray(wires)) {
    return (
      <Refusal
        title="This session has no trace to draw."
        missing="the adapted session's nodes, frames and wires — nothing was read for this reading"
        remedy="thruline export --all"
      />
    )
  }

  const v = view || { transform: 'translate(0px,0px) scale(1)', gridSize: '40px 40px', gridPos: '0px 0px' }

  /* AN OVERLAY IS NOT A LAYER OVER SOMETHING ELSE TO READ. `reading` and
   * `absence` were painted as centred blocks WHILE the transform layer and the
   * minimap kept drawing underneath them, so on the one screen the emptiness
   * copy was written for — a session with no recorded events, whose single
   * DERIVED `ending` node the adapter synthesises anyway — the node card and
   * the minimap collided with the panel and both texts were cut off in the
   * overlap band ("CLAIMS & EV…" over "no recorded ev…"). The grid keeps
   * painting, because that is how a reader knows they are still on Trace; the
   * contents do not, because there is nothing behind the panel worth reading
   * and a card asserting one object beside a sentence saying the record holds
   * no recorded events reads as a contradiction. */
  const overlay = reading ? 'reading' : absence ? 'absence' : null
  /* T-6-4: under reduced motion the parts this file draws are drawn FINISHED —
   * a thread line whole with no drawing mask, a card with no transition —
   * rather than animated and then undone by the stylesheet. */
  const still = prefersReducedMotion()

  return (
    <div
      ref={canvasRef}
      role="application"
      aria-label={
        'Trace graph — drag to pan, wheel to zoom, drag nodes to rearrange (layout only). ' +
        (windowNote || 'Every card in scope is drawn.') +
        ' Tab moves to the selected card; use find (Command-K) or the overview to reach a card that is not drawn.'
      }
      onMouseDown={handlers.canvasDown}
      onDoubleClick={handlers.onFit}
      style={sx(CANVAS, { backgroundSize: v.gridSize, backgroundPosition: v.gridPos })}
    >
      <div style={sx(XFORM, { transform: v.transform }, overlay && HIDDEN)}>
        {frames.map((f) => (
          <div key={f.id} data-lane={f.lane || undefined} style={sx(FRAME, { left: f.x, top: f.y, width: f.w, height: f.h })}>
            {/* A lane band's name and count are drawn at the left edge of the
                VIEW (`lanes` below), so they never scroll away; the band here
                draws no head of its own. */}
            {f.bare ? null : (
              <div style={FRAME_HEAD}>
                <span>{f.title}</span>
                <span style={FRAME_COUNT}>{f.count}</span>
              </div>
            )}
          </div>
        ))}

        <svg aria-hidden="true" style={WIRE_SVG}>
          {wires.map((w) => (
            <g key={w.id} data-edge={w.kind || undefined} data-lit={w.lit ? 'yes' : undefined}>
              {/* A thread line DRAWS once, from its cause end to its effect
                  end (the path runs from the wire's `from` card to its `to`
                  card), through a mask so the kind's own dash pattern is kept.
                  With reduced motion the mask is simply whole. */}
              {w.lit && w.box && !still ? (
                <mask
                  id={'ws-draw-' + w.id}
                  maskUnits="userSpaceOnUse"
                  x={w.box ? w.box.x0 - 12 : 0}
                  y={w.box ? w.box.y0 - 12 : 0}
                  width={w.box ? w.box.x1 - w.box.x0 + 24 : 0}
                  height={w.box ? w.box.y1 - w.box.y0 + 24 : 0}
                >
                  {/* `white` is the mask's full-coverage value, not a paint:
                      nothing on screen is drawn in it. */}
                  <path d={w.d} pathLength="1" className="ws-trace-draw" fill="none" stroke="white" strokeWidth="8" />
                </mask>
              ) : null}
              <path
                d={w.d}
                mask={w.lit && w.box && !still ? 'url(#ws-draw-' + w.id + ')' : undefined}
                onMouseEnter={() => handlers.onWireEnter && handlers.onWireEnter(w.id)}
                onMouseLeave={() => handlers.onWireLeave && handlers.onWireLeave(w.id)}
                onClick={() => handlers.onWireClick && handlers.onWireClick(w.id)}
                style={sx(WIRE_PATH, {
                  stroke: w.color,
                  strokeWidth: `${w.width}px`,
                  strokeDasharray: w.dash,
                  strokeLinecap: w.cap || 'round',
                  opacity: w.opacity,
                })}
              />
              {w.showLabel ? (
                <text x={w.lx} y={w.ly} textAnchor="middle" style={WIRE_LABEL}>
                  {w.label}
                </text>
              ) : null}
            </g>
          ))}
        </svg>

        {/* NO FRESH CLOSURES HERE. `NodeCard` is `React.memo`'d and the node
            view-models are memoised upstream on everything except `view`, so a
            pan re-renders zero cards — but only while these four props keep
            their identity. The card carries `data-id` and the root's handlers
            read it back through `idEvt`, which has accepted that shape since it
            was written. Four fresh arrows per card per render would defeat the
            memo entirely and quietly. */}
        {nodes.map((n) => (
          <NodeCard
            key={n.id}
            node={n}
            selected={!!n.sel}
            tabbable={!!n.tabbable}
            still={still}
            onDown={handlers.onNodeDown}
            onKey={handlers.onNodeKey}
            onDouble={handlers.onNodeDouble}
            onToggle={handlers.onNodeToggle}
          />
        ))}
      </div>

      {overlay || !emptyNote ? null : <div style={CANVAS_NOTE}>{emptyNote}</div>}

      {!overlay && Array.isArray(lanes)
        ? lanes.map((l) => (
            <div key={l.id} className="ws-trace-lane-label" data-lane={l.id} style={sx(LANE_LABEL, { top: l.top + 4 })}>
              {l.label}
              {l.count ? <span style={sx(FRAME_COUNT, { marginLeft: 8 })}>{l.count}</span> : null}
            </div>
          ))
        : null}

      {!overlay && threadBar ? (
        <div className="ws-trace-thread-bar" role="status" aria-live="polite" style={THREAD_BAR}>
          <strong>{threadBar.line}</strong>
          {threadBar.missing.map((m) => (
            <span key={m} style={THREAD_BAR_MISSING}>{m}</span>
          ))}
          <button type="button" className="ws-hv" title={threadBar.showAllHint} onClick={onShowAll} style={THREAD_BAR_BTN}>
            {threadBar.showAll}
          </button>
        </div>
      ) : null}

      {/* The grid keeps painting behind both of these — a reader must be able
          to see they are still on Trace. `reading` wins over `absence`: while a
          read is in flight nothing may be said about what the record holds. */}
      {overlay === 'reading' ? (
        <Centred>
          <Waiting endpoint={reading} />
        </Centred>
      ) : overlay === 'absence' ? (
        <Centred>
          <Absence {...absence} />
        </Centred>
      ) : null}

      {/* THE OVERVIEW (M2). It used to map every node straight into a viewBox
          in GRAPH coordinates, which on the measured session drew 1,487 rects
          about 0.09 px tall inside a 170x108 box — an empty rectangle where the
          only navigation aid for a 130,000 px-tall graph should be. It is now
          drawn in its own pixel space by `viewport.minimapLayout` +
          `minimapCells`: the two axes are scaled independently, every mark has
          a floor of one whole cell, a cell holding several cards takes the
          WORST of them, and the caption states the scale rather than implying
          one. It is built from the UNWINDOWED node list, so windowing the
          canvas never shrinks the overview. */}
      {minimap && !overlay ? <div style={MM_CAPTION}>{minimap.caption}</div> : null}
      {minimap && !overlay ? (
        <svg
          role="img"
          aria-label={minimap.note}
          viewBox={`0 0 ${minimap.w} ${minimap.h}`}
          preserveAspectRatio="none"
          onMouseDown={handlers.onMinimapDown}
          style={MINIMAP}
        >
          {(minimap.frames || []).map((f, i) => (
            <rect key={f.id || `mmf${i}`} x={f.x} y={f.y} width={f.w} height={f.h} style={MM_FRAME} />
          ))}
          {(minimap.cells || []).map((c, i) => (
            <rect key={`mmc${i}`} x={c.x} y={c.y} width={c.w} height={c.h} style={{ fill: c.fill }} />
          ))}
          {minimap.sel ? (
            <rect x={minimap.sel.x} y={minimap.sel.y} width={minimap.sel.w} height={minimap.sel.h} style={MM_SEL} />
          ) : null}
          {minimap.viewport ? (
            <rect
              x={minimap.viewport.x}
              y={minimap.viewport.y}
              width={minimap.viewport.w}
              height={minimap.viewport.h}
              style={MM_VIEWPORT}
            />
          ) : null}
        </svg>
      ) : null}

      <div className="ws-trace-legend" role="note" style={sx(LEGEND, Array.isArray(legend) && LEGEND_LIVE)}>
        {Array.isArray(legend) ? (
          <React.Fragment>
            {legend.map((e) => (
              <span key={e.kind} className="ws-trace-legend-item" data-edge={e.kind} title={e.hint} style={LEGEND_ITEM}>
                <LineSample color={e.color} dash={e.dash} cap={e.cap} />
                <span>{e.label}</span>
              </span>
            ))}
            {legendThread ? (
              <span className="ws-trace-legend-item" data-edge="thread" title={legendThread.hint} style={LEGEND_ITEM}>
                <LineSample color={C.amber} dash="none" cap="round" />
                <span>{legendThread.label}</span>
              </span>
            ) : null}
          </React.Fragment>
        ) : (
          <React.Fragment>
            <span style={sx(LEGEND_RULE, { borderTop: `2px solid ${C.wireRecorded}` })} />
            <span>recorded order</span>
            <span style={sx(LEGEND_RULE, { borderTop: `2px solid ${C.amber}`, marginLeft: 6 })} />
            <span>documented reference</span>
            <span style={sx(LEGEND_RULE, { borderTop: `2px dashed ${C.wireInferred}`, marginLeft: 6 })} />
            <span>inferred (by text or order)</span>
          </React.Fragment>
        )}
        <span style={{ marginLeft: 8, color: C.inkFaint }}>{replayNote}</span>
        {/* The window, stated. The bottom dock has said `Showing N of M` about
            its own ±40 event window since it was written; drawing fewer cards
            than the record holds without saying so would be the same silence
            this product exists to break. The count itself is unchanged —
            `drawnCards` above the canvas still counts every card in scope. */}
        {windowNote ? <span style={{ marginLeft: 8, color: C.inkFaint }}>{windowNote}</span> : null}
      </div>
    </div>
  )
}

/* ── Node card (:537-562) ────────────────────────────────────────────────── */

const NODE = Object.freeze({
  position: 'absolute',
  background: C.surf2,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  fontSize: FS.body,
  userSelect: 'none',
  cursor: 'default',
  outlineOffset: '2px',
  transition: `opacity ${MOTION.quick} ${MOTION.ease}`,
})
/* Reduced motion (T-6-4): the card is simply at its opacity. */
const NODE_STILL = Object.freeze({ transition: 'none' })
const NODE_HEAD = Object.freeze({
  height: 30,
  display: 'flex',
  alignItems: 'center',
  gap: '6px',
  padding: '0 8px',
  borderBottom: `1px solid ${A.nodeHeaderUnderline}`,
  borderRadius: RADIUS.nodeHeader,
  cursor: 'move',
})
/* T-6-4: no opacity on the header's words. At 0.75 the kind label fell below
 * 4.5:1 on four of the seven family grounds (3.56 on green); at full ink every
 * family passes (lowest 4.95). `contrast.test.js` holds it. */
const NODE_TYPE = Object.freeze({
  font: `600 ${FS.micro} ${FONT.mono}`,
  letterSpacing: LS.nodeType,
  whiteSpace: 'nowrap',
})
const NODE_SUB = Object.freeze({
  fontSize: FS.ctl,
  fontWeight: 600,
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  flex: 1,
})
const NODE_HDR_RIGHT = Object.freeze({ font: `${FS.meta} ${FONT.mono}`, whiteSpace: 'nowrap' })
/* The toggle drawn beside its card, right-aligned to the card's header; and
 * the invisible twin that keeps its place in the header. */
const NODE_TOGGLE_OVER = Object.freeze({ position: 'absolute', transform: 'translateX(-100%)', boxSizing: 'border-box', whiteSpace: 'nowrap' })
const NODE_TOGGLE_TWIN = Object.freeze({ visibility: 'hidden', display: 'inline-flex', alignItems: 'center', boxSizing: 'border-box', whiteSpace: 'nowrap', flexShrink: 0 })
const NODE_TOGGLE = Object.freeze({
  height: SIZE.nodeToggle,
  padding: '0 5px',
  background: A.nodeToggle,
  border: `1px solid ${A.nodeToggleBorder}`,
  borderRadius: RADIUS.pill,
  color: C.white,
  fontSize: FS.meta,
  cursor: 'pointer',
  fontFamily: FONT.mono,
})
const PIN_STRIP = Object.freeze({ position: 'relative' })
const PIN_ROW = Object.freeze({ position: 'absolute', height: 20, display: 'flex', alignItems: 'center', gap: '5px' })
const PIN_ROW_L = Object.freeze({ left: -5 })
const PIN_ROW_R = Object.freeze({ right: -5, flexDirection: 'row-reverse' })
const PIN_SQUARE = Object.freeze({ width: 9, height: 9, flex: '0 0 9px', boxSizing: 'border-box' })
const PIN_LABEL = Object.freeze({ fontSize: FS.cap, color: C.inkSoft, whiteSpace: 'nowrap' })
const NODE_BODY = Object.freeze({ padding: '2px 8px 6px' })
const NODE_TITLE = Object.freeze({
  fontSize: FS.body,
  fontWeight: 500,
  lineHeight: 1.3,
  color: C.inkBright,
  wordBreak: 'break-word',
  textWrap: 'pretty',
})
const NODE_LINE = Object.freeze({ fontSize: FS.sm, lineHeight: 1.3, marginTop: 3, wordBreak: 'break-word' })
const NODE_CLAUSE = Object.freeze({
  fontSize: FS.sm,
  lineHeight: 1.3,
  marginTop: 3,
  marginLeft: 8,
  color: C.inkMute,
  fontStyle: 'normal',
  wordBreak: 'break-word',
})
/* P6-3: where a run ran, one line per typed field — never wrapped, so
 * `trace-view.EXTRA_LINE_H` budgets the card exactly. */
const FACT_LINE = Object.freeze({
  display: 'flex',
  gap: '6px',
  height: 16,
  lineHeight: '16px',
  fontSize: FS.sm,
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
})
const FACT_LABEL = Object.freeze({ color: C.inkSoft, flex: '0 0 auto' })
const FACT_VALUE = Object.freeze({ color: C.ink, overflow: 'hidden', textOverflow: 'ellipsis' })
const FACT_ABSENT = Object.freeze({ color: C.inkSoft })
/* "Ran on an earlier version": caution yellow, and the words carry it. */
const FACT_BAND = Object.freeze({ color: C.yellow, border: `1px solid ${C.yellow}`, borderRadius: RADIUS.ctl, padding: '0 5px', display: 'inline-block', lineHeight: '14px' })
/* The agent's own words are violet (§4), and say what they are. */
const AGENT_LINE = Object.freeze({ color: C.violet })
/* T-6-4: the position sits on its own black ground. Amber straight on a
 * family header fell to 3.13:1 on green; on black it is 10.29. */
const THREAD_MARK = Object.freeze({ font: `${FS.meta} ${FONT.mono}`, color: C.amber, background: C.bgApp, borderRadius: RADIUS.ctl, padding: '0 4px', whiteSpace: 'nowrap' })
const NODE_EXTRA = Object.freeze({ marginTop: 4 })

const NODE_FOOT = Object.freeze({
  borderTop: `1px solid ${C.line1}`,
  padding: '3px 8px',
  font: `${FS.meta} ${FONT.mono}`,
  color: C.inkDim,
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
})

/**
 * One node card (spec-template :537-562). Header (type · sub · state pill ·
 * right detail · group toggle) → pin strip → body (title + lines + clause) →
 * foot.
 *
 * A node's state word renders through `<StateWord variant="chip">` and the
 * card's body carries the clause as its own line under `lines[]` — the pill at
 * :542 is a `white-space: nowrap` header chip and a sentence cannot live in it.
 * The design paints that pill with no clause anywhere on the card, across 11
 * cards in 3 sessions (spec-doctrine C-1, F-2).
 *
 * A FINDING NODE HAS NO STATE WORD. When `node.severity` is present the card
 * renders `<Significance>` in place of the pill and nothing in the word slot.
 * The design puts the severity label in the word slot with Not proven's own
 * glyph and then adds the claim's word beneath, which is two words on one item
 * (spec-doctrine F-4, C20).
 *
 * The group toggle's label carries a noun: `▸ 3 events`, never `▸ 3`
 * (spec-doctrine D-3, F-7); the root composes `node.toggleLabel` with
 * `countPhrase` and this card renders it verbatim.
 *
 * Node fields read here, and nowhere else in this file:
 * `id x y w opacity border shadow hdr hdrText type sub hdrRight group
 *  toggleLabel toggleAria wordSlug clause severity pinsH pinsL pinsR title
 *  titleFont lines foot aria`.
 *
 * @param {object} props
 * @param {object} props.node the node view-model described in `GraphCanvas.nodes`
 * @param {boolean} props.selected sets `aria-pressed`
 * @param {(e: MouseEvent) => void} props.onDown drag start; the group toggle
 *   swallows it via its own `onMouseDown`
 * @param {(e: KeyboardEvent) => void} props.onKey Enter/Space selects and
 *   centres; arrows nudge `pos[id]` by 10px, or 1px with Shift
 * @param {() => void} props.onDouble
 * @param {(() => void)|null} props.onToggle null for a non-group node
 * @returns {JSX.Element}
 */
function NodeCardImpl({ node, selected, tabbable, still, onDown, onKey, onDouble, onToggle }) {
  const n = node || {}
  const lines = Array.isArray(n.lines) ? n.lines : []
  const pinsL = Array.isArray(n.pinsL) ? n.pinsL : []
  const pinsR = Array.isArray(n.pinsR) ? n.pinsR : []
  const w = wordOf(n.wordSlug)
  /* A finding never reaches the word slot (F-4); a negative word never reaches
   * the body without its clause (C-1). */
  const showWord = !n.severity && !!w.slug
  const clauseLine = showWord && w.negative ? n.clause || NO_CLAUSE : null

  /* N13-F11-AXE-SR (axe nested-interactive): a group's open/close button is
   * drawn BESIDE the card, over its header, not inside the card's own
   * role="button"; a hidden twin in the header keeps its width. */
  const toggle = n.group && onToggle ? (
    <button
      type="button"
      className="ws-hv"
      data-id={n.id}
      tabIndex={tabbable ? 0 : -1}
      aria-label={n.toggleAria}
      onMouseDown={(e) => {
        e.stopPropagation()
      }}
      onClick={onToggle}
      style={sx(NODE_TOGGLE, HOVER.overlayCtl(), NODE_TOGGLE_OVER, { left: n.x + n.w - 9, top: n.y + 1 + (NODE_HEAD.height - SIZE.nodeToggle) / 2, opacity: n.opacity })}
    >
      {n.toggleLabel}
    </button>
  ) : null

  return (
    <React.Fragment>
    <div
      role="button"
      data-id={n.id}
      /* ROVING TABINDEX. Every card used to be `tabIndex={0}`, so Tab walked
       * all 1,481 of them; with the canvas windowed it would have walked only
       * what happened to be mounted, and — the real hazard — a card holding
       * focus could be unmounted by a pan, dropping focus to `<body>` and
       * stranding the reader with no way back. One tab stop, on the selected
       * card, which the root pins into the window whatever the view. */
      tabIndex={tabbable ? 0 : -1}
      aria-label={n.aria}
      aria-pressed={selected ? 'true' : 'false'}
      onMouseDown={onDown}
      onKeyDown={onKey}
      onDoubleClick={onDouble}
      data-lit={n.lit ? 'yes' : undefined}
      data-hollow={n.hollow ? 'yes' : undefined}
      style={sx(NODE, {
        left: n.x,
        top: n.y,
        width: n.w,
        opacity: n.opacity,
        /* A run with no recorded result is HOLLOW: no ground, a dashed edge. */
        border: `1px ${n.hollow ? 'dashed' : 'solid'} ${n.border}`,
        background: n.hollow ? 'transparent' : undefined,
        boxShadow: n.shadow,
      }, still ? NODE_STILL : null)}
    >
      <div style={sx(NODE_HEAD, { background: n.hdr })}>
        <span style={sx(NODE_TYPE, { color: n.hdrText })}>{n.type}</span>
        <span style={sx(NODE_SUB, { color: n.hdrText })}>{n.sub}</span>
        {n.severity ? <Significance severity={n.severity} /> : null}
        {showWord ? <StateWord slug={n.wordSlug} clause={n.clause} variant="chip" /> : null}
        {n.threadMark ? <span className="ws-trace-mark" style={THREAD_MARK}>{n.threadMark}</span> : null}
        {n.hdrRight ? <span style={sx(NODE_HDR_RIGHT, { color: n.hdrText })}>{n.hdrRight}</span> : null}
        {toggle ? <span style={sx(NODE_TOGGLE, NODE_TOGGLE_TWIN)} aria-hidden="true">{n.toggleLabel}</span> : null}
      </div>

      <div style={sx(PIN_STRIP, { height: n.pinsH })}>
        {pinsL.map((p, i) => (
          <Pin key={p.id || `l${i}`} pin={p} index={i} side="left" />
        ))}
        {pinsR.map((p, i) => (
          <Pin key={p.id || `r${i}`} pin={p} index={i} side="right" />
        ))}
      </div>

      <div style={NODE_BODY}>
        <div style={sx(NODE_TITLE, { fontFamily: n.titleFont })}>{n.title}</div>
        {lines.map((l, i) => (
          <div key={l.id || `${n.id}-l${i}`} style={sx(NODE_LINE, { color: l.color, fontStyle: l.style })}>
            {l.text}
          </div>
        ))}
        {clauseLine ? <div style={NODE_CLAUSE}>{clauseLine}</div> : null}
        {(n.facts && n.facts.length) || n.factsAbsent || n.band || n.hollowLine || n.agentLine ? (
          <div className="ws-trace-facts" style={NODE_EXTRA}>
            {n.band ? (
              <div style={FACT_LINE} title={n.bandWhy || undefined}>
                <span className="ws-trace-band" style={FACT_BAND}>{n.band}</span>
              </div>
            ) : null}
            {n.hollowLine ? <div style={sx(FACT_LINE, FACT_ABSENT)}>{n.hollowLine}</div> : null}
            {(n.facts || []).map((f) => (
              <div key={f.label} style={FACT_LINE} title={f.label + ': ' + f.value}>
                <span style={FACT_LABEL}>{f.label}</span>
                <span style={FACT_VALUE}>{f.value}</span>
              </div>
            ))}
            {n.factsAbsent ? <div style={sx(FACT_LINE, FACT_ABSENT)} title={n.factsAbsent}>{n.factsAbsent}</div> : null}
            {n.agentLine ? <div className="ws-trace-agent" style={sx(FACT_LINE, AGENT_LINE)}>{n.agentLine}</div> : null}
          </div>
        ) : null}
      </div>

      <div style={NODE_FOOT}>{n.foot}</div>
    </div>
    {toggle}
    </React.Fragment>
  )
}

/**
 * `NodeCard` as the canvas mounts it: memoised on its own props.
 *
 * A pan changes `view` and nothing else. The root's node view-models are
 * memoised on everything EXCEPT `view`, and the four handlers above are stable
 * instance callbacks, so with this wrapper a pan re-renders zero cards instead
 * of reconciling ~26,000 elements to arrive at the same tree. Measured on
 * a long session (1,481 cards): one pan mousemove, 678 ms before, and the
 * window on top of this takes the mounted element count from ~28,700 to a few
 * hundred.
 */
export const NodeCard = React.memo(NodeCardImpl)

/**
 * One 9px pin square and its label (spec-template :548, :551).
 *
 * Appearance comes from `logic.pinStyle` when the pin carries only its type,
 * and from the pin itself when the root already resolved it. The row offset is
 * the pin index × 20 (spec-script §B.4 #28) — deliberately 10px different from
 * the wire attach point, which the root computes with `pinPos`.
 *
 * @param {object} props
 * @param {{t?:string,c?:string,top?:number,label?:string,title?:string}} props.pin
 * @param {number} props.index position in its own side's list
 * @param {'left'|'right'} props.side
 * @returns {JSX.Element}
 */
function Pin({ pin, index, side }) {
  const ps = pin && pin.bg ? pin : pinStyle(pin || {})
  const top = pin && pin.top != null ? pin.top : index * 20
  return (
    <div title={pin.title} style={sx(PIN_ROW, side === 'left' ? PIN_ROW_L : PIN_ROW_R, { top })}>
      <span style={sx(PIN_SQUARE, { background: ps.bg, border: ps.border, borderRadius: ps.radius, clipPath: ps.clip })} />
      <span style={PIN_LABEL}>{pin.label}</span>
    </div>
  )
}

/* ── Timeline (:582-596) ─────────────────────────────────────────────────── */

const TL_LIST = Object.freeze({ flex: 1, overflow: 'auto', padding: '6px 0' })
const TL_CAPTION = Object.freeze({ padding: '2px 12px 6px', fontSize: FS.sm, color: C.inkDim })
const TL_ROW = Object.freeze({
  display: 'grid',
  gridTemplateColumns: GRID.timelineRow,
  alignItems: 'center',
  gap: '10px',
  padding: '2px 12px',
  cursor: 'pointer',
  fontSize: FS.body,
})
const TL_MARK = Object.freeze({ fontSize: FS.body, textAlign: 'center' })
/* LEG-DIM: a dimmed row's ink — text2, ≥ 7.04:1 on every ground it sits on
 * (contrast.test.js). Its mark and kind take the same neutral ink. */
const TL_DIM = Object.freeze({ color: C.inkDim })
const TL_SEQ = Object.freeze({ fontFamily: FONT.mono, fontSize: FS.sm, color: C.inkMute })
const TL_KIND = Object.freeze({ fontWeight: 500 })
const TL_TEXT = Object.freeze({ overflow: 'hidden', whiteSpace: 'nowrap', textOverflow: 'ellipsis' })
const TL_REF = Object.freeze({
  fontFamily: FONT.mono,
  fontSize: FS.cap,
  color: C.inkFaint,
  overflow: 'hidden',
  whiteSpace: 'nowrap',
  textOverflow: 'ellipsis',
})
const TL_NOTE = Object.freeze({ padding: '8px 12px', fontSize: FS.ctl, color: C.inkMute })

/**
 * The recorded timeline (spec-template :582-596). `role="list"`, one
 * `role="listitem"` per event, grid `GRID.timelineRow`.
 *
 * Dimming is NEVER hiding: a dimmed row desaturates — its text, mark and kind
 * in text2 (`TL_DIM`), never a lower opacity (LEG-DIM; it used to render at
 * `opacity: 0.35`, 1.80:1). A dimmed node is at `0.22` while a thread is lit
 * (the named `dimmed` exception, contrast.test.js, twinned undimmed in the
 * Outline) and a wire touching one at `0.12` (spec-script §F.5).
 * The one place a filter actually removes rows is `RecordsTable`, and even
 * there the footer keeps the honest denominator.
 *
 * @param {object} props
 * @param {string} props.caption
 * @param {Array<{id:string,seq:number,mark:string,markColor:string,kind:string,kindColor:string,text:string,font:string,fontSize:string,ref:string,sel:boolean,dim?:boolean,bg:string,edge:string}>} props.rows
 * @param {string|null} props.note
 * @param {number} props.rowH 28 compact · 32 comfortable
 * @param {string|null} [props.reading] the address being read, when a read is in
 *   flight: the caption slot and three inert bars at the row's own height, then
 *   the waiting line. No row is drawn, because a drawn row is a row.
 * @param {object|null} [props.absence] `<Absence>` props for a record that was
 *   read and holds no events to place on a line
 * @param {(id: string) => void} props.onSelect
 * @param {(id: string, e: KeyboardEvent) => void} props.onKey
 * @returns {JSX.Element}
 */
export function Timeline({ caption, rows, note, rowH, reading, absence, onSelect, onKey }) {
  if (!Array.isArray(rows)) {
    return (
      <Refusal
        title="This session has no recorded timeline to show."
        missing="the session's events — nothing was adapted for this reading"
        remedy="thruline export --all"
      />
    )
  }
  if (reading) {
    return (
      <div role="list" aria-label="Recorded timeline" aria-busy="true" style={TL_LIST}>
        <Skel n={1} rowH={rowH} />
        <Skel n={3} rowH={rowH} />
        <Waiting endpoint={reading} />
      </div>
    )
  }
  if (absence && !rows.length) {
    return (
      <div style={TL_LIST}>
        <div style={TL_CAPTION}>{caption}</div>
        <Absence {...absence} />
      </div>
    )
  }
  return (
    <div role="list" aria-label="Recorded timeline" style={TL_LIST}>
      <div style={TL_CAPTION}>{caption}</div>
      {rows.map((r) => (
        <div
          key={r.id}
          role="listitem"
          tabIndex={0}
          className="ws-hv"
          aria-current={r.sel ? 'true' : 'false'}
          onClick={() => onSelect && onSelect(r.id)}
          onKeyDown={(e) => onKey && onKey(r.id, e)}
          style={sx(TL_ROW, HOVER.dataRow(), {
            minHeight: rowH,
            background: r.bg,
            borderLeft: `2px solid ${r.edge}`,
          }, r.dim ? TL_DIM : null)}
        >
          <span style={sx(TL_MARK, { color: r.dim ? TL_DIM.color : r.markColor })} aria-hidden="true">{r.mark}</span>
          <span style={TL_SEQ}>{r.seq}</span>
          <span style={sx(TL_KIND, { color: r.dim ? TL_DIM.color : r.kindColor })}>{r.kind}</span>
          <span style={sx(TL_TEXT, { fontFamily: r.font, fontSize: r.fontSize })}>{r.text}</span>
          <span style={TL_REF}>{r.ref}</span>
        </div>
      ))}
      {note ? <div style={TL_NOTE}>{note}</div> : null}
    </div>
  )
}

/* ── Scoped-records table (:599-616) ─────────────────────────────────────── */

/* N13-FOCUS-2411: the scroll box keeps focus below its sticky header. */
const TB_WRAP = Object.freeze({ flex: 1, overflow: 'auto', scrollPaddingTop: SIZE.stickyHeader })
const TB_HEAD = Object.freeze({
  display: 'grid',
  gap: '10px',
  padding: '0 12px',
  height: SIZE.stickyHeader,
  alignItems: 'center',
  position: 'sticky',
  top: 0,
  background: C.surf1,
  borderBottom: `1px solid ${C.line1}`,
  fontSize: FS.cap,
  letterSpacing: LS.cap,
  textTransform: 'uppercase',
  color: C.inkDim,
  zIndex: 1,
})
const TB_GROUP_CAP = Object.freeze({
  padding: '6px 12px 2px',
  fontSize: FS.cap,
  letterSpacing: LS.cap,
  textTransform: 'uppercase',
  color: C.inkSoft,
  fontWeight: 600,
})
const TB_GROUP_COUNT = Object.freeze({
  color: C.inkFaint,
  fontWeight: 400,
  textTransform: 'none',
  letterSpacing: 0,
  fontFamily: FONT.mono,
})
const TB_ROW = Object.freeze({
  display: 'grid',
  gap: '10px',
  padding: '0 12px',
  alignItems: 'center',
  cursor: 'pointer',
  fontSize: FS.body,
})
const TB_CELL = Object.freeze({ overflow: 'hidden', whiteSpace: 'nowrap', textOverflow: 'ellipsis' })
const TB_FOOT = Object.freeze({ padding: '8px 12px', fontSize: FS.sm, color: C.inkFaint })

/**
 * The scoped-records table (spec-template :599-616). A sticky header, optional
 * group captions, dynamic columns from the Columns menu.
 *
 * The footer keeps the honest denominator — `N of M objects · same scope,
 * selection and filters as the graph` — because this is the one surface where
 * filtering REMOVES rows (spec-script §F.5).
 *
 * ARIA IS FIXED, not reproduced (spec-template Decision D5, CONTRACT §8
 * template-D5): the design's `role="table"` holds `role="row"` directly with no
 * `rowgroup`, which a screen reader cannot navigate. Here the wrapper is a real
 * `role="grid"` with `aria-rowcount`, each group is a `role="rowgroup"`, and
 * cells are `role="gridcell"` — `role="cell"` is not a valid child of a grid.
 *
 * @param {object} props
 * @param {string} props.cols a CSS grid template, composed from `columns` state
 * @param {Array<{key:string,label:string}>} props.head
 * @param {Array<{title:string|null,count:string|null,rows:Array<object>}>} props.groups
 *   `count` is a rendered phrase or null; a zero count renders NO count element
 * @param {string} props.foot
 * @param {number} props.rowH
 * @param {string|null} [props.reading] the address being read, when a read is in
 *   flight. The header row renders FOR REAL — the column set is known chrome, so
 *   faking it is unnecessary — over three inert bars on the real column
 *   template. The footer stays blank: a denominator invented while reading is a
 *   fabricated count.
 * @param {object|null} [props.absence] `<Absence>` props for a record that was
 *   read and holds no objects to list
 * @param {(id: string) => void} props.onSelect
 * @param {(id: string, e: KeyboardEvent) => void} props.onKey
 * @returns {JSX.Element}
 */
export function RecordsTable({ cols, head, groups, foot, rowH, reading, absence, onSelect, onKey }) {
  if (!Array.isArray(groups) || !Array.isArray(head)) {
    return (
      <Refusal
        title="This session has no scoped records to list."
        missing="the adapted session's objects — nothing was read for this reading"
        remedy="thruline export --all"
      />
    )
  }
  if (reading) {
    return (
      <div aria-busy="true" style={TB_WRAP}>
        <div style={sx(TB_HEAD, { gridTemplateColumns: cols })}>
          {head.map((h) => (
            <span key={h.key} className="ws-head-cell">
              {h.label}
            </span>
          ))}
        </div>
        <Skel n={3} rowH={rowH} />
        <Waiting endpoint={reading} />
      </div>
    )
  }
  const bodyRows = groups.reduce((sum, g) => sum + (Array.isArray(g.rows) ? g.rows.length : 0), 0)
  if (absence && !bodyRows) {
    return (
      <div style={TB_WRAP}>
        <div style={sx(TB_HEAD, { gridTemplateColumns: cols })}>
          {head.map((h) => (
            <span key={h.key} className="ws-head-cell">
              {h.label}
            </span>
          ))}
        </div>
        <Absence {...absence} />
      </div>
    )
  }
  return (
    <div role="grid" aria-label="Scoped records" aria-rowcount={bodyRows + 1} style={TB_WRAP}>
      <div role="rowgroup">
        <div role="row" aria-rowindex={1} style={sx(TB_HEAD, { gridTemplateColumns: cols })}>
          {head.map((h) => (
            <span key={h.key} role="columnheader" className="ws-head-cell">
              {h.label}
            </span>
          ))}
        </div>
      </div>
      {groups.map((g, gi) => (
        <div key={g.key || g.title || `g${gi}`} role="rowgroup">
          {/* F27: a `rowgroup` permits only `row` children. The group caption
              used to sit inside one as a bare `<div>`, which is its own ARIA
              defect. It is a row now, spanning the grid, and its single child
              is the header cell for the group it names. */}
          {g.title ? (
            <div role="row" style={TB_GROUP_CAP}>
              <span role="columnheader" aria-colspan={head.length}>
                {g.title} {g.count ? <span style={TB_GROUP_COUNT}>{g.count}</span> : null}
              </span>
            </div>
          ) : null}
          {(g.rows || []).map((r) => (
            <div
              key={r.id}
              role="row"
              tabIndex={0}
              className="ws-hv"
              aria-selected={r.sel ? 'true' : 'false'}
              onClick={() => onSelect && onSelect(r.id)}
              onKeyDown={(e) => onKey && onKey(r.id, e)}
              style={sx(TB_ROW, HOVER.dataRow(), {
                gridTemplateColumns: cols,
                minHeight: rowH,
                background: r.bg,
                borderLeft: `2px solid ${r.edge}`,
              })}
            >
              {(r.cells || []).map((c, ci) => (
                <span
                  key={c.key || `${r.id}-c${ci}`}
                  role="gridcell"
                  style={sx(TB_CELL, { color: c.color, fontFamily: c.font, fontSize: c.fontSize })}
                >
                  {c.glyph ? <span aria-hidden="true">{c.glyph} </span> : null}
                  {c.text}
                </span>
              ))}
            </div>
          ))}
        </div>
      ))}
      <div style={TB_FOOT}>{foot}</div>
    </div>
  )
}

/* ── Pinned claim document (:619-639) ────────────────────────────────────── */

const DOC_PAGE = Object.freeze({
  flex: 1,
  overflow: 'auto',
  padding: '14px 18px',
  display: 'grid',
  gridTemplateColumns: GRID.claimDocPage,
  gap: '18px',
  alignContent: 'start',
})
const DOC_WORDROW = Object.freeze({ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: 8, flexWrap: 'wrap' })
const DOC_TYPE = Object.freeze({ fontSize: FS.sm, color: C.inkDim })
const DOC_ID = Object.freeze({ font: `${FS.sm} ${FONT.mono}`, color: C.inkDim })
const DOC_TITLE = Object.freeze({
  fontSize: FS.docTitle,
  fontWeight: 500,
  color: C.inkBright,
  lineHeight: 1.35,
  marginBottom: 10,
  textWrap: 'pretty',
})
const DOC_SENTENCE = Object.freeze({ fontSize: FS.lg, lineHeight: 1.5, marginBottom: 4, textWrap: 'pretty' })
const DOC_AUTHORED = Object.freeze({
  marginTop: 10,
  fontSize: FS.body,
  fontStyle: 'italic',
  color: C.inkMute,
  borderLeft: `2px solid ${C.line2}`,
  paddingLeft: 8,
})
const DOC_AUTHORED_KICKER = Object.freeze({
  fontStyle: 'normal',
  fontSize: '9.5px',
  letterSpacing: LS.frame,
  textTransform: 'uppercase',
  color: C.inkFaint,
  marginRight: 6,
})
const DOC_KICKER = Object.freeze({ marginTop: 14, marginBottom: 4 })
const DOC_NARR = Object.freeze({ fontSize: FS.base, lineHeight: 1.5, color: C.ink })
/* A3-NOTES-NEVER-OPEN: the folded notes under the claim document's findings. */
const DOC_NOTES = Object.freeze({ marginTop: 10 })
const DOC_NOTES_SUMMARY = Object.freeze({ cursor: 'pointer', fontSize: FS.sm, color: C.inkMute })
const DOC_NOTES_ABOUT = Object.freeze({ fontSize: FS.sm, color: C.inkMute, margin: '4px 0 6px' })
const DOC_FINDING = Object.freeze({
  width: '100%',
  display: 'grid',
  gridTemplateColumns: GRID.claimDocFinding,
  gap: '10px',
  alignItems: 'center',
  padding: '5px 8px',
  background: C.surf1,
  border: `1px solid ${C.line1}`,
  borderRadius: RADIUS.ctl,
  marginBottom: 3,
  color: C.ink,
  fontSize: FS.body,
  cursor: 'pointer',
  textAlign: 'left',
})
const DOC_FINDING_NAME = Object.freeze({ fontWeight: 500 })
const DOC_FINDING_REF = Object.freeze({
  color: C.inkMute,
  fontSize: FS.ctl,
  overflow: 'hidden',
  whiteSpace: 'nowrap',
  textOverflow: 'ellipsis',
  fontFamily: FONT.mono,
})
const DOC_EV_ROW = Object.freeze({
  display: 'grid',
  gridTemplateColumns: GRID.claimDocEvidence,
  gap: '10px',
  padding: '5px 0',
  borderBottom: `1px solid ${C.lineRow}`,
  fontSize: FS.body,
})
const DOC_EV_K = Object.freeze({ color: C.inkDim })
const DOC_EV_V = Object.freeze({ wordBreak: 'break-word' })
const DOC_CLOSING = Object.freeze({ marginTop: 10, fontSize: FS.ctl, color: C.inkDim })

/**
 * The pinned claim document (spec-template :619-639) — a two-column reading
 * surface, `GRID.claimDocPage`.
 *
 * Left: word row · title · sentences · `As the CLI narrates it · {register}` ·
 * the narration · the authored block · Findings.
 * Right: `The record, in the disclosure order` · evidence key/value rows · the
 * closing sentence.
 *
 * TWO ORDERING RULES, both doctrine:
 *  · The authored block renders BELOW the narration, never between the record
 *    sentences and the narration — authored text may not sit inside the
 *    record's own reading order (spec-doctrine E-2, F-17). The design has it at
 *    :625, directly under the record sentences; this port moves it.
 *  · The clause is upright in `C.inkMute`, NEVER italic; italic is reserved for
 *    authored/inferred text (BRIEF:1562, C-12). Do not "harmonise" the two
 *    blocks. On this surface the clause arrives inside `insp.sentences`
 *    verbatim (spec-doctrine C-11) and is rendered upright with them.
 *
 * The findings caption is a count phrase, not `Findings · N`: a naked number
 * carries no noun (spec-doctrine F-7, whose worked example rewrites exactly
 * this caption shape).
 *
 * The design's `claimDocInspector` MUTATES `this.state.sel` to reuse the
 * inspector builder. That does not survive: the root parameterises the builder
 * as `inspector(selId)` and calls `inspector('clm')` (spec-script §E6).
 *
 * @param {object} props
 * @param {object} props.insp the inspector view-model for the pinned claim,
 *   built by `inspector(claimNodeId)` — same shape `Inspector` takes
 * @param {string} props.registerLabel `'manager register'` | `'operator register'`
 * @param {(findingKey: string) => void} props.onFinding
 * @returns {JSX.Element}
 */
export function ClaimDocument({ insp, registerLabel, onFinding }) {
  if (!insp || !Array.isArray(insp.sentences)) {
    return (
      <Refusal
        title="There is no claim to pin open."
        missing="a claim in this session's record"
        remedy="thruline explain --json"
      />
    )
  }
  const sentences = insp.sentences
  const narr = Array.isArray(insp.narr) ? insp.narr : []
  const findings = Array.isArray(insp.findings) ? insp.findings : []
  /* A3-NOTES-NEVER-OPEN: `info` findings arrive apart, as notes. */
  const notes = Array.isArray(insp.notes) ? insp.notes : []
  const FC = HQ_COPY.trace.finding_counts
  const evidence = Array.isArray(insp.evidence) ? insp.evidence : []

  return (
    <div style={DOC_PAGE}>
      <div>
        <div style={DOC_WORDROW}>
          <StateWord slug={insp.wordSlug} clause={insp.clause} variant="inline" />
          <span style={DOC_TYPE}>{insp.typeLabel}</span>
          <span style={DOC_ID}>{insp.id}</span>
        </div>
        <div style={DOC_TITLE}>{insp.title}</div>
        {sentences.map((s, i) => (
          <div key={s.id || `s${i}`} style={sx(DOC_SENTENCE, { color: s.color })}>
            {s.text}
          </div>
        ))}

        {/* The same unguarded heading as `docks.jsx`, and it ships with the
            same guard in the same commit: a section whose body is empty does
            not render its caption. `/api/narrate` does not exist, so on every
            real session this was a kicker over a void. */}
        {narr.length ? (
          <React.Fragment>
            <Kicker style={DOC_KICKER}>As the CLI narrates it · {registerLabel}</Kicker>
            {narr.map((s, i) => (
              <div key={s.id || `n${i}`} style={DOC_NARR}>
                {s.text}
              </div>
            ))}
          </React.Fragment>
        ) : null}

        {insp.commentary ? (
          <div style={DOC_AUTHORED}>
            <span style={DOC_AUTHORED_KICKER}>authored</span>
            {insp.commentary}
          </div>
        ) : null}

        {insp.hasFindings && findings.length ? (
          <>
            {/* F12/F13 / A3-NOTES-NEVER-OPEN: one count phrase from the copy
              * file ("3 findings"), counting `fail` + `warn` only. The
              * Inspector's copy of this kicker (docks.jsx) reads identically. */}
            <Kicker style={DOC_KICKER}>{findings.length === 1 ? FC.findings_one : fill(FC.findings_many, { n: findings.length })}</Kicker>
            {findings.map((f) => (
              <button
                key={f.key}
                type="button"
                className="ws-hv"
                onClick={() => onFinding && onFinding(f.key)}
                style={sx(DOC_FINDING, HOVER.treeTab())}
              >
                <span style={DOC_FINDING_NAME}>{f.name}</span>
                <span style={{ color: f.sigColor, fontSize: FS.sm }}>{f.sig}</span>
                <span style={DOC_FINDING_REF}>{f.ref}</span>
              </button>
            ))}
          </>
        ) : null}

        {/* A3-NOTES-NEVER-OPEN: `info` findings are notes — folded away,
          * counted in their own noun, never called open. */}
        {insp.hasFindings && notes.length ? (
          <details style={DOC_NOTES}>
            <summary style={DOC_NOTES_SUMMARY}>{notes.length === 1 ? FC.notes_one : fill(FC.notes_many, { n: notes.length })}</summary>
            <div style={DOC_NOTES_ABOUT}>{FC.notes_about}</div>
            {notes.map((f) => (
              <button
                key={f.key}
                type="button"
                className="ws-hv"
                onClick={() => onFinding && onFinding(f.key)}
                style={sx(DOC_FINDING, HOVER.treeTab())}
              >
                <span style={DOC_FINDING_NAME}>{f.name}</span>
                <span style={DOC_FINDING_REF}>{f.ref}</span>
              </button>
            ))}
          </details>
        ) : null}
      </div>

      <div>
        <Kicker style={{ marginBottom: 6 }}>The record, in the disclosure order</Kicker>
        {evidence.map((r, i) => (
          <div key={r.k || `e${i}`} style={DOC_EV_ROW}>
            <span style={DOC_EV_K}>{r.k}</span>
            <span style={sx(DOC_EV_V, { color: r.color, fontFamily: r.font, fontSize: r.fontSize })}>{r.v}</span>
          </div>
        ))}
        <div style={DOC_CLOSING}>{insp.closing}</div>
      </div>
    </div>
  )
}

/* ── Compare pane (:642-660) ─────────────────────────────────────────────── */

const CMP_WRAP = Object.freeze({ flex: 1, overflow: 'auto', padding: '12px 16px' })
const CMP_HEAD = Object.freeze({
  display: 'grid',
  gridTemplateColumns: GRID.compare,
  gap: '12px',
  alignItems: 'end',
  marginBottom: 10,
})
const CMP_LEAD = Object.freeze({ fontSize: FS.body, color: C.inkBright, fontWeight: 500 })
const CMP_PICKER = Object.freeze({ display: 'flex', alignItems: 'center', gap: '6px' })
const CMP_PICKER_LABEL = Object.freeze({ fontSize: FS.cap, color: C.inkDim })
const CMP_SELECT = Object.freeze({
  height: SIZE.ctl,
  background: C.fieldBg,
  border: `1px solid ${C.lineField}`,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  font: 'inherit',
  fontSize: FS.ctl,
  padding: '0 6px',
})
const CMP_COLHEAD = Object.freeze({
  display: 'grid',
  gridTemplateColumns: GRID.compare,
  gap: '12px',
  padding: '6px 0',
  borderBottom: `1px solid ${C.lineStrong}`,
  fontSize: FS.cap,
  letterSpacing: LS.cap,
  textTransform: 'uppercase',
  color: C.inkDim,
})
const CMP_COLHEAD_SIDE = Object.freeze({
  fontFamily: FONT.mono,
  textTransform: 'none',
  letterSpacing: 0,
  color: C.inkBright,
})
const CMP_ROW = Object.freeze({
  display: 'grid',
  gridTemplateColumns: GRID.compare,
  gap: '12px',
  padding: '7px 0',
  borderBottom: `1px solid ${C.lineRow}`,
  fontSize: FS.body,
  alignItems: 'start',
})
const CMP_K = Object.freeze({ color: C.inkDim, fontWeight: 500 })
const CMP_LINE = Object.freeze({ lineHeight: 1.45, wordBreak: 'break-word' })
const CMP_AUTOPSY = Object.freeze({ marginTop: 12, fontSize: FS.sm, color: C.inkDim, fontFamily: FONT.mono })
const CMP_CAVEAT = Object.freeze({ marginTop: 4, fontSize: FS.sm, color: C.inkFaint })

/**
 * Compare sessions (spec-template :642-660) — two `<select>`s and twelve
 * aligned rows, `GRID.compare`.
 *
 * Rows: Session line · Scope of this reading · Capture · Claims · Findings ·
 * Checks · Promises · Asks · Risk flags · Ending · Receipt · Binding.
 *
 * Every word in every cell carries its clause (spec-doctrine C-8, C-9, C-10 —
 * two sessions side by side, four words, zero clauses in the design). The root
 * composes each cell as `lines[]` and puts the clause on its own line, so this
 * component renders lines and never re-authors one. Every zero arrives as an
 * honest SENTENCE, never `none`: `No findings are recorded against this
 * claim.` / `No asks were extracted from this session.` / `No risk flags are
 * recorded in this session.` (spec-doctrine F-6).
 *
 * `capture_fidelity` does not appear here. It is engine vocabulary in a
 * first-level body cell; the row carries BRIEF:1521's instrument sentence
 * instead (spec-doctrine F-9, A2 #1) — again composed upstream.
 *
 * The caveat at :658 is verbatim and permanent: `Passing one declared check
 * proves that check at its stated scope. It never clears a project.`
 *
 * @param {object} props
 * @param {string} props.a @param {string} props.b session keys
 * @param {Array<{key:string,label:string}>} props.options
 * @param {string} props.aLabel @param {string} props.bLabel
 * @param {Array<{k:string,a:Array<object>,b:Array<object>}>} props.rows
 * @param {string} props.autopsy the mono summary line
 * @param {(key: string) => void} props.onA @param {(key: string) => void} props.onB
 * @returns {JSX.Element}
 */
function CompareLine({ line }) {
  /* DOC-02: the root composes each cell as `{ text, wordSlug, clause }`. An
   * earlier draft rendered `text` alone, which dropped every clause on the one
   * surface that can show four negative words at once. The word goes through
   * `<StateWord>` — the component that refuses a negative word with no clause —
   * and the text is whatever the line says besides the word. */
  const word = line.wordSlug ? (
    <StateWord slug={line.wordSlug} clause={line.clause === undefined ? null : line.clause} variant="row" />
  ) : null
  const text = line.text ? (
    <div style={sx(CMP_LINE, { color: line.color, fontFamily: line.font, fontSize: line.fontSize })}>{line.text}</div>
  ) : null
  if (!word && !text) return null
  return (
    <div>
      {word}
      {text}
    </div>
  )
}

export function ComparePane({ a, b, options, aLabel, bLabel, rows, autopsy, onA, onB, onChooseSession }) {
  if (!Array.isArray(rows) || !Array.isArray(options)) {
    return (
      <Refusal
        title="There is nothing to compare yet."
        missing="two readings on this machine — comparing needs two"
        remedy="thruline ingest <path>"
      />
    )
  }
  return (
    <div style={CMP_WRAP}>
      <div style={CMP_HEAD}>
        <div style={CMP_LEAD}>Two sessions, aligned. Each side keeps its own scope and capture limits.</div>
        <div style={CMP_PICKER}>
          <span style={CMP_PICKER_LABEL}>A</span>
          <select
            value={a}
            onChange={(e) => onA && onA(e.target.value)}
            aria-label="Session A"
            style={CMP_SELECT}
          >
            {options.map((o) => (
              <option key={o.key} value={o.key}>
                {o.label}
              </option>
            ))}
          </select>
        </div>
        <div style={CMP_PICKER}>
          <span style={CMP_PICKER_LABEL}>B</span>
          <select
            value={b}
            onChange={(e) => onB && onB(e.target.value)}
            aria-label="Session B"
            style={CMP_SELECT}
          >
            {options.map((o) => (
              <option key={o.key} value={o.key}>
                {o.label}
              </option>
            ))}
          </select>
        </div>
      </div>

      {a === b ? <div className="ws-compare-empty">
        <h2>{options.length < 2 ? 'Open a second session to compare.' : 'Choose two different sessions.'}</h2>
        <p>{options.length < 2 ? 'Choose another session, then return to Compare. This session will stay available on the other side.' : 'Both sides currently show the same session. Pick a different session above to see what changed.'}</p>
        {onChooseSession ? <button type="button" onClick={onChooseSession}>Choose another session →</button> : null}
      </div> : <>
      <div style={CMP_COLHEAD}>
        <span>Aligned on</span>
        <span style={CMP_COLHEAD_SIDE}>{aLabel}</span>
        <span style={CMP_COLHEAD_SIDE}>{bLabel}</span>
      </div>

      {rows.map((r) => (
        <div key={r.k} style={CMP_ROW}>
          <span style={CMP_K}>{r.k}</span>
          <div>{(r.a || []).map((l, i) => <CompareLine key={`${r.k}-a${i}`} line={l} />)}</div>
          <div>{(r.b || []).map((l, i) => <CompareLine key={`${r.k}-b${i}`} line={l} />)}</div>
        </div>
      ))}

      {autopsy ? <div style={CMP_AUTOPSY}>{autopsy}</div> : null}
      <div style={CMP_CAVEAT}>
        Passing one declared check proves that check at its stated scope. It never clears a project.
      </div>
      </>}
    </div>
  )
}

/* ── Destinations drawer (:663-682) ──────────────────────────────────────── */

const DRAWER = Object.freeze({
  position: 'absolute',
  left: 0,
  top: 0,
  bottom: 0,
  maxWidth: '100%',
  zIndex: Z.drawer,
  background: C.surf1,
  borderRight: `1px solid ${C.line2}`,
  boxShadow: SHADOW.drawer,
  display: 'flex',
  flexDirection: 'column',
})
const DRAWER_BAR = Object.freeze({
  height: 32,
  display: 'flex',
  alignItems: 'center',
  gap: '8px',
  padding: '0 10px',
  background: C.surf2Hi,
  borderBottom: `1px solid ${C.lineStrong}`,
})
const DRAWER_TITLE = Object.freeze({ fontSize: FS.body, fontWeight: 600, color: C.inkBright })
const DRAWER_PILL = Object.freeze({
  fontSize: FS.meta,
  color: C.amber,
  border: `1px solid ${C.amberBd}`,
  borderRadius: RADIUS.pill,
  padding: '0 4px',
  fontFamily: FONT.mono,
})
const DRAWER_CLOSE = Object.freeze({
  background: 'transparent',
  border: 0,
  color: C.inkDim,
  cursor: 'pointer',
  fontSize: '15px',
  padding: '0 4px',
})
const DRAWER_BODY = Object.freeze({ flex: 1, overflow: 'auto', padding: '10px 12px' })
const DRAWER_LEAD = Object.freeze({
  fontSize: FS.lg,
  color: C.inkBright,
  lineHeight: 1.45,
  marginBottom: 8,
  textWrap: 'pretty',
})
const DRAWER_SECTION = Object.freeze({ marginBottom: 12 })
const DRAWER_SECTION_CAP = Object.freeze({ margin: '8px 0 4px' })
const DRAWER_ROW = Object.freeze({
  display: 'grid',
  gap: '10px',
  padding: '5px 6px',
  borderBottom: `1px solid ${C.lineRow}`,
  fontSize: FS.body,
  alignItems: 'start',
  borderRadius: RADIUS.pill,
})
const DRAWER_CELL = Object.freeze({ wordBreak: 'break-word', lineHeight: 1.4 })
const DRAWER_NOTE = Object.freeze({ fontSize: FS.ctl, color: C.inkMute, marginTop: 5, lineHeight: 1.45 })
const DRAWER_FILTER_ROW = Object.freeze({ display: 'flex', flexDirection: 'column', gap: '4px', margin: '0 0 10px' })
const DRAWER_FILTER_LABEL = Object.freeze({ fontSize: FS.cap, color: C.inkMute, letterSpacing: '.04em', textTransform: 'uppercase' })
const DRAWER_FILTER_INPUT = Object.freeze({
  height: SIZE.ctl,
  padding: '0 8px',
  background: C.surf2,
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  font: `${FS.ctl} ${FONT.mono}`,
})
const DRAWER_ACTION = Object.freeze({
  margin: '6px 6px 0 0',
  height: SIZE.ctl,
  padding: '0 9px',
  background: C.surf3,
  border: `1px solid ${C.line2}`,
  borderRadius: RADIUS.ctl,
  color: C.ink,
  fontSize: FS.ctl,
  cursor: 'pointer',
})

/**
 * The destinations drawer (spec-template :663-682) — the panel a rail
 * destination opens, over the centre content, at z-index 30.
 *
 * ALL FIFTEEN destinations open this one drawer in the design; there are no
 * routes and no URLs (spec-template Decision D3). The port keeps drawer-only
 * navigation: the shipped app has NO router, no `history.pushState` and no
 * `popstate` listener anywhere in `ui/src` (spec-target §A.2), and adding one
 * is a separate change with its own deep-linking contract.
 *
 * Two destinations do not render a drawer at all, and the root does not mount
 * this component for them. `inbox` renders the shipped `os/shell.jsx`, because
 * the Inbox is the app's ONE attention queue and a second view over the same
 * derivation is a second place for the same fact to disagree (doctrine 6,
 * `lib/inbox.test.js` "there is exactly one attention queue in the app").
 * `trace` closes the drawer and shows the canvas.
 *
 * A destination whose source in `data.DESTINATION_SOURCE` is absent renders
 * `<Refusal>` inside the drawer body — INSTEAD of the sections, naming the
 * missing document or endpoint. It never renders a review note, a "coming
 * soon", or an empty section. The design's `unverified` sentence is deleted
 * outright (spec-doctrine F-5, A5) and no replacement is invented here: the
 * lead text is the root's, from the record.
 *
 * @param {object} props
 * @param {string|null} props.destination the rail key; null closes the drawer
 * @param {string} props.title @param {string} props.lead
 * @param {number} props.width 520, or 600 for receipts/explorer/handoff
 * @param {Array<{title:string|null,cols:string,rows:Array<object>,note:string|null,actions:Array<{label:string,onClick:()=>void}>}>} props.sections
 * @param {string|null} props.demoLabel the pill repeated in the drawer title bar
 * @param {{value:string,label:string,placeholder:string,onChange:(v:string)=>void}|null} [props.filter]
 *   an optional labelled field above the sections, for the one destination
 *   whose list is longer than a panel. It NARROWS and never re-orders — the
 *   order a section arrives in is the order it renders in — and it carries a
 *   real `<label>`, because a search field whose only name is a placeholder has
 *   no accessible name once anything is typed into it. Null renders nothing.
 * @param {{title:string,missing:string,remedy?:string}|null} props.refusal
 * @param {string|null} [props.reading] the address being read, when this
 *   destination's document is IN FLIGHT. It renders `<Waiting>` instead of the
 *   sections and it is tested BEFORE the refusal, because a read that has not
 *   come back yet is not a missing document — the `agreed` destination used to
 *   route its in-flight read through `<Refusal>` and told the reader, in the
 *   danger palette under "WHAT IS MISSING", that the document was missing while
 *   it was being fetched (`state.js`, THE FOUR FACTS).
 * @param {() => void} props.onClose
 * @returns {JSX.Element|null}
 */
export function DestinationsDrawer({ destination, title, lead, width, sections, demoLabel, filter, refusal, reading, onClose }) {
  const ref = React.useRef(null)
  const titleId = `ws-drawer-title-${destination || 'none'}`
  /* R13: the drawer behaved like a dialog — Escape closed it, it had a × — but
   * carried none of the semantics, so `document.querySelector('[role=dialog]')`
   * found nothing and a screen reader was never told a panel had opened. It is
   * named by its own title, and focus moves into it on open; the root returns
   * focus to the rail button on close. It is deliberately NOT `aria-modal`:
   * the canvas behind it stays readable and reachable, which is the whole
   * point of a drawer rather than a modal. */
  React.useEffect(() => {
    if (!destination || !ref.current) return
    const first = ref.current.querySelector('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])')
    if (first && first.focus) first.focus()
  }, [destination])
  if (!destination) return null
  const secs = Array.isArray(sections) ? sections : []
  return (
    <section role="dialog" aria-labelledby={titleId} ref={ref} style={sx(DRAWER, { width })}>
      <div style={DRAWER_BAR}>
        <span id={titleId} style={DRAWER_TITLE}>
          {title}
        </span>
        {demoLabel ? <span style={DRAWER_PILL}>{demoLabel}</span> : null}
        <span style={{ flex: 1 }} />
        <button type="button" className="ws-hv" aria-label="Close" onClick={onClose} style={sx(DRAWER_CLOSE, HOVER.ghost())}>
          ×
        </button>
      </div>
      <div style={DRAWER_BODY}>
        {reading ? (
          <Waiting endpoint={reading} />
        ) : refusal ? (
          <Refusal title={refusal.title} missing={refusal.missing} remedy={refusal.remedy} detail={refusal.detail} />
        ) : (
          <>
            <div style={DRAWER_LEAD}>{lead}</div>
            {filter ? (
              <div style={DRAWER_FILTER_ROW}>
                <label htmlFor={`${titleId}-filter`} style={DRAWER_FILTER_LABEL}>
                  {filter.label}
                </label>
                <input
                  id={`${titleId}-filter`}
                  type="search"
                  value={filter.value}
                  placeholder={filter.placeholder}
                  onChange={(e) => filter.onChange(e.target.value)}
                  style={DRAWER_FILTER_INPUT}
                />
              </div>
            ) : null}
            {secs.map((s, si) => (
              <div key={s.key || s.title || `sec${si}`} style={DRAWER_SECTION}>
                {s.title ? <Kicker style={DRAWER_SECTION_CAP}>{s.title}</Kicker> : null}
                {(s.rows || []).map((r, ri) => (
                  <div
                    key={r.key || `${si}-r${ri}`}
                    role={r.role}
                    tabIndex={r.tab === undefined || r.tab === null ? undefined : Number(r.tab)}
                    className="ws-hv"
                    onClick={r.onClick}
                    onKeyDown={r.onKey}
                    style={sx(DRAWER_ROW, HOVER.dataRow(), {
                      gridTemplateColumns: s.cols,
                      background: r.bg,
                      cursor: r.cursor,
                    })}
                  >
                    {/* A cell is a `div`, not a `span`: `<StateWord variant="row">`
                        stacks its clause as a block element, and a block inside
                        an inline element is invalid markup the browser will
                        reparent. */}
                    {(r.cells || []).map((c, ci) => (
                      <div
                        key={c.key || `${si}-r${ri}-c${ci}`}
                        style={sx(DRAWER_CELL, {
                          color: c.color,
                          fontFamily: c.font,
                          fontSize: c.fontSize,
                          textDecoration: c.deco,
                        })}
                      >
                        {/* DOC-03: a cell carrying a state word renders it
                          * through `<StateWord>`, which stacks the record's own
                          * `What would prove it:` clause beneath it. Before this
                          * channel existed NO drawer lens could satisfy doctrine
                          * 3 — `cell()` had nowhere to put a clause and this
                          * element rendered `c.text` alone. */}
                        {c.wordSlug ? (
                          <StateWord slug={c.wordSlug} clause={c.clause === undefined ? null : c.clause} variant="row" />
                        ) : null}
                        {c.glyph ? <span aria-hidden="true">{c.glyph} </span> : null}
                        {c.text}
                      </div>
                    ))}
                  </div>
                ))}
                {s.note ? <div style={DRAWER_NOTE}>{s.note}</div> : null}
                {(s.actions || []).map((act, ai) => (
                  <button
                    key={act.label || `${si}-a${ai}`}
                    type="button"
                    className="ws-hv"
                    onClick={act.onClick}
                    style={sx(DRAWER_ACTION, HOVER.raise())}
                  >
                    {act.label}
                  </button>
                ))}
              </div>
            ))}
          </>
        )}
      </div>
    </section>
  )
}
