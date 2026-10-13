/* NOT-APPROVAL-WORDS: the Inbox card's reasons. `awaiting approval` fires only when the engine says ready; nothing here says why a shut Approve door is shut (RUN-LINK-TRUTH-R5 census). */
/* Attention — the ONE derivation of "does this Change need a person, and how
 * many things on it need you" (A3-F2, report `#ui-counts`, F2).
 *
 * PURE. No React, no fetch, no clock, no locale, no randomness, no model. It
 * reads the same documents the Change routes serve and the ONE derived
 * view-model (`deriveChangeModel`), and it decides two numbers and nothing
 * else:
 *
 *  - `needsPerson` — whether this Change is one the Inbox lists. The Inbox's
 *    heading, and every other list of Changes that says how many need a
 *    person, counts the Changes for which this is true. Nothing else in
 *    `ui/src` may decide it (`attention-count.test.js` greps for a second
 *    decision).
 *  - `items` — how many things ON this Change need you. A Change-level count,
 *    with its own noun (`HQ_COPY.change.attention`), never the Inbox's noun.
 *
 * and one typed number beside them, `notes`: the informational items (a
 * finding recorded "for the record", or a card whose state this file does not
 * know). A note is counted and stated, never "open", and never part of
 * `items` or of `needsPerson` (A3-NOTES-NEVER-OPEN).
 *
 * Why one file: before this, "needs a person" was decided inline in the
 * Inbox's own item builder, so the next surface that wanted the number had to
 * re-derive it — which is exactly how the way-in strip came to say something
 * different about the same Change (A3-F14). `inbox-view.js` and
 * `change-model.js` both import this file; neither re-derives it.
 */

import { acceptedScope, scopeOfWork } from './change-view.js'

const list = (v) => (Array.isArray(v) ? v : [])
const str = (v) => (typeof v === 'string' && v.trim() ? v.trim() : null)

/**
 * The reasons, in priority order, highest first.
 *
 * The order is the argument: a person told "2 things have no evidence yet"
 * and not told that the record holds the opposite of what was said has been
 * told the smaller truth first. Observed failure outranks everything; a
 * decision waiting on a person outranks nothing, because it is the one that
 * can wait.
 */
export const REASONS = Object.freeze([
  'observed_failure',
  'applicable_finding',
  'interrupted_capture',
  'missing_evidence',
  'invalidated_after_edit',
  'approved_code_moved',
  'awaiting_definition',
  'awaiting_approval',
])

/**
 * The verdict-card states a person must act on, and nothing else.
 *
 * An explicit allow-list (A3-NOTES-NEVER-OPEN): the Inbox used to count ANY
 * state it did not recognise as an open finding, so a card kind added later
 * would have become an attention item without anyone deciding it should. A
 * state not named here is a note — counted and stated, never "open".
 * `not_proven` is counted apart, as the record showing the opposite.
 */
export const OPEN_CARD_STATES = Object.freeze(['loose', 'couldnt', 'fog'])

/**
 * The findings of the confirmed work, counted by the class each reason is
 * about. Every one of these is a TYPED field of the verdict the existing
 * model already builds: `state` is the observed result of one finding, and
 * `kind === 'recording_gap'` is the card `verdict-model.js` adds when, and
 * only when, the recorder says the capture is degraded.
 *
 * `notes` is the typed count of informational items: a card whose state is
 * not on the allow-list above, or one that carries `severity: 'info'` (the
 * verdict model makes no such card today — `buildVerdict` keeps an `info`
 * finding off the cards — and this holds the line if one ever arrives).
 *
 * @param {Array<{id: string, findings: object[]}>} work confirmed work only
 * @returns {{contradicted: number, open: number, gaps: number, notes: number}}
 */
export function findingCounts(work) {
  let contradicted = 0
  let open = 0
  let notes = 0
  const gaps = new Set()
  for (const w of list(work)) {
    for (const f of list(w && w.findings)) {
      if (!f) continue
      if (f.kind === 'recording_gap') {
        gaps.add(String(w.id))
        continue
      }
      if (f.severity === 'info') notes += 1
      else if (f.state === 'not_proven') contradicted += 1
      else if (OPEN_CARD_STATES.includes(f.state)) open += 1
      else if (f.state) notes += 1
    }
  }
  return { contradicted, open, gaps: gaps.size, notes }
}

