import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* T-3-1 — the criterion list, rendered, and the one write on the Change screen.
 *
 * The same harness the Change screen uses: the surface is bundled and rendered
 * to static markup, and every assertion is about THE STRINGS AND CONTROLS A
 * PERSON MEETS. A probe bundle — the real copy file with two strings replaced —
 * proves the words come from the copy file rather than from a literal that
 * happens to match it.
 *
 * Every id is synthetic and in the `cccccccc-` family.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-links-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { CriterionCoverage, CriterionRow, CHANGE_LINKS } from './criterion-links.jsx';
  import { buildCoverageView } from './coverage-view.js';
  import { HQ_COPY } from './hq-copy.js';
  export { buildCoverageView, HQ_COPY, CHANGE_LINKS };
  export const renderSection = props => renderToStaticMarkup(React.createElement(CriterionCoverage, props));
  export const renderRow = props => renderToStaticMarkup(React.createElement('ol', null, React.createElement(CriterionRow, props)));
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

const M = await load('links.mjs')

const COPY_FILE = join(here, 'hq-copy.js')
const PROBED = await load('links-probe.mjs', [{
  name: 'probe-copy',
  setup(b) {
    b.onLoad({ filter: /hq-copy\.js$/ }, () => ({
      contents: readFileSync(COPY_FILE, 'utf8')
        .replace("none: 'No test ran for this.',", "none: 'probe-nothing-ran',")
        .replace("declared: 'Looked at because the checks you accepted cover this kind of thing.'", "declared: 'probe-basis-declared'"),
      loader: 'js',
    }))
  },
}])

const { buildCoverageView, HQ_COPY, renderSection, renderRow, CHANGE_LINKS } = M
const C = HQ_COPY.change.coverage
const CHANGE = 'cccccccc-0000-4000-8000-00000000c001'
const SESSION = 'cccccccc-0000-4000-8000-00000000s001'
const RUN = 'cccccccc-0000-4000-8000-00000000b001'

const text = (html) => html.replace(/<[^>]*>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ').trim()

const DOC = {
  scope_version: 1,
  criteria: [
    {
      index: 1,
      text: 'The build stays clean under clippy after the change.',
      class: 'build',
      state: 'evidence',
      state_slug: 'coverage_evidence',
      absent_tool: null,
      items: [{
        link_id: 'cce_0000', kind: 'check_run', evidence_id: RUN, session_id: SESSION,
        basis: 'declared', basis_slug: 'basis_declared', detail: 'clippy', actor: null,
        asserted_at: '2026-02-01T12:00:00Z', reason: null,
        name: 'bash:/tmp/synthetic:cargo clippy', observed: 'proven', bears: true, note_slug: null,
      }],
      removed: [],
    },
    {
      index: 2,
      text: 'HQ reopens where you left it after a restart.',
      class: 'unclassified',
      state: 'none',
      state_slug: 'coverage_no_test_ran',
      absent_tool: null,
      items: [],
      removed: [],
    },
  ],
  candidates: [{ kind: 'check_run', evidence_id: RUN, session_id: SESSION, name: 'bash:/tmp/synthetic:cargo clippy', observed: 'proven', bears: true }],
  link_rows: 1,
}

const view = buildCoverageView(DOC)

/* ── what a person reads ────────────────────────────────────────────────── */

test('every accepted line is on the screen, with its own sentence under it', () => {
  const body = text(renderSection({ changeId: CHANGE, view }))
  assert.ok(body.includes('The build stays clean under clippy after the change.'))
  assert.ok(body.includes('HQ reopens where you left it after a restart.'))
  assert.ok(body.includes('No test ran for this.'))
})

test('the sentences come from the copy file, not from a literal in the view', async () => {
  const probeView = PROBED.buildCoverageView(DOC)
  const body = text(PROBED.renderSection({ changeId: CHANGE, view: probeView }))
  assert.ok(body.includes('probe-nothing-ran'), 'the empty-line sentence is a literal in the view')
  assert.ok(body.includes('probe-basis-declared'), 'the reason a link exists is a literal in the view')
})

test('a line with nothing behind it is not styled as a failure and says nothing green', () => {
  const html = renderSection({ changeId: CHANGE, view })
  const body = text(html)
  for (const forbidden of ['Failed', 'FAIL', '✓', '✗', 'all clear', 'Passed', 'Ready for approval']) {
    assert.ok(!body.includes(forbidden), `a verdict word reached the line: ${forbidden}`)
  }
  assert.ok(!/class="[^"]*\b(green|success|ok)\b/.test(html), 'a colour is carrying the meaning')
})

