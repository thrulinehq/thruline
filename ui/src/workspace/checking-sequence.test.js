import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { HQ_COPY } from './hq-copy.js'

const temporary = await mkdtemp(join(tmpdir(), 'thruline-checking-'))
after(() => rm(temporary, { recursive: true, force: true }))
const result = await build({
  stdin: { contents: `
    import React from 'react';
    import { renderToStaticMarkup } from 'react-dom/server';
    import { CheckingSequence } from './checking-sequence.jsx';
    export * from './checking-sequence.jsx';
    export const render = props => renderToStaticMarkup(React.createElement(CheckingSequence, props));
  `, resolveDir: dirname(fileURLToPath(import.meta.url)) },
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
})
const compiled = join(temporary, 'checking-sequence.mjs')
await writeFile(compiled, result.outputFiles[0].text)
const M = await import(pathToFileURL(compiled).href)

/* The contract test's real three-event run (docs/UI-SERVER-CONTRACT.md §2.1.1),
 * with elapsed times that vary so proportionality is observable. */
const PHASES = [
  { phase: 'read_record', count: 3, detail: 'session s1 · 3 events', elapsed_ms: 40 },
  { phase: 'find_claims', count: 1, detail: 'clm_7ZQ4M2K9PX3B8T1R6W5N0C4VHD', elapsed_ms: 5 },
  { phase: 'match_proof', count: 0, detail: 'no check cited', elapsed_ms: 0 },
  { phase: 'workspace_since', count: 2, detail: '0 edits · 2 trees', elapsed_ms: 12 },
  { phase: 'run_verifiers', count: 1, detail: 'tests_pass@1', elapsed_ms: 900 },
  { phase: 'write_up', count: 0, detail: '0 findings', elapsed_ms: 2 },
]
const SESSION = { id: 's1', label: 'live-0009', harness: 'Claude', finalTree: 'e71c5b04deadbeef' }
const rowsOf = (html) => [...html.matchAll(/<li class="ws-checking-row ws-checking-(\w+)" data-phase="([^"]*)">/g)].map((m) => ({ state: m[1], phase: m[2] }))

test('(a) rows rendered == phases given, in order, an unknown phase id rendered raw and never dropped', () => {
  const withUnknown = [...PHASES.slice(0, 3), { phase: 'future_phase', count: 7, detail: 'new in a later engine', elapsed_ms: 3 }, ...PHASES.slice(3)]
  const html = M.render({ session: SESSION, phases: withUnknown, status: { kind: 'ready', cached: false }, reducedMotion: false })
  assert.deepEqual(rowsOf(html).map((r) => r.phase), withUnknown.map((p) => p.phase))
  assert.match(html, /future_phase<\/span><span class="ws-checking-count">7</, 'unknown id: raw id as the label, bare number as the count')
  for (const [id, label] of Object.entries(HQ_COPY.phases.labels)) assert.ok(html.includes(label), `${id} label`)
  assert.match(html, /3 events/)
  assert.match(html, /1 completion statement</)
  assert.match(html, /0 checks cited/)
  assert.match(html, /2 trees/)
  assert.match(html, /1 checker</)
  assert.match(html, /0 findings/)
  assert.match(html, /new in a later engine/, 'detail is rendered')
  assert.match(html, /Checking what Claude said against the record/)
  assert.match(html, /Skip to verdict/)
  assert.doesNotMatch(html, /ws-checking-static/)
  assert.equal(rowsOf(html).filter((r) => r.state === 'done').length, 0, 'before the first tick nothing is ticked')
  assert.equal(rowsOf(M.render({ session: SESSION, phases: [], status: { kind: 'ready' } })).length, 0, 'no phases → no rows, none invented')
})

