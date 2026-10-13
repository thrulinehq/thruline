import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* T-2-5 — the whole story on one page, rendered.
 *
 * The same harness the Change screen uses: the surface is bundled and rendered
 * to static markup, and every assertion below is about the STRINGS AND
 * CONTROLS A PERSON MEETS. A second bundle is built over a PROBE copy — the
 * real `HQ_COPY` with two packet strings replaced — because equality with the
 * real copy cannot tell a string that came from the copy file from a literal
 * that happens to match it.
 *
 * The fixture Change is synthetic. No string below came off a real machine.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-packet-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { PacketPage, packetIdFromHash } from './packet.jsx';
  import { ChangeScreen } from './change-workspace.jsx';
  import { buildPacketView, SECTIONS } from './packet-view.js';
  import { buildChangeView } from './change-view.js';
  import { HQ_COPY } from './hq-copy.js';
  import { chooseScreen, screenKey, watchAddress } from '../routes.js';
  import { absenceWithoutItsScope } from './change-view.js';
  import { deriveChangeModel } from './change-model.js';
  export { deriveChangeModel };
  export { buildPacketView, buildChangeView, SECTIONS, packetIdFromHash, chooseScreen, screenKey, watchAddress, absenceWithoutItsScope, HQ_COPY };
  export const renderPage = props => renderToStaticMarkup(React.createElement(PacketPage, props));
  export const renderChange = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
