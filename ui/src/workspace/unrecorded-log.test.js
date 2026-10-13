import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* UNRECORDED-WORK-NO-QUOTE (launch card; the founder retired verbatim quoting
 * at launch, FOUNDER-INBOX FI-30 on #27).
 *
 * For linked work ThruLine was not recording, the Change says three things and
 * shows no word of the agent's own log:
 *   1. what the agent said in that work was not recorded, so it has not been
 *      checked (the sentence rounds 1–3 built, unchanged);
 *   2. ThruLine wasn't recording when the work was done, the agent's own log
 *      holds what it said, and this release does not show its words;
 *   3. where in that log the agent said it — a file and a line — one level
 *      down, folded away on the page and in the technical part of a file.
 *
 * The server serves the place beside the Change (`GET /api/changes/<id>` →
 * `agent_log.sessions[].receipt`, {file, line}). This file drives the REAL
 * loader (`readChangeDocuments`, the served routes answered by a stub
 * `fetch`) and then every surface from the one field `hq.unrecorded`: every
 * stage and altitude of the Change, the whole-story page, and the exported
 * file in all three formats, redacted or not.
 *
 * The canary sentence and the key pieces below are invented. Where a served
 * document carries them anyway (a server from before this card, a receipt
 * with an extra field) the UI never draws them.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-unrecorded-log-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeShell, ChangeScreen, readChangeDocuments } from './change-workspace.jsx';
  import { PacketPage } from './packet.jsx';
  import { buildPacketView } from './packet-view.js';
  import { buildChangeView, STAGES } from './change-view.js';
  import * as Model from './change-model.js';
  import { HQ_COPY } from './hq-copy.js';
  import { renderExport } from './packet-export.js';
  export { buildPacketView, buildChangeView, STAGES, Model, HQ_COPY, renderExport, readChangeDocuments };
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
const file = join(temporary, 'unrecorded-log.mjs')
await writeFile(file, bundled.outputFiles[0].text)
const U = await import(pathToFileURL(file).href)
const { buildPacketView, buildChangeView, STAGES, Model, HQ_COPY, renderExport, readChangeDocuments } = U

const MD = HQ_COPY.change.packet.markdown
const T = HQ_COPY.change.unrecorded

/* The card's three facts, in the words the copy file must hold. */
const LOG_ONE = "ThruLine wasn't recording when one piece of the work you linked was done. The agent's own log of it holds what it said, but this release does not show its words."
const LOG_MANY = "ThruLine wasn't recording when 2 pieces of the work you linked were done. The agent's own logs of them hold what it said, but this release does not show their words."

const LOG = '/synthetic/claude/projects/-synthetic-shop/cccccccc-0000-4000-8000-00000000q001.jsonl'
const LOG_2 = '/synthetic/claude/projects/-synthetic-shop/cccccccc-0000-4000-8000-00000000q002.jsonl'
const RECEIPT = LOG + ':4'
const RECEIPT_2 = LOG_2 + ':6'

const fillN = (template, n) => template.replace('{n}', String(n))

/* What only the agent's log holds. None of it may reach any surface. */
const CANARY = 'The quokka cart total is added and the tests pass.'
const WORDS = ['quokka', 'cart total is added', 'zqvx', 'krmt', 'wbnj', 'hcsf', 'plmg', 'QX72-M9ZP']

const CHANGE_ID = 'cccccccc-0000-4000-8000-00000000c0q1'
const SESSION = 'cccccccc-0000-4000-8000-00000000q001'
const SESSION_2 = 'cccccccc-0000-4000-8000-00000000q002'

