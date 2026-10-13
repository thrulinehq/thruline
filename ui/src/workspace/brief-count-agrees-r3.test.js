/* BRIEF-COUNT-AGREES-R3 (launch-critical) — "set aside" is counted the way its
 * list is, a set-aside review is named as the review, the tell-the-agent panel
 * reads the same tally, and the top says which version "none has evidence" is
 * about.
 *
 * Found by the round-2 adversary (gates/adversary-BRIEF-COUNT-AGREES-R2), on a
 * real `thruline ui`, after `thruline change waive --obligation criterion:1` and
 * `--obligation review` on a Change whose accepted profile requires a review:
 *
 *   the top:                  "1 of 2 things that count as done have evidence.
 *                              1 was set aside, with nobody checking."
 *   the set-aside list under it: TWO entries — the line, and the review
 *
 * on the Change screen, the whole-story page and every export. The round-2
 * sweep's review option served only a review that needed evidence, never one
 * that was set aside, so it never saw it. Three more findings of the same class,
 * older than round 2, are fixed in the same derivation:
 *
 *   - the tell-the-agent panel said "Nothing to ask the agent for: every line
 *     that counts as done is proven on <V>" while a line whose applicability was
 *     never assessed still needed evidence: `tell-draft.js` read the coverage,
 *     not the served tally;
 *   - the review's own entry under "Set aside, with nobody checking:" carried no
 *     words naming it as the review;
 *   - "None has evidence yet" was said over lines the page itself shows as
 *     "Proven on the previous version — needs a re-check".
 *
 * The sweep that holds all of this over every mix of outcomes is
 * `brief-count-agrees-r3-harness.mjs` (its shards are the `-sweep-N` and `-random`
 * files beside this one). This file pins, word for word, the cases the
 * adversary named and the wording this round chose. Where the review is not set
 * aside, every string round 2 pinned is the string it was, and round 2's own
 * file still passes.
 */

import test from 'node:test'
import assert from 'node:assert/strict'
import { M, surfaces, readMoment, shape, COV, checkCase, checkTop, checkTell, checkSetAside, expectOf, WORDS, REVIEW_ENTRY, IDENTITY_WORDS, REVIEW_WORDS, asideFactsOf } from './brief-count-agrees-r3-harness.mjs'

const say = (S) => ({
  top: S.hq.headline.text,
  page: S.page.top[0],
  ask: S.ask.sentences[0],
  briefList: S.brief['brief/manager'].sentence[0],
  setAside: S.brief['brief/manager'].setAside,
  pageSetAside: S.page.setAsideRows,
  tell: S.brief['brief/manager'].tell.join(' '),
})

/* The words that follow the review's name, for the two ways it was set aside. */
const REVIEW_WAIVED = 'Waived by sam, under the release policy: The release policy asks for no review here. Nobody checked this.'
const REVIEW_NOT_APPLICABLE = "Doesn't apply here — The release policy asks for no review here (said by sam, under the release policy). Nobody checked this."
const ASIDE = 'Set aside, with nobody checking:'
/** The name of the run the walk's capture links to every line that has evidence. */
const RUN_NAME = 'bash:$HOME/shop:cargo test'

/* A case is read, and what a person reads is pinned below; then the same case is held to every rule of the sweep
 * (`checkCase`), so a pinned string that is right never hides a surface that disagrees with it. */
const pending = []
const both = async (lines, opts) => {
  const S = await surfaces(lines, opts)
  pending.push(() => checkCase(`[${lines.join(', ')}] + review ${opts.review ?? 'none'}${opts.identity === false ? ' + identity unconfirmed' : ''}`, lines, opts, S))
  return S
}
const named = (name, fn) => test(name, async () => {
  pending.length = 0
  await fn()
  for (const check of pending) check()
})

/* ── the adversary's case: a line and the review, both set aside ───────── */

