/* NOT-APPROVAL-WORDS: the top sentence counts evidence, and `approveBlocker` reaches a surface only when there is no derived model at all (RUN-LINK-TRUTH-R6); with a served readiness answer every surface says `readiness.blocker` (not ready) or `readiness.available` (ready) instead (RUN-LINK-TRUTH-R5/R6 census). */
/* The Change workspace — the deterministic model behind the screen (T-2-4).
 *
 * PURE. No React, no fetch, no clock, no locale, no randomness, no model. One
 * set of functions over the documents `/api/changes/<id>` (T-2-1) and
 * `/api/changes/<id>/proposals` (T-2-3) already serve, plus the sessions the
 * existing adapter already builds. It decides; `change-workspace.jsx` only
 * lays out what it decided, and every word it lays out comes from `HQ_COPY`.
 *
 * WHAT THIS FILE IS FOR. Sam's first question — "what am I being asked to
 * accept?" — has to be answerable from the top of the screen in ten seconds,
 * out loud, without opening anything. That is one title, one sentence, one
 * named version. `conclusionOf` is that sentence and nothing else is allowed
 * to be it.
 *
 * THE RULES IT HOLDS (03-USER-LENS.md §3, `B7.*`):
 *
 *  - The five state words describe ONE piece of evidence. They are never the
 *    Change-level answer, so `conclusionOf` cannot return one: it returns a
 *    copy key into `HQ_COPY.change.conclusion`, and that section is swept for
 *    state words and percentages by hq-copy.test.js.
 *  - Missing evidence is missing. A criterion with nothing behind it is
 *    counted as needing evidence — never as satisfied, never as failed
 *    (`B7.2`, `B7.15`).
 *  - The dimensions stay apart (`B7.4`). What counts as done, what the record
 *    holds about the linked work, whether a version is bound, and whether a
 *    person has decided are four separate readings here and four separate
 *    regions on the screen. None of them is composed into one badge, and
 *    nothing is green.
 *  - Only confirmed work counts (`B3.12`, T-2-2). A suggestion is a
 *    suggestion: it contributes no finding card and no node, and the screen
 *    says how many were left out rather than hiding them.
 *
 * WHERE `evidence` COMES FROM (T-3-1). It is the typed entry list
 * `coverage-view.js` builds out of `GET /api/changes/<id>/coverage`, and that
 * document is the engine's one documented rule speaking. Nothing in this file
 * matches a criterion to a claim by its words: that was true when the link did
 * not exist and it is still true now that it does, because doing it twice is
 * how a product grows two answers to one question.
 */

import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'

const C = HQ_COPY.change

/** The six stages, in the brief's order (`B4.1`). Never reordered, never cut. */
export const STAGES = Object.freeze(['define', 'prepare', 'build', 'verify', 'review', 'approve'])

const list = (v) => (Array.isArray(v) ? v : [])
const str = (v) => (typeof v === 'string' && v.trim() ? v.trim() : null)

/* ── who this Change is about ──────────────────────────────────────────── */

/**
 * The standing link for every session this Change names, mirroring
 * `change_link::standing_link` (the latest row for the pair wins). The served
 * array is ordered `asserted_at, id`, so the last row for a session is that
 * session's standing answer.
 *
 * @param {object} change the `change` object of `/api/changes/<id>`
 * @returns {{confirmed: string[], suggested: string[], rejected: string[]}}
 *   each sorted, so the same record always renders the same screen
 */
export function scopeOfWork(change) {
  const standing = new Map()
  for (const a of list(change && change.associations)) {
    const id = str(a && a.session_id)
    if (!id) continue
    standing.set(id, str(a.provenance) || 'suggested')
  }
  const out = { confirmed: [], suggested: [], rejected: [] }
  for (const [id, provenance] of standing) {
    if (provenance === 'confirmed') out.confirmed.push(id)
    else if (provenance === 'rejected') out.rejected.push(id)
    else out.suggested.push(id)
  }
  out.confirmed.sort()
  out.suggested.sort()
  out.rejected.sort()
  return out
}

/* ── what counts as done ───────────────────────────────────────────────── */

/**
 * What has been accepted as counting as done, from the Change record itself.
 *
 * @param {object} change
 * @returns {{version: number|null, criteria: string[], checkProfile: string[],
 *   acceptedBy: string|null, acceptedAt: string|null, digest: string|null}}
 */