test('(b) the schedule: cached ≤ 600 ms, fresh ≤ 1500 ms, no per-row floor, proportional ordering preserved', () => {
  const sum = (a) => a.reduce((x, y) => x + y, 0)
  const cached = M.schedule(PHASES, { cached: true })
  const fresh = M.schedule(PHASES, { cached: false })
  assert.equal(cached.length, PHASES.length)
  assert.ok(sum(cached) <= 600 + 1e-9, `cached total ${sum(cached)}`)
  assert.ok(sum(fresh) <= 1500 + 1e-9, `fresh total ${sum(fresh)}`)
  const order = (a) => a.map((d, i) => [d, i]).sort((x, y) => y[0] - x[0]).map(([, i]) => i)
  const recordedOrder = order(PHASES.map((p) => p.elapsed_ms))
  assert.equal(order(cached)[0], recordedOrder[0], 'the slowest recorded phase is the longest row')
  assert.ok(cached[4] > cached[0] && cached[0] > cached[3], 'proportional: 900 > 40 > 12')
  assert.ok(fresh[4] > fresh[0] && fresh[0] > fresh[3])
  /* Monotone: a phase recorded slower than another never plays shorter. */
  const recorded = PHASES.map((p) => p.elapsed_ms)
  for (const d of [cached, fresh]) for (let i = 0; i < d.length; i++) for (let j = 0; j < d.length; j++) {
    if (recorded[i] > recorded[j]) assert.ok(d[i] >= d[j], `row ${i} (${recorded[i]} ms) plays shorter than row ${j} (${recorded[j]} ms)`)
  }
  /* No timer pads a row (ruled, ADJUDICATIONS item E): a 0 ms phase gets 0 ms. */
  assert.equal(cached[2], 0, 'match_proof recorded 0 ms plays 0 ms')
  assert.equal(fresh[2], 0)
  assert.equal(M.ROW_FLOOR_MS, undefined, 'the floor constant is gone, not merely unused')
  /* Small recorded times play at their real length — the sequence is not stretched to the cap. */
  const quick = M.schedule(PHASES.map((p) => ({ ...p, elapsed_ms: 1 })), { cached: true })
  assert.deepEqual(quick, PHASES.map(() => 1))
  assert.deepEqual(M.schedule(PHASES, { cached: false }), PHASES.map((p) => p.elapsed_ms), 'a 959 ms run under the 1500 ms fresh cap plays at its recorded length, row for row')
  assert.deepEqual(M.schedule([], { cached: true }), [])
  /* Unknown stays unknown: a row with no measured time — the engine writes `null` —
   * has no schedule, alone or among timed rows. Never a coerced 0, which would state a measurement the log lacks. */
  for (const bad of [{ phase: 'x' }, { phase: 'x', elapsed_ms: null }, { phase: 'x', elapsed_ms: -1 }, { phase: 'x', elapsed_ms: 1.5 }, { phase: 'x', elapsed_ms: '5' }, null]) {
    assert.equal(M.schedule([bad], { cached: true }), null, `no schedule for ${JSON.stringify(bad)}`)
    assert.equal(M.schedule([...PHASES, bad], { cached: false }), null, `one untimed row among timed ones is still no schedule: ${JSON.stringify(bad)}`)
  }
  assert.equal(M.landing({ kind: 'ready' }, false, PHASES.length, false), 'immediate', 'rows with no schedule are not played')
  const untimedPlay = M.render({ session: SESSION, phases: [...PHASES.slice(0, 5), { phase: 'write_up', count: 0, detail: '0 findings', elapsed_ms: null }], status: { kind: 'ready', cached: false }, reducedMotion: false })
  assert.match(untimedPlay, /ws-checking-static/, 'the component does not play an untimed log: it lands at once')
  /* Falsifiable: a fixed per-row timer would ignore the data. */
  assert.notDeepEqual(M.schedule(PHASES, { cached: true }), M.schedule(PHASES.map((p) => ({ ...p, elapsed_ms: 100 })), { cached: true }))
  /* Many rows still fit the cap. */
  const many = M.schedule(Array.from({ length: 20 }, (_, i) => ({ phase: `p${i}`, elapsed_ms: i })), { cached: true })
  assert.ok(sum(many) <= 600 + 1e-9)
})

test('(b2) the fresh cap is a live guard: a recorded total above 1500 ms is scaled to at most 1500', () => {
  /* The live session at ADJUDICATIONS item E: run_verifiers took 11 259 ms. */
  const SLOW = [
    { phase: 'read_record', count: 5240, elapsed_ms: 310 },
    { phase: 'find_claims', count: 0, elapsed_ms: 4 },
    { phase: 'match_proof', count: 0, elapsed_ms: 0 },
    { phase: 'workspace_since', count: 1681, elapsed_ms: 620 },
    { phase: 'run_verifiers', count: 1, elapsed_ms: 11259 },
    { phase: 'write_up', count: 0, elapsed_ms: 0 },
  ]
  const sum = (a) => a.reduce((x, y) => x + y, 0)
  const total = sum(SLOW.map((p) => p.elapsed_ms))
  assert.ok(total > 1500, `fixture must exceed the fresh cap, recorded ${total}`)
  const fresh = M.schedule(SLOW, { cached: false })
  const cached = M.schedule(SLOW, { cached: true })
  assert.ok(sum(fresh) <= 1500 + 1e-9, `fresh total ${sum(fresh)}`)
  assert.ok(sum(fresh) > 1499, 'a run over the cap fills the cap exactly — scaled, not truncated')
  assert.ok(sum(cached) <= 600 + 1e-9, `cached total ${sum(cached)}`)
  assert.equal(M.FRESH_CAP_MS, 1500)
  assert.equal(M.CACHED_CAP_MS, 600)
  /* One factor for every row: proportions survive the scaling. */
  assert.ok(Math.abs(fresh[4] / fresh[0] - 11259 / 310) < 1e-6)
  assert.equal(fresh[2], 0)
})

