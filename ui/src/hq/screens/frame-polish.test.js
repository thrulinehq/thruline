/* Polish (HQ Next): the shared frame and the controls, on the live path.
 *
 * 1. The top bar never wraps: every app screen's live header carries the
 *    `tl-topbar` rules (controls keep their size, the breadcrumb title absorbs
 *    the squeeze with an ellipsis and its full text in title=); the gallery
 *    header does not, so the canvas renders as drawn. The CSS itself is
 *    measured in headless Chrome (report), not here.
 * 2. One recording derivation: every pill reads `recordingPill` (data/project.js)
 *    and never says "· N projects", "not known" or "From the record".
 * 3. Quotes inside ❝ ❞ use `statements[].plain`.
 * 5. The canvas's static focus ring is drawn in the gallery only.
 * 6. A control that looks enabled does something real; otherwise it is off
 *    and says why.
 * Fixtures are synthetic; nothing here is the founder's record. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'

const here = dirname(fileURLToPath(import.meta.url))
const read = (p) => readFileSync(join(here, p), 'utf8')

const APP = ['Inbox', 'Change-Brief', 'Every-Statement', 'What-Changed', 'Done-Criteria', 'Repair-Loop', 'Approve-Sheet', 'Command-Ask', 'Whole-Story', 'Record-Over-Time', 'Team-View', 'Settings']
const CHANGE = ['Change-Brief', 'Every-Statement', 'What-Changed', 'Done-Criteria', 'Repair-Loop', 'Approve-Sheet', 'Command-Ask', 'Whole-Story']

const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { withFrame } from '../data/project.js'
import { changeSnapshot } from '../data/change.js'
import { threadFixture } from './What-Changed/thread-fixture.js'
import CB_T from './Change-Brief/template.jsx'
import CB_L from './Change-Brief/logic.js'
import ES_T from './Every-Statement/template.jsx'
import ES_L from './Every-Statement/logic.js'
import { everyStatementFromChange } from './Every-Statement/from-change.js'
import WC_T from './What-Changed/template.jsx'
import WC_L from './What-Changed/logic.js'
import { whatChangedFromChange } from './What-Changed/from-change.js'
import WS_T from './Whole-Story/template.jsx'
import WS_L from './Whole-Story/logic.js'
import { wholeStoryFrom } from './Whole-Story/from-change.js'
import DC_T from './Done-Criteria/template.jsx'
import DC_L from './Done-Criteria/logic.js'
import { selectDoneCriteria } from './Done-Criteria/from-change.js'
import AS_T from './Approve-Sheet/template.jsx'
import AS_L from './Approve-Sheet/logic.js'
import { selectApproveSheet } from './Approve-Sheet/from-change.js'
import ST_T from './Settings/template.jsx'
import ST_L from './Settings/logic.js'
import { settingsFromServer } from './Settings/from-server.js'
import RT_T from './Record-Over-Time/template.jsx'
import RT_L from './Record-Over-Time/logic.js'
import { recordFromStatements } from './Record-Over-Time/from-projects.js'
import { commandFromChange } from './Command-Ask/from-change.js'
const S = {
  cb: [CB_T, CB_L], es: [ES_T, ES_L], wc: [WC_T, WC_L], ws: [WS_T, WS_L], dc: [DC_T, DC_L], as: [AS_T, AS_L], st: [ST_T, ST_L], rt: [RT_T, RT_L],
}
export { changeSnapshot, threadFixture, everyStatementFromChange, whatChangedFromChange, wholeStoryFrom, selectDoneCriteria, selectApproveSheet, settingsFromServer, recordFromStatements, commandFromChange }
export function render(key, props, { live = true, state } = {}) {
  const [Template, Logic] = S[key]
  const L = live ? withFrame(Logic) : Logic
  const inst = new L(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-polish-')), 'screens.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const NOW = new Date('2026-10-07T10:30:00.000Z')
const LANES_ON = { claude_code: { live: 1 }, codex: { live: 0 }, window_minutes: 5 }
const LANES_OFF = { claude_code: { live: 0 }, codex: { live: 0 }, window_minutes: 5 }
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const words = (html) => unescape(html.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ')

/** A fixture thread whose claim is written in markdown, as agents write. */
function markdownThread() {
  const v = M.threadFixture()
  for (const c of v.claims) c.text = '**`pytest -q`:** [4 passed](https://example.test/run), 0 failed.'
  return v
}
const snap = (v = M.threadFixture()) => M.changeSnapshot(v, null, NOW, { storageBytes: 5_400_000_000 })

