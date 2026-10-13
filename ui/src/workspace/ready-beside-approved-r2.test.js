/* READY-BESIDE-APPROVED, repair round (verifier FAIL at bcb5551e).
 *
 * The first round hid "Ready for approval for version V" beside "Approved on
 * V". Its verifier found what that left behind, and this file holds each one:
 *
 *   1. Ask. "Why can't I approve?" (an offered question; also typing "ready")
 *      on an approved Change answered "No such answer is written down … nothing
 *      has been checked against this version yet" beside "Approved on e71c5b0"
 *      and "1 of 1 … have evidence". On an approved version Ask now says it is
 *      approved, what that rests on, what it was given under, and how to take
 *      it back; and the question offered is one that fits.
 *   3. Two guards nothing held: hiding the answer even when the engine says
 *      NOT ready (M3), and comparing seven-character labels instead of whole
 *      versions (M7).
 *   4. The JSON export said `ready: null` both for an approved version and for
 *      a Change whose readiness could not be read. They are told apart now.
 *
 * (Finding 2, the exceptions in the exported file, is held in
 * `change-exceptions.test.js`, beside the exceptions it is about.)
 *
 * Expected values are the record's own words: copy strings and the served
 * fields. Every fixture is synthetic. No string here came off a real machine.
 */

import test from 'node:test'
import assert from 'node:assert/strict'
import { dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { build } from 'esbuild'
import { CLASSES, FORMATS, renderExport } from './packet-export.js'
import { HQ_COPY } from './hq-copy.js'

const here = dirname(fileURLToPath(import.meta.url))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeBrief } from './change-workspace.jsx';
  import { PacketPage } from './packet.jsx';
  import { AskPanel } from './ask-hq.jsx';
  import { answer, ask, suggestions } from './ask-hq.js';
  import { deriveChangeModel } from './change-model.js';
  import { buildChangeView } from './change-view.js';
  import { buildCoverageView, evidenceEntries } from './coverage-view.js';
  import { buildPacketView } from './packet-view.js';
  export { answer, ask, suggestions, deriveChangeModel, buildChangeView, buildCoverageView, evidenceEntries, buildPacketView };
  export const renderBrief = props => renderToStaticMarkup(React.createElement(ChangeBrief, props));
  export const renderPacket = props => renderToStaticMarkup(React.createElement(PacketPage, props));
  export const renderAsk = props => renderToStaticMarkup(React.createElement(AskPanel, props));
