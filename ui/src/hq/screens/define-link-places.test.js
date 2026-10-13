/* "Say what counts as done" / "What counts as done ›" — which Change draws the
 * link, and where (DESIGN-SPEC §3.02 and §3.03; census rows brief#19 and
 * statements#19).
 *
 * The link belongs to a statement the record cannot check by itself (a
 * "Recorded" claim: "I fixed …", no verifier), while nothing is accepted as
 * done. On such a Change it is drawn in FOUR places on the Brief — the landing
 * column, the inspector note, the "Proof stops here" plate, the sticky footer —
 * and in TWO on Every statement — the inspector's "Nothing to run" line and the
 * ② Rests on chip. On a Change whose statement a test can check ("All tests
 * pass.") the inspector's primary act is "Check it again" and the only place
 * left is the landing column. Both are the design, and both are pinned here, so
 * a census Change of the wrong kind is not mistaken for a missing link.
 *
 * Every record here is synthetic; nothing reads a store or reaches a server. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>', { url: 'http://127.0.0.1/' })
globalThis.window = dom.window
globalThis.document = dom.window.document

const here = dirname(fileURLToPath(import.meta.url))

const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import * as R from '../routes.js'
import * as SC from '../screen.jsx'
import { withFrame } from '../data/project.js'
import { changeSnapshot } from '../data/change.js'
import { everyStatementFromChange } from './Every-Statement/from-change.js'
import BriefT from './Change-Brief/template.jsx'
import BriefL from './Change-Brief/logic.js'
import StmtT from './Every-Statement/template.jsx'
import StmtL from './Every-Statement/logic.js'
const S = { brief: [BriefT, BriefL], statements: [StmtT, StmtL] }
export { R, SC, changeSnapshot, everyStatementFromChange }
export function render(name, props, link, state) {
  const L = withFrame(S[name][1])
  const inst = new L(props)
  inst.props = props
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(S[name][0], { v: inst.renderVals(), link }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-define-link-')), 'screens.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

/* ---------- a synthetic served ThreadView: one edit, one agent message ---------- */
const T = (m) => `2026-10-07T10:${String(m).padStart(2, '0')}:00.000Z`
const V1 = 'wt-1111111111111111'
const TID = '0123456789abcdef'
const NOW = new Date('2026-10-07T10:12:00.000Z')
const edit = { id: 'e1', event_id: 'e1', session_id: 's1', ts: T(1), file: '/repo/checkout/api.py', op: 'edit', action: null, moved_to: null, hunks: [], lines_added: 1, lines_removed: 0, ok: true, source: 'structured_patch', via: 'tool', version_id: V1, raw_file: '/x.jsonl', raw_line: 2 }
const command = { id: 'c1', event_id: 'c1', part: 0, session_id: 's1', ts: T(2), command: 'pytest -q tests/test_totals.py', readable: true, outcome: 'succeeded', exit_code: 0, output_excerpt: [], output_lines_total: 0, output_scope: 'command', is_test: true, is_commit: false,
  tally: { passed: 4, failed: 0, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 }, tally_line: null, via: 'bash', version_id: V1, raw_file: '/x.jsonl', raw_line: 3 }
const claim = (text, kind, verifier, rests) => ({ id: 'm1#0', event_id: 'm1', session_id: 's1', ts: T(4), text, kind, has_verifier: verifier, marker: 'completion', rests_on_command_id: rests, followed_by_passing_test: false, version_id: V1, raw_file: '/x.jsonl', raw_line: 5 })
const thread = (said, { run = false } = {}) => ({
  schema_version: 1,
  project: { id: 'p1', name: 'shop', root: '/repo' },
  thread: { id: TID, title: 'Fix the total in checkout/api.py', current_version_id: V1, whose_move: 'you', proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: [] } },
  prompts: [{ event_id: 'p1', session_id: 's1', seq: 1, ts: T(0), text: 'Fix the total in checkout/api.py', work: true, version_id: V1, raw_file: '/x.jsonl', raw_line: 1 }],
  edits: [edit],
  commands: run ? [command] : [],
  claims: [said],
  versions: [{ id: V1, commit: null, short: null, session_id: 's1', from_seq: 1, to_seq: 4, committed: false, message: null, boundary_event_id: null, committed_at: null }],
  current_version_id: V1,
  sessions: [{ id: 's1', ledger_project_id: 'lp', source: 'claude_code', origin: 'person', started_at: T(0), last_at: T(8), file: '/x.jsonl' }],
  files: [],
  masking: { masked: 0, families: [], complete: true },
})
const RECORDED = () => M.changeSnapshot(thread(claim('I fixed the total in `checkout/api.py`.', 'fixed', false, null)), null, NOW, { storageBytes: 2.1e9 })
const CHECKABLE = () => M.changeSnapshot(thread(claim('All tests pass.', 'tests_pass', true, 'c1'), { run: true }), null, NOW, { storageBytes: 2.1e9 })

