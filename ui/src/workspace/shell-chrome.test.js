/* P6-1 — the three altitudes, rendered (design record §3.1, §6).
 *
 * The surfaces are bundled and rendered the same way the rest of this suite
 * renders them: to static markup for structure, and into jsdom with real key
 * presses for the keyboard. Every fixture is synthetic; no string here came
 * off a real machine.
 *
 * What is held here, one test per rule:
 *  - the shell chrome imports nothing that could produce a state word, and
 *    spells none; Brief derives none of its own either;
 *  - every shell string is Sam's vocabulary, and every off slot says why and
 *    what would enable it;
 *  - Brief is one story with ONE primary and no toolbar, strip, dock or tree;
 *    Explore is one slim toolbar and three dock tabs; Workbench is every control the
 *    workspace had at HEAD dc47818, by name and by count;
 *  - the key map works under real key presses, ignores typing, and remembers
 *    only what a person chose.
 */
import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'
import { HQ_COPY } from './hq-copy.js'
import { STATE_WORD_SET } from './change-model.js'
import { KEYMAP } from './altitude.js'
import { census, missingNames, renderWorkspaceHtml } from './control-census.js'
import { RAIL } from './logic.js'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-altitudes-'))
after(() => rm(temporary, { recursive: true, force: true }))

const COMMON = {
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
}
async function load(name, contents) {
  const result = await build({ ...COMMON, stdin: { contents, resolveDir: here } })
  const file = join(temporary, name)
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}

const STATIC = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { Workspace } from './shell.jsx';
  import { adaptSession } from './adapter.js';
  import { ChangeShell } from './change-workspace.jsx';
  import { KeySheet, ShellTopBar } from './shell-chrome.jsx';
  import { buildChangeView } from './change-view.js';
  import { buildCoverageView, evidenceEntries } from './coverage-view.js';
  import { deriveChangeModel } from './change-model.js';
  import { scopeLine } from './inbox-view.js';
  export { scopeLine };
  export const html = (el) => renderToStaticMarkup(el);
  export const renderShell = (props) => renderToStaticMarkup(React.createElement(ChangeShell, props));
  export const renderKeys = () => renderToStaticMarkup(React.createElement(KeySheet, { onClose() {} }));
  export const renderTop = (props) => renderToStaticMarkup(React.createElement(ShellTopBar, props));
  export { Workspace, adaptSession, buildChangeView, buildCoverageView, evidenceEntries, deriveChangeModel };
