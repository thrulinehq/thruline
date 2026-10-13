/* RUN-LINK-TRUTH-R4 — one leading blocker for approval, chosen once, with its
 * own pair for every reason the engine can serve; no two surfaces disagree.
 *
 * Round 3's adversary (attack 5) served a Change whose `candidate_identity`
 * obligation is unconfirmed (first in the engine's order) and whose one
 * accepted line's own reason is `readiness_unlinked_run`. Brief and the
 * Approve stage said the generic "…run the checks" pair — a check had
 * already run — while "What we checked" and the Inspector, on the same
 * screen, said "A test ran for this Change but isn't linked to this line —
 * link it." Two answers to "why is approval off?".
 *
 * Now ONE function (`blockerOf`, served as `hq.readiness.blocker`) picks the
 * leading blocker: the first missing obligation in the engine's served order.
 * Every surface that says why approval is off reads it. Each reason slug the
 * engine can serve for a missing obligation has its own pair, and "Check it
 * again" is named only where running a check is the act. A line's OWN reason
 * is untouched. Every string below is synthetic.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-run-link-r4-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeBrief, ChangeScreen, Inspector, showsDecision } from './change-workspace.jsx';
  import { CriterionSummary, CriterionCoverage } from './criterion-links.jsx';
  import { deriveChangeModel, readinessOf } from './change-model.js';
  import { buildChangeView } from './change-view.js';
  import { buildCoverageView, evidenceEntries, unlinkedCompletedRun } from './coverage-view.js';
  import { HQ_COPY } from './hq-copy.js';
  export { deriveChangeModel, readinessOf, showsDecision, buildChangeView, buildCoverageView, evidenceEntries, unlinkedCompletedRun, HQ_COPY };
  export const renderBrief = props => renderToStaticMarkup(React.createElement(ChangeBrief, props));
  export const renderScreen = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderInspector = props => renderToStaticMarkup(React.createElement(Inspector, props));
  export const renderSummary = props => renderToStaticMarkup(React.createElement(CriterionSummary, props));
  export const renderCoverage = props => renderToStaticMarkup(React.createElement(CriterionCoverage, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'run-link-r4.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const C = M.HQ_COPY.change
const R = C.model.readiness
const APPROVE = C.approve.action
const LINE_1 = 'When the cart is empty, the total shows zero.'
const LINE_2 = 'The total never goes below zero.'
const RUN_NAME = 'cargo test'

/* ── the reasons the engine can serve for a missing obligation, read from
 * the engine's source at this sha, so a new one cannot arrive unpaired ─── */

const RUST = await readFile(join(here, '../../../crates/core/src/change_readiness.rs'), 'utf8')
const SLUG_FN = RUST.slice(RUST.indexOf('impl Unsatisfied'), RUST.indexOf('impl Unsatisfied') + 1500)
const ENGINE_SLUGS = [...SLUG_FN.matchAll(/=> "(readiness_[a-z_]+)"/g)].map((m) => m[1])
/* The two that belong to obligations tied to no line. */
const NON_LINE = { candidate_identity: 'readiness_inputs_missing', review: 'readiness_review_missing' }
const LINE_SLUGS = ENGINE_SLUGS.filter((s) => !Object.values(NON_LINE).includes(s))

/* Where running a check is the act that can clear the reason. */
const RUNNABLE = ['readiness_nothing_looked', 'readiness_settled_nothing', 'readiness_other_version', 'readiness_stale', 'readiness_stopped', 'readiness_could_not_assess']
/* Where no act on the screen or in the terminal clears it in this build. */
const NO_ACT = ['readiness_tool_absent', 'readiness_unknown', 'readiness_review_missing']
const ACT_WORDS = [/Check it again/, /thruline /, /\blink\b/i, /Look at what broke/, /\brun\b/i]

/* ── served documents, in the engine's shape ───────────────────────────── */

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

/**
 * @param {{identity?: string|null, c1?: string|null, c2?: string|null, review?: string|null|undefined}} missing
 *   the slug each obligation is missing with, or null when it is satisfied;
 *   `review` undefined means the policy asks for no review at all.
 */
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
  return view
}

/* ── reading every "why is approval off" off the rendered surfaces ─────── */

const unesc = (s) => s.replaceAll('&#x27;', "'").replaceAll('&quot;', '"').replaceAll('&amp;', '&').replaceAll('&lt;', '<').replaceAll('&gt;', '>')

/** Every off control on a rendered surface: its label, its id, and the words its aria-describedby points at. */
function offControls(html) {
  const out = []
  const re = /<button type="button" disabled="" aria-describedby="([^"]+)"[^>]*>([^<]*)<\/button>/g
  for (const m of html.matchAll(re)) {
    const id = m[1]
    const said = new RegExp(`<p class="ws-change-blocked-why" id="${id.replace(/[-]/g, '\\-')}"[^>]*>([^<]*)</p>`).exec(html)
    out.push({ id, label: unesc(m[2]), said: said ? unesc(said[1]) : null })
  }
  return out
}

