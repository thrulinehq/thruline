/* T-DIST-4 acceptance A — the licence, on the screen.
 *
 * Sam's question: "can I still see my own work if I don't pay?" Two halves,
 * asked of the rendered DOM rather than of the model:
 *
 *   1. The Change screen, mounted in jsdom with the server's answers stubbed,
 *      with the licence ABSENT and then VALID. Absent: Approve is off, the
 *      reason and what turns it on are on screen, the door to the licence page
 *      is there, and every other control that was on is still on. Valid:
 *      Approve turns on once a name is in the box.
 *   2. The licence page, for absent, valid, expired and tampered: each one's
 *      own sentence.
 *
 * Every fixture is synthetic. No string here came off a real machine.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'
import { HQ_COPY } from './hq-copy.js'
import { licenceView } from './licence-view.js'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-licence-'))
after(() => rm(temporary, { recursive: true, force: true }))

const COMMON = {
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
}
async function load(name, contents) {
  const result = await build({ ...COMMON, stdin: { contents, resolveDir: here } })
  const file = join(temporary, name)
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}

const L = HQ_COPY.licence

/* Ruling 3 (D-298): what an expired paid licence is told to do. */
const RENEW = "Go to the licence page on ThruLine's website with your licence key, renew there if your subscription has ended, then download the new licence and install it."

/* ── what `GET /api/licence` serves, for each case ─────────────────────── */

/* `thruline.licence.status` v4, as `licence_cmd::status_body` builds it: the
 * paid gate is on unless the build has no licence key (`no_key`, which forces
 * it off), approving is allowed only for a valid licence or with the gate off,
 * and the headline is "free" only for no licence with the gate on. `extra`
 * stands for what one case changes, and wins. */
function served(status, extra = {}) {
  const valid = status === 'valid'
  const keyless = status === 'no_key'
  const gate = 'paid_gate' in extra ? extra.paid_gate !== false : !keyless
  return {
    schema_version: 4,
    shape: 'thruline.licence.status',
    status,
    sentence: 'server sentence — the screen words it from the copy file',
    enable: valid ? null : 'server fix',
    plan: valid ? 'pro' : null,
    expires_at: valid ? '2027-09-23T00:00:00Z' : null,
    expires_on: valid ? '23 Sep 2027' : status === 'expired' ? '1 Jan 2020' : null,
    paid: [{ surface: 'approve_version', plans: ['pro'] }],
    entitlements: { approve_version: valid || !gate ? { allowed: true } : { allowed: false, reason: 'r', enable: 'e', status } },
    headline: gate && status === 'absent' ? 'free' : 'licence',
    paid_gate: gate,
    technical: valid
      ? { key_id: 'k1', licence_id: 'L1', machine_bound: false, keys_built_in: 1 }
      : { key_id: null, licence_id: null, machine_bound: null, keys_built_in: keyless ? 0 : 1 },
    ...extra,
  }
}
const ABSENT = served('absent')
const VALID = served('valid')
const EXPIRED = served('expired')
const TAMPERED = served('tampered')

/* ── one Change the record backs, ready to approve (the T-S-4 fixture) ── */

const VERSION_A = 'e71c5b0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
const CHANGE = {
  id: 'ch1',
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Totals add up on the checkout page' },
  implementation: { snapshot_id: VERSION_A, bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1,
    criteria: ['The totals add up'],
    allowed_scope: ['checkout/'],
    check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet',
  decisions: { standing: null, history: [], target_revision: VERSION_A, scope_version: 1 },
}
const COVERAGE = {
  scope_version: 1,
  link_rows: 1,
  execution_sessions: ['s1'],
  criteria: [{
    index: 1,
    text: 'The totals add up',
    state: 'evidence',
    items: [{
      link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run',
      name: 'tests pass', observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies',
      fresh: true, fresh_slug: 'applicability_still_applies',
    }],
  }],
}
const READINESS = {
  schema_version: 1, ready: true, scope_version: 1, target_version: 'a1b2c3d4e5f6', headline_slug: 'readiness_ready',
  obligations: [
    { id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: null, disposition: null, waivable: false },
    { id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: 'The totals add up', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: 'tests pass', disposition: null, waivable: true },
  ],
  rests_on: [{ obligation_id: 'criterion:1', text: 'The totals add up', evidence_name: 'tests pass' }],
  missing: [], waived: [], not_applicable: [],
}

/* ── the Change screen, mounted ────────────────────────────────────────── */

const DRIVE = `
  import React from 'react';
  import { createRoot } from 'react-dom/client';
  import { flushSync } from 'react-dom';
  import ChangeWorkspace from './change-workspace.jsx';
  export { React, createRoot, flushSync, ChangeWorkspace };
`
const settle = () => new Promise((r) => setTimeout(r, 5))
const answer = (doc) => ({ ok: true, status: 200, headers: { get: () => null }, text: async () => JSON.stringify(doc) })

async function withChange(licenceDoc, run, change = CHANGE, more = {}) {
  const routes = {
    '/api/changes/ch1': { change },
    '/api/changes/ch1/proposals': { proposals: null },
    '/api/changes/ch1/coverage': more.coverage || { coverage: COVERAGE, readiness: READINESS },
    '/api/changes/ch1/inputs': {},
    '/api/changes/ch1/applicability': { items: [] },
  }
  if (licenceDoc) routes['/api/licence'] = licenceDoc
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/#/changes/ch1' })
  const w = dom.window
  const names = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'HTMLInputElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage', 'fetch']
  const held = names.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  const set = (n, v) => Object.defineProperty(globalThis, n, { value: v, configurable: true, writable: true })
  for (const n of names) if (w[n] !== undefined) set(n, n === 'window' ? w : w[n])
  const asked = []
  set('fetch', async (u, init) => {
    asked.push(((init && init.method) || 'GET') + ' ' + u)
    return u in routes ? answer(routes[u]) : { ok: false, status: 404, headers: { get: () => null }, text: async () => '{}' }
  })
  try {
    const M = await load('drive-' + Math.random().toString(36).slice(2) + '.mjs', DRIVE)
    const root = M.createRoot(w.document.getElementById('root'))
    M.flushSync(() => root.render(M.React.createElement(M.ChangeWorkspace, { id: 'ch1', stage: more.stage || 'verify' })))
    for (let i = 0; i < 60 && !w.document.querySelector('.ws-decision'); i += 1) await settle()
    await settle()
    const type = async (input, text) => {
      const setter = Object.getOwnPropertyDescriptor(w.HTMLInputElement.prototype, 'value').set
      setter.call(input, text)
      input.dispatchEvent(new w.Event('input', { bubbles: true }))
      await settle()
      await settle()
    }
    try {
      return await run({ w, d: w.document, type, asked })
    } finally {
      root.unmount()
    }
  } finally {
    for (const [n, d] of held) {
      if (d) Object.defineProperty(globalThis, n, d)
      else delete globalThis[n]
    }
    w.close()
  }
}