`
const M = await load('altitudes.mjs', STATIC)

const doc = (html) => new JSDOM('<!doctype html><body>' + html + '</body>').window.document
const S = HQ_COPY.shell
const read = (f) => readFileSync(join(here, f), 'utf8')

/* ── the Change fixture: one line, one run that proved it on e71c5b0 ────── */

const CHANGE = {
  id: 'ch1',
  project_id: 'p1',
  request: { title: 'Totals add up on the checkout page' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1,
    criteria: ['The totals add up'],
    allowed_scope: ['checkout/'],
    check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  examination: 'none_yet',
}
const COVERAGE = {
  scope_version: 1,
  link_rows: 1,
  criteria: [{ index: 1, text: 'The totals add up', state: 'evidence', items: [{
    link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run', name: 'tests pass',
    observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies', fresh: true, fresh_slug: 'applicability_still_applies',
  }] }],
}
function changeModel(change = CHANGE, coverage = COVERAGE) {
  const model = M.buildChangeView({ change, proposals: null, work: [], evidence: M.evidenceEntries(coverage) })
  model.coverage = M.buildCoverageView(coverage)
  model.hq = M.deriveChangeModel(change, coverage, null, null, [])
  return model
}
const shellAt = (altitude, over) => M.renderShell({ model: changeModel(), stage: 'define', altitude, register: 'manager', onAltitude() {}, onRegister() {}, onReload() {}, ...over })

/* ── 1. no per-screen state strings (§3.8, §10) ───────────────────────── */

test('the shell chrome imports nothing that could produce a state word: React, the copy, the template filler and the altitude module', () => {
  const src = read('shell-chrome.jsx')
  const imports = [...src.matchAll(/^import\s[^'"]*['"]([^'"]+)['"]/gm)].map((m) => m[1]).sort()
  assert.deepEqual(imports, ['./altitude.js', './hq-copy.js', './hq-narrate.js', 'react'])
  /* and altitude.js imports nothing at all */
  assert.deepEqual([...read('altitude.js').matchAll(/^import\s/gm)], [])
  const code = src.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '')
  for (const word of STATE_WORD_SET) assert.ok(!code.includes(word), `shell-chrome.jsx spells the state word "${word}"`)
})

test('Brief derives no state word, version or count of its own: it reads each off model.hq', () => {
  const src = read('change-workspace.jsx')
  const start = src.indexOf('export function ChangeBrief(')
  const end = src.indexOf('\n}\n', start)
  assert.ok(start > 0 && end > start, 'ChangeBrief moved')
  const body = src.slice(start, end).replace(/\/\*[\s\S]*?\*\//g, '')
  for (const word of STATE_WORD_SET) assert.ok(!body.includes(word), `ChangeBrief spells "${word}"`)
  for (const field of ['hq.word', 'hq.state', 'hq.inspector.shows.line', 'hq.headline.text', 'hq.version.text', 'hq.packet.coversLine', 'hq.actionableRuns']) {
    assert.ok(body.includes(field), `ChangeBrief no longer reads ${field}`)
  }
  assert.doesNotMatch(body, /versionSentence|versionLine|deriveChangeModel|buildChangeView|stateLine|criterionState/, 'Brief derives instead of reading')
  /* the breadcrumb takes its version off the model too */
  const crumbs = src.slice(src.indexOf('export function changeCrumbs('), src.indexOf('export function ChangeShell('))
  assert.match(crumbs, /v\.short/)
  for (const word of STATE_WORD_SET) assert.ok(!crumbs.includes(word))
})

/* ── 2. the words (§5, 03-USER-LENS rules 2 and 6) ────────────────────── */

const walk = (o, p = []) => Object.entries(o).flatMap(([k, v]) => (typeof v === 'string' ? [[p.concat(k).join('.'), v]] : walk(v, p.concat(k))))
const SHELL_STRINGS = walk(S)

test('every shell string is Sam\'s vocabulary: no engine noun, no accusation, no all-clear, no state word', () => {
  assert.ok(SHELL_STRINGS.length > 60, `found ${SHELL_STRINGS.length}`)
  const PLAIN_BANNED = [/\bevents?\b/i, /\breceipts?\b/i, /\bcandidates?\b/i, /\bapplicability\b/i, /\bdispositions?\b/i, /\bpacket\b/i, /\bledger\b/i, /\bsession id\b/i, /\btree\b/i, /\bpredicates?\b/i, /\bseq\b/i, /\bfacets?\b/i, /\bobjects?\b/i, /\bderived\b/i]
  const NEVER = [/\blie[ds]?\b/i, /\blying\b/i, /\bhallucinat/i, /\bcaught\b/i, /\bbullshit\b/i, /\bbug-free\b/i, /\bsafe\b/i, /\ball clear\b/i, /\d\s*%/, /\bPro\b/]
  for (const [path, text] of SHELL_STRINGS) {
    for (const re of [...PLAIN_BANNED, ...NEVER]) assert.doesNotMatch(text, re, `shell.${path}: ${text}`)
    for (const word of STATE_WORD_SET) assert.ok(!text.includes(word), `shell.${path} carries the state word "${word}"`)
  }
})

test('every off slot is a reason and an enabler, and the enabler names what would turn it on', () => {
  const pairs = []
  const find = (o, p) => {
    for (const [k, v] of Object.entries(o)) {
      if (v && typeof v === 'object' && 'reason' in v) pairs.push([p + k, v])
      else if (v && typeof v === 'object') find(v, p + k + '.')
    }
  }
  find(S, 'shell.')
  /* Five at P6-1; P6-3 turned two of them (Focus thread, Walk me through it)
   * into working buttons, so three remain: Ask, Outline on the Change screen,
   * and Saved views on the Change screen. */
  assert.ok(pairs.length >= 3, `found ${pairs.length}`)
  for (const [path, pair] of pairs) {
    assert.deepEqual(Object.keys(pair).sort(), ['enabler', 'reason'], path)
    assert.ok(HQ_COPY.enablers.some((e) => pair.enabler.toLowerCase().includes(e)), `${path}.enabler names nothing that enables it: ${pair.enabler}`)
  }
})

test('the key list renders every §6 key, and — since P6-3 — none says it is still to come', () => {
  const d = doc(M.renderKeys())
  const rows = [...d.querySelectorAll('.ws-shell-keys > div')]
  assert.equal(rows.length, KEYMAP.length)
  for (const row of rows) {
    const later = row.textContent.includes(S.keys.later)
    assert.equal(later, row.getAttribute('data-wired') === 'no', row.textContent)
  }
  assert.deepEqual(rows.filter((r) => r.getAttribute('data-wired') === 'no').map((r) => r.querySelector('dt').textContent), [])
  for (const [k, line] of [['JK', S.keys.walk], ['F', S.keys.thread]]) {
    const row = rows.find((r) => r.querySelector('dt').textContent === k)
    assert.equal(row.querySelector('dd').textContent, line, `${k} is described live, with no "later"`)
  }
  assert.ok(d.body.textContent.includes(S.keys.moved), 'the three keys the workspace gave up are named, with where their acts went')
})

/* ── 3. the Change screen at each altitude ────────────────────────────── */

test('Brief on the Change screen: one title, one sentence, one state chip, one version, one scope line — each the model\'s own field', () => {
  const model = changeModel()
  const hq = model.hq
  const d = doc(shellAt('brief'))
  const story = d.querySelector('.ws-brief-story')
  assert.equal(d.querySelectorAll('h1').length, 1)
  assert.equal(story.querySelector('h1').textContent, hq.title)
  assert.equal(story.querySelector('.ws-change-conclusion').textContent, hq.headline.text)
  const chip = story.querySelector('.ws-brief-chip')
  assert.equal(chip.textContent, hq.inspector.shows.line, 'the chip is the model\'s own line')
  assert.ok(chip.textContent.startsWith(hq.word + ':'), 'led by the state word')
  assert.ok(chip.textContent.includes('version ' + hq.version.short) && chip.textContent.includes('checkout/'), '"Proven" with its version and scope beside it, never alone (§3.8)')
  assert.equal(chip.getAttribute('data-state'), hq.state)
  assert.equal(story.querySelector('.ws-change-version').textContent, hq.version.text)
  assert.equal(story.querySelector('.ws-brief-scope').textContent, hq.packet.coversLine)
  assert.equal(d.querySelectorAll('.ws-brief-chip').length, 1)
})

test('Brief has ONE primary action, the Inspector\'s, and no stage strip, toolbar, filter, dock or tree', () => {
  const d = doc(shellAt('brief'))
  assert.equal(d.querySelectorAll('.ws-change-primary').length, 1, 'one filled primary')
  assert.equal(d.querySelectorAll('[data-primary-action]').length, 1)
  assert.ok(d.querySelector('[data-primary-action] .ws-change-primary'), 'the one primary is inside the marked box')
  assert.equal(d.querySelector('.ws-change-stages'), null, 'no six-stage strip')
  assert.equal(d.querySelector('[role="toolbar"]'), null)
  assert.equal(d.querySelector('[role="tablist"]'), null)
  /* the Inspector's four blocks, in order */
  const I = changeModel().hq.inspector
  assert.deepEqual([...d.querySelectorAll('.ws-change-inspector > .ws-inspector-block > h3')].map((h) => h.textContent), [I.what.heading, I.shows.heading, I.settle.heading, I.action.heading])
})

test('Explore and Workbench on the Change screen are the whole Change screen as before: the six stages and the technical drawer', () => {
  for (const alt of ['explore', 'workbench']) {
    const d = doc(shellAt(alt))
    assert.equal(d.querySelectorAll('.ws-change-stage').length, 6, alt)
    assert.ok(d.querySelector('.ws-change-technical'), alt)
    assert.equal(d.querySelector('.ws-brief-story'), null, alt)
  }
})

test('the top bar: project › work › version from the model, the three altitudes, Plain / Technical, Ask on with a Change open, and no read time it does not have', () => {
  const d = doc(shellAt('brief'))
  const crumbs = [...d.querySelectorAll('.ws-shell-crumb')].map((c) => c.textContent.replace('›', '').trim())
  /* BUILD-10 (D-050): this fixture records no folder for its work, so the
   * project segment is dropped. It used to expect "Project not named in this
   * record" first, which was the defect (P6B-HTT-7). */
  assert.deepEqual(crumbs, [CHANGE.request.title, 'version e71c5b0'])
  const seg = (label) => [...d.querySelectorAll(`[aria-label="${label}"] button`)].map((b) => [b.textContent, b.getAttribute('aria-pressed')])
  assert.deepEqual(seg(S.topbar.altitude_label), [['Brief', 'true'], ['Explore', 'false'], ['Workbench', 'false']])
  const hints = [...d.querySelectorAll(`[aria-label="${S.topbar.altitude_label}"] button`)].map((b) => b.getAttribute('title'))
  assert.deepEqual(hints.map((h) => h.match(/Press (\d)\.$/)[1]), ['1', '2', '3'])
  for (const hint of hints) assert.doesNotMatch(hint, /\bPro\b/)
  assert.deepEqual(seg(S.topbar.register_label), [['Plain', 'true'], ['Technical', 'false']])
  /* P6-4: with a Change on the screen Ask is a door, not a slot. It was the
   * off slot until P6-4 and this assertion is corrected in place rather than
   * left standing; the off form is still held, one test below, on the screen
   * that really has no Change to look in. */
  const ask = d.querySelector('.ws-shell-ask-btn')
  assert.equal(ask.textContent, HQ_COPY.ask.label)
  assert.equal(ask.hasAttribute('aria-disabled'), false, 'Ask is not off on a screen with a Change open')
  assert.equal(ask.getAttribute('title'), HQ_COPY.ask.hint)
  assert.equal([...d.querySelectorAll('.ws-shell-slot-btn')].filter((b) => b.textContent === S.ask.label).length, 0)
  assert.ok(d.querySelector('.ws-shell-read').textContent.includes(S.topbar.read_unknown), 'the honest unavailable form')
  assert.doesNotMatch(d.querySelector('.ws-shell-top').textContent, /\blive\b|\d\d:\d\d/i, 'no live pill and no invented time')
})

test('P6-4: a shell with no Change to look in keeps Ask off, and says why and what would turn it on', () => {
  const d = doc(M.renderTop({ crumbs: [], altitude: 'brief', register: 'manager', onAltitude() {}, onRegister() {} }))
  assert.equal(d.querySelector('.ws-shell-ask-btn'), null, 'no live Ask without a panel to open')
  const ask = [...d.querySelectorAll('.ws-shell-slot-btn')].find((b) => b.textContent === S.ask.label)
  assert.equal(ask.getAttribute('aria-disabled'), 'true')
  const why = d.getElementById(ask.getAttribute('aria-describedby')).textContent
  assert.ok(why.includes(S.ask.why.reason) && why.includes(S.ask.why.enabler))
  /* And the reason is the TRUE one now: not "it is not built", but that this
   * screen has no Change for Ask to look in. */
  assert.doesNotMatch(why, /not built/i)
})

test('a Change with no version names none in the top bar, and the sidebar says what Outline and Saved views wait for', () => {
  const change = { ...CHANGE, implementation: null }
  const d = doc(M.renderShell({ model: changeModel(change), stage: 'define', altitude: 'brief', register: 'manager', onAltitude() {}, onRegister() {} }))
  assert.equal(d.querySelector('.ws-shell-crumb-version').textContent.replace('›', '').trim(), S.topbar.version_none)
  const side = d.querySelector('.ws-shell-side')
  assert.deepEqual([...side.querySelectorAll('button')].map((b) => b.textContent), [S.sidebar.work, S.sidebar.outline, S.sidebar.saved])
  for (const b of side.querySelectorAll('[aria-disabled="true"]')) {
    const why = d.getElementById(b.getAttribute('aria-describedby')).textContent
    assert.ok(HQ_COPY.enablers.some((e) => why.toLowerCase().includes(e)), why)
  }
})

/* ── 4. the workspace at each altitude, and the Workbench census ────────────── */

const at = (altitude, over) => doc(renderWorkspaceHtml(M, altitude, over))
const RAIL_LABELS = RAIL.flatMap((g) => g.items.map((it) => it[1]))
const names = (d) => census(d).map((r) => r.name)
const BASELINE = JSON.parse(read('workbench-controls.baseline.json'))

test('Workbench draws every control the workspace drew at HEAD dc47818 — every name, at least as many times — in Focus and in Everything', () => {
  /* Both modes, because Focus folds the filter strip's lens controls away:
   * counted in Focus alone, the strip could vanish from Workbench unnoticed. */
  assert.equal(BASELINE.altitude, 'workbench', 'the baseline is keyed to the current altitude')
  assert.equal(BASELINE.total, 93, 'the Focus baseline this card measured')
  assert.equal(BASELINE.everything.total, 95, 'the Everything baseline this card measured')
  for (const [mode, base, over] of [['focus', BASELINE, undefined], ['everything', BASELINE.everything, { focusMode: 'everything' }]]) {
    const after = census(at('workbench', over))
    assert.deepEqual(missingNames(base.rows, after), [], `${mode}: a control Workbench used to have is gone`)
    assert.ok(after.length >= base.total, `${mode}: ${after.length} < ${base.total}`)
  }
})

test('the fifteen rail destinations are Workbench\'s: absent at Brief and Explore, every one back at Workbench', () => {
  assert.equal(RAIL_LABELS.length, 15)
  /* The rail is the \`Destinations\` nav; the HQ panel has its own "Trace" tab,
   * which is not the rail's, so the rail is read as a region. */
  const rail = (d) => d.querySelector('nav[aria-label="Destinations"]')
  for (const alt of ['brief', 'explore']) {
    const d = at(alt)
    assert.ok(d.querySelector('nav[aria-label="' + S.sidebar.label + '"]'), `${alt} has the labelled sidebar`)
    assert.equal(rail(d), null, `${alt} draws the fifteen-destination rail`)
    const n = names(d)
    for (const label of RAIL_LABELS.filter((l) => l !== 'Trace')) assert.ok(!n.includes(label), `${alt} shows the rail's ${label}`)
  }
  const workbench = at('workbench')
  assert.equal(workbench.querySelector('nav[aria-label="' + S.sidebar.label + '"]'), null, 'at Workbench the rail itself is back in its place')
  const inRail = census({ querySelectorAll: (q) => rail(workbench).querySelectorAll(q) }).map((r) => r.name)
  for (const label of RAIL_LABELS) assert.ok(inRail.includes(label), `Workbench's rail lost ${label}`)
})

