import test from 'node:test'
import assert from 'node:assert/strict'
import { buildVerdict, claimKind, claimState } from './verdict-model.js'
import { HQ_COPY } from './hq-copy.js'
import { findBannedToken, placeholders } from './hq-narrate.js'

/* ── fixtures ────────────────────────────────────────────────────────── */

function claim(i, over = {}) {
  return { id: `c${i}`, node: i === 1 ? 'clm' : `clm${i}`, kind: 'task_complete', text: `Thing ${i} is done.`, verdict: 'unsupported', verifier: 'tests_pass@1', evidence: [], reasons: [], findings: [], ...over }
}
function finding(i, key, severity = 'fail', path) {
  const f = { key, severity, detail: `${key} detail`, node: `f_${key}_${i}` }
  if (path) f.path = path
  return f
}
function session(over = {}) {
  return { key: 'k', harness: 'Claude', finalTree: 'T2', capture: { degraded: false, condition: 'intact' }, events: [], claims: [], promises: [], checks: [], nodes: [], ...over }
}
const EVENTS = [
  { seq: 1, kind: 'user_prompt', text: 'fix the totals' },
  { seq: 5, kind: 'exec', text: 'cargo test → exit 1' },
  { seq: 6, kind: 'snapshot', text: 'tree T1' },
  { seq: 9, kind: 'fs_change', text: 'modify src/a.rs' },
  { seq: 12, kind: 'snapshot', text: 'tree T2' },
  { seq: 14, kind: 'tool_result', text: 'Bash FAILED' },
]

/* ── the copy as a matcher: is a rendered string composed of templates? ── */

function leaves(obj, out = []) {
  if (Array.isArray(obj)) obj.forEach((v) => leaves(v, out))
  else if (obj && typeof obj === 'object') Object.values(obj).forEach((v) => leaves(v, out))
  else if (typeof obj === 'string') out.push(obj)
  return out
}
/* A template that is nothing but a placeholder (`{n}`) would match anything
 * and make this matcher accept every string; those are excluded, and a
 * rendered value that is a bare number is accepted as a number instead. */
const TEMPLATES = [...new Set(leaves(HQ_COPY))].filter((t) => t.replace(/\{[a-z_]+\}/g, '').trim().length > 1)
const escape = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
/* A slot the template closes with a quotation mark and then a full stop (`"{x}".`) renders without that stop when, and only
 * when, its value ends its own sentence (hq-narrate `closeQuotation`, commit G p1p2-followups, p2 verifier F6). The
 * matcher reads that one shape as well as the template's own; a stop-less quotation that did not end is still rejected. */
const QUOTE_THEN_STOP = /\\\{[a-z_]+\\\}"\\\./g
const REGEXES = TEMPLATES.map((t) => new RegExp('^' + escape(t).replace(QUOTE_THEN_STOP, '(?:[\\s\\S]*?"\\.|[\\s\\S]*?[.!?…]\\s*")').replace(/\\\{[a-z_]+\\\}/g, '[\\s\\S]*?') + '(?: |$)'))
function fromCopy(text, depth = 0) {
  if (!text) return true
  if (depth === 0 && /^\d+$/.test(text)) return true
  if (depth > 8) return false
  for (const re of REGEXES) {
    const m = re.exec(text)
    if (m && fromCopy(text.slice(m[0].length), depth + 1)) return true
  }
  return false
}
function renderedStrings(V) {
  const out = [V.headline, HQ_COPY.states[V.state].word]
  for (const c of V.findings) { out.push(c.record.text, c.do.text); if (c.kind) out.push(HQ_COPY.kinds[c.kind]); if (c.do.prompt) out.push(c.do.prompt); if (c.said) out.push(c.said.when) }
  for (const h of V.holdsUp) out.push(h.record.text, h.when)
  for (const q of V.questions) out.push(q.label, q.answer)
  return out
}

/* ── zero ────────────────────────────────────────────────────────────── */