`

/* No temporary file: the bundle is imported from a data URL. */
const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(" + JSON.stringify(import.meta.url) + ');' },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  return import('data:text/javascript;base64,' + Buffer.from(result.outputFiles[0].text).toString('base64'))
})()

const Q = HQ_COPY.ask.questions.why_not_approve

/* ── the record ───────────────────────────────────────────────────────── */

const V = 'e71c5b0aaaa'

const changeOf = ({ snapshot = V, decisions = null } = {}) => ({
  id: 'ch1',
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Totals add up on the checkout page' },
  implementation: { snapshot_id: snapshot, bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1,
    criteria: ['The totals add up'],
    allowed_scope: ['checkout/'],
    check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet',
  ...(decisions ? { decisions } : {}),
})

const IDENTITY = { id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: null, waiver: null, waivable: false }

/** One accepted line, backed on `target` by a finished run: the engine says ready. */
const covered = (target = V) => ({
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
      fresh: true, fresh_slug: 'applicability_still_applies', execution: 'completed', execution_slug: 'execution_completed',
    }],
  }],
  candidates: [],
  readiness: {
    schema_version: 1,
    ready: true,
    scope_version: 1,
    target_version: target,
    headline_slug: 'readiness_ready',
    obligations: [
      IDENTITY,
      { id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: 'The totals add up', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: 'cargo clippy', waiver: null, waivable: true },
    ],
    rests_on: [{ obligation_id: 'criterion:1', text: 'The totals add up', evidence_name: 'cargo clippy' }],
    missing: [],
    waived: [],
    not_applicable: [],
  },
})

/** The same line, and the run on `target` was interrupted: the engine says NOT ready. */
const interrupted = (target = V) => ({
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
      fresh: true, fresh_slug: 'applicability_still_applies', execution: 'interrupted', execution_slug: 'execution_interrupted',
    }],
  }],
  candidates: [],
  readiness: {
    schema_version: 1,
    ready: false,
    scope_version: 1,
    target_version: target,
    headline_slug: 'readiness_needs_evidence_one',
    obligations: [
      IDENTITY,
      { id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: 'The totals add up', outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'stopped', unsatisfied_slug: 'readiness_stopped', evidence_name: null, waiver: null, waivable: true },
    ],
    rests_on: [],
    missing: ['criterion:1'],
    waived: [],
    not_applicable: [],
  },
})

/** The coverage with no readiness block at all: the answer could not be read. */
const unreadable = () => {
  const c = covered()
  delete c.readiness
  return c
}

const approval = (target = V, id = 'cd_1') => ({
  id, act: 'approved', target_revision: target, scope_version: 1, actor: 'sam', decided_at: '2026-09-22T11:00:00Z',
})
const approvedOn = (target, now = target) => ({ standing: approval(target), history: [approval(target)], target_revision: now, scope_version: 1 })
const withdrawnOn = (target) => {
  const a = approval(target)
  const w = { id: 'cd_2', act: 'withdrawn', target_revision: target, actor: 'sam', decided_at: '2026-09-22T12:00:00Z', withdraws: a.id }
  return { standing: null, history: [a, w], target_revision: target, scope_version: 1 }
}

/* ── what a person meets ──────────────────────────────────────────────── */

const plain = (html) => html
  .replace(/<[^>]+>/g, ' ')
  .replace(/&quot;/g, '"').replace(/&#x27;/g, "'").replace(/&#39;/g, "'")
  .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
  .replace(/\s+/g, ' ')

function surfaces(change, coverage) {
  const hq = M.deriveChangeModel(change, coverage, null, null, [])
  const model = M.buildChangeView({ change, proposals: null, work: [], evidence: M.evidenceEntries(coverage) })
  model.coverage = M.buildCoverageView(coverage)
  model.examined = null
  model.hq = hq
  const packet = M.buildPacketView({ change, proposals: null, work: [], coverage, evidence: [], hq })
  packet.hq = hq
  return {
    hq,
    brief: plain(M.renderBrief({ model, onCopy: () => {}, onReload: () => {}, notes: [], register: 'manager', onExplore: () => {} })),
    page: plain(M.renderPacket({ model: packet, notes: [], onCopy: () => {} })),
    ask: plain(M.renderAsk({ hq, onGo: () => {} })),
    files: Object.fromEntries(FORMATS.flatMap((format) => [[], CLASSES].map((redact) => [
      format + (redact.length ? '/redacted' : ''), renderExport(hq.packet, null, { format, redact }).text,
    ]))),
  }
}

const offered = (hq) => M.suggestions(hq).find((q) => q.id === 'why_not_approve').question
const jsonFiles = (s) => Object.entries(s.files).filter(([k]) => k.startsWith('json')).map(([k, t]) => [k, JSON.parse(t).readiness])

/* ── 1. Ask on an approved version ────────────────────────────────────── */

test('Ask, on a version that stands approved, says it is approved, what that rests on, and how to take it back — never "no such answer"', () => {
  const s = surfaces(changeOf({ decisions: approvedOn(V) }), covered())
  assert.equal(s.hq.decision.headline, 'Approved on e71c5b0')
  for (const r of [M.answer('why_not_approve', s.hq), M.ask('Why can\'t I approve?', s.hq), M.ask('is it ready', s.hq)]) {
    assert.equal(r.id, 'why_not_approve')
    assert.equal(r.answered, true, 'an approved version is never the "the record doesn\'t say" card')
    assert.equal(r.question, Q.question_approved, 'the question it answers is the one that fits')
    const said = r.sentences.join('\n')
    assert.equal(r.sentences[0], 'Approved on e71c5b0', 'it opens with the decision, in the words the screen shows')
    assert.ok(r.sentences.includes(Q.approved_already), 'it says there is nothing left to approve')
    assert.ok(r.sentences.includes(s.hq.decision.by), 'who approved it, when, against which list')
    assert.ok(r.sentences.includes(HQ_COPY.change.model.decision.not_an_action), 'what an approval does not do')
    assert.ok(said.includes('The totals add up — cargo clippy'), 'what the approval rests on, line by line')
    assert.ok(r.sentences.includes(Q.approved_withdraw), 'how to take it back')
    for (const wrong of ['No such answer', 'nothing has been checked', 'Ready for approval']) {
      assert.ok(!said.includes(wrong) && !String(r.missing || '').includes(wrong), `Ask says "${wrong}" of an approved version`)
    }
    assert.equal(r.action.label, Q.action_ready, 'its one action goes to the decision')
  }
  assert.equal(M.answer('why_not_approve', s.hq).question, Q.question_approved)
})

test('Ask does not offer "Why can\'t I approve?" for a version that stands approved; it offers a question that fits', () => {
  const s = surfaces(changeOf({ decisions: approvedOn(V) }), covered())
  assert.equal(offered(s.hq), Q.question_approved)
  assert.notEqual(Q.question_approved, Q.question)
  assert.ok(!s.ask.includes(Q.question), 'the panel a person opens does not offer it')
  assert.ok(s.ask.includes(Q.question_approved))
})

test('Ask still offers and answers "Why can\'t I approve?" where approval is still open: not approved, withdrawn, approved-but-not-ready, and an approval on another version', () => {
  const cases = {
    'not approved, ready': [changeOf(), covered(), true],
    withdrawn: [changeOf({ decisions: withdrawnOn(V) }), covered(), true],
    'approved, and the engine says NOT ready': [changeOf({ decisions: approvedOn(V) }), interrupted(), false],
    'approved on a version sharing the first seven characters': [changeOf({ snapshot: 'e71c5b0bbbb', decisions: approvedOn(V, 'e71c5b0bbbb') }), covered('e71c5b0bbbb'), true],
  }
  for (const [name, [change, coverage, ready]] of Object.entries(cases)) {
    const s = surfaces(change, coverage)
    assert.equal(offered(s.hq), Q.question, `${name}: the question is offered as it was`)
    assert.ok(s.ask.includes(Q.question), `${name}: the panel offers it`)
    const r = M.answer('why_not_approve', s.hq)
    assert.equal(r.answered, true, name)
    assert.equal(r.question, Q.question, name)
    assert.ok(!r.sentences.includes(Q.approved_already), `${name}: never says there is nothing left to approve`)
    if (ready) assert.match(r.sentences[0], /^Ready for approval for version /, `${name}: the ready answer`)
    else assert.deepEqual(r.sentences.slice(0, s.hq.readiness.offLines.length), s.hq.readiness.offLines, `${name}: the blocker, first`)
  }
})

test('Ask on an approved version: every word is the copy file\'s or the record\'s', () => {
  const s = surfaces(changeOf({ decisions: approvedOn(V) }), covered())
  const r = M.answer('why_not_approve', s.hq)
  const inModel = JSON.stringify(s.hq)
  const copy = []
  const walk = (o) => { if (typeof o === 'string') copy.push(o); else if (o && typeof o === 'object') Object.values(o).forEach(walk) }
  walk(HQ_COPY)
  const fromCopy = (t) => copy.some((c) => c === t || (c.includes('{') && c.replace(/\{[a-z_]+\}/g, '').trim() && new RegExp('^' + c.replace(/[.*+?^${}()|[\]\\]/g, '\\$&').replace(/\\\{[a-z_]+\\\}/g, '.+') + '$').test(t)))
  for (const s2 of [...r.sentences, r.question, r.action.label, ...r.evidence.flatMap((e) => [e.kindName, e.label])]) {
    for (const part of String(s2).split(/ — |: /)) {
      assert.ok(fromCopy(part) || inModel.includes(JSON.stringify(part).slice(1, -1)), `"${part}" is in neither the copy file nor the model`)
    }
  }
})

/* ── 3. the two guards nothing held ───────────────────────────────────── */

test('M3: approved on V, and the engine says V is NOT ready — the not-ready answer stays on every surface', () => {
  const expected = surfaces(changeOf(), interrupted()).hq.readiness.sentence
  assert.ok(expected, 'the unapproved derivation of the same coverage has a sentence')
  const s = surfaces(changeOf({ decisions: approvedOn(V) }), interrupted())
  assert.equal(s.hq.decision.approved, true)
  assert.equal(s.hq.decision.movedSince, false, 'the approval is on this very version')
  assert.equal(s.hq.readiness.sentence, expected, 'the engine said not ready; that is never hidden')
  assert.equal(s.hq.readiness.hiddenBecause, undefined)
  assert.equal(s.hq.packet.readinessLine, expected)
  assert.ok(s.brief.includes(expected), 'Brief')
  assert.ok(s.page.includes(expected), 'the whole-story page')
  for (const [k, text] of Object.entries(s.files)) {
    if (k.startsWith('json')) continue
    assert.ok(plain(text).replace(/\\/g, '').includes(expected), `${k} export`)
  }
  for (const [k, r] of jsonFiles(s)) {
    assert.equal(r.ready, false, k)
    assert.equal(r.sentence, expected, k)
    assert.ok(!('hidden_because' in r) && !('shown' in r), `${k}: nothing was hidden`)
  }
})

test('M7: an approval on one version never hides the ready answer of another that shares its first seven characters', () => {
  const NOW = 'e71c5b0bbbb'
  const s = surfaces(changeOf({ snapshot: NOW, decisions: approvedOn(V, NOW) }), covered(NOW))
  assert.equal(s.hq.decision.approved, true)
  assert.equal(s.hq.decision.movedSince, true, 'the approval names the other version')
  assert.match(s.hq.readiness.sentence || '', /^Ready for approval for version \S+\. It rests on:$/)
  assert.equal(s.hq.readiness.hiddenBecause, undefined)
  assert.ok(s.brief.includes(s.hq.readiness.sentence), 'Brief still says this version is ready')
  assert.ok(s.page.includes(s.hq.readiness.sentence), 'and the whole-story page')
  for (const [k, r] of jsonFiles(s)) {
    assert.equal(r.ready, true, k)
    assert.equal(r.sentence, s.hq.readiness.sentence, k)
    assert.ok(!('hidden_because' in r), `${k}: nothing was hidden`)
  }
})

/* ── 4. the JSON: approved, not approved, unreadable ──────────────────── */

test('the JSON tells an approved version from one whose readiness could not be read, and keeps `ready` the engine\'s answer', () => {
  const shapes = {
    approved: surfaces(changeOf({ decisions: approvedOn(V) }), covered()),
    'not approved, ready': surfaces(changeOf(), covered()),
    'not approved, not ready': surfaces(changeOf(), interrupted()),
    unreadable: surfaces(changeOf(), unreadable()),
  }
  const pick = (r) => ({ ready: r.ready, sentence: r.sentence === null ? null : 'said', shown: r.shown, hidden_because: r.hidden_because })
  const expected = {
    approved: { ready: true, sentence: null, shown: false, hidden_because: 'approved' },
    'not approved, ready': { ready: true, sentence: 'said', shown: undefined, hidden_because: undefined },
    'not approved, not ready': { ready: false, sentence: 'said', shown: undefined, hidden_because: undefined },
    unreadable: { ready: null, sentence: null, shown: undefined, hidden_because: undefined },
  }
  for (const [name, s] of Object.entries(shapes)) {
    for (const [k, r] of jsonFiles(s)) assert.deepEqual(pick(r), expected[name], `${name} (${k})`)
  }
  assert.equal(shapes.unreadable.hq.readiness.absent, true, 'the unreadable case really is one')
})
