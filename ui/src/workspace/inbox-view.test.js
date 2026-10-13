import test from 'node:test'
import assert from 'node:assert/strict'

import {
  REASONS,
  REASON_STAGE,
  findingCounts,
  reasonsFor,
  reasonLine,
  scopeLine,
  changeHrefAt,
  inboxItem,
  inboxItems,
  attentionOf,
  sectionOf,
  inboxSections,
  SECTION_OF_REASON,
  formatSize,
  storageReading,
} from './inbox-view.js'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'

/* T-2-6 — the Inbox model.
 *
 * Every fixture below is synthetic. No string here came off a real machine,
 * and every id is in the `cccccccc-` family.
 */

const C = HQ_COPY.change.inbox
const SESSION_A = 'cccccccc-0000-4000-8000-00000000s001'
const SESSION_B = 'cccccccc-0000-4000-8000-00000000s002'

function aChange(over = {}) {
  return {
    id: 'cccccccc-0000-4000-8000-00000000c001',
    project_id: 'cccccccc-0000-4000-8000-00000000p001',
    opened_at: '2026-01-02T09:00:00Z',
    request: { title: 'Retry failed uploads', purpose: 'Uploads that fail once are lost.', requester: 'sam', source: 'person' },
    accepted_scope: null,
    implementation: null,
    associations: [],
    examination: 'none_yet',
    decision: 'none_yet',
    ...over,
  }
}

const accepted = (criteria, profile = []) => ({
  accepted_scope: {
    version: 1,
    proposed_at: '2026-01-02T09:30:00Z',
    criteria,
    exclusions: [],
    allowed_scope: [],
    constraints: [],
    check_profile: profile,
    acceptance: { actor: 'sam', accepted_at: '2026-01-02T10:00:00Z', rationale: 'That is what I asked for.' },
  },
})

const linked = (ids, provenance = 'confirmed') => ({
  associations: ids.map((id, i) => ({
    id: 'cccccccc-0000-4000-8000-00000000a00' + (i + 1),
    session_id: id,
    provenance,
    basis: 'person',
    actor: 'sam',
    asserted_at: '2026-01-02T11:0' + i + ':00Z',
    reason: null,
  })),
})

const work = (id, findings) => ({ id, findings, verdict: null, focus: null, steps: 3 })
const finding = (state, kind = 'claim') => ({ id: 'f' + state, state, kind })

/* ── the reasons, and the order they are tried in ───────────────────────── */

test('the approved code move joins the reason order after invalidated checks', () => {
  assert.deepEqual([...REASONS], [
    'observed_failure',
    'applicable_finding',
    'interrupted_capture',
    'missing_evidence',
    'invalidated_after_edit',
    'approved_code_moved',
    'awaiting_definition',
    'awaiting_approval',
  ])
  /* Every one of them opens the Change somewhere, and every one of those is a
   * stage the Change screen has. `inbox-view.js` throws on load otherwise. */
  for (const id of REASONS) assert.ok(REASON_STAGE[id], id)
  assert.equal(REASON_STAGE.awaiting_definition, 'define')
  assert.equal(REASON_STAGE.awaiting_approval, 'approve')
})

/* SEAM-INBOX-APPROVED-VERIFY findings 1 and 3 — a standing approval that names
 * a version other than the code's is a decision waiting on a person, whichever
 * of the two facts says so: the record re-bound to another version
 * (`hq.decision.movedSince`, what "Check it again" does) or the served code
 * moved in the folder (`hq.codeMoved`). Its section, the stage its link opens
 * and the number of items it adds are each pinned, so none can drift. */
const standing = (movedSince, codeMoved) => ({
  change: aChange(accepted(['a'])),
  evidence: [{ criterion: 1, state: 'proven' }],
  hq: { decision: { approved: true, movedSince }, codeMoved },
})
const SERVED_MOVE = { recorded: 'e71c5b0', now: '9f3d21e', commits: 0, paths: [] }

