/* C30 P0-2: Approve reads the server's answer, and draws every one.
 *
 * The live screen (index.jsx, the real file) is mounted in jsdom with its one
 * live-input hook replaced by a fixed snapshot and a counting `reload`; every
 * `fetch` is a stub, so nothing here reaches a server or a store. The name is
 * typed, Approve is pressed, and the stub answers exactly as
 * `crates/cli/src/uiserve.rs` `change_decide` does:
 *
 *   200 {acted: {outcome: "moved", seen, now, changed[]}, decisions}   the code moved
 *   200 {acted: {outcome: "recorded", decision}, decisions}            recorded
 *   403 {error: "licence_required", status, reason, enable}            no licence that allows approving
 *   400 {error: "refused", detail}                                     refused, in the engine's own sentence
 *   503 {error: "upgrade_pending", message}                            the store needs an upgrade
 *   (no answer)                                                        unreachable
 *
 * The defect this pins: the answer was thrown away (`.then(live.reload)`), so
 * a person could believe they had approved. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import { changeSnapshot } from '../../data/change.js'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>', { url: 'http://127.0.0.1/' })
globalThis.window = dom.window
globalThis.document = dom.window.document
globalThis.IS_REACT_ACT_ENVIRONMENT = false

const here = dirname(fileURLToPath(import.meta.url))
/* index.jsx, bundled as it ships, except its one live-input module, which is
 * replaced by a stand-in reading `globalThis.__LIVE` (a fixed snapshot, the
 * tracked Change, the licence, and a reload that counts). */
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { createRoot } from 'react-dom/client'
import Screen from './index.jsx'
export function mount(el, at) { const root = createRoot(el); root.render(React.createElement(Screen, { at })); return root }`
  const LIVE_STUB = `
