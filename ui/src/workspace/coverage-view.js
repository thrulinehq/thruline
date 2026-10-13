/* NOT-APPROVAL-WORDS: reads which lines the engine names as having an unlinked run, for the lines' own words; it says nothing about approval (RUN-LINK-TRUTH-R5 census). */
/* What was looked at for each thing that counts as done — the deterministic
 * model behind it (T-3-1).
 *
 * PURE. No React, no fetch, no clock, no locale, no randomness, no model. One
 * set of functions over the document `GET /api/changes/<id>/coverage` serves,
 * and nothing else. `criterion-links.jsx` and `change-workspace.jsx` lay out
 * what this decided; every word they lay out comes from `HQ_COPY`.
 *
 * WHAT THIS FILE IS FOR. Sam's second question — "what still needs evidence,
 * and what's actually wrong?" — answered per thing-that-counts-as-done rather
 * than per piece of linked work. Until this card, the Change screen could say
 * "two recordings are linked" and could not say whether anything in them bore
 * on line 3.
 *
 * THE RULES IT HOLDS:
 *
 *  - NOTHING IS INFERRED HERE. Every state, basis and observed word on the
 *    screen is a typed field of the served document. This file does not match
 *    a criterion to a run by its words — the engine's one documented rule does
 *    that, and doing it a second time here is how two answers start.
 *  - A run that settled nothing does not count as evidence. `bears` is a typed
 *    field; `evidenceEntries` carries it through so the top sentence counts the
 *    same way this list reads (`Q3`).
 *  - Missing is missing. A line with nothing behind it renders "No test ran for
 *    this." — never a pass, never a failure.
 *  - An empty document is an answer. A Change with nothing accepted yet has no
 *    rows, and the caller renders the sentence that says what would fill it.
 */

import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'

const C = HQ_COPY.change.coverage

const list = (v) => (Array.isArray(v) ? v : [])
const str = (v) => (typeof v === 'string' && v.trim() ? v.trim() : null)

/** The five states the record can be in about one accepted line. */
export const STATES = Object.freeze(['evidence', 'none', 'absent_tool', 'ran_nothing', 'other_version', 'unlinked_run'])

/** The four reasons a link exists. There is no fifth, and never a timing one. */
export const BASES = Object.freeze(['declared', 'named', 'manual', 'not-bearing'])

/** The copy key for one basis, as the served document spells it. */
export function basisKey(basis) {
  return basis === 'not-bearing' ? 'not_bearing' : String(basis || '')
}

/**
 * The sentence for one row's state, with its one placeholder filled.
 *
 * A state the document invents is not rendered as a guess: the caller gets
 * null and shows nothing rather than a half-sentence.
 *
 * @param {{state: string, absent_tool: ?string}} row
 * @returns {string|null}
 */
export function stateSentence(row) {
  const state = str(row && row.state)
  if (!state || !STATES.includes(state)) return null
  const template = C.states[state]
  if (!template) return null
  return template.includes('{tool}')
    ? fill(template, { tool: str(row.absent_tool) || '' })
    : template
}

/**
 * T-3-6-F3 (`Q2`, `B16.8`, `B16.10`) — the two sentences that go beside one
 * finding, built ONCE, here, from typed fields of the served document.
 *
 * `line` is the run's own words and is never composed: it is shown, or the
 * absence is stated. `classText` is this product's sentence ABOUT a declared
 * class, keyed by the engine's own slug, so a class the copy file has no
 * sentence for renders the "nobody said" line rather than a slug. An absent
 * class is said, not assumed: `B16.10` is about a seeded demonstration never
 * reading as field accuracy, and silence reading as "ordinary" is that same
 * failure in a different shape.
 *
 * @param {{failureLine: ?string, evidenceClass: ?string, evidenceClassSlug: ?string}} item
 * @returns {{line: string|null, none: string, heading: string,
 *   classWord: string|null, classText: string, classLabel: string}}
 */
export function failureOf(item) {
  const F = HQ_COPY.change.coverage.failure
  const K = HQ_COPY.change.coverage.evidence_class
  const i = item || {}
  const slug = str(i.evidenceClassSlug)
  return {
    heading: F.heading,
    line: str(i.failureLine),
    none: F.none,
    classLabel: K.label,
    classWord: str(i.evidenceClass),
    classText: (slug && K[slug]) || K.none,
  }
}

/**
 * One linked piece of evidence, reduced to what a person reads.
 *
 * @param {object} raw one entry of a criterion's `items` or `removed`
 * @returns {object|null} null when the document's entry is not one we can
 *   render without inventing something
 */
