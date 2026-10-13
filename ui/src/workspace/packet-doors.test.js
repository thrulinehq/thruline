/* T-S-5 — the packet's two real doors, and the taken-away copy.
 *
 * Acceptance item 0 FIRST, because a Markdown builder behind a screen nobody
 * can reach is not the thing the design record asked for: this file asserts
 * the two doors in the rendered DOM before it asserts a single byte of
 * Markdown. Then the snapshot, then the fence — no digit sequence and no state
 * word in the copied text that is not a typed field of the one view-model.
 *
 * Every fixture is synthetic. No string here came off a real machine.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

import { STATE_WORD_SET } from './change-model.js'

const here = dirname(fileURLToPath(import.meta.url))
const GOLDEN = fileURLToPath(new URL('./packet-markdown.golden.md', import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-doors-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeScreen, packetHref } from './change-workspace.jsx';
  import { PacketPage, packetIdFromHash } from './packet.jsx';
  import { deriveChangeModel, packetMarkdown } from './change-model.js';
  import { buildChangeView } from './change-view.js';
  import { buildCoverageView, evidenceEntries } from './coverage-view.js';
  import { buildPacketView } from './packet-view.js';
  export { deriveChangeModel, packetMarkdown, packetHref, packetIdFromHash,
    buildChangeView, buildCoverageView, evidenceEntries, buildPacketView };
  export const renderChange = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderPacket = props => renderToStaticMarkup(React.createElement(PacketPage, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'doors.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

/* ── the agreement fixture, plus a decision history ───────────────────── */

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

const DECIDED = {
  ...CHANGE,
  decisions: {
    target_revision: 'e71c5b0aaaa',
    standing: { act: 'approved', target_revision: 'e71c5b0aaaa', actor: 'sam', decided_at: '2026-09-22T11:00:00Z', scope_version: 1 },
    history: [{ act: 'approved', target_revision: 'e71c5b0aaaa', actor: 'sam', decided_at: '2026-09-22T11:00:00Z', scope_version: 1 }],
  },
}

