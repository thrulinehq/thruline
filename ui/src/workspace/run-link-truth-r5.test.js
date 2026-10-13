/* RUN-LINK-TRUTH-R5 — every surface that says why approval is off leads with
 * the one blocker, including Ask, the page and the exports; a census makes a
 * sixth surface impossible to miss.
 *
 * Round 4's adversary asked the Change screen's own Ask bar "Why can't I
 * approve?" on the round-4 Change (`candidate_identity` unconfirmed, first in
 * the engine's order; line 1's own reason `readiness_unlinked_run`). Brief,
 * the Approve stage, the Inspector and "What we checked" all said the one
 * blocker (`hq.readiness.blocker`). Ask answered with its own derivation —
 * "2 things still need evidence:" and every missing line's per-line reason,
 * "link it" among them — and never named the act that clears the leading
 * blocker.
 *
 * Now `readinessOf` serves `readiness.blockers` (one pair per missing
 * obligation, in served order, each keyed by its own slug — the same
 * `blockerOf` pair) and `readiness.offLines` (the blocker first, then each
 * other missing obligation with its own pair). Every surface that says why
 * approval is off reads those, and nothing else.
 *
 * THE CENSUS. Every non-test file under ui/src that reads readiness.missing,
 * readiness.obligations, a readiness reason slug, the pair copy, or decision
 * availability (readiness.ready, showsDecision, canApprove, the Approve
 * disabled pair, `awaiting_approval`) to put words on a surface, found by the
 * grep in the last test of this file and classified here. The last test
 * fails when that grep finds a file this list does not name, or a file named
 * here no longer matches — so a sixth surface cannot arrive unannounced.
 *
 *  STATES WHY APPROVAL IS OFF (reads `readiness.blocker` / `offLines` /
 *  `blockers`, all built by `blockerOf`; asserted by the table below):
 *   - workspace/ask-hq.js         Ask's "Why can't I approve?" — offLines, the blocker first.
 *   - workspace/change-workspace.jsx  Brief's off Approve, the Approve stage's off Approve,
 *                                  the Inspector's off primary (round 4's table), and the
 *                                  Readiness list (the record's own list: each line keeps its
 *                                  own reason, card item 4; its first line is the blocker's
 *                                  own obligation — asserted below).
 *   - workspace/criterion-links.jsx   "What we checked"'s off primary — `hq.inspector.action`,
 *                                  whose pair is `readiness.blocker` (round 4's table).
 *   - workspace/change-model.js   where `blockerOf`, `blockers` and `offLines` are built; the
 *                                  packet's `readinessBlocker` / `readinessBlockerPair`, the
 *                                  Inspector's action pair (`actionBlocker`), and the retired
 *                                  `packetMarkdown`.
 *   - workspace/packet.jsx        the whole-story page's readiness block, and Copy as Markdown
 *                                  (`copiedMarkdown`, which is the export's Markdown).
 *   - workspace/packet-export.js  the Markdown, HTML and JSON files.
 *   - hq/screens/Approve-Sheet/from-change.js  HQ's Approve sheet: carries `readiness.blocker`
 *                                  (the `blockerOf` pair) to the sheet as `readinessBlocker`.
 *   - hq/screens/Approve-Sheet/logic.js  HQ's Approve sheet's off Approve: the
 *                                  `readinessBlocker` pair, reason then act, word for word.
 *   - hq/screens/PR-Face/from-change.js  HQ's pull-request note (C30 R-PR): carries
 *                                  `model.readiness.blocker`'s reason into the note's facts.
 *   - hq/screens/PR-Face/markdown.js  the note's second line: that reason, word for word.
 *
 *  READS THOSE FIELDS BUT SAYS NOTHING ABOUT WHY APPROVAL IS OFF (each file
 *  carries a `NOT-APPROVAL-WORDS:` comment saying why):
 *   - workspace/attention.js      the Inbox card's reasons: `awaiting approval` fires only when
 *                                  ready; nothing here explains a shut door.
 *   - workspace/inbox-view.js     renders those reasons' copy; the reason is `headline.text`,
 *                                  the top sentence about evidence, not approval.
 *   - workspace/change-view.js    the top sentence (evidence counts) and `approveBlocker`,
 *                                  which reaches a surface only when no readiness was served.
 *   - workspace/coverage-view.js  which lines the engine names as having an unlinked run.
 *   - workspace/run-route.js      the run sheet's unlinked lines.
 *   - workspace/hq-copy.js        the copy file: the words, not a surface.
 *   - workspace/attention-count.fixture.js  a test fixture.
 *   - workspace/integrations.jsx  an integration recipe's own `ready`, not the Change's.
 *   - hq/data/change.js           HQ's one Change model: `readiness.ready` picks the state word,
 *                                  the stage marks and the primary act; no why-not words.
 *   - hq/data/inbox-ready.js      the Inbox's approve row: reads `readiness_ready` and the lines a
 *                                  ready answer rests on to file a Change that is ready; a Change
 *                                  that is not ready gets no row and no words about why.
 *   - hq/data/project.js          the one "your move" count (C46 §1.1): files and counts a Change
 *                                  that IS ready (`filesAsApprove`, `readingOf`, `yourMove`).
 *   - hq/data/notification-events.js  a browser notification when a Change IS ready;
 *                                  never words about why approval is off.
 *   - hq/screens/Changes/from-projects.js  hands each project's reading to the Inbox's filing.
 *   - hq/screens/Command-Ask/from-change.js  a thread a ready Change files is "your move" in Find.
 *   - hq/screens/Command-Ask/index.jsx  passes which Changes are ready to from-change.js.
 *   - hq/screens/PR-Face/pr-fixture.js  a test fixture.
 *
 * Every string below is synthetic.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-run-link-r5-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeBrief, ChangeScreen, Inspector, showsDecision } from './change-workspace.jsx';
  import { CriterionSummary } from './criterion-links.jsx';
  import { deriveChangeModel, packetMarkdown } from './change-model.js';
  import { buildChangeView } from './change-view.js';
  import { buildCoverageView, evidenceEntries, unlinkedCompletedRun } from './coverage-view.js';
  import { buildPacketView } from './packet-view.js';
  import { PacketPage, copiedMarkdown } from './packet.jsx';
  import { renderExport } from './packet-export.js';
  import { answer } from './ask-hq.js';
  import { HQ_COPY } from './hq-copy.js';
  export { deriveChangeModel, packetMarkdown, showsDecision, buildChangeView, buildCoverageView, evidenceEntries, unlinkedCompletedRun, buildPacketView, copiedMarkdown, renderExport, answer, HQ_COPY };
  export const renderBrief = props => renderToStaticMarkup(React.createElement(ChangeBrief, props));
  export const renderScreen = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderInspector = props => renderToStaticMarkup(React.createElement(Inspector, props));
  export const renderSummary = props => renderToStaticMarkup(React.createElement(CriterionSummary, props));
  export const renderPacket = props => renderToStaticMarkup(React.createElement(PacketPage, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'run-link-r5.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const C = M.HQ_COPY.change
const R = C.model.readiness
const LINE_1 = 'When the cart is empty, the total shows zero.'
const LINE_2 = 'The total never goes below zero.'
const RUN_NAME = 'cargo test'

/* ── the reasons the engine can serve, read from its source at this sha ── */

