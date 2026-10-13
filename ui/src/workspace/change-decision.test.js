/* T-S-4 acceptance item 0 — the screen, in the DOM.
 *
 * Sam's question is "what exactly did I just approve, and can I take it back?",
 * and this file is that question asked of the rendered markup rather than of
 * the model. Four states are walked, in the order a person meets them:
 *
 *   1. nothing decided, the record backs the line  → *Approve this version*,
 *      naming the version, with the sentence that says what approving does NOT
 *      do beside it;
 *   2. approved                                    → "Approved on e71c5b0",
 *      the same sentence, and *Withdraw approval* live;
 *   3. approved, and the code moved since          → "Approved on e71c5b0 —
 *      code changed since", with the approval still naming the OLD version;
 *   4. an approval refused because the tree moved  → the refusal and the list
 *      of files, from the server's answer.
 *
 * Every string asserted here comes from `change-model.js` via `HQ_COPY`; the
 * test reaches for the rendered text, so a screen that grew its own wording
 * would fail rather than agree with itself.
 *
 * Every fixture is synthetic. No string here came off a real machine.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { licenceView } from './licence-view.js'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-decide-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeScreen, MovedRefusal } from './change-workspace.jsx';
  import { InboxItem, InboxScreen, assembleInbox } from './inbox.jsx';
  import { PacketPage } from './packet.jsx';
  import { buildPacketView } from './packet-view.js';
  import { deriveChangeModel, decisionOf, movedSentences, shortVersions } from './change-model.js';
  import { buildChangeView } from './change-view.js';
  import { buildCoverageView, evidenceEntries } from './coverage-view.js';
  import { inboxItem } from './inbox-view.js';
  export { deriveChangeModel, decisionOf, movedSentences, shortVersions, buildChangeView, buildCoverageView, evidenceEntries, inboxItem, buildPacketView, assembleInbox };
  export const renderPacket = props => renderToStaticMarkup(React.createElement(PacketPage, props));
  export const renderChange = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderRefused = props => renderToStaticMarkup(React.createElement(MovedRefusal, props));
  export const renderRow = props => renderToStaticMarkup(React.createElement('ul', null, React.createElement(InboxItem, props)));
  export const renderInbox = props => renderToStaticMarkup(React.createElement(InboxScreen, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'decide.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

/* ── one Change whose one accepted line the record backs ──────────────── */

const VERSION_A = 'e71c5b0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
const VERSION_B = '9f3d21ebbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'

