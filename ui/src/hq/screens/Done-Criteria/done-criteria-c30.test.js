/* C30 "Every drawn feature works" — Done-Criteria (FEATURE-CENSUS §3.05).
 *
 * One test (or more) per census row this card owns, each asserting the
 * effect on the real record's shape: the screen built from the one Change
 * snapshot (data/change.js) and a tracked Change's documents, its writes
 * caught by stubbed actions. Rows: 5, 6, 8, 9, 10, 11, 13, 15, 16 and the
 * DEFECT "Stage glyphs hard-coded ✓ ● ○ — —". The gallery (no `data`) still
 * draws the canvas; that is asserted here too, and its pixels by pixdiff. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { changeSnapshot } from '../../data/change.js'
import { selectDoneCriteria } from './from-change.js'
import { acceptCriteria, acceptAnswer } from './actions.js'
import { DONE_TEMPLATES, TEMPLATE_WORDS, templatesOf } from './templates.js'

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
export function make(props, state) { const inst = new Logic(props || {}); inst.props = props || {}; if (state) inst.state = { ...inst.state, ...state }; return inst }
export function html(inst) { return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h })) }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'done-criteria-c30-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const html = (inst) => unescape(M.html(inst))
const text = (inst) => html(inst).replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ')

/* A served ThreadView in the API.md shape: one prompt, one edit of src/post.py. */
const T = (m) => new Date(Date.UTC(2026, 9, 6, 12, m, 0)).toISOString()
function thread() {
  const cur = 'wt-2222222222222222'
  return {
    schema_version: 1,
    project: { id: 'p1', name: 'ledger-api', root: '/r/ledger-api' },
    thread: { id: '0123456789abcdef', title: 'Stop the import double-posting', current_version_id: cur, whose_move: 'you', proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: [] } },
    prompts: [{ event_id: 'ev-p', session_id: 's1', seq: 1, ts: T(0), text: 'the import posts twice', work: true, version_id: 'wt-1111111111111111', raw_file: '/m', raw_line: 1 }],
    edits: [{ id: 'ed0', event_id: 'ed0', session_id: 's1', ts: T(2), file: '/r/ledger-api/src/post.py', op: 'edit', lines_added: 1, lines_removed: 0, ok: true, version_id: cur, raw_file: '/m', raw_line: 3 }],
    commands: [], claims: [],
    versions: [{ id: cur, commit: null, short: null, session_id: 's1', from_seq: 1, to_seq: 9, committed: false }],
    current_version_id: cur,
    sessions: [{ id: 's1', ledger_project_id: 'lp', source: 'claude_code', origin: 'person', started_at: T(0), last_at: T(5), file: '/m' }],
    files: [], masking: { masked: 0, families: [], complete: true },
  }
}
const snap = () => changeSnapshot(thread(), null, new Date(Date.UTC(2026, 9, 6, 13, 0)), { storageBytes: 3 * 1024 ** 3 })
const proposed = ({ notes = [{ text: 'make it right' }] } = {}) => ({
  id: 'chg_1',
  change: { accepted_scope: null, code_moved: null },
  proposals: { criteria: [{ number: 1, text: 'Each import row posts once.' }, { number: 2, text: 'Re-running an import posts nothing new.' }], notes, scope: { paths: ['src/**'] } },
  coverage: { criteria: [], candidates: [] },
  model: null,
})
const accepted = () => ({
  id: 'chg_1',
  change: { accepted_scope: { version: 1, allowed_scope: ['src/**'], exclusions: ['refunds'], acceptance: { actor: 'robin', accepted_at: T(8) } }, code_moved: null },
  proposals: null,
  coverage: { criteria: [{ n: 1, items: [] }], candidates: [] },
  model: { criteria: [
    { n: 1, text: 'Each import row posts once.', state: 'not_proven', word: 'Not proven', line: 'Nothing proves this yet.' },
    { n: 2, text: 'Every posted row is logged.', state: 'not_proven', word: 'Not proven', line: 'Nothing proves this yet.' },
  ] },
})
/* the screen's data as index.jsx hands it over */
function live(tracked, { templates = null, track = null, accepting = null, snapshot = snap() } = {}) {
  const d = selectDoneCriteria(snapshot, tracked, { templates })
  d.track = track || { untracked: !tracked, loading: false, busy: false, said: null, note: null }
  d.act.accepting = accepting || { busy: false, said: null, version: null }
  return d
}
const NOT_AVAILABLE = /not available/i

