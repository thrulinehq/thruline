/* T-5-3 acceptance item 0 — exceptions, in the DOM.
 *
 * Sam's step 7 is "if I approve with something missing, does it say so on the
 * approval itself, forever?" This file asks that of the rendered markup rather
 * than of the model, in the order a person meets it:
 *
 *   1. one accepted line still needs evidence  → Approve is shut, with the
 *      reason and what would turn it on;
 *   2. that line is set aside, with a name, an authority and a reason → the
 *      headline becomes "Ready for approval with exceptions for version …",
 *      the set-aside line is listed UNDER its own heading and is absent from
 *      what the answer rests on, and it is not a result;
 *   3. approved → "Approved with exceptions on e71c5b0", never "Approved on
 *      e71c5b0", on the screen, in the history, in the Inbox and in the packet;
 *   4. the packet's Markdown carries who set it aside, under whose authority,
 *      and why.
 *
 * Every string asserted is one `change-model.js` produced out of `HQ_COPY`, so
 * a screen that grew its own wording fails rather than agrees with itself.
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
const temporary = await mkdtemp(join(tmpdir(), 'thruline-except-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeScreen } from './change-workspace.jsx';
  import { InboxItem } from './inbox.jsx';
  import { PacketPage } from './packet.jsx';
  import { buildPacketView } from './packet-view.js';
  import { deriveChangeModel, packetMarkdown } from './change-model.js';
  import { buildChangeView } from './change-view.js';
  import { buildCoverageView, evidenceEntries } from './coverage-view.js';
  import { inboxItem } from './inbox-view.js';
  export { deriveChangeModel, packetMarkdown, buildChangeView, buildCoverageView, evidenceEntries, inboxItem, buildPacketView };
  export const renderPacket = props => renderToStaticMarkup(React.createElement(PacketPage, props));
  export const renderChange = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderRow = props => renderToStaticMarkup(React.createElement('ul', null, React.createElement(InboxItem, props)));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'except.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const VERSION_A = 'e71c5b0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'

/** The authority a person had, as the ACCEPTED profile records it. */
const AUTHORITY = 'the release policy'
const WAIVED_LINE = 'Waived by sam, under the release policy: the payment sandbox is down all week. Nobody checked this.'

const WAIVER = {
  id: 'cx_1',
  obligation_id: 'criterion:2',
  kind: 'waived',
  actor: 'sam',
  authority: AUTHORITY,
  rationale: 'the payment sandbox is down all week',
  at: '2026-09-23T10:00:00Z',
}

const EXCEPTION_ROW = {
  waiver_id: 'cx_1',
  obligation_id: 'criterion:2',
  kind: 'waived',
  actor: 'sam',
  authority: AUTHORITY,
  rationale: 'the payment sandbox is down all week',
}

const APPROVAL_WITH_EXCEPTIONS = {
  id: 'cd_1', act: 'approved', target_revision: VERSION_A, scope_version: 1,
  actor: 'sam', decided_at: '2026-09-23T11:00:00Z', withdraws: null, record_path: null,
  exceptions: [EXCEPTION_ROW],
}

