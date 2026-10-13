/* M6.R2.2 — L1, the board.
 *
 * The gates this screen has to pass are the ones L0 passes, one altitude down,
 * plus the one that is new here: a region that is designed into the layout and
 * renders nothing, because its class has not graduated. */

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

import { GLYPH, SLUG_FOR_WORD, board, chipsFrom, chipLine, graduated, proofKey, uneaten } from './board.js'
import * as boardModule from './board.js'
import { bannedIn, word } from './voice.js'

const here = dirname(fileURLToPath(import.meta.url))
const src = (rel) => readFileSync(join(here, '..', rel), 'utf8')

/* RA5: the arcade L1 is superseded by the macOS graph shell; structural
 * assertions live against the new files, same rules. */
const GRAPH = src('mac/graph.jsx')
const FLOW = src('mac/decision-flow.jsx')
const WORK_MAP = src('mac/work-map.jsx')
const MACSHELL = src('mac/shell.jsx')

const SYNTH = {
  contract_version: 2,
  project: { id: 'p', name: 'synth', source: 'claude_code' },
  diagnostics: false,
  ledger: { events: 4, head: 'abc', verified: true },
  sessions: [
    { id: 's1', label: 'Session 1', date: 'Jul 27', msgs: 812 },
    { id: 's2', label: 'Session 2', date: 'Jul 29', msgs: 1 },
  ],
  counts: { recorded: 1, supported: 0, pending: 1, unknown: 0, contradicted: 1, refused: 0 },
  nodes: [
    { id: 'n1', label: 'first', sort_key: 'recorded', session_id: 's1', evidence_count: 1,
      anchor_event_id: 'e1', accusation: false,
      dimensions: { lifecycle: 'completed', evidence: 'observed', currency: 'current' } },
    { id: 'n2', label: 'second', sort_key: 'pending', session_id: 's1', evidence_count: 0,
      anchor_event_id: 'e2', accusation: false,
      dimensions: { lifecycle: 'proposed', evidence: 'unknown', currency: 'current' } },
    { id: 'n3', label: 'third', sort_key: 'contradicted', session_id: 's2', evidence_count: 2,
      anchor_event_id: 'e3', accusation: true,
      dimensions: { lifecycle: 'completed', evidence: 'contradicted', currency: 'current' } },
    /* A shadow node, present in the document. It must not reach the board. */
    { id: 'n4', label: 'shadow', sort_key: 'dropped', session_id: 's2', evidence_count: 0,
      anchor_event_id: 'e4', accusation: false,
      dimensions: { lifecycle: 'abandoned', evidence: 'unknown', currency: 'superseded' } },
  ],
  edges: [],
  receipts: {
    n3: { claim: { text: 'all 33 tests pass', session: 's2', message_no: 4, ts: '2026-07-29T10:00:00Z' } },
  },
}

/* ── the board derives, it does not compose ────────────────────────────── */

test('past runs are newest first, and every count carries its noun', () => {
  const b = board(SYNTH)
  assert.deepEqual(b.runs.map((r) => r.date), ['Jul 29', 'Jul 27'])
  assert.equal(b.runs[0].messages, '1 message')
  assert.equal(b.runs[1].messages, '812 messages')
  assert.equal(b.runs[0].claims, '1 claim')
  assert.equal(b.runs[1].claims, '2 claims')
  /* A83 DOM-scan catch: a run with no claims rendered "14 messages · 0 claims"
     — the `· 0 conflicts` shape §4 bans. The clause drops; the run stays. */
  const empty = board({ ...SYNTH, nodes: [], receipts: {} })
  assert.equal(empty.runs[0].claims, '')
  assert.equal(empty.runs[0].messages, '1 message')
  for (const r of b.runs) {
    assert.doesNotMatch(r.messages, /^\d+$/, 'a naked number reached a run card')
    assert.equal(bannedIn(r.verdictLine), null)
  }
  // The verdict line is the authority's composition, worst first.
  assert.equal(b.runs[0].verdictLine, '1 contradicted')
  assert.equal(b.runs[1].verdictLine, '1 unfinished · 1 noted')
})

test('the dot ledger is newest first and identical across two derivations', () => {
  const a = board(SYNTH)
  const b = board(SYNTH)
  assert.deepEqual(a.ledger, b.ledger, 'two derivations of one document disagree')
  assert.deepEqual(a.ledger.map((r) => r.id), ['n3', 'n2', 'n1'])
  const row = a.ledger[0]
  assert.equal(row.word, word('contradicted'))
  assert.equal(row.sentence, 'The AI said this was done — the record shows otherwise.')
  // The project's own words sit beside our sentence, never rewritten into it.
  assert.equal(row.subject, 'all 33 tests pass')
  assert.equal(row.date, 'Jul 29')
  assert.ok(row.glyph && row.glyph.length > 0, 'a state with no glyph survives only in colour')
})

