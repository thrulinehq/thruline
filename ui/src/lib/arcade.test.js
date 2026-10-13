/* The cast, canvas-contained (RA5, design v2).
 *
 * The macOS chrome speaks system typography; the arcade survives as the
 * canvas's inhabitants. Same discipline as every skin before it: the skin may
 * gamify the look, never the information; every sprite is bound to a record;
 * the benched ghost renders on no product path; and the mock gallery never
 * ships. */

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

import { CAST, CAST_FOR_KIND, CHOMP, NIB_FRAMES, PAL, GRAY, sprite, HOLLOW } from './arcade.js'
import { arcLengths, chompFrameAt, pointAt, segmentEase } from './walk.js'

const here = dirname(fileURLToPath(import.meta.url))
const src = (rel) => readFileSync(join(here, '..', rel), 'utf8')

/* ── §5w: the benched ghost ─────────────────────────────────────────── */

test('HOLLOW ships as an asset', () => {
  assert.ok(HOLLOW.length > 0, 'the design ships; only the appearance is withheld')
  assert.equal(CAST.hollow.name, 'HOLLOW')
  assert.equal(CAST.hollow.word, 'no proof found')
})

test('the benched ghost has no product render path', () => {
  const benched = Object.entries(CAST).filter(([, m]) => m.bench).map(([k]) => k)
  assert.deepEqual(benched, ['hollow'])
  assert.ok(!Object.values(CAST_FOR_KIND).includes('hollow'))

  // The one seat-granting module refuses a benched member by construction.
  const sprites = src('mac/sprites.jsx')
  assert.match(sprites, /if \(!member \|\| member\.bench\) return null/, 'GhostAt must refuse a benched member')

  // Nothing outside the asset module, the EXHIBIT, and this test names it.
  // cast.jsx is the one deliberate exception (RA6 amendment B): a marked
  // design exhibit on its own route, where HOLLOW appears as a shown design
  // with the bench notice verbatim — never as a product appearance.
  for (const file of ['mac/sprites.jsx', 'mac/graph.jsx', 'mac/work-map.jsx', 'mac/decision-flow.jsx', 'mac/shell.jsx', 'mac/inspector.jsx', 'os/shell.jsx', 'main.jsx', 'gallery.jsx']) {
    assert.ok(!/hollow/i.test(src(file)), `${file} names the benched ghost`)
  }
  const exhibit = src('cast.jsx')
  assert.match(exhibit, /DESIGN EXHIBIT/, 'the exhibit must mark itself')
  assert.match(
    exhibit,
    /Designed this round, allowed on no screen until graduation rules it in\./,
    'the bench notice renders verbatim',
  )
  // And no real-data surface imports the exhibit: only main's ?cast route.
  for (const file of ['mac/shell.jsx', 'mac/graph.jsx', 'mac/work-map.jsx', 'mac/decision-flow.jsx', 'mac/inspector.jsx', 'os/shell.jsx', 'gallery.jsx']) {
    assert.ok(!/cast\.jsx/.test(src(file)), `${file} imports the exhibit into a data surface`)
  }
  assert.match(src('main.jsx'), /has\('cast'\)/, 'the exhibit is route-gated')
})

/* ── the cast lives in the canvas only (RA5 directive 4) ────────────── */

test('sprites stay in the retained graph canvas and do not enter the vertical flow or chrome', () => {
  assert.match(src('mac/graph.jsx'), /from '\.\/sprites\.jsx'/)
  for (const chrome of ['mac/work-map.jsx', 'mac/decision-flow.jsx', 'mac/shell.jsx', 'mac/inspector.jsx', 'main.jsx', 'refresh.jsx']) {
    assert.ok(!/sprites\.jsx|GhostAt|NibOnTrail/.test(src(chrome)), `${chrome} summons the cast outside the canvas`)
  }
  // And each ghost is seated on its own record's state, behind the filter.
  const graph = src('mac/graph.jsx')
  assert.match(graph, /state === 'contradicted' && !dimmed\(n\)/)
  assert.match(graph, /state === 'stale' && !dimmed\(n\)/)
  assert.match(graph, /who="snag"/)
  assert.match(graph, /who="dusk"/)
})