named('a line and the review both set aside: the top counts the lines and names the review, and the list under it holds the two, on the Change screen', async () => {
  const S = await both(['satisfied', 'waived'], { review: 'waived' })
  const said = say(S)
  assert.equal(said.top, '1 of 2 things that count as done have evidence. 1 of them was set aside, with nobody checking. The review was set aside, with nobody checking.')
  assert.equal(said.setAside.length, 2, 'the list holds a line and the review')
  assert.ok(said.setAside[0].startsWith(WORDS(1) + ' — '), said.setAside[0])
  assert.equal(said.setAside[1], REVIEW_ENTRY + ' — ' + REVIEW_WAIVED, 'the review\'s entry names itself as the review')
  assert.equal(S.brief['brief/manager'].setAsideHeading[0], ASIDE)
  assert.equal(S.hq.readiness.ready, true, 'set aside is not evidence, and it is not a blocker either: this is ready with exceptions')
})

named('the same on the whole-story page: its top is the Change screen\'s, and its list holds the same two', async () => {
  const S = await both(['satisfied', 'waived'], { review: 'waived' })
  const said = say(S)
  assert.equal(said.page, said.top)
  /* Ready with exceptions: the page's list under "It rests on:" ends with what was set aside, each named. */
  assert.deepEqual(S.page.readinessRows.slice(-2).map((r) => r.split(' — ')[0]), [WORDS(1), REVIEW_ENTRY])
  assert.equal(S.page.readinessRows.length, 3, 'one line rests on evidence, two were set aside')
})

named('every export carries the same top and names the review\'s entry (Markdown, HTML, JSON, full and redacted)', async () => {
  const S = await both(['satisfied', 'waived'], { review: 'waived' })
  const top = '1 of 2 things that count as done have evidence. 1 of them was set aside, with nobody checking. The review was set aside, with nobody checking.'
  for (const [name, text] of Object.entries(S.exports)) {
    if (name.startsWith('json')) {
      const j = JSON.parse(text)
      assert.equal(j.headline, top, name)
      assert.ok(j.readiness.rests_on.some((r) => r.startsWith(REVIEW_ENTRY + ' — ' + REVIEW_WAIVED)), name)
    } else {
      assert.ok(text.includes(top), name)
      assert.ok(text.includes(REVIEW_ENTRY + ' — ' + REVIEW_WAIVED), `${name} does not name the review's entry`)
    }
  }
  assert.equal(S.exports['copy as markdown'], S.exports.md)
})

named('the review set aside as "does not apply" is said the same way, in its own words', async () => {
  const S = await both(['satisfied', 'waived'], { review: 'not_applicable' })
  const said = say(S)
  assert.equal(said.top, '1 of 2 things that count as done have evidence. 1 of them was set aside, with nobody checking. The review was set aside, with nobody checking.')
  assert.equal(said.setAside[1], REVIEW_ENTRY + ' — ' + REVIEW_NOT_APPLICABLE)
})

named('Ask says the same when nothing waits for evidence: the lines and the review, each with its own count', async () => {
  const S = await both(['satisfied', 'waived'], { review: 'waived' })
  assert.equal(S.ask.sentences[0], 'Nothing that counts as done is waiting for evidence. 1 of them was set aside, with nobody checking. The review was set aside, with nobody checking.')
})

/* ── every way the lines and the review can both be set aside ──────────── */

named('every line set aside and the review too: "All N of them", then the review, and one line: "That one"', async () => {
  const all = await both(['waived', 'not_applicable', 'waived'], { review: 'waived' })
  assert.equal(say(all).top, 'Accepted: 3 things count as done. All 3 of them were set aside, with nobody checking. The review was set aside, with nobody checking.')
  assert.equal(say(all).setAside.length, 4)
  const one = await both(['waived'], { review: 'not_applicable' })
  assert.equal(say(one).top, 'Accepted: 1 thing counts as done. That one was set aside, with nobody checking. The review was set aside, with nobody checking.')
  assert.equal(say(one).setAside.length, 2)
})

