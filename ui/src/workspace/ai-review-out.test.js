/* AI-REVIEW-OUT-OF-LAUNCH (founder ruling R-2026-09-28-20) — "PR-Agent … In
 * the launch build it isn't offered at all: no entry point, and not a
 * disabled button either. 'Review my change' runs the deterministic review
 * only."
 *
 * The screen does not decide what the build offers; the server says so, in
 * `workflow.ai_review` on the tool-evidence reading and `ai_review` on the
 * Change's coverage reading, and anything short of an explicit `true` is the
 * launch configuration. This file mounts:
 *   - the tool-evidence screen (integrations.jsx, integration-workflow.jsx,
 *     integration-agent-repair.jsx, integration-review.jsx) with a reading
 *     that STILL lists a ready PR-Agent recipe and catalog source — as a
 *     server from before this card would — and asserts no PR-Agent control
 *     or sentence reaches the page, including once "Change review against
 *     registered base" (the "Review my change" control) is chosen;
 *   - the same screen with `ai_review: true`, which shows it, so the gate is
 *     the flag and not a deletion;
 *   - the Change screen's Review stage, which offers no "Ask for a review"
 *     and names no review verb unless the served flag says AI review exists.
 *
 * Every fixture is synthetic. */
import assert from 'node:assert/strict'
import test from 'node:test'
import { build as bundle } from 'esbuild'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { HQ_COPY } from './hq-copy.js'

const here = dirname(fileURLToPath(import.meta.url))
const AI = /pr-agent|pr_agent|11436/i

const L = await (async () => {
  const temp = await mkdtemp(join(tmpdir(), 'thruline-ai-review-out-'))
  const out = await bundle({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: {
      contents: `
        import React from 'react';
        import { createRoot } from 'react-dom/client';
        import { flushSync } from 'react-dom';
        import { renderToStaticMarkup } from 'react-dom/server';
        import { IntegrationWorkspace } from './integrations.jsx';
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
        export { React, createRoot, flushSync, IntegrationWorkspace };
      `,
      resolveDir: here,
    },
  })
  const file = join(temp, 'drive.mjs')
  await writeFile(file, out.outputFiles[0].text)
  const m = await import(pathToFileURL(file).href)
  await rm(temp, { recursive: true, force: true })
  return m
})()

const SESSION = 'review-t'
const RECIPE = (id) => ({ id, ready: true, accepted: false, requirement: `${id} requirement`, why: 'why', support: null, review_digest: 'd', next: 'next' })
const PACK = (id, feature) => ({ id, feature, feature_name: feature, contribution: `${id} contribution`, limits: '', candidate_check_ids: [], repository: 'x/y', commit: 'c', declared_main_license: 'MIT', submodules: 0 })
const READING = (aiReview) => ({
  schema: 'thruline.integration-workspace.v1',
  session: SESSION,
  root: '/synthetic/repo',
  workflow: {
    schema: 'thruline.integration-workflow.v1',
    ...(aiReview === undefined ? {} : { ai_review: aiReview }),
    accepted_revision: null,
    recipes: [RECIPE('ast-grep'), RECIPE('pr-agent')],
    trust: 'Local acceptance.',
  },
  project_profile: null,
  catalog: { projects: [PACK('ast-grep', 'structure'), PACK('pr-agent', 'local-repair-advisors')] },
  results: { contract: 'thruline.integration-observation.v1', runs: [] },
})
const PROJECTS = { projects: [{ session: SESSION, label: 'Synthetic', root: '/synthetic/repo', change_review_available: true, base_commit: '3a2cbfc1a51b09910dd3fb614191cb1d46236e13' }] }

async function mountTools(reading) {
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/' })
  const w = dom.window
  const names = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame']
  const held = names.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  for (const n of names) if (w[n] !== undefined) Object.defineProperty(globalThis, n, { value: n === 'window' ? w : w[n], configurable: true, writable: true })
  const realFetch = globalThis.fetch
  const asked = []
  globalThis.fetch = async (url, init = {}) => {
    asked.push([init.method || 'GET', String(url), init.body || null])
    const answer = (body) => ({ ok: true, status: 200, json: async () => body })
    if (String(url).startsWith('/api/integrations?')) return answer(reading)
    if (String(url) === '/api/review-projects') return answer(PROJECTS)
    if (String(url) === '/api/integrations/action') return answer({ reviews: [] })
    return { ok: false, status: 404, json: async () => ({ error: 'not in this fixture' }) }
  }
  const settle = () => new Promise((r) => setTimeout(r, 10))
  const root = L.createRoot(w.document.getElementById('root'))
  L.flushSync(() => root.render(L.React.createElement(L.IntegrationWorkspace, { session: { id: SESSION }, onCopy() {} })))
  for (let i = 0; i < 5; i++) await settle()
  return {
    d: w.document,
    w,
    asked,
    settle,
    close() {
      root.unmount()
      globalThis.fetch = realFetch
      for (const [n, dsc] of held) {
        if (dsc) Object.defineProperty(globalThis, n, dsc)
        else delete globalThis[n]
      }
      w.close()
    },
  }
}

