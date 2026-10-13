/* C46 §1.2(1): Settings leaves paths out when shown or exported. The server's
 * capture holds are the only source of row state; writes use address 27. */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { build } from 'esbuild'
import { settingsFromServer } from './from-server.js'
import { liveOverrides, EXCLUSIONS_SENTENCE } from './live.js'
import { setExclusion, EXCLUSION_WRITE } from './writes.js'
import { holdsOf } from '../../data/holds.js'

const here = dirname(fileURLToPath(import.meta.url))
const PROJECTS = { projects: [
  { id: 'old', name: 'old', root: '/u/old', sessions: 2, last_activity: '2026-09-01T10:00:00Z' },
  { id: 'shop', name: 'shop', root: '/u/shop', sessions: 9, last_activity: '2026-10-08T09:30:00Z' },
] }
const rows = [
  { key: 'env', covers: '.env and .env.* files', note: 'environment files', built_in: true, on: true, source: 'default', basis: 'on by default (not recorded)' },
  { key: 'secrets', covers: 'secrets/', note: 'the whole folder', built_in: true, on: true, source: 'default', basis: 'on by default (not recorded)' },
  { key: 'generated', covers: 'generated files', note: 'files your repo marks as generated', built_in: true, on: false, source: 'default', basis: 'off by default (not recorded)' },
  { key: 'vendor/legacy', covers: 'vendor/legacy', note: null, built_in: false, on: true, source: 'recorded', since: '2026-10-08T09:40:00Z', by: 'Robin' },
]
const capture = (over = {}) => ({ schema_version: 1, project: { id: 'shop' }, holds: {
  paused: false, honoured: true, exclusions_honoured: true,
  note: 'A path exclusion leaves matching entries out when ThruLine shows or exports a view.',
  exclusions: rows,
  ...over,
} })

function skeleton() {
  const sw = () => ({ checked: 'false', dis: 'false', word: 'Off' })
  return {
    uiK: {}, notes: {}, content: {}, src: { cc: {}, cx: {}, rec: {} },
    sw: { speaker: sw(), pause: sw(), redact: sw(), cvd: sw(), digest: sw() },
    notifs: [{ label: 'a', sw: sw() }], cvd: {}, segDensity: [{}, {}], segMotion: [{}, {}], segKeys: [{}, {}, {}],
    keymap: [{ keyText: 'j / k' }, { keyText: '⌘K' }], permRows: [], tpls: [{}], excl: [{}],
  }
}
function ctx(d, { state = {}, actions = {} } = {}) {
  const set = []
  const mkSwitch = (on, fn, blocked) => ({ checked: on ? 'true' : 'false', dis: blocked ? 'true' : 'false', toggle: blocked ? () => {} : fn })
  return {
    set,
    c: { D: d, s: state, tech: false, stopped: false, NOW: '09:41',
      OFF: { dis: 'true', cur: 'not-allowed' }, ui: { dis: 'false', cur: 'pointer', pulse: 'p' }, mkSwitch,
      colors: { T1: 'a', T2: 'b', T3: 'c', OR: 'd', CTRL: 'e', BLUE: 'f', HAIR: 'g' },
      setState: (p) => set.push(p), actions },
  }
}
const live = (doc = capture(), opts) => liveOverrides(skeleton(), ctx(settingsFromServer({ projects: PROJECTS, capture: doc }), opts).c)

test('the project in front of the person gets the server’s built-ins and typed paths, with their current standing', () => {
  const d = settingsFromServer({ projects: PROJECTS, capture: capture() })
  assert.equal(d.project.id, 'shop')
  assert.equal(d.project.exclusionsHonoured, true)
  assert.deepEqual(d.exclusions.map((e) => [e.key, e.path, e.note, e.on, e.builtIn, e.source]), [
    ['env', '.env  .env.*', 'environment files', true, true, 'default'],
    ['secrets', 'secrets/', 'the whole folder', true, true, 'default'],
    ['generated', 'generated files', 'files your repo marks as generated', false, true, 'default'],
    ['vendor/legacy', 'vendor/legacy', null, true, false, 'recorded'],
  ])
  assert.deepEqual(settingsFromServer({ projects: PROJECTS, capture: null }).exclusions, [])
  assert.deepEqual(settingsFromServer({ projects: PROJECTS, capture: { error: 'failed' } }).exclusions, [])
  const another = settingsFromServer({ projects: PROJECTS, capture: capture(), current: 'old' })
  assert.equal(another.project.holds, 'unread', 'a read for shop must not become old’s policy')
  assert.deepEqual(another.exclusions, [])
})

