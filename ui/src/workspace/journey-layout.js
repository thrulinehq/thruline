/* A chronological, horizontal presentation over existing nodes. No evidence
 * joins are created. Chapters bound reading height, not evidence coverage. */
import { estH } from './logic.js'
import { placementSeq } from './lod.js'

/* `heightOf` defaults to `estH`; the Trace passes its own card height (P6-3),
 * which budgets the lines a card adds for where a run ran. */
export function journeyLayout(nodes, visible, widthOf = n => n.w || 280, heightOf = estH) {
  const shown = nodes.filter(visible).slice().sort((a, b) => {
    const x = placementSeq(a), y = placementSeq(b)
    if (x == null && y != null) return 1
    if (x != null && y == null) return -1
    return (x ?? 0) - (y ?? 0) || a.id.localeCompare(b.id)
  })
  const pos = new Map(), frames = [], chapterFor = new Map()
  let x = 0
  for (let offset = 0; offset < shown.length; offset += 12) {
    const list = shown.slice(offset, offset + 12)
    const ordinal = frames.length + 1, id = `journey-${ordinal}`
    const rowHeights = [0, 0, 0], colWidths = [0, 0, 0, 0]
    list.forEach((n, i) => {
      const w = widthOf(n)
      rowHeights[i % 3] = Math.max(rowHeights[i % 3], heightOf(n, w))
      colWidths[Math.floor(i / 3)] = Math.max(colWidths[Math.floor(i / 3)], w)
    })
    const dx = colWidths.map((_, i) => colWidths.slice(0, i).reduce((s, w) => s + w + 48, 0))
    const dy = rowHeights.map((_, i) => rowHeights.slice(0, i).reduce((s, h) => s + h + 30, 0))
    list.forEach((n, i) => {
      pos.set(n.id, { x: x + 20 + dx[Math.floor(i / 3)], y: 55 + dy[i % 3], w: widthOf(n) })
      chapterFor.set(n.id, id)
    })
    const first = list.map(placementSeq).find(s => s != null)
    const last = list.map(n => n.lastSeq ?? placementSeq(n)).filter(s => s != null).at(-1)
    const w = colWidths.filter(Boolean).reduce((s, w) => s + w + 48, 0) - 8
    const h = 75 + rowHeights.filter(Boolean).reduce((s, h) => s + h + 30, 0)
    const alerts = list.filter(n => n.risk || n.finding || n.fam === 'red' || n.fam === 'amber').length
    frames.push({ id, x, y: 0, w, h, firstNode: list[0].id, firstSeq: first, lastSeq: last, n: list.length,
      title: `PART ${ordinal} · ${first == null ? 'LINKED EVIDENCE' : `EVENTS ${first}–${last}`}`,
      count: `${list.length} steps${alerts ? ` · ${alerts} review points` : ''}`, alerts })
    x += w + 64
  }
  for (const n of nodes) if (!pos.has(n.id)) {
    const parent = pos.get(n.member)
    pos.set(n.id, parent ? { ...parent, w: widthOf(n) } : { x: n.x || 0, y: n.y || 0, w: widthOf(n) })
  }
  return { pos, frames, chapterFor, frameH: new Map(frames.map(f => [f.id, f.h])) }
}
