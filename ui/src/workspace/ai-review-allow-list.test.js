/* AI-REVIEW-OUT-OF-LAUNCH-R5 — one allow-list, held equal on both sides.
 *
 * Four rounds normalised spellings of the withheld review's id and each lost
 * to the next spelling, so the launch build no longer asks "is this the
 * withheld id?" of anything an operator typed. It asks "is this an id the
 * build ships?", and names an id only if so. The launch UI twin
 * (ai-review-ui.launch.js) has decided that way since round 2
 * (`isAiReviewPack` is `!SHIPPED.has(id)`); the engine now does the same, from
 * `SHIPPED_IDS` in crates/cli/src/integration_cmd.rs. This file pins:
 *   - the engine's list is the twin's set, id for id (and not the withheld id);
 *   - the twin refuses every look-alike the round-4 adversary served and every
 *     ASCII name that contains the withheld id, exactly as the engine does;
 *   - the sentences a build that runs no AI review says instead of naming an
 *     id are word for word the copy file's, the Rust doors' and the Rust
 *     tests', so the terminal and the screen cannot say two things.
 *
 * Every fixture is synthetic. */
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { HQ_COPY } from './hq-copy.js'
import { launchCatalog } from '../../vite.config.js'

const read = (rel) => readFileSync(new URL(rel, import.meta.url), 'utf8')
const ENGINE = read('../../../crates/cli/src/integration_cmd.rs')
const LAUNCH_TESTS = read('../../../crates/cli/tests/ai_review_out_of_launch.rs')
const INPUTS = read('../../../crates/cli/tests/support/review_fixture.rs')

/** A Rust string literal's text: `\u{1d429}` and `\"` unescaped. */
const unrust = (literal) => literal
  .replace(/\\u\{([0-9a-fA-F]+)\}/g, (_, hex) => String.fromCodePoint(parseInt(hex, 16)))
  .replace(/\\"/g, '"')
  .replace(/\\\\/g, '\\')

const stringConstant = (source, name) => {
  const m = source.match(new RegExp(`const ${name}: &str =\\s*"((?:[^"\\\\]|\\\\.)*)";`))
  return m ? unrust(m[1]) : null
}

const stringList = (source, name) => {
  const m = source.match(new RegExp(`const ${name}: &\\[&str\\] = &?\\[([\\s\\S]*?)\\];`))
  assert.ok(m, `${name} is in the source`)
  return [...m[1].matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((x) => unrust(x[1]))
}

const engineShips = () => {
  const m = ENGINE.match(/const SHIPPED_IDS: \[&str; (\d+)\] = \[([^\]]*)\];/)
  assert.ok(m, 'SHIPPED_IDS is in integration_cmd.rs')
  const ids = [...m[2].matchAll(/"([^"]*)"/g)].map((x) => x[1])
  assert.equal(ids.length, Number(m[1]), 'the declared length is the list\'s length')
  return ids
}

test('the engine ships exactly the ids the launch UI twin ships, and not the withheld id', () => {
  const engine = engineShips()
  assert.equal(new Set(engine).size, engine.length, 'no id is listed twice')
  const twin = launchCatalog().projects.map((p) => p.id)
  assert.deepEqual([...engine].sort(), [...twin].sort(), 'SHIPPED_IDS is the launch catalog\'s ids')
  assert.ok(!engine.includes(stringConstant(ENGINE, 'AI_REVIEW_PACK')), 'the withheld id is not shipped')
})

test('the launch twin and the engine refuse the same look-alikes', async () => {
  const engine = new Set(engineShips())
  globalThis.__THRULINE_LAUNCH_SOURCE_IDS__ = launchCatalog().projects.map((p) => p.id)
  const twin = await import('./ai-review-ui.launch.js')
  // Every catalog id: shipped by the engine exactly when the twin does not call it AI review.
  const full = JSON.parse(read('../../../spec/integrations/catalog.json')).projects.map((p) => p.id)
  for (const id of full) assert.equal(!twin.isAiReviewPack(id), engine.has(id), `${id} is decided alike`)
  // Every id the Rust tests attack with, in any spelling.
  const attacks = [...stringList(INPUTS, 'ROUND_4'), ...stringList(INPUTS, 'ROUND_5'), ...stringList(INPUTS, 'NOT_A_SPELLING'), 'pr-agent']
  assert.ok(attacks.length >= 24, 'the attack list was read')
  for (const id of attacks) {
    assert.ok(twin.isAiReviewPack(id), `${JSON.stringify(id)} is not shipped, as far as the launch UI twin knows`)
    assert.ok(!engine.has(id), `${JSON.stringify(id)} is not shipped, as far as the engine knows`)
  }
  // And the deterministic tools the launch build offers are still shipped by both.
  for (const id of ['ast-grep', 'betterleaks', 'ruff', 'clippy']) {
    assert.ok(!twin.isAiReviewPack(id) && engine.has(id), `${id} is shipped`)
  }
})

const COPY = HQ_COPY.change.review.not_run_here

test('the sentences a build without AI review says instead of naming an id are the copy file\'s, word for word', () => {
  assert.equal(stringConstant(ENGINE, 'SELECTION_NOT_RUN_ONE'), COPY.selection_one, 'one selection, terminal and screen')
  assert.equal(stringConstant(ENGINE, 'SELECTION_NOT_RUN_MANY'), COPY.selection_many, 'several selections')
  assert.equal(stringConstant(ENGINE, 'CHECK_NOT_RUN'), COPY.check, 'a check the build does not run')
  // The Rust tests pin what the doors serve against the same words.
  assert.equal(stringConstant(LAUNCH_TESTS, 'SELECTION_ONE'), COPY.selection_one)
  assert.equal(stringConstant(LAUNCH_TESTS, 'SELECTION_MANY'), COPY.selection_many)
  assert.equal(stringConstant(LAUNCH_TESTS, 'CHECK_NOT_RUN'), COPY.check)
})

test('those sentences name no id, and never claim a review happened', () => {
  for (const sentence of Object.values(COPY)) {
    assert.doesNotMatch(sentence, /pr-agent|pr_agent|agent|qodo|codium|11436/i, sentence)
    assert.doesNotMatch(sentence, /no findings|0 findings|found nothing|raised nothing|\bComplete\b/, sentence)
  }
  assert.match(COPY.selection_one, /^1 registered selection is not one this build runs/)
  assert.match(COPY.selection_many, /^\{n\} registered selections are not ones this build runs/)
})