test('Brief in the workspace: the story alone — no toolbar, filter strip, dock, tree, doc tabs or canvas', () => {
  const d = at('brief')
  assert.equal(d.querySelector('[role="toolbar"]'), null)
  assert.ok(d.querySelector('main.ws-shell-story[data-primary-action]'))
  const n = names(d)
  for (const gone of ['Filter objects', 'Find in trace', 'Graph', 'Findings1 finding', 'Hide left dock', 'Panels ▾', 'Zoom in', 'Focus thread']) {
    assert.ok(!n.includes(gone), `Brief shows ${gone}`)
  }
})

test('Explore in the workspace: ONE slim toolbar with exactly the §3.1 set, and three dock tabs', () => {
  const d = at('explore')
  const bars = d.querySelectorAll('[role="toolbar"]')
  assert.equal(bars.length, 1)
  assert.equal(bars[0].getAttribute('aria-label'), S.explore.toolbar_label)
  const inBar = census({ querySelectorAll: (s) => bars[0].querySelectorAll(s) }).map((r) => r.name)
  assert.deepEqual(inBar, ['Graph', 'Timeline', 'Table', 'Find in trace', S.slots.focus_thread, S.slots.walk, HQ_COPY.trace.scrub.label, 'Fit', 'Zoom out', 'Zoom in', 'Line the claim up against the check', S.explore.story])
  const n = names(d)
  for (const workbench of ['Show findings only', 'Expand', 'Panels ▾', 'Reset layout', 'Filter objects', 'Explain this →', 'Hide left dock']) assert.ok(!n.includes(workbench), `Explore shows Workbench's ${workbench}`)
  const tabs = [...d.querySelectorAll('button')].map((b) => b.textContent).filter((t) => /^(Findings|Events|Checks|Promises|Output|Receipt checks)/.test(t))
  assert.deepEqual(tabs.map((t) => t.match(/^(Findings|Events|Checks|Promises|Output|Receipt checks)/)[1]), ['Findings', 'Events', 'Checks'])
  /* P6-3: the two slots are working buttons now, each saying what it does */
  for (const [label, hint] of [[S.slots.focus_thread, S.slots.focus_thread_hint], [S.slots.walk, S.slots.walk_hint]]) {
    const b = [...bars[0].querySelectorAll('button')].find((x) => x.textContent === label)
    assert.equal(b.getAttribute('aria-disabled'), null, label)
    assert.equal(b.disabled, false, label)
    assert.equal(b.getAttribute('title'), hint, label)
  }
})

