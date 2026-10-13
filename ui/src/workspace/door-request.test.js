/* PATH-UI-2-R2 — the request a door on one screen makes of another.
 *
 * The whole-story page is its own root, so a link from it to the Change
 * screen is a NEW document (`routes.js` `watchAddress`). What the person meant
 * — "open this line" — has to travel in the address, or it is lost on the way.
 * This is the one place that writes it and the one place that reads it, and it
 * is PURE: strings in, strings out, no window and no storage.
 */

import test from 'node:test'
import assert from 'node:assert/strict'
import { DOOR_PARAM, doorHref, doorFromSearch, searchWithoutDoor } from './door-request.js'

test('the address of a door names the Change, the Verify stage and the line', () => {
  assert.equal(DOOR_PARAM, 'door')
  assert.equal(doorHref('ch1', 2, ''), '?door=2#/changes/ch1/verify')
  assert.equal(doorHref('ch1', 1), '?door=1#/changes/ch1/verify')
})

test('whatever else the address carried stays on it, and an earlier request is replaced, never doubled', () => {
  assert.equal(doorHref('ch1', 2, '?altitude=explore'), '?altitude=explore&door=2#/changes/ch1/verify')
  assert.equal(doorHref('ch1', 2, 'altitude=pro'), '?altitude=pro&door=2#/changes/ch1/verify')
  assert.equal(doorHref('ch1', 3, '?door=1&altitude=explore'), '?altitude=explore&door=3#/changes/ch1/verify')
  assert.equal((doorHref('ch1', 3, '?door=1&door=2').match(/door=/g) || []).length, 1)
})

test('an id is written as the one address grammar reads it: encoded, never trusted', () => {
  assert.equal(doorHref('a b/c', 1, ''), '?door=1#/changes/a%20b%2Fc/verify')
  assert.equal(doorHref('', 1, ''), '?door=1#/changes//verify', 'an empty id is written as it is; the routing table refuses the address, not this file')
})

test('a request is read only when it is one positive whole number of at most four digits', () => {
  assert.deepEqual(doorFromSearch('?door=1'), { n: 1 })
  assert.deepEqual(doorFromSearch('door=2'), { n: 2 })
  assert.deepEqual(doorFromSearch('?altitude=explore&door=12'), { n: 12 })
  assert.deepEqual(doorFromSearch('?door=9999'), { n: 9999 })
  for (const bad of ['', '?', '?door', '?door=', '?door=0', '?door=-1', '?door=1.5', '?door=abc', '?door=1e2', '?door=+3', '?door= 3', '?door=03', '?door=10000', '?door=99999999999999999999', '?altitude=explore']) {
    assert.equal(doorFromSearch(bad), null, `accepted: ${JSON.stringify(bad)}`)
  }
})

test('two requests in one address are neither: a reading that had to pick one would be a guess', () => {
  assert.equal(doorFromSearch('?door=1&door=2'), null)
  assert.equal(doorFromSearch('?door=1&door=1'), null)
})

test('anything that is not a string is no request', () => {
  for (const v of [null, undefined, 3, {}, [], true]) assert.equal(doorFromSearch(v), null)
})

test('the request is taken off the address once it has been used, and nothing else is', () => {
  assert.equal(searchWithoutDoor('?door=2'), '')
  assert.equal(searchWithoutDoor('?door=2&altitude=explore'), '?altitude=explore')
  assert.equal(searchWithoutDoor('?altitude=explore&door=2'), '?altitude=explore')
  assert.equal(searchWithoutDoor('?altitude=explore'), '?altitude=explore')
  assert.equal(searchWithoutDoor(''), '')
  assert.equal(searchWithoutDoor('?door=1&door=2'), '')
  assert.equal(searchWithoutDoor(null), '')
})

test('what a door writes is what the reader reads back', () => {
  for (const n of [1, 2, 9, 10, 9999]) {
    for (const search of ['', '?altitude=explore', '?altitude=pro&x=1']) {
      const href = doorHref('ch1', n, search)
      const url = new URL(href, 'http://127.0.0.1/')
      assert.deepEqual(doorFromSearch(url.search), { n }, href)
      assert.equal(url.hash, '#/changes/ch1/verify')
      assert.equal(searchWithoutDoor(url.search), search ? new URL(search.startsWith('?') ? search : '?' + search, 'http://x/').search : '', href)
    }
  }
})

test('a line that is not a whole number is not written at all', () => {
  for (const bad of [0, -1, 1.5, NaN, Infinity, '2', null, undefined, 10000]) {
    assert.equal(doorHref('ch1', bad, ''), '#/changes/ch1/verify', `${String(bad)}: falls back to the plain address, and says nothing it cannot back`)
  }
})
