/* Settings › "Your agent can ask the record": the word beside it says "Installed" only where the
 * install plan read for the project in front of the person (`GET /api/hq/projects/<id>/install`,
 * parts.mcp) says the MCP server is installed (C43 G3: Installed has a record behind it; census
 * honesty:words/settings). It used to be written unconditionally, so a project with no MCP server
 * read "Installed by thruline init". Proven against stubbed answers. Synthetic fixtures only. */
import test from 'node:test'
import assert from 'node:assert/strict'
import { settingsFromServer } from './from-server.js'
import { liveOverrides } from './live.js'

const PROJECTS = { projects: [{ id: 'shop', name: 'shop', root: '/u/shop', sessions: 9, last_activity: '2026-10-08T09:30:00Z' }] }
const part = (state, over = {}) => Object.assign({ state, file: '.mcp.json', level: 'project', adds: [], where: [], refusal: null }, over)
const plan = (mcp) => ({
  schema_version: 1, project: { id: 'shop', root: '/u/shop' }, plan: 'p1',
  parts: { hooks: part('installed', { file: '.claude/settings.local.json', level: 'project-local', where: ['project-local'] }), speaker: part('will_add', { file: '.claude/settings.local.json', level: 'project-local' }), mcp, run_capture: part('will_add') },
  undo: { project: null, user: null, shim: null },
})
function skeleton() {
  const sw = () => ({ checked: 'false', dis: 'false', word: 'Off' })
  return {
    uiK: {}, notes: {}, content: {}, src: { cc: {}, cx: {}, rec: {} }, sw: { speaker: sw(), pause: sw(), redact: sw(), cvd: sw(), digest: sw() },
    notifs: [{ label: 'a', sw: sw() }], cvd: {}, segDensity: [{}, {}], segMotion: [{}, {}], segKeys: [{}, {}, {}],
    keymap: [{ keyText: 'j / k' }, { keyText: '⌘K' }], permRows: [], tpls: [{}], excl: [{}],
  }
}
function word(install) {
  const mkSwitch = (on, fn, blocked) => ({ checked: on ? 'true' : 'false', dis: blocked ? 'true' : 'false', word: on ? 'On' : 'Off', toggle: fn })
  const c = { D: settingsFromServer({ projects: PROJECTS, install }), s: {}, tech: false, stopped: false, NOW: '09:41', OFF: { dis: 'true', cur: 'not-allowed' }, ui: { dis: 'false', pulse: 'p' }, mkSwitch,
    colors: { T1: 'a', T2: 'b', T3: 'c', OR: 'd', CTRL: 'e', BLUE: 'f', HAIR: 'g' }, setState: () => {}, actions: {} }
  return liveOverrides(skeleton(), c).content.mcpWord
}

test('"Installed" is drawn only where the install plan says the MCP server is installed', () => {
  assert.equal(word(plan(part('installed', { where: ['project'] }))), 'Installed by thruline init')
  assert.doesNotMatch(word(plan(part('will_add'))), /\bInstalled\b/)
  assert.doesNotMatch(word(plan(part('refused', { refusal: { code: 'name_taken', plain: 'x' } }))), /\bInstalled\b/)
  assert.doesNotMatch(word(null), /\bInstalled\b/, 'a plan that was not read states nothing')
  assert.doesNotMatch(word({ error: 'x' }), /\bInstalled\b/, 'a plan that failed to read states nothing')
})

test('each state that is not installed says what is known, in plain words', () => {
  assert.equal(word(plan(part('will_add'))), 'Not installed for shop')
  assert.equal(word(plan(part('refused', { refusal: { code: 'name_taken', plain: 'x' } }))), 'Not installed by ThruLine here')
  assert.equal(word(null), 'Not read yet')
  assert.equal(word({ error: 'x' }), 'Could not be read just now')
})