export function acceptedScope(change) {
  const s = change && change.accepted_scope
  if (!s) return { version: null, criteria: [], allowedScope: [], checkProfile: [], acceptedBy: null, acceptedAt: null, digest: null }
  const a = s.acceptance || null
  return {
    version: typeof s.version === 'number' ? s.version : null,
    criteria: list(s.criteria).map(String),
    /* What this Change covers (T-3-2b). A typed field of the accepted scope,
     * not a reading of anything: empty here means the scope check refuses
     * every piece of work under this Change, and the screen says so. */
    allowedScope: list(s.allowed_scope).map(String),
    checkProfile: list(s.check_profile).map(String),
    acceptedBy: a ? str(a.actor) : null,
    acceptedAt: a ? str(a.accepted_at) : null,
    digest: str(s.content_sha256),
  }
}

/**
 * QUAL-F2-R2 (`Q7`) — whether the standing list of what counts as done was
 * accepted after the work it covers was already in the record.
 *
 * The engine decides it (`change.retrospective`, `RetrospectiveAcceptance` in
 * crates/core/src/change.rs); this only reads it. `null` when the field is
 * `null`, absent (a server older than it), not an object, its date is not
 * the `YYYY-MM-DD` the engine serves as `accepted_on` — the UTC date of the
 * acceptance — or it is about another version of the list than the one this
 * Change is showing. The sentence it drives says "this list" and needs its
 * date, so an undated "after", or one about a list that is not on the page, is
 * not said at all; and the date is never re-derived here from the spelling of
 * a stamp, which is another zone's date for a stamp that carries an offset.
 *
 * @param {object} change
 * @returns {{scopeVersion: number, acceptedOn: string, workOn: string|null}|null}
 */
export function retrospectiveOf(change) {
  const r = change && change.retrospective
  if (!r || typeof r !== 'object' || Array.isArray(r)) return null
  const acceptedOn = typeof r.accepted_on === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(r.accepted_on) ? r.accepted_on : null
  if (!acceptedOn) return null
  const shown = change.accepted_scope && typeof change.accepted_scope.version === 'number' ? change.accepted_scope.version : null
  if (shown === null || r.scope_version !== shown) return null
  return {
    scopeVersion: r.scope_version,
    acceptedOn,
    workOn: dayOf(r.work_recorded_at),
  }
}

/* ── the one sentence at the top ───────────────────────────────────────── */

/**
 * The current conclusion, as a copy key and the numbers that fill it.
 *
 * The rule, over what exists and nothing else:
 *
 *   nothing accepted                          → `no_criteria`
 *   accepted, and the record holds a problem  → `problem`
 *   accepted, nothing has evidence            → `no_evidence`
 *   accepted, some have evidence              → `some_evidence`
 *
 * A problem outranks a count of evidence because a person who is told "2 of 3
 * have evidence" and not told that the third contradicts what was said has
 * been told the smaller truth first. It is never a state word and never a
 * percentage; the sweep in hq-copy.test.js is what keeps that true of the
 * words, and the `key` return is what keeps it true of this file.
 *
 * @param {{criteria: string[], evidence: Array<{criterion: number, state: string}>}} record
 *   `evidence` is one entry per criterion that has any, with the observed
 *   result for it. A criterion absent from the list has no evidence; that is
 *   the whole meaning of its absence, and it is never read as a pass.
 * @returns {{key: string, values: object, total: number, withEvidence: number,
 *   withoutEvidence: number, problems: number}}
 */
