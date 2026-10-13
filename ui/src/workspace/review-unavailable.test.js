/* T-4-1a-FIX (founder ruling R-2026-09-28-5 (a)) — "when no review model is
 * reachable, the review path says plainly that review findings aren't
 * available, and why. Never an empty "no findings", never an invented
 * finding."
 *
 * The server says whether review findings can be had on this machine
 * (`review_findings` on the coverage and readiness documents, only where the
 * Change's accepted policy expects a review). This file holds:
 *   - the two sentences, word for word, here AND in the Rust doors, so the
 *     terminal and the screen cannot say two different things;
 *   - the readiness answer naming the review obligation says the sentence;
 *   - the Change screen says it where the review would show;
 *   - nothing on either says "no findings", "0 findings" or "Complete".
 *
 * Every fixture is synthetic. */
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { build as bundle } from 'esbuild'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { HQ_COPY } from './hq-copy.js'
import { readinessOf } from './change-model.js'

const TOOL = "Review findings aren't available: the review tool isn't set up on this machine."
const MODEL = "Review findings aren't available: the review model at 127.0.0.1:11436 isn't answering."
const NEVER = /no findings|0 findings|found nothing|raised nothing|\bComplete\b/

const U = HQ_COPY.change.review.unavailable_now

test('the two sentences are the copy file\'s, word for word', () => {
  assert.equal(U.tool_not_set_up, TOOL)
  assert.equal(U.model_not_answering, MODEL)
  for (const s of Object.values(U)) assert.doesNotMatch(s, NEVER)
})

test('the terminal doors say the same two sentences as the screen', () => {
  const src = readFileSync(new URL('../../../crates/cli/src/integration_cmd/review.rs', import.meta.url), 'utf8')
  const constant = (name) => {
    const m = src.match(new RegExp(`const ${name}: &str =\\s*("[^"]*");`))
    return m ? JSON.parse(m[1]) : null
  }
  assert.equal(constant('FINDINGS_UNAVAILABLE_TOOL'), TOOL, 'review.rs carries the tool sentence verbatim')
  assert.equal(constant('FINDINGS_UNAVAILABLE_MODEL'), MODEL, 'review.rs carries the model sentence verbatim')
})

const REVIEW_OBLIGATION = Object.freeze({
  id: 'review', kind: 'review', criterion_index: null, text: '',
  outcome: 'needs_evidence', outcome_slug: 'needs_evidence',
  unsatisfied: 'review_missing', unsatisfied_slug: 'readiness_review_missing',
  evidence_name: null, waiver: null, waivable: true,
})
const READINESS = (obligations) => ({
  change_id: 'chg-u', target_version: '01M37FKDBV5DSH8YMNR3NJMJCG', ready: false,
  headline_slug: 'readiness_needs_evidence', obligations, rests_on: [], waived: [], with_exceptions: false,
})
const COVERAGE = (obligations, said) => ({
  scope_version: 1, criteria: [], candidates: [], executions: [],
  readiness: READINESS(obligations),
  review_findings: said,
})
const SAID = (why, technical = 'Tools are not provisioned.') => ({ available: false, why, sentence: 'server words are not shown', technical })

test('where the policy expects a review, the readiness answer says why there are no review findings', () => {
  for (const [why, sentence] of [['tool_not_set_up', TOOL], ['model_not_answering', MODEL]]) {
    const r = readinessOf(COVERAGE([REVIEW_OBLIGATION], SAID(why)), '01M37FK')
    assert.deepEqual(r.reviewUnavailable, { sentence, technical: 'Tools are not provisioned.' })
    const line = r.missing.find((m) => m.id === 'review')
    assert.ok(line, 'the review obligation is listed')
    assert.ok(line.reason.startsWith(HQ_COPY.change.model.readiness.reasons.readiness_review_missing), 'its own reason stays')
    assert.ok(line.reason.endsWith(sentence), `and says why: ${line.reason}`)
    assert.doesNotMatch(line.reason, NEVER)
  }
})

