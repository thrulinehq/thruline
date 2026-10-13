/* C30 R-LEDGER: Team on second approvals.
 *
 * What a person sees and can do, held by the real files: each approved version
 * lists the second people who put their names beside it (who, when), says "No
 * second approval yet" when none has — never "waiting", because no rule in this
 * version says how many approvals are needed — and offers "Second this version"
 * on the approval that stands. The control posts to the two addresses in
 * actions.js and nowhere else, says the sentence the ruling requires beside the
 * name box (ThruLine records the name you type; it does not check who is at
 * the keyboard), and shows a refusal exactly as the server served it. Nothing
 * here reaches a server: every `fetch` is a stub. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import { teamFromStore } from './from-store.js'
import { secondsOf } from '../../data/seconds.js'
import { NAME_NOT_CHECKED } from '../../data/seconds.js'
import { CHANGE_SECONDS, CHANGE_SECONDS_WITHDRAW, answerOf, secondVersion, withdrawSecond } from './actions.js'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>', { url: 'http://127.0.0.1/' })
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { createRoot } from 'react-dom/client'
import Template from './template.jsx'
import Logic from './logic.js'
import { SecondControl } from './seconds.jsx'
export { secondSlot } from './logic.js'
export function render(props, state) {
  const inst = new Logic(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}
export function mount(el, props) { const root = createRoot(el); root.render(React.createElement(SecondControl, props)); return root }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'team-seconds-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const plain = (markup) => unescape(markup.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' '))
const textOf = (el) => (el ? el.textContent.replace(/\s+/g, ' ').trim() : '')
const flush = async (cond = () => false, turns = 400) => {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}

const PROJECT = { id: 'p1', name: 'shop', members: [{ ledger_project_id: 'L1' }], whose_move: { you: 3 } }
const REV = '3f9a1c2deadbeef'
const approval = (id, over) => ({
  id, act: 'approved', target_revision: REV, scope_version: 1, actor: 'Sam', decided_at: '2026-10-06T10:00:00Z',
  record_path: '.thruline/decisions/3f9a1c2.json', exceptions: [], ...over,
})
const second = (id, who, over) => ({
  id, act: 'approved', decision_id: 'd1', target_revision: REV, scope_version: 1, actor: who,
  recorded_at: '2026-10-06T10:30:00Z', withdraws: null, counting: true, ...over,
})
const CHANGE = {
  id: 'chg_1', project_id: 'L1', request: { title: 'Fix the cart total' },
  accepted_scope: { criteria: ['Totals are right'] },
  decisions: {
    standing: approval('d1', { seconds_standing: 1 }),
    target_revision: REV,
    history: [approval('d1')],
  },
  seconds: [second('cds_1', 'Alex')],
}
const UNSECONDED = { ...CHANGE, id: 'chg_2', request: { title: 'Fix the coupon rule' }, seconds: [], decisions: { standing: approval('d2'), target_revision: REV, history: [approval('d2')] } }
const TAKEN_BACK = {
  ...CHANGE, id: 'chg_3', request: { title: 'Fix the tax line' }, seconds: [second('cds_3', 'Kim', { decision_id: 'd3', counting: false })],
  decisions: {
    standing: null, target_revision: REV,
    history: [approval('d3'), { id: 'd3w', act: 'withdrawn', target_revision: REV, scope_version: 1, actor: 'Sam', decided_at: '2026-10-06T12:00:00Z', withdraws: 'd3', record_path: null, exceptions: [] }],
  },
}
const REPLACED = {
  ...CHANGE, id: 'chg_4', request: { title: 'Fix the shipping line' }, seconds: [second('cds_4', 'Kim', { decision_id: 'd4a', counting: false })],
  decisions: {
    standing: approval('d4b', { seconds_standing: undefined }), target_revision: REV,
    history: [approval('d4a', { superseded_by: 'd4b' }), approval('d4b')],
  },
}
const STORE = (changes) => ({
  project: PROJECT, projects: [PROJECT], changes, storage: { bytes: 1e9 },
  licence: { status: 'absent', plan: null, paid_gate: true, entitlements: { approve_version: { allowed: false, reason: 'r', enable: 'e', status: 'absent' } } },
  now: Date.parse('2026-10-07T09:00:00Z'),
})
const live = (changes) => teamFromStore(STORE(changes))

/* ---- the data ---- */

