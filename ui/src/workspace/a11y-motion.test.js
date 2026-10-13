/* T-6-4 · D — reduced motion resolves to the FINISHED state (design record §4).
 *
 * Two halves, because motion lives in two places:
 *
 *  - the stylesheet: every `@keyframes` is played only inside
 *    `prefers-reduced-motion: no-preference`, so under `reduce` nothing plays;
 *    and the state an element rests in when nothing plays IS the animation's
 *    last frame (the thread line whole, the Read's header in place, a revealed
 *    row fully shown) — so turning motion off never leaves a half-drawn line
 *    or a faded row. The three cards' own surfaces (the shell, the Trace, the
 *    Brief) carry no animation or transition outside that opt-in.
 *  - the JavaScript: the canvas asks `prefers-reduced-motion` itself, and
 *    under a matched query (a stubbed `matchMedia` in jsdom) it renders the lit
 *    thread whole with no drawing mask and every card with no transition;
 *    with no preference it renders the mask and the transition as P6-3 built.
 */
import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { randomUUID } from 'node:crypto'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'
import { prefersReducedMotion, REDUCED_MOTION_QUERY } from './reduced-motion.js'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-a11y-motion-'))
after(() => rm(temporary, { recursive: true, force: true }))
const CSS = readFileSync(join(here, 'workspace.css'), 'utf8')

/* THE BUNDLE IS BUILT ONCE, BEFORE ANY TEST IS REGISTERED (T-6-4 fix round 1).
 * It used to be built at the point of first use, below the async `landing`
 * test. node:test starts running a registered test while this module is still
 * awaiting, so that test called `bundle()` a second time, before the first
 * had set its cache, and both wrote the same `motion.mjs`. One import could
 * then read the file half-written, or after the other's `after()` removed it.
 * The fix has three parts. The promise is memoised, not its result, so a
 * second caller waits for the first build instead of starting another. Each
 * build writes its own file (`randomUUID`), so no two writes share a path.
 * And the one build finishes (`await`) before the first `test()` exists. */
