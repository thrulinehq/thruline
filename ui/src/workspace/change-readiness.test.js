/* T-3-5 acceptance item 0, in the DOM.
 *
 * The computation is asserted in `crates/core/tests/change_readiness.rs`. This
 * file asserts the thing a PERSON meets: three live records — no accepted
 * criteria, one interrupted mandatory run, everything covered — rendered
 * through the ONE derivation onto the Change screen, the Inbox row and the
 * packet page, all three agreeing because all three read `hq.readiness`.
 *
 * The three rules it fences:
 *
 *   - a Change with no accepted criteria NEVER reads ready;
 *   - a Change with one interrupted mandatory run reads not ready AND NAMES IT;
 *   - a fully covered Change reads "Ready for approval for version <v>" and
 *     LISTS WHAT IT RESTS ON, and the word "Ready" appears nowhere else.
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
import { CLASSES, FORMATS, renderExport } from './packet-export.js'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-ready-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeScreen, ChangeBrief } from './change-workspace.jsx';
  import { PacketPage } from './packet.jsx';
  import { InboxItem } from './inbox.jsx';
  import { deriveChangeModel, packetMarkdown } from './change-model.js';
  import { buildChangeView } from './change-view.js';
  import { buildCoverageView, evidenceEntries } from './coverage-view.js';
  import { buildPacketView } from './packet-view.js';
  import { inboxItem, reasonLine } from './inbox-view.js';
  export { deriveChangeModel, packetMarkdown, buildChangeView, buildCoverageView, evidenceEntries, buildPacketView, inboxItem, reasonLine };
  export const renderChange = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderBrief = props => renderToStaticMarkup(React.createElement(ChangeBrief, props));
  export const renderPacket = props => renderToStaticMarkup(React.createElement(PacketPage, props));
  export const renderRow = props => renderToStaticMarkup(React.createElement('ul', null, React.createElement(InboxItem, props)));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'ready.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

/* ── the record ───────────────────────────────────────────────────────── */

const CHANGE = {
  id: 'ch1',
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Totals add up on the checkout page' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1,
    criteria: ['The totals add up'],
    allowed_scope: ['checkout/'],
    check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet',
}

/** Nothing has been accepted as counting as done. */
const NO_CRITERIA = {
  scope_version: null,
  link_rows: 0,
  execution_sessions: [],
  criteria: [],
  candidates: [],
  readiness: {
    schema_version: 1,
    ready: false,
    scope_version: null,
    target_version: 'e71c5b0aaaa',
    headline_slug: 'readiness_nothing_accepted',
    obligations: [
      { id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: null, waiver: null, waivable: false },
    ],
    rests_on: [],
    missing: [],
    waived: [],
    not_applicable: [],
  },
}

/** One accepted line, and the run that covers it was interrupted.
 *
 * **The shape the engine really serves**, not a hand-flattering one. A run
 * that was stopped settled nothing, so it does not bear, so the criterion's
 * coverage state is `ran_nothing` and NOT `evidence`. This fixture said
 * `evidence` / `bears: true` until the T-3-5 verifier found that the live
 * screen said "Something ran, and it settled nothing about this" for exactly
 * this case; the fixture was flattering the product and the bug was invisible
 * here. `crates/core/tests/change_readiness_derivation.rs` derives this same
 * document from a real ledger, so the two cannot drift apart silently.
 */
const INTERRUPTED = {
  scope_version: 1,
  link_rows: 1,
  execution_sessions: ['s1'],
  criteria: [{
    index: 1,
    text: 'The totals add up',
    state: 'ran_nothing',
    items: [{
      link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run',
      name: 'cargo test', observed: 'couldnt', bears: false, applies: true, applies_slug: 'inputs_applies',
      fresh: true, fresh_slug: 'applicability_still_applies',
      execution: 'interrupted', execution_slug: 'execution_interrupted',
    }],
  }],
  candidates: [],
  readiness: {
    schema_version: 1,
    ready: false,
    scope_version: 1,
    target_version: 'e71c5b0aaaa',
    headline_slug: 'readiness_needs_evidence_one',
    obligations: [
      { id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: null, waiver: null, waivable: false },
      { id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: 'The totals add up', outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'stopped', unsatisfied_slug: 'readiness_stopped', evidence_name: null, waiver: null, waivable: true },
    ],
    rests_on: [],
    missing: ['criterion:1'],
    waived: [],
    not_applicable: [],
  },
}

