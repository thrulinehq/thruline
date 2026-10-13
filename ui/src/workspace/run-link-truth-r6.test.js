/* RUN-LINK-TRUTH-R6 — when readiness is ready (with or without exceptions),
 * no surface draws a "blocked" line about approval; `approveBlocker` speaks
 * only when no readiness answer is served.
 *
 * Round 5's adversary rendered `change-exceptions.test.js`'s own
 * `coverageWith(WAIVER)` — line 2 set aside by a named person, the engine
 * answering `readiness_ready_with_exceptions`, `hq.decision.canApprove` true —
 * at the Approve stage. The stage's `<Blocked id="ws-change-why-approve">`
 * fell back to `model.blocker` (`approveBlocker` in change-view.js, a count of
 * lines with no evidence item) whenever `readiness.ready` was true, and said
 * "1 thing that counts as done has no evidence yet. Run the check for it from
 * this screen…" under "Set aside, with nobody checking: … Nobody checked this."
 *
 * THE RULE. When a readiness answer is served, every surface explains approval
 * from it and from nothing else:
 *   - ready / ready with exceptions: no blocked line anywhere; the Change
 *     screen's surfaces show the live decision, or say it is available in the
 *     words `readiness.available` carries;
 *   - not ready: every surface leads with `readiness.blocker` (rounds 4–5);
 *   - no readiness answer: every surface says the absent answer's pair, and
 *     `approveBlocker` reaches a surface only when there is no derived model
 *     at all (`model.hq` absent).
 *
 * THE CENSUS (round 5's list, every surface that explains approval): Brief,
 * the Approve stage, the Inspector, What we checked, Ask, the whole-story
 * page, the Markdown, HTML and JSON files, and Copy as Markdown.
 *
 * Every string below is synthetic.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-run-link-r6-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeBrief, ChangeScreen, Inspector, showsDecision } from './change-workspace.jsx';
  import { CriterionSummary } from './criterion-links.jsx';
  import { deriveChangeModel } from './change-model.js';
  import { buildChangeView } from './change-view.js';
  import { buildCoverageView, evidenceEntries, unlinkedCompletedRun } from './coverage-view.js';
  import { buildPacketView } from './packet-view.js';
  import { PacketPage, copiedMarkdown } from './packet.jsx';
  import { renderExport } from './packet-export.js';
  import { answer } from './ask-hq.js';
  import { HQ_COPY } from './hq-copy.js';
  export { deriveChangeModel, showsDecision, buildChangeView, buildCoverageView, evidenceEntries, unlinkedCompletedRun, buildPacketView, copiedMarkdown, renderExport, answer, HQ_COPY };
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
  const file = join(temporary, 'run-link-r6.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const C = M.HQ_COPY.change
const R = C.model.readiness
const D = C.approve.disabled
const VERSION = 'e71c5b0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
const LINE_1 = 'The totals add up'
const LINE_2 = 'A declined card shows a message'
const AUTHORITY = 'the release policy'
const WAIVED_LINE = 'Waived by sam, under the release policy: the payment sandbox is down all week. Nobody checked this.'
const NOT_APPLICABLE_LINE = "Doesn't apply here — the payment sandbox is down all week (said by sam, under the release policy). Nobody checked this."

/* ── the adversary's fixture: change-exceptions.test.js, verbatim in shape ── */

const setAside = (kind) => ({
  id: 'cx_1', obligation_id: 'criterion:2', kind, actor: 'sam', authority: AUTHORITY,
  rationale: 'the payment sandbox is down all week', at: '2026-09-23T10:00:00Z',
})