test('the live pane says precisely what is left out and preserves the sealed-copy boundary', () => {
  const v = live()
  assert.equal(EXCLUSIONS_SENTENCE, "Never show or export these paths. ThruLine's sealed copy of each transcript still holds what Claude Code wrote.")
  assert.equal(v.notes.excl, EXCLUSIONS_SENTENCE)
  assert.equal(v.content.exclTitle, 'Paths left out of views and exports')
  assert.deepEqual(v.excl.map((e) => [e.key, e.checked, e.path]), [
    ['env', 'true', '.env  .env.*'], ['secrets', 'true', 'secrets/'],
    ['generated', 'false', 'generated files'], ['vendor/legacy', 'true', 'vendor/legacy'],
  ])
  assert.equal(v.uiK.addPath.dis, 'false')
  assert.equal(v.uiK.excl.dis, 'false')
  assert.doesNotMatch(v.notes.excl, /never be recorded|What was not recorded/)
})

test('each row asks for the opposite act exactly once, waits for the write, and keeps a refusal in words', async () => {
  const asked = []
  const d = settingsFromServer({ projects: PROJECTS, capture: capture() })
  const answered = holdsOf(capture())
  const x = ctx(d, { actions: { setExclusion: (act, pattern) => {
    asked.push([act, pattern]); return Promise.resolve({ kind: 'kept', holds: answered })
  } } })
  const v = liveOverrides(skeleton(), x.c)
  v.excl[0].toggle()
  v.excl[0].toggle()
  v.excl[2].toggle()
  assert.deepEqual(asked, [['included', 'env']], 'a second press in the same render cannot send another act')
  assert.deepEqual(x.set[0], { exclusionBusy: true, exclusionNote: null, exclusionProject: 'shop' })
  await new Promise((r) => setTimeout(r, 0))
  assert.deepEqual(x.set[1], { exclusionBusy: false, exclusionNote: null, exclusionAnswer: { project: 'shop', from: undefined, holds: answered } })
  for (const [i, want] of [[2, ['excluded', 'generated']], [3, ['included', 'vendor/legacy']]]) {
    const next = ctx(d, { actions: { setExclusion: (act, pattern) => {
      asked.push([act, pattern]); return Promise.resolve({ kind: 'kept', holds: answered })
    } } })
    liveOverrides(skeleton(), next.c).excl[i].toggle()
    assert.deepEqual(asked.at(-1), want)
  }

  const busy = live(capture(), { state: { exclusionBusy: true }, actions: { setExclusion: () => { throw new Error('second press') } } })
  assert.equal(busy.uiK.excl.dis, 'true')
  busy.excl[0].toggle()
  const refused = ctx(d, { actions: { setExclusion: () => Promise.resolve({ kind: 'refused', detail: 'This path is already excluded. Nothing was saved.' }) } })
  liveOverrides(skeleton(), refused.c).excl[2].toggle()
  await new Promise((r) => setTimeout(r, 0))
  assert.deepEqual(refused.set[1], { exclusionBusy: false, exclusionNote: 'This path is already excluded. Nothing was saved.', exclusionAnswer: { project: 'shop', from: undefined, holds: null } })
  assert.match(live(capture(), { state: { exclusionNote: 'This path is already excluded. Nothing was saved.', exclusionProject: 'shop' } }).notes.excl, /Nothing was saved/)
})

test('a kept answer shows the new checkbox state and blocks stale writes until capture is read again', async () => {
  const before = settingsFromServer({ projects: PROJECTS, capture: capture() })
  const afterRows = rows.map((r) => r.key === 'env' ? { ...r, on: false, source: 'recorded' } : r)
  const answer = holdsOf(capture({ exclusions: afterRows }))
  const firstRead = {}
  const acts = []
  const write = (act, pattern) => { acts.push([act, pattern]); return Promise.resolve({ kind: 'kept', holds: answer }) }
  const first = ctx(before, { actions: { captureDoc: firstRead, setExclusion: write } })
  const staleRow = liveOverrides(skeleton(), first.c).excl[0]
  staleRow.toggle()
  await new Promise((r) => setTimeout(r, 0))
  staleRow.toggle()
  assert.deepEqual(acts, [['included', 'env']], 'the old closure cannot resend the stale act')

  const written = first.set.at(-1)
  const waiting = liveOverrides(skeleton(), ctx(before, {
    state: { ...written, exclusionProject: 'shop' },
    actions: { captureDoc: firstRead, setExclusion: write },
  }).c)
  assert.equal(waiting.excl[0].checked, 'false', 'the answer takes effect while capture still holds the old document')
  assert.equal(waiting.uiK.excl.dis, 'true')
  assert.match(waiting.notes.excl, /Reading which paths shop leaves out after that change/)
  waiting.excl[0].toggle()
  assert.deepEqual(acts, [['included', 'env']], 'another act waits for the capture read')

  const nextRead = {}
  const fresh = settingsFromServer({ projects: PROJECTS, capture: capture({ exclusions: afterRows }) })
  const reread = liveOverrides(skeleton(), ctx(fresh, {
    state: { ...written, exclusionProject: 'shop' },
    actions: { captureDoc: nextRead, setExclusion: write },
  }).c)
  assert.equal(reread.excl[0].checked, 'false')
  assert.equal(reread.uiK.excl.dis, 'false')
  reread.excl[0].toggle()
  assert.deepEqual(acts, [['included', 'env'], ['excluded', 'env']], 'the next act follows the completed read')
  const later = liveOverrides(skeleton(), ctx(before, {
    state: { ...written, exclusionProject: 'shop' },
    actions: { captureDoc: nextRead },
  }).c)
  assert.equal(later.excl[0].checked, 'true', 'a later read can supersede the temporary answer')
})