const RUST = await readFile(join(here, '../../../crates/core/src/change_readiness.rs'), 'utf8')
const SLUG_FN = RUST.slice(RUST.indexOf('impl Unsatisfied'), RUST.indexOf('impl Unsatisfied') + 1500)
const ENGINE_SLUGS = [...SLUG_FN.matchAll(/=> "(readiness_[a-z_]+)"/g)].map((m) => m[1])
const NON_LINE = { candidate_identity: 'readiness_inputs_missing', review: 'readiness_review_missing' }
const LINE_SLUGS = ENGINE_SLUGS.filter((s) => !Object.values(NON_LINE).includes(s))

/* ── served documents, in the engine's shape (round 4's fixtures) ──────── */

const change = () => ({
  id: 'ch1', project_id: 'p1', opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Empty cart total' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1, criteria: [LINE_1, LINE_2], allowed_scope: ['src/'], check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet', decisions: [],
})

const RUN_CANDIDATE = {
  evidence_id: 'eb_run1', kind: 'check_run', execution: 'completed', execution_slug: 'execution_completed',
  bears: true, observed: 'proven', applies: true, name: RUN_NAME, session_id: 's1',
}

const PROVEN_ITEM = {
  link_id: 'l9', evidence_id: 'e9', session_id: 's1', basis: 'declared', kind: 'check_run', name: 'tests pass',
  observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies', fresh: true, fresh_slug: 'applicability_still_applies',
}