async function chooseChangeReview(s) {
  const scope = s.d.querySelector('select[aria-label="Review scope"]')
  assert.ok(scope, 'the review scope control is on the screen')
  const option = [...scope.options].find((o) => o.value === 'change')
  assert.ok(option && !option.disabled, '"Change review against registered base" can be chosen')
  const setter = Object.getOwnPropertyDescriptor(s.w.HTMLSelectElement.prototype, 'value').set
  setter.call(scope, 'change')
  scope.dispatchEvent(new s.w.Event('change', { bubbles: true }))
  for (let i = 0; i < 3; i++) await s.settle()
}

test('the launch configuration: the tool-evidence screen offers no PR-Agent control, source or sentence', async () => {
  for (const flag of [false, undefined]) {
    const s = await mountTools(READING(flag))
    try {
      assert.ok(s.d.body.textContent.includes('ast-grep'), 'the deterministic recipe and source are still shown')
      assert.doesNotMatch(s.d.body.textContent, AI, `ai_review=${flag}: nothing on the screen names PR-Agent`)
      assert.equal(s.d.querySelector('[aria-label="PR-Agent proposal actions"]'), null, 'no PR-Agent proposal panel')
      await chooseChangeReview(s)
      assert.doesNotMatch(s.d.body.textContent, AI, 'choosing "Change review against registered base" names no AI reviewer')
      assert.ok(s.d.body.textContent.includes('Registered base: 3a2cbfc1a51b09910dd3fb614191cb1d46236e13'), 'the change review still names its base')
      for (const [, , body] of s.asked) assert.doesNotMatch(String(body || ''), AI, 'no request names PR-Agent')
    } finally { s.close() }
  }
})

test('with ai_review: true (the ai-review build), the same reading shows PR-Agent — the flag is the gate, not a deletion', async () => {
  const s = await mountTools(READING(true))
  try {
    assert.match(s.d.body.textContent, /PR-Agent/)
  } finally { s.close() }
})

/* ── the Change screen's Review stage ─────────────────────────────────── */

const plain = (html) => html.replace(/<[^>]+>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ')
const CHANGE = Object.freeze({
  id: 'chg-u', project_id: 'p', opened_at: '2026-09-23T15:54:15Z',
  request: { title: 'Fix addition', purpose: 'add() should add', requester: 'sam', source: 'person' },
  accepted_scope: null, scope_history: [], implementation: { version: 0, bound_at: '2026-09-23T15:54:30Z', repository_identity: 'wsid1:x', base_revision: '3a2cbfc1a51b09910dd3fb614191cb1d46236e13', snapshot_id: '01M37FKDBV5DSH8YMNR3NJMJCG', workspace_identity: null },
  implementation_history: [], associations: [], examination: 'none_yet', decision: 'none_yet',
  decisions: { standing: null, history: [], target_revision: '01M37FKDBV5DSH8YMNR3NJMJCG', scope_version: null },
  reviews: [], masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'x' },
})
const COVERAGE = (extra) => ({
  scope_version: 1, criteria: [], candidates: [], executions: [],
  readiness: { change_id: 'chg-u', target_version: '01M37FKDBV5DSH8YMNR3NJMJCG', ready: false, headline_slug: 'readiness_needs_evidence', obligations: [], rests_on: [], waived: [], with_exceptions: false },
  review_findings: null,
  ...extra,
})

test('the launch configuration: the Review stage offers no "Ask for a review", enabled or not, and names no review verb', () => {
  const ask = HQ_COPY.change.actions.ask_for_review
  for (const coverage of [COVERAGE({}), COVERAGE({ ai_review: false }), null]) {
    const text = plain(L.renderScreen({ model: L.modelOf(CHANGE, coverage), stage: 'review' }))
    assert.ok(text.includes(HQ_COPY.change.review.heading), 'the Review stage rendered')
    assert.ok(!text.includes(ask), `no "${ask}" control: ${JSON.stringify(coverage && coverage.ai_review)}`)
    assert.ok(!text.includes('thruline integration review'), 'no review verb is suggested')
    assert.doesNotMatch(text, AI)
  }
  const dev = plain(L.renderScreen({ model: L.modelOf(CHANGE, COVERAGE({ ai_review: true })), stage: 'review' }))
  assert.ok(dev.includes(ask), 'the ai-review build keeps its control')
})
