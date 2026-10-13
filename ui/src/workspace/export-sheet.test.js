/* T-5-4 — the whole story, as a file: the door, the sheet, and the one act.
 *
 * Card §B item 1 in a DOM: the sheet is opened from the door on the real
 * packet page, each format's preview is read out of the `<pre>` a person
 * reads, Save is clicked, and the bytes handed to the browser are compared
 * with the bytes shown — `sha256(preview) == sha256(file)` — for all three
 * formats and a redacted copy. The same comparison against headless Chrome's
 * real download is `docs/loop/export/export-walk.mjs`; this is the in-process
 * half, and it runs with the suite.
 *
 * Plus: the one-Markdown rule (Copy as Markdown copies exactly the sheet's
 * Markdown), one primary per screen, no write address, every word from the
 * copy file, and the Plain face free of the engine's word for the file.
 *
 * Every fixture is synthetic. No string here came off a real machine.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM, VirtualConsole } from 'jsdom'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-export-sheet-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { createRoot } from 'react-dom/client';
  import { flushSync } from 'react-dom';
  import { PacketPage, copiedMarkdown } from './packet.jsx';
  import { ExportSheet, CHANGE_EXPORT } from './export-sheet.jsx';
  import { renderExport } from './packet-export.js';
  import { deriveChangeModel } from './change-model.js';
  import { buildPacketView } from './packet-view.js';
  import { HQ_COPY } from './hq-copy.js';
  export { React, createRoot, flushSync, PacketPage, ExportSheet, CHANGE_EXPORT, renderExport, deriveChangeModel, buildPacketView, copiedMarkdown, HQ_COPY };
  export const html = (el) => renderToStaticMarkup(el);
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
const M = await load('sheet.mjs')
/* A second bundle over a PROBE copy: equality with the real copy cannot tell a
 * string that came from the copy file from a literal that happens to match. */
const COPY_FILE = join(here, 'hq-copy.js')
const PROBED = await load('sheet-probe.mjs', [{
  name: 'probe-copy',
  setup(b) {
    b.onLoad({ filter: /hq-copy\.js$/ }, () => ({
      contents: readFileSync(COPY_FILE, 'utf8')
        .replace("heading: 'The whole story, as a file',", "heading: 'probe-export-heading',")
        .replace("save: 'Save this file',", "save: 'probe-export-save',")
        .replace("door: 'Save the whole story as a file',", "door: 'probe-export-door',")
        .replace("unreadable_label: 'Could not be read now',", "unreadable_label: 'probe-unreadable-label',"),
      loader: 'js',
    }))
  },
}])

const X = M.HQ_COPY.change.export

/* ── the fixture: a Change with two criteria, a run, a finding, an approval
 *    and its withdrawal ────────────────────────────────────────────────── */