test('below BP.sheet the inspector is a bottom sheet across the canvas, not a squeezed column', () => {
  const wide = at('explore').querySelector('aside.ws-evidence-panel')
  assert.ok(wide && !wide.classList.contains('ws-shell-sheet'))
  const narrow = at('explore', { narrow: true, panels: { left: false, right: true, bottom: true, minimap: true } }).querySelector('aside.ws-evidence-panel')
  assert.ok(narrow.classList.contains('ws-shell-sheet'))
  const style = narrow.getAttribute('style')
  assert.match(style, /bottom:\s*0/)
  assert.match(style, /left:\s*0/)
  assert.match(style, /right:\s*0/)
  assert.match(style, /top:\s*auto/, 'the panel rule pins top: 0; the sheet lets go of it, or it hangs from the top')
})

test('the workspace no longer binds F, P and . to its own acts: those keys are the §6 map\'s', () => {
  const src = read('shell.jsx')
  const handle = src.slice(src.indexOf('  handleKey(e) {'), src.indexOf('  /* ── Replay'))
  assert.doesNotMatch(handle, /e\.key === '(f|F|p|P|\.)'/, 'the workspace still answers a §6 key itself')
  assert.match(handle, /e\.key === '\]'/, 'the keys the map does not own are still the workspace\'s')
})