const APPROVE = 'Approve this version'
const buttonsNamed = (d, label) => [...d.querySelectorAll('button')].filter((b) => b.textContent.trim() === label)

/** Every control a person can use right now, by what it says. */
function enabledControls(d) {
  const out = []
  for (const el of d.querySelectorAll('button, a[href], input, summary, select, textarea')) {
    if (el.disabled || el.getAttribute('aria-disabled') === 'true') continue
    /* A link is the same door whatever its words say (the licence line's
     * words change with the licence); a control is what it says. */
    const tag = el.tagName.toLowerCase()
    out.push(tag + ':' + (tag === 'a' ? el.getAttribute('href') : el.textContent.trim() || el.type))
  }
  return out
}

test('A: absent — Approve is off with both halves of why and the door to the licence page; everything else is still on', async () => {
  let before
  await withChange(VALID, async ({ d, type }) => {
    assert.ok(d.querySelector('.ws-decision'), 'the decision block rendered')
    await type(d.querySelector('.ws-decision input'), 'Sam')
    before = enabledControls(d)
    assert.ok(before.includes('button:' + APPROVE), 'the fixture offers approval when licensed')
  })
  await withChange(ABSENT, async ({ d, type, asked }) => {
    assert.ok(asked.includes('GET /api/licence'), 'the screen asked whether this machine may approve')
    assert.ok(!asked.some((a) => a.startsWith('POST')), 'rendering wrote nothing')
    await type(d.querySelector('.ws-decision input'), 'Sam')
    const approve = buttonsNamed(d, APPROVE)
    assert.equal(approve.length, 1, 'Approve is still shown, as the one primary')
    assert.equal(approve[0].disabled, true, 'Approve is off without a licence, name or no name')
    const why = d.getElementById(approve[0].getAttribute('aria-describedby'))
    assert.ok(why, 'the off control is described by its reason')
    /* T-DIST-5: the exact pair the licence page leads with, not a bucket. */
    assert.equal(why.textContent, L.status.absent.problem + ' ' + L.status.absent.fix)
    const door = d.querySelector('.ws-decision a[href="#/licence"]')
    assert.ok(door, 'the door to the licence page is beside it')
    assert.equal(door.textContent, L.approve.link)
    /* Everything that was on is still on — the licence turns off ONE thing. */
    const after = enabledControls(d)
    for (const control of before) {
      if (control === 'button:' + APPROVE) continue
      assert.ok(after.includes(control), `the licence turned off something that is not Approve: ${control}`)
    }
    /* The quiet line in the sidebar, a door and not a banner. */
    const line = d.querySelector('.ws-shell-licence a')
    assert.ok(line, 'the sidebar carries the licence line')
    assert.equal(line.textContent, L.line.absent)
    assert.equal(line.getAttribute('href'), '#/licence')
  })
})