test('NIB walks only a trail the layout derived from eaten records', () => {
  assert.match(src('mac/graph.jsx'), /g\.trails\.map\(\(t\) => \(\s*<NibOnTrail/)
  assert.match(src('lib/layout.js'), /const EATEN = new Set\(\['supported', 'verified', 'recorded'\]\)/)
  assert.match(src('lib/layout.js'), /eaten\.length >= 2/, 'a trail of one point is not a walk')
  // RA6 motion pass: reduced motion parks the final frame in the walker
  // itself — the loop never starts — and ghosts hold still by design law.
  const sprites = src('mac/sprites.jsx')
  assert.match(sprites, /prefers-reduced-motion/, 'the walker must consult the OS setting')
  assert.match(sprites, /if \(mql\.matches\) \{[\s\S]{0,200}place\(total, 0\)/, 'reduced motion parks the trail end')
  // GhostAt renders static frames only: no animation, no loop, no charm.
  const ghost = sprites.slice(sprites.indexOf('function GhostAt'), sprites.indexOf('the walk, as craft'))
  for (const banned of ['requestAnimationFrame', 'animation', 'setInterval']) {
    assert.ok(!ghost.includes(banned), `a ghost animates: ${banned}`)
  }
})

test('the chomp lands open exactly at pellet contact, at any spacing', () => {
  // The RB-era pin holds the data (CHOMP[0] === 2); this holds the runtime:
  // distance 0 → the fully-open frame, mid-leg → closed, and the phase is
  // total over the leg no matter its length.
  for (const spacing of [1, 42, 220, 1000]) {
    assert.equal(chompFrameAt(0, spacing), CHOMP[0], `contact must be open at spacing ${spacing}`)
    assert.equal(chompFrameAt(spacing / 2, spacing), 0, `mid-leg must pass closed at spacing ${spacing}`)
  }
  assert.equal(chompFrameAt(0, 0), CHOMP[0], 'degenerate spacing still lands open')
})

test('the walk is arc-length true and eased inside each segment', () => {
  const pts = [{ x: 0, y: 0 }, { x: 100, y: 0 }, { x: 100, y: 50 }]
  const cum = arcLengths(pts)
  assert.deepEqual(cum, [0, 100, 150])
  // Segment midpoint in arc length is the eased midpoint in space.
  const mid = pointAt(pts, cum, 50)
  assert.equal(mid.seg, 0)
  assert.ok(Math.abs(mid.x - 50) < 1e-9, 'ease-in-out is symmetric at the midpoint')
  // Easing is monotone and clamped — no overshoot robot could hide in.
  let prev = -1
  for (let t = 0; t <= 1.0001; t += 0.05) {
    const v = segmentEase(Math.min(1, t))
    assert.ok(v >= prev - 1e-12 && v >= 0 && v <= 1, 'easing must be monotone in [0,1]')
    prev = v
  }
  // Ends of the trail resolve exactly.
  assert.equal(pointAt(pts, cum, 0).x, 0)
  const end = pointAt(pts, cum, 150)
  assert.ok(Math.abs(end.y - 50) < 1e-9)
})

/* ── sprite integrity ───────────────────────────────────────────────── */

test('NIB is the v2 three-frame chomper and the chomp lands open on contact', () => {
  assert.equal(NIB_FRAMES.length, 3, 'closed, half, open')
  for (const rows of NIB_FRAMES) {
    assert.equal(rows.length, 13)
    for (const r of rows) assert.equal(r.length, 13)
  }
  assert.deepEqual(CHOMP, [2, 1, 0, 1])
})

test('every cast member carries a glyph and a word', () => {
  for (const [key, m] of Object.entries(CAST)) {
    assert.ok(m.glyph && m.glyph.length > 0, `${key} has no glyph`)
    assert.ok(m.word && m.word.length > 0, `${key} has no word`)
    assert.ok(m.frames.length > 0, `${key} has no sprite`)
  }
})

test('every sprite has a grayscale print', () => {
  for (const [key, m] of Object.entries(CAST)) {
    const colour = sprite(m.frames[0], 3, PAL)
    const gray = sprite(m.frames[0], 3, GRAY)
    assert.equal(colour.w, gray.w, `${key} changes shape in grayscale`)
    assert.equal(colour.shadow.split(',').length, gray.shadow.split(',').length)
  }
})

/* ── the mock tooling stays off the product routes ──────────────────── */

test('the state gallery is reachable only behind the mock-tooling flag', () => {
  const main = src('main.jsx')
  assert.match(main, /import\.meta\.env\.VITE_MOCK_TOOLING === '1'/)
  assert.match(main, /await import\('\.\/gallery\.jsx'\)/)
  assert.equal([...main.matchAll(/gallery\.jsx/g)].length, 1)
  assert.match(src('gallery.jsx'), /MOCK TOOLING/)
  // Mock rows wear unmistakably fake names (RA5 directive 5).
  for (const m of src('gallery.jsx').matchAll(/ROW\(\s*'([^']+)'/g)) {
    assert.match(m[1], /^MOCK-/, `a mock project name could pass for real: ${m[1]}`)
  }
})

test('candidate altitudes never wear a meaning\'s name (RB7 proxy rule)', () => {
  // Deterministic Layer 2 speaks candidate vocabulary. The Work Map may now
  // mount a separately labelled, server-validated shadow branch, but the
  // derivation and the retained deterministic reference remain meaning-free.
  const text = [src('mac/distilled.jsx'), src('lib/work-map.js')].join('\n').replace(/\/\*[\s\S]*?\*\//g, '')
  for (const word of ['decision', 'Decision', 'dead end', 'dead-end', 'purpose', 'Purpose', 'abandoned', 'Abandoned', 'outcome', 'Outcome']) {
    assert.ok(!text.includes(word), `a candidate surface wears a meaning's name: ${word}`)
  }
  assert.match(text, /unlabeled — awaiting interpretation/, 'the Layer-3 slot must render, honestly empty')
  assert.match(text, /unresolved/, 'the tail speaks candidate vocabulary')
  assert.equal([...text.matchAll(/fetch\(/g)].length, 0, 'the altitude renders what it is handed')
  const component = src('mac/work-map.jsx')
  assert.match(component, /Local interpretation/)
  assert.match(component, /Model-proposed · not a decision/)
  assert.match(component, /cannot alter candidate or receipt truth/)
})

test('the recovery surfaces exist only in the marked gallery (RB0 pin, v3 teeth)', () => {
  // RA6: the designed recovery surfaces are all shadow classes. The gallery
  // holds them on MOCK- names; no product file may render a stall verdict, a
  // suspected cause, or a recovery metric until RB2+ graduate them.
  const mock = src('gallery.jsx')
  for (const marker of ['P1 STALLED', 'Recover now', 'Pause &amp; preserve', 'Recovery readiness']) {
    assert.ok(mock.includes(marker), `the designed surface lost its mock: ${marker}`)
  }
  for (const file of ['mac/shell.jsx', 'mac/work-map.jsx', 'mac/decision-flow.jsx', 'mac/graph.jsx', 'mac/inspector.jsx', 'os/shell.jsx', 'main.jsx', 'refresh.jsx']) {
    const text = src(file).replace(/\/\*[\s\S]*?\*\//g, '')
    for (const marker of ['stalled', 'STALLED', 'suspected', 'readiness', 'heartbeat', 'Recover now', 'Pause &amp; preserve', 'confidence']) {
      assert.ok(!text.includes(marker), `${file} renders shadow vocabulary before graduation: ${marker}`)
    }
  }
})

test('the state gallery is absent from a production build', (t) => {
  const dist = join(here, '..', '..', 'dist', 'assets')
  let files
  try {
    files = readdirSync(dist)
  } catch {
    t.diagnostic('dist/ not built — run `npm run build` for this gate to mean anything')
    t.skip('no production build present')
    return
  }
  const bundle = files.filter((f) => f.endsWith('.js')).map((f) => readFileSync(join(dist, f), 'utf8'))
  assert.ok(bundle.length > 0, 'no JS in the production build')
  for (const text of bundle) {
    assert.ok(!text.includes('MOCK TOOLING'), 'the state gallery reached a production bundle')
    assert.ok(!text.includes('MOCK-payments-demo'), 'mock data reached a production bundle')
    assert.ok(!text.includes('P1 STALLED'), 'the shadow recovery surface reached a production bundle')
    assert.ok(!text.includes('Recovery readiness'), 'a shadow metric reached a production bundle')
  }
})

/* ── §5u: nothing on a screen executes work ─────────────────────────── */

test('no macOS surface can execute work, and the load discipline holds', () => {
  for (const file of ['mac/shell.jsx', 'mac/work-map.jsx', 'mac/decision-flow.jsx', 'mac/graph.jsx', 'mac/inspector.jsx', 'mac/sprites.jsx', 'os/shell.jsx', 'gallery.jsx', 'main.jsx']) {
    const text = src(file)
    for (const forbidden of ['child_process', 'exec(', 'spawn(', 'eval(', 'new Function']) {
      assert.ok(!text.includes(forbidden), `${file} contains ${forbidden}`)
    }
  }
  // main fetches the index; the shell has exactly one project load plus the
  // two fixed same-origin local-mind endpoints. Interpretation is user-run.
  assert.equal([...src('main.jsx').matchAll(/fetch\(/g)].length, 1)
  assert.match(src('main.jsx'), /fetch\(INDEX/)
  const mac = src('mac/shell.jsx')
  assert.equal([...mac.matchAll(/fetch\(/g)].length, 3)
  assert.equal([...mac.matchAll(/fetch\(PROJECT\(r\.fixture\)/g)].length, 1)
  assert.equal([...mac.matchAll(/fetch\(MIND_ENDPOINTS\.(?:status|interpret)/g)].length, 2)
  assert.match(mac, /const sourceText = await res\.text\(\)/)
  assert.match(mac, /fixtureSourceDigest\(sourceText\)/)
  assert.match(mac, /JSON\.parse\(sourceText\)/)
  assert.doesNotMatch(mac, /fixtureSourceDigest\(JSON\.stringify/)
  assert.equal([...src('mac/graph.jsx').matchAll(/fetch\(/g)].length, 0)
  assert.equal([...src('mac/decision-flow.jsx').matchAll(/fetch\(/g)].length, 0)
  assert.equal([...src('mac/work-map.jsx').matchAll(/fetch\(/g)].length, 0)
  // The OS shell (RA6) adds exactly two reads: the lanes presence poll and
  // the ledger tab's one fixture. Nothing else.
  const os = src('os/shell.jsx')
  assert.equal([...os.matchAll(/fetch\(/g)].length, 2)
  assert.match(os, /fetch\('\/api\/lanes'/)
  assert.match(os, /fetch\(PROJECT\(row\.fixture\)/)
})