/* ── 5. real key presses (jsdom) ──────────────────────────────────────── */

const DRIVE = `
  import React from 'react';
  import { createRoot } from 'react-dom/client';
  import { flushSync } from 'react-dom';
  import ChangeWorkspace from './change-workspace.jsx';
  import { ALTITUDE_KEY } from './altitude.js';
  export { React, createRoot, flushSync, ChangeWorkspace, ALTITUDE_KEY };
`

function answer(doc) {
  return { ok: true, status: 200, headers: { get: () => null }, text: async () => JSON.stringify(doc) }
}
const ROUTES = {
  '/api/changes/ch1': { change: CHANGE },
  '/api/changes/ch1/proposals': { proposals: null },
  '/api/changes/ch1/coverage': { coverage: COVERAGE },
  '/api/changes/ch1/inputs': {},
  '/api/changes/ch1/applicability': { items: [] },
}

/* No \`act()\`: inside an esbuild bundle React's act cannot reach Node's
 * \`timers\` and falls back to a MessageChannel it never closes, which keeps
 * the test process alive after the last test. The drive renders with
 * \`flushSync\` and lets effects settle on a short timer, as the stage-strip
 * drive in change-workspace.test.js does. */
const settle = () => new Promise((r) => setTimeout(r, 5))

/** A browser, borrowed for one test, with the Change screen mounted in it. */
async function withChange(url, run) {
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url })
  const w = dom.window
  const names = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage', 'fetch']
  const held = names.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of names) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  set('fetch', async (u) => (u in ROUTES ? answer(ROUTES[u]) : { ok: false, status: 404, headers: { get: () => null }, text: async () => '{}' }))
  try {
    const L = await load('drive-' + Math.random().toString(36).slice(2) + '.mjs', DRIVE)
    const root = L.createRoot(w.document.getElementById('root'))
    L.flushSync(() => root.render(L.React.createElement(L.ChangeWorkspace, { id: 'ch1', stage: 'define' })))
    for (let i = 0; i < 40 && !w.document.querySelector('.ws-shell'); i += 1) await settle()
    await settle()
    const press = async (key, target, extra) => {
      (target || w.document.body).dispatchEvent(new w.KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...extra }))
      await settle()
      await settle()
    }
    try {
      return await run({ w, d: w.document, press, L })
    } finally {
      root.unmount()
    }
  } finally {
    for (const [n, d] of held) {
      if (d) Object.defineProperty(globalThis, n, d)
      else delete globalThis[n]
    }
    w.close()
  }
}

