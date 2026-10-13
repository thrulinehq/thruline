import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* UNRECORDED-WORK-NOT-SILENT-R2 (launch card, round 2).
 *
 * Round 1 decided "unrecorded" from `steps`, the explain reading's count of
 * events. The round-1 adversary broke it: a linked conversation can hold
 * recorded events — the agent's own words among them — and still no captured
 * claim, because claims are extracted only from a recorded `stop` event
 * (crates/cli/src/claims_cmd.rs:209, explain_cmd.rs:123-147). A conversation
 * recorded or imported without one comes back from /api/claims as
 * `claims: []`, and "The record holds nothing the agent said about this
 * Change." was printed over it.
 *
 * This file drives the REAL loader (`readChangeDocuments`, with the routes it
 * reads answered by a stub `fetch`) and then every surface, for each shape:
 *  1. 0 steps;
 *  2. steps without a recorded end (the adversary's repro, byte for byte, and
 *     the shape the server actually serves for it);
 *  3. a recorded end with no claim — the one shape `said_none` may render in;
 *  4. a recorded claim — neither sentence, and the claim shows;
 *  5. claims unknown (the claims read refused).
 * Each is asserted on the Change screen (Verify and the top line), the
 * whole-story page, and the exported file, from the one field `hq.unrecorded`.
 *
 * Every fixture is synthetic. No string here came off a real machine.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-unrecorded-capture-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeShell, ChangeScreen, readChangeDocuments } from './change-workspace.jsx';
  import { PacketPage } from './packet.jsx';
  import { buildPacketView } from './packet-view.js';
  import { buildChangeView } from './change-view.js';
  import * as Model from './change-model.js';
  import { HQ_COPY } from './hq-copy.js';
  import { fill } from './hq-narrate.js';
  import { renderExport } from './packet-export.js';
  export { buildPacketView, buildChangeView, Model, HQ_COPY, fill, renderExport, readChangeDocuments };
  export const renderShell = props => renderToStaticMarkup(React.createElement(ChangeShell, props));
  export const renderScreen = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderPage = props => renderToStaticMarkup(React.createElement(PacketPage, props));
