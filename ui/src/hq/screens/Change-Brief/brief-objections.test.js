/* C30 R-LEDGER: the Brief's objections.
 *
 * What a person sees and can do, held by the real files: the ⋯ menu's "This is
 * wrong…" opens a form (objection.jsx) that keeps one objection beside the
 * selected statement through the one address in objection-actions.js, the
 * inspector says "You said this is wrong: …" under the statement beside the
 * verdict it already has, and the artboard in the gallery is exactly what it
 * was. Nothing here reaches a server: every `fetch` is a stub.
 *
 * An objection is words beside a statement. Nothing here, and nothing it feeds,
 * writes a verdict, a count, readiness or an approval; the form says so beside
 * the box and the line says what the verdict still is. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import { recordObjection, objectionAnswerOf, PROJECT_OBJECTIONS } from './objection-actions.js'
import { KEPT_BESIDE, SENTENCE_CHANGED } from '../../data/objections.js'

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
import { BriefObjectionForm } from './objection.jsx'
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
export function mountForm(el, props) { const root = createRoot(el); root.render(React.createElement(BriefObjectionForm, props)); return root }
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
  const file = join(await mkdtemp(join(tmpdir(), 'brief-objections-')), 'brief.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const flush = async (cond = () => false, turns = 400) => {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}
const NOW = new Date(Date.UTC(2026, 9, 7, 12, 0, 0))
const SERVED = [
  { id: 'obj_1', actor: 'sam', at: '2026-10-07T11:30:00Z', reason: 'The test never ran.', matches_sentence: true },
  { id: 'obj_2', actor: 'robin', at: '2026-10-07T11:45:00Z', reason: 'Nor did the build.', matches_sentence: false },
]
/* m1#0 "Tests pass." (the statement the brief is about) carries the objections; m1#1 does not */
const snapshot = (objections = SERVED) => {
  const thread = M.threadFixture()
  if (objections) thread.claims[0].objections = objections
  return M.changeSnapshot(thread, { change: { id: 'chg_1', notes: [] }, hq: null }, NOW, {})
}
const html = (data, acts = {}, state = {}) => M.render({ data, actions: acts, frame: {} }, state)
const textOf = (el) => (el ? el.textContent.replace(/\s+/g, ' ').trim() : '')
const plain = (markup) => markup.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ')
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')

/* ---- what is on the Brief ---- */

test('the inspector says what you said is wrong, with who and when, beside the verdict the statement already has', () => {
  const data = snapshot()
  const verdict = data.statements.find((s) => s.id === 'm1#0').proof.verdict
  assert.equal(verdict, 'not_proven', 'the fixture’s statement has a verdict of its own')
  const t = unescape(plain(html(data, { noteChangeId: 'chg_1' })))
  assert.ok(t.includes('What you said is wrong'), 'the heading')
  assert.ok(t.includes('You said this is wrong: The test never ran.'), t.slice(0, 300))
  assert.ok(t.includes('sam'), 'who')
  assert.ok(/sam · 20\d\d-\d\d-\d\d \d\d:\d\d/.test(t), 'who and when')
  assert.ok(t.includes('You said an earlier wording of this was wrong: Nor did the build.'))
  assert.ok(t.includes(SENTENCE_CHANGED), 'the sentence changed since: said, not carried onto the new wording')
  assert.ok(t.includes('What ThruLine says about this statement is unchanged: Not proven.'))
  assert.ok(t.includes(KEPT_BESIDE))
})

test('an objection moves no verdict: the Brief’s facts are the ones it had without it, line for line', () => {
  const vals = (data) => {
    const v = M.vals({ data, actions: {}, frame: {} }, {})
    // everything the Brief says about the statement, as plain data
    return JSON.stringify({ pill: v.pill, headline: v.headline, stages: v.stages, rows: v.rows, whyVerdict: v.whyVerdict, settle: v.settle, outLines: v.outLines, afterLines: v.afterLines, statusLeft: v.statusLeft, chainCols: v.chainCols, findCount: v.findCount })
  }
  assert.equal(vals(snapshot()), vals(snapshot(null)))
})

