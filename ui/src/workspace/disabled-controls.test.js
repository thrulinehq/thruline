import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'
import { licenceView } from './licence-view.js'

/* T-2-10, finding 2 — every disabled control says WHAT WOULD ENABLE IT.
 *
 * `03-USER-LENS.md` rule 6 has two halves, and the second one is the one that
 * goes missing. The Phase 2 exit check found a control whose reason read
 * "Accepting what counts as done cannot be done from this screen yet. Nothing
 * on this screen enables it." — the first half twice, in the ten-second read
 * of the first screen a new person opens, with nowhere to go from it.
 *
 * One test, over the four screens this build has, driven over the RENDERED
 * DOM rather than over the copy file: every `<button disabled>` is found, its
 * `aria-describedby` is followed to the reason a person actually reads, and
 * that text must contain one of the acts in `HQ_COPY.enablers` — a command to
 * run, a thing to click, a person to ask, or what this build's own next
 * version arrives with. A new disabled control cannot be added without one,
 * and a reason that loses its enabler in an edit fails here.
 *
 * Every fixture is synthetic. No string here came off a real machine.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-disabled-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeScreen } from './change-workspace.jsx';
  import { PacketPage } from './packet.jsx';
  import { InboxScreen } from './inbox.jsx';
  import { FirstUseScreen } from './first-use.jsx';
  import { buildChangeView, STAGES } from './change-view.js';
  import { buildPacketView } from './packet-view.js';
  import { inboxItems, storageReading } from './inbox-view.js';
  import { candidateRows } from './first-use-view.js';
  import { HQ_COPY } from './hq-copy.js';
  import { createRoot } from 'react-dom/client';
  import { flushSync } from 'react-dom';
  import Workspace from './shell.jsx';
  import { ChangeShell } from './change-workspace.jsx';
  import { ShellTopBar } from './shell-chrome.jsx';
  import { deriveChangeModel } from './change-model.js';
  import { A11Y_CHANGE, A11Y_COVERAGE } from './a11y-fixture.js';
  export { buildChangeView, buildPacketView, inboxItems, storageReading, candidateRows, STAGES, HQ_COPY };
  export { React, createRoot, flushSync, Workspace, deriveChangeModel, A11Y_CHANGE, A11Y_COVERAGE };
  export const renderShell = props => renderToStaticMarkup(React.createElement(ChangeShell, props));
  export const shellElement = props => React.createElement(ChangeShell, props);
  export const renderTopBar = props => renderToStaticMarkup(React.createElement(ShellTopBar, props));
  export const renderChange = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderPacket = props => renderToStaticMarkup(React.createElement(PacketPage, props));
  export const renderInbox = props => renderToStaticMarkup(React.createElement(InboxScreen, props));
  export const renderFirstUse = props => renderToStaticMarkup(React.createElement(FirstUseScreen, props));
`

const result = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  stdin: { contents: ENTRY, resolveDir: here },
})
const file = join(temporary, 'disabled.mjs')
await writeFile(file, result.outputFiles[0].text)
const M = await import(pathToFileURL(file).href)

const {
  buildChangeView, buildPacketView, inboxItems, storageReading, candidateRows, STAGES, HQ_COPY,
  renderChange, renderPacket, renderInbox, renderFirstUse,
} = M

const text = (html) => html.replace(/<[^>]*>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/&#x2F;/g, '/').replace(/\s+/g, ' ').trim()

const CHANGE_ID = 'cccccccc-0000-4000-8000-00000000c001'
const SESSION_A = 'cccccccc-0000-4000-8000-00000000s001'

const aChange = (over = {}) => ({
  id: CHANGE_ID,
  project_id: 'cccccccc-0000-4000-8000-00000000p001',
  opened_at: '2026-01-02T09:00:00Z',
  request: { title: 'Retry failed uploads', purpose: 'Uploads that fail once are lost.', requester: 'sam', source: 'person' },
  accepted_scope: null,
  implementation: null,
  associations: [],
  examination: 'none_yet',
  decision: 'none_yet',
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
  ...over,
})

const accepted = {
  accepted_scope: {
    version: 1,
    criteria: ['A failed upload is tried again three times.'],
    exclusions: [], allowed_scope: [], constraints: [], check_profile: ['cargo-test'],
    content_sha256: '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
    acceptance: { actor: 'sam', accepted_at: '2026-01-02T09:06:00Z', rationale: 'That is what I asked for.' },
    proposed_at: '2026-01-02T09:05:00Z',
  },
}

const linked = {
  associations: [{ id: 'a1', session_id: SESSION_A, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-01-02T10:01:00Z', reason: null }],
}

const PROPOSALS = { criteria: [{ text: 'A failed upload is tried again three times.' }, { text: 'The person is told when it gives up.' }] }
const WORK = [{ id: SESSION_A, findings: [], verdict: null, focus: null, steps: 41 }]

/* The four screens, in every state this build can put them in that has a
 * control turned off. The Change screen is rendered at all six stages of three
 * different Changes, because which pair a stage shows depends on the record. */
