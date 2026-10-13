/* C30 R-LEDGER: what people said was wrong with a statement, as the screens
 * read it. Held here: the model is pure, an absent key is nothing, an
 * objection to an earlier wording says so, the verdict a statement already had
 * is carried through untouched, and the words the screens say beside the box
 * are the words the server says. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { objectionsOf, objectedStatements, KEPT_BESIDE, OBJECTION_MAX_CHARS, SENTENCE_CHANGED, VERDICT_WORDS } from './objections.js'
import { changeSnapshot } from './change.js'
import { threadFixture } from '../screens/What-Changed/thread-fixture.js'

const here = dirname(fileURLToPath(import.meta.url))
const NOW = new Date(Date.UTC(2026, 9, 7, 12, 0, 0))
const SERVED = [
  { id: 'obj_1', actor: 'sam', at: '2026-10-07T11:30:00Z', reason: 'The test never ran.', matches_sentence: true },
  { id: 'obj_2', actor: 'robin', at: '2026-10-07T11:45:00Z', reason: 'Nor did the build.', matches_sentence: false },
]

test('the served objections become lines with who, when and a lead that says which wording', () => {
  const got = objectionsOf(SERVED)
  assert.deepEqual(got.map((o) => [o.id, o.who, o.reason, o.matches]), [
    ['obj_1', 'sam', 'The test never ran.', true],
    ['obj_2', 'robin', 'Nor did the build.', false],
  ])
  assert.equal(got[0].lead, 'You said this is wrong:')
  assert.equal(got[1].lead, 'You said an earlier wording of this was wrong:')
  assert.match(got[0].meta, /^sam · 20\d\d-\d\d-\d\d \d\d:\d\d$/)
  assert.equal(got[0].at, '2026-10-07T11:30:00Z')
})

test('an absent key, a non-list and an entry with no words are nothing, and nothing is made up', () => {
  assert.deepEqual(objectionsOf(undefined), [])
  assert.deepEqual(objectionsOf(null), [])
  assert.deepEqual(objectionsOf({}), [])
  assert.deepEqual(objectionsOf([null, 7, {}, { id: 'x', reason: '   ' }]), [])
  const [unnamed] = objectionsOf([{ id: 'o', reason: 'Wrong.', at: 'not a time' }])
  assert.equal(unnamed.who, 'someone')
  assert.equal(unnamed.when, '')
  assert.equal(unnamed.meta, 'someone', 'no clock reading is invented')
  // a flag the server did not send is not read as "the sentence changed"
  assert.equal(unnamed.matches, true)
})

test('the statements objected to carry the verdict they already had, and none is computed here', () => {
  const statements = [
    { id: 'a#0', step: 3, plain: 'Tests pass.', text: 'Tests pass.', proof: { verdict: 'contradicted' }, objections: objectionsOf(SERVED) },
    { id: 'a#1', step: 4, plain: 'Fixed.', proof: { verdict: 'proven' } },
    { id: 'a#2', step: 5, text: 'Raw text.', proof: { verdict: 'recorded' }, objections: objectionsOf(SERVED.slice(0, 1)) },
    { id: 'a#3', step: 6, plain: 'No proof object.', objections: objectionsOf(SERVED.slice(0, 1)) },
  ]
  const got = objectedStatements(statements)
  assert.deepEqual(got.map((s) => [s.id, s.verdict, s.verdictWord]), [
    ['a#0', 'contradicted', 'Not proven'],
    ['a#2', 'recorded', 'Recorded'],
    ['a#3', null, ''],
  ])
  assert.equal(got[1].plain, 'Raw text.')
  assert.deepEqual(objectedStatements(undefined), [])
  assert.deepEqual(objectedStatements([{ id: 'z', proof: { verdict: 'proven' } }]), [])
  for (const word of Object.values(VERDICT_WORDS)) assert.ok(word)
})

test('the Change snapshot carries objections on the statement they are about and nowhere else, and moves no verdict', () => {
  const plain = threadFixture()
  const objected = threadFixture()
  objected.claims[0].objections = SERVED
  const tracked = { change: { id: 'chg_1', notes: [] }, hq: null }
  const without = changeSnapshot(plain, tracked, NOW, {})
  const withIt = changeSnapshot(objected, tracked, NOW, {})
  const byId = (snap, id) => snap.statements.find((s) => s.id === id)
  assert.deepEqual(byId(withIt, 'm1#0').objections.map((o) => o.reason), ['The test never ran.', 'Nor did the build.'])
  assert.equal('objections' in byId(withIt, 'm1#1'), false, 'the other statement is the object it was')
  assert.equal('objections' in byId(without, 'm1#0'), false, 'nobody objected: no key')
  // with the objections taken off, the whole snapshot is the one without them
  const stripped = JSON.parse(JSON.stringify(withIt))
  for (const s of stripped.statements) delete s.objections
  assert.deepEqual(stripped, JSON.parse(JSON.stringify(without)))
  assert.equal(byId(withIt, 'm1#0').proof.verdict, byId(without, 'm1#0').proof.verdict)
})

test('the words the screens say beside the box are the server’s words, plain and without a percentage', () => {
  const rust = readFileSync(join(here, '../../../../crates/core/src/statement_objection.rs'), 'utf8')
  const kept = rust.match(/pub const KEPT_BESIDE: &str = "((?:[^"\\]|\\.|\\\n)*)"/s)
  assert.ok(kept, 'KEPT_BESIDE is declared in statement_objection.rs')
  assert.equal(kept[1].replace(/\\\n\s*/g, ''), KEPT_BESIDE)
  const limit = rust.match(/pub const MAX_REASON_CHARS: usize = (\d+);/)
  assert.equal(Number(limit[1]), OBJECTION_MAX_CHARS)
  for (const text of [KEPT_BESIDE, SENTENCE_CHANGED, ...objectionsOf(SERVED).map((o) => o.lead)]) {
    for (const w of ['lie', 'cheat', 'violate', 'ignore', 'hallucinat', 'trial', 'Not available in this version']) {
      assert.ok(!new RegExp('\\b' + w, 'i').test(text), w + ' in: ' + text)
    }
    assert.ok(!/\d\s?%/.test(text), text)
  }
  assert.match(SENTENCE_CHANGED, /sentence changed since/)
})
