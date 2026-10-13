import test from 'node:test'
import assert from 'node:assert/strict'

import {
  SECTIONS,
  SECTION_SOURCE,
  PROVENANCE,
  buildPacketView,
  criteriaMapping,
  evidenceItem,
  findingEvidence,
  footerOf,
  recordAddress,
  sectionsOf,
  stillNeedsEvidence,
  sameLine,
  unacceptedProposals,
} from './packet-view.js'
import { HQ_COPY } from './hq-copy.js'

/* T-2-5 — the model behind the whole story on one page.
 *
 * Pure: no bundle, no DOM, no clock. Everything here is a decision over a
 * typed record, and every record below is synthetic — no string came off a
 * real machine.
 */

const P = HQ_COPY.change.packet
const SESSION_A = 'cccccccc-0000-4000-8000-00000000s001'
const SESSION_B = 'cccccccc-0000-4000-8000-00000000s002'

function aChange(over = {}) {
  return {
    id: 'cccccccc-0000-4000-8000-00000000c001',
    project_id: 'cccccccc-0000-4000-8000-00000000p001',
    opened_at: '2026-01-02T09:00:00Z',
    request: { title: 'Retry failed uploads', purpose: 'Uploads that fail once are lost, and the person never finds out.', requester: 'sam', source: 'person' },
    accepted_scope: null,
    implementation: null,
    associations: [],
    examination: 'none_yet',
    decision: 'none_yet',
    masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
    ...over,
  }
}