export function conclusionOf(record) {
  const criteria = list(record && record.criteria)
  const total = criteria.length
  const evidence = list(record && record.evidence)
  const problems = new Set()
  const covered = new Set()
  for (const e of evidence) {
    const n = e && typeof e.criterion === 'number' ? e.criterion : null
    if (n == null || n < 1 || n > total) continue
    /* T-3-1 / `Q3`. An entry that says it settles nothing does not move this
     * count. A test command that exited 0 is a command that exited 0, and the
     * record holds no count of what it ran; treating that as "this line has
     * evidence" is the accidental satisfaction the card exists to stop. The
     * item is still SHOWN under its line, by `coverage-view.js` — it just
     * cannot make the top sentence say a bigger number. `bears` is a typed
     * field of the served document; absent means the caller is an older
     * reading, and an absent flag is not a denial. */
    if (e.bears === false) continue
    covered.add(n)
    /* Every observed result that is short of Proven leaves the criterion
     * needing evidence; only `not_proven` — the record holding the opposite —
     * is a problem the top sentence names. "Recorded" is not a failure
     * and is never rendered as one (`B7.2`). */
    if (e.state === 'not_proven') problems.add(n)
  }
  for (const n of problems) covered.delete(n)
  const withEvidence = covered.size
  const withoutEvidence = total - withEvidence - problems.size

  if (total === 0) return { key: 'no_criteria', values: {}, total, withEvidence: 0, withoutEvidence: 0, problems: 0 }
  if (problems.size) {
    return {
      key: problems.size === 1 ? 'problem_one' : 'problem',
      values: { total, problems: problems.size },
      total,
      withEvidence,
      withoutEvidence,
      problems: problems.size,
    }
  }
  /* RUN-LINK-TRUTH: nothing has evidence, and the engine names lines whose
   * passing run nobody linked. The sentence says the run is there and is not
   * linked, rather than that nothing has been done; the counts are unchanged. */
  const unlinked = typeof (record && record.unlinked) === 'number' ? record.unlinked : 0
  if (withEvidence === 0 && unlinked > 0) {
    return {
      key: total === 1 ? 'unlinked_one' : 'unlinked',
      values: { total, n: unlinked },
      total,
      withEvidence: 0,
      withoutEvidence: total,
      problems: 0,
      evidenceLines: [],
    }
  }
  if (withEvidence === 0) {
    return {
      key: total === 1 ? 'no_evidence_one' : 'no_evidence',
      values: { total },
      total,
      withEvidence: 0,
      withoutEvidence: total,
      problems: 0,
      evidenceLines: [],
    }
  }
  return {
    key: withoutEvidence === 1 ? 'some_evidence_one' : 'some_evidence',
    values: { total, with_evidence: withEvidence, without_evidence: withoutEvidence },
    total,
    withEvidence,
    withoutEvidence,
    problems: 0,
    /* BRIEF-COUNT-AGREES — WHICH lines the count above is a count of, so a
     * caller holding the served readiness can tell whether it is a count of
     * the same lines and not only of the same size. Read, never displayed. */
    evidenceLines: [...covered].sort((a, b) => a - b),
  }
}

/**
 * NEVER-ASSESSED-ON-SCREEN, BRIEF-COUNT-AGREES, BRIEF-COUNT-AGREES-R2 — the top
 * sentence, when the readiness answer disagrees with it.
 *
 * "N of M things that count as done have evidence. K still need it." is built
 * from the coverage: does any evidence bear on the line. The list under it, the
 * whole-story page's own list, Ask and the Inbox's items are built from the
 * engine's readiness answer (`readinessOf`, T-3-5): what stands between this
 * version and approval, where a line whose evidence was never assessed is still
 * a line that needs it (R-2026-09-28-25) and a line a named person set aside is
 * not. Two derivations of one noun phrase gave the walk's screen "1 still needs
 * it" above "2 things still need evidence:", "0 still need it" above "1 thing
 * still needs evidence", and a set-aside line counted as one that "still needs
 * it" beside "Ready for approval with exceptions".
 *
 * There is ONE definition of what still needs evidence, and it is the answer's
 * (`readiness.tally`): an accepted line the answer says needs it. A set-aside
 * line is never one. The identity, or a review, that is missing is not a line at
 * all: it is said on its own, in words of its own, and never counted among "the
 * things that count as done".
 *
 * The top keeps the coverage's own sentence only where that sentence is a count
 * of the very lines the answer names — the lines with evidence are the lines the
 * answer says are met, nothing was set aside, and nothing beyond the lines is
 * missing. Anywhere else the top says what the answer says, in its own clause
 * and with its own count, and claims nothing about a line the answer does not
 * call met:
 *
 *   some line needs evidence       "Accepted: 3 things count as done. 2 things still need evidence."
 *   nothing needs it, some set aside
 *                                  "1 of 2 things that count as done have evidence. 1 was set aside, with nobody checking."
 *   something beyond the lines     the same, then what is missing, on its own
 *
 * BRIEF-COUNT-AGREES-R3 — "set aside" is counted the way the set-aside LIST is,
 * which holds every entry a named person's waiver is on: the lines, and a review
 * (`readiness.tally.setAside` and `.setAsideApart`). A bare "1 was set aside"
 * over that list of two was the round-2 adversary's break. With only lines set
 * aside the sentences are the ones above, byte for byte. With a review (or
 * anything else that is not a line) set aside too, the count is said of the
 * lines that count as done ("1 of them was set aside"), and the review is named
 * apart, in a sentence of its own ("The review was set aside, with nobody
 * checking."), so each number has its own noun. A review set aside beside lines
 * that all have evidence is said too: the top never reads as all-checked while
 * the list under it holds something nobody checked.
 *
 * And "none has evidence yet" is a count of the lines with evidence ON THIS
 * VERSION. With a line the answer says was proven on another version (its reason
 * slug is `readiness_other_version`) the page shows that line as "Proven on the
 * previous version — needs a re-check", so the top says it is about this version
 * and never that nothing was checked. (A line stale on THIS version has its
 * evidence counted here, so the top never says none has any.)
 *
 * It reads the answer and derives no count of its own; the coverage counts on
 * the conclusion (`withEvidence`, `withoutEvidence`, `problems`) are left
 * exactly as they were.
 *
 * Every other case is returned untouched, byte for byte: no answer served, an
 * answer that does not name the same lines the record accepts, a problem,
 * nothing accepted, or a sentence that is already the answer's own numbers.
 *
 * @param {object} conclusion what `conclusionOf` decided
 * @param {object|null} readiness what `readinessOf` returned
 * @returns {object}
 */