test('a standing approval on another version is waiting on you, opens Build, and is one item — re-bound record or moved folder alike', () => {
  for (const [label, input] of [
    ['re-bound record, no served move', standing(true, null)],
    ['served move, record not re-bound', standing(false, SERVED_MOVE)],
    ['both', standing(true, SERVED_MOVE)],
  ]) {
    const reasons = reasonsFor(input)
    assert.deepEqual(reasons.map((r) => r.id), ['approved_code_moved'], label)
    const attention = attentionOf(input)
    assert.equal(attention.needsPerson, true, label)
    assert.equal(attention.items, 1, label + ': one decision waiting is one item')
    const item = inboxItem(input)
    assert.ok(item, label + ': the Change is listed')
    assert.equal(item.attention.items, 1, label)
    assert.equal(sectionOf(item), 'you', label + ': filed under Waiting on you')
    assert.equal(item.line.stage, 'build', label)
    assert.equal(item.href, changeHrefAt(item.id, 'build'), label + ': the link opens Build')
    assert.equal(item.reasonText, C.reasons.approved_code_moved.line, label)
    const sections = inboxSections([item], 0)
    assert.equal(sections.find((x) => x.id === 'you').count, 1, label)
    assert.equal(sections.find((x) => x.id === 'check').count, 0, label)
    assert.equal(sections.find((x) => x.id === 'nothing').count, 0, label + ': never settled')
  }
  assert.equal(SECTION_OF_REASON.approved_code_moved, 'you')
  assert.equal(REASON_STAGE.approved_code_moved, 'build')
  /* Counted beside other reasons: one more item, never zero. */
  const withMissing = { ...standing(true, null), change: aChange(accepted(['a', 'b'])) }
  assert.deepEqual(reasonsFor(withMissing).map((r) => r.id), ['missing_evidence', 'approved_code_moved'])
  assert.equal(attentionOf(withMissing).items, 2)
})

test('an approval standing on the version the code is at adds nothing; a withdrawn one adds nothing', () => {
  assert.deepEqual(reasonsFor(standing(false, null)).map((r) => r.id), [])
  assert.equal(inboxItem(standing(false, null)), null)
  const withdrawn = { ...standing(true, SERVED_MOVE), hq: { decision: { approved: false, movedSince: false }, codeMoved: SERVED_MOVE } }
  assert.deepEqual(reasonsFor(withdrawn).map((r) => r.id), [])
})

test('the approved-then-moved sentence claims no direction: the folder can be behind or beside the approved version', () => {
  const line = C.reasons.approved_code_moved.line
  assert.equal(line, 'The code has changed since the version that was approved, and the approval covers only that version.')
  assert.doesNotMatch(line, /older|newer|ahead|behind|later version|earlier version/i)
})

test('a Change with nothing accepted is awaiting definition, and the action is accepting what should count', () => {
  const reasons = reasonsFor({ change: aChange() })
  assert.deepEqual(reasons.map((r) => r.id), ['awaiting_definition'])
  const line = reasonLine(reasons[0])
  assert.equal(line.template, C.reasons.awaiting_definition.line)
  assert.equal(line.action, 'Accept what should count as done')
  assert.equal(line.stage, 'define')
})

test('an accepted criterion with nothing behind it is missing evidence, counted from the accepted list', () => {
  const change = aChange(accepted(['a', 'b']))
  const reasons = reasonsFor({ change })
  assert.deepEqual(reasons.map((r) => r.id), ['missing_evidence'])
  assert.equal(reasons[0].count, 2)
  assert.equal(fill(reasonLine(reasons[0]).template, { n: 2 }), '2 things that count as done have no evidence yet.')
  /* One is one thing, never "1 things". */
  const one = reasonsFor({ change: aChange(accepted(['a'])) })
  assert.equal(reasonLine(one[0]).template, C.reasons.missing_evidence.line_one)
  /* And a criterion that HAS evidence is not counted as missing it. */
  const covered = reasonsFor({ change, evidence: [{ criterion: 1, state: 'proven' }] })
  assert.equal(covered[0].count, 1)
  assert.equal(reasonsFor({ change, evidence: [{ criterion: 1 }, { criterion: 2 }] }).length, 0)
})

/* T-3-6-F1 (`Q1`). This used to assert the opposite: a Change whose one
 * accepted line was PROVEN still carried an Inbox row reading "1 required
 * check has not run", because the row had a second source — required checks
 * derived from the Change's `examination` field, which no verb in this build
 * ever moves off `none_yet`. Two counts of one thing is how the Inbox and the
 * Change screen came to disagree; there is now one, and this is the test that
 * it stayed one. */
