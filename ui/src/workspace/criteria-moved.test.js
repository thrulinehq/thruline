import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* T7 (report F5, defect T7) — the decision rests on the accepted criteria of
 * the current version.
 *
 * Done when (F5, verbatim): "Dropping a criterion changes Decision, take-away
 * and headline identically". Predeclared test form (card T7):
 *
 *  1. Not yet approved: list 1 accepts four lines, list 2 accepts three of
 *     them. The Decision's rests-on list, the take-away's `readinessRests` and
 *     the headline all name exactly the three lines of list 2, all read from
 *     one served field (`coverage.readiness.rests_on`). Already true on the
 *     base; pinned here.
 *  2. Approved on list 1, then list 2 accepted, code unchanged: the Decision,
 *     the take-away's decision block and the Brief headline each state, in the
 *     same sentence from the copy file, that the approval was given against
 *     list 1 and the accepted list is now list 2; none presents the approval
 *     as resting on list 2; the dropped line is named where the record can
 *     name it, and otherwise the sentence says the record does not list it.
 *  3. (no-ellipsis.test.js) No version, scope or state word is ellipsized.
 *
 * Every fixture here is synthetic. No string below came off a real machine.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-moved-list-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeShell } from './change-workspace.jsx';
  import { PacketPage } from './packet.jsx';
  import { buildPacketView } from './packet-view.js';
  import { buildChangeView } from './change-view.js';
  import { buildCoverageView, evidenceEntries, unlinkedCompletedRun } from './coverage-view.js';
  import { deriveChangeModel } from './change-model.js';
  import { renderExport } from './packet-export.js';
  import { HQ_COPY } from './hq-copy.js';
  import { fill } from './hq-narrate.js';
  export { buildPacketView, buildChangeView, buildCoverageView, evidenceEntries, unlinkedCompletedRun, deriveChangeModel, renderExport, HQ_COPY, fill };
  export const renderRoute = props => renderToStaticMarkup(React.createElement(ChangeShell, props));
  export const renderPacket = props => renderToStaticMarkup(React.createElement(PacketPage, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'moved-list.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const { buildPacketView, buildChangeView, buildCoverageView, evidenceEntries, unlinkedCompletedRun, deriveChangeModel, renderExport, HQ_COPY, fill, renderRoute, renderPacket } = M
const D = HQ_COPY.change.model.decision

const REVISION = 'e71c5b04d2a9f3c1b8e6d7a5c4b3a2918f7e6d5c'
const LIST_1 = [
  'Applying a discount code once reduces the total by exactly that discount.',
  'Applying the same discount code twice does not reduce the total twice.',
  'Checkout totals with no discount are unchanged.',
  'The receipt email shows the discounted total.',
]
const DROPPED = LIST_1[3]
const LIST_2 = LIST_1.slice(0, 3)

const APPROVAL = (scope) => ({
  id: 'dec_synthetic_0701', act: 'approved', target_revision: REVISION, actor: 'sam',
  decided_at: '2026-01-02T11:00:00Z', scope_version: scope, withdraws: null, record_path: null, exceptions: [],
})

/** The Change as `GET /api/changes/<id>` serves it, standing at accepted list `now`. */
const change = ({ now, approvedOn }) => ({
  id: 'cccccccc-0000-4000-8000-00000000c071',
  project_id: 'cccccccc-0000-4000-8000-00000000p071',
  opened_at: '2026-01-02T09:00:00Z',
  request: { title: 'Discount codes apply once', purpose: 'Discount codes apply once', requester: 'sam', source: 'person' },
  accepted_scope: {
    version: now, criteria: now === 1 ? LIST_1 : LIST_2, exclusions: [], allowed_scope: ['checkout/**'],
    constraints: [], check_profile: ['pytest'],
    content_sha256: '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
    acceptance: { actor: 'sam', accepted_at: '2026-01-02T10:30:00Z', rationale: 'the email is a separate change' },
    proposed_at: '2026-01-02T10:29:00Z',
  },
  implementation: {
    version: 1, bound_at: '2026-01-02T10:00:00Z', repository_identity: 'repo-synthetic-0071',
    base_revision: REVISION, snapshot_id: null, workspace_identity: 'workspace-synthetic-0071',
  },
  associations: [], examination: 'none_yet', decision: 'none_yet',
  decisions: approvedOn
    ? { standing: APPROVAL(approvedOn), history: [APPROVAL(approvedOn)], target_revision: REVISION, scope_version: now }
    : { standing: null, history: [], target_revision: REVISION, scope_version: now },
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
})

const item = (n) => ({
  link_id: 'cce_synthetic_07' + n, kind: 'check_run', evidence_id: 'evb_synthetic_0701',
  session_id: 'cli:check:2026-01-02T10:05:00Z', basis: 'declared', basis_slug: 'basis_declared',
  name: 'bash:synthetic-repo:pytest -q tests/test_totals.py', observed: 'proven', bears: true, applies: true,
  applies_slug: 'inputs_applies', asserted_at: '2026-01-02T10:05:00Z', execution: 'completed', execution_slug: 'execution_completed',
  failure_line: null, evidence_class: null, evidence_class_slug: null,
})

/** The coverage document for list `now`: every line proven, and ready (the engine's readiness). */
const coverage = (now) => {
  const lines = now === 1 ? LIST_1 : LIST_2
  return {
    schema_version: 1, change_id: 'cccccccc-0000-4000-8000-00000000c071', scope_version: now, link_rows: lines.length,
    executions: [], run_checks: null,
    criteria: lines.map((text, i) => ({ index: i + 1, text, state: 'evidence', state_slug: 'coverage_evidence', items: [item(i + 1)], removed: [] })),
    readiness: {
      ready: true, target_version: REVISION, headline_slug: 'readiness_ready', with_exceptions: false,
      obligations: lines.map((text, i) => ({ id: 'obl_' + (i + 1), criterion_index: i + 1, text, outcome: 'satisfied', evidence_name: 'pytest -q tests/test_totals.py', waivable: true })),
      rests_on: lines.map((text, i) => ({ obligation_id: 'obl_' + (i + 1), text, evidence_name: 'pytest -q tests/test_totals.py' })),
    },
  }
}

const surfaces = ({ now, approvedOn }) => {
  const ch = change({ now, approvedOn })
  const cov = coverage(now)
  const hq = deriveChangeModel(ch, cov, null, null, [])
  const model = buildChangeView({ change: ch, proposals: null, work: [], evidence: evidenceEntries(cov), unlinkedRun: unlinkedCompletedRun(cov) })
  model.coverage = buildCoverageView(cov)
  model.hq = hq
  const plain = (html) => html.replace(/<[^>]+>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ')
  const brief = renderRoute({ model, stage: 'verify', altitude: 'brief', register: 'manager', onGo: () => {}, onCopy: () => {}, onReload: () => {}, onAltitude: () => {}, onRegister: () => {} })
  /* The Brief's story column (headline) and its Inspector (the Decision), apart. */
  const story = /<section class="ws-brief-story">([\s\S]*?)<\/section><div class="ws-brief-inspector"/.exec(brief)
  /* The Decision's own div, and nothing beside it. Since the A3 integration
   * (integrate 6) it carries w3-approval's data-no-key-primary (A3-F8a) and is
   * followed, inside the Inspector's footer, by A3-F4's one numbers line —
   * and, since STEP-7-LIMITS, by the record's limit line before it. */
  const decisionBlock = /<div class="ws-decision"[^>]*>([\s\S]*?)<\/div>(?:<p class="ws-change-muted ws-change-code-limit">[^<]*<\/p>)*(?:<p class="ws-change-muted ws-inspector-numbers">[^<]*<\/p>)?<\/section>/.exec(brief)
  const packet = renderPacket({ model: { ...buildPacketView({ change: ch, proposals: null, work: [], coverage: cov, evidence: [], hq }), hq }, notes: [] })
  const decisionsSection = /<section class="ws-packet-section" id="ws-packet-decisions">([\s\S]*?)<\/section>/.exec(packet)
  return {
    hq,
    headline: story ? plain(story[1]) : '',
    decision: decisionBlock ? plain(decisionBlock[1]) : '',
    takeAway: decisionsSection ? plain(decisionsSection[1]) : '',
    file: renderExport(hq.packet, null, { format: 'md' }).text,
    brief: plain(brief),
  }
}

/* ── 1. not yet approved: one field, three surfaces ────────────────────── */

test('T7 done-when 1: before approval, dropping a line changes the Decision\'s rests-on list, the take-away and the headline identically', () => {
  const s = surfaces({ now: 2, approvedOn: null })
  const rests = s.hq.readiness.restsOn.map((r) => r.text)
  assert.deepEqual(rests, LIST_2, 'the Decision rests on exactly list 2, from coverage.readiness.rests_on')
  assert.equal(s.hq.packet.readinessRests.length, 3)
  for (const [i, line] of s.hq.packet.readinessRests.entries()) assert.ok(line.startsWith(LIST_2[i]), 'the take-away names the same three')
  assert.equal(s.hq.counts.total, 3, 'the headline counts the three')
  for (const text of [s.brief, s.file]) {
    for (const line of LIST_2) assert.ok(text.includes(line), `missing: ${line}`)
    assert.ok(!text.includes(DROPPED), 'the dropped line is still presented as counting')
  }
  /* Nothing says a list moved when no approval stands to have moved under. */
  assert.equal(s.hq.decision.listMovedLine, null)
})

/* ── 2. approved on list 1, list 2 accepted, code unchanged ────────────── */

test('T7 done-when 2: approved on list 1, then list 2 accepted — Decision, take-away and Brief headline each say so, in the same sentence', () => {
  const s = surfaces({ now: 2, approvedOn: 1 })
  const sentence = fill(D.list_moved, { then: 1, now: 2 })
  assert.equal(s.hq.decision.listMovedLine, sentence, 'one sentence, from the copy file, filled from the two served list numbers')
  assert.ok(s.decision.includes(sentence), 'the Decision says it')
  assert.ok(s.headline.includes(sentence), 'the Brief headline says it')
  assert.ok(s.takeAway.includes(sentence), 'the take-away\'s decision block says it')
  assert.ok(s.file.includes(sentence), 'the exported file says it')
  /* It names list 1 as what the approval was given against, and list 2 as
   * what is accepted now; the dropped line is not invented. */
  assert.match(sentence, /list 1/)
  assert.match(sentence, /list 2/)
  assert.match(sentence, /The record does not show here which lines list 2 left out\./)
  assert.ok(!s.decision.includes(DROPPED) && !s.headline.includes(DROPPED), 'a dropped line named from nowhere')
})

test('T7 done-when 2: nothing presents the standing approval as resting on list 2', () => {
  const s = surfaces({ now: 2, approvedOn: 1 })
  for (const [name, text] of [['Decision', s.decision], ['take-away', s.takeAway], ['file', s.file]]) {
    assert.doesNotMatch(text, /Approved[^.]*against what counts as done, version 2/, `${name} says the approval was given against list 2`)
  }
  assert.match(s.hq.decision.by, /version 1\.$/, 'the approval names the list it was given against')
})

test('T7: approved on the list that is still accepted — no surface says the list moved', () => {
  const s = surfaces({ now: 2, approvedOn: 2 })
  assert.equal(s.hq.decision.listMovedLine, null)
  const opener = D.list_moved.split('{')[0]
  for (const text of [s.decision, s.headline, s.takeAway, s.file]) assert.ok(!text.includes(opener), 'a list said to have moved that did not')
})