function everyScreen() {
  const out = []
  const models = [
    ['a Change with nothing proposed', buildChangeView({ change: aChange(), work: [] })],
    ['a Change with suggestions and nothing accepted', buildChangeView({ change: aChange(), proposals: PROPOSALS, work: [] })],
    ['a Change with what counts as done accepted and work linked', buildChangeView({ change: aChange({ ...accepted, ...linked }), proposals: PROPOSALS, work: WORK })],
  ]
  for (const [what, model] of models) {
    for (const stage of STAGES) out.push([`the Change screen, ${what}, at ${stage}`, renderChange({ model, stage })])
  }
  out.push(['the whole story on one page', renderPacket({ model: buildPacketView({ change: aChange({ ...accepted, ...linked }), work: WORK, build: [] }) })])

  const inputs = [{ change: aChange({ ...accepted, ...linked }), work: WORK }]
  const { items, settled } = inboxItems(inputs)
  out.push(['the Inbox', renderInbox({
    state: {
      status: 'ready', items, settled, total: 1, unread: 0, newest: CHANGE_ID,
      storage: storageReading({ bytes: 3_174_400, path: '/tmp/fixture-store', parts: [{ key: 'records', bytes: 3_174_400 }] }),
      notes: [],
    },
  })])

  const OVERVIEW = {
    schema_version: 1, registered: 0,
    store: { folder: '/tmp/fixture-home/.thruline', bytes: 897024 },
    candidates: [{
      key: 'cccccccc00000001', label: '/tmp/fixture/uploads-api', source: 'claude_code',
      recordings: 14, not_read: 0, first_recorded: '2026-01-02T09:00:00Z', last_recorded: '2026-01-09T09:00:00Z',
      registered: false, project: null,
    }],
    codex_recordings: 0, notes: [],
  }
  out.push(['the first-use screen, nothing picked', renderFirstUse({
    state: {
      status: 'ready', step: 'pick', selected: null,
      ...candidateRows(OVERVIEW),
      storage: { known: true, size: '0.9 MB', path: OVERVIEW.store.folder, bytes: OVERVIEW.store.bytes },
    },
  })])
  return out
}

/** Every disabled control on one screen, with the reason a person reads. */
function disabledControls(html) {
  const found = []
  for (const m of html.matchAll(/<button[^>]*\bdisabled\b[^>]*>(.*?)<\/button>/g)) {
    const attrs = m[0].slice(0, m[0].indexOf('>'))
    const described = /aria-describedby="([^"]+)"/.exec(attrs)
    const label = text(m[1])
    if (!described) { found.push({ label, why: null }); continue }
    const why = new RegExp(`id="${described[1]}"[^>]*>(.*?)</`).exec(html)
    found.push({ label, why: why ? text(why[1]) : null })
  }
  return found
}

const saysAnAct = (why) => HQ_COPY.enablers.filter((phrase) => why.toLowerCase().includes(phrase.toLowerCase()))