test('DEFECT stage glyphs: the strip draws the Change’s own stages, the gallery the canvas’s', () => {
  const s = snap()
  s.change.stages = [
    { id: 'define', status: 'done' }, { id: 'prepare', status: 'done' }, { id: 'build', status: 'done' },
    { id: 'verify', status: 'done' }, { id: 'review', status: 'current' }, { id: 'approve', status: 'not_started' },
  ]
  const out = html(M.make({ data: live(proposed(), { snapshot: s }) }))
  const glyph = (name) => (out.match(new RegExp('<span aria-hidden="true">([^<]*)</span>' + name)) || [])[1]
  assert.deepEqual(['Prepare', 'Build', 'Verify', 'Review', 'Approve'].map(glyph), ['✓', '✓', '✓', '●', '○'])
  /* the model's own reading of an ordinary Change: built, not verified */
  const plain = html(M.make({ data: live(proposed()) }))
  const g2 = (name) => (plain.match(new RegExp('<span aria-hidden="true">([^<]*)</span>' + name)) || [])[1]
  assert.deepEqual(['Prepare', 'Build', 'Verify', 'Review', 'Approve'].map(g2), ['✓', '●', '○', '—', '—'])
  /* a stage the model does not name is never drawn as reached */
  const none = snap(); none.change.stages = []
  const blank = html(M.make({ data: live(proposed(), { snapshot: none }) }))
  assert.ok(!/<span aria-hidden="true">[✓●]<\/span>(Prepare|Build|Verify|Review|Approve)/.test(blank), 'a glyph with no stage behind it')
  const gallery = html(M.make({}))
  const g3 = (name) => (gallery.match(new RegExp('<span aria-hidden="true">([^<]*)</span>' + name)) || [])[1]
  assert.deepEqual(['Prepare', 'Build', 'Verify', 'Review', 'Approve'].map(g3), ['✓', '●', '○', '—', '—'])
})

test('row 5 / R-UNTRACKED: untracked work offers “Track this work as a Change”, never a draft to accept', () => {
  const calls = []
  const inst = M.make({ data: live(null), actions: { track: () => calls.push('track'), accept: () => calls.push('accept') } })
  const v = inst.renderVals()
  assert.equal(v.track.offered, true)
  assert.equal(v.track.label, 'Track this work as a Change')
  assert.equal(v.showAcceptBtn, false)
  assert.equal(v.showAcceptDisabled, false, 'no Accept button stands beside the act that is offered')
  assert.equal(v.addOff, true, 'nothing can be added to a Change that does not exist')
  assert.match(v.addNote, /Track this work as a Change first/)
  const out = text(inst)
  assert.ok(out.includes('Track this work as a Change'), out)
  assert.ok(out.includes('This work is not tracked as a Change yet, so there is nothing to accept.'))
  assert.ok(out.includes('It accepts and approves nothing.'))
  assert.ok(!NOT_AVAILABLE.test(out), out.match(NOT_AVAILABLE))
  v.track.act()
  assert.deepEqual(calls, ['track'])
  /* pressed: busy, and a second press goes nowhere */
  const busy = M.make({ data: live(null, { track: { untracked: true, busy: true } }), actions: { track: () => calls.push('again') } })
  const vb = busy.renderVals()
  assert.equal(vb.track.label, 'Tracking this work…')
  vb.track.act()
  assert.deepEqual(calls, ['track'])
  /* refused: the client's own sentence, and nothing else */
  const said = 'ThruLine could not track this work, so nothing was written. Reload this page and try again.'
  assert.equal(M.make({ data: live(null, { track: { untracked: true, said } }) }).renderVals().barLine1, said)
  /* still reading whether it is tracked: nothing is offered yet */
  const reading = M.make({ data: live(null, { track: { untracked: false, loading: true } }) }).renderVals()
  assert.equal(reading.track.offered, false)
  assert.equal(reading.showAcceptDisabled, false)
  assert.match(reading.barLine1, /^Reading whether this work is tracked/)
})