const obligation = (id, kind, n, text, slug) => ({
  id, kind, criterion_index: n, text,
  outcome: slug ? 'needs_evidence' : 'satisfied', outcome_slug: slug ? 'needs_evidence' : 'satisfied',
  unsatisfied: slug ? slug.replace(/^readiness_/, '') : null, unsatisfied_slug: slug || null,
  evidence_name: slug ? null : 'tests pass', linked_by: null, waiver: null, waivable: kind !== 'candidate_identity',
})

function coverage(missing) {
  const obligations = [
    obligation('candidate_identity', 'candidate_identity', null, '', missing.identity || null),
    obligation('criterion:1', 'criterion', 1, LINE_1, missing.c1 || null),
    obligation('criterion:2', 'criterion', 2, LINE_2, missing.c2 || null),
  ]
  if (missing.review !== undefined) obligations.push(obligation('review', 'review', null, '', missing.review || null))
  const row = (n, text, slug) => slug
    ? { index: n, text, state: 'none', items: [], removed: [] }
    : { index: n, text, state: 'evidence', items: [{ ...PROVEN_ITEM, link_id: 'l' + n, evidence_id: 'e' + n }], removed: [] }
  const ids = obligations.filter((o) => o.outcome === 'needs_evidence').map((o) => o.id)
  return {
    scope_version: 1, link_rows: 0, execution_sessions: ['s1'],
    criteria: [row(1, LINE_1, missing.c1), row(2, LINE_2, missing.c2)],
    candidates: [RUN_CANDIDATE], executions: [],
    readiness: {
      schema_version: 2, change_id: 'ch1', ready: false, with_exceptions: false, scope_version: 1,
      target_version: 'e71c5b0aaaa', headline_slug: ids.length === 1 ? 'readiness_needs_evidence_one' : 'readiness_needs_evidence',
      obligations, rests_on: [], missing: ids, waived: [], not_applicable: [],
    },
  }
}

function model(cov) {
  const ch = change()
  const view = M.buildChangeView({ change: ch, proposals: null, work: [], evidence: M.evidenceEntries(cov), unlinkedRun: M.unlinkedCompletedRun(cov) })
  view.hq = M.deriveChangeModel(ch, cov, null, null, [])
  return { view, cov, ch }
}

/* ── what this test expects, derived from the served input only ────────── */

/** NEVER-ASSESSED-ON-SCREEN (D-050): where the screen offers Check it again for this Change (this model has an id), a line never
 * assessed says the on-screen act in place of the terminal command. `R.blocked_by.readiness_never_assessed` is the terminal's
 * pair and is unchanged. */
const NEVER_ASSESSED_HERE = {
  reason: "This version can't be approved yet: whether a check still applies to a line of what counts as done was never assessed.",
  enabler: 'Open Check it again on this Change, and it will be assessed; the control is in Brief or Inspector. The run sheet shows which checks can run here.',
}
/** The pair for one served slug, by the copy file, independent of the code under test. */
const pairFor = (slug) => slug === 'readiness_unlinked_run'
  ? { reason: R.blocked_unlinked.reason.replace('{name}', RUN_NAME), enabler: R.blocked_unlinked.enabler }
  : slug === 'readiness_never_assessed' ? NEVER_ASSESSED_HERE : R.blocked_by[slug]
const said = (p) => p.reason + ' ' + p.enabler

/* EXPORT-MD-ESCAPE-R3 (D-050). The Markdown file writes a code span in the file's own
 * words as escaped TEXT — `thruline change prepare` is \`thruline change prepare\` in it —
 * because no recorded or copied text is ever emitted as a live code span (a renderer
 * that reads spans differently was made to run a script by one; the file's words and a
 * recorded person's words go through the one helper). The words a reader sees are the same,
 * backticks included. The two surfaces that are that file's text are 'markdown file' and
 * 'copy as markdown' (it copies the export's Markdown); the retired packetMarkdown is not. These sentences hold nothing between their backticks that a
 * renderer reads as markup, so escaping the backticks is all it takes. The old expected
 * value here was the sentence as the copy file has it. */
const mdCode = (s) => s.replaceAll('`', '\\`')

/** The missing obligations in served order, as [text, slug]. */
const servedMissing = (c) => [
  ['', c.identity], [LINE_1, c.c1], [LINE_2, c.c2], ['', c.review],
].filter(([, s]) => s)

/* ── reading each surface ──────────────────────────────────────────────── */