test('an unrecognised reason is still said, never left silent', () => {
  const r = readinessOf(COVERAGE([REVIEW_OBLIGATION], SAID('something_new')), '01M37FK')
  assert.equal(r.reviewUnavailable.sentence, U.unknown)
  assert.match(U.unknown, /^Review findings aren't available/)
})

test('no review obligation, or a reachable review, adds nothing', () => {
  assert.equal(readinessOf(COVERAGE([], SAID('tool_not_set_up')), '01M37FK').reviewUnavailable, null)
  assert.equal(readinessOf(COVERAGE([REVIEW_OBLIGATION], null), '01M37FK').reviewUnavailable, null)
  assert.equal(readinessOf(null, '01M37FK').reviewUnavailable, null)
  const set = readinessOf(COVERAGE([{ ...REVIEW_OBLIGATION, outcome: 'waived', unsatisfied_slug: null }], SAID('tool_not_set_up')), '01M37FK')
  assert.ok(set.reviewUnavailable, 'even set aside, the Review stage still says why there are none')
})

/* ── rendered: the Change screen ─────────────────────────────────────── */

const here = dirname(fileURLToPath(import.meta.url))
const temp = await mkdtemp(join(tmpdir(), 'thruline-review-unavailable-'))
const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeScreen } from './change-workspace.jsx';
  import { buildChangeView } from './change-view.js';
  import { deriveChangeModel } from './change-model.js';
  import { buildChangeReviews } from './review-model.js';
  export function modelOf(change, coverage) {
    const model = buildChangeView({ change, proposals: null, work: [], evidence: [] });
    model.hq = deriveChangeModel(change, coverage, null, null, []);
    model.reviews = buildChangeReviews(change);
    model.standings = null;
    return model;
  }
  export const renderScreen = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
`
const R = await (async () => {
  const out = await bundle({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temp, 'render.mjs')
  await writeFile(file, out.outputFiles[0].text)
  const m = await import(pathToFileURL(file).href)
  await rm(temp, { recursive: true, force: true })
  return m
})()

const plain = (html) => html.replace(/<[^>]+>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ')
const CHANGE = Object.freeze({
  id: 'chg-u', project_id: 'p', opened_at: '2026-09-23T15:54:15Z',
  request: { title: 'Fix addition', purpose: 'add() should add', requester: 'sam', source: 'person' },
  accepted_scope: null, scope_history: [], implementation: { version: 0, bound_at: '2026-09-23T15:54:30Z', repository_identity: 'wsid1:x', base_revision: '3a2cbfc1a51b09910dd3fb614191cb1d46236e13', snapshot_id: '01M37FKDBV5DSH8YMNR3NJMJCG', workspace_identity: null },
  implementation_history: [], associations: [], examination: 'none_yet', decision: 'none_yet',
  decisions: { standing: null, history: [], target_revision: '01M37FKDBV5DSH8YMNR3NJMJCG', scope_version: null },
  reviews: [], masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'x' },
})

test('the Review stage says the sentence where the review would show, with the detail one level down', () => {
  for (const [why, sentence] of [['tool_not_set_up', TOOL], ['model_not_answering', MODEL]]) {
    const html = R.renderScreen({ model: R.modelOf(CHANGE, COVERAGE([REVIEW_OBLIGATION], SAID(why, 'the engine said this'))), stage: 'review' })
    const text = plain(html)
    const at = text.indexOf(sentence)
    assert.ok(at >= 0, `the Review stage says: ${sentence}`)
    const review = text.slice(text.indexOf(HQ_COPY.change.review.heading))
    assert.ok(review.includes(sentence), 'under the review heading, where the review would show')
    assert.match(html, /<details[^>]*>(?:(?!<\/details>).)*the engine said this/s, 'the technical detail is inside a details element')
    assert.doesNotMatch(text, NEVER)
  }
})

test('with no review expected, the Review stage does not mention one being unavailable', () => {
  const html = R.renderScreen({ model: R.modelOf(CHANGE, COVERAGE([], SAID('tool_not_set_up'))), stage: 'review' })
  assert.ok(!plain(html).includes("Review findings aren't available"))
})