test('rows 8 and 10: Add a statement adds a proposed line in the person’s words; it is accepted only by the accept act', () => {
  const sent = []
  const inst = M.make({ data: live(proposed()), actions: { accept: (d) => { sent.push(d); return Promise.resolve({ ok: true }) } } })
  let v = inst.renderVals()
  assert.equal(v.addOff, undefined, 'the opener is on for a tracked Change')
  assert.ok(!NOT_AVAILABLE.test(v.addNote + v.composerWhy + v.tplWhy))
  v.openAdd()
  inst.setState({ composerText: '  The importer logs each row it posts.  ' })
  v = inst.renderVals()
  assert.equal(v.composerOpen, true)
  assert.equal(v.composerCanAdd, true)
  assert.match(v.composerWhy, /arrives below as a proposal/)
  v.composerSubmit()
  v = inst.renderVals()
  assert.equal(v.composerOpen, false)
  assert.equal(v.cards.length, 3)
  assert.equal(v.cards[2].text, 'The importer logs each row it posts.')
  assert.equal(v.cards[2].stateWord, 'Proposed', 'adding a line is not accepting it')
  assert.match(v.cards[2].source, /your words · added here/)
  assert.equal(v.showAcceptBtn, false, 'nothing is accepted for the person')
  assert.match(v.counter, /0 of 3 accepted · 3 proposed/)
  /* accepted by the person, then the one act: proposals by number, their own line by its words */
  v.cards[0].accept()
  v = inst.renderVals()
  v.cards[2].accept()
  v = inst.renderVals()
  assert.equal(v.showAcceptBtn, true)
  v.recordV()
  assert.deepEqual(sent, [{ accepted: [1], edits: {}, folders: ['src/**'], replacing: false, lines: ['The importer logs each row it posts.'] }])
})

test('row 8: the composer refuses what the accept act would', () => {
  const inst = M.make({ data: live(proposed()) }, { composer: 'line' })
  const say = (t) => { inst.setState({ composerText: t }); return inst.renderVals() }
  let v = say('')
  assert.equal(v.composerCanAdd, false)
  assert.match(v.composerWhy, /Say one thing a check could observe/)
  v = say('x'.repeat(241))
  assert.equal(v.composerCanAdd, false)
  assert.match(v.composerWhy, /longer than 240 characters/)
  v = say('each import row posts once')
  assert.equal(v.composerCanAdd, false)
  assert.match(v.composerWhy, /already in the list/)
  const out = text(inst)
  assert.ok(out.includes('Add as proposed'))
  assert.ok(!/needs engine|not available/i.test(out))
})