/* ---- 1. the top bar never wraps ---- */

test('polish 1: every app header is the no-wrap top bar on the live path only, and every Change breadcrumb title carries its full text', () => {
  for (const n of APP) {
    const t = read(n + '/template.jsx')
    assert.ok(t.includes('<header className={v?.rec ? "tl-topbar" : undefined}'), n + ': header not gated to the live path')
    const css = read(n + '/styles.css')
    assert.ok(css.includes('.tl-topbar>:not(nav){flex-shrink:0;white-space:nowrap}'), n + ': controls may shrink or wrap')
    assert.ok(css.includes('.tl-topbar>nav>.tl-crumb-title{flex:0 1 auto !important;min-width:0 !important;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}'), n + ': title does not absorb the squeeze')
    const idx = read(n + '/index.jsx')
    assert.match(idx, /withFrame\(Logic\)/, n + ': live screen is not given the frame')
  }
  for (const n of CHANGE) {
    const t = read(n + '/template.jsx')
    assert.match(t, /<span className="tl-crumb-title" title=\{[^}]+\}/, n + ': breadcrumb title has no full-text title=')
  }
  const s = snap()
  const live = M.render('cb', { data: s, frame: { lanes: LANES_OFF } })
  assert.match(live, /<header class="tl-topbar"/)
  assert.match(live, /class="tl-crumb-title" title="Fix the checkout total"/)
  assert.doesNotMatch(M.render('cb', {}, { live: false }), /tl-topbar/, 'the gallery header must render as the canvas draws it')
})

/* ---- 2. one honest recording pill ---- */