test('every disabled control on every screen says why it is off AND names an act that would enable it', () => {
  let seen = 0
  for (const [where, html] of everyScreen()) {
    for (const control of disabledControls(html)) {
      seen += 1
      assert.ok(control.why, `${where}: the control "${control.label}" is off and points at no reason at all`)
      assert.ok(control.why.length > 30, `${where}: "${control.label}" says nothing: ${control.why}`)
      const acts = saysAnAct(control.why)
      assert.ok(
        acts.length > 0,
        `${where}: "${control.label}" says why it is off and never says what would turn it on — no act from HQ_COPY.enablers is in: ${control.why}`,
      )
    }
  }
  /* A screen that rendered nothing would pass the loop above by being empty.
     The build has more controls off than on, and this is the floor. */
  assert.ok(seen >= 8, `only ${seen} disabled controls were rendered; the sweep is not reaching the screens`)
})

test('the enabler sweep is a live instrument: a reason with no act in it fails it', () => {
  assert.deepEqual(saysAnAct('Accepting what counts as done cannot be done from this screen yet. Nothing on this screen enables it.'), [])
  assert.ok(saysAnAct('Accept them from a terminal, in this project: thruline change accept x --all.').length > 0)
})

/* The finding itself, on the screen it was found on. The exit check reproduced
 * it on the first-use path: a Change proposed from the record, its suggestions
 * shown, and the one control under them off with nowhere to go. */
test('the Define stage names the command that accepts what counts as done, with this Change in it', () => {
  const html = renderChange({ model: buildChangeView({ change: aChange(), proposals: PROPOSALS, work: [] }), stage: 'define' })
  const why = disabledControls(html).find((c) => c.label === HQ_COPY.change.actions.accept_criteria)
  assert.ok(why && why.why, 'the accept control is not off with a reason on this Change')
  assert.match(why.why, /thruline change accept/, why.why)
  assert.ok(why.why.includes(CHANGE_ID), `the command names no Change, so it cannot be run as read: ${why.why}`)
  assert.doesNotMatch(why.why, /Nothing on this screen enables it/, 'the reason went back to denying an enabler exists')
})

/* ── T-6-6 · C — the P6 surfaces, every `disabled` AND every `aria-disabled` ──
 *
 * The three P6 cards added controls the sweep above never rendered: the shell's
 * top bar (Ask, and Refresh while it refreshes), the sidebar's Outline and Saved
 * views slots on the Change screen, the Change screen's Brief (the Inspector's
 * one primary, Approve and Withdraw), and the Trace's scrubber. Two of them are
 * `aria-disabled` rather than `disabled` — so they stay reachable — which the
 * regex above does not look for at all. This sweep reads the rendered DOM,
 * finds `[disabled]` and `[aria-disabled="true"]` alike, follows each one's
 * `aria-describedby` to what a person reads, and requires a reason AND an act
 * from `HQ_COPY.enablers`. The workspace is mounted for real, in jsdom, over a
 * served session that holds nothing (the one state that turns the scrubber
 * off) and over the bundled demo record at Brief and Explore.
 */

const {
  React, createRoot, flushSync, Workspace, deriveChangeModel, A11Y_CHANGE, A11Y_COVERAGE,
  renderShell, renderTopBar,
} = M

const OFF = 'button[disabled], input[disabled], select[disabled], textarea[disabled], [aria-disabled="true"]'
const squash = (s) => String(s || '').replace(/\s+/g, ' ').trim()

/** Every off control in one document, with the words its description carries. */
function offControls(doc) {
  return [...doc.querySelectorAll(OFF)].map((el) => {
    const ids = (el.getAttribute('aria-describedby') || '').split(/\s+/).filter(Boolean)
    const why = squash(ids.map((id) => { const t = doc.getElementById(id); return t ? t.textContent : '' }).join(' '))
    const label = squash(el.getAttribute('aria-label') || el.textContent || el.getAttribute('title') || el.tagName)
    return { label, why: why || null }
  })
}