test('row 11: Add to out of scope keeps it in the draft, recorded only with the version; the record’s own are shown as recorded', () => {
  const sent = []
  const inst = M.make({ data: live(proposed()), actions: { accept: (d) => { sent.push(d); return Promise.resolve({ ok: false }) } } })
  let v = inst.renderVals()
  v.openScope()
  inst.setState({ composerText: 'refunds' })
  v = inst.renderVals()
  assert.equal(v.composerAdd, 'Add to out of scope')
  assert.equal(v.composerCanAdd, true)
  assert.match(v.composerWhy, /Recorded as out of scope with v1/)
  v.composerSubmit()
  v = inst.renderVals()
  assert.deepEqual(v.outItems.map((o) => [o.text, o.removable]), [['refunds', true]])
  assert.equal(v.outEmpty, '')
  assert.ok(text(inst).includes('refunds'))
  v.cards[0].accept()
  inst.renderVals().recordV()
  assert.deepEqual(sent[0].exclusions, ['refunds'])
  /* taken back before accepting: gone from the draft */
  inst.renderVals().outItems[0].remove()
  assert.deepEqual(inst.renderVals().outItems, [])
  assert.equal(inst.renderVals().outEmpty, 'Nothing yet.')
  /* the standing version's out-of-scope list is the record's, and cannot be removed here */
  const rec = M.make({ data: live(accepted()) }).renderVals()
  assert.deepEqual(rec.outItems.map((o) => [o.text, o.removable]), [['refunds', false]])
  /* a duplicate is not added twice */
  const dup = M.make({ data: live(accepted()) }, { reopen: true, composer: 'scope', composerText: 'Refunds' }).renderVals()
  assert.equal(dup.composerCanAdd, false)
  assert.match(dup.composerWhy, /already out of scope/)
})

test('rows 9 and 13: Start from a template reads the policy file and says what it holds, in plain words', () => {
  const doc = (state, reason, extra = {}) => ({ data: { schema_version: 1, project: { id: 'p1' }, policy: { path: '.thruline/policy.yml', state, detail: null, ...extra }, templates: [], reason } })
  const words = (templates, face) => M.make({ data: live(proposed(), { templates }) }, { tplOpen: true, face: face || 'plain' }).renderVals().tplWhy
  assert.equal(words(doc('absent', 'no_policy_file')), TEMPLATE_WORDS.absent)
  assert.match(TEMPLATE_WORDS.absent, /no \.thruline\/policy\.yml.*thruline policy --init/)
  assert.equal(words(doc('read', 'no_templates_section')), TEMPLATE_WORDS.none)
  assert.equal(words(doc('unreadable', 'policy_not_read', { detail: 'line 2: unknown key `templates`' })), TEMPLATE_WORDS.notRead)
  assert.ok(words(doc('unreadable', 'policy_not_read', { detail: 'line 2: unknown key `templates`' }), 'technical').endsWith('line 2: unknown key `templates`'))
  assert.equal(words(null), TEMPLATE_WORDS.reading)
  assert.equal(words({ data: null, error: new Error('HTTP 503') }), TEMPLATE_WORDS.failed)
  for (const w of Object.values(TEMPLATE_WORDS)) assert.ok(!NOT_AVAILABLE.test(w) && !/engine|\broute\b|endpoint/i.test(w), w)
  assert.equal(DONE_TEMPLATES('p 1'), '/api/hq/projects/p%201/done-templates')
  /* the panel is open and drawn for a tracked Change, with its link to Rules (row 13) */
  const out = html(M.make({ data: live(proposed(), { templates: doc('absent', 'no_policy_file') }) }, { tplOpen: true }))
  assert.ok(out.includes('Templates from Rules · this repository'))
  assert.ok(out.includes('href="Settings.dc.html#rules"'))
  assert.ok(unescape(out).includes(TEMPLATE_WORDS.absent))
  assert.ok(!out.includes('needs engine'))
})

