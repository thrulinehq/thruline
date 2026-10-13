/* C30 R-ROUTES "Export and Download" on The whole story (FEATURE-CENSUS
 * §3.13 rows 2, 6, 7, 8 and 9), through the page's own logic and template.
 *
 *  6. Markdown / HTML / JSON: the picked format makes a real file of it.
 *  8. Download saves exactly the text the preview shows, under the T-5-4 name
 *     `thruline-packet-<change>-<version7>[-redacted].<ext>`; Copy as Markdown
 *     copies the Markdown file's text. Nothing is posted (the live test).
 *  7. The redacted copy is a second, labelled file: classes and counts at the
 *     top, a marker in place of every removal, `derived_from_sha256` naming the
 *     full file. Each class switch changes what is removed; each count is the
 *     removals actually made.
 *  9. Readable now draws, per record, whether it can be read now and why not,
 *     in the route's own words; untracked work says it applies to a tracked
 *     Change only, never "all readable".
 *  2. Plain / Technical: Technical draws the file's own technical part.
 *  One source: every count and sentence in the file is the one the page shows.
 *
 * Nothing here reaches a server or a store: the file is captured from the
 * screen's `io`, and the records are a synthetic `GET …/export` answer. */
import { wholeStoryFrom, MAX_RUNS } from './from-change.js'
import { DEMO as BRIEF } from '../Change-Brief/demo.js'
import { sha256Hex } from '../../../workspace/packet-export.js'
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
export function instance(props, state) {
  const inst = new Logic(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return inst
}
export function draw(v) { return renderToStaticMarkup(React.createElement(Template, { v, link: (h) => h })) }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'ws-export-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
/** What a person reads on the page: the drawn text, and every aria-label and title. */
const words = (html) => {
  const h = unescape(html)
  const attrs = (h.match(/(?:aria-label|title)="[^"]*"/g) || []).map((a) => a.replace(/^[^"]*"|"$/g, '')).join('\n')
  return h.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ') + '\n' + attrs
}

/* ---- fixtures: the Change-Brief snapshots, a tracked Change, a served /export answer ---- */
const HOME = '/Users/someone'
const S1 = BRIEF.states.s1
const READY = BRIEF.states.ready
const ROOT = READY.project.worktree // '~/code/checkout-web'
const TRACKED = {
  change: {
    id: 'chg_1', accepted_scope: { acceptance: { accepted_at: '2026-09-26T10:36:00Z' } },
    decisions: {
      standing: { id: 'a2', target_revision: 'e71c5b0aaaa', actor: 'Sam', decided_at: '2026-09-26T10:40:00Z', scope_version: 1 },
      history: [
        { id: 'a1', act: 'approved', target_revision: '9f2ad3eaaaa', actor: 'Sam', decided_at: '2026-09-26T10:37:00Z', scope_version: 1, record_path: HOME + '/code/checkout-web/.thruline/decisions/9f2ad3e.json' },
        { id: 'w1', act: 'withdrawn', withdraws: 'a1', target_revision: '9f2ad3eaaaa', actor: 'Sam', decided_at: '2026-09-26T10:38:00Z' },
        { id: 'a2', act: 'approved', target_revision: 'e71c5b0aaaa', actor: 'Sam', decided_at: '2026-09-26T10:40:00Z', scope_version: 1, record_path: HOME + '/code/checkout-web/.thruline/decisions/e71c5b0.json' },
      ],
    },
  },
  hq: { version: { short: 'e71c5b0' }, criteria: [
    { n: 1, text: 'Totals are right once a discount applies', state: 'proven', proven: { version: 'e71c5b0', scope: ['checkout/', 'tests/test_totals.py'] } },
    { n: 2, text: 'A second code does not stack', state: 'needs_recheck' },
  ] },
}
const SNAP_T = { ...READY, change: { ...READY.change, id: 'chg_1' }, criteria: { ...READY.criteria, accepted: true, lineCount: 2, provenOn: { count: 1 } } }
const DATA_T = () => wholeStoryFrom(SNAP_T, TRACKED)
const DATA_U = () => wholeStoryFrom(S1, null)

