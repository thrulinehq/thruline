/* Settings previews the committed base policy's named lines. It has no policy
 * save address, so Offer/Stop must never claim a change was kept. */
import test from 'node:test'
import assert from 'node:assert/strict'
import { dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { build } from 'esbuild'
import { settingsFromServer, templatesFromRead } from './from-server.js'
import { liveOverrides } from './live.js'

const here = dirname(fileURLToPath(import.meta.url))
const PROJECTS = { projects: [
  { id: 'shop', name: 'shop', root: '/u/shop', sessions: 3 },
  { id: 'other', name: 'other', root: '/u/other', sessions: 1 },
] }
const policy = (state, templates = [], detail = null, project = 'shop') => ({
  data: { schema_version: 1, project: { id: project }, policy: { path: '.thruline/policy.yml', state, detail }, templates },
  error: null, loading: false,
})
const named = policy('read', [{ name: 'bug_fix', lines: ['Handle empty input.', 'Keep the original bytes.'] }])

function skeleton() {
  const sw = () => ({ checked: 'false', dis: 'false', word: 'Off' })
  return {
    uiK: {}, notes: {}, content: {}, src: { cc: {}, cx: {}, rec: {} },
    sw: { speaker: sw(), pause: sw(), redact: sw(), cvd: sw(), digest: sw() },
    notifs: [{ label: 'a', sw: sw() }], cvd: {}, segDensity: [{}, {}], segMotion: [{}, {}], segKeys: [{}, {}, {}],
    keymap: [{ keyText: 'j / k' }, { keyText: '⌘K' }], permRows: [], tpls: [{}], excl: [{}],
  }
}
function live(read, current = 'shop') {
  const writes = []
  const D = settingsFromServer({ projects: PROJECTS, templates: read, current })
  const OFF = { dis: 'true', cur: 'not-allowed' }
  const mkSwitch = (on, fn, blocked) => ({ checked: on ? 'true' : 'false', dis: blocked ? 'true' : 'false', toggle: blocked ? () => {} : fn })
  const v = liveOverrides(skeleton(), {
    D, s: {}, tech: false, stopped: false, NOW: '09:41', OFF, ui: { dis: 'false', cur: 'pointer', pulse: 'p' }, mkSwitch,
    colors: { T1: 'a', T2: 'b', T3: 'c', OR: 'd', CTRL: 'e', BLUE: 'f', HAIR: 'g' },
    setState: (x) => writes.push(x), actions: {},
  })
  return { D, v, writes }
}

test('the named lines come only from the selected project’s committed template answer', () => {
  const { D, v, writes } = live(named)
  assert.equal(D.templateRead.status, 'read')
  assert.deepEqual(D.criteriaTemplates, [{ key: 'bug_fix', text: 'bug_fix', lines: ['Handle empty input.', 'Keep the original bytes.'] }])
  assert.deepEqual(v.tpls[0].lines, ['Handle empty input.', 'Keep the original bytes.'])
  assert.equal(v.tpls[0].btn, 'Stop offering')
  assert.equal(v.tpls[0].disabled, true)
  assert.equal(v.uiK.tpl.dis, 'true')
  v.tpls[0].toggle()
  assert.deepEqual(writes, [], 'a disabled Offer/Stop cannot change page state or write')
  assert.match(v.content.tplNote, /cannot save changes to the committed base policy/)
  assert.match(v.content.tplNote, /Not yet/)
  assert.match(v.content.tplNote, /base branch this machine reads/)
  assert.match(v.content.policyPreview, /Not yet/)
  assert.doesNotMatch(v.content.policyPreview, /\b[0-9]+ of (your last )?[0-9]+ Changes/)
  const other = live(named, 'other')
  assert.deepEqual(other.D.criteriaTemplates, [], 'a previous project’s lines cannot appear under another project')
  assert.equal(other.v.tpls[0].btn, 'Offer to new Changes')
})

test('empty, absent, unreadable, failed and loading reads have distinct words with no invented lines', () => {
  for (const [read, status, phrase] of [
    [null, 'loading', /Reading this project/],
    [policy('absent'), 'absent', /no \.thruline\/policy\.yml/],
    [policy('read'), 'empty', /holds no named templates/],
    [policy('unreadable', [], 'base branch is ambiguous'), 'unreadable', /could not read.*committed base policy/],
    [{ data: null, error: new Error('offline'), loading: false }, 'failed', /could not read.*just now/],
  ]) {
    const { D, v } = live(read)
    assert.equal(D.templateRead.status, status)
    assert.deepEqual(D.criteriaTemplates, [])
    assert.equal(v.tpls[0].disabled, true)
    assert.equal(v.tpls[0].btn, 'Offer to new Changes')
    assert.match(v.tpls[0].word, phrase)
    assert.deepEqual(v.tpls[0].lines, [])
  }
  assert.equal(templatesFromRead(named, null).status, 'no_project')
  assert.equal(templatesFromRead({ ...named, loading: true }, 'shop').status, 'loading')
  assert.equal(templatesFromRead(policy('read', [{ name: 'bad', lines: [] }]), 'shop').status, 'failed')
})

test('the rendered pane previews every line and the Offer/Stop button is actually disabled', async () => {
  const source = `import React from 'react'; import { renderToStaticMarkup } from 'react-dom/server'; import Template from './template.jsx'; export const render = (v) => renderToStaticMarkup(React.createElement(Template, { v, link: (x) => x }));`
  const built = await build({
    stdin: { contents: source, resolveDir: here, loader: 'jsx' },
    bundle: true, write: false, platform: 'browser', format: 'esm', jsx: 'automatic',
    loader: { '.css': 'empty' }, logLevel: 'silent',
  })
  const module = await import('data:text/javascript;base64,' + Buffer.from(built.outputFiles[0].text).toString('base64'))
  const markup = module.render(live(named).v)
  assert.match(markup, /aria-label="Preview of bug_fix"/)
  assert.match(markup, /<li>Handle empty input\.<\/li>/)
  assert.match(markup, /<li>Keep the original bytes\.<\/li>/)
  assert.match(markup, /disabled=""[^>]*>Stop offering<\/button>/)
})