test('Add a path opens one live-only inline editor; Save keeps a typed path, Cancel closes it, refusal leaves the draft', async () => {
  const asked = []
  const d = settingsFromServer({ projects: PROJECTS, capture: capture() })
  const answered = holdsOf(capture({ exclusions: [...rows, { key: 'src/private/**', covers: 'src/private/**', built_in: false, on: true, source: 'recorded' }] }))
  const opening = ctx(d)
  liveOverrides(skeleton(), opening.c).addPath()
  assert.deepEqual(opening.set, [{ pathNote: true, pathProject: 'shop', pathDraft: '', exclusionNote: null }])

  const editing = ctx(d, { state: { pathNote: true, pathProject: 'shop', pathDraft: 'src/private/**' }, actions: { setExclusion: (act, path) => {
    asked.push([act, path]); return Promise.resolve({ kind: 'kept', holds: answered })
  } } })
  const editor = liveOverrides(skeleton(), editing.c).pathEditor
  assert.equal(editor.open, true)
  assert.equal(editor.value, 'src/private/**')
  editor.change('src/private/*')
  assert.deepEqual(editing.set[0], { pathDraft: 'src/private/*' })
  editor.submit({ preventDefault() {} })
  assert.deepEqual(asked, [['excluded', 'src/private/**']])
  await new Promise((r) => setTimeout(r, 0))
  assert.deepEqual(editing.set.slice(-1)[0], { exclusionBusy: false, exclusionNote: null, exclusionAnswer: { project: 'shop', from: undefined, holds: answered }, pathNote: false, pathDraft: '' })
  const cancelling = ctx(d, { state: { pathNote: true, pathProject: 'shop', pathDraft: 'unsaved' } })
  liveOverrides(skeleton(), cancelling.c).pathEditor.cancel()
  assert.deepEqual(cancelling.set.slice(-1)[0], { pathNote: false, pathDraft: '', exclusionNote: null })

  const refused = ctx(d, { state: { pathNote: true, pathProject: 'shop', pathDraft: 'bad' }, actions: { setExclusion: () => Promise.resolve({ kind: 'refused', detail: 'That cannot be left out. Nothing was saved.' }) } })
  liveOverrides(skeleton(), refused.c).pathEditor.submit({ preventDefault() {} })
  await new Promise((r) => setTimeout(r, 0))
  assert.deepEqual(refused.set.slice(-1)[0], { exclusionBusy: false, exclusionNote: 'That cannot be left out. Nothing was saved.', exclusionAnswer: { project: 'shop', from: undefined, holds: null } })
  const said = live(capture(), { state: { pathNote: true, pathProject: 'shop', pathDraft: 'bad', exclusionProject: 'shop', exclusionNote: 'That cannot be left out. Nothing was saved.' } })
  assert.equal(said.pathEditor.value, 'bad')
  assert.match(said.notes.excl, /Nothing was saved/)
  const moved = liveOverrides(skeleton(), ctx(settingsFromServer({ projects: PROJECTS, capture: { ...capture(), project: { id: 'old' } }, current: 'old' }), {
    state: { pathNote: true, pathProject: 'shop', pathDraft: 'bad', exclusionProject: 'shop', exclusionNote: 'That cannot be left out. Nothing was saved.' },
  }).c)
  assert.equal(moved.pathEditor.open, false, 'a draft from shop cannot be submitted for old')
  assert.doesNotMatch(moved.notes.excl, /Nothing was saved/, 'shop’s refusal does not appear beside old')
})