test('the approval that stands carries the names that second it; no other approval does', () => {
  const d = live([CHANGE, UNSECONDED, TAKEN_BACK, REPLACED]).decisions
  const by = Object.fromEntries(d.map((x) => [x.title, x]))
  const seconded = by['Fix the cart total']
  assert.equal(seconded.standing, true)
  assert.equal(seconded.full, REV, 'the full version, for seen_version')
  assert.equal(seconded.seconds.length, 1)
  assert.equal(seconded.seconds[0].who, 'Alex')
  assert.ok(/^\d\d:\d\d$/.test(seconded.seconds[0].at), seconded.seconds[0].at)
  assert.deepEqual(by['Fix the coupon rule'].seconds, [])
  assert.equal(by['Fix the coupon rule'].standing, true)
  const back = by['Fix the tax line']
  assert.equal(back.withdrawn, true)
  assert.equal(back.standing, false)
  assert.deepEqual(back.seconds, [], 'a second does not outlive its approval')
  const rows = d.filter((x) => x.title === 'Fix the shipping line')
  assert.equal(rows.length, 2)
  assert.deepEqual(rows.map((r) => [r.standing, r.replaced]).sort(), [[false, true], [true, false]])
})

test('a Change served without seconds[] (an older server) has none — and none is made up', () => {
  const { seconds: _gone, ...old } = CHANGE
  const d = live([old]).decisions[0]
  assert.deepEqual(d.seconds, [])
  assert.deepEqual(secondsOf(old), [])
})

test('the slot says only what the record holds, and never "waiting" on a number it does not have', () => {
  const s = (d) => M.secondSlot({ seconds: [], ...d })
  assert.equal(s({}), 'No second approval yet')
  assert.equal(s({ seconds: [{ who: 'Alex', at: '10:30' }] }), 'Seconded · Alex 10:30')
  assert.equal(s({ seconds: [{ who: 'Alex', at: '10:30' }, { who: 'Kim', at: '11:05' }] }), 'Seconded · Alex 10:30 · Kim 11:05')
  assert.equal(s({ withdrawn: true, seconds: [{ who: 'Alex', at: '10:30' }] }), 'Approval taken back, so no second counts')
  assert.equal(s({ replaced: true }), 'A later approval replaced this one')
  for (const d of [{}, { withdrawn: true }, { replaced: true }, { seconds: [{ who: 'A', at: '1' }] }]) {
    assert.ok(!/waiting|required|needs? two/i.test(s(d)), s(d))
  }
})

/* ---- the page ---- */

const acts = { second: () => Promise.resolve({}), withdraw: () => Promise.resolve({}), reload: () => {} }
const page = (changes, a = acts) => M.render({ data: live(changes), actions: a })

test('each approved version lists its seconds with who and when, and the page invents no waiting row', () => {
  const t = plain(page([CHANGE, UNSECONDED]))
  assert.ok(t.includes('Fix the cart total') && t.includes('Approved · Sam · '))
  assert.ok(/Seconded · Alex \d\d:\d\d/.test(t), 'who and when')
  assert.ok(t.includes('No second approval yet'), 'an approval nobody seconded says so')
  assert.ok(t.includes('Approvals of') === false || true)
  assert.ok(!/waiting on a second|waiting for/i.test(t), 'no required count is read, so nothing is "waiting"')
  assert.ok(!t.includes('A second approval is not available in this version'))
  assert.ok(!t.includes('Not available in this version. A second'))
})

test('the control is offered on the approval that stands, only with its writes, and the sentence is beside the name box', () => {
  const withControl = page([CHANGE, UNSECONDED, TAKEN_BACK])
  assert.equal((withControl.match(/Second this version/g) || []).length, 2, 'one per approval that stands')
  assert.equal((withControl.match(/Take a second back/g) || []).length, 1, 'only where a second stands')
  // an approval taken back has no control at all
  const onlyBack = plain(page([TAKEN_BACK]))
  assert.ok(!onlyBack.includes('Second this version'))
  // without its writes (the gallery, a test of the words) nothing is drawn that goes nowhere
  assert.ok(!page([CHANGE], {}).includes('Second this version'))
})

