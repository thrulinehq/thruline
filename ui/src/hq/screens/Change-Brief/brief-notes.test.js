/* C30 R-LEDGER: the Brief's notes.
 *
 * What a person sees and can do, held by the real files: the ⋯ menu's "Add a
 * note" opens a form (notes.jsx) that keeps one note on the tracked Change
 * through the one address in notes-actions.js, the Brief lists the notes the
 * record holds with who and when, and the artboard in the gallery is exactly
 * what it was. Nothing here reaches a server: every `fetch` is a stub.
 *
 * A note is words. Nothing here, and nothing it feeds, writes a verdict, a
 * count, readiness or an approval; the form says so beside the box. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import { addNote, noteAnswerOf, CHANGE_NOTES } from './notes-actions.js'

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
import { withFrame } from '../../data/project.js'
import { changeSnapshot } from '../../data/change.js'
import { threadFixture } from '../What-Changed/thread-fixture.js'
import { BriefNotes } from './notes.jsx'
export { changeSnapshot, threadFixture }
export function render(props, state) {
  const L = withFrame(Logic)
  const inst = new L(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}
export function vals(props, state) {
  const L = withFrame(Logic)
  const inst = new L(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return inst.renderVals()
}
export function mount(el, props) { const root = createRoot(el); root.render(React.createElement(BriefNotes, props)); return root }
export function mountTemplate(el, props, state, over) {
  const L = withFrame(Logic)
  const inst = new L(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  const root = createRoot(el)
  root.render(React.createElement(Template, { v: { ...inst.renderVals(), ...over }, link: (h) => h }))
  return root
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'brief-notes-')), 'brief.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const flush = async (cond = () => false, turns = 400) => {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}
const NOW = new Date(Date.UTC(2026, 9, 7, 12, 0, 0))
const NOTES = [
  { id: 'cn_1', actor: 'alex', body: 'Read it twice; fine by me.', recorded_at: '2026-10-07T11:30:00Z', decision_id: null },
  { id: 'cn_2', actor: 'sam', body: 'The agent changed the tests after I looked.', recorded_at: '2026-10-07T11:45:00Z', decision_id: 'cd_1' },
]
const snapshot = (notes = NOTES) => M.changeSnapshot(M.threadFixture(), { change: { id: 'chg_1', notes }, hq: null }, NOW, {})
const html = (data, acts = {}, state = {}) => M.render({ data, actions: acts, frame: {} }, state)
const textOf = (el) => (el ? el.textContent.replace(/\s+/g, ' ').trim() : '')
const plain = (markup) => markup.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ')

/* ---- what is on the Brief ---- */

test('the snapshot carries the served notes, and the Brief lists them with who and when', () => {
  const data = snapshot()
  assert.deepEqual(data.notes.map((n) => [n.who, n.body, n.decisionId]), [
    ['alex', 'Read it twice; fine by me.', null],
    ['sam', 'The agent changed the tests after I looked.', 'cd_1'],
  ])
  const t = plain(html(data, { noteChangeId: 'chg_1' }))
  assert.ok(t.includes('Notes'), 'the heading')
  assert.ok(t.includes('alex') && t.includes('Read it twice; fine by me.'))
  assert.ok(t.includes('sam') && t.includes('The agent changed the tests after I looked.'))
  assert.ok(/20\d\d-\d\d-\d\d \d\d:\d\d/.test(t), 'when: ' + t.slice(0, 200))
  assert.ok(t.includes('kept with an approval or a withdrawal'), 'the one kept with a decision says so')
})

test('a Change with no notes draws no notes section, and no form until one is asked for', () => {
  const t = plain(html(snapshot([]), { noteChangeId: 'chg_1' }))
  assert.ok(!t.includes('words people kept here'), 'no empty section')
  assert.ok(!t.includes('A note on this Change'))
})

test('the gallery artboard is what it was: no notes section, and the menu item only closes the menu', () => {
  const g = M.render({}, { more: true })
  assert.ok(!g.includes('bn-note'))
  assert.ok(!plain(g).includes('words people kept here'))
  assert.ok(plain(g).includes('Add a note'), 'the canvas’s menu item')
  const v = M.vals({}, { more: true })
  assert.equal(v.noteRows, null)
  assert.equal(typeof v.openNote, 'function')
  assert.equal(v.openNote, v.closeMore, 'in the gallery the item does what it always did')
})

test('"Add a note" in the menu is the control that opens the form; the form is on the tracked Change', () => {
  const data = snapshot([])
  const menu = html(data, { noteChangeId: 'chg_1' }, { more: true })
  assert.ok(menu.includes('Add a note') && menu.includes('Open in editor') && menu.includes('This is wrong…'))
  const open = html(data, { noteChangeId: 'chg_1' }, { noteOpen: true })
  assert.ok(open.includes('id="bn-note"'), 'the box')
  const t = plain(open)
  assert.ok(t.includes('A note on this Change'))
  assert.ok(t.includes('kept with your name and the time'))
  assert.ok(t.includes('it changes no result and no approval'))
  assert.ok(t.includes('Keep note') && t.includes('Cancel'))
  assert.ok(!t.includes('Not available in this version'))
})