test('row 9: picking a template adds its lines as proposals, never accepted, never twice', () => {
  const templates = { data: { policy: { state: 'read' }, reason: null, templates: [{ name: 'Bug fix', lines: ['The bug no longer reproduces.', 'Each import row posts once.', '  '] }] } }
  assert.deepEqual(templatesOf(templates).templates, [{ name: 'Bug fix', from: '.thruline/policy.yml', lines: ['The bug no longer reproduces.', 'Each import row posts once.'] }])
  const inst = M.make({ data: live(proposed(), { templates }) }, { tplOpen: true })
  let v = inst.renderVals()
  assert.equal(v.tplWhy, TEMPLATE_WORDS.some)
  assert.equal(v.templates.length, 1)
  assert.equal(v.templates[0].note, 'from .thruline/policy.yml · 2 lines')
  assert.ok(html(inst).includes('aria-label="Add the lines of the template “Bug fix” as proposals"'), 'each row is a button live')
  v.templates[0].pick()
  v = inst.renderVals()
  assert.equal(v.tplOpen, false)
  assert.deepEqual(v.cards.map((c) => [c.text, c.stateWord]), [
    ['Each import row posts once.', 'Proposed'], ['Re-running an import posts nothing new.', 'Proposed'],
    ['The bug no longer reproduces.', 'Proposed'],
  ], 'a line already in the list is not added again')
  assert.match(v.cards[2].source, /from the template “Bug fix”/)
  assert.equal(v.showAcceptBtn, false)
})

test('row 5: a drop is said as what it is — it stays out of the version; the record keeps no row for it', () => {
  const sent = []
  const inst = M.make({ data: live(proposed()), actions: { accept: (d) => { sent.push(d); return Promise.resolve({ ok: false }) } } }, { lines: ['accepted', 'dropped'] })
  const v = inst.renderVals()
  assert.equal(v.cards[1].stateWord, 'Dropped · stays out of v1')
  assert.match(v.barLine1, /1 dropped, stays out of v1/)
  const out = text(inst)
  assert.ok(!/recorded with v1|recorded in v1/.test(out.replace('recorded with who and when', '')), out.match(/.{30}recorded (with|in) v1.{20}/))
  v.recordV()
  assert.deepEqual(sent[0].accepted, [1], 'the dropped proposal is not named')
  /* the gallery keeps the canvas's words */
  const gallery = M.make({}, { lines: ['accepted', 'accepted', 'dropped', 'accepted'] }).renderVals()
  assert.equal(gallery.cards[2].stateWord, 'Dropped · recorded with v1')
})

test('row 15: the folders are recorded as kept; their steps are read for the served `src/**` form; a version being changed keeps them', () => {
  const sent = []
  const inst = M.make({ data: live(proposed()), actions: { accept: (d) => { sent.push(d); return Promise.resolve({ ok: false }) } } })
  let v = inst.renderVals()
  assert.deepEqual(v.folders.map((f) => [f.path, f.note]), [['src/**', '1 edit · step 2']])
  v.folders[0].toggle()
  v = inst.renderVals()
  assert.equal(v.folders[0].offEdit, true)
  v.cards[0].accept()
  inst.renderVals().recordV()
  assert.deepEqual(sent[0].folders, [], 'the person’s own list: here, nothing')
  /* "Change it": the standing version's folders are kept, with no ✕ that the accept act would undo */
  const re = M.make({ data: live(accepted()) })
  re.renderVals().changeIt()
  v = re.renderVals()
  assert.deepEqual(v.folders.map((f) => [f.onEdit, f.onLocked]), [[false, true]])
  assert.equal(v.coversNote, 'v1 covers these folders, and v2 keeps every one of them.')
  assert.ok(!html(re).includes('aria-label="Remove src/** from what this Change covers"'))
})

test('row 15 / R-UNTRACKED: tracked on this screen, the Change’s own proposed folders are kept, not dropped for want of a click', () => {
  const sent = []
  /* first drawn untracked (the recorded edits' top folder, `src/`), then the
     tracked Change's documents arrive with the proposed `src/**` */
  const inst = M.make({ data: live(null) })
  inst.renderVals()
  inst.props = { data: live(proposed()), actions: { accept: (d) => { sent.push(d); return Promise.resolve({ ok: false }) } } }
  const v = inst.renderVals()
  assert.deepEqual(v.folders.map((f) => [f.path, f.onEdit, f.offEdit]), [['src/**', true, false]])
  assert.match(v.ruleSentence, /anywhere in src\/\*\* reopens every line/)
  v.cards[0].accept()
  inst.renderVals().recordV()
  assert.deepEqual(sent[0].folders, ['src/**'])
  /* a recorded version draws what it covers, whatever the screen held before */
  inst.props = { data: live(accepted()) }
  assert.deepEqual(inst.renderVals().folders.map((f) => [f.path, f.onLocked]), [['src/**', true]])
})

