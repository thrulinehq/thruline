/* A3-F8a — no single key records an approval.
 *
 * Before this card, once a name was in the box and focus had left it, ONE
 * press of `.` on the Change screen POSTed `/api/changes/<id>/decisions`: the
 * shell's `.` presses the screen's one primary (`altitude.js` `pressPrimary`),
 * the Brief marks its whole Inspector as the primary's box, and Approve was a
 * `.ws-change-primary` inside it. A hash-chained, named approval a person never
 * meant to give.
 *
 * What this file holds the product to, driven over the REAL Change shell in
 * jsdom with every write counted:
 *
 *   (a) with a name typed and Approve on, no key pressed on the page records
 *       anything — `.`, Enter, Space, ⌘↵, Ctrl+↵ and every key in the shell's
 *       map — and `.` says, in one copy string, how approving is done;
 *   (b) Withdraw is never pressed by `.` either, and a click still withdraws;
 *   (c) the primaries `.` pressed before are still pressed;
 *   and the confirming act: Approve opens a sheet and records nothing; inside
 *       the sheet Enter, Space, `.` and every mapped key record nothing, Esc
 *       closes it; only ⌘↵ / Ctrl+↵ inside it, or its own button, records —
 *       exactly once, to the one decide address.
 *
 * Every fixture is synthetic. No string here came off a real machine.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-approve-keys-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { createRoot } from 'react-dom/client';
  import { flushSync } from 'react-dom';
  import { ChangeShell } from './change-workspace.jsx';
  import { buildChangeView } from './change-view.js';
  import { deriveChangeModel } from './change-model.js';
  import { A11Y_CHANGE, A11Y_COVERAGE } from './a11y-fixture.js';
  import { HQ_COPY } from './hq-copy.js';
  import { KEYMAP, pressPrimary } from './altitude.js';
  import { licenceView } from './licence-view.js';
  export { React, createRoot, flushSync, ChangeShell, buildChangeView, deriveChangeModel, A11Y_CHANGE, A11Y_COVERAGE, HQ_COPY, KEYMAP, pressPrimary, licenceView };
`
const result = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  stdin: { contents: ENTRY, resolveDir: here },
})
const file = join(temporary, 'approve-keys.mjs')
await writeFile(file, result.outputFiles[0].text)
const M = await import(pathToFileURL(file).href)
const { HQ_COPY, KEYMAP, pressPrimary } = M
const D = HQ_COPY.change.model.decision

/* The Change is ready (the server's readiness answer), so the Inspector's
 * primary is the Decision. The same shape `disabled-controls.test.js` drives. */
const READY = {
  ...M.A11Y_COVERAGE,
  readiness: {
    ready: true,
    target_version: 'e71c5b0aaaa',
    headline_slug: 'readiness_ready',
    obligations: [{ id: 'o1', criterion_index: 1, text: 'The totals add up', outcome: 'satisfied', evidence_name: 'tests pass' }],
    rests_on: [{ obligation_id: 'o1', text: 'The totals add up', evidence_name: 'tests pass' }],
  },
}
const APPROVED = {
  ...M.A11Y_CHANGE,
  decisions: { standing: { target_revision: 'e71c5b0aaaa', actor: 'sam', decided_at: '2026-09-22T11:00:00Z', scope_version: 1 }, history: [] },
}
/* T-DIST-4 (the dist merge): approving needs a licence that works; this file
 * is about which keys record, so the machine it runs on has one — the same
 * licence `disabled-controls.test.js` gives its busy-state walk. */
const LICENSED = M.licenceView({ status: 'valid', plan: 'pro', expires_on: '23 Sep 2027', entitlements: { approve_version: { allowed: true } }, technical: {} })
const shellModel = (change, licence = LICENSED) => ({ ...M.buildChangeView({ change, work: [] }), hq: M.deriveChangeModel(change, READY, null, null, []), licence })

/* Every single key the shell's map answers, plus the keys a browser turns
 * into a press of whatever is focused. The chords are pressed separately. */
