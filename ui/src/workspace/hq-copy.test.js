import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { HQ_COPY } from './hq-copy.js'
import { fill, compose, SlotError, pick, ground, verdictKeys, findBannedToken, BANNED_TOKENS, placeholders } from './hq-narrate.js'
import * as narrate from './hq-narrate.js'
import { renderCopyMarkdown } from './hq-copy-export.mjs'

const GOLDEN = fileURLToPath(new URL('./hq-copy.golden.json', import.meta.url))
const COPY_MD = fileURLToPath(new URL('../../../docs/HQ-COPY.md', import.meta.url))

// One value per placeholder the copy file uses. `fill` throws on any other.
const SAMPLE = {
  agent: 'Claude',
  reported: 12,
  proven: 9,
  not_proven: 2,
  couldnt: 1,
  failed: '2 failed command exits and 13 failed tool calls',
  loose: 1,
  n: 9,
  claim_quote: 'All builds are green and the app is ready to test.',
  prompt_quote: 'Rename the flag to --debug and run the tests.',
  claim_kind: 'the tests passing',
  when: '14:41',
  reason: 'no run is bound to the final tree',
  detector: 'test_run_failed',
  detector_plain: 'the test run failed',
  detail: 'cargo test exited 1',
  path: 'src/app/main.rs',
  counts: '2 failed command exits and 13 failed tool calls',
  hash: 'e71c5b04',
  status: 502,
  // ── the Change workspace (T-2-4) ──
  total: 3,
  with_evidence: 2,
  without_evidence: 1,
  problems: 1,
  revision: 'a1b2c3d',
  date: '17 Sep 2026',
  who: 'Sam',
  version: 2,
  // ── the version the record stands at NOW, beside the one approved (T-5-2) ──
  now: '9f3d21e',
  // ── the accepted list's own number, which is not a version (T-3-2b r1) ──
  list: 2,
  shown: 7,
  // ── the whole story on one page (T-2-5) ──
  why: 'That is what I asked for.',
  work: '3 pieces of linked work',
  steps: '41 recorded steps',
  // ── the Inbox (T-2-6) ──
  size: '7.2 GB',
  // ── the disabled accept control's enabler (T-2-10) ──
  change: 'the-change',
  noun: 'records',
  // ── criterion to evidence (T-3-1) ──
  rows: 3,
  // ── the list of Changes, how many were looked at (A3-F14) ──
  looked: 25,
  // ── what exactly will be examined (T-3-2) ──
  value: 'a1b2c3d4e5f6',
  port: 45781,
  app: 'serve',
  count: '2 files',
  // ── the first-use path (T-2-7) ──
  tool: 'cargo',
  name: '.thruline/cargo-test.json',
  // ── the one derived view-model (T-S-3) ──
  what: 'tests pass',
  scope: 'checkout/',
  word: 'Proven',
  reasons: 'The affected code changed afterward.',
  // ── readiness, and the two dispositions (T-3-5) ──
  authority: 'the accepted check policy',
  why: 'this change touches no code',
  // ── the packet's Copy as Markdown (T-S-5) ──
  said: 'The tests pass.',
  record: 'No run is bound to the final tree.',
  // ── the Trace (P6-3) ──
  a: 12,
  b: 40,
  i: 2,
  stage: 'request',
  from: 'Check run',
  ran: 'c324df6',
  final: '9f2ad3e',
  step: 'step 46',
  lane: 'Execution',
  title: 'Run automated checks',
  kind: 'documented reference',
  // ── what review found (T-4-1) ──
  reviewer: 'PR-Agent 0.45.0',
  current: '9f2ad3e',
  started: '2026-09-23T15:58:18Z',
  ended: '2026-09-23T16:09:18Z',
  opinion: 'it would ask for changes',
  round: 2,
  line: 2,
  // ── where each thing review found stands (T-4-2) ──
  actor: 'Sam',
  label: 'Waived',
  missing: 'the reason and why you can decide this',
  parts: 'the reason',
  to: 'Waived',
  // ── the message for the agent (T5) ──
  check: 'pytest -q tests/test_totals.py',
  /* BUILD-16-R2: the run door's command, as `runCheckCommand` builds it, in the agent's draft. */
  command: 'thruline check --for-change the-change -- <the command you would run>',
  passed: 4,
  // `{line}` is shared with the review's finding_at (T-4-1): one key, the
  // value above (a key given twice fills with the last and hides the first).
  statement: 'detector v2 · nothing matched a known secret shape',
  slot: 'check',
  // ── the list an approval was given against (T7) ──
  then: 1,
  // ── the licence (T-DIST-4) ──
  plan: 'Pro',
  // ── the read-time git line (STEP-7-GIT-LINE); `{now}` is shared above ──
  files: '1 file',
  commits: '1 commit',
  recorded: 'e71c5b0',
  // ── NEVER-ASSESSED-ON-SCREEN: the readiness clause the top sentence carries ──
  readiness: '1 thing still needs evidence.',
  // ── BRIEF-COUNT-AGREES-R2: what Ask says of the lines that were set aside ──
  aside: '1 was set aside, with nobody checking.',
}

function walk(obj, prefix = '', out = []) {
  if (Array.isArray(obj)) obj.forEach((v, i) => walk(v, `${prefix}[${i}]`, out))
  else if (obj && typeof obj === 'object') for (const [k, v] of Object.entries(obj)) walk(v, prefix ? `${prefix}.${k}` : k, out)
  else if (typeof obj === 'string') out.push([prefix, obj])
  else if (typeof obj !== 'boolean') assert.fail(`non-string leaf at ${prefix}: ${typeof obj}`)
  return out
}

const STRINGS = walk(HQ_COPY)

/* PRICING-TRUTHS-IN-BINARY, from A3-WORKBENCH (ruling #5, R-2026-09-26-3):
 * the free third view is Workbench, keys 1 / 2 / 3, and "Pro" names the plan
 * (`licence.plans.pro`) and nothing else. The census over every golden value
 * is `pricing-census.test.js`; this pins the words themselves. */
test('PRICING-TRUTHS: the third view is Workbench, chosen with 3, and "Pro" names only the plan', () => {
  assert.deepEqual(Object.keys(HQ_COPY.shell.altitudes), ['brief', 'explore', 'workbench'])
  assert.equal(HQ_COPY.shell.altitudes.workbench.label, 'Workbench')
  assert.equal(HQ_COPY.shell.altitudes.brief.hint, 'The story: what you are asked to accept, and the one thing to do. Press 1.')
  assert.equal(HQ_COPY.shell.altitudes.explore.hint, 'The whole picture, with one slim toolbar. Press 2.')
  assert.equal(HQ_COPY.shell.altitudes.workbench.hint, 'Every control ThruLine has. Press 3.')
  assert.equal(HQ_COPY.shell.keys.workbench, 'Workbench: every control')
  assert.equal(HQ_COPY.shell.keys.pro, undefined)
  for (const [path, value] of STRINGS) {
    /* The licence section names Pro as the paid plan, including the C20
     * refusal. pricing-census.test.js checks those exact plan-name forms. */
    if (path.startsWith('licence.')) continue
    assert.doesNotMatch(value, /\bPro\b/, `${path} uses the plan's name for something else`)
  }
})

test('PRICING-TRUTHS: the plans link says where it goes, and names no amount', () => {
  assert.equal(HQ_COPY.licence.approve.plans, 'See plans ↗')
  assert.equal(HQ_COPY.licence.approve.plans_opens, 'Opens thrulinehq.com in your browser.')
})

const VERDICT_SECTIONS = ['states', 'kinds', 'detectors', 'headline', 'families', 'clicked', 'phases', 'questions', 'chrome']
const onVerdictTab = STRINGS.filter(([path]) => VERDICT_SECTIONS.some((s) => path.startsWith(s + '.')))

