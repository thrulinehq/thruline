/* T-S-3 — the one derived view-model, and the fence around it.
 *
 * Acceptance (a) is the agreement test: one record, one state change, and the
 * headline, the Inspector's block 2, the Inbox row and the packet criterion
 * line must all say the same state word and name the same version. Acceptance
 * (b) is the grep: no screen may construct one of the five state words.
 */

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

import {
  deriveChangeModel,
  STATE_WORDS,
  STATE_WORD_SET,
  criterionState,
  stateLine,
  versionSentence,
} from './change-model.js'
import { HQ_COPY } from './hq-copy.js'

const HERE = dirname(fileURLToPath(import.meta.url))

/* ── the fixture: one Change, one criterion, one run that proved it ────── */

const CHANGE = {
  id: 'ch1',
  project_id: 'p1',
  request: { title: 'Totals add up on the checkout page' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1,
    criteria: ['The totals add up'],
    allowed_scope: ['checkout/'],
    check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  examination: 'none_yet',
}

const item = (over) => ({
  link_id: 'l1',
  evidence_id: 'e1',
  session_id: 's1',
  basis: 'declared',
  kind: 'check_run',
  name: 'tests pass',
  observed: 'proven',
  bears: true,
  applies: true,
  applies_slug: 'inputs_applies',
  ...over,
})

const coverage = (over) => ({
  scope_version: 1,
  link_rows: 1,
  criteria: [{ index: 1, text: 'The totals add up', state: 'evidence', items: [item(over)] }],
})

const APPLICABILITY_STALE = {
  items: [
    {
      link_id: 'l1',
      criterion_index: 1,
      version: 'e71c5b0',
      reason_slug: 'applicability_reason_source_changed',
      reason_paths: ['checkout/totals.py'],
    },
  ],
}

const proven = () => deriveChangeModel(CHANGE, coverage({ fresh: true, fresh_slug: 'applicability_still_applies' }), null, null, [])
const stale = () => deriveChangeModel(CHANGE, coverage({ fresh: false, fresh_slug: 'applicability_needs_a_recheck' }), APPLICABILITY_STALE, null, [])

/* ── the five states ──────────────────────────────────────────────────── */

test('each state the record can be in derives one of the five words, and no sixth', () => {
  assert.equal(proven().criteria[0].word, 'Proven')
  assert.equal(stale().criteria[0].word, 'Needs a re-check')

  const notProven = deriveChangeModel(CHANGE, coverage({ observed: 'not_proven' }), null, null, [])
  assert.equal(notProven.criteria[0].word, 'Not proven')

  const settledNothing = deriveChangeModel(CHANGE, coverage({ bears: false }), null, null, [])
  assert.equal(settledNothing.criteria[0].word, 'Recorded')

  const nothing = deriveChangeModel(
    CHANGE,
    { scope_version: 1, link_rows: 0, criteria: [{ index: 1, text: 'The totals add up', state: 'none', items: [] }] },
    null, null, [],
  )
  assert.equal(nothing.criteria[0].word, 'In progress')

  assert.equal(new Set(STATE_WORD_SET).size, 5, 'five state words, and no sixth')
})

test('an absent freshness assessment is not read as "still applies"', () => {
  /* The evidence has never been assessed. That is a fact about the record and
   * it must not become a claim that nothing has moved. */
  const m = deriveChangeModel(CHANGE, coverage({}), null, null, [])
  assert.equal(m.criteria[0].freshness, null)
  assert.equal(m.criteria[0].word, 'Proven')
})

/* ── acceptance (a): the four surfaces agree ──────────────────────────── */

test('one record changes state once, and the headline, the Inspector, the Inbox row and the packet line all say the same word and the same version', () => {
  const before = proven()
  assert.equal(before.headline.word, 'Proven')
  assert.equal(before.inspector.shows.word, 'Proven')
  assert.equal(before.inboxRow.word, 'Proven')
  assert.equal(before.packet.criteria[0].word, 'Proven')

  const after = stale()
  const surfaces = [
    after.headline.word,
    after.inspector.shows.word,
    after.inboxRow.word,
    after.packet.criteria[0].word,
  ]
  assert.deepEqual(surfaces, ['Needs a re-check', 'Needs a re-check', 'Needs a re-check', 'Needs a re-check'])

  /* Same version, named the same way, on all of them. */
  assert.equal(after.headline.version, 'e71c5b0')
  assert.equal(after.version.short, 'e71c5b0')
  assert.equal(after.packet.version, after.inboxRow.version)
  assert.match(after.inboxRow.version, /e71c5b0/)
  for (const line of [after.inspector.shows.line, after.packet.criteria[0].line, after.inboxRow.line]) {
    assert.match(line, /^Needs a re-check · version e71c5b0$/)
  }
})

test('the freshness sentence and its reason class are the engine’s words, and the reason is one level down', () => {
  const f = stale().criteria[0].freshness
  assert.equal(f.fresh, false)
  assert.equal(f.sentence, 'Proven on the previous version — needs a re-check.')
  assert.equal(f.reasonClass, 'applicability_reason_source_changed')
  assert.equal(f.reason, 'The affected code changed afterward.')
  assert.deepEqual(f.paths, ['checkout/totals.py'])
  /* The path is in Technical, never in the plain sentence. */
  assert.doesNotMatch(f.sentence, /checkout\/totals\.py/)
  assert.doesNotMatch(f.reason, /checkout\/totals\.py/)
  /* The path is in the technical half of the reason, which is the drawer. */
  assert.match(f.reasonTechnical, /checkout\/totals\.py/)
})

test('the "what changed since" line lists the reason classes in Sam’s words, once each', () => {
  assert.equal(stale().changedSince.text, 'Since this was checked: The affected code changed afterward.')
  assert.equal(proven().changedSince.any, false)
})

test('a still-applying criterion names the path and the version it was checked against', () => {
  const doc = { items: [{ link_id: 'l1', criterion_index: 1, version: 'e71c5b0', reason_paths: ['checkout/'] }] }
  const m = deriveChangeModel(CHANGE, coverage({ fresh: true, fresh_slug: 'applicability_still_applies' }), doc, null, [])
  assert.equal(m.criteria[0].freshness.sentence, 'Still applies — checkout/ unchanged since e71c5b0.')
})

/* ── §3.8: "Proven" never appears without scope and version ───────────── */

test('"Proven" is never rendered without its scope and its version', () => {
  const m = proven()
  assert.equal(m.criteria[0].line, 'Proven: tests pass · version e71c5b0 · checkout/')
  assert.deepEqual(m.criteria[0].proven, { version: 'e71c5b0', scope: ['checkout/'] })

  /* With nothing bound, the word may not stand alone as a claim. */
  const unbound = deriveChangeModel(
    { ...CHANGE, implementation: null },
    coverage({ fresh: true, fresh_slug: 'applicability_still_applies' }),
    null, null, [],
  )
  assert.equal(unbound.criteria[0].proven, null)
  assert.equal(unbound.criteria[0].line, 'Proven')
  assert.doesNotMatch(unbound.criteria[0].line, /:/)
  /* Bare is honest; a scope-and-version claim with neither behind it is not. */
  assert.equal(stateLine('proven', null, [], 'tests pass'), 'Proven')
})

/* ── the four-block Inspector ─────────────────────────────────────────── */

test('the Inspector is four blocks in the design’s order, with exactly one primary action that says what would enable it', () => {
  const m = stale()
  const i = m.inspector
  assert.deepEqual(
    [i.what.heading, i.shows.heading, i.settle.heading, i.action.heading],
    ['What this is', 'What the record shows', 'What would settle it', 'What to do'],
  )
  assert.equal(i.what.quote, 'The totals add up')
  assert.equal(i.settle.sentence, 'The same check, run again on the version this Change is about now.')
  assert.equal(i.action.label, 'Run the check again')
  assert.equal(i.action.enabled, false)
  assert.ok(i.action.reason && i.action.enabler, 'a disabled control names what would enable it (T-2-10)')

  /* One primary button, chosen by state — not two, and never none. */
  assert.equal(proven().inspector.action.label, 'Approve this version')
})

test('the disabled primary names the act on its OWN button, and only the Approve button says approve', () => {
  /* T-S-3 verifier finding 1: the reason used to be the Approve blocker's,
   * whatever the button said. The noun on the button must be the noun in the
   * sentence under it. */
  /* T-3-6-F3 (`Q2`): `not_proven` left this table. Its primary is no longer a
   * disabled button asking for a run that already happened — it is a LIVE door
   * to the line that broke, and a control that is on does not owe a sentence
   * about what would turn it on. The case below asserts that shape instead. */
  const NOUN = { needs_recheck: 'check', recorded: 'check', in_progress: 'check', proven: 'approv' }
  const cases = [
    ['needs_recheck', stale()],
    ['proven', proven()],
    ['recorded', deriveChangeModel(CHANGE, coverage({ bears: false }), null, null, [])],
    ['in_progress', deriveChangeModel(
      CHANGE,
      { scope_version: 1, link_rows: 0, criteria: [{ index: 1, text: 'The totals add up', state: 'none', items: [] }] },
      null, null, [],
    )],
  ]
  for (const [state, m] of cases) {
    const a = m.inspector.action
    assert.equal(m.state, state, `${state} fixture reaches its state`)
    assert.ok(a.reason && a.enabler, `${state}: a disabled control names what would enable it (T-2-10)`)
    const text = (a.reason + ' ' + a.enabler).toLowerCase()
    assert.match(text, new RegExp(NOUN[state]), `${state}: the reason names the act on its own button`)
    if (state !== 'proven') {
      assert.doesNotMatch(
        text,
        /approv/,
        `${state}: the button says "${a.label}" and the reason talks about approving`,
      )
    }
    /* And the button's own words are in the sentence's subject matter. */
    if (state !== 'proven') assert.match(a.label.toLowerCase(), /check/)
  }

  /* T-3-6-F3 (`Q2`, 03-USER-LENS.md rules 6 and 7). The one primary on a
   * Change whose record holds a failure is a door, it is ON, it says what it
   * opens, and it does not say "run". */
  const broken = deriveChangeModel(CHANGE, coverage({ observed: 'not_proven' }), null, null, [])
  assert.equal(broken.state, 'not_proven')
  assert.equal(broken.inspector.action.label, HQ_COPY.change.model.actions.look)
  assert.equal(broken.inspector.action.enabled, true, 'reading what broke is an act this build can do')
  assert.equal(broken.inspector.action.door, 1, 'the door names the line that broke')
  assert.doesNotMatch(broken.inspector.action.label.toLowerCase(), /run/)
})

/* ── the model is deterministic ───────────────────────────────────────── */

test('the same documents derive the same model, twice', () => {
  assert.deepEqual(stale(), stale())
})

/* ── acceptance (b): the fence ────────────────────────────────────────── */

/* P6-DESIGN-FIXES-13: `criterion-links.jsx` renders the accepted lines and,
 * since this card, the Change-level state word the headline carries. The T-S-3
 * coder noted it sat outside this fence; it is inside it now. */
const SCREENS = ['change-workspace.jsx', 'change-view.js', 'coverage-view.js', 'criterion-links.jsx', 'inbox.jsx', 'inbox-view.js', 'packet.jsx', 'packet-view.js']

test('no screen constructs a state word: the five words exist in change-model.js and hq-copy.js and nowhere else', () => {
  const offenders = []
  for (const file of SCREENS) {
    const src = readFileSync(join(HERE, file), 'utf8')
    for (const line of src.split('\n')) {
      /* Comments are prose about the rule, not renderings of it. */
      const code = line.replace(/\/\*.*?\*\//g, '')
      if (/^\s*[*]|^\s*\/\//.test(line)) continue
      for (const word of STATE_WORD_SET) {
        if (code.includes(`'${word}`) || code.includes(`"${word}`) || code.includes(`>${word}<`)) {
          offenders.push(`${file}: ${line.trim()}`)
        }
      }
    }
  }
  assert.deepEqual(offenders, [], 'a state word is being constructed outside change-model.js + hq-copy.js')
})

/* R8-VOCABULARY (ruling R-2026-09-23-8): the switch is all-or-nothing. The
 * retired word may not survive anywhere in the app's source — not as a state
 * word, not as a lowercase composition form, not in a comment that a later
 * reader would copy. The per-producing-state mapping is the table at the head
 * of docs/HQ-COPY.md; the words it kept (Loose thread, Fog) are not retired. */
const RETIRED_STATE_WORDS = [/couldn['\u2019]t check/i]

/* What a reader actually SEES, not what the source bytes are. Written inside a
 * single-quoted JS string with the apostrophe backslash-escaped, the retired
 * word is still the retired word byte-for-byte once the string is evaluated —
 * `node --test` prints it verbatim in every test name — yet neither the pattern
 * above nor `grep -F` matches the source line, because a backslash sits between
 * the `n` and the apostrophe. Three live test names in verdict-model.test.js
 * rode through this fence on exactly that gap (verifier round 1 under
 * R-2026-09-23-8). Normalising each line to its rendered form first closes it,
 * and closes the sibling escapes at the same time, so the next evasion has to
 * be invented rather than reached for. */
const rendered = (line) => line
  .replace(/\\u2019/gi, '’')
  .replace(/\\x27/gi, "'")
  .replace(/&#0*39;|&#x0*27;|&#0*8217;|&rsquo;|&apos;/gi, "'")
  .replace(/\\(.)/g, '$1')

const carriesRetiredWord = (line) =>
  RETIRED_STATE_WORDS.some((re) => re.test(line) || re.test(rendered(line)))

/* The only two places the word may survive, because both exist to retire it:
 * this file, which names it in order to ban it, and the "Was" column of the
 * switch table that hq-copy-export.mjs prints into docs/HQ-COPY.md, which is
 * the record of what each state word was before the switch. The second is not
 * waved past — it is limited to mapping-table rows and its count is asserted,
 * so the exemption cannot quietly grow a seventh site. */
const MAPPING_TABLE_ROWS = 6
const isMappingTableRow = (line) => /^\s*p\('\|.*\|\s*\*\*/.test(line)

test('R8 · the retired state word survives nowhere under ui/src, in any case, apostrophe or escape', () => {
  const root = join(HERE, '..')
  const offenders = []
  let mappingRows = 0
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (entry.name === 'node_modules' || entry.name.startsWith('.')) continue
      const full = join(dir, entry.name)
      if (entry.isDirectory()) { walk(full); continue }
      if (!/\.(js|jsx|css|json|mjs)$/.test(entry.name)) continue
      /* This test names the word in order to ban it; it is its own exception. */
      if (full.endsWith('change-model.test.js')) continue
      const src = readFileSync(full, 'utf8')
      src.split('\n').forEach((line, i) => {
        if (!carriesRetiredWord(line)) return
        if (full.endsWith('hq-copy-export.mjs') && isMappingTableRow(line)) { mappingRows += 1; return }
        offenders.push(`${full.slice(root.length + 1)}:${i + 1}: ${line.trim()}`)
      })
    }
  }
  walk(root)
  assert.deepEqual(offenders, [], 'the retired state word is still in the source — a partial switch')
  assert.equal(mappingRows, MAPPING_TABLE_ROWS, 'the mapping table is the only exemption, and it has not grown')
})

test('R8 · the fence catches the escaped forms that carried the word past it before', () => {
  const evasions = [
    "test('zero: nothing reported is not Proven — Couldn\\'t check', () => {",
    'const word = "Couldn’t check"',
    "const word = 'couldn\\u2019t check'",
    'const word = "Couldn&#39;t check"',
    "const word = 'COULDN\\x27T CHECK'",
  ]
  for (const line of evasions) {
    assert.ok(carriesRetiredWord(line), `the fence must catch this evasion: ${line}`)
  }
  /* And it does not fire on the two ordinary words in a sentence, on the engine
   * slug (a key, which stays), or on another file's ban assertion. */
  for (const clean of [
    'the record could not check this either way',
    "couldnt: { word: 'Recorded' },",
    'assert.doesNotMatch(GUIDE, /\\*\\*couldn.t check\\*\\*/i)',
  ]) {
    assert.ok(!carriesRetiredWord(clean), `the fence must not fire on: ${clean}`)
  }
})

test('the browser’s freshness copy is the engine’s, key for key', () => {
  const F = HQ_COPY.change.coverage.freshness
  const slugs = Object.keys(F)
  /* 8 since T-F-1-F1: `applicability_still_applies_unnamed`, the same
   * conclusion for a record that names neither the paths compared nor the
   * version compared against. The count is a fence, not the fence — the
   * key-for-key equality with `voice.rs` is asserted in Rust. */
  assert.equal(slugs.length, 8, 'every applicability line the engine can emit has a sentence here')
  for (const slug of slugs) {
    assert.ok(F[slug].plain && F[slug].technical, `${slug} has both registers`)
    assert.ok(slug.startsWith('applicability_'), `${slug} is an engine slug`)
  }
})

test('the version sentence has one author', () => {
  assert.equal(versionSentence({ bound: false }), HQ_COPY.change.version.none)
  assert.equal(versionSentence({ bound: true, key: 'placeholder' }), HQ_COPY.change.version.placeholder)
  assert.match(versionSentence({ bound: true, named: true, key: 'bound', values: { revision: 'e71c5b0', date: '2026-09-22' }, date: '2026-09-22' }), /e71c5b0/)
})

test('criterionState reads typed fields and never the prose', () => {
  assert.equal(criterionState({ items: [] }, null), 'in_progress')
  assert.equal(criterionState({ items: [{ bears: true, state: 'proven' }] }, { fresh: false }), 'needs_recheck')
  assert.equal(STATE_WORDS.needs_recheck, 'Needs a re-check')
})

test('A3 g2 r1: a stale proof from another version is answered by a run on this version that holds — the engine\'s own readiness rule — and by nothing less', () => {
  /* The shape the server serves after a code change and Check it again: the
   * earlier run is about another version and no longer applies; the new run
   * is on this version and carries no freshness row yet. */
  const stale = { bears: true, state: 'proven', applies: false, fresh: false }
  const passHere = { bears: true, state: 'proven', applies: true, fresh: null }
  const failHere = { bears: true, state: 'not_proven', applies: true, fresh: null }
  /* The re-check passed: Proven, as "Ready for approval … It rests on" says on the same screen. */
  assert.equal(criterionState({ items: [stale, passHere] }, { fresh: false }), 'proven')
  /* It failed: not a pass, and the stale proof still leads, as the engine's readiness reason does. */
  assert.equal(criterionState({ items: [stale, failHere] }, { fresh: false }), 'needs_recheck')
  assert.equal(criterionState({ items: [stale, passHere, failHere] }, { fresh: false }), 'needs_recheck')
  /* A stale item on THIS version is not answered by a pass beside it. */
  assert.equal(criterionState({ items: [{ ...stale, applies: true }, passHere] }, { fresh: false }), 'needs_recheck')
  /* A run whose version the record does not state is not a run on this version. */
  assert.equal(criterionState({ items: [stale, { ...passHere, applies: null }] }, { fresh: false }), 'needs_recheck')
  /* Nothing on this version at all: the stale proof alone is Recorded, as before. */
  assert.equal(criterionState({ items: [stale] }, { fresh: false }), 'recorded')
})

/* ── T-3-4: execution state beside the observed result, never merged ────── */

const EXEC_COVERAGE = (state, reasonSlug) => ({
  schema_version: 1,
  change_id: 'chg_x',
  scope_version: 1,
  criteria: [],
  candidates: [],
  executions: [{
    run_key: 'cli:check:2026-09-22T10:00:00Z',
    binding_id: null,
    name: 'cargo test',
    state,
    state_slug: 'execution_' + state,
    reason_slug: reasonSlug || null,
    started_at: '2026-09-22T10:00:00Z',
    recorded_at: '2026-09-22T10:09:00Z',
    cleanup_verified: true,
    survivors: 0,
    settles: state === 'completed',
  }],
  link_rows: 0,
})

test('T-3-4 · an interrupted run reads in Sam\'s words, carries one action, and never carries a state word', () => {
  const hq = deriveChangeModel({ id: 'chg_x' }, EXEC_COVERAGE('interrupted', 'execution_reason_hq_restarted'), null, null, [])
  assert.equal(hq.stoppedRuns.length, 1)
  const run = hq.stoppedRuns[0]
  assert.equal(run.executionLine, 'This check was interrupted when ThruLine restarted. Run it again.')
  assert.equal(run.action, 'Run it again')
  assert.equal(run.word, null, 'a run that did not finish carries no state word at all')
  assert.equal(run.settles, false)
  /* The two halves are two fields. Merging them is the defect. */
  assert.ok(run.observedLine && run.observedLine !== run.executionLine)
  /* Never "Proven", on any field of the card. */
  for (const value of Object.values(run)) {
    if (typeof value === 'string') assert.ok(!/Proven/.test(value), 'a stopped run may never read Proven: ' + value)
  }
  assert.equal(hq.inboxRow.stoppedRuns, 1, 'the Inbox reason fires from the record')
})

test('T-3-4 · cancelled and blocked are stopped too; a completed run is not, and settles', () => {
  for (const state of ['cancelled', 'blocked']) {
    const hq = deriveChangeModel({ id: 'chg_x' }, EXEC_COVERAGE(state), null, null, [])
    assert.equal(hq.stoppedRuns.length, 1, state + ' must be a run a person has to act on')
    assert.equal(hq.stoppedRuns[0].settles, false)
    assert.equal(hq.stoppedRuns[0].action, 'Run it again')
  }
  const done = deriveChangeModel({ id: 'chg_x' }, EXEC_COVERAGE('completed'), null, null, [])
  assert.equal(done.stoppedRuns.length, 0)
  assert.equal(done.executions[0].settles, true)
  assert.equal(done.executions[0].observedLine, null)
})

test('T-3-4 · "nothing looked" is never rendered as "nothing was left running"', () => {
  const doc = EXEC_COVERAGE('interrupted', 'execution_reason_pid_reused')
  doc.executions[0].cleanup_verified = null
  doc.executions[0].survivors = null
  const hq = deriveChangeModel({ id: 'chg_x' }, doc, null, null, [])
  assert.equal(hq.stoppedRuns[0].cleanupLine, "ThruLine can't say whether anything from this check was left running.")
})

test('T-3-4 · a running run offers one control — stop it — and a stopped one offers Run it again', () => {
  const going = deriveChangeModel({ id: 'chg_x' }, EXEC_COVERAGE('running'), null, null, [])
  assert.equal(going.openRuns.length, 1)
  assert.equal(going.stoppedRuns.length, 0, 'a run still going is not a run that was cut short')
  const run = going.actionableRuns[0]
  assert.equal(run.action, 'Stop this check')
  assert.equal(run.act, 'cancel')
  assert.equal(run.settles, false, 'nothing is settled until it finishes')
  assert.equal(run.word, null)
  assert.ok(run.actionNote && /settles nothing/.test(run.actionNote))

  const stopped = deriveChangeModel({ id: 'chg_x' }, EXEC_COVERAGE('cancelled', 'execution_reason_cancelled_by_person'), null, null, [])
  assert.equal(stopped.actionableRuns[0].act, 'rerun')
  assert.equal(stopped.actionableRuns[0].action, 'Run it again')
  assert.equal(stopped.actionableRuns[0].executionLine, 'You stopped this check.')

  /* A finished run needs no card: it is already answered by its criterion. */
  const done = deriveChangeModel({ id: 'chg_x' }, EXEC_COVERAGE('completed'), null, null, [])
  assert.equal(done.actionableRuns.length, 0)
})

/* ── T9 Part A: one applicability rule, shown in Technical ─────────────── */

import { freshnessOf } from './change-model.js'

const T9_DOC = { items: [{ link_id: 'l1', criterion_index: 1, checked_paths: ['cart/'], old_snapshot: '9f2ad3e41aa', new_snapshot: 'e71c5b0c9bb', reason_paths: [] }] }
const T9_APPLIES = { n: 1, items: [{ id: 'l1', fresh: true, freshSlug: 'applicability_still_applies' }] }

test('(t9.A1) an Applies record carries old→new and the checked path into Technical', () => {
  const f = freshnessOf(T9_APPLIES, T9_DOC)
  assert.equal(f.fresh, true)
  assert.deepEqual(f.carried, { path: 'cart/', from: '9f2ad3e', to: 'e71c5b0' })
  assert.equal(f.carriedTechnical, 'carried forward: cart/ unchanged 9f2ad3e→e71c5b0')
  assert.equal(f.rule, HQ_COPY.change.coverage.carried.applicability_rule)
})

test('(t9.A1) a missing old_snapshot, new_snapshot or checked path falls back to _unnamed, never a hole', () => {
  for (const drop of ['old_snapshot', 'new_snapshot', 'checked_paths']) {
    const doc = { items: [{ ...T9_DOC.items[0], [drop]: undefined }] }
    const f = freshnessOf(T9_APPLIES, doc)
    assert.equal(f.carriedTechnical, HQ_COPY.change.coverage.carried.applicability_carried_forward_unnamed, drop)
    assert.doesNotMatch(f.carriedTechnical, /[{}]|undefined|null/, drop)
  }
})

test('(t9.A2) a stale record carries nothing forward, and names the one rule and its reason class slug', () => {
  const row = { n: 1, items: [{ id: 'l1', fresh: false, freshSlug: 'applicability_needs_a_recheck' }] }
  const doc = { items: [{ ...T9_DOC.items[0], reason_paths: ['cart/total.js'], reason_slug: 'applicability_reason_source_changed' }] }
  const f = freshnessOf(row, doc)
  assert.equal(f.fresh, false)
  assert.equal(f.carried, null)
  assert.equal(f.carriedTechnical, null)
  assert.equal(f.rule, HQ_COPY.change.coverage.carried.applicability_rule)
  assert.equal(f.reasonClass, 'applicability_reason_source_changed')
})