export function itemOf(raw, counts = null) {
  const r = raw || {}
  const tally = (counts && str(r.evidence_id) && counts.get(str(r.evidence_id))) || null
  const basis = str(r.basis)
  if (!basis || !BASES.includes(basis)) return null
  const evidenceId = str(r.evidence_id)
  if (!evidenceId) return null
  const key = basisKey(basis)
  return {
    id: str(r.link_id) || evidenceId,
    kind: str(r.kind) || 'check_run',
    evidenceId,
    session: str(r.session_id),
    basis,
    basisText: C.basis[key] || '',
    /* The evidence's own typed name, as the record holds it and the masker
     * left it. It is the record speaking, not this product. */
    name: str(r.name),
    state: str(r.observed),
    bears: r.bears === true,
    note: r.note_slug === 'item_checked_nothing'
      ? C.item.checked_nothing
      : r.note_slug === 'item_out_of_reach'
        ? C.item.out_of_reach
        : null,
    actor: str(r.actor),
    at: str(r.asserted_at),
    reason: str(r.reason),
    /* T-3-2 — was this examination about the version you are looking at?
     * `null` is the question NOT ASKED: this piece of evidence is not an
     * examination, or the Change has not written down what it is about, and a
     * mismatch against nothing is not a fact. */
    applies: r.applies === true ? true : r.applies === false ? false : null,
    appliesText: (str(r.applies_slug) && C.applies[str(r.applies_slug)]) || null,
    /* T-S-2/T-S-3 — does this evidence still apply now that the tree has
     * moved? `null` is the question NOT ASSESSED, and an absent assessment is
     * never read as "still applies". The SENTENCE is built in
     * `change-model.js`, from `HQ_COPY.change.coverage.freshness`, so it is
     * built once for the Change screen, the Inbox row and the packet. */
    fresh: r.fresh === true ? true : r.fresh === false ? false : null,
    freshSlug: str(r.fresh_slug),
    /* T-3-6-F3 (`Q2`) — what the check SAID when it broke, as the served
     * document holds it: a typed field read out of the run's own output at
     * record time and masked at the boundary. `null` is "the record holds no
     * line from the failure", which the caller says in those words and never
     * fills in. Nothing here parses it, shortens it or rewrites it. */
    failureLine: str(r.failure_line),
    /* T-3-6-F3 (`B16.10`) — where this finding came from, DECLARED at record
     * time. `null` is nobody said; it is never read as "ordinary". The
     * SENTENCE is built in `change-model.js` so the Change screen, the Inbox
     * row and the packet build it once. */
    evidenceClass: str(r.evidence_class),
    evidenceClassSlug: str(r.evidence_class_slug),
    /* T-3-6-F3 verifier finding 1. The two SENTENCES are attached HERE, on the
     * one item derivation every consumer goes through, and nowhere else. They
     * were built a level up in `change-model.js`, on a copy of the items the
     * packet reads — so the packet carried what broke and the Verify stage,
     * which is where "Look at what broke" sends a person, rendered the raw
     * items and showed neither. One place, one answer. */
    failure: failureOf({
      failureLine: str(r.failure_line),
      evidenceClass: str(r.evidence_class),
      evidenceClassSlug: str(r.evidence_class_slug),
    }),
    /* T5/T6 — the run's own typed test counts, when the record holds them.
     * The coverage document does not carry them; the run sheet's document
     * (`GET /api/changes/<id>/run`, `checks[].last`) does, per run key, and
     * `testCounts` joins the two by the execution record's own binding id —
     * never by a name or a time. `null` is "the record this screen read holds
     * no count for this run", and nothing downstream fills it in. */
    testsPassed: tally ? tally.passed : null,
    testsFailed: tally ? tally.failed : null,
  }
}

/**
 * T5/T6 — the typed test counts the record holds, keyed by the evidence id a
 * coverage item carries.
 *
 * The join is typed end to end: the run sheet's `checks[].last.run` is an
 * execution record's `run_key`; that record's `binding_id` is the evidence id
 * of the check run it produced. A count is carried only when both halves are
 * integers — a missing half is not zero.
 *
 * @param {object} doc the coverage object, carrying `executions` and, when
 *   the reader asked for them, `run_checks` (the run sheet's `checks`)
 * @returns {Map<string, {passed: number, failed: number}>}
 */
export function testCounts(doc) {
  const bindings = new Map()
  for (const e of list(doc && doc.executions)) {
    const run = str(e && e.run_key)
    const binding = str(e && e.binding_id)
    if (run && binding) bindings.set(run, binding)
  }
  const out = new Map()
  for (const c of list(doc && doc.run_checks)) {
    const last = c && c.last
    if (!last) continue
    const binding = bindings.get(str(last.run) || '')
    if (!binding) continue
    if (!Number.isInteger(last.tests_passed) || !Number.isInteger(last.tests_failed)) continue
    out.set(binding, { passed: last.tests_passed, failed: last.tests_failed })
  }
  return out
}

