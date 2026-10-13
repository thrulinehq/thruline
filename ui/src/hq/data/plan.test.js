/* C30 P0-6: the plan rule is ONE function, used everywhere.
 *
 * Pro = a valid licence that allows approving (C21-b, C30). The rule reads
 * `GET /api/licence` (`thruline.licence.status` v4, `licence_cmd::status_body`)
 * and decides from the server's own explicit answer,
 * `entitlements.approve_version.allowed`, together with `status`. A document
 * that does not carry that answer is not read as a plan at all.
 *
 * The cross-screen test feeds ONE licence document to Settings, Approve and
 * Team-View — each through its own selector and its own logic, as each screen
 * runs — and asserts that all three say the same plan. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const screens = join(here, '..', 'screens')

/* The three screens' selectors and logics, bundled as the screens run them. */
const S = await (async () => {
  const ENTRY = `
import SettingsLogic from '../screens/Settings/logic.js'
import ApproveLogic from '../screens/Approve-Sheet/logic.js'
import TeamLogic from '../screens/Team-View/logic.js'
import { settingsFromServer } from '../screens/Settings/from-server.js'
import { selectApproveSheet } from '../screens/Approve-Sheet/from-change.js'
import { teamFromStore } from '../screens/Team-View/from-store.js'
import { DEMO as BRIEF } from '../screens/Change-Brief/demo.js'
const vals = (L, data) => { const i = new L({ data }); i.props = { data }; return i.renderVals() }
export function settings(licence) { return vals(SettingsLogic, settingsFromServer({ licence, now: new Date(Date.UTC(2026, 9, 7, 12)) })) }
export function approve(licence) { return vals(ApproveLogic, selectApproveSheet(BRIEF.states.ready, null, { licence })) }
export function team(licence) {
  const project = { id: 'p1', name: 'shop', members: [], whose_move: { you: 0 } }
  return vals(TeamLogic, teamFromStore({ project, projects: [project], changes: [], storage: null, licence, now: Date.UTC(2026, 9, 7, 12) }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'plan-rule-')), 'screens.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

/* The plan word each screen puts on the page, or null when it states none. */
const word = (s) => {
  const m = /^(?:Plan: )?(Pro|Free)$/.exec(String(s || '').trim())
  return m ? m[1] : null
}
const said = (licence) => ({
  settings: word(S.settings(licence).content.planNav),
  approve: word(S.approve(licence).planStatus),
  team: word(S.team(licence).plan.label),
})

/* /api/licence documents in the shape `licence_cmd::status_body` serves (v4). */
const allowed = { approve_version: { allowed: true } }
const refusedAs = (status) => ({ approve_version: { allowed: false, reason: 'r', enable: 'e', status } })
const LICENCES = {
  'a valid Pro licence': [{ status: 'valid', plan: 'pro', expires_on: '2027-10-07', paid_gate: true, headline: 'licence', entitlements: allowed }, 'Pro'],
  'a valid licence whose plan field is not "pro"': [{ status: 'valid', plan: 'team', expires_on: '2027-10-07', paid_gate: true, headline: 'licence', entitlements: allowed }, 'Pro'],
  'an expired licence': [{ status: 'expired', plan: null, expires_on: '2026-09-01', paid_gate: true, headline: 'licence', entitlements: refusedAs('expired') }, 'Free'],
  'no licence': [{ status: 'absent', plan: null, expires_on: null, paid_gate: true, headline: 'free', entitlements: refusedAs('absent') }, 'Free'],
  'a valid licence the server does not let approve': [{ status: 'valid', plan: 'pro', expires_on: '2027-10-07', paid_gate: true, entitlements: refusedAs('wrong_machine') }, 'Free'],
  'a malformed answer: no explicit approving answer': [{ status: 'valid', plan: 'pro' }, null],
  'a malformed answer: a status that is not a word': [{ status: 7, entitlements: allowed }, null],
  'a malformed answer: not a document': ['<html>502</html>', null],
  'a malformed answer: an array': [[], null],
  'no answer at all': [null, null],
}

for (const [name, [doc, expected]] of Object.entries(LICENCES)) {
  test(`one licence document, one plan on all three screens: ${name}`, () => {
    const s = said(doc)
    assert.deepEqual(s, { settings: expected, approve: expected, team: expected }, JSON.stringify(s))
  })
}

test('no trial words on any of the three screens, for any licence document (C20)', () => {
  for (const [doc] of Object.values(LICENCES)) {
    for (const v of [S.settings(doc).content.planNav, S.approve(doc).planStatus, S.approve(doc).planLine, S.team(doc).plan.label, S.team(doc).plan.lead]) {
      assert.ok(!/trial/i.test(String(v)), String(v))
    }
  }
})

/* ---- the one function ---- */

test('plan.js: Pro is a valid licence the server says may approve; Free is a read answer that is not; null is no answer', async () => {
  const { planOf, licencePlan, PLAN_WORDS } = await import('./plan.js')
  for (const [name, [doc, expected]] of Object.entries(LICENCES)) {
    assert.equal(planOf(doc), expected == null ? null : expected.toLowerCase(), name)
    const p = licencePlan(doc)
    assert.equal(p.plan, planOf(doc), name)
    assert.equal(p.word, expected, name)
  }
  assert.deepEqual(PLAN_WORDS, { pro: 'Pro', free: 'Free' })
})

test('plan.js: approving follows the server’s own answer, and the plan never widens it', async () => {
  const { licencePlan } = await import('./plan.js')
  assert.equal(licencePlan(LICENCES['a valid Pro licence'][0]).approving, true)
  assert.equal(licencePlan(LICENCES['a valid licence whose plan field is not "pro"'][0]).approving, true)
  for (const n of ['an expired licence', 'no licence', 'a valid licence the server does not let approve', 'a malformed answer: no explicit approving answer', 'no answer at all']) {
    assert.equal(licencePlan(LICENCES[n][0]).approving, false, n)
  }
  /* a build whose paid gate is off: nothing needs Pro, so approving is open while the plan stays Free */
  const off = licencePlan({ status: 'absent', plan: null, paid_gate: false, entitlements: allowed })
  assert.deepEqual([off.plan, off.approving], ['free', true])
  /* an older answer that says "allowed" with no licence behind it and the gate on is not a grant */
  assert.equal(licencePlan({ status: 'absent', paid_gate: true, entitlements: allowed }).approving, false)
})

test('census: Settings, Approve and Team-View decide the plan only through data/plan.js', () => {
  const files = []
  for (const dir of ['Settings', 'Approve-Sheet', 'Team-View']) {
    for (const n of readdirSync(join(screens, dir))) {
      const p = join(screens, dir, n)
      if (statSync(p).isFile() && /\.(js|jsx)$/.test(n) && !n.endsWith('.test.js')) files.push([dir + '/' + n, readFileSync(p, 'utf8')])
    }
  }
  for (const [n, src] of files) {
    assert.ok(!/licence\w*\.status\s*===/.test(src), `${n} reads the licence status itself`)
    assert.ok(!/licence\w*\.plan\s*===/.test(src), `${n} reads the licence plan field itself`)
  }
  for (const n of ['Settings/from-server.js', 'Approve-Sheet/from-change.js', 'Team-View/from-store.js']) {
    assert.ok(files.find(([f]) => f === n)[1].includes("from '../../data/plan.js'"), `${n} calls the one plan rule`)
  }
})