test('row 5: changing a recorded version keeps its lines by their words, never by a proposal number', () => {
  const sent = []
  const inst = M.make({ data: live(accepted()), actions: { accept: (d) => { sent.push(d); return Promise.resolve({ ok: false }) } } })
  inst.renderVals().changeIt()
  let v = inst.renderVals()
  assert.equal(v.canChange, true)
  v.cards[1].drop()
  v = inst.renderVals()
  v.recordV()
  assert.deepEqual(sent, [{ accepted: [], edits: {}, folders: ['src/**'], replacing: true, lines: ['Each import row posts once.'] }])
})

test('the notes grid: what the Change covers and out of scope are drawn when no note was kept', () => {
  const out = text(M.make({ data: live(proposed({ notes: [] })) }))
  assert.ok(out.includes('What the Change covers'))
  assert.ok(out.includes('Out of scope'))
  assert.ok(out.includes('Nothing yet.'))
  assert.ok(!out.includes('Not checkable as written'))
  assert.ok(text(M.make({ data: live(proposed()) })).includes('Not checkable as written'))
})

test('row 16: Accept shows its answer — the version it wrote, or the refusal in the server’s words', async () => {
  let resolve
  const inst = M.make({ data: live(proposed()), actions: { accept: () => new Promise((r) => { resolve = r }) } })
  inst.renderVals().cards[0].accept()
  inst.setState({ added: [{ text: 'The importer logs each row it posts.', state: 'accepted' }] })
  inst.renderVals().recordV()
  resolve({ ok: true })
  await new Promise((r) => setTimeout(r, 0))
  assert.deepEqual(inst.state.added, [], 'the draft is cleared once the version is written')
  /* busy */
  const busy = M.make({ data: live(proposed(), { accepting: { busy: true } }), actions: { accept: () => assert.fail('a second press while the first is answered') } }, { lines: ['accepted', 'proposed'] })
  assert.equal(busy.renderVals().barLine1, 'Recording what counts as done (v1)…')
  busy.renderVals().recordV()
  /* refused: the server's sentence on the bar, whole in its title */
  const said = 'This would drop 1 line you already accepted, or replace one with something that says less. Nothing was saved. Say plainly that you are replacing them, and it will be.'
  const refused = M.make({ data: live(proposed(), { accepting: { said } }) }, { lines: ['accepted', 'proposed'] })
  assert.equal(refused.renderVals().barLine1, said)
  assert.ok(html(refused).includes('title="' + said + '"'))
  /* written, not yet read back; then read back */
  assert.equal(M.make({ data: live(proposed(), { accepting: { version: 1 } }) }).renderVals().barLine1, 'Recorded as v1. Reading it back…')
  const back = M.make({ data: live(accepted(), { accepting: { version: 1 } }) }).renderVals()
  assert.match(back.barLine1, /^What counts as done v1 · accepted .* by robin · 2 lines$/)
})