`

const bundled = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  stdin: { contents: ENTRY, resolveDir: here },
})
const file = join(temporary, 'unrecorded-capture.mjs')
await writeFile(file, bundled.outputFiles[0].text)
const U = await import(pathToFileURL(file).href)
const { buildPacketView, buildChangeView, Model, HQ_COPY, renderExport, readChangeDocuments } = U

const MD = HQ_COPY.change.packet.markdown
const V = HQ_COPY.change.verify
const T = HQ_COPY.change.unrecorded
const SAID_NONE = 'The record holds nothing the agent said about this Change.'
const AGENT_WORDS = 'Done. The cart total is added and the tests pass.'

const CHANGE_ID = 'cccccccc-0000-4000-8000-00000000c0r2'
const SESSION = 'cccccccc-0000-4000-8000-00000000r001'

const change = {
  id: CHANGE_ID,
  project_id: 'cccccccc-0000-4000-8000-00000000p0r2',
  opened_at: '2026-01-02T09:00:00Z',
  request: { title: 'Add the cart total', purpose: 'The cart shows no total.', requester: 'sam', source: 'person' },
  accepted_scope: {
    version: 1, criteria: ['The cart shows a total.'], exclusions: [], allowed_scope: [], constraints: [], check_profile: [],
    content_sha256: '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
    acceptance: { actor: 'sam', accepted_at: '2026-01-02T09:06:00Z', rationale: 'That is what I asked for.' },
    proposed_at: '2026-01-02T09:05:00Z',
  },
  implementation: {
    version: 1, bound_at: '2026-01-02T10:00:00Z', repository_identity: 'repo-synthetic-0001',
    base_revision: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0', snapshot_id: null, workspace_identity: 'workspace-synthetic-0001',
  },
  associations: [{ id: 'a1', session_id: SESSION, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-01-02T10:01:00Z', reason: null }],
  examination: 'none_yet', decision: 'none_yet',
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
}
const coverage = { scope_version: 1, link_rows: 0, execution_sessions: [], criteria: [
  { index: 1, text: 'The cart shows a total.', state: 'none', items: [] },
], candidates: [] }

const explainOf = (whatHappened, why) => ({
  shape: 'thruline.explain', schema_version: 1, session: SESSION,
  what_happened: whatHappened,
  ...(why ? { why_it_stopped: { kind: why, summary: 'synthetic', last_events: [] } } : {}),
  claims_vs_proof: [], promises: [], risk_flags: [],
})
const claimsOf = (claims, degraded) => ({
  shape: 'thruline.claims', schema_version: 1, session: SESSION, final_tree_hash: null, checks: 0,
  claims, promises: [], negated: [],
  ...(degraded === undefined ? {} : { capture_degraded: degraded }),
})
const A_CLAIM = {
  id: 'clm_r2', kind: 'tests_pass', text: 'The tests pass.', span: { event_id: 'evt_stop', offset: [0, 15] },
  scope: null, extractor: 'table', verdict: 'unsupported', verifier: 'tests_pass@1',
  evidence: [], reasons: ['no test run is recorded'], warnings: [], binding: null, findings: [],
}
const PROMPT = { seq: 1, kind: 'user_prompt', text: 'Add the cart total.' }
const SAID = { seq: 2, kind: 'assistant_text', text: AGENT_WORDS }
const STOP = { seq: 3, kind: 'stop', text: 'stopped: ' + AGENT_WORDS }

/* The shapes. `claims: null` means the claims read is refused (HTTP 500). */
const SHAPES = {
  zero_steps: { explain: explainOf([], 'no_stop_recorded'), claims: claimsOf([], false) },
  /* The adversary's repro (repro-adv.mjs), exactly: no why_it_stopped, no
   * capture_degraded field, an empty claims list. */
  no_end_adversary: { explain: explainOf([PROMPT, SAID]), claims: { shape: 'thruline.claims', schema_version: 1, session: SESSION, claims: [] } },
  /* What the server actually serves for the same conversation: explain says
   * no stop was recorded (crates/explain/src/stop.rs NoStopRecorded), claims
   * says capture is degraded (claims_cmd.rs lifecycle_gap). */
  no_end_served: { explain: explainOf([PROMPT, SAID], 'no_stop_recorded'), claims: claimsOf([], true) },
  /* A stop was recorded, and the work carried on after it: the record ends
   * before the work does, so the claims view is degraded. */
  end_before_work: { explain: explainOf([PROMPT, STOP, { seq: 4, kind: 'tool_call', text: 'Edit cart.js' }], 'agent_finished'), claims: claimsOf([], true) },
  recorded_end_no_claim: { explain: explainOf([PROMPT, SAID, STOP], 'agent_finished'), claims: claimsOf([], false) },
  recorded_claim: { explain: explainOf([PROMPT, SAID, STOP], 'agent_finished'), claims: claimsOf([A_CLAIM], false) },
  claims_unknown: { explain: explainOf([PROMPT, SAID, STOP], 'agent_finished'), claims: null },
  /* UNRECORDED-WORK-NOT-SILENT-R3 — the round-2 adversary's break: the claim
   * was captured AND the record backs it (`supported` → Proven), and the said
   * section printed said_none over it. One shape per captured claim state. */
  claim_proven: { explain: explainOf([PROMPT, SAID, STOP], 'agent_finished'), claims: claimsOf([{ ...A_CLAIM, verdict: 'supported', reasons: [] }], false) },
  claim_recorded: { explain: explainOf([PROMPT, SAID, STOP], 'agent_finished'), claims: claimsOf([{ ...A_CLAIM, verdict: 'unverifiable' }], false) },
  claim_fog: { explain: explainOf([PROMPT, SAID, STOP], 'agent_finished'), claims: claimsOf([{ ...A_CLAIM, verdict: 'unverifiable' }], true) },
  /* A verdict this build has no word for: still captured, so still shown —
   * with no state word, because none is computed for it. */
  claim_unmapped: { explain: explainOf([PROMPT, SAID, STOP], 'agent_finished'), claims: claimsOf([{ ...A_CLAIM, verdict: 'pending' }], false) },
}

/** The real loader, reading one shape through a stub of the served routes. */
async function load(shape) {
  const routes = {
    ['/api/changes/' + CHANGE_ID]: { change },
    ['/api/changes/' + CHANGE_ID + '/proposals']: { proposals: [] },
    ['/api/changes/' + CHANGE_ID + '/coverage']: { coverage, readiness: null },
    '/api/explain': shape.explain,
    '/api/claims': shape.claims,
  }
  const held = Object.getOwnPropertyDescriptor(globalThis, 'fetch')
  Object.defineProperty(globalThis, 'fetch', {
    configurable: true, writable: true,
    value: async (url) => {
      const path = String(url).split('?')[0]
      const doc = Object.prototype.hasOwnProperty.call(routes, path) ? routes[path] : undefined
      const ok = doc !== undefined && doc !== null
      const status = ok ? 200 : doc === null ? 500 : 404
      return { ok, status, statusText: String(status), headers: { get: () => null }, text: async () => (ok ? JSON.stringify(doc) : '{}') }
    },
  })
  try {
    const read = await readChangeDocuments(CHANGE_ID)
    assert.equal(read.status, 'ready', JSON.stringify(read))
    return read
  } finally {
    if (held) Object.defineProperty(globalThis, 'fetch', held)
    else delete globalThis.fetch
  }
}

const text = (html) => html.replace(/<[^>]+>/g, ' ').replace(/&#x27;/g, "'").replace(/&#39;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ')

/** Every surface the card names, for one shape, from the real loader's work. */
async function surfaces(shapeName) {
  const read = await load(SHAPES[shapeName])
  assert.equal(read.work.length, 1, shapeName + ': the linked conversation is read')
  const hq = Model.deriveChangeModel(change, coverage, null, null, read.work)
  const model = { ...buildChangeView({ change, work: read.work }), hq }
  const said = hq.packet.sections.find((s) => s.id === 'said')
  const page = { ...buildPacketView({ change, work: read.work, coverage, evidence: [], hq }), hq }
  const exports = ['md', 'html', 'json'].map((format) => {
    const out = renderExport(hq.packet, null, { format })
    return text(typeof out === 'string' ? out : (out.text || out.content || JSON.stringify(out)))
  })
  return {
    hq,
    work: read.work,
    said,
    /* What the section renders: its rows, or its `none` when it has none. */
    saidText: said.rows.length ? said.rows.map((r) => [r.text, ...r.lines].join('\n')).join('\n') : said.none,
    verify: text(U.renderScreen({ model, stage: 'verify' })),
    top: ['brief', 'explore'].map((altitude) => text(U.renderShell({ model, stage: 'define', altitude, register: 'manager' }))),
    page: text(U.renderPage({ model: page })),
    exports,
  }
}

/** The shape is served as NOT recorded, on every surface, and nothing is quoted. */
async function assertNotRecorded(shapeName) {
  const s = await surfaces(shapeName)
  assert.equal(s.hq.unrecorded.count, 1, shapeName + ': hq.unrecorded counts it')
  assert.deepEqual(s.hq.unrecorded.ids, [SESSION])
  assert.ok(!s.saidText.includes(SAID_NONE), shapeName + ' said: ' + s.saidText)
  assert.ok(s.saidText.includes(MD.said_unrecorded_one), shapeName + ' said: ' + s.saidText)
  assert.ok(!s.verify.includes('contradicts what was said'), shapeName + ' verify')
  /* The Verify view prints its one line only when the work raised no card.
   * Where the claims view is degraded, the record's own card ("part of this
   * session was not recorded") stands in that place instead — also true, and
   * not "nothing contradicts". Either way the view never says nothing is
   * wrong with work whose words were not recorded. */
  if (s.work.every((w) => !w.findings.length)) assert.ok(s.verify.includes(V.unrecorded_one), shapeName + ' verify')
  else assert.ok(s.work.some((w) => w.findings.some((f) => f.state === 'fog')), shapeName + ' verify: a card stands in the line\'s place')
  for (const t of s.top) assert.ok(t.includes(T.top_one), shapeName + ' top')
  assert.ok(!s.page.includes(SAID_NONE), shapeName + ' page')
  assert.ok(!s.page.includes('contradicts what was said'), shapeName + ' page')
  assert.ok(s.page.includes(MD.said_unrecorded_one), shapeName + ' page')
  for (const e of s.exports) {
    assert.ok(!e.includes(SAID_NONE), shapeName + ' export')
    assert.ok(e.includes(MD.said_unrecorded_one), shapeName + ' export: ' + e.slice(0, 300))
  }
  /* Nothing quoted from the agent's log (FOUNDER-ASKS #19): the agent's own
   * sentence reaches none of the new lines. */
  for (const line of [s.hq.unrecorded.top, s.hq.unrecorded.verify, s.hq.unrecorded.said]) {
    assert.ok(!line.includes(AGENT_WORDS), line)
  }
  return s
}

test('0 steps: not recorded, on the Change screen, the page and the export', async () => {
  await assertNotRecorded('zero_steps')
})

test('the adversary\'s repro — events, no recorded end, no claims: not recorded, never said_none', async () => {
  const s = await assertNotRecorded('no_end_adversary')
  /* ThruLine DOES hold steps here, so no sentence may say it holds none. */
  for (const line of [s.saidText, s.verify, s.page, ...s.exports]) {
    assert.ok(!line.includes('holds none of the steps'), line.slice(0, 300))
  }
})

test('events without a recorded end, as the server serves them: not recorded', async () => {
  await assertNotRecorded('no_end_served')
})

test('a stop recorded before the work ended (capture degraded): not recorded', async () => {
  await assertNotRecorded('end_before_work')
})

test('claims unknown (the claims read refused): not recorded', async () => {
  await assertNotRecorded('claims_unknown')
})

test('a recorded end with no claim: said_none renders, and no not-recorded line anywhere', async () => {
  const s = await surfaces('recorded_end_no_claim')
  assert.equal(s.hq.unrecorded.count, 0)
  assert.equal(s.said.rows.length, 0)
  assert.equal(s.said.none, MD.said_none)
  assert.ok(s.verify.includes(V.nothing_wrong_unchecked_one), s.verify)
  assert.ok(!s.verify.includes(V.unrecorded_one))
  for (const t of s.top) assert.ok(!t.includes(T.top_one))
  assert.ok(s.page.includes(SAID_NONE), 'page')
  for (const e of s.exports) {
    assert.ok(e.includes(SAID_NONE), 'export')
    assert.ok(!e.includes(MD.said_unrecorded_one), 'export')
  }
})

test('a recorded claim: neither sentence, and the claim shows', async () => {
  const s = await surfaces('recorded_claim')
  assert.equal(s.hq.unrecorded.count, 0)
  assert.ok(s.said.rows.length >= 1, 'the claim is a row')
  assert.ok(s.saidText.includes('The tests pass.'), s.saidText)
  assert.ok(!s.saidText.includes(SAID_NONE), s.saidText)
  assert.ok(!s.saidText.includes(MD.said_unrecorded_one), s.saidText)
  assert.ok(!s.verify.includes(V.unrecorded_one))
  for (const t of s.top) assert.ok(!t.includes(T.top_one))
  /* The Markdown and HTML files render the rows; the JSON file carries every
   * section's `none` copy beside its rows, as it does for every section, so
   * only the rendered formats are held to the absence of said_none here. */
  const [md, html, json] = s.exports
  for (const e of [md, html]) assert.ok(!e.includes(SAID_NONE), 'export')
  for (const e of [md, html, json]) {
    assert.ok(!e.includes(MD.said_unrecorded_one), 'export')
    assert.ok(e.includes('The tests pass.'), 'export')
  }
})

test('not recorded claims less, and only less: no state word, count, readiness or approval moves', async () => {
  const decided = (hq) => ({
    criteria: hq.criteria.map((c) => [c.n, c.state, c.word, c.line]),
    counts: hq.counts,
    headline: hq.headline,
    readiness: hq.readiness,
    decision: hq.decision,
    action: hq.inspector.action,
    inbox: { ...hq.inboxRow },
    runs: hq.packet.runs,
    proof: hq.packet.sections.find((s) => s.id === 'proof'),
  })
  const recorded = (await surfaces('recorded_end_no_claim')).hq
  for (const shape of ['zero_steps', 'no_end_adversary', 'no_end_served', 'end_before_work', 'claims_unknown']) {
    assert.deepEqual(decided((await surfaces(shape)).hq), decided(recorded), shape + ' moved a decided field')
  }
})

/* ── UNRECORDED-WORK-NOT-SILENT-R3 ─────────────────────────────────────────
 * Every captured claim shows in the said section exactly once, beside the
 * state word the product already computes for it (`HQ_COPY.states`, via
 * `verdictWord`), on the Change model, the whole-story page and all three
 * export formats. said_none renders nowhere while a claim exists — not even
 * in the JSON file's section `none`. */
const CLAIM_SHAPES = {
  recorded_claim: HQ_COPY.states.not_proven.word,
  claim_proven: HQ_COPY.states.proven.word,
  claim_recorded: HQ_COPY.states.couldnt.word,
  claim_fog: HQ_COPY.states.fog.word,
  claim_unmapped: null,
}

for (const [shape, word] of Object.entries(CLAIM_SHAPES)) {
  test(`R3 ${shape}: the captured claim is one said row${word ? ' marked ' + word : ''}, and said_none renders nowhere`, async () => {
    const s = await surfaces(shape)
    assert.equal(s.hq.unrecorded.count, 0, shape + ': the work is recorded')
    const rows = s.said.rows.filter((r) => [r.text, ...r.lines].join('\n').includes('The tests pass.'))
    assert.equal(rows.length, 1, shape + ': the claim shows once: ' + s.saidText)
    if (word) assert.ok(rows[0].text.includes(' — ' + word), shape + ': its state word: ' + rows[0].text)
    else for (const w of Object.values(HQ_COPY.states).map((x) => x.word)) assert.ok(!rows[0].text.includes(' — ' + w), shape + ': no word invented: ' + rows[0].text)
    assert.notEqual(s.said.none, SAID_NONE, shape + ': the section none')
    assert.ok(!s.saidText.includes(SAID_NONE), shape)
    assert.ok(!s.saidText.includes(MD.said_unrecorded_one), shape)
    assert.ok(!s.verify.includes(SAID_NONE), shape + ' verify')
    assert.ok(!s.page.includes(SAID_NONE), shape + ' page')
    assert.ok(s.page.includes('The tests pass.'), shape + ' page')
    const [md, html, json] = s.exports
    for (const [format, e] of [['md', md], ['html', html], ['json', json]]) {
      assert.ok(!e.includes(SAID_NONE), shape + ' ' + format + ' carries said_none')
      assert.ok(e.includes('The tests pass.'), shape + ' ' + format + ' lost the claim')
      if (word) assert.ok(e.includes(' — ' + word), shape + ' ' + format + ' lost the state word')
    }
    const doc = JSON.parse(renderExport(s.hq.packet, null, { format: 'json' }).text)
    const section = doc.sections.find((x) => x.id === 'said')
    assert.notEqual(section.none, SAID_NONE, shape + ': the JSON section none')
  })
}

test('R3: a proven claim beside a not-proven one in two linked conversations: both show, once each, with their own words', async () => {
  const proven = await surfaces('claim_proven')
  const flagged = await surfaces('recorded_claim')
  const work = [...proven.work, ...flagged.work.map((w) => ({ ...w, id: 'cccccccc-0000-4000-8000-00000000r002' }))]
  const two = { ...change, associations: [...change.associations, { ...change.associations[0], id: 'a2', session_id: 'cccccccc-0000-4000-8000-00000000r002' }] }
  const hq = Model.deriveChangeModel(two, coverage, null, null, work)
  const said = hq.packet.sections.find((x) => x.id === 'said')
  const texts = said.rows.map((r) => r.text)
  assert.equal(texts.filter((t) => t.includes(' — ' + HQ_COPY.states.proven.word)).length, 1, texts.join('\n'))
  assert.equal(texts.filter((t) => t.includes(' — ' + HQ_COPY.states.not_proven.word)).length, 1, texts.join('\n'))
  assert.notEqual(said.none, SAID_NONE)
})

test('R3: still said_none where the end was recorded and nothing was claimed; nothing decided moves when a claim shows', async () => {
  const quiet = await surfaces('recorded_end_no_claim')
  assert.equal(quiet.said.none, SAID_NONE)
  assert.equal(quiet.said.rows.length, 0)
  const decided = (hq) => ({
    criteria: hq.criteria.map((c) => [c.n, c.state, c.word, c.line]),
    counts: hq.counts,
    readiness: hq.readiness,
    decision: hq.decision,
    runs: hq.packet.runs,
    proof: hq.packet.sections.find((x) => x.id === 'proof'),
  })
  for (const shape of Object.keys(CLAIM_SHAPES)) {
    assert.deepEqual(decided((await surfaces(shape)).hq), decided(quiet.hq), shape + ' moved a decided field')
  }
})