/**
 * Every reason this Change carries, highest first.
 *
 * @param {object} input
 * @param {object} input.change the `change` object of `/api/changes/<id>`
 * @param {object|null} [input.proposals] the `proposals` object
 * @param {Array<object>} [input.work] the CONFIRMED work, already adapted
 * @param {Array<{criterion: number, state: string}>} [input.evidence] which
 *   accepted criterion has evidence, and what it showed. Empty on every route
 *   this build serves — the derived model's `criteria` stand in for it.
 * @param {Array<object>} [input.applicability] what stopped applying after an
 *   edit (Phase 3 writes it; empty until then)
 * @param {object} [input.hq] the ONE derived view-model for this Change
 * @returns {Array<{id: string, values: object, count: number, variant: ?string}>}
 */
export function reasonsFor(input) {
  const i = input || {}
  const change = i.change || {}
  const accepted = acceptedScope(change)
  const scope = scopeOfWork(change)
  const confirmed = new Set(scope.confirmed)
  /* Asserted, not trusted: a session that is only suggested contributing a
   * reason would be the product deciding, on a person's behalf, that a
   * machine guess is a fact (`B8.8`). */
  const work = list(i.work).filter((w) => w && confirmed.has(String(w.id)))
  const counts = findingCounts(work)

  const out = []
  /* `shown` is the number a sentence about the reason says, when that is not the number of items it adds up to: what is
   * missing that is not one of the things that count as done (BRIEF-COUNT-AGREES-R2) is an item that needs you, and it is
   * never one of them. */
  const add = (id, count, variant, shown) => out.push({
    id, values: count == null ? {} : { n: count }, count: count == null ? 0 : count, variant: variant || null, ...(shown ? { shown: shown.lines, words: shown.words } : {}),
  })

  if (counts.contradicted > 0) add('observed_failure', counts.contradicted)
  if (counts.open > 0) add('applicable_finding', counts.open)
  /* T-3-4 — a run that was cancelled, interrupted or blocked is an interrupted
   * capture, and the Change's own derived model is where that fact lives. It
   * is added to the capture gaps the work already carried rather than replacing
   * them: both are the same thing seen from two records. */
  const stoppedRuns = list(i.hq && i.hq.stoppedRuns).length
  if (counts.gaps + stoppedRuns > 0) add('interrupted_capture', counts.gaps + stoppedRuns)

  /* P6-INBOX-REASON — which accepted lines have something behind them comes
   * from the ONE derived view-model, not from a list every caller hard-coded
   * empty. A criterion counts as covered when the model says it is `proven` —
   * the same judgement the Change screen and the packet render. `recorded`
   * and `needs_recheck` are NOT covered: something ran and settled nothing, or
   * settled it on a version the record has moved off, and both of those are
   * still reasons a person is needed. `i.evidence` is still honoured when a
   * caller passes one, so nothing supplying real rows changes behaviour. */
  const derived = list(i.hq && i.hq.criteria)
    .filter((c) => c && c.state === 'proven')
    .map((c) => ({ criterion: c.n, state: c.state }))
  const evidence = list(i.evidence).length ? list(i.evidence) : derived
  if (accepted.criteria.length) {
    const covered = new Set()
    for (const e of evidence) {
      const n = e && typeof e.criterion === 'number' ? e.criterion : null
      if (n != null && n >= 1 && n <= accepted.criteria.length) covered.add(n)
    }
    /* TELL-DRAFT-WAIVED — a line a named person set aside under the accepted
     * profile's authority (waived, or does not apply) is not one that is
     * waiting on evidence: nobody is to check it. Read from the served
     * readiness's `waivedLines`, the field the Readiness list reads. Without
     * this a Change ready with exceptions was filed under missing evidence. */
    for (const o of list(i.hq && i.hq.readiness && i.hq.readiness.waivedLines)) {
      const n = o && typeof o.n === 'number' ? o.n : null
      if (n != null && n >= 1 && n <= accepted.criteria.length) covered.add(n)
    }
    /* PATH-UI-2-R2 — the served readiness answer is read here too, as the
     * Change screen reads it (`hq.readiness`). Proven on the coverage is not
     * the same as ready: a line whose evidence is there and whose applicability
     * was never assessed reads `proven` and is still listed by the engine as
     * missing, and this file used to count it as covered, so a Change with such
     * a line beside a set-aside one was told to the person as "nothing is
     * waiting" while the Change screen said it could not be approved yet.
     *
     * When the answer is served, is NOT ready, and names what is missing:
     *  - a line it names is never covered, whatever the coverage calls it, and
     *    a set-aside line never stands in for it (waived lines are never in the
     *    engine's missing list, and never subtract from another line's count);
     *  - anything else it names (the work is not confirmed, a review is not
     *    written down) is one more thing to check, counted once.
     * With no answer, or a ready one, or one that names nothing, nothing here
     * moves: the reading above is what it was. */
    const answer = i.hq && i.hq.readiness
    const named = answer && !answer.absent && !answer.ready ? list(answer.missing) : []
    const otherwise = new Set()
    named.forEach((o, at) => {
      const n = o && typeof o.n === 'number' ? o.n : null
      if (n != null && n >= 1 && n <= accepted.criteria.length) covered.delete(n)
      else otherwise.add(str(o && o.id) || '#' + at)
    })
    /* BRIEF-COUNT-AGREES-R2 — the same count the top, the list under it, the
     * whole-story page and Ask state (`readiness.tally`): the lines the served
     * answer says still need evidence, and each thing that is not a line and
     * is missing (the identity, a review), once. Read from the answer, not
     * worked out again from the coverage above, which counts a set-aside line
     * and a never-assessed line the other way round. With no answer, or one
     * that names nothing, the reading above is what it was. */
    const said = answer && !answer.absent && !answer.ready ? answer.tally : null
    const served = said ? list(said.needing).length + list(said.beyond).length : 0
    const without = served ? served : accepted.criteria.length - covered.size + otherwise.size
    /* T-3-6-F1 (`Q1`): one count, from the coverage above, and no fallback. */
    if (without > 0) add('missing_evidence', without, null, served ? { lines: list(said.needing).length, words: list(said.beyond).map((b) => b.words).filter(Boolean).join(' ') } : null)
  }

  const invalidated = list(i.applicability).filter((a) => a && a.applies === false).length
  if (invalidated > 0) add('invalidated_after_edit', invalidated)

  /* INBOX-APPROVED-THEN-MOVED — the Brief already says the code in the
   * recorded folder moved, but a standing approval could leave this Change
   * out of the Inbox altogether. Both facts come from this same derived
   * model: the served code_moved comparison and the standing decision. A
   * withdrawn approval has no standing decision and adds no reason.
   *
   * SEAM-INBOX-APPROVED-VERIFY finding 1 — the same is true when the record
   * itself has been re-bound to another version since the approval
   * (`decision.movedSince`, which "Check it again" produces) and the served
   * comparison says nothing. One reason for both: either way the approval
   * covers only the version it names and the code is no longer that version,
   * and the sentence says exactly that and nothing about direction. */
  const standing = i.hq && i.hq.decision && i.hq.decision.approved
  if (standing && (i.hq.codeMoved || i.hq.decision.movedSince)) add('approved_code_moved', null)

  if (!accepted.criteria.length) add('awaiting_definition', null)
  /* T-3-5 — `awaiting approval` fires from the READINESS computation, which is
   * the record, as well as from the served placeholder. */
  const ready = Boolean(i.hq && i.hq.readiness && i.hq.readiness.ready)
  const approved = Boolean(i.hq && i.hq.decision && i.hq.decision.approved)
  if (str(change.decision) === 'awaiting_approval' || (ready && !approved)) add('awaiting_approval', null)

  out.sort((a, b) => REASONS.indexOf(a.id) - REASONS.indexOf(b.id))
  return out
}

/**
 * The attention a Change needs, decided once.
 *
 * `items` is the sum of the counted reasons (each one is a count of distinct
 * things of one class: findings the record contradicts, findings still open,
 * gaps and stopped runs, accepted lines with nothing proven behind them,
 * checks that stopped applying) plus one for each reason that is a single
 * decision waiting on a person (`approved_code_moved`, `awaiting_definition`,
 * `awaiting_approval`).
 * `notes` never contributes to it.
 *
 * @param {object} input the same input `reasonsFor` takes
 * @returns {{needsPerson: boolean, items: number, notes: number, reasons: object[]}}
 */
export function attentionOf(input) {
  const i = input || {}
  const reasons = reasonsFor(i)
  const confirmed = new Set(scopeOfWork(i.change || {}).confirmed)
  const work = list(i.work).filter((w) => w && confirmed.has(String(w.id)))
  const items = reasons.reduce((sum, r) => sum + (r.values && typeof r.values.n === 'number' ? r.count : 1), 0)
  return Object.freeze({
    needsPerson: reasons.length > 0,
    items,
    notes: findingCounts(work).notes,
    reasons,
  })
}

export default attentionOf
