/* T5 (report F13, its T5 part only) — the message for the agent, rebuilt
 * whenever the Change's verdict moves.
 *
 * THE DEFECT. The only hand-off the Change screen offered was each finding
 * card's own prompt, composed from the SESSION's verdict. A Check-it-again
 * run is an execution session, which the reader skips on purpose, so a
 * failing re-run reached coverage and readiness and never reached the card
 * holding "Copy a message for the agent". After a failing re-run the screen
 * still offered a message that did not mention the failure, or none at all.
 *
 * THE RULE. The draft is a PURE function of the one derived view-model
 * (`deriveChangeModel`'s output) — the accepted lines, the state the model
 * gave each, the version the screen names, and each run's typed fields:
 * its name, its observed result, whether it was about this version, its typed
 * test counts, and the failing line the engine served masked. Same fields,
 * same bytes; a different verdict, a different draft.
 *
 * NOTHING IS STORED. There is no cache, no browser storage and no state held
 * across a read: the draft is composed at render from the documents the last
 * read returned, and the run sheet's `onRan` re-read is what refreshes it.
 *
 * WHAT IT MAY SAY. Every sentence is `HQ_COPY.change.tell`, filled by
 * `compose`, which throws on a missing or empty slot rather than rendering a
 * hole. No sentence says a test passed unless it is the run's own typed count
 * saying it. Nothing is sent: a person copies the draft and pastes it.
 *
 * A LINE THE SERVED ANSWER SAYS STILL NEEDS EVIDENCE (BRIEF-COUNT-AGREES-R3).
 * The panel used to read the coverage alone, so a line whose evidence was never
 * assessed as applying to this version — proven on the coverage, and listed as
 * needing evidence by the engine's readiness — was "covered": "Nothing to ask
 * the agent for: every line that counts as done is proven on <V>" over a Change
 * that says one still needs evidence. It reads the one tally the top and the list
 * read (`hq.readiness.tally.needing`) and asks about those lines too.
 *
 * A LINE A PERSON SET ASIDE (TELL-DRAFT-WAIVED). A line the served readiness
 * says a named person set aside — waived, or does not apply, under the
 * accepted profile's authority (T-5-3) — is not asked of the agent: nobody is
 * to check it, so the draft neither names it nor asks for a run on it. Which
 * lines those are is read from `hq.readiness.waivedLines`, the field the
 * Readiness list reads, and from nothing else. When every other line is
 * proven, the covered sentence says so with the set-aside lines in it, and
 * never that every line is proven.
 *
 * WHO IT ADDRESSES (BUILD-16-R2, path step 5, PW-F8). The draft is the
 * agent's: what to do, and the one command that records a run for this
 * Change (`runCheckCommand`, the run door's own builder). It says a person
 * then decides which line the run supports, and never that the run will
 * count. The masking statement is the person's, so it is returned beside the
 * draft (`masking`) for the screen to show next to the copy control, never
 * inside the copied text.
 */

import { HQ_COPY } from './hq-copy.js'
import { compose } from './hq-narrate.js'
import { runCheckCommand } from './coverage-view.js'

const T = HQ_COPY.change.tell
const D = T.draft

const list = (v) => (Array.isArray(v) ? v : [])
const str = (v) => (typeof v === 'string' && v.trim() ? v.trim() : null)

/** The draft kinds, worst first. A Change's draft takes the first one any of its lines reaches. */
export const TELL_KINDS = Object.freeze(['opposite', 'older', 'nothing', 'fog'])

/**
 * The most recent of some items, by the one time an item carries — when it
 * was linked (`asserted_at`) — and by served order on a tie. The served order
 * alone is not a time: the coverage document lists a criterion's items by
 * link id, so the last item is not the latest run.
 */
function latest(items) {
  return items.reduce((out, i) => (out === null || String(i.at || '') >= String(out.at || '') ? i : out), null)
}

/**
 * What the record says about one accepted line, as one of the draft kinds —
 * or `proven` when the model already holds it proven on this version.
 *
 * TYPED FIELDS FIRST, in the card's order (A3 g2 r1). The first fact is a run
 * that shows the opposite ON THIS VERSION: it bears, it is about this version
 * (`applies`), and the record does not call it stale (`fresh`) — the engine's
 * own test for a run that counts. It is read before the model's state word,
 * because the model may lead that line with "Needs a re-check": a proof from
 * another version went stale beside the failure, and freshness outranks a run
 * there, as the engine's readiness reason does. Reading the state word first
 * is what made the message say "Nothing on <version> has run for it" directly
 * under the failure it had just shown (the verifier's real-product walk).
 *
 * "Another version" is said only of a run that was about another version
 * (`applies === false`). A stale run that was about THIS version is said as
 * that (`moved`), never as "another version".
 *
 * @param {object} c one of `hq.criteria`
 * @param {string|null} version the short version the screen names
 * @returns {{kind: string, item: object|null, moved?: boolean}}
 */
