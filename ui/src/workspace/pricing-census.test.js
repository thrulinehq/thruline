/* PRICING-TRUTHS-IN-BINARY — the pricing census, over every hq-copy golden value.
 *
 * The app names one plan anyone can buy (Pro, founder ruling 2, D-298), states
 * no amount anywhere (a price in the binary could not follow a price the
 * website changes), and calls its free third view by its ruled name,
 * Workbench (ruling #5, R-2026-09-26-3) — so "Pro" is the plan's name and
 * nothing else's.
 *
 * The same rules run over the binary's own words (the licence crate's Problem,
 * Verdict and refusal sentences, every `--help`, the licence status document) in
 * `crates/cli/tests/pricing_census.rs`. This file is the screen's half, so a
 * UI-only change is caught by `npm test` without a cargo build.
 *
 * Rules (each a regular expression over one rendered golden value):
 *  - an amount: `[$€£]\d`, `USD`, `per month`, `per year`, `per seat`, `/mo`;
 *  - a tier nobody can buy: `Team` or `Enterprise` as a capitalised word,
 *    which is how a plan is named (the card's rule; a lowercase common noun
 *    is not a plan). No golden
 *    value is reachable only with such a licence installed (a dormant Team
 *    licence reads `licence.status.valid.sentence_unnamed`), so there is no
 *    exception here;
 *  - `Pro` anywhere except as the plan name: only under `licence.`, and only
 *    in one of the plan-name forms below.
 */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const GOLDEN = JSON.parse(readFileSync(join(here, 'hq-copy.golden.json'), 'utf8'))

export const AMOUNT = [/[$€£]\d/, /\bUSD\b/, /\bper month\b/i, /\bper year\b/i, /\bper seat\b/i, /\/mo\b/i]
export const TIER = [/\bTeams?\b/, /\bEnterprises?\b/]
/* The plan's name, as the copy file uses it: the plan on its own, the
 * licence's sentence, and the sidebar line. */
export const PLAN_NAME_FORMS = [/^Pro$/, /\bThruLine Pro licence\b/g, /\bPro licence\b/g, /^Licence: Pro,/, /\bGet Pro at https:\/\/thrulinehq\.com\/pricing\b/g]

/** Every reason `value` (at golden key `key`) breaks the census. */
export function censusHits(key, value) {
  const hits = []
  for (const re of AMOUNT) if (re.test(value)) hits.push(`amount ${re}`)
  for (const re of TIER) if (re.test(value)) hits.push(`tier ${re}`)
  if (/\bPro\b/.test(value)) {
    let rest = value
    if (key.startsWith('licence.')) for (const re of PLAN_NAME_FORMS) rest = rest.replace(re, '')
    if (/\bPro\b/.test(rest)) hits.push('"Pro" not as the plan name')
  }
  return hits
}

test('the census reads the whole golden', () => {
  assert.ok(Object.keys(GOLDEN).length > 1500, `found ${Object.keys(GOLDEN).length} golden values`)
})

test('the census is a live instrument: each rule catches what it names, and the plan name passes', () => {
  for (const s of ['$20 a month', '€5', '£9', '20 USD', '10 per month', '100 per year', '30 per seat', '$9/mo']) {
    assert.ok(censusHits('x', s).some((h) => h.startsWith('amount')), s)
  }
  for (const s of ['Team', 'the Teams plan', 'Enterprise plan']) assert.ok(censusHits('licence.x', s).some((h) => h.startsWith('tier')), s)
  assert.deepEqual(censusHits('shell.altitudes.pro.label', 'Pro'), ['"Pro" not as the plan name'])
  assert.deepEqual(censusHits('shell.keys.x', 'Buttons in Pro now'), ['"Pro" not as the plan name'])
  assert.deepEqual(censusHits('licence.status.x', 'Pro is the free view'), ['"Pro" not as the plan name'])
  assert.deepEqual(censusHits('licence.plans.pro', 'Pro'), [])
  assert.deepEqual(censusHits('licence.status.valid.sentence', 'This machine has a ThruLine Pro licence, valid until 17 Sep 2026.'), [])
  assert.deepEqual(censusHits('licence.status.absent.problem', 'Approving a version needs a Pro licence.'), [])
  assert.deepEqual(censusHits('licence.status.absent.fix', 'Get Pro at https://thrulinehq.com/pricing, then install your licence with: thruline licence install <file>'), [])
  assert.deepEqual(censusHits('licence.line.valid', 'Licence: Pro, until 17 Sep 2026'), [])
  /* Words that only look like the rules. */
  for (const s of ['Proven', 'Prove it', 'a project', 'the model', '$ thruline ui', 'press 3', '~/.thruline/models', 'steam', 'the enterprise managed-settings snippet']) {
    assert.deepEqual(censusHits('x', s), [], s)
  }
})

test('no hq-copy golden value states an amount, names Team or Enterprise, or says "Pro" except as the plan name', () => {
  const found = []
  for (const [key, value] of Object.entries(GOLDEN)) {
    for (const hit of censusHits(key, String(value))) found.push(`${key}: ${hit}: ${JSON.stringify(value)}`)
  }
  assert.deepEqual(found, [])
})