test('a statement nobody objected to draws no objections section, and no form until one is asked for', () => {
  const none = unescape(plain(html(snapshot(null), { noteChangeId: 'chg_1' })))
  assert.ok(!none.includes('What you said is wrong'))
  assert.ok(!none.includes('What is wrong with this statement?'))
  // the objection is on m1#0; the inspector on m1#1 shows none of it
  const other = unescape(plain(html(snapshot(), { noteChangeId: 'chg_1' }, { frag: 'm1#1' })))
  assert.ok(!other.includes('You said this is wrong'), 'an objection stays beside its own statement')
})

test('the gallery artboard is what it was: no objections section, no form, and the menu item only closes the menu', () => {
  const g = M.render({}, { more: true })
  assert.ok(!g.includes('bo-reason'))
  assert.ok(!plain(g).includes('What you said is wrong'))
  assert.ok(plain(g).includes('This is wrong…'), 'the canvas’s menu item')
  const v = M.vals({}, { more: true })
  assert.equal(v.objectionLines, null)
  assert.equal(v.objectionForm, null)
  assert.equal(typeof v.openObjection, 'function')
  assert.equal(v.openObjection, v.closeMore, 'in the gallery the item does what it always did')
})

/* ---- the control ---- */

test('"This is wrong…" is the third menu item and the control that opens the form; the other two are left as they were', async () => {
  const pressed = []
  document.body.innerHTML = '<div id="root"></div>'
  const root = M.mountTemplate(document.getElementById('root'), { data: snapshot(null), actions: { noteChangeId: 'chg_1' }, frame: {} }, { more: true }, {
    openNote: () => pressed.push('openNote'),
    closeMore: () => pressed.push('closeMore'),
    openObjection: () => pressed.push('openObjection'),
  })
  await flush(() => document.querySelector('[role="menu"]'))
  const items = [...document.querySelectorAll('[role="menuitem"]')]
  assert.deepEqual(items.map(textOf), ['Add a note', 'Open in editor', 'This is wrong…'])
  for (const item of items) item.dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await flush(() => pressed.length >= 3)
  assert.deepEqual(pressed, ['openNote', 'closeMore', 'openObjection'])
  root.unmount()
})

test('asked for, the form shows the sentence, says what the objection does not do, and offers Keep and Cancel', () => {
  const open = html(snapshot(null), {}, { objectOpen: true })
  assert.ok(open.includes('id="bo-reason"'), 'the box')
  const t = unescape(plain(open))
  assert.ok(t.includes('What is wrong with this statement?'))
  assert.ok(t.includes('“Tests pass.”'), 'the sentence being objected to is shown')
  assert.ok(t.includes('Your objection is kept beside this statement. It does not change what ThruLine says about it.'))
  assert.ok(t.includes('Keep objection') && t.includes('Cancel'))
  assert.ok(!t.includes('Not available in this version'))
})

test('the form belongs to the statement it was opened on: another statement gets a fresh one', () => {
  const data = snapshot(null)
  assert.equal(M.vals({ data, actions: {}, frame: {} }, { objectOpen: true }).objectionForm.key, 'm1#0')
  assert.equal(M.vals({ data, actions: {}, frame: {} }, { objectOpen: true, frag: 'm1#1' }).objectionForm.key, 'm1#1')
})

/* ---- the form ---- */

const typeInto = (box, value) => {
  Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set.call(box, value)
  box.dispatchEvent(new window.Event('input', { bubbles: true }))
}

async function openForm(keepIt, extra = {}) {
  const calls = { closed: 0, reloaded: 0, kept: [] }
  document.body.innerHTML = '<div id="root"></div>'
  const root = M.mountForm(document.getElementById('root'), {
    open: true, sentence: 'Tests pass.',
    onClose: () => { calls.closed += 1 },
    reload: () => { calls.reloaded += 1 },
    keep: (reason) => { calls.kept.push(reason); return keepIt(reason) },
    ...extra,
  })
  await flush(() => document.getElementById('bo-reason'))
  const box = document.getElementById('bo-reason')
  assert.ok(box, 'the box is on the page')
  const keep = () => [...document.querySelectorAll('button')].find((b) => /^Keep objection|^Keeping/.test(textOf(b)))
  return { calls, box, keep, root }
}