const FULL = aChange({
  accepted_scope: {
    version: 1,
    criteria: ['A failed upload is tried again three times.', 'The person is told when it finally gives up.'],
    exclusions: ['Uploads bigger than 2 GB.'], allowed_scope: [], constraints: [], check_profile: ['cargo-test'],
    content_sha256: '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
    acceptance: { actor: 'sam', accepted_at: '2026-01-02T09:06:00Z', rationale: 'That is what I asked for.' },
    proposed_at: '2026-01-02T09:05:00Z',
  },
  implementation: {
    version: 1, bound_at: '2026-01-02T10:00:00Z', repository_identity: 'repo-synthetic-0001',
    base_revision: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0', snapshot_id: null, workspace_identity: 'workspace-synthetic-0001',
  },
  associations: [
    { id: 'a1', session_id: SESSION_A, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-01-02T10:01:00Z', reason: null },
    { id: 'a2', session_id: SESSION_B, provenance: 'suggested', basis: 'timing', actor: null, asserted_at: '2026-01-02T10:02:00Z', reason: null },
  ],
})

const CARD = {
  id: 'clm_1',
  state: 'not_proven',
  kind: 'said_done_no_proof',
  seq: 44,
  said: null,
  record: { text: 'Not proven: nothing in the record shows this was done.', role: 'record', refs: [] },
  do: { text: 'Ask the AI for proof.', role: 'do', refs: [], prompt: null },
  detector: null,
  proofNode: null,
}

/* ── the twelve sections, in the brief's order ─────────────────────────── */

test('the sections are the brief’s twelve, in the brief’s reading order', () => {
  /* `B10.3`, one sentence, in order: purpose and original request · accepted
     criteria and exclusions · exact target and assessment inputs · actual diff
     summary · criterion-to-evidence mapping · examination results and
     limitations · applicable review rounds · finding dispositions ·
     repair/rerun history · freshness/applicability · exceptions · approval
     history. Twelve. */
  assert.equal(SECTIONS.length, 12)
  assert.deepEqual([...SECTIONS], [
    'purpose', 'accepted', 'target', 'changed', 'mapping',
    'examination', 'review', 'dispositions', 'repairs', 'freshness',
    'exceptions', 'decisions',
  ])
  assert.deepEqual(Object.keys(SECTION_SOURCE), [...SECTIONS], 'a section with no line of the brief behind it')
  assert.deepEqual(Object.keys(HQ_COPY.change.packet.sections), [...SECTIONS], 'a section with no copy, or copy with no section')
})

test('the card’s ten named sections are these twelve, in this order', () => {
  /* The task card names ten of the twelve in its own words. They are a
     subsequence of the brief's twelve and in the same order; the two the card
     folds into its neighbours ("examination results and limitations",
     "finding dispositions") are rendered separately because `B10.3` names
     them separately. */
  const card = ['purpose', 'accepted', 'target', 'changed', 'mapping', 'review', 'repairs', 'freshness', 'exceptions', 'decisions']
  let at = -1
  for (const id of card) {
    const next = SECTIONS.indexOf(id)
    assert.ok(next > at, `${id} is out of the card's order`)
    at = next
  }
})

test('every section has a heading and a sentence for when it holds nothing', () => {
  for (const id of SECTIONS) {
    const s = HQ_COPY.change.packet.sections[id]
    assert.ok(s.heading.length > 3, `${id} has no heading`)
    assert.ok(s.empty.length > 30, `${id}.empty teaches nothing: ${s.empty}`)
  }
})

test('no heading is a state word: the five describe one piece of evidence, never a section', () => {
  const five = Object.values(HQ_COPY.states).map((s) => s.word)
  for (const id of SECTIONS) {
    for (const word of five) {
      assert.ok(!HQ_COPY.change.packet.sections[id].heading.includes(word), `${id} heading renders the state word "${word}"`)
    }
  }
})

test('the six sections nothing in this build records are false on a full Change, not merely on an empty one', () => {
  const model = buildPacketView({ change: FULL, work: [{ id: SESSION_A, findings: [CARD], steps: 41 }], build: ['3 files changed'] })
  const byId = Object.fromEntries(model.sections.map((s) => [s.id, s.filled]))
  assert.deepEqual(byId, {
    purpose: true, accepted: true, target: true, changed: true, mapping: true, examination: true,
    review: false, dispositions: false, repairs: false, freshness: false, exceptions: false, decisions: false,
  })
})

test('freshness is never answered yes: the section says unknown in words', () => {
  const empty = HQ_COPY.change.packet.sections.freshness.empty
  /* Two halves, both required: it says ThruLine cannot answer this, and it
     says how to read that silence. The sentence may contain the word "yes" —
     it says "not as yes" — but it may never ASSERT that the evidence still
     applies, which is the collapse `B7.2` forbids. */
  assert.match(empty, /does not track this yet/)
  assert.match(empty, /unknown, not as yes/)
  assert.doesNotMatch(empty, /\bstill applies\b|\bthe answer is yes\b/i)
  const model = buildPacketView({ change: FULL, work: [] })
  assert.equal(model.sections.find((s) => s.id === 'freshness').filled, false)
})

/* ── one evidence item: labelled, addressed, or refused ────────────────── */

test('an evidence item carries a provenance word off the brief’s four and an address in the record', () => {
  const made = evidenceItem({ provenance: 'captured', session: SESSION_A, seq: 44, state: 'proven', criterion: 1 })
  assert.equal(made.ok, true)
  assert.equal(made.item.provenance, 'captured')
  assert.equal(made.item.provenanceWord, P.provenance.captured)
  assert.equal(made.item.record.href, '/sessions?session=' + encodeURIComponent(SESSION_A) + '#/trace')
  assert.equal(made.item.record.where, 'step 44 of the work you linked')
  assert.deepEqual([...PROVENANCE], ['captured', 'imported', 'attested', 'generated'])
  for (const word of PROVENANCE) assert.ok(P.provenance[word], `no word for provenance ${word}`)
})

test('an item with no provenance is REFUSED, never shown unlabelled', () => {
  for (const raw of [
    { session: SESSION_A, seq: 44 },
    { provenance: '', session: SESSION_A, seq: 44 },
    { provenance: 'vibes', session: SESSION_A, seq: 44 },
  ]) {
    const made = evidenceItem(raw)
    assert.equal(made.ok, false, `an unlabelled item was accepted: ${JSON.stringify(raw)}`)
    assert.equal(made.reason, 'no_provenance')
  }
})

test('an item with no address in the record is refused too: a label with nothing to open is not a link', () => {
  const made = evidenceItem({ provenance: 'captured', seq: 44 })
  assert.equal(made.ok, false)
  assert.equal(made.reason, 'no_record')
})

test('an item with no step still has an address: the piece of linked work itself', () => {
  const made = evidenceItem({ provenance: 'captured', session: SESSION_A })
  assert.equal(made.ok, true)
  assert.equal(made.item.record.where, P.record.at_work)
  assert.equal(recordAddress(SESSION_A, null).where, P.record.at_work)
})

test('a refused item is counted on the page, not dropped in silence', () => {
  const { rows, refused } = criteriaMapping(['one', 'two'], [
    { criterion: 1, provenance: 'captured', session: SESSION_A, seq: 3 },
    { criterion: 1, session: SESSION_A, seq: 4 },
    { criterion: 9, provenance: 'captured', session: SESSION_A, seq: 5 },
  ])
  assert.equal(refused, 2, 'an unlabelled item and one pointing at no criterion are both refused')
  assert.equal(rows[0].items.length, 1)
})

test('a finding of the confirmed work is a captured observation, labelled as one', () => {
  const { items, refused } = findingEvidence([{ id: SESSION_A, findings: [CARD] }])
  assert.equal(refused, 0)
  assert.equal(items.length, 1)
  assert.equal(items[0].provenance, 'captured')
  assert.equal(items[0].record.where, 'step 44 of the work you linked')
  assert.equal(items[0].card, CARD)
})

/* ── the criterion-to-evidence mapping ─────────────────────────────────── */

test('a thing that counts as done with nothing behind it is marked as having nothing, never as passing', () => {
  const { rows } = criteriaMapping(['A failed upload is tried again three times.'], [])
  assert.equal(rows.length, 1)
  assert.equal(rows[0].nothing, true)
  assert.equal(rows[0].items.length, 0)
  assert.equal(P.mapping.nothing_for_it, 'No test ran for this.')
})

test('a thing with evidence lists it, and the evidence keeps its own observed result', () => {
  const { rows } = criteriaMapping(['one', 'two'], [
    { criterion: 2, provenance: 'captured', session: SESSION_A, seq: 7, state: 'proven', summary: 'cargo test exited 0' },
  ])
  assert.equal(rows[0].nothing, true)
  assert.equal(rows[1].nothing, false)
  assert.equal(rows[1].items[0].state, 'proven')
  assert.equal(rows[1].items[0].summary, 'cargo test exited 0')
})

test('what still needs evidence is the things with nothing behind them, and the things the record holds the opposite of', () => {
  const { rows } = criteriaMapping(['one', 'two', 'three'], [
    { criterion: 2, provenance: 'captured', session: SESSION_A, seq: 7, state: 'proven' },
    { criterion: 3, provenance: 'captured', session: SESSION_A, seq: 8, state: 'not_proven' },
  ])
  assert.deepEqual(stillNeedsEvidence(rows), [
    { n: 1, text: 'one', why: P.needs.no_test },
    { n: 3, text: 'three', why: P.needs.problem },
  ])
})

test('the served routes hand in no link between a criterion and a piece of evidence, so every line reads as having nothing', () => {
  /* The boundary stated in `packet-view.js`: today's documents carry no such
     link, and inventing one from the words would be the collapse `B7.2`
     forbids. This test is what says so out loud if that ever changes. */
  const model = buildPacketView({ change: FULL, work: [{ id: SESSION_A, findings: [CARD], steps: 41 }] })
  assert.equal(model.mapping.rows.length, 2)
  for (const row of model.mapping.rows) assert.equal(row.nothing, true)
  assert.equal(model.needs.length, 2)
})

/* ── what this page is made of (03-USER-LENS.md rule 13) ───────────────── */

test('the footer’s two numbers are the typed fields, summed, and nothing else', () => {
  assert.equal(footerOf([]).text, P.footer.none)
  assert.deepEqual(footerOf([{ steps: 41 }]), { work: 1, steps: 41, text: 'Built from 1 piece of linked work · 41 recorded steps.' })
  assert.deepEqual(footerOf([{ steps: 41 }, { steps: 1 }]), { work: 2, steps: 42, text: 'Built from 2 pieces of linked work · 42 recorded steps.' })
  assert.equal(footerOf([{ steps: 1 }]).text, 'Built from 1 piece of linked work · 1 recorded step.')
})

test('a step count that is not a number is not counted: the footer never estimates', () => {
  assert.equal(footerOf([{ steps: null }, { steps: 'lots' }, { steps: -3 }]).steps, 0)
})

test('changing the typed field changes the footer', () => {
  const one = buildPacketView({ change: FULL, work: [{ id: SESSION_A, findings: [], steps: 41 }] })
  const two = buildPacketView({ change: FULL, work: [{ id: SESSION_A, findings: [], steps: 42 }] })
  assert.equal(one.footer.steps, 41)
  assert.equal(two.footer.steps, 42)
  assert.notEqual(one.footer.text, two.footer.text)
})

/* ── the top of the page is the same words as the Change screen ────────── */

test('the conclusion and the version are decided once, by the Change screen’s own model', () => {
  const model = buildPacketView({ change: FULL, work: [] })
  assert.equal(model.conclusion.key, 'no_evidence')
  assert.deepEqual(model.conclusion.values, { total: 2 })
  assert.equal(model.version.bound, true)
  assert.equal(model.version.values.revision, 'a1b2c3d')
})

test('only confirmed work reaches the page: a suggestion contributes no evidence and is counted as left out', () => {
  const model = buildPacketView({
    change: FULL,
    work: [{ id: SESSION_A, findings: [CARD], steps: 10 }, { id: SESSION_B, findings: [CARD], steps: 99 }],
  })
  assert.deepEqual(model.work.map((w) => w.id), [SESSION_A])
  assert.equal(model.footer.steps, 10, 'a suggested piece of work was counted into what the page is made of')
  assert.deepEqual(model.scope.suggested, [SESSION_B])
})

test('sectionsOf is a live instrument: an empty Change has nothing filled', () => {
  const sections = sectionsOf({})
  assert.equal(sections.length, 12)
  assert.equal(sections.filter((s) => s.filled).length, 0)
  for (const s of sections) assert.equal(s.empty, HQ_COPY.change.packet.sections[s.id].empty)
})

/* ── a suggestion that is already accepted is not "not accepted" ───────── */

/* The suggestions route re-derives its list from the request text on every
 * read, so an accepted Change comes back holding the accepted lines as
 * suggestions too. What the page may call "not accepted" is what nobody has
 * accepted, word for word — anything else says the opposite of the line
 * directly above it (T-2-5 verifier round 0, on a real Change). */

test('a suggestion whose sentence is already accepted is not offered as unaccepted', () => {
  const accepted = ['A failed upload is tried again three times.', 'The person is told when it finally gives up.']
  const proposals = [{ text: 'A failed upload is tried again three times.' }, { text: 'The person is told when it finally gives up.' }]
  assert.deepEqual(unacceptedProposals(proposals, accepted), [])
})

test('a suggestion nobody accepted survives, and is still the page’s to label', () => {
  const accepted = ['A failed upload is tried again three times.']
  const proposals = [{ text: 'A failed upload is tried again three times.' }, { text: 'The retry interval is written down.' }]
  assert.deepEqual(unacceptedProposals(proposals, accepted).map((p) => p.text), ['The retry interval is written down.'])
})

test('with nothing accepted, every suggestion is unaccepted — the filter takes nothing away', () => {
  const proposals = [{ text: 'One.' }, { text: 'Two.' }]
  assert.deepEqual(unacceptedProposals(proposals, []).map((p) => p.text), ['One.', 'Two.'])
  assert.deepEqual(unacceptedProposals(proposals, null).map((p) => p.text), ['One.', 'Two.'])
})

test('sameLine ignores spacing, case and a trailing full stop, and nothing else', () => {
  assert.equal(sameLine('  A failed   upload is tried  again. '), sameLine('a failed upload is tried again'))
  assert.notEqual(sameLine('A failed upload is tried again three times.'), sameLine('A failed upload is tried again twice.'))
  assert.equal(sameLine(null), '')
})

test('buildPacketView carries the unaccepted list, and leaves the document’s own list untouched', () => {
  const proposals = { criteria: [
    { text: 'A failed upload is tried again three times.' },
    { text: 'The person is told when it finally gives up.' },
    { text: 'The retry interval is written down.' },
  ] }
  const model = buildPacketView({ change: FULL, proposals, work: [] })
  assert.deepEqual(model.proposals.map((p) => p.text), proposals.criteria.map((p) => p.text), 'the document’s own list was rewritten')
  assert.deepEqual(model.unaccepted.map((p) => p.text), ['The retry interval is written down.'])
})

test('on a Change whose accepted scope came straight off the suggestions, nothing is left to call unaccepted', () => {
  const proposals = { criteria: FULL.accepted_scope.criteria.map((text) => ({ text })) }
  const model = buildPacketView({ change: FULL, proposals, work: [] })
  assert.deepEqual(model.unaccepted, [])
})