/** Everything accepted is backed, on this version, by a run that finished. */
const COVERED = {
  scope_version: 1,
  link_rows: 1,
  execution_sessions: ['s1'],
  criteria: [{
    index: 1,
    text: 'The totals add up',
    state: 'evidence',
    items: [{
      link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run',
      name: 'cargo clippy', observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies',
      fresh: true, fresh_slug: 'applicability_still_applies',
      execution: 'completed', execution_slug: 'execution_completed',
    }],
  }],
  candidates: [],
  readiness: {
    schema_version: 1,
    ready: true,
    scope_version: 1,
    target_version: 'e71c5b0aaaa',
    headline_slug: 'readiness_ready',
    obligations: [
      { id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: null, waiver: null, waivable: false },
      { id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: 'The totals add up', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: 'cargo clippy', waiver: null, waivable: true },
    ],
    rests_on: [{ obligation_id: 'criterion:1', text: 'The totals add up', evidence_name: 'cargo clippy' }],
    missing: [],
    waived: [],
    not_applicable: [],
  },
}

/** The same record with the line waived instead of backed. */
const WAIVED = {
  ...INTERRUPTED,
  readiness: {
    ...INTERRUPTED.readiness,
    ready: true,
    headline_slug: 'readiness_ready',
    obligations: [
      INTERRUPTED.readiness.obligations[0],
      {
        ...INTERRUPTED.readiness.obligations[1],
        outcome: 'waived',
        outcome_slug: 'waived',
        waiver: {
          obligation_id: 'criterion:1', kind: 'waived', actor: 'sam',
          authority: 'the accepted check policy', rationale: 'covered by the design review',
          at: '2026-09-22T12:00:00Z',
        },
      },
    ],
    rests_on: [],
    missing: [],
    waived: ['criterion:1'],
  },
}