const SINGLE_KEYS = [...new Set(['.', 'Enter', ' ', ...KEYMAP.filter((r) => !r.combo).flatMap((r) => r.keys)])]

const GLOBALS = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'FocusEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage', 'CSS', 'fetch']
const settle = () => new Promise((r) => setTimeout(r, 10))
const settled = async (n = 4) => { for (let i = 0; i < n; i += 1) await settle() }

async function mounted(change, run, licence) {
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/?altitude=brief', pretendToBeVisual: true })
  const w = dom.window
  const held = GLOBALS.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of GLOBALS) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  const writes = []
  set('fetch', (url, opts) => {
    if (opts && String(opts.method).toUpperCase() === 'POST') writes.push({ url: String(url), body: JSON.parse(opts.body || '{}') })
    return new Promise(() => {})
  })
  const root = M.createRoot(w.document.getElementById('root'))
  try {
    M.flushSync(() => root.render(M.React.createElement(M.ChangeShell, {
      model: shellModel(change, licence), stage: 'approve', altitude: 'brief', register: 'manager',
      onGo() {}, onCopy() {}, onReload() {}, onAltitude() {}, onRegister() {},
    })))
    await settled(6)
    const d = w.document
    const key = async (target, k, mods = {}) => {
      (target || d.body).dispatchEvent(new w.KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...mods }))
      await settled()
    }
    /* Type a name the way a keyboard does (see `disabled-controls.test.js`
     * `decisionMidWrite` for why React needs focus, input and key up here). */
    const typeName = async (name) => {
      const input = d.querySelector('.ws-decision .ws-cov-actor input')
      assert.ok(input, 'the name box is drawn')
      const setValue = Object.getOwnPropertyDescriptor(w.HTMLInputElement.prototype, 'value').set
      input.attachEvent = () => {}
      input.detachEvent = () => {}
      input.focus()
      input.dispatchEvent(new w.FocusEvent('focusin', { bubbles: true }))
      setValue.call(input, name)
      input.dispatchEvent(new w.Event('input', { bubbles: true }))
      input.dispatchEvent(new w.KeyboardEvent('keyup', { key: 'm', bubbles: true }))
      await settled()
      return input
    }
    const button = (label) => [...d.querySelectorAll('.ws-decision button')].find((b) => b.textContent.trim() === label) || null
    const click = async (el) => { el.dispatchEvent(new w.MouseEvent('click', { bubbles: true })); await settled() }
    const sheet = () => d.querySelector('[role="dialog"][aria-modal="true"].ws-decision-sheet')
    return await run({ d, w, key, typeName, button, click, sheet, writes })
  } finally {
    root.unmount()
    for (const [n, desc] of held) {
      if (desc) Object.defineProperty(globalThis, n, desc)
      else delete globalThis[n]
    }
    w.close()
  }
}

test('A3-F8a (a) · with a name typed and Approve on, no key pressed on the page records an approval, and `.` says how approving is done', async () => {
  await mounted(M.A11Y_CHANGE, async ({ d, key, typeName, button, writes }) => {
    const input = await typeName('sam')
    const approve = button(D.approve)
    assert.ok(approve && !approve.disabled, 'with a name typed, Approve is on')
    input.blur()
    d.body.focus()
    for (const k of SINGLE_KEYS) {
      await key(d.body, k)
      assert.deepEqual(writes, [], `one press of ${JSON.stringify(k)} on the page wrote ${JSON.stringify(writes)}`)
      if (k !== 'Escape') await key(d.body, 'Escape')
    }
    for (const mods of [{ metaKey: true }, { ctrlKey: true }]) {
      await key(d.body, 'Enter', mods)
      assert.deepEqual(writes, [], `${JSON.stringify(mods)}+Enter on the page wrote`)
    }
    await key(d.body, '.')
    assert.deepEqual(writes, [], '. wrote')
    assert.equal(typeof HQ_COPY.shell.notes.approve_by_hand, 'string', 'the refusal is a copy string')
    assert.equal(d.querySelector('.ws-shell-note').textContent, HQ_COPY.shell.notes.approve_by_hand, '. says how approving is done')
  })
})