named('only the review set aside, every line with evidence: the top says so, and never reads as all-checked', async () => {
  const two = await both(['satisfied', 'satisfied'], { review: 'waived' })
  assert.equal(say(two).top, '2 of 2 things that count as done have evidence. The review was set aside, with nobody checking.')
  assert.deepEqual(say(two).setAside.map((e) => e.split(' — ')[0]), [REVIEW_ENTRY])
  assert.equal(say(two).ask, 'Nothing that counts as done is waiting for evidence. The review was set aside, with nobody checking.')
  const one = await both(['satisfied'], { review: 'not_applicable' })
  assert.equal(say(one).top, 'The 1 thing that counts as done has evidence. The review was set aside, with nobody checking.')
  const id = await both(['satisfied', 'satisfied'], { review: 'waived', identity: false })
  assert.equal(say(id).top, `2 of 2 things that count as done have evidence. The review was set aside, with nobody checking. ${IDENTITY_WORDS}`)
})

named('the whole-story page over a set-aside review and nothing waiting says nothing waits, not that everything has something behind it', async () => {
  const S = await both(['satisfied', 'satisfied'], { review: 'waived' })
  assert.deepEqual(S.page.needsHeading, ['Nothing here is waiting for evidence'])
  assert.doesNotMatch(S.page.needsSection.join(' '), /has something behind it/)
})

named('a set-aside review beside a line that still needs evidence: the top counts the line, and the review is in the list under its own heading', async () => {
  const S = await both(['satisfied', 'nothing_looked'], { review: 'waived' })
  const said = say(S)
  assert.equal(said.top, '1 of 2 things that count as done have evidence. 1 still needs it.')
  assert.equal(said.briefList, '1 thing still needs evidence:')
  assert.deepEqual(said.setAside.map((e) => e.split(' — ')[0]), [REVIEW_ENTRY])
  assert.deepEqual(S.page.setAsideRows.map((e) => e.split(' — ')[0]), [REVIEW_ENTRY])
  assert.equal(S.hq.readiness.ready, false)
})

named('a review that is missing is still said on its own and never among the set aside', async () => {
  const S = await both(['satisfied', 'waived'], { review: 'needs' })
  assert.equal(say(S).top, `1 of 2 things that count as done have evidence. 1 was set aside, with nobody checking. ${REVIEW_WORDS}`)
  assert.deepEqual(say(S).setAside.map((e) => e.split(' — ')[0]), [WORDS(1)])
})

named('where the review is not set aside, the strings round 2 pinned are the strings they were', async () => {
  for (const review of ['none', 'satisfied']) {
    const S = await both(['satisfied', 'waived'], { review })
    assert.equal(say(S).top, '1 of 2 things that count as done have evidence. 1 was set aside, with nobody checking.', review)
    assert.equal(say(S).ask, 'Nothing that counts as done is waiting for evidence. 1 was set aside, with nobody checking.', review)
    const all = await both(['waived', 'not_applicable', 'waived'], { review })
    assert.equal(say(all).top, 'Accepted: 3 things count as done. All 3 were set aside, with nobody checking.', review)
    const one = await both(['waived'], { review })
    assert.equal(say(one).top, 'Accepted: 1 thing counts as done. It was set aside, with nobody checking.', review)
  }
  const none = await both(['satisfied', 'satisfied'], { review: 'satisfied' })
  assert.equal(say(none).top, '2 of 2 things that count as done have evidence. 0 still need it.')
})

/* ── the odd answers: still one number, still one list ─────────────────── */

named('a review that is satisfied, on an answer that still carries an earlier waiver record, is not set aside', async () => {
  const S = await surfaces(['satisfied', 'satisfied'], { review: 'satisfied' }, (r) => { r.obligations[3].waiver = { actor: 'sam', authority: 'the release policy', rationale: 'Set aside before a review was written down' } })
  assert.equal(say(S).top, '2 of 2 things that count as done have evidence. 0 still need it.')
  assert.deepEqual(say(S).setAside, [])
  assert.equal(S.hq.readiness.ready, true)
  assert.equal(S.hq.readiness.withExceptions, false)
})