test('Keep objection sends the words once, then closes the form and re-reads the record', async () => {
  const { calls, box, keep, root } = await openForm(async () => ({ ok: true, status: 200, doc: { schema_version: 1, objection: { id: 'obj_9' } } }))
  assert.equal(keep().disabled, true, 'nothing to keep yet')
  typeInto(box, '  The test never ran.  ')
  await flush(() => !keep().disabled)
  keep().dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await flush(() => calls.reloaded === 1)
  assert.deepEqual(calls.kept, ['  The test never ran.  '], 'the words as typed; recordObjection trims them on the way out')
  assert.equal(calls.closed, 1)
  assert.equal(calls.reloaded, 1)
  root.unmount()
})

test('a refusal is drawn in plain words and every word the person typed stays in the box', async () => {
  const { calls, box, keep, root } = await openForm(async () => ({ ok: false, status: 404, doc: { error: 'no_such_statement' } }))
  typeInto(box, 'my words')
  await flush(() => !keep().disabled)
  keep().dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await flush(() => document.querySelector('[role="alert"]'))
  assert.match(textOf(document.querySelector('[role="alert"]')), /^Not kept\. ThruLine has no record of this statement\./)
  assert.equal(box.value, 'my words', 'nothing the person typed is lost')
  assert.equal(calls.closed, 0)
  assert.equal(calls.reloaded, 0)
  root.unmount()
})

test('a thrown request is drawn as "did not answer", never as kept', async () => {
  const { calls, box, keep, root } = await openForm(async () => { throw new Error('network') })
  typeInto(box, 'words')
  await flush(() => !keep().disabled)
  keep().dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await flush(() => document.querySelector('[role="alert"]'))
  assert.match(textOf(document.querySelector('[role="alert"]')), /did not answer/)
  assert.equal(calls.reloaded, 0)
  root.unmount()
})

test('a reason over 2,000 characters says how many too many and cannot be kept', async () => {
  const { box, keep, root } = await openForm(async () => ({ ok: true, status: 200, doc: { objection: {} } }))
  typeInto(box, 'x'.repeat(2005))
  await flush(() => document.querySelector('[role="status"]'))
  assert.match(textOf(document.querySelector('[role="status"]')), /5 too many: a reason can be at most 2,000 characters/)
  assert.equal(keep().disabled, true)
  root.unmount()
})

test('through the whole Brief, Keep objection hands the SELECTED statement’s id and the words to the one act, then re-reads', async () => {
  const given = []
  let reloaded = 0
  document.body.innerHTML = '<div id="root"></div>'
  const root = M.mountTemplate(document.getElementById('root'), {
    data: snapshot(null), frame: {},
    actions: {
      keepObjection: (statement, reason) => { given.push([statement, reason]); return Promise.resolve({ ok: true, status: 200, doc: { objection: { id: 'o' } } }) },
      reload: () => { reloaded += 1 },
    },
  }, { objectOpen: true, frag: 'm1#1' })
  await flush(() => document.getElementById('bo-reason'))
  typeInto(document.getElementById('bo-reason'), 'Not fixed.')
  const keep = () => [...document.querySelectorAll('button')].find((b) => /^Keep objection/.test(textOf(b)))
  await flush(() => keep() && !keep().disabled)
  keep().dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await flush(() => reloaded === 1)
  assert.deepEqual(given, [['m1#1', 'Not fixed.']])
  root.unmount()
})

/* ---- the act ---- */

test('the act sends exactly {statement, reason} to the one address, trimmed, and nothing without words or a statement', async () => {
  const sent = []
  const stub = async (url, init) => { sent.push([url, init]); return { ok: true, status: 200, json: async () => ({ objection: { id: 'o' } }) } }
  const res = await recordObjection({ project: 'p 1', statement: 'm1#0', reason: '  Wrong.  ' }, stub)
  assert.equal(res.status, 200)
  assert.equal(sent.length, 1)
  assert.equal(sent[0][0], '/api/hq/projects/p%201/objections')
  assert.equal(sent[0][0], PROJECT_OBJECTIONS('p 1'))
  assert.equal(sent[0][1].method, 'POST')
  assert.deepEqual(JSON.parse(sent[0][1].body), { statement: 'm1#0', reason: 'Wrong.' }, 'no digest, no sentence, no verdict, no name')
  for (const bad of [{ project: '', statement: 's', reason: 'r' }, { project: 'p', statement: '', reason: 'r' }, { project: 'p', statement: 's', reason: '   ' }, { project: 'p', statement: 's' }]) {
    const none = await recordObjection(bad, stub)
    assert.deepEqual(none, { ok: false, status: 0, doc: { error: 'not_available' } })
  }
  assert.equal(sent.length, 1, 'nothing else was sent')
})