test('real keys: a newcomer opens at Brief; E, P and B move and are remembered; typing in a box moves nothing', async () => {
  await withChange('http://127.0.0.1/#/changes/ch1', async ({ w, d, press, L }) => {
    assert.ok(d.querySelector('.ws-shell'), 'the Change screen mounted')
    assert.equal(d.querySelector('.ws-shell').getAttribute('data-altitude'), 'brief')
    assert.equal(w.localStorage.getItem(L.ALTITUDE_KEY), null, 'nothing remembered before anything was chosen')
    assert.equal(d.querySelector('.ws-change-stages'), null)

    await press('2')
    assert.equal(d.querySelector('.ws-shell').getAttribute('data-altitude'), 'explore')
    assert.equal(d.querySelectorAll('.ws-change-stage').length, 6)
    assert.equal(w.localStorage.getItem(L.ALTITUDE_KEY), 'explore')

    /* A box on the page: the name box beside a decision, or any input. */
    const box = d.createElement('input')
    d.body.appendChild(box)
    await press('3', box)
    assert.equal(d.querySelector('.ws-shell').getAttribute('data-altitude'), 'explore', '3 typed into a box does not switch altitude')
    await press('3', d.body, { ctrlKey: true })
    assert.equal(d.querySelector('.ws-shell').getAttribute('data-altitude'), 'explore', 'Ctrl+3 does not switch altitude')

    await press('3')
    assert.equal(d.querySelector('.ws-shell').getAttribute('data-altitude'), 'workbench')
    await press('1')
    assert.equal(d.querySelector('.ws-shell').getAttribute('data-altitude'), 'brief')
    assert.equal(w.localStorage.getItem(L.ALTITUDE_KEY), 'brief')
  })
})