test('the five state words appear on the item and never as the answer for the line', () => {
  const body = text(renderSection({ changeId: CHANGE, view }))
  /* Proven is on the run. The line above it says a sentence, not a word. */
  assert.ok(body.includes('Proven — The record backs it'))
  assert.ok(body.includes(C.states.evidence))
})

/* ── every disabled control says why and what would turn it on ──────────── */

test('with no name given, both acts are off and each says why and what enables it', () => {
  const html = renderSection({ changeId: CHANGE, view })
  const offs = [...html.matchAll(/<button[^>]*disabled[^>]*aria-describedby="([^"]+)"/g)]
  assert.ok(offs.length >= 2, 'the acts are on without a name')
  for (const m of offs) {
    const why = new RegExp(`id="${m[1]}"[^>]*>([^<]+)<`).exec(html)
    assert.ok(why, 'a disabled control points at a reason that is not on the page')
    assert.ok(why[1].trim().length > 40, `the reason says nothing: ${why && why[1]}`)
  }
  const body = text(html)
  assert.ok(body.includes(C.disabled.needs_name.reason))
  assert.ok(body.includes(C.disabled.needs_name.enabler))
})

test('with a name and nothing to link, the control is off for the OTHER reason', () => {
  const nothing = buildCoverageView({ ...DOC, candidates: [] })
  const body = text(renderRow({ row: nothing.rows[1], changeId: CHANGE, actor: 'sam', candidates: [], onAct: () => {}, busy: false, note: null }))
  assert.ok(body.includes(C.disabled.nothing_to_link.reason))
  assert.ok(body.includes(C.disabled.nothing_to_link.enabler))
  assert.ok(!body.includes(C.disabled.needs_name.reason), 'the wrong reason is shown')
})

test('with a name and something to link, the act is on', () => {
  const html = renderRow({ row: view.rows[1], changeId: CHANGE, actor: 'sam', candidates: view.candidates, onAct: () => {}, busy: false, note: null })
  assert.ok(/<button[^>]*>This bears on it<\/button>/.test(html), html)
  assert.ok(!/disabled/.test(html.split('This bears on it')[0].slice(-120)), 'the act is still off')
})

/* ── removing a link never removes it ───────────────────────────────────── */

test('a link a person took off the line is still on the screen, with who took it off', () => {
  const withRemoved = buildCoverageView({
    ...DOC,
    criteria: [{ ...DOC.criteria[1], removed: [{
      link_id: 'cce_0002', kind: 'check_run', evidence_id: RUN, session_id: SESSION,
      basis: 'not-bearing', basis_slug: 'basis_not_bearing', detail: null, actor: 'sam',
      asserted_at: '2026-02-02T12:00:00Z', reason: 'wrong line',
      name: 'bash:/tmp/synthetic:cargo clippy', observed: 'proven', bears: true, note_slug: null,
    }] }],
  })
  const body = text(renderSection({ changeId: CHANGE, view: withRemoved }))
  assert.ok(body.includes(C.removed_heading))
  assert.ok(body.includes(C.basis.not_bearing))
  assert.ok(body.includes('Taken off by sam'), 'the person who decided is not named as what they did')
  assert.ok(!body.includes('Asked by sam'), 'a person who took a link off is rendered as the person who asked for the work')
  assert.ok(body.includes('wrong line'), 'the words they gave are not shown')
})