/**
 * One row per thing that counts as done, in the order it was accepted.
 *
 * @param {object} doc the `coverage` object of the served document
 * @returns {Array<{n: number, text: string, state: string, sentence: string,
 *   items: object[], removed: object[]}>}
 */
export function coverageRows(doc) {
  /* T-3-6-F1, coordinator ruling R-2026-09-23: the matching rule does not
   * widen, and the SENTENCE stops being false. "No test ran for this." is
   * reserved for a Change the record holds no unlinked, finished, counted test
   * run for. When there IS one, a line with nothing linked to it says so and
   * says what to do — the run is right there, and the person is one click from
   * putting it where it belongs. One decision, made here, so the Change
   * screen, the packet and the Approve control cannot answer it differently. */
  const unlinked = unlinkedCompletedRun(doc)
  const counts = testCounts(doc)
  return list(doc && doc.criteria).map((c) => ({
    n: typeof c.index === 'number' ? c.index : 0,
    text: String(c.text == null ? '' : c.text),
    state: (str(c.state) || 'none') === 'none' && unlinked ? 'unlinked_run' : str(c.state) || 'none',
    sentence: ((str(c.state) || 'none') === 'none' && unlinked
      ? C.states.unlinked_run
      : stateSentence(c)) || C.states.none,
    /* The technical half of the sentence above: which run, and which version
     * it ran on. Named, never summarised, and null when there is no such run. */
    unlinkedRun: (str(c.state) || 'none') === 'none' ? unlinked : null,
    tool: str(c.absent_tool),
    items: list(c.items).map((i) => itemOf(i, counts)).filter(Boolean),
    removed: list(c.removed).map((i) => itemOf(i)).filter(Boolean),
  }))
}

/**
 * RUN-LINK-TRUTH — the accepted lines the ENGINE names as having a passing run
 * for this Change that nobody linked (`readiness_unlinked_run`), by criterion
 * index, from the served readiness the coverage document carries. Nothing is
 * decided here; no readiness served, and the answer is empty.
 *
 * @param {object} doc the `coverage` object
 * @returns {number[]}
 */
export function servedUnlinkedLines(doc) {
  const readiness = doc && doc.readiness
  return list(readiness && readiness.obligations)
    .filter((o) => o && o.unsatisfied_slug === 'readiness_unlinked_run' && typeof o.criterion_index === 'number')
    .map((o) => o.criterion_index)
}

/**
 * A finished test run this Change holds that is linked to no accepted line.
 *
 * Typed fields only: `execution === 'completed'` (T-3-4's veto already read),
 * `bears === true` and an observed result of `proven` — which, since
 * T-3-6-F1, means the record holds its own count of passes. A candidate that
 * appears under any criterion's items is linked and is not this.
 *
 * @param {object} doc the `coverage` object
 * @returns {{id: string, name: string}|null}
 */
export function unlinkedCompletedRun(doc) {
  const linked = new Set()
  for (const c of list(doc && doc.criteria)) {
    for (const i of list(c && c.items)) if (i && i.evidence_id) linked.add(String(i.evidence_id))
  }
  for (const c of list(doc && doc.candidates)) {
    if (!c || c.kind !== 'check_run') continue
    if (c.execution !== 'completed' || c.bears !== true || c.observed !== 'proven') continue
    const id = c.evidence_id ? String(c.evidence_id) : ''
    if (!id || linked.has(id)) continue
    return { id, name: String(c.name == null ? '' : c.name) }
  }
  return null
}

/**
 * The typed evidence entries the Change screen and the packet already take.
 *
 * `provenance` is `captured` for everything here and says so: each of the three
 * kinds is something this machine's own recorder wrote. `bears` travels with
 * the entry, because a run that settled nothing must not move the count in the
 * top sentence — that is `Q3` expressed as one boolean rather than as a rule
 * two files have to remember.
 *
 * @param {object} doc the `coverage` object
 * @returns {Array<{criterion: number, state: string, provenance: string,
 *   session: string, bears: boolean, summary: string}>}
 */
export function evidenceEntries(doc) {
  const out = []
  for (const row of coverageRows(doc)) {
    for (const item of row.items) {
      if (!item.session) continue
      /* A result about another version does not move the count in the top
       * sentence, for the same reason it does not change the row's state: it
       * is a result about something else (`B6.4`). It is still on the screen,
       * with the sentence that says which version it was about. */
      if (item.applies === false) continue
      out.push({
        criterion: row.n,
        state: item.state,
        provenance: 'captured',
        session: item.session,
        seq: null,
        bears: item.bears,
        summary: item.name,
      })
    }
  }
  return out
}

/**
 * What the technical layer says about how much of this is written down.
 *
 * One typed field, with its noun attached. A bare number is Sam's "a number
 * with no sentence".
 *
 * @param {object} doc
 * @returns {string}
 */