test('a Change whose every accepted line has evidence carries no missing-evidence row', () => {
  const change = aChange(accepted(['a'], ['tests']))
  const proposals = { profile: { checks: [{ id: 'tests', mandatory: true }, { id: 'lint', mandatory: false }] } }
  const reasons = reasonsFor({ change, proposals, evidence: [{ criterion: 1, state: 'proven' }] })
  assert.deepEqual(reasons.map((r) => r.id), [], 'nothing is missing, so nothing may say something is')
  for (const r of reasons) assert.notEqual(r.variant, 'checks')
})

test('a Change with a line nothing has been recorded for says so, counted from the same coverage', () => {
  const change = aChange(accepted(['a', 'b'], ['tests']))
  const proposals = { profile: { checks: [{ id: 'tests', mandatory: true }] } }
  const reasons = reasonsFor({ change, proposals, evidence: [{ criterion: 1, state: 'proven' }] })
  assert.deepEqual(reasons.map((r) => r.id), ['missing_evidence'])
  assert.equal(reasons[0].count, 1)
  assert.equal(fill(reasonLine(reasons[0]).template, { n: 1 }), '1 thing that counts as done has no evidence yet.')
})

test('a finding the record holds against what was said is an observed failure, and it outranks everything', () => {
  const change = aChange({ ...accepted(['a', 'b']), ...linked([SESSION_A]) })
  const reasons = reasonsFor({ change, work: [work(SESSION_A, [finding('not_proven'), finding('couldnt')])] })
  assert.equal(reasons[0].id, 'observed_failure')
  assert.equal(reasons[0].count, 1)
  assert.deepEqual(reasons.map((r) => r.id), ['observed_failure', 'applicable_finding', 'missing_evidence'])
  assert.equal(reasonLine(reasons[0]).template, C.reasons.observed_failure.line_one)
  assert.equal(reasonLine(reasons[0]).stage, 'verify')
})

test('an open finding that is not against what was said is an applicable finding, counted apart', () => {
  const change = aChange({ ...accepted(['a']), ...linked([SESSION_A]) })
  const reasons = reasonsFor({ change, work: [work(SESSION_A, [finding('couldnt'), finding('loose')])] })
  assert.equal(reasons[0].id, 'applicable_finding')
  assert.equal(reasons[0].count, 2)
  assert.equal(fill(reasonLine(reasons[0]).template, { n: 2 }), '2 things about the work you linked are waiting on evidence.')
})

test('a recording gap is interrupted capture, counted once per piece of work and never as a finding', () => {
  const change = aChange({ ...accepted(['a']), ...linked([SESSION_A, SESSION_B]) })
  const reasons = reasonsFor({
    change,
    work: [
      work(SESSION_A, [finding('fog', 'recording_gap'), finding('fog', 'recording_gap')]),
      work(SESSION_B, [finding('fog', 'recording_gap')]),
    ],
  })
  assert.deepEqual(reasons.map((r) => r.id), ['interrupted_capture', 'missing_evidence'])
  assert.equal(reasons[0].count, 2, 'two pieces of work with gaps, not three cards')
  assert.equal(findingCounts([work(SESSION_A, [finding('fog', 'recording_gap')])]).open, 0, 'a gap is not also an open finding')
})

test('evidence that stopped applying after an edit is its own reason, and it renders only when that record exists', () => {
  const change = aChange({ ...accepted(['a']), ...linked([SESSION_A]) })
  assert.equal(reasonsFor({ change, evidence: [{ criterion: 1 }] }).length, 0, 'nothing is claimed while no such record exists')
  const reasons = reasonsFor({
    change,
    evidence: [{ criterion: 1 }],
    applicability: [{ applies: false }, { applies: true }],
  })
  assert.deepEqual(reasons.map((r) => r.id), ['invalidated_after_edit'])
  assert.equal(reasons[0].count, 1)
  assert.equal(reasonLine(reasons[0]).stage, 'build')
})

test('a decision waiting on a person is its own reason, and it is the lowest', () => {
  const change = aChange({ ...accepted(['a']), decision: 'awaiting_approval' })
  const reasons = reasonsFor({ change, evidence: [{ criterion: 1 }] })
  assert.deepEqual(reasons.map((r) => r.id), ['awaiting_approval'])
  const both = reasonsFor({ change })
  assert.deepEqual(both.map((r) => r.id), ['missing_evidence', 'awaiting_approval'])
  assert.equal(both[0].id, 'missing_evidence', 'the thing that can wait waits')
})