export function readinessConclusion(conclusion, readiness) {
  const out = readinessConclusionOf(conclusion, readiness)
  const here = out && NONE_HERE[out.key]
  const t = here && readiness && !readiness.absent ? readiness.tally : null
  return t && list(t.needing).some((m) => PROVEN_ELSEWHERE.has(m.slug)) ? { ...out, key: here } : out
}

/**
 * BRIEF-COUNT-AGREES-R3 — the top's three sentences that say none of the lines
 * has evidence, and the same sentences said of THIS version. Used only when the
 * answer names a line whose evidence was about another one.
 */
const NONE_HERE = Object.freeze({ no_evidence: 'no_evidence_here', no_evidence_one: 'no_evidence_here_one', unlinked: 'unlinked_here' })
/** The engine's reason that says a line's evidence is about a version other than this one (`change_readiness::why_not`, `OtherVersion`). */
const PROVEN_ELSEWHERE = Object.freeze(new Set(['readiness_other_version']))

/**
 * BRIEF-COUNT-AGREES-R3 — what a person is told was set aside, from the served
 * tally, as one string: the lines' count, then each thing that is not a line, in
 * the answer's order. With only lines set aside it is the sentence the top and
 * Ask have always said ("1 was set aside, with nobody checking."). With a review
 * (or anything else that is not a line) in the same list, the lines' count says
 * what it counts ("1 of them") and the review is named apart, so no bare number
 * is said over a list that holds a different number.
 *
 * @param {{setAside: object[], setAsideApart?: object[]}} tally `readiness.tally`
 * @param {boolean} [all] the top's "All N were set aside" form, said when no line has evidence
 * @returns {string}
 */
export function setAsideSaid(tally, all = false) {
  const K = C.conclusion
  const lines = list(tally && tally.setAside).length
  const apart = list(tally && tally.setAsideApart)
  const named = apart.length > 0
  const words = []
  if (lines) {
    words.push(all
      ? (lines === 1 ? K[named ? 'set_aside_of_all_one' : 'set_aside_all_one'] : fill(K[named ? 'set_aside_of_all' : 'set_aside_all'], { n: lines }))
      : (lines === 1 ? K[named ? 'set_aside_of_one' : 'set_aside_one'] : fill(K[named ? 'set_aside_of_many' : 'set_aside_many'], { n: lines })))
  }
  for (const a of apart) words.push(a.words)
  return words.filter(Boolean).join(' ')
}

