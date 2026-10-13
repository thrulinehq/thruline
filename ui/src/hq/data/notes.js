/* What people kept on a Change, as the screens draw it (C30 R-LEDGER).
 *
 *   notesOf(servedChange) → [{id, who, at, when, body, decisionId}]
 *
 * PURE: no fetch, no clock. The notes are the served Change's own `notes[]`
 * (`GET /api/changes/<id>`, `crates/core/src/change_note.rs`): words a person
 * typed, with who and when, already masked by the server. A note is words and
 * never a result: nothing here, and nothing that reads this, moves a verdict,
 * a count, readiness or which approval stands.
 *
 * Writing a note is not here. Its one address is declared beside its one
 * control (screens/Change-Brief/notes-actions.js); the Approve sheet's note
 * rides on the approval it is kept with (screens/Approve-Sheet/actions.js).
 */

const list = (x) => (Array.isArray(x) ? x : [])
const str = (x) => (typeof x === 'string' ? x : '')
const pad = (n) => String(n).padStart(2, '0')

/** The longest note the record keeps, in characters (`change_note::MAX_NOTE_CHARS`). */
export const NOTE_MAX_CHARS = 2000

/** A recorded UTC moment as "2026-10-07 14:02", in this machine's time; '' when it is not a time. */
export function whenOf(iso) {
  const t = typeof iso === 'string' && iso ? Date.parse(iso) : NaN
  if (Number.isNaN(t)) return ''
  const d = new Date(t)
  return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()) + ' ' + pad(d.getHours()) + ':' + pad(d.getMinutes())
}

/**
 * The notes a served Change carries, oldest first (the order the record
 * appended them). A note with no words is not drawn; none is made up.
 * @param {object|null|undefined} change  the served `change` document
 */
export function notesOf(change) {
  return list(change && change.notes)
    .filter((n) => n && str(n.body).trim())
    .map((n) => ({
      id: str(n.id),
      who: str(n.actor).trim() || 'someone',
      at: str(n.recorded_at),
      when: whenOf(n.recorded_at),
      body: str(n.body),
      /* set when it was kept with an approval or a withdrawal */
      decisionId: n.decision_id ? String(n.decision_id) : null,
    }))
}
