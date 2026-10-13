/* C54 Settings: the Read again act, browser permission boundary, and the
 * weekly-summary name. Synthetic served data only; no live machine reads. */
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
const projects = { projects: [{ id: 'shop', name: 'shop', root: '/scratch/shop', sessions: 1 }] }
const storage = { bytes: 1200, path: '/scratch/record' }

function skeleton() {
  const sw = () => ({ checked: 'false', dis: 'false', word: 'Off' })
  return {
    uiK: {}, notes: {}, content: {}, src: { cc: {}, cx: {}, rec: {} },
    sw: { speaker: sw(), pause: sw(), redact: sw(), cvd: sw(), digest: sw() },
    notifs: [{ label: 'a', sw: sw() }], cvd: {}, segDensity: [{}, {}], segMotion: [{}, {}], segKeys: [{}, {}, {}],
    keymap: [{ keyText: 'j / k' }, { keyText: '⌘K' }], permRows: [], tpls: [{}], excl: [{}],
  }
}

function context(actions, stopped = false) {
  const updates = []
  const mkSwitch = (on, fn, blocked) => ({ checked: on ? 'true' : 'false', dis: blocked ? 'true' : 'false', toggle: blocked ? () => {} : fn })
  const D = settingsFromServer({ storage, projects, current: 'shop' })
  return {
    updates,
    ctx: {
      D, s: {}, tech: false, stopped, NOW: '09:41', OFF: { dis: 'true', cur: 'not-allowed' },
      ui: { dis: 'false', cur: 'pointer', pulse: 'p' }, mkSwitch,
      colors: { T1: 'a', T2: 'b', T3: 'c', OR: 'd', CTRL: 'e', BLUE: 'f', HAIR: 'g' },
      setState: (update) => updates.push(update), actions,
    },
  }
}

let modulePromise
function settingsModule() {
  if (!modulePromise) modulePromise = (async () => {
    const source = `import React from 'react'; import { renderToStaticMarkup } from 'react-dom/server'; import Logic from './logic.js'; import Template from './template.jsx'; export { Logic }; export const render = (v) => renderToStaticMarkup(React.createElement(Template, { v, link: (x) => x }));`
    const built = await build({
      stdin: { contents: source, resolveDir: here, loader: 'jsx' },
      bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
      banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
      loader: { '.css': 'empty' }, logLevel: 'silent',
    })
    const path = join(await mkdtemp(join(tmpdir(), 'settings-c54-')), 'module.mjs')
    await writeFile(path, built.outputFiles[0].text)
    return import(pathToFileURL(path).href)
  })()
  return modulePromise
}

test('Read again rereads the watch and selected project capture health, unless HQ is stopped', () => {
  const calls = []
  const actions = { reloadObservation: () => calls.push('observation'), reloadCapture: () => calls.push('capture') }
  const active = context(actions)
  const row = liveOverrides(skeleton(), active.ctx)
  row.src.cx.retry()
  assert.deepEqual(calls, ['observation', 'capture'])
  assert.deepEqual(active.updates, [{ codexTried: true }])

  const stopped = context(actions, true)
  liveOverrides(skeleton(), stopped.ctx).src.cx.retry()
  assert.deepEqual(calls, ['observation', 'capture'])
  assert.deepEqual(stopped.updates, [])
  assert.match(readFileSync(join(here, 'index.jsx'), 'utf8'), /reloadCapture:\s*\(\)\s*=>\s*capture\.reload\(\)/)
})

test('notification switches use the permission-gated writer; other choices keep the ordinary writer', async () => {
  const { Logic } = await settingsModule()
  const raw = [], guarded = []
  const data = settingsFromServer({ storage, projects, current: 'shop' })
  const prefs = { status: 'read', prefs: {
    face: 'plain', density: 'comfortable', motion: 'system', colour_vision: 'standard', keys: 'on',
    masked_copy: true, notifications: { ready: false, not_backed: false, agent_waiting: false, recording_stopped: false }, weekly_summary: false,
  }, pending: {}, writeError: {} }
  const screen = new Logic({
    data, prefs, at: { screen: 'settings' },
    setPreference: (key, value) => { raw.push([key, value]); return Promise.resolve(true) },
    actions: { setPreference: (key, value) => { guarded.push([key, value]); return Promise.resolve(false) } },
  })
  const values = screen.renderVals()
  for (const row of values.notifs) row.sw.toggle()
  assert.deepEqual(guarded, [
    ['notifications.ready', true], ['notifications.not_backed', true],
    ['notifications.agent_waiting', true], ['notifications.recording_stopped', true],
  ])
  assert.deepEqual(raw, [], 'no notification reaches the raw preferences writer')
  values.sw.digest.toggle()
  values.segFace[1].pick()
  assert.deepEqual(raw, [['weekly_summary', true], ['face', 'technical']])
})

test('the weekly switch names the file-backed summary exactly', async () => {
  const { Logic, render } = await settingsModule()
  const data = settingsFromServer({ storage, projects, current: 'shop' })
  const screen = new Logic({ data, at: { screen: 'settings' }, actions: {} })
  const markup = render(screen.renderVals())
  const label = markup.match(/<span id="l-digest"[^>]*>([^<]*)<\/span>/)?.[1]
  assert.equal(label, 'A weekly summary, written to a file')
})