const unesc = (s) => s.replaceAll('&#x27;', "'").replaceAll('&#39;', "'").replaceAll('&quot;', '"').replaceAll('&#x2F;', '/').replaceAll('&amp;', '&').replaceAll('&lt;', '<').replaceAll('&gt;', '>')
const plain = (html) => unesc(html.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ').trim()

/** Every sentence of the copy file that can explain a shut Approve door, or a line's reason. */
const APPROVAL_WORDS = [
  ...Object.values(R.blocked_by).map((p) => p.reason),
  R.blocked_unlinked.reason.replace('{name}', RUN_NAME), R.blocked_unlinked.reason_unnamed,
  R.blocked_reason,
  ...Object.values(R.reasons),
]

/** The first sentence on a text surface that explains why approval is off. */
function leadingApprovalWords(text) {
  let at = -1
  let hit = null
  for (const w of APPROVAL_WORDS) {
    const i = text.indexOf(w)
    if (i >= 0 && (at < 0 || i < at || (i === at && w.length > hit.length))) { at = i; hit = w }
  }
  return { at, hit }
}

function surfaces(m) {
  const hq = m.view.hq
  const page = M.renderPacket({ model: { ...M.buildPacketView({ change: m.ch, proposals: null, work: [], coverage: m.cov, evidence: [], hq }), hq }, notes: [] })
  const json = M.renderExport(hq.packet, null, { format: 'json' }).text
  return {
    ask: M.answer('why_not_approve', hq),
    text: {
      'whole-story page': plain(page),
      'markdown file': M.renderExport(hq.packet, null, { format: 'md' }).text,
      'html file': plain(M.renderExport(hq.packet, null, { format: 'html' }).text),
      'copy as markdown': M.copiedMarkdown(hq, null),
      'retired packetMarkdown': M.packetMarkdown(hq),
    },
    json: JSON.parse(json),
    jsonText: json,
  }
}

function assertLeads(c, label) {
  const m = model(coverage(c))
  const hq = m.view.hq
  const missing = servedMissing(c)
  const lead = pairFor(missing[0][1])
  assert.equal(hq.readiness.ready, false, label)
  assert.equal(said(hq.readiness.blocker), said(lead), `${label}: the one blocker is the first missing obligation's own pair`)
  const s = surfaces(m)

  /* Ask: the blocker first, then every other missing obligation, in served order, with its own pair. */
  assert.equal(s.ask.answered, true, label)
  const expected = [said(lead), ...missing.slice(1).map(([text, slug]) => [text, said(pairFor(slug))].filter(Boolean).join(' — '))]
  assert.deepEqual(s.ask.sentences.slice(0, expected.length), expected, `${label}: Ask's answer to "Why can't I approve?"`)
  assert.equal(s.ask.sentences.length, expected.length, `${label}: Ask says nothing beside the pairs`)
  for (const r of Object.values(R.reasons)) {
    assert.ok(!s.ask.sentences.some((x) => x.endsWith(' — ' + r) || x === r), `${label}: Ask gives a line's own reason as why approval is off: ${r}`)
  }

  /* Every text surface: the first approval sentence is the blocker, with its enabler right after it. */
  for (const [surface, text] of Object.entries(s.text)) {
    const { at, hit } = leadingApprovalWords(text)
    assert.equal(hit, lead.reason, `${label}: ${surface} leads with a different reason:\n  ${hit}\n  one answer: ${lead.reason}`)
    const enabler = surface === 'markdown file' || surface === 'copy as markdown' ? mdCode(said(lead)) : said(lead)
    assert.equal(text.indexOf(enabler), at, `${label}: ${surface} does not follow the blocker with its enabler`)
  }
  assert.equal(leadingApprovalWords(s.jsonText).hit, lead.reason, `${label}: the JSON file leads with a different reason`)
  assert.deepEqual(s.json.readiness.blocker, { reason: lead.reason, enabler: lead.enabler }, `${label}: the JSON file's blocker`)

  /* The Readiness list keeps each line's own reason (card item 4), and its first line is the blocker's own obligation. */
  assert.deepEqual(hq.readiness.blockers.map((b) => b.id), hq.readiness.missing.map((x) => x.id), `${label}: one pair per missing obligation, in served order`)
  assert.deepEqual(hq.readiness.blockers.map((b) => said(b)), missing.map(([, slug]) => said(pairFor(slug))), `${label}: each pair is its own slug's`)
  /* NEVER-ASSESSED-ON-SCREEN (D-050): a line never assessed reads the on-screen act where the screen offers it (old: `R.reasons[slug]`,
   * the terminal's sentence, which is unchanged in the copy file and in voice.rs). */
  const own = (slug) => (slug === 'readiness_never_assessed' ? 'Whether this still applies was never assessed. Check it again on this Change, and it will be.' : R.reasons[slug])
  hq.readiness.missing.forEach((x, i) => assert.equal(x.reason, own(missing[i][1]), `${label}: a line's own reason is unchanged`))
}

/* ── the break round 4's adversary found ──────────────────────────────── */

test('the adversary\'s Change on the Ask bar: identity unconfirmed first, line 1 an unlinked run — Ask leads with the identity blocker and its act', () => {
  const m = model(coverage({ identity: NON_LINE.candidate_identity, c1: 'readiness_unlinked_run' }))
  const hq = m.view.hq
  const b = hq.readiness.blocker
  assert.equal(b.reason, R.blocked_by.readiness_inputs_missing.reason)
  const ask = M.answer('why_not_approve', hq)
  assert.equal(ask.answered, true)
  assert.equal(ask.sentences[0], b.reason + ' ' + b.enabler, 'Ask leads with the one blocker, word for word')
  assert.ok(!ask.sentences.includes(hq.readiness.sentence), 'Ask does not lead with its own count of what still needs evidence')
  assert.equal(ask.sentences[1], LINE_1 + ' — ' + said(pairFor('readiness_unlinked_run')), 'the unlinked line follows, with its own pair')
  assertLeads({ identity: NON_LINE.candidate_identity, c1: 'readiness_unlinked_run' }, 'identity + unlinked run')
})

/* ── the table: every one or two missing obligations, every surface ────── */

function cases() {
  const out = []
  out.push({ identity: NON_LINE.candidate_identity })
  out.push({ review: NON_LINE.review })
  for (const s of LINE_SLUGS) out.push({ c1: s })
  out.push({ identity: NON_LINE.candidate_identity, review: NON_LINE.review })
  for (const s of LINE_SLUGS) {
    out.push({ identity: NON_LINE.candidate_identity, c1: s })
    out.push({ c1: s, review: NON_LINE.review })
    for (const t of LINE_SLUGS) out.push({ c1: s, c2: t })
  }
  out.push({ identity: NON_LINE.candidate_identity, c1: 'readiness_unlinked_run', c2: 'readiness_stale', review: NON_LINE.review })
  return out
}

test('table: for every one or two missing obligations, Ask, the page, the three files, Copy as Markdown and the retired Markdown lead with the one blocker', () => {
  const all = cases()
  assert.equal(all.length, 4 + LINE_SLUGS.length * (3 + LINE_SLUGS.length))
  for (const c of all) assertLeads(c, JSON.stringify(c))
})

/* ── rounds 1–4 hold on the same Changes ───────────────────────────────── */

const offPairs = (html) => [...html.matchAll(/<p class="ws-change-blocked-why" id="(ws-(?:brief|change|inspector|cov)-why-(?:approve|primary)?)"[^>]*>([^<]*)<\/p>/g)].map((x) => [x[1], unesc(x[2])])

test('rounds 1-4 hold: nothing links itself, no decide control while not ready, and Brief, the Approve stage, the Inspector and What we checked say the one blocker', () => {
  for (const c of [{ c1: 'readiness_unlinked_run' }, { identity: NON_LINE.candidate_identity, c1: 'readiness_unlinked_run' }, { c1: 'readiness_stale', review: NON_LINE.review }]) {
    const m = model(coverage(c))
    const hq = m.view.hq
    const one = said(hq.readiness.blocker)
    assert.equal(hq.criteria[0].items.length, 0, 'nothing is linked to the line automatically')
    assert.equal(M.showsDecision(hq), false)
    const props = { model: m.view, onCopy: () => {}, onReload: () => {}, notes: [], register: 'manager', onExplore: () => {}, onDoor: () => {} }
    const html = [
      M.renderBrief(props),
      M.renderScreen({ ...props, stage: 'approve', onGo: () => {} }),
      M.renderInspector({ hq, cards: [], onCopy: () => {}, onDecided: () => {}, onRecheck: () => {}, onDoor: () => {} }),
      M.renderSummary({ hq }),
    ].join('')
    const pairs = offPairs(html).filter(([id]) => /why-approve|inspector-why|cov-why-primary/.test(id))
    assert.ok(pairs.length >= 2, JSON.stringify(c))
    for (const [id, words] of pairs) assert.equal(words, one, `${JSON.stringify(c)}: ${id}`)
  }
})

/* ── the census: no sixth surface arrives unannounced ──────────────────── */

const ROOT = join(here, '..')
const MENTIONS = /readiness|showsDecision|canApprove|approveBlocker|approve\.disabled|awaiting_approval/
const READS = /\.missing\b|\.obligations\b|blocked_by|blocked_reason|blocked_unlinked|unsatisfied_slug|readiness_[a-z_]+|readinessRests|readinessLine|showsDecision|canApprove|\.blocker\b|reasons\[|\.ready\b|approveBlocker|approve\.disabled|awaiting_approval/
const THROUGH_BLOCKER = /blockerOf|readiness\.blocker\b|\.offLines\b|readinessBlocker|inspector\.action\b|hq\.readiness\.blocker/
const MARKER = /NOT-APPROVAL-WORDS: \S/

const EXPLAINS = [
  'workspace/ask-hq.js', 'workspace/change-workspace.jsx', 'workspace/criterion-links.jsx',
  'workspace/change-model.js', 'workspace/packet.jsx', 'workspace/packet-export.js',
  'hq/screens/Approve-Sheet/from-change.js', 'hq/screens/Approve-Sheet/logic.js',
  'hq/screens/PR-Face/from-change.js', 'hq/screens/PR-Face/markdown.js',
]
const NOT_APPROVAL = [
  'workspace/attention.js', 'workspace/inbox-view.js', 'workspace/change-view.js', 'workspace/coverage-view.js',
  'workspace/run-route.js', 'workspace/hq-copy.js', 'workspace/attention-count.fixture.js', 'workspace/integrations.jsx',
  'hq/data/change.js', 'hq/data/inbox-ready.js', 'hq/screens/PR-Face/pr-fixture.js',
  // C46 §1.1: the one "your move" count and the screens that hand it a readiness reading
  'hq/data/project.js', 'hq/data/notification-events.js', 'hq/screens/Changes/from-projects.js',
  'hq/screens/Command-Ask/from-change.js', 'hq/screens/Command-Ask/index.jsx',
]

async function sources(dir) {
  const out = []
  for (const e of await readdir(dir, { withFileTypes: true })) {
    const p = join(dir, e.name)
    if (e.isDirectory()) out.push(...await sources(p))
    else if (/\.(js|jsx)$/.test(e.name) && !/\.test\.js$/.test(e.name)) out.push(p)
  }
  return out
}

test('census: every file under ui/src that reads readiness or decision availability is listed in this file\'s header, and each one either goes through blockerOf or says why its words are not about approval', async () => {
  const found = []
  for (const p of await sources(ROOT)) {
    const src = await readFile(p, 'utf8')
    if (MENTIONS.test(src) && READS.test(src)) found.push(relative(ROOT, p).split('\\').join('/'))
  }
  assert.deepEqual(found.sort(), [...EXPLAINS, ...NOT_APPROVAL].sort(), 'a file reads readiness or decision availability and is not in the census — classify it here and in the header')
  const header = (await readFile(fileURLToPath(import.meta.url), 'utf8')).split('*/')[0]
  for (const f of found) assert.ok(header.includes(f), `${f}: not listed in this file's header`)
  for (const f of EXPLAINS) {
    const src = await readFile(join(ROOT, f), 'utf8')
    assert.match(src, THROUGH_BLOCKER, `${f} explains why approval is off and does not go through blockerOf`)
  }
  for (const f of NOT_APPROVAL) {
    const src = await readFile(join(ROOT, f), 'utf8')
    assert.match(src, MARKER, `${f} reads readiness or decision availability and does not say, in a NOT-APPROVAL-WORDS: comment, why its words are not about approval`)
  }
})

test('census: Ask reads the served lines, never readiness.missing or a line\'s reason, to explain approval', async () => {
  const src = await readFile(join(here, 'ask-hq.js'), 'utf8')
  const body = src.slice(src.indexOf('function whyNotApprove'), src.indexOf('function saidVsRecord'))
  assert.match(body, /offLines/, 'Ask reads readiness.offLines')
  assert.doesNotMatch(body, /m\.reason\b|\.reasons\b/, 'Ask builds no reason of its own')
})