const props = (m) => ({ model: m, onCopy: () => {}, onReload: () => {}, notes: [], register: 'manager', onExplore: () => {}, onDoor: () => {} })

/**
 * The words each surface uses for "why approval is off", keyed by surface.
 * A surface whose one control is a live door (the run sheet, what broke)
 * says nothing about approval, so it cannot disagree; it is left out.
 */
function whyApprovalIsOff(m) {
  const said = {}
  const brief = offControls(M.renderBrief(props(m)))
  const approveStage = offControls(M.renderScreen({ ...props(m), stage: 'approve', onGo: () => {} }))
  const inspector = offControls(M.renderInspector({ hq: m.hq, cards: [], onCopy: () => {}, onDecided: () => {}, onRecheck: () => {}, onDoor: () => {} }))
  const summary = offControls(M.renderSummary({ hq: m.hq }))
  const pick = (name, list, id) => {
    const c = list.find((x) => x.id === id)
    if (c) said[name] = c
  }
  pick('brief', brief, 'ws-brief-why-approve')
  pick('approve stage', approveStage, 'ws-change-why-approve')
  pick('inspector', inspector, 'ws-inspector-why')
  pick('what we checked', summary, 'ws-cov-why-primary')
  /* The Inspector's model action, which "What we checked" and the Inspector draw when it is off. */
  const a = m.hq.inspector.action
  if (!a.enabled) said['inspector model'] = { id: 'model', label: a.offLabel || a.label, said: a.reason + ' ' + a.enabler }
  return said
}

function assertOneAnswer(m, label) {
  const b = m.hq.readiness.blocker
  const expected = b.reason + ' ' + b.enabler
  const said = whyApprovalIsOff(m)
  assert.ok(said.brief, `${label}: Brief draws no off Approve`)
  assert.ok(said['approve stage'], `${label}: the Approve stage draws no off Approve`)
  for (const [surface, c] of Object.entries(said)) {
    assert.equal(c.said, expected, `${label}: ${surface} says why approval is off differently:\n  ${c.said}\n  one answer: ${expected}`)
    assert.equal(c.label, APPROVE, `${label}: ${surface}'s off control explaining approval is labelled "${c.label}"`)
  }
  return b
}

/* ── the break round 3's adversary found ──────────────────────────────── */

test('the adversary\'s Change: identity unconfirmed first, the line\'s own reason is an unlinked run — every surface names the identity blocker, and not "run the checks"', () => {
  const m = model(coverage({ identity: 'readiness_inputs_missing', c1: 'readiness_unlinked_run' }))
  assert.equal(m.hq.readiness.ready, false)
  assert.equal(M.showsDecision(m.hq), false, 'no decide control while not ready')
  const b = assertOneAnswer(m, 'identity + unlinked run')
  assert.equal(b.reason, R.blocked_by.readiness_inputs_missing.reason)
  assert.equal(b.enabler, R.blocked_by.readiness_inputs_missing.enabler)
  assert.doesNotMatch(b.enabler, /Run the checks/, 'a check already ran; running checks is not the act that clears this')
  assert.equal(m.hq.inspector.action.reason, b.reason, 'the Inspector\'s model says the same, word for word')
})

/* ── every slug the engine can serve has its own pair ──────────────────── */