test('A: valid — Approve turns on once a name is in the box, and the sidebar names the plan and the day', async () => {
  await withChange(VALID, async ({ d, type }) => {
    const [off] = buttonsNamed(d, APPROVE)
    assert.equal(off.disabled, true, 'no name yet: off for that reason')
    assert.ok(d.body.textContent.includes(HQ_COPY.change.model.decision.actor_missing))
    await type(d.querySelector('.ws-decision input'), 'Sam')
    const [on] = buttonsNamed(d, APPROVE)
    assert.equal(on.disabled, false, 'licensed and named: Approve is on')
    assert.equal(d.querySelector('.ws-decision a[href="#/licence"]'), null, 'no licence door when nothing is wrong')
    assert.equal(d.querySelector('.ws-shell-licence a').textContent, 'Licence: Pro, until 23 Sep 2027')
  })
})

test('A: a licence answer that never arrived is not a licence — Approve stays off and says it could not find out', async () => {
  await withChange(null, async ({ d, type }) => {
    await type(d.querySelector('.ws-decision input'), 'Sam')
    const [approve] = buttonsNamed(d, APPROVE)
    assert.equal(approve.disabled, true)
    assert.equal(d.getElementById(approve.getAttribute('aria-describedby')).textContent,
      L.status.unknown.problem + ' ' + L.status.unknown.fix)
  })
})

test('A: a broken licence says exactly what is wrong with it, and points at the page', async () => {
  await withChange(TAMPERED, async ({ d, type }) => {
    await type(d.querySelector('.ws-decision input'), 'Sam')
    const [approve] = buttonsNamed(d, APPROVE)
    assert.equal(approve.disabled, true)
    assert.equal(d.getElementById(approve.getAttribute('aria-describedby')).textContent,
      L.status.tampered.problem + ' ' + L.status.tampered.fix)
    assert.equal(d.querySelector('.ws-shell-licence a').textContent, L.line.problem)
  })
})

/* ── the licence page ──────────────────────────────────────────────────── */

const PAGE = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { LicencePanel } from './licence.jsx';
  import { licenceView } from './licence-view.js';
  export const renderPage = (doc) => renderToStaticMarkup(React.createElement(LicencePanel, { view: licenceView(doc), onCopy() {} }));