const shellModel = (change, coverage) => {
  const model = buildChangeView({ change, work: [] })
  return { ...model, hq: deriveChangeModel(change, coverage || null, null, null, []) }
}
const APPROVED = {
  ...A11Y_CHANGE,
  decisions: { standing: { target_revision: 'e71c5b0aaaa', actor: 'sam', decided_at: '2026-09-22T11:00:00Z', scope_version: 1 }, history: [] },
}

/* P6B-MERGE — the p6b branch forked before T-3-5, which shut the Approve door
 * on READINESS and on nothing else: `A11Y_COVERAGE` carries no `readiness`
 * block, so on this side `readinessOf` answers `absent`, the Inspector draws
 * the off control instead of `Decision`, and the busy-state drive below never
 * finds the name box. The product rule is the one that was kept; the fixture
 * is what moved. This is the same coverage with the server's readiness answer
 * added — synthetic, ready, resting on the one linked run — used ONLY by the
 * mid-write drive, so every other screen in this file keeps the shut door it
 * was written against. */
const READY_COVERAGE = {
  ...A11Y_COVERAGE,
  readiness: {
    ready: true,
    target_version: 'e71c5b0aaaa',
    headline_slug: 'readiness_ready',
    obligations: [{ id: 'o1', criterion_index: 1, text: 'The totals add up', outcome: 'satisfied', evidence_name: 'tests pass' }],
    rests_on: [{ obligation_id: 'o1', text: 'The totals add up', evidence_name: 'tests pass' }],
  },
}

function p6StaticScreens() {
  const out = []
  const models = [
    ['a Change with nothing linked', shellModel(aChange())],
    ['a proven Change, nobody named', shellModel(A11Y_CHANGE, A11Y_COVERAGE)],
    ['an approved Change, nobody named', shellModel(APPROVED, A11Y_COVERAGE)],
  ]
  for (const [what, model] of models) {
    for (const altitude of ['brief', 'explore']) {
      out.push([`the Change shell at ${altitude}, ${what}`, renderShell({ model, stage: 'approve', altitude, register: 'manager' })])
    }
  }
  out.push(['the top bar while it refreshes', renderTopBar({
    crumbs: [{ key: 'work', text: 'Retry failed uploads', work: true }],
    altitude: 'explore', register: 'manager', readWhen: null, onRefresh: () => {}, refreshing: true,
  })])
  return out.map(([where, html]) => [where, new JSDOM('<!doctype html><body>' + html + '</body>').window.document])
}

const GLOBALS = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage', 'CSS', 'fetch']
const settle = () => new Promise((r) => setTimeout(r, 10))
const EMPTY_SESSION = 'empty-session-1'
const EMPTY_ROUTES = {
  '/api/sessions': { shape: 'thruline.sessions', schema_version: 1, total: 1, sessions: [{ id: EMPTY_SESSION, repo_root: '/work/uploads', event_count: 0 }] },
  '/api/explain': { shape: 'thruline.explain', schema_version: 1, session: EMPTY_SESSION, what_happened: [], claims_vs_proof: [], promises: [], risk_flags: [] },
  '/api/claims': { shape: 'thruline.claims', schema_version: 1, session: EMPTY_SESSION, claims: [] },
}
const answering = (routes) => async (url) => {
  const path = String(url).replace(/^https?:\/\/[^/]+/, '').split('?')[0]
  const doc = Object.prototype.hasOwnProperty.call(routes, path) ? routes[path] : null
  return doc
    ? { ok: true, status: 200, statusText: 'OK', headers: { get: () => null }, text: async () => JSON.stringify(doc), json: async () => doc }
    : { ok: false, status: 404, statusText: 'Not Found', headers: { get: () => null }, text: async () => '{}', json: async () => ({}) }
}

