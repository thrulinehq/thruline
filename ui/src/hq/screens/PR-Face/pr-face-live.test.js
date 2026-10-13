/* PR-Face on a live address: a preview of the note for a tracked Change's pull
 * request, from the ONE Change snapshot and the tracked Change's own model
 * (C30 R-PR; the R-PR features themselves are pinned in pr-face-r-pr.test.js).
 *
 * What these tests hold the screen to:
 * - every fact in the note is a typed field of the snapshot or the model (a
 *   changed field changes the note; an absent one is said in words, never 0);
 * - what Copy as Markdown copies is the text the preview draws;
 * - every time in the note is UTC with Z, never an offset, on any machine;
 * - nothing on the page fetches anything but its one GET, or posts anything
 *   but the existing track act; there is no post button;
 * - recorded words cannot open a link, an image or a code span of their own;
 * - the live page carries none of the canvas's demo content or notes.
 * Synthetic fixtures only (pr-fixture.js); no record is read.
 */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { prFaceFrom, noteFacts, utcClock } from './from-change.js'
import { composeNote, MAX_ROWS } from './markdown.js'
import { copyMarkdown } from './copy.js'
import { CHANGE_ID, NOW, tracked, snapshot, prDoc } from './pr-fixture.js'
import { checkReceipt } from './read.js'

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
export function instance(props) { const inst = new Logic(props || {}); inst.props = props || {}; return inst }
export function render(props, state) {
  const inst = instance(props)
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'pr-face-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const T = tracked()
const live = (over, t = T, now = NOW) => snapshot(t, over, now)
const data = (snap = live(), t = T, ctx = {}) => prFaceFrom(snap, { now: NOW, tracked: t, pr: { doc: prDoc() }, ...ctx })
const md = (snap, t) => data(snap, t).markdown

test('an explicit receipt check uses the launch token and keeps the passive read separate', async () => {
  const calls = []
  const f = async (url, options) => {
    calls.push({ url, options })
    if (url === '/api/run/token') return { ok: true, status: 200, json: async () => ({ token: 'private-launch-token' }) }
    return { ok: true, status: 200, json: async () => ({ checked: { receipt: 'r.json' } }) }
  }
  const result = await checkReceipt(CHANGE_ID, f)
  assert.equal(result.checked.receipt, 'r.json')
  assert.deepEqual(calls.map((x) => x.url), ['/api/run/token', `/api/changes/${CHANGE_ID}/pr?check=1`])
  assert.equal(calls[1].options.method, 'GET')
  assert.equal(calls[1].options.headers['X-ThruLine-Run-Token'], 'private-launch-token')
})

test('a checked receipt draws the CLI title, conclusion and badge only after the explicit act', () => {
  const doc = prDoc({ receipts: { dir: '.thruline/receipts', count: 1, newest: 'r.json', newest_fingerprint: '1234:200', reason: null } })
  const base = prFaceFrom(live(), { now: NOW, tracked: T, pr: { doc } })
  const before = M.render({ data: base })
  assert.match(before, /Check this receipt/)
  assert.doesNotMatch(before, /role="img"[^>]*alt="thruline: partial"/)

  const checked = {
    receipt: 'r.json', head: doc.git.head, fingerprint: '1234:200',
    check_run: { name: 'thruline/receipt', conclusion: 'neutral', output: { title: 'Receipt checked: partial' } },
    badge: { aria_label: 'thruline: partial', svg: '<svg role="img" aria-label="thruline: partial"></svg>' },
  }
  const checkedData = prFaceFrom(live(), { now: NOW, tracked: T, pr: { doc }, checked })
  const after = M.render({ data: checkedData })
  assert.deepEqual(checkedData.checkRun.title, ['Receipt checked: partial'])
  assert.deepEqual(checkedData.checkRun.lines, [['neutral']])
  assert.equal(checkedData.badge.image.svg, checked.badge.svg)
  assert.match(after, /Receipt checked: partial/)
  assert.match(after, /neutral/)
  assert.match(after, /alt="thruline: partial"/)
  assert.match(after, /aria-label="thruline: partial"/)
  assert.match(after, /src="data:image\/svg\+xml,/)

  const reread = prDoc({ read_at: '2026-10-07T07:00:00Z', receipts: doc.receipts })
  assert.match(M.render({ data: prFaceFrom(live(), { now: NOW, tracked: T, pr: { doc: reread }, checked }) }), /Receipt checked: partial/)
  const changed = prDoc({ receipts: { dir: '.thruline/receipts', count: 1, newest: 'other.json', newest_fingerprint: '1235:200', reason: null } })
  const stale = M.render({ data: prFaceFrom(live(), { now: NOW, tracked: T, pr: { doc: changed }, checked }) })
  assert.doesNotMatch(stale, /Receipt checked: partial/)
})

/* ------------------------------------------------------------ facts */

test('the four lines state the model’s own readiness, and one row stands for each line of what counts as done', () => {
  const m = md()
  assert.match(m, /\*\*○ Not proven — 3 things still need evidence before `bbbbbbb` can be approved\.\*\*/)
  assert.match(m, /This version can't be approved yet: a line of what counts as done was checked, and the files it looked at have changed since\./)
  assert.match(m, /\*\*To settle it:\*\* run the Change’s check on `bbbbbbb`, or \*Check it again\* in ThruLine\./)
  assert.match(m, /\*\*For approval:\*\* approval of `bbbbbbb` waits on 3 of 4 lines\. Not approved\./)
  assert.match(m, /Where the proof stops: ⑤ RE-CHECKED ×1 · ② RESTS ON ×1/)
  assert.match(m, /\| ● \| The total adds up \| Proven \| accepted 18:35Z → Proven: pytest -q tests · version bbbbbbb · cart\/ \| — \|/)
  assert.match(m, /\| ◭ \| A discount applies once \| Needs a re-check \| accepted 18:35Z → `pytest -q tests` → Proven on the previous version — needs a re-check\. \| ⑤ \|/)
  assert.match(m, /\| ◐ \| No discount leaves the total unchanged \| In progress \| accepted 18:35Z → No test ran for this\. \| ② \|/)
  assert.match(m, /\| ○ \| The API reports the total \| Not proven \| accepted 18:35Z → `pytest -q tests` → Not proven · version bbbbbbb \| — \|/)
  assert.match(m, /<summary>What the agent said, and what the record shows \(2 statements · 1 a run can check\)<\/summary>/)
  assert.match(m, /- ○ ❝Tests pass\.❞ \(step 4\) — Not proven: said at step 4 → step 3, 7 passed, 0 failed, on `aaaaaaa` → `cart\/api\.py` changed \(step 5\) → nothing on `bbbbbbb`/)
  assert.match(m, /<summary>Runs, failures first — 1 finished, none on <code>bbbbbbb<\/code> · 1 with no result yet<\/summary>/)
  assert.match(m, /`bbbbbbb` holds unsaved changes that no commit contains yet\./)
})

test('a changed field changes the note: the rows and numbers are read, not written', () => {
  const t = tracked({ states: ['proven', 'proven', 'stale', 'none'] })
  const m = md(live(undefined, t), t)
  assert.match(m, /waits on 2 of 4 lines/)
  assert.match(m, /\| ● \| A discount applies once \| Proven \|/)
  assert.match(m, /\| ◭ \| No discount leaves the total unchanged \| Needs a re-check \|/)
  assert.doesNotMatch(m, /3 things still need/)
  const s = live({ commands: [{ id: 'c1', session_id: 's1', ts: '2026-09-29T18:10:00Z', command: 'pytest -q tests', is_test: true, outcome: 'succeeded', exit_code: 0, tally: { passed: 9, failed: 0 }, version_id: 'aaaaaaa1' }] })
  assert.match(md(s), /step 3, 9 passed, 0 failed/)
  assert.doesNotMatch(md(s), /7 passed/)
})

test('ready and approved come only from the model’s readiness and the standing approval', () => {
  const r = tracked({ ready: true, states: ['proven', 'proven', 'proven', 'proven'] })
  const mr = md(live(undefined, r), r)
  assert.match(mr, /\*\*◇ Ready for approval for version `bbbbbbb`\.\*\*/)
  assert.match(mr, /\*\*For approval:\*\* not approved yet\. ThruLine never approves for you\./)
  assert.doesNotMatch(mr, /Approved on/)
  /* the snapshot's stages say nothing on their own: an "approve done" stage with no standing row is not approval */
  const s = live(undefined, r)
  s.change.stages = s.change.stages.map((x) => (x.id === 'approve' ? { ...x, status: 'done' } : x))
  assert.doesNotMatch(md(s, r), /◆ Approved/)
})

test('a fact the record does not hold is said in words, never 0', () => {
  const s = live()
  s.masking = null
  s.change.title = null
  const m = data(s).markdown
  assert.match(m, /masking not recorded/)
  assert.match(m, /ThruLine · a Change with no recorded title/)
  assert.match(m, /⋯ `… pytest -q` · no result yet · on `bbbbbbb` · step \d+ · 18:25Z/)
  assert.doesNotMatch(m, /0 passed, 0 failed/)
  const none = data({ ...s, currentVersion: null, versions: [] }).markdown
  assert.match(none, /no recorded version/)
  assert.match(none, /ThruLine has recorded no version for this Change\./)
  /* a Change with nothing accepted: no table, said in words */
  const t = tracked({ unaccepted: true })
  const d = data(live(undefined, t), t)
  assert.equal(d.note.table, null)
  assert.deepEqual(d.note.tableNote, ['What counts as done is not accepted yet, so there is no line to follow link by link.'])
  assert.match(d.markdown, /\*\*▪ Recorded — what counts as done is not accepted yet, so nothing is measured on `bbbbbbb`\.\*\*/)
  assert.match(d.markdown, /\*\*For approval:\*\* what counts as done is not accepted yet, so there is nothing to approve\. Not approved\./)
  assert.doesNotMatch(d.markdown, /Where the proof stops/)
})

test('a long list is cut with its count said, never silently', () => {
  const f = noteFacts(live(), { now: NOW, tracked: T })
  const lines = Array.from({ length: MAX_ROWS + 3 }, (_, i) => ({ ...f.lines[0], n: i + 1, text: 'Line ' + i }))
  const n = composeNote({ ...f, lines })
  assert.equal(n.table.rows.length, MAX_ROWS)
  assert.deepEqual(n.tableAfter, ['3 more lines of what counts as done are not in this table; ThruLine holds every one.'])
})

/* ------------------------------------------------------------ time */

test('every time in the note is UTC with Z, never an offset, on a machine in any zone', () => {
  const was = process.env.TZ
  process.env.TZ = 'Asia/Tokyo'
  try {
    const d = data()
    assert.doesNotMatch(d.copy, /\+09:?00|JST|UTC\+|GMT/)
    assert.match(d.copy, /as read at 2026-10-07 07:00Z; every time in this note is UTC/)
    assert.match(d.copy, /Not recorded: anything before 2026-09-29 18:05Z\./)
    assert.match(d.copy, /step \d+ · 18:10Z/)
    for (const t of d.copy.match(/\b\d\d:\d\d\b(?!Z)/g) || []) assert.fail('a time without Z: ' + t)
    assert.equal(utcClock('2026-09-29T18:10:00+09:00'), '09:10Z')
  } finally {
    if (was === undefined) delete process.env.TZ
    else process.env.TZ = was
  }
})

/* ------------------------------------------------------------ preview == Markdown */

const unescapeHtml = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')

/** The words drawn: the comment body (without the annotation discs and the
 *  "hidden when rendered" label), then the setup line under it. */
function previewText(html) {
  const a = html.indexOf('padding:14px 18px 16px')
  const b = html.indexOf('</article>')
  assert.ok(a > 0 && b > a, 'the comment body is drawn')
  let s = html.slice(html.lastIndexOf('<div', a), b)
  const u = html.indexOf('<p id="pr-setup"')
  assert.ok(u > b, 'the setup line is drawn under the comment')
  s += html.slice(u, html.indexOf('</p>', u))
  s = s.replace(/<span aria-hidden="true" style="position:absolute;right:-40px[^"]*">[A-F]<\/span>/g, '')
  s = s.replace(/hidden when rendered/, '')
  s = s.replace(/<span aria-hidden="true" style="flex:0 0 12px;color:#A1A1AA;font-size:12px">[▾▸]<\/span>/g, '')
  s = s.replace(/<span aria-hidden="true" style="color:#A1A1AA">•<\/span>/g, '')
  s = s.replace(/<!-- -->/g, '').replace(/<[^>]+>/g, '')
  return unescapeHtml(s).replace(/\s+/g, '')
}

/** The words of the Markdown, without its syntax. */
function markdownText(m) {
  return m
    .split('\n')
    .filter((l) => !/^\|( *:?-+:? *\|)+$/.test(l))
    .map((l) => l.replace(/^- /, ''))
    .join('\n')
    .replace(/<\/?(details|summary|code|b|i)( open)?>/g, '')
    .replace(/\\\|/g, '\u0000')
    .replace(/\|/g, '')
    .replace(/\u0000/g, '|')
    .replace(/\*\*/g, '')
    .replace(/(^|[^\\])\*/g, '$1')
    .replace(/`/g, '')
    .replace(/\\([!-/:-@[-`{-~])/g, '$1')
    .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
    .replace(/\s+/g, '')
}

test('Copy as Markdown copies the text the preview draws — the note, then the setup line — with every fold open', () => {
  const ready = tracked({ ready: true, states: ['proven', 'proven', 'proven', 'proven'] })
  for (const [snap, t] of [[live(), T], [live(undefined, ready), ready]]) {
    const d = data(snap, t)
    const toggled = Object.fromEntries(d.note.layers.map((l, i) => [i, !l.open]))
    const html = M.render({ data: d }, { toggled })
    assert.equal(previewText(html), markdownText(d.copy))
  }
})

test('the comparison above is not vacuous: a different text is a different text', () => {
  const d = data()
  const html = M.render({ data: d })
  assert.notEqual(previewText(html), markdownText(d.copy.replace('The total adds up', 'The total adds')))
})

/* ------------------------------------------------------------ safety */

test('recorded words cannot open a link, an image, HTML or a code span of their own', () => {
  const t = tracked()
  t.model.criteria[0] = { ...t.model.criteria[0], text: 'ok `x` <img src=x onerror=alert(1)> [here](http://evil.example) | a' }
  const s = live({
    claims: [{ id: 'k9', event_id: 'm9', session_id: 's1', ts: '2026-09-29T18:12:00Z', text: 'ok `x` <img src=x onerror=alert(1)> [here](http://evil.example) | a', kind: 'tests_pass', has_verifier: true, rests_on_command_id: 'c1', version_id: 'aaaaaaa1' }],
    commands: [{ id: 'c1', session_id: 's1', ts: '2026-09-29T18:10:00Z', command: 'pytest `whoami` | tee out', is_test: true, outcome: 'succeeded', exit_code: 0, tally: { passed: 1, failed: 0 }, version_id: 'aaaaaaa1' }],
  }, t)
  s.change.title = '<script>x</script> **bold** [t](javascript:1)'
  const m = data(s, t).copy
  assert.doesNotMatch(m, /<img|<script|(?<!\\)\]\(/)
  assert.doesNotMatch(m, /`pytest `whoami`/)
  const row = m.split('\n').find((l) => l.startsWith('| ●'))
  assert.ok(row, 'the line has its row')
  assert.equal(row.split(/(?<!\\)\|/).length, 7, 'a pipe in recorded words does not add a table column')
  const tags = new Set((m.match(/<\/?[a-z]+/g) || []))
  for (const tg of tags) assert.ok(['<details', '</details', '<summary', '</summary', '<code', '</code'].includes(tg), 'unexpected tag ' + tg)
  assert.ok(m.startsWith('<!-- thruline:note change=cccccccc-0000-4000-8000-0000000000a1 version=bbbbbbb criteria=v1 schema=note/1 -->\n'))
})

test('a path or command never carries a home folder into the note', () => {
  const m = data(live({ edits: [
    { id: 'ed1', session_id: 's1', ts: '2026-09-29T18:15:00Z', file: '/Users/someone/elsewhere/notes/a.md', version_id: 'bbbbbbb2' },
  ] })).copy
  assert.match(m, /`…\/notes\/a\.md` changed \(step \d+\)/, 'the edit after the run is in the chain')
  assert.doesNotMatch(m, /\/Users\/|someone/)
})

/* ------------------------------------------------------------ the page */

test('the live page has one primary action, no post button, and says it posts nothing', () => {
  const html = unescapeHtml(M.render({ data: data() }))
  assert.equal((html.match(/class="tl-btn"/g) || []).length, 1)
  assert.match(html, /<a href="#\/changes\/cccccccc-0000-4000-8000-0000000000a1\/approve\?project=p1" class="tl-btn"[^>]*>The approval it waits on<span aria-hidden="true">→<\/span><\/a>/)
  assert.match(html, /<button type="button" class="tl-ghost"[^>]*>Copy as Markdown<\/button>/)
  assert.doesNotMatch(html, /Post to the pull request|pr-post-why|Not available in this version/)
  assert.match(html, /This is the note for this Change’s pull request, read from the record\./)
  assert.match(html, /ThruLine posts nothing and makes no network call\./)
})

test('the live page carries none of the canvas’s demo content, notes or trial', () => {
  for (const d of [data(), prFaceFrom(snapshot(null), { now: NOW })]) {
    const html = unescapeHtml(M.render({ data: d }))
    for (const w of ['Illustrative', 'NEEDS ENGINE', 'ENGINE', 'ashwin', 'checkout-web', 'e71c5b0', '3f9a1c2', '#482', '+00:00', 'R17', 'coderabbit', '16:09', 'DEMO', 'trial', 'Trial', 'posted with gh', 'thruline change note', 'Why this beats a walkthrough', 'Six contrasts', 'sha256:…']) {
      assert.ok(!html.includes(w), 'live page says ' + JSON.stringify(w))
    }
  }
  const gallery = unescapeHtml(M.render({}))
  for (const w of ['Illustrative: PR #482', 'The check run', 'Why this beats a walkthrough', '16:09 · the same comment, edited in place', 'The approval it waits on']) {
    assert.ok(gallery.includes(w), 'gallery lost ' + JSON.stringify(w))
  }
  assert.ok(!gallery.includes('Post to the pull request'), 'the gallery never drew a post button')
})

/* ------------------------------------------------------------ no network */

test('nothing on this page fetches or posts by itself: Copy as Markdown is the clipboard only', async () => {
  const realFetch = globalThis.fetch
  const calls = []
  globalThis.fetch = (...a) => { calls.push(a); return Promise.reject(new Error('no network')) }
  try {
    const d = data()
    let copied = null
    const inst = M.instance({ data: d, actions: { copy: (t) => copyMarkdown(t, { clipboard: { writeText: async (x) => { copied = x } } }) } })
    inst.renderVals().onCopy()
    await new Promise((r) => setTimeout(r, 10))
    assert.equal(copied, d.copy)
    assert.equal(inst.state.copy, 'done')
    assert.equal(inst.renderVals().copyLabel, 'Copied')
    const bare = M.instance({ data: d, actions: { copy: (t) => copyMarkdown(t, {}) } })
    bare.renderVals().onCopy()
    await new Promise((r) => setTimeout(r, 10))
    assert.equal(bare.state.copy, 'failed')
    assert.match(bare.renderVals().copyStatus, /did not let ThruLine copy/)
    M.render({ data: d })
    assert.equal(calls.length, 0)
  } finally {
    globalThis.fetch = realFetch
  }
  /* passive view and Copy name no write address; the explicit check reuses GET
   * and gets the launch token only after its button is pressed. */
  for (const f of readdirSync(here).filter((x) => /\.(js|jsx)$/.test(x) && !x.endsWith('.test.js') && x !== 'pr-fixture.js')) {
    const src = readFileSync(join(here, f), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
    assert.doesNotMatch(src, /\bfetch\(|XMLHttpRequest|sendBeacon|WebSocket|method:\s*['"]POST/, f + ' reaches the network')
    const addresses = src.match(/['"`]\/api\/[^'"`]*/g) || []
    assert.deepEqual(addresses, f === 'read.js' ? ["'/api/changes/", "'/api/run/token"] : [], f + ' names an address')
  }
})
