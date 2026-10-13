/* Settings permissions belong to exact recorded folders. These synthetic
 * answers never touch a live store or start a check. */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { settingsFromServer } from './from-server.js'
import { liveOverrides } from './live.js'
import { PERMISSIONS_READ, permissionWrite, permissionAnswer, writeThenRead } from './run-permissions.js'

const PROJECTS = { projects: [
  { id: 'shop', name: 'shop', root: '/u/shop', sessions: 4, last_activity: '2026-10-08T12:00:00Z' },
  { id: 'other', name: 'other', root: '/u/other', sessions: 2, last_activity: '2026-10-07T12:00:00Z' },
] }
const A = 'a'.repeat(64), B = 'b'.repeat(64), OLD = 'c'.repeat(64), MISSING = 'd'.repeat(64), NEW = 'e'.repeat(64)
const ROWS = { schema_version: 1, project: 'shop', folders: [
  { id: A, folder: '~/shop/a', state: 'given', actor: 'Sam', at: '2026-10-08T09:40:00Z', change_id: 'change-a', latest_row: 'row-a', action: 'revoke', historical: false },
  { id: B, folder: '~/shop/b', state: 'revoked', actor: null, at: null, change_id: 'change-b', latest_row: 'row-b', action: 'grant', historical: false },
  { id: OLD, folder: '~/shop/old', state: 'given', actor: 'Lee', at: '2026-10-07T11:00:00Z', change_id: null, latest_row: 'row-old', action: 'revoke', historical: true },
  { id: MISSING, folder: '~/shop/missing', state: 'file_missing', actor: null, at: null, change_id: 'change-m', latest_row: 'row-m', action: 'grant', historical: false },
  { id: NEW, folder: '~/shop/new', state: 'never', actor: null, at: null, change_id: 'change-n', latest_row: null, action: null, historical: false },
] }

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
    c: { D: d, s: state, tech: false, stopped: false, NOW: '09:41', OFF: { dis: 'true', cur: 'not-allowed' }, ui: { dis: 'false', pulse: 'p', sFg: 'a', sBd: 'b', cur: 'pointer' }, mkSwitch,
      colors: { T1: 'a', T2: 'b', T3: 'c', OR: 'd', CTRL: 'e', BLUE: 'f', HAIR: 'g' }, setState: (update) => {
        const patch = typeof update === 'function' ? update(state) : update
        if (patch) { Object.assign(state, patch); set.push(patch) }
      }, actions },
  }
}
const live = (permissions, opts) => liveOverrides(skeleton(), ctx(settingsFromServer({ projects: PROJECTS, runPermissions: permissions }), opts).c)

test('the served folder rows belong only to the current HQ project; switching projects never shows the old read', () => {
  assert.equal(settingsFromServer({}).runPermission.status, 'loading', 'an unread project list is still loading')
  assert.equal(settingsFromServer({ projectsError: new Error('unreachable') }).runPermission.status, 'project_failed', 'a failed project read does not say Reading forever')
  assert.equal(settingsFromServer({ projects: { projects: [] } }).runPermission.status, 'no_project', 'an answered empty list has no current project')
  const shop = settingsFromServer({ projects: PROJECTS, runPermissions: ROWS })
  assert.equal(shop.runPermission.status, 'read')
  assert.deepEqual(shop.runPermission.folders.map((r) => r.id), ROWS.folders.map((r) => r.id))
  const other = settingsFromServer({ projects: PROJECTS, current: 'other', runPermissions: ROWS })
  assert.equal(other.runPermission.status, 'loading')
  assert.deepEqual(other.runPermission.folders, [])
})

test('one live row per exact folder, with actor/time only for held grant, and safe action per row', () => {
  const v = live(ROWS)
  assert.deepEqual(v.permRows.map((r) => r.folder), ROWS.folders.map((r) => r.folder))
  assert.deepEqual(v.permRows.map((r) => r.btn), ['Take back permission', 'Allow again', 'Take back permission', 'Allow again', null])
  assert.match(v.permRows[0].word, /Allowed by Sam.*\d\d:\d\d/)
  assert.equal(v.permRows[0].color, 'a', 'an allowed folder uses the bright state word')
  assert.equal(v.permRows[1].color, 'b', 'a taken-back folder uses the muted state word')
  assert.match(v.permRows[2].word, /Allowed by Lee/)
  assert.match(v.permRows[3].word, /^Not allowed here/)
  assert.equal(v.permRows[4].word, 'Not asked yet')
  assert.match(v.permRows[4].sub, /asks for this folder/)
  assert.ok(v.permRows.every((r) => !r.sub.includes('once per project')))
})