test('every ledger row carries a glyph and a word, so the state survives grayscale', () => {
  for (const state of Object.keys(GLYPH)) {
    assert.ok(GLYPH[state], `${state} has no glyph`)
    assert.ok(word(state), `${state} has no word`)
  }
  const b = board(SYNTH)
  for (const r of b.ledger) {
    assert.ok(r.glyph, `${r.id} renders colour alone`)
    assert.ok(r.word, `${r.id} renders colour alone`)
  }
})

test('a zero-count chip does not exist to be rendered', () => {
  assert.deepEqual(chipsFrom({ contradicted: 0, refused: 0 }), [])
  assert.equal(chipLine([]), '')
  const b = board(SYNTH)
  for (const c of b.chips) assert.ok(c.count > 0, `a zero chip survived: ${c.word}`)
  assert.equal(chipLine(b.chips), '1 contradicted · 1 unfinished · 1 noted')
})

test('the index emits words, and every word maps back to a state', () => {
  for (const w of ['contradicted', 'unfinished', 'noted', 'insufficient evidence', 'supported', 'proven', 'out of date']) {
    assert.ok(SLUG_FOR_WORD[w], `no state for the index word ${JSON.stringify(w)}`)
  }
})

/* ── shadow containment, one altitude down ─────────────────────────────── */

test('no shadow class reaches the board', () => {
  const b = board(SYNTH)
  assert.ok(!b.ledger.some((r) => r.id === 'n4'), 'a shadow node reached the dot ledger')
  assert.ok(!b.chips.some((c) => ['no proof found', 'abandoned'].includes(c.word)))
  assert.ok(!b.runs.some((r) => /no proof found|abandoned/.test(r.verdictLine)))
  // Shadow nodes and shadow claims on this surface: 0 / 0.
  assert.equal(b.ledger.filter((r) => ['unsupported', 'dropped'].includes(r.state)).length, 0)
  assert.equal(b.chips.filter((c) => ['unsupported', 'dropped'].includes(c.state)).length, 0)
})

test('the uneaten region is designed and does not render', () => {
  /* The gate outlives the screen that first carried it: `uneaten` still
     answers null while its class is in shadow, and no macOS surface names the
     region. The moment the class graduates this test starts failing on the
     absence of a caller — which is the alarm doing its job. */
  assert.equal(graduated('dropped'), false)
  assert.equal(uneaten('dropped', [{ id: 'x' }]), null)
  assert.equal(graduated('recorded'), true)
  assert.deepEqual(uneaten('recorded', []), { label: 'LEFTOVERS', rows: [] })
  for (const text of [GRAPH, FLOW, WORK_MAP, MACSHELL]) {
    assert.ok(!/LEFTOVERS/.test(text.replace(/\/\*[\s\S]*?\*\//g, '')), 'the region label reached a macOS surface')
  }
})

test('every first-level string the graph shell hardcodes passes the lexicon gate', () => {
  const stripped = (t) => t.replace(/\/\*[\s\S]*?\*\//g, '')
  for (const [name, text] of [['mac/work-map.jsx', WORK_MAP], ['mac/decision-flow.jsx', FLOW], ['mac/shell.jsx', MACSHELL]]) {
    for (const m of stripped(text).matchAll(/>([^<>{}\n]{4,120})</g)) {
      const t = m[1].trim()
      if (!/[A-Za-z]/.test(t) || !t.includes(' ')) continue
      if (/(px|rem|%|var\(|:\s*\d|--[a-z]|;)/.test(t)) continue
      const hit = bannedIn(t)
      assert.equal(hit, null, `banned term ${JSON.stringify(hit)} in ${name}: ${JSON.stringify(t)}`)
    }
  }
})

test('the shell composes no verdict English — words and sentences come from the authority', () => {
  assert.match(src('lib/board.js'), /sentence: sentence\(n\.state\)/)
  assert.match(src('lib/macmap.js'), /chipLabel\(word\(state\)\)/)
  // No hardcoded authority sentence in the macOS surfaces.
  for (const banned of [
    'The AI said this was done',
    'Backed by the record',
    'No proof of this in the record',
  ]) {
    assert.ok(!WORK_MAP.includes(banned) && !FLOW.includes(banned) && !MACSHELL.includes(banned), `a surface hardcodes: ${banned}`)
  }
})