test('zero: nothing reported is not Proven — Recorded, headline/none, no cards', () => {
  const V = buildVerdict(session())
  assert.equal(V.state, 'couldnt')
  assert.equal(V.headline, 'Claude reported nothing as done in this session.')
  assert.deepEqual(V.findings, [])
  assert.deepEqual(V.holdsUp, [])
  /* commit G p3-wording: `failed_commands` added a failed command exit and a failed tool call up as commands; each kind is its own count now. */
  assert.deepEqual(V.counts, { reported: 0, proven: 0, not_proven: 0, couldnt: 0, fog: 0, loose: 0, failed_exits: 0, failed_tool_calls: 0 })
  assert.deepEqual(V.questions.map((q) => q.answer), ['0 reported · 0 proven', 'None', "Recorded", 'Nothing to do'])
  assert.equal(V.treeMoved, false)
  assert.equal(V.clicked('nope'), null)
  assert.match(V.exportMarkdown(), /^# ThruLine verdict — Recorded\n/)
})

test('zero with degraded capture is Fog with one recording-gap card and no Said row', () => {
  const V = buildVerdict(session({ capture: { degraded: true, condition: 'degraded' } }))
  assert.equal(V.state, 'fog')
  assert.equal(V.findings.length, 1)
  assert.equal(V.findings[0].kind, 'recording_gap')
  assert.equal(V.findings[0].state, 'fog')
  assert.equal(V.findings[0].said, null)
})

/* ── one ─────────────────────────────────────────────────────────────── */

test('one: a single unsupported claim with no findings is Not proven · Said done, no proof, with a prompt', () => {
  const V = buildVerdict(session({ claims: [claim(1)] }))
  assert.equal(V.state, 'not_proven')
  assert.equal(V.findings.length, 1)
  const card = V.findings[0]
  assert.equal(card.kind, 'said_done_no_proof')
  assert.equal(card.said.quote, 'Thing 1 is done.')
  assert.equal(card.said.when, 'a point in this session')
  assert.equal(card.record.text, 'Not proven: nothing in the record shows this was done.')
  assert.deepEqual(card.record.refs, ['clm'])
  assert.equal(card.record.role, 'grounded')
  assert.equal(card.do.role, 'inference')
  assert.match(card.do.prompt, /paste the full output/)
  assert.equal(card.proofNode, 'clm')
  assert.equal(V.headline, 'Claude said this session finished. Of what it reported done, the record backs 0 of 1; 1 has no proof.')
  assert.equal(V.questions[3].answer, '1 step')
  const md = V.exportMarkdown()
  assert.match(md, /### Not proven · Said done, no proof/)
  /* The Said line is the family's said template through the engine (commit G, p2-quotations), no longer the raw quote
   * with "— when" after it; the export writes that line, manager register, with its chip. */
  assert.match(md, /- Said: The AI said this was done at a point in this session: "Thing 1 is done\." \[clm\]/)
  assert.match(md, /- Record: Not proven: nothing in the record shows this was done\. \[clm\]/)
})

test('one proven claim is Proven, holds up with its check refs, and has no card', () => {
  const V = buildVerdict(session({ claims: [claim(1, { verdict: 'supported', evidence: ['k1'] })], checks: [{ id: 'k1', node: 'chk', exit: 0, inputTree: 'T2' }] }))
  assert.equal(V.state, 'proven')
  assert.deepEqual(V.findings, [])
  assert.equal(V.holdsUp.length, 1)
  assert.deepEqual(V.holdsUp[0].refs, ['clm', 'chk'])
  assert.equal(V.holdsUp[0].record.role, 'grounded')
  assert.equal(V.headline, 'Claude said this session finished. Of what it reported done, the record backs 1 of 1.')
  assert.equal(V.treeMoved, false)
  assert.equal(V.questions[2].answer, 'Yes')
})

/* ── kind mapping ────────────────────────────────────────────────────── */

test('kind mapping: every detector lands on the plan §1.3 kind', () => {
  const degraded = false
  for (const det of ['test_run_failed', 'reexecution_failed']) {
    const c = claim(1, { findings: [finding(1, det)] })
    assert.equal(claimKind(c, claimState(c, degraded)), 'command_failed_then_done', det)
  }
  const other = claim(1, { findings: [finding(1, 'some_other_check', 'fail')] })
  assert.equal(claimKind(other, 'not_proven'), 'record_says_otherwise')
  const warnOnly = claim(1, { findings: [finding(1, 'some_other_check', 'warn')] })
  assert.equal(claimKind(warnOnly, 'not_proven'), 'said_done_no_proof')
  assert.equal(claimKind(claim(1, { verdict: 'unverifiable' }), 'couldnt'), 'couldnt_check')
  assert.equal(claimKind(claim(1, { verdict: 'unverifiable' }), 'fog'), null)
  assert.equal(claimKind(claim(1, { verdict: 'supported' }), 'proven'), null)

  for (const det of ['assertion_removed', 'assertion_weakened', 'skip_marker_added', 'test_deleted', 'narrowed_run', 'tests_ran_before_final_edit']) {
    for (const [severity, state] of [['fail', 'not_proven'], ['warn', 'loose']]) {
      const V = buildVerdict(session({ claims: [claim(1, { verdict: 'supported', findings: [finding(1, det, severity, 'tests/t.rs')] })] }))
      const card = V.findings.find((c) => c.kind === 'tests_weakened')
      assert.ok(card, `${det} ${severity} produces a Tests weakened card`)
      assert.equal(card.state, state, `${det} ${severity}`)
      assert.equal(card.proofNode, `f_${det}_1`)
      assert.equal(card.blast, 1)
      assert.ok(card.record.refs.includes(`f_${det}_1`))
      assert.equal(V.state, state)
    }
  }
  const dropped = buildVerdict(session({ promises: [{ id: 'p1', node: 'prm', kind: 'follow_up', status: 'dropped', text: 'I will add tests later.' }] }))
  assert.equal(dropped.findings[0].kind, 'left_behind')
  assert.equal(dropped.findings[0].state, 'loose')
  assert.equal(dropped.state, 'loose')
  const open = buildVerdict(session({ promises: [{ id: 'p1', node: 'prm', kind: 'follow_up', status: 'open', text: 'Later.' }] }))
  assert.equal(open.findings[0].kind, null)
  assert.equal(open.findings[0].state, 'loose')
  const fulfilled = buildVerdict(session({ promises: [{ id: 'p1', node: 'prm', kind: 'follow_up', status: 'fulfilled', text: 'Later.' }] }))
  assert.deepEqual(fulfilled.findings, [])
})

/* ── session state rule ──────────────────────────────────────────────── */

test('session state: Not proven > Loose thread > Recorded > Fog > Proven', () => {
  const np = claim(1)
  const cc = claim(2, { verdict: 'unverifiable' })
  const ok = claim(3, { verdict: 'supported' })
  const loose = { id: 'p', node: 'prm', kind: 'follow_up', status: 'dropped', text: 'x' }
  assert.equal(buildVerdict(session({ claims: [ok, cc, np], promises: [loose] })).state, 'not_proven')
  assert.equal(buildVerdict(session({ claims: [ok, cc], promises: [loose] })).state, 'loose')
  assert.equal(buildVerdict(session({ claims: [ok, cc] })).state, 'couldnt')
  assert.equal(buildVerdict(session({ claims: [ok] })).state, 'proven')
  const fog = buildVerdict(session({ claims: [ok, cc], capture: { degraded: true } }))
  assert.equal(fog.state, 'fog', 'unverifiable under degraded capture is fog, not couldnt')
  assert.equal(fog.counts.fog, 1)
  assert.equal(fog.counts.couldnt, 0)
  assert.equal(buildVerdict(session({ claims: [ok, np], capture: { degraded: true } })).state, 'not_proven', 'fog never hides a Not proven')
})

/* ── N mixed, ranking ────────────────────────────────────────────────── */

test('N: ranking is tier, then blast radius, then recency, then input order', () => {
  const S = session({
    events: EVENTS,
    claims: [
      claim(1, { verdict: 'unverifiable' }),
      claim(2, { findings: [finding(2, 'test_run_failed', 'fail', 'src/a.rs')] }),
      claim(3, { findings: [finding(3, 'x', 'fail', 'src/a.rs'), finding(3, 'y', 'fail', 'src/b.rs')] }),
      claim(4, { verdict: 'supported', evidence: ['k1'] }),
      claim(5, { seq: 40 }),
      claim(6, { seq: 30 }),
      claim(7),
    ],
    promises: [{ id: 'p1', node: 'prm', kind: 'follow_up', status: 'dropped', text: 'Add a test.' }],
    checks: [{ id: 'k1', node: 'chk', exit: 0, inputTree: 'T1' }],
    capture: { degraded: true, condition: 'degraded' },
  })
  const V = buildVerdict(S)
  assert.equal(V.state, 'not_proven')
  const order = V.findings.map((c) => `${c.state}:${c.id}`)
  assert.deepEqual(order, [
    'not_proven:c3',   // blast 2
    'not_proven:c2',   // blast 1
    'not_proven:c5',   // blast 0, seq 40
    'not_proven:c6',   // blast 0, seq 30
    'not_proven:c7',   // blast 0, no seq, input order after c5/c6 (both have seq)
    'loose:p1',
    'fog:c1',          // unverifiable under degraded capture
    'fog:recording_gap',
  ])
  assert.equal(V.findings[1].kind, 'command_failed_then_done')
  assert.equal(V.findings[0].kind, 'record_says_otherwise')
  /* commit G p3-wording: EVENTS holds one failed command exit (seq 5) and one failed tool call (seq 14), each its own count. */
  assert.deepEqual(V.counts, { reported: 7, proven: 1, not_proven: 5, couldnt: 0, fog: 1, loose: 1, failed_exits: 1, failed_tool_calls: 1 })
  assert.deepEqual(V.checked, { events: 6, claims: 7, checks: 1, trees: 2, verifiers: 1, derivedRefs: [] })
  assert.deepEqual(V.headlineRefs, ['clm', 'clm2', 'clm3', 'clm4', 'clm5', 'clm6', 'clm7', 'prm'], 'the headline counts every claim, then every loose promise')
  assert.equal(V.treeMoved, true, 'the cited check ran on T1, the final tree is T2')
  /* commit G p3-wording: the answer was "2" and the clause "2 commands failed" for one exec row and one tool result the record does not link. */
  assert.deepEqual(V.questions.map((q) => q.answer), ['7 reported · 1 proven', '1 failed command exit and 1 failed tool call', 'No — changed after the last check', '8 steps'])
  assert.match(V.headline, /^Claude said this session finished\. Of what it reported done, the record backs 1 of 7; 5 have no proof\. The record shows 1 failed command exit and 1 failed tool call with no successful re-run afterwards\. 1 thing it said it would do was left behind\.$/)
  for (const c of V.findings) assert.equal(c.record.violations.length, 0, `${c.id}: ${JSON.stringify(c.record.violations)}`)
})

test('failed commands: a later successful run of the same tool answers "later passed"', () => {
  const later = [...EVENTS, { seq: 20, kind: 'exec', text: 'cargo test → exit 0' }, { seq: 21, kind: 'tool_result', text: 'Bash ok' }]
  const V = buildVerdict(session({ events: later, claims: [claim(1)] }))
  /* commit G p3-wording: the two failed rows are one exec and one tool result, named by kind rather than added up as "2".
   * The headline check was a pattern on the old clause ("commands failed"), which can no longer render and so could no
   * longer fail; it is now the exact headline, with no failure clause. */
  assert.deepEqual([V.counts.failed_exits, V.counts.failed_tool_calls], [1, 1])
  assert.equal(V.questions[1].answer, '1 failed command exit and 1 failed tool call, later passed')
  assert.deepEqual(V.questions[1].refs, ['ev5', 'ev14'])
  assert.equal(V.headline, 'Claude said this session finished. Of what it reported done, the record backs 0 of 1; 1 has no proof.', 'recovered failures do not reach the headline')
  const partial = [...EVENTS, { seq: 20, kind: 'exec', text: 'cargo test → exit 0' }]
  assert.equal(buildVerdict(session({ events: partial, claims: [claim(1)] })).questions[1].answer, '1 failed command exit and 1 failed tool call')
})

test('treeMoved: only cited checks count, and no checks means Recorded', () => {
  const cited = session({ claims: [claim(1, { verdict: 'supported', evidence: ['k1'] })], checks: [{ id: 'k1', node: 'chk', exit: 0, inputTree: 'T1' }, { id: 'k2', node: 'chk2', exit: 0, inputTree: 'T2' }] })
  assert.equal(buildVerdict(cited).treeMoved, true)
  const uncited = session({ claims: [claim(1, { verdict: 'supported', evidence: ['k2'] })], checks: [{ id: 'k1', node: 'chk', exit: 0, inputTree: 'T1' }, { id: 'k2', node: 'chk2', exit: 0, inputTree: 'T2' }] })
  assert.equal(buildVerdict(uncited).treeMoved, false)
  assert.equal(buildVerdict(uncited).questions[2].answer, 'Yes')
  const none = buildVerdict(session({ claims: [claim(1)] }))
  assert.equal(none.treeMoved, false)
  assert.equal(none.questions[2].answer, "Recorded")
  const noTree = buildVerdict(session({ finalTree: undefined, claims: [claim(1, { evidence: ['k1'] })], checks: [{ id: 'k1', node: 'chk', exit: 0, inputTree: 'T1' }] }))
  assert.equal(noTree.treeMoved, false)
  assert.equal(noTree.questions[2].answer, "Recorded")
})

/* ── clicked ─────────────────────────────────────────────────────────── */

test('clicked: a template per node type, null for a type the copy does not name', () => {
  const S = session({
    claims: [claim(1)],
    nodes: [
      { id: 'ev5', objType: 'event', kind: 'exec', seq: 5, title: 'cargo test', lines: ['cargo test → exit 1'] },
      { id: 'ev7', objType: 'event', kind: 'note', seq: 7, title: 'note', lines: [] },
      { id: 'clm', objType: 'claim', title: '“Thing 1 is done.”', lines: [] },
      { id: 'ev8', objType: 'event', kind: 'compaction', seq: 8, title: 'x', lines: [] },
    ],
  })
  const V = buildVerdict(S)
  assert.equal(V.clicked('ev5').title, 'You clicked: a command the AI ran at step 5.')
  assert.match(V.clicked('ev5').body.text, /cargo test → exit 1/)
  assert.equal(V.clicked('ev7').title, 'You clicked: a note the AI wrote at step 7.')
  assert.match(V.clicked('clm').body.text, /"Thing 1 is done\."/)
  assert.equal(V.clicked('ev8'), null)
  assert.equal(V.clicked('ev5').link, 'Show raw record →')
  assert.deepEqual(V.clicked('ev5').refs, ['ev5'])
})

/* ── registers ───────────────────────────────────────────────────────── */

test('operator register: same facts, terse prose, paths allowed; export is always the manager register', () => {
  const S = session({ events: EVENTS, claims: [claim(1, { findings: [finding(1, 'assertion_removed', 'fail', 'tests/t.rs')] })] })
  const M = buildVerdict(S, { register: 'manager' })
  const O = buildVerdict(S, { register: 'operator' })
  assert.equal(M.state, O.state)
  assert.deepEqual(M.counts, O.counts)
  assert.deepEqual(M.findings.map((c) => c.id), O.findings.map((c) => c.id))
  const weakO = O.findings.find((c) => c.kind === 'tests_weakened')
  const weakM = M.findings.find((c) => c.kind === 'tests_weakened')
  assert.match(weakO.record.text, /tests\/t\.rs/)
  assert.doesNotMatch(weakM.record.text, /tests\/t\.rs/)
  assert.equal(O.exportMarkdown(), M.exportMarkdown())
})

/* ── huge ────────────────────────────────────────────────────────────── */

test('huge: 500 claims rank deterministically and the first five are the most serious', () => {
  const claims = []
  for (let i = 1; i <= 500; i++) {
    const verdict = i % 5 === 0 ? 'supported' : i % 7 === 0 ? 'unverifiable' : 'unsupported'
    const findings = i % 3 === 0 ? [finding(i, 'test_run_failed', 'fail', `src/${i % 11}.rs`), finding(i, 'z', 'fail', `src/${i % 13}.rs`)] : []
    claims.push(claim(i, { verdict, findings, seq: i % 2 ? i : null }))
  }
  const S = session({ events: EVENTS, claims })
  const t0 = Date.now()
  const A = buildVerdict(S)
  const elapsed = Date.now() - t0
  const B = buildVerdict(S)
  assert.deepEqual(A.findings.map((c) => c.id), B.findings.map((c) => c.id), 'same input, same order')
  assert.equal(A.findings.length, 500 - 100)
  assert.equal(A.counts.reported, 500)
  assert.equal(A.counts.proven, 100)
  const top = A.findings.slice(0, 5)
  for (const c of top) assert.equal(c.state, 'not_proven')
  for (let i = 1; i < A.findings.length; i++) {
    const a = A.findings[i - 1], b = A.findings[i]
    assert.ok(a.tier < b.tier || (a.tier === b.tier && (a.blast > b.blast || (a.blast === b.blast && (a.seq ?? -1) >= (b.seq ?? -1)))), `rank order broken at ${i}: ${a.id} before ${b.id}`)
  }
  assert.ok(elapsed < 2000, `500 claims took ${elapsed}ms`)
  assert.ok(A.exportMarkdown().length > 10000)
})

/* ── receipts for the header (ADJUDICATIONS item D findings 2 and 3) ─── */

test('derivedRefs names the verifier-synthesised claims; headlineRefs is empty only when the headline counts nothing', () => {
  const synthesised = claim(1, { verdict: 'unverifiable', verifier: 'unclaimed_completion@1', text: 'unfinished work' })
  const V = buildVerdict(session({ claims: [synthesised, claim(2, { verifier: 'tests_pass@1' })] }))
  assert.deepEqual(V.checked.derivedRefs, ['clm'], 'only the unclaimed_completion claim is derived')
  assert.equal(V.checked.claims, 2, 'the model counts every adapted claim')
  assert.equal(V.counts.reported, 2)
  assert.deepEqual(V.headlineRefs, ['clm', 'clm2'])
  const zero = buildVerdict(session())
  assert.deepEqual(zero.headlineRefs, [])
  assert.doesNotMatch(zero.headline, /\d/, 'a headline with no receipt carries no number')
  const loose = buildVerdict(session({ promises: [{ id: 'p1', node: 'prm', kind: 'follow_up', status: 'open', text: 'Later.' }] }))
  assert.deepEqual(loose.headlineRefs, ['prm'])
  assert.match(loose.headline, /1 thing it said it would do/)
  assert.equal(buildVerdict(session({ harness: undefined })).headline, `${HQ_COPY.chrome.agent_fallback} reported nothing as done in this session.`, 'the agent fallback is copy, not a literal')
})

/* ── headline number agreement (ADJUDICATIONS item D third ruling, finding 4) ── */

/** Every place a headline number disagrees with the word it counts: "1 claims", "2 claim", "1 thing … were". */
const SINGULAR_WORDS = new Set(['claim', 'thing', 'promise', 'command', 'check', 'finding', 'statement', 'has', 'was', 'is'])
const S_ENDING_SINGULARS = new Set(['has', 'was', 'is', 'its', 'this'])
const PLURAL_VERBS = new Set(['have', 'were', 'are'])
function disagreements(text) {
  const out = []
  for (const m of text.matchAll(/\b(\d+)\s+(\p{L}+)/gu)) {
    const n = Number(m[1])
    const w = m[2].toLowerCase()
    if (n === 1 && ((/s$/.test(w) && !S_ENDING_SINGULARS.has(w)) || PLURAL_VERBS.has(w))) out.push(m[0])
    if (n !== 1 && SINGULAR_WORDS.has(w)) out.push(m[0])
  }
  for (const m of text.matchAll(/\b(\d+)\s+(things?|promises?|commands?)\b[^.;]*?\b(was|were)\b/gu)) {
    const one = Number(m[1]) === 1
    if (one !== (m[3] === 'was')) out.push(m[0])
  }
  /* commit G p3-wording: a failure clause puts "failed" between the number and its noun ("2 failed command exits"), which
   * the first loop never reads as a noun. */
  for (const m of text.matchAll(/\b(\d+)\s+failed\s+(command exit|tool call)(s?)\b/gu)) {
    if ((Number(m[1]) === 1) === (m[3] === 's')) out.push(m[0])
  }
  return out
}

test('headline numbers agree with their words in both registers, at reported = 0, 1 and 2', () => {
  /* The guard is live: the third ruling's two live headlines, and a verb at a distance. */
  assert.deepEqual(disagreements('Claude Code claimed completion. Record backs 0/1 claims; 3 promises left behind.'), ['1 claims'])
  assert.deepEqual(disagreements('Claude said this session finished. The record backs 0 of 1 things it reported done; 1 has no proof.'), ['1 things'])
  assert.equal(disagreements('and 1 thing it said it would do were left behind.').length, 1)
  assert.equal(disagreements('2 promise left behind. 2 has no proof.').length, 2)
  assert.deepEqual(disagreements('the record backs 0 of 1; 1 have no proof.'), ['1 have'], 'a /one variant not selected')
  assert.deepEqual(disagreements('Of what it reported done, the record backs 0 of 1; 1 has no proof. 2 commands failed.'), [])
  assert.deepEqual(disagreements('The record shows 2 failed command exit and 1 failed tool calls with no successful re-run afterwards.'), ['2 failed command exit', '1 failed tool calls'], 'a failure noun after "failed"')
  assert.deepEqual(disagreements('The record shows 1 failed command exit and 2 failed tool calls with no successful re-run afterwards.'), [])

  const ok = (i) => claim(i, { verdict: 'supported' })
  const np = (i) => claim(i)
  const cc = (i) => claim(i, { verdict: 'unverifiable' })
  const promise = (i) => ({ id: `p${i}`, node: `prm${i}`, kind: 'follow_up', status: 'dropped', text: `Later ${i}.` })
  const oneFailure = [{ seq: 5, kind: 'exec', text: 'cargo test → exit 1' }]
  const degraded = { degraded: true, condition: 'degraded' }
  const cases = [
    // reported = 0
    ['0 · nothing', {}],
    ['0 · one promise', { promises: [promise(1)] }],
    ['0 · two promises', { promises: [promise(1), promise(2)] }],
    ['0 · two promises, one failed command', { promises: [promise(1), promise(2)], events: oneFailure }],
    // reported = 1
    ['1 · proven', { claims: [ok(1)] }],
    ['1 · not proven', { claims: [np(1)] }],
    ['1 · couldnt', { claims: [cc(1)] }],
    ['1 · fog', { claims: [cc(1)], capture: degraded }],
    ['1 · proven, one promise', { claims: [ok(1)], promises: [promise(1)] }],
    ['1 · proven, two promises', { claims: [ok(1)], promises: [promise(1), promise(2)] }],
    ['1 · not proven, one failed command, one promise', { claims: [np(1)], events: oneFailure, promises: [promise(1)] }],
    ['1 · not proven, two failed commands, two promises', { claims: [np(1)], events: EVENTS, promises: [promise(1), promise(2)] }],
    ['1 · not proven, two failed command exits and two failed tool calls', { claims: [np(1)], events: [...EVENTS, { seq: 30, kind: 'exec', text: 'cargo build → exit 1' }, { seq: 31, kind: 'tool_result', text: 'Bash FAILED' }] }],
    // reported = 2
    ['2 · proven', { claims: [ok(1), ok(2)] }],
    ['2 · one not proven', { claims: [np(1), ok(2)] }],
    ['2 · two not proven', { claims: [np(1), np(2)] }],
    ['2 · not proven and couldnt', { claims: [np(1), cc(2)] }],
    ['2 · couldnt', { claims: [cc(1), cc(2)] }],
    ['2 · fog', { claims: [ok(1), cc(2)], capture: degraded }],
    ['2 · proven, two promises', { claims: [ok(1), ok(2)], promises: [promise(1), promise(2)] }],
  ]
  const seen = new Set()
  for (const register of ['manager', 'operator']) {
    for (const [label, over] of cases) {
      const V = buildVerdict(session(over), { register })
      seen.add(V.state)
      assert.deepEqual(disagreements(V.headline), [], `[${register}] ${label}: ${V.headline}`)
      assert.doesNotMatch(V.headline, /\bthings it reported\b/, `[${register}] ${label}: Plain counts what was reported, not "things"`)
    }
  }
  assert.deepEqual([...seen].sort(), ['couldnt', 'fog', 'loose', 'not_proven', 'proven'], 'every session state was rendered')
})

/* ── the copy is the only source of words ────────────────────────────── */

test('the copy matcher is a guard: it rejects prose that is not in the copy', () => {
  assert.equal(fromCopy('The agent lied about the tests.'), false)
  assert.equal(fromCopy('Not proven: nothing in the record shows this was done. And then some invented words.'), false)
  assert.equal(fromCopy('Not proven: nothing in the record shows this was done.'), true)
  /* commit G p3-wording: the headline's failure clause is now `headline/clause/failed` over `chrome.failed`. */
  assert.equal(fromCopy('Claude said this session finished. Of what it reported done, the record backs 1 of 2; 1 has no proof. The record shows 3 failed command exits and 2 failed tool calls with no successful re-run afterwards.'), true)
  assert.equal(fromCopy('42'), true, 'a bare count is a number, not prose')
  assert.equal(fromCopy('42 things'), false)
  /* commit G p1p2-followups: a quotation that ends its own sentence renders without the template's stop after it, and the
   * matcher reads that shape; a quotation that did not end its sentence and lost the stop is still not from the copy. */
  const TAIL = ' Is that done? If it is, paste the change; if not, do it now and paste what you changed.'
  assert.equal(fromCopy(`Earlier you said "Add a test."${TAIL}`), true)
  assert.equal(fromCopy(`Earlier you said "Add a test".${TAIL}`), true)
  assert.equal(fromCopy(`Earlier you said "Add a test"${TAIL}`), false, 'the stop may give way only to a quotation that ended its sentence')
})

test('no rendered string comes from outside HQ_COPY, in either register', () => {
  const S = session({
    events: EVENTS,
    claims: [claim(1, { findings: [finding(1, 'test_run_failed', 'fail', 'src/a.rs')], evidence: ['k1'] }), claim(2, { verdict: 'supported', evidence: ['k1'] }), claim(3, { verdict: 'unverifiable', reasons: ['no lint run recorded'] }), claim(4, { findings: [finding(4, 'narrowed_run', 'warn', 'tests/t.rs')] })],
    promises: [{ id: 'p1', node: 'prm', kind: 'follow_up', status: 'dropped', text: 'Add a test.' }, { id: 'p2', node: 'prm2', kind: 'follow_up', status: 'open', text: 'Later.' }],
    checks: [{ id: 'k1', node: 'chk', exit: 1, inputTree: 'T1' }],
    capture: { degraded: true, condition: 'degraded' },
  })
  for (const register of ['manager', 'operator']) {
    const V = buildVerdict(S, { register })
    for (const s of renderedStrings(V)) assert.ok(fromCopy(s), `[${register}] not from the copy: ${JSON.stringify(s)}`)
    for (const c of V.findings) assert.equal(c.record.violations.length, 0, `${register} ${c.id}: ${JSON.stringify(c.record.violations)}`)
  }
})

test('banned tokens never appear in rendered prose, even when the record quotes one', () => {
  const S = session({
    events: EVENTS,
    claims: [claim(1, { text: 'I did not lie, the fake tests pass.', findings: [finding(1, 'test_run_failed', 'fail', 'src/a.rs')] }), claim(2, { verdict: 'unverifiable', reasons: ['lazy evaluation not recorded'] })],
    promises: [{ id: 'p1', node: 'prm', kind: 'follow_up', status: 'dropped', text: 'I was lying about this.' }],
  })
  for (const register of ['manager', 'operator']) {
    const V = buildVerdict(S, { register })
    const prose = [V.headline, ...V.findings.flatMap((c) => [c.record.text, c.do.text]), ...V.questions.map((q) => q.answer)]
    for (const s of prose) assert.equal(findBannedToken(s), null, `[${register}] ${s}`)
    for (const c of V.findings) assert.equal(c.record.violations.length, 0, `${register} ${c.id}: ${JSON.stringify(c.record.violations)}`)
  }
})

/* ── a bracket inside a slot value is prose, never a ref (STEP0 audit §1.2 [C]) ── */

test('a bracket inside a slot value changes neither the family nor the refs', () => {
  const op = (S) => buildVerdict(session(S), { register: 'operator' }).findings
  /* A test-weakening finding under a bracketed route directory. */
  const weak = (path) => op({ claims: [claim(1, { verdict: 'supported', seq: 3, findings: [finding(1, 'assertion_removed', 'fail', path)] })] })[0]
  const bracketed = weak('app/[slug]/page.test.tsx')
  assert.equal(bracketed.kind, 'tests_weakened')
  assert.equal(bracketed.record.key, 'verdict/tests_weakened/not_proven')
  assert.deepEqual(bracketed.record.refs, ['f_assertion_removed_1', 'clm'])
  assert.equal(bracketed.record.text, 'Not proven: the record shows the tests weakened — assertion_removed detail in app/[slug]/page.test.tsx.')
  assert.deepEqual(bracketed.record.violations, [])
  assert.deepEqual(bracketed.record.fallbacks, [])
  const plain = weak('app/slug/page.test.tsx')
  assert.equal(bracketed.record.key, plain.record.key, 'same family as the same path without brackets')
  assert.deepEqual(bracketed.record.refs, plain.record.refs, 'same refs as the same path without brackets')

  /* A failing finding with no node: nothing may render "[undefined]". */
  const nodeless = op({ claims: [claim(2, { seq: 3, findings: [{ key: 'some_detector', severity: 'fail', detail: 'exit 1 on build' }] })] })[0]
  assert.equal(nodeless.kind, 'record_says_otherwise')
  assert.equal(nodeless.record.key, 'verdict/record_says_otherwise/not_proven')
  assert.deepEqual(nodeless.record.refs, ['clm2', 'ev3'])
  assert.equal(nodeless.record.text, 'Not proven: the record shows the opposite — exit 1 on build.')
  assert.deepEqual(nodeless.record.violations, [])
  assert.deepEqual(nodeless.record.fallbacks, [])

  /* A reason carrying a bracket. */
  const reasoned = (reason) => op({ claims: [claim(3, { seq: 3, reasons: [reason] })] })[0]
  const wip = reasoned('no check covers [WIP] item')
  assert.equal(wip.kind, 'said_done_no_proof')
  assert.equal(wip.record.key, 'verdict/said_done_no_proof/not_proven')
  assert.deepEqual(wip.record.refs, ['clm3', 'ev3'])
  assert.equal(wip.record.text, 'Not proven: nothing in the record shows this. No check covering it ran after the last edit — no check covers [WIP] item.')
  assert.deepEqual(wip.record.violations, [])
  assert.deepEqual(wip.record.fallbacks, [])
  const noWip = reasoned('no check covers WIP item')
  assert.equal(wip.record.key, noWip.record.key, 'same family as the same reason without brackets')
  assert.deepEqual(wip.record.refs, noWip.record.refs, 'same refs as the same reason without brackets')
})

const said = (r) => ({ key: r.key, text: r.text, refs: r.refs, role: r.role, violations: r.violations, fallbacks: r.fallbacks })

test('a bracketed banned word in a slot value is refused although the engine supplies the chips, and the fallback says why', () => {
  /* Fix pass 2, verifier finding 2: with bracket tokens cut out of the checked text (break VP07) this card rendered
   * "Not proven: the record shows the opposite — marked [lazy] by the runner." with no violation. */
  const card = buildVerdict(session({ claims: [claim(2, { seq: 3, findings: [{ key: 'some_detector', severity: 'fail', detail: 'marked [lazy] by the runner', node: 'f_b' }] })] }), { register: 'operator' }).findings[0]
  const lazy = [{ kind: 'banned_token', token: 'lazy', word: '[lazy]' }]
  assert.equal(card.kind, 'record_says_otherwise')
  assert.deepEqual(said(card.record), {
    key: 'verdict/*', text: 'The record was read for this claim.', refs: ['clm2', 'f_b', 'ev3'], role: 'grounded', violations: [],
    fallbacks: [{ key: 'verdict/record_says_otherwise/not_proven', violations: lazy }, { key: 'verdict/*/not_proven', violations: lazy }],
  })
})

test('a sentence the engine gives no chips reads a bracket in its text as prose too: a receipt-less Fog card keeps its family and holds no ref', () => {
  /* Fix pass 2, verifier finding 3: reading refs out of the text when no chip is supplied (break VP08) moved this card
   * to verdict/*. */
  const card = buildVerdict(session({ capture: { degraded: true, condition: 'degraded', degradedReason: 'recorder [redacted] not vouched' } }), { register: 'operator' }).findings[0]
  assert.equal(card.kind, 'recording_gap')
  assert.deepEqual(said(card.record), {
    key: 'verdict/recording_gap/fog', text: 'Fog: the record for this session is incomplete — recorder [redacted] not vouched. What happened in the gap cannot be settled.',
    refs: [], role: 'inference', violations: [], fallbacks: [],
  })
})

test('a Record sentence quotes a detail or a path that ends in dots as recorded', () => {
  /* Fix pass 2, verifier finding 1: detail "…after cd .." rendered "…after cd." and a path ".." was refused as empty. */
  const op = (S) => buildVerdict(session(S), { register: 'operator' }).findings[0]
  const cd = op({ claims: [claim(2, { seq: 3, findings: [{ key: 'some_detector', severity: 'fail', detail: 'exit 1 after cd ..', node: 'f_x' }] })] })
  assert.deepEqual(said(cd.record), { key: 'verdict/record_says_otherwise/not_proven', text: 'Not proven: the record shows the opposite — exit 1 after cd ..', refs: ['clm2', 'f_x', 'ev3'], role: 'grounded', violations: [], fallbacks: [] })
  const up = op({ claims: [claim(1, { verdict: 'supported', seq: 3, findings: [finding(1, 'assertion_removed', 'fail', '..')] })] })
  assert.equal(up.kind, 'tests_weakened')
  assert.deepEqual(said(up.record), { key: 'verdict/tests_weakened/not_proven', text: 'Not proven: the record shows the tests weakened — assertion_removed detail in ..', refs: ['f_assertion_removed_1', 'clm'], role: 'grounded', violations: [], fallbacks: [] })
  assert.deepEqual({ key: up.do.key, prompt: up.do.prompt, fallbacks: up.do.fallbacks }, {
    key: 'verdict/tests_weakened/not_proven',
    prompt: 'A test in .. changed in this session so that it checks less than it did. Restore what it checked, re-run the tests on the current code, and paste the full output including the command you ran.',
    fallbacks: [],
  })
})

test('a family fallback is never silent: every family passed over is reported with its violations', () => {
  /* Forced by a banned token in the reason: both {reason} families are refused. */
  const banned = buildVerdict(session({ claims: [claim(1, { verdict: 'unverifiable', reasons: ['lazy evaluation not recorded'] })] }), { register: 'operator' }).findings[0]
  assert.equal(banned.record.key, 'verdict/*')
  assert.equal(banned.record.text, 'The record was read for this claim.')
  assert.deepEqual(banned.record.violations, [], 'the sentence on screen is clean')
  assert.deepEqual(banned.record.fallbacks, [
    { key: 'verdict/couldnt_check/couldnt', violations: [{ kind: 'banned_token', token: 'lazy', word: 'lazy' }] },
    { key: 'verdict/*/couldnt', violations: [{ kind: 'banned_token', token: 'lazy', word: 'lazy' }] },
  ])
  /* Forced by an empty slot: a failing finding with no detail cannot fill "the opposite — {detail}." */
  const empty = buildVerdict(session({ claims: [claim(2, { seq: 3, findings: [{ key: 'some_detector', severity: 'fail', detail: '', node: 'f_x' }] })] }), { register: 'operator' }).findings[0]
  assert.equal(empty.kind, 'record_says_otherwise')
  assert.equal(empty.record.key, 'verdict/*/not_proven')
  assert.equal(empty.record.text, 'Not proven: nothing in the record shows this — no reason recorded.')
  assert.deepEqual(empty.record.refs, ['clm2', 'f_x', 'ev3'])
  assert.deepEqual(empty.record.violations, [])
  assert.deepEqual(empty.record.fallbacks, [{ key: 'verdict/record_says_otherwise/not_proven', violations: [{ kind: 'empty_slot', slot: 'detail' }] }])
})

test('a Do prompt that cannot fill takes its Do line down the chain with it, and says so: no hole in a copy-prompt', () => {
  /* Fix pass 1, verifier finding 2: a narrowed-run card with no path rendered the prompt "A test in  changed…". */
  const noPath = session({ claims: [claim(1, { verdict: 'supported', seq: 3, findings: [finding(1, 'narrowed_run', 'fail')] })] })
  const card = buildVerdict(noPath, { register: 'operator' }).findings[0]
  assert.equal(card.kind, 'tests_weakened')
  assert.equal(card.record.key, 'verdict/*/not_proven')
  assert.deepEqual(card.record.fallbacks, [{ key: 'verdict/tests_weakened/not_proven', violations: [{ kind: 'empty_slot', slot: 'path' }] }])
  assert.equal(card.do.key, 'verdict/*/not_proven')
  assert.equal(card.do.text, 'What would prove it: a captured run that covers this claim on the final tree.')
  assert.equal(card.do.prompt, 'Re-run the check that covers "Thing 1 is done." on the current code and paste the full output, including the command you ran.')
  assert.deepEqual(card.do.violations, [])
  assert.deepEqual(card.do.fallbacks, [{ key: 'verdict/tests_weakened/not_proven', violations: [{ kind: 'empty_slot', slot: 'path', part: 'prompt' }] }])

  /* With a path, both lines keep the specific family. */
  const withPath = buildVerdict(session({ claims: [claim(1, { verdict: 'supported', seq: 3, findings: [finding(1, 'narrowed_run', 'fail', 'tests/test_rate.py')] })] }), { register: 'operator' }).findings[0]
  assert.equal(withPath.do.key, 'verdict/tests_weakened/not_proven')
  assert.equal(withPath.do.text, 'Read the test change before accepting the run.')
  assert.equal(withPath.do.prompt, 'A test in tests/test_rate.py changed in this session so that it checks less than it did. Restore what it checked, re-run the tests on the current code, and paste the full output including the command you ran.')
  assert.deepEqual(withPath.do.fallbacks, [])

  /* Plain names no path: its prompt fills, so its Do line stays on the specific family. */
  const plain = buildVerdict(noPath, { register: 'manager' }).findings[0]
  assert.equal(plain.do.key, 'verdict/tests_weakened/not_proven')
  assert.equal(plain.do.prompt, 'A test changed in this session so that it checks less than it did. Restore what it checked, re-run the tests on the current code, and paste the full output including the command you ran.')
  assert.deepEqual(plain.do.fallbacks, [])
})

test('every family placeholder the model must fill is one the model supplies', () => {
  /* No `check_ref`: a ref reaches a sentence only as a chip the engine appends, never through a slot. */
  const supplied = ['claim_quote', 'claim_kind', 'when', 'reason', 'detector', 'detector_plain', 'detail', 'path', 'counts']
  for (const [key, regs] of Object.entries(HQ_COPY.families)) {
    for (const reg of ['manager', 'operator']) for (const [part, text] of Object.entries(regs[reg])) {
      for (const p of placeholders(text)) assert.ok(supplied.includes(p), `${key}.${reg}.${part} uses {${p}}`)
    }
  }
})

/* ── quotations: the Said row and the clicked body (STEP0 audit §1.2, §3.4; commit G, p2-quotations) ──
 * The Said row rendered `card.said.quote` raw, and the clicked body filled a claim's text and the user's own prompt
 * into a sentence the panel styled as grounded; ground() ran on neither. A quotation is someone's recorded words: a
 * part of its own, verbatim, read by no ThruLine rule and licensing nothing, shown only when it is the text of the
 * record its ref resolves to. The words around it are ThruLine's, and are grounded like any sentence. */

const PROBE = 'The agent lied; all 14 tests pass [c9] {n}'
/* EXPORT-MD-ESCAPE-R4 (D-050): the export writes recorded text through `escapeMarkdown`, which escapes every bracket (D-246), so
 * the line it writes for the probe is `\[c9\]`, not `[c9]`. The old expected value was `${PROBE}` itself. What a reader sees is the same. */
const PROBE_MD = 'The agent lied; all 14 tests pass \\[c9\\] {n}'
const quoted = (r) => ({ key: r.key, text: r.text, parts: r.parts, quote: r.quote, role: r.role, refs: r.refs, violations: r.violations, fallbacks: r.fallbacks })
const saidLines = (md) => md.split('\n').filter((l) => l.startsWith('- Said: '))

test('Said: the template\'s own words are grounded with the chip of the record it quotes, and the claim or promise text is a quotation part, verbatim', () => {
  const S = session({ claims: [claim(1, { text: PROBE, seq: 3 })] })
  const manager = buildVerdict(S).findings[0].said
  assert.deepEqual(quoted(manager), {
    key: 'verdict/said_done_no_proof/not_proven', text: `The AI said this was done at step 3: "${PROBE}"`,
    parts: [{ text: 'The AI said this was done at step 3: ' }, { quote: PROBE, speaker: 'agent' }],
    quote: PROBE, role: 'grounded', refs: ['clm'], violations: [], fallbacks: [],
  })
  assert.equal(manager.when, 'step 3')
  const operator = buildVerdict(S, { register: 'operator' }).findings[0].said
  assert.deepEqual(quoted(operator), {
    key: 'verdict/said_done_no_proof/not_proven', text: `The agent wrote: "${PROBE}" — step 3`,
    parts: [{ text: 'The agent wrote: ' }, { quote: PROBE, speaker: 'agent' }, { text: ' — step 3' }],
    quote: PROBE, role: 'grounded', refs: ['clm'], violations: [], fallbacks: [],
  })
  /* The export writes the line the panel shows, with its chip, in the manager register. */
  assert.deepEqual(saidLines(buildVerdict(S, { register: 'operator' }).exportMarkdown()), [`- Said: The AI said this was done at step 3: "${PROBE_MD}" [clm]`])
  const promise = buildVerdict(session({ promises: [{ id: 'p1', node: 'prm', kind: 'follow_up', status: 'dropped', text: PROBE }] })).findings[0].said
  assert.deepEqual(quoted(promise), {
    key: 'verdict/left_behind/loose', text: `The AI said it would come back to this at a point in this session: "${PROBE}"`,
    parts: [{ text: 'The AI said it would come back to this at a point in this session: ' }, { quote: PROBE, speaker: 'agent' }],
    quote: PROBE, role: 'grounded', refs: ['prm'], violations: [], fallbacks: [],
  })
})

test('Said: a ref that resolves to no record falls back to a template that does not quote, with no chip, and says why', () => {
  const S = session({ claims: [claim(1, { node: undefined, text: PROBE, seq: 3 })] })
  const unresolved = (key) => ({ key, violations: [{ kind: 'quote_ref_unresolved', slot: 'claim_quote', ref: null }] })
  /* commit G p1p2-followups: every Plain said template now quotes (p2 coder loose thread 1), so Plain, like Technical,
   * reaches the Said line that quotes nothing; that line no longer points at a record it has no chip for (p2 F5). The
   * exact values change for those two reasons: Plain was 'verdict/*' "The AI reported task complete at step 3." with one
   * fallback, and Technical read "…; see the record.". */
  assert.deepEqual(quoted(buildVerdict(S).findings[0].said), {
    key: 'chrome.said_unquoted', text: 'The AI wrote about this at step 3; its words are not shown here.', parts: [{ text: 'The AI wrote about this at step 3; its words are not shown here.' }],
    quote: null, role: 'inference', refs: [], violations: [],
    fallbacks: ['verdict/said_done_no_proof/not_proven', 'verdict/*/not_proven', 'verdict/*'].map(unresolved),
  })
  assert.deepEqual(quoted(buildVerdict(S, { register: 'operator' }).findings[0].said), {
    key: 'chrome.said_unquoted', text: 'The agent wrote about this at step 3; its words are not shown here.', parts: [{ text: 'The agent wrote about this at step 3; its words are not shown here.' }],
    quote: null, role: 'inference', refs: [], violations: [],
    fallbacks: ['verdict/said_done_no_proof/not_proven', 'verdict/*/not_proven', 'verdict/*'].map(unresolved),
  })
  const md = buildVerdict(S).exportMarkdown()
  assert.deepEqual(saidLines(md), ['- Said: The AI wrote about this at step 3; its words are not shown here.'])
  assert.equal(md.includes(PROBE), false)
})

test('Said: a quote that is not the text of the record its ref resolves to is never shown, and the line says why', () => {
  /* Two claims share one ref. The chip opens the first; the second claim's words are not that record's words, so they
   * are not quoted under that chip. */
  const S = session({ claims: [claim(1, { text: 'Thing 1 is done.', seq: 2 }), claim(2, { node: 'clm', text: PROBE, seq: 3 })] })
  const mismatch = (key) => ({ key, violations: [{ kind: 'quote_mismatch', slot: 'claim_quote', ref: 'clm' }] })
  const [second, first] = buildVerdict(S).findings
  assert.deepEqual([second.id, first.id], ['c2', 'c1'], 'ranked by recency')
  // commit G p1p2-followups (p2 coder loose thread 1, p2 F5): this was the verdict/*/not_proven family's "The AI reported
  // task complete at step 3." with one fallback; every Plain said template now quotes, so Plain reaches the Said line that
  // quotes nothing.
  assert.deepEqual(quoted(second.said), {
    key: 'chrome.said_unquoted', text: 'The AI wrote about this at step 3; its words are not shown here.', parts: [{ text: 'The AI wrote about this at step 3; its words are not shown here.' }],
    quote: null, role: 'grounded', refs: ['clm'], violations: [],
    fallbacks: ['verdict/said_done_no_proof/not_proven', 'verdict/*/not_proven', 'verdict/*'].map(mismatch),
  })
  assert.deepEqual(quoted(first.said), {
    key: 'verdict/said_done_no_proof/not_proven', text: 'The AI said this was done at step 2: "Thing 1 is done."',
    parts: [{ text: 'The AI said this was done at step 2: ' }, { quote: 'Thing 1 is done.', speaker: 'agent' }],
    quote: 'Thing 1 is done.', role: 'grounded', refs: ['clm'], violations: [], fallbacks: [],
  })
  assert.deepEqual(quoted(buildVerdict(S, { register: 'operator' }).findings[0].said), {
    key: 'chrome.said_unquoted', text: 'The agent wrote about this at step 3; its words are not shown here.', parts: [{ text: 'The agent wrote about this at step 3; its words are not shown here.' }],
    quote: null, role: 'grounded', refs: ['clm'], violations: [],
    fallbacks: ['verdict/said_done_no_proof/not_proven', 'verdict/*/not_proven', 'verdict/*'].map(mismatch),
  })
  const md = buildVerdict(S).exportMarkdown()
  assert.deepEqual(saidLines(md), ['- Said: The AI wrote about this at step 3; its words are not shown here. [clm]', '- Said: The AI said this was done at step 2: "Thing 1 is done." [clm]'])
  assert.equal(md.includes(PROBE), false, 'the unmatched words reach no line of the export')
})

test('clicked: the body\'s role, refs and violations are ground()\'s over the template\'s own words, and a body ground refuses falls back to one that names the node without its text', () => {
  const cmd = 'cargo test --manifest-path crates/app/Cargo.toml → exit 1'
  const commit = 'git commit -m "fake fixtures"'
  const S = session({
    events: [{ seq: 7, kind: 'exec', text: cmd }, { seq: 9, kind: 'exec', text: commit }],
    nodes: [
      { id: 'ev7', objType: 'event', kind: 'exec', seq: 7, title: 'Run automated checks', lines: [cmd] },
      { id: 'ev9', objType: 'event', kind: 'exec', seq: 9, title: 'Save changes in version history', lines: [commit] },
    ],
  })
  const EXEC = 'This is a command run in the terminal, with its exit code and output'
  const TAIL = ' Commands are the strongest proof in the record: they either passed or they did not.'
  const unquoted = `${EXEC}.${TAIL}`
  assert.deepEqual(quoted(buildVerdict(S, { register: 'operator' }).clicked('ev7').body), {
    key: 'clicked.exec.body', text: `${EXEC} — ${cmd}.${TAIL}`, parts: [{ text: `${EXEC} — ${cmd}.${TAIL}` }], quote: null, role: 'grounded', refs: ['ev7'], violations: [], fallbacks: [],
  })
  assert.deepEqual(quoted(buildVerdict(S).clicked('ev7').body), {
    key: 'clicked.exec.unquoted', text: unquoted, parts: [{ text: unquoted }], quote: null, role: 'grounded', refs: ['ev7'], violations: [],
    fallbacks: [{ key: 'clicked.exec.body', violations: [{ kind: 'path_in_manager_register', token: 'crates/app/Cargo.toml' }] }],
  })
  for (const register of ['manager', 'operator']) {
    assert.deepEqual(quoted(buildVerdict(S, { register }).clicked('ev9').body), {
      key: 'clicked.exec.unquoted', text: unquoted, parts: [{ text: unquoted }], quote: null, role: 'grounded', refs: ['ev9'], violations: [],
      fallbacks: [{ key: 'clicked.exec.body', violations: [{ kind: 'banned_token', token: 'fake', word: '"fake' }] }],
    }, register)
  }
})

test('clicked: the user\'s own prompt, their ask and a claim\'s text are quotations in the body, never words of the grounded sentence; with no record behind the node the body names it without them', () => {
  const S = session({
    events: [{ seq: 8, kind: 'user_prompt', text: PROBE }],
    claims: [claim(1, { text: PROBE })],
    asks: [{ id: 'a1', node: 'ask1', kind: 'feature', text: PROBE }],
    nodes: [
      { id: 'ev8', objType: 'event', kind: 'user_prompt', seq: 8, title: PROBE, lines: [] },
      { id: 'ask1', objType: 'ask', title: `“${PROBE}”`, lines: [] },
      { id: 'clm', objType: 'claim', title: `“${PROBE}”`, lines: ['scope turn · extracted by x@1'] },
      { id: 'ev9', objType: 'event', kind: 'user_prompt', seq: 9, title: PROBE, lines: [] },
      { id: 'clm9', objType: 'claim', title: `“${PROBE}”`, lines: [] },
    ],
  })
  const USER = ' What you asked for is measured against what the AI reported done.'
  const CLAIM = "This is a claim — the AI's own words that a piece of work is finished"
  const STATE = ' Its state on the Verdict tab is what the record says about it.'
  for (const register of ['manager', 'operator']) {
    const V = buildVerdict(S, { register })
    assert.deepEqual(quoted(V.clicked('ev8').body), {
      key: 'clicked.user_prompt.body', text: `This is your own message to the AI: "${PROBE}".${USER}`,
      /* `excerpt: true` since commit G p1p2-followups (p2 verifier F3): a user prompt's text is the timeline's, the first
       * line of the message shortened when long, so its quotation is marked as part of what was written. */
      parts: [{ text: 'This is your own message to the AI: ' }, { quote: PROBE, speaker: 'user', excerpt: true }, { text: `.${USER}` }],
      quote: PROBE, role: 'grounded', refs: ['ev8'], violations: [], fallbacks: [],
    }, register)
    assert.deepEqual(quoted(V.clicked('ask1').body), {
      key: 'clicked.ask.body', text: `This is a request from you: "${PROBE}". ThruLine measures what the AI reported done against it.`,
      parts: [{ text: 'This is a request from you: ' }, { quote: PROBE, speaker: 'user' }, { text: '. ThruLine measures what the AI reported done against it.' }],
      quote: PROBE, role: 'grounded', refs: ['ask1'], violations: [], fallbacks: [],
    }, register)
    assert.deepEqual(quoted(V.clicked('clm').body), {
      key: 'clicked.claim.body', text: `${CLAIM}: "${PROBE}".${STATE}`,
      parts: [{ text: `${CLAIM}: ` }, { quote: PROBE, speaker: 'agent' }, { text: `.${STATE}` }],
      quote: PROBE, role: 'grounded', refs: ['clm'], violations: [], fallbacks: [],
    }, register)
    /* No record behind the node: no quotation, and the node's own title (the probe) does not stand in for one. */
    assert.deepEqual(quoted(V.clicked('ev9').body), {
      key: 'clicked.user_prompt.unquoted', text: `This is your own message to the AI.${USER}`, parts: [{ text: `This is your own message to the AI.${USER}` }],
      quote: null, role: 'grounded', refs: ['ev9'], violations: [], fallbacks: [{ key: 'clicked.user_prompt.body', violations: [{ kind: 'quote_ref_unresolved', slot: 'prompt_quote', ref: 'ev9' }] }],
    }, register)
    assert.deepEqual(quoted(V.clicked('clm9').body), {
      key: 'clicked.claim.unquoted', text: `${CLAIM}.${STATE}`, parts: [{ text: `${CLAIM}.${STATE}` }],
      quote: null, role: 'grounded', refs: ['clm9'], violations: [], fallbacks: [{ key: 'clicked.claim.body', violations: [{ kind: 'quote_ref_unresolved', slot: 'claim_quote', ref: 'clm9' }] }],
    }, register)
  }
})

/* ── commit G p1p2-followups: the p1 and p2 verifiers' minor findings ── */

test('Said: a card on a generic family quotes the agent\'s words in Plain as it does in Technical — an open promise and a Fog claim', () => {
  /* p2 coder loose thread 1: the Plain said templates of verdict/*\/{proven,not_proven,couldnt,loose,fog} and verdict/*
   * quoted nothing, so these rows showed no agent words in Plain. */
  const open = session({ promises: [{ id: 'p1', node: 'prm', kind: 'follow_up', status: 'open', text: PROBE, seq: 5 }] })
  assert.deepEqual(quoted(buildVerdict(open).findings[0].said), {
    key: 'verdict/*/loose', text: `The AI said it would come back to this at step 5: "${PROBE}"`,
    parts: [{ text: 'The AI said it would come back to this at step 5: ' }, { quote: PROBE, speaker: 'agent' }],
    quote: PROBE, role: 'grounded', refs: ['prm'], violations: [], fallbacks: [],
  })
  assert.deepEqual(saidLines(buildVerdict(open).exportMarkdown()), [`- Said: The AI said it would come back to this at step 5: "${PROBE_MD}" [prm]`])
  const fog = session({ capture: { degraded: true, condition: 'degraded' }, claims: [claim(1, { verdict: 'unverifiable', text: PROBE, seq: 3 })] })
  assert.deepEqual(quoted(buildVerdict(fog).findings.find((c) => c.id === 'c1').said), {
    key: 'verdict/*/fog', text: `The AI reported task complete at step 3: "${PROBE}"`,
    parts: [{ text: 'The AI reported task complete at step 3: ' }, { quote: PROBE, speaker: 'agent' }],
    quote: PROBE, role: 'grounded', refs: ['clm'], violations: [], fallbacks: [],
  })
})

test('clicked: a body slot that is empty or blank is never filled with nothing; the body that names the node is shown, and the reason is recorded', () => {
  /* p1 verifier pass 3, carried finding 3: a clicked template filled by fill() read "This is the tool's answer to the AI
   * — . It is evidence…". The body goes through compose now, whose empty and blank slots are refused. */
  const S = session({ nodes: [
    { id: 'ev14', objType: 'event', kind: 'tool_result', seq: 14, title: '', sub: '', lines: [] },
    { id: 'ev15', objType: 'event', kind: 'tool_result', seq: 15, title: '   ', sub: '', lines: [] },
    { id: 'ev16', objType: 'event', kind: 'exec', seq: 16, title: 'x', lines: ['   '] },
  ] })
  const TOOL = "This is the tool's answer to the AI. It is evidence of what happened, and the checks read it."
  const EXEC = 'This is a command run in the terminal, with its exit code and output. Commands are the strongest proof in the record: they either passed or they did not.'
  for (const register of ['manager', 'operator']) {
    const V = buildVerdict(S, { register })
    for (const [id, type, text] of [['ev14', 'tool_result', TOOL], ['ev15', 'tool_result', TOOL], ['ev16', 'exec', EXEC]]) {
      assert.deepEqual(quoted(V.clicked(id).body), {
        key: `clicked.${type}.unquoted`, text, parts: [{ text }], quote: null, role: 'grounded', refs: [id], violations: [],
        fallbacks: [{ key: `clicked.${type}.body`, violations: [{ kind: 'empty_slot', slot: 'detail' }] }],
      }, `[${register}] ${id}`)
    }
  }
})

test('clicked and Do prompt: a quotation that ends its own sentence is not followed by the template\'s full stop', () => {
  /* p2 verifier F6: "…in `src/cli.rs`.". What you asked for…" and "Earlier you said "…in a follow-up.". Is that done?". */
  const S = session({
    events: [{ seq: 8, kind: 'user_prompt', text: 'Rename the flag in `src/cli.rs`.' }, { seq: 9, kind: 'user_prompt', text: 'Please drop the unused import and keep cli…' }, { seq: 10, kind: 'user_prompt', text: 'fix the totals' }],
    claims: [claim(1, { text: 'Thing 1 is done.' })],
    promises: [{ id: 'p1', node: 'prm', kind: 'follow_up', status: 'open', text: 'Is the changelog next?' }],
    asks: [{ id: 'a1', node: 'ask1', kind: 'feature', text: 'Add a debug flag!' }],
    nodes: [
      { id: 'ev8', objType: 'event', kind: 'user_prompt', seq: 8, title: 'x', lines: [] },
      { id: 'ev9', objType: 'event', kind: 'user_prompt', seq: 9, title: 'x', lines: [] },
      { id: 'ev10', objType: 'event', kind: 'user_prompt', seq: 10, title: 'x', lines: [] },
      { id: 'clm', objType: 'claim', title: 'x', lines: [] },
      { id: 'prm', objType: 'promise', title: 'x', lines: [] },
      { id: 'ask1', objType: 'ask', title: 'x', lines: [] },
    ],
  })
  const USER = 'What you asked for is measured against what the AI reported done.'
  for (const register of ['manager', 'operator']) {
    const V = buildVerdict(S, { register })
    assert.deepEqual(['ev8', 'ev9', 'ev10', 'clm', 'prm', 'ask1'].map((id) => V.clicked(id).body.text), [
      `This is your own message to the AI: "Rename the flag in \`src/cli.rs\`." ${USER}`,
      `This is your own message to the AI: "Please drop the unused import and keep cli…" ${USER}`,
      `This is your own message to the AI: "fix the totals". ${USER}`,
      'This is a claim — the AI\'s own words that a piece of work is finished: "Thing 1 is done." Its state on the Verdict tab is what the record says about it.',
      'This is a promise — the AI\'s own words that it would come back to something: "Is the changelog next?" Its state is whether the record shows it done later.',
      'This is a request from you: "Add a debug flag!" ThruLine measures what the AI reported done against it.',
    ], register)
    assert.deepEqual(V.clicked('ev8').body.parts, [{ text: 'This is your own message to the AI: ' }, { quote: 'Rename the flag in `src/cli.rs`.', speaker: 'user', excerpt: true }, { text: ` ${USER}` }], register)
    assert.equal(V.findings.find((c) => c.id === 'p1').do.prompt, 'Earlier you said "Is the changelog next?" Is that done? If it is, paste the change; if not, do it now and paste what you changed.', register)
  }
})

test('clicked: a node kind or type that is an Object.prototype name, or the shared link label, is an unknown kind — no clicked body, and no throw', () => {
  /* p2 verifier F8: clickedType read HQ_COPY.clicked[kind] through the prototype, so these threw "fill: template must be
   * a string". The copy has no generic clicked template; an unknown kind has always had no clicked body (ev9 below). */
  const KINDS = ['constructor', 'toString', '__proto__', 'hasOwnProperty', 'link']
  const nodes = [
    ...KINDS.map((kind, i) => ({ id: `ev${i + 1}`, objType: 'event', kind, seq: i + 1, title: 't', lines: [] })),
    ...KINDS.map((kind, i) => ({ id: `ob${i + 1}`, objType: kind, title: 't', lines: [] })),
    { id: 'ev9', objType: 'event', kind: 'compaction', seq: 9, title: 't', lines: [] },
    { id: 'ev10', objType: 'event', kind: 'exec', seq: 10, title: 't', lines: ['cargo test → exit 1'] },
  ]
  for (const register of ['manager', 'operator']) {
    const V = buildVerdict(session({ nodes }), { register })
    assert.equal(V.clicked('ev9'), null, 'an unknown kind has no clicked body')
    for (const n of nodes.slice(0, 10)) assert.equal(V.clicked(n.id), null, `[${register}] objType ${n.objType}, kind ${n.kind}`)
    assert.equal(V.clicked('ev10').type, 'exec', 'a kind the copy names still resolves')
  }
})

/* ── failed command exits and failed tool calls (commit G p3-wording) ────
 * STEP0 audit §2.6: one failed `cargo test` read "2 commands failed". The record does not link an exec row to the tool
 * call that ran it (docs/INTERFACE-REQUESTS.md rows 20-22), so the panel names each record by its kind, counts each as
 * itself, and says neither that the two are one run nor that they are two. No row is paired by position, time or order. */
const callRow = (seq, command, tool = 'Bash') => ({ seq, kind: 'tool_call', tool, text: `${tool} ${JSON.stringify({ command }).slice(0, 50)}…`, input: JSON.stringify({ command, description: 'd' }) })
const execRow = (seq, argv, code) => ({ seq, kind: 'exec', text: `${argv} → exit ${code}` })
const resultRow = (seq, ok, tool = 'Bash') => ({ seq, kind: 'tool_result', tool, text: `${tool} ${ok ? 'ok' : 'FAILED'}` })
/** Every number that stands for commands or runs ("2 commands", "1 failed command", "3 runs"); a failed command exit is not one. */
const runCounts = (text) => String(text).match(/\b\d+ (?:failed )?(?:commands?|runs?)\b(?! exits?\b)/g) || []
const FAILED_CLAIM = [claim(1, { findings: [finding(1, 'test_run_failed')] })]
const FAILED_LABEL = 'Which commands or tool calls failed?'
const FAILED_FAMILY = 'verdict/command_failed_then_done/not_proven'
/** What one register shows about failures: the counts, the question, the headline, the command-failed card's Record,
 * and every number on screen or in the export that stands for commands or runs. */
function failureSurfaces(S, register) {
  const V = buildVerdict(S, { register })
  const card = V.findings.find((c) => c.kind === 'command_failed_then_done')
  return {
    counts: { failed_exits: V.counts.failed_exits, failed_tool_calls: V.counts.failed_tool_calls },
    question: { label: V.questions[1].label, answer: V.questions[1].answer, refs: V.questions[1].refs },
    headline: V.headline,
    record: card ? { key: card.record.key, text: card.record.text, fallbacks: card.record.fallbacks } : null,
    runs: [...renderedStrings(V), V.exportMarkdown()].flatMap(runCounts),
  }
}

test('the run-count guard is live: it flags a number that stands for commands or runs, and not a failed command exit (commit G p3-wording)', () => {
  assert.deepEqual(runCounts('2 commands failed. 1 command failed, no successful re-run. 2 failed commands. 3 runs.'), ['2 commands', '1 command', '2 failed commands', '3 runs'])
  assert.deepEqual(runCounts('The record shows 1 failed command exit and 2 failed command exits, and 1 failed tool call.'), [])
})

test('two distinct failed runs, each with a failed command exit and a failed tool call: four records, each counted as itself (commit G p3-wording)', () => {
  const S = session({ claims: FAILED_CLAIM, events: [callRow(20, 'cargo test'), execRow(24, 'cargo test', 101), resultRow(28, false), callRow(32, 'cargo build'), execRow(36, 'cargo build', 101), resultRow(40, false)] })
  const shared = { counts: { failed_exits: 2, failed_tool_calls: 2 }, question: { label: FAILED_LABEL, answer: '2 failed command exits and 2 failed tool calls', refs: ['ev24', 'ev28', 'ev36', 'ev40'] }, runs: [] }
  assert.deepEqual(failureSurfaces(S, 'manager'), { ...shared,
    headline: 'Claude said this session finished. Of what it reported done, the record backs 0 of 1; 1 has no proof. The record shows 2 failed command exits and 2 failed tool calls with no successful re-run afterwards.',
    record: { key: FAILED_FAMILY, text: 'Not proven: before this was reported done, the record shows 2 failed command exits and 2 failed tool calls, and no successful re-run afterwards.', fallbacks: [] } })
  assert.deepEqual(failureSurfaces(S, 'operator'), { ...shared,
    headline: 'Claude claimed completion. Claims the record backs: 0/1; 1 without proof. In the record: 2 failed command exits and 2 failed tool calls, no successful re-run.',
    record: { key: FAILED_FAMILY, text: 'Not proven: before this claim the record shows 2 failed command exits and 2 failed tool calls, and no successful re-run afterwards.', fallbacks: [] } })
})

test('a failed command exit with no failed tool result: one failed command exit, and no tool call is named (commit G p3-wording)', () => {
  const S = session({ claims: FAILED_CLAIM, events: [callRow(20, 'cargo test || true'), execRow(24, 'cargo test', 1), resultRow(28, true)] })
  const shared = { counts: { failed_exits: 1, failed_tool_calls: 0 }, question: { label: FAILED_LABEL, answer: '1 failed command exit', refs: ['ev24'] }, runs: [] }
  assert.deepEqual(failureSurfaces(S, 'manager'), { ...shared,
    headline: 'Claude said this session finished. Of what it reported done, the record backs 0 of 1; 1 has no proof. The record shows 1 failed command exit with no successful re-run afterwards.',
    record: { key: FAILED_FAMILY, text: 'Not proven: before this was reported done, the record shows 1 failed command exit, and no successful re-run afterwards.', fallbacks: [] } })
  assert.deepEqual(failureSurfaces(S, 'operator'), { ...shared,
    headline: 'Claude claimed completion. Claims the record backs: 0/1; 1 without proof. In the record: 1 failed command exit, no successful re-run.',
    record: { key: FAILED_FAMILY, text: 'Not proven: before this claim the record shows 1 failed command exit, and no successful re-run afterwards.', fallbacks: [] } })
})

test('a failed tool result that is not a command, with no exec: one failed tool call, never a command (commit G p3-wording)', () => {
  const S = session({ claims: FAILED_CLAIM, events: [callRow(20, 'x', 'Edit'), resultRow(24, false, 'Edit')] })
  const shared = { counts: { failed_exits: 0, failed_tool_calls: 1 }, question: { label: FAILED_LABEL, answer: '1 failed tool call', refs: ['ev24'] }, runs: [] }
  assert.deepEqual(failureSurfaces(S, 'manager'), { ...shared,
    headline: 'Claude said this session finished. Of what it reported done, the record backs 0 of 1; 1 has no proof. The record shows 1 failed tool call with no successful re-run afterwards.',
    record: { key: FAILED_FAMILY, text: 'Not proven: before this was reported done, the record shows 1 failed tool call, and no successful re-run afterwards.', fallbacks: [] } })
  assert.deepEqual(failureSurfaces(S, 'operator'), { ...shared,
    headline: 'Claude claimed completion. Claims the record backs: 0/1; 1 without proof. In the record: 1 failed tool call, no successful re-run.',
    record: { key: FAILED_FAMILY, text: 'Not proven: before this claim the record shows 1 failed tool call, and no successful re-run afterwards.', fallbacks: [] } })
})

test('a failed command exit and a failed tool call that both later passed: the answer names each, the headline adds no clause, and the card falls back rather than show a zero (commit G p3-wording)', () => {
  const S = session({ claims: FAILED_CLAIM, events: [execRow(24, 'cargo test', 101), resultRow(28, false), execRow(32, 'cargo test', 0), resultRow(36, true)] })
  const shared = { counts: { failed_exits: 1, failed_tool_calls: 1 }, question: { label: FAILED_LABEL, answer: '1 failed command exit and 1 failed tool call, later passed', refs: ['ev24', 'ev28'] }, runs: [] }
  const passedOver = [{ key: FAILED_FAMILY, violations: [{ kind: 'empty_slot', slot: 'counts' }] }]
  assert.deepEqual(failureSurfaces(S, 'manager'), { ...shared,
    headline: 'Claude said this session finished. Of what it reported done, the record backs 0 of 1; 1 has no proof.',
    record: { key: 'verdict/*/not_proven', text: 'Not proven: nothing in the record shows this.', fallbacks: passedOver } })
  assert.deepEqual(failureSurfaces(S, 'operator'), { ...shared,
    headline: 'Claude claimed completion. Claims the record backs: 0/1; 1 without proof.',
    record: { key: 'verdict/*/not_proven', text: 'Not proven: nothing in the record shows this — test_run_failed detail.', fallbacks: passedOver } })
})
