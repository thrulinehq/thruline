/* EXPORT-MD-ESCAPE-R4 (3) — the other places Markdown is written from recorded
 * text.
 *
 * The exported packet file (`packet-export.js`) was made safe by three rounds of
 * `escapeMarkdown`. Two more writers reach a person from the launch UI and wrote
 * recorded text RAW, and the R3 adversary named them as out of scope, with a
 * reproduction: a claim whose text is
 *
 *     Done <img src=x onerror=alert(1)> [click](javascript:alert(2)) <script>alert(3)</script>
 *
 * came out of `buildVerdict(S, {register:'manager'}).exportMarkdown()` verbatim,
 * live in all three renderers (an `<img onerror>`, an `<a href=javascript:>`, a
 * `<script>`), and the same text in a node's title reached the clipboard the same
 * way from the inspector's Copy Markdown button.
 *
 *  - `verdict-model.js` `exportMarkdown()`   — the session verdict's summary, from
 *    `verdict-panel.jsx` (`exportSummary`), on the Verdict tab;
 *  - `shell.jsx` `markdown()`                 — the inspector's Copy Markdown, wired
 *    from `docks.jsx` (the button, `onCopyMd`) and `hq.jsx` (`onCopyMarkdown`).
 *    There is one writer behind both buttons; `docks.jsx` writes no Markdown.
 *
 * Both are reachable from `ui/src/main.jsx` (its `Workspace` is `shell.jsx`'s
 * default export, which imports `docks.jsx`, `hq.jsx` and `verdict-model.js`).
 *
 * The rule is `packet-export.js`'s, applied by the same helper: recorded text is
 * written as the words it is and cannot be read as markup, a link, an image or a
 * heading by any renderer. These tests hold both writers to the invariant the
 * packet export is held to (D-246), through the three real vendored renderers.
 *
 * Every string here is synthetic.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

import * as MI from '../../test-support/markdown-instrument.mjs'
import { buildVerdict } from './verdict-model.js'
import * as PE from './packet-export.js'
import { enumerateInspector, inspectorMarkdown, INSPECTOR_FIELDS } from '../../test-support/inspector-enumerate.mjs'

/* The adversary's payload, verbatim, and the shapes that broke the packet export in rounds 1 to 4. */
const OOS = 'Done <img src=x onerror=alert(1)> [click](javascript:alert(2)) <script>alert(3)</script>'
const PAYLOADS = {
  adversary_out_of_scope: OOS,
  image_in_a_label: '[![a\\](https://127.0.0.1:1/p.png)](https://127.0.0.1:1/x)',
  plain_link: 'see [docs](https://ok.example/guide)',
  bare_host: 'see //evil.example/x.png',
  angle_email: 'mail <1@b.co> now',
  ftp_and_email: 'ftp://h.co/x and a@b.co',
  forged_structure: 'done\n# Forged heading\n- forged item\n> forged quote',
  definition: '[a]: javascript:alert(1)',
  span_and_backslash: '_a `<img src=x onerror=alert(1)>_\\`',
  round_two_link: '[[a `]x](javascript:alert(1))`',
  origin_at_first: '@e.co._x',
  origin_at_underscore: '@_e.co',
}

/* Text with no backslash, backtick or emphasis mark in it is shown exactly as recorded. */
const SHOWN_AS_RECORDED = ['adversary_out_of_scope', 'plain_link', 'bare_host', 'angle_email', 'ftp_and_email', 'definition']

/* The verdict summary's own structure has `###` headings, one per card; the packet export's has none. */
const VERDICT_OWN_TAGS = { tags: ['h3'] }

