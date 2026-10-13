import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* T-2-4 — the Change screen, rendered.
 *
 * The same harness the Verdict panel uses: the surface is bundled and rendered
 * to static markup, and every assertion below is about the STRINGS AND
 * CONTROLS A PERSON MEETS. A second bundle is built over a PROBE copy — the
 * real `HQ_COPY` with two strings replaced — because equality with the real
 * copy cannot tell a string that came from the copy file from a literal that
 * happens to match it.
 *
 * The fixture Change is synthetic. No string below came off a real machine.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-change-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeScreen, ChangeNotice, StageStrip, changeIdFromHash, CriterionLine, ChangeShell } from './change-workspace.jsx';
  import { deriveChangeModel } from './change-model.js';
  import { Card } from './verdict-panel.jsx';
  import { buildChangeView, STAGES, absenceWithoutItsScope, ABSENCE_OF_PROBLEM, SAYS_ITS_SCOPE } from './change-view.js';
  import { HQ_COPY } from './hq-copy.js'
  import { fill } from './hq-narrate.js';
  export { buildChangeView, STAGES, changeIdFromHash, absenceWithoutItsScope, ABSENCE_OF_PROBLEM, SAYS_ITS_SCOPE, HQ_COPY, fill };
  export const renderScreen = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderNotice = props => renderToStaticMarkup(React.createElement(ChangeNotice, props));
  export const renderStrip = props => renderToStaticMarkup(React.createElement(StageStrip, props));
  export const renderCard = props => renderToStaticMarkup(React.createElement(Card, props));
  export const renderLine = props => renderToStaticMarkup(React.createElement(CriterionLine, props));
  export const renderShell = props => renderToStaticMarkup(React.createElement(ChangeShell, props));
  export { deriveChangeModel };
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

const M = await load('change.mjs')

/* The probe: the real copy file with two Change strings swapped. A surface
 * that renders them through HQ_COPY renders the probe; one carrying a literal
 * renders the real words and fails. */
const COPY_FILE = join(here, 'hq-copy.js')
const PROBED = await load('change-probe.mjs', [{
  name: 'probe-copy',
  setup(b) {
    b.onLoad({ filter: /hq-copy\.js$/ }, () => ({
      contents: readFileSync(COPY_FILE, 'utf8')
        .replace("no_criteria: 'Nothing counts as done yet: accept what should.'", "no_criteria: 'probe-conclusion-no-criteria'")
        .replace("empty: 'No review has been written down for this Change yet, so there is nothing here to read.'", "empty: 'probe-review-empty'"),
      loader: 'js',
    }))
  },
}])

const { buildChangeView, STAGES, HQ_COPY, renderScreen, renderNotice, renderStrip, renderCard, changeIdFromHash, absenceWithoutItsScope, ABSENCE_OF_PROBLEM, SAYS_ITS_SCOPE, fill } = M

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
    exclusions: [], allowed_scope: [], constraints: [], check_profile: ['cargo-test'],
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
  said: { parts: [{ text: 'The AI said this was done at step 44: ' }, { quote: 'All tests pass.', speaker: 'agent' }], refs: ['r-44'] },
  record: { text: 'Not proven: nothing in the record shows this was done.', role: 'record', refs: ['r-44'] },
  do: { text: 'Ask the AI for proof: the output of a check, not a sentence.', role: 'do', refs: [], prompt: 'Run the check that shows "All tests pass." on the current code and paste the full output, including the command you ran.' },
  detector: null,
  proofNode: null,
}

const modelWith = (over = {}) => buildChangeView({
  change: FULL,
  work: [{ id: SESSION_A, findings: [CARD], focus: { summary: { total: 6, shown: 2, merged: 4 }, labels: [{ id: 'n1', text: 'Prompt seq 8' }, { id: 'n2', text: 'File change seq 17' }] } }],
  ...over,
})

