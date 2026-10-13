import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* T-3-2b — what this Change covers, what the check command is, and what a
 * Change with nothing bound to it says.
 *
 * Three screens, one question each:
 *
 *  1. DEFINE says which folders the Change covers, before anybody accepts
 *     them. Until this card the product could not say it at all, and the scope
 *     check refused every piece of work with no surface explaining why.
 *  2. THE FIRST SCREEN shows the same folders beside what counts as done, so
 *     the one click a person makes covers both — nobody can agree to folders
 *     they were never shown.
 *  3. VERIFY says "nothing is bound to this Change yet" when that is why no
 *     item says which version it was checked on, and names the one act that
 *     changes it.
 *
 * The prepared command is asserted to be ONE text across two screens, by
 * rendering both and comparing what each shows.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-covers-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeScreen } from './change-workspace.jsx';
  import { FirstUseScreen } from './first-use.jsx';
  import { buildChangeView, coversOf } from './change-view.js';
  import { buildCoverageView, runCheckCommand } from './coverage-view.js';
  import { HQ_COPY } from './hq-copy.js';
  export { buildChangeView, coversOf, buildCoverageView, runCheckCommand, HQ_COPY };
  export const renderScreen = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderFirstUse = props => renderToStaticMarkup(React.createElement(FirstUseScreen, props));
