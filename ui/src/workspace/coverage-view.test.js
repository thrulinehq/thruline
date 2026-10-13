import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import {
  BASES,
  STATES,
  basisKey,
  buildCoverageView,
  coverageRows,
  evidenceEntries,
  itemOf,
  linkBlocker,
  rowsLine,
  stateSentence,
} from './coverage-view.js'
import { conclusionOf } from './change-view.js'
import { mappingFromCoverage, stillNeedsEvidence } from './packet-view.js'
import { HQ_COPY } from './hq-copy.js'

/* T-3-1 — the model behind "what was looked at for each thing that counts as
 * done".
 *
 * The document below is the shape `GET /api/changes/<id>/coverage` serves,
 * copied from the engine's own snapshot
 * (`crates/core/tests/snapshots/change_evidence_rule__the_served_coverage_is_stable.snap`)
 * rather than invented here, so a field the engine renames fails this file.
 * Every id is synthetic and in the `cccccccc-` family.
 */

const here = dirname(fileURLToPath(import.meta.url))
const C = HQ_COPY.change.coverage
const SESSION = 'cccccccc-0000-4000-8000-00000000s001'

const item = (over = {}) => ({
  link_id: 'cce_0000',
  kind: 'check_run',
  evidence_id: 'cccccccc-0000-4000-8000-00000000b001',
  session_id: SESSION,
  basis: 'declared',
  basis_slug: 'basis_declared',
  detail: 'cargo-test',
  actor: null,
  asserted_at: '2026-02-01T12:00:00Z',
  reason: null,
  name: 'bash:/tmp/synthetic:cargo clippy',
  observed: 'proven',
  bears: true,
  note_slug: null,
  ...over,
})

const DOC = {
  schema_version: 1,
  change_id: 'cccccccc-0000-4000-8000-00000000c001',
  scope_version: 1,
  criteria: [
    {
      index: 1,
      text: 'The build stays clean under clippy after the change.',
      class: 'build',
      state: 'evidence',
      state_slug: 'coverage_evidence',
      absent_tool: null,
      items: [item()],
      removed: [],
    },
    {
      index: 2,
      text: 'The test suite passes on the packages that changed.',
      class: 'test',
      state: 'ran_nothing',
      state_slug: 'coverage_ran_checked_nothing',
      absent_tool: null,
      items: [item({ link_id: 'cce_0001', evidence_id: 'cccccccc-0000-4000-8000-00000000b002', name: 'bash:/tmp/synthetic:cargo test', observed: 'couldnt', bears: false, note_slug: 'item_checked_nothing' })],
      removed: [],
    },
    {
      index: 3,
      text: 'HQ reopens where you left it after a restart.',
      class: 'unclassified',
      state: 'none',
      state_slug: 'coverage_no_test_ran',
      absent_tool: null,
      items: [],
      removed: [],
    },
    {
      index: 4,
      text: 'The interface behaves the same in a browser.',
      class: 'test',
      state: 'absent_tool',
      state_slug: 'coverage_tool_absent',
      absent_tool: 'playwright',
      items: [],
      removed: [item({ link_id: 'cce_0002', basis: 'not-bearing', actor: 'sam', reason: 'wrong line' })],
    },
  ],
  candidates: [
    { kind: 'check_run', evidence_id: 'cccccccc-0000-4000-8000-00000000b001', session_id: SESSION, name: 'bash:/tmp/synthetic:cargo clippy', observed: 'proven', bears: true },
  ],
  link_rows: 3,
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
}

/* ── the four states, and the sentence for each ─────────────────────────── */

test('a line with nothing behind it says "No test ran for this." — the sentence rule 5 names', () => {
  const rows = coverageRows(DOC)
  assert.equal(rows[2].state, 'none')
  assert.equal(rows[2].sentence, 'No test ran for this.')
})

test('a line whose check cannot run here names the tool, and never reads as a pass', () => {
  const rows = coverageRows(DOC)
  assert.equal(rows[3].sentence, "The check this needs isn't available here: playwright.")
})

test('a line whose only evidence settled nothing says so and still shows the run', () => {
  const rows = coverageRows(DOC)
  assert.equal(rows[1].state, 'ran_nothing')
  assert.equal(rows[1].items.length, 1, 'the run is shown, not hidden')
  assert.equal(rows[1].items[0].note, C.item.checked_nothing)
  assert.equal(rows[1].items[0].bears, false)
})