named('a waived obligation for a line the Change does not have is set aside and is not one of the lines: said apart, listed once, and named by its own words', async () => {
  const aside = (r, id, n) => ({ ...r.obligations[1], id, criterion_index: n, text: 'A line this Change does not have.', outcome: 'waived', outcome_slug: 'waived', evidence_name: null, waiver: { actor: 'sam', authority: 'the accepted profile', rationale: 'Nobody checked this' } })
  const S = await surfaces(['satisfied', 'satisfied'], {}, (r) => { r.obligations.push(aside(r, 'criterion:9', 9), aside(r, 'criterion:9', 9)) })
  assert.equal(say(S).top, '2 of 2 things that count as done have evidence. Something else this Change needs was set aside, with nobody checking.')
  assert.deepEqual(say(S).setAside.map((e) => e.split(' — ')[0]), ['A line this Change does not have.'], 'the same obligation served twice is listed once')
})

named('an outcome of waived with no person\'s waiver on it is not "set aside by a person": no surface counts it as set aside, and none lists it as set aside', async () => {
  const S = await surfaces(['satisfied', 'waived'], {}, (r) => { r.obligations[2].waiver = null })
  const said = say(S)
  assert.doesNotMatch(said.top, /set aside/)
  assert.deepEqual(said.setAside, [])
  assert.deepEqual(said.pageSetAside, [])
  assert.doesNotMatch(said.ask, /set aside/)
})

named('Ask, asked why approval is not available, lists the review by name among what was set aside', async () => {
  const S = await both(['satisfied', 'waived'], { review: 'waived' })
  const why = M.askAnswer('why_not_approve', S.hq)
  assert.ok(why.sentences.some((s) => s.startsWith(REVIEW_ENTRY + ' — ' + REVIEW_WAIVED)), JSON.stringify(why.sentences))
})

/* ── the tell-the-agent panel reads the served tally ───────────────────── */

named('a line whose applicability was never assessed still needs evidence, so the panel does not say every line is proven, and asks about it', async () => {
  const S = await both(['satisfied', 'never_assessed'], {})
  const said = say(S)
  assert.doesNotMatch(said.tell, /Nothing to ask the agent for/)
  assert.equal(S.tell.kind, 'nothing')
  assert.equal(S.tell.covered, null)
  const version = S.hq.version.short
  assert.ok(S.tell.draft.includes(`"${WORDS(1)}"`), 'the line that needs evidence is asked about')
  /* The run is named, and it is not said to have passed: the record does not yet say whether it still applies. */
  assert.ok(S.tell.draft.includes(`The run that bears on it, ${RUN_NAME}, ran on ${version}, and the record does not yet say whether it still applies, so it does not count yet.`), S.tell.draft)
  assert.ok(S.tell.draft.includes(`Please run ${RUN_NAME} on the current code.`), S.tell.draft)
  assert.doesNotMatch(S.tell.draft, /passed|proven|covered/i)
  assert.ok(!S.tell.draft.includes(WORDS(0)), 'the line that is met is not')
  assert.deepEqual(S.tell.lines.map((l) => l.kind), ['proven', 'nothing'])
  const alone = await both(['never_assessed'], {})
  assert.equal(alone.tell.covered, null)
  assert.ok(alone.tell.draft.includes(`"${WORDS(0)}"`))
})