test('A3-F8a (b) · `.` never presses Withdraw; a click on it still withdraws, once, with no sheet in between', async () => {
  await mounted(APPROVED, async ({ d, key, typeName, button, click, writes }) => {
    const input = await typeName('sam')
    const withdraw = button(D.withdraw)
    assert.ok(withdraw && !withdraw.disabled, 'with a name typed, Withdraw is on')
    input.blur()
    d.body.focus()
    await key(d.body, '.')
    assert.deepEqual(writes, [], '. withdrew the approval')
    await click(withdraw)
    assert.equal(writes.length, 1, 'the click withdrew')
    assert.equal(writes[0].url, '/api/changes/ch1/decisions/withdraw')
    assert.equal(writes[0].body.actor, 'sam')
  })
})

test('A3-F8a · Approve opens a sheet and records nothing; inside it Enter, Space, `.` and every mapped key record nothing; Esc closes it and gives focus back', async () => {
  await mounted(M.A11Y_CHANGE, async ({ d, key, typeName, button, click, sheet, writes }) => {
    await typeName('sam')
    const approve = button(D.approve)
    await click(approve)
    assert.deepEqual(writes, [], 'a press of Approve recorded an approval with no confirming act')
    assert.ok(sheet(), 'Approve opened no confirming sheet')
    assert.ok(sheet().contains(d.activeElement), 'focus is not inside the sheet')
    const text = sheet().textContent
    for (const s of [D.confirm.heading, D.confirm.recorded, D.not_an_action, D.confirm.act, D.confirm.keys]) {
      assert.ok(text.includes(s), `the sheet does not say: ${s}`)
    }
    assert.ok(text.includes('sam'), 'the sheet does not name who is approving')
    for (const k of SINGLE_KEYS.filter((x) => x !== 'Escape')) {
      await key(d.activeElement, k)
      assert.deepEqual(writes, [], `one press of ${JSON.stringify(k)} inside the sheet wrote`)
      assert.ok(sheet(), `${JSON.stringify(k)} closed the sheet`)
    }
    await key(d.activeElement, 'Escape')
    assert.equal(sheet(), null, 'Esc left the sheet open')
    assert.deepEqual(writes, [], 'Esc wrote')
    assert.ok(d.activeElement === button(D.approve), 'Esc did not give focus back to Approve')
  })
})

test('A3-F8a · ⌘↵ from the name box opens the sheet and records nothing', async () => {
  await mounted(M.A11Y_CHANGE, async ({ key, typeName, sheet, writes }) => {
    const input = await typeName('sam')
    await key(input, 'Enter', { metaKey: true })
    assert.deepEqual(writes, [], '⌘↵ in the name box recorded an approval')
    assert.ok(sheet(), '⌘↵ in the name box opened no sheet')
  })
})

test('A3-F8a · only the confirming act records — ⌘↵ or Ctrl+↵ inside the sheet, or its own button — exactly once, to the one decide address, in the typed name', async () => {
  for (const how of ['meta', 'ctrl', 'button']) {
    await mounted(M.A11Y_CHANGE, async ({ d, key, typeName, button, click, sheet, writes }) => {
      await typeName('sam')
      await click(button(D.approve))
      assert.ok(sheet(), `${how}: no sheet`)
      if (how === 'button') {
        const confirm = [...sheet().querySelectorAll('button')].find((b) => b.textContent.trim() === D.confirm.act)
        assert.ok(confirm && !confirm.disabled, `${how}: the sheet has no live confirm button`)
        await click(confirm)
      } else {
        await key(d.activeElement, 'Enter', how === 'meta' ? { metaKey: true } : { ctrlKey: true })
      }
      assert.equal(writes.length, 1, `${how}: the confirming act wrote ${writes.length} times`)
      assert.equal(writes[0].url, '/api/changes/ch1/decisions', `${how}: wrote to ${writes[0].url}`)
      assert.equal(writes[0].body.actor, 'sam', `${how}: the approval does not carry the typed name`)
      assert.equal(sheet(), null, `${how}: the sheet stayed open over the write`)
    })
  }
})