function readinessConclusionOf(conclusion, readiness) {
  const c = conclusion
  if (!c || !readiness || readiness.absent || c.problems) return c
  const t = readiness.tally
  if (!t || !c.total) return c
  const need = t.needing.length
  const aside = t.setAside.length
  const apart = list(t.setAsideApart).length
  const beyond = t.beyond.length
  const met = t.met.map((m) => m.n)
  /* The top says a review a person set aside only when no line waits for evidence: with one waiting the sentence is the
   * count of the lines, and a review is not a line, so it moves none of those numbers. */
  if (!aside && !(apart && !need) && !beyond && need + met.length === c.total && namesTheSameLines(c, met)) return c
  /* An answer that names no line this record accepts says nothing to count. */
  if (!need && !aside && !beyond && need + met.length !== c.total) return c

  const K = C.conclusion
  /* What is missing that is not a line, said on its own (`tally.beyond[].words`). */
  const beyondWords = t.beyond.map((b) => b.words).filter(Boolean).join(' ')
  const then = (words) => [words, beyondWords].filter(Boolean).join(' ')
  if (need > 0) {
    const S = C.model.readiness.states
    const count = fill(S[need === 1 ? 'readiness_needs_evidence_one' : 'readiness_needs_evidence'], { n: need }).replace(/:$/, '.')
    return { ...c, key: c.total === 1 ? 'accepted_needs_one' : 'accepted_needs', values: { total: c.total, readiness: then(count) } }
  }
  if (aside && !met.length) {
    return { ...c, key: c.total === 1 ? 'accepted_needs_one' : 'accepted_needs', values: { total: c.total, readiness: then(setAsideSaid(t, true)) } }
  }
  if (aside) {
    return { ...c, key: 'evidence_then', values: { with_evidence: met.length, total: c.total, readiness: then(setAsideSaid(t)) } }
  }
  if (apart || beyond) {
    /* Nothing waits for evidence, no line is set aside, and something is: a review a person set aside, or something that
     * is missing and is not a line. Said after the count of the lines that have evidence, each on its own. */
    const rest = then(setAsideSaid(t))
    return c.total === 1
      ? { ...c, key: 'evidence_then_one', values: { readiness: rest } }
      : { ...c, key: 'evidence_then', values: { with_evidence: met.length, total: c.total, readiness: rest } }
  }
  return { ...c, key: 'some_evidence', values: { total: c.total, with_evidence: met.length, without_evidence: 0 } }
}

/**
 * Is the coverage's count of lines with evidence a count of exactly the lines
 * the readiness answer says are met? With the coverage's line list absent (a
 * caller that built the conclusion by hand) the sizes alone are compared.
 */
function namesTheSameLines(c, met) {
  if (!Array.isArray(c.evidenceLines)) return c.withEvidence === met.length
  const have = new Set(c.evidenceLines)
  return have.size === met.length && met.every((n) => have.has(n))
}

/**
 * Has anything at all been checked against what counts as done?
 *
 * One reading of one typed field: the number of accepted criteria that have any
 * evidence behind them. Not a judgement, not a state word — the same count the
 * top sentence is built from, asked a second question.
 *
 * @param {{conclusion?: {withEvidence?: number}}} model
 * @returns {boolean} true when NO criterion has any evidence
 */
export function nothingCheckedYet(model) {
  const c = model && model.conclusion
  return !c || !c.withEvidence
}

/**
 * STEP-7-LIMITS-R2 — the one number for "the work you linked" on a page: the
 * pieces of work actually READ (`model.work`, the conversations). A confirmed
 * check-execution session is never read as work, so it is never in it. Every
 * sentence that counts linked work — Verify's "From N pieces of linked work",
 * the nothing-wrong line below it, the page's footer — counts this set.
 *
 * @param {{work?: object[]}} model
 * @returns {number}
 */
export function workReadCount(model) {
  return Array.isArray(model && model.work) ? model.work.length : 0
}

/**
 * The one absence-of-problem sentence about the linked work, with its scope in
 * it (T-2-8-F5), for the Change screen's Verify view and the page alike.
 *
 * PW-F13: it counts the pieces of work actually READ (`model.work`), which is
 * what the page's footer counts. It used to count `scope.confirmed`, which
 * also holds the check-execution sessions a run links as Confirmed — those
 * are never read as work, so one page said "2 pieces" above a footer that
 * said "1 piece". With nothing read at all it says so, never "0 pieces".
 *
 * @param {{work?: object[], conclusion?: object}} model
 * @returns {string}
 */
export function nothingWrongLine(model) {
  const V = HQ_COPY.change.verify
  const n = workReadCount(model)
  if (!n) return V.nothing_read
  if (nothingCheckedYet(model)) return n === 1 ? V.nothing_wrong_unchecked_one : V.nothing_wrong_unchecked_many.replace('{n}', String(n))
  return n === 1 ? V.nothing_wrong_one : V.nothing_wrong_many.replace('{n}', String(n))
}

/**
 * THE RULE (`T-2-8-F5`).
 *
 * No surface may print an absence-of-problem sentence for a Change whose
 * criteria have no evidence without naming that, in the same sentence.
 *
 * "Nothing in the linked work contradicts what was said" and "nothing in the
 * work you confirmed is waiting on evidence" are both true in their own narrow
 * terms and both read, to somebody scanning the headings, as "there is nothing
 * wrong here". On a Change with two confirmed recordings and zero check
 * results they sat four lines under "None has evidence yet" — the product
 * disagreeing with itself in one field of view, which spends trust faster than
 * a plain absence would.
 *
 * This is the mechanical form: every sentence of a rendered surface that says
 * an absence of problems must, when nothing has been checked, also carry the
 * clause that says so. A test drives the zero-evidence Change through the
 * Change screen and the packet page and asks this of every sentence on them.
 *
 * @param {string} text the words a person reads, as one string
 * @returns {string[]} the offending sentences, empty when the rule holds
 */