test('notes, counting second approvals and objections are present in each exported format', () => {
  const data = DATA_T()
  data.notes = [{ id: 'n1', who: 'Alex', when: '2026-10-09 12:01', body: 'Checked the discount boundary.' }]
  data.seconds = [{ id: 's1', who: 'Morgan', when: '2026-10-09 12:02' }]
  data.objected = [{ id: 'claim1', step: 52, plain: 'Tests pass.', verdictWord: 'Not proven', objections: [{ id: 'o1', lead: 'You said this is wrong:', reason: 'The total still changes.', meta: 'Alex · 2026-10-09 12:03', matches: true }] }]
  for (const fmt of ['md', 'html', 'json']) {
    const file = fileOf({ data, readable: READ_OK }, { fmt })
    for (const phrase of ['Checked the discount boundary.', 'Morgan', 'The total still changes.', 'Not proven']) assert.ok(file.text.includes(phrase), fmt + ' ' + phrase)
    if (fmt === 'json') {
      const sections = JSON.parse(file.text).sections
      for (const id of ['notes', 'seconds', 'objections']) assert.ok(sections.some((s) => s.id === id), id)
    }
  }
})

/* Every standing word the route serves (crates/core/src/change_export.rs `Standing`). */
const RECORDS = {
  schema_version: 1, change_id: 'chg_1', checked_at: '2026-09-26T10:45:00Z',
  worktree: HOME + '/code/checkout-web',
  decisions: [
    { decision_id: 'a1', act: 'approved', version: '9f2ad3eaaaa', path: HOME + '/code/checkout-web/.thruline/decisions/9f2ad3e.json', standing: 'content_changed' },
    { decision_id: 'w1', act: 'withdrawn', version: '9f2ad3eaaaa', path: null, standing: null },
    { decision_id: 'a2', act: 'approved', version: 'e71c5b0aaaa', path: HOME + '/code/checkout-web/.thruline/decisions/e71c5b0.json', standing: 'readable' },
    { decision_id: 'a0', act: 'approved', version: '4b19a0caaaa', path: null, standing: 'not_written' },
  ],
  evidence: [
    { kind: 'check_run', evidence_id: 'ev_run', session_id: 's1', criteria: [1], name: 'pytest -q tests/test_totals.py', standing: 'missing', execution_recorded: null },
    { kind: 'finding', evidence_id: 'ev_find', session_id: 's1', criteria: [2], name: null, standing: 'dangling_reference', execution_recorded: null },
    { kind: 'observation', evidence_id: 'ev_obs', session_id: 's1', criteria: [], name: 'watched the totals', standing: 'permission_denied', execution_recorded: null },
  ],
  transcripts: [
    { session_id: 's1', file: HOME + '/.claude/projects/-w-checkout-web/s1.jsonl', lines: 40, lines_gone: 1, lines_changed: 2, standing: 'content_changed' },
    { session_id: 's2', file: HOME + '/.claude/projects/-w-checkout-web/s2.jsonl', lines: 12, lines_gone: 0, lines_changed: 0, standing: 'readable' },
  ],
  totals: { records: 8, readable: 2, not_readable: 6 },
  masking: { state: 'ran', detector: 'v3', masked: 0, patterns: [], statement: 'detector v3 · nothing matched a known secret shape · best-effort, not a guarantee' },
}
const READ_OK = { status: 'ready', doc: RECORDS }

/** The screen, given its data and an io that captures what it saves and copies. */
function screen(props, state) {
  const saved = []
  const copied = []
  const io = { save: (f) => { saved.push(f) }, copy: (t) => { copied.push(t); return Promise.resolve() } }
  const inst = M.instance({ ...props, io }, state)
  return { inst, saved, copied, v: () => inst.renderVals() }
}
/** The file Download hands to the browser, in this state. */
function fileOf(props, state) {
  const s = screen(props, state)
  s.v().download()
  assert.equal(s.saved.length, 1, 'Download hands exactly one file to the browser')
  return s.saved[0]
}
const previewText = (v) => (v.preview || []).map((l) => (l.text === ' ' ? '' : l.text)).join('\n') + '\n'
const settle = () => new Promise((r) => setImmediate(r))
const MD_MARK = { paths: '\\[left out: a file or folder path\\]', words: '\\[left out: the agent’s own words\\]' }
const RAW_MARK = { paths: '[left out: a file or folder path]', words: '[left out: the agent’s own words]' }
const count = (text, needle) => text.split(needle).length - 1