test('(a) every string renders and matches the checked-in golden byte-for-byte', () => {
  assert.ok(STRINGS.length > 200, `expected a full copy file, found ${STRINGS.length} strings`)
  const rendered = {}
  for (const [path, text] of STRINGS) rendered[path] = fill(text, SAMPLE)
  const json = JSON.stringify(rendered, null, 2) + '\n'
  if (process.env.UPDATE_GOLDEN === '1') writeFileSync(GOLDEN, json)
  const onDisk = readFileSync(GOLDEN, 'utf8')
  assert.equal(json, onDisk, 'hq-copy.golden.json differs from the render; UPDATE_GOLDEN=1 to accept a wording change')
})

test('(a) rendered copy carries no unfilled brace', () => {
  for (const [path, text] of STRINGS) assert.doesNotMatch(fill(text, SAMPLE), /[{}]/, path)
})

test('(b) no banned token, in any inflection, anywhere in the copy', () => {
  for (const [path, text] of STRINGS) {
    if (path.startsWith('banned[')) continue
    const hit = findBannedToken(text)
    assert.equal(hit, null, `${path}: "${hit && hit.word}" reduces to banned token "${hit && hit.token}"`)
  }
  assert.deepEqual([...HQ_COPY.banned], [...BANNED_TOKENS])
  assert.equal(BANNED_TOKENS.length, 13)
})

/* The engine noun "reading" (a session read) never renders. The verb is
 * allowed in exactly these paths, by name — not by case accident.
 * `change.packet.sections.review.empty` is the ruled REVIEW-EMPTY-TRUE
 * sentence (D-331): "anyone else reading it" is a person reading the Change,
 * the verb, and the noun patterns above still apply to it. */
const READING_VERB_ALLOWED = ['phases.labels.read_record', 'phases.title', 'chrome.reading.title', 'change.packet.sections.review.empty']
const READING_NOUN = [/\b(?:a|this|the|that|each|every|one|no)\s+reading\b/i, /\breading\s*[.·,;:!?]/i, /\breading\s*$/i, /\breadings\b/i]

test('(c) engine vocabulary never reaches the Verdict tab', () => {
  const forbidden = ['check receipt', 'review point', 'object', 'certify', 'unverified']
  for (const [path, text] of onVerdictTab) {
    const lower = text.toLowerCase()
    for (const word of forbidden) assert.ok(!lower.includes(word), `${path} contains "${word}": ${text}`)
  }
  for (const [path, text] of STRINGS) {
    if (path.startsWith('forbidden_on_verdict[')) continue
    for (const re of READING_NOUN) assert.doesNotMatch(text, re, `${path} uses the engine noun "reading": ${text}`)
    if (!READING_VERB_ALLOWED.includes(path)) assert.doesNotMatch(text, /\breading\b/i, `${path} says "reading" outside the allow-list: ${text}`)
  }
  assert.match(HQ_COPY.phases.labels.read_record, /^Reading the record$/, 'the allow-listed verb phrase is the one the brief names')
  for (const [path, text] of STRINGS.filter(([p]) => p.startsWith('headline.'))) {
    assert.doesNotMatch(text, /\bevents?\b/i, `${path} counts events in a headline: ${text}`)
  }
  assert.equal(HQ_COPY.phases.checked.events, '{n} events', '"events" is allowed on the Checked: line — the brief bans it only as a headline count')
  for (const [key, v] of Object.entries(HQ_COPY.vocabulary)) {
    if (v.dropped) continue
    for (const word of forbidden) assert.ok(!v.panel.toLowerCase().includes(word), `vocabulary.${key}.panel`)
  }
})

test('(c) the reading guard is a live instrument', () => {
  for (const bad of ['Refresh this reading.', 'a reading of the record', 'The reading · 3 events', 'two readings']) {
    assert.ok(READING_NOUN.some((re) => re.test(bad)), `guard misses: ${bad}`)
  }
  assert.ok(!READING_NOUN.some((re) => re.test('Reading the record')), 'the verb phrase passes the noun patterns')
})

test('(c) no state word outside the five can render: vocabulary is a review table, never a render source', () => {
  const five = Object.values(HQ_COPY.states).map((s) => s.word)
  for (const [key, v] of Object.entries(HQ_COPY.vocabulary)) {
    if (v.dropped) continue
    if (/^(supported|contradicted|no_proof_found|insufficient_evidence|not_evaluated|dropped_promise|chain_intact_false|raw_files_missing)$/.test(key)) {
      assert.ok(five.includes(v.panel), `vocabulary.${key}.panel = "${v.panel}" is a sixth state word`)
    }
  }
  const here = dirname(fileURLToPath(import.meta.url))
  for (const file of ['verdict-model.js', 'verdict-panel.jsx', 'checking-sequence.jsx', 'hq.jsx']) {
    const src = readFileSync(join(here, file), 'utf8')
    assert.ok(!/HQ_COPY\.vocabulary|\.vocabulary\b/.test(src), `${file} reads HQ_COPY.vocabulary — a render path for a sixth word`)
    assert.ok(!/Not checked/.test(src), `${file} carries the literal "Not checked"`)
  }
})

test('(c) rulings: fog count has a plural, "when" carries no engine jargon, detectors map to plain phrases', () => {
  assert.equal(HQ_COPY.chrome.counts.fog_one, '1 recording gap')
  assert.equal(fill(HQ_COPY.chrome.counts.fog, { n: 2 }), '2 recording gaps')
  assert.equal(fill(HQ_COPY.chrome.when_seq, { n: 5 }), 'step 5')
  assert.doesNotMatch(HQ_COPY.chrome.when_seq, /\bseq\b/)
  assert.equal(fill(HQ_COPY.clicked.exec.title, { when: fill(HQ_COPY.chrome.when_seq, { n: 5 }) }), 'You clicked: a command the AI ran at step 5.')
  for (const id of ['test_run_failed', 'reexecution_failed', 'assertion_removed', 'assertion_weakened', 'skip_marker_added', 'test_deleted', 'narrowed_run', 'tests_ran_before_final_edit']) {
    assert.ok(typeof HQ_COPY.detectors[id] === 'string' && !/_/.test(HQ_COPY.detectors[id]), `detectors.${id}`)
  }
  for (const [key, regs] of Object.entries(HQ_COPY.families)) {
    for (const [part, text] of Object.entries(regs.manager)) assert.ok(!placeholders(text).includes('detector'), `${key}.manager.${part} shows a raw detector id in Plain`)
  }
  /* Verifier ids map to plain phrases the same way (ADJUDICATIONS item E finding 6). */
  for (const id of ['unclaimed_completion', 'tests_pass']) {
    assert.ok(typeof HQ_COPY.chrome.verifiers[id] === 'string' && !/[_@]/.test(HQ_COPY.chrome.verifiers[id]), `chrome.verifiers.${id}`)
  }
  assert.equal(HQ_COPY.chrome.no_reason, 'no reason recorded')
  assert.equal(HQ_COPY.chrome.agent_fallback, 'The AI')
  assert.equal(HQ_COPY.chrome.agent_fallback_lower, 'the AI')
  assert.equal(HQ_COPY.phases.checked.claims_one, '1 completion statement', 'the Checked: line counts extractor-matched statements under a noun the pills do not use')
  assert.equal(fill(HQ_COPY.phases.checked.derived, { n: 2 }), '2 derived from unfinished work')
  assert.equal(HQ_COPY.states.loose.subtitle, 'Promised, and the record shows no follow-through')
  assert.doesNotMatch(HQ_COPY.clicked.note.body, /not its text|AI talking/)
  assert.match(HQ_COPY.clicked.note.body, /record holds the text/)
})