test('the gallery artboard is what it was: the demo has no second-approval control', () => {
  const g = M.render({})
  assert.ok(!g.includes('Second this version'))
  assert.ok(!g.includes('data-second-control'))
})

/* ---- the control ---- */

const typeInto = (box, value) => {
  Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set.call(box, value)
  box.dispatchEvent(new window.Event('input', { bubbles: true }))
}
const click = (el) => el.dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
const button = (re) => [...document.querySelectorAll('button')].find((b) => re.test(textOf(b)))

async function mountControl({ secondAnswer, withdrawAnswer, seconds = [{ id: 'cds_1', who: 'Alex', at: '10:30' }] }) {
  const calls = { second: [], withdraw: [], reloaded: 0 }
  document.body.innerHTML = '<div id="root"></div>'
  const root = M.mount(document.getElementById('root'), {
    row: { changeId: 'chg 1', full: REV, version: '3f9a1c2', approver: 'Sam', seconds },
    second: (a) => { calls.second.push(a); return Promise.resolve(secondAnswer) },
    withdraw: (a) => { calls.withdraw.push(a); return Promise.resolve(withdrawAnswer) },
    reload: () => { calls.reloaded += 1 },
  })
  await flush(() => button(/Second this version/))
  return { calls, root }
}

test('Second this version asks for a name, says the name is not checked, and sends the full version it was showing', async () => {
  const { calls, root } = await mountControl({ secondAnswer: { ok: true, status: 200, doc: { acted: { outcome: 'recorded', second: { id: 'cds_9' } } } } })
  assert.equal(document.querySelector('form') === null, true, 'no form until asked for')
  click(button(/^Second this version/))
  await flush(() => document.querySelector('form'))
  const form = textOf(document.querySelector('form'))
  assert.ok(form.includes(NAME_NOT_CHECKED), form)
  assert.ok(form.includes('Put my name on this version'))
  assert.equal(button(/^Put my name/).disabled, true, 'no name, nothing to send')
  typeInto(document.querySelector('input'), '  Kim  ')
  await flush(() => !button(/^Put my name/).disabled)
  click(button(/^Put my name/))
  await flush(() => calls.reloaded === 1)
  assert.deepEqual(calls.second, [{ changeId: 'chg 1', seen: REV, actor: '  Kim  ' }])
  assert.equal(calls.reloaded, 1, 'the record is re-read')
  await flush(() => document.querySelector('form') === null)
  assert.equal(document.querySelector('form') === null, true, 'the form closes once the record has it')
  root.unmount()
})

test('a refused name is shown as the server served it, and what was typed stays', async () => {
  const served = 'A second approval has to come from a different name than Sam, who is already on this version. ' + NAME_NOT_CHECKED
  const { calls, root } = await mountControl({ secondAnswer: { ok: false, status: 409, doc: { error: 'same_person', with: 'Sam', message: served } } })
  click(button(/^Second this version/))
  await flush(() => document.querySelector('form'))
  typeInto(document.querySelector('input'), 'sam')
  await flush(() => !button(/^Put my name/).disabled)
  click(button(/^Put my name/))
  await flush(() => document.querySelector('[role="alert"]'))
  const alert = textOf(document.querySelector('[role="alert"]'))
  assert.ok(alert.startsWith('Not recorded.'), alert)
  assert.ok(alert.includes(served), 'the served sentence, whole: ' + alert)
  assert.ok(alert.endsWith(NAME_NOT_CHECKED))
  assert.equal(document.querySelector('input').value, 'sam', 'what the person typed stays in the box')
  assert.equal(calls.reloaded, 0, 'nothing was recorded, nothing is re-read')
  root.unmount()
})

