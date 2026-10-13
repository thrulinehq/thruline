import test from 'node:test'
import assert from 'node:assert/strict'
import { observationView } from './observation-model.js'
test('old connected state cannot survive a failed status read', () => assert.match(observationView({ enabled: true, state: 'watching' }, 'unreachable').label, /disconnected/))
test('verification without an observer never establishes capture', () => assert.match(observationView({ enabled: false }).label, /not configured/))
test('automatic file observation preserves its evidence ceiling', () => assert.match(observationView({ enabled: true, state: 'watching' }).summary, /agent reports, not execution proof/))
test('pause and missing source are readable without color', () => {
  assert.match(observationView({ enabled: true, state: 'paused' }).label, /paused/)
  assert.match(observationView({ enabled: true, state: 'disconnected' }).label, /disconnected/)
})