named('a line the answer says is stale is asked about too: in the words for a run whose files changed since when the record says so, and as a run not yet assessed when it does not; a set-aside line still is not', async () => {
  const version = (S) => S.hq.version.short
  /* The record's own typed field for the run says the files it looked at have changed. */
  const { screen } = await readMoment((reads) => {
    shape(['stale', 'waived', 'satisfied'], {})(reads)
    COV(reads).coverage.criteria[0].items[0].fresh = false
  })
  const tell = M.tellOf(screen.model.hq)
  assert.ok(tell.draft.includes(`"${WORDS(0)}"`))
  assert.ok(tell.draft.includes(`The run that bears on it, ${RUN_NAME}, ran on ${screen.model.hq.version.short}, and what it checked has changed since, so it no longer settles it.`), tell.draft)
  assert.ok(!tell.draft.includes(WORDS(1)) && !tell.draft.includes(WORDS(2)))
  assert.deepEqual(tell.lines.map((l) => l.kind), ['older', 'set_aside', 'proven'])
  /* The record says nothing about whether it still applies: it is not yet assessed, as for a never-assessed line. */
  const S = await both(['stale', 'waived', 'satisfied'], {})
  assert.ok(S.tell.draft.includes(`The run that bears on it, ${RUN_NAME}, ran on ${version(S)}, and the record does not yet say whether it still applies, so it does not count yet.`), S.tell.draft)
  assert.ok(!S.tell.draft.includes(WORDS(1)) && !S.tell.draft.includes(WORDS(2)))
  assert.deepEqual(S.tell.lines.map((l) => l.kind), ['nothing', 'set_aside', 'proven'])
})

named('with nothing waiting the panel says so as it always did, with a set-aside line in it or without', async () => {
  const S = await both(['satisfied', 'satisfied'], {})
  assert.equal(S.tell.kind, 'covered')
  assert.equal(S.tell.covered, `Nothing to ask the agent for: every line that counts as done is proven on ${S.hq.version.short}.`)
  const aside = await both(['satisfied', 'waived'], {})
  assert.equal(aside.tell.covered, `Nothing to ask the agent for: every line that counts as done is proven on ${aside.hq.version.short}, or was set aside by a person.`)
  /* A set-aside review is not a line, and the sentence is about lines. */
  const review = await both(['satisfied', 'satisfied'], { review: 'waived' })
  assert.equal(review.tell.kind, 'covered')
})

named('the panel over lines that need a check is what it was: the same kinds, the same paragraphs', async () => {
  const S = await both(['nothing_looked', 'satisfied'], {})
  assert.equal(S.tell.kind, 'nothing')
  assert.ok(S.tell.draft.includes(`"${WORDS(0)}"`))
  const other = await both(['other_version', 'satisfied'], {})
  assert.equal(other.tell.kind, 'older')
})

/* ── "none has evidence" says which version ───────────────────────────── */

named('lines proven on another version: the top says none has evidence for THIS version, over the rows that say proven on the previous one', async () => {
  const two = await both(['other_version', 'other_version'], {})
  assert.equal(say(two).top, 'Accepted: 2 things count as done. None has evidence for this version yet.')
  assert.equal(say(two).page, say(two).top)
  assert.equal(say(two).briefList, '2 things still need evidence:')
  const one = await both(['other_version'], {})
  assert.equal(say(one).top, 'Accepted: 1 thing counts as done. It has no evidence for this version yet.')
  const mixed = await both(['other_version', 'nothing_looked'], {})
  assert.equal(say(mixed).top, 'Accepted: 2 things count as done. None has evidence for this version yet.')
})

named('a run nobody linked beside a line proven on another version says both, and says which version', async () => {
  const S = await both(['other_version', 'unlinked_run'], {})
  assert.equal(say(S).top, "Accepted: 2 things count as done. None has evidence for this version yet: a test ran for this Change and isn't linked to 1 of them — link it.")
})

