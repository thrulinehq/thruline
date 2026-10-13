/* Census §3.08 row 4: ⌘K's Find rows search the record across Changes —
 * including what the agent READ and each VERSION — through
 * GET /api/hq/projects/<id>/search (data/search.js). Each row links to the
 * Change screen that shows it: never the Inbox in its place, never a raw
 * `.dc.html`. The palette is rendered through template + logic with the live
 * data and a served search document; nothing is fetched. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { changeSnapshot } from '../../data/change.js'
import { linker } from '../../routes.js'
import { commandFromChange } from './from-change.js'
import { searchHitRow, searchFind } from './live.js'

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
function make(props, state) { const inst = new Logic(props || {}); inst.props = props || {}; if (state) inst.state = { ...inst.state, ...state }; return inst }
export function render(props, state, link) { return renderToStaticMarkup(React.createElement(Template, { v: make(props, state).renderVals(), link })) }
export function vals(props, state) { return make(props, state).renderVals() }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-command-search-')), 'command.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')

const A = '0a0a0a0a0a0a0a0a' // the open Change
const B = '0b0b0b0b0b0b0b0b' // another Change
const T = (m) => '2026-10-07T09:0' + m + ':00.000Z'
const VIEW = {
  project: { id: 'p1', name: 'shop', root: '/r' },
  thread: { id: A, title: 'Fix the total', whose_move: 'you', proof: { edited_after_last_pass: [] } },
  prompts: [{ event_id: 'ev-p', session_id: 's1', ts: T(0), text: 'fix the totals', work: true }],
  edits: [{ id: 'ev-e1#0', ts: T(1), file: '/r/cart/totals.py', lines_added: 2, lines_removed: 1, ok: true, version_id: '3f9a1c2', session_id: 's1' }],
  commands: [], claims: [],
  versions: [{ id: '3f9a1c2', short: '3f9a1c2', commit: '3f9a1c2deadbeef', committed: true }],
  current_version_id: '3f9a1c2',
  sessions: [{ id: 's1', source: 'claude_code', origin: 'person', started_at: T(0) }],
  files: [],
}
const PROJECT_VIEW = { project: { id: 'p1', name: 'shop', root: '/r' }, threads: [{ id: A, title: 'Fix the total', whose_move: 'you', top_files: [] }, { id: B, title: 'Write the migration guide', whose_move: 'done', top_files: [] }] }
const NOW = new Date('2026-10-07T10:00:00Z')
const open = commandFromChange(changeSnapshot(VIEW, null, NOW), PROJECT_VIEW, { readAt: '10:00', deletions: [] })
const closed = commandFromChange(null, PROJECT_VIEW, { readAt: '10:00' })

const hit = (o) => ({ thread_title: 'Write the migration guide', open_thread: false, at: T(5), event_id: 'ev-' + o.kind, count: 1, source: 'claude_code', work: null, outcome: null, exit_code: null, tally: null, verdict: null, claim_kind: null, version: null, thread_id: B, ...o })
const DOC = {
  schema_version: 1, project_id: 'p1', thread: A, limit: 20, total: 9, by_kind: {},
  results: [
    hit({ kind: 'Read', thread_id: A, thread_title: 'Fix the total', open_thread: true, text: 'cart/totals.py', count: 2 }),
    hit({ kind: 'Version', thread_id: A, thread_title: 'Fix the total', open_thread: true, text: '3f9a1c2', version: { id: '3f9a1c2', short: '3f9a1c2', commit: '3f9a1c2deadbeef', committed: true, message: 'Apply the discount to the totals once', current: true } }),
    hit({ kind: 'File', text: 'docs/totals.md', count: 3 }),
    hit({ kind: 'Said', text: 'Totals guide written.', verdict: 'recorded', claim_kind: 'done' }),
  ],
  searched: {}, masking: { masked: 0, families: [], complete: true },
}
const served = (q, doc = DOC) => ({ status: 'served', q, doc })
const AT = { screen: 'brief', change: A, project: 'p1' }
const props = (found, data = open, at = AT) => ({ data, found, at, overlay: true, projectId: 'p1' })
const html = (p, q) => unescape(M.render(p, { query: q, active: '' }, linker(p.at)))

test('Find shows what the agent read and each version, each linking to its Change screen', () => {
  const v = M.vals(props(served('totals')), { query: 'totals', active: '' })
  const kinds = v.findRows.map((r) => r.kind)
  assert.ok(kinds.includes('Read'), kinds.join())
  assert.ok(kinds.includes('Version') || v.sec.findMoreShow, kinds.join())
  const read = v.findRows.find((r) => r.kind === 'Read')
  assert.equal(read.text, 'cart/totals.py')
  assert.equal(read.href, '#/changes/' + A + '?project=p1')
  assert.match(read.meta, /^read by Claude Code, 2 times · \d\d:\d\d$/)
  // the rest of the record is counted in the more line, from the route's own total
  // 3 in this Change (its request, edit and file) + the 9 the search found, 4 shown
  assert.equal(v.sec.findMoreText, (3 + DOC.total - 4) + ' more in the record · keep typing to narrow it')
})

test('every row the search adds names its Change screen: Read and Version the Brief, File What changed, Said by its verdict', () => {
  const at = (h) => searchHitRow(h, { when: () => '09:05' })
  const screens = Object.fromEntries(DOC.results.map((h) => [h.kind, at(h).screen]))
  assert.deepEqual(screens, { Read: 'brief', Version: 'brief', File: 'changed', Said: 'brief' })
  assert.equal(at(hit({ kind: 'Said', text: 'Tests pass.', verdict: 'not_proven' })).screen, 'statements')
  const v = at(DOC.results[1])
  assert.equal(v.meta, 'current · commit “Apply the discount to the totals once” · 09:05')
  const other = at(DOC.results[2])
  assert.equal(other.change, B)
  assert.match(other.meta, /^changed 3 times · Write the migration guide · 09:05$/, 'a row of another Change names it')
})

test('a found row says recorded words as every Change screen does: no home folder, no markdown markers', () => {
  const at = (h) => searchHitRow(h, { when: () => '09:05' })
  const said = at(hit({ kind: 'Said', text: 'Added **the notes** to `/Users/sam/code/app/notes.md`.', verdict: 'recorded', thread_title: 'Tidy /Users/sam/code/app' }))
  assert.equal(said.text, 'Added the notes to ~/code/app/notes.md.')
  assert.equal(said.title, said.text)
  assert.doesNotMatch(said.meta, /\/Users\/sam/)
  const cmd = at(hit({ kind: 'Command', text: 'cat /Users/sam/.zshrc', outcome: 'succeeded' }))
  assert.equal(cmd.text, 'cat ~/.zshrc')
})

test('no row links to the Inbox in its place, and no raw .dc.html survives', () => {
  for (const [p, q] of [[props(served('totals')), 'totals'], [props(served('guide'), closed, { screen: 'inbox', project: 'p1' }), 'guide']]) {
    const out = html(p, q)
    const hrefs = (out.match(/href="[^"]*"/g) || []).map((h) => h.slice(6, -1))
    const find = hrefs.filter((h) => h.startsWith('#/changes/'))
    assert.ok(find.length >= 1, out.slice(0, 200))
    assert.ok(!hrefs.some((h) => /\.dc\.html/.test(h)), hrefs.join(' '))
    const rows = (out.match(/id="tl-opt-find-[^"]*"[^>]*href="[^"]*"|href="[^"]*"[^>]*id="tl-opt-find-[^"]*"/g) || [])
    assert.ok(rows.every((r) => !/#\/inbox/.test(r)), rows.join('\n'))
  }
})

test('with no Change open, the search runs across every Change of the project', () => {
  const v = M.vals(props(served('guide', { ...DOC, thread: null, results: DOC.results.map((h) => ({ ...h, open_thread: false })) }), closed, { screen: 'inbox', project: 'p1' }), { query: 'guide', active: '' })
  const fromSearch = v.findRows.filter((r) => r.kind !== 'Change')
  assert.ok(fromSearch.length > 0)
  for (const r of fromSearch) assert.match(r.href, /^#\/changes\/[0-9a-f]{16}(\/(changed|statements))?\?project=p1$/)
})

test('an answer for other text is never drawn as this text’s; the hint says the rest is being searched', () => {
  const v = M.vals(props(served('tot')), { query: 'totals', active: '' })
  assert.ok(!v.findRows.some((r) => r.kind === 'Read'), 'the answer was for "tot"')
  assert.match(v.sec.findHint, /searching the rest of shop…$/)
  assert.equal(v.sec.findEmptyShow, false, 'nothing found is not said while the record is still answering')
  const reading = M.vals(props({ status: 'reading', q: 'totals' }), { query: 'totals', active: '' })
  assert.match(reading.sec.findHint, /searching the rest of shop…$/)
  // nothing here matches, and the rest of the record has not answered: no "nothing contains" yet
  const unanswered = M.vals(props({ status: 'reading', q: 'zzzz' }), { query: 'zzzz', active: '' })
  assert.equal(unanswered.findRows.length, 0)
  assert.equal(unanswered.sec.findEmptyShow, false)
  const answered = M.vals(props(served('zzzz', { ...DOC, total: 0, results: [] })), { query: 'zzzz', active: '' })
  assert.equal(answered.sec.findEmptyShow, true)
  assert.deepEqual(searchFind({ status: 'served', q: 'tot', doc: DOC }, 'totals', { when: () => '' }).state, 'reading')
})

test('a search that failed is said in place, and "nothing found" then speaks only for this Change', () => {
  const failed = { status: 'failed', q: 'zzzz', message: 'ThruLine could not search the rest of the record just now. Type again to retry.' }
  const v = M.vals(props(failed), { query: 'zzzz', active: '' })
  assert.equal(v.sec.findMoreShow, true)
  assert.equal(v.sec.findMoreText, failed.message)
  assert.equal(v.sec.findEmptyShow, true)
  assert.match(v.sec.findEmptyText, /^Nothing in this Change’s steps, files or commands contains “zzzz”\.$/)
  const out = html(props(failed), 'zzzz')
  assert.ok(out.includes(failed.message))
})

test('the silent answer counts what the search found too', () => {
  const v = M.vals(props(served('was a migration run?', { ...DOC, total: 3, results: DOC.results.slice(2) })), { query: 'was a migration run?', asked: 'typed', askedText: 'was a migration run?', active: 'ask-typed' })
  assert.equal(v.answer.silent, true)
  assert.match(v.answer.missing, /^4 things in the record mention “migration” \(listed under Find\)/, "1 Change title here and 3 the search found")
  assert.match(v.answer.lookedFor, /, and in the rest of shop\.$/)
  // "Try" offers two of the design's five, by their own words
  assert.deepEqual([v.answer.tryChangedText, v.answer.tryDestructiveText], ['What changed after the last check?', 'Was anything destructive?'])
  const waiting = M.vals(props({ status: 'reading', q: 'was a migration run?' }), { query: 'was a migration run?', asked: 'typed', askedText: 'was a migration run?', active: 'ask-typed' })
  assert.match(waiting.answer.missing, /The rest of shop is still being searched\.$/, 'never "none" before the record has answered')
})

test('the design gallery is untouched: no search, no live rows', () => {
  const g = M.vals({}, { query: 'check', active: '' })
  assert.ok(g.findRows.every((r) => /\.dc\.html$/.test(r.href)), 'the canvas rows keep the canvas links in the gallery')
  assert.equal(g.sec.findHint, 'in the 19 recorded steps')
})

test('a served Read hit remains visible when local file matches fill the Find rows', () => {
  const manyLocal = {
    ...open,
    findable: Array.from({ length: 5 }, (_, i) => ({
      id: `local-${i}`, kind: 'File', path: `cart/totals-${i}.py`,
      changedAt: [i + 1], runAt: [], uncovered: false, lastVersion: null,
    })),
  }
  const v = M.vals(props(served('totals'), manyLocal), { query: 'totals', active: '' })
  assert.ok(v.findRows.some((r) => r.kind === 'Read' && r.text === 'cart/totals.py'),
    'the recorded Read hit must not be hidden behind the local first four matches')
})

test('a copied live answer uses the read time shown on the palette', async () => {
  const copied = []
  Object.defineProperty(globalThis, 'navigator', {
    configurable: true,
    value: { clipboard: { writeText: async (text) => { copied.push(text) } } },
  })
  const v = M.vals(props(served('changed')), { query: 'changed', asked: 'changed', active: 'ask-changed' })
  v.answer.copy()
  await new Promise((resolve) => setTimeout(resolve, 0))
  assert.equal(copied.length, 1)
  const readAt = /^read (.+) ↻$/.exec(v.top.readStamp)?.[1]
  assert.ok(readAt, 'the palette displays a read time')
  assert.ok(copied[0].endsWith(`From the ThruLine record of shop, read ${readAt}.`))
})

test('slash entry selects the first Ask row while its field stays available', () => {
  const v = M.vals({ ...props(null), askFirst: true }, { query: '', active: 'act-check' })
  assert.match(v.activeDomId, /^tl-opt-ask-/)
})

test('the palette follows its own dot, y and g shortcuts from outside the field', () => {
  const followed = []
  const doc = { getElementById: (id) => ({
    getAttribute: (key) => key === 'href' ? '#/target' : null,
    click: () => followed.push(id),
  }) }
  const e = (key, code) => ({ key, code, target: { ownerDocument: doc }, preventDefault() {} })
  const v = M.vals(props(null), { query: '', active: 'act-check' })
  v.onPaletteKey(e('.', 'Period'))
  v.onPaletteKey(e('y', 'KeyY'))
  for (const [key, code] of [['d', 'KeyD'], ['l', 'KeyL'], ['w', 'KeyW'], ['i', 'KeyI']]) {
    v.onPaletteKey(e('g', 'KeyG'))
    v.onPaletteKey(e(key, code))
  }
  assert.deepEqual(followed, [
    'tl-opt-act-check', 'tl-opt-act-tell', 'tl-opt-act-define',
    'tl-opt-go-statements', 'tl-opt-go-story', 'tl-opt-go-inbox',
  ])
})

test('the answer’s Check action follows the served run-aware check address', () => {
  const checkHref = '#/changes/' + A + '/check?project=p1&run=recorded'
  const data = { ...open, answers: { ...open.answers, finish: {
    ...open.answers.finish, unanswered: false, state: 'not_proven', word: 'Not proven',
    lines: [], evidence: [], action: 'check',
  } } }
  const p = { ...props(null, data), actions: { checkHref } }
  const v = M.vals(p, { query: '', asked: 'finish', active: 'ask-finish' })
  assert.equal(v.answer.checkHref, checkHref)
  assert.equal(v.answer.actCheck, true)
  const answered = M.render(p, { query: '', asked: 'finish', active: 'ask-finish' }, linker(AT))
  const action = answered.match(/<a href="([^"]+)" class="tl-btn" aria-keyshortcuts="\."/)
  assert.ok(action, 'the answer draws a Check action')
  assert.equal(unescape(action[1]), checkHref)
})