const changeAt = (decisions) => ({
  id: 'ch1',
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Totals add up on the checkout page' },
  implementation: { snapshot_id: VERSION_A, bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1,
    criteria: ['The totals add up', 'A declined card shows a message'],
    allowed_scope: ['checkout/'],
    check_profile: ['policy:exceptions-allowed-by:' + AUTHORITY],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet',
  decisions,
})

const NONE = { standing: null, history: [], target_revision: VERSION_A, scope_version: 1 }

const proven = {
  link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run',
  name: 'tests pass', observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies',
  fresh: true, fresh_slug: 'applicability_still_applies',
}

/** The coverage document, with the second line either unmet or set aside. */
const coverageWith = (waiver) => ({
  scope_version: 1,
  link_rows: 1,
  execution_sessions: ['s1'],
  criteria: [
    { index: 1, text: 'The totals add up', state: 'evidence', items: [proven] },
    { index: 2, text: 'A declined card shows a message', state: 'none', items: [] },
  ],
  /* The readiness block exactly as `change_readiness::compute` serves it. The
   * `with_exceptions` branch is the engine's answer, carried, not re-derived. */
  readiness: {
    schema_version: 2,
    ready: Boolean(waiver),
    with_exceptions: Boolean(waiver),
    scope_version: 1,
    target_version: VERSION_A,
    headline_slug: waiver ? 'readiness_ready_with_exceptions' : 'readiness_needs_evidence_one',
    obligations: [
      { id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: null, waiver: null, waivable: false },
      { id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: 'The totals add up', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: 'tests pass', waiver: null, waivable: true },
      waiver
        ? { id: 'criterion:2', kind: 'criterion', criterion_index: 2, text: 'A declined card shows a message', outcome: 'waived', outcome_slug: 'waived', unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked', evidence_name: null, waiver, waivable: true }
        : { id: 'criterion:2', kind: 'criterion', criterion_index: 2, text: 'A declined card shows a message', outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked', evidence_name: null, waiver: null, waivable: true },
    ],
    rests_on: waiver
      ? [{ obligation_id: 'criterion:1', text: 'The totals add up', evidence_name: 'tests pass' }]
      : [],
    missing: waiver ? [] : ['criterion:2'],
    waived: waiver ? ['criterion:2'] : [],
    not_applicable: [],
  },
})

function screen(change, coverage) {
  const hq = M.deriveChangeModel(change, coverage, null, null, [])
  const model = M.buildChangeView({ change, proposals: null, work: [], evidence: M.evidenceEntries(coverage) })
  model.coverage = M.buildCoverageView(coverage)
  model.examined = null
  model.hq = hq
  const row = M.inboxItem({ change, proposals: null, work: [], evidence: [], applicability: [], hq })
  return {
    hq,
    dom: M.renderChange({ model, stage: 'verify', onGo: () => {}, onCopy: () => {}, notes: [], onReload: () => {} }),
    brief: M.renderChange({ model, stage: 'brief', onGo: () => {}, onCopy: () => {}, notes: [], onReload: () => {} }),
    approve: M.renderChange({ model, stage: 'approve', onGo: () => {}, onCopy: () => {}, notes: [], onReload: () => {} }),
    row: row ? M.renderRow({ item: row }) : '',
  }
}

/* ── 1. an unmet line shuts the Approve door, with the reason ──────────── */

test('with one accepted line still needing evidence, Approve is shut and says why and what would turn it on', () => {
  const { hq, dom, approve } = screen(changeAt(NONE), coverageWith(null))
  assert.equal(hq.readiness.ready, false)
  assert.equal(hq.readiness.withExceptions, false)
  assert.ok(dom.includes('1 thing still needs evidence:'), 'the readiness sentence counts it')
  assert.ok(dom.includes('No test ran for this.'), 'in Sam’s words, as missing')
  /* The apostrophe is HTML-escaped in the markup, so the assertion is on the
   * half of the sentence that carries the fact. */
  /* D-050 (RUN-LINK-TRUTH-R4): the pair for this line's own reason, no test ran, not the generic one. */
  assert.ok(approve.includes('be approved yet: no test ran for a line of what counts as done.'), 'the door says why')
  assert.ok(approve.includes('Open Check it again on this Change'), 'and the next act')
})

/* ── 2. set aside: cleared, and never evidence ────────────────────────── */

test('a line set aside makes the answer "Ready for approval with exceptions", names who and why, and is absent from what it rests on', () => {
  const { hq, brief } = screen(changeAt(NONE), coverageWith(WAIVER))
  assert.equal(hq.readiness.ready, true)
  assert.equal(hq.readiness.withExceptions, true, 'the engine’s own field, carried')
  assert.equal(hq.readiness.slug, 'readiness_ready_with_exceptions')
  assert.equal(hq.readiness.sentence, 'Ready for approval with exceptions for version e71c5b0. It rests on:')
  assert.ok(brief.includes('Ready for approval with exceptions for version e71c5b0. It rests on:'))
  assert.ok(!brief.includes('Ready for approval for version e71c5b0'),
    'the ordinary ready sentence never appears over an answer with something set aside')

  /* What it rests on holds the CHECKED line and only that. */
  assert.deepEqual(hq.readiness.restsOn.map((r) => r.id), ['criterion:1'])
  assert.ok(!hq.readiness.restsOn.some((r) => r.id === 'criterion:2'),
    'a set-aside line is never something the answer rests on')

  /* And it is listed, under its own heading, saying nobody checked it. */
  assert.equal(hq.readiness.waivedLines.length, 1)
  assert.equal(hq.readiness.waivedLines[0].waiver, WAIVED_LINE)
  assert.equal(hq.readiness.waivedLines[0].isEvidence, false, 'never a result')
  assert.ok(brief.includes('Set aside, with nobody checking:'), 'its own heading')
  assert.ok(brief.includes(WAIVED_LINE), 'who, under what authority, and why')
})

test('nothing about a set-aside line reads as a pass, and no colour carries the meaning', () => {
  const { hq, brief } = screen(changeAt(NONE), coverageWith(WAIVER))
  const waived = hq.readiness.obligations.find((o) => o.id === 'criterion:2')
  assert.equal(waived.outcome, 'waived')
  assert.equal(waived.isEvidence, false)
  assert.ok(WAIVED_LINE.endsWith('Nobody checked this.'))
  for (const colour of ['green', 'Green', 'all clear', 'passed']) {
    assert.ok(!brief.includes(colour), `the screen must not say ${colour}`)
  }
})

/* ── 3. the approval says so, and keeps saying so ─────────────────────── */

test('an approval given with something set aside reads "Approved with exceptions on <v>" — never "Approved on <v>"', () => {
  const decisions = { standing: APPROVAL_WITH_EXCEPTIONS, history: [APPROVAL_WITH_EXCEPTIONS], target_revision: VERSION_A, scope_version: 1 }
  const { hq, dom, brief } = screen(changeAt(decisions), coverageWith(WAIVER))
  assert.equal(hq.decision.withExceptions, true)
  assert.equal(hq.decision.headline, 'Approved with exceptions on e71c5b0')
  assert.ok(dom.includes('Approved with exceptions on e71c5b0'))
  assert.ok(!dom.includes('>Approved on e71c5b0<'), 'the ordinary label is never used for this act')
  assert.ok(brief.includes('Approved with exceptions on e71c5b0'), 'Brief agrees')
  /* The Inbox reads the model's headline and derives none of its own. */
  assert.equal(hq.inboxRow.decision, hq.decision.headline)
})

test('the approval carries its exceptions beside it, with actor, authority and rationale, and the history says which act had them', () => {
  const decisions = { standing: APPROVAL_WITH_EXCEPTIONS, history: [APPROVAL_WITH_EXCEPTIONS], target_revision: VERSION_A, scope_version: 1 }
  const { hq, dom } = screen(changeAt(decisions), coverageWith(WAIVER))
  const line = 'Set aside: criterion:2 — the payment sandbox is down all week (by sam, under the release policy). Nobody checked this.'
  assert.deepEqual(hq.decision.exceptions.map((e) => e.line), [line])
  assert.ok(dom.includes('What this approval was given under'))
  assert.ok(dom.includes(line))
  assert.equal(hq.decision.history[0].line,
    'Approved with exceptions on e71c5b0 by sam on 2026-09-23, against what counts as done, version 1.')
})

/* ── 4. the packet a person takes away ────────────────────────────────── */

function packet(change, coverage) {
  const hq = M.deriveChangeModel(change, coverage, null, null, [])
  const model = M.buildPacketView({ change, proposals: null, work: [], coverage, evidence: [], hq })
  model.hq = hq
  return { hq, dom: M.renderPacket({ model, onCopy: () => {}, onBack: () => {} }) }
}

test('the packet and its Markdown both carry the label and every exception, with who, whose authority and why', () => {
  const decisions = { standing: APPROVAL_WITH_EXCEPTIONS, history: [APPROVAL_WITH_EXCEPTIONS], target_revision: VERSION_A, scope_version: 1 }
  const { hq, dom } = packet(changeAt(decisions), coverageWith(WAIVER))
  const line = 'Set aside: criterion:2 — the payment sandbox is down all week (by sam, under the release policy). Nobody checked this.'
  assert.ok(dom.includes('Approved with exceptions on e71c5b0'), 'the packet states the label')
  assert.ok(dom.includes(line), 'and what the approval was given under')

  const md = M.packetMarkdown(hq)
  assert.ok(md.includes('Approved with exceptions on e71c5b0'))
  assert.ok(md.includes('What this approval was given under'))
  assert.ok(md.includes('- ' + line))
  assert.ok(!md.includes('Ready for approval with exceptions for version e71c5b0'),
    'the approved version cannot also be offered as ready for approval')
  assert.ok(!md.includes('Proven — A declined card shows a message'),
    'the set-aside line never renders as a result in the takeaway')
})

/* READY-BESIDE-APPROVED repair (verifier finding 2). `packetMarkdown` above is
 * the legacy text, and nobody receives it: Copy as Markdown and the three files
 * of the export sheet are `renderExport`. Those used to name the exception only
 * through the readiness line, and once that line was (rightly) hidden beside
 * the approval they named nothing — "Approved with exceptions on e71c5b0" over
 * no word of what was set aside. Every format a person can take away, full and
 * redacted, names each exception the standing approval carries — the line, the
 * person, the authority, the reason — under the approval (T-5-3, `B10.9`). */
test('every exported file, full and redacted, names each exception under the approval it was given with: line, person, authority, reason', () => {
  const decisions = { standing: APPROVAL_WITH_EXCEPTIONS, history: [APPROVAL_WITH_EXCEPTIONS], target_revision: VERSION_A, scope_version: 1 }
  const { hq } = packet(changeAt(decisions), coverageWith(WAIVER))
  const line = 'Set aside: criterion:2 — the payment sandbox is down all week (by sam, under the release policy). Nobody checked this.'
  const heading = 'What this approval was given under'
  const unescape = (t) => t.replace(/\\(.)/g, '$1').replace(/&#39;|&#x27;/g, "'").replace(/&amp;/g, '&')
  for (const format of FORMATS) {
    for (const redact of [[], CLASSES]) {
      const at = format + (redact.length ? ' (redacted)' : '')
      const text = renderExport(hq.packet, null, { format, redact }).text
      if (format === 'json') {
        const json = JSON.parse(text)
        assert.equal(json.decision.standing, 'Approved with exceptions on e71c5b0', at)
        assert.equal(json.decision.exceptions_heading, heading, at)
        assert.deepEqual(json.decision.exceptions, [{
          line, obligation_id: 'criterion:2', kind: 'waived', actor: 'sam', authority: AUTHORITY, rationale: 'the payment sandbox is down all week',
        }], at + ': each exception, as typed fields and as the line')
        continue
      }
      const said = unescape(text)
      const approved = said.indexOf('Approved with exceptions on e71c5b0')
      const under = said.indexOf(heading, approved)
      const named = said.indexOf(line, under)
      assert.ok(approved >= 0, at + ': the label')
      assert.ok(under > approved, at + ': the heading, under the approval')
      assert.ok(named > under, at + ': the exception, under its heading, with who, whose authority and why')
      assert.ok(!said.includes('Ready for approval'), at + ': the approved version is not offered as ready')
    }
  }
})
