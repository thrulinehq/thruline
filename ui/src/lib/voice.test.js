/* §5v's teeth, UI side. The Rust suite walks the engine's first-level strings;
 * this walks the shell's — every chip label, every badge word, every receipt
 * verdict line, every readiness pair, every proof-history title.
 *
 * It fails loud and names the offending string, because a doctrine test that
 * says only "assertion failed" sends the next person hunting. */

import assert from 'node:assert/strict'
import test from 'node:test'

import { RECEIPT_STATE, SLABEL, STATES, verdictLine } from './design.js'
import {
  ABSTENTION,
  CURRENCY,
  DISCLOSURE,
  ENUMS,
  PROOF_CURRENTNESS,
  READINESS,
  VERDICT,
  bannedIn,
  chipLabel,
  composition,
  countPhrase,
  headline,
  sentence,
  word,
} from './voice.js'

/** Every first-level string this shell can put on screen, with its origin. */
function firstLevelStrings() {
  const out = []
  const add = (where, text) => out.push([where, text])

  for (const s of STATES) {
  add(`SLABEL.${s}`, SLABEL[s])
  add(`RECEIPT_STATE.${s}`, RECEIPT_STATE[s])
  add(`verdictLine(${s})`, verdictLine(s))
  add(`word(${s})`, word(s))
  add(`sentence(${s})`, sentence(s))
  add(`chipLabel(${s})`, chipLabel(word(s)))
  }
  for (const [slug, t] of Object.entries(VERDICT)) {
  add(`VERDICT.${slug}.word`, t.word)
  add(`VERDICT.${slug}.sentence`, t.sentence)
  add(`VERDICT.${slug} chip`, chipLabel(t.word))
  }
  for (const [slug, t] of Object.entries({ ...CURRENCY, ...PROOF_CURRENTNESS, ...ABSTENTION, ...DISCLOSURE })) {
  add(`${slug}.word`, t.word)
  add(`${slug}.sentence`, t.sentence)
  add(`${slug} chip`, chipLabel(t.word))
  }
  for (const [slug, r] of Object.entries(READINESS)) {
  add(`READINESS.${slug}.title`, r.title)
  add(`READINESS.${slug}.detail`, r.detail)
  }
  for (const n of [0, 1, 2, 17]) {
  add(`headline(${n})`, headline('thruline', n))
  add(`countPhrase(${n})`, countPhrase(n, 'thing worth a look', 'things worth a look'))
  }
  add('composition', composition(STATES.map((s, i) => [i, word(s)])))
  return out
}

test('the one voice: lets no engine lexicon reach a first-level string', () => {
  const offenders = firstLevelStrings()
    .map(([where, text]) => [where, text, bannedIn(text)])
    .filter(([, , hit]) => hit !== null)
    .map(([where, text, hit]) => `${where}: ${hit} in ${JSON.stringify(text)}`)
  assert.deepEqual(offenders, [])
})

test('the one voice: walks a real number of strings, not a handful', () => {
  assert.ok((firstLevelStrings().length) > 60)
})

test('the one voice: catches the leaks that actually shipped', () => {
  // These are the pre-M6.R1 strings from design.js and App.jsx. If the ban
  // stops catching them, the retrofit has been quietly undone.
  assert.equal(bannedIn('Unsupported'), 'unsupported')
  assert.equal(bannedIn('Refused 0'), 'refused')
  assert.equal(bannedIn('Unsupported · 6'), 'unsupported')
  assert.equal(bannedIn('deterministic · local'), 'deterministic · local')
  assert.equal(bannedIn('claude-opus · fidelity high'), 'fidelity')
  // C0.2, evolved in the open: "Supported" and "Contradicted" are now the
  // doctrine-adopted rendered words — the old leak shape is the vocabulary.
  assert.equal(bannedIn('Contradicted'), null)
  assert.equal(bannedIn('Contradicted: 3'), null)
  assert.equal(bannedIn('3 Contradicted claims'), null)
  // C0.2's dead vocabulary and confidence theater, banned as jargon.
  assert.equal(bannedIn('chain UNVERIFIED'), 'unverified')
  assert.equal(bannedIn('94% confidence'), 'confidence')
})

test('the one voice: leaves ordinary English alone', () => {
  // The rule exists to produce better English, so it must not forbid it.
  assert.equal(bannedIn('Installation not verified'), null)
  // Worth being explicit about: the OLD receipt line is not a lexicon
  // violation. It was replaced because doctrine specifies a sentence for this
  // state ("Backed by the record."), and because paper cut #4 says the receipt
  // must not show engine reasoning at first level — not because the ban caught
  // it. Two different rules; conflating them would inflate what this test
  // claims to prove.
  assert.equal(bannedIn('Supported by evidence'), null)
  assert.equal(bannedIn('Supported Codex commands can create proof.'), null)
  assert.equal(bannedIn('Backed by the record.'), null)
  assert.equal(bannedIn('Existing signatures remain recorded; new assurance is paused.'), null)
})

test('the one voice: renders no naked numbers and no zero-count elements', () => {
  assert.equal(composition([[3, 'supported'], [0, 'not evaluated']]), '3 supported')
  assert.equal(composition([[0, 'supported']]), '')
  assert.equal(countPhrase(1, 'thing', 'things'), '1 thing')
  assert.equal(headline('thruline', 0), 'thruline — everything checks out.')
  assert.ok((headline('thruline', 2)).includes('2 things worth a look'))
})

test('the one voice: produces the doctrine badge example from the table, not a literal', () => {
  // C0.2 re-cut the words; the example still falls out of the table.
  assert.equal(composition([[3, word('supported')], [1, word('unsupported')]]), '3 supported · 1 no proof found')
})

test('the one voice: keeps every engine enum out of every label', () => {
  for (const s of STATES) {
    assert.ok(!(ENUMS).includes(SLABEL[s].toLowerCase()))
  }
})