export function rowsLine(doc) {
  const rows = doc && typeof doc.link_rows === 'number' ? doc.link_rows : 0
  if (rows <= 0) return C.rows_none
  return rows === 1 ? C.rows_one : fill(C.rows_many, { rows })
}

/**
 * Why the link control is off, and what would turn it on.
 *
 * Two reasons only, in this order, because the first is about the record and
 * the second is about the person: there is nothing to link, or nobody has said
 * who is deciding. A control that is on when neither holds returns null.
 *
 * `whySlug` is the served reason there is nothing to link — the link layer's
 * own refusal word for this Change. It exists because "confirm the work that
 * belongs to this Change" is the wrong act to ask for on a Change whose work is
 * already confirmed; the pair keyed by the slug names the act that would
 * actually turn the control on. A slug with no pair falls back to the general
 * sentence rather than to silence.
 *
 * @param {{candidates: object[], actor: string, whySlug: ?string}} record
 * @returns {{reason: string, enabler: string}|null}
 */
export function linkBlocker(record) {
  const r = record || {}
  if (!list(r.candidates).length) {
    const slug = str(r.whySlug)
    return (slug && C.disabled.by_reason[slug]) || C.disabled.nothing_to_link
  }
  if (!str(r.actor)) return C.disabled.needs_name
  return null
}

/**
 * Everything the coverage section renders, decided in one place.
 *
 * @param {object} doc the `coverage` object of `/api/changes/<id>/coverage`
 * @returns {object}
 */
export function buildCoverageView(doc) {
  const rows = coverageRows(doc)
  return {
    accepted: doc && typeof doc.scope_version === 'number' ? doc.scope_version : null,
    whySlug: str(doc && doc.nothing_to_link_slug),
    /* Why nothing here says which version it was checked on (T-3-2b). Served
     * exactly when this Change has no record of what it is about, which is the
     * one case where every item's `applies` is null. */
    nothingBoundSlug: str(doc && doc.nothing_bound_slug),
    /* Which linked sessions are one captured execution rather than a recorded
     * conversation (T-3-2b). The loader reads it so it does not ask an
     * execution for a transcript and print the refusal at a person. */
    executionSessions: list(doc && doc.execution_sessions).map(String),
    rows,
    candidates: list(doc && doc.candidates).map((c) => ({
      kind: str(c.kind) || 'check_run',
      evidenceId: str(c.evidence_id),
      session: str(c.session_id),
      name: str(c.name),
      state: str(c.observed),
      bears: c.bears === true,
      execution: str(c.execution) || 'not_run',
      executionSlug: str(c.execution_slug) || 'execution_not_run',
    })).filter((c) => c.evidenceId),
    /* T-3-4 — every run this Change has an EXECUTION record for, including the
     * ones that never produced a binding. An interrupted run is exactly that
     * case, and a screen that only knew about bindings would show nothing at
     * all where a check was cut short. */
    executions: list(doc && doc.executions).map((e) => ({
      runKey: str(e.run_key),
      bindingId: str(e.binding_id),
      name: str(e.name),
      state: str(e.state) || 'not_run',
      stateSlug: str(e.state_slug) || 'execution_not_run',
      reasonSlug: str(e.reason_slug),
      actor: str(e.actor),
      startedAt: str(e.started_at),
      recordedAt: str(e.recorded_at),
      cleanupVerified: e.cleanup_verified === true ? true : e.cleanup_verified === false ? false : null,
      survivors: typeof e.survivors === 'number' ? e.survivors : null,
      settles: e.settles === true,
    })).filter((e) => e.runKey),
    rowsLine: rowsLine(doc),
    /* T5 — the masking statement the engine served beside this document
     * (`CoverageView.masking`), word for word, or null when it served none. */
    masking: str(doc && doc.masking && (doc.masking.statement || doc.masking.reason)),
    /* Read first: the lines with nothing behind them and the lines whose
     * evidence settled nothing. Same order as the rows, so a person scanning
     * down finds them where they already are. */
    needing: rows.filter((r) => r.state !== 'evidence'),
  }
}

/**
 * The command that records one run for one Change (T-3-2b).
 *
 * ONE BUILDER, TWO SCREENS. The first-use path and the Verify view both offer
 * "run the check", and they call this — so the text a person copies on the
 * first screen is the same text they copy on the Change screen, and neither can
 * drift into a command the other does not recognise.
 *
 * The command the person would run is left as the person's: this product does
 * not choose what to run, and a command it made up would be the first thing
 * Sam pasted without reading.
 *
 * @param {string} changeId
 * @returns {string}
 */
export function runCheckCommand(changeId) {
  return fill(C.run_check_command, { change: String(changeId || '') })
}

export default buildCoverageView