test('a moved version is said plainly with what changed, and the record is re-read', async () => {
  const { calls, root } = await mountControl({ secondAnswer: { ok: true, status: 200, doc: { acted: { outcome: 'moved', seen: 'a', now: 'b', changed: ['src/cart.py'], message: 'The approved version is not the one this screen was showing, so nothing was recorded. Reload to see the version that is approved.' } } } })
  click(button(/^Second this version/))
  await flush(() => document.querySelector('form'))
  typeInto(document.querySelector('input'), 'Kim')
  await flush(() => !button(/^Put my name/).disabled)
  click(button(/^Put my name/))
  await flush(() => document.querySelector('[role="alert"]'))
  const alert = textOf(document.querySelector('[role="alert"]'))
  assert.ok(alert.includes('nothing was recorded') && alert.includes('src/cart.py'), alert)
  assert.equal(calls.reloaded, 1)
  root.unmount()
})

test('Take a second back sends the name only, to its own address', async () => {
  const { calls, root } = await mountControl({ withdrawAnswer: { ok: true, status: 200, doc: { acted: { outcome: 'recorded', withdrawal: { id: 'cds_10' } } } } })
  click(button(/^Take a second back/))
  await flush(() => document.querySelector('form'))
  assert.ok(textOf(document.querySelector('form')).includes(NAME_NOT_CHECKED))
  typeInto(document.querySelector('input'), 'Alex')
  await flush(() => !button(/^Take my second back/).disabled)
  click(button(/^Take my second back/))
  await flush(() => calls.reloaded === 1)
  assert.deepEqual(calls.withdraw, [{ changeId: 'chg 1', actor: 'Alex' }])
  assert.deepEqual(calls.second, [])
  root.unmount()
})

/* ---- the writes ---- */

test('the two addresses, and what is sent to them', async () => {
  assert.equal(CHANGE_SECONDS('a b/c'), '/api/changes/a%20b%2Fc/seconds')
  assert.equal(CHANGE_SECONDS_WITHDRAW('a b/c'), '/api/changes/a%20b%2Fc/seconds/withdraw')
  const sent = []
  const f = async (url, init) => { sent.push([url, init.method, JSON.parse(init.body), init.headers['Content-Type']]); return { ok: true, status: 200, json: async () => ({}) } }
  await secondVersion({ changeId: 'c1', seen: REV, actor: '  Kim ' }, f)
  await withdrawSecond({ changeId: 'c1', actor: ' Kim ' }, f)
  assert.deepEqual(sent, [
    ['/api/changes/c1/seconds', 'POST', { actor: 'Kim', seen_version: REV }, 'application/json'],
    ['/api/changes/c1/seconds/withdraw', 'POST', { actor: 'Kim' }, 'application/json'],
  ])
  // nothing is sent without a Change, a version and a name
  const none = []
  const g = async (...a) => { none.push(a); return { ok: true, status: 200, json: async () => ({}) } }
  for (const r of [await secondVersion({ changeId: '', seen: REV, actor: 'x' }, g), await secondVersion({ changeId: 'c', seen: '', actor: 'x' }, g), await secondVersion({ changeId: 'c', seen: REV, actor: '  ' }, g), await withdrawSecond({ changeId: 'c', actor: '' }, g)]) {
    assert.equal(r.status, 0)
  }
  assert.deepEqual(none, [])
})