`
const P = await load('page.mjs', PAGE)
const page = (doc) => new JSDOM('<!doctype html><body>' + P.renderPage(doc) + '</body>').window.document

test('A: the licence page says each status in its own sentence — absent, valid, expired, tampered', () => {
  const cases = [
    [VALID, 'This machine has a ThruLine Pro licence, valid until 23 Sep 2027.', null],
    /* Ruling 3 (D-298): an expired paid licence is final, so its fix is the
     * renew path — for a lapsed subscription and a card that renews itself. */
    [EXPIRED, 'This licence expired on 1 Jan 2020.', RENEW],
    [TAMPERED, "This licence has been changed, or wasn't made by the maker it names, so it can't be trusted.", "Download it again from the licence page on ThruLine's website, with your licence key."],
  ]
  for (const [doc, sentence, fix] of cases) {
    const d = page(doc)
    assert.equal(d.querySelector('[data-licence-status]').textContent, sentence, doc.status)
    const shownFix = d.querySelector('.ws-licence-fix')
    assert.equal(shownFix ? shownFix.textContent : null, fix, doc.status + ' fix')
  }
})

test('A: the page says what needs a licence, why the rest never will, and gives the exact command with one copy control', () => {
  const d = page(ABSENT)
  const text = d.body.textContent
  for (const s of [L.page.needs, L.page.free, L.page.why, L.page.offline]) assert.ok(text.includes(s), s)
  assert.equal(d.querySelector('.ws-licence-install code').textContent, 'thruline licence install <file>')
  const primaries = d.querySelectorAll('.ws-change-primary')
  assert.equal(primaries.length, 1, 'one primary action on the page')
  assert.equal(primaries[0].textContent, L.page.copy)
  /* No upload, no form, no write: a licence is installed from a terminal. */
  assert.equal(d.querySelector('input[type="file"], form'), null)
})

test('P0: the licence page does not promise removal of records this version cannot remove', () => {
  /* First-use and Inbox already say this version removes none of the records
   * ThruLine keeps. The licence page must not offer that absent CLI capability
   * as one of the things that works without a licence. */
  const honestFree = 'Everything else works without one, and always will: recording your work, looking through it, checking it, linking evidence, stopping a check, taking an approval back, and exporting your own records.'
  assert.equal(L.page.free, honestFree, 'the licence copy promises unavailable record removal')
  const d = page(ABSENT)
  assert.equal(d.querySelector('.ws-licence-needs .ws-change-muted').textContent, honestFree)
})

test('A: signature, key id and fingerprint are on the technical layer only', () => {
  const TECHNICAL = /\b(signatures?|key id|fingerprints?)\b/i
  for (const doc of [ABSENT, VALID, EXPIRED, TAMPERED]) {
    const d = page(doc)
    const details = d.querySelector('details')
    assert.ok(details && TECHNICAL.test(details.textContent), 'the technical layer names them')
    details.remove()
    assert.doesNotMatch(d.body.textContent, TECHNICAL, `${doc.status}: a technical word on the plain layer`)
  }
  const walk = (obj, prefix, out) => {
    for (const [k, v] of Object.entries(obj)) {
      if (typeof v === 'string') out.push([prefix + k, v])
      else walk(v, prefix + k + '.', out)
    }
    return out
  }
  for (const [path, text] of walk(L, 'licence.', [])) {
    if (path.startsWith('licence.technical.')) continue
    assert.doesNotMatch(text, TECHNICAL, `${path} says a technical word on the plain layer: ${text}`)
  }
})

test('the licence view never reads a missing or partial answer as licensed', () => {
  assert.equal(licenceView(null).status, 'unknown')
  assert.equal(licenceView(null).approve.allowed, false)
  assert.equal(licenceView({ status: 'something new' }).status, 'unknown')
  /* "valid" with no plan or day is not trusted as valid. */
  assert.equal(licenceView({ ...VALID, plan: null }).status, 'unknown')
  /* allowed:true without status valid is not allowed either. */
  assert.equal(licenceView({ ...ABSENT, entitlements: { approve_version: { allowed: true } } }).approve.allowed, false)
})

test('the licence page and its model reach no write address', () => {
  for (const file of ['licence.jsx', 'licence-view.js']) {
    const src = readFileSync(join(here, file), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
    for (const forbidden of ['writeAction', "method: 'POST'", 'method: "POST"', 'fetch(']) {
      assert.ok(!src.includes(forbidden), `${file} can write: ${forbidden}`)
    }
  }
})

/* ── C20: Free and Pro, with no approval trial ─────────────────────────── */

/* A v0.1.0 server could still return a running approval trial. The v0.1.1
 * screen treats that data as stale: only a valid paid licence backs Approve. */
const staleV010TrialAnswerToRefuse = (status, licence = 'absent') => served(licence, {
  schema_version: 3, // the v0.1.0 document; v4 has no such block
  paid_gate: true,
  headline: 'trial',
  trial: {
    status, days: 14, days_left: status === 'active' ? 9 : null,
    started_day: status === 'active' ? '23 Sep 2026' : null,
    ends_day: status === 'active' ? '7 Oct 2026' : null,
  },
  entitlements: { approve_version: { allowed: true, by: 'trial' } },
})
const LEGACY_NOT_STARTED = staleV010TrialAnswerToRefuse('not_started')
const LEGACY_RUNNING = staleV010TrialAnswerToRefuse('active')
const PRO_RULE = 'Approving a version needs a Pro licence.'
const PLANS_URL = 'https://thrulinehq.com/pricing'

test('C20: Free and Pro are the only licence views, with no countdown or trial section', async () => {
  for (const [name, doc, label] of [
    ['Free', ABSENT, 'Free'],
    ['legacy not started', LEGACY_NOT_STARTED, 'Free'],
    ['legacy running', LEGACY_RUNNING, 'Free'],
    ['Pro', VALID, 'Pro'],
  ]) {
    const d = page(doc)
    const line = licenceView(doc).line
    assert.match(d.body.textContent, new RegExp('\\b' + label + '\\b'), name + ': plan is missing')
    assert.match(line, new RegExp('\\b' + label + '\\b'), name + ': sidebar plan is missing')
    assert.doesNotMatch(d.body.textContent, /trial|\b\d+ days? left\b/i, name + ': obsolete countdown or copy')
    assert.equal(d.querySelector('.ws-licence-trial'), null, name + ': obsolete section is visible')
    await withChange(doc, async ({ d: screen }) => {
      assert.doesNotMatch(screen.body.textContent, /trial|\b\d+ days? left\b/i, name + ': Change screen offers a countdown')
    })
  }
})

test('C20: an unlicensed Approve gives the Pro requirement, pricing link, and licence-install path', async () => {
  const d = page(ABSENT)
  assert.ok(d.body.textContent.includes(PRO_RULE), 'the licence page does not state the Pro requirement')
  assert.equal(d.querySelector('.ws-licence-install code').textContent, 'thruline licence install <file>')
  await withChange(ABSENT, async ({ d: screen, type, asked }) => {
    await type(screen.querySelector('.ws-decision input'), 'Sam')
    const [approve] = buttonsNamed(screen, APPROVE)
    assert.ok(approve && approve.disabled, 'without Pro, Approve is available')
    const why = screen.getElementById(approve.getAttribute('aria-describedby'))
    assert.ok(why && why.textContent.includes(PRO_RULE), 'the Approve door does not say the same Pro rule')
    const plans = screen.querySelector('.ws-decision a[href="' + PLANS_URL + '"]')
    assert.ok(plans, 'beside Approve there is no pricing link')
    assert.equal(plans.getAttribute('target'), '_blank')
    assert.ok(screen.querySelector('.ws-decision a[href="#/licence"]'), 'the install instructions have no door')
    assert.ok(!asked.some((request) => request.startsWith('POST')), 'drawing the refusal wrote data')
  })
})

test('C20: stale unstarted or running trial data cannot open approving', async () => {
  for (const [name, doc] of [['not started', LEGACY_NOT_STARTED], ['running', LEGACY_RUNNING]]) {
    const view = licenceView(doc)
    assert.equal(view.approve.allowed, false, name + ': an old entitlement opened approving')
    assert.equal(view.startsTrialSentence || null, null, name + ': a confirming sheet promises a trial')
    assert.doesNotMatch(view.sentence, /trial/i, name + ': stale copy reached the page')
    await withChange(doc, async ({ d, type, asked }) => {
      await type(d.querySelector('.ws-decision input'), 'Sam')
      const [approve] = buttonsNamed(d, APPROVE)
      assert.ok(approve && approve.disabled, name + ': Approve is on')
      assert.ok(!asked.some((request) => request.startsWith('POST')), name + ': the screen wrote data')
    })
  }
})

test('C20: an expired paid licence cannot approve from a stale trial entitlement', () => {
  /* Both inputs: the v0.2 answer for an expired licence, and the same licence
   * with a v0.1.0 grant left on it. Each refuses, with the renew path. */
  for (const [name, doc] of [['v0.2 expired', EXPIRED], ['expired, stale v0.1.0 grant', staleV010TrialAnswerToRefuse('active', 'expired')]]) {
    const view = licenceView(doc)
    assert.equal(view.approve.allowed, false, name)
    assert.equal(view.approve.reason, 'This licence expired on 1 Jan 2020.', name)
    assert.equal(view.approve.enabler, RENEW, name)
    assert.doesNotMatch(page(doc).body.textContent, /trial/i, name)
  }
})

test('C20: withdrawing a recorded approval still works on Free', async () => {
  const approval = {
    id: 'cd_1', act: 'approved', target_revision: CHANGE.implementation.snapshot_id, scope_version: 1,
    actor: 'sam', decided_at: '2026-09-22T11:00:00Z', withdraws: null, record_path: null,
  }
  const approved = { ...CHANGE, decisions: { ...CHANGE.decisions, standing: approval, history: [approval] } }
  await withChange(ABSENT, async ({ d, type }) => {
    await type(d.querySelector('.ws-decision input'), 'Sam')
    const withdraw = buttonsNamed(d, HQ_COPY.change.model.decision.withdraw)
    assert.equal(withdraw.length, 1)
    assert.equal(withdraw[0].disabled, false, 'Free shut Withdraw')
  }, approved)
})

/* One Change whose evidence is not ready: the refusal is about readiness,
 * so a pricing link must not appear beside that refusal. */
const NEEDS_EVIDENCE = {
  coverage: { ...COVERAGE, criteria: [{ index: 1, text: 'The totals add up', state: 'none', items: [], removed: [] }] },
  readiness: {
    ...READINESS,
    ready: false,
    headline_slug: 'readiness_needs_evidence_one',
    obligations: [
      READINESS.obligations[0],
      { id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: 'The totals add up', outcome: 'unsatisfied', outcome_slug: 'unsatisfied', unsatisfied: 'no_evidence', unsatisfied_slug: 'readiness_no_evidence', evidence_name: null, disposition: null, waivable: true },
    ],
    rests_on: [],
    missing: ['criterion:1'],
  },
}

/* ── the paid gate (R-2026-09-24-25): ON as shipped, since LICENCE-GATE-ON ── */

/* The paid gate is on as shipped (LICENCE-GATE-ON). This is what the server
 * serves with it off, in a build with no licence key or a debug build told to
 * turn it off: no licence installed, and approving open on the server's word alone. */
const GATE_OFF = served('absent', { paid_gate: false, headline: 'licence', entitlements: { approve_version: { allowed: true } } })
const GATE_OFF_NO_KEY = served('no_key', { paid_gate: false, headline: 'licence', entitlements: { approve_version: { allowed: true } } })

test('gate OFF — with no licence and no trial, Approve turns on once a name is in the box, and no licence door is shown', async () => {
  for (const doc of [GATE_OFF, GATE_OFF_NO_KEY]) {
    const v = licenceView(doc)
    assert.equal(v.gateOff, true)
    assert.equal(v.approve.allowed, true, `${doc.status}: the gate is off, and approving was still held shut`)
    await withChange(doc, async ({ d, type }) => {
      const [off] = buttonsNamed(d, APPROVE)
      assert.equal(off.disabled, true, 'no name yet: off for that reason, not for a licence')
      assert.ok(d.body.textContent.includes(HQ_COPY.change.model.decision.actor_missing))
      await type(d.querySelector('.ws-decision input'), 'Sam')
      const [on] = buttonsNamed(d, APPROVE)
      assert.equal(on.disabled, false, `${doc.status}: gate off and named — Approve is on`)
      assert.equal(d.querySelector('.ws-decision a[href="#/licence"]'), null, 'no licence door beside an open Approve')
    })
  }
})

test('gate OFF — the licence page says nothing needs a licence, and never names approving as paid', () => {
  const d = page(GATE_OFF)
  const text = d.body.textContent
  assert.ok(text.includes(L.page.needs_off), 'the page says nothing needs a licence')
  assert.ok(!text.includes(L.page.needs), 'the page names a paid surface that is open anyway')
  assert.equal(d.querySelector('.ws-licence-trial'), null, 'a trial is shown while none is counted')
})

test('C20 gate OFF — the Free page states the plan and sends nobody to buy Pro', () => {
  const d = page(GATE_OFF)
  assert.equal(d.querySelector('[data-licence-status]').textContent, L.status.absent.plan)
  assert.equal(d.querySelector('.ws-licence-fix'), null, 'a fix is offered while nothing needs one')
  assert.ok(!d.body.textContent.includes(L.status.absent.fix), 'the page sends a person to buy Pro that nothing needs')
})

test('gate flag — only an explicit `paid_gate: false` opens Approve without a licence or trial behind it', () => {
  /* A document without the field, or with anything but `false`, is read as
   * the gate being on: the server's word needs a source behind it. */
  for (const paid_gate of [undefined, true, 'false', 0, null]) {
    const doc = served('absent', { paid_gate, trial: null, entitlements: { approve_version: { allowed: true } } })
    assert.equal(licenceView(doc).approve.allowed, false, `paid_gate=${JSON.stringify(paid_gate)} opened Approve`)
  }
  assert.equal(licenceView(null).approve.allowed, false, 'no answer is still not a licence')
  /* Gate off does not override a server that says no. */
  assert.equal(licenceView({ ...GATE_OFF, entitlements: { approve_version: { allowed: false, status: 'absent' } } }).approve.allowed, false)
})

/* ADV-dist-copy-3 — a build with no licence key built in (a scratch build; the
 * shipped one carries the production key, LICENCE-GATE-ON): the server forces
 * the gate OFF (`GATE_OFF_NO_KEY` is what it serves). Nothing in it needs a
 * licence and nothing is wrong, so the page and the sidebar say exactly that,
 * and offer no command that this build always refuses. */
test('gate OFF, no key — the page says the plain truth, offers no install it would refuse, and the sidebar is calm', async () => {
  const d = page(GATE_OFF_NO_KEY)
  assert.equal(d.querySelector('[data-licence-status]').textContent,
    "This copy of ThruLine doesn't check licences, and nothing in it needs one.")
  assert.equal(d.querySelector('.ws-licence-fix').textContent, "You don't need to do anything.")
  assert.equal(d.querySelector('.ws-licence-install'), null, 'the page offers an install this build always refuses')
  assert.ok(!d.body.textContent.includes(L.page.install_command), 'the install command is on the page')
  assert.equal(d.querySelectorAll('.ws-change-primary').length, 0, 'a copy control for a command that always refuses')
  assert.doesNotMatch(d.body.textContent, /official release|built without|needs a look/i)
  assert.ok(d.body.textContent.includes(L.page.needs_off), 'the page still says nothing needs a licence')
  await withChange(GATE_OFF_NO_KEY, async ({ d: screen }) => {
    const line = screen.querySelector('.ws-shell-licence a')
    assert.equal(line.textContent, 'Licence: not needed')
    assert.equal(line.getAttribute('href'), '#/licence', 'the line is still the door to the page')
  })
  /* Only that build. A build with a key and the gate OFF still offers the
   * install, and a licence file that does not work still needs a look. */
  assert.ok(page(GATE_OFF).querySelector('.ws-licence-install'), 'a build with a key lost its install command')
  await withChange(served('tampered', { paid_gate: false, entitlements: { approve_version: { allowed: true } } }), async ({ d: screen }) => {
    assert.equal(screen.querySelector('.ws-shell-licence a').textContent, L.line.problem)
  })
})

/* ── ruling 2: Pro is the only plan anyone can buy or see ─────────────── */

/* Founder ruling 2 (D-298): "Pro is the only purchasable plan. Team licence code
 * may stay dormant, but Team must not be buyable or shown anywhere a user sees." */

test('ruling 2: the licence copy names no plan but Pro, and no copy string anywhere says Team', () => {
  assert.deepEqual(Object.keys(L.plans), ['pro'])
  assert.deepEqual(Object.values(L.plans), ['Pro'])
  const walk = (obj, prefix, out) => {
    for (const [k, v] of Object.entries(obj)) {
      if (typeof v === 'string') out.push([prefix + k, v])
      else if (v && typeof v === 'object') walk(v, prefix + k + '.', out)
    }
    return out
  }
  for (const [path, text] of walk(HQ_COPY, '', [])) {
    assert.doesNotMatch(text, /\bTeam\b/, `${path} names Team: ${text}`)
  }
  const golden = JSON.parse(readFileSync(join(here, 'hq-copy.golden.json'), 'utf8'))
  for (const [k, v] of Object.entries(golden)) {
    assert.doesNotMatch(String(v), /\bTeam\b/, `golden ${k}: ${v}`)
    assert.doesNotMatch(k, /plans\.(?!pro$)/, `golden ${k}: a plan other than Pro`)
  }
})

test('ruling 2: a dormant Team licence reads as "a ThruLine licence" — never Team, never Pro — and keeps approving open', async () => {
  const TEAM = served('valid', { plan: 'team' })
  const v = licenceView(TEAM)
  assert.equal(v.status, 'valid')
  assert.equal(v.sentence, 'This machine has a ThruLine licence, valid until 23 Sep 2027.')
  assert.equal(v.line, 'Licence: valid until 23 Sep 2027')
  assert.equal(v.approve.allowed, true)
  const d = page(TEAM)
  assert.equal(d.querySelector('[data-licence-status]').textContent, v.sentence)
  const details = d.querySelector('details')
  if (details) details.remove()
  assert.doesNotMatch(d.querySelector('[data-licence-status]').textContent, /\bTeam\b|\bPro\b/, 'the dormant licence status must not claim a sold plan')
  assert.doesNotMatch(d.body.textContent, /\bTeam\b/, 'the dormant plan must not be named on the page')
  await withChange(TEAM, async ({ d: c }) => {
    assert.equal(c.querySelector('.ws-shell-licence a').textContent, 'Licence: valid until 23 Sep 2027')
    assert.doesNotMatch(c.body.textContent, /\bTeam\b/)
  })
  /* A plan this build has never heard of is still not trusted as valid. */
  assert.equal(licenceView(served('valid', { plan: 'gold' })).status, 'unknown')
})

/* ── PRICING-TRUTHS-IN-BINARY: a shut Approve points at the plans page ────
 *
 * Beside an Approve that is shut for a licence reason — any refusal the
 * licence document names: no licence, a licence that cannot be used, or an
 * expired one — a click-only "See plans ↗"
 * opens https://thrulinehq.com/pricing in the browser, and a visible sentence
 * says it opens the website (A3-N11's form: no amount). Not beside an open
 * Approve, not with the gate off, not when the licence could not be read (that
 * is not known to be a licence reason), and not beside an Approve shut for a
 * reason a licence does not fix. Nothing is fetched and nothing is sent. */

const plansLinks = (d) => [...d.querySelectorAll('a')].filter((a) => a.getAttribute('href') === PLANS_URL || a.textContent === L.approve.plans)

const LICENCE_SHUT = Object.keys(L.status).filter((k) => k !== 'valid' && k !== 'unknown' && k !== 'no_key').map((k) => [k, served(k)])

test('PRICING-TRUTHS: beside every Approve shut for a licence reason, one click-only See plans ↗ to the pricing page, with a visible sentence saying it opens the website', async () => {
  for (const [name, doc] of LICENCE_SHUT) {
    await withChange(doc, async ({ d, type, asked, w }) => {
      await type(d.querySelector('.ws-decision input'), 'Sam')
      const [approve] = buttonsNamed(d, APPROVE)
      assert.equal(approve.disabled, true, `${name}: the fixture shuts Approve`)
      const links = plansLinks(d)
      assert.equal(links.length, 1, `${name}: ${links.length} plans links beside the shut Approve`)
      const [plans] = links
      assert.ok(plans.closest('.ws-decision'), `${name}: the link is inside the decision, beside Approve`)
      assert.equal(plans.textContent, 'See plans ↗')
      assert.equal(plans.getAttribute('href'), PLANS_URL)
      assert.equal(plans.getAttribute('target'), '_blank', `${name}: opens in the browser, not in place of the work`)
      const rel = (plans.getAttribute('rel') || '').split(/\s+/)
      assert.ok(rel.includes('noopener') && rel.includes('noreferrer'), `${name}: rel is "${plans.getAttribute('rel')}" — the page could reach back, or be told where it came from`)
      const said = d.getElementById(plans.getAttribute('aria-describedby') || '')
      assert.ok(said, `${name}: the link is described`)
      assert.equal(said.textContent, 'Opens thrulinehq.com in your browser.')
      assert.ok(!said.hidden && !/ws-shell-sr/.test(said.className || ''), `${name}: the sentence is on screen, not only read aloud`)
      assert.ok(said.closest('.ws-decision'), `${name}: the sentence is beside the link`)
      /* Plan chrome is neutral: a secondary link, never the primary. */
      assert.ok(!plans.classList.contains('ws-change-primary'), `${name}: the plans link competes with the primary`)
      /* Nothing is fetched, nothing is sent, nothing opens by itself. */
      assert.ok(!asked.some((a) => /thrulinehq|pricing/.test(a)), `${name}: the screen asked the website for something: ${asked.join(', ')}`)
      assert.ok(!asked.some((a) => a.startsWith('POST')), `${name}: rendering wrote nothing`)
      assert.equal(w.location.href, 'http://127.0.0.1/#/changes/ch1', `${name}: something navigated by itself`)
    })
  }
})

test('PRICING-TRUTHS: no plans link beside an open Approve, with the gate off, when the licence could not be read, or beside a Withdraw', async () => {
  const TEAM = served('valid', { plan: 'team' })
  for (const [name, doc] of [['valid', VALID], ['dormant Team', TEAM], ['gate off', GATE_OFF], ['gate off, no key', GATE_OFF_NO_KEY], ['not read', null]]) {
    await withChange(doc, async ({ d, type }) => {
      const input = d.querySelector('.ws-decision input')
      assert.ok(input, `${name}: the decision rendered`)
      await type(input, 'Sam')
      assert.equal(plansLinks(d).length, 0, `${name}: See plans is offered where a licence is not the reason`)
      assert.ok(!d.body.textContent.includes(L.approve.plans_opens), `${name}: the sentence is on screen without its link`)
    })
  }
  /* An Approve shut because evidence is missing, with a licence that works. */
  for (const stage of ['verify', 'approve']) {
    await withChange(VALID, async ({ d, type }) => {
      const input = d.querySelector('.ws-decision input')
      if (input) await type(input, 'Sam')
      assert.ok(d.body.textContent.length > 200, `${stage}: the screen rendered`)
      assert.ok(buttonsNamed(d, APPROVE).every((b) => b.disabled), `${stage}: the fixture leaves Approve on`)
      assert.equal(plansLinks(d).length, 0, `${stage}: readiness, not a licence, shuts this Approve`)
    }, CHANGE, { coverage: NEEDS_EVIDENCE, stage })
  }
  /* A standing approval offers Withdraw, which is never paid, and no plans. */
  const APPROVAL = {
    id: 'cd_1', act: 'approved', target_revision: CHANGE.implementation.snapshot_id, scope_version: 1,
    actor: 'sam', decided_at: '2026-09-22T11:00:00Z', withdraws: null, record_path: null,
  }
  await withChange(ABSENT, async ({ d, type }) => {
    await type(d.querySelector('.ws-decision input'), 'Sam')
    assert.equal(plansLinks(d).length, 0, 'beside Withdraw')
  }, { ...CHANGE, decisions: { ...CHANGE.decisions, standing: APPROVAL, history: [APPROVAL] } })
})

test('PRICING-TRUTHS: the model says when to offer plans — exactly the licence refusals', () => {
  for (const [name, doc] of LICENCE_SHUT) assert.equal(licenceView(doc).approve.plans, true, name)
  for (const [name, doc] of [['valid', VALID], ['gate off', GATE_OFF], ['not read', null], ['no_key: nothing needs a licence', served('no_key')], ['older server, no slug', { ...ABSENT, entitlements: { approve_version: { allowed: false } } }]]) {
    assert.ok(!licenceView(doc).approve.plans, name)
  }
})

test('PRICING-TRUTHS: the pricing page is named only in the refusal copy and link, and nothing opens or fetches it by itself', () => {
  const src = join(here, '..')
  const files = []
  const walkDir = (dir) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const p = join(dir, e.name)
      if (e.isDirectory()) walkDir(p)
      else if (/\.(jsx?|mjs)$/.test(e.name) && !/\.test\.(js|mjs)$/.test(e.name)) files.push(p)
    }
  }
  walkDir(src)
  assert.ok(files.length > 50, `found ${files.length} source files`)
  const naming = []
  for (const f of files) {
    const text = readFileSync(f, 'utf8')
    assert.doesNotMatch(text, /window\.open\s*\(/, `${f} opens a window by itself`)
    assert.doesNotMatch(text, /location\.(assign|replace)\s*\(\s*['"`]https?:/, `${f} navigates off the machine by itself`)
    assert.doesNotMatch(text, /fetch\s*\(\s*['"`]https?:/, `${f} fetches from the web`)
    if (/thrulinehq\.com\/pricing/.test(text)) naming.push(f)
  }
  assert.deepEqual(naming.map((f) => f.slice(src.length + 1)).sort(), ['workspace/hq-copy.js', 'workspace/licence-view.js'], 'the pricing page is named outside its refusal copy and link')
  const view = readFileSync(join(here, 'licence-view.js'), 'utf8')
  assert.match(view, /export const PLANS_URL = 'https:\/\/thrulinehq\.com\/pricing'/)
})