test('every answer the server can give is read, in plain words, and none is dropped', () => {
  const cases = [
    [{ status: 400, doc: { error: 'reason_required' } }, 'Say what is wrong with this statement.'],
    [{ status: 400, doc: { error: 'malformed_request' } }, 'could not read the request'],
    [{ status: 403, doc: { error: 'same_origin_required' } }, 'only from its own page'],
    [{ status: 404, doc: { error: 'no_such_project' } }, 'no record of this project'],
    [{ status: 404, doc: { error: 'no_such_statement' } }, 'no record of this statement'],
    [{ status: 409, doc: { error: 'too_long', field: 'reason', limit: 2000 } }, 'at most 2,000 characters'],
    [{ status: 413, doc: null }, 'at most 2,000 characters'],
    [{ status: 503, doc: { error: 'upgrade_pending' } }, 'thruline upgrade'],
    [{ status: 503, doc: { error: 'store_busy' } }, 'busy'],
    [{ status: 0, doc: { error: 'unreachable' } }, 'did not answer'],
    [{ status: 500, doc: null }, 'answered 500'],
    [{ status: 200, doc: {} }, 'cannot read'],
  ]
  for (const [res, words] of cases) {
    const a = objectionAnswerOf({ ok: false, ...res })
    assert.equal(a.kind, 'refused', JSON.stringify(res))
    assert.ok(a.detail.includes(words), `${JSON.stringify(res)} → ${a.detail}`)
    assert.ok(!/\b(lie|cheat|violate|ignore|hallucinat)/i.test(a.detail), a.detail)
  }
  assert.equal(objectionAnswerOf(null).kind, 'refused')
  assert.equal(objectionAnswerOf({ ok: true, status: 200, doc: { objection: { id: 'o' } } }).kind, 'kept')
})

test('census: in this folder only the two action files write, each reaches exactly one address, and this one takes none back', () => {
  const files = readdirSync(here).filter((n) => /\.(js|jsx)$/.test(n) && !n.endsWith('.test.js'))
  for (const n of files.filter((n) => n !== 'notes-actions.js' && n !== 'objection-actions.js')) {
    assert.ok(!/method:\s*['"](POST|PUT|PATCH|DELETE)['"]/.test(readFileSync(join(here, n), 'utf8')), `${n} must not write`)
  }
  const a = readFileSync(join(here, 'objection-actions.js'), 'utf8')
  const addresses = [...a.matchAll(/'\/api\/hq\/projects\/' \+ encodeURIComponent\(project\) \+ '([^']*)'/g)].map((m) => m[1])
  assert.deepEqual(addresses, ['/objections'])
  assert.equal([...a.matchAll(/method:\s*'POST'/g)].length, 1)
  assert.ok(!/withdraw|DELETE/i.test(a.replace(/\/\*[\s\S]*?\*\//, '')), 'taking one back is not offered (C34 Q16)')
  assert.ok(!/withdraw/i.test(readFileSync(join(here, 'objection.jsx'), 'utf8').replace(/\/\*[\s\S]*?\*\//, '')))
})

test('the form’s words are plain: none of the banned ones, no confidence, no stand-in text', () => {
  const src = readFileSync(join(here, 'objection.jsx'), 'utf8') + readFileSync(join(here, 'objection-actions.js'), 'utf8')
  const drawn = [...src.matchAll(/(?:>|')([A-Z][^<>'{}\n]{12,})(?:<|')/g)].map((m) => m[1]).join(' ')
  for (const word of ['lie', 'cheat', 'violate', 'ignore', 'hallucinat', 'trial', 'Not available in this version']) {
    assert.ok(!new RegExp('\\b' + word, 'i').test(drawn), word + ' in: ' + drawn.slice(0, 300))
  }
  assert.ok(!/\d\s?%/.test(drawn), 'a percentage')
})