test('(c) manager register names no file path and uses no {path} placeholder', () => {
  for (const [key, regs] of Object.entries(HQ_COPY.families)) {
    for (const [part, text] of Object.entries(regs.manager)) {
      assert.ok(!placeholders(text).includes('path'), `${key}.manager.${part} uses {path}`)
      const g = ground(fill(text, SAMPLE), { refs: ['r-2215'], numbers: Object.values(SAMPLE), register: 'manager' })
      assert.ok(!g.violations.some((v) => v.kind === 'path_in_manager_register'), `${key}.manager.${part}: ${JSON.stringify(g.violations)}`)
    }
  }
  for (const [key, regs] of Object.entries(HQ_COPY.headline)) {
    const g = ground(fill(regs.manager, SAMPLE), { numbers: Object.values(SAMPLE), register: 'manager' })
    assert.ok(g.ok, `${key}.manager: ${JSON.stringify(g.violations)}`)
  }
})

test('(d) grounding: an invented number is flagged', () => {
  const g = ground('The record shows 15 commands failing [r-1].', { refs: ['r-1'], numbers: [9], register: 'manager' })
  assert.equal(g.ok, false)
  assert.deepEqual(g.violations.map((v) => v.kind), ['unlicensed_number'])
  assert.equal(g.role, 'grounded')
  assert.ok(ground('The record shows 15 commands failing [r-1].', { refs: ['r-1'], numbers: ['15'] }).ok)
  assert.ok(ground('Last failure at 14:28 [r-1].', { refs: ['r-1'], numbers: ['14:28'] }).ok, 'a time licenses its digit runs')
  assert.ok(ground('Last failure [r-2215].', { refs: ['r-2215'], numbers: [] }).ok, 'digits inside a ref chip are not numbers')
})

test('(d) grounding: an unknown ref is flagged', () => {
  const g = ground('The record backs this [r-9].', { refs: ['r-1', '[r-2]'], numbers: [] })
  assert.deepEqual(g.violations, [{ kind: 'unknown_ref', id: 'r-9' }])
  assert.ok(ground('The record backs this [r-2].', { refs: ['r-1', '[r-2]'], numbers: [] }).ok, 'bracketed refs are accepted')
})

test('(d) grounding: banned tokens are flagged, inflections included', () => {
  for (const [sentence, token] of [
    ['The agent lied about it [r-1].', 'lied'],
    ['The record shows it was Faking a pass [r-1].', 'fake'],
    ['Liars, all of them [r-1].', 'liar'],
    ['A lie-shaped claim [r-1].', 'lie'],
    ['Sheer laziness [r-1].', 'lazy'],
    ['Deceptively green [r-1].', 'deceptive'],
    ['It was hallucinating [r-1].', 'hallucinated'],
    ['Dishonesty [r-1].', 'dishonest'],
  ]) {
    const g = ground(sentence, { refs: ['r-1'], numbers: [] })
    assert.deepEqual(g.violations.map((v) => [v.kind, v.token]), [['banned_token', token]], sentence)
  }
  for (const ok of ['The record relies on the final tree [r-1].', 'Earlier runs applied [r-1].', 'A familiar shape [r-1].', 'Delayed [r-1].']) {
    assert.ok(ground(ok, { refs: ['r-1'], numbers: [] }).ok, ok)
  }
})

test('(d) grounding: a sentence with no ref is an inference, not a violation', () => {
  const g = ground('Ask the AI to show its work for this one.', { refs: ['r-1'], numbers: [] })
  assert.equal(g.role, 'inference')
  assert.equal(g.ok, true)
  assert.equal(ground('Proven [r-1].', { refs: ['r-1'], numbers: [] }).role, 'grounded')
})

test('(d) grounding: a path in the manager register is flagged, and allowed in the operator register', () => {
  const s = 'The record shows src/app/main.rs changed after the last run [r-1].'
  const m = ground(s, { refs: ['r-1'], numbers: [], register: 'manager' })
  assert.deepEqual(m.violations.map((v) => v.kind), ['path_in_manager_register'])
  assert.ok(ground(s, { refs: ['r-1'], numbers: [], register: 'operator' }).ok)
  assert.ok(ground('Proven 9/12 either way [r-1].', { refs: ['r-1'], numbers: [9, 12], register: 'manager' }).ok, 'a ratio is not a path')
})

test('(d) grounding: sentence budget is 3 for operator, 5 for manager', () => {
  const four = 'One [r-1]. Two. Three. Four.'
  assert.deepEqual(ground(four, { refs: ['r-1'], numbers: [], register: 'operator' }).violations, [{ kind: 'too_many_sentences', count: 4, budget: 3 }])
  assert.ok(ground(four, { refs: ['r-1'], numbers: [], register: 'manager' }).ok)
  assert.equal(ground('The agent wrote: "All tests pass." [r-1]', { refs: ['r-1'], numbers: [] }).ok, true, 'a quoted sentence end counts once')
})

test('(e) fill throws on a missing placeholder and never renders a brace', () => {
  assert.throws(() => fill('{agent} said {reported} things', { agent: 'Claude' }), /missing placeholder value for \{reported\}/)
  assert.throws(() => fill('{n}', { n: null }), /\{n\}/)
  assert.equal(fill('{agent} said {reported} things', { agent: 'Claude', reported: 12 }), 'Claude said 12 things')
  assert.equal(fill('plain', {}), 'plain')
})

test('(e) fill resolves own properties only: an inherited name throws exactly as a missing name does', () => {
  /* STEP0 audit §1.1 (a2): `values[name]` read through the prototype rendered
   * "function Object() { [native code] }" and "[object Object]". */
  assert.throws(() => fill('{constructor}', {}), { message: 'fill: missing placeholder value for {constructor}' })
  assert.throws(() => fill('{__proto__}', {}), { message: 'fill: missing placeholder value for {__proto__}' })
  assert.throws(() => fill('Said {n} things', Object.create({ n: 9 })), { message: 'fill: missing placeholder value for {n}' }, 'a value on the prototype is not a value')
  assert.equal(fill('{constructor}', { constructor: 'own' }), 'own', 'an own property of that name still fills')
  assert.equal(fill('{__proto__}', JSON.parse('{"__proto__":"own"}')), 'own', 'an own __proto__ key still fills')
})

test('(e) compose refuses a slot it cannot fill: a missing, inherited, empty or blank value throws a SlotError; other values are trimmed', () => {
  const slotError = (kind, slot) => (e) => e instanceof SlotError && e.kind === kind && e.slot === slot &&
    e.message === `compose: ${kind === 'empty_slot' ? 'empty' : 'missing'} placeholder value for {${slot}}`
  assert.throws(() => compose('Proven: the record backs it — {reason}.', {}), slotError('missing_slot', 'reason'))
  assert.throws(() => compose('x {constructor}.', {}), slotError('missing_slot', 'constructor'), 'an inherited name is a missing name')
  assert.throws(() => compose('x {__proto__}.', {}), slotError('missing_slot', '__proto__'), 'an inherited name is a missing name')
  assert.throws(() => compose('Said {n} things', Object.create({ n: 9 })), slotError('missing_slot', 'n'), 'a value on the prototype is not a value')
  assert.throws(() => compose('the opposite — {detail}.', { detail: '' }), slotError('empty_slot', 'detail'))
  assert.throws(() => compose('the opposite — {detail}.', { detail: '   ' }), slotError('empty_slot', 'detail'), 'a blank value is an empty value')
  assert.throws(() => compose('the tests weakened — {detail} in {path} here', { detail: 'x', path: ' \t ' }), slotError('empty_slot', 'path'), 'a blank value is refused where no full stop follows it too')
  assert.equal(compose('the tests weakened — {detail} in {path}.', { detail: '  assertion removed  ', path: ' src/a.rs ' }), 'the tests weakened — assertion removed in src/a.rs.')
  assert.equal(compose('{constructor}', { constructor: 'own' }), 'own', 'an own property of that name still composes')
})