test('(e) phase details render verifier ids by register: Plain a phrase, Technical a code chip, the log untouched', () => {
  const rows = [{ phase: 'run_verifiers', count: 2, detail: 'unclaimed_completion@1 · tests_pass@1', elapsed_ms: 3 }, { phase: 'future', count: 1, detail: 'some_new_checker@7 ran', elapsed_ms: 1 }]
  const plain = M.render({ session: SESSION, phases: rows, status: { kind: 'ready', cached: true }, reducedMotion: true })
  assert.doesNotMatch(plain, /@\d/, 'Plain never shows a raw verifier id')
  assert.match(plain, new RegExp(HQ_COPY.chrome.verifiers.unclaimed_completion))
  assert.match(plain, new RegExp(HQ_COPY.chrome.verifiers.tests_pass))
  assert.match(plain, /<span>some new checker<\/span> ran/, 'an unknown id is spelled out, never dropped')
  assert.match(plain, / · /, 'the engine text around the ids is kept')
  const tech = M.render({ session: SESSION, phases: rows, status: { kind: 'ready', cached: true }, reducedMotion: true, register: 'operator' })
  assert.match(tech, /<code class="ws-checking-verifier">unclaimed_completion@1<\/code> · <code class="ws-checking-verifier">tests_pass@1<\/code>/)
  assert.match(tech, /<code class="ws-checking-verifier">some_new_checker@7<\/code> ran/)
  assert.equal(M.verifierPlain('unclaimed_completion@1'), HQ_COPY.chrome.verifiers.unclaimed_completion)
  assert.equal(M.verifierPlain('foo_bar@2'), 'foo bar')
  assert.equal(rows[0].detail, 'unclaimed_completion@1 · tests_pass@1', 'the log is not altered')
  const dflt = M.render({ session: SESSION, phases: rows, status: { kind: 'ready', cached: true }, reducedMotion: true, register: 'manager' })
  assert.equal(dflt, plain, 'no register is Plain')
})

test('(e2) find_claims names claims by id, never by text: Plain shows no raw claim id, Technical shows each in a code chip, the log untouched', () => {
  /* The phase log names claims by id and carries only counts; the panel reads claim text from `/api/claims`. */
  const ids = ['clm_7ZQ4M2K9PX3B8T1R6W5N0C4VHD', 'clm_01J9ZC8Q5T2R7W4N6B3M1K0PXA']
  const rows = [
    { phase: 'find_claims', count: 2, detail: ids.join(' · '), elapsed_ms: 1 },
    { phase: 'run_verifiers', count: 2, detail: `${ids[0]} · tests_pass@1`, elapsed_ms: 1 },
    { phase: 'future', count: 1, detail: `kept ${ids[1]} here`, elapsed_ms: 1 },
  ]
  const plain = M.render({ session: SESSION, phases: rows, status: { kind: 'ready', cached: true }, reducedMotion: true })
  assert.doesNotMatch(plain, /clm_/, 'Plain never shows a raw claim id')
  assert.match(plain, /<span class="ws-checking-count">2 completion statements<\/span><\/li>/, 'a detail of claim ids alone renders nothing in Plain; the label and count stay')
  assert.match(plain, /<span class="ws-checking-detail"><span>the tests pass<\/span><\/span>/, 'a dropped claim id takes its separator with it; the verifier phrase stays')
  assert.match(plain, /kept\s+here/, 'the engine text around an id is kept')
  const tech = M.render({ session: SESSION, phases: rows, status: { kind: 'ready', cached: true }, reducedMotion: true, register: 'operator' })
  assert.match(tech, new RegExp(`<code class="ws-checking-verifier ws-checking-claim">${ids[0]}</code> · <code class="ws-checking-verifier ws-checking-claim">${ids[1]}</code>`), 'Technical: each claim id in a code chip')
  assert.match(tech, new RegExp(`<code class="ws-checking-verifier ws-checking-claim">${ids[0]}</code> · <code class="ws-checking-verifier">tests_pass@1</code>`))
  assert.equal(rows[0].detail, ids.join(' · '), 'the log is not altered')
})

test('(c) reduced motion renders every row ticked, static, with no schedule and no elapsed counter', () => {
  const html = M.render({ session: SESSION, phases: PHASES, status: { kind: 'ready', cached: false }, reducedMotion: true })
  const rows = rowsOf(html)
  assert.equal(rows.length, PHASES.length)
  assert.ok(rows.every((r) => r.state === 'done'), JSON.stringify(rows))
  assert.match(html, /ws-checking-static/)
  assert.doesNotMatch(html, /ws-checking-spinning/)
  assert.match(html, /<span class="ws-checking-elapsed"><\/span>/, 'no elapsed counter under reduced motion')
  /* ADJUDICATIONS item D fifth ruling, finding 5: while the log loads under reduced motion no timer runs, so a
   * counter would sit at "0.0 s" beside "still loading" and read as a measurement. */
  const loading = M.render({ session: SESSION, phases: [], status: { kind: 'loading' }, reducedMotion: true })
  assert.match(loading, /<span class="ws-checking-elapsed"><\/span>/, 'no frozen elapsed counter while loading under reduced motion')
  assert.equal(M.landing({ kind: 'ready' }, true, PHASES.length), 'immediate')
  assert.equal(M.landing({ kind: 'ready' }, false, PHASES.length), 'play')
  assert.equal(M.landing({ kind: 'loading' }, false, 0), 'wait')
})