test('a served exclusion marker and count reach all Whole Story files, including a redacted copy', () => {
  const marker = 'left out by your exclusion (`src/a.rs`)'
  const snapshot = {
    ...SNAP_T,
    leftOutByExclusion: 2,
    statements: SNAP_T.statements.map((st, i) => i === 0 ? { ...st, text: marker, plain: marker } : st),
  }
  const props = { data: wholeStoryFrom(snapshot, TRACKED), readable: READ_OK }
  for (const fmt of ['md', 'html', 'json']) {
    for (const words of [false, true]) {
      const file = fileOf(props, { fmt, words })
      assert.ok(file.text.includes('left out by your exclusion'), `${fmt}: marker remains`)
      assert.ok(file.text.includes('Left out by your exclusion: 2 entries'), `${fmt}: count remains`)
      if (fmt === 'json') assert.equal(JSON.parse(file.text).left_out_by_exclusion, 2)
    }
  }
})

/* ------------------------------------------------------------------ row 6 */

test('row 6: each format makes a real file of that format', () => {
  const props = { data: DATA_T(), readable: READ_OK }
  const md = fileOf(props, { fmt: 'md' })
  assert.match(md.filename, /\.md$/)
  assert.match(md.mime, /^text\/markdown/)
  assert.ok(md.text.startsWith('# Fix double-applied discount on checkout total\n'), md.text.slice(0, 80))
  assert.ok(md.text.includes('\n## What counts as done, and what proves it\n'))
  assert.ok(md.text.includes('\n## Readable now\n'))

  const html = fileOf(props, { fmt: 'html' })
  assert.match(html.filename, /\.html$/)
  assert.match(html.mime, /^text\/html/)
  assert.ok(html.text.startsWith('<!doctype html>'))
  const doc = new JSDOM(html.text).window.document
  assert.equal(doc.querySelector('h1').textContent, 'Fix double-applied discount on checkout total')
  assert.match(doc.querySelector('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'), /default-src 'none'/)
  assert.equal(doc.querySelectorAll('script, link, img, iframe').length, 0, 'the page runs nothing and fetches nothing')
  assert.ok([...doc.querySelectorAll('h2')].some((h) => h.textContent === 'What counts as done, and what proves it'))

  const json = fileOf(props, { fmt: 'json' })
  assert.match(json.filename, /\.json$/)
  assert.match(json.mime, /^application\/json/)
  const j = JSON.parse(json.text)
  assert.equal(j.format, 'thruline-packet')
  assert.equal(j.schema_version, 1)
  assert.equal(j.title, 'Fix double-applied discount on checkout total')
  assert.ok(Array.isArray(j.sections) && j.sections.some((s) => s.id === 'done'))
  assert.equal(j.redaction.redacted, false)
  assert.equal(j.redaction.derived_from_sha256, null)
})

test('row 6: picking a format changes the file, its name, the Download label and the note', () => {
  const s = screen({ data: DATA_T(), readable: READ_OK })
  const names = []
  for (const [i, ext] of [[1, 'html'], [2, 'json'], [0, 'md']]) {
    s.v().fmts[i].pick()
    const v = s.v()
    assert.equal(v.downloadLabel, 'Download .' + ext)
    assert.match(v.fileName, new RegExp('\\.' + ext + '$'))
    names.push(v.fileName)
  }
  assert.equal(new Set(names).size, 3)
})

/* ------------------------------------------------------------------ row 8 */