test('(e) fill and compose drop one closing full stop of a value only where the template puts a full stop right after the slot', () => {
  /* Fix pass 1, verifier finding 1: the claims__05 clicked body read "…in `src/cli.rs`.. What you asked for…". */
  const body = 'This is your own message to the AI — {detail}. What you asked for is measured.'
  for (const f of [fill, compose]) {
    assert.equal(f(body, { detail: 'Rename the flag in `src/cli.rs`.' }), 'This is your own message to the AI — Rename the flag in `src/cli.rs`. What you asked for is measured.', `${f.name}: the template's stop is the only stop`)
    /* Fix pass 2, verifier finding 1: this value rendered "exited 1." when the whole run of stops was dropped. A value
     * ending in two stops is indistinguishable from `git diff main..`, so it now reads as recorded. */
    assert.equal(f('the opposite — {detail}.', { detail: 'exited 1..' }), 'the opposite — exited 1..', `${f.name}: only the one stop the template supplies is merged; the value reads as recorded`)
    assert.equal(f('the opposite — {detail}.', { detail: 'exited 1. ' }), 'the opposite — exited 1.', `${f.name}: a stop followed by a space`)
    assert.equal(f('the opposite — {detail}.', { detail: 'see ../src/a.rs' }), 'the opposite — see ../src/a.rs.', `${f.name}: stops inside a value stay`)
    assert.equal(f('{detail} then more.', { detail: 'e.g.' }), 'e.g. then more.', `${f.name}: a value's stop stays where the template does not put a full stop after it`)
    /* commit G p1p2-followups, p2 verifier F6: this rendered '"All tests pass.".', a full stop, a closing quotation mark
     * and another full stop. The value is still whole; the template's own stop gives way to the value's. */
    assert.equal(f('"{detail}".', { detail: 'All tests pass.' }), '"All tests pass."', `${f.name}: a quote mark between the slot and the stop keeps the value whole, and a value that ends its sentence takes the template's stop`)
  }
})

test('(e) the stop boundary never deletes a recorded character: a command or path that ends in dots reads verbatim', () => {
  /* Fix pass 2, verifier finding 1: dropping the whole trailing run rendered `cd ..` as "cd.", `ls -la .` as
   * "ls -la." and `git diff main..` as "git diff main.", and refused a path `..` as empty. */
  const body = 'This is a command run in the terminal — {detail}. Commands are the strongest proof.'
  for (const f of [fill, compose]) {
    for (const [detail, shown] of [
      ['Ran cd ..', 'Ran cd ..'],
      ['Ran ls -la .', 'Ran ls -la .'],
      ['Ran git diff main..', 'Ran git diff main..'],
      ['waiting...', 'waiting...'],
      ['Ran ls -la . ', 'Ran ls -la .'],
      ['Ran cargo build', 'Ran cargo build.'],
      /* commit G p1p2-followups, p1 verifier pass 3 P3-03: a value with no closing stop loses its trailing blanks too, or
       * `cargo build ` renders "Ran cargo build . Commands". `compose` trims every value first; `fill` is what this row guards. */
      ['Ran cargo build ', 'Ran cargo build.'],
      /* …all of them, of any kind, and a long run of them, on both sides of the stop test: a fill that dropped only one
       * blank passed the row above (p1p2-followups pass-1 verifier, break F1b). */
      ['Ran cargo build \t \n ', 'Ran cargo build.'],
      [`Ran cargo build${' \t\n'.repeat(40)}`, 'Ran cargo build.'],
      ['Ran ls -la . \t \n ', 'Ran ls -la .'],
      [`Ran ls -la .${' \t\n'.repeat(40)}`, 'Ran ls -la .'],
    ]) {
      assert.equal(f(body, { detail }), `This is a command run in the terminal — ${shown} Commands are the strongest proof.`, `${f.name}: ${JSON.stringify(detail)}`)
    }
    assert.equal(f('the tests weakened — removed in {path}.', { path: '..' }), 'the tests weakened — removed in ..', `${f.name}: a path of two stops is a path`)
  }
})

test('(e) compose: a null value is a missing value, and emptiness is judged once the template has taken its stop, so a value of one stop is refused rather than rendered " ."', () => {
  /* Fix pass 2, verifier finding 5 (breaks VP05 and VP06 survived). */
  const slotError = (kind, slot) => (e) => e instanceof SlotError && e.kind === kind && e.slot === slot &&
    e.message === `compose: ${kind === 'empty_slot' ? 'empty' : 'missing'} placeholder value for {${slot}}`
  assert.throws(() => compose('the opposite — {detail}.', { detail: null }), slotError('missing_slot', 'detail'), 'null is missing, never "null"')
  assert.throws(() => compose('the opposite — {detail}.', { detail: '.' }), slotError('empty_slot', 'detail'), 'a lone stop leaves nothing once the template takes it')
  assert.throws(() => compose('the opposite — {detail}.', { detail: ' . ' }), slotError('empty_slot', 'detail'), 'a blank-padded lone stop too')
})

test('(e) a value that ends its own sentence inside the template\'s quotation marks takes the place of the full stop the template puts after them: never a stop after a quotation that already ended', () => {
  /* commit G p1p2-followups, p2 verifier F6: "…in `src/cli.rs`.". What you asked for…" in a clicked body, and
   * "Earlier you said "…in a follow-up.". Is that done?" in a Do prompt. The value reads as recorded; only the stop the
   * template puts after its closing quotation mark gives way. */
  const tpl = 'Earlier you said "{claim_quote}". Is that done?'
  for (const f of [fill, compose]) {
    for (const [value, shown] of [
      ['I will add tests.', '"I will add tests."'],
      ['Is the changelog next?', '"Is the changelog next?"'],
      ['Done!', '"Done!"'],
      ['keep cli…', '"keep cli…"'],
      ['fix the totals', '"fix the totals".'],
      ['bump to v1.2', '"bump to v1.2".'],
    ]) {
      assert.equal(f(tpl, { claim_quote: value }), `Earlier you said ${shown} Is that done?`, `${f.name}: ${JSON.stringify(value)}`)
    }
    assert.equal(f('The agent wrote: "{claim_quote}" — {when}', { claim_quote: 'Done.', when: 'step 3' }), 'The agent wrote: "Done." — step 3', `${f.name}: no stop after the closing quotation mark, so nothing gives way`)
  }
  const { composeQuoted } = narrate
  const record = (ref) => ({ ev8: 'Rename the flag in `src/cli.rs`.', ev9: 'fix the totals' }[ref])
  const body = 'This is your own message to the AI: "{prompt_quote}". What you asked for is measured.'
  assert.deepEqual(composeQuoted(body, {}, { slot: 'prompt_quote', text: 'Rename the flag in `src/cli.rs`.', ref: 'ev8' }, record), {
    text: 'This is your own message to the AI: "Rename the flag in `src/cli.rs`." What you asked for is measured.',
    authored: 'This is your own message to the AI:  What you asked for is measured.',
    parts: [{ text: 'This is your own message to the AI: ' }, { quote: 'Rename the flag in `src/cli.rs`.', speaker: 'user' }, { text: ' What you asked for is measured.' }],
    quote: 'Rename the flag in `src/cli.rs`.', slots: [], violation: null,
  })
  assert.equal(composeQuoted(body, {}, { slot: 'prompt_quote', text: 'fix the totals', ref: 'ev9' }, record).text, 'This is your own message to the AI: "fix the totals". What you asked for is measured.', 'a quotation that does not end its sentence keeps the template\'s stop')
})