const unescape = (x) => x.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
function draw(name, snapshot, data, state) {
  const at = { screen: name, change: TID }
  const link = M.R.linker(M.SC.linkAt(at, M.SC.projectOf(snapshot)))
  const html = unescape(M.render(name, { data, frame: {}, at }, link, state))
  return new JSDOM(html).window.document
}
const name = (a) => a.textContent.replace(/\s+/g, ' ').trim()
/** The links of `root` whose name starts with `re`. */
const linksIn = (root, re) => Array.from(root.querySelectorAll('a[href]')).filter((a) => re.test(name(a)))
const DEFINE = /^Say what counts as done/
const COUNTS = /^What counts as done ›$/
const opensDone = (a) => M.R.parse(a.getAttribute('href')).screen === 'done' && M.R.parse(a.getAttribute('href')).change === TID

test('a Recorded claim, nothing accepted: the Brief draws "Say what counts as done" in the landing column, the note, the stop plate and the footer', () => {
  const s = RECORDED()
  assert.equal(s.statements[0].verifier, false, 'the statement is a Recorded claim')
  assert.equal(s.criteria.accepted, false)
  const doc = draw('brief', s, s)
  const landing = doc.querySelector('section[aria-label="Where this Change stands"]')
  const inspector = doc.querySelector('aside[aria-label="Statement inspector"]')
  const plates = inspector.querySelector('ol[aria-label="Proof chain, link by link"]')
  assert.equal(linksIn(landing, DEFINE).length, 1, 'landing column')
  assert.equal(linksIn(inspector, DEFINE).length, 3, 'note, stop plate and footer')
  assert.equal(linksIn(plates, DEFINE).length, 1, 'the stop plate')
  const inInspector = linksIn(inspector, DEFINE)
  assert.equal(plates.contains(inInspector[0]), false, 'the first one is the note, above the chain')
  assert.equal(plates.contains(inInspector[1]), true, 'the middle one is the stop plate')
  assert.equal(plates.contains(inInspector[2]), false, 'the last one is the sticky footer, after the chain')
  for (const a of [...linksIn(landing, DEFINE), ...inInspector]) assert.ok(opensDone(a), 'each opens this Change’s Done-Criteria: ' + a.getAttribute('href'))
})

test('a Recorded claim, nothing accepted: Every statement draws "What counts as done ›" in the inspector and on the ② Rests on link', () => {
  const s = RECORDED()
  const doc = draw('statements', s, M.everyStatementFromChange(s))
  const inspector = doc.querySelector('aside[aria-label="Selected statement"]')
  const found = linksIn(inspector, COUNTS)
  assert.equal(found.length, 2, 'the "Nothing to run" line and the chain chip')
  for (const a of found) assert.ok(opensDone(a), a.getAttribute('href'))
  const chain = inspector.querySelector('ol')
  assert.equal(linksIn(chain, COUNTS).length, 1, 'one of them sits on the proof chain')
})

test('a statement a test can check: the inspector offers "Check it again", not the define link; only the landing column keeps it', () => {
  const s = CHECKABLE()
  assert.equal(s.statements[0].verifier, true)
  const brief = draw('brief', s, s)
  assert.equal(linksIn(brief.querySelector('section[aria-label="Where this Change stands"]'), DEFINE).length, 1, 'landing column')
  assert.equal(linksIn(brief.querySelector('aside[aria-label="Statement inspector"]'), DEFINE).length, 0, 'inspector')
  const stmts = draw('statements', s, M.everyStatementFromChange(s))
  assert.equal(linksIn(stmts.querySelector('aside[aria-label="Selected statement"]'), COUNTS).length, 0, 'Every statement inspector')
})

/* census statements#20 "Receipt chip Accepted": an accepted line's ① Accepted
 * names a version of what counts as done, which sits at no step (DECISIONS C30
 * R-ROUTES "Step view", decision 5). The receipt is drawn, as text: no link, no
 * button, no arrow. */
test('an accepted line: ① Accepted draws its receipt "Accepted v1" as plain text, with no link, no button and no arrow', () => {
  const s = RECORDED()
  s.criteria = {
    ...s.criteria, accepted: true, acceptedAt: '10:30', acceptedBy: 'you', version: 'v1', lineCount: 1, acceptedOnVersion: null, provenOn: null,
    lines: [{ id: 'line-1', num: 1, gate: false, link: null, text: 'When the coupon has expired, the checkout is rejected.', proof: { verdict: 'in_progress', stopsAt: 2, restsOn: null, lookedAt: [], misses: [] }, recordedLine: null }],
  }
  const doc = draw('statements', s, M.everyStatementFromChange(s), { sel: 'line-1' })
  const inspector = doc.querySelector('aside[aria-label="Selected statement"]')
  assert.match(inspector.textContent, /① Accepted/, 'the selected line shows ① Accepted in its proof chain')
  const item = Array.from(inspector.querySelectorAll('li')).find((li) => /① Accepted/.test(li.textContent))
  const receipt = Array.from(item.querySelectorAll('span')).filter((sp) => sp.children.length === 2 && name(sp.children[0]) === 'Accepted' && name(sp.children[1]) === 'v1')
  assert.equal(receipt.length, 1, 'one receipt: the kind "Accepted" and the value "v1"')
  assert.equal(name(receipt[0]).includes('›'), false, 'no arrow: nothing is opened')
  assert.equal(Array.from(inspector.querySelectorAll('a, button')).filter((x) => /^Accepted\b/.test(name(x))).length, 0, 'not a link and not a button')
})