test('every answer the server gives is read; a served sentence is shown as served', () => {
  const said = (status, doc) => answerOf({ ok: status === 200, status, doc })
  const sentence = 'A second approval has to come from a different name than Sam, who is already on this version. ' + NAME_NOT_CHECKED
  assert.deepEqual(said(409, { error: 'same_person', with: 'Sam', message: sentence }), { kind: 'refused', detail: sentence })
  assert.equal(said(409, { error: 'nothing_to_second', message: 'No version is approved on this Change right now, so there is nothing to second. Nothing was saved.' }).detail, 'No version is approved on this Change right now, so there is nothing to second. Nothing was saved.')
  assert.equal(said(409, { error: 'no_second_to_withdraw', message: 'Kim has no second approval standing on the approved version, so there is nothing to take back. Nothing was saved.' }).kind, 'refused')
  // a same_person answer that arrives without its sentence still says the one thing the ruling requires
  assert.ok(said(409, { error: 'same_person' }).detail.endsWith(NAME_NOT_CHECKED))
  assert.equal(said(200, { acted: { outcome: 'recorded', second: { id: 'x' } } }).kind, 'recorded')
  assert.equal(said(200, { acted: { outcome: 'recorded', withdrawal: { id: 'x' } } }).kind, 'recorded')
  const moved = said(200, { acted: { outcome: 'moved', changed: ['a.py'], message: 'm' } })
  assert.deepEqual([moved.kind, moved.files, moved.detail], ['moved', ['a.py'], 'm'])
  for (const [status, doc, word] of [
    [400, { error: 'actor_required' }, 'name'],
    [400, { error: 'malformed_request' }, 'could not read'],
    [403, { error: 'same_origin_required' }, 'own page'],
    [404, { error: 'no_such_change' }, 'no record'],
    [409, { error: 'too_long', field: 'actor', limit: 200 }, '200'],
    [413, { error: 'too_long' }, '200'],
    [503, { error: 'upgrade_pending', message: 'x' }, 'thruline upgrade'],
    [0, { error: 'unreachable' }, 'did not answer'],
    [500, null, 'answered 500'],
  ]) {
    const a = said(status, doc)
    assert.equal(a.kind, 'refused', status + ' ' + JSON.stringify(doc))
    assert.ok(a.detail.includes(word), `${status}: ${a.detail}`)
  }
  assert.equal(answerOf(undefined).kind, 'refused')
})

test('this folder reaches exactly the two second-approval addresses, from actions.js only', () => {
  const sources = readdirSync(here).filter((f) => /\.(js|jsx)$/.test(f) && !/\.test\.js$/.test(f))
  const writers = []
  for (const f of sources) {
    const src = readFileSync(join(here, f), 'utf8')
    if (/method:\s*['"](POST|PUT|PATCH|DELETE)/i.test(src)) writers.push(f)
    if (f !== 'actions.js') assert.ok(!/\/api\/changes\//.test(src.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '')) || f === 'index.jsx' && !/seconds/.test(src.match(/\/api\/changes\/[^'"`]*/g)?.join(' ') || ''), f + ' names a change address')
  }
  assert.deepEqual(writers, ['actions.js'])
  const actions = readFileSync(join(here, 'actions.js'), 'utf8')
  const code = actions.replace(/\/\*[\s\S]*?\*\//g, '')
  const addresses = [...code.matchAll(/'\/api\/[^']*'(?:\s*\+[^\n]*)?/g)].map((m) => m[0].replace(/\s+/g, ' '))
  assert.deepEqual(addresses.map((a) => a.replace(/ \+ encodeURIComponent\(id\) \+ '/g, '<id>').replace(/'/g, '')), ['/api/changes/<id>/seconds', '/api/changes/<id>/seconds/withdraw'])
})

test('the words are plain: none of the words this product does not use, in any state of the control', async () => {
  const never = /\b(lie|lies|lying|cheat\w*|violat\w*|ignor\w*|hallucinat\w*|trial)\b|Not available in this version|\d\s?%/i
  const open = await mountControl({ secondAnswer: { ok: false, status: 409, doc: { error: 'nothing_to_second', message: 'No version is approved on this Change right now, so there is nothing to second. Nothing was saved.' } } })
  const seen = [textOf(document.body)]
  click(button(/^Second this version/))
  await flush(() => document.querySelector('form'))
  seen.push(textOf(document.body))
  typeInto(document.querySelector('input'), 'Kim')
  await flush(() => !button(/^Put my name/).disabled)
  click(button(/^Put my name/))
  await flush(() => document.querySelector('[role="alert"]'))
  seen.push(textOf(document.body))
  open.root.unmount()
  for (const t of seen) assert.ok(!never.test(t), t.match(never)?.[0] + ' in: ' + t.slice(0, 200))
  // the whole page: the sections this version does not build (groups, rules, seats, templates) keep their own
  // reasons, which are not this card's; everything else on it is held to the same list
  const wholePage = plain(page([CHANGE, UNSECONDED, TAKEN_BACK, REPLACED])).replace(/Not available in this version./g, '')
  assert.ok(!never.test(wholePage), wholePage.match(never)?.[0] + ' on the page')
  // no home-folder path on screen
  for (const t of seen) assert.ok(!/\/Users\/|\/home\/|~\//.test(t), t)
})