test('every state and every basis has a sentence in the copy file', () => {
  for (const state of STATES) assert.ok(C.states[state], `no sentence for ${state}`)
  for (const basis of BASES) assert.ok(C.basis[basisKey(basis)], `no sentence for ${basis}`)
})

test('a state the document invents renders nothing rather than half a sentence', () => {
  assert.equal(stateSentence({ state: 'wishful' }), null)
  assert.equal(stateSentence({}), null)
})

/* ── nothing is inferred here ───────────────────────────────────────────── */

test('the model matches no criterion to any run: the engine owns that rule', () => {
  /* A document whose served state DISAGREES with what a matcher would say. The
   * line is about clippy and the run names clippy, so anything matching text to
   * text here would call it evidence. The record says nothing is linked, and
   * the record is what the screen shows. */
  const contradiction = {
    ...DOC,
    criteria: [{
      index: 1,
      text: 'The build stays clean under clippy after the change.',
      class: 'build',
      state: 'none',
      state_slug: 'coverage_no_test_ran',
      absent_tool: null,
      items: [],
      removed: [],
    }],
    candidates: [{ kind: 'check_run', evidence_id: 'cccccccc-0000-4000-8000-00000000b001', session_id: SESSION, name: 'bash:/tmp/synthetic:cargo clippy', observed: 'proven', bears: true }],
  }
  const rows = coverageRows(contradiction)
  assert.equal(rows[0].state, 'none')
  assert.equal(rows[0].sentence, 'No test ran for this.')
  assert.equal(rows[0].items.length, 0)
  assert.equal(evidenceEntries(contradiction).length, 0)
  /* And the only file allowed to decide it names its one rule. */
  const engine = readFileSync(join(here, '..', '..', '..', 'crates', 'core', 'src', 'change_evidence.rs'), 'utf8')
  assert.ok(engine.includes('pub const THE_RULE: &str = "links_for"'), 'the engine no longer names its one rule')
})

test('an entry with no basis, or a basis nobody defined, never reaches the screen', () => {
  assert.equal(itemOf({ evidence_id: 'x' }), null)
  assert.equal(itemOf({ basis: 'timing', evidence_id: 'x' }), null, 'timing is not a basis')
  assert.equal(itemOf({ basis: 'manual' }), null, 'an item with no address is refused')
})

/* ── the top sentence counts through the mapping ────────────────────────── */

test('the conclusion counts a line as having evidence only when something bears on it', () => {
  const criteria = DOC.criteria.map((c) => c.text)
  const conclusion = conclusionOf({ criteria, evidence: evidenceEntries(DOC) })
  assert.equal(conclusion.total, 4)
  assert.equal(conclusion.withEvidence, 1, 'only the line with a bearing item counts')
  assert.equal(conclusion.withoutEvidence, 3)
})

test('a run that settled nothing cannot move the count, however many there are', () => {
  const many = { ...DOC, criteria: DOC.criteria.map((c) => ({ ...c, state: 'ran_nothing', items: [item({ bears: false, observed: 'couldnt', note_slug: 'item_checked_nothing' })] })) }
  const conclusion = conclusionOf({ criteria: many.criteria.map((c) => c.text), evidence: evidenceEntries(many) })
  assert.equal(conclusion.withEvidence, 0)
  assert.equal(conclusion.key.startsWith('no_evidence'), true)
})

test('a confirmed session with an unrelated green run does not reach the count at all', () => {
  /* The engine refuses to link it, so it is not in `criteria[].items`; it is in
   * `candidates`, where a person can still find it. The count reads the links
   * and nothing else. */
  const unrelated = { ...DOC, criteria: DOC.criteria.map((c) => ({ ...c, state: 'none', items: [] })), candidates: [{ kind: 'check_run', evidence_id: 'cccccccc-0000-4000-8000-00000000b009', session_id: SESSION, name: 'bash:/tmp/synthetic:ls -la', observed: 'proven', bears: true }] }
  assert.equal(evidenceEntries(unrelated).length, 0)
  const conclusion = conclusionOf({ criteria: unrelated.criteria.map((c) => c.text), evidence: evidenceEntries(unrelated) })
  assert.equal(conclusion.withEvidence, 0)
})

/* ── the technical layer's count ────────────────────────────────────────── */

