/* T-3-2 — a result about another version is shown and counts for nothing, on
 * the Change screen and in the packet (`B6.4`, `Q4`).
 *
 * The engine decides whether an item applies; these tests are the fence around
 * what the two surfaces do with that decision. The failure they exist to stop
 * is a green run from yesterday's server moving the count in the top sentence.
 */

import test from 'node:test'
import assert from 'node:assert/strict'

import { coverageRows, evidenceEntries, itemOf, STATES } from './coverage-view.js'
import { mappingFromCoverage, stillNeedsEvidence } from './packet-view.js'

function item(overrides = {}) {
  return {
    link_id: 'cce_1',
    kind: 'check_run',
    evidence_id: 'eb1',
    session_id: 's1',
    basis: 'declared',
    basis_slug: 'basis_declared',
    name: 'bash:/tmp/p:cargo test',
    observed: 'proven',
    bears: true,
    note_slug: null,
    applies: true,
    applies_slug: 'inputs_applies',
    ...overrides,
  }
}

function coverage(items, state) {
  return {
    schema_version: 1,
    criteria: [
      {
        index: 1,
        text: 'Two retry requests for the same delivery do not enqueue duplicate work.',
        class: 'test',
        state,
        state_slug:
          state === 'other_version' ? 'coverage_other_version' : 'coverage_evidence',
        absent_tool: null,
        items,
        removed: [],
      },
    ],
    candidates: [],
    link_rows: items.length,
  }
}

test('the fifth state exists, and the surface knows it', () => {
  assert.ok(STATES.includes('other_version'))
  const rows = coverageRows(coverage([item({ applies: false, applies_slug: 'inputs_other_app' })], 'other_version'))
  assert.equal(rows[0].state, 'other_version')
  assert.equal(
    rows[0].sentence,
    'Something ran for this, and not on the code this Change is set up against. It is shown below, and it settles nothing here.',
  )
})

test('a result from a different running copy of the app says so, in those words', () => {
  const row = itemOf(item({ applies: false, applies_slug: 'inputs_other_app' }))
  assert.equal(row.applies, false)
  assert.equal(row.appliesText, 'This came from a different running copy of the app.')
})

test('every mismatch class has a sentence and none of them reads as a pass', () => {
  const classes = [
    ['inputs_missing', /isn't counted/],
    ['inputs_unknown_version', /can't tell which code/],
    ['inputs_other_revision', /different code than this Change is set up against/],
    ['inputs_other_edits', /different unsaved changes/],
    ['inputs_other_folder_state', /not set up the same way/],
    ['inputs_unknown_worktree', /can't tell which copy/],
    ['inputs_other_worktree', /second checkout of the same commit/],
    ['inputs_no_app_named', /doesn't say which running copy/],
    ['inputs_unknown_app', /can't tell which running copy/],
    ['inputs_other_app', /different running copy of the app/],
    ['inputs_other_database', /different database/],
  ]
  for (const [slug, shape] of classes) {
    const row = itemOf(item({ applies: false, applies_slug: slug }))
    assert.match(row.appliesText, shape, slug)
    assert.doesNotMatch(row.appliesText.toLowerCase(), /\b(passed|all clear|broken|failed)\b/, slug)
  }
})

test('a mismatched item never moves the count in the top sentence', () => {
  const both = coverage(
    [
      item({ link_id: 'a', evidence_id: 'eb1', applies: true, applies_slug: 'inputs_applies' }),
      item({
        link_id: 'b',
        evidence_id: 'eb2',
        applies: false,
        applies_slug: 'inputs_other_app',
      }),
    ],
    'evidence',
  )
  const entries = evidenceEntries(both)
  assert.equal(entries.length, 1, 'only the item about this version counts')
  assert.equal(entries[0].summary, 'bash:/tmp/p:cargo test')
})

test('a mismatched item is still on the screen: nothing is hidden to tidy it up', () => {
  const rows = coverageRows(
    coverage([item({ applies: false, applies_slug: 'inputs_other_app' })], 'other_version'),
  )
  assert.equal(rows[0].items.length, 1)
  assert.equal(rows[0].items[0].appliesText, 'This came from a different running copy of the app.')
})

test('an item the question was not asked about carries no sentence and is not accused', () => {
  const row = itemOf(item({ applies: null, applies_slug: null }))
  assert.equal(row.applies, null)
  assert.equal(row.appliesText, null)
  const entries = evidenceEntries(coverage([item({ applies: null, applies_slug: null })], 'evidence'))
  assert.equal(entries.length, 1, 'a question nobody asked does not remove an item')
})

test('the packet prints the mismatch and puts the line back on the needs list', () => {
  const doc = coverage([item({ applies: false, applies_slug: 'inputs_other_app' })], 'other_version')
  const mapping = mappingFromCoverage(doc)
  assert.equal(mapping.rows[0].items[0].applies, false)
  assert.equal(
    mapping.rows[0].items[0].appliesText,
    'This came from a different running copy of the app.',
  )
  const needs = stillNeedsEvidence(mapping.rows)
  assert.equal(needs.length, 1)
  assert.equal(
    needs[0].why,
    'Something ran for this, and not on the code this Change is set up against. It is shown below, and it settles nothing here.',
  )
})

/* T-3-2b — the sibling checkout. Both folders clean, same commit; the refusal
 * stays and the sentence stops blaming unsaved changes that do not exist. */

test('a clean second checkout of the same commit is refused, and the sentence says why', () => {
  const row = itemOf(item({ applies: false, applies_slug: 'inputs_other_worktree' }))
  assert.equal(row.applies, false, 'the refusal stands')
  assert.equal(
    row.appliesText,
    "This is a second checkout of the same commit. ThruLine records one checkout per Change — pick the one you're working in.",
  )
})

test('the words "unsaved changes" are said only about a genuinely dirty side', () => {
  /* Every sentence the engine can put on an item when neither side has
   * uncommitted changes. None of them may mention unsaved changes. */
  const cleanOnBothSides = [
    'inputs_other_worktree',
    'inputs_other_folder_state',
    'inputs_unknown_worktree',
    'inputs_other_revision',
    'inputs_applies',
  ]
  for (const slug of cleanOnBothSides) {
    const text = itemOf(item({ applies: slug === 'inputs_applies', applies_slug: slug })).appliesText
    assert.doesNotMatch(text, /unsaved changes/, slug)
  }
  /* And it is still said when one side genuinely has them. */
  assert.match(
    itemOf(item({ applies: false, applies_slug: 'inputs_other_edits' })).appliesText,
    /different unsaved changes/,
  )
})