const changeOf = (sessions) => ({
  id: CHANGE_ID,
  project_id: 'cccccccc-0000-4000-8000-00000000p0q1',
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
  associations: sessions.map((id, i) => ({ id: 'a' + (i + 1), session_id: id, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-01-02T10:01:00Z', reason: null })),
  examination: 'none_yet', decision: 'none_yet',
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
})
const change = changeOf([SESSION])
const coverage = { scope_version: 1, link_rows: 0, execution_sessions: [], criteria: [
  { index: 1, text: 'The cart shows a total.', state: 'none', items: [] },
], candidates: [] }

/* What the server serves beside the Change (agent_log.rs `AgentLogView`). */
const MASKING = { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' }
const located = (id, place) => ({ session_id: id, receipt: place, not_located: null })
const SERVED = { sessions: [located(SESSION, { file: LOG, line: 4 })], masking: MASKING }
const SERVED_TWO = { sessions: [located(SESSION, { file: LOG, line: 4 }), located(SESSION_2, { file: LOG_2, line: 6 })], masking: MASKING }
/* The role field was missing or disagreed, so nothing is located. */
const NOT_LOCATED = { sessions: [{ session_id: SESSION, receipt: null, not_located: 'no_agent_claim' }], masking: MASKING }
/* A server from before this card still serves the words beside the Change. */
const STALE = { sessions: [{ session_id: SESSION, quote: { text: '…' + CANARY, shortened: true, file: LOG, line: 4, role_field: 'message.role' }, withheld: null, not_quoted: null }], masking: MASKING }

const explainOf = (whatHappened, why) => ({
  shape: 'thruline.explain', schema_version: 1, session: SESSION,
  what_happened: whatHappened,
  ...(why ? { why_it_stopped: { kind: why, summary: 'synthetic', last_events: [] } } : {}),
  claims_vs_proof: [], promises: [], risk_flags: [],
})
const claimsOf = (claims, degraded) => ({
  shape: 'thruline.claims', schema_version: 1, session: SESSION, final_tree_hash: null, checks: 0,
  claims, promises: [], negated: [], capture_degraded: degraded,
})
const A_CLAIM = {
  id: 'clm_q1', kind: 'tests_pass', text: 'The tests pass.', span: { event_id: 'evt_stop', offset: [0, 15] },
  scope: null, extractor: 'table', verdict: 'unsupported', verifier: 'tests_pass@1',
  evidence: [], reasons: ['no test run is recorded'], warnings: [], binding: null, findings: [],
}
const PROMPT = { seq: 1, kind: 'user_prompt', text: 'Add the cart total.' }
const SAID = { seq: 2, kind: 'assistant_text', text: 'Done.' }
const STOP = { seq: 3, kind: 'stop', text: 'stopped' }

const SHAPES = {
  /* ThruLine was not recording: the conversation reaches the server only as
   * an ingested log, so the execution store holds nothing of it. */
  zero_steps: { explain: explainOf([], 'no_stop_recorded'), claims: claimsOf([], false) },
  no_end: { explain: explainOf([PROMPT, SAID], 'no_stop_recorded'), claims: claimsOf([], true) },
  /* ThruLine WAS recording. */
  recorded_end_no_claim: { explain: explainOf([PROMPT, SAID, STOP], 'agent_finished'), claims: claimsOf([], false) },
  recorded_claim: { explain: explainOf([PROMPT, SAID, STOP], 'agent_finished'), claims: claimsOf([A_CLAIM], false) },
}

/** The real loader, reading one shape through a stub of the served routes. */
async function load(shape, head) {
  const routes = {
    ['/api/changes/' + CHANGE_ID]: head,
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
async function surfaces(shapeName, agentLog, options = {}) {
  const sessions = options.sessions || [SESSION]
  const theChange = changeOf(sessions)
  const head = { change: theChange }
  if (agentLog !== undefined) head.agent_log = agentLog
  if (options.stale) head.agent_said = options.stale
  const read = await load(SHAPES[shapeName], head)
  const hq = Model.deriveChangeModel(theChange, coverage, null, null, read.work)
  const model = { ...buildChangeView({ change: theChange, work: read.work }), hq }
  const said = hq.packet.sections.find((s) => s.id === 'said')
  const page = { ...buildPacketView({ change: theChange, work: read.work, coverage, evidence: [], hq }), hq }
  const exportOf = (format, redact) => renderExport(hq.packet, null, { format, redact }).text
  const shells = []
  for (const stage of STAGES) {
    shells.push(U.renderScreen({ model, stage }))
    for (const altitude of ['brief', 'explore']) shells.push(U.renderShell({ model, stage, altitude, register: 'manager' }))
  }
  const redact = ['agent_words', 'paths']
  return {
    read,
    hq,
    said,
    shells,
    top: ['brief', 'explore'].map((altitude) => U.renderShell({ model, stage: 'define', altitude, register: 'manager' })),
    page: U.renderPage({ model: page }),
    md: exportOf('md'),
    html: exportOf('html'),
    json: exportOf('json'),
    redacted: ['md', 'html', 'json'].map((format) => exportOf(format, redact)),
  }
}

const everything = (s) => [...s.shells, s.page, s.md, s.html, s.json, ...s.redacted, JSON.stringify(s.hq), JSON.stringify(s.read.work)]

function assertNoWords(label, surface) {
  for (const word of WORDS) assert.ok(!surface.includes(word), `${label} carries "${word}": ${surface.slice(0, 300)}`)
}

/* ── the copy ─────────────────────────────────────────────────────────── */

test('the copy says what the card says, in plain words, and quotes no one', () => {
  assert.equal(T.log_one, LOG_ONE)
  assert.equal(T.log_many, "ThruLine wasn't recording when {n} pieces of the work you linked were done. The agent's own logs of them hold what it said, but this release does not show their words.")
  for (const sentence of [T.log_one, T.log_many]) {
    assert.doesNotMatch(sentence, /["“”]/, 'a quote mark: ' + sentence)
    assert.doesNotMatch(sentence, /\b(session|event|ledger|receipt|packet|lie|lied|false|fake|claimed)\b/i, sentence)
    assert.ok(!/\{(?!n\})/.test(sentence), 'no slot for anything the agent said: ' + sentence)
  }
})

test('the quote sentences are gone from the copy', () => {
  assert.equal(T.quote, undefined)
  assert.equal(T.withheld, undefined)
})

/* ── where ThruLine was not recording ─────────────────────────────────── */

for (const shape of ['zero_steps', 'no_end']) {
  test(shape + ': every stage and altitude keeps the not-recorded sentence and adds the three facts, with the place one level down', async () => {
    const s = await surfaces(shape, SERVED)
    assert.equal(s.hq.unrecorded.count, 1)
    assert.equal(s.hq.unrecorded.held, LOG_ONE)
    assert.deepEqual(s.hq.unrecorded.logs.map((l) => l.receipt), [RECEIPT])
    for (const html of s.top) {
      const t = text(html)
      assert.ok(t.includes(T.top_one), t.slice(0, 400))
      assert.ok(t.indexOf(LOG_ONE) > t.indexOf(T.top_one), t.slice(0, 600))
      /* One level down: the place is inside a folded <details>, closed, and
       * never on the plain face. */
      const at = html.indexOf('ws-log-receipt')
      assert.ok(at > 0, 'the fold is drawn')
      const fold = html.slice(html.lastIndexOf('<details', at), html.indexOf('</details>', at))
      assert.ok(fold.includes(RECEIPT), fold)
      assert.doesNotMatch(fold, /<details[^>]*\bopen\b/, 'the fold is closed')
      assert.ok(!text(html.replace(/<details[\s\S]*?<\/details>/g, '')).includes(RECEIPT), 'the place is not on the plain face')
    }
    for (const html of s.shells) assertNoWords('a stage or altitude', html)
  })

  test(shape + ': the said section, the page and all three formats say the same, and Copy as Markdown is the export', async () => {
    const s = await surfaces(shape, SERVED)
    const mine = s.said.rows.filter((r) => r.text === MD.said_unrecorded_one)
    assert.equal(mine.length, 1)
    assert.equal(s.said.rows[s.said.rows.length - 1], mine[0])
    assert.deepEqual(mine[0].lines, [LOG_ONE])
    assert.deepEqual(mine[0].receipts, [RECEIPT])
    const page = text(s.page)
    assert.ok(page.includes(MD.said_unrecorded_one), 'page')
    assert.ok(page.includes(LOG_ONE), 'page')
    assert.ok(s.page.includes('ws-log-receipt') && s.page.includes(RECEIPT), 'page: the place, folded')
    for (const [name, out] of [['md', s.md], ['html', text(s.html)]]) {
      assert.ok(out.includes(LOG_ONE), name + ': ' + out.slice(0, 400))
      /* One level down in a file: its technical part, after the heading. */
      const tech = out.indexOf(HQ_COPY.change.export.doc.technical_heading)
      assert.ok(tech > 0 && out.indexOf(RECEIPT) > tech, name + ': the place is in the technical part')
      assert.ok(out.includes(HQ_COPY.change.export.doc.tech_agent_said + ': ' + RECEIPT), name)
    }
    const doc = JSON.parse(s.json)
    const row = doc.sections.find((x) => x.id === 'said').rows.find((r) => r.text === MD.said_unrecorded_one)
    assert.deepEqual(row.lines, [LOG_ONE])
    assert.deepEqual(row.receipts, [RECEIPT])
  })

  test(shape + ': nothing of the log reaches any surface, redacted or not', async () => {
    const s = await surfaces(shape, SERVED)
    for (const out of everything(s)) assertNoWords(shape, out)
  })
}

test('the redacted copy removes the log path and keeps the sentence', async () => {
  const s = await surfaces('no_end', SERVED)
  assert.equal(s.redacted.length, 3)
  for (const out of s.redacted) {
    assert.ok(!out.includes(LOG), out.slice(0, 400))
    assert.ok(text(out).includes(LOG_ONE), out.slice(0, 400))
  }
})

test('two pieces of work, both located: one sentence for both, and both places', async () => {
  const s = await surfaces('zero_steps', SERVED_TWO, { sessions: [SESSION, SESSION_2] })
  assert.equal(s.hq.unrecorded.count, 2)
  assert.equal(s.hq.unrecorded.held, LOG_MANY)
  assert.deepEqual(s.hq.unrecorded.logs.map((l) => l.receipt), [RECEIPT, RECEIPT_2])
  for (const html of s.top) {
    const t = text(html)
    assert.ok(t.includes(LOG_MANY), t.slice(0, 600))
    assert.equal(t.split(LOG_MANY).length - 1, 1, 'said once, not once for each piece of work')
    assert.ok(html.includes(RECEIPT) && html.includes(RECEIPT_2))
  }
  const row = s.said.rows.find((r) => r.text === fillN(MD.said_unrecorded_many, 2))
  assert.deepEqual(row.lines, [LOG_MANY])
  assert.deepEqual(row.receipts, [RECEIPT, RECEIPT_2])
  for (const out of everything(s)) assertNoWords('two pieces', out)
})


test('two pieces of work, one located: the sentence for one, and only its place', async () => {
  const served = { sessions: [located(SESSION, { file: LOG, line: 4 }), { session_id: SESSION_2, receipt: null, not_located: 'log_unreadable' }], masking: MASKING }
  const s = await surfaces('zero_steps', served, { sessions: [SESSION, SESSION_2] })
  assert.equal(s.hq.unrecorded.count, 2)
  assert.equal(s.hq.unrecorded.held, LOG_ONE)
  assert.deepEqual(s.hq.unrecorded.logs.map((l) => l.receipt), [RECEIPT])
  assert.ok(text(s.top[0]).includes(fillN(T.top_many, 2)))
  assert.ok(!s.page.includes(RECEIPT_2))
})

/* ── nothing to say beyond the not-recorded sentence ──────────────────── */

for (const [name, served] of [['no role-marked claim', NOT_LOCATED], ['an older server with no agent_log', undefined]]) {
  test(name + ': the not-recorded sentence stands alone', async () => {
    const s = await surfaces('no_end', served)
    assert.equal(s.hq.unrecorded.count, 1)
    assert.equal(s.hq.unrecorded.held, null)
    assert.deepEqual(s.hq.unrecorded.logs, [])
    assert.deepEqual(s.said.rows.find((r) => r.text === MD.said_unrecorded_one).lines, [])
    for (const out of [...s.shells.map(text), text(s.page), s.md, text(s.html), s.json]) {
      assert.ok(!out.includes("ThruLine wasn't recording"), out.slice(0, 300))
      assert.ok(!out.includes('does not show its words'))
    }
    for (const html of s.top) assert.ok(text(html).includes(T.top_one))
  })
}

/* ── ThruLine was recording: the log is not mentioned ─────────────────── */

for (const shape of ['recorded_end_no_claim', 'recorded_claim']) {
  test(shape + ': ThruLine was recording, so a served place appears nowhere', async () => {
    const s = await surfaces(shape, SERVED)
    assert.equal(s.hq.unrecorded.count, 0)
    assert.equal(s.hq.unrecorded.held, null)
    assert.deepEqual(s.hq.unrecorded.logs, [])
    for (const out of [...s.shells.map(text), text(s.page), s.md, text(s.html), s.json]) {
      assert.ok(!out.includes(LOG_ONE), shape)
      assert.ok(!out.includes(RECEIPT), shape)
    }
  })
}

/* ── the UI draws no word, whatever is served ─────────────────────────── */

test('a server that still serves the words is not read for them', async () => {
  const s = await surfaces('no_end', undefined, { stale: STALE })
  for (const out of everything(s)) assertNoWords('an older server', out)
  assert.equal(s.hq.unrecorded.held, null)
})

test('a receipt that carries more than a file and a line is read for those two only', async () => {
  const hostile = { sessions: [located(SESSION, { file: LOG, line: 4, text: CANARY, said: CANARY, quote: CANARY, words: 'zqvx krmt wbnj hcsf plmg' })], masking: MASKING }
  const s = await surfaces('no_end', hostile)
  assert.equal(s.hq.unrecorded.held, LOG_ONE)
  assert.deepEqual(Object.keys(s.hq.unrecorded.logs[0]).sort(), ['file', 'id', 'line', 'receipt'])
  assert.deepEqual(Object.keys(s.read.work[0].log).sort(), ['file', 'line'])
  for (const out of everything(s)) assertNoWords('a hostile receipt', out)
})

/* ── it claims less, and only less ────────────────────────────────────── */

test('the place moves no state word, count, readiness, coverage, approval or finding', async () => {
  const decided = (hq) => JSON.stringify({
    criteria: hq.criteria.map((c) => [c.n, c.state, c.word, c.line]),
    counts: hq.counts,
    headline: hq.headline,
    state: hq.state,
    word: hq.word,
    readiness: hq.readiness,
    decision: hq.decision,
    action: hq.inspector.action,
    inbox: hq.inboxRow,
    attention: hq.attention,
    unrecorded: { count: hq.unrecorded.count, ids: hq.unrecorded.ids, top: hq.unrecorded.top, verify: hq.unrecorded.verify, said: hq.unrecorded.said },
    runs: hq.packet.runs,
    findings: hq.packet.findings,
    notRecorded: hq.packet.notRecorded,
    readinessLine: hq.packet.readinessLine,
    readinessRests: hq.packet.readinessRests,
  })
  for (const shape of Object.keys(SHAPES)) {
    const without = await surfaces(shape, NOT_LOCATED)
    const withPlace = await surfaces(shape, SERVED)
    assert.equal(decided(withPlace.hq), decided(without.hq), shape)
    /* Never a finding and never the speaker hooks' words. */
    for (const out of [...withPlace.shells.map(text), text(withPlace.page), withPlace.md]) {
      assert.ok(!/claimed, and no proof found/i.test(out), shape)
    }
  }
})
