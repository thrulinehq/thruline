/* T-S-3 acceptance (a), in the DOM.
 *
 * The model-level agreement is asserted in `change-model.test.js`. This file
 * asserts the thing a PERSON meets: one record, one state change, and the
 * Change screen, the Inspector inside it, the Inbox row and the packet page all
 * render the same state word and name the same version. The four surfaces are
 * bundled and rendered to static markup by the same harness the other screen
 * tests use.
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

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-agree-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeScreen, ChangeBrief } from './change-workspace.jsx';
  import { PacketPage } from './packet.jsx';
  import { InboxItem } from './inbox.jsx';
  import { deriveChangeModel } from './change-model.js';
  import { buildChangeView } from './change-view.js';
  import { buildCoverageView, evidenceEntries } from './coverage-view.js';
  import { buildPacketView } from './packet-view.js';
  import { inboxItem } from './inbox-view.js';
  export { deriveChangeModel, buildChangeView, buildCoverageView, evidenceEntries, buildPacketView, inboxItem };
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
  const file = join(temporary, 'agree.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

/* ── one Change, one criterion, one run that proved it ────────────────── */

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

/* RUN-LINK-TRUTH-R3 — `ready` adds the engine's served readiness, ready, for
 * the one test that asks for the decision door: the door opens on readiness
 * (`showsDecision`), and a document with no readiness served is not ready. */
const READY = {
  schema_version: 1, change_id: 'ch1', ready: true, with_exceptions: false, scope_version: 1,
  target_version: 'e71c5b0aaaa', headline_slug: 'readiness_ready',
  obligations: [
    {
      id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '',
      outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null,
      evidence_name: null, linked_by: null, waiver: null, waivable: false,
    },
    {
      id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: 'The totals add up',
      outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null,
      evidence_name: 'tests pass', linked_by: null, waiver: null, waivable: true,
    },
  ],
  rests_on: [{ obligation: 'criterion:1', criterion_index: 1, text: 'The totals add up', evidence_name: 'tests pass', linked_by: null }],
  missing: [], waived: [], not_applicable: [],
}

const coverageDoc = (fresh, ready = false) => ({
  ...(ready ? { readiness: READY } : {}),
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
      fresh, fresh_slug: fresh ? 'applicability_still_applies' : 'applicability_needs_a_recheck',
    }],
  }],
})

/* The served item's OWN shape (`GET /api/changes/<id>/applicability`), not a
 * convenient one: there is no `version` field on it — the version is the one
 * the evidence's snapshot recorded, and the paths the rule compared are
 * `checked_paths`. A fixture that carried a `version` key is how T-F-1-F1
 * went unseen in this file while the screen rendered "since .". */
const APPLICABILITY = {
  items: [{
    link_id: 'l1', criterion_index: 1,
    old_snapshot: 'e71c5b0d9a1f4c27b3e85a60d4c9f118aa27b4e3',
    new_snapshot: '4a2f77c1b9e0d3568af41c2e7b905dd6134ecb82',
    checked_paths: ['checkout/'],
    reason_slug: 'applicability_reason_source_changed', reason_paths: ['checkout/totals.py'],
  }],
}

/** The five surfaces, rendered off ONE derived model, exactly as the app does. */
function surfaces(fresh, ready = false) {
  const coverage = coverageDoc(fresh, ready)
  const hq = M.deriveChangeModel(CHANGE, coverage, APPLICABILITY, null, [])

  const change = M.buildChangeView({ change: CHANGE, proposals: null, work: [], evidence: M.evidenceEntries(coverage) })
  change.coverage = M.buildCoverageView(coverage)
  change.examined = null
  change.hq = hq

  const packet = M.buildPacketView({ change: CHANGE, proposals: null, work: [], coverage, evidence: [] })
  packet.hq = hq

  const row = M.inboxItem({ change: CHANGE, proposals: null, work: [], evidence: [], applicability: [], hq })

  return {
    hq,
    change: M.renderChange({ model: change, stage: 'verify', onGo: () => {}, onCopy: () => {}, notes: [], onReload: () => {} }),
    /* P6-MERGE: the fifth surface. Brief is a DIFFERENT component reading the
     * SAME derived model, so it is the surface most able to drift. */
    brief: M.renderBrief({ model: change, onCopy: () => {}, onReload: () => {}, notes: [], register: 'manager', onExplore: () => {} }),
    packet: M.renderPacket({ model: packet, notes: [], onCopy: () => {} }),
    row: row ? M.renderRow({ item: row }) : '',
  }
}