test('the row count reaches a person with its noun, and zero is a sentence', () => {
  assert.equal(rowsLine({ link_rows: 1 }), '1 link in the record behind this.')
  assert.equal(rowsLine({ link_rows: 3 }), '3 links in the record behind this.')
  assert.equal(rowsLine({ link_rows: 0 }), C.rows_none)
  assert.equal(rowsLine(null), C.rows_none)
})

/* ── the disabled control always says why ───────────────────────────────── */

test('the enabler names the act that would actually turn the control on', () => {
  /* The real Change of the T-3-1 walk: the work IS confirmed and the accepted
   * list names no files. Asking the person to confirm the work is asking for
   * what they have already done (T24-7's shape). */
  const scope = linkBlocker({ candidates: [], actor: 'sam', whySlug: 'not_applicable_no_scope' })
  assert.equal(scope, C.disabled.by_reason.not_applicable_no_scope)
  assert.ok(!scope.enabler.includes('Confirm the work'), scope.enabler)
  assert.ok(scope.enabler.includes('which files this Change is about'))
  /* And every refusal the link layer can give has its own pair. */
  for (const slug of ['not_applicable_no_link', 'not_applicable_only_suggested', 'not_applicable_rejected', 'not_applicable_no_scope', 'not_applicable_outside_scope', 'not_applicable_no_changed_paths']) {
    const pair = C.disabled.by_reason[slug]
    assert.ok(pair, `no pair for ${slug}`)
    assert.ok(pair.reason.length > 30 && pair.enabler.length > 30, slug)
  }
  /* A word nobody wrote a pair for falls back rather than going silent. */
  assert.equal(linkBlocker({ candidates: [], actor: 'sam', whySlug: 'something_new' }), C.disabled.nothing_to_link)
})

test('the link control is off with both halves of the reason, in both cases', () => {
  const nothing = linkBlocker({ candidates: [], actor: 'sam' })
  assert.equal(nothing, C.disabled.nothing_to_link)
  assert.ok(nothing.reason.length > 20 && nothing.enabler.length > 20)
  const nameless = linkBlocker({ candidates: [{ evidenceId: 'x' }], actor: '' })
  assert.equal(nameless, C.disabled.needs_name)
  assert.ok(nameless.enabler.length > 20)
  assert.equal(linkBlocker({ candidates: [{ evidenceId: 'x' }], actor: 'sam' }), null)
})

/* ── the packet reads the same document ─────────────────────────────────── */

test('the packet section is built from the coverage and adds nothing to it', () => {
  const mapping = mappingFromCoverage(DOC)
  assert.equal(mapping.rows.length, 4)
  assert.equal(mapping.rows[2].sentence, 'No test ran for this.')
  assert.equal(mapping.rows[1].items[0].why, C.basis.declared)
  assert.equal(mapping.rows[3].removed[0].who, 'sam')
  assert.equal(mapping.refused, 0)
})

test('the packet’s "still needs evidence" list names each reason apart', () => {
  const needs = stillNeedsEvidence(mappingFromCoverage(DOC).rows)
  const why = needs.map((n) => n.why)
  assert.equal(needs.length, 3)
  assert.ok(why.includes('No test ran for this.'))
  assert.ok(why.some((w) => w.includes('checked nothing')))
  assert.ok(why.some((w) => w.includes('playwright')))
})

test('the packet no longer claims the record holds no link, because it does', () => {
  const sentence = HQ_COPY.change.packet.limits.no_link_between
  assert.ok(
    !sentence.includes('Nothing in this build writes down'),
    'the page still says the link does not exist: ' + sentence,
  )
  assert.ok(
    sentence.includes('not a statement that any of it settles'),
    'the page dropped the half that is still true: ' + sentence,
  )
})

/* ── an empty document is an answer ─────────────────────────────────────── */

test('a Change with nothing accepted has no rows and no invented ones', () => {
  const view = buildCoverageView({ criteria: [], candidates: [], link_rows: 0, scope_version: null })
  assert.deepEqual(view.rows, [])
  assert.equal(view.accepted, null)
  assert.equal(view.rowsLine, C.rows_none)
  assert.deepEqual(buildCoverageView(null).rows, [])
})

test('the lines to read first are the ones that are not settled', () => {
  const view = buildCoverageView(DOC)
  assert.deepEqual(view.needing.map((r) => r.n), [2, 3, 4])
})
