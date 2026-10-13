import { test } from 'node:test'
import assert from 'node:assert/strict'
import { acceptedAfterWork } from './after-work.js'

test('the instants decide when both are held — across midnight, and not by the faces', () => {
  const at = (d, h, m) => new Date(2026, 9, d, h, m, 0).toISOString() // local time: the faces are 23:50 / 00:10 in any timezone
  assert.equal(acceptedAfterWork(at(7, 0, 10), at(6, 23, 50), '00:10', '23:50'), true)
  assert.equal(acceptedAfterWork(at(6, 23, 40), at(6, 23, 50), '23:40', '23:50'), false)
  assert.equal(acceptedAfterWork(at(5, 23, 59), at(6, 0, 5), '23:59', '00:05'), false)
  assert.equal(acceptedAfterWork(at(6, 12, 0), at(6, 12, 0), '12:00', '12:00'), false, 'a tie is not after')
})

test('the faces decide only when the record holds neither instant (the gallery)', () => {
  assert.equal(acceptedAfterWork(undefined, undefined, '16:06', '16:04'), true)
  assert.equal(acceptedAfterWork(null, null, '16:04', '16:06'), false)
  assert.equal(acceptedAfterWork('2026-10-07T00:10:00Z', null, '16:06', '16:04'), true, 'one instant is not two: the faces')
  assert.equal(acceptedAfterWork(undefined, undefined, undefined, undefined), false)
})