test('Look somewhere else opens the existing Check sheet only for a current recorded Change', () => {
  const opened = []
  const actions = { openCheckPicker: (change) => opened.push(change) }
  const one = live({ ...ROWS, folders: [ROWS.folders[0]] }, { actions })
  assert.equal(one.uiK.look.dis, 'false')
  assert.equal(one.where.folder, '~/shop/a', 'the exact recorded check folder replaces the project-root guess')
  assert.match(one.where.found, /picking one runs the check there/)
  one.lookElsewhere()
  assert.deepEqual(opened, ['change-a'])

  const state = {}
  const many = live(ROWS, { state, actions })
  assert.equal(many.uiK.look.dis, 'false')
  assert.equal(many.where.folder, '', 'several folders are never represented by one arbitrary folder')
  many.lookElsewhere()
  const selected = live(ROWS, { state, actions })
  assert.deepEqual(selected.lookChoices.map((r) => r.label), ['~/shop/a', '~/shop/b', '~/shop/missing', '~/shop/new'])
  selected.lookChoices[1].open()
  assert.deepEqual(opened, ['change-a', 'change-b'])

  const old = live({ ...ROWS, folders: [ROWS.folders[2]] }, { actions })
  assert.equal(old.uiK.look.dis, 'true', 'a historical permission has no current Change to check')
  assert.match(old.where.found, /No current Change/)
  old.lookElsewhere()
  assert.equal(opened.length, 2)
  assert.equal(live(null, { actions }).uiK.look.dis, 'true', 'an unread folder list cannot open a guessed Check sheet')
})

test('loading, failed, and write-pending rows do not act; an empty read does not invent a permission', () => {
  const reading = live(null)
  assert.equal(reading.permRows[0].word, 'Reading permissions…')
  assert.equal(reading.permRows[0].color, 'b')
  assert.equal(reading.permRows[0].btn, null)
  const failed = live({ error: 'unreadable' })
  assert.match(failed.permRows[0].sub, /could not be read/)
  assert.equal(failed.permRows[0].btn, null)
  const empty = live({ schema_version: 1, project: 'shop', folders: [] })
  assert.equal(empty.permRows[0].word, 'No permission folders yet')
  const busy = live(ROWS, { state: { permissionBusy: A, permissionProject: 'shop' } })
  assert.equal(busy.permRows[0].look.dis, 'true')
  assert.equal(busy.permRows[0].word, 'Checking permission…')
  assert.match(busy.permRows[0].sub, /Reading the permission again/)
  const orphan = live({ ...ROWS, folders: [{ ...ROWS.folders[1], change_id: null, historical: true }] })
  assert.equal(orphan.permRows[0].btn, null, 'a historical folder cannot offer Allow again')
  for (const invalid of [{ ...ROWS.folders[0], id: null }, { ...ROWS.folders[0], id: 'short' }, { ...ROWS.folders[0], folder: '' }, { ...ROWS.folders[0], latest_row: null }, { ...ROWS.folders[0], latest_row: 3 }, { ...ROWS.folders[0], change_id: 4 }]) {
    const row = live({ ...ROWS, folders: [invalid] }).permRows[0]
    assert.equal(row.btn, null, 'an unreadable target cannot offer a write')
    assert.equal(row.look.dis, 'true')
  }
  const badGrant = live({ ...ROWS, folders: [{ ...ROWS.folders[1], latest_row: null }] }).permRows[0]
  assert.equal(badGrant.btn, null, 'a revoked grant needs the row it read')
  const notInRecord = live({ ...ROWS, folders: [{ ...ROWS.folders[3], state: 'not_in_record', latest_row: null }] }).permRows[0]
  assert.equal(notInRecord.btn, 'Allow again', 'a cache with no ledger row may offer a guarded first grant for a current Change')
  const orphanNotInRecord = live({ ...ROWS, folders: [{ ...ROWS.folders[2], state: 'not_in_record' }] }).permRows[0]
  assert.equal(orphanNotInRecord.btn, 'Take back permission', 'an old standing grant can be revoked when its file lacks a matching record')
})

