/* T-6-4 · E — the Outline is the accessible EQUIVALENT of the picture
 * (design record §3.3), not a summary of it.
 *
 * For every synthetic session — the Trace fixture (every event kind, a closed
 * fold), the three bundled demo records (one with a closed Focus group), and
 * the Trace fixture again with one line of a kind this view does not know —
 * the set of card ids the Outline lists equals the set the picture's model
 * draws: the cards `laneLayout` lays out, plus every card folded inside a
 * drawn group (recursively). Every lane is named, empty or not. Every line on
 * the lit thread is named by its kind in words, at both of its ends. The list
 * is a named region with `aria-current` on the selected row, and what the
 * thread bar says is a polite live region.
 */
import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'
import { HQ_COPY } from './hq-copy.js'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-a11y-outline-'))
after(() => rm(temporary, { recursive: true, force: true }))

const result = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  stdin: {
    resolveDir: here,
    contents: `
      import { renderToStaticMarkup } from 'react-dom/server';
      import { Workspace } from './shell.jsx';
      import { adaptSession } from './adapter.js';
      import { TRACE_DOCS } from './trace-fixture.js';
      import { DEMO_ORDER, demoSession } from './demo-data.js';
      import { laneLayout, edgeKindOf, edgeWord, discrepancyAnchor, LANES, laneLabel } from './trace-view.js';
      export const html = (el) => renderToStaticMarkup(el);
      export { Workspace, adaptSession, TRACE_DOCS, DEMO_ORDER, demoSession, laneLayout, edgeKindOf, edgeWord, discrepancyAnchor, LANES, laneLabel };
    `,
  },
})
const file = join(temporary, 'outline.mjs')
await writeFile(file, result.outputFiles[0].text)
const M = await import(pathToFileURL(file).href)

const T = HQ_COPY.trace
const doc = (html) => new JSDOM('<!doctype html><body>' + html + '</body>').window.document

/** A workspace instance over one session, state set directly (as trace.test.js does). */
function workspace(s, over) {
  const w = new M.Workspace({ index: { projects: [], totals: {}, kind: 'index', contract_version: 1 } })
  w.setState = (u, then) => {
    w.state = { ...w.state, ...(typeof u === 'function' ? u(w.state) : u) }
    if (then) then()
  }
  w.toast = () => {}
  w.log = () => {}
  w.state = {
    ...w.state,
    altitude: 'explore',
    session: s.key,
    corpus: { order: [s.key], sessions: { [s.key]: s } },
    dataStatus: 'ready',
    cw: 40000,
    ch: 40000,
    view: { x: 0, y: 0, z: 1 },
    doc: 'trace',
    outline: true,
    ...(over || {}),
  }
  return w
}

/** The Trace fixture with one line of a kind this view does not know. */
function withUnknownKind() {
  const s = M.adaptSession(M.TRACE_DOCS)
  const a = s.nodes.find((n) => n.objType === 'promise')
  const b = s.nodes.find((n) => n.objType === 'claim')
  a.pinsR = a.pinsR.concat([{ id: 'x', label: 'x', t: 'ref' }])
  b.pinsL = b.pinsL.concat([{ id: 'x', label: 'x', t: 'ref' }])
  s.wires = s.wires.concat([{ id: 'w_unknown', from: [a.id, 'x'], to: [b.id, 'x'], kind: 'xref', c: 'grey', label: 'unknown' }])
  return s
}

const SESSIONS = [
  ['the Trace fixture', () => M.adaptSession(M.TRACE_DOCS)],
  ...M.DEMO_ORDER.map((k) => [`demo record ${k}`, () => M.demoSession(k)]),
  ['the Trace fixture with a line of an unknown kind', withUnknownKind],
]

/** What the picture's model draws: laneLayout's cards, plus every card folded inside a drawn group. */
function drawnIds(w) {
  const G = w.G()
  const visible = (n) => w.visible(n)
  const lay = M.laneLayout(G.nodes, { visible })
  const shown = G.nodes.filter(visible)
  assert.equal(lay.lanes.reduce((t, l) => t + l.n, 0), shown.length, 'laneLayout lays out exactly the visible cards')
  const byId = new Map(G.nodes.map((n) => [n.id, n]))
  const out = new Set()
  const add = (n) => {
    if (!n || out.has(n.id)) return
    out.add(n.id)
    if (n.group && Array.isArray(n.members)) n.members.forEach((m) => add(byId.get(m)))
  }
  shown.forEach(add)
  return out
}

let sawClosedGroup = false
let threadsChecked = 0
let linesChecked = 0