`

const COMMON = {
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
}

async function load(name, plugins) {
  const result = await build({ ...COMMON, stdin: { contents: ENTRY, resolveDir: here }, plugins })
  const file = join(temporary, name)
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}

const M = await load('packet.mjs')

const COPY_FILE = join(here, 'hq-copy.js')
const PROBED = await load('packet-probe.mjs', [{
  name: 'probe-copy',
  setup(b) {
    b.onLoad({ filter: /hq-copy\.js$/ }, () => ({
      contents: readFileSync(COPY_FILE, 'utf8')
        .replace("empty: 'Nothing has been waved through. No exception is written down for this Change.'", "empty: 'probe-exceptions-empty'")
        .replace("nothing_for_it: 'No test ran for this.'", "nothing_for_it: 'probe-nothing-ran'"),
      loader: 'js',
    }))
  },
}])

const { buildPacketView, buildChangeView, SECTIONS, HQ_COPY, renderPage, renderChange, packetIdFromHash, chooseScreen, screenKey, watchAddress, absenceWithoutItsScope } = M
const P = HQ_COPY.change.packet

const SESSION_A = 'cccccccc-0000-4000-8000-00000000s001'
const SESSION_B = 'cccccccc-0000-4000-8000-00000000s002'

function aChange(over = {}) {
  return {
    id: 'cccccccc-0000-4000-8000-00000000c001',
    project_id: 'cccccccc-0000-4000-8000-00000000p001',
    opened_at: '2026-01-02T09:00:00Z',
    request: { title: 'Retry failed uploads', purpose: 'Uploads that fail once are lost, and the person never finds out.', requester: 'sam', source: 'person' },
    accepted_scope: null,
    implementation: null,
    associations: [],
    examination: 'none_yet',
    decision: 'none_yet',
    masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
    ...over,
  }
}

const FULL = aChange({
  accepted_scope: {
    version: 1,
    criteria: ['A failed upload is tried again three times.', 'The person is told when it finally gives up.'],
    exclusions: ['Uploads bigger than 2 GB.'], allowed_scope: [], constraints: [], check_profile: ['cargo-test'],
    content_sha256: '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
    acceptance: { actor: 'sam', accepted_at: '2026-01-02T09:06:00Z', rationale: 'That is what I asked for.' },
    proposed_at: '2026-01-02T09:05:00Z',
  },
  implementation: {
    version: 1, bound_at: '2026-01-02T10:00:00Z', repository_identity: 'repo-synthetic-0001',
    base_revision: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0', snapshot_id: null, workspace_identity: 'workspace-synthetic-0001',
  },
  associations: [
    { id: 'a1', session_id: SESSION_A, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-01-02T10:01:00Z', reason: null },
    { id: 'a2', session_id: SESSION_B, provenance: 'suggested', basis: 'timing', actor: null, asserted_at: '2026-01-02T10:02:00Z', reason: null },
  ],
})

/** One finding card in the shape `verdict-model.buildVerdict` produces. */
const CARD = {
  id: 'clm_1',
  state: 'not_proven',
  kind: 'said_done_no_proof',
  seq: 44,
  said: { parts: [{ text: 'The AI said this was done at step 44: ' }, { quote: 'All tests pass.', speaker: 'agent' }], refs: ['r-44'] },
  record: { text: 'Not proven: nothing in the record shows this was done.', role: 'record', refs: ['r-44'] },
  do: { text: 'Ask the AI for proof: the output of a check, not a sentence.', role: 'do', refs: [], prompt: 'Run the check that shows "All tests pass." on the current code and paste the full output, including the command you ran.' },
  detector: null,
  proofNode: null,
}

const WORK = [{ id: SESSION_A, findings: [CARD], steps: 41 }]

const modelWith = (over = {}) => buildPacketView({ change: FULL, work: WORK, build: ['3 files changed in the upload path'], ...over })
const bare = () => buildPacketView({ change: aChange({ request: { title: '', purpose: '', requester: '', source: 'person' } }), work: [] })

const text = (html) => html.replace(/<[^>]*>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/&#x2F;/g, '/').replace(/\s+/g, ' ').trim()
const headings = (html) => [...html.matchAll(/<h2[^>]*>(.*?)<\/h2>/g)].map((m) => text(m[1]))
/** The Plain surface: everything before the technical layer opens. */
const plainOf = (html) => text(html.slice(0, html.indexOf('ws-change-technical')))

/* ── the sections, in the brief's order ────────────────────────────────── */

test('the page renders the twelve sections in the brief’s order, under the still-needs-evidence list', () => {
  const html = renderPage({ model: modelWith() })
  const found = headings(html)
  assert.deepEqual(found, [
    P.needs.heading,
    ...SECTIONS.map((id) => P.sections[id].heading),
  ], 'the sections are not the brief’s twelve, in order, under what to read first')
})

test('the twelve sections render in that order on an empty Change too: an order is not a property of a full record', () => {
  assert.deepEqual(headings(renderPage({ model: bare() })), [P.needs.no_criteria_heading, ...SECTIONS.map((id) => P.sections[id].heading)])
})

test('every section with nothing in it renders its own sentence from the copy file', () => {
  const html = renderPage({ model: bare() })
  const body = text(html)
  for (const id of SECTIONS) {
    assert.ok(body.includes(P.sections[id].empty), `section ${id} renders no sentence for holding nothing; expected: ${P.sections[id].empty}`)
  }
})

test('one test per section: each empty sentence is inside its own section and no other', () => {
  const html = renderPage({ model: bare() })
  for (const id of SECTIONS) {
    const start = html.indexOf('id="ws-packet-' + id + '"')
    assert.ok(start > 0, `section ${id} is not on the page`)
    const end = html.indexOf('<section', start + 1)
    const section = text(html.slice(start, end === -1 ? html.indexOf('ws-change-notes') === -1 ? html.length : html.indexOf('ws-change-notes') : end))
    assert.ok(section.includes(P.sections[id].heading), `section ${id} lost its heading`)
    assert.ok(section.includes(P.sections[id].empty), `section ${id} does not hold its own empty sentence`)
  }
})

test('the six sections nothing in this build records still say so on a FULL Change: never blank, never a tick', () => {
  const body = text(renderPage({ model: modelWith() }))
  for (const id of ['review', 'dispositions', 'repairs', 'freshness', 'exceptions', 'decisions']) {
    assert.ok(body.includes(P.sections[id].empty), `section ${id} on a full Change says nothing at all`)
  }
  assert.ok(!/✓|✔|&#x2713;/.test(renderPage({ model: modelWith() })), 'a tick reached the page')
})

test('the empty sentences come from the copy file, not from a literal in the view', () => {
  const html = PROBED.renderPage({ model: PROBED.buildPacketView({ change: aChange({ request: { title: '', purpose: '', requester: '', source: 'person' } }), work: [] }) })
  assert.ok(html.includes('probe-exceptions-empty'), 'the exceptions empty state is a literal in the view')
})

/* REVIEW-EMPTY-TRUE (QA P0-2, D-331) — with no review recorded, the record
 * says only that none was written down. Whether a person read the Change or a
 * tool reviewed it outside ThruLine is unknown, so the section must not say
 * nobody did. */
test('a Change with no recorded review says only that ThruLine has no record of one, never that nobody read it', () => {
  const html = renderPage({ model: bare() })
  const start = html.indexOf('id="ws-packet-review"')
  assert.ok(start > 0, 'the review section is not on the page')
  const end = html.indexOf('<section', start + 1)
  const section = text(html.slice(start, end === -1 ? html.length : end))
  assert.ok(section.includes('No review has been written down for this Change. ThruLine has no record of anyone else reading it or of a tool reviewing it.'),
    'the review section does not say what the record holds: no review written down, and no record of anyone reading it')
  assert.ok(!section.includes('Nobody else has read it'), 'the review section claims nobody read the Change; the record cannot say that')
  assert.ok(!section.includes('no tool has reviewed it'), 'the review section claims no tool reviewed the Change; the record cannot say that')
})

/* ── the criterion-to-evidence mapping ─────────────────────────────────── */

test('a thing that counts as done with nothing behind it renders "No test ran for this."', () => {
  const html = renderPage({ model: modelWith() })
  const start = html.indexOf('id="ws-packet-mapping"')
  const section = text(html.slice(start, html.indexOf('id="ws-packet-examination"')))
  assert.ok(section.includes('A failed upload is tried again three times.'))
  assert.equal([...section.matchAll(/No test ran for this\./g)].length, 2, 'both things that count as done should say nothing ran')
  for (const claim of ['passed', 'satisfied', 'met', 'done ✓']) {
    assert.ok(!section.includes(claim), `the mapping reads a criterion with nothing behind it as ${claim}`)
  }
})

test('that sentence comes from the copy file', () => {
  const html = PROBED.renderPage({ model: PROBED.buildPacketView({ change: FULL, work: [] }) })
  assert.ok(html.includes('probe-nothing-ran'), '"No test ran for this." is a literal in the view')
})

test('a thing WITH evidence lists it, with its observed result, instead of the nothing-ran line', () => {
  const model = modelWith({
    evidence: [{ criterion: 1, provenance: 'captured', session: SESSION_A, seq: 7, state: 'proven', summary: 'cargo test --workspace exited 0' }],
  })
  const html = renderPage({ model })
  const start = html.indexOf('id="ws-packet-mapping"')
  const section = text(html.slice(start, html.indexOf('id="ws-packet-examination"')))
  assert.ok(section.includes('cargo test --workspace exited 0'))
  assert.ok(section.includes('Proven'), 'the evidence item lost its observed result')
  assert.equal([...section.matchAll(/No test ran for this\./g)].length, 1, 'only the second thing has nothing behind it')
})

/* ── every evidence item: labelled and addressed ───────────────────────── */

test('every evidence item on the page carries a provenance label and a link to its record', () => {
  const model = modelWith({
    evidence: [{ criterion: 1, provenance: 'captured', session: SESSION_A, seq: 7, state: 'proven', summary: 'cargo test --workspace exited 0' }],
  })
  const html = renderPage({ model })
  const items = [...html.matchAll(/<div class="ws-packet-item">([\s\S]*?)<\/p><\/div>/g)].map((m) => m[0])
  assert.ok(items.length >= 2, `expected the mapped evidence and the finding, found ${items.length}`)
  for (const item of items) {
    assert.match(item, /ws-packet-provenance-word">([^<]+)</, 'an evidence item has no word for where it came from')
    assert.match(item, /<a class="ws-packet-record" href="\/sessions\?session=[^"]+#\/trace">Show the record<\/a>/, 'an evidence item has no link to its record')
    assert.match(item, /ws-packet-where">(step \d+ of the work you linked|the work you linked)</, 'an evidence item does not say where in the record it is')
  }
  assert.ok(text(html).includes(P.provenance.captured))
})

test('an item the record cannot place is refused on the page, and the page says how many', () => {
  const one = renderPage({ model: modelWith({ evidence: [{ criterion: 1, session: SESSION_A, seq: 7 }] }) })
  assert.ok(text(one).includes(P.record.refused_one), 'an unlabelled item vanished in silence')
  const two = renderPage({ model: modelWith({ evidence: [{ criterion: 1, session: SESSION_A }, { criterion: 2, provenance: 'vibes', session: SESSION_A }] }) })
  assert.ok(text(two).includes('2 things that would be evidence here are not shown'))
  /* And it is not rendered anyway, somewhere else on the page. */
  assert.ok(!one.includes('ws-packet-provenance-word">undefined'))
})

test('no refusal line is shown when there is nothing to refuse', () => {
  assert.ok(!renderPage({ model: modelWith() }).includes('ws-packet-refused'))
})

/* ── accepted, and only-suggested, are never the same sentence ─────────── */

/* The suggestions route re-derives its list from the request text on every
 * read, so a Change whose scope was accepted off that list comes back with
 * the accepted lines sitting in the suggestions document too. Rendering both
 * blocks put the same sentence on the page twice, four lines apart: once as
 * "Accepted at version 1 by sam", once under "Suggested, and not accepted —
 * nothing counts until somebody accepts it". Found by the T-2-5 verifier on a
 * real Change through the real server; the fixtures here reproduce that exact
 * document pair. */

const acceptedSection = (html) => text(html.slice(html.indexOf('id="ws-packet-accepted"'), html.indexOf('id="ws-packet-target"')))
const ECHOED = { criteria: FULL.accepted_scope.criteria.map((t) => ({ text: t })) }

test('a line that is accepted is never also shown as not accepted', () => {
  const html = renderPage({ model: modelWith({ proposals: ECHOED }) })
  const section = acceptedSection(html)
  assert.ok(section.includes('Accepted as what counts as done (list 1) by sam on 2026-01-02.'), section.slice(0, 300))
  assert.ok(!section.includes(P.accepted.proposed_heading), `an accepted line is under "${P.accepted.proposed_heading}"`)
  assert.ok(!section.includes(P.accepted.proposed_note))
  assert.ok(!section.includes(P.accepted.unaccepted))
  for (const line of FULL.accepted_scope.criteria) {
    assert.equal([...section.matchAll(new RegExp(line.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g'))].length, 1,
      `"${line}" is on the page twice, under two opposite labels`)
  }
})

test('a suggestion nobody accepted is still shown, still labelled, beside the accepted ones', () => {
  const proposals = { criteria: [...ECHOED.criteria, { text: 'The retry interval is written down.' }] }
  const section = acceptedSection(renderPage({ model: modelWith({ proposals }) }))
  assert.ok(section.includes(P.accepted.proposed_heading), 'the only unaccepted suggestion vanished')
  assert.ok(section.includes(P.accepted.proposed_note))
  assert.ok(section.includes('The retry interval is written down.'))
  const after = section.slice(section.indexOf(P.accepted.proposed_heading))
  for (const line of FULL.accepted_scope.criteria) {
    assert.ok(!after.includes(line), `the accepted line "${line}" is under the suggestions heading`)
  }
})

test('with nothing accepted, every suggestion is shown and labelled as accepted by nobody', () => {
  const model = buildPacketView({ change: aChange(), proposals: ECHOED, work: [] })
  const section = acceptedSection(renderPage({ model }))
  assert.ok(section.includes(P.accepted.proposed_heading))
  assert.ok(section.includes(P.accepted.unaccepted), 'the suggestions are not said to be accepted by nobody')
  for (const line of FULL.accepted_scope.criteria) assert.ok(section.includes(line), `the suggestion "${line}" is not shown`)
  assert.ok(!section.includes('Accepted as what counts as done (list'))
})

test('that guard is a live instrument: the two blocks are about different lines, not about which block ran', () => {
  const echoed = renderPage({ model: modelWith({ proposals: ECHOED }) })
  const extra = renderPage({ model: modelWith({ proposals: { criteria: [{ text: 'The retry interval is written down.' }] } }) })
  assert.ok(!acceptedSection(echoed).includes(P.accepted.proposed_heading))
  assert.ok(acceptedSection(extra).includes(P.accepted.proposed_heading), 'the heading never renders at all, so its absence proves nothing')
})

/* ── what to read first ────────────────────────────────────────────────── */

test('the page opens with the conclusion sentence, the version, and what still needs evidence — in that order', () => {
  const html = renderPage({ model: modelWith() })
  const order = ['ws-change-kicker', 'ws-change-title', 'ws-change-conclusion', 'ws-change-version', 'ws-packet-needs', 'ws-packet-section']
  let at = -1
  for (const cls of order) {
    const next = html.indexOf(cls)
    assert.ok(next > at, `${cls} is out of reading order`)
    at = next
  }
  const top = text(html.slice(0, html.indexOf('ws-packet-section')))
  assert.ok(top.includes('Accepted: 2 things count as done. None has evidence yet.'))
  assert.ok(top.includes('version a1b2c3d · 2026-01-02'))
  assert.ok(top.includes('A failed upload is tried again three times.'), 'the still-needs-evidence list is not at the top')
  assert.ok(top.includes('No test ran for this.'))
})

test('a Change with nothing accepted says why nothing can need evidence, instead of an empty list', () => {
  const top = text(renderPage({ model: bare() }))
  assert.ok(top.includes(P.needs.no_criteria))
})

/* ── the rules that hold for the whole page ────────────────────────────── */

test('no heading anywhere on the page is a state word', () => {
  const html = renderPage({ model: modelWith({ evidence: [{ criterion: 1, provenance: 'captured', session: SESSION_A, seq: 7, state: 'not_proven' }] }) })
  const all = [...html.matchAll(/<h([123])[^>]*>(.*?)<\/h\1>/g)].map((m) => text(m[2]))
  assert.ok(all.length >= 14, `expected every heading on the page, found ${all.length}`)
  for (const word of Object.values(HQ_COPY.states).map((s) => s.word)) {
    for (const heading of all) assert.ok(!heading.includes(word), `the heading "${heading}" renders the state word "${word}"`)
  }
})

test('the five state words appear only as one piece of evidence’s observed result', () => {
  const html = renderPage({ model: modelWith({ evidence: [{ criterion: 1, provenance: 'captured', session: SESSION_A, seq: 7, state: 'proven' }] }) })
  /* "Proven" is on the page — on an evidence item, which is the only place a
     state word belongs. It is never the Change-level answer: not the one
     conclusion sentence, not the version line, not any heading
     (`03-USER-LENS.md` rule 3). The read-first list carries the record's own
     sentence about one finding, which is an evidence item like any other. */
  const one = (cls) => { const m = new RegExp('class="' + cls + '"[^>]*>(.*?)</p>').exec(html); return m ? text(m[1]) : '' }
  for (const word of Object.values(HQ_COPY.states).map((s) => s.word)) {
    assert.ok(!one('ws-change-conclusion').includes(word), `the conclusion sentence renders the state word "${word}"`)
    assert.ok(!one('ws-change-version').includes(word), `the version line renders the state word "${word}"`)
  }
  assert.ok(html.includes('ws-packet-state'), 'the evidence item lost its observed result')
})

test('what the record holds against what was said is read FIRST, not forty lines down', () => {
  const html = renderPage({ model: modelWith() })
  const top = html.slice(0, html.indexOf('ws-packet-section'))
  assert.ok(text(top).includes(P.needs.wrong_heading), 'the top says nothing about what is actually wrong')
  assert.ok(text(top).includes('Not proven: nothing in the record shows this was done.'), 'the finding is not in the read-first list')
  /* And it is still there in full, in its own section: the list is a pointer,
     not a second source of truth. */
  assert.ok(text(html.slice(html.indexOf('id="ws-packet-examination"'))).includes('Not proven: nothing in the record shows this was done.'))
})

test('a gap in the recording is not "what is wrong": only the record holding the opposite, or a loose thread, is', () => {
  const couldnt = { ...CARD, id: 'clm_2', state: 'couldnt', record: { text: "Recorded: not enough was recorded.", role: 'record', refs: [] } }
  const model = buildPacketView({ change: FULL, work: [{ id: SESSION_A, findings: [couldnt], steps: 4 }] })
  assert.equal(model.wrong.length, 0)
  const html = renderPage({ model })
  assert.ok(!text(html.slice(0, html.indexOf('ws-packet-section'))).includes(P.needs.wrong_heading), 'a recording gap was listed as something wrong')
})

test('no percentage anywhere on the page, in any shape', () => {
  for (const model of [modelWith(), bare(), modelWith({ evidence: [{ criterion: 1, provenance: 'captured', session: SESSION_A, seq: 7, state: 'proven' }] })]) {
    const body = text(renderPage({ model }))
    assert.doesNotMatch(body, /\d+\s*%/, `a percentage reached the page: ${body.slice(0, 200)}`)
    assert.doesNotMatch(body, /\bper cent\b|\bpercent\b/i)
  }
})

test('nothing on the page is green, and nothing is an all-clear', () => {
  const body = text(renderPage({ model: modelWith() }))
  assert.doesNotMatch(body, /\bgreen\b|\ball clear\b|\beverything checks out\b|\bready to (ship|merge)\b|\blooks good\b/i)
})

test('no word off the banned list reaches the rendered Plain surface', () => {
  const banned = [/\bevents?\b/i, /\breceipts?\b/i, /\bcandidates?\b/i, /\bapplicability\b/i, /\bdispositions?\b/i, /\bpacket\b/i, /\bledger\b/i, /\bsession id\b/i, /\btree hash\b/i, /\bpredicates?\b/i]
  for (const model of [modelWith(), bare(), modelWith({ evidence: [{ criterion: 1, provenance: 'captured', session: SESSION_A, seq: 7, state: 'proven', summary: 'cargo test exited 0' }] })]) {
    const plain = plainOf(renderPage({ model }))
    for (const re of banned) assert.doesNotMatch(plain, re, `the page renders a banned word: ${plain.slice(0, 300)}`)
  }
})

test('the rendered banned-word sweep is a live instrument', () => {
  const banned = [/\bpacket\b/i, /\breceipts?\b/i]
  assert.ok(banned.some((re) => re.test('packet v3 · 2 receipts')))
})

/* ── what this page is made of (03-USER-LENS.md rule 13) ───────────────── */

test('the footer says what this Change’s record is made of, in words and one real number', () => {
  const body = text(renderPage({ model: modelWith() }))
  assert.ok(body.includes('Built from 1 piece of linked work · 41 recorded steps.'), body.slice(-300))
  assert.ok(body.includes(P.footer.lead))
})

test('the footer’s number is the typed field: change the field, the footer changes', () => {
  const a = text(renderPage({ model: buildPacketView({ change: FULL, work: [{ id: SESSION_A, findings: [], steps: 41 }] }) }))
  const b = text(renderPage({ model: buildPacketView({ change: FULL, work: [{ id: SESSION_A, findings: [], steps: 42 }] }) }))
  assert.ok(a.includes('41 recorded steps'))
  assert.ok(b.includes('42 recorded steps'))
  assert.ok(!b.includes('41 recorded steps'))
})

test('a Change with nothing linked says so rather than showing a zero', () => {
  const body = text(renderPage({ model: bare() }))
  assert.ok(body.includes(P.footer.none))
  assert.ok(!body.includes('0 recorded steps'))
})

/* ── controls ──────────────────────────────────────────────────────────── */

test('the page has exactly one primary action, and it is the way back to the Change', () => {
  for (const model of [modelWith(), bare()]) {
    const html = renderPage({ model })
    const primaries = [...html.matchAll(/class="ws-(?:change|verdict)-primary[^"]*"/g)]
    assert.equal(primaries.length, 1, `the page has ${primaries.length} primary actions`)
    assert.match(html, /class="ws-change-primary ws-packet-back" href="#\/changes\/[^"]+">Back to this Change</)
  }
})

test('every control that is off says why, and what would turn it on', () => {
  const html = renderPage({ model: modelWith() })
  const offs = [...html.matchAll(/<button[^>]*disabled[^>]*aria-describedby="([^"]+)"/g)]
  for (const m of offs) {
    const why = new RegExp(`id="${m[1]}"[^>]*>([^<]+)<`).exec(html)
    assert.ok(why, 'a disabled control points at a reason that is not on the page')
    assert.ok(why[1].trim().length > 60, `the reason says nothing: ${why && why[1]}`)
  }
  /* No disabled button without a reason, in any spelling. */
  assert.equal([...html.matchAll(/<button[^>]*disabled/g)].length, offs.length, 'a disabled control with no reason beside it')
  /* T-5-4: the control that was off ("Send this to somebody else", Phase 5)
   * is now the door to the file (asserted, with the one derived model it
   * needs, in export-sheet.test.js). Its old reason is gone because it is no
   * longer true. */
  assert.ok(!text(html).includes(P.send_blocked.reason), 'the page still says it cannot write a file')
})

test('the page reads and never writes: no POST, no write route, no export', () => {
  const source = readFileSync(join(here, 'packet.jsx'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
  for (const forbidden of ["method: 'POST'", 'method: "POST"', '/associations', '/scope/accept', 'child_process', 'eval(', 'writeFile', 'download']) {
    assert.ok(!source.includes(forbidden), `the page can write: ${forbidden}`)
  }
})

/* ── the address, and the way in ───────────────────────────────────────── */

test('the address names one Change and nothing else', () => {
  assert.deepEqual(packetIdFromHash('#/changes/abc-123/packet'), { id: 'abc-123' })
  for (const bad of ['', '#/changes/abc-123', '#/changes//packet', '#/changes/a/b/packet', '#/changes/../../etc/passwd/packet', '#/trace']) {
    assert.equal(packetIdFromHash(bad), null, `accepted a bad address: ${bad}`)
  }
})

test('the Change workspace carries the way in, as a secondary action', () => {
  const model = buildChangeView({ change: FULL, work: [] })
  const html = renderChange({ model, stage: 'define' })
  assert.match(html, /<a class="ws-change-secondary" href="#\/changes\/cccccccc-0000-4000-8000-00000000c001\/packet">Read the whole story on one page<\/a>/)
  /* Secondary: it does not compete with the stage's one primary action. */
  assert.equal([...html.matchAll(/class="ws-change-primary"/g)].length, 1)
})

/* ── the technical layer stays one click down ──────────────────────────── */

test('ids, digests and how a link was made are inside one details element at the bottom', () => {
  const html = renderPage({ model: modelWith() })
  const at = html.indexOf('ws-change-technical')
  assert.ok(at > 0, 'the technical layer is not on the page')
  assert.match(html.slice(at - 60, at + 40), /<details/)
  assert.doesNotMatch(html, /<details class="ws-change-technical"[^>]*\bopen\b/, 'the technical layer is open by default')
  assert.ok(!plainOf(html).includes('2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c'), 'a digest reached the Plain surface')
  assert.ok(!plainOf(html).includes('cccccccc-0000-4000-8000-00000000c001'), 'the Change id is in the words on the Plain surface')
  assert.ok(text(html.slice(at)).includes('cccccccc-0000-4000-8000-00000000c001'), 'the technical layer lost the id')
})

/* ── the click has to do what it looks like it does ────────────────────── */

const keyOf = (hash) => screenKey(chooseScreen({ pathname: '/', hash, search: '' }))

test('the two screens are different roots, and the one watcher knows which is which', () => {
  assert.notEqual(keyOf('#/changes/abc'), keyOf('#/changes/abc/packet'))
  assert.notEqual(keyOf('#/changes/abc/verify'), keyOf('#/changes/abc/packet'))
  assert.notEqual(keyOf('#/changes/abc/packet'), keyOf('#/inbox'))
  /* Moving between stages on the Change screen is not a move to another
     screen, and neither is landing on the address you are already at. */
  assert.equal(keyOf('#/changes/abc'), keyOf('#/changes/abc/verify'))
  assert.equal(keyOf('#/changes/abc/packet'), keyOf('#/changes/abc/packet'))
})

test('a real hash change from one screen to the other boots the other one', () => {
  const handlers = []
  let reloads = 0
  const fake = {
    location: { pathname: '/', hash: '#/changes/abc', search: '', reload() { reloads += 1 } },
    addEventListener: (type, fn) => { if (type === 'hashchange') handlers.push(fn) },
    removeEventListener: (type, fn) => { const i = handlers.indexOf(fn); if (i >= 0) handlers.splice(i, 1) },
  }
  const off = watchAddress(fake)
  assert.equal(handlers.length, 1, 'nothing is watching the address at all')
  /* Moving between stages must not boot anything. */
  fake.location.hash = '#/changes/abc/verify'
  handlers[0]()
  assert.equal(reloads, 0, 'a stage change booted the app')
  /* The link to the whole story on one page must. */
  fake.location.hash = '#/changes/abc/packet'
  handlers[0]()
  assert.equal(reloads, 1, 'the click changed the address and left the old screen on the glass')
  /* And so must Back to the Inbox — the pair no watcher covered, which is the
     defect T-2-8-F4 found on the real walk. */
  fake.location.hash = '#/inbox'
  handlers[0]()
  assert.equal(reloads, 2, 'Back to the Inbox left the Change on the glass')
  off()
  assert.equal(handlers.length, 0, 'the watcher outlives the screen')
})

test('that drive is a live instrument: with the watcher cut out, the click does nothing', async () => {
  const CUT = "win.addEventListener('hashchange', onHash)"
  let cut = 0
  const NOWATCH = await load('packet-nowatch.mjs', [{
    name: 'cut-the-watcher',
    setup(b) {
      b.onLoad({ filter: /routes\.js$/ }, () => {
        const source = readFileSync(join(here, '..', 'routes.js'), 'utf8')
        cut = source.split(CUT).length - 1
        return { contents: source.split(CUT).join(''), loader: 'js' }
      })
    },
  }])
  assert.equal(cut, 1, 'the control cut nothing out: the line it names is no longer in the source')
  const handlers = []
  let reloads = 0
  NOWATCH.watchAddress({
    location: { pathname: '/', hash: '#/changes/abc', search: '', reload() { reloads += 1 } },
    addEventListener: (type, fn) => { if (type === 'hashchange') handlers.push(fn) },
    removeEventListener: () => {},
  })
  assert.equal(handlers.length, 0, 'the cut did not reach the watcher')
  assert.equal(reloads, 0)
})

/* ── T-2-8-F5, on the page a person sends to somebody else ──────────────── */

test('the whole story on one page says nothing is wrong only where it says nothing was checked', () => {
  const twoConfirmed = aChange({
    accepted_scope: FULL.accepted_scope,
    implementation: FULL.implementation,
    associations: [
      { id: 'a1', session_id: SESSION_A, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-01-02T10:01:00Z', reason: null },
      { id: 'a2', session_id: SESSION_B, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-01-02T10:02:00Z', reason: null },
    ],
  })
  const model = buildPacketView({
    change: twoConfirmed,
    evidence: [],
    /* R2: the loader's shape carries whether each end was recorded
     * (`ended`) and how many claims were captured (`claims`); this fixture is
     * two recorded conversations holding none. */
    work: [{ id: SESSION_A, findings: [], steps: 41, ended: true, claims: 0 }, { id: SESSION_B, findings: [], steps: 12, ended: true, claims: 0 }],
  })
  assert.equal(model.conclusion.key, 'no_evidence', 'the fixture is not the zero-evidence Change this is about')
  const body = text(renderPage({ model }))
  assert.deepEqual(
    absenceWithoutItsScope(body), [],
    'a sentence on the page says nothing is wrong without saying nothing was checked',
  )
  assert.ok(
    body.includes(HQ_COPY.change.verify.nothing_wrong_unchecked_many.replace('{n}', '2')),
    'the scoped sentence is not on the page',
  )
  /* And the page still says, four times, that no test ran — the two facts now
     agree instead of contradicting each other in one field of view. */
  assert.ok(body.includes(HQ_COPY.change.packet.mapping.nothing_for_it))
})

/* ── STEP-7-LIMITS — the page states the record's limit, and one count ──── */

const withCheckRun = () => aChange({
  accepted_scope: FULL.accepted_scope,
  implementation: FULL.implementation,
  associations: [
    { id: 'a1', session_id: SESSION_A, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-01-02T10:01:00Z', reason: null },
    { id: 'a2', session_id: SESSION_B, provenance: 'confirmed', basis: 'check', actor: 'thruline', asserted_at: '2026-01-02T10:02:00Z', reason: null },
  ],
})

test('PW-F13 · one page gives one count: the nothing-wrong line and the footer count the same pieces of work read', () => {
  /* One conversation read, and one confirmed check-execution session that is
   * never read as work. The footer already counted 1; the sentence said 2.
   * SEAM-LOCATOR-R2: the recorded work carries the loader's shape, ended: true,
   * claims: 0, as ac252d78 gave every other recorded-work fixture here — work
   * without a recorded end is unrecorded work, which says so instead. */
  const model = buildPacketView({ change: withCheckRun(), evidence: [], work: [{ id: SESSION_A, findings: [], steps: 41, ended: true, claims: 0 }] })
  const body = text(renderPage({ model }))
  assert.equal(model.footer.work, 1)
  assert.ok(body.includes(HQ_COPY.change.verify.nothing_wrong_unchecked_one), body)
  assert.ok(!body.includes(HQ_COPY.change.verify.nothing_wrong_unchecked_many.replace('{n}', '2')), 'the check run is counted as a piece of work read')
})

test('STEP-7-LIMITS-R2 · the packet page gives one number for the work you linked, and the check run is not in it', () => {
  const model = buildPacketView({ change: withCheckRun(), evidence: [], work: [{ id: SESSION_A, findings: [], steps: 41 }] })
  const body = text(renderPage({ model }))
  const counts = [...body.matchAll(/(\d+) pieces? of (?:linked work|work you linked)/g)].map((m) => Number(m[1]))
  assert.ok(counts.length >= 2, `the page no longer counts linked work: ${body}`)
  assert.deepEqual([...new Set(counts)], [1], `the page counts linked work as ${counts.join(' and ')}`)
})

test('STEP-7-LIMITS · the page\'s "What changed" section states the limit from hq.codeLimit, and never reads as unchanged', () => {
  const change = withCheckRun()
  const work = [{ id: SESSION_A, findings: [], steps: 41 }]
  const hq = { ...M.deriveChangeModel(change, null, null, null, work), codeLimit: { lines: ['probe-code-limit'], text: 'probe-code-limit' } }
  const model = buildPacketView({ change, evidence: [], work, hq })
  const html = renderPage({ model })
  const section = /<section[^>]*id="ws-packet-changed"[^>]*>([\s\S]*?)<\/section>/.exec(html)
  assert.ok(section, 'no What changed section on the page')
  assert.ok(section[1].includes('probe-code-limit'), `the What changed section does not state the limit: ${text(section[1])}`)
})

/* ── STEP-7-GIT-LINE — the page's "What changed" carries the moved line ─── */

test('STEP-7-GIT-LINE · the page\'s "What changed" states the served move, with its path, from the one derivation', () => {
  const change = { ...withCheckRun(), code_moved: { recorded: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0', now: 'f0e1d2c3b4a5968778695a4b3c2d1e0f9a8b7c6d', changed_paths: ['src/lib.rs'], commits: 1, uncommitted_compared: false } }
  const work = [{ id: SESSION_A, findings: [], steps: 41 }]
  const hq = M.deriveChangeModel(change, null, null, null, work)
  assert.ok(hq.codeMoved, 'no moved line derived')
  const model = buildPacketView({ change, evidence: [], work, hq })
  const html = renderPage({ model })
  const section = /<section[^>]*id="ws-packet-changed"[^>]*>([\s\S]*?)<\/section>/.exec(html)
  assert.ok(section, 'no What changed section on the page')
  const body = text(section[1])
  assert.ok(body.includes(text(hq.codeMoved.sentence)), `the What changed section does not state the move: ${body}`)
  assert.ok(body.includes('src/lib.rs'))
})
