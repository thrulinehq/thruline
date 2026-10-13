/* Decisions on HQ screens (docs/decisions/API.md, schema_version 1, with the
 * C35 additions): the reader never invents a verdict, and the model every
 * screen uses says only what typed fields say. Whether a decision refuses an
 * approval is the engine's own answer (`approval_gate`, `blocks_approval`),
 * never the state word and the severity. Reads are stubbed; nothing is POSTed. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { DECISIONS_API, approvalRefusals, confirmPlan, decisionRow, decisionSearchItems, decisionsModel, gateNote, gateOf, notCheckedOn, readDecisions, shortVersion, transportOf } from './decisions.js'
import { shortVersion as hqShortVersion } from './project.js'
import {
  DECISIONS_CASES, DECISIONS_DORMANT, DECISIONS_EMPTY, DECISIONS_LIST,
  GATE_ACTIVE, GATE_DORMANT, TRANSPORT_NATIVE, TRANSPORT_TERMINAL, TRANSPORT_UNAVAILABLE, listWith,
} from './decisions-fixture.js'

const answer = (status, body) => async () => ({ ok: status >= 200 && status < 300, status, text: async () => (typeof body === 'string' ? body : JSON.stringify(body)) })
const served = { status: 'served', doc: DECISIONS_LIST }
const CURRENT = 'wt-3c3ca28a1b2c3d4e'
const COMMIT = '3c3ca28a1b2c3d4e5f60718293a4b5c6d7e8f901'
const clone = (x) => JSON.parse(JSON.stringify(x))
const modelOf = (doc) => decisionsModel({ status: 'served', doc })

test('the list address is the contract’s', () => {
  assert.equal(DECISIONS_API.list('p 1'), '/api/hq/projects/p%201/decisions')
  assert.equal(DECISIONS_API.detail('p1', 'dec-01', 'wt-1'), '/api/hq/projects/p1/decisions/dec-01?version=wt-1')
})

test('a server without the route reads "not checked", never a verdict', async () => {
  assert.deepEqual(await readDecisions('p1', answer(404, 'not found')), { status: 'not_served' })
  const m = decisionsModel(await readDecisions('p1', answer(404, { error: 'not_found' })))
  assert.equal(m.status, 'not_served')
  assert.equal(m.message, 'Decisions are not checked in this version of ThruLine.')
  assert.deepEqual([m.rows, m.broken, m.counts], [[], [], null])
})

test('a served list is read; a contract error and a pending upgrade are said, not hidden', async () => {
  assert.equal((await readDecisions('p1', answer(200, DECISIONS_LIST))).status, 'served')
  const missing = await readDecisions('p1', answer(404, { schema_version: 1, error: 'no_such_project', detail: 'no project p1' }))
  assert.deepEqual(missing, { status: 'unreadable', message: 'ThruLine could not read this project’s decisions: no project p1.' })
  const upgrade = await readDecisions('p1', answer(503, { schema_version: 1, error: 'upgrade_pending', detail: null }))
  assert.match(upgrade.message, /thruline upgrade/)
  const down = await readDecisions('p1', async () => { throw new Error('refused') })
  assert.equal(down.status, 'unreadable')
})

test('every state reads in the record’s words', () => {
  const rows = Object.fromEntries(decisionsModel(served).rows.map((r) => [r.id, r]))
  assert.equal(rows['dec-01'].sentence, 'Broken on wt-3c3ca28: the record shows it in 2 places.')
  assert.deepEqual(rows['dec-01'].firstFinding.where, 'ui/src/hq/screens/Approve-Sheet/logic.js:212')
  assert.equal(rows['dec-02'].sentence, 'Holds on wt-3c3ca28, in 310 places checked.')
  assert.equal(rows['dec-03'].sentence, 'No proof found on wt-3c3ca28: the record has no failing run before the first source edit.')
  assert.equal(rows['dec-03'].checkableIf, 'Checkable when a test run is recorded before the source edit.')
  assert.equal(rows['dec-04'].sentence, 'Last checked on wt-0aa1bb2; a later commit moved the code since, so it is not checked on wt-3c3ca28.')
  assert.equal(rows['dec-05'].sentence, 'Proposed. Not in force until you accept it and confirm it yourself.')
})

test('no verdict is never "holds"', () => {
  const d = { ...DECISIONS_LIST.decisions[1], current_verdict: null }
  const row = decisionRow(d, CURRENT)
  assert.equal(row.state, 'not_checked')
  assert.equal(row.sentence, 'Not checked yet.')
})

test('the Inbox’s group is the accepted decisions the record shows broken, blocking first', () => {
  const m = decisionsModel(served)
  assert.deepEqual(m.broken.map((r) => r.id), ['dec-01'])
  assert.deepEqual(m.counts, { decisions: 5, accepted: 4, broken: 1, proposed: 1 })
  assert.deepEqual(m.rows.map((r) => r.state), ['broken', 'unverifiable', 'stale', 'holds', 'proposed'])
  assert.deepEqual(decisionsModel({ status: 'served', doc: DECISIONS_EMPTY }).counts, { decisions: 0, accepted: 0, broken: 0, proposed: 0 })
})

test('approving a version is refused, in the decision’s own words, by the engine’s answer for that exact version', () => {
  const m = decisionsModel(served)
  const refused = approvalRefusals(m, CURRENT)
  assert.deepEqual(refused.map((r) => [r.id, r.kind]), [['dec-01', 'broken'], ['dec-03', 'no_proof']])
  assert.equal(refused[0].words, 'Never show a home-folder path on any screen')
  assert.equal(refused[0].sentence, 'Broken on wt-3c3ca28: the record shows it in 2 places.')
  assert.equal(refused[0].firstFinding.where, 'ui/src/hq/screens/Approve-Sheet/logic.js:212')
  assert.equal(refused[1].sentence, 'No proof found on wt-3c3ca28: the record has no failing run before the first source edit.')
  // another version: the engine said nothing about it, so nothing refuses; the stale one is "not checked on" it
  assert.deepEqual(approvalRefusals(m, 'wt-ffffffffffffffff'), [])
  assert.deepEqual(notCheckedOn(m, CURRENT).map((r) => r.id), ['dec-04'])
  // the commit id names the same version as the HQ id does
  assert.deepEqual(approvalRefusals(m, COMMIT).map((r) => r.id), ['dec-01', 'dec-03'])
  // a warning never refuses, and a stale projection never does by itself
  assert.ok(!refused.some((r) => r.id === 'dec-02' || r.id === 'dec-04'))
})

test('the state word and the severity no longer refuse: only the engine’s blocks_approval does', () => {
  // a broken, blocking decision whose result the engine says does not block (a valid waiver covers it)
  const waived = clone(DECISIONS_LIST)
  waived.decisions[0].current_verdict.waived = { id: 'w1', actor: 'you', because: 'shipping a fix next', at: '2026-10-07T10:00:00Z', version_id: CURRENT, expires_version: null, expires_at: null }
  waived.decisions[0].current_verdict.blocks_approval = false
  waived.decisions[0].blocks_approval = false
  assert.deepEqual(approvalRefusals(modelOf(waived), CURRENT).map((r) => r.id), ['dec-03'])
  // the same broken, blocking state with the typed fields still true is a refusal
  assert.deepEqual(approvalRefusals(modelOf(DECISIONS_LIST), CURRENT).map((r) => r.id), ['dec-01', 'dec-03'])
  // the waiver without the engine saying so refuses nothing new, and removes nothing: the engine decides
  const word = clone(DECISIONS_LIST)
  word.decisions[0].current_verdict.waived = waived.decisions[0].current_verdict.waived
  assert.deepEqual(approvalRefusals(modelOf(word), CURRENT).map((r) => r.id), ['dec-01', 'dec-03'])
  // an unverifiable result the engine does not count (on_unverifiable: warn)
  const warn = clone(DECISIONS_LIST)
  warn.decisions[2].blocks_approval = false
  warn.decisions[2].current_verdict.blocks_approval = false
  assert.deepEqual(approvalRefusals(modelOf(warn), CURRENT).map((r) => r.id), ['dec-01'])
  // a broken decision that only warns
  const warns = clone(DECISIONS_LIST)
  warns.decisions[0].severity = 'warns'
  warns.decisions[0].blocks_approval = false
  warns.decisions[0].current_verdict.blocks_approval = false
  assert.deepEqual(approvalRefusals(modelOf(warns), CURRENT).map((r) => r.id), ['dec-03'])
})

test('each rule, one decision at a time (the gate active)', () => {
  const m = modelOf(DECISIONS_CASES)
  const refused = approvalRefusals(m, CURRENT)
  assert.deepEqual(refused.map((r) => [r.id, r.kind]), [
    ['cs-broken-blocks', 'broken'],
    ['cs-stale-no-check', 'no_check'],
    ['cs-no-verdict', 'no_check'],
  ])
  // not refusing: a broken warning, a valid waiver, an unverifiable result the policy only warns about, a result that holds
  for (const id of ['cs-broken-warns', 'cs-broken-waived', 'cs-unverifiable-warn', 'cs-holds-blocks']) {
    assert.ok(!refused.some((r) => r.id === id), id)
  }
  const row = (id) => m.rows.find((r) => r.id === id)
  assert.equal(row('cs-broken-waived').waived.actor, 'you')
  assert.equal(row('cs-unverifiable-warn').state, 'unverifiable')
})

test('a missing check is its own refusal, distinct from a broken one, and says so in the decision’s own words', () => {
  const m = modelOf(DECISIONS_CASES)
  const refused = approvalRefusals(m, CURRENT)
  const none = refused.find((r) => r.id === 'cs-no-verdict')
  assert.equal(none.words, 'The privacy sentence is on every settings screen')
  assert.equal(none.kind, 'no_check')
  assert.equal(none.sentence, 'No check is recorded on wt-3c3ca28 yet.')
  assert.equal(none.firstFinding, null)
  const stale = refused.find((r) => r.id === 'cs-stale-no-check')
  assert.equal(stale.sentence, 'No check is recorded on wt-3c3ca28 yet. The latest result is on wt-0aa1bb2.')
  assert.notEqual(none.kind, refused.find((r) => r.id === 'cs-broken-blocks').kind)
  // it refuses for the version the engine answered about, and for no other
  assert.deepEqual(approvalRefusals(m, 'wt-ffffffffffffffff'), [])
})

test('a dormant gate refuses nothing, whatever the state words say, and the gate’s own reason is said', () => {
  const m = modelOf(DECISIONS_DORMANT)
  assert.equal(m.gate.active, false)
  assert.deepEqual(approvalRefusals(m, CURRENT), [])
  // the states are still what the record shows
  assert.deepEqual(m.rows.map((r) => r.state), ['broken', 'unverifiable', 'stale', 'holds', 'proposed'])
  // and even if a result still carried a true flag, a dormant gate enables no refusal
  const odd = clone(DECISIONS_DORMANT)
  odd.decisions[0].blocks_approval = true
  odd.decisions[0].current_verdict.blocks_approval = true
  assert.deepEqual(approvalRefusals(modelOf(odd), CURRENT), [])
  assert.equal(gateNote(m), 'Decisions you set do not block approving right now. Decisions approval is dormant in this record.')
  // blocking decisions are not drawn as blocking while nothing is enforced
  assert.equal(m.rows.find((r) => r.id === 'dec-01').severityWords, 'Set to block approval')
  assert.equal(modelOf(DECISIONS_LIST).rows.find((r) => r.id === 'dec-01').severityWords, 'Blocks approval')
  assert.equal(modelOf(DECISIONS_LIST).rows.find((r) => r.id === 'dec-02').severityWords, 'Warns')
})

test('an active gate has no note; no blocking decision means nothing to say; a server that served no gate says it cannot tell', () => {
  assert.equal(gateNote(modelOf(DECISIONS_LIST)), null)
  const noBlocking = clone(DECISIONS_DORMANT)
  noBlocking.decisions = noBlocking.decisions.filter((d) => d.severity !== 'blocks')
  assert.equal(gateNote(modelOf(noBlocking)), null)
  assert.equal(gateNote(modelOf(DECISIONS_EMPTY)), null)
  const older = clone(DECISIONS_LIST)
  delete older.approval_gate
  delete older.confirmation_transport
  const m = modelOf(older)
  assert.deepEqual(m.gate, { known: false, active: null, reason: null })
  assert.equal(m.transport, null)
  assert.deepEqual(approvalRefusals(m, CURRENT), [], 'HQ derives no refusal from the state word; the server stays the gate')
  assert.match(gateNote(m), /did not say whether decisions block approving/)
  assert.equal(gateNote(decisionsModel(undefined)), null)
})

test('a gate member with no reason, and the gate’s reason is shown exactly as given', () => {
  const odd = clone(DECISIONS_DORMANT)
  odd.approval_gate = { active: false, reason: null, members: [] }
  assert.equal(gateNote(modelOf(odd)), 'Decisions you set do not block approving right now.')
  const given = clone(DECISIONS_DORMANT)
  given.approval_gate.reason = 'No blocking decision is confirmed (“p1”).'
  assert.equal(gateNote(modelOf(given)), 'Decisions you set do not block approving right now. No blocking decision is confirmed (“p1”).')
})

test('a result whose own flag is false but whose decision’s is true is still the engine’s refusal', () => {
  const doc = clone(DECISIONS_LIST)
  doc.decisions[1].blocks_approval = true // dec-02 holds, warns: odd, but the typed field decides
  const r = approvalRefusals(modelOf(doc), CURRENT).find((x) => x.id === 'dec-02')
  assert.equal(r.kind, 'blocked')
  assert.match(r.sentence, /^Holds on wt-3c3ca28, in 310 places checked\. ThruLine’s approval check still counts it as blocking\.$/)
})

test('the fixture carries exactly the shapes API.md names (a test of the fixture, so a shape change is seen here first)', () => {
  const keys = (o) => Object.keys(o).sort()
  const LIST = ['approval_gate', 'confirmation_transport', 'current_version', 'decisions', 'project_id', 'schema_version']
  const DECISION = ['acceptance_reason', 'accepted_at', 'accepted_by', 'applies_from_version', 'blocks_approval', 'current_verdict', 'id', 'kind', 'kind_code', 'lifecycle', 'params', 'params_schema_version', 'params_sha256', 'project_id', 'revision', 'severity', 'sources', 'span']
  const VERDICT = ['basis_state', 'blocks_approval', 'check_impl_version', 'checkable_if', 'checked_places', 'computed_at', 'decision_id', 'finding_count', 'findings', 'id', 'last_held_version', 'not_looked_at', 'reason', 'record_projection', 'revision', 'stale_reason', 'state', 'surfaces', 'tool_versions', 'version', 'visual', 'waived']
  const FINDING = ['diff_image_path', 'difference_pct', 'edit_id', 'id', 'introduced_at', 'line', 'message', 'path', 'present_since_version', 'reference_hash', 'screen', 'session_id', 'span', 'surface', 'thread_id']
  for (const doc of [DECISIONS_LIST, DECISIONS_EMPTY, DECISIONS_DORMANT, DECISIONS_CASES]) {
    assert.deepEqual(keys(doc), LIST)
    assert.deepEqual(keys(doc.approval_gate), ['active', 'members', 'reason'])
    for (const m of doc.approval_gate.members) assert.deepEqual(keys(m), ['active', 'project_id', 'reason'])
    assert.deepEqual(keys(doc.confirmation_transport), ['available', 'browser_available', 'native', 'preparation_is_person_act', 'reason', 'requires_presence'])
    assert.deepEqual(keys(doc.confirmation_transport.native), ['available', 'reason', 'schema_version'])
    for (const d of doc.decisions) {
      assert.deepEqual(keys(d), DECISION, d.id)
      assert.equal(typeof d.blocks_approval, 'boolean', d.id)
      if (d.current_verdict) {
        assert.deepEqual(keys(d.current_verdict), VERDICT, d.id)
        assert.equal(typeof d.current_verdict.blocks_approval, 'boolean', d.id)
        for (const f of d.current_verdict.findings) assert.deepEqual(keys(f), FINDING, f.id)
      }
    }
  }
  // a proposal has no severity yet (API.md: null before an acceptance exists)
  assert.equal(DECISIONS_LIST.decisions.find((d) => d.lifecycle === 'proposed').severity, null)
  // the transports, one of each word the contract has
  assert.deepEqual([TRANSPORT_NATIVE, TRANSPORT_TERMINAL, TRANSPORT_UNAVAILABLE].map((t) => t.available), ['native_click', 'terminal', 'unavailable'])
  assert.equal(GATE_ACTIVE.active, true)
  assert.equal(GATE_DORMANT.active, false)
  assert.equal(typeof GATE_DORMANT.reason, 'string')
})

test('the gate and the transport are read as the server serves them', () => {
  assert.deepEqual(gateOf(DECISIONS_LIST), { known: true, active: true, reason: null })
  assert.deepEqual(gateOf(DECISIONS_DORMANT), { known: true, active: false, reason: 'Decisions approval is dormant in this record.' })
  assert.deepEqual(gateOf({}), { known: false, active: null, reason: null })
  assert.deepEqual(gateOf({ approval_gate: { active: 'yes' } }), { known: false, active: null, reason: null })
  assert.deepEqual(transportOf(DECISIONS_LIST), { mode: 'native_click', native: true, reason: null })
  // native only when BOTH the word and the native preflight say so
  const half = { confirmation_transport: { ...TRANSPORT_NATIVE, native: { schema_version: 1, available: false, reason: 'The preflight said no.' } } }
  assert.deepEqual(transportOf(half), { mode: 'native_click', native: false, reason: 'The preflight said no.' })
  const word = { confirmation_transport: { ...TRANSPORT_TERMINAL, native: { schema_version: 1, available: true, reason: null } } }
  assert.equal(transportOf(word).native, false)
  assert.deepEqual(transportOf({ confirmation_transport: TRANSPORT_TERMINAL }), { mode: 'terminal', native: false, reason: 'The native review needs ThruLine to run in the foreground of its own terminal window.' })
  assert.equal(transportOf({}), null)
  assert.equal(transportOf({ confirmation_transport: { available: 'something_new' } }).mode, 'unavailable')
})

test('"Confirm on this Mac" is planned only where the terminal path is, only on a native transport, and only with the exact fields the server needs', () => {
  const rows = Object.fromEntries(modelOf(DECISIONS_LIST).rows.map((r) => [r.id, r]))
  // waive: the exact commit of the result; retire: nothing more; accept: its revision and the commit it applies from
  assert.deepEqual(confirmPlan(rows['dec-01'], 'waive'), { offer: true, action: 'waive', fixed: { version_id: COMMIT } })
  assert.deepEqual(confirmPlan(rows['dec-01'], 'retire'), { offer: true, action: 'retire', fixed: {} })
  assert.deepEqual(confirmPlan(rows['dec-05'], 'accept'), { offer: true, action: 'accept', fixed: { revision: 1, applies_from_version: COMMIT } })
  // where the terminal path is not offered, nothing is
  assert.deepEqual(confirmPlan(rows['dec-05'], 'waive'), { offer: false, note: null })
  assert.deepEqual(confirmPlan(rows['dec-01'], 'accept'), { offer: false, note: null })
  assert.deepEqual(confirmPlan(null, 'retire'), { offer: false, note: null })
})

test('when the transport is not native, the server’s own reason is said as given and only the terminal path remains', () => {
  const asked = (transport) => Object.fromEntries(modelOf(listWith({ decisions: DECISIONS_LIST.decisions, transport })).rows.map((r) => [r.id, r]))['dec-01']
  const terminal = confirmPlan(asked(TRANSPORT_TERMINAL), 'retire')
  assert.equal(terminal.offer, false)
  assert.equal(terminal.note, 'Confirming on this Mac from this page is not available here. The native review needs ThruLine to run in the foreground of its own terminal window.')
  const gone = confirmPlan(asked(TRANSPORT_UNAVAILABLE), 'retire')
  assert.equal(gone.offer, false)
  assert.ok(gone.note.endsWith('The native review is only available on macOS.'))
  // no reason served: still no control, and the terminal path is named
  const bare = confirmPlan(asked({ ...TRANSPORT_TERMINAL, reason: null, native: { schema_version: 1, available: false, reason: null } }), 'retire')
  assert.equal(bare.note, 'Confirming on this Mac from this page is not available here. Use the terminal path above.')
  // no transport served at all: say nothing
  const none = clone(DECISIONS_LIST)
  delete none.confirmation_transport
  assert.deepEqual(confirmPlan(modelOf(none).rows.find((r) => r.id === 'dec-01'), 'retire'), { offer: false, note: null })
})

test('a missing exact commit is said, not guessed', () => {
  // dirty tree: the list has no current version, so a proposal cannot name where it applies from
  const dirty = listWith({ decisions: DECISIONS_LIST.decisions, current: null })
  const accept = confirmPlan(modelOf(dirty).rows.find((r) => r.id === 'dec-05'), 'accept')
  assert.equal(accept.offer, false)
  assert.match(accept.note, /no exact commit to apply this from right now/)
  // a result that is not on a commit cannot be set aside from the page
  const snap = clone(DECISIONS_LIST)
  snap.decisions[0].current_verdict.version.commit_oid = null
  const waive = confirmPlan(modelOf(snap).rows.find((r) => r.id === 'dec-01'), 'waive')
  assert.equal(waive.offer, false)
  assert.match(waive.note, /not on a commit/)
})

test('the copy added for these rules uses no intent word, no confidence and no time-zone name', () => {
  const m = modelOf(DECISIONS_CASES)
  const said = [
    ...approvalRefusals(m, CURRENT).map((r) => r.sentence),
    gateNote(modelOf(DECISIONS_DORMANT)), gateNote(decisionsModel(undefined)),
    confirmPlan(modelOf(listWith({ decisions: DECISIONS_LIST.decisions, transport: TRANSPORT_TERMINAL })).rows[0], 'retire').note,
  ].filter(Boolean).join(' ')
  assert.ok(said.length > 100)
  assert.doesNotMatch(said, /\b(lie|lied|cheat\w*|violat\w*|ignor\w*|hallucinat\w*|trial)\b|%|\b(UTC|GMT|PST|IST)\b/i)
})

test('⌘K finds a decision by its words, its kind and its state', () => {
  const items = decisionSearchItems(decisionsModel(served), (r) => '#/x/' + r.id)
  const find = (q) => items.filter((i) => i.hay.includes(q)).map((i) => i.id)
  assert.deepEqual(find('home-folder'), ['d:dec-01'])
  assert.deepEqual(find('a5'), ['d:dec-01'])
  assert.ok(find('broken on').includes('d:dec-01'))
  assert.ok(!find('warns').includes('d:dec-05'), 'a proposal has no severity yet, so it is not found as one that warns')
  assert.ok(find('warns').includes('d:dec-02'))
  assert.equal(items[0].kind, 'Decision')
  assert.equal(items[0].href, '#/x/dec-01')
})

test('no sentence uses an intent word, a confidence or the word trial', () => {
  const rows = decisionsModel(served).rows
  const words = rows.flatMap((r) => [r.sentence, r.checkableIf, r.waivedSentence]).filter(Boolean).join(' ')
  assert.doesNotMatch(words, /\b(lie|lied|cheat\w*|violat\w*|ignor\w*|hallucinat\w*|trial)\b|%/i)
})

test('a decision names a version exactly as every other HQ screen does (one answer)', () => {
  for (const id of ['wt-3c3ca28a1b2c3d4e', '3c3ca28a1b2c3d4e5f60718293a4b5c6d7e8f901', 'wt-0aa1bb2cc3dd4ee5']) {
    assert.equal(shortVersion(id), hqShortVersion(id), id)
  }
  assert.equal(shortVersion(null), null)
  assert.equal(decisionRow(DECISIONS_LIST.decisions[0], CURRENT).version, 'wt-3c3ca28')
})