test('(d) grounding with supplied chips still reads every bracketed word of the sentence: a banned word, a number and a manager-register path are flagged', () => {
  /* Fix pass 2, verifier finding 2: cutting bracket tokens out of the text before these checks, even with chips
   * supplied (break VP07), let "marked [lazy] by the runner" through with no violation. */
  const lazy = 'Not proven: the record shows the opposite — marked [lazy] by the runner.'
  assert.deepEqual(ground(lazy, { refs: ['f1'], numbers: [], register: 'operator', chips: ['f1'] }), { ok: false, role: 'grounded', text: lazy, violations: [{ kind: 'banned_token', token: 'lazy', word: '[lazy]' }] })
  assert.deepEqual(ground('Not proven: the record shows the opposite — exit [42] on build.', { refs: ['f1'], numbers: [], register: 'operator', chips: ['f1'] }).violations, [{ kind: 'unlicensed_number', number: '42' }])
  assert.deepEqual(ground('Not proven: the record shows the opposite — output of [src/app/main.rs] unparsed.', { refs: ['f1'], numbers: [], register: 'manager', chips: ['f1'] }).violations, [{ kind: 'path_in_manager_register', token: '[src/app/main.rs]' }])
})

test('(e) the stop boundary costs time linear in the value: a long run of stops or blanks that does not end the value is not rescanned', () => {
  /* Fix pass 2, verifier finding 4: /[\s.]+$/ took 1,694 ms on a 60,000-character run followed by one more
   * character. 100,000 characters stay far under the bound when linear, and far over it when quadratic. */
  const tpl = 'output — {detail}. Next.'
  for (const detail of ['.'.repeat(100000) + 'F', ' '.repeat(100000) + 'x']) {
    for (const [f, shown] of [[fill, detail], [compose, detail.trim()]]) {
      const t0 = performance.now()
      const text = f(tpl, { detail })
      const ms = performance.now() - t0
      assert.equal(text, `output — ${shown}. Next.`, `${f.name}: the value is kept whole`)
      assert.ok(ms < 1000, `${f.name}: ${Math.round(ms)} ms for ${detail.length} characters`)
    }
  }
})

test('(d) grounding: refs the engine supplies as chips are the only refs; a bracket in the sentence is prose', () => {
  /* The template path appends chips itself, so a bracket inside a slot value
   * (a route directory, a reason) is text, never a ref (STEP0 audit §1.2 [C]). */
  const s = 'Not proven: the record shows the tests weakened — removed in app/[slug]/page.test.tsx.'
  assert.deepEqual(ground(s, { refs: ['f1'], numbers: [], register: 'operator', chips: ['f1'] }), { ok: true, role: 'grounded', text: s, violations: [] })
  assert.deepEqual(ground(s, { refs: ['f1'], numbers: [], register: 'operator', chips: ['f9'] }).violations, [{ kind: 'unknown_ref', id: 'f9' }], 'a supplied chip is still checked against the record')
  assert.equal(ground(s, { refs: ['f1'], numbers: [], register: 'operator', chips: [] }).role, 'inference', 'no supplied chip is an inference, whatever brackets the text holds')
  assert.deepEqual(ground(`${s} [f1]`, { refs: ['f1'], numbers: [], register: 'operator' }).violations, [{ kind: 'unknown_ref', id: 'slug' }], 'without supplied chips the text is read for refs, as before')
})

/* ── quotations (STEP0 audit §1.2, §3.4; commit G, p2-quotations) ───────
 * The Said row rendered the claim text raw and the clicked body filled the user's own prompt into a grounded sentence.
 * A quotation is someone's recorded words: carried verbatim as its own part, never composed into ThruLine's sentence,
 * and shown only when it is the text of the record its ref resolves to. */
const QUOTE_PROBE = 'The agent lied; all 14 tests pass [c9] {n}'

test('(g) composeQuoted: the template\'s own words compose; the quotation is carried verbatim as its own part and composes nothing', () => {
  const { composeQuoted } = narrate
  const record = (ref) => (ref === 'clm' ? QUOTE_PROBE : ref === 'ev8' ? QUOTE_PROBE : undefined)
  assert.deepEqual(composeQuoted('The agent wrote: "{claim_quote}" — {when}', { when: ' step 3 ', claim_quote: 'never read from values' }, { slot: 'claim_quote', text: QUOTE_PROBE, ref: 'clm' }, record), {
    text: `The agent wrote: "${QUOTE_PROBE}" — step 3`,
    authored: 'The agent wrote:  — step 3',
    parts: [{ text: 'The agent wrote: ' }, { quote: QUOTE_PROBE, speaker: 'agent' }, { text: ' — step 3' }],
    quote: QUOTE_PROBE, slots: ['when'], violation: null,
  })
  assert.deepEqual(composeQuoted('This is your own message to the AI: "{prompt_quote}". What you asked for is measured.', {}, { slot: 'prompt_quote', text: QUOTE_PROBE, ref: 'ev8' }, record), {
    text: `This is your own message to the AI: "${QUOTE_PROBE}". What you asked for is measured.`,
    authored: 'This is your own message to the AI: . What you asked for is measured.',
    parts: [{ text: 'This is your own message to the AI: ' }, { quote: QUOTE_PROBE, speaker: 'user' }, { text: '. What you asked for is measured.' }],
    quote: QUOTE_PROBE, slots: [], violation: null,
  })
  assert.deepEqual(composeQuoted('The AI reported {claim_kind} at {when}.', { claim_kind: 'task complete', when: 'step 3' }, { slot: 'claim_quote', text: QUOTE_PROBE, ref: 'clm' }, record), {
    text: 'The AI reported task complete at step 3.', authored: 'The AI reported task complete at step 3.', parts: [{ text: 'The AI reported task complete at step 3.' }], quote: null, slots: ['claim_kind', 'when'], violation: null,
  }, 'a template that quotes nothing shows nothing of the quote')
})

test('(g) composeQuoted refuses a quotation it cannot show: an unresolved ref, words that are not the record\'s, an empty quote, or no quote for the slot the template quotes', () => {
  const { composeQuoted } = narrate
  const tpl = 'The agent wrote: "{claim_quote}" — {when}'
  const record = (ref) => ({ clm: QUOTE_PROBE, blank: '  ' }[ref])
  const q = (text, ref, slot = 'claim_quote') => ({ slot, text, ref })
  assert.deepEqual(composeQuoted(tpl, { when: 'step 3' }, q('All tests pass.', 'clm'), record), { violation: { kind: 'quote_mismatch', slot: 'claim_quote', ref: 'clm' } })
  assert.deepEqual(composeQuoted(tpl, { when: 'step 3' }, q(`${QUOTE_PROBE} `, 'clm'), record), { violation: { kind: 'quote_mismatch', slot: 'claim_quote', ref: 'clm' } }, 'equal means equal: a trailing blank is a different text')
  assert.deepEqual(composeQuoted(tpl, { when: 'step 3' }, q(QUOTE_PROBE, 'clm9'), record), { violation: { kind: 'quote_ref_unresolved', slot: 'claim_quote', ref: 'clm9' } })
  assert.deepEqual(composeQuoted(tpl, { when: 'step 3' }, q(QUOTE_PROBE, undefined), record), { violation: { kind: 'quote_ref_unresolved', slot: 'claim_quote', ref: null } })
  assert.deepEqual(composeQuoted(tpl, { when: 'step 3' }, q(QUOTE_PROBE, 'constructor'), record), { violation: { kind: 'quote_ref_unresolved', slot: 'claim_quote', ref: 'constructor' } }, 'an inherited name resolves to no record')
  assert.deepEqual(composeQuoted(tpl, { when: 'step 3' }, q('  ', 'blank'), record), { violation: { kind: 'empty_quote', slot: 'claim_quote', ref: 'blank' } })
  assert.deepEqual(composeQuoted(tpl, { when: 'step 3' }, q(QUOTE_PROBE, 'clm', 'prompt_quote'), record), { violation: { kind: 'missing_quote', slot: 'claim_quote' } })
  assert.deepEqual(composeQuoted(tpl, { when: 'step 3' }, null, record), { violation: { kind: 'missing_quote', slot: 'claim_quote' } })
  /* A quotation slot written without its quotation marks is authored text, and its value is never authored. */
  const slotError = (kind, slot) => (e) => e instanceof SlotError && e.kind === kind && e.slot === slot
  assert.throws(() => composeQuoted('The agent wrote {claim_quote}.', { claim_quote: QUOTE_PROBE }, q(QUOTE_PROBE, 'clm'), record), slotError('missing_slot', 'claim_quote'))
  assert.throws(() => composeQuoted('"{claim_quote}" and "{claim_quote}"', {}, q(QUOTE_PROBE, 'clm'), record), slotError('missing_slot', 'claim_quote'), 'one quotation per template')
  assert.throws(() => composeQuoted(tpl, { when: '' }, q(QUOTE_PROBE, 'clm'), record), slotError('empty_slot', 'when'), 'the template\'s own slots compose as compose does')
  assert.deepEqual({ ...narrate.QUOTE_SLOTS }, { claim_quote: 'agent', prompt_quote: 'user' })
  assert.ok(Object.isFrozen(narrate.QUOTE_SLOTS))
})

