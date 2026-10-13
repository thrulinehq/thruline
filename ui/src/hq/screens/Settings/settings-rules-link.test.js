/* Settings › Rules › "What counts as done for … ›" (DESIGN-SPEC §3.10 G2; census settings#12).
 *
 * The canvas names a Change in that link and opens its What counts as done. The live page used to
 * say "What counts as done, Change by Change" with no link at all (C30 R-LINKS: a link goes where it
 * says or is no link; no Change was in front of the person, so there was nowhere to go). It now names
 * the project's most recently active piece of work from the project view the page already knows how
 * to read (`GET /api/hq/projects/<id>`, threads newest first), and opens that Change's page. With no
 * such Change read, it is the same no link as before: nothing is invented. Synthetic data only. */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { settingsFromServer } from './from-server.js'
import { liveOverrides } from './live.js'

const here = dirname(fileURLToPath(import.meta.url))
const PROJECTS = { projects: [{ id: 'shop', name: 'shop', root: '/u/shop', sessions: 9, last_activity: '2026-10-08T09:30:00Z' }, { id: 'old', name: 'old', root: '/u/old', sessions: 1 }] }
const T1 = 'aaaaaaaaaaaaaaaa'
const T2 = 'bbbbbbbbbbbbbbbb'
const view = (over = {}) => Object.assign({ schema_version: 1, project: { id: 'shop' }, threads: [{ id: T1, title: 'Fix the discount total.' }, { id: T2, title: 'Older work.' }] }, over)
const GENERIC = 'What counts as done, Change by Change '

function skeleton() {
  const sw = () => ({ checked: 'false', dis: 'false', word: 'Off' })
  return {
    uiK: {}, notes: {}, content: {}, src: { cc: {}, cx: {}, rec: {} }, sw: { speaker: sw(), pause: sw(), redact: sw(), cvd: sw(), digest: sw() },
    notifs: [{ label: 'a', sw: sw() }], cvd: {}, segDensity: [{}, {}], segMotion: [{}, {}], segKeys: [{}, {}, {}],
    keymap: [{ keyText: 'j / k' }, { keyText: '⌘K' }], permRows: [], tpls: [{}], excl: [{}],
  }
}
const live = (input) => {
  const mkSwitch = (on, fn, blocked) => ({ checked: on ? 'true' : 'false', dis: blocked ? 'true' : 'false', word: on ? 'On' : 'Off', toggle: fn })
  const c = { D: settingsFromServer({ projects: PROJECTS, ...input }), s: {}, tech: false, stopped: false, NOW: '09:41', OFF: { dis: 'true', cur: 'not-allowed' }, ui: { dis: 'false', pulse: 'p' }, mkSwitch,
    colors: { T1: 'a', T2: 'b', T3: 'c', OR: 'd', CTRL: 'e', BLUE: 'f', HAIR: 'g' }, setState: () => {}, actions: {} }
  return liveOverrides(skeleton(), c)
}

test('the link names the most recently active Change of the project in front of the person, and opens its page', () => {
  const v = live({ projectView: view(), current: 'shop' })
  assert.equal(v.content.policyExample, 'What counts as done for Fix the discount total. ')
  assert.equal(v.content.policyHref, '#/changes/' + T1 + '/done?project=shop')
})

test('with no Change read, it is the generic words and no link, as before', () => {
  for (const [name, input] of [
    ['not read', {}],
    ['failed', { projectView: { error: 'x' } }],
    ['another project’s view', { projectView: view({ project: { id: 'old' } }) }],
    ['no threads', { projectView: view({ threads: [] }) }],
    ['a thread with no title', { projectView: view({ threads: [{ id: T1, title: '  ' }] }) }],
    ['a thread id that is not a thread id', { projectView: view({ threads: [{ id: 'x/../y', title: 'Fix it' }] }) }],
  ]) {
    const v = live({ ...input, current: 'shop' })
    assert.equal(v.content.policyExample, GENERIC, name)
    assert.equal(v.content.policyHref, undefined, name)
  }
})

let modulePromise
function settingsModule() {
  if (!modulePromise) modulePromise = (async () => {
    const source = `import React from 'react'; import { renderToStaticMarkup } from 'react-dom/server'; import Logic from './logic.js'; import Template from './template.jsx'; export { Logic }; export const render = (v) => renderToStaticMarkup(React.createElement(Template, { v, link: (x) => undefined }));`
    const built = await build({
      stdin: { contents: source, resolveDir: here, loader: 'jsx' },
      bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
      banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
      loader: { '.css': 'empty' }, logLevel: 'silent',
    })
    const path = join(await mkdtemp(join(tmpdir(), 'settings-rules-')), 'module.mjs')
    await writeFile(path, built.outputFiles[0].text)
    return import(pathToFileURL(path).href)
  })()
  return modulePromise
}
const anchorFor = (markup) => (markup.match(/<a [^>]*>\s*What counts as done[^<]*<span/) || [])[0] || null

test('drawn: the named link is a real anchor with that href; without a Change it is the same no-href anchor as before', async () => {
  const { Logic, render } = await settingsModule()
  const draw = (input) => render(new Logic({ data: settingsFromServer({ projects: PROJECTS, ...input }), at: { screen: 'settings', pane: 'rules' }, actions: {} }).renderVals())
  const named = anchorFor(draw({ projectView: view(), current: 'shop' }))
  assert.match(named, /href="#\/changes\/aaaaaaaaaaaaaaaa\/done\?project=shop"/)
  assert.match(named, /What counts as done for Fix the discount total\./)
  const bare = anchorFor(draw({ current: 'shop' }))
  assert.doesNotMatch(bare, /href=/)
  assert.match(bare, /What counts as done, Change by Change/)
})

test('the page reads the project view for the project in front of the person, and hands it to the data', () => {
  const src = readFileSync(join(here, 'index.jsx'), 'utf8')
  assert.match(src, /useResource\(current \? API\.project\(current\) : null\)/)
  assert.match(src, /projectView: projectView\.error \? \{ error: projectView\.error \} : projectView\.data/)
})