/* ── the agreement ────────────────────────────────────────────────────── */

test('the record changes state once, and the Change screen, its Inspector, the Inbox row and the packet page all render the same state word and the same version', () => {
  const before = surfaces(true)
  assert.equal(before.hq.word, 'Proven')
  /* Before: the same word, with scope and version, on every surface that
   * carries a line for this criterion. */
  for (const dom of [before.change, before.packet]) {
    assert.match(dom, /Proven: tests pass · version e71c5b0 · checkout\//)
  }

  const after = surfaces(false)
  assert.equal(after.hq.word, 'Needs a re-check')
  const LINE = 'Needs a re-check · version e71c5b0'

  /* 1. The Change screen's criterion line. */
  assert.ok(after.change.includes(LINE), 'the Change screen says it')
  /* 2. The Inspector's block 2, inside that same screen. */
  const inspector = after.change.slice(after.change.indexOf('ws-change-inspector'))
  assert.ok(inspector.includes('What the record shows'), 'the Inspector renders block 2')
  assert.ok(inspector.includes(LINE), 'the Inspector block 2 says it')
  /* 3. The Inbox row. */
  assert.ok(after.row.includes(LINE), 'the Inbox row says it')
  /* 4. The packet criterion line. */
  assert.ok(after.packet.includes(LINE), 'the packet says it')

  /* And none of them still says the word that was true before. */
  for (const dom of [after.row, after.packet]) {
    assert.doesNotMatch(dom, /Proven: tests pass/, 'a surface kept saying what was true before')
  }
})

test('the freshness sentence reaches all three surfaces, and its paths stay one level down', () => {
  const after = surfaces(false)
  const SENTENCE = 'Proven on the previous version — needs a re-check.'
  for (const dom of [after.change, after.packet]) assert.ok(dom.includes(SENTENCE), 'the freshness sentence renders')
  /* The reason class is in Sam's words, on the screen; the path is not. */
  assert.ok(after.change.includes('The affected code changed afterward.'))
  assert.ok(after.change.includes('Since this was checked: The affected code changed afterward.'))
  const plainHalf = after.change.slice(0, after.change.indexOf('ws-change-technical'))
  assert.doesNotMatch(plainHalf, /checkout\/totals\.py/, 'a path escaped the Technical drawer')
  assert.match(after.change, /checkout\/totals\.py/, 'the path is in the drawer, not gone')
})

/* -- T-F-1-F1: a freshness line never renders with a hole in it ---------- */

test('the still-applies line names the paths the rule compared and the version the evidence recorded, on every surface', () => {
  const before = surfaces(true)
  const SENTENCE = 'Still applies — checkout/ unchanged since e71c5b0.'
  assert.equal(before.hq.criteria[0].freshness.sentence, SENTENCE, 'the one derivation says it once')
  let seen = 0
  for (const [name, dom] of Object.entries(before)) {
    if (name === 'hq') continue
    if (!dom.includes('Still applies')) continue
    assert.ok(dom.includes(SENTENCE), `${name} renders the still-applies line with its two values`)
    seen += 1
  }
  assert.ok(seen >= 1, 'at least one surface renders the line, or this test is asserting nothing')
})

test('no rendered freshness line carries an empty value, on any surface, fresh or stale', () => {
  /* The defect this guards (T-F-1-F1): "Still applies — this path unchanged
   * since ." — a claim whose subject and version are both holes. The sweep is
   * on the rendered DOM of every surface, in both states, because the line is
   * built once and read by all of them. */
  const HOLES = [
    [/ since \./, 'a version slot rendered empty'],
    [/this path/, 'the literal words "this path" stood in for a real one'],
    [/— +unchanged/, 'a path slot rendered empty'],
    [/\{(path|version)\}/, 'an unfilled placeholder reached the DOM'],
  ]
  for (const fresh of [true, false]) {
    const rendered = surfaces(fresh)
    for (const [name, dom] of Object.entries(rendered)) {
      if (name === 'hq') continue
      for (const [hole, why] of HOLES) assert.doesNotMatch(dom, hole, `${name} (fresh=${fresh}): ${why}`)
    }
  }
})

test('a record that names neither path nor version says so, rather than rendering a hole', () => {
  /* What the engine serves when the accepted scope cannot be read: an
   * `Applies` record with no `checked_paths` (and `reason_paths` empty by
   * design) and no snapshot. The conclusion still stands; the subject is
   * stated as unnamed, in the copy file's words. */
  const bare = { items: [{ link_id: 'l1', criterion_index: 1, checked_paths: [], reason_paths: [] }] }
  const hq = M.deriveChangeModel(CHANGE, coverageDoc(true), bare, null, [])
  const f = hq.criteria[0].freshness
  assert.equal(f.sentence, 'Still applies — nothing it depends on has changed since this version.')
  assert.doesNotMatch(f.sentence, / since \./)
  assert.doesNotMatch(f.technical, /\{|\}/)
  assert.equal(f.fresh, true, 'an unnamed subject does not turn a fresh record stale')
})

test('the Inspector renders four blocks in the design’s order, with one primary control that says what would enable it', () => {
  const dom = surfaces(false).change
  const inspector = dom.slice(dom.indexOf('ws-change-inspector'))
  const order = ['What this is', 'What the record shows', 'What would settle it', 'What to do']
  let at = -1
  for (const heading of order) {
    const found = inspector.indexOf(heading)
    assert.ok(found > at, `${heading} is out of order or missing`)
    at = found
  }
  assert.ok(inspector.includes('Run the check again'), 'the one primary button, chosen by state')
  assert.ok(inspector.includes('The same check, run again on the version this Change is about now.'))
  /* T-2-10 had this control off, saying what would turn it on, because this
   * build could not start a run. P6-5 (design record §3.7) makes running the
   * check an act this build CAN do: the one primary is on, and it is the run
   * sheet's door. The test says the new truth rather than being deleted. */
  const primary = (inspector.match(/<button[^>]*>Run the check again<\/button>/) || [''])[0]
  assert.ok(primary, 'the one primary is a button')
  assert.doesNotMatch(primary, /disabled/, 'the control is on: it runs the check')
  assert.match(primary, /aria-haspopup="dialog"/, 'it opens the run sheet')
})

/* ── P6-DESIGN-FIXES-13, design record §8 item 1 ───────────────────────── */

/** The "What was looked at for each thing that counts as done" section, alone. */
const criteriaNode = (dom) => dom.slice(dom.indexOf('class="ws-cov"'), dom.indexOf('ws-change-run-check'))

test('the criteria node says what the headline says: the same count sentence, the same state word and version, and one primary', () => {
  const s = surfaces(true, true)
  const node = criteriaNode(s.change)
  assert.ok(node.length > 0, 'the criteria node renders on this Change')

  /* 1. The COUNT. The screen's own top sentence, word for word — not a second
   *    count of the same lines. */
  assert.ok(s.change.includes(s.hq.headline.text), 'the headline sentence is at the top')
  assert.ok(node.includes(s.hq.headline.text), 'the criteria node says the same sentence')

  /* 2. The STATE WORD, with its scope and version, exactly as the Inspector
   *    and the packet render it. */
  const LINE = 'Proven: tests pass · version e71c5b0 · checkout/'
  assert.equal(s.hq.word, 'Proven')
  assert.ok(node.includes(LINE), 'the criteria node carries the same line')
  assert.ok(s.packet.includes(LINE), 'and so does the packet')

  /* 3. The PRIMARY is the decision door, not a re-read of the statements. */
  assert.ok(node.includes('Review and decide'), 'the primary is the Approve door')
  assert.ok(node.includes('aria-controls="ws-change-decision"'), 'it points at the T-S-4 Decision block')
  assert.ok(s.change.includes('id="ws-change-decision"'), 'and that block is on this screen')
  assert.doesNotMatch(node, /Review what counts as done/, 'the prototype’s primary is gone')

  /* RUN-LINK-TRUTH-R3 — the same proven line with no readiness served: the
   * Inspector draws no Decision, so the criteria node offers no door to one. */
  assert.doesNotMatch(criteriaNode(surfaces(true).change), /Review and decide/, 'no decision door without a ready engine')
})

test('when the record no longer backs the lines, the criteria node changes with the headline and offers no decision', () => {
  const s = surfaces(false)
  const node = criteriaNode(s.change)
  assert.equal(s.hq.word, 'Needs a re-check')
  assert.ok(node.includes(s.hq.headline.text), 'the same count sentence as the headline')
  assert.ok(node.includes('Needs a re-check · version e71c5b0'), 'the same state word and version')
  /* The model's own primary for this state, off, with both halves of why —
   * never an approval door on a line that needs a re-check. */
  assert.ok(node.includes('Run the check again'), 'the model’s primary for this state')
  assert.doesNotMatch(node, /Review and decide/, 'no decision door while a line needs a re-check')
  assert.ok(node.includes('ws-cov-why-primary'), 'the reason is attached to the control')
})

/* ── P6-MERGE: the Brief surface joins the agreement ───────────────────── */

/** The one state chip's own text, so the assertion cannot be satisfied by some
 *  other block of the same page that happens to repeat the line. */
function chipText(dom) {
  const m = /<span class="ws-brief-chip"[^>]*>([^<]*)<\/span>/.exec(dom)
  return m ? m[1].replace(/&#x27;/g, "'").replace(/&amp;/g, '&') : null
}

test('Brief renders the same state word, the same line and the same version as the Change screen, and derives none of them itself', () => {
  const before = surfaces(true)
  /* The model's own leading line — the one place scope and version are joined. */
  const LINE_BEFORE = 'Proven: tests pass · version e71c5b0 · checkout/'
  assert.equal(before.hq.inspector.shows.line, LINE_BEFORE)
  assert.ok(before.change.includes(LINE_BEFORE), 'the Change screen says it')
  assert.ok(before.brief.includes(LINE_BEFORE), 'Brief says the same line')
  assert.equal(chipText(before.brief), LINE_BEFORE, 'the chip itself carries the model’s joined line, not a bare word')

  const after = surfaces(false)
  const LINE_AFTER = 'Needs a re-check · version e71c5b0'
  assert.equal(after.hq.word, 'Needs a re-check')
  assert.ok(after.change.includes(LINE_AFTER), 'the Change screen says it')
  assert.ok(after.brief.includes(LINE_AFTER), 'Brief says the same line')
  assert.equal(chipText(after.brief), LINE_AFTER, 'the chip itself changed with the record')
  assert.doesNotMatch(after.brief, /Proven: tests pass/, 'Brief kept saying what was true before')

  /* Same headline sentence, same version sentence — not a second derivation. */
  for (const s of [before, after]) {
    assert.ok(s.brief.includes(s.hq.headline.text), 'Brief carries the model’s one sentence')
    assert.ok(s.change.includes(s.hq.headline.text), 'and so does the Change screen')
    assert.ok(s.brief.includes(s.hq.version.text), 'Brief carries the model’s version sentence')
    assert.ok(s.brief.includes(s.hq.packet.coversLine), 'Brief carries the model’s one scope line')
  }
})

test('Brief is one sentence, one state chip, one scope line, the four-block Inspector and one primary', () => {
  const dom = surfaces(false).brief
  const count = (re) => (dom.match(re) || []).length
  assert.equal(count(/class="ws-change-conclusion"/g), 1, 'exactly one conclusion sentence')
  assert.equal(count(/class="ws-brief-chip"/g), 1, 'exactly one state chip')
  assert.equal(count(/class="ws-brief-scope"/g), 1, 'exactly one scope line')

  const inspector = dom.slice(dom.indexOf('ws-change-inspector'))
  let at = -1
  for (const heading of ['What this is', 'What the record shows', 'What would settle it', 'What to do']) {
    const found = inspector.indexOf(heading)
    assert.ok(found > at, `${heading} is out of order or missing on Brief`)
    at = found
  }
  /* One primary on the screen, and it is inside the marked region. */
  assert.equal(count(/data-primary-action=""/g), 1, 'one marked primary region')
  assert.ok(inspector.includes('Run the check again'), 'the model’s primary for this state')
  /* P6-5: on, and the run sheet's door (it used to be off: this build could
   * not start a run). */
  const primary = (inspector.match(/<button[^>]*>Run the check again<\/button>/) || [''])[0]
  assert.doesNotMatch(primary, /disabled/, 'and it is on')
  assert.match(primary, /class="ws-change-primary"[^>]*aria-haspopup="dialog"|aria-haspopup="dialog"[^>]*class="ws-change-primary"/, 'and it opens the run sheet')
  assert.doesNotMatch(dom, /Review and decide/, 'no second primary while a line needs a re-check')
})

test('Brief’s Plain face carries no engine noun: no seq, no clm_, no chk_, no tree hash', () => {
  const dom = surfaces(false).brief
  const plain = dom.slice(0, dom.indexOf('ws-change-technical') === -1 ? dom.length : dom.indexOf('ws-change-technical'))
  for (const re of [/\bseq\b/, /\bclm_/, /\bchk_/, /tree hash/i, /input_tree_hash/, /argv_hash/]) {
    assert.doesNotMatch(plain, re, `an engine noun reached Brief’s Plain face: ${re}`)
  }
})