test('(g) the copy writes every quotation as one, and every clicked body that can fail has a body that names the node and quotes nothing', () => {
  const slots = Object.keys(narrate.QUOTE_SLOTS || {})
  assert.deepEqual(slots, ['claim_quote', 'prompt_quote'])
  for (const [path, text] of STRINGS) {
    const bare = placeholders(text.replace(/"\{(?:claim_quote|prompt_quote)\}"/g, '')).filter((p) => slots.includes(p))
    assert.deepEqual(bare, [], `${path} writes a quotation slot without its quotation marks: ${text}`)
  }
  for (const [type, card] of Object.entries(HQ_COPY.clicked)) {
    if (type === 'link') continue
    if (!placeholders(card.body).length) { assert.equal(card.unquoted, undefined, `clicked.${type}: a body with no slot cannot fail on one`); continue }
    assert.equal(typeof card.unquoted, 'string', `clicked.${type}.body has a slot and no unquoted body to fall back to`)
    assert.deepEqual(placeholders(card.unquoted), [], `clicked.${type}.unquoted carries a slot`)
    assert.match(card.unquoted, /^This is /, `clicked.${type}.unquoted says what the node is`)
  }
  assert.equal(HQ_COPY.clicked.note.unquoted, undefined)
  assert.equal(HQ_COPY.clicked.user_prompt.body, 'This is your own message to the AI: "{prompt_quote}". What you asked for is measured against what the AI reported done.')
  assert.equal(HQ_COPY.clicked.ask.body, 'This is a request from you: "{prompt_quote}". ThruLine measures what the AI reported done against it.')
  for (const reg of ['manager', 'operator']) assert.deepEqual(placeholders(HQ_COPY.chrome.said_unquoted[reg]), ['when'], `chrome.said_unquoted.${reg}`)
  assert.deepEqual(Object.keys(HQ_COPY.chrome.quote_label).sort(), [...new Set(Object.values(narrate.QUOTE_SLOTS))].sort(), 'a label for each speaker a quotation can have')
})

test('(g) the Said row exists to quote: every family\'s said template quotes the agent\'s words in both registers, and the Said line that quotes nothing points at no record it has no chip for', () => {
  /* commit G p1p2-followups. p2 coder loose thread 1: the Plain said templates of the generic families quoted nothing, so
   * open promises and Fog claims showed no agent words in Plain. p2 verifier F5: the last-resort Said line read "its
   * words are in the record" / "see the record", and it renders with no chip when the ref resolves to nothing. */
  for (const [key, regs] of Object.entries(HQ_COPY.families)) {
    for (const reg of ['manager', 'operator']) assert.equal(regs[reg].said.split('"{claim_quote}"').length - 1, 1, `${key}.${reg}.said quotes nothing: ${regs[reg].said}`)
  }
  assert.deepEqual({ ...HQ_COPY.chrome.said_unquoted }, {
    manager: 'The AI wrote about this at {when}; its words are not shown here.',
    operator: 'The agent wrote about this at {when}; its words are not shown here.',
  })
  for (const reg of ['manager', 'operator']) assert.doesNotMatch(HQ_COPY.chrome.said_unquoted[reg], /\brecord\b/, `chrome.said_unquoted.${reg} points at the record, and it can render with no chip`)
})

test('(g) a quotation\'s label names whose words they are in each register\'s own word, and a quotation that is only part of what was written never says "as recorded"', () => {
  /* commit G p1p2-followups. p2 coder loose thread 3: "The AI's words" in Technical, whose copy says "agent". p2 verifier
   * F3: a user prompt as the timeline holds it is the message's first line, shortened when long, and was titled "Your
   * words, as recorded". */
  assert.deepEqual(JSON.parse(JSON.stringify(HQ_COPY.chrome.quote_label)), {
    agent: { manager: "The AI's words, as recorded", operator: "The agent's words, as recorded" },
    user: { manager: 'Your words, as recorded', operator: 'Your words, as recorded' },
  })
  assert.deepEqual(JSON.parse(JSON.stringify(HQ_COPY.chrome.quote_label_excerpt)), {
    user: { manager: 'The first line of your message, shortened if it was long', operator: 'The first line of your message, shortened if it was long' },
  })
  for (const reg of ['manager', 'operator']) assert.doesNotMatch(HQ_COPY.chrome.quote_label_excerpt.user[reg], /as recorded|your words/i, `chrome.quote_label_excerpt.user.${reg} claims the whole message`)
})

test('(f) exactly five state words, Fog subtitled "Recording gap"', () => {
  assert.deepEqual(Object.keys(HQ_COPY.states), ['proven', 'not_proven', 'couldnt', 'loose', 'fog'])
  assert.deepEqual(Object.values(HQ_COPY.states).map((s) => s.word), ['Proven', 'Not proven', "Recorded", 'Loose thread', 'Fog'])
  assert.equal(HQ_COPY.states.fog.subtitle, 'Recording gap')
  assert.ok(Object.isFrozen(HQ_COPY) && Object.isFrozen(HQ_COPY.families['verdict/*'].manager))
})

/* T8 added the eighth: `workspace_unreadable` is backed by the engine's typed
 * `unobservable_cause` (binding.rs `UnobservableCause::WorkspaceUnreadable`). */
test('finding kinds are exactly the engine-backed eight', () => {
  assert.deepEqual(Object.keys(HQ_COPY.kinds), [
    'said_done_no_proof', 'record_says_otherwise', 'command_failed_then_done', 'tests_weakened', 'left_behind', 'couldnt_check', 'recording_gap',
    'workspace_unreadable',
  ])
  for (const label of Object.values(HQ_COPY.kinds)) assert.doesNotMatch(label, /Instruction not followed|Delivered less|Security|Structure/)
})