const changeAt = (revision, decisions) => ({
  id: 'ch1',
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Totals add up on the checkout page' },
  implementation: { snapshot_id: revision, bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1,
    criteria: ['The totals add up'],
    allowed_scope: ['checkout/'],
    check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet',
  decisions,
})

const NONE = { standing: null, history: [], target_revision: VERSION_A, scope_version: 1 }

const APPROVAL = {
  id: 'cd_1', act: 'approved', target_revision: VERSION_A, scope_version: 1,
  actor: 'sam', decided_at: '2026-09-22T11:00:00Z', withdraws: null, record_path: null,
}

const WITHDRAWAL = {
  id: 'cd_2', act: 'withdrawn', target_revision: VERSION_A, scope_version: 1,
  actor: 'sam', decided_at: '2026-09-22T12:00:00Z', withdraws: 'cd_1', record_path: null,
}

const FULL_COVERAGE = {
  scope_version: 1,
  link_rows: 1,
  execution_sessions: ['s1'],
  criteria: [{
    index: 1,
    text: 'The totals add up',
    state: 'evidence',
    items: [{
      link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run',
      name: 'tests pass', observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies',
      fresh: true, fresh_slug: 'applicability_still_applies',
    }],
  }],
  /* T-3-5 — the Approve door opens on readiness, so a fixture that expects to
   * be offered approval has to be a record that IS ready. This block is the
   * shape `change_readiness::compute` serves for exactly that. */
  readiness: {
    schema_version: 1,
    ready: true,
    scope_version: 1,
    target_version: 'a1b2c3d4e5f6',
    headline_slug: 'readiness_ready',
    obligations: [
      { id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: null, waiver: null, waivable: false },
      { id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: 'The totals add up', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: 'tests pass', waiver: null, waivable: true },
    ],
    rests_on: [{ obligation_id: 'criterion:1', text: 'The totals add up', evidence_name: 'tests pass' }],
    missing: [],
    waived: [],
    not_applicable: [],
  },
}

/** The Change screen and the Inbox row, off ONE derived model, as the app does. */
function screen(change) {
  const COVERAGE = FULL_COVERAGE
  const hq = M.deriveChangeModel(change, FULL_COVERAGE, null, null, [])
  const model = M.buildChangeView({ change, proposals: null, work: [], evidence: M.evidenceEntries(FULL_COVERAGE) })
  model.coverage = M.buildCoverageView(FULL_COVERAGE)
  model.examined = null
  model.hq = hq
  /* T-DIST-4: approving needs a licence that works. These walks are about the
   * decision itself, so the machine they run on has one; the unlicensed
   * screen is `licence.test.js`'s. */
  model.licence = licenceView({ status: 'valid', plan: 'pro', expires_on: '23 Sep 2027', entitlements: { approve_version: { allowed: true } }, technical: {} })
  const row = M.inboxItem({ change, proposals: null, work: [], evidence: [], applicability: [], hq })
  return {
    hq,
    dom: M.renderChange({ model, stage: 'verify', onGo: () => {}, onCopy: () => {}, notes: [], onReload: () => {} }),
    row: row ? M.renderRow({ item: row }) : '',
  }
}

const NOT_AN_ACTION = 'It does not merge, deploy or tell the agent anything.'

/* ── 1. nothing decided yet ───────────────────────────────────────────── */

test('the Inspector’s block 4 offers Approve this version, naming the version, with the sentence that says what it does not do', () => {
  const { hq, dom } = screen(changeAt(VERSION_A, NONE))
  assert.equal(hq.state, 'proven', 'the fixture is the state the decision is offered in')
  const inspector = dom.slice(dom.indexOf('ws-change-inspector'))
  assert.ok(inspector.includes('What to do'), 'block 4 renders')
  assert.ok(inspector.includes('Approve this version'), 'the primary is the decision')
  assert.ok(inspector.includes(NOT_AN_ACTION), 'the sentence is beside the control, not in a help page')
  /* The version it is about is named on the screen, in the one version line. */
  assert.ok(dom.includes('version e71c5b0'), 'the version is named')
  /* Nothing has been approved, and the screen says so rather than staying silent. */
  assert.ok(inspector.includes('Nothing has been approved for this version yet.'))
  assert.ok(!inspector.includes('Withdraw approval'), 'there is nothing to withdraw yet')
})

test('an approval with no name on it is refused by the control itself, and says what would turn it on', () => {
  const { dom } = screen(changeAt(VERSION_A, NONE))
  const inspector = dom.slice(dom.indexOf('ws-change-inspector'))
  assert.ok(inspector.includes('An approval has to have a name on it.'))
  assert.ok(inspector.includes('Put your name in the box above, and the button turns on.'))
  assert.ok(inspector.includes('disabled'), 'the control is off until it is named')
})

/* ── 2. approved ──────────────────────────────────────────────────────── */

test('after approval the headline reads Approved on the version, with the sentence, and Withdraw approval is live', () => {
  const approved = { standing: APPROVAL, history: [APPROVAL], target_revision: VERSION_A, scope_version: 1 }
  const { hq, dom, row } = screen(changeAt(VERSION_A, approved))
  assert.equal(hq.decision.headline, 'Approved on e71c5b0')
  assert.equal(hq.decision.movedSince, false)
  assert.ok(dom.includes('Approved on e71c5b0'), 'the headline is on the screen')
  assert.ok(dom.includes(NOT_AN_ACTION), 'the sentence is beside it')
  assert.ok(dom.includes('Approved by sam on 2026-09-22, against what counts as done, version 1.'),
    'who, when, and which accepted-criteria version')
  assert.ok(dom.includes('Withdraw approval'), 'taking it back is visible')
  /* P6-INBOX-REASON (T-3-4): this Change is proven AND approved, so it needs
   * nothing from anybody and the Inbox does not list it at all. Before the
   * fix the row existed and read "has no evidence yet" beside this very
   * headline, because `reasonsFor` was answering from a hard-coded empty
   * evidence list. A row that is absent for the right reason is the fix;
   * agreement on a Change that IS listed is asserted below. */
  assert.equal(row, '', 'a proven, approved Change is settled and is not in the Inbox')
  /* Agreement is still asserted, at the field the row renders: the Inbox reads
   * the model's own decision headline and derives none of its own. */
  assert.equal(hq.inboxRow.decision, 'Approved on e71c5b0')
  assert.equal(hq.inboxRow.decision, hq.decision.headline)
})

/* ── 3. approved, and the code moved since ────────────────────────────── */

test('after an edit the headline reads Approved on the OLD version — code changed since, and the approval never moves to the new one', () => {
  const approved = { standing: APPROVAL, history: [APPROVAL], target_revision: VERSION_B, scope_version: 1 }
  const change = changeAt(VERSION_B, approved)
  const { hq, dom, row } = screen(change)
  assert.equal(hq.decision.movedSince, true)
  assert.equal(hq.decision.headline, 'Approved on e71c5b0 — code changed since')
  assert.equal(hq.decision.version, 'e71c5b0', 'the approval still names the version it was given for')
  assert.ok(dom.includes('Approved on e71c5b0 — code changed since'))
  /* D-050 (SEAM-INBOX-APPROVED-VERIFY finding 1). This used to assert
   * `row === ''` — "proven and approved: nothing is asked of anybody". That
   * expected value encoded the defect: the approval names e71c5b0, the record
   * now stands at 9f3d21e (what "Check it again" does), and the Inbox said
   * "nothing on it is waiting on you" while this screen said the code changed
   * since. The approval covers e71c5b0 only, so deciding about 9f3d21e is
   * waiting on a person, and the row says so. */
  assert.match(row, /The code has changed since the version that was approved, and the approval covers only that version\./)
  assert.match(row, /Approved on e71c5b0 — code changed since/)
  assert.doesNotMatch(row, /older/)
  assert.ok(hq.attention.reasons.some((r) => r.id === 'approved_code_moved'), 'the Change\'s own attention names it too')
  assert.equal(hq.inboxRow.decision, 'Approved on e71c5b0 — code changed since',
    'the Inbox reads the model\'s headline and never re-derives it')
  const docs = new Map([[change.id, { change, coverage: FULL_COVERAGE, applicability: null, proposals: null, work: [] }]])
  const state = M.assembleInbox([{ id: change.id, opened_at: change.opened_at, request: change.request }], docs)
  assert.equal(state.items.length, 1, 'the approved-then-re-bound Change is an Inbox item')
  assert.equal(state.settled, 0, 'and is never counted as settled')
  const inbox = M.renderInbox({ state: { ...state, storage: { known: false }, notes: [] } })
  assert.doesNotMatch(inbox, /Nothing needs your attention right now/)
  assert.doesNotMatch(inbox, /nothing on it is waiting on you/)
  const you = inbox.indexOf('Waiting on you')
  assert.ok(you >= 0 && inbox.indexOf('Approved on e71c5b0 — code changed since', you) > you, 'filed under Waiting on you')
  /* The screen's own version line has moved on; the approval has not. */
  assert.ok(dom.includes('version 9f3d21e'), 'the record stands at the new version')
  assert.ok(!dom.includes('Approved on 9f3d21e'), 'the approval did not inherit')
})

test('INBOX-APPROVED-THEN-MOVED: a standing approval on the recorded version is attention when the served code has moved', () => {
  const approved = { standing: APPROVAL, history: [APPROVAL], target_revision: VERSION_A, scope_version: 1 }
  const moved = { recorded: VERSION_A, now: VERSION_B, changed_paths: ['checkout/totals.py'], commits: 1, uncommitted_compared: false }
  const change = { ...changeAt(VERSION_A, approved), code_moved: moved }
  const { hq, dom, row } = screen(change)
  assert.equal(hq.decision.approved, true, 'the approval still stands')
  assert.equal(hq.decision.movedSince, false, 'the recorded target has not been rebound')
  assert.equal(hq.codeMoved.recorded, moved.recorded, 'Brief reads the served comparison')
  assert.match(dom, /The code has changed since version e71c5b0 was recorded/)
  assert.match(row, /The code has changed since the version that was approved, and the approval covers only that version\./)
  assert.doesNotMatch(row, /older/)
  assert.match(row, /Approved on e71c5b0/)
  assert.match(row, /The code has changed since version e71c5b0 was recorded/)

  const docs = new Map([[change.id, { change, coverage: FULL_COVERAGE, applicability: null, proposals: null, work: [] }]])
  const state = M.assembleInbox([{ id: change.id, opened_at: change.opened_at, request: change.request }], docs)
  const inbox = M.renderInbox({ state: { ...state, storage: { known: false }, notes: [] } })
  assert.equal(state.items.length, 1, 'the approved Change is an Inbox item')
  assert.equal(state.settled, 0)
  assert.doesNotMatch(inbox, /Nothing needs your attention right now/)

  for (const code_moved of [null, { ...moved, now: moved.recorded }]) {
    const unchanged = screen({ ...change, code_moved })
    assert.equal(unchanged.row, '', 'without a served code move, a same-version approval remains settled')
  }
  const withdrawn = { standing: null, history: [APPROVAL, WITHDRAWAL], target_revision: VERSION_A, scope_version: 1 }
  const afterWithdrawal = screen({ ...change, decisions: withdrawn })
  assert.doesNotMatch(afterWithdrawal.row, /approval covers only that version/, 'history does not revive a withdrawn approval')

  /* Advisory 4a: the served comparison says Some with commits = 0 when the
   * folder is behind or beside the recorded commit. The row still names the
   * standing approval and the move, and claims no direction. */
  const behind = screen({ ...change, code_moved: { ...moved, commits: 0 } })
  assert.match(behind.row, /The code has changed since the version that was approved, and the approval covers only that version\./)
  assert.match(behind.row, /Approved on e71c5b0/)
  assert.doesNotMatch(behind.row, /older|newer/)
})

/* ── 4. the refusal, with what changed ────────────────────────────────── */

test('a refused approval says it was refused and lists the files that changed; with no list it says the record cannot name them', () => {
  const named = M.movedSentences({ outcome: 'moved', seen: VERSION_A, now: VERSION_B, changed: ['checkout/totals.py', 'checkout/cart.py'] })
  assert.ok(named.lines[0].startsWith('Not approved.'))
  assert.equal(named.lines[1], '2 files changed since this screen loaded:')
  assert.deepEqual(named.files, ['checkout/totals.py', 'checkout/cart.py'])
  assert.ok(named.enabler.includes('then approve that one'))

  const one = M.movedSentences({ outcome: 'moved', changed: ['checkout/totals.py'] })
  assert.equal(one.lines[1], 'One file changed since this screen loaded:')

  /* An empty list is NOT "nothing moved". */
  const unnamed = M.movedSentences({ outcome: 'moved', changed: [] })
  assert.equal(unnamed.lines[1], 'The record cannot name which files changed, because it does not hold a full copy of both versions.')
  assert.deepEqual(unnamed.files, [])
})

/* ── withdrawal is its own act, and it stays in the history ───────────── */

test('a withdrawal is its own recorded act: the approval stops standing, both acts stay readable, and the screen offers approval again', () => {
  const withdrawn = { standing: null, history: [APPROVAL, WITHDRAWAL], target_revision: VERSION_A, scope_version: 1 }
  const { hq, dom } = screen(changeAt(VERSION_A, withdrawn))
  assert.equal(hq.decision.approved, false, 'nothing stands')
  assert.equal(hq.decision.history.length, 2, 'both acts are still there')
  assert.deepEqual(hq.decision.history.map((h) => h.act), ['approved', 'withdrawn'])
  assert.ok(dom.includes('Approved on e71c5b0 by sam on 2026-09-22, against what counts as done, version 1.'))
  assert.ok(dom.includes('The approval on e71c5b0 was withdrawn by sam on 2026-09-22.'))
  assert.ok(dom.includes('Approve this version'), 'approval is on offer again')
  /* The history still says "Approved on e71c5b0 by sam …" — that act happened
   * and is never removed. What must not be there is the HEADLINE. */
  assert.equal(hq.decision.headline, null, 'nothing stands, so nothing is headlined')
  assert.ok(!dom.includes('>Approved on e71c5b0</p>'), 'the withdrawn approval is not still the headline')
})

/* ── the act carries the WHOLE revision, not the seven a person reads ─── */

test('the version the act sends is the full recorded revision; the seven characters are for reading only', () => {
  const approved = { standing: APPROVAL, history: [APPROVAL], target_revision: VERSION_A, scope_version: 1 }
  const decision = M.decisionOf(changeAt(VERSION_A, approved), 'e71c5b0')
  assert.equal(decision.version, 'e71c5b0', 'what a person reads')
  assert.equal(decision.seen, VERSION_A, 'what the act sends')
  assert.equal(decision.approvedFull, VERSION_A, 'what a withdrawal names')
})

/* ── the words are the copy file's, and the screen writes none of its own ─ */

test('every decision string the screen renders is one the view-model produced', () => {
  const approved = { standing: APPROVAL, history: [APPROVAL], target_revision: VERSION_A, scope_version: 1 }
  const { hq, dom } = screen(changeAt(VERSION_A, approved))
  for (const said of [hq.decision.headline, hq.decision.sentence, hq.decision.by, hq.decision.withdrawLabel]) {
    assert.ok(dom.includes(said), `the screen renders the model's own "${said}"`)
  }
})

/* ── T-S-4-F2: the packet carries the approval history ────────────────── */

/** The packet page, off the SAME derived model the Change screen reads. */
function packet(change) {
  const hq = M.deriveChangeModel(change, FULL_COVERAGE, null, null, [])
  const model = M.buildPacketView({ change, proposals: null, work: [], coverage: FULL_COVERAGE, evidence: [], hq })
  model.hq = hq
  return M.renderPacket({ model, notes: [], onCopy: () => {} })
}

test('the packet renders the approval history — both acts, in order, from the one view-model', () => {
  const both = { standing: null, history: [APPROVAL, WITHDRAWAL], target_revision: VERSION_A, scope_version: 1 }
  const dom = packet(changeAt(VERSION_A, both))
  assert.ok(dom.includes('Approved on e71c5b0 by sam on 2026-09-22, against what counts as done, version 1.'))
  assert.ok(dom.includes('The approval on e71c5b0 was withdrawn by sam on 2026-09-22.'))
  assert.ok(dom.includes(NOT_AN_ACTION), 'the packet says what an approval does not do too')
  assert.ok(!dom.includes('Nobody has approved this'), 'the section is no longer unconditionally empty')
})

test('a Change nobody has decided still says so, rather than claiming a history', () => {
  const dom = packet(changeAt(VERSION_A, NONE))
  assert.ok(dom.includes('Nobody has approved this'), 'no decision reads as no decision')
})

/* ── T-5-2 — an approval never survives a version it did not see ──────── */

/* `B10.9`. A history of "Approved by sam" then "Approval withdrawn by sam"
 * reads as one decision that followed the code. These two acts were about two
 * different versions, and every line now says which. */
test('every line of the decision history names the version its act was about', () => {
  const laterWithdrawal = { ...WITHDRAWAL, target_revision: VERSION_B }
  const both = { standing: null, history: [APPROVAL, laterWithdrawal], target_revision: VERSION_B, scope_version: 1 }
  const decision = M.decisionOf(changeAt(VERSION_B, both), '9f3d21e')
  assert.deepEqual(decision.history.map((h) => h.version), ['e71c5b0', '9f3d21e'])
  assert.equal(decision.history[0].line,
    'Approved on e71c5b0 by sam on 2026-09-22, against what counts as done, version 1.')
  assert.equal(decision.history[1].line,
    'The approval on 9f3d21e was withdrawn by sam on 2026-09-22.')
  const dom = packet(changeAt(VERSION_B, both))
  assert.ok(dom.includes('Approved on e71c5b0 by sam'), 'the packet carries the version of each act')
  assert.ok(dom.includes('The approval on 9f3d21e was withdrawn'))
})

/* T-3-6-F5. The act used to be allowed to send an empty version when the
 * served document carried none, which the server reads as "I saw nothing" and
 * refuses as a moved tree for the wrong reason. */
test('the version the act sends is never empty while the record names one anywhere', () => {
  const noTarget = { standing: null, history: [], target_revision: null, scope_version: 1 }
  const decision = M.decisionOf(changeAt(VERSION_A, noTarget), 'e71c5b0')
  assert.equal(decision.seen, VERSION_A, 'it falls back to the version the screen rendered')
  assert.notEqual(decision.seen, '', 'never the empty string')

  const noneAnywhere = M.decisionOf({ ...changeAt(VERSION_A, noTarget), implementation: null }, null)
  assert.equal(noneAnywhere.seen, null, 'and when the record names none, it says so rather than sending ""')
  assert.equal(noneAnywhere.canApprove, false, 'with no version there is nothing to approve')
})

/* `Q13`. "It moved" without saying from what to what is a feeling; the line
 * names both versions, and it is on Brief, where "is this approved?" is read,
 * not only in the refusal that follows pressing Approve. */
test('an approval that no longer covers the record says so on Brief, naming both versions', () => {
  const approved = { standing: APPROVAL, history: [APPROVAL], target_revision: VERSION_B, scope_version: 1 }
  const { hq, dom } = screen(changeAt(VERSION_B, approved))
  /* MOVED-WHILE-OPEN-TRUTH moved this expected value (D-050). It read "This Change moved while this screen was open — …",
   * which a fresh derivation cannot know: the screen may have been opened after the move. The old value encoded that
   * false sentence about the reader's own session. A screen that WATCHED the version change says it
   * (moved-while-open-truth.test.js); every other says the two facts and nothing about itself. */
  assert.equal(hq.decision.movedLine,
    'This Change has moved since it was approved — the approval names e71c5b0, and the record now stands at 9f3d21e.')
  assert.doesNotMatch(hq.decision.movedLine, /while this screen was open/)
  assert.ok(dom.includes(hq.decision.movedLine), 'the screen renders the model\'s own sentence')

  const still = M.decisionOf(changeAt(VERSION_A, { standing: APPROVAL, history: [APPROVAL], target_revision: VERSION_A, scope_version: 1 }), 'e71c5b0')
  assert.equal(still.movedLine, null, 'nothing ever says a Change moved that did not')
})

/* ── T-5-2 fix round — a version label is a name, so it is unique ─────── */

/* Verifier finding 1. Seven characters is safe for a git revision and unsafe
 * for a ULID: the leading ten characters are the millisecond it was minted.
 * These two are the real pair out of this task's own acceptance walk. */
const ULID_M = '01M37KZMP5NBY1G2297MT3GV6K'
const ULID_P = '01M37KZP1AZXMCTYZWTPWD5E57'

test('two versions that share ten characters are told apart everywhere the screen names them', () => {
  const forms = M.shortVersions([ULID_M, ULID_P])
  assert.notEqual(forms.get(ULID_M), forms.get(ULID_P), 'two trees are never one name')
  assert.equal(forms.get(ULID_M), '01M37KZM', 'the shortest prefix that tells them apart, and no longer')
  assert.equal(forms.get(ULID_P), '01M37KZP')

  /* Never shorter than seven, even when one character would do. */
  const easy = M.shortVersions(['e71c5b0aaaa', '9f3d21ebbbb'])
  assert.equal(easy.get('e71c5b0aaaa'), 'e71c5b0')

  /* A version that IS the prefix of another: it is shown whole, and the
   * longer one grows by exactly as much as it takes to differ. Widening
   * always terminates, because at the longest id the prefixes are the ids. */
  const nested = M.shortVersions(['abc1234', 'abc1234567'])
  assert.deepEqual([...nested.values()], ['abc1234', 'abc12345'])
  assert.equal(new Set(nested.values()).size, 2, 'and they are still two names')
})

test('a history of two approvals minted in the same millisecond renders two different names', () => {
  const first = { ...APPROVAL, id: 'cd_1', target_revision: ULID_M }
  const second = { ...APPROVAL, id: 'cd_2', target_revision: ULID_P, decided_at: '2026-09-23T17:11:03Z' }
  const doc = { standing: second, history: [first, second], target_revision: ULID_P, scope_version: 1 }
  const decision = M.decisionOf(changeAt(ULID_P, doc), '01M37KZ')
  assert.deepEqual(decision.history.map((h) => h.version), ['01M37KZM', '01M37KZP'])
  assert.notEqual(decision.history[0].line, decision.history[1].line, 'two acts, two readable lines')
  assert.ok(decision.history[0].line.includes('Approved on 01M37KZM by sam'))
  assert.ok(decision.history[1].line.includes('Approved on 01M37KZP by sam'))
  assert.equal(decision.movedSince, false, 'the standing approval IS the version the record stands at')
  assert.equal(decision.movedLine, null)

  const dom = packet(changeAt(ULID_P, doc))
  assert.ok(dom.includes('Approved on 01M37KZM by sam'), 'the packet and its Markdown name both')
  assert.ok(dom.includes('Approved on 01M37KZP by sam'))
})

test('the moved sentence never names the same short form for two different versions', () => {
  const standing = { ...APPROVAL, target_revision: ULID_M }
  const doc = { standing, history: [standing], target_revision: ULID_P, scope_version: 1 }
  /* The caller still passes the screen's own seven characters; the model
   * decides the names from the whole revisions, so the two halves differ. */
  const decision = M.decisionOf(changeAt(ULID_P, doc), '01M37KZ')
  assert.equal(decision.movedSince, true, 'compared on the whole revisions, not on the labels')
  assert.equal(decision.version, '01M37KZM')
  assert.equal(decision.nowVersion, '01M37KZP')
  /* MOVED-WHILE-OPEN-TRUTH (D-050): the same two versions, said without a claim about the reader's own session. */
  assert.equal(decision.movedLine,
    'This Change has moved since it was approved — the approval names 01M37KZM, and the record now stands at 01M37KZP.')
  const [named, now] = [decision.version, decision.nowVersion]
  assert.notEqual(named, now, 'a sentence that names one version twice names nothing')
  assert.equal(decision.headline, 'Approved on 01M37KZM — code changed since')
})

/* `Q13`, the control. After a refusal the same button would be refused for the
 * same reason, so the one primary becomes the door to what was checked. */
test('the refusal names the file that changed and offers See what changed, not a second Approve', () => {
  const moved = M.movedSentences({ outcome: 'moved', seen: VERSION_A, now: VERSION_B, changed: ['src/lib.rs'] })
  const dom = M.renderRefused({ moved, onRecheck: () => {} })
  assert.ok(dom.includes('Not approved.'), 'the refusal is shown, not swallowed')
  assert.ok(dom.includes('One file changed since this screen loaded:'))
  assert.ok(dom.includes('src/lib.rs'), 'the file the person changed is named')
  assert.ok(!/[ -]/.test(dom),
    'no control character reaches the list where a filename belongs')
  assert.ok(dom.includes('See what changed'), 'the one primary is the door to what moved')
  assert.ok(!dom.includes('Approve this version'), 'pressing Approve again is not offered')
  assert.equal(dom.match(/ws-change-primary/g).length, 1, 'one primary, and only one')
})

/* ── T-5-2-NO-REVIVAL (R-2026-09-28-10) ───────────────────────────────── */

/* Approve A, the code moves to B, approve B, withdraw B — served exactly as
 * `change_decision::read` serves it: nothing stands, and A's approval names
 * the later approval that replaced it. Every surface says "not approved", and
 * the history keeps all three acts with A's marked as replaced, so nothing
 * reads as "Approved on A — code changed since" after a withdrawal. */
test('withdrawing the latest approval never brings back an earlier one, on the screen, the Inbox or the packet', () => {
  const approveA = { ...APPROVAL, superseded_by: 'cd_2' }
  const approveB = { ...APPROVAL, id: 'cd_2', target_revision: VERSION_B, decided_at: '2026-09-23T11:00:00Z', superseded_by: null }
  const withdrawB = { ...WITHDRAWAL, id: 'cd_3', target_revision: VERSION_B, decided_at: '2026-09-23T12:00:00Z', withdraws: 'cd_2', superseded_by: null }
  const doc = { standing: null, history: [approveA, approveB, withdrawB], target_revision: VERSION_B, scope_version: 1 }
  const change = changeAt(VERSION_B, doc)

  const { hq, dom, row } = screen(change)
  assert.equal(hq.decision.approved, false, 'the last act was a withdrawal: nothing is approved')
  assert.equal(hq.decision.headline, null, 'no approval headline')
  assert.equal(hq.decision.movedLine, null, 'no "code changed since" for an approval that no longer stands')
  assert.ok(!dom.includes('code changed since'), 'the screen does not revive A as the approval')
  assert.ok(!dom.includes('>Approved on e71c5b0</p>'), 'A is not the headline')
  assert.ok(dom.includes('Approve this version'), 'approval is on offer again')
  assert.ok(!dom.includes('Withdraw approval'), 'there is nothing standing to withdraw')
  assert.ok(!row.includes('Approved on'), 'the Inbox row does not call it approved')

  assert.deepEqual(hq.decision.history.map((h) => [h.act, h.version]),
    [['approved', 'e71c5b0'], ['approved', '9f3d21e'], ['withdrawn', '9f3d21e']],
    'all three acts, each with its own version')
  assert.equal(hq.decision.history[0].line,
    'Approved on e71c5b0 by sam on 2026-09-22, against what counts as done, version 1. Replaced by the later approval on 9f3d21e.')
  assert.equal(hq.decision.history[1].line,
    'Approved on 9f3d21e by sam on 2026-09-23, against what counts as done, version 1.',
    'B was withdrawn, not replaced')
  assert.equal(hq.decision.history[2].line, 'The approval on 9f3d21e was withdrawn by sam on 2026-09-23.')
  assert.ok(dom.includes('Replaced by the later approval on 9f3d21e.'), 'the screen says A was replaced')

  const page = packet(change)
  assert.ok(page.includes('Replaced by the later approval on 9f3d21e.'), 'the packet says A was replaced')
  assert.ok(!page.includes('code changed since'), 'the packet does not revive A either')
})
