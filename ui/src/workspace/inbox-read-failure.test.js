import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* INBOX-READ-FAILURE-NOT-SETTLED. Synthetic GET responses drive the same
 * readInbox -> readChangeDocuments -> assembleInbox path the landing uses.
 * The generated Change is kept local to this test; no live record is read. */
const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-inbox-read-'))
after(() => rm(temporary, { recursive: true, force: true }))

const result = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  stdin: {
    contents: `
      import React from 'react';
      import { renderToStaticMarkup } from 'react-dom/server';
      import { readInbox, assembleInbox, InboxScreen, READ_LIMIT } from './inbox.jsx';
      import { readChangeDocuments } from './change-workspace.jsx';
      import { API, CHANGE, CHANGE_PROPOSALS, CHANGE_COVERAGE, CHANGE_INPUTS, CHANGE_APPLICABILITY, CHANGE_FINDINGS } from './data.js';
      import { aStore } from './attention-count.fixture.js';
      import { HQ_COPY } from './hq-copy.js';
      export { readInbox, assembleInbox, readChangeDocuments, READ_LIMIT, API, CHANGE, CHANGE_PROPOSALS, CHANGE_COVERAGE, CHANGE_INPUTS, CHANGE_APPLICABILITY, CHANGE_FINDINGS, aStore, HQ_COPY };
      export const renderInbox = state => renderToStaticMarkup(React.createElement(InboxScreen, { state }));
    `,
    resolveDir: here,
  },
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
})
const bundled = join(temporary, 'inbox-read.mjs')
await writeFile(bundled, result.outputFiles[0].text)
const M = await import(pathToFileURL(bundled).href)