test('real keys: ? opens the key list, / opens Ask, Esc closes; T flips Plain and Technical; F and J say this screen has no picture; . says why the primary is off', async () => {
  await withChange('http://127.0.0.1/#/changes/ch1', async ({ d, press }) => {
    await press('?', d.body, { shiftKey: true })
    assert.equal(d.querySelector('[role="dialog"] h2').textContent, S.keys.title)
    await press('Escape')
    assert.equal(d.querySelector('[role="dialog"]'), null)
    /* P6-4: / opens ASK ITSELF now, not an explanation of what it will be.
     * There is a box to type in, and it is still not a chat: one question at
     * a time, the five it can answer offered first, and no reply, no history,
     * no avatar and nothing pending. */
    await press('/')
    assert.equal(d.querySelector('[role="dialog"] h2').textContent, HQ_COPY.ask.title)
    assert.ok(d.querySelector('[role="dialog"] .ws-ask') !== null, '/ opens Ask')
    assert.ok(d.querySelector('#ws-ask-box') !== null, 'there is one box to type a question into')
    assert.equal(d.querySelectorAll('[role="dialog"] .ws-ask-suggest-btn').length, 5, 'the five questions come before typing')
    assert.ok(d.querySelector('.ws-ask-permanent').textContent === HQ_COPY.ask.permanent, 'the permanent line is on screen before anything is asked')
    await press('Escape')

    const pressed = () => [...d.querySelectorAll(`[aria-label="${S.topbar.register_label}"] button`)].find((b) => b.getAttribute('aria-pressed') === 'true').textContent
    assert.equal(pressed(), 'Plain')
    assert.equal(d.querySelector('details.ws-change-technical').hasAttribute('open'), false)
    await press('t')
    assert.equal(pressed(), 'Technical')
    assert.equal(d.querySelector('details.ws-change-technical').hasAttribute('open'), true, 'Technical opens the technical layer')
    await press('t')
    assert.equal(pressed(), 'Plain')

    await press('f')
    assert.equal(d.querySelector('.ws-shell-note').textContent, S.notes.thread_here_none)
    await press('j')
    assert.equal(d.querySelector('.ws-shell-note').textContent, S.notes.walk_here_none)

    /* The Inspector's one primary on this fixture is Approve, with nobody named: off, with its reason. */
    await press('.')
    const note = d.querySelector('.ws-shell-note').textContent
    const why = d.getElementById(d.querySelector('[data-primary-action] .ws-change-primary').getAttribute('aria-describedby')).textContent.trim()
    assert.ok(note.startsWith(S.notes.primary_off.replace(' {reason}', '')), note)
    assert.ok(note.includes(why), 'the note carries the control\'s own reason and enabler')
  })
})

test('real keys: an old ?altitude=pro address shows Workbench on that page and remembers nothing', async () => {
  await withChange('http://127.0.0.1/?altitude=pro#/changes/ch1', async ({ w, d, L }) => {
    assert.equal(d.querySelector('.ws-shell').getAttribute('data-altitude'), 'workbench')
    assert.equal(w.localStorage.getItem(L.ALTITUDE_KEY), null)
  })
})

/* ── T10: the read time is the time a read COMPLETED ─────────────────── */

test('(t10.3) the top bar shows local HH:MM with the date and zone on hover; without a read, the honest form', () => {
  const on = doc(M.renderTop({ crumbs: [], altitude: 'brief', register: 'manager', onAltitude() {}, onRegister() {}, readWhen: '16:04', readTitle: '2026-09-26 16:04 +00:00' }))
  const span = on.querySelector('.ws-shell-read span')
  assert.equal(span.textContent, S.topbar.read_at.replace('{when}', '16:04'))
  assert.equal(span.getAttribute('title'), '2026-09-26 16:04 +00:00', 'the date and zone are on hover')
  const off = doc(M.renderTop({ crumbs: [], altitude: 'brief', register: 'manager', onAltitude() {}, onRegister() {} }))
  assert.equal(off.querySelector('.ws-shell-read span').textContent, S.topbar.read_unknown)
})