export function absenceWithoutItsScope(text) {
  const sentences = String(text || '')
    .split(/(?<=[.!?])\s+/)
    .map((s) => s.trim())
    .filter(Boolean)
  return sentences.filter(
    (s) => ABSENCE_OF_PROBLEM.some((phrase) => s.includes(phrase)) && !s.includes(SAYS_ITS_SCOPE),
  )
}

/** The shapes a sentence takes when it says nothing is wrong. */
export const ABSENCE_OF_PROBLEM = Object.freeze([
  'contradicts what was said',
  'is waiting on evidence',
])

/** The clause that makes one of those sentences honest on an unchecked Change. */
export const SAYS_ITS_SCOPE = 'has been checked against what counts as done'

/* ── the one named version ─────────────────────────────────────────────── */

/**
 * The version this Change is about, as the Plain line names it: the short
 * revision and the day it was bound. It is a sentence in the text, not a chip
 * beside it — the baseline's H2T-4 was a person having to look away from the
 * words to a small tag to learn which version they were being asked about.
 *
 * The snapshot wins over the base revision when one exists: the snapshot is
 * the exact copy of the work being looked at, and the base is where it started.
 *
 * @param {object} change
 * @returns {{bound: boolean, key: string, values: object, revision: string|null, date: string|null}}
 */
export function targetVersion(change) {
  const b = change && change.implementation
  const revision = b ? str(b.snapshot_id) || str(b.base_revision) : null
  if (!b || !revision) return { bound: false, key: 'none', values: {}, revision: null, date: null }
  /* The revision that reads as nothing. A record can hold a revision of
   * zeros — git's own way of naming no revision at all — and rendering seven
   * of them as `version 0000000` puts a string on screen that reads like a
   * placeholder, which is what a person on the real screen paused over. The
   * test is on the SHORT form, because the short form is what is read: a
   * revision whose first seven characters are zeros names nothing a person
   * could look up either. It IS bound, so this does not say that no version
   * is bound; it says that what is written down names nothing. */
  if (/^0+$/.test(shortRevision(revision))) {
    return { bound: true, named: false, key: 'placeholder', values: {}, revision: null, date: dayOf(str(b.bound_at)) }
  }
  const date = dayOf(str(b.bound_at))
  return {
    bound: true,
    named: true,
    key: 'bound',
    values: { revision: shortRevision(revision), date: date || '' },
    revision: shortRevision(revision),
    date,
  }
}

/**
 * The project's name as a person reads it, from a recorded folder: its last
 * path segment, or null when nothing is recorded (BUILD-14, BUILD-10).
 *
 * ONE rule, used by the Inbox card and the Change screen's crumb bar. The
 * served Change carries no project display name (`crates/core/src/change.rs`
 * `ChangeView` has `project_id` and no name), so there is no recorded name to
 * prefer; the folder it is given is the one the record holds for the Change's
 * bound work, `implementation.workspace_identity`. A repository identity (the
 * engine's `wsid1:` digest) is never a name and is never passed here.
 *
 * @param {string|null} folder
 * @returns {string|null}
 */
export function projectNameOf(folder) {
  const f = str(folder)
  if (!f) return null
  const parts = f.split(/[\\/]+/).filter(Boolean)
  return parts.length ? parts[parts.length - 1] : null
}

/** The first 7 characters of a revision, or the whole of a shorter one. Never a hash Sam has to read. */
export function shortRevision(revision) {
  const r = str(revision)
  if (!r) return null
  return r.length > 7 ? r.slice(0, 7) : r
}

/**
 * The day part of a recorded UTC timestamp, as `YYYY-MM-DD`.
 *
 * Deliberately not localised and deliberately not a time: a locale-dependent
 * date would make this module impure and the record is what it is. Anything
 * that is not a canonical timestamp returns null, and the version line then
 * renders without a date rather than with an invented one.
 */
export function dayOf(value) {
  const v = str(value)
  if (!v) return null
  const m = /^(\d{4}-\d{2}-\d{2})T\d{2}:\d{2}:\d{2}/.exec(v)
  return m ? m[1] : null
}