const plain = html => html.replace(/<[^>]*>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ').trim()
const clone = value => JSON.parse(JSON.stringify(value))

function store(count = 1, { ready = false, linked = false } = {}) {
  const seed = M.aStore(6100, { count: 1, force: { onlyInfo: true, criteria: 1 } })
  const sourceRow = seed.all[0]
  const sourceDocs = seed.docsById.get(sourceRow.id)
  const all = []
  const docsById = new Map()
  for (let i = 0; i < count; i += 1) {
    const id = `cccccccc-0000-4000-8000-${String(i).padStart(12, '0')}`
    const row = clone(sourceRow)
    row.id = id
    row.opened_at = `2026-01-${String(i + 1).padStart(2, '0')}T09:00:00Z`
    row.request.title = `Synthetic Change ${i}`
    const docs = clone(sourceDocs)
    docs.change.id = id
    docs.change.opened_at = row.opened_at
    docs.change.request.title = row.request.title
    if (!linked) docs.change.associations = []
    if (ready) docs.coverage.readiness = {
      ready: true, target_version: docs.change.implementation.snapshot_id,
      headline_slug: 'readiness_ready', obligations: [], rests_on: [], with_exceptions: false,
    }
    all.push(row)
    docsById.set(id, docs)
  }
  return { all, docsById }
}

async function withResponses(data, { refused = new Set(), missing = new Set(), badExplain = false } = {}, run = () => M.readInbox()) {
  const routes = new Map([[M.API.changes, { changes: data.all }], [M.API.storage, { bytes: 1000, path: '/tmp/synthetic-store' }]])
  for (const row of data.all) {
    const docs = data.docsById.get(row.id)
    routes.set(M.CHANGE(row.id), { change: docs.change })
    routes.set(M.CHANGE_PROPOSALS(row.id), { proposals: docs.proposals })
    routes.set(M.CHANGE_COVERAGE(row.id), { coverage: docs.coverage, readiness: docs.coverage.readiness })
    routes.set(M.CHANGE_INPUTS(row.id), docs.inputs || null)
    routes.set(M.CHANGE_APPLICABILITY(row.id), docs.applicability || null)
    routes.set(M.CHANGE_FINDINGS(row.id), docs.findings || null)
    for (const association of docs.change.associations) {
      const suffix = '?session=' + encodeURIComponent(association.session_id)
      routes.set(M.API.explain + suffix, badExplain ? { shape: 'unreadable.explain', schema_version: 1 } : { shape: 'thruline.explain', schema_version: 1 })
      routes.set(M.API.claims + suffix, { shape: 'thruline.claims', schema_version: 1 })
    }
  }
  const original = globalThis.fetch
  const requests = []
  const unexpected = []
  globalThis.fetch = async (url, init) => {
    requests.push({ url, method: init.method })
    if (refused.has(url)) return new Response(JSON.stringify({ error: 'projection_failed', command: url, exit_code: 1 }), { status: 502 })
    if (missing.has(url)) return new Response('{}')
    if (!routes.has(url)) {
      unexpected.push(url)
      throw new Error('unexpected synthetic route')
    }
    return new Response(JSON.stringify(routes.get(url)))
  }
  try {
    const value = await run()
    assert.deepEqual(unexpected, [], 'the reader requested an unserved fixture route')
    assert.ok(requests.length > 0 && requests.every(({ method }) => method === 'GET'), 'the Inbox wrote while reading')
    return { value, requests }
  } finally {
    globalThis.fetch = original
  }
}

test('the singular and plural incomplete-read sentences come from the copy file', () => {
  assert.equal(M.HQ_COPY.change.inbox.unreadable_one, '1 Change could not be read in full. Whether it needs your attention is unresolved.')
  assert.equal(M.HQ_COPY.change.inbox.unreadable_many, '{n} Changes could not be read in full. Whether they need your attention is unresolved.')
})

test('a listed Change with refused or missing detail is unresolved through readInbox', async () => {
  const data = store()
  const detail = M.CHANGE(data.all[0].id)
  for (const options of [{ refused: new Set([detail]) }, { missing: new Set([detail]) }]) {
    const { value: state } = await withResponses(data, options)
    const shown = plain(M.renderInbox(state))
    assert.equal(state.unreadable, 1)
    assert.equal(state.unread, 0, 'an attempted read is not beyond the read limit')
    assert.equal(state.settled, 0)
    assert.deepEqual(state.items, [])
    assert.match(shown, /1 Change could not be read in full\. Whether it needs your attention is unresolved\./)
    assert.doesNotMatch(shown, /Nothing needs your attention right now|Nothing needed|0 Changes are open/)
    assert.ok(state.notes.some(note => note.includes(options.missing ? 'Synthetic Change 0' : detail)), 'the failed detail read is named')
  }
})

test('a ready Change with a refused subordinate read or failed adaptation supplies no action from partial facts', async () => {
  const data = store(1, { ready: true, linked: true })
  const row = data.all[0]
  const id = row.id
  const session = data.docsById.get(id).change.associations[0].session_id
  const subordinate = [M.CHANGE_PROPOSALS(id), M.CHANGE_COVERAGE(id), M.CHANGE_INPUTS(id), M.CHANGE_APPLICABILITY(id), M.CHANGE_FINDINGS(id), M.API.explain + '?session=' + encodeURIComponent(session), M.API.claims + '?session=' + encodeURIComponent(session)]
  for (const route of subordinate) {
    const { value: state } = await withResponses(data, { refused: new Set([route]) })
    const shown = plain(M.renderInbox(state))
    assert.equal(state.unreadable, 1, route)
    assert.equal(state.settled, 0, route)
    assert.deepEqual(state.items, [], route)
    assert.doesNotMatch(shown, /Nothing needs your attention right now|Nothing needed|Approve this version/, route)
    assert.ok(state.notes.some(note => note.includes(route)), `missing refusal note for ${route}`)
    const { value: partial } = await withResponses(data, { refused: new Set([route]) }, () => M.readChangeDocuments(id))
    assert.equal(partial.status, 'ready', 'the Change screen keeps its partial record')
    assert.equal(partial.change.id, id)
    assert.equal(partial.incomplete, true, route)
    assert.ok(partial.notes.some(note => note.includes(route)), route)
  }
  const { value: adapted } = await withResponses(data, { badExplain: true })
  assert.equal(adapted.unreadable, 1)
  assert.equal(adapted.settled, 0)
  assert.deepEqual(adapted.items, [])
  assert.ok(adapted.notes.some(note => note.includes('the explain projection')))
})

test('attempted incomplete Changes, complete rows and Changes beyond READ_LIMIT keep separate counts', async () => {
  const data = store(M.READ_LIMIT + 2)
  const newest = data.all.at(-1).id
  const next = data.all.at(-2).id
  const failures = new Set([M.CHANGE(newest), M.CHANGE_PROPOSALS(next)])
  const { value: state, requests } = await withResponses(data, { refused: failures })
  const shown = plain(M.renderInbox(state))
  assert.equal(state.unreadable, 2)
  assert.equal(state.unread, 2)
  assert.equal(state.settled, M.READ_LIMIT - 2)
  assert.equal(state.total, M.READ_LIMIT + 2)
  assert.equal(state.changes.filter(change => change.hq).length, M.READ_LIMIT - 2, 'complete rows retain their derived truth')
  assert.match(shown, /2 Changes could not be read in full\. Whether they need your attention is unresolved\./)
  assert.match(shown, /2 more Changes are open, and this screen has not looked at them yet\./)
  assert.match(shown, /Nothing needed.*23 Changes/s, 'the fully read settled rows remain visible')
  assert.ok(state.notes.some(note => note.includes(M.CHANGE(newest))))
  assert.ok(state.notes.some(note => note.includes(M.CHANGE_PROPOSALS(next))))
  assert.equal(requests.filter(({ url }) => url.startsWith('/api/changes/') && url.endsWith('/proposals')).length, M.READ_LIMIT - 1)
})

test('a storage-size-only refusal leaves a fully read settled Change settled', async () => {
  const data = store()
  const { value: state } = await withResponses(data, { refused: new Set([M.API.storage]) })
  const shown = plain(M.renderInbox(state))
  assert.equal(state.unreadable, 0)
  assert.equal(state.settled, 1)
  assert.match(shown, /Nothing needs your attention right now/)
  assert.ok(state.notes.some(note => note.includes(M.API.storage)))
})
