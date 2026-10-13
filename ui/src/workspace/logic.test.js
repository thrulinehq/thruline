import test from 'node:test'
import assert from 'node:assert/strict'
import { C } from './tokens.js'
import {
  FACETS,
  GLOSS,
  NEGATIVE_WORDS,
  WORD,
  WORDKEY,
  WORST_FIRST,
  estH,
  normalizeWord,
  projectForSessionLine,
  sessionWordFrom,
  severityOf,
  short,
} from '../workspace/logic.js'

/* The pure half of the workspace, pinned. Tests verify intent, not
 * implementation trivia (Rule 9): every assertion below is a doctrine rule that
 * the design got wrong, so a regression here is a doctrine regression.
 *
 * No list to join: since R-2026-09-23-14 `ui/package.json`'s `test` script is
 * a glob over every `.test.js` under `src/`, so a new suite there is run the
 * moment it is saved, and the out-of-tree suites are named after it.
 */

test('the seven words are exactly the seven words', () => {
  assert.deepEqual(Object.keys(WORD).sort(), ['broken', 'couldnt', 'fog', 'kept', 'loose', 'not_proven', 'proven'].sort())
})

test('the third word is Recorded, and no state word carries an apostrophe at all', () => {
  // R8-VOCABULARY: the `couldnt` slug used to carry a two-word phrase with an
  // apostrophe in it, and the hazard was U+2019 vs U+0027 (POS:57, BRIEF:45) — a rendered word that does not
  // match a narration prefix byte for byte renders WORDLESS, a silent
  // doctrine failure. The design record §5 word has no apostrophe, so the
  // hazard is removed rather than managed; this asserts it cannot come back.
  assert.equal(WORD.couldnt[0], 'Recorded')
  for (const [slug, w] of Object.entries(WORD)) {
    assert.ok(!/['\u2019]/.test(w[0]), `${slug} word carries an apostrophe: ${w[0]}`)
  }
})

test('Kept is □ and Broken is ⊠', () => {
  // BRIEF:399. The design's ✕ also collides with the four × close buttons in
  // this same UI.
  assert.equal(WORD.kept[2], '□')
  assert.equal(WORD.broken[2], '⊠')
})

test('normalizeWord folds U+2019 to U+0027 and touches nothing else', () => {
  assert.equal(normalizeWord('the agent’s run'), "the agent's run")
  assert.equal(normalizeWord('the decision is the reader’s'), "the decision is the reader's")
})

test('worst-first ordering exists and is the ruled order', () => {
  // Absent from the design entirely (spec-doctrine §B2).
  assert.deepEqual([...WORST_FIRST], ['broken', 'fog', 'not_proven', 'couldnt', 'loose', 'kept', 'proven'])
})

test('every negative word has a gloss, and no gloss is Not proven wearing another name', () => {
  for (const slug of NEGATIVE_WORDS) assert.ok(GLOSS[slug], `no gloss for ${slug}`)
  assert.notEqual(GLOSS.couldnt, GLOSS.not_proven)
  assert.notEqual(GLOSS.fog, GLOSS.not_proven)
})

test('there are sixteen facets and every word slug is a legal word: value', () => {
  assert.equal(FACETS.length, 16)
  for (const slug of Object.keys(WORD)) assert.ok(WORDKEY[slug], `no query slug for ${slug}`)
})

test('projectForSessionLine turns Broken into Not proven and drops Kept', () => {
  // RSV:25-35. Kept and Broken are constraints; a session line that inherits
  // either has put a constraint word on a claim surface, which is the exact
  // leak the amendment was written to stop.
  assert.deepEqual(projectForSessionLine(['broken', 'kept', 'proven']), ['not_proven', 'proven'])
  assert.deepEqual(projectForSessionLine(['kept']), [])
})

test('sessionWordFrom: no items → none, clean → Proven, mixed → the worst', () => {
  assert.equal(sessionWordFrom([]), null)
  assert.equal(sessionWordFrom(['kept']), null)
  assert.equal(sessionWordFrom(['proven', 'proven']), 'proven')
  assert.equal(sessionWordFrom(['proven', 'loose']), 'loose')
  assert.equal(sessionWordFrom(['loose', 'fog', 'proven']), 'fog')
  // Broken is projected before ranking, so it never reaches the session line.
  assert.equal(sessionWordFrom(['broken', 'loose']), 'not_proven')
})

test('short keeps the algorithm prefix and elides only the body', () => {
  assert.equal(short('sha256:afb960f9eb04728d'), 'sha256:afb960f9…')
  assert.equal(short('c324df66'), 'c324df66')
  assert.equal(short(null), '')
})

test('estH grows with pins, a wrapping title and body lines', () => {
  const bare = { title: 'x', lines: [], pinsL: [], pinsR: [] }
  const pinned = { title: 'x', lines: [], pinsL: [{ id: 'i' }, { id: 'o' }], pinsR: [] }
  const bodied = { title: 'x', lines: ['one line'], pinsL: [], pinsR: [] }
  assert.ok(estH(pinned, 210) > estH(bare, 210))
  assert.ok(estH(bodied, 210) > estH(bare, 210))
  // The same node is taller in a narrower card, because the title wraps.
  const long = { title: 'a'.repeat(200), lines: [], pinsL: [], pinsR: [] }
  assert.ok(estH(long, 120) > estH(long, 260))
})

test('severityOf returns a significance and never a state word', () => {
  const sev = severityOf('fail')
  assert.equal(sev.label, 'why it is not proven')
  assert.equal(sev.key, 'why-not-proven')
  for (const slug of Object.keys(WORD)) assert.notEqual(sev.label, WORD[slug][0])
  // The three severity glyphs are BRIEF:399's, and none of them is a word glyph.
  const glyphs = ['fail', 'warn', 'info'].map((s) => severityOf(s).glyph)
  assert.deepEqual(glyphs, ['▲', '●', '○'])
  assert.deepEqual(severityOf('nope'), { label: '', color: C.inkMute, glyph: '', key: '' })
})
