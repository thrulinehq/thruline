import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* P6-2 — the Read (design record §3.2; 03-USER-LENS.md rule 10), rendered.
 *
 * The served answers below are the shape `crates/cli/src/uiserve/read_route.rs`
 * answers with; `crates/cli/tests/hq_read.rs` proves against the shipping
 * binary that those rows ARE the pipeline's own phase log, that a cached
 * answer is one comparison step, and that a malformed rule table fails
 * `find_claims` for real. Here every assertion is about what the RAIL SAYS
 * for each of those answers. A probe bundle swaps copy strings, so a literal
 * in the component cannot pass. */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-read-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ReadRailBody } from './read-rail.jsx';
  import { readRailModel, stepLabel, stepTime, stepCount, READ_CONTRACT } from './read-model.js';
  import { READ } from './data.js';
  import { HQ_COPY } from './hq-copy.js';
  export { readRailModel, stepLabel, stepTime, stepCount, READ_CONTRACT, READ, HQ_COPY };
  export const render = (read, opts = {}) => renderToStaticMarkup(React.createElement(ReadRailBody, { m: readRailModel(read, opts), reduced: !!opts.reducedMotion }));
`
const COMMON = {
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
}
async function load(name, plugins) {
  const result = await build({ ...COMMON, stdin: { contents: ENTRY, resolveDir: here }, plugins })
  const file = join(temporary, name)
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}
const M = await load('read.mjs')
const COPY_FILE = join(here, 'hq-copy.js')
const PROBED = await load('read-probe.mjs', [{
  name: 'probe-copy',
  setup(b) {
    b.onLoad({ filter: /hq-copy\.js$/ }, () => ({
      contents: readFileSync(COPY_FILE, 'utf8')
        .replace("title: 'Opening this work',", "title: 'probe-title',")
        .replace("skip: 'Skip',", "skip: 'probe-skip',")
        .replace("cached: 'Read earlier at {when}, nothing changed since.',", "cached: 'probe-cached {when}',")
        .replace("not_run: 'Did not run.',", "not_run: 'probe-not-run',")
        .replace("run_verifiers: 'Applying the rules to the record',", "run_verifiers: 'probe-rules',"),
      loader: 'js',
    }))
  },
}])

const { render, readRailModel, stepLabel, stepTime, HQ_COPY, READ } = M
const R = HQ_COPY.read

const text = (html) => html.replace(/<[^>]+>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ').trim()
const plainOf = (html) => text(html.replace(/<details class="ws-read-technical">[\s\S]*?<\/details>/g, ''))
const rowsOf = (html) => [...html.matchAll(/<li class="ws-read-row[^"]*" data-phase="([^"]*)" data-status="([^"]*)">([\s\S]*?)<\/li>/g)].map((m) => ({ phase: m[1], status: m[2], body: m[3] }))

/* What `claims --json --phases` printed for the walk's session, as the route serves it. */
const PIPELINE = ['read_record', 'find_claims', 'match_proof', 'workspace_since', 'run_verifiers', 'write_up']
const FRESH = {
  contract: 'thruline.read.v1', session: 'cccccccc-0000-4000-8000-000000000062', cached: false, complete: true, reason: null,
  read_at: '2026-09-24T09:15:02Z',
  phases: [
    { phase: 'read_record', status: 'done', count: 3, detail: 'session cccccccc-0000-4000-8000-000000000062 · 3 events', elapsed_ms: 6 },
    { phase: 'find_claims', status: 'done', count: 1, detail: 'clm_1KP3XCE7XMT3SQYQNTF4RJ17NJ', elapsed_ms: 0 },
    { phase: 'match_proof', status: 'done', count: 0, detail: 'no check cited', elapsed_ms: 7 },
    { phase: 'workspace_since', status: 'done', count: 0, detail: '0 edits · 0 trees', elapsed_ms: 0 },
    { phase: 'run_verifiers', status: 'done', count: 1, detail: 'tests_pass@1', elapsed_ms: 11259 },
    { phase: 'write_up', status: 'done', count: 0, detail: '0 findings', elapsed_ms: 2 },
  ],
  compared: { outcome: 'first', elapsed_ms: 3, fingerprint: 'a'.repeat(64) },
}
const CACHED = {
  contract: 'thruline.read.v1', session: FRESH.session, cached: true, complete: true, reason: null, read_at: FRESH.read_at,
  phases: [{ phase: 'compare_record', status: 'done', count: 3, detail: '', elapsed_ms: 2 }],
  compared: { outcome: 'unchanged', elapsed_ms: 2, fingerprint: 'a'.repeat(64) },
}
const REASON = 'loading the claim pattern table: /home/sam/shop/.thruline/claims.yml:1: expected `version:` or `patterns:`'
const FAILED = {
  contract: 'thruline.read.v1', session: FRESH.session, cached: false, complete: false, reason: REASON, read_at: FRESH.read_at,
  phases: [
    { phase: 'read_record', status: 'done', count: 3, detail: 'session x · 3 events', elapsed_ms: 6 },
    { phase: 'find_claims', status: 'failed', count: null, detail: null, elapsed_ms: null, reason: REASON },
    { phase: 'match_proof', status: 'not_run', count: null, detail: null, elapsed_ms: null },
    { phase: 'workspace_since', status: 'not_run', count: null, detail: null, elapsed_ms: null },
    { phase: 'run_verifiers', status: 'withheld', count: null, detail: null, elapsed_ms: null, withheld_by: 'find_claims' },
    { phase: 'write_up', status: 'withheld', count: null, detail: null, elapsed_ms: null, withheld_by: 'run_verifiers' },
  ],
  compared: { outcome: 'changed', elapsed_ms: 3, fingerprint: 'b'.repeat(64) },
}
const UNMEASURED = {
  ...FRESH,
  phases: FRESH.phases.map((p) => (p.phase === 'find_claims' ? { ...p, status: 'unmeasured', elapsed_ms: null }
    : p.phase === 'run_verifiers' || p.phase === 'write_up' ? { phase: p.phase, status: 'withheld', count: null, detail: null, elapsed_ms: null, withheld_by: p.phase === 'run_verifiers' ? 'find_claims' : 'run_verifiers' } : p)),
}
const ready = (doc) => ({ status: 'ready', doc })
const LOADING = { status: 'loading' }
const REFUSED = { status: 'refused', refusal: '/api/read?session=x answered HTTP 404' }
const ALL = { loading: [LOADING], fresh: [ready(FRESH)], cached: [ready(CACHED)], failed: [ready(FAILED)], unmeasured: [ready(UNMEASURED)], refused: [REFUSED] }

test('(p62) the rendered steps are EXACTLY the served steps, in order — nothing added, nothing dropped', () => {
  for (const doc of [FRESH, CACHED, FAILED, UNMEASURED]) {
    const rows = rowsOf(render(ready(doc)))
    assert.deepEqual(rows.map((r) => [r.phase, r.status]), doc.phases.map((p) => [p.phase, p.status]), doc.cached ? 'cached' : doc.complete ? 'fresh' : 'failed')
  }
  /* A step this build has no name for is still shown, under its own id. */
  const odd = { ...FRESH, phases: [...FRESH.phases, { phase: 'future_phase', status: 'done', count: 4, detail: '', elapsed_ms: 1 }] }
  const rows = rowsOf(render(ready(odd)))
  assert.equal(rows.length, 7)
  assert.match(rows[6].body, /future_phase/)
  assert.match(rows[6].body, />4</, 'an unnamed count renders its bare number')
})

test('(p62) while the answer is on its way no step is shown, and the rail says live steps need engine support', () => {
  const html = render(LOADING)
  assert.deepEqual(rowsOf(html), [], 'a row before the engine reported it would be an invented step')
  const t = text(html)
  assert.ok(t.includes(R.loading), t)
  assert.ok(t.includes('needs engine support'), t)
  for (const id of PIPELINE) assert.ok(!t.includes(stepLabel(id)), `"${stepLabel(id)}" shown before the engine reported it`)
})

test('(p62) each time and count is the served one, as measured — nothing scaled, padded or rounded to a stand-in', () => {
  const rows = rowsOf(render(ready(FRESH)))
  const times = rows.map((r) => (r.body.match(/<span class="ws-read-time">([^<]*)<\/span>/) || [])[1])
  assert.deepEqual(times, ['6 ms', '0 ms', '7 ms', '0 ms', '11.3 s', '2 ms'])
  const counts = rows.map((r) => (r.body.match(/<span class="ws-read-count">([^<]*)<\/span>/) || [])[1])
  /* A3-NOTES-NEVER-OPEN: the engine's write_up number counts every severity, so it is named "findings and notes". */
  assert.deepEqual(counts, ['3 recorded steps', '1 completion statement', '0 checks found', '0 snapshots of the files', '1 result', '0 findings and notes'])
  assert.equal(stepTime(null), '', 'an unmeasured time is no time, never 0')
  assert.equal(stepTime(1.5), '', 'a time that is not a whole number of milliseconds is not shown')
})

test('(p62) a cached read says "read earlier, nothing changed since" and shows only the comparison it did', () => {
  const html = render(ready(CACHED))
  const t = plainOf(html)
  assert.ok(/^Opening this work Skip Read earlier at \d\d:\d\d, nothing changed since\./.test(t), t)
  const rows = rowsOf(html)
  assert.deepEqual(rows.map((r) => r.phase), ['compare_record'])
  assert.match(rows[0].body, /Comparing the record with the earlier read/)
  assert.match(rows[0].body, /3 recorded steps compared/)
  assert.match(rows[0].body, /2 ms/)
  for (const id of PIPELINE) assert.ok(!t.includes(stepLabel(id)), `a cached read replays "${stepLabel(id)}"`)
})

test('(p62) a failed step says so; what needs it is withheld; a step that did not run shows no result', () => {
  const html = render(ready(FAILED))
  const rows = rowsOf(html)
  const by = Object.fromEntries(rows.map((r) => [r.phase, r]))
  assert.match(by.find_claims.body, /Could not finish\./)
  assert.match(by.run_verifiers.body, /Withheld: it needs “Finding what the agent said”, which did not finish\./)
  assert.match(by.write_up.body, /Withheld: it needs “Applying the rules to the record”, which did not finish\./)
  assert.match(by.match_proof.body, /Did not run\./)
  for (const id of ['find_claims', 'match_proof', 'workspace_since', 'run_verifiers', 'write_up']) {
    assert.doesNotMatch(by[id].body, /ws-read-count|ws-read-time/, `${id} shows a result it does not have`)
  }
  assert.match(by.read_record.body, /3 recorded steps/)
  const t = plainOf(html)
  assert.ok(t.includes(R.incomplete), t)
  assert.ok(!t.includes('claims.yml'), 'the engine\'s error (a path) is on the Technical face, not the plain one')
  assert.ok(text(html).includes('claims.yml'), 'the engine\'s error is one level down, never hidden')
  /* An unmeasured step keeps its count, says its time is unknown, and withholds what needs it. */
  const u = Object.fromEntries(rowsOf(render(ready(UNMEASURED))).map((r) => [r.phase, r]))
  assert.match(u.find_claims.body, /1 completion statement/)
  assert.match(u.find_claims.body, /how long it took could not be measured/)
  assert.doesNotMatch(u.find_claims.body, /ws-read-time/)
  assert.match(u.run_verifiers.body, /whose time could not be measured/)
  assert.doesNotMatch(u.run_verifiers.body, /ws-read-count/)
})

test('(p62) Skip is visible in every state, in both motion settings', () => {
  for (const [name, [read]] of Object.entries(ALL)) {
    for (const reducedMotion of [false, true]) {
      const html = render(read, { reducedMotion })
      assert.match(html, /<button type="button" class="ws-text-button ws-read-skip"[^>]*>Skip<\/button>/, `${name} reduced=${reducedMotion}`)
      assert.doesNotMatch(html, /ws-read-skip"[^>]*(disabled|hidden)/, `${name}: Skip is there but off`)
    }
  }
})

test('(p62) reduced motion: the finished list, no spinner and no counter; with motion, both only while waiting', () => {
  const waiting = render(LOADING, { reducedMotion: false, waitedMs: 1234 })
  assert.match(waiting, /ws-read-spinner/)
  assert.ok(text(waiting).includes('Waiting 1.2 s so far.'), 'the counter is the wall time it was given, no more')
  const still = render(LOADING, { reducedMotion: true, waitedMs: 1234 })
  assert.doesNotMatch(still, /ws-read-spinner|ws-read-waiting/)
  for (const doc of [FRESH, CACHED, FAILED]) {
    for (const reducedMotion of [false, true]) {
      const html = render(ready(doc), { reducedMotion, waitedMs: 999 })
      assert.doesNotMatch(html, /ws-read-spinner|ws-read-waiting/, 'nothing moves once the answer is here')
      assert.equal(rowsOf(html).length, doc.phases.length, 'the finished list, whole')
    }
  }
})

test('(p62) Phase 5 is "Applying the rules to the record", and "running" is said nowhere on the rail', () => {
  assert.equal(PIPELINE[4], 'run_verifiers')
  assert.equal(stepLabel('run_verifiers'), 'Applying the rules to the record')
  assert.equal(HQ_COPY.phases.labels.run_verifiers, 'Applying the rules to the record', 'one name per phase: the Verdict tab says the same')
  const words = [...Object.values(HQ_COPY.phases.labels), ...JSON.stringify(R).split('"')]
  for (const w of words) assert.doesNotMatch(w, /\brunning\b/i, w)
  for (const [name, [read]] of Object.entries(ALL)) assert.doesNotMatch(text(render(read)), /\brunning\b/i, name)
})

test('(p62) the rail\'s plain face uses Sam\'s words: no engine noun above Technical', () => {
  const banned = [/\bevents?\b/i, /\breceipts?\b/i, /\bledger\b/i, /\bsession id\b/i, /\bseq\b/i, /\bclaim id\b/i, /\btree hash\b/i, /\bpredicates?\b/i, /\bclm_/, /@\d/, /fingerprint/i, /\bphase\b/i]
  for (const [name, [read]] of Object.entries(ALL)) {
    const t = plainOf(render(read))
    for (const re of banned) assert.doesNotMatch(t, re, `${name}: ${t}`)
  }
})

test('(p62) every word on the rail comes from the copy file', () => {
  const html = PROBED.render(ready(FAILED))
  const t = text(html)
  for (const probe of ['probe-title', 'probe-skip', 'probe-not-run', 'probe-rules']) assert.ok(t.includes(probe), `${probe} missing: ${t}`)
  assert.ok(text(PROBED.render(ready(CACHED))).includes('probe-cached'), 'the cached sentence is not the copy file\'s')
})

test('(p62) the rail reaches one address, a GET, through the read layer, and paces nothing', () => {
  assert.equal(READ('a b'), '/api/read?session=a%20b')
  const src = readFileSync(join(here, 'read-rail.jsx'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '')
  assert.doesNotMatch(src, /fetch\(/, 'the rail fetches through data.js, the one place the workspace fetches')
  assert.match(src, /readProjection\(READ\(session\)\)/)
  assert.doesNotMatch(src, /POST|writeAction/)
  assert.doesNotMatch(src, /setTimeout/, 'no step is revealed on a timer')
  assert.equal((src.match(/setInterval\(/g) || []).length, 1, 'the one timer is the wall-clock counter')
  assert.match(src, /if \(!loading \|\| reduced\) return undefined/, 'and it runs only while waiting, never under reduced motion')
  const model = readFileSync(join(here, 'read-model.js'), 'utf8')
  assert.doesNotMatch(model, /setTimeout|setInterval|schedule\(/)
})