test('polish 2: the pill on every live screen is the one derivation; quiet when nothing is recent', () => {
  for (const n of APP.filter((x) => x !== 'Settings')) {
    assert.match(read(n + '/template.jsx'), /\{v\?\.rec\?\.text \?\? /, n + ': the pill text does not come from recordingPill')
  }
  assert.match(read('Settings/live.js'), /recordingPill\(/)
  const s = snap()
  // Scope this contract to the header: the chain check can truthfully say
  // its result is not known yet while the recording pill has a known state.
  const header = (html) => {
    const top = new JSDOM(html).window.document.querySelector('header.tl-topbar')
    assert.ok(top, 'the live header is present')
    return top.textContent.replace(/\s+/g, ' ')
  }
  const on = header(M.render('cb', { data: s, frame: { lanes: LANES_ON } }))
  const off = header(M.render('cb', { data: s, frame: { lanes: LANES_OFF } }))
  assert.ok(on.includes('Recording') && !on.includes('Not recording now'))
  assert.ok(off.includes('Not recording now'))
  const es = header(M.render('es', { data: M.everyStatementFromChange(s), frame: { lanes: LANES_OFF } }))
  const ws = header(M.render('ws', { data: M.wholeStoryFrom(s, null, { now: NOW.getTime() }), frame: { lanes: LANES_OFF } }))
  const rt = header(M.render('rt', { data: M.recordFromStatements({ projects: [{ id: 'p1', name: 'shop', threads: 1, claims: 0, sources: ['claude_code'] }], statements: { p1: { claims: [], sessions: [] } }, storage: { bytes: 1e9 }, now: NOW.getTime() }), frame: { lanes: LANES_OFF } }))
  for (const t of [on, off, es, ws, rt]) {
    assert.doesNotMatch(t, /Recording · \d+ project|not known|From the record|following live|project recorded/i)
  }
})

/* ---- 3. quotes read as words ---- */

test('polish 3: every quote inside ❝ ❞ is plain words — no **, backticks or ](', () => {
  const s = snap(markdownThread())
  const raw = /\*\*|`|\]\(/
  const quoted = (html) => [...words(html).matchAll(/❝([^❞]*)❞/g)].map((m) => m[1])
  const pages = {
    'Change-Brief': M.render('cb', { data: s, frame: {} }),
    'Every-Statement': M.render('es', { data: M.everyStatementFromChange(s), frame: {} }),
    'What-Changed': M.render('wc', { data: M.whatChangedFromChange(s), frame: {} }),
    'Whole-Story': M.render('ws', { data: M.wholeStoryFrom(s, null, { now: NOW.getTime() }), frame: {} }),
    'Done-Criteria': M.render('dc', { data: M.selectDoneCriteria(s, null), frame: {} }),
  }
  for (const [n, html] of Object.entries(pages)) {
    const qs = quoted(html)
    assert.ok(qs.length > 0, n + ' draws no quote')
    for (const q of qs) assert.doesNotMatch(q, raw, n + ': raw markdown in a quote: ' + q)
  }
  // Find, ask, act draws its quotes from the same plain words
  const ca = JSON.stringify(M.commandFromChange(s, { project: { id: 'p1', name: 'shop' }, threads: [] }, { readAt: '10:30' }))
  assert.doesNotMatch(ca, /\*\*`pytest/)
  // the verbatim words are still in the snapshot
  assert.ok(s.statements[0].text.includes('**`pytest -q`:**'))
})

/* ---- 5. focus ring only on real focus ---- */

test('polish 5: the canvas’s static focus ring is gallery-only; live, only :focus-visible draws it', () => {
  for (const n of APP.filter((x) => x !== 'Command-Ask')) {
    const t = read(n + '/template.jsx')
    assert.ok(!t.includes('outline: "2px solid #FF7A29"'), n + ': a permanent focus ring is drawn on the live path')
    assert.ok(read(n + '/styles.css').includes('button:focus-visible,a:focus-visible,input:focus-visible,textarea:focus-visible,select:focus-visible,[tabindex]:focus-visible{outline:2px solid #FF7A29 !important;outline-offset:2px}'), n)
  }
  const s = snap()
  const live = M.render('cb', { data: s, frame: {} })
  assert.doesNotMatch(live, /outline:2px solid #FF7A29/)
  assert.match(M.render('cb', {}, { live: false }), /outline:2px solid #FF7A29/, 'the gallery keeps the canvas ring')
})

/* ---- 6. enabled means it does something ---- */

test('polish 6: Settings says what its buttons really do: “Read again” (Codex) and “Show how” (run capture is turned on in a terminal)', () => {
  const data = M.settingsFromServer({ storage: { bytes: 5e9, parts: [] }, lanes: LANES_OFF, now: NOW })
  const html = M.render('st', { data, frame: { lanes: LANES_OFF } })
  const t = words(html)
  assert.ok(t.includes('Read again'), 'Codex button')
  assert.ok(t.includes('Show how'), 'run capture button')
  assert.ok(!/\bTurn on\b/.test(t) && !/\bTry again\b/.test(t))
})

/* C30 (Done-Criteria rows 8–10): the openers work for a tracked Change; for
 * work not tracked as a Change (as here) they stay off, with the reason — and
 * the reason is what to do, never "Not available in this version" (C30). */
test('polish 6: Done-Criteria’s add and template openers are off for untracked work, with the reason', () => {
  const html = M.render('dc', { data: M.selectDoneCriteria(snap(), null), frame: {} })
  for (const label of ['Add a statement', 'Start from a template']) {
    const i = html.indexOf(label)
    const tag = html.slice(html.lastIndexOf('<button', i), i)
    assert.match(tag, /disabled=""/, label + ' looks enabled')
    assert.match(tag, /title="Track this work as a Change first\./, label + ' gives no reason')
  }
  assert.ok(words(html).includes('Track this work as a Change first. Then anything you add arrives as a proposal.'))
  assert.ok(!/not available in this version/i.test(words(html)))
})

test('polish 6: the Approve sheet never says “All 0 rest on” when nothing counts as done', () => {
  // the claim rests on a run on the current version, and nothing counts as done
  const v = M.threadFixture()
  const cur = v.current_version_id || v.thread.current_version_id
  for (const k of ['edits', 'commands', 'claims', 'prompts']) for (const r of v[k] || []) r.version_id = cur
  const data = M.selectApproveSheet(snap(v), null, {})
  assert.ok(data.run, 'fixture: the approval would rest on a run')
  const t = words(M.render('as', { data, frame: {} }))
  assert.ok(!/All 0 rest on/.test(t))
})

test('polish 6 (B): Record over time counts from the statements route; what a server does not send is “not counted”, never 0; Export CSV is on', () => {
  const day = (n, h = 10) => new Date(2026, 9, 7 - n, h, 0, 0).toISOString()
  const now = new Date(2026, 9, 7, 12, 0, 0).getTime()
  const doc = {
    claims: [
      { id: 'a', thread_id: 't', ts: day(0), agent: 'claude_code', session_id: 's1', kind: 'tests_pass', has_verifier: true, verdict: 'proven', verdict_basis: 'run_holds' },
      { id: 'b', thread_id: 't', ts: day(1), agent: 'claude_code', session_id: 's1', kind: 'tests_pass', has_verifier: true, verdict: 'not_proven', verdict_basis: 'edited_since_run' },
      { id: 'c', thread_id: 't', ts: day(2), agent: 'codex', session_id: 's2', kind: 'done', has_verifier: false, verdict: 'recorded', verdict_basis: 'no_verifier' },
      { id: 'd', thread_id: 't', ts: day(20), agent: 'codex', session_id: 's2', kind: 'tests_pass', has_verifier: true, verdict: 'contradicted', verdict_basis: 'later_run_failed' },
    ],
    sessions: [{ id: 's1', source: 'claude_code', started_at: day(1, 9), last_at: day(0, 11) }, { id: 's2', source: 'codex', started_at: day(20, 9), last_at: day(2, 11) }],
  }
  const d = M.recordFromStatements({ projects: [{ id: 'p1', name: 'shop', threads: 1, claims: 4, sources: ['claude_code', 'codex'], last_activity: day(0) }], statements: { p1: doc }, storage: { bytes: 2e9 }, now })
  assert.deepEqual([d.tallies[7].p1.cc.statements, d.tallies[7].p1.cc.proven, d.tallies[7].p1.cc.needsRecheck], [2, 1, 1])
  assert.equal(d.tallies[7].p1.cx.recordedStopsAtSo, 1)
  assert.equal(d.tallies[30].p1.cx.rerunFailed, 1)
  assert.equal(d.tallies[7].p1.cx.rerunFailed, 0, 'a claim 20 days ago is outside the 7-day window')
  assert.equal(d.tallies[7].p1.cc.reproven, null, 'an unserved count is absent, never 0')
  assert.equal(d.daily.p1.cc.statements[89 - 5], null, 'a day Claude Code was not recorded is null')
  assert.equal(d.daily.p1.cc.statements[89], 1)
  const html = M.render('rt', { data: d, frame: { lanes: LANES_OFF } }, { state: { range: '7' } })
  const t = words(html)
  assert.ok(t.includes('1 of 3 statements are proven on the version they were about.'), t.slice(0, 400))
  // this fixture is the route as an older server sent it: no proven_at, run_before or proof_moved
  assert.ok(t.includes('Not counted: proof times not recorded'))
  assert.ok(t.includes('Earlier re-checks are not counted'))
  assert.ok(!t.includes('Not available in this version'))
  const i = html.indexOf('Export CSV</button>')
  const tag = html.slice(html.lastIndexOf('<button', i), i)
  assert.doesNotMatch(tag, /disabled=""/)
  assert.match(tag, /title="Save the counts on this page as a CSV file/)
})
