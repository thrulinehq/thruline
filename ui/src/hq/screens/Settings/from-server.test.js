/* Settings on the record: every fact from a served field, the canvas's errors
 * corrected, no trial. The pause write (write address 26) is proven in
 * settings-pause.test.js; it replaced the Codex-watch write (row 12) in C46 §1.2(1). */
import test from 'node:test'
import assert from 'node:assert/strict'
import { settingsFromServer, planOf, HOOK_EVENTS, DORMANT_EVENTS, SHIM_TOOLS } from './from-server.js'
import { READS } from './writes.js'
import { liveOverrides, NA, PLAN_SENTENCE, PRICING_URL } from './live.js'

const STORAGE = { schema_version: 4, bytes: 5400616896, path: '/store', also: null, also_command_log: null, measured: 'every file', parts: [{ key: 'preserved_transcripts', bytes: 2995322419 }, { key: 'records', bytes: 2405019648 }, { key: 'command_log', bytes: 184320 }, { key: 'export', bytes: 90509 }] }
const LANES = { claude_code: { live: 1, root: '~/.claude/projects' }, codex: { live: 0, root: '~/.codex/sessions' }, gemini: { roadmap: true }, window_minutes: 5 }
const LICENCE_ABSENT = { status: 'absent', plan: null, expires_on: null, paid_gate: true, headline: 'free', entitlements: { approve_version: { allowed: false, reason: 'r', enable: 'e', status: 'absent' } } }
const PROJECTS = { projects: [{ id: 'a', name: 'thruline', root: '/u/thruline', sessions: 539 }, { id: 'b', name: 'empty', root: '/u/empty', sessions: 0 }] }

test('storage says the served total and parts, in a person’s numbers (L1.1)', () => {
  const d = settingsFromServer({ storage: STORAGE })
  assert.equal(d.storage.size, '5.4 GB')
  assert.equal(d.storage.bytes, 5400616896)
  assert.deepEqual(d.storage.parts.map((p) => [p.key, p.size]), [['preserved_transcripts', '3.0 GB'], ['records', '2.4 GB'], ['command_log', '184.3 KB'], ['export', '90.5 KB']])
  assert.equal(d.storage.root, '/store')
})

test('a store that was not read is said to be unread, never 0', () => {
  const d = settingsFromServer({})
  assert.equal(d.storage.size, null)
  assert.equal(d.storage.known, false)
  assert.equal(d.sources.claudeCode.recent, null)
  assert.equal(d.egress.localModel, null)
  assert.equal(d.plan, null)
})

test('the plan is Free or Pro, never a trial (C21-b)', () => {
  assert.equal(planOf(LICENCE_ABSENT), 'free')
  // C30 P0-6: the one plan rule (data/plan.js) reads the server's explicit approving answer
  assert.equal(planOf({ status: 'valid', plan: 'pro', trial: { status: 'active', days_left: 9 }, entitlements: { approve_version: { allowed: true } } }), 'pro')
  assert.equal(planOf({ status: 'expired', plan: 'pro', entitlements: { approve_version: { allowed: false } } }), 'free')
  assert.equal(planOf({ status: 'valid', plan: 'pro' }), null, 'no explicit approving answer: not read as a plan')
  assert.equal(planOf(null), null)
})

test('the canvas’s factual errors are corrected: 7 hook events + 1 dormant, global; 15 shim tools', () => {
  assert.equal(HOOK_EVENTS.length, 7)
  assert.deepEqual(DORMANT_EVENTS, ['StopFailure'])
  assert.equal(SHIM_TOOLS.length, 15)
  const d = settingsFromServer({ lanes: LANES })
  assert.equal(d.sources.claudeCode.hookEvents.count, 7)
  assert.equal(d.sources.claudeCode.hookEvents.scope, 'global')
  assert.equal(d.sources.claudeCode.recent, 1)
  assert.equal(d.sources.codex.recent, 0)
})

test('until the folder permission read answers, Settings does not invent permission from HQ project rows', () => {
  const d = settingsFromServer({ projects: PROJECTS })
  assert.deepEqual(d.checkPermissions.map((p) => [p.project, p.allowed, p.hasRecords]), [['thruline', null, true], ['empty', null, false]])
  assert.deepEqual(d.runPermission, { status: 'loading', folders: [] })
})

test('Codex observation: not configured, watching, paused', () => {
  assert.equal(settingsFromServer({ observation: { enabled: false, state: 'not_configured' } }).sources.codex.status, 'not_watched')
  assert.equal(settingsFromServer({ observation: { enabled: true, state: 'observing' } }).sources.codex.status, 'observing')
  const p = settingsFromServer({ observation: { enabled: true, state: 'paused' } })
  assert.equal(p.sources.codex.status, 'paused')
  // C46 §1.2(1): the project's pause is the record's (`holds`), not the Codex watch's state
  assert.equal(p.project.paused, false)
})

test('every read is a GET address; the write is not among them', () => {
  assert.ok(Object.values(READS).every((u) => u.startsWith('/api/')))
  assert.deepEqual(Object.keys(READS).sort(), ['lanes', 'licence', 'mind', 'observation', 'projects', 'storage'])
})

/* ---- what the live page says ---- */

