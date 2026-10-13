/* Settings › "Add up to 3 lines when a Claude Code session starts" on the record (C46 §1.2(2);
 * C30 R-ROUTES Settings "Speaker switch: wraps speaker install/uninstall").
 *
 * The switch reads where ThruLine's speaker hooks stand for the project in front of the person from
 * the install plan (`GET /api/hq/projects/<id>/install`, parts.speaker), turns them on through the
 * install route with only the speaker ticked (write address 20), and takes them back through the
 * removal route (write address 30, `POST /api/hq/projects/<id>/uninstall`, {part: "speaker", plan}).
 * Speakers turned on at another level (the user's settings, by `thruline init`) are shown on and
 * never changed here. Nothing here says "Not available in this version".
 * Proven against stubbed answers; nothing here reaches a server. Synthetic fixtures only. */
import test from 'node:test'
import assert from 'node:assert/strict'
import { settingsFromServer } from './from-server.js'
import { liveOverrides, NA } from './live.js'
import { UNINSTALL_ADDRESS, postUninstall, uninstallAnswer } from '../Onboarding/install.js'

const PROJECTS = { projects: [{ id: 'shop', name: 'shop', root: '/u/shop', sessions: 9, last_activity: '2026-10-08T09:30:00Z' }] }
const part = (state, over = {}) => Object.assign({ state, file: '.claude/settings.local.json', level: 'project-local', adds: [], where: [], refusal: null }, over)
const plan = ({ hooks = part('installed', { where: ['project-local'] }), speaker = part('will_add') } = {}) => ({
  schema_version: 1, project: { id: 'shop', root: '/u/shop' }, plan: 'p1',
  parts: { hooks, speaker, mcp: part('will_add', { file: '.mcp.json', level: 'project' }), run_capture: part('will_add') },
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
function ctx(d, { state = {}, actions = {} } = {}) {
  const set = []
  const mkSwitch = (on, fn, blocked) => ({ checked: on ? 'true' : 'false', dis: blocked ? 'true' : 'false', word: on ? 'On' : 'Off', toggle: blocked ? () => {} : fn })
  return {
    set,
    c: { D: d, s: state, tech: false, stopped: false, NOW: '09:41', OFF: { dis: 'true', cur: 'not-allowed' }, ui: { dis: 'false', pulse: 'p' }, mkSwitch,
      colors: { T1: 'a', T2: 'b', T3: 'c', OR: 'd', CTRL: 'e', BLUE: 'f', HAIR: 'g' }, setState: (p) => set.push(p), actions },
  }
}
const live = (install, opts) => liveOverrides(skeleton(), ctx(settingsFromServer({ projects: PROJECTS, install }), opts).c)
const words = (v) => [v.content.speakerConsent, v.content.speakerOn].filter(Boolean).join(' | ')

test('off, with the hooks installed: the switch is live, and a click turns the lines on through the install route', async () => {
  const asked = []
  const v = live(plan(), { actions: { setSpeaker: (on) => { asked.push(on); return Promise.resolve({ kind: 'kept' }) } } })
  assert.equal(v.s_speaker, false)
  assert.equal(v.sw.speaker.dis, 'false')
  assert.equal(v.sw.speaker.checked, 'false')
  assert.equal(v.content.speakerConsent, 'Those lines become part of what your agent sends to its own provider. Off until you agree, one project at a time.')
  v.sw.speaker.toggle()
  assert.deepEqual(asked, [true])
  assert.ok(!words(v).includes(NA), words(v))
})

test('on in this project’s own file: the switch is on, says where, and a click takes the lines back', () => {
  const asked = []
  const v = live(plan({ speaker: part('installed', { where: ['project-local'] }) }), { actions: { setSpeaker: (on) => { asked.push(on); return Promise.resolve({ kind: 'kept' }) } } })
  assert.equal(v.s_speaker, true)
  assert.equal(v.sw.speaker.checked, 'true')
  assert.equal(v.sw.speaker.dis, 'false')
  assert.equal(v.content.speakerOn, 'On for shop from its next session · in .claude/settings.local.json')
  assert.doesNotMatch(v.content.speakerOn, /you agreed at/, 'no consent time is recorded, so none is drawn')
  v.sw.speaker.toggle()
  assert.deepEqual(asked, [false])
})

test('on at another level (thruline init, the user’s settings): shown on, never changed here, and the words say where', () => {
  const v = live(plan({ speaker: part('installed', { where: ['user'] }) }), { actions: { setSpeaker: () => { throw new Error('must not be called') } } })
  assert.equal(v.s_speaker, true)
  assert.equal(v.sw.speaker.checked, 'true')
  assert.equal(v.sw.speaker.dis, 'true')
  assert.equal(v.content.speakerConsent, 'On for every session Claude Code starts in shop, set in your user Claude Code settings, which this page does not change. Turn it off in a terminal: thruline init --undo.')
  v.sw.speaker.toggle()
})

test('without ThruLine’s hooks, refused, unread or failed: the switch waits and says why in plain words', () => {
  const noHooks = live(plan({ hooks: part('will_add') }))
  assert.equal(noHooks.sw.speaker.dis, 'true')
  assert.equal(noHooks.content.speakerConsent, 'ThruLine’s hooks are not installed for shop yet, so there is nothing to add these lines to. Install them on First run, then turn this on.')
  const refused = live(plan({ speaker: part('refused', { refusal: { code: 'split', plain: 'Some speaker hooks are already in the user Claude Code settings, and adding the rest here would split them across two files. Nothing was added.' } }) }))
  assert.equal(refused.sw.speaker.dis, 'true')
  assert.match(refused.content.speakerConsent, /^Some speaker hooks are already in the user Claude Code settings/)
  const unread = liveOverrides(skeleton(), ctx(settingsFromServer({ projects: PROJECTS, install: null })).c)
  assert.equal(unread.sw.speaker.dis, 'true')
  assert.equal(unread.content.speakerConsent, 'Reading whether ThruLine’s lines are on for shop…')
  const failed = liveOverrides(skeleton(), ctx(settingsFromServer({ projects: PROJECTS, install: { error: 'ThruLine could not read what Install would add (it answered 500).' } })).c)
  assert.equal(failed.sw.speaker.dis, 'true')
  assert.equal(failed.content.speakerConsent, 'Whether ThruLine’s lines are on for shop could not be read just now. Reload this page to try again.')
  for (const v of [noHooks, refused, unread, failed]) assert.ok(!words(v).includes(NA), words(v))
})

test('one press at a time, and a refusal is said in place of the consent line', async () => {
  const busy = live(plan(), { state: { speakerBusy: true }, actions: { setSpeaker: () => { throw new Error('must not be called') } } })
  assert.equal(busy.sw.speaker.dis, 'true')
  busy.sw.speaker.toggle()
  const r = ctx(settingsFromServer({ projects: PROJECTS, install: plan() }), { actions: { setSpeaker: () => Promise.resolve({ kind: 'refused', detail: 'Another install is being written for this HQ right now, so nothing changed. Press the switch again in a moment.' }) } })
  liveOverrides(skeleton(), r.c).sw.speaker.toggle()
  await new Promise((res) => setTimeout(res, 0))
  assert.deepEqual(r.set, [{ speakerBusy: true, speakerNote: null }, { speakerBusy: false, speakerNote: 'Another install is being written for this HQ right now, so nothing changed. Press the switch again in a moment.' }])
  const said = live(plan(), { state: { speakerNote: 'X nothing changed.' } })
  assert.equal(said.content.speakerConsent, 'X nothing changed.')
})

test('the removal is write address 30 for that project: the run token, the page’s Origin, exactly {part, plan}', async () => {
  const calls = []
  const f = async (url, init = {}) => {
    calls.push([url, init.method || 'GET', init.headers && init.headers['X-ThruLine-Run-Token'], init.body])
    if (url === '/api/run/token') return { ok: true, status: 200, json: async () => ({ token: 't0k' }) }
    return { ok: true, status: 200, json: async () => ({ changed: true, part: 'speaker', removed: ['SessionStart brief --hook session-start'], plain: 'Took ThruLine’s lines out of .claude/settings.local.json.' }) }
  }
  const res = await postUninstall('shop', 'speaker', 'p1', f)
  assert.equal(UNINSTALL_ADDRESS('shop'), '/api/hq/projects/shop/uninstall')
  assert.deepEqual(calls, [['/api/run/token', 'GET', undefined, undefined], ['/api/hq/projects/shop/uninstall', 'POST', 't0k', '{"part":"speaker","plan":"p1"}']])
  assert.deepEqual(uninstallAnswer(res), { kind: 'kept' })
  // every refusal comes back in words that say nothing changed
  for (const [status, doc, want] of [
    [409, { error: 'elsewhere', plain: 'ThruLine’s lines are turned on in the user Claude Code settings, which this page does not change, so they stay on. Turn them off in a terminal. Nothing changed here.' }, /^ThruLine’s lines are turned on in the user/],
    [409, { error: 'not_installed', plain: 'ThruLine’s lines are not turned on for this project. Nothing changed.' }, /Nothing changed\.$/],
    [409, { error: 'plan_changed', plan: {} }, /changed since this page read it, so nothing changed/],
    [409, { error: 'install_in_progress' }, /nothing changed/],
    [403, { error: 'run_token_required' }, /nothing changed/],
  ]) {
    const a = uninstallAnswer({ ok: false, status, doc })
    assert.equal(a.kind, 'refused')
    assert.match(a.detail, want)
  }
  assert.match(uninstallAnswer({ ok: false, status: 0, doc: { error: 'unreachable' } }).detail, /did not answer/)
})
