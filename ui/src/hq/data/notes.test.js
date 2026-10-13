import { test } from 'node:test'
import assert from 'node:assert/strict'
import { notesOf, whenOf, NOTE_MAX_CHARS } from './notes.js'

test('notesOf: the served notes, oldest first, with who and when and the decision they were kept with', () => {
  const change = {
    notes: [
      { id: 'cn_1', actor: 'alex', body: 'Looks right to me.', recorded_at: '2026-10-07T12:30:00Z', decision_id: null },
      { id: 'cn_2', actor: 'sam', body: 'Read the diff by hand.', recorded_at: '2026-10-07T13:00:00Z', decision_id: 'cd_9' },
    ],
  }
  const notes = notesOf(change)
  assert.deepEqual(notes.map((n) => [n.id, n.who, n.body, n.decisionId]), [
    ['cn_1', 'alex', 'Looks right to me.', null],
    ['cn_2', 'sam', 'Read the diff by hand.', 'cd_9'],
  ])
  assert.ok(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/.test(notes[0].when), notes[0].when)
  assert.equal(notes[0].at, '2026-10-07T12:30:00Z')
})

test('notesOf: nothing served is no notes, and a note with no words is not drawn', () => {
  assert.deepEqual(notesOf(null), [])
  assert.deepEqual(notesOf({}), [])
  assert.deepEqual(notesOf({ notes: 'no' }), [])
  assert.deepEqual(notesOf({ notes: [{ id: 'cn_1', actor: 'sam', body: '   ', recorded_at: '2026-10-07T12:30:00Z' }, null] }), [])
  assert.equal(notesOf({ notes: [{ id: 'cn_1', body: 'hi', recorded_at: 'nope' }] })[0].who, 'someone')
  assert.equal(notesOf({ notes: [{ id: 'cn_1', body: 'hi', recorded_at: 'nope' }] })[0].when, '')
})

test('whenOf: a moment the record holds, or nothing', () => {
  assert.equal(whenOf(''), '')
  assert.equal(whenOf(undefined), '')
  assert.equal(whenOf('not a time'), '')
  assert.ok(whenOf('2026-10-07T12:30:00Z').startsWith('2026-10-0'))
})

test('the limit is the record’s own', () => {
  assert.equal(NOTE_MAX_CHARS, 2000)
})