/* A document the way the screen draws it: the box `.` reads, and in it the
 * decision (which says it is never pressed by a key) and, where a screen has
 * one, a live primary of another kind. */
function page(inner) {
  const dom = new JSDOM('<!doctype html><body><div data-primary-action="">' + inner + '</div></body>')
  const d = dom.window.document
  let clicks = []
  for (const b of d.querySelectorAll('button')) b.addEventListener('click', () => { clicks.push(b.textContent) })
  return { d, clicks: () => clicks, close: () => dom.window.close() }
}

test('A3-F8a (a) · `.` never presses a live decision act: it presses nothing and hands back why', () => {
  const p = page('<div class="ws-decision" data-no-key-primary="Approving is done by hand."><button class="ws-change-primary">Approve this version</button></div>')
  const r = pressPrimary(p.d)
  assert.deepEqual(p.clicks(), [])
  assert.equal(r.pressed, false)
  assert.equal(r.reason, 'Approving is done by hand.')
  p.close()
})

test('A3-F8a (c) · every other primary `.` pressed before is still pressed — alone, and beside a decision act', () => {
  for (const label of ['Show', 'Run the check', 'Copy prompt']) {
    const alone = page(`<button class="ws-change-primary">${label}</button>`)
    assert.deepEqual(pressPrimary(alone.d), { pressed: true, reason: null })
    assert.deepEqual(alone.clicks(), [label])
    alone.close()
    const beside = page(`<div class="ws-decision" data-no-key-primary="By hand."><button class="ws-change-primary">Approve this version</button></div><button class="ws-change-primary">${label}</button>`)
    assert.deepEqual(pressPrimary(beside.d), { pressed: true, reason: null })
    assert.deepEqual(beside.clicks(), [label])
    beside.close()
  }
})

test('A3-F8a · an OFF decision act still answers `.` the old way: its own reason, not the by-hand note', () => {
  const p = page('<div class="ws-decision" data-no-key-primary="By hand."><button class="ws-change-primary" disabled aria-describedby="w">Approve this version</button><p id="w" tabindex="-1">An approval has to have a name on it.</p></div>')
  const r = pressPrimary(p.d)
  assert.deepEqual(r, { pressed: false, reason: 'An approval has to have a name on it.' })
  assert.deepEqual(p.clicks(), [])
  p.close()
})

/* The dist merge put the licence gate (T-DIST-4) and this file's confirming
 * sheet (A3-F8a) into one Decision. The chord and the button open the same
 * sheet, so with no licence that works neither may: Approve is drawn off with
 * its reason, and ⌘↵ / Ctrl+↵ from the name box open nothing and record
 * nothing. `licenceView(null)` is the screen that could not ask — never a pass. */
test('the dist merge · with no licence that works, Approve is off, and ⌘↵ or Ctrl+↵ from the name box opens no sheet and records nothing', async () => {
  await mounted(M.A11Y_CHANGE, async ({ key, typeName, button, sheet, writes }) => {
    const input = await typeName('sam')
    const approve = button(D.approve)
    assert.ok(approve, 'Approve is not drawn at all')
    assert.ok(approve.disabled, 'with no licence read, Approve is on')
    for (const mods of [{ metaKey: true }, { ctrlKey: true }]) {
      await key(input, 'Enter', mods)
      /* `ok(… === null)`, not `equal(sheet(), null)`: a failing equal would
       * hand the reporter a live jsdom node to print, and that never ends. */
      assert.ok(sheet() === null, `${JSON.stringify(mods)}+Enter opened a sheet the button would not`)
      assert.deepEqual(writes, [], `${JSON.stringify(mods)}+Enter recorded an approval`)
    }
  }, M.licenceView(null))
})
