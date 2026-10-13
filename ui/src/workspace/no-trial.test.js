/* C20: no UI copy offers an approval trial. Keep this census broad enough to
 * catch copy added outside HQ_COPY as well as stale built assets. The only
 * excluded path with a legacy `trial` field is a captured API test fixture:
 * ui/src/workspace/walk-capture.fixture.json. It is protocol data, not copy.
 * Test-only mentions are in ui/src/workspace/licence.test.js,
 * ui/src/workspace/no-trial.test.js, ui/src/workspace/pricing-census.test.js,
 * and ui/src/workspace/hq-copy.test.js (the old golden's placeholder fixture).
 */

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const ui = join(here, '../..')
const word = /trial/i

function filesUnder(dir, accept) {
  const found = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) found.push(...filesUnder(path, accept))
    else if (entry.isFile() && accept(path)) found.push(path)
  }
  return found
}

function trialHits(path) {
  const lines = readFileSync(path, 'utf8').split('\n')
  const hits = []
  for (let i = 0; i < lines.length; i += 1) {
    if (word.test(lines[i])) hits.push(`${relative(ui, path)}:${i + 1}`)
  }
  return hits
}

test('C20 UI census reads source, the copy golden, and built UI assets', () => {
  const source = filesUnder(join(ui, 'src'), (path) => /\.(?:js|jsx|mjs)$/.test(path) && !/\.test\.(?:js|mjs)$/.test(path))
  const golden = join(here, 'hq-copy.golden.json')
  const built = filesUnder(join(ui, 'dist'), (path) => /\.(?:js|html|css)$/.test(path))
  assert.ok(source.length > 100, `source census found only ${source.length} files`)
  assert.ok(built.length > 5, `built UI census found only ${built.length} files`)
  assert.ok(readFileSync(golden, 'utf8').length > 10000, 'copy golden is missing or empty')
  const hits = [...source, golden, ...built].flatMap(trialHits)
  assert.equal(hits.length, 0, `${hits.length} approval trial matches; first: ${hits.slice(0, 12).join(', ')}`)
})

test('C20 UI census catches legacy copy in every surface it scans', () => {
  for (const phrase of ['14-day free trial', 'Trial · 1 day left', 'trial.json', 'starts_trial']) {
    assert.match(phrase, word)
  }
  for (const phrase of ['Free', 'ThruLine Pro licence', '14-day paid renewal grace']) {
    assert.doesNotMatch(phrase, word)
  }
})