test('(c2) a row whose count is not a whole, non-negative number shows no count — never a 0 the log did not carry', () => {
  /* ADJUDICATIONS item D seventh ruling, item E addendum: live, "✓ Checking the workspace since 0 trees" for a log
   * whose workspace_since count was the string "1681", beside "Checked: 5240 events". */
  const rows = [
    { phase: 'read_record', count: 5240 },
    { phase: 'workspace_since', count: '1681' },
    { phase: 'match_proof', count: -1 },
    { phase: 'run_verifiers', count: 1.5 },
    { phase: 'find_claims' },
    { phase: 'write_up', count: null },
    { phase: 'mystery', count: 'seven' },
  ]
  const html = M.render({ session: SESSION, phases: rows, status: { kind: 'ready', cached: true }, reducedMotion: true })
  const text = html.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ')
  assert.match(text, /Reading the record 5240 events/, 'a whole count still renders')
  assert.doesNotMatch(text, /\b0 (?:trees?|checks? cited|checkers?|completion statements?|findings?)\b|-1|1\.5|1681|seven/, `a malformed count renders as nothing: ${text}`)
  assert.equal(rowsOf(html).length, rows.length, 'every row is still listed, by its label')
})

test('(d) a fetch failure renders the static list with the reason and lands the verdict anyway', () => {
  const html = M.render({ session: SESSION, phases: [], status: { kind: 'failed', reason: 'HTTP 502' }, reducedMotion: false })
  assert.match(html, /ws-checking-failed/)
  assert.match(html, /could not be loaded — HTTP 502\. The verdict below does not depend on it\./)
  assert.match(M.render({ session: SESSION, phases: [], status: { kind: 'failed' }, reducedMotion: false }), new RegExp(`could not be loaded — ${HQ_COPY.chrome.no_reason}\\.`), 'a failure with no reason says so in copy')
  assert.match(M.render({ session: { id: 's1' }, phases: [], status: { kind: 'failed', reason: 'x' } }), new RegExp(`Checking what ${HQ_COPY.chrome.agent_fallback_lower} said`), 'no harness → the copy fallback')
  assert.match(html, /ws-checking-static/)
  assert.equal(rowsOf(html).length, 0, 'no rows invented for a log that never arrived')
  assert.equal(M.landing({ kind: 'failed', reason: 'HTTP 502' }, false, 0), 'immediate', 'the effect calls onDone at once for this mode')
  /* A stale-but-present log under a failed refresh still renders its real rows, ticked. */
  const stale = M.render({ session: SESSION, phases: PHASES, status: { kind: 'failed', reason: 'network' }, reducedMotion: false })
  assert.ok(rowsOf(stale).every((r) => r.state === 'done'))
})

test('while the fetch is in flight: title, all rows pending, elapsed counter, skip link', () => {
  const html = M.render({ session: SESSION, phases: [], status: { kind: 'loading' }, reducedMotion: false })
  assert.match(html, /Checking what Claude said against the record/)
  assert.match(html, /ws-checking-spinning/)
  assert.match(html, /<span class="ws-checking-elapsed">0\.0 s<\/span>/)
  assert.match(html, /Skip to verdict/)
  assert.equal(M.landing({ kind: 'loading' }, false, 0), 'wait')
})

test('the browser cache keys on session and final tree', () => {
  const store = new Map()
  const storage = { getItem: (k) => (store.has(k) ? store.get(k) : null), setItem: (k, v) => store.set(k, v) }
  const doc = { contract: 'thruline.phases.v1', session: 's1', phases: PHASES }
  M.writePhaseCache('s1', 'T1', doc, storage)
  assert.equal(M.phasesCacheKey('s1'), 'thruline-phases:s1')
  assert.deepEqual(M.readPhaseCache('s1', 'T1', storage).phases, PHASES)
  assert.equal(M.readPhaseCache('s1', 'T2', storage), null, 'a moved tree misses the cache')
  assert.equal(M.readPhaseCache('s2', 'T1', storage), null)
  store.set('thruline-phases:s3', '{not json')
  assert.equal(M.readPhaseCache('s3', 'T1', storage), null, 'a corrupt entry is a miss, not a throw')
})