test('PRICING-TRUTHS: Free and Pro screens name no other tier or amount', async () => {
  const TEAM = served('valid', { plan: 'team' })
  for (const [name, doc] of [['absent', ABSENT], ['dormant Team', TEAM], ['valid Pro', VALID], ['gate off', GATE_OFF]]) {
    for (const stage of ['verify', 'approve']) {
      await withChange(doc, async ({ d, type }) => {
        const input = d.querySelector('.ws-decision input')
        if (input) await type(input, 'Sam')
        /* Text node by text node: `textContent` runs "Explore" and the next
         * button's label together, and a word boundary would vanish. */
        const texts = []
        const walker = d.createTreeWalker(d.body, 4)
        for (let n = walker.nextNode(); n; n = walker.nextNode()) texts.push(n.nodeValue)
        const words = [...texts, ...[...d.querySelectorAll('[aria-label], [title]')].flatMap((el) => [el.getAttribute('aria-label') || '', el.getAttribute('title') || ''])].join('\n')
        assert.doesNotMatch(words, /\bTeams?\b|\bEnterprises?\b/, `${name}/${stage}: a tier nobody can buy is on screen`)
        assert.doesNotMatch(words, /[$€£]\d|\bUSD\b|\bper (month|year|seat)\b|\/mo\b/i, `${name}/${stage}: an amount is on screen`)
        assert.doesNotMatch(words.replaceAll('Licence: Pro,', '').replaceAll('Pro licence', '').replaceAll('Get Pro at https://thrulinehq.com/pricing', ''), /\bPro\b/, `${name}/${stage}: "Pro" names something other than the plan`)
      }, CHANGE, { stage })
    }
  }
})
