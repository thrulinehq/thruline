/* C30 R-LEDGER: second approvals, as the screens read them.
 *
 * The one sentence the ruling requires beside every name box (Q3) is said by
 * the server as well as by the screen; these tests hold the two copies
 * together by reading the Rust source, so the page cannot drift from what the
 * server serves. Nothing here reaches a server. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { NAME_NOT_CHECKED, SECOND_NAME_MAX_CHARS, secondsOf } from './seconds.js'

const rust = (rel) => readFileSync(new URL('../../../../crates/' + rel, import.meta.url), 'utf8')

test('the sentence and the limit are the server’s own', () => {
  const core = rust('core/src/change_decision_second.rs')
  const said = /NAME_NOT_CHECKED: &str =\s*"([^"]*)";/.exec(core)
  assert.ok(said, 'NAME_NOT_CHECKED is declared in change_decision_second.rs')
  assert.equal(NAME_NOT_CHECKED, said[1])
  assert.equal(NAME_NOT_CHECKED, 'ThruLine records the name you type; it does not check who is at the keyboard.')
  const server = rust('cli/src/uiserve.rs')
  const limit = /const DECISION_ACTOR_MAX_CHARS: usize = (\d+);/.exec(server)
  assert.ok(limit, 'the name limit is declared in uiserve.rs')
  assert.equal(SECOND_NAME_MAX_CHARS, Number(limit[1]))
})

const row = (over) => ({
  id: 'cds_1', act: 'approved', decision_id: 'cd_1', target_revision: 'v1', scope_version: 1,
  actor: 'Alex', recorded_at: '2026-10-06T10:30:00Z', withdraws: null, counting: true, ...over,
})

test('only seconds the server marked as counting are drawn, oldest first', () => {
  const got = secondsOf({
    seconds: [
      row({ id: 'a', actor: 'Alex' }),
      row({ id: 'b', actor: 'Kim', counting: false }), // its approval was replaced, or it was taken back
      row({ id: 'c', act: 'withdrawn', withdraws: 'b', counting: false }),
      row({ id: 'd', actor: '  Pat ', recorded_at: '2026-10-06T11:00:00Z' }),
      null,
    ],
  })
  assert.deepEqual(got.map((s) => [s.id, s.who, s.decisionId]), [['a', 'Alex', 'cd_1'], ['d', 'Pat', 'cd_1']])
  assert.ok(/^20\d\d-\d\d-\d\d \d\d:\d\d$/.test(got[0].when), got[0].when)
})

test('a Change served without seconds, or with nothing, has none — and none is made up', () => {
  assert.deepEqual(secondsOf(null), [])
  assert.deepEqual(secondsOf({}), [])
  assert.deepEqual(secondsOf({ seconds: 'x' }), [])
  assert.deepEqual(secondsOf({ seconds: [] }), [])
  // a row with no name is drawn as "someone", not dropped and not invented
  assert.equal(secondsOf({ seconds: [row({ actor: '   ' })] })[0].who, 'someone')
})