/** The real Workspace, mounted in jsdom at one altitude over one set of routes. */
async function workspaceAt(altitude, routes, run) {
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/?altitude=' + altitude, pretendToBeVisual: true })
  const w = dom.window
  const held = GLOBALS.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of GLOBALS) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  set('fetch', answering(routes))
  const root = createRoot(w.document.getElementById('root'))
  try {
    const ref = React.createRef()
    flushSync(() => root.render(React.createElement(Workspace, { ref, index: { projects: [], totals: {}, kind: 'index', contract_version: 1 } })))
    for (let i = 0; i < 60; i += 1) await settle()
    return await run(w.document, ref.current)
  } finally {
    root.unmount()
    for (const [n, desc] of held) {
      if (desc) Object.defineProperty(globalThis, n, desc)
      else delete globalThis[n]
    }
    w.close()
  }
}

test('C · every disabled or aria-disabled control on the P6 surfaces says why AND names an act that would turn it on', async () => {
  const seen = []
  const check = (where, doc) => {
    for (const c of offControls(doc)) {
      seen.push(c.label)
      assert.ok(c.why, `${where}: "${c.label}" is off and points at no reason at all`)
      assert.ok(c.why.length > 30, `${where}: "${c.label}" says nothing: ${c.why}`)
      assert.ok(saysAnAct(c.why).length > 0, `${where}: "${c.label}" says why it is off and never what would turn it on: ${c.why}`)
    }
  }
  for (const [where, doc] of p6StaticScreens()) check(where, doc)
  for (const altitude of ['brief', 'explore']) {
    await workspaceAt(altitude, {}, async (d) => check(`the workspace at ${altitude}, the bundled record`, d))
    await workspaceAt(altitude, EMPTY_ROUTES, async (d, ws) => {
      assert.equal(ws.state.isDemo, false, 'the empty served session opened')
      check(`the workspace at ${altitude}, a session that holds nothing`, d)
    })
  }
  /* Non-vacuity: every P6 control the card names was actually reached off. */
  const S = HQ_COPY.shell
  for (const label of [S.ask.label, S.sidebar.outline, S.sidebar.saved, S.topbar.refreshing, HQ_COPY.trace.scrub.label,
    HQ_COPY.change.model.decision.approve, HQ_COPY.change.model.decision.withdraw, HQ_COPY.change.model.actions.read]) {
    assert.ok(seen.includes(label), `the sweep never reached "${label}" switched off; it saw: ${[...new Set(seen)].join(' | ')}`)
  }
})

test('C · the scrubber is off only when there is nothing to move through: at the record\'s ends it stays on', async () => {
  await workspaceAt('explore', {}, async (d, ws) => {
    const range = d.querySelector('.ws-trace-scrub input[type="range"]')
    assert.ok(range, 'the scrubber is drawn at Explore')
    assert.equal(range.hasAttribute('disabled'), false, 'the scrubber is off over a record that holds steps')
    const n = ws.S().events.length
    for (const at of [0, n - 1]) {
      ws.scrubTo(at)
      for (let i = 0; i < 4; i += 1) await settle()
      assert.equal(d.querySelector('.ws-trace-scrub input[type="range"]').hasAttribute('disabled'), false, `the scrubber went off at step ${at}`)
    }
  })
})

/* ── T-6-6 fix round 1 — Approve and Withdraw WHILE BUSY ──────────────────
 *
 * `Decision` holds `busy` in its own state; no prop sets it. So the real
 * Change shell is mounted in jsdom over a proven Change (Approve on offer) and
 * over an approved one (Withdraw on offer), a name is typed into the box the
 * way a keyboard would, the button is clicked, and the write is left
 * unanswered (a `fetch` that never settles). While it hangs the button is off,
 * and the sweep must find its reason AND an enabler, like every other one.
 */
