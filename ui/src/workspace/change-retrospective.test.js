import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* QUAL-F2-R2 (`Q7`, brief §16 row 7) — what counts as done, accepted after the
 * work it covers was already in the record, says so on the Change and on the
 * packet, in the predeclared words, with the date:
 *
 *   "Sam sees that the criteria were accepted after the work was done, in
 *    those words, with the date."  (docs/PHASE1-DECISIONS.md §2.5, row Q7)
 *
 * The served field is `change.retrospective` (crates/core/src/change.rs,
 * `RetrospectiveAcceptance`): an object when it holds, `null` when it does
 * not. These tests render both surfaces from a served shape and read the
 * element each sentence must live in — the same two elements the Q07 case
 * names (`#ws-change-panel-define`, `#ws-packet-accepted`). The control is the
 * same Change with `retrospective: null`: nothing on any surface says "after".
 *
 * A second bundle is built over a PROBE copy — the real `HQ_COPY` with the two
 * new strings replaced — because equality with the real copy cannot tell a
 * string that came from the copy file from a literal that happens to match it.
 *
 * The fixture is synthetic. No string below came off a real machine.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-retrospective-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeScreen } from './change-workspace.jsx';
  import { PacketPage } from './packet.jsx';
  import { buildChangeView } from './change-view.js';
  import { buildPacketView } from './packet-view.js';
  import { deriveChangeModel } from './change-model.js';
  import { HQ_COPY } from './hq-copy.js';
  export { buildChangeView, buildPacketView, deriveChangeModel, HQ_COPY };
  export const renderScreen = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderPacket = props => renderToStaticMarkup(React.createElement(PacketPage, props));
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

const M = await load('retrospective.mjs')

const COPY_FILE = join(here, 'hq-copy.js')
const PROBE_DEFINE = 'probe-define-after-work {date}'
const PROBE_PACKET = 'probe-packet-after-work {date}'
const PROBED = await load('retrospective-probe.mjs', [{
  name: 'probe-copy',
  setup(b) {
    b.onLoad({ filter: /hq-copy\.js$/ }, () => {
      const source = readFileSync(COPY_FILE, 'utf8')
      /* Each pattern is anchored to the start of its own key: `after_work` is
       * also the tail of `accepted_after_work`, and an unanchored second
       * pattern would overwrite the first's probe. */
      const swapped = source
        .replace(/(\s)accepted_after_work: '[^']*'/, `$1accepted_after_work: '${PROBE_DEFINE}'`)
        .replace(/(\s)after_work: '[^']*'/, `$1after_work: '${PROBE_PACKET}'`)
      assert.notEqual(swapped, source, 'the probe replaced nothing')
      assert.ok(swapped.includes(PROBE_DEFINE) && swapped.includes(PROBE_PACKET), 'a probe string was not placed')
      return { contents: swapped, loader: 'js' }
    })
  },
}])

const { buildChangeView, buildPacketView, HQ_COPY, renderScreen, renderPacket } = M

/* The predeclared words, and the date the acceptance row carries. */
const AFTER_WORDS = 'accepted after the work was done'
const ACCEPTED_AT = '2026-09-26T09:20:00Z'
const ACCEPTED_DATE = '2026-09-26'
const WORK_AT = '2026-09-25T17:10:00Z'
const WORK_DATE = '2026-09-25'