test('row 8: Download saves exactly the text the preview shows, in every format and copy', () => {
  for (const fmt of ['md', 'html', 'json']) {
    for (const state of [{}, { paths: true }, { words: true }, { paths: true, words: true }]) {
      const s = screen({ data: DATA_T(), readable: READ_OK }, { fmt, ...state })
      const v = s.v()
      v.download()
      assert.equal(s.saved.length, 1)
      assert.equal(s.saved[0].text, previewText(v), fmt + ' ' + JSON.stringify(state))
      assert.equal(s.saved[0].filename, v.fileName)
    }
  }
})

test('row 8: the preview draws the file’s lines and nothing between them (no stray blank line in a pre-wrap box)', () => {
  for (const state of [{ fmt: 'md', paths: true }, { fmt: 'json' }]) {
    const v = screen({ data: DATA_T(), readable: READ_OK }, state).v()
    const region = new JSDOM(M.draw(v)).window.document.querySelector('[role="region"][aria-label="The file, word for word"]')
    assert.equal(region.textContent, v.preview.map((l) => l.text).join(''), 'the region holds the lines and nothing else')
    assert.equal(region.children.length, v.preview.length)
  }
})

test('row 8: Copy as Markdown copies the Markdown file’s text, whichever format is picked', async () => {
  const mdFull = fileOf({ data: DATA_T(), readable: READ_OK }, { fmt: 'md' }).text
  const mdCut = fileOf({ data: DATA_T(), readable: READ_OK }, { fmt: 'md', words: true }).text
  for (const fmt of ['html', 'json', 'md']) {
    const s = screen({ data: DATA_T(), readable: READ_OK }, { fmt })
    s.v().copyMd()
    await settle()
    assert.deepEqual(s.copied, [mdFull], fmt)
    assert.match(s.v().statusText, /^Copied: the same text the Markdown file holds\./)
  }
  const s = screen({ data: DATA_T(), readable: READ_OK }, { fmt: 'json', words: true })
  s.v().copyMd()
  await settle()
  assert.deepEqual(s.copied, [mdCut], 'with a redacted copy on, it copies the redacted Markdown')
})

test('row 8: the controls are on, say what they did, and never say "Not available in this version"', async () => {
  const s = screen({ data: DATA_T(), readable: READ_OK })
  const html = M.draw(s.v())
  assert.ok(!words(html).includes('Not available in this version'))
  for (const label of ['Download .md', 'Copy as Markdown']) {
    const at = html.indexOf(label)
    const button = html.slice(html.lastIndexOf('<button', at), at)
    assert.ok(at > 0 && !button.includes('disabled=""'), label + ' is on')
  }
  const sw = html.slice(html.indexOf('role="switch"') - 40, html.indexOf('role="switch"') + 400)
  assert.ok(!sw.includes('disabled=""'), 'the redacted-copy switch is on')
  assert.equal(s.v().exportOff, false)
  s.v().download()
  assert.match(s.v().statusText, /^Your browser has the file now/)
  const failing = M.instance({ data: DATA_T(), readable: READ_OK, io: { save: () => { throw new Error('blocked') }, copy: () => Promise.reject(new Error('no')) } })
  failing.renderVals().download()
  assert.match(failing.renderVals().statusText, /would not save the file\. Nothing was saved and nothing was sent\./)
  failing.renderVals().copyMd()
  await settle()
  assert.match(failing.renderVals().statusText, /would not copy the text\. Nothing was copied, and nothing was sent\./)
})

test('file names: thruline-packet-<change>-<version7>[-redacted].<ext>', () => {
  for (const fmt of ['md', 'html', 'json']) {
    assert.equal(fileOf({ data: DATA_T(), readable: READ_OK }, { fmt }).filename, 'thruline-packet-fix-double-applied-discount-on-checkout-total-e71c5b0.' + fmt)
    assert.equal(fileOf({ data: DATA_T(), readable: READ_OK }, { fmt, paths: true }).filename, 'thruline-packet-fix-double-applied-discount-on-checkout-total-e71c5b0-redacted.' + fmt)
  }
  const long = { ...DATA_T(), version: { id: 'wt-0123456789abcdef', commit: null, unsaved: null } }
  assert.match(fileOf({ data: long, readable: READ_OK }).filename, /^thruline-packet-[a-z0-9-]+-[A-Za-z0-9_-]{7}\.md$/)
  const none = { ...DATA_T(), version: { id: null, commit: null, unsaved: null } }
  assert.match(fileOf({ data: none, readable: READ_OK }).filename, /-unversioned\.md$/)
})

