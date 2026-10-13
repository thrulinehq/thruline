/* Settings › Pause recording on the record (C46 §1.2(1); C30 R-ROUTES Settings "Pause"; C34 Q5).
 *
 * The switch pauses or resumes recording in the project in front of the person, through write
 * address 26 (`POST /api/hq/projects/<id>/capture/pause`, data/holds.js), and it reads where the
 * project stands from the record (`holds` on `GET /api/hq/projects/<id>/capture`), never from the
 * page's own state. The six capture callers honour a recorded pause (06613d5b), so the switch is
 * live; the words say what a pause does under the session-start rule, in the server's own sentence
 * (C34 Q5's Settings copy change), and nothing here says "Not available in this version".
 * Proven against stubbed answers; nothing here reaches a server. Synthetic fixtures only. */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { settingsFromServer } from './from-server.js'
import { liveOverrides, NA, PAUSE_SENTENCE } from './live.js'
import { setPause, PAUSE_WRITE } from './writes.js'

const here = dirname(fileURLToPath(import.meta.url))
const repo = join(here, '..', '..', '..', '..', '..')

const PROJECTS = { projects: [
  { id: 'old', name: 'old-notes', root: '/u/old-notes', sessions: 4, last_activity: '2026-09-01T10:00:00Z' },
  { id: 'shop', name: 'shop', root: '/u/shop', sessions: 9, last_activity: '2026-10-08T09:30:00Z' },
] }
const holdsDoc = (over = {}) => ({
  schema_version: 1,
  project: { id: 'shop' },
  holds: Object.assign({
    paused: false, paused_at: null, paused_by: null, honoured: true, exclusions_honoured: true,
    note: 'While paused, ThruLine reads nothing from this folder, and a session started while it was paused is never read. A path exclusion leaves matching entries out when ThruLine shows or exports a view. It does not stop recording, and ThruLine’s sealed copy of each transcript still holds what Claude Code wrote.',
    exclusions: [],
  }, over),
})
const PAUSED_AT = '2026-10-08T09:40:00Z'
const localHM = (iso) => { const d = new Date(iso); return String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0') }

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
const pauseWords = (v) => [v.content.pauseLabel, v.content.pauseSub, v.content.pausedGap, v.notes.pause].filter(Boolean).join(' | ')

test('the switch acts on the project in front of the person, from the record, and says nothing is unavailable', () => {
  const d = settingsFromServer({ projects: PROJECTS, capture: holdsDoc(), current: undefined })
  assert.equal(d.project.id, 'shop', 'the most recently active project, as every screen reads it — not the first row')
  assert.equal(d.project.name, 'shop')
  assert.equal(d.project.paused, false)
  const { c } = ctx(d)
  const v = liveOverrides(skeleton(), c)
  assert.equal(v.sw.pause.dis, 'false', 'the switch is live')
  assert.equal(v.sw.pause.checked, 'false')
  assert.equal(v.s_paused, false)
  assert.equal(v.content.pauseLabel, 'Pause recording in shop')
  assert.equal(v.content.pauseSub, PAUSE_SENTENCE)
  assert.ok(!pauseWords(v).includes(NA), pauseWords(v))
  assert.equal(settingsFromServer({ projects: PROJECTS, capture: holdsDoc(), current: 'old' }).project.id, 'old', '?project= names it')
})

test('the words are the server’s own pause sentence (crates/core capture_hold.rs HONOURED_NOTE), never "nothing is recorded"', () => {
  const src = readFileSync(join(repo, 'crates', 'core', 'src', 'capture_hold.rs'), 'utf8')
  const note = (src.match(/pub const HONOURED_NOTE: &str = "([^"]+)";/) || [])[1]
  assert.ok(note, 'HONOURED_NOTE is where it was')
  assert.ok(note.startsWith(PAUSE_SENTENCE + ' '), 'the Settings sentence is the first sentence the server serves: ' + note)
  assert.doesNotMatch(PAUSE_SENTENCE, /nothing is recorded|Not recorded/i)
})

test('a click pauses: it asks for exactly the act the record does not hold yet, once, and keeps the server’s refusal in words', async () => {
  const asked = []
  let answer = { kind: 'kept', act: 'paused' }
  const d = settingsFromServer({ projects: PROJECTS, capture: holdsDoc() })
  const { c, set } = ctx(d, { actions: { setPause: (act) => { asked.push(act); return Promise.resolve(answer) } } })
  const v = liveOverrides(skeleton(), c)
  v.sw.pause.toggle()
  assert.deepEqual(asked, ['paused'])
  await new Promise((r) => setTimeout(r, 0))
  assert.deepEqual(set, [{ pauseBusy: true, pauseNote: null }, { pauseBusy: false, pauseNote: null }])
  // while a write is out, the switch takes no second press
  const busy = liveOverrides(skeleton(), ctx(d, { state: { pauseBusy: true }, actions: { setPause: (a) => { asked.push(a); return Promise.resolve(answer) } } }).c)
  assert.equal(busy.sw.pause.dis, 'true')
  busy.sw.pause.toggle()
  assert.deepEqual(asked, ['paused'])
  // a refusal is said beside the switch, in the words holds.js gives it
  answer = { kind: 'refused', code: 'already_paused', detail: 'This project is already paused. Nothing was saved.' }
  const r = ctx(d, { actions: { setPause: () => Promise.resolve(answer) } })
  liveOverrides(skeleton(), r.c).sw.pause.toggle()
  await new Promise((res) => setTimeout(res, 0))
  assert.deepEqual(r.set[1], { pauseBusy: false, pauseNote: 'This project is already paused. Nothing was saved.' })
  const said = liveOverrides(skeleton(), ctx(d, { state: { pauseNote: answer.detail } }).c)
  assert.equal(said.notes.pause, answer.detail)
})

test('paused in the record: the switch, the gap, the pill, the status bar and the Claude Code row all say so; a click resumes', () => {
  const asked = []
  const d = settingsFromServer({ projects: PROJECTS, capture: holdsDoc({ paused: true, paused_at: PAUSED_AT, paused_by: 'robin' }) })
  assert.equal(d.project.paused, true)
  const v = liveOverrides(skeleton(), ctx(d, { actions: { setPause: (a) => { asked.push(a); return Promise.resolve({ kind: 'kept' }) } } }).c)
  assert.equal(v.s_paused, true)
  assert.equal(v.sw.pause.checked, 'true')
  assert.equal(v.content.pausedGap, 'Paused by you, ' + localHM(PAUSED_AT) + ' – now')
  assert.equal(v.pill.word, 'Paused by you')
  assert.match(v.statusLeft, /^Recording paused by you in shop · /)
  assert.equal(v.src.cc.word, 'Paused by you')
  assert.match(v.src.cc.sentence, /^ThruLine reads nothing from shop while it is paused/)
  v.sw.pause.toggle()
  assert.deepEqual(asked, ['resumed'])
})

test('where the record could not be read, or there is no project, the switch is off and says why in plain words', () => {
  const failed = liveOverrides(skeleton(), ctx(settingsFromServer({ projects: PROJECTS, capture: { error: 'ThruLine could not check this folder (it answered 500).' } })).c)
  assert.equal(failed.sw.pause.dis, 'true')
  assert.match(failed.notes.pause, /^Whether shop is paused could not be read just now/)
  const reading = liveOverrides(skeleton(), ctx(settingsFromServer({ projects: PROJECTS, capture: null })).c)
  assert.equal(reading.sw.pause.dis, 'true')
  assert.equal(reading.notes.pause, 'Reading whether shop is paused…')
  const none = liveOverrides(skeleton(), ctx(settingsFromServer({ projects: { projects: [] } })).c)
  assert.equal(none.sw.pause.dis, 'true')
  assert.equal(none.notes.pause, 'No project is recorded on this machine yet, so there is nothing to pause.')
  for (const v of [failed, reading, none]) assert.ok(!pauseWords(v).includes(NA), pauseWords(v))
})

test('the write is write address 26 for that project, with exactly {act}; the answer is read by holds.js', async () => {
  const calls = []
  const f = async (url, init) => { calls.push([url, init.method, init.body]); return { ok: true, status: 200, json: async () => ({ act: 'paused', hold: { id: 'h1' }, holds: holdsDoc({ paused: true, paused_at: PAUSED_AT }).holds, codex_observer: 'not_watching' }) } }
  const a = await setPause('shop', 'paused', f)
  assert.deepEqual(calls, [['/api/hq/projects/shop/capture/pause', 'POST', '{"act":"paused"}']])
  assert.equal(PAUSE_WRITE('shop'), '/api/hq/projects/shop/capture/pause')
  assert.equal(a.kind, 'kept')
  assert.equal(a.holds.paused, true)
  const refused = await setPause('shop', 'paused', async () => ({ ok: false, status: 409, json: async () => ({ error: 'already_paused', message: 'This folder is already paused. Nothing was recorded.' }) }))
  assert.deepEqual([refused.kind, refused.detail], ['refused', 'This folder is already paused. Nothing was recorded.'])
  const none = await setPause(null, 'paused', async () => { throw new Error('must not be called') })
  assert.equal(none.kind, 'refused')
})

test('Settings no longer reaches /api/observation (write address 12): the pause is the record’s, and row 26 also pauses the Codex watch', () => {
  const files = ['index.jsx', 'writes.js', 'live.js', 'logic.js', 'from-server.js'].map((f) => readFileSync(join(here, f), 'utf8'))
  for (const src of files) {
    assert.doesNotMatch(src, /setObservation|OBSERVATION_WRITE/, 'the row 12 write is gone')
    assert.doesNotMatch(src, /method:\s*'POST'/, 'Settings sends its writes through data/holds.js and data/preferences.js only')
  }
})