test('unread, failed, missing project, and a server that does not honour exclusions all disable writes and say why', () => {
  const cases = [
    [settingsFromServer({ projects: PROJECTS }), /Reading which paths shop leaves out/],
    [settingsFromServer({ projects: PROJECTS, capture: { error: 'failed' } }), /Which paths shop leaves out could not be read/],
    [settingsFromServer({ projects: { projects: [] } }), /No project is recorded/],
    [settingsFromServer({ projects: PROJECTS, capture: capture({ exclusions_honoured: false }) }), /cannot leave paths out/],
  ]
  for (const [d, reason] of cases) {
    const v = liveOverrides(skeleton(), ctx(d).c)
    assert.equal(v.uiK.excl.dis, 'true')
    assert.equal(v.uiK.addPath.dis, 'true')
    assert.match(v.notes.excl, reason)
    assert.match(v.notes.excl, /sealed copy of each transcript/)
    if (d.project.holds === 'read') assert.equal(v.excl.length, 4, 'known rows stay visible when this binary cannot honour them')
    v.addPath()
  }
  const d = settingsFromServer({ projects: PROJECTS, capture: capture() })
  const stopped = ctx(d, { actions: { setExclusion: () => { throw new Error('HQ is stopped') } } })
  stopped.c.stopped = true
  const v = liveOverrides(skeleton(), stopped.c)
  assert.equal(v.uiK.excl.dis, 'true')
  assert.equal(v.uiK.addPath.dis, 'true')
  v.excl[0].toggle()
  v.addPath()
  assert.deepEqual(stopped.set, [])
})

test('write address 27 sends only the act and path through recordHold/holdAnswerOf', async () => {
  const calls = []
  const f = async (url, init) => {
    calls.push([url, init.method, init.body])
    return { ok: true, status: 200, json: async () => ({ act: 'excluded', hold: { id: 'h1' }, holds: capture().holds }) }
  }
  const kept = await setExclusion('shop', 'excluded', 'vendor/legacy', f)
  assert.equal(EXCLUSION_WRITE('shop'), '/api/hq/projects/shop/capture/exclusions')
  assert.deepEqual(calls, [['/api/hq/projects/shop/capture/exclusions', 'POST', '{"act":"excluded","pattern":"vendor/legacy"}']])
  assert.equal(kept.kind, 'kept')
  assert.equal(kept.holds.exclusionsHonoured, true)
  const blank = await setExclusion('shop', 'excluded', ' ', async () => { throw new Error('must not send') })
  assert.equal(blank.kind, 'refused')
  assert.match(blank.detail, /box is empty/)
  const bad = await setExclusion('shop', 'excluded', '../outside', async () => ({ ok: false, status: 400, json: async () => ({ error: 'bad_pattern', message: 'That path is outside this project. Nothing was saved.' }) }))
  assert.equal(bad.detail, 'That path is outside this project. Nothing was saved.')
  const src = readFileSync(join(here, 'index.jsx'), 'utf8')
  assert.match(src, /setExclusion: \(act, pattern\) => setExclusion\(current, act, pattern\)/)
  assert.match(src, /capture\.reload\(\)/)
})

test('the inline input is live-only; the gallery still has its original checkbox note and heading', async () => {
  const source = `import React from 'react'; import { renderToStaticMarkup } from 'react-dom/server'; import Template from './template.jsx'; export const render = (v) => renderToStaticMarkup(React.createElement(Template, { v, link: (x) => x }));`
  const built = await build({
    stdin: { contents: source, resolveDir: here, loader: 'jsx' },
    bundle: true, write: false, platform: 'browser', format: 'esm', jsx: 'automatic',
    loader: { '.css': 'empty' }, logLevel: 'silent',
  })
  const module = await import('data:text/javascript;base64,' + Buffer.from(built.outputFiles[0].text).toString('base64'))
  const base = { uiK: { addPath: {}, excl: {} }, excl: [], content: {}, notes: { excl: '' }, pn: { agents: { display: 'flex', order: 1 }, privacy: { display: 'none', order: 2 } } }
  const gallery = module.render({ ...base, pathNote: true })
  assert.match(gallery, /Never record these paths/)
  assert.match(gallery, /Adding your own path arrives with path exclusions; nothing was added/)
  assert.doesNotMatch(gallery, /settings-exclusion-path/)
  const live = module.render({ ...base,
    content: { exclTitle: 'Paths left out of views and exports', exclGroup: 'Paths left out of views and exports' },
    notes: { excl: EXCLUSIONS_SENTENCE },
    pathEditor: { open: true, value: 'vendor/legacy', busy: false, change() {}, submit() {}, cancel() {} },
  })
  assert.match(live, /Paths left out of views and exports/)
  assert.match(live, /id="settings-exclusion-path"/)
  assert.match(live, /value="vendor\/legacy"/)
  assert.match(live, /Save path/)
  assert.match(live, /Cancel/)
  assert.doesNotMatch(live, /Adding your own path arrives/)
})
