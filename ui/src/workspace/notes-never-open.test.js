import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* A3-NOTES-NEVER-OPEN — informational items are "notes", are never "open",
 * and never enter the attention count (report `#ui-counts`, F2's notes
 * clause; done when, verbatim: "A property test: every surface's count is
 * equal").
 *
 * Predeclared in the card:
 *   1. every surface that states a findings count states `fail` + `warn`
 *      findings only; a count that includes `info` findings is labelled with
 *      the notes noun from the copy file — asserted on the RENDERED string;
 *   2. no rendered string pairs a notes count with "open";
 *   3. a session whose only findings are `info` produces 0 attention items
 *      and does not put its Change in the Inbox;
 *   4. `Findings · N` no longer exists as a literal; the heading comes from
 *      `hq-copy.js`.
 *
 * The surfaces: the session Inspector (`docks.jsx`), the claim document
 * (`canvas.jsx`, which carries the same kicker), the verdict cards the
 * Change layer counts (`verdict-model.js`), and the two renderings of the
 * engine's single `write_up` count — the Read (`read-rail.jsx`) and the
 * Verdict tab's checking sequence (`checking-sequence.jsx`). The engine
 * serves ONE number for `write_up`, every severity together; the browser
 * does not split it (NEEDS ENGINE), it names it truthfully.
 *
 * Every fixture is synthetic.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-notes-'))
after(() => rm(temporary, { recursive: true, force: true }))

const out = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  stdin: {
    contents: `
      import React from 'react';
      import { renderToStaticMarkup } from 'react-dom/server';
      import { Workspace } from './shell.jsx';
      import { Inspector } from './docks.jsx';
      import { ClaimDocument } from './canvas.jsx';
      import { CheckingSequence } from './checking-sequence.jsx';
      import { ReadRailBody } from './read-rail.jsx';
      import { readRailModel, stepCount, READ_CONTRACT } from './read-model.js';
      import { buildVerdict } from './verdict-model.js';
      import { attentionOf } from './attention.js';
      import { assembleInbox, InboxScreen } from './inbox.jsx';
      import { HQ_COPY } from './hq-copy.js';
      import { aStore, aSession, rng } from './attention-count.fixture.js';
      export { Workspace, readRailModel, stepCount, READ_CONTRACT, buildVerdict, attentionOf, assembleInbox, HQ_COPY, aStore, aSession, rng };
      const noop = () => {};
      export const renderInspector = (insp) => renderToStaticMarkup(React.createElement(Inspector, { width: 320, position: 'relative', shadow: 'none', insp, tab: 'details', registerLabel: '', cmdOpen: false, commands: [], copyNextLabel: '', reading: false, onTab: noop, onFinding: noop, on: {} }));
      export const renderClaimDoc = (insp) => renderToStaticMarkup(React.createElement(ClaimDocument, { insp, registerLabel: '', onFinding: noop }));
      export const renderChecking = (phases) => renderToStaticMarkup(React.createElement(CheckingSequence, { session: { id: 's1', label: 's1', harness: 'the agent', finalTree: 'e71c5b04deadbeef' }, phases, status: { kind: 'ready', cached: false }, reducedMotion: true }));
      export const renderRead = (doc) => renderToStaticMarkup(React.createElement(ReadRailBody, { m: readRailModel({ status: 'ready', doc }, { reducedMotion: true }), reduced: true }));
      export const renderInbox = (state) => renderToStaticMarkup(React.createElement(InboxScreen, { state }));
    `,
    resolveDir: here,
  },
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
})
const file = join(temporary, 'notes.mjs')
await writeFile(file, out.outputFiles[0].text)
const M = await import(pathToFileURL(file).href)
const { HQ_COPY } = M
const I = () => HQ_COPY.trace.finding_counts