/* ------------------------------------------------------------------ row 7 */

test('row 7: the redacted copy is a second file — labelled at the top, a marker per removal, derived_from_sha256 naming the full file', () => {
  for (const fmt of ['md', 'json', 'html']) {
    const props = { data: DATA_T(), readable: READ_OK }
    const full = fileOf(props, { fmt })
    const cut = fileOf(props, { fmt, paths: true })
    assert.notEqual(cut.filename, full.filename)
    assert.equal(fileOf(props, { fmt }).text, full.text, 'the full file is unchanged by the redacted copy')
    if (fmt === 'md') {
      const lines = cut.text.split('\n')
      assert.equal(lines[0], '> This copy leaves things out. Each one is marked where it was. The full copy is unchanged.')
      const n = count(cut.text, MD_MARK.paths)
      assert.ok(n >= 3, 'the root, the decision files and the scope are paths the record holds: ' + n)
      assert.equal(lines[1], '> - File and folder paths: ' + n + ' left out')
      assert.ok(lines.slice(0, 6).some((l) => l.includes('Left out: the paths the record holds as paths')))
      assert.ok(lines.slice(0, 8).some((l) => l.includes('derived_from_sha256') && l.includes(sha256Hex(full.text))), 'the label names the full file by its fingerprint')
      assert.ok(!cut.text.includes(ROOT), 'the project’s own folder is left out wherever it appears')
      assert.ok(!cut.text.includes('.thruline/decisions/'), 'decision files are left out')
      assert.ok(full.text.includes(ROOT) && full.text.includes('.thruline/decisions/e71c5b0.json'))
    }
    if (fmt === 'json') {
      const j = JSON.parse(cut.text)
      assert.equal(j.redaction.redacted, true)
      assert.equal(j.redaction.derived_from_sha256, sha256Hex(full.text))
      const n = count(cut.text, RAW_MARK.paths)
      assert.deepEqual(j.redaction.removed, [{ class: 'paths', count: n }])
      assert.ok(n > 0)
    }
    if (fmt === 'html') {
      const doc = new JSDOM(cut.text).window.document
      const label = doc.querySelector('main > section.label')
      assert.ok(label && label === doc.querySelector('main').firstElementChild, 'the label opens the page')
      assert.match(label.textContent, /File and folder paths: \d+ left out/)
      assert.ok(label.textContent.includes(sha256Hex(full.text)))
      assert.ok(doc.body.textContent.includes(RAW_MARK.paths))
    }
  }
})