test('pick walks specific → general and every state has a guaranteed fallback', () => {
  for (const state of Object.keys(HQ_COPY.states)) assert.ok(HQ_COPY.families[`verdict/*/${state}`], state)
  assert.equal(pick(HQ_COPY.families, verdictKeys('tests_weakened', 'not_proven')).key, 'verdict/tests_weakened/not_proven')
  assert.equal(pick(HQ_COPY.families, verdictKeys('no_such_kind', 'not_proven')).key, 'verdict/*/not_proven')
  assert.equal(pick(HQ_COPY.families, verdictKeys(null, 'nope')).key, 'verdict/*')
  assert.throws(() => pick(HQ_COPY.families, ['missing']), /no family/)
  for (const [key, regs] of Object.entries(HQ_COPY.families)) {
    for (const reg of ['manager', 'operator']) {
      assert.deepEqual(Object.keys(regs[reg]).slice(0, 3), ['said', 'record', 'do'], `${key}.${reg}`)
      const prompt = regs[reg].prompt
      if (prompt) assert.match(prompt, /paste/, `${key}.${reg}.prompt asks for output`)
    }
  }
  for (const [type, card] of Object.entries(HQ_COPY.clicked)) {
    if (type === 'link') continue
    assert.match(card.title, /^You clicked: /, type)
    assert.match(card.body, /^This is /, `${type} body says what the thing is`)
  }
  assert.equal(HQ_COPY.clicked.link, 'Show raw record →')
})

test('docs/HQ-COPY.md equals the export on disk', () => {
  const md = renderCopyMarkdown()
  assert.equal(readFileSync(COPY_MD, 'utf8'), md, 'regenerate: node ui/src/workspace/hq-copy-export.mjs > docs/HQ-COPY.md')
  for (const [, text] of onVerdictTab) assert.ok(md.includes(text), `export is missing: ${text}`)
})

/* ── T-2-4: the Change workspace's own copy fence ─────────────────────────
 *
 * The Change screen is the first surface a person meets, so its strings are
 * held to 03-USER-LENS.md §3 rules 2, 3, 5 and 6 mechanically rather than by
 * review. Each test below is one of those rules.
 */

const CHANGE_STRINGS = STRINGS.filter(([path]) => path.startsWith('change.'))

/* Rule 2's banned list, as words on a Plain surface. `receipt` is banned as a
 * count, so the pattern is a number or "N" beside it; the bare noun is not
 * used on this surface at all, which is the simpler thing to hold. */
const PLAIN_BANNED = [
  /\bevents?\b/i,
  /\breceipts?\b/i,
  /\bcandidates?\b/i,
  /\bapplicability\b/i,
  /\bdispositions?\b/i,
  /\bpacket\b/i,
  /\bledger\b/i,
  /\bsession id\b/i,
  /\btree hash\b/i,
  /\bpredicates?\b/i,
]

test('(t24) no banned word of the human vocabulary is in the Change workspace copy', () => {
  assert.ok(CHANGE_STRINGS.length > 60, `expected the whole section, found ${CHANGE_STRINGS.length}`)
  for (const [path, text] of CHANGE_STRINGS) {
    for (const re of PLAIN_BANNED) {
      assert.doesNotMatch(text, re, `${path} uses a word Sam has never heard: ${text}`)
    }
  }
})

test('(t24) the banned-word sweep is a live instrument', () => {
  for (const bad of ['3 events in view', 'two receipts', 'the candidate tree', 'packet v2', 'read from the ledger', 'session id 44', 'tree hash e71c', 'its applicability', 'the disposition', 'a predicate']) {
    assert.ok(PLAIN_BANNED.some((re) => re.test(bad)), `sweep misses: ${bad}`)
  }
  for (const good of ['the version this Change is about', 'what counts as done', 'the work you confirmed']) {
    assert.ok(!PLAIN_BANNED.some((re) => re.test(good)), `sweep false-positives: ${good}`)
  }
})

test('(t24) no conclusion sentence is a state word, a percentage, or an all-clear', () => {
  const five = Object.values(HQ_COPY.states).map((s) => s.word)
  for (const [key, text] of Object.entries(HQ_COPY.change.conclusion)) {
    for (const word of five) {
      assert.ok(!text.includes(word), `conclusion.${key} renders the state word "${word}": ${text}`)
    }
    assert.doesNotMatch(text, /\d\s*%|\bpercent\b/i, `conclusion.${key} carries a percentage: ${text}`)
    assert.doesNotMatch(
      text,
      /\b(all clear|everything checks out|ready to (?:ship|merge)|bug-free|production-safe|looks good|all good)\b/i,
      `conclusion.${key} declares an all-clear: ${text}`,
    )
    assert.doesNotMatch(text, /\bgreen\b/i, `conclusion.${key} says green: ${text}`)
  }
})

test('(t24) every stage has a name, a question and a sentence saying what would put something in it', () => {
  const order = ['define', 'prepare', 'build', 'verify', 'review', 'approve']
  assert.deepEqual(Object.keys(HQ_COPY.change.stages).filter((k) => k !== 'strip_label' && k !== 'views_only'), order)
  for (const key of order) {
    const stage = HQ_COPY.change.stages[key]
    assert.equal(typeof stage.name, 'string')
    assert.match(stage.question, /\?$/, `stages.${key}.question is not a question`)
    assert.ok(stage.empty.length > 30, `stages.${key}.empty teaches nothing: ${stage.empty}`)
  }
})

test('(t24) every disabled reason for Approve carries what would enable it', () => {
  const d = HQ_COPY.change.approve.disabled
  assert.ok(Object.keys(d).length >= 4)
  for (const [key, pair] of Object.entries(d)) {
    assert.deepEqual(Object.keys(pair).sort(), ['enabler', 'reason'], `approve.disabled.${key} is not a reason/enabler pair`)
    assert.ok(pair.reason.trim().length > 10, `approve.disabled.${key}.reason says nothing`)
    assert.ok(pair.enabler.trim().length > 10, `approve.disabled.${key}.enabler says nothing`)
  }
})

test('(t24) the reason Approve is off never asks for something that would not turn it on', () => {
  /* Measured on a real screen (T24-6): a Change with its work confirmed, no
     version bound, and an enabler reading "Link the work that was done to this
     Change." The work was linked. Nothing in this build binds a version —
     `change::append_implementation` has no caller outside tests — so the
     enabler named an act that could not enable the control. */
  const d = HQ_COPY.change.approve.disabled
  assert.doesNotMatch(d.no_version.enabler, /link the work/i, 'linking work does not bind a version; nothing in this build does')
  assert.match(d.no_version.enabler, /bound to this Change/, 'the enabler stopped naming what would actually record a version')
})

/* T-5-3. This test used to assert that "Approved with exceptions" was rendered
 * NOWHERE, because the build could not make that decision. T-5-3 built it —
 * the two write routes, `thruline change waive`, the exceptions bound onto the
 * approval — so the assertion is inverted rather than deleted: the label now
 * has to exist, and it has to be the decision family's, not a stray sentence
 * somewhere else. The inversion is recorded here rather than made silently. */
test('(t24) "Approved with exceptions" exists, and only in the decision copy that records one', () => {
  const said = STRINGS.filter(([, text]) => /approved with exceptions/i.test(text))
  assert.ok(said.length >= 2, 'the label a T-5-3 approval carries has to be sayable')
  for (const [path] of said) {
    assert.match(path, /^change\.model\.decision\./, `${path} is not the decision's own copy`)
  }
  /* And the ordinary label is still there, for the ordinary act. */
  assert.equal(HQ_COPY.change.model.decision.approved, 'Approved on {version}')
})