const CHANGE = {
  id: 'ch1',
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Totals add up on the checkout page', purpose: 'The totals are wrong.', requester: 'sam', source: 'person' },
  implementation: { version: 1, snapshot_id: 'e71c5b0aaaa', base_revision: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1,
    criteria: ['The totals add up', 'Nothing else on the page breaks'],
    exclusions: [], allowed_scope: ['checkout/'], constraints: [], check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z', rationale: 'yes' },
  },
  associations: [{ id: 'a1', session_id: 's1', provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-09-22T09:30:00Z', reason: null }],
  examination: 'none_yet',
  decision: 'none_yet',
  decisions: {
    target_revision: 'e71c5b0aaaa',
    standing: null,
    history: [
      { act: 'approved', target_revision: 'e71c5b0aaaa', actor: 'sam', decided_at: '2026-09-22T11:00:00Z', scope_version: 1 },
      { act: 'withdrawn', target_revision: 'e71c5b0aaaa', actor: 'sam', decided_at: '2026-09-22T11:30:00Z', scope_version: 1 },
    ],
  },
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
}
const COVERAGE = {
  scope_version: 1, link_rows: 1, execution_sessions: [],
  criteria: [
    { index: 1, text: 'The totals add up', state: 'evidence', items: [{ link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run', name: 'cargo test', observed: 'not_proven', bears: true, applies: true, applies_slug: 'inputs_applies', failure_line: 'left: 17, right: 18' }] },
    { index: 2, text: 'Nothing else on the page breaks', state: 'none', items: [] },
  ],
  candidates: [],
}
/** One finding card in the shape `verdict-model.buildVerdict` produces. */
const CARD = {
  id: 'clm_1',
  state: 'not_proven',
  kind: 'said_done_no_proof',
  seq: 44,
  said: { parts: [{ text: 'The agent said this was done at step 44: ' }, { quote: 'all tests pass', speaker: 'agent' }], text: 'The agent said this was done at step 44: "all tests pass"', refs: ['r-44'] },
  record: { text: 'Not proven: nothing in the record shows this was done.', role: 'record', refs: ['r-44'] },
  do: { text: 'Ask the agent for proof: the output of a check, not a sentence.', role: 'do', refs: [], prompt: 'Run the check and paste the full output.' },
  detector: null,
  proofNode: null,
}
const WORK = [{ id: 's1', findings: [CARD], steps: 12 }]
const RECORDS = {
  schema_version: 1, change_id: 'ch1', checked_at: '2026-09-23T12:00:00Z',
  decisions: [
    { decision_id: 'cd_1', act: 'approved', version: 'e71c5b0aaaa', path: '/Users/synthetic/shop/.thruline/decisions/e71c5b0aaaa.json', standing: 'permission_denied' },
    { decision_id: 'cd_2', act: 'withdrawn', version: 'e71c5b0aaaa', path: null, standing: null },
  ],
  evidence: [{ kind: 'check_run', evidence_id: 'e1', session_id: 's1', criteria: [1], name: 'cargo test', standing: 'readable', execution_recorded: true }],
  transcripts: [{ session_id: 's1', file: '/Users/synthetic/.claude/projects/shop/s1.jsonl', lines: 12, lines_gone: 0, lines_changed: 0, standing: 'readable' }],
  totals: { records: 3, readable: 2, not_readable: 1 },
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
}

function modelOf(L = M) {
  const hq = L.deriveChangeModel(CHANGE, COVERAGE, { items: [] }, null, WORK)
  return { ...L.buildPacketView({ change: CHANGE, proposals: null, work: WORK, coverage: COVERAGE, evidence: [], hq }), hq }
}

const decode = (s) => s
  .replace(/&quot;/g, '"').replace(/&#x27;/g, "'").replace(/&#39;/g, "'")
  .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const sha = (s) => createHash('sha256').update(s, 'utf8').digest('hex')

/** The Plain face of a rendered sheet: its words, without the file preview,
 * the rendered frame, and the technical details one click down. */
function plainFace(html) {
  return decode(html
    .replace(/<pre[\s\S]*?<\/pre>/g, '')
    .replace(/<iframe[^>]*>(<\/iframe>)?/g, '')
    .replace(/<details[\s\S]*?<\/details>/g, '')
    .replace(/<[^>]+>/g, ' '))
}

/* ── the door ─────────────────────────────────────────────────────────── */

test('the door is on the whole-story page: secondary, in Sam\'s words, and the page keeps exactly one primary', () => {
  const html = M.html(M.React.createElement(M.PacketPage, { model: modelOf(), notes: [], onCopy: () => {}, records: RECORDS }))
  assert.match(html, /<button type="button" class="ws-change-secondary ws-export-open">Save the whole story as a file<\/button>/)
  assert.ok(decode(html).includes(X.door_note))
  assert.equal([...html.matchAll(/class="ws-(?:change|verdict)-primary[^"]*"/g)].length, 1, 'the page has more than one primary')
  assert.ok(!decode(html).includes(M.HQ_COPY.change.packet.send_blocked.reason), 'the old "cannot do this yet" still stands')
})

test('the sheet is its own screen, with one primary — Save — after the preview, and a way back', () => {
  const html = M.html(M.React.createElement(M.PacketPage, { model: modelOf(), notes: [], onCopy: () => {}, records: RECORDS, sheetOpen: true }))
  const primaries = [...html.matchAll(/class="ws-(?:change|verdict)-primary[^"]*"[^>]*>([^<]*)</g)].map((m) => m[1])
  assert.deepEqual(primaries, [X.save])
  assert.ok(html.indexOf('ws-export-preview') < html.indexOf('ws-export-save'), 'Save comes before the preview')
  assert.ok(html.includes('ws-export-close'))
  assert.ok(!html.includes('ws-packet-back'), 'the page\'s own primary is beside the sheet\'s')
  const face = plainFace(html)
  assert.ok(face.includes(X.lead), 'the sheet does not say where the file goes and that nothing is sent')
  assert.match(X.lead, /Nothing is sent anywhere/)
})

test('the preview IS the file: the <pre> and the sandboxed frame hold exactly renderExport\'s text, for every format and a redacted copy', () => {
  const model = modelOf()
  for (const [format, redact] of [['html', []], ['md', []], ['json', []], ['html', ['agent_words', 'paths', 'failure_output']]]) {
    const expected = M.renderExport(model.hq.packet, RECORDS, { format, redact })
    const html = M.html(M.React.createElement(M.ExportSheet, { packet: model.hq.packet, records: RECORDS, onClose: () => {}, initialFormat: format, initialLeaveOut: redact }))
    const pre = /<pre[^>]*>([\s\S]*?)<\/pre>/.exec(html)
    assert.ok(pre, 'no preview')
    assert.equal(decode(pre[1]), expected.text, `${format}${redact.length ? ' redacted' : ''}: the preview is not the file`)
    if (format === 'html') {
      const frame = /<iframe[^>]*sandbox=""[^>]*srcDoc="([^"]*)"/i.exec(html) || /<iframe[^>]*srcDoc="([^"]*)"[^>]*sandbox=""/i.exec(html)
      assert.ok(frame, 'the rendered page is not in a sandbox with no permissions')
      assert.equal(decode(frame[1]), expected.text, 'the rendered page is not the file')
    }
    assert.ok(html.includes(expected.filename), 'the file name is not stated')
    assert.ok(html.includes(sha(expected.text)), 'the fingerprint is not the text\'s')
  }
})

/* ── the act, in a DOM ────────────────────────────────────────────────── */

const GLOBALS = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'MouseEvent', 'KeyboardEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'fetch']
const settle = () => new Promise((r) => setTimeout(r, 5))

async function mounted(run, { failSave = false } = {}) {
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/', pretendToBeVisual: true, virtualConsole: new VirtualConsole() })
  const w = dom.window
  const held = GLOBALS.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of GLOBALS) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  /* Nothing on this screen may fetch while a file is made; any call is recorded. */
  const fetched = []
  set('fetch', async (...args) => { fetched.push(args); throw new Error('no fetch expected') })
  /* The browser's download, observed: the Blob it was handed. */
  const blobs = []
  const heldUrl = { create: URL.createObjectURL, revoke: URL.revokeObjectURL }
  URL.createObjectURL = (blob) => { if (failSave) throw new Error('this browser will not save'); blobs.push(blob); return 'blob:thruline-test/' + blobs.length }
  URL.revokeObjectURL = () => {}
  let copied = null
  Object.defineProperty(w.navigator, 'clipboard', { value: { writeText: async (t) => { copied = t } }, configurable: true })
  const root = M.createRoot(w.document.getElementById('root'))
  try {
    M.flushSync(() => root.render(M.React.createElement(M.PacketPage, { model: modelOf(), notes: [], onCopy: () => {}, records: RECORDS })))
    await settle()
    return await run({ w, d: w.document, blobs, fetched, copied: () => copied })
  } finally {
    root.unmount()
    URL.createObjectURL = heldUrl.create
    URL.revokeObjectURL = heldUrl.revoke
    for (const [n, desc] of held) {
      if (desc) Object.defineProperty(globalThis, n, desc)
      else delete globalThis[n]
    }
    w.close()
  }
}

const click = async (el) => { el.click(); for (let i = 0; i < 3; i += 1) await settle() }

test('(B.1, in a DOM) door → sheet; for each format and a redacted copy, the bytes saved are the bytes previewed; nothing is fetched', async () => {
  await mounted(async ({ d, blobs, fetched }) => {
    await click(d.querySelector('.ws-export-open'))
    assert.ok(d.querySelector('.ws-export-sheet'), 'the door did not open the sheet')
    assert.equal(d.activeElement && d.activeElement.id, 'ws-export-heading', 'focus did not move to the sheet')
    const radio = (f) => d.querySelector(`input[name="ws-export-format"][value="${f}"]`)
    const pairs = []
    for (const [format, leaveOut] of [['md', []], ['json', []], ['html', []], ['html', ['agent_words']]]) {
      await click(radio(format))
      for (const c of ['agent_words', 'paths', 'failure_output']) {
        const box = d.querySelector(`input[name="ws-export-leave-out-${c}"]`)
        if (box.checked !== leaveOut.includes(c)) await click(box)
      }
      const pre = d.querySelector('pre.ws-export-preview')
      const shown = pre.textContent
      await click(d.querySelector('.ws-export-save'))
      const blob = blobs[blobs.length - 1]
      const saved = await blob.text()
      pairs.push([format + (leaveOut.length ? '-redacted' : ''), sha(shown), sha(saved)])
      assert.equal(sha(saved), sha(shown), `${format}: the file is not the preview`)
      assert.equal(pre.getAttribute('data-export-filename'), d.querySelector('.ws-export-filename').textContent)
      if (leaveOut.length) assert.match(pre.getAttribute('data-export-filename'), /-redacted\./)
      const status = d.querySelector('[role="status"]')
      assert.ok(status && status.textContent === X.saved, 'the save is not reported in the copy file\'s words')
    }
    assert.equal(pairs.length, 4)
    assert.deepEqual(fetched, [], 'making a file fetched something')
    /* The way back returns focus to the door. */
    await click(d.querySelector('.ws-export-close'))
    assert.ok(d.querySelector('.ws-export-open'), 'the way back does not return to the page')
    assert.ok(d.activeElement && d.activeElement.classList.contains('ws-export-open'), 'focus did not return to the door')
  })
})

test('(one Markdown) Copy as Markdown copies exactly the text the sheet previews and saves as Markdown', async () => {
  await mounted(async ({ d, copied }) => {
    const copyButton = [...d.querySelectorAll('button')].find((b) => b.textContent === M.HQ_COPY.change.packet.copy.label)
    await click(copyButton)
    const copiedText = copied()
    assert.ok(copiedText && copiedText.length > 100, 'nothing was copied')
    await click(d.querySelector('.ws-export-open'))
    await click(d.querySelector('input[name="ws-export-format"][value="md"]'))
    assert.equal(d.querySelector('pre.ws-export-preview').textContent, copiedText, 'two Markdown texts for one Change')
  })
  const model = modelOf()
  assert.equal(M.copiedMarkdown(model.hq, RECORDS), M.renderExport(model.hq.packet, RECORDS, { format: 'md' }).text)
})

/* STEP-7-LIMITS-R2 — the file that leaves the tool says what ThruLine has and
 * has not looked at, in every format and in Copy as Markdown, from hq.packet:
 * the same derivation the Change screen prints (hq.codeLimit), not a second one. */
test('STEP-7-LIMITS-R2 · every export format and Copy as Markdown carry the limit sentence, from hq.packet', () => {
  const model = modelOf()
  const L = M.HQ_COPY.change.model.code_limit
  const limit = model.hq.version && model.hq.version.short ? L.named.replace('{version}', model.hq.version.short) : L.unnamed
  assert.deepEqual(model.hq.packet.codeLines, model.hq.codeLimit.lines, 'the packet does not carry the screen\'s derivation')
  assert.ok(model.hq.packet.codeLines.includes(limit), 'the packet has no limit sentence')
  const md = M.renderExport(model.hq.packet, RECORDS, { format: 'md' }).text
  const html = decode(M.renderExport(model.hq.packet, RECORDS, { format: 'html' }).text.replace(/<[^>]*>/g, ' ')).replace(/\s+/g, ' ')
  const json = JSON.parse(M.renderExport(model.hq.packet, RECORDS, { format: 'json' }).text)
  assert.ok(md.includes(limit), 'the Markdown file does not say the limit')
  assert.ok(html.includes(limit), 'the HTML file does not say the limit')
  assert.ok(json.code && Array.isArray(json.code.lines) && json.code.lines.includes(limit), 'the JSON file does not say the limit')
  assert.ok(M.copiedMarkdown(model.hq, RECORDS).includes(limit), 'Copy as Markdown does not say the limit')
  /* The export reads the packet's field, not the copy file: a probe line in
   * the packet reaches every format. */
  const probed = { ...model.hq.packet, codeLines: ['probe-code-limit-line'] }
  for (const format of ['md', 'html', 'json']) assert.ok(M.renderExport(probed, RECORDS, { format }).text.includes('probe-code-limit-line'), `${format}: not from hq.packet`)
})

/* ── words ────────────────────────────────────────────────────────────── */

const PLAIN_BANNED = [/\bevents?\b/i, /\breceipts?\b/i, /\bcandidates?\b/i, /\bapplicability\b/i, /\bdispositions?\b/i, /\bpacket\b/i, /\bledger\b/i, /\bsession id\b/i, /\btree hash\b/i, /\bpredicates?\b/i]

test('(B.7) the sheet\'s Plain face carries no banned word; the file\'s name — which does — is one click down', () => {
  const model = modelOf()
  for (const format of ['html', 'md', 'json']) {
    const html = M.html(M.React.createElement(M.ExportSheet, { packet: model.hq.packet, records: RECORDS, onClose: () => {}, initialFormat: format }))
    const face = plainFace(html)
    for (const re of PLAIN_BANNED) assert.doesNotMatch(face, re, `${format}: the sheet says ${re}`)
    const details = /<details[\s\S]*?<\/details>/.exec(html)[0]
    assert.ok(details.includes('thruline-packet-'), 'the file name is not in the technical details')
    assert.doesNotMatch(html, /<details[^>]*\bopen\b/, 'the technical details are open by default')
  }
})

test('(B.7) every word on the sheet and the door comes from the copy file (probe)', () => {
  const model = modelOf(PROBED)
  const page = PROBED.html(PROBED.React.createElement(PROBED.PacketPage, { model, notes: [], onCopy: () => {}, records: RECORDS }))
  assert.ok(page.includes('probe-export-door'), 'the door label is not the copy file\'s')
  const sheet = PROBED.html(PROBED.React.createElement(PROBED.ExportSheet, { packet: model.hq.packet, records: RECORDS, onClose: () => {}, initialFormat: 'md' }))
  assert.ok(sheet.includes('probe-export-heading') && sheet.includes('probe-export-save'))
  assert.ok(sheet.includes('probe-unreadable-label'), 'the file\'s own words are not the copy file\'s')
  /* And no JSX text in the source is a word of its own. */
  for (const f of ['export-sheet.jsx']) {
    const src = readFileSync(join(here, f), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/\{\/\*[\s\S]*?\*\/\}/g, '')
    const literals = [...src.matchAll(/>\s*([A-Za-z][^<>{}]*?)\s*</g)].map((m) => m[1]).filter((t) => /[a-z]{2,}/i.test(t))
    assert.deepEqual(literals, [], `${f} renders words of its own`)
  }
})

test('(B.7) no control on the sheet is disabled without saying why and what would enable it', () => {
  const model = modelOf()
  const html = M.html(M.React.createElement(M.ExportSheet, { packet: model.hq.packet, records: null, onClose: () => {} }))
  const offs = [...html.matchAll(/<(button|input)[^>]*\bdisabled\b[^>]*>/g)]
  for (const m of offs) assert.match(m[0], /aria-describedby="[^"]+"/, 'a disabled control with no reason')
})

/* ── no write address ─────────────────────────────────────────────────── */

test('no write: the sheet and the renderer reach one GET address and nothing else; the server is never asked to write', () => {
  assert.equal(M.CHANGE_EXPORT('ch 1'), '/api/changes/ch%201/export')
  for (const f of ['export-sheet.jsx', 'packet-export.js']) {
    const src = readFileSync(join(here, f), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
    for (const forbidden of ["method: 'POST'", 'method: "POST"', 'writeAction', 'XMLHttpRequest', 'sendBeacon', 'WebSocket', 'child_process', 'eval(']) {
      assert.ok(!src.includes(forbidden), `${f} can write: ${forbidden}`)
    }
    const addresses = [...src.matchAll(/'\/api\/[^']*'/g)].map((m) => m[0])
    assert.deepEqual(addresses, f === 'export-sheet.jsx' ? ["'/api/changes/'"] : [], `${f} reaches an address it does not declare`)
  }
  assert.ok(!/\bfetch\(/.test(readFileSync(join(here, 'packet-export.js'), 'utf8')), 'the renderer fetches')
})

/* ── T-5-4 fix round 1 ────────────────────────────────────────────────── */

/* Verifier finding 1: the saved line named the file, and the file's name
 * carries the engine's word for it. The static sweep above never pressed
 * Save, so it never met that line. This one presses it, in every state the
 * sheet can reach: each format × each leave-out choice, before Save, after
 * Save, and with a browser that refuses to save. */
const NEVER_WORDS = /\b(lied|lie|liar|hallucinated|caught|bullshit|bug-free|safe|all clear)\b|\d\s*%|\bpercent\b/i

/** Every word the sheet shows on its Plain face, read from the live DOM:
 * not the file preview, not the rendered frame, not the technical details. */
function livePlainFace(d) {
  const sheet = d.querySelector('.ws-export-sheet')
  const copy = sheet.cloneNode(true)
  for (const el of copy.querySelectorAll('pre, iframe, details')) el.remove()
  return copy.textContent.replace(/\s+/g, ' ').trim()
}

const SUBSETS = [[], ['agent_words'], ['paths'], ['failure_output'], ['agent_words', 'paths'], ['agent_words', 'failure_output'], ['paths', 'failure_output'], ['agent_words', 'paths', 'failure_output']]

async function setState(d, format, leaveOut) {
  await click(d.querySelector(`input[name="ws-export-format"][value="${format}"]`))
  for (const c of ['agent_words', 'paths', 'failure_output']) {
    const box = d.querySelector(`input[name="ws-export-leave-out-${c}"]`)
    if (box.checked !== leaveOut.includes(c)) await click(box)
  }
}

function sweep(face, where) {
  for (const re of PLAIN_BANNED) assert.doesNotMatch(face, re, `${where}: the sheet says ${re} — ${face}`)
  assert.doesNotMatch(face, NEVER_WORDS, `${where}: a never-word on the sheet — ${face}`)
}

test('(fix 1) every state the sheet reaches — each format, each leave-out choice, before and after Save, and a refused save — keeps its Plain face free of banned and never-words; the saved line names no file and no folder', async () => {
  let states = 0
  await mounted(async ({ d, blobs }) => {
    await click(d.querySelector('.ws-export-open'))
    sweep(livePlainFace(d), 'opened'); states += 1
    for (const format of ['html', 'md', 'json']) {
      for (const leaveOut of SUBSETS) {
        const where = `${format} [${leaveOut.join(',')}]`
        await setState(d, format, leaveOut)
        sweep(livePlainFace(d), where + ' before Save'); states += 1
        const before = blobs.length
        await click(d.querySelector('.ws-export-save'))
        assert.equal(blobs.length, before + 1, `${where}: Save handed the browser nothing`)
        const status = d.querySelector('.ws-export-sheet [role="status"]')
        assert.ok(status, `${where}: no saved line`)
        assert.equal(status.textContent, X.saved)
        const filename = d.querySelector('.ws-export-filename').textContent
        assert.ok(!status.textContent.includes(filename), `${where}: the saved line names the file`)
        assert.doesNotMatch(status.textContent, /Downloads\/|\/Users\/|~\//, `${where}: the saved line claims a folder`)
        sweep(livePlainFace(d), where + ' after Save'); states += 1
      }
    }
  })
  await mounted(async ({ d }) => {
    await click(d.querySelector('.ws-export-open'))
    await click(d.querySelector('.ws-export-save'))
    const status = d.querySelector('.ws-export-sheet [role="status"]')
    assert.ok(status && status.textContent === X.save_failed, 'a refused save is not said')
    sweep(livePlainFace(d), 'refused save'); states += 1
  }, { failSave: true })
  assert.equal(states, 1 + 3 * SUBSETS.length * 2 + 1, 'a state was skipped')
})

/* Verifier finding 18: a named keyboard-and-labels test for the sheet. */
test('(fix 2) keyboard and labels: every control is named, Tab follows the order on screen and reaches the primary, the primary is a real button, nothing is disabled without its reason, and focus goes in on open and back to the door on close', async () => {
  await mounted(async ({ d }) => {
    const door = d.querySelector('.ws-export-open')
    assert.equal(door.tagName, 'BUTTON')
    assert.equal(door.textContent, X.door)
    await click(door)
    const sheet = d.querySelector('.ws-export-sheet')
    assert.equal(d.activeElement, d.getElementById('ws-export-heading'), 'focus did not move into the sheet on open')
    assert.equal(sheet.getAttribute('aria-labelledby'), 'ws-export-heading')

    /** The accessible name, the way a screen reader derives it for these elements. */
    const nameOf = (el) => {
      if (el.getAttribute('aria-label')) return el.getAttribute('aria-label').trim()
      if (el.getAttribute('aria-labelledby')) return d.getElementById(el.getAttribute('aria-labelledby')).textContent.trim()
      if (el.tagName === 'INPUT') { const label = el.closest('label'); return label ? label.textContent.trim() : '' }
      if (el.tagName === 'IFRAME') return (el.getAttribute('title') || '').trim()
      if (el.tagName === 'PRE') { const prev = el.previousElementSibling; return el.getAttribute('aria-label') || (prev ? prev.textContent.trim() : '') }
      return el.textContent.trim()
    }
    const controls = [...sheet.querySelectorAll('input, button, iframe, pre[tabindex], summary, a[href]')]
    for (const el of controls) assert.ok(nameOf(el).length > 3, `a control with no name: <${el.tagName.toLowerCase()} class="${el.className}">`)
    for (const f of ['html', 'md', 'json']) assert.ok(nameOf(d.querySelector(`input[value="${f}"]`)).startsWith(X.formats[f].label))
    for (const c of ['agent_words', 'paths', 'failure_output']) assert.equal(nameOf(d.querySelector(`input[value="${c}"]`)), X.classes[c])
    assert.ok(sheet.querySelectorAll('fieldset > legend').length === 2, 'the two groups are not named by a legend')

    /* Tab order: nothing reorders it (no positive tabindex), so it is DOM
     * order — and the DOM order is the order on screen, top to bottom. */
    const tabbable = [...sheet.querySelectorAll('input, button, pre[tabindex="0"], summary, a[href]')]
      .filter((el) => !el.disabled && el.getAttribute('tabindex') !== '-1')
    assert.deepEqual([...sheet.querySelectorAll('[tabindex]')].filter((el) => Number(el.getAttribute('tabindex')) > 0), [], 'a positive tabindex reorders Tab')
    const order = tabbable.map((el) => el.classList.contains('ws-export-save') ? 'save' : el.classList.contains('ws-export-close') ? 'close' : el.tagName === 'PRE' ? 'preview' : el.tagName === 'SUMMARY' ? 'technical' : el.type + ':' + el.value)
    assert.deepEqual(order, ['radio:html', 'radio:md', 'radio:json', 'checkbox:agent_words', 'checkbox:paths', 'checkbox:failure_output', 'preview', 'save', 'close', 'technical'])

    /* The primary: one, a real button, reachable by Tab. */
    const primaries = [...sheet.querySelectorAll('.ws-change-primary')]
    assert.equal(primaries.length, 1)
    assert.equal(primaries[0].tagName, 'BUTTON')
    assert.equal(primaries[0].getAttribute('type'), 'button')
    assert.ok(tabbable.includes(primaries[0]))

    /* Nothing disabled without its reason and what would enable it. */
    for (const el of sheet.querySelectorAll('[disabled], [aria-disabled="true"]')) {
      const why = el.getAttribute('aria-describedby') && d.getElementById(el.getAttribute('aria-describedby'))
      assert.ok(why && why.textContent.trim().length > 20, `disabled with no reason: ${el.outerHTML.slice(0, 80)}`)
    }

    /* Close: focus returns to the door. */
    await click(d.querySelector('.ws-export-close'))
    assert.ok(d.activeElement && d.activeElement.classList.contains('ws-export-open'), 'focus did not return to the door')
  })
})
