/* C35: "Confirm on this Mac" on the screens that offer an act on a decision.
 *
 * What a person sees and can do, held by the real files: the Inbox's "Decision
 * broken" preview (set it aside for this version, retire it) and the Change
 * Brief's proposed decisions (accept) each keep their terminal command and, only
 * when the list says the native transport is available, add the control that
 * walks the flow in data/decision-confirm.js. Nothing here reaches a server:
 * `fetch` is a stub, and a click in the page is never a confirmation.
 *
 * Synthetic fixtures only (data/decisions-fixture.js). */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import { DECISIONS_CHANGED, decisionsModel } from '../data/decisions.js'
import { DECISIONS_LIST, TRANSPORT_NATIVE, TRANSPORT_TERMINAL, TRANSPORT_UNAVAILABLE, listWith } from '../data/decisions-fixture.js'

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
import { DecisionPreview } from './Inbox/decisions.jsx'
import { BriefDecisions, briefDecisions } from './Change-Brief/decisions.jsx'
export { React, renderToStaticMarkup, DecisionPreview, BriefDecisions, briefDecisions }
export function mount(el, element) { const root = createRoot(el); root.render(element); return root }`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-decision-confirm-')), 'confirm.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const COMMIT = '3c3ca28a1b2c3d4e5f60718293a4b5c6d7e8f901'
const SHA = 'd'.repeat(64)
const model = (transport = TRANSPORT_NATIVE) => decisionsModel({ status: 'served', doc: listWith({ decisions: DECISIONS_LIST.decisions, transport }) })
const rowOf = (m, id) => m.rows.find((r) => r.id === id)
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const plain = (markup) => unescape(markup).replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ')
const textOf = (el) => (el ? el.textContent.replace(/\s+/g, ' ').trim() : '')
const flush = async (cond = () => false, turns = 600) => {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}
const click = (el) => el.dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
const type = (el, value) => {
  const proto = el.tagName === 'TEXTAREA' ? window.HTMLTextAreaElement.prototype : window.HTMLInputElement.prototype
  Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, value)
  el.dispatchEvent(new window.Event('input', { bubbles: true }))
}
const button = (label) => [...document.querySelectorAll('button')].find((b) => textOf(b) === label)
const buttonStarting = (label) => [...document.querySelectorAll('button')].find((b) => textOf(b).startsWith(label))

const BANNED = [/\blie\b/i, /\bcheat/i, /\bviolat/i, /\bignor/i, /\bhallucinat/i, /\btrial\b/i, /\d\s*%/, /\b(UTC|GMT|PST|IST)\b/, /\/Users\//]

/* ---- what is drawn, and when ---- */

const inbox = (m) => plain(M.renderToStaticMarkup(M.React.createElement(M.DecisionPreview, { row: rowOf(m, 'dec-01') })))
const brief = (m, candidate = 'wt-3c3ca28a1b2c3d4e') => plain(M.renderToStaticMarkup(M.React.createElement(M.BriefDecisions, { view: M.briefDecisions(m, candidate) })))

test('on a native transport the Inbox preview keeps both terminal commands and adds "Confirm on this Mac" to each', () => {
  const t = inbox(model())
  assert.match(t, /thruline decide prepare waive dec-01 --version-id wt-3c3ca28a1b2c3d4e/)
  assert.match(t, /thruline decide prepare retire dec-01/)
  assert.equal((t.match(/Confirm on this Mac/g) || []).length, 2, 'one control for the waiver, one for the retirement')
  assert.match(t, /Or confirm it in a window on this Mac/)
  assert.match(t, /HQ copies the command; it records nothing and runs nothing\./, 'the sentence about the copied command stays')
})

test('on a native transport the Brief offers a proposed decision’s accept the same way, and no accepted decision’s', () => {
  const t = brief(model())
  assert.ok(unescape(M.renderToStaticMarkup(M.React.createElement(M.BriefDecisions, { view: M.briefDecisions(model(), 'wt-3c3ca28a1b2c3d4e') }))).includes('thruline decide prepare accept dec-05 --actor <your name> --because <why>'))
  assert.equal((t.match(/Confirm on this Mac/g) || []).length, 1, 'only the proposal')
  assert.match(t, /Accept it in your own terminal/)
  // the proposal has no severity yet: the person chooses, and the row says so
  assert.match(t, /You choose whether it blocks approval or only warns when you accept it/)
  assert.match(t, /Proposed\. Not in force until you accept it and confirm it yourself\./)
})

test('when the transport is not native_click there is no control, the server’s reason is said as given, and the terminal path stays', () => {
  for (const [transport, reason] of [
    [TRANSPORT_TERMINAL, 'The native review needs ThruLine to run in the foreground of its own terminal window.'],
    [TRANSPORT_UNAVAILABLE, 'The native review is only available on macOS.'],
  ]) {
    const i = inbox(model(transport))
    assert.doesNotMatch(i, /Confirm on this Mac|Or confirm it in a window/)
    assert.match(i, new RegExp('not available here\\. ' + reason.replace(/[.]/g, '\\.')))
    assert.match(i, /thruline decide prepare waive dec-01/)
    assert.match(i, /thruline decide prepare retire dec-01/)
    const b = brief(model(transport))
    assert.doesNotMatch(b, /Confirm on this Mac/)
    assert.match(b, /not available here\./)
    assert.match(b, /thruline decide prepare accept dec-05/)
  }
  // the word says native_click, the native preflight says no: still no control
  const half = { ...TRANSPORT_NATIVE, native: { schema_version: 1, available: false, reason: 'The preflight said no.' }, reason: null }
  const t = inbox(model(half))
  assert.doesNotMatch(t, /Confirm on this Mac/)
  assert.match(t, /not available here\. The preflight said no\./)
  // no transport served at all (an older server): no control, and nothing said about one
  const bare = decisionsModel({ status: 'served', doc: (() => { const d = JSON.parse(JSON.stringify(DECISIONS_LIST)); delete d.confirmation_transport; return d })() })
  const n = inbox(bare)
  assert.doesNotMatch(n, /Confirm on this Mac|not available here/)
  assert.match(n, /thruline decide prepare retire dec-01/)
})

test('a dormant gate is said in the Brief with the server’s own reason, and blocking decisions are not drawn as blocking', () => {
  const dormant = decisionsModel({ status: 'served', doc: JSON.parse(JSON.stringify(listWith({ decisions: DECISIONS_LIST.decisions, gate: { active: false, reason: 'Decisions approval is dormant in this record.', members: [] } }))) })
  const t = brief(dormant)
  assert.match(t, /Decisions you set do not block approving right now\. Decisions approval is dormant in this record\./)
  assert.match(t, /Set to block approval/)
  assert.doesNotMatch(t, /Blocks approval/)
  // an active gate says nothing of the kind
  assert.doesNotMatch(brief(model()), /do not block approving right now/)
  assert.match(brief(model()), /Blocks approval/)
})

test('the design gallery and a server without decisions draw no control', () => {
  assert.equal(M.briefDecisions(undefined, 'wt-3c3ca28a1b2c3d4e'), null)
  assert.equal(M.briefDecisions(decisionsModel(undefined), 'wt-3c3ca28a1b2c3d4e'), null)
  const quiet = plain(M.renderToStaticMarkup(M.React.createElement(M.BriefDecisions, { view: M.briefDecisions(decisionsModel({ status: 'not_served' }), 'wt-3c3ca28a1b2c3d4e') })))
  assert.doesNotMatch(quiet, /Confirm on this Mac/)
})

/* ---- the walk, as a person takes it ---- */

function server(over = {}) {
  const calls = []
  let polls = 0
  const f = async (url, init = {}) => {
    const method = init.method || 'GET'
    const body = init.body ? JSON.parse(init.body) : undefined
    calls.push({ method, url, body, token: (init.headers || {})['X-ThruLine-Run-Token'] })
    const key = method + ' ' + url
    const hit = Object.keys(over).find((k) => key.endsWith(k))
    const answer = hit ? (typeof over[hit] === 'function' ? over[hit](body, ++polls) : over[hit]) : null
    const a = answer || (() => {
      if (key.endsWith('/confirmation-token')) return { status: 200, doc: { schema_version: 1, token: 'tok-1' } }
      if (key.endsWith('/prepare')) return { status: 200, doc: { schema_version: 1, action: body.action, act_row_id: 'act-9', state: 'pending_bridge', preparation_is_person_act: false } }
      if (key.endsWith('/review')) return { status: 200, doc: { schema_version: 1, review_sha256: SHA, review: { schema_version: 1, project_id: 'lp1', decision_id: 'dec-x', action: body.action, act_row_id: body.act_row_id, decision: { revision: 1, revision_basis: 'act_revision', kind: 'ui_copy_absent', text: 'Never show a home-folder path on any screen', params: { terms: ['home-folder'] }, params_sha256: 'c0ffee00'.repeat(8) }, confirmation: { thruline_decision_act: { action: body.action } }, supersession: null, summary: 'Review the ' + body.action + ' action and the displayed decision revision before confirming.' }, preparation_is_person_act: false } }
      if (key.endsWith('/native-confirm')) return { status: 202, doc: { schema_version: 1, job_id: 'job-1', kind: 'native_confirmation', project_id: 'p1', decision_id: 'dec-x', act_row_id: 'act-9', state: 'running', receipt_id: null, errors: [], progress: { label: 'Review in the native window', unit: 'person_action', completed: 0, total: 1 } } }
      return { status: 404, doc: { schema_version: 1, error: 'job_not_found', detail: null } }
    })()
    return { ok: a.status >= 200 && a.status < 300, status: a.status, json: async () => a.doc }
  }
  return { f, calls }
}
const JOB_DONE = { status: 200, doc: { schema_version: 1, job_id: 'job-1', kind: 'native_confirmation', project_id: 'p1', decision_id: 'dec-x', act_row_id: 'act-9', state: 'complete', receipt_id: 'rcpt-1', derived_written: true, errors: [], progress: { label: 'Done', unit: 'person_action', completed: 1, total: 1 } } }
const JOB_RUNNING = { status: 200, doc: { ...JOB_DONE.doc, state: 'running', receipt_id: null, derived_written: undefined, progress: { label: 'Review in the native window', unit: 'person_action', completed: 0, total: 1 } } }

async function open(element, { fetchOver } = {}) {
  const s = server(fetchOver)
  globalThis.fetch = s.f
  document.body.innerHTML = '<div id="root"></div>'
  const root = M.mount(document.getElementById('root'), element)
  await flush(() => document.querySelector('button'))
  return { ...s, root }
}
const inboxPreview = () => M.React.createElement(M.DecisionPreview, { row: rowOf(model(), 'dec-01') })
const briefRows = () => M.React.createElement(M.BriefDecisions, { view: M.briefDecisions(model(), 'wt-3c3ca28a1b2c3d4e') })
/* real time, not turns: the flow waits between reads of a running job */
const waitFor = async (cond, ms = 8000) => {
  const t0 = Date.now()
  while (!cond() && Date.now() - t0 < ms) await new Promise((r) => setTimeout(r, 5))
}

test('retire from the Inbox: the form, the exact review in full, then the window; every step one request, and no word of "accepted" until the receipt', async () => {
  let n = 0
  const events = []
  window.addEventListener(DECISIONS_CHANGED, () => events.push('changed'))
  const { calls, root } = await open(inboxPreview(), {
    fetchOver: { 'check?job_id=job-1': () => (++n < 2 ? JOB_RUNNING : JOB_DONE) },
  })
  // collapsed: one control per act, nothing sent
  assert.equal([...document.querySelectorAll('button')].filter((b) => textOf(b) === 'Confirm on this Mac').length, 2)
  assert.deepEqual(calls, [])
  // open the retirement's form (the second control)
  click([...document.querySelectorAll('button')].filter((b) => textOf(b) === 'Confirm on this Mac')[1])
  await waitFor(() => document.querySelector('input[type="text"]'))
  const panel = textOf(document.querySelector('section[aria-label="Retire this rule on this Mac"]'))
  assert.match(panel, /A click in this page is not your confirmation\./)
  assert.match(panel, /The confirmation is the Confirm control in the separate window that opens on this Mac, followed by the Mac’s own presence check\./)
  assert.match(panel, /Preparing adds a pending line to ThruLine’s record; it does nothing until you confirm it in that window\./)
  // no default reason, no default name: Prepare is off until both are typed
  assert.equal(button('Prepare the review').disabled, true)
  type(document.querySelector('input[type="text"]'), 'Robin')
  type(document.querySelector('textarea'), 'This rule is replaced by the new one')
  await waitFor(() => !button('Prepare the review').disabled)
  assert.deepEqual(calls, [], 'typing sends nothing')
  click(button('Prepare the review'))
  await waitFor(() => document.querySelector('pre'))
  // prepare, then the review read back; the window has NOT been asked for
  assert.deepEqual(calls.map((c) => c.method + ' ' + c.url.replace('/api/hq/', '')), [
    'GET decisions/confirmation-token',
    'POST projects/p1/decisions/dec-01/prepare',
    'POST projects/p1/decisions/dec-01/review',
  ])
  assert.deepEqual(calls[1].body, { schema_version: 1, action: 'retire', actor: 'Robin', because: 'This rule is replaced by the new one' })
  assert.deepEqual(calls[2].body, { schema_version: 1, action: 'retire', act_row_id: 'act-9' })
  const shown = textOf(document.querySelector('section[aria-label="The exact review"]'))
  assert.match(shown, /Review the retire action and the displayed decision revision before confirming\./)
  assert.match(shown, /Never show a home-folder path on any screen/)
  assert.ok(shown.includes('Review digest (SHA-256): ' + SHA))
  assert.ok(document.querySelector('pre').textContent.includes('"params_sha256"'), 'every field of the review, as returned')
  assert.match(textOf(document.querySelector('[role="status"]')), /A pending line is in the record \(act-9\)\. It does nothing until you confirm it on this Mac\./)
  assert.ok(buttonStarting('Open the window on this Mac'), 'offered only now that the review is on screen')
  assert.match(textOf(document.querySelector('section[aria-label="Retire this rule on this Mac"]')), /Closing this leaves the pending line unconfirmed/)
  // ask for the window: this sends the digest of the review that was shown, and nothing else
  click(buttonStarting('Open the window on this Mac'))
  await waitFor(() => calls.some((c) => c.url.includes('job_id')))
  assert.deepEqual(calls[3].body, { schema_version: 1, action: 'retire', act_row_id: 'act-9', expected_review_sha256: SHA })
  for (const k of ['presence', 'uid', 'nonce', 'result', 'actor', 'because']) assert.ok(!(k in calls[3].body), k)
  assert.equal(calls[3].token, 'tok-1')
  // while it runs, the page says it is waiting and never says it was done
  await waitFor(() => /Waiting for you in the window on this Mac/.test(textOf(document.querySelector('[role="status"]'))))
  const running = textOf(document.querySelector('[role="status"]'))
  assert.match(running, /^Review in the native window\. Waiting for you in the window on this Mac\. Nothing is recorded until you choose Confirm there and pass the Mac’s own presence check\.$/)
  assert.doesNotMatch(running, /You retired|recorded as receipt/)
  assert.deepEqual(events, [], 'nothing is re-read as changed while it runs')
  // the job names a receipt: now, and only now, the record says so
  await waitFor(() => /recorded as receipt rcpt-1/.test(textOf(document.querySelector('section[aria-label="Retire this rule on this Mac"]'))))
  assert.match(textOf(document.querySelector('[role="status"]')), /^You retired this rule\. It is recorded as receipt rcpt-1\. The decisions file in the project was refreshed\.$/)
  assert.deepEqual(events, ['changed'], 'the screens re-read the list once, after the receipt')
  root.unmount()
})

test('accept from the Brief: the person chooses blocks or warns (no default), and the body names the revision and the exact commit', async () => {
  const { calls, root } = await open(briefRows())
  click(buttonStarting('Confirm on this Mac'))
  await waitFor(() => document.querySelector('input[type="text"]'))
  type(document.querySelector('input[type="text"]'), 'Robin')
  type(document.querySelector('textarea'), 'It is how we write times')
  await waitFor(() => button('Prepare the review').getAttribute('aria-disabled') === 'true')
  assert.equal(button('Prepare the review').disabled, true, 'named and reasoned, but neither blocks nor warns chosen')
  assert.deepEqual([...document.querySelectorAll('input[type="radio"]')].map((r) => r.checked), [false, false])
  assert.match(textOf(document.querySelector('fieldset')), /When the record shows it broken\s*It blocks approval\s*It only warns/)
  assert.match(textOf(document.querySelector('section[aria-label="Accept this rule on this Mac"]')), /It applies from 3c3ca28, the version ThruLine is reading now\./)
  click(document.querySelectorAll('input[type="radio"]')[1])
  await waitFor(() => !button('Prepare the review').disabled)
  click(button('Prepare the review'))
  await waitFor(() => document.querySelector('pre'))
  assert.deepEqual(calls[1].body, { schema_version: 1, action: 'accept', actor: 'Robin', because: 'It is how we write times', revision: 1, severity: 'warns', applies_from_version: COMMIT })
  root.unmount()
})

test('every refusal the server sends is drawn with its text, and the person can try again', async () => {
  // the window is busy
  const busy = await open(inboxPreview(), { fetchOver: { '/native-confirm': { status: 409, doc: { schema_version: 1, error: 'native_confirmation_busy', detail: null } } } })
  click(buttonStarting('Confirm on this Mac'))
  await waitFor(() => document.querySelector('input[type="text"]'))
  type(document.querySelector('input[type="text"]'), 'Robin')
  type(document.querySelector('textarea'), 'Because')
  await waitFor(() => !button('Prepare the review').disabled)
  click(button('Prepare the review'))
  await waitFor(() => buttonStarting('Open the window on this Mac'))
  click(buttonStarting('Open the window on this Mac'))
  await waitFor(() => document.querySelector('[role="alert"]'))
  assert.equal(textOf(document.querySelector('[role="alert"]')), 'Not done. Another confirmation window is already open on this Mac. Finish or close it, then try again. Nothing was confirmed.')
  assert.ok(buttonStarting('Open the window on this Mac'), 'the review stays, and so does the button')
  busy.root.unmount()
  // a prepare the server refuses stays on the form with the server's text
  const refused = await open(inboxPreview(), { fetchOver: { '/prepare': { status: 400, doc: { schema_version: 1, error: 'exact_version_required', detail: 'No commit named 3c3ca28 in this project.' } } } })
  click(buttonStarting('Confirm on this Mac'))
  await waitFor(() => document.querySelector('input[type="text"]'))
  type(document.querySelector('input[type="text"]'), 'Robin')
  type(document.querySelector('textarea'), 'Because')
  await waitFor(() => !button('Prepare the review').disabled)
  click(button('Prepare the review'))
  await waitFor(() => document.querySelector('[role="alert"]'))
  assert.equal(textOf(document.querySelector('[role="alert"]')), 'Not done. ThruLine could not resolve that version to an exact commit. No commit named 3c3ca28 in this project. Nothing was prepared.')
  assert.ok(document.querySelector('input[type="text"]'), 'still the form, and what was typed is kept')
  assert.equal(document.querySelector('input[type="text"]').value, 'Robin')
  // native confirmation unavailable: the server's own reason, verbatim
  refused.root.unmount()
  const gone = await open(inboxPreview(), { fetchOver: { '/native-confirm': { status: 409, doc: { schema_version: 1, error: 'native_confirmation_unavailable', detail: 'The native review needs a foreground owned terminal.' } } } })
  click(buttonStarting('Confirm on this Mac'))
  await waitFor(() => document.querySelector('input[type="text"]'))
  type(document.querySelector('input[type="text"]'), 'Robin')
  type(document.querySelector('textarea'), 'Because')
  await waitFor(() => !button('Prepare the review').disabled)
  click(button('Prepare the review'))
  await waitFor(() => buttonStarting('Open the window on this Mac'))
  click(buttonStarting('Open the window on this Mac'))
  await waitFor(() => document.querySelector('[role="alert"]'))
  assert.match(textOf(document.querySelector('[role="alert"]')), /not available right now\. The native review needs a foreground owned terminal\. Nothing was confirmed\.$/)
  gone.root.unmount()
})

test('a job that fails, or ends without a receipt, is drawn as it is: nothing recorded, nothing called accepted', async () => {
  const failed = await open(inboxPreview(), { fetchOver: { 'check?job_id=job-1': { status: 200, doc: { ...JOB_DONE.doc, state: 'failed', receipt_id: null, derived_written: undefined, errors: ['cancelled in the window'] } } } })
  click(buttonStarting('Confirm on this Mac'))
  await waitFor(() => document.querySelector('input[type="text"]'))
  type(document.querySelector('input[type="text"]'), 'Robin')
  type(document.querySelector('textarea'), 'Because')
  await waitFor(() => !button('Prepare the review').disabled)
  click(button('Prepare the review'))
  await waitFor(() => buttonStarting('Open the window on this Mac'))
  click(buttonStarting('Open the window on this Mac'))
  await waitFor(() => document.querySelector('[role="alert"]'))
  assert.equal(textOf(document.querySelector('[role="alert"]')), 'Not confirmed. cancelled in the window No receipt was recorded.')
  assert.ok(button('Start over'))
  failed.root.unmount()
  // a receipt whose project copy did not refresh
  const stale = await open(inboxPreview(), { fetchOver: { 'check?job_id=job-1': { status: 200, doc: { ...JOB_DONE.doc, derived_written: false, errors: ['could not write the decisions file'] } } } })
  click(buttonStarting('Confirm on this Mac'))
  await waitFor(() => document.querySelector('input[type="text"]'))
  type(document.querySelector('input[type="text"]'), 'Robin')
  type(document.querySelector('textarea'), 'Because')
  await waitFor(() => !button('Prepare the review').disabled)
  click(button('Prepare the review'))
  await waitFor(() => buttonStarting('Open the window on this Mac'))
  click(buttonStarting('Open the window on this Mac'))
  await waitFor(() => document.querySelector('[role="alert"]'))
  const said = textOf(document.querySelector('[role="alert"]'))
  assert.match(said, /the receipt is recorded \(rcpt-1\)\. The decisions file in the project did not refresh: could not write the decisions file/)
  assert.doesNotMatch(said, /was refreshed/)
  stale.root.unmount()
})

test('every word the control draws, in every state it reaches, passes the copy rules', async () => {
  const { root } = await open(inboxPreview(), { fetchOver: { 'check?job_id=job-1': JOB_RUNNING } })
  const panel = () => textOf(document.querySelector('section[aria-label$="on this Mac"]'))
  const seen = [textOf(document.querySelectorAll('button')[2].parentElement.parentElement)]
  click(buttonStarting('Confirm on this Mac'))
  await waitFor(() => document.querySelector('input[type="text"]'))
  seen.push(panel())
  type(document.querySelector('input[type="text"]'), 'Robin')
  type(document.querySelector('textarea'), 'Because')
  await waitFor(() => !button('Prepare the review').disabled)
  click(button('Prepare the review'))
  await waitFor(() => buttonStarting('Open the window on this Mac'))
  seen.push(panel())
  click(buttonStarting('Open the window on this Mac'))
  await waitFor(() => /Waiting for you/.test(panel()))
  seen.push(panel())
  root.unmount()
  const all = seen.join(' ')
  assert.ok(all.length > 800, all.length + '')
  for (const w of BANNED) assert.doesNotMatch(all, w)
  assert.doesNotMatch(all, /leaves this machine/i)
})
