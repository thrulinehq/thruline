/* Second approvals, as the screens draw them (C30 R-LEDGER).
 *
 *   secondsOf(servedChange)  → [{id, who, at, when}]   the seconds that still count
 *
 * PURE: no fetch, no clock. The seconds are the served Change's own `seconds[]`
 * (`GET /api/changes/<id>`, `crates/core/src/change_decision_second.rs`): a
 * second person's name on the version an approval stands for, already masked
 * by the server, each with a `counting` the server derived (true only for a
 * second no later withdrawal took back whose approval is the one that stands).
 * A second is a name and never a result: nothing here, and nothing that reads
 * this, moves a verdict, a count, readiness, which approval stands, or whether
 * approving is open.
 *
 * Writing one is not here. Its two addresses are declared beside the one
 * control that uses them (screens/Team-View/actions.js).
 */
import { whenOf } from './notes.js'

const list = (x) => (Array.isArray(x) ? x : [])
const str = (x) => (typeof x === 'string' ? x : '')

/** Ruling Q3, said before a second is made and again when a name is refused.
 * The server serves the same sentence on a refusal (`NAME_NOT_CHECKED` in
 * `change_decision_second.rs`); `seconds.test.js` holds the two together. */
export const NAME_NOT_CHECKED = 'ThruLine records the name you type; it does not check who is at the keyboard.'

/** The longest name the record keeps, in characters (`DECISION_ACTOR_MAX_CHARS`). */
export const SECOND_NAME_MAX_CHARS = 200

/**
 * The seconds that count on the standing approval of a served Change, oldest
 * first (the order the record appended them). A row the server did not mark as
 * counting is not drawn as a second; none is made up.
 * @param {object|null|undefined} change  the served `change` document
 */
export function secondsOf(change) {
  return list(change && change.seconds)
    .filter((s) => s && s.act === 'approved' && s.counting === true)
    .map((s) => ({
      id: str(s.id),
      who: str(s.actor).trim() || 'someone',
      at: str(s.recorded_at),
      when: whenOf(s.recorded_at),
      decisionId: str(s.decision_id),
    }))
}