test('row 7: each class switch changes what is removed, and each count is the removals actually made', () => {
  const props = { data: DATA_T(), readable: READ_OK }
  const full = fileOf(props, { fmt: 'md' }).text
  const quote = 'Tests pass.'
  assert.ok(full.includes(quote) && full.includes(ROOT))

  const w = screen(props, { fmt: 'md', words: true })
  const wv = w.v()
  wv.download()
  const wt = w.saved[0].text
  const nw = count(wt, MD_MARK.words)
  assert.ok(nw >= 2, 'both said quotes are the agent’s own words: ' + nw)
  assert.ok(!wt.includes(quote), 'the agent’s words are gone')
  assert.ok(wt.includes(ROOT), 'paths stay when only the words are left out')
  assert.equal(count(wt, MD_MARK.paths), 0)
  assert.ok(wt.split('\n')[1] === '> - The agent’s own words: ' + nw + ' left out')
  assert.equal(wv.classes[1].count, String(nw))

  const p = screen(props, { fmt: 'md', paths: true })
  const pv = p.v()
  pv.download()
  const pt = p.saved[0].text
  assert.ok(pt.includes(quote), 'the words stay when only paths are left out')
  assert.equal(pv.classes[0].count, String(count(pt, MD_MARK.paths)))

  const b = screen(props, { fmt: 'md', paths: true, words: true })
  const bv = b.v()
  bv.download()
  const bt = b.saved[0].text
  assert.equal(bv.classes[0].count, String(count(bt, MD_MARK.paths)))
  assert.equal(bv.classes[1].count, String(count(bt, MD_MARK.words)))
  assert.ok(bt.includes('> - File and folder paths: ' + count(bt, MD_MARK.paths) + ' left out'))
  assert.ok(bt.includes('> - The agent’s own words: ' + count(bt, MD_MARK.words) + ' left out'))

  /* the switches themselves */
  const t = screen(props)
  assert.equal(t.v().cutOn, 'false')
  t.v().toggleCut()
  assert.equal(t.v().cutOn, 'true')
  assert.match(t.v().fileName, /-redacted\.md$/)
  t.v().classes[1].toggle()
  assert.equal(t.v().classes[1].checked, 'true')
  t.v().toggleCut()
  assert.equal(t.v().cutOn, 'false')
  assert.equal(t.v().classes[0].checked, 'false')
  assert.equal(t.v().classes[1].checked, 'false')
  /* nothing of a check's printed output is on the page, so that class leaves out nothing and is off */
  assert.equal(t.v().classes[2].count, '0')
  assert.equal(t.v().classes[2].disabled, true)
})

/* ------------------------------------------------------------------ row 9 */

test('row 9: Readable now reads the route and draws each record, in its own words', () => {
  const s = screen({ data: DATA_T(), readable: READ_OK })
  const v = s.v()
  assert.notEqual(v.readableShow, false)
  const rows = v.readable.map((r) => r.what + ' | ' + r.word + ' | ' + r.detail)
  assert.deepEqual(v.readable.map((r) => r.word), ['content changed', 'readable', 'never written', 'missing', 'dangling reference', 'permission denied', 'content changed', 'readable'], rows.join('\n'))
  assert.ok(!v.readable.some((r) => r.detail && r.detail.includes(HOME)), 'no home folder on screen')
  assert.ok(v.readable[0].detail.includes('.thruline/decisions/9f2ad3e.json'))
  assert.equal(v.readable.filter((r) => r.glyph === '✓').length, 2)
  assert.equal(v.readable.filter((r) => r.glyph === '◌').length, 6)
  assert.equal(v.page.readableSum, '2 of the 8 records this rests on could be read.')
  assert.match(v.page.readableAt, /^looked at \d\d:\d\d$/)
  const html = words(M.draw(v))
  for (const w of ['missing', 'permission denied', 'content changed', 'dangling reference', 'never written']) assert.ok(html.includes(w), w)
  /* the file says the same, and says it in the place each record belongs to */
  const md = fileOf({ data: DATA_T(), readable: READ_OK }, { fmt: 'md' }).text
  assert.ok(md.includes('2 of the 8 records this rests on could be read.'))
  const dec = md.slice(md.indexOf('## Decision history'), md.indexOf('## What was not recorded'))
  assert.ok(dec.includes('Could not be read now: The approval, as a file · content changed'), dec)
  const done = md.slice(md.indexOf('## What counts as done'), md.indexOf('## What the agent said'))
  assert.ok(done.includes('Could not be read now: A check run · missing'), done)
  const said = md.slice(md.indexOf('## What the agent said'), md.indexOf('## Runs'))
  assert.ok(said.includes('Could not be read now: The recorded conversation · content changed'), said)
})