for (const [name, make] of SESSIONS) {
  test(`E · ${name}: the Outline lists every card the picture draws — folded ones included — and nothing else, and names all four lanes`, () => {
    const s = make()
    assert.ok(s, name)
    const w = workspace(s)
    const want = drawnIds(w)
    if (w.visibleNodes().some((n) => n.group && Array.isArray(n.members) && n.members.length)) sawClosedGroup = true
    const d = doc(M.html(w.render()))
    const region = d.querySelector('.ws-trace-outline')
    assert.ok(region, 'the Outline is drawn')
    assert.ok(d.querySelector('[role="application"]') === null, 'with the Outline showing there is no canvas: the list is the only way in')
    const listed = [...region.querySelectorAll('[data-section="all"] .ws-trace-outline-item')].map((b) => b.getAttribute('data-id'))
    assert.equal(new Set(listed).size, listed.length, 'each card is listed once')
    assert.deepEqual([...listed].sort(), [...want].sort())
    /* every lane, by name, with what it holds */
    const lanes = [...region.querySelectorAll('.ws-trace-outline-lanes li')]
    assert.deepEqual(lanes.map((l) => l.getAttribute('data-lane')), [...M.LANES])
    for (const l of lanes) assert.ok(l.textContent.startsWith(M.laneLabel(l.getAttribute('data-lane')) + ' lane: '), l.textContent)
  })
}

test('E · at least one session had a closed group whose cards are listed inside it', () => {
  assert.ok(sawClosedGroup, 'no session exercised a closed group — the parity above proves nothing about folds')
})

for (const [name, make] of SESSIONS) {
  test(`E · ${name}: with the discrepancy's thread lit, every line on it is named by its kind in words at both ends, the thread is listed with each card's place on it, and it is announced`, () => {
    const s = make()
    const anchor = M.discrepancyAnchor(s)
    if (!anchor) return
    const w = workspace(s, { sel: anchor })
    const t = w.thread()
    if (!t) return
    const d = doc(M.html(w.render()))
    const region = d.querySelector('.ws-trace-outline')
    /* the landmark and its selection */
    assert.equal(region.getAttribute('role'), 'region')
    assert.equal(region.getAttribute('aria-label'), T.outline.label)
    const current = [...region.querySelectorAll('[aria-current="true"]')].map((e) => e.getAttribute('data-id'))
    assert.ok(current.length >= 1 && current.every((id) => id === anchor), 'aria-current marks the selected card and nothing else')
    /* the thread bar's words are a polite live region */
    const live = region.querySelector('[role="status"][aria-live="polite"]')
    assert.ok(live && live.textContent === w.threadBar().line, 'the thread bar is spoken')
    /* the thread, in order, each with "n of N" */
    const thread = [...region.querySelectorAll('[data-section="thread"] .ws-trace-outline-item')]
    assert.deepEqual(thread.map((b) => b.getAttribute('data-id')), t.ids)
    thread.forEach((b, i) => assert.ok(b.textContent.includes(`${i + 1} of ${t.ids.length} on the thread`), b.textContent))
    /* every line on the thread, named by kind at both ends */
    const text = (id) => {
      const el = region.querySelector(`[data-section="all"] [data-id="${id}"]`) || region.querySelector(`[data-section="thread"] [data-id="${id}"]`)
      return el ? el.textContent : ''
    }
    for (const wid of t.wires) {
      const wire = s.wires.find((x) => x.id === wid)
      assert.ok(wire, wid)
      const word = M.edgeWord(M.edgeKindOf(wire))
      for (const end of [wire.from[0], wire.to[0]]) assert.ok(text(end).includes(word + ': '), `the line ${wid} (${word}) is not named at ${end}`)
      linesChecked += 1
    }
    threadsChecked += 1
  })
}

test('E · a line of a kind this view does not know is named as itself in the Outline, never as "inferred"', () => {
  const s = withUnknownKind()
  const d = doc(M.html(workspace(s).render()))
  const region = d.querySelector('.ws-trace-outline')
  assert.ok(region.textContent.includes(T.edges.unclassified + ': '), 'the unknown line is in words')
})

test('E · the thread checks above were not vacuous', () => {
  assert.ok(threadsChecked >= 3, `only ${threadsChecked} sessions had a thread to check`)
  assert.ok(linesChecked >= 6, `only ${linesChecked} thread lines were checked`)
})

test('E · recorded order is named too: every drawn card that has a neighbour in the record says so', () => {
  const s = M.adaptSession(M.TRACE_DOCS)
  const w = workspace(s, { focusMode: 'everything' })
  const order = w.allWires().filter((x) => x.kind === 'order')
  assert.ok(order.length > 3, 'the picture draws recorded-order lines')
  const d = doc(M.html(w.render()))
  for (const o of order) {
    const el = d.querySelector(`.ws-trace-outline [data-section="all"] [data-id="${o.a}"]`)
    assert.ok(el && el.textContent.includes(T.edges.order + ': '), `${o.a} does not name its recorded-order neighbour`)
  }
})