const text = (html) => html.replace(/<[^>]*>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ').trim()

/* ── Sam's question 1, from the top, in ten seconds ────────────────────── */

test('the top is one title, one sentence and one named version, in that order and nothing between', () => {
  const html = renderScreen({ model: modelWith(), stage: 'define' })
  const top = html.slice(0, html.indexOf('ws-change-stages'))
  const order = ['ws-change-kicker', 'ws-change-title', 'ws-change-conclusion', 'ws-change-version']
  let at = -1
  for (const cls of order) {
    const next = top.indexOf(cls)
    assert.ok(next > at, `${cls} is out of reading order at the top of the screen`)
    at = next
  }
  assert.match(top, />Retry failed uploads</)
  assert.ok(top.includes('Accepted: 2 things count as done. None has evidence yet.'))
  assert.ok(top.includes('version a1b2c3d · 2026-01-02'), 'the version is not in the Plain text at the top')
  /* H2T-4: the version is part of the words, not a chip outside them. */
  assert.ok(!/ws-change-chip/.test(top))
})

test('the top carries no state word, no percentage and nothing green', () => {
  const html = renderScreen({ model: modelWith(), stage: 'define' })
  const top = text(html.slice(0, html.indexOf('ws-change-stages')))
  for (const word of Object.values(HQ_COPY.states).map((s) => s.word)) {
    assert.ok(!top.includes(word), `the top renders the state word "${word}": ${top}`)
  }
  assert.doesNotMatch(top, /\d+\s*%/)
  assert.doesNotMatch(top, /\bgreen\b|\ball clear\b|\beverything checks out\b/i)
})

/* ── the banned-word sweep over the rendered Plain DOM ─────────────────── */

test('no word off the banned list reaches the rendered Plain surface of any stage', () => {
  const banned = [/\bevents?\b/i, /\breceipts?\b/i, /\bcandidates?\b/i, /\bapplicability\b/i, /\bdispositions?\b/i, /\bpacket\b/i, /\bledger\b/i, /\bsession id\b/i, /\btree hash\b/i, /\bpredicates?\b/i]
  for (const stage of STAGES) {
    const html = renderScreen({ model: modelWith(), stage })
    /* The technical layer is one click down and keeps its own words; the sweep
     * is over the Plain surface, which is everything before it. */
    const plain = text(html.slice(0, html.indexOf('ws-change-technical')))
    for (const re of banned) assert.doesNotMatch(plain, re, `stage ${stage} renders a banned word: ${plain}`)
  }
})

test('the rendered banned-word sweep is a live instrument', () => {
  const banned = [/\bevents?\b/i, /\bledger\b/i]
  assert.ok(banned.some((re) => re.test('16 of 16 events in view · Ledger read 18:07:43 UTC')))
})

/* ── the six stages are views ──────────────────────────────────────────── */

test('the strip renders the six, marks the open one, and is reachable by keyboard', () => {
  const model = modelWith()
  const html = renderStrip({ stages: model.stages, active: 'verify', onGo: () => {} })
  assert.match(html, /role="tablist"/)
  assert.match(html, /aria-label="The six stages of this Change"/)
  for (const name of ['Define', 'Prepare', 'Build', 'Verify', 'Review', 'Approve']) {
    assert.ok(html.includes('>' + name + '<'), `the strip lost ${name}`)
  }
  assert.equal([...html.matchAll(/role="tab"/g)].length, 6)
  assert.equal([...html.matchAll(/aria-selected="true"/g)].length, 1)
  assert.equal([...html.matchAll(/tabindex="0"/g)].length, 1, 'the strip is one tab stop, with arrow keys inside it')
  assert.equal([...html.matchAll(/tabindex="-1"/g)].length, 5)
  assert.match(html, /id="ws-change-tab-verify"[^>]*aria-selected="true"/)
})

test('each stage renders its own panel, wired to its own tab', () => {
  for (const stage of STAGES) {
    const html = renderScreen({ model: modelWith(), stage })
    assert.match(html, new RegExp(`id="ws-change-panel-${stage}"`))
    assert.match(html, new RegExp(`aria-labelledby="ws-change-tab-${stage}"`))
  }
})

test('the strip says in words that moving between stages settles nothing', () => {
  const html = renderScreen({ model: modelWith(), stage: 'define' })
  assert.ok(text(html).includes('These six are views of the same Change. Moving between them settles nothing.'))
})

test('navigating writes nothing: the screen reaches exactly three write addresses of its own, the run sheet three and the review section one, and none is reachable by navigating', () => {
  const source = readFileSync(join(here, 'change-workspace.jsx'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
  for (const forbidden of ["method: 'POST'", 'method: "POST"', '/associations', '/scope/accept', 'child_process', 'eval(']) {
    assert.ok(!source.includes(forbidden), `the Change screen can write: ${forbidden}`)
  }
  /* T-S-4 widened this file from "no write at all" to "exactly two writes",
   * and the test is widened to say so rather than left phrased as an absolute
   * it no longer holds. T-3-4 widens it once more, and records the widening
   * the same way rather than silently: the third is the one address that stops
   * a running check. All three are declared as named constants next to the
   * control that uses them, and nothing else in the file constructs an
   * `/api/changes/` address that is not a read. */
  const writes = source.match(/'\/api\/changes\/' \+ encodeURIComponent\((?:id|run)\)[^\n]*/g) || []
  assert.deepEqual(writes.map((w) => w.trim()).sort(), [
    "'/api/changes/' + encodeURIComponent(id) + '/decisions'",
    "'/api/changes/' + encodeURIComponent(id) + '/decisions/withdraw'",
    "'/api/changes/' + encodeURIComponent(id) + '/runs/' + encodeURIComponent(run) + '/cancel'",
  ], 'the Change screen reaches a write address it does not declare')
  /* P6-5 widens it once more, and says so: the run sheet this screen mounts
   * (`run-sheet.jsx`) writes to addresses of its own, and its Cancel is
   * handed the T-3-4 stop above rather than declaring a copy. P6-5B adds the
   * third, taking the permission back. Six in all, across the two files, and
   * a seventh cannot appear unannounced. */
  const sheet = readFileSync(join(here, 'run-sheet.jsx'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
  const sheetAddresses = sheet.match(/'\/api\/changes\/' \+ encodeURIComponent\((?:id|run)\)[^\n]*/g) || []
  assert.deepEqual(sheetAddresses.map((w) => w.trim()).sort(), [
    "'/api/changes/' + encodeURIComponent(id) + '/run'",
    "'/api/changes/' + encodeURIComponent(id) + '/run/permission'",
    "'/api/changes/' + encodeURIComponent(id) + '/run/permission/revoke'",
  ], 'the run sheet reaches an address it does not declare')
  const sheetWrites = (sheet.match(/postWithToken\((\w+)\(/g) || []).map((w) => w.replace('postWithToken(', '').replace('(', '')).sort()
  assert.deepEqual(sheetWrites, ['RUN_PERMISSION', 'RUN_PERMISSION_REVOKE', 'RUN_SHEET'], 'the run sheet writes somewhere other than its three addresses')
  assert.equal((sheet.match(/method: 'POST'/g) || []).length, 1, 'the run sheet posts outside its one token-carrying writer')
  assert.ok(!sheet.includes('/cancel'), 'the run sheet declares its own copy of the stop address')
  assert.match(source, /export const stopRun = \(changeId\) => \(run, actor\) => writeAction\(CHANGE_CANCEL_RUN\(changeId, run\), \{ actor \}\)/)
  /* T-4-2 widens the census again, and records it the same way: the section
   * that says where each thing review found stands is mounted by this screen
   * (Brief and the Review stage) and lives in `change-review.jsx`, which now
   * declares exactly ONE write address beside its one control. Counted here,
   * and in CLAUDE.md's write-address table, in the same change. */
  const review = readFileSync(join(here, 'change-review.jsx'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
  for (const forbidden of ["method: 'POST'", 'method: "POST"', '/associations', '/scope/accept', '/decisions', '/cancel', 'child_process', 'eval(']) {
    assert.ok(!review.includes(forbidden), `the review section can write: ${forbidden}`)
  }
  const reviewWrites = review.match(/'\/api\/changes\/' \+ encodeURIComponent\((?:id|run)\)[^\n]*/g) || []
  assert.deepEqual(reviewWrites.map((w) => w.trim()), [
    "'/api/changes/' + encodeURIComponent(id) + '/findings/' + encodeURIComponent(finding) + '/standing'",
  ], 'the review section reaches a write address it does not declare')
  assert.equal((review.match(/writeAction\(/g) || []).length, 1, 'one write call, from the one control')
  /* T5 — the Change screen now also READS the run sheet's document (typed
   * test counts for the message for the agent). It reaches it through
   * `readProjection`, whose method is GET for every address but `/api/refresh`,
   * and through nothing that writes; the message itself is copy only. */
  assert.deepEqual(source.match(/\bRUN_SHEET\b[^\n]*/g).map((l) => l.trim()).sort(), [
    'RUN_SHEET(id))',
    "RUN_SHEET, RunDoor, RunSheetHost } from './run-sheet.jsx'",
  ], 'the Change screen uses the run sheet address for something other than one read')
  assert.match(source, /await readProjection\(RUN_SHEET\(id\)\)/)
  assert.ok(!/writeAction\(RUN_SHEET/.test(source), 'the Change screen writes to the run sheet address')
  /* Every address it reads comes from `data.js`, and the only Change addresses
   * exported there are the two GETs. */
  const data = readFileSync(join(here, 'data.js'), 'utf8')
  assert.match(data, /export const CHANGE = \(id\) =>/)
  assert.match(data, /export const CHANGE_PROPOSALS = \(id\) =>/)
  assert.ok(!/scope\/accept|\/associations/.test(data), 'a write route reached the read layer')
  /* T-DIST-4 widens the READ census by one address, recorded here rather than
   * silently: `GET /api/licence`, which says whether this machine may approve.
   * It is read through `readProjection` (a GET: it is not `API.refresh`), and
   * the write census above is unchanged — the licence adds no write address
   * anywhere on this screen. */
  assert.match(data, /\n  licence: '\/api\/licence',\n/, 'the licence read is declared in the read layer')
  assert.match(source, /readProjection\(API\.licence\)/, 'the Change screen reads the licence through the read layer')
  assert.ok(!/writeAction\(API\.licence/.test(source), 'the licence address is written to')
  const reads = [...source.matchAll(/readProjection\(([A-Z_.a-z]+)/g)].map((m) => m[1]).sort()
  /* `CHANGE_FINDINGS` (T-4-2, `GET …/findings`) arrived on main beside this
   * census before the dist merge; named here so the census stays whole.
   * `RUN_SHEET` (A3 T5, `GET …/run`, the one read pinned by the T5 lines
   * above) arrived on the integration before this census met it, at the A2
   * merge of dist onto it; named here for the same reason. */
  assert.deepEqual([...new Set(reads)], ['API.claims', 'API.explain', 'API.licence', 'CHANGE', 'CHANGE_APPLICABILITY', 'CHANGE_COVERAGE', 'CHANGE_FINDINGS', 'CHANGE_INPUTS', 'CHANGE_PROPOSALS', 'RUN_SHEET'],
    'the Change screen reads an address the census does not name')
})

/* ── every stage with nothing in it teaches ────────────────────────────── */

test('every one of the six, on an empty Change, renders the sentence that says what would fill it', () => {
  const bare = buildChangeView({ change: aChange(), work: [] })
  for (const stage of STAGES) {
    const body = text(renderScreen({ model: bare, stage }))
    const expected = HQ_COPY.change.stages[stage].empty
    assert.ok(body.includes(expected), `stage ${stage} renders no empty-state sentence; expected: ${expected}`)
  }
})

test('the empty-state sentences come from the copy file, not from a literal in the view', () => {
  const bare = PROBED.buildChangeView({ change: aChange(), work: [] })
  const body = PROBED.renderScreen({ model: bare, stage: 'review' })
  assert.ok(body.includes('probe-review-empty'), 'the Review empty state is a literal in the view')
  assert.ok(PROBED.renderScreen({ model: bare, stage: 'define' }).includes('probe-conclusion-no-criteria'), 'the top sentence is a literal in the view')
})

/* ── one primary action per view ───────────────────────────────────────── */

test('every view has exactly one primary action, and every disabled control says why and what would enable it', () => {
  for (const model of [modelWith(), buildChangeView({ change: aChange(), work: [] })]) {
    for (const stage of STAGES) {
      const html = renderScreen({ model, stage })
      const panel = html.slice(html.indexOf('ws-change-panel"'), html.indexOf('ws-change-trace'))
      /* The Verify view's one primary is the finding card's own "Copy prompt
       * for the agent", which wears the Verdict panel's primary class because
       * it IS that button, reused. Both classes are counted; the rule is one
       * filled control per view, whichever module drew it. */
      const primaries = [...panel.matchAll(/class="ws-(?:change|verdict)-primary"/g)].length
      assert.equal(primaries, 1, `stage ${stage} has ${primaries} primary actions`)
      for (const m of panel.matchAll(/<button[^>]*disabled[^>]*aria-describedby="([^"]+)"/g)) {
        const why = new RegExp(`id="${m[1]}"[^>]*>([^<]+)<`).exec(panel)
        assert.ok(why, `stage ${stage}: a disabled control points at a reason that is not on the screen`)
        assert.ok(why[1].trim().length > 30, `stage ${stage}: the reason says nothing: ${why[1]}`)
      }
    }
  }
})

test('Approve is off, and it says both why and what would enable it', () => {
  const html = renderScreen({ model: modelWith(), stage: 'approve' })
  const body = text(html)
  assert.match(html, /<button type="button" disabled[^>]*class="ws-change-primary">Approve this version<\/button>/)
  const blocker = modelWith().blocker
  /* The reason carries a count now (T-3-6-F1), so it is compared as the
     screen renders it rather than as the template. */
  assert.ok(body.includes(fill(blocker.reason, blocker.values)), 'the Approve reason is missing')
  assert.doesNotMatch(body, /required check/, 'the Approve reason must not count checks the record was never asked about')
  assert.ok(body.includes(blocker.enabler), 'what would enable Approve is missing')
  assert.ok(body.includes('Nothing here turns green. What is missing is shown as missing.'))
})

test('"Approved with exceptions" is rendered on no stage: that decision is a later phase', () => {
  for (const stage of STAGES) {
    assert.doesNotMatch(text(renderScreen({ model: modelWith(), stage })), /approved with exceptions/i)
  }
})

/* ── Verify and Review reuse the finding cards, scoped to confirmed work ── */

test('Verify renders the existing finding card, scoped to the confirmed work, and says what it left out', () => {
  const html = renderScreen({ model: modelWith(), stage: 'verify' })
  const body = text(html)
  assert.match(html, /class="ws-verdict-card ws-verdict-card-not_proven"/, 'the existing finding card was not reused')
  assert.ok(body.includes('Not proven: nothing in the record shows this was done.'))
  assert.ok(body.includes('Copy prompt for the agent'))
  assert.ok(body.includes('Only work you confirmed belongs to this Change is counted here.'))
  assert.ok(body.includes('From 1 piece of linked work'))
  assert.ok(body.includes('1 more piece of work was suggested and not confirmed, so nothing from it is counted.'))
})

test('a session that is only suggested contributes no card and no node to the screen', () => {
  const model = buildChangeView({
    change: FULL,
    work: [
      { id: SESSION_A, findings: [], focus: { summary: { total: 2, shown: 0, merged: 2 }, labels: [] } },
      { id: SESSION_B, findings: [{ ...CARD, id: 'from_suggested' }], focus: { summary: { total: 5, shown: 5, merged: 0 }, labels: [{ id: 'nS', text: 'Suggested work node' }] } },
    ],
  })
  const html = renderScreen({ model, stage: 'verify' })
  assert.ok(!html.includes('from_suggested'))
  assert.ok(!html.includes('Suggested work node'), 'a node from unconfirmed work reached the focus list')
})

/* ── the focus set, scoped the same way ────────────────────────────────── */

test('the focus set renders as a keyboard-readable list and states what it folded away', () => {
  /* Verify, not Define: the focus set used to render under every stage, where
     a reader had to decide each time whether it belonged to the stage above it
     or to the Change (T24-4). It now belongs to the stage whose question it
     answers, and the test below holds it there. */
  const body = text(renderScreen({ model: modelWith(), stage: 'verify' }))
  assert.ok(body.includes('The work behind this Change'))
  assert.ok(body.includes('2 of 6 steps shown'))
  assert.ok(body.includes('4 steps folded away'))
  assert.ok(body.includes('Prompt seq 8'))
  assert.ok(body.includes('Open the full trace'), 'the full trace is not reachable one level down')
})

/* ── the technical layer is one level down, never on top ───────────────── */

test('ids, digests and how a link was made sit inside one details element at the bottom', () => {
  const html = renderScreen({ model: modelWith(), stage: 'define' })
  const at = html.indexOf('ws-change-technical')
  assert.ok(at > 0)
  /* The rule is about what a person READS: no id and no digest in the words
   * on the primary surface. T-2-5 added the link to the whole story on one
   * page, whose href is `#/changes/<id>/packet` — the id is in the ADDRESS,
   * which the browser's own address bar already shows and which no reader
   * meets as text. So the sweep is over the rendered text, and the raw markup
   * is checked separately for the digest, which belongs in no attribute
   * either. */
  const plain = text(html.slice(0, at))
  assert.ok(!plain.includes('cccccccc-0000-4000-8000-00000000c001'), 'the Change id is on the primary surface')
  assert.ok(!html.slice(0, at).includes('2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c'), 'a digest is on the primary surface')
  assert.match(html.slice(at), /<summary>The technical details<\/summary>/)
  /* And it is SHUT. `<details open>` renders every id and digest on the
   * primary surface while leaving the markup order untouched, which the
   * position check above cannot see (break B14). */
  assert.doesNotMatch(html, /<details class="ws-change-technical"[^>]*\bopen\b/, 'the technical layer is open by default')
  assert.ok(html.slice(at).includes('cccccccc-0000-4000-8000-00000000c001'))
})

/* ── nothing renders blank, ever ───────────────────────────────────────── */

test('loading, missing and refused each say what happened in a sentence', () => {
  assert.ok(text(renderNotice({ status: 'loading' })).includes('Opening this Change…'))
  assert.ok(text(renderNotice({ status: 'not_found' })).includes('There is no Change at this address.'))
  const refused = text(renderNotice({ status: 'refused', refusal: '/api/changes/x did not answer' }))
  assert.ok(refused.includes('This Change could not be opened.'))
  assert.ok(refused.includes('/api/changes/x did not answer'))
  assert.doesNotMatch(refused, /[{}]/)
})

/* ── the address ───────────────────────────────────────────────────────── */

test('the address names one Change and, optionally, one stage; anything else is refused', () => {
  assert.deepEqual(changeIdFromHash('#/changes/abc-123'), { id: 'abc-123', stage: 'define' })
  assert.deepEqual(changeIdFromHash('#/changes/abc-123/verify'), { id: 'abc-123', stage: 'verify' })
  assert.deepEqual(changeIdFromHash('#/changes/abc-123/nonsense'), { id: 'abc-123', stage: 'define' })
  for (const bad of ['', '#/trace', '#/changes/', '#/changes/a/b/c', '#/changes/../../etc/passwd']) {
    assert.equal(changeIdFromHash(bad), null, `accepted a bad address: ${bad}`)
  }
})

/* ── motion and colour ─────────────────────────────────────────────────── */

test('the screen has no motion to reduce, and its reduced-motion block says so', () => {
  const source = readFileSync(join(here, 'change-workspace.jsx'), 'utf8')
  assert.ok(!/animation|transition|setInterval|setTimeout|requestAnimationFrame/.test(source.replace(/\/\*[\s\S]*?\*\//g, '')))
  const css = readFileSync(join(here, 'workspace.css'), 'utf8')
  const block = css.slice(css.indexOf('/* ── the Change workspace (T-2-4)'))
  assert.match(block, /@media \(prefers-reduced-motion: reduce\)/)
  assert.match(block, /animation: none !important/)
  /* Nothing on this screen is carried by colour alone: the open stage is
   * marked by weight and a rule, and no rule here sets a hue for a state. */
  const on = /\.ws-change-stage-on\s*\{([^}]*)\}/.exec(block)
  assert.ok(on, 'the open-stage rule is gone')
  assert.ok(!/#[0-9a-f]{3,8}|rgba?\(/i.test(on[1]), `the open stage is marked by a hue: ${on[1]}`)
})

test('a node whose own words are an internal detector name reaches the list in plain words (H2T-5)', async () => {
  /* The list is built by `labelsOf` inside the reader, so it is exercised
   * through the same path the screen uses: a label equal to a detector id must
   * render the copy file's phrase, and one with no entry must survive. */
  const model = buildChangeView({
    change: FULL,
    work: [{ id: SESSION_A, findings: [], focus: { summary: { total: 2, shown: 2, merged: 0 }, labels: [
      { id: 'n1', text: HQ_COPY.detectors.test_run_failed },
      { id: 'n2', text: 'Prompt seq 8' },
    ] } }],
  })
  const body = text(renderScreen({ model, stage: 'verify' }))
  assert.ok(body.includes('the test run failed'))
  assert.ok(!body.includes('test_run_failed'))
  assert.ok(body.includes('Prompt seq 8'))
})

test('the folded steps are accounted for in words, not left as a bare number', () => {
  /* "12 steps folded away" with nothing saying where they went is a count
   * with no subject (UX-DOCTRINE §4). `focusView` accounts for every node it
   * does not show, by category, with a count-bearing label; the screen renders
   * that accounting. */
  const model = buildChangeView({
    change: FULL,
    work: [{ id: SESSION_A, findings: [], focus: {
      summary: { total: 6, shown: 1, merged: 5 },
      labels: [{ id: 'n1', text: 'Prompt seq 8' }],
      /* The focus set's OWN label and line, verbatim — Trace's words, which
       * include two the banned list forbids here ("events", a tree hash). */
      groups: [
        { id: 'grp_focus_holds', label: '4 events that hold up', count: 4, line: 'reads, edits and runs with nothing against them', members: [], kind: 'holds' },
        { id: 'grp_focus_trees', label: '1 tree recorded', count: 1, line: 'tree 0d8a474fc679', members: [], kind: 'trees' },
      ],
    } }],
  })
  const body = text(renderScreen({ model, stage: 'verify' }))
  assert.ok(body.includes('5 steps folded away'))
  assert.ok(body.includes('4 steps where nothing was found against what was said'), 'the folded steps are not accounted for')
  assert.ok(body.includes('1 snapshot of the work'))
  /* And Trace's own words did not come with them. */
  assert.ok(!body.includes('events that hold up'))
  assert.ok(!body.includes('0d8a474fc679'))
  assert.ok(!body.includes('tree recorded'))
})

test('a folded category the copy file has no line for is dropped, never rendered in Trace\'s words', () => {
  const model = buildChangeView({
    change: FULL,
    work: [{ id: SESSION_A, findings: [], focus: {
      summary: { total: 3, shown: 1, merged: 2 },
      labels: [{ id: 'n1', text: 'Prompt seq 8' }],
      groups: [{ id: 'grp_focus_newthing', label: '2 ledger reads', count: 2, line: 'x', members: [], kind: 'newthing' }],
    } }],
  })
  /* Verify, where the focus set lives: on any other stage this assertion
     would pass because nothing renders there at all, which is a test that
     proves nothing. */
  const body = text(renderScreen({ model, stage: 'verify' }))
  assert.ok(body.includes('Prompt seq 8'), 'the focus set did not render, so the sweep below tests nothing')
  assert.ok(!body.includes('ledger reads'), 'an unknown category leaked Trace vocabulary onto the Plain surface')
})

/* ── fix round 1: the four had-to-think moments the walkthrough found ──── */

test('the focus set renders under Verify and under no other stage (T24-4)', () => {
  /* The walkthrough's pause: "The work behind this Change" sat below every
     stage panel, so on Verify a reader had to decide whether it belonged to
     Verify or to the Change. It answers Verify's question — "what happened
     when we checked it?" — and it now renders there and nowhere else. */
  const model = modelWith()
  for (const stage of STAGES) {
    const body = text(renderScreen({ model, stage }))
    const here = body.includes('The work behind this Change')
    assert.equal(here, stage === 'verify', `${stage}: the focus set is ${here ? 'here and should not be' : 'missing from Verify'}`)
  }
})

test('a focus set with nothing short of proven says so, instead of "0 of 1 steps shown" over an empty list (T24-5)', () => {
  const model = buildChangeView({
    change: FULL,
    work: [{ id: SESSION_A, findings: [], focus: { summary: { total: 1, shown: 0, merged: 1 }, labels: [], groups: [{ kind: 'holds', count: 1 }] } }],
  })
  const body = text(renderScreen({ model, stage: 'verify' }))
  assert.ok(!body.includes('0 of 1 steps shown'), 'a count over an empty list is a number with no sentence')
  /* The scoped wording, because this fixture's criteria have no evidence:
     "nothing is waiting on evidence" on a Change nobody has checked is the
     all-clear T-2-8-F5 found, and it now says what it is about. */
  assert.ok(body.includes(HQ_COPY.change.trace.none_shown_unchecked), 'nothing said what the empty list meant')
  /* The folded steps are still accounted for: the count did not vanish with
     the line that used to carry it. */
  assert.ok(body.includes('1 step where nothing was found against what was said'))

  /* And when there is no accounting either — the shape the real record
     produced on the walkthrough — the sentence does not point at lines that
     are not on the screen. */
  const bare = buildChangeView({
    change: FULL,
    work: [{ id: SESSION_A, findings: [], focus: { summary: { total: 1, shown: 0, merged: 1 }, labels: [], groups: [] } }],
  })
  const bareBody = text(renderScreen({ model: bare, stage: 'verify' }))
  assert.ok(bareBody.includes(HQ_COPY.change.trace.none_shown_nothing_folded_unchecked))
  assert.ok(!bareBody.includes('in the lines below'), 'the screen points at an accounting it does not show')
})

test('a version written down as all zeros is said to name nothing, never rendered as "version 0000000" (T24-3)', () => {
  const model = buildChangeView({
    change: aChange({
      implementation: {
        version: 1, bound_at: '2026-01-02T10:00:00Z', repository_identity: 'repo-synthetic-0001',
        base_revision: '0'.repeat(40), snapshot_id: null, workspace_identity: null,
      },
      accepted_scope: FULL.accepted_scope,
    }),
    work: [],
  })
  const body = text(renderScreen({ model, stage: 'define' }))
  assert.ok(!body.includes('version 0000000'), 'a placeholder revision was printed as if it were a version')
  assert.ok(body.includes(HQ_COPY.change.version.placeholder))
  /* And the Approve view names the same version the top names — one version,
     one sentence, in both places. */
  const approve = text(renderScreen({ model, stage: 'approve' }))
  assert.ok(!approve.includes('version 0000000'))
})

test('with work linked and nothing against it, the off control does not ask for the linking again (T24-7)', () => {
  /* Read off the real screen: "nothing has been checked for this Change. Link
     the work that was done to this Change" — on a Change with one confirmed
     piece of linked work. The same falsehood as T24-6, one control along. */
  const linked = buildChangeView({ change: FULL, work: [{ id: SESSION_A, findings: [], focus: null }] })
  const body = text(renderScreen({ model: linked, stage: 'verify' }))
  /* The scoped form of that reason, because this Change's criteria have no
     evidence (T-2-8-F5). It still does not ask for the linking again. */
  assert.ok(body.includes(HQ_COPY.change.blocked.nothing_to_say_unchecked.reason))
  assert.ok(!body.includes('nothing has been checked for this Change'), 'the screen says nothing was checked about work it just counted')

  /* And with nothing linked, the original reason is the true one. */
  const bare = buildChangeView({ change: aChange(), work: [] })
  const bareBody = text(renderScreen({ model: bare, stage: 'verify' }))
  assert.ok(bareBody.includes(HQ_COPY.change.blocked.nothing_checked.reason))
})

test('the finding card on this screen carries no raw id chips and no control that does nothing (T24-2)', () => {
  const carded = buildChangeView({
    change: FULL,
    work: [{ id: SESSION_A, findings: [{ ...CARD, proofNode: 'n-44' }], focus: null }],
  })
  const html = renderScreen({ model: carded, stage: 'verify' })
  assert.match(html, /ws-verdict-card/, 'the card itself is gone')
  assert.ok(!html.includes('ws-verdict-chip-ref'), 'the record’s own ids are chips on the Plain surface')
  assert.ok(!text(html).includes('r-44'), 'a receipt id reached the Plain surface')
  assert.ok(!html.includes('ws-verdict-proof'), 'a "Show proof" button with no viewer behind it is a control that does nothing')
  /* The sentence is still marked as resting on the record: hiding the chips
     did not turn a grounded line into an inference. */
  const before = html.slice(0, html.indexOf('Not proven: nothing in the record')).slice(-140)
  assert.ok(before.includes('class="ws-verdict-sentence"'), `the record line lost its own class: ${before}`)
  assert.ok(!before.includes('ws-verdict-inference'), 'the grounding of the sentence moved with its chips')
})

test('hiding the receipts is this screen asking, not the card changing: the card still renders them by default', () => {
  /* The break that matters here is the one that would silently take the chips
     off Trace too. The same card, rendered with no `receipts` prop, is the
     Trace rendering, and it keeps every chip. */
  const trace = renderCard({ card: { ...CARD, proofNode: 'n-44' }, register: 'operator', onShowProof: () => {}, vote: () => {}, voted: null })
  assert.ok(trace.includes('ws-verdict-chip-ref'), 'the default rendering lost its receipts')
  assert.ok(trace.includes('r-44'))
  assert.ok(trace.includes('ws-verdict-proof'), 'the proof control vanished where a viewer does exist')
  const off = renderCard({ card: { ...CARD, proofNode: 'n-44' }, register: 'operator', receipts: 'off', vote: () => {}, voted: null })
  assert.ok(!off.includes('ws-verdict-chip-ref'))
})

/* ── the strip under REAL key presses (fix round 2) ─────────────────────── *
 *
 * Everything above this line renders to static markup and reads attributes.
 * That is how the strip's ARIA wiring passed round 1 while being broken: the
 * attributes were right and DOM focus never moved, so a keyboard user's own
 * focus ring and a screen reader's cursor both stayed on the stage they
 * started from while the panel changed underneath. Attributes cannot show
 * that. Only pressing the key can.
 *
 * So these two tests mount the real component over a real DOM (jsdom), put
 * focus on a tab the way a person does, dispatch real `keydown` events with
 * real key names, and read `document.activeElement` after each one. The
 * second test is the negative control: the same drive over a build of the
 * same file with the focus call cut out, which must fail the way the shipped
 * build passes. A guard nobody has watched fail is not a guard.
 */

const KEY_DRIVE = `
  import React from 'react';
  import { createRoot } from 'react-dom/client';
  import { flushSync } from 'react-dom';
  import { StageStrip } from './change-workspace.jsx';
  export { React, createRoot, flushSync, StageStrip };
`

/** The focus call itself, cut out of the source — the negative control's only edit. */
const FOCUS_CALL = "if (el && typeof el.focus === 'function') el.focus()"

async function loadDrive(name, plugins) {
  const result = await build({ ...COMMON, stdin: { contents: KEY_DRIVE, resolveDir: here }, plugins })
  const file = join(temporary, name)
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}

/** A browser, borrowed for one test and given back. */
async function withDom(run) {
  const { JSDOM } = await import('jsdom')
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/' })
  const w = dom.window
  const names = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame']
  const held = names.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  for (const n of names) {
    if (w[n] === undefined) continue
    Object.defineProperty(globalThis, n, { value: n === 'window' ? w : w[n], configurable: true, writable: true })
  }
  try {
    return await run(w)
  } finally {
    for (const [n, d] of held) {
      if (d) Object.defineProperty(globalThis, n, d)
      else delete globalThis[n]
    }
    w.close()
  }
}

/**
 * Press keys at the strip and write down, after each one, where focus is,
 * which tab says it is selected, and which tab is the strip's tab stop.
 * Returns the transcript; asserts nothing, so both tests can read it.
 */
async function driveStrip(w, L) {
  const { React, createRoot, flushSync, StageStrip } = L
  const stages = modelWith().stages
  const Harness = () => {
    const [active, setActive] = React.useState('define')
    return React.createElement(StageStrip, { stages, active, onGo: setActive })
  }
  const root = createRoot(w.document.getElementById('root'))
  flushSync(() => root.render(React.createElement(Harness)))

  const id = (el) => (el && el.id ? el.id : String(el && el.tagName))
  const tabs = () => [...w.document.querySelectorAll('[role="tab"]')]
  const look = (key) => ({
    key,
    focused: id(w.document.activeElement),
    selected: id(tabs().find((b) => b.getAttribute('aria-selected') === 'true')),
    tabStop: id(tabs().find((b) => b.getAttribute('tabindex') === '0')),
  })

  /* A person arrives at the strip and the first tab takes focus. */
  w.document.getElementById('ws-change-tab-define').focus()
  const transcript = [look('(focus on Define)')]
  for (const key of ['ArrowRight', 'ArrowRight', 'ArrowLeft', 'End', 'Home', 'ArrowDown']) {
    w.document.activeElement.dispatchEvent(new w.KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
    await new Promise((r) => setTimeout(r, 0))
    transcript.push(look(key))
  }
  root.unmount()
  return transcript
}

/* Where each key must leave the person, in order, from Define. */
const EXPECTED = [
  ['(focus on Define)', 'define'],
  ['ArrowRight', 'prepare'],
  ['ArrowRight', 'build'],
  ['ArrowLeft', 'prepare'],
  ['End', 'approve'],
  ['Home', 'define'],
  ['ArrowDown', 'prepare'],
]

test('a real key press moves keyboard focus with the selection, across the whole strip', async () => {
  const transcript = await withDom(async (w) => driveStrip(w, await loadDrive('change-keys.mjs')))
  assert.equal(transcript.length, EXPECTED.length)
  for (let i = 0; i < EXPECTED.length; i += 1) {
    const [key, stage] = EXPECTED[i]
    const seen = transcript[i]
    const want = 'ws-change-tab-' + stage
    assert.equal(seen.key, key)
    assert.equal(seen.selected, want, `after ${key} the open stage is ${seen.selected}, not ${want}`)
    /* The three must agree. Focus is the one this screen used to drop. */
    assert.equal(seen.focused, want, `after ${key} keyboard focus is on ${seen.focused}, not on the open stage ${want}`)
    assert.equal(seen.tabStop, want, `after ${key} the strip's one tab stop is ${seen.tabStop}, not the open stage ${want}`)
  }
})

test('that drive is a live instrument: with the focus call cut out, it fails', async () => {
  let cut = 0
  const transcript = await withDom(async (w) => driveStrip(w, await loadDrive('change-keys-nofocus.mjs', [{
    name: 'cut-the-focus-call',
    setup(b) {
      b.onLoad({ filter: /change-workspace\.jsx$/ }, () => {
        const source = readFileSync(join(here, 'change-workspace.jsx'), 'utf8')
        cut = source.split(FOCUS_CALL).length - 1
        return { contents: source.split(FOCUS_CALL).join(''), loader: 'jsx' }
      })
    },
  }])))
  assert.equal(cut, 1, 'the control cut nothing out: the focus call it names is no longer in the source')

  /* Selection still moves — that is exactly why the round-1 attribute read
     passed — and focus is left behind on the stage the person started from. */
  const after = transcript[1]
  assert.equal(after.selected, 'ws-change-tab-prepare', 'the control broke more than the focus call')
  assert.equal(after.focused, 'ws-change-tab-define', 'the control did not reproduce the defect')
  assert.notEqual(after.focused, after.selected)
  assert.throws(() => {
    for (const s of transcript) assert.equal(s.focused, s.selected)
  }, /AssertionError/, 'the drive passed over a build with no focus call: it is not measuring focus')
})

/* ── T-2-8-F5: nothing reads as an all-clear where nothing was checked ──── */

/** The real shape: criteria accepted, work confirmed, and no check has run. */
const TWO_CONFIRMED = aChange({
  accepted_scope: FULL.accepted_scope,
  implementation: FULL.implementation,
  associations: [
    { id: 'a1', session_id: SESSION_A, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-01-02T10:01:00Z', reason: null },
    { id: 'a2', session_id: SESSION_B, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-01-02T10:02:00Z', reason: null },
  ],
})

const nothingChecked = () => buildChangeView({
  change: TWO_CONFIRMED,
  evidence: [],
  work: [
    { id: SESSION_A, findings: [], focus: { summary: { total: 1, shown: 0, merged: 1 }, labels: [], groups: [] } },
    { id: SESSION_B, findings: [], focus: { summary: { total: 1, shown: 0, merged: 1 }, labels: [], groups: [] } },
  ],
})

test('on a Change where nothing has been checked, no sentence reads as "there is nothing wrong" (T-2-8-F5)', () => {
  const model = nothingChecked()
  assert.equal(model.conclusion.key, 'no_evidence', 'the fixture is not the zero-evidence Change this is about')
  for (const stage of STAGES) {
    const body = text(renderScreen({ model, stage }))
    assert.deepEqual(
      absenceWithoutItsScope(body), [],
      `${stage}: a sentence says nothing is wrong without saying nothing was checked`,
    )
  }
})

test('both sentences carry their scope, and the copy file is where they come from', () => {
  const body = text(renderScreen({ model: nothingChecked(), stage: 'verify' }))
  assert.ok(
    body.includes(HQ_COPY.change.verify.nothing_wrong_unchecked_many.replace('{n}', '2')),
    `the scoped sentence is not on the screen: ${body}`,
  )
  assert.ok(
    body.includes(HQ_COPY.change.trace.none_shown_nothing_folded_unchecked),
    'the focus set sentence is not the scoped one',
  )
  /* And the disabled control below them, which carried the same reassurance. */
  assert.ok(body.includes(HQ_COPY.change.blocked.nothing_to_say_unchecked.reason))
  /* The unscoped wording is reachable — it is what a CHECKED Change says — but
     not from here. */
  assert.ok(!body.includes(HQ_COPY.change.verify.nothing_wrong_many.replace('{n}', '2')))
})

test('the rule is a live instrument: the unscoped sentences are exactly what it catches', () => {
  /* The screen before the fix, word for word out of T-2-8's DOM dump. */
  const before = 'Only work you confirmed belongs to this Change is counted here. From 2 pieces of linked work Nothing in the linked work contradicts what was said. Nothing in the work you confirmed is waiting on evidence. The steps the record holds are in the full trace.'
  assert.deepEqual(absenceWithoutItsScope(before), [
    'From 2 pieces of linked work Nothing in the linked work contradicts what was said.',
    'Nothing in the work you confirmed is waiting on evidence.',
  ])
  /* And a sentence that says its scope passes. */
  assert.deepEqual(absenceWithoutItsScope(HQ_COPY.change.verify.nothing_wrong_unchecked_one), [])
  assert.equal(ABSENCE_OF_PROBLEM.length, 2)
  assert.ok(SAYS_ITS_SCOPE.length > 0)
})

test('a Change that HAS been checked keeps the plain sentence, without the clause that would be untrue', () => {
  const checked = buildChangeView({
    change: TWO_CONFIRMED,
    evidence: [{ criterion: 1, state: 'proven' }],
    work: [
      { id: SESSION_A, findings: [], focus: { summary: { total: 1, shown: 0, merged: 1 }, labels: [], groups: [] } },
      { id: SESSION_B, findings: [], focus: { summary: { total: 1, shown: 0, merged: 1 }, labels: [], groups: [] } },
    ],
  })
  const body = text(renderScreen({ model: checked, stage: 'verify' }))
  assert.ok(body.includes(HQ_COPY.change.verify.nothing_wrong_many.replace('{n}', '2')), body)
  assert.ok(!body.includes(SAYS_ITS_SCOPE), 'the unchecked clause is on a Change that has been checked')
})

/* ── T9 Part A: a still-applies line has a Technical drawer ───────────── */

test('(t9.A1, t9.A2) a still-applies line has a Technical drawer naming what was compared and the one rule', () => {
  const C = M.HQ_COPY.change.coverage.carried
  const applies = M.renderLine({ criterion: { n: 1, text: 'The totals add up', state: 'proven', line: 'Proven', freshness: {
    fresh: true, sentence: 'Still applies — cart/ unchanged since 9f2ad3e.', technical: 'x', paths: [], checkedPaths: ['cart/'],
    reason: null, reasonTechnical: null, reasonClass: null,
    carried: { path: 'cart/', from: '9f2ad3e', to: 'e71c5b0' }, carriedTechnical: 'carried forward: cart/ unchanged 9f2ad3e→e71c5b0', rule: C.applicability_rule,
  } } })
  assert.match(applies, /<details class="ws-change-technical"/, 'an Applies line has a Technical drawer')
  const drawer = applies.slice(applies.indexOf('ws-change-technical'))
  assert.ok(drawer.includes('carried forward: cart/ unchanged 9f2ad3e→e71c5b0'), drawer)
  assert.ok(drawer.includes(C.applicability_rule), 'the drawer names the one rule')

  const stale = M.renderLine({ criterion: { n: 2, text: 'The totals add up', state: 'needs_recheck', line: 'Needs a re-check', freshness: {
    fresh: false, sentence: 'Proven on the previous version — needs a re-check.', technical: 't', paths: ['cart/total.js'], checkedPaths: ['cart/'],
    reason: 'The affected code changed afterward.', reasonTechnical: 'A source path … changed: cart/total.js.', reasonClass: 'applicability_reason_source_changed',
    carried: null, carriedTechnical: null, rule: C.applicability_rule,
  } } })
  const staleDrawer = stale.slice(stale.indexOf('ws-change-technical'))
  assert.ok(staleDrawer.includes(C.applicability_rule), 'the stale drawer names the rule too')
  assert.ok(staleDrawer.includes('<code>applicability_reason_source_changed</code>'), 'and the reason class slug')
  assert.ok(!staleDrawer.includes('carried forward'), 'a stale line carries nothing forward')
})

/* ── A3-F12 — the Inspector is never longer than about two viewports ─────
 *
 * Twelve finding cards and six accepted lines, the first of which has five
 * pieces of evidence behind it, one of them a failure with its own words.
 * The Inspector draws the leading card and the leading line's first piece in
 * full, keeps the failure in full wherever it sits, and folds the rest behind
 * one disclosure each whose summary says, in words, how many it holds.
 * Nothing is removed: the folded cards and items are all in the markup. */
const SIX = ['Uploads retry.', 'Retries back off.', 'Three tries at most.', 'The person is told.', 'Nothing is lost.', 'The log says why.']
const TWELVE = Array.from({ length: 12 }, (_, n) => ({ ...CARD, id: 'clm_' + (n + 1) }))
const item = (n, observed, extra) => ({
  link_id: 'l' + n, evidence_id: 'e' + n, session_id: SESSION_A, basis: 'declared', kind: 'check_run', name: 'check ' + n,
  observed, bears: true, applies: true, applies_slug: 'inputs_applies', ...(extra || {}),
})
const FOLD_CHANGE = aChange({ ...FULL, accepted_scope: { ...FULL.accepted_scope, criteria: SIX } })
const FOLD_COVERAGE = {
  scope_version: 1, link_rows: 5,
  criteria: SIX.map((text, i) => ({
    index: i + 1, text, state: 'evidence',
    items: i === 0 ? [item(1, 'proven'), item(2, 'proven'), item(3, 'not_proven', { failure_line: 'FAILED retry_backoff: expected 3 attempts, saw 1' }), item(4, 'proven'), item(5, 'proven')] : [],
  })),
}
const foldModel = () => {
  const work = [{ id: SESSION_A, findings: TWELVE, focus: null }]
  const model = buildChangeView({ change: FOLD_CHANGE, work })
  return { ...model, hq: M.deriveChangeModel(FOLD_CHANGE, FOLD_COVERAGE, null, null, work) }
}

test('A3-F12 · the Inspector folds cards after the leading one, and says how many it folded', async () => {
  const { JSDOM } = await import('jsdom')
  const html = M.renderShell({ model: foldModel(), stage: 'define', altitude: 'brief', register: 'manager' })
  const d = new JSDOM('<!doctype html><body>' + html + '</body>').window.document
  const inspector = d.querySelector('.ws-change-inspector')
  assert.ok(inspector, 'the Inspector is drawn at Brief')
  const I = HQ_COPY.change.model.inspector
  const all = [...inspector.querySelectorAll('.ws-verdict-card')]
  const open = all.filter((c) => !c.closest('details'))
  assert.equal(all.length, 12, 'every card is still in the Inspector: folded, not removed')
  assert.equal(open.length, 1, 'only the leading card is drawn in full')
  const cardFold = [...inspector.querySelectorAll('details.ws-inspector-fold')].find((el) => el.querySelector('.ws-verdict-card'))
  assert.ok(cardFold, 'the other cards sit behind one disclosure')
  assert.equal(cardFold.querySelector('summary').textContent.trim(), M.fill(I.fold_cards_many, { n: 11 }))

  /* The leading line's evidence: the first piece and the failure in full, the other three folded. */
  const items = [...inspector.querySelectorAll('.ws-inspector-items > li, .ws-inspector-fold .ws-inspector-items > li')]
  assert.equal(items.length, 5, 'every piece of evidence is still in the Inspector')
  const shown = items.filter((li) => !li.closest('details'))
  assert.deepEqual(shown.map((li) => li.querySelector('.ws-inspector-item-name').textContent), ['check 1', 'check 3'])
  assert.ok(shown[1].textContent.includes('FAILED retry_backoff'), 'the failure is never folded, and its own words are drawn')
  const itemFold = [...inspector.querySelectorAll('details.ws-inspector-fold')].find((el) => el.querySelector('.ws-inspector-items'))
  assert.equal(itemFold.querySelector('summary').textContent.trim(), M.fill(I.fold_items_many, { n: 3 }))
})

test('A3-F12 · nothing folds when there is nothing to fold: one card, one piece of evidence', async () => {
  const { JSDOM } = await import('jsdom')
  const work = [{ id: SESSION_A, findings: [CARD], focus: null }]
  const coverage = { scope_version: 1, link_rows: 1, criteria: [{ index: 1, text: FULL.accepted_scope.criteria[0], state: 'evidence', items: [item(1, 'proven')] }, { index: 2, text: FULL.accepted_scope.criteria[1], state: 'none', items: [] }] }
  const model = { ...buildChangeView({ change: FULL, work }), hq: M.deriveChangeModel(FULL, coverage, null, null, work) }
  const d = new JSDOM('<!doctype html><body>' + M.renderShell({ model, stage: 'define', altitude: 'brief', register: 'manager' }) + '</body>').window.document
  assert.equal(d.querySelectorAll('.ws-change-inspector details.ws-inspector-fold').length, 0)
  assert.equal(d.querySelectorAll('.ws-change-inspector .ws-verdict-card').length, 1)
})

/* ── A3-F15 — below 800 px the Brief headline stays ─────────────────────
 *
 * The static twin of `scripts/testing/brief-390.test.mjs` (which measures it
 * at 390 px in Chrome): whatever the Change's state, the Brief renders the
 * one sentence, and draws no graph at all. Width is CSS; what can be decided
 * without a browser is that the sentence is in the markup and a canvas is not. */
test('A3-F15 · the Brief headline is always rendered and no canvas is drawn', () => {
  const at = (items) => ({ scope_version: 1, link_rows: items.length, criteria: [{ index: 1, text: FULL.accepted_scope.criteria[0], state: items.length ? 'evidence' : 'none', items }, { index: 2, text: FULL.accepted_scope.criteria[1], state: 'none', items: [] }] })
  const cases = [
    ['proven', FULL, at([item(1, 'proven', { fresh: true })])],
    ['needs a re-check', FULL, at([item(1, 'proven', { fresh: false, fresh_slug: 'applicability_needs_a_recheck' })])],
    ['not proven', FULL, at([item(1, 'not_proven', { failure_line: 'FAILED retry: expected 3 attempts, saw 1' })])],
    ['nothing accepted', aChange(), null],
  ]
  for (const [what, change, coverage] of cases) {
    const model = { ...buildChangeView({ change, work: [] }), hq: M.deriveChangeModel(change, coverage, null, null, []) }
    const html = M.renderShell({ model, stage: 'define', altitude: 'brief', register: 'manager' })
    const conclusion = /<p class="ws-change-conclusion">([^<]*)<\/p>/.exec(html)
    assert.ok(conclusion && conclusion[1].trim().length > 0, `${what}: the Brief has no headline sentence`)
    assert.equal(conclusion[1], model.hq.headline.text.replace(/&/g, '&amp;').replace(/'/g, '&#x27;').replace(/"/g, '&quot;'), `${what}: the headline is not the model's`)
    assert.match(html, /<h1 class="ws-change-title">/, `${what}: the Brief has no title`)
    assert.doesNotMatch(html, /<canvas|ws-canvas/, `${what}: a graph is drawn on the Brief`)
  }
})

/* ── STEP-7-LIMITS — the record's limit, in words, from one derivation ────
 *
 * PW-F12 (path walk step 7): after the agent committed, the Change read
 * exactly as before and nothing said that ThruLine had not looked. ThruLine
 * records the code only when the Change is made or a check runs; the screen
 * now says so at the top (Brief and Explore) and beside Approve, and reads
 * nothing from git to say it. Every surface prints `hq.codeLimit.lines` and
 * nothing else — the probe below replaces the field and every surface
 * follows it, which a surface reaching into the copy file directly would not. */

const LIMIT_CLAIMS_MORE = /unchanged|not changed|hasn't changed|has not moved|nothing[^.]*changed|still applies|up to date|current code/i

const limitModel = () => {
  const coverage = { scope_version: 1, link_rows: 1, criteria: [{ index: 1, text: FULL.accepted_scope.criteria[0], state: 'evidence', items: [item(1, 'proven', { fresh: true })] }, { index: 2, text: FULL.accepted_scope.criteria[1], state: 'none', items: [] }] }
  return { ...buildChangeView({ change: FULL, work: [] }), hq: M.deriveChangeModel(FULL, coverage, null, null, []) }
}

test('STEP-7-LIMITS · the limit is one derived field, in the copy file\'s words, and it claims nothing about whether the code moved', () => {
  const hq = limitModel().hq
  const L = HQ_COPY.change.model.code_limit
  assert.ok(hq.codeLimit, 'the model derives no limit sentence')
  assert.ok(Array.isArray(hq.codeLimit.lines) && hq.codeLimit.lines.length === 1, 'exactly one line: nothing is read from git, so there is nothing to add to it')
  const expected = hq.version.short ? fill(L.named, { version: hq.version.short }) : L.unnamed
  assert.equal(hq.codeLimit.lines[0], expected)
  assert.equal(hq.codeLimit.text, hq.codeLimit.lines.join(' '))
  for (const s of [L.named, L.unnamed]) {
    assert.doesNotMatch(s, LIMIT_CLAIMS_MORE, `the limit reads as an all-clear: ${s}`)
    assert.match(s, /check/, 'the limit does not say that running the check records what is there now')
    assert.match(s, /not been looked at/, 'the limit does not say later commits are not looked at')
  }
})

test('STEP-7-LIMITS · Brief and Explore say the limit at the top, and it sits beside Approve on both', async () => {
  const { JSDOM } = await import('jsdom')
  const model = limitModel()
  model.hq = { ...model.hq, codeLimit: { lines: ['probe-code-limit'], text: 'probe-code-limit' } }

  const brief = new JSDOM('<!doctype html><body>' + M.renderShell({ model, stage: 'define', altitude: 'brief', register: 'manager' }) + '</body>').window.document
  const story = brief.querySelector('.ws-brief-story')
  assert.ok(story && story.textContent.includes('probe-code-limit'), 'the top of Brief does not say the limit')
  const footer = brief.getElementById('ws-change-decision')
  assert.ok(footer && footer.textContent.includes('probe-code-limit'), 'the space beside Approve on Brief does not say the limit')

  for (const stage of ['define', 'approve']) {
    const explore = new JSDOM('<!doctype html><body>' + M.renderShell({ model, stage, altitude: 'explore', register: 'manager' }) + '</body>').window.document
    const inner = explore.querySelector('.ws-change-inner')
    const top = [...inner.children].slice(0, [...inner.children].findIndex((el) => el.matches('nav, [role="tablist"], .ws-change-stages')))
    assert.ok(top.some((el) => el.textContent.includes('probe-code-limit')), `${stage}: the top of Explore does not say the limit`)
  }
  const approve = new JSDOM('<!doctype html><body>' + M.renderShell({ model, stage: 'approve', altitude: 'explore', register: 'manager' }) + '</body>').window.document
  const panel = approve.querySelector('.ws-change-panel')
  assert.ok(panel.textContent.includes('probe-code-limit'), 'the Approve stage does not say the limit beside Approve')
})

/* ── STEP-7-LIMITS · PW-F13 — one count of the work actually read ─────── */

test('PW-F13 · "Nothing in the N pieces of work" counts the conversations read, not the check run linked beside them', () => {
  /* One conversation and one check-execution session, both confirmed. The
   * execution is never read as work (`readChangeDocuments` skips it), so the
   * sentence about what the linked work contradicts is about ONE piece. */
  const change = aChange({
    accepted_scope: FULL.accepted_scope,
    implementation: FULL.implementation,
    associations: [
      { id: 'a1', session_id: SESSION_A, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-01-02T10:01:00Z', reason: null },
      { id: 'a2', session_id: SESSION_B, provenance: 'confirmed', basis: 'check', actor: 'thruline', asserted_at: '2026-01-02T10:02:00Z', reason: null },
    ],
  })
  const model = buildChangeView({
    change,
    evidence: [{ criterion: 1, state: 'proven' }],
    work: [{ id: SESSION_A, findings: [], focus: { summary: { total: 1, shown: 0, merged: 1 }, labels: [], groups: [] } }],
  })
  const body = text(renderScreen({ model, stage: 'verify' }))
  assert.ok(body.includes(HQ_COPY.change.verify.nothing_wrong_one), body)
  assert.ok(!body.includes(fill(HQ_COPY.change.verify.nothing_wrong_many, { n: 2 })), 'the check run is counted as a piece of work read')
})

test('PW-F13 · with only a check run linked, no sentence counts "pieces of work" that were never read', () => {
  const change = aChange({
    accepted_scope: FULL.accepted_scope,
    implementation: FULL.implementation,
    associations: [
      { id: 'a2', session_id: SESSION_B, provenance: 'confirmed', basis: 'check', actor: 'thruline', asserted_at: '2026-01-02T10:02:00Z', reason: null },
    ],
  })
  const model = buildChangeView({ change, evidence: [{ criterion: 1, state: 'proven' }], work: [] })
  const body = text(renderScreen({ model, stage: 'verify' }))
  assert.ok(body.includes(HQ_COPY.change.verify.nothing_read), body)
  assert.ok(!body.includes(HQ_COPY.change.verify.nothing_wrong_one), 'a check run is counted as one piece of work read')
  assert.ok(!/Nothing in the 0 pieces/.test(body), 'a count of nothing read reads as an all-clear')
})

/* ── STEP-7-LIMITS-R2 — every count of linked work on one screen is one number ─
 *
 * Round 1's adversary: the Verify stage printed "From 2 pieces of linked work"
 * (every confirmed session, check runs included) two sentences above "Nothing
 * in the 1 piece of work you linked…" (the conversations read). Every sentence
 * that counts linked work now counts the same set — the conversations read —
 * and a check-execution session is never one of them. */

const LINKED_WORK_COUNT = /(\d+) pieces? of (?:linked work|work you linked)/g
const linkedWorkCounts = (body) => [...body.matchAll(LINKED_WORK_COUNT)].map((m) => Number(m[1]))

const conversationAndCheckRun = () => aChange({
  accepted_scope: FULL.accepted_scope,
  implementation: FULL.implementation,
  associations: [
    { id: 'a1', session_id: SESSION_A, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-01-02T10:01:00Z', reason: null },
    { id: 'a2', session_id: SESSION_B, provenance: 'confirmed', basis: 'check', actor: 'thruline', asserted_at: '2026-01-02T10:02:00Z', reason: null },
  ],
})

test('STEP-7-LIMITS-R2 · every stage gives one number for the work you linked: the conversations read, never the check run beside them', () => {
  const model = buildChangeView({
    change: conversationAndCheckRun(),
    evidence: [{ criterion: 1, state: 'proven' }],
    work: [{ id: SESSION_A, findings: [], focus: { summary: { total: 1, shown: 0, merged: 1 }, labels: [], groups: [] } }],
  })
  let seen = 0
  for (const stage of STAGES) {
    const counts = linkedWorkCounts(text(renderScreen({ model, stage })))
    seen += counts.length
    assert.deepEqual([...new Set(counts)].filter((n) => n !== 1), [], `stage ${stage} counts linked work as ${counts.join(' and ')}, not the 1 conversation read`)
  }
  assert.ok(seen >= 2, 'the Verify stage no longer states the count at all')
})

test('STEP-7-LIMITS-R2 · with only a check run linked, no stage counts linked work with a number', () => {
  const change = aChange({
    accepted_scope: FULL.accepted_scope,
    implementation: FULL.implementation,
    associations: [
      { id: 'a2', session_id: SESSION_B, provenance: 'confirmed', basis: 'check', actor: 'thruline', asserted_at: '2026-01-02T10:02:00Z', reason: null },
    ],
  })
  const model = buildChangeView({ change, evidence: [{ criterion: 1, state: 'proven' }], work: [] })
  for (const stage of STAGES) {
    const body = text(renderScreen({ model, stage }))
    assert.deepEqual(linkedWorkCounts(body), [], `stage ${stage} counts a check run as linked work read: ${body}`)
  }
  assert.ok(text(renderScreen({ model, stage: 'verify' })).includes(HQ_COPY.change.verify.nothing_read), 'Verify no longer says that nothing was read')
})

/* ── STEP-7-GIT-LINE — the read-time git line (R-2026-09-28-42) ───────────
 *
 * Founder ruling #21 (a): "The read-time git line, cautious direction only,
 * with its receipt shown." The served Change carries `code_moved` — the HEAD
 * recorded with the version against the HEAD of the recorded folder now — and
 * ONE derivation turns it into the sentence every surface prints, FIRST in
 * `hq.codeLimit.lines`, beside the limit sentence and never instead of it.
 * The receipt (the two commits) sits one level down. It can only make the
 * record claim LESS: nothing about readiness, coverage, a state word, a
 * decision or approval moves because of it, and equal heads render nothing. */

const MOVED = {
  recorded: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0',
  now: 'f0e1d2c3b4a5968778695a4b3c2d1e0f9a8b7c6d',
  changed_paths: ['src/lib.rs'],
  commits: 1,
  uncommitted_compared: false,
}

const MOVED_CLAIMS_MORE = /unchanged|nothing changed|nothing has changed|not changed|still applies|up to date/i

const movedModel = (moved) => {
  const change = moved === undefined ? FULL : { ...FULL, code_moved: moved }
  const coverage = { scope_version: 1, link_rows: 1, criteria: [{ index: 1, text: FULL.accepted_scope.criteria[0], state: 'evidence', items: [item(1, 'proven', { fresh: true })] }, { index: 2, text: FULL.accepted_scope.criteria[1], state: 'none', items: [] }] }
  return { ...buildChangeView({ change, work: [] }), hq: M.deriveChangeModel(change, coverage, null, null, []) }
}

test('STEP-7-GIT-LINE · a served move becomes one sentence, first, naming the version, the count and the path, beside the limit', () => {
  const hq = movedModel(MOVED).hq
  assert.ok(hq.codeMoved, 'the model derives no moved line from the served field')
  assert.equal(hq.codeLimit.moved, hq.codeMoved, 'two derivations of one field')
  const [first, second] = hq.codeLimit.lines
  assert.equal(hq.codeLimit.lines.length, 2, hq.codeLimit.lines.join(' | '))
  assert.equal(first, hq.codeMoved.sentence)
  assert.ok(hq.version.short && first.includes(hq.version.short), `the version is not named: ${first}`)
  assert.ok(first.includes('src/lib.rs'), `the path is not named: ${first}`)
  assert.match(first, /1 file/)
  assert.match(first, /1 commit/)
  assert.match(first, /check/, 'the line does not say what would look at it')
  assert.equal(second, movedModel(undefined).hq.codeLimit.lines[0], 'the limit sentence is replaced instead of kept beside')
  /* The receipt, one level down: both commits, and what was not compared. */
  const receipt = hq.codeMoved.receipt
  assert.ok(receipt.includes(MOVED.recorded) && receipt.includes(MOVED.now), `the receipt does not name both commits: ${receipt}`)
  assert.ok(hq.codeMoved.uncommitted && /not compared/.test(hq.codeMoved.uncommitted), 'the technical layer does not say uncommitted edits are not compared')
})

test('STEP-7-GIT-LINE · equal heads (no field, or null) render exactly what the base rendered', () => {
  for (const moved of [undefined, null]) {
    const hq = movedModel(moved).hq
    assert.equal(hq.codeMoved, null)
    assert.equal(hq.codeLimit.lines.length, 1)
  }
})

test('STEP-7-GIT-LINE · cautious only: readiness, counts, state words, decision and approval are byte-identical with the field on and off', () => {
  const on = movedModel(MOVED).hq
  const off = movedModel(null).hq
  /* D-050 (APPLIES-VS-MOVED). `freshness` used to be pinned byte-identical with
   * the field on and off, and that pinned the defect: a line that still applies
   * said "Still applies — … unchanged since …" beside "The code has changed
   * since …". With the field on, exactly two fields of a line that still
   * applies differ — the sentence and the Technical carried-forward line — and
   * each is exactly the framed sentence below. Every other field, of every
   * line, is still byte-identical. (Old: `c.freshness`. New: `frame(c.freshness)`
   * on the off side.) */
  const frame = (f) => (f && f.fresh
    ? {
      ...f,
      sentence: `Still applies to version ${off.version.short} as recorded. The code has changed since.`,
      carriedTechnical: f.carriedTechnical ? f.carriedTechnical + ' These are recorded versions; the code has changed since.' : f.carriedTechnical,
    }
    : f)
  const surface = (hq, framed) => JSON.stringify({
    readiness: hq.readiness,
    counts: hq.counts,
    state: hq.state,
    word: hq.word,
    headline: hq.headline,
    criteria: hq.criteria.map((c) => [c.n, c.state, c.word, c.line, framed ? frame(c.freshness) : c.freshness]),
    decision: hq.decision,
    action: hq.inspector.action,
    inboxRow: hq.inboxRow,
    attention: hq.attention,
    readinessPacket: [hq.packet.readiness, hq.packet.readinessLine, hq.packet.readinessRests, hq.packet.decision],
  })
  assert.equal(surface(on), surface(off, true))
})

test('STEP-7-GIT-LINE · no word it can print says the code is unchanged or that anything still applies', () => {
  const C = HQ_COPY.change.model.code_moved
  const walk = (v) => (typeof v === 'string' ? [v] : Object.values(v).flatMap(walk))
  for (const s of walk(C)) assert.doesNotMatch(s, MOVED_CLAIMS_MORE, s)
  const hq = movedModel(MOVED).hq
  for (const s of [hq.codeMoved.sentence, hq.codeMoved.receipt, hq.codeMoved.uncommitted]) assert.doesNotMatch(s, MOVED_CLAIMS_MORE, s)
})

test('STEP-7-GIT-LINE · Brief and Explore say it at the top, with the receipt one level down', async () => {
  const { JSDOM } = await import('jsdom')
  const model = movedModel(MOVED)
  const sentence = model.hq.codeMoved.sentence
  const brief = new JSDOM('<!doctype html><body>' + M.renderShell({ model, stage: 'define', altitude: 'brief', register: 'manager' }) + '</body>').window.document
  const story = brief.querySelector('.ws-brief-story')
  assert.ok(story && story.textContent.includes(sentence), 'the top of Brief does not say the code moved')
  const details = [...story.querySelectorAll('details')].find((d) => d.textContent.includes(MOVED.now))
  assert.ok(details, 'the receipt is not one level down on Brief')
  assert.ok(details.textContent.includes(MOVED.recorded), 'the recorded commit is not in the receipt')

  const explore = new JSDOM('<!doctype html><body>' + M.renderShell({ model, stage: 'define', altitude: 'explore', register: 'manager' }) + '</body>').window.document
  const inner = explore.querySelector('.ws-change-inner')
  const top = [...inner.children].slice(0, [...inner.children].findIndex((el) => el.matches('nav, [role="tablist"], .ws-change-stages')))
  assert.ok(top.some((el) => el.textContent.includes(sentence)), 'the top of Explore does not say the code moved')
  assert.ok(top.some((el) => el.matches('details') && el.textContent.includes(MOVED.now)), 'the receipt is not one level down on Explore')
})