test('the person who linked something is named as having linked it', () => {
  const byHand = buildCoverageView({
    ...DOC,
    criteria: [{ ...DOC.criteria[1], state: 'evidence', items: [{
      link_id: 'cce_0003', kind: 'check_run', evidence_id: RUN, session_id: SESSION,
      basis: 'manual', basis_slug: 'basis_manual', detail: null, actor: 'sam',
      asserted_at: '2026-02-02T12:00:00Z', reason: 'I checked by hand',
      name: 'bash:/tmp/synthetic:cargo clippy', observed: 'proven', bears: true, note_slug: null,
    }], removed: [] }],
  })
  const body = text(renderSection({ changeId: CHANGE, view: byHand }))
  assert.ok(body.includes('Linked by sam'))
  assert.ok(body.includes(C.basis.manual))
})

/* ── the one write, and its shape ───────────────────────────────────────── */

test('this file posts to exactly one address, and the Change screen posts to none', () => {
  const source = readFileSync(join(here, 'criterion-links.jsx'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
  const addresses = [...source.matchAll(/'\/api\/[^']*'/g)].map((m) => m[0])
  assert.deepEqual(addresses, ["'/api/changes/'"], `more than one address: ${addresses}`)
  assert.ok(source.includes("+ '/links'"), 'the address is not the links route')
  assert.equal(CHANGE_LINKS('a b'), '/api/changes/a%20b/links')

  const screen = readFileSync(join(here, 'change-workspace.jsx'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
  for (const forbidden of ["method: 'POST'", 'method: "POST"', '/associations', '/scope/accept', '/links']) {
    assert.ok(!screen.includes(forbidden), `the Change screen can write: ${forbidden}`)
  }
})

test('neither act can be sent without a name, and the request names which act it is', () => {
  const source = readFileSync(join(here, 'criterion-links.jsx'), 'utf8')
  /* The two acts, and nothing else, reach the body. */
  assert.ok(/act: 'link'/.test(source) && /act: 'unlink'/.test(source))
  assert.ok(!/act: 'delete'|DELETE/.test(source), 'a delete reached the surface')
  /* The control that would send it is off until a name is typed: proven by the
   * rendered screen above, and by the model's own blocker here. */
  const html = renderSection({ changeId: CHANGE, view })
  assert.ok(!/<button(?![^>]*disabled)[^>]*>This bears on it</.test(html), 'the act is live with no name')
})

/* ── the empty document ─────────────────────────────────────────────────── */

test('a Change with nothing accepted renders no list at all, rather than an empty one', () => {
  const empty = buildCoverageView({ criteria: [], candidates: [], link_rows: 0, scope_version: null })
  assert.equal(renderSection({ changeId: CHANGE, view: empty }), '')
})

/* ── T-3-6-F1 / R-2026-09-23: "no test ran" is reserved ─────────────────── */

/** The same document, plus one finished, counted run linked to nothing. */
const WITH_UNLINKED = {
  ...DOC,
  candidates: [
    ...DOC.candidates,
    {
      kind: 'check_run',
      evidence_id: 'cccccccc-0000-4000-8000-00000000b002',
      session_id: SESSION,
      name: 'bash:/tmp/synthetic:cargo test --test=restart',
      observed: 'proven',
      bears: true,
      execution: 'completed',
      execution_slug: 'execution_completed',
    },
  ],
}

test('a line with nothing linked, on a Change holding a finished run linked to nothing, never says no test ran', () => {
  const body = text(renderSection({ changeId: CHANGE, view: buildCoverageView(WITH_UNLINKED) }))
  assert.ok(!body.includes('No test ran for this.'), 'a run is on record; saying none ran is false')
  assert.ok(body.includes("A test ran for this Change but isn't linked to this line — link it."))
  /* The act it names is reachable from the same line: the manual link control
   * T-3-1 already has, at the one address it already posts to. */
  assert.ok(body.includes(C.link) || body.includes('Link'), 'the act the sentence names is not on the line')
})

test('with no such run on record, the line still says no test ran for this', () => {
  const body = text(renderSection({ changeId: CHANGE, view }))
  assert.ok(body.includes('No test ran for this.'))
  assert.ok(!body.includes("isn't linked to this line"))
})