named('where nothing was proven on another version, "none has evidence yet" is the sentence it was', async () => {
  const none = await both(['nothing_looked', 'nothing_looked'], {})
  assert.equal(say(none).top, 'Accepted: 2 things count as done. None has evidence yet.')
  const one = await both(['nothing_looked'], {})
  assert.equal(say(one).top, 'Accepted: 1 thing counts as done. It has no evidence yet.')
  const unlinked = await both(['unlinked_run', 'nothing_looked'], {})
  assert.equal(say(unlinked).top, "Accepted: 2 things count as done. None has evidence yet: a test ran for this Change and isn't linked to 1 of them — link it.")
  const settled = await both(['settled_nothing', 'settled_nothing'], {})
  assert.equal(say(settled).top, 'Accepted: 2 things count as done. None has evidence yet.')
})

named('a line with evidence beside one proven on another version is counted as it was: one of two, and one still needs it', async () => {
  const S = await both(['satisfied', 'other_version'], {})
  assert.equal(say(S).top, '1 of 2 things that count as done have evidence. 1 still needs it.')
})

/* ── the harness is a live instrument ─────────────────────────────────── */

named('the harness catches a count of lines said over a list that holds more', () => {
  const X = expectOf(['satisfied', 'waived'], { review: 'waived' })
  assert.equal(X.entries, 2)
  assert.throws(() => checkTop('bad', '1 of 2 things that count as done have evidence. 1 was set aside, with nobody checking.', X), /says 1 were set aside, and the set-aside list holds 2/)
  assert.throws(() => checkTop('bad', '1 of 2 things that count as done have evidence. 1 of them was set aside, with nobody checking.', X), /does not account for them/)
  checkTop('ok', '1 of 2 things that count as done have evidence. 1 of them was set aside, with nobody checking. The review was set aside, with nobody checking.', X)
  checkTop('ok', '1 of 2 things that count as done have evidence. 2 were set aside, with nobody checking.', X)
  assert.throws(() => checkTop('bad', '1 of 2 things that count as done have evidence. 1 of them was set aside, with nobody checking. The review was set aside, with nobody checking.', expectOf(['satisfied', 'waived'], { review: 'none' })), /the review was set aside, and it was not/)
})

named('the harness catches "every line is proven" over a line that still needs evidence, and "none has evidence" over another version', () => {
  const X = expectOf(['satisfied', 'never_assessed'], {})
  const covered = { kind: 'covered', draft: null, covered: 'Nothing to ask the agent for: every line that counts as done is proven on e71c5b0.', lines: [] }
  assert.throws(() => checkTell('bad', X, ['A message for the agent Nothing to ask the agent for: every line that counts as done is proven on e71c5b0.'], covered), /nothing is to be asked while 1 line/)
  const prev = expectOf(['other_version', 'other_version'], {})
  assert.throws(() => checkTop('bad', 'Accepted: 2 things count as done. None has evidence yet.', prev), /does not say it is about this version/)
  checkTop('ok', 'Accepted: 2 things count as done. None has evidence for this version yet.', prev)
})

named('the sentences the harness reads a set-aside claim out of are the three families and nothing else', () => {
  assert.deepEqual(asideFactsOf('1 was set aside, with nobody checking.'), { bare: [1], scoped: [], review: false })
  assert.deepEqual(asideFactsOf('All 3 were set aside, with nobody checking.'), { bare: [3], scoped: [], review: false })
  assert.deepEqual(asideFactsOf('Accepted: 1 thing counts as done. It was set aside, with nobody checking.'), { bare: [1], scoped: [], review: false })
  assert.deepEqual(asideFactsOf('2 of them were set aside, with nobody checking. The review was set aside, with nobody checking.'), { bare: [], scoped: [2], review: true })
  assert.deepEqual(asideFactsOf('All 3 of them were set aside, with nobody checking.'), { bare: [], scoped: [3], review: false })
  assert.deepEqual(asideFactsOf('That one was set aside, with nobody checking.'), { bare: [], scoped: [1], review: false })
  assert.equal(M.HQ_COPY.change.model.readiness.set_aside_heading, ASIDE)
  void checkSetAside
})