test('row 9: every one readable, still being looked at, and a read that failed', () => {
  const all = { ...RECORDS, decisions: [RECORDS.decisions[2]], evidence: [], transcripts: [RECORDS.transcripts[1]], totals: { records: 2, readable: 2, not_readable: 0 } }
  assert.equal(screen({ data: DATA_T(), readable: { status: 'ready', doc: all } }).v().page.readableSum, 'Every one of the 2 records this rests on could be read.')
  const reading = screen({ data: DATA_T(), readable: { status: 'reading' } }).v()
  assert.ok(!/could be read\./.test(reading.page.readableSum), reading.page.readableSum)
  assert.equal(reading.readable.length, 0)
  const failed = screen({ data: DATA_T(), readable: { status: 'failed', note: 'did not answer' } }).v()
  assert.match(failed.page.readableSum, /could not be checked.*unknown, not as readable/)
  assert.equal(failed.readable.length, 0)
  const f = fileOf({ data: DATA_T(), readable: { status: 'failed' } }, { fmt: 'md' }).text
  assert.ok(f.includes(failed.page.readableSum))
})

/* ---------------------------------------------------------- untracked work */

test('untracked work: the file is built from the snapshot alone, and Readable now says it applies to a tracked Change only', () => {
  const d = DATA_U()
  assert.equal(d.tracked, false)
  for (const readable of [undefined, { status: 'untracked' }]) {
    const s = screen({ data: d, readable })
    const v = s.v()
    assert.equal(v.readableShow, false)
    assert.equal(v.readable.length, 0)
    assert.match(v.page.readableSum, /tracked Change only/)
    assert.ok(!/Every one of the|could be read\./.test(v.page.readableSum))
    v.download()
    const md = s.saved[0].text
    assert.ok(md.startsWith('# ' + d.change.title + '\n'))
    assert.ok(md.includes(v.page.readableSum))
    assert.ok(!md.includes('Every one of the'))
    assert.ok(md.includes('Tests pass.'), 'what the agent said is in it, from the snapshot')
  }
})

/* ---------------------------------------------------------------- packet */

test('"packet" stays off the Plain page; in the file it is in the name and the technical part only', () => {
  for (const d of [DATA_T(), DATA_U()]) {
    for (const fmt of ['md', 'html', 'json']) {
      const s = screen({ data: d, readable: d.tracked ? READ_OK : undefined }, { fmt })
      const v = s.v()
      /* the page as drawn, without the file's own name and text (T-5-4 allows the word there) */
      const plain = words(M.draw({ ...v, fileName: '', preview: [] }))
      assert.ok(!/packet/i.test(plain), fmt + ': ' + plain.slice(Math.max(0, plain.search(/packet/i) - 60), plain.search(/packet/i) + 60))
      v.download()
      const text = s.saved[0].text
      if (fmt === 'md') {
        const at = text.indexOf('\n## Technical details\n')
        assert.ok(at > 0)
        assert.ok(!/packet/i.test(text.slice(0, at)), 'not in the plain part')
        assert.ok(text.slice(at).includes('thruline-packet'))
      }
      if (fmt === 'json') {
        const j = JSON.parse(text)
        const rest = JSON.stringify({ ...j, format: null, technical: null })
        assert.ok(!/packet/i.test(rest), 'only `format` and `technical` carry it')
      }
    }
  }
})

/* ------------------------------------------------------------ one source */

test('one source: the file’s counts and sentences are the page’s own', () => {
  const props = { data: DATA_T(), readable: READ_OK }
  const s = screen(props, { fmt: 'json' })
  const v = s.v()
  v.download()
  const j = JSON.parse(s.saved[0].text)
  const sec = (id) => j.sections.find((x) => x.id === id)
  assert.ok(j.cover.includes(v.page.restsOn))
  assert.equal(sec('done').meta, v.page.doneMeta)
  assert.equal(sec('said').meta, v.page.saidMeta)
  assert.equal(sec('findings').meta, v.page.findingsMeta)
  assert.equal(sec('not-recorded').meta, v.page.gapsMeta)
  assert.equal(sec('integrity').meta, v.page.integrityMeta)
  assert.equal(sec('readable').lede, v.page.readableSum)
  assert.equal(sec('runs').rows.length, v.runs.length)
  v.runs.forEach((r, i) => assert.ok(sec('runs').rows[i].includes(r.result) && sec('runs').rows[i].includes(r.step), sec('runs').rows[i]))
  assert.equal(sec('decisions').rows.length, v.page.dec.length)
  assert.equal(sec('not-recorded').rows.length, v.page.gaps.length)
  assert.deepEqual(sec('readable').rows.length, v.readable.length)
  assert.ok(j.footer.includes(v.page.footTimes))

  /* every number in the file's plain part is one the page draws */
  const md = fileOf(props, { fmt: 'md' }).text
  const body = md.slice(0, md.indexOf('\n## Technical details\n'))
  const page = words(M.draw({ ...v, fileName: '', preview: [] }))
  const missing = [...new Set(body.match(/\d+(?:[.,:]\d+)*/g))].filter((n) => !page.includes(n))
  assert.deepEqual(missing, [], 'numbers in the file the page does not show')
})