const change = () => ({
  id: 'ch1', project_id: 'p1', opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Totals add up on the checkout page' },
  implementation: { snapshot_id: VERSION, bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1, criteria: [LINE_1, LINE_2], allowed_scope: ['checkout/'],
    check_profile: ['policy:exceptions-allowed-by:' + AUTHORITY],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet', decisions: [],
})

const proven = (n) => ({
  link_id: 'l' + n, evidence_id: 'e' + n, session_id: 's1', basis: 'declared', kind: 'check_run',
  name: 'tests pass', observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies',
  fresh: true, fresh_slug: 'applicability_still_applies',
})

const identity = { id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: null, waiver: null, waivable: false }
const satisfied = (n, text) => ({ id: 'criterion:' + n, kind: 'criterion', criterion_index: n, text, outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: 'tests pass', waiver: null, waivable: true })

/**
 * The four readiness states, each as the engine serves it.
 *   ready        — both lines checked;
 *   waived / not_applicable — line 2 set aside by a named person (T-5-3);
 *   not_ready    — line 2 has no test;
 *   none         — no readiness answer at all.
 */
function coverage(state) {
  const two = state === 'ready'
  const criteria = [
    { index: 1, text: LINE_1, state: 'evidence', items: [proven(1)] },
    two ? { index: 2, text: LINE_2, state: 'evidence', items: [proven(2)] } : { index: 2, text: LINE_2, state: 'none', items: [] },
  ]
  const base = { scope_version: 1, link_rows: two ? 2 : 1, execution_sessions: ['s1'], criteria }
  if (state === 'none') return base
  const kind = state === 'waived' || state === 'not_applicable' ? state : null
  const ready = state !== 'not_ready'
  const line2 = two
    ? satisfied(2, LINE_2)
    : { id: 'criterion:2', kind: 'criterion', criterion_index: 2, text: LINE_2, outcome: kind || 'needs_evidence', outcome_slug: kind || 'needs_evidence', unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked', evidence_name: null, waiver: kind ? setAside(kind) : null, waivable: true }
  return {
    ...base,
    readiness: {
      schema_version: 2, ready, with_exceptions: Boolean(kind), scope_version: 1, target_version: VERSION,
      headline_slug: kind ? 'readiness_ready_with_exceptions' : ready ? 'readiness_ready' : 'readiness_needs_evidence_one',
      obligations: [identity, satisfied(1, LINE_1), line2],
      rests_on: ready
        ? [{ obligation_id: 'criterion:1', text: LINE_1, evidence_name: 'tests pass' }, ...(two ? [{ obligation_id: 'criterion:2', text: LINE_2, evidence_name: 'tests pass' }] : [])]
        : [],
      missing: ready ? [] : ['criterion:2'],
      waived: kind === 'waived' ? ['criterion:2'] : [],
      not_applicable: kind === 'not_applicable' ? ['criterion:2'] : [],
    },
  }
}

function modelOf(state) {
  const ch = change()
  const cov = coverage(state)
  const view = M.buildChangeView({ change: ch, proposals: null, work: [], evidence: M.evidenceEntries(cov), unlinkedRun: M.unlinkedCompletedRun(cov) })
  view.coverage = M.buildCoverageView(cov)
  view.examined = null
  view.hq = M.deriveChangeModel(ch, cov, null, null, [])
  return { view, cov, ch, hq: view.hq }
}

/* ── reading the surfaces ──────────────────────────────────────────────── */

const unesc = (s) => s.replaceAll('&#x27;', "'").replaceAll('&#39;', "'").replaceAll('&quot;', '"').replaceAll('&#x2F;', '/').replaceAll('&amp;', '&').replaceAll('&lt;', '<').replaceAll('&gt;', '>')
const plain = (html) => unesc(html.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ').trim()
const said = (p) => p.reason + ' ' + p.enabler

/** Every sentence the copy file has for an Approve door that is OFF. */
const BLOCKED_WORDS = [
  ...Object.values(R.blocked_by).map((p) => p.reason),
  R.blocked_unlinked.reason_unnamed, R.blocked_unlinked.reason.split('{name}')[0],
  R.blocked_reason,
  ...['no_criteria', 'no_version', 'problem_one', 'problem', 'unlinked_one', 'unlinked', 'no_evidence_one', 'no_evidence']
    .map((k) => D[k].reason.split('{n}').pop()),
]

/** The OFF pairs a render draws: id → words. */
const offPairs = (html) => [...html.matchAll(/<p class="ws-change-blocked-why" id="([^"]+)"[^>]*>([^<]*)<\/p>/g)].map((x) => [x[1], unesc(x[2])])
const APPROVAL_OFF = /why-approve|inspector-why|cov-why-primary/
/** An off Approve control explained by readiness (`Blocked`, described by one
 * of the approval ids). The live Decision's own button — off while the licence
 * is unread (`ws-decision-why`, T-DIST-4) — is the live decision, not a
 * blocked line about readiness. */
const offApprove = (html) => new RegExp(`<button[^>]*disabled=""[^>]*aria-describedby="(ws-brief-why-approve|ws-change-why-approve|ws-inspector-why|ws-cov-why-primary)"[^>]*>${C.approve.action}</button>`).test(html)

function surfaces(m) {
  const hq = m.hq
  const props = { model: m.view, onCopy: () => {}, onReload: () => {}, notes: [], register: 'manager', onExplore: () => {}, onDoor: () => {} }
  const page = M.renderPacket({ model: { ...M.buildPacketView({ change: m.ch, proposals: null, work: [], coverage: m.cov, evidence: [], hq }), hq }, notes: [] })
  const json = M.renderExport(hq.packet, null, { format: 'json' }).text
  return {
    screen: {
      brief: M.renderBrief(props),
      'approve stage': M.renderScreen({ ...props, stage: 'approve', onGo: () => {} }),
      inspector: M.renderInspector({ hq, cards: [], onCopy: () => {}, onDecided: () => {}, onRecheck: () => {}, onDoor: () => {} }),
      'what we checked': M.renderSummary({ hq }),
    },
    ask: M.answer('why_not_approve', hq),
    text: {
      'whole-story page': plain(page),
      'markdown file': M.renderExport(hq.packet, null, { format: 'md' }).text,
      'html file': plain(M.renderExport(hq.packet, null, { format: 'html' }).text),
      'json file': json,
      'copy as markdown': M.copiedMarkdown(hq, null),
    },
    json: JSON.parse(json),
  }
}

/* ── 1. the break round 5's adversary found ────────────────────────────── */

for (const kind of ['waived', 'not_applicable']) {
  test(`the adversary's Change (${kind}): the Approve stage draws no blocked line, says approval is available, and keeps the set-aside line`, () => {
    const m = modelOf(kind)
    assert.equal(m.hq.readiness.ready, true)
    assert.equal(m.hq.readiness.withExceptions, true)
    assert.equal(m.hq.decision.canApprove, true)
    const approve = M.renderScreen({ model: m.view, stage: 'approve', onGo: () => {}, onCopy: () => {}, notes: [], onReload: () => {} })
    const text = plain(approve)
    assert.ok(!text.includes('has no evidence yet'), 'the stage must not say a set-aside line has no evidence yet')
    assert.ok(!text.includes('Run the check for it'), 'nor send a person to run the check nobody is going to run')
    assert.deepEqual(offPairs(approve), [], 'no off control with a reason on a Change that is ready')
    assert.ok(!offApprove(approve), 'no disabled Approve on a Change that is ready')
    assert.ok(text.includes(said(R.available_with_exceptions)), 'the stage says approval is available, with exceptions')
    assert.match(approve, /class="ws-change-primary ws-approve-decide"/, 'and its one primary is a live door to the decision')
    assert.ok(text.includes(R.set_aside_heading), 'Set aside keeps its heading')
    assert.ok(text.includes(kind === 'waived' ? WAIVED_LINE : NOT_APPLICABLE_LINE), 'and its line, exactly as T-5-3 serves it')
  })
}

/* ── 2. the census: four readiness states, every surface ───────────────── */

function assertReady(state) {
  const m = modelOf(state)
  const hq = m.hq
  const r = hq.readiness
  const exceptions = state !== 'ready'
  assert.equal(r.ready, true, state)
  assert.equal(r.withExceptions, exceptions, state)
  assert.equal(M.showsDecision(hq), true, state)
  const available = exceptions ? R.available_with_exceptions : R.available
  assert.deepEqual(r.available, { reason: available.reason, enabler: available.enabler }, `${state}: the served "available" pair`)
  assert.equal(r.blocker, null, `${state}: no blocker is served on a ready answer`)
  const s = surfaces(m)
  for (const [surface, html] of Object.entries(s.screen)) {
    const off = offPairs(html).filter(([id]) => APPROVAL_OFF.test(id))
    assert.deepEqual(off, [], `${state}: ${surface} draws an off pair about approval`)
    assert.ok(!offApprove(html), `${state}: ${surface} draws a disabled Approve`)
    const words = plain(html)
    for (const w of BLOCKED_WORDS) assert.ok(!words.includes(w), `${state}: ${surface} says a blocked line: ${w}`)
    if (exceptions) {
      assert.ok(!words.includes(R.available.reason + ' ' + R.available.enabler), `${state}: ${surface} says the no-exceptions words over an answer with a line set aside`)
    }
  }
  /* The live decision, or "available". */
  assert.match(s.screen.inspector, /class="ws-decision"/, `${state}: the Inspector shows the live decision`)
  assert.match(s.screen.brief, /class="ws-decision"/, `${state}: Brief shows the live decision`)
  assert.match(s.screen['what we checked'], /class="ws-change-primary ws-cov-decide"/, `${state}: What we checked opens the live decision`)
  assert.ok(plain(s.screen['approve stage']).includes(said(available)), `${state}: the Approve stage says it is available`)
  assert.match(s.screen['approve stage'], /class="ws-change-primary ws-approve-decide"/, `${state}: the Approve stage opens the live decision`)
  /* Ask says the same: the readiness sentence, what it rests on, what was set aside, and that it is available. */
  assert.equal(s.ask.answered, true, state)
  assert.equal(s.ask.sentences[0], r.sentence, `${state}: Ask leads with the readiness sentence`)
  assert.ok(s.ask.sentences.includes(said(available)), `${state}: Ask says approval is available in the same words`)
  for (const w of BLOCKED_WORDS) assert.ok(!s.ask.sentences.some((x) => x.includes(w)), `${state}: Ask says a blocked line: ${w}`)
  /* The page, the three files and Copy as Markdown: the readiness sentence, and no blocked line. */
  for (const [surface, text] of Object.entries(s.text)) {
    assert.ok(text.includes(r.sentence), `${state}: ${surface} carries the readiness sentence`)
    for (const w of BLOCKED_WORDS) assert.ok(!text.includes(w), `${state}: ${surface} says a blocked line: ${w}`)
  }
  assert.equal(s.json.readiness.blocker, null, `${state}: the JSON file carries no blocker`)
  /* The set-aside line: everywhere the readiness list is, as T-5-3 serves it. */
  if (exceptions) {
    const line = state === 'waived' ? WAIVED_LINE : NOT_APPLICABLE_LINE
    assert.equal(r.waivedLines[0].waiver, line, state)
    for (const surface of ['brief', 'approve stage']) {
      const words = plain(s.screen[surface])
      assert.ok(words.includes(R.set_aside_heading) && words.includes(line), `${state}: ${surface} keeps the set-aside line`)
    }
    assert.ok(s.ask.sentences.some((x) => x.includes(line)), `${state}: Ask names what was set aside`)
    for (const surface of ['whole-story page', 'markdown file', 'copy as markdown']) {
      assert.ok(s.text[surface].includes(line), `${state}: ${surface} keeps the set-aside line`)
    }
  }
}

test('census, ready: no surface draws a blocked line; the Change screen shows the live decision or says it is available', () => assertReady('ready'))
test('census, ready with exceptions (waived): the same, and the set-aside line stays', () => assertReady('waived'))
test('census, ready with exceptions (does not apply): the same, and the set-aside line stays', () => assertReady('not_applicable'))

test('census, not ready: every surface leads with the one blocker, and none says approval is available', () => {
  const m = modelOf('not_ready')
  const hq = m.hq
  const one = said(hq.readiness.blocker)
  assert.equal(hq.readiness.blocker.reason, R.blocked_by.readiness_nothing_looked.reason)
  assert.equal(hq.readiness.available, null, 'nothing is "available" on a not-ready answer')
  const s = surfaces(m)
  const avail = [R.available.reason, R.available_with_exceptions.reason, D.decide_below.reason]
  for (const [surface, html] of Object.entries(s.screen)) {
    const off = offPairs(html).filter(([id]) => APPROVAL_OFF.test(id))
    /* The Inspector's one primary on this Change is the run door (P6-5), a
     * live act; every OFF pair about approval that is drawn is the one blocker. */
    if (surface === 'brief' || surface === 'approve stage') assert.ok(off.length >= 1, `not ready: ${surface} draws the off Approve`)
    for (const [id, words] of off) assert.equal(words, one, `not ready: ${surface} (${id})`)
    for (const w of avail) assert.ok(!plain(html).includes(w), `not ready: ${surface} says approval is available`)
  }
  assert.equal(s.ask.sentences[0], one, 'not ready: Ask leads with the one blocker')
  for (const [surface, text] of Object.entries(s.text)) {
    const b = hq.readiness.blocker
    assert.ok(surface === 'json file' ? text.includes(JSON.stringify(b.reason)) && text.includes(JSON.stringify(b.enabler)) : text.includes(one), `not ready: ${surface} carries the one blocker`)
    for (const w of avail) assert.ok(!text.includes(w), `not ready: ${surface} says approval is available`)
  }
})

test('census, no readiness answer: every surface that draws the Approve door says the absent answer\'s pair; approveBlocker speaks only when there is no model at all', () => {
  const m = modelOf('none')
  const hq = m.hq
  assert.equal(hq.readiness.absent, true)
  assert.equal(hq.readiness.available, null)
  const one = said(hq.readiness.blocker)
  const s = surfaces(m)
  const approveBlockerWords = BLOCKED_WORDS.slice(-8)
  for (const [surface, html] of Object.entries(s.screen)) {
    for (const [id, words] of offPairs(html).filter(([i]) => /why-approve/.test(i))) assert.equal(words, one, `no readiness: ${surface} (${id})`)
    for (const w of approveBlockerWords) assert.ok(!plain(html).includes(w), `no readiness, with a model: ${surface} says approveBlocker's words: ${w}`)
    assert.ok(!plain(html).includes(R.available.reason), `no readiness: ${surface} says approval is available`)
  }
  assert.ok(offPairs(s.screen['approve stage']).some(([id]) => id === 'ws-change-why-approve'), 'the Approve stage still draws the off door')
  assert.equal(s.ask.answered, false, 'Ask says it cannot answer rather than inventing a blocker')

  /* With no derived model at all, the Approve stage is the one place approveBlocker speaks. */
  const bare = { ...m.view, hq: null }
  const approve = M.renderScreen({ model: bare, stage: 'approve', onGo: () => {}, onCopy: () => {}, notes: [], onReload: () => {} })
  const off = offPairs(approve).find(([id]) => id === 'ws-change-why-approve')
  assert.ok(off, 'with no model the Approve stage still says why it is off')
  const b = m.view.blocker
  assert.equal(off[1], (b.reason + ' ' + b.enabler).replaceAll('{n}', String(b.values.n)))
})

/* ── 3. the source: model.blocker is read in one guarded place ─────────── */

test('source: change-workspace.jsx reads model.blocker only where no derived model exists', async () => {
  const src = await readFile(join(here, 'change-workspace.jsx'), 'utf8')
  const reads = src.split('\n').filter((l) => /model\.blocker\b/.test(l) && !/^\s*(\*|\/\/|\{\/\*)/.test(l))
  assert.ok(reads.length >= 1)
  for (const l of reads) assert.match(l, /NO-READINESS/, `model.blocker read outside the no-model branch: ${l.trim()}`)
})