function skeleton() {
  const sw = () => ({ checked: 'false', dis: 'false', word: 'Off' })
  return {
    uiK: {}, notes: {}, content: {}, src: { cc: {}, cx: {}, rec: {} }, sw: { speaker: sw(), pause: sw(), redact: sw(), cvd: sw(), digest: sw() },
    notifs: [{ label: 'a', sw: sw() }], cvd: {}, segDensity: [{}, {}], segMotion: [{}, {}], segKeys: [{}, {}, {}],
    keymap: [{ keyText: 'j / k' }, { keyText: '⌘K' }], permRows: [], tpls: [{}], excl: [{}],
  }
}
function ctx(d) {
  const OFF = { dis: 'true', cur: 'not-allowed' }
  const mkSwitch = (on, fn, blocked) => ({ checked: on ? 'true' : 'false', dis: blocked ? 'true' : 'false', word: on ? 'On' : 'Off', toggle: blocked ? () => {} : fn })
  return { D: d, s: {}, tech: false, stopped: false, NOW: '09:41', OFF, ui: { dis: 'false', pulse: 'p' }, mkSwitch, colors: { T1: 'a', T2: 'b', T3: 'c', OR: 'd', CTRL: 'e', BLUE: 'f', HAIR: 'g' }, setState: () => {}, actions: {} }
}

test('live: storage headline uses the served total; Remove is disabled with its reason', () => {
  const d = settingsFromServer({ storage: STORAGE, lanes: LANES, licence: LICENCE_ABSENT, projects: PROJECTS })
  const v = liveOverrides(skeleton(), ctx(d))
  assert.equal(v.store.headline, 'ThruLine is keeping 5.4 GB of records on this machine')
  assert.equal(v.store.rows.length, 4)
  assert.equal(v.uiK.remove.dis, 'true')
  assert.match(v.notes.storage, /^Removing is not available in this version/)
})

test('live: Free, the plan sentence and See plans → pricing; no trial anywhere', () => {
  const d = settingsFromServer({ licence: LICENCE_ABSENT })
  const v = liveOverrides(skeleton(), ctx(d))
  assert.equal(v.content.planNav, 'Plan: Free')
  assert.ok(v.content.asideNote.includes(PLAN_SENTENCE))
  assert.equal(PLAN_SENTENCE, 'Approving a version needs Pro. Everything else is free.')
  assert.deepEqual(v.planLink, { href: PRICING_URL, label: 'See plans' })
  assert.equal(PRICING_URL, 'https://thrulinehq.com/pricing')
  assert.doesNotMatch(JSON.stringify(v), /trial/i)
})

test('live: every unbacked control is disabled and says why in plain words', () => {
  const d = settingsFromServer({ storage: STORAGE, lanes: LANES, observation: { enabled: false, state: 'not_configured' }, projects: PROJECTS })
  const v = liveOverrides(skeleton(), ctx(d))
  for (const k of ['perm', 'look', 'tpl', 'remove', 'confirm']) assert.equal(v.uiK[k].dis, 'true', k)
  // C46 §1.2(1) and (2): the pause, exclusions and speaker are backed now (write addresses 26, 27, 20 and 30);
  // until where each stands is read they wait, and say so (settings-pause.test.js, settings-speaker.test.js)
  assert.equal(v.sw.speaker.dis, 'true', 'speaker waits for the install plan')
  assert.equal(v.content.speakerConsent, 'Reading whether ThruLine’s lines are on for thruline…')
  assert.equal(v.sw.pause.dis, 'true', 'pause waits for the record')
  assert.equal(v.notes.pause, 'Reading whether thruline is paused…')
  assert.equal(v.uiK.addPath.dis, 'true', 'Add a path waits for the record')
  assert.equal(v.uiK.excl.dis, 'true', 'the exclusion rows wait for the record')
  assert.match(v.notes.excl, /^Reading which paths thruline leaves out/)
  /* C30 R-PREFS changed this on purpose: the masked copy, colour vision and the weekly summary are the person's stored
   * choices now, not "unbacked". Until the choices have been read they are locked (never drawn as a value nobody read);
   * once read they are live (settings-preferences.test.js). */
  for (const k of ['redact', 'cvd', 'digest']) assert.equal(v.sw[k].dis, 'true', k + ' is locked while the choices are not read')
  assert.match(v.content.tplNote, /^Not yet\. Settings can preview templates/)
  assert.match(v.content.policyPreview, /^Not yet\. This page cannot count/)
  assert.equal(v.where.found, 'Reading this project’s check folders…')
  assert.equal(v.permRows[0].word, 'Reading permissions…')
  assert.ok(!v.notes.notif.includes(NA), 'the notification switches keep a choice now; the note says what they do and do not do')
  assert.deepEqual(v.permRows.map((p) => p.project), ['thruline'])
  assert.match(v.src.cc.meta, /^7 hook events · SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, Stop, SessionEnd, PreCompact · plus StopFailure, wired but not yet observed · installed for every project/)
  assert.match(v.runCap.tools, /the 15 tools it knows/)
  assert.doesNotMatch(JSON.stringify(v), /\bengine\b|\broute\b|\bendpoint\b/i)
})

test('live: with HQ watching Codex, the pause switch is still the project’s recorded pause (row 26), and the Codex row says its own state', () => {
  const asked = []
  const capture = { schema_version: 1, project: { id: 'a' }, holds: { paused: false, honoured: true, note: '', exclusions: [] } }
  const d = settingsFromServer({ observation: { enabled: true, state: 'observing', events_written: 12 }, projects: PROJECTS, capture })
  const c = ctx(d)
  c.actions = { setPause: (a) => { asked.push(a); return Promise.resolve({ kind: 'kept' }) } }
  const v = liveOverrides(skeleton(), c)
  assert.equal(v.sw.pause.dis, 'false')
  v.sw.pause.toggle()
  assert.deepEqual(asked, ['paused'])
  assert.equal(v.content.pauseLabel, 'Pause recording in thruline')
  assert.match(v.src.cx.line, /^12 steps recorded/)
})