/* The levels of the headings a file has, in order: what the file's structure is, whatever words its headings hold (the inspector's first heading is the recorded title itself). */
const headings = (md) => md.split('\n').filter((line) => /^#{1,6}(?: |$)/.test(line)).map((line) => /^#+/.exec(line)[0])

/** What a person sees of a file, in each renderer. */
const seenIn = (md) => MI.RENDERERS.map((r) => [r.name, MI.textOf(r.render(md))])

/* ── the session verdict's summary ────────────────────────────────────── */

function claim(i, over = {}) {
  return { id: `c${i}`, node: i === 1 ? 'clm' : `clm${i}`, kind: 'task_complete', text: `Thing ${i} is done.`, verdict: 'unsupported', verifier: 'tests_pass@1', evidence: [], reasons: [], findings: [], ...over }
}

const ORDINARY_TEXT = 'the totals are right'

/** A session whose every field the verdict summary writes recorded text from holds `text`. */
function verdictSession(text) {
  /* The receipt chips carry node ids, which are the engine's; in a hostile export they are as hostile as anything else, so they hold the text too (the ordinary session keeps `clm`). */
  const nodeOf = (ordinary) => (text === ORDINARY_TEXT ? ordinary : text)
  return {
    key: 'k',
    harness: text,
    finalTree: 'T2',
    capture: { degraded: false, condition: 'intact' },
    events: [
      { seq: 1, kind: 'user_prompt', text },
      { seq: 5, kind: 'exec', text },
      { seq: 12, kind: 'snapshot', text: 'tree T2' },
    ],
    claims: [
      claim(1, { text, reasons: [text], node: nodeOf('clm'), findings: [{ key: 'test_run_failed', severity: 'fail', detail: text, path: text, node: nodeOf('f1') }] }),
      claim(2, { text, verdict: 'supported', evidence: ['k1'], node: nodeOf('clm2') }),
      claim(3, { text, verdict: 'unverifiable' }),
    ],
    promises: [{ id: 'p1', node: nodeOf('prm'), kind: 'follow_up', text, status: 'open', followUp: text }],
    checks: [{ id: 'k1', node: nodeOf('chk'), exit: 0, inputTree: 'T2', command: text }],
    nodes: [],
  }
}

const BENIGN_VERDICT = buildVerdict(verdictSession(ORDINARY_TEXT), { register: 'manager' }).exportMarkdown()

test('R4 (3) the verdict summary: recorded text, of every kind, is written as the words it is — nothing live in any of the three renderers, and no heading or item the file did not write', () => {
  for (const register of ['manager', 'operator']) {
    for (const [name, text] of Object.entries(PAYLOADS)) {
      const md = buildVerdict(verdictSession(text), { register }).exportMarkdown()
      assert.deepEqual(MI.hazardsRendered(md, VERDICT_OWN_TAGS), [], `${register}, ${name}: live in a renderer`)
      assert.ok(!md.includes('<'), `${register}, ${name}: a raw "<" is in the file`)
      assert.ok(!/(^|\n)- \[[ xX]\]/.test(md), `${register}, ${name}: a task-list box`)
      assert.deepEqual(headings(md), headings(BENIGN_VERDICT), `${register}, ${name}: the file's headings changed`)
      assert.deepEqual(MI.definitionsIn(md), [], `${register}, ${name}: a definition formed`)
    }
  }
})

test('R4 (3) the verdict summary: the adversary\'s claim text is in the file as text — the same characters, in every renderer — and is what the helper writes', () => {
  const md = buildVerdict(verdictSession(OOS), { register: 'manager' }).exportMarkdown()
  assert.ok(md.includes(PE.escapeMarkdown(OOS)), 'the claim is not written by escapeMarkdown, the helper the packet export uses')
  for (const [name, text] of seenIn(md)) assert.ok(text.includes(OOS), `${name}: the claim's characters are not what a reader sees`)
  for (const key of SHOWN_AS_RECORDED) {
    const shown = buildVerdict(verdictSession(PAYLOADS[key]), { register: 'manager' }).exportMarkdown()
    for (const [name, text] of seenIn(shown)) assert.ok(text.includes(PAYLOADS[key]), `${name}, ${key}: ${JSON.stringify(PAYLOADS[key])} is not shown as recorded`)
  }
})

test('R4 (3) the verdict summary: what the file wrote itself is unchanged — an ordinary session\'s summary has the same headings, counts and receipts', () => {
  assert.match(BENIGN_VERDICT, /^# ThruLine verdict — /)
  assert.match(BENIGN_VERDICT, /\[clm\]/, 'the receipt chips are the file\'s own and are still there')
  assert.ok(headings(BENIGN_VERDICT).length >= 3)
  assert.deepEqual(MI.hazardsRendered(BENIGN_VERDICT, VERDICT_OWN_TAGS), [])
})

/* ── the inspector's Copy Markdown ────────────────────────────────────── */

const temporary = await mkdtemp(join(tmpdir(), 'thruline-markdown-surfaces-'))
after(() => rm(temporary, { recursive: true, force: true }))
const bundle = await build({
  entryPoints: [fileURLToPath(new URL('./shell.jsx', import.meta.url))],
  bundle: true, write: false, format: 'esm', platform: 'node',
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
})
const compiled = join(temporary, 'workspace.mjs')
await writeFile(compiled, bundle.outputFiles[0].text)
const { Workspace } = await import(pathToFileURL(compiled).href)

/** A workspace with one claim selected, every field the inspector's Markdown reads holding `text`. */
function workspace(text) {
  const session = {
    id: 'session', key: 'session', capture: {}, narr: {}, ended: '2026-09-08T22:59:00Z', finalTree: 'tree-final', origin: text,
    events: [{ seq: 10, kind: 'session_start', text: 'Started' }],
    claims: [{ id: text, node: 'clm2', text, plain: text, kind: 'tests_pass', word: 'not_proven', verdict: 'unsupported', eventId: 'ev-two', scope: `file:${text}`, evidence: ['check-two'], findings: [], reasons: [text], warnings: [text], nextCheck: text }],
    checks: [{ id: 'check-two', node: 'chk2', command: text, category: 'test', runner: text, exit: 1, inputTree: 'tree-old' }],
    promises: [], asks: [],
  }
  session.nodes = [{ id: 'clm2', objId: 'claim-two', objType: 'claim', title: text }, { id: 'chk2', objId: 'check-two', objType: 'check', title: text }]
  const w = Object.create(Workspace.prototype)
  w.props = { commentary: false }
  w.state = { session: 'session', sel: 'clm2', rep: 'review', doc: 'trace', drawer: 'claims', panels: { right: false, bottom: true }, expanded: {}, docs: [], compare: { a: 'session', b: 'session' }, corpus: { order: ['session'], sessions: { session } } }
  w.S = () => session
  w.reg = () => 'mgr'
  w.nodeById = (id) => session.nodes.find((node) => node.id === id)
  w.dbPath = () => '/record.sqlite'
  return w
}

const BENIGN_INSPECTOR = workspace('the totals are right').markdown()
assert.deepEqual(MI.unescapedMarkup(BENIGN_INSPECTOR), [], 'the fixture: an ordinary inspector file has no bracket of its own')

test('R5 an @-first recorded origin cannot compose with the footer underscore into a mail link, through production Workspace.markdown and every real renderer', () => {
  for (const origin of [PAYLOADS.origin_at_first, PAYLOADS.origin_at_underscore]) {
    const md = inspectorMarkdown(Workspace, 'origin', origin)
    const findings = MI.RENDERERS.map((r) => [r.name, MI.liveIn(r.render(md))])
    assert.deepEqual(findings, MI.RENDERERS.map((r) => [r.name, []]), origin)
  }
})

test('R5 every literal before recorded text is checked, including the wordless type label and all eight inspector fields', () => {
  for (const field of INSPECTOR_FIELDS) {
    for (const wordSlug of ['not_proven', 'proven', null]) {
      for (const text of [PAYLOADS.origin_at_first, PAYLOADS.origin_at_underscore, '@e.co', ...SHOWN_AS_RECORDED.map((key) => PAYLOADS[key])]) {
        const md = inspectorMarkdown(Workspace, field, text, wordSlug)
        assert.deepEqual(MI.hazardsRendered(md), [], `${field}, ${wordSlug}, ${JSON.stringify(text)}`)
      }
    }
  }
})

test('R5 visible characters stay recorded in every inspector field, including a leading @ and the preceding literals', () => {
  for (const field of INSPECTOR_FIELDS) {
    for (const text of ['@e.co', 'the totals are right', ...SHOWN_AS_RECORDED.map((key) => PAYLOADS[key])]) {
      const md = inspectorMarkdown(Workspace, field, text)
      for (const [name, shown] of seenIn(md)) assert.ok(shown.includes(text), `${field}, ${name}: ${JSON.stringify(text)}`)
    }
  }
})

/* The leading-origin atoms include the two reported breaks within N=5. Each
 * string is placed in each of the eight fields, so '# ', the word + ' · ',
 * ' · ', line start, '- ', ': ' and the footer '_' are all exercised. Larger
 * URL/markup-alphabet sweeps use this same instrument outside the bounded suite.
 * 6 + 6^2 + ... + 6^5 = 9,330 strings; 74,640 field cases; 223,920 renders. */
test('R5 bounded leading-literal enumeration: eight inspector fields, N=5, all three vendored renderers', { timeout: 60000 }, async (t) => {
  const symbols = ['@', '_', '.', 'e.co', 'x', ' ']
  const started = performance.now()
  const r = await enumerateInspector({ compiled, symbols, max: 5 })
  t.diagnostic(`${JSON.stringify(symbols)}, N=5, ${r.strings} strings, ${r.judged} field cases, ${r.renders} renders, ${r.workers} workers, ${Math.round(performance.now() - started)} ms`)
  assert.equal(r.strings, 9330)
  assert.deepEqual(r.fields, INSPECTOR_FIELDS)
  assert.equal(r.judged, 74640)
  assert.equal(r.renders, 223920)
  assert.equal(r.found, 0, JSON.stringify(r.samples))
  assert.ok(performance.now() - started < 60000, 'the committed enumeration exceeded 60 seconds')
})

test('R4 (3) the inspector\'s Copy Markdown: recorded text, in every field it writes, is written as the words it is — nothing live in any of the three renderers, and no heading or item the file did not write', () => {
  for (const [name, text] of Object.entries(PAYLOADS)) {
    const md = workspace(text).markdown()
    assert.deepEqual(MI.hazardsRendered(md), [], `${name}: live in a renderer`)
    assert.ok(!md.includes('<'), `${name}: a raw "<" is in the file`)
    assert.deepEqual(headings(md), headings(BENIGN_INSPECTOR), `${name}: the file's headings changed`)
    assert.deepEqual(MI.definitionsIn(md), [], `${name}: a definition formed`)
    assert.deepEqual(MI.unescapedMarkup(md), [], `${name}: a live bracket or "<" (the file writes none of its own)`)
  }
})

test('R4 (3) the inspector\'s Copy Markdown: the adversary\'s claim text is in the file as text, the same characters in every renderer, and is what the helper writes', () => {
  const md = workspace(OOS).markdown()
  assert.ok(md.includes(PE.escapeMarkdown(OOS)), 'the text is not written by escapeMarkdown, the helper the packet export uses')
  for (const [name, text] of seenIn(md)) assert.ok(text.includes(OOS), `${name}: the claim's characters are not what a reader sees`)
  for (const key of SHOWN_AS_RECORDED) {
    for (const [name, text] of seenIn(workspace(PAYLOADS[key]).markdown())) assert.ok(text.includes(PAYLOADS[key]), `${name}, ${key}: ${JSON.stringify(PAYLOADS[key])} is not shown as recorded`)
  }
  /* What the file wrote itself is still there. */
  assert.match(BENIGN_INSPECTOR, /^# “the totals are right”\n/)
  assert.match(BENIGN_INSPECTOR, /## Record/)
  assert.match(BENIGN_INSPECTOR, /copying does not execute\.\_$/)
})

/* ── one helper, and where each surface is reached from ───────────────── */

const read = (name) => readFileSync(new URL(name, import.meta.url), 'utf8')
const IMPORTS_THE_HELPER = /import\s*\{[^}]*\bescapeMarkdown\b[^}]*\}\s*from\s*'\.\/packet-export\.js'/

test('R4 (3) one helper: verdict-model.js and shell.jsx import escapeMarkdown from packet-export.js, and docks.jsx and hq.jsx write no Markdown of their own', () => {
  assert.ok(IMPORTS_THE_HELPER.test(read('./verdict-model.js')), 'verdict-model.js does not import the helper')
  assert.ok(IMPORTS_THE_HELPER.test(read('./shell.jsx')), 'shell.jsx does not import the helper')
  /* The Copy Markdown button is `docks.jsx`'s, and it calls the shell's writer: there is nothing in the dock to escape. */
  const docks = read('./docks.jsx')
  assert.match(docks, /onClick=\{h\.onCopyMd\}[^>]*>\s*Copy Markdown/)
  assert.match(read('./shell.jsx'), /onCopyMd: \(\) => this\.copy\(this\.markdown\(\), 'Markdown'\)/)
  assert.match(read('./shell.jsx'), /onCopyMarkdown: inspOn\.onCopyMd/)
  for (const file of ['./docks.jsx', './hq.jsx']) assert.ok(!/join\('\\n'\)|`#|'# '|'## '/.test(read(file).replace(/\/\*[\s\S]*?\*\//g, '')), `${file} builds Markdown text of its own`)
  /* The verdict panel's export button calls the model's writer. */
  assert.match(read('./verdict-panel.jsx'), /const md = V\.exportMarkdown\(\)/)
})