/** `renderToStaticMarkup` escapes; the assertions compare what a person reads. */
const plain = (html) => html
  .replace(/<[^>]+>/g, ' ')
  .replace(/&quot;/g, '"').replace(/&#x27;/g, "'").replace(/&#39;/g, "'")
  .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')

/** The three surfaces, off ONE derived model, exactly as the app builds them. */
function surfaces(coverage, stage = 'verify', source = CHANGE) {
  const hq = M.deriveChangeModel(source, coverage, null, null, [])
  const change = M.buildChangeView({ change: source, proposals: null, work: [], evidence: M.evidenceEntries(coverage) })
  change.coverage = M.buildCoverageView(coverage)
  change.examined = null
  change.hq = hq
  const packet = M.buildPacketView({ change: source, proposals: null, work: [], coverage, evidence: [], hq })
  packet.hq = hq
  const item = M.inboxItem({ change: source, proposals: null, work: [], evidence: [], applicability: [], hq })
  return {
    hq,
    item,
    change: plain(M.renderChange({ model: change, stage, onGo: () => {}, onCopy: () => {}, notes: [], onReload: () => {} })),
    /* T-3-5 fix round: BRIEF is the screen `#/changes/<id>` actually renders
     * since P6. The readiness block was wired into `ChangeScreen` alone, so
     * every assertion in this file passed while the screen a person opens said
     * nothing about readiness at all. It is a surface here now, and it cannot
     * be dropped again without this file going red. */
    brief: plain(M.renderBrief({ model: change, onCopy: () => {}, onReload: () => {}, notes: [], register: 'manager', onExplore: () => {} })),
    packet: plain(M.renderPacket({ model: packet, notes: [], onCopy: () => {} })),
    row: item ? plain(M.renderRow({ item })) : '',
    md: M.packetMarkdown(hq),
  }
}

const READY = 'Ready for approval for version e71c5b0. It rests on:'

const APPROVAL = {
  id: 'cd_1', act: 'approved', target_revision: 'e71c5b0aaaa', scope_version: 1,
  actor: 'sam', decided_at: '2026-09-22T11:00:00Z',
}

const decisions = (standing, history, target_revision = 'e71c5b0aaaa') => ({
  standing, history, target_revision, scope_version: 1,
})

test('an approved version keeps its evidence and approval, without offering readiness on any surface or export', () => {
  const source = { ...CHANGE, decisions: decisions(APPROVAL, [APPROVAL]) }
  const s = surfaces(COVERED, 'verify', source)
  assert.equal(s.hq.readiness.ready, true, 'the engine still says its conditions are met')
  assert.equal(s.hq.decision.approvedFull, APPROVAL.target_revision, 'the standing act names this full version')
  assert.equal(s.hq.decision.headline, 'Approved on e71c5b0')
  for (const [surface, text] of Object.entries({ Brief: s.brief, Inspector: s.change, 'whole-story page': s.packet, 'Copy as Markdown': s.md, Inbox: s.row })) {
    assert.ok(!text.includes('Ready for approval'), `${surface} still offers approval readiness for an approved version`)
  }
  assert.equal(s.hq.packet.readinessLine, null)
  assert.deepEqual(s.hq.packet.readinessRests, [], 'no orphan readiness list follows a hidden sentence')
  for (const format of FORMATS) {
    for (const redact of [[], CLASSES]) {
      const file = renderExport(s.hq.packet, null, { format, redact }).text
      if (format === 'json') {
        const json = JSON.parse(file)
        /* D-050 (READY-BESIDE-APPROVED repair, finding 4): `ready` was null here, which is also what a Change whose
         * readiness could not be read says. `ready` is the engine's answer, and what the file does not offer is
         * said in its own fields. */
        assert.equal(json.readiness.ready, true, 'JSON keeps the engine\'s answer')
        assert.equal(json.readiness.shown, false, 'JSON does not offer readiness for an approved version')
        assert.equal(json.readiness.hidden_because, 'approved')
        assert.equal(json.readiness.sentence, null)
        assert.equal(json.decision.standing, 'Approved on e71c5b0')
      } else {
        assert.ok(!file.includes('Ready for approval'), `${format} export still offers readiness`)
        assert.ok(file.includes('Approved on e71c5b0'), `${format} export lost the standing decision`)
      }
    }
  }
})

test('withdrawal permits readiness again only while the existing evidence conditions hold', () => {
  const withdrawal = { id: 'cd_2', act: 'withdrawn', target_revision: APPROVAL.target_revision, actor: 'sam', decided_at: '2026-09-22T12:00:00Z', withdraws: APPROVAL.id }
  const source = { ...CHANGE, decisions: decisions(null, [APPROVAL, withdrawal]) }
  const ready = surfaces(COVERED, 'verify', source)
  assert.equal(ready.hq.decision.approved, false)
  assert.equal(ready.hq.decision.history.length, 2, 'both acts remain in history')
  assert.ok(ready.brief.includes(READY))
  assert.ok(ready.packet.includes(READY))
  assert.ok(renderExport(ready.hq.packet, null, { format: 'md' }).text.includes(READY))
  assert.equal(JSON.parse(renderExport(ready.hq.packet, null, { format: 'json' }).text).readiness.ready, true)
  const interrupted = surfaces(INTERRUPTED, 'verify', source)
  assert.equal(interrupted.hq.readiness.ready, false)
  assert.ok(!interrupted.brief.includes(READY), 'withdrawal never fabricates missing evidence')
})

test('an approval on a different version does not hide the current version\'s ready answer', () => {
  const current = '9f3d21ebbbb'
  const source = {
    ...CHANGE,
    implementation: { ...CHANGE.implementation, snapshot_id: current },
    decisions: decisions(APPROVAL, [APPROVAL], current),
  }
  const coverage = { ...COVERED, readiness: { ...COVERED.readiness, target_version: current } }
  const s = surfaces(coverage, 'verify', source)
  assert.equal(s.hq.decision.movedSince, true)
  assert.equal(s.hq.decision.approvedFull, APPROVAL.target_revision, 'approval remains bound to its original version')
  assert.ok(s.brief.includes('Ready for approval for version 9f3d21e. It rests on:'))
  assert.ok(s.packet.includes('Ready for approval for version 9f3d21e. It rests on:'))
  assert.equal(JSON.parse(renderExport(s.hq.packet, null, { format: 'json' }).text).readiness.ready, true)
})

/* ── (0a) no accepted criteria ────────────────────────────────────────── */

test('(0a) a Change with nothing accepted never reads ready, and says what would fill it', () => {
  const s = surfaces(NO_CRITERIA)
  assert.equal(s.hq.readiness.ready, false)
  assert.equal(s.hq.readiness.slug, 'readiness_nothing_accepted')
  assert.ok(s.change.includes('Nothing counts as done yet, so nothing is ready.'))
  assert.ok(!s.change.includes('Ready for approval'), 'the screen never offers readiness')
  assert.ok(!s.packet.includes('Ready for approval'), 'nor does the packet')
  assert.ok(!s.md.includes('Ready for approval'), 'nor the copied text')
  assert.ok(s.hq.readiness.restsOn.length === 0, 'and it rests on nothing')
})

/* ── (0b) one interrupted mandatory run ───────────────────────────────── */

test('(0b) an interrupted run reads not ready, and the screen NAMES the line and why', () => {
  const s = surfaces(INTERRUPTED)
  assert.equal(s.hq.readiness.ready, false)
  assert.ok(s.change.includes('1 thing still needs evidence:'), 'the count, in Sam’s words')
  assert.ok(s.change.includes('The totals add up'), 'the line is named')
  assert.ok(
    s.change.includes('The check that covers this was stopped before it finished.'),
    'and why it does not count',
  )
  assert.ok(!s.change.includes('Ready for approval'))
  /* A stopped run never reads as a pass anywhere on the surface. */
  assert.ok(!s.change.includes('Proven: cargo test'))
})

test('(0b) the packet and the Inbox row say the same thing, from the same field', () => {
  const s = surfaces(INTERRUPTED)
  assert.ok(s.packet.includes('1 thing still needs evidence:'), 'the packet page')
  assert.ok(s.md.includes('1 thing still needs evidence:'), 'the copied text')
  assert.equal(s.hq.inboxRow.ready, false)
  assert.ok(!s.row.includes('Ready'), 'the Inbox row never says ready either')
})

/* ── (0c) everything covered ──────────────────────────────────────────── */

test('(0c) a fully covered Change reads ready for ONE named version, and lists what it rests on', () => {
  const s = surfaces(COVERED)
  assert.equal(s.hq.readiness.ready, true)
  assert.ok(s.change.includes(READY), 'the sentence names the version and promises the list')
  /* The list is THERE, on the screen, not implied. */
  assert.ok(s.change.includes('The totals add up — cargo clippy'), 'what it rests on is listed')
  assert.ok(s.packet.includes(READY) && s.packet.includes('The totals add up — cargo clippy'))
  assert.ok(s.md.includes(READY) && s.md.includes('- The totals add up — cargo clippy'))
})

test('(0c) "Ready" appears nowhere but the readiness sentence', () => {
  const s = surfaces(COVERED)
  const hits = s.change.split('Ready').length - 1
  assert.equal(hits, 1, 'exactly one "Ready" on the whole Change screen')
  /* And no colour and no percentage anywhere near it. */
  assert.ok(!/\bgreen\b/i.test(s.change), 'no colour word')
  assert.ok(!s.hq.readiness.sentence.includes('%'), 'no percentage in the sentence')
  /* Rule 3: the readiness sentence is not one of the five state words. */
  for (const word of ['Proven', 'Not proven', 'In progress', 'Recorded', 'Needs a re-check']) {
    assert.ok(!s.hq.readiness.sentence.includes(word), `the sentence carries the state word ${word}`)
  }
})

/* ── the Approve door, and the Inbox reason ───────────────────────────── */

test('the Approve door is off until the record is ready, and its reason names what is missing', () => {
  /* Rule 6, on the surface where approving is the act: the Approve stage. */
  const stopped = surfaces(INTERRUPTED, 'approve')
  assert.equal(stopped.hq.readiness.ready, false)
  assert.equal(
    stopped.hq.readiness.blocker.reason,
    /* D-050 (RUN-LINK-TRUTH-R4): the stopped check's own pair, not the generic one. */
    "This version can't be approved yet: the check that covers a line of what counts as done was stopped before it finished.",
  )
  assert.ok(stopped.change.includes(stopped.hq.readiness.blocker.reason), 'the reason is beside the control')
  assert.ok(stopped.change.includes('Open Check it again on this Change'), 'and the next act')
  assert.ok(stopped.change.includes('1 thing still needs evidence:'), 'and what is missing, named')

  /* And in the Inspector, on the stage that carries it. The live Approve
   * control is the Decision block — the one with the name box and the buttons
   * — and it is NOT rendered while the record is not ready; the disabled
   * control stands there instead with both halves of why, which is rule 6
   * working rather than a live door. */
  const stoppedVerify = surfaces(INTERRUPTED, 'verify')
  assert.equal(stoppedVerify.hq.inspector.action.enabled, false)
  assert.ok(
    !stoppedVerify.change.includes('Your name, for the record'),
    'the live Approve block was rendered on a record that is not ready',
  )
  assert.ok(stopped.change.includes('Approve this version'), 'the off control still names its act')

  const covered = surfaces(COVERED)
  assert.ok(covered.change.includes('Your name, for the record'), 'ready, so the live door is offered')
  assert.ok(covered.change.includes('Approve this version'), 'and it names one version')
  assert.ok(
    covered.change.includes('It does not merge, deploy or tell the agent anything.'),
    'and it still says what approving does not do',
  )
})

test('the Inbox reason `awaiting approval` fires from the readiness record', () => {
  const covered = surfaces(COVERED)
  assert.equal(covered.item.reason.id, 'awaiting_approval', 'ready → waiting for a person')
  const stopped = surfaces(INTERRUPTED)
  assert.notEqual(stopped.item && stopped.item.reason.id, 'awaiting_approval', 'not ready → not waiting')
})

/* ── a waiver is never a pass ─────────────────────────────────────────── */

test('a waived line is visible, names who and why, and never reads like a result', () => {
  const s = surfaces(WAIVED)
  assert.ok(
    s.change.includes('Waived by sam, under the accepted check policy: covered by the design review. Nobody checked this.'),
    'the waiver is on the screen, with its actor, its authority and its reason',
  )
  assert.ok(s.change.includes('The totals add up'), 'beside the line it is about')
  /* And it is NOT in what the answer rests on. */
  assert.equal(s.hq.readiness.restsOn.length, 0)
  assert.ok(!s.change.includes('The totals add up — cargo test'), 'a waiver is never listed as evidence')
  assert.ok(s.md.includes('Nobody checked this'), 'the copied text carries it too')
})

/* ── (0e) the screen a person actually opens ──────────────────────────── */

/* The whole of the T-3-5 fix round. `#/changes/<id>` renders `ChangeBrief`
 * (P6), not the stage view the readiness block was wired into — so the engine
 * answered "the check that covers this was stopped before it finished" and the
 * screen printed a coverage line instead, live, on a real ledger. Brief reads
 * the SAME `hq.readiness` as every other surface; this test is the fence that
 * says so. */
test('(0e) Brief — the screen #/changes/<id> renders — carries the same readiness answer', () => {
  const nothing = surfaces(NO_CRITERIA)
  assert.ok(nothing.brief.includes('Nothing counts as done yet, so nothing is ready.'))
  assert.ok(!nothing.brief.includes('Ready for approval'), 'an empty set is never ready on Brief either')

  const stopped = surfaces(INTERRUPTED)
  assert.ok(stopped.brief.includes('1 thing still needs evidence:'), 'the count')
  assert.ok(stopped.brief.includes('The totals add up'), 'the line is named')
  assert.ok(
    stopped.brief.includes('The check that covers this was stopped before it finished.'),
    'Brief names the stopped check — the sentence the live screen was missing',
  )
  assert.ok(!stopped.brief.includes('Ready for approval'))

  const ready = surfaces(COVERED)
  assert.ok(ready.brief.includes(READY), 'the strongest sentence, on the screen that shows it')
  assert.ok(ready.brief.includes('The totals add up'), 'and what it rests on, listed under it')
  for (const word of ['green', 'all clear', '%']) {
    assert.ok(!ready.brief.includes(word), `Brief says "${word}" about readiness`)
  }
})