test('(t24) every other disabled control on the Change screen also carries what would enable it', () => {
  const b = HQ_COPY.change.blocked
  assert.ok(Object.keys(b).length >= 4)
  for (const [key, pair] of Object.entries(b)) {
    assert.deepEqual(Object.keys(pair).sort(), ['enabler', 'reason'], `blocked.${key} is not a reason/enabler pair`)
    assert.ok(pair.reason.trim().length > 10 && pair.enabler.trim().length > 10, `blocked.${key} says nothing`)
  }
  /* One label per stage's one primary action, and no more: a second label here
   * is a second primary action waiting to be rendered. */
  /* AI-REVIEW-OUT-OF-LAUNCH (R-2026-09-28-20): the Review stage's one primary
   * depends on the build. With AI review it is `ask_for_review` (off, and it
   * says why); without it — the launch build — it is
   * `see_what_you_would_approve`, and "Ask for a review" is not rendered at
   * all. Still exactly one per stage, per build. */
  assert.deepEqual(Object.keys(HQ_COPY.change.actions).sort(), ['accept_criteria', 'ask_for_review', 'copy_prompt', 'see_what_changed', 'see_what_we_checked', 'see_what_you_would_approve'])
})

test('BUILD-2 run help names the existing Check it again door and keys exclude decisions', () => {
  const blocked = HQ_COPY.change.model.blocked
  const keys = ['needs_recheck', 'not_proven', 'recorded', 'in_progress']
  assert.equal(HQ_COPY.run.door, 'Check it again')
  const screen = readFileSync(join(dirname(COPY_MD), '../ui/src/workspace/change-workspace.jsx'), 'utf8')
  assert.match(screen, /<RunDoor changeId=/, 'the named control must exist on the Change screen')
  for (const key of keys) {
    const pair = blocked[key]
    assert.doesNotMatch(pair.reason, /not something this version of ThruLine can start/i, key)
    assert.match(pair.enabler, /Check it again/, key)
    assert.match(pair.enabler, /Brief or Inspector/, key)
    assert.doesNotMatch(pair.enabler, /Run the check yourself/, key)
  }
  assert.doesNotMatch(HQ_COPY.shell.keys.primary, /^Press the one main action on this screen$/)
  assert.match(HQ_COPY.shell.keys.primary, /Approve or Withdraw/)
  assert.match(HQ_COPY.shell.notes.approve_by_hand, /never done with a key/)
})

/* ── T-4-2: where each thing review found stands ─────────────────────────
 *
 * `B9.11`: the standings of a review finding are not the claim-verdict
 * vocabulary, and the claim-verdict vocabulary never describes a finding's
 * standing. Pinned as a golden (the seven labels, word for word) and then
 * held both ways over the whole copy file.
 */
const STANDING_LABELS_GOLDEN = Object.freeze({
  open: 'Still open',
  under_repair: 'Being repaired',
  awaiting_verification: 'Waiting for a re-check',
  fixed_with_evidence: 'Fixed',
  disputed: 'Disputed',
  informational: 'For information only',
  waived: 'Waived',
})

/* The claim side: the five state words, their subtitles, and every sentence
 * the vocabulary gives a claim verdict — and the Change screen's five ruled
 * state words (`change.model.states`, 03-USER-LENS §3: Proven · Not proven ·
 * In progress · Recorded · Needs a re-check), which are badges and nothing
 * else (R-2026-09-26-3). Before ADV-review-input-3 this list read only
 * `HQ_COPY.states`, so "In progress" and "Needs a re-check" were never
 * fenced, and a standing sentence borrowed one. */
function claimVerdictWords() {
  const out = []
  for (const s of Object.values(HQ_COPY.states)) out.push(s.word, s.subtitle)
  for (const w of Object.values(HQ_COPY.change.model.states)) out.push(w)
  for (const v of Object.values(HQ_COPY.vocabulary)) {
    if (v && typeof v.panel === 'string' && Object.values(HQ_COPY.states).some((s) => s.word === v.panel)) {
      out.push(v.panel)
      if (v.record_line) out.push(v.record_line)
      if (v.subtitle) out.push(v.subtitle)
    }
  }
  return [...new Set(out)]
}

const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
const phrase = (p) => new RegExp(`(^|[^A-Za-z])${escapeRe(p)}([^A-Za-z]|$)`, 'i')

test('(t42) the seven standing labels are exactly the golden, in the technical order', () => {
  assert.deepEqual({ ...HQ_COPY.change.review.standing.labels }, STANDING_LABELS_GOLDEN)
  assert.deepEqual(Object.keys(HQ_COPY.change.review.standing.labels), ['open', 'under_repair', 'awaiting_verification', 'fixed_with_evidence', 'disputed', 'informational', 'waived'])
})

test('(t42) no state word and no claim-verdict word describes a finding standing', () => {
  const standing = STRINGS.filter(([path]) => path.startsWith('change.review.standing.'))
  assert.ok(standing.length > 60, `expected the whole standing section, found ${standing.length}`)
  const words = claimVerdictWords()
  assert.ok(words.includes('Proven') && words.includes('Not proven') && words.includes('Fog'), 'the claim side was read')
  assert.ok(words.includes('In progress') && words.includes('Needs a re-check') && words.includes('Recorded'), 'the ruled state words were read')
  for (const [path, text] of standing) {
    for (const w of words) assert.doesNotMatch(text, phrase(w), `${path} borrows the claim word "${w}": ${text}`)
    /* The roots, too: a finding is fixed or not, and "proof" is the claim side's noun. */
    assert.doesNotMatch(text, /\bprov(e|en|ed|es|ing)\b|\bproof\b/i, `${path} uses the claim side's noun: ${text}`)
  }
})

test('(t42) no finding-standing label describes a claim', () => {
  const claimSide = STRINGS.filter(([path]) => VERDICT_SECTIONS.some((s) => path.startsWith(s + '.')) || path.startsWith('vocabulary.'))
  assert.ok(claimSide.length > 100)
  for (const label of Object.values(STANDING_LABELS_GOLDEN)) {
    for (const [path, text] of claimSide) assert.doesNotMatch(text, phrase(label), `${path} uses the standing label "${label}": ${text}`)
  }
  /* And the two sets are disjoint as words. */
  const five = Object.values(HQ_COPY.states).map((s) => s.word.toLowerCase())
  for (const label of Object.values(STANDING_LABELS_GOLDEN)) assert.ok(!five.includes(label.toLowerCase()), label)
})

test('(t42) the golden sweep is a live instrument', () => {
  const words = claimVerdictWords()
  for (const bad of ['Fixed — Proven on version 2', 'Not proven yet', 'Loose thread, waived', 'proof of the fix', 'The version has moved since, so it needs a re-check.', 'In progress, waived']) {
    assert.ok(words.some((w) => phrase(w).test(bad)) || /\bprov(e|en|ed|es|ing)\b|\bproof\b/i.test(bad), `sweep misses: ${bad}`)
  }
})

test('(t42) every disabled standing control carries what would enable it, and every refusal has a sentence', () => {
  const S = HQ_COPY.change.review.standing
  for (const [key, pair] of Object.entries(S.blocked)) {
    assert.deepEqual(Object.keys(pair).sort(), ['enabler', 'reason'], `standing.blocked.${key}`)
    assert.ok(pair.reason.trim().length > 10 && pair.enabler.trim().length > 10, `standing.blocked.${key} says nothing`)
  }
  for (const code of ['no_such_finding', 'finding_not_readable', 'moved', 'unchanged', 'missing', 'evidence_required', 'evidence_not_for_this', 'mark_goes_no_further', 'mark_needs_an_open_finding', 'evidence_not_found', 'evidence_not_finished', 'evidence_not_current', 'evidence_not_later', 'evidence_same_code', 'evidence_still_raised', 'evidence_raised_elsewhere', 'evidence_not_passing', 'evidence_no_place', 'evidence_outside_scope', 'too_long', 'other']) {
    assert.ok(typeof S.refused[code] === 'string' && S.refused[code].length > 10, `no sentence for ${code}`)
  }
  /* A resolved mark reads as waiting, never as fixed, in its own words. */
  assert.match(S.meanings.awaiting_verification, /^Marked resolved — nothing yet shows it was fixed/)
  assert.doesNotMatch(S.labels.awaiting_verification, /^Fixed/)
})