const text = (html) => html.replace(/<[^>]*>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/&#x2F;/g, '/').replace(/\s+/g, ' ').trim()

/** The text inside the element whose id is `id`, up to its matching close. */
function region(html, id) {
  const open = html.indexOf(`id="${id}"`)
  assert.ok(open > 0, `no element with id ${id} on the page`)
  const start = html.lastIndexOf('<', open)
  const tag = /^<([a-z0-9]+)/i.exec(html.slice(start))[1]
  let depth = 0
  const re = new RegExp(`<(/?)${tag}\\b[^>]*>`, 'gi')
  re.lastIndex = start
  for (let m = re.exec(html); m; m = re.exec(html)) {
    depth += m[1] ? -1 : 1
    if (depth === 0) return text(html.slice(start, m.index + m[0].length))
  }
  return text(html.slice(start))
}

function aChange(retrospective, over = {}) {
  return {
    schema_version: 6,
    id: 'cccccccc-0000-4000-8000-00000000c0f2',
    project_id: 'cccccccc-0000-4000-8000-00000000p0f2',
    opened_at: '2026-09-25T09:00:00Z',
    request: { title: 'Retry a failed delivery', purpose: 'Two retry requests for the same delivery do not enqueue duplicate work.', requester: 'sam', source: 'person' },
    accepted_scope: {
      version: 1,
      proposed_at: ACCEPTED_AT,
      criteria: ['Two retry requests for the same delivery do not enqueue duplicate work.'],
      exclusions: [], allowed_scope: ['src/**'], constraints: [], check_profile: [],
      content_sha256: 'e'.repeat(64),
      acceptance: { actor: 'sam', accepted_at: ACCEPTED_AT, rationale: 'accepting what was built' },
    },
    scope_history: [],
    implementation: { version: 0, bound_at: WORK_AT, repository_identity: 'repo-synthetic', base_revision: '3062e374' + '0'.repeat(32), snapshot_id: null, workspace_identity: null },
    implementation_history: [],
    associations: [],
    examination: 'none_yet',
    decision: 'none_yet',
    reviews: [],
    code_moved: null,
    retrospective,
    masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
    ...over,
  }
}

const RETROSPECTIVE = {
  scope_version: 1,
  accepted_at: ACCEPTED_AT,
  accepted_on: ACCEPTED_DATE,
  work_record_id: 'ei-work',
  work_recorded_at: WORK_AT,
  opened_revision: 'a6b45180' + '0'.repeat(32),
  work_revision: '3062e374' + '0'.repeat(32),
}

const STAGES = ['define', 'prepare', 'build', 'verify', 'review', 'approve']
const screenOf = (mod, change, stage) => mod.renderScreen({ model: mod.buildChangeView({ change, work: [] }), stage })
const defineOf = (change, mod = M) => screenOf(mod, change, 'define')
/* QUAL-F2-R3: the packet page is built the way the product builds it
 * (`readPacket` in packet.jsx): with the ONE derived view-model, `hq`. Before
 * R3 this helper left it out, and a page with no `hq` could not tell a
 * sentence that came from the derived section array (which every exported file
 * walks) from one a component composed for itself — which is how R2's
 * JSX-only sentence passed every test here and was in no exported file. The
 * assertions below are unchanged; only the input now has what the page always
 * has. */
const packetOf = (change, mod = M) => mod.renderPacket({
  model: mod.buildPacketView({ change, work: [], hq: mod.deriveChangeModel(change, null, null, null, []) }),
})

/* ── the case (Q07 V1): accepted after the work ─────────────────────────── */

test('the Change says, where what counts as done is named, that it was accepted after the work was done, with the date', () => {
  const define = region(defineOf(aChange(RETROSPECTIVE)), 'ws-change-panel-define')
  assert.ok(define.includes(AFTER_WORDS), `Define does not say "${AFTER_WORDS}": ${define}`)
  assert.ok(define.includes(ACCEPTED_DATE), `Define does not carry the acceptance date: ${define}`)
})

test('the packet says it in the accepted section, in those words, with the date', () => {
  const accepted = region(packetOf(aChange(RETROSPECTIVE)), 'ws-packet-accepted')
  assert.ok(accepted.includes(AFTER_WORDS), `the packet's accepted section does not say "${AFTER_WORDS}": ${accepted}`)
  assert.ok(accepted.includes(ACCEPTED_DATE), `the packet's accepted section does not carry the acceptance date: ${accepted}`)
})

test('the date said is the acceptance date, not the date the work entered the record', () => {
  for (const [name, body] of [['Define', region(defineOf(aChange(RETROSPECTIVE)), 'ws-change-panel-define')], ['packet', region(packetOf(aChange(RETROSPECTIVE)), 'ws-packet-accepted')]]) {
    const sentence = body.slice(body.indexOf(AFTER_WORDS))
    assert.ok(sentence.includes(ACCEPTED_DATE), `${name}: ${sentence}`)
    assert.ok(!body.includes(WORK_DATE), `${name} names the work's date as if it were the acceptance's: ${body}`)
  }
})

/* The card says the date is a UTC one. The engine serves it (`accepted_on`,
 * the UTC date of the instant); this reads it rather than re-deriving a date
 * from the spelling of a stamp, which is the local date for an offset stamp. */
test('the date said is the served UTC date, not the front of the stamp’s spelling', () => {
  const change = aChange({ ...RETROSPECTIVE, accepted_at: '2026-09-26T23:30:00-05:00', accepted_on: '2026-09-27' })
  for (const [name, html, id] of [['Define', defineOf(change), 'ws-change-panel-define'], ['packet', packetOf(change), 'ws-packet-accepted']]) {
    const body = region(html, id)
    const sentence = body.slice(body.indexOf(AFTER_WORDS))
    assert.ok(sentence.includes('2026-09-27'), `${name}: ${sentence}`)
  }
})

test('the words come from the copy file on both surfaces, and say what they rest on', () => {
  const define = HQ_COPY.change.define.accepted_after_work
  const packet = HQ_COPY.change.packet.accepted.after_work
  for (const [path, s] of [['change.define.accepted_after_work', define], ['change.packet.accepted.after_work', packet]]) {
    assert.equal(typeof s, 'string', `${path} is not in the copy file`)
    assert.ok(s.includes(AFTER_WORDS), `${path} does not use the predeclared words: ${s}`)
    assert.ok(s.includes('{date}'), `${path} has no date in it: ${s}`)
    assert.ok(s.includes('commit'), `${path} does not say what the record showed: ${s}`)
  }
})

test('what each surface prints is the copy file’s string, not a literal in the component', () => {
  const define = region(defineOf(aChange(RETROSPECTIVE), PROBED), 'ws-change-panel-define')
  assert.ok(define.includes(`probe-define-after-work ${ACCEPTED_DATE}`), `Define does not print the copy file's string: ${define}`)
  const packet = region(packetOf(aChange(RETROSPECTIVE), PROBED), 'ws-packet-accepted')
  assert.ok(packet.includes(`probe-packet-after-work ${ACCEPTED_DATE}`), `the packet does not print the copy file's string: ${packet}`)
})

/* ── the control (Q07 CTL): accepted first ───────────────────────────────── */

test('accepted before the work: nothing on any surface says after', () => {
  const change = aChange(null)
  for (const stage of STAGES) assert.ok(!text(screenOf(M, change, stage)).includes(AFTER_WORDS), `the ${stage} stage says "after" on an acceptance that came first`)
  assert.ok(!text(packetOf(change)).includes(AFTER_WORDS), 'the packet says "after" on an acceptance that came first')
})

test('a server older than the field (no `retrospective` key) says nothing either', () => {
  const change = aChange(undefined)
  delete change.retrospective
  assert.equal(buildChangeView({ change, work: [] }).retrospective, null)
  for (const stage of STAGES) assert.ok(!text(screenOf(M, change, stage)).includes(AFTER_WORDS))
  assert.ok(!text(packetOf(change)).includes(AFTER_WORDS))
})

/* ── the model: one reading, both surfaces ─────────────────────────────── */

test('the model reads the served field once, and the packet carries the same reading', () => {
  const change = aChange(RETROSPECTIVE)
  const view = buildChangeView({ change, work: [] })
  assert.deepEqual(view.retrospective, { scopeVersion: 1, acceptedOn: ACCEPTED_DATE, workOn: WORK_DATE })
  assert.deepEqual(buildPacketView({ change, work: [] }).retrospective, view.retrospective)
})

/* ── claim less ─────────────────────────────────────────────────────────── */

test('a served fact whose date cannot be read is not said at all: the sentence needs its date', () => {
  for (const accepted_on of [undefined, null, '', 'sometime on friday', '26 Sep 2026', '2026-9-26', 20260926, '2026-09-26T09:20:00Z']) {
    const change = aChange({ ...RETROSPECTIVE, accepted_on })
    assert.equal(buildChangeView({ change, work: [] }).retrospective, null, `accepted_on ${JSON.stringify(accepted_on)} was read as a date`)
    assert.ok(!text(defineOf(change)).includes(AFTER_WORDS), `Define says it with accepted_on ${JSON.stringify(accepted_on)}`)
    assert.ok(!text(packetOf(change)).includes(AFTER_WORDS), `the packet says it with accepted_on ${JSON.stringify(accepted_on)}`)
  }
})

test('a served fact that is not an object is not said', () => {
  for (const retrospective of [true, 'yes', 1, [], [RETROSPECTIVE]]) {
    const change = aChange(retrospective)
    assert.equal(buildChangeView({ change, work: [] }).retrospective, null, `${JSON.stringify(retrospective)} was read as a fact`)
    assert.ok(!text(defineOf(change)).includes(AFTER_WORDS))
  }
})

/* The sentence is about the list that is named next to it. Served against a
 * Change with no accepted list there is no list for it to be about, and a
 * sentence about "this list" with no list above it says nothing true. */
test('with no accepted list on the page there is no sentence about it', () => {
  const change = aChange(RETROSPECTIVE, { accepted_scope: null })
  assert.ok(!text(defineOf(change)).includes(AFTER_WORDS))
  assert.ok(!text(packetOf(change)).includes(AFTER_WORDS))
})

test('the sentence names no state word and accuses no one', () => {
  for (const s of [HQ_COPY.change.define.accepted_after_work, HQ_COPY.change.packet.accepted.after_work]) {
    assert.equal(typeof s, 'string', 'the sentence is not in the copy file')
    for (const banned of ['Proven', 'Not proven', 'In progress', 'Recorded', 'Needs a re-check', 'Loose thread', 'Fog', 'Approved', 'lie', 'lied', 'wrong', 'fail', 'ledger', 'receipt', 'event', 'disposition']) {
      assert.ok(!new RegExp(`\\b${banned}\\b`, 'i').test(s), `"${banned}" is in: ${s}`)
    }
  }
})

/* ── self-attack (against the first fix) ───────────────────────────────── */

/* The sentence says "This list". If the served fact is about another version
 * of the list than the one on the page, "this list was accepted after the work"
 * would be a claim about a list that is not the one being read. The engine
 * derives both from the standing scope, so they agree; a page that meets a
 * pair that does not agree says nothing rather than pick one. */
test('a fact about another version of the list than the one on the page is not said', () => {
  for (const scope_version of [2, 0, null, undefined, '1']) {
    const change = aChange({ ...RETROSPECTIVE, scope_version })
    assert.equal(buildChangeView({ change, work: [] }).retrospective, null, `scope_version ${JSON.stringify(scope_version)} was read as the page's list (1)`)
    assert.ok(!text(defineOf(change)).includes(AFTER_WORDS), `Define says it with scope_version ${JSON.stringify(scope_version)}`)
    assert.ok(!text(packetOf(change)).includes(AFTER_WORDS), `the packet says it with scope_version ${JSON.stringify(scope_version)}`)
  }
})

test('a fact about the same list is said, whatever number that list has', () => {
  const change = aChange({ ...RETROSPECTIVE, scope_version: 3 })
  change.accepted_scope.version = 3
  assert.ok(text(defineOf(change)).includes(AFTER_WORDS))
  assert.ok(text(packetOf(change)).includes(AFTER_WORDS))
})

/* Both surfaces print one sentence, once. The packet already carries the date
 * in the line above it; the sentence must not be printed twice on either. */
test('the sentence appears exactly once on each surface', () => {
  const count = (s) => s.split(AFTER_WORDS).length - 1
  assert.equal(count(text(defineOf(aChange(RETROSPECTIVE)))), 1)
  assert.equal(count(text(packetOf(aChange(RETROSPECTIVE)))), 1)
})

/* Claim less: the sentence says what the record showed and nothing about the
 * work meeting the list, and it never says nothing was agreed earlier. */
test('the sentence does not claim the work meets the list or that nothing was agreed earlier', () => {
  for (const s of [HQ_COPY.change.define.accepted_after_work, HQ_COPY.change.packet.accepted.after_work]) {
    for (const overclaim of [/meets?\b/i, /satisf/i, /proven?\b/i, /passed?\b/i, /not agreed/i, /never agreed/i, /no agreement/i, /before anyone/i, /nothing was agreed/i]) {
      assert.ok(!overclaim.test(s), `${overclaim} is in: ${s}`)
    }
  }
})