`

const result = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  stdin: { contents: ENTRY, resolveDir: here },
})
const file = join(temporary, 'covers.mjs')
await writeFile(file, result.outputFiles[0].text)
const M = await import(pathToFileURL(file).href)
const { buildChangeView, coversOf, buildCoverageView, runCheckCommand, HQ_COPY, renderScreen, renderFirstUse } = M

const CHANGE_ID = 'cccccccc-0000-4000-8000-00000000c001'
const text = (html) => html.replace(/<[^>]*>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/\s+/g, ' ').trim()

function aChange(over = {}) {
  return {
    id: CHANGE_ID,
    project_id: 'cccccccc-0000-4000-8000-00000000p001',
    opened_at: '2026-01-02T09:00:00Z',
    request: { title: 'Retry failed uploads', purpose: 'Uploads that fail once are lost.', requester: 'sam', source: 'person' },
    accepted_scope: null,
    implementation: null,
    associations: [],
    examination: 'none_yet',
    decision: 'none_yet',
    masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
    ...over,
  }
}

const PROPOSALS = {
  criteria: [{ number: 1, text: 'A failed upload is tried again three times.' }],
  scope: { paths: ['src/upload/**'], from_files: 2, not_shown: 0, bound: 25, reason_slug: 'scope_from_the_work', empty_slug: null, over_bound_slug: null },
}

/* ── the model ─────────────────────────────────────────────────────────── */

test('accepted folders are what the Change covers, and they are marked as accepted', () => {
  const covers = coversOf({ allowedScope: ['src/upload/**'] }, PROPOSALS)
  assert.deepEqual(covers.paths, ['src/upload/**'])
  assert.equal(covers.accepted, true)
})

test('with nothing accepted, the proposed folders are shown and marked as not accepted', () => {
  const covers = coversOf({ allowedScope: [] }, PROPOSALS)
  assert.deepEqual(covers.paths, ['src/upload/**'])
  assert.equal(covers.accepted, false)
  assert.equal(covers.reasonSlug, 'scope_from_the_work')
})

test('a Change with neither covers nothing, and says so rather than guessing', () => {
  const covers = coversOf({ allowedScope: [] }, { criteria: [], scope: { paths: [], empty_slug: 'scope_none_found' } })
  assert.deepEqual(covers.paths, [])
  assert.equal(covers.emptySlug, 'scope_none_found')
  /* Nothing becomes a wildcard when the record is silent. */
  assert.ok(!covers.paths.includes('**'))
})

/* ── Define ────────────────────────────────────────────────────────────── */

test('Define says which folders this Change covers, in the copy file words', () => {
  const model = buildChangeView({ change: aChange(), proposals: PROPOSALS, work: [] })
  const body = text(renderScreen({ model, stage: 'define' }))
  assert.ok(body.includes(HQ_COPY.change.define.covers_heading), body)
  assert.ok(body.includes('src/upload/**'), body)
  assert.ok(body.includes(HQ_COPY.change.define.covers_proposed_note), body)
})

test('a Change that covers nothing says what would cover it, never a blank list', () => {
  const model = buildChangeView({ change: aChange(), proposals: { criteria: [], scope: { paths: [], empty_slug: 'scope_none_found' } }, work: [] })
  const body = text(renderScreen({ model, stage: 'define' }))
  assert.ok(body.includes(HQ_COPY.change.define.covers_none), body)
})

test('the accepted folders read as accepted and say what falls outside them', () => {
  const model = buildChangeView({
    change: aChange({
      accepted_scope: {
        version: 1,
        criteria: ['A failed upload is tried again three times.'],
        exclusions: [], allowed_scope: ['src/upload/**'], constraints: [], check_profile: [],
        content_sha256: 'x',
        acceptance: { actor: 'sam', accepted_at: '2026-01-02T09:06:00Z', rationale: 'yes' },
        proposed_at: '2026-01-02T09:05:00Z',
      },
    }),
    proposals: PROPOSALS,
    work: [],
  })
  const body = text(renderScreen({ model, stage: 'define' }))
  assert.ok(body.includes(HQ_COPY.change.define.covers_accepted_note), body)
})

/* ── nothing bound ─────────────────────────────────────────────────────── */

const coverageDoc = (over = {}) => ({
  scope_version: 1,
  criteria: [{ index: 1, text: 'A failed upload is tried again three times.', class: 'test', state: 'none', state_slug: 'coverage_none', absent_tool: null, items: [], removed: [] }],
  candidates: [],
  link_rows: 0,
  nothing_to_link_slug: null,
  nothing_bound_slug: null,
  execution_sessions: [],
  ...over,
})

test('a Change with nothing bound says so, and offers the one act that binds it', () => {
  const model = buildChangeView({ change: aChange(), proposals: PROPOSALS, work: [] })
  model.coverage = buildCoverageView(coverageDoc({ nothing_bound_slug: 'inputs_nothing_bound' }))
  const body = text(renderScreen({ model, stage: 'verify' }))
  assert.ok(body.includes(HQ_COPY.change.coverage.nothing_bound), body)
  assert.ok(body.includes('thruline change prepare ' + CHANGE_ID), body)
})

test('a Change that is bound says nothing about binding', () => {
  const model = buildChangeView({ change: aChange(), proposals: PROPOSALS, work: [] })
  model.coverage = buildCoverageView(coverageDoc())
  const body = text(renderScreen({ model, stage: 'verify' }))
  assert.ok(!body.includes(HQ_COPY.change.coverage.nothing_bound), body)
})

test('an execution is not asked for a transcript it never had', () => {
  const view = buildCoverageView(coverageDoc({ execution_sessions: ['cli:check:2026-09-19T06:45:01Z'] }))
  assert.deepEqual(view.executionSessions, ['cli:check:2026-09-19T06:45:01Z'])
  /* And the loader is the thing that must read it: the source names the
     field, so the skip cannot be lost in a refactor without this failing. */
  const source = readFileSync(new URL('./change-workspace.jsx', import.meta.url), 'utf8')
  assert.ok(source.includes('execution_sessions'), 'the loader no longer reads which linked sessions are executions')
  assert.ok(
    source.includes('if (executions.has(String(sessionId))) continue'),
    'the loader reads the list and then asks an execution for a transcript anyway'
  )
})

/* ── one prepared command, two screens ─────────────────────────────────── */

test('the prepared check command names the Change and leaves the command to the person', () => {
  const command = runCheckCommand(CHANGE_ID)
  assert.ok(command.startsWith('thruline check --for-change ' + CHANGE_ID), command)
  assert.ok(command.includes('--'), command)
  /* This product never chooses what to run. */
  assert.ok(!/cargo|npm|pytest|make /.test(command), command)
})

test('the first screen and the Change screen offer the same command, from the same builder', () => {
  const onFirst = text(renderFirstUse({
    state: { status: 'ready', step: 'done', change: CHANGE_ID, criteria: [{ text: 'A failed upload is tried again three times.' }], covers: ['src/upload/**'] },
  }))
  const model = buildChangeView({ change: aChange(), proposals: PROPOSALS, work: [] })
  model.coverage = buildCoverageView(coverageDoc())
  const onChange = text(renderScreen({ model, stage: 'verify' }))
  const command = runCheckCommand(CHANGE_ID)
  assert.ok(onFirst.includes(command), onFirst)
  assert.ok(onChange.includes(command), onChange)
})

/* ── the first screen ──────────────────────────────────────────────────── */

test('the first screen shows the folders the one click covers, before it is clicked', () => {
  const body = text(renderFirstUse({
    state: { status: 'ready', step: 'done', change: CHANGE_ID, criteria: [{ text: 'A failed upload is tried again three times.' }], covers: ['src/upload/**'] },
  }))
  assert.ok(body.includes(HQ_COPY.first_use.done.covers_heading), body)
  assert.ok(body.includes('src/upload/**'), body)
  assert.ok(body.includes(HQ_COPY.first_use.done.covers_note), body)
  /* And the button that does it is still the same one, still secondary. */
  assert.ok(body.includes(HQ_COPY.first_use.done.criteria_action), body)
})

test('the first screen says when the record shows no folders at all', () => {
  const body = text(renderFirstUse({
    state: { status: 'ready', step: 'done', change: CHANGE_ID, criteria: [{ text: 'A failed upload is tried again three times.' }], covers: [] },
  }))
  assert.ok(body.includes(HQ_COPY.first_use.done.covers_none), body)
})

test('one primary action on the first screen, with the prepared command beside it and not competing', () => {
  const html = renderFirstUse({
    state: { status: 'ready', step: 'done', change: CHANGE_ID, criteria: [{ text: 'A failed upload is tried again three times.' }], covers: ['src/upload/**'] },
  })
  assert.equal((html.match(/class="ws-inbox-primary"/g) || []).length, 1, html)
})

/* ── one screen, one meaning for "version" (T-3-2b r1) ─────────────────── */

/* The Change screen already spends that word on one record: the version of the
 * work bound to the Change (`change.version.*`). Until this round the
 * applicability sentences this card put on the same screen spent it on a
 * second, unrelated record, so a first-time reader could see
 *
 *   "No version of the work is bound to this Change yet …"      (the top)
 *   "Checked on this version."                                  (an item)
 *
 * at the same time, with nothing saying they are about different things. These
 * tests are the fence: the copy file first, then the rendered screen in
 * exactly that state.
 */

/** Every sentence on these screens that is allowed to say "version", and the
 *  one record it names: the version of the work. */
const THE_WORK_S_VERSION = [
  HQ_COPY.change.version.none,
  HQ_COPY.change.version.bound,
  HQ_COPY.change.version.placeholder,
  HQ_COPY.change.version.label,
]

/** Everything the rendered screen is allowed to say "version" about: the
 *  version of the work, and ThruLine's own version where the sentence names
 *  ThruLine, which nobody can read as a claim about their code. */
function withoutTheWorksVersion(body) {
  let rest = body
  for (const sentence of THE_WORK_S_VERSION) rest = rest.split(sentence).join(' ')
  return rest.replace(/version of ThruLine/g, 'build of ThruLine')
}

test('no sentence about what was examined borrows the word the work’s version owns', () => {
  const examined = [
    ...Object.values(HQ_COPY.change.coverage.applies),
    HQ_COPY.change.coverage.states.other_version,
    HQ_COPY.change.coverage.nothing_bound,
    HQ_COPY.change.coverage.nothing_bound_action,
    ...Object.values(HQ_COPY.change.packet.mapping),
  ]
  for (const sentence of examined) {
    assert.ok(
      !/version/i.test(String(sentence)),
      'an examined-code sentence says "version", which on this screen names the work: ' + sentence,
    )
  }
  /* And the number of an accepted list is not a version either — it is on the
     same screen as the line above. */
  for (const sentence of [
    HQ_COPY.change.define.accepted_note,
    HQ_COPY.first_use.done.criteria_accepted,
    HQ_COPY.change.packet.accepted.note_full,
    HQ_COPY.change.packet.accepted.note_undated,
    HQ_COPY.change.packet.accepted.note_unattributed,
  ]) {
    assert.ok(!/version/i.test(String(sentence)), 'an accepted-list note says "version": ' + sentence)
  }
})

test('the two surfaces say the same sentence about what a run was checked on', () => {
  assert.equal(HQ_COPY.change.packet.mapping.applies, HQ_COPY.change.coverage.applies.inputs_applies)
  assert.equal(HQ_COPY.change.packet.mapping.other_version, HQ_COPY.change.coverage.states.other_version)
})

test('Verify, with no version of the work bound and a run that applies, uses the word once', () => {
  const model = buildChangeView({ change: aChange(), proposals: PROPOSALS, work: [] })
  model.coverage = buildCoverageView(coverageDoc({
    criteria: [{
      index: 1,
      text: 'A failed upload is tried again three times.',
      class: 'test',
      state: 'evidence',
      state_slug: 'coverage_evidence',
      absent_tool: null,
      removed: [],
      items: [{
        link_id: 'cce_1', kind: 'check_run', evidence_id: 'eb1', session_id: 'cli:check:2026-09-19T06:45:01Z',
        basis: 'named', basis_slug: 'basis_named', name: 'bash:cargo test', observed: 'proven',
        bears: true, note_slug: 'item_checked_nothing', applies: true, applies_slug: 'inputs_applies',
      }],
    }],
    execution_sessions: ['cli:check:2026-09-19T06:45:01Z'],
  }))
  const body = text(renderScreen({ model, stage: 'verify' }))
  /* The item's sentence is there, and it does not claim a version. */
  assert.ok(body.includes(HQ_COPY.change.coverage.applies.inputs_applies), body)
  /* Take out the one sentence that is about the version of the work, and no
     "version" is left anywhere on the screen. */
  let rest = withoutTheWorksVersion(body)
  assert.ok(
    !/version/i.test(rest),
    'a second meaning of "version" is on the Verify screen: ' + (rest.match(/.{0,70}version.{0,70}/i) || [''])[0],
  )
})

test('Define, in the same state, uses the word once too', () => {
  const model = buildChangeView({
    change: aChange({
      accepted_scope: {
        version: 1,
        criteria: ['A failed upload is tried again three times.'],
        exclusions: [], allowed_scope: ['src/upload/**'], constraints: [], check_profile: [],
        content_sha256: 'x',
        acceptance: { actor: 'sam', accepted_at: '2026-01-02T09:06:00Z', rationale: 'yes' },
        proposed_at: '2026-01-02T09:05:00Z',
      },
    }),
    proposals: PROPOSALS,
    work: [],
  })
  const body = text(renderScreen({ model, stage: 'define' }))
  let rest = withoutTheWorksVersion(body)
  assert.ok(
    !/version/i.test(rest),
    'a second meaning of "version" is on the Define screen: ' + (rest.match(/.{0,70}version.{0,70}/i) || [''])[0],
  )
})