import React from 'react'
export function useLive() { return globalThis.__LIVE }
export const Reading = () => React.createElement('div', null, 'Reading the record…')
export const Said = ({ what }) => React.createElement('div', null, what)`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
    plugins: [{
      name: 'live-stub',
      setup(b) {
        b.onResolve({ filter: /Repair-Loop\/live\.js$/ }, () => ({ path: 'live-stub', namespace: 'stub' }))
        b.onLoad({ filter: /.*/, namespace: 'stub' }, () => ({ contents: LIVE_STUB, loader: 'jsx', resolveDir: here }))
      },
    }],
  })
  const file = join(await mkdtemp(join(tmpdir(), 'approve-answer-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

/* A served ThreadView (API.md shape) and a tracked Change the engine reads as ready. */
const T = (m) => new Date(Date.UTC(2026, 9, 6, 12, m, 0)).toISOString()
const CUR = 'wt-2222222222222222'
function thread() {
  return {
    schema_version: 1,
    project: { id: 'p1', name: 'ledger-api', root: '/r/ledger-api' },
    thread: { id: '0123456789abcdef', title: 'Stop the import double-posting', current_version_id: CUR, whose_move: 'you', proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: [] } },
    prompts: [{ event_id: 'ev-p', session_id: 's1', seq: 1, ts: T(0), text: 'the import posts twice', work: true, version_id: CUR, raw_file: '/m', raw_line: 1 }],
    edits: [{ id: 'ed0', event_id: 'ed0', session_id: 's1', ts: T(2), file: '/r/ledger-api/src/post.py', op: 'edit', lines_added: 1, lines_removed: 0, ok: true, version_id: CUR, raw_file: '/m', raw_line: 3 }],
    commands: [{ id: 'c0#0', event_id: 'c0', part: 0, session_id: 's1', ts: T(3), command: 'pytest -q tests/test_api.py', outcome: 'succeeded', exit_code: 0, is_test: true, is_commit: false, tally: { passed: 4, failed: 0, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 }, version_id: CUR, raw_file: '/m', raw_line: 4 }],
    claims: [],
    versions: [{ id: CUR, commit: null, short: null, session_id: 's1', from_seq: 1, to_seq: 9, committed: false }],
    current_version_id: CUR,
    sessions: [{ id: 's1', ledger_project_id: 'lp', source: 'claude_code', origin: 'person', started_at: T(0), last_at: T(5), file: '/m' }],
    files: [],
    masking: { masked: 0, families: [], complete: true },
  }
}
const SEEN = 'full-rev-abc'
function tracked() {
  return {
    id: 'chg_1',
    change: { accepted_scope: { version: 1, acceptance: { actor: 'sam', accepted_at: T(5) } }, decisions: { history: [] }, code_moved: null },
    model: {
      criteria: [1, 2].map((n) => ({ n, text: 'Line ' + n + ' holds.', state: 'proven', word: 'Proven' })),
      decision: { seen: SEEN, approvedFull: null },
      readiness: { ready: true, sentence: null, waivedLines: [] },
    },
  }
}
/* /api/licence as `licence_cmd::status_body` serves it for a Pro licence. */
const PRO = { status: 'valid', plan: 'pro', expires_on: '2027-10-07', paid_gate: true, headline: 'licence', entitlements: { approve_version: { allowed: true } } }

const DECIDE = '/api/changes/chg_1/decisions'
const ROW = { id: 'cd_01', act: 'approved', target_revision: SEEN, scope_version: 1, actor: 'Robin', decided_at: '2026-10-07T12:00:00Z', withdraws: null, record_path: '/r/ledger-api/.thruline/decisions/' + SEEN + '.json', exceptions: [] }
const ANSWERS = {
  moved: { status: 200, doc: { schema_version: 5, acted: { outcome: 'moved', seen: SEEN, now: 'b2c3d4e5f6a7b8c9d0e1', changed: ['src/post.py', 'src/api.py'] }, decisions: { standing: null, history: [], target_revision: 'b2c3d4e5f6a7b8c9d0e1', scope_version: 1 } } },
  movedUnnamed: { status: 200, doc: { schema_version: 5, acted: { outcome: 'moved', seen: SEEN, now: 'b2c3d4e5f6a7b8c9d0e1', changed: [] }, decisions: { standing: null, history: [], target_revision: 'b2c3d4e5f6a7b8c9d0e1', scope_version: 1 } } },
  recorded: { status: 200, doc: { schema_version: 5, acted: { outcome: 'recorded', decision: ROW }, decisions: { standing: ROW, history: [ROW], target_revision: SEEN, scope_version: 1 } } },
  licence: { status: 403, doc: { error: 'licence_required', status: 'expired', reason: 'This licence expired on 2026-09-01.', enable: 'Renew it.' } },
  refused: { status: 400, doc: { error: 'refused', detail: 'Sam approved this version on 2026-10-07. One approval stands at a time, so withdraw that one before approving again.' } },
  upgrade: { status: 503, doc: { error: 'upgrade_pending', message: 'schema upgrade pending — run `thruline upgrade` (about 35 seconds)' } },
  unreachable: null,
}

/** Let React and the stubbed fetch settle: a bounded number of turns, never a clock. */
async function settle(cond = () => false, turns = 400) {
  for (let i = 0; i < turns && !cond(); i++) await new Promise((r) => setImmediate(r))
}
const textOf = (el) => (el ? el.textContent.replace(/\s+/g, ' ').trim() : '')

/** Mount the live screen, type a name (and a note, when one is given), press Approve, and return what is on screen after the answer. */
async function pressApprove(answer, note = '') {
  const posts = []
  let reloads = 0
  globalThis.fetch = async (url, init = {}) => {
    if ((init.method || 'GET') === 'POST') {
      posts.push({ url, body: JSON.parse(init.body) })
      if (!answer) throw new TypeError('Failed to fetch')
      return { ok: answer.status < 400, status: answer.status, json: async () => answer.doc, text: async () => JSON.stringify(answer.doc) }
    }
    return { ok: true, status: 200, json: async () => ({}), text: async () => '{}' }
  }
  globalThis.__LIVE = {
    snapshot: changeSnapshot(thread(), null, new Date(Date.UTC(2026, 9, 6, 13, 0)), { storageBytes: 3 * 1024 ** 3 }),
    tracked: tracked(),
    licence: PRO,
    error: null,
    reload: () => { reloads += 1 },
  }
  document.body.innerHTML = '<div id="root"></div>'
  const root = M.mount(document.getElementById('root'), { screen: 'Approve-Sheet', change: '0123456789abcdef', project: 'p1' })
  await settle(() => document.getElementById('as-name'))
  const input = document.getElementById('as-name')
  assert.ok(input, 'the sheet is open with its name box')
  const setValue = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set
  setValue.call(input, 'Robin')
  input.dispatchEvent(new window.Event('input', { bubbles: true }))
  if (note) {
    const box = document.getElementById('as-note')
    assert.ok(box && !box.disabled, 'the note box is on')
    Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set.call(box, note)
    box.dispatchEvent(new window.Event('input', { bubbles: true }))
  }
  const approveBtn = () => [...document.querySelectorAll('[role="dialog"] button')].find((b) => /^◇\s*Approve /.test(textOf(b)) && !b.disabled)
  await settle(() => approveBtn())
  assert.ok(approveBtn(), 'Approve is on: ready, Pro, a name in the box')
  approveBtn().dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
  await settle(() => posts.length === 1)
  await settle()
  const out = {
    posts, reloads,
    dialog: textOf(document.querySelector('[role="dialog"]')),
    alert: textOf(document.querySelector('[role="dialog"] [role="alert"]')),
    why: textOf(document.getElementById('as-why')),
    noteBox: document.getElementById('as-note') ? document.getElementById('as-note').value : null,
    approveOn: Boolean(approveBtn()),
    pricing: [...document.querySelectorAll('[role="dialog"] a')].some((a) => a.getAttribute('href') === 'https://thrulinehq.com/pricing'),
    seeWhatChanged: [...document.querySelectorAll('[role="dialog"] a')].some((a) => textOf(a) === 'See what changed'),
    page: textOf(document.body),
  }
  root.unmount()
  return out
}

test('the code moved: the refusal is drawn with its files, and nothing is re-read behind it', async () => {
  const s = await pressApprove(ANSWERS.moved)
  assert.deepEqual(s.posts, [{ url: DECIDE, body: { actor: 'Robin', seen_version: SEEN } }])
  assert.ok(s.alert.includes('Changed while this was open'), 'the refusal alert: ' + s.alert)
  assert.ok(s.alert.includes('Not approved.'), s.alert)
  assert.ok(s.alert.includes('src/post.py, src/api.py'), 'the files the server named: ' + s.alert)
  assert.ok(s.alert.includes('Two files changed since this screen loaded'), s.alert)
  assert.ok(s.alert.includes('b2c3d4e'), 'the version the code is now: ' + s.alert)
  assert.ok(s.seeWhatChanged, 'See what changed is offered')
  assert.equal(s.approveOn, false, 'Approve stays off on the moved code')
  assert.equal(s.reloads, 0, 'a refusal is drawn, never reloaded away')
  assert.ok(!/\b(undefined|null|NaN)\b/.test(s.page), 'a hole on the page: ' + (s.page.match(/.{40}\b(undefined|null|NaN)\b.{20}/) || [''])[0])
})

test('the code moved and the record cannot name the files: it says so, never "No files changed"', async () => {
  const s = await pressApprove(ANSWERS.movedUnnamed)
  assert.ok(s.alert.includes('Changed while this was open'), s.alert)
  assert.ok(!/No files? changed/.test(s.alert), s.alert)
  assert.ok(s.alert.includes('could not name the files'), s.alert)
  assert.equal(s.reloads, 0)
})

test('403 licence_required: the C20 refusal, See plans → thrulinehq.com/pricing, and nothing re-read', async () => {
  const s = await pressApprove(ANSWERS.licence)
  assert.ok(s.why.startsWith('Approving a version needs Pro.'), 'the refusal at the control: ' + s.why)
  assert.ok(s.why.includes('Everything else is free.'), s.why)
  assert.ok(s.pricing, 'See plans ↗ links to the pricing page')
  assert.equal(s.approveOn, false, 'Approve is off after the licence refusal')
  assert.ok(!/trial/i.test(s.page), 'no trial words (C20)')
  assert.equal(s.reloads, 0)
})

test('recorded: drawn as approved from the server’s own row, then the record is re-read', async () => {
  const s = await pressApprove(ANSWERS.recorded)
  assert.ok(s.dialog.includes('Name on the record: Robin'), 'the row the server recorded: ' + s.dialog.slice(0, 400))
  assert.ok(/Approved \S+ · \d\d:\d\d · by you/.test(s.dialog), 'the approval with the row’s own time')
  assert.ok(s.dialog.includes('Withdraw approval'), 'the act that follows an approval')
  assert.equal(s.approveOn, false)
  assert.equal(s.reloads, 1, 'the rest of the record is re-read once, after the answer is drawn')
})

test('any other refusal is drawn at the control, in the server’s words or plain ones; nothing is re-read', async () => {
  const refused = await pressApprove(ANSWERS.refused)
  assert.ok(refused.why.startsWith('Not approved.'), refused.why)
  assert.ok(refused.why.includes('One approval stands at a time'), 'the engine’s own sentence: ' + refused.why)
  assert.equal(refused.reloads, 0)
  const upgrade = await pressApprove(ANSWERS.upgrade)
  assert.ok(upgrade.why.startsWith('Not approved.'), upgrade.why)
  assert.ok(upgrade.why.includes('thruline upgrade'), upgrade.why)
  assert.equal(upgrade.reloads, 0)
  const gone = await pressApprove(ANSWERS.unreachable)
  assert.ok(gone.why.startsWith('Not approved.'), gone.why)
  assert.ok(gone.why.includes('ThruLine did not answer'), gone.why)
  assert.equal(gone.reloads, 0)
})

/* C30 R-LEDGER: the note is kept with the approval. */
const NOTED = { status: 200, doc: { schema_version: 7, acted: { outcome: 'recorded', decision: { ...ROW, note: 'Read the diff by hand.' } }, decisions: { standing: { ...ROW, note: 'Read the diff by hand.' }, history: [{ ...ROW, note: 'Read the diff by hand.' }], target_revision: SEEN, scope_version: 1 } } }

test('a note typed in the box is sent with the approval, and drawn from the row the server recorded', async () => {
  const s = await pressApprove(NOTED, '  Read the diff by hand.\n')
  assert.deepEqual(s.posts, [{ url: DECIDE, body: { actor: 'Robin', seen_version: SEEN, note: 'Read the diff by hand.' } }])
  assert.ok(s.dialog.includes('Note: “Read the diff by hand.”'), 'the note kept with the decision: ' + s.dialog.slice(0, 500))
  assert.ok(!s.dialog.includes('Not available in this version'), s.dialog)
  assert.equal(s.reloads, 1)
})

test('no note typed: no note is sent', async () => {
  const s = await pressApprove(ANSWERS.recorded, '   ')
  assert.deepEqual(s.posts, [{ url: DECIDE, body: { actor: 'Robin', seen_version: SEEN } }])
})

test('a refused approval leaves the words where the person typed them, and says so in plain words when they were too long', async () => {
  const words = 'x'.repeat(2001)
  const tooLong = { status: 409, doc: { error: 'too_long', field: 'note', limit: 2000 } }
  const s = await pressApprove(tooLong, words)
  assert.equal(s.posts[0].body.note, words, 'the words were sent as typed')
  assert.ok(s.why.startsWith('Not approved.'), s.why)
  assert.ok(s.why.includes('at most 2,000 characters'), s.why)
  assert.equal(s.noteBox, words, 'nothing the person typed is thrown away')
  const refused = await pressApprove(ANSWERS.refused, 'my words')
  assert.equal(refused.noteBox, 'my words')
})
