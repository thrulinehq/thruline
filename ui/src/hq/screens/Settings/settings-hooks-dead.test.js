/* Settings, census settings#3: hooks that point at a ThruLine program that is no longer on this machine.
 *
 * The capture route says so (`claude_code_hooks.alive === false`, `missing_programs > 0`;
 * API.md "Capture health"), and DESIGN-SPEC §3.10 "System-state bar conditions" (dead) says what the
 * page then draws: the system-state bar "◌ Nothing recorded since HH:MM.", the Claude Code row "Hooks
 * not firing" and the status pill "Not recording". The live page used to hard-code the row as observing,
 * so no bar was ever drawn on the record. Synthetic served data only. */
import test from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { settingsFromServer } from './from-server.js'

const here = dirname(fileURLToPath(import.meta.url))
const PROJECTS = { projects: [{ id: 'shop', name: 'shop', root: '/scratch/shop', sessions: 3 }, { id: 'other', name: 'other', root: '/scratch/other', sessions: 1 }] }
const STORAGE = { bytes: 1200, path: '/scratch/record' }
const LANES = { claude_code: { live: 2, root: '~/.claude/projects' }, codex: { live: 0, root: '~/.codex/sessions' }, window_minutes: 5 }
const LAST_STEP = '2026-10-12T10:41:03.000Z'
const localHM = (iso) => { const d = new Date(iso); return String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0') }

const capture = (id, over = {}) => ({
  schema_version: 1, checked_at: '2026-10-12T11:00:00Z', project: { id, root: '/scratch/' + id },
  claude_code_hooks: Object.assign({
    installed: true, installed_reason: null, alive: true, alive_reason: null, events: ['Stop'], missing_programs: 0,
    levels: [{ scope: 'user', path: '/scratch/.claude/settings.json', state: 'read', reason: null,
      commands: [{ event: 'Stop', command: '/scratch/gone/thruline sensor append Stop', kind: 'recorder', program_exists: true, missing_program: null, this_program: false }] }],
  }, over),
  record: { sessions: { claude_code: 3, codex: 0 }, first_step: { at: '2026-10-01T08:00:00Z', agent: 'claude_code' }, latest_step: { at: LAST_STEP, agent: 'claude_code' },
    through_hooks: { deliveries: 5, first_at: '2026-10-01T08:00:00Z', latest_at: '2026-10-12T10:40:00Z', latest_event: 'Stop' }, reason: null },
  holds: { paused: false, paused_at: null, paused_by: null, honoured: true, exclusions_honoured: true, note: 'n', exclusions: [] },
})
const DEAD = (id = 'shop') => capture(id, {
  alive: false, alive_reason: '1 of 1 ThruLine hook commands name a program that is not on this machine', missing_programs: 1,
  levels: [{ scope: 'user', path: '/scratch/.claude/settings.json', state: 'read', reason: null,
    commands: [{ event: 'Stop', command: '/scratch/gone/thruline sensor append Stop', kind: 'recorder', program_exists: false, missing_program: '/scratch/gone/thruline', this_program: null }] }],
})

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
    const path = join(await mkdtemp(join(tmpdir(), 'settings-dead-')), 'module.mjs')
    await writeFile(path, built.outputFiles[0].text)
    return import(pathToFileURL(path).href)
  })()
  return modulePromise
}
const draw = async (cap, current = 'shop') => {
  const { Logic, render } = await settingsModule()
  const data = settingsFromServer({ storage: STORAGE, lanes: LANES, projects: PROJECTS, capture: cap, current })
  const v = new Logic({ data, at: { screen: 'settings' }, actions: {} }).renderVals()
  return { data, v, markup: render(v) }
}

test('a capture read that says the hooks name a missing program makes the Claude Code row dead, with the last recorded step', () => {
  const d = settingsFromServer({ storage: STORAGE, lanes: LANES, projects: PROJECTS, capture: DEAD(), current: 'shop' })
  assert.equal(d.sources.claudeCode.status, 'hooks_dead')
  assert.equal(d.sources.claudeCode.lastStep, localHM(LAST_STEP), 'the time is the record\'s latest step, as this machine\'s HH:MM')
  const ok = settingsFromServer({ storage: STORAGE, lanes: LANES, projects: PROJECTS, capture: capture('shop'), current: 'shop' })
  assert.equal(ok.sources.claudeCode.status, 'observing', 'every program resolves: nothing is dead')
  assert.equal(settingsFromServer({ storage: STORAGE, lanes: LANES, projects: PROJECTS, current: 'shop' }).sources.claudeCode.status, 'observing', 'no capture read: nothing is said to be dead')
  assert.equal(settingsFromServer({ storage: STORAGE, lanes: LANES, projects: PROJECTS, capture: { error: 'x' }, current: 'shop' }).sources.claudeCode.status, 'observing', 'a failed read is not a dead recorder')
  assert.equal(settingsFromServer({ storage: STORAGE, lanes: LANES, projects: PROJECTS, capture: DEAD('other'), current: 'shop' }).sources.claudeCode.status, 'observing', 'another project\'s read is never this project\'s state')
  const unreadable = capture('shop', { installed: null, alive: null, missing_programs: 0 })
  assert.equal(settingsFromServer({ storage: STORAGE, lanes: LANES, projects: PROJECTS, capture: unreadable, current: 'shop' }).sources.claudeCode.status, 'observing', 'alive: null is unknown, not dead')
})

test('dead hooks draw the system-state bar, the "Hooks not firing" row and the "Not recording" pill (DESIGN-SPEC §3.10)', async () => {
  const { v, markup } = await draw(DEAD())
  const hm = localHM(LAST_STEP)
  assert.equal(v.sys.show, true)
  assert.equal(v.sys.lead, 'Nothing recorded since ' + hm + '.')
  assert.equal(v.sys.cmd, 'thruline init')
  assert.equal(v.sys.label, 'Copy repair command')
  assert.match(markup, /<div role="status"[^>]*>.*Nothing recorded since /)
  assert.equal(v.src.cc.word, 'Hooks not firing')
  assert.equal(v.src.cc.live, false)
  assert.equal(v.pill.word, 'Not recording', 'dead outranks the activity pill, even with recordings written in the last minutes')
  assert.match(v.statusLeft, new RegExp('^Nothing recorded since ' + hm + ' · Claude Code’s hooks aren’t firing$'))
  assert.equal(v.bus.line.startsWith('2px dashed'), true, 'the thread is not lit when nothing is arriving through the hooks')
  assert.doesNotMatch(markup, /Recording now/)
})

test('working hooks draw no system-state bar and the pill says Recording', async () => {
  const { v } = await draw(capture('shop'))
  assert.equal(v.sys.show, false)
  assert.equal(v.src.cc.word, 'Recording now')
  assert.equal(v.pill.word, 'Recording')
})

test('dead hooks with no step on record say "Nothing recorded." and never "since null"', async () => {
  const bare = DEAD()
  bare.record = { sessions: { claude_code: 0, codex: 0 }, first_step: null, latest_step: null, through_hooks: { deliveries: 0, first_at: null, latest_at: null, latest_event: null }, reason: null }
  const { v, markup } = await draw(bare)
  assert.equal(v.sys.lead, 'Nothing recorded.')
  assert.doesNotMatch(markup, /null|undefined/)
})