test('(t10.1) the Change screen stamps the time each read completes, and a Refresh advances it', async () => {
  const { readStamp } = await import('./read-stamp.js')
  const realNow = Date.now
  const T1 = Date.UTC(2026, 8, 26, 10, 34, 41)
  const T2 = T1 + 5 * 60 * 1000
  Date.now = () => T1
  try {
    await withChange('http://127.0.0.1/#/changes/ch1', async ({ w, d }) => {
      const span = () => d.querySelector('.ws-shell-read span')
      assert.equal(span().textContent, S.topbar.read_at.replace('{when}', readStamp(T1).text), 'the first completed read is stamped')
      assert.equal(span().getAttribute('title'), readStamp(T1).title)
      Date.now = () => T2
      const refresh = [...d.querySelectorAll('.ws-shell-refresh')].find((b) => b.textContent === S.topbar.refresh)
      refresh.dispatchEvent(new w.MouseEvent('click', { bubbles: true }))
      for (let i = 0; i < 40 && span().getAttribute('title') !== readStamp(T2).title; i += 1) await settle()
      assert.equal(span().getAttribute('title'), readStamp(T2).title, 'the second completed read advances the stamp')
    })
  } finally {
    Date.now = realNow
  }
})

test('(t10.1, t10.2, t10.5) the Trace stamps lastReadAt only where a read completes, and the bar and the strip read that one field', () => {
  const src = read('shell.jsx')
  const at = src.indexOf('  componentDidMount() {')
  const mount = src.slice(at, src.indexOf('\n  }\n', at))
  assert.doesNotMatch(mount, /lastReadAt|readAt|nowUTC\(\)/, 'nothing is stamped when the page opens')
  assert.doesNotMatch(src, /readAt: this\.nowUTC\(\)/, 'no stamp from the clock at the moment a read starts or at mount')
  const body = (name) => { const i = src.indexOf('  async ' + name + '('); return src.slice(i, src.indexOf('\n  }\n', i)) }
  assert.match(body('loadData'), /lastReadAt: Date\.now\(\)/, 'the first load stamps when it completes')
  assert.match(body('openListedSession'), /lastReadAt: Date\.now\(\)/, 'a session switch stamps when it completes')
  assert.equal([...src.matchAll(/lastReadAt: /g)].length, 4, 'initial null, two loadData completions, one switch completion — nowhere else')
  assert.match(src, /readStamp\(st\.lastReadAt\)/, 'the top bar and the status strip render from one field')
  assert.doesNotMatch(src, /'Ledger read ' \+ \(st\.readAt/, 'the strip no longer prints a mount-time guess')
  assert.doesNotMatch(read('change-workspace.jsx'), /readWhen=\{null\}/, 'the Change screen passes the time of its last completed read')
})

/* ── BUILD-10 (P6B-HTT-7 = P6-IR-1, path step 3): the crumb bar's first word ── */

const STAGE_IDS = ['define', 'prepare', 'build', 'verify', 'review', 'approve']
const firstCrumbs = (change) => {
  const out = []
  for (const altitude of ['brief', 'explore', 'workbench']) {
    for (const stage of STAGE_IDS) {
      const d = doc(M.renderShell({ model: changeModel(change), stage, altitude, register: 'manager', onAltitude() {}, onRegister() {}, onReload() {} }))
      const crumbs = [...d.querySelectorAll('.ws-shell-crumb')].map((c) => c.textContent.replace('›', '').trim())
      out.push({ altitude, stage, crumbs })
    }
  }
  return out
}

test('BUILD-10: with a folder recorded, the crumb bar opens with its last segment, at every altitude and on every stage, by the Inbox card\'s own rule', () => {
  const change = { ...CHANGE, implementation: { ...CHANGE.implementation, workspace_identity: '/tmp/fixture/shop' } }
  const project = M.scopeLine(change).project
  assert.equal(project, 'shop')
  for (const { altitude, stage, crumbs } of firstCrumbs(change)) {
    assert.deepEqual(crumbs, [project, CHANGE.request.title, 'version e71c5b0'], altitude + ' ' + stage)
  }
})

test('BUILD-10: with nothing to name the project by, the crumb bar drops the segment, and its first word is never "Project not named in this record"', () => {
  for (const change of [CHANGE, { ...CHANGE, implementation: null }]) {
    for (const { altitude, stage, crumbs } of firstCrumbs(change)) {
      assert.equal(crumbs[0], CHANGE.request.title, altitude + ' ' + stage)
      assert.doesNotMatch(crumbs.join(' | '), /not named in this record/, altitude + ' ' + stage)
      assert.doesNotMatch(crumbs[0], /\bnot\b/i, altitude + ' ' + stage)
    }
  }
  assert.doesNotMatch(JSON.stringify(HQ_COPY), /Project not named in this record/, 'the sentence is gone from the copy file')
})
