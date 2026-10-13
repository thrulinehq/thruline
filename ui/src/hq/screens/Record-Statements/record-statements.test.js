/* The Record KPI destination lists the same served claims as the card, across Changes. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { parse } from '../../routes.js'
import { readStatementDocuments, statementList } from './model.js'

const here = dirname(fileURLToPath(import.meta.url))

const NOW = new Date(2026, 9, 9, 12).getTime()
const day = (ago) => new Date(2026, 9, 9 - ago, 10).toISOString()
const projects = [{ id: 'p1', name: 'alpha' }, { id: 'p2', name: 'beta' }]
const claim = (id, age, agent, verdict, moved, thread_id = 't-' + id) => ({
  id, thread_id, ts: day(age), agent, verdict, verdict_basis: verdict === 'not_proven' ? 'edited_since_run' : 'run_holds',
  proof_moved: moved, proven_at: verdict === 'proven' ? day(age) : null, run_before: false, has_verifier: true,
})
const docs = {
  p1: { claims: [
    claim('a1', 1, 'claude_code', 'proven', false),
    claim('a2', 2, 'codex', 'not_proven', true),
    claim('a3', 3, 'claude_code', 'proven', true),
    claim('a4', 40, 'claude_code', 'recorded', false),
  ], sessions: [] },
  p2: { claims: [
    claim('b1', 4, 'codex', 'proven', true),
    claim('b2', 8, 'claude_code', 'proven', false),
    claim('b3', 1, 'claude_code', 'recorded', false, null),
  ], sessions: [] },
}
const at = (values = {}) => ({ range: '30', agent: 'all', scope: 'all', subset: 'all', ...values })

test('reads only existing per-project statements GET addresses, once each', async () => {
  const seen = []
  const result = await readStatementDocuments(projects, async (url) => {
    seen.push(url)
    return docs[url.endsWith('/p1/statements') ? 'p1' : 'p2']
  })
  assert.deepEqual(seen, ['/api/hq/projects/p1/statements', '/api/hq/projects/p2/statements'])
  assert.deepEqual(result, docs)
})

test('KPI 1, 2, and 3 lists preserve the clicked range, agent and project slice', () => {
  const all = statementList({ projects, documents: docs, at: at(), now: NOW })
  assert.equal(all.count, 6)
  assert.deepEqual(all.rows.map((r) => r.id), ['a1', 'b3', 'a2', 'a3', 'b1', 'b2'])
  assert.equal(statementList({ projects, documents: docs, at: at({ subset: 'proven', scope: 'p1' }), now: NOW }).count, 2)
  assert.deepEqual(statementList({ projects, documents: docs, at: at({ subset: 'moved', range: '7', agent: 'cx' }), now: NOW }).rows.map((r) => r.id), ['a2', 'b1'])
  assert.deepEqual(statementList({ projects, documents: docs, at: at({ subset: 'open', range: '7' }), now: NOW }).rows.map((r) => r.id), ['a2'])
  assert.deepEqual(statementList({ projects, documents: docs, at: at({ subset: 'recorded', range: '7' }), now: NOW }).rows.map((r) => r.id), ['b3'])
})

test('a missing proof-moved field or project document never reads as zero', () => {
  const absent = structuredClone(docs)
  delete absent.p1.claims[0].proof_moved
  const moved = statementList({ projects, documents: absent, at: at({ subset: 'moved' }), now: NOW })
  assert.equal(moved.served, false)
  assert.equal(moved.count, null)
  assert.deepEqual(moved.rows, [])
  assert.equal(statementList({ projects, documents: { p1: docs.p1 }, at: at(), now: NOW }).served, false)
  assert.equal(statementList({ projects, documents: docs, at: at({ scope: 'unknown' }), now: NOW }).notFound, true)
  for (const bad of [null, {}, { claims: null }, { claims: {} }]) {
    const malformed = { ...docs, p1: bad }
    const view = statementList({ projects, documents: malformed, at: at(), now: NOW })
    assert.equal(view.served, false)
    assert.equal(view.count, null)
    assert.deepEqual(view.rows, [])
  }
})

test('each row names its receipt and links only to its actual Change', () => {
  const view = statementList({ projects, documents: docs, at: at({ project: 'p1' }), now: NOW })
  const byId = Object.fromEntries(view.rows.map((r) => [r.id, r]))
  assert.equal(byId.a2.href, '#/changes/t-a2?project=p1&statement=a2')
  assert.equal(byId.b1.href, '#/changes/t-b1?project=p2&statement=b1')
  assert.deepEqual(parse(byId.a2.href), { screen: 'brief', change: 't-a2', project: 'p1', statement: 'a2' })
  assert.equal(byId.b3.href, undefined)
  assert.equal(byId.b3.reason, 'No Change was recorded for this statement.')
})

test('the destination draws the selected count, receipt rows and true Change links', async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic', logLevel: 'silent',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' },
    stdin: {
      contents: `import React from 'react'; import { renderToStaticMarkup } from 'react-dom/server'; import { Page } from './index.jsx'; import { FrameContext } from '../../frame-state.jsx'; export const render = (view) => renderToStaticMarkup(React.createElement(FrameContext.Provider, { value: { at: { screen: 'recordStatements', project: 'p1' }, openCommand: null } }, React.createElement(Page, { view, at: { project: 'p1' }, shown: 100 })));`,
      resolveDir: here, loader: 'jsx',
    },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'record-statements-')), 'page.mjs')
  await writeFile(file, result.outputFiles[0].text)
  const { render } = await import(pathToFileURL(file).href)
  const html = render(statementList({ projects, documents: docs, at: at(), now: NOW }))
  assert.match(html, /<main aria-label="Statements across Changes"/)
  assert.match(html, /<a href="#\/record\?project=p1" aria-current="page"/, 'Record over time stays the section in view')
  assert.doesNotMatch(html, /<main[^>]*><main|<footer[^>]*>[^]*<footer/, 'one frame, not a page of its own inside another')
  assert.match(html, /6 statements/)
  assert.match(html, /Last 30 days · Both agents · All projects/)
  assert.ok(html.includes('href="#/changes/t-a2?project=p1&amp;statement=a2"'), 'the row opens its exact statement')
  assert.ok(html.includes('Open this statement in its Change'), 'the link names its destination')
  assert.match(html, /No Change was recorded for this statement/)
})

test('Record KPI markup accepts three distinct list destinations without changing the gallery links', async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic', logLevel: 'silent',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' },
    stdin: {
      contents: `import React from 'react'; import { renderToStaticMarkup } from 'react-dom/server'; import Template from '../Record-Over-Time/template.jsx'; import Logic from '../Record-Over-Time/logic.js'; export function render() { const inst = new Logic({}); inst.props = {}; const v = inst.renderVals(); v.k = { ...v.k, nHref: '#/record/statements?subset=all', pHref: '#/record/statements?subset=proven', reHref: '#/record/statements?subset=moved' }; return renderToStaticMarkup(React.createElement(Template, { v, link: () => undefined })); }`,
      resolveDir: here, loader: 'jsx',
    },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'record-kpi-links-')), 'template.mjs')
  await writeFile(file, result.outputFiles[0].text)
  const { render } = await import(pathToFileURL(file).href)
  const html = render()
  assert.match(html, /href="#\/record\/statements\?subset=all"[^>]*aria-label="Statements the agents made/)
  assert.match(html, /href="#\/record\/statements\?subset=proven"[^>]*aria-label="Proven on the version/)
  assert.match(html, /href="#\/record\/statements\?subset=moved"[^>]*aria-label="Needed a re-check/)
})

test('the live Record’s three counts open the list with the range, agent and project slice they count, and the list holds the same number', async () => {
  const { recordFromStatements } = await import('../Record-Over-Time/from-projects.js')
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic', logLevel: 'silent',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' },
    stdin: {
      contents: `import React from 'react'; import { renderToStaticMarkup } from 'react-dom/server'; import Template from '../Record-Over-Time/template.jsx'; import Logic from '../Record-Over-Time/logic.js'; export function render(props, state) { const inst = new Logic(props); inst.props = props; inst.state = { ...inst.state, ...state }; const v = inst.renderVals(); return { k: v.k, html: renderToStaticMarkup(React.createElement(Template, { v, link: () => undefined })) }; }`,
      resolveDir: here, loader: 'jsx',
    },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'record-live-kpi-')), 'live.mjs')
  await writeFile(file, result.outputFiles[0].text)
  const { render } = await import(pathToFileURL(file).href)
  const data = recordFromStatements({ projects, statements: docs, storage: null, now: NOW })
  const cases = [
    [{ range: '30', agent: 'all', project: 'all' }, { project: 'p1' }],
    [{ range: '7', agent: 'cx', project: 'p1' }, { project: 'p1' }],
    [{ range: '90', agent: 'cc', project: 'p2' }, undefined],
  ]
  for (const [state, where] of cases) {
    const { k, html } = render({ data, at: where }, state)
    const opens = (aria) => {
      const m = new RegExp(`href="([^"]+)"[^>]*aria-label="${aria}`).exec(html)
      assert.ok(m, `a link opens "${aria}"`)
      return parse(m[1].replaceAll('&amp;', '&'))
    }
    const slice = { screen: 'recordStatements', project: where?.project, range: state.range, agent: state.agent, scope: state.project }
    const nOpens = opens('Statements')
    const pOpens = opens('Proven on the version')
    const reOpens = opens('Needed a re-check')
    assert.deepEqual(nOpens, { ...slice, subset: 'all' })
    assert.deepEqual(pOpens, { ...slice, subset: 'proven' })
    assert.deepEqual(reOpens, { ...slice, subset: 'moved' })
    for (const [subset, shown] of [['all', k.n], ['proven', k.p], ['moved', k.re]]) {
      const list = statementList({ projects, documents: docs, at: { ...slice, subset }, now: NOW })
      assert.equal(String(list.count), shown, `the ${subset} list holds the ${shown} the card counts (${JSON.stringify(state)})`)
    }
  }
  // a record that does not say whether a later edit moved the proof counts "need a re-check now", and opens that list
  const unsaid = structuredClone(docs)
  for (const p of Object.values(unsaid)) for (const c of p.claims) delete c.proof_moved
  const { k, html } = render({ data: recordFromStatements({ projects, statements: unsaid, storage: null, now: NOW }), at: undefined }, { range: '30', agent: 'all', project: 'all' })
  assert.equal(parse(/href="([^"]+)"[^>]*aria-label="Need a re-check now/.exec(html)[1].replaceAll('&amp;', '&')).subset, 'open')
  assert.equal(String(statementList({ projects, documents: unsaid, at: at({ subset: 'open' }), now: NOW }).count), k.re)
})