test('a Change carrying several reasons is ONE item: the highest is its line, the rest are listed under it', () => {
  const change = aChange({ ...accepted(['a', 'b']), ...linked([SESSION_A]), decision: 'awaiting_approval' })
  const item = inboxItem({ change, work: [work(SESSION_A, [finding('not_proven'), finding('couldnt'), finding('fog', 'recording_gap')])] })
  assert.equal(item.reason.id, 'observed_failure')
  assert.deepEqual(item.also.map((a) => a.reason.id), ['applicable_finding', 'interrupted_capture', 'missing_evidence', 'awaiting_approval'])
  assert.equal(item.href, '#/changes/' + change.id + '/verify')
  assert.equal(item.title, 'Retry failed uploads')
})

/* ── only confirmed work decides anything ──────────────────────────────── */

test('a suggestion cannot make a reason, cannot change one, and is counted apart', () => {
  const change = aChange({ ...accepted(['a']), ...linked([SESSION_A], 'suggested') })
  const item = inboxItem({ change, work: [work(SESSION_A, [finding('not_proven')])] })
  assert.equal(item.reason.id, 'missing_evidence', 'a suggested piece of work contributed no finding')
  assert.equal(item.also.length, 0)
  assert.equal(item.suggested, 1)
  /* And the same record with the link confirmed says the other thing — so the
   * assertion above is about the filter, not about an empty fixture. */
  const confirmed = inboxItem({ change: aChange({ ...accepted(['a']), ...linked([SESSION_A]) }), work: [work(SESSION_A, [finding('not_proven')])] })
  assert.equal(confirmed.reason.id, 'observed_failure')
  assert.equal(confirmed.suggested, 0)
})

test('a rejected link contributes nothing and is not counted as a suggestion either', () => {
  const change = aChange({ ...accepted(['a']), ...linked([SESSION_A], 'rejected') })
  const item = inboxItem({ change, work: [work(SESSION_A, [finding('not_proven')])] })
  assert.equal(item.reason.id, 'missing_evidence')
  assert.equal(item.suggested, 0)
})

/* ── the queue ─────────────────────────────────────────────────────────── */

test('a Change that needs nothing is not listed, and is counted as settled rather than hidden', () => {
  const settled = aChange({ id: 'cccccccc-0000-4000-8000-00000000c009', ...accepted(['a']) })
  const { items, settled: n } = inboxItems([
    { change: settled, evidence: [{ criterion: 1, state: 'proven' }] },
    { change: aChange() },
  ])
  assert.deepEqual(items.map((i) => i.id), ['cccccccc-0000-4000-8000-00000000c001'])
  assert.equal(n, 1)
})

test('the queue is ordered by reason first and by the recorded opening time within one reason', () => {
  const mk = (id, openedAt, over) => ({ change: aChange({ id, opened_at: openedAt, ...over }) })
  const { items } = inboxItems([
    mk('cccccccc-0000-4000-8000-00000000c001', '2026-01-01T09:00:00Z', accepted(['a'])),
    mk('cccccccc-0000-4000-8000-00000000c002', '2026-01-03T09:00:00Z', {}),
    mk('cccccccc-0000-4000-8000-00000000c003', '2026-01-05T09:00:00Z', accepted(['a', 'b'])),
  ])
  assert.deepEqual(items.map((i) => [i.reason.id, i.id]), [
    ['missing_evidence', 'cccccccc-0000-4000-8000-00000000c003'],
    ['missing_evidence', 'cccccccc-0000-4000-8000-00000000c001'],
    ['awaiting_definition', 'cccccccc-0000-4000-8000-00000000c002'],
  ])
})

test('a Change with no id is no item at all, and is not counted as settled', () => {
  const { items, settled } = inboxItems([{ change: { request: { title: 'x' } } }, null, {}])
  assert.equal(items.length, 0)
  assert.equal(settled, 0)
})

/* ── where the work is ─────────────────────────────────────────────────── */

test('the scope line names the repository and the version, and the version comes from the Change screen’s own function', () => {
  const bound = aChange({
    implementation: {
      version: 1,
      bound_at: '2026-01-04T09:00:00Z',
      repository_identity: 'thruline',
      base_revision: 'a1b2c3d4e5f6',
      snapshot_id: null,
      workspace_identity: null,
    },
  })
  const s = scopeLine(bound)
  assert.equal(s.repository, 'thruline')
  assert.equal(s.version.bound, true)
  assert.equal(s.version.values.revision, 'a1b2c3d')
  assert.equal(fill(HQ_COPY.change.version.bound, s.version.values), 'version a1b2c3d · 2026-01-04')
  const none = scopeLine(aChange())
  assert.equal(none.repository, null)
  assert.equal(none.version.bound, false)
})