let bundling = null
function bundle() {
  if (!bundling) bundling = (async () => {
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
          import { discrepancyAnchor } from './trace-view.js';
          import { landing } from './checking-sequence.jsx';
          export const html = (el) => renderToStaticMarkup(el);
          export { Workspace, adaptSession, TRACE_DOCS, discrepancyAnchor, landing };
        `,
      },
    })
    const file = join(temporary, 'motion-' + randomUUID() + '.mjs')
    await writeFile(file, result.outputFiles[0].text)
    return await import(pathToFileURL(file).href)
  })()
  return bundling
}
const M = await bundle()


/* ── the stylesheet ───────────────────────────────────────────────────── */

const stripComments = (s) => s.replace(/\/\*[\s\S]*?\*\//g, '')
/** Every rule with its selector, the at-rules around it, and its declarations. */
function rules(css) {
  const out = []
  const stack = []
  let buf = ''
  for (const ch of stripComments(css)) {
    if (ch === '{') { stack.push(buf.trim()); buf = '' } else if (ch === '}') {
      const prelude = stack.pop()
      if (buf.trim() || !prelude.startsWith('@')) {
        const decls = {}
        for (const d of buf.split(';')) {
          const i = d.indexOf(':')
          if (i > 0) decls[d.slice(0, i).trim().toLowerCase()] = d.slice(i + 1).trim()
        }
        out.push({ selector: prelude, at: stack.filter((p) => p.startsWith('@')), decls })
      }
      buf = ''
    } else buf += ch
  }
  return out
}
const RS = rules(CSS)
const NO_PREF = '@media (prefers-reduced-motion: no-preference)'
/* What an element shows when no rule sets the property: the value it rests at. */
const DEFAULTS = { opacity: '1', transform: 'none' }

test('D · every @keyframes plays only when motion is welcome, and nothing is left looping', () => {
  const frames = [...new Set(RS.filter((r) => r.at.some((a) => a.startsWith('@keyframes '))).map((r) => r.at.find((a) => a.startsWith('@keyframes ')).slice(11).trim()))]
  assert.ok(frames.length >= 3, `found ${frames.join(', ')}`)
  for (const name of frames) {
    const users = RS.filter((r) => new RegExp(`\\b${name}\\b`).test(r.decls.animation || r.decls['animation-name'] || ''))
    assert.ok(users.length > 0, `${name} is declared and never played`)
    for (const u of users) assert.ok(u.at.includes(NO_PREF), `${u.selector} plays ${name} outside ${NO_PREF}`)
    for (const u of users) assert.doesNotMatch(u.decls.animation, /infinite|alternate/, `${u.selector} loops`)
  }
})

test('D · with motion off, every animated element rests in the animation\'s LAST frame: the thread line whole, the Read in place, a revealed row fully shown', () => {
  const frames = RS.filter((r) => r.selector === 'to' || r.selector === '100%')
  assert.ok(frames.length >= 3)
  let checked = 0
  for (const to of frames) {
    const name = to.at.find((a) => a.startsWith('@keyframes ')).slice(11).trim()
    const users = RS.filter((r) => new RegExp(`\\b${name}\\b`).test(r.decls.animation || ''))
    for (const u of users) {
      for (const sel of u.selector.split(/\s*,\s*/)) {
        for (const [prop, value] of Object.entries(to.decls)) {
          /* the resting rule: the same selector, outside any media query */
          const rest = RS.filter((r) => r.at.length === 0 && r.selector.split(/\s*,\s*/).includes(sel) && r.decls[prop] != null)
          const resting = rest.length ? rest[rest.length - 1].decls[prop] : DEFAULTS[prop]
          assert.equal(resting, value, `${sel} rests at ${prop}: ${resting} with motion off, but ${name} ends at ${value}`)
          checked += 1
        }
      }
    }
  }
  assert.ok(checked >= 5, `only ${checked} last-frame properties were checked`)
  /* the thread line in particular: its resting dash offset is 0 — fully drawn */
  const draw = RS.find((r) => r.at.length === 0 && r.selector === '.ws-trace-draw')
  assert.equal(draw.decls['stroke-dashoffset'], '0')
})

test('D · the reduce block reaches every screen root, and the three cards\' own surfaces carry no motion outside the opt-in', () => {
  const reduce = RS.find((r) => r.at.includes('@media (prefers-reduced-motion: reduce)') && /\.ws-root \*,/.test(r.selector))
  assert.ok(reduce)
  assert.equal(reduce.decls.animation, 'none !important')
  assert.equal(reduce.decls.transition, 'none !important')
  const OWN = /\.ws-(shell|trace|brief)\b|\.ws-shell-|\.ws-trace-|\.ws-brief-/
  for (const r of RS.filter((x) => OWN.test(x.selector))) {
    for (const prop of ['animation', 'transition', 'animation-name', 'transition-property']) {
      if (r.decls[prop] == null || /^none/.test(r.decls[prop])) continue
      assert.ok(r.at.includes(NO_PREF), `${r.selector} moves (${prop}: ${r.decls[prop]}) outside ${NO_PREF}`)
    }
  }
  /* nothing the shell draws needs the reduce block: it does not move at all */
  assert.deepEqual(RS.filter((x) => /\.ws-shell/.test(x.selector) && (x.decls.animation || x.decls.transition)).map((x) => x.selector), [])
})

test('D · the Read lands at once under reduced motion: no timed ticking, no landing animation', () => {
  const { landing } = M
  assert.equal(landing({ kind: 'ready' }, true, 5, true), 'immediate')
  assert.equal(landing({ kind: 'ready' }, false, 5, true), 'play', 'the guard is live: with motion welcome it plays')
  const src = readFileSync(join(here, 'verdict-panel.jsx'), 'utf8')
  assert.match(src, /play > 0 && !reduced \? ' ws-verdict-landing' : ''/, 'the landing class is never given under reduced motion')
})

/* ── the JavaScript ───────────────────────────────────────────────────── */

test('D · prefersReducedMotion reads the one query, and a missing or broken matchMedia reads as no preference', () => {
  const asked = []
  const win = { matchMedia: (q) => { asked.push(q); return { matches: true } } }
  assert.equal(prefersReducedMotion(win), true)
  assert.deepEqual(asked, [REDUCED_MOTION_QUERY])
  assert.equal(prefersReducedMotion({ matchMedia: () => ({ matches: false }) }), false)
  assert.equal(prefersReducedMotion({}), false)
  assert.equal(prefersReducedMotion({ matchMedia: () => { throw new Error('blocked') } }), false)
  assert.equal(prefersReducedMotion(null), false)
})


/** The workspace at Explore with the discrepancy's thread lit, rendered under a window that answers `matchMedia` as given. */
function renderUnder(reduce) {
  const dom = new JSDOM('<!doctype html><body></body>', { url: 'http://127.0.0.1/?altitude=explore' })
  dom.window.matchMedia = (q) => ({ matches: q === REDUCED_MOTION_QUERY ? reduce : false, media: q, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {} })
  const held = Object.getOwnPropertyDescriptor(globalThis, 'window')
  Object.defineProperty(globalThis, 'window', { value: dom.window, configurable: true, writable: true })
  try {
    const s = M.adaptSession(M.TRACE_DOCS)
    const w = new M.Workspace({ index: { projects: [], totals: {}, kind: 'index', contract_version: 1 } })
    w.state = {
      ...w.state,
      altitude: 'explore', session: s.key, corpus: { order: [s.key], sessions: { [s.key]: s } }, dataStatus: 'ready',
      cw: 40000, ch: 40000, view: { x: 0, y: 0, z: 1 }, focusMode: 'everything', sel: M.discrepancyAnchor(s),
    }
    return new JSDOM('<!doctype html><body>' + M.html(w.render()) + '</body>').window.document
  } finally {
    if (held) Object.defineProperty(globalThis, 'window', held)
    else delete globalThis.window
    dom.window.close()
  }
}

test('D · under a matched prefers-reduced-motion query the lit thread is drawn whole — no drawing mask — and no card carries a transition', () => {
  const d = renderUnder(true)
  const lit = [...d.querySelectorAll('g[data-lit="yes"]')]
  assert.ok(lit.length >= 2, 'a thread is lit')
  for (const g of lit) {
    assert.equal(g.querySelector('mask'), null, 'no drawing mask under reduced motion')
    assert.equal(g.querySelector(':scope > path').getAttribute('mask'), null, 'the line is not masked: it is whole')
  }
  assert.equal(d.querySelector('.ws-trace-draw'), null)
  const cards = [...d.querySelectorAll('[role="application"] [role="button"][data-id]')]
  assert.ok(cards.length > 5)
  for (const c of cards) assert.match(c.getAttribute('style'), /transition:\s*none/, `${c.getAttribute('data-id')} still transitions`)
})

test('D · with no preference the same render keeps P6-3\'s one-time draw and the card transition — the guard above is live', () => {
  const d = renderUnder(false)
  const lit = [...d.querySelectorAll('g[data-lit="yes"]')]
  assert.ok(lit.length >= 2)
  for (const g of lit) assert.ok(g.querySelector('mask path.ws-trace-draw'), 'the drawing mask is there')
  const card = d.querySelector('[role="application"] [role="button"][data-id]')
  assert.match(card.getAttribute('style'), /transition:\s*opacity/)
})