export function lineKindOf(c, version) {
  const items = list(c && c.items)
  if (!version) return { kind: 'fog', item: null }
  const here = items.filter((i) => i && i.bears === true && i.applies !== false)
  const opposite = here.filter((i) => i.fresh !== false && i.state === 'not_proven')
  if (opposite.length) return { kind: 'opposite', item: latest(opposite) }
  if (c.state === 'proven') return { kind: 'proven', item: null }
  /* Proof that ran on this version and no longer applies: what it checked has
   * changed since. */
  const staleHere = here.filter((i) => i.fresh === false)
  if (c.state === 'needs_recheck' && staleHere.length) return { kind: 'older', item: latest(staleHere), moved: true }
  /* Every bearing run was about another version. */
  const elsewhere = items.filter((i) => i && i.bears === true && i.applies === false)
  if (elsewhere.length && !here.length) return { kind: 'older', item: latest(elsewhere) }
  /* The model holds the line stale and no item says which one went stale:
   * the only bearing runs are on this version, so it is said as that. */
  if (c.state === 'needs_recheck' && here.length && !items.some((i) => i && i.fresh === false)) {
    return { kind: 'older', item: latest(here), moved: true }
  }
  return { kind: 'nothing', item: null }
}

/**
 * The accepted lines a named person set aside, by number — from the served
 * readiness the Readiness list reads (`waivedLines`: an outcome of waived or
 * does-not-apply WITH its waiver record). No readiness, or an outcome with no
 * waiver record, sets nothing aside.
 *
 * @param {object|null} hq
 * @returns {Set<number>}
 */
function setAsideLines(hq) {
  return new Set(list(hq && hq.readiness && hq.readiness.waivedLines).map((o) => (o ? o.n : null)).filter((n) => Number.isInteger(n)))
}

/**
 * BRIEF-COUNT-AGREES-R3 — the accepted lines the served answer says still need
 * evidence, by number: the one tally (`readiness.tally.needing`) the top, the
 * list under it, the whole-story page, Ask and the Inbox all read. A line the
 * coverage holds as proven and the answer holds as needing evidence — its
 * applicability was never assessed — is one of them, and it is not "covered". No
 * answer served, or one that names no such line, names none.
 *
 * @param {object|null} hq
 * @returns {Object<number, true>} the line numbers
 */
function needingLines(hq) {
  const r = hq && hq.readiness
  const out = {}
  for (const o of list(r && !r.absent && r.tally && r.tally.needing)) if (o && Number.isInteger(o.n)) out[o.n] = true
  return out
}

/** One accepted line's paragraph of the draft, every sentence composed. */
function paragraph(c, k, version) {
  const what = c.text
  const check = k.item ? str(k.item.name) : null
  if (k.kind === 'opposite') {
    const counted = Number.isInteger(k.item.testsFailed) && Number.isInteger(k.item.testsPassed)
    const lines = [counted
      ? compose(D.opposite, { what, version, check, failed: k.item.testsFailed, passed: k.item.testsPassed })
      : compose(D.opposite_no_count, { what, version, check })]
    const line = k.item.failure ? str(k.item.failure.line) : str(k.item.failureLine)
    lines.push(line ? compose(D.failure_line, { line }) : D.failure_none)
    const ask = compose(D.ask_opposite, { check })
    /* HTT-TEXT-RUN-TOGETHER — a failure that printed more than one line is
     * quoted whole, and the ask starts on a line of its own: the last line of
     * the failure never runs straight into "Please fix it, …". A one-line
     * failure keeps the paragraph it always had. */
    if (line && /[\r\n]/.test(line)) return [lines.join(' ') + '\n' + ask]
    lines.push(ask)
    return lines
  }
  if (k.unassessed) {
    /* A run the record does not name is not named. */
    if (!check) return [compose(D.nothing, { what, version }), D.ask_nothing]
    return [compose(D.unassessed, { what, version, check }), compose(D.ask_older, { check })]
  }
  if (k.kind === 'older') {
    /* A stale line whose run the record does not name is still older proof;
     * it is said without a name rather than with an invented one. */
    if (!check) return [compose(D.nothing, { what, version }), D.ask_nothing]
    return [compose(k.moved ? D.moved : D.older, { what, version, check }), compose(D.ask_older, { check })]
  }
  if (k.kind === 'fog') return [compose(D.fog, { what }), D.ask_fog]
  return [compose(D.nothing, { what, version }), D.ask_nothing]
}