test('the address of an item opens the Change at the stage that settles its reason, and never at an invented one', () => {
  assert.equal(changeHrefAt('c1', 'verify'), '#/changes/c1/verify')
  assert.equal(changeHrefAt('c1', 'nowhere'), '#/changes/c1/define')
  assert.equal(changeHrefAt('a b', 'define'), '#/changes/a%20b/define')
})

/* ── what this machine is keeping (`L1.1`) ─────────────────────────────── */

test('a byte count reads as a person reads it, to one decimal, and a size that is not a number is not a zero', () => {
  assert.equal(formatSize(0), '0 B')
  assert.equal(formatSize(999), '999 B')
  assert.equal(formatSize(1000), '1.0 KB')
  assert.equal(formatSize(7_200_000_000), '7.2 GB')
  assert.equal(formatSize(7_249_000_000), '7.2 GB')
  assert.equal(formatSize(7_250_000_000), '7.3 GB')
  assert.equal(formatSize(1_500_000), '1.5 MB')
  for (const bad of [null, undefined, -1, NaN, '7', {}]) assert.equal(formatSize(bad), null, String(bad))
})

test('the storage reading carries the served number and nothing invented', () => {
  const known = storageReading({ bytes: 7_200_000_000, path: '/store' })
  assert.deepEqual(known, { known: true, size: '7.2 GB', path: '/store', also: null, alsoCommandLog: null, bytes: 7_200_000_000, parts: [] })
  /* T-7-1: a second folder the server counted is carried, never invented. */
  assert.equal(storageReading({ bytes: 1, path: '/store', also: '/apart' }).also, '/apart')
  /* T-7-1 verifier: a command log the server counted on its own, in a folder
   * that holds the records folder, is carried the same way. */
  assert.equal(storageReading({ bytes: 1, path: '/store', also_command_log: '/home/exec.db' }).alsoCommandLog, '/home/exec.db')
  /* A different served number is a different sentence — the sentence is the
   * field's, not this file's. */
  assert.equal(storageReading({ bytes: 41_000_000, path: '/store' }).size, '41.0 MB')
  assert.equal(storageReading(null).known, false)
  assert.equal(storageReading({}).known, false)
  assert.equal(storageReading({ bytes: 'lots' }).known, false)
})

/* T-2-10, finding 1: the number is the folder's, and the breakdown says which
 * part of the folder it is. The nouns are the copy file's, largest first, and
 * a component key this build does not know is named rather than dropped —
 * dropping it would make the parts stop adding up to the total a person was
 * just told. */
test('the breakdown names each part from the copy file, largest first, and nothing it was not served', () => {
  const read = storageReading({
    bytes: 3_174_400,
    path: '/store',
    parts: [
      { key: 'command_log', bytes: 184_320 },
      { key: 'preserved_transcripts', bytes: 876_544 },
      { key: 'records', bytes: 2_109_440 },
      { key: 'export', bytes: 0 },
      { key: 'something_new', bytes: 4_096 },
    ],
  })
  assert.deepEqual(read.parts.map((p) => p.key), ['records', 'preserved_transcripts', 'command_log', 'something_new'])
  assert.deepEqual(read.parts.map((p) => p.noun), [
    HQ_COPY.change.inbox.storage.parts.records,
    HQ_COPY.change.inbox.storage.parts.preserved_transcripts,
    HQ_COPY.change.inbox.storage.parts.command_log,
    HQ_COPY.change.inbox.storage.parts.other,
  ])
  assert.deepEqual(read.parts.map((p) => p.size), ['2.1 MB', '876.5 KB', '184.3 KB', '4.1 KB'])
  assert.deepEqual(storageReading({ bytes: 10, path: '/store' }).parts, [])
  for (const bad of [{ key: 'records' }, { bytes: 10 }, { key: 'records', bytes: -1 }]) {
    assert.deepEqual(storageReading({ bytes: 10, path: '/store', parts: [bad] }).parts, [], JSON.stringify(bad))
  }
})