const text = (html) => html.replace(/<[^>]*>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ').trim()
const NOTES_OPEN = /\bnotes?\b[^.]*\bopen\b|\bopen\b[^.]*\bnotes?\b/i
const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
const phrase = (one, many, n) => (n === 1 ? one : many.replace('{n}', String(n)))

/* ── a session Inspector over a generated claim ────────────────────────── */

function workspaceWith(findings) {
  const session = {
    id: 'session', key: 'session', capture: {}, narr: {}, ended: '2026-09-08T22:59:00Z', finalTree: 'tree-final',
    events: [{ seq: 10, kind: 'session_start', text: 'Started' }],
    claims: [{ id: 'claim-one', node: 'clm', text: 'A synthetic claim', kind: 'tests_pass', word: 'proven', verdict: 'supported', evidence: [], findings }],
    checks: [], promises: [], asks: [],
  }
  session.nodes = [{ id: 'clm', objId: 'claim-one', objType: 'claim', title: 'A synthetic claim' }]
  const w = Object.create(M.Workspace.prototype)
  w.props = { commentary: false }
  w.state = { session: 'session', sel: null, rep: 'review', doc: 'trace', drawer: 'claims', panels: { right: false, bottom: true }, expanded: {}, docs: [], compare: { a: 'session', b: 'session' }, corpus: { order: ['session'], sessions: { session } } }
  w.S = () => session
  w.reg = () => 'mgr'
  w.nodeById = (id) => session.nodes.find((node) => node.id === id)
  w.dbPath = () => '/record.sqlite'
  w.stopReplay = () => {}
  w.centerOn = () => {}
  w.setState = (update, then) => { w.state = { ...w.state, ...(typeof update === 'function' ? update(w.state) : update) }; if (then) then() }
  return w
}

const KEYS = ['assertion_removed', 'skip_marker_added', 'coverage_not_measured', 'test_added', 'reexecution_confirms']
const SEVERITIES = ['fail', 'warn', 'info']

test('NOTES · property: the Inspector and the claim document state fail+warn as findings and info as notes, from the copy file, over 300 generated claims', () => {
  const r = M.rng(77)
  let infos = 0
  for (let k = 0; k < 300; k += 1) {
    const n = Math.floor(r() * 6)
    const findings = Array.from({ length: n }, (_, j) => {
      const key = KEYS[Math.floor(r() * KEYS.length)]
      return { key, severity: SEVERITIES[Math.floor(r() * 3)], name: key, node: `f_${k}_${j}`, detail: 'synthetic', path: 'src/a.js' }
    })
    const want = { findings: findings.filter((f) => f.severity !== 'info').length, notes: findings.filter((f) => f.severity === 'info').length }
    infos += want.notes
    const insp = workspaceWith(findings).inspector('clm')
    assert.deepEqual(insp.findingsCount, want, `claim ${k}: the typed count`)
    for (const [name, html] of [['Inspector', M.renderInspector(insp)], ['claim document', M.renderClaimDoc(insp)]]) {
      const t = text(html)
      const f = phrase(I().findings_one, I().findings_many, want.findings)
      const nn = phrase(I().notes_one, I().notes_many, want.notes)
      if (want.findings) assert.ok(t.includes(f), `claim ${k}, ${name}: does not state "${f}": ${t.slice(0, 300)}`)
      else assert.doesNotMatch(t, new RegExp('\\b\\d+ findings?\\b'), `claim ${k}, ${name}: states a findings count with no fail or warn finding`)
      if (want.notes) assert.ok(t.includes(nn), `claim ${k}, ${name}: does not state "${nn}"`)
      else assert.doesNotMatch(t, /\b\d+ notes?\b/, `claim ${k}, ${name}: states a notes count with no info finding`)
      /* The only findings count on the surface is the fail+warn one. */
      for (const m of t.matchAll(/\b(\d+) findings?\b/g)) assert.equal(Number(m[1]), want.findings, `claim ${k}, ${name}: a findings count that is not fail+warn: ${m[0]}`)
      for (const m of t.matchAll(/\b(\d+) notes?\b/g)) assert.equal(Number(m[1]), want.notes, `claim ${k}, ${name}: a notes count that is not the info count: ${m[0]}`)
      assert.doesNotMatch(t, NOTES_OPEN, `claim ${k}, ${name}: pairs notes with "open"`)
    }
  }
  assert.ok(infos >= 100, `only ${infos} info findings were generated`)
})

test('NOTES · an info-only claim shows "1 note", never "Findings · 1"', () => {
  const insp = workspaceWith([{ key: 'coverage_not_measured', severity: 'info', name: 'Coverage not measured', node: 'f_c', detail: '' }]).inspector('clm')
  assert.deepEqual(insp.findingsCount, { findings: 0, notes: 1 })
  for (const html of [M.renderInspector(insp), M.renderClaimDoc(insp)]) {
    const t = text(html)
    assert.ok(t.includes(I().notes_one), t)
    assert.doesNotMatch(t, /Findings\s*·\s*1|\b1 finding\b/i)
  }
})

/* ── the Change layer: info never becomes an attention item ────────────── */

test('NOTES · a Change whose only linked work carries info-severity findings has 0 items and is not in the Inbox (60 generated stores)', () => {
  let changes = 0
  for (let seed = 9000; seed < 9060; seed += 1) {
    const store = M.aStore(seed, { count: 4, force: { onlyInfo: true, criteria: 1 } })
    for (const row of store.all) {
      const d = store.docsById.get(row.id)
      const a = M.attentionOf({ change: d.change, proposals: d.proposals, work: d.work, applicability: [], hq: null })
      const fromWork = a.reasons.filter((x) => ['observed_failure', 'applicable_finding'].includes(x.id))
      assert.deepEqual(fromWork, [], `seed ${seed} ${row.id}: info findings became a reason: ${JSON.stringify(fromWork)}`)
      changes += 1
    }
    const state = { ...M.assembleInbox(store.all, store.docsById), storage: { known: false }, notes: [] }
    assert.equal(state.items.length, 0, `seed ${seed}: ${state.items.length} Changes with info-only work are in the Inbox: ${state.items.map((i) => i.reasonText).join(' | ')}`)
    assert.doesNotMatch(text(M.renderInbox(state)), /waiting on evidence/)
  }
  assert.ok(changes >= 200)
})

test('NOTES · the verdict cards the Change layer counts carry no info-severity finding, and an unknown card state is a note, never open', () => {
  /* A lowered skip marker (the run reported nothing skipped) is info; it must not become a "Tests weakened" card. */
  const s = { id: 's', key: 's', capture: { degraded: false }, claims: [{ id: 'c', node: 'clm', text: 'A claim', kind: 'tests_pass', verdict: 'supported', evidence: [], findings: [{ key: 'skip_marker_added', severity: 'info', node: 'f', detail: 'nothing unrun' }] }], promises: [], checks: [], events: [], nodes: [] }
  assert.deepEqual(M.buildVerdict(s).findings, [], 'an info finding became a verdict card')
  for (const severity of ['warn', 'fail']) {
    const v = M.buildVerdict({ ...s, claims: [{ ...s.claims[0], findings: [{ ...s.claims[0].findings[0], severity }] }] })
    assert.equal(v.findings.length, 1, `a ${severity} skip marker is still a card`)
  }
  const change = { id: 'cccccccc-0000-4000-8000-00000000c009', accepted_scope: null, associations: [{ session_id: 's', provenance: 'confirmed' }] }
  const a = M.attentionOf({ change, work: [{ id: 's', findings: [{ state: 'a_state_added_later', kind: 'claim' }] }] })
  assert.equal(a.notes, 1, 'an unknown card state is counted as a note')
  assert.deepEqual(a.reasons.map((x) => x.id), ['awaiting_definition'], 'an unknown card state became an attention reason')
})

/* ── the engine's single write_up count, named truthfully ──────────────── */

test('NOTES · the Read and the checking sequence name the engine’s write_up count as findings and notes, and never split it', () => {
  for (const counts of [HQ_COPY.read.counts, HQ_COPY.phases.counts]) {
    assert.match(counts.write_up, /findings and notes/)
    assert.match(counts.write_up_one, /finding or note/)
  }
  assert.equal(M.stepCount('write_up', 9), HQ_COPY.read.counts.write_up.replace('{n}', '9'))
  const phases = [{ phase: 'write_up', status: 'done', count: 9, detail: '9 findings', elapsed_ms: 2 }]
  const read = text(M.renderRead({ contract: M.READ_CONTRACT, cached: false, phases }))
  assert.ok(read.includes(HQ_COPY.read.counts.write_up.replace('{n}', '9')), read)
  assert.doesNotMatch(read, /\b9 findings\b(?! and notes)/, 'the Read states the combined count as findings')
  const seq = text(M.renderChecking([{ phase: 'write_up', count: 9, detail: '9 findings', elapsed_ms: 2 }]))
  assert.ok(seq.includes(HQ_COPY.phases.counts.write_up.replace('{n}', '9')), seq)
  assert.doesNotMatch(seq, /\b9 findings\b(?! and notes)/, 'the checking sequence states the combined count as findings (the engine detail restates it)')
})

/* ── the words ─────────────────────────────────────────────────────────── */

test('NOTES · no copy string pairs a notes count with "open", and the findings heading is not a literal', () => {
  const walk = (o, at) => Object.entries(o).forEach(([k, v]) => {
    if (v && typeof v === 'object') walk(v, `${at}.${k}`)
    else if (typeof v === 'string' && /\{n\}|\d/.test(v)) assert.doesNotMatch(v, NOTES_OPEN, `${at}.${k}: ${v}`)
  })
  walk(HQ_COPY, 'HQ_COPY')
  for (const f of ['docks.jsx', 'canvas.jsx']) {
    const src = readFileSync(join(here, f), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:\\])\/\/.*$/gm, '$1')
    assert.doesNotMatch(src, /Findings · /, `${f} still carries the "Findings · " literal`)
  }
  assert.match(I().notes_many, /notes/)
  assert.doesNotMatch(JSON.stringify(I()), /\bopen\b/i)
})