test('on work that is not tracked as a Change the form says so and offers nothing to keep', () => {
  const open = html(snapshot([]), {}, { noteOpen: true })
  assert.ok(!open.includes('id="bn-note"'))
  assert.ok(plain(open).includes('A note is kept on a Change, and this work is not tracked as one yet.'))
})

test('the menu’s "Add a note" is wired to the control that opens the form, "Open in editor" is left as it was, and "This is wrong…" opens the objection form (brief-objections.test.js)', async () => {
  const pressed = []
  document.body.innerHTML = '<div id="root"></div>'
  const root = M.mountTemplate(document.getElementById('root'), { data: snapshot([]), actions: { noteChangeId: 'chg_1' }, frame: {} }, { more: true }, {
    openNote: () => pressed.push('openNote'),
    closeMore: () => pressed.push('closeMore'),
    openObjection: () => pressed.push('openObjection'),
  })
  await flush(() => document.querySelector('[role="menu"]'))
  const items = [...document.querySelectorAll('[role="menuitem"]')]
  assert.deepEqual(items.map(textOf), ['Add a note', 'Open in editor', 'This is wrong…'])
  items[0].dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  items[1].dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  items[2].dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await flush(() => pressed.length >= 3)
  assert.deepEqual(pressed, ['openNote', 'closeMore', 'openObjection'])
  root.unmount()
})

/* ---- the form ---- */

const typeInto = (box, value) => {
  Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set.call(box, value)
  box.dispatchEvent(new window.Event('input', { bubbles: true }))
}

async function openForm(add, extra = {}) {
  const calls = { closed: 0, reloaded: 0, added: [] }
  document.body.innerHTML = '<div id="root"></div>'
  const root = M.mount(document.getElementById('root'), {
    notes: [], changeId: 'chg_1', open: true,
    onClose: () => { calls.closed += 1 },
    reload: () => { calls.reloaded += 1 },
    add: (body) => { calls.added.push(body); return add(body) },
    ...extra,
  })
  await flush(() => document.getElementById('bn-note'))
  const box = document.getElementById('bn-note')
  assert.ok(box, 'the box is on the page')
  const keep = () => [...document.querySelectorAll('button')].find((b) => /^Keep note|^Keeping/.test(textOf(b)))
  return { calls, box, keep, root }
}

test('Keep note sends the words once, then closes the form and re-reads the record', async () => {
  const { calls, box, keep, root } = await openForm(async () => ({ ok: true, status: 200, doc: { schema_version: 7, note: { id: 'cn_9' } } }))
  assert.equal(keep().disabled, true, 'nothing to keep yet')
  typeInto(box, '  Looks right to me.  ')
  await flush(() => !keep().disabled)
  assert.equal(keep().disabled, false)
  keep().dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await flush(() => calls.reloaded === 1)
  assert.deepEqual(calls.added, ['  Looks right to me.  '], 'the words as typed; addNote trims them on the way out')
  assert.equal(calls.closed, 1)
  assert.equal(calls.reloaded, 1)
  root.unmount()
})

test('a refusal is drawn in plain words and every word the person typed stays in the box', async () => {
  const { calls, box, keep, root } = await openForm(async () => ({ ok: false, status: 409, doc: { error: 'too_long', field: 'body', limit: 2000 } }))
  typeInto(box, 'my words')
  await flush(() => !keep().disabled)
  keep().dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await flush(() => document.querySelector('[role="alert"]'))
  const alert = textOf(document.querySelector('[role="alert"]'))
  assert.ok(alert.startsWith('Not kept.'), alert)
  assert.ok(alert.includes('at most 2,000 characters'), alert)
  assert.equal(document.getElementById('bn-note').value, 'my words')
  assert.equal(calls.closed, 0)
  assert.equal(calls.reloaded, 0, 'a note that was not kept re-reads nothing')
  root.unmount()
})

test('a note over the limit cannot be sent, and the box says by how much', async () => {
  const { calls, box, keep, root } = await openForm(async () => ({ ok: true, status: 200, doc: { note: {} } }))
  typeInto(box, 'x'.repeat(2003))
  await flush(() => document.querySelector('[role="status"]'))
  assert.ok(textOf(document.querySelector('[role="status"]')).startsWith('3 too many'), textOf(document.querySelector('[role="status"]')))
  assert.equal(keep().disabled, true)
  keep().dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await flush(() => false, 20)
  assert.deepEqual(calls.added, [])
  root.unmount()
})