test('row 16: the answer, in words, from every shape the route answers with', () => {
  assert.deepEqual(acceptAnswer({ ok: true, status: 200, doc: { accepted: { version: 3 } } }), { ok: true, version: 3, said: null })
  assert.equal(acceptAnswer({ ok: false, status: 400, doc: { error: 'refused', detail: 'This is already what counts as done, version 2. There is nothing new to accept.' } }).said, 'This is already what counts as done, version 2. There is nothing new to accept.')
  assert.equal(acceptAnswer({ ok: false, status: 409, doc: { error: 'too_long', part: 'lines', index: 0, limit: 240, plain: 'One line is longer than 240 characters. Nothing was saved.' } }).said, 'One line is longer than 240 characters. Nothing was saved.')
  assert.match(acceptAnswer({ ok: false, status: 0, doc: { error: 'unreachable' } }).said, /did not answer, so nothing was recorded/)
  assert.match(acceptAnswer({ ok: false, status: 503, doc: { error: 'store_unavailable', detail: 'schema upgrade pending' } }).said, /could not open the record.*schema upgrade pending/)
  assert.match(acceptAnswer({ ok: false, status: 400, doc: { error: 'malformed_request' } }).said, /nothing was recorded/)
  assert.match(acceptAnswer({ ok: true, status: 200, doc: {} }).said, /did not say which version/)
})

test('row 16 / scope/accept: the person’s lines and out-of-scope items reach the one write; only their own lines are enough', async () => {
  const calls = []
  const f = async (url, init = {}) => { calls.push({ url, body: JSON.parse(init.body) }); return { ok: true, status: 200, json: async () => ({ accepted: { version: 1 } }) } }
  const r = await acceptCriteria({ changeId: 'chg_1', accepted: [2], lines: [' Mine. ', ''], exclusions: ['refunds', ' '], folders: ['src/**'] }, f)
  assert.equal(r.ok, true)
  await acceptCriteria({ changeId: 'chg_1', accepted: [], lines: ['Only mine.'] }, f)
  assert.deepEqual(calls.map((c) => c.body), [
    { criteria: [2], actor: 'you', lines: ['Mine.'], exclusions: ['refunds'], allowed_scope: ['src/**'] },
    { actor: 'you', lines: ['Only mine.'] },
  ])
  assert.ok(calls.every((c) => c.url === '/api/changes/chg_1/scope/accept'))
  const none = await acceptCriteria({ changeId: 'chg_1', accepted: [], lines: [' '] }, f)
  assert.equal(none.ok, false)
  assert.equal(calls.length, 2, 'nothing to accept writes nothing')
})

test('the live screen never says “Not available”, a design note or an engine word, in any state this card adds', () => {
  const states = [
    [live(null), {}], [live(null, { track: { untracked: true, busy: true } }), {}],
    [live(proposed()), { composer: 'line', composerText: 'x'.repeat(241) }], [live(proposed()), { composer: 'scope' }],
    [live(proposed()), { tplOpen: true }], [live(proposed({ notes: [] })), { added: [{ text: 'Mine.', state: 'dropped' }] }],
    [live(accepted()), { reopen: true }], [live(accepted()), { tplOpen: true, face: 'technical' }],
  ]
  for (const [data, st] of states) {
    const out = text(M.make({ data }, st))
    const tag = JSON.stringify(st)
    assert.ok(!NOT_AVAILABLE.test(out), tag + ': ' + out.match(/.{40}not available.{40}/i))
    assert.ok(!/needs engine|Design note|Demo state|\bengine\b|\btrial\b/i.test(out), tag)
    assert.ok(!/\b(undefined|null|NaN)\b/.test(out), tag + ': ' + out.match(/.{30}\b(undefined|null|NaN)\b.{20}/))
  }
})

test('census: Done-Criteria reaches the accept and link writes through actions.js, Track through Repair-Loop’s one client, and reads its templates with a GET', () => {
  const index = readFileSync(join(here, 'index.jsx'), 'utf8')
  assert.match(index, /import \{ trackThenReread \} from '\.\.\/Repair-Loop\/actions\.js'/)
  assert.match(index, /useResource\(projectId \? DONE_TEMPLATES\(projectId\) : null\)/)
  for (const n of ['index.jsx', 'templates.js', 'from-change.js', 'logic.js']) {
    assert.ok(!/method:\s*['"]POST['"]|fetch\(/.test(readFileSync(join(here, n), 'utf8')), n + ' must not write')
  }
})