const COVERAGE = {
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
  /* T-3-5 — the readiness block the engine serves for this record: one
   * criterion, backed, on the version the Change is bound to. */
  readiness: {
    schema_version: 1,
    ready: true,
    scope_version: 1,
    target_version: 'e71c5b0aaaa',
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
  /* Live data (T-S-5 verifier finding 2): the candidate list carries the
   * engine's own `observation` rows beside the one real check. */
  candidates: [
    { kind: 'check_run', evidence_id: 'e2', session_id: 's1', name: 'cargo clippy', observed: 'proven', bears: false },
    { kind: 'observation', evidence_id: 'o1', session_id: 's1', name: 'COMMAND_STARTED', observed: 'recorded', bears: false },
    { kind: 'observation', evidence_id: 'o2', session_id: 's1', name: 'COMMAND_FINISHED', observed: 'recorded', bears: false },
    { kind: 'observation', evidence_id: 'o3', session_id: 's1', name: 'EVIDENCE_BOUND', observed: 'recorded', bears: false },
  ],
}

/* T-F-1-F1 — the served item's OWN shape, not a convenient one. There is no
 * `version` field on `GET /api/changes/<id>/applicability`: the version is
 * the one the evidence's snapshot recorded, and the paths the rule compared
 * are `checked_paths`. This fixture carried a `version` key and no paths,
 * which is how the packet's golden was checked in reading "Still applies —
 * this path unchanged since e71c5b0." for a whole release. */
const APPLICABILITY = {
  items: [{
    link_id: 'l1',
    criterion_index: 1,
    old_snapshot: 'e71c5b0aaaa',
    checked_paths: ['checkout/'],
  }],
}

function surfaces(change) {
  const hq = M.deriveChangeModel(change, COVERAGE, APPLICABILITY, null, [])
  const model = M.buildChangeView({ change, proposals: null, work: [], evidence: M.evidenceEntries(COVERAGE) })
  model.coverage = M.buildCoverageView(COVERAGE)
  model.examined = null
  model.hq = hq
  /* `hq` goes IN, as `readPacket` passes it, because `sectionsOf` reads it:
   * setting it afterwards leaves the decision section deciding it is empty. */
  const packet = M.buildPacketView({ change, proposals: null, work: [], coverage: COVERAGE, evidence: [], hq })
  return {
    hq,
    change: M.renderChange({ model, stage: 'verify', onGo: () => {}, onCopy: () => {}, notes: [], onReload: () => {} }),
    packet: M.renderPacket({ model: packet, notes: [], onCopy: () => {} }),
  }
}

/* ── (0) the screen, first ────────────────────────────────────────────── */

test('(0) the door in the top bar reaches the packet, one click from the Change screen', () => {
  const dom = surfaces(CHANGE).change
  const bar = dom.slice(dom.indexOf('ws-change-topbar'), dom.indexOf('ws-change-title'))
  assert.ok(bar.includes('ws-change-packet-door'), 'the top bar carries the door')
  assert.ok(bar.includes('The whole story'), 'the door is labelled in Sam’s words')
  assert.ok(bar.includes('href="#/changes/ch1/packet"'), 'the door points at the packet route')
  /* Above the fold: before the panel, not below it. */
  assert.ok(dom.indexOf('ws-change-packet-door') < dom.indexOf('ws-change-panel'), 'the door is above the panel')
})

test('(0) the door on the Decision state reaches the same packet', () => {
  const dom = surfaces(CHANGE).change
  assert.ok(dom.includes('ws-decision'), 'the Decision block renders on a proven Change')
  const decision = dom.slice(dom.indexOf('ws-decision-not-an-action'))
  assert.ok(decision.includes('ws-decision-packet'), 'the Decision block carries the door')
  assert.ok(decision.includes('Read the whole story on one page'))
  assert.ok(decision.includes('href="#/changes/ch1/packet"'), 'the same route, from one helper')
})

test('(0) the route both doors name is the route the packet answers to', () => {
  assert.equal(M.packetHref('ch1'), '#/changes/ch1/packet')
  assert.deepEqual(M.packetIdFromHash(M.packetHref('ch1')), { id: 'ch1' })
})

test('(0) the packet page offers Copy as Markdown, and says what it does and does not do', () => {
  const dom = surfaces(CHANGE).packet
  assert.ok(dom.includes('Copy as Markdown'), 'the control is on the page')
  assert.ok(dom.includes('Nothing is written to a file, and nothing leaves this Mac.'))
  assert.doesNotMatch(dom, /Save as file/, 'no export to disk (T-S-5 forbidden)')
})

test('(0) what the button copies carries every criterion line, run version and decision act the page shows', () => {
  const s = surfaces(DECIDED)
  const md = M.packetMarkdown(s.hq)
  /* The criterion line, word for word as the screen renders it. */
  const line = s.hq.criteria[0].line
  assert.ok(s.packet.includes(line), 'the page shows the line')
  assert.ok(md.includes(line), 'the copied text carries the same line')
  /* The run, with the version it ran on. */
  assert.ok(md.includes('tests pass'), 'the run is named')
  assert.ok(md.includes('version e71c5b0'), 'the run names its version')
  /* The decision act. */
  const act = s.hq.packet.decisionHistory[0].line
  assert.ok(s.packet.includes(act), 'the page shows the act')
  assert.ok(md.includes(act), 'the copied text carries the same act')
  assert.ok(md.includes(s.hq.decision.headline), 'and the standing decision')
})

/* ── (1) the snapshot ─────────────────────────────────────────────────── */

test('(1) the Markdown is what it was, byte for byte, on the agreement fixture', () => {
  const md = M.packetMarkdown(surfaces(DECIDED).hq)
  if (process.env.UPDATE_GOLDEN === '1') writeFileSync(GOLDEN, md)
  assert.equal(md, readFileSync(GOLDEN, 'utf8'), 'packet-markdown.golden.md differs; UPDATE_GOLDEN=1 to accept a wording change')
})

test('(1) it is pure: the same model twice is the same bytes', () => {
  const hq = surfaces(DECIDED).hq
  assert.equal(M.packetMarkdown(hq), M.packetMarkdown(hq))
  assert.equal(M.packetMarkdown(null), '')
})

/* ── (2) the fence: nothing in the text that is not in the model ──────── */

/** Every string and number reachable in `hq.packet`, as one haystack. */
function typedFields(node, out = []) {
  if (node === null || node === undefined) return out
  if (Array.isArray(node)) { for (const v of node) typedFields(v, out); return out }
  if (typeof node === 'object') { for (const v of Object.values(node)) typedFields(v, out); return out }
  if (typeof node === 'string' || typeof node === 'number') out.push(String(node))
  return out
}

test('(2) no digit sequence in the copied text that is not a typed field of the model', () => {
  const hq = surfaces(DECIDED).hq
  const md = M.packetMarkdown(hq)
  const haystack = typedFields(hq.packet).join(' ')
  const stray = [...md.matchAll(/\d+/g)].map((m) => m[0]).filter((d) => !haystack.includes(d))
  assert.deepEqual([...new Set(stray)], [], 'a number in the taken-away copy came from somewhere other than the model')
})

test('(2) no state word in the copied text that is not a typed field of the model', () => {
  const hq = surfaces(DECIDED).hq
  const md = M.packetMarkdown(hq)
  const haystack = typedFields(hq.packet).join(' ')
  for (const word of STATE_WORD_SET) {
    if (md.includes(word)) assert.ok(haystack.includes(word), `the copied text says "${word}" and the model does not`)
  }
})

test('(2) and the fence is live: a number smuggled past the model is caught', () => {
  const hq = surfaces(DECIDED).hq
  const haystack = typedFields(hq.packet).join(' ')
  const smuggled = M.packetMarkdown(hq) + '\n4 of 4 checks passed on build 99417.\n'
  const stray = [...smuggled.matchAll(/\d+/g)].map((m) => m[0]).filter((d) => !haystack.includes(d))
  assert.ok(stray.includes('99417'), 'the sweep would miss a number written into the builder')
})

/* ── (3) the sections the design record asks for ──────────────────────── */

test('(3) every §3.6 section is present, in order', () => {
  const md = M.packetMarkdown(surfaces(DECIDED).hq)
  const order = [
    'What counts as done, and what proves it',
    'What the agent said, against what the record shows',
    'Every run, and the version it ran on',
    'What was found, and what was done about it',
    'Decision history',
    'What was not recorded',
  ]
  let at = md.indexOf('# Totals add up on the checkout page')
  assert.ok(at === 0, 'the headline is first')
  for (const heading of order) {
    const found = md.indexOf('## ' + heading)
    assert.ok(found > at, `${heading} is out of order or missing`)
    at = found
  }
  /* The headline carries its scope and its version. */
  assert.ok(md.includes('Covers: checkout/'), 'the headline names the scope')
  assert.ok(md.includes('e71c5b0'), 'the headline names the version')
})

test('(3) an empty section says so in words, and is never a blank or a tick', () => {
  const md = M.packetMarkdown(surfaces(CHANGE).hq)
  assert.ok(md.includes('The record holds nothing the agent said about this Change.'))
  assert.ok(md.includes('Nothing was found in the work linked to this Change.'))
  assert.ok(md.includes('No decision yet.'), 'no decision says no decision')
})

/* ── (4) one story: the page and the copied text carry the same rows ──── */

/** `renderToStaticMarkup` escapes; the assertions compare what a person reads. */
const plain = (html) => html
  .replace(/<[^>]+>/g, '\u0000')
  .replace(/&quot;/g, '"').replace(/&#x27;/g, "'").replace(/&#39;/g, "'")
  .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')

test('(4) every §3.6 section the copied text carries is on the page, with the same title and the same rows', () => {
  const s = surfaces(DECIDED)
  const page = plain(s.packet)
  const md = M.packetMarkdown(s.hq)
  const sections = s.hq.packet.sections
  assert.equal(sections.length, 6, 'six body sections, and no seventh')
  for (const section of sections) {
    assert.ok(page.includes(section.title), `the page is missing the section "${section.title}"`)
    assert.ok(md.includes('## ' + section.title), `the copied text is missing "${section.title}"`)
    if (!section.rows.length) {
      assert.ok(page.includes(section.none), `the page hides what "${section.title}" says when empty`)
      assert.ok(md.includes(section.none), `the copied text hides it`)
      continue
    }
    for (const row of section.rows) {
      /* page ⊇ markdown: the direction the verifier found missing. */
      assert.ok(page.includes(row.text), `the page never shows: ${row.text}`)
      assert.ok(md.includes(row.text), `the copied text never shows: ${row.text}`)
      for (const sub of row.lines) {
        assert.ok(page.includes(sub), `the page never shows: ${sub}`)
        assert.ok(md.includes(sub), `the copied text never shows: ${sub}`)
      }
    }
  }
  /* And the two the verifier found only in the clipboard are on the page. */
  assert.ok(page.includes('Every run, and the version it ran on'))
  assert.ok(page.includes('What was not recorded'))
  /* One heading, not two, for the decision history. */
  assert.equal(page.split('Decision history').length - 1, 1, 'the decision history is headed once')
})

/* ── (5) no engine constant reaches a person ──────────────────────────── */

const ENGINE_CONSTANTS = ['COMMAND_STARTED', 'COMMAND_FINISHED', 'EVIDENCE_BOUND']

test('(5) an observation row is never a run, on the page or in the copied text', () => {
  const s = surfaces(DECIDED)
  const md = M.packetMarkdown(s.hq)
  const page = plain(s.packet)
  for (const token of ENGINE_CONSTANTS) {
    assert.ok(!md.includes(token), `the copied text leaks the engine constant ${token}`)
    assert.ok(!page.includes(token), `the page leaks the engine constant ${token}`)
  }
  /* The real check IS there: the filter keeps runs, it does not hide them. */
  const runs = s.hq.packet.runs.map((r) => r.name)
  assert.ok(runs.includes('cargo clippy'), 'an unlinked real run is still a run')
  assert.ok(runs.includes('tests pass'), 'a linked run is still a run')
  assert.equal(runs.length, 2, 'two runs, and the three observations are not among them')
})

/* ── (6) P6-DESIGN-FIXES-13, design record §8 item 3 ───────────────────── */

/* One of the two sentences, never both: "nothing counts as done yet" is about
 * what was ACCEPTED, and "No test ran for this." is about what was RUN. The
 * prototype's packet said both at once — four statements listed as having no
 * evidence, under a page that then said nothing was accepted. */

const NOTHING_ACCEPTED = 'Nothing counts as done yet, so there is nothing on this page for the work to be judged against.'
const NO_EVIDENCE = 'No test ran for this.'

/** The same two surfaces, over documents this file's fixtures choose. */
function surfacesWith({ change, coverage, proposals }) {
  const hq = M.deriveChangeModel(change, coverage || null, APPLICABILITY, proposals || null, [])
  const packet = M.buildPacketView({ change, proposals: proposals || null, work: [], coverage: coverage || null, evidence: [], hq })
  return { hq, md: M.packetMarkdown(hq), page: plain(M.renderPacket({ model: packet, notes: [], onCopy: () => {} })) }
}

/** Proposals on the record, and nobody has accepted any of them. */
const PROPOSED_ONLY = {
  id: 'ch2',
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Totals add up on the checkout page' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  associations: [],
  examination: 'none_yet',
}

const PROPOSALS = { criteria: [{ text: 'The totals add up' }, { text: 'Nothing else on the page breaks' }] }

/** Accepted, and nothing has run against any of it. */
const NO_RUNS_COVERAGE = {
  scope_version: 1,
  link_rows: 0,
  execution_sessions: [],
  criteria: [{ index: 1, text: 'The totals add up', state: 'none', items: [] }],
  candidates: [],
}

test('(6) proposals present and nothing accepted: the page and the copied text say nothing counts as done yet, and list nothing as lacking evidence', () => {
  const s = surfacesWith({ change: PROPOSED_ONLY, coverage: null, proposals: PROPOSALS })
  assert.equal(s.hq.packet.acceptedCount, 0, 'the fixture accepts nothing')

  assert.ok(s.page.includes(NOTHING_ACCEPTED), 'the page says nothing counts as done yet')
  assert.ok(s.md.includes(NOTHING_ACCEPTED), 'and so does the copied text')

  /* And NOT the other one, on either surface. */
  assert.ok(!s.page.includes(NO_EVIDENCE), 'the page also claimed statements lack evidence')
  assert.ok(!s.md.includes(NO_EVIDENCE), 'the copied text also claimed statements lack evidence')

  /* The suggestions are still shown — as suggestions, not as statements that
   * failed to find evidence. */
  assert.ok(s.page.includes('Nothing else on the page breaks'), 'the suggestion is on the page')
})

test('(6) accepted with no runs: the statements are listed as still needing evidence, and nothing says nothing was accepted', () => {
  const s = surfacesWith({ change: CHANGE, coverage: NO_RUNS_COVERAGE, proposals: null })
  assert.equal(s.hq.packet.acceptedCount, 1, 'the fixture accepts one line')
  assert.equal(s.hq.criteria[0].state, 'in_progress', 'nothing has run against it')

  assert.ok(s.page.includes(NO_EVIDENCE), 'the page says no test ran for it')
  assert.ok(s.md.includes(NO_EVIDENCE), 'and so does the copied text')
  assert.ok(s.page.includes('The totals add up'), 'the statement itself is listed')

  /* And NOT the other one, on either surface. */
  assert.doesNotMatch(s.page, /[Nn]othing counts as done yet/, 'the page also said nothing was accepted')
  assert.doesNotMatch(s.md, /[Nn]othing counts as done yet/, 'the copied text also said nothing was accepted')
})

test('(5) and the sweep is live: an observation renamed a run would be caught', () => {
  const leaky = { ...COVERAGE, candidates: COVERAGE.candidates.map((c) => ({ ...c, kind: 'check_run' })) }
  const hq = M.deriveChangeModel(DECIDED, leaky, APPLICABILITY, null, [])
  const md = M.packetMarkdown(hq)
  assert.ok(ENGINE_CONSTANTS.some((t) => md.includes(t)), 'the sweep would miss a leak')
})