/**
 * The message for the agent, for one Change, from its derived view-model.
 *
 * Returns `null` when the Change has no accepted line — there is nothing the
 * record can hold the agent to, and the session cards keep their own prompt.
 * Otherwise returns the kind, and either the draft text or, when every line
 * is proven on this version, no draft and the sentence that says why.
 *
 * @param {object|null} hq what `deriveChangeModel` returned
 * @returns {{kind: string, draft: string|null, covered: string|null,
 *   masking?: string, lines: Array<{n: number, kind: string}>}|null}
 */
export function tellDraftOf(hq) {
  const criteria = list(hq && hq.criteria)
  if (!criteria.length) return null
  const version = hq && hq.version ? str(hq.version.short) : null
  const aside = setAsideLines(hq)
  const needing = needingLines(hq)
  /* A line the coverage calls proven and the served answer says still needs evidence is asked about, whatever ran: the record
   * does not yet say whether the run that bears on it still applies (never assessed, or could not be). The run is named and is
   * not said to have passed, and the ask is the one that re-runs it. (A run the record says went stale is a line the
   * coverage does not call proven: `lineKindOf` tells it as such.) With no run to name it is a line nothing settles. */
  const kindOf = (c) => {
    const k = lineKindOf(c, version)
    if (k.kind !== 'proven' || !(c.n in needing)) return k
    const item = latest(list(c.items).filter((i) => i && i.bears === true && i.applies !== false))
    return item ? { kind: 'nothing', item, unassessed: true } : { kind: 'nothing', item: null }
  }
  const kinds = criteria.map((c) => ({ c, k: aside.has(c.n) ? { kind: 'set_aside', item: null } : kindOf(c) }))
  const open = kinds.filter(({ k }) => k.kind !== 'proven' && k.kind !== 'set_aside')
  const lines = kinds.map(({ c, k }) => ({ n: c.n, kind: k.kind }))
  if (!open.length) {
    /* APPLIES-VS-MOVED — "nothing to ask" is about the version as recorded;
     * while the served Change says the code moved since, it says so. */
    const covered = compose(aside.size ? T.covered_with_set_aside : T.covered, { version })
    return { kind: 'covered', draft: null, covered: hq.codeMoved ? covered + ' ' + T.covered_moved : covered, lines }
  }
  const kind = TELL_KINDS.find((x) => open.some(({ k }) => k.kind === x))
  const title = str(hq.title)
  const out = [D.preamble]
  if (title) out.push(version ? compose(D.about, { title, version }) : compose(D.about_no_version, { title }))
  for (const { c, k } of open) out.push(paragraph(c, k, version).join(' '))
  /* The command that records a run for this Change, as the run door prints
   * it. With no Change id the command would have a hole, so the slot is left
   * empty and `compose` refuses it. */
  const id = str(hq.id)
  out.push([compose(D.record, { command: id ? runCheckCommand(id) : null }), D.decides].join(' '))
  /* The masking line the record served, never one made up here — for the
   * screen, beside the copy control, not for the copied text. */
  const served = hq.coverage ? str(hq.coverage.masking) : null
  const masking = served ? compose(T.masking, { statement: served }) : T.masking_none
  return { kind, draft: out.join('\n\n'), covered: null, masking, lines }
}

/**
 * The session cards, with their copy action pointed at the Change's draft.
 *
 * When the Change holds accepted lines, the hand-off is the Change's: a
 * card's own prompt was composed from one session's verdict and does not move
 * when a later run on this Change fails. A covered Change offers no message,
 * so no card offers one either. With no accepted line the cards are returned
 * unchanged.
 *
 * @param {Array<object>} cards the finding cards
 * @param {ReturnType<typeof tellDraftOf>} tell
 */
export function cardsTold(cards, tell) {
  if (!tell) return list(cards)
  return list(cards).map((card) => ({ ...card, do: { ...(card.do || {}), prompt: tell.draft } }))
}

export default tellDraftOf