test('a request that never gets an answer is drawn as one, and nothing is claimed kept', async () => {
  const { box, keep, root } = await openForm(async () => { throw new Error('boom') })
  typeInto(box, 'words')
  await flush(() => !keep().disabled)
  keep().dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await flush(() => document.querySelector('[role="alert"]'))
  assert.ok(textOf(document.querySelector('[role="alert"]')).includes('ThruLine did not answer'))
  root.unmount()
})

/* ---- the one write address ---- */

function stub(answer) {
  const calls = []
  const f = async (url, init = {}) => {
    calls.push({ url, method: init.method || 'GET', body: init.body ? JSON.parse(init.body) : null })
    if (answer === null) throw new TypeError('Failed to fetch')
    return { ok: answer.status < 400, status: answer.status, json: async () => answer.doc }
  }
  return { f, calls }
}

test('Add a note: write address 21, a body of words and nothing else', async () => {
  const { f, calls } = stub({ status: 200, doc: { note: { id: 'cn_1' } } })
  await addNote({ changeId: 'chg 1', body: '  Read the diff.\n' }, f)
  assert.deepEqual(calls, [{ url: '/api/changes/chg%201/notes', method: 'POST', body: { body: 'Read the diff.' } }])
  assert.equal(CHANGE_NOTES('x'), '/api/changes/x/notes')
})

test('nothing is sent without a Change or without words', async () => {
  const { f, calls } = stub({ status: 200, doc: {} })
  for (const a of [{ changeId: null, body: 'hi' }, { changeId: 'c', body: '   ' }, { changeId: 'c', body: undefined }]) {
    assert.equal((await addNote(a, f)).ok, false)
  }
  assert.equal(calls.length, 0)
})

test('every answer the server gives is read, and none is thrown away', async () => {
  const kept = noteAnswerOf({ ok: true, status: 200, doc: { note: { id: 'cn_1' } } })
  assert.equal(kept.kind, 'kept')
  const cases = [
    [{ status: 400, doc: { error: 'body_required' } }, 'has to say something'],
    [{ status: 403, doc: { error: 'same_origin_required' } }, 'only from its own page'],
    [{ status: 404, doc: { error: 'no_such_change' } }, 'no record of this Change'],
    [{ status: 409, doc: { error: 'too_long', field: 'body' } }, 'at most 2,000 characters'],
    [{ status: 413, doc: null }, 'at most 2,000 characters'],
    [{ status: 503, doc: { error: 'upgrade_pending', message: 'x' } }, 'thruline upgrade'],
    [{ status: 0, doc: { error: 'unreachable' } }, 'did not answer'],
    [{ status: 500, doc: null }, 'answered 500'],
    [{ status: 200, doc: {} }, 'cannot read'],
  ]
  for (const [res, words] of cases) {
    const a = noteAnswerOf({ ok: false, ...res })
    assert.equal(a.kind, 'refused', JSON.stringify(res))
    assert.ok(a.detail.includes(words), `${JSON.stringify(res)} → ${a.detail}`)
    assert.ok(!/\b(lie|cheat|violate|ignore|hallucinat)/i.test(a.detail), a.detail)
  }
  assert.equal(noteAnswerOf(null).kind, 'refused')
})

test('census: in this folder only notes-actions.js and objection-actions.js write, and notes-actions.js reaches exactly one address', () => {
  for (const n of readdirSync(here).filter((n) => /\.(js|jsx)$/.test(n) && !n.endsWith('.test.js') && n !== 'notes-actions.js' && n !== 'objection-actions.js')) {
    assert.ok(!/method:\s*['"]POST['"]/.test(readFileSync(join(here, n), 'utf8')), `${n} must not POST`)
  }
  const a = readFileSync(join(here, 'notes-actions.js'), 'utf8')
  const addresses = [...a.matchAll(/'\/api\/changes\/' \+ encodeURIComponent\(id\) \+ '([^']*)'/g)].map((m) => m[1])
  assert.deepEqual(addresses, ['/notes'])
  assert.equal([...a.matchAll(/method:\s*'POST'/g)].length, 1)
})

test('the form’s words are plain: none of the banned ones, no confidence, no stand-in text', () => {
  const src = readFileSync(join(here, 'notes.jsx'), 'utf8') + readFileSync(join(here, 'notes-actions.js'), 'utf8')
  const drawn = [...src.matchAll(/(?:>|')([A-Z][^<>'{}\n]{12,})(?:<|')/g)].map((m) => m[1]).join(' ')
  for (const word of ['lie', 'cheat', 'violate', 'ignore', 'hallucinat', 'trial', 'Not available in this version']) {
    assert.ok(!new RegExp('\\b' + word, 'i').test(drawn), word + ' in: ' + drawn.slice(0, 300))
  }
  assert.ok(!/\d\s?%/.test(drawn), 'a percentage')
})