test('the reasons read from the engine are the set this test knows, and each has its own pair in the copy file', () => {
  assert.deepEqual([...ENGINE_SLUGS].sort(), [
    'readiness_could_not_assess', 'readiness_inputs_missing', 'readiness_never_assessed', 'readiness_not_proven', 'readiness_nothing_looked',
    'readiness_other_version', 'readiness_review_missing', 'readiness_settled_nothing', 'readiness_stale',
    'readiness_stopped', 'readiness_tool_absent', 'readiness_unknown', 'readiness_unlinked_run',
  ], 'the engine serves a reason this card did not pair; give it one')
  const reasons = new Set()
  for (const slug of ENGINE_SLUGS) {
    const pair = slug === 'readiness_unlinked_run' ? R.blocked_unlinked : R.blocked_by[slug]
    assert.ok(pair && pair.reason && pair.enabler, `${slug}: no pair of its own`)
    assert.notEqual(pair.reason, R.blocked_reason, `${slug}: still the generic reason`)
    assert.notEqual(pair.enabler, R.blocked_enabler, `${slug}: still the generic "run the checks" enabler`)
    assert.ok(!reasons.has(pair.reason), `${slug}: shares its reason with another slug`)
    reasons.add(pair.reason)
    assert.match(pair.reason, /^This version can't be approved yet: /, `${slug}: the reason says what it is about`)
    assert.doesNotMatch(pair.enabler, /turns on|will be approv/i, `${slug}: the enabler promises a result`)
    assert.equal(/Check it again/.test(pair.enabler), RUNNABLE.includes(slug), `${slug}: "Check it again" is named exactly where running a check is the act`)
    if (NO_ACT.includes(slug)) {
      for (const w of ACT_WORDS) assert.doesNotMatch(pair.enabler, w, `${slug}: nothing clears this, and the enabler names an act`)
    }
  }
})

/* ── the table: every one or two missing obligations the engine can serve ─ */

function cases() {
  const out = []
  /* one missing */
  out.push({ identity: NON_LINE.candidate_identity })
  out.push({ review: NON_LINE.review })
  for (const s of LINE_SLUGS) out.push({ c1: s })
  /* two missing, in every served order */
  out.push({ identity: NON_LINE.candidate_identity, review: NON_LINE.review })
  for (const s of LINE_SLUGS) {
    out.push({ identity: NON_LINE.candidate_identity, c1: s })
    out.push({ c1: s, review: NON_LINE.review })
    for (const t of LINE_SLUGS) out.push({ c1: s, c2: t })
  }
  return out
}

const firstOf = (c) => c.identity || c.c1 || c.c2 || c.review

/* NEVER-ASSESSED-ON-SCREEN (D-050) — the pair for a line never assessed, where the screen offers Check it again for this
 * Change (this model has an id, so it does). `R.blocked_by.readiness_never_assessed` is the terminal's pair and is unchanged;
 * the screen says the on-screen act in place of the command. */
const NEVER_ASSESSED_HERE = {
  reason: "This version can't be approved yet: whether a check still applies to a line of what counts as done was never assessed.",
  enabler: 'Open Check it again on this Change, and it will be assessed; the control is in Brief or Inspector. The run sheet shows which checks can run here.',
}

test('table: for every one or two missing obligations, every surface names the first one\'s pair, word for word', () => {
  const all = cases()
  assert.equal(all.length, 3 + LINE_SLUGS.length * (3 + LINE_SLUGS.length), `the table is ${all.length} cases`)
  for (const c of all) {
    const label = JSON.stringify(c)
    const m = model(coverage(c))
    assert.equal(m.hq.readiness.ready, false, label)
    const b = assertOneAnswer(m, label)
    const first = firstOf(c)
    const pair = first === 'readiness_unlinked_run' ? null : first === 'readiness_never_assessed' ? NEVER_ASSESSED_HERE : R.blocked_by[first]
    if (pair) {
      assert.equal(b.reason, pair.reason, `${label}: the first missing obligation's own reason`)
      assert.equal(b.enabler, pair.enabler, `${label}: the first missing obligation's own next act`)
    } else {
      assert.equal(b.reason, R.blocked_unlinked.reason.replace('{name}', RUN_NAME), label)
      assert.equal(b.enabler, R.blocked_unlinked.enabler, label)
    }
  }
})

/* ── a line's own reason is unchanged ──────────────────────────────────── */

test('a line\'s own reason still reads the unlinked-run sentence; only "why approval is off" is one answer', () => {
  const cov = coverage({ identity: NON_LINE.candidate_identity, c1: 'readiness_unlinked_run' })
  const m = model(cov)
  const line = m.hq.readiness.obligations.find((o) => o.id === 'criterion:1')
  assert.equal(line.reason, R.reasons.readiness_unlinked_run)
  const section = M.renderCoverage({ changeId: 'ch1', view: M.buildCoverageView(cov), hq: m.hq })
  assert.ok(unesc(section).includes(R.reasons.readiness_unlinked_run) || unesc(section).includes(C.coverage.states.unlinked_run), 'What we checked still says the line\'s own sentence')
})

/* ── rounds 1–3 hold ───────────────────────────────────────────────────── */

test('rounds 1-3 hold: nothing links itself, no decide control while not ready, and the enabler never promises', () => {
  for (const c of [{ c1: 'readiness_unlinked_run' }, { identity: NON_LINE.candidate_identity, c1: 'readiness_unlinked_run' }]) {
    const m = model(coverage(c))
    assert.equal(m.hq.criteria[0].items.length, 0, 'nothing is linked to the line automatically')
    assert.equal(M.showsDecision(m.hq), false)
    const summary = M.renderSummary({ hq: m.hq })
    assert.ok(!summary.includes('ws-cov-decide'), 'no live decide door in What we checked')
    assert.doesNotMatch(m.hq.readiness.blocker.enabler, /turns on/)
  }
})

test('a line the record holds the opposite of keeps its live "Look at what broke" door, and the surfaces that do say why approval is off say the one answer', () => {
  const cov = coverage({ identity: NON_LINE.candidate_identity, c1: 'readiness_not_proven' })
  cov.criteria[0] = { index: 1, text: LINE_1, state: 'evidence', items: [{ ...PROVEN_ITEM, observed: 'not_proven' }], removed: [] }
  const m = model(cov)
  assert.equal(m.hq.state, 'not_proven')
  assert.equal(m.hq.inspector.action.enabled, true, 'the door to what broke stays on')
  const b = assertOneAnswer(m, 'identity + not proven')
  assert.equal(b.reason, R.blocked_by.readiness_inputs_missing.reason)
})