async function decisionMidWrite(change, label, run) {
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/?altitude=brief', pretendToBeVisual: true })
  const w = dom.window
  const held = GLOBALS.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of GLOBALS) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  let posted = 0
  set('fetch', () => { posted += 1; return new Promise(() => {}) })
  const root = createRoot(w.document.getElementById('root'))
  try {
    /* T-DIST-4: approving needs a licence that works; this walk is about the
     * busy state, so the machine it runs on has one. */
    const model = shellModel(change, READY_COVERAGE)
    model.licence = licenceView({ status: 'valid', plan: 'pro', expires_on: '23 Sep 2027', entitlements: { approve_version: { allowed: true } }, technical: {} })
    flushSync(() => root.render(M.shellElement({ model, stage: 'approve', altitude: 'brief', register: 'manager' })))
    for (let i = 0; i < 6; i += 1) await settle()
    const d = w.document
    const input = d.querySelector('.ws-decision .ws-cov-actor input')
    assert.ok(input, `${label}: the name box is drawn`)
    const setValue = Object.getOwnPropertyDescriptor(w.HTMLInputElement.prototype, 'value').set
    /* React was bundled before this window existed, so it decided `input`
     * events are unsupported and reads a typed value the old way: it starts
     * watching the box when it takes focus (through `attachEvent`, which jsdom
     * does not have — given a no-op here) and compares the value on the key
     * coming up. So: focus, type, key up — what a keyboard does. */
    input.attachEvent = () => {}
    input.detachEvent = () => {}
    input.focus()
    input.dispatchEvent(new w.FocusEvent('focusin', { bubbles: true }))
    setValue.call(input, 'sam')
    input.dispatchEvent(new w.Event('input', { bubbles: true }))
    input.dispatchEvent(new w.KeyboardEvent('keyup', { key: 'm', bubbles: true }))
    for (let i = 0; i < 4; i += 1) await settle()
    const btn = [...d.querySelectorAll('.ws-decision button')].find((b) => b.textContent.trim() === label)
    assert.ok(btn && !btn.disabled, `${label}: with a name typed the button is on`)
    btn.dispatchEvent(new w.MouseEvent('click', { bubbles: true }))
    for (let i = 0; i < 4; i += 1) await settle()
    /* A3-F8a: Approve opens a confirming sheet and writes nothing; the write
     * is the sheet's own button. Withdraw opens none. Whether Approve may
     * ever write without the sheet is `approve-keys.test.js`'s to hold. */
    const sheet = d.querySelector('.ws-decision-sheet[role="dialog"]')
    if (sheet) {
      assert.equal(posted, 0, `${label}: the press that opened the sheet wrote`)
      const confirm = [...sheet.querySelectorAll('button')].find((b) => b.textContent.trim() === HQ_COPY.change.model.decision.confirm.act)
      assert.ok(confirm && !confirm.disabled, `${label}: the sheet has no live confirm`)
      confirm.dispatchEvent(new w.MouseEvent('click', { bubbles: true }))
      for (let i = 0; i < 4; i += 1) await settle()
    }
    assert.equal(posted, 1, `${label}: the click wrote once`)
    return await run(d)
  } finally {
    root.unmount()
    for (const [n, desc] of held) {
      if (desc) Object.defineProperty(globalThis, n, desc)
      else delete globalThis[n]
    }
    w.close()
  }
}

test('C · Approve and Withdraw, while their write is in flight, are off and say why AND what turns them back on', async () => {
  const D = HQ_COPY.change.model.decision
  for (const [change, label] of [[A11Y_CHANGE, D.approve], [APPROVED, D.withdraw]]) {
    await decisionMidWrite(change, label, async (d) => {
      const busy = [...d.querySelectorAll('.ws-decision button[disabled]')].filter((b) => b.textContent.trim() === D.working)
      assert.equal(busy.length, 1, `${label}: mid-write, the button is not off and reading "${D.working}"`)
      const [control] = offControls(d).filter((c) => c.label === D.working)
      assert.ok(control && control.why, `${label}: busy, and points at no reason`)
      assert.ok(control.why.includes(D.working_why.reason), `${label}: busy, and the reason is not said: ${control.why}`)
      assert.ok(saysAnAct(control.why).length > 0, `${label}: busy, and never says what turns it back on: ${control.why}`)
      assert.ok(control.why.includes(D.working_why.enabler), `${label}: busy, and the enabler is not said: ${control.why}`)
    })
  }
})