test('one source: the runs in the file are the runs the page lists, with the page’s note about the rest', () => {
  const runs = Array.from({ length: MAX_RUNS + 5 }, (_, i) => ({ id: 'r' + i, step: i + 1, kind: 'test', command: 'pytest -q', version: 'e71c5b0', status: 'finished', outcome: 'succeeded', passed: 100 + i, failed: 0, time: '10:00' }))
  const d = wholeStoryFrom({ ...S1, runs }, null)
  const s = screen({ data: d }, { fmt: 'json' })
  const v = s.v()
  v.download()
  const j = JSON.parse(s.saved[0].text)
  const rows = j.sections.find((x) => x.id === 'runs').rows
  assert.equal(rows.length, MAX_RUNS)
  assert.ok(!rows.some((r) => r.includes('100 passed')), 'not the runs the page leaves out')
  assert.ok(j.sections.find((x) => x.id === 'runs').notes.some((n) => n.includes('These are the last ' + MAX_RUNS + ' of ' + (MAX_RUNS + 5) + ' test runs.')))
})

/* ------------------------------------------------------------------ row 2 */

test('row 2: Plain / Technical — the toggle flips the face, and Technical draws the file’s technical part', () => {
  const s = screen({ data: DATA_T(), readable: READ_OK })
  const plain = s.v()
  assert.equal(plain.techPressed, 'false')
  assert.ok(!plain.techLines)
  assert.ok(!plain.readable.some((r) => r.tech))
  assert.ok(words(M.draw(plain)).includes('Face: Plain (switch to Technical)'))
  plain.toggleFace()
  const tech = s.v()
  assert.equal(tech.techPressed, 'true')
  assert.ok(words(M.draw(tech)).includes('Face: Technical (switch to Plain)'))
  tech.download()
  const file = s.saved[0]
  assert.ok(tech.techLines.includes('Format: thruline-packet 1'))
  assert.ok(tech.techLines.some((l) => l.includes(sha256Hex(file.text))), 'the fingerprint of this exact file')
  assert.ok(tech.techLines.includes('Change id: chg_1'))
  for (const line of tech.techLines.filter((l) => !/sha256|bytes/.test(l))) assert.ok(file.text.includes(line), 'the file’s own technical line: ' + line)
  assert.deepEqual(tech.readable.map((r) => r.tech.split(' · ').pop()), ['content_changed', 'readable', 'not_written', 'missing', 'dangling_reference', 'permission_denied', 'content_changed', 'readable'])
  const drawn = words(M.draw(tech))
  assert.ok(drawn.includes('Format: thruline-packet 1') && drawn.includes('content_changed'))
  tech.toggleFace()
  assert.equal(s.v().techPressed, 'false')
  /* the face lives in one place a preference can set */
  assert.equal(screen({ data: DATA_T(), readable: READ_OK, face: 'technical' }).v().techPressed, 'true')
})

test('the design gallery is drawn as before: no export handler, no technical lines', () => {
  const v = M.instance({}).renderVals()
  assert.equal(v.isDemo, true)
  assert.ok(!v.techLines)
  assert.equal(v.preview[0].text, '# Fix double-applied discount on checkout total')
})