test('a refreshed row keeps stable identity and a refusal remains visible if its folder disappears', () => {
  const template = readFileSync(fileURLToPath(new URL('./template.jsx', import.meta.url)), 'utf8')
  assert.match(template, /\(v\?\.permRows \|\| \[\]\)\.map\(\(p, p\$i\) => \(<React\.Fragment key=\{p\?\.id \|\| p\$i\}/)
  const v = live({ ...ROWS, folders: ROWS.folders.slice(1) }, {
    state: { permissionProject: 'shop', permissionNoteFor: A, permissionNote: 'This permission changed in the record. Read it again.' },
  })
  assert.equal(v.permissionNotice, 'This permission changed in the record. Read it again.')
  assert.ok(v.permRows.every((r) => !r.sub.includes(v.permissionNotice)), 'the message belongs to the card, not a different folder')
  assert.match(template, /v\?\.permissionNotice/)
})

test('one pending press sends one row and says a refusal until a fresh read answers', async () => {
  const asked = []
  let resolve
  const work = new Promise((r) => { resolve = r })
  const d = settingsFromServer({ projects: PROJECTS, runPermissions: ROWS })
  const h = ctx(d, { actions: { setRunPermission: (row) => { asked.push(row.id); return work } } })
  const v = liveOverrides(skeleton(), h.c)
  v.permRows[0].toggle()
  v.permRows[0].toggle()
  assert.deepEqual(asked, [A])
  assert.equal(h.set[0].permissionBusy, A)
  assert.equal(h.set[0].permissionProject, 'shop')
  assert.equal(typeof h.set[0].permissionRequest, 'symbol')
  assert.equal(h.set[0].permissionNote, null)
  assert.equal(h.set[0].permissionNoteFor, null)
  resolve({ kind: 'refused', detail: 'This permission changed in the record. Read it again.' })
  await new Promise((r) => setTimeout(r, 0))
  assert.deepEqual(h.set[1], { permissionBusy: null, permissionRequest: null, permissionNote: 'This permission changed in the record. Read it again.', permissionNoteFor: A })
})

test('a late write from an earlier project or press cannot clear the current permission wait', async () => {
  const deferred = () => {
    let resolve
    return { promise: new Promise((done) => { resolve = done }), resolve: (answer) => resolve(answer) }
  }
  const firstShop = deferred(), other = deferred(), secondShop = deferred()
  const waiting = [firstShop, other, secondShop]
  const asked = []
  const shared = {}
  const h = ctx(settingsFromServer({ projects: PROJECTS, runPermissions: ROWS }), {
    state: shared,
    actions: { setRunPermission: (row) => { asked.push(row.id); return waiting.shift().promise } },
  })
  const show = (project) => {
    const doc = project === 'shop' ? ROWS : { schema_version: 1, project: 'other', folders: [{ ...ROWS.folders[1], folder: '~/other/b' }] }
    h.c.D = settingsFromServer({ projects: PROJECTS, current: project, runPermissions: doc })
    return liveOverrides(skeleton(), h.c)
  }
  show('shop').permRows[0].toggle()
  show('other').permRows[0].toggle()
  show('shop').permRows[0].toggle()
  assert.deepEqual(asked, [A, B, A])
  assert.equal(shared.permissionProject, 'shop')
  assert.equal(shared.permissionBusy, A)
  assert.equal(show('shop').permRows[0].look.dis, 'true')

  firstShop.resolve({ kind: 'refused', detail: 'Old shop answer' })
  await new Promise((done) => setTimeout(done, 0))
  assert.equal(shared.permissionBusy, A, 'the old press for the same project and folder did not clear the newer press')
  assert.equal(shared.permissionNote, null)
  assert.equal(h.set.length, 3)

  other.resolve({ kind: 'refused', detail: 'Other project answer' })
  await new Promise((done) => setTimeout(done, 0))
  assert.equal(shared.permissionBusy, A, 'the other project did not clear the current project wait')
  assert.equal(shared.permissionNote, null)
  assert.equal(h.set.length, 3)
  assert.equal(show('shop').permRows[0].look.dis, 'true')

  secondShop.resolve({ kind: 'refused', detail: 'Current shop answer' })
  await new Promise((done) => setTimeout(done, 0))
  assert.equal(shared.permissionBusy, null)
  assert.equal(shared.permissionNote, 'Current shop answer')
  assert.equal(shared.permissionNoteFor, A)
  assert.equal(h.set.length, 4)
})

test('writes carry the opaque folder and row guards, not a browser path; no Settings press starts a check', async () => {
  const calls = []
  const f = async (url, init = {}) => {
    calls.push([url, init.method || 'GET', init.body || null, init.headers && init.headers['X-ThruLine-Run-Token']])
    return { ok: true, status: 200, json: async () => url === '/api/run/token' ? { token: 't0k' } : { granted: true } }
  }
  assert.equal(PERMISSIONS_READ('shop'), '/api/hq/projects/shop/run-permissions')
  await permissionWrite('shop', ROWS.folders[0], f)
  await permissionWrite('shop', ROWS.folders[1], f)
  await permissionWrite('shop', ROWS.folders[2], f)
  assert.deepEqual(calls, [
    ['/api/run/token', 'GET', null, undefined],
    ['/api/changes/change-a/run/permission/revoke', 'POST', JSON.stringify({ expected_folder: A, expected_row: 'row-a' }), 't0k'],
    ['/api/run/token', 'GET', null, undefined],
    ['/api/changes/change-b/run/permission', 'POST', JSON.stringify({ expected_folder: B, expected_row: 'row-b' }), 't0k'],
    ['/api/run/token', 'GET', null, undefined],
    [`/api/hq/projects/shop/run-permissions/${OLD}/revoke`, 'POST', '{"expected_row":"row-old"}', 't0k'],
  ])
  const before = calls.length
  const orphanGrant = await permissionWrite('shop', { ...ROWS.folders[1], change_id: null, historical: true }, f)
  assert.equal(orphanGrant.doc.error, 'not_available')
  assert.equal(calls.length, before, 'an orphan cannot grant a folder or even fetch a token')
  const unguardedRevoke = await permissionWrite('shop', { ...ROWS.folders[0], latest_row: null }, f)
  assert.equal(unguardedRevoke.doc.error, 'not_available')
  assert.equal(calls.length, before, 'a revoke without the row guard cannot reach the server')
  const malformedId = await permissionWrite('shop', { ...ROWS.folders[0], id: 'short' }, f)
  assert.equal(malformedId.doc.error, 'not_available')
  assert.equal(calls.length, before, 'a malformed opaque id cannot reach the server')
  assert.equal(permissionAnswer({ ok: false, status: 409, doc: { error: 'permission_target_changed', plain: 'This permission changed in the record. Read it again.' } }).detail, 'This permission changed in the record. Read it again.')
  assert.equal(permissionAnswer({ ok: false, status: 0, doc: { error: 'unreachable' } }).kind, 'refused')
  assert.equal(permissionAnswer({ ok: false, status: 500, doc: { error: 'write_failed' } }).detail, 'ThruLine could not finish this permission change. Check the refreshed row before retrying.')
})

test('a permission press waits for a fresh read after either answer before reporting the result', async () => {
  const calls = []
  const f = async (url, init = {}) => {
    calls.push(init.method === 'POST' ? 'post' : 'token')
    return { ok: url === '/api/run/token', status: url === '/api/run/token' ? 200 : 409, json: async () => url === '/api/run/token' ? { token: 't0k' } : { error: 'permission_target_changed', plain: 'This permission changed in the record. Read it again.' } }
  }
  const refresh = async () => { calls.push('read'); return { ok: true, doc: ROWS } }
  const result = await writeThenRead('shop', ROWS.folders[0], refresh, f)
  assert.deepEqual(calls, ['token', 'post', 'read'])
  assert.deepEqual(result, { kind: 'refused', detail: 'This permission changed in the record. Read it again.' })
  assert.match((await writeThenRead('shop', ROWS.folders[0], async () => ({ ok: false }), f)).detail, /could not read this permission again/)
})

/* DESIGN-SPEC §3.10 F1 draws "Allowed by … · 16:09 today": a bare clock time on a grant made on an
 * earlier day reads as if it were today's, so the row says which day (census settings#9). The
 * actor stays the record's own (C46 sheet: "Render the record's actor and time only when held"). */
test('an allowed folder says which day its time belongs to', () => {
  const at = (y, mo, d, h, mi) => new Date(y, mo, d, h, mi).toISOString()
  const rows = (given) => ({ schema_version: 1, project: 'shop', folders: [{ id: A, folder: '~/shop/a', state: 'given', actor: 'Sam', at: given, change_id: 'change-a', latest_row: 'row-a', action: 'revoke', historical: false }] })
  const word = (given) => {
    const d = settingsFromServer({ projects: PROJECTS, runPermissions: rows(given), now: new Date(2026, 9, 12, 18, 30) })
    return liveOverrides(skeleton(), ctx(d).c).permRows[0].word
  }
  assert.equal(word(at(2026, 9, 12, 9, 40)), 'Allowed by Sam · 09:40 today')
  assert.equal(word(at(2026, 9, 11, 23, 59)), 'Allowed by Sam · 23:59 yesterday')
  assert.equal(word(at(2026, 9, 5, 11, 0)), 'Allowed by Sam · 11:00, 5 Oct')
  assert.equal(word(at(2025, 11, 31, 8, 5)), 'Allowed by Sam · 08:05, 31 Dec 2025')
})
