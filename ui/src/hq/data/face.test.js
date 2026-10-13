/* How a screen reads and changes a stored preference (data/face.js), PURE.
 *
 * Live (handed the store's state and writer) a screen draws the STORED face and
 * density and a change is a write; the design gallery hands a screen nothing, so
 * it keeps its own local value, as the canvas drew it. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { storedOf, isLive, choose, faceOf, otherFace, switchFace, densityOf, faceBar } from './face.js'
import { DEFAULTS } from './preferences.js'

const state = (over = {}) => ({ status: 'read', prefs: Object.assign({}, DEFAULTS, over), pending: {}, writeError: {} })
const live = (over, asked = []) => ({ prefs: state(over), setPreference: (n, v) => { asked.push([n, v]); return Promise.resolve(true) } })

test('live: the stored face and density; gallery (no props): the screen’s own', () => {
  assert.equal(faceOf(live({ face: 'technical' })), 'technical')
  assert.equal(faceOf(live({ face: 'plain' }), 'technical'), 'plain', 'the stored one wins over a local one')
  assert.equal(faceOf({}, 'technical'), 'technical')
  assert.equal(faceOf(undefined), 'plain')
  assert.equal(faceOf({}, 'loud'), 'plain', 'a face nobody offers is plain')
  assert.equal(densityOf(live({ density: 'compact' })), 'compact')
  assert.equal(densityOf({}, 'compact'), 'compact')
  assert.equal(densityOf({}), 'comfortable')
})

test('a stored value that is not one the route takes is the screen’s own', () => {
  assert.equal(storedOf({ prefs: { prefs: { face: 'loud' } } }, 'face', 'plain'), 'plain')
  assert.equal(storedOf({ prefs: { prefs: {} } }, 'density', 'compact'), 'compact')
  assert.equal(storedOf({ prefs: null }, 'face', 'technical'), 'technical')
})

test('choose: live it is a write through the store and nothing local; the gallery sets its own', () => {
  const asked = []
  const local = []
  choose(live({}, asked), 'face', 'technical', (v) => local.push(v))
  assert.deepEqual(asked, [['face', 'technical']])
  assert.deepEqual(local, [])
  choose({}, 'face', 'technical', (v) => local.push(v))
  assert.deepEqual(local, ['technical'])
  assert.equal(isLive({}), false)
  assert.equal(isLive({ prefs: state() }), false, 'the store without its writer is not live')
  assert.equal(isLive(live({})), true)
})

test('choose: a refusal never throws into the screen (it is kept in the store and said there)', async () => {
  const props = { prefs: state(), setPreference: () => Promise.reject(new Error('Not a choice ThruLine offers: face')) }
  choose(props, 'face', 'technical')
  await new Promise((r) => setImmediate(r))
})

test('switchFace and otherFace: the face a click or t moves to', () => {
  assert.equal(otherFace('plain'), 'technical')
  assert.equal(otherFace('technical'), 'plain')
  const asked = []
  switchFace(live({ face: 'plain' }, asked), 'technical')
  assert.deepEqual(asked, [['face', 'technical']])
})

test('faceBar: only a live screen has one; its click asks for the other face; the face in use is the bright one', () => {
  assert.equal(faceBar({}), undefined)
  const asked = []
  const plain = faceBar(live({ face: 'plain' }, asked))
  assert.deepEqual([plain.face, plain.tech, plain.aria, plain.plain, plain.technical], ['plain', false, 'Face: Plain. Switch to Technical', '#EDEDEF', undefined])
  plain.toggle()
  const tech = faceBar(live({ face: 'technical' }, asked))
  assert.deepEqual([tech.face, tech.tech, tech.aria, tech.plain, tech.technical], ['technical', true, 'Face: Technical. Switch to Plain', '#A1A1AA', '#EDEDEF'])
  tech.toggle()
  assert.deepEqual(asked, [['face', 'technical'], ['face', 'plain']])
})