/* ── Approve: why it is off, and what would turn it on ─────────────────── */

/**
 * The single reason Approve is disabled, and what would enable it.
 *
 * The prerequisites are checked first, because the first thing a person
 * should learn is the thing they can act on: nothing accepted, then no
 * version bound, then the accepted lines that still have no evidence. When
 * none of those holds, the act itself is live (T-S-4) in exactly one place —
 * the panel that names the version — and this control is a door to it, which
 * it says. Every reason here is a fact about the record; a disabled control
 * that mis-states the record is worse than one that says nothing, and that is
 * the defect T-3-6-F1 closed.
 *
 * @param {{criteria: string[], versionBound: boolean, problems: number,
 *   withoutEvidence: number, unlinkedRun: ?object}} record
 * @returns {{key: string, reason: string, enabler: string, values: object}}
 */
export function approveBlocker(record) {
  const r = record || {}
  const D = C.approve.disabled
  if (!list(r.criteria).length) return { key: 'no_criteria', values: {}, ...D.no_criteria }
  if (!r.versionBound) return { key: 'no_version', values: {}, ...D.no_version }
  /* T-3-6-F1 (`Q1`). This used to read a SECOND count — one derived from the
   * Change's `examination` field, which this build only ever sets to
   * `none_yet` — so a Change with two passing runs on record was told "1
   * required check has not run". A disabled control that mis-states the
   * record is worse than one that says nothing (03-USER-LENS.md rule 6), and
   * the cause was having two answers to one question. There is now one: the
   * number of accepted lines the SAME coverage the top sentence is built from
   * holds no evidence for. */
  /* T-3-6-F3 (`Q2`). THE ORDER, stated once: failure, then an unlinked run,
   * then no evidence. A Change whose record holds the opposite of what was
   * said was being given the sentence a Change with nothing run at all gets,
   * so the act it invited — run the check — was the one act that would not
   * help, and a person reading only this panel would go and do it. A failure
   * outranks both counts below it because it is the thing to act on first and
   * the only one where running something again is the wrong move. */
  const problems = typeof r.problems === 'number' ? r.problems : 0
  if (problems > 0) {
    const pair = problems === 1 ? D.problem_one : D.problem
    return { key: problems === 1 ? 'problem_one' : 'problem', values: { n: problems }, ...pair }
  }
  const n = typeof r.withoutEvidence === 'number' ? r.withoutEvidence : 0
  if (n > 0) {
    /* T-3-6-F1 / R-2026-09-23: the reason follows the same case the lines
     * themselves are in. A Change holding a finished, counted run that is
     * linked to nothing is not a Change with no evidence; it is a Change one
     * act away from having it, and the control says which act. */
    if (r.unlinkedRun) {
      const pair = n === 1 ? D.unlinked_one : D.unlinked
      return { key: n === 1 ? 'unlinked_one' : 'unlinked', values: { n }, ...pair }
    }
    const pair = n === 1 ? D.no_evidence_one : D.no_evidence
    return { key: n === 1 ? 'no_evidence_one' : 'no_evidence', values: { n }, ...pair }
  }
  return { key: 'decide_below', values: {}, ...D.decide_below }
}


/* ── the six stages, as views ──────────────────────────────────────────── */

/**
 * What each stage holds, and whether it holds anything at all.
 *
 * A stage with nothing in it is not empty on screen: it renders the sentence
 * that says what would put something there (03-USER-LENS.md rule 12). This
 * function decides `filled`; the view renders `HQ_COPY.change.stages[<id>].empty`
 * whenever it is false, for every one of the six, with no exception.
 *
 * @param {object} model the model `buildChangeView` returns
 * @returns {Array<{id: string, name: string, question: string, filled: boolean, empty: string}>}
 */
export function stagesOf(model) {
  const m = model || {}
  const filled = {
    define: Boolean(str(m.request && m.request.title)),
    prepare: Boolean(m.prepare && (m.prepare.repository || m.prepare.base)),
    build: Boolean(m.build && m.build.length),
    verify: Boolean(m.findings && m.findings.length),
    review: Boolean(m.review && m.review.length),
    approve: Boolean(m.accepted && m.accepted.criteria.length && m.version.bound),
  }
  return STAGES.map((id) => ({
    id,
    name: C.stages[id].name,
    question: C.stages[id].question,
    filled: filled[id] === true,
    empty: C.stages[id].empty,
  }))
}

/* ── the whole screen, decided in one place ────────────────────────────── */

/**
 * What this Change covers, and whose list it is (T-3-2b).
 *
 * Accepted folders win: they are what the scope check actually uses. A Change
 * with none shows what the proposer would cover, marked as not yet accepted,
 * and a Change with neither shows the served sentence for that — never a blank
 * list, and never a wildcard.
 *
 * @param {object} accepted
 * @param {object|null} proposals the `proposals` object of the proposals document
 */
export function coversOf(accepted, proposals) {
  const acceptedPaths = list(accepted && accepted.allowedScope).map(String)
  const proposed = proposals && proposals.scope ? proposals.scope : null
  if (acceptedPaths.length) {
    return {
      paths: acceptedPaths,
      accepted: true,
      attributionSlug: proposals && proposals.accepted ? str(proposals.accepted.scope_attribution_slug) : '',
      reasonSlug: '',
      emptySlug: '',
    }
  }
  return {
    paths: list(proposed && proposed.paths).map(String),
    accepted: false,
    attributionSlug: '',
    reasonSlug: str(proposed && proposed.reason_slug),
    emptySlug: str(proposed && proposed.empty_slug),
  }
}

/**
 * Everything the screen renders, from the documents and nothing else.
 *
 * @param {object} input
 * @param {object} input.change the `change` object of `/api/changes/<id>`
 * @param {object|null} [input.proposals] the `proposals` object of `/api/changes/<id>/proposals`
 * @param {Array<{id: string, findings: object[], focus: object|null}>} [input.work]
 *   one entry per CONFIRMED session, already built by the existing verdict
 *   model and focus view. The caller filters; this asserts the filter held.
 * @returns {object}
 */
export function buildChangeView(input) {
  const i = input || {}
  const change = i.change || {}
  const scope = scopeOfWork(change)
  const accepted = acceptedScope(change)
  const version = targetVersion(change)
  const confirmed = new Set(scope.confirmed)

  /* The filter is asserted here rather than trusted. A session that is only
   * suggested contributing one card would be the product deciding, on a
   * person's behalf, that a machine guess is a fact. */
  const work = list(i.work).filter((w) => w && confirmed.has(String(w.id)))

  const findings = []
  for (const w of work) for (const f of list(w.findings)) findings.push({ ...f, sessionId: String(w.id) })

  const conclusion = conclusionOf({
    criteria: accepted.criteria,
    evidence: list(i.evidence),
    /* RUN-LINK-TRUTH: how many lines the ENGINE names as having a passing
     * run nobody linked — the served reason, counted, never re-derived. */
    unlinked: typeof i.unlinkedLines === 'number' ? i.unlinkedLines : 0,
  })
  /* One count, from one place: the lines that still need evidence, and the
   * lines the record holds a problem for, are the same numbers the sentence
   * at the top of the screen is built from (T-3-6-F1). */
  const blocker = approveBlocker({
    criteria: accepted.criteria,
    versionBound: version.bound,
    /* T-3-6-F3: the two counts stay APART here. They were added together,
     * which is how a failure came to be told as an absence. */
    problems: conclusion.problems,
    withoutEvidence: conclusion.withoutEvidence,
    /* The same one fact `coverageRows` decides with, passed in rather than
     * decided again (T-3-6-F1 / R-2026-09-23). */
    unlinkedRun: i.unlinkedRun || null,
  })

  const b = change.implementation || null
  const model = {
    id: str(change.id),
    projectId: str(change.project_id),
    openedAt: str(change.opened_at),
    request: change.request || {},
    accepted,
    /* QUAL-F2-R2: read once here, so the Change screen and the packet say the
     * same thing about when the list was accepted. */
    retrospective: retrospectiveOf(change),
    proposals: list(i.proposals && i.proposals.criteria),
    /* The folders this Change covers: the accepted ones when there are any,
     * and otherwise the ones the proposer read off the confirmed work. The
     * sentence beside them is the served slug, never written here. */
    covers: coversOf(accepted, i.proposals || null),
    conclusion,
    version,
    scope,
    work,
    findings,
    review: list(change.reviews),
    build: list(i.build),
    prepare: b
      ? {
          repository: str(b.repository_identity),
          base: shortRevision(b.base_revision),
          snapshot: shortRevision(b.snapshot_id),
          workspace: str(b.workspace_identity),
        }
      : null,
    blocker,
    masking: change.masking || null,
  }
  model.stages = stagesOf(model)
  return model
}

export default buildChangeView
